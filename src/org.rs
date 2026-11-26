//! Turn user input (`https://github.com/Montimage`, `github.com/Montimage`, `Montimage`,
//! `https://github.com/Montimage/mmt-probe`, `Montimage/mmt-probe`) into a scan target:
//! a whole organization (or user), or one repository.

use crate::error::{AppError, Result};

const HINT: &str = "use an organization like https://github.com/Montimage or Montimage, or a repository like https://github.com/Montimage/mmt-probe or Montimage/mmt-probe";

/// What to scan: every repository of `owner`, or only `owner/repo`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub owner: String,
    pub repo: Option<String>,
}

impl Target {
    /// `owner` or `owner/repo`.
    pub fn label(&self) -> String {
        match &self.repo {
            Some(r) => format!("{}/{r}", self.owner),
            None => self.owner.clone(),
        }
    }
}

/// Extract and validate the organization (or user) login from one argument,
/// ignoring any repository in it.
pub fn parse_org(input: &str) -> Result<String> {
    parse_target(input).map(|t| t.owner)
}

/// Parse one argument into an organization or repository target. A URL or
/// `owner/repo` path with a second segment names a repository; anything after
/// it (`/pulls`, `/tree/main`) is ignored, and a `.git` suffix is dropped.
pub fn parse_target(input: &str) -> Result<Target> {
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
    let bad = || {
        AppError::input(format!(
            "'{}' is not a GitHub organization or repository",
            printable(raw)
        ))
        .hint(HINT)
    };
    let (name, repo) = match parts.as_slice() {
        [] => return Err(bad()),
        [host] if host.contains('.') => {
            return Err(AppError::input(format!(
                "'{}' has no organization in its path",
                printable(raw)
            ))
            .hint(HINT))
        }
        [host, org, rest @ ..] if host.contains('.') => (*org, rest.first().copied()),
        [first, rest @ ..] => (*first, rest.first().copied()),
    };
    let name = name.trim_start_matches('@');
    let valid = !name.is_empty()
        && name.len() <= 39
        && !name.starts_with('-')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if !valid {
        return Err(bad());
    }
    let repo = match repo {
        None => None,
        Some(r) => {
            let r = r.strip_suffix(".git").unwrap_or(r);
            let ok = !r.is_empty()
                && r.len() <= 100
                && r != "."
                && r != ".."
                && r.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
            if !ok {
                return Err(bad());
            }
            Some(r.to_string())
        }
    };
    Ok(Target {
        owner: name.to_string(),
        repo,
    })
}

/// Parse every argument, dropping case-insensitive duplicates (first spelling wins).
pub fn parse_all(inputs: &[String]) -> Result<Vec<Target>> {
    let mut out: Vec<Target> = Vec::new();
    for input in inputs {
        let t = parse_target(input)?;
        if !out
            .iter()
            .any(|o| o.label().eq_ignore_ascii_case(&t.label()))
        {
            out.push(t);
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
        ] {
            assert_eq!(parse_org(input).unwrap(), "Montimage", "input: {input}");
        }
        assert_eq!(
            parse_org("https://github.com/montimage-projects").unwrap(),
            "montimage-projects"
        );
    }

    #[test]
    fn repository_urls_and_paths_are_repo_targets() {
        for input in [
            "https://github.com/Montimage/mmt-probe",
            "https://github.com/Montimage/mmt-probe/",
            "https://github.com/Montimage/mmt-probe.git",
            "https://github.com/Montimage/mmt-probe/pulls?q=is:open",
            "https://github.com/Montimage/mmt-probe/tree/main/src",
            "github.com/Montimage/mmt-probe",
            "Montimage/mmt-probe",
        ] {
            let t = parse_target(input).unwrap();
            assert_eq!(
                (t.owner.as_str(), t.repo.as_deref()),
                ("Montimage", Some("mmt-probe")),
                "input: {input}"
            );
        }
        let t = parse_target("socketio/socket.io_v2").unwrap();
        assert_eq!(t.repo.as_deref(), Some("socket.io_v2"));
        assert_eq!(t.label(), "socketio/socket.io_v2");
        assert_eq!(parse_target("Montimage").unwrap().repo, None);
        assert_eq!(parse_org("Montimage/mmt-probe").unwrap(), "Montimage");
        for junk in ["acme/..", "acme/.", "acme/a b", "acme/.git", "acme/héllo"] {
            assert!(parse_target(junk).is_err(), "input: {junk}");
        }
    }

    #[test]
    fn rejects_junk() {
        for input in [
            "",
            "   ",
            "https://github.com",
            "github.com/",
            "bad name",
            "-lead",
            "a_b",
            "x".repeat(40).as_str(),
            "héllo",
        ] {
            let err = parse_org(input).unwrap_err();
            assert_eq!(err.exit_code(), 3, "input: {input}");
            assert!(err.hint.is_some());
        }
    }

    #[test]
    fn dedupes_case_insensitively() {
        let got = parse_all(&[
            "Montimage".into(),
            "https://github.com/montimage".into(),
            "b".into(),
        ])
        .unwrap();
        let labels: Vec<String> = got.iter().map(Target::label).collect();
        assert_eq!(labels, ["Montimage", "b"]);
        let got = parse_all(&["acme/API".into(), "acme/api".into(), "acme".into()]).unwrap();
        let labels: Vec<String> = got.iter().map(Target::label).collect();
        assert_eq!(labels, ["acme/API", "acme"]);
    }
}
