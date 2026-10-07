//! What an Empryo slash command sent from the chat printed, for its archived row
//! (session_chat_local_command.rs).
//!
//! CDXC:SessionChat 2026-10-06 WHY:
//! Empryo 3.9.0-beta prints a command's result as event rows in its transcript tree (`├─ ✓ 󰚩 * master  /repo  (clean)` for `/worktree list`), opens an interactive panel for most others, and repaints its whole alternate screen every few seconds (rotating tips, cache timers, the side panel). A screen diff picks up that churn, so the result is the event rows the command added, and a panel left open is recorded as such; the live card for the open panel comes from session_chat_empryo_blocking.rs.

/// The archived result of a command whose panel took the keyboard.
const PANEL_OPENED: &str = "Opened in Empryo's terminal.";

/// Event rows in screen order, without their tree connector or Nerd Font icons.
fn event_rows(screen: &str) -> Vec<String> {
    screen
        .lines()
        .filter_map(|line| {
            let line = crate::session_chat_options::strip_ansi_sgr(line);
            let row = line.trim();
            let row = row
                .strip_prefix("├─ ")
                .or_else(|| row.strip_prefix("╰─ "))?;
            let row: String = row
                .chars()
                .filter(|ch| !crate::session_chat_options::is_nerd_font_icon(*ch))
                .collect();
            let row = row.split_whitespace().collect::<Vec<_>>().join(" ");
            (!row.is_empty()).then_some(row)
        })
        .collect()
}

/// The rows `after` adds to `before`, or the panel note while a panel has the keyboard.
pub(crate) fn empryo_command_output(before: &str, after: &str) -> Option<String> {
    if crate::session_chat_composer::empryo_input_unfocused(after) {
        return Some(PANEL_OPENED.to_string());
    }
    let mut earlier = event_rows(before);
    let added: Vec<String> = event_rows(after)
        .into_iter()
        .filter(|row| match earlier.iter().position(|seen| seen == row) {
            Some(index) => {
                earlier.remove(index);
                false
            }
            None => true,
        })
        .collect();
    (!added.is_empty()).then(|| added.join("\n"))
}
