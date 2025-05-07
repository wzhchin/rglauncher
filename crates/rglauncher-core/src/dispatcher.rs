use crate::config::{Config, ParsedConfig};
use crate::plugins::app::AppPlugin;
#[cfg(feature = "calc")]
use crate::plugins::calc::CalcPlugin;
#[cfg(feature = "clip")]
use crate::plugins::clip::{ClipPlugin, ClipReq};
use crate::plugins::history::{HistoryDb, HistoryItem};
#[cfg(feature = "mdict")]
use crate::plugins::mdict::{DictMsg, DictPlugin};
#[cfg(feature = "wmwin")]
use crate::plugins::win::WinPlugin;
use crate::plugins::{history, PRWrapper, Plugin, PluginResult};
use crate::userinput::UserInput;
use crate::ResultMsg;
use arc_swap::ArcSwapOption;
use chin_tools::{AResult, EResult};
use chrono::{NaiveDateTime, Utc};
use flume::{Receiver, Sender};
use futures::executor::ThreadPool;
use futures::task::SpawnExt;
use lazy_static::lazy_static;
use rusqlite::Connection;
use std::cell::RefCell;
use std::sync::Arc;

lazy_static! {
    static ref CONFIG: ArcSwapOption<ParsedConfig> = ArcSwapOption::empty();
}

thread_local! {
    pub static CONNECTION: RefCell<Option<Connection>> = RefCell::new(None);
}

pub fn db_init() {
    CONNECTION.with_borrow_mut(|con| {
        let conn = Connection::open(&CONFIG.load().as_ref().unwrap().db.db_path).unwrap();

        let history = history::HistoryDb::new(Some(&conn));
        history.try_create_table().unwrap();

        con.replace(conn);
    })
}

#[derive(Clone)]
pub enum DispatchMsg {
    UserInput(Arc<UserInput>, Sender<ResultMsg>),
    RefreshContent,
    SetHistory(PRWrapper),
    PluginMsg,
}

pub struct PluginDispatcher {
    pub tx: Sender<DispatchMsg>,
    rx: Receiver<DispatchMsg>,

    app: Arc<AppPlugin>,
    win: Arc<WinPlugin>,
    calc: Arc<CalcPlugin>,
    #[cfg(feature = "clip")]
    clip: Arc<ClipPlugin>,
    #[cfg(feature = "fmdict")]
    dict: Arc<DictPlugin>,
}

macro_rules! handle_input {
    ($user_input_arc:expr, $plugin:expr, $executor:expr, $sender:expr ) => {{
        let user_input = $user_input_arc.clone();
        if user_input.input.is_empty() {
            $sender
                .send_async(ResultMsg::Result(
                    user_input.signal.clone(),
                    $plugin
                        .get_history()
                        .into_iter()
                        .map(|item| (item.body, item.weight as i32).into())
                        .collect(),
                ))
                .await
                .unwrap();
        } else {
            let sender = $sender.clone();
            let plugin = $plugin.clone();
            if let Err(err) = $executor.spawn(async move {
                match plugin.handle_input(&user_input) {
                    Ok(result) => {
                        if user_input.cancelled() {
                            tracing::info!("cancelled");
                            return;
                        }
                        sender
                            .send_async(ResultMsg::Result(
                                user_input.signal.clone(),
                                result.into_iter().map(|e| e.into()).collect(),
                            ))
                            .await
                            .unwrap();
                    }
                    Err(err) => {
                        tracing::error!(
                            "unable to handle input: {} -- {}",
                            plugin.get_type_id(),
                            err
                        );
                    }
                }
            }) {
                tracing::error!("unable to spawn: {}", err);
            }
        }
    }};
}

macro_rules! handle_refresh {
    ($executor:tt, $plugin:expr) => {
        let plugin = $plugin.clone();
        if let Err(err) = $executor.spawn(async move { plugin.refresh_content() }) {
            tracing::error!("unable to refresh {}", err)
        }
    };
}

impl PluginDispatcher {
    pub fn new(config: &Arc<ParsedConfig>) -> AResult<PluginDispatcher> {
        let (tx, rx) = flume::unbounded();

        CONFIG.store(Some(config.clone()));
        db_init();

        let app = AppPlugin::new()?.into();
        let win = WinPlugin::new()?.into();
        #[cfg(feature = "clip")]
        let clip = ClipPlugin::new()?.into();
        #[cfg(feature = "fmdict")]
        let dict = DictPlugin::new(config.dict.as_ref())?.into();
        let calc = CalcPlugin::new()?.into();

        Ok(PluginDispatcher {
            app,
            win,
            #[cfg(feature = "clip")]
            clip,
            calc,
            #[cfg(feature = "fmdict")]
            dict,
            tx,
            rx,
        })
    }

    pub async fn spawn_blocking(&self) -> EResult {
        let executor = ThreadPool::builder()
            .after_start(move |_| {
                db_init();
            })
            .name_prefix("rgl")
            .create()?;

        loop {
            match self.rx.recv_async().await? {
                DispatchMsg::UserInput(user_input, sender) => {
                    let user_input_arc: Arc<UserInput> = user_input;

                    handle_input!(user_input_arc, self.app, executor, sender);
                    handle_input!(user_input_arc, self.win, executor, sender);
                    handle_input!(user_input_arc, self.calc, executor, sender);
                    #[cfg(feature = "clip")]
                    handle_input!(user_input_arc, self.clip, executor, sender);
                    #[cfg(feature = "fmdict")]
                    handle_input!(user_input_arc, self.dict, executor, sender);
                }
                DispatchMsg::RefreshContent => {
                    handle_refresh!(executor, self.app);
                    handle_refresh!(executor, self.win);
                    handle_refresh!(executor, self.calc);

                    #[cfg(feature = "clip")]
                    {
                        handle_refresh!(self.clip);
                    }

                    #[cfg(feature = "fmdict")]
                    {
                        handle_refresh!(self.dict);
                    }
                }
                DispatchMsg::SetHistory(prwrapper) => {
                    let history_id = HistoryDb::get_id(&prwrapper.body);

                    match prwrapper.body {
                        crate::plugins::PluginResultEnum::Calc(body) => {
                            let _ = self.calc.add_history(HistoryItem {
                                id: history_id,
                                plugin_type: body.get_type_id().into(),
                                body: body,
                                weight: 1.,
                                update_time: Utc::now().naive_utc(),
                            });
                        }
                        crate::plugins::PluginResultEnum::Win(body) => {
                            let _ = self.win.add_history(HistoryItem {
                                id: history_id,
                                plugin_type: body.get_type_id().into(),
                                body: body,
                                weight: 1.,
                                update_time: Utc::now().naive_utc(),
                            });
                        }
                        crate::plugins::PluginResultEnum::App(body) => {
                            let _ = self.app.add_history(HistoryItem {
                                id: history_id,
                                plugin_type: body.get_type_id().into(),
                                body: body,
                                weight: 1.,
                                update_time: Utc::now().naive_utc(),
                            });
                        }
                    }
                }
                DispatchMsg::PluginMsg => {}
            }
        }
    }
}
