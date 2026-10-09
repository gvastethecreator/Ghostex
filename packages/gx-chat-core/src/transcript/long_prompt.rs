//! A long prompt's first lines: what its bubble shows until the reader asks for the rest.
//!
//! CDXC:SessionChat 2026-10-09 DECISION: User: "make really long user messages like this not show in full after like x lines we just hide the rest to avoid this issue (add show more button there if needed)", after a 41,000-character pasted log slowed the whole app while it scrolled. A prompt longer than 20 lines or 2,000 characters shows its first 12 lines (at most 1,200 characters) and a Show more / Show less toggle under them; the hidden part is not laid out until it is shown.
//! CDXC:SessionChat 2026-10-09 SEE-ALSO: the toggle is drawn by `apps/desktop/src/app/native_chat/transcript.rs` (the user bubble, with `thinking.rs`'s `show_more_toggle`) and the phone's `apps/mobile/app/src/chat/native/transcript/MessageRow.tsx` `UserMessage`.

/// A prompt longer than this many lines is collapsed.
const COLLAPSE_LINES: usize = 20;
/// A prompt longer than this many characters is collapsed.
const COLLAPSE_CHARS: usize = 2_000;
/// How many lines a collapsed prompt shows.
const PREVIEW_LINES: usize = 12;
/// How many characters a collapsed prompt shows at most.
const PREVIEW_CHARS: usize = 1_200;

/// The start of `body` a collapsed bubble shows, or `None` when the prompt is short enough to
/// show whole. A line the character budget runs out in is cut at a word and ends with an
/// ellipsis, and a code fence the cut leaves open is closed so the rest does not render as code.
pub fn collapsed_prompt(body: &str) -> Option<String> {
    let body = body.trim_matches('\n');
    if body.lines().count() <= COLLAPSE_LINES && body.chars().count() <= COLLAPSE_CHARS {
        return None;
    }
    let mut lines: Vec<String> = Vec::new();
    let mut budget = PREVIEW_CHARS;
    for line in body.lines().take(PREVIEW_LINES) {
        let length = line.chars().count();
        if length <= budget {
            lines.push(line.to_string());
            budget = budget.saturating_sub(length + 1);
            continue;
        }
        let cut: String = line.chars().take(budget).collect();
        let cut = match cut.rfind(char::is_whitespace) {
            Some(space) if space > cut.len() / 2 => &cut[..space],
            _ => cut.as_str(),
        };
        lines.push(format!("{}\u{2026}", cut.trim_end()));
        break;
    }
    let fences = lines
        .iter()
        .filter(|line| {
            let line = line.trim_start();
            line.starts_with("```") || line.starts_with("~~~")
        })
        .count();
    if fences % 2 == 1 {
        lines.push("```".to_string());
    }
    Some(lines.join("\n"))
}
