//! `ghostex work-mode` and `ghostex link-session` (crate::work_mode): the per-project switch, the
//! Linear key, and a session's hand-set links.

use std::io::{BufRead, IsTerminal, Write};

use serde_json::{json, Map, Value};

use super::actions::with_resolved_gxserver_session_params;
use super::args::parse_args;
use super::output::print_json;
use super::rpc::{self, CliError, CliResult};

/// `ghostex work-mode on|off|status|linear-key …`.
pub(super) fn work_mode_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    let subcommand = parsed.rest.first().map(String::as_str).unwrap_or("status");
    match subcommand {
        "on" | "off" => {
            let mut params = project_selector(flags);
            params.insert("enabled".to_string(), json!(subcommand == "on"));
            let result =
                rpc::call_gxserver_rpc("/api/setProjectWorkMode", &Value::Object(params), flags)?;
            print_json(&result);
            Ok(())
        }
        "status" => {
            let result = rpc::call_gxserver_rpc("/api/readWorkModeStatus", &json!({}), flags)?;
            print_json(&result);
            Ok(())
        }
        "linear-key" => {
            let mut params = Map::new();
            if let Some(project_id) = flags.string_value("projectId") {
                params.insert("projectId".to_string(), json!(project_id));
            }
            if !flags.truthy("clear") {
                params.insert("apiKey".to_string(), json!(read_secret_line()?));
            }
            let result =
                rpc::call_gxserver_rpc("/api/setLinearApiKey", &Value::Object(params), flags)?;
            print_json(&result);
            Ok(())
        }
        other => Err(CliError::Other(format!(
            "Unknown work-mode command \"{other}\". Use on, off, status or linear-key."
        ))),
    }
}

/// `ghostex link-session <selector> [--pr N|URL|none] [--linear SPX-1,SPX-2|none]
/// [--issue N|none] [--linear-project NAME|none] [--auto]`.
pub(super) fn link_session_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let flags = &parsed.flags;
    let mut payload = Map::new();
    if let Some(selector) = flags
        .string_value("sessionId")
        .map(str::to_string)
        .or_else(|| parsed.rest.first().cloned())
    {
        payload.insert("sessionId".to_string(), json!(selector));
    }
    let mut params = with_resolved_gxserver_session_params(&Value::Object(payload), flags)?
        .as_object()
        .cloned()
        .unwrap_or_default();
    if flags.truthy("auto") {
        params.insert("clear".to_string(), json!(true));
    }
    for (flag, key) in [
        ("pr", "pullRequest"),
        ("linear", "linearIssues"),
        ("issue", "githubIssues"),
        ("linearProject", "linearProject"),
    ] {
        if let Some(value) = flags.string_value(flag) {
            params.insert(key.to_string(), json!(value));
        }
    }
    if !params.contains_key("clear")
        && ![
            "pullRequest",
            "linearIssues",
            "githubIssues",
            "linearProject",
        ]
        .iter()
        .any(|key| params.contains_key(*key))
    {
        return Err(CliError::Other(
            "Pass --pr, --linear, --issue, --linear-project or --auto.".to_string(),
        ));
    }
    let result = rpc::call_gxserver_rpc("/api/setSessionWorkLinks", &Value::Object(params), flags)?;
    print_json(&result);
    Ok(())
}

/// `--project-id`, else `--path`, else the current folder.
fn project_selector(flags: &super::args::Flags) -> Map<String, Value> {
    let mut params = Map::new();
    if let Some(project_id) = flags.string_value("projectId") {
        params.insert("projectId".to_string(), json!(project_id));
    } else if let Some(path) = flags.string_value("path") {
        params.insert("path".to_string(), json!(path));
    } else if let Ok(cwd) = std::env::current_dir() {
        params.insert("path".to_string(), json!(cwd.to_string_lossy()));
    }
    params
}

/// Reads the key from stdin, so it never lands in shell history or the process list.
fn read_secret_line() -> CliResult<String> {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        eprint!("Paste your Linear API key and press Enter: ");
        let _ = std::io::stderr().flush();
    }
    let mut line = String::new();
    stdin
        .lock()
        .read_line(&mut line)
        .map_err(|error| CliError::Other(format!("Could not read the key: {error}")))?;
    let key = line.trim().to_string();
    if key.is_empty() {
        return Err(CliError::Other(
            "No key was given. Pipe it in, or pass --clear to remove the key.".to_string(),
        ));
    }
    Ok(key)
}
