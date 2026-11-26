//! Resolve an owner (organization or user) and list the repositories the token can read.

use serde_json::Value;

use super::{unexpected, Client, Pages};
use crate::error::Result;
use crate::model::{PrivateAccess, RepoTarget};

#[derive(Debug, Clone)]
pub struct Owner {
    pub login: String,
    pub name: Option<String>,
    pub is_org: bool,
    pub avatar_url: String,
    pub html_url: String,
}

#[derive(Debug, Clone)]
pub struct Repo {
    pub name: String,
    pub fork: bool,
    pub archived: bool,
    pub private: bool,
}

/// An owner's repositories and whether its private ones could be listed.
#[derive(Debug, Clone)]
pub struct Listing {
    pub repos: Vec<Repo>,
    pub access: PrivateAccess,
    /// HTTP status of the full listing when it failed and only public repos were listed.
    pub failed_status: Option<u16>,
}

/// Look up an owner by login. `Ok(None)` when GitHub has no such user or org.
pub fn owner(client: &Client, login: &str) -> Result<Option<Owner>> {
    let path = format!("/users/{login}");
    let r = client.get(&path)?;
    if r.status == 404 {
        return Ok(None);
    }
    if r.status != 200 {
        return Err(unexpected(&client.url(&path), &r));
    }
    let b = &r.body;
    Ok(Some(Owner {
        login: b["login"].as_str().unwrap_or(login).to_string(),
        name: b["name"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        is_org: b["type"].as_str() == Some("Organization"),
        avatar_url: b["avatar_url"].as_str().unwrap_or_default().to_string(),
        html_url: b["html_url"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| format!("https://github.com/{login}")),
    }))
}

/// Look up one repository. `Ok(None)` when it does not exist or the token cannot
/// read it (GitHub answers 404 for both). Names come back in GitHub's casing.
pub fn repo(client: &Client, owner: &str, name: &str) -> Result<Option<(Owner, RepoTarget)>> {
    let path = format!("/repos/{owner}/{name}");
    let r = client.get(&path)?;
    if r.status == 404 {
        return Ok(None);
    }
    if r.status != 200 {
        return Err(unexpected(&client.url(&path), &r));
    }
    let b = &r.body;
    let o = &b["owner"];
    let login = o["login"].as_str().unwrap_or(owner).to_string();
    let repo_name = b["name"].as_str().unwrap_or(name).to_string();
    Ok(Some((
        Owner {
            name: None,
            is_org: o["type"].as_str() == Some("Organization"),
            avatar_url: o["avatar_url"].as_str().unwrap_or_default().to_string(),
            html_url: o["html_url"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| format!("https://github.com/{login}")),
            login: login.clone(),
        },
        RepoTarget {
            html_url: b["html_url"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| format!("https://github.com/{login}/{repo_name}")),
            private: b["private"].as_bool().unwrap_or(false)
                || b["visibility"].as_str() == Some("private"),
            name: repo_name,
        },
    )))
}

/// Every repository of the owner the token can read (forks and archived included;
/// callers filter). Organizations are listed with `type=all`, which includes private
/// repositories the token can see; if that listing answers with an unexpected 4xx status
/// (e.g. 403 for an organization that enforces SAML SSO), devstats falls back to the
/// public listing. A 5xx that outlasts the client's retries is an error, like on any
/// other listing. GitHub lists only the public repositories of a user account.
pub fn list(client: &Client, owner: &Owner) -> Result<Listing> {
    let mut failed_status = None;
    let items = if owner.is_org {
        let all = format!("/orgs/{}/repos?type=all&per_page=100", owner.login);
        match client.get_all(&all)? {
            Pages::Stopped { resp, .. }
                if resp.status != 200 && resp.status != 404 && resp.status < 500 =>
            {
                failed_status = Some(resp.status);
                let public = format!("/orgs/{}/repos?type=public&per_page=100", owner.login);
                items(client.get_all(&public)?)?
            }
            pages => items(pages)?,
        }
    } else {
        let path = format!("/users/{}/repos?type=owner&per_page=100", owner.login);
        items(client.get_all(&path)?)?
    };
    let repos: Vec<Repo> = items.iter().filter_map(repo_from).collect();
    let access = access(
        owner.is_org,
        client.has_token(),
        failed_status.is_some(),
        repos.iter().any(|r| r.private),
    );
    Ok(Listing {
        repos,
        access,
        failed_status,
    })
}

/// The items of a listing; a 404 is an empty list, any other failure an error.
fn items(pages: Pages) -> Result<Vec<Value>> {
    match pages {
        Pages::All(items) => Ok(items),
        Pages::Stopped { resp, .. } if resp.status == 404 => Ok(vec![]),
        Pages::Stopped { url, resp } => Err(unexpected(&url, &resp)),
    }
}

/// Why private repositories are (or are not) in the scan. User accounts come first
/// (no token changes that listing), then a failed full listing, then the listing itself.
fn access(is_org: bool, has_token: bool, failed: bool, any_private: bool) -> PrivateAccess {
    if !is_org {
        PrivateAccess::UserAccount
    } else if failed {
        PrivateAccess::ListingFailed
    } else if any_private {
        PrivateAccess::Included
    } else if !has_token {
        PrivateAccess::NoToken
    } else {
        PrivateAccess::NoneVisible
    }
}

fn repo_from(v: &Value) -> Option<Repo> {
    Some(Repo {
        name: v["name"].as_str()?.to_string(),
        fork: v["fork"].as_bool().unwrap_or(false),
        archived: v["archived"].as_bool().unwrap_or(false),
        private: v["private"].as_bool().unwrap_or(false)
            || v["visibility"].as_str() == Some("private"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn access_status_precedence() {
        use PrivateAccess::*;
        assert_eq!(access(false, true, false, false), UserAccount);
        assert_eq!(access(true, true, true, false), ListingFailed);
        assert_eq!(access(true, false, true, false), ListingFailed);
        assert_eq!(access(true, true, false, true), Included);
        assert_eq!(access(true, false, false, false), NoToken);
        assert_eq!(access(true, true, false, false), NoneVisible);
    }

    #[test]
    fn private_flag_from_either_field() {
        let r = |v| repo_from(&v).unwrap().private;
        assert!(r(json!({"name": "a", "private": true})));
        assert!(r(json!({"name": "a", "visibility": "private"})));
        assert!(!r(
            json!({"name": "a", "private": false, "visibility": "public"})
        ));
        assert!(!r(json!({"name": "a"})));
    }
}
