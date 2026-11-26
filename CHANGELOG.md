# Changelog

## Unreleased

## v0.3.0 — 2026-11-27

### Features
- Scan a single repository: pass `https://github.com/OWNER/REPO` (any page of it) or `OWNER/REPO` to rank who contributed to that project. `--period`, `--since` and `--until` apply as usual. The org listing is skipped, a named repository is scanned even when it is a fork or archived, and a repository whose owner is also scanned as a whole is counted once. (#11, #10) @luongnv89
- Add `--pr-weeks`: each contributor's pull requests per week (opened, merged, still open, closed), grouped by the week they were opened. The table gets a "pull requests per week" box, TSV prints one `login, week, prs, merged, open, closed` line per contributor and week, the JSON always carries `contributors[].pr_weeks`, and the HTML table gets a "PRs / week" sparkline column with the weekly numbers on hover. (#11, #10) @luongnv89
- Break pull requests down per contributor into merged, still open, and closed without merging: new `prs_open` and `prs_closed` fields in the JSON (contributors and totals), two new table columns, the HTML report, and two new TSV fields appended at the end. (#11, #10) @luongnv89

### Upgrade notes
- A URL with a repository path, such as `https://github.com/Montimage/mmt-probe`, used to scan the whole `Montimage` organization. It now scans only that repository; pass `https://github.com/Montimage` for the whole organization. (#11, #10) @luongnv89
- TSV lines have two more fields at the end (`prs_open`, `prs_closed`). (#11, #10) @luongnv89
- The positional argument in `--help` is now `TARGET` instead of `ORG`. (#11, #10) @luongnv89
- Building from source now needs Rust 1.88 or newer (was 1.82). Run `rustup update` before `cargo install` (#13, #12, #14) @luongnv89

### Documentation
- Add the devstats logo to the README header, with light and dark variants that follow the GitHub theme @luongnv89

### Dependencies
- Update `url` from 2.5.8 to 2.5.9 to fix RUSTSEC-2026-0592 (#13, #12) @luongnv89

### Other Changes
- Add pre-commit and pre-push hooks (`cargo fmt`, clippy, `cargo audit`, unit tests, the full test suite and a network-free end-to-end script), and a leaner CI that verifies the hooks on pull requests and runs the ubuntu/macOS × stable/beta matrix plus the MSRV build on merges to main (#13, #12) @luongnv89
- Fix the release workflow: replace the retired `macos-13` runner with `macos-15-intel`, add a `tag` input to upload binaries to an existing release, and add job timeouts (#13, #12) @luongnv89
- Add CODEOWNERS for the hook config, workflows, scripts, tests and `Cargo.toml` (#13, #12) @luongnv89
- Add the devstats logo set (SVG variants, PNG exports) and a brand showcase page under `assets/logo/` @luongnv89

**Full Changelog**: https://github.com/luongnv89/devstats-cli/compare/v0.2.0...v0.3.0

## v0.2.0 — 2026-11-18

### Features
- Scan an organization's private repositories when the token can read them. Private repos count toward the repo total, contributor totals and every output format (table, JSON, TSV, HTML). When private repos are not included (no token, none visible, failed full listing, or a user account), the scan finishes on public repos and the report says so in the table footer, `orgs[].private_access` in the JSON, the HTML org cards and stderr. The response cache is now kept separately per token (#1, #4) @luongnv89
- Add `-p/--period` with the presets `all` (alias `all-time`), `1y`, `6m`, `3m`, `1m`, `1w` and `1d`. With no `--period`, `--since` or `--until`, devstats now reports on the last 1 year; pass `--period all` for the previous all-time behavior. The selected period appears in the table footer, the HTML report header and the JSON `range` (#2, #5) @luongnv89
- Highlight the top value of each metric column in the table (bold cyan; ties all lead). Respects `--no-color`, `NO_COLOR` and plain output (#3, #6) @luongnv89

### Documentation
- Add example runs for the openai and anthropics organizations (#7, #8) @luongnv89
- Rewrite the README as a landing page with the full reference collapsed (#7, #9) @luongnv89

### Upgrade notes
- The default time window changed from all time to the last 1 year. Scripts that pass no date flags and expect all-time totals should add `--period all`.

**Full Changelog**: https://github.com/luongnv89/devstats-cli/compare/v0.1.1...v0.2.0
