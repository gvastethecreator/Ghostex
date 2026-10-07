use super::*;

/// Cursor's tail window. Claude activity scans the whole screen because queued
/// messages can push live status and tool rows away from the bottom.
pub(super) const CURSOR_ACTIVITY_SCAN_LINES: usize = 15;

/// Rows after the label that may carry the bar. Claude paints it on the very
/// next line; two leaves room for a wrap.
pub(super) const ACTIVITY_PERCENT_LOOKAHEAD: usize = 2;

/// Activity kind for Claude Code, Codex, Cursor and Grok compaction (manual and automatic), and Hermes's `/compress`.
pub const SESSION_CHAT_ACTIVITY_COMPACTING: &str = "compacting";

/// Claude Code's current assistant status, not yet flushed to transcript JSONL.
/// Since 2026-09-11 this is only the allowlisted star-marker rows; a `⏺`
/// prose row is a `agent-stream`.
pub const SESSION_CHAT_ACTIVITY_CLAUDE_STATUS: &str = "claude-status";

/*
CDXC:AgentScreenDetection 2026-09-11 DECISION:
User: while Claude Code streams a long reply, the chat view stayed empty until
the whole message reached the transcript JSONL, although the terminal was
already painting it. Read the reply off the terminal as it comes in (chunks
every second are enough), show it in the chat as the message being written,
and switch to the transcript's row the moment it is saved there.

The `⏺` row and every indented row under it, down to the next marker (the
spinner, a tool gutter, the next bullet, the composer), is the message Claude
is writing. A `⏺` row is therefore published as `agent-stream` with the whole
block as `text`, not as the first-paragraph `claude-status` it used to be. The
first paragraph stays in `label` so an older client still shows something.
The kind is agent-neutral on purpose: the client does the same thing with a
streamed reply whoever wrote it, so a Codex reader only needs its own block
grammar feeding the shared stitching, rendering, and retirement below.

Claude runs as a full-screen TUI here, so a message longer than the grid
scrolls its `⏺` row off the top and the capture only holds the tail. Each
probe therefore carries the painted rows and whether the bullet was on
screen; the detection funnel (`merge_agent_stream`) stitches a headless
sample onto the rows it accumulated from earlier probes by finding the rows
they share. A stream only ever STARTS from a visible bullet: a headless sample
with nothing to stitch onto is not evidence of a message (the top of the grid
can just as well be the tail of the user's prompt or of a thinking block).
*/
pub const SESSION_CHAT_ACTIVITY_AGENT_STREAM: &str = "agent-stream";

/// The row of a Claude Code tool call: the row directly above the `⎿` output
/// gutter, with or without its bullet. The client shows it as a pending tool
/// row at the bottom of the transcript, never as reasoning history — the
/// transcript writes the call itself once its result lands, and the screen
/// paints the description in a different form ("Reading …" for "Read …")
/// than the transcript stores, so the row could never be matched as prose.
pub const SESSION_CHAT_ACTIVITY_CLAUDE_TOOL: &str = "claude-tool";

/// Cursor Agent's live reasoning/composition row before its transcript flushes.
pub const SESSION_CHAT_ACTIVITY_CURSOR_THINKING: &str = "cursor-thinking";

/// A Claude Code background shell or monitor that remains live after the
/// assistant turn.
pub const SESSION_CHAT_ACTIVITY_SHELLS_RUNNING: &str = "shells-running";

/// Star frames Claude may use for allowlisted non-general status rows. Merely
/// having one of these markers is not sufficient evidence: custom working
/// spinner text uses the same frames and must never become chat history.
pub(super) const CLAUDE_SPECIAL_STATUS_MARKERS: &str = "✳✶✻✽✸✹✺✷✴";

/// The phrase Claude paints while compacting. Matched case-sensitively on the
/// space-collapsed line, so prose that merely mentions compaction cannot hit
/// it — the label only counts when it OWNS a line (see `activity_from_line`).
pub(super) const COMPACTING_LABEL: &str = "Compacting conversation";

/// What the client shows. `kind` is an open set, so a client that has never
/// heard of one still renders `label` plus whatever progress came with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionChatTerminalActivity {
    pub kind: &'static str,
    /// Agent-facing wording, without the spinner glyph or the clock.
    pub label: String,
    /// 0-100, only when the screen actually painted a percentage.
    pub percent: Option<u8>,
    /// Seconds the CLI reports it has been running, only when it painted them.
    pub elapsed_seconds: Option<u64>,
    /// RFC3339 millis. The client interpolates its own clock from this, so a
    /// 3s probe cadence still reads as a smoothly ticking timer.
    pub detected_at: String,
    /// The token counter Claude paints after the compaction clock
    /// (`↓ 901 tokens`), exactly as shown, only when it painted one.
    pub tokens: Option<String>,
    /// The tool block Claude painted under a `claude-tool` row (the `⎿` gutter
    /// and its continuation rows), exactly as shown on the terminal.
    pub detail: Option<String>,
    /// `agent-stream` only: the message text painted so far, stitched across
    /// probes when its bullet has scrolled off the grid.
    pub text: Option<String>,
    /// `agent-stream` only, never serialized: the painted rows this value was
    /// rendered from, kept so the next probe can be stitched onto them.
    pub stream: Option<AgentStreamRows>,
}

/// One painted row of a Claude message block, layout kept (see `ScreenRow`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentStreamRow {
    /// Trimmed row text.
    pub text: String,
    /// Leading spaces on the physical row.
    pub indent: usize,
    /// A blank row separated this row from the previous one.
    pub after_blank: bool,
    /// Every character of the row is painted bold.
    pub bold: bool,
}

/// The rows of one Claude message block as painted, and whether they start at
/// its `⏺` row or at the top of a grid the bullet has scrolled off.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentStreamRows {
    pub rows: Vec<AgentStreamRow>,
    pub head_visible: bool,
}

impl SessionChatTerminalActivity {
    pub(super) fn new(kind: &'static str, label: impl Into<String>) -> Self {
        Self {
            kind,
            label: label.into(),
            percent: None,
            elapsed_seconds: None,
            tokens: None,
            detected_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            detail: None,
            text: None,
            stream: None,
        }
    }

    /*
    Two samples of the SAME run, ignoring the numbers. Progress changing is not
    a new activity — if it were, the client's `detectedAt`-anchored clock would
    restart from zero on every probe and the timer would never advance past the
    poll interval.
    */
    pub fn same_activity(&self, other: Option<&SessionChatTerminalActivity>) -> bool {
        other.is_some_and(|other| self.kind == other.kind && self.label == other.label)
    }

    /// True when a re-detect says the same thing INCLUDING its numbers, i.e.
    /// there is nothing new to publish.
    pub fn unchanged(&self, other: Option<&SessionChatTerminalActivity>) -> bool {
        other.is_some_and(|other| {
            self.same_activity(Some(other))
                && self.percent == other.percent
                && self.elapsed_seconds == other.elapsed_seconds
                && self.tokens == other.tokens
                && self.detail == other.detail
                && self.text == other.text
        })
    }

    /*
    An ongoing run keeps its original `detectedAt`: it anchors the client's
    elapsed clock, so re-minting it every 3s would peg the timer at ~0s
    forever. Same instance-not-sample rule as a terminal notice's timestamp.
    */
    pub fn carry_forward_detected_at(&mut self, previous: Option<&SessionChatTerminalActivity>) {
        if let Some(previous) = previous.filter(|previous| self.same_activity(Some(previous))) {
            // Once a run has an elapsed baseline, keep that first sample with
            // its first timestamp. The client advances it locally; accepting
            // every later CLI clock sample as well would count the same time
            // twice. If elapsed first APPEARS later, that later sample needs
            // its own timestamp and becomes the baseline instead.
            if previous.elapsed_seconds.is_none() && self.elapsed_seconds.is_some() {
                return;
            }
            self.detected_at = previous.detected_at.clone();
            if self.elapsed_seconds.is_some() {
                self.elapsed_seconds = previous.elapsed_seconds;
            }
        }
    }

    pub fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("kind".to_string(), json!(self.kind));
        map.insert("label".to_string(), json!(self.label));
        if let Some(percent) = self.percent {
            map.insert("percent".to_string(), json!(percent));
        }
        if let Some(elapsed_seconds) = self.elapsed_seconds {
            map.insert("elapsedSeconds".to_string(), json!(elapsed_seconds));
        }
        if let Some(tokens) = &self.tokens {
            map.insert("tokens".to_string(), json!(tokens));
        }
        map.insert("detectedAt".to_string(), json!(self.detected_at));
        if let Some(detail) = &self.detail {
            map.insert("detail".to_string(), json!(detail));
        }
        if let Some(text) = &self.text {
            map.insert("text".to_string(), json!(text));
        }
        Value::Object(map)
    }

    /*
    CDXC:AgentScreenDetection 2026-09-02:
    Most terminal activity is stale scrollback once the main turn becomes
    ready: a `⏺` status row stays on the primary screen after Claude stops, so
    the hook-derived working flag is what proves it current. Two kinds are
    proven by the screen itself and must NOT be gated on that flag:

      - a background shell: Claude reports it precisely because that work
        remains live after the assistant turn has finished;
      - compaction: Claude repaints the `Compacting conversation` row in place
        and replaces it with its `Compacted` line when done, so a whole capture
        that still shows the row is proof the compaction is running, and a
        whole capture without it is proof it ended. Hooks are not a start
        authority here — `PreCompact` is deliberately unregistered and a
        `/compact` typed in the terminal is not proven to raise
        `UserPromptSubmit` — so gating on "working" hid a compaction the
        terminal was visibly showing;
      - a running `!` shell command (2026-10-07): its run-in-background hint
        is painted only while it runs, and Claude is not working meanwhile.
    */
    pub fn remains_live_when_ready(&self) -> bool {
        matches!(
            self.kind,
            SESSION_CHAT_ACTIVITY_SHELLS_RUNNING
                | SESSION_CHAT_ACTIVITY_COMPACTING
                | SESSION_CHAT_ACTIVITY_SHELL_COMMAND
        )
    }
}

/// Which activity a client may be told about given the hook-derived working
/// flag: everything while the agent is working, and only the screen-proven
/// kinds (see `remains_live_when_ready`) once it is ready.
pub fn publishable_session_chat_terminal_activity(
    working: bool,
    activity: Option<SessionChatTerminalActivity>,
) -> Option<SessionChatTerminalActivity> {
    activity.filter(|activity| working || activity.remains_live_when_ready())
}

/// CDXC:SessionStatus 2026-09-22 DECISION:
/// User: a Claude session whose footer reports a background shell still running gets its own sidebar indicator (a grey dot) rather than showing idle. This widens the 2026-09-06 decision, which counted only monitors, and both now feed the `backgroundWorkDetectedAt` presentation field through the persisted `sessionChatMonitorDetectedAt` marker instead of the working state.
pub(crate) fn is_session_chat_background_work_activity(
    activity: Option<&SessionChatTerminalActivity>,
) -> bool {
    activity.is_some_and(|activity| {
        activity.kind == SESSION_CHAT_ACTIVITY_SHELLS_RUNNING
            && activity
                .label
                .rsplit_once(" · ")
                .is_some_and(|(_, status)| {
                    status.split_once(' ').is_some_and(|(count, suffix)| {
                        count.parse::<u64>().is_ok_and(|count| count > 0)
                            && matches!(
                                suffix,
                                "shell still running"
                                    | "shells still running"
                                    | "monitor still running"
                                    | "monitors still running"
                            )
                    })
                })
    })
}

/// Follower reconciles (1s each) probed back-to-back after the transcript
/// records a command that starts long on-screen work. Claude paints the
/// compaction row within a second or two of the `/compact` record; eight
/// covers a slow first repaint without turning into a polling tier.
pub const SESSION_CHAT_ACTIVITY_COMMAND_PROBE_TICKS: u64 = 8;

/*
CDXC:AgentScreenDetection 2026-09-02:
The transcript is the one place BOTH ways of issuing `/compact` land: Claude
records `<command-name>/compact</command-name>` whether the bytes came from the
chat composer or were typed straight into the terminal. The follower keys its
fast re-probe off that row, so a terminal-typed compaction shows its card as
quickly as a chat-sent one, and the follower stays the single owner of what it
has published. Only Claude's user-role command envelope counts: prose that
mentions the command, tool output, and the `<local-command-stdout>` completion
row do not.
*/
pub fn transcript_message_starts_session_chat_activity(
    agent: Option<&str>,
    message: &crate::session_chat::SessionChatMessage,
) -> bool {
    if message.role != crate::session_chat::SessionChatRole::User {
        return false;
    }
    let text = crate::session_chat_decode_claude::message_text(message);
    let command = claude_command_envelope_name(&text).unwrap_or(text.as_str());
    crate::session_chat_options::is_session_chat_activity_command_text(agent, command)
}

/// `<command-name>/compact</command-name>…` → `/compact`. `None` for any text
/// that does not open with Claude's command envelope.
fn claude_command_envelope_name(text: &str) -> Option<&str> {
    const OPEN: &str = "<command-name>";
    const CLOSE: &str = "</command-name>";
    let rest = text.trim_start().strip_prefix(OPEN)?;
    let end = rest.find(CLOSE)?;
    Some(rest[..end].trim())
}

/// Change test for a value that can also disappear; an omitted field on a frame
/// means CLEARED, so present→absent is a change clients must be told about.
pub fn same_session_chat_terminal_activity(
    current: Option<&SessionChatTerminalActivity>,
    published: Option<&SessionChatTerminalActivity>,
) -> bool {
    match (current, published) {
        (None, None) => true,
        (Some(current), published) => current.unchanged(published),
        (None, Some(_)) => false,
    }
}

pub fn is_session_chat_compacting_activity(activity: Option<&SessionChatTerminalActivity>) -> bool {
    activity.is_some_and(|activity| activity.kind == SESSION_CHAT_ACTIVITY_COMPACTING)
}
