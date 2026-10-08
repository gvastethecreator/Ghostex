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

/// Whether a custom agent's command chose its own login, so an automatic account would override or reject it: it assigns the provider's profile directory (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`; also PowerShell's `$env:` form) before its executable, or it never runs the provider CLI or its account helper (a wrapper such as `claude-personal`).
/// Any command that names the provider CLI or helper anywhere (an absolute or quoted path, a Windows `.exe`/`.cmd` shim, `& '…'`, `npx @anthropic-ai/claude-code`, `mise exec -- claude`) keeps the account flow it had before, so only those two shapes change behaviour. Clients can ask this for an agent's command to show that it uses its own login.
pub(crate) fn uses_own_login(base: &str, provider: Provider) -> bool {
    let profile_var = match provider {
        Provider::Claude => "CLAUDE_CONFIG_DIR",
        Provider::Codex => "CODEX_HOME",
    };
    // An empty or unfinished command keeps the error account assignment gave it before.
    if base.trim().is_empty() || reusable_account_command(base, provider.id()).is_err() {
        return false;
    }
    let mut offset = 0;
    let mut skip_value = false;
    let mut in_powershell_assignment = false;
    while let Some((_, end, word)) = command_word(base, offset) {
        offset = end;
        if std::mem::take(&mut skip_value) {
            continue;
        }
        // `$env:NAME = 'value';` spans words up to its `;`, and PowerShell names ignore case.
        if let Some(name) = word
            .get(..5)
            .filter(|prefix| prefix.eq_ignore_ascii_case("$env:"))
            .map(|_| word[5..].split('=').next().unwrap_or_default())
        {
            if name.eq_ignore_ascii_case(profile_var) {
                return true;
            }
            in_powershell_assignment = !word.ends_with(';');
            continue;
        }
        if in_powershell_assignment {
            in_powershell_assignment = !word.ends_with(';');
            continue;
        }
        if let Some((name, _)) = word.split_once('=') {
            if name == profile_var {
                return true;
            }
            continue;
        }
        // PowerShell's call operator and the standard invocation prefixes come before the executable.
        if matches!(word.as_str(), "&" | "env" | "exec" | "command" | "export") {
            continue;
        }
        // Options of the prefixes above, such as `env -u NAME claude`; `env -u`/`-C` and `exec -a` take the next word as their value.
        if word.starts_with('-') {
            skip_value = matches!(word.as_str(), "-u" | "--unset" | "-C" | "--chdir" | "-a");
            continue;
        }
        break;
    }
    !mentions_provider(base, provider)
}

/// Whether any word of a command runs the provider CLI or its account helper, read from the raw text so unquoted Windows paths (whose backslashes a POSIX word reader drops) still count.
fn mentions_provider(command: &str, provider: Provider) -> bool {
    let package = match provider {
        Provider::Claude => "@anthropic-ai/claude-code",
        Provider::Codex => "@openai/codex",
    };
    command
        .split(|c: char| c.is_whitespace() || "'\"`;&|()=,".contains(c))
        .any(|token| {
            let token = token.to_ascii_lowercase();
            let name = token.rsplit(['/', '\\']).next().unwrap_or(&token);
            let stem = [".exe", ".cmd", ".bat", ".ps1"]
                .iter()
                .find_map(|suffix| name.strip_suffix(suffix))
                .unwrap_or(name);
            stem == provider.id() || stem == provider.helper() || token.contains(package)
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// What account assignment did before custom agents could keep their own login.
    fn assigned_before(base: &str, provider: Provider) -> bool {
        reusable_account_command(base, provider.id())
            .and_then(|base| with_account_command(&base, provider, "WRAPPER"))
            .is_ok()
    }

    #[test]
    fn provider_commands_keep_the_account_flow() {
        let claude = [
            "claude",
            "claude --dangerously-skip-permissions --model opus --effort high",
            "claude --resume 0b1c2d --model 'opus[1m]'",
            "claude --append-system-prompt-file '/Users/me/.ghostex/coordinator.md'",
            "/opt/homebrew/bin/claude --model sonnet",
            "~/.local/bin/claude",
            "'/Users/me/My Tools/claude' --model sonnet",
            "env FOO=bar claude",
            "env -u ANTHROPIC_API_KEY claude",
            "env -u CLAUDE_CONFIG_DIR claude",
            "GHOSTEX_PROMPT_EDITING_ENABLED=1 VISUAL='ghostex-editor --wait' claude",
            "CODEX_HOME=/tmp/x claude",
            "command claude",
            "exec claude --model opus",
            "export FOO=1; claude",
            "cswap run 2 --share-history -- --model opus",
            "'/Users/me/.local/bin/cswap' run '1' --share-history --",
            r"& 'C:\Users\me\.local\bin\cswap.exe' run '1' --share-history -- --model opus",
            r"& 'C:\Users\me\.local\bin\claude.exe' --model opus",
            r"& 'C:\Program Files\nodejs\claude.cmd'",
            r"C:\Users\me\AppData\Roaming\npm\claude.cmd --model opus",
            r"& $env:LOCALAPPDATA\Programs\claude\claude.exe",
            "claude.EXE",
            "claude.ps1 --model opus",
            "Claude",
            "$env:GHOSTEX_PROMPT_EDITING_ENABLED='1'; claude",
            "$env:FOO = 'a b'; claude",
            "npx @anthropic-ai/claude-code --model opus",
            "bunx @anthropic-ai/claude-code@latest",
            "mise exec -- claude",
            "claude 'unfinished",
            "",
        ];
        let codex = [
            "codex",
            "codex --yolo -c model_reasoning_effort=high resume 019a",
            "CLAUDE_CONFIG_DIR=/tmp/x codex",
            "xswap run 1 --share-history -- --yolo",
            r"& 'C:\Users\me\AppData\Local\Programs\codex-swap\xswap.exe' run '1' --share-history --",
            r"& 'C:\Users\me\AppData\Roaming\npm\codex.cmd' --yolo",
            "npx @openai/codex",
        ];
        for (provider, commands) in [
            (Provider::Claude, &claude[..]),
            (Provider::Codex, &codex[..]),
        ] {
            for base in commands {
                assert!(!uses_own_login(base, provider), "{base}");
            }
        }
    }

    #[test]
    fn own_login_commands_are_only_profile_assignments_or_other_executables() {
        // (command, sets the profile directory): every other own-login command was refused by account assignment before.
        let claude = [
            (
                "CLAUDE_CONFIG_DIR=\"$HOME/.claude-alt\" command claude",
                true,
            ),
            (
                "CLAUDE_CONFIG_DIR=~/.claude-profiles/work claude --model opus",
                true,
            ),
            ("env CLAUDE_CONFIG_DIR=/x claude", true),
            ("export CLAUDE_CONFIG_DIR=/x; claude", true),
            (
                r"$env:CLAUDE_CONFIG_DIR='C:\Users\me\.claude-work'; claude",
                true,
            ),
            (
                r"$env:claude_config_dir = 'C:\x'; & 'C:\Users\me\.local\bin\claude.exe'",
                true,
            ),
            ("claude-personal", false),
            ("claude-personal --model opus", false),
            ("~/bin/claude-work --resume abc", false),
            ("c2", false),
        ];
        let codex = [
            ("CODEX_HOME=~/.codex-profiles/work codex", true),
            (r"$env:CODEX_HOME='C:\x'; codex --yolo", true),
            ("codex-work", false),
        ];
        for (provider, commands) in [
            (Provider::Claude, &claude[..]),
            (Provider::Codex, &codex[..]),
        ] {
            for (base, sets_profile) in commands {
                assert!(uses_own_login(base, provider), "{base}");
                if !sets_profile {
                    assert!(!assigned_before(base, provider), "{base}");
                }
            }
        }
    }
}
