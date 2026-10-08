//! Exact name plus business-address state. A tie stores nothing.

use std::collections::HashMap;

use crate::normalize::normalize_name;

pub const METHOD: &str = "legal_name_exact";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Company {
    pub cik: String,
    pub conformed_name: String,
    pub former_names: Vec<String>,
    pub business_state: String,
    pub entity_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipientHit {
    pub uei: String,
    pub name: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchRow {
    pub uei: String,
    pub cik: String,
    pub method: String,
    pub recipient_name: String,
    pub sec_name: String,
    pub state: String,
}

/// Keep a recipient UEI on a listed CIK when the normalized name and the
/// business-address state agree, and exactly one kept CIK wins that pair.
pub fn accept(companies: &[Company], hits: &[RecipientHit]) -> Vec<MatchRow> {
    let mut index: HashMap<(String, String), Vec<Indexed>> = HashMap::new();
    for company in companies {
        if is_fund(company) {
            continue;
        }
        let state = company.business_state.trim().to_ascii_uppercase();
        if state.is_empty() {
            continue;
        }
        let mut names = Vec::with_capacity(1 + company.former_names.len());
        names.push(company.conformed_name.as_str());
        names.extend(company.former_names.iter().map(String::as_str));
        for raw in names {
            let norm = normalize_name(raw);
            if norm.is_empty() {
                continue;
            }
            index
                .entry((norm, state.clone()))
                .or_default()
                .push(Indexed {
                    cik: company.cik.clone(),
                    sec_name: company.conformed_name.clone(),
                });
        }
    }

    let mut unique: HashMap<(String, String), Indexed> = HashMap::new();
    for (key, rows) in index {
        let mut ciks: Vec<&str> = rows.iter().map(|r| r.cik.as_str()).collect();
        ciks.sort_unstable();
        ciks.dedup();
        if ciks.len() == 1 {
            unique.insert(key, rows.into_iter().next().expect("one row"));
        }
    }

    let mut out = Vec::new();
    let mut seen_uei = std::collections::HashSet::new();
    for hit in hits {
        let uei = hit.uei.trim().to_ascii_uppercase();
        if uei.len() != 12 || !uei.chars().all(|c| c.is_ascii_alphanumeric()) {
            continue;
        }
        if !seen_uei.insert(uei.clone()) {
            continue;
        }
        let norm = normalize_name(&hit.name);
        let state = hit.state.trim().to_ascii_uppercase();
        if norm.is_empty() || state.is_empty() {
            continue;
        }
        let Some(hit_co) = unique.get(&(norm, state.clone())) else {
            continue;
        };
        out.push(MatchRow {
            uei,
            cik: hit_co.cik.clone(),
            method: METHOD.to_string(),
            recipient_name: hit.name.clone(),
            sec_name: hit_co.sec_name.clone(),
            state,
        });
    }
    out.sort_by(|a, b| a.uei.cmp(&b.uei).then(a.cik.cmp(&b.cik)));
    out
}

struct Indexed {
    cik: String,
    sec_name: String,
}

fn is_fund(company: &Company) -> bool {
    company
        .entity_type
        .trim()
        .eq_ignore_ascii_case("investment")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn co(cik: &str, name: &str, former: &[&str], state: &str, entity: &str) -> Company {
        Company {
            cik: cik.to_string(),
            conformed_name: name.to_string(),
            former_names: former.iter().map(|s| (*s).to_string()).collect(),
            business_state: state.to_string(),
            entity_type: entity.to_string(),
        }
    }

    fn hit(uei: &str, name: &str, state: &str) -> RecipientHit {
        RecipientHit {
            uei: uei.to_string(),
            name: name.to_string(),
            state: state.to_string(),
        }
    }

    #[test]
    fn lockheed_shaped_name_keeps_two_ueis() {
        let companies = [co(
            "0000936468",
            "LOCKHEED MARTIN CORP",
            &[],
            "MD",
            "operating",
        )];
        let hits = [
            hit("XFJMYSYFJEK4", "LOCKHEED MARTIN CORPORATION", "MD"),
            hit("G4KDGE4JFFK7", "Lockheed Martin Corp.", "md"),
        ];
        let rows = accept(&companies, &hits);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.cik == "0000936468"));
        assert!(rows.iter().all(|r| r.method == METHOD));
        assert!(rows.iter().all(|r| r.state == "MD"));
        assert_eq!(rows[0].sec_name, "LOCKHEED MARTIN CORP");
    }

    #[test]
    fn short_name_collision_stores_nothing() {
        let companies = [
            co("0000000001", "NEUTRON INC", &[], "TX", "operating"),
            co("0000000002", "Neutron Corp.", &[], "TX", "operating"),
        ];
        let hits = [hit("NEUTRONUEI01", "NEUTRON", "TX")];
        assert!(accept(&companies, &hits).is_empty());
    }

    #[test]
    fn former_name_matches() {
        let companies = [co(
            "0000000003",
            "NEW NAME INC",
            &["OLD NAME CORP"],
            "CA",
            "operating",
        )];
        let hits = [hit("OLDNAMEUEI01", "OLD NAME CORPORATION", "CA")];
        let rows = accept(&companies, &hits);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].cik, "0000000003");
        assert_eq!(rows[0].sec_name, "NEW NAME INC");
        assert_eq!(rows[0].recipient_name, "OLD NAME CORPORATION");
    }

    #[test]
    fn state_mismatch_stores_nothing() {
        let companies = [co(
            "0000936468",
            "LOCKHEED MARTIN CORP",
            &[],
            "MD",
            "operating",
        )];
        let hits = [hit("XFJMYSYFJEK4", "LOCKHEED MARTIN CORPORATION", "TX")];
        assert!(accept(&companies, &hits).is_empty());
    }

    #[test]
    fn fund_cik_is_dropped() {
        let companies = [co("0000000009", "SOME FUND INC", &[], "NY", "investment")];
        let hits = [hit("SOMEFUNDUEI1", "SOME FUND INC", "NY")];
        assert!(accept(&companies, &hits).is_empty());
    }
}
