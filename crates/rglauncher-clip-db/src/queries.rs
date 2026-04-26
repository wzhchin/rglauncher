use miette::{Context, IntoDiagnostic, Result, miette};
use regex::Regex;
use rusqlite::{Connection, params};

use crate::types::{ClipboardEntry, SearchParams};

#[tracing::instrument(skip(conn))]
pub fn count_entries(conn: &Connection) -> Result<usize> {
    tracing::debug!("getting count of total entries");

    conn.query_row(
        include_str!("./queries/count_entries.sql"),
        [],
        |row| row.get::<_, usize>(0),
    )
    .into_diagnostic()
    .context("failed to query: count of clipboard entries")
}

#[tracing::instrument(skip(conn))]
pub fn get_all_entries(conn: &Connection) -> Result<Vec<ClipboardEntry>> {
    tracing::debug!("getting all entries");

    let mut stmt = conn
        .prepare(include_str!("./queries/get_all.sql"))
        .into_diagnostic()
        .context("failed to prepare: get all entries")?;

    let mapped = stmt
        .query_map([], |row| ClipboardEntry::try_from(row))
        .into_diagnostic()
        .context("failed to query: get all entries")?;
    let entries: Vec<ClipboardEntry> = mapped
        .collect::<std::result::Result<Vec<_>, _>>()
        .into_diagnostic()
        .context("failed to create clipboard entries from database rows")?;

    Ok(entries)
}

#[tracing::instrument(skip(conn))]
pub fn delete_all_entries(conn: &Connection) -> Result<()> {
    tracing::debug!("deleting all entries");

    conn.execute(include_str!("./queries/delete_all.sql"), [])
        .map(|_| ())
        .into_diagnostic()
        .context("failed to execute: wipe entries")?;

    vacuum(conn)
}

#[tracing::instrument(skip(conn))]
fn vacuum(conn: &Connection) -> Result<()> {
    tracing::debug!("vacuuming DB");

    let estimated_free = get_estimated_free_space(conn).unwrap_or(1_000_000);
    if estimated_free < 1_000_000 {
        tracing::debug!(
            "estimated freed space ({estimated_free}) under the threshold - skipping VACUUM"
        );
        return Ok(());
    }

    conn.execute("VACUUM;", [])
        .map(|_| ())
        .into_diagnostic()
        .context("failed to execute: vacuum")
}

#[tracing::instrument(skip(conn))]
pub fn delete_entries_older_than(conn: &Connection, cutoff: &str) -> Result<usize> {
    tracing::debug!("deleting old entries before {}", cutoff);

    let changed = conn
        .execute(include_str!("./queries/delete_old.sql"), [cutoff])
        .into_diagnostic()
        .context("failed to execute: delete old entries")?;

    if changed > 0 {
        vacuum(conn).map(|_| changed)
    } else {
        Ok(changed)
    }
}

#[tracing::instrument(skip(conn))]
pub fn trim_entries(conn: &Connection, limit: usize) -> Result<usize> {
    tracing::debug!("trimming entries over limit");

    let count = count_entries(conn)?;
    if count <= limit {
        tracing::trace!("not over limit");
        return Ok(0);
    }

    let del = count - limit;
    let changed = conn
        .execute(include_str!("./queries/trim_entries.sql"), [del])
        .into_diagnostic()
        .context("failed to execute: trim clipboard entries")?;
    assert_eq!(
        del, changed,
        "should only delete specified number of entries"
    );

    vacuum(conn).map(|_| changed)
}

#[tracing::instrument(skip(conn))]
pub fn get_entry_by_id(conn: &Connection, id: &str) -> Result<ClipboardEntry> {
    tracing::debug!("getting entry by ID");

    conn.query_row(
        include_str!("./queries/get_entry.sql"),
        [id],
        |row| ClipboardEntry::try_from(row),
    )
    .into_diagnostic()
    .context("couldn't get entry by ID")
}

#[tracing::instrument(skip(conn))]
pub fn get_estimated_free_space(conn: &Connection) -> Result<u64> {
    tracing::debug!("getting estimate of space that can be freed");

    conn.query_row(
        "SELECT freelist_count * page_size AS freelist_size FROM pragma_freelist_count(), pragma_page_size()",
        [],
        |row| row.get::<_, u64>("freelist_size"),
    )
    .into_diagnostic()
    .context("couldn't get estimated free space")
}

#[tracing::instrument(skip(conn))]
pub fn delete_entry_by_id(conn: &Connection, id: &str) -> Result<()> {
    tracing::debug!("deleting specific entry by ID");

    let changed = conn
        .execute(include_str!("./queries/delete_entry.sql"), [id])
        .into_diagnostic()
        .context("failed to execute: delete specific entry")?;

    if changed == 0 {
        return Err(miette!("entry not found"));
    }
    assert_eq!(changed, 1, "should only delete specified entry");

    vacuum(conn)
}

#[tracing::instrument(skip_all)]
pub fn upsert_entry(conn: &Connection, entry: impl AsRef<ClipboardEntry>) -> Result<()> {
    let ClipboardEntry {
        id,
        content_type,
        content,
        favicon,
        timestamp,
        source,
        source_icon,
        language,
        ..
    } = entry.as_ref();

    tracing::debug!("upserting entry, id={}", id);
    tracing::debug!(
        "entry content preview: {}",
        &content[..64.min(content.len())]
    );

    conn.execute(
        include_str!("./queries/upsert_post.sql"),
        params![
            id,
            content_type.to_string(),
            content,
            favicon,
            timestamp,
            source,
            source_icon,
            language,
        ],
    )
    .map(|_| ())
    .into_diagnostic()
    .context("failed to execute: upsert clipboard entry")
}

#[tracing::instrument(skip(conn))]
pub fn search_entries(
    conn: &Connection,
    search_params: SearchParams,
) -> Result<Vec<ClipboardEntry>> {
    let SearchParams {
        query,
        use_regex,
        offset,
        limit,
    } = search_params;

    let mut where_clauses = Vec::new();
    let mut sql_params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(ref q) = query {
        if !q.is_empty() {
            if use_regex {
                Regex::new(q)
                    .into_diagnostic()
                    .context("invalid regex pattern")?;
                where_clauses.push("content regexp ?".to_string());
                sql_params.push(Box::new(q.clone()));
            } else {
                where_clauses.push("content LIKE ?".to_string());
                sql_params.push(Box::new(format!("%{q}%")));
            }
        }
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", where_clauses.join(" AND "))
    };

    let sql = format!(
        "SELECT
            id, content_type, content, favicon, timestamp, source, source_icon, language, icount
        FROM history
        {where_sql}
        ORDER BY timestamp DESC
        LIMIT ? OFFSET ?"
    );

    tracing::debug!("search SQL: {sql}");

    let mut stmt = conn
        .prepare(&sql)
        .into_diagnostic()
        .context("failed to prepare: search entries")?;

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = {
        let mut refs: Vec<&dyn rusqlite::types::ToSql> = Vec::with_capacity(sql_params.len() + 2);
        for p in &sql_params {
            refs.push(p.as_ref());
        }
        refs.push(&limit as &dyn rusqlite::types::ToSql);
        refs.push(&offset as &dyn rusqlite::types::ToSql);
        refs
    };

    let mapped = stmt
        .query_map(param_refs.as_slice(), |row| ClipboardEntry::try_from(row))
        .into_diagnostic()
        .context("failed to query: search entries")?;
    let entries: Vec<ClipboardEntry> = mapped
        .collect::<std::result::Result<Vec<_>, _>>()
        .into_diagnostic()
        .context("failed to create clipboard entries from search results")?;

    Ok(entries)
}
