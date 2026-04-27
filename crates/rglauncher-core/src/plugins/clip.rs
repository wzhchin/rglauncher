use arboard::Clipboard;
use chin_tools::{aanyhow, AResult};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::path::Path;

use crate::dispatcher::CONNECTION;
use crate::impl_history;
use crate::plugins::history::{HistoryDb, HistoryItem};
use crate::plugins::{Plugin, PluginResult};
use crate::userinput::UserInput;
use crate::util::score_utils;

use super::history::HistoryCache;

pub const TYPE_ID: &str = "clipboard";

thread_local! {
    static CLIP_DB: RefCell<Option<Connection>> = const { RefCell::new(None) };
}

fn clip_db_path() -> AResult<std::path::PathBuf> {
    Ok(dirs::cache_dir()
        .ok_or_else(|| aanyhow!("cache dir not found"))?
        .join("rglauncher/clip.db"))
}

fn with_clip_db<F, R>(f: F) -> AResult<R>
where
    F: FnOnce(&Connection) -> AResult<R>,
{
    CLIP_DB.with_borrow_mut(|cell| {
        if cell.is_none() {
            let path = clip_db_path()?;
            if !path.exists() {
                return Err(aanyhow!("clip db not found: {}", path.display()));
            }
            let conn = Connection::open(&path)?;
            cell.replace(conn);
        }
        f(cell.as_ref().unwrap())
    })
}

#[derive(Clone)]
pub enum ClipReq {}

#[derive(Clone, Deserialize, Serialize)]
pub struct ClipResult {
    pub content: String,
    pub content_type: String,
    pub insert_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
    pub count: i64,
    pub id: String,
    pub is_image: bool,
    #[serde(default)]
    pub display_name: String,
}

impl ClipResult {
    fn compute_display_name(content: &str, is_image: bool) -> String {
        if is_image {
            Path::new(content)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| content.to_string())
        } else {
            content.to_string()
        }
    }
}

impl PluginResult for ClipResult {
    fn icon_name(&self) -> &str {
        if self.is_image {
            "image-x-generic"
        } else {
            "clipboard"
        }
    }

    fn name(&self) -> &str {
        &self.display_name
    }

    fn extra(&self) -> Option<&str> {
        None
    }

    fn on_enter(&self) {
        let mut clipboard = Clipboard::new().unwrap();
        if self.is_image {
            let img = image::ImageReader::open(&self.content)
                .unwrap()
                .decode()
                .unwrap();
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            let image_data = arboard::ImageData {
                width: w as usize,
                height: h as usize,
                bytes: rgba.into_raw().into(),
            };
            clipboard.set_image(image_data).unwrap();
        } else {
            clipboard.set_text(self.content.as_str()).unwrap();
        }
    }

    fn get_type_id(&self) -> &'static str {
        &TYPE_ID
    }

    fn get_id(&self) -> &str {
        self.id.as_str()
    }

    fn to_enum(self) -> super::PluginResultEnum {
        super::PluginResultEnum::Clip(self)
    }
}

pub struct ClipPlugin {
    history: HistoryCache<ClipResult>,
}

impl ClipPlugin {
    pub fn new() -> AResult<Self> {
        let histories: Vec<HistoryItem<ClipResult>> =
            CONNECTION.with_borrow(|e| HistoryDb::new(e.as_ref()).fetch_histories(TYPE_ID))?;

        Ok(ClipPlugin {
            history: HistoryCache::new(histories),
        })
    }
}

impl Plugin for ClipPlugin {
    type R = ClipResult;

    type T = ClipReq;

    fn handle_input(&self, user_input: &UserInput) -> AResult<Vec<(ClipResult, i32)>> {
        if user_input.input.is_empty() {
            return Err(aanyhow!("empty input"));
        }

        with_clip_db(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, content_type, content, timestamp, icount \
                 FROM history WHERE content LIKE ? ORDER BY timestamp DESC LIMIT 100",
            )?;

            let result = stmt
                .query_map([format!("%{}%", user_input.input.as_str())], |row| {
                    let id: String = row.get(0)?;
                    let content_type: String = row.get(1)?;
                    let content: String = row.get(2)?;
                    let is_image = content_type == "image";
                    let display_name = ClipResult::compute_display_name(&content, is_image);
                    Ok((
                        ClipResult {
                            content,
                            content_type,
                            insert_time: row.get(3)?,
                            update_time: row.get(3)?,
                            count: row.get(4)?,
                            id,
                            is_image,
                            display_name,
                        },
                        score_utils::middle(0),
                    ))
                })?
                .collect::<Result<Vec<(ClipResult, i32)>, rusqlite::Error>>()?;

            Ok(result)
        })
    }

    fn get_type_id(&self) -> &'static str {
        &TYPE_ID
    }

    impl_history!();
}
