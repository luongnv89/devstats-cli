# devstats

Contribution stats for every contributor across one or more GitHub organizations:
commits, lines added and deleted, pull requests, and issues, ranked in a btop-style
terminal table or written to a single self-contained HTML report.

```text
╭─┐devstats · Montimage, montimage-projects┌──────────────────────────────────────────────────────╮
│ 2 orgs   69 repos   57 contributors   6,206 commits   +22.78M / −12.24M lines                    │
│ 1,474 PRs (1,413 merged)   1,638 issues                                                         │
╰─┘2026-11-18 · last 1 year (since 2025-11-18)└───────────────────────────────────────────────────╯
╭─┐contributors · top 5 by commits┌──────────────────────────────────────────────────────────────╮
│ #  contributor    commits  +lines  −lines    PRs  merged  issues  repos  share                    │
│ 1  luongnv89        2,541   2.87M   2.37M  1,366   1,324   1,580     26  ■■■■······  41%          │
│ 2  nhnghia          1,666   6.57M   3.00M     41      37      12     25  ■■■·······  27%          │
│ 3  strongcourage      706   4.15M  641.1K     10      10       1     11  ■·········  11%          │
│ 4  JeevanPokhrel      300  336.6K   22.6K      0       0       0      2  ··········   5%          │
│ 5  dev-montimage      240  183.0K   46.9K     12      12      13      5  ··········   4%          │
╰─┘57 contributors · sorted by commits · --top 0 shows all└────────────────────────────────────────╯
```

## Install

With Homebrew (macOS and Linux):

```bash
brew install luongnv89/tap/devstats
```

From source (Rust 1.82 or newer):

```bash
cargo install --git https://github.com/luongnv89/devstats-cli
```

Prebuilt binaries for Linux, macOS, and Windows are attached to each tagged GitHub release
(see `.github/workflows/release.yml`).

## Usage

```bash
devstats https://github.com/Montimage
devstats https://github.com/Montimage https://github.com/montimage-projects --top 10 --sort lines
devstats Montimage --period 6m --html report.html
devstats Montimage --period all
devstats Montimage --since 2025-01-01 --until 2025-06-30
devstats Montimage montimage-projects --format json | jq '.contributors[0]'
devstats Montimage --format tsv --top 0 > contributors.tsv
```

Organizations can be given as `https://github.com/NAME`, `github.com/NAME`, or `NAME`.
A user account works too. An unknown name prints a warning and the others still run.

| Option | Default | What it does |
|---|---|---|
| `--html [PATH]` | `devstats-report.html` | Also write the HTML report. Put it after the orgs, or use `--html=PATH` |
| `-f, --format` | `table` | `table`, `json`, or `tsv` on stdout |
| `-s, --sort` | `commits` | `commits`, `additions`, `deletions`, `lines`, `prs`, `issues`, `repos` |
| `-n, --top N` | `25` | Show the top N contributors; `0` shows everyone |
| `-p, --period` | `1y` | Preset period ending today: `all`, `1y`, `6m`, `3m`, `1m`, `1w`, `1d` (see below) |
| `--since` / `--until` | | Limit to an explicit date range (`YYYY-MM-DD`) instead of a preset |
| `--include-forks` | off | Also scan forked repositories |
| `--exclude-archived` | off | Skip archived repositories |
| `--include-bots` | off | Keep `*[bot]` accounts |
| `--per-org` | off | Also print one table per organization |
| `-j, --jobs N` | `6` | Repositories fetched in parallel |
| `--token` | see below | GitHub token |
| `--api-url` | `https://api.github.com` | For GitHub Enterprise (or `DEVSTATS_API_URL`) |
| `--no-cache` | off | Skip the 1-hour response cache |
| `--no-color`, `-q`, `-v` | | No colors (also `NO_COLOR`), quiet, verbose API log |

### Time period

By default devstats reports on the **last 1 year**. Pick another preset with `--period`:

| Preset | Range |
|---|---|
| `all` (or `all-time`) | everything, with no start date |
| `1y` | the last year (default) |
| `6m` | the last 6 months |
| `3m` | the last 3 months |
| `1m` | the last month |
| `1w` | the last week |
| `1d` | the last day |

A preset starts on the same calendar date one period ago (a month-end date clamps, so 6
months before Aug 31 is Feb 28) and ends today. `--since` and `--until` give an explicit
range instead and cannot be combined with `--period`. With only `--until`, the range has no
start date, as before. The period appears in the table header and the HTML report, and as
`range.period` / `range.period_label` in the JSON output. Commits and lines come in weekly
buckets, so `1w` and `1d` count every week that overlaps the period, while pull requests and
issues are counted by exact creation date.

### Token

devstats looks for a token in this order: `--token`, `GITHUB_TOKEN`, `GH_TOKEN`, then
`gh auth token` (for github.com only). Without a token, GitHub allows only 60 requests per
hour. That's rarely enough, so run `gh auth login` or export `GITHUB_TOKEN`. Public
repositories need no scopes.

### Private repositories

An organization's private repositories are scanned when the token can read them. That
needs one of:

- a classic token with the `repo` scope (the token `gh auth login` creates has it), or
- a fine-grained token whose resource owner is the organization, with access to the
  repositories and the **Metadata** repository permission (read). Pull requests and
  issues come from Search, which only returns items the token can read, so also grant
  **Pull requests** (read) and **Issues** (read). Without them, private PRs and issues
  are not counted.

If the organization uses SAML single sign-on, the token must also be authorized for it.

Without a token, or without access, only public repositories are scanned and the report
says so: the table footer reads `public repos only`, the JSON has
`orgs[].private_access` (`included`, `none_visible`, `no_token`, `listing_failed`, or
`user_account`), and the HTML report shows it on each organization card and in the notes.
`none_visible` means the token saw no private repositories: GitHub's listing can't tell
"no access" from "no private repositories". If the full listing fails (for example SSO
not authorized), devstats warns and falls back to the public repositories. The private
count shows up in the repo totals (`totals.repos_private`).

For a user account, GitHub lists only public repositories, so only those are scanned.

### Output

- **Terminal:** a summary box and a ranked table with a share meter and a 52-week braille
  activity graph. The highest value in each metric column is shown in bold cyan, and ties
  are all highlighted; `--no-color` and `NO_COLOR` turn this off with the other colors.
  Columns drop out on narrow terminals. The glyphs fall back to ASCII outside a UTF-8
  locale.
- **Piped, or `--format tsv`:** one contributor per line, tab-separated, with no header and
  no escape codes. The fields are `login, commits, additions, deletions, prs, prs_merged,
  issues, repos, orgs`.
- **`--format json`:** the full report: totals, per-organization breakdowns, the weekly
  series, and the 52-week activity of each contributor.
- **`--html`:** one offline HTML file with KPI tiles, ranked bars with a metric switch, code
  churn, the weekly commit timeline, organization cards, and a sortable, searchable
  contributor table. It has light and dark themes and prints cleanly.

### Exit codes

| Code | Meaning |
|---|---|
| 0 | success |
| 1 | runtime error (network, API, rate limit exhausted, unwritable `--html` path) |
| 2 | usage error (unknown flag, bad value, `--since` after `--until`, `--period` with a date flag) |
| 3 | input error (invalid org name or URL, none of the orgs exist) |
| 130 | interrupted (Ctrl+C) |

### Shell completions

```bash
devstats --completions bash > ~/.local/share/bash-completion/completions/devstats
devstats --completions zsh  > "${fpath[1]}/_devstats"
devstats --completions fish > ~/.config/fish/completions/devstats.fish
```

## How the numbers are counted

- **Commits and lines** come from GitHub's contributor statistics
  (`/repos/{owner}/{repo}/stats/contributors`). They cover the default branch only, and only
  commits linked to a GitHub account. GitHub reports 0 lines for repositories with 10,000 or
  more commits. GitHub computes these stats lazily, so the first run on a cold repository
  can take a minute while devstats retries.
- **Pull requests and issues** come from one org-wide Search query each, counted by author and
  creation date. Search covers every repository the token can read, private ones included. Search returns at most 1,000 results per query, so devstats splits larger
  date ranges automatically. Search is also limited to 30 requests per minute, so large orgs
  can pause briefly.
- Contributors are merged by GitHub login across repositories and organizations. Bots are
  excluded by default.
- Repositories are the organization's public ones plus the private ones the token can read
  (see [Private repositories](#private-repositories)).
- Responses are cached for an hour in `~/.cache/devstats` (or `$XDG_CACHE_HOME/devstats`), so
  re-running with different `--sort`, `--top`, or `--html` options is instant. The cache is
  kept separately per token (keyed by a hash, never the token itself), so one token's private
  data is never served to another token or to a tokenless run.

## Development

```bash
cargo test          # unit tests + integration tests against a mock GitHub API
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

## License

[Apache-2.0](LICENSE)
