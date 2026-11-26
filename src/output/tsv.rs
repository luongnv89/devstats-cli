//! `--format tsv` and the plain form of `--format table`: one contributor per line,
//! tab-separated, no header, no escape codes.
//!
//! Fields: login, commits, additions, deletions, prs, prs_merged, issues, repos, orgs
//! (comma-separated; `owner/repo` for a repository target), prs_open, prs_closed

use crate::model::Report;
use crate::style::sanitize;

/// `--pr-weeks`: one line per contributor and week with at least one pull request:
/// login, week (Sunday start, YYYY-MM-DD), prs, merged, open, closed.
pub fn render_pr_weeks(report: &Report) -> String {
    let mut out = String::new();
    for c in &report.contributors {
        let login = sanitize(&c.login, true);
        for w in &c.pr_weeks {
            out.push_str(&format!(
                "{login}\t{}\t{}\t{}\t{}\t{}\n",
                w.week, w.prs, w.merged, w.open, w.closed
            ));
        }
    }
    out
}

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
            c.prs_open.to_string(),
            c.prs_closed.to_string(),
        ];
        out.push_str(&fields.join("\t"));
        out.push('\n');
    }
    out
}
