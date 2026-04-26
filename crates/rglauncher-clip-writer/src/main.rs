use std::io::Read;

use miette::{Context, IntoDiagnostic, Result};
use rglauncher_clip_db::{ClipboardEntry, ContentType, delete_all_entries, init_db, trim_entries, upsert_entry, hash_bytes, delete_entries_older_than};

const MAX_ENTRIES: usize = 1000;
const MAX_ENTRY_AGE_DAYS: i64 = 14;
const MAX_ENTRY_LEN: usize = 5_000_000;

fn main() -> Result<()> {
    init_tracing();

    let base_dir = dirs::cache_dir()
        .context("could not identify user cache directory")?
        .join("rglauncher");

    std::fs::create_dir_all(&base_dir)
        .into_diagnostic()
        .context("failed to create cache directory")?;

    let path_db = base_dir.join("clip.db");
    let data_dir = base_dir.join("clip-data");

    store(&path_db, &data_dir)
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .init();
}

fn store(path_db: &std::path::Path, data_dir: &std::path::Path) -> Result<()> {
    if let Ok(s) = std::env::var("CLIPBOARD_STATE") {
        tracing::debug!("CLIPBOARD_STATE={s}");
        match s.as_str() {
            "sensitive" => {
                tracing::trace!("sensitive - not storing");
                return Ok(());
            }
            "clear" => {
                tracing::debug!("explicitly cleared clipboard");
                return delete_all_entries(&init_db(path_db)?);
            }
            "nil" => return Ok(()),
            _ => {}
        }
    };

    let buf = {
        let mut buf = vec![];
        std::io::stdin()
            .read_to_end(&mut buf)
            .into_diagnostic()
            .context("failed to read from STDIN")?;
        buf
    };

    if buf.is_empty() {
        tracing::trace!("no content to store");
        return Ok(());
    }

    if buf.len() > MAX_ENTRY_LEN && MAX_ENTRY_LEN != 0 {
        tracing::debug!(
            "content length ({}) exceeds max ({})",
            buf.len(),
            MAX_ENTRY_LEN
        );
        return Ok(());
    }

    let conn = &init_db(path_db)?;

    if MAX_ENTRY_AGE_DAYS != 0 {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(MAX_ENTRY_AGE_DAYS);
        let _ = delete_entries_older_than(conn, &cutoff.to_rfc3339());
    }

    let inspected = content_inspector::inspect(&buf);

    let entry = if inspected.is_binary() {
        let file_path = save_binary_to_file(data_dir, &buf)?;
        ClipboardEntry::new(
            "System".to_string(),
            ContentType::Image,
            file_path,
            None,
            None,
            None,
        )
    } else {
        let text = String::from_utf8_lossy(&buf).to_string();
        if text.trim().is_empty() {
            tracing::trace!("only whitespace content");
            return Ok(());
        }

        ClipboardEntry::new(
            "System".to_string(),
            ContentType::Text,
            text,
            None,
            None,
            None,
        )
    };

    upsert_entry(conn, entry)?;

    if MAX_ENTRIES != 0 {
        let _ = trim_entries(conn, MAX_ENTRIES);
    }

    Ok(())
}

fn save_binary_to_file(data_dir: &std::path::Path, data: &[u8]) -> Result<String> {
    let images_dir = data_dir.join("images");

    std::fs::create_dir_all(&images_dir)
        .into_diagnostic()
        .context("failed to create images directory")?;

    let hash = hash_bytes(data);
    let ext = detect_image_extension(data).unwrap_or("bin");
    let file_name = format!("{}.{}", hash, ext);
    let file_path = images_dir.join(&file_name);

    if !file_path.exists() {
        std::fs::write(&file_path, data)
            .into_diagnostic()
            .context("failed to write image file")?;
    }

    Ok(file_path.to_string_lossy().to_string())
}

fn detect_image_extension(data: &[u8]) -> Option<&'static str> {
    if data.len() < 8 {
        return None;
    }

    if &data[0..8] == b"\x89PNG\r\n\x1a\n" {
        return Some("png");
    }
    if &data[0..3] == b"\xff\xd8\xff" {
        return Some("jpg");
    }
    if &data[0..4] == b"RIFF" && data.len() >= 12 && &data[8..12] == b"WEBP" {
        return Some("webp");
    }
    if &data[0..6] == b"GIF87a" || &data[0..6] == b"GIF89a" {
        return Some("gif");
    }

    None
}
