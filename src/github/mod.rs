//! Minimal GitHub REST client: auth, `Link` pagination, rate-limit waits, retries.

pub mod cache;
pub mod repos;
pub mod search;
pub mod stats;

use std::io::Read;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::error::{AppError, Result};
use crate::ui::Reporter;

pub const DEFAULT_API: &str = "https://api.github.com";
/// Longest rate-limit wait devstats sits through before giving up.
const MAX_RATE_WAIT: u64 = 120;

pub struct Client {
    agent: ureq::Agent,
    base: String,
    token: Option<String>,
    pub ui: Arc<Reporter>,
    /// Unit for backoff sleeps; tests shrink it through DEVSTATS_BACKOFF_MS.
    pub backoff: Duration,
    cache: Option<cache::Cache>,
}

#[derive(Debug)]
pub struct Resp {
    pub status: u16,
    pub body: Value,
    pub next: Option<String>,
}

impl Client {
    pub fn new(base: &str, token: Option<String>, ui: Arc<Reporter>, use_cache: bool) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(15))
            .timeout_read(Duration::from_secs(90))
            .user_agent(concat!("devstats/", env!("CARGO_PKG_VERSION")))
            .build();
        let backoff = std::env::var("DEVSTATS_BACKOFF_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .map(Duration::from_millis)
            .unwrap_or(Duration::from_secs(1));
        let cache = if use_cache {
            cache::Cache::from_env()
        } else {
            None
        };
        if let Some(c) = &cache {
            ui.debug(&format!("cache: {}", c.dir().display()));
        }
        Client {
            agent,
            base: base.trim_end_matches('/').to_string(),
            token,
            ui,
            backoff,
            cache,
        }
    }

    pub fn url(&self, path_or_url: &str) -> String {
        if path_or_url.starts_with("http://") || path_or_url.starts_with("https://") {
            path_or_url.to_string()
        } else {
            format!("{}{}", self.base, path_or_url)
        }
    }

    /// GET one URL. Returns the response for every HTTP status; callers decide what
    /// 202/204/404 mean. Fails only on network errors, repeated 5xx, or a rate limit
    /// that would take longer than `MAX_RATE_WAIT` to reset.
    pub fn get(&self, path_or_url: &str) -> Result<Resp> {
        let url = self.url(path_or_url);
        if let Some(hit) = self.cache.as_ref().and_then(|c| c.get(&url)) {
            self.ui
                .debug(&format!("GET {url} → {} (cached)", hit.status));
            return Ok(hit);
        }
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            let mut req = self
                .agent
                .get(&url)
                .set("Accept", "application/vnd.github+json")
                .set("X-GitHub-Api-Version", "2022-11-28");
            if let Some(t) = &self.token {
                req = req.set("Authorization", &format!("Bearer {t}"));
            }
            let resp = match req.call() {
                Ok(r) => r,
                Err(ureq::Error::Status(_, r)) => r,
                Err(ureq::Error::Transport(t)) => {
                    if attempt < 3 {
                        self.ui
                            .debug(&format!("network error on {url}: {t}; retrying"));
                        std::thread::sleep(self.backoff * attempt);
                        continue;
                    }
                    return Err(AppError::runtime(format!("cannot reach the GitHub API: {t}"))
                        .hint("check your network connection, or --api-url if you use GitHub Enterprise"));
                }
            };
            let status = resp.status();
            let remaining = resp.header("x-ratelimit-remaining").map(str::to_string);
            let reset = resp
                .header("x-ratelimit-reset")
                .and_then(|v| v.parse::<u64>().ok());
            let retry_after = resp
                .header("retry-after")
                .and_then(|v| v.parse::<u64>().ok());
            let next = resp.header("link").and_then(parse_next_link);
            self.ui.debug(&format!(
                "GET {url} → {status}{}",
                remaining
                    .as_deref()
                    .map(|r| format!(" (rate limit left: {r})"))
                    .unwrap_or_default()
            ));

            let body: Value = if status == 204 {
                Value::Null
            } else {
                let mut text = String::new();
                let _ = resp
                    .into_reader()
                    .take(256 * 1024 * 1024)
                    .read_to_string(&mut text);
                serde_json::from_str(&text).unwrap_or(Value::Null)
            };

            let rate_limited = (status == 403 || status == 429)
                && (remaining.as_deref() == Some("0")
                    || retry_after.is_some()
                    || body["message"]
                        .as_str()
                        .is_some_and(|m| m.to_lowercase().contains("rate limit")));
            if rate_limited {
                let wait = retry_after.unwrap_or_else(|| {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    reset.map(|r| r.saturating_sub(now) + 1).unwrap_or(60)
                });
                if wait > MAX_RATE_WAIT || attempt > 5 {
                    let hint = if self.token.is_some() {
                        format!(
                            "the limit resets in about {} min; run again then",
                            wait.div_ceil(60)
                        )
                    } else {
                        "set GITHUB_TOKEN (or run `gh auth login`): a token raises the limit from 60 to 5,000 requests/hour".into()
                    };
                    return Err(AppError::runtime("GitHub API rate limit exhausted").hint(hint));
                }
                self.ui
                    .warn(&format!("GitHub rate limit reached; waiting {wait}s"));
                std::thread::sleep(
                    Duration::from_secs(wait).min(self.backoff * (wait as u32).max(1)),
                );
                continue;
            }
            if status >= 500 && attempt < 4 {
                self.ui
                    .debug(&format!("server error {status} on {url}; retrying"));
                std::thread::sleep(self.backoff * attempt);
                continue;
            }
            if status == 401 {
                return Err(AppError::runtime("GitHub rejected the token (401 Bad credentials)")
                    .hint("check --token / GITHUB_TOKEN / GH_TOKEN, or unset them to run unauthenticated"));
            }
            let resp = Resp { status, body, next };
            if status == 200 {
                if let Some(c) = &self.cache {
                    c.put(&url, &resp);
                }
            }
            return Ok(resp);
        }
    }

    /// GET every page of a list endpoint. `Ok(None)` on 404.
    pub fn get_all(&self, path: &str) -> Result<Option<Vec<Value>>> {
        let mut out = Vec::new();
        let mut next = Some(path.to_string());
        while let Some(url) = next {
            let r = self.get(&url)?;
            if r.status == 404 {
                return Ok(None);
            }
            if r.status != 200 {
                return Err(unexpected(&url, &r));
            }
            match r.body {
                Value::Array(items) => out.extend(items),
                _ => return Err(unexpected(&url, &r)),
            }
            next = r.next;
        }
        Ok(Some(out))
    }
}

pub fn unexpected(url: &str, r: &Resp) -> AppError {
    let msg = r.body["message"].as_str().unwrap_or("unexpected response");
    AppError::runtime(format!("GitHub API returned {} for {url}: {msg}", r.status))
}

/// Pull the `rel="next"` URL out of a `Link` header.
pub fn parse_next_link(header: &str) -> Option<String> {
    header.split(',').find_map(|part| {
        let mut pieces = part.split(';');
        let url = pieces.next()?.trim().strip_prefix('<')?.strip_suffix('>')?;
        pieces
            .any(|p| p.trim() == "rel=\"next\"")
            .then(|| url.to_string())
    })
}

/// Token precedence: --token > GITHUB_TOKEN > GH_TOKEN > `gh auth token` (default API only).
pub fn resolve_token(flag: Option<&str>, default_api: bool) -> Option<(String, &'static str)> {
    let nonempty = |s: String| (!s.trim().is_empty()).then(|| s.trim().to_string());
    if let Some(t) = flag.map(str::to_string).and_then(nonempty) {
        return Some((t, "--token"));
    }
    for var in ["GITHUB_TOKEN", "GH_TOKEN"] {
        if let Some(t) = std::env::var(var).ok().and_then(nonempty) {
            return Some((
                t,
                if var == "GITHUB_TOKEN" {
                    "GITHUB_TOKEN"
                } else {
                    "GH_TOKEN"
                },
            ));
        }
    }
    if default_api && std::env::var_os("DEVSTATS_NO_GH").is_none() {
        let out = std::process::Command::new("gh")
            .args(["auth", "token"])
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        if out.status.success() {
            return String::from_utf8(out.stdout)
                .ok()
                .and_then(nonempty)
                .map(|t| (t, "gh auth token"));
        }
    }
    None
}

/// Percent-encode a search query for a URL.
pub fn encode_query(q: &str) -> String {
    let mut out = String::new();
    for b in q.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b':' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_link() {
        let h = r#"<https://api.github.com/x?page=2>; rel="next", <https://api.github.com/x?page=5>; rel="last""#;
        assert_eq!(
            parse_next_link(h).as_deref(),
            Some("https://api.github.com/x?page=2")
        );
        assert_eq!(parse_next_link(r#"<https://a/x?page=1>; rel="prev""#), None);
    }

    #[test]
    fn query_encoding() {
        assert_eq!(
            encode_query("org:a-b is:pr created:2020-01-01..2020-02-01"),
            "org:a-b+is:pr+created:2020-01-01..2020-02-01"
        );
    }
}
