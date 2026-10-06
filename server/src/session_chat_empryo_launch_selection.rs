//! CDXC:AgentProviders 2026-10-06 DECISION:
//! Sven (Empryo harness spec, user story 22): "start an Empryo session with a chosen model and effort, so that it begins on the right lane."
//!
//! CDXC:AgentProviders 2026-10-06 WHY:
//! The spec planned `--model <provider/model> --effort <level>` on the launch line, but Empryo 3.9.0-beta's terminal app ignores both (only `--headless` parses them; measured: `empryo --model subscriptions/gpt-6-sol --effort high` opened on the saved default). So a create that names an Empryo model keeps the launch command as it is, records the pick on the session, and the first hook Empryo sends once it is up hands the pick to the durable model selection queue, which types it through the `/models` and `/effort` driver as soon as Empryo's input box accepts it and holds chat messages behind it until then.
//! SEE-ALSO: server/src/agents/launch_plan.rs `apply_requested_agent_model`, server/src/session_chat_empryo_picker.rs, server/src/session_chat_model_selection.rs.

use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;

/// The runtime setting that carries an Empryo launch pick until Empryo is up.
const EMPRYO_LAUNCH_SELECTION_KEY: &str = "empryoLaunchSelection";

/// The pick an Empryo create carries, refused when it names an effort without its model.
pub(crate) fn empryo_launch_selection(
    model: Option<&str>,
    effort: Option<&str>,
) -> Result<Value, DomainStateError> {
    let model = model
        .filter(|model| model.contains('/') && !model.starts_with('/') && !model.ends_with('/'))
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "An Empryo launch model is provider/model, such as subscriptions/gpt-6-luna, and an effort needs one.",
            )
        })?;
    Ok(json!({ "model": model, "effort": effort.unwrap_or_default() }))
}

pub(crate) fn record_empryo_launch_selection(
    runtime_settings: &mut Map<String, Value>,
    selection: Value,
) {
    runtime_settings.insert(EMPRYO_LAUNCH_SELECTION_KEY.to_string(), selection);
}

/// Moves the Empryo launch pick of the session an agent hook reported into the model selection
/// queue; a no-op for every other hook, so the first one Empryo sends once it is up delivers it.
pub(crate) fn queue_empryo_launch_selection_from_hook(
    state: &AppState,
    db: &rusqlite::Connection,
    hook_result: &Value,
) {
    let Some(session) = hook_result.get("session").filter(|session| {
        session
            .get("runtimeSettings")
            .and_then(|runtime| runtime.get(EMPRYO_LAUNCH_SELECTION_KEY))
            .is_some()
    }) else {
        return;
    };
    let text = |value: &Value, key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let (project_id, session_id) = (text(session, "projectId"), text(session, "sessionId"));
    let Some(selection) = take_empryo_launch_selection(state, db, &project_id, &session_id) else {
        return;
    };
    let params = json!({
        "projectId": project_id,
        "sessionId": session_id,
        "model": text(&selection, "model"),
        "effort": text(&selection, "effort"),
    });
    if let Err(error) =
        crate::session_chat_model_selection::enqueue(state, params.as_object().unwrap())
    {
        let _ = state.logger.log(crate::logging::GxserverLogInput {
            level: crate::logging::LogLevel::Warn,
            event: "empryoLaunchSelectionNotQueued".to_string(),
            server_id: Some(state.metadata.server_id.clone()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: Some(error.message),
            details: Some(params),
        });
    }
}

/// Removes and returns the pick, under the database's write lock so a hook updating the same row
/// at the same moment keeps its change and a second hook finds nothing left to queue.
fn take_empryo_launch_selection(
    state: &AppState,
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
) -> Option<Value> {
    let transaction =
        rusqlite::Transaction::new_unchecked(db, rusqlite::TransactionBehavior::Immediate).ok()?;
    let repository = DomainRepository::new(&transaction, &state.metadata.server_id);
    let session = repository.get_session(project_id, session_id).ok()??;
    let mut runtime = session.get("runtimeSettings")?.as_object()?.clone();
    let selection = runtime.remove(EMPRYO_LAUNCH_SELECTION_KEY)?;
    repository
        .update_session(
            json!({
                "projectId": project_id,
                "sessionId": session_id,
                "runtimeSettings": runtime,
            })
            .as_object()?,
        )
        .ok()?;
    transaction.commit().ok()?;
    Some(selection)
}
