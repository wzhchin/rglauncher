use std::io::Read;

use image::GenericImageView;
use miette::{Context, IntoDiagnostic, Result};
use rglauncher_clip_db::{ClipboardEntry, delete_all_entries, init_db, upsert_entry};

const MAX_ENTRIES: usize = 1000;
const MAX_ENTRY_AGE_SECS: u64 = 14 * 24 * 3600;
const MAX_ENTRY_LEN: usize = 5_000_000;

fn main() -> Result<()> {
    init_tracing();

    let path_db = dirs::data_local_dir()
        .context("could not identify user data directory")?
        .join("clipvault.db");

    store(&path_db)
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .init();
}

fn store(path_db: &std::path::Path) -> Result<()> {
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

    if buf.trim_ascii().is_empty() {
        tracing::debug!("only ASCII whitespace content");
        return Ok(());
    }

    let conn = &init_db(path_db)?;

    if MAX_ENTRY_AGE_SECS != 0 {
        let timestamp = rglauncher_clip_db::now() - MAX_ENTRY_AGE_SECS;
        let _ = rglauncher_clip_db::delete_entries_older_than(conn, timestamp);
    }

    let entry = {
        let content_type = content_inspector::inspect(&buf);
        let (mut mimetype, mut extra_preview_data) = (None, None);

        if content_type.is_binary() {
            if let Some((img_mimetype, img)) = decode_image(&buf) {
                let (w, h) = img.dimensions();
                extra_preview_data = Some(format!("{w}x{h}"));
                mimetype = Some(img_mimetype.into());
            } else if let Some(content_mimetype) = sniff_mimetype(&buf) {
                mimetype = Some(content_mimetype);
            }
        }

        ClipboardEntry {
            content: buf,
            content_type: Some(content_type),
            mimetype,
            extra_preview_data,
            ..Default::default()
        }
    };

    upsert_entry(conn, entry)?;

    if MAX_ENTRIES != 0 {
        let _ = rglauncher_clip_db::trim_entries(conn, MAX_ENTRIES);
    }

    Ok(())
}

fn decode_image(data: &[u8]) -> Option<(&'static str, image::DynamicImage)> {
    use std::io::Cursor;
    let img_reader = image::ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .ok()?;
    let mimetype = img_reader.format()?.to_mime_type();
    let img = img_reader.decode().ok()?;
    Some((mimetype, img))
}

fn sniff_mimetype(data: &[u8]) -> Option<String> {
    use mime_sniffer::MimeTypeSniffer;
    data.sniff_mime_type().map(String::from)
}
