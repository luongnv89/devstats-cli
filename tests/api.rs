//! End-to-end runs against a mock GitHub API.

mod common;
use common::*;
use predicates::prelude::*;
use serde_json::{json, Value};

fn run_json(server: &mockito::ServerGuard, extra: &[&str]) -> Value {
    let mut cmd = devstats();
    cmd.args(["acme", "--format", "json", "--api-url", &server.url()])
        .args(extra);
    let out = cmd.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&out).expect("stdout is JSON")
}

#[test]
fn json_report_aggregates_commits_and_lines() {
    let mut server = mockito::Server::new();
    let _m = mock_acme(&mut server);
    let r = run_json(&server, &[]);
    let c = &r["contributors"];
    assert_eq!(c[0]["login"], "alice");
    assert_eq!(c[0]["commits"], 12);
    assert_eq!(c[0]["additions"], 1020);
    assert_eq!(c[0]["repos"], 2);
    assert_eq!(c[1]["login"], "bob");
    assert_eq!(
        c.as_array().unwrap().len(),
        2,
        "bots are excluded by default"
    );
    assert_eq!(r["totals"]["commits"], 17);
    assert_eq!(r["totals"]["repos"], 2, "the fork is skipped");
    assert_eq!(r["totals"]["repos_skipped"], 1);
    assert_eq!(r["orgs"][0]["name"], "Acme Corp");
}

#[test]
fn tsv_has_one_record_per_contributor() {
    let mut server = mockito::Server::new();
    let _m = mock_acme(&mut server);
    devstats()
        .args([
            "https://github.com/acme",
            "--format",
            "tsv",
            "--api-url",
            &server.url(),
        ])
        .assert()
        .success()
        .stdout("alice\t12\t1020\t102\t0\t0\t0\t2\tacme\nbob\t5\t50\t5\t0\t0\t0\t1\tacme\n");
}

#[test]
fn filters_forks_archived_and_bots() {
    let mut server = mockito::Server::new();
    let _m = mock_acme(&mut server);
    let _fork = json_mock(
        &mut server,
        "/repos/acme/fork1/stats/contributors",
        200,
        json!([author("carol", &[(1, 7, 7, 7)])]),
    );
    let r = run_json(
        &server,
        &["--include-forks", "--exclude-archived", "--include-bots"],
    );
    let logins: Vec<_> = r["contributors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["login"].as_str().unwrap())
        .collect();
    assert_eq!(logins, ["dependabot[bot]", "alice", "carol", "bob"]);
    assert_eq!(
        r["contributors"][1]["commits"], 10,
        "archived repo web is skipped"
    );
}

#[test]
fn stats_202_is_retried_until_ready() {
    let mut server = mockito::Server::new();
    let computing = server
        .mock("GET", "/repos/acme/api/stats/contributors")
        .with_status(202)
        .with_body("{}")
        .expect(2)
        .create();
    let _m = mock_acme(&mut server);
    let r = run_json(&server, &[]);
    computing.assert();
    assert_eq!(r["contributors"][0]["commits"], 12);
}

#[test]
fn stats_that_never_finish_warn_and_continue() {
    let mut server = mockito::Server::new();
    // Registered first and expecting every round, so it answers all 9 attempts.
    let still = server
        .mock("GET", "/repos/acme/web/stats/contributors")
        .with_status(202)
        .with_body("{}")
        .expect(9)
        .create();
    let _m = mock_acme(&mut server);
    let mut cmd = devstats();
    cmd.args(["acme", "--format", "json", "--api-url", &server.url()]);
    let out = cmd
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "still computing stats for acme/web",
        ))
        .get_output()
        .stdout
        .clone();
    let r: Value = serde_json::from_slice(&out).unwrap();
    still.assert();
    assert_eq!(r["orgs"][0]["incomplete_repos"][0], "web");
    assert_eq!(r["contributors"][0]["commits"], 10);
}

#[test]
fn missing_org_warns_and_others_continue() {
    let mut server = mockito::Server::new();
    let _m = mock_acme(&mut server);
    let _nf = json_mock(
        &mut server,
        "/users/ghost",
        404,
        json!({"message": "Not Found"}),
    );
    let mut cmd = devstats();
    cmd.args([
        "ghost",
        "acme",
        "--format",
        "tsv",
        "--api-url",
        &server.url(),
    ]);
    cmd.assert()
        .success()
        .stderr(predicate::str::contains("'ghost' was not found"))
        .stdout(predicate::str::starts_with("alice\t"));
}

#[test]
fn no_org_found_exits_3() {
    let mut server = mockito::Server::new();
    let _nf = json_mock(
        &mut server,
        "/users/ghost",
        404,
        json!({"message": "Not Found"}),
    );
    devstats()
        .args(["ghost", "--api-url", &server.url()])
        .assert()
        .code(3)
        .stdout("")
        .stderr(predicate::str::contains("none of the given organizations"));
}

#[test]
fn user_accounts_use_the_users_repo_endpoint() {
    let mut server = mockito::Server::new();
    let _u = json_mock(
        &mut server,
        "/users/solo",
        200,
        json!({"login": "solo", "type": "User", "html_url": "https://github.com/solo"}),
    );
    let _r = json_mock(
        &mut server,
        "/users/solo/repos",
        200,
        json!([{"name": "dots", "fork": false, "archived": false}]),
    );
    let _s = json_mock(
        &mut server,
        "/repos/solo/dots/stats/contributors",
        200,
        json!([author("solo", &[(1, 3, 3, 3)])]),
    );
    let _q = json_mock(
        &mut server,
        "/search/issues",
        200,
        json!({"total_count": 0, "items": []}),
    );
    devstats()
        .args(["solo", "--format", "tsv", "--api-url", &server.url()])
        .assert()
        .success()
        .stdout("solo\t3\t3\t3\t0\t0\t0\t1\tsolo\n");
}

#[test]
fn token_precedence_flag_over_env() {
    let mut server = mockito::Server::new();
    let auth = server
        .mock("GET", "/users/acme")
        .match_header("authorization", "Bearer from-flag")
        .with_status(200)
        .with_body(
            json!({"login": "acme", "type": "Organization", "html_url": "https://github.com/acme"})
                .to_string(),
        )
        .expect(1)
        .create();
    let _m = mock_acme(&mut server);
    devstats()
        .env("GITHUB_TOKEN", "from-env")
        .args([
            "acme",
            "--token",
            "from-flag",
            "--format",
            "tsv",
            "--api-url",
            &server.url(),
        ])
        .assert()
        .success();
    auth.assert();
}

#[test]
fn bad_credentials_exit_1_with_hint() {
    let mut server = mockito::Server::new();
    let _m = json_mock(
        &mut server,
        "/users/acme",
        401,
        json!({"message": "Bad credentials"}),
    );
    devstats()
        .env("GITHUB_TOKEN", "nope")
        .args(["acme", "--api-url", &server.url()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("Bad credentials").and(predicate::str::contains("hint:")));
}

#[test]
fn exhausted_rate_limit_exits_1() {
    let mut server = mockito::Server::new();
    let far = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    let _m = server
        .mock("GET", "/users/acme")
        .with_status(403)
        .with_header("x-ratelimit-remaining", "0")
        .with_header("x-ratelimit-reset", &far.to_string())
        .with_body(r#"{"message":"API rate limit exceeded"}"#)
        .create();
    devstats()
        .args(["acme", "--api-url", &server.url()])
        .assert()
        .code(1)
        .stderr(
            predicate::str::contains("rate limit exhausted")
                .and(predicate::str::contains("GITHUB_TOKEN")),
        );
}

#[test]
fn short_rate_limit_is_waited_out() {
    let mut server = mockito::Server::new();
    let limited = server
        .mock("GET", "/users/acme")
        .with_status(403)
        .with_header("retry-after", "2")
        .with_body(r#"{"message":"You have exceeded a secondary rate limit"}"#)
        .expect(1)
        .create();
    let _m = mock_acme(&mut server);
    devstats()
        .args(["acme", "--format", "tsv", "--api-url", &server.url()])
        .assert()
        .success()
        .stderr(predicate::str::contains("waiting 2s"));
    limited.assert();
}

#[test]
fn search_counts_prs_and_issues_and_splits_big_ranges() {
    let mut server = mockito::Server::new();
    let today = chrono::Utc::now().date_naive();
    let full = server
        .mock(
            "GET",
            mockito::Matcher::Regex(format!(r"is:pr\+created:2008-01-01\.\.{today}&")),
        )
        .with_status(200)
        .with_body(json!({"total_count": 1500, "items": []}).to_string())
        .expect(1)
        .create();
    let halves = server
        .mock("GET", mockito::Matcher::Regex(r"^/search/issues\?q=org:acme\+is:pr".into()))
        .with_status(200)
        .with_body(json!({"total_count": 2, "items": [search_item("alice", true), search_item("renovate[bot]", true)]}).to_string())
        .expect(2)
        .create();
    let issues = server
        .mock(
            "GET",
            mockito::Matcher::Regex(r"^/search/issues\?q=org:acme\+is:issue".into()),
        )
        .with_status(200)
        .with_body(json!({"total_count": 1, "items": [search_item("carol", false)]}).to_string())
        .create();
    let _m = mock_acme(&mut server);
    let r = run_json(&server, &[]);
    full.assert();
    halves.assert();
    issues.assert();
    let alice = r["contributors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["login"] == "alice")
        .unwrap();
    assert_eq!(
        (alice["prs"].as_u64(), alice["prs_merged"].as_u64()),
        (Some(2), Some(2))
    );
    let carol = r["contributors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["login"] == "carol")
        .unwrap();
    assert_eq!(carol["issues"], 1);
    assert_eq!(carol["commits"], 0);
    assert_eq!(r["totals"]["prs"], 2, "bot PRs are excluded");
}

#[test]
fn html_report_is_self_contained_and_escapes_data() {
    let mut server = mockito::Server::new();
    let _s = json_mock(
        &mut server,
        "/repos/acme/web/stats/contributors",
        200,
        json!([author(
            "x</script><img src=x onerror=alert(1)>",
            &[(1, 1, 1, 1)]
        )]),
    );
    let _m = mock_acme(&mut server);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.html");
    devstats()
        .args(["acme", "--format", "tsv", "--api-url", &server.url()])
        .arg(format!("--html={}", path.display()))
        .assert()
        .success()
        .stderr(predicate::str::contains("wrote"))
        .stdout(predicate::str::starts_with("alice\t"));
    let html = std::fs::read_to_string(&path).unwrap();
    assert!(html.starts_with("<!doctype html>"));
    assert!(!html.contains("__DEVSTATS_DATA__"));
    assert!(
        !html.contains("<script src") && !html.contains("<link rel=\"stylesheet\""),
        "no external assets"
    );
    assert_eq!(
        html.matches("</script>").count(),
        2,
        "embedded data cannot close the script tag"
    );
    let start = html.find(r#"id="devstats-data">"#).unwrap() + r#"id="devstats-data">"#.len();
    let end = start + html[start..].find("</script>").unwrap();
    let data: Value = serde_json::from_str(&html[start..end]).expect("embedded JSON parses");
    assert_eq!(data["contributors"][0]["login"], "alice");
    assert!(data["contributors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["login"].as_str().unwrap().contains("</script>")));
}

#[test]
fn html_default_path_and_unwritable_path() {
    let mut server = mockito::Server::new();
    let _m = mock_acme(&mut server);
    let dir = tempfile::tempdir().unwrap();
    devstats()
        .current_dir(dir.path())
        .args([
            "acme",
            "--format",
            "tsv",
            "--api-url",
            &server.url(),
            "--html",
        ])
        .assert()
        .success();
    assert!(dir.path().join("devstats-report.html").exists());
    devstats()
        .args([
            "acme",
            "--api-url",
            &server.url(),
            "--html=/nonexistent-dir/x.html",
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("cannot write the HTML report"));
}

#[test]
fn second_run_is_served_from_the_cache_unless_no_cache() {
    let mut server = mockito::Server::new();
    let owner = server
        .mock("GET", "/users/acme")
        .with_status(200)
        .with_body(
            json!({"login": "acme", "type": "Organization", "html_url": "https://github.com/acme"})
                .to_string(),
        )
        .expect(2)
        .create();
    let _m = mock_acme(&mut server);
    let cache = tempfile::tempdir().unwrap();
    let run = |extra: &[&str]| {
        devstats()
            .env("DEVSTATS_CACHE_DIR", cache.path())
            .args(["acme", "--format", "tsv", "--api-url", &server.url()])
            .args(extra)
            .assert()
            .success()
            .stdout(predicate::str::starts_with("alice\t12\t"));
    };
    run(&[]); // network
    run(&[]); // cache
    run(&["--no-cache"]); // network again
    owner.assert();
    assert!(std::fs::read_dir(cache.path()).unwrap().count() >= 5);
}

const TOKEN: &str = "s3cret-test-token";

/// With `TOKEN`, acme's full listing also has the private repo `vault`
/// (alice +7 commits, carol 4). Register it before `mock_acme`.
fn mock_private_vault(server: &mut mockito::ServerGuard) -> Vec<mockito::Mock> {
    let base = server.url();
    vec![
        server
            .mock("GET", "/orgs/acme/repos?type=all&per_page=100")
            .match_header("authorization", format!("Bearer {TOKEN}").as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_header("link", &format!("<{base}/orgs/acme/repos?type=all&per_page=100&page=2>; rel=\"next\""))
            .with_body(json!([
                {"name": "api", "fork": false, "archived": false, "private": false},
                {"name": "fork1", "fork": true, "archived": false, "private": false},
                {"name": "vault", "fork": false, "archived": false, "private": true, "visibility": "private"},
            ]).to_string())
            .expect(1)
            .create(),
        json_mock(server, "/repos/acme/vault/stats/contributors", 200, json!([
            author("alice", &[(1, 7, 70, 7)]),
            author("carol", &[(2, 4, 40, 4)]),
        ])),
    ]
}

fn notes(r: &Value) -> Vec<String> {
    r["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn private_repos_the_token_can_read_are_scanned() {
    let mut server = mockito::Server::new();
    let private = mock_private_vault(&mut server);
    let _m = mock_acme(&mut server);
    let r = run_json(&server, &["--token", TOKEN]);
    private[0].assert();
    assert_eq!(r["totals"]["repos"], 3, "api, web and the private vault");
    assert_eq!(r["totals"]["repos_private"], 1);
    assert_eq!(r["totals"]["commits"], 28);
    assert_eq!(r["contributors"][0]["login"], "alice");
    assert_eq!(r["contributors"][0]["commits"], 19);
    assert_eq!(r["contributors"][0]["repos"], 3);
    assert!(r["contributors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["login"] == "carol" && c["commits"] == 4));
    let org = &r["orgs"][0];
    assert_eq!(org["private_access"], "included");
    assert_eq!(org["private_repos"], 1);
    assert_eq!(org["totals"]["repos_private"], 1);
    assert!(!notes(&r).iter().any(|n| n.contains("not included")));
}

#[test]
fn private_repos_reach_tsv_table_and_html() {
    let mut server = mockito::Server::new();
    let _p = mock_private_vault(&mut server);
    let _m = mock_acme(&mut server);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.html");
    devstats()
        .args([
            "acme",
            "--token",
            TOKEN,
            "--format",
            "tsv",
            "--api-url",
            &server.url(),
        ])
        .arg(format!("--html={}", path.display()))
        .assert()
        .success()
        .stdout(
            "alice\t19\t1090\t109\t0\t0\t0\t3\tacme\n\
             bob\t5\t50\t5\t0\t0\t0\t1\tacme\n\
             carol\t4\t40\t4\t0\t0\t0\t1\tacme\n",
        );
    let html = std::fs::read_to_string(&path).unwrap();
    let start = html.find(r#"id="devstats-data">"#).unwrap() + r#"id="devstats-data">"#.len();
    let end = start + html[start..].find("</script>").unwrap();
    let data: Value = serde_json::from_str(&html[start..end]).unwrap();
    assert_eq!(data["totals"]["repos_private"], 1);
    assert_eq!(data["orgs"][0]["private_access"], "included");
    assert_eq!(data["orgs"][0]["private_repos"], 1);

    let mut server = mockito::Server::new();
    let _p = mock_private_vault(&mut server);
    let _m = mock_acme(&mut server);
    let out = devstats()
        .env("FORCE_COLOR", "1")
        .env("NO_COLOR", "1")
        .env("COLUMNS", "200")
        .args(["acme", "--token", TOKEN, "--api-url", &server.url()])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let table = String::from_utf8(out).unwrap();
    assert!(table.contains("3 repos (1 private)"), "{table}");
    assert!(table.contains("carol"), "{table}");
    assert!(!table.contains("public repos only"), "{table}");
}

#[test]
fn tokenless_run_reports_public_repos_only() {
    let mut server = mockito::Server::new();
    let _m = mock_acme(&mut server);
    let mut cmd = devstats();
    cmd.args([
        "acme",
        "--quiet",
        "--format",
        "json",
        "--api-url",
        &server.url(),
    ]);
    let out = cmd
        .assert()
        .success()
        .stderr("")
        .get_output()
        .stdout
        .clone();
    let r: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(r["orgs"][0]["private_access"], "no_token");
    assert_eq!(r["totals"]["repos_private"], 0);
    assert!(
        notes(&r).contains(&"acme: private repositories not included (no token).".to_string()),
        "{:?}",
        notes(&r)
    );
    let out = devstats()
        .env("FORCE_COLOR", "1")
        .env("NO_COLOR", "1")
        .env("COLUMNS", "200")
        .args(["acme", "--quiet", "--api-url", &server.url()])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let table = String::from_utf8(out).unwrap();
    assert!(table.contains("all time · public repos only"), "{table}");
}

#[test]
fn token_without_private_access_reports_none_visible() {
    let mut server = mockito::Server::new();
    let _m = mock_acme(&mut server);
    let mut cmd = devstats();
    cmd.args([
        "acme",
        "--token",
        TOKEN,
        "--format",
        "json",
        "--api-url",
        &server.url(),
    ]);
    let out = cmd
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "acme: no private repositories visible to this token",
        ))
        .get_output()
        .stdout
        .clone();
    let r: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(r["orgs"][0]["private_access"], "none_visible");
    assert_eq!(r["totals"]["repos"], 2);
    assert!(notes(&r).contains(
        &"acme: private repositories not included (none visible to this token).".to_string()
    ));
}

#[test]
fn failed_full_listing_falls_back_to_public_repos() {
    let mut server = mockito::Server::new();
    let denied = server
        .mock("GET", "/orgs/acme/repos?type=all&per_page=100")
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"message":"Resource protected by organization SAML enforcement."}"#)
        .expect(1)
        .create();
    let public = server
        .mock("GET", "/orgs/acme/repos?type=public&per_page=100")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!([
                {"name": "api", "fork": false, "archived": false},
                {"name": "fork1", "fork": true, "archived": false},
                {"name": "web", "fork": false, "archived": true},
            ])
            .to_string(),
        )
        .expect(1)
        .create();
    let _m = mock_acme(&mut server);
    let mut cmd = devstats();
    cmd.args([
        "acme",
        "--token",
        TOKEN,
        "--format",
        "json",
        "--api-url",
        &server.url(),
    ]);
    let out = cmd
        .assert()
        .success()
        .stderr(
            predicate::str::contains("acme: the full repository listing failed (HTTP 403)").and(
                predicate::str::contains("only public repositories are scanned"),
            ),
        )
        .get_output()
        .stdout
        .clone();
    denied.assert();
    public.assert();
    let r: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(r["orgs"][0]["private_access"], "listing_failed");
    assert_eq!(r["totals"]["repos"], 2);
    assert_eq!(r["contributors"][0]["commits"], 12);
    assert!(notes(&r).iter().any(|n| n
        == "acme: private repositories not included (the full repository listing failed with HTTP 403; public repositories only)."));
}

#[test]
fn rate_limit_on_the_listing_still_aborts() {
    let mut server = mockito::Server::new();
    let far = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    let _limited = server
        .mock("GET", "/orgs/acme/repos?type=all&per_page=100")
        .with_status(403)
        .with_header("x-ratelimit-remaining", "0")
        .with_header("x-ratelimit-reset", &far.to_string())
        .with_body(r#"{"message":"API rate limit exceeded"}"#)
        .create();
    let _m = mock_acme(&mut server);
    devstats()
        .args(["acme", "--api-url", &server.url()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("rate limit exhausted"));
}

#[test]
fn persistent_server_error_on_the_listing_still_aborts() {
    let mut server = mockito::Server::new();
    // Every attempt (the first plus the client's retries) answers 502.
    let down = server
        .mock("GET", "/orgs/acme/repos?type=all&per_page=100")
        .with_status(502)
        .with_header("content-type", "application/json")
        .with_body(r#"{"message":"Bad Gateway"}"#)
        .expect(4)
        .create();
    let public = server
        .mock("GET", "/orgs/acme/repos?type=public&per_page=100")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body("[]")
        .expect(0)
        .create();
    let _m = mock_acme(&mut server);
    devstats()
        .args(["acme", "--token", TOKEN, "--api-url", &server.url()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("GitHub API returned 502"));
    down.assert();
    public.assert();
}

#[test]
fn failure_on_a_later_page_falls_back_without_mixing_results() {
    let mut server = mockito::Server::new();
    let base = server.url();
    let page1 = server
        .mock("GET", "/orgs/acme/repos?type=all&per_page=100")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_header(
            "link",
            &format!("<{base}/orgs/acme/repos?type=all&per_page=100&page=2>; rel=\"next\""),
        )
        .with_body(
            json!([
                {"name": "api", "fork": false, "archived": false, "private": false},
                {"name": "extra", "fork": false, "archived": false, "private": false},
                {"name": "vault", "fork": false, "archived": false, "private": true, "visibility": "private"},
            ])
            .to_string(),
        )
        .expect(1)
        .create();
    let page2 = server
        .mock("GET", "/orgs/acme/repos?type=all&per_page=100&page=2")
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"message":"Resource protected by organization SAML enforcement."}"#)
        .expect(1)
        .create();
    let public = server
        .mock("GET", "/orgs/acme/repos?type=public&per_page=100")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            json!([
                {"name": "api", "fork": false, "archived": false},
                {"name": "web", "fork": false, "archived": true},
            ])
            .to_string(),
        )
        .expect(1)
        .create();
    let page1_only: Vec<_> = ["extra", "vault"]
        .iter()
        .map(|name| {
            server
                .mock(
                    "GET",
                    path(&format!("/repos/acme/{name}/stats/contributors")),
                )
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body("[]")
                .expect(0)
                .create()
        })
        .collect();
    let _m = mock_acme(&mut server);
    let r = run_json(&server, &["--token", TOKEN]);
    page1.assert();
    page2.assert();
    public.assert();
    for m in &page1_only {
        m.assert();
    }
    assert_eq!(r["orgs"][0]["private_access"], "listing_failed");
    assert_eq!(r["totals"]["repos_private"], 0);
    assert_eq!(r["totals"]["repos"], 2, "only the type=public listing");
    assert_eq!(r["contributors"][0]["commits"], 12);
}

#[test]
fn cache_is_not_shared_between_tokenless_and_token_runs() {
    let mut server = mockito::Server::new();
    let anon_listing = server
        .mock("GET", "/orgs/acme/repos?type=all&per_page=100")
        .match_header("authorization", mockito::Matcher::Missing)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!([{"name": "api", "fork": false, "archived": false}]).to_string())
        .expect(1)
        .create();
    let token_listing = mock_private_vault(&mut server);
    let _m = mock_acme(&mut server);
    let cache = tempfile::tempdir().unwrap();
    let run = |token: Option<&str>| -> Value {
        let mut cmd = devstats();
        cmd.env("DEVSTATS_CACHE_DIR", cache.path()).args([
            "acme",
            "--format",
            "json",
            "--api-url",
            &server.url(),
        ]);
        if let Some(t) = token {
            cmd.args(["--token", t]);
        }
        serde_json::from_slice(&cmd.assert().success().get_output().stdout).unwrap()
    };
    let anon = run(None);
    assert_eq!(anon["totals"]["repos"], 1);
    assert_eq!(anon["orgs"][0]["private_access"], "no_token");
    let with_token = run(Some(TOKEN));
    anon_listing.assert();
    token_listing[0].assert();
    assert_eq!(
        with_token["totals"]["repos_private"], 1,
        "not the tokenless listing"
    );
    assert_eq!(with_token["orgs"][0]["private_access"], "included");
    for entry in std::fs::read_dir(cache.path()).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains(TOKEN), "token written to {}", path.display());
        assert!(!path.to_string_lossy().contains(TOKEN));
    }
}
