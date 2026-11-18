//! Owned lifetime-local runtime substate for
//! [`crate::world_engine::WorldState`].
//!
//! Holds `composed_maximized` (`caps.maximize == false` only); commands and
//! gestures mutate it under commit/rollback gates, native-state handling reads
//! it as policy input, and lifetime retirement prunes it. Never stored intent.

use std::collections::BTreeSet;

use crate::directional::WindowId;

#[derive(Debug, Clone, Default)]
pub(crate) struct RuntimeState {
    pub(crate) composed_maximized: BTreeSet<WindowId>,
}

impl RuntimeState {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::default()
    }
}
