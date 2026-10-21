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
    EffectKind, HoldReason, OneShotEffect, ScopeAuthority, TilingMode, TransferKind,
    TransferTarget, WorldReport,
};

impl WorldEngine {
    /// Post-staging logical convergence: trailing lifecycle per screen (creation/removal over homed occupancy), equal-count
    /// parity, history, and stale session cleanup. Runs after staging so
    /// occupancy reads homed facts. Shared mode runs lifecycle once over
    /// the unified ring with union occupancy: a workspace occupied on any
    /// live output survives everywhere (never deleted because one screen's
    /// scoped view is empty).
    pub(crate) fn converge_logical_post(&mut self, report: &WorldReport) {
        use crate::workspace::{WorkspaceFacts, plan_trailing};
        let live: Vec<String> = report
            .screens
            .iter()
            .map(|screen| screen.output.0.clone())
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
            let id = format!("{}/{}", key.output.0, key.workspace.0);
            if fact.sticky {
                sticky_homed.insert(id);
            } else {
                occupied.insert(id);
            }
        }
        let mut hosted: BTreeSet<String> = BTreeSet::new();
        for entry in self.displaced.values() {
            for id in &entry.workspaces {
                hosted.insert(format!("{}/{}", entry.dest, id));
            }
        }
        let shared = report.workspace_mode == crate::workspace_scope::WorkspaceMode::Shared;
        if shared {
            // Union occupancy over logical ids across all live outputs.
            let mut occupied_ids: BTreeSet<String> = BTreeSet::new();
            let mut retained_ids: BTreeSet<String> = BTreeSet::new();
            for (window, key) in &self.staged.homing {
                let Some(fact) = self.staged.facts.get(window) else {
                    continue;
                };
                if fact.sticky {
                    retained_ids.insert(key.workspace.0.clone());
                } else {
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
            let complete_all = live.iter().all(|output| {
                report
                    .screens
                    .iter()
                    .find(|screen| screen.output.0 == *output)
                    .is_some_and(|screen| screen.complete)
            });
            if let (Some(first), Some(ring)) = (first, ring)
                && complete_all
                && report.inventory_complete
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
                        retained: retained_ids.contains(id),
                    })
                    .collect();
                let plan = plan_trailing(&facts);
                let remove: BTreeSet<usize> = plan.remove.into_iter().collect();
                if !remove.is_empty() {
                    let mut kept: Vec<String> = Vec::new();
                    for (index, id) in ring.iter().enumerate() {
                        if remove.contains(&index) {
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
                    let out = OutputId(first.clone());
                    if let Some(id) = self.create_trailing_workspace(&out, report.default_tiled) {
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
                                .or_insert(if report.default_tiled {
                                    TilingMode::Tiled
                                } else {
                                    TilingMode::Floating
                                });
                        }
                    }
                }
            }
        } else {
            for output in &live {
                let complete = report
                    .screens
                    .iter()
                    .find(|screen| screen.output.0 == *output)
                    .is_some_and(|screen| screen.complete);
                if !complete || !report.inventory_complete {
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
                            retained: sticky_homed.contains(&scoped) || hosted.contains(&scoped),
                        }
                    })
                    .collect();
                let plan = plan_trailing(&facts);
                let remove: BTreeSet<usize> = plan.remove.into_iter().collect();
                if !remove.is_empty() {
                    let mut kept: Vec<String> = Vec::new();
                    for (index, id) in ring.iter().enumerate() {
                        if remove.contains(&index) {
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
                    self.create_trailing_workspace(&out, report.default_tiled);
                }
            }
        }
        // Equal-count parity: pad shorter rings to the longest (never
        // remove for parity; per-screen lifecycle already cleaned).
        if report.equal_workspace_count {
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
                        .create_trailing_workspace(&out, report.default_tiled)
                        .is_none()
                    {
                        break;
                    }
                }
            }
        }
        // View sanity after lifecycle.
        for output in &live {
            let head = self
                .mapping
                .get(output)
                .and_then(|ring| ring.first().cloned());
            let valid = self.views.get(output).is_some_and(|view| {
                self.mapping
                    .get(output)
                    .is_some_and(|ring| ring.contains(view))
            });
            if !valid {
                if let Some(head) = head {
                    self.views.insert(output.clone(), head);
                } else {
                    self.views.remove(output);
                }
            }
        }
        // History observes effective views over the fresh rings. Shared
        // mode keeps one history over the unified order; per-output
        // entries still discard with their outputs.
        if report.workspace_mode == crate::workspace_scope::WorkspaceMode::Shared {
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

    /// Pre-sweep logical convergence: bootstrap, hotplug
    /// displacement/return, tiling seeds, and view sanity. Runs after
    /// [`WorldEngine::stage_facts`] (echoes inform homing) and before
    /// view settlement and staging. Shared mode keeps one unified
    /// ring/view/history across all outputs (per-screen mappings mirror
    /// the shared order); it never displaces on disconnect and refuses
    /// whole-workspace migration.
    pub(crate) fn converge_logical_pre(&mut self, report: &WorldReport) {
        use crate::workspace_scope::{DisplacedEntry, OutputFacts, SurvivorInfo, WorkspaceMode};
        let shared = report.workspace_mode == WorkspaceMode::Shared;
        let live: Vec<String> = report
            .screens
            .iter()
            .map(|screen| screen.output.0.clone())
            .collect();
        let live_set: BTreeSet<&str> = live.iter().map(String::as_str).collect();
        for screen in &report.screens {
            self.output_facts.insert(
                screen.output.0.clone(),
                OutputFacts {
                    rect: screen.full_rect,
                    primary: screen.primary,
                },
            );
        }
        let primary = report
            .screens
            .iter()
            .find(|screen| screen.primary)
            .map(|screen| screen.output.0.clone())
            .or_else(|| live.first().cloned());
        // Bootstrap: every live screen owns at least one logical workspace
        // plus a view. Fresh ids are stable within the generation. Shared
        // mode bootstraps one logical workspace on the first live output in
        // stable output-key order (quiet, no previous transition) and
        // mirrors it to every other live output.
        if shared {
            let mut ordered = live.clone();
            ordered.sort();
            let inhabited = ordered.iter().any(|output| {
                self.mapping
                    .get(output)
                    .is_some_and(|ring| !ring.is_empty())
            });
            if !inhabited {
                if let Some(first) = ordered.first().cloned() {
                    let id = self
                        .create_trailing_workspace(&OutputId(first.clone()), report.default_tiled);
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
                    let id = self
                        .create_trailing_workspace(&OutputId(output.clone()), report.default_tiled);
                    if let Some(id) = id {
                        self.views.insert(output.clone(), id);
                    }
                }
            }
        }
        // Returns first: a live origin reclaims its preserved units.
        // Shared mode never displaces: disconnected output history
        // discards while the one shared history persists.
        let mut fresh_views: Vec<(OutputId, String)> = Vec::new();
        let mut fresh_members: Vec<(WindowId, DomainKey, DomainKey)> = Vec::new();
        if !shared {
            let returned: Vec<String> = self
                .displaced
                .keys()
                .filter(|origin| live_set.contains(origin.as_str()))
                .cloned()
                .collect();
            for origin in returned {
                let Some(entry) = self.displaced.remove(&origin) else {
                    continue;
                };
                let home = entry.workspaces.clone();
                for id in &home {
                    self.take_workspace_from_all(id, &origin);
                }
                {
                    let slot = self.mapping.entry(origin.clone()).or_default();
                    for id in &home {
                        if !slot.iter().any(|kept| kept == id) {
                            slot.push(id.clone());
                        }
                    }
                }
                for id in &home {
                    self.carry_tiling(id, &entry.dest, &origin);
                }
                let origin_id = OutputId(origin.clone());
                for id in &home {
                    self.rekey_members(id, &entry.dest, &origin);
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
                for id in &home {
                    fresh_views.push((origin_id.clone(), id.clone()));
                }
                // Members still homed away from the origin ride one policy
                // transfer each back home.
                for fact in &report.windows {
                    if fact.sticky {
                        continue;
                    }
                    let moved_id = home.iter().find(|id| {
                        self.retained_homing(&fact.window).is_some_and(|key| {
                            key.output.0 == entry.dest && key.workspace.0 == **id
                        })
                    });
                    let (Some(moved_id), Some(source)) =
                        (moved_id, self.retained_homing(&fact.window))
                    else {
                        continue;
                    };
                    let target = DomainKey {
                        output: origin_id.clone(),
                        workspace: WorkspaceId(moved_id.clone()),
                    };
                    if source == target {
                        continue;
                    }
                    // Observed on the origin screen: the return carries it
                    // without actuation; only truly stray members move.
                    if fact.output == origin_id {
                        continue;
                    }
                    fresh_members.push((fact.window.clone(), target, source));
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
                        let survivors: Vec<SurvivorInfo> = report
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
                // Stray scan before the rekey below: members homed to the
                // displaced units but observed away from the destination
                // ride one policy transfer each. Members observed at the
                // destination carry without actuation; sticky facts never
                // ride. Must run before `rekey_members`, which moves every
                // member home onto the destination and would make the
                // origin-homing check below vacuous.
                let mut stray_members: Vec<(WindowId, DomainKey, DomainKey)> = Vec::new();
                if fresh {
                    let dest_id = OutputId(dest.clone());
                    for fact in &report.windows {
                        if fact.sticky {
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
                            home_of(&fact.window).is_some_and(|key| {
                                key.output.0 == origin && key.workspace.0 == **id
                            })
                        });
                        let (Some(moved_id), Some(source)) = (moved_id, home_of(&fact.window))
                        else {
                            continue;
                        };
                        if fact.output == dest_id {
                            continue;
                        }
                        let target = DomainKey {
                            output: dest_id.clone(),
                            workspace: WorkspaceId(moved_id.clone()),
                        };
                        if source == target {
                            continue;
                        }
                        stray_members.push((fact.window.clone(), target, source));
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
                self.displaced.insert(
                    origin.clone(),
                    DisplacedEntry {
                        workspaces: candidates.clone(),
                        dest: dest.clone(),
                    },
                );
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
        // native). Operation identities pin the creating revision plus a
        // per-run index: stable across repeated snapshots of the
        // outstanding intent, yet distinct for every cycle.
        let epoch = self.state_revision;
        if !report.focused_global.is_empty() {
            let active = WindowId(report.focused_global.clone());
            if let Some(homing) = self.logical_homing(report, &active) {
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
                source_output: source.output,
                source_workspace: source.workspace.0,
                members: Vec::new(),
                migration_refill: None,
                migration_initial_empty: false,
                migration_source_index: None,
                migration_source_view: None,
                migration_target_view: None,
                migration_terminal: None,
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
        let seed = if report.default_tiled {
            TilingMode::Tiled
        } else {
            TilingMode::Floating
        };
        for key in ringed {
            self.tiling.entry(key).or_insert(seed);
        }
        // View sanity: every live screen shows a ring member.
        for output in &live {
            let head = self
                .mapping
                .get(output)
                .and_then(|ring| ring.first().cloned());
            let valid = self.views.get(output).is_some_and(|view| {
                self.mapping
                    .get(output)
                    .is_some_and(|ring| ring.contains(view))
            });
            if !valid {
                if let Some(head) = head {
                    self.views.insert(output.clone(), head);
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
    /// source current (echo, else core view) when it differs from the
    /// migrated id, else the per-output previous live logical id (MRU
    /// setting), else the last remaining ring member. `None` when no other
    /// member lives.
    pub(crate) fn logical_source_refill(
        &self,
        report: &WorldReport,
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
        match crate::workspace_scope::MigrationRefill::from(report.migration_source_refill) {
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
    /// At most one candidate per report: the last newcomer in report order
    /// (new birth order; the D2 fallback anchor is retained state, never a
    /// candidate). Fresh seeds emit nothing (no retained session means
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
    /// newcomer. The `af-<window>` id marks admission intent so the adapter
    /// applies its float-tolerant guard only there; diagnostic float
    /// origin is never consulted.
    pub(crate) fn take_admission_focus(
        &mut self,
        report: &WorldReport,
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
        self.admission_focus_spent
            .retain(|id| known_now.contains(id));
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
        // The gesture barrier spends honestly: a newcomer arriving
        // mid-gesture drops its activation here and never replays it on
        // a later report (safety over liveness; the at-most-once
        // watermark still records it as spent).
        if report.gesture_barrier {
            self.admission_focus_spent.insert(target.0.clone());
            return Vec::new();
        }
        if !self.admission_focus_spent.insert(target.0.clone()) {
            return Vec::new();
        }
        vec![OneShotEffect {
            id: format!("af-{}", target.0),
            kind: EffectKind::Focus,
            target,
            issued_revision: self.state_revision + 1,
        }]
    }

    /// Prior-reply proof that `key` was a controlled, tiled, empty
    /// logical workspace: the last accepted reply published the scope
    /// controlled with no window rows homed there. Covers the first fresh
    /// map into a previously empty workspace, which carries no retained
    /// session to diff against. Anything else (no prior reply, prior
    /// scope absent/held/untouched, prior roster non-empty) is not birth
    /// evidence: truly unknown rosters never infer admission by absence.
    /// Restart/incarnation resets clear the retained replies, so fresh
    /// adoption stays quiet; disable/enable adoption replays a prior
    /// untouched scope, never a controlled empty one.
    fn prior_empty_controlled(&self, key: &DomainKey) -> bool {
        let Some(reply) = self.last_reply.as_ref() else {
            return false;
        };
        let scope = format!("{}/{}", key.output.0, key.workspace.0);
        reply
            .scopes
            .iter()
            .any(|status| status.scope == scope && status.authority == ScopeAuthority::Controlled)
            && !reply
                .windows
                .iter()
                .any(|window| window.output == key.output && window.workspace == key.workspace.0)
    }

    /// Reconcile one readable staged domain through the retained engine
    /// sweep. Failures hold the scope explicitly; floating callers never
    /// reach here. A gesture (`gesture_barrier`) without an explicit
    /// Session drag defers geometric capacity repair (D7): when the
    /// carried bounds/gaps changed, membership still converges at the
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
            closed_windows: domain.closed.clone(),
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: Vec::new(),
            command,
        };
        match self.engine.handle(&event) {
            CoreReply::Projection(_)
            | CoreReply::Tiled(_)
            | CoreReply::Released
            | CoreReply::NoGroup { .. } => {}
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
            closed_windows: domain.closed.clone(),
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: Vec::new(),
            command: CoreCommand::Reconcile,
        };
        match self.engine.handle(&event) {
            CoreReply::Projection(_)
            | CoreReply::Tiled(_)
            | CoreReply::Released
            | CoreReply::NoGroup { .. } => {}
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
        report: &WorldReport,
        owner: &OwnerId,
        correlation: &CorrelationId,
        held_scopes: &mut BTreeSet<DomainKey>,
        was_floating: &BTreeSet<DomainKey>,
    ) {
        let _ = report;
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
        // the swept key.
        let mut by_id: BTreeMap<&WindowId, EngineWindow> = BTreeMap::new();
        for fact in self.staged.facts.values() {
            let Some(rect) = fact.rect else {
                continue;
            };
            if fact.minimized {
                continue;
            }
            by_id.insert(
                &fact.window,
                EngineWindow {
                    window: fact.window.clone(),
                    output: fact.output.clone(),
                    workspace: WorkspaceId(String::new()),
                    rect,
                    floating: fact.floating,
                    fit_excluded: fact.fit_excluded,
                    fullscreen: fact.fullscreen,
                    maximized: fact.maximized,
                    sticky: fact.sticky,
                    fixed_auto: fact.fixed_auto,
                    fixed_suppress: fact.fixed_suppress,
                    hints: fact.hints,
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
        out
    }
}
