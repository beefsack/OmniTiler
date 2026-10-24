//! Host-owned native workspace adoption (capability-gated).
//!
//! Split from [`crate::world_engine`]: same `impl WorldEngine` ownership,
//! only relocated for source size. Maps `(output, native desktop id)` to
//! stable core logical workspace ids, preserves native order at initial
//! adoption, binds outside-created native desktops, and derives logical
//! receipts from native evidence for the existing homing/settlement path.
//! No behavior change when `host_owned_workspace_domains` is off.

use std::collections::BTreeSet;

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
    /// capability is on and the output has a native row, an adopted
    /// binding, or per-window native evidence in this report. Absent rows
    /// never imply deletion, and outputs with no native evidence at all
    /// fall back to the native-free homing path (preserving the
    /// pre-adoption contract for capability-on reports that carry only
    /// logical receipts); once adoption evidence exists, unknown native
    /// membership keeps retained homing instead of collapsing into the
    /// current view. A fact carrying `native_workspace: Some` while its
    /// output has no native list is itself adoption evidence: it must
    /// never silently adopt the current logical view.
    pub(crate) fn native_authoritative(&self, report: &WorldReport, output: &OutputId) -> bool {
        report.host_owned_workspace_domains
            && (report.native.iter().any(|entry| entry.output == *output)
                || self.native_map.keys().any(|(out, _)| out == &output.0)
                || report
                    .windows
                    .iter()
                    .any(|fact| fact.output == *output && fact.native_workspace.is_some()))
    }

    /// Effective logical receipt for one fact: the native-derived logical
    /// id when the capability is on, else the adapter's logical receipt.
    ///
    /// Host-owned rules (unknown/conflicting observations hold, never
    /// invent or collapse):
    /// - `native_workspace: Some(nid)` mapped to `L`, with no conflicting
    ///   `observed_workspace`, yields `Some(L)`.
    /// - A conflicting `observed_workspace` naming a known workspace on the
    ///   observed screen (or a floating receipt naming an owned workspace)
    ///   holds: `None`, keeping retained homing.
    /// - Unmapped native ids hold (`None`): creation is gated on complete
    ///   screens plus complete inventory in
    ///   [`WorldEngine::converge_native_pre`], so an unmapped id here is a
    ///   partial report, never a new binding.
    /// - `None` native yields `None` while native evidence is
    ///   authoritative for the output: unknown membership never collapses
    ///   into the current view and never proves arrival. Callers keep
    ///   retained homing; write authority for that scope is fenced at
    ///   publish, it is not held by returning `None` here. Outputs with no
    ///   native row and no adopted binding fall back to the adapter
    ///   logical receipt (pre-adoption).
    pub(crate) fn effective_logical_receipt(
        &self,
        report: &WorldReport,
        fact: &WindowFact,
    ) -> Option<String> {
        if !report.host_owned_workspace_domains {
            return fact.observed_workspace.clone();
        }
        if !self.native_authoritative(report, &fact.output) {
            return fact.observed_workspace.clone();
        }
        let native = fact.native_workspace.as_deref()?;
        let mapped = self.native_logical(&fact.output, native)?;
        if let Some(observed) = fact.observed_workspace.as_deref()
            && observed != mapped
        {
            let known = self.known_workspace(&fact.output, observed)
                || (fact.floating && self.owning_output(observed).is_some());
            if known {
                return None;
            }
        }
        Some(mapped)
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
    /// `current_native` when known, else `None` (unknown view holds).
    pub(crate) fn native_echo_for(
        &self,
        report: &WorldReport,
        output: &OutputId,
    ) -> Option<String> {
        if !report.host_owned_workspace_domains {
            return None;
        }
        let native = report.native.iter().find(|entry| entry.output == *output)?;
        if native.current_native.is_empty() {
            return None;
        }
        let mapped = self.native_logical(output, &native.current_native)?;
        if self.known_workspace(output, &mapped) {
            Some(mapped)
        } else {
            None
        }
    }

    /// View echo actually proving for one output: the explicit logical echo
    /// when present, else the native-derived echo under the capability.
    /// A conflicting pair (both present and different, both known) holds
    /// (`None`): the core never invents which truth wins.
    pub(crate) fn view_echo_for(&self, report: &WorldReport, output: &OutputId) -> Option<String> {
        let explicit = report
            .views
            .iter()
            .find(|view| view.output == *output)
            .map(|view| view.view.as_str())
            .filter(|view| !view.is_empty())
            .map(str::to_owned);
        let native = self.native_echo_for(report, output);
        match (explicit, native) {
            (Some(explicit), Some(native)) if explicit != native => None,
            (Some(explicit), _) => {
                if self.known_workspace(output, &explicit) {
                    Some(explicit)
                } else {
                    None
                }
            }
            (None, Some(native)) => Some(native),
            (None, None) => None,
        }
    }

    /// Whether a complete native list is available for one output: the
    /// capability is on, the report inventory is complete, and the screen
    /// is complete. Partial reports never create mappings.
    fn native_list_complete(&self, report: &WorldReport, output: &str) -> bool {
        let _ = self;
        report.host_owned_workspace_domains
            && report.inventory_complete
            && report
                .screens
                .iter()
                .find(|screen| screen.output.0 == output)
                .is_some_and(|screen| screen.complete)
    }

    /// Adopt native bindings before ring/view settlement: initial adoption
    /// builds each display's ring from its observed native order, later
    /// reports bind outside-created native desktops at native-order
    /// position. Never deletes mappings (absent rows never imply deletion).
    /// Runs inside [`WorldEngine::converge_logical_pre`], after the
    /// native-free bootstrap so the fallback stays intact when no complete
    /// native list is available.
    pub(crate) fn converge_native_pre(&mut self, report: &WorldReport) {
        use crate::workspace_scope::WorkspaceMode;
        if !report.host_owned_workspace_domains {
            return;
        }
        let shared = report.workspace_mode == WorkspaceMode::Shared;
        if shared {
            self.converge_native_shared(report);
        } else {
            // Per-output adoption, in live report order.
            let live: Vec<String> = report
                .screens
                .iter()
                .map(|screen| screen.output.0.clone())
                .collect();
            for output in &live {
                self.adopt_native_output(report, output);
            }
        }
        // Translate native current views into staged echoes so homing and
        // the existing settlement path prove against them. Explicit logical
        // echoes win; a conflicting pair holds (neither is staged).
        let outputs: Vec<OutputId> = report
            .screens
            .iter()
            .map(|screen| screen.output.clone())
            .collect();
        for output in &outputs {
            let explicit = report
                .views
                .iter()
                .find(|view| view.output == *output)
                .map(|view| view.view.as_str())
                .filter(|view| !view.is_empty())
                .map(str::to_owned);
            let native = self.native_echo_for(report, output);
            match (explicit, native) {
                (Some(explicit), Some(native)) if explicit != native => {
                    self.staged.echoes.remove(&output.0);
                }
                (None, Some(native)) => {
                    self.staged.echoes.insert(output.0.clone(), native);
                }
                _ => {}
            }
        }
    }

    /// Adopt one output's native list (separate-view modes).
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
            // Initial adoption: one logical per native, in native order,
            // preserving member order for the startup inference below.
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
            // View follows the native current when it names a bound id,
            // else the ring head.
            let current = report
                .native
                .iter()
                .find(|entry| entry.output.0 == output)
                .map(|entry| entry.current_native.clone())
                .unwrap_or_default();
            let view = if current.is_empty() {
                None
            } else {
                self.native_logical(&out, &current)
            }
            .filter(|logical| ids.iter().any(|id| id == logical))
            .or_else(|| ids.first().cloned());
            if let Some(view) = view {
                self.views.insert(output.to_owned(), view);
            }
            return;
        }
        // Outside-created desktops: bind each unmapped native id, inserted
        // at its native-order position (count of already-bound predecessors).
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
            // `create_trailing_workspace` appends; reposition to the
            // native-order slot instead (spare stays last).
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
            // Keep the ordinary trailing spare last: a ring whose tail is
            // unbound (the spare) stays last when the newcomer inserts
            // ahead of it.
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
    /// dropping members.
    fn converge_native_shared(&mut self, report: &WorldReport) {
        let mut ordered: Vec<String> = report
            .screens
            .iter()
            .map(|screen| screen.output.0.clone())
            .collect();
        ordered.sort();
        // Incompatible fresh projection: adopt nothing (no merge policy
        // selected). The fallback singletons stay unbound so the reply
        // still publishes held rows; members stay unhomed.
        if self.native_map.is_empty() && shared_order_conflict(report) {
            return;
        }
        // Union in first-appearance order.
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
        // Only a complete, nonempty native projection can replace unbound
        // pre-adoption rings. Missing/partial native evidence retains them.
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
        let inhabited = ordered.iter().any(|output| {
            self.mapping
                .get(output)
                .is_some_and(|ring| !ring.is_empty())
        });
        if !inhabited {
            // Initial adoption: one logical per union native.
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
                    // Only bind outputs that actually observe this native;
                    // unobserved pairs stay unmapped so publish can tell
                    // the spare from a bound row per output.
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
            // View follows the first output's native current when bound,
            // else the head.
            let current = report
                .native
                .iter()
                .find(|entry| entry.output.0 == first)
                .map(|entry| entry.current_native.clone())
                .unwrap_or_default();
            let head = ids.first().map(|(_, id)| id.clone());
            let view = self
                .native_map
                .iter()
                .find(|((out, native), _)| out == &first && native == &current)
                .map(|(_, id)| id.clone())
                .or(head);
            if let Some(view) = view {
                for output in &ordered {
                    self.views.insert(output.clone(), view.clone());
                }
                self.history.prime_shared(&view);
            }
            return;
        }
        // Outside-created: bind each union native missing a pair. A native
        // id already bound anywhere keeps its unified logical id: outputs
        // that newly observe it only gain the missing `(output, native)`
        // pair, so an outside move onto it homes the shared logical
        // instead of stranding. Genuinely new ids allocate once and insert
        // at union-order position on every ring (spare stays last); rings
        // stay coherent.
        let first = ordered.first().cloned().unwrap_or_default();
        let out = OutputId(first.clone());
        for (index, native) in union.iter().enumerate() {
            // Union-order slot: count of already-bound predecessors.
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
                for output in &ordered {
                    let observed = report.native.iter().any(|entry| {
                        entry.output.0 == *output && entry.native_order.iter().any(|n| n == native)
                    });
                    if observed
                        && self
                            .native_logical(&OutputId(output.clone()), native)
                            .is_none()
                    {
                        self.native_map
                            .insert((output.clone(), native.clone()), existing.clone());
                    }
                }
                for output in &ordered {
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
            // Undo the helper's append to `first`; each output inserts
            // below at the union-order slot (spare stays last).
            if let Some(ring) = self.mapping.get_mut(&first)
                && ring.last().is_some_and(|tail| tail == &id)
            {
                ring.pop();
            }
            for output in &ordered {
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
                    // Keep an unbound trailing tail last.
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
}
