//! The Work page's routes (crate::work_mode): its list, one ticket's details, and the team flow
//! the details draw as a tracker.

use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::protocol::rpc_success;
use crate::work_mode::{
    build_work_list, load_work_projects, read_work_item, refresh_work_feeds, resolve_team_flow,
    store_team_flow, team_flow_rule_catalog, validate_team_flow_steps, work_feed_plan,
    work_feeds_stale, TeamFlowScope, WorkItemRef,
};

use super::*;

/// How long the list waits for Linear and `gh`. Past it, the page gets what the caches hold and
/// `refreshing: true`, and asks again a moment later while the fetch finishes on its own.
const WORK_LIST_WAIT: Duration = Duration::from_secs(8);
/// How long one ticket's details may take before the page gets an error it can retry.
const WORK_ITEM_WAIT: Duration = Duration::from_secs(20);

pub(super) async fn route_work_items_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    let RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        token_extension_id,
    } = request;
    if !matches!(
        endpoint.path.as_str(),
        "/api/listWorkItems" | "/api/readWorkItem" | "/api/readTeamFlow" | "/api/updateTeamFlow"
    ) {
        return Err(RouteHttpRequest {
            state,
            endpoint,
            request_id,
            body_json,
            token_extension_id,
        });
    }
    let params = match read_domain_rpc_params(&body_json) {
        Ok(params) => params,
        Err(error) => return Ok(domain_error_response(endpoint.path, request_id, error)),
    };
    let result = match endpoint.path.as_str() {
        "/api/listWorkItems" => list_work_items(state.clone(), params).await,
        "/api/readWorkItem" => read_work_item_route(state.clone(), params).await,
        "/api/readTeamFlow" => run_blocking(state.clone(), params, read_team_flow).await,
        _ => run_blocking(state.clone(), params, update_team_flow).await,
    };
    Ok(match result {
        Ok(value) => routed_json(
            Some(endpoint.path),
            StatusCode::OK,
            rpc_success(request_id, value),
        ),
        Err(error) => domain_error_response(endpoint.path, request_id, error),
    })
}

fn project_ids(params: &Map<String, Value>) -> Option<Vec<String>> {
    params
        .get("projectIds")
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(str::to_string)
                .collect()
        })
}

fn task_error(error: impl std::fmt::Display) -> DomainStateError {
    DomainStateError::corrupt_state(format!("Work page request failed: {error}"))
}

async fn run_blocking(
    state: Arc<AppState>,
    params: Map<String, Value>,
    work: fn(&AppState, &Map<String, Value>) -> Result<Value, DomainStateError>,
) -> Result<Value, DomainStateError> {
    tokio::task::spawn_blocking(move || work(&state, &params))
        .await
        .map_err(task_error)?
}

/// `{ projectIds?, force? }` → the list (see `build_work_list`) plus `refreshing`.
async fn list_work_items(
    state: Arc<AppState>,
    params: Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let wanted = project_ids(&params);
    let force = params
        .get("force")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let load_state = state.clone();
    let (projects, plan) = tokio::task::spawn_blocking(move || {
        let projects = load_work_projects(&load_state, wanted.as_deref())?;
        let plan = work_feed_plan(&projects);
        Ok::<_, DomainStateError>((projects, plan))
    })
    .await
    .map_err(task_error)??;
    let github_cwds = plan.github_cwds();
    let mut refreshing = false;
    if force || work_feeds_stale(&plan.linear, &github_cwds) {
        let linear = plan.linear.clone();
        let refresh = tokio::task::spawn_blocking(move || {
            refresh_work_feeds(&linear, &github_cwds, force);
        });
        refreshing = tokio::time::timeout(WORK_LIST_WAIT, refresh).await.is_err();
    }
    let mut list = tokio::task::spawn_blocking(move || build_work_list(&projects, &plan))
        .await
        .map_err(task_error)?;
    list["refreshing"] = json!(refreshing);
    Ok(list)
}

/// `{ projectId?, projectIds?, linearIssue? | githubIssue? | pullRequest?, force? }`.
async fn read_work_item_route(
    state: Arc<AppState>,
    params: Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let item_ref = WorkItemRef::from_params(&params).ok_or_else(|| {
        DomainStateError::bad_request("Pass linearIssue, githubIssue or pullRequest.")
    })?;
    let task = tokio::task::spawn_blocking(move || {
        let project_id = params
            .get("projectId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string);
        let mut wanted = project_ids(&params);
        if let (Some(wanted), Some(project_id)) = (wanted.as_mut(), project_id.as_ref()) {
            if !wanted.contains(project_id) {
                wanted.push(project_id.clone());
            }
        }
        let force = params
            .get("force")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let projects = load_work_projects(&state, wanted.as_deref())?;
        let plan = work_feed_plan(&projects);
        Ok::<_, DomainStateError>(read_work_item(
            &state.paths,
            &projects,
            &plan,
            project_id.as_deref(),
            &item_ref,
            force,
        ))
    });
    match tokio::time::timeout(WORK_ITEM_WAIT, task).await {
        Ok(result) => result.map_err(task_error)?,
        Err(_) => Err(DomainStateError::bad_request(
            "Linear or GitHub took too long to answer. Try again.",
        )),
    }
}

/// `{ projectId? | workspaceId? | scope: "default" }` → the steps that scope uses, where they come
/// from, and the rules a step can use.
fn read_team_flow(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let scope = TeamFlowScope::from_params(params);
    let (project_id, workspace_id) = match &scope {
        TeamFlowScope::Project(id) => (Some(id.as_str()), None),
        TeamFlowScope::Workspace(id) => (None, Some(id.as_str())),
        TeamFlowScope::Default => (None, None),
    };
    let (steps, source) = resolve_team_flow(&state.paths, project_id, workspace_id);
    Ok(json!({ "steps": steps, "source": source, "rules": team_flow_rule_catalog() }))
}

/// `{ projectId? | workspaceId? | scope: "default", steps }` saves; `reset: true` removes the
/// scope's own steps so it uses the next one up again.
fn update_team_flow(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let scope = TeamFlowScope::from_params(params);
    let steps = if params.get("reset").and_then(Value::as_bool) == Some(true) {
        None
    } else {
        Some(validate_team_flow_steps(params.get("steps").ok_or_else(
            || DomainStateError::bad_request("Pass steps, or reset: true."),
        )?)?)
    };
    store_team_flow(&state.paths, &scope, steps).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not save the team flow: {error}"),
    })?;
    read_team_flow(state, params)
}
