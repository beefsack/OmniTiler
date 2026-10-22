//! Planner-owned tiling and view commands (D2): per-workspace tiling
//! toggles, policy and selection view retention, and scoped rings.
//!
//! Split from [`crate::world_commands`]: this module only extends the
//! [`crate::world_engine::WorldEngine`] `impl`.

use std::collections::BTreeMap;

use crate::directional::OutputId;
use crate::session::DomainKey;
use crate::world_commands::CommandApplied;
use crate::world_engine::{
    KIND_TILING_TARGET, KIND_VIEW_OUTPUT, KIND_VIEW_TARGET, MSG_TILING_TARGET, MSG_VIEW_OUTPUT,
    MSG_VIEW_TARGET, RetainedTransfer, WorldEngine,
};
use crate::world_report::{TilingMode, TransferKind, TransferTarget, WorldReport};

impl WorldEngine {
    /// Planner-owned per-workspace tiling toggle. The workspace must be a
    /// known logical id on `output`. Disabling releases the retained
    /// session (no frame writes while floating) and drops window transfers
    /// touching the scope; re-enabling re-adopts through the fresh route.
    /// Idempotent.
    pub(crate) fn set_workspace_tiling(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        output: &OutputId,
        workspace: &str,
        tiled: bool,
    ) -> CommandApplied {
        let _ = report;
        let known = self
            .mapping
            .get(&output.0)
            .is_some_and(|ring| ring.iter().any(|id| id == workspace));
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
        self.retain_view_inner(output, workspace, operation_id);
    }

    /// Shared view retention: latest view command wins per output. The
    /// output must be a live screen and the workspace must name a known
    /// logical id in its ring.
    pub(crate) fn retain_view_inner(
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
            migration_source_index: None,
            migration_source_view: None,
            migration_target_view: None,
            migration_terminal: None,
            pending_member: None,
            pending_members: BTreeMap::new(),
            pending_view: None,
        });
        self.views.insert(output.0.clone(), workspace.to_owned());
    }

    /// Retain one view target for selection/switch commands. The output must
    /// be a live screen and the workspace must name a known logical id in
    /// its ring. Shared mode fans out: the ring unifies across all live
    /// outputs and every output shows the same view with one shared
    /// history entry.
    pub(crate) fn retain_view(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        output: &OutputId,
        workspace: &str,
    ) -> CommandApplied {
        if !self.mapping.contains_key(&output.0) {
            return CommandApplied::Rejected {
                kind: KIND_VIEW_OUTPUT,
                message: MSG_VIEW_OUTPUT,
            };
        }
        let known = self
            .mapping
            .get(&output.0)
            .is_some_and(|ring| ring.iter().any(|id| id == workspace));
        if !known {
            return CommandApplied::Rejected {
                kind: KIND_VIEW_TARGET,
                message: MSG_VIEW_TARGET,
            };
        }
        self.note_command(command_seq);
        // Latest view command wins per output. Core views move immediately
        // (logical authority); the retained flight proves actuation.
        self.retain_view_inner(output, workspace, format!("op-{command_seq}"));
        if report.workspace_mode == crate::workspace_scope::WorkspaceMode::Shared {
            let canonical = self.mapping.get(&output.0).cloned().unwrap_or_default();
            let live: Vec<String> = {
                let mut live: Vec<String> = report
                    .screens
                    .iter()
                    .map(|screen| screen.output.0.clone())
                    .collect();
                live.sort();
                live
            };
            for out in &live {
                self.mapping.insert(out.clone(), canonical.clone());
                self.views.insert(out.clone(), workspace.to_owned());
            }
            self.history.observe_shared(&canonical, workspace);
        }
        CommandApplied::NoEffect
    }

    /// Scoped selection ring for one output: the retained logical ring.
    /// Shared mode returns the unified order (first live screen's ring).
    pub(crate) fn selection_ring(&self, report: &WorldReport, output: &OutputId) -> Vec<String> {
        if report.workspace_mode == crate::workspace_scope::WorkspaceMode::Shared {
            let mut live: Vec<String> = report
                .screens
                .iter()
                .map(|screen| screen.output.0.clone())
                .collect();
            live.sort();
            if let Some(first) = live.first() {
                return self.mapping.get(first).cloned().unwrap_or_default();
            }
        }
        self.mapping.get(&output.0).cloned().unwrap_or_default()
    }

    /// Selection ring plus visibility: the ring for numbered/relative
    /// resolution.
    pub(crate) fn resolve_selection_ring(
        &self,
        report: &WorldReport,
        output: &OutputId,
    ) -> Option<Vec<String>> {
        let ring = self.selection_ring(report, output);
        if ring.is_empty() { None } else { Some(ring) }
    }

    /// Resolve the switch output for numbered/relative/previous/trailing
    /// selections. An explicit output names a genuinely semantic target
    /// and passes through (the retain path below validates it); `None`
    /// resolves in core from the cursor-first switch target over the
    /// report cursor and retained focused facts, then the primary/first
    /// fallback. `None` only when every source is unknown: callers refuse
    /// without inventing focus.
    pub(crate) fn resolve_selection_output(
        &self,
        report: &WorldReport,
        explicit: Option<&OutputId>,
    ) -> Option<OutputId> {
        if let Some(output) = explicit {
            return Some(output.clone());
        }
        crate::cursor_targeting::resolve_switch_output(
            report.switch_target,
            self.fresh_cursor_output(report).as_ref(),
            self.focused_output(report).as_ref(),
            &self.primary_first_outputs(report),
        )
    }

    /// Send ring for a window homed on `output`: the same ring as
    /// selection, resolved once before transfer.
    pub(crate) fn send_ring(&self, report: &WorldReport, output: &OutputId) -> Vec<String> {
        self.selection_ring(report, output)
    }
}
