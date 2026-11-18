//! Owned logical workspace model substate for [`crate::world_engine::WorldState`].
//!
//! Holds core logical workspace facts: rings, current/previous views,
//! history, topology/output facts, hotplug displacement and relocation state,
//! tiling modes, and the sole window-lifetime/home registry. Native desktop
//! bindings and settlement evidence live separately in
//! [`crate::world_settlement_state::SettlementState`].

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::WindowId;
use crate::geometry::Rect;
use crate::session::DomainKey;
use crate::workspace_scope::{DisplacedEntry, OutputFacts, ScopeHistory};
use crate::world_members::MemberState;
use crate::world_report::TilingMode;

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceState {
    pub(crate) creation_counter: u64,
    /// Core-owned logical workspace rings (D2): per-screen ordered lists of
    /// stable logical workspace ids, retained across reports.
    pub(crate) mapping: BTreeMap<String, Vec<String>>,
    /// Core-owned current view per screen: the stable logical workspace id
    /// shown, retained across reports.
    pub(crate) views: BTreeMap<String, String>,
    /// Planner-owned previous-view history (D2): per-output entries
    /// observed from verified views only.
    pub(crate) history: ScopeHistory,
    /// Planner-owned hotplug displacement: removed screens mapped to their
    /// preserved logical workspace units plus the hosting survivor.
    pub(crate) displaced: BTreeMap<String, DisplacedEntry>,
    /// Members rehomed off a removed output by this report's displacement.
    /// Cleared at the start of every accepted run.
    pub(crate) displaced_rehomed: BTreeSet<WindowId>,
    /// Last observed screen facts by stable output key (FULL rectangle,
    /// work area, plus active-screen primary).
    pub(crate) output_facts: BTreeMap<String, OutputFacts>,
    /// Core-owned live output topology: validated FULL rectangles by stable
    /// output id, retained from the last accepted report claiming complete
    /// screens.
    pub(crate) live_topology: BTreeMap<String, Rect>,
    /// Planner-owned per-workspace tiling mode (absent means Tiled).
    pub(crate) tiling: BTreeMap<DomainKey, TilingMode>,
    /// Core-owned logical membership (D2): stable window-lifetime homing
    /// independent of layout sessions. The single homing owner.
    pub(crate) members: BTreeMap<WindowId, MemberState>,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceState {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            creation_counter: 0,
            mapping: BTreeMap::new(),
            views: BTreeMap::new(),
            history: ScopeHistory::new(),
            displaced: BTreeMap::new(),
            displaced_rehomed: BTreeSet::new(),
            output_facts: BTreeMap::new(),
            live_topology: BTreeMap::new(),
            tiling: BTreeMap::new(),
            members: BTreeMap::new(),
        }
    }

    #[must_use]
    pub(crate) fn core_homing(&self, window: &WindowId) -> Option<DomainKey> {
        crate::world_members::core_homing_in(&self.members, &self.mapping, &self.displaced, window)
    }

    #[must_use]
    pub(crate) fn is_floating(&self, key: &DomainKey) -> bool {
        self.tiling
            .get(key)
            .is_some_and(|mode| *mode == TilingMode::Floating)
    }
}
