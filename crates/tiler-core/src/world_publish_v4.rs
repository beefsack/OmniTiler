//! Parallel v4 desired-state publication: a genuine v4 reply assembled
//! from staged/retained state, mirroring the legacy [`WorldEngine::publish`]
//! path through the shared [`crate::world_publish::PublicationRows`]
//! assembly. Reads no accepted-run identity: the caller supplies the
//! envelope directly, and the issuing revision stamps both the reply and
//! the latest-published presentation revision for later renderer-input
//! validation. The narrow producer style hold applies before any write
//! authority is assembled, as a per-reply projection over a local held
//! set: retained holds and staged state are never mutated.
use std::collections::BTreeSet;

use crate::directional::{OutputId, WorkspaceId};
use crate::session::DomainKey;
use crate::world_engine::WorldEngine;
use crate::world_report::{
    CommandResult, HoldReason, OneShotEffect, TransferTargetV4, WORLD_CONTRACT_VERSION_V4,
    WorldCommand, WorldReplyV4,
};

/// Direct v4 publication envelope: the reply identity the legacy path reads
/// from the accepted report, plus the issuing presentation revision that
/// stamps the reply and the latest-published record. No accepted run, no
/// ingress replay, no budget wiring.
// allow(dead_code): pending 7d command dispatch; tests drive it.
#[allow(dead_code)]
pub(crate) struct PublishEnvelopeV4 {
    pub adapter_incarnation: String,
    pub request_id: String,
    pub observation_seq: u64,
    pub command_consumed: bool,
    pub issuing_revision: u64,
}

impl WorldEngine {
    /// Assemble a genuine v4 reply from staged/retained state: shared scope
    /// and window rows, planner-owned workspace and output rows, retained
    /// transfer correlation, verbatim staged diagnostics, and exactly-once
    /// terminal takes. The producer style hold (if any) merges its output
    /// domains into a local held set only: the caller's retained
    /// `hold_reasons`/staged state are untouched, and the held projection
    /// (output/workspace rows, transfer `held`, suppressed write lanes)
    /// lives on this reply alone. `born_paused` is always false here:
    /// presentation suspension is not a write hold, and v4 carries no
    /// pause lane. View echoes always pass empty: the unknown-echo guard
    /// stays on the legacy path. Never retains pager presentation (7d) and
    /// never touches candidate adoption (7e).
    // allow(dead_code): pending 7d command dispatch; tests drive it.
    #[allow(dead_code)]
    pub(crate) fn publish_v4(
        &mut self,
        envelope: PublishEnvelopeV4,
        command: Option<&WorldCommand>,
        effects: Vec<OneShotEffect>,
        command_result: Option<CommandResult>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> WorldReplyV4 {
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
                .staged
                .domains
                .iter()
                .map(|domain| domain.key.clone())
                .filter(|key| &key.output == output)
                .collect();
            for key in staged_keys {
                local_held.insert(key);
            }
            if let Some(ring) = self.mapping.get(&output.0) {
                for workspace in ring.clone() {
                    local_held.insert(DomainKey {
                        output: output.clone(),
                        workspace: WorkspaceId(workspace),
                    });
                }
            }
            let transfer_keys: Vec<DomainKey> = self
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
        // Focus, Raise, and Stack are all native effects: any kind homed
        // on the style-held output never records, retains, warps, or
        // publishes, so the adapter never actuates what it cannot verify.
        // Unknown homing passes through.
        let effects: Vec<OneShotEffect> = effects
            .into_iter()
            .filter(|effect| {
                let Some(output) = &style_output else {
                    return true;
                };
                self.staged
                    .homing
                    .get(&effect.target)
                    .is_none_or(|home| home.output != *output)
            })
            .collect();
        let rows =
            self.assemble_publication_rows(false, &effects, &local_held, style_output.as_ref());
        let mut workspaces = self.plan_workspaces(&rows.scopes);
        // Output views read the caller's held scopes only: the style hold
        // arrives via `style_output` so a style-only hold yields
        // `Unreadable` while pre-existing holds keep `IncompleteInventory`.
        let mut outputs = self.plan_outputs_inner(&[], style_output.as_ref(), held_scopes);
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
        let windows = rows.windows;
        let cursor_warp = self.warp_for_publish(&effects, &windows, &local_held);
        let gesture_settled = self.gesture_terminal.take();
        let operation_settled = std::mem::take(&mut self.operation_settled);
        let native_creations = self
            .native_creation_desires()
            .into_iter()
            .filter(|desire| {
                style_output
                    .as_ref()
                    .is_none_or(|output| *output != desire.target_output)
            })
            .collect();
        let intent_writes =
            self.drain_intent_saves(command, envelope.command_consumed, &command_result);
        let switch_settled = std::mem::take(&mut self.switch_settled);
        // Retained transfer projection: the explicit originating command
        // correlation rides verbatim (never parsed from operation ids), with
        // `held` set when the style-held output is the transfer source or
        // target. A per-reply projection only: retained settlement meaning
        // is untouched.
        let transfers = self
            .retained
            .iter()
            .map(|retained| TransferTargetV4 {
                operation_id: retained.target.operation_id.clone(),
                kind: retained.target.kind.clone(),
                target_output: retained.target.target_output.clone(),
                target_workspace: retained.target.target_workspace.clone(),
                follow: retained.target.follow,
                held: retained.target.held
                    || style_output.as_ref().is_some_and(|output| {
                        *output == retained.target.target_output
                            || *output == retained.source_output
                    }),
                command_seq: retained.command_seq,
            })
            .collect();
        let mut presentation = self.presentation_for_publish_v4();
        presentation.rev = envelope.issuing_revision;
        self.latest_published_presentation_rev_v4 = envelope.issuing_revision;
        // Retained close-focus lane: post-convergence eligibility gates run
        // here so every publication carries the current row, and re-emission
        // alone never marks it attempted.
        self.validate_pending_close_focus();
        let close_focus = self.pending_close_focus_request();
        WorldReplyV4 {
            contract_version: WORLD_CONTRACT_VERSION_V4,
            adapter_incarnation: envelope.adapter_incarnation,
            core_generation: self.core_generation_str.clone(),
            request_id: envelope.request_id,
            based_on_observation_seq: envelope.observation_seq,
            state_revision: envelope.issuing_revision,
            ready: self.is_ready(),
            command_consumed: envelope.command_consumed,
            scopes: rows.scopes,
            workspaces,
            outputs,
            diagnostics: self.staged.diagnostics.clone(),
            windows,
            transfers,
            effects,
            close_focus,
            command_result,
            gesture_settled,
            operation_settled,
            native_creations,
            cursor_warp,
            presentation,
            intent_writes,
            switch_settled,
        }
    }
}
