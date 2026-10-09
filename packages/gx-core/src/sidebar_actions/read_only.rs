//! The actions that only read: the copy actions, Open Folder, and Open in Editor.
//!
//! These are the first of the roughly forty-five payloads to move into the store, because none of
//! them changes a session, writes client storage, or needs an optimistic update: each one resolves
//! an id and makes exactly one call. What they establish is the shape the rest follow, so the
//! resolution is ported line by line from the shipped TypeScript and the calls are values a gate
//! can enumerate.
//!
//! Ported from these files of the deleted `apps/desktop/sidebar/gxserver-runtime/` (see git
//! history): `core.ts` (the `handleSidebarMessage` arms), `app-shot-and-misc.ts`
//! (`postProjectPathActionForGroup`, `copyWorkspaceProjectRemoteUrl`), `sessions-and-focus.ts`
//! (`copySessionDetails`) and `remote-machines.ts` (`postRemoteProjectNativeAction`,
//! `postRemoteToast`).

use serde_json::Value;

use crate::core::Core;
use crate::keys::ProjectKey;
use crate::sidebar_view::text::js_trim;
use crate::sidebar_view::SidebarInputs;

use super::plan::{ActionEffect, SidebarActionPlan, ToastLevel};
use super::resolve::{
    local_project_group_project_id, native_project_path_action, non_empty, text_field,
};

/// Every message type this file answers. The host checks it before it handles a command (it kept
/// such commands out of the old runtime, and the deleted parity gate enumerated it), so the set
/// lives in one place.
pub const READ_ONLY_MESSAGE_TYPES: [&str; 7] = [
    "copySessionDetails",
    "copyText",
    "copyWorkspaceProjectPathForGroup",
    "copyWorkspaceProjectRemoteUrl",
    "openWorkspaceProjectInFinderForGroup",
    "openWorkspaceProjectInIdeForGroup",
    "openWorkspaceProjectInTargetForGroup",
];

/// The calls one read-only message makes, or `None` when this file does not own that message.
///
/// `message` is the inner payload of a `{ type: 'command', message }` menu command, which is what
/// `GpuiSidebarRuntime.handleSidebarMessage` receives.
pub fn plan_read_only_action(
    core: &Core,
    inputs: &SidebarInputs,
    message: &Value,
) -> Option<SidebarActionPlan> {
    let kind = text_field(message, "type")?;
    match kind {
        // `copySessionDetails`: the text the menu built, straight to the clipboard. The value is
        // NOT trimmed on the way, because `normalizeNonEmptyString` tests the trim and returns the
        // original string.
        "copySessionDetails" => Some(copy_text(text_field(message, "detailsText"))),
        // `copyText`: a session menu's Copy row (a branch, a Linear ID, a link).
        "copyText" => Some(copy_text(text_field(message, "text"))),
        // `copyWorkspaceProjectRemoteUrl`: the same call with the menu's URL.
        "copyWorkspaceProjectRemoteUrl" => Some(copy_text(text_field(message, "remoteUrl"))),
        "copyWorkspaceProjectPathForGroup" => Some(project_path_action(
            core,
            inputs,
            message,
            ProjectPathAction::CopyPath,
        )),
        "openWorkspaceProjectInFinderForGroup" => Some(project_path_action(
            core,
            inputs,
            message,
            ProjectPathAction::OpenInFinder,
        )),
        "openWorkspaceProjectInIdeForGroup" => Some(project_path_action(
            core,
            inputs,
            message,
            ProjectPathAction::OpenInIde,
        )),
        // A bot row's Open in names one Open In target by id; the host runs the target it finds
        // under that id in its own Settings, so no command text travels with the message.
        "openWorkspaceProjectInTargetForGroup" => Some(
            text_field(message, "targetId")
                .map(js_trim)
                .filter(|target_id| !target_id.is_empty())
                .map_or_else(SidebarActionPlan::nothing, |target_id| {
                    project_path_action(
                        core,
                        inputs,
                        message,
                        ProjectPathAction::OpenInTarget(target_id),
                    )
                }),
        ),
        _ => None,
    }
}

fn copy_text(value: Option<&str>) -> SidebarActionPlan {
    match non_empty(value) {
        Some(text) => SidebarActionPlan::one(ActionEffect::CopyText {
            text: text.to_string(),
        }),
        None => SidebarActionPlan::nothing(),
    }
}

/// The actions `postProjectPathActionForGroup` accepts, plus a bot row's Open in, with what each
/// one does on a remote project. Only Copy Path and Open in Editor have a remote form; the local
/// file manager and the local Open In targets cannot open a path on another computer, and the
/// TypeScript says so in a toast rather than failing silently.
#[derive(Clone, Copy)]
enum ProjectPathAction<'a> {
    CopyPath,
    OpenInFinder,
    OpenInIde,
    /// The Open In target id the row names.
    OpenInTarget(&'a str),
}

impl ProjectPathAction<'_> {
    fn local_action(self) -> &'static str {
        match self {
            Self::CopyPath => "copyWorkspaceProjectPath",
            Self::OpenInFinder => "openWorkspaceProjectInFinder",
            Self::OpenInIde => "openWorkspaceProjectInIde",
            Self::OpenInTarget(_) => "openWorkspaceProjectInTarget",
        }
    }

    fn remote_action(self) -> Option<&'static str> {
        match self {
            Self::CopyPath => Some("copyRemoteProjectPath"),
            Self::OpenInIde => Some("openRemoteWorkspaceProjectInIde"),
            Self::OpenInFinder | Self::OpenInTarget(_) => None,
        }
    }
}

fn project_path_action(
    core: &Core,
    inputs: &SidebarInputs,
    message: &Value,
    action: ProjectPathAction<'_>,
) -> SidebarActionPlan {
    let Some(group_id) = text_field(message, "groupId") else {
        return SidebarActionPlan::nothing();
    };
    let remote =
        ProjectKey::parse_sidebar_group_id(group_id).filter(|project| !project.machine.is_local());
    if let Some(project) = remote {
        let Some(remote_action) = action.remote_action() else {
            return SidebarActionPlan::one(ActionEffect::Toast {
                level: ToastLevel::Warning,
                title: "Remote project open unavailable".to_string(),
                description: Some(
                    "Remote project locations cannot be opened in the local file manager."
                        .to_string(),
                ),
            });
        };
        return match native_project_path_action(remote_action, &project.to_workspace_project_id()) {
            Some(effect) => SidebarActionPlan::one(effect),
            None => SidebarActionPlan::nothing(),
        };
    }
    let Some(project_id) = local_project_group_project_id(core, inputs, group_id) else {
        return SidebarActionPlan::nothing();
    };
    let mut effect = native_project_path_action(action.local_action(), &project_id);
    if let (
        ProjectPathAction::OpenInTarget(target_id),
        Some(ActionEffect::NativeProjectPathAction { payload }),
    ) = (action, effect.as_mut())
    {
        payload["targetId"] = Value::String(target_id.to_string());
    }
    match effect {
        Some(effect) => SidebarActionPlan::one(effect),
        None => SidebarActionPlan::nothing(),
    }
}
