use super::*;

/// Activity kind for a `!` shell command the user ran in Claude Code that is still running: `label`
/// is the command, `detail` the output painted under it so far.
pub const SESSION_CHAT_ACTIVITY_SHELL_COMMAND: &str = "shell-command";

/// The hint Claude paints under a `!` command only while it runs.
const CLAUDE_SHELL_RUNNING_HINT: &str = "(ctrl+b to run in background)";

/// The gutter text Claude paints before a running command has printed anything (`Running… (14s)`).
const CLAUDE_SHELL_RUNNING_LABEL: &str = "Running…";

/// Columns between the gutter row's indent and its text (`⎿` and two spaces), where the output's
/// own rows start.
const CLAUDE_TOOL_OUTPUT_CONTENT_OFFSET: usize = 3;

/*
CDXC:SessionChat 2026-10-07 DECISION:
User: "do the live output: while a ! command runs, show its output in the running card as it appears on the terminal (gxserver reads the running block off the screen), so a URL that aws login prints shows right away. Keep the recorded card replacing it at the end with no duplicate." Claude writes a `!` command to its transcript only once it has finished, so an interactive `! aws login` printed its sign-in URL on the terminal alone. While it runs Claude paints it as

    !  aws login
      ⎿  Attempting to open your default browser. …
         https://eu-north-1.signin.aws.amazon.com/v1/authorize?response_type=code&…
         (18s)
         (ctrl+b to run in background)

and the hint row exists only while the command runs, so the block is screen-proven like a background shell (`remains_live_when_ready`): Claude is not "working" while a `!` command runs, and the working gate would hide it. Rows Claude hard-wrapped at the grid width are joined back without a break, so a long URL arrives whole and still opens as one link.
SEE-ALSO: packages/gx-chat-core/src/session/terminal.rs and composition.rs (the running Shell card that shows this output).
*/
pub(super) fn claude_shell_command_activity(
    rows: &[ScreenRow],
) -> Option<SessionChatTerminalActivity> {
    let hint = rows
        .iter()
        .rposition(|row| row.text == CLAUDE_SHELL_RUNNING_HINT)?;
    let gutter = rows[..hint]
        .iter()
        .rposition(|row| row.text.starts_with(CLAUDE_TOOL_OUTPUT_MARKER))?;
    let gutter_indent = rows[gutter].indent;
    // Everything between the gutter and the hint belongs to the command's block.
    if rows[gutter + 1..hint]
        .iter()
        .any(|row| row.indent <= gutter_indent)
    {
        return None;
    }
    // The `!` row, walked back over the rows its command wrapped onto.
    let mut start = gutter.checked_sub(1)?;
    while start > 0
        && !rows[start].text.starts_with('!')
        && rows[start].indent >= CLAUDE_STATUS_CONTINUATION_INDENT
        && !rows[start].after_blank
    {
        start -= 1;
    }
    if !rows[start].text.starts_with('!') || rows[start].indent != 0 {
        return None;
    }
    let width = screen_width(rows);
    let command = join_wrapped_rows(&rows[start..gutter], width, " ", None);
    let command = command.strip_prefix('!')?.trim().to_string();
    if command.is_empty() {
        return None;
    }

    let mut output_rows: Vec<ScreenRow> = rows[gutter..hint]
        .iter()
        .map(|row| ScreenRow {
            text: row.text.clone(),
            indent: row.indent,
            after_blank: row.after_blank,
            end: row.end,
            bold: row.bold,
        })
        .collect();
    output_rows[0].text = output_rows[0]
        .text
        .trim_start_matches(CLAUDE_TOOL_OUTPUT_MARKER)
        .trim()
        .to_string();
    let mut elapsed_seconds = None;
    if let Some(clock) = output_rows.last().and_then(|row| {
        hidden_lines_clock(&row.text).or_else(|| parenthesized_elapsed_seconds(&row.text))
    }) {
        elapsed_seconds = Some(clock);
        output_rows.pop();
    }
    if let Some(first) = output_rows.first() {
        if let Some(rest) = first.text.strip_prefix(CLAUDE_SHELL_RUNNING_LABEL) {
            elapsed_seconds =
                elapsed_seconds.or_else(|| parenthesized_elapsed_seconds(rest.trim()));
            output_rows.remove(0);
        }
    }
    let output = join_wrapped_rows(
        &output_rows,
        width,
        "\n",
        Some(gutter_indent + CLAUDE_TOOL_OUTPUT_CONTENT_OFFSET),
    );

    let mut activity =
        SessionChatTerminalActivity::new(SESSION_CHAT_ACTIVITY_SHELL_COMMAND, command);
    activity.elapsed_seconds = elapsed_seconds;
    activity.detail = (!output.trim().is_empty()).then_some(output);
    Some(activity)
}

/// `… +1 lines (1m 5s)`: the row Claude paints instead of the bare clock once it has cut the top of
/// a long output. The lines it hid are not on the screen, so the row is screen chrome, not output;
/// the recorded card carries the whole output once the command ends.
fn hidden_lines_clock(text: &str) -> Option<u64> {
    let rest = text.trim_start_matches(['…', ' ']).strip_prefix('+')?;
    let digits = rest.find(|c: char| !c.is_ascii_digit())?;
    let rest = rest[digits..].trim_start();
    if digits == 0 || !rest.starts_with("line") {
        return None;
    }
    parenthesized_elapsed_seconds(
        rest.trim_start_matches(|c: char| c.is_ascii_alphabetic())
            .trim(),
    )
}

/// `(18s)`, `(1m 5s)`: the clock Claude paints under a running command.
fn parenthesized_elapsed_seconds(text: &str) -> Option<u64> {
    parse_elapsed_seconds(text.strip_prefix('(')?.strip_suffix(')')?.trim())
}

/// The rows as one text: a row that reaches the grid's right edge was hard-wrapped by the terminal
/// and continues on the next row with no break, any other row ends with `separator`. With
/// `content_indent`, a row indented past it keeps the extra indentation as leading spaces.
fn join_wrapped_rows(
    rows: &[ScreenRow],
    width: usize,
    separator: &str,
    content_indent: Option<usize>,
) -> String {
    let mut text = String::new();
    let mut wrapped = false;
    for (index, row) in rows.iter().enumerate() {
        if index > 0 && !wrapped {
            text.push_str(separator);
        }
        if !wrapped {
            if let Some(content_indent) = content_indent.filter(|_| index > 0) {
                text.push_str(&" ".repeat(row.indent.saturating_sub(content_indent)));
            }
        }
        text.push_str(&row.text);
        wrapped = width > 0 && row.end + 1 >= width;
    }
    text
}
