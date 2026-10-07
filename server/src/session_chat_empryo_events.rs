//! Empryo's events, read off its screen while a turn runs and kept beside the chat mirror.
//!
//! CDXC:SessionChat 2026-10-07 DECISION:
//! Sven asked for Empryo's events in the chat; the ones Empryo 3.9.1-beta never records (the Genome waits, "ChatGPT is safety-buffering …") may be read from the session's own screen, robust to 3.9.1's layout and never duplicated on re-reads.
//!
//! CDXC:SessionChat 2026-10-07 WHY:
//! A turn's events sit in its activity strand under the reply's header (`2 actions · 6 events ▾`), one tree row each (`├─ ▲ 󰚄 Genome not ready after 120s, …`), interleaved with its action rows by time; the strand is open while the turn runs. Action rows pad their label into a column (`Ran         ls`) and events are prose, which is how the two are told apart. Older events fold into one `+N completed · Click to expand` row as newer ones arrive, so the strand is read every few seconds (session_chat_fleet_status.rs) and each event is kept the first time it shows, with the newest call started above it as its place. The screen is read only while the session's followed tab has a turn in flight, and only when its newest prompt is that turn's, so another tab's events are never taken. Effort changes are left out here: the tab's log records them (session_chat_empryo_notes.rs).
//! SEE-ALSO: server/src/session_chat_empryo_mirror.rs, server/src/session_chat_empryo_notes.rs

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::DomainRepository;
use crate::session_chat_empryo_mirror::EmpryoInFlight;
use crate::session_chat_options::{is_nerd_font_icon, strip_ansi_sgr};

/// One event as the strand showed it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EmpryoScreenEvent {
    pub(crate) text: String,
    /// `warning` (`▲`) or `error` (`✗`); absent for a plain event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) severity: Option<String>,
    /// The newest of the turn's calls that had started above it, or none when it came first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) after: Option<String>,
    pub(crate) seen_at: i64,
}

impl EmpryoScreenEvent {
    /// The notice text, marked the way Empryo marks a warning or a failure.
    pub(crate) fn notice_text(&self) -> String {
        match self.severity.as_deref() {
            Some("warning") if !self.text.starts_with('\u{26a0}') => {
                format!("\u{26a0} {}", self.text)
            }
            Some("error") => format!("\u{2717} {}", self.text),
            _ => self.text.clone(),
        }
    }
}

/// The events kept for each turn of one mirror, keyed by turn.
pub(crate) type EmpryoScreenEvents = HashMap<String, Vec<EmpryoScreenEvent>>;

pub(crate) fn empryo_screen_events_path(mirror_path: &Path) -> PathBuf {
    mirror_path.with_extension("events.json")
}

pub(crate) fn read_empryo_screen_events(mirror_path: &Path) -> EmpryoScreenEvents {
    fs::read(empryo_screen_events_path(mirror_path))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// An event row of a strand, with how many action rows the strand shows below it.
#[derive(Debug, PartialEq)]
struct StrandEvent {
    text: String,
    severity: Option<&'static str>,
    actions_below: usize,
}

/// The prompt header of a turn: `◆  You · 02:10 PM`.
fn is_prompt_header(line: &str) -> bool {
    line.trim_start_matches(|ch: char| !ch.is_alphanumeric())
        .starts_with("You \u{b7} ")
}

/// A strand's summary row: `2 actions · 6 events ▾`, `1 action ▾`.
fn is_strand_summary(line: &str) -> bool {
    line.starts_with(|ch: char| ch.is_ascii_digit())
        && line
            .split(" \u{b7} ")
            .next()
            .is_some_and(|first| first.ends_with(" action") || first.ends_with(" actions"))
}

fn collapse_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether the prompt the screen shows under its newest header is the turn's: the shown line can
/// be cut at the window's width, so the longer one has to start with the shorter.
fn shows_prompt(shown: &str, prompt: &str) -> bool {
    let shown = collapse_spaces(shown.trim_end_matches('\u{2026}'));
    let prompt = collapse_spaces(prompt.lines().next().unwrap_or_default());
    !shown.is_empty()
        && !prompt.is_empty()
        && (prompt.starts_with(&shown) || shown.starts_with(&prompt))
}

/// One strand row after its tree connector: whether it has finished, its status glyph's
/// severity, and its text with the icons gone and the label column (two or more spaces) kept.
///
/// A row still running (`…` or a spinner: "Waiting for subscriptions/gpt-6.1-sol · 2s",
/// "Thinking") is live status that Empryo drops once it ends; only a finished event (`✓`, `▲`,
/// `✗`) stays in its strand.
fn strand_row(row: &str) -> (bool, Option<&'static str>, String) {
    let row = row.trim_start();
    let first = row.chars().next();
    let finished = matches!(first, Some('\u{2713}' | '\u{25b2}' | '\u{2717}'));
    let severity = match first {
        Some('\u{25b2}') => Some("warning"),
        Some('\u{2717}') => Some("error"),
        _ => None,
    };
    let text: String = row.chars().filter(|ch| !is_nerd_font_icon(*ch)).collect();
    let text = text
        .trim_start_matches(|ch: char| !ch.is_alphanumeric() && ch != '+')
        .trim_end()
        .to_string();
    (finished, severity, text)
}

/// An action or thought row: a short label padded into a column (`Ran         ls`,
/// `Synthesized  · 15 lines`). Events are prose with single spaces.
fn row_label(text: &str) -> Option<&str> {
    let gap = text.find("  ")?;
    let label = &text[..gap];
    (gap <= 16 && label.split(' ').count() <= 2).then_some(label)
}

/// The events of the strands under the screen's newest prompt, when that prompt is `prompt`.
fn strand_events(screen: &str, prompt: &str) -> Vec<StrandEvent> {
    let lines: Vec<String> = screen.lines().map(strip_ansi_sgr).collect();
    let Some(header) = lines.iter().rposition(|line| is_prompt_header(line)) else {
        return Vec::new();
    };
    let below = &lines[header + 1..];
    let Some(shown) = below
        .iter()
        .map(|line| line.trim())
        .find(|line| !line.is_empty())
    else {
        return Vec::new();
    };
    if !shows_prompt(shown, prompt) {
        return Vec::new();
    }
    // (first line, continuation) of every strand row, top to bottom, across the turn's strands.
    let mut rows: Vec<(String, String)> = Vec::new();
    let mut in_strand = false;
    for line in below {
        let trimmed = line.trim();
        if is_strand_summary(trimmed) {
            in_strand = true;
            continue;
        }
        if !in_strand {
            continue;
        }
        if let Some(row) = trimmed
            .strip_prefix("\u{251c}\u{2500}")
            .or_else(|| trimmed.strip_prefix("\u{2570}\u{2500}"))
        {
            rows.push((row.to_string(), String::new()));
        } else if let Some(more) = trimmed.strip_prefix('\u{2502}') {
            if let Some((_, continuation)) = rows.last_mut() {
                continuation.push(' ');
                continuation.push_str(more.trim());
            }
        } else {
            in_strand = false;
        }
    }
    let parsed: Vec<(bool, Option<&'static str>, String, Option<String>)> = rows
        .iter()
        .map(|(first, continuation)| {
            let (finished, severity, text) = strand_row(first);
            let label = row_label(&text).map(str::to_string);
            (finished, severity, format!("{text}{continuation}"), label)
        })
        .collect();
    let is_action =
        |label: &Option<String>| label.as_deref().is_some_and(|label| label != "Thought");
    parsed
        .iter()
        .enumerate()
        .filter(|(_, (finished, _, text, label))| {
            *finished
                && label.is_none()
                && !text.is_empty()
                // The fold of older events, whose rows showed on their own earlier.
                && !text.starts_with('+')
                // The tab's log records effort changes (session_chat_empryo_notes.rs).
                && !text.starts_with("Effort:")
        })
        .map(|(index, (_, severity, text, _))| StrandEvent {
            text: collapse_spaces(text),
            severity: *severity,
            actions_below: parsed[index + 1..]
                .iter()
                .filter(|(_, _, _, label)| is_action(label))
                .count(),
        })
        .collect()
}

/// Keeps the events the screen shows for the turn in flight that it does not hold yet. Returns
/// whether it kept any.
fn keep_events(
    mirror_path: &Path,
    in_flight: &EmpryoInFlight,
    seen: Vec<StrandEvent>,
    now: i64,
) -> bool {
    let mut events = read_empryo_screen_events(mirror_path);
    let kept = events.entry(in_flight.key.clone()).or_default();
    let mut changed = false;
    for event in seen {
        // A wrapped row can be read before its next line is drawn; the whole row replaces the cut
        // one it starts with, and a cut read of a row already kept adds nothing.
        if let Some(known) = kept.iter_mut().find(|known| {
            event.text.starts_with(&known.text) || known.text.starts_with(&event.text)
        }) {
            if event.text.len() > known.text.len() {
                known.text = event.text;
                changed = true;
            }
            continue;
        }
        let started = in_flight.calls.len();
        kept.push(EmpryoScreenEvent {
            text: event.text,
            severity: event.severity.map(str::to_string),
            after: started
                .checked_sub(event.actions_below + 1)
                .map(|at| in_flight.calls[at].clone()),
            seen_at: now,
        });
        changed = true;
    }
    if !changed {
        return false;
    }
    let path = empryo_screen_events_path(mirror_path);
    let temp = path.with_extension("json.tmp");
    serde_json::to_vec(&events).ok().is_some_and(|bytes| {
        fs::File::create(&temp)
            .and_then(|mut file| file.write_all(&bytes))
            .and_then(|()| fs::rename(&temp, &path))
            .is_ok()
    })
}

/// One look at a running Empryo session's screen (the fleet status pass, every few seconds).
pub(crate) fn observe_empryo_screen_events(repository: &DomainRepository<'_>, session: &Value) {
    let Some(log) = crate::session_chat_pi_models::empryo_session_log(repository, session) else {
        return;
    };
    let Some(mirror_path) =
        crate::session_chat_empryo_mirror::resolve_empryo_chat_transcript_path(None, Some(&log))
    else {
        return;
    };
    let Some(in_flight) = crate::session_chat_empryo_mirror::empryo_turn_in_flight(&mirror_path)
    else {
        return;
    };
    let Some(Ok(capture)) = session
        .get("zmxName")
        .and_then(Value::as_str)
        .map(crate::zmx::read_zmx_session_screen_capture)
    else {
        return;
    };
    if capture.truncated {
        return;
    }
    let seen = strand_events(&capture.text, &in_flight.prompt);
    if seen.is_empty() {
        return;
    }
    if keep_events(&mirror_path, &in_flight, seen, crate::server::now_ms()) {
        crate::session_chat_empryo_mirror::sync_empryo_transcript_mirror_for_path(&mirror_path);
    }
}
