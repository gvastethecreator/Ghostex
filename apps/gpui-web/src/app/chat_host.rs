//! What the page does for a chat view's app-shell requests. The chat itself (its socket, storage and timers) is the desktop's Rust chat host in `app/gx_chat/`, run on the page's thread; a view only hands the app its presentation cache and the host actions a shell performs.
use ghostex_gx_core::SessionKey;
use serde_json::Value;

use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    /// A chat view's `broker` request, which is only ever the presentation cache the next view of the session opens from.
    pub(crate) fn web_relay_chat_broker(&mut self, session: &SessionKey, message: &Value) {
        if message["method"] == "presentation" {
            self.chat_presentations
                .insert(session.clone(), message["params"]["state"].clone());
        }
    }

    pub(crate) fn web_chat_host_action(
        &mut self,
        session: &SessionKey,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        match message["action"].as_str().or_else(|| message["method"].as_str()) {
            // The composer's terminal button: the same session, as a terminal.
            Some("terminalView" | "switchToTerminal") => self.web_show_terminal(true, cx),
            // A row of a coordinator's Threads panel: that thread's chat, on the same machine.
            // gxserver resumed it first when it was closed (`openCoordinatorThread`).
            Some("openCoordinatorThread") => {
                let text = |key: &str| {
                    message[key]
                        .as_str()
                        .or_else(|| message["params"][key].as_str())
                        .map(str::to_string)
                };
                if let (Some(project_id), Some(session_id)) = (text("projectId"), text("sessionId")) {
                    self.web_open_session_in_work_area(
                        SessionKey {
                            machine: session.machine.clone(),
                            project_id,
                            session_id,
                        },
                        false,
                        cx,
                    );
                }
            }
            Some("composerReady") => {}
            other => log::info!("chat host action not handled on web yet: {other:?}"),
        }
    }
}
