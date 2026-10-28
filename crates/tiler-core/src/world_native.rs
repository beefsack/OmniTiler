//! Host-owned native workspace adoption (capability-gated).
//!
//! Split from [`crate::world_engine`]: same `impl WorldEngine` ownership,
//! only relocated for source size. Maps `(output, native desktop id)` to
//! stable core logical workspace ids, preserves native order at initial
//! adoption, binds outside-created native desktops, and derives logical
//! receipts from native evidence for the existing homing/settlement path.
//! No behavior change when `host_owned_workspace_domains` is off.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::OutputId;
use crate::world_engine::WorldEngine;
use crate::world_report::{WindowFact, WorldReport};

/// True when the shared first-appearance union across sorted outputs
/// would reverse some output's reported native order. Choosing another
/// merge order is unresolved, so the fresh projection is ambiguous.
/// Identical lists and disjoint lists are consistent. Pure report shape:
/// outputs without a row carry no information and are skipped.
pub(crate) fn shared_order_conflict(report: &WorldReport) -> bool {
    let mut ordered: Vec<&str> = report
        .screens
        .iter()
        .map(|screen| screen.output.0.as_str())
        .collect();
    ordered.sort();
    let mut union: Vec<&str> = Vec::new();
    for output in &ordered {
        if let Some(entry) = report.native.iter().find(|entry| entry.output.0 == *output) {
            for id in &entry.native_order {
                if !union.contains(&id.as_str()) {
                    union.push(id);
                }
            }
        }
    }
    let position = |id: &str| union.iter().position(|member| *member == id);
    for output in &ordered {
        if let Some(entry) = report.native.iter().find(|entry| entry.output.0 == *output) {
            let mut last: Option<usize> = None;
            for id in &entry.native_order {
                let Some(at) = position(id) else {
                    continue;
                };
                if last.is_some_and(|previous| at <= previous) {
                    return true;
                }
                last = Some(at);
            }
        }
    }
    false
}

impl WorldEngine {
    /// Logical id bound to one `(output, native id)` pair, if any.
    pub(crate) fn native_logical(&self, output: &OutputId, native_id: &str) -> Option<String> {
        self.native_map
            .get(&(output.0.clone(), native_id.to_owned()))
            .cloned()
    }

    /// Publish-time native binding for one workspace row: the native id
    /// observed for `(output, logical)` pair, else `None` (trailing spare,
    /// native-free row, or a logical present on an output that never
    /// observed its native id, e.g. after migration or shared mirroring).
    /// Only the exact observed pair publishes; never fabricated from
    /// another output's observation.
    pub(crate) fn native_binding_for(&self, output: &str, logical: &str) -> Option<String> {
        self.native_map
            .iter()
            .find(|((out, _), id)| out == output && id.as_str() == logical)
            .map(|((_, native), _)| native.clone())
    }

    /// Whether one logical id is native-bound on any output (retained
    /// against trailing-lifecycle removal even when empty).
    pub(crate) fn is_native_bound(&self, logical: &str) -> bool {
        self.native_map.values().any(|id| id == logical)
    }

    /// Whether native evidence is authoritative for one output: the
    /// capability is on. Host-owned mode is native-only: the core derives
    /// every logical receipt from per-window `native_workspace` plus the
    /// per-output `native[]` order/current, and never reads the adapter
    /// logical receipt (`observed_workspace`) on any output. Tiler-owned
    /// Windows never sets the capability and keeps the logical route
    /// unchanged.
    /// Capability-named note: native-only evidence, core-derived receipts.
    pub(crate) fn native_authoritative(&self, report: &WorldReport, output: &OutputId) -> bool {
        let _ = output;
        report.host_owned_workspace_domains
    }

    /// Effective logical receipt for one fact: the native-derived logical
    /// id when the capability is on (native-only, the adapter logical
    /// receipt is never consulted), else the adapter's logical receipt.
    ///
    /// Host-owned rules (native-only; unknown/unmapped holds, never invent
    /// or collapse):
    /// - `native_workspace: Some(nid)` mapped to `L` yields `Some(L)`, but
    ///   only when the output's native list reads complete (complete screen
    ///   plus complete inventory). During inventory or per-screen gaps the
    ///   mapping may be stale, so even a mapped id yields `None`: callers
    ///   keep retained homing; write authority for that scope is fenced at
    ///   publish, it is not held by returning `None` here.
    /// - `None` or unmapped native yields `None`: unknown membership never
    ///   collapses into the current view and never proves arrival. Callers
    ///   keep retained homing; write authority for that scope is fenced at
    ///   publish, it is not held by returning `None` here.
    ///
    /// Capability-named note: no fallback dual receipt route for host-owned.
    pub(crate) fn effective_logical_receipt(
        &self,
        report: &WorldReport,
        fact: &WindowFact,
    ) -> Option<String> {
        if !report.host_owned_workspace_domains {
            return fact.observed_workspace.clone();
        }
        if !self.native_list_complete(report, &fact.output.0) {
            return None;
        }
        let native = fact.native_workspace.as_deref()?;
        self.native_logical(&fact.output, native)
    }

    /// Whether the fresh shared projection is order-ambiguous: the report
    /// carries a complete snapshot, no binding exists yet, and the
    /// first-appearance union across sorted outputs would reverse some
    /// output's reported native order. Identical lists and disjoint lists
    /// are consistent. While ambiguous the core adopts nothing (no merge
    /// policy selected) and holds the affected rows through the existing
    /// fences instead.
    pub(crate) fn native_order_conflict(&self, report: &WorldReport) -> bool {
        report.host_owned_workspace_domains
            && report.inventory_complete
            && report.workspace_mode == crate::workspace_scope::WorkspaceMode::Shared
            && self.native_map.is_empty()
            && shared_order_conflict(report)
    }

    /// Native-derived view echo for one output: the mapped logical for
    /// `current_native` when present and known, else `None` (a `None`
    /// current surface is not an ordered desktop: retains the logical view
    /// while still using the order; unknown holds).
    /// Capability-named note: `None` current never adopts, never moves.
    pub(crate) fn native_echo_for(
        &self,
        report: &WorldReport,
        output: &OutputId,
    ) -> Option<String> {
        if !report.host_owned_workspace_domains {
            return None;
        }
        if !self.native_list_complete(report, &output.0) {
            return None;
        }
        let native = report.native.iter().find(|entry| entry.output == *output)?;
        let current = native.current_native.as_deref()?;
        let mapped = self.native_logical(output, current)?;
        if self.known_workspace(output, &mapped) {
            Some(mapped)
        } else {
            None
        }
    }

    /// View echo actually proving for one output.
    ///
    /// Host-owned domains are native-only: the explicit logical echo is
    /// never consulted and never conflicts. The staged echo derives
    /// solely from the native current map; an unknown current proves
    /// nothing (retained views hold, flights hold).
    ///
    /// Without the capability no native echo exists here: only the
    /// explicit logical echo proves, when it names a known workspace.
    pub(crate) fn view_echo_for(&self, report: &WorldReport, output: &OutputId) -> Option<String> {
        if report.host_owned_workspace_domains {
            return self.native_echo_for(report, output);
        }
        let explicit = report
            .views
            .iter()
            .find(|view| view.output == *output)
            .map(|view| view.view.as_str())
            .filter(|view| !view.is_empty())
            .map(str::to_owned);
        match explicit {
            Some(explicit) if self.known_workspace(output, &explicit) => Some(explicit),
            _ => None,
        }
    }

    /// Whether a complete native list is available for one output: the
    /// capability is on, the report inventory is complete, and the screen
    /// is complete. Partial reports never create mappings.
    pub(crate) fn native_list_complete(&self, report: &WorldReport, output: &str) -> bool {
        let _ = self;
        report.host_owned_workspace_domains
            && report.inventory_complete
            && report
                .screens
                .iter()
                .find(|screen| screen.output.0 == output)
                .is_some_and(|screen| screen.complete)
    }

    /// Adopt native bindings before ring/view settlement.
    ///
    /// Capability-named notes: late desktops belong to pending creations,
    /// not outside adoption; one execution authorization per op; logical
    /// order follows observed native order (no insertion-ordinal
    /// assumption); a complete snapshot prunes vanished ids (removal, not
    /// rename; partial/absent rows never remove). Native current views are
    /// staged into echoes before desires mint, so a pruned-away stale view
    /// never recreates.
    pub(crate) fn converge_native_pre(&mut self, report: &WorldReport) {
        use crate::workspace_scope::WorkspaceMode;
        if !report.host_owned_workspace_domains {
            return;
        }
        let shared = report.workspace_mode == WorkspaceMode::Shared;
        self.prune_retained_creations(report);
        if shared {
            self.converge_native_shared(report);
        } else {
            let live: Vec<String> = report
                .screens
                .iter()
                .map(|screen| screen.output.0.clone())
                .collect();
            for output in &live {
                self.prune_vanished_output(report, output);
                self.settle_native_creations(report, output);
            }
            for output in &live {
                self.reorder_ring_to_native(report, output);
            }
            for output in &live {
                self.adopt_native_output(report, output);
            }
        }
        self.retry_pending_source_releases(report);
        let outputs: Vec<OutputId> = report
            .screens
            .iter()
            .map(|screen| screen.output.clone())
            .collect();
        for output in &outputs {
            // Native-only view receipts (this function returns early
            // without the capability): the explicit logical echo is
            // never evidence and never conflicts. The staged echo
            // derives solely from the native current; an unknown
            // current stages nothing so the retained view holds.
            match self.native_echo_for(report, output) {
                Some(native) => {
                    self.staged.echoes.insert(output.0.clone(), native);
                }
                None => {
                    self.staged.echoes.remove(&output.0);
                }
            }
        }
        if !shared {
            let live: Vec<String> = report
                .screens
                .iter()
                .map(|screen| screen.output.0.clone())
                .collect();
            for output in &live {
                self.ensure_native_creations(report, output);
            }
        }
    }

    /// Drop retained creations whose logical workspace left every ring or
    /// whose output left the live set (lifecycle/disconnect cleanup).
    fn prune_retained_creations(&mut self, report: &WorldReport) {
        let live: BTreeSet<String> = report
            .screens
            .iter()
            .map(|screen| screen.output.0.clone())
            .collect();
        self.retained_creations.retain(|retained| {
            live.contains(&retained.target_output.0)
                && self
                    .mapping
                    .get(&retained.target_output.0)
                    .is_some_and(|ring| ring.iter().any(|id| id == &retained.logical_workspace))
        });
    }

    /// Prune vanished native ids for one output on a complete snapshot:
    /// the row is present, the screen is complete, and the inventory is
    /// complete. Vanished pairs delete; a logical left unbound everywhere
    /// is lifecycle removal, never a rename. Partial or absent rows never
    /// prune.
    fn prune_vanished_output(&mut self, report: &WorldReport, output: &str) {
        if !self.native_list_complete(report, output) {
            return;
        }
        let Some(order) = report
            .native
            .iter()
            .find(|entry| entry.output.0 == output)
            .map(|entry| entry.native_order.clone())
        else {
            return;
        };
        let live: BTreeSet<String> = order.into_iter().collect();
        self.native_map
            .retain(|(out, native), _| out != output || live.contains(native));
    }

    /// Exact-correlated raw creation attempt for one retained op (op plus
    /// logical plus output; forged rows read as absent).
    fn creation_attempt_for(
        report: &WorldReport,
        operation_id: &str,
        logical: &str,
        output: &OutputId,
    ) -> Option<crate::world_report::NativeCreationAttempt> {
        report
            .creation_attempts
            .iter()
            .find(|attempt| {
                attempt.operation_id == operation_id
                    && attempt.logical_workspace == logical
                    && attempt.target_output == *output
            })
            .cloned()
    }

    /// Unmapped native ids for one output in reported order.
    fn unmapped_natives(&self, report: &WorldReport, output: &str) -> Vec<String> {
        let out = OutputId(output.to_owned());
        report
            .native
            .iter()
            .find(|entry| entry.output.0 == output)
            .map(|entry| entry.native_order.clone())
            .unwrap_or_default()
            .into_iter()
            .filter(|native| self.native_logical(&out, native).is_none())
            .collect()
    }

    /// Whether a logical workspace on `output` needs a native backing
    /// desktop: unbound and effectively needed. Needed means viewed (the
    /// retained core view, e.g. a fresh trailing selection or a migrated
    /// target), targeted by a retained transfer or view flight (e.g. an
    /// occupied spare committed by a send), or effectively occupied: a
    /// retained member homed here that is neither explicitly closed nor
    /// proven homed elsewhere by the staged observation (a vanished
    /// desktop's departed member does not justify a recreation). The
    /// ordinary trailing spare (empty, unviewed, untargeted, unoccupied)
    /// never needs a desire; a vanished desktop's emptied logical is
    /// lifecycle removal, never a recreation.
    fn logical_needs_backing(&self, _report: &WorldReport, output: &str, logical: &str) -> bool {
        if self.native_binding_for(output, logical).is_some() {
            return false;
        }
        if self.effective_view(&OutputId(output.to_owned())).as_deref() == Some(logical) {
            return true;
        }
        if self.retained.iter().any(|retained| {
            retained.target.target_output.0 == output && retained.target.target_workspace == logical
        }) {
            return true;
        }
        for (window, state) in &self.members {
            if state.home.output.0 != output || state.home.workspace.0 != logical {
                continue;
            }
            if !self.staged.facts.contains_key(window) {
                continue;
            }
            if self.staged.closes.contains(window) {
                continue;
            }
            if let Some(homed) = self.staged.homing.get(window)
                && (homed.output.0 != output || homed.workspace.0 != logical)
            {
                continue;
            }
            return true;
        }
        false
    }

    /// Settle retained creations for one output against the fresh readback.
    ///
    /// Returns true when the output is blocked: an outstanding creation
    /// still needs correlation against fresh candidates there, so nothing
    /// binds and nothing is adopted as outside on this output. Rules,
    /// strict in the attempt's favor only when exact:
    /// - Only the first exact-correlated attempt is consumed; a changed
    ///   replay for the same op (different after-image or different
    ///   correlated id) latches ambiguity instead of overwriting
    ///   the consumed evidence.
    /// - Refused/threw, future stamp, pre-mint stamp, a stale mint-time
    ///   baseline on an uncorrelated receipt, or a correlated id that is
    ///   not new (already in `before`) latch ambiguity (no blind recreate,
    ///   never a new op). A correlated receipt's `before` may truthfully
    ///   include outside desktops that arrived after mint (never a
    ///   baseline refusal); only its named desktop settles it.
    /// - A claimed delta must equal the fresh readback; a no-delta
    ///   Accepted attempt stays pending (a later unique readback delta may
    ///   still settle it) instead of latching a permanent order mismatch.
    ///   A correlated receipt whose named desktop is not visible yet stays
    ///   pending the same way, and its after-image is never compared
    ///   against a raced readback.
    /// - A clean host-correlated receipt binds exactly its named desktop
    ///   causally at most once, even with racing outside additions. A
    ///   receipt naming a desktop already bound elsewhere, or two
    ///   outstanding operations naming the same desktop, latches ambiguity
    ///   instead of stealing or double-binding. A correlated pending never
    ///   resolves by unique delta: only its named desktop proves it.
    /// - Without a correlated id, binding needs exactly one addition in
    ///   the fresh readback for a single outstanding creation (accepted
    ///   uncorrelated inference); anything else holds with
    ///   `AmbiguousCreation`. A race of several candidates latches
    ///   permanently until a new generation or intent, never recreates.
    /// - Candidates with no attempt at all hold transiently without
    ///   adopting; absence never binds.
    ///
    /// Capability-named note: core owns binding; one authorization per op.
    fn settle_native_creations(&mut self, report: &WorldReport, output: &str) -> bool {
        if !self.native_list_complete(report, output) {
            return false;
        }
        let out = OutputId(output.to_owned());
        let order: Vec<String> = report
            .native
            .iter()
            .find(|entry| entry.output.0 == output)
            .map(|entry| entry.native_order.clone())
            .unwrap_or_default();
        if order.is_empty() {
            return false;
        }
        let pending_here: Vec<usize> = self
            .retained_creations
            .iter()
            .enumerate()
            .filter(|(_, retained)| retained.target_output.0 == output)
            .map(|(index, _)| index)
            .collect();
        if pending_here.is_empty() {
            return false;
        }
        // Stage only the first exact attempt per op: a changed replay for
        // an already-consumed op latches ambiguity instead of overwriting
        // the consumed evidence.
        for index in &pending_here {
            let retained = &self.retained_creations[*index];
            let Some(attempt) = Self::creation_attempt_for(
                report,
                &retained.operation_id,
                &retained.logical_workspace,
                &retained.target_output,
            ) else {
                continue;
            };
            let slot = &mut self.retained_creations[*index];
            match slot.pending.as_ref() {
                None => slot.pending = Some(attempt),
                Some(staged) if *staged != attempt => slot.ambiguous = true,
                _ => {}
            }
        }
        // Validate staged attempts: refusal, future or pre-mint stamps, a
        // stale mint-time baseline on an uncorrelated receipt, and a
        // correlated id that is not new (already in `before`) all latch
        // ambiguity. A correlated receipt's `before` may truthfully
        // include outside desktops that arrived after mint (never a
        // baseline refusal); only its named desktop settles it. A no-delta
        // uncorrelated Accepted attempt stays pending: the desktop may
        // simply not be visible yet. A correlated receipt skips the
        // after-image comparison (a raced readback never invalidates
        // causal proof); only its named desktop settles it.
        for index in &pending_here {
            let inconsistent = {
                let retained = &self.retained_creations[*index];
                match retained.pending.as_ref() {
                    None => false,
                    Some(attempt) => {
                        !matches!(attempt.setter, crate::world_report::FrameSetter::Accepted)
                            || (attempt.created_native.is_none()
                                && attempt.order_before != retained.order_before)
                            || (report.monotonic_ms != 0
                                && attempt.attempt_ms > report.monotonic_ms)
                            || (retained.minted_ms != 0 && attempt.attempt_ms < retained.minted_ms)
                            || attempt.created_native.as_ref().is_some_and(|created| {
                                attempt.order_before.iter().any(|id| id == created)
                            })
                            || (attempt.created_native.is_none()
                                && attempt.order_after != attempt.order_before
                                && attempt.order_after != order)
                    }
                }
            };
            if inconsistent {
                self.retained_creations[*index].ambiguous = true;
            }
        }
        let unmapped: Vec<String> = order
            .iter()
            .filter(|native| self.native_logical(&out, native).is_none())
            .cloned()
            .collect();
        if unmapped.is_empty() {
            return false;
        }
        // Causal resolution first: a clean host-correlated receipt binds
        // exactly its named desktop at most once, even with racing outside
        // additions. A receipt naming a desktop already bound elsewhere, or
        // two outstanding operations naming the same desktop, latches
        // ambiguity instead of stealing or double-binding. A named desktop
        // that is not visible yet stays pending (never a guess).
        let mut claims: BTreeMap<String, usize> = BTreeMap::new();
        for index in &pending_here {
            let retained = &self.retained_creations[*index];
            if retained.ambiguous {
                continue;
            }
            if let Some(attempt) = retained.pending.as_ref()
                && let Some(created) = attempt.created_native.as_deref()
                && matches!(attempt.setter, crate::world_report::FrameSetter::Accepted)
            {
                *claims.entry(created.to_owned()).or_default() += 1;
            }
        }
        let mut bindable: Vec<(usize, String)> = Vec::new();
        for index in &pending_here {
            let (created, logical) = {
                let retained = &self.retained_creations[*index];
                if retained.ambiguous {
                    continue;
                }
                let Some(attempt) = retained.pending.as_ref() else {
                    continue;
                };
                let Some(created) = attempt.created_native.clone() else {
                    continue;
                };
                if !matches!(attempt.setter, crate::world_report::FrameSetter::Accepted) {
                    continue;
                }
                (created, retained.logical_workspace.clone())
            };
            let reused = self.native_logical(&out, &created).is_some()
                || claims.get(&created).is_some_and(|count| *count > 1);
            if reused {
                self.retained_creations[*index].ambiguous = true;
                continue;
            }
            if unmapped.iter().any(|id| id == &created)
                && self
                    .mapping
                    .get(output)
                    .is_some_and(|ring| ring.iter().any(|id| id == &logical))
            {
                bindable.push((*index, created));
            }
        }
        // Bindable receipts are unique by construction (a desktop claimed
        // twice latched above), so descending removal keeps indices valid.
        bindable.sort_by_key(|left| std::cmp::Reverse(left.0));
        for (index, created) in bindable {
            let logical = self.retained_creations[index].logical_workspace.clone();
            self.bind_native_at_order(output, &created, &logical, &order);
            self.retained_creations.remove(index);
        }
        let pending_here: Vec<usize> = self
            .retained_creations
            .iter()
            .enumerate()
            .filter(|(_, retained)| retained.target_output.0 == output)
            .map(|(index, _)| index)
            .collect();
        if pending_here.is_empty() {
            return false;
        }
        let unmapped: Vec<String> = order
            .iter()
            .filter(|native| self.native_logical(&out, native).is_none())
            .cloned()
            .collect();
        if unmapped.is_empty() {
            return false;
        }
        // Fresh candidates with no attempt at all hold transiently without
        // adopting (a concurrent outside desktop must never be fabricated
        // onto the pending logical). Not latched: a later attempt may still
        // correlate a single candidate.
        if pending_here
            .iter()
            .all(|index| self.retained_creations[*index].pending.is_none())
        {
            return true;
        }
        // Any latched ambiguity holds; a multi-candidate race or several
        // outstanding creations never resolve by guessing.
        if pending_here
            .iter()
            .any(|index| self.retained_creations[*index].ambiguous)
        {
            return true;
        }
        if pending_here.len() == 1 {
            let index = pending_here[0];
            // A correlated pending never resolves by unique delta: only its
            // named desktop proves it, and an unnamed singleton may be an
            // outside race. Hold transiently until the receipt arrives.
            if self.retained_creations[index]
                .pending
                .as_ref()
                .is_some_and(|attempt| attempt.created_native.is_some())
            {
                return true;
            }
            let baseline: BTreeSet<&str> = self.retained_creations[index]
                .order_before
                .iter()
                .map(String::as_str)
                .collect();
            let added: Vec<String> = order
                .iter()
                .filter(|id| !baseline.contains(id.as_str()))
                .cloned()
                .collect();
            if added.len() == 1 && self.native_logical(&out, &added[0]).is_none() {
                let logical = self.retained_creations[index].logical_workspace.clone();
                if self
                    .mapping
                    .get(output)
                    .is_some_and(|ring| ring.iter().any(|id| id == &logical))
                {
                    self.bind_native_at_order(output, &added[0], &logical, &order);
                    self.retained_creations.remove(index);
                    return false;
                }
            }
            // Zero additions with candidates present, or several additions:
            // hold without guessing. A >1 race latches permanently (until
            // a new generation or intent); the single case stays transient.
            if added.len() != 1 {
                for index in &pending_here {
                    self.retained_creations[*index].ambiguous = true;
                }
            }
            return true;
        }
        for index in &pending_here {
            self.retained_creations[*index].ambiguous = true;
        }
        true
    }

    /// Adopt one unmapped native desktop as a fresh empty logical row at
    /// its native-order position (the outside-created rule per desktop,
    /// selected or not): mints a
    /// fresh logical id (never reusing a retired or unbound row, which
    /// would rename a vanished id), binds the pair, and inserts at the
    /// bound-predecessor position keeping the ordinary trailing spare
    /// last. A desktop missing from the reported order lands at the end
    /// by the same count. Returns the fresh logical id, or `None` when
    /// already bound or minting fails. Callers gate on the complete list
    /// and pending creations.
    pub(crate) fn adopt_single_native(
        &mut self,
        report: &WorldReport,
        output: &str,
        native: &str,
    ) -> Option<String> {
        let out = OutputId(output.to_owned());
        if self.native_logical(&out, native).is_some() {
            return None;
        }
        let order: Vec<String> = report
            .native
            .iter()
            .find(|entry| entry.output.0 == output)
            .map(|entry| entry.native_order.clone())
            .unwrap_or_default();
        let predecessors = order
            .iter()
            .take_while(|id| id.as_str() != native)
            .filter(|id| self.native_logical(&out, id).is_some())
            .count();
        let id = self.create_trailing_workspace(&out, report.default_tiled)?;
        self.native_map
            .insert((output.to_owned(), native.to_owned()), id.clone());
        if let Some(ring) = self.mapping.get_mut(output)
            && ring.last().is_some_and(|tail| tail == &id)
        {
            ring.pop();
        }
        let tail_unbound = self
            .mapping
            .get(output)
            .and_then(|ring| ring.last())
            .is_some_and(|tail| !self.is_native_bound(tail));
        let ring = self.mapping.entry(output.to_owned()).or_default();
        let at = predecessors.min(ring.len());
        let at = if tail_unbound && at >= ring.len() && !ring.is_empty() {
            ring.len() - 1
        } else {
            at
        };
        ring.insert(at, id.clone());
        Some(id)
    }

    fn bind_native_at_order(
        &mut self,
        output: &str,
        native: &str,
        logical: &str,
        order: &[String],
    ) {
        self.native_map
            .insert((output.to_owned(), native.to_owned()), logical.to_owned());
        let position = order
            .iter()
            .take_while(|id| id.as_str() != native)
            .filter(|id| {
                let out = OutputId(output.to_owned());
                self.native_logical(&out, id).is_some()
            })
            .count();
        let tail_unbound = self
            .mapping
            .get(output)
            .and_then(|ring| ring.last())
            .is_some_and(|tail| tail != logical && !self.is_native_bound(tail));
        if let Some(ring) = self.mapping.get_mut(output) {
            ring.retain(|id| id != logical);
        }
        let ring = self.mapping.entry(output.to_owned()).or_default();
        let at = position.min(ring.len());
        if tail_unbound && at >= ring.len() && !ring.is_empty() {
            ring.insert(ring.len() - 1, logical.to_owned());
        } else {
            ring.insert(at, logical.to_owned());
        }
    }

    /// Normalize one output's ring so bound rows follow the observed
    /// native order; unbound rows (spare, pending, migrated-awaiting)
    /// keep their existing relative order after the bound prefix. A host
    /// reorder with no new ids reorders logical rows instead of keeping a
    /// stale insertion order. No-op on partial snapshots.
    fn reorder_ring_to_native(&mut self, report: &WorldReport, output: &str) {
        if !self.native_list_complete(report, output) {
            return;
        }
        let out = OutputId(output.to_owned());
        let order: Vec<String> = report
            .native
            .iter()
            .find(|entry| entry.output.0 == output)
            .map(|entry| entry.native_order.clone())
            .unwrap_or_default();
        if order.is_empty() {
            return;
        }
        let Some(ring) = self.mapping.get(output).cloned() else {
            return;
        };
        let mut bound: Vec<String> = Vec::new();
        let mut seen = BTreeSet::new();
        for native in &order {
            if let Some(logical) = self.native_logical(&out, native)
                && seen.insert(logical.clone())
                && ring.iter().any(|id| id == &logical)
            {
                bound.push(logical);
            }
        }
        if bound.is_empty() {
            return;
        }
        let bound_set: BTreeSet<String> = bound.iter().cloned().collect();
        let mut next: Vec<String> = bound;
        for id in &ring {
            if !bound_set.contains(id) {
                next.push(id.clone());
            }
        }
        if next != ring {
            self.mapping.insert(output.to_owned(), next);
        }
    }

    /// Mint creation desires for unbound logicals that need backing (never
    /// the ordinary spare). One stable `op-native-<serial>` per logical;
    /// replay idempotent; the adapter never treats an unbound row without a
    /// desire as permission.
    pub(crate) fn ensure_native_creations(&mut self, report: &WorldReport, output: &str) {
        if !report.host_owned_workspace_domains {
            return;
        }
        if !self.native_list_complete(report, output) {
            return;
        }
        let order: Vec<String> = report
            .native
            .iter()
            .find(|entry| entry.output.0 == output)
            .map(|entry| entry.native_order.clone())
            .unwrap_or_default();
        if order.is_empty() {
            return;
        }
        let ring: Vec<String> = self.mapping.get(output).cloned().unwrap_or_default();
        for logical in ring {
            if !self.logical_needs_backing(report, output, &logical) {
                continue;
            }
            let exists = self.retained_creations.iter().any(|retained| {
                retained.target_output.0 == output && retained.logical_workspace == logical
            });
            if exists {
                continue;
            }
            self.native_creation_seq = self.native_creation_seq.saturating_add(1);
            let operation_id = format!("op-native-{}", self.native_creation_seq);
            self.retained_creations
                .push(crate::world_engine::RetainedNativeCreation {
                    operation_id,
                    logical_workspace: logical,
                    target_output: OutputId(output.to_owned()),
                    order_before: order.clone(),
                    minted_ms: report.monotonic_ms,
                    pending: None,
                    ambiguous: false,
                });
        }
    }

    /// Retry deferred option-1 source releases: arrival-gated release
    /// that deferred (partial list or pending creation) must still
    /// complete even after its flight settled — settling the flight must
    /// never orphan a dormant binding. Entries whose workspace rejoined
    /// the ring go stale and drop; still-gated entries re-queue through
    /// the release routine itself. Runs in native intake so fresh
    /// bindings stage into echoes on the same report.
    fn retry_pending_source_releases(&mut self, report: &WorldReport) {
        for (output, workspace) in std::mem::take(&mut self.pending_source_releases) {
            let out = OutputId(output.clone());
            self.release_migrated_source_backing(report, &out, &workspace);
        }
    }

    /// Drop retained creations that no longer need backing and never
    /// latched ambiguity (e.g. minted against a stale retained view that
    /// later proves elsewhere): they must not recreate from stale state.
    /// Ambiguous rows persist for their visible hold until the mapping
    /// drops them or a new generation starts.
    pub(crate) fn prune_unneeded_creations(&mut self, report: &WorldReport) {
        let mut drop: Vec<usize> = Vec::new();
        for (index, retained) in self.retained_creations.iter().enumerate() {
            if retained.ambiguous {
                continue;
            }
            if !self.logical_needs_backing(
                report,
                &retained.target_output.0,
                &retained.logical_workspace,
            ) {
                drop.push(index);
            }
        }
        for index in drop.into_iter().rev() {
            self.retained_creations.remove(index);
        }
    }

    /// Adopt one output's native list (separate-view modes): initial
    /// adoption builds the ring in native order; later reports bind
    /// outside-created desktops to fresh logical ids at native-order
    /// position, never by renaming another logical (a vanished id within
    /// the generation is removal, never rename). While a pending creation
    /// is unresolved, unmapped desktops are never adopted as outside
    /// creations (late desktops belong to the pending creation); a
    /// multi-candidate collision holds with `AmbiguousCreation`.
    fn adopt_native_output(&mut self, report: &WorldReport, output: &str) {
        let Some(order) = report
            .native
            .iter()
            .find(|entry| entry.output.0 == output)
            .map(|entry| entry.native_order.clone())
        else {
            return;
        };
        if order.is_empty() || !self.native_list_complete(report, output) {
            return;
        }
        let out = OutputId(output.to_owned());
        let ring_empty = self.mapping.get(output).is_some_and(Vec::is_empty);
        if ring_empty {
            let mut ids = Vec::with_capacity(order.len());
            for native in &order {
                if let Some(existing) = self.native_logical(&out, native) {
                    if !ids.contains(&existing) {
                        ids.push(existing);
                    }
                    continue;
                }
                let Some(id) = self.create_trailing_workspace(&out, report.default_tiled) else {
                    break;
                };
                self.native_map
                    .insert((output.to_owned(), native.clone()), id.clone());
                ids.push(id);
            }
            self.mapping.insert(output.to_owned(), ids.clone());
            let current = report
                .native
                .iter()
                .find(|entry| entry.output.0 == output)
                .and_then(|entry| entry.current_native.clone());
            let view = current
                .and_then(|current| self.native_logical(&out, &current))
                .filter(|logical| ids.iter().any(|id| id == logical))
                .or_else(|| ids.first().cloned());
            if let Some(view) = view {
                self.views.insert(output.to_owned(), view);
            }
            return;
        }
        let pending_here = self
            .retained_creations
            .iter()
            .any(|retained| retained.target_output.0 == output);
        let unmapped: Vec<String> = order
            .iter()
            .filter(|native| self.native_logical(&out, native).is_none())
            .cloned()
            .collect();
        if unmapped.is_empty() {
            return;
        }
        if pending_here {
            return;
        }
        // Outside creation always mints a fresh logical id: reusing a
        // retired or unbound logical for a new native desktop would rename
        // a vanished id within the generation, which is forbidden. The
        // retired logical is lifecycle removal; the newcomer is adoption.
        for (index, native) in order.iter().enumerate() {
            if self.native_logical(&out, native).is_some() {
                continue;
            }
            let predecessors = order[..index]
                .iter()
                .filter(|id| self.native_logical(&out, id).is_some())
                .count();
            let Some(id) = self.create_trailing_workspace(&out, report.default_tiled) else {
                break;
            };
            self.native_map
                .insert((output.to_owned(), native.clone()), id.clone());
            if let Some(ring) = self.mapping.get_mut(output)
                && ring.last().is_some_and(|tail| tail == &id)
            {
                ring.pop();
            }
            let tail_unbound = self
                .mapping
                .get(output)
                .and_then(|ring| ring.last())
                .is_some_and(|tail| !self.is_native_bound(tail));
            let ring = self.mapping.entry(output.to_owned()).or_default();
            let at = predecessors.min(ring.len());
            let at = if tail_unbound && at >= ring.len() && !ring.is_empty() {
                ring.len() - 1
            } else {
                at
            };
            ring.insert(at, id);
        }
    }

    /// Adopt native lists in shared (linked-view) mode: one unified ring
    /// over the union of native ids in first-appearance order across
    /// sorted outputs. Same id strings across outputs bind to the same
    /// logical (linked views span displays); differing lists union without
    /// dropping members. Pending creations settle first (late desktops
    /// belong to them); multi-candidate collisions hold.
    fn converge_native_shared(&mut self, report: &WorldReport) {
        let mut ordered: Vec<String> = report
            .screens
            .iter()
            .map(|screen| screen.output.0.clone())
            .collect();
        ordered.sort();
        if self.native_map.is_empty() && shared_order_conflict(report) {
            return;
        }
        let mut union: Vec<String> = Vec::new();
        let mut seen = BTreeSet::new();
        for output in &ordered {
            let Some(order) = report
                .native
                .iter()
                .find(|entry| entry.output.0 == *output)
                .map(|entry| entry.native_order.clone())
            else {
                continue;
            };
            for native in order {
                if seen.insert(native.clone()) {
                    union.push(native);
                }
            }
        }
        if union.is_empty() {
            return;
        }
        if !report.inventory_complete {
            return;
        }
        let all_complete = ordered.iter().all(|output| {
            report
                .screens
                .iter()
                .find(|screen| screen.output.0 == *output)
                .is_some_and(|screen| screen.complete)
        });
        if !all_complete {
            return;
        }
        if self.native_map.is_empty() {
            let homed_live = self
                .members
                .values()
                .any(|state| ordered.contains(&state.home.output.0));
            if !homed_live {
                for output in &ordered {
                    self.mapping.remove(output);
                    self.views.remove(output);
                }
            }
        }
        for output in &ordered {
            self.prune_vanished_output(report, output);
        }
        let inhabited = ordered.iter().any(|output| {
            self.mapping
                .get(output)
                .is_some_and(|ring| !ring.is_empty())
        });
        if !inhabited {
            let first = ordered.first().cloned().unwrap_or_default();
            let out = OutputId(first.clone());
            let mut ids = Vec::with_capacity(union.len());
            for native in &union {
                let Some(id) = self.create_trailing_workspace(&out, report.default_tiled) else {
                    break;
                };
                ids.push((native.clone(), id));
            }
            for output in &ordered {
                let ring: Vec<String> = ids.iter().map(|(_, id)| id.clone()).collect();
                self.mapping.insert(output.clone(), ring);
            }
            for output in &ordered {
                for (native, id) in &ids {
                    let observed = report.native.iter().any(|entry| {
                        entry.output.0 == *output
                            && entry.native_order.iter().any(|id| id == native)
                    });
                    if observed {
                        self.native_map
                            .insert((output.clone(), native.clone()), id.clone());
                    }
                }
            }
            let current = report
                .native
                .iter()
                .find(|entry| entry.output.0 == first)
                .and_then(|entry| entry.current_native.clone());
            let head = ids.first().map(|(_, id)| id.clone());
            let view = current
                .and_then(|current| {
                    self.native_map
                        .iter()
                        .find(|((out, native), _)| out == &first && native == &current)
                        .map(|(_, id)| id.clone())
                })
                .or(head);
            if let Some(view) = view {
                for output in &ordered {
                    self.views.insert(output.clone(), view.clone());
                }
                self.history.prime_shared(&view);
            }
            return;
        }
        let mut blocked: BTreeSet<String> = BTreeSet::new();
        for output in &ordered {
            if self.settle_native_creations(report, output) {
                // Scoped hold: only outputs with an outstanding creation
                // facing fresh candidates suppress adoption. Unrelated
                // outputs keep adopting their own outside desktops.
                blocked.insert(output.clone());
            }
        }
        self.adopt_native_shared_union(report, &ordered, &union, &blocked);
        self.reorder_shared_rings(&ordered, &union);
        self.ensure_native_creations_shared(report, &ordered);
    }

    /// Outside-adoption over the shared union: binds missing pairs and
    /// allocates genuinely new ids once at union-order position on every
    /// ring (spare stays last). Natives unmapped on a blocked output (an
    /// outstanding creation faces fresh candidates there) are skipped
    /// everywhere: hold, no guessing, no partial adoption of a race.
    fn adopt_native_shared_union(
        &mut self,
        report: &WorldReport,
        ordered: &[String],
        union: &[String],
        blocked: &BTreeSet<String>,
    ) {
        let first = ordered.first().cloned().unwrap_or_default();
        let out = OutputId(first.clone());
        for (index, native) in union.iter().enumerate() {
            // A race on any blocked output poisons the whole native: an
            // outstanding creation faces this candidate somewhere, so no
            // output adopts or pairs it until the race resolves.
            let raced = ordered.iter().any(|output| {
                blocked.contains(output)
                    && report.native.iter().any(|entry| {
                        entry.output.0 == *output && entry.native_order.iter().any(|n| n == native)
                    })
                    && self
                        .native_logical(&OutputId(output.clone()), native)
                        .is_none()
            });
            if raced {
                continue;
            }
            let at = union[..index]
                .iter()
                .filter(|predecessor| {
                    self.native_map
                        .iter()
                        .any(|((_, bound), _)| bound == *predecessor)
                })
                .count();
            if let Some(existing) = self
                .native_map
                .iter()
                .find(|((_, bound), _)| bound == native)
                .map(|(_, id)| id.clone())
            {
                for output in ordered {
                    let observed = report.native.iter().any(|entry| {
                        entry.output.0 == *output && entry.native_order.iter().any(|n| n == native)
                    });
                    if observed
                        && !blocked.contains(output)
                        && self
                            .native_logical(&OutputId(output.clone()), native)
                            .is_none()
                    {
                        self.native_map
                            .insert((output.clone(), native.clone()), existing.clone());
                    }
                }
                for output in ordered {
                    let tail_unbound = self
                        .mapping
                        .get(output)
                        .and_then(|ring| ring.last())
                        .is_some_and(|tail| tail != &existing && !self.is_native_bound(tail));
                    let ring = self.mapping.entry(output.clone()).or_default();
                    if !ring.iter().any(|kept| kept == &existing) {
                        let at = at.min(ring.len());
                        if tail_unbound && at >= ring.len() && !ring.is_empty() {
                            let back = ring.len() - 1;
                            ring.insert(back, existing.clone());
                        } else {
                            ring.insert(at, existing.clone());
                        }
                    }
                }
                continue;
            }
            let Some(id) = self.create_trailing_workspace(&out, report.default_tiled) else {
                break;
            };
            if let Some(ring) = self.mapping.get_mut(&first)
                && ring.last().is_some_and(|tail| tail == &id)
            {
                ring.pop();
            }
            for output in ordered {
                let observed = report.native.iter().any(|entry| {
                    entry.output.0 == *output && entry.native_order.iter().any(|n| n == native)
                });
                if observed {
                    self.native_map
                        .insert((output.clone(), native.clone()), id.clone());
                }
                let tail_unbound = self
                    .mapping
                    .get(output)
                    .and_then(|ring| ring.last())
                    .is_some_and(|tail| tail != &id && !self.is_native_bound(tail));
                let ring = self.mapping.entry(output.clone()).or_default();
                if !ring.iter().any(|kept| kept == &id) {
                    let at = at.min(ring.len());
                    if tail_unbound && at >= ring.len() && !ring.is_empty() {
                        let back = ring.len() - 1;
                        ring.insert(back, id.clone());
                    } else {
                        ring.insert(at, id.clone());
                    }
                }
            }
        }
    }

    /// Normalize shared rings so globally bound rows follow the union
    /// order; unbound rows keep their existing relative order after the
    /// bound prefix. Rings stay coherent across outputs while host
    /// reorders propagate instead of sticking at insertion positions.
    fn reorder_shared_rings(&mut self, ordered: &[String], union: &[String]) {
        let mut bound_in_union: Vec<String> = Vec::new();
        let mut seen = BTreeSet::new();
        for native in union {
            let Some(logical) = self
                .native_map
                .iter()
                .find(|((_, bound), _)| bound == native)
                .map(|(_, id)| id.clone())
            else {
                continue;
            };
            if seen.insert(logical.clone()) {
                bound_in_union.push(logical);
            }
        }
        if bound_in_union.is_empty() {
            return;
        }
        for output in ordered {
            let Some(ring) = self.mapping.get(output).cloned() else {
                continue;
            };
            if !bound_in_union
                .iter()
                .all(|id| ring.iter().any(|kept| kept == id))
            {
                continue;
            }
            let bound_set: BTreeSet<&str> = bound_in_union.iter().map(String::as_str).collect();
            let mut next: Vec<String> = bound_in_union.clone();
            for id in &ring {
                if !bound_set.contains(id.as_str()) {
                    next.push(id.clone());
                }
            }
            if next != ring {
                self.mapping.insert(output.clone(), next);
            }
        }
    }

    /// Mint shared creation desires for globally unbound logicals (no
    /// binding on any output) that are effectively needed (viewed anywhere,
    /// targeted by a retained flight, or effectively occupied on any
    /// ordered output and observed live). The desire targets the first
    /// ordered output with an observed native row (independent orders may
    /// not list the sorted-first output at all); binding fans out on proof.
    /// The trailing spare never mints.
    pub(crate) fn ensure_native_creations_shared(
        &mut self,
        report: &WorldReport,
        ordered: &[String],
    ) {
        if !report.host_owned_workspace_domains || ordered.is_empty() {
            return;
        }
        if !report.inventory_complete {
            return;
        }
        let all_complete = ordered.iter().all(|output| {
            report
                .screens
                .iter()
                .find(|screen| screen.output.0 == *output)
                .is_some_and(|screen| screen.complete)
        });
        if !all_complete {
            return;
        }
        let first = ordered
            .iter()
            .find(|output| report.native.iter().any(|entry| entry.output.0 == **output))
            .or_else(|| ordered.first());
        let Some(first) = first else {
            return;
        };
        let order_non_empty = report
            .native
            .iter()
            .find(|entry| entry.output.0 == *first)
            .is_some_and(|entry| !entry.native_order.is_empty());
        if !order_non_empty {
            return;
        }
        let ring: Vec<String> = self.mapping.get(first).cloned().unwrap_or_default();
        for logical in ring {
            if self.is_native_bound(&logical) {
                continue;
            }
            let viewed = self.views.values().any(|view| view == &logical);
            let targeted = self
                .retained
                .iter()
                .any(|retained| retained.target.target_workspace == logical);
            let mut occupied = false;
            for (window, state) in &self.members {
                if state.home.workspace.0 != logical || !ordered.contains(&state.home.output.0) {
                    continue;
                }
                if !self.staged.facts.contains_key(window) {
                    continue;
                }
                if self.staged.closes.contains(window) {
                    continue;
                }
                if let Some(homed) = self.staged.homing.get(window)
                    && (homed.workspace.0 != logical || !ordered.contains(&homed.output.0))
                {
                    continue;
                }
                occupied = true;
                break;
            }
            if !(viewed || targeted || occupied) {
                continue;
            }
            let exists = self
                .retained_creations
                .iter()
                .any(|retained| retained.logical_workspace == logical);
            if exists {
                continue;
            }
            let order: Vec<String> = report
                .native
                .iter()
                .find(|entry| entry.output.0 == *first)
                .map(|entry| entry.native_order.clone())
                .unwrap_or_default();
            self.native_creation_seq = self.native_creation_seq.saturating_add(1);
            let operation_id = format!("op-native-{}", self.native_creation_seq);
            self.retained_creations
                .push(crate::world_engine::RetainedNativeCreation {
                    operation_id,
                    logical_workspace: logical,
                    target_output: OutputId(first.clone()),
                    order_before: order,
                    minted_ms: report.monotonic_ms,
                    pending: None,
                    ambiguous: false,
                });
        }
    }

    /// Outstanding native creation desires for the reply: one stable op per
    /// unconsumed authorization. Once an attempt is consumed (pending) or
    /// ambiguity latches, the authorization is spent: no republication, and
    /// ambiguous rows carry no execution work (the workspace hold alone
    /// signals). Replay idempotent by op id.
    pub(crate) fn native_creation_desires(
        &self,
        report: &WorldReport,
    ) -> Vec<crate::world_report::NativeCreationDesire> {
        if !report.host_owned_workspace_domains {
            return Vec::new();
        }
        if !report.inventory_complete {
            return Vec::new();
        }
        if report.workspace_mode == crate::workspace_scope::WorkspaceMode::Shared
            && !report.screens.iter().all(|screen| screen.complete)
        {
            return Vec::new();
        }
        self.retained_creations
            .iter()
            .filter(|retained| retained.pending.is_none() && !retained.ambiguous)
            .filter(|retained| {
                report
                    .screens
                    .iter()
                    .find(|screen| screen.output == retained.target_output)
                    .is_some_and(|screen| screen.complete)
            })
            .filter(|retained| {
                report
                    .native
                    .iter()
                    .find(|entry| entry.output == retained.target_output)
                    .is_some_and(|entry| !entry.native_order.is_empty())
            })
            .map(|retained| crate::world_report::NativeCreationDesire {
                operation_id: retained.operation_id.clone(),
                logical_workspace: retained.logical_workspace.clone(),
                target_output: retained.target_output.clone(),
            })
            .collect()
    }

    /// Whether one workspace row holds ambiguous creation (producer of
    /// `HoldReason::AmbiguousCreation`): a retained creation for this pair
    /// latched ambiguity (refused/threw/mismatch/race), or an outstanding
    /// unconsumed creation still faces fresh unmapped candidates on its
    /// output (transient correlation hold: not latched, clears when the
    /// late readback binds or the candidates resolve).
    pub(crate) fn creation_hold_for(
        &self,
        report: &WorldReport,
        output: &str,
        logical: &str,
    ) -> bool {
        if self.retained_creations.iter().any(|retained| {
            retained.target_output.0 == output
                && retained.logical_workspace == logical
                && retained.ambiguous
        }) {
            return true;
        }
        if !report.host_owned_workspace_domains {
            return false;
        }
        let pending = self.retained_creations.iter().any(|retained| {
            retained.target_output.0 == output
                && retained.logical_workspace == logical
                && retained.pending.is_none()
                && !retained.ambiguous
        });
        if !pending {
            return false;
        }
        !self.unmapped_natives(report, output).is_empty()
    }
}
