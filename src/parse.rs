//! SEC ticker file, submissions JSON, and USAspending recipient JSON.

use anyhow::{bail, Context, Result};
use serde_json::Value;

use crate::match_rule::{Company, RecipientHit};
use crate::pad_cik;

#[derive(Debug, Clone)]
pub struct TickerCik {
    pub cik: String,
}

#[derive(Debug, Clone)]
pub struct RecipientStub {
    pub id: String,
    pub uei: String,
    pub name: String,
}

pub fn parse_ticker_file(body: &str) -> Result<Vec<TickerCik>> {
    let v: Value = serde_json::from_str(body).context("company_tickers.json")?;
    let obj = v
        .as_object()
        .context("company_tickers.json is not an object")?;
    let mut out = Vec::with_capacity(obj.len());
    for row in obj.values() {
        let cik = row
            .get("cik_str")
            .and_then(|c| {
                c.as_i64()
                    .map(|n| n.to_string())
                    .or_else(|| c.as_str().map(str::to_string))
            })
            .context("cik_str")?;
        out.push(TickerCik { cik: pad_cik(&cik) });
    }
    out.sort_by(|a, b| a.cik.cmp(&b.cik));
    out.dedup_by(|a, b| a.cik == b.cik);
    Ok(out)
}

pub fn parse_submissions(cik: &str, body: &str) -> Result<Company> {
    let v: Value = serde_json::from_str(body).context("submissions json")?;
    let conformed = v
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if conformed.is_empty() {
        bail!("submissions for {cik} have no name");
    }
    let entity_type = v
        .get("entityType")
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string();
    let business_state = v
        .pointer("/addresses/business/stateOrCountry")
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .trim()
        .to_ascii_uppercase();
    let mut former_names = Vec::new();
    if let Some(arr) = v.get("formerNames").and_then(|n| n.as_array()) {
        for item in arr {
            if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                let name = name.trim();
                if !name.is_empty() {
                    former_names.push(name.to_string());
                }
            }
        }
    }
    Ok(Company {
        cik: pad_cik(cik),
        conformed_name: conformed,
        former_names,
        business_state,
        entity_type,
    })
}

pub fn parse_recipient_search(body: &str) -> Result<Vec<RecipientStub>> {
    let v: Value = serde_json::from_str(body).context("recipient search")?;
    let Some(results) = v.get("results").and_then(|n| n.as_array()) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for row in results {
        let uei = row.get("uei").and_then(|n| n.as_str()).unwrap_or("").trim();
        let name = row
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .trim();
        let id = row.get("id").and_then(|n| n.as_str()).unwrap_or("").trim();
        if uei.is_empty() || name.is_empty() || id.is_empty() {
            continue;
        }
        out.push(RecipientStub {
            id: id.to_string(),
            uei: uei.to_ascii_uppercase(),
            name: name.to_string(),
        });
    }
    Ok(out)
}

pub fn parse_recipient_state(body: &str) -> Result<String> {
    let v: Value = serde_json::from_str(body).context("recipient detail")?;
    let state = v
        .pointer("/location/state_code")
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .trim()
        .to_ascii_uppercase();
    Ok(state)
}

pub fn hit(stub: &RecipientStub, state: &str) -> RecipientHit {
    RecipientHit {
        uei: stub.uei.clone(),
        name: stub.name.clone(),
        state: state.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticker_file_pads_cik() {
        let body = r#"{"0":{"cik_str":936468,"ticker":"LMT","title":"LOCKHEED MARTIN CORP"}}"#;
        let rows = parse_ticker_file(body).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].cik, "0000936468");
    }

    #[test]
    fn submissions_keep_former_name_and_business_state() {
        let body = r#"{
            "name": "NEW NAME INC",
            "entityType": "operating",
            "formerNames": [{"name": "OLD NAME CORP"}],
            "addresses": {"business": {"stateOrCountry": "ca"}}
        }"#;
        let co = parse_submissions("3", body).unwrap();
        assert_eq!(co.cik, "0000000003");
        assert_eq!(co.former_names, vec!["OLD NAME CORP"]);
        assert_eq!(co.business_state, "CA");
    }

    #[test]
    fn search_and_detail_read_uei_and_state() {
        let search = r#"{"results":[{"id":"abc-C","uei":"xfjmysyfjek4","name":"LOCKHEED MARTIN CORPORATION"}]}"#;
        let stubs = parse_recipient_search(search).unwrap();
        assert_eq!(stubs[0].uei, "XFJMYSYFJEK4");
        let detail = r#"{"location":{"state_code":"md"}}"#;
        assert_eq!(parse_recipient_state(detail).unwrap(), "MD");
    }
}
