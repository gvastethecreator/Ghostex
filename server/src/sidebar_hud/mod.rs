//! Sidebar HUD agent and action buttons, split by concern. Every submodule is glob
//! re-exported here, so `crate::sidebar_hud::*` paths are unchanged.

use std::{
    collections::HashSet,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{Map, Value};

use crate::domain::DomainStateError;

mod buttons;
mod model;
mod mutations;
mod normalize;
mod roster;
#[cfg(test)]
mod tests;

pub use buttons::*;
pub use model::*;
pub use mutations::*;
pub(crate) use normalize::*;
pub use roster::*;
