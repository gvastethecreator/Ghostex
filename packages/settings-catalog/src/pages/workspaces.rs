use crate::rows::{row, section, Page, Section};

pub(crate) fn page() -> Page {
    Page {
        id: "workspaces",
        title: "Workspaces",
        sections: vec![workspaces()],
    }
}

/// CDXC:Workspaces 2026-10-09 DECISION:
/// User (mockup 09): one Settings page for the workspaces, each with its name, color, kind (Work
/// or Personal, which sets the work-mode default), its Linear API key, the Claude account its
/// agents use and its own browser sign-ins.
pub(crate) fn workspaces() -> Section {
    section(
        "workspaces",
        "Workspaces",
        vec![
            row(
                "workspaceName",
                "Workspace name",
                "The name and letter shown on the workspace button left of your Spaces.",
            ),
            row(
                "workspaceColor",
                "Workspace color",
                "The color of the workspace button.",
            ),
            row(
                "workspaceKind",
                "Work or Personal",
                "Work turns work mode on for the workspace's projects by default; Personal leaves it off.",
            ),
            row(
                "workspaceLinearApiKey",
                "Linear API key",
                "The Linear key this workspace's projects use, unless a project sets its own.",
            ),
            row(
                "workspaceClaudeAccount",
                "Claude account",
                "Which of your Claude accounts agents in this workspace's projects use.",
            ),
            row(
                "workspaceBrowserSignins",
                "Browser sign-ins",
                "Each workspace's Browser keeps its own cookies; sign out of every site here.",
            ),
            row(
                "newWorkspace",
                "New workspace",
                "Add a workspace, for example one per company you work for.",
            ),
        ],
    )
}
