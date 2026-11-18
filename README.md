# devstats

Contribution stats for every contributor across one or more GitHub organizations:
commits, lines added and deleted, pull requests, and issues, ranked in a btop-style
terminal table or written to a single self-contained HTML report.

```text
╭─┐devstats · Montimage, montimage-projects┌──────────────────────────────────────────────────────╮
│ 2 orgs   69 repos   57 contributors   6,206 commits   +22.78M / −12.24M lines                    │
│ 1,474 PRs (1,413 merged)   1,638 issues                                                         │
╰─┘2026-11-18 · all time└─────────────────────────────────────────────────────────────────────────╯
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
devstats Montimage --since 2025-01-01 --html report.html
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
| `--since` / `--until` | all time | Limit to a date range (`YYYY-MM-DD`) |
| `--include-forks` | off | Also scan forked repositories |
| `--exclude-archived` | off | Skip archived repositories |
| `--include-bots` | off | Keep `*[bot]` accounts |
| `--per-org` | off | Also print one table per organization |
| `-j, --jobs N` | `6` | Repositories fetched in parallel |
| `--token` | see below | GitHub token |
| `--api-url` | `https://api.github.com` | For GitHub Enterprise (or `DEVSTATS_API_URL`) |
| `--no-cache` | off | Skip the 1-hour response cache |
| `--no-color`, `-q`, `-v` | | No colors (also `NO_COLOR`), quiet, verbose API log |

### Token

devstats looks for a token in this order: `--token`, `GITHUB_TOKEN`, `GH_TOKEN`, then
`gh auth token` (for github.com only). Without a token, GitHub allows only 60 requests per
hour. That's rarely enough, so run `gh auth login` or export `GITHUB_TOKEN`. Public data
needs no scopes.

### Output

- **Terminal:** a summary box and a ranked table with a share meter and a 52-week braille
  activity graph. Columns drop out on narrow terminals. The glyphs fall back to ASCII
  outside a UTF-8 locale.
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
| 2 | usage error (unknown flag, bad value, `--since` after `--until`) |
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
  creation date. Search returns at most 1,000 results per query, so devstats splits larger
  date ranges automatically. Search is also limited to 30 requests per minute, so large orgs
  can pause briefly.
- Contributors are merged by GitHub login across repositories and organizations. Bots are
  excluded by default.
- Responses are cached for an hour in `~/.cache/devstats` (or `$XDG_CACHE_HOME/devstats`), so
  re-running with different `--sort`, `--top`, or `--html` options is instant.

## Development

```bash
cargo test          # unit tests + integration tests against a mock GitHub API
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

## License

[Apache-2.0](LICENSE)
