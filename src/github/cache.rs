//! On-disk cache for successful GET responses, so repeat runs skip the network
//! (and the 30 requests/minute Search limit).
//!
//! Location: $DEVSTATS_CACHE_DIR, else $XDG_CACHE_HOME/devstats, else ~/.cache/devstats.
//! Entries expire after one hour ($DEVSTATS_CACHE_TTL seconds overrides it).
//! Only public data is fetched, so entries are keyed by URL alone.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use super::Resp;

pub struct Cache {
    dir: PathBuf,
    ttl: u64,
}

impl Cache {
    pub fn from_env() -> Option<Cache> {
        let var = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let dir = var("DEVSTATS_CACHE_DIR")
            .or_else(|| var("XDG_CACHE_HOME").map(|d| d.join("devstats")))
            .or_else(|| var("HOME").map(|h| h.join(".cache").join("devstats")))?;
        let ttl = std::env::var("DEVSTATS_CACHE_TTL")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3600);
        Some(Cache { dir, ttl })
    }

    pub fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    fn path(&self, url: &str) -> PathBuf {
        self.dir.join(format!("{:016x}.json", fnv1a(url)))
    }

    /// A fresh cached response for `url`, if there is one.
    pub fn get(&self, url: &str) -> Option<Resp> {
        let text = std::fs::read_to_string(self.path(url)).ok()?;
        let v: Value = serde_json::from_str(&text).ok()?;
        if v["url"].as_str() != Some(url) || now().saturating_sub(v["t"].as_u64()?) > self.ttl {
            return None;
        }
        Some(Resp {
            status: v["status"].as_u64()? as u16,
            body: v["body"].clone(),
            next: v["next"].as_str().map(str::to_string),
        })
    }

    /// Store a response; failures are ignored (the cache is only an optimization).
    pub fn put(&self, url: &str, r: &Resp) {
        if std::fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let entry =
            json!({"url": url, "t": now(), "status": r.status, "next": r.next, "body": r.body});
        let path = self.path(url);
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));
        if std::fs::write(&tmp, entry.to_string()).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn fnv1a(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100000001b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_expiry() {
        let dir = tempfile::tempdir().unwrap();
        let c = Cache {
            dir: dir.path().to_path_buf(),
            ttl: 3600,
        };
        let r = Resp {
            status: 200,
            body: json!([1, 2]),
            next: Some("https://x/2".into()),
        };
        assert!(c.get("https://x/1").is_none());
        c.put("https://x/1", &r);
        let got = c.get("https://x/1").unwrap();
        assert_eq!(
            (got.status, got.body, got.next),
            (200, json!([1, 2]), Some("https://x/2".into()))
        );
        let expired = Cache {
            dir: dir.path().to_path_buf(),
            ttl: 0,
        };
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert!(expired.get("https://x/1").is_none());
    }
}
