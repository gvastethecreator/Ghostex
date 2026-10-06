//! Empryo fork: a copy of the session folder under a new id, resumed with `empryo --session`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use super::*;
use crate::domain::DomainStateError;
use crate::session_chat_empryo_mirror::{is_safe_empryo_session_id, pending_prompts};

/// CDXC:SessionFork 2026-10-06 DECISION:
/// "Fork. Gated on a spike that copies the session folder under a new id, cuts `session.jsonl` at the chosen turn, drops `writer.lock` and resumes with `--session`. If it fails, fork stays off for Empryo, with no fallback." The spike passed on Empryo 3.9.0-beta (card agent-bo-95422942): the resumed copy answered from the kept turns only and the original's files stayed byte-identical. Ghostex's Fork has no turn picker, so the copy keeps every finished turn and drops a turn still in flight.
///
/// CDXC:SessionFork 2026-10-06 WHY:
/// Empryo replays history from `session.jsonl` and recomputes `messageRange`, but `--session` reports "Session not found" without a `meta.json`, so both files are written: the log first, then the index whose `id` names the folder. `~/.empryo/threads.db` registers the copy on its first resume and is never written here. `.lock/` and `prompt-prefix-*.json` stay behind; Empryo recreates them. The fork's name goes into Empryo's own record as a custom title, the same name the `/rename` path persists for Claude and Codex.
pub(crate) fn fork_empryo_session_folder(
    project: &Value,
    source_session: &Value,
    fork_id: &str,
    fork_title: &str,
) -> Result<(), DomainStateError> {
    let runtime = object_field(source_session, "runtimeSettings");
    let source_id = read_text_from_map(&runtime, "agentSessionId")
        .filter(|id| is_safe_empryo_session_id(id))
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "This Empryo session has no saved conversation to fork yet.",
            )
        })?;
    let source_dir =
        empryo_session_dir(project, source_session, &runtime, &source_id).ok_or_else(|| {
            DomainStateError::bad_request(format!(
                "Empryo's saved conversation {source_id} was not found, so it cannot be forked."
            ))
        })?;
    let fork_dir = source_dir.with_file_name(fork_id);
    let log = fs::read_to_string(source_dir.join("session.jsonl")).map_err(io_error)?;
    let meta: Value = fs::read_to_string(source_dir.join("meta.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .ok_or_else(|| {
            DomainStateError::bad_request("Empryo's session index (meta.json) is unreadable.")
        })?;
    let fork_log = fork_session_log(&log, fork_id, fork_title).ok_or_else(|| {
        DomainStateError::bad_request(
            "Fork is only available after an Empryo turn has finished. Wait for its reply, then fork it.",
        )
    })?;
    let fork_meta = fork_session_meta(meta, &source_id, fork_id, fork_title);
    let meta_text = serde_json::to_string_pretty(&fork_meta)
        .map_err(|error| DomainStateError::corrupt_state(error.to_string()))?;
    create_private_dir(&fork_dir).map_err(io_error)?;
    let written = write_private_file(&fork_dir.join("session.jsonl"), fork_log.as_bytes())
        .and_then(|()| write_private_file(&fork_dir.join("meta.json"), meta_text.as_bytes()));
    if let Err(error) = written {
        // The folder was created by this call (`create` refuses an existing one), so it holds only
        // this fork's half-written files.
        let _ = fs::remove_dir_all(&fork_dir);
        return Err(io_error(error));
    }
    Ok(())
}

/// `<root>/.empryo/sessions/<id>`: the folder the hook-reported log sits in, else the one under the
/// session's working folder, which is where `empryo --session` looks.
fn empryo_session_dir(
    project: &Value,
    session: &Value,
    runtime: &Map<String, Value>,
    source_id: &str,
) -> Option<PathBuf> {
    let from_hook = runtime
        .get("agentSessionPath")
        .and_then(Value::as_str)
        .map(Path::new)
        .and_then(Path::parent)
        .map(Path::to_path_buf);
    let from_cwd = read_text_value(session, "cwd")
        .or_else(|| read_text_value(project, "path"))
        .map(|cwd| Path::new(&cwd).join(".empryo/sessions").join(source_id));
    [from_hook, from_cwd].into_iter().flatten().find(|dir| {
        dir.file_name().and_then(|name| name.to_str()) == Some(source_id)
            && dir.join("meta.json").is_file()
    })
}

/// The source log up to its last finished turn, with the header record renamed to the fork and the
/// fork's name appended; every other kept line is copied byte for byte. Turns start and end by the
/// chat mirror's rule (`CDXC:SessionChat` in `session_chat_empryo_mirror.rs`), so the fork keeps the
/// turns the chat shows as finished; anything after the last one that starts another turn is cut,
/// as is a torn last line. `None` when no turn has finished.
fn fork_session_log(log: &str, fork_id: &str, fork_title: &str) -> Option<String> {
    let lines: Vec<(&str, RecordHead)> = log
        .split_inclusive('\n')
        .take_while(|line| line.ends_with('\n'))
        .map_while(|line| Some((line, serde_json::from_str::<RecordHead>(line).ok()?)))
        .collect();
    let last_reply = lines.iter().rposition(|(_, record)| record.ends_turn())?;
    let end = lines[last_reply + 1..]
        .iter()
        .position(|(_, record)| record.starts_turn())
        .map_or(lines.len(), |offset| last_reply + 1 + offset);
    let (header_line, _) = lines.first().filter(|(_, record)| record.k == "meta")?;
    let mut header: Value = serde_json::from_str(header_line).ok()?;
    header["id"] = json!(fork_id);
    let next_seq = lines[..end]
        .iter()
        .filter_map(|(_, record)| record.seq)
        .max()
        .unwrap_or(0)
        + 1;
    let title = json!({
        "k": "title",
        "title": fork_title,
        "custom": true,
        "seq": next_seq,
        "ts": now_ms(),
    });
    let mut fork_log = format!("{header}\n");
    lines[1..end]
        .iter()
        .for_each(|(line, _)| fork_log.push_str(line));
    fork_log.push_str(&format!("{title}\n"));
    Some(fork_log)
}

/// The fields of one `session.jsonl` record the cut reads; the large `ui`/`core` copies are skipped.
#[derive(serde::Deserialize)]
struct RecordHead {
    #[serde(default)]
    k: String,
    seq: Option<u64>,
    status: Option<String>,
    patch: Option<Value>,
}

impl RecordHead {
    fn starts_turn(&self) -> bool {
        self.k == "user"
            || (self.k == "tab"
                && self
                    .patch
                    .as_ref()
                    .and_then(pending_prompts)
                    .is_some_and(|pending| !pending.is_empty()))
    }

    fn ends_turn(&self) -> bool {
        self.k == "assistant" && matches!(self.status.as_deref(), Some("complete" | "partial"))
    }
}

fn fork_session_meta(mut meta: Value, source_id: &str, fork_id: &str, fork_title: &str) -> Value {
    meta["id"] = json!(fork_id);
    meta["title"] = json!(fork_title);
    meta["customTitle"] = json!(fork_title);
    if let Some(tabs) = meta.get_mut("tabs").and_then(Value::as_array_mut) {
        for tab in tabs {
            if tab.get("sessionId").and_then(Value::as_str) == Some(source_id) {
                tab["sessionId"] = json!(fork_id);
            }
        }
    }
    meta
}

#[cfg(unix)]
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new().mode(0o700).create(dir)
}

#[cfg(not(unix))]
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    fs::create_dir(dir)
}

fn write_private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(bytes)
}

fn io_error(error: std::io::Error) -> DomainStateError {
    DomainStateError::bad_request(format!("Empryo fork failed: {error}"))
}
