//! The composer's Dictate control: the system speech recognizer (GPUI Kit's speech input) turns
//! what the reader says into text, typed into the draft when they stop.
//!
//! CDXC:SessionChat 2026-10-08 DECISION: User asked for voice input in the chat box, to the left of the Terminal View button, on the desktop and the phone (GPUI Kit's speech input, #3333, on the desktop). Pressing the microphone starts listening and pressing it again stops; the recognized text is typed into the draft and never sent by itself.
//! CDXC:SessionChat 2026-10-08 SEE-ALSO: `apps/mobile/app/src/chat/native/composer/useDictation.ts` is the phone's control; the toolbar order is `toolbar.rs` `COMPOSER_CONTROLS` and the phone's `menus.ts`.

use super::state::NativeChatView;
use gpui::{App, AppContext as _, Context, Entity, EntityInputHandler as _, Subscription, Window};
use gpui_component::speech::{SpeechEvent, SpeechState};
use serde_json::json;

/// Whether this build can dictate. macOS and Windows have a system recognizer; Linux has none and
/// the web build has no microphone, so the control is not drawn there.
pub(super) const DICTATION: bool = cfg!(any(target_os = "macos", target_os = "windows"));

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
                        this.host(
                            "toast",
                            json!({"message": format!("Dictation stopped: {error}"), "level": "error"}),
                            cx,
                        );
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
