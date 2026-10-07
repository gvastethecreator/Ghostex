//! Whether a machine's section shows Spaces: the one answer every Spaces gate asks (the Space row,
//! the filtered list, Space menus and drop targets, reveal and follow, Space Sleep, the Space
//! switch and its hotkeys).

use crate::keys::MachineId;
use crate::presentation_store::PresentationStore;

use super::inputs::SidebarInputs;
use super::reveal::machine_key;

/// Whether `machine`'s section shows Spaces.
///
/// CDXC:Spaces 2026-10-06 DECISION:
/// "When I enable spaces on one of the host machines then I connect to that machine we should also show spaces on the client connecting there please (so spaces is a setting per gxserver not per client app, but it's toggled in settings like it is now basically)". A remote machine's section follows the switch its own gxserver publishes with its Spaces document; this computer's section follows this computer's switch, which the Settings page edits and which is the same file this computer's gxserver reads.
///
/// CDXC:Spaces 2026-10-06 WHY:
/// A daemon from before the switch was published sends none, and its section keeps following this computer's switch as every section did before, rather than losing a Space row the user can see today.
pub fn spaces_enabled_on(
    store: &PresentationStore,
    inputs: &SidebarInputs,
    machine: &MachineId,
) -> bool {
    if inputs.spaces_lifted {
        return false;
    }
    let own = inputs.settings.sidebar_spaces_enabled;
    match machine {
        MachineId::Local => own,
        MachineId::Remote(_) => store
            .machine(machine)
            .and_then(|entry| entry.side_state().spaces_enabled)
            .unwrap_or(own),
    }
}

/// Whether the selected machine's section shows Spaces.
pub fn section_spaces_enabled(store: &PresentationStore, inputs: &SidebarInputs) -> bool {
    spaces_enabled_on(store, inputs, &machine_key(&inputs.ui.selected_machine_id))
}
