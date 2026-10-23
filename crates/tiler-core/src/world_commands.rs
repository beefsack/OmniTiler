//! World command execution (D2/D3): one fresh command per accepted report.
//!
//! Owns selection/send/migration planning over the logical rings; the
//! [`CoreCommand`] operation bridge lives in [`crate::world_operations`]
//! and staged observation/homing in [`crate::world_observe`]. Transition
//! entry points stay on [`crate::world_engine::WorldEngine`]; this module
//! only extends its `impl`.

use std::collections::{BTreeMap, BTreeSet};

use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
use crate::directional::{Direction, OutputId, WindowId};
use crate::engine::Engine;
use crate::ids::{CorrelationId, OwnerId};
use crate::seed::EngineWindow;
use crate::session::{DomainKey, OutputDomain};
use crate::workspace_scope::{
    PreviousTarget, TrailingSelection, resolve_numbered, resolve_relative_send,
    resolve_relative_step, resolve_trailing,
};
use crate::world_engine::{
    KIND_CREATION_EXHAUSTED, KIND_DIRECTION, KIND_FOCUS, KIND_MIGRATION, KIND_SCOPE_HELD,
    KIND_SELECT_UNKNOWN, KIND_SEND_TARGET, KIND_SEND_WINDOW, KIND_SNAPSHOT, MSG_CREATION_EXHAUSTED,
    MSG_DIRECTION, MSG_FOCUS, MSG_MIGRATION, MSG_MIGRATION_FLIGHT, MSG_MIGRATION_INVENTORY,
    MSG_MIGRATION_RECIPROCAL, MSG_MIGRATION_RESIDUE, MSG_MIGRATION_SHARED, MSG_MIGRATION_SOURCE,
    MSG_MIGRATION_SOURCE_UNREADABLE, MSG_MIGRATION_TARGET_GEOMETRY,
    MSG_MIGRATION_TARGET_INCOMPLETE, MSG_SCOPE_HELD, MSG_SELECT_UNKNOWN, MSG_SEND_TARGET,
    MSG_SEND_WINDOW, MSG_SNAPSHOT, RetainedTransfer, WorldEngine,
};
use crate::world_report::{
    CommandResult, OneShotEffect, SendSource, TransferKind, TransferTarget, WorldCommand,
    WorldReport,
};

impl WorldEngine {
    /// Execute one fresh command after the sweep. Returns the emitted
    /// one-shot effect, if any. Every accepted command advances the
    /// monotonic command high-watermark; commandless reports never move it.
    /// Staged carriers back the planning index; logical rings and views
    /// authorize every target. `frozen_send_source` carries the implicit
    /// send source resolved from pre-observation retained homing in
    /// [`run_accepted`](crate::world_engine::WorldEngine::run_accepted)
    /// before staging (`None` unless the report carries an implicit send
    /// that resolved, or an implicit send that did not resolve): the
    /// command below must use it verbatim and never re-resolve, so
    /// first-tick admissions and rehomed movers keep the retired
    /// adapter contract exactly.
    pub(crate) fn execute_command(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        command: &WorldCommand,
        held_scopes: &BTreeSet<DomainKey>,
        frozen_send_source: Option<OutputId>,
        frozen_migration_source: Option<(OutputId, String)>,
    ) -> CommandApplied {
        // Staged carrier index for command planning: homed Engine
        // carriers keyed by lifetime. Owned locally so planning borrows
        // nothing from `self`.
        let mut owned: Vec<EngineWindow> = Vec::new();
        for domain in &self.staged.domains {
            owned.extend(domain.windows.iter().cloned());
        }
        let mut by_id: BTreeMap<&WindowId, &EngineWindow> = BTreeMap::new();
        for window in &owned {
            by_id.insert(&window.window, window);
        }
        match command {
            WorldCommand::Focus { window, direction } => {
                if !matches!(direction.as_str(), "left" | "right" | "up" | "down") {
                    return CommandApplied::Rejected {
                        kind: KIND_DIRECTION,
                        message: MSG_DIRECTION,
                    };
                }
                let known = by_id.contains_key(window)
                    || self.engine.domain_keys().iter().any(|key| {
                        self.engine.session(key).is_some_and(|session| {
                            session
                                .snapshot()
                                .windows
                                .iter()
                                .any(|link| &link.window == window)
                                || session.is_exception(window)
                        })
                    });
                if !known {
                    return CommandApplied::Rejected {
                        kind: KIND_FOCUS,
                        message: MSG_FOCUS,
                    };
                }
                self.note_command(command_seq);
                CommandApplied::Effect(OneShotEffect::explicit_focus(
                    command_seq,
                    window.clone(),
                    self.state_revision + 1,
                ))
            }
            WorldCommand::Send {
                window,
                target_output,
                target_workspace,
                follow,
            } => self.start_send(
                report,
                command_seq,
                window,
                target_output,
                target_workspace,
                *follow,
                &by_id,
                held_scopes,
            ),
            WorldCommand::SwitchView { output, workspace } => {
                self.retain_view(report, command_seq, output, workspace)
            }
            WorldCommand::SelectWorkspace { output, workspace } => {
                // Global-unique visible-elsewhere swap: selecting a target
                // visible on another output exchanges the two views instead
                // of duplicating the selection, with scoped numbering kept.
                if report.workspace_mode == crate::workspace_scope::WorkspaceMode::GlobalUnique {
                    let mut current_by_output: BTreeMap<String, String> = BTreeMap::new();
                    for screen in &report.screens {
                        if let Some(view) = self.effective_view(&screen.output) {
                            current_by_output.insert(screen.output.0.clone(), view);
                        }
                    }
                    if let Some(plan) = crate::workspace_scope::plan_global_swap(
                        &current_by_output,
                        &output.0,
                        workspace,
                    ) {
                        // Nonempty swap must move complete logical units:
                        // Engine sessions rekey to the moved outputs (tree,
                        // order/shares, focus, floats, minimized slots ride
                        // the relocate), tiling/floating scopes follow the
                        // ids, and retained migration flights keep delayed
                        // members homed at the source with Controlled
                        // desire at the target until arrival. Empty swaps
                        // keep the mapping/views/history-only path.
                        let out_a = OutputId(plan.active_output.clone());
                        let out_b = OutputId(plan.other_output.clone());
                        let src_a = DomainKey {
                            output: out_a.clone(),
                            workspace: crate::directional::WorkspaceId(plan.active_current.clone()),
                        };
                        let tgt_a = DomainKey {
                            output: out_b.clone(),
                            workspace: crate::directional::WorkspaceId(plan.active_current.clone()),
                        };
                        let src_b = DomainKey {
                            output: out_b.clone(),
                            workspace: crate::directional::WorkspaceId(plan.target.clone()),
                        };
                        let tgt_b = DomainKey {
                            output: out_a.clone(),
                            workspace: crate::directional::WorkspaceId(plan.target.clone()),
                        };
                        let members_a = Self::swap_source_members(self, &src_a);
                        let members_b = Self::swap_source_members(self, &src_b);
                        let has_session_a = self.engine.session(&src_a).is_some();
                        let has_session_b = self.engine.session(&src_b).is_some();
                        if !has_session_a
                            && !has_session_b
                            && members_a.is_empty()
                            && members_b.is_empty()
                        {
                            let next = crate::workspace_scope::apply_swap_assignments(
                                &self.mapping,
                                &plan,
                            );
                            self.mapping = next;
                            self.views
                                .insert(plan.active_output.clone(), plan.target.clone());
                            self.views
                                .insert(plan.other_output.clone(), plan.active_current.clone());
                            for out in [&plan.active_output, &plan.other_output] {
                                let ring = self.mapping.get(out).cloned().unwrap_or_default();
                                if let Some(cur) = self.views.get(out).cloned() {
                                    self.history.observe(out, &cur, &ring);
                                }
                            }
                            self.note_command(command_seq);
                            self.retain_view(report, command_seq, output, workspace);
                            // retain_view pushed a second identical flight for
                            // the active output; the views above already moved,
                            // so one flight suffices.
                            return CommandApplied::NoEffect;
                        }
                        // Safety fences for the stateful move: complete
                        // inventory, live screens, no held scopes on any
                        // touched key. Refusals reuse the migration
                        // vocabulary and leave all state untouched.
                        let held_touched = [&src_a, &tgt_a, &src_b, &tgt_b]
                            .iter()
                            .any(|key| held_scopes.contains(*key));
                        let screens_complete = self.screen_complete(&out_a) == Some(true)
                            && self.screen_complete(&out_b) == Some(true);
                        if !report.inventory_complete || !screens_complete || held_touched {
                            return CommandApplied::Rejected {
                                kind: KIND_MIGRATION,
                                message: MSG_MIGRATION,
                            };
                        }
                        if self.engine.session(&tgt_a).is_some()
                            || self.engine.session(&tgt_b).is_some()
                        {
                            return CommandApplied::Rejected {
                                kind: KIND_MIGRATION,
                                message: MSG_MIGRATION,
                            };
                        }
                        // Target geometry from the staged screens (inset
                        // work areas, matching the migration commit).
                        let target_geo = |out: &OutputId| {
                            self.staged
                                .screens
                                .iter()
                                .find(|screen| screen.output == *out)
                                .and_then(|screen| {
                                    let area = screen.work_area?;
                                    let inset =
                                        crate::geometry::inset_bounds(area, screen.outer_gap)
                                            .ok()?;
                                    Some((inset, screen.gap, screen.outer_gap))
                                })
                        };
                        let Some((bounds_a, gap_a, outer_a)) = target_geo(&out_b) else {
                            return CommandApplied::Rejected {
                                kind: KIND_MIGRATION,
                                message: MSG_MIGRATION,
                            };
                        };
                        let Some((bounds_b, gap_b, outer_b)) = target_geo(&out_a) else {
                            return CommandApplied::Rejected {
                                kind: KIND_MIGRATION,
                                message: MSG_MIGRATION,
                            };
                        };
                        // Validate both relocates on clones before mutating, with the
                        // migration sticky-exclusion policy: sticky floats stay
                        // on the physical source output (same-output rehome to
                        // the incoming view), never cross-output. Strip sticky
                        // exceptions before relocate validation; they merge
                        // into the incoming moved session on the same physical
                        // output. A stripped sticky-only side is still a valid
                        // rootless empty domain to move (trees holds key with
                        // `None`), then the opposite physical sticky
                        // reattaches to it. A side with no session at all
                        // contributes no host: the opposite sticky then rides
                        // a stay derived from its own actual source session
                        // (never a fresh session). Tiled or minimized sticky
                        // fences with the migration vocabulary (no silent
                        // wrong move).
                        let sticky_ids_for =
                            |session: &crate::session::Session| -> (BTreeSet<WindowId>, bool) {
                                let mut ids = BTreeSet::new();
                                for observed in session.exception_observed() {
                                    if self
                                        .staged
                                        .facts
                                        .get(&observed.window)
                                        .is_some_and(|fact| fact.sticky)
                                    {
                                        ids.insert(observed.window.clone());
                                    }
                                }
                                let tiled_sticky = session.snapshot().windows.iter().any(|link| {
                                    self.staged
                                        .facts
                                        .get(&link.window)
                                        .is_some_and(|fact| fact.sticky)
                                });
                                let minimized_sticky =
                                    session.minimized_entries().iter().any(|(window, _, _)| {
                                        self.staged
                                            .facts
                                            .get(window)
                                            .is_some_and(|fact| fact.sticky)
                                    });
                                (ids, tiled_sticky || minimized_sticky)
                            };
                        let mut moved_a = None;
                        let mut sticky_a = Vec::new();
                        let mut ids_a = BTreeSet::new();
                        let mut source_a = None;
                        if let Some(session) = self.engine.session(&src_a).cloned() {
                            let (ids, bad) = sticky_ids_for(&session);
                            if bad {
                                return CommandApplied::Rejected {
                                    kind: KIND_MIGRATION,
                                    message: MSG_MIGRATION,
                                };
                            }
                            let mut stripped = session.clone();
                            sticky_a = stripped.extract_exception_floats(&ids);
                            let stripped_empty = stripped.snapshot().windows.is_empty()
                                && stripped.exception_count() == 0
                                && stripped.minimized_entries().is_empty();
                            if stripped_empty && sticky_a.is_empty() {
                                // Degenerate empty session carries nothing;
                                // leave the stale slot for the usual cleanup
                                // (exact prior behavior).
                            } else {
                                // Rootless empty domains relocate fine, so
                                // sticky-only sides validate like the rest.
                                if !stripped.relocate_domain(&src_a, &tgt_a, bounds_a, gap_a) {
                                    return CommandApplied::Rejected {
                                        kind: KIND_MIGRATION,
                                        message: MSG_MIGRATION,
                                    };
                                }
                                moved_a = Some(stripped);
                            }
                            ids_a = ids;
                            source_a = Some(session);
                        }
                        let mut moved_b = None;
                        let mut sticky_b = Vec::new();
                        let mut ids_b = BTreeSet::new();
                        let mut source_b = None;
                        if let Some(session) = self.engine.session(&src_b).cloned() {
                            let (ids, bad) = sticky_ids_for(&session);
                            if bad {
                                return CommandApplied::Rejected {
                                    kind: KIND_MIGRATION,
                                    message: MSG_MIGRATION,
                                };
                            }
                            let mut stripped = session.clone();
                            sticky_b = stripped.extract_exception_floats(&ids);
                            let stripped_empty = stripped.snapshot().windows.is_empty()
                                && stripped.exception_count() == 0
                                && stripped.minimized_entries().is_empty();
                            if stripped_empty && sticky_b.is_empty() {
                                // Degenerate empty session: see above.
                            } else {
                                if !stripped.relocate_domain(&src_b, &tgt_b, bounds_b, gap_b) {
                                    return CommandApplied::Rejected {
                                        kind: KIND_MIGRATION,
                                        message: MSG_MIGRATION,
                                    };
                                }
                                moved_b = Some(stripped);
                            }
                            ids_b = ids;
                            source_b = Some(session);
                        }
                        // Derive both sessionless sticky stays before any
                        // commit: the derivation is fallible, so both sides
                        // prepare before the mapping/views/history commit
                        // and either Engine rekey. A failed preparation
                        // refuses with the migration vocabulary and leaves
                        // all world state untouched.
                        let mut stay_a = None;
                        if moved_a.is_none() && !sticky_b.is_empty() {
                            let Some(source_b) = source_b.as_ref() else {
                                return CommandApplied::Rejected {
                                    kind: KIND_MIGRATION,
                                    message: MSG_MIGRATION,
                                };
                            };
                            let Some(stay) =
                                source_b.split_sticky_stay(&ids_b, &tgt_a, bounds_a, gap_a)
                            else {
                                return CommandApplied::Rejected {
                                    kind: KIND_MIGRATION,
                                    message: MSG_MIGRATION,
                                };
                            };
                            stay_a = Some(stay);
                        }
                        let mut stay_b = None;
                        if moved_b.is_none() && !sticky_a.is_empty() {
                            let Some(source_a) = source_a.as_ref() else {
                                return CommandApplied::Rejected {
                                    kind: KIND_MIGRATION,
                                    message: MSG_MIGRATION,
                                };
                            };
                            let Some(stay) =
                                source_a.split_sticky_stay(&ids_a, &tgt_b, bounds_b, gap_b)
                            else {
                                return CommandApplied::Rejected {
                                    kind: KIND_MIGRATION,
                                    message: MSG_MIGRATION,
                                };
                            };
                            stay_b = Some(stay);
                        }
                        // Commit mapping/views/history first (existing
                        // parked-history semantics: out-of-scope previous
                        // entries invalidate via observe).
                        let next =
                            crate::workspace_scope::apply_swap_assignments(&self.mapping, &plan);
                        self.mapping = next;
                        self.views
                            .insert(plan.active_output.clone(), plan.target.clone());
                        self.views
                            .insert(plan.other_output.clone(), plan.active_current.clone());
                        for out in [&plan.active_output, &plan.other_output] {
                            let ring = self.mapping.get(out).cloned().unwrap_or_default();
                            if let Some(cur) = self.views.get(out).cloned() {
                                self.history.observe(out, &cur, &ring);
                            }
                        }
                        // Commit the Engine rekey atomically: both sources
                        // removed, both targets stored. Targets were proven
                        // free above, so no member state is deleted to avoid
                        // a collision. Sticky merges into the incoming moved
                        // session on the same physical output (no frame
                        // writes, Untouched). When this side contributed no
                        // moved host (no source session at all), the opposite
                        // sticky rides the stay derived before the commit
                        // above; the derivation already fenced there rather
                        // than losing the lifetime.
                        if let Some(mut moved) = moved_a {
                            if !sticky_b.is_empty() {
                                moved.insert_exception_floats(sticky_b, &tgt_a);
                            }
                            self.engine.remove(&src_a);
                            self.engine.insert_raw(tgt_a.clone(), moved, outer_a);
                        } else if !sticky_b.is_empty() {
                            let Some(stay) = stay_a else {
                                return CommandApplied::Rejected {
                                    kind: KIND_MIGRATION,
                                    message: MSG_MIGRATION,
                                };
                            };
                            self.engine.insert_raw(tgt_a.clone(), stay, outer_a);
                        }
                        if let Some(mut moved) = moved_b {
                            if !sticky_a.is_empty() {
                                moved.insert_exception_floats(sticky_a, &tgt_b);
                            }
                            self.engine.remove(&src_b);
                            self.engine.insert_raw(tgt_b.clone(), moved, outer_b);
                        } else if !sticky_a.is_empty() {
                            let Some(stay) = stay_b else {
                                return CommandApplied::Rejected {
                                    kind: KIND_MIGRATION,
                                    message: MSG_MIGRATION,
                                };
                            };
                            self.engine.insert_raw(tgt_b.clone(), stay, outer_b);
                        }
                        // Tiling modes and floating scopes follow the ids.
                        let mode_a = self.tiling.remove(&src_a);
                        let mode_b = self.tiling.remove(&src_b);
                        if let Some(mode) = mode_a {
                            self.tiling.insert(tgt_a.clone(), mode);
                        }
                        if let Some(mode) = mode_b {
                            self.tiling.insert(tgt_b.clone(), mode);
                        }
                        for (src, tgt) in [(&src_a, &tgt_a), (&src_b, &tgt_b)] {
                            if self.floating_scopes.remove(src) {
                                self.floating_scopes.insert(tgt.clone());
                            }
                        }
                        // Single-owner membership follows both logical ids.
                        self.rekey_members(&src_a.workspace.0, &src_a.output.0, &tgt_a.output.0);
                        self.rekey_members(&src_b.workspace.0, &src_b.output.0, &tgt_b.output.0);
                        // Retained actuation flights for nonempty sides:
                        // delayed members home at the source by raw screen
                        // (existing migration homing) with Controlled
                        // desire at the target; unchanged reports hold
                        // without flap, arrival plus view echoes settle.
                        if !members_a.is_empty() {
                            self.retained.push(RetainedTransfer {
                                target: TransferTarget {
                                    operation_id: format!("op-{command_seq}-swap-a"),
                                    kind: TransferKind::Migration,
                                    target_output: tgt_a.output.clone(),
                                    target_workspace: tgt_a.workspace.0.clone(),
                                    follow: false,
                                    held: false,
                                },
                                policy: false,
                                source_output: src_a.output.clone(),
                                source_workspace: src_a.workspace.0.clone(),
                                members: members_a,
                                migration_refill: None,
                                migration_initial_empty: false,
                                migration_source_index: None,
                                migration_source_view: None,
                                migration_target_view: None,
                                migration_terminal: None,
                                pending_member: None,
                                pending_members: BTreeMap::new(),
                                pending_view: None,
                            });
                        }
                        if !members_b.is_empty() {
                            self.retained.push(RetainedTransfer {
                                target: TransferTarget {
                                    operation_id: format!("op-{command_seq}-swap-b"),
                                    kind: TransferKind::Migration,
                                    target_output: tgt_b.output.clone(),
                                    target_workspace: tgt_b.workspace.0.clone(),
                                    follow: false,
                                    held: false,
                                },
                                policy: false,
                                source_output: src_b.output.clone(),
                                source_workspace: src_b.workspace.0.clone(),
                                members: members_b,
                                migration_refill: None,
                                migration_initial_empty: false,
                                migration_source_index: None,
                                migration_source_view: None,
                                migration_target_view: None,
                                migration_terminal: None,
                                pending_member: None,
                                pending_members: BTreeMap::new(),
                                pending_view: None,
                            });
                        }
                        self.note_command(command_seq);
                        self.retain_view(report, command_seq, output, workspace);
                        // retain_view pushed a second identical flight for
                        // the active output; the views above already moved,
                        // so one flight suffices.
                        return CommandApplied::NoEffect;
                    }
                }
                self.retain_view(report, command_seq, output, workspace)
            }
            WorldCommand::SelectNumbered { output, index } => {
                let Some(resolved) = self.resolve_selection_output(report, output.as_ref()) else {
                    return CommandApplied::Rejected {
                        kind: KIND_SELECT_UNKNOWN,
                        message: MSG_SELECT_UNKNOWN,
                    };
                };
                let Some(target) = self
                    .resolve_selection_ring(report, &resolved)
                    .and_then(|ring| resolve_numbered(&ring, *index))
                else {
                    return CommandApplied::Rejected {
                        kind: KIND_SELECT_UNKNOWN,
                        message: MSG_SELECT_UNKNOWN,
                    };
                };
                self.retain_view(report, command_seq, &resolved, &target)
            }
            WorldCommand::SelectRelative { output, delta } => {
                let Some(resolved) = self.resolve_selection_output(report, output.as_ref()) else {
                    return CommandApplied::Rejected {
                        kind: KIND_SELECT_UNKNOWN,
                        message: MSG_SELECT_UNKNOWN,
                    };
                };
                let current = self.effective_view(&resolved).unwrap_or_default();
                let Some(target) = self
                    .resolve_selection_ring(report, &resolved)
                    .and_then(|ring| resolve_relative_step(&ring, &current, *delta))
                else {
                    return CommandApplied::Rejected {
                        kind: KIND_SELECT_UNKNOWN,
                        message: MSG_SELECT_UNKNOWN,
                    };
                };
                self.retain_view(report, command_seq, &resolved, &target)
            }
            WorldCommand::SelectPrevious { output } => {
                let Some(resolved) = self.resolve_selection_output(report, output.as_ref()) else {
                    return CommandApplied::Rejected {
                        kind: KIND_SELECT_UNKNOWN,
                        message: MSG_SELECT_UNKNOWN,
                    };
                };
                // Shared mode walks the one shared history over the
                // unified order; scoped modes use the per-output entry.
                if report.workspace_mode == crate::workspace_scope::WorkspaceMode::Shared {
                    let live: Vec<String> = self.selection_ring(report, &resolved);
                    let current = self.effective_view(&resolved).unwrap_or_default();
                    match self.history.resolve_shared_for(&live, &current) {
                        PreviousTarget::Select(target) => {
                            self.retain_view(report, command_seq, &resolved, &target)
                        }
                        PreviousTarget::AlreadyThere(_) | PreviousTarget::NoHistory => {
                            self.note_command(command_seq);
                            CommandApplied::NoEffect
                        }
                    }
                } else {
                    let live: Vec<String> = self.selection_ring(report, &resolved);
                    let current = self.effective_view(&resolved).unwrap_or_default();
                    match self
                        .history
                        .resolve_previous_for(&resolved.0, &live, &current)
                    {
                        PreviousTarget::Select(target) => {
                            self.retain_view(report, command_seq, &resolved, &target)
                        }
                        PreviousTarget::AlreadyThere(_) | PreviousTarget::NoHistory => {
                            // No-op until the next recorded change: reconcile
                            // only, never retain or emit.
                            self.note_command(command_seq);
                            CommandApplied::NoEffect
                        }
                    }
                }
            }
            WorldCommand::SelectTrailing { output } => {
                let Some(resolved) = self.resolve_selection_output(report, output.as_ref()) else {
                    return CommandApplied::Rejected {
                        kind: KIND_SELECT_UNKNOWN,
                        message: MSG_SELECT_UNKNOWN,
                    };
                };
                let ring = self.selection_ring(report, &resolved);
                let occupied = self.ring_occupied(&resolved);
                let selection = resolve_trailing(&ring, &occupied);
                match selection {
                    TrailingSelection::Reuse(target) => {
                        self.retain_view(report, command_seq, &resolved, &target)
                    }
                    TrailingSelection::Create => {
                        // Trailing workspaces exist immediately in the
                        // logical contract: mint, then select.
                        let created =
                            self.create_trailing_workspace(&resolved, report.default_tiled);
                        let Some(logical) = created else {
                            return CommandApplied::Rejected {
                                kind: KIND_CREATION_EXHAUSTED,
                                message: MSG_CREATION_EXHAUSTED,
                            };
                        };
                        self.retain_view(report, command_seq, &resolved, &logical)
                    }
                }
            }
            WorldCommand::SendRelative {
                window,
                source,
                delta,
                follow,
            } => {
                let Some(source_row) = by_id.get(window) else {
                    return CommandApplied::Rejected {
                        kind: KIND_SEND_WINDOW,
                        message: MSG_SEND_WINDOW,
                    };
                };
                let current = source_row.workspace.0.clone();
                // Implicit sources arrive frozen from pre-observation
                // retained homing (see `run_accepted`); explicit outputs
                // pass through unchanged. An unfrozen implicit source
                // refuses here: nothing was invented, so there is no
                // source scope to resolve a ring over.
                let source = match source {
                    SendSource::Explicit(output) => output.clone(),
                    SendSource::HomeThenAction | SendSource::HomeThenCachedLive => {
                        let Some(resolved) = frozen_send_source.clone() else {
                            return CommandApplied::Rejected {
                                kind: KIND_SEND_WINDOW,
                                message: MSG_SEND_WINDOW,
                            };
                        };
                        resolved
                    }
                };
                let ring = self.send_ring(report, &source_row.output);
                let Some(target_workspace) = resolve_relative_send(&ring, &current, *delta) else {
                    return CommandApplied::Rejected {
                        kind: KIND_SEND_TARGET,
                        message: MSG_SEND_TARGET,
                    };
                };
                self.start_send(
                    report,
                    command_seq,
                    window,
                    &source,
                    &target_workspace,
                    *follow,
                    &by_id,
                    held_scopes,
                )
            }
            WorldCommand::SendNumbered {
                window,
                source,
                index,
                follow,
            } => {
                let output = match source {
                    SendSource::Explicit(output) => output.clone(),
                    SendSource::HomeThenAction | SendSource::HomeThenCachedLive => {
                        let Some(resolved) = frozen_send_source.clone() else {
                            return CommandApplied::Rejected {
                                kind: KIND_SEND_WINDOW,
                                message: MSG_SEND_WINDOW,
                            };
                        };
                        resolved
                    }
                };
                self.start_numbered_send(
                    report,
                    command_seq,
                    window,
                    &output,
                    *index,
                    *follow,
                    &by_id,
                    held_scopes,
                )
            }
            WorldCommand::SetWorkspaceTiling {
                output,
                workspace,
                tiled,
            } => self.set_workspace_tiling(report, command_seq, output, workspace, *tiled),
            WorldCommand::MigrateWorkspace {
                source_output,
                target_output,
                workspace,
            } => self.start_migration(
                report,
                command_seq,
                source_output,
                target_output,
                workspace,
                held_scopes,
            ),
            WorldCommand::FocusDirectional { window, direction } => {
                self.focus_directional(report, command_seq, window, direction, &by_id, held_scopes)
            }
            WorldCommand::MoveDirectional {
                window,
                direction,
                same_axis,
            } => self.move_directional(
                report,
                command_seq,
                window,
                direction,
                *same_axis,
                &by_id,
                held_scopes,
            ),
            WorldCommand::SendDirectional {
                window,
                direction,
                follow,
            } => self.send_directional(
                report,
                command_seq,
                window,
                direction,
                *follow,
                &by_id,
                held_scopes,
            ),
            WorldCommand::MigrateDirectional { direction } => {
                // Direction-only commands resolve no output: the frozen
                // pre-observation source (output plus its retained view)
                // applies verbatim, or the command refuses when the frozen
                // source is unknown. Same-report view settlement or
                // bootstrap must never retarget the current intent; the
                // strict topology target and all gates below run on staged
                // state unchanged.
                let Some((source, view)) = frozen_migration_source.clone() else {
                    return CommandApplied::Rejected {
                        kind: KIND_MIGRATION,
                        message: MSG_MIGRATION_SOURCE,
                    };
                };
                self.migrate_directional(
                    report,
                    command_seq,
                    &source,
                    &view,
                    direction,
                    &by_id,
                    held_scopes,
                )
            }
            WorldCommand::Operation(command) => {
                self.execute_operation(report, command_seq, command, &by_id, held_scopes)
            }
        }
    }
}

impl WorldEngine {
    /// Retained member set for one global-unique swap side: the single-owner
    /// core membership homed at the source (tiled, float, minimized alike;
    /// floating scopes carry no session) plus staged facts homed there.
    /// Sticky facts stay out: they are adapter-owned all-desktop lifetimes,
    /// never swapped member state. Sorted and deduplicated for deterministic
    /// settlement.
    pub(crate) fn swap_source_members(engine: &WorldEngine, source: &DomainKey) -> Vec<WindowId> {
        let mut members: Vec<WindowId> = Vec::new();
        for (window, state) in &engine.members {
            if state.home == *source {
                members.push(window.clone());
            }
        }
        for domain in &engine.staged.domains {
            if domain.key != *source {
                continue;
            }
            for window in &domain.windows {
                members.push(window.window.clone());
            }
            members.extend(domain.minimized.iter().cloned());
        }
        members.retain(|window| {
            !engine
                .staged
                .facts
                .get(window)
                .is_some_and(|fact| fact.sticky)
        });
        members.sort();
        members.dedup();
        members
    }

    /// Whole-workspace migration through the logical rings plus the
    /// retained Engine rekey (R-WS-12): refusal reasons stay in the fixed
    /// vocabulary with no native writes. The moved logical workspace keeps
    /// its id, tree, order/shares, remembered focus and tiling mode; only
    /// its owning screen changes. The Engine owns the transactional
    /// tree/order/gaps/float-overlay relocation (`Engine::handle` with
    /// `MigrateWorkspace`, D8 carry included); this path only scopes,
    /// retains, and reports. Scope planning runs before the Engine commit
    /// so a scope refusal never leaves a committed rekey behind; an Engine
    /// refusal leaves scope, flights, and ordering exactly as before.
    /// The commit is immediately authoritative: the target scope publishes
    /// Controlled membership desire before any arrival, and unchanged
    /// reports never flap the source. Member arrival, target view, and
    /// source refill settle independently through the retained flight.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start_migration(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        source_output: &OutputId,
        target_output: &OutputId,
        workspace: &str,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        // Shared mode refuses whole-workspace migration with a reason
        // before any writes and no native settings mutation.
        if report.workspace_mode == crate::workspace_scope::WorkspaceMode::Shared {
            return CommandApplied::Rejected {
                kind: KIND_MIGRATION,
                message: MSG_MIGRATION_SHARED,
            };
        }
        // Partial inventory fences: members and views must prove fully.
        if !report.inventory_complete {
            return CommandApplied::Rejected {
                kind: KIND_MIGRATION,
                message: MSG_MIGRATION_INVENTORY,
            };
        }
        let source_key = DomainKey {
            output: source_output.clone(),
            workspace: crate::directional::WorkspaceId(workspace.to_owned()),
        };
        let target_key = DomainKey {
            output: target_output.clone(),
            workspace: crate::directional::WorkspaceId(workspace.to_owned()),
        };
        // Logical homing prerequisites: the workspace lives in the source
        // ring, both screens are live and complete, and neither key is
        // held. Floating sources carry below by sessionless core-owned
        // membership remap (no retained session, no tree to relocate),
        // so only genuinely held scopes refuse here.
        let homed_source = self
            .mapping
            .get(&source_output.0)
            .is_some_and(|ring| ring.iter().any(|id| id == workspace));
        if !homed_source {
            return CommandApplied::Rejected {
                kind: KIND_MIGRATION,
                message: MSG_MIGRATION_SOURCE,
            };
        }
        let source_staged = self.staged.domains.iter().find(|d| d.key == source_key);
        let source_readable = source_staged.is_some_and(|d| d.readable);
        let target_complete = self.screen_complete(target_output) == Some(true);
        if !source_readable {
            return CommandApplied::Rejected {
                kind: KIND_MIGRATION,
                message: MSG_MIGRATION_SOURCE_UNREADABLE,
            };
        }
        if !target_complete {
            return CommandApplied::Rejected {
                kind: KIND_MIGRATION,
                message: MSG_MIGRATION_TARGET_INCOMPLETE,
            };
        }
        if held_scopes.contains(&source_key) || held_scopes.contains(&target_key) {
            return CommandApplied::Rejected {
                kind: KIND_SCOPE_HELD,
                message: MSG_SCOPE_HELD,
            };
        }
        // One flight per source workspace: a retained migration still
        // awaiting arrival fences a second commit.
        if self.retained.iter().any(|r| {
            matches!(&r.target.kind, TransferKind::Migration)
                && r.source_output == *source_output
                && r.source_workspace == workspace
        }) {
            return CommandApplied::Rejected {
                kind: KIND_MIGRATION,
                message: MSG_MIGRATION_FLIGHT,
            };
        }
        // Target residue refuses as occupied: a homed fact at the target
        // key means newer external input already claims it.
        if self.staged.homing.values().any(|key| key == &target_key) {
            return CommandApplied::Rejected {
                kind: KIND_MIGRATION,
                message: MSG_MIGRATION_RESIDUE,
            };
        }
        let Some((direction, direction_token)) =
            self.reciprocal_direction_for(source_output, target_output)
        else {
            return CommandApplied::Rejected {
                kind: KIND_MIGRATION,
                message: MSG_MIGRATION_RECIPROCAL,
            };
        };
        let _ = direction;
        // Target scope geometry: the target screen's work area with its
        // own gaps. Inset exhaustion refuses with no mutation.
        let target_screen = self
            .staged
            .screens
            .iter()
            .find(|screen| screen.output == *target_output);
        let Some((work_area, gap, outer_gap)) =
            target_screen.map(|screen| (screen.work_area, screen.gap, screen.outer_gap))
        else {
            return CommandApplied::Rejected {
                kind: KIND_MIGRATION,
                message: MSG_MIGRATION_TARGET_GEOMETRY,
            };
        };
        let Some(work_area) = work_area else {
            return CommandApplied::Rejected {
                kind: KIND_MIGRATION,
                message: MSG_MIGRATION_TARGET_GEOMETRY,
            };
        };
        let projected = match crate::geometry::inset_bounds(work_area, outer_gap) {
            Ok(projected) => projected,
            Err(_) => {
                return CommandApplied::Rejected {
                    kind: KIND_MIGRATION,
                    message: MSG_MIGRATION_TARGET_GEOMETRY,
                };
            }
        };
        let target_domain = OutputDomain {
            id: target_output.clone(),
            workspace: crate::directional::WorkspaceId(workspace.to_owned()),
            bounds: projected,
            gap,
            adjacent: BTreeMap::new(),
        };
        if !target_domain.validate() {
            return CommandApplied::Rejected {
                kind: KIND_MIGRATION,
                message: MSG_MIGRATION_TARGET_GEOMETRY,
            };
        }
        let (Some(owner), Some(correlation)) = (
            OwnerId::parse(&report.owner),
            CorrelationId::parse(&report.request_id),
        ) else {
            return CommandApplied::Rejected {
                kind: KIND_SNAPSHOT,
                message: MSG_SNAPSHOT,
            };
        };
        let accepted = self
            .engine
            .session(&source_key)
            .map(|s| (s.accepted_revision(), s.accepted_fingerprint()));
        let (revision, fingerprint) = accepted.unwrap_or((0, 0));
        let source_staged = source_staged.expect("readable source staged");
        let staged_output = OutputDomain {
            id: source_key.output.clone(),
            workspace: source_key.workspace.clone(),
            bounds: source_staged.bounds,
            gap: source_staged.gap,
            adjacent: BTreeMap::new(),
        };
        let Some(inset_source) =
            crate::world_engine::inset_world_domain(&staged_output, source_staged.outer_gap)
        else {
            return CommandApplied::Rejected {
                kind: KIND_SCOPE_HELD,
                message: MSG_SCOPE_HELD,
            };
        };
        // Floating sources carry by sessionless core-owned membership
        // remap: the workspace holds no retained session (the sweep skips
        // floating scopes), so there is no tree to relocate and no fake
        // session is built. Members ride core membership plus staged
        // homing with stable identities (sticky stays physical, via the
        // shared collector), and the authoritative mapping/refill/
        // retained-transfer commit below runs unchanged. No focus or
        // frame writes either way.
        if self.floating_scopes.contains(&source_key) {
            let members = Self::swap_source_members(self, &source_key);
            let floats = members
                .iter()
                .filter(|window| {
                    self.staged
                        .facts
                        .get(*window)
                        .is_some_and(|fact| fact.floating)
                        || self
                            .members
                            .get(*window)
                            .is_some_and(|state| state.floating)
                })
                .count();
            return self.commit_migration(
                report,
                command_seq,
                source_output,
                target_output,
                workspace,
                direction_token,
                members,
                floats,
            );
        }
        let event = CoreEvent {
            owner,
            generation: self.core_generation.clone(),
            correlation,
            revision,
            fingerprint,
            domain: Engine::canonical_component_domain(&inset_source),
            domain_key: source_key.clone(),
            outer_gap: source_staged.outer_gap,
            focused_window: source_staged.focused.clone(),
            windows: self.shielded_windows(&source_key, &source_staged.windows),
            closed_windows: source_staged.closed.clone(),
            directional: None,
            directional_target_outer_gap: None,
            target_domain: Some((
                Engine::canonical_component_domain(&target_domain),
                target_key.clone(),
            )),
            target_windows: Vec::new(),
            command: CoreCommand::MigrateWorkspace {
                direction: direction_token.to_owned(),
            },
        };
        match self.engine.handle(&event) {
            CoreReply::MigrateWorkspace(plan) => {
                let _ = plan;
                // Full relocated member set from the committed target slot
                // (tiled links plus carried float exceptions) with stable
                // window identities, plus non-sticky minimized lifetimes
                // homed at the source: the adapter actuates each member
                // exactly once under the stable transfer identity below,
                // with truthful partial feedback.
                let mut members: Vec<WindowId> = self
                    .engine
                    .session(&target_key)
                    .map(|session| {
                        let mut members: Vec<WindowId> = session
                            .snapshot()
                            .windows
                            .into_iter()
                            .map(|link| link.window)
                            .collect();
                        for observed in session.exception_observed() {
                            if !members.iter().any(|w| w == &observed.window) {
                                members.push(observed.window.clone());
                            }
                        }
                        members
                    })
                    .unwrap_or_default();
                for (window, homed) in &self.staged.homing {
                    if homed == &source_key
                        && !members.iter().any(|member| member == window)
                        && self
                            .staged
                            .facts
                            .get(window)
                            .is_some_and(|fact| fact.minimized && !fact.sticky)
                    {
                        members.push(window.clone());
                    }
                }
                members.sort();
                let floats = self
                    .engine
                    .session(&target_key)
                    .map(|session| session.exception_count())
                    .unwrap_or_default();
                self.commit_migration(
                    report,
                    command_seq,
                    source_output,
                    target_output,
                    workspace,
                    direction_token,
                    members,
                    floats,
                )
            }
            CoreReply::Rejected { kind, message } => CommandApplied::Rejected { kind, message },
            CoreReply::SnapshotInvalid { message, detail } => CommandApplied::Rejected {
                kind: detail,
                message,
            },
            CoreReply::Diverged(reason) => CommandApplied::Rejected {
                kind: reason.as_str(),
                message: reason.message(),
            },
            _ => CommandApplied::Rejected {
                kind: KIND_MIGRATION,
                message: MSG_MIGRATION,
            },
        }
    }

    /// Authoritative migration commit shared by the tiled (engine-relocated)
    /// and floating (sessionless core-membership) paths: migration
    /// provenance, the logical-id ring move with its tiling mode, both
    /// view updates, source refill (or a fresh trailing), the retained
    /// refill-view plus migration transfers, and the single-owner
    /// membership rekey. No focus or frame writes: settlement proves
    /// arrival through member homing plus both views.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn commit_migration(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        source_output: &OutputId,
        target_output: &OutputId,
        workspace: &str,
        direction_token: &str,
        members: Vec<WindowId>,
        floats: usize,
    ) -> CommandApplied {
        let source_key = DomainKey {
            output: source_output.clone(),
            workspace: crate::directional::WorkspaceId(workspace.to_owned()),
        };
        let target_key = DomainKey {
            output: target_output.clone(),
            workspace: crate::directional::WorkspaceId(workspace.to_owned()),
        };
        self.command_result = Some(CommandResult::Migration {
            direction: match direction_token {
                "left" => Direction::Left,
                "right" => Direction::Right,
                "up" => Direction::Up,
                _ => Direction::Down,
            },
            source: source_key,
            target: target_key,
            members: members.clone(),
            floats,
        });
        // Authoritative ring move: the logical id changes screens,
        // landing after the target's current view (append when the
        // target view is unknown). Tiling mode follows the id.
        let source_index = self
            .mapping
            .get(&source_output.0)
            .and_then(|ring| ring.iter().position(|id| id == workspace));
        let prev_source_view = self.views.get(&source_output.0).cloned();
        let prev_target_view = self.views.get(&target_output.0).cloned();
        if let Some(ring) = self.mapping.get_mut(&source_output.0) {
            ring.retain(|id| id != workspace);
        }
        let insert_at = self
            .effective_view(target_output)
            .and_then(|view| {
                self.mapping
                    .get(&target_output.0)
                    .and_then(|ring| ring.iter().position(|id| id == &view))
                    .map(|pos| pos + 1)
            })
            .unwrap_or_else(|| {
                self.mapping
                    .get(&target_output.0)
                    .map(Vec::len)
                    .unwrap_or(0)
            });
        {
            let ring = self.mapping.entry(target_output.0.clone()).or_default();
            let at = insert_at.min(ring.len());
            ring.insert(at, workspace.to_owned());
        }
        self.carry_tiling(workspace, &source_output.0, &target_output.0);
        // Floating scope follows the id exactly like the tiling mode
        // above (mirroring the global-swap commit): without this the
        // commit reply publishes the floating target as a tiled scope
        // until the next report recomputes it, stalling the flight
        // mid-actuation and dropping arrived members from the rows.
        if self.floating_scopes.remove(&DomainKey {
            output: source_output.clone(),
            workspace: crate::directional::WorkspaceId(workspace.to_owned()),
        }) {
            self.floating_scopes.insert(DomainKey {
                output: target_output.clone(),
                workspace: crate::directional::WorkspaceId(workspace.to_owned()),
            });
        }
        self.views
            .insert(target_output.0.clone(), workspace.to_owned());
        // The emptied source shows the refill (or a fresh trailing
        // when nothing remains); views always name a ring member.
        let refill = self.logical_source_refill(report, source_output, workspace);
        if self
            .mapping
            .get(&source_output.0)
            .is_some_and(Vec::is_empty)
        {
            self.create_trailing_workspace(source_output, report.default_tiled);
        }
        let source_view = refill.clone().unwrap_or_else(|| {
            self.mapping
                .get(&source_output.0)
                .and_then(|ring| ring.first().cloned())
                .unwrap_or_default()
        });
        if !source_view.is_empty() {
            self.views.insert(source_output.0.clone(), source_view);
        }
        self.note_command(command_seq);
        // R-WS-12 source-refill actuation: retain the core-chosen
        // refill view on the source output so the adapter shows it
        // from the core result (retained views win in plan_outputs
        // and actuate through the shared view path). An emptied
        // source (no refill) shows its fresh trailing instead.
        if let Some(refill) = refill.clone() {
            self.retained.retain(|r| {
                !(matches!(&r.target.kind, TransferKind::View)
                    && r.target.target_output == *source_output)
            });
            self.retained.push(RetainedTransfer {
                target: TransferTarget {
                    operation_id: format!("op-{command_seq}-refill"),
                    kind: TransferKind::View,
                    target_output: source_output.clone(),
                    target_workspace: refill,
                    follow: false,
                    held: false,
                },
                policy: false,
                source_output: source_output.clone(),
                source_workspace: workspace.to_owned(),
                members: Vec::new(),
                migration_refill: None,
                migration_initial_empty: false,
                migration_source_index: None,
                migration_source_view: None,
                migration_target_view: None,
                migration_terminal: None,
                pending_member: None,
                pending_members: BTreeMap::new(),
                pending_view: None,
            });
        }
        let initial_empty = members.is_empty();
        self.retained.push(RetainedTransfer {
            target: TransferTarget {
                operation_id: format!("op-{command_seq}"),
                kind: TransferKind::Migration,
                target_output: target_output.clone(),
                target_workspace: workspace.to_owned(),
                follow: false,
                held: false,
            },
            policy: false,
            source_output: source_output.clone(),
            source_workspace: workspace.to_owned(),
            members,
            migration_refill: refill,
            migration_initial_empty: initial_empty,
            migration_source_index: source_index,
            migration_source_view: prev_source_view,
            migration_target_view: prev_target_view,
            migration_terminal: None,
            pending_member: None,
            pending_members: BTreeMap::new(),
            pending_view: None,
        });
        // Single-owner membership follows the logical id: every
        // member homed at the source rekeys to the target.
        self.rekey_members(workspace, &source_output.0, &target_output.0);
        CommandApplied::NoEffect
    }
}

pub(crate) enum CommandApplied {
    Effect(OneShotEffect),
    NoEffect,
    Rejected {
        kind: &'static str,
        message: &'static str,
    },
}
