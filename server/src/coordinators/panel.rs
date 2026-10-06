//! The `coordinatorThreads` chat field: a coordinator's threads as its chat's Threads panel reads
//! them.
//!
//! CDXC:Coordinators 2026-09-30 WHY:
//! Claude puts an Overview of the threads beside the conversation; Ghostex puts a Threads panel above the coordinator's composer, next to the Subagents and Tasks panels, so it also reaches the web build and the phone (which has no sidebar tree). The supervisor refreshes this cache every tick and republishes the coordinator's chat state when it changed; the frame builders only read the cache, because they run under the stream's emit lock and must not touch the database. Absent on a frame means unchanged (like `appCommands`), so a builder that leaves it out can never blank the panel.
//! SEE-ALSO: server/src/server/coordinator_runtime.rs (refresh and republish), server/src/session_chat_follower/frames.rs and session_chat_read.rs (carry it), packages/gx-chat-core/src/extras/coordinator_threads.rs (the panel).
//!
//! CDXC:Coordinators 2026-10-06 WHY:
//! Every thread is sent, resolved ones included, with no cap: the panel's "N more" row must reach each thread the coordinator has so the user can still open and talk to any of them (the user: "Don't actually 'hide' them please"). The core orders and folds them by `activeAt`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::brief::report_headline;
use super::endpoint::session_title_of;
use super::records::{list_coordinators, list_threads, SessionKey};
use super::state::{
    classify_thread_session, thread_needs_user_approval, thread_prompt, ThreadProgress, ThreadState,
};
use crate::domain::DomainRepository;
use crate::presentation::now_iso;

fn panels() -> &'static Mutex<HashMap<SessionKey, Value>> {
    static PANELS: OnceLock<Mutex<HashMap<SessionKey, Value>>> = OnceLock::new();
    PANELS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Adds `coordinatorThreads` to a chat frame or read result of a coordinator session.
pub fn insert_coordinator_threads(
    frame: &mut Map<String, Value>,
    project_id: &str,
    session_id: &str,
) {
    let value = panels().lock().ok().and_then(|panels| {
        panels
            .get(&(project_id.to_string(), session_id.to_string()))
            .cloned()
    });
    if let Some(value) = value {
        frame.insert("coordinatorThreads".to_string(), value);
    }
}

/// Rebuilds every coordinator's panel and returns the coordinators whose panel changed.
pub fn refresh_coordinator_panels(
    db: &Connection,
    repository: &DomainRepository<'_>,
) -> Vec<SessionKey> {
    let Ok(coordinators) = list_coordinators(db) else {
        return Vec::new();
    };
    let threads = list_threads(db).unwrap_or_default();
    let generated_at = now_iso();
    let mut next: HashMap<SessionKey, Value> = HashMap::new();
    for coordinator in coordinators {
        let key = coordinator.key();
        if repository
            .get_session(&key.0, &key.1)
            .ok()
            .flatten()
            .is_none()
        {
            continue;
        }
        let mut rows = Vec::new();
        for thread in threads
            .iter()
            .filter(|thread| thread.coordinator_key() == key)
        {
            let session = repository
                .get_session(&thread.project_id, &thread.session_id)
                .ok()
                .flatten();
            let state = classify_thread_session(
                session.as_ref(),
                ThreadProgress::of(thread),
                &generated_at,
                false,
            );
            let detail = match state {
                ThreadState::Waiting => session
                    .as_ref()
                    .and_then(thread_prompt)
                    .map(|prompt| report_headline(&prompt.summary, 140))
                    .unwrap_or_else(|| "Waiting for an answer".to_string()),
                ThreadState::Working => report_headline(&thread.task, 140),
                _ => thread
                    .last_report
                    .as_deref()
                    .map(|report| report_headline(report, 140))
                    .unwrap_or_default(),
            };
            // The session's own activity clock, as the sidebar's relative time reads it; a thread whose
            // session is gone falls back to its record's last change.
            let active_at = session
                .as_ref()
                .and_then(|session| session.get("lastActiveAt"))
                .and_then(Value::as_str)
                .unwrap_or(thread.updated_at.as_str())
                .to_string();
            let mut row = json!({
                "projectId": thread.project_id,
                "sessionId": thread.session_id,
                "title": session.as_ref().map(session_title_of).unwrap_or_else(|| report_headline(&thread.task, 60)),
                "state": state.as_str(),
                "detail": detail,
                "activeAt": active_at,
            });
            if state == ThreadState::Waiting
                && session.as_ref().is_some_and(thread_needs_user_approval)
            {
                row["needsApproval"] = json!(true);
            }
            if let Some(session) = session.as_ref() {
                if let Some(lifecycle) = session.get("lifecycleState") {
                    row["lifecycleState"] = lifecycle.clone();
                }
                if let Some(marker) =
                    crate::worktree_sessions::read_worktree_session_marker(session)
                {
                    row["branch"] = json!(marker.branch);
                }
            }
            rows.push(row);
        }
        next.insert(key, json!({ "threads": rows }));
    }
    let Ok(mut panels) = panels().lock() else {
        return Vec::new();
    };
    let mut changed = next
        .iter()
        .filter(|(key, value)| panels.get(*key) != Some(*value))
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    // A coordinator that went away keeps its last panel on any open chat; nothing to republish.
    changed.sort();
    *panels = next;
    changed
}
