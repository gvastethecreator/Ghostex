//! What a thread is doing, decided once in gxserver for the supervisor, the sidebar, the chat's
//! Threads panel and `ghostex coordinator status`.

use serde_json::Value;

use crate::agents::session_chat_prompt_setting;
use crate::presentation::{effective_lifecycle_state, presentation_activity};
use crate::session_chat_interactive::{
    parse_stored_session_chat_prompt, SessionChatInteractivePrompt,
};
use crate::session_status::TURN_COMPLETE_ATTENTION_SOURCE;

/// CDXC:Coordinators 2026-09-30 WHY:
/// Claude's Overview groups threads as Ready for review, Waiting on you, Working, Landing, Idle and Resolved. The first and fourth depend on pull requests, which a Ghostex thread does not always open, so a finished turn is one state (`finished`) and the report says whether there is a branch or pull request. A sleeping thread is still open work, not done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadState {
    /// A question, an approval, or another prompt that needs an answer is on its screen.
    Waiting,
    Working,
    /// Its turn ended and nobody resolved it yet.
    Finished,
    Sleeping,
    /// Its session was closed or no longer exists.
    Closed,
    /// Resolved by the user or the coordinator.
    Done,
}

impl ThreadState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Working => "working",
            Self::Finished => "finished",
            Self::Sleeping => "sleeping",
            Self::Closed => "closed",
            Self::Done => "done",
        }
    }

    /// The heading the CLI prints over a group of threads in this state.
    pub fn heading(self) -> &'static str {
        match self {
            Self::Waiting => "Waiting on you",
            Self::Working => "Working",
            Self::Finished => "Finished",
            Self::Sleeping => "Sleeping",
            Self::Closed => "Closed",
            Self::Done => "Done",
        }
    }

    /// Display order: what needs someone first.
    pub fn order(self) -> u8 {
        match self {
            Self::Waiting => 0,
            Self::Finished => 1,
            Self::Working => 2,
            Self::Sleeping => 3,
            Self::Closed => 4,
            Self::Done => 5,
        }
    }
}

/// The question or approval a thread is showing, as one line for a report, and a key that changes
/// whenever a different prompt appears.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadPrompt {
    pub key: String,
    pub summary: String,
}

/// What the thread record adds to the session's own state.
#[derive(Clone, Copy, Debug)]
pub struct ThreadProgress {
    pub resolved: bool,
    /// It has run a turn since it was started (seen working, or a turn was already reported).
    pub has_run: bool,
}

impl ThreadProgress {
    pub fn of(thread: &super::records::ThreadRecord) -> Self {
        Self {
            resolved: thread.is_resolved(),
            has_run: thread.observed_working || thread.reported_at.is_some(),
        }
    }
}

/// `transcript_working` is the supervisor's own reading of the transcript (mid-turn or not); the
/// projection passes false and relies on the hook state alone.
pub fn classify_thread_session(
    session: Option<&Value>,
    progress: ThreadProgress,
    now_iso: &str,
    transcript_working: bool,
) -> ThreadState {
    if progress.resolved {
        return ThreadState::Done;
    }
    let Some(session) = session else {
        return ThreadState::Closed;
    };
    match effective_lifecycle_state(session).as_str() {
        "running" => {}
        "sleeping" => return ThreadState::Sleeping,
        _ => return ThreadState::Closed,
    }
    let activity = presentation_activity(session, now_iso);
    if activity == "working" || transcript_working {
        return ThreadState::Working;
    }
    if thread_prompt(session).is_some() {
        return ThreadState::Waiting;
    }
    // A Stop hook's attention is a finished turn; any other attention (a permission, a bell, an
    // "Action Required" title) is the agent waiting for someone.
    if activity == "attention"
        && attention_source(session).as_deref() != Some(TURN_COMPLETE_ATTENTION_SOURCE)
    {
        return ThreadState::Waiting;
    }
    // A thread whose brief has not run yet is starting, not finished: its agent is still coming
    // up, or its first message is waiting for the input box.
    if !progress.has_run {
        return ThreadState::Working;
    }
    ThreadState::Finished
}

fn attention_source(session: &Value) -> Option<String> {
    session
        .pointer("/runtimeSettings/agentActivity/attentionSource")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// A waiting thread is blocked on something only the user can grant: an approval card, or attention
/// that is no question at all (a permission notification, a folder-trust dialog). A question in its
/// chat is the coordinator's to answer, so it does not count.
pub fn thread_needs_user_approval(session: &Value) -> bool {
    if let Some(prompt) =
        session_chat_prompt_setting(session).and_then(|stored| parse_stored_session_chat_prompt(&stored))
    {
        return matches!(prompt, SessionChatInteractivePrompt::Approval { .. });
    }
    crate::session_chat_async_questions::pending_question_count(session) == 0
}

/// The prompt a thread is showing: the stored question or approval card, or the async questions
/// Claude asks without blocking.
pub fn thread_prompt(session: &Value) -> Option<ThreadPrompt> {
    if let Some(stored) = session_chat_prompt_setting(session) {
        if let Some(prompt) = parse_stored_session_chat_prompt(&stored) {
            let summary = match &prompt {
                SessionChatInteractivePrompt::Question { questions, .. } => questions
                    .iter()
                    .map(|question| {
                        let options = question
                            .options
                            .iter()
                            .map(|option| option.label.trim())
                            .filter(|label| !label.is_empty())
                            .collect::<Vec<_>>();
                        if options.is_empty() {
                            question.question.trim().to_string()
                        } else {
                            format!(
                                "{} (options: {})",
                                question.question.trim(),
                                options.join(" / ")
                            )
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                SessionChatInteractivePrompt::Approval { tool, summary, .. } => match summary {
                    Some(summary) if !summary.trim().is_empty() => {
                        format!("Approval for {tool}: {}", summary.trim())
                    }
                    _ => format!("Approval for {tool}"),
                },
            };
            return Some(ThreadPrompt {
                key: stored_prompt_key(&stored),
                summary,
            });
        }
    }
    let pending = crate::session_chat_async_questions::pending_question_count(session);
    if pending > 0 {
        let ids = session
            .pointer("/runtimeSettings/sessionChatAsyncQuestionIds")
            .map(Value::to_string)
            .unwrap_or_default();
        return Some(ThreadPrompt {
            key: stored_prompt_key(&ids),
            summary: if pending == 1 {
                "It asked a question in its chat.".to_string()
            } else {
                format!("It asked {pending} questions in its chat.")
            },
        });
    }
    None
}

fn stored_prompt_key(stored: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(stored.as_bytes()))
        .chars()
        .take(16)
        .collect()
}

/// CDXC:Coordinators 2026-09-30 DECISION:
/// User (question 1, answer 1B): a coordinator's threads trust the worktrees of a project the user already trusted, so a worktree thread starts without stopping at the agent's folder-trust question. The project counts as trusted because its coordinator runs in it (the user created it there and answered that folder's own question). Only an open thread qualifies, and only in the project's own folder or a worktree Ghostex made for it; any other folder still asks.
/// SEE-ALSO: server/src/session_chat_trust_memory.rs (`session_folders_remembered` asks this), the "Trust and Remember" decision there.
pub fn coordinator_thread_folder_trusted(
    repository: &crate::domain::DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> bool {
    let Ok(Some(thread)) = super::records::read_thread(repository.db, project_id, session_id)
    else {
        return false;
    };
    if thread.is_resolved() {
        return false;
    }
    let coordinator_alive = repository
        .get_session(
            &thread.coordinator_project_id,
            &thread.coordinator_session_id,
        )
        .ok()
        .flatten()
        .is_some();
    if !coordinator_alive || thread.coordinator_project_id != project_id {
        return false;
    }
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return false;
    };
    let cwd = session.get("cwd").and_then(Value::as_str).map(str::trim);
    let project_path = repository
        .get_project(project_id)
        .ok()
        .flatten()
        .and_then(|project| {
            project
                .get("path")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    match cwd.filter(|cwd| !cwd.is_empty()) {
        None => true,
        Some(cwd) if project_path.as_deref().map(str::trim) == Some(cwd) => true,
        Some(cwd) => crate::worktree_sessions::read_worktree_session_marker(&session)
            .is_some_and(|marker| marker.path.trim() == cwd),
    }
}
