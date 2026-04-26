use std::path::Path;

use miette::{Context, IntoDiagnostic, Result};
use regex::Regex;
use rusqlite::Connection;
use tracing::instrument;

pub fn get_db_connection(path_db: &Path) -> Result<Connection> {
    Connection::open(path_db)
        .into_diagnostic()
        .context("failed to connect to the database")
}

#[instrument]
pub fn init_db(path_db: &Path) -> Result<Connection> {
    tracing::debug!("initialising DB");
    let conn = get_db_connection(path_db)?;

    tracing::trace!("applying PRAGMA");
    conn.pragma_update(None, "journal_mode", "WAL")
        .into_diagnostic()
        .context("failed to apply PRAGMA: journal mode")?;
    conn.pragma_update(None, "synchronous", "normal")
        .into_diagnostic()
        .context("failed to apply PRAGMA: synchronous")?;
    conn.pragma_update(None, "journal_size_limit", "6144000")
        .into_diagnostic()
        .context("failed to apply PRAGMA: journal size limit")?;
    conn.pragma_update(None, "cache_size", "10000")
        .into_diagnostic()
        .context("failed to apply PRAGMA: cache size")?;

    tracing::trace!("applying migrations");
    apply_migrations(&conn)?;

    tracing::trace!("registering regexp function");
    conn.create_scalar_function(
        "regexp",
        2,
        rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let pattern = ctx.get::<String>(0)?;
            let text = ctx.get::<String>(1)?;
            let re =
                Regex::new(&pattern).map_err(|e| rusqlite::Error::UserFunctionError(e.into()))?;
            Ok(re.is_match(&text))
        },
    )
    .into_diagnostic()
    .context("failed to register regexp function")?;

    Ok(conn)
}

fn apply_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS clipboard (
            id INTEGER PRIMARY KEY,
            content BLOB NOT NULL UNIQUE,
            last_updated INTEGER NOT NULL
        ) STRICT;",
    )
    .into_diagnostic()
    .context("failed to create clipboard table")?;

    conn.execute_batch("CREATE INDEX IF NOT EXISTS last_updated ON clipboard (last_updated);")
        .into_diagnostic()
        .context("failed to create last_updated index")?;

    let has_content_type = has_column(conn, "clipboard", "content_type");
    if !has_content_type {
        conn.execute_batch(
            "ALTER TABLE clipboard ADD COLUMN content_type INTEGER;
             ALTER TABLE clipboard ADD COLUMN mimetype TEXT;
             ALTER TABLE clipboard ADD COLUMN extra_preview_data TEXT;",
        )
        .into_diagnostic()
        .context("failed to add preview data columns")?;
    }

    Ok(())
}

fn has_column(conn: &Connection, table: &str, column: &str) -> bool {
    let columns: Vec<String> = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .flatten()
        .collect();
    columns.iter().any(|c| c == column)
}
