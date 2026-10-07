//! When a popup that closes on click-away (the new-thread picker, the chat's option menus and
//! Quick Access) has really been dismissed. The other app modals never close on click-away
//! (CDXC:AppModal 2026-09-30 in app/window/modal_window_frame.rs).
//!
//! CDXC:AppModal 2026-09-30 WHY:
//! On Linux the window manager can move keyboard focus without a click: Hyprland (Omarchy's default) and other focus-follows-mouse setups focus the main window as soon as the pointer crosses onto it, so every popup that closed on focus loss closed when the mouse moved (Cmd+N's picker, the Saved Prompts Quick Access from a right-click, the chat's model menu, which flashed and vanished because the pointer is still over its trigger). Taking focus back fights the window manager on every pointer motion. So on Linux a focus loss dismisses a popup only once no Ghostex window holds focus (focus left the app), and a press in the main window after the popup opened is the click-away. macOS and Windows focus only on a click, so their focus loss stays the dismissal.
use gpui::{Context, Subscription, Window};

/// The latest pointer press in the main window, which Linux popups close on.
#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
struct MainWindowPointerPress(std::time::Instant);

#[cfg(target_os = "linux")]
impl gpui::Global for MainWindowPointerPress {}

/// How long a Linux focus loss waits for another Ghostex window to report focus before it counts
/// as focus leaving the app: the lost and the gained focus arrive as separate X11 events.
#[cfg(target_os = "linux")]
const FOCUS_SETTLE: std::time::Duration = std::time::Duration::from_millis(120);

/// Records a pointer press in the main window, closing the popups open before it (Linux).
pub(crate) fn note_main_window_pointer_press(cx: &mut gpui::App) {
    #[cfg(target_os = "linux")]
    cx.set_global(MainWindowPointerPress(std::time::Instant::now()));
    #[cfg(not(target_os = "linux"))]
    let _ = cx;
}

/// Call when a popup that closes on click-away has lost focus. Runs `dismiss` now, except on
/// Linux, where it runs only once focus has left every Ghostex window.
pub(crate) fn dismiss_on_focus_loss<T: 'static>(
    this: &mut T,
    window: &mut Window,
    cx: &mut Context<T>,
    dismiss: impl FnOnce(&mut T, &mut Window, &mut Context<T>) + 'static,
) {
    #[cfg(target_os = "linux")]
    {
        let _ = this;
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(FOCUS_SETTLE).await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !window.is_window_active() && cx.active_window().is_none() {
                    dismiss(this, window, cx);
                }
            });
        })
        .detach();
    }
    #[cfg(not(target_os = "linux"))]
    dismiss(this, window, cx);
}

/// Runs `dismiss` when the main window is pressed after this popup opened. Only Linux needs it:
/// elsewhere that press takes focus from the popup and `dismiss_on_focus_loss` covers it.
pub(crate) fn observe_main_window_press<T: 'static>(
    window: &mut Window,
    cx: &mut Context<T>,
    dismiss: impl Fn(&mut T, &mut Window, &mut Context<T>) + 'static,
) -> Subscription {
    #[cfg(target_os = "linux")]
    {
        let opened_at = std::time::Instant::now();
        cx.observe_global_in::<MainWindowPointerPress>(window, move |this, window, cx| {
            if cx.global::<MainWindowPointerPress>().0 > opened_at {
                dismiss(this, window, cx);
            }
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (window, cx, dismiss);
        Subscription::new(|| {})
    }
}
