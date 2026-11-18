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

use chrono::Utc;
use clap::{CommandFactory, FromArgMatches};

use crate::cli::{Cli, Format};
use crate::error::{AppError, Result};
use crate::github::{repos, search, stats, Client, DEFAULT_API};
use crate::model::{OrgData, PrivateAccess};
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

fn execute(cli: &Cli, ui: Arc<Reporter>, out_mode: Mode) -> Result<()> {
    if let (Some(s), Some(u)) = (cli.since, cli.until) {
        if s > u {
            return Err(AppError::usage(format!("--since {s} is after --until {u}"))
                .hint("swap the two dates"));
        }
    }
    let logins = org::parse_all(&cli.orgs)?;

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
            for login in &logins {
                if let Some(d) = collect_org(&client, login, cli)? {
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
            AppError::input("none of the given organizations exist on GitHub")
                .hint("check the spelling, e.g. https://github.com/Montimage"),
        );
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
        Format::Tsv => output::tsv::render(&report),
        Format::Table if out_mode.styled => output::table::render(&report, &out_mode, cli.per_org),
        Format::Table => output::tsv::render(&report),
    };
    if let Some(path) = &cli.html {
        output::html::write(&report, path)?;
        ui.info(&format!("wrote {}", path.display()));
    }
    write_stdout(&text)
}

/// Fetch everything for one org. `Ok(None)` (after a warning) when it does not exist.
fn collect_org(client: &Client, login: &str, cli: &Cli) -> Result<Option<OrgData>> {
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
        PrivateAccess::Included | PrivateAccess::NoToken => {}
    }
    client.ui.progress(&owner.login, 0, Some(scanned.len()));

    let from = cli.since.unwrap_or_else(search::epoch);
    let to = cli.until.unwrap_or_else(|| Utc::now().date_naive());
    // Search (rate-limited to 30/min) runs beside the stats workers.
    let (repo_stats, items) = std::thread::scope(|s| {
        let search = s.spawn(|| -> Result<Vec<model::Item>> {
            let mut items = search::fetch(client, &owner.login, owner.is_org, true, from, to)?;
            items.extend(search::fetch(
                client,
                &owner.login,
                owner.is_org,
                false,
                from,
                to,
            )?);
            Ok(items)
        });
        let on_done = |n: usize| client.ui.progress(&owner.login, n, Some(scanned.len()));
        let st = stats::fetch_all(client, &owner.login, &scanned, cli.jobs as usize, &on_done);
        if !search.is_finished() {
            client
                .ui
                .progress_note(&owner.login, "counting pull requests and issues");
        }
        (
            st,
            search
                .join()
                .unwrap_or_else(|_| Err(AppError::runtime("search worker crashed"))),
        )
    });
    Ok(Some(OrgData {
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
