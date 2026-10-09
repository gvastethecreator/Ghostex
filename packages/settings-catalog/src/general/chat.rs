use crate::data::*;
use crate::json::opt;
use crate::rows::{row, section, Section};

pub(crate) fn chat() -> Section {
    section(
        "chat",
        "Chat",
        vec![
            row("preferredAgentInterface", "Default view for compatible agents", "Automatically switch to chat as soon as Ghostex detects that an agent session supports it.").options(PREFERRED_AGENT_INTERFACE_OPTIONS),
            row("closeEmptySessionsOnNew", "Close empty sessions when starting a new one", "When you start a new session, close this project's other sessions that have nothing typed, drafted or queued. Only applies when Chat is the default view for that agent; with Terminal it never runs."),
            row("sessionChatFontFamily", "Chat font family", "Use any installed font in chat messages and the prompt composer."),
            row("sessionChatZoomPercent", "Default chat zoom (%)", "Scale the desktop chat interface, including messages and the prompt composer, from 70% to 200% in 5% steps. Default: 100%."),
            row("sessionChatCustomTranscriptWidthEnabled", "Custom transcript width", "Let the transcript use a different width from the prompt composer."),
            row("sessionChatTranscriptWidthPercent", "Transcript width", "Set the centered transcript width without changing the prompt composer."),
            row("sessionChatFileEditPreviews", "Show file edit previews", "Show the first seven code lines in each file edit. Turn off to show only the path and change counts."),
            row("sessionChatKeepComposerExpanded", "Keep chat box expanded while scrolling", "Keep the desktop chat box at full size while you scroll the transcript instead of shrinking it as you scroll up and growing it back at the end."),
            row("sessionChatConfirmEscapeInterrupt", "Press Escape twice to interrupt", "While the agent is working, the first Escape asks you to press Escape again within 2 seconds before it interrupts. Turn off to interrupt on the first Escape."),
            row("sessionChatSimpleMode", "Simple mode", "Simplify all chats: hide tool command previews and group file edits behind an expandable file count."),
            row("sessionChatVerboseMode", "Verbose mode", "Expand thinking blocks to show their tool calls by default. Each chat can override it from its composer."),
        ],
    )
}

pub(crate) fn sounds() -> Section {
    section(
        "sounds",
        "Sounds",
        vec![
            row("completionSound", "Completion Sound", "Sound for terminal completions, or Off.").options(&[opt("Off", "off")]).options_of(COMPLETION_SOUND_OPTIONS, "label", "value"),
            row("showMacOSAttentionNotifications", "Attention Notifications", "Show a system notification when a session needs attention."),
            row("resetExpirySystemNotifications", "Reset Expiry Notifications", "Show a system notification when a banked Claude or Codex usage reset expires within 3 days and again within 24 hours. The red notification in the bell stays either way."),
            row("attentionNotificationActions", "Agent Completion Alert Test", "Test the current completion alert settings or open Notification Settings."),
            row("actionCompletionSound", "Action Completion Sound", "Sound for action completions.").options_of(COMPLETION_SOUND_OPTIONS, "label", "value"),
            row("copySound", "Copy Sound", "Play a short sound when copying to the clipboard, including text from the chat composer."),
        ],
    )
}
