//! Planner-owned tiling and view commands (D2): per-workspace tiling
//! toggles, policy and selection view retention, scoped rings, and stable
//! pending-creation tokens.
//!
//! Split from [`crate::world_commands`]: this module only extends the
//! [`crate::world_engine::WorldEngine`] `impl`.

use std::collections::BTreeMap;

use crate::directional::OutputId;
use crate::session::DomainKey;
use crate::workspace_scope::{plan_global_swap, scoped_ring_ids};
use crate::world_commands::CommandApplied;
use crate::world_engine::{
    KIND_TILING_TARGET, KIND_VIEW_OUTPUT, KIND_VIEW_TARGET, MSG_TILING_TARGET, MSG_VIEW_OUTPUT,
    MSG_VIEW_TARGET, PendingCreation, RetainedTransfer, WorldEngine,
};
use crate::world_publish::live_native_order;
use crate::world_report::{TilingMode, TransferKind, TransferTarget, WorldReport};

impl WorldEngine {
    /// Planner-owned per-workspace tiling toggle. The workspace must be a
    /// live native id on `output`. Disabling releases the retained session
    /// (no frame writes while floating) and drops window transfers touching
    /// the scope; re-enabling re-adopts through the fresh route. Idempotent.
    pub(crate) fn set_workspace_tiling(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        output: &OutputId,
        workspace: &str,
        tiled: bool,
    ) -> CommandApplied {
        let known = report.topology.iter().any(|topology| {
            &topology.output == output && topology.native_order.iter().any(|id| id == workspace)
        });
        if !known {
            return CommandApplied::Rejected {
                kind: KIND_TILING_TARGET,
                message: MSG_TILING_TARGET,
            };
        }
        let key = DomainKey {
            output: output.clone(),
            workspace: crate::directional::WorkspaceId(workspace.to_owned()),
        };
        let mode = if tiled {
            TilingMode::Tiled
        } else {
            TilingMode::Floating
        };
        self.tiling.insert(key.clone(), mode);
        if tiled {
            self.floating_scopes.remove(&key);
        } else {
            self.floating_scopes.insert(key.clone());
            // Release retained state: the adapter owns floating geometry.
            self.engine.remove(&key);
            // Newer input wins: window transfers touching the scope settle.
            self.retained.retain(|r| match &r.target.kind {
                TransferKind::Window(_) => {
                    let target_key = DomainKey {
                        output: r.target.target_output.clone(),
                        workspace: crate::directional::WorkspaceId(
                            r.target.target_workspace.clone(),
                        ),
                    };
                    target_key != key
                        && !(r.source_output == key.output && r.source_workspace == key.workspace.0)
                }
                TransferKind::View | TransferKind::Migration => true,
            });
            self.pending_sends.retain(|pending| {
                !(pending.source_output == key.output
                    && pending.source_workspace == key.workspace.0)
            });
        }
        self.note_command(command_seq);
        CommandApplied::NoEffect
    }

    /// Policy-retained view target (hotplug displacement/return):
    /// latest per output wins, exactly like command views, under a caller
    /// supplied stable operation identity (per-cycle epoch, see the
    /// displacement pass). Never consumes a command sequence.
    pub(crate) fn retain_policy_view(
        &mut self,
        output: &OutputId,
        workspace: &str,
        operation_id: String,
    ) {
        self.retained.retain(|r| {
            !(matches!(r.target.kind, TransferKind::View) && r.target.target_output == *output)
        });
        self.retained.push(RetainedTransfer {
            target: TransferTarget {
                operation_id,
                kind: TransferKind::View,
                target_output: output.clone(),
                target_workspace: workspace.to_owned(),
                follow: false,
                held: false,
            },
            policy: false,
            source_output: output.clone(),
            source_workspace: String::new(),
            members: Vec::new(),
            migration_refill: None,
            migration_initial_empty: false,
        });
    }

    /// Retain one view target for selection/switch commands. The output must
    /// be observed and the workspace must name a known native id (or a
    /// pending logical token just created by the trailing path).
    pub(crate) fn retain_view(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        output: &OutputId,
        workspace: &str,
    ) -> CommandApplied {
        let Some(topology) = report
            .topology
            .iter()
            .find(|topology| &topology.output == output)
        else {
            return CommandApplied::Rejected {
                kind: KIND_VIEW_OUTPUT,
                message: MSG_VIEW_OUTPUT,
            };
        };
        let known = topology.native_order.iter().any(|id| id == workspace)
            || self
                .pending_creations
                .iter()
                .any(|pending| pending.output == *output && pending.logical_id == workspace);
        if !known {
            return CommandApplied::Rejected {
                kind: KIND_VIEW_TARGET,
                message: MSG_VIEW_TARGET,
            };
        }
        self.note_command(command_seq);
        // Latest view command wins per output.
        self.retained.retain(|r| {
            !(matches!(r.target.kind, TransferKind::View) && r.target.target_output == *output)
        });
        self.retained.push(RetainedTransfer {
            target: TransferTarget {
                operation_id: format!("op-{command_seq}"),
                kind: TransferKind::View,
                target_output: output.clone(),
                target_workspace: workspace.to_owned(),
                follow: false,
                held: false,
            },
            policy: false,
            source_output: output.clone(),
            source_workspace: String::new(),
            members: Vec::new(),
            migration_refill: None,
            migration_initial_empty: false,
        });
        CommandApplied::NoEffect
    }

    /// Scoped selection ring for one output (D2): the retained mapping
    /// ring for local/global-unique, the shared order for shared mode.
    pub(crate) fn selection_ring(&self, report: &WorldReport, output: &OutputId) -> Vec<String> {
        let live = live_native_order(report);
        scoped_ring_ids(
            report.workspace_mode,
            &self.mapping,
            &self.shared_order,
            &output.0,
            &live,
        )
    }

    /// Selection ring plus global-unique swap visibility: the ring for
    /// numbered/relative resolution.
    pub(crate) fn resolve_selection_ring(
        &self,
        report: &WorldReport,
        output: &OutputId,
    ) -> Option<Vec<String>> {
        let ring = self.selection_ring(report, output);
        if ring.is_empty() { None } else { Some(ring) }
    }

    /// Send ring for a window homed on `output`: the same scoped ring as
    /// selection, resolved once before transfer.
    pub(crate) fn send_ring(&self, report: &WorldReport, output: &OutputId) -> Vec<String> {
        self.selection_ring(report, output)
    }

    /// Visible-elsewhere swap plan for global-unique selection (D2).
    pub(crate) fn global_swap_plan(
        &self,
        report: &WorldReport,
        output: &OutputId,
        target: &str,
    ) -> Option<crate::workspace_scope::SwapPlan> {
        let current_by_output: BTreeMap<String, String> = report
            .topology
            .iter()
            .map(|topology| (topology.output.0.clone(), topology.current_native.clone()))
            .collect();
        plan_global_swap(&current_by_output, &output.0, target)
    }

    /// Stable pending logical token for one output, shared with lifecycle.
    /// Exhaustion refuses (`None`): the counter never wraps and never
    /// reuses the final token, so a saturated backend re-bootstraps under a
    /// new generation instead of aliasing two outputs' tokens.
    pub(crate) fn pending_token(&mut self, output: &OutputId) -> Option<String> {
        if let Some(pending) = self
            .pending_creations
            .iter()
            .find(|pending| pending.output == *output)
        {
            return Some(pending.logical_id.clone());
        }
        if self.creation_counter == u64::MAX {
            return None;
        }
        // Creation tokens never wrap: exhaustion refuses and re-bootstraps.
        let next = self.creation_counter.saturating_add(1);
        self.creation_counter = next;
        let logical = format!("wl-{next}");
        self.pending_creations.push(PendingCreation {
            logical_id: logical.clone(),
            output: output.clone(),
        });
        Some(logical)
    }
}
