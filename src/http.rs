//! Live SEC and USAspending fetches.
//!
//! Successful JSON is written under a cache directory. A later run reads that
//! file and does not ask again. A 429, 500, 502, 503, or 504, or a dropped
//! connection, is retried a few times. The name-and-state rule is unchanged.

use std::cell::Cell;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::blocking::{Client, Response};
use reqwest::StatusCode;

use crate::resolve::Sources;

pub const DEFAULT_SLEEP_SECS: f64 = 0.5;
const TICKER_URL: &str = "https://www.sec.gov/files/company_tickers.json";
const RECIPIENT_SEARCH: &str = "https://api.usaspending.gov/api/v2/recipient/";
const MAX_ATTEMPTS: u32 = 4;

pub struct LiveSources {
    client: Client,
    sec_ua: String,
    sleep: Duration,
    cache_dir: PathBuf,
    live_fetches: Cell<u64>,
}

impl LiveSources {
    pub fn new(sec_ua: &str, sleep_secs: f64, cache_dir: PathBuf) -> Result<Self> {
        if sleep_secs < 0.0 {
            bail!("sleep must be >= 0");
        }
        fs::create_dir_all(&cache_dir)
            .with_context(|| format!("create {}", cache_dir.display()))?;
        let client = Client::builder()
            .timeout(Duration::from_secs(45))
            .build()
            .context("http client")?;
        Ok(Self {
            client,
            sec_ua: sec_ua.to_string(),
            sleep: Duration::from_secs_f64(sleep_secs),
            cache_dir,
            live_fetches: Cell::new(0),
        })
    }

    fn pause(&self) {
        if !self.sleep.is_zero() {
            thread::sleep(self.sleep);
        }
    }

    fn cached(&self, key: &str) -> Result<Option<String>> {
        let path = self.cache_dir.join(key);
        if !path.is_file() {
            return Ok(None);
        }
        let body =
            fs::read_to_string(&path).with_context(|| format!("read cache {}", path.display()))?;
        if body.is_empty() {
            return Ok(None);
        }
        Ok(Some(body))
    }

    fn store(&self, key: &str, body: &str) -> Result<()> {
        let path = self.cache_dir.join(key);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        let tmp = path.with_extension("tmp");
        {
            let mut file =
                fs::File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
            file.write_all(body.as_bytes())
                .with_context(|| format!("write {}", tmp.display()))?;
            file.sync_all()
                .with_context(|| format!("sync {}", tmp.display()))?;
        }
        fs::rename(&tmp, &path).with_context(|| format!("rename {}", path.display()))?;
        Ok(())
    }

    fn note_live(&self, label: &str) {
        let n = self.live_fetches.get() + 1;
        self.live_fetches.set(n);
        if n % 500 == 0 {
            tracing::info!(live_fetches = n, label, "fetch");
        }
    }

    /// Return cached JSON, or fetch it, retrying a transient failure.
    fn fetch(&self, key: &str, label: &str, live: impl Fn() -> Result<Response>) -> Result<String> {
        if let Some(body) = self.cached(key)? {
            return Ok(body);
        }
        self.pause();
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            match live() {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let body = resp.text().with_context(|| format!("read {label}"))?;
                        self.store(key, &body)?;
                        self.note_live(label);
                        return Ok(body);
                    }
                    let wait = retry_after_secs(&resp).unwrap_or_else(|| backoff_secs(attempt));
                    let _ = resp.text();
                    if is_transient(status) && attempt < MAX_ATTEMPTS {
                        tracing::warn!(attempt, %status, label, "transient status, retrying");
                        thread::sleep(Duration::from_secs(wait.min(120)));
                        continue;
                    }
                    bail!("{label} -> {status}");
                }
                Err(err) if attempt < MAX_ATTEMPTS => {
                    tracing::warn!(attempt, label, "fetch failed, retrying: {err:#}");
                    thread::sleep(Duration::from_secs(backoff_secs(attempt)));
                }
                Err(err) => return Err(err),
            }
        }
    }

    fn sec_get(&self, key: &str, url: &str) -> Result<String> {
        let ua = self.sec_ua.clone();
        let client = self.client.clone();
        let url_owned = url.to_string();
        self.fetch(key, url, move || {
            client
                .get(&url_owned)
                .header("User-Agent", &ua)
                .header("Accept", "application/json")
                .send()
                .with_context(|| format!("GET {url_owned}"))
        })
    }
}

impl Sources for LiveSources {
    fn ticker_file(&self) -> Result<String> {
        self.sec_get("sec/company_tickers.json", TICKER_URL)
    }

    fn submissions(&self, cik10: &str) -> Result<String> {
        let url = format!("https://data.sec.gov/submissions/CIK{cik10}.json");
        let key = format!("sec/CIK{cik10}.json");
        self.sec_get(&key, &url)
    }

    fn search_recipients(&self, normalized_name: &str) -> Result<String> {
        let key = format!("usa/search/{}.json", search_key(normalized_name));
        if self.cached(&key)?.is_none() {
            tracing::info!(name = %normalized_name, "recipient search live");
        }
        let name = normalized_name.to_string();
        let label = format!("POST recipient search {normalized_name}");
        let client = self.client.clone();
        match self.fetch(&key, &label, move || {
            let body = serde_json::json!({
                "keyword": name,
                "limit": 100,
                "page": 1
            });
            client
                .post(RECIPIENT_SEARCH)
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .context("POST recipient search")
        }) {
            Ok(body) => Ok(body),
            // A keyword the gateway will not answer (502/timeout after retries)
            // contributes no recipient. It is not cached, so a later run tries
            // again. Submissions and recipient detail still abort the pass.
            Err(err) => {
                tracing::warn!(
                    name = %normalized_name,
                    "recipient search unavailable, no UEI from this name: {err:#}"
                );
                Ok(r#"{"results":[]}"#.to_string())
            }
        }
    }

    fn recipient_detail(&self, id: &str) -> Result<String> {
        let key = detail_key(id)?;
        let url = format!("https://api.usaspending.gov/api/v2/recipient/{id}/");
        let client = self.client.clone();
        self.fetch(&key, "GET recipient detail", move || {
            client
                .get(&url)
                .send()
                .with_context(|| format!("GET {url}"))
        })
    }
}

fn is_transient(status: StatusCode) -> bool {
    matches!(status.as_u16(), 429 | 500 | 502 | 503 | 504)
}

fn backoff_secs(attempt: u32) -> u64 {
    2u64.saturating_pow(attempt.min(6))
}

fn retry_after_secs(resp: &Response) -> Option<u64> {
    let header = resp.headers().get(reqwest::header::RETRY_AFTER)?;
    let text = header.to_str().ok()?;
    text.parse::<u64>().ok()
}

fn search_key(normalized_name: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    normalized_name.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn detail_key(id: &str) -> Result<String> {
    if id.is_empty()
        || id.contains('/')
        || id.contains('\\')
        || id.contains("..")
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("recipient id is not a cache key: {id}");
    }
    Ok(format!("usa/detail/{id}.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_statuses() {
        assert!(is_transient(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_transient(StatusCode::BAD_GATEWAY));
        assert!(!is_transient(StatusCode::NOT_FOUND));
        assert!(!is_transient(StatusCode::OK));
    }

    #[test]
    fn cache_roundtrip_skips_a_second_read() {
        let dir = tempfile::tempdir().unwrap();
        let sources = LiveSources::new(
            "uei-cik test@example.invalid",
            0.0,
            dir.path().to_path_buf(),
        )
        .unwrap();
        sources
            .store("sec/CIK0000000001.json", "{\"cik\":1}")
            .unwrap();
        let body = sources.cached("sec/CIK0000000001.json").unwrap();
        assert_eq!(body.as_deref(), Some("{\"cik\":1}"));
        assert!(sources.cached("sec/missing.json").unwrap().is_none());
    }

    #[test]
    fn detail_key_rejects_path_separators() {
        assert!(detail_key("779b3e3e-364f-7cbc-28a4-a2b4484b86f4-C").is_ok());
        assert!(detail_key("../etc/passwd").is_err());
        assert!(detail_key("a/b").is_err());
    }
}
