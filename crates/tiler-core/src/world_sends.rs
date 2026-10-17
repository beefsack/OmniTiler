//! Window send commands (D2/D3): the shared send path for direct,
//! relative, and numbered sends, including floating-workspace boundary
//! transfers and the stable command high-watermark.
//!
//! Split from [`crate::world_commands`]: this module only extends the
//! [`crate::world_engine::WorldEngine`] `impl`.

use std::collections::{BTreeMap, BTreeSet};

use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
use crate::directional::{OutputId, WindowId};
use crate::engine::Engine;
use crate::ids::{CorrelationId, OwnerId};
use crate::seed::EngineWindow;
use crate::session::DomainKey;
use crate::workspace_scope::{TrailingSelection, resolve_numbered, resolve_trailing};
use crate::world_commands::CommandApplied;
use crate::world_engine::{
    KIND_CREATION_EXHAUSTED, KIND_IN_FLIGHT, KIND_SCOPE_HELD, KIND_SEND_REFUSED, KIND_SEND_TARGET,
    KIND_SEND_WINDOW, KIND_SNAPSHOT, MSG_CREATION_EXHAUSTED, MSG_IN_FLIGHT, MSG_SCOPE_HELD,
    MSG_SEND_REFUSED, MSG_SEND_TARGET, MSG_SEND_WINDOW, MSG_SNAPSHOT, PendingSend,
    RetainedTransfer, WorldEngine,
};
use crate::world_publish::homed_occupancy;
use crate::world_report::{TransferKind, TransferTarget, WorldReport};

impl WorldEngine {
    /// Record one consumed command sequence on the monotonic high-watermark.
    /// Never evicts: any sequence at or below the mark stays consumed.
    pub(crate) fn note_command(&mut self, command_seq: u64) {
        self.command_highwater = Some(
            self.command_highwater
                .map_or(command_seq, |mark| mark.max(command_seq)),
        );
    }
    /// Shared window-send path for [`WorldCommand::Send`],
    /// [`WorldCommand::SendRelative`], [`WorldCommand::SendNumbered`], and
    /// send operations. Consumes one command sequence on success.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start_send(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        window: &WindowId,
        target_output: &OutputId,
        target_workspace: &str,
        follow: bool,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        let operation_id = format!("op-{command_seq}");
        let result = self.execute_send(
            report,
            &operation_id,
            window,
            target_output,
            target_workspace,
            follow,
            by_id,
            held_scopes,
        );
        self.settle_send(command_seq, operation_id, result)
    }

    /// Settle one executed send: retain the transfer under its stable
    /// operation identity on success (consuming the command sequence), or
    /// refuse without mutation. Never retains a view target: sends never
    /// view-switch, including `follow=false` stays.
    pub(crate) fn settle_send(
        &mut self,
        command_seq: u64,
        operation_id: String,
        result: Result<RetainedTransfer, (&'static str, &'static str)>,
    ) -> CommandApplied {
        match result {
            Ok(retained) => {
                debug_assert_eq!(retained.target.operation_id, operation_id);
                self.note_command(command_seq);
                self.retained.push(retained);
                CommandApplied::NoEffect
            }
            Err((kind, message)) => CommandApplied::Rejected { kind, message },
        }
    }

    /// Ordinary tiled sends commit pair convergence; D16 ordinary/combo float sends
    /// commit the slotless deferred path with membership only (no frame);
    /// sends touching a floating workspace (either endpoint, including
    /// both) retain a membership-only transfer with no two-domain Engine
    /// plan while the tiled side reconciles through the sweep. Sticky and
    /// unexplained fit-excluded windows (game/overlay without a
    /// floating/fullscreen/maximized origin) have no automatic permission
    /// and the engine/world refusal is propagated. Success builds (but does
    /// not retain) one window transfer.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn execute_send(
        &mut self,
        report: &WorldReport,
        operation_id: &str,
        window: &WindowId,
        target_output: &OutputId,
        target_workspace: &str,
        follow: bool,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Result<RetainedTransfer, (&'static str, &'static str)> {
        if self.retained.iter().any(|r| match &r.target.kind {
            TransferKind::Window(w) => w == window,
            TransferKind::View | TransferKind::Migration => false,
        }) {
            return Err((KIND_IN_FLIGHT, MSG_IN_FLIGHT));
        }
        let Some(source_row) = by_id.get(window) else {
            return Err((KIND_SEND_WINDOW, MSG_SEND_WINDOW));
        };
        // No automatic permission for sticky windows and unexplained
        // fit-excluded rows (game/overlay without a floating, fullscreen,
        // or maximized origin): only tiled members, D16-eligible floats,
        // and approved combos pass, and the engine decides their
        // admission (deferred slotless carry).
        if source_row.sticky
            || (source_row.fit_excluded
                && !source_row.floating
                && !source_row.fullscreen
                && !source_row.maximized)
        {
            return Err((KIND_SEND_REFUSED, MSG_SEND_REFUSED));
        }
        let source_key = DomainKey {
            output: source_row.output.clone(),
            workspace: source_row.workspace.clone(),
        };
        let target_key = DomainKey {
            output: target_output.clone(),
            workspace: crate::directional::WorkspaceId(target_workspace.to_owned()),
        };
        if target_key == source_key {
            return Err((KIND_SEND_TARGET, MSG_SEND_TARGET));
        }
        if held_scopes.contains(&source_key) || held_scopes.contains(&target_key) {
            return Err((KIND_SCOPE_HELD, MSG_SCOPE_HELD));
        }
        // Floating-workspace sends (either endpoint floating, including
        // both): transfer project membership with no two-domain Engine
        // plan, mirroring the legacy native-only path (`!sourceTiled ||
        // !targetTiled` in the plan adapter entry). Shielding presents
        // the mover at the target so the tiled side reconciles through
        // its sweep; floating-side frames stay untouched (explicit
        // untouched rows, never geometry).
        let source_floating = self.floating_scopes.contains(&source_key);
        let target_floating = self.floating_scopes.contains(&target_key);
        if source_floating || target_floating {
            // Both domains must be observed (target readable: the adapter
            // can only verify membership it can see).
            if !report
                .domains
                .iter()
                .any(|d| d.readable && d.domain.key() == target_key)
            {
                return Err((KIND_SEND_TARGET, MSG_SEND_TARGET));
            }
            if !report.domains.iter().any(|d| d.domain.key() == source_key) {
                return Err((KIND_SEND_WINDOW, MSG_SEND_WINDOW));
            }
            return Ok(RetainedTransfer {
                target: TransferTarget {
                    operation_id: operation_id.to_owned(),
                    kind: TransferKind::Window(window.clone()),
                    target_output: target_output.clone(),
                    target_workspace: target_workspace.to_owned(),
                    follow,
                    held: false,
                },
                policy: false,
                source_output: source_key.output,
                source_workspace: source_key.workspace.0,
            });
        }
        let owner = report.owner.clone();
        let (Some(owner), Some(correlation)) = (
            OwnerId::parse(&owner),
            CorrelationId::parse(&report.request_id),
        ) else {
            return Err((KIND_SNAPSHOT, MSG_SNAPSHOT));
        };
        let Some(source_domain) = report.domains.iter().find(|d| {
            d.domain.id == source_key.output && d.domain.workspace == source_key.workspace
        }) else {
            return Err((KIND_SEND_WINDOW, MSG_SEND_WINDOW));
        };
        let Some(target_domain) = report.domains.iter().find(|d| {
            d.readable
                && d.domain.id == target_key.output
                && d.domain.workspace == target_key.workspace
        }) else {
            return Err((KIND_SEND_TARGET, MSG_SEND_TARGET));
        };
        // Source-only in `windows`, target-only in `target_windows`:
        // the engine combines them for pair convergence, so
        // concatenating here would duplicate every row.
        let (Some(inset_source), Some(inset_target)) = (
            crate::world_engine::inset_world_domain(&source_domain.domain, source_domain.outer_gap),
            crate::world_engine::inset_world_domain(&target_domain.domain, target_domain.outer_gap),
        ) else {
            return Err((KIND_SCOPE_HELD, MSG_SCOPE_HELD));
        };
        let event = CoreEvent {
            owner,
            generation: self.core_generation.clone(),
            correlation,
            revision: 0,
            fingerprint: 0,
            domain: Engine::canonical_component_domain(&inset_source),
            domain_key: source_key.clone(),
            outer_gap: source_domain.outer_gap,
            focused_window: source_domain.focused.clone(),
            windows: source_domain.windows.clone(),
            closed_windows: source_domain.closed.clone(),
            directional: None,
            directional_target_outer_gap: None,
            target_domain: Some((
                Engine::canonical_component_domain(&inset_target),
                target_key.clone(),
            )),
            target_windows: target_domain.windows.clone(),
            command: CoreCommand::SendToWorkspace {
                window: window.0.clone(),
                target_output: target_output.0.clone(),
                target_workspace: target_workspace.to_owned(),
                follow,
            },
        };
        match self.engine.handle(&event) {
            CoreReply::SendWorkspace(_) | CoreReply::SendOutput(_) => Ok(RetainedTransfer {
                target: TransferTarget {
                    operation_id: operation_id.to_owned(),
                    kind: TransferKind::Window(window.clone()),
                    target_output: target_output.clone(),
                    target_workspace: target_workspace.to_owned(),
                    follow,
                    held: false,
                },
                policy: false,
                source_output: source_key.output,
                source_workspace: source_key.workspace.0,
            }),
            // Engine refusals are fixed redacted vocabularies: safe
            // to forward without echoing input.
            CoreReply::Rejected { kind, message } => Err((kind, message)),
            CoreReply::SnapshotInvalid { message, detail } => Err((detail, message)),
            CoreReply::Diverged(reason) => Err((reason.as_str(), reason.message())),
            _ => Err((KIND_SNAPSHOT, MSG_SNAPSHOT)),
        }
    }

    /// Numbered send over the scoped ring for `output`: positional
    /// `1..=9`, or `0` for the trailing path (reuse the live trailing
    /// empty, else mint a stable pending token and pend the send under
    /// core until the binding is reported). Bound targets transfer
    /// exactly like [`WorldCommand::Send`] with no view target at any
    /// step: sends never view-switch, including stays.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start_numbered_send(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        window: &WindowId,
        output: &OutputId,
        index: u8,
        follow: bool,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> CommandApplied {
        let Some(source_row) = by_id.get(window) else {
            return CommandApplied::Rejected {
                kind: KIND_SEND_WINDOW,
                message: MSG_SEND_WINDOW,
            };
        };
        if self.retained.iter().any(|r| match &r.target.kind {
            TransferKind::Window(w) => w == window,
            TransferKind::View | TransferKind::Migration => false,
        }) {
            return CommandApplied::Rejected {
                kind: KIND_IN_FLIGHT,
                message: MSG_IN_FLIGHT,
            };
        }
        let source_key = DomainKey {
            output: source_row.output.clone(),
            workspace: source_row.workspace.clone(),
        };
        // Held scopes refuse; floating workspaces route into the boundary
        // branch of [`WorldEngine::execute_send`] (membership-only when
        // exactly one endpoint floats, held when both do).
        if held_scopes.contains(&source_key) {
            return CommandApplied::Rejected {
                kind: KIND_SCOPE_HELD,
                message: MSG_SCOPE_HELD,
            };
        }
        let ring = self.send_ring(report, output);
        if ring.is_empty() {
            return CommandApplied::Rejected {
                kind: KIND_SEND_TARGET,
                message: MSG_SEND_TARGET,
            };
        }
        if index == 0 {
            let occupied = homed_occupancy(report);
            match resolve_trailing(&ring, &occupied) {
                TrailingSelection::Reuse(target) => {
                    let operation_id = format!("op-{command_seq}");
                    let result = self.execute_send(
                        report,
                        &operation_id,
                        window,
                        output,
                        &target,
                        follow,
                        by_id,
                        held_scopes,
                    );
                    self.settle_send(command_seq, operation_id, result)
                }
                TrailingSelection::Create => {
                    let Some(logical) = self.pending_token(output) else {
                        return CommandApplied::Rejected {
                            kind: KIND_CREATION_EXHAUSTED,
                            message: MSG_CREATION_EXHAUSTED,
                        };
                    };
                    // Pend under core: stable identity, no view target, and
                    // never the token as a native id. The engine send starts
                    // once the binding names the native id.
                    self.note_command(command_seq);
                    self.pending_sends.push(PendingSend {
                        operation_id: format!("op-{command_seq}"),
                        logical_id: logical,
                        output: output.clone(),
                        window: (*window).clone(),
                        follow,
                        source_output: source_key.output,
                        source_workspace: source_key.workspace.0,
                    });
                    CommandApplied::NoEffect
                }
            }
        } else {
            let Some(target) = resolve_numbered(&ring, index) else {
                return CommandApplied::Rejected {
                    kind: KIND_SEND_TARGET,
                    message: MSG_SEND_TARGET,
                };
            };
            let operation_id = format!("op-{command_seq}");
            let result = self.execute_send(
                report,
                &operation_id,
                window,
                output,
                &target,
                follow,
                by_id,
                held_scopes,
            );
            self.settle_send(command_seq, operation_id, result)
        }
    }
}
