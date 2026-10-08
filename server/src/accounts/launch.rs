use super::{helpers, model::*, store};
use crate::{
    agents::{command_word, quote_shell_arg, reusable_account_command},
    domain::{DomainRepository, DomainStateError},
};
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

/// CDXC:AgentProviders 2026-09-07 DECISION:
/// Picking an account for a custom agent keeps its model, instructions and other command arguments. Replace only the provider invocation, retaining the original command for later account switches and resumes.
pub(crate) fn with_account_command(
    base: &str,
    provider: Provider,
    wrapper: &str,
) -> Result<String, DomainStateError> {
    let invalid = || {
        DomainStateError::bad_request(format!(
        "Account selection needs a {} command (or {} run). Update this custom agent's command while keeping its arguments.",
        provider.id(), provider.helper()
    ))
    };
    let mut offset = 0;
    while let Some((start, end, word)) = command_word(base, offset) {
        offset = end;
        // Environment assignments and the standard invocation prefixes keep their original shell spelling.
        if word.contains('=') || matches!(word.as_str(), "env" | "exec" | "command") {
            continue;
        }
        let executable = Path::new(&word)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if executable == provider.id() {
            return Ok(format!("{}{wrapper}{}", &base[..start], &base[end..]));
        }
        if executable == provider.helper() {
            let (_, run_end, run) = command_word(base, offset).ok_or_else(invalid)?;
            if run != "run" {
                return Err(invalid());
            }
            let (_, slot_end, _) = command_word(base, run_end).ok_or_else(invalid)?;
            offset = slot_end;
            while let Some((_, end, word)) = command_word(base, offset) {
                if word == "--" {
                    return Ok(format!("{}{wrapper}{}", &base[..start], &base[end..]));
                }
                if !matches!(
                    word.as_str(),
                    "--share-history" | "--no-share" | "--require-session"
                ) {
                    return Ok(format!("{}{wrapper}{}", &base[..start], &base[offset..]));
                }
                offset = end;
            }
            return Ok(format!("{}{wrapper}", &base[..start]));
        }
        return Err(invalid());
    }
    Err(invalid())
}

/// Whether a command sets the provider's profile directory itself or runs something other than the provider CLI or its account helper, so an automatic account would override or reject it.
fn uses_own_login(base: &str, provider: Provider) -> bool {
    let profile_var = match provider {
        Provider::Claude => "CLAUDE_CONFIG_DIR",
        Provider::Codex => "CODEX_HOME",
    };
    let mut offset = 0;
    while let Some((_, end, word)) = command_word(base, offset) {
        offset = end;
        if let Some((name, _)) = word.split_once('=') {
            if name == profile_var {
                return true;
            }
            continue;
        }
        if matches!(word.as_str(), "env" | "exec" | "command") {
            continue;
        }
        let executable = Path::new(&word)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        return executable != provider.id() && executable != provider.helper();
    }
    false
}

pub(crate) fn provider(project: &Value, session: &Value) -> Option<Provider> {
    match crate::agents::session_agent_family_id(project, session).as_deref() {
        Some("claude") => Some(Provider::Claude),
        Some("codex") => Some(Provider::Codex),
        _ => None,
    }
}
pub(crate) fn command(home: &Path, account: &SavedAccount) -> Result<String, DomainStateError> {
    validate_identity(home, account)?;
    let executable = helpers::executable(home, account.provider.helper()).ok_or_else(|| {
        DomainStateError::bad_request(format!(
            "Install {} on this computer first.",
            account.provider.helper()
        ))
    })?;
    // CDXC:AgentProviders 2026-09-18 WHY:
    // Windows sessions run in PowerShell, where a quoted executable path in command position is a string literal that prints instead of running. The call operator is required, as in agent_cli/endpoint.rs and zmx/scripts_windows.rs.
    let invoke = if cfg!(windows) { "& " } else { "" };
    Ok(format!(
        "{invoke}{} run {} --share-history --",
        quote_shell_arg(&executable.to_string_lossy()),
        quote_shell_arg(&account.selector)
    ))
}
pub(crate) fn assign(
    runtime: &mut Map<String, Value>,
    account: &SavedAccount,
    command: String,
) -> Result<String, DomainStateError> {
    let base = runtime
        .get("accountBaseCommand")
        .or_else(|| runtime.get("agentCommand"))
        .and_then(Value::as_str)
        .unwrap_or(account.provider.id());
    let base = reusable_account_command(base, account.provider.id())?;
    let command = with_account_command(&base, account.provider, &command)?;
    runtime.insert("accountBaseCommand".into(), json!(base));
    for (k, v) in [
        ("accountId", json!(account.id)),
        ("accountProvider", json!(account.provider)),
        ("accountName", json!(account.name)),
        ("accountSlot", json!(account.selector)),
        ("accountCommand", json!(command)),
        ("agentCommand", json!(command)),
    ] {
        runtime.insert(k.into(), v);
    }
    Ok(command)
}
pub(crate) fn apply_new_session(
    db: &Connection,
    agent_id: &str,
    icon: Option<&str>,
    runtime: &mut Map<String, Value>,
) -> Result<Option<String>, DomainStateError> {
    let provider = match icon.unwrap_or(agent_id) {
        "claude" => Provider::Claude,
        "codex" => Provider::Codex,
        _ => return Ok(None),
    };
    // CDXC:AgentProviders 2026-10-08 WHY:
    // Upstream #208: a custom agent whose command sets its own profile (CLAUDE_CONFIG_DIR or CODEX_HOME) or runs a wrapper such as `claude-personal` had that profile replaced by cswap's session profile, or failed to launch, as soon as any account was registered. Such a command already chose its login, so it runs as-is. The sidebar launcher sends the rule's account as `accountId`, so an automatic choice cannot be told apart from a picked one here.
    if runtime
        .get("accountBaseCommand")
        .or_else(|| runtime.get("agentCommand"))
        .and_then(Value::as_str)
        .is_some_and(|base| uses_own_login(base, provider))
    {
        return Ok(None);
    }
    let registry = store::read(db)?;
    // CDXC:AgentProviders 2026-09-11 DECISION: User: use the current CLI login until an account is added to Ghostex for that provider (2026-09-09); once accounts exist, a launch without an explicit account uses the provider's Account for new sessions rule from Settings (Most limit remaining by default, see default_account.rs), which supersedes the lowest-slot choice. When that rule yields no account the launch keeps the current CLI login, so a normal CLI launch needs no account switcher.
    let snapshot = super::runtime::current_snapshot();
    let id = runtime
        .get("accountId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            super::default_account::quick_launch_account(&registry, &snapshot, provider)
                .map(|a| a.id.clone())
        });
    let Some(id) = id else {
        return Ok(None);
    };
    let account = registry
        .accounts
        .iter()
        .find(|a| a.id == id && a.provider == provider)
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "The selected account is no longer registered. Choose another account.",
            )
        })?;
    let home = home()?;
    let cmd = command(&home, account)?;
    let assigned = assign(runtime, account, cmd)?;
    super::default_account::record_last_used(db, &registry, provider, &account.id)?;
    Ok(Some(assigned))
}
/// CDXC:AgentProviders 2026-09-18 WHY:
/// Windows has no HOME, so reading it alone failed every account launch and resume with "The server's home directory is unavailable."
pub(crate) fn home() -> Result<PathBuf, DomainStateError> {
    ["HOME", "USERPROFILE"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .find(|p| p.is_absolute())
        .ok_or_else(|| DomainStateError::bad_request("The server's home directory is unavailable."))
}
pub(crate) fn validate_session(
    repository: &DomainRepository<'_>,
    session: &Value,
) -> Result<(), DomainStateError> {
    let Some(id) = session
        .pointer("/runtimeSettings/accountId")
        .and_then(Value::as_str)
    else {
        return Ok(());
    };
    let registry = store::read(repository.db)?;
    let account = registry
        .accounts
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "This account was removed from Ghostex. Select an account before resuming.",
            )
        })?;
    validate_identity(&home()?, account)
}
pub(crate) fn validate_identity(
    home: &Path,
    account: &SavedAccount,
) -> Result<(), DomainStateError> {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"));
    // CDXC:AgentProviders 2026-09-18 WHY:
    // xswap's registry is %LOCALAPPDATA%\codex-swap on Windows and $XDG_DATA_HOME/codex-swap elsewhere (its fsutil::default_data_dir).
    // Reading the POSIX location on Windows made every saved Codex account fail identity validation as changed or unavailable.
    let xswap_data_home = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| data_home.clone())
    } else {
        data_home.clone()
    };
    let path = match account.provider {
        Provider::Codex => std::env::var_os("XSWAP_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| xswap_data_home.join("codex-swap"))
            .join("accounts.json"),
        Provider::Claude => super::claude_resets::swap_root(home).join("sequence.json"),
    };
    let fail = || {
        DomainStateError::bad_request("The saved account changed or is unavailable. Refresh Accounts and reconnect it before resuming.")
    };
    let raw = std::fs::read(path).map_err(|_| fail())?;
    let data: Value = serde_json::from_slice(&raw).map_err(|_| fail())?;
    let identity = if account.provider == Provider::Claude {
        let row = &data["accounts"][&account.selector];
        format!(
            "{}:{}",
            row["email"].as_str().unwrap_or("").to_lowercase(),
            row["organizationUuid"].as_str().unwrap_or("")
        )
    } else {
        let row = data["accounts"]
            .as_array()
            .and_then(|rows| {
                rows.iter().find(|r| {
                    r["number"].as_u64().map(|n| n.to_string()).as_deref()
                        == Some(&account.selector)
                })
            })
            .ok_or_else(fail)?;
        row.pointer("/identity/accountId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    if identity != account.identity || identity.is_empty() {
        return Err(fail());
    }
    Ok(())
}
/// CDXC:AgentProviders 2026-09-19 DECISION:
/// User: "I literally didn't touch the default in the session or in settings" and the session still did not switch accounts at a limit. A session follows the provider's continuation defaults from Settings > Accounts as they are now, unless it has its own Customize settings. This supersedes the 2026-09-05 copy saved at launch (`accountPolicyDefault`), which forks, restored sessions, CLIs adopted from a terminal, and agent re-detection never received or lost, leaving them off while Settings said on.
pub(crate) fn effective_policy(registry: &Registry, provider: Provider, session: &Value) -> Policy {
    session
        .pointer("/runtimeSettings/accountPolicyOverride")
        .filter(|v| !v.is_null())
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .or_else(|| registry.defaults.get(&provider).cloned())
        .unwrap_or_default()
}
