//! The per-project Work mode switch, stored as `launchSettings.workMode` on the project.

use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};

pub(crate) fn project_work_mode(project: &Value) -> bool {
    project
        .get("launchSettings")
        .and_then(|settings| settings.get("workMode"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Turns work mode on or off by hand. `launchSettings` is replaced whole on update, so the rest of
/// it is read first and written back unchanged.
///
/// CDXC:WorkMode 2026-10-09 WHY:
/// A hand-set value is stored even when it is `false` and loses `workModeFromWorkspace`, so moving
/// the project or changing its workspace's kind keeps it (`crate::workspaces::place_project_in_workspace`).
pub(crate) fn set_project_work_mode(
    repository: &DomainRepository<'_>,
    project: &Value,
    enabled: bool,
) -> Result<Value, DomainStateError> {
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut launch_settings = project
        .get("launchSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    launch_settings.insert("workMode".to_string(), Value::Bool(enabled));
    launch_settings.remove("workModeFromWorkspace");
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project_id));
    update.insert("launchSettings".to_string(), Value::Object(launch_settings));
    repository.update_project(&update)
}

/// The project a request names: `projectId`, else the project whose folder holds `path`
/// (the deepest one, so a worktree project wins over its parent checkout).
pub(crate) fn resolve_work_mode_project(
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    if let Some(project_id) = params
        .get("projectId")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
    {
        return repository.get_project(project_id)?.ok_or_else(|| {
            DomainStateError::bad_request(format!("No project matched \"{project_id}\"."))
        });
    }
    let path = params
        .get("path")
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
        .ok_or_else(|| DomainStateError::bad_request("Pass projectId or path."))?;
    let wanted = comparable_path(path);
    let mut best: Option<(usize, Value)> = None;
    for project in repository.list_projects()? {
        let Some(project_path) = project.get("path").and_then(Value::as_str) else {
            continue;
        };
        let candidate = comparable_path(project_path);
        if candidate.is_empty() {
            continue;
        }
        let inside = wanted == candidate || wanted.starts_with(&format!("{candidate}/"));
        if inside
            && best
                .as_ref()
                .is_none_or(|(length, _)| candidate.len() > *length)
        {
            best = Some((candidate.len(), project));
        }
    }
    best.map(|(_, project)| project)
        .ok_or_else(|| DomainStateError::bad_request(format!("No project holds \"{path}\".")))
}

fn comparable_path(path: &str) -> String {
    let path = path.trim().replace('\\', "/");
    let path = path.trim_end_matches('/').to_string();
    if cfg!(windows) || cfg!(target_os = "macos") {
        path.to_lowercase()
    } else {
        path
    }
}
