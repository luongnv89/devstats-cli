//! The report structure shared by every output format (it is also the JSON schema).

use chrono::NaiveDate;
use serde::Serialize;

use crate::cli::SortKey;
use crate::github::repos::Owner;
use crate::github::stats::RepoStats;

/// A pull request or issue opened in an organization (from the Search API).
#[derive(Debug, Clone)]
pub struct Item {
    pub login: String,
    pub is_bot: bool,
    pub is_pr: bool,
    pub merged: bool,
    pub created: NaiveDate,
}

/// Whether an organization's private repositories are in the scan, and if not, why.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrivateAccess {
    /// The listing returned at least one private repository; they are all scanned.
    Included,
    /// A token was used but the listing showed no private repositories. A listing
    /// cannot tell "no access" from "no private repositories".
    NoneVisible,
    /// No token was used, so GitHub showed public repositories only.
    NoToken,
    /// The full listing failed (e.g. SAML SSO not authorized); public repositories only.
    ListingFailed,
    /// GitHub lists only the public repositories of a user account.
    UserAccount,
}

/// Everything fetched for one organization, before aggregation.
#[derive(Debug, Clone)]
pub struct OrgData {
    pub owner: Owner,
    pub repos_scanned: Vec<String>,
    pub repos_skipped: usize,
    /// Private repositories among `repos_scanned`.
    pub private_repos: usize,
    pub private_access: PrivateAccess,
    /// HTTP status of the full repository listing when it failed (public repos only).
    pub listing_status: Option<u16>,
    pub stats: Vec<RepoStats>,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Totals {
    pub contributors: usize,
    pub repos: usize,
    /// Private repositories among `repos`.
    pub repos_private: usize,
    pub repos_skipped: usize,
    pub commits: u64,
    pub additions: u64,
    pub deletions: u64,
    pub prs: u64,
    pub prs_merged: u64,
    pub issues: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Contributor {
    pub login: String,
    pub html_url: String,
    pub avatar_url: String,
    pub commits: u64,
    pub additions: u64,
    pub deletions: u64,
    pub prs: u64,
    pub prs_merged: u64,
    pub issues: u64,
    /// Repositories with at least one commit by this contributor in the range.
    pub repos: usize,
    pub orgs: Vec<String>,
    /// Share of all commits, in percent.
    pub share: f64,
    /// Commits per week for the 52 weeks ending at the range end (oldest first).
    pub activity: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WeekPoint {
    pub week: String,
    pub commits: u64,
    pub additions: u64,
    pub deletions: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrgReport {
    pub login: String,
    pub name: Option<String>,
    pub kind: &'static str,
    pub avatar_url: String,
    pub html_url: String,
    pub totals: Totals,
    /// Private repositories scanned in this organization.
    pub private_repos: usize,
    pub private_access: PrivateAccess,
    pub incomplete_repos: Vec<String>,
    pub contributors: Vec<Contributor>,
    pub weekly: Vec<WeekPoint>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Range {
    pub since: Option<String>,
    pub until: Option<String>,
    /// Start date of the first of the 52 `activity` weeks.
    pub activity_from: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub tool: &'static str,
    pub version: &'static str,
    pub generated_at: String,
    pub range: Range,
    pub sort: SortKey,
    /// 0 means every contributor is listed.
    pub top: usize,
    pub totals: Totals,
    pub orgs: Vec<OrgReport>,
    pub contributors: Vec<Contributor>,
    pub weekly: Vec<WeekPoint>,
    pub notes: Vec<String>,
}

impl Report {
    pub fn org_names(&self) -> Vec<&str> {
        self.orgs.iter().map(|o| o.login.as_str()).collect()
    }
}
