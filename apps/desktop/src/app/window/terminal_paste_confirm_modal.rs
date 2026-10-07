//! Native "Paste potentially unsafe text?" confirmation for terminal paste protection.
//!
//! CDXC:Clipboard 2026-10-06 WHY:
//! This was a gpui-component alert dialog drawn inside the main window, so a browser tab, the Files or Code view, or an extension view (CEF child windows that always paint above GPUI content) covered it. It opens as a native app-modal child window instead, like every other app dialog, with the same copy, buttons, Enter/Escape keys and paste/cancel outcome.
//! SEE-ALSO: apps/desktop/src/app/terminal_input/paste_and_shortcuts.rs (opens it and applies the outcome), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path; replacing the modal cancels the paste).
use super::native_modal_kit::*;
use gpui::{App, Context, FocusHandle, IntoElement, KeyDownEvent, Render, Window};
use std::rc::Rc;

pub(crate) const TERMINAL_PASTE_CONFIRM_MODAL_WIDTH: f32 = 440.0;
/// First-frame height only; the window is resized to the measured layout on the first prepaint.
pub(crate) const TERMINAL_PASTE_CONFIRM_MODAL_INITIAL_HEIGHT: f32 = 180.0;

const TITLE: &str = "Paste potentially unsafe text?";
const DESCRIPTION: &str =
    "This paste contains a newline or terminal control sequence and may run commands.";

/// What the dialog asks its host to do. The dialog removes its own window before sending either.
pub(crate) enum TerminalPasteConfirmModalCommand {
    /// "Paste" or Enter.
    Paste,
    /// "Cancel", Escape or the corner close.
    Cancel,
}

pub(crate) type TerminalPasteConfirmModalHost =
    Rc<dyn Fn(TerminalPasteConfirmModalCommand, &mut App)>;

pub(crate) struct GpuiTerminalPasteConfirmModalWindow {
    host: TerminalPasteConfirmModalHost,
    palette: ModalPalette,
    fit: ModalFit,
    focus_handle: FocusHandle,
}

impl GpuiTerminalPasteConfirmModalWindow {
    pub(crate) fn new(
        palette: ModalPalette,
        host: TerminalPasteConfirmModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            host,
            palette,
            fit: ModalFit::new(),
            focus_handle,
        }
    }

    fn close_window_and_send(
        &mut self,
        command: TerminalPasteConfirmModalCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.remove_window();
        (self.host)(command, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let command = match event.keystroke.key.as_str() {
            "enter" => TerminalPasteConfirmModalCommand::Paste,
            "escape" => TerminalPasteConfirmModalCommand::Cancel,
            _ => return,
        };
        self.close_window_and_send(command, window, cx);
        cx.stop_propagation();
    }
}

impl Render for GpuiTerminalPasteConfirmModalWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let footer = modal_footer(vec![
            modal_action_button(
                &p,
                "terminal-paste-confirm-cancel",
                "Cancel",
                None,
                ModalButtonTone::Neutral,
                false,
                |this, window, cx| {
                    this.close_window_and_send(TerminalPasteConfirmModalCommand::Cancel, window, cx)
                },
                cx,
            ),
            modal_action_button(
                &p,
                "terminal-paste-confirm-paste",
                "Paste",
                None,
                ModalButtonTone::Primary,
                false,
                |this, window, cx| {
                    this.close_window_and_send(TerminalPasteConfirmModalCommand::Paste, window, cx)
                },
                cx,
            ),
        ]);
        modal_shell(
            &p,
            "ghostex-gpui-terminal-paste-confirm-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            vec![modal_header(&p, TITLE, Some(DESCRIPTION))],
            footer,
            None,
            cx,
        )
    }
}

impl ModalCornerClose for GpuiTerminalPasteConfirmModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_window_and_send(TerminalPasteConfirmModalCommand::Cancel, window, cx);
    }
}
