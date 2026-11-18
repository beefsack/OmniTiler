//! Owned retained v4 pager presentation substate for
//! [`crate::world_engine::WorldState`].
//!
//! Holds the retained order, live frames, observed focus, and latest published
//! revision. Projection and query behavior remain separate.

use std::collections::BTreeMap;

use crate::directional::WindowId;
use crate::geometry::Rect;

#[derive(Debug, Clone, Default)]
pub(crate) struct PresentationState {
    pub(crate) order_v4: Vec<WindowId>,
    pub(crate) live_v4: BTreeMap<WindowId, Rect>,
    pub(crate) focused_v4: Option<WindowId>,
    pub(crate) latest_published_rev_v4: u64,
}

impl PresentationState {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::default()
    }
}
