//! `--format table` on a styled stdout: summary box and contributors table (btop look).

use crate::model::{Contributor, PrivateAccess, Report};
use crate::style::components::{boxed, meter_with, sparkline, table, Col, Line};
use crate::style::theme::Role;
use crate::style::{sanitize, truncate, width, Mode};

pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
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

fn num(mode: &Mode, role: Role, n: u64) -> Line {
    let (v, unit) = compact(n);
    let mut l = Line::of(mode, role, &v);
    l.push(mode, Role::Dim, unit);
    l
}

/// Which values lead their column: every value equal to the highest one, so ties
/// all lead. Nothing leads in a column of zeros or in a table of one row, where
/// there is no other value to stand out from.
fn leaders(values: impl IntoIterator<Item = u64>) -> Vec<bool> {
    let values: Vec<u64> = values.into_iter().collect();
    let max = values.iter().copied().max().unwrap_or(0);
    let on = max > 0 && values.len() > 1;
    values.iter().map(|&v| on && v == max).collect()
}

fn sep(mode: &Mode) -> &'static str {
    if mode.unicode() {
        " · "
    } else {
        " - "
    }
}

pub fn range_label(r: &Report, mode: &Mode) -> String {
    let dates = match (&r.range.since, &r.range.until) {
        (None, None) => "all time".into(),
        (Some(s), None) => format!("since {s}"),
        (None, Some(u)) => format!("until {u}"),
        (Some(s), Some(u)) => format!("{s} {} {u}", mode.g("→", "to")),
    };
    match r.range.period_label {
        Some(label) if label != dates => format!("{label} ({dates})"),
        _ => dates,
    }
}

struct Section {
    title: String,
    footer: String,
    lines: Vec<Line>,
    border: u8,
}

pub fn render(r: &Report, mode: &Mode, per_org: bool, pr_weeks: bool) -> String {
    let w = mode.width;
    let shown = r.contributors.len();
    let label = r.sort.label();
    let title = if shown < r.totals.contributors {
        format!("contributors{}top {shown} by {label}", sep(mode))
    } else {
        format!("contributors{}{shown} by {label}", sep(mode))
    };
    let mut footer = format!(
        "{} contributors{}sorted by {}",
        thousands(r.totals.contributors as u64),
        sep(mode),
        label
    );
    if shown < r.totals.contributors {
        footer.push_str(&format!("{}--top 0 shows all", sep(mode)));
    }
    let mut sections = vec![Section {
        lines: contributor_lines(mode, &r.contributors, w, r.sort),
        title,
        footer,
        border: 0,
    }];
    if per_org {
        for (i, org) in r.orgs.iter().enumerate() {
            let shown = org.contributors.len();
            sections.push(Section {
                title: format!(
                    "{}{}top {shown} by {label}",
                    sanitize(&org.login, false),
                    sep(mode)
                ),
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
    if pr_weeks {
        let rows: usize = r.contributors.iter().map(|c| c.pr_weeks.len()).sum();
        sections.push(Section {
            title: format!(
                "pull requests per week{}{} rows",
                sep(mode),
                thousands(rows as u64)
            ),
            footer: format!(
                "weeks start on Sunday{}PRs by creation week, state as of today",
                sep(mode)
            ),
            lines: pr_week_lines(mode, &r.contributors, w),
            border: 1,
        });
    }
    let (summary_title, summary_items, summary_footer) = summary_parts(r, mode);
    // One box width for every section: wide enough for the content, never wider than the terminal.
    let one_line: usize = summary_items.iter().map(|i| i.width).sum::<usize>()
        + 3 * summary_items.len().saturating_sub(1);
    let natural = sections
        .iter()
        .flat_map(|s| {
            s.lines
                .iter()
                .map(|l| l.width + 4)
                .chain([width(&s.title) + 6, width(&s.footer) + 6])
        })
        .chain([
            one_line + 4,
            width(&summary_title) + 6,
            width(&summary_footer) + 6,
        ])
        .max()
        .unwrap_or(w);
    let bw = natural.min(w);
    let mut out = boxed(
        mode,
        &summary_title,
        &pack(summary_items, bw.saturating_sub(4)),
        &summary_footer,
        bw,
        0,
    );
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
    let owners = r.orgs.iter().filter(|o| o.kind != "repository").count();
    let repos_word = if t.repos == 1 { "repo" } else { "repos" };
    let mut stats: Vec<(String, String)> = Vec::new();
    if owners > 0 {
        stats.push((
            owners.to_string(),
            if owners == 1 { "org" } else { "orgs" }.into(),
        ));
    }
    stats.extend([
        (
            thousands(t.repos as u64),
            if t.repos_private > 0 {
                format!(
                    "{repos_word} ({} private)",
                    thousands(t.repos_private as u64)
                )
            } else {
                repos_word.into()
            },
        ),
        (thousands(t.contributors as u64), "contributors".into()),
        (thousands(t.commits), "commits".into()),
        (
            format!(
                "+{} / {minus}{}",
                join(compact(t.additions)),
                join(compact(t.deletions))
            ),
            "lines".into(),
        ),
        (
            thousands(t.prs),
            format!(
                "PRs ({} merged, {} open, {} closed)",
                thousands(t.prs_merged),
                thousands(t.prs_open),
                thousands(t.prs_closed)
            ),
        ),
        (thousands(t.issues), "issues".into()),
    ]);
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
    let date = &r.generated_at[..10.min(r.generated_at.len())];
    let mut rest = range_label(r, mode);
    let incomplete: usize = r.orgs.iter().map(|o| o.incomplete_repos.len()).sum();
    if incomplete > 0 {
        rest.push_str(&format!("{}{incomplete} repos still computing", sep(mode)));
    }
    let public_only: Vec<String> = r
        .orgs
        .iter()
        .filter(|o| {
            !matches!(
                o.private_access,
                PrivateAccess::Included | PrivateAccess::Repository
            )
        })
        .map(|o| sanitize(&o.login, false))
        .collect();
    let footer = if public_only.len() == r.orgs.len() && !public_only.is_empty() {
        let all = "public repos only";
        fit_footer(mode, date, &rest, all, all)
    } else if !public_only.is_empty() {
        let named = format!("private repos not included for {}", public_only.join(", "));
        let counted = format!(
            "private repos not included for {} of {} orgs",
            public_only.len(),
            r.orgs.len()
        );
        fit_footer(mode, date, &rest, &named, &counted)
    } else {
        format!("{date}{}{rest}", sep(mode))
    };
    (title, items, footer)
}

/// A summary footer ending in a private-repo disclosure (`long`, or the shorter
/// `short`). The box truncates its footer to the terminal width minus 6, so the
/// disclosure must stay visible: use `long` if it fits, else `short`, then drop the
/// date, and as a last resort put the disclosure first.
fn fit_footer(mode: &Mode, date: &str, rest: &str, long: &str, short: &str) -> String {
    let max = mode.width.saturating_sub(6);
    let s = sep(mode);
    [
        format!("{date}{s}{rest}{s}{long}"),
        format!("{date}{s}{rest}{s}{short}"),
        format!("{rest}{s}{short}"),
    ]
    .into_iter()
    .find(|f| width(f) <= max)
    .unwrap_or_else(|| format!("{short}{s}{rest}"))
}

fn join((v, u): (String, &'static str)) -> String {
    format!("{v}{u}")
}

fn contributor_lines(
    mode: &Mode,
    list: &[Contributor],
    w: usize,
    sort: crate::cli::SortKey,
) -> Vec<Line> {
    if list.is_empty() {
        return vec![Line::of(
            mode,
            Role::Dim,
            "no contributions found in this range",
        )];
    }
    let minus = mode.g("−", "-");
    // Drop order when narrow (highest first): activity, repos, share, closed, open, merged, issues, -lines, +lines.
    let cols = vec![
        Col::new("#", true, 0),
        Col::new("contributor", false, 0).shrinkable(),
        Col::new("commits", true, 0),
        Col::new("+lines", true, 1),
        Col::new(&format!("{minus}lines"), true, 2),
        Col::new("PRs", true, 0),
        Col::new("merged", true, 4),
        Col::new("open", true, 5),
        Col::new("closed", true, 6),
        Col::new("issues", true, 3),
        Col::new("repos", true, 8),
        Col::new("share", false, 7),
        Col::new("activity 52w", false, 9),
    ];
    let logins: Vec<String> = list.iter().map(|c| sanitize(&c.login, false)).collect();
    let peaks: Vec<u64> = list
        .iter()
        .map(|c| c.activity.iter().copied().max().unwrap_or(0))
        .collect();
    // The highest value of each metric column is drawn in `Role::Lead` (per box).
    // Share is commits over the same total, so it leads exactly where commits do.
    let lead_commits = leaders(list.iter().map(|c| c.commits));
    let lead_additions = leaders(list.iter().map(|c| c.additions));
    let lead_deletions = leaders(list.iter().map(|c| c.deletions));
    let lead_prs = leaders(list.iter().map(|c| c.prs));
    let lead_merged = leaders(list.iter().map(|c| c.prs_merged));
    let lead_open = leaders(list.iter().map(|c| c.prs_open));
    let lead_closed = leaders(list.iter().map(|c| c.prs_closed));
    let lead_issues = leaders(list.iter().map(|c| c.issues));
    let lead_repos = leaders(list.iter().map(|c| c.repos as u64));
    let lead_peak = leaders(peaks.iter().copied());
    let rows: Vec<Vec<Line>> = list
        .iter()
        .zip(&logins)
        .enumerate()
        .map(|(i, (c, login))| {
            let role = |lead: &[bool], base: Role| if lead[i] { Role::Lead } else { base };
            let key_role = |lead: &[bool], k: crate::cli::SortKey| {
                role(lead, if k == sort { Role::Title } else { Role::Text })
            };
            let mut activity = sparkline(mode, &c.activity, None);
            if activity.width > 0 {
                activity.pad(1);
            }
            activity.push(
                mode,
                role(&lead_peak, Role::Dim),
                &format!("{}/wk", thousands(peaks[i])),
            );
            vec![
                Line::of(mode, Role::Dim, &(i + 1).to_string()),
                Line::of(mode, Role::Text, login),
                Line::of(
                    mode,
                    key_role(&lead_commits, crate::cli::SortKey::Commits),
                    &thousands(c.commits),
                ),
                num(mode, role(&lead_additions, Role::Text), c.additions),
                num(mode, role(&lead_deletions, Role::Text), c.deletions),
                Line::of(
                    mode,
                    key_role(&lead_prs, crate::cli::SortKey::Prs),
                    &thousands(c.prs),
                ),
                Line::of(
                    mode,
                    role(&lead_merged, Role::Text),
                    &thousands(c.prs_merged),
                ),
                Line::of(mode, role(&lead_open, Role::Text), &thousands(c.prs_open)),
                Line::of(
                    mode,
                    role(&lead_closed, Role::Text),
                    &thousands(c.prs_closed),
                ),
                Line::of(
                    mode,
                    key_role(&lead_issues, crate::cli::SortKey::Issues),
                    &thousands(c.issues),
                ),
                Line::of(
                    mode,
                    key_role(&lead_repos, crate::cli::SortKey::Repos),
                    &thousands(c.repos as u64),
                ),
                meter_with(mode, c.share, 10, role(&lead_commits, Role::Text)),
                activity,
            ]
        })
        .collect();
    let shrink = |r: usize, _c: usize, width: usize| {
        Line::of(
            mode,
            Role::Text,
            &truncate(&logins[r], width, mode.unicode()),
        )
    };
    table(mode, &cols, &rows, w.saturating_sub(4), &shrink)
}

/// `--pr-weeks`: each contributor's weeks with pull requests, grouped by contributor
/// (in ranking order), oldest week first, with a bar scaled to the busiest week.
fn pr_week_lines(mode: &Mode, list: &[Contributor], w: usize) -> Vec<Line> {
    let with_prs: Vec<&Contributor> = list.iter().filter(|c| !c.pr_weeks.is_empty()).collect();
    if with_prs.is_empty() {
        return vec![Line::of(mode, Role::Dim, "no pull requests in this range")];
    }
    let cols = vec![
        Col::new("contributor", false, 0).shrinkable(),
        Col::new("week", false, 0),
        Col::new("PRs", true, 0),
        Col::new("merged", true, 3),
        Col::new("open", true, 2),
        Col::new("closed", true, 1),
        Col::new("", false, 4),
    ];
    let max = with_prs
        .iter()
        .flat_map(|c| c.pr_weeks.iter().map(|w| w.prs))
        .max()
        .unwrap_or(1)
        .max(1);
    let bar = mode.g("■", "#");
    let mut logins: Vec<String> = Vec::new();
    let mut rows: Vec<Vec<Line>> = Vec::new();
    for c in with_prs {
        let login = sanitize(&c.login, false);
        for (i, wk) in c.pr_weeks.iter().enumerate() {
            let cells = ((wk.prs as f64 / max as f64) * 10.0).ceil() as usize;
            let zero = |n: u64| if n == 0 { Role::Dim } else { Role::Text };
            rows.push(vec![
                if i == 0 {
                    Line::of(mode, Role::Text, &login)
                } else {
                    Line::new()
                },
                Line::of(mode, Role::Dim, &wk.week),
                Line::of(mode, Role::Title, &thousands(wk.prs)),
                Line::of(mode, zero(wk.merged), &thousands(wk.merged)),
                Line::of(mode, zero(wk.open), &thousands(wk.open)),
                Line::of(mode, zero(wk.closed), &thousands(wk.closed)),
                Line::of(mode, Role::Text, &bar.repeat(cells.max(1))),
            ]);
            logins.push(if i == 0 { login.clone() } else { String::new() });
        }
    }
    let shrink = |r: usize, _c: usize, width: usize| {
        Line::of(
            mode,
            Role::Text,
            &truncate(&logins[r], width, mode.unicode()),
        )
    };
    table(mode, &cols, &rows, w.saturating_sub(4), &shrink)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::SortKey;
    use crate::model::{OrgReport, Range, Totals};
    use crate::style::theme::paint;
    use crate::style::{Glyphs, Tier};

    #[test]
    fn number_formats() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(1234567), "1,234,567");
        assert_eq!(compact(9_999), ("9,999".into(), ""));
        assert_eq!(compact(412_889), ("412.9".into(), "K"));
        assert_eq!(compact(1_920_000), ("1.92".into(), "M"));
        assert_eq!(compact(999_960), ("1.00".into(), "M"));
    }

    fn org(login: &str, access: PrivateAccess) -> OrgReport {
        OrgReport {
            login: login.into(),
            name: None,
            kind: "Organization",
            avatar_url: String::new(),
            html_url: String::new(),
            totals: Totals::default(),
            private_repos: 0,
            private_access: access,
            incomplete_repos: vec![],
            contributors: vec![],
            weekly: vec![],
        }
    }

    fn report(orgs: Vec<OrgReport>) -> Report {
        Report {
            tool: "devstats",
            version: "0",
            generated_at: "2026-11-18T10:00:00Z".into(),
            range: Range {
                period: None,
                period_label: None,
                since: Some("2025-01-01".into()),
                until: Some("2025-12-31".into()),
                activity_from: "2025-01-05".into(),
            },
            sort: SortKey::Commits,
            top: 0,
            totals: Totals::default(),
            orgs,
            contributors: vec![],
            weekly: vec![],
            notes: vec![],
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn person(
        login: &str,
        commits: u64,
        additions: u64,
        deletions: u64,
        prs: u64,
        repos: usize,
        share: f64,
        peak: u64,
    ) -> Contributor {
        Contributor {
            login: login.into(),
            html_url: String::new(),
            avatar_url: String::new(),
            commits,
            additions,
            deletions,
            prs,
            prs_merged: 0,
            prs_open: 0,
            prs_closed: 0,
            pr_weeks: vec![],
            issues: 0,
            repos,
            orgs: vec![],
            share,
            activity: vec![0, peak],
        }
    }

    #[test]
    fn pr_weeks_section_groups_weeks_under_each_contributor() {
        use crate::model::PrWeek;
        let wk = |week: &str, prs, merged, open, closed| PrWeek {
            week: week.into(),
            prs,
            merged,
            open,
            closed,
        };
        let mut ana = person("ana", 4, 0, 0, 3, 1, 50.0, 1);
        ana.pr_weeks = vec![wk("2025-03-02", 1, 1, 0, 0), wk("2025-03-09", 2, 0, 1, 1)];
        let mut r = report(vec![org("acme", PrivateAccess::Included)]);
        r.contributors = vec![ana, person("bo", 4, 0, 0, 0, 1, 50.0, 1)];
        let mode = Mode::styled(Tier::None, Glyphs::Unicode, 100);
        let without = render(&r, &mode, false, false);
        assert!(!without.contains("pull requests per week"));
        let out = render(&r, &mode, false, true);
        assert!(out.contains("pull requests per week · 2 rows"), "{out}");
        let rows: Vec<&str> = out.lines().filter(|l| l.contains("2025-03-")).collect();
        assert_eq!(rows.len(), 2, "{out}");
        assert!(
            rows[0].contains("ana") && rows[0].contains("■■■■■"),
            "{out}"
        );
        assert!(
            !rows[1].contains("ana"),
            "login only on the first week: {out}"
        );
        assert!(rows[1].contains("■■■■■■■■■■"), "{out}");
        assert!(!out
            .lines()
            .any(|l| l.contains(" bo ") && l.contains("2025-")));
        r.contributors[0].pr_weeks.clear();
        let out = render(&r, &mode, false, true);
        assert!(out.contains("no pull requests in this range"), "{out}");
    }

    fn sample() -> Vec<Contributor> {
        vec![
            person("ana", 40, 1_000, 9, 3, 2, 44.4, 7),
            person("bo", 40, 200, 9, 1, 1, 44.4, 3),
            person("cy", 10, 1_000, 2, 1, 1, 11.1, 2),
        ]
    }

    #[test]
    fn the_highest_value_of_each_metric_column_leads() {
        let m = Mode::styled(Tier::Ansi16, Glyphs::Unicode, 200);
        let lead = |s: &str| paint(&m, Role::Lead, s);
        let lines = contributor_lines(&m, &sample(), 200, SortKey::Commits);
        let rows: Vec<&str> = lines[1..].iter().map(|l| l.text.as_str()).collect();
        let (ana, bo, cy) = (rows[0], rows[1], rows[2]);
        // commits: a tie at 40 leads in both rows, and the sort column's other values stay bold.
        assert!(
            ana.contains(&lead("40")) && bo.contains(&lead("40")),
            "{ana}\n{bo}"
        );
        assert!(cy.contains(&paint(&m, Role::Title, "10")), "{cy}");
        assert!(!cy.contains(&lead("10")), "{cy}");
        // +lines tie at 1,000; −lines and PRs lead in one place each.
        assert!(ana.contains(&lead("1,000")) && cy.contains(&lead("1,000")));
        assert!(!bo.contains(&lead("200")), "{bo}");
        assert!(ana.contains(&lead("9")) && bo.contains(&lead("9")));
        assert!(!cy.contains(&lead("2")), "{cy}");
        assert!(ana.contains(&lead("3")) && !bo.contains(&lead("1")));
        // repos, share (follows commits) and the activity peak.
        assert!(ana.contains(&lead("2")));
        assert!(ana.contains(&lead(" 44%")) && bo.contains(&lead(" 44%")));
        assert!(!cy.contains(&lead(" 11%")), "{cy}");
        assert!(ana.contains(&lead("7/wk")) && !bo.contains(&lead("3/wk")));
        // merged and issues are all zero: nothing leads there.
        assert!(rows.iter().all(|r| !r.contains(&lead("0"))), "{rows:?}");
        // Rank and contributor name are never highlighted.
        for (i, (row, name)) in rows.iter().zip(["ana", "bo", "cy"]).enumerate() {
            assert!(
                row.starts_with(&paint(&m, Role::Dim, &(i + 1).to_string())),
                "{row}"
            );
            assert!(!row.contains(&lead(name)), "{row}");
        }
        // Exactly the leading cells: ana 7 (all but merged/issues), bo 3, cy 1 (+lines).
        let count = |r: &str| r.matches("\x1b[96m").count();
        assert_eq!((count(ana), count(bo), count(cy)), (7, 3, 1), "{rows:?}");
        // The header row is never highlighted.
        assert!(!lines[0].text.contains("\x1b[96m"), "{}", lines[0].text);
    }

    #[test]
    fn no_highlight_without_color_or_for_a_single_row() {
        let none = Mode::styled(Tier::None, Glyphs::Unicode, 200);
        for l in contributor_lines(&none, &sample(), 200, SortKey::Commits) {
            assert!(!l.text.contains('\x1b'), "{}", l.text);
        }
        let m = Mode::styled(Tier::Ansi16, Glyphs::Unicode, 200);
        let one = &sample()[..1];
        for l in contributor_lines(&m, one, 200, SortKey::Commits) {
            assert!(!l.text.contains("\x1b[96m"), "{}", l.text);
        }
    }

    #[test]
    fn leaders_include_ties_and_skip_zero_columns() {
        assert_eq!(leaders([3, 5, 5, 1]), [false, true, true, false]);
        assert_eq!(leaders([0, 0]), [false, false]);
        assert_eq!(leaders([4]), [false]);
        assert_eq!(leaders([]), Vec::<bool>::new());
    }

    fn footer_line(out: &str) -> &str {
        out.lines().find(|l| l.starts_with("╰")).unwrap()
    }

    #[test]
    fn private_disclosure_survives_a_narrow_terminal() {
        use PrivateAccess::*;
        let mode = Mode::styled(Tier::None, Glyphs::Unicode, 80);
        let r = report(vec![
            org("acme-platform-engineering", Included),
            org("acme-security-research", ListingFailed),
        ]);
        let out = render(&r, &mode, false, false);
        let footer = footer_line(&out);
        assert!(width(footer) <= 80, "{out}");
        assert!(!footer.contains('…'), "{footer}");
        assert!(
            footer.contains("private repos not included for 1 of 2 orgs"),
            "{footer}"
        );
        assert!(footer.contains("2025-01-01 → 2025-12-31"), "{footer}");

        let mut r = r;
        r.orgs[0].incomplete_repos = vec!["a".into(), "b".into()];
        let out = render(&r, &mode, false, false);
        let footer = footer_line(&out);
        assert!(
            footer.contains("private repos not included for 1 of 2 orgs"),
            "{footer}"
        );

        let wide = Mode::styled(Tier::None, Glyphs::Unicode, 200);
        let out = render(&r, &wide, false, false);
        assert!(
            footer_line(&out).contains(
                "2026-11-18 · 2025-01-01 → 2025-12-31 · 2 repos still computing · \
                 private repos not included for acme-security-research"
            ),
            "{out}"
        );
    }

    #[test]
    fn footer_shows_the_period_and_keeps_the_disclosure() {
        let mut r = report(vec![org("acme", PrivateAccess::NoToken)]);
        r.range.period = Some(crate::cli::Period::Year);
        r.range.period_label = Some("last 1 year");
        r.range.since = Some("2025-11-18".into());
        r.range.until = None;
        let wide = Mode::styled(Tier::None, Glyphs::Unicode, 200);
        assert!(
            footer_line(&render(&r, &wide, false, false))
                .contains("2026-11-18 · last 1 year (since 2025-11-18) · public repos only"),
            "{}",
            render(&r, &wide, false, false)
        );
        for w in [80, 60] {
            let mode = Mode::styled(Tier::None, Glyphs::Unicode, w);
            let out = render(&r, &mode, false, false);
            let footer = footer_line(&out);
            assert!(width(footer) <= w, "{out}");
            assert!(footer.contains("last 1 year"), "{footer}");
            assert!(footer.contains("public repos only"), "{footer}");
        }
        // "all time" is not repeated in brackets.
        r.range.period = Some(crate::cli::Period::All);
        r.range.period_label = Some("all time");
        r.range.since = None;
        assert_eq!(range_label(&r, &wide), "all time");
        // --since/--until without a preset keep the plain dates.
        r.range.period = None;
        r.range.period_label = None;
        r.range.since = Some("2025-01-01".into());
        r.range.until = Some("2025-12-31".into());
        assert_eq!(range_label(&r, &wide), "2025-01-01 → 2025-12-31");
    }

    #[test]
    fn public_only_disclosure_survives_a_narrow_terminal() {
        let mut r = report(vec![org("acme", PrivateAccess::NoToken)]);
        r.orgs[0].incomplete_repos = vec!["a".into(), "b".into()];
        for w in [80, 60] {
            let mode = Mode::styled(Tier::None, Glyphs::Unicode, w);
            let out = render(&r, &mode, false, false);
            let footer = footer_line(&out);
            assert!(width(footer) <= w, "{out}");
            assert!(footer.contains("public repos only"), "{footer}");
            // At 60 columns the disclosure leads and only the range after it is cut.
            assert!(w == 60 || !footer.contains('…'), "{footer}");
        }
        r.orgs[0].incomplete_repos.clear();
        let mode = Mode::styled(Tier::None, Glyphs::Unicode, 60);
        let footer = render(&r, &mode, false, false);
        let footer = footer_line(&footer);
        assert!(!footer.contains('…'), "{footer}");
        assert!(
            footer.contains("2025-01-01 → 2025-12-31 · public repos only"),
            "{footer}"
        );
        let wide = Mode::styled(Tier::None, Glyphs::Unicode, 200);
        let out = render(&r, &wide, false, false);
        assert!(
            footer_line(&out).contains("2026-11-18 · 2025-01-01 → 2025-12-31 · public repos only"),
            "{out}"
        );
    }
}
