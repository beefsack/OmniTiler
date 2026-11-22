//! Parallel v4 desired-state publication: a genuine v4 reply assembled
//! from staged/retained state through the shared
//! [`PublicationRows`](crate::world_publish::PublicationRows) assembly. Reads no accepted-run identity: the caller supplies the
//! envelope directly, and the issuing revision stamps both the reply and
//! the latest-published presentation revision for later renderer-input
//! validation. The narrow producer style hold applies before any write
//! authority is assembled, as a per-reply projection over a local held
//! set: retained holds and staged state are never mutated.
use crate::world_transaction::WorldTransaction;
use std::collections::BTreeSet;

use crate::directional::{OutputId, WorkspaceId};
use crate::session::DomainKey;
use crate::world_presentation::PresentationUnderlay;
use crate::world_report::{
    BorderPresentationV4, HoldReason, OneShotEffect, TransferTargetV4, WORLD_CONTRACT_VERSION_V4,
    WorldCommand, WorldPresentationV4, WorldReplyV4,
};

/// Direct v4 publication envelope: the reply identity the legacy path reads
/// from the accepted report, plus the issuing presentation revision that
/// stamps the reply and the latest-published record. No accepted run, no
/// ingress replay, no budget wiring.
pub(crate) struct PublishEnvelopeV4 {
    pub adapter_incarnation: String,
    pub request_id: String,
    pub observation_seq: u64,
    pub command_consumed: bool,
    pub issuing_revision: u64,
}

impl WorldTransaction<'_> {
    /// Assemble a genuine v4 reply from staged/retained state: shared scope
    /// and window rows, planner-owned workspace and output rows, retained
    /// transfer correlation, verbatim staged diagnostics, and retained
    /// latest-wins settlement projections. The producer style hold (if any) merges its output
    /// domains into a local held set only: the caller's retained
    /// `hold_reasons`/staged state are untouched, and the held projection
    /// (output/workspace rows, transfer `held`, suppressed write lanes)
    /// lives on this reply alone. `born_paused` is always false here:
    /// presentation suspension is not a write hold, and v4 carries no
    /// pause lane. View echoes always pass empty: the unknown-echo guard
    /// stays on the legacy path. Never retains pager presentation (7d) and
    /// never touches candidate adoption (7e).
    pub(crate) fn publish_v4(
        &mut self,
        envelope: PublishEnvelopeV4,
        _command: Option<&WorldCommand>,
        effects: Vec<OneShotEffect>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> WorldReplyV4 {
        let roster_unproved = self.shared_slot_transition_unproved();
        // Narrow producer style hold first, before any authority assembly:
        // the style-held output's domains join a local held set so the
        // established Hold paths suppress every write lane there.
        let style_hold = self.presentation_style_hold_v4();
        let style_output: Option<OutputId> = style_hold.map(|(output, _)| output);
        // Local held expansion for the narrow style hold: staged domains
        // plus every logical home that could authorize a write there. The
        // mapping ring covers fresh logicals with no staged domain, and
        // retained transfer endpoints cover in-flight movers whose homes
        // have not yet converged.
        let mut local_held = held_scopes.clone();
        if let Some(output) = &style_output {
            let staged_keys: Vec<DomainKey> = self
                .world
                .staged
                .domains
                .iter()
                .map(|domain| domain.key.clone())
                .filter(|key| &key.output == output)
                .collect();
            for key in staged_keys {
                local_held.insert(key);
            }
            if let Some(ring) = self.world.workspace.mapping.get(&output.0) {
                for workspace in ring.clone() {
                    local_held.insert(DomainKey {
                        output: output.clone(),
                        workspace: WorkspaceId(workspace),
                    });
                }
            }
            let transfer_keys: Vec<DomainKey> = self
                .world
                .settlement
                .retained
                .iter()
                .flat_map(|retained| {
                    let target = DomainKey {
                        output: retained.target.target_output.clone(),
                        workspace: WorkspaceId(retained.target.target_workspace.clone()),
                    };
                    let source = DomainKey {
                        output: retained.source_output.clone(),
                        workspace: WorkspaceId(retained.source_workspace.clone()),
                    };
                    [target, source]
                })
                .filter(|key| &key.output == output)
                .collect();
            for key in transfer_keys {
                local_held.insert(key);
            }
        }
        // One retained focus request: focus carriers stay INTERNAL through
        // row assembly and cursor-warp projection, never publishing on the
        // v4 wire.
        let rows =
            self.assemble_publication_rows(false, &effects, &local_held, style_output.as_ref());
        let mut workspaces = self.plan_workspaces(&rows.scopes);
        // Output views read the caller's held scopes only: the style hold
        // arrives via `style_output` so a style-only hold yields
        // `Unreadable` while pre-existing holds keep `IncompleteInventory`.
        let mut outputs = self.plan_outputs_inner(&[], style_output.as_ref(), held_scopes);
        if roster_unproved {
            for row in &mut outputs {
                if !self.shared_output_write_held(&row.output) {
                    continue;
                }
                if let Some(view) = self.unproved_shared_view(&row.output) {
                    row.tiling_mode = self.workspace_tiling_mode(&row.output, &view);
                    row.logical_view = Some(view);
                }
                row.hold = row.hold.or(Some(HoldReason::Unreadable));
            }
        }
        // Post-set the Unreadable hold on held-output rows, keeping any
        // existing hold (output rows already carry Unreadable from the
        // core above; workspace rows gain it here).
        if let Some(output) = &style_output {
            for row in &mut workspaces {
                if &row.output == output {
                    row.hold = row.hold.or(Some(HoldReason::Unreadable));
                }
            }
            for row in &mut outputs {
                if &row.output == output {
                    row.hold = row.hold.or(Some(HoldReason::Unreadable));
                }
            }
        }
        let mut windows = rows.windows;
        let roster_plan = self.world.settlement.shared_roster.plan();
        if roster_unproved {
            for row in &mut windows {
                if !self.shared_output_write_held(&row.output) {
                    continue;
                }
                // Frame control is independent of the slot transition. Preserve
                // the projected tile for the same lifetime only when the plan
                // names this exact destination; the producer must first prove
                // arrival in its resolved native slot.
                let arriving = roster_plan.is_some_and(|plan| {
                    plan.members.iter().any(|step| {
                        step.window == row.lifetime
                            && step.output == row.output
                            && plan.projections.get(&row.output).is_some_and(|ring| {
                                ring.iter()
                                    .zip(&plan.slots)
                                    .any(|(id, slot)| id == &row.workspace && slot == &step.target)
                            })
                    })
                });
                if let Some(home) = self.unproved_shared_home(&row.lifetime) {
                    row.output = home.output;
                    row.workspace = home.workspace.0;
                }
                if !arriving {
                    row.allocation = None;
                    row.effective_frame = None;
                    row.frame_authority =
                        crate::world_report::ScopeAuthority::Hold(HoldReason::Unreadable);
                }
                row.frame_operation = None;
                row.clear_operation = None;
                let hold = crate::world_report::ScopeAuthority::Hold(HoldReason::Unreadable);
                row.membership_authority = hold;
                row.visibility = None;
                row.visibility_authority = hold;
            }
        }
        let cursor_warp = self.warp_for_publish(&effects, &windows, &local_held);
        self.prune_settled_outcomes();
        self.prune_drag_preview(&local_held);
        let drag_preview = self.world.gesture.drag_preview.clone();
        let gesture_settled = self.world.gesture.terminal;
        let operation_settled = self
            .world
            .settlement
            .operation_settled
            .values()
            .cloned()
            .collect();
        let native_creations = self
            .native_creation_desires()
            .into_iter()
            .filter(|desire| {
                style_output
                    .as_ref()
                    .is_none_or(|output| *output != desire.target_output)
            })
            .collect();
        let intent_desired = self.project_intent_desired();
        let switch_settled = self
            .world
            .settlement
            .switch_settled
            .values()
            .cloned()
            .collect();
        // Retained transfer projection: the explicit originating command
        // correlation rides verbatim (never parsed from operation ids), with
        // `held` set when native authority belongs to the roster plan or
        // when the style-held output is the transfer source or target.
        // Correlation is descriptive and never renews native authority.
        let transfers = self
            .world
            .settlement
            .retained
            .iter()
            .map(|retained| TransferTargetV4 {
                operation_id: retained.target.operation_id.clone(),
                kind: retained.target.kind.clone(),
                target_output: retained.target.target_output.clone(),
                target_workspace: retained.target.target_workspace.clone(),
                follow: retained.target.follow,
                held: retained.target.held
                    || roster_unproved
                    || style_output.as_ref().is_some_and(|output| {
                        *output == retained.target.target_output
                            || *output == retained.source_output
                    }),
                command_seq: retained.command_seq,
                parent: retained.migration_parent_op.clone(),
            })
            .collect();
        // Retained focus lane: post-convergence eligibility gates run
        // here so every publication carries the current row, and re-emission
        // alone never marks it attempted. The `afterShown` projection
        // copies the frozen selection binding (frozen above at the same
        // selection freeze); parent settlement cannot erase the copy.
        self.validate_pending_focus();
        self.freeze_focus_after_shown();
        let focus_request = self.pending_focus_request();
        let focus_held = roster_unproved
            && focus_request
                .as_ref()
                .is_some_and(|focus| self.shared_output_write_held(&focus.output));
        self.freeze_shared_roster_focus(focus_request.as_ref().filter(|_| focus_held).cloned());
        // Keep the single canonical retained grant visible. On the shared
        // slot route its identical plan operand gates the native action;
        // a new grant on an affected output waits for the epoch.
        let focus_request = focus_request.filter(|focus| {
            !focus_held
                || self
                    .world
                    .settlement
                    .shared_roster
                    .plan()
                    .and_then(|plan| plan.focus.as_ref())
                    == Some(focus)
        });
        let cursor_warp = if focus_held { None } else { cursor_warp };
        WorldReplyV4 {
            contract_version: WORLD_CONTRACT_VERSION_V4,
            adapter_incarnation: envelope.adapter_incarnation,
            core_generation: self.connection.core_generation_str.clone(),
            request_id: envelope.request_id,
            based_on_observation_seq: envelope.observation_seq,
            state_revision: envelope.issuing_revision,
            ready: self.is_ready(),
            command_consumed: envelope.command_consumed,
            scopes: rows.scopes,
            workspaces,
            outputs,
            diagnostics: self.world.staged.diagnostics.clone(),
            windows,
            transfers,
            focus_request,
            drag_preview,
            gesture_settled,
            operation_settled,
            native_creations,
            cursor_warp,
            presentation: WorldPresentationV4 {
                foreground: String::new(),
                rev: 0,
                suspended_output: None,
                border: BorderPresentationV4 {
                    visible: false,
                    reason: None,
                    subject: None,
                    outsets: None,
                    width: None,
                    radius: None,
                    witness: None,
                },
                underlay: PresentationUnderlay::default(),
                pager: Vec::new(),
            },
            intent_desired,
            switch_settled,
        }
    }
}
