//! btop-style verification (btop-style.md §7) plus table layout checks.

mod common;
use common::*;
use serde_json::{json, Value};
use unicode_width::UnicodeWidthStr;

const ESC: u8 = 0x1b;

fn stdout_of(server: &mockito::ServerGuard, env: &[(&str, &str)], args: &[&str]) -> Vec<u8> {
    let mut cmd = devstats();
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.args(["acme", "--api-url", &server.url()]).args(args);
    cmd.assert().success().get_output().stdout.clone()
}

#[test]
fn t1_captured_stdout_is_plain() {
    let mut server = mockito::Server::new();
    let _m = mock_acme(&mut server);
    let out = stdout_of(&server, &[], &[]);
    assert!(!out.contains(&ESC));
    assert!(!String::from_utf8(out.clone()).unwrap().contains('╭'));
    assert!(String::from_utf8(out).unwrap().starts_with("alice\t12\t"));
}

#[test]
fn t2_json_ignores_force_color() {
    let mut server = mockito::Server::new();
    let _m = mock_acme(&mut server);
    let out = stdout_of(&server, &[("FORCE_COLOR", "1")], &["--format", "json"]);
    assert!(!out.contains(&ESC));
    serde_json::from_slice::<Value>(&out).expect("valid JSON");
}

#[test]
fn t3_no_color_wins_over_force_color() {
    let mut server = mockito::Server::new();
    let _m = mock_acme(&mut server);
    for env in [
        vec![("FORCE_COLOR", "1"), ("NO_COLOR", "1")],
        vec![("FORCE_COLOR", "1")],
    ] {
        let mut args = vec![];
        if env.len() == 1 {
            args.push("--no-color");
        }
        let out = String::from_utf8(stdout_of(&server, &env, &args)).unwrap();
        assert!(!out.as_bytes().contains(&ESC));
        assert!(out.contains("╭─┐devstats · acme┌"), "{out}");
    }
}

#[test]
fn t4_force_color_styles_the_box() {
    let mut server = mockito::Server::new();
    let _m = mock_acme(&mut server);
    let out = String::from_utf8(stdout_of(&server, &[("FORCE_COLOR", "1")], &[])).unwrap();
    assert!(out.as_bytes().contains(&ESC));
    assert!(out.contains("╭─┐"));
    assert!(out.contains("alice"));
}

#[test]
fn t8_control_characters_are_escaped() {
    let mut server = mockito::Server::new();
    let _s = json_mock(
        &mut server,
        "/repos/acme/api/stats/contributors",
        200,
        json!([author("ev\u{1b}[31mil\tx", &[(1, 9, 9, 9)])]),
    );
    let _m = mock_acme(&mut server);
    let plain = String::from_utf8(stdout_of(&server, &[], &["--format", "tsv"])).unwrap();
    assert!(!plain.as_bytes().contains(&ESC));
    let first = plain.lines().next().unwrap();
    assert_eq!(first.split('\t').count(), 11, "field count kept: {first}");
    assert!(first.starts_with("ev\\x1b[31mil\\tx\t"));
    let styled = stdout_of(&server, &[("FORCE_COLOR", "1"), ("NO_COLOR", "1")], &[]);
    assert!(!styled.contains(&ESC));
}

#[test]
fn table_fits_narrow_terminals_and_ascii_locales() {
    let mut server = mockito::Server::new();
    let _m = mock_acme(&mut server);
    for cols in ["60", "80", "200"] {
        let out = String::from_utf8(stdout_of(
            &server,
            &[("FORCE_COLOR", "1"), ("NO_COLOR", "1"), ("COLUMNS", cols)],
            &[],
        ))
        .unwrap();
        let max: usize = cols.parse().unwrap();
        let widths: Vec<usize> = out.lines().map(UnicodeWidthStr::width).collect();
        assert!(widths.iter().all(|w| *w <= max), "COLUMNS={cols}: {out}");
        assert!(
            widths.windows(2).all(|p| p[0] == p[1]),
            "all box lines share one width: {out}"
        );
    }
    let ascii = String::from_utf8(stdout_of(
        &server,
        &[("FORCE_COLOR", "1"), ("NO_COLOR", "1"), ("LANG", "C")],
        &[],
    ))
    .unwrap();
    assert!(ascii.is_ascii(), "{ascii}");
    assert!(ascii.starts_with("+-[devstats - acme]"));
}

#[test]
fn per_org_adds_one_box_per_org() {
    let mut server = mockito::Server::new();
    let _m = mock_acme(&mut server);
    let out = String::from_utf8(stdout_of(
        &server,
        &[("FORCE_COLOR", "1"), ("NO_COLOR", "1")],
        &["--per-org"],
    ))
    .unwrap();
    assert_eq!(out.matches("╭─┐").count(), 3, "{out}");
    assert!(out.contains("acme · top 2 by commits"));
}

#[test]
fn top_values_are_highlighted_unless_color_is_off() {
    use devstats::style::theme::{style, Role};
    use devstats::style::Tier;
    let mut server = mockito::Server::new();
    let _m = mock_acme(&mut server);
    // TERM=xterm-256color: the 256-color form of the lead role.
    let lead = style(Role::Lead, Tier::Ansi256).render().to_string();
    let out = String::from_utf8(stdout_of(&server, &[("FORCE_COLOR", "1")], &[])).unwrap();
    // alice leads +lines (1,020) and repos (2); her row carries the highlight.
    let alice = out.lines().find(|l| l.contains("alice")).unwrap();
    assert!(alice.contains(&format!("{lead}1,020")), "{alice}");
    for env in [
        vec![("FORCE_COLOR", "1"), ("NO_COLOR", "1")],
        vec![("FORCE_COLOR", "1")],
    ] {
        let args: Vec<&str> = if env.len() == 1 {
            vec!["--no-color"]
        } else {
            vec![]
        };
        let out = String::from_utf8(stdout_of(&server, &env, &args)).unwrap();
        assert!(!out.contains(&lead), "{out}");
        assert!(!out.as_bytes().contains(&ESC), "{out}");
    }
}
