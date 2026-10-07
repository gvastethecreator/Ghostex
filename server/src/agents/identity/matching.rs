use serde_json::{Map, Value};

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum IdentityMatchStrength {
    /// Both sides carry the same agent session id: provably one conversation.
    ConversationId,
    /// Only the session store path matched. For per-conversation transcript
    /// files (Claude, Codex) that still means the same conversation; for
    /// shared-store agents it means nothing.
    StorePath,
}

/// CDXC:SessionTitles 2026-09-17 DECISION:
/// User: for zcode, only accept a trusted-title sibling match when the conversation id matches too, and save donated titles with a non-user source.
/// zcode keeps every conversation in ONE SQLite database, so agentSessionPath is identical for all zcode sessions; the old path-only fallback let a fresh unbound pane adopt a sibling's user-pinned title ("111") and record it as user, which permanently outranked zcode's own titles.
/// Hermes stores its conversations in one state.db too, but the identity path carries its per-conversation mirror file (`hermes-chat-mirror/<id>.jsonl`), so its path fallback stays safe; add an agent here if its identity path ever becomes a shared multi-conversation store.
fn identity_uses_shared_session_store(agent: Option<&str>) -> bool {
    normalize_agent_id(agent).as_deref() == Some("zcode")
}

pub(crate) fn identities_match_strength(
    left: &ResolvedIdentity,
    right: &ResolvedIdentity,
) -> Option<IdentityMatchStrength> {
    let left_agent = normalize_agent_id(left.agent_id.as_deref());
    let right_agent = normalize_agent_id(right.agent_id.as_deref());
    if left_agent.is_some() && right_agent.is_some() && left_agent != right_agent {
        return None;
    }
    if left.agent_session_id.is_some()
        && right.agent_session_id.is_some()
        && left.agent_session_id == right.agent_session_id
    {
        return Some(IdentityMatchStrength::ConversationId);
    }
    let shared_store = identity_uses_shared_session_store(left_agent.as_deref())
        || identity_uses_shared_session_store(right_agent.as_deref());
    if shared_store {
        return None;
    }
    (left.agent_session_path.is_some()
        && right.agent_session_path.is_some()
        && left.agent_session_path == right.agent_session_path)
        .then_some(IdentityMatchStrength::StorePath)
}

pub(crate) fn normalize_codex_session_id(value: &str) -> Option<String> {
    is_uuid(value.trim()).then(|| value.trim().to_ascii_lowercase())
}

#[derive(Clone)]
pub(crate) struct IdentityInput {
    pub(crate) agent_id: Option<String>,
    pub(crate) agent_name: Option<String>,
    pub(crate) agent_session_id: Option<String>,
    pub(crate) agent_session_path: Option<String>,
    pub(crate) runtime_settings: Map<String, Value>,
    pub(crate) startup_text: Option<String>,
}

#[derive(Clone, Default)]
pub(crate) struct ResolvedIdentity {
    pub(crate) agent_id: Option<String>,
    pub(crate) agent_session_id: Option<String>,
    pub(crate) agent_session_path: Option<String>,
}

pub(crate) fn resolve_session_identity(input: &IdentityInput) -> ResolvedIdentity {
    let resume = parse_agent_resume_identity(input.startup_text.as_deref());
    let agent_session_path = input
        .agent_session_path
        .clone()
        .or_else(|| read_text_from_map(&input.runtime_settings, "agentSessionPath"));
    let agent_session_id = input
        .agent_session_id
        .clone()
        .or_else(|| read_text_from_map(&input.runtime_settings, "agentSessionId"))
        .or(resume.agent_session_id);
    let agent_id = normalize_agent_id(input.agent_id.as_deref())
        .or_else(|| normalize_agent_id(input.agent_name.as_deref()))
        .or_else(|| {
            normalize_agent_id(read_text_from_map(&input.runtime_settings, "agentName").as_deref())
        })
        .or_else(|| {
            normalize_agent_id(read_text_from_map(&input.runtime_settings, "agentId").as_deref())
        })
        .or_else(|| infer_agent_id_from_path(agent_session_path.as_deref()))
        .or(resume.agent_id);
    ResolvedIdentity {
        agent_id,
        agent_session_id,
        agent_session_path,
    }
}

pub(crate) fn parse_agent_resume_identity(text: Option<&str>) -> ResolvedIdentity {
    let text = text.unwrap_or_default();
    for (agent_id, needle) in [
        ("codex", "codex"),
        ("zcode", "zcode"),
        ("claude", "claude"),
        ("cursor", "cursor-agent"),
        ("opencode", "opencode"),
        ("pi", "pi"),
        ("kiro", "kiro-cli"),
        ("omp", "omp"),
        ("empryo", "empryo"),
    ] {
        let lower = text.to_ascii_lowercase();
        if !lower.contains(needle) {
            continue;
        }
        /*
        CDXC:SessionFork 2026-09-02:
        A fork launch names the PARENT conversation (`codex fork <id>`,
        `claude --resume <id> --fork-session`); the forked conversation's own id
        is only known once the agent's hook or transcript reports it. Seeding
        the parent id here made the fork row and its parent share one identity
        (see `extract_agent_process_session_id`), so a fork launch contributes
        the agent only.
        */
        if resume_reference_is_fork(text, needle) {
            return ResolvedIdentity {
                agent_id: Some(agent_id.to_string()),
                agent_session_id: None,
                agent_session_path: None,
            };
        }
        if let Some(reference) = quoted_or_next_resume_reference(text, needle) {
            return ResolvedIdentity {
                agent_id: Some(agent_id.to_string()),
                agent_session_id: Some(reference),
                agent_session_path: None,
            };
        }
    }
    ResolvedIdentity::default()
}

fn resume_reference_is_fork(text: &str, command: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let Some(index) = lower.find(command) else {
        return false;
    };
    text[index + command.len()..]
        .split_whitespace()
        .map(|token| token.trim_matches(['"', '\'', '\r', '\n', ';']))
        .any(|token| {
            matches!(token, "fork" | "--fork" | "--fork-session")
                || token.starts_with("--fork=")
                || token.starts_with("--fork-session=")
        })
}

pub(crate) fn quoted_or_next_resume_reference(text: &str, command: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let index = lower.find(command)?;
    let tail = &text[index + command.len()..];
    let tokens = tail
        .split_whitespace()
        .map(|token| token.trim_matches(['"', '\'', '\r', '\n', ';']))
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    for (index, token) in tokens.iter().enumerate() {
        if matches!(
            *token,
            "resume" | "fork" | "--resume" | "--session" | "-s" | "--resume-id"
        ) {
            return tokens.get(index + 1).map(|value| (*value).to_string());
        }
    }
    None
}
