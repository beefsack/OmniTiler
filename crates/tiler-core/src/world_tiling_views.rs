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
use crate::world_report::{OneShotEffect, TilingMode, TransferKind, TransferTarget};

impl WorldEngine {
    /// Planner-owned per-workspace tiling toggle. The workspace must be a
    /// known logical id on `output`. Disabling releases the retained
    /// session (no frame writes while floating) and drops window transfers
    /// touching the scope; re-enabling re-adopts through the fresh route.
    /// Idempotent.
    pub(crate) fn set_workspace_tiling(
        &mut self,
        command_seq: u64,
        output: &OutputId,
        workspace: &str,
        tiled: bool,
    ) -> CommandApplied {
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

    /// Neutral per-workspace tiling toggle. The workspace must be a known
    /// logical id on `output` (same refusal as
    /// [`Self::set_workspace_tiling`]); the retained mode is resolved at
    /// execution time, so queued toggles compose with no adapter snapshot
    /// policy. Idempotent per execution.
    pub(crate) fn toggle_workspace_tiling(
        &mut self,
        command_seq: u64,
        output: &OutputId,
        workspace: &str,
    ) -> CommandApplied {
        let tiled = self.logical_tiling(&output.0, workspace) != TilingMode::Tiled;
        self.set_workspace_tiling(command_seq, output, workspace, tiled)
    }

    /// Policy-retained view target (hotplug displacement/return):
    /// latest per output wins, exactly like command views, under a caller
    /// supplied stable operation identity (per-cycle `op-dpv-*` with a
    /// per-run index, see the displacement pass). Never consumes a command
    /// sequence.
    ///
    /// The retained flight proves actuation like a command view. Beside
    /// it, the policy view binds a focusless frozen selection under the
    /// same parent: no command sequence is invented, no focus effect is
    /// minted, and visibility completes through the common path while
    /// native focus stays untouched. Each parent keeps its own pending
    /// carrier (latest per output wins); the command carrier is never
    /// touched here (command dispatch owns supersession there at command
    /// time).
    pub(crate) fn retain_policy_view(
        &mut self,
        output: &OutputId,
        workspace: &str,
        operation_id: String,
    ) {
        // Departing home reads before retention moves the view: the
        // post-retain view already names the target.
        let departing: Vec<DomainKey> = self
            .effective_view(output)
            .filter(|view| view != workspace)
            .map(|view| DomainKey {
                output: output.clone(),
                workspace: crate::directional::WorkspaceId(view),
            })
            .into_iter()
            .collect();
        self.retain_view_inner(output, workspace, operation_id.clone());
        let home = DomainKey {
            output: output.clone(),
            workspace: crate::directional::WorkspaceId(workspace.to_owned()),
        };
        let rev = self.state_revision + 1;
        let obs = self.staged.observation.observation_seq;
        self.pending_policy_selections
            .retain(|pending| pending.focus_home.output != *output);
        self.pending_policy_selections
            .push(crate::world_selection::PendingSelection {
                command_seq: None,
                parent_op: operation_id,
                focus: None,
                focus_home: home.clone(),
                selected: vec![home],
                departing,
                affected_outputs: vec![output.clone()],
                rev,
                obs,
                follow_mover: false,
            });
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
            migration_selection_rev: None,
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
        // Affected outputs for selection authorization: shared mode fans
        // out to every live output, otherwise just the target output.
        // Departing views are captured before retention moves them.
        let affected: Vec<OutputId> =
            if self.workspace_mode_setting() == crate::workspace_scope::WorkspaceMode::Shared {
                let mut live: Vec<String> = self
                    .staged
                    .screens
                    .iter()
                    .map(|screen| screen.output.0.clone())
                    .collect();
                live.sort();
                live.into_iter().map(OutputId).collect()
            } else {
                vec![output.clone()]
            };
        let departing: Vec<DomainKey> = affected
            .iter()
            .filter_map(|out| {
                self.effective_view(out).map(|view| DomainKey {
                    output: out.clone(),
                    workspace: crate::directional::WorkspaceId(view),
                })
            })
            .collect();
        self.note_command(command_seq);
        // Latest view command wins per output. Core views move immediately
        // (logical authority); the retained flight proves actuation.
        // Native-free redundant selects mint no flight: the view already
        // shows the target, supersession and history converge without it,
        // and the selection binding (when one freezes below) retires
        // nothing. Host-owned redundant selects keep their flight for the
        // staged native proof.
        let native_free = !self.host_owned_workspace_domains();
        if changed || !native_free {
            self.retain_view_inner(output, workspace, format!("op-{command_seq}"));
        }
        if self.workspace_mode_setting() == crate::workspace_scope::WorkspaceMode::Shared {
            let canonical = self.mapping.get(&output.0).cloned().unwrap_or_default();
            let live: Vec<String> = {
                let mut live: Vec<String> = self
                    .staged
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
        // Changed selections supersede retained selection authorization on
        // every affected output, whether or not they resolve focus: latest
        // intent wins and stale controlled targets must not survive.
        self.prune_selection_for_outputs(&affected);
        let target_key = DomainKey {
            output: output.clone(),
            workspace: crate::directional::WorkspaceId(workspace.to_owned()),
        };
        let Some(window) = self.remembered_view_focus(&target_key, held_scopes, by_id) else {
            let rev = self.state_revision + 1;
            let selected: Vec<DomainKey> = affected
                .iter()
                .map(|out| DomainKey {
                    output: out.clone(),
                    workspace: crate::directional::WorkspaceId(workspace.to_owned()),
                })
                .collect();
            self.pending_selection = Some(crate::world_selection::PendingSelection {
                command_seq: Some(command_seq),
                parent_op: format!("op-{command_seq}"),
                focus: None,
                focus_home: target_key,
                selected,
                departing,
                affected_outputs: affected,
                rev,
                obs: self.staged.observation.observation_seq,
                follow_mover: false,
            });
            return CommandApplied::NoEffect;
        };
        // Native-free suppression-capable route: an actual resolved focus
        // effect carries the arrival grant with shows and hides authorized
        // under its issuing revision. Host-owned native route: the same
        // awaitShown carrier with native desktop obligations (native
        // selector readback plus native-home arrival) and no per-window
        // targets. Ordinary direct focus, admission, and every
        // out-of-scope route keep the legacy effect below.
        //
        // A resolved focus that fails route eligibility still freezes a
        // focusless binding with no required focus target: visibility
        // completes through the common path and focus reports unproven,
        // while the legacy direct effect below stays untouched.
        let eligible = self.selection_route_eligible(&window, &target_key, held_scopes)
            || self.host_selection_route_eligible(&window, &target_key, held_scopes);
        let rev = self.state_revision + 1;
        let selected: Vec<DomainKey> = affected
            .iter()
            .map(|out| DomainKey {
                output: out.clone(),
                workspace: crate::directional::WorkspaceId(workspace.to_owned()),
            })
            .collect();
        let pending = if eligible {
            crate::world_selection::PendingSelection {
                command_seq: Some(command_seq),
                parent_op: format!("op-{command_seq}"),
                focus: Some(window.clone()),
                focus_home: target_key,
                selected,
                departing,
                affected_outputs: affected,
                rev,
                obs: self.staged.observation.observation_seq,
                follow_mover: false,
            }
        } else {
            crate::world_selection::PendingSelection {
                command_seq: Some(command_seq),
                parent_op: format!("op-{command_seq}"),
                focus: None,
                focus_home: target_key,
                selected,
                departing,
                affected_outputs: affected,
                rev,
                obs: self.staged.observation.observation_seq,
                follow_mover: false,
            }
        };
        self.pending_selection = Some(pending);
        if !eligible {
            // Best-effort memory (same contract as the direct focus lane).
            let _ = self.record_command_focus(&window, held_scopes);
            return CommandApplied::Effect(OneShotEffect::explicit_focus(
                command_seq,
                window,
                self.state_revision + 1,
            ));
        }
        // Best-effort memory (same contract as the direct focus lane).
        let _ = self.record_command_focus(&window, held_scopes);
        let mut effect = OneShotEffect::explicit_focus(command_seq, window, rev);
        effect.await_shown = true;
        CommandApplied::Effect(effect)
    }

    /// Remembered R-WS-09 focus candidate for one view target: the
    /// canonical last-active identity first (tile/float/sticky, most
    /// recent wins), then the tiled MRU fallback over the target domain's
    /// view-return candidates (remembered leaf, else MRU recency, else
    /// layout order). Canonical misses fall through when absent,
    /// ineligible (unstaged, held, unexplained fit-excluded, or homed
    /// away), or pruned. Tiled fallback keeps the existing ineligibility
    /// (sticky, floating, unexplained fit-excluded, unstaged). `None`
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
        if let Some(window) = self.last_active_window.get(target).cloned() {
            let eligible = by_id.get(&window).is_some_and(|row| {
                if row.fit_excluded && !row.floating && !row.fullscreen && !row.maximized {
                    return false;
                }
                let Some(homing) = self.staged.homing.get(&window) else {
                    return false;
                };
                if held_scopes.contains(homing) {
                    return false;
                }
                if row.sticky {
                    homing.output == target.output
                } else {
                    homing == target
                }
            });
            if eligible {
                return Some(window);
            }
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
    pub(crate) fn selection_ring(
        &self,
        ctx: &crate::world_engine::RawRouting<'_>,
        output: &OutputId,
    ) -> Vec<String> {
        if self.workspace_mode_setting() == crate::workspace_scope::WorkspaceMode::Shared {
            let mut live: Vec<String> = ctx
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
        ctx: &crate::world_engine::RawRouting<'_>,
        output: &OutputId,
    ) -> Option<Vec<String>> {
        let ring = self.selection_ring(ctx, output);
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
        ctx: &crate::world_engine::RawRouting<'_>,
        explicit: Option<&OutputId>,
    ) -> Option<OutputId> {
        if let Some(output) = explicit {
            return Some(output.clone());
        }
        crate::cursor_targeting::resolve_switch_output(
            self.switch_target_setting(),
            self.fresh_cursor_output(ctx).as_ref(),
            self.focused_output(ctx).as_ref(),
            &self.primary_first_outputs(ctx),
        )
    }

    /// Send ring for a window homed on `output`: the same ring as
    /// selection, resolved once before transfer.
    pub(crate) fn send_ring(
        &self,
        ctx: &crate::world_engine::RawRouting<'_>,
        output: &OutputId,
    ) -> Vec<String> {
        self.selection_ring(ctx, output)
    }

    /// Resolve the migration/control action output from the neutral
    /// report facts: the adapter-grounded foreground output, the cursor
    /// output, the adapter-cached output, then known-registry order. Every
    /// leg filters to the ordered known registry (which retains mapped
    /// stale outputs); a stale mapped leg selects and refuses downstream.
    /// The sole policy owner for migration/control source selection;
    /// explicit command outputs bypass it unchanged.
    pub(crate) fn resolve_action_output(
        &self,
        ctx: &crate::world_engine::RawRouting<'_>,
    ) -> Option<OutputId> {
        crate::cursor_targeting::resolve_action_output(
            ctx.action_foreground,
            self.fresh_cursor_output(ctx).as_ref(),
            ctx.action_cached,
            ctx.action_outputs,
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
        ctx: &crate::world_engine::RawRouting<'_>,
        window: &WindowId,
    ) -> Option<OutputId> {
        if let Some(home) = self
            .members
            .get(window)
            .map(|state| state.home.output.clone())
        {
            return Some(home);
        }
        self.resolve_action_output(ctx)
    }
}
