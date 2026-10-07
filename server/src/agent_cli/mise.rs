use super::{catalog::Definition, process};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub(crate) struct Installation {
    pub tool: Option<String>,
    pub executable: String,
}

/// CDXC:AgentProviders 2026-09-14 DECISION:
/// User: support mise for CLI installs and updates; most agents on their computer are managed by mise.
/// WHY: Ask mise for the tool owning a binary, including custom backends, before classifying npm packages or native binaries by their paths.
/// A Node runtime managed by mise does not make its global npm packages separate mise tools.
/// ZCode publishes hyphenated release versions; its catalog entry enables mise prereleases for that package so latest resolves to a published version.
pub(crate) fn installation(
    definition: &Definition,
    executable: Option<&str>,
    home: &Path,
) -> Option<Installation> {
    let mise = process::resolve("mise", home)?;
    let path = query(&mise, &["which", &definition.binary], home)?;
    let tool = query(&mise, &["which", &definition.binary, "--plugin"], home)?;
    if tool.starts_with('-')
        || !tool
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"@:/._-".contains(&byte))
    {
        return None;
    }
    if let Some(executable) = executable {
        let actual = canonical(executable);
        let expected = canonical(&path);
        // Activated shells can keep a previous mise version's directory on PATH until their next prompt.
        let normalized = path.replace('\\', "/");
        let same_install = normalized
            .split_once("/installs/")
            .is_some_and(|(base, rest)| {
                let tool_dir = rest.split('/').next().unwrap_or_default();
                actual.starts_with(Path::new(base).join("installs").join(tool_dir))
            });
        if actual != expected && !is_shim(executable, &mise) && !same_install {
            return None;
        }
    }
    Some(Installation {
        tool: (!matches!(
            tool.rsplit(':').next(),
            Some("node" | "bun" | "pnpm" | "npm")
        ))
        .then_some(tool),
        executable: crate::platform::live_path::runnable(&path),
    })
}

fn canonical(path: &str) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.into())
}

fn is_shim(path: &str, mise: &str) -> bool {
    let normalized = path.replace('\\', "/").to_lowercase();
    normalized.contains("/mise/shims/") || canonical(path) == canonical(mise)
}

pub(crate) fn looks_managed(path: &str) -> bool {
    let original = path.replace('\\', "/").to_lowercase();
    let real = canonical(path)
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase();
    if matches!(real.rsplit('/').next(), Some("mise" | "mise.exe")) {
        return true;
    }
    [original, real].iter().any(|path| {
        (path.contains("/mise/installs/")
            && !["node", "bun", "pnpm", "npm"]
                .iter()
                .any(|runtime| path.contains(&format!("/installs/{runtime}/"))))
            || path.contains("/mise/shims/")
    })
}

fn query(mise: &str, args: &[&str], home: &Path) -> Option<String> {
    let mut command = crate::platform::process::background_command(mise);
    command.args(args).current_dir(home).env("HOME", home);
    #[cfg(not(windows))]
    command.env(
        "PATH",
        crate::agent_hooks::probing::normalize_gxserver_process_path(
            std::env::var("PATH").ok().as_deref(),
            home,
        ),
    );
    crate::agent_hooks::probing::run_command_stdout_with_timeout(command, Duration::from_secs(3))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && !value.contains(['\r', '\n']))
}

pub(crate) fn command(tool: &str, installed: bool) -> String {
    if installed {
        // Keep older versions available to agents and subprocesses that are already running.
        format!(
            "mise upgrade --bump --no-prune --yes {}",
            process::quote(tool)
        )
    } else {
        format!(
            "mise use --global --yes {}",
            process::quote(&format!("{tool}@latest"))
        )
    }
}
