use arboard::Clipboard;
use chin_tools::AResult;
use chrono::{DateTime, Utc};

use crate::plugins::{Plugin, PluginResult};
use crate::userinput::UserInput;
use crate::util::score_utils;

use super::CONNECTION;

pub const TYPE_ID: &str = "clipboard";

#[derive(Clone)]
pub enum ClipReq {}

#[derive(Clone)]
pub struct ClipResult {
    pub content: String,
    pub mime: String,
    pub insert_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
    pub count: i64,
    pub id: String,
}

impl PluginResult for ClipResult {
    fn icon_name(&self) -> &str {
        "clipboard"
    }

    fn name(&self) -> &str {
        self.content.as_str()
    }

    fn extra(&self) -> Option<&str> {
        None
    }

    fn on_enter(&self) {
        let mut clipboard = Clipboard::new().unwrap();
        clipboard.set_text(self.content.as_str()).unwrap();
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

pub struct ClipPlugin;

impl ClipPlugin {
    pub fn new() -> AResult<Self> {
        Ok(ClipPlugin {})
    }
}

impl Plugin for ClipPlugin {
    type R = ClipResult;

    type T = ClipReq;

    fn handle_input(&self, user_input: &UserInput) -> AResult<Vec<(ClipResult, i32)>> {
        if user_input.input.is_empty() {
            return eanyhow!("empty input");
        }

        let vec = CONNECTION.with_borrow(|conn| {
            if let Some(conn) = conn {
                let mut stmt = conn.prepare(
                    "SELECT content0, mimes, insert_time, update_time, count \
            from clipboard where content0 like ? order by UPDATE_TIME desc limit 100",
                )?;

                let result = stmt
                    .query_map([format!("%{}%", user_input.input.as_str())], |row| {
                        Ok((
                            ClipResult {
                                content: row.get(0)?,
                                mime: row.get(1)?,
                                insert_time: row.get(2)?,
                                update_time: row.get(3)?,
                                count: row.get(4)?,
                                id: format!("{:x}", md5::compute(&(row.get::<usize, String>(0)?))),
                            },
                            score_utils::middle(0),
                        ))
                    })?
                    .collect::<Result<Vec<(ClipResult, i32)>, rusqlite::Error>>()?;

                Ok(result)
            } else {
                Err(Error::msg("unable to find connection"))
            }
        });

        vec
    }

    fn get_type_id(&self) -> &'static str {
        &TYPE_ID
    }
}
