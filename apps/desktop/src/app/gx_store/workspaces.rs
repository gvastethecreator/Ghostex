//! Workspaces in the sidebar: the workspace this window shows (a sidebar input, so gx-core filters
//! the projects and Spaces by it), the workspace tile's menu commands, and Move to workspace.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_menu/workspace.rs (the menus),
//! packages/gx-core/src/sidebar_view/workspaces.rs (the filter), server/src/workspaces/ and
//! server/src/server/route_http/workspaces.rs (the document and its routes),
//! apps/desktop/src/app/workspace_windows/window_workspace.rs (saving it with the window).

use ghostex_gx_core::MachineId;
use ghostex_gx_core::protocol::{SidebarWorkspace, SidebarWorkspacesState};
use serde_json::{Value, json};

use super::rpc::gxserver_rpc_result_task;
use crate::GhostexGpuiApp;

/// What the workspace tile draws.
pub(crate) struct WorkspaceTile {
    pub(crate) name: String,
    pub(crate) letter: String,
    pub(crate) color: String,
    /// The tile's menu, as the sidebar menu JSON.
    pub(crate) menu: Value,
}

impl GhostexGpuiApp {
    /// This computer's workspaces; `None` until its daemon published them (or an older daemon).
    pub(crate) fn gx_store_workspaces_state(&self) -> Option<&SidebarWorkspacesState> {
        self.gx_store
            .core
            .presentation()
            .machine(&MachineId::Local)?
            .side_state()
            .workspaces
            .as_ref()
    }

    /// The workspace id this window was set to, as saved (`None` = the default workspace).
    pub(crate) fn gx_store_window_workspace_id(&self) -> Option<String> {
        self.gx_store
            .sidebar_list
            .last_inputs
            .host
            .window_workspace_id
            .clone()
    }

    /// The workspace this window shows, resolved against the document (a deleted one reads as the
    /// default); `None` before the document arrived.
    pub(crate) fn gx_store_resolved_window_workspace_id(&self) -> Option<String> {
        let window_workspace_id = self.gx_store_window_workspace_id();
        self.gx_store_workspaces_state()
            .map(|state| state.resolve(window_workspace_id.as_deref()).to_string())
    }

    /// The window's workspace unless it is the default one: what a new Space and the Browser's
    /// profile take, where the default workspace is written as no id.
    pub(crate) fn gx_store_window_non_default_workspace_id(&self) -> Option<String> {
        let state = self.gx_store_workspaces_state()?;
        let window_workspace_id = self.gx_store_window_workspace_id();
        let resolved = state.resolve(window_workspace_id.as_deref());
        (resolved != state.default_id()).then(|| resolved.to_string())
    }

    pub(crate) fn gx_store_window_workspace(&self) -> Option<&SidebarWorkspace> {
        let state = self.gx_store_workspaces_state()?;
        let window_workspace_id = self.gx_store_window_workspace_id();
        state
            .workspaces
            .get(state.resolve(window_workspace_id.as_deref()))
    }

    /// Sets the window's workspace before its first list is built.
    pub(crate) fn gx_store_init_window_workspace(&mut self, workspace_id: Option<String>) {
        self.gx_store
            .sidebar_list
            .last_inputs
            .host
            .window_workspace_id = workspace_id;
    }

    /// Switches this window to another workspace: the list is rebuilt on it and the choice is
    /// saved with the window.
    pub(crate) fn gx_store_set_window_workspace(
        &mut self,
        workspace_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.gx_store_window_workspace_id() == workspace_id {
            return;
        }
        self.gx_store
            .sidebar_list
            .last_inputs
            .host
            .window_workspace_id = workspace_id;
        self.persist_window_workspace_id();
        self.gx_store_sidebar_state_changed(cx);
        cx.notify();
    }

    /// The tile left of the Spaces row, when this computer's daemon has workspaces.
    pub(crate) fn gx_store_workspace_tile(&self) -> Option<WorkspaceTile> {
        let state = self.gx_store_workspaces_state()?;
        let workspace = self.gx_store_window_workspace()?;
        Some(WorkspaceTile {
            name: workspace.name.clone(),
            letter: workspace.letter.clone(),
            color: workspace.color.clone(),
            menu: ghostex_gx_core::menu_to_json(&ghostex_gx_core::workspace_menu(
                state,
                &workspace.workspace_id,
            )),
        })
    }

    /// The workspace tile's menu commands and a project's Move to workspace rows.
    pub(crate) fn gx_store_run_workspaces(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let text = |key: &str| {
            command
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        match command.get("type").and_then(Value::as_str) {
            Some("selectWorkspace") => {
                let Some(workspace_id) = text("workspaceId") else {
                    return true;
                };
                if self.gx_store_resolved_window_workspace_id().as_deref() == Some(&workspace_id) {
                    return true;
                }
                if !self.focus_window_showing_workspace(&workspace_id, cx) {
                    self.gx_store_set_window_workspace(Some(workspace_id), cx);
                }
                true
            }
            Some("openWorkspaceWindow") => {
                if let Some(workspace_id) = text("workspaceId") {
                    self.open_workspace_in_new_window(&workspace_id, cx);
                }
                true
            }
            Some("openWorkspaceSettings") => {
                self.open_workspace_settings_page(cx);
                true
            }
            Some("newWorkspace") => {
                let move_project_id = text("moveProjectId");
                self.gx_store_create_workspace(move_project_id, cx);
                true
            }
            Some("command")
                if command["message"].get("type").and_then(Value::as_str)
                    == Some("moveProjectToWorkspace") =>
            {
                let message = &command["message"];
                let (Some(project_id), Some(workspace_id)) = (
                    message.get("projectId").and_then(Value::as_str),
                    message.get("workspaceId").and_then(Value::as_str),
                ) else {
                    return true;
                };
                self.gx_store_move_project_to_workspace(
                    project_id.to_string(),
                    workspace_id.to_string(),
                    cx,
                );
                true
            }
            _ => false,
        }
    }

    fn gx_store_move_project_to_workspace(
        &mut self,
        project_id: String,
        workspace_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = gxserver_rpc_result_task(
                &background,
                "/api/moveProjectToWorkspace",
                json!({ "projectId": project_id, "workspaceId": workspace_id }),
                super::sidebar_lifecycle::rpc_timeout(),
            )
            .await;
            if let Err(message) = result {
                let _ = this.update(cx, |this, cx| {
                    this.dispatch_gpui_workspace_action_toast(
                        "error",
                        "Couldn't move the project",
                        &message,
                        cx,
                    );
                });
            }
        })
        .detach();
    }

    /// "New workspace…": creates a Work workspace, moves the project the menu was opened on into
    /// it, and opens the Workspaces settings page to name it and set it up.
    fn gx_store_create_workspace(
        &mut self,
        move_project_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let created = gxserver_rpc_result_task(
                &background,
                "/api/createWorkspace",
                json!({ "name": "New workspace", "kind": "work" }),
                super::sidebar_lifecycle::rpc_timeout(),
            )
            .await;
            let workspace_id = match created {
                Ok(result) => result
                    .get("workspaceId")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                Err(message) => {
                    let _ = this.update(cx, |this, cx| {
                        this.dispatch_gpui_workspace_action_toast(
                            "error",
                            "Couldn't create the workspace",
                            &message,
                            cx,
                        );
                    });
                    return;
                }
            };
            if let (Some(project_id), Some(workspace_id)) = (move_project_id, workspace_id) {
                let moved = gxserver_rpc_result_task(
                    &background,
                    "/api/moveProjectToWorkspace",
                    json!({ "projectId": project_id, "workspaceId": workspace_id }),
                    super::sidebar_lifecycle::rpc_timeout(),
                )
                .await;
                if let Err(message) = moved {
                    let _ = this.update(cx, |this, cx| {
                        this.dispatch_gpui_workspace_action_toast(
                            "error",
                            "Couldn't move the project",
                            &message,
                            cx,
                        );
                    });
                }
            }
            let _ = this.update(cx, |this, cx| this.open_workspace_settings_page(cx));
        })
        .detach();
    }
}
