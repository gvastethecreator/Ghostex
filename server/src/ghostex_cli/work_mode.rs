//! `ghostex work-mode` and `ghostex link-session` (crate::work_mode): the per-project switch, the
//! Linear key, creating a Linear ticket, starting work on a ticket, and a session's hand-set links.

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
        "create-ticket" => create_ticket(&parsed.rest[1..], flags),
        "start" => {
            let ticket = parsed.rest.get(1).ok_or_else(|| {
                CliError::Other("Pass the ticket: ghostex work-mode start SPX-1245 (or #218).".to_string())
            })?;
            let result = start_work(ticket, flags)?;
            print_json(&result);
            Ok(())
        }
        other => Err(CliError::Other(format!(
            "Unknown work-mode command \"{other}\". Use on, off, status, linear-key, create-ticket or start."
        ))),
    }
}

/// `ghostex work-mode create-ticket --title T [--description D] [--team-id id]
/// [--linear-project-id id] [--no-assign] [--start [--agent id] [--model m] [--effort e]]`.
fn create_ticket(rest: &[String], flags: &super::args::Flags) -> CliResult<()> {
    let title = flags
        .string_value("title")
        .map(str::to_string)
        .or_else(|| (!rest.is_empty()).then(|| rest.join(" ")))
        .ok_or_else(|| CliError::Other("Pass --title \"…\".".to_string()))?;
    let mut params = project_selector(flags);
    params.insert("title".to_string(), json!(title));
    for (flag, key) in [
        ("description", "description"),
        ("teamId", "teamId"),
        ("linearProjectId", "linearProjectId"),
    ] {
        if let Some(value) = flags.string_value(flag) {
            params.insert(key.to_string(), json!(value));
        }
    }
    params.insert("assignToMe".to_string(), json!(!flags.truthy("noAssign")));
    let created = rpc::call_gxserver_rpc(
        "/api/createLinearIssue",
        &Value::Object(params),
        &with_default_timeout(flags, "30000"),
    )?;
    if !flags.truthy("start") {
        print_json(&created);
        return Ok(());
    }
    let identifier = created
        .get("identifier")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            CliError::Other("Linear did not say which ticket it created.".to_string())
        })?;
    let started = start_work(identifier, flags)?;
    print_json(&json!({ "ticket": created, "started": started }));
    Ok(())
}

/// Starts an agent on a ticket (`SPX-1245`, or `#218` / `218` for a GitHub issue) in a worktree on
/// the ticket's branch, linked to it, with nothing sent to the agent.
fn start_work(ticket: &str, flags: &super::args::Flags) -> CliResult<Value> {
    let mut params = project_selector(flags);
    let ticket = ticket.trim();
    if let Ok(number) = ticket.trim_start_matches('#').parse::<u64>() {
        params.insert("githubIssue".to_string(), json!(number));
    } else {
        params.insert("linearIssue".to_string(), json!(ticket));
    }
    for (flag, key) in [
        ("agent", "agentId"),
        ("model", "model"),
        ("effort", "effort"),
    ] {
        if let Some(value) = flags.string_value(flag) {
            params.insert(key.to_string(), json!(value));
        }
    }
    rpc::call_gxserver_rpc(
        "/api/startWorkOnTicket",
        &Value::Object(params),
        &with_default_timeout(flags, "180000"),
    )
}

/// Linear calls take a few seconds, and starting work fetches the branch, cuts the worktree, runs
/// the project's setup command and starts the agent: longer than the CLI's 15s default.
fn with_default_timeout(flags: &super::args::Flags, timeout_ms: &str) -> super::args::Flags {
    let mut flags = flags.clone();
    if !flags.contains("timeout") && !flags.contains("timeoutMs") {
        flags.insert_text("timeout", timeout_ms);
    }
    flags
}

/// `ghostex link-session <selector> [--pr N|URL|none] [--linear SPX-1,SPX-2|none]
/// [--issue N|none] [--linear-project NAME|none] [--auto]`, or
/// `ghostex link-session <selector> --candidates pullRequest|linearIssue|linearProject|githubIssue
/// [--query text]` to list what the Link to picker suggests.
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
    // CDXC:WorkMode 2026-10-09 SEE-ALSO: the phone's Link to picker (apps/mobile/app/src/screens/sessions-screen/work-link-picker.tsx) reads the same suggestions the desktop picker does through this flag, and saves through the flags below.
    if let Some(kind) = flags.string_value("candidates") {
        params.insert("kind".to_string(), json!(kind));
        if let Some(query) = flags.string_value("query") {
            params.insert("query".to_string(), json!(query));
        }
        let result = rpc::call_gxserver_rpc(
            "/api/listWorkLinkCandidates",
            &Value::Object(params),
            &with_default_timeout(flags, "30000"),
        )?;
        print_json(&result);
        return Ok(());
    }
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
            // `none` unlinks. The other kinds read it themselves; a Linear project is a name, so
            // the server takes the empty string for "explicitly none" (what the desktop's Unlink sends).
            let value = if key == "linearProject" && value.trim().eq_ignore_ascii_case("none") {
                ""
            } else {
                value
            };
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
