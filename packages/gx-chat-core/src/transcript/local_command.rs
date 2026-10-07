//! The two rows one slash command renders as, and the escaped-markup contract around them.
//!
//! Ported from `packages/core-ui/chat/session-chat-local-command-transcript.ts`.

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole, ChatSource};

/*
CDXC:SessionChat 2026-09-10 WHY:
The escaped-markup contract, one place for both halves of it. gxserver marks a harness marker whose
payload it escaped (Codex's `!` commands, and the slash commands it archives and replays in
session_chat_local_command.rs), because the reader strips markup out of these rows to find their
text and would otherwise eat a command or an output that contains `<…>`. The attribute string has to
match gxserver's `ESCAPED_MARKUP_ATTRIBUTE` byte for byte.
*/
pub const ESCAPED_MARKUP_ATTRIBUTE: &str = "data-ghostex-escaped=\"html\"";

const CODEX_LOCAL_COMMAND_INPUT: &str = "<bash-input data-ghostex-escaped=\"html\">";
const CODEX_LOCAL_COMMAND_OUTPUT: &str = "<bash-stdout data-ghostex-escaped=\"html\">";

pub fn decode_escaped_markup(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn text_block(block: Option<&ChatBlock>) -> Option<&str> {
    match block {
        Some(ChatBlock::Text { text }) => Some(text),
        _ => None,
    }
}

/// Splits Codex's two-block local-command row into the command and its output, so each gets its own
/// marker. Identical text to the rows gxserver replays from its archive.
pub fn normalize_local_command_messages(messages: &[ChatMessage]) -> Vec<ChatMessage> {
    let mut normalized = Vec::with_capacity(messages.len());
    for message in messages {
        let command = text_block(message.blocks.first());
        let output = text_block(message.blocks.get(1));
        let splits = message.role == ChatRole::User
            && message.source == ChatSource::Transcript
            && message.blocks.len() == 2
            && command.is_some_and(|text| text.starts_with(CODEX_LOCAL_COMMAND_INPUT))
            && output.is_some_and(|text| text.starts_with(CODEX_LOCAL_COMMAND_OUTPUT));
        if !splits {
            normalized.push(message.clone());
            continue;
        }
        let mut first = message.clone();
        first.id = format!("{}:command", message.id);
        first.blocks = vec![message.blocks[0].clone()];
        let mut second = message.clone();
        second.id = format!("{}:output", message.id);
        second.blocks = vec![message.blocks[1].clone()];
        normalized.push(first);
        normalized.push(second);
    }
    normalized
}

/// The tool name a `!` shell command's card carries; the name makes it a command tool, so the card
/// gets the terminal glyph and its Command and Result blocks.
pub const SHELL_COMMAND_TOOL_NAME: &str = "Shell";

/// The body of the first `<tag>` or `<tag data-ghostex-escaped="html">` element in `text`, entities
/// decoded.
fn harness_tag_body(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let start = text.find(&open)? + open.len();
    let rest = &text[start..];
    let body_start = if let Some(rest) = rest.strip_prefix('>') {
        text.len() - rest.len()
    } else {
        let attribute = rest
            .strip_prefix(' ')?
            .strip_prefix(ESCAPED_MARKUP_ATTRIBUTE)?;
        text.len() - attribute.strip_prefix('>')?.len()
    };
    let close = format!("</{tag}>");
    let end = body_start + text[body_start..].find(&close)?;
    Some(decode_escaped_markup(&text[body_start..end]))
}

fn joined_text(message: &ChatMessage) -> String {
    message
        .blocks
        .iter()
        .filter_map(|block| match block {
            ChatBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// The command of a `!` shell row: Claude's `<bash-input>` row, or the escaped marker gxserver
/// writes for a Codex `!` command.
pub fn shell_command_input(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if !trimmed.starts_with("<bash-input") || !trimmed.ends_with("</bash-input>") {
        return None;
    }
    let command = harness_tag_body(trimmed, "bash-input")?;
    let command = command.trim();
    (!command.is_empty()).then(|| command.to_string())
}

/// The chat text a `!` shell row stands for (`!aws login`), the same text the composer sent, so an
/// echo and its recorded row agree.
pub fn shell_command_prompt_text(message: &ChatMessage) -> Option<String> {
    if message.role != ChatRole::User {
        return None;
    }
    shell_command_input(&joined_text(message)).map(|command| format!("!{command}"))
}

/// What a `!` command printed, stdout then stderr, and whether only stderr had anything to say.
fn shell_command_output(text: &str) -> Option<(String, bool)> {
    let trimmed = text.trim();
    if !trimmed.starts_with("<bash-stdout") && !trimmed.starts_with("<bash-stderr") {
        return None;
    }
    let clean = |body: Option<String>| {
        crate::transcript::noise::strip_chat_ansi(&body.unwrap_or_default().replace("\r\n", "\n"))
            .trim_end()
            .to_string()
    };
    let stdout = clean(harness_tag_body(trimmed, "bash-stdout"));
    let stderr = clean(harness_tag_body(trimmed, "bash-stderr"));
    let failed = stdout.trim().is_empty() && !stderr.trim().is_empty();
    let output = match (stdout.trim().is_empty(), stderr.trim().is_empty()) {
        (_, true) => stdout,
        (true, false) => stderr,
        (false, false) => format!("{stdout}\n{stderr}"),
    };
    Some((output, failed))
}

/// The card a `!` command renders as: a user turn holding the command as a shell tool call and its
/// output as that call's result. `output` is `None` while the command still runs.
pub fn shell_command_message(
    template: &ChatMessage,
    command: &str,
    output: Option<(String, bool)>,
) -> ChatMessage {
    let call_id = Some(format!("shell-command:{}", template.id));
    let mut blocks = vec![ChatBlock::ToolCall {
        name: SHELL_COMMAND_TOOL_NAME.to_string(),
        input: serde_json::json!({ "command": command }),
        call_id: call_id.clone(),
    }];
    if let Some((output, failed)) = output.filter(|(output, _)| !output.trim().is_empty()) {
        blocks.push(ChatBlock::ToolResult {
            output,
            is_error: failed.then_some(true),
            call_id,
        });
    }
    let mut message = template.clone();
    message.role = ChatRole::User;
    message.blocks = blocks;
    message
}

/// Whether a row is a `!` command's card, which renders as the tool card in the user's place.
pub fn is_shell_command_message(message: &ChatMessage) -> bool {
    message.role == ChatRole::User
        && matches!(
            message.blocks.first(),
            Some(ChatBlock::ToolCall { name, .. }) if name == SHELL_COMMAND_TOOL_NAME
        )
        && message.blocks.iter().all(|block| {
            matches!(
                block,
                ChatBlock::ToolCall { .. } | ChatBlock::ToolResult { .. }
            )
        })
}

/*
CDXC:SessionChat 2026-10-07 DECISION:
User: a `!` shell command sent to the agent ("! aws login") showed nothing in chat, then two plain "Local command" / "Local command output" rows; "let's please just show this in a new component similar to the ones we already have". Claude writes a `!` command as two user rows, `<bash-input> cmd</bash-input>` and `<bash-stdout>…</bash-stdout><bash-stderr>…</bash-stderr>` (its own output HTML-escaped), and only once the command has finished; gxserver writes a Codex `!` command as the same pair of escaped markers in one row, split above. The pair folds into ONE user row holding the command as a shell tool call and the output (stdout, then stderr) as its result, so every renderer draws the tool card where the user's message sits and the row opens its own turn, which the collapsed markers never did (summary mode filed them under the previous turn's work, or dropped them when no turn came before).
SEE-ALSO: session/pending.rs (the echo drawn as this card until the row lands), apps/desktop/src/app/native_chat/transcript.rs and apps/mobile/app/src/chat (the card in the user's place).
*/
pub fn fold_shell_commands(messages: &[ChatMessage]) -> Vec<ChatMessage> {
    let mut folded = Vec::with_capacity(messages.len());
    let mut index = 0;
    while index < messages.len() {
        let message = &messages[index];
        let command = (message.role == ChatRole::User)
            .then(|| shell_command_input(&joined_text(message)))
            .flatten();
        let Some(command) = command else {
            folded.push(message.clone());
            index += 1;
            continue;
        };
        let output = messages
            .get(index + 1)
            .filter(|next| next.role == ChatRole::User)
            .and_then(|next| shell_command_output(&joined_text(next)));
        index += if output.is_some() { 2 } else { 1 };
        folded.push(shell_command_message(message, &command, output));
    }
    folded
}
