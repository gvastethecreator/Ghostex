//! The workspace each window shows, saved with its slot, and the window moves the workspace tile's
//! menu asks for: bring forward the window that already shows a workspace, open one in a new
//! window, and open the Workspaces settings page.
//!
//! CDXC:Workspaces 2026-10-09 DECISION:
//! User: a window shows one workspace at a time; the tile's menu opens a workspace in a new window,
//! and choosing a workspace another window already shows brings that window forward instead of
//! switching this one. File > New Window opens on the workspace of the window it came from.
//! SEE-ALSO: apps/desktop/src/app/gx_store/workspaces.rs (the menu's commands), packages/gx-core/src/sidebar_view/workspaces.rs (the filter).

use std::fs;
use std::path::PathBuf;

use gpui::App;

use super::open::{WorkspaceWindowStart, open_new_workspace_window_on};
use super::registry::WORKSPACE_WINDOWS;
use super::slots::workspace_window_state_path;
use crate::app::helpers::*;
use crate::*;

fn window_workspace_path(slot: u32) -> PathBuf {
    workspace_window_state_path(
        ghostex_state_root().join("gpui-window-workspace.json"),
        slot,
    )
}

/// The workspace the slot's window showed when the app last quit; `None` = the default one.
pub(crate) fn saved_window_workspace_id(slot: u32) -> Option<String> {
    fs::read_to_string(window_workspace_path(slot))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| value.get("workspaceId")?.as_str().map(str::to_string))
        .filter(|id| !id.trim().is_empty())
}

pub(super) fn discard_window_workspace(slot: u32) {
    let _ = fs::remove_file(window_workspace_path(slot));
}

impl GhostexGpuiApp {
    /// Called once the window's app exists: the workspace it starts on.
    pub(crate) fn restore_window_workspace(&mut self, start: &WorkspaceWindowStart) {
        let workspace_id = match start {
            WorkspaceWindowStart::Restore { slot, .. } => saved_window_workspace_id(*slot),
            WorkspaceWindowStart::New { workspace_id, .. } => workspace_id.clone(),
        };
        self.gx_store_init_window_workspace(workspace_id);
        self.persist_window_workspace_id();
    }

    /// Saves the window's workspace with its slot.
    pub(crate) fn persist_window_workspace_id(&self) {
        let path = window_workspace_path(self.workspace_window_slot);
        match self.gx_store_window_workspace_id() {
            Some(workspace_id) => {
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(
                    path,
                    serde_json::json!({ "workspaceId": workspace_id }).to_string(),
                );
            }
            None => {
                let _ = fs::remove_file(path);
            }
        }
    }

    /// Brings forward another window that shows `workspace_id`; `false` when none does.
    pub(crate) fn focus_window_showing_workspace(
        &self,
        workspace_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let own = cx.entity_id();
        let windows: Vec<(gpui::AnyWindowHandle, gpui::WeakEntity<GhostexGpuiApp>)> =
            WORKSPACE_WINDOWS.with(|windows| {
                windows
                    .borrow()
                    .iter()
                    .filter(|entry| entry.app.entity_id() != own && !entry.closing)
                    .map(|entry| (entry.handle, entry.app.clone()))
                    .collect()
            });
        let target = windows.into_iter().find(|(_, app)| {
            app.upgrade().is_some_and(|app| {
                app.read(cx)
                    .gx_store_resolved_window_workspace_id()
                    .as_deref()
                    == Some(workspace_id)
            })
        });
        let Some((handle, _)) = target else {
            return false;
        };
        cx.defer(move |cx: &mut App| {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
        });
        true
    }

    /// A new window on `workspace_id`, cascaded from this one.
    pub(crate) fn open_workspace_in_new_window(
        &self,
        workspace_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let own = cx.entity_id();
        let source = WORKSPACE_WINDOWS.with(|windows| {
            windows
                .borrow()
                .iter()
                .find(|entry| entry.app.entity_id() == own)
                .map(|entry| (entry.handle, entry.app.clone()))
        });
        let workspace_id = workspace_id.to_string();
        cx.defer(move |cx: &mut App| open_new_workspace_window_on(source, Some(workspace_id), cx));
    }

    /// Settings on its Workspaces page.
    pub(crate) fn open_workspace_settings_page(&mut self, cx: &mut gpui::Context<Self>) {
        self.open_gpui_settings_tab_from_new_thread_picker("workspaces", cx);
    }
}
