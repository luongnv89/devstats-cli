//! `--format tsv` and the plain form of `--format table`: one contributor per line,
//! tab-separated, no header, no escape codes.
//!
//! Fields: login, commits, additions, deletions, prs, prs_merged, issues, repos, orgs (comma-separated)

use crate::model::Report;
use crate::style::sanitize;

pub fn render(report: &Report) -> String {
    let mut out = String::new();
    for c in &report.contributors {
        let fields = [
            sanitize(&c.login, true),
            c.commits.to_string(),
            c.additions.to_string(),
            c.deletions.to_string(),
            c.prs.to_string(),
            c.prs_merged.to_string(),
            c.issues.to_string(),
            c.repos.to_string(),
            c.orgs
                .iter()
                .map(|o| sanitize(o, true))
                .collect::<Vec<_>>()
                .join(","),
        ];
        out.push_str(&fields.join("\t"));
        out.push('\n');
    }
    out
}
