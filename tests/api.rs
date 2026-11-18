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
