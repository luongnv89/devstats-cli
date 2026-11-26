//! Usage-level behavior: help, version, exit codes for bad input. No network.

mod common;
use common::devstats;
use predicates::prelude::*;

#[test]
fn help_lists_every_option() {
    let out = devstats()
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let help = String::from_utf8(out).unwrap();
    for opt in [
        "--html",
        "--format",
        "--sort",
        "--top",
        "--period",
        "--since",
        "--until",
        "--include-forks",
        "--exclude-archived",
        "--include-bots",
        "--per-org",
        "--pr-weeks",
        "--jobs",
        "--token",
        "--api-url",
        "--no-color",
        "--quiet",
        "--verbose",
        "--version",
    ] {
        assert!(help.contains(opt), "--help is missing {opt}");
    }
    assert!(help.contains("Examples:"));
    for preset in ["all", "1y", "6m", "3m", "1m", "1w", "1d"] {
        assert!(
            help.contains(&format!("{preset}:")),
            "--help is missing the {preset} preset"
        );
    }
    assert!(
        help.contains("default: 1y"),
        "--help must state the 1y default"
    );
}

#[test]
fn short_help_works() {
    devstats()
        .arg("-h")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: devstats"));
}

#[test]
fn version_matches_cargo() {
    devstats()
        .arg("--version")
        .assert()
        .success()
        .stdout(format!("devstats {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn missing_org_is_a_usage_error() {
    devstats()
        .assert()
        .code(2)
        .stderr(predicate::str::contains("<TARGET>"));
}

#[test]
fn unknown_flag_is_a_usage_error() {
    devstats().args(["acme", "--bogus"]).assert().code(2);
}

#[test]
fn bad_enum_and_date_values_are_usage_errors() {
    devstats()
        .args(["acme", "--format", "xml"])
        .assert()
        .code(2);
    devstats()
        .args(["acme", "--sort", "stars"])
        .assert()
        .code(2);
    devstats()
        .args(["acme", "--since", "2025-13-01"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("YYYY-MM-DD"));
    devstats().args(["acme", "--jobs", "0"]).assert().code(2);
    devstats()
        .args(["acme", "--period", "2y"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("1y"));
}

#[test]
fn period_conflicts_with_explicit_dates() {
    for flag in ["--since", "--until"] {
        devstats()
            .args(["acme", "--period", "6m", flag, "2025-01-01"])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("cannot be used with"));
    }
}

#[test]
fn since_after_until_is_a_usage_error() {
    devstats()
        .args([
            "acme",
            "--since",
            "2026-02-01",
            "--until",
            "2026-01-01",
            "--api-url",
            "http://127.0.0.1:9",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("after --until"));
}

#[test]
fn junk_org_is_an_input_error_with_a_hint() {
    devstats()
        .args(["not an org", "--api-url", "http://127.0.0.1:9"])
        .assert()
        .code(3)
        .stdout("")
        .stderr(
            predicate::str::contains("not a GitHub organization")
                .and(predicate::str::contains("hint:")),
        );
}

#[test]
fn completions_for_each_shell() {
    for shell in ["bash", "zsh", "fish", "powershell", "elvish"] {
        let out = devstats()
            .args(["--completions", shell])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("devstats") && text.contains("include-forks"),
            "{shell}"
        );
    }
    devstats().args(["--completions", "tcsh"]).assert().code(2);
}

#[test]
fn completions_flag_is_hidden_but_no_cache_is_listed() {
    devstats().arg("--help").assert().success().stdout(
        predicate::str::contains("--no-cache").and(predicate::str::contains("--completions").not()),
    );
}

#[test]
fn readme_documents_every_preset_and_the_default() {
    let readme =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md")).unwrap();
    assert!(
        readme.contains("| `-p, --period` | `1y` |"),
        "options table"
    );
    for preset in ["all", "1y", "6m", "3m", "1m", "1w", "1d"] {
        assert!(
            readme.contains(&format!("| `{preset}`")),
            "README is missing {preset}"
        );
    }
}
