use std::ops::{Deref, DerefMut};
use std::path::Path;

use anyhow::{Context, Result};
use capturable_state::{
    apply_runtime_pragmas, install, CaptureConfig, CaptureMode, Nudge, TableSpec,
};
use rusqlite::{params, Connection, OptionalExtension};

use crate::match_rule::MatchRow;
use crate::time::utc_iso;

pub const DB_NAME: &str = "uei-cik";

pub struct WorkDb {
    conn: Connection,
    pub nudge: Nudge,
}

impl Deref for WorkDb {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        &self.conn
    }
}

impl DerefMut for WorkDb {
    fn deref_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}

fn open_conn(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
    }
    let conn = Connection::open(path).with_context(|| format!("open {}", path.display()))?;
    apply_runtime_pragmas(&conn)?;
    apply_schema(&conn)?;
    Ok(conn)
}

pub fn open(path: &Path) -> Result<Connection> {
    open_conn(path)
}

pub fn open_work(path: &Path) -> Result<WorkDb> {
    let conn = open_conn(path)?;
    let nudge = install_capture(&conn, path)?;
    Ok(WorkDb { conn, nudge })
}

fn apply_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(include_str!("schema.sql"))
        .context("apply schema")?;
    Ok(())
}

fn install_capture(conn: &Connection, path: &Path) -> Result<Nudge> {
    let tables = [
        TableSpec::new("matches", CaptureMode::After),
        TableSpec::new("match_runs", CaptureMode::After),
    ];
    install(conn, &CaptureConfig::new(DB_NAME, path, &tables))
}

/// Change-aware upsert. An identical live row emits no extra `_outbox` row.
/// `as_of_date` changes only when another stored column changes, or the row
/// is coming back from retraction.
pub fn upsert_match(conn: &Connection, row: &MatchRow, as_of_date: &str) -> Result<bool> {
    let n = conn.execute(
        "INSERT INTO matches (
            uei, cik, method, recipient_name, sec_name, state, as_of_date, deleted_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)
         ON CONFLICT(uei, cik) DO UPDATE SET
            method = excluded.method,
            recipient_name = excluded.recipient_name,
            sec_name = excluded.sec_name,
            state = excluded.state,
            as_of_date = excluded.as_of_date,
            deleted_at = NULL
         WHERE matches.method IS NOT excluded.method
            OR matches.recipient_name IS NOT excluded.recipient_name
            OR matches.sec_name IS NOT excluded.sec_name
            OR matches.state IS NOT excluded.state
            OR matches.deleted_at IS NOT NULL",
        params![
            row.uei,
            row.cik,
            row.method,
            row.recipient_name,
            row.sec_name,
            row.state,
            as_of_date,
        ],
    )?;
    Ok(n > 0)
}

/// Retract live pairs that this successful run did not accept.
pub fn retract_absent(conn: &Connection, kept: &[MatchRow], deleted_at: i64) -> Result<i64> {
    conn.execute_batch("CREATE TEMP TABLE IF NOT EXISTS uei_keep (uei TEXT NOT NULL, cik TEXT NOT NULL, PRIMARY KEY (uei, cik))")?;
    conn.execute("DELETE FROM uei_keep", [])?;
    for row in kept {
        conn.execute(
            "INSERT INTO uei_keep (uei, cik) VALUES (?1, ?2)",
            params![row.uei, row.cik],
        )?;
    }
    let n = conn.execute(
        "UPDATE matches
         SET deleted_at = ?1
         WHERE deleted_at IS NULL
           AND NOT EXISTS (
             SELECT 1 FROM uei_keep k
             WHERE k.uei = matches.uei AND k.cik = matches.cik
           )",
        params![deleted_at],
    )?;
    Ok(n as i64)
}

#[allow(clippy::too_many_arguments)]
pub fn upsert_run(
    conn: &Connection,
    as_of_date: &str,
    started_at: chrono::DateTime<chrono::Utc>,
    finished_at: chrono::DateTime<chrono::Utc>,
    status: &str,
    ciks_seen: i64,
    recipients_seen: i64,
    matches_upserted: i64,
    matches_retracted: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO match_runs (
            as_of_date, started_at, finished_at, status,
            ciks_seen, recipients_seen, matches_upserted, matches_retracted
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(as_of_date) DO UPDATE SET
            started_at = excluded.started_at,
            finished_at = excluded.finished_at,
            status = excluded.status,
            ciks_seen = excluded.ciks_seen,
            recipients_seen = excluded.recipients_seen,
            matches_upserted = excluded.matches_upserted,
            matches_retracted = excluded.matches_retracted",
        params![
            as_of_date,
            utc_iso(started_at),
            utc_iso(finished_at),
            status,
            ciks_seen,
            recipients_seen,
            matches_upserted,
            matches_retracted,
        ],
    )?;
    Ok(())
}

#[derive(Debug)]
pub struct StatusRow {
    pub as_of_date: String,
    pub started_at: String,
    pub finished_at: String,
    pub status: String,
    pub ciks_seen: i64,
    pub recipients_seen: i64,
    pub matches_upserted: i64,
    pub matches_retracted: i64,
}

pub fn last_run(conn: &Connection) -> Result<Option<StatusRow>> {
    conn.query_row(
        "SELECT as_of_date, started_at, finished_at, status,
                ciks_seen, recipients_seen, matches_upserted, matches_retracted
         FROM match_runs
         ORDER BY as_of_date DESC
         LIMIT 1",
        [],
        |r| {
            Ok(StatusRow {
                as_of_date: r.get(0)?,
                started_at: r.get(1)?,
                finished_at: r.get(2)?,
                status: r.get(3)?,
                ciks_seen: r.get(4)?,
                recipients_seen: r.get(5)?,
                matches_upserted: r.get(6)?,
                matches_retracted: r.get(7)?,
            })
        },
    )
    .optional()
    .context("last match_run")
}

#[derive(Debug)]
pub struct LookupRow {
    pub uei: String,
    pub cik: String,
    pub recipient_name: String,
    pub sec_name: String,
    pub state: String,
}

pub fn lookup(conn: &Connection, query: &str) -> Result<Vec<LookupRow>> {
    let q = query.trim();
    let cik = crate::pad_cik(q);
    let uei = q.to_ascii_uppercase();
    let mut stmt = conn.prepare(
        "SELECT uei, cik, recipient_name, sec_name, state
         FROM matches
         WHERE deleted_at IS NULL AND (uei = ?1 OR cik = ?2)
         ORDER BY uei",
    )?;
    let rows = stmt.query_map(params![uei, cik], |r| {
        Ok(LookupRow {
            uei: r.get(0)?,
            cik: r.get(1)?,
            recipient_name: r.get(2)?,
            sec_name: r.get(3)?,
            state: r.get(4)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn outbox_count(conn: &Connection) -> Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM _outbox", [], |r| r.get(0))
        .context("outbox count")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::match_rule::METHOD;
    use chrono::{TimeZone, Utc};

    fn row(uei: &str) -> MatchRow {
        MatchRow {
            uei: uei.to_string(),
            cik: "0000936468".into(),
            method: METHOD.into(),
            recipient_name: "LOCKHEED MARTIN CORPORATION".into(),
            sec_name: "LOCKHEED MARTIN CORP".into(),
            state: "MD".into(),
        }
    }

    #[test]
    fn identical_rerun_writes_no_outbox_row_and_retraction_does() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("uei-cik.sqlite");
        let db = open_work(&path).unwrap();
        let day = Utc.with_ymd_and_hms(2026, 10, 7, 12, 0, 0).unwrap();
        let first = row("XFJMYSYFJEK4");
        let second = row("G4KDGE4JFFK7");
        assert!(upsert_match(&db, &first, "2026-10-07").unwrap());
        assert!(upsert_match(&db, &second, "2026-10-07").unwrap());
        let after_insert = outbox_count(&db).unwrap();
        assert!(!upsert_match(&db, &first, "2026-10-08").unwrap());
        assert_eq!(outbox_count(&db).unwrap(), after_insert);
        let retracted = retract_absent(&db, &[first], day.timestamp()).unwrap();
        assert_eq!(retracted, 1);
        let live: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM matches WHERE deleted_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(live, 1);
    }
}
