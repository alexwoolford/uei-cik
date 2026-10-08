//! SEC fair-access User-Agent. Fail closed.

use std::fmt;

/// Sample shape: `uei-cik you@real-domain`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserAgentError(pub String);

impl fmt::Display for UserAgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for UserAgentError {}

pub fn validate_user_agent(raw: &str) -> Result<String, UserAgentError> {
    let ua = raw.trim();
    if ua.is_empty() {
        return Err(UserAgentError(
            "SEC_USER_AGENT is required (sample: uei-cik you@real-domain)".into(),
        ));
    }
    let lower = ua.to_ascii_lowercase();
    if lower.contains("example.com") {
        return Err(UserAgentError(
            "Set SEC_USER_AGENT to a real descriptive contact (not example.com)".into(),
        ));
    }
    if ua.contains("://github.com") || (ua.contains('(') && ua.contains("github")) {
        return Err(UserAgentError(
            "Do not use a github-paren User-Agent; Akamai 403s it as an undeclared bot. Use: uei-cik you@real-domain".into(),
        ));
    }
    Ok(ua.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_sample_shape() {
        assert!(validate_user_agent("uei-cik you@real-domain").is_ok());
    }

    #[test]
    fn refuses_empty_example_and_github_paren() {
        assert!(validate_user_agent("").is_err());
        assert!(validate_user_agent("uei-cik contact@example.com").is_err());
        assert!(validate_user_agent(
            "uei-cik/0.1 (https://github.com/alexwoolford/uei-cik; you@real-domain)"
        )
        .is_err());
    }
}
