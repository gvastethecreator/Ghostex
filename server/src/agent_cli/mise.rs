use super::{catalog::Definition, process};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
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

/// The backend `mise use` would install a tool with: an explicit backend as written, else the first backend
/// `mise registry` lists for this platform. `Some("")` means the registry has none that runs here.
fn backend(tool: &str, home: &Path) -> Option<String> {
    static CACHE: LazyLock<Mutex<HashMap<String, (Instant, String)>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    if tool.contains(':') {
        return Some(tool.to_string());
    }
    if let Some((_, backend)) = CACHE
        .lock()
        .ok()
        .and_then(|cache| cache.get(tool).cloned())
        .filter(|(checked, _)| checked.elapsed() < Duration::from_secs(600))
    {
        return Some(backend);
    }
    let mise = process::resolve("mise", home)?;
    let mut command = crate::platform::process::background_command(&mise);
    command
        .args(["registry", tool])
        .current_dir(home)
        .env("HOME", home);
    // A failed or slow probe says nothing about the registry, so it is not cached.
    let listed = crate::agent_hooks::probing::run_command_stdout_with_timeout(
        command,
        Duration::from_secs(3),
    )?;
    let backend = listed
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string();
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(tool.to_string(), (Instant::now(), backend.clone()));
    }
    Some(backend)
}

/// CDXC:AgentProviders 2026-10-07 WHY:
/// mise's registry can list a tool whose only backends do not run on this platform (cursor-agent's only backend is `http:cursor-agent`, which mise does not support on Windows), so `mise use` failed at once and the one-click install never reached the official installer. `mise registry <tool>` prints nothing for such a tool, which makes the mise method unavailable there. mise installs an `npm:` backend (Gemini's, the packages Ghostex derives from `npmPackage`) with npm, so that method also needs npm on this computer; the npm method installs Node.js itself instead.
pub(crate) fn unavailable_reason(tool: &str, home: &Path) -> Option<String> {
    let backend = backend(tool, home)?;
    if backend.is_empty() {
        let platform = match std::env::consts::OS {
            "windows" => "Windows",
            "macos" => "macOS",
            "linux" => "Linux",
            other => other,
        };
        return Some(format!(
            "mise cannot install {tool} on {platform}. Use the official installer instead."
        ));
    }
    (backend.starts_with("npm:") && !crate::agent_hooks::probing::command_exists("npm", home))
        .then(|| {
            format!("mise installs {tool} with npm, which is not on this computer. Use another method; npm installs Node.js first.")
        })
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
