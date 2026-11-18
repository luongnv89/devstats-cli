//! Command-line definition (clap derive).

use std::path::PathBuf;

use chrono::NaiveDate;
use clap::{Parser, ValueEnum};

const EXAMPLES: &str = "\
Examples:
  devstats https://github.com/Montimage
  devstats https://github.com/Montimage https://github.com/montimage-projects --top 10 --sort lines
  devstats Montimage --since 2025-01-01 --html report.html
  devstats Montimage montimage-projects --format json | jq '.contributors[0]'
  devstats Montimage --format tsv --top 0 > contributors.tsv

Token: --token > GITHUB_TOKEN > GH_TOKEN > `gh auth token` (github.com only).
Without a token GitHub allows 60 requests/hour, which is rarely enough.";

#[derive(Parser, Debug, Clone)]
#[command(
    name = "devstats",
    version,
    about = "Contribution stats for every contributor across GitHub organizations",
    long_about = "Collects commits, lines added/deleted, pull requests and issues for every \
contributor in one or more public GitHub organizations (or users), then prints a ranked table \
or writes a self-contained HTML report.",
    after_help = EXAMPLES
)]
pub struct Cli {
    /// Organization or user: https://github.com/Montimage, github.com/Montimage, or Montimage
    #[arg(value_name = "ORG", required = true)]
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

fn parse_date(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| format!("'{s}' is not a date; use YYYY-MM-DD, e.g. 2025-01-31"))
}
