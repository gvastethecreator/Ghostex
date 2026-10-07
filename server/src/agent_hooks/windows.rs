use serde_json::{json, Value};
use std::{io::Read, path::Path};

/// CDXC:AgentHooks 2026-09-14 WHY:
/// Windows command hooks must read JSON from stdin directly; passing it through Windows PowerShell 5.1 native argv strips JSON quotes.
/// Installation still uses the existing explicit install and Codex trust flow.
///
/// CDXC:AgentHooks 2026-10-04 WHY:
/// Claude Code runs its hooks through the same shell as its statusLine (Git Bash, or PowerShell without Git), so Claude's hooks take the statusline's both-shell form (`statusline::agent_statusline_command`): gxserver named by its space-free short path, arguments single-quoted, no powershell.exe. Its SessionStart hook is how gxserver learns a new chat's Claude session id, and through PowerShell it arrived after the first statusline, holding the chat's model pill back by up to a second. Other agents keep the powershell.exe form, because their CLIs may run hooks through cmd.exe, which keeps single quotes literally.
pub(crate) fn command(agent: &str, notify_path: &Path) -> String {
    let executable = std::env::current_exe().unwrap_or_default();
    let notify = notify_path.to_string_lossy();
    if let Some(executable) =
        bare_command_path(&executable).filter(|_| agent == "claude" && !notify.contains('\''))
    {
        return format!("{executable} agent-hook-notify-native '{notify}' 'claude'");
    }
    let quote = |text: &str| format!("'{}'", text.replace('\'', "''"));
    format!(
        "powershell.exe -NoLogo -NoProfile -NonInteractive -WindowStyle Hidden -Command \"& {} agent-hook-notify-native {} {}\"",
        quote(&executable.to_string_lossy()),
        quote(&notify_path.to_string_lossy()),
        quote(agent)
    )
}

/// `path` as a command word that Git Bash and PowerShell both run as it stands: its 8.3 short
/// form with forward slashes, when that has no space, quote or other character either shell
/// would read. `None` when the volume keeps no short names and the long path needs quoting.
pub(crate) fn bare_command_path(path: &Path) -> Option<String> {
    use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut buffer = vec![0u16; 1024];
    // SAFETY: `wide` is NUL-terminated and `buffer` holds `buffer.len()` UTF-16 units.
    let length = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetShortPathNameW(
            wide.as_ptr(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        )
    } as usize;
    if length == 0 || length >= buffer.len() {
        return None;
    }
    let short = std::ffi::OsString::from_wide(&buffer[..length])
        .into_string()
        .ok()?
        .replace('\\', "/");
    short
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ':' | '/' | '.' | '_' | '~' | '-'))
        .then_some(short)
}

pub(crate) fn notify(args: Vec<String>) -> anyhow::Result<()> {
    let script = std::fs::read_to_string(
        args.first()
            .ok_or_else(|| anyhow::anyhow!("Missing hook path"))?,
    )?;
    let directory = super::install::notify_hook_state_directory(&script)
        .ok_or_else(|| anyhow::anyhow!("Missing hook state directory"))?;
    let mut input = String::new();
    std::io::stdin()
        .take(1024 * 1024)
        .read_to_string(&mut input)?;
    // CDXC:AgentHooks 2026-10-07 WHY: Cursor CLI runs Windows hooks as `$OutputEncoding = [System.Text.Encoding]::UTF8; Get-Content <payload> -Raw | & { $input | <command> }`, and that encoding writes a UTF-8 BOM before the JSON in pwsh and Windows PowerShell alike; serde rejected it ("expected value at line 1 column 1"), so no Cursor hook ever reached gxserver and Cursor chats never found their transcript.
    let mut payload: Value = serde_json::from_str(input.trim_start_matches('\u{feff}'))?;
    let agent = args.get(1).map(String::as_str).unwrap_or("codex");
    if let Some(object) = payload.as_object_mut() {
        object.entry("agent").or_insert_with(|| json!(agent));
    }
    let state = [
        "VSMUX_SESSION_STATE_FILE",
        "GHOSTEX_SESSION_STATE_FILE",
        "ghostex_SESSION_STATE_FILE",
    ]
    .into_iter()
    .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()))
    .unwrap_or_default();
    let answer = if std::env::var("GHOSTEX_INTERNAL_PROMPT_GENERATION").as_deref() != Ok("1")
        && std::env::var("GHOSTEX_INTERNAL_TITLE_GENERATION").as_deref() != Ok("1")
    {
        super::run_notify_hook(vec![
            state,
            payload.to_string(),
            directory.to_string_lossy().into_owned(),
        ])
        .ok()
        .flatten()
    } else {
        None
    };
    if let Some(answer) = answer {
        // A ZCode coordinator's SessionStart answer replaces the canned response.
        println!("{answer}");
        return Ok(());
    }
    if agent != "antigravity" {
        if payload["hook_event_name"] == "Interrupt" {
            println!("{{}}");
        } else {
            println!("{{\"continue\":true}}");
        }
    }
    Ok(())
}

/// Resolved over the live registry PATH, so a CLI installed while gxserver runs is not reported missing.
pub(crate) fn resolve_command(command: &str) -> Option<String> {
    crate::platform::live_path::find(command, &[]).map(|path| path.to_string_lossy().into_owned())
}
