//! Exact USAspending recipient UEI to listed SEC CIK.
//! A map, not a score: unmatched names stay unmatched.

pub mod db;
pub mod http;
pub mod match_rule;
pub mod normalize;
pub mod parse;
pub mod resolve;
pub mod sec_ua;
pub mod time;

pub use db::{open, open_work, WorkDb, DB_NAME};
pub use match_rule::{accept, Company, MatchRow, RecipientHit};
pub use resolve::{apply_matches, resolve, ResolveStats};
pub use sec_ua::{validate_user_agent, UserAgentError};

pub(crate) fn pad_cik(cik: &str) -> String {
    let digits: String = cik.chars().filter(|c| c.is_ascii_digit()).collect();
    format!("{digits:0>10}")
}
