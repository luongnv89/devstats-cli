//! Command-line definition (clap derive).

use std::path::PathBuf;

use chrono::{Days, Months, NaiveDate};
use clap::{Parser, ValueEnum};

const EXAMPLES: &str = "\
Examples:
  devstats https://github.com/Montimage
  devstats https://github.com/Montimage https://github.com/montimage-projects --top 10 --sort lines
  devstats Montimage --period 6m --html report.html
  devstats Montimage --period all
  devstats Montimage --since 2025-01-01 --until 2025-06-30
  devstats Montimage montimage-projects --format json | jq '.contributors[0]'
  devstats Montimage --format tsv --top 0 > contributors.tsv

Token: --token > GITHUB_TOKEN > GH_TOKEN > `gh auth token` (github.com only).
Without a token GitHub allows 60 requests/hour, which is rarely enough, and only
public repositories are scanned.";

#[derive(Parser, Debug, Clone)]
#[command(
    name = "devstats",
    version,
    about = "Contribution stats for every contributor across GitHub organizations",
    long_about = "Collects commits, lines added/deleted, pull requests and issues for every \
contributor in one or more GitHub organizations (or users), then prints a ranked table \
or writes a self-contained HTML report. Private organization repositories are included when \
the token can read them; without a token only public repositories are scanned.",
    after_help = EXAMPLES
)]
pub struct Cli {
    /// Organization or user: https://github.com/Montimage, github.com/Montimage, or Montimage
    #[arg(value_name = "ORG", required_unless_present = "completions")]
    pub orgs: Vec<String>,

    /// Also write a self-contained HTML report [default: devstats-report.html]. Put it after the ORGs, or use --html=PATH
    #[arg(long, value_name = "PATH", num_args = 0..=1, default_missing_value = "devstats-report.html")]
    pub html: Option<PathBuf>,

    /// Format of the stdout output
    #[arg(short, long, value_enum, default_value_t = Format::Table)]
    pub format: Format,

    /// Rank contributors by this metric
    #[arg(short, long, value_enum, default_value_t = SortKey::Commits)]
    pub sort: SortKey,

    /// Show only the top N contributors (0 = all)
    #[arg(short = 'n', long, value_name = "N", default_value_t = 25)]
    pub top: usize,

    /// Only count activity in this period, ending today [default: 1y unless --since or --until is given]
    #[arg(short, long, value_enum, value_name = "PERIOD", conflicts_with_all = ["since", "until"])]
    pub period: Option<Period>,

    /// Only count activity on or after this date (YYYY-MM-DD)
    #[arg(long, value_name = "DATE", value_parser = parse_date)]
    pub since: Option<NaiveDate>,

    /// Only count activity on or before this date (YYYY-MM-DD)
    #[arg(long, value_name = "DATE", value_parser = parse_date)]
    pub until: Option<NaiveDate>,

    /// Also scan forked repositories
    #[arg(long)]
    pub include_forks: bool,

    /// Skip archived repositories
    #[arg(long)]
    pub exclude_archived: bool,

    /// Keep bot accounts (logins ending in [bot] or of type Bot)
    #[arg(long)]
    pub include_bots: bool,

    /// Also print one table per organization (table format only)
    #[arg(long)]
    pub per_org: bool,

    /// Number of repositories fetched in parallel
    #[arg(short = 'j', long, value_name = "N", default_value_t = 6,
          value_parser = clap::value_parser!(u16).range(1..=32))]
    pub jobs: u16,

    /// GitHub token (overrides GITHUB_TOKEN, GH_TOKEN and `gh auth token`)
    #[arg(long, value_name = "TOKEN")]
    pub token: Option<String>,

    /// GitHub API base URL, for GitHub Enterprise [env: DEVSTATS_API_URL] [default: https://api.github.com]
    #[arg(long, value_name = "URL")]
    pub api_url: Option<String>,

    /// Always fetch fresh data (skip the 1-hour response cache in ~/.cache/devstats)
    #[arg(long)]
    pub no_cache: bool,

    /// Print a shell completion script and exit (bash, zsh, fish, elvish, powershell)
    #[arg(long, value_name = "SHELL", hide = true)]
    pub completions: Option<clap_complete::Shell>,

    /// Disable colors (also honored: NO_COLOR)
    #[arg(long)]
    pub no_color: bool,

    /// Print no progress or warnings, only errors
    #[arg(short, long, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Log every API call to stderr
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Boxed table for humans (tab-separated when piped)
    Table,
    /// Full report as JSON
    Json,
    /// Tab-separated records, no header
    Tsv,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SortKey {
    Commits,
    Additions,
    Deletions,
    /// additions + deletions
    Lines,
    Prs,
    Issues,
    Repos,
}

impl SortKey {
    pub fn label(self) -> &'static str {
        match self {
            SortKey::Commits => "commits",
            SortKey::Additions => "lines added",
            SortKey::Deletions => "lines deleted",
            SortKey::Lines => "lines changed",
            SortKey::Prs => "pull requests",
            SortKey::Issues => "issues",
            SortKey::Repos => "repositories",
        }
    }
}

/// A preset time period, counted back from today.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Period {
    /// All time (no start date)
    #[value(name = "all", alias = "all-time")]
    #[serde(rename = "all")]
    All,
    /// The last year
    #[value(name = "1y")]
    #[serde(rename = "1y")]
    Year,
    /// The last 6 months
    #[value(name = "6m")]
    #[serde(rename = "6m")]
    SixMonths,
    /// The last 3 months
    #[value(name = "3m")]
    #[serde(rename = "3m")]
    ThreeMonths,
    /// The last month
    #[value(name = "1m")]
    #[serde(rename = "1m")]
    Month,
    /// The last week
    #[value(name = "1w")]
    #[serde(rename = "1w")]
    Week,
    /// The last day
    #[value(name = "1d")]
    #[serde(rename = "1d")]
    Day,
}

impl Period {
    /// The period used when neither --period nor a date flag is given.
    pub const DEFAULT: Period = Period::Year;

    pub fn label(self) -> &'static str {
        match self {
            Period::All => "all time",
            Period::Year => "last 1 year",
            Period::SixMonths => "last 6 months",
            Period::ThreeMonths => "last 3 months",
            Period::Month => "last 1 month",
            Period::Week => "last 1 week",
            Period::Day => "last 1 day",
        }
    }

    /// First day of the period ending on `end`: the same calendar date one period
    /// earlier (month ends clamp, e.g. 6 months before Aug 31 is Feb 28/29).
    /// `None` for all time.
    pub fn start(self, end: NaiveDate) -> Option<NaiveDate> {
        let months = |n| end.checked_sub_months(Months::new(n));
        match self {
            Period::All => None,
            Period::Year => months(12),
            Period::SixMonths => months(6),
            Period::ThreeMonths => months(3),
            Period::Month => months(1),
            Period::Week => end.checked_sub_days(Days::new(7)),
            Period::Day => end.checked_sub_days(Days::new(1)),
        }
    }
}

fn parse_date(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| format!("'{s}' is not a date; use YYYY-MM-DD, e.g. 2025-01-31"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn period_start_is_the_same_date_one_period_earlier() {
        let end = d("2026-11-18");
        let cases = [
            (Period::All, None),
            (Period::Year, Some("2025-11-18")),
            (Period::SixMonths, Some("2026-05-18")),
            (Period::ThreeMonths, Some("2026-08-18")),
            (Period::Month, Some("2026-10-18")),
            (Period::Week, Some("2026-11-11")),
            (Period::Day, Some("2026-11-17")),
        ];
        for (p, want) in cases {
            assert_eq!(p.start(end), want.map(d), "{p:?}");
        }
    }

    #[test]
    fn period_start_clamps_month_ends() {
        assert_eq!(
            Period::SixMonths.start(d("2026-08-31")),
            Some(d("2026-02-28"))
        );
        assert_eq!(Period::Month.start(d("2024-03-31")), Some(d("2024-02-29")));
        assert_eq!(Period::Year.start(d("2024-02-29")), Some(d("2023-02-28")));
        assert_eq!(Period::Week.start(d("2026-01-03")), Some(d("2025-12-27")));
    }

    #[test]
    fn default_period_is_one_year() {
        assert_eq!(Period::DEFAULT, Period::Year);
        assert_eq!(Period::DEFAULT.label(), "last 1 year");
    }
}
