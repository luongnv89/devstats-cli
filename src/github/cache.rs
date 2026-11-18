//! On-disk cache for successful GET responses, so repeat runs skip the network
//! (and the 30 requests/minute Search limit).
//!
//! Location: $DEVSTATS_CACHE_DIR, else $XDG_CACHE_HOME/devstats, else ~/.cache/devstats.
//! Entries expire after one hour ($DEVSTATS_CACHE_TTL seconds overrides it).
//! A token can read private repositories, so entries are keyed by a fingerprint of the
//! credential plus the URL: a run never sees what another token (or no token) fetched.
//! The fingerprint is a hash; the token itself is never written. On Unix the directory
//! is created 0700 and the files 0600.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use super::Resp;

pub struct Cache {
    dir: PathBuf,
    ttl: u64,
    /// `anon` without a token, else a hash of the token.
    fingerprint: String,
}

impl Cache {
    pub fn from_env(token: Option<&str>) -> Option<Cache> {
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
        Some(Cache {
            dir,
            ttl,
            fingerprint: fingerprint(token),
        })
    }

    pub fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    fn path(&self, url: &str) -> PathBuf {
        let key = fnv1a(&format!("{}\n{url}", self.fingerprint));
        self.dir.join(format!("{key:016x}.json"))
    }

    /// A fresh cached response for `url`, if there is one.
    pub fn get(&self, url: &str) -> Option<Resp> {
        let text = std::fs::read_to_string(self.path(url)).ok()?;
        let v: Value = serde_json::from_str(&text).ok()?;
        if v["url"].as_str() != Some(url)
            || v["fp"].as_str() != Some(&self.fingerprint)
            || now().saturating_sub(v["t"].as_u64()?) > self.ttl
        {
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
        if create_private_dir(&self.dir).is_err() {
            return;
        }
        let entry = json!({
            "url": url, "fp": self.fingerprint, "t": now(),
            "status": r.status, "next": r.next, "body": r.body
        });
        let path = self.path(url);
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));
        if write_private(&tmp, entry.to_string().as_bytes()).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

/// `anon`, or a hash of the token (never the token itself).
fn fingerprint(token: Option<&str>) -> String {
    match token {
        None => "anon".into(),
        Some(t) => format!("{:016x}", fnv1a(&format!("devstats-cache-token\n{t}"))),
    }
}

fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    let mut b = std::fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(dir)
}

fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(path)?.write_all(data)
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
            fingerprint: fingerprint(None),
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
            fingerprint: fingerprint(None),
        };
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert!(expired.get("https://x/1").is_none());
    }
    #[test]
    fn entries_are_separated_per_token() {
        let dir = tempfile::tempdir().unwrap();
        let cache = |token: Option<&str>| Cache {
            dir: dir.path().to_path_buf(),
            ttl: 3600,
            fingerprint: fingerprint(token),
        };
        let (anon, a, b) = (cache(None), cache(Some("tok-a")), cache(Some("tok-b")));
        let url = "https://x/orgs/acme/repos";
        let paths = [anon.path(url), a.path(url), b.path(url)];
        assert!(paths[0] != paths[1] && paths[1] != paths[2] && paths[0] != paths[2]);
        let r = Resp {
            status: 200,
            body: json!(["private"]),
            next: None,
        };
        a.put(url, &r);
        assert!(anon.get(url).is_none());
        assert!(b.get(url).is_none());
        assert_eq!(
            cache(Some("tok-a")).get(url).unwrap().body,
            json!(["private"])
        );
        // An entry copied under another key is still rejected by its fingerprint.
        std::fs::copy(a.path(url), b.path(url)).unwrap();
        assert!(b.get(url).is_none());
        let text = std::fs::read_to_string(a.path(url)).unwrap();
        assert!(!text.contains("tok-a"), "the token is never written");
    }

    #[cfg(unix)]
    #[test]
    fn cache_dir_and_files_are_private_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let c = Cache {
            dir: root.path().join("devstats"),
            ttl: 3600,
            fingerprint: fingerprint(Some("t")),
        };
        let r = Resp {
            status: 200,
            body: json!([]),
            next: None,
        };
        c.put("https://x/1", &r);
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(c.dir()), 0o700);
        assert_eq!(mode(&c.path("https://x/1")), 0o600);
    }
}
