//! Hotplug displacement and return (D2, local/global-unique only):
//! removed origins keep their live workspaces as one separate unit on a
//! surviving output, with focused-survivor views and per-member policy
//! transfers for native actuation.
//!
//! Split from [`crate::world_publish`]: this module only extends the
//! [`crate::world_engine::WorldEngine`] `impl`.

use std::collections::BTreeSet;

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::session::DomainKey;
use crate::workspace_scope::{
    DisplacedEntry, OutputFacts, SurvivorInfo, WorkspaceMode, choose_displaced_destination,
};
use crate::world_engine::{RetainedTransfer, WorldEngine};
use crate::world_publish::{current_view, live_native_order, observed_homing};
use crate::world_report::{TransferKind, TransferTarget, WorldReport};

impl WorldEngine {
    /// Strip workspace ids from every mapping entry except `keep`.
    fn take_workspaces_from_all(&mut self, ids: &[String], keep: &str) {
        for (output, list) in self.mapping.iter_mut() {
            if output != keep {
                list.retain(|id| !ids.iter().any(|dropped| dropped == id));
            }
        }
    }

    /// Move the retained tiling mode for one workspace from one output
    /// scope to another, so manual toggles follow hotplug displacement
    /// and return instead of reseeding from the saved default.
    fn carry_tiling(&mut self, workspace: &str, from_output: &str, to_output: &str) {
        if from_output == to_output {
            return;
        }
        let from = DomainKey {
            output: OutputId(from_output.to_owned()),
            workspace: WorkspaceId(workspace.to_owned()),
        };
        if let Some(mode) = self.tiling.remove(&from) {
            self.tiling.insert(
                DomainKey {
                    output: OutputId(to_output.to_owned()),
                    workspace: WorkspaceId(workspace.to_owned()),
                },
                mode,
            );
        }
    }

    /// Policy member moves for a fresh hotplug placement: the
    /// then-current roster observed in `workspaces`, each without an
    /// outstanding transfer or float placement, homed away from `target`.
    /// Sticky windows never move (they belong everywhere already);
    /// minimized absences ride their explicit rows. Fresh placements only,
    /// so proven refusals never re-arm and user commands never duplicate;
    /// newer external input settles through the shared transfer path
    /// (a user-moved window is never pulled back).
    fn policy_member_moves(
        &self,
        report: &WorldReport,
        workspaces: &[String],
        target: &OutputId,
    ) -> Vec<(WindowId, DomainKey, DomainKey)> {
        let in_scope: BTreeSet<&str> = workspaces.iter().map(String::as_str).collect();
        let mut moves = Vec::new();
        for domain in &report.domains {
            for window in &domain.windows {
                if !in_scope.contains(window.workspace.0.as_str()) {
                    continue;
                }
                if window.sticky {
                    continue;
                }
                if self.retained.iter().any(
                    |r| matches!(&r.target.kind, TransferKind::Window(w) if w == &window.window),
                ) {
                    continue;
                }
                if self
                    .retained_floats
                    .iter()
                    .any(|held| held.window == window.window)
                {
                    continue;
                }
                let source = DomainKey {
                    output: window.output.clone(),
                    workspace: window.workspace.clone(),
                };
                let dest = DomainKey {
                    output: target.clone(),
                    workspace: window.workspace.clone(),
                };
                if source == dest {
                    continue;
                }
                moves.push((window.window.clone(), dest, source));
            }
        }
        moves
    }

    /// Hotplug displacement and return (D2, local/global-unique only;
    /// shared mode never displaces). Removed origins keep their live
    /// workspaces as one separate unit on a surviving output (never
    /// merged into the survivor layout): the mapping moves them origin
    /// to survivor, tiling modes follow the workspace, and a fresh
    /// displacement or return retains a policy view only when the active
    /// window homes in the moving workspace (all other views are
    /// preserved, never stolen). Returning origins reclaim their
    /// then-current contents and the record drops; empty records expire.
    /// No snapshots: contents ride live observation throughout, and
    /// per-output history discards with the output in the history pass
    /// below. The adapter actuates views natively from the retained
    /// transfer; rows publish from the moved mapping with no adapter
    /// displacement state.
    pub(crate) fn converge_displacement(&mut self, report: &WorldReport) {
        if report.workspace_mode == WorkspaceMode::Shared {
            return;
        }
        // Fail-closed like the legacy topology handler: partial
        // enumeration never moves displacement state in either direction.
        // Geometry-dependent remap additionally fences narrowly on
        // missing/invalid/duplicate FULL rects (never derived from work
        // areas or window frames); unrelated tiling still converges.
        if !report.inventory_complete || report.topology.iter().any(|topology| !topology.complete) {
            return;
        }
        {
            let mut seen: BTreeSet<&str> = BTreeSet::new();
            for topology in &report.topology {
                let Some(rect) = topology.geometry else {
                    return;
                };
                if topology.output.0.is_empty()
                    || !crate::bounds::valid_carried_rect(rect.x, rect.y, rect.w, rect.h)
                    || !seen.insert(topology.output.0.as_str())
                {
                    return;
                }
            }
        }
        let live_order = live_native_order(report);
        let live: BTreeSet<&str> = live_order.iter().map(String::as_str).collect();
        let live_outputs: BTreeSet<&str> = report
            .topology
            .iter()
            .map(|topology| topology.output.0.as_str())
            .collect();
        // Retain last observed native facts for every live output; removed
        // origins keep their disconnect-time facts below while a record
        // needs placement.
        for topology in &report.topology {
            self.output_facts.insert(
                topology.output.0.clone(),
                OutputFacts {
                    rect: topology.geometry,
                    primary: topology.primary,
                },
            );
        }
        // Wired active-screen primary, else deterministic first order
        // (mirrors the legacy active-screen key with its order fallback).
        let primary = report
            .topology
            .iter()
            .find(|topology| topology.primary)
            .map(|topology| topology.output.0.clone())
            .or_else(|| {
                report
                    .topology
                    .first()
                    .map(|topology| topology.output.0.clone())
            });
        let primary_ref = primary.as_deref();
        // Live native geometry per surviving output for the nearest
        // fallback (independent output facts, never window frames).
        let survivors: Vec<SurvivorInfo> = report
            .topology
            .iter()
            .map(|topology| SurvivorInfo {
                key: topology.output.0.clone(),
                rect: topology.geometry,
            })
            .collect();
        let mut fresh_views: Vec<(OutputId, String)> = Vec::new();
        let mut fresh_members: Vec<(WindowId, DomainKey, DomainKey)> = Vec::new();
        // Returns first: a live origin reclaims its then-current contents.
        let returned: Vec<String> = self
            .displaced
            .keys()
            .filter(|origin| live_outputs.contains(origin.as_str()))
            .cloned()
            .collect();
        for origin in returned {
            let Some(entry) = self.displaced.remove(&origin) else {
                continue;
            };
            let home: Vec<String> = entry
                .workspaces
                .into_iter()
                .filter(|id| live.contains(id.as_str()))
                .collect();
            let origin_id = OutputId(origin.clone());
            self.take_workspaces_from_all(&home, &origin);
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
                fresh_views.push((origin_id.clone(), id.clone()));
            }
            // Return actuation: then-current members homed away from the
            // origin ride one policy transfer each (adapter moves them
            // natively; observation proof settles).
            fresh_members.extend(self.policy_member_moves(report, &home, &origin_id));
        }
        // Displacement: removed origins place their live workspaces as
        // one separate unit on one surviving output.
        let removed: Vec<String> = self
            .mapping
            .keys()
            .filter(|output| !live_outputs.contains(output.as_str()))
            .cloned()
            .collect();
        for origin in removed {
            let mut candidates: Vec<String> =
                self.mapping.get(&origin).cloned().unwrap_or_default();
            if let Some(record) = self.displaced.get(&origin) {
                for id in &record.workspaces {
                    if !candidates.iter().any(|kept| kept == id) {
                        candidates.push(id.clone());
                    }
                }
            }
            candidates.retain(|id| live.contains(id.as_str()));
            candidates.sort();
            candidates.dedup();
            let prev = self
                .displaced
                .get(&origin)
                .map(|record| record.dest.clone());
            if candidates.is_empty() {
                self.displaced.remove(&origin);
                continue;
            }
            let dest = prev
                .clone()
                .filter(|dest| live_outputs.contains(dest.as_str()))
                .or_else(|| {
                    // Disconnect-time removed rect when retained, never a
                    // post-disconnect window frame proxy.
                    let removed = self.output_facts.get(&origin).and_then(|facts| facts.rect);
                    choose_displaced_destination(&survivors, primary_ref, removed)
                });
            let Some(dest) = dest else {
                // No survivor: keep the mapping untouched; the next
                // report retries once outputs exist again.
                continue;
            };
            let fresh = prev.as_deref() != Some(dest.as_str());
            self.take_workspaces_from_all(&candidates, &dest);
            let slot = self.mapping.entry(dest.clone()).or_default();
            for id in &candidates {
                if !slot.iter().any(|kept| kept == id) {
                    slot.push(id.clone());
                }
            }
            let from = prev.unwrap_or(origin.clone());
            for id in &candidates {
                self.carry_tiling(id, &from, &dest);
            }
            self.displaced.insert(
                origin,
                DisplacedEntry {
                    workspaces: candidates.clone(),
                    dest: dest.clone(),
                },
            );
            if fresh {
                let dest_id = OutputId(dest.clone());
                fresh_members.extend(self.policy_member_moves(report, &candidates, &dest_id));
                for id in candidates {
                    fresh_views.push((dest_id.clone(), id));
                }
            }
        }
        // Focused survivor view policy, fresh placements only: when the
        // active window homes in a moving workspace whose output does not
        // show it, retain the view (adapter actuates; focus stays native).
        // Operation identities pin the creating revision plus a per-run
        // index: stable across repeated snapshots of the outstanding
        // intent (fresh placements only, never reissued), yet distinct
        // for every new displacement/return cycle, so stale feedback for
        // a settled cycle can never settle the new target. Revisions never
        // wrap (exhaustion re-bootstraps), and no raw ids ride the token,
        // keeping it far inside the opaque bound. Incarnation/generation
        // resets clear all retained intents with the revision scope.
        let epoch = self.state_revision;
        if !report.focused_global.is_empty() {
            let active = WindowId(report.focused_global.clone());
            if let Some(homing) = observed_homing(report, &active) {
                for (index, (output, workspace)) in fresh_views.into_iter().enumerate() {
                    if homing.workspace.0 == workspace
                        && current_view(report, &output) != Some(workspace.as_str())
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
        // affected member (adapter actuates natively under its stable
        // operation identity; observation proof, proven refusal, or newer
        // external input settles through the shared transfer path).
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
            });
        }
        // Prune retained output facts to live outputs plus displaced
        // origins still needing placement; returned origins drop with
        // their records so a later disconnect stores fresh facts.
        self.output_facts.retain(|output, _| {
            live_outputs.contains(output.as_str()) || self.displaced.contains_key(output)
        });
    }
}
