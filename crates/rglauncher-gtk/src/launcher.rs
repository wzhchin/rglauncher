use std::sync::Arc;

use crate::application::RGLApplication;
use crate::window::RGWindow;
use chin_tools::AResult;
use flume::{Receiver, Sender};
use gtk::glib::MainContext;
use rglcore::{
    config::ParsedConfig,
    dispatcher::{DispatchMsg, PluginDispatcher},
    PluginType,
};

#[derive(Clone)]
pub struct Launcher {
    app: RGLApplication,
    pub config: Arc<ParsedConfig>,

    dispatcher_tx: flume::Sender<DispatchMsg>,

    pub launcher_tx: Sender<LauncherMsg>,
    launcher_rx: Receiver<LauncherMsg>,
}

pub enum LauncherMsg {
    SelectSomething,
    Exit,
    NewWindow(Vec<PluginType>),
}

impl Launcher {
    pub fn spawn(
        application: RGLApplication,
        config: Arc<ParsedConfig>,
        launcher_tx: &Sender<LauncherMsg>,
        launcher_rx: &Receiver<LauncherMsg>,
    ) -> AResult<Self> {
        let dispathcer = PluginDispatcher::new(&config, vec![])?;
        let dispatcher_tx = dispathcer.tx.clone();

        MainContext::ref_thread_default().spawn_local(async move {
            if let Err(err) = dispathcer.spawn_blocking().await {
                tracing::error!("dispatcher failed: {err}");
            }
        });

        Ok(Launcher {
            app: application,
            config,
            dispatcher_tx,
            launcher_tx: launcher_tx.clone(),
            launcher_rx: launcher_rx.clone(),
        })
    }

    pub fn new_window(&self, plugin_types: Vec<PluginType>) {
        let launcher_rx = self.launcher_rx.clone();
        let launcher_tx = self.launcher_tx.clone();
        let dispatcher_tx = self.dispatcher_tx.clone();
        let app_args = self.config.clone();
        let app = self.app.clone();

        RGWindow::setup_one(&app, app_args.clone(), &dispatcher_tx, &launcher_tx, plugin_types.clone());

        MainContext::ref_thread_default().spawn_local(async move {
            let dispatcher_tx = dispatcher_tx.clone();
            let launcher_tx = launcher_tx.clone();
            let app_args = app_args.clone();
            let app = app.clone();
            loop {
                match launcher_rx.recv_async().await {
                    Ok(msg) => match msg {
                        LauncherMsg::Exit => {}
                        LauncherMsg::NewWindow(pt) => {
                            dispatcher_tx
                                .send(DispatchMsg::RefreshContent(pt.clone()))
                                .expect("unable to create new window");
                            RGWindow::setup_one(
                                &app,
                                app_args.clone(),
                                &dispatcher_tx,
                                &launcher_tx,
                                pt,
                            );
                        }
                        LauncherMsg::SelectSomething => {}
                    },
                    Err(_) => {}
                }
            }
        });
    }
}
