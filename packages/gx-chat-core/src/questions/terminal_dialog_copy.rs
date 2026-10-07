//! Hand-written copy for agent panels whose painted text reads badly in a chat card.
//!
//! CDXC:SessionChat 2026-09-24 DECISION: User: the card a slash command like `/fast` opens must not look like terminal text in a text area; show copy we wrote for each of these panels instead of the panel's original text.
//! WHY: the copy is chosen only when every line of the panel is one we recognise, so a panel Claude rewords, or a state we have not seen, keeps its original text rather than a sentence that is no longer true. Panels whose layout is the content (the `/usage` bars, the `/status` table, the `/mobile` QR code) are deliberately not here.

use serde_json::{json, Value};

use crate::questions::model::TerminalDialog;

/// The card's title and markdown paragraphs for a panel we have copy for, or `None`.
pub fn terminal_dialog_copy(dialog: &TerminalDialog) -> Option<Value> {
    if !dialog.rows.is_empty() {
        return None;
    }
    let title = plain_title(&dialog.title);
    let lines: Vec<&str> = dialog
        .body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let (title, paragraphs) = if dialog.input.is_some() {
        if dialog.input.as_deref() == Some("text") && title == CLAUDE_SIGN_IN_TITLE {
            claude_sign_in_copy(&lines)?
        } else {
            return None;
        }
    } else if title == "Fast mode" || title == "Fast mode (research preview)" {
        fast_mode_copy(&lines)?
    } else if let Some(left) = title.strip_prefix("Guest passes · ") {
        guest_passes_copy(left, &lines)?
    } else if title == CODEX_WELCOME_TITLE {
        codex_device_code_copy(&lines)?
    } else if title == CURSOR_BROWSER_OPENED_TITLE || title == CURSOR_BROWSER_OFF_TITLE {
        cursor_sign_in_copy(title == CURSOR_BROWSER_OPENED_TITLE, &lines)?
    } else {
        return None;
    };
    Some(json!({ "title": title, "paragraphs": paragraphs }))
}

/// The title without the leading glyph Claude draws before some panel names (`↯ Fast mode`).
fn plain_title(title: &str) -> &str {
    title
        .trim()
        .trim_start_matches(|c: char| !c.is_alphanumeric())
        .trim_start()
}

/// Claude's `/fast` panel: what the mode costs, and whether credits block turning it on.
fn fast_mode_copy(lines: &[&str]) -> Option<(String, Vec<String>)> {
    const COST: &str = ". Draws from usage credits at a higher rate. Separate rate limits apply.";
    let mut model = None;
    let mut needs_credits = false;
    for line in lines {
        if let Some(rest) = line.strip_prefix("High-speed mode for ") {
            model = Some(rest.strip_suffix(COST)?);
        } else if *line == "Fast mode requires usage credits · /usage-credits to turn them on" {
            needs_credits = true;
        } else if line.starts_with("Learn more: ") {
            // The docs link adds nothing the two sentences do not already say.
        } else {
            return None;
        }
    }
    let mut paragraphs = vec![format!(
        "Fast mode makes {} answer faster. It uses your usage credits at a higher rate and has its own rate limits.",
        model?
    )];
    if needs_credits {
        paragraphs.push(
            "Usage credits are off for this account, so Fast mode can't be turned on yet. Send `/usage-credits` to turn them on."
                .to_string(),
        );
    }
    Some(("Fast mode".to_string(), paragraphs))
}

/// Claude's `/passes` panel: the ticket art is dropped, the offer and the referral link stay.
fn guest_passes_copy(left: &str, lines: &[&str]) -> Option<(String, Vec<String>)> {
    let count = left.strip_suffix(" left")?;
    count.parse::<u32>().ok()?;
    let mut offer = None;
    let mut link = None;
    for line in lines {
        if line.starts_with("https://") && !line.contains(' ') {
            link = Some(*line);
        } else if line.starts_with("Share a free week of Claude Code with friends.") {
            offer = Some(*line);
        } else if !line.contains("CC ✻") {
            return None;
        }
    }
    let passes = if count == "1" { "pass" } else { "passes" };
    Some((
        "Guest passes".to_string(),
        vec![
            format!("You have {count} guest {passes} left. {}", offer?),
            format!("Your referral link: {}", link?),
        ],
    ))
}

/// The heading of Codex's sign-in screens, including its device-code step.
pub const CODEX_WELCOME_TITLE: &str = "Welcome to Codex, OpenAI's command-line coding agent";

/// The card title of Codex's device-code step, whose Previous and Next keys have nothing to move through.
pub const CODEX_DEVICE_CODE_TITLE: &str = "Sign in to Codex";

/// CDXC:Onboarding 2026-09-29 WHY: Sign in with Device Code is how Codex signs in on a computer you are not sitting at (a remote machine, a VM), and its painted terminal text left the link unclickable and the code buried in a text block.
fn codex_device_code_copy(lines: &[&str]) -> Option<(String, Vec<String>)> {
    let [intro, open, link, enter, code, rest @ ..] = lines else {
        return None;
    };
    let expiry = enter
        .strip_prefix("2. Enter this one-time code after you are signed in")?
        .trim();
    let code_ok = !code.is_empty() && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    let link_ok = link.starts_with("https://") && !link.contains(char::is_whitespace);
    let warning = match rest {
        [] => None,
        [warning] if warning.starts_with("Continue only if you started this login in Codex") => {
            Some(warning.to_string())
        }
        _ => return None,
    };
    if *intro != "Finish signing in via your browser"
        || *open != "1. Open this link in your browser and sign in"
        || !code_ok
        || !link_ok
    {
        return None;
    }
    let expiry = expiry
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
        .map(|value| format!(" ({value})"))
        .unwrap_or_default();
    let mut paragraphs = vec![
        format!("Open this link in any browser and sign in: {link}"),
        format!("Then enter this one-time code: `{code}`{expiry}."),
    ];
    paragraphs.extend(warning);
    Some((CODEX_DEVICE_CODE_TITLE.to_string(), paragraphs))
}

/// Cursor's heading over its sign-in link when it opened the browser, and when opening it is turned off.
pub const CURSOR_BROWSER_OPENED_TITLE: &str = "Signing in with the browser...";
pub const CURSOR_BROWSER_OFF_TITLE: &str = "Signing in";

/// CDXC:AgentProviders 2026-10-07 WHY: Cursor's browser step prints its sign-in link on one unwrapped terminal line, which a chat card cannot click; the card links it instead. The browser opens on the computer running Cursor, so the link is also how a phone or another computer signs in.
/// SEE-ALSO: server/src/session_chat_cursor_login.rs, which sends this step as a dialog.
fn cursor_sign_in_copy(browser_opened: bool, lines: &[&str]) -> Option<(String, Vec<String>)> {
    let [instruction, link] = lines else {
        return None;
    };
    if *instruction != "If your browser didn't open, click this link to log in:"
        || !link.starts_with("https://")
        || link.contains(char::is_whitespace)
    {
        return None;
    }
    let paragraphs = if browser_opened {
        vec![
            "Finish signing in on the Cursor page that opened in your browser. The chat continues on its own once you're signed in.".to_string(),
            format!("Browser didn't open, or signing in from another device? [Open Cursor's sign-in page]({link})"),
        ]
    } else {
        vec![
            format!("[Open Cursor's sign-in page]({link}) in any browser and sign in."),
            "The chat continues on its own once you're signed in.".to_string(),
        ]
    };
    Some(("Sign in to Cursor".to_string(), paragraphs))
}

/// The title of Claude's first-run sign-in step, where its browser sign-in hands back a code.
pub const CLAUDE_SIGN_IN_TITLE: &str = "Browser didn't open? Use the url below to sign in";

/// CDXC:Onboarding 2026-09-28 WHY: a new Claude install signs in through this card, and its terminal wording ("Browser didn't open?", a bare link, a field) did not say what to do with the code the browser shows.
/// Claude 2.1.27x adds "Hold Shift while selecting to use your terminal's native copy" under the link, a terminal-only key the card leaves out.
fn claude_sign_in_copy(lines: &[&str]) -> Option<(String, Vec<String>)> {
    let lines: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| !line.starts_with("Hold Shift while selecting"))
        .collect();
    let [link] = lines.as_slice() else {
        return None;
    };
    (link.starts_with("https://") && !link.contains(char::is_whitespace)).then(|| {
        (
            "Sign in to Claude".to_string(),
            vec![
                "Sign in on the page that opened in your browser. When it shows a code, paste it below."
                    .to_string(),
                format!("Browser didn't open? Open this link to sign in: {link}"),
            ],
        )
    })
}
