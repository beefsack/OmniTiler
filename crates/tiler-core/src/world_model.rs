//! Logical lifecycle and convergence for the world transition.
//!
//! Split from [crate::world_engine]: same impl WorldEngine ownership,
//! only relocated for source size. Owns ring/view/lifecycle convergence,
//! admission focus, and the Engine sweep over staged domains.
//! No behavior change.

use std::collections::{BTreeMap, BTreeSet};

use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::engine::Engine;
use crate::ids::{CorrelationId, OwnerId};
use crate::seed::EngineWindow;
use crate::session::{DomainKey, OutputDomain};
use crate::world_engine::{RetainedTransfer, StagedDomain, WorldEngine, inset_world_domain};
use crate::world_report::{
    EffectCause, EffectKind, HeldScreenRow, HoldReason, OneShotEffect, ScreenReport, TilingMode,
    TransferKind, TransferTarget,
};

impl WorldEngine {
    /// Live output order for one staged observation: ordinal-interleaved
    /// merge of valid and held rows. Each held row returns to its carried
    /// `screens` ordinal while valid rows fill the remaining slots in
    /// staged order, so a future exact request encoder can reconstruct
    /// the input order. Held outputs keep rings and displacement
    /// bookkeeping so their scopes hold narrowly, but
    /// completeness/primary resolution below only ever selects valid
    /// screens.
    pub(crate) fn live_output_order(
        screens: &[ScreenReport],
        held_screens: &[HeldScreenRow],
    ) -> Vec<String> {
        // Factual carried order: valid rows keep their relative `screens`
        // order while each held row returns to its original `screens`
        // index, so a future exact request encoder can reconstruct the
        // input order. Validation fences ordinals (distinct, in range)
        // before mutation; the merge below stays total on invalid input
        // instead of panicking.
        let total = screens.len().saturating_add(held_screens.len());
        let mut held_by_ordinal: BTreeMap<usize, &str> = BTreeMap::new();
        for held in held_screens {
            if held.ordinal < total {
                held_by_ordinal
                    .entry(held.ordinal)
                    .or_insert(held.output.0.as_str());
            }
        }
        let mut valid = screens.iter();
        let mut live: Vec<String> = Vec::with_capacity(total);
        for index in 0..total {
            if let Some(output) = held_by_ordinal.get(&index) {
                live.push((*output).to_owned());
            } else if let Some(screen) = valid.next() {
                live.push(screen.output.0.clone());
            }
        }
        // Defensive tail: invalid reports (rejected by validation) still
        // resolve every live handle exactly once.
        for screen in valid {
            if !live.iter().any(|output| output == &screen.output.0) {
                live.push(screen.output.0.clone());
            }
        }
        for held in held_screens {
            if !live.iter().any(|output| output == &held.output.0) {
                live.push(held.output.0.clone());
            }
        }
        live
    }

    /// Post-staging logical convergence: trailing lifecycle per screen (creation/removal over homed occupancy), equal-count
    /// parity, history, and stale session cleanup. Runs after staging so
    /// occupancy reads homed facts. Shared mode runs lifecycle once over
    /// the unified ring with union occupancy: a workspace occupied on any
    /// live output survives everywhere (never deleted because one screen's
    /// scoped view is empty).
    pub(crate) fn converge_logical_post(&mut self) {
        use crate::workspace::{WorkspaceFacts, plan_trailing};
        let inventory_complete = self.staged.observation.inventory_complete;
        let live: Vec<String> =
            Self::live_output_order(&self.staged.screens, &self.staged.held_screens);
        // Narrow derived presence (output ids plus flags only, never whole
        // rows): screen completeness and non-empty native orders.
        let complete_map: BTreeMap<String, bool> = self
            .staged
            .screens
            .iter()
            .map(|screen| (screen.output.0.clone(), screen.complete))
            .collect();
        let native_nonempty: BTreeSet<String> = self
            .staged
            .observation
            .native
            .iter()
            .filter(|entry| !entry.native_order.is_empty())
            .map(|entry| entry.output.0.clone())
            .collect();
        // Homed occupancy per logical id: non-sticky facts only. Sticky
        // homing and displaced hosting retain against removal without
        // occupying.
        let mut occupied: BTreeSet<String> = BTreeSet::new();
        let mut sticky_homed: BTreeSet<String> = BTreeSet::new();
        for (window, key) in &self.staged.homing {
            let Some(fact) = self.staged.facts.get(window) else {
                continue;
            };
            let _ = fact;
            let id = format!("{}/{}", key.output.0, key.workspace.0);
            if self.derived_for(window).sticky {
                sticky_homed.insert(id);
            } else {
                occupied.insert(id);
            }
        }
        // Narrowly held rows are live presence with no usable facts: their
        // retained homes occupy, never an empty workspace. Held rows carry
        // no sticky facts, so the retained home keeps plain occupancy (the
        // safe direction: never delete a live workspace).
        for held in &self.staged.held_windows {
            if let Some(key) = self.core_homing(&held.window) {
                occupied.insert(format!("{}/{}", key.output.0, key.workspace.0));
            }
        }
        let mut hosted: BTreeSet<String> = BTreeSet::new();
        for entry in self.displaced.values() {
            for id in &entry.workspaces {
                hosted.insert(format!("{}/{}", entry.dest, id));
            }
        }
        let shared = self.workspace_mode_setting() == crate::workspace_scope::WorkspaceMode::Shared;
        if shared {
            // Union occupancy over logical ids across all live outputs.
            let mut occupied_ids: BTreeSet<String> = BTreeSet::new();
            let mut retained_ids: BTreeSet<String> = BTreeSet::new();
            for (window, key) in &self.staged.homing {
                let Some(fact) = self.staged.facts.get(window) else {
                    continue;
                };
                let _ = fact;
                if self.derived_for(window).sticky {
                    retained_ids.insert(key.workspace.0.clone());
                } else {
                    occupied_ids.insert(key.workspace.0.clone());
                }
            }
            // Narrowly held rows are live presence: their retained homes
            // occupy under union semantics too.
            for held in &self.staged.held_windows {
                if let Some(key) = self.core_homing(&held.window) {
                    occupied_ids.insert(key.workspace.0.clone());
                }
            }
            for entry in self.displaced.values() {
                for id in &entry.workspaces {
                    retained_ids.insert(id.clone());
                }
            }
            let mut ordered = live.clone();
            ordered.sort();
            let first = ordered.first().cloned();
            let ring = first
                .as_ref()
                .and_then(|first| self.mapping.get(first).cloned());
            let complete_all = live
                .iter()
                .all(|output| complete_map.get(output.as_str()).copied().unwrap_or(false));
            if let (Some(first), Some(ring)) = (first, ring)
                && complete_all
                && inventory_complete
            {
                let visible = self
                    .effective_view(&OutputId(first.clone()))
                    .unwrap_or_default();
                let facts: Vec<WorkspaceFacts> = ring
                    .iter()
                    .map(|id| WorkspaceFacts {
                        empty: !occupied_ids.contains(id),
                        visible: visible == *id,
                        occupied: occupied_ids.contains(id),
                        retained: retained_ids.contains(id)
                            || self.retained.iter().any(|r| {
                                r.target.target_workspace == *id
                                    || r.source_workspace == *id
                                    || r.migration_refill.as_deref() == Some(id.as_str())
                            })
                            || self
                                .pending_source_releases
                                .iter()
                                .any(|(_, workspace)| workspace == id)
                            || self
                                .retained_creations
                                .iter()
                                .any(|retained| retained.logical_workspace == *id),
                    })
                    .collect();
                let plan = plan_trailing(&facts);
                let remove: BTreeSet<usize> = plan.remove.into_iter().collect();
                if !remove.is_empty() {
                    // Absence-intent tombstone path: prune the ring row now
                    // and publish an `existence:false` row from the marker
                    // (see `plan_workspaces`). The old native-first
                    // keep-the-ring behavior is removed: no ring row is
                    // retained pending native deletion.
                    let mut kept: Vec<String> = Vec::new();
                    for (index, id) in ring.iter().enumerate() {
                        if remove.contains(&index) {
                            // Locally retire: prune the ring row while the
                            // native desktop survives. Record every live
                            // `(output, native)` pair for this logical so
                            // ordinary re-adoption suppresses until
                            // positive evidence clears it. No binding is
                            // retained outside rings. Unknown native
                            // occupancy fails closed: retain the row, never
                            // retire on unreadable counts.
                            let pairs: Vec<(String, String)> = self
                                .native_map
                                .iter()
                                .filter(|(_, logical)| *logical == id)
                                .map(|((output, native), _)| (output.clone(), native.clone()))
                                .collect();
                            if !pairs.is_empty()
                                && pairs
                                    .iter()
                                    .any(|(_, native)| !self.native_occupancy_known(native))
                            {
                                kept.push(id.clone());
                                continue;
                            }
                            for pair in pairs {
                                self.native_map.remove(&pair);
                                self.retired_natives.insert(pair, id.clone());
                            }
                            for output in &live {
                                let key = DomainKey {
                                    output: OutputId(output.clone()),
                                    workspace: WorkspaceId(id.clone()),
                                };
                                self.engine.remove(&key);
                                self.tiling.remove(&key);
                            }
                        } else {
                            kept.push(id.clone());
                        }
                    }
                    for output in &live {
                        self.mapping.insert(output.clone(), kept.clone());
                    }
                }
                if plan.need_append {
                    let default_tiled = self.default_tiled();
                    let out = OutputId(first.clone());
                    if let Some(id) = self.create_trailing_workspace(&out, default_tiled) {
                        for output in &live {
                            if output == &first {
                                continue;
                            }
                            let slot = self.mapping.entry(output.clone()).or_default();
                            if !slot.iter().any(|kept| kept == &id) {
                                slot.push(id.clone());
                            }
                            self.tiling
                                .entry(DomainKey {
                                    output: OutputId(output.clone()),
                                    workspace: WorkspaceId(id.clone()),
                                })
                                .or_insert(if default_tiled {
                                    TilingMode::Tiled
                                } else {
                                    TilingMode::Floating
                                });
                        }
                    }
                }
            }
        } else {
            let mut plans = BTreeMap::new();
            for output in &live {
                let complete = complete_map.get(output.as_str()).copied().unwrap_or(false);
                if !complete || !inventory_complete {
                    continue;
                }
                let Some(ring) = self.mapping.get(output).cloned() else {
                    continue;
                };
                let facts: Vec<WorkspaceFacts> = ring
                    .iter()
                    .map(|id| {
                        let scoped = format!("{output}/{id}");
                        let visible = self.effective_view(&OutputId(output.clone())).as_deref()
                            == Some(id.as_str());
                        WorkspaceFacts {
                            empty: !occupied.contains(&scoped),
                            visible,
                            occupied: occupied.contains(&scoped),
                            retained: sticky_homed.contains(&scoped)
                                || hosted.contains(&scoped)
                                || self.retained.iter().any(|r| {
                                    (r.target.target_output.0 == *output
                                        && r.target.target_workspace == *id)
                                        || (r.source_output.0 == *output
                                            && r.source_workspace == *id)
                                        || r.migration_refill.as_deref() == Some(id.as_str())
                                })
                                || self
                                    .pending_source_releases
                                    .iter()
                                    .any(|(out, workspace)| out == output && workspace == id)
                                || self.retained_creations.iter().any(|retained| {
                                    retained.target_output.0 == *output
                                        && retained.logical_workspace == *id
                                }),
                        }
                    })
                    .collect();
                let plan = plan_trailing(&facts);
                plans.insert(output.clone(), (ring, plan));
            }
            // Shared identical roster: prune one GLOBAL native set on every
            // output (identity-based, not count-based). Intersect the local
            // ordinary-empty plan with the globally eligible natives so a
            // globally occupied/shown/protected native stays everywhere and
            // all outputs choose the same removal identities with equal
            // lengths. Any shared-active state without one computable
            // global set (divergent split async, partial, or different
            // orders, or otherwise unresolved coverage) holds all pruning
            // instead of falling back to independent per-output identity
            // removal. No window moves to compact.
            if let Some(global) = self.shared_global_prunable_natives() {
                for (output, (ring, plan)) in plans.iter_mut() {
                    let global_here: BTreeSet<usize> = ring
                        .iter()
                        .enumerate()
                        .filter(|(_, id)| {
                            self.native_binding_for(output, id)
                                .is_some_and(|native| global.contains(&native))
                        })
                        .map(|(index, _)| index)
                        .collect();
                    plan.remove.retain(|index| global_here.contains(index));
                }
            } else if self.shared_roster_active() {
                for (_, plan) in plans.values_mut() {
                    plan.remove.clear();
                }
            }
            if self.host_owned_workspace_domains() && self.equal_workspace_count() {
                // Partial native retirement readback must not replace an old
                // logical identity with freshly minted equal-count padding.
                let floor = live
                    .iter()
                    .filter_map(|output| {
                        let ring = self.mapping.get(output)?;
                        Some(plans.get(output).map_or(ring.len(), |(_, plan)| {
                            ring.len() - plan.remove.len() + usize::from(plan.need_append)
                        }))
                    })
                    .max()
                    .unwrap_or(0);
                for (ring, plan) in plans.values_mut() {
                    let budget = (ring.len() + usize::from(plan.need_append)).saturating_sub(floor);
                    if plan.remove.len() > budget {
                        plan.remove.drain(..plan.remove.len() - budget);
                    }
                }
            }
            for (output, (ring, plan)) in plans {
                let remove: BTreeSet<usize> = plan.remove.into_iter().collect();
                if !remove.is_empty() {
                    // Absence-intent tombstone path (see shared branch):
                    // prune now, publish `existence:false` from the marker.
                    let mut kept: Vec<String> = Vec::new();
                    for (index, id) in ring.iter().enumerate() {
                        if remove.contains(&index) {
                            // Locally retire bound rows while the native
                            // survives; unbound rows prune directly. Unknown
                            // native occupancy fails closed: retain the row.
                            if let Some(native) = self.native_binding_for(&output, id) {
                                if !self.native_occupancy_known(&native) {
                                    kept.push(id.clone());
                                    continue;
                                }
                                self.retired_natives
                                    .insert((output.clone(), native.clone()), id.clone());
                                self.native_map.remove(&(output.clone(), native));
                            }
                            let key = DomainKey {
                                output: OutputId(output.clone()),
                                workspace: WorkspaceId(id.clone()),
                            };
                            self.engine.remove(&key);
                            self.tiling.remove(&key);
                        } else {
                            kept.push(id.clone());
                        }
                    }
                    self.mapping.insert(output.clone(), kept);
                }
                if plan.need_append {
                    let out = OutputId(output.clone());
                    self.create_trailing_workspace(&out, self.default_tiled());
                }
            }
        }
        // Equal-count parity: pad shorter rings to the longest (never
        // remove for parity; per-screen lifecycle already cleaned).
        // Host-owned padding needs readable inventories, screens, and
        // native orders: an incomplete list or a missing/empty native row
        // proves no count, so padding waits for a complete re-read.
        let inventories_readable = inventory_complete
            && live
                .iter()
                .all(|output| complete_map.get(output.as_str()).copied().unwrap_or(false));
        let native_orders_known = !self.host_owned_workspace_domains()
            || live
                .iter()
                .all(|output| native_nonempty.contains(output.as_str()));
        if self.equal_workspace_count() && inventories_readable && native_orders_known {
            let max_len = live
                .iter()
                .filter_map(|output| self.mapping.get(output).map(Vec::len))
                .max()
                .unwrap_or(0);
            for output in &live {
                while self
                    .mapping
                    .get(output)
                    .is_some_and(|ring| ring.len() < max_len)
                {
                    let out = OutputId(output.clone());
                    if self
                        .create_trailing_workspace(&out, self.default_tiled())
                        .is_none()
                    {
                        break;
                    }
                }
            }
        }
        // View sanity after lifecycle: invalid views use the known-echo fallback.
        for output in &live {
            let valid = self.views.get(output).is_some_and(|view| {
                self.mapping
                    .get(output)
                    .is_some_and(|ring| ring.contains(view))
            });
            if !valid {
                if let Some(fallback) = self.preferred_view_fallback(output) {
                    self.views.insert(output.clone(), fallback);
                } else {
                    self.views.remove(output);
                }
            }
        }
        // History observes effective views over the fresh rings. Shared
        // mode keeps one history over the unified order; per-output
        // entries still discard with their outputs.
        if self.workspace_mode_setting() == crate::workspace_scope::WorkspaceMode::Shared {
            let mut ordered = live.clone();
            ordered.sort();
            if let Some(first) = ordered.first() {
                let canonical = self.mapping.get(first).cloned().unwrap_or_default();
                for output in &ordered {
                    self.mapping.insert(output.clone(), canonical.clone());
                }
                if let Some(current) = self.effective_view(&OutputId(first.clone())) {
                    for output in &ordered {
                        self.views.insert(output.clone(), current.clone());
                    }
                    self.history.observe_shared(&canonical, &current);
                }
            }
        } else {
            for output in &live {
                if let Some(current) = self.effective_view(&OutputId(output.clone())) {
                    let ring = self.mapping.get(output).cloned().unwrap_or_default();
                    self.history.observe(output, &current, &ring);
                }
            }
        }
        for output in self.history.history_outputs() {
            if !live.iter().any(|id| id == &output) {
                self.history.discard_output(&output);
            }
        }
        // Stale sessions: keys outside every ring retire (displaced cells
        // re-seed on their survivor; removed trailing cells drop).
        let mut ringed: BTreeSet<DomainKey> = BTreeSet::new();
        for (output, ring) in &self.mapping {
            for id in ring {
                ringed.insert(DomainKey {
                    output: OutputId(output.clone()),
                    workspace: WorkspaceId(id.clone()),
                });
            }
        }
        for key in self.engine.domain_keys() {
            if !ringed.contains(&key) {
                self.engine.remove(&key);
            }
        }
    }

    /// Shared physical-return matcher: for each displaced origin, match
    /// its disconnect-time panel identity against the live handles (same
    /// unique physical match plus the same ambiguous-hold when one live
    /// panel is claimed twice). Used by the logical return scan and the
    /// native binding carry so the two never diverge.
    pub(crate) fn matched_physical_returns(
        &self,
        live_refs: &[(&str, &crate::world_report::OutputIdentity)],
        origins: &[String],
    ) -> Vec<(String, String)> {
        let mut matched: Vec<(String, String)> = Vec::new();
        for origin in origins {
            let Some(entry) = self.displaced.get(origin) else {
                continue;
            };
            let siblings: Vec<&crate::world_report::OutputIdentity> = origins
                .iter()
                .filter(|other| *other != origin)
                .filter_map(|other| self.displaced.get(other))
                .map(|sibling| &sibling.origin_identity)
                .collect();
            if let Some(handle) =
                crate::workspace_scope::match_physical_return(entry, live_refs, &siblings)
            {
                matched.push((origin.clone(), handle));
            }
        }
        // One live panel claimed twice is ambiguous: hold every such
        // claim rather than merging two physical identities.
        {
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            for (_, handle) in &matched {
                *counts.entry(handle.clone()).or_default() += 1;
            }
            matched.retain(|(_, handle)| counts.get(handle) == Some(&1));
        }
        matched
    }

    /// Pre-sweep logical convergence: bootstrap, hotplug
    /// displacement/return, tiling seeds, and view sanity. Runs after
    /// [`WorldEngine::stage_facts`] (echoes inform homing) and before
    /// view settlement and staging. Shared mode keeps one unified
    /// ring/view/history across all outputs (per-screen mappings mirror
    /// the shared order); it never displaces on disconnect and refuses
    /// whole-workspace migration.
    pub(crate) fn converge_logical_pre(&mut self) {
        use crate::workspace_scope::{DisplacedEntry, OutputFacts, SurvivorInfo, WorkspaceMode};
        // Fresh-displacement rehoming below is per-report evidence: clear
        // the discriminator so an earlier disconnect never suppresses a
        // later report's genuinely new lifetimes.
        self.displaced_rehomed.clear();
        let shared = self.workspace_mode_setting() == WorkspaceMode::Shared;
        let inventory_complete = self.staged.observation.inventory_complete;
        let staged_focused = self.staged.observation.focused_global.clone();
        let staged_primary = self.staged.observation.primary_output.clone();
        let live: Vec<String> =
            Self::live_output_order(&self.staged.screens, &self.staged.held_screens);
        let designated = staged_primary.as_ref().map(|out| out.0.clone());
        // Narrow derived presence (output ids plus flags only, never whole
        // rows): valid screens, screen completeness, and non-empty native
        // orders.
        let valid_set: BTreeSet<String> = self
            .staged
            .screens
            .iter()
            .map(|screen| screen.output.0.clone())
            .collect();
        let complete_map: BTreeMap<String, bool> = self
            .staged
            .screens
            .iter()
            .map(|screen| (screen.output.0.clone(), screen.complete))
            .collect();
        let native_nonempty: BTreeSet<String> = self
            .staged
            .observation
            .native
            .iter()
            .filter(|entry| !entry.native_order.is_empty())
            .map(|entry| entry.output.0.clone())
            .collect();
        let live_set: BTreeSet<&str> = live.iter().map(String::as_str).collect();
        for screen in &self.staged.screens {
            self.output_facts.insert(
                screen.output.0.clone(),
                OutputFacts {
                    rect: screen.full_rect,
                    primary: designated.as_deref() == Some(screen.output.0.as_str()),
                    identity: screen.identity.clone(),
                },
            );
        }
        for held in &self.staged.held_screens {
            // Held outputs keep disconnect-time-style facts (no geometry)
            // so displacement still places survivors without inventing a
            // proxy frame; the designated flag never marks a held output.
            // Held identity rows are unreadable: record unknown rather
            // than stale evidence.
            self.output_facts.insert(
                held.output.0.clone(),
                OutputFacts {
                    rect: None,
                    primary: false,
                    identity: crate::world_report::OutputIdentity::unknown(),
                },
            );
        }
        // Host designation wins when it names a live valid screen;
        // otherwise the first valid screen in stable report order. Never
        // the active screen, never a held output.
        let primary = designated
            .filter(|output| valid_set.contains(output.as_str()))
            .or_else(|| {
                live.iter()
                    .find(|output| valid_set.contains(output.as_str()))
                    .cloned()
            });
        // Bootstrap: every live screen owns at least one logical workspace
        // plus a view. Fresh ids are stable within the generation. Shared
        // mode bootstraps one logical workspace on the first live output in
        // stable output-key order (quiet, no previous transition) and
        // mirrors it to every other live output.
        //
        // Host-owned adoption takes precedence over this single-workspace
        // fallback: when a complete native list is available for an empty
        // ring, that ring is left empty here and
        // [`WorldEngine::converge_native_pre`] builds one logical per
        // native desktop in native order below.
        let native_bootstrap = self.host_owned_workspace_domains() && inventory_complete;
        if shared {
            let mut ordered = live.clone();
            ordered.sort();
            let inhabited = ordered.iter().any(|output| {
                self.mapping
                    .get(output)
                    .is_some_and(|ring| !ring.is_empty())
            });
            if !inhabited {
                // An incompatible fresh shared projection adopts nothing
                // (no merge policy selected): fall through to the fallback
                // singleton below so the reply still publishes held rows,
                // and [`WorldEngine::converge_native_pre`] skips adoption.
                let native_ready = native_bootstrap
                    && !self.native_order_conflict()
                    && ordered
                        .iter()
                        .all(|output| complete_map.get(output.as_str()).copied().unwrap_or(false))
                    && ordered
                        .iter()
                        .any(|output| native_nonempty.contains(output.as_str()));
                if !native_ready && let Some(first) = ordered.first().cloned() {
                    let id = self
                        .create_trailing_workspace(&OutputId(first.clone()), self.default_tiled());
                    if let Some(id) = id {
                        for output in &ordered {
                            let ring = self.mapping.entry(output.clone()).or_default();
                            if !ring.iter().any(|kept| kept == &id) {
                                ring.push(id.clone());
                            }
                            self.views.insert(output.clone(), id.clone());
                        }
                        self.history.prime_shared(&id);
                    }
                }
            } else {
                // Mirror the first ordered ring to every live output so
                // per-screen mappings stay unified without rewriting
                // history beyond the shared rules.
                if let Some(first) = ordered.first().cloned() {
                    let canonical = self.mapping.get(&first).cloned().unwrap_or_default();
                    for output in &ordered {
                        self.mapping.insert(output.clone(), canonical.clone());
                    }
                    if let Some(view) = self.views.get(&first).cloned() {
                        for output in &ordered {
                            self.views.insert(output.clone(), view.clone());
                        }
                    }
                }
            }
        } else {
            for output in &live {
                self.mapping.entry(output.clone()).or_default();
                let empty = self.mapping.get(output).is_some_and(Vec::is_empty);
                if empty {
                    let native_ready = native_bootstrap
                        && complete_map.get(output.as_str()).copied().unwrap_or(false)
                        && native_nonempty.contains(output.as_str());
                    if native_ready {
                        continue;
                    }
                    let id = self
                        .create_trailing_workspace(&OutputId(output.clone()), self.default_tiled());
                    if let Some(id) = id {
                        self.views.insert(output.clone(), id);
                    }
                }
            }
        }
        // Host-owned native adoption: build each display's ring from its
        // observed native order (initial) and bind outside-created native
        // desktops (later). Absent rows never delete mappings.
        self.converge_native_pre();
        // Returns first: a live origin reclaims its preserved units.
        // Shared mode never displaces: disconnected output history
        // discards while the one shared history persists.
        let mut fresh_views: Vec<(OutputId, String)> = Vec::new();
        let mut fresh_members: Vec<(WindowId, DomainKey, DomainKey)> = Vec::new();
        if !shared {
            // Returns first: a live panel whose physical reconnect identity
            // matches a displaced origin's disconnect-time snapshot reclaims
            // that origin's preserved units onto the matched live handle
            // (which may differ from the origin handle after a rename). No
            // live-name fallback: unknown, duplicate or mismatched
            // identities hold rather than reclaim a same-named panel.
            let live_panels: Vec<(String, crate::world_report::OutputIdentity)> = self
                .staged
                .screens
                .iter()
                .map(|screen| (screen.output.0.clone(), screen.identity.clone()))
                .collect();
            let live_refs: Vec<(&str, &crate::world_report::OutputIdentity)> = live_panels
                .iter()
                .map(|(handle, identity)| (handle.as_str(), identity))
                .collect();
            let origins: Vec<String> = self.displaced.keys().cloned().collect();
            let matched = self.matched_physical_returns(&live_refs, &origins);
            // Pre-rekey member homes: the scan below must read where
            // members lived before this report's return rekeys them onto
            // the live handle, otherwise every home already names the
            // origin and no return flight ever mints.
            let pre_homes: BTreeMap<WindowId, DomainKey> = self
                .members
                .iter()
                .map(|(window, state)| (window.clone(), state.home.clone()))
                .collect();
            for (origin, live_handle) in matched {
                let Some(entry) = self.displaced.remove(&origin) else {
                    continue;
                };
                // Original affinity, established at displacement: only the
                // owned subset returns. Riders owned by other origins stay
                // on the survivor until their own origin returns.
                let owned: Vec<String> = entry.owned.clone();
                for id in &owned {
                    self.take_workspace_from_all(id, &live_handle);
                }
                {
                    let slot = self.mapping.entry(live_handle.clone()).or_default();
                    for id in &owned {
                        if !slot.iter().any(|kept| kept == id) {
                            slot.push(id.clone());
                        }
                    }
                }
                for id in &owned {
                    self.carry_tiling(id, &entry.dest, &live_handle);
                }
                let origin_id = OutputId(live_handle.clone());
                for id in &owned {
                    self.rekey_members(id, &entry.dest, &live_handle);
                    let source = DomainKey {
                        output: OutputId(entry.dest.clone()),
                        workspace: WorkspaceId(id.clone()),
                    };
                    let target = DomainKey {
                        output: origin_id.clone(),
                        workspace: WorkspaceId(id.clone()),
                    };
                    // Best-effort preservation: a failed relocate keeps
                    // homing (rehome covers it) for a later report.
                    let _ = self.relocate_session_for_move(&source, &target);
                }
                for id in &owned {
                    fresh_views.push((origin_id.clone(), id.clone()));
                }
                // Members homed away from the origin (pre-rekey view) ride
                // one policy transfer each back home. Staged admitted
                // facts only: own-process and safety exclusions never
                // ride. Never replace an outstanding non-policy Window or
                // Migration flight: the command path owns it.
                for (window, fact) in &self.staged.facts.clone() {
                    if self.derived_for(window).sticky {
                        continue;
                    }
                    let moved_id = owned.iter().find(|id| {
                        pre_homes.get(window).is_some_and(|key| {
                            key.output.0 == entry.dest && key.workspace.0 == **id
                        })
                    });
                    let (Some(moved_id), Some(source)) = (moved_id, pre_homes.get(window).cloned())
                    else {
                        continue;
                    };
                    let occupied = self.retained.iter().any(|retained| {
                        if retained.policy {
                            false
                        } else {
                            match &retained.target.kind {
                                TransferKind::Window(w) => w == window,
                                TransferKind::Migration => {
                                    retained.members.iter().any(|member| member == window)
                                }
                                TransferKind::View => false,
                            }
                        }
                    });
                    if occupied {
                        continue;
                    }
                    let target = DomainKey {
                        output: origin_id.clone(),
                        workspace: WorkspaceId(moved_id.clone()),
                    };
                    if source == target {
                        continue;
                    }
                    // Observed on the origin screen: the return carries it
                    // without actuation; only truly stray members move.
                    // Unknown output never counts as observed on origin.
                    if fact.output.as_ref() == Some(&origin_id) {
                        continue;
                    }
                    fresh_members.push((window.clone(), target, source));
                }
                // Reconcile the records left behind: reclaimed units leave
                // every remaining hosting and affinity set so a later
                // return never yanks them back. Emptied records drop with
                // their stale view and history, as on displacement.
                for record in self.displaced.values_mut() {
                    record.workspaces.retain(|id| !owned.contains(id));
                    record.owned.retain(|id| !owned.contains(id));
                }
                let emptied: Vec<String> = self
                    .displaced
                    .iter()
                    .filter(|(_, record)| record.workspaces.is_empty())
                    .map(|(other, _)| other.clone())
                    .collect();
                for other in emptied {
                    self.displaced.remove(&other);
                    self.views.remove(&other);
                    self.history.discard_output(&other);
                }
            }
            // Displacement: removed origins place their units on one survivor.
            let removed: Vec<String> = self
                .mapping
                .keys()
                .filter(|output| !live_set.contains(output.as_str()))
                .cloned()
                .collect();
            for origin in removed {
                let mut candidates: Vec<String> = self.mapping.remove(&origin).unwrap_or_default();
                if let Some(record) = self.displaced.get(&origin) {
                    for id in &record.workspaces {
                        if !candidates.iter().any(|kept| kept == id) {
                            candidates.push(id.clone());
                        }
                    }
                }
                // Shared native roster: memberless non-trailing rows drop
                // only as proven redundant empty padding (ws-6). They
                // would otherwise arrive unbound (bindings key the live
                // output), escape both pruners (which require bindings),
                // linger mid-ring, and get backed as duplicate trailing
                // spares once pruning ahead of them settles. The removed
                // ring's trailing row rides along too unless it is itself
                // proven redundant: same-native alias of a live survivor
                // row, known-empty, unprotected. A redundant tail would
                // otherwise push the survivor's own spare mid-ring, retire
                // it, then inherit the live tail and mint the duplicate
                // backing the serial receipts expose. The drop
                // needs the identical-roster gate (divergent, partial,
                // incomplete, or held evidence holds everything) plus
                // per-row proof: retained target/source/refill, retained
                // creation, pending source release, displaced
                // hosting/affinity, creation hold, live views, member
                // homing, and occupied/unknown natives all keep the row;
                // only a bound, survivor-aliased, known-empty ordinary
                // row drops. Dropped rows also leave the displaced record
                // (return never reclaims them) and lose only their own
                // origin native pair, so a same-name return adopts fresh
                // instead of resurrecting the stale logical. Independent
                // rosters keep the legacy adopt-everything path untouched.
                if self.shared_native_roster() && self.shared_disconnect_prune_ready() {
                    let pre: Vec<String> = candidates.clone();
                    candidates.retain(|id| {
                        if self.members.values().any(|state| {
                            state.home.output.0 == origin && state.home.workspace.0 == *id
                        }) {
                            return true;
                        }
                        if self
                            .staged
                            .homing
                            .values()
                            .any(|key| key.output.0 == origin && key.workspace.0 == *id)
                        {
                            return true;
                        }
                        !self.shared_disconnect_orphan_droppable(&origin, id)
                    });
                    let kept: BTreeSet<String> = candidates.iter().cloned().collect();
                    let dropped: Vec<String> =
                        pre.into_iter().filter(|id| !kept.contains(id)).collect();
                    if !dropped.is_empty() {
                        self.native_map.retain(|(out, _), logical| {
                            out != &origin || !dropped.iter().any(|id| id == logical)
                        });
                    }
                }
                candidates.sort();
                candidates.dedup();
                let prev = self
                    .displaced
                    .get(&origin)
                    .map(|record| record.dest.clone());
                if candidates.is_empty() {
                    self.displaced.remove(&origin);
                    self.views.remove(&origin);
                    self.history.discard_output(&origin);
                    continue;
                }
                let dest = prev
                    .clone()
                    .filter(|dest| live_set.contains(dest.as_str()))
                    .or_else(|| {
                        let removed_rect =
                            self.output_facts.get(&origin).and_then(|facts| facts.rect);
                        let survivors: Vec<SurvivorInfo> = self
                            .staged
                            .screens
                            .iter()
                            .map(|screen| SurvivorInfo {
                                key: screen.output.0.clone(),
                                rect: screen.full_rect,
                            })
                            .collect();
                        crate::workspace_scope::choose_displaced_destination(
                            &survivors,
                            primary.as_deref(),
                            removed_rect,
                        )
                    });
                let Some(dest) = dest else {
                    // No survivor: keep the ring untouched; the next report
                    // retries once screens exist again.
                    self.mapping.insert(origin.clone(), candidates);
                    continue;
                };
                let fresh = prev.as_deref() != Some(dest.as_str());
                // Stray scan before the rekey below: every member homed to
                // the displaced units rides one policy transfer each onto
                // the separate displaced layout (new native backing via
                // the existing creation mechanism below, never a hidden
                // divergent home). Members already observed on the
                // destination screen ride too: the stale topology receipt
                // (old native now bound to a survivor unit) must not
                // collapse them onto the survivor's own unit, and the
                // SOURCE-narrowed settlement below holds the flight until
                // native backing plus arrival prove it. Sticky facts never
                // ride. Must run before `rekey_members`, which moves every
                // member home onto the destination and would make the
                // origin-homing check below vacuous.
                let mut stray_members: Vec<(WindowId, DomainKey, DomainKey)> = Vec::new();
                if fresh {
                    // Staged admitted facts only (see above).
                    for window in self.staged.facts.clone().keys() {
                        if self.derived_for(window).sticky {
                            continue;
                        }
                        // Recorded homes, not the remapped view: the
                        // displaced record lands below (and the origin ring
                        // is already removed above), so the remapping helper
                        // cannot resolve origin homes yet.
                        let home_of = |window: &WindowId| {
                            self.members.get(window).map(|state| state.home.clone())
                        };
                        let moved_id = candidates.iter().find(|id| {
                            home_of(window).is_some_and(|key| {
                                key.output.0 == origin && key.workspace.0 == **id
                            })
                        });
                        let (Some(moved_id), Some(source)) = (moved_id, home_of(window)) else {
                            continue;
                        };
                        // Disconnect rehoming, never a fresh admission: this
                        // lifetime's recorded home sat on the removed
                        // origin's units, so `take_admission_focus` spends
                        // it without emitting even when the survivor homing
                        // lands on a previously empty controlled unit.
                        self.displaced_rehomed.insert(window.clone());
                        // Never replace an outstanding non-policy Window or
                        // Migration flight: the command path owns it.
                        let occupied = self.retained.iter().any(|retained| {
                            if retained.policy {
                                false
                            } else {
                                match &retained.target.kind {
                                    TransferKind::Window(w) => w == window,
                                    TransferKind::Migration => {
                                        retained.members.iter().any(|member| member == window)
                                    }
                                    TransferKind::View => false,
                                }
                            }
                        });
                        if occupied {
                            continue;
                        }
                        let target = DomainKey {
                            output: OutputId(dest.clone()),
                            workspace: WorkspaceId(moved_id.clone()),
                        };
                        if source == target {
                            continue;
                        }
                        stray_members.push((window.clone(), target, source));
                    }
                }
                for id in &candidates {
                    self.take_workspace_from_all(id, &dest);
                }
                {
                    let slot = self.mapping.entry(dest.clone()).or_default();
                    for id in &candidates {
                        if !slot.iter().any(|kept| kept == id) {
                            slot.push(id.clone());
                        }
                    }
                }
                let from = prev.unwrap_or(origin.clone());
                for id in &candidates {
                    self.carry_tiling(id, &from, &dest);
                }
                for id in &candidates {
                    self.rekey_members(id, &origin, &dest);
                    let source = DomainKey {
                        output: OutputId(origin.clone()),
                        workspace: WorkspaceId(id.clone()),
                    };
                    let target = DomainKey {
                        output: OutputId(dest.clone()),
                        workspace: WorkspaceId(id.clone()),
                    };
                    let _ = self.relocate_session_for_move(&source, &target);
                }
                // Snapshot the panel identity plus its contention flags.
                // Re-displacement keeps the original snapshot; a fresh
                // record scans retained facts for peers sharing the tuple
                // or connector. Unknown holds any return.
                let (identity, tuple_contested, connector_contested) = self
                    .displaced
                    .get(&origin)
                    .map(|record| {
                        (
                            record.origin_identity.clone(),
                            record.origin_tuple_contested,
                            record.origin_connector_contested,
                        )
                    })
                    .unwrap_or_else(|| {
                        let (tuple, connector) = crate::workspace_scope::snapshot_contention(
                            &self.output_facts,
                            &origin,
                        );
                        (
                            self.output_facts
                                .get(&origin)
                                .map(|facts| facts.identity.clone())
                                .unwrap_or_else(crate::world_report::OutputIdentity::unknown),
                            tuple,
                            connector,
                        )
                    });
                self.displaced.insert(
                    origin.clone(),
                    DisplacedEntry {
                        workspaces: candidates.clone(),
                        // Original affinity, established here rather than
                        // inferred at return: units already owned by an
                        // earlier displaced origin ride as hosted guests
                        // only. First displacer wins, so owned sets stay
                        // disjoint while hosting sets may overlap.
                        owned: candidates
                            .iter()
                            .filter(|id| {
                                !self.displaced.iter().any(|(other, record)| {
                                    other != &origin && record.owned.iter().any(|kept| kept == *id)
                                })
                            })
                            .cloned()
                            .collect(),
                        dest: dest.clone(),
                        origin_identity: identity,
                        origin_tuple_contested: tuple_contested,
                        origin_connector_contested: connector_contested,
                    },
                );
                // Chain refresh: earlier records whose units rode along on
                // this ring now live on the new dest. Without this the
                // return rekeys against a stale source and trims the unit.
                for (other, record) in self.displaced.iter_mut() {
                    if *other != origin
                        && record.workspaces.iter().any(|id| candidates.contains(id))
                    {
                        record.dest = dest.clone();
                    }
                }
                if fresh {
                    let dest_id = OutputId(dest.clone());
                    for id in &candidates {
                        fresh_views.push((dest_id.clone(), id.clone()));
                    }
                    // Strays collected above (pre-rekey): members of the
                    // displaced units observed away from the destination.
                    // Members observed on the destination screen stay homed
                    // via rehome and ride no transfer.
                    fresh_members.extend(stray_members);
                }
                self.views.remove(&origin);
                self.history.discard_output(&origin);
            }
        } else {
            // Shared mode: discard disconnected output history; the one
            // shared history persists.
            let gone: Vec<String> = self
                .history
                .history_outputs()
                .into_iter()
                .filter(|output| !live_set.contains(output.as_str()))
                .collect();
            for output in gone {
                self.history.discard_output(&output);
            }
        }
        // Focused survivor/return view policy, fresh placements only: when
        // the active window homes in a moving workspace whose output does
        // not show it, retain the view (adapter actuates; focus stays
        // native) and bind one focusless frozen selection under the same
        // parent. Operation identities pin the creating revision plus a
        // per-run index: stable across repeated snapshots of the
        // outstanding intent, yet distinct for every cycle.
        let epoch = self.state_revision;
        if !staged_focused.is_empty() {
            let active = WindowId(staged_focused.clone());
            if let Some(homing) = self.logical_homing(&active) {
                for (index, (output, workspace)) in fresh_views.into_iter().enumerate() {
                    if homing.workspace.0 == workspace
                        && self.effective_view(&output).as_deref() != Some(workspace.as_str())
                    {
                        self.retain_policy_view(
                            &output,
                            &workspace,
                            format!("op-dpv-{epoch}-{index}"),
                        );
                    }
                }
            }
        }
        // Policy member transfers: one stable membership-only intent per
        // affected member.
        for (index, (window, target, source)) in fresh_members.into_iter().enumerate() {
            let operation_id = format!("op-hp-{epoch}-{index}");
            self.retained
                .retain(|r| !matches!(&r.target.kind, TransferKind::Window(w) if w == &window));
            self.retained.push(RetainedTransfer {
                target: TransferTarget {
                    operation_id,
                    kind: TransferKind::Window(window),
                    target_output: target.output,
                    target_workspace: target.workspace.0,
                    follow: false,
                    held: false,
                },
                policy: true,
                command_seq: None,
                source_output: source.output,
                source_workspace: source.workspace.0,
                members: Vec::new(),
                migration_refill: None,
                migration_initial_empty: false,
                migration_source_index: None,
                migration_source_view: None,
                migration_target_view: None,
                migration_terminal: None,
                migration_selection_rev: None,
                pending_member: None,
                pending_members: BTreeMap::new(),
                pending_view: None,
            });
        }
        // Tiling seeds for new logical ids; prune keys outside all rings
        // so a reused id reseeds from the current default.
        let mut ringed: BTreeSet<DomainKey> = BTreeSet::new();
        for (output, ring) in &self.mapping {
            for id in ring {
                ringed.insert(DomainKey {
                    output: OutputId(output.clone()),
                    workspace: WorkspaceId(id.clone()),
                });
            }
        }
        self.tiling.retain(|key, _| ringed.contains(key));
        let seed = if self.default_tiled() {
            TilingMode::Tiled
        } else {
            TilingMode::Floating
        };
        for key in ringed {
            self.tiling.entry(key).or_insert(seed);
        }
        // View sanity: invalid views use the known-echo fallback.
        for output in &live {
            let valid = self.views.get(output).is_some_and(|view| {
                self.mapping
                    .get(output)
                    .is_some_and(|ring| ring.contains(view))
            });
            if !valid {
                if let Some(fallback) = self.preferred_view_fallback(output) {
                    self.views.insert(output.clone(), fallback);
                } else {
                    self.views.remove(output);
                }
            }
        }
        // Prune retained output facts to live screens plus displaced
        // origins still needing placement.
        self.output_facts.retain(|output, _| {
            live_set.contains(output.as_str()) || self.displaced.contains_key(output)
        });
    }

    /// Mint one trailing logical workspace on `output`: stable `ws-<n>` id,
    /// ring push, tiling seed from `default_tiled`. `None` on counter
    /// exhaustion (the counter never wraps; a saturated backend
    /// re-bootstraps under a new generation).
    pub(crate) fn create_trailing_workspace(
        &mut self,
        output: &OutputId,
        default_tiled: bool,
    ) -> Option<String> {
        if self.creation_counter == u64::MAX {
            return None;
        }
        let next = self.creation_counter.saturating_add(1);
        self.creation_counter = next;
        let id = format!("ws-{next}");
        self.mapping
            .entry(output.0.clone())
            .or_default()
            .push(id.clone());
        let key = DomainKey {
            output: output.clone(),
            workspace: WorkspaceId(id.clone()),
        };
        self.tiling.entry(key).or_insert(if default_tiled {
            TilingMode::Tiled
        } else {
            TilingMode::Floating
        });
        Some(id)
    }

    /// Source-refill view for one migration: the still-valid unrelated
    /// source current (staged echo, else core view) when it differs from the
    /// migrated id, else the per-output previous live logical id (MRU
    /// setting), else the last remaining ring member. `None` when no other
    /// member lives. Native-free outputs stage no echoes, so the core view
    /// decides there; host-owned outputs read the staged native echo.
    pub(crate) fn logical_source_refill(
        &self,
        source_output: &OutputId,
        migrated: &str,
    ) -> Option<String> {
        let ring = self.mapping.get(&source_output.0)?;
        let candidates: Vec<String> = ring
            .iter()
            .filter(|id| id.as_str() != migrated)
            .cloned()
            .collect();
        if candidates.is_empty() {
            return None;
        }
        let current = self
            .known_echo(source_output)
            .or_else(|| self.views.get(&source_output.0).cloned());
        if let Some(current) = current
            && current != migrated
            && candidates.iter().any(|id| id == &current)
        {
            return Some(current);
        }
        match crate::workspace_scope::MigrationRefill::from(self.settings.migration_refill) {
            crate::workspace_scope::MigrationRefill::MostRecentlyUsed => {
                if let Some(prev) = self.history.previous(&source_output.0)
                    && prev != migrated
                    && candidates.iter().any(|id| id == &prev)
                {
                    return Some(prev);
                }
                candidates.last().cloned()
            }
            crate::workspace_scope::MigrationRefill::LastRemaining => candidates.last().cloned(),
        }
    }

    /// At-most-once fresh-admission focus intent (D4): the genuinely new
    /// lifetime admitted by this report's sweep, tiled or floated alike,
    /// carries one Focus effect even when the incoming native report still
    /// focuses the previous tile. The adapter owns native activation
    /// through the existing one-shot machinery (`issued_revision`
    /// watermark, spent-before-dispatch, lost/stale drops); no persistent
    /// focus desired exists.
    ///
    /// At most one candidate per report: the last newcomer in staged order
    /// that is not withheld (new birth order; the D2 fallback anchor is
    /// retained state, never a candidate). Automatic fixed-size floats
    /// (retained classifier origin, D8) are skipped and spent during
    /// selection, so a withheld newcomer never displaces an ordinary one
    /// in the same batch; explicit tile wins stay ordinary.
    /// Fresh seeds emit nothing (no retained session means
    /// startup, restart, or disable/enable re-adoption, where the observed
    /// native focus stays authoritative), except the first fresh map into
    /// a previously reported empty readable controlled tiled domain: the
    /// prior accepted report proves the empty roster there, so the newly
    /// arrived lifetime focuses once. A workspace with no prior roster, a
    /// recovered unreadable scope, or a prior held/untouched scope never
    /// infers birth by absence. Re-observation, shrink evictions
    /// (D7 victims are retained), closes, gap/bounds edits, settings Apply,
    /// explicit tile retries, held scopes, and floating scopes emit
    /// nothing: only a retained domain gaining an unknown lifetime.
    /// A mid-gesture newcomer spends without emitting (never replayed),
    /// and an unexplained fit-excluded newcomer (game/overlay with no
    /// floating, fullscreen, or maximized origin) never takes automatic
    /// activation; normalized ordinary floats stay eligible.
    /// D01 native activation ownership: a newcomer admitted behind a
    /// fullscreen/maximized sibling never takes automatic activation
    /// either (the host owns activation away from a protected overlay);
    /// the suppressed intent spends without emitting, never retried.
    /// Windows under an outstanding transfer never count (cross-domain
    /// arrivals settle through the transfer, never a focus steal). Spent
    /// intents prune on close, so a reused id focuses again as a fresh
    /// newcomer. The typed [`EffectCause::AutomaticAdmission`] marks
    /// admission policy while the `af-<token>` id spelling stays opaque
    /// (never prefix-based authority); the token is the first
    /// automatic-focus candidate's issuing revision interned per live
    /// lifetime (see [`crate::world_focus`]), never the window spelling,
    /// so maximum-length subjects stay within the opaque bound. Diagnostic
    /// float origin is never consulted.
    pub(crate) fn take_admission_focus(
        &mut self,
        known_before: &BTreeMap<DomainKey, BTreeSet<String>>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Vec<OneShotEffect> {
        let mut known_now: BTreeSet<String> = BTreeSet::new();
        let mut current: BTreeMap<DomainKey, BTreeSet<String>> = BTreeMap::new();
        for key in self.engine.domain_keys() {
            let Some(session) = self.engine.session(&key) else {
                continue;
            };
            let mut known = BTreeSet::new();
            for link in session.snapshot().windows {
                known.insert(link.window.0.clone());
                known_now.insert(link.window.0.clone());
            }
            for observed in session.exception_observed() {
                known.insert(observed.window.0.clone());
                known_now.insert(observed.window.0.clone());
            }
            for (window, _, _) in session.minimized_entries() {
                known_now.insert(window.0.clone());
            }
            current.insert(key, known);
        }
        // Same-lifetime safety exclusions keep the spent mark: the
        // exclusion retires the slot without ever closing the lifetime,
        // so the lift restores without a fresh admission intent. An
        // explicit close is never staged excluded and still prunes.
        let excluded_here: BTreeSet<String> =
            self.staged.excluded.iter().map(|id| id.0.clone()).collect();
        self.admission_focus_spent
            .retain(|id| known_now.contains(id) || excluded_here.contains(id));
        // Admission policy shares the fresh-placement bootstrap phase:
        // the baseline report itself stays quiet (startup adoption), and
        // incomplete inventories never mint (no speculative ingress
        // across a gap). Only a complete report after the baseline mints.
        if !self.placement_bootstrapped || !self.staged.observation.inventory_complete {
            return Vec::new();
        }
        let transferred: BTreeSet<&str> = self
            .retained
            .iter()
            .flat_map(|retained| match &retained.target.kind {
                TransferKind::Window(window) => vec![window.0.as_str()],
                // Outstanding whole-workspace migration members (aligned
                // mode) settle through the transfer, never a focus steal:
                // disjoint migrations retain no members, so this arm is
                // empty there.
                TransferKind::Migration => retained
                    .members
                    .iter()
                    .map(|window| window.0.as_str())
                    .collect(),
                TransferKind::View => Vec::new(),
            })
            .collect();
        let mut newcomer: Option<(WindowId, DomainKey)> = None;
        // Sticky lifetimes follow the current view without members-map
        // homing, so a view switch restages an already-known sticky in a
        // domain that never held it. That move is visibility, never a
        // birth: only a sticky unknown to every retained session counts
        // as a new admission. Ordinary windows keep per-domain diffing.
        let known_anywhere: BTreeSet<&str> = known_before
            .values()
            .flat_map(|known| known.iter().map(String::as_str))
            .collect();
        for domain in &self.staged.domains.clone() {
            if !domain.readable
                || self.floating_scopes.contains(&domain.key)
                || held_scopes.contains(&domain.key)
            {
                continue;
            }
            // A domain with no retained session is a fresh seed (bootstrap,
            // restart, disable/enable re-adoption) with one exception: a
            // previously accepted empty controlled logical workspace proves
            // its empty roster through the prior reply, so the first fresh
            // map there still counts as admission.
            let known = known_before.get(&domain.key);
            if known.is_none() && !self.prior_empty_controlled(&domain.key) {
                continue;
            }
            let empty = BTreeSet::new();
            let before: &BTreeSet<String> = known.unwrap_or(&empty);
            let converged = current.get(&domain.key);
            for window in &domain.windows {
                let id = window.window.0.as_str();
                if before.contains(id) || transferred.contains(id) {
                    continue;
                }
                // Disconnect rehoming is never a fresh admission: members
                // carried off a removed output by this report's
                // displacement spend here without emitting, so a survivor
                // landing on a previously empty controlled unit never
                // replays as a newcomer on a later report either.
                if self.displaced_rehomed.contains(&window.window) {
                    self.admission_focus_spent.insert(window.window.0.clone());
                    continue;
                }
                if window.sticky && known_anywhere.contains(id) {
                    continue;
                }
                // Safety priority: an unexplained fit-excluded newcomer
                // (game/overlay with no floating, fullscreen, or
                // maximized origin) never takes automatic activation,
                // mirroring the send path's no-automatic-permission
                // refusal. A normalized ordinary float (floating with an
                // origin) stays eligible, capacity floats alike; only
                // the unsafe shape is skipped, never the latest
                // eligible newcomer.
                if window.fit_excluded
                    && !window.floating
                    && !window.fullscreen
                    && !window.maximized
                {
                    continue;
                }
                if converged.is_some_and(|known| known.contains(id)) {
                    // Automatic fixed-size floats never take admission
                    // focus (D8): the retained classifier origin decides,
                    // never raw hints, so hint loss still withholds and
                    // born-fullscreen bypasses stay ordinary. An explicit
                    // tile win (D3) is ordinary admission and keeps its
                    // intent. Each withheld newcomer spends here so it
                    // never replays, while selection continues to the last
                    // non-withheld newcomer, preserving ordinary selection.
                    if let Some(session) = self.engine.session(&domain.key)
                        && session.is_automatic_fixed_float(&window.window)
                        && !session.has_fixed_tile_override(&window.window)
                    {
                        self.admission_focus_spent.insert(window.window.0.clone());
                        continue;
                    }
                    newcomer = Some((window.window.clone(), domain.key.clone()));
                }
            }
        }
        let Some((target, target_key)) = newcomer else {
            return Vec::new();
        };
        // D01 native activation ownership: never force activation away
        // from a protected overlay. A fullscreen/maximized sibling in the
        // newcomer's own domain owns host activation, denial, and
        // explicit switching; the automatic intent spends without
        // emitting and is never retried. Explicit user focus/send
        // shortcuts ride commands, never this path.
        if self.staged.domains.iter().any(|domain| {
            domain.key == target_key
                && domain.windows.iter().any(|window| {
                    window.window != target && (window.fullscreen || window.maximized)
                })
        }) {
            self.admission_focus_spent.insert(target.0.clone());
            return Vec::new();
        }
        // Focused float/sticky ownership: never activate a newcomer over
        // a different focused floating/sticky window; the intent spends
        // without emitting and is never retried. An empty or unknown
        // foreground imposes no shape, and focusing the already-focused
        // window itself is unaffected.
        if !self.staged.observation.focused_global.is_empty()
            && self.staged.observation.focused_global != target.0
        {
            let focused = WindowId(self.staged.observation.focused_global.clone());
            let shape = self.derived_for(&focused);
            if shape.floating || shape.sticky {
                self.admission_focus_spent.insert(target.0.clone());
                return Vec::new();
            }
        }
        // Normative automatic target restrictions: never a sticky target,
        // and an ordinary float never carries fullscreen/maximized/sticky.
        // Each withheld birth spends here so it never replays. A maximized
        // tiled newcomer stays an ordinary automatic target: the maximized
        // veto rides the float exception only, never a blanket veto.
        {
            let shape = self.derived_for(&target);
            let raw = self.staged.facts.get(&target);
            let sticky = shape.sticky || raw.is_some_and(|fact| fact.sticky == Some(true));
            let float_overlay = shape.floating
                && (shape.overlay_fullscreen
                    || shape.overlay_maximized
                    || raw.is_some_and(|fact| {
                        fact.fullscreen == Some(true) || fact.maximized == Some(true)
                    }));
            if sticky || float_overlay {
                self.admission_focus_spent.insert(target.0.clone());
                return Vec::new();
            }
        }
        // The gesture barrier spends honestly: a newcomer arriving
        // mid-gesture drops its activation here and never replays it on
        // a later report (safety over liveness; the at-most-once
        // watermark still records it as spent).
        if self.staged.gesture_barrier {
            self.admission_focus_spent.insert(target.0.clone());
            return Vec::new();
        }
        if !self.admission_focus_spent.insert(target.0.clone()) {
            return Vec::new();
        }
        let token = self.automatic_focus_token(&target);
        vec![OneShotEffect {
            id: format!("af-{token}"),
            kind: EffectKind::Focus,
            target,
            issued_revision: self.state_revision + 1,
            cause: EffectCause::AutomaticAdmission,
            command_seq: None,
            await_shown: false,
        }]
    }

    /// Prior proof that `key` was a controlled, empty logical workspace:
    /// the last accepted publication carried the scope controlled with
    /// no window rows homed there. Read from the retained prior-accepted
    /// context, never from the typed replay cache. Anything else (no
    /// prior acceptance, prior scope absent/held/untouched, prior roster
    /// non-empty) is not birth evidence: truly unknown rosters never
    /// infer admission by absence.
    fn prior_empty_controlled(&self, key: &DomainKey) -> bool {
        self.prior_context.empty_controlled.contains(key)
    }

    /// Reconcile one readable staged domain through the retained engine
    /// sweep. Failures hold the scope explicitly; floating callers never
    /// reach here. A gesture (`gesture_barrier`) without an explicit
    /// Session drag defers geometric capacity repair (D7): when the
    /// carried bounds/gaps changed, membership still converges at the
    /// Closed ids converged for one staged domain: explicit closes homed
    /// here plus safety/own-process exclusions the retained session still
    /// holds (links, exceptions, minimized slots). Exclusions never enter
    /// or keep membership, so the sweep drops their slots exactly like a
    /// close; this authorizes no native close, only slot removal.
    pub(crate) fn closed_for_domain(&self, domain: &StagedDomain) -> Vec<WindowId> {
        let mut closed = domain.closed.clone();
        let Some(session) = self.engine.session(&domain.key) else {
            return closed;
        };
        for id in &self.staged.excluded {
            if closed.iter().any(|known| known == id) {
                continue;
            }
            let held_here = session.snapshot().windows.iter().any(|link| {
                &link.window == id
                    && link.output == domain.key.output
                    && link.workspace == domain.key.workspace
            }) || session.exception_observed().iter().any(|observed| {
                &observed.window == id
                    && observed.output == domain.key.output
                    && observed.workspace == domain.key.workspace
            }) || session.minimized_entries().iter().any(
                |(window, output, workspace)| {
                    window == id
                        && output == &domain.key.output
                        && workspace == &domain.key.workspace
                },
            );
            if held_here {
                closed.push(id.clone());
            }
        }
        closed
    }

    /// retained geometry, but gap adoption and victim repair wait for the
    /// first report after the gesture clears. The scope holds as
    /// [`HoldReason::GestureBarrier`] with topology/order retained, so no
    /// float intent persists mid-gesture and no classification runs.
    /// Unchanged shapes converge exactly as without the barrier.
    pub(crate) fn sweep_domain(
        &mut self,
        domain: &StagedDomain,
        owner: &OwnerId,
        correlation: &CorrelationId,
        held_scopes: &mut BTreeSet<DomainKey>,
    ) {
        let key = domain.key.clone();
        let staged_domain = OutputDomain {
            id: key.output.clone(),
            workspace: key.workspace.clone(),
            bounds: domain.bounds,
            gap: domain.gap,
            adjacent: BTreeMap::new(),
        };
        // The Engine retains the outer-inset work area. Inset exhaustion
        // holds geometrically (capacity B), never seeds an empty domain or
        // drops retained topology. This protected hold stands even
        // mid-gesture: the barrier defers repair, never exhaustion.
        let Some(inset) = inset_world_domain(&staged_domain, domain.outer_gap) else {
            self.hold_reasons
                .insert(key.clone(), HoldReason::ConstraintClamp);
            held_scopes.insert(key);
            return;
        };
        if self.defer_gesture_repair(domain, &inset, owner, correlation, held_scopes) {
            return;
        }
        let windows = self.shielded_windows(&domain.key, &domain.windows);
        let accepted = self
            .engine
            .session(&key)
            .map(|s| (s.accepted_revision(), s.accepted_fingerprint()));
        let (revision, fingerprint) = accepted.unwrap_or((0, 0));
        // Gap/bounds changes adopt through the topology-preserving
        // update-gaps path (legacy parity); unchanged shapes reconcile.
        // Unknown sessions reconcile (fresh seed), never update-gaps
        // (which refuses unknown domains so the fresh route seeds them).
        let command = match self.engine.session(&key) {
            Some(session) => {
                let retained_gap = session
                    .domains()
                    .iter()
                    .find(|d| d.key() == key)
                    .map(|d| (d.bounds, d.gap));
                let retained_outer = self.engine.outer_gap(&key);
                match (retained_gap, retained_outer) {
                    (Some((bounds, gap)), Some(outer))
                        if bounds == inset.bounds
                            && gap == inset.gap
                            && outer == domain.outer_gap =>
                    {
                        CoreCommand::Reconcile
                    }
                    (Some(_), Some(_)) => CoreCommand::UpdateGaps,
                    _ => CoreCommand::Reconcile,
                }
            }
            None => CoreCommand::Reconcile,
        };
        let retained_focus = self.engine.session(&key).and_then(|session| {
            let (focus_domain, focus_leaf) = session.focus();
            match (focus_domain, focus_leaf) {
                (Some(focus_domain), Some(focus_leaf)) if focus_domain == key => {
                    session.focused_window_for(&focus_leaf, &key)
                }
                _ => None,
            }
        });
        let event = CoreEvent {
            owner: owner.clone(),
            generation: self.core_generation.clone(),
            correlation: correlation.clone(),
            revision,
            fingerprint,
            domain: Engine::canonical_component_domain(&inset),
            domain_key: key.clone(),
            outer_gap: domain.outer_gap,
            focused_window: domain.focused.clone(),
            windows,
            closed_windows: self.closed_for_domain(domain),
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: Vec::new(),
            command,
        };
        match self.engine.handle(&event) {
            CoreReply::Projection(_) | CoreReply::Tiled(_) | CoreReply::Released => {
                // Safety-exclusion focus hold: the retained focus converged
                // away as a safety/own-process exclusion (never an explicit
                // positive close) with no observed focus. The scope stays
                // fenced until the adapter proves focus, even though an
                // explicit positive close of the focus reflows readable.
                // Convergence already ran, so the slot is retired; only
                // publication holds.
                if domain.focused.0.is_empty()
                    && let Some(window) = retained_focus
                    && self.staged.excluded.contains(&window)
                    && !domain.closed.contains(&window)
                {
                    self.hold_reasons
                        .insert(key.clone(), HoldReason::Unreadable);
                    held_scopes.insert(key);
                }
            }
            refused => {
                // Single-policy classification (capacity B): the staged
                // reason publishes with the held scope and its rows.
                self.hold_reasons
                    .insert(key.clone(), Self::classify_sweep_hold(&refused));
                held_scopes.insert(key);
            }
        }
    }

    /// Gesture repair deferral for one domain (D7): true when the barrier
    /// owned this domain (the caller returns without sweeping further).
    /// Convergence-only at the retained bounds/gaps when those changed:
    /// membership still reconciles through the ordinary [`Engine::handle`]
    /// path at retained geometry (a `Reconcile` there never adopts gaps or
    /// repairs), then the scope holds as [`HoldReason::GestureBarrier`]
    /// with retained topology, order, focus, and float state untouched.
    /// Unchanged shapes return false so the ordinary sweep converges them
    /// fully (the barrier only withholds frame writes at publish). Fresh
    /// seeds (no retained session) also return false. A failed
    /// convergence keeps its classified hold: the barrier postpones
    /// repair, never real failures.
    pub(crate) fn defer_gesture_repair(
        &mut self,
        domain: &StagedDomain,
        inset: &OutputDomain,
        owner: &OwnerId,
        correlation: &CorrelationId,
        held_scopes: &mut BTreeSet<DomainKey>,
    ) -> bool {
        if !self.staged.gesture_barrier {
            return false;
        }
        let key = domain.key.clone();
        let retained = self.engine.session(&key).and_then(|session| {
            let geometry = session
                .domains()
                .iter()
                .find(|d| d.key() == key)
                .map(|d| (d.bounds, d.gap))?;
            let outer = self.engine.outer_gap(&key)?;
            Some((
                geometry.0,
                geometry.1,
                outer,
                session.accepted_revision(),
                session.accepted_fingerprint(),
            ))
        });
        let Some((bounds, gap, outer, revision, fingerprint)) = retained else {
            return false;
        };
        if bounds == inset.bounds && gap == inset.gap && outer == domain.outer_gap {
            return false;
        }
        let windows = self.shielded_windows(&key, &domain.windows);
        let retained_domain = OutputDomain {
            id: key.output.clone(),
            workspace: key.workspace.clone(),
            bounds,
            gap,
            adjacent: BTreeMap::new(),
        };
        let event = CoreEvent {
            owner: owner.clone(),
            generation: self.core_generation.clone(),
            correlation: correlation.clone(),
            revision,
            fingerprint,
            domain: Engine::canonical_component_domain(&retained_domain),
            domain_key: key.clone(),
            outer_gap: outer,
            focused_window: domain.focused.clone(),
            windows,
            closed_windows: self.closed_for_domain(domain),
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: Vec::new(),
            command: CoreCommand::Reconcile,
        };
        match self.engine.handle(&event) {
            CoreReply::Projection(_) | CoreReply::Tiled(_) | CoreReply::Released => {}
            refused => {
                // Single-policy classification (capacity B): the staged
                // reason publishes with the held scope and its rows.
                self.hold_reasons
                    .insert(key.clone(), Self::classify_sweep_hold(&refused));
                held_scopes.insert(key);
                return true;
            }
        }
        self.hold_reasons
            .insert(key.clone(), HoldReason::GestureBarrier);
        held_scopes.insert(key);
        true
    }

    /// Re-sweep domains a command just re-tiled: the pre-command floating
    /// set skipped them, so without this pass a floating-to-tiled flip
    /// would publish no session rows until the next report. Only domains
    /// floating before the command and tiled after sweep here, so command
    /// releases and ordinary failures are never resurrected.
    pub(crate) fn resweep_retiled(
        &mut self,
        owner: &OwnerId,
        correlation: &CorrelationId,
        held_scopes: &mut BTreeSet<DomainKey>,
        was_floating: &BTreeSet<DomainKey>,
    ) {
        let domains = self.staged.domains.clone();
        for domain in &domains {
            if !domain.readable {
                continue;
            }
            if !was_floating.contains(&domain.key)
                || self.floating_scopes.contains(&domain.key)
                || held_scopes.contains(&domain.key)
                || self.engine.contains(&domain.key)
            {
                continue;
            }
            self.sweep_domain(domain, owner, correlation, held_scopes);
        }
    }

    /// Observation presented to convergence for one staged domain:
    /// outstanding transfer windows are stripped from their source and
    /// presented at their target so committed retained topology survives
    /// actuation lag. Whole-workspace migration members shield per member:
    /// delayed members still observed at the source strip there and inject
    /// at the target so the relocated tree/order/shares survive; arrived
    /// members already at the target stay (never duplicated); explicitly
    /// closed and externally moved members never inject (newer input wins,
    /// never fought). Absent members without a close carry no geometry and
    /// never infer closure. Windows with an outstanding explicit
    /// float-placement target are presented as floating so convergence
    /// retains the committed exception (and its exact geometry) instead of
    /// re-admitting them tiled from a stale non-floating observation: a
    /// lost placement reply must not dissolve the target it never
    /// delivered.
    pub(crate) fn shielded_windows(
        &self,
        key: &DomainKey,
        windows: &[EngineWindow],
    ) -> Vec<EngineWindow> {
        let mut out: Vec<EngineWindow> = windows
            .iter()
            .filter(|window| {
                if self.retained.iter().any(|r| match &r.target.kind {
                    TransferKind::Window(w) => {
                        w.0 == window.window.0
                            && r.source_output == key.output
                            && r.source_workspace == key.workspace.0
                    }
                    TransferKind::View | TransferKind::Migration => false,
                }) {
                    return false;
                }
                // Migration source shielding: strip surviving members still
                // listed at the source so delayed/partial observations never
                // rebuild the emptied source cell. Closed/externally-moved
                // members already dropped from `members`, and arrived members
                // are already absent here.
                !self.retained.iter().any(|r| {
                    matches!(&r.target.kind, TransferKind::Migration)
                        && r.source_output == key.output
                        && r.source_workspace == key.workspace.0
                        && r.members.iter().any(|m| m.0 == window.window.0)
                })
            })
            .cloned()
            .collect();
        for entry in &mut out {
            if self
                .retained_floats
                .iter()
                .any(|held| held.window == entry.window)
            {
                entry.floating = true;
            }
        }
        // Raw fact index for injection: source-observed facts rehomed to
        // the swept key. Admitted facts only: held/excluded windows never
        // inject geometry.
        let mut by_id: BTreeMap<&WindowId, EngineWindow> = BTreeMap::new();
        for fact in self.staged.facts.values() {
            let derived = self.staged.derived.get(&fact.window);
            if derived.is_some_and(|cls| cls.hidden || cls.hold || cls.excluded) {
                continue;
            }
            let Some(rect) = fact.rect else {
                continue;
            };
            let Some(fact_output) = fact.output.as_ref() else {
                continue;
            };
            let Some(derived) = derived.copied() else {
                continue;
            };
            by_id.insert(
                &fact.window,
                EngineWindow {
                    window: fact.window.clone(),
                    output: fact_output.clone(),
                    workspace: WorkspaceId(String::new()),
                    rect,
                    floating: derived.floating,
                    fit_excluded: derived.fit_excluded,
                    fullscreen: derived.overlay_fullscreen,
                    maximized: derived.overlay_maximized,
                    sticky: derived.sticky,
                    fixed_auto: derived.fixed_auto,
                    fixed_suppress: derived.fixed_suppress,
                    hints: fact
                        .hints
                        .unwrap_or(crate::size_hints::WindowSizeHints::none()),
                },
            );
        }
        for retained in &self.retained {
            if let TransferKind::Window(window) = &retained.target.kind
                && retained.target.target_output == key.output
                && retained.target.target_workspace == key.workspace.0
                && !out.iter().any(|entry| entry.window == *window)
                && let Some(source) = by_id.get(window)
            {
                let mut injected = (*source).clone();
                injected.output = key.output.clone();
                injected.workspace = key.workspace.clone();
                out.push(injected);
            }
        }
        for retained in &self.retained {
            if !matches!(&retained.target.kind, TransferKind::Migration) {
                continue;
            }
            if retained.target.target_output != key.output
                || retained.target.target_workspace != key.workspace.0
            {
                continue;
            }
            let source_key = DomainKey {
                output: retained.source_output.clone(),
                workspace: crate::directional::WorkspaceId(retained.source_workspace.clone()),
            };
            for member in &retained.members {
                if out.iter().any(|entry| entry.window == *member) {
                    continue;
                }
                let Some(observed) = by_id.get(member) else {
                    continue;
                };
                let observed_output = &observed.output;
                let observed_homing = self.staged_homing(member);
                // Inject only members still observed at the source: homed
                // at the source key (raw output on the source screen, not
                // yet arrived). Members observed elsewhere stay out (newer
                // input wins, never fought).
                if observed_output == &source_key.output
                    && observed_homing.as_ref() == Some(&source_key)
                {
                    let mut injected = (*observed).clone();
                    injected.output = key.output.clone();
                    injected.workspace = key.workspace.clone();
                    out.push(injected);
                }
            }
        }
        // Stage 3b: merge retained learned minima into the normalized hint
        // vector before Engine projection, so proven app-held growth feeds
        // the actual desired layout (not only pair suppression). Declared
        // maxima contradictions keep the declared pair; infeasible minima
        // still project proportionally and flag overconstrained downstream.
        for entry in &mut out {
            let declared = entry.hints;
            entry.hints = self.effective_hints(&entry.window, declared);
        }
        out
    }
}

#[cfg(test)]
mod prior_empty_scope_tests {
    use super::*;
    use crate::world_engine::{KIND_REUSED, WorldOutcome};
    use crate::world_report::{AuthorityScope, ScopeAuthority, WorldCommand};
    use crate::world_v4_support_tests::{accepted, fact, report, report_full, screen};

    #[test]
    fn output_control_requires_exact_prior_workspace_row_for_empty_proof() {
        let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
        let initial = report(1, 1, vec![screen("out-a", 0)], Vec::new(), None, None);
        let reply = accepted(&mut engine, &initial);
        assert!(reply.scopes.iter().any(|scope| {
            scope.scope == AuthorityScope::Output(OutputId("out-a".to_owned()))
                && scope.authority == ScopeAuthority::Controlled
        }));

        let known_empty = reply
            .workspaces
            .iter()
            .find(|workspace| workspace.output == OutputId("out-a".to_owned()))
            .expect("published workspace row");
        assert!(engine.prior_empty_controlled(&DomainKey {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(known_empty.id.clone()),
        }));
        assert!(!engine.prior_empty_controlled(&DomainKey {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId("newly-appearing".to_owned()),
        }));
    }

    #[test]
    fn cleared_typed_cache_keeps_controlled_empty_proof() {
        let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
        let initial = report(1, 1, vec![screen("out-a", 0)], Vec::new(), None, None);
        let reply = accepted(&mut engine, &initial);
        let known_empty = reply
            .workspaces
            .iter()
            .find(|workspace| workspace.output == OutputId("out-a".to_owned()))
            .expect("published workspace row");
        let key = DomainKey {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(known_empty.id.clone()),
        };
        assert!(engine.prior_empty_controlled(&key));
        // Test-only eviction of the typed replay cache: production keeps
        // full typed equality and the cached reply for duplicate checks.
        // The shared prior-context lane must not depend on those lanes.
        engine.last_report_v4 = None;
        engine.last_reply_v4 = None;
        assert!(engine.prior_empty_controlled(&key));
        assert!(!engine.prior_empty_controlled(&DomainKey {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId("newly-appearing".to_owned()),
        }));
    }

    #[test]
    fn rejected_report_preserves_controlled_empty_proof() {
        let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
        let initial = report(1, 1, vec![screen("out-a", 0)], Vec::new(), None, None);
        let reply = accepted(&mut engine, &initial);
        let known_empty = reply
            .workspaces
            .iter()
            .find(|workspace| workspace.output == OutputId("out-a".to_owned()))
            .expect("published workspace row");
        let key = DomainKey {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(known_empty.id.clone()),
        };
        // Reused sequence with different input refuses without adopting a
        // staged candidate, so prior accepted proof survives.
        let reused = report(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0)],
            None,
            None,
        );
        match engine.handle_report(&reused, &[]).0 {
            WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, KIND_REUSED),
            other => panic!("expected reused refusal, got {other:?}"),
        }
        assert!(engine.prior_empty_controlled(&key));
    }

    #[test]
    fn consumed_command_report_refreshes_controlled_empty_proof() {
        let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
        let refused_focus = WorldCommand::Focus {
            window: WindowId("win-zzz".to_owned()),
            direction: "left".to_owned(),
        };
        // Empty observation plus a refusing command: the refusal reply
        // still publishes the controlled-empty workspace and consumes the
        // command sequence.
        let first = report_full(
            1,
            1,
            vec![screen("out-a", 0)],
            Vec::new(),
            Vec::new(),
            "",
            Some(1),
            Some(refused_focus),
        );
        match engine.handle_report(&first, &[]).0 {
            WorldOutcome::Accepted(reply) => match reply.command_result {
                Some(crate::world_report::CommandResult::Rejected { kind, .. }) => {
                    assert_eq!(kind, crate::world_report::CommandRejectKind::FocusUnknown)
                }
                other => panic!("expected focus refusal result, got {other:?}"),
            },
            other => panic!("expected accepted refusal reply, got {other:?}"),
        }
        // A redelivery reconciles observation only. The controlled-empty
        // proof must refresh from the consumed report's own publication.
        let redelivery = report(
            2,
            2,
            vec![screen("out-a", 0)],
            Vec::new(),
            Some(1),
            Some(WorldCommand::Focus {
                window: WindowId("win-zzz".to_owned()),
                direction: "left".to_owned(),
            }),
        );
        match engine.handle_report(&redelivery, &[]).0 {
            WorldOutcome::CommandConsumed(reply) => {
                let known_empty = reply
                    .workspaces
                    .iter()
                    .find(|workspace| workspace.output == OutputId("out-a".to_owned()))
                    .expect("published workspace row");
                assert!(engine.prior_empty_controlled(&DomainKey {
                    output: OutputId("out-a".to_owned()),
                    workspace: WorkspaceId(known_empty.id.clone()),
                }));
            }
            other => panic!("expected consumed command, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod live_output_order_tests {
    use super::*;
    use crate::connection::RawSettingValue;
    use crate::world_report::{
        AuthorityScope, HeldScreenRow, RowDiagnostic, RowDiagnosticKind, RowDiagnosticLane,
    };
    use crate::world_report_v4_checks::{
        report_digest_v4 as report_digest, validate_report_v4_reason as validate_report_reason,
    };
    use crate::world_v4_support_tests::{report, screen};

    fn held(output: &str, ordinal: usize) -> HeldScreenRow {
        HeldScreenRow {
            output: OutputId(output.to_owned()),
            ordinal,
            fields: vec![
                ("out".to_owned(), RawSettingValue::String(output.to_owned())),
                ("scale".to_owned(), RawSettingValue::Number(0.0)),
            ],
            diagnostic: RowDiagnostic {
                scope: AuthorityScope::Output(OutputId(output.to_owned())),
                row: RowDiagnosticLane::Screen,
                id: Some(output.to_owned()),
                kind: RowDiagnosticKind::InvalidFact,
            },
        }
    }

    #[test]
    fn mixed_valid_held_valid_restores_carried_order() {
        let mut staged = report(
            1,
            1,
            vec![screen("out-a", 0), screen("out-c", 800)],
            Vec::new(),
            None,
            None,
        );
        staged.held_screens = vec![held("out-b", 1)];
        assert_eq!(validate_report_reason(&staged), None);
        assert_eq!(
            WorldEngine::live_output_order(&staged.screens, &staged.held_screens),
            vec!["out-a", "out-b", "out-c"]
        );
    }

    #[test]
    fn held_rows_at_either_end_merge_in_place() {
        let mut staged = report(1, 1, vec![screen("out-b", 0)], Vec::new(), None, None);
        staged.held_screens = vec![held("out-a", 0), held("out-c", 2)];
        // Carried order was [held out-a, valid out-b, held out-c]: the
        // valid row fills the single non-held slot in order.
        assert_eq!(validate_report_reason(&staged), None);
        assert_eq!(
            WorldEngine::live_output_order(&staged.screens, &staged.held_screens),
            vec!["out-a", "out-b", "out-c"]
        );
    }

    #[test]
    fn out_of_range_or_duplicate_ordinals_refuse() {
        let mut staged = report(1, 1, vec![screen("out-a", 0)], Vec::new(), None, None);
        staged.held_screens = vec![held("out-b", 2)];
        assert_eq!(
            validate_report_reason(&staged),
            Some("held-ordinal-invalid")
        );
        let mut staged = report(1, 1, Vec::new(), Vec::new(), None, None);
        staged.held_screens = vec![held("out-a", 0), held("out-b", 0)];
        assert_eq!(
            validate_report_reason(&staged),
            Some("held-ordinal-invalid")
        );
    }

    #[test]
    fn ordinal_separates_digest_and_equality() {
        let mut first = report(1, 1, vec![screen("out-a", 0)], Vec::new(), None, None);
        first.held_screens = vec![held("out-b", 1)];
        let mut second = first.clone();
        second.held_screens[0].ordinal = 0;
        assert_ne!(first, second);
        assert_ne!(report_digest(&first), report_digest(&second));
    }
}
