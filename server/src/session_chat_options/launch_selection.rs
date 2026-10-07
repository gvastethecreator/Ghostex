use super::*;

/// CDXC:AgentScreenDetection 2026-10-04 DECISION:
/// User: after the new-thread hotkey the composer must show its details as instantly as possible. What the session's agent command was started with (`--model`, `--effort`, Codex's `-m` and `model_reasoning_effort`, Claude's `--permission-mode` and `--dangerously-skip-permissions`) is known the moment the session exists, so it fills the pills before the agent has painted. It is the weakest evidence and holds only until the agent reports through its transcript or statusline; anything the command does not name stays loading, never guessed from settings files (Claude's own settings disagreed with the effort it actually ran).
/// SEE-ALSO: `merge_session_chat_option_selections`, `apply_detected_choice` in packages/gx-chat-core/src/menus/option_values.rs (launch evidence never replaces a pick made in the chat).
pub(crate) fn read_session_chat_launch_selection(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    agent: Option<SessionChatOptionAgent>,
) -> Option<SessionChatDetectedSelection> {
    let agent = agent?;
    let session = repository.get_session(project_id, session_id).ok()??;
    let command = session
        .pointer("/launchSettings/agentLaunchPlan/command")
        .or_else(|| session.pointer("/runtimeSettings/agentCommand"))
        .and_then(Value::as_str)?;
    launch_command_selection(agent, command)
}

/// The options one agent command line names, read word by word so a quoted prompt or another
/// option's value that happens to look like a flag is never taken for one.
pub(crate) fn launch_command_selection(
    agent: SessionChatOptionAgent,
    command: &str,
) -> Option<SessionChatDetectedSelection> {
    let family = match agent {
        SessionChatOptionAgent::Claude => "claude",
        SessionChatOptionAgent::Codex => "codex",
        SessionChatOptionAgent::Pi => "pi",
        _ => return None,
    };
    let mut words = Vec::new();
    let mut offset = 0;
    while !command[offset..].trim().is_empty() {
        let (start, end, word) = crate::agents::command_word(command, offset)?;
        offset = end;
        words.push((command[start..end].to_string(), word));
    }
    let mut selection = SessionChatDetectedSelection::default();
    let mut index = 0;
    while index < words.len() {
        let (literal, word) = &words[index];
        index += 1;
        if !crate::agents::is_option_word(literal, word) || !word.starts_with('-') {
            continue;
        }
        let (flag, inline) = match word.split_once('=') {
            Some((flag, value)) => (flag, Some(value.to_string())),
            None => (word.as_str(), None),
        };
        let takes_value = inline.is_none() && crate::agents::option_takes_value(family, flag);
        let value = match inline {
            Some(value) => Some(value),
            None if takes_value => {
                let value = words.get(index).map(|(_, value)| value.clone());
                index += 1;
                value
            }
            None => None,
        };
        let value = value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        match (agent, flag, value) {
            (_, "--model", Some(model)) | (SessionChatOptionAgent::Codex, "-m", Some(model)) => {
                selection.model = launch_model_choice(family, model);
            }
            (SessionChatOptionAgent::Claude, "--effort", Some(effort))
            | (SessionChatOptionAgent::Pi, "--thinking", Some(effort)) => {
                selection.effort = launch_effort_choice(effort);
            }
            (SessionChatOptionAgent::Codex, "-c" | "--config", Some(setting)) => {
                if let Some(effort) = setting.strip_prefix("model_reasoning_effort=") {
                    selection.effort = launch_effort_choice(effort.trim_matches(['"', '\'']));
                }
            }
            (SessionChatOptionAgent::Claude, "--permission-mode", Some(mode)) => {
                selection.mode = launch_mode_choice(mode);
            }
            (SessionChatOptionAgent::Claude, "--dangerously-skip-permissions", None) => {
                selection.mode = launch_mode_choice("bypassPermissions");
            }
            _ => {}
        }
    }
    (selection.model.is_some() || selection.effort.is_some() || selection.mode.is_some())
        .then_some(selection)
}

fn launch_choice(value: &str, label: &str) -> SessionChatDetectedChoice {
    SessionChatDetectedChoice {
        value: value.to_string(),
        label: label.to_string(),
        source: SessionChatOptionEvidence::Launch,
    }
}

/// A catalog value keeps the catalog's own label; a Claude model id or alias outside it is named
/// the way a transcript record of the same id would be.
fn launch_model_choice(family: &str, model: &str) -> Option<SessionChatDetectedChoice> {
    if let Some(entry) = crate::agent_model_catalog::catalog_model(family, model) {
        let label = entry.get("label").and_then(Value::as_str).unwrap_or(model);
        return Some(launch_choice(model, label));
    }
    match family {
        "claude" => claude_transcript_model_choice(model)
            .map(|choice| launch_choice(&choice.value, &choice.label)),
        _ => Some(launch_choice(model, model)),
    }
}

fn launch_effort_choice(effort: &str) -> Option<SessionChatDetectedChoice> {
    let effort = effort.trim().to_ascii_lowercase();
    (!effort.is_empty() && effort.chars().all(|ch| ch.is_ascii_alphanumeric()))
        .then(|| launch_choice(&effort, &effort))
}

/// Claude's `--permission-mode` takes the same names as the transcript's `permissionMode`.
fn launch_mode_choice(mode: &str) -> Option<SessionChatDetectedChoice> {
    claude_transcript_mode_choice(mode).map(|choice| launch_choice(&choice.value, &choice.label))
}
