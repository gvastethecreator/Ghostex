//! CDXC:Release 2026-09-22 DECISION:
//! User: Linux does not update itself as a first step because there are too many package managers; the user only needs to learn about the new version from the Update button before the project name and read its changelog in the same Ghostex Update dialog Windows shows, with a button that opens the download page.
//! User: the version and changelog are read from the Windows x64 Velopack feed already attached to every stable GitHub release, rather than from a Linux feed of its own.
//! SEE-ALSO: apps/desktop/src/app/os_integration/updater.rs (the check loop and the dialog actions), apps/desktop/src/app/window/update_available_modal.rs (the `Notify` state), tooling/release-gpui/windows.ps1 (cuts the CHANGELOG.md section into the feed's `NotesMarkdown`).
use std::io::Read as _;
use std::path::Path;
use std::time::Duration;

const RELEASE_FEED_URL: &str =
    "https://github.com/maddada/Ghostex/releases/latest/download/releases.win-x64-stable.json";
const RELEASE_PAGE_URL_PREFIX: &str = "https://github.com/maddada/Ghostex/releases/tag/v";
const PACKAGED_INSTALL_ROOT: &str = "/opt/ghostex";
const FEED_MAX_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LinuxUpdate {
    version: String,
    notes_markdown: String,
}

impl LinuxUpdate {
    pub(crate) fn version(&self) -> &str {
        &self.version
    }

    pub(crate) fn notes_markdown(&self) -> &str {
        &self.notes_markdown
    }

    pub(crate) fn release_page_url(&self) -> String {
        format!("{RELEASE_PAGE_URL_PREFIX}{}", self.version)
    }
}

/// Every Linux package (deb, rpm, tarball, AUR) installs under `/opt/ghostex`; a cargo build does not and runs without update checks, like a Windows build without its Velopack manifest.
pub(crate) fn is_packaged_install() -> bool {
    std::env::current_exe().is_ok_and(|exe| exe.starts_with(Path::new(PACKAGED_INSTALL_ROOT)))
}

/// The newest stable release when it is newer than this build. Blocking: call it off the main thread.
pub(crate) fn check_for_updates() -> Result<Option<LinuxUpdate>, String> {
    let tls_config = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .tls_config(tls_config)
        .build();
    let mut response = ureq::Agent::new_with_config(config)
        .get(RELEASE_FEED_URL)
        .call()
        .map_err(|error| error.to_string())?;
    let mut body = String::new();
    response
        .body_mut()
        .as_reader()
        .take(FEED_MAX_BYTES)
        .read_to_string(&mut body)
        .map_err(|error| error.to_string())?;
    newest_update(&body, env!("GHOSTEX_BUILD_MARKETING_VERSION"))
}

fn newest_update(feed_text: &str, current_version: &str) -> Result<Option<LinuxUpdate>, String> {
    let feed: serde_json::Value =
        serde_json::from_str(feed_text).map_err(|error| error.to_string())?;
    let assets = feed
        .get("Assets")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "release feed has no Assets array".to_string())?;
    let current = parse_version(current_version)
        .ok_or_else(|| format!("unreadable build version {current_version}"))?;
    // Delta entries carry no notes; only the full package does.
    let newest = assets
        .iter()
        .filter(|asset| {
            asset
                .get("Type")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|kind| kind.eq_ignore_ascii_case("full"))
        })
        .filter_map(|asset| {
            let version = asset.get("Version")?.as_str()?;
            Some((parse_version(version)?, version, asset))
        })
        .max_by_key(|(parsed, _, _)| *parsed);
    Ok(newest
        .filter(|(parsed, _, _)| *parsed > current)
        .map(|(_, version, asset)| LinuxUpdate {
            version: version.to_string(),
            notes_markdown: asset
                .get("NotesMarkdown")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
        }))
}

/// Stable releases are plain `major.minor.patch`; anything else is not a stable version and is skipped.
fn parse_version(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.trim().split('.');
    let parsed = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(parsed)
}
