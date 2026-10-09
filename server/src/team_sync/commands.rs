//! What this Ghostex does with a command from its member's queue.

use serde_json::{json, Value};

use crate::server::AppState;

use super::connections::TeamConnection;

pub(crate) enum CommandOutcome {
    Done(Value),
    Failed(String),
}

/// Whether this Ghostex runs commands of this type.
///
/// CDXC:TeamSync 2026-10-09 WHY:
/// A type this build does not know stays `pending` in the team's queue instead of failing, so a
/// newer Ghostex of the same member (or this one after an update) still picks it up.
pub(crate) fn handles_command_type(command_type: &str) -> bool {
    matches!(command_type, "ping" | "slack.request")
}

/// Runs one claimed command. Blocking; the runtime calls it from `spawn_blocking`.
pub(crate) fn run_team_command(
    state: &AppState,
    connection: &TeamConnection,
    command: &Value,
) -> CommandOutcome {
    match command.get("type").and_then(Value::as_str).unwrap_or("") {
        "ping" => CommandOutcome::Done(json!({
            "pong": true,
            "workspaceId": connection.workspace_id,
            "serverId": state.metadata.server_id,
            "version": state.version,
            "at": chrono::Utc::now().to_rfc3339(),
        })),
        "slack.request" => super::slack_request::run_slack_request(state, connection, command),
        other => CommandOutcome::Failed(format!("This Ghostex cannot run \"{other}\" commands.")),
    }
}
