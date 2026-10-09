//! The workspace a window shows: which of this computer's projects and Spaces it lists.
//!
//! CDXC:Workspaces 2026-10-09 DECISION:
//! User: each workspace has its own Spaces and projects, and a window shows one workspace at a
//! time. A project or Space with no `workspaceId` belongs to the default workspace, and a worktree
//! project follows its parent checkout's.
//!
//! CDXC:Workspaces 2026-10-09 WHY:
//! Only this computer's projects are filtered one by one: the window's workspace is one of this
//! computer's workspaces, and a remote machine's projects carry that machine's own workspace ids,
//! so a remote machine's tab is placed whole instead (`window_machine_tabs`). A daemon without
//! workspaces (no `sidebarWorkspaces`) is never filtered.

use std::borrow::Cow;

use ghostex_gx_protocol::{
    PresentationProject, SidebarSpacesState, SidebarWorkspace, SidebarWorkspacesState,
};

use crate::keys::MachineId;
use crate::presentation_store::{LoadedPresentation, PresentationStore};

use super::inputs::{MachineTabInput, SidebarInputs};

/// One machine's workspaces and the one this window shows there.
pub struct WindowWorkspace<'a> {
    pub state: &'a SidebarWorkspacesState,
    pub workspace_id: &'a str,
}

impl<'a> WindowWorkspace<'a> {
    pub fn workspace(&self) -> Option<&'a SidebarWorkspace> {
        self.state.workspaces.get(self.workspace_id)
    }

    /// Whether a Space with this `workspaceId` belongs to the window's workspace.
    pub fn shows_space(&self, space_workspace_id: Option<&str>) -> bool {
        self.state.resolve(space_workspace_id) == self.workspace_id
    }

    /// Whether a project belongs to the window's workspace.
    pub fn shows_project(&self, loaded: &LoadedPresentation, project_id: &str) -> bool {
        match loaded.project(project_id) {
            Some(project) => {
                project.every_workspace
                    || project_workspace_id(self.state, loaded, project) == self.workspace_id
            }
            // Rows the presentation does not hold (chats, placeholders) are in every workspace.
            None => true,
        }
    }
}

/// The workspace a project belongs to: its own, or its parent checkout's for a worktree project.
pub fn project_workspace_id<'a>(
    state: &'a SidebarWorkspacesState,
    loaded: &'a LoadedPresentation,
    project: &'a PresentationProject,
) -> &'a str {
    let parent = project
        .worktree
        .as_ref()
        .and_then(|worktree| worktree.get("parentProjectId"))
        .and_then(|id| id.as_str())
        .and_then(|parent_id| loaded.project(parent_id));
    let id = parent
        .and_then(|parent| parent.workspace_id.as_deref())
        .or(project.workspace_id.as_deref());
    state.resolve(id)
}

/// The workspace the window shows on `machine`; `None` when that section is not filtered.
pub fn window_workspace<'a>(
    store: &'a PresentationStore,
    inputs: &'a SidebarInputs,
    machine: &MachineId,
) -> Option<WindowWorkspace<'a>> {
    if !machine.is_local() {
        return None;
    }
    let state = store.machine(machine)?.side_state().workspaces.as_ref()?;
    Some(WindowWorkspace {
        state,
        workspace_id: state.resolve(inputs.host.window_workspace_id.as_deref()),
    })
}

/// `machine`'s Space document as this window sees it: only the Spaces of the window's workspace.
pub fn window_spaces<'a>(
    store: &'a PresentationStore,
    inputs: &'a SidebarInputs,
    machine: &MachineId,
) -> Option<Cow<'a, SidebarSpacesState>> {
    let spaces = store.machine(machine)?.side_state().spaces.as_ref()?;
    let Some(workspace) = window_workspace(store, inputs, machine) else {
        return Some(Cow::Borrowed(spaces));
    };
    let mut filtered = spaces.clone();
    filtered
        .spaces
        .retain(|_, space| workspace.shows_space(space.workspace_id.as_deref()));
    filtered
        .order
        .retain(|space_id| filtered.spaces.contains_key(space_id));
    Some(Cow::Owned(filtered))
}

/// The machine tabs a window shows: this computer always, and a remote machine only in the
/// workspace it was put in on this computer (Personal until moved with Move to workspace on its
/// tab). Every tab while this computer's daemon has no workspaces.
///
/// CDXC:Workspaces 2026-10-09 DECISION:
/// User: remote machines' sidebar sections follow the window's workspace too. A remote machine's
/// projects carry that machine's workspace ids, which do not match this computer's, so the rule is
/// the simplest correct one: the machine's tab shows in the workspace it was assigned to on this
/// computer, assignable with Move to workspace on the tab, defaulting to Personal.
pub fn window_machine_tabs(
    store: &PresentationStore,
    window_workspace_id: Option<&str>,
    tabs: &[MachineTabInput],
) -> Vec<MachineTabInput> {
    let Some(state) = store
        .machine(&MachineId::Local)
        .and_then(|machine| machine.side_state().workspaces.as_ref())
    else {
        return tabs.to_vec();
    };
    let window = state.resolve(window_workspace_id);
    tabs.iter()
        .filter(|tab| tab.is_local() || state.machine_workspace(&tab.machine_id) == window)
        .cloned()
        .collect()
}
