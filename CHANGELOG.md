# Changelog

## Unreleased

### Features
- Scan a single repository: pass `https://github.com/OWNER/REPO` (any page of it) or `OWNER/REPO` to rank who contributed to that project. `--period`, `--since` and `--until` apply as usual. The org listing is skipped, a named repository is scanned even when it is a fork or archived, and a repository whose owner is also scanned as a whole is counted once.
- Add `--pr-weeks`: each contributor's pull requests per week (opened, merged, still open, closed), grouped by the week they were opened. The table gets a "pull requests per week" box, TSV prints one `login, week, prs, merged, open, closed` line per contributor and week, the JSON always carries `contributors[].pr_weeks`, and the HTML table gets a "PRs / week" sparkline column with the weekly numbers on hover.
- Break pull requests down per contributor into merged, still open, and closed without merging: new `prs_open` and `prs_closed` fields in the JSON (contributors and totals), two new table columns, the HTML report, and two new TSV fields appended at the end.

### Upgrade notes
- A URL with a repository path, such as `https://github.com/Montimage/mmt-probe`, used to scan the whole `Montimage` organization. It now scans only that repository; pass `https://github.com/Montimage` for the whole organization.
- TSV lines have two more fields at the end (`prs_open`, `prs_closed`).
- The positional argument in `--help` is now `TARGET` instead of `ORG`.

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
