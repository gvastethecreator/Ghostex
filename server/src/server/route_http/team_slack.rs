//! Slack command flow routes (crate::team_sync): the team flow settings, the Slack app manifest,
//! and `ghostex slack post`. Every one uses this computer's member token, so they are full-local.

use serde_json::{Map, Value};

use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;
use crate::team_sync;

use super::*;

type TeamSlackOperation = fn(&GxserverPaths, &Map<String, Value>) -> Result<Value, String>;

fn paths_operation(path: &str) -> Option<TeamSlackOperation> {
    Some(match path {
        "/api/readTeamFlow" => team_sync::read_team_flow,
        "/api/setTeamFlow" => team_sync::set_team_flow,
        "/api/readSlackManifest" => team_sync::slack_manifest,
        _ => return None,
    })
}

pub(super) async fn route_team_slack_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    let path = request.endpoint.path.as_str();
    let operation = paths_operation(path);
    if operation.is_none() && path != "/api/postSlackWorkingThread" {
        return Err(request);
    }
    let RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        ..
    } = request;
    // Off the async runtime: every operation is a network call to the team's Convex project.
    let worker_state = state.clone();
    let worker_endpoint = endpoint.path.clone();
    let worker_request_id = request_id.clone();
    let response = tokio::task::spawn_blocking(move || {
        handle_domain_http(
            &worker_state,
            worker_endpoint,
            worker_request_id,
            &body_json,
            |_, _, params, _| match operation {
                Some(operation) => {
                    operation(&worker_state.paths, params).map_err(DomainStateError::bad_request)
                }
                None => team_sync::post_slack_working_thread(&worker_state, params),
            },
        )
    })
    .await;
    Ok(match response {
        Ok(response) => response,
        Err(error) => domain_error_response(
            endpoint.path,
            request_id,
            DomainStateError::corrupt_state(format!("The Slack request failed: {error}")),
        ),
    })
}
