use crate::dispatcher::CONNECTION;
use crate::impl_history;
use crate::plugins::history::{HistoryDb, HistoryItem};
use crate::plugins::{Plugin, PluginResult};
use crate::userinput::UserInput;
use arc_swap::ArcSwap;
use chin_tools::utils::cmd_util::parse_cmd_string;
use chin_tools::{AResult, SharedStr};
use freedesktop_desktop_entry::DesktopEntry;
use lazy_static::lazy_static;
use nucleo_matcher::Matcher;
use parking_lot::Mutex;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::io;
use std::option::Option::None;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tracing::{error, info};

use crate::util::fuzzy::{fuzzy_atom, fuzzy_score_best, new_matcher};
use crate::util::score_utils;

use super::history::HistoryCache;

#[derive(Clone)]
pub enum AppReq {}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppResult {
    pub icon_name: SharedStr,
    pub app_name: SharedStr,
    pub app_desc: SharedStr,
    pub exec: SharedStr,
    pub id: SharedStr,
    pub desktop_path: SharedStr,
    pub terminal: bool,
}

lazy_static! {
    static ref PLACE_HOLDER_REPLACER: Regex = Regex::new(r"%\w").unwrap();
}

pub const TYPE_ID: &str = "app_result";

impl PluginResult for AppResult {
    fn icon_name(&self) -> &str {
        self.icon_name.as_str()
    }

    fn name(&self) -> &str {
        self.app_name.as_ref()
    }

    fn extra(&self) -> Option<&str> {
        Some(self.app_desc.as_ref())
    }

    fn on_enter(&self) {
        if self.terminal {
            let true_command = PLACE_HOLDER_REPLACER
                .replace_all(self.exec.as_str(), "")
                .trim()
                .to_string();
            let term = crate::dispatcher::terminal_cmd();
            info!("exec terminal app via {term}: {true_command}");
            if let Err(e) = Command::new(&term).arg("-e").arg(&true_command).spawn() {
                error!("unable to spawn terminal app ({term} -e {true_command}): {e}");
            }
        } else {
            let true_command: Vec<String> = parse_cmd_string(self.exec.as_str())
                .into_iter()
                .filter(|e| !e.starts_with("%"))
                .collect();
            info!("exec command: {:?}", true_command);
            match run_command(true_command.iter().map(|e| e.as_str()).collect())
                .map_err(|e| error!("unable to start {}", e))
            {
                Ok(_) => {}
                Err(_) => {
                    error!("Unable to exec command: {:?}", true_command);
                }
            }
        }
    }

    fn get_type_id(&self) -> &'static str {
        TYPE_ID
    }

    fn get_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.desktop_path)
    }

    fn to_enum(self) -> super::PluginResultEnum {
        super::PluginResultEnum::App(self)
    }
}

pub struct AppPlugin {
    applications: ArcSwap<Vec<AppResult>>,
    history: HistoryCache<AppResult>,
    /// Reused across queries (nucleo Matcher is stateful/alloc-heavy to rebuild).
    matcher: Mutex<Matcher>,
}

impl AppPlugin {
    pub fn new() -> AResult<Self> {
        info!("Creating App Plugin");

        let applications = ArcSwap::new(Self::read_applications().into());

        let histories: Vec<HistoryItem<AppResult>> =
            CONNECTION.with_borrow(|e| HistoryDb::new(e.as_ref()).fetch_histories(TYPE_ID))?;
        let histories = histories
            .into_iter()
            .filter(|e| PathBuf::from(e.body.desktop_path.as_str()).exists())
            .collect();

        Ok(AppPlugin {
            applications,
            matcher: Mutex::new(new_matcher()),
            history: HistoryCache::new(histories),
        })
    }

    fn read_applications() -> Vec<AppResult> {
        let locale = sys_locale::get_locale().map_or(vec![], |e| vec![e]);
        freedesktop_desktop_entry::Iter::new(freedesktop_desktop_entry::default_paths())
            .into_iter()
            .filter_map(|path| {
                if let Ok(Ok(entry)) = std::fs::read_to_string(&path)
                    .as_ref()
                    .map(|bytes| DesktopEntry::from_str(&path, bytes, Some(&locale)))
                {
                    if entry.no_display() {
                        return None;
                    }

                    return Some(AppResult {
                        id: entry.id().into(),
                        icon_name: entry.icon().unwrap_or_default().into(),
                        app_name: entry.name(&locale).unwrap_or_default().as_ref().into(),
                        app_desc: entry.comment(&locale).unwrap_or_default().as_ref().into(),
                        exec: entry.exec()?.into(),
                        desktop_path: path.to_str()?.to_owned().into(),
                        terminal: entry.terminal(),
                    });
                }

                None
            })
            .collect()
    }
}

impl Plugin for AppPlugin {
    type R = AppResult;

    type T = AppReq;

    fn refresh_content(&self) {
        let applications: std::sync::Arc<Vec<AppResult>> = Self::read_applications().into();
        let _ = CONNECTION.with_borrow(|conn| {
            let ho = HistoryDb::new(conn.as_ref());
            self.history.remove_unvalid(
                |k, _| {
                    applications
                        .iter()
                        .find(|w| w.get_id() == k.as_str())
                        .is_none()
                },
                ho,
            )
        });
        self.applications.store(applications);
    }

    fn handle_input(&self, user_input: &UserInput) -> AResult<Vec<(AppResult, i32)>> {
        let histories = self.history.histories.load();
        let apps = self.applications.load();
        let q = user_input.input.as_str();

        // Empty query: skip fuzzy work; rank by history only.
        if q.is_empty() {
            let mut result: Vec<(AppResult, i32)> = apps
                .iter()
                .map(|app| {
                    let history_id: SharedStr = format!("{}{}", TYPE_ID, app.get_id()).into();
                    let hist_boost = histories
                        .get(&history_id)
                        .map(|h| (h.weight * 20.0) as i64)
                        .unwrap_or(0);
                    (app.clone(), score_utils::high(hist_boost))
                })
                .collect();
            result.sort_unstable_by(|a, b| b.1.cmp(&a.1));
            // Cap early — UI also caps; keeps clone cost down on cold open.
            if result.len() > 800 {
                result.truncate(800);
            }
            return Ok(result);
        }

        let pattern = fuzzy_atom(q);
        let mut matcher = self.matcher.lock();
        let mut buf = Vec::new();
        let result = apps
            .iter()
            .filter_map(|app| {
                let score = fuzzy_score_best(
                    &mut matcher,
                    &pattern,
                    &[app.app_name.as_str(), app.id.as_str()],
                    &mut buf,
                )?;
                let history_id: SharedStr = format!("{}{}", TYPE_ID, app.get_id()).into();
                let hist_boost = histories
                    .get(&history_id)
                    .map(|h| (h.weight * 20.0) as i64)
                    .unwrap_or(0);
                Some((
                    app.clone(),
                    score_utils::high(score as i64 + hist_boost),
                ))
            })
            .collect();

        Ok(result)
    }

    fn get_type_id(&self) -> &'static str {
        &TYPE_ID
    }

    impl_history!();
}

//https://github.com/alacritty/alacritty/blob/f7811548ae9cabb1122f43b42fec4d660318bc96/alacritty/src/daemon.rs#L28
fn run_command(command_and_args: Vec<&str>) -> io::Result<()> {
    let mut command = Command::new(&command_and_args[0]);
    command
        .args(&command_and_args[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    unsafe {
        command
            .pre_exec(|| {
                match libc::fork() {
                    -1 => return Err(io::Error::last_os_error()),
                    0 => (),
                    _ => libc::_exit(0),
                }

                if libc::setsid() == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            })
            .spawn()?
            .wait()
            .map(|_| ())
    }
}
