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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{NaiveDate, Utc};
use clap::{CommandFactory, FromArgMatches};

use crate::cli::{Cli, Format, Period};
use crate::error::{AppError, Result};
use crate::github::{repos, search, stats, Client, DEFAULT_API};
use crate::model::{OrgData, PrivateAccess};
use crate::org::Target;
use crate::style::{decide, Env, Mode, Stream};
use crate::ui::Reporter;

/// Parse arguments, run, and return the process exit code.
pub fn run() -> i32 {
    let matches = Cli::command()
        .styles(style::theme::clap_styles())
        .color(help_color())
        .get_matches();
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(c) => c,
        Err(e) => e.exit(),
    };
    if let Some(shell) = cli.completions {
        let mut cmd = Cli::command();
        let mut buf = Vec::new();
        clap_complete::generate(shell, &mut cmd, "devstats", &mut buf);
        return match write_stdout(&String::from_utf8_lossy(&buf)) {
            Ok(()) => 0,
            Err(e) => e.exit_code(),
        };
    }
    let out_mode = decide(
        &Env::capture(Stream::Stdout),
        Stream::Stdout,
        cli.format != Format::Table,
        cli.no_color,
    );
    let err_mode = decide(
        &Env::capture(Stream::Stderr),
        Stream::Stderr,
        false,
        cli.no_color,
    );
    let ui = Arc::new(Reporter::new(err_mode, cli.verbose, cli.quiet));
    match execute(&cli, ui.clone(), out_mode) {
        Ok(()) => 0,
        Err(e) => {
            ui.error(&e);
            e.exit_code()
        }
    }
}

/// Color for clap's help and usage errors, decided before parsing: `--no-color` and
/// NO_COLOR turn it off entirely, FORCE_COLOR forces it, otherwise clap checks the TTY.
fn help_color() -> clap::ColorChoice {
    let set = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let no_color_flag = std::env::args_os()
        .take_while(|a| a != "--")
        .any(|a| a == "--no-color");
    if no_color_flag || set("NO_COLOR").is_some() || set("TERM").as_deref() == Some("dumb") {
        clap::ColorChoice::Never
    } else if set("FORCE_COLOR").is_some_and(|v| v != "0") {
        clap::ColorChoice::Always
    } else {
        clap::ColorChoice::Auto
    }
}

/// The date range to report on, resolved once from --period, --since and --until.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateRange {
    pub since: Option<NaiveDate>,
    pub until: Option<NaiveDate>,
    /// The preset in effect; `None` when the range comes from --since/--until.
    pub period: Option<Period>,
}

impl DateRange {
    /// `--period` wins (clap keeps it apart from the date flags); with no flag at
    /// all the default period applies; otherwise the explicit dates are used as given.
    pub fn resolve(cli: &Cli, today: NaiveDate) -> Result<Self> {
        if cli.period.is_some() || (cli.since.is_none() && cli.until.is_none()) {
            let period = cli.period.unwrap_or(Period::DEFAULT);
            return Ok(DateRange {
                since: period.start(today),
                until: None,
                period: Some(period),
            });
        }
        if let (Some(s), Some(u)) = (cli.since, cli.until) {
            if s > u {
                return Err(AppError::usage(format!("--since {s} is after --until {u}"))
                    .hint("swap the two dates"));
            }
        }
        Ok(DateRange {
            since: cli.since,
            until: cli.until,
            period: None,
        })
    }
}

fn execute(cli: &Cli, ui: Arc<Reporter>, out_mode: Mode) -> Result<()> {
    let today = Utc::now().date_naive();
    let range = DateRange::resolve(cli, today)?;
    let targets = drop_covered(org::parse_all(&cli.orgs)?, &ui);

    let api = cli
        .api_url
        .clone()
        .or_else(|| {
            std::env::var("DEVSTATS_API_URL")
                .ok()
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| DEFAULT_API.to_string());
    let token = github::resolve_token(
        cli.token.as_deref(),
        api.trim_end_matches('/') == DEFAULT_API,
    );
    match &token {
        Some((_, source)) => ui.debug(&format!("using the token from {source}")),
        None => ui.warn("no GitHub token found; only public repositories are scanned, and GitHub allows only 60 requests/hour without one (set GITHUB_TOKEN or run `gh auth login`)"),
    }
    let client = Client::new(&api, token.map(|(t, _)| t), ui.clone(), !cli.no_cache);

    // Keep the progress line's clock moving during long waits.
    let running = AtomicBool::new(true);
    let data = std::thread::scope(|s| {
        s.spawn(|| {
            while running.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(250));
                ui.tick();
            }
        });
        let result = (|| {
            let mut data = Vec::new();
            for t in &targets {
                let d = match &t.repo {
                    Some(repo) => collect_repo(&client, &t.owner, repo, cli, &range, today)?,
                    None => collect_org(&client, &t.owner, cli, &range, today)?,
                };
                if let Some(d) = d {
                    data.push(d);
                }
            }
            Ok::<_, AppError>(data)
        })();
        running.store(false, Ordering::SeqCst);
        result
    })?;
    ui.clear();
    if data.is_empty() {
        return Err(
            AppError::input("none of the given organizations or repositories exist on GitHub")
                .hint("check the spelling, e.g. https://github.com/Montimage or https://github.com/Montimage/mmt-probe"),
        );
    }

    let opts = aggregate::Options {
        since: range.since,
        until: range.until,
        period: range.period,
        include_bots: cli.include_bots,
        sort: cli.sort,
        top: cli.top,
        today,
    };
    let report = aggregate::build(&data, &opts);

    let text = match cli.format {
        Format::Json => output::json::render(&report),
        Format::Table if out_mode.styled => {
            output::table::render(&report, &out_mode, cli.per_org, cli.pr_weeks)
        }
        // TSV, or a table piped to another program.
        _ if cli.pr_weeks => output::tsv::render_pr_weeks(&report),
        _ => output::tsv::render(&report),
    };
    if let Some(path) = &cli.html {
        output::html::write(&report, path)?;
        ui.info(&format!("wrote {}", path.display()));
    }
    write_stdout(&text)
}

/// Fetch everything for one org. `Ok(None)` (after a warning) when it does not exist.
fn collect_org(
    client: &Client,
    login: &str,
    cli: &Cli,
    range: &DateRange,
    today: NaiveDate,
) -> Result<Option<OrgData>> {
    client.ui.progress(login, 0, None);
    let Some(owner) = repos::owner(client, login)? else {
        client
            .ui
            .warn(&format!("'{login}' was not found on GitHub; skipping it"));
        return Ok(None);
    };
    let listing = repos::list(client, &owner)?;
    let total = listing.repos.len();
    let (scanned, private): (Vec<String>, Vec<bool>) = listing
        .repos
        .into_iter()
        .filter(|r| cli.include_forks || !r.fork)
        .filter(|r| !(cli.exclude_archived && r.archived))
        .map(|r| (r.name, r.private))
        .unzip();
    let private_repos = private.iter().filter(|p| **p).count();
    client.ui.debug(&format!(
        "{}: {} of {total} repos to scan ({private_repos} private)",
        owner.login,
        scanned.len()
    ));
    match listing.access {
        PrivateAccess::ListingFailed => client.ui.warn(&format!(
            "{}: the full repository listing failed (HTTP {}); only public repositories are scanned (if the organization uses SAML single sign-on, authorize the token for it)",
            owner.login,
            listing.failed_status.unwrap_or_default()
        )),
        PrivateAccess::NoneVisible => client.ui.info(&format!(
            "{}: no private repositories visible to this token; scanned public repositories only",
            owner.login
        )),
        PrivateAccess::UserAccount => client.ui.debug(&format!(
            "{}: user account; GitHub lists only its public repositories",
            owner.login
        )),
        PrivateAccess::Included | PrivateAccess::NoToken | PrivateAccess::Repository => {}
    }
    client.ui.progress(&owner.login, 0, Some(scanned.len()));

    let scope = search::scope(&owner.login, owner.is_org, None);
    let (repo_stats, items) =
        fetch_activity(client, &owner.login, &scope, &scanned, cli, range, today);
    Ok(Some(OrgData {
        repo: None,
        repos_skipped: total - scanned.len(),
        repos_scanned: scanned,
        private_repos,
        private_access: listing.access,
        listing_status: listing.failed_status,
        stats: repo_stats?,
        items: items?,
        owner,
    }))
}

/// Fetch everything for one repository. `Ok(None)` (after a warning) when it does
/// not exist or the token cannot read it. --include-forks and --exclude-archived do
/// not apply: the repository was named explicitly.
fn collect_repo(
    client: &Client,
    owner: &str,
    name: &str,
    cli: &Cli,
    range: &DateRange,
    today: NaiveDate,
) -> Result<Option<OrgData>> {
    let label = format!("{owner}/{name}");
    client.ui.progress(&label, 0, None);
    let Some((owner, repo)) = repos::repo(client, owner, name)? else {
        client.ui.warn(&format!(
            "'{label}' was not found on GitHub (or this token cannot read it); skipping it"
        ));
        return Ok(None);
    };
    let label = format!("{}/{}", owner.login, repo.name);
    let scanned = vec![repo.name.clone()];
    client.ui.progress(&label, 0, Some(1));
    let scope = search::scope(&owner.login, owner.is_org, Some(&repo.name));
    let (repo_stats, items) =
        fetch_activity(client, &owner.login, &scope, &scanned, cli, range, today);
    Ok(Some(OrgData {
        private_repos: repo.private as usize,
        private_access: PrivateAccess::Repository,
        listing_status: None,
        repos_skipped: 0,
        repos_scanned: scanned,
        stats: repo_stats?,
        items: items?,
        repo: Some(repo),
        owner,
    }))
}

/// Contributor stats for `repos` of `owner`, and the PRs and issues in `scope`.
/// Search (rate-limited to 30/min) runs beside the stats workers.
#[allow(clippy::type_complexity)]
fn fetch_activity(
    client: &Client,
    owner: &str,
    scope: &str,
    repos: &[String],
    cli: &Cli,
    range: &DateRange,
    today: NaiveDate,
) -> (Result<Vec<stats::RepoStats>>, Result<Vec<model::Item>>) {
    let from = range.since.unwrap_or_else(search::epoch);
    let to = range.until.unwrap_or(today);
    let label = scope.split_once(':').map_or(scope, |(_, s)| s);
    std::thread::scope(|s| {
        let search = s.spawn(|| -> Result<Vec<model::Item>> {
            let mut items = search::fetch(client, scope, true, from, to)?;
            items.extend(search::fetch(client, scope, false, from, to)?);
            Ok(items)
        });
        let on_done = |n: usize| client.ui.progress(label, n, Some(repos.len()));
        let st = stats::fetch_all(client, owner, repos, cli.jobs as usize, &on_done);
        if !search.is_finished() {
            client
                .ui
                .progress_note(label, "counting pull requests and issues");
        }
        (
            st,
            search
                .join()
                .unwrap_or_else(|_| Err(AppError::runtime("search worker crashed"))),
        )
    })
}

/// Drop repository targets whose owner is also scanned as a whole (with a warning),
/// so their contributions are not counted twice.
fn drop_covered(targets: Vec<Target>, ui: &Reporter) -> Vec<Target> {
    let whole: Vec<String> = targets
        .iter()
        .filter(|t| t.repo.is_none())
        .map(|t| t.owner.to_lowercase())
        .collect();
    targets
        .into_iter()
        .filter(|t| {
            let covered = t.repo.is_some() && whole.contains(&t.owner.to_lowercase());
            if covered {
                ui.warn(&format!(
                    "'{}' is already covered by the scan of {}; counting it once",
                    t.label(),
                    t.owner
                ));
            }
            !covered
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn resolve(args: &[&str]) -> Result<DateRange> {
        let cli = Cli::try_parse_from(["devstats", "acme"].iter().chain(args)).unwrap();
        DateRange::resolve(&cli, d("2026-11-18"))
    }

    #[test]
    fn no_flags_means_the_last_year() {
        let r = resolve(&[]).unwrap();
        assert_eq!(r.period, Some(Period::Year));
        assert_eq!(r.since, Some(d("2025-11-18")));
        assert_eq!(r.until, None);
    }

    #[test]
    fn period_all_has_no_start() {
        let r = resolve(&["--period", "all"]).unwrap();
        assert_eq!(
            (r.since, r.until, r.period),
            (None, None, Some(Period::All))
        );
    }

    #[test]
    fn explicit_dates_skip_the_default_period() {
        let r = resolve(&["--since", "2025-01-01"]).unwrap();
        assert_eq!(
            (r.since, r.until, r.period),
            (Some(d("2025-01-01")), None, None)
        );
        let r = resolve(&["--until", "2025-06-30"]).unwrap();
        assert_eq!(
            (r.since, r.until, r.period),
            (None, Some(d("2025-06-30")), None)
        );
        assert!(resolve(&["--since", "2025-02-01", "--until", "2025-01-01"]).is_err());
    }
}
