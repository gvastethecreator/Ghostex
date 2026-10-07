//! A coordinator's Threads panel, above the composer: one list of its threads, the working and the
//! recently active ones, with the rest a click away.
//!
//! CDXC:Coordinators 2026-09-30 WHY:
//! The panel shows a coordinator's threads without opening the sidebar, which the phone does not have. Order, labels and the fold are decided here once for the desktop, web and phone renderers; gxserver only sends each thread's state, activity time and one line of detail (`coordinatorThreads`).
//!
//! CDXC:Coordinators 2026-10-07 DECISION:
//! The user: "When I open the chat on mobile or on desktop, we're always expanding the threads in the coordinator sessions. Let's not do this. Also, please let's change it so that we don't show the top three done unless they are in the last 2 hours. By default, we should only show working. On the phone, showing working plus three finished ones is a bit much; the vertical space is not that much." The option they picked: the panel stays open as before, but lists only the working threads and those active in the last 2 hours (no fixed "3 most recent"), then "N more" for every other thread, closed ones included, and "Show fewer" folds them back; the header stays "2 working · 96 threads". The panel's own fold is remembered (`threadsCollapsed`), so a chat opens the way the user last left it rather than expanded every time. Still standing from the 2026-10-06 decisions it supersedes: no Finished / Sleeping / Done groups ("done and finished mean the same thing"), no "Needs you" group, every thread reachable ("Don't actually 'hide' them please"), and the rare thread blocked on something only the user can grant (a permission or folder-trust prompt) listed right after the working ones with an amber "Needs your approval" tag. The sidebar applies the same two hours (packages/gx-core/src/sidebar_view/threads.rs).
//!
//! CDXC:Coordinators 2026-10-07 WHY:
//! gxserver's frames carry only a summary (the rows listed by default, each `recent` one marked by gxserver, the total and a revision), because the whole list was ~30KB on every state frame. "N more" reads the full list once (`readCoordinatorThreads`), keeps it while the list is open, reads it again when a frame brings a new revision, and drops it when the list folds. Until the read answers, the summary rows stay listed.
//! SEE-ALSO: server/src/coordinators/panel.rs (the field, the summary and the read), apps/desktop/src/app/native_chat/coordinator_threads.rs and apps/mobile/app/src/chat/native/cards/AgentPanels.tsx (the renderers), packages/gx-core/src/sidebar_view/threads.rs (the sidebar's two-hour rule).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::effect::Effect;
use crate::event::StorageKey;
use crate::state::PanelsState;
use crate::wire::ChatRpcMethod;

/// One thread row.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordinatorThreadRow {
    /// `<projectId>:<sessionId>`, stable across frames.
    pub key: String,
    pub project_id: String,
    pub session_id: String,
    pub title: String,
    /// One line: its task while working, the prompt it is blocked on, or its last report.
    pub detail: String,
    pub working: bool,
    /// Blocked on something only the user can grant; drawn with the amber tag.
    pub needs_approval: bool,
    /// Active in the last two hours, as gxserver judged it; listed without "N more".
    #[serde(skip)]
    pub recent: bool,
    /// The worktree branch, when it has one.
    pub branch: String,
    pub lifecycle_state: String,
}

/// The panel.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordinatorThreadsPanel {
    /// "1 working · 94 threads".
    pub meta: String,
    pub rows: Vec<CoordinatorThreadRow>,
    /// A thread needs the user's approval, which marks the header.
    pub attention: bool,
    pub collapsed: bool,
    pub show_all: bool,
    /// "91 more" / "Show fewer", or "" when every thread is already listed.
    pub more_label: String,
}

/// The tag a row blocked on the user carries.
pub const NEEDS_APPROVAL_LABEL: &str = "Needs your approval";

/// The store and key the panel's fold is remembered under: a preference like the task panel's,
/// not per-session view state.
pub fn threads_collapsed_key() -> StorageKey {
    StorageKey {
        store: "threadsCollapsed".to_string(),
        suffix: String::new(),
    }
}

/// `'1'` when folded, a delete when not.
pub fn write_threads_collapsed(collapsed: bool) -> Effect {
    Effect::WriteStorage {
        key: threads_collapsed_key(),
        value: collapsed.then(|| "1".to_string()),
        durable: false,
    }
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// `true` when the session is a coordinator, whatever its thread count.
pub fn is_coordinator(threads: Option<&Value>) -> bool {
    threads.is_some_and(Value::is_object)
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// The read "N more" needs now: the list is open and what it holds (if anything) is not the
/// revision the latest frame names. A gxserver that sends no revision sends every thread on its
/// frames, so there is nothing to read.
pub fn read_all_threads(
    panels: &mut PanelsState,
    threads: Option<&Value>,
    next_request_id: impl FnOnce() -> u64,
) -> Option<Effect> {
    if !panels.threads_show_all || panels.threads_all_request.is_some() {
        return None;
    }
    let revision = threads?
        .get("revision")
        .and_then(Value::as_str)
        .filter(|revision| !revision.is_empty())?;
    if panels
        .threads_all
        .as_ref()
        .is_some_and(|(loaded, _)| loaded == revision)
    {
        return None;
    }
    let request_id = next_request_id();
    panels.threads_all_request = Some((request_id, revision.to_string()));
    Some(Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::ReadCoordinatorThreads,
        params: Box::new(json!({})),
    })
}

/// Takes the answer to [`read_all_threads`]; `false` when `request_id` is not that read.
pub fn settle_read(
    panels: &mut PanelsState,
    request_id: u64,
    answer: Result<&Value, String>,
) -> bool {
    let asked = match panels.threads_all_request.take() {
        Some((id, asked)) if id == request_id => asked,
        other => {
            panels.threads_all_request = other;
            return false;
        }
    };
    panels.threads_all = Some(match answer {
        Ok(result) => (
            result
                .get("revision")
                .and_then(Value::as_str)
                .unwrap_or(&asked)
                .to_string(),
            result.get("threads").cloned().unwrap_or(Value::Null),
        ),
        Err(_) => (asked, Value::Null),
    });
    true
}

fn parse_rows(rows: Option<&Value>) -> Vec<(String, CoordinatorThreadRow)> {
    rows.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|row| {
            let working = text(row, "state") == "working";
            (
                text(row, "activeAt"),
                CoordinatorThreadRow {
                    key: format!("{}:{}", text(row, "projectId"), text(row, "sessionId")),
                    project_id: text(row, "projectId"),
                    session_id: text(row, "sessionId"),
                    title: text(row, "title"),
                    detail: text(row, "detail"),
                    working,
                    needs_approval: !working && row.get("needsApproval") == Some(&Value::Bool(true)),
                    recent: row.get("recent") == Some(&Value::Bool(true)),
                    branch: text(row, "branch"),
                    lifecycle_state: text(row, "lifecycleState"),
                },
            )
        })
        .collect()
}

/// The panel, or `None` for a session that is not a coordinator or has no threads yet. `threads`
/// is the frames' summary; `all` the full list "N more" read, while it is open.
pub fn coordinator_threads_panel(
    threads: Option<&Value>,
    collapsed: bool,
    show_all: bool,
    all: Option<&Value>,
) -> Option<CoordinatorThreadsPanel> {
    let threads = threads?;
    let summary = parse_rows(threads.get("threads"));
    let all = all.filter(|all| all.is_array()).filter(|_| show_all);
    let mut rows = match all {
        Some(all) => parse_rows(Some(all)),
        None => summary,
    };
    // An older gxserver sends every thread and no total.
    let total = threads
        .get("total")
        .and_then(Value::as_u64)
        .map_or(0, |total| total as usize)
        .max(rows.len());
    if total == 0 {
        return None;
    }
    // Working, then blocked on the user, then the rest; each newest first (ISO times sort as text).
    // The summary holds exactly the rows listed by default; the full list holds every thread.
    let rank = |row: &CoordinatorThreadRow| match (row.working, row.needs_approval) {
        (true, _) => 0,
        (false, true) => 1,
        (false, false) => 2,
    };
    rows.sort_by(|(left_at, left), (right_at, right)| {
        rank(left)
            .cmp(&rank(right))
            .then_with(|| right_at.cmp(left_at))
    });
    let working = rows.iter().filter(|(_, row)| row.working).count();
    let approval = rows.iter().filter(|(_, row)| row.needs_approval).count();
    let listed = rows
        .iter()
        .filter(|(_, row)| row.working || row.needs_approval || row.recent)
        .count();
    let hidden = total.saturating_sub(listed);
    let mut meta = Vec::new();
    if working > 0 {
        meta.push(format!("{working} working"));
    }
    if approval > 0 {
        meta.push(plural(approval, "needs your approval", "need your approval"));
    }
    meta.push(plural(total, "thread", "threads"));
    let rows: Vec<CoordinatorThreadRow> = rows
        .into_iter()
        .map(|(_, row)| row)
        .filter(|row| show_all || row.working || row.needs_approval || row.recent)
        .collect();
    Some(CoordinatorThreadsPanel {
        meta: meta.join(" \u{b7} "),
        attention: approval > 0,
        collapsed,
        show_all,
        more_label: match (hidden > 0, show_all) {
            (false, _) => String::new(),
            (true, true) => "Show fewer".to_string(),
            (true, false) => format!("{hidden} more"),
        },
        rows,
    })
}

/// The document value, `null` when there is no panel.
pub fn project(
    threads: Option<&Value>,
    collapsed: bool,
    show_all: bool,
    all: Option<&Value>,
) -> Value {
    coordinator_threads_panel(threads, collapsed, show_all, all)
        .and_then(|panel| serde_json::to_value(panel).ok())
        .unwrap_or(Value::Null)
}
