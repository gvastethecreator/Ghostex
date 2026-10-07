//! The text helpers the menus share.

pub(crate) use crate::sidebar_view::text::js_trim;

/// `resolveSessionChatTranscriptAgent(agentId, agentIcon)`: which transcript family an agent
/// belongs to, or `None` when it has none. Both candidates are tried in order, trimmed and
/// lowercased.
///
/// SEE-ALSO: packages/shared/session-chat-agents.ts.
pub(crate) fn transcript_agent(
    agent_id: Option<&str>,
    agent_icon: Option<&str>,
) -> Option<&'static str> {
    for candidate in [agent_id, agent_icon] {
        let Some(candidate) = candidate else {
            continue;
        };
        let normalized = js_trim(candidate).to_lowercase();
        let resolved = match normalized.as_str() {
            "antigravity" | "antigravity-cli" | "antigravity cli" | "agy" => Some("antigravity"),
            "claude" | "openclaude" => Some("claude"),
            "codex" => Some("codex"),
            "cursor" | "cursor-agent" | "cursor cli" => Some("cursor"),
            "empryo" => Some("empryo"),
            "grok" | "grok-build" => Some("grok"),
            "hermes" | "hermes-agent" | "hermes agent" => Some("hermes"),
            "pi" | "omp" => Some("pi"),
            "zcode" | "zcode-cli" => Some("zcode"),
            "freebuff" => Some("freebuff"),
            _ => None,
        };
        if resolved.is_some() {
            return resolved;
        }
    }
    None
}

/// `formatIdentifier` from the Copy Details text: a known agent's proper name, else the id split
/// on `-` and `_` with each part capitalized.
pub(crate) fn format_identifier(value: &str) -> String {
    let known = match value {
        "browser" => Some("Browser"),
        "claude" => Some("Claude"),
        "codex" => Some("Codex"),
        "copilot" => Some("Copilot"),
        "cursor-cli" => Some("Cursor CLI"),
        "gemini" => Some("Gemini"),
        "opencode" => Some("OpenCode"),
        "pi" => Some("Pi"),
        "terminal" => Some("Terminal"),
        _ => None,
    };
    if let Some(known) = known {
        return known.to_string();
    }
    value
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            // `charAt(0).toUpperCase() + slice(1)` works on UTF-16 units; the first code unit of
            // an astral character has no uppercase form either way, so taking the first character
            // gives the same answer for every input that is not a lone surrogate.
            let mut characters = part.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
