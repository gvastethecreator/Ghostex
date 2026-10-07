//! The Session resume hooks card at the bottom of the Agents page: what hooks do, Install all
//! (for the agents that are on), Uninstall all, Refresh, the hook state folder, and whether a
//! hook installs without asking when an agent is turned on.
//!
//! CDXC:AgentHooks 2026-10-06 DECISION:
//! User: "ok implement the plan" for the Agents page redesign. The bulk hook tools leave the top of the agent list for this card at the bottom; the list keeps only a summary line (counting agents that are on) with Fix all. This replaces the 2026-08-28 roster toolbar ("quiet whole-set controls, a readiness chip and an info tooltip"), because a "3/23 hooks ready" count of every supported agent named agents the user never turned on.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    ButtonSize, ButtonVariant, RowSpec, card_inset, settings_button_sized, settings_section,
    toggle_field,
};
use super::super::super::palette::SettingsPalette;
use super::super::super::search::{TabSearch, should_show_setting};
use super::AgentsTab;
use super::icons;
use super::model::{HookStatus, any_hook_removable, hook_agent_id};
use super::turn_on::AUTO_INSTALL_HOOKS;
use gpui::{
    AnyElement, Context, Div, ParentElement as _, SharedString, Styled as _, Window, div, px,
};
use gpui_component::{h_flex, v_flex};

impl AgentsTab {
    pub(super) fn render_hooks_card(
        &mut self,
        p: &SettingsPalette,
        search: &TabSearch,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let section = search.section(super::HOOKS_ANCHOR);
        let status = self
            .store
            .read(cx)
            .host_payload("agentHookStatus")
            .map(HookStatus::parse);
        let loading = self.hook_status_loading;
        let mut on_hooks: Vec<String> = self
            .all_agents(cx)
            .iter()
            .filter(|agent| agent.enabled)
            .filter_map(hook_agent_id)
            .collect();
        on_hooks.sort();
        on_hooks.dedup();
        let mut rows: Vec<AnyElement> = Vec::new();
        if should_show_setting(&section, "agentResumeHooks", true) {
            let ready = status.as_ref().map_or(0, |status| {
                on_hooks
                    .iter()
                    .filter(|hook| {
                        status
                            .item(hook)
                            .is_some_and(|item| item.status == "installed")
                    })
                    .count()
            });
            let summary = match &status {
                Some(status) if status.error_message.is_some() => {
                    "Ghostex could not check the hooks.".to_string()
                }
                Some(_) => format!(
                    "{ready} of the {} agent CLIs you use have their hook.",
                    on_hooks.len()
                ),
                None if loading => "Checking hooks…".to_string(),
                None => "Hooks not checked yet.".to_string(),
            };
            let loading_reason: SharedString = "Hook status is being checked.".into();
            let removable = any_hook_removable(status.as_ref());
            let install_ids = on_hooks.clone();
            let buttons = h_flex()
                .flex_shrink_0()
                .flex_wrap()
                .items_center()
                .justify_end()
                .gap(px(6.0))
                .child(settings_button_sized(
                    p,
                    "agents-hooks-install-all",
                    "Install all",
                    Some(icons::DOWNLOAD),
                    ButtonVariant::Ghost,
                    ButtonSize::Sm,
                    loading || install_ids.is_empty(),
                    Some(loading_reason.clone()),
                    move |page: &mut Self, _window, cx| {
                        page.install_hooks(Some(install_ids.clone()), cx);
                        cx.notify();
                    },
                    cx,
                ))
                // CDXC:AgentHooks 2026-08-19-11:20 (agents.tsx): Uninstall All sits beside the install it undoes and stays disabled while status loads or no Ghostex hook is present.
                .child(settings_button_sized(
                    p,
                    "agents-hooks-uninstall-all",
                    "Uninstall all",
                    Some(icons::TRASH),
                    ButtonVariant::Ghost,
                    ButtonSize::Sm,
                    loading || !removable,
                    Some(if loading {
                        loading_reason.clone()
                    } else {
                        "No Ghostex hooks are installed.".into()
                    }),
                    |page: &mut Self, _window, cx| {
                        page.uninstall_hooks(None, cx);
                        cx.notify();
                    },
                    cx,
                ))
                .child(settings_button_sized(
                    p,
                    "agents-hooks-refresh",
                    "Refresh",
                    Some(icons::REFRESH),
                    ButtonVariant::Ghost,
                    ButtonSize::Sm,
                    loading,
                    Some(loading_reason),
                    |page: &mut Self, _window, cx| {
                        page.request_hook_status(cx);
                        cx.notify();
                    },
                    cx,
                ));
            let small = |text: String| {
                div()
                    .min_w_0()
                    .text_size(px(12.5))
                    .line_height(px(18.0))
                    .text_color(hsla(p.muted))
                    .child(text)
            };
            let mut text = v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(20.0))
                        .text_color(hsla(p.foreground))
                        .child("Hooks let Ghostex resume the exact conversation after sleep, reload or restart."),
                )
                .child(small(summary));
            if let Some(status) = &status {
                if let Some(message) = &status.error_message {
                    text = text.child(
                        div()
                            .text_size(px(12.5))
                            .text_color(hsla(p.destructive))
                            .child(message.clone()),
                    );
                }
                if !status.hook_state_directory.is_empty() {
                    text = text.child(
                        small(format!("Hook state: {}", status.hook_state_directory))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis(),
                    );
                }
            }
            rows.push(card_inset(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(text)
                    .child(buttons),
            ));
        }
        if should_show_setting(&section, AUTO_INSTALL_HOOKS, true) {
            let checked = self.store.read(cx).values().bool(AUTO_INSTALL_HOOKS);
            rows.push(toggle_field(
                self,
                p,
                AUTO_INSTALL_HOOKS,
                RowSpec::new("Install the hook when I turn on an agent").description(
                    "Install an agent's session resume hook as soon as you turn it on, without asking.",
                ),
                checked,
                cx,
            ));
        }
        settings_section(p, "Session resume hooks", None, None, rows)
    }
}
