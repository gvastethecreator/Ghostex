//! Work mode's sidebar commands: a project's Work Mode switch, and a session card's work chip.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_menu/project.rs (the Work Mode row),
//! apps/desktop/src/app/native_sidebar/work_chips.rs (the chips), server/src/work_mode/ (the
//! `/api/setProjectWorkMode` answer and the links the chips show).

use serde_json::Value;

use super::rpc::gxserver_rpc_result_task;
use crate::GhostexGpuiApp;
use crate::app::model::{GpuiBrowserRendererOpenReuse, GpuiSidebarOpenBrowserUrlMessage};

impl GhostexGpuiApp {
    /// Answers `openWorkLink` (a chip's click, at the top level) and the wrapped
    /// `setProjectWorkMode` (the project menu's Work Mode row).
    pub(crate) fn gx_store_run_work_mode(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        match command.get("type").and_then(Value::as_str) {
            Some("openWorkLink") => {
                let Some(url) = command
                    .get("url")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
                    .map(str::to_string)
                else {
                    return true;
                };
                let message = GpuiSidebarOpenBrowserUrlMessage {
                    url,
                    reuse: GpuiBrowserRendererOpenReuse::Exact,
                    from_quick_header: false,
                    project_id: None,
                };
                self.defer_in_main_window(cx, move |this, window, cx| {
                    if !this.project_switch_pending_requests.is_empty() {
                        this.flush_coalesced_project_switch_requests(window, cx);
                    }
                    this.open_browser_url_from_renderer_command(message, window, cx);
                });
                true
            }
            Some("command")
                if command["message"].get("type").and_then(Value::as_str)
                    == Some("setProjectWorkMode") =>
            {
                let message = &command["message"];
                let (Some(project_id), Some(enabled)) = (
                    message.get("projectId").and_then(Value::as_str),
                    message.get("enabled").and_then(Value::as_bool),
                ) else {
                    return true;
                };
                let params = serde_json::json!({ "projectId": project_id, "enabled": enabled });
                let background = cx.background_executor().clone();
                cx.spawn(async move |this, cx| {
                    let result = gxserver_rpc_result_task(
                        &background,
                        "/api/setProjectWorkMode",
                        params,
                        super::sidebar_lifecycle::rpc_timeout(),
                    )
                    .await;
                    if let Err(message) = result {
                        let _ = this.update(cx, |this, cx| {
                            this.dispatch_gpui_workspace_action_toast(
                                "error",
                                "Couldn't change Work Mode",
                                &message,
                                cx,
                            );
                        });
                    }
                })
                .detach();
                true
            }
            _ => false,
        }
    }
}
