//! The workspace tile's menu (left of the Spaces row) and a project's "Move to workspace" submenu.
//!
//! CDXC:Workspaces 2026-10-09 DECISION:
//! User (mockups 01 and 07): the workspace tile's menu lists the workspaces (the current one
//! checked), "Open <name> in a new window", "Workspace settings…" and "New workspace…"; right-click
//! a project → "Move to workspace ▸" lists the other workspaces and "New workspace…", next to the
//! Work mode switch.

use ghostex_gx_protocol::SidebarWorkspacesState;
use serde_json::json;

use super::commands::MenuCommand;
use super::item::MenuItem;

/// The workspace tile's menu for a window showing `current`.
pub fn workspace_menu(state: &SidebarWorkspacesState, current: &str) -> Vec<MenuItem> {
    let mut menu = vec![MenuItem::heading("Workspaces")];
    for workspace in state.ordered() {
        let mut row = MenuItem::row(
            &workspace.name,
            if workspace.is_work() {
                "briefcase"
            } else {
                "user"
            },
            MenuCommand::host(json!({
                "type": "selectWorkspace",
                "workspaceId": workspace.workspace_id,
            })),
        )
        .with_checked(workspace.workspace_id == current);
        row.icon_color = Some(workspace.color.clone());
        menu.push(row);
    }
    menu.push(MenuItem::separator());
    if let Some(workspace) = state.workspaces.get(current) {
        menu.push(MenuItem::row(
            &format!("Open {} in a new window", workspace.name),
            "external-link",
            MenuCommand::host(json!({
                "type": "openWorkspaceWindow",
                "workspaceId": workspace.workspace_id,
            })),
        ));
    }
    menu.push(MenuItem::row(
        "Workspace settings…",
        "settings",
        MenuCommand::host(json!({ "type": "openWorkspaceSettings", "workspaceId": current })),
    ));
    menu.push(MenuItem::row(
        "New workspace…",
        "plus",
        MenuCommand::host(json!({ "type": "newWorkspace" })),
    ));
    menu
}

/// "Move to workspace ▸" for a project in `project_workspace_id`: every other workspace, then
/// "New workspace…", which creates one and moves the project into it.
pub(crate) fn move_to_workspace_menu(
    state: &SidebarWorkspacesState,
    project_id: &str,
    project_workspace_id: &str,
) -> MenuItem {
    let mut children: Vec<MenuItem> = state
        .ordered()
        .filter(|workspace| workspace.workspace_id != project_workspace_id)
        .map(|workspace| {
            let mut row = MenuItem::row(
                &workspace.name,
                if workspace.is_work() {
                    "briefcase"
                } else {
                    "user"
                },
                MenuCommand::command(json!({
                    "type": "moveProjectToWorkspace",
                    "projectId": project_id,
                    "workspaceId": workspace.workspace_id,
                })),
            );
            row.icon_color = Some(workspace.color.clone());
            row
        })
        .collect();
    if !children.is_empty() {
        children.push(MenuItem::separator());
    }
    children.push(MenuItem::row(
        "New workspace…",
        "plus",
        MenuCommand::host(json!({ "type": "newWorkspace", "moveProjectId": project_id })),
    ));
    MenuItem::submenu("Move to workspace", "switch-horizontal", children)
}
