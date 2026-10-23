//! World operation bridge (D2/D3): the operation route and its settlement.
//!
//! Split from [`crate::world_commands`]: same `impl WorldEngine` ownership,
//! only relocated for source size. Owns the operation route through the
//! full event, directional pairs and drag sources, and the shared reply
//! settlement. Transition entry points stay on the engine; this module
//! only extends its `impl`. No behavior change.

use std::collections::{BTreeMap, BTreeSet};

use crate::boundary::{CoreCommand, CoreEvent, CoreReply, TiledKind};
use crate::directional::{Direction, OutputId, SameAxisMove, WindowId};
use crate::engine::Engine;
use crate::ids::{CorrelationId, OwnerId};
use crate::seed::EngineWindow;
use crate::session::{DomainKey, OutputDomain};
use crate::world_engine::{
    KIND_DIRECTION, KIND_IN_FLIGHT, KIND_MIGRATION, KIND_OPERATION, KIND_SCOPE_HELD,
    KIND_SEND_TARGET, KIND_SEND_WINDOW, KIND_SNAPSHOT, MSG_DIRECTION, MSG_IN_FLIGHT,
    MSG_MIGRATION_TOPOLOGY, MSG_MIGRATION_TOPOLOGY_UNREADABLE, MSG_OPERATION, MSG_SCOPE_HELD,
    MSG_SEND_TARGET, MSG_SEND_WINDOW, MSG_SNAPSHOT, RetainedTransfer, WorldEngine,
};
use crate::world_report::{
    CommandResult, OneShotEffect, TransferKind, TransferTarget, WorldCommand, WorldReport,
};

use crate::world_commands::CommandApplied;

impl WorldEngine {
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
                None,
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
                None,
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
    /// (centre/span/left-top with reciprocal existence) unless the caller
    /// pins a dispatch-selected target, which validates against fresh
    /// topology without re-ranking the mover frame; focus keeps the
    /// strict unique-reciprocal map. The source is the window's homing
    /// domain; the target resolves to the same workspace preferred, else
    /// the target output's current view, else the first readable domain
    /// there. No resolvable target or no readable target refuses without
    /// mutation: a cross-output request never silently runs as a local
    /// move/focus. Unreadable topology refuses. A pending window transfer
    /// for the mover refuses as transfer-in-flight before any engine
    /// mutation, exactly like the explicit send path.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn operation_directional(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        command: &CoreCommand,
        window: &str,
        direction: &str,
        pinned_target: Option<&OutputId>,
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
        // Pending-transfer fence (same window, any origin): a newer move
        // never mutates under an outstanding flight. Observation lag keeps
        // the mover homed at the source, so without this the engine would
        // see a plausible second move; the fence refuses before any
        // convergence or commit, and the retained flight still settles on
        // arrival proof. Settled arrivals drop the flight before commands
        // run, so settled retries proceed.
        if matches!(command, CoreCommand::Move { .. })
            && self.retained.iter().any(|r| match &r.target.kind {
                TransferKind::Window(w) => w == &window_id,
                TransferKind::View | TransferKind::Migration => false,
            })
        {
            return CommandApplied::Rejected {
                kind: KIND_IN_FLIGHT,
                message: MSG_IN_FLIGHT,
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
            if let Some(pinned) = pinned_target {
                // Dispatch-pinned target: validate forward/reverse touch on
                // fresh topology (reverse uniqueness not required); the
                // relocated mover frame is never consulted.
                let Some(source_report) = self
                    .staged
                    .domains
                    .iter()
                    .find(|d| d.readable && d.key == source_key)
                else {
                    return CommandApplied::Rejected {
                        kind: KIND_OPERATION,
                        message: MSG_OPERATION,
                    };
                };
                self.directional_pair_for_target(
                    &source_key,
                    source_report,
                    direction_parsed,
                    pinned,
                    held_scopes,
                )
            } else {
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
            }
        } else {
            if pinned_target.is_some() {
                return CommandApplied::Rejected {
                    kind: KIND_OPERATION,
                    message: MSG_OPERATION,
                };
            }
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
        let Some(source_report) = self
            .staged
            .domains
            .iter()
            .find(|d| d.readable && d.key == source_key)
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
        let Some(target_report) = self
            .staged
            .domains
            .iter()
            .find(|d| d.readable && d.key == target_key)
        else {
            return CommandApplied::Rejected {
                kind: KIND_OPERATION,
                message: MSG_OPERATION,
            };
        };
        let mut combined = self.shielded_windows(&source_key, &source_report.windows);
        combined.extend(self.shielded_windows(&target_key, &target_report.windows));
        let mut closed = source_report.closed.clone();
        closed.extend(target_report.closed.clone());
        let source_output_domain = OutputDomain {
            id: source_key.output.clone(),
            workspace: source_key.workspace.clone(),
            bounds: source_report.bounds,
            gap: source_report.gap,
            adjacent: source_domain.adjacent.clone(),
        };
        let target_output_domain = OutputDomain {
            id: target_key.output.clone(),
            workspace: target_key.workspace.clone(),
            bounds: target_report.bounds,
            gap: target_report.gap,
            adjacent: target_domain.adjacent.clone(),
        };
        let (Some(inset_source), Some(inset_target)) = (
            crate::world_engine::inset_world_domain(&source_output_domain, source_report.outer_gap),
            crate::world_engine::inset_world_domain(&target_output_domain, target_outer),
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
    ///
    /// World-owned stage-5 lattice runs first: sticky subjects route in
    /// the displayed view, automatic floats and floating workspaces refuse,
    /// maximized subjects fence, and float origins resolve candidates and
    /// targets in core (miss retains, hit emits the explicit focus
    /// effect). Tiled subjects fall through to the pair/local path
    /// unchanged.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn focus_directional(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        window: &WindowId,
        direction: &str,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        use crate::world_directional::{
            DirectionalOutcome, DirectionalVerdict, early_verdict, effective_domain,
            float_focus_candidates, select_float_focus_target,
        };
        let Some(direction_parsed) = parse_direction_token(direction) else {
            return CommandApplied::Rejected {
                kind: KIND_DIRECTION,
                message: MSG_DIRECTION,
            };
        };
        let Some(source_row) = by_id.get(window) else {
            self.command_result = Some(CommandResult::Directional {
                outcome: DirectionalOutcome::FocusFloatRefusedStale,
                target: Some(window.clone()),
                rect: None,
                cause: Some("unknown-window".to_owned()),
            });
            self.note_command(command_seq);
            return CommandApplied::NoEffect;
        };
        let Some(home) = self.staged.homing.get(window).cloned() else {
            self.command_result = Some(CommandResult::Directional {
                outcome: DirectionalOutcome::FocusFloatRefusedStale,
                target: Some(window.clone()),
                rect: None,
                cause: Some("unknown-window".to_owned()),
            });
            self.note_command(command_seq);
            return CommandApplied::NoEffect;
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
        let view = self.effective_view(&home.output);
        let effective = effective_domain(&home, source_row.sticky, view.as_deref());
        let tiling = self.logical_tiling(&effective.output.0, &effective.workspace.0);
        if let Some(early) = early_verdict(source_row, tiling, false) {
            debug_assert!(!matches!(early, DirectionalVerdict::NeedClear { .. }));
            if let DirectionalVerdict::Refused { outcome, cause } = early {
                self.command_result = Some(CommandResult::Directional {
                    outcome,
                    target: None,
                    rect: None,
                    cause,
                });
                self.note_command(command_seq);
                return CommandApplied::NoEffect;
            }
        }
        if source_row.floating || source_row.sticky {
            let mut staged: Vec<EngineWindow> = Vec::new();
            for domain in &self.staged.domains {
                staged.extend(domain.windows.iter().cloned());
            }
            let candidates = float_focus_candidates(
                &staged,
                &self.staged.homing,
                window,
                &effective.output,
                &effective.workspace.0,
            );
            let Some(target) = select_float_focus_target(
                source_row.rect.x,
                source_row.rect.y,
                direction,
                &candidates,
            ) else {
                self.command_result = Some(CommandResult::Directional {
                    outcome: DirectionalOutcome::FocusFloatRetained,
                    target: None,
                    rect: None,
                    cause: Some("no-target".to_owned()),
                });
                self.note_command(command_seq);
                return CommandApplied::NoEffect;
            };
            self.command_result = Some(CommandResult::Directional {
                outcome: DirectionalOutcome::FocusFloatApplied,
                target: Some(target.clone()),
                rect: None,
                cause: None,
            });
            self.note_command(command_seq);
            return CommandApplied::Effect(OneShotEffect::explicit_focus(
                command_seq,
                target,
                self.state_revision + 1,
            ));
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
                float_subject: false,
            };
            return self.operation_directional(
                report,
                command_seq,
                &command,
                &window.0,
                direction,
                None,
                by_id,
                held_scopes,
            );
        }
        let local_command = CoreCommand::Focus {
            window: window.0.clone(),
            direction: direction.to_owned(),
            cross_output_transfer: false,
            float_subject: false,
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
    ///
    /// World-owned stage-5 lattice runs first with the same order as
    /// focus: automatic/floating-workspace/fullscreen refusals, then
    /// maximize need-clear (the adapter clears once and continues from
    /// fresh observation), then float half-snap authorization in core.
    /// Tiled subjects fall through to the pair/local path unchanged.
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
        use crate::world_directional::{
            DirectionalOutcome, DirectionalVerdict, early_verdict, effective_domain,
            float_half_snap_rect, half_snap_constraints_ok,
        };
        let Some(direction_parsed) = parse_direction_token(direction) else {
            return CommandApplied::Rejected {
                kind: KIND_DIRECTION,
                message: MSG_DIRECTION,
            };
        };
        let Some(source_row) = by_id.get(window) else {
            self.command_result = Some(CommandResult::Directional {
                outcome: DirectionalOutcome::MoveFloatRefusedStale,
                target: Some(window.clone()),
                rect: None,
                cause: Some("unknown-window".to_owned()),
            });
            self.note_command(command_seq);
            return CommandApplied::NoEffect;
        };
        let Some(home) = self.staged.homing.get(window).cloned() else {
            self.command_result = Some(CommandResult::Directional {
                outcome: DirectionalOutcome::MoveFloatRefusedStale,
                target: Some(window.clone()),
                rect: None,
                cause: Some("unknown-window".to_owned()),
            });
            self.note_command(command_seq);
            return CommandApplied::NoEffect;
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
        let view = self.effective_view(&home.output);
        let effective = effective_domain(&home, source_row.sticky, view.as_deref());
        let tiling = self.logical_tiling(&effective.output.0, &effective.workspace.0);
        match early_verdict(source_row, tiling, true) {
            Some(DirectionalVerdict::Refused { outcome, cause }) => {
                self.command_result = Some(CommandResult::Directional {
                    outcome,
                    target: None,
                    rect: None,
                    cause,
                });
                self.note_command(command_seq);
                return CommandApplied::NoEffect;
            }
            Some(DirectionalVerdict::NeedClear { window }) => {
                // Fresh need: bind the original intent under the stable
                // originating operation identity and publish a one-shot
                // `clear_operation` desire (see publish). The adapter
                // dispatches the native clear at most once and reports a
                // correlated `ClearAttempt`; the commandless settlement
                // resumes the original intent under this same identity on
                // proof. Never a second command or child id.
                let operation_id = format!("op-{command_seq}");
                self.retained_clears
                    .push(crate::world_engine::RetainedClear {
                        operation_id,
                        window: window.clone(),
                        direction: direction.to_owned(),
                        home: home.clone(),
                        view: view.clone(),
                        focus: report.focused_global.clone(),
                        float_route: source_row.floating || source_row.sticky,
                        same_axis,
                        pending: None,
                        ambiguous: false,
                    });
                self.command_result = Some(CommandResult::Directional {
                    outcome: DirectionalOutcome::NeedClear,
                    target: Some(window),
                    rect: None,
                    cause: None,
                });
                self.note_command(command_seq);
                return CommandApplied::NoEffect;
            }
            None => {}
        }
        if source_row.floating || source_row.sticky {
            // Float and sticky half-snaps share the uniform retained
            // float-placement lane: the stable `frame_operation` token is
            // the only write authority, so ordinary unsolicited sticky
            // frame writes stay prohibited elsewhere (the tiled lane never
            // writes sticky frames).
            let Some(screen) = report.screens.iter().find(|s| s.output == effective.output) else {
                self.command_result = Some(CommandResult::Directional {
                    outcome: DirectionalOutcome::MoveFloatRefusedUnknownOutput,
                    target: None,
                    rect: None,
                    cause: None,
                });
                self.note_command(command_seq);
                return CommandApplied::NoEffect;
            };
            let (Some(work), gap) = (screen.work_area, screen.gap) else {
                self.command_result = Some(CommandResult::Directional {
                    outcome: DirectionalOutcome::MoveFloatRefusedUnknownOutput,
                    target: None,
                    rect: None,
                    cause: None,
                });
                self.note_command(command_seq);
                return CommandApplied::NoEffect;
            };
            let Some(half) = float_half_snap_rect(work, gap, direction) else {
                self.command_result = Some(CommandResult::Directional {
                    outcome: DirectionalOutcome::MoveFloatRefusedInvalidGeometry,
                    target: None,
                    rect: None,
                    cause: None,
                });
                self.note_command(command_seq);
                return CommandApplied::NoEffect;
            };
            if !half_snap_constraints_ok(half, source_row.hints) {
                self.command_result = Some(CommandResult::Directional {
                    outcome: DirectionalOutcome::MoveFloatRefusedConstraints,
                    target: None,
                    rect: None,
                    cause: None,
                });
                self.note_command(command_seq);
                return CommandApplied::NoEffect;
            }
            self.command_result = Some(CommandResult::Directional {
                outcome: DirectionalOutcome::MoveFloatApplied,
                target: Some(window.clone()),
                rect: Some(half),
                cause: None,
            });
            self.note_command(command_seq);
            // Retain the explicit one-shot placement under the stable
            // command identity so the uniform float lane (not a bespoke
            // adapter write) actuates it at most once: the reply rect names
            // the authorized desire, and only native observation confirms it
            // (proven applied). One shot per command, never a continuous
            // fight: settlement drops or supersedes the target like any
            // explicit float placement.
            let operation_id = format!("op-{command_seq}");
            self.retained_floats.retain(|held| held.window != *window);
            self.retained_floats
                .push(crate::world_engine::RetainedFloat {
                    operation_id,
                    window: window.clone(),
                    target: half,
                    source: Some(source_row.rect),
                    output: effective.output.clone(),
                    workspace: effective.workspace.0.clone(),
                    ambiguous: false,
                    pending: None,
                });
            return CommandApplied::NoEffect;
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
        // Tiled directional moves keep focus on the mover through one spent
        // core effect (same at-most-once lane as directional focus): the
        // adapter only verifies the foreground for its snap chain and never
        // actuates a second time. Refusals pass through untouched.
        let applied = if let Some(target) = pinned {
            let command = CoreCommand::Move {
                window: window.0.clone(),
                direction: direction.to_owned(),
                cross_output_transfer: true,
                same_axis_move: same_axis,
            };
            self.operation_directional(
                report,
                command_seq,
                &command,
                &window.0,
                direction,
                Some(&target),
                by_id,
                held_scopes,
            )
        } else {
            let local_command = CoreCommand::Move {
                window: window.0.clone(),
                direction: direction.to_owned(),
                cross_output_transfer: false,
                same_axis_move: same_axis,
            };
            self.run_local_operation(report, command_seq, &local_command, by_id, held_scopes)
        };
        match applied {
            CommandApplied::NoEffect => CommandApplied::Effect(OneShotEffect::explicit_focus(
                command_seq,
                window.clone(),
                self.state_revision + 1,
            )),
            CommandApplied::Effect(_) | CommandApplied::Rejected { .. } => applied,
        }
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
        let Some(current) = self.effective_view(&target_output) else {
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
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn migrate_directional(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        source_output: &OutputId,
        workspace: &str,
        direction: &str,
        _by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        let Some(direction_parsed) = parse_direction_token(direction) else {
            return CommandApplied::Rejected {
                kind: KIND_DIRECTION,
                message: MSG_DIRECTION,
            };
        };
        let target_output = match self.migration_topology_target(source_output, direction_parsed) {
            Ok(Some(target)) => target,
            Err(_) => {
                return CommandApplied::Rejected {
                    kind: KIND_MIGRATION,
                    message: MSG_MIGRATION_TOPOLOGY_UNREADABLE,
                };
            }
            Ok(None) => {
                return CommandApplied::Rejected {
                    kind: KIND_MIGRATION,
                    message: MSG_MIGRATION_TOPOLOGY,
                };
            }
        };
        self.start_migration(
            report,
            command_seq,
            source_output,
            &target_output,
            workspace,
            held_scopes,
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
            Some(_) => self.drag_pointer_domain_staged(x, y, held_scopes),
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
                &domain_report,
                by_id,
                held_scopes,
            );
        }
        // No pointer domain (source `None`, or pointer outside every
        // readable scope with a source): the legacy singleton route, which
        // refuses resolve-outside through the engine verdict below.
        let (domain_key, domain_report) = match self.operation_domain_staged(command, held_scopes) {
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
            &domain_report,
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
        domain_report: &crate::world_engine::StagedDomain,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        let _ = by_id;
        let source_key = source_key.clone();
        let domain_key = domain_key.clone();
        let Some(source_report) = self
            .staged
            .domains
            .iter()
            .find(|d| d.readable && d.key == source_key)
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
        let mut combined = self.shielded_windows(&source_key, &source_report.windows);
        combined.extend(self.shielded_windows(&domain_key, &domain_report.windows));
        let source_output_domain = OutputDomain {
            id: source_key.output.clone(),
            workspace: source_key.workspace.clone(),
            bounds: source_report.bounds,
            gap: source_report.gap,
            adjacent: BTreeMap::new(),
        };
        let domain_output_domain = OutputDomain {
            id: domain_key.output.clone(),
            workspace: domain_key.workspace.clone(),
            bounds: domain_report.bounds,
            gap: domain_report.gap,
            adjacent: BTreeMap::new(),
        };
        let (Some(inset_source), Some(inset_target)) = (
            crate::world_engine::inset_world_domain(&source_output_domain, source_report.outer_gap),
            crate::world_engine::inset_world_domain(&domain_output_domain, domain_report.outer_gap),
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
        _by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        let (domain_key, _) =
            match self.operation_domain_staged(&CoreCommand::ActiveGroup, held_scopes) {
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
            held_scopes,
        )
    }

    /// Domain release: drop the retained session for the operation domain.
    /// The adapter explicitly retired this scope; the next publish carries
    /// no retained rows for it. Never touches other sessions, flights, or
    /// ordering beyond consuming the command sequence.
    pub(crate) fn operation_release(
        &mut self,
        _report: &WorldReport,
        command_seq: u64,
        command: &CoreCommand,
        _by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        let (domain_key, _) = match self.operation_domain_staged(command, held_scopes) {
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
    /// resizes, float toggles, and same-domain drag preview/drop. A pending
    /// window transfer for a move subject refuses as transfer-in-flight
    /// before any engine mutation, exactly like the cross-output pair and
    /// the explicit send path (covers the no-candidate local fallback when
    /// topology turns to a gap under a flight).
    pub(crate) fn run_local_operation(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        command: &CoreCommand,
        _by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        if let CoreCommand::Move { window, .. } = command {
            let window_id = WindowId(window.clone());
            if self.retained.iter().any(|r| match &r.target.kind {
                TransferKind::Window(w) => w == &window_id,
                TransferKind::View | TransferKind::Migration => false,
            }) {
                return CommandApplied::Rejected {
                    kind: KIND_IN_FLIGHT,
                    message: MSG_IN_FLIGHT,
                };
            }
        }
        let (domain_key, domain_report) = match self.operation_domain_staged(command, held_scopes) {
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
        let staged_output = OutputDomain {
            id: domain_key.output.clone(),
            workspace: domain_key.workspace.clone(),
            bounds: domain_report.bounds,
            gap: domain_report.gap,
            adjacent: BTreeMap::new(),
        };
        let Some(inset) =
            crate::world_engine::inset_world_domain(&staged_output, domain_report.outer_gap)
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
            windows: self.shielded_windows(&domain_key, &domain_report.windows),
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
        // still carry the row token until native observation confirms it,
        // operation-correlated raw write facts prove refusal, or a newer
        // intent replaces it. Unfloat (no rect) withdraws any outstanding
        // target for the window. Refusals retain nothing. A no-space retry
        // (still floated) neither retains a new target nor withdraws: the
        // window never left float, so there is nothing to place or clear.
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
                            ambiguous: false,
                            pending: None,
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
                // contract (Window kind, follow to the target view). The
                // same-window pending-transfer fence lives pre-engine in
                // the pair/local routes; the in-flight skip below is only
                // a no-double-retain backstop.
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
                // explicit send: one identity-pinned window transfer. The
                // same-window pending-transfer fence lives pre-engine in
                // the pair/local routes; the in-flight skip below is only
                // a no-double-retain backstop.
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

    /// Pointer-target staged domain for a drag preview/drop: the readable
    /// non-held staged domain whose work area contains the pointer. The
    /// mover still homes the source, so homing the window would resolve
    /// the source, never the destination. `None` refuses without mutation
    /// (pointer outside every readable scope: resolve-outside).
    pub(crate) fn drag_pointer_domain_staged(
        &self,
        x: i32,
        y: i32,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Option<(DomainKey, crate::world_engine::StagedDomain)> {
        self.staged
            .domains
            .iter()
            .filter(|d| d.readable && !held_scopes.contains(&d.key))
            .find(|d| {
                let bounds = d.bounds;
                let (x, y) = (i64::from(x), i64::from(y));
                let (left, top) = (i64::from(bounds.x), i64::from(bounds.y));
                x >= left
                    && x < left + i64::from(bounds.w)
                    && y >= top
                    && y < top + i64::from(bounds.h)
            })
            .map(|d| (d.key.clone(), d.clone()))
    }

    /// Observation envelope for one operation command: the staged domain
    /// of its subject window when the window homes to one, else the first
    /// readable non-held staged domain. `None` selects the operation
    /// refusal without mutation.
    pub(crate) fn operation_domain_staged(
        &self,
        command: &CoreCommand,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Option<(DomainKey, crate::world_engine::StagedDomain)> {
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
            && let Some(key) = self.staged_homing(&window)
            && !held_scopes.contains(&key)
            && let Some(domain) = self
                .staged
                .domains
                .iter()
                .find(|d| d.readable && d.key == key)
        {
            return Some((key, domain.clone()));
        }
        self.staged
            .domains
            .iter()
            .filter(|d| d.readable && !held_scopes.contains(&d.key))
            .map(|d| (d.key.clone(), d.clone()))
            .next()
    }
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

/// The window lifetime a world command names, if any. Used only to
/// supersede a pending directional maximize-clear binding: any same-window
/// new intent drops the old binding and re-evaluates fresh, never
/// consuming it.
pub(crate) fn command_window(command: &WorldCommand) -> Option<WindowId> {
    match command {
        WorldCommand::Focus { window, .. }
        | WorldCommand::FocusDirectional { window, .. }
        | WorldCommand::MoveDirectional { window, .. }
        | WorldCommand::SendDirectional { window, .. }
        | WorldCommand::Send { window, .. }
        | WorldCommand::SendRelative { window, .. }
        | WorldCommand::SendNumbered { window, .. } => Some(window.clone()),
        WorldCommand::Operation(command) => match command {
            CoreCommand::Move { window, .. }
            | CoreCommand::Focus { window, .. }
            | CoreCommand::Resize { window, .. }
            | CoreCommand::PointerResize { window, .. }
            | CoreCommand::ToggleFloat { window, .. }
            | CoreCommand::ToggleOrientation { window, .. }
            | CoreCommand::DragDrop { window, .. }
            | CoreCommand::DragPreview { window, .. } => Some(WindowId(window.clone())),
            CoreCommand::SendToWorkspace { window, .. }
            | CoreCommand::SendToOutput { window, .. } => Some(WindowId(window.clone())),
            CoreCommand::Reconcile
            | CoreCommand::UpdateGaps
            | CoreCommand::ActiveGroup
            | CoreCommand::ReleaseDomain
            | CoreCommand::MigrateWorkspace { .. } => None,
        },
        WorldCommand::SwitchView { .. }
        | WorldCommand::SelectWorkspace { .. }
        | WorldCommand::SelectNumbered { .. }
        | WorldCommand::SelectRelative { .. }
        | WorldCommand::SelectPrevious { .. }
        | WorldCommand::SelectTrailing { .. }
        | WorldCommand::MigrateWorkspace { .. }
        | WorldCommand::MigrateDirectional { .. }
        | WorldCommand::SetWorkspaceTiling { .. } => None,
    }
}
