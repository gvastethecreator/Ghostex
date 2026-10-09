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

/// Whether a project shows in every workspace: the Ghostex config folder's project, where every
/// Help chat (and the Cloud Boxes setup chat) lives.
///
/// CDXC:Workspaces 2026-10-09 DECISION:
/// User: the Help-chat project must not flip workspaces each time Help is opened from a different
/// window; make it visible in every workspace. It is recognised by its folder (the Ghostex config
/// folder the desktop roots it at), so no client has to mark it and installs that already have it
/// need no migration; its own `workspaceId` still decides its Linear key and Claude account.
pub(crate) fn project_in_every_workspace(project: &Value) -> bool {
    static CONFIG_DIR: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let config_dir = CONFIG_DIR.get_or_init(|| {
        comparable_path(
            &ghostex_paths::GhostexPaths::resolve()
                .config_dir
                .to_string_lossy(),
        )
    });
    !config_dir.is_empty()
        && project
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(|path| comparable_path(path) == *config_dir)
}

fn comparable_path(path: &str) -> String {
    let path = path.trim().replace('\\', "/");
    let path = path.trim_end_matches('/');
    if cfg!(windows) || cfg!(target_os = "macos") {
        path.to_lowercase()
    } else {
        path.to_string()
    }
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
/// window's workspace, or a workspace name from the CLI), so it shows in the window it was added
/// from; with none it stays in the default workspace. A worktree project of a registered checkout
/// takes its parent's workspace instead. Returns the project as it is now.
///
/// CDXC:Workspaces 2026-10-09 DECISION:
/// User: new projects land in the window's workspace from every path (Add Project, a clone from
/// Add Project, worktree projects, and `ghostex` CLI project creation, which uses Personal when it
/// has no window to go by).
pub(crate) fn place_added_project(
    repository: &DomainRepository<'_>,
    db: &rusqlite::Connection,
    project: Value,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    if project_parent_id(&project).is_some() {
        return place_worktree_project(repository, db, project);
    }
    let Some(reference) = params
        .get("workspaceId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
    else {
        return Ok(project);
    };
    let workspaces = read_sidebar_workspaces(db)?;
    let Some(workspace_id) = find_workspace_id(&workspaces, reference) else {
        return Ok(project);
    };
    Ok(
        place_project_in_workspace(repository, &workspaces, &project, &workspace_id)?
            .unwrap_or(project),
    )
}

fn project_parent_id(project: &Value) -> Option<&str> {
    project
        .get("worktree")
        .and_then(|worktree| worktree.get("parentProjectId"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
}

/// A worktree project takes its parent checkout's workspace and Work mode (as the parent has it:
/// from its workspace or set by hand), unless its own switch was set by hand. Returns the project
/// as it is now.
///
/// CDXC:WorkMode 2026-10-09 WHY:
/// Work mode is read from the project's own row (`project_work_mode`), and work happens in
/// worktree projects, so a ticket's worktree in a Work workspace (or under a Personal project with
/// Work mode turned on by hand) needs the parent's value written on it or its cards lose their
/// work chips.
pub(crate) fn place_worktree_project(
    repository: &DomainRepository<'_>,
    db: &rusqlite::Connection,
    project: Value,
) -> Result<Value, DomainStateError> {
    let Some(parent) = project_parent_id(&project)
        .map(|parent_id| repository.get_project(parent_id))
        .transpose()?
        .flatten()
    else {
        return Ok(project);
    };
    let workspaces = read_sidebar_workspaces(db)?;
    let workspace_id = match stored_project_workspace_id(&parent) {
        Some(id) if workspace_exists(&workspaces, id) => id.to_string(),
        _ => DEFAULT_WORKSPACE_ID.to_string(),
    };
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
    if !project_work_mode_set_by_hand(&project) {
        let parent_settings = parent.get("launchSettings");
        for key in ["workMode", "workModeFromWorkspace"] {
            match parent_settings.and_then(|settings| settings.get(key)) {
                Some(value) => {
                    launch_settings.insert(key.into(), value.clone());
                }
                None => {
                    launch_settings.remove(key);
                }
            }
        }
    }
    if launch_settings == before {
        return Ok(project);
    }
    let mut update = Map::new();
    update.insert(
        "projectId".into(),
        project.get("projectId").cloned().unwrap_or(Value::Null),
    );
    update.insert("launchSettings".into(), Value::Object(launch_settings));
    repository.update_project(&update)
}

/// The project `reference` names: a project id, else a project name (any case, when only one
/// project has it), else a folder path inside a project (the deepest project wins).
pub(crate) fn resolve_project_reference(
    repository: &DomainRepository<'_>,
    reference: &str,
) -> Result<String, DomainStateError> {
    let reference = reference.trim();
    if reference.is_empty() {
        return Err(DomainStateError::bad_request("Pass the project."));
    }
    let projects = repository.list_projects()?;
    let id_of = |project: &Value| {
        project
            .get("projectId")
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    if let Some(id) = projects.iter().filter_map(id_of).find(|id| id == reference) {
        return Ok(id);
    }
    let named: Vec<&Value> = projects
        .iter()
        .filter(|project| {
            project
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name.trim().eq_ignore_ascii_case(reference))
        })
        .collect();
    match named.as_slice() {
        [only] => {
            return id_of(only).ok_or_else(|| DomainStateError::bad_request("No project id."))
        }
        [_, _, ..] => {
            return Err(DomainStateError::bad_request(format!(
                "Several projects are named \"{reference}\"; pass its id or folder."
            )))
        }
        [] => {}
    }
    let mut params = Map::new();
    params.insert("path".into(), json!(reference));
    let project =
        crate::work_mode::resolve_work_mode_project(repository, &params).map_err(|_| {
            DomainStateError::bad_request(format!("No project matched \"{reference}\"."))
        })?;
    id_of(&project).ok_or_else(|| DomainStateError::bad_request("No project id."))
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
