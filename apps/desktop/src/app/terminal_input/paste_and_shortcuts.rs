//! Pasting text and images into the focused terminal, the paste confirmation dialog, terminal toolbar hotkeys and prompt editor shortcuts.

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::ClipboardItem;
use gpui::Window;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn paste_into_focused_terminal_from_clipboard(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:Clipboard 2026-06-23-09:59:
        Cmd+V terminal paste is scoped to the shell focus model instead of the body mouse handlers: only the currently focused mounted Agents or command Ghostty surface can receive clipboard bytes. Clipboard contents stay ephemeral, explicit-string-only, and are never logged, persisted, or converted from file paths.

        CDXC:Clipboard 2026-06-27-10:28:
        Direct GPUI terminal paste uses the same runtime-only previewable-image setting as macOS before targeting the focused mounted Ghostty surface. Disabled keeps explicit-string-only behavior; enabled converts only validated image file references or raw image bytes into Markdown before any terminal insertion.
        */
        let Some(item) = cx.read_from_clipboard() else {
            return false;
        };
        let paste_previewable_images_enabled =
            shared_settings::shared_sidebar_settings_snapshot().terminal_paste_previewable_images();
        if paste_previewable_images_enabled
            && self.paste_clipboard_image_into_focused_remote_terminal(&item, cx)
        {
            return true;
        }
        let Some(text) = terminal_clipboard_paste_text(
            &item,
            paste_previewable_images_enabled,
            self.focused_terminal_is_factory_droid(),
        ) else {
            return false;
        };

        self.paste_text_into_focused_terminal_surface(&text, cx)
    }

    pub(crate) fn paste_image_or_send_control_v(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        let enabled =
            shared_settings::shared_sidebar_settings_snapshot().terminal_paste_previewable_images();
        if enabled && let Some(item) = cx.read_from_clipboard() {
            if self.paste_clipboard_image_into_focused_remote_terminal(&item, cx) {
                return true;
            }
            if let Some(markdown) = terminal_clipboard_previewable_image_markdown_text(&item) {
                let text = if self.focused_terminal_is_factory_droid() {
                    format!("  {markdown}")
                } else {
                    markdown
                };
                return self.paste_text_into_focused_terminal_surface(&text, cx);
            }
        }
        self.send_text_to_focused_terminal_surface("\u{16}", cx)
    }

    /*
    CDXC:Clipboard 2026-08-21:
    A remote session's terminal runs on the remote machine, so the local
    "[Image #N](path)" reference a clipboard image normally produces names a
    file the remote agent cannot open. Pasting into a remote terminal therefore
    takes the same route the Attach File or Folder button already takes: stage
    the clipboard payload, upload it over that machine's SSH connection, and
    paste the returned remote path. Ownership of the paste is claimed only once
    a remote image destination is proven (focused remote-attached session, an
    accepted clipboard image, and a reachable remote machine); every other
    clipboard shape falls through to the unchanged local paste path.
    */
    pub(crate) fn paste_clipboard_image_into_focused_remote_terminal(
        &mut self,
        item: &ClipboardItem,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(session_id) = self.focused_terminal_shell_session_id() else {
            return false;
        };
        let Some(remote_machine_id) = self.remote_machine_id_for_attached_shell_session(session_id)
        else {
            return false;
        };
        let Some(payload) = terminal_clipboard_image_payload(item) else {
            return false;
        };

        // Runtime IDs are project-local, so async uploads also retain the
        // originating viewer identity before they can paste their result.
        let runtime_session_id = self
            .agents_gpui_engine_terminals
            .get(&session_id)
            .map(|record| record.runtime_session_id);
        let originating_view_id = self
            .agents_gpui_engine_terminals
            .get(&session_id)
            .map(|record| record.view.entity_id());
        let target = GpuiEngineTerminalEventTarget::Agents(session_id);
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        let Some(config) =
            gpui_remote_machine_config_from_settings(settings.object(), remote_machine_id.as_str())
        else {
            self.dispatch_gpui_workspace_action_toast(
                "warning",
                "Image paste unavailable",
                "The saved remote machine is missing required SSH settings.",
                cx,
            );
            return true;
        };
        let Some(remote_target) = self.gpui_remote_gxserver_request_target(&remote_machine_id)
        else {
            self.dispatch_gpui_workspace_action_toast(
                "warning",
                "Image paste unavailable",
                "Reconnect the remote machine before pasting an image.",
                cx,
            );
            return true;
        };

        let pad_reference = self.focused_terminal_is_factory_droid();
        self.dispatch_gpui_workspace_action_toast(
            "info",
            "Uploading image",
            "Uploading the pasted image to the remote machine.",
            cx,
        );
        let background = cx.background_executor().clone();
        let viewer_lease = self
            .agents_gpui_engine_terminals
            .get(&session_id)
            .map(|record| record.pin_viewer());
        cx.spawn(async move |this, cx| {
            let _viewer_lease = viewer_lease;
            let result = background
                .spawn(async move {
                    gpui_upload_terminal_clipboard_image_to_remote(
                        &config,
                        &remote_target.execution_target,
                        payload,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let destination_is_live = match runtime_session_id {
                    Some(runtime_session_id) => {
                        this.gpui_engine_terminal_target_matches_runtime(target, runtime_session_id)
                            && originating_view_id.is_some_and(|view_id| {
                                this.gpui_terminal_viewer_matches_entity(target, view_id)
                            })
                    }
                    None => this.focused_terminal_shell_session_id() == Some(session_id),
                };
                if !destination_is_live {
                    return;
                }
                match result {
                    Ok(references) => {
                        let markdown = gpui_terminal_attachment_markdown_text(&references);
                        let text = if pad_reference {
                            format!("  {markdown}")
                        } else {
                            markdown
                        };
                        match runtime_session_id {
                            Some(runtime_session_id) => {
                                this.paste_text_into_gpui_engine_terminal_target(
                                    target,
                                    runtime_session_id,
                                    text.as_str(),
                                    cx,
                                );
                            }
                            None => {
                                this.paste_text_into_focused_terminal_surface(text.as_str(), cx);
                            }
                        }
                    }
                    Err(message) => this.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Image upload failed",
                        message.as_str(),
                        cx,
                    ),
                }
            });
        })
        .detach();
        true
    }

    pub(crate) fn focused_terminal_shell_session_id(&self) -> Option<TerminalSessionId> {
        match focused_terminal_text_target(self.active_mode, self.shell_focus) {
            Some(FocusedTerminalTextTarget::Agents) => focused_agents_terminal_surface_mount_slot(
                self.active_mode,
                self.shell_focus,
                &self.agents_workspace,
            )
            .map(|slot| slot.session_id),
            _ => None,
        }
    }

    pub(crate) fn focused_terminal_is_factory_droid(&self) -> bool {
        self.focused_terminal_shell_session_id()
            .and_then(|session_id| self.agents_workspace.session(session_id))
            .and_then(|session| session.agent_icon)
            == Some("factory-droid")
    }

    pub(crate) fn remote_machine_id_for_attached_shell_session(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<String> {
        self.remote_attach_sessions
            .iter()
            .find_map(|(key, mapped_session_id)| {
                (*mapped_session_id == session_id).then(|| key.remote_machine_id.clone())
            })
    }

    pub(crate) fn paste_text_into_focused_terminal_surface(
        &mut self,
        text: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        // GPUI-engine terminals get real paste semantics (bracketed-paste
        // aware, unsafe bytes stripped) instead of ghostty text insertion.
        if let Some(view) = self.focused_gpui_engine_terminal_view() {
            let paste_protection = shared_settings::shared_sidebar_settings_snapshot()
                .terminal_clipboard_paste_protection();
            if paste_protection
                && view.update(cx, |view, _cx| view.paste_requires_confirmation(text))
            {
                if self.pending_terminal_paste_confirmation.is_none() {
                    self.pending_terminal_paste_confirmation =
                        Some(PendingGpuiTerminalPasteConfirmation {
                            text: text.to_string(),
                            view,
                        });
                    self.open_terminal_paste_confirmation_modal(cx);
                    cx.notify();
                }
                return true;
            }
            view.update(cx, |view, cx| view.paste_text(text, cx));
            return true;
        }
        self.send_text_to_focused_terminal_surface(text, cx)
    }

    /// Asks before a paste that could run commands, in a native app-modal window
    /// (window/terminal_paste_confirm_modal.rs) so CEF views cannot cover it.
    fn open_terminal_paste_confirmation_modal(&mut self, cx: &mut gpui::Context<Self>) {
        let palette = self.gpui_native_modal_palette();
        let host = self.native_app_modal_host(cx, |app, command, cx| {
            app.handle_terminal_paste_confirmation_modal_command(command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::TerminalPasteConfirm,
            TERMINAL_PASTE_CONFIRM_MODAL_WIDTH,
            TERMINAL_PASTE_CONFIRM_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiTerminalPasteConfirmModalWindow::new(palette, host, window, cx))
            },
            cx,
        );
        if self.native_app_modal_kind() == Some(GpuiAppModalKind::TerminalPasteConfirm) {
            self.terminal_paste_confirmation_dialog_open = true;
        } else {
            self.pending_terminal_paste_confirmation = None;
        }
    }

    fn handle_terminal_paste_confirmation_modal_command(
        &mut self,
        command: TerminalPasteConfirmModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        let pending = self.pending_terminal_paste_confirmation.take();
        self.terminal_paste_confirmation_dialog_open = false;
        self.release_native_app_modal_window(GpuiAppModalKind::TerminalPasteConfirm, cx);
        if let (TerminalPasteConfirmModalCommand::Paste, Some(pending)) = (command, pending) {
            pending
                .view
                .update(cx, |view, cx| view.paste_text(&pending.text, cx));
        }
        cx.notify();
    }

    pub(crate) fn focused_gpui_engine_terminal_action_target(
        &self,
    ) -> Option<(
        GpuiEngineTerminalEventTarget,
        AgentsTerminalRuntimeSessionId,
    )> {
        match self.focused_terminal_text_mount_target()? {
            FocusedTerminalTextMountTarget::Agents(slot_id) => {
                let record = self.agents_gpui_engine_terminals.get(&slot_id.session_id)?;
                Some((
                    GpuiEngineTerminalEventTarget::Agents(slot_id.session_id),
                    record.runtime_session_id,
                ))
            }
            FocusedTerminalTextMountTarget::Command(slot_id) => {
                let record = self
                    .command_gpui_engine_terminals
                    .get(&slot_id.session_id)?;
                Some((
                    GpuiEngineTerminalEventTarget::Command(slot_id.session_id),
                    record.runtime_session_id,
                ))
            }
        }
    }

    pub(crate) fn run_gpui_terminal_toolbar_hotkey_action(
        &mut self,
        action_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !matches!(
            action_id,
            "promptEditor"
                | "attachFileOrFolder"
                | "exportTranscript"
                | "sessionNote"
                | "stashPrompt"
                | "stashedPrompts"
                | "toggleAgentActions"
                | "scrollTerminalToTop"
                | "scrollTerminalToBottom"
        ) {
            return false;
        }
        if action_id == "sessionNote" {
            if let Some(session_id) = self.focused_agents_or_companion_shell_session_id() {
                let _ = self.dispatch_gpui_workspace_terminal_runtime_action(
                    "openSessionNote",
                    session_id,
                    cx,
                );
            }
            return true;
        }
        if action_id == "promptEditor" {
            if let Some((target, runtime_session_id)) =
                self.focused_gpui_engine_terminal_action_target()
            {
                self.handle_gpui_engine_prompt_editor_shortcut(target, runtime_session_id, cx);
            } else {
                #[cfg(target_os = "macos")]
                if let Some(target) = self.focused_native_terminal_prompt_editor_target() {
                    self.handle_focused_native_terminal_prompt_editor_shortcut(target, cx);
                }
            }
            return true;
        }
        let Some((target, runtime_session_id)) = self.focused_gpui_engine_terminal_action_target()
        else {
            return true;
        };
        match action_id {
            "attachFileOrFolder" => {
                if let Some(attachment_target) =
                    self.gpui_terminal_attachment_target_for_engine_target(target)
                {
                    self.request_gpui_engine_terminal_attachment_paths(
                        attachment_target,
                        runtime_session_id,
                        cx,
                    );
                }
            }
            "stashPrompt" => {
                if let GpuiEngineTerminalEventTarget::Agents(session_id) = target {
                    self.request_gpui_stash_prompt_for_active_input(session_id, cx);
                }
            }
            "stashedPrompts" => {
                if matches!(target, GpuiEngineTerminalEventTarget::Agents(_)) {
                    let _ = self.open_gpui_stashed_prompts_modal_for_focused_agents_session(cx);
                }
            }
            "exportTranscript" => {
                if let GpuiEngineTerminalEventTarget::Agents(session_id) = target
                    && self.focused_agents_or_companion_shell_session_id() == Some(session_id)
                {
                    let _ = self.dispatch_gpui_workspace_terminal_runtime_action(
                        "exportTranscript",
                        session_id,
                        cx,
                    );
                }
            }
            "toggleAgentActions" => {
                if let GpuiEngineTerminalEventTarget::Agents(session_id) = target
                    && self.focused_agents_or_companion_shell_session_id() == Some(session_id)
                {
                    self.toggle_terminal_agent_action_bar_menu(session_id, cx);
                }
            }
            "scrollTerminalToTop" | "scrollTerminalToBottom" => {
                let Some(view) = self.gpui_engine_terminal_view_for_target(target) else {
                    return true;
                };
                view.update(cx, |view, cx| {
                    if action_id == "scrollTerminalToTop" {
                        view.scroll_to_top(window, cx);
                    } else {
                        view.scroll_to_bottom(window, cx);
                    }
                });
            }
            _ => {}
        }
        true
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn focused_native_terminal_prompt_editor_target(
        &self,
    ) -> Option<FocusedTerminalTextMountTarget> {
        let target = self.focused_terminal_text_mount_target()?;
        match target {
            FocusedTerminalTextMountTarget::Agents(slot_id)
                if self.agents_terminal_ghostty_surface_matches(slot_id) =>
            {
                Some(target)
            }
            FocusedTerminalTextMountTarget::Command(slot_id)
                if self.command_terminal_ghostty_surface_matches(slot_id) =>
            {
                Some(target)
            }
            _ => None,
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn handle_focused_native_terminal_prompt_editor_shortcut(
        &mut self,
        target: FocusedTerminalTextMountTarget,
        cx: &mut gpui::Context<Self>,
    ) {
        let delivery_target = RemotePromptEditorDeliveryTarget::NativeTerminal(target);
        if let Some((key, connection_generation)) =
            self.remote_prompt_editor_context_for_delivery_target(delivery_target)
        {
            cx.spawn(async move |this, cx| {
                let _ = this.update_in(cx, |this, window, cx| {
                    this.queue_remote_prompt_editor_request(
                        &key,
                        connection_generation,
                        delivery_target,
                        window,
                        cx,
                    );
                });
            })
            .detach();
            return;
        }
        let originating_session_id = match target {
            FocusedTerminalTextMountTarget::Agents(slot_id) => self
                .local_workspace_session_mappings
                .iter()
                .find_map(|(key, mapped_session_id)| {
                    (*mapped_session_id == slot_id.session_id)
                        .then(|| format!("{}:{}", key.project_id, key.session_id))
                }),
            FocusedTerminalTextMountTarget::Command(slot_id) => self
                .command_gxserver_session_mappings
                .get(&slot_id.session_id)
                .map(|key| format!("{}:{}", key.project_id, key.session_id)),
        };
        let Some(originating_session_id) = originating_session_id else {
            let _ = self.send_prompt_editor_shortcut_to_native_terminal_target(target);
            return;
        };
        cx.spawn(async move |this, cx| {
            let fronted = cx
                .background_executor()
                .spawn(
                    async move { gpui_ghostex_editor_daemon_front(Some(&originating_session_id)) },
                )
                .await;
            let _ = this.update(cx, |this, cx| {
                if fronted {
                    if !this.prompt_editor_daemon_open {
                        this.prompt_editor_daemon_open = true;
                        cx.notify();
                    }
                } else {
                    let _ = this.send_prompt_editor_shortcut_to_native_terminal_target(target);
                }
            });
        })
        .detach();
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn send_prompt_editor_shortcut_to_native_terminal_target(
        &mut self,
        target: FocusedTerminalTextMountTarget,
    ) -> bool {
        match target {
            FocusedTerminalTextMountTarget::Agents(slot_id) => {
                self.send_text_bytes_to_mounted_agents_terminal_surface(slot_id, b"\x07")
            }
            FocusedTerminalTextMountTarget::Command(slot_id) => {
                if !self
                    .command_pane
                    .is_current_terminal_body_mount_slot(slot_id)
                {
                    return false;
                }
                let runtime_session_id = command_terminal_runtime_session_id(slot_id);
                let Some(surface) = self.command_terminal_ghostty_surfaces.get_mut(&slot_id) else {
                    return false;
                };
                if surface.mount_slot_id() != slot_id
                    || surface.runtime_session_id() != runtime_session_id
                {
                    return false;
                }
                surface.send_text_bytes(b"\x07");
                true
            }
        }
    }
}
