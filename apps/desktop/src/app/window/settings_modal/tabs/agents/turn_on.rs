//! Turning an agent on: it is on at once, and its row then offers what it still needs, inline:
//! its CLI install (with the resume hook after it) when the CLI is not on this computer, or the
//! one-time session resume hook question when the CLI is here but the hook is not.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    ButtonSize, ButtonVariant, checkbox_control, settings_button_sized, settings_icon,
};
use super::super::super::palette::SettingsPalette;
use super::AgentsTab;
use super::cli::{cli_definition, ghost_icon_button};
use super::icons;
use super::model::{AgentButton, HookStatus, hook_agent_id, hook_supported_agents};
use super::status::{amber, emerald};
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    Styled as _, Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::json;
use std::time::Duration;

/// The setting that skips the hook question.
pub(super) const AUTO_INSTALL_HOOKS: &str = "agentHooksAutoInstall";
/// How long the "Installed" note stays on a row.
const INSTALLED_NOTE: Duration = Duration::from_secs(6);

/// The inline step of a row that was just turned on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TurnOnStep {
    /// The CLI is not on this computer: offer its install.
    InstallCli,
    /// The CLI is here but the session resume hook is not: ask once.
    HookQuestion,
    /// The CLI install just finished.
    Installed,
}

impl AgentsTab {
    /// Turns `agent` on and opens the step its row still needs.
    pub(super) fn turn_on_agent(&mut self, agent: &AgentButton, cx: &mut Context<Self>) {
        self.set_agents_enabled(vec![agent.agent_id.clone()], true, cx);
        let cli_agent = self.cli_agent_id(agent);
        let hook = hook_agent_id(agent);
        let status = self
            .store
            .read(cx)
            .host_payload("agentHookStatus")
            .map(HookStatus::parse);
        let hook_status = hook
            .as_deref()
            .and_then(|hook| status.as_ref().and_then(|status| status.item(hook)))
            .map(|item| item.status.clone());
        if cli_definition(&cli_agent).is_some()
            && self.cli_missing(&cli_agent, hook_status.as_deref())
        {
            self.turn_on
                .insert(agent.agent_id.clone(), TurnOnStep::InstallCli);
            if hook.is_some() {
                self.hook_after_cli.insert(cli_agent);
            }
            return;
        }
        let Some(hook) = hook else {
            return;
        };
        if matches!(
            hook_status.as_deref(),
            None | Some("installed" | "notRequired" | "cliMissing")
        ) {
            return;
        }
        if self.store.read(cx).values().bool(AUTO_INSTALL_HOOKS) {
            self.install_hooks(Some(vec![hook]), cx);
        } else {
            self.turn_on
                .insert(agent.agent_id.clone(), TurnOnStep::HookQuestion);
        }
    }

    /// A CLI install finished: rows waiting on it show "Installed", and its hook installs when
    /// the user asked for it.
    pub(super) fn on_cli_installed(&mut self, cli_agent: &str, cx: &mut Context<Self>) {
        let agents = self.all_agents(cx);
        let waiting: Vec<String> = agents
            .iter()
            .filter(|agent| {
                self.turn_on.get(&agent.agent_id) == Some(&TurnOnStep::InstallCli)
                    && self.cli_agent_id(agent) == cli_agent
            })
            .map(|agent| agent.agent_id.clone())
            .collect();
        for agent_id in &waiting {
            self.turn_on.insert(agent_id.clone(), TurnOnStep::Installed);
        }
        if !waiting.is_empty() {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(INSTALLED_NOTE).await;
                let _ = this.update(cx, |page, cx| {
                    for agent_id in &waiting {
                        if page.turn_on.get(agent_id) == Some(&TurnOnStep::Installed) {
                            page.turn_on.remove(agent_id);
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        if self.hook_after_cli.remove(cli_agent)
            && hook_supported_agents()
                .iter()
                .any(|agent| agent.agent_id == cli_agent)
        {
            self.install_hooks(Some(vec![cli_agent.to_string()]), cx);
        }
        cx.notify();
    }

    /// The step under a row that was just turned on, if it has one.
    pub(super) fn render_turn_on_step(
        &mut self,
        p: &SettingsPalette,
        agent: &AgentButton,
        expanded: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let step = *self.turn_on.get(&agent.agent_id)?;
        if !agent.enabled {
            return None;
        }
        let agent_id = agent.agent_id.clone();
        let name = agent.name.clone();
        let cli_agent = self.cli_agent_id(agent);
        let dismiss_agent = agent_id.clone();
        let dismiss = ghost_icon_button(
            p,
            SharedString::from(format!("agent-turn-on-dismiss-{agent_id}")),
            settings_icon(icons::X, 14.0, p.muted).into_any_element(),
            false,
            false,
            move |page: &mut Self, _window, cx| {
                page.turn_on.remove(&dismiss_agent);
                cx.notify();
            },
            cx,
        );
        let title = |text: String| {
            div()
                .text_size(px(13.5))
                .line_height(px(18.0))
                .text_color(hsla(p.foreground))
                .child(text)
        };
        let note = |text: &str| {
            div()
                .text_size(px(12.5))
                .line_height(px(17.0))
                .text_color(hsla(p.muted))
                .child(text.to_string())
        };
        let (icon_path, icon_color, border) = match step {
            TurnOnStep::InstallCli => {
                let (_, icon) = amber(p);
                (icons::ALERT_TRIANGLE, icon, css_fade(icon, 0.3))
            }
            TurnOnStep::HookQuestion => (icons::INFO_CIRCLE, p.settings_accent, p.hairline),
            TurnOnStep::Installed => {
                let (_, icon) = emerald(p);
                (icons::CIRCLE_CHECK_FILLED, icon, p.hairline)
            }
        };
        let mut body = v_flex().flex_1().min_w_0().gap(px(4.0));
        match step {
            TurnOnStep::InstallCli => {
                body = body
                    .child(title(format!("{name} isn't installed on this computer")))
                    .child(note(
                        "Ghostex can run the CLI's own installer. You can also install it yourself; the row updates once Ghostex finds it.",
                    ));
                if expanded {
                    body = body.child(note("Its CLI controls are in the open row below."));
                } else if let Some(controls) = self.render_cli_controls(p, &cli_agent, window, cx) {
                    body = body.child(controls);
                }
                if hook_agent_id(agent).is_some() {
                    let checked = self.hook_after_cli.contains(&cli_agent);
                    let toggle_cli = cli_agent.clone();
                    body = body.child(
                        div().pt(px(8.0)).child(checkbox_control(
                            p,
                            SharedString::from(format!("agent-hook-after-cli-{agent_id}")),
                            checked,
                            Some(
                                note("Turn on its session resume hook when it's installed")
                                    .into_any_element(),
                            ),
                            8.0,
                            move |page: &mut Self, checked, _window, cx| {
                                if checked {
                                    page.hook_after_cli.insert(toggle_cli.clone());
                                } else {
                                    page.hook_after_cli.remove(&toggle_cli);
                                }
                                cx.notify();
                            },
                            cx,
                        )),
                    );
                }
            }
            TurnOnStep::HookQuestion => {
                let hook = hook_agent_id(agent).unwrap_or_default();
                let install_agent = agent_id.clone();
                let later_agent = agent_id.clone();
                let auto = self.store.read(cx).values().bool(AUTO_INSTALL_HOOKS);
                body = body
                    .child(title(format!("Turn on session resume for {name}?")))
                    .child(note(
                        "The hook lets Ghostex reopen the exact conversation after sleep, reload or restart. It only writes session ids into Ghostex's own files.",
                    ))
                    .child(
                        h_flex()
                            .pt(px(8.0))
                            .flex_wrap()
                            .items_center()
                            .gap(px(8.0))
                            .child(settings_button_sized(
                                p,
                                SharedString::from(format!("agent-turn-on-hook-{agent_id}")),
                                "Install hook",
                                Some(icons::DOWNLOAD),
                                ButtonVariant::Default,
                                ButtonSize::Sm,
                                false,
                                None,
                                move |page: &mut Self, _window, cx| {
                                    page.install_hooks(Some(vec![hook.clone()]), cx);
                                    page.turn_on.remove(&install_agent);
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(settings_button_sized(
                                p,
                                SharedString::from(format!("agent-turn-on-later-{agent_id}")),
                                "Not now",
                                None,
                                ButtonVariant::Ghost,
                                ButtonSize::Sm,
                                false,
                                None,
                                move |page: &mut Self, _window, cx| {
                                    page.turn_on.remove(&later_agent);
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(div().flex_1())
                            .child(checkbox_control(
                                p,
                                SharedString::from(format!("agent-hook-always-{agent_id}")),
                                auto,
                                Some(
                                    note("Always install the hook when I turn on an agent")
                                        .into_any_element(),
                                ),
                                8.0,
                                |page: &mut Self, checked, _window, cx| {
                                    page.save(AUTO_INSTALL_HOOKS, json!(checked), cx);
                                },
                                cx,
                            )),
                    );
            }
            TurnOnStep::Installed => {
                let hook_on = hook_agent_id(agent).is_some();
                body = body.child(title(if hook_on {
                    format!("{name} is installed; its session resume hook is being turned on")
                } else {
                    format!("{name} is installed")
                }));
            }
        }
        Some(
            h_flex()
                .id(SharedString::from(format!("agent-turn-on-step-{agent_id}")))
                .mx(px(4.0))
                .mb(px(12.0))
                .ml(px(46.0))
                .p(px(12.0))
                .gap(px(12.0))
                .items_start()
                .rounded(px(10.0))
                .border_1()
                .border_color(hsla(border))
                .bg(hsla(css_fade(p.raised_hover, 0.6)))
                .child(
                    div()
                        .pt(px(1.0))
                        .child(settings_icon(icon_path, 16.0, icon_color)),
                )
                .child(body)
                .child(dismiss)
                .into_any_element(),
        )
    }
}
