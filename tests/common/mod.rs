//! Shared helpers: a clean subprocess environment and a mock GitHub API.
#![allow(dead_code)]

use assert_cmd::Command;
use mockito::{Matcher, Mock, ServerGuard};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// `devstats` with an explicit, reproducible environment (btop-style.md §7).
pub fn devstats() -> Command {
    let mut cmd = Command::cargo_bin("devstats").unwrap();
    for var in [
        "FORCE_COLOR",
        "NO_COLOR",
        "CLICOLOR_FORCE",
        "COLORTERM",
        "COLUMNS",
        "LC_ALL",
        "LC_CTYPE",
        "GITHUB_TOKEN",
        "GH_TOKEN",
        "DEVSTATS_API_URL",
    ] {
        cmd.env_remove(var);
    }
    cmd.env("TERM", "xterm-256color")
        .env("LANG", "C.UTF-8")
        .env("DEVSTATS_NO_GH", "1")
        .env("DEVSTATS_BACKOFF_MS", "1")
        // A fresh cache dir per command: mock servers reuse ports, so a shared cache could leak between tests.
        .env(
            "DEVSTATS_CACHE_DIR",
            std::env::temp_dir().join(format!(
                "devstats-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            )),
        );
    cmd
}

/// Match a path regardless of its query string.
pub fn path(p: &str) -> Matcher {
    let escaped: String = p
        .chars()
        .flat_map(|c| {
            if "[].?+*()".contains(c) {
                vec!['\\', c]
            } else {
                vec![c]
            }
        })
        .collect();
    Matcher::Regex(format!("^{escaped}(\\?.*)?$"))
}

pub fn json_mock(server: &mut ServerGuard, p: &str, status: usize, body: Value) -> Mock {
    server
        .mock("GET", path(p))
        .with_status(status)
        .with_header("content-type", "application/json")
        .with_body(body.to_string())
        .create()
}

/// Unix timestamp of the Sunday starting the week `weeks_ago` weeks before now.
pub fn week(weeks_ago: i64) -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let day = now / 86_400;
    let sunday = day - (day + 4) % 7; // 1970-01-01 was a Thursday
    (sunday - 7 * weeks_ago) * 86_400
}

pub fn author(login: &str, weeks: &[(i64, u64, u64, u64)]) -> Value {
    let kind = if login.ends_with("[bot]") {
        "Bot"
    } else {
        "User"
    };
    json!({
        "author": {"login": login, "type": kind, "avatar_url": format!("https://avatars.example/{login}")},
        "weeks": weeks.iter().map(|&(w, c, a, d)| json!({"w": week(w), "c": c, "a": a, "d": d})).collect::<Vec<_>>()
    })
}

pub fn search_item(login: &str, merged: bool) -> Value {
    json!({
        "user": {"login": login, "type": "User"},
        "created_at": "2026-01-10T10:00:00Z",
        "pull_request": if merged { json!({"merged_at": "2026-01-11T10:00:00Z"}) } else { json!({"merged_at": null}) }
    })
}

/// Org `acme`: repos api, web, and a fork (skipped by default).
/// alice: 12 commits; bob: 5 commits; dependabot[bot]: 40 commits.
pub fn mock_acme(server: &mut ServerGuard) -> Vec<Mock> {
    let base = server.url();
    vec![
        json_mock(server, "/users/acme", 200, json!({
            "login": "acme", "type": "Organization", "name": "Acme Corp",
            "avatar_url": "https://avatars.example/acme", "html_url": "https://github.com/acme"
        })),
        server
            .mock("GET", "/orgs/acme/repos?type=all&per_page=100")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_header("link", &format!("<{base}/orgs/acme/repos?type=all&per_page=100&page=2>; rel=\"next\""))
            .with_body(json!([{"name": "api", "fork": false, "archived": false}, {"name": "fork1", "fork": true, "archived": false}]).to_string())
            .create(),
        server
            .mock("GET", "/orgs/acme/repos?type=all&per_page=100&page=2")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!([{"name": "web", "fork": false, "archived": true}]).to_string())
            .create(),
        json_mock(server, "/repos/acme/api/stats/contributors", 200, json!([
            author("alice", &[(1, 10, 1000, 100)]),
            author("bob", &[(2, 5, 50, 5)]),
            author("dependabot[bot]", &[(1, 40, 400, 40)]),
        ])),
        json_mock(server, "/repos/acme/web/stats/contributors", 200, json!([author("alice", &[(3, 2, 20, 2)])])),
        json_mock(server, "/search/issues", 200, json!({"total_count": 0, "incomplete_results": false, "items": []})),
    ]
}
