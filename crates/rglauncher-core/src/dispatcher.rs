use crate::config::ParsedConfig;
use crate::plugins::app::AppPlugin;
#[cfg(feature = "calc")]
use crate::plugins::calc::CalcPlugin;
#[cfg(feature = "clip")]
use crate::plugins::clip::{ClipPlugin, ClipReq};
use crate::plugins::history::{HistoryDb, HistoryItem};
#[cfg(feature = "wmwin")]
use crate::plugins::win::WinPlugin;
use crate::plugins::{history, PRWrapper, Plugin, PluginResult};
use crate::userinput::UserInput;
use crate::PluginType;
use arc_swap::ArcSwapOption;
use chin_tools::{AResult, EResult};
use chrono::Utc;
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
    UserInput(Arc<UserInput>, Sender<crate::ResultMsg>, Vec<PluginType>),
    RefreshContent(Vec<PluginType>),
    SetHistory(PRWrapper),
    PluginMsg,
}

pub struct PluginDispatcher {
    pub tx: Sender<DispatchMsg>,
    rx: Receiver<DispatchMsg>,

    app: Option<Arc<AppPlugin>>,
    #[cfg(feature = "wmwin")]
    win: Option<Arc<WinPlugin>>,
    #[cfg(feature = "calc")]
    calc: Option<Arc<CalcPlugin>>,
    #[cfg(feature = "clip")]
    clip: Option<Arc<ClipPlugin>>,
}

macro_rules! handle_input {
    ($user_input_arc:expr, $plugin:expr, $executor:expr, $sender:expr ) => {{
        let user_input = $user_input_arc.clone();
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
                        .send_async(crate::ResultMsg::Result(
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
    }};
}

macro_rules! dispatch_input {
    ($plugin:expr, $user_input_arc:expr, $executor:expr, $sender:expr, $pty:expr, $types:expr) => {
        if let Some(plugin) = $plugin.as_ref() {
            if contains($types, $pty) {
                handle_input!($user_input_arc, plugin, $executor, $sender);
            }
        }
    };
}

macro_rules! handle_refresh {
    ($executor:tt, $plugin:expr, $pty:expr, $types:expr) => {
        if let Some(plugin) = $plugin.as_ref() {
            if contains($types, $pty) {
                let plugin = plugin.clone();
                if let Err(err) = $executor.spawn(async move { plugin.refresh_content() }) {
                    tracing::error!("unable to refresh {}", err)
                }
            }
        }
    };
}

fn contains(types: &[PluginType], target: PluginType) -> bool {
    types.is_empty() || types.contains(&target)
}

impl PluginDispatcher {
    pub fn new(config: &Arc<ParsedConfig>, plugin_types: Vec<PluginType>) -> AResult<PluginDispatcher> {
        let (tx, rx) = flume::unbounded();

        CONFIG.store(Some(config.clone()));
        db_init();

        let app = if contains(&plugin_types, PluginType::App) {
            Some(Arc::new(AppPlugin::new()?))
        } else {
            None
        };

        #[cfg(feature = "wmwin")]
        let win = if contains(&plugin_types, PluginType::Win) {
            Some(Arc::new(WinPlugin::new()?))
        } else {
            None
        };

        #[cfg(feature = "calc")]
        let calc = if contains(&plugin_types, PluginType::Calc) {
            Some(Arc::new(CalcPlugin::new()?))
        } else {
            None
        };

        #[cfg(feature = "clip")]
        let clip = if contains(&plugin_types, PluginType::Clip) {
            Some(Arc::new(ClipPlugin::new()?))
        } else {
            None
        };

        Ok(PluginDispatcher {
            app,
            #[cfg(feature = "wmwin")]
            win,
            #[cfg(feature = "calc")]
            calc,
            #[cfg(feature = "clip")]
            clip,
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
                DispatchMsg::UserInput(user_input_arc, sender, plugin_types) => {
                    dispatch_input!(self.app, user_input_arc, executor, sender, PluginType::App, &plugin_types);
                    #[cfg(feature = "wmwin")]
                    dispatch_input!(self.win, user_input_arc, executor, sender, PluginType::Win, &plugin_types);
                    #[cfg(feature = "calc")]
                    dispatch_input!(self.calc, user_input_arc, executor, sender, PluginType::Calc, &plugin_types);
                    #[cfg(feature = "clip")]
                    dispatch_input!(self.clip, user_input_arc, executor, sender, PluginType::Clip, &plugin_types);
                }
                DispatchMsg::RefreshContent(plugin_types) => {
                    handle_refresh!(executor, self.app, PluginType::App, &plugin_types);
                    #[cfg(feature = "wmwin")]
                    handle_refresh!(executor, self.win, PluginType::Win, &plugin_types);
                    #[cfg(feature = "calc")]
                    handle_refresh!(executor, self.calc, PluginType::Calc, &plugin_types);

                    #[cfg(feature = "clip")]
                    handle_refresh!(executor, self.clip, PluginType::Clip, &plugin_types);
                }
                DispatchMsg::SetHistory(prwrapper) => {
                    let history_id = HistoryDb::get_id(&prwrapper.body);

                    match prwrapper.body {
                        crate::plugins::PluginResultEnum::Calc(body) => {
                            if let Some(calc) = self.calc.as_ref() {
                                let _ = calc.add_history(HistoryItem {
                                    id: history_id,
                                    plugin_type: body.get_type_id().into(),
                                    body: body,
                                    weight: 1.,
                                    update_time: Utc::now().naive_utc(),
                                });
                            }
                        }
                        crate::plugins::PluginResultEnum::Win(body) => {
                            #[cfg(feature = "wmwin")]
                            if let Some(win) = self.win.as_ref() {
                                let _ = win.add_history(HistoryItem {
                                    id: history_id,
                                    plugin_type: body.get_type_id().into(),
                                    body: body,
                                    weight: 1.,
                                    update_time: Utc::now().naive_utc(),
                                });
                            }
                        }
                        crate::plugins::PluginResultEnum::App(body) => {
                            if let Some(app) = self.app.as_ref() {
                                let _ = app.add_history(HistoryItem {
                                    id: history_id,
                                    plugin_type: body.get_type_id().into(),
                                    body: body,
                                    weight: 1.,
                                    update_time: Utc::now().naive_utc(),
                                });
                            }
                        }
                        #[cfg(feature = "clip")]
                        crate::plugins::PluginResultEnum::Clip(body) => {
                            if let Some(clip) = self.clip.as_ref() {
                                let _ = clip.add_history(HistoryItem {
                                    id: history_id,
                                    plugin_type: body.get_type_id().into(),
                                    body: body,
                                    weight: 1.,
                                    update_time: Utc::now().naive_utc(),
                                });
                            }
                        }
                    }
                }
                DispatchMsg::PluginMsg => {}
            }
        }
    }
}
