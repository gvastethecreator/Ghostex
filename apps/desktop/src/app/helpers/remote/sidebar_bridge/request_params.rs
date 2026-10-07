use super::*;
use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_remote_sidebar_request_params(
    path: &str,
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    /*
    CDXC:RemoteMachines 2026-06-24-18:22:
    The remote sidebar bridge allowlists project mutation endpoints only for id-scoped operations. Shape params at the Rust boundary so CEF cannot tunnel arbitrary updateProject fields, paths, names, commands, URLs, branch refs, tokens, stdout/stderr, or daemon response authority to a remote gxserver.
    */
    match path {
        "/api/updateProject" => gpui_remote_sidebar_update_project_params(params),
        "/api/readAgentHookStatus" | "/api/installAgentHooks" => {
            gpui_remote_sidebar_agent_hook_params(params)
        }
        "/api/readSidebarHud" => gpui_remote_sidebar_read_sidebar_hud_params(params),
        "/api/updateSidebarProjectCollections" => {
            gpui_remote_sidebar_project_collections_params(params)
        }
        "/api/updateSidebarSpaces" => gpui_remote_sidebar_spaces_params(params),
        "/api/updateWorkspaceSessionGroups" => gpui_remote_sidebar_workspace_groups_params(params),
        "/api/switchSessionAgent" => gpui_remote_sidebar_switch_session_agent_params(params),
        "/api/saveSessionAgentNote" => gpui_remote_sidebar_session_note_params(params),
        "/api/agentAccounts" => gpui_remote_sidebar_agent_accounts_params(params),
        "/api/closeProjectToRecent"
        | "/api/restoreRecentProject"
        | "/api/removeRecentProject"
        | "/api/removeProject"
        | "/api/listProjectWorktrees"
        | "/api/mergeWorktreeIntoMain" => gpui_remote_sidebar_project_id_params(params),
        "/api/scheduleDelayedSend" => gpui_remote_sidebar_delayed_send_params(params, false),
        "/api/cancelDelayedSend" => gpui_remote_sidebar_delayed_send_params(params, true),
        "/api/postponeDelayedSend" => {
            let delay_ms = params.get("delayMs")?.as_u64()?;
            gpui_command_delayed_send_duration_from_millis(delay_ms)?;
            let mut shaped = gpui_remote_sidebar_delayed_send_params(params, true)?;
            shaped["delayMs"] = serde_json::json!(delay_ms);
            Some(shaped)
        }
        "/api/startSessionProvider" => gpui_remote_sidebar_session_lifecycle_params(params, None),
        "/api/sendSessionMessage" => gpui_remote_sidebar_send_session_message_params(params),
        "/api/queueSessionChatPrompt" => {
            let mut shaped = gpui_remote_sidebar_send_session_message_params(params)?;
            shaped.as_object_mut()?.remove("submit");
            shaped["startupSend"] = serde_json::json!(true);
            Some(shaped)
        }
        "/api/settleSession"
        | "/api/unsettleSession"
        | "/api/unsnoozeSession"
        | "/api/exportSessionTranscript" => {
            gpui_remote_sidebar_session_lifecycle_params(params, None)
        }
        "/api/snoozeSession" => {
            gpui_remote_sidebar_session_lifecycle_params(params, Some("snoozedUntil"))
        }
        "/api/requestSessionRename" => gpui_remote_sidebar_request_session_rename_params(params),
        "/api/createProjectWorktree" => gpui_remote_sidebar_create_project_worktree_params(params),
        "/api/openProjectWorktree" => gpui_remote_sidebar_open_project_worktree_params(params),
        "/api/createWorktreeSession" => gpui_remote_sidebar_create_worktree_session_params(params),
        "/api/removeSessionWorktree" => gpui_remote_sidebar_remove_session_worktree_params(params),
        "/api/checkoutProjectNewBranch" => {
            gpui_remote_sidebar_checkout_project_new_branch_params(params)
        }
        _ => Some(params),
    }
}

pub(crate) fn gpui_remote_sidebar_agent_hook_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let agent_ids = params
        .as_object()?
        .get("agentIds")?
        .as_array()?
        .iter()
        .map(serde_json::Value::as_str)
        .collect::<Option<Vec<_>>>()?;
    if agent_ids.is_empty()
        || agent_ids.len() > 16
        || agent_ids
            .iter()
            .any(|agent_id| !gpui_remote_sidebar_agent_id_allowed(agent_id))
    {
        return None;
    }
    Some(serde_json::json!({ "agentIds": agent_ids }))
}

pub(crate) fn gpui_remote_sidebar_delayed_send_params(
    params: serde_json::Value,
    cancel: bool,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let session_id = object
        .get("sessionId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_session_id_allowed(value))?;
    let mut shaped = serde_json::Map::new();
    shaped.insert("projectId".to_string(), serde_json::json!(project_id));
    shaped.insert("sessionId".to_string(), serde_json::json!(session_id));
    if cancel {
        return Some(serde_json::Value::Object(shaped));
    }
    let delay_ms = object.get("delayMs").and_then(serde_json::Value::as_u64);
    let send_when_agent_stops = object
        .get("sendWhenAgentStops")
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    let send_when_all_project_sessions_stop = object
        .get("sendWhenAllProjectSessionsStop")
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    let watched = object
        .get("sendWhenSpecificAgentFinishes")
        .filter(|value| !value.is_null());
    if usize::from(delay_ms.is_some())
        + usize::from(send_when_agent_stops)
        + usize::from(send_when_all_project_sessions_stop)
        + usize::from(watched.is_some())
        != 1
    {
        return None;
    }
    if let Some(delay_ms) = delay_ms {
        gpui_command_delayed_send_duration_from_millis(delay_ms)?;
        shaped.insert("delayMs".to_string(), serde_json::json!(delay_ms));
    } else if let Some(watched) = watched {
        let project_id = watched.get("projectId")?.as_str()?;
        let session_id = watched.get("sessionId")?.as_str()?;
        if !gpui_remote_sidebar_project_id_allowed(project_id)
            || !gpui_remote_sidebar_session_id_allowed(session_id)
        {
            return None;
        }
        shaped.insert(
            "sendWhenSpecificAgentFinishes".to_string(),
            serde_json::json!({
                "projectId": project_id,
                "sessionId": session_id,
            }),
        );
    } else if send_when_agent_stops {
        shaped.insert(
            "sendWhenAgentStops".to_string(),
            serde_json::Value::Bool(true),
        );
    } else {
        shaped.insert(
            "sendWhenAllProjectSessionsStop".to_string(),
            serde_json::Value::Bool(true),
        );
    }
    Some(serde_json::Value::Object(shaped))
}

pub(crate) fn gpui_remote_sidebar_project_id_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    Some(serde_json::json!({ "projectId": project_id }))
}

pub(crate) fn gpui_remote_sidebar_project_collections_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let state = gpui_remote_sidebar_project_collections_state(params.get("state")?)?;
    Some(serde_json::json!({ "state": state }))
}

pub(crate) fn gpui_remote_sidebar_project_collections_state(
    value: &serde_json::Value,
) -> Option<serde_json::Value> {
    const MAX_COLLECTIONS: usize = 256;
    const MAX_PROJECT_IDS_PER_COLLECTION: usize = 512;
    const MAX_ID_CHARS: usize = 256;
    const MAX_TITLE_CHARS: usize = 256;
    const MAX_NEXT_COLLECTION_NUMBER: u64 = 1_000_000;

    let source = value.as_object()?;
    let source_collections = source.get("collections")?.as_object()?;
    let source_order = source.get("order")?.as_array()?;
    let next_collection_number = source.get("nextCollectionNumber")?.as_u64()?;
    if source_collections.len() > MAX_COLLECTIONS
        || source_order.len() != source_collections.len()
        || !(1..=MAX_NEXT_COLLECTION_NUMBER).contains(&next_collection_number)
    {
        return None;
    }

    // CDXC:RemoteMachines 2026-09-25 WHY:
    // A collection carries no `collapsed` flag any more: gxserver's normalizer drops it and the
    // store's document (gx-core `CollectionsDocument::to_wire_json`) never writes it, since the
    // collapsed state lives in the sidebar's own client state. Requiring it here refused every
    // remote Project Group edit before it was sent.
    fn bounded_text(candidate: &str, max_chars: usize) -> Option<&str> {
        let trimmed = candidate.trim();
        (!trimmed.is_empty()
            && trimmed.chars().count() <= max_chars
            && !trimmed.contains('\0')
            && !trimmed.chars().any(char::is_control))
        .then_some(trimmed)
    }
    let valid_color = |candidate: &str| {
        candidate == "transparent"
            || (candidate.len() == 7
                && candidate.starts_with('#')
                && candidate[1..].bytes().all(|byte| byte.is_ascii_hexdigit()))
    };

    let mut collections = serde_json::Map::new();
    for (collection_id, candidate) in source_collections {
        let normalized_collection_id = bounded_text(collection_id, MAX_ID_CHARS)?;
        let candidate = candidate.as_object()?;
        let embedded_collection_id = candidate.get("collectionId")?.as_str()?;
        if embedded_collection_id != normalized_collection_id {
            return None;
        }
        let title = bounded_text(candidate.get("title")?.as_str()?, MAX_TITLE_CHARS)?;
        let color = candidate.get("color")?.as_str()?;
        let source_project_ids = candidate.get("projectIds")?.as_array()?;
        if !valid_color(color)
            || source_project_ids.is_empty()
            || source_project_ids.len() > MAX_PROJECT_IDS_PER_COLLECTION
        {
            return None;
        }
        let mut project_ids = Vec::with_capacity(source_project_ids.len());
        for project_id in source_project_ids {
            let project_id = project_id.as_str()?.trim();
            if !gpui_remote_sidebar_project_id_allowed(project_id) {
                return None;
            }
            project_ids.push(serde_json::Value::String(project_id.to_string()));
        }
        collections.insert(
            normalized_collection_id.to_string(),
            serde_json::json!({
                "collectionId": normalized_collection_id,
                "color": color,
                "projectIds": project_ids,
                "title": title,
            }),
        );
    }

    let mut order = Vec::with_capacity(source_order.len());
    let mut seen_order_ids = std::collections::HashSet::new();
    for collection_id in source_order {
        let collection_id = bounded_text(collection_id.as_str()?, MAX_ID_CHARS)?;
        if !collections.contains_key(collection_id)
            || !seen_order_ids.insert(collection_id.to_string())
        {
            return None;
        }
        order.push(serde_json::Value::String(collection_id.to_string()));
    }

    Some(serde_json::json!({
        "collections": collections,
        "nextCollectionNumber": next_collection_number,
        "order": order,
    }))
}

/*
CDXC:StateSync 2026-07-29:
Settle/snooze params reduced to their id scope at the Rust boundary. The only
extra field any of them may carry is `snoozedUntil`, and it is accepted only as
a bounded RFC3339-shaped ASCII timestamp — the remote daemon validates that it
is strictly in the future, but CEF must not be able to tunnel arbitrary text
through this endpoint on the way there.
*/
pub(crate) fn gpui_remote_sidebar_session_lifecycle_params(
    params: serde_json::Value,
    extra_timestamp_key: Option<&str>,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let session_id = object
        .get("sessionId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_session_id_allowed(value))?;
    let mut shaped = serde_json::Map::new();
    shaped.insert(
        "projectId".to_string(),
        serde_json::Value::String(project_id.to_string()),
    );
    shaped.insert(
        "sessionId".to_string(),
        serde_json::Value::String(session_id.to_string()),
    );
    if let Some(key) = extra_timestamp_key {
        let timestamp = object
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| gpui_remote_sidebar_iso_timestamp_allowed(value))?;
        shaped.insert(
            key.to_string(),
            serde_json::Value::String(timestamp.to_string()),
        );
    }
    Some(serde_json::Value::Object(shaped))
}

/*
CDXC:RemoteMachines 2026-08-18:
A remote agent prompt is user-authored message text, not a command: gxserver
types it into the session and submits it. Shape it to the two ids plus the
bounded body and pin `submit` here so this route can never become a way for CEF
to write unsubmitted terminal input or pass daemon-only send flags.
*/
pub(crate) fn gpui_remote_sidebar_send_session_message_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    const MAX_MESSAGE_BYTES: usize = 32 * 1024;
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let session_id = object
        .get("sessionId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_session_id_allowed(value))?;
    let text = object
        .get("text")
        .and_then(serde_json::Value::as_str)
        .filter(|value| {
            !value.is_empty() && value.len() <= MAX_MESSAGE_BYTES && !value.contains('\0')
        })?;
    let mut shaped = serde_json::json!({
        "projectId": project_id,
        "sessionId": session_id,
        "submit": true,
        "text": text,
    });
    // The send's id (gxserver session_chat_send_requests.rs) rides along when the page named it.
    if let Some(id) = object
        .get("sendRequestId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty() && id.len() <= 128 && id.chars().all(|c| c.is_ascii_graphic()))
    {
        shaped["sendRequestId"] = serde_json::json!(id);
    }
    Some(shaped)
}

pub(crate) fn gpui_remote_sidebar_request_session_rename_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    /*
    CDXC:RemoteMachines 2026-08-12:
    A remote rename may carry only the target ids, bounded normalized title,
    and optional agent id into the selected machine's gxserver. The native
    bridge fixes the request reason/source and opts into daemon-owned command
    submission so CEF cannot turn this route into arbitrary remote terminal
    input.
    */
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let session_id = object
        .get("sessionId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_session_id_allowed(value))?;
    let title = gxserver_workspace_terminal_rename_title_field(object, "title").ok()?;
    let mut shaped = serde_json::Map::new();
    shaped.insert("projectId".to_string(), serde_json::json!(project_id));
    shaped.insert("reason".to_string(), serde_json::json!("gpui-sidebar"));
    shaped.insert("sessionId".to_string(), serde_json::json!(session_id));
    shaped.insert(
        "submitAgentRenameCommand".to_string(),
        serde_json::Value::Bool(true),
    );
    shaped.insert("title".to_string(), serde_json::Value::String(title));
    shaped.insert("titleSource".to_string(), serde_json::json!("user"));
    if let Some(agent_name) = object.get("agentName") {
        let agent_name = agent_name
            .as_str()
            .map(str::trim)
            .filter(|value| gpui_remote_sidebar_agent_id_allowed(value))?;
        shaped.insert(
            "agentName".to_string(),
            serde_json::Value::String(agent_name.to_string()),
        );
    }
    Some(serde_json::Value::Object(shaped))
}

pub(crate) fn gpui_remote_sidebar_update_project_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let git_config = object
        .get("gitConfig")
        .and_then(serde_json::Value::as_object)
        .and_then(gpui_remote_sidebar_git_preferences_update_payload)?;
    Some(serde_json::json!({
        "gitConfig": git_config,
        "projectId": project_id,
    }))
}

pub(crate) fn gpui_remote_sidebar_create_project_worktree_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let base_ref = object
        .get("baseRef")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_git_ref_allowed(value))?;
    let name_hint = object
        .get("nameHint")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_slug_label_allowed(value))?;
    Some(serde_json::json!({
        "baseRef": base_ref,
        "nameHint": name_hint,
        "projectId": project_id,
    }))
}

pub(crate) fn gpui_remote_sidebar_open_project_worktree_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let worktree_key = object
        .get("worktreeKey")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_worktree_key_allowed(value))?;
    Some(serde_json::json!({
        "projectId": project_id,
        "worktreeKey": worktree_key,
    }))
}

/*
CDXC:StateSync 2026-07-29:
Sidebar V2 worktree-create params, reduced to the P4 wire contract at the Rust
boundary. Every optional field is dropped unless it passes its own shape check,
so a malformed value can never be forwarded verbatim: the remote daemon then
sees a well-formed request missing that field rather than renderer-supplied text
it has to defend against. Only `projectId` is mandatory — everything else has a
server-side default (last agent, project default branch, no prompt).

`existingWorktree.path` is the one path this bridge accepts from the renderer,
and it is accepted only because the flow demands it: the client learned the path
from THAT machine's own presentation (`session.cwd`). It is still bounded and
absolute-only here, and the daemon re-applies its own path-safety normalization
before touching the filesystem.
*/
pub(crate) fn gpui_remote_sidebar_create_worktree_session_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let mut shaped = serde_json::Map::new();
    shaped.insert(
        "projectId".to_string(),
        serde_json::Value::String(project_id.to_string()),
    );
    if let Some(agent_id) = object
        .get("agentId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_agent_id_allowed(value))
    {
        shaped.insert(
            "agentId".to_string(),
            serde_json::Value::String(agent_id.to_string()),
        );
    }
    if let Some(base_branch) = object
        .get("baseBranch")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_git_ref_allowed(value))
    {
        shaped.insert(
            "baseBranch".to_string(),
            serde_json::Value::String(base_branch.to_string()),
        );
    }
    if let Some(first_prompt) = object
        .get("firstPrompt")
        .and_then(serde_json::Value::as_str)
        .filter(|value| gpui_remote_sidebar_first_prompt_allowed(value))
    {
        shaped.insert(
            "firstPrompt".to_string(),
            serde_json::Value::String(first_prompt.to_string()),
        );
    }
    if object
        .get("startFromOrigin")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        shaped.insert("startFromOrigin".to_string(), serde_json::Value::Bool(true));
    }
    if let Some(path) = object
        .get("existingWorktree")
        .and_then(serde_json::Value::as_object)
        .and_then(|worktree| worktree.get("path"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_worktree_path_allowed(value))
    {
        shaped.insert(
            "existingWorktree".to_string(),
            serde_json::json!({ "path": path }),
        );
    }
    Some(serde_json::Value::Object(shaped))
}

pub(crate) fn gpui_remote_sidebar_remove_session_worktree_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let worktree_path = object
        .get("worktreePath")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_worktree_path_allowed(value))?;
    let mut shaped = serde_json::Map::new();
    shaped.insert(
        "projectId".to_string(),
        serde_json::Value::String(project_id.to_string()),
    );
    shaped.insert(
        "worktreePath".to_string(),
        serde_json::Value::String(worktree_path.to_string()),
    );
    /*
    `force` is a DESTRUCTIVE escalation (delete a dirty checkout), so it is
    forwarded only as an explicit `true`. Any other value simply omits the key
    and the daemon re-applies its dirty refusal.
    */
    if object.get("force").and_then(serde_json::Value::as_bool) == Some(true) {
        shaped.insert("force".to_string(), serde_json::Value::Bool(true));
    }
    Some(serde_json::Value::Object(shaped))
}

pub(crate) fn gpui_remote_sidebar_checkout_project_new_branch_params(
    params: serde_json::Value,
) -> Option<serde_json::Value> {
    let object = params.as_object()?;
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_project_id_allowed(value))?;
    let branch_label = object
        .get("branchLabel")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_bounded_text_label_allowed(value))?;
    Some(serde_json::json!({
        "branchLabel": branch_label,
        "projectId": project_id,
    }))
}
