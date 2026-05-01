use crate::launcher::LauncherMsg;
use crate::pluginpreview::PreviewMsg;
use crate::sidebar::SidebarMsg;
use flume::{Receiver, Sender};
use rglcore::dispatcher::DispatchMsg;
use rglcore::plugins::{PRWrapper, PluginResult};
use rglcore::userinput::Signal;
use rglcore::{PluginType, ResultMsg};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{debug, error};

pub struct ResultHolder {
    pub result_tx: Sender<ResultMsg>,
    result_rx: Receiver<ResultMsg>,

    launcher_tx: Sender<LauncherMsg>,
    dispatch_tx: flume::Sender<DispatchMsg>,

    sidebar_tx: Sender<SidebarMsg>,
    preview_tx: Sender<PreviewMsg>,

    current_index: Option<u32>,
    signal_and_results: Option<(Signal, Vec<PRWrapper>)>,
    last: Instant,
    plugin_types: Vec<PluginType>,
}

impl ResultHolder {
    fn new(
        launcher_tx: &Sender<LauncherMsg>,
        dispatch_tx: &flume::Sender<DispatchMsg>,
        sidebar_tx: &Sender<SidebarMsg>,
        preview_tx: &Sender<PreviewMsg>,
        plugin_types: Vec<PluginType>,
    ) -> Self {
        let (result_tx, result_rx) = flume::unbounded();

        Self {
            current_index: None,

            result_tx,
            result_rx,
            launcher_tx: launcher_tx.clone(),
            dispatch_tx: dispatch_tx.clone(),
            sidebar_tx: sidebar_tx.clone(),
            preview_tx: preview_tx.clone(),
            last: Instant::now(),
            signal_and_results: None,
            plugin_types,
        }
    }

    fn send_to_sidebar(&mut self) {
        let holder = if let Some((_, holder)) = self.signal_and_results.as_mut() {
            let holder = holder.clone();
            self.last = Instant::now();
            holder
        } else {
            vec![]
        };

        if holder.is_empty() {
            self.preview_tx
                .send(PreviewMsg::Clear)
                .expect("unable to clear preview");
        }

        self.sidebar_tx
            .send(SidebarMsg::Result(holder))
            .expect("unable to send result to sidebar");
    }

    fn accept_messages(&mut self) {
        let interval = Duration::from_millis(30);
        let mut received_something = false;
        let mut next_sleep_time = 100000;
        loop {
            match self
                .result_rx
                .recv_timeout(Duration::from_millis(next_sleep_time))
            {
                Ok(msg) => match msg {
                    ResultMsg::Result(input, extends) => {
                        if input.valid() {
                            if let Some((_, results)) = self.signal_and_results.as_mut() {
                                results.extend(extends);
                                results.sort_by(|e1, e2| e2.score.cmp(&e1.score));
                                received_something = true;
                                next_sleep_time = 50;
                            }
                        }
                    }
                    ResultMsg::UserInput(input) => {
                        self.signal_and_results
                            .replace((input.signal.clone(), vec![]));
                        self.current_index.take();
                        debug!("Send message to dispatcher: {:?}", input.input);
                        match self
                            .dispatch_tx
                            .send(DispatchMsg::UserInput(input.into(), self.result_tx.clone(), self.plugin_types.clone()))
                        {
                            Ok(_) => {
                                self.last = Instant::now();
                            }
                            Err(err) => {
                                error!("unable send to dispatcher {}", err);
                                break;
                            }
                        }
                    }
                    ResultMsg::ChangeSelect(item) => {
                        self.current_index.replace(item.clone());
                        match self.signal_and_results.as_ref().map(|(_, r)| r.get(item as usize)) {
                            Some(Some(pr)) => {
                                self.preview_tx
                                    .send(PreviewMsg::PluginResult(pr.clone()))
                                    .expect("unable to send preview msg");
                            }
                            _ => {}
                        }
                    }
                    ResultMsg::SelectSomething => match self.current_index.clone() {
                        None => {}
                        Some(id) => {
                            if let Some(Some(pr)) =
                                self.signal_and_results.as_ref().map(|(_, r)| r.get(id as usize))
                            {
                                pr.on_enter();
                                self.launcher_tx
                                    .send(LauncherMsg::SelectSomething)
                                    .expect("unable to send select");
                                self.dispatch_tx
                                    .send(DispatchMsg::SetHistory(pr.clone()))
                                    .expect("unable to set history");
                            }
                        }
                    },
                },
                Err(_ex) => {
                    // error!("unable to receive message: {:?}", ex);
                }
            }
            if received_something
                && Instant::now()
                    .duration_since(self.last)
                    .cmp(&interval)
                    .is_ge()
            {
                self.send_to_sidebar();
                received_something = false;
                next_sleep_time = 100000;
            }
        }
    }

    pub fn start(
        launcher_tx: &Sender<LauncherMsg>,
        dispatch_tx: &flume::Sender<DispatchMsg>,
        sidebar_tx: &Sender<SidebarMsg>,
        preview_tx: &Sender<PreviewMsg>,
        plugin_types: Vec<PluginType>,
    ) -> Sender<ResultMsg> {
        let mut result_handler = Self::new(launcher_tx, dispatch_tx, sidebar_tx, preview_tx, plugin_types);

        let result_tx = result_handler.result_tx.clone();

        thread::spawn(move || {
            result_handler.accept_messages();
        });

        result_tx
    }
}
