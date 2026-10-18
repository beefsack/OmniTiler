//! World command execution (D2/D3): one fresh command per accepted report.
//!
//! Split from [`crate::world_engine`]: selection/send/migration commands,
//! the [`CoreCommand`] operation bridge (core-topology-derived directional
//! pairs, cross-output drag sources, directional migration, domain release),
//! and the shared reply settlement. Transition entry points stay on
//! [`crate::world_engine::WorldEngine`]; this module only extends its `impl`.

use std::collections::{BTreeMap, BTreeSet};

use crate::boundary::{CoreCommand, CoreEvent, CoreReply, TiledKind};
use crate::directional::{Direction, OutputId, SameAxisMove, WindowId};
use crate::engine::Engine;
use crate::ids::{CorrelationId, OwnerId};
use crate::seed::EngineWindow;
use crate::session::{DomainKey, OutputDomain};
use crate::workspace_scope::{
    MigrationRefill, MigrationRequest, PreviousTarget, TrailingSelection, WorkspaceMode,
    apply_migration, apply_swap_assignments, plan_migration, resolve_numbered,
    resolve_relative_send, resolve_relative_step, resolve_trailing,
};
use crate::world_engine::{
    KIND_CREATION_EXHAUSTED, KIND_DIRECTION, KIND_FOCUS, KIND_MIGRATION, KIND_OPERATION,
    KIND_SCOPE_HELD, KIND_SELECT_UNKNOWN, KIND_SEND_TARGET, KIND_SEND_WINDOW, KIND_SNAPSHOT,
    MSG_CREATION_EXHAUSTED, MSG_DIRECTION, MSG_FOCUS, MSG_MIGRATION, MSG_OPERATION, MSG_SCOPE_HELD,
    MSG_SELECT_UNKNOWN, MSG_SEND_TARGET, MSG_SEND_WINDOW, MSG_SNAPSHOT, RetainedTransfer,
    WorldEngine,
};
use crate::world_publish::{current_view, homed_occupancy, live_native_order};
use crate::world_report::CommandResult;
use crate::world_report::{OneShotEffect, TransferKind, TransferTarget, WorldCommand, WorldReport};

impl WorldEngine {
    /// Execute one fresh command after the sweep. Returns the emitted
    /// one-shot effect, if any. Every accepted command advances the
    /// monotonic command high-watermark; commandless reports never move it.
    pub(crate) fn execute_command(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        command: &WorldCommand,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
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
                by_id,
                held_scopes,
            ),
            WorldCommand::SwitchView { output, workspace } => {
                self.retain_view(report, command_seq, output, workspace)
            }
            WorldCommand::SelectWorkspace { output, workspace } => {
                self.retain_view(report, command_seq, output, workspace)
            }
            WorldCommand::SelectNumbered { output, index } => {
                let Some(target) = self
                    .resolve_selection_ring(report, output)
                    .and_then(|ring| resolve_numbered(&ring, *index))
                else {
                    return CommandApplied::Rejected {
                        kind: KIND_SELECT_UNKNOWN,
                        message: MSG_SELECT_UNKNOWN,
                    };
                };
                self.retain_view(report, command_seq, output, &target)
            }
            WorldCommand::SelectRelative { output, delta } => {
                let current = current_view(report, output).unwrap_or_default().to_owned();
                let Some(target) = self
                    .resolve_selection_ring(report, output)
                    .and_then(|ring| resolve_relative_step(&ring, &current, *delta))
                else {
                    return CommandApplied::Rejected {
                        kind: KIND_SELECT_UNKNOWN,
                        message: MSG_SELECT_UNKNOWN,
                    };
                };
                // Global-unique visible-elsewhere swap: exchange views
                // instead of duplicating the selection.
                if report.workspace_mode == WorkspaceMode::GlobalUnique
                    && let Some(plan) = self.global_swap_plan(report, output, &target)
                {
                    self.note_command(command_seq);
                    self.mapping = apply_swap_assignments(&self.mapping, &plan);
                    self.retained.retain(|r| {
                        !(matches!(r.target.kind, TransferKind::View)
                            && (r.target.target_output.0 == plan.active_output
                                || r.target.target_output.0 == plan.other_output))
                    });
                    self.retained.push(RetainedTransfer {
                        target: TransferTarget {
                            operation_id: format!("op-{command_seq}"),
                            kind: TransferKind::View,
                            target_output: OutputId(plan.active_output.clone()),
                            target_workspace: plan.target.clone(),
                            follow: false,
                            held: false,
                        },
                        policy: false,
                        source_output: OutputId(plan.other_output.clone()),
                        source_workspace: plan.active_current.clone(),
                    });
                    self.history.observe(
                        &plan.active_output,
                        &plan.target,
                        &self
                            .mapping
                            .get(&plan.active_output)
                            .cloned()
                            .unwrap_or_default(),
                    );
                    return CommandApplied::NoEffect;
                }
                self.retain_view(report, command_seq, output, &target)
            }
            WorldCommand::SelectPrevious { output } => {
                let live: Vec<String> = self.selection_ring(report, output);
                let current = current_view(report, output).unwrap_or_default().to_owned();
                match self
                    .history
                    .resolve_previous_for(&output.0, &live, &current)
                {
                    PreviousTarget::Select(target) => {
                        self.retain_view(report, command_seq, output, &target)
                    }
                    PreviousTarget::AlreadyThere(_) | PreviousTarget::NoHistory => {
                        // No-op until the next recorded change: reconcile
                        // only, never retain or emit.
                        self.note_command(command_seq);
                        CommandApplied::NoEffect
                    }
                }
            }
            WorldCommand::SelectTrailing { output } => {
                let ring = self.selection_ring(report, output);
                let occupied = homed_occupancy(report);
                match resolve_trailing(&ring, &occupied) {
                    TrailingSelection::Reuse(target) => {
                        self.retain_view(report, command_seq, output, &target)
                    }
                    TrailingSelection::Create => {
                        let Some(logical) = self.pending_token(output) else {
                            return CommandApplied::Rejected {
                                kind: KIND_CREATION_EXHAUSTED,
                                message: MSG_CREATION_EXHAUSTED,
                            };
                        };
                        self.retain_view(report, command_seq, output, &logical)
                    }
                }
            }
            WorldCommand::SendRelative {
                window,
                output,
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
                    output,
                    &target_workspace,
                    *follow,
                    by_id,
                    held_scopes,
                )
            }
            WorldCommand::SendNumbered {
                window,
                output,
                index,
                follow,
            } => self.start_numbered_send(
                report,
                command_seq,
                window,
                output,
                *index,
                *follow,
                by_id,
                held_scopes,
            ),
            WorldCommand::SetWorkspaceTiling {
                output,
                workspace,
                tiled,
            } => self.set_workspace_tiling(report, command_seq, output, workspace, *tiled),
            WorldCommand::MigrateWorkspace {
                source_output,
                target_output,
                workspace,
            } => self.start_migration(report, command_seq, source_output, target_output, workspace),
            WorldCommand::FocusDirectional {
                window,
                direction,
                float_subject,
            } => self.focus_directional(
                report,
                command_seq,
                window,
                direction,
                *float_subject,
                by_id,
                held_scopes,
            ),
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
                by_id,
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
                by_id,
                held_scopes,
            ),
            WorldCommand::MigrateDirectional {
                source_output,
                workspace,
                direction,
            } => self.migrate_directional(report, command_seq, source_output, workspace, direction),
            WorldCommand::Operation(command) => {
                self.execute_operation(report, command_seq, command, by_id, held_scopes)
            }
        }
    }
}

impl WorldEngine {
    /// Whole-workspace migration through the pure scope policy (D2)
    /// plus the retained Engine rekey (R-WS-12): refusal reasons stay in
    /// the fixed vocabulary with no native writes. The Engine owns the
    /// transactional tree/order/gaps/float-overlay relocation
    /// (`Engine::handle` with `MigrateWorkspace`, D8 carry included); this
    /// path only scopes, retains, and reports. Scope planning runs before
    /// the Engine commit so a scope refusal never leaves a committed rekey
    /// behind; an Engine refusal leaves scope, flights, and ordering
    /// exactly as before.
    pub(crate) fn start_migration(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        source_output: &OutputId,
        target_output: &OutputId,
        workspace: &str,
    ) -> CommandApplied {
        // Whole-workspace moves need a provably complete window inventory
        // to prove the target empty and the source fully observed; partial
        // inventory fences even when the monitor topology itself is
        // complete and retained. This preserves the established report
        // contract while live-topology retention stays independent of
        // unrelated window-read gaps.
        if !report.inventory_complete {
            return CommandApplied::Rejected {
                kind: KIND_MIGRATION,
                message: MSG_MIGRATION,
            };
        }
        let live: BTreeSet<String> = live_native_order(report).into_iter().collect();
        let target_current = report
            .topology
            .iter()
            .find(|topology| &topology.output == target_output)
            .map(|topology| topology.current_native.clone())
            .unwrap_or_default();
        let source_current = report
            .topology
            .iter()
            .find(|topology| &topology.output == source_output)
            .map(|topology| topology.current_native.clone());
        let request = MigrationRequest {
            mode: report.workspace_mode,
            live,
            source: source_output.0.clone(),
            target: target_output.0.clone(),
            workspace: workspace.to_owned(),
            target_current,
            source_current,
            previous_before: self.history.previous(&source_output.0),
            // Ingress R-WS-12 G-37 setting (main.xml `migrationSourceRefill`,
            // catalog `migration.refill`): the validated report value selects
            // the source refill for this migration only. Retained history and
            // already-committed state never reset on a setting change.
            refill: MigrationRefill::from(report.migration_source_refill),
        };
        let scope_plan = match plan_migration(&self.mapping, &request) {
            Ok(plan) => plan,
            Err(_) => {
                return CommandApplied::Rejected {
                    kind: KIND_MIGRATION,
                    message: MSG_MIGRATION,
                };
            }
        };
        // Retained Engine rekey through the transactional migration plan.
        // The direction derives from the retained core-owned FULL topology
        // (strict unique-reciprocal, no wrap), never the carried adapter
        // adjacency: the target keeps the source workspace id on the
        // target output with empty target membership (a different
        // current workspace there stays KDE-native, same-workspace residue
        // refuses as occupied below).
        let source_key = DomainKey {
            output: source_output.clone(),
            workspace: crate::directional::WorkspaceId(workspace.to_owned()),
        };
        let target_key = DomainKey {
            output: target_output.clone(),
            workspace: crate::directional::WorkspaceId(workspace.to_owned()),
        };
        let Some(source_report) = report
            .domains
            .iter()
            .find(|d| d.readable && d.domain.key() == source_key)
        else {
            return CommandApplied::Rejected {
                kind: KIND_MIGRATION,
                message: MSG_MIGRATION,
            };
        };
        if report
            .domains
            .iter()
            .any(|d| d.readable && d.domain.key() == target_key && !d.windows.is_empty())
        {
            return CommandApplied::Rejected {
                kind: KIND_MIGRATION,
                message: MSG_MIGRATION,
            };
        }
        let Some((direction, direction_token)) =
            self.reciprocal_direction_for(source_output, target_output)
        else {
            return CommandApplied::Rejected {
                kind: KIND_MIGRATION,
                message: MSG_MIGRATION,
            };
        };
        let _ = direction;
        // Target scope geometry: the target output's current readable view
        // when observed, else the source scope (same workspace, same gaps).
        // Inset by the chosen outer gap exactly like the legacy target
        // scope; exhaustion refuses with no mutation.
        let (raw_bounds, raw_gap, raw_outer) = report
            .domains
            .iter()
            .find(|d| {
                d.readable
                    && d.domain.id == *target_output
                    && report.topology.iter().any(|t| {
                        t.output == *target_output && t.current_native == d.domain.workspace.0
                    })
            })
            .map(|d| (d.domain.bounds, d.domain.gap, d.outer_gap))
            .unwrap_or((
                source_report.domain.bounds,
                source_report.domain.gap,
                source_report.outer_gap,
            ));
        let projected = match crate::geometry::inset_bounds(raw_bounds, raw_outer) {
            Ok(projected) => projected,
            Err(_) => {
                return CommandApplied::Rejected {
                    kind: KIND_MIGRATION,
                    message: MSG_MIGRATION,
                };
            }
        };
        let target_domain = OutputDomain {
            id: target_output.clone(),
            workspace: crate::directional::WorkspaceId(workspace.to_owned()),
            bounds: projected,
            gap: raw_gap,
            adjacent: BTreeMap::new(),
        };
        if !target_domain.validate() {
            return CommandApplied::Rejected {
                kind: KIND_MIGRATION,
                message: MSG_MIGRATION,
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
        let Some(inset_source) =
            crate::world_engine::inset_world_domain(&source_report.domain, source_report.outer_gap)
        else {
            return CommandApplied::Rejected {
                kind: KIND_SCOPE_HELD,
                message: MSG_SCOPE_HELD,
            };
        };
        let event = CoreEvent {
            owner,
            generation: self.core_generation.clone(),
            correlation,
            revision,
            fingerprint,
            domain: Engine::canonical_component_domain(&inset_source),
            domain_key: source_key.clone(),
            outer_gap: source_report.outer_gap,
            focused_window: source_report.focused.clone(),
            windows: self.shielded_windows(&source_key, &source_report.windows, &{
                let mut map: BTreeMap<&WindowId, &EngineWindow> = BTreeMap::new();
                for candidate in &report.domains {
                    for window in &candidate.windows {
                        map.insert(&window.window, window);
                    }
                }
                map
            }),
            closed_windows: source_report.closed.clone(),
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
                // Full relocated member set from the committed target slot
                // (tiled links plus carried float exceptions, excluding
                // stayed sticky) with stable window identities: the adapter
                // actuates each member exactly once under the stable
                // transfer identity below, with truthful partial feedback.
                let (members, floats) = self
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
                        members.sort();
                        (members, session.exception_count())
                    })
                    .unwrap_or_default();
                let _ = plan;
                self.command_result = Some(CommandResult::Migration {
                    direction: match direction_token {
                        "left" => Direction::Left,
                        "right" => Direction::Right,
                        "up" => Direction::Up,
                        _ => Direction::Down,
                    },
                    source: source_key,
                    target: target_key,
                    members,
                    floats,
                });
                self.note_command(command_seq);
                self.mapping = apply_migration(&self.mapping, &scope_plan);
                // R-WS-12 G-37 source-refill actuation: retain the
                // core-chosen refill view on the source output so the
                // adapter shows it from the core result (retained views win
                // in plan_outputs and actuate through the shared view path).
                // Never a TS selection. An emptied source (no refill) retains
                // nothing; ordinary trailing lifecycle converges later.
                if let Some(refill) = scope_plan.refill.clone() {
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
                    });
                }
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
                });
                CommandApplied::NoEffect
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

    /// Existing operation route through the full [`CoreEvent`] (D2/D3
    /// migration path for KDE): the homing (or first readable) domain
    /// supplies the observation envelope. Send operations retain a window
    /// transfer exactly like [`WorldCommand::Send`]; directional focus
    /// emits one at-most-once effect; read-only queries reconcile only.
    /// Cross-output move/focus carry the core-topology-derived directional
    /// pair (strict unique-reciprocal, never carried adjacency, never
    /// silently local); cross-output drag
    /// sources carry the source-first pair with combined rows; directional
    /// migration resolves through the world migration path; domain release
    /// drops the retained session. Anything unresolvable refuses without
    /// mutation, never silently degrades.
    pub(crate) fn execute_operation(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        command: &CoreCommand,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        match command {
            CoreCommand::SendToWorkspace {
                window,
                target_output,
                target_workspace,
                follow,
            }
            | CoreCommand::SendToOutput {
                window,
                target_output,
                target_workspace,
                follow,
            } => self.start_send(
                report,
                command_seq,
                &WindowId(window.clone()),
                &OutputId(target_output.clone()),
                target_workspace,
                *follow,
                by_id,
                held_scopes,
            ),
            CoreCommand::MigrateWorkspace { direction } => {
                self.operation_migration(report, command_seq, direction, by_id, held_scopes)
            }
            CoreCommand::ReleaseDomain => {
                self.operation_release(report, command_seq, command, by_id, held_scopes)
            }
            CoreCommand::Move {
                window,
                direction,
                cross_output_transfer,
                ..
            } if *cross_output_transfer => self.operation_directional(
                report,
                command_seq,
                command,
                window,
                direction,
                by_id,
                held_scopes,
            ),
            CoreCommand::Focus {
                window,
                direction,
                cross_output_transfer,
                ..
            } if *cross_output_transfer => self.operation_directional(
                report,
                command_seq,
                command,
                window,
                direction,
                by_id,
                held_scopes,
            ),
            CoreCommand::DragDrop { source, .. } | CoreCommand::DragPreview { source, .. }
                if source.is_some() =>
            {
                self.operation_drag(report, command_seq, command, by_id, held_scopes)
            }
            CoreCommand::Reconcile
            | CoreCommand::UpdateGaps
            | CoreCommand::Move { .. }
            | CoreCommand::Focus { .. }
            | CoreCommand::Resize { .. }
            | CoreCommand::PointerResize { .. }
            | CoreCommand::ToggleFloat { .. }
            | CoreCommand::ToggleOrientation { .. }
            | CoreCommand::DragDrop { .. }
            | CoreCommand::DragPreview { .. }
            | CoreCommand::ActiveGroup => {
                self.run_local_operation(report, command_seq, command, by_id, held_scopes)
            }
        }
    }

    /// Cross-output directional move/focus through the core-derived pair.
    /// Moves resolve the target through the windowed FULL selector
    /// (centre/span/left-top with reciprocal existence); focus keeps the
    /// strict unique-reciprocal map. The source is the window's homing
    /// domain; the target resolves to the same workspace preferred, else
    /// the target output's current view, else the first readable domain
    /// there. No resolvable target or no readable target refuses without
    /// mutation: a cross-output request never silently runs as a local
    /// move/focus. Unreadable topology refuses.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn operation_directional(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        command: &CoreCommand,
        window: &str,
        direction: &str,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        let window_id = WindowId(window.to_owned());
        let Some(source_row) = by_id.get(&window_id) else {
            return CommandApplied::Rejected {
                kind: KIND_OPERATION,
                message: MSG_OPERATION,
            };
        };
        let source_key = DomainKey {
            output: source_row.output.clone(),
            workspace: source_row.workspace.clone(),
        };
        if held_scopes.contains(&source_key) {
            return CommandApplied::Rejected {
                kind: KIND_SCOPE_HELD,
                message: MSG_SCOPE_HELD,
            };
        }
        let is_move = matches!(command, CoreCommand::Move { .. });
        let pair = if is_move {
            let Some(direction_parsed) = parse_direction_token(direction) else {
                return CommandApplied::Rejected {
                    kind: KIND_OPERATION,
                    message: MSG_OPERATION,
                };
            };
            match self.directional_pair_for_window(
                report,
                &source_key,
                direction_parsed,
                &source_row.rect,
                held_scopes,
            ) {
                Ok(pair) => pair,
                Err(_) => {
                    return CommandApplied::Rejected {
                        kind: KIND_OPERATION,
                        message: MSG_OPERATION,
                    };
                }
            }
        } else {
            self.directional_pair_for(report, &source_key, direction, held_scopes)
        };
        let Some(((source_domain, source_key), (target_domain, target_key), target_outer, _)) =
            pair
        else {
            return CommandApplied::Rejected {
                kind: KIND_OPERATION,
                message: MSG_OPERATION,
            };
        };
        let Some(source_report) = report
            .domains
            .iter()
            .find(|d| d.readable && d.domain.key() == source_key)
        else {
            return CommandApplied::Rejected {
                kind: KIND_OPERATION,
                message: MSG_OPERATION,
            };
        };
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
        // Pair convergence builds its observation from the combined
        // both-domain rows (source first), exactly like the legacy
        // directional route: source-only rows would starve the target side
        // and refuse as unchanged.
        let Some(target_report) = report
            .domains
            .iter()
            .find(|d| d.readable && d.domain.key() == target_key)
        else {
            return CommandApplied::Rejected {
                kind: KIND_OPERATION,
                message: MSG_OPERATION,
            };
        };
        let mut combined = self.shielded_windows(&source_key, &source_report.windows, by_id);
        combined.extend(self.shielded_windows(&target_key, &target_report.windows, by_id));
        let mut closed = source_report.closed.clone();
        closed.extend(target_report.closed.clone());
        let (Some(inset_source), Some(inset_target)) = (
            crate::world_engine::inset_world_domain(&source_domain, source_report.outer_gap),
            crate::world_engine::inset_world_domain(&target_domain, target_outer),
        ) else {
            return CommandApplied::Rejected {
                kind: KIND_SCOPE_HELD,
                message: MSG_SCOPE_HELD,
            };
        };
        let event = CoreEvent {
            owner,
            generation: self.core_generation.clone(),
            correlation,
            revision,
            fingerprint,
            domain: Engine::canonical_component_domain(&inset_source),
            domain_key: source_key.clone(),
            outer_gap: source_report.outer_gap,
            focused_window: source_report.focused.clone(),
            windows: combined,
            closed_windows: closed,
            directional: Some(vec![
                (inset_source, source_key.clone()),
                (inset_target, target_key),
            ]),
            directional_target_outer_gap: Some(target_outer),
            target_domain: None,
            target_windows: Vec::new(),
            command: command.clone(),
        };
        self.finish_operation_event(report, command_seq, &event)
    }

    /// Pinned cross-output directional move/focus for a dispatch-selected
    /// target: the same validated reciprocal pair as
    /// [`WorldEngine::operation_directional`] but the target output is the
    /// caller-pinned selection, never a re-rank of the mover frame. Fresh
    /// topology only validates forward/reverse touch (reverse uniqueness
    /// not required); the relocated mover frame is never consulted. `None`
    /// refuses without mutation.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn operation_directional_pinned(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        command: &CoreCommand,
        window: &str,
        direction: &str,
        pinned_target: &OutputId,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        let window_id = WindowId(window.to_owned());
        let Some(source_row) = by_id.get(&window_id) else {
            return CommandApplied::Rejected {
                kind: KIND_OPERATION,
                message: MSG_OPERATION,
            };
        };
        let source_key = DomainKey {
            output: source_row.output.clone(),
            workspace: source_row.workspace.clone(),
        };
        if held_scopes.contains(&source_key) {
            return CommandApplied::Rejected {
                kind: KIND_SCOPE_HELD,
                message: MSG_SCOPE_HELD,
            };
        }
        let Some(direction_parsed) = parse_direction_token(direction) else {
            return CommandApplied::Rejected {
                kind: KIND_OPERATION,
                message: MSG_OPERATION,
            };
        };
        let Some(source_report) = report
            .domains
            .iter()
            .find(|d| d.readable && d.domain.key() == source_key)
        else {
            return CommandApplied::Rejected {
                kind: KIND_OPERATION,
                message: MSG_OPERATION,
            };
        };
        let Some(((source_domain, source_key), (target_domain, target_key), target_outer, _)) =
            self.directional_pair_for_target(
                report,
                &source_key,
                source_report,
                direction_parsed,
                pinned_target,
                held_scopes,
            )
        else {
            return CommandApplied::Rejected {
                kind: KIND_OPERATION,
                message: MSG_OPERATION,
            };
        };
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
        let Some(target_report) = report
            .domains
            .iter()
            .find(|d| d.readable && d.domain.key() == target_key)
        else {
            return CommandApplied::Rejected {
                kind: KIND_OPERATION,
                message: MSG_OPERATION,
            };
        };
        let mut combined = self.shielded_windows(&source_key, &source_report.windows, by_id);
        combined.extend(self.shielded_windows(&target_key, &target_report.windows, by_id));
        let mut closed = source_report.closed.clone();
        closed.extend(target_report.closed.clone());
        let (Some(inset_source), Some(inset_target)) = (
            crate::world_engine::inset_world_domain(&source_domain, source_report.outer_gap),
            crate::world_engine::inset_world_domain(&target_domain, target_outer),
        ) else {
            return CommandApplied::Rejected {
                kind: KIND_SCOPE_HELD,
                message: MSG_SCOPE_HELD,
            };
        };
        let event = CoreEvent {
            owner,
            generation: self.core_generation.clone(),
            correlation,
            revision,
            fingerprint,
            domain: Engine::canonical_component_domain(&inset_source),
            domain_key: source_key.clone(),
            outer_gap: source_report.outer_gap,
            focused_window: source_report.focused.clone(),
            windows: combined,
            closed_windows: closed,
            directional: Some(vec![
                (inset_source, source_key.clone()),
                (inset_target, target_key),
            ]),
            directional_target_outer_gap: Some(target_outer),
            target_domain: None,
            target_windows: Vec::new(),
            command: command.clone(),
        };
        self.finish_operation_event(report, command_seq, &event)
    }

    /// Direction-only focus (core-owned topology): strict parity with the
    /// extracted KDE map. Left/right resolve the strict unique-reciprocal
    /// target from retained FULL rects and run the cross-output pair,
    /// returning its result directly including refusal (held/unreadable/
    /// inset-invalid targets refuse; no silent local fallback, preserving
    /// target/safety fences). Up/down and missing/ambiguous/partial/
    /// edgeless topology run the local single-domain focus. No adapter
    /// target or `crossOutput` is read; the core decides.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn focus_directional(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        window: &WindowId,
        direction: &str,
        float_subject: bool,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        let Some(direction_parsed) = parse_direction_token(direction) else {
            return CommandApplied::Rejected {
                kind: KIND_DIRECTION,
                message: MSG_DIRECTION,
            };
        };
        let Some(source_row) = by_id.get(window) else {
            return CommandApplied::Rejected {
                kind: KIND_FOCUS,
                message: MSG_FOCUS,
            };
        };
        let source_key = DomainKey {
            output: source_row.output.clone(),
            workspace: source_row.workspace.clone(),
        };
        if held_scopes.contains(&source_key) {
            return CommandApplied::Rejected {
                kind: KIND_SCOPE_HELD,
                message: MSG_SCOPE_HELD,
            };
        }
        // KDE focus policy: only left/right may cross. Up/down always run
        // local, so a vertical strict edge never diverts focus. When a
        // strict target exists, run the cross pair and return its result
        // directly (including refusal): old KDE crossForDirection was true
        // for strict adjacency irrespective of held/unreadable targets and
        // the cross operation refused. No silent relabel as local success.
        let cross_target = matches!(direction_parsed, Direction::Left | Direction::Right)
            .then(|| self.core_topology_target(&source_key.output, direction_parsed))
            .flatten();
        if cross_target.is_some() {
            let command = CoreCommand::Focus {
                window: window.0.clone(),
                direction: direction.to_owned(),
                cross_output_transfer: true,
                float_subject,
            };
            return self.operation_directional(
                report,
                command_seq,
                &command,
                &window.0,
                direction,
                by_id,
                held_scopes,
            );
        }
        let local_command = CoreCommand::Focus {
            window: window.0.clone(),
            direction: direction.to_owned(),
            cross_output_transfer: false,
            float_subject,
        };
        self.run_local_operation(report, command_seq, &local_command, by_id, held_scopes)
    }

    /// Direction-only move (core-owned topology): windowed selection. All
    /// four directions rank FULL edge-touch candidates by mover-centre
    /// projection, then span overlap, then left/top, with reciprocal edge
    /// existence (reverse uniqueness not required). A selected target runs
    /// the cross pair, returning its result directly including refusal.
    /// Unreadable topology refuses; no candidate (or missing reverse
    /// touch) runs local R1-R3. No adapter target or `crossOutput` is
    /// read; the core decides.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn move_directional(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        window: &WindowId,
        direction: &str,
        same_axis: SameAxisMove,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        let Some(direction_parsed) = parse_direction_token(direction) else {
            return CommandApplied::Rejected {
                kind: KIND_DIRECTION,
                message: MSG_DIRECTION,
            };
        };
        let Some(source_row) = by_id.get(window) else {
            return CommandApplied::Rejected {
                kind: KIND_OPERATION,
                message: MSG_OPERATION,
            };
        };
        let source_key = DomainKey {
            output: source_row.output.clone(),
            workspace: source_row.workspace.clone(),
        };
        if held_scopes.contains(&source_key) {
            return CommandApplied::Rejected {
                kind: KIND_SCOPE_HELD,
                message: MSG_SCOPE_HELD,
            };
        }
        // Windowed selection: unreadable refuses, no candidate runs local.
        // The dispatch-selected target is pinned through the validated
        // reciprocal pair (no re-ranking of any relocated mover within
        // this report): fresh topology only validates forward/reverse
        // touch, never re-ranks the mover frame.
        let pinned = match self.window_topology_target(
            &source_key.output,
            direction_parsed,
            &source_row.rect,
        ) {
            Err(_) => {
                return CommandApplied::Rejected {
                    kind: KIND_OPERATION,
                    message: MSG_OPERATION,
                };
            }
            Ok(None) => None,
            Ok(Some(target)) => Some(target),
        };
        if let Some(target) = pinned {
            let command = CoreCommand::Move {
                window: window.0.clone(),
                direction: direction.to_owned(),
                cross_output_transfer: true,
                same_axis_move: same_axis,
            };
            return self.operation_directional_pinned(
                report,
                command_seq,
                &command,
                &window.0,
                direction,
                &target,
                by_id,
                held_scopes,
            );
        }
        let local_command = CoreCommand::Move {
            window: window.0.clone(),
            direction: direction.to_owned(),
            cross_output_transfer: false,
            same_axis_move: same_axis,
        };
        self.run_local_operation(report, command_seq, &local_command, by_id, held_scopes)
    }

    /// Direction-only window send (core-owned topology): resolve the
    /// windowed target output plus its current native view, then run the
    /// cross-output send path. No adapter endpoint is read. Unreadable
    /// topology, no candidate, or missing reverse touch refuses without
    /// mutation.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn send_directional(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        window: &WindowId,
        direction: &str,
        follow: bool,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        let Some(direction_parsed) = parse_direction_token(direction) else {
            return CommandApplied::Rejected {
                kind: KIND_DIRECTION,
                message: MSG_DIRECTION,
            };
        };
        let Some(source_row) = by_id.get(window) else {
            return CommandApplied::Rejected {
                kind: KIND_SEND_WINDOW,
                message: MSG_SEND_WINDOW,
            };
        };
        let target_output = match self.window_topology_target(
            &source_row.output,
            direction_parsed,
            &source_row.rect,
        ) {
            Ok(Some(target)) => target,
            _ => {
                return CommandApplied::Rejected {
                    kind: KIND_SEND_TARGET,
                    message: MSG_SEND_TARGET,
                };
            }
        };
        let Some(current) = report
            .topology
            .iter()
            .find(|t| t.output == target_output)
            .map(|t| t.current_native.clone())
        else {
            return CommandApplied::Rejected {
                kind: KIND_SEND_TARGET,
                message: MSG_SEND_TARGET,
            };
        };
        if current.is_empty() {
            return CommandApplied::Rejected {
                kind: KIND_SEND_TARGET,
                message: MSG_SEND_TARGET,
            };
        }
        self.start_output_send(
            report,
            command_seq,
            window,
            &target_output,
            &current,
            follow,
            by_id,
            held_scopes,
        )
    }

    /// Direction-only whole-workspace migration (core-owned topology):
    /// resolve the largest-shared-edge target from the explicit source
    /// output, then run the world migration path. The adapter reports
    /// source identity only. Unreadable topology, no candidate, or missing
    /// reverse touch refuses without mutation; no wrap.
    pub(crate) fn migrate_directional(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        source_output: &OutputId,
        workspace: &str,
        direction: &str,
    ) -> CommandApplied {
        let Some(direction_parsed) = parse_direction_token(direction) else {
            return CommandApplied::Rejected {
                kind: KIND_DIRECTION,
                message: MSG_DIRECTION,
            };
        };
        let target_output = match self.migration_topology_target(source_output, direction_parsed) {
            Ok(Some(target)) => target,
            _ => {
                return CommandApplied::Rejected {
                    kind: KIND_MIGRATION,
                    message: MSG_MIGRATION,
                };
            }
        };
        self.start_migration(
            report,
            command_seq,
            source_output,
            &target_output,
            workspace,
        )
    }

    /// Cross-output drag preview/drop through the source-first pair. The
    /// pointer domain (resolved from the carried pointer, never the mover
    /// homing: native reassignment has not happened yet) is the drop
    /// target; `source` names the capture domain the mover leaves. The pair
    /// carries both validated domains with the combined shielded rows,
    /// exactly like the engine's pair transaction; pointer-inside-source
    /// keeps the legacy singleton route below.
    pub(crate) fn operation_drag(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        command: &CoreCommand,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        let (x, y, source) = match command {
            CoreCommand::DragDrop { x, y, source, .. }
            | CoreCommand::DragPreview { x, y, source, .. } => (*x, *y, source.clone()),
            _ => {
                return CommandApplied::Rejected {
                    kind: KIND_OPERATION,
                    message: MSG_OPERATION,
                };
            }
        };
        // Pointer-target destination first: the mover still homes the
        // source, so window homing can never resolve the destination.
        let pointer_found = match source {
            Some(_) => drag_pointer_domain(report, x, y, held_scopes),
            None => None,
        };
        if let Some((domain_key, domain_report)) = pointer_found {
            let Some(source_key) = source else {
                return CommandApplied::Rejected {
                    kind: KIND_OPERATION,
                    message: MSG_OPERATION,
                };
            };
            if source_key == domain_key {
                return self.run_local_operation(report, command_seq, command, by_id, held_scopes);
            }
            return self.operation_drag_pair(
                report,
                command_seq,
                command,
                &source_key,
                &domain_key,
                domain_report,
                by_id,
                held_scopes,
            );
        }
        // No pointer domain (source `None`, or pointer outside every
        // readable scope with a source): the legacy singleton route, which
        // refuses resolve-outside through the engine verdict below.
        let (domain_key, domain_report) =
            match operation_domain(report, command, by_id, held_scopes) {
                Some(found) => found,
                None => {
                    return CommandApplied::Rejected {
                        kind: KIND_OPERATION,
                        message: MSG_OPERATION,
                    };
                }
            };
        let Some(source_key) = source else {
            return self.run_local_operation(report, command_seq, command, by_id, held_scopes);
        };
        if source_key == domain_key {
            return self.run_local_operation(report, command_seq, command, by_id, held_scopes);
        }
        self.operation_drag_pair(
            report,
            command_seq,
            command,
            &source_key,
            &domain_key,
            domain_report,
            by_id,
            held_scopes,
        )
    }

    /// Source-first pair transaction for a validated cross-output drag: the
    /// pointer domain is the destination, `source_key` the capture domain
    /// the mover leaves. Combined shielded rows ride source-first, exactly
    /// like the engine's pair transaction.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn operation_drag_pair(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        command: &CoreCommand,
        source_key: &DomainKey,
        domain_key: &DomainKey,
        domain_report: &crate::world_report::DomainReport,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        let source_key = source_key.clone();
        let domain_key = domain_key.clone();
        let Some(source_report) = report
            .domains
            .iter()
            .find(|d| d.readable && d.domain.key() == source_key)
        else {
            return CommandApplied::Rejected {
                kind: KIND_SCOPE_HELD,
                message: MSG_SCOPE_HELD,
            };
        };
        if held_scopes.contains(&source_key) {
            return CommandApplied::Rejected {
                kind: KIND_SCOPE_HELD,
                message: MSG_SCOPE_HELD,
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
            .session(&domain_key)
            .map(|s| (s.accepted_revision(), s.accepted_fingerprint()));
        let (revision, fingerprint) = accepted.unwrap_or((0, 0));
        let mut combined = self.shielded_windows(&source_key, &source_report.windows, by_id);
        combined.extend(self.shielded_windows(&domain_key, &domain_report.windows, by_id));
        let (Some(inset_source), Some(inset_target)) = (
            crate::world_engine::inset_world_domain(&source_report.domain, source_report.outer_gap),
            crate::world_engine::inset_world_domain(&domain_report.domain, domain_report.outer_gap),
        ) else {
            return CommandApplied::Rejected {
                kind: KIND_SCOPE_HELD,
                message: MSG_SCOPE_HELD,
            };
        };
        let event = CoreEvent {
            owner,
            generation: self.core_generation.clone(),
            correlation,
            revision,
            fingerprint,
            domain: Engine::canonical_component_domain(&inset_target),
            domain_key: domain_key.clone(),
            outer_gap: domain_report.outer_gap,
            focused_window: domain_report.focused.clone(),
            windows: combined,
            closed_windows: domain_report.closed.clone(),
            directional: Some(vec![
                (inset_source, source_key),
                (inset_target, domain_key.clone()),
            ]),
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: Vec::new(),
            command: command.clone(),
        };
        match self.engine.handle(&event) {
            CoreReply::Tiled(plan) => {
                // Cross-output drop arrival needs native actuation: retain
                // one window transfer under the stable operation identity so
                // the adapter moves the member (and follows the views)
                // exactly once. Read-only previews never retain.
                self.command_result = Self::command_result_from_reply(
                    &event.command,
                    &CoreReply::Tiled(plan.clone()),
                );
                let is_drop = matches!(command, CoreCommand::DragDrop { .. });
                if is_drop && let CoreCommand::DragDrop { window, .. } = command {
                    let window_id = WindowId(window.clone());
                    let target_key = domain_key.clone();
                    if let Some(pair) = event.directional.as_ref().and_then(|pair| pair.first()) {
                        let source_key = pair.1.clone();
                        self.note_command(command_seq);
                        self.retained.push(RetainedTransfer {
                            target: TransferTarget {
                                operation_id: format!("op-{command_seq}"),
                                kind: TransferKind::Window(window_id),
                                target_output: target_key.output,
                                target_workspace: target_key.workspace.0,
                                follow: false,
                                held: false,
                            },
                            policy: false,
                            source_output: source_key.output,
                            source_workspace: source_key.workspace.0,
                        });
                        return CommandApplied::NoEffect;
                    }
                }
                self.note_command(command_seq);
                CommandApplied::NoEffect
            }
            CoreReply::DragPreview(plan) => {
                self.command_result =
                    Self::command_result_from_reply(&event.command, &CoreReply::DragPreview(plan));
                self.note_command(command_seq);
                CommandApplied::NoEffect
            }
            CoreReply::MigrateWorkspace(_) | CoreReply::Released => {
                self.note_command(command_seq);
                CommandApplied::NoEffect
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
                kind: KIND_OPERATION,
                message: MSG_OPERATION,
            },
        }
    }

    /// Directional whole-workspace migration through the world path: the
    /// operation domain supplies the source scope and the retained
    /// core-owned FULL topology supplies the largest-shared-edge target
    /// (reciprocal existence, never carried adjacency), reusing the same
    /// scope policy as the explicit migration command. Refuses without
    /// mutation when the direction has no resolvable target or the scope
    /// policy refuses.
    pub(crate) fn operation_migration(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        direction: &str,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        let (domain_key, _) =
            match operation_domain(report, &CoreCommand::ActiveGroup, by_id, held_scopes) {
                Some(found) => found,
                None => {
                    return CommandApplied::Rejected {
                        kind: KIND_OPERATION,
                        message: MSG_OPERATION,
                    };
                }
            };
        let Some(direction) = parse_direction_token(direction) else {
            return CommandApplied::Rejected {
                kind: KIND_OPERATION,
                message: MSG_OPERATION,
            };
        };
        let target_output = match self.migration_topology_target(&domain_key.output, direction) {
            Ok(Some(target)) => target,
            _ => {
                return CommandApplied::Rejected {
                    kind: KIND_OPERATION,
                    message: MSG_OPERATION,
                };
            }
        };
        self.start_migration(
            report,
            command_seq,
            &domain_key.output,
            &target_output,
            &domain_key.workspace.0,
        )
    }

    /// Domain release: drop the retained session for the operation domain.
    /// The adapter explicitly retired this scope; the next publish carries
    /// no retained rows for it. Never touches other sessions, flights, or
    /// ordering beyond consuming the command sequence.
    pub(crate) fn operation_release(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        command: &CoreCommand,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        let (domain_key, _) = match operation_domain(report, command, by_id, held_scopes) {
            Some(found) => found,
            None => {
                return CommandApplied::Rejected {
                    kind: KIND_OPERATION,
                    message: MSG_OPERATION,
                };
            }
        };
        self.engine.remove(&domain_key);
        self.note_command(command_seq);
        CommandApplied::NoEffect
    }

    /// Legacy local operation envelope: homing (or first readable) domain,
    /// no directional pair. Shared by local move/focus, read-only queries,
    /// resizes, float toggles, and same-domain drag preview/drop.
    pub(crate) fn run_local_operation(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        command: &CoreCommand,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        let (domain_key, domain_report) =
            match operation_domain(report, command, by_id, held_scopes) {
                Some(found) => found,
                None => {
                    return CommandApplied::Rejected {
                        kind: KIND_OPERATION,
                        message: MSG_OPERATION,
                    };
                }
            };
        let owner = report.owner.clone();
        let (Some(owner), Some(correlation)) = (
            OwnerId::parse(&owner),
            CorrelationId::parse(&report.request_id),
        ) else {
            return CommandApplied::Rejected {
                kind: KIND_SNAPSHOT,
                message: MSG_SNAPSHOT,
            };
        };
        let accepted = self
            .engine
            .session(&domain_key)
            .map(|s| (s.accepted_revision(), s.accepted_fingerprint()));
        let (revision, fingerprint) = accepted.unwrap_or((0, 0));
        let Some(inset) =
            crate::world_engine::inset_world_domain(&domain_report.domain, domain_report.outer_gap)
        else {
            return CommandApplied::Rejected {
                kind: KIND_SCOPE_HELD,
                message: MSG_SCOPE_HELD,
            };
        };
        let event = CoreEvent {
            owner,
            generation: self.core_generation.clone(),
            correlation,
            revision,
            fingerprint,
            domain: Engine::canonical_component_domain(&inset),
            domain_key: domain_key.clone(),
            outer_gap: domain_report.outer_gap,
            focused_window: domain_report.focused.clone(),
            windows: self.shielded_windows(&domain_key, &domain_report.windows, by_id),
            closed_windows: domain_report.closed.clone(),
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: Vec::new(),
            command: command.clone(),
        };
        self.finish_operation_event(report, command_seq, &event)
    }

    /// Correlated per-command outcome from one [`CoreReply`]: `Some` only for
    /// plans the snapshot cannot express (preview/float/group/migration).
    /// Move/resize/tiled-admit/focus/send geometry rides the snapshot,
    /// transfers, and effects, so those map to `None`: the snapshot IS the
    /// result, never a silent discard.
    pub(crate) fn command_result_from_reply(
        command: &CoreCommand,
        reply: &CoreReply,
    ) -> Option<CommandResult> {
        match reply {
            CoreReply::DragPreview(plan) => Some(CommandResult::DragPreview {
                rect: plan.preview.proposed_rect,
                hover: Some(plan.preview.hover_prior()),
                domain: plan.preview.domain.clone(),
                source: plan.preview.source_domain.clone(),
                target: plan.preview.target_window.clone(),
                revision: plan.preview.revision,
            }),
            CoreReply::Tiled(plan) if plan.kind == TiledKind::ToggleFloat => {
                // A capacity re-float (D5 explicit-tile no-space retry)
                // carries the float window with no rect: the window stays
                // floated and the adapter must record no tile override.
                let no_space = plan.float_window.is_some() && plan.float_rect.is_none();
                if plan.float_window.is_some() || plan.float_rect.is_some() {
                    let window = plan.float_window.clone().unwrap_or_else(|| match command {
                        CoreCommand::ToggleFloat { window, .. } => WindowId(window.clone()),
                        _ => WindowId(String::new()),
                    });
                    Some(CommandResult::Float {
                        window,
                        rect: plan.float_rect,
                        no_space,
                    })
                } else {
                    // Unfloat-to-tile names the command window with no rect:
                    // the tiled placement rides the snapshot.
                    match command {
                        CoreCommand::ToggleFloat { window, .. } => Some(CommandResult::Float {
                            window: WindowId(window.clone()),
                            rect: None,
                            no_space: false,
                        }),
                        _ => None,
                    }
                }
            }
            CoreReply::ActiveGroup(found) => Some(CommandResult::ActiveGroup {
                base_revision: found.base_revision,
                group: found.group.clone(),
                focused_leaf: found.focused_leaf.clone(),
                focused_window: found.focused_window.clone(),
                members: found.members.clone(),
                bounds: found.bounds,
            }),
            CoreReply::NoGroup {
                base_revision,
                reason,
            } => Some(CommandResult::NoGroup {
                base_revision: *base_revision,
                reason: *reason,
            }),
            CoreReply::MigrateWorkspace(plan) => Some(CommandResult::Migration {
                direction: plan.direction,
                source: plan.source.clone(),
                target: plan.target.clone(),
                members: plan.geometry.iter().map(|g| g.window.clone()).collect(),
                floats: plan.floats,
            }),
            _ => None,
        }
    }

    /// Shared reply settlement for operation events: stages the correlated
    /// command result (preview/float/group only; move/resize/tiled geometry
    /// rides the snapshot), at-most-once focus effects, sequence
    /// consumption on success, verbatim refusal passthrough. Never mutates
    /// on refusal.
    pub(crate) fn finish_operation_event(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        event: &CoreEvent,
    ) -> CommandApplied {
        let _ = report;
        let reply = self.engine.handle(event);
        self.command_result = Self::command_result_from_reply(&event.command, &reply);
        // Explicit float placement retains its target under the stable
        // command operation identity (`op-<cmdSeq>`) so a lost one-shot
        // result reply never loses the placement: later unrelated replies
        // still carry the row token until observation confirms it, proven
        // feedback settles it, or a newer intent replaces it. Unfloat
        // (no rect) withdraws any outstanding target for the window.
        // Refusals retain nothing. A no-space retry (still floated)
        // neither retains a new target nor withdraws: the window never
        // left float, so there is nothing to place or clear.
        if let Some(CommandResult::Float {
            window,
            rect,
            no_space,
        }) = &self.command_result
        {
            let window = window.clone();
            match rect {
                Some(target) => {
                    let source = event
                        .windows
                        .iter()
                        .find(|entry| entry.window == window)
                        .map(|entry| entry.rect);
                    let (output, workspace) = event
                        .windows
                        .iter()
                        .find(|entry| entry.window == window)
                        .map(|entry| (entry.output.clone(), entry.workspace.0.clone()))
                        .unwrap_or_else(|| {
                            (
                                event.domain_key.output.clone(),
                                event.domain_key.workspace.0.clone(),
                            )
                        });
                    self.retained_floats.retain(|held| held.window != window);
                    self.retained_floats
                        .push(crate::world_engine::RetainedFloat {
                            operation_id: format!("op-{command_seq}"),
                            window,
                            target: *target,
                            source,
                            output,
                            workspace,
                        });
                }
                None => {
                    if !no_space {
                        self.retained_floats.retain(|held| held.window != window);
                    }
                }
            }
        }
        match reply {
            CoreReply::Tiled(_)
            | CoreReply::Projection(_)
            | CoreReply::Resize(_)
            | CoreReply::ActiveGroup(_)
            | CoreReply::NoGroup { .. }
            | CoreReply::DragPreview(_) => {
                self.note_command(command_seq);
                CommandApplied::NoEffect
            }
            CoreReply::MoveDirectional(plan) => {
                // Exhausted R4 cross-output moves retain one identity-pinned
                // window transfer under the stable command identity so the
                // adapter relocates natively (output + membership, arrival
                // gate defers foreign geometry, focus stays on the mover).
                // Local R1-R3 plans (no cross) retain nothing: focus
                // unchanged, no transfer. Uses the existing send transfer
                // contract (Window kind, follow to the target view).
                if let Some(cross) = &plan.cross {
                    let window = cross.intent_window.clone();
                    let in_flight = self.retained.iter().any(|r| match &r.target.kind {
                        TransferKind::Window(w) => w == &window,
                        TransferKind::View | TransferKind::Migration => false,
                    });
                    if !in_flight {
                        self.retained.push(RetainedTransfer {
                            target: TransferTarget {
                                operation_id: format!("op-{command_seq}"),
                                kind: TransferKind::Window(window),
                                target_output: cross.target_output.clone(),
                                target_workspace: cross.target_workspace.0.clone(),
                                follow: true,
                                held: false,
                            },
                            policy: false,
                            source_output: cross.source_output.clone(),
                            source_workspace: cross.source_workspace.0.clone(),
                        });
                    }
                }
                self.note_command(command_seq);
                CommandApplied::NoEffect
            }
            CoreReply::FocusDirectional(plan) => {
                self.note_command(command_seq);
                CommandApplied::Effect(OneShotEffect::explicit_focus(
                    command_seq,
                    plan.to_window.clone(),
                    self.state_revision + 1,
                ))
            }
            CoreReply::SendWorkspace(plan) | CoreReply::SendOutput(plan) => {
                // Pair-route sends that surface here (R4 capacity-float
                // fallback under a Move command) retain exactly like an
                // explicit send: one identity-pinned window transfer.
                let (window, source_output, source_workspace, target_output, target_workspace) =
                    match &plan.operation {
                        crate::contract::LifecycleOperation::MoveTiled {
                            window,
                            source_output,
                            source_workspace,
                            target_output,
                            target_workspace,
                            ..
                        }
                        | crate::contract::LifecycleOperation::MoveDeferred {
                            window,
                            source_output,
                            source_workspace,
                            target_output,
                            target_workspace,
                        }
                        | crate::contract::LifecycleOperation::MoveToFloat {
                            window,
                            source_output,
                            source_workspace,
                            target_output,
                            target_workspace,
                            ..
                        } => (
                            window.clone(),
                            source_output.clone(),
                            source_workspace.clone(),
                            target_output.clone(),
                            target_workspace.clone(),
                        ),
                        _ => {
                            self.note_command(command_seq);
                            return CommandApplied::NoEffect;
                        }
                    };
                let in_flight = self.retained.iter().any(|r| match &r.target.kind {
                    TransferKind::Window(w) => w == &window,
                    TransferKind::View | TransferKind::Migration => false,
                });
                if !in_flight {
                    self.retained.push(RetainedTransfer {
                        target: TransferTarget {
                            operation_id: format!("op-{command_seq}"),
                            kind: TransferKind::Window(window),
                            target_output,
                            target_workspace: target_workspace.0.clone(),
                            follow: plan.follow,
                            held: false,
                        },
                        policy: false,
                        source_output,
                        source_workspace: source_workspace.0.clone(),
                    });
                }
                self.note_command(command_seq);
                CommandApplied::NoEffect
            }
            // Unreachable through the local subset (migration and
            // release take dedicated routes above): consume the
            // sequence without mutating further.
            CoreReply::MigrateWorkspace(_) | CoreReply::Released => {
                self.note_command(command_seq);
                CommandApplied::NoEffect
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
        }
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

/// Pointer-target domain for a drag preview/drop: the readable non-held
/// domain whose bounds contain the pointer. The mover still homes the
/// source (native reassignment has not happened), so homing the window
/// would resolve the source, never the destination. `None` refuses without
/// mutation (pointer outside every readable scope: resolve-outside).
pub(crate) fn drag_pointer_domain<'a>(
    report: &'a WorldReport,
    x: i32,
    y: i32,
    held_scopes: &BTreeSet<DomainKey>,
) -> Option<(DomainKey, &'a crate::world_report::DomainReport)> {
    report
        .domains
        .iter()
        .filter(|d| d.readable && !held_scopes.contains(&d.domain.key()))
        .find(|d| {
            let bounds = d.domain.bounds;
            let (x, y) = (i64::from(x), i64::from(y));
            let (left, top) = (i64::from(bounds.x), i64::from(bounds.y));
            x >= left && x < left + i64::from(bounds.w) && y >= top && y < top + i64::from(bounds.h)
        })
        .map(|d| (d.domain.key(), d))
}

/// Observation envelope for one operation command: the homing domain of
/// its subject window when the window names one, else the first readable
/// non-held domain. `None` selects the operation refusal without mutation.
pub(crate) fn operation_domain<'a>(
    report: &'a WorldReport,
    command: &CoreCommand,
    by_id: &BTreeMap<&WindowId, &EngineWindow>,
    held_scopes: &BTreeSet<DomainKey>,
) -> Option<(DomainKey, &'a crate::world_report::DomainReport)> {
    let window = match command {
        CoreCommand::Move { window, .. }
        | CoreCommand::Focus { window, .. }
        | CoreCommand::Resize { window, .. }
        | CoreCommand::PointerResize { window, .. }
        | CoreCommand::ToggleFloat { window, .. }
        | CoreCommand::ToggleOrientation { window, .. }
        | CoreCommand::DragDrop { window, .. }
        | CoreCommand::DragPreview { window, .. } => Some(WindowId(window.clone())),
        CoreCommand::Reconcile
        | CoreCommand::UpdateGaps
        | CoreCommand::ActiveGroup
        | CoreCommand::SendToWorkspace { .. }
        | CoreCommand::SendToOutput { .. }
        | CoreCommand::MigrateWorkspace { .. }
        | CoreCommand::ReleaseDomain => None,
    };
    if let Some(window) = window
        && let Some(row) = by_id.get(&window)
    {
        let key = DomainKey {
            output: row.output.clone(),
            workspace: row.workspace.clone(),
        };
        if !held_scopes.contains(&key)
            && let Some(domain) = report
                .domains
                .iter()
                .find(|d| d.readable && d.domain.key() == key)
        {
            return Some((key, domain));
        }
    }
    report
        .domains
        .iter()
        .filter(|d| d.readable && !held_scopes.contains(&d.domain.key()))
        .map(|d| (d.domain.key(), d))
        .next()
}

/// Parse a validated direction token to the engine axis. The report gate
/// already restricts the vocabulary; `None` refuses without mutation.
pub(crate) fn parse_direction_token(direction: &str) -> Option<Direction> {
    match direction {
        "left" => Some(Direction::Left),
        "right" => Some(Direction::Right),
        "up" => Some(Direction::Up),
        "down" => Some(Direction::Down),
        _ => None,
    }
}

/// Core-topology-derived directional pair for a cross-output move/focus:
/// the source domain plus the retained FULL-topology target output in the
/// requested direction (strict unique-reciprocal, never carried adapter
/// adjacency), resolved to the same workspace preferred, else the target
/// output's current view, else the first readable non-held domain there,
/// with the target's validated outer gap. The returned domains carry the
/// core-derived reciprocal pair adjacency (source names target, target
/// names source back), so the Engine's existing reciprocal R4/session
/// contracts validate a core derivation, not host facts. `None` refuses
/// without mutation: no/incomplete/stale topology, no resolvable target,
/// unreadable source, or no readable target.
type DirectionalPair = (
    (OutputDomain, DomainKey),
    (OutputDomain, DomainKey),
    i32,
    Direction,
);

impl WorldEngine {
    /// Strict unique-reciprocal target output for one source in one
    /// direction over the retained live FULL topology. `None` fences:
    /// cleared topology, gaps, or ambiguous edges.
    pub(crate) fn core_topology_target(
        &self,
        source: &OutputId,
        direction: Direction,
    ) -> Option<OutputId> {
        let entries = self.live_topology_entries();
        let map = crate::output_selection::unique_reciprocal_adjacency(&entries).ok()?;
        map.get(&(source.clone(), direction)).cloned()
    }

    /// Reciprocal edge existence between two outputs in one direction over
    /// the retained live FULL topology: the forward edge-touch candidate
    /// set contains the target AND the reverse set contains the source.
    /// Reverse uniqueness is NOT required (valid reverse ambiguity still
    /// pairs). `false` on any unreadable topology, unknown ids, or missing
    /// touch either way: callers fence without mutation.
    pub(crate) fn reciprocal_pair_exists(
        &self,
        source: &OutputId,
        target: &OutputId,
        direction: Direction,
    ) -> bool {
        use crate::output_selection::{OutputEntry, candidates_in_direction};
        let entries = self.live_topology_entries();
        let opposite = match direction {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
        };
        let (Some(source_entry), Some(target_entry)) = (
            entries.iter().find(|e| &e.id == source).cloned(),
            entries.iter().find(|e| &e.id == target).cloned(),
        ) else {
            return false;
        };
        let Ok(forward) = candidates_in_direction(&source_entry, direction, &entries) else {
            return false;
        };
        if !forward.iter().any(|c| &c.id == target) {
            return false;
        }
        let back_entry = OutputEntry {
            id: target_entry.id.clone(),
            rect: target_entry.rect,
        };
        let Ok(reverse) = candidates_in_direction(&back_entry, opposite, &entries) else {
            return false;
        };
        reverse.iter().any(|c| &c.id == source)
    }

    /// Reciprocal direction from one output to a named other output, with
    /// the Engine direction token. Forward plus reverse edge-touch must
    /// both hold; reverse uniqueness is not required. `None` fences.
    pub(crate) fn reciprocal_direction_for(
        &self,
        source: &OutputId,
        target: &OutputId,
    ) -> Option<(Direction, &'static str)> {
        const ORDER: [(Direction, &str); 4] = [
            (Direction::Left, "left"),
            (Direction::Right, "right"),
            (Direction::Up, "up"),
            (Direction::Down, "down"),
        ];
        ORDER
            .into_iter()
            .find(|(direction, _)| self.reciprocal_pair_exists(source, target, *direction))
    }

    /// Windowed directional target over retained FULL topology
    /// (2026-10-09: centre projection, then span overlap, then left/top).
    /// `Err` on any unreadable topology (refuse); `Ok(None)` when no
    /// candidate touches or the selected target lacks reverse edge-touch
    /// (caller-side local for moves, refusal for sends). No wrap.
    pub(crate) fn window_topology_target(
        &self,
        source: &OutputId,
        direction: Direction,
        window: &crate::geometry::Rect,
    ) -> Result<Option<OutputId>, crate::output_selection::SelectionError> {
        use crate::output_selection::select_for_window;
        let entries = self.live_topology_entries();
        let Some(source_entry) = entries.iter().find(|e| &e.id == source).cloned() else {
            return Err(crate::output_selection::SelectionError::UnreadableTopology);
        };
        // select_for_window validates every entry (including duplicates
        // and rect mismatches) and ranks centre/span/left-top.
        let selected = select_for_window(direction, &source_entry, window, &entries)?;
        let Some(candidate) = selected else {
            return Ok(None);
        };
        if !self.reciprocal_pair_exists(source, &candidate.id, direction) {
            return Ok(None);
        }
        Ok(Some(candidate.id))
    }

    /// Migration directional target over retained FULL topology
    /// (2026-10-09: largest shared edge, then left/top; never window
    /// position). `Err` on unreadable topology; `Ok(None)` on no
    /// candidate or missing reverse touch. No wrap.
    pub(crate) fn migration_topology_target(
        &self,
        source: &OutputId,
        direction: Direction,
    ) -> Result<Option<OutputId>, crate::output_selection::SelectionError> {
        use crate::output_selection::select_for_migration;
        let entries = self.live_topology_entries();
        let Some(source_entry) = entries.iter().find(|e| &e.id == source).cloned() else {
            return Err(crate::output_selection::SelectionError::UnreadableTopology);
        };
        let selected = select_for_migration(direction, &source_entry, &entries)?;
        let Some(candidate) = selected else {
            return Ok(None);
        };
        if !self.reciprocal_pair_exists(source, &candidate.id, direction) {
            return Ok(None);
        }
        Ok(Some(candidate.id))
    }

    pub(crate) fn directional_pair_for(
        &self,
        report: &WorldReport,
        source_key: &DomainKey,
        direction: &str,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Option<DirectionalPair> {
        let direction = parse_direction_token(direction)?;
        let source = report
            .domains
            .iter()
            .find(|d| d.readable && d.domain.key() == *source_key)?;
        if held_scopes.contains(source_key) {
            return None;
        }
        let target_output = self.core_topology_target(&source_key.output, direction)?;
        self.directional_pair_for_target(
            report,
            source_key,
            source,
            direction,
            &target_output,
            held_scopes,
        )
    }

    /// Validated reciprocal pair for an explicitly pinned target output
    /// (dispatch-selected via the windowed/migration selectors, or a named
    /// migration endpoint). Verifies the pinned pair still touches
    /// reciprocally in fresh topology without re-ranking any relocated
    /// mover: forward and reverse edge-touch must both hold, reverse
    /// uniqueness is not required. `None` refuses without mutation.
    pub(crate) fn directional_pair_for_target(
        &self,
        report: &WorldReport,
        source_key: &DomainKey,
        source: &crate::world_report::DomainReport,
        direction: Direction,
        target_output: &OutputId,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Option<DirectionalPair> {
        if !self.reciprocal_pair_exists(&source_key.output, target_output, direction) {
            return None;
        }
        let current = report
            .topology
            .iter()
            .find(|t| t.output == *target_output)
            .map(|t| t.current_native.clone());
        let candidates: Vec<&crate::world_report::DomainReport> = report
            .domains
            .iter()
            .filter(|d| {
                d.readable
                    && d.domain.id == *target_output
                    && !held_scopes.contains(&d.domain.key())
            })
            .collect();
        // Same workspace preferred (directional counterpart), else the target
        // output's current view, else the first readable domain there.
        let target = candidates
            .iter()
            .find(|d| d.domain.workspace.0 == source.domain.workspace.0)
            .or_else(|| {
                let wanted = current.as_deref()?;
                candidates.iter().find(|d| d.domain.workspace.0 == wanted)
            })
            .or_else(|| candidates.first())
            .copied()?;
        let opposite = match direction {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
        };
        let mut source_domain = source.domain.clone();
        let mut target_domain = target.domain.clone();
        source_domain.adjacent = BTreeMap::from([(direction, target_output.clone())]);
        target_domain.adjacent = BTreeMap::from([(opposite, source.domain.id.clone())]);
        Some((
            (source_domain, source_key.clone()),
            (target_domain, target.domain.key()),
            target.outer_gap,
            direction,
        ))
    }

    /// Windowed directional pair for exhausted moves and directional
    /// sends: selects via [`WorldEngine::window_topology_target`] (centre,
    /// span, left/top) then builds the validated reciprocal pair for the
    /// pinned target without re-ranking. `Err` on unreadable topology
    /// (refuse); `Ok(None)` on no candidate, missing reverse touch, or no
    /// readable target domain.
    pub(crate) fn directional_pair_for_window(
        &self,
        report: &WorldReport,
        source_key: &DomainKey,
        direction: Direction,
        window: &crate::geometry::Rect,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Result<Option<DirectionalPair>, crate::output_selection::SelectionError> {
        let Some(source) = report
            .domains
            .iter()
            .find(|d| d.readable && d.domain.key() == *source_key)
        else {
            return Ok(None);
        };
        if held_scopes.contains(source_key) {
            return Ok(None);
        }
        let selected = self.window_topology_target(&source_key.output, direction, window)?;
        let Some(target_output) = selected else {
            return Ok(None);
        };
        Ok(self.directional_pair_for_target(
            report,
            source_key,
            source,
            direction,
            &target_output,
            held_scopes,
        ))
    }
}
