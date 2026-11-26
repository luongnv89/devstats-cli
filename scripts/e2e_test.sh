#!/usr/bin/env bash
# scripts/e2e_test.sh: end-to-end smoke tests for the devstats binary.
# Builds once, then checks every flag family against the exit-code map in
# src/error.rs (0 ok, 1 runtime, 2 usage, 3 input). Network-free: runtime-error
# cases point --api-url at a closed local port, and HOME, the cache dir and gh's
# config are throwaway dirs, so no token, cache or gh login is used.
# The success path (real API responses) is covered by tests/api.rs via mockito.
set -euo pipefail

cd "$(dirname "$0")/.."
cargo build --locked --quiet
BIN="$PWD/${CARGO_TARGET_DIR:-target}/debug/devstats"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
# DEVSTATS_BACKOFF_MS shrinks the client's retry sleeps (src/github/mod.rs), as tests/api.rs does.
export HOME="$TMP/home" XDG_CACHE_HOME="$TMP/cache" GH_CONFIG_DIR="$TMP/gh" NO_COLOR=1 DEVSTATS_BACKOFF_MS=1
mkdir -p "$HOME"
unset GITHUB_TOKEN GH_TOKEN DEVSTATS_API_URL
DEAD_API="http://127.0.0.1:9"   # discard port: connection refused at once

PASSED=0; FAILED=0
pass() { echo "  ok   $1"; PASSED=$((PASSED + 1)); }
fail() { echo "  FAIL $1"; FAILED=$((FAILED + 1)); }

# expect CODE PATTERN DESC -- ARGS...   (an empty PATTERN skips the output check)
expect() {
  local want="$1" pattern="$2" desc="$3"; shift 4
  local out code
  set +e
  out="$("$BIN" "$@" 2>&1)"; code=$?
  set -e
  if [ "$code" -ne "$want" ]; then
    fail "$desc (exit $code, want $want): devstats $*"
  elif [ -n "$pattern" ] && ! grep -qE -- "$pattern" <<<"$out"; then
    fail "$desc (output lacks /$pattern/): devstats $*"
  else
    pass "$desc"
  fi
}

echo "=== devstats E2E ($BIN, v$VERSION) ==="

echo "-- help and version"
expect 0 "^devstats $VERSION$" "--version"              -- --version
expect 0 "Usage: devstats"     "--help"                 -- --help
expect 0 "Usage: devstats"     "-h"                     -- -h
expect 0 "Examples:"           "--help shows examples"  -- --help

echo "-- shell completions"
for sh in bash zsh fish powershell elvish; do
  expect 0 "include-forks" "--completions $sh" -- --completions "$sh"
done
expect 2 "" "--completions tcsh is rejected" -- --completions tcsh

echo "-- usage errors (exit 2)"
expect 2 "<TARGET>"            "no target"              --
expect 2 ""                    "unknown flag"           -- acme --bogus
expect 2 ""                    "bad --format"           -- acme --format xml
expect 2 ""                    "bad --sort"             -- acme --sort stars
expect 2 "YYYY-MM-DD"          "bad --since date"       -- acme --since 2025-13-01
expect 2 "YYYY-MM-DD"          "bad --until date"       -- acme --until yesterday
expect 2 ""                    "--jobs 0"               -- acme --jobs 0
expect 2 ""                    "--jobs 33"              -- acme --jobs 33
expect 2 "1y"                  "bad --period"           -- acme --period 2y
expect 2 "cannot be used with" "--period with --since"  -- acme --period 6m --since 2025-01-01
expect 2 "cannot be used with" "--quiet with --verbose" -- acme --quiet --verbose
expect 2 "after --until"       "--since after --until"  -- acme --since 2026-02-01 --until 2026-01-01 --api-url "$DEAD_API"

echo "-- input errors (exit 3)"
expect 3 "not a GitHub organization" "junk target" -- "not an org" --api-url "$DEAD_API"

echo "-- runtime errors (exit 1): each target shape and flag parses and reaches the API"
api=(--api-url "$DEAD_API" --no-cache)
expect 1 "" "org name"               -- acme "${api[@]}"
expect 1 "" "org URL"                -- https://github.com/acme "${api[@]}"
expect 1 "" "owner/repo"             -- acme/widgets "${api[@]}"
expect 1 "" "repo URL"               -- https://github.com/acme/widgets "${api[@]}"
expect 1 "" "two orgs with --per-org" -- acme beta --per-org "${api[@]}"
for fmt in table json tsv; do
  expect 1 "" "--format $fmt"        -- acme --format "$fmt" "${api[@]}"
done
for sort in commits additions deletions lines prs issues repos; do
  expect 1 "" "--sort $sort"         -- acme --sort "$sort" "${api[@]}"
done
for p in all 1y 6m 3m 1m 1w 1d; do
  expect 1 "" "--period $p"          -- acme --period "$p" "${api[@]}"
done
expect 1 "" "--since/--until range"  -- acme --since 2025-01-01 --until 2025-06-30 "${api[@]}"
expect 1 "" "every boolean flag"     -- acme --include-forks --exclude-archived --include-bots --pr-weeks --no-color "${api[@]}"
expect 1 "" "--top, --jobs, --token" -- acme --top 0 --jobs 32 --token dummy "${api[@]}"
expect 1 "" "--html=PATH"            -- acme --html="$TMP/report.html" "${api[@]}"
expect 1 "" "--quiet"                -- acme --quiet "${api[@]}"
expect 1 "" "--verbose"              -- acme --verbose "${api[@]}"
DEVSTATS_API_URL="$DEAD_API" expect 1 "" "DEVSTATS_API_URL env" -- acme --no-cache

echo
echo "=== Results: $PASSED passed, $FAILED failed ==="
[ "$FAILED" -eq 0 ]
