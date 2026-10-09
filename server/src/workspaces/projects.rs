//! Which workspace a project belongs to (`launchSettings.workspaceId`), moving a project between
//! workspaces, and the work-mode default a workspace gives its projects.

use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};

use super::store::*;

/// The workspace id stored on a project; `None` means the default workspace.
pub(crate) fn stored_project_workspace_id(project: &Value) -> Option<&str> {
    project
        .get("launchSettings")
        .and_then(|settings| settings.get("workspaceId"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
}

/// The workspace a project belongs to: a worktree project follows its parent checkout, and an
/// id the workspaces document no longer has reads as the default workspace.
pub(crate) fn project_workspace_id(
    workspaces: &Value,
    project: &Value,
    projects: &[Value],
) -> String {
    let parent = project
        .get("worktree")
        .and_then(|worktree| worktree.get("parentProjectId"))
        .and_then(Value::as_str)
        .and_then(|parent_id| {
            projects.iter().find(|candidate| {
                candidate.get("projectId").and_then(Value::as_str) == Some(parent_id)
            })
        });
    let id = stored_project_workspace_id(parent.unwrap_or(project))
        .or_else(|| stored_project_workspace_id(project));
    match id {
        Some(id) if workspace_exists(workspaces, id) => id.to_string(),
        _ => DEFAULT_WORKSPACE_ID.to_string(),
    }
}

/// Whether the person set this project's Work mode switch by hand. A value the workspace's
/// default wrote carries `workModeFromWorkspace`; a `workMode` without it was set by hand
/// (every switch set before workspaces existed was).
pub(crate) fn project_work_mode_set_by_hand(project: &Value) -> bool {
    let settings = project.get("launchSettings");
    settings
        .and_then(|settings| settings.get("workMode"))
        .is_some_and(Value::is_boolean)
        && settings
            .and_then(|settings| settings.get("workModeFromWorkspace"))
            .and_then(Value::as_bool)
            != Some(true)
}

/// Writes `workspaceId` (dropped for the default workspace) and, unless the person set Work mode
/// by hand, the workspace's work-mode default into the project's `launchSettings`. Returns the
/// updated project, or `None` when nothing changed.
///
/// CDXC:WorkMode 2026-10-09 DECISION:
/// User: moving a project to another workspace follows that workspace's default (Work on,
/// Personal off) unless the work mode was set by hand on that project.
///
/// CDXC:WorkMode 2026-10-09 WHY:
/// The default is written into `launchSettings.workMode` (marked `workModeFromWorkspace`) instead
/// of being looked up on every read, because `project_work_mode` runs in projections that only
/// hold the project row; every move, kind change and new project rewrites it here.
pub(crate) fn place_project_in_workspace(
    repository: &DomainRepository<'_>,
    workspaces: &Value,
    project: &Value,
    workspace_id: &str,
) -> Result<Option<Value>, DomainStateError> {
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut launch_settings = project
        .get("launchSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let before = launch_settings.clone();
    if workspace_id == DEFAULT_WORKSPACE_ID {
        launch_settings.remove("workspaceId");
    } else {
        launch_settings.insert("workspaceId".into(), json!(workspace_id));
    }
    if !project_work_mode_set_by_hand(project) {
        if workspace_kind(workspaces, workspace_id).default_work_mode() {
            launch_settings.insert("workMode".into(), Value::Bool(true));
            launch_settings.insert("workModeFromWorkspace".into(), Value::Bool(true));
        } else {
            launch_settings.remove("workMode");
            launch_settings.remove("workModeFromWorkspace");
        }
    }
    if launch_settings == before {
        return Ok(None);
    }
    let mut update = Map::new();
    update.insert("projectId".into(), json!(project_id));
    update.insert("launchSettings".into(), Value::Object(launch_settings));
    repository.update_project(&update).map(Some)
}

/// Moves a project and its worktree projects into a workspace. Returns the ids of every project
/// row that changed.
pub(crate) fn move_project_to_workspace(
    repository: &DomainRepository<'_>,
    workspaces: &Value,
    project_id: &str,
    workspace_id: &str,
) -> Result<Vec<String>, DomainStateError> {
    if !workspace_exists(workspaces, workspace_id) {
        return Err(DomainStateError::bad_request("No such workspace."));
    }
    let projects = repository.list_projects()?;
    let project = projects
        .iter()
        .find(|project| project.get("projectId").and_then(Value::as_str) == Some(project_id))
        .ok_or_else(|| {
            DomainStateError::bad_request(format!("No project matched \"{project_id}\"."))
        })?;
    let mut changed = Vec::new();
    let worktrees = projects.iter().filter(|candidate| {
        candidate
            .get("worktree")
            .and_then(|worktree| worktree.get("parentProjectId"))
            .and_then(Value::as_str)
            == Some(project_id)
    });
    for moving in std::iter::once(project).chain(worktrees) {
        if place_project_in_workspace(repository, workspaces, moving, workspace_id)?.is_some() {
            if let Some(id) = moving.get("projectId").and_then(Value::as_str) {
                changed.push(id.to_string());
            }
        }
    }
    Ok(changed)
}

/// Re-applies a workspace's work-mode default to every project in it (after its kind changed or
/// after it was deleted and its projects fell back to the default workspace). Returns the ids of
/// the project rows that changed.
pub(crate) fn reapply_workspace_defaults(
    repository: &DomainRepository<'_>,
    workspaces: &Value,
    workspace_id: &str,
) -> Result<Vec<String>, DomainStateError> {
    let projects = repository.list_projects()?;
    let mut changed = Vec::new();
    for project in &projects {
        // A project whose stored workspace was deleted resolves to the default workspace here,
        // and placing it there drops the stale id.
        if project_workspace_id(workspaces, project, &projects) != workspace_id {
            continue;
        }
        if place_project_in_workspace(repository, workspaces, project, workspace_id)?.is_some() {
            if let Some(id) = project.get("projectId").and_then(Value::as_str) {
                changed.push(id.to_string());
            }
        }
    }
    Ok(changed)
}

/// Puts a project a client just added into the workspace it names (`params.workspaceId`, the
/// window's workspace), so it shows in the window it was added from. Returns the project as it is
/// now.
pub(crate) fn place_added_project(
    repository: &DomainRepository<'_>,
    db: &rusqlite::Connection,
    project: Value,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let Some(workspace_id) = params
        .get("workspaceId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
    else {
        return Ok(project);
    };
    let workspaces = read_sidebar_workspaces(db)?;
    if !workspace_exists(&workspaces, workspace_id) {
        return Ok(project);
    }
    Ok(
        place_project_in_workspace(repository, &workspaces, &project, workspace_id)?
            .unwrap_or(project),
    )
}

/// The Claude account the workspace of `project` picked for its agents, if any.
pub(crate) fn project_claude_account_id(
    db: &rusqlite::Connection,
    project: &Value,
) -> Result<Option<String>, DomainStateError> {
    let workspaces = read_sidebar_workspaces(db)?;
    let project_workspace = match project
        .get("worktree")
        .and_then(|worktree| worktree.get("parentProjectId"))
        .and_then(Value::as_str)
    {
        // A worktree project follows its parent checkout's workspace.
        Some(parent_id) => {
            let parent = db
                .query_row(
                    "SELECT launchSettingsJson FROM projects WHERE projectId = ?1",
                    [parent_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .ok()
                .flatten()
                .and_then(|text| serde_json::from_str::<Value>(&text).ok());
            parent
                .as_ref()
                .and_then(|settings| settings.get("workspaceId"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| stored_project_workspace_id(project).map(str::to_string))
        }
        None => stored_project_workspace_id(project).map(str::to_string),
    };
    let workspace_id = match project_workspace {
        Some(id) if workspace_exists(&workspaces, &id) => id,
        _ => DEFAULT_WORKSPACE_ID.to_string(),
    };
    Ok(workspace_claude_account_id(&workspaces, &workspace_id))
}
