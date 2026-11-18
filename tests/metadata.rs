//! Release metadata stays in sync with Cargo.toml.

#[test]
fn homebrew_formula_points_at_this_version() {
    let formula = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/packaging/homebrew/devstats.rb"
    ))
    .unwrap();
    let url = format!(
        "url \"https://github.com/luongnv89/devstats-cli/archive/refs/tags/v{}.tar.gz\"",
        env!("CARGO_PKG_VERSION")
    );
    assert!(formula.contains(&url), "formula url must be: {url}");
    assert!(formula.contains("license \"Apache-2.0\""));
}

#[test]
fn license_matches_cargo() {
    assert_eq!(env!("CARGO_PKG_LICENSE"), "Apache-2.0");
    let license = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/LICENSE")).unwrap();
    assert!(license.contains("Apache License") && license.contains("Version 2.0"));
}
