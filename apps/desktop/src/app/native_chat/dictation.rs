//! The composer's Dictate control: the system speech recognizer (GPUI Kit's speech input) turns
//! what the reader says into text, typed into the draft when they stop.
//!
//! CDXC:SessionChat 2026-10-08 DECISION: User asked for voice input in the chat box, to the left of the Terminal View button, on the desktop and the phone (GPUI Kit's speech input, #3333, on the desktop). Pressing the microphone starts listening and pressing it again stops; the recognized text is typed into the draft and never sent by itself.
//! CDXC:SessionChat 2026-10-08 SEE-ALSO: `apps/mobile/app/src/chat/native/composer/useDictation.ts` is the phone's control; the toolbar order is `toolbar.rs` `COMPOSER_CONTROLS` and the phone's `menus.ts`.

use super::state::{NativeChatEvent, NativeChatView};
use gpui::{App, AppContext as _, Context, Entity, EntityInputHandler as _, Subscription, Window};
use gpui_component::speech::{SpeechError, SpeechEvent, SpeechState};
use serde_json::json;

/// Whether this build can dictate. macOS and Windows have a system recognizer; Linux has none and
/// the web build has no microphone, so the control is not drawn there.
pub(super) const DICTATION: bool = cfg!(any(target_os = "macos", target_os = "windows"));

/// Where to allow the microphone when the system refused it.
const MICROPHONE_ACCESS_HINT: &str = if cfg!(target_os = "windows") {
    "Ghostex can't use the microphone. Turn on Microphone access and Let desktop apps access your microphone in Windows Settings > Privacy & security > Microphone."
} else {
    "Ghostex can't use the microphone or speech recognition. Allow Ghostex under Microphone and under Speech Recognition in System Settings > Privacy & Security."
};

/// What to install when the system cannot recognize its language.
const SPEECH_LANGUAGE_HINT: &str = if cfg!(target_os = "windows") {
    "Windows has no speech recognition for your language. Add a speech language in Windows Settings > Time & language > Speech."
} else {
    "Your Mac can't recognize your language on the device. Check that Dictation is on in System Settings > Keyboard."
};

/// The app toast a failed dictation shows, from the composer or the terminal bar: what went wrong
/// and, when a system setting is missing, where to turn it on.
///
/// CDXC:SessionChat 2026-10-09 WHY: User: "pls fix dictate feature it doesnt work at all (at least on windows)". Windows dictation fails at its start (0x80045509) while Online speech recognition is off, its default on many PCs, and the composer reported that as a `sessionChatHostAction` "toast", which the app drops, so pressing the microphone did nothing visible. This raw `{type: "toast"}` message reaches the app's toast window from both buttons.
pub(crate) fn dictation_failure_toast(error: &SpeechError) -> serde_json::Value {
    let description = match error {
        SpeechError::PermissionDenied => MICROPHONE_ACCESS_HINT.to_owned(),
        SpeechError::NoInputDevice => {
            "No microphone was found. Connect one, or choose an input device in your sound settings."
                .to_owned()
        }
        SpeechError::Unsupported => SPEECH_LANGUAGE_HINT.to_owned(),
        // The recognizer's own words, which on Windows name the privacy setting to turn on.
        SpeechError::Recognizer(error) => sentence(&format!("{error:#}")),
        SpeechError::Input(error) => sentence(&format!("The microphone stopped working: {error:#}")),
    };
    json!({
        "type": "toast",
        "level": "error",
        "title": "Dictation failed",
        "description": description,
        "durationMs": 12_000,
    })
}

/// `text` with a capital first letter and a closing full stop.
fn sentence(text: &str) -> String {
    let text = text.trim().trim_end_matches(['.', ';', ':']);
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    format!("{}{}.", first.to_uppercase(), chars.as_str())
}

/// A dictation session and the subscription that types its result into the draft.
pub(crate) type Dictation = Option<(Entity<SpeechState>, Subscription)>;

impl NativeChatView {
    /// Whether the microphone is listening (or finishing what it heard).
    pub(super) fn dictating(&self, cx: &App) -> bool {
        self.dictation
            .as_ref()
            .is_some_and(|(speech, _)| speech.read(cx).status().is_active())
    }

    /// Start listening, or stop and type what was heard.
    pub(super) fn toggle_dictation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dictation.is_none() {
            let speech = cx.new(SpeechState::new);
            let subscription =
                cx.subscribe_in(&speech, window, |this, _, event, window, cx| match event {
                    SpeechEvent::Final(text) => this.insert_dictation(text, window, cx),
                    SpeechEvent::Error(error) => {
                        cx.emit(NativeChatEvent::Host(dictation_failure_toast(error)));
                        cx.notify();
                    }
                    _ => cx.notify(),
                });
            self.dictation = Some((speech, subscription));
        }
        if let Some((speech, _)) = &self.dictation {
            speech.update(cx, |speech, cx| speech.toggle(cx));
        }
        cx.notify();
    }

    /// Type a finished dictation into the draft, a space apart from the text before it.
    fn insert_dictation(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = text.trim();
        if text.is_empty() {
            cx.notify();
            return;
        }
        let spaced = if self.draft.is_empty() || self.draft.ends_with(char::is_whitespace) {
            text.to_owned()
        } else {
            format!(" {text}")
        };
        if let Some(input) = self.input.clone() {
            input.update(cx, |input, cx| {
                input.focus(window, cx);
                input.replace_text_in_range(None, &spaced, window, cx);
            });
        }
        cx.notify();
    }
}
