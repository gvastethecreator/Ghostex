//! The workspace tile's menu (left of the Spaces row) and a project's "Move to workspace" submenu.
//!
//! CDXC:Workspaces 2026-10-09 DECISION:
//! User (mockups 01 and 07): the workspace tile's menu lists the workspaces (the current one
//! checked), "Workspace settings…" and "New workspace…"; right-click a project → "Move to
//! workspace ▸" lists the other workspaces and "New workspace…", next to the Work mode switch.
//! User, later the same day: "remove this window item, instead add a child button on the right side
//! of workspaces that aren't currently the selected one AND don't already have a window open. When
//! that one is clicked then we open that in a new window". This supersedes the "Open <name> in a
//! new window" row.

use ghostex_gx_protocol::SidebarWorkspacesState;
use serde_json::json;

use super::commands::MenuCommand;
use super::item::{MenuItem, MenuSecondary};

/// The workspace tile's menu for a window showing `current`; `in_windows` are the workspaces some
/// window already shows, which get no new-window button.
pub fn workspace_menu(
    state: &SidebarWorkspacesState,
    current: &str,
    in_windows: &[String],
) -> Vec<MenuItem> {
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
        if workspace.workspace_id != current && !in_windows.contains(&workspace.workspace_id) {
            row.secondary = Some(MenuSecondary {
                icon: "external-link".to_string(),
                label: String::new(),
                command: MenuCommand::host(json!({
                    "type": "openWorkspaceWindow",
                    "workspaceId": workspace.workspace_id,
                })),
            });
        }
        menu.push(row);
    }
    menu.push(MenuItem::separator());
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

/// "Move to workspace ▸" on a remote machine's tab: every workspace of this computer other than
/// the one the tab shows in now (`SidebarWorkspacesState::machine_workspace`).
pub fn machine_workspace_menu(state: &SidebarWorkspacesState, machine_id: &str) -> MenuItem {
    let current = state.machine_workspace(machine_id);
    let children: Vec<MenuItem> = state
        .ordered()
        .filter(|workspace| workspace.workspace_id != current)
        .map(|workspace| {
            let mut row = MenuItem::row(
                &workspace.name,
                if workspace.is_work() {
                    "briefcase"
                } else {
                    "user"
                },
                MenuCommand::host(json!({
                    "type": "moveMachineToWorkspace",
                    "machineId": machine_id,
                    "workspaceId": workspace.workspace_id,
                })),
            );
            row.icon_color = Some(workspace.color.clone());
            row
        })
        .collect();
    MenuItem::submenu("Move to workspace", "switch-horizontal", children)
}
