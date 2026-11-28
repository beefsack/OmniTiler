//! Owned focus, cursor/hover, and return-memory substate for
//! [`crate::world_engine::WorldState`].
//!
//! Holds the retained execution subject (`logical_focus_cursor`), retained
//! focus request (`pending_focus`), workspace return memory
//! (`last_active_window`), cursor/hover history (`last_cursor`, `prev_cursor`,
//! `ffc_last_hover`), fresh-window homing memory (`fresh_focus`), and
//! automatic-focus identity/spend state (`admission_focus_spent`,
//! `automatic_focus_tokens`), plus per-run batch focus scratch
//! (`batch_base_focus`, `batch_command_focus`, `implicit_event_focus`).

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{OutputId, WindowId};
use crate::fresh_focus::FreshFocusMemory;
use crate::session::DomainKey;
use crate::world_close_focus::PendingFocus;
use crate::world_cursor::PendingWarp;
use crate::world_report::ViewRef;

#[derive(Debug, Clone)]
pub(crate) struct DetachedCursorGuard {
    pub(crate) issued_observation: u64,
    pub(crate) refused_preimage: Option<ViewRef>,
}

#[derive(Debug, Clone)]
pub(crate) struct FocusState {
    pub(crate) admission_focus_spent: BTreeSet<String>,
    pub(crate) automatic_focus_tokens: BTreeMap<WindowId, u64>,
    pub(crate) pending_focus: Option<PendingFocus>,
    /// One retained cursor-warp request, independent of `pending_focus`: the
    /// focus row may settle while the warp still awaits adapter spend or a
    /// final retry. Never more than one; `None` publishes no request.
    pub(crate) pending_warp: Option<PendingWarp>,
    /// Mint counter for warp ids: incremented on every mint, so a cleared
    /// request id is never reused.
    pub(crate) warp_mint_seq: u64,
    pub(crate) last_cursor: Option<(i32, i32)>,
    pub(crate) prev_cursor: Option<(i32, i32)>,
    pub(crate) ffc_last_hover: Option<WindowId>,
    pub(crate) fresh_focus: FreshFocusMemory,
    pub(crate) last_active_window: BTreeMap<DomainKey, WindowId>,
    pub(crate) logical_focus_cursor: Option<WindowId>,
    pub(crate) detached_cursor_guards: BTreeMap<OutputId, DetachedCursorGuard>,
    pub(crate) batch_base_focus: BTreeMap<DomainKey, Option<WindowId>>,
    pub(crate) batch_command_focus: BTreeSet<WindowId>,
    pub(crate) implicit_event_focus: Option<WindowId>,
}

impl Default for FocusState {
    fn default() -> Self {
        Self::new()
    }
}

impl FocusState {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            admission_focus_spent: BTreeSet::new(),
            automatic_focus_tokens: BTreeMap::new(),
            pending_focus: None,
            pending_warp: None,
            warp_mint_seq: 0,
            last_cursor: None,
            prev_cursor: None,
            ffc_last_hover: None,
            fresh_focus: FreshFocusMemory::default(),
            last_active_window: BTreeMap::new(),
            logical_focus_cursor: None,
            detached_cursor_guards: BTreeMap::new(),
            batch_base_focus: BTreeMap::new(),
            batch_command_focus: BTreeSet::new(),
            implicit_event_focus: None,
        }
    }
}
