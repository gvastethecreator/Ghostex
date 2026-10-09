//! Where a Slack request's `cloud` session runs.
//!
//! CDXC:TeamSync 2026-10-09 DECISION:
//! User: "cloud" means Claude Code on the web for now, but the cloud runner must stay swappable so
//! the team's own cloud computer can be added later. Every cloud start goes through
//! `CloudRunner`; `cloud_runner()` is the one place that picks the implementation.

use std::io::Read;
use std::path::Path;
use std::process::{Child, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::platform::process::background_command;

/// How long a start may take before it counts as failed.
const START_TIMEOUT: Duration = Duration::from_secs(180);
/// After the session URL appears, how long the CLI gets to finish on its own.
const EXIT_GRACE: Duration = Duration::from_secs(3);

pub(crate) struct CloudStartRequest<'a> {
    /// The project's folder; its Git remote tells the runner which repo to work on.
    pub(crate) repo_dir: &'a Path,
    /// The first message, ticket and thread included.
    pub(crate) prompt: &'a str,
}

pub(crate) struct CloudSession {
    pub(crate) url: String,
}

pub(crate) trait CloudRunner {
    /// Stable name recorded with the session (`claude-code-web`).
    fn name(&self) -> &'static str;
    /// Starts a session and returns where to open it. Blocking.
    fn start(&self, request: &CloudStartRequest<'_>) -> Result<CloudSession, String>;
}

pub(crate) fn cloud_runner() -> Box<dyn CloudRunner + Send + Sync> {
    Box::new(ClaudeCodeOnTheWeb)
}

/// `claude --cloud "<prompt>"` in the project's folder: Claude Code creates a session on
/// claude.ai/code for that repo with the prompt as its first message and prints its URL.
///
/// CDXC:TeamSync 2026-10-09 WHY:
/// The flag was `--remote` in earlier Claude Code releases; 2.1.295 on the user's machine lists
/// `--cloud [description|session_id|url]` ("Create a cloud session with the given description").
/// It runs with the requester's own Claude login, which is why the command runs on the
/// requester's computer and not in Convex.
struct ClaudeCodeOnTheWeb;

impl CloudRunner for ClaudeCodeOnTheWeb {
    fn name(&self) -> &'static str {
        "claude-code-web"
    }

    fn start(&self, request: &CloudStartRequest<'_>) -> Result<CloudSession, String> {
        let mut command = background_command(claude_program());
        command
            .arg("--cloud")
            .arg(request.prompt)
            .current_dir(request.repo_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|error| {
            format!(
                "Could not run Claude Code ({error}). Is the `claude` CLI installed and signed in?"
            )
        })?;
        let output = collect_output(&mut child);
        let started = Instant::now();
        let mut text = String::new();
        let mut url_seen_at: Option<Instant> = None;
        loop {
            while let Ok(chunk) = output.try_recv() {
                text.push_str(&chunk);
            }
            if url_seen_at.is_none() && session_url(&text).is_some() {
                url_seen_at = Some(Instant::now());
            }
            let exited = child.try_wait().ok().flatten();
            if let Some(status) = exited {
                // Drain what the readers still hold.
                std::thread::sleep(Duration::from_millis(100));
                while let Ok(chunk) = output.try_recv() {
                    text.push_str(&chunk);
                }
                return match session_url(&text) {
                    Some(url) => Ok(CloudSession { url }),
                    None => Err(format!(
                        "Claude Code did not start a cloud session (exit {}): {}",
                        status.code().unwrap_or(-1),
                        last_lines(&text)
                    )),
                };
            }
            if let Some(seen) = url_seen_at {
                if seen.elapsed() >= EXIT_GRACE {
                    // The session lives on claude.ai; the local CLI only attached to it.
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(CloudSession {
                        url: session_url(&text).unwrap_or_default(),
                    });
                }
            }
            if started.elapsed() >= START_TIMEOUT {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "Claude Code did not report a cloud session within {}s: {}",
                    START_TIMEOUT.as_secs(),
                    last_lines(&text)
                ));
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

fn claude_program() -> &'static str {
    if cfg!(windows) {
        "claude.exe"
    } else {
        "claude"
    }
}

/// Stdout and stderr chunks as they arrive.
fn collect_output(child: &mut Child) -> mpsc::Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    let readers: Vec<Box<dyn Read + Send>> = [
        child
            .stdout
            .take()
            .map(|out| Box::new(out) as Box<dyn Read + Send>),
        child
            .stderr
            .take()
            .map(|err| Box::new(err) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    .collect();
    for mut reader in readers {
        let sender = sender.clone();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 4096];
            while let Ok(read) = reader.read(&mut buffer) {
                if read == 0
                    || sender
                        .send(String::from_utf8_lossy(&buffer[..read]).to_string())
                        .is_err()
                {
                    break;
                }
            }
        });
    }
    receiver
}

/// The first `https://claude.ai/code/…` URL in the CLI's output.
fn session_url(text: &str) -> Option<String> {
    let start = text.find("https://claude.ai/code/")?;
    let url: String = text[start..]
        .chars()
        .take_while(|character| {
            !character.is_whitespace() && !matches!(character, '"' | '\'' | ')' | '>' | '\u{1b}')
        })
        .collect();
    Some(url.trim_end_matches(['.', ',']).to_string())
}

fn last_lines(text: &str) -> String {
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let tail = &lines[lines.len().saturating_sub(4)..];
    if tail.is_empty() {
        "no output".to_string()
    } else {
        tail.join(" / ")
    }
}
