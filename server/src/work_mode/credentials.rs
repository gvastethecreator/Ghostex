//! The Linear API keys work mode reads Linear with: one shared key, plus an optional key per
//! project that overrides it.
//!
//! CDXC:WorkMode 2026-10-09 WHY:
//! gxserver has no OS credential store it can use on every platform (its own auth token is a
//! private file too), so the keys live in one private file next to that token: the auth folder is
//! 0700 and the file 0600 on macOS and Linux. They are never written to the settings file, never
//! published, and never returned by any endpoint.
//!
//! CDXC:WorkMode 2026-10-09 DECISION:
//! User: the Linear key is set once and shared by the projects, and a single project can override
//! it in its own settings.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde_json::{json, Map, Value};

use crate::paths::GxserverPaths;

const CREDENTIALS_FILE_NAME: &str = "work-mode-credentials.json";

fn credentials_path(paths: &GxserverPaths) -> PathBuf {
    paths.auth_dir.join(CREDENTIALS_FILE_NAME)
}

fn read_credentials(paths: &GxserverPaths) -> Map<String, Value> {
    fs::read_to_string(credentials_path(paths))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn write_credentials(
    paths: &GxserverPaths,
    credentials: &Map<String, Value>,
) -> std::io::Result<()> {
    fs::create_dir_all(&paths.auth_dir)?;
    let path = credentials_path(paths);
    let temp = path.with_extension("json.tmp");
    let _ = fs::remove_file(&temp);
    {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(Value::Object(credentials.clone()).to_string().as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(&temp, &path)
}

fn non_empty(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// The key a project's Linear calls use: its own override, else the shared key.
pub(crate) fn linear_api_key(paths: &GxserverPaths, project_id: Option<&str>) -> Option<String> {
    let credentials = read_credentials(paths);
    project_id
        .and_then(|project_id| {
            non_empty(
                credentials
                    .get("projectLinearApiKeys")
                    .and_then(|keys| keys.get(project_id)),
            )
        })
        .or_else(|| non_empty(credentials.get("linearApiKey")))
}

/// Stores (or, with `None`, removes) the shared key or one project's override.
pub(crate) fn store_linear_api_key(
    paths: &GxserverPaths,
    project_id: Option<&str>,
    api_key: Option<&str>,
) -> std::io::Result<()> {
    let mut credentials = read_credentials(paths);
    let api_key = api_key.map(str::trim).filter(|key| !key.is_empty());
    match project_id {
        Some(project_id) => {
            let mut keys = credentials
                .get("projectLinearApiKeys")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            match api_key {
                Some(key) => {
                    keys.insert(project_id.to_string(), json!(key));
                }
                None => {
                    keys.remove(project_id);
                }
            }
            credentials.insert("projectLinearApiKeys".to_string(), Value::Object(keys));
        }
        None => match api_key {
            Some(key) => {
                credentials.insert("linearApiKey".to_string(), json!(key));
            }
            None => {
                credentials.remove("linearApiKey");
            }
        },
    }
    write_credentials(paths, &credentials)
}

/// Which keys exist, without their values.
pub(crate) fn linear_api_key_summary(paths: &GxserverPaths) -> Value {
    let credentials = read_credentials(paths);
    let project_overrides: Vec<String> = credentials
        .get("projectLinearApiKeys")
        .and_then(Value::as_object)
        .map(|keys| {
            keys.iter()
                .filter(|(_, key)| non_empty(Some(key)).is_some())
                .map(|(project_id, _)| project_id.clone())
                .collect()
        })
        .unwrap_or_default();
    json!({
        "shared": non_empty(credentials.get("linearApiKey")).is_some(),
        "projectOverrides": project_overrides,
    })
}
