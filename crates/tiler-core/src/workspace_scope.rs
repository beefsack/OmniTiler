//! Pure workspace scope/order/history/ring/assignment/swap policy.
//!
//! Shared owner-facing policy for the three workspace modes
//! (`per-output-local` default, `global-unique`, `shared`). Local and
//! global-unique keep per-output history/rings and scoped numbering;
//! global-unique additionally swaps a target that is visible on another
//! output instead of duplicating the selection; shared keeps one history
//! and one ring across all outputs.
//!
//! Every function here is pure: mapping rebuilds return fresh assignment
//! plans, history observations record completed transitions, and swap,
//! selection, send and migration resolvers return plans. Adapters own all
//! runtime effects (native reads/writes, creation, removal, focus). This
//! mirrors the KDE `WorkspaceNativeAdapter` seams (`selectPrevious` /
//! `selectPreviousShared`, `selectRelative` / `selectRelativeShared`,
//! `rebuildGlobalMapping` / `rebuildLocalMapping`, `scopedRingIds`,
//! `swapGlobalIfVisibleElsewhere`, `recordHistoryObservations` /
//! `observeSharedHistory`) without any host coupling.
//!
//! Invariants (REQ-WS-08/11, R-WS-08/11/15/16/17):
//! - Record every successful observed workspace change regardless of
//!   producer (commands, native switches, verified send-follow, hotplug).
//!   Same-workspace activation and output focus alone never record.
//! - A surviving workspace, even empty, stays a valid previous target.
//!   A removed, moved, or out-of-scope id clears the previous entry;
//!   toggle is a no-op until the next recorded change. No ordinal
//!   reinterpretation or recreation.
//! - A disconnected output's history is discarded with the output;
//!   reconnect selection never consults or restores history (reconnect
//!   only re-primes baselines, so later changes record anew).
//! - The ring is every existing scoped id in order, including the
//!   trailing empty and ordinals beyond 9; first/last wrap. Selection
//!   itself creates nothing; the trailing path reuses or reports creation.
//! - Trailing lifecycle reuses [`crate::workspace::plan_trailing`] over
//!   scoped facts (see [`scoped_trailing_facts`]); the global live-count
//!   floor stays an adapter-side gate next to the scoped plan.

use std::collections::{BTreeMap, BTreeSet};

use crate::workspace::{self, WorkspaceFacts};

/// Workspace scope mode token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkspaceMode {
    /// Independent per-output workspace sets (default).
    PerOutputLocal,
    /// Distinct per-output sets with cross-output visible-elsewhere swap.
    GlobalUnique,
    /// One workspace set shown across all outputs.
    Shared,
}

/// Default scope: per-output-local sets.
pub const DEFAULT_WORKSPACE_MODE: WorkspaceMode = WorkspaceMode::PerOutputLocal;

/// Canonical mode tokens, in stable order.
pub const WORKSPACE_MODE_TOKENS: [&str; 3] = ["per-output-local", "global-unique", "shared"];

/// Parse a mode token; unknown values fall back to the default.
#[must_use]
pub fn parse_workspace_mode(value: &str) -> WorkspaceMode {
    match value {
        "global-unique" => WorkspaceMode::GlobalUnique,
        "shared" => WorkspaceMode::Shared,
        _ => WorkspaceMode::PerOutputLocal,
    }
}

/// Canonical token for a mode.
#[must_use]
pub fn mode_token(mode: WorkspaceMode) -> &'static str {
    match mode {
        WorkspaceMode::PerOutputLocal => "per-output-local",
        WorkspaceMode::GlobalUnique => "global-unique",
        WorkspaceMode::Shared => "shared",
    }
}

/// Pure topology inputs for a mapping rebuild.
///
/// `assignments` maps stable output keys to their scoped workspace ids in
/// order. Entries for removed outputs are retained (filtered to live) so a
/// later displacement pass can move them as a unit; pruning empty
/// removed-output entries is an adapter-side responsibility until the
/// displacement lifetime is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedTopology {
    /// Live output keys in deterministic order.
    pub outputs: Vec<String>,
    /// Live backing ids in native order.
    pub live_order: Vec<String>,
    /// Scoped assignments per output key (ignored for shared).
    pub assignments: BTreeMap<String, Vec<String>>,
    /// Preferred adoption key for unassigned live ids; falls back to the
    /// first live output when absent or not live.
    pub primary: Option<String>,
}

/// Result of a pure mapping rebuild: the retained assignment plus the live
/// ids adopted onto the primary (the adapter marks those managed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingRebuild {
    pub mapping: BTreeMap<String, Vec<String>>,
    pub adopted: Vec<String>,
}

fn rebuild_scoped(topology: &ScopedTopology) -> MappingRebuild {
    let live: BTreeSet<&str> = topology.live_order.iter().map(String::as_str).collect();
    let mut outputs: Vec<&str> = Vec::new();
    for key in &topology.outputs {
        if !outputs.contains(&key.as_str()) {
            outputs.push(key.as_str());
        }
    }
    let mut mapping: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (key, ids) in &topology.assignments {
        let kept: Vec<String> = ids
            .iter()
            .filter(|id| live.contains(id.as_str()))
            .cloned()
            .collect();
        mapping.insert(key.clone(), kept);
    }
    let primary = topology
        .primary
        .as_deref()
        .filter(|key| outputs.contains(key))
        .or_else(|| outputs.first().copied());
    let mut adopted = Vec::new();
    if let Some(primary) = primary {
        let assigned: BTreeSet<String> = mapping
            .values()
            .flat_map(|ids| ids.iter().cloned())
            .collect();
        let entry = mapping.entry(primary.to_owned()).or_default();
        for id in &topology.live_order {
            if !assigned.contains(id) && !entry.iter().any(|kept| kept == id) {
                entry.push(id.clone());
                adopted.push(id.clone());
            }
        }
    }
    MappingRebuild { mapping, adopted }
}

/// Rebuild the per-output-local assignment: retain filtered lists, adopt
/// unassigned live ids onto the primary. Mirrors `rebuildLocalMapping`.
#[must_use]
pub fn rebuild_local_mapping(topology: &ScopedTopology) -> MappingRebuild {
    rebuild_scoped(topology)
}

/// Rebuild the global-unique assignment: same retention/adoption shape as
/// local (empty removed-output entries are conservatively retained; the
/// adapter prunes them once the displacement lifetime is known). The modes
/// differ in swap selection, not in rebuild. Mirrors
/// `rebuildGlobalMapping` apart from that documented retention difference.
#[must_use]
pub fn rebuild_global_mapping(topology: &ScopedTopology) -> MappingRebuild {
    rebuild_scoped(topology)
}

/// Rebuild the shared order: the live native order itself. Mirrors
/// `rebuildSharedMapping`.
#[must_use]
pub fn rebuild_shared_order(live_order: &[String]) -> Vec<String> {
    live_order.to_vec()
}

/// Scoped ring ids: every existing scoped id in order, including the
/// trailing empty and ordinals beyond 9. Shared returns the shared order;
/// local/global-unique return the output's assignment. All filtered to
/// live. Selection over the ring creates nothing.
#[must_use]
pub fn scoped_ring_ids(
    mode: WorkspaceMode,
    mapping: &BTreeMap<String, Vec<String>>,
    shared_order: &[String],
    output: &str,
    live_order: &[String],
) -> Vec<String> {
    let live: BTreeSet<&str> = live_order.iter().map(String::as_str).collect();
    if mode == WorkspaceMode::Shared {
        return shared_order
            .iter()
            .filter(|id| live.contains(id.as_str()))
            .cloned()
            .collect();
    }
    mapping
        .get(output)
        .map(|ids| {
            ids.iter()
                .filter(|id| live.contains(id.as_str()))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// Resolve a numbered selection (1..=9) to a stable id positionally. `0`
/// takes the trailing path instead and refuses here. Never creates.
#[must_use]
pub fn resolve_numbered(ring: &[String], index: u8) -> Option<String> {
    let pos = workspace::select_existing(ring.len(), index)?;
    ring.get(pos).cloned()
}

/// Resolve an ordinal ring step with first/last wrap. `delta` must be -1
/// (previous) or +1 (next); unknown current ids and empty rings refuse.
/// Never creates.
#[must_use]
pub fn resolve_relative_step(ring: &[String], current: &str, delta: i32) -> Option<String> {
    if delta != -1 && delta != 1 || ring.is_empty() {
        return None;
    }
    let at = ring.iter().position(|id| id == current)?;
    let next = ring[((at as i32 + delta + ring.len() as i32) as usize) % ring.len()].clone();
    Some(next)
}

/// Resolve a relative send target: the same scoped ring as selection,
/// resolved once before transfer. Filling a resolved trailing empty is an
/// ordinary transfer; lifecycle supplies the next spare afterwards.
#[must_use]
pub fn resolve_relative_send(ring: &[String], current: &str, delta: i32) -> Option<String> {
    resolve_relative_step(ring, current, delta)
}

/// Trailing selection outcome: reuse the live trailing empty, or create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrailingSelection {
    Reuse(String),
    Create,
}

/// Resolve the trailing path: reuse the literal last scoped id when it is
/// unoccupied, else report creation. Idempotent while already on the
/// trailing empty (returns `Reuse` of that same id).
///
/// Precondition: `ring` is a live-filtered scoped ring (the output of
/// [`scoped_ring_ids`]). Liveness is established by the caller; every id
/// in the ring is treated as a live reuse candidate by definition.
#[must_use]
pub fn resolve_trailing(ring: &[String], occupied: &BTreeSet<String>) -> TrailingSelection {
    if let Some(last) = ring.last()
        && !occupied.contains(last)
    {
        return TrailingSelection::Reuse(last.clone());
    }
    TrailingSelection::Create
}

/// Previous-toggle resolution for one scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviousTarget {
    /// No recorded change yet, or the entry was invalidated: no-op until
    /// the next recorded change. Never recreates or reinterprets ordinals.
    NoHistory,
    /// The remembered id is the current view: already there, no effect.
    AlreadyThere(String),
    /// Select the remembered stable id.
    Select(String),
}

/// Scoped previous-view history: per-output entries for local/global-unique
/// plus one shared entry. Baselines prime without recording; only completed
/// observed changes record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopeHistory {
    previous: BTreeMap<String, String>,
    baseline: BTreeMap<String, String>,
    shared_previous: Option<String>,
    shared_baseline: Option<String>,
}

/// One recorded observation: the output whose view changed, its new
/// previous, and the new current.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedChange {
    pub output: String,
    pub previous: String,
    pub current: String,
}

/// One live output view for batch observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveView {
    /// Currently observed workspace id on the output.
    pub current: String,
    /// The recording output's scoped live ids (ring scope).
    pub scope: Vec<String>,
}

impl ScopeHistory {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Previously recorded id for one output, if any.
    #[must_use]
    pub fn previous(&self, output: &str) -> Option<String> {
        self.previous.get(output).cloned()
    }

    /// Observation baseline for one output, if primed.
    #[must_use]
    pub fn baseline(&self, output: &str) -> Option<String> {
        self.baseline.get(output).cloned()
    }

    /// Record one successfully observed view change. Only completed
    /// transitions call this (verified selects, native switches, verified
    /// send-follow arrivals, hotplug): never attempted setters,
    /// same-workspace activation, or output focus alone. Unknown current
    /// ids never record. Returns true exactly when this call records a new
    /// transition (a valid previous now exists).
    ///
    /// Eager validated observation contract: when the previous baseline
    /// has left the scope, the previous entry clears eagerly as part of
    /// the same observation instead of surviving until a later validation
    /// pass; the final history matches validate-after-record.
    pub fn observe(&mut self, output: &str, current: &str, scope: &[String]) -> bool {
        if !scope.iter().any(|id| id == current) {
            self.validate_output(output, scope);
            return false;
        }
        match self.baseline.get(output).cloned() {
            None => {
                self.baseline.insert(output.to_owned(), current.to_owned());
                self.validate_output(output, scope);
                false
            }
            Some(base) if base == current => {
                self.validate_output(output, scope);
                false
            }
            Some(base) => {
                if scope.iter().any(|id| id == &base) {
                    self.previous.insert(output.to_owned(), base);
                } else {
                    self.previous.remove(output);
                }
                self.baseline.insert(output.to_owned(), current.to_owned());
                self.validate_output(output, scope);
                self.previous.contains_key(output)
            }
        }
    }

    /// Observe all live outputs at once: records actual view changes,
    /// ignores unchanged views, and discards histories (previous plus
    /// baseline) for outputs absent from the live set. Outputs with an
    /// unknown current record nothing.
    pub fn observe_all(&mut self, live: &BTreeMap<String, LiveView>) -> Vec<RecordedChange> {
        let mut recorded = Vec::new();
        for (output, view) in live {
            if self.observe(output, &view.current, &view.scope)
                && let Some(previous) = self.previous.get(output).cloned()
            {
                recorded.push(RecordedChange {
                    output: output.clone(),
                    previous,
                    current: view.current.clone(),
                });
            }
        }
        let live_keys: BTreeSet<&str> = live.keys().map(String::as_str).collect();
        for key in self
            .previous
            .keys()
            .chain(self.baseline.keys())
            .cloned()
            .collect::<Vec<_>>()
        {
            if !live_keys.contains(key.as_str()) {
                self.previous.remove(&key);
                self.baseline.remove(&key);
            }
        }
        recorded
    }

    /// Resolve the previous-view toggle target with validation: a removed
    /// or out-of-scope id clears and reports [`PreviousTarget::NoHistory`].
    /// A surviving empty id stays valid.
    pub fn resolve_previous_for(
        &mut self,
        output: &str,
        scope: &[String],
        current: &str,
    ) -> PreviousTarget {
        let Some(id) = self.previous.get(output).cloned() else {
            return PreviousTarget::NoHistory;
        };
        if !scope.iter().any(|kept| kept == &id) {
            self.previous.remove(output);
            return PreviousTarget::NoHistory;
        }
        if id == current {
            return PreviousTarget::AlreadyThere(id);
        }
        PreviousTarget::Select(id)
    }

    /// Prime a baseline without recording (output creation, enable, or
    /// reconnect return). Reconnect selection never consults history; it
    /// only re-primes, so later changes record anew.
    pub fn prime(&mut self, output: &str, current: &str) {
        self.baseline.insert(output.to_owned(), current.to_owned());
    }

    /// Discard one output's history (disconnect handling). Never consulted
    /// for reconnect selection.
    pub fn discard_output(&mut self, output: &str) {
        self.previous.remove(output);
        self.baseline.remove(output);
    }

    /// Record one successfully observed shared change over the full shared
    /// order. Same record/no-op/invalidate rules as [`Self::observe`].
    pub fn observe_shared(&mut self, live: &[String], current: &str) -> bool {
        if !live.iter().any(|id| id == current) {
            self.validate_shared(live);
            return false;
        }
        match self.shared_baseline.clone() {
            None => {
                self.shared_baseline = Some(current.to_owned());
                self.validate_shared(live);
                false
            }
            Some(base) if base == current => {
                self.validate_shared(live);
                false
            }
            Some(base) => {
                if live.iter().any(|id| id == &base) {
                    self.shared_previous = Some(base);
                } else {
                    self.shared_previous = None;
                }
                self.shared_baseline = Some(current.to_owned());
                self.validate_shared(live);
                self.shared_previous.is_some()
            }
        }
    }

    /// Resolve the shared previous-view toggle with validation.
    pub fn resolve_shared_for(&mut self, live: &[String], current: &str) -> PreviousTarget {
        let Some(id) = self.shared_previous.clone() else {
            return PreviousTarget::NoHistory;
        };
        if !live.iter().any(|kept| kept == &id) {
            self.shared_previous = None;
            return PreviousTarget::NoHistory;
        }
        if id == current {
            return PreviousTarget::AlreadyThere(id);
        }
        PreviousTarget::Select(id)
    }

    /// Prime the shared baseline without recording.
    pub fn prime_shared(&mut self, current: &str) {
        self.shared_baseline = Some(current.to_owned());
    }

    /// Previously recorded shared id, if any.
    #[must_use]
    pub fn shared_previous(&self) -> Option<String> {
        self.shared_previous.clone()
    }

    /// Discard the shared history.
    pub fn discard_shared(&mut self) {
        self.shared_previous = None;
        self.shared_baseline = None;
    }

    fn validate_output(&mut self, output: &str, scope: &[String]) {
        if let Some(prev) = self.previous.get(output).cloned()
            && !scope.iter().any(|id| id == &prev)
        {
            self.previous.remove(output);
        }
    }

    fn validate_shared(&mut self, live: &[String]) {
        if let Some(prev) = self.shared_previous.clone()
            && !live.iter().any(|id| id == &prev)
        {
            self.shared_previous = None;
        }
    }
}

/// Pure global-unique swap plan: when the selection target is visible on
/// another output, exchange the two views instead of duplicating the
/// selection. Mirrors `swapGlobalIfVisibleElsewhere` (map-only part; the
/// adapter performs the two native writes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwapPlan {
    pub target: String,
    pub active_output: String,
    pub active_current: String,
    pub other_output: String,
}

/// Plan a visible-elsewhere swap. `current_by_output` holds the observed
/// current id per live output key. Returns `None` when no swap applies:
/// unknown active view, already there, or the target is not visible
/// elsewhere.
#[must_use]
pub fn plan_global_swap(
    current_by_output: &BTreeMap<String, String>,
    active_output: &str,
    target: &str,
) -> Option<SwapPlan> {
    let active_current = current_by_output.get(active_output)?.clone();
    if active_current == target {
        return None;
    }
    for (output, current) in current_by_output {
        if output != active_output && current == target {
            return Some(SwapPlan {
                target: target.to_owned(),
                active_output: active_output.to_owned(),
                active_current,
                other_output: output.clone(),
            });
        }
    }
    None
}

/// Apply a [`SwapPlan`] to a scoped assignment: the target moves to the
/// active key's list and the active current moves to the other key's list
/// (both appended at the end, matching the adapter mapping order).
#[must_use]
pub fn apply_swap_assignments(
    mapping: &BTreeMap<String, Vec<String>>,
    plan: &SwapPlan,
) -> BTreeMap<String, Vec<String>> {
    let mut next = mapping.clone();
    for ids in next.values_mut() {
        ids.retain(|id| id != &plan.target && id != &plan.active_current);
    }
    next.entry(plan.active_output.clone())
        .or_default()
        .push(plan.target.clone());
    next.entry(plan.other_output.clone())
        .or_default()
        .push(plan.active_current.clone());
    next
}

/// Source-refill choice for a migration that empties its source scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationRefill {
    /// Keep the last remaining scoped id (default).
    LastRemaining,
    /// Keep the snapshotted per-output previous id when it survives among
    /// the remaining scope, else the last remaining.
    MostRecentlyUsed,
}

/// Pure whole-workspace migration refusal reasons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationRefusal {
    SharedMode,
    UnknownOutput,
    SameOutput,
    UnknownWorkspace,
    OutOfScope,
    DuplicateTarget,
    TargetCurrentUnknown,
}

/// Pure whole-workspace migration inputs: move one live backing id from
/// the source scope to the target scope.
///
/// `live` is the global live id set; `source_current` is the source's live
/// current view when readable (a still-scoped live current is always kept
/// as the refill); `previous_before` is the snapshotted per-output previous
/// id for `MostRecentlyUsed` refill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationRequest {
    pub mode: WorkspaceMode,
    pub live: BTreeSet<String>,
    pub source: String,
    pub target: String,
    pub workspace: String,
    pub target_current: String,
    pub source_current: Option<String>,
    pub previous_before: Option<String>,
    pub refill: MigrationRefill,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationPlan {
    /// Move one live backing id from the source scope to the target scope,
    /// inserted immediately after the target's live current. The target
    /// prior workspace stays listed and hidden. Map-only: native views and
    /// lifecycle converge on the caller.
    pub moved_id: String,
    pub source: String,
    pub target: String,
    pub insert_after: String,
    /// Source refill id, or `None` when the source keeps no workspace
    /// (ordinary trailing lifecycle converges later; nothing recreates).
    pub refill: Option<String>,
    /// Whether a source-scope previous entry naming the moved id must be
    /// invalidated (1.3/1.5 history rule).
    pub invalidate_previous_in_source: bool,
}

/// Plan a whole-workspace migration over scoped assignments.
pub fn plan_migration(
    mapping: &BTreeMap<String, Vec<String>>,
    request: &MigrationRequest,
) -> Result<MigrationPlan, MigrationRefusal> {
    let mode = request.mode;
    let live = &request.live;
    let source = request.source.as_str();
    let target = request.target.as_str();
    let workspace = request.workspace.as_str();
    let target_current = request.target_current.as_str();
    let source_current = request.source_current.as_deref();
    let previous_before = request.previous_before.as_deref();
    let refill = request.refill;
    if mode == WorkspaceMode::Shared {
        return Err(MigrationRefusal::SharedMode);
    }
    if source == target {
        return Err(MigrationRefusal::SameOutput);
    }
    let source_list = mapping.get(source).ok_or(MigrationRefusal::UnknownOutput)?;
    let target_list = mapping.get(target).ok_or(MigrationRefusal::UnknownOutput)?;
    if !live.contains(workspace) {
        return Err(MigrationRefusal::UnknownWorkspace);
    }
    if !source_list.iter().any(|id| id == workspace) {
        return Err(MigrationRefusal::OutOfScope);
    }
    if target_list.iter().any(|id| id == workspace) {
        return Err(MigrationRefusal::DuplicateTarget);
    }
    if target_current == workspace
        || !live.contains(target_current)
        || !target_list.iter().any(|id| id == target_current)
    {
        return Err(MigrationRefusal::TargetCurrentUnknown);
    }
    let ring: Vec<String> = source_list
        .iter()
        .filter(|id| id.as_str() != workspace && live.contains(id.as_str()))
        .cloned()
        .collect();
    let refill_id = match source_current {
        Some(current) if ring.iter().any(|id| id == current) => Some(current.to_owned()),
        _ => match refill {
            MigrationRefill::MostRecentlyUsed => match previous_before {
                Some(prev) if ring.iter().any(|id| id == prev) => Some(prev.to_owned()),
                _ => ring.last().cloned(),
            },
            MigrationRefill::LastRemaining => ring.last().cloned(),
        },
    };
    Ok(MigrationPlan {
        moved_id: workspace.to_owned(),
        source: source.to_owned(),
        target: target.to_owned(),
        insert_after: target_current.to_owned(),
        refill: refill_id,
        invalidate_previous_in_source: previous_before == Some(workspace),
    })
}

/// Apply a [`MigrationPlan`] to a scoped assignment.
#[must_use]
pub fn apply_migration(
    mapping: &BTreeMap<String, Vec<String>>,
    plan: &MigrationPlan,
) -> BTreeMap<String, Vec<String>> {
    let mut next = mapping.clone();
    if let Some(source) = next.get_mut(&plan.source) {
        source.retain(|id| id != &plan.moved_id);
    }
    let target = next.entry(plan.target.clone()).or_default();
    if !target.iter().any(|id| id == &plan.moved_id) {
        match target.iter().position(|id| id == &plan.insert_after) {
            Some(at) => target.insert(at + 1, plan.moved_id.clone()),
            None => target.push(plan.moved_id.clone()),
        }
    }
    next
}

/// Build scoped trailing facts for [`workspace::plan_trailing`] from one
/// scope's ring: empty means unoccupied, visible from the observed view
/// set, retained covers displaced plus transaction-retained ids. Sticky
/// all-desktop windows never occupy a backing id and stay out of
/// `occupied`.
#[must_use]
pub fn scoped_trailing_facts(
    ring: &[String],
    visible: &BTreeSet<String>,
    occupied: &BTreeSet<String>,
    retained: &BTreeSet<String>,
) -> Vec<WorkspaceFacts> {
    ring.iter()
        .map(|id| WorkspaceFacts {
            empty: !occupied.contains(id),
            visible: visible.contains(id),
            occupied: occupied.contains(id),
            retained: retained.contains(id),
        })
        .collect()
}
