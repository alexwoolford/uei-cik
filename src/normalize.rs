//! Shared name normalization for the exact UEI–CIK rule.

const SUFFIXES: &[&str] = &[
    "INC",
    "CORP",
    "LLC",
    "LTD",
    "CO",
    "COMPANY",
    "CORPORATION",
    "INCORPORATED",
    "LIMITED",
];

/// Uppercase, expand `&`, drop a trailing EDGAR tag, strip punctuation,
/// collapse whitespace, and drop trailing legal suffixes.
pub fn normalize_name(raw: &str) -> String {
    let mut s = raw.trim().to_ascii_uppercase().replace('&', " AND ");
    if let Some(stripped) = strip_edgar_tag(&s) {
        s = stripped;
    }
    let mut cleaned = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c.is_ascii_whitespace() {
            cleaned.push(c);
        }
    }
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    drop_suffixes(&collapsed)
}

fn strip_edgar_tag(s: &str) -> Option<String> {
    let t = s.trim_end();
    let bytes = t.as_bytes();
    let close = bytes.iter().rposition(|b| *b == b'/')?;
    if close + 1 != bytes.len() {
        return None;
    }
    let open = bytes[..close].iter().rposition(|b| *b == b'/')?;
    let tag = &t[open + 1..close];
    if tag.is_empty() || tag.len() > 4 || !tag.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(t[..open].trim_end().to_string())
}

fn drop_suffixes(s: &str) -> String {
    let mut words: Vec<&str> = s.split_whitespace().collect();
    while words.len() > 1 {
        let last = words[words.len() - 1];
        if SUFFIXES.contains(&last) {
            words.pop();
        } else {
            break;
        }
    }
    words.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lockheed_forms_meet() {
        assert_eq!(
            normalize_name("LOCKHEED MARTIN CORPORATION /DE/"),
            "LOCKHEED MARTIN"
        );
        assert_eq!(normalize_name("Lockheed Martin Corp."), "LOCKHEED MARTIN");
    }

    #[test]
    fn ampersand_and_suffix() {
        assert_eq!(normalize_name("AT&T INC."), "AT AND T");
        assert_eq!(normalize_name("OLD NAME CORPORATION"), "OLD NAME");
        assert_eq!(normalize_name("OLD NAME CORP"), "OLD NAME");
    }
}
