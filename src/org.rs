//! Turn user input (`https://github.com/Montimage`, `github.com/Montimage`,
//! `Montimage`) into a GitHub login.

use crate::error::{AppError, Result};

const HINT: &str = "use a URL like https://github.com/Montimage or a bare name like Montimage";

/// Extract and validate the organization (or user) login from one argument.
pub fn parse_org(input: &str) -> Result<String> {
    let raw = input.trim();
    let lower = raw.to_ascii_lowercase();
    let mut s = raw;
    for scheme in ["https://", "http://"] {
        if lower.starts_with(scheme) {
            s = &raw[scheme.len()..];
            break;
        }
    }
    let s = s.split(['?', '#']).next().unwrap_or("");
    let parts: Vec<&str> = s.split('/').filter(|p| !p.is_empty()).collect();
    let bad = || AppError::input(format!("'{}' is not a GitHub organization", printable(raw))).hint(HINT);
    let name = match parts.as_slice() {
        [] => return Err(bad()),
        [host] if host.contains('.') => {
            return Err(AppError::input(format!("'{}' has no organization in its path", printable(raw))).hint(HINT))
        }
        [host, org, ..] if host.contains('.') => *org,
        [first, ..] => *first,
    };
    let name = name.trim_start_matches('@');
    let valid = !name.is_empty()
        && name.len() <= 39
        && !name.starts_with('-')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if valid {
        Ok(name.to_string())
    } else {
        Err(bad())
    }
}

/// Parse every argument, dropping case-insensitive duplicates (first spelling wins).
pub fn parse_all(inputs: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for input in inputs {
        let org = parse_org(input)?;
        if !out.iter().any(|o| o.eq_ignore_ascii_case(&org)) {
            out.push(org);
        }
    }
    Ok(out)
}

fn printable(s: &str) -> String {
    crate::style::sanitize(s, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_common_forms() {
        for input in [
            "https://github.com/Montimage",
            "https://github.com/Montimage/",
            "http://github.com/Montimage",
            "HTTPS://GitHub.com/Montimage",
            "github.com/Montimage",
            "www.github.com/Montimage",
            "Montimage",
            "@Montimage",
            "  Montimage  ",
            "https://github.com/Montimage?tab=repositories",
            "https://github.com/Montimage/mmt-probe",
        ] {
            assert_eq!(parse_org(input).unwrap(), "Montimage", "input: {input}");
        }
        assert_eq!(parse_org("https://github.com/montimage-projects").unwrap(), "montimage-projects");
    }

    #[test]
    fn rejects_junk() {
        for input in ["", "   ", "https://github.com", "github.com/", "bad name", "-lead", "a_b", "x".repeat(40).as_str(), "héllo"] {
            let err = parse_org(input).unwrap_err();
            assert_eq!(err.exit_code(), 3, "input: {input}");
            assert!(err.hint.is_some());
        }
    }

    #[test]
    fn dedupes_case_insensitively() {
        let got = parse_all(&["Montimage".into(), "https://github.com/montimage".into(), "b".into()]).unwrap();
        assert_eq!(got, vec!["Montimage".to_string(), "b".to_string()]);
    }
}
