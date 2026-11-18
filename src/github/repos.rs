//! Resolve an owner (organization or user) and list its public repositories.

use serde_json::Value;

use super::{unexpected, Client};
use crate::error::Result;

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
        name: b["name"].as_str().filter(|s| !s.is_empty()).map(str::to_string),
        is_org: b["type"].as_str() == Some("Organization"),
        avatar_url: b["avatar_url"].as_str().unwrap_or_default().to_string(),
        html_url: b["html_url"].as_str().map(str::to_string).unwrap_or_else(|| format!("https://github.com/{login}")),
    }))
}

/// All public repositories of the owner (forks and archived included; callers filter).
pub fn list(client: &Client, owner: &Owner) -> Result<Vec<Repo>> {
    let path = if owner.is_org {
        format!("/orgs/{}/repos?type=public&per_page=100", owner.login)
    } else {
        format!("/users/{}/repos?type=owner&per_page=100", owner.login)
    };
    let items = client.get_all(&path)?.unwrap_or_default();
    Ok(items.iter().filter(|r| !r["private"].as_bool().unwrap_or(false)).filter_map(repo_from).collect())
}

fn repo_from(v: &Value) -> Option<Repo> {
    Some(Repo {
        name: v["name"].as_str()?.to_string(),
        fork: v["fork"].as_bool().unwrap_or(false),
        archived: v["archived"].as_bool().unwrap_or(false),
    })
}
