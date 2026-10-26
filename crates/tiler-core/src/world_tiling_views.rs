//! Planner-owned tiling and view commands (D2): per-workspace tiling
//! toggles, policy and selection view retention, and scoped rings.
//!
//! Split from [`crate::world_commands`]: this module only extends the
//! [`crate::world_engine::WorldEngine`] `impl`.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{OutputId, WindowId};
use crate::seed::EngineWindow;
use crate::session::DomainKey;
use crate::world_commands::CommandApplied;
use crate::world_engine::{
    KIND_TILING_TARGET, KIND_VIEW_OUTPUT, KIND_VIEW_TARGET, MSG_TILING_TARGET, MSG_VIEW_OUTPUT,
    MSG_VIEW_TARGET, RetainedTransfer, WorldEngine,
};
use crate::world_report::{OneShotEffect, TilingMode, TransferKind, TransferTarget, WorldReport};

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
    ///
    /// R-WS-09: returning to a workspace restores its remembered focus
    /// (REQ-WS-09); with no remembered focus it falls back to the
    /// most-recently-used eligible member, else the first member in
    /// deterministic layout order (tentative, pending user review). When
    /// the view actually changes, emit one explicit at-most-once focus
    /// effect to the resolved window when it stages eligible. Valid
    /// no-focus states (R-CLOSE-03/REQ-MNZ-03: empty workspaces,
    /// minimized-only survivors) naturally resolve to no candidate and
    /// stay `NoEffect`: no focus is invented.
    pub(crate) fn retain_view(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        output: &OutputId,
        workspace: &str,
        held_scopes: &BTreeSet<DomainKey>,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
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
        // View-change gate: a redundant select (already showing the
        // target) keeps the prior no-effect behavior and never invents
        // focus.
        let current = self.effective_view(output);
        let changed = current.as_deref() != Some(workspace);
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
        if !changed {
            return CommandApplied::NoEffect;
        }
        let target_key = DomainKey {
            output: output.clone(),
            workspace: crate::directional::WorkspaceId(workspace.to_owned()),
        };
        let Some(window) = self.remembered_view_focus(&target_key, held_scopes, by_id) else {
            return CommandApplied::NoEffect;
        };
        CommandApplied::Effect(OneShotEffect::explicit_focus(
            command_seq,
            window,
            self.state_revision + 1,
        ))
    }

    /// Remembered R-WS-09 focus candidate for one view target: the first
    /// eligible window over the target domain's view-return candidates
    /// (remembered, else MRU recency order, else layout order). Ineligible
    /// top candidates (sticky, floating, unexplained fit-excluded, or
    /// unstaged rows) fall through to the next linked member. `None`
    /// preserves every valid no-focus state and every gate (including
    /// held scopes) without inventing a fallback.
    pub(crate) fn remembered_view_focus(
        &self,
        target: &DomainKey,
        held_scopes: &BTreeSet<DomainKey>,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
    ) -> Option<WindowId> {
        if held_scopes.contains(target) {
            return None;
        }
        if self.floating_scopes.contains(target) {
            return None;
        }
        let staged = self.staged.domains.iter().find(|d| d.key == *target)?;
        if !staged.readable {
            return None;
        }
        let session = self.engine.session(target)?;
        for leaf in session.view_return_candidates(target) {
            let Some(window) = session.focused_window_for(&leaf, target) else {
                continue;
            };
            let Some(row) = by_id.get(&window) else {
                continue;
            };
            // No automatic permission for unexplained fit-excluded rows
            // (game/overlay without a floating, fullscreen, or maximized
            // origin), mirroring the send path. Overlays stay focusable;
            // sticky never rides a view return.
            if row.sticky {
                continue;
            }
            if row.fit_excluded && !row.floating && !row.fullscreen && !row.maximized {
                continue;
            }
            // Floating rows never resolve through a linked leaf, but
            // belt-and-suspenders keeps the view return on tiled members.
            if row.floating {
                continue;
            }
            return Some(window);
        }
        None
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

    /// Resolve the migration/control action output from the neutral
    /// report facts: the adapter-grounded foreground output, the cursor
    /// output, the adapter-cached output, then known-registry order. Every
    /// leg filters to the ordered known registry (which retains mapped
    /// stale outputs); a stale mapped leg selects and refuses downstream.
    /// The sole policy owner for migration/control source selection;
    /// explicit command outputs bypass it unchanged.
    pub(crate) fn resolve_action_output(&self, report: &WorldReport) -> Option<OutputId> {
        crate::cursor_targeting::resolve_action_output(
            report.action_foreground.as_ref(),
            self.fresh_cursor_output(report).as_ref(),
            report.action_cached.as_ref(),
            &report.action_outputs,
        )
    }

    /// Resolve an omitted send source from the pre-observation retained
    /// home, else the migration/control action context. `None` only when
    /// both are unknown: callers refuse without inventing focus. Call with
    /// pre-sweep engine state only (see `run_accepted`): the retained home
    /// must predate staging, settle, and reconcile for the comparison
    /// against the staged mover to preserve the retired adapter contract
    /// exactly.
    pub(crate) fn resolve_home_then_action(
        &self,
        report: &WorldReport,
        window: &WindowId,
    ) -> Option<OutputId> {
        if let Some(home) = self
            .members
            .get(window)
            .map(|state| state.home.output.clone())
        {
            return Some(home);
        }
        self.resolve_action_output(report)
    }

    /// Resolve an omitted send source from the pre-observation retained
    /// home, else the cached output when live, else the first live screen
    /// in report order. Same pre-sweep contract as
    /// [`resolve_home_then_action`](Self::resolve_home_then_action):
    /// the keyboard digit/relative fallback universe (cached-live, then
    /// native screen enumeration order), never the pointer.
    pub(crate) fn resolve_home_then_cached_live(
        &self,
        report: &WorldReport,
        window: &WindowId,
    ) -> Option<OutputId> {
        if let Some(home) = self
            .members
            .get(window)
            .map(|state| state.home.output.clone())
        {
            return Some(home);
        }
        let live: BTreeSet<&str> = report
            .screens
            .iter()
            .map(|screen| screen.output.0.as_str())
            .collect();
        if let Some(cached) = report.action_cached.as_ref()
            && live.contains(cached.0.as_str())
        {
            return Some(cached.clone());
        }
        report.screens.first().map(|screen| screen.output.clone())
    }
}
