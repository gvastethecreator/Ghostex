//! The coordinator's role: the playbook it runs by, and how it reaches the agent at launch.

use std::path::{Path, PathBuf};

use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

/// CDXC:Coordinators 2026-09-30 WHY:
/// The role reaches the agent as a system prompt, not as a chat message: a first message scrolls away, is summarised by compaction, and shows in the chat as if the user typed it. The flag lives in the session's saved base command, which resume, fork and account wrapping rebuild from, so it survives all three (the same mechanism as per-session model flags). Dynamic state (goal, instructions, memory, threads) is read through `ghostex coordinator status` instead, because it changes while the session runs.
/// SEE-ALSO: server/src/agents/launch_plan.rs (applies the flags), server/src/ghostex_cli/coordinator/ (the verbs this playbook names).
pub const COORDINATOR_ROLE_PROMPT: &str = include_str!("role.md");

/// CDXC:Coordinators 2026-10-03 WHY:
/// Codex takes its extra instructions inline (`-c developer_instructions=...`), and ZCode has no
/// system-prompt or config flag a launch command could carry, so both get this short pointer —
/// Codex inline at launch, ZCode as SessionStart hook context — and read the full playbook from
/// `ghostex coordinator guide`. Claude is the only family that carries the whole role file.
pub const GUIDE_POINTER_COORDINATOR_INSTRUCTIONS: &str = "You are a Ghostex coordinator: the user talks only to you, and you hand real work to thread sessions instead of doing it yourself, so you stay free to talk. Before your first reply, and again after any context compaction, run `ghostex coordinator guide` and follow it for the whole session. Run `ghostex coordinator status` at the start of every request.";

/// Where gxserver keeps the role file Claude coordinators load at launch.
pub fn coordinator_role_file(paths: &GxserverPaths) -> PathBuf {
    paths
        .root_dir
        .join("coordinators")
        .join("coordinator-role.md")
}

/// Writes the role file when it is missing or out of date, so an upgrade reaches every
/// coordinator at its next launch or resume.
pub fn ensure_coordinator_role_file(paths: &GxserverPaths) -> std::io::Result<PathBuf> {
    let path = coordinator_role_file(paths);
    if std::fs::read_to_string(&path).ok().as_deref() != Some(COORDINATOR_ROLE_PROMPT) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, COORDINATOR_ROLE_PROMPT)?;
    }
    Ok(path)
}

/// True for the agent families a coordinator can run on.
pub fn coordinator_agent_family_supported(family: &str) -> bool {
    matches!(family, "claude" | "codex" | "zcode")
}

/// Appends the role flags to an agent command of the given family. ZCode's command stays
/// unchanged: its role reaches the agent through the SessionStart hook
/// (`ingest_agent_hook_event` answers it with the guide pointer).
pub fn with_coordinator_role(
    command: &str,
    family: &str,
    role_file: &Path,
) -> Result<String, DomainStateError> {
    let command = command.trim();
    match family {
        "claude" => Ok(format!(
            "{command} --append-system-prompt-file {}",
            crate::agents::quote_shell_arg(&role_file.to_string_lossy())
        )),
        "codex" => {
            let value = serde_json::to_string(GUIDE_POINTER_COORDINATOR_INSTRUCTIONS)
                .unwrap_or_else(|_| "\"\"".to_string());
            Ok(format!(
                "{command} -c {}",
                crate::agents::quote_shell_arg(&format!("developer_instructions={value}"))
            ))
        }
        "zcode" => Ok(command.to_string()),
        _ => Err(DomainStateError::bad_request(
            "A coordinator runs on Claude, Codex or ZCode. Pick one of those agents.",
        )),
    }
}
