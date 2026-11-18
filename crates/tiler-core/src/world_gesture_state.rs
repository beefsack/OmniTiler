//! Owned gesture lifecycle and drag-preview substate for
//! [`crate::world_engine::WorldState`].
//!
//! Holds the active retained gesture binding (`retained`), consumed gesture
//! watermark (`consumed_seq`), latest settled terminal (`terminal`), retained
//! active drag-preview overlay (`drag_preview`), and commit-only trace reason
//! (`staged_drag_preview_reason`).

use crate::gesture::GestureSettled;
use crate::world_gesture::RetainedGesture;
use crate::world_report::DragPreviewV4;

#[derive(Debug, Clone, Default)]
pub(crate) struct GestureState {
    pub(crate) retained: Option<RetainedGesture>,
    pub(crate) consumed_seq: u64,
    pub(crate) terminal: Option<GestureSettled>,
    pub(crate) drag_preview: Option<DragPreviewV4>,
    pub(crate) staged_drag_preview_reason: Option<&'static str>,
}

impl GestureState {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::default()
    }
}
