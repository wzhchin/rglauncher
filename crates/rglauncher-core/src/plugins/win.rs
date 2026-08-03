use std::borrow::Cow;
use std::collections::HashMap;
use std::process::Command;

use arc_swap::ArcSwap;
use chin_tools::{aanyhow, AResult, AnyhowContext, SharedStr};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::dispatcher::CONNECTION;
use crate::impl_history;
use crate::plugins::history::{HistoryDb, HistoryItem};
use crate::plugins::{Plugin, PluginResult};
use crate::userinput::UserInput;

use crate::util::fuzzy::{fuzzy_atom, fuzzy_score_best, new_matcher};
use crate::util::score_utils;

use super::history::HistoryCache;

pub const TYPE_ID: &str = "wmwindows";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum WMEnum {
    Niri,
    Hypr,
}
pub trait WMBehavier {
    fn focus_window(&self, id: &str) -> AResult<()>;
    fn list_windows(&self) -> AResult<Vec<WinResult>>;
}

impl WMEnum {
    pub fn new() -> AResult<WMEnum> {
        if let Ok(_) = std::env::var("HYPRLAND_INSTANCE_SIGNATURE") {
            Ok(WMEnum::Hypr)
        } else if let Ok(_) = std::env::var("NIRI_SOCKET") {
            Ok(WMEnum::Niri)
        } else {
            Err(aanyhow!("unknown not implemented"))
        }
    }
}

impl WMBehavier for WMEnum {
    fn focus_window(&self, id: &str) -> AResult<()> {
        match self {
            WMEnum::Niri => {
                let output = Command::new("niri")
                    .arg("msg")
                    .arg("action")
                    .arg("focus-window")
                    .arg("--id")
                    .arg(id)
                    .output()?;
                if !output.status.success() {
                    error!("unable to success: {:?}", output);
                }
            }
            WMEnum::Hypr => {
                let output = Command::new("hyprctl")
                    .arg("dispatch")
                    .arg("focuswindow")
                    .arg("address:".to_owned() + id)
                    .output()?;
                if !output.status.success() {
                    error!("unable to success: {:?}", output);
                }
            }
        }
        Ok(())
    }

    fn list_windows(&self) -> AResult<Vec<WinResult>> {
        match self {
            WMEnum::Niri => {
                let ws_output = Command::new("niri")
                    .arg("msg")
                    .arg("-j")
                    .arg("workspaces")
                    .output()?;

                let out = String::from_utf8(ws_output.stdout)?;

                let json = serde_json::from_str::<serde_json::Value>(out.as_str())
                    .unwrap_or_else(|_| serde_json::Value::Null);
                let ws_map: HashMap<i64, String> = json
                    .as_array()
                    .context("niri workspace output is not a valid json")?
                    .iter()
                    .filter_map(|e| {
                        let name = if let Some(Some(name)) = e.get("name").map(|e| e.as_str()) {
                            name.to_string()
                        } else {
                            e.get("idx")?.as_i64()?.to_string()
                        };
                        Some((e.get("id")?.as_i64()?, name))
                    })
                    .collect();

                let windows_output = Command::new("niri")
                    .arg("msg")
                    .arg("-j")
                    .arg("windows")
                    .output()?;

                let out = String::from_utf8(windows_output.stdout)?;

                let json = serde_json::from_str::<serde_json::Value>(out.as_str())
                    .unwrap_or_else(|_| serde_json::Value::Null);

                let vec: Vec<WinResult> = json
                    .as_array()
                    .context("niri windows output is not a valid json")?
                    .iter()
                    .filter_map(|e| {
                        let a = Some(WinResult {
                            class: e.get("app_id")?.as_str()?.into(),
                            title: e.get("title")?.as_str()?.into(),
                            address: e.get("id")?.as_i64()?.to_string().into(),
                            pid: e.get("pid")?.as_i64()?,
                            workspace: ws_map
                                .get(&e.get("workspace_id")?.as_i64()?)?
                                .as_str()
                                .into(),
                            wm_type: WMEnum::Niri,
                        });
                        a
                    })
                    .collect();

                Ok(vec)
            }
            WMEnum::Hypr => {
                let output = Command::new("hyprctl").arg("clients").arg("-j").output()?;
                let out = String::from_utf8(output.stdout)?;
                let json = serde_json::from_str::<serde_json::Value>(out.as_str())?;

                let array = json
                    .as_array()
                    .context("hyprctl output is not a valid json")?;
                let vec: Vec<WinResult> = array
                    .iter()
                    .filter_map(|e| {
                        let class = e.get("class")?.as_str()?;
                        let monitor = e.get("monitor")?.as_i64()?;
                        if monitor == -1 {
                            return None;
                        }

                        Some(WinResult {
                            class: class.into(),
                            title: e.get("title")?.as_str()?.into(),
                            address: e.get("address")?.as_str()?.into(),
                            pid: e.get("pid")?.as_i64()?,
                            workspace: e.get("workspace")?.get("name")?.as_str()?.into(),
                            wm_type: WMEnum::Hypr,
                        })
                    })
                    .collect();
                Ok(vec)
            }
        }
    }
}

#[derive(Clone)]
pub enum WindowMsg {}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WinResult {
    pub class: SharedStr,
    pub title: SharedStr,
    pub address: SharedStr,
    pub pid: i64,
    pub workspace: SharedStr,
    pub wm_type: WMEnum,
}

impl PluginResult for WinResult {
    fn icon_name(&self) -> &str {
        self.class.as_str()
    }

    fn name(&self) -> &str {
        self.title.as_str()
    }

    fn extra(&self) -> Option<&str> {
        Some(self.workspace.as_str())
    }

    fn on_enter(&self) {
        if let Err(e) = self.wm_type.focus_window(&self.address) {
            error!("unable to focus on {}", e);
        }
    }

    fn get_type_id(&self) -> &'static str {
        &TYPE_ID
    }

    fn get_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.address.as_str())
    }

    fn to_enum(self) -> super::PluginResultEnum {
        super::PluginResultEnum::Win(self)
    }
}

pub struct WinPlugin {
    windows: arc_swap::ArcSwap<Vec<WinResult>>,
    history: HistoryCache<WinResult>,
    wm_type: WMEnum,
}

impl WinPlugin {
    pub fn new() -> AResult<Self> {
        info!("Creating Windows Plugin");
        let wm_type = WMEnum::new()?;
        let wins: std::sync::Arc<Vec<WinResult>> = wm_type.list_windows()?.into();

        let histories: Vec<HistoryItem<WinResult>> =
            CONNECTION.with_borrow(|e| HistoryDb::new(e.as_ref()).fetch_histories(TYPE_ID))?;
        let history = HistoryCache::new(histories);

        let _ = CONNECTION.with_borrow(|e| {
            let ho = HistoryDb::new(e.as_ref());
            history.remove_unvalid( 
                |_, v| {
                    wins.iter()
                        .find(|w| w.get_id() == v.body.address.as_str())
                        .is_some()
                },
                ho,
            )
        });

        Ok(WinPlugin {
            windows: ArcSwap::new(wins),
            wm_type,
            history,
        })
    }
}

impl Plugin for WinPlugin {
    type R = WinResult;

    type T = WindowMsg;

    fn refresh_content(&self) {
        if let Ok(windows) = self.wm_type.list_windows() {
            let _ = CONNECTION.with_borrow(|conn| {
                let ho = HistoryDb::new(conn.as_ref());

                let _ = self.history.remove_unvalid(
                    |_, v| windows.iter().find(|w| w.get_id() == v.body.get_id()).is_some(),
                    ho,
                );
            });
            self.windows.store(windows.into());
        }
    }

    fn handle_input(&self, user_input: &UserInput) -> AResult<Vec<(WinResult, i32)>> {
        let mut result = vec![];

        // When the input is empty (e.g. a freshly opened window), the cached
        // window list may be stale because the daemon-level RefreshContent
        // triggered on NewWindow races ahead asynchronously and may not have
        // run yet. Refresh synchronously here so the initial view reflects the
        // current set of windows instead of a leftover snapshot.
        if user_input.input.is_empty() {
            self.refresh_content();
        }

        let q = user_input.input.as_str();
        if q.is_empty() {
            for window in self.windows.load().iter() {
                result.push((window.clone(), score_utils::high(100)));
            }
            return Ok(result);
        }

        let pattern = fuzzy_atom(q);
        let mut matcher = new_matcher();
        let mut buf = Vec::new();
        for window in self.windows.load().iter() {
            if let Some(score) = fuzzy_score_best(
                &mut matcher,
                &pattern,
                &[
                    window.class.as_str(),
                    window.title.as_str(),
                    window.workspace.as_str(),
                ],
                &mut buf,
            ) {
                result.push((
                    window.clone(),
                    score_utils::high((score as i64) * 8),
                ));
            }
        }

        Ok(result)
    }

    fn get_type_id(&self) -> &'static str {
        &TYPE_ID
    }

    impl_history!();
}
