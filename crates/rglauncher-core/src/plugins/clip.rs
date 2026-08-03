//! Clipboard history plugin.
//!
//! Performance model (inspired by Loungy):
//! - Load rows into an in-memory cache on refresh / mtime change.
//! - Keystroke path only filters the cache (nucleo on title), never `LIKE %q%` on SQLite.
//! - Large text bodies are **not** copied into list results; paste/preview resolve by id.

use arboard::Clipboard;
use arc_swap::ArcSwap;
use chin_tools::{aanyhow, AResult};
use chrono::{DateTime, Utc};
use lazy_static::lazy_static;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Instant, SystemTime};

lazy_static! {
    /// Hot map id → full body for paste/preview when list results omit large text.
    static ref CONTENT_BY_ID: ArcSwap<HashMap<String, Arc<str>>> =
        ArcSwap::from_pointee(HashMap::new());
}

use crate::dispatcher::CONNECTION;
use crate::impl_history;
use crate::plugins::history::{HistoryDb, HistoryItem};
use crate::plugins::{Plugin, PluginResult};
use crate::userinput::UserInput;
use crate::util::fuzzy::{fuzzy_atom, fuzzy_score, new_matcher};
use crate::util::score_utils;

use super::history::HistoryCache;

pub const TYPE_ID: &str = "clipboard";
const DISPLAY_NAME_CHARS: usize = 64;
/// Inline full text into list results only when small enough to clone cheaply.
const INLINE_CONTENT_BYTES: usize = 4 * 1024;
/// Cap returned list rows (UI also caps).
const MAX_LIST_RESULTS: usize = 1500;
/// Max rows kept in the in-memory index.
const MAX_CACHE_ROWS: i64 = 8000;

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
            // Read-only path: tolerate concurrent writer.
            let _ = conn.execute_batch("PRAGMA query_only=ON; PRAGMA busy_timeout=200;");
            cell.replace(conn);
        }
        f(cell.as_ref().unwrap())
    })
}

fn db_mtime() -> Option<SystemTime> {
    clip_db_path()
        .ok()
        .and_then(|p| std::fs::metadata(p).ok())
        .and_then(|m| m.modified().ok())
}

/// One row in the hot index. `content` is shared so filter never re-reads SQLite.
#[derive(Clone)]
struct ClipCacheEntry {
    id: String,
    content_type: String,
    display_name: String,
    is_image: bool,
    count: i64,
    timestamp: DateTime<Utc>,
    content: Arc<str>,
}

impl ClipCacheEntry {
    fn to_list_result(&self) -> ClipResult {
        let inline = self.is_image || self.content.len() <= INLINE_CONTENT_BYTES;
        ClipResult {
            // Large text left empty → resolve on paste/preview by id from cache/DB.
            content: if inline {
                self.content.to_string()
            } else {
                String::new()
            },
            content_type: self.content_type.clone(),
            insert_time: self.timestamp,
            update_time: self.timestamp,
            count: self.count,
            id: self.id.clone(),
            is_image: self.is_image,
            display_name: self.display_name.clone(),
        }
    }
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
            first_line_preview(content, DISPLAY_NAME_CHARS)
        }
    }

    /// Full body for paste/preview. Uses inlined content when present, else cache, else DB.
    pub fn resolved_content(&self) -> String {
        if !self.content.is_empty() || self.is_image {
            return self.content.clone();
        }
        if let Some(cached) = ClipPlugin::content_from_cache(&self.id) {
            return cached;
        }
        load_content_by_id(&self.id).unwrap_or_default()
    }
}

fn first_line_preview(content: &str, max_chars: usize) -> String {
    let first_line = content
        .split(['\n', '\r'])
        .next()
        .unwrap_or_default()
        .trim();
    let mut chars = first_line.chars();
    let mut preview: String = chars.by_ref().take(max_chars).collect();
    let was_truncated = chars.next().is_some();
    let has_more_lines = content.contains(['\n', '\r']);

    if was_truncated || has_more_lines {
        preview.push('…');
    }
    preview
}

fn load_content_by_id(id: &str) -> AResult<String> {
    with_clip_db(|conn| {
        let content: String = conn.query_row(
            "SELECT content FROM history WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;
        Ok(content)
    })
}

fn load_cache_from_db() -> AResult<Vec<ClipCacheEntry>> {
    with_clip_db(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, content_type, content, timestamp, icount \
             FROM history ORDER BY timestamp DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([MAX_CACHE_ROWS], |row| {
            let id: String = row.get(0)?;
            let content_type: String = row.get(1)?;
            let content: String = row.get(2)?;
            let timestamp: DateTime<Utc> = row.get(3)?;
            let count: i64 = row.get(4)?;
            let is_image = content_type == "image";
            let display_name = ClipResult::compute_display_name(&content, is_image);
            Ok(ClipCacheEntry {
                id,
                content_type,
                display_name,
                is_image,
                count,
                timestamp,
                content: Arc::from(content),
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
}

#[cfg(test)]
mod tests {
    use super::first_line_preview;

    #[test]
    fn clipboard_preview_uses_only_first_line() {
        assert_eq!(
            first_line_preview("first line\nsecond line", 64),
            "first line…"
        );
    }

    #[test]
    fn clipboard_preview_marks_long_single_line() {
        assert_eq!(first_line_preview("你好世界", 2), "你好…");
    }

    #[test]
    fn clipboard_preview_leaves_short_single_line_unchanged() {
        assert_eq!(first_line_preview("  short text  ", 64), "short text");
    }
}

impl PluginResult for ClipResult {
    fn icon_name(&self) -> &str {
        if self.is_image {
            "image-x-generic"
        } else {
            "text-x-generic"
        }
    }

    fn name(&self) -> &str {
        &self.display_name
    }

    fn extra(&self) -> Option<&str> {
        None
    }

    fn on_enter(&self) {
        let mut clipboard = match Clipboard::new() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("clipboard open failed: {e}");
                return;
            }
        };
        if self.is_image {
            let path = self.resolved_content();
            let decoded = image::ImageReader::open(&path)
                .map_err(|e| e.to_string())
                .and_then(|r| r.decode().map_err(|e| e.to_string()));
            match decoded {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    let (w, h) = rgba.dimensions();
                    let image_data = arboard::ImageData {
                        width: w as usize,
                        height: h as usize,
                        bytes: rgba.into_raw().into(),
                    };
                    if let Err(e) = clipboard.set_image(image_data) {
                        tracing::error!("set_image failed: {e}");
                    }
                }
                Err(e) => tracing::error!("decode image {path}: {e}"),
            }
        } else {
            let text = self.resolved_content();
            if let Err(e) = clipboard.set_text(text) {
                tracing::error!("set_text failed: {e}");
            }
        }
    }

    fn get_type_id(&self) -> &'static str {
        &TYPE_ID
    }

    fn get_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.id.as_str())
    }

    fn to_enum(self) -> super::PluginResultEnum {
        super::PluginResultEnum::Clip(self)
    }
}

pub struct ClipPlugin {
    history: HistoryCache<ClipResult>,
    entries: ArcSwap<Vec<ClipCacheEntry>>,
    /// Last seen clip.db mtime; reload when writer advances it.
    db_mtime: ArcSwap<Option<SystemTime>>,
}

impl ClipPlugin {
    pub fn new() -> AResult<Self> {
        let histories: Vec<HistoryItem<ClipResult>> =
            CONNECTION.with_borrow(|e| HistoryDb::new(e.as_ref()).fetch_histories(TYPE_ID))?;

        let plugin = ClipPlugin {
            history: HistoryCache::new(histories),
            entries: ArcSwap::from_pointee(Vec::new()),
            db_mtime: ArcSwap::from_pointee(None),
        };
        plugin.reload_cache();
        Ok(plugin)
    }

    fn content_from_cache(id: &str) -> Option<String> {
        CONTENT_BY_ID
            .load()
            .get(id)
            .map(|s| s.to_string())
    }

    fn reload_cache(&self) {
        let started = Instant::now();
        match load_cache_from_db() {
            Ok(entries) => {
                let n = entries.len();
                let bytes: usize = entries.iter().map(|e| e.content.len()).sum();
                let mut by_id = std::collections::HashMap::with_capacity(n);
                for e in &entries {
                    by_id.insert(e.id.clone(), e.content.clone());
                }
                CONTENT_BY_ID.store(Arc::new(by_id));
                self.entries.store(Arc::new(entries));
                self.db_mtime.store(Arc::new(db_mtime()));
                tracing::info!(
                    event = "clipboard_cache_reload",
                    count = n,
                    total_content_bytes = bytes,
                    elapsed_us = started.elapsed().as_micros() as u64,
                );
            }
            Err(e) => {
                tracing::warn!("clipboard cache reload failed: {e:#}");
            }
        }
    }

    fn ensure_fresh(&self) {
        let current = db_mtime();
        let cached = self.db_mtime.load();
        if current != **cached {
            self.reload_cache();
        }
    }
}

impl Plugin for ClipPlugin {
    type R = ClipResult;

    type T = ClipReq;

    fn refresh_content(&self) {
        self.reload_cache();
    }

    fn handle_input(&self, user_input: &UserInput) -> AResult<Vec<(ClipResult, i32)>> {
        let started = Instant::now();
        self.ensure_fresh();

        let entries = self.entries.load();
        let q = user_input.input.as_str();
        let query_empty = q.is_empty();

        let result: Vec<(ClipResult, i32)> = if query_empty {
            entries
                .iter()
                .take(MAX_LIST_RESULTS)
                .map(|e| {
                    // Recency already in DB order (DESC timestamp).
                    (e.to_list_result(), score_utils::middle(e.count.max(0)))
                })
                .collect()
        } else {
            let pattern = fuzzy_atom(q);
            let mut matcher = new_matcher();
            let mut buf = Vec::new();
            let mut scored: Vec<(ClipResult, i32)> = entries
                .iter()
                .filter_map(|e| {
                    // Title-first; also score small bodies so short clips stay findable.
                    let mut best = fuzzy_score(&mut matcher, &pattern, &e.display_name, &mut buf);
                    if best.is_none() && !e.is_image && e.content.len() <= INLINE_CONTENT_BYTES {
                        best = fuzzy_score(&mut matcher, &pattern, e.content.as_ref(), &mut buf);
                    }
                    let score = best?;
                    Some((
                        e.to_list_result(),
                        score_utils::middle(score as i64 + e.count.max(0)),
                    ))
                })
                .collect();
            scored.sort_unstable_by(|a, b| b.1.cmp(&a.1));
            if scored.len() > MAX_LIST_RESULTS {
                scored.truncate(MAX_LIST_RESULTS);
            }
            scored
        };

        tracing::info!(
            event = "clipboard_query_complete",
            query_empty,
            count = result.len(),
            cache_size = entries.len(),
            elapsed_us = started.elapsed().as_micros() as u64,
        );

        Ok(result)
    }

    fn get_type_id(&self) -> &'static str {
        &TYPE_ID
    }

    impl_history!();
}
