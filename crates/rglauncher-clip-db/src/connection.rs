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
        "CREATE TABLE IF NOT EXISTS history (
            id TEXT PRIMARY KEY,
            content_type TEXT NOT NULL,
            content TEXT NOT NULL,
            favicon TEXT,
            timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
            source TEXT DEFAULT 'System' NOT NULL,
            source_icon TEXT,
            language TEXT,
            icount INT DEFAULT 0 NOT NULL
        );",
    )
    .into_diagnostic()
    .context("failed to create history table")?;

    conn.execute_batch("CREATE INDEX IF NOT EXISTS idx_history_timestamp ON history (timestamp);")
        .into_diagnostic()
        .context("failed to create timestamp index")?;

    Ok(())
}
