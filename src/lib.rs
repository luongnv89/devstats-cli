//! devstats: contribution stats for every contributor across GitHub organizations.

pub mod aggregate;
pub mod cli;
pub mod error;
pub mod github;
pub mod model;
pub mod org;
pub mod output;
pub mod style;
pub mod ui;

use std::io::Write;
use std::sync::Arc;

use chrono::Utc;
use clap::{CommandFactory, FromArgMatches};

use crate::cli::{Cli, Format};
use crate::error::{AppError, Result};
use crate::github::{repos, stats, Client, DEFAULT_API};
use crate::model::OrgData;
use crate::ui::Reporter;

/// Parse arguments, run, and return the process exit code.
pub fn run() -> i32 {
    let matches = Cli::command().get_matches();
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(c) => c,
        Err(e) => e.exit(),
    };
    let ui = Arc::new(Reporter::new(cli.verbose, cli.quiet));
    match execute(&cli, ui.clone()) {
        Ok(()) => 0,
        Err(e) => {
            ui.error(&e);
            e.exit_code()
        }
    }
}

fn execute(cli: &Cli, ui: Arc<Reporter>) -> Result<()> {
    if let (Some(s), Some(u)) = (cli.since, cli.until) {
        if s > u {
            return Err(AppError::usage(format!("--since {s} is after --until {u}")).hint("swap the two dates"));
        }
    }
    let logins = org::parse_all(&cli.orgs)?;

    let api = cli
        .api_url
        .clone()
        .or_else(|| std::env::var("DEVSTATS_API_URL").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| DEFAULT_API.to_string());
    let token = github::resolve_token(cli.token.as_deref(), api.trim_end_matches('/') == DEFAULT_API);
    match &token {
        Some((_, source)) => ui.debug(&format!("using the token from {source}")),
        None => ui.warn("no GitHub token found; GitHub allows only 60 requests/hour without one (set GITHUB_TOKEN or run `gh auth login`)"),
    }
    let client = Client::new(&api, token.map(|(t, _)| t), ui.clone());

    let mut data = Vec::new();
    for login in &logins {
        if let Some(d) = collect_org(&client, login, cli)? {
            data.push(d);
        }
    }
    if data.is_empty() {
        return Err(AppError::input("none of the given organizations exist on GitHub")
            .hint("check the spelling, e.g. https://github.com/Montimage"));
    }

    let opts = aggregate::Options {
        since: cli.since,
        until: cli.until,
        include_bots: cli.include_bots,
        sort: cli.sort,
        top: cli.top,
        today: Utc::now().date_naive(),
    };
    let report = aggregate::build(&data, &opts);

    let text = match cli.format {
        Format::Json => output::json::render(&report),
        Format::Tsv | Format::Table => output::tsv::render(&report),
    };
    write_stdout(&text)
}

/// Fetch everything for one org. `Ok(None)` (after a warning) when it does not exist.
fn collect_org(client: &Client, login: &str, cli: &Cli) -> Result<Option<OrgData>> {
    let Some(owner) = repos::owner(client, login)? else {
        client.ui.warn(&format!("'{login}' was not found on GitHub; skipping it"));
        return Ok(None);
    };
    let all = repos::list(client, &owner)?;
    let total = all.len();
    let scanned: Vec<String> = all
        .into_iter()
        .filter(|r| cli.include_forks || !r.fork)
        .filter(|r| !(cli.exclude_archived && r.archived))
        .map(|r| r.name)
        .collect();
    client.ui.debug(&format!("{}: {} of {total} repos to scan", owner.login, scanned.len()));
    let repo_stats = stats::fetch_all(client, &owner.login, &scanned, cli.jobs as usize, &|_| {})?;
    Ok(Some(OrgData {
        repos_skipped: total - scanned.len(),
        repos_scanned: scanned,
        stats: repo_stats,
        items: Vec::new(),
        owner,
    }))
}

/// Write to stdout; a closed pipe (`devstats … | head`) is not an error.
fn write_stdout(text: &str) -> Result<()> {
    let mut out = std::io::stdout().lock();
    match out.write_all(text.as_bytes()).and_then(|_| out.flush()) {
        Err(e) if e.kind() != std::io::ErrorKind::BrokenPipe => {
            Err(AppError::runtime(format!("cannot write output: {e}")))
        }
        _ => Ok(()),
    }
}
