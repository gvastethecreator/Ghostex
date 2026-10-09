//! The facts only the host knows that the menus read.
//!
//! Each one is named here so there is one list of what the menus still borrow from outside the
//! store. The agents and the Saved Actions come from the daemon's sidebar HUD, which moves into
//! the store with the session lifecycle (M5); the primary agent and the keep-awake runtime are
//! client storage; the bridge flag is the host's own capability.

use std::collections::BTreeMap;

/// One agent the launcher offers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LauncherAgent {
    pub agent_id: String,
    pub name: String,
    pub icon: Option<String>,
    /// The HUD's `usesOwnLogin`: the agent's command picks its own login, so the launcher offers
    /// no account and starts it without one.
    pub uses_own_login: bool,
}

impl LauncherAgent {
    /// The provider whose accounts the launcher offers for this agent, if any.
    pub fn account_provider(&self) -> Option<&'static str> {
        if self.uses_own_login {
            return None;
        }
        super::account_provider(&self.agent_id, self.icon.as_deref())
    }
}

/// One Saved Action that can sit on a project header.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeaderCommand {
    pub command_id: String,
    pub name: String,
    pub icon: Option<String>,
    pub show_on_project_row: bool,
}

/// One Open In target (an editor, the file manager), as the header Open In button lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuOpenTarget {
    pub target_id: String,
    pub label: String,
    pub icon: String,
}

/// Everything the menus read that is neither the store nor the settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuHost {
    /// The agents the launcher offers, in HUD order.
    pub agents: Vec<LauncherAgent>,
    /// The agent the user launched last.
    pub primary_agent_id: Option<String>,
    /// Saved Actions that apply to every project.
    pub global_commands: Vec<HeaderCommand>,
    /// Saved Actions by project id.
    pub project_commands: BTreeMap<String, Vec<HeaderCommand>>,
    /// The armed keep-awake duration, when one is running.
    pub keep_awake_minutes: Option<i64>,
    /// The selected machine tab is reachable: always true for the local daemon.
    pub machine_connected: bool,
    /// The visible Open In targets, in the header Open In button's order. Empty where the host
    /// cannot launch an app.
    pub open_targets: Vec<MenuOpenTarget>,
    /// The host is the desktop app, which can check for updates, restart and quit. False for a
    /// page in a browser.
    pub app_lifecycle: bool,
    /// The agentbox locations `/api/agentbox status` last reported ready on this computer; empty
    /// where the host has not read one (the launcher then offers no Run in a Box page).
    pub agentbox_locations: Vec<crate::agentbox::AgentboxLocation>,
}

impl MenuHost {
    /// `agents.find(agentId === readPrimaryAgentLauncherId()) ?? agents[0]`.
    pub(crate) fn primary_agent(&self) -> Option<&LauncherAgent> {
        self.primary_agent_id
            .as_deref()
            .and_then(|agent_id| self.agents.iter().find(|agent| agent.agent_id == agent_id))
            .or_else(|| self.agents.first())
    }
}
