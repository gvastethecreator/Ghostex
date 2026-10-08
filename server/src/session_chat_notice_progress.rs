use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use serde_json::Value;

use crate::{domain::DomainRepository, session_chat_notice::SessionChatTerminalNotice};

struct ClearedNotice {
    instance: String,
    response_at: i64,
}

fn cleared() -> &'static Mutex<HashMap<String, ClearedNotice>> {
    static CLEARED: OnceLock<Mutex<HashMap<String, ClearedNotice>>> = OnceLock::new();
    CLEARED.get_or_init(|| Mutex::new(HashMap::new()))
}

fn key(project: &str, session: &str) -> String {
    crate::server::session_observer_key(project, session)
}

fn instance(notice: &SessionChatTerminalNotice) -> String {
    format!("{}\n{}", notice.identity(), notice.detected_at)
}

/// Whether a transcript error record is the agent reporting a usage limit: Claude's "You've hit your session limit", "You've reached your weekly limit" and "You're out of usage credits", Codex's "hit your usage limit".
fn transcript_usage_limit_message(message: &str) -> bool {
    let text = message.to_lowercase();
    text.contains("usage limit")
        || text.contains("out of usage credits")
        || ((text.contains("hit your") || text.contains("reached your")) && text.contains("limit"))
}

pub(crate) fn has_cleared_error(
    state: &crate::server::AppState,
    project: &str,
    session: &str,
) -> bool {
    state.session_chat_option_cache.lock().is_ok_and(|entries| {
        entries
            .get(&key(project, session))
            .and_then(|entry| entry.value.notice.as_ref())
            .is_some_and(|notice| !visible(project, session, notice))
    })
}

pub(crate) fn visible(project: &str, session: &str, notice: &SessionChatTerminalNotice) -> bool {
    cleared()
        .lock()
        .ok()
        .and_then(|entries| {
            entries
                .get(&key(project, session))
                .map(|entry| entry.instance.clone())
        })
        .as_deref()
        != Some(instance(notice).as_str())
}

/// CDXC:AgentScreenDetection 2026-09-08 DECISION:
/// User: when AI messages arrive in the transcript after an error, that error is old and must no longer be shown.
/// Keep the raw screen notice in the detection cache so repeated captures retain the cleared instance's identity, while UI and recovery consumers receive only the visible notice.
pub(crate) fn refresh(
    repository: &DomainRepository<'_>,
    project: &str,
    session: &str,
    agent: Option<&str>,
    notice: &SessionChatTerminalNotice,
) {
    if !crate::accounts::recovery::retryable(notice)
        || (notice.is_answerable()
            && notice.kind != crate::session_chat_notice::SESSION_CHAT_NOTICE_USAGE_LIMIT)
    {
        return;
    }
    let Some(transcript_agent) = crate::session_chat::resolve_session_chat_transcript_agent(agent)
    else {
        return;
    };
    let Ok(Some(row)) = repository.get_session(project, session) else {
        return;
    };
    let Some(path) = crate::session_chat::resolve_session_chat_transcript_path(
        transcript_agent,
        row.pointer("/runtimeSettings/agentSessionId")
            .and_then(Value::as_str),
        row.pointer("/runtimeSettings/agentSessionPath")
            .and_then(Value::as_str),
    ) else {
        return;
    };
    let Ok(text) = crate::session_chat_options::transcript_tail_text(&path) else {
        return;
    };
    let Some(observed_at) = chrono::DateTime::parse_from_rfc3339(&notice.detected_at)
        .ok()
        .map(|t| t.timestamp_millis())
    else {
        return;
    };
    let mut response_at = None;
    let mut error_at = None;
    let mut usage_limit_error_at = None;
    for line in text.lines() {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if record["isSidechain"] == true {
            continue;
        }
        let Some(timestamp) = crate::session_chat::parse_timestamp(record.get("timestamp")) else {
            continue;
        };
        let payload = &record["payload"];
        let error_message = if record["type"] == "event_msg"
            && matches!(payload["type"].as_str(), Some("error" | "stream_error"))
        {
            payload["message"].as_str().map(str::to_owned)
        } else if record["type"] == "event_msg" && payload["type"] == "task_complete" {
            // CDXC:AgentProviders 2026-09-17 WHY:
            // Codex also records usage limits in task_complete.error, without a separate error event. Missing that record leaves a fresh limit hidden by the previous account switch or mistaken for an error the agent already recovered from.
            payload["error"]["message"].as_str().map(str::to_owned)
        } else if record["isApiErrorMessage"] == true {
            record["message"]["content"].as_array().map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|block| block["text"].as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        } else {
            None
        };
        if let Some(message) = error_message {
            if notice
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains(message.trim()))
                && !message.trim().is_empty()
            {
                error_at = Some(timestamp);
            }
            if transcript_usage_limit_message(&message) {
                usage_limit_error_at = Some(timestamp);
            }
        }
        let codex_response = (record["type"] == "response_item"
            && payload["type"] == "message"
            && payload["role"] == "assistant")
            || (record["type"] == "event_msg" && payload["type"] == "agent_message")
            || (record["type"] == "event_msg"
                && payload["type"] == "item_completed"
                && payload["item"]["type"] == "AgentMessage");
        let claude_response = record["type"] == "assistant"
            && record["isApiErrorMessage"] != true
            && record["message"]["model"]
                .as_str()
                .is_some_and(|model| model != "<synthetic>");
        if codex_response || claude_response {
            response_at = Some(timestamp);
        }
    }
    // CDXC:AgentProviders 2026-09-11 WHY:
    // A limit the transcript records after the account switch is the new login running out, whatever the screen says: the resumed CLI repaints the old limit with other glyphs and wording, so the screen text cannot tell old from new. Only a transcript error written after the switch time lifts the suppression, in memory and in the row, so the notice can show and the switch pass can act on it.
    if notice.kind == crate::session_chat_notice::SESSION_CHAT_NOTICE_USAGE_LIMIT {
        if let Some(since) =
            crate::session_chat_notice::account_usage_notice_suppression(project, session)
        {
            if usage_limit_error_at.is_some_and(|error| error > since.timestamp_millis()) {
                crate::session_chat_notice::lift_account_usage_notice_suppression(project, session);
                let mut runtime = row["runtimeSettings"]
                    .as_object()
                    .cloned()
                    .unwrap_or_default();
                runtime.remove("accountSuppressedUsageNotice");
                runtime.remove("accountSuppressedUsageNoticeAt");
                let _ = crate::accounts::endpoint::update_session(repository, &row, runtime);
            }
        }
    }
    // The limit chooser's body contains options, not the transcript's error
    // text. Its quota error still anchors progress and lifts switch suppression.
    if notice.kind == crate::session_chat_notice::SESSION_CHAT_NOTICE_USAGE_LIMIT {
        error_at = usage_limit_error_at.or(error_at);
    }
    let recovered = response_at.filter(|response| *response > error_at.unwrap_or(observed_at));
    if let Ok(mut entries) = cleared().lock() {
        if let Some(response_at) = recovered {
            entries.insert(
                key(project, session),
                ClearedNotice {
                    instance: instance(notice),
                    response_at,
                },
            );
        } else if error_at.is_some_and(|error| {
            entries
                .get(&key(project, session))
                .is_some_and(|entry| error > entry.response_at)
        }) {
            // CDXC:AgentProviders 2026-09-22 WHY:
            // The screen can report a new limit before its transcript record arrives. Comparing the later record with screen-detection time kept that limit hidden forever; compare it with the response that actually cleared the old error instead.
            entries.remove(&key(project, session));
        }
    }
}
