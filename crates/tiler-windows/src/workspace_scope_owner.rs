#![forbid(unsafe_code)]
//! Adopted workspace-scope mode owner state (item 12 stage B).
//!
//! The saved `core.workspace.mode` adopts only on owner restart: the startup
//! value becomes the enforced mode for the whole run, and later live file
//! edits never adopt. The saved document keeps the pending choice; drift
//! between the adopted mode and the saved mode is reported (log event plus a
//! status suffix) without any live effect.
//!
//! Default local behavior is preserved: a fresh owner adopts
//! [`tiler_core::workspace_scope::DEFAULT_WORKSPACE_MODE`], and the scoped
//! selection/history helpers below delegate local mode straight back to
//! [`crate::workspace::ManagedWorkspaces`] (same resolvers, same order, same
//! mutations), so the local path stays byte-for-byte equivalent.
//! Global-unique resolves over the same per-output scoped ring but records
//! into the retained [`tiler_core::workspace_scope::ScopeHistory`]; shared
//! resolves over one logical ring/history ([`WorkspaceScopeOwner::shared_ring`]
//! plus [`ScopeHistory`] shared entries) across output-bound Engine domains.
//! In particular hotplug/displacement/reconnect wiring lives in
//! `tiling_sys_scope_hotplug`, not here: this module only holds adopted mode,
//! scoped selection/history, and the shared-order baseline.
//!
//! Native effects, sends and trailing lifecycle use this policy through the
//! adjacent runtime scope modules. Stage C still owns disconnect/reconnect
//! baseline discard/re-priming and shared-order synchronization.

use std::collections::{BTreeMap, BTreeSet};

use tiler_core::workspace_scope::{
    DEFAULT_WORKSPACE_MODE, PreviousTarget, ScopeHistory, ScopedTopology, SwapPlan, WorkspaceMode,
    mode_token, plan_global_swap, resolve_numbered, resolve_relative_step, scoped_ring_ids,
};

use crate::workspace::{ManagedWorkspaces, WindowKey};

/// Lane name reported in `settings-applied` when a live file edit carries a
/// pending mode that needs an owner restart. The lane is informational only:
/// it never wakes the pump and never adopts.
pub const SCOPE_PENDING_LANE: &str = "workspace-scope-pending";

/// Adopted scope-mode owner state plus scoped selection/history.
///
/// `adopted` is fixed at startup from the validated saved mode and never
/// changes live. `history` records completed scoped transitions for
/// global-unique (per-output entries) and shared (single entry); it stays
/// empty for local mode, which keeps recording into `ManagedWorkspaces`.
/// `shared_order` is the single logical shared ring, synced explicitly via
/// `sync_shared_order` (hotplug keeps it stably synced).
#[derive(Debug, Clone)]
pub struct WorkspaceScopeOwner {
    adopted: WorkspaceMode,
    history: ScopeHistory,
    shared_order: Vec<String>,
}

impl Default for WorkspaceScopeOwner {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceScopeOwner {
    /// Fresh owner before startup adoption: default local mode, empty
    /// history and shared order. Local runtime behavior is unchanged.
    #[must_use]
    pub fn new() -> Self {
        Self {
            adopted: DEFAULT_WORKSPACE_MODE,
            history: ScopeHistory::new(),
            shared_order: Vec::new(),
        }
    }

    /// Startup adoption: the validated saved mode becomes the enforced mode
    /// for the whole run. Call once before `sync_monitor_outputs`.
    #[must_use]
    pub fn adopt_startup(saved: WorkspaceMode) -> Self {
        Self {
            adopted: saved,
            history: ScopeHistory::new(),
            shared_order: Vec::new(),
        }
    }

    /// Enforced mode for this run (startup-adopted, never live-adopted).
    #[must_use]
    pub const fn adopted(&self) -> WorkspaceMode {
        self.adopted
    }

    /// Canonical token of the enforced mode.
    #[must_use]
    pub fn adopted_token(&self) -> &'static str {
        mode_token(self.adopted)
    }

    /// Scoped history: per-output entries for global-unique plus the single
    /// shared entry. Empty for local mode (workspaces own local history).
    #[must_use]
    pub const fn history(&self) -> &ScopeHistory {
        &self.history
    }

    /// Mutable scoped history (selection resolution plus completed-transition
    /// recording for non-local modes).
    pub fn history_mut(&mut self) -> &mut ScopeHistory {
        &mut self.history
    }

    /// Single logical shared ring across output-bound domains. Empty until
    /// `sync_shared_order`; hotplug keeps it stably synced.
    #[must_use]
    pub const fn shared_order(&self) -> &Vec<String> {
        &self.shared_order
    }

    /// Stage an explicit shared order (tests plus later hotplug sync).
    pub fn set_shared_order(&mut self, order: Vec<String>) {
        self.shared_order = order;
    }

    /// Drift between the enforced startup mode and a newly saved mode.
    /// `None` means no restart is needed. Never mutates the owner.
    #[must_use]
    pub const fn drift(&self, saved: WorkspaceMode) -> Option<ScopeDrift> {
        if saved as u8 == self.adopted as u8 {
            None
        } else {
            Some(ScopeDrift {
                adopted: self.adopted,
                saved,
            })
        }
    }

    /// True when the saved mode differs from the enforced mode.
    #[must_use]
    pub const fn requires_restart(&self, saved: WorkspaceMode) -> bool {
        self.drift(saved).is_some()
    }

    /// Owner status for one applied revision. Matches the legacy
    /// `saved:{revision}` exactly when there is no drift; otherwise appends
    /// the bounded `+scope-restart:{saved-token}` suffix so the tray status
    /// row surfaces the pending restart. The suffix never feeds poll
    /// comparison (polls compare documents, not status).
    #[must_use]
    pub fn status_for(&self, revision: u64, saved: WorkspaceMode) -> String {
        match self.drift(saved) {
            None => format!("saved:{revision}"),
            Some(drift) => format!("saved:{revision}+scope-restart:{}", mode_token(drift.saved)),
        }
    }

    /// Startup evidence event. `outcome` is always `adopted`, `scope` is
    /// always `startup`: the saved mode is in force from here on.
    #[must_use]
    pub fn startup_event(&self) -> serde_json::Value {
        serde_json::json!({
            "event": "workspace-scope",
            "adopted": mode_token(self.adopted),
            "saved": mode_token(self.adopted),
            "outcome": "adopted",
            "scope": "startup",
        })
    }

    /// Drift evidence event for a live file edit. The saved pending choice is
    /// retained in the document; nothing adopts live.
    #[must_use]
    pub fn drift_event(&self, drift: ScopeDrift, revision: u64) -> serde_json::Value {
        serde_json::json!({
            "event": "workspace-scope",
            "adopted": mode_token(drift.adopted),
            "saved": mode_token(drift.saved),
            "revision": revision,
            "outcome": "restart-required",
            "scope": "live-pending",
        })
    }

    /// Pure scoped-topology constructor: passes live outputs,
    /// live backing ids, scoped assignments, and the primary adoption key
    /// straight into the core policy. No filtering or pruning here; the
    /// adapter owns empty removed-output pruning per the stage A contract.
    #[must_use]
    pub fn scoped_topology(
        outputs: Vec<String>,
        live_order: Vec<String>,
        assignments: BTreeMap<String, Vec<String>>,
        primary: Option<String>,
    ) -> ScopedTopology {
        ScopedTopology {
            outputs,
            live_order,
            assignments,
            primary,
        }
    }

    /// Live-filtered scoped ring for selection/send input. Delegates to the
    /// core policy; local mode returns the output's
    /// assignment, shared returns the shared order.
    #[must_use]
    pub fn ring_for(
        &self,
        mapping: &BTreeMap<String, Vec<String>>,
        output: &str,
        live_order: &[String],
    ) -> Vec<String> {
        scoped_ring_ids(
            self.adopted,
            mapping,
            &self.shared_order,
            output,
            live_order,
        )
    }

    /// Scoped ring for one output derived from live workspaces. Local and
    /// global-unique return the output's existing ids in order (including the
    /// trailing empty and ordinals beyond 9); shared returns the synced
    /// shared order filtered to live (empty until `sync_shared_order` runs).
    /// Selection over the ring creates nothing.
    #[must_use]
    pub fn scoped_ring(&self, workspaces: &ManagedWorkspaces, output: &str) -> Vec<String> {
        match self.adopted {
            WorkspaceMode::PerOutputLocal | WorkspaceMode::GlobalUnique => {
                workspaces.workspace_ids(output)
            }
            WorkspaceMode::Shared => self.shared_ring(workspaces),
        }
    }

    /// One logical shared ring across output-bound domains: the synced shared
    /// order filtered to currently live ids. Empty until `sync_shared_order`.
    #[must_use]
    pub fn shared_ring(&self, workspaces: &ManagedWorkspaces) -> Vec<String> {
        let live: BTreeSet<String> = mapping_from_workspaces(workspaces)
            .values()
            .flat_map(|ids| ids.iter().cloned())
            .collect();
        self.shared_order
            .iter()
            .filter(|id| live.contains(*id))
            .cloned()
            .collect()
    }

    /// Rebuild the shared order from live workspaces (deterministic output
    /// order, first-seen id order). Explicit startup sync; stage C
    /// wires hotplug/displacement sync. No effect on local/global rings.
    pub fn sync_shared_order(&mut self, workspaces: &ManagedWorkspaces) {
        let mapping = mapping_from_workspaces(workspaces);
        self.shared_order = live_union(&mapping);
    }

    /// Resolve a numbered selection (1..=9) without activating or creating.
    /// `0` refuses here (trailing path). Local delegates to the workspace
    /// resolver so the path stays byte-for-byte equivalent; global/shared
    /// resolve positionally over the scoped ring.
    #[must_use]
    pub fn resolve_numbered(
        &self,
        workspaces: &ManagedWorkspaces,
        output: &str,
        index: u8,
    ) -> Option<String> {
        match self.adopted {
            WorkspaceMode::PerOutputLocal => workspaces.resolve_send(output, index),
            WorkspaceMode::GlobalUnique | WorkspaceMode::Shared => {
                resolve_numbered(&self.scoped_ring(workspaces, output), index)
            }
        }
    }

    /// Resolve an ordinal ring step (-1/+1) without activating or creating.
    /// Local delegates to the workspace resolver; global resolves over the
    /// per-output scoped ring from the active view; shared resolves over the
    /// shared ring from the requesting output's bound current. Shared native
    /// selections synchronize this current across all output-bound domains.
    #[must_use]
    pub fn resolve_relative(
        &self,
        workspaces: &ManagedWorkspaces,
        output: &str,
        delta: i32,
    ) -> Option<String> {
        match self.adopted {
            WorkspaceMode::PerOutputLocal => workspaces.resolve_relative(output, delta),
            WorkspaceMode::GlobalUnique | WorkspaceMode::Shared => {
                let current = workspaces.active_id(output)?;
                resolve_relative_step(&self.scoped_ring(workspaces, output), &current, delta)
            }
        }
    }

    /// Resolve the previous-view toggle target without activating, appending,
    /// or creating. Local delegates to the workspace resolver; global
    /// validates the scoped per-output entry; shared validates the single
    /// shared entry against the shared ring. A surviving empty id stays
    /// valid; removed/out-of-scope clears to `None` (no-op until the next
    /// recorded change, never recreation or ordinal reinterpretation).
    /// Returning the active id itself is a valid no-op target: the caller
    /// treats it as already-there with no native effect and no history.
    pub fn resolve_previous(
        &mut self,
        workspaces: &mut ManagedWorkspaces,
        output: &str,
    ) -> Option<String> {
        match self.adopted {
            WorkspaceMode::PerOutputLocal => workspaces.resolve_previous(output),
            WorkspaceMode::GlobalUnique => {
                let scope = workspaces.workspace_ids(output);
                let current = workspaces.active_id(output).unwrap_or_default();
                match self.history.resolve_previous_for(output, &scope, &current) {
                    PreviousTarget::NoHistory => None,
                    PreviousTarget::AlreadyThere(id) | PreviousTarget::Select(id) => Some(id),
                }
            }
            WorkspaceMode::Shared => {
                let live = self.shared_ring(workspaces);
                let current = workspaces.active_id(output).unwrap_or_default();
                match self.history.resolve_shared_for(&live, &current) {
                    PreviousTarget::NoHistory => None,
                    PreviousTarget::AlreadyThere(id) | PreviousTarget::Select(id) => Some(id),
                }
            }
        }
    }

    /// Resolve the trailing path without creating (reuse check only). Local
    /// and global-unique reuse the output's trailing empty when unoccupied;
    /// shared reuses the shared ring's trailing empty when globally
    /// unoccupied (sticky members never occupy). Creation is owned by
    /// `resolve_trailing_select`.
    #[must_use]
    pub fn trailing_reuse_id(
        &self,
        workspaces: &ManagedWorkspaces,
        output: &str,
        sticky: &BTreeSet<WindowKey>,
    ) -> Option<String> {
        match self.adopted {
            WorkspaceMode::PerOutputLocal | WorkspaceMode::GlobalUnique => {
                let ids = workspaces.workspace_ids(output);
                let last = ids.last()?;
                if workspaces.workspace_members(output, last).is_empty() {
                    Some(last.clone())
                } else {
                    None
                }
            }
            WorkspaceMode::Shared => {
                let ring = self.shared_ring(workspaces);
                let last = ring.last()?;
                if !shared_occupied_excluding(workspaces, sticky).contains(last) {
                    Some(last.clone())
                } else {
                    None
                }
            }
        }
    }

    /// Resolve the trailing select path. Local/global-unique reuse or append
    /// through the workspace resolver (same mutations as today, preserving
    /// trailing lifecycle); shared reuses the shared trailing empty or mints
    /// one brand-new logical id across the shared order plus every
    /// output-bound domain (same id, fresh per-output tokens, no activation)
    /// and returns it for selection. `Unknown` means no output/ring to
    /// select.
    pub fn resolve_trailing_select(
        &mut self,
        workspaces: &mut ManagedWorkspaces,
        output: &str,
        sticky: &BTreeSet<WindowKey>,
    ) -> TrailingResolve {
        match self.adopted {
            WorkspaceMode::PerOutputLocal | WorkspaceMode::GlobalUnique => {
                match workspaces.resolve_send_trailing(output) {
                    Some((id, _)) => TrailingResolve::Reuse(id),
                    None => TrailingResolve::Unknown,
                }
            }
            WorkspaceMode::Shared => {
                let ring = self.shared_ring(workspaces);
                if ring.is_empty() {
                    return TrailingResolve::Unknown;
                }
                let occupied = shared_occupied_excluding(workspaces, sticky);
                if let Some(last) = ring.last()
                    && !occupied.contains(last)
                {
                    return TrailingResolve::Reuse(last.clone());
                }
                // Occupied shared tail: mint one logical trailing workspace
                // across all output domains so selection (and sends) proceed
                // like the local path instead of refusing.
                let outputs = workspaces.output_keys();
                let id = workspaces.mint_shared_id(&outputs);
                if id.is_empty() {
                    return TrailingResolve::Unknown;
                }
                if !self.shared_order.iter().any(|kept| kept == &id) {
                    self.shared_order.push(id.clone());
                }
                TrailingResolve::Reuse(id)
            }
        }
    }

    /// Record one successfully observed view change. Call only after the
    /// verified hide/reveal activation in `workspace_do_select` - including
    /// when later focus/geometry fails below. Never for same-view activation
    /// (returned early), failed hide/reveal (returned partial before
    /// activation), or focus-alone (`note_foreground` never records).
    /// Local records into workspaces; global records the per-output scoped
    /// entry; shared records the single shared entry over the shared ring.
    /// Returns true exactly when a new previous entry was recorded.
    pub fn record_completed(
        &mut self,
        workspaces: &mut ManagedWorkspaces,
        output: &str,
        current: &str,
    ) -> bool {
        match self.adopted {
            WorkspaceMode::PerOutputLocal => workspaces.observe_workspace_change(output, current),
            WorkspaceMode::GlobalUnique => {
                let scope = workspaces.workspace_ids(output);
                self.history.observe(output, current, &scope)
            }
            WorkspaceMode::Shared => {
                let live = self.shared_ring(workspaces);
                // Fall back to the live union when the shared order has not
                // been synced yet so an early completed transition still
                // primes honestly instead of validating against empty.
                if live.is_empty() {
                    let mapping = mapping_from_workspaces(workspaces);
                    self.history.observe_shared(&live_union(&mapping), current)
                } else {
                    self.history.observe_shared(&live, current)
                }
            }
        }
    }

    /// Prime one output's scoped baseline from its live active view without
    /// recording (creation/enable path). Local baselines already prime via
    /// `ensure_output`; this primes the retained scoped history for
    /// global-unique. Never consults history for selection.
    pub fn prime_output(&mut self, workspaces: &ManagedWorkspaces, output: &str) {
        if self.adopted == WorkspaceMode::GlobalUnique
            && let Some(active) = workspaces.active_id(output)
        {
            self.history.prime(output, &active);
        }
    }

    /// Prime the shared baseline from one output's live active view without
    /// recording. Later units call this on enable/creation; reconnect
    /// selection never consults history.
    pub fn prime_shared_view(&mut self, workspaces: &ManagedWorkspaces, output: &str) {
        if self.adopted == WorkspaceMode::Shared
            && let Some(active) = workspaces.active_id(output)
        {
            self.history.prime_shared(&active);
        }
    }

    /// Discard one output's scoped history (disconnect path). The caller
    /// still discards the workspace history separately; shared history
    /// persists across disconnects (scope-wide, never output-bound).
    pub fn discard_scoped_output(&mut self, output: &str) {
        self.history.discard_output(output);
    }

    /// Plan a global-unique visible-elsewhere swap without performing it.
    /// Returns `Some` when `target` is current on another output: the later
    /// swap unit must exchange the two views (two-output hide/reveal plus
    /// Engine session moves plus history on both outputs) instead of the
    /// single-output select. `None` means the ordinary single-output path
    /// applies (unknown view, already there, or not visible elsewhere).
    #[must_use]
    pub fn swap_plan_for(
        &self,
        workspaces: &ManagedWorkspaces,
        active_output: &str,
        target: &str,
    ) -> Option<SwapPlan> {
        if self.adopted != WorkspaceMode::GlobalUnique {
            return None;
        }
        let mut current_by_output = BTreeMap::new();
        for output in workspaces.output_keys() {
            if let Some(active) = workspaces.active_id(&output) {
                current_by_output.insert(output, active);
            }
        }
        plan_global_swap(&current_by_output, active_output, target)
    }

    /// Plan a shared selection without performing it. Returns `Some` when the
    /// target is in the shared ring: the later shared-effects unit must show
    /// it on every output-bound domain (`outputs`), move Engine sessions,
    /// and record one shared history entry. `None` means unknown target or
    /// non-shared mode. No mutation, no native effects here.
    #[must_use]
    pub fn plan_shared_select(
        &self,
        workspaces: &ManagedWorkspaces,
        target: &str,
    ) -> Option<SharedSelectPlan> {
        if self.adopted != WorkspaceMode::Shared {
            return None;
        }
        let ring = self.shared_ring(workspaces);
        if !ring.iter().any(|id| id == target) {
            return None;
        }
        Some(SharedSelectPlan {
            target: target.to_owned(),
            outputs: workspaces.output_keys(),
        })
    }
}

/// Scoped assignment snapshot: stable output keys to their scoped workspace
/// ids in order. Built from live workspaces; the adapter filters/adopts per
/// the stage A contract (no pruning here).
#[must_use]
pub fn mapping_from_workspaces(workspaces: &ManagedWorkspaces) -> BTreeMap<String, Vec<String>> {
    let mut mapping = BTreeMap::new();
    for output in workspaces.output_keys() {
        mapping.insert(output.clone(), workspaces.workspace_ids(&output));
    }
    mapping
}

/// Deterministic live id union over a scoped mapping: first-seen order
/// across outputs in key order.
#[must_use]
pub fn live_union(mapping: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    let mut live = Vec::new();
    for ids in mapping.values() {
        for id in ids {
            if !live.iter().any(|kept| kept == id) {
                live.push(id.clone());
            }
        }
    }
    live
}

/// Workspace ids with members on one output (trailing occupancy input).
#[must_use]
pub fn occupied_ids(workspaces: &ManagedWorkspaces, output: &str) -> BTreeSet<String> {
    workspaces
        .workspace_ids(output)
        .into_iter()
        .filter(|id| !workspaces.workspace_members(output, id).is_empty())
        .collect()
}

/// Workspace ids with members on any output (shared trailing occupancy).
/// Sticky all-desktop members never occupy: they stay visible everywhere
/// without holding any backing id.
#[must_use]
pub fn shared_occupied(workspaces: &ManagedWorkspaces) -> BTreeSet<String> {
    shared_occupied_excluding(workspaces, &BTreeSet::new())
}

/// Workspace ids with non-sticky members on any output (shared trailing
/// occupancy input). Sticky members stay members but count as empty, so a
/// sticky-only trailing id reuses like an empty one while the sticky float
/// state itself is retained by the owner tables.
#[must_use]
pub fn shared_occupied_excluding(
    workspaces: &ManagedWorkspaces,
    sticky: &BTreeSet<WindowKey>,
) -> BTreeSet<String> {
    let mut occupied = BTreeSet::new();
    for output in workspaces.output_keys() {
        for id in workspaces.workspace_ids(&output) {
            let members = workspaces.workspace_members(&output, &id);
            if members.iter().any(|m| !sticky.contains(m)) {
                occupied.insert(id);
            }
        }
    }
    occupied
}

/// Trailing select resolution: reuse selects the existing (or just-minted)
/// trailing id; unknown has no ring/output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrailingResolve {
    /// Select the trailing id (reused empty, or newly minted for shared).
    Reuse(String),
    /// Unknown output or empty ring: no-op until the next change.
    Unknown,
}

/// Planned shared selection across output-bound Engine domains (map-only).
/// The later shared-effects unit performs the native writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedSelectPlan {
    /// Shared target workspace id (in the shared ring).
    pub target: String,
    /// Output keys that must converge onto the target.
    pub outputs: Vec<String>,
}

/// Reported drift between the enforced startup mode and a saved mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScopeDrift {
    /// Enforced mode for this run (startup-adopted).
    pub adopted: WorkspaceMode,
    /// Newly saved mode (pending until the next owner restart).
    pub saved: WorkspaceMode,
}

/// Startup mode for one owner run: the validated saved mode, or the default
/// local mode when there are no live settings (proof owners) or the document
/// carries the backfilled default.
#[must_use]
pub fn startup_mode(live: Option<&crate::settings::LiveSettings>) -> WorkspaceMode {
    live.map(|live| live.settings.core.workspace.mode_enum())
        .unwrap_or(DEFAULT_WORKSPACE_MODE)
}

/// Saved mode of one validated settings document.
#[must_use]
pub fn saved_mode(settings: &crate::settings::Settings) -> WorkspaceMode {
    settings.core.workspace.mode_enum()
}

/// Pending lane plus drift evidence for one validated live document.
///
/// Compares the saved mode against the owner's startup-adopted mode and
/// returns the pending lane with its `workspace-scope` event when they
/// differ. The caller logs the event. Never mutates the adopted mode: the
/// saved pending choice stays in the document and takes effect only on the
/// next owner restart. Returns `None` when there is no drift (no lane, no
/// event).
#[must_use]
pub fn pending_scope_lane(
    owner: &WorkspaceScopeOwner,
    settings: &crate::settings::Settings,
) -> Option<(&'static str, serde_json::Value)> {
    let saved = saved_mode(settings);
    let drift = owner.drift(saved)?;
    Some((
        SCOPE_PENDING_LANE,
        owner.drift_event(drift, settings.revision),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiler_core::workspace_scope::WorkspaceMode::{GlobalUnique, PerOutputLocal, Shared};

    #[test]
    fn fresh_owner_adopts_default_local_with_empty_scaffolding() {
        let owner = WorkspaceScopeOwner::new();
        assert_eq!(owner.adopted(), PerOutputLocal);
        assert_eq!(owner.adopted_token(), "per-output-local");
        assert_eq!(owner.history().previous("mon-a"), None);
        assert!(owner.shared_order().is_empty());
    }

    #[test]
    fn startup_adoption_pins_each_saved_mode() {
        for mode in [PerOutputLocal, GlobalUnique, Shared] {
            let owner = WorkspaceScopeOwner::adopt_startup(mode);
            assert_eq!(owner.adopted(), mode);
            assert!(!owner.requires_restart(mode));
            assert_eq!(owner.drift(mode), None);
        }
    }

    #[test]
    fn drift_reports_saved_pending_without_live_adoption() {
        let mut owner = WorkspaceScopeOwner::adopt_startup(PerOutputLocal);
        let drift = owner.drift(GlobalUnique).expect("drift");
        assert_eq!(
            drift,
            ScopeDrift {
                adopted: PerOutputLocal,
                saved: GlobalUnique,
            }
        );
        assert!(owner.requires_restart(GlobalUnique));
        // Reporting never adopts: the enforced mode is unchanged.
        let _ = owner.history_mut();
        owner.set_shared_order(vec!["ws-1".to_owned()]);
        assert_eq!(owner.adopted(), PerOutputLocal);
        assert_eq!(
            owner.drift(GlobalUnique).expect("still drifted").saved,
            GlobalUnique
        );
    }

    #[test]
    fn status_matches_legacy_without_drift_and_suffixes_with_drift() {
        let owner = WorkspaceScopeOwner::adopt_startup(PerOutputLocal);
        assert_eq!(owner.status_for(7, PerOutputLocal), "saved:7");
        assert_eq!(
            owner.status_for(7, GlobalUnique),
            "saved:7+scope-restart:global-unique"
        );
        assert_eq!(owner.status_for(9, Shared), "saved:9+scope-restart:shared");
    }

    #[test]
    fn startup_mode_defaults_local_without_live_settings() {
        assert_eq!(startup_mode(None), PerOutputLocal);
        let live = crate::settings::LiveSettings::fresh(crate::settings::Settings::default(), None);
        assert_eq!(startup_mode(Some(&live)), PerOutputLocal);
        let mut shared = crate::settings::Settings::default();
        shared.core.workspace.mode = "shared".to_owned();
        let live = crate::settings::LiveSettings::fresh(shared, None);
        assert_eq!(startup_mode(Some(&live)), Shared);
    }

    #[test]
    fn scoped_topology_and_ring_preserve_local_behavior() {
        let owner = WorkspaceScopeOwner::adopt_startup(PerOutputLocal);
        let mut assignments = BTreeMap::new();
        assignments.insert(
            "mon-a".to_owned(),
            vec!["ws-1".to_owned(), "ws-2".to_owned()],
        );
        let topology = WorkspaceScopeOwner::scoped_topology(
            vec!["mon-a".to_owned()],
            vec!["ws-1".to_owned(), "ws-2".to_owned()],
            assignments.clone(),
            None,
        );
        assert_eq!(topology.outputs, vec!["mon-a".to_owned()]);
        let live = vec!["ws-1".to_owned(), "ws-2".to_owned()];
        assert_eq!(owner.ring_for(&assignments, "mon-a", &live), live);
        assert!(
            owner
                .ring_for(&assignments, "mon-missing", &live)
                .is_empty()
        );
        // Shared scaffolding: an empty shared order yields an empty ring.
        let shared_owner = WorkspaceScopeOwner::adopt_startup(Shared);
        assert!(
            shared_owner
                .ring_for(&assignments, "mon-a", &live)
                .is_empty()
        );
    }

    #[test]
    fn startup_and_drift_events_carry_bounded_vocabulary() {
        let owner = WorkspaceScopeOwner::adopt_startup(GlobalUnique);
        let started = owner.startup_event();
        assert_eq!(started["event"], "workspace-scope");
        assert_eq!(started["adopted"], "global-unique");
        assert_eq!(started["outcome"], "adopted");
        assert_eq!(started["scope"], "startup");
        let drift = owner.drift(Shared).expect("drift");
        let event = owner.drift_event(drift, 11);
        assert_eq!(event["event"], "workspace-scope");
        assert_eq!(event["adopted"], "global-unique");
        assert_eq!(event["saved"], "shared");
        assert_eq!(event["outcome"], "restart-required");
        assert_eq!(event["scope"], "live-pending");
        assert_eq!(event["revision"], 11);
    }
}
