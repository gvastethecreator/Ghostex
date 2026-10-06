//! A coordinator's Threads panel, above the composer: one list of its threads, the working ones
//! first, then the latest others, with the rest a click away.
//!
//! CDXC:Coordinators 2026-09-30 WHY:
//! The panel shows a coordinator's threads without opening the sidebar, which the phone does not have. Order, labels and the fold are decided here once for the desktop, web and phone renderers; gxserver only sends each thread's state, activity time and one line of detail (`coordinatorThreads`).
//!
//! CDXC:Coordinators 2026-10-06 DECISION:
//! The user, on the panel grouped Working / Finished / Sleeping / Done: "We need to simplify the sections here. I don't like sleeping, done, finished. User doesn't care about 'sleeping' or 'awake'. Also done and finished mean the same thing." Then: "I don't think 'Needs you' is an actual case, since threads just talk to the main agent in a coordinator; they never actually directly 'need me'." And: "Don't hide them. We show the last active few like we do now and I can click to see more in the threads component in the chat view." So there are no group headings: working threads first, then the 3 most recently active other threads (kept from the 2026-10-05 "show latest 3" decision, which this supersedes otherwise), then a "N more" row that lists every thread in place and folds them again. Every row still opens its thread, resolved ones included. The header says only how many work and how many threads there are. The rare thread blocked on something only the user can grant (a permission or folder-trust prompt) stays in the list right after the working ones with an amber "Needs your approval" tag instead of a group.
//! SEE-ALSO: server/src/coordinators/panel.rs (the field), apps/desktop/src/app/native_chat/coordinator_threads.rs and apps/mobile/app/src/chat/native/cards/AgentPanels.tsx (the renderers), packages/gx-core/src/sidebar_view/threads.rs (the sidebar's two-hour rule).

use serde::{Deserialize, Serialize};
use serde_json::Value;

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

/// Threads that neither work nor need approval listed before the rest fold behind `more_label`.
const OTHER_ROWS_SHOWN: usize = 3;

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

/// The panel, or `None` for a session that is not a coordinator or has no threads yet.
pub fn coordinator_threads_panel(
    threads: Option<&Value>,
    collapsed: bool,
    show_all: bool,
) -> Option<CoordinatorThreadsPanel> {
    let mut rows: Vec<(String, CoordinatorThreadRow)> = threads?
        .get("threads")
        .and_then(Value::as_array)
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
                    branch: text(row, "branch"),
                    lifecycle_state: text(row, "lifecycleState"),
                },
            )
        })
        .collect();
    if rows.is_empty() {
        return None;
    }
    // Working, then blocked on the user, then the rest; each newest first (ISO times sort as text).
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
    let total = rows.len();
    let working = rows.iter().filter(|(_, row)| row.working).count();
    let approval = rows.iter().filter(|(_, row)| row.needs_approval).count();
    let pinned = working + approval;
    let hidden = total.saturating_sub(pinned + OTHER_ROWS_SHOWN);
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
        .take(if show_all { total } else { total - hidden })
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
pub fn project(threads: Option<&Value>, collapsed: bool, show_all: bool) -> Value {
    coordinator_threads_panel(threads, collapsed, show_all)
        .and_then(|panel| serde_json::to_value(panel).ok())
        .unwrap_or(Value::Null)
}
