//! CDXC:SessionChat 2026-10-07 DECISION:
//! Coordinator for Sven: "right after a window joins, Ghostex makes Empryo show this session's own tab … a send must never land in another session's tab: hold it until the right tab shows."
//!
//! CDXC:SessionChat 2026-10-07 WHY:
//! Empryo 3.9.1-beta runs one engine per repository for every window in it. A window that starts while the engine is up joins it, adds its own tab, and opens on the engine's first tab instead (`tabs.find(own) ?? tabs[0]` in its engine boot), so the coordinator's `/agent` line and chat messages typed there went into another session's conversation (seen live 2026-10-07). Every window rewrites `.empryo/tabs.json` with the engine's tabs in bar order, so a session's own tab (the first tab of its own folder) has a known place in the bar, and Empryo's Ctrl+] / Ctrl+\ step to it. The tab's turns are then logged in the engine's session (session_chat_empryo_mirror.rs follows them there).
//! SEE-ALSO: server/src/session_chat_composer_input.rs `empryo_tab_bar`, server/src/session_chat_send/steps.rs (`SelectEmpryoTab`), server/src/zmx/provider.rs (the start that selects it), proxysoul/Empryo#236.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::domain::DomainRepository;
use crate::session_chat_send::{
    capture_session_terminal_text_vt, execute_session_chat_send, write_session_chat_payload,
    SessionChatSendStep,
};

/// Empryo's next-tab and previous-tab keys (Ctrl+] and Ctrl+\).
const EMPRYO_NEXT_TAB: &str = "\u{1d}";
const EMPRYO_PREVIOUS_TAB: &str = "\u{1c}";
/// How long one press may take to repaint the bar.
const EMPRYO_TAB_POLL: Duration = Duration::from_millis(150);
/// How long a send waits for the bar to show its tab before it is held.
pub(crate) const EMPRYO_SEND_TAB_WAIT_MS: u64 = 5_000;
/// A started window draws its input box before it has joined the engine and named its tab in
/// `tabs.json`, so the selection after a start waits longer.
const EMPRYO_START_TAB_WAIT_MS: u64 = 30_000;
/// How long a started Empryo may take to draw its input box, polled every half second.
const EMPRYO_START_WAIT: Duration = Duration::from_secs(60);
const EMPRYO_START_POLL: Duration = Duration::from_millis(500);
/// How long a window that joined another engine may show its input box without a tab bar: it
/// draws the bar a moment after the box, so one still missing then never opened its own tab.
const EMPRYO_JOIN_BAR_GRACE: Duration = Duration::from_secs(5);

const EMPRYO_OTHER_TAB_MESSAGE: &str =
    "Empryo is showing another session's tab, so nothing was sent.";

/// The tab ids a session folder's `meta.json` lists, in order.
fn tab_ids(folder: &Path) -> Vec<String> {
    let meta = std::fs::read(folder.join("meta.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    meta.as_ref()
        .and_then(|meta| meta.get("tabs"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|tab| tab.get("id").and_then(Value::as_str).map(str::to_string))
        .collect()
}

/// A session's own tab: the first tab of its own folder, the one its seed or Empryo wrote.
fn own_tab(session_log: &Path) -> Option<String> {
    tab_ids(session_log.parent()?).into_iter().next()
}

/// The place of `tab` in its window's tab bar, as `(position, tab count)`, from the
/// `.empryo/tabs.json` every window rewrites in bar order whenever it holds two or more tabs.
/// `None` while that list does not name the tab yet.
fn tab_place(session_log: &Path, tab: &str) -> Option<(usize, usize)> {
    let ids = bar_ids(session_log)?;
    Some((ids.iter().position(|id| id == tab)?, ids.len()))
}

/// `.empryo/tabs.json` beside a session folder: the engine's tab ids in bar order.
fn tabs_json(session_log: &Path) -> Option<PathBuf> {
    Some(session_log.parent()?.parent()?.parent()?.join("tabs.json"))
}

fn bar_ids(session_log: &Path) -> Option<Vec<String>> {
    let bar: Value = serde_json::from_slice(&std::fs::read(tabs_json(session_log)?).ok()?).ok()?;
    Some(
        bar.as_array()?
            .iter()
            .filter_map(|tab| tab.get("id").and_then(Value::as_str).map(str::to_string))
            .collect(),
    )
}

/// Whether the session's Empryo runs the engine it talks to (`empryo engine --engine` under its
/// window), which is the window that never draws the tabs other windows add. `None` when the
/// process table cannot be read.
async fn window_runs_own_engine(zmx_name: &str) -> Option<bool> {
    #[cfg(unix)]
    {
        let zmx_name = zmx_name.to_string();
        tokio::task::spawn_blocking(move || {
            let table = crate::zmx::read_process_snapshot().ok()?;
            Some(crate::zmx::zmx_session_runs(&table, &zmx_name, |command| {
                let mut words = command.split_whitespace();
                words
                    .next()
                    .is_some_and(|program| program.ends_with("empryo"))
                    && words.any(|word| word == "--engine")
            }))
        })
        .await
        .ok()
        .flatten()
    }
    #[cfg(not(unix))]
    {
        let _ = zmx_name;
        None
    }
}

/// The `session.jsonl` of a Ghostex session's Empryo, read from the state database.
fn empryo_session_log_for(project_id: &str, session_id: &str) -> Option<PathBuf> {
    let paths = crate::paths::get_gxserver_paths(None);
    let db = crate::session_chat_hermes::open_read_only_state_db(&paths.state_db_file)?;
    let repository = DomainRepository::new(&db, "");
    let session = repository.get_session(project_id, session_id).ok()??;
    crate::session_chat_pi_models::empryo_session_log(&repository, &session)
}

/// Makes the window show the session's own tab, stepping through the bar one tab at a time and
/// reading it back after each press. Errors when the bar never confirms the tab within `wait_ms`,
/// so the send that asked for it is held instead of typed into another session's conversation.
pub(crate) async fn select_empryo_own_tab(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    wait_ms: u64,
) -> Result<(), String> {
    let Some((log, tab)) = empryo_session_log_for(project_id, session_id)
        .and_then(|log| own_tab(&log).map(|tab| (log, tab)))
    else {
        return Ok(());
    };
    let deadline = Instant::now() + Duration::from_millis(wait_ms);
    // Read once: which engine a window talks to does not change while it runs.
    let mut own_engine: Option<Option<bool>> = None;
    loop {
        let bar = capture_session_terminal_text_vt(zmx_name)
            .await
            .map(|screen| crate::session_chat_composer::empryo_tab_bar(&screen));
        match (tab_place(&log, &tab), bar) {
            // No tab bar: the window shows only this session's tab when it runs its own engine
            // (that window never draws the tabs other windows add). A window that joined another
            // engine shows that engine's tab until its bar is up, so its send waits for the bar.
            (_, Some(None)) => {
                if own_engine.is_none() {
                    own_engine = Some(window_runs_own_engine(zmx_name).await);
                }
                if own_engine.flatten() != Some(false) {
                    return Ok(());
                }
            }
            (Some((target, count)), Some(Some((shown, Some(active))))) if shown == count => {
                let key = match target.cmp(&active) {
                    Ordering::Equal => return Ok(()),
                    Ordering::Greater => EMPRYO_NEXT_TAB,
                    Ordering::Less => EMPRYO_PREVIOUS_TAB,
                };
                write_session_chat_payload(project_id, session_id, zmx_name, source, key).await?;
            }
            _ => {}
        }
        if Instant::now() >= deadline {
            return Err(EMPRYO_OTHER_TAB_MESSAGE.to_string());
        }
        tokio::time::sleep(EMPRYO_TAB_POLL).await;
    }
}

/// After an Empryo session is created, or a resume, fork or wake starts its terminal (the desktop
/// app starts a new session's terminal itself, so creation counts as a start): waits for Empryo's
/// input box, then runs the tab selection in the session's send queue, ahead of whatever the chat
/// sends next, so the window shows this session's own conversation as soon as it is up.
///
/// CDXC:SessionChat 2026-10-07 WHY:
/// Empryo 3.9.1-beta sometimes joins a window to the running engine without opening the window's own tab (two threads started a second apart; seen live 2026-10-07): the window shows only the engine's first tab, so its brief went into the coordinator's conversation. Such a window has no tab bar although it runs no engine of its own; with `state` (a create) it is restarted once through sleep and wake, the cycle the sidebar uses, and the wake selects the tab again. Without one its sends stay held.
pub(crate) fn select_empryo_own_tab_after_start(
    session: &Value,
    state: Option<crate::server::AppState>,
) {
    if crate::session_chat_follower::session_chat_agent_for_session(session).as_deref()
        != Some("empryo")
    {
        return;
    }
    let text = |key: &str| session.get(key).and_then(Value::as_str).map(str::to_string);
    let (Some(project_id), Some(session_id), Ok(runtime)) = (
        text("projectId"),
        text("sessionId"),
        tokio::runtime::Handle::try_current(),
    ) else {
        return;
    };
    let Ok(zmx_name) = crate::zmx::provider_zmx_session_name(session) else {
        return;
    };
    runtime.spawn(async move {
        let deadline = Instant::now() + EMPRYO_START_WAIT;
        let mut box_since: Option<Instant> = None;
        while Instant::now() < deadline {
            let screen = capture_session_terminal_text_vt(&zmx_name).await;
            let input_box = screen.as_deref().is_some_and(|screen| {
                crate::session_chat_composer::empryo_composer_busy(screen).is_some()
            });
            if !input_box {
                box_since = None;
                tokio::time::sleep(EMPRYO_START_POLL).await;
                continue;
            }
            let since = *box_since.get_or_insert_with(Instant::now);
            let bar = screen.as_deref().is_some_and(|screen| {
                crate::session_chat_composer::empryo_tab_bar(screen).is_some()
            });
            if bar || window_runs_own_engine(&zmx_name).await != Some(false) {
                if let Err(error) = execute_session_chat_send(
                    &project_id,
                    &session_id,
                    &zmx_name,
                    "session-chat-empryo-tab",
                    vec![SessionChatSendStep::SelectEmpryoTab {
                        wait_ms: EMPRYO_START_TAB_WAIT_MS,
                    }],
                )
                .await
                {
                    record(
                        &project_id,
                        &session_id,
                        "sessionChatEmpryoTabNotSelected",
                        &error.message,
                    );
                }
                return;
            }
            if since.elapsed() >= EMPRYO_JOIN_BAR_GRACE {
                let Some(state) = state else {
                    record(
                        &project_id,
                        &session_id,
                        "sessionChatEmpryoTabMissing",
                        EMPRYO_OTHER_TAB_MESSAGE,
                    );
                    return;
                };
                record(
                    &project_id,
                    &session_id,
                    "sessionChatEmpryoTabRejoin",
                    "Empryo joined the running engine without this session's tab; restarting it.",
                );
                let restarted = restart_session(state, &project_id, &session_id).await;
                if let Err(reason) = restarted {
                    record(
                        &project_id,
                        &session_id,
                        "sessionChatEmpryoTabMissing",
                        &reason,
                    );
                }
                return;
            }
            tokio::time::sleep(EMPRYO_START_POLL).await;
        }
    });
}

/// Puts the session to sleep and wakes it, the cycle the sidebar uses, so its Empryo starts again.
async fn restart_session(
    state: crate::server::AppState,
    project_id: &str,
    session_id: &str,
) -> Result<(), String> {
    let (project_id, session_id) = (project_id.to_string(), session_id.to_string());
    tokio::task::spawn_blocking(move || {
        let db = crate::storage::open_gxserver_database(&state.paths)
            .map_err(|error| error.to_string())?;
        let repository = DomainRepository::new(&db, &state.metadata.server_id);
        let row = repository
            .get_session(&project_id, &session_id)
            .map_err(|error| error.message)?
            .ok_or_else(|| "The session no longer exists.".to_string())?;
        for path in ["/api/sleepSession", "/api/wakeSession"] {
            crate::accounts::endpoint::cycle(&state, &repository, &row, path)
                .map_err(|error| error.message)?;
        }
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}

fn record(project_id: &str, session_id: &str, event: &str, reason: &str) {
    crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
        event,
        project_id,
        session_id,
        reason,
        &[],
    );
}

/// What a tab scan depends on: the sessions folder's and `tabs.json`'s modification times. A new
/// session folder, or a window joining or switching tabs, changes one of them.
pub(crate) fn empryo_tab_scan_key(
    session_log: &Path,
) -> (Option<std::time::SystemTime>, Option<std::time::SystemTime>) {
    let modified = |path: Option<&Path>| path?.metadata().ok()?.modified().ok();
    (
        modified(session_log.parent().and_then(Path::parent)),
        modified(tabs_json(session_log).as_deref()),
    )
}

/// One pass over the session folders beside `session_log`'s: the `session.jsonl` that logs this
/// session's own tab now, and the tabs other Ghostex sessions own (every other folder's first tab),
/// which that log can hold too. While `.empryo/tabs.json` lists the tab, the log is the one of the
/// engine's own window, whose tab the list starts with (a joined window's tab reaches that
/// session's `meta.json` only minutes later); otherwise it is the most recently written log among
/// the folders that list the tab.
pub(crate) fn empryo_tab_scan(session_log: &Path) -> Option<(PathBuf, HashSet<String>)> {
    let own = session_log.parent()?;
    let tab = own_tab(session_log)?;
    let engine_tab = bar_ids(session_log)
        .filter(|ids| ids.contains(&tab))
        .and_then(|ids| ids.into_iter().next());
    let mut engine_log = None;
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    let mut foreign = HashSet::new();
    for folder in std::fs::read_dir(own.parent()?)
        .ok()?
        .filter_map(Result::ok)
    {
        let folder = folder.path();
        let tabs = tab_ids(&folder);
        let log = folder.join("session.jsonl");
        if engine_tab.is_some() && tabs.first() == engine_tab.as_ref() {
            engine_log = Some(log.clone());
        }
        if folder != own {
            foreign.extend(tabs.first().cloned());
        }
        let modified = log.metadata().and_then(|meta| meta.modified());
        if let (true, Ok(modified)) = (tabs.contains(&tab), modified) {
            if newest.as_ref().is_none_or(|(time, _)| modified > *time) {
                newest = Some((modified, log));
            }
        }
    }
    let host = engine_log
        .or(newest.map(|(_, log)| log))
        .unwrap_or_else(|| session_log.to_path_buf());
    Some((host, foreign))
}
