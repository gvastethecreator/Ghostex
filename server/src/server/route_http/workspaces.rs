//! Workspace routes (crate::workspaces): read, create, update and delete workspaces, and move a
//! project into one.

use serde_json::{json, Map, Value};

use crate::{
    constants::GXSERVER_PROTOCOL_VERSION,
    domain::{DomainRepository, DomainStateError},
    presentation::increment_presentation_revision,
    sidebar_project_collections::{
        read_sidebar_project_collections, update_sidebar_project_collections,
    },
    sidebar_spaces::rehome_sidebar_spaces_for_workspaces,
    work_mode::{store_linear_api_key, LinearKeyScope},
    workspaces::{
        create_workspace_in, delete_workspace_in, move_project_to_workspace,
        read_sidebar_workspaces, reapply_workspace_defaults, update_workspace_in, workspace_kind,
        write_sidebar_workspaces, DEFAULT_WORKSPACE_ID,
    },
};

use super::super::work_mode_sync::{publish_project_work_mode_change, spawn_work_mode_refresh};
use super::*;

pub(super) async fn route_workspaces_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    let RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        token_extension_id,
    } = request;
    Ok(match endpoint.path.as_str() {
        "/api/readWorkspaces" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, db, _, _| {
                read_sidebar_workspaces(db)
                    .map(|workspaces| json!({ "sidebarWorkspaces": workspaces }))
            },
        ),
        "/api/createWorkspace" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |_, db, params, _| {
                let _event_sequence = lock_presentation_event_sequence(&state)?;
                let current = read_sidebar_workspaces(db)?;
                let (workspace_id, next) = create_workspace_in(&current, params)?;
                let workspaces = write_sidebar_workspaces(db, &next)?;
                broadcast_workspaces(&state, db, &workspaces)?;
                Ok(json!({ "workspaceId": workspace_id, "sidebarWorkspaces": workspaces }))
            },
        ),
        "/api/updateWorkspace" => {
            let response = handle_domain_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
                |repository, db, params, _| {
                    let workspace_id = required_text(params, "workspaceId")?;
                    let (workspaces, kind_changed) = {
                        let _event_sequence = lock_presentation_event_sequence(&state)?;
                        let current = read_sidebar_workspaces(db)?;
                        let next = update_workspace_in(&current, &workspace_id, params)?;
                        let workspaces = write_sidebar_workspaces(db, &next)?;
                        if workspaces != current {
                            broadcast_workspaces(&state, db, &workspaces)?;
                        }
                        let kind_changed = workspace_kind(&current, &workspace_id)
                            != workspace_kind(&workspaces, &workspace_id);
                        (workspaces, kind_changed)
                    };
                    if kind_changed {
                        let changed =
                            reapply_workspace_defaults(repository, &workspaces, &workspace_id)?;
                        publish_projects(&state, db, repository, &changed)?;
                    }
                    Ok(json!({ "workspaceId": workspace_id, "sidebarWorkspaces": workspaces }))
                },
            );
            spawn_work_mode_refresh(&state);
            response
        }
        "/api/deleteWorkspace" => {
            let response = handle_domain_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
                |repository, db, params, _| {
                    let workspace_id = required_text(params, "workspaceId")?;
                    let workspaces = {
                        let _event_sequence = lock_presentation_event_sequence(&state)?;
                        let current = read_sidebar_workspaces(db)?;
                        let next = delete_workspace_in(&current, &workspace_id)?;
                        let workspaces = write_sidebar_workspaces(db, &next)?;
                        broadcast_workspaces(&state, db, &workspaces)?;
                        if let Some(spaces) =
                            rehome_sidebar_spaces_for_workspaces(db, Some(&workspace_id), &[])?
                        {
                            broadcast_spaces(&state, db, spaces)?;
                        }
                        workspaces
                    };
                    // Its projects fall back to the default workspace and take its default.
                    let changed =
                        reapply_workspace_defaults(repository, &workspaces, DEFAULT_WORKSPACE_ID)?;
                    publish_projects(&state, db, repository, &changed)?;
                    let _ = store_linear_api_key(
                        &state.paths,
                        LinearKeyScope::Workspace(&workspace_id),
                        None,
                    );
                    Ok(json!({ "workspaceId": workspace_id, "sidebarWorkspaces": workspaces }))
                },
            );
            spawn_work_mode_refresh(&state);
            response
        }
        "/api/moveProjectToWorkspace" => {
            let response = handle_domain_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
                |repository, db, params, _| {
                    let project_id = required_text(params, "projectId")?;
                    let workspace_id = required_text(params, "workspaceId")?;
                    let workspaces = read_sidebar_workspaces(db)?;
                    let changed = move_project_to_workspace(
                        repository,
                        &workspaces,
                        &project_id,
                        &workspace_id,
                    )?;
                    if !changed.is_empty() {
                        leave_old_workspace_groups(&state, db, &changed)?;
                    }
                    publish_projects(&state, db, repository, &changed)?;
                    Ok(json!({
                        "projectId": project_id,
                        "workspaceId": workspace_id,
                        "movedProjectIds": changed,
                    }))
                },
            );
            spawn_work_mode_refresh(&state);
            response
        }
        _ => {
            return Err(RouteHttpRequest {
                state,
                endpoint,
                request_id,
                body_json,
                token_extension_id,
            })
        }
    })
}

/// A moved project leaves its project group and the Spaces that list it, which belong to the
/// workspace it left, so it shows in the new workspace's Other view.
///
/// CDXC:Workspaces 2026-10-09 DECISION:
/// User (mockup 07): moving a project takes it out of this workspace's Spaces and puts it in the
/// other workspace's "Other" group, where you can add it to a Space; its sessions, worktrees and
/// links move with it.
fn leave_old_workspace_groups(
    state: &AppState,
    db: &rusqlite::Connection,
    moved_project_ids: &[String],
) -> Result<(), DomainStateError> {
    let _event_sequence = lock_presentation_event_sequence(state)?;
    let previous_collections = read_sidebar_project_collections(db)?;
    let mut next = previous_collections.clone();
    if let Some(collections) = next.get_mut("collections").and_then(Value::as_object_mut) {
        for collection in collections.values_mut() {
            if let Some(project_ids) = collection
                .get_mut("projectIds")
                .and_then(Value::as_array_mut)
            {
                project_ids.retain(|id| {
                    !id.as_str()
                        .is_some_and(|id| moved_project_ids.iter().any(|moved| moved == id))
                });
            }
        }
    }
    if next != previous_collections {
        let mut params = Map::new();
        params.insert("state".to_string(), next);
        let collections = update_sidebar_project_collections(db, &params)?;
        let revision = increment_presentation_revision(db)?;
        state.event_hub.broadcast(json!({
            "protocolVersion": GXSERVER_PROTOCOL_VERSION,
            "revision": revision,
            "serverId": state.metadata.server_id.clone(),
            "sidebarProjectCollections": collections.clone(),
            "type": "sidebarProjectCollectionsChanged",
        }));
        broadcast_pruned_sidebar_spaces(state, db, &previous_collections, &collections)?;
    }
    if let Some(spaces) = rehome_sidebar_spaces_for_workspaces(db, None, moved_project_ids)? {
        broadcast_spaces(state, db, spaces)?;
    }
    Ok(())
}

/// Publishes each changed project and its sessions (their cards follow the project's Work mode).
fn publish_projects(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    project_ids: &[String],
) -> Result<(), DomainStateError> {
    for project_id in project_ids {
        publish_project_work_mode_change(state, db, repository, project_id)?;
    }
    Ok(())
}

/// Callers hold the presentation event sequencer.
fn broadcast_workspaces(
    state: &AppState,
    db: &rusqlite::Connection,
    workspaces: &Value,
) -> Result<(), DomainStateError> {
    let revision = increment_presentation_revision(db)?;
    state.event_hub.broadcast(json!({
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "revision": revision,
        "serverId": state.metadata.server_id.clone(),
        "sidebarWorkspaces": workspaces.clone(),
        "type": "sidebarWorkspacesChanged",
    }));
    Ok(())
}

/// Callers hold the presentation event sequencer.
fn broadcast_spaces(
    state: &AppState,
    db: &rusqlite::Connection,
    spaces: Value,
) -> Result<(), DomainStateError> {
    let revision = increment_presentation_revision(db)?;
    state.event_hub.broadcast(json!({
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "revision": revision,
        "serverId": state.metadata.server_id.clone(),
        "sidebarSpaces": spaces,
        "sidebarSpacesEnabled": crate::sidebar_spaces::read_sidebar_spaces_enabled(&state.paths),
        "type": "sidebarSpacesChanged",
    }));
    Ok(())
}

fn required_text(params: &Map<String, Value>, key: &str) -> Result<String, DomainStateError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::bad_request(format!("Pass {key}.")))
}
