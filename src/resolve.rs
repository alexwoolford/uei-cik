//! Load both sides, accept exact pairs, write the map.

use anyhow::Result;

use crate::db::{retract_absent, upsert_match, upsert_run};
use crate::match_rule::{accept, MatchRow, RecipientHit};
use crate::normalize::normalize_name;
use crate::parse::{
    hit, parse_recipient_search, parse_recipient_state, parse_submissions, parse_ticker_file,
    RecipientStub,
};

pub struct ResolveStats {
    pub status: &'static str,
    pub ciks_seen: i64,
    pub recipients_seen: i64,
    pub matches_upserted: i64,
    pub matches_retracted: i64,
}

pub trait Sources {
    fn ticker_file(&self) -> Result<String>;
    fn submissions(&self, cik10: &str) -> Result<String>;
    fn search_recipients(&self, normalized_name: &str) -> Result<String>;
    fn recipient_detail(&self, id: &str) -> Result<String>;
}

/// Build the accepted pairs in memory. A fetch error returns before any write.
pub fn resolve(sources: &dyn Sources) -> Result<(Vec<MatchRow>, i64, i64)> {
    let tickers = parse_ticker_file(&sources.ticker_file()?)?;
    tracing::info!(ciks = tickers.len(), "ticker file loaded");
    let mut companies = Vec::with_capacity(tickers.len());
    for (i, t) in tickers.iter().enumerate() {
        let body = sources.submissions(&t.cik)?;
        companies.push(parse_submissions(&t.cik, &body)?);
        if (i + 1) % 1000 == 0 {
            tracing::info!(submissions = i + 1, "submissions loaded");
        }
    }
    let ciks_seen = companies.len() as i64;

    let mut names = Vec::new();
    for co in &companies {
        if co.entity_type.trim().eq_ignore_ascii_case("investment") {
            continue;
        }
        let mut variants = vec![co.conformed_name.clone()];
        variants.extend(co.former_names.iter().cloned());
        for raw in variants {
            let norm = normalize_name(&raw);
            if !norm.is_empty() {
                names.push(norm);
            }
        }
    }
    names.sort();
    names.dedup();
    tracing::info!(ciks = ciks_seen, names = names.len(), "names to search");

    let mut hits = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (i, name) in names.iter().enumerate() {
        if (i + 1) % 500 == 0 {
            tracing::info!(searches = i + 1, names = names.len(), "recipient search");
        }
        let stubs = parse_recipient_search(&sources.search_recipients(name)?)?;
        for stub in stubs {
            if normalize_name(&stub.name) != *name {
                continue;
            }
            if !seen.insert(stub.uei.clone()) {
                continue;
            }
            hits.push(stub_to_hit(sources, &stub)?);
        }
    }
    let recipients_seen = hits.len() as i64;
    let matches = accept(&companies, &hits);
    Ok((matches, ciks_seen, recipients_seen))
}

fn stub_to_hit(sources: &dyn Sources, stub: &RecipientStub) -> Result<RecipientHit> {
    let state = parse_recipient_state(&sources.recipient_detail(&stub.id)?)?;
    Ok(hit(stub, &state))
}

/// Write a completed match set. Call only after `resolve` succeeds.
pub fn apply_matches(
    conn: &rusqlite::Connection,
    as_of_date: &str,
    started_at: chrono::DateTime<chrono::Utc>,
    finished_at: chrono::DateTime<chrono::Utc>,
    ciks_seen: i64,
    recipients_seen: i64,
    matches: &[MatchRow],
) -> Result<ResolveStats> {
    let mut upserted = 0;
    for row in matches {
        if upsert_match(conn, row, as_of_date)? {
            upserted += 1;
        }
    }
    let retracted = retract_absent(conn, matches, finished_at.timestamp())?;
    upsert_run(
        conn,
        as_of_date,
        started_at,
        finished_at,
        "ok",
        ciks_seen,
        recipients_seen,
        upserted,
        retracted,
    )?;
    Ok(ResolveStats {
        status: "ok",
        ciks_seen,
        recipients_seen,
        matches_upserted: upserted,
        matches_retracted: retracted,
    })
}

pub fn record_error(
    conn: &rusqlite::Connection,
    as_of_date: &str,
    started_at: chrono::DateTime<chrono::Utc>,
    finished_at: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    upsert_run(
        conn,
        as_of_date,
        started_at,
        finished_at,
        "error",
        0,
        0,
        0,
        0,
    )
}
