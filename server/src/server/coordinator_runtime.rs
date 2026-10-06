//! The coordinator supervisor: every two seconds it looks at every open thread and tells its
//! coordinator when the thread finished a turn, started waiting on an answer, or was closed.
//! Also the HTTP glue for the coordinator endpoints and coordinator creation.
//!
//! CDXC:Coordinators 2026-09-30 WHY:
//! firstmate's lesson: supervision must cost the coordinator nothing until something needs it. gxserver already knows every thread's state from agent hooks, so it watches and the coordinator only wakes for a report; the coordinator never polls, sleeps or runs `wait-for-text`. A report is marked delivered only after the send succeeded, so a restart or a coordinator busy with a question card delays a report but never loses or repeats it.
//! SEE-ALSO: server/src/coordinators/ (records, states, report text), server/src/delayed_sends.rs (the same stability-window idea for one watched agent).

use super::*;

use crate::coordinators::{
    self, agent_message, classify_thread_session, clear_thread_pending_message, list_threads,
    record_thread_report, report_body, set_thread_observed_working, set_thread_resolved,
    thread_prompt, MessageSender, SessionKey, ThreadProgress, ThreadRecord, ThreadReport,
    ThreadState,
};
use crate::presentation::effective_lifecycle_state;
use crate::session_chat_queue_runtime::SessionChatTranscriptGate;

const TICK: Duration = Duration::from_secs(2);
/// A thread must stay out of work this long before its turn counts as finished: hooks and title
/// observation can dip between tool calls.
const FINISH_STABILITY_MS: i64 = 4_000;
/// Waits after a failed delivery, doubled per failure up to the last value.
const RETRY_DELAYS_MS: [i64; 4] = [10_000, 20_000, 40_000, 60_000];
/// A message handed to a thread is reported undelivered only once it is this old...
const UNDELIVERED_MIN_AGE_MS: i64 = 60_000;
/// ...and the thread has sat idle without it this long: a busy thread takes a message typed during
/// its turn at its next input boundary, and a starting one once its input box appears.
const UNDELIVERED_IDLE_MS: i64 = 20_000;
/// How often one thread's transcript is read for a pending message.
const DELIVERY_CHECK_EVERY_MS: i64 = 6_000;

#[derive(Default)]
struct SupervisorMemory {
    /// When each thread was first seen out of work since it last worked.
    not_working_since: HashMap<SessionKey, i64>,
    /// Coordinators with a delivery running right now.
    in_flight: HashSet<SessionKey>,
    /// Coordinator → (consecutive failures, earliest retry in ms).
    retry: HashMap<SessionKey, (usize, i64)>,
    transcript_gates: HashMap<SessionKey, SessionChatTranscriptGate>,
    /// Each thread's transcript file, resolved once for the pending-message check.
    transcript_paths: HashMap<SessionKey, std::path::PathBuf>,
    /// When each thread's pending message was last looked for in its transcript.
    delivery_checked_at: HashMap<SessionKey, i64>,
    /// Since when a thread has been idle without its pending message (sent at, since).
    undelivered_idle_since: HashMap<SessionKey, (String, i64)>,
    /// The `sendRequestId` of each report still waiting to reach its coordinator, by coordinator
    /// and report text, so a retried report is the same send to gxserver's send ledger.
    report_send_ids: HashMap<(SessionKey, String), String>,
}

enum ReportKind {
    Finished,
    Waiting {
        key: String,
        summary: String,
    },
    Closed,
    Undelivered {
        sent_at: String,
        excerpt: String,
        evidence: Option<String>,
    },
}

struct PendingReport {
    thread: ThreadRecord,
    kind: ReportKind,
    sender: MessageSender,
    session: Option<Value>,
}

struct Delivery {
    coordinator: SessionKey,
    reports: Vec<PendingReport>,
}

pub(crate) fn start_coordinator_runtime(state: Arc<AppState>) {
    let _ = coordinators::ensure_coordinator_role_file(&state.paths);
    let memory = Arc::new(Mutex::new(SupervisorMemory::default()));
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut clock = tokio::time::interval(TICK);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = clock.tick() => {}
            }
            let ticking = state.clone();
            let ticking_memory = memory.clone();
            let deliveries = tokio::task::spawn_blocking(move || tick(&ticking, &ticking_memory))
                .await
                .unwrap_or_default();
            for delivery in deliveries {
                tokio::spawn(deliver(state.clone(), memory.clone(), delivery));
            }
        }
    });
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

/// Launcher display names by agent id ("Claude" for a custom Claude launcher), as the sidebar and
/// `ghostex agents types` show them.
fn agent_names(repository: &DomainRepository<'_>) -> HashMap<String, String> {
    let projects = repository.list_projects().unwrap_or_default();
    crate::sidebar_hud::sidebar_agent_buttons_from_projects(&projects)
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|agent| {
            let id = text(agent, "agentId");
            let name = text(agent, "name");
            (!id.is_empty() && !name.is_empty()).then_some((id, name))
        })
        .collect()
}

fn sender_for(
    state: &AppState,
    project: Option<&Value>,
    session: &Value,
    thread: &ThreadRecord,
    names: &HashMap<String, String>,
) -> MessageSender {
    let presentation = project.map(|project| {
        crate::presentation::project_presentation_session(
            project,
            &crate::presentation::default_group_id(&thread.project_id),
            session,
            &crate::presentation::now_iso(),
        )
    });
    let presented = presentation.as_ref().unwrap_or(session);
    let title = presented
        .get("displayTitle")
        .or_else(|| presented.get("title"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    MessageSender {
        agent_name: names
            .get(&text(session, "agentId"))
            .cloned()
            .unwrap_or_else(|| {
                let name = text(presented, "agentName");
                if name.is_empty() {
                    text(session, "agentId")
                } else {
                    name
                }
            }),
        title,
        session_id: thread.session_id.clone(),
        agent_id: text(session, "agentId"),
        agent_session_id: session
            .pointer("/runtimeSettings/agentSessionId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        global_ref: crate::ids::create_global_session_ref(
            state.metadata.server_id.as_str(),
            &thread.project_id,
            &thread.session_id,
        ),
    }
}

fn closed_sender(state: &AppState, thread: &ThreadRecord) -> MessageSender {
    MessageSender {
        title: thread
            .task
            .lines()
            .next()
            .unwrap_or_default()
            .chars()
            .take(80)
            .collect(),
        session_id: thread.session_id.clone(),
        global_ref: crate::ids::create_global_session_ref(
            state.metadata.server_id.as_str(),
            &thread.project_id,
            &thread.session_id,
        ),
        ..MessageSender::default()
    }
}

fn tick(state: &AppState, memory: &Mutex<SupervisorMemory>) -> Vec<Delivery> {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return Vec::new();
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    for (project_id, session_id) in
        crate::coordinators::refresh_coordinator_panels(&db, &repository)
    {
        republish_coordinator_chat(state, &project_id, &session_id);
    }
    let Ok(threads) = list_threads(&db) else {
        return Vec::new();
    };
    let Ok(mut memory) = memory.lock() else {
        return Vec::new();
    };
    let open = threads
        .into_iter()
        .filter(|thread| !thread.is_resolved())
        .collect::<Vec<_>>();
    let open_keys = open.iter().map(ThreadRecord::key).collect::<HashSet<_>>();
    memory
        .not_working_since
        .retain(|key, _| open_keys.contains(key));
    memory
        .transcript_gates
        .retain(|key, _| open_keys.contains(key));
    memory
        .transcript_paths
        .retain(|key, _| open_keys.contains(key));
    memory
        .delivery_checked_at
        .retain(|key, _| open_keys.contains(key));
    memory
        .undelivered_idle_since
        .retain(|key, _| open_keys.contains(key));
    if open.is_empty() {
        return Vec::new();
    }
    let now = now_ms();
    let now_iso = crate::presentation::now_iso();
    let mut coordinator_alive: HashMap<SessionKey, bool> = HashMap::new();
    let mut projects: HashMap<String, Option<Value>> = HashMap::new();
    let mut names: Option<HashMap<String, String>> = None;
    let mut pending: std::collections::BTreeMap<SessionKey, Vec<PendingReport>> =
        std::collections::BTreeMap::new();
    for thread in open {
        let coordinator_key = thread.coordinator_key();
        let alive = *coordinator_alive
            .entry(coordinator_key.clone())
            .or_insert_with(|| {
                repository
                    .get_session(&coordinator_key.0, &coordinator_key.1)
                    .ok()
                    .flatten()
                    .is_some_and(|session| {
                        matches!(
                            effective_lifecycle_state(&session).as_str(),
                            "running" | "sleeping"
                        )
                    })
            });
        // A closed coordinator has nobody to report to; its threads wait until it is resumed.
        if !alive {
            continue;
        }
        let key = thread.key();
        let session = repository
            .get_session(&thread.project_id, &thread.session_id)
            .ok()
            .flatten();
        let Some(session) = session else {
            memory.not_working_since.remove(&key);
            pending
                .entry(coordinator_key)
                .or_default()
                .push(PendingReport {
                    sender: closed_sender(state, &thread),
                    thread,
                    kind: ReportKind::Closed,
                    session: None,
                });
            continue;
        };
        let lifecycle = effective_lifecycle_state(&session);
        if lifecycle == "stopped" {
            memory.not_working_since.remove(&key);
            let project = projects
                .entry(thread.project_id.clone())
                .or_insert_with(|| repository.get_project(&thread.project_id).ok().flatten())
                .clone();
            pending
                .entry(coordinator_key)
                .or_default()
                .push(PendingReport {
                    sender: sender_for(
                        state,
                        project.as_ref(),
                        &session,
                        &thread,
                        names.get_or_insert_with(|| agent_names(&repository)),
                    ),
                    thread,
                    kind: ReportKind::Closed,
                    session: Some(session),
                });
            continue;
        }
        if !matches!(lifecycle.as_str(), "running" | "sleeping") {
            // `missing` / `unknown`: the provider has not been probed or died; nothing to report yet.
            continue;
        }
        // The supervisor reads the session's own state; "starting" is a presentation notion.
        let as_run = ThreadProgress {
            resolved: false,
            has_run: true,
        };
        let hook_state = classify_thread_session(Some(&session), as_run, &now_iso, false);
        // A thread stuck on a screen that blocks input (folder trust, an expired login, a usage
        // limit) never works and never asks through a hook, so it would never report. The chat's
        // own cached screen reading says so without a new screen capture.
        let screen = (lifecycle == "running").then(|| {
            crate::session_chat_options::cached_session_chat_screen_state(
                state,
                &thread.project_id,
                &thread.session_id,
            )
        });
        /*
        CDXC:Coordinators 2026-10-06 WHY:
        Empryo has no hook for a question or an approval: the PreToolUse of the tool that asks leaves its hooks at working while its choice panel waits, so its thread never reported waiting. While hooks say working, a question or a permission prompt the chat read off the screen still counts (Claude's own permission prompt moves its hooks to attention, so Claude and Codex threads report as before); any other blocking screen counts only once hooks stop saying working, as before.
        SEE-ALSO: server/src/session_chat_empryo_question.rs (the panel reading).
        */
        let blocking_notice = screen
            .as_ref()
            .and_then(|screen| screen.notice.clone())
            .filter(|notice| notice.blocks_input() && !notice.auto_trust)
            .filter(|notice| {
                hook_state != ThreadState::Working
                    || notice.kind
                        == crate::session_chat_notice::SESSION_CHAT_NOTICE_PERMISSION_PROMPT
            });
        let screen_question = screen
            .and_then(|screen| screen.prompt)
            .map(|prompt| coordinators::screen_thread_prompt(&prompt));
        let state_now = if blocking_notice.is_some() || screen_question.is_some() {
            ThreadState::Waiting
        } else if hook_state != ThreadState::Working
            && hook_state != ThreadState::Sleeping
            && thread.observed_working
        {
            // Only a thread about to be reported pays for the transcript read.
            let working = memory
                .transcript_gates
                .entry(key.clone())
                .or_default()
                .is_working(&session);
            if working {
                ThreadState::Working
            } else {
                hook_state
            }
        } else {
            hook_state
        };
        let report = match state_now {
            ThreadState::Working => {
                memory.not_working_since.remove(&key);
                if !thread.observed_working {
                    let _ =
                        set_thread_observed_working(&db, &thread.project_id, &thread.session_id);
                }
                None
            }
            ThreadState::Waiting => {
                memory.not_working_since.remove(&key);
                let prompt = match (
                    &blocking_notice,
                    thread_prompt(&session).or(screen_question),
                ) {
                    (Some(notice), _) => Some((
                        format!("notice:{}:{}", notice.kind, notice.title),
                        match notice.detail.as_deref().map(str::trim).filter(|detail| !detail.is_empty()) {
                            Some(detail) => format!("Its screen shows: {}\n\n{detail}\n\nSomeone has to answer it in that thread.", notice.title.trim()),
                            None => format!("Its screen shows: {}\n\nSomeone has to answer it in that thread.", notice.title.trim()),
                        },
                    )),
                    (None, Some(prompt)) => Some((format!("prompt:{}", prompt.key), prompt.summary)),
                    (None, None) => None,
                };
                let (prompt_key, summary) = match prompt {
                    Some(prompt) => prompt,
                    None => (
                        format!(
                            "attention:{}",
                            session
                                .pointer("/runtimeSettings/agentActivity/attentionEventId")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                        ),
                        "It is waiting for someone to look at its screen (an approval or a prompt the chat cannot show).".to_string(),
                    ),
                };
                (thread.reported_prompt_key.as_deref() != Some(prompt_key.as_str())).then_some(
                    ReportKind::Waiting {
                        key: prompt_key,
                        summary,
                    },
                )
            }
            ThreadState::Finished | ThreadState::Sleeping if thread.observed_working => {
                let since = *memory.not_working_since.entry(key.clone()).or_insert(now);
                (now - since >= FINISH_STABILITY_MS).then_some(ReportKind::Finished)
            }
            _ => None,
        };
        let report = match report {
            Some(report) => Some(report),
            None => pending_delivery(&db, &mut memory, &thread, &session, state_now, now),
        };
        if let Some(kind) = report {
            let project = projects
                .entry(thread.project_id.clone())
                .or_insert_with(|| repository.get_project(&thread.project_id).ok().flatten())
                .clone();
            pending
                .entry(coordinator_key)
                .or_default()
                .push(PendingReport {
                    sender: sender_for(
                        state,
                        project.as_ref(),
                        &session,
                        &thread,
                        names.get_or_insert_with(|| agent_names(&repository)),
                    ),
                    thread,
                    kind,
                    session: Some(session),
                });
        }
    }
    let mut deliveries = Vec::new();
    for (coordinator, reports) in pending {
        if memory.in_flight.contains(&coordinator) {
            continue;
        }
        if memory
            .retry
            .get(&coordinator)
            .is_some_and(|(_, retry_at)| now < *retry_at)
        {
            continue;
        }
        memory.in_flight.insert(coordinator.clone());
        deliveries.push(Delivery {
            coordinator,
            reports,
        });
    }
    deliveries
}

async fn deliver(state: Arc<AppState>, memory: Arc<Mutex<SupervisorMemory>>, delivery: Delivery) {
    let coordinator = delivery.coordinator.clone();
    // CDXC:Coordinators 2026-10-04 WHY: closing a thread and then its coordinator a second apart let a tick queue the thread's "closed" report while the coordinator was still open, and sending it woke the closed coordinator back up. The coordinator is checked again right before sending; a report for a closed one waits until it is resumed.
    let open_state = state.clone();
    let open_key = coordinator.clone();
    let coordinator_open =
        tokio::task::spawn_blocking(move || coordinator_is_open(&open_state, &open_key))
            .await
            .unwrap_or(false);
    let mut failed = false;
    let reports = if coordinator_open {
        delivery.reports
    } else {
        Vec::new()
    };
    for report in reports {
        let thread_ref = report.sender.global_ref.clone();
        let (body, finished_text) = match &report.kind {
            ReportKind::Finished => {
                let session = report.session.clone();
                let reported_at = report.thread.reported_at.clone();
                let message = tokio::task::spawn_blocking(move || {
                    session
                        .as_ref()
                        .and_then(crate::notification_feed::body::last_assistant_message)
                        .filter(|(_, timestamp)| {
                            // A reply we already forwarded is not this turn's report.
                            match (timestamp, reported_at.as_deref().and_then(parse_iso_ms_opt)) {
                                (Some(timestamp), Some(reported)) => *timestamp > reported,
                                _ => true,
                            }
                        })
                        .map(|(text, _)| text)
                })
                .await
                .ok()
                .flatten();
                let body = report_body(
                    &ThreadReport::Finished {
                        message: message.as_deref(),
                    },
                    &thread_ref,
                );
                (body, Some(message.unwrap_or_default()))
            }
            ReportKind::Waiting { summary, .. } => (
                report_body(&ThreadReport::Waiting { prompt: summary }, &thread_ref),
                None,
            ),
            ReportKind::Closed => (report_body(&ThreadReport::Closed, &thread_ref), None),
            ReportKind::Undelivered {
                excerpt, evidence, ..
            } => (
                report_body(
                    &ThreadReport::Undelivered {
                        excerpt,
                        evidence: evidence.as_deref(),
                    },
                    &thread_ref,
                ),
                None,
            ),
        };
        let message = agent_message(&report.sender, &body);
        let report_key = (coordinator.clone(), message.clone());
        let send_request_id = memory
            .lock()
            .map(|mut memory| {
                memory
                    .report_send_ids
                    .entry(report_key.clone())
                    .or_insert_with(|| Uuid::new_v4().to_string())
                    .clone()
            })
            .unwrap_or_else(|_| Uuid::new_v4().to_string());
        match send_to_coordinator(&state, &coordinator, &message, &send_request_id).await {
            Ok(()) => {
                if let Ok(mut memory) = memory.lock() {
                    memory.report_send_ids.remove(&report_key);
                }
                let settle_state = state.clone();
                let thread = report.thread.clone();
                let kind_key = match &report.kind {
                    ReportKind::Waiting { key, .. } => Some(key.clone()),
                    _ => None,
                };
                let closed = matches!(report.kind, ReportKind::Closed);
                let undelivered_sent_at = match &report.kind {
                    ReportKind::Undelivered { sent_at, .. } => Some(sent_at.clone()),
                    _ => None,
                };
                let _ = tokio::task::spawn_blocking(move || match undelivered_sent_at {
                    Some(sent_at) => settle_undelivered(&settle_state, &thread, &sent_at),
                    None => settle_report(
                        &settle_state,
                        &thread,
                        finished_text.as_deref(),
                        kind_key.as_deref(),
                        closed,
                    ),
                })
                .await;
            }
            Err(error) => {
                let _ = state.logger.log_routine(
                    crate::logging::DiagnosticLogScenario::ServerLifecycle,
                    GxserverLogInput {
                        level: LogLevel::Warn,
                        event: "coordinatorReportDeferred".to_string(),
                        server_id: Some(state.metadata.server_id.clone()),
                        request_id: None,
                        client: None,
                        duration_ms: None,
                        error: Some(error.chars().take(300).collect()),
                        details: Some(json!({
                            "coordinatorProjectId": coordinator.0,
                            "coordinatorSessionId": coordinator.1,
                            "threadSessionId": report.thread.session_id,
                            "sendRequestId": send_request_id,
                        })),
                    },
                );
                failed = true;
                break;
            }
        }
    }
    if let Ok(mut memory) = memory.lock() {
        memory.in_flight.remove(&coordinator);
        if failed {
            let failures = memory
                .retry
                .get(&coordinator)
                .map(|(count, _)| *count)
                .unwrap_or(0);
            let delay = RETRY_DELAYS_MS[failures.min(RETRY_DELAYS_MS.len() - 1)];
            memory
                .retry
                .insert(coordinator, (failures + 1, now_ms() + delay));
        } else {
            memory.retry.remove(&coordinator);
            memory
                .report_send_ids
                .retain(|(owner, _), _| owner != &coordinator);
        }
    }
}

fn parse_iso_ms_opt(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.timestamp_millis())
}

fn settle_report(
    state: &AppState,
    thread: &ThreadRecord,
    finished_text: Option<&str>,
    prompt_key: Option<&str>,
    closed: bool,
) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    if closed {
        let _ = set_thread_resolved(&db, &thread.project_id, &thread.session_id, true);
    } else {
        let _ = record_thread_report(
            &db,
            &thread.project_id,
            &thread.session_id,
            finished_text.map(|text| {
                if text.is_empty() {
                    "(no final message)"
                } else {
                    text
                }
            }),
            prompt_key,
        );
    }
    let _ = schedule_presentation_session_delta(
        state,
        &db,
        &repository,
        &thread.project_id,
        &thread.session_id,
    );
}

/// The thread's pending message, checked against its transcript: cleared once it shows up, and
/// reported to the coordinator when the thread sits idle without it. See the CDXC:Coordinators
/// 2026-10-04 note on `watch_pending_message` in server/src/coordinators/endpoint.rs.
fn pending_delivery(
    db: &rusqlite::Connection,
    memory: &mut SupervisorMemory,
    thread: &ThreadRecord,
    session: &Value,
    state_now: ThreadState,
    now: i64,
) -> Option<ReportKind> {
    let key = thread.key();
    let (Some(excerpt), Some(sent_at)) = (
        thread.pending_message.as_deref(),
        thread.pending_message_at.as_deref(),
    ) else {
        memory.undelivered_idle_since.remove(&key);
        return None;
    };
    // A working thread is either on the message or takes it at its next input boundary, so it is
    // never reported; its transcript is still read, so a message it took clears while it works.
    let working = state_now == ThreadState::Working;
    if working {
        memory.undelivered_idle_since.remove(&key);
    }
    if memory
        .delivery_checked_at
        .get(&key)
        .is_some_and(|checked| now - checked < DELIVERY_CHECK_EVERY_MS)
    {
        return None;
    }
    memory.delivery_checked_at.insert(key.clone(), now);
    let sent_ms = parse_iso_ms_opt(sent_at).unwrap_or(now);
    let needles = coordinators::delivery_needles(excerpt);
    let mut path = memory.transcript_paths.remove(&key);
    let recorded = coordinators::transcript_records_message(session, &needles, sent_ms, &mut path);
    if let Some(path) = path {
        memory.transcript_paths.insert(key.clone(), path);
    }
    match recorded {
        Some(true) => {
            memory.undelivered_idle_since.remove(&key);
            let _ =
                clear_thread_pending_message(db, &thread.project_id, &thread.session_id, sent_at);
            // A turn too short for a tick to see it working still gets its report.
            if !thread.observed_working {
                let _ = set_thread_observed_working(db, &thread.project_id, &thread.session_id);
            }
            return None;
        }
        // No transcript to judge by yet: a missing message cannot be told from a slow agent.
        None => {
            memory.undelivered_idle_since.remove(&key);
            return None;
        }
        Some(false) => {}
    }
    // A question or a blocking screen is reported as waiting; the message follows its answer.
    if working || state_now == ThreadState::Waiting {
        memory.undelivered_idle_since.remove(&key);
        return None;
    }
    // Held in the thread's own queue: it goes out once the input box is ready, and a queued
    // message from another agent that fails already tells its sender.
    let queue = crate::session_chat_queue::read_session_chat_queue_snapshot_with(
        db,
        &thread.project_id,
        &thread.session_id,
    );
    if let Some(row) = queue.queue.iter().find(|row| {
        coordinators::holds_message(&coordinators::normalize_delivery_text(&row.text), &needles)
    }) {
        memory.undelivered_idle_since.remove(&key);
        if row.state == crate::session_chat_queue::SESSION_CHAT_QUEUE_STATE_FAILED {
            let _ =
                clear_thread_pending_message(db, &thread.project_id, &thread.session_id, sent_at);
        }
        return None;
    }
    let idle = memory
        .undelivered_idle_since
        .entry(key)
        .or_insert_with(|| (sent_at.to_string(), now));
    if idle.0 != sent_at {
        *idle = (sent_at.to_string(), now);
    }
    if now - sent_ms < UNDELIVERED_MIN_AGE_MS || now - idle.1 < UNDELIVERED_IDLE_MS {
        return None;
    }
    let evidence = crate::session_chat_notice::session_chat_watchdog_notice(
        &thread.project_id,
        &thread.session_id,
    )
    .map(|notice| notice.title);
    Some(ReportKind::Undelivered {
        sent_at: sent_at.to_string(),
        excerpt: excerpt.to_string(),
        evidence,
    })
}

fn coordinator_is_open(state: &AppState, coordinator: &SessionKey) -> bool {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return false;
    };
    DomainRepository::new(&db, state.metadata.server_id.as_str())
        .get_session(&coordinator.0, &coordinator.1)
        .ok()
        .flatten()
        .is_some_and(|session| {
            matches!(
                effective_lifecycle_state(&session).as_str(),
                "running" | "sleeping"
            )
        })
}

fn settle_undelivered(state: &AppState, thread: &ThreadRecord, sent_at: &str) {
    if let Ok(db) = open_gxserver_database(&state.paths) {
        let _ = clear_thread_pending_message(&db, &thread.project_id, &thread.session_id, sent_at);
    }
}

/// The same default delivery `ghostex agents send` uses: typed now, picked up by a busy agent at
/// its next input boundary, and a sleeping coordinator is woken for it. A report retried after a
/// failure keeps its `sendRequestId`, so one that did arrive is never typed again.
async fn send_to_coordinator(
    state: &AppState,
    coordinator: &SessionKey,
    message: &str,
    send_request_id: &str,
) -> std::result::Result<(), String> {
    let body = json!({
        "params": {
            "projectId": coordinator.0,
            "sessionId": coordinator.1,
            "text": message,
            "sendRequestId": send_request_id,
        }
    });
    let routed = crate::session_chat_send::handle_send_session_chat_message_http(
        state,
        "/api/sendSessionChatMessage".to_string(),
        Uuid::new_v4().to_string(),
        &body,
    )
    .await;
    if routed.response.status().is_success() {
        return Ok(());
    }
    let bytes = to_bytes(routed.response.into_body(), 64 * 1024)
        .await
        .unwrap_or_default();
    Err(String::from_utf8_lossy(&bytes).to_string())
}

/// Re-sends an open coordinator chat its current state, now carrying the refreshed Threads panel;
/// the same frame a queue change sends, without its presentation delta.
fn republish_coordinator_chat(state: &AppState, project_id: &str, session_id: &str) {
    let key = session_observer_key(project_id, session_id);
    let options = state
        .session_chat_option_cache
        .lock()
        .ok()
        .and_then(|cache| cache.get(&key).map(|entry| entry.value.options.clone()))
        .unwrap_or_default();
    let screen = crate::session_chat_options::cached_session_chat_screen_state(
        state, project_id, session_id,
    );
    crate::session_chat_options::emit_session_chat_options_state_frame(
        &state.session_chat_followers,
        &state.event_hub,
        &state.paths,
        &state.metadata.server_id,
        project_id,
        session_id,
        options.as_ref(),
        screen.borrow(),
    );
}

/// `/api/createAgentSession` with a `coordinator` object: points the launch at the role file.
pub(crate) fn prepare_coordinator_create_params(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<Map<String, Value>, DomainStateError> {
    let mut params = params.clone();
    let Some(coordinator) = params.get_mut("coordinator").and_then(Value::as_object_mut) else {
        return Ok(params);
    };
    let role_file = coordinators::ensure_coordinator_role_file(&state.paths).map_err(|error| {
        DomainStateError {
            code: "internalError",
            message: format!("Could not write the coordinator role file: {error}"),
        }
    })?;
    coordinator.insert(
        "roleFile".to_string(),
        Value::String(role_file.to_string_lossy().to_string()),
    );
    Ok(params)
}

/// Queues a message in a session's chat queue and tells its viewers.
fn queue_session_chat_prompt(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    text: &str,
    startup_send: bool,
) -> std::result::Result<(), DomainStateError> {
    let mut queue_params = Map::new();
    queue_params.insert("projectId".to_string(), json!(project_id));
    queue_params.insert("sessionId".to_string(), json!(session_id));
    queue_params.insert("text".to_string(), json!(text));
    queue_params.insert("startupSend".to_string(), json!(startup_send));
    let result = crate::session_chat_queue::handle_session_chat_queue_endpoint(
        &state.paths,
        state.metadata.server_id.as_str(),
        "/api/queueSessionChatPrompt",
        &queue_params,
    )?;
    if result.broadcast {
        crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
            state, project_id, session_id,
        );
    }
    Ok(())
}

/// Hands a coordinator whose role arrives as a queued line (Empryo's `/agent`, see
/// `coordinator_role_queued_command`) its role: writes the profile the line names, then queues
/// it. A new coordinator's line waits for the input box like a first message; a promoted one's
/// waits for the running turn to end.
pub(crate) fn queue_coordinator_role_command(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    command: &str,
    startup_send: bool,
) -> std::result::Result<(), DomainStateError> {
    coordinators::ensure_empryo_coordinator_agent_file(&state.paths).map_err(|error| {
        DomainStateError {
            code: "internalError",
            message: format!("Could not write the Empryo coordinator profile: {error}"),
        }
    })?;
    queue_session_chat_prompt(state, project_id, session_id, command, startup_send)
}

/// `/api/promoteCoordinator`: makes an existing session a coordinator, then queues its playbook
/// in the session's chat queue, which hands it over only once the agent is idle (never mid-turn).
/// See the CDXC:Coordinators decision on `promote_session_to_coordinator`.
pub(crate) fn promote_coordinator(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> std::result::Result<Value, DomainStateError> {
    let role_file = coordinators::ensure_coordinator_role_file(&state.paths).map_err(|error| {
        DomainStateError {
            code: "internalError",
            message: format!("Could not write the coordinator role file: {error}"),
        }
    })?;
    let promotion = coordinators::promote_session_to_coordinator(
        db,
        state.metadata.server_id.as_str(),
        params,
        &role_file,
    )?;
    let (project_id, session_id) = &promotion.key;
    schedule_presentation_session_delta(state, db, repository, project_id, session_id)?;
    // The coordinator record is committed, so a failure from here on is reported, not raised.
    let playbook_error = promotion
        .role_command
        .as_deref()
        .map_or(Ok(()), |command| {
            queue_coordinator_role_command(state, project_id, session_id, command, false)
        })
        .and_then(|()| {
            queue_session_chat_prompt(
                state,
                project_id,
                session_id,
                &promotion.playbook_message,
                false,
            )
        })
        .err()
        .map(|error| error.message);
    Ok(json!({
        "ok": true,
        "globalRef": crate::ids::create_global_session_ref(state.metadata.server_id.as_str(), project_id, session_id),
        "title": promotion.title,
        "playbookQueued": playbook_error.is_none(),
        "playbookError": playbook_error,
    }))
}

pub(crate) fn handle_coordinator_http(
    state: &AppState,
    endpoint_path: &str,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> std::result::Result<Value, DomainStateError> {
    let output = coordinators::handle_coordinator_endpoint(endpoint_path, db, repository, params)?;
    for (project_id, session_id) in &output.changed_sessions {
        schedule_presentation_session_delta(state, db, repository, project_id, session_id)?;
    }
    Ok(output.result)
}
