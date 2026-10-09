//! Typed-but-unsaved text on the Team and Team flow rows, keyed by input id. A field shows its
//! draft while there is one and the saved value otherwise; saving clears the draft, and a Save
//! button is on while the draft differs from the saved value.
use super::*;
use gpui::Entity;
use gpui_component::input::InputState;

impl WorkspacesTab {
    /// The draft of `id`, or `""`.
    pub(super) fn draft(&self, id: &SharedString) -> String {
        self.drafts.get(id).cloned().unwrap_or_default()
    }

    /// Whether `id` holds typed text that differs from `saved`.
    pub(super) fn draft_changed(&self, id: &SharedString, saved: &str) -> bool {
        self.drafts.get(id).is_some_and(|draft| draft != saved)
    }

    /// The input of `id`, showing its draft or `saved`.
    pub(super) fn draft_input(
        &mut self,
        id: &SharedString,
        saved: &str,
        placeholder: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        let shown = self
            .drafts
            .get(id)
            .cloned()
            .unwrap_or_else(|| saved.to_string());
        let draft_id = id.clone();
        FieldStates::text_state(
            self,
            id,
            &shown,
            Some(placeholder),
            move |page: &mut Self, text, _window, cx| {
                page.drafts.insert(draft_id.clone(), text);
                cx.notify();
            },
            window,
            cx,
        )
    }
}
