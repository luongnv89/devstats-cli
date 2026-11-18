//! Pull requests and issues per author, through the Search API (`/search/issues`).
//!
//! One query covers a whole organization (`org:X is:pr created:A..B`), 100 results
//! per page. Search returns at most 1000 results per query, so a range with more is
//! split in half until every part fits.

use chrono::{Duration, NaiveDate};
use serde_json::Value;

use super::{encode_query, unexpected, Client};
use crate::error::Result;
use crate::model::Item;

/// GitHub launched in 2008; nothing is older.
pub fn epoch() -> NaiveDate {
    NaiveDate::from_ymd_opt(2008, 1, 1).unwrap()
}

const SEARCH_CAP: u64 = 1000;

/// Every PR (`prs = true`) or issue opened in the owner's repositories within `from..=to`.
pub fn fetch(
    client: &Client,
    owner: &str,
    is_org: bool,
    prs: bool,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<Item>> {
    let mut out = Vec::new();
    fetch_range(client, owner, is_org, prs, from, to, &mut out)?;
    Ok(out)
}

fn fetch_range(
    client: &Client,
    owner: &str,
    is_org: bool,
    prs: bool,
    from: NaiveDate,
    to: NaiveDate,
    out: &mut Vec<Item>,
) -> Result<()> {
    let q = format!(
        "{}:{owner} is:{} created:{from}..{to}",
        if is_org { "org" } else { "user" },
        if prs { "pr" } else { "issue" }
    );
    let path = format!(
        "/search/issues?q={}&per_page=100&sort=created&order=asc",
        encode_query(&q)
    );
    let first = client.get(&path)?;
    if first.status == 422 {
        // GitHub answers 422 when the owner has no searchable repositories.
        client
            .ui
            .debug(&format!("search for '{q}' returned 422; counting nothing"));
        return Ok(());
    }
    if first.status != 200 {
        return Err(unexpected(&client.url(&path), &first));
    }
    let total = first.body["total_count"].as_u64().unwrap_or(0);
    if total > SEARCH_CAP && from < to {
        let mid = from + Duration::days((to - from).num_days() / 2);
        client
            .ui
            .debug(&format!("'{q}' has {total} results; splitting at {mid}"));
        fetch_range(client, owner, is_org, prs, from, mid, out)?;
        return fetch_range(client, owner, is_org, prs, mid + Duration::days(1), to, out);
    }
    if total > SEARCH_CAP {
        client.ui.warn(&format!(
            "more than {SEARCH_CAP} {} were opened in {owner} on {from}; only the first {SEARCH_CAP} are counted",
            if prs { "pull requests" } else { "issues" }
        ));
    }
    if first.body["incomplete_results"].as_bool() == Some(true) {
        client.ui.warn(&format!(
            "GitHub search timed out for '{q}'; counts may be slightly low"
        ));
    }
    collect(&first.body, prs, out);
    let mut next = first.next;
    while let Some(url) = next {
        let r = client.get(&url)?;
        if r.status != 200 {
            return Err(unexpected(&url, &r));
        }
        collect(&r.body, prs, out);
        next = r.next;
    }
    Ok(())
}

fn collect(body: &Value, prs: bool, out: &mut Vec<Item>) {
    let Some(items) = body["items"].as_array() else {
        return;
    };
    for it in items {
        let Some(login) = it["user"]["login"].as_str() else {
            continue;
        };
        let created = it["created_at"]
            .as_str()
            .and_then(|s| s.get(..10))
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            .unwrap_or_else(epoch);
        out.push(Item {
            login: login.to_string(),
            is_bot: it["user"]["type"].as_str() == Some("Bot") || login.ends_with("[bot]"),
            is_pr: prs,
            merged: prs && it["pull_request"]["merged_at"].is_string(),
            created,
        });
    }
}
