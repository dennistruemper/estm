//! SQLite persistence for clipboard history (`fun.estm.*` via [`sqlite_path`]).
use rusqlite::{params, Connection, OptionalExtension};
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

/// Hard cap — larger blobs are skipped (until rich media is supported).
const CLIPBOARD_MAX_CHARS: usize = 512_000;
const MAX_LABEL_CHARS: usize = 80;

/// Retention policy passed in from [`crate::settings::HistoryLimits`].
#[derive(Clone, Copy, Debug)]
pub struct HistoryLimits {
    pub max_rows: i64,
    /// `None` = ignore age; only cap by row count.
    pub max_age_ms: Option<i64>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipRow {
    pub id: i64,
    pub plaintext: String,
    pub created_ms: i64,
    pub pinned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[inline]
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[inline]
pub fn fingerprint_plaintext(raw: &str) -> String {
    raw.trim().split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn sqlite_path() -> PathBuf {
    directories::ProjectDirs::from("fun", "estm", "ESTM")
        .expect("unable to resolve application storage directory")
        .data_dir()
        .join("estm.sqlite")
}

pub fn open_connection(path: &Path) -> rusqlite::Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    }
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
    Ok(conn)
}

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS clips (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            plaintext TEXT NOT NULL,
            created_ms INTEGER NOT NULL,
            pinned INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0, 1))
        );

        CREATE INDEX IF NOT EXISTS idx_clips_created ON clips (created_ms);
        "#,
    )?;
    ensure_label_column(conn)
}

fn ensure_label_column(conn: &Connection) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(clips)")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        if row.get::<_, String>(1)? == "label" {
            return Ok(());
        }
    }
    conn.execute("ALTER TABLE clips ADD COLUMN label TEXT", [])?;
    Ok(())
}

fn read_clip_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ClipRow> {
    let label: Option<String> = row.get(4)?;
    Ok(ClipRow {
        id: row.get(0)?,
        plaintext: row.get(1)?,
        created_ms: row.get(2)?,
        pinned: row.get::<_, i64>(3)? != 0,
        label: label.and_then(|s| {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }),
    })
}

fn normalize_label(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    Some(t.chars().take(MAX_LABEL_CHARS).collect())
}

pub fn prune_and_cap(conn: &Connection, limits: HistoryLimits) -> rusqlite::Result<()> {
    if let Some(ttl_ms) = limits.max_age_ms {
        let cutoff = now_ms().saturating_sub(ttl_ms);
        conn.execute(
            "DELETE FROM clips WHERE pinned = 0 AND created_ms < ?1",
            [cutoff],
        )?;
    }

    let max_rows = limits.max_rows.max(1);
    conn.execute_batch(&format!(
        r"DELETE FROM clips WHERE id NOT IN (
               SELECT id FROM clips
               ORDER BY pinned DESC, created_ms DESC, id DESC
               LIMIT {max_rows}
           );",
    ))?;

    Ok(())
}

/// Returns `true` if history changed (new row or timestamp refresh on duplicate suppression).
pub fn record_plaintext_capture(
    conn: &Connection,
    plaintext: String,
    limits: HistoryLimits,
) -> rusqlite::Result<bool> {
    if plaintext.len() > CLIPBOARD_MAX_CHARS {
        return Ok(false);
    }

    let body = plaintext.trim_start_matches('\u{feff}').trim_matches('\0');

    let trimmed_all = body.trim();

    let fp = fingerprint_plaintext(trimmed_all);
    if fp.is_empty() {
        return Ok(false);
    }

    let prev = conn
        .query_row(
            "SELECT id, plaintext FROM clips ORDER BY id DESC LIMIT 1",
            [],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()?;

    if let Some((id, prev_text)) = prev {
        let prev_fp = fingerprint_plaintext(prev_text.trim());
        if prev_fp == fp && !prev_fp.is_empty() {
            let ts = now_ms();
            conn.execute(
                "UPDATE clips SET created_ms = ?1 WHERE id = ?2",
                params![ts, id],
            )?;
            prune_and_cap(conn, limits)?;
            return Ok(true);
        }
    }

    let ts = now_ms();
    conn.execute(
        "INSERT INTO clips (plaintext, created_ms, pinned) VALUES (?1, ?2, 0)",
        params![trimmed_all, ts],
    )?;
    prune_and_cap(conn, limits)?;

    Ok(true)
}

pub fn list_recent(conn: &Connection, limit: i64) -> rusqlite::Result<Vec<ClipRow>> {
    let mut stmt = conn.prepare(
        r#"SELECT id, plaintext, created_ms, pinned, label
           FROM clips
           ORDER BY pinned DESC, created_ms DESC, id DESC
           LIMIT ?1"#,
    )?;
    let mut rows = stmt.query(params![limit])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(read_clip_row(&row)?);
    }
    Ok(out)
}

fn escape_like_user_input(q: &str) -> String {
    q.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

pub fn search(conn: &Connection, query: &str, limit: i64) -> rusqlite::Result<Vec<ClipRow>> {
    let pattern = format!("%{}%", escape_like_user_input(query));
    let mut stmt = conn.prepare(
        r#"SELECT id, plaintext, created_ms, pinned, label
           FROM clips
           WHERE plaintext LIKE ?1 ESCAPE '\'
              OR (label IS NOT NULL AND label LIKE ?1 ESCAPE '\')
           ORDER BY
             CASE
               WHEN label IS NOT NULL AND label LIKE ?1 ESCAPE '\' THEN 0
               ELSE 1
             END,
             pinned DESC,
             created_ms DESC,
             id DESC
           LIMIT ?2"#,
    )?;
    let mut rows = stmt.query(params![pattern, limit])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(read_clip_row(&row)?);
    }
    Ok(out)
}

pub fn clear_all(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("DELETE FROM clips;")?;
    Ok(())
}

pub fn get_plaintext(conn: &Connection, id: i64) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT plaintext FROM clips WHERE id = ?1", [id], |r| {
        r.get(0)
    })
    .optional()
}

/// Returns number of rows updated (0 if id missing).
pub fn set_pinned(conn: &Connection, id: i64, pinned: bool) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE clips SET pinned = ?2 WHERE id = ?1",
        params![id, pinned as i64],
    )
}

/// Returns number of rows deleted (0 if id missing).
pub fn delete_by_id(conn: &Connection, id: i64) -> rusqlite::Result<usize> {
    conn.execute("DELETE FROM clips WHERE id = ?1", [id])
}

/// Returns number of rows updated (0 if id missing). `label: None` clears the label.
pub fn set_label(conn: &Connection, id: i64, label: Option<&str>) -> rusqlite::Result<usize> {
    let stored = label.and_then(normalize_label);
    conn.execute(
        "UPDATE clips SET label = ?2 WHERE id = ?1",
        params![id, stored],
    )
}
