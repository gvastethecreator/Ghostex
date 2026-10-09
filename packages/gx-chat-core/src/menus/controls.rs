//! The Switch Account panel's own controller: who owns the accounts read, when it runs again, and
//! whether the switch card is on screen.
//!
//! Port of `packages/shared/session-chat-controller/native-controls.ts`.

use serde_json::{json, Value};

use crate::menus::account_switch::AccountSwitchStatus;
use crate::menus::accounts_presentation::SwitchProgress;
use crate::menus::option_menus::DraftAgent;
use crate::state::ChatState;

/// How often the session's accounts are re-read while a chat is open.
pub const ACCOUNTS_POLL_MS: i64 = 30_000;

/// The provider the switch card and the panel belong to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccountProviders {
    /// The switch's own provider, else the transcript family when it manages accounts.
    pub switch: Option<String>,
    /// The panel's provider: a draft's base agent (a custom agent built on Claude still manages
    /// Claude accounts), else the transcript family.
    pub panel: Option<String>,
}

/// `provider` and `panelProvider`.
///
/// A session on its own login has neither (CDXC:AgentProviders 2026-10-09 in
/// server/src/accounts/launch.rs), so the chat shows no Switch Account panel or switch card for it.
pub fn account_providers(state: &ChatState) -> AccountProviders {
    if state.session.uses_own_login {
        return AccountProviders::default();
    }
    let agent = state.session.agent.as_deref();
    let switch = switch_progress(state)
        .map(|progress| progress.provider)
        .or_else(|| match agent {
            Some(agent @ ("claude" | "codex")) => Some(agent.to_string()),
            _ => None,
        });
    let family = DraftAgent::list(state.session.available_agents.as_ref())
        .and_then(|agents| {
            agents
                .iter()
                .find(|row| {
                    Some(row.agent_id.as_str()) == state.session.session_agent_id.as_deref()
                })
                .and_then(|row| row.base_agent_id.clone())
        })
        .or_else(|| agent.map(str::to_string));
    let panel = match family.as_deref() {
        Some(family @ ("claude" | "codex")) => Some(family.to_string()),
        _ => None,
    };
    AccountProviders { switch, panel }
}

/// The `accountSwitch` gxserver reports, read out of family a's fold.
pub fn switch_progress(state: &ChatState) -> Option<SwitchProgress> {
    state
        .session
        .account_switch
        .value()
        .and_then(SwitchProgress::from_value)
}

/// `ready`: the target account is bound and no queued model change is still pending.
pub fn switch_ready(state: &ChatState) -> bool {
    let bound = match switch_progress(state).and_then(|progress| progress.to_account_id) {
        None => true,
        Some(target) if target.is_empty() => true,
        Some(target) => {
            crate::menus::options::accounts_state(state)
                .and_then(|accounts| {
                    accounts
                        .session
                        .as_ref()
                        .and_then(|session| session.account_id.clone())
                })
                .as_deref()
                == Some(target.as_str())
        }
    };
    let pending_state = state
        .session
        .pending_model_selection
        .value()
        .and_then(|value| value.get("state"))
        .and_then(Value::as_str);
    bound && pending_state != Some("queued") && pending_state != Some("applying")
}

/// The card, the clock and the send hold for this frame.
pub fn account_switch_status(state: &ChatState, now_ms: i64) -> AccountSwitchStatus {
    let progress = switch_progress(state);
    state
        .menus
        .account_switch
        .status(progress.as_ref(), switch_ready(state), now_ms)
}

/// What the periodic read is keyed on, so a provider, agent or switch change re-reads at once
/// rather than waiting out the poll.
pub fn accounts_poll_key(state: &ChatState) -> Option<String> {
    let providers = account_providers(state);
    if providers.switch.is_none() && providers.panel.is_none() {
        return None;
    }
    let progress = switch_progress(state);
    Some(format!(
        "{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}",
        providers.switch.unwrap_or_default(),
        providers.panel.unwrap_or_default(),
        state.session.session_agent_id.clone().unwrap_or_default(),
        progress
            .as_ref()
            .map(|progress| progress.id.clone())
            .unwrap_or_default(),
        progress
            .as_ref()
            .map(|progress| progress.phase.clone())
            .unwrap_or_default(),
    ))
}

/// The params of the periodic session read.
pub fn session_accounts_request() -> Value {
    json!({ "operation": "session" })
}
