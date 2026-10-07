//! The Sort & Filter page's Group Working Sessions row (`sidebarAction: groupWorkingSessions`),
//! which flips the `groupWorkingSessions` setting.
//!
//! CDXC:Sidebar 2026-10-05 WHY:
//! The row writes the setting, not the sidebar's own state like Show Hidden and the tag filters,
//! because Settings > Sidebar shows the same switch and the two must always agree. The save goes
//! through the same end function as Hide Machine (`machine_disable.rs`), so the new value reaches
//! the sidebar, the open Settings window and every other surface through the ordinary settings
//! fan-out instead of a second path.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_view/ordering.rs (`is_grouped_working`, the rule),
//! packages/settings-catalog/src/general/sidebar.rs (the Settings row),
//! apps/desktop/src/app/gx_store/sidebar_state_actions.rs (the host).

use serde_json::{json, Map, Value};

use super::plan::{ActionEffect, SidebarActionPlan};
use super::resolve::text_field;
use crate::sidebar_view::GROUP_WORKING_SESSIONS_SETTING_KEY;

/// The `sidebarAction` the menu row carries.
pub const GROUP_WORKING_SESSIONS_ACTION: &str = "groupWorkingSessions";

/// `source` of the patch.
pub const GROUP_WORKING_SESSIONS_SETTINGS_SOURCE: &str = "sidebar:groupWorkingSessions";

/// Whether this renderer command is the Group Working Sessions row.
pub fn owns_group_working_sessions_command(command: &Value) -> bool {
    text_field(command, "type") == Some("sidebarAction")
        && text_field(command, "action") == Some(GROUP_WORKING_SESSIONS_ACTION)
}

/// The patch that flips the setting, given the settings as saved.
pub fn plan_group_working_sessions(
    command: &Value,
    saved_settings: &Map<String, Value>,
) -> Option<SidebarActionPlan> {
    if !owns_group_working_sessions_command(command) {
        return None;
    }
    let on = saved_settings
        .get(GROUP_WORKING_SESSIONS_SETTING_KEY)
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Some(SidebarActionPlan::one(ActionEffect::UpdateSettingsPatch {
        message: json!({
            "type": "updateSettingsPatch",
            "source": GROUP_WORKING_SESSIONS_SETTINGS_SOURCE,
            "patch": { GROUP_WORKING_SESSIONS_SETTING_KEY: !on },
        }),
    }))
}
