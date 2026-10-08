//! The Faster rendering setting: GPUI Fast's view retention, where a view whose inputs did not
//! change is drawn again from the last frame instead of being rendered, laid out and painted.
//!
//! CDXC:Settings 2026-10-08 DECISION: User: "I would like it to be a toggle in settings (even if requires app restart)" for GPUI Fast's retained mode. Settings > General > Experimental > Faster rendering, off by default. It applies without a restart: windows opened later start with it, and open windows switch when it changes.
//! CDXC:Settings 2026-10-08 SEE-ALSO: `.dependencies/zed` (the GPUI Fast fork) `crates/gpui/src/fast/retained.rs` `set_default_view_retention`. The web build keeps accessibility on, under which GPUI Fast retains nothing, so the setting is the desktop's; `?retention=on` with `?a11y=off` turns it on in a web page for testing (`apps/gpui-web/src/lib.rs`).

use crate::shared_settings::SharedSidebarSettingsSnapshot;
use std::sync::atomic::{AtomicU8, Ordering};

/// What was last applied: 0 nothing yet, 1 off, 2 on.
static APPLIED: AtomicU8 = AtomicU8::new(0);

pub(crate) fn faster_rendering_enabled(settings: &SharedSidebarSettingsSnapshot) -> bool {
    settings
        .object()
        .get("fasterRendering")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
}

/// Sets what windows start with. Called before the first window opens.
pub(crate) fn apply_view_retention_default(settings: &SharedSidebarSettingsSnapshot) {
    let enabled = faster_rendering_enabled(settings);
    gpui::set_default_view_retention(enabled);
    APPLIED.store(if enabled { 2 } else { 1 }, Ordering::Relaxed);
}

/// Applies the setting after settings changed: switches every open window when it changed, and
/// while it is on redraws every window from scratch once.
///
/// CDXC:Settings 2026-10-08 WHY: theme, glass and settings values are read from shared statics
/// during render rather than from entities, so a retained view would keep the old ones. A settings
/// change is rare, so it simply redraws everything once.
pub(crate) fn apply_view_retention_after_settings_change(
    settings: &SharedSidebarSettingsSnapshot,
    cx: &mut gpui::App,
) {
    let enabled = faster_rendering_enabled(settings);
    let switched =
        APPLIED.swap(if enabled { 2 } else { 1 }, Ordering::Relaxed) != if enabled { 2 } else { 1 };
    if switched {
        gpui::set_default_view_retention(enabled);
    }
    if !switched && !enabled {
        return;
    }
    // Deferred, so the window running this update can be updated too.
    cx.defer(move |cx| {
        for handle in cx.windows() {
            let _ = handle.update(cx, |_, window, _| {
                if switched {
                    window.set_view_retention(enabled);
                }
                window.refresh();
            });
        }
    });
}
