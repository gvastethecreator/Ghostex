//! The transcript region's own state: which of the empty, loading, starting and ready pictures the
//! renderer draws, and the copy that goes with it.

use ghostex_gx_protocol::Tri;
use serde::{Deserialize, Serialize};

/// What the transcript region is doing, from
/// `packages/core-ui/chat/session-chat-view-state.ts`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewState {
    /// `ready`, `empty`, `loading`, `starting`, `notFound`, `error`.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub is_working: Tri<bool>,
    /// The failure copy a `kind: "error"` view carries.
    #[serde(default, skip_serializing_if = "Tri::is_absent")]
    pub error: Tri<String>,
}

/// The headline and detail an empty transcript shows.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmptyState {
    pub title: String,
    pub detail: String,
    /// The retry button's label, present when the region offers one (a conversation that could
    /// not be loaded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

/// What the loading hold says once it has something to say, above its Try now button.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadingNotice {
    pub title: String,
    /// The line under the title, empty when there is none.
    pub detail: String,
    pub action: String,
}

/// The greeting a brand new session shows instead of the empty copy.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSessionWelcome {
    /// The agent's display name, or `null` when the session has no agent label yet.
    pub agent_name: Option<String>,
    /// The agent mark, or `null` when the agent has none.
    pub icon: Option<String>,
    /// Dropped once a notice or question card takes the space below the mark.
    pub show_title: bool,
    pub title: String,
}
