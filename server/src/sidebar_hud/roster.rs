//! The Settings › Agents roster: every agent Ghostex knows (built-in and custom, on and off) in the
//! one shared order, with whether it is on and when it was last used.

use std::collections::HashMap;

use super::*;

/// CDXC:AgentLauncher 2026-10-06 DECISION:
/// User: "ok implement the plan" for the Agents page redesign: every agent has an on/off switch instead of Add/Delete, agents that are off but were used before stay dimmed in place in the list, and agents that are off and never used sit in a compact "More agents" grid. Turning an agent off writes the stored `hidden` flag (built-in and custom alike) and keeps its slot in `customAgentOrder`, so turning it back on restores it where it was. "Used before" comes from the sessions table here so every client agrees.
pub fn read_sidebar_agent_roster(projects: &[Value], last_used: &HashMap<String, String>) -> Value {
    let (stored_agents, stored_order) = sidebar_agent_state_from_projects(projects);
    let order = full_sidebar_agent_order(&stored_agents, &stored_order);
    let entries = order
        .iter()
        .filter_map(|agent_id| sidebar_agent_roster_entry(&stored_agents, agent_id, last_used))
        .collect();
    Value::Array(entries)
}

fn sidebar_agent_roster_entry(
    stored_agents: &[StoredSidebarAgent],
    agent_id: &str,
    last_used: &HashMap<String, String>,
) -> Option<Value> {
    let stored = stored_agents
        .iter()
        .find(|agent| agent.agent_id == agent_id);
    let default_agent = default_sidebar_agent_by_id(agent_id);
    if stored.is_none() && default_agent.is_none() {
        return None;
    }
    let mut entry = Map::new();
    if let Some(mode) = stored.and_then(|agent| agent.accept_all_mode.as_ref()) {
        entry.insert("acceptAllMode".to_string(), Value::String(mode.clone()));
    }
    entry.insert("agentId".to_string(), Value::String(agent_id.to_string()));
    let command = stored
        .map(|agent| agent.command.clone())
        .or_else(|| default_agent.map(|agent| agent.command.to_string()))
        .unwrap_or_default();
    entry.insert("command".to_string(), Value::String(command));
    if let Some(default_agent) = default_agent {
        entry.insert(
            "defaultCommand".to_string(),
            Value::String(default_agent.command.to_string()),
        );
        entry.insert(
            "defaultName".to_string(),
            Value::String(default_agent.name.to_string()),
        );
    }
    entry.insert(
        "enabled".to_string(),
        Value::Bool(is_sidebar_agent_enabled(stored_agents, agent_id)),
    );
    let icon = stored
        .and_then(|agent| agent.icon.clone())
        .or_else(|| default_agent.map(|agent| agent.icon.to_string()));
    if let Some(icon) = icon.filter(|icon| !icon.is_empty()) {
        entry.insert("icon".to_string(), Value::String(icon));
    }
    entry.insert(
        "isDefault".to_string(),
        Value::Bool(default_agent.is_some()),
    );
    if let Some(at) = last_used.get(agent_id) {
        entry.insert("lastUsedAt".to_string(), Value::String(at.clone()));
    }
    let name = match (stored, default_agent) {
        (Some(stored), Some(_)) => default_sidebar_agent_name(agent_id, &stored.name),
        (Some(stored), None) => stored.name.clone(),
        (None, Some(default_agent)) => default_agent.name.to_string(),
        (None, None) => return None,
    };
    entry.insert("name".to_string(), Value::String(name));
    Some(Value::Object(entry))
}

/// Every agent id in display order, off agents included: the stored order first (ids that still
/// exist), then the agents the launcher shows that the order never named, then every other agent
/// (off built-ins in their default order, then off custom agents).
pub(super) fn full_sidebar_agent_order(
    stored_agents: &[StoredSidebarAgent],
    stored_order: &[String],
) -> Vec<String> {
    let mut known = DEFAULT_SIDEBAR_AGENTS
        .iter()
        .map(|agent| agent.agent_id.to_string())
        .collect::<Vec<_>>();
    for agent in stored_agents {
        if !known.contains(&agent.agent_id) {
            known.push(agent.agent_id.clone());
        }
    }
    let mut order = stored_order
        .iter()
        .filter(|agent_id| known.contains(agent_id))
        .cloned()
        .collect::<Vec<_>>();
    let visible = sidebar_button_ids(
        &sidebar_agent_buttons_from_state(stored_agents, stored_order),
        "agentId",
    );
    for agent_id in visible.into_iter().chain(known) {
        if !order.contains(&agent_id) {
            order.push(agent_id);
        }
    }
    order
}

/// Applies a requested order to the full order without moving the agents it leaves out: the
/// slots the requested ids held are refilled in the requested sequence, so an agent that is off
/// (and missing from a launcher-only reorder) keeps its place.
pub(super) fn merge_sidebar_agent_order(
    full_order: &[String],
    requested: &[String],
) -> Vec<String> {
    let requested = requested
        .iter()
        .filter(|agent_id| full_order.contains(agent_id))
        .cloned()
        .collect::<Vec<_>>();
    let mut queue = requested.iter();
    full_order
        .iter()
        .map(|agent_id| {
            if requested.contains(agent_id) {
                queue.next().cloned().unwrap_or_else(|| agent_id.clone())
            } else {
                agent_id.clone()
            }
        })
        .collect()
}
