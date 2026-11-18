//! Per-repository contributor statistics (`/repos/{o}/{r}/stats/contributors`).
//!
//! GitHub computes these lazily and answers `202 Accepted` until they are ready.
//! Every repo is requested once in parallel (which starts the computation for all of
//! them), then the ones still at 202 are retried in rounds with growing pauses.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use serde_json::Value;

use super::Client;
use crate::error::Result;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct Week {
    /// Unix timestamp (seconds) of the week's start, Sunday 00:00 UTC.
    pub w: i64,
    pub a: u64,
    pub d: u64,
    pub c: u64,
}

#[derive(Debug, Clone)]
pub struct AuthorStats {
    pub login: String,
    pub avatar_url: String,
    pub is_bot: bool,
    pub weeks: Vec<Week>,
}

#[derive(Debug, Clone)]
pub struct RepoStats {
    pub repo: String,
    pub authors: Vec<AuthorStats>,
    /// False when GitHub never finished computing the stats.
    pub complete: bool,
}

enum Fetch {
    Ready(Vec<AuthorStats>),
    Computing,
}

/// Pauses (in backoff units) before each retry round for repos still at 202.
const ROUNDS: [u32; 9] = [0, 2, 3, 5, 8, 10, 10, 10, 10];

pub fn fetch_all(
    client: &Client,
    owner: &str,
    repos: &[String],
    jobs: usize,
    on_done: &(dyn Fn(usize) + Sync),
) -> Result<Vec<RepoStats>> {
    let mut done: Vec<Option<RepoStats>> = vec![None; repos.len()];
    let mut pending: Vec<usize> = (0..repos.len()).collect();
    let finished = AtomicUsize::new(0);
    for (round, pause) in ROUNDS.iter().enumerate() {
        if pending.is_empty() {
            break;
        }
        if round > 0 {
            client.ui.debug(&format!(
                "{} repo(s) still computing stats; retrying",
                pending.len()
            ));
            std::thread::sleep(client.backoff * *pause);
        }
        let results = par_map(&pending, jobs, |&i| {
            let r = fetch_one(client, owner, &repos[i]);
            if matches!(r, Ok(Fetch::Ready(_))) {
                on_done(finished.fetch_add(1, Ordering::SeqCst) + 1);
            }
            r
        })?;
        let mut still = Vec::new();
        for (&i, r) in pending.iter().zip(results) {
            match r {
                Fetch::Ready(authors) => {
                    done[i] = Some(RepoStats {
                        repo: repos[i].clone(),
                        authors,
                        complete: true,
                    })
                }
                Fetch::Computing => still.push(i),
            }
        }
        pending = still;
    }
    for i in pending {
        client.ui.warn(&format!(
            "GitHub is still computing stats for {owner}/{}; its commits are missing. Run again in a minute",
            repos[i]
        ));
        done[i] = Some(RepoStats {
            repo: repos[i].clone(),
            authors: vec![],
            complete: false,
        });
    }
    Ok(done.into_iter().flatten().collect())
}

fn fetch_one(client: &Client, owner: &str, repo: &str) -> Result<Fetch> {
    let path = format!("/repos/{owner}/{repo}/stats/contributors");
    let r = client.get(&path)?;
    match r.status {
        200 => Ok(Fetch::Ready(parse(&r.body))),
        202 => Ok(Fetch::Computing),
        204 | 404 | 409 | 451 => Ok(Fetch::Ready(vec![])), // empty, gone, or blocked repo
        s => {
            client.ui.warn(&format!(
                "skipping {owner}/{repo}: GitHub returned {s} for its stats"
            ));
            Ok(Fetch::Ready(vec![]))
        }
    }
}

pub fn parse(body: &Value) -> Vec<AuthorStats> {
    let Some(items) = body.as_array() else {
        return vec![];
    };
    items
        .iter()
        .filter_map(|item| {
            let author = item.get("author").filter(|a| !a.is_null())?;
            let login = author["login"].as_str()?.to_string();
            let is_bot = author["type"].as_str() == Some("Bot") || login.ends_with("[bot]");
            let weeks = item["weeks"]
                .as_array()
                .map(|ws| {
                    ws.iter()
                        .filter_map(|w| {
                            Some(Week {
                                w: w["w"].as_i64()?,
                                a: w["a"].as_u64().unwrap_or(0),
                                d: w["d"].as_u64().unwrap_or(0),
                                c: w["c"].as_u64().unwrap_or(0),
                            })
                        })
                        .filter(|w| w.a + w.d + w.c > 0)
                        .collect()
                })
                .unwrap_or_default();
            Some(AuthorStats {
                login,
                avatar_url: author["avatar_url"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                is_bot,
                weeks,
            })
        })
        .collect()
}

/// Map `f` over `items` with up to `jobs` threads, keeping order. Stops early on the first error.
pub fn par_map<T: Sync, R: Send>(
    items: &[T],
    jobs: usize,
    f: impl Fn(&T) -> Result<R> + Sync,
) -> Result<Vec<R>> {
    let n = items.len();
    let next = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    let slots: Vec<Mutex<Option<Result<R>>>> = (0..n).map(|_| Mutex::new(None)).collect();
    std::thread::scope(|s| {
        for _ in 0..jobs.clamp(1, n.max(1)) {
            s.spawn(|| loop {
                if failed.load(Ordering::SeqCst) {
                    break;
                }
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= n {
                    break;
                }
                let r = f(&items[i]);
                if r.is_err() {
                    failed.store(true, Ordering::SeqCst);
                }
                *slots[i].lock().unwrap() = Some(r);
            });
        }
    });
    let results: Vec<Option<Result<R>>> =
        slots.into_iter().map(|m| m.into_inner().unwrap()).collect();
    if let Some(pos) = results.iter().position(|r| matches!(r, Some(Err(_)))) {
        let Some(Some(Err(e))) = results.into_iter().nth(pos) else {
            unreachable!()
        };
        return Err(e);
    }
    Ok(results
        .into_iter()
        .map(|r| r.and_then(|r| r.ok()).expect("every slot filled"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_skips_null_authors_and_flags_bots() {
        let body = json!([
            {"author": null, "total": 3, "weeks": [{"w": 1, "a": 1, "d": 1, "c": 3}]},
            {"author": {"login": "dependabot[bot]", "type": "Bot", "avatar_url": ""}, "weeks": [{"w": 1, "a": 2, "d": 0, "c": 1}]},
            {"author": {"login": "alice", "type": "User", "avatar_url": "u"}, "weeks": [{"w": 1, "a": 5, "d": 2, "c": 2}, {"w": 2, "a": 0, "d": 0, "c": 0}]}
        ]);
        let got = parse(&body);
        assert_eq!(got.len(), 2);
        assert!(got[0].is_bot);
        assert_eq!(got[1].login, "alice");
        assert_eq!(got[1].weeks.len(), 1, "empty weeks are dropped");
    }

    #[test]
    fn par_map_keeps_order_and_propagates_errors() {
        let items: Vec<u32> = (0..50).collect();
        let out = par_map(&items, 8, |x| Ok(x * 2)).unwrap();
        assert_eq!(out, items.iter().map(|x| x * 2).collect::<Vec<_>>());
        let err = par_map(&items, 4, |&x| {
            if x == 7 {
                Err(crate::error::AppError::runtime("boom"))
            } else {
                Ok(x)
            }
        });
        assert_eq!(err.unwrap_err().msg, "boom");
        assert!(par_map(&Vec::<u32>::new(), 4, |x| Ok(*x))
            .unwrap()
            .is_empty());
    }
}
