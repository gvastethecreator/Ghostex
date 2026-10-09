//! The background refresh of every cache work mode reads. Blocking (Linear and `gh` are network
//! calls), so it only ever runs on a blocking worker, never on a request path.

use std::collections::HashMap;

use serde_json::Value;

use crate::paths::GxserverPaths;
use crate::session_git_status::gh_cli_is_available;

use super::*;

/// Refreshes what the given sessions of work-mode projects link to, within the per-pass budgets.
pub(crate) fn refresh_work_caches(paths: &GxserverPaths, projects: &[Value], sessions: &[Value]) {
    let projects_by_id: HashMap<&str, &Value> = projects
        .iter()
        .filter(|project| project_work_mode(project))
        .filter_map(|project| {
            project
                .get("projectId")
                .and_then(Value::as_str)
                .map(|id| (id, project))
        })
        .collect();
    if projects_by_id.is_empty() {
        return;
    }

    // Which key each project uses, remembered for the projection.
    let mut keys: HashMap<u64, String> = HashMap::new();
    for project_id in projects_by_id.keys() {
        let key = linear_api_key(paths, Some(project_id));
        let fingerprint = key.as_deref().map(linear_key_fingerprint);
        remember_project_linear_key(project_id, fingerprint);
        if let (Some(fingerprint), Some(key)) = (fingerprint, key) {
            keys.insert(fingerprint, key);
        }
    }

    let mut linear_budget = MAX_LINEAR_REQUESTS_PER_PASS;
    for key in keys.values() {
        if linear_budget == 0 {
            break;
        }
        if refresh_linear_team_keys(key) {
            linear_budget -= 1;
        }
    }

    let session_targets = |sessions: &[Value]| -> Vec<WorkTargets> {
        sessions
            .iter()
            .filter_map(|session| {
                let project_id = session.get("projectId").and_then(Value::as_str)?;
                let project = projects_by_id.get(project_id)?;
                Some(work_targets(project, session))
            })
            .collect()
    };

    let mut wanted_issues: HashMap<u64, Vec<String>> = HashMap::new();
    for targets in session_targets(sessions) {
        if let Some(key) = targets.linear_key {
            wanted_issues
                .entry(key)
                .or_default()
                .extend(targets.linear_issues.iter().cloned());
        }
    }
    for (fingerprint, identifiers) in &wanted_issues {
        if linear_budget == 0 {
            break;
        }
        if let Some(key) = keys.get(fingerprint) {
            linear_budget = linear_budget.saturating_sub(refresh_linear_issues(
                key,
                identifiers,
                linear_budget,
            ));
        }
    }

    if !gh_cli_is_available() {
        return;
    }
    // Resolved again: an issue Linear just answered for can bring its PR along.
    let mut gh_budget = MAX_GH_CALLS_PER_PASS;
    for targets in session_targets(sessions) {
        if gh_budget == 0 {
            break;
        }
        let Some(cwd) = targets.cwd.as_deref() else {
            continue;
        };
        if let Some(selector) = targets.pull_request.as_deref() {
            if refresh_work_pull_request(cwd, selector) {
                gh_budget -= 1;
            }
        }
        for number in &targets.github_issues {
            if gh_budget == 0 {
                break;
            }
            if refresh_work_github_issue(cwd, *number) {
                gh_budget -= 1;
            }
        }
    }
}
