//! GitHub through `gh`: a linked PR's state and checks, and a linked issue's state, behind TTL
//! caches the presentation projection only reads.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::session_git_status::{parse_gh_pull_request_state, run_gh_command, PullRequestState};

/// Checks finish within minutes, so a linked PR is re-read every two.
const PULL_REQUEST_TTL: Duration = Duration::from_secs(120);
const ISSUE_TTL: Duration = Duration::from_secs(5 * 60);
/// `gh` calls one background pass may make.
pub(crate) const MAX_GH_CALLS_PER_PASS: usize = 12;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WorkPullRequestInfo {
    pub(crate) number: u64,
    pub(crate) state: PullRequestState,
    pub(crate) url: Option<String>,
    /// `passing`, `failing` or `pending`; `None` for a PR without checks.
    pub(crate) checks: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WorkGithubIssueInfo {
    pub(crate) number: u64,
    pub(crate) title: Option<String>,
    /// `open` or `closed`.
    pub(crate) state: Option<String>,
    pub(crate) url: Option<String>,
}

struct Cached<T> {
    value: T,
    fetched_at: Instant,
}

#[derive(Default)]
struct GithubCache {
    pull_requests: HashMap<String, Cached<Option<WorkPullRequestInfo>>>,
    issues: HashMap<String, Cached<Option<WorkGithubIssueInfo>>>,
}

fn cache() -> &'static Mutex<GithubCache> {
    static CACHE: OnceLock<Mutex<GithubCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(GithubCache::default()))
}

/// A PR is asked about by number from the session's checkout, or by URL from anywhere.
fn cache_key(cwd: &str, selector: &str) -> String {
    if selector.starts_with("http") {
        selector.to_string()
    } else {
        format!("{cwd}\u{1f}{selector}")
    }
}

pub(crate) fn cached_work_pull_request(cwd: &str, selector: &str) -> Option<WorkPullRequestInfo> {
    let cache = cache().lock().ok()?;
    cache
        .pull_requests
        .get(&cache_key(cwd, selector))
        .and_then(|cached| cached.value.clone())
}

pub(crate) fn cached_work_github_issue(cwd: &str, number: u64) -> Option<WorkGithubIssueInfo> {
    let cache = cache().lock().ok()?;
    cache
        .issues
        .get(&cache_key(cwd, &number.to_string()))
        .and_then(|cached| cached.value.clone())
}

/// Re-reads a PR when stale. Returns whether `gh` ran.
pub(crate) fn refresh_work_pull_request(cwd: &str, selector: &str) -> bool {
    let key = cache_key(cwd, selector);
    if cache().lock().ok().is_some_and(|cache| {
        cache
            .pull_requests
            .get(&key)
            .is_some_and(|cached| cached.fetched_at.elapsed() < PULL_REQUEST_TTL)
    }) {
        return false;
    }
    let info = run_gh_command(
        Some(cwd),
        &[
            "pr",
            "view",
            selector,
            "--json",
            "number,state,url,isDraft,statusCheckRollup",
        ],
    )
    .and_then(|output| parse_pull_request(&output));
    if let Ok(mut cache) = cache().lock() {
        cache.pull_requests.insert(
            key,
            Cached {
                value: info,
                fetched_at: Instant::now(),
            },
        );
    }
    true
}

/// Re-reads an issue when stale. Returns whether `gh` ran.
pub(crate) fn refresh_work_github_issue(cwd: &str, number: u64) -> bool {
    let key = cache_key(cwd, &number.to_string());
    if cache().lock().ok().is_some_and(|cache| {
        cache
            .issues
            .get(&key)
            .is_some_and(|cached| cached.fetched_at.elapsed() < ISSUE_TTL)
    }) {
        return false;
    }
    let number_text = number.to_string();
    let info = run_gh_command(
        Some(cwd),
        &[
            "issue",
            "view",
            number_text.as_str(),
            "--json",
            "number,title,state,url",
        ],
    )
    .and_then(|output| serde_json::from_str::<Value>(output.trim()).ok())
    .map(|value| WorkGithubIssueInfo {
        number,
        title: value
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_string),
        state: value
            .get("state")
            .and_then(Value::as_str)
            .map(|state| state.to_ascii_lowercase()),
        url: value.get("url").and_then(Value::as_str).map(str::to_string),
    });
    if let Ok(mut cache) = cache().lock() {
        cache.issues.insert(
            key,
            Cached {
                value: info,
                fetched_at: Instant::now(),
            },
        );
    }
    true
}

fn parse_pull_request(output: &str) -> Option<WorkPullRequestInfo> {
    let value: Value = serde_json::from_str(output.trim()).ok()?;
    let number = value.get("number").and_then(Value::as_u64)?;
    let is_draft = value
        .get("isDraft")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let state = parse_gh_pull_request_state(value.get("state").and_then(Value::as_str)?, is_draft)?;
    Some(WorkPullRequestInfo {
        number,
        state,
        url: value.get("url").and_then(Value::as_str).map(str::to_string),
        checks: value.get("statusCheckRollup").and_then(checks_state),
    })
}

/// One word for all of a PR's checks: any failure fails it, anything unfinished keeps it pending.
pub(super) fn checks_state(rollup: &Value) -> Option<&'static str> {
    let items = rollup.as_array()?;
    if items.is_empty() {
        return None;
    }
    let upper = |item: &Value, key: &str| {
        item.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_uppercase()
    };
    let mut pending = false;
    for item in items {
        let conclusion = upper(item, "conclusion");
        let status = upper(item, "status");
        let state = upper(item, "state");
        if matches!(
            conclusion.as_str(),
            "FAILURE" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED" | "STARTUP_FAILURE"
        ) || matches!(state.as_str(), "FAILURE" | "ERROR")
        {
            return Some("failing");
        }
        if (!status.is_empty() && status != "COMPLETED")
            || matches!(state.as_str(), "PENDING" | "EXPECTED")
        {
            pending = true;
        }
    }
    Some(if pending { "pending" } else { "passing" })
}
