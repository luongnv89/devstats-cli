//! Merge per-repo stats and search items into ranked contributors.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{Datelike, NaiveDate, TimeZone, Utc};

use crate::cli::SortKey;
use crate::model::{
    Contributor, OrgData, OrgReport, PrivateAccess, Range, Report, Totals, WeekPoint,
};

pub const WEEK: i64 = 7 * 86_400;
pub const ACTIVITY_WEEKS: usize = 52;

#[derive(Debug, Clone)]
pub struct Options {
    pub since: Option<NaiveDate>,
    pub until: Option<NaiveDate>,
    pub include_bots: bool,
    pub sort: SortKey,
    pub top: usize,
    pub today: NaiveDate,
}

pub fn ts(d: NaiveDate) -> i64 {
    d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp()
}

/// Unix timestamp of the Sunday that starts `d`'s week (GitHub's week boundary).
pub fn week_start(d: NaiveDate) -> i64 {
    ts(d) - d.weekday().num_days_from_sunday() as i64 * 86_400
}

pub fn week_label(w: i64) -> String {
    Utc.timestamp_opt(w, 0)
        .single()
        .map(|t| t.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

#[derive(Default)]
struct Acc {
    login: String,
    avatar_url: String,
    commits: u64,
    additions: u64,
    deletions: u64,
    prs: u64,
    prs_merged: u64,
    issues: u64,
    repos: BTreeSet<String>,
    orgs: BTreeSet<String>,
    activity: Vec<u64>,
}

struct Window {
    start: Option<i64>,
    end: Option<i64>,
    act_start: i64,
    end_week: i64,
}

impl Window {
    fn new(o: &Options) -> Self {
        let end_week = week_start(o.until.unwrap_or(o.today));
        Window {
            start: o.since.map(ts),
            end: o.until.map(|d| ts(d) + 86_399),
            act_start: end_week - (ACTIVITY_WEEKS as i64 - 1) * WEEK,
            end_week,
        }
    }
    /// A GitHub week counts when it overlaps the range.
    fn has_week(&self, w: i64) -> bool {
        self.start.is_none_or(|s| w + WEEK > s) && self.end.is_none_or(|e| w <= e)
    }
    fn has_day(&self, d: NaiveDate, o: &Options) -> bool {
        o.since.is_none_or(|s| d >= s) && o.until.is_none_or(|u| d <= u)
    }
}

fn entry<'m>(accs: &'m mut HashMap<String, Acc>, login: &str, avatar: &str) -> &'m mut Acc {
    let acc = accs.entry(login.to_lowercase()).or_insert_with(|| Acc {
        login: login.to_string(),
        activity: vec![0; ACTIVITY_WEEKS],
        ..Default::default()
    });
    if acc.avatar_url.is_empty() {
        acc.avatar_url = avatar.to_string();
    }
    acc
}

fn aggregate<'a>(
    orgs: impl IntoIterator<Item = &'a OrgData>,
    o: &Options,
) -> (Vec<Contributor>, Totals, Vec<WeekPoint>) {
    let win = Window::new(o);
    let mut accs: HashMap<String, Acc> = HashMap::new();
    let mut weekly: BTreeMap<i64, (u64, u64, u64)> = BTreeMap::new();
    let mut totals = Totals::default();
    let mut web_base = String::from("https://github.com");

    for org in orgs {
        let owner = &org.owner;
        if let Some((base, _)) = owner.html_url.rsplit_once('/') {
            web_base = base.to_string();
        }
        totals.repos += org.repos_scanned.len();
        totals.repos_private += org.private_repos;
        totals.repos_skipped += org.repos_skipped;
        for repo in &org.stats {
            for author in repo.authors.iter().filter(|a| o.include_bots || !a.is_bot) {
                let weeks: Vec<_> = author.weeks.iter().filter(|w| win.has_week(w.w)).collect();
                if weeks.is_empty() {
                    continue;
                }
                let acc = entry(&mut accs, &author.login, &author.avatar_url);
                for w in &weeks {
                    acc.commits += w.c;
                    acc.additions += w.a;
                    acc.deletions += w.d;
                    if w.w >= win.act_start && w.w <= win.end_week {
                        acc.activity[((w.w - win.act_start) / WEEK) as usize] += w.c;
                    }
                    let slot = weekly.entry(w.w).or_default();
                    slot.0 += w.c;
                    slot.1 += w.a;
                    slot.2 += w.d;
                }
                if weeks.iter().any(|w| w.c > 0) {
                    acc.repos.insert(format!("{}/{}", owner.login, repo.repo));
                }
                acc.orgs.insert(owner.login.clone());
            }
        }
        for item in org
            .items
            .iter()
            .filter(|i| (o.include_bots || !i.is_bot) && win.has_day(i.created, o))
        {
            let acc = entry(&mut accs, &item.login, "");
            if item.is_pr {
                acc.prs += 1;
                acc.prs_merged += item.merged as u64;
            } else {
                acc.issues += 1;
            }
            acc.orgs.insert(owner.login.clone());
        }
    }

    let mut list: Vec<Contributor> = accs
        .into_values()
        .filter(|a| a.commits + a.additions + a.deletions + a.prs + a.issues > 0)
        .map(|a| Contributor {
            html_url: format!("{web_base}/{}", a.login),
            avatar_url: a.avatar_url,
            commits: a.commits,
            additions: a.additions,
            deletions: a.deletions,
            prs: a.prs,
            prs_merged: a.prs_merged,
            issues: a.issues,
            repos: a.repos.len(),
            orgs: a.orgs.into_iter().collect(),
            share: 0.0,
            activity: a.activity,
            login: a.login,
        })
        .collect();

    totals.contributors = list.len();
    for c in &list {
        totals.commits += c.commits;
        totals.additions += c.additions;
        totals.deletions += c.deletions;
        totals.prs += c.prs;
        totals.prs_merged += c.prs_merged;
        totals.issues += c.issues;
    }
    for c in &mut list {
        c.share = if totals.commits == 0 {
            0.0
        } else {
            c.commits as f64 * 100.0 / totals.commits as f64
        };
    }
    sort(&mut list, o.sort);
    (list, totals, weekly_series(&weekly, &win, o))
}

fn weekly_series(
    weekly: &BTreeMap<i64, (u64, u64, u64)>,
    win: &Window,
    o: &Options,
) -> Vec<WeekPoint> {
    let Some(&first_data) = weekly.keys().next() else {
        return vec![];
    };
    let first = o.since.map(week_start).unwrap_or(first_data);
    let last = win.end_week.max(*weekly.keys().last().unwrap());
    let mut out = Vec::new();
    let mut w = first;
    while w <= last {
        let (c, a, d) = weekly.get(&w).copied().unwrap_or_default();
        out.push(WeekPoint {
            week: week_label(w),
            commits: c,
            additions: a,
            deletions: d,
        });
        w += WEEK;
    }
    out
}

pub fn sort_value(c: &Contributor, key: SortKey) -> u64 {
    match key {
        SortKey::Commits => c.commits,
        SortKey::Additions => c.additions,
        SortKey::Deletions => c.deletions,
        SortKey::Lines => c.additions + c.deletions,
        SortKey::Prs => c.prs,
        SortKey::Issues => c.issues,
        SortKey::Repos => c.repos as u64,
    }
}

pub fn sort(list: &mut [Contributor], key: SortKey) {
    list.sort_by(|a, b| {
        sort_value(b, key)
            .cmp(&sort_value(a, key))
            .then(b.commits.cmp(&a.commits))
            .then_with(|| a.login.to_lowercase().cmp(&b.login.to_lowercase()))
    });
}

fn truncate(mut list: Vec<Contributor>, top: usize) -> Vec<Contributor> {
    if top > 0 {
        list.truncate(top);
    }
    list
}

pub fn build(orgs: &[OrgData], o: &Options) -> Report {
    let (contributors, totals, weekly) = aggregate(orgs, o);
    let org_reports = orgs
        .iter()
        .map(|org| {
            let (list, totals, weekly) = aggregate(std::iter::once(org), o);
            OrgReport {
                login: org.owner.login.clone(),
                name: org.owner.name.clone(),
                kind: if org.owner.is_org {
                    "organization"
                } else {
                    "user"
                },
                avatar_url: org.owner.avatar_url.clone(),
                html_url: org.owner.html_url.clone(),
                totals,
                private_repos: org.private_repos,
                private_access: org.private_access,
                incomplete_repos: org
                    .stats
                    .iter()
                    .filter(|s| !s.complete)
                    .map(|s| s.repo.clone())
                    .collect(),
                contributors: truncate(list, o.top),
                weekly,
            }
        })
        .collect();
    let win = Window::new(o);
    let mut notes: Vec<String> = vec![
        "Commits and lines come from GitHub's contributor statistics: default branch only, commits linked to a GitHub account only.".into(),
        "GitHub reports 0 added/deleted lines for repositories with 10,000 or more commits.".into(),
        "Pull requests and issues are counted by author and creation date, across the repositories of each organization the token can read (public repositories only without a token).".into(),
    ];
    notes.extend(orgs.iter().filter_map(private_note));
    Report {
        tool: "devstats",
        version: env!("CARGO_PKG_VERSION"),
        generated_at: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        range: Range {
            since: o.since.map(|d| d.to_string()),
            until: o.until.map(|d| d.to_string()),
            activity_from: week_label(win.act_start),
        },
        sort: o.sort,
        top: o.top,
        totals,
        orgs: org_reports,
        contributors: truncate(contributors, o.top),
        weekly,
        notes,
    }
}

/// A note for an organization whose private repositories are not in the scan.
fn private_note(org: &OrgData) -> Option<String> {
    let why = match org.private_access {
        PrivateAccess::Included => return None,
        PrivateAccess::NoToken => "no token".to_string(),
        PrivateAccess::NoneVisible => "none visible to this token".to_string(),
        PrivateAccess::ListingFailed => format!(
            "the full repository listing failed with HTTP {}; public repositories only",
            org.listing_status.unwrap_or_default()
        ),
        PrivateAccess::UserAccount => "user account: public repositories only".to_string(),
    };
    Some(format!(
        "{}: private repositories not included ({why}).",
        org.owner.login
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::repos::Owner;
    use crate::github::stats::{AuthorStats, RepoStats, Week};
    use crate::model::Item;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn author(login: &str, weeks: &[(&str, u64, u64, u64)]) -> AuthorStats {
        AuthorStats {
            login: login.into(),
            avatar_url: format!("https://avatars/{login}"),
            is_bot: login.ends_with("[bot]"),
            weeks: weeks
                .iter()
                .map(|&(w, c, a, dd)| Week {
                    w: week_start(d(w)),
                    a,
                    d: dd,
                    c,
                })
                .collect(),
        }
    }

    fn org(login: &str, repos: Vec<(&str, Vec<AuthorStats>)>, items: Vec<Item>) -> OrgData {
        OrgData {
            owner: Owner {
                login: login.into(),
                name: None,
                is_org: true,
                avatar_url: String::new(),
                html_url: format!("https://github.com/{login}"),
            },
            repos_scanned: repos.iter().map(|(n, _)| n.to_string()).collect(),
            repos_skipped: 0,
            private_repos: 0,
            private_access: PrivateAccess::NoneVisible,
            listing_status: None,
            stats: repos
                .into_iter()
                .map(|(n, authors)| RepoStats {
                    repo: n.into(),
                    authors,
                    complete: true,
                })
                .collect(),
            items,
        }
    }

    fn item(login: &str, is_pr: bool, merged: bool, created: &str) -> Item {
        Item {
            login: login.into(),
            is_bot: login.ends_with("[bot]"),
            is_pr,
            merged,
            created: d(created),
        }
    }

    fn opts() -> Options {
        Options {
            since: None,
            until: None,
            include_bots: false,
            sort: SortKey::Commits,
            top: 0,
            today: d("2026-11-18"),
        }
    }

    fn fixture() -> Vec<OrgData> {
        vec![
            org(
                "acme",
                vec![
                    (
                        "api",
                        vec![
                            author("alice", &[("2026-01-05", 10, 100, 10)]),
                            author("Bob", &[("2025-06-02", 3, 30, 3)]),
                        ],
                    ),
                    (
                        "web",
                        vec![
                            author("alice", &[("2026-03-02", 5, 50, 5)]),
                            author("ci[bot]", &[("2026-03-02", 99, 9, 9)]),
                        ],
                    ),
                ],
                vec![
                    item("alice", true, true, "2026-01-06"),
                    item("bob", true, false, "2025-06-03"),
                    item("carol", false, false, "2026-02-01"),
                ],
            ),
            org(
                "beta",
                vec![("lib", vec![author("bob", &[("2026-05-04", 20, 7, 1)])])],
                vec![],
            ),
        ]
    }

    #[test]
    fn merges_across_repos_and_orgs_case_insensitively() {
        let r = build(&fixture(), &opts());
        let names: Vec<_> = r.contributors.iter().map(|c| c.login.as_str()).collect();
        assert_eq!(names, ["Bob", "alice", "carol"]);
        let bob = &r.contributors[0];
        assert_eq!(
            (
                bob.commits,
                bob.additions,
                bob.repos,
                bob.prs,
                bob.prs_merged
            ),
            (23, 37, 2, 1, 0)
        );
        assert_eq!(bob.orgs, ["acme", "beta"]);
        let alice = &r.contributors[1];
        assert_eq!(
            (alice.commits, alice.repos, alice.prs, alice.prs_merged),
            (15, 2, 1, 1)
        );
        assert_eq!(r.totals.commits, 38);
        assert_eq!(r.totals.contributors, 3);
        assert_eq!(r.totals.issues, 1);
        assert!((r.contributors.iter().map(|c| c.share).sum::<f64>() - 100.0).abs() < 1e-9);
        assert_eq!(r.orgs.len(), 2);
        assert_eq!(r.orgs[1].totals.commits, 20);
    }

    #[test]
    fn bots_are_excluded_unless_asked() {
        let r = build(&fixture(), &opts());
        assert!(r.contributors.iter().all(|c| c.login != "ci[bot]"));
        let r = build(
            &fixture(),
            &Options {
                include_bots: true,
                ..opts()
            },
        );
        assert_eq!(r.contributors[0].login, "ci[bot]");
    }

    #[test]
    fn since_until_filter_weeks_and_items() {
        let r = build(
            &fixture(),
            &Options {
                since: Some(d("2026-01-01")),
                until: Some(d("2026-04-01")),
                ..opts()
            },
        );
        let names: Vec<_> = r.contributors.iter().map(|c| c.login.as_str()).collect();
        assert_eq!(names, ["alice", "carol"]);
        assert_eq!(r.totals.commits, 15);
        assert_eq!(r.totals.prs, 1);
    }

    #[test]
    fn sort_keys_and_top() {
        let r = build(
            &fixture(),
            &Options {
                sort: SortKey::Lines,
                top: 2,
                ..opts()
            },
        );
        assert_eq!(r.contributors.len(), 2);
        assert_eq!(r.contributors[0].login, "alice"); // 165 lines vs Bob's 41
        assert_eq!(r.totals.contributors, 3, "totals ignore --top");
        let r = build(
            &fixture(),
            &Options {
                sort: SortKey::Issues,
                ..opts()
            },
        );
        assert_eq!(r.contributors[0].login, "carol");
    }

    #[test]
    fn activity_covers_last_52_weeks() {
        let r = build(&fixture(), &opts());
        let alice = r.contributors.iter().find(|c| c.login == "alice").unwrap();
        assert_eq!(alice.activity.len(), 52);
        assert_eq!(alice.activity.iter().sum::<u64>(), 15);
        assert_eq!(
            r.range.activity_from,
            week_label(week_start(d("2026-11-18")) - 51 * WEEK)
        );
        let bob = &r.contributors[0];
        assert_eq!(
            bob.activity.iter().sum::<u64>(),
            20,
            "Bob's 2025-06 commits fall outside the window"
        );
    }

    #[test]
    fn weekly_series_is_contiguous() {
        let r = build(&fixture(), &opts());
        assert_eq!(
            r.weekly.first().unwrap().week,
            week_label(week_start(d("2025-06-02")))
        );
        assert_eq!(
            r.weekly.last().unwrap().week,
            week_label(week_start(d("2026-11-18")))
        );
        assert_eq!(r.weekly.iter().map(|p| p.commits).sum::<u64>(), 38);
    }

    #[test]
    fn week_start_is_sunday() {
        assert_eq!(week_label(week_start(d("2026-11-18"))), "2026-11-15");
        assert_eq!(week_label(week_start(d("2026-11-15"))), "2026-11-15");
    }
    #[test]
    fn private_repos_are_totalled_and_missing_access_is_noted() {
        let mut orgs = fixture();
        orgs[0].private_repos = 1;
        orgs[0].private_access = PrivateAccess::Included;
        orgs[1].private_access = PrivateAccess::NoToken;
        let r = build(&orgs, &opts());
        assert_eq!(r.totals.repos, 3);
        assert_eq!(r.totals.repos_private, 1);
        assert_eq!(r.orgs[0].totals.repos_private, 1);
        assert_eq!(
            (r.orgs[0].private_repos, r.orgs[0].private_access),
            (1, PrivateAccess::Included)
        );
        assert_eq!(r.orgs[1].totals.repos_private, 0);
        assert!(!r.notes.iter().any(|n| n.starts_with("acme:")));
        assert!(r
            .notes
            .contains(&"beta: private repositories not included (no token).".to_string()));
        let reasons = [
            (
                PrivateAccess::NoneVisible,
                None,
                "(none visible to this token)",
            ),
            (
                PrivateAccess::ListingFailed,
                Some(403),
                "(the full repository listing failed with HTTP 403; public repositories only)",
            ),
            (
                PrivateAccess::UserAccount,
                None,
                "(user account: public repositories only)",
            ),
        ];
        for (access, status, why) in reasons {
            orgs[1].private_access = access;
            orgs[1].listing_status = status;
            let r = build(&orgs, &opts());
            let note = format!("beta: private repositories not included {why}.");
            assert!(r.notes.contains(&note), "{:?}", r.notes);
        }
    }
}
