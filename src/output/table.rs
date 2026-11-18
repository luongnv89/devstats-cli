//! `--format table` on a styled stdout: summary box and contributors table (btop look).

use crate::model::{Contributor, Report};
use crate::style::components::{boxed, meter, sparkline, table, Col, Line};
use crate::style::theme::Role;
use crate::style::{sanitize, truncate, width, Mode};

pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `9,999`, `412.9` + `K`, `1.92` + `M`: the number and its unit (drawn dim).
pub fn compact(n: u64) -> (String, &'static str) {
    match n {
        0..=9_999 => (thousands(n), ""),
        10_000..=999_949 => (format!("{:.1}", n as f64 / 1e3), "K"),
        _ => (format!("{:.2}", n as f64 / 1e6), "M"),
    }
}

fn num(mode: &Mode, n: u64) -> Line {
    let (v, unit) = compact(n);
    let mut l = Line::of(mode, Role::Text, &v);
    l.push(mode, Role::Dim, unit);
    l
}

fn sep(mode: &Mode) -> &'static str {
    if mode.unicode() {
        " · "
    } else {
        " - "
    }
}

pub fn range_label(r: &Report, mode: &Mode) -> String {
    match (&r.range.since, &r.range.until) {
        (None, None) => "all time".into(),
        (Some(s), None) => format!("since {s}"),
        (None, Some(u)) => format!("until {u}"),
        (Some(s), Some(u)) => format!("{s} {} {u}", mode.g("→", "to")),
    }
}

struct Section {
    title: String,
    footer: String,
    lines: Vec<Line>,
    border: u8,
}

pub fn render(r: &Report, mode: &Mode, per_org: bool) -> String {
    let w = mode.width;
    let shown = r.contributors.len();
    let label = r.sort.label();
    let title = if shown < r.totals.contributors {
        format!("contributors{}top {shown} by {label}", sep(mode))
    } else {
        format!("contributors{}{shown} by {label}", sep(mode))
    };
    let mut footer = format!("{} contributors{}sorted by {}", thousands(r.totals.contributors as u64), sep(mode), label);
    if shown < r.totals.contributors {
        footer.push_str(&format!("{}--top 0 shows all", sep(mode)));
    }
    let mut sections = vec![Section { lines: contributor_lines(mode, &r.contributors, w, r.sort), title, footer, border: 0 }];
    if per_org {
        for (i, org) in r.orgs.iter().enumerate() {
            let shown = org.contributors.len();
            sections.push(Section {
                title: format!("{}{}top {shown} by {label}", sanitize(&org.login, false), sep(mode)),
                footer: format!(
                    "{} contributors{}{} repos{}{} commits",
                    thousands(org.totals.contributors as u64),
                    sep(mode),
                    org.totals.repos,
                    sep(mode),
                    thousands(org.totals.commits)
                ),
                lines: contributor_lines(mode, &org.contributors, w, r.sort),
                border: ((i + 1) % 4) as u8,
            });
        }
    }
    let (summary_title, summary_items, summary_footer) = summary_parts(r, mode);
    // One box width for every section: wide enough for the content, never wider than the terminal.
    let one_line: usize = summary_items.iter().map(|i| i.width).sum::<usize>() + 3 * summary_items.len().saturating_sub(1);
    let natural = sections
        .iter()
        .flat_map(|s| s.lines.iter().map(|l| l.width + 4).chain([width(&s.title) + 6, width(&s.footer) + 6]))
        .chain([one_line + 4, width(&summary_title) + 6, width(&summary_footer) + 6])
        .max()
        .unwrap_or(w);
    let bw = natural.min(w);
    let mut out = boxed(mode, &summary_title, &pack(summary_items, bw.saturating_sub(4)), &summary_footer, bw, 0);
    for s in sections {
        out.push_str(&boxed(mode, &s.title, &s.lines, &s.footer, bw, s.border));
    }
    out
}

/// Pack items into as few rows as fit in `inner` cells, three spaces apart.
fn pack(items: Vec<Line>, inner: usize) -> Vec<Line> {
    let mut rows: Vec<Line> = vec![Line::new()];
    for item in items {
        let cur = rows.last_mut().unwrap();
        if cur.width > 0 && cur.width + 3 + item.width > inner {
            rows.push(Line::new());
        }
        let cur = rows.last_mut().unwrap();
        if cur.width > 0 {
            cur.pad(3);
        }
        cur.append(&item);
    }
    rows
}

fn summary_parts(r: &Report, mode: &Mode) -> (String, Vec<Line>, String) {
    let t = &r.totals;
    let minus = mode.g("−", "-");
    let stats: Vec<(String, String)> = vec![
        (r.orgs.len().to_string(), if r.orgs.len() == 1 { "org" } else { "orgs" }.into()),
        (thousands(t.repos as u64), "repos".into()),
        (thousands(t.contributors as u64), "contributors".into()),
        (thousands(t.commits), "commits".into()),
        (format!("+{} / {minus}{}", join(compact(t.additions)), join(compact(t.deletions))), "lines".into()),
        (thousands(t.prs), format!("PRs ({} merged)", thousands(t.prs_merged))),
        (thousands(t.issues), "issues".into()),
    ];
    let items: Vec<Line> = stats
        .into_iter()
        .map(|(value, label)| {
            let mut item = Line::of(mode, Role::Title, &value);
            item.pad(1).push(mode, Role::Dim, &label);
            item
        })
        .collect();
    let names: Vec<String> = r.org_names().iter().map(|o| sanitize(o, false)).collect();
    let title = format!("devstats{}{}", sep(mode), names.join(", "));
    let mut footer = format!("{}{}{}", &r.generated_at[..10.min(r.generated_at.len())], sep(mode), range_label(r, mode));
    let incomplete: usize = r.orgs.iter().map(|o| o.incomplete_repos.len()).sum();
    if incomplete > 0 {
        footer.push_str(&format!("{}{incomplete} repos still computing", sep(mode)));
    }
    (title, items, footer)
}

fn join((v, u): (String, &'static str)) -> String {
    format!("{v}{u}")
}

fn contributor_lines(mode: &Mode, list: &[Contributor], w: usize, sort: crate::cli::SortKey) -> Vec<Line> {
    if list.is_empty() {
        return vec![Line::of(mode, Role::Dim, "no contributions found in this range")];
    }
    let minus = mode.g("−", "-");
    // Drop order when narrow (highest first): activity, merged, repos, share, issues, -lines, +lines.
    let cols = vec![
        Col::new("#", true, 0),
        Col::new("contributor", false, 0).shrinkable(),
        Col::new("commits", true, 0),
        Col::new("+lines", true, 1),
        Col::new(&format!("{minus}lines"), true, 2),
        Col::new("PRs", true, 0),
        Col::new("merged", true, 6),
        Col::new("issues", true, 3),
        Col::new("repos", true, 5),
        Col::new("share", false, 4),
        Col::new("activity 52w", false, 7),
    ];
    let logins: Vec<String> = list.iter().map(|c| sanitize(&c.login, false)).collect();
    let rows: Vec<Vec<Line>> = list
        .iter()
        .zip(&logins)
        .enumerate()
        .map(|(i, (c, login))| {
            let peak = c.activity.iter().copied().max().unwrap_or(0);
            let mut activity = sparkline(mode, &c.activity, None);
            if activity.width > 0 {
                activity.pad(1);
            }
            activity.push(mode, Role::Dim, &format!("{}/wk", thousands(peak)));
            let key_role = |k: crate::cli::SortKey| if k == sort { Role::Title } else { Role::Text };
            vec![
                Line::of(mode, Role::Dim, &(i + 1).to_string()),
                Line::of(mode, Role::Text, login),
                Line::of(mode, key_role(crate::cli::SortKey::Commits), &thousands(c.commits)),
                num(mode, c.additions),
                num(mode, c.deletions),
                Line::of(mode, key_role(crate::cli::SortKey::Prs), &thousands(c.prs)),
                Line::of(mode, Role::Text, &thousands(c.prs_merged)),
                Line::of(mode, key_role(crate::cli::SortKey::Issues), &thousands(c.issues)),
                Line::of(mode, key_role(crate::cli::SortKey::Repos), &thousands(c.repos as u64)),
                meter(mode, c.share, 10),
                activity,
            ]
        })
        .collect();
    let shrink = |r: usize, _c: usize, width: usize| Line::of(mode, Role::Text, &truncate(&logins[r], width, mode.unicode()));
    table(mode, &cols, &rows, w.saturating_sub(4), &shrink)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_formats() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(1234567), "1,234,567");
        assert_eq!(compact(9_999), ("9,999".into(), ""));
        assert_eq!(compact(412_889), ("412.9".into(), "K"));
        assert_eq!(compact(1_920_000), ("1.92".into(), "M"));
        assert_eq!(compact(999_960), ("1.00".into(), "M"));
    }
}
