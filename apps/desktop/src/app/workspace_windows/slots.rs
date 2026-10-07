//! Each workspace window's saved layout, focus and frame, kept apart in numbered slots, and the
//! list of open slots the next launch reopens.
//!
//! CDXC:AppWindows 2026-10-01 WHY:
//! Two windows writing one layout file overwrote each other's tabs and would restore one window's Delayed Sends and Commands terminals in both. Slot 0 keeps the file names a single-window install always used, so nothing moves for it; slot N adds `.window-N` before `.json`. The windows open at quit are listed in `gpui-workspace-windows.json` and all reopen at the next launch, where they were; a window the user closes while another stays open is forgotten with its files, so closing a window is how one goes away for good.
//! SEE-ALSO: apps/desktop/src/app/model/workspace_shell_state.rs (one writer per slot).

use std::cell::RefCell;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use gpui::{App, Window, WindowBounds};

use super::registry::{
    WORKSPACE_WINDOWS, open_workspace_window_slots, used_workspace_window_slots,
};
use crate::app::helpers::*;
use crate::*;

/// The most windows a launch reopens; a list longer than this was not written by a person.
const MAX_RESTORED_WINDOWS: usize = 16;
const MANIFEST_VERSION: u64 = 1;
/// Matches the single window's frame debounce (`schedule_gpui_window_frame_state_persist`).
const FRAME_PERSIST_DEBOUNCE: Duration = Duration::from_millis(750);

thread_local! {
    static FRAME_PERSIST_PENDING: RefCell<HashSet<u32>> = RefCell::new(HashSet::new());
}

/// `base` for slot 0; `<stem>.window-<slot>.json` beside it otherwise.
pub(crate) fn workspace_window_state_path(base: PathBuf, slot: u32) -> PathBuf {
    if slot == 0 {
        return base;
    }
    let stem = base
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    base.with_file_name(format!("{stem}.window-{slot}.json"))
}

fn workspace_windows_manifest_path() -> PathBuf {
    ghostex_state_root().join("gpui-workspace-windows.json")
}

fn frame_state_path(slot: u32) -> PathBuf {
    workspace_window_state_path(gpui_window_frame_state_path(), slot)
}

/// The slots open when the app last quit, the lead's first. An install that never opened a second
/// window has no list and reopens slot 0, as it always did.
pub(super) fn saved_workspace_window_slots() -> Vec<u32> {
    let saved = fs::read_to_string(workspace_windows_manifest_path())
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .filter(|value| {
            value.get("version").and_then(serde_json::Value::as_u64) == Some(MANIFEST_VERSION)
        })
        .and_then(|value| {
            let slots = value.get("slots")?.as_array()?;
            let mut seen = HashSet::new();
            Some(
                slots
                    .iter()
                    .filter_map(serde_json::Value::as_u64)
                    .filter_map(|slot| u32::try_from(slot).ok())
                    .filter(|slot| seen.insert(*slot))
                    .take(MAX_RESTORED_WINDOWS)
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_default();
    if saved.is_empty() { vec![0] } else { saved }
}

/// Writes the open windows' slots, oldest first.
pub(super) fn write_workspace_windows_manifest() {
    let path = workspace_windows_manifest_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let payload = serde_json::json!({
        "version": MANIFEST_VERSION,
        "slots": open_workspace_window_slots(),
    });
    let _ = fs::write(path, payload.to_string());
}

/// The lowest slot no open window uses, with anything an earlier window left in it cleared.
pub(super) fn allocate_workspace_window_slot() -> u32 {
    let used = used_workspace_window_slots();
    let slot = (0..).find(|slot| !used.contains(slot)).unwrap_or(0);
    discard_workspace_window_slot_files(slot);
    slot
}

/// A window the user closed while another stays open: its files go and the next launch does not
/// reopen it.
pub(super) fn forget_workspace_window_slot(slot: u32) {
    discard_workspace_window_slot_files(slot);
    write_workspace_windows_manifest();
}

fn discard_workspace_window_slot_files(slot: u32) {
    discard_gpui_workspace_shell_state(slot);
    let _ = fs::remove_file(workspace_window_state_path(
        gpui_gxserver_presentation_focus_state_path(),
        slot,
    ));
    let _ = fs::remove_file(frame_state_path(slot));
}

/// Where the slot's window was when the app last quit, kept on a display that still exists.
pub(super) fn restored_workspace_window_bounds(
    slot: u32,
    cx: &App,
) -> Option<(WindowBounds, gpui::DisplayId)> {
    if slot == 0 {
        return restored_gpui_window_bounds(cx);
    }
    restored_gpui_window_bounds_from_state(
        load_gpui_window_frame_state_file(&frame_state_path(slot))?,
        GPUI_WINDOW_FRAME_MIN_WIDTH,
        GPUI_WINDOW_FRAME_MIN_HEIGHT,
        cx,
    )
}

/// How a workspace window's frame changed since it last reported.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkspaceWindowFrameChange {
    Unchanged,
    /// Same size and state, somewhere else (the windows it owns follow it, owned_windows.rs).
    Moved,
    /// A new size, or maximized, restored or full screen.
    Resized,
}

/// Records the window's frame and says how it changed since it last reported; when it changed,
/// the slot's frame file is written after a short quiet period.
pub(super) fn note_workspace_window_frame(window: &Window, cx: &App) -> WorkspaceWindowFrameChange {
    let window_id = gpui::Window::window_handle(window).window_id();
    let frame = gpui_window_frame_state_from_window(window, cx);
    let changed = WORKSPACE_WINDOWS.with(|windows| {
        let mut windows = windows.borrow_mut();
        let entry = windows
            .iter_mut()
            .find(|entry| entry.handle.window_id() == window_id)?;
        if entry.frame == frame || entry.closing {
            return None;
        }
        let resized = match (&entry.frame, &frame) {
            (Some(before), Some(after)) => {
                before.state != after.state
                    || before.width != after.width
                    || before.height != after.height
            }
            _ => false,
        };
        entry.frame = frame;
        Some((entry.slot, resized))
    });
    let Some((slot, resized)) = changed else {
        return WorkspaceWindowFrameChange::Unchanged;
    };
    if slot == 0 {
        // Slot 0 keeps the single window's own frame persistence (helpers/os_cli).
        record_gpui_window_frame_state(window, cx);
        schedule_gpui_window_frame_state_persist(cx);
    } else if FRAME_PERSIST_PENDING.with(|pending| pending.borrow_mut().insert(slot)) {
        cx.spawn(async move |cx| {
            cx.background_executor().timer(FRAME_PERSIST_DEBOUNCE).await;
            FRAME_PERSIST_PENDING.with(|pending| pending.borrow_mut().remove(&slot));
            persist_workspace_window_slot_frame(slot);
        })
        .detach();
    }
    if resized {
        WorkspaceWindowFrameChange::Resized
    } else {
        WorkspaceWindowFrameChange::Moved
    }
}

/// Seeds slot 0's frame record when its window opens, as the single window always did.
pub(super) fn record_opened_workspace_window_frame(slot: u32, window: &Window, cx: &App) {
    if slot == 0 {
        record_gpui_window_frame_state(window, cx);
    }
}

/// Writes the frame of the open window in `slot` now (the quit path).
pub(crate) fn persist_workspace_window_slot_frame(slot: u32) {
    let frame = WORKSPACE_WINDOWS.with(|windows| {
        windows
            .borrow()
            .iter()
            .find(|entry| entry.slot == slot && !entry.closing)
            .map(|entry| entry.frame.clone())
    });
    // A slot whose window closed was forgotten; writing would bring its file back.
    if let Some(frame) = frame {
        persist_workspace_window_frame_now(slot, frame.as_ref());
    }
}

pub(super) fn persist_workspace_window_frame_now(slot: u32, frame: Option<&GpuiWindowFrameState>) {
    if slot == 0 {
        persist_gpui_window_frame_state();
    } else if let Some(frame) = frame {
        write_gpui_window_frame_state_file(&frame_state_path(slot), frame);
    }
}

impl GhostexGpuiApp {
    /// This window's sidebar focus file.
    pub(crate) fn presentation_focus_state_path(&self) -> PathBuf {
        workspace_window_state_path(
            gpui_gxserver_presentation_focus_state_path(),
            self.workspace_window_slot,
        )
    }
}
