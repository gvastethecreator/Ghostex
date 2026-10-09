//! Open, link and close for the work-mode Link to picker, shared by the desktop and the GPUI web
//! build (symlinked there).
//! SEE-ALSO: apps/desktop/src/app/window/work_link_picker_modal.rs (the window),
//! apps/desktop/src/app/gx_store/work_mode.rs (`gx_store_set_session_work_links`, the write),
//! packages/gx-core/src/sidebar_actions/modals.rs (the `linkWork` action that opens it).
use crate::app::window::*;
use crate::*;
use ghostex_gx_core::SessionKey;

impl GhostexGpuiApp {
    /// Opens the picker for the `workLinkPicker` modal: `sessionId` (the sidebar row id),
    /// `sessionTitle` and `kind`. An open for another machine's row, or without a kind, is dropped:
    /// the submenu is only offered on this computer's work-mode rows.
    pub(crate) fn open_gpui_work_link_picker_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str| {
            message
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let Some(session) = text("sessionId")
            .as_deref()
            .and_then(SessionKey::parse_sidebar_session_id)
            .filter(|session| session.machine.is_local())
        else {
            return;
        };
        let Some(kind) = text("kind").as_deref().and_then(WorkLinkKind::parse) else {
            return;
        };
        let config = WorkLinkPickerConfig {
            project_id: session.project_id.clone(),
            session_id: session.session_id.clone(),
            session_title: text("sessionTitle").unwrap_or_else(|| "This session".to_string()),
            kind,
        };
        let (project_id, session_id) = (session.project_id, session.session_id);
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            if let WorkLinkPickerCommand::Link { links } = command {
                app.gx_store_set_session_work_links(&project_id, &session_id, links, cx);
            }
            app.release_native_app_modal_window(GpuiAppModalKind::WorkLinkPicker, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::WorkLinkPicker,
            WORK_LINK_PICKER_MODAL_WIDTH,
            WORK_LINK_PICKER_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiWorkLinkPickerModalWindow::new(config, host, window, cx))
            },
            cx,
        );
    }
}
