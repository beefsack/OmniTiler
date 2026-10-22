//! Window send commands (D2/D3): the shared send path for direct,
//! relative, and numbered sends, including floating-workspace boundary
//! transfers and the stable command high-watermark.
//!
//! Split from [`crate::world_commands`]: this module only extends the
//! [`crate::world_engine::WorldEngine`] `impl`.

use std::collections::{BTreeMap, BTreeSet};

use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::engine::Engine;
use crate::ids::{CorrelationId, OwnerId};
use crate::seed::EngineWindow;
use crate::session::RefusalKind;
use crate::session::{DomainKey, OutputDomain};
use crate::workspace_scope::{TrailingSelection, resolve_numbered, resolve_trailing};
use crate::world_commands::CommandApplied;
use crate::world_engine::{
    KIND_CREATION_EXHAUSTED, KIND_IN_FLIGHT, KIND_SCOPE_HELD, KIND_SEND_REFUSED, KIND_SEND_TARGET,
    KIND_SEND_WINDOW, KIND_SNAPSHOT, MSG_CREATION_EXHAUSTED, MSG_IN_FLIGHT, MSG_SCOPE_HELD,
    MSG_SEND_REFUSED, MSG_SEND_TARGET, MSG_SEND_WINDOW, MSG_SNAPSHOT, RetainedTransfer,
    StagedDomain, WorldEngine,
};
use crate::world_report::{OneShotEffect, TransferKind, TransferTarget, WorldReport};

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
        self.settle_send(report, command_seq, operation_id, result)
    }

    /// Cross-output window-send path for direction-only sends
    /// ([`WorldCommand::SendDirectional`]): the target is the strict
    /// topology neighbor's current view, always a different output. Uses
    /// `SendToOutput`/`MoveToOutput` (cross-output only) instead of the
    /// same-output `SendToWorkspace` path above.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start_output_send(
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
        let result = self.execute_output_send(
            report,
            &operation_id,
            window,
            target_output,
            target_workspace,
            follow,
            by_id,
            held_scopes,
        );
        self.settle_send(report, command_seq, operation_id, result)
    }

    /// Settle one executed send: retain the transfer under its stable
    /// operation identity (consuming the command sequence), then emit the
    /// one-shot focus outcome. Follow focuses the mover; stay focuses the
    /// source MRU survivor (`None` when the source is left empty, so no
    /// effect rather than a guessed fallback). Follow also selects the
    /// target view on the same output (core logical authority: the moved
    /// view plus a retained follow-view flight under `{op}-follow`, latest
    /// per output wins exactly like selection; shared mode fans out); stay
    /// never moves any view and retains no flight. The moved view is still
    /// only a desire: native arrival proves separately through the retained
    /// flights, never from this commit.
    pub(crate) fn settle_send(
        &mut self,
        report: &WorldReport,
        command_seq: u64,
        operation_id: String,
        result: Result<(RetainedTransfer, Option<WindowId>), (&'static str, &'static str)>,
    ) -> CommandApplied {
        match result {
            Ok((retained, focus)) => {
                debug_assert_eq!(retained.target.operation_id, operation_id);
                // Single-owner membership commits with the transfer: the
                // mover homes at the target while settlement proves
                // arrival (shielded, never flapping back).
                if let TransferKind::Window(window) = &retained.target.kind {
                    let target = DomainKey {
                        output: retained.target.target_output.clone(),
                        workspace: crate::directional::WorkspaceId(
                            retained.target.target_workspace.clone(),
                        ),
                    };
                    self.commit_send_member(window, &target);
                }
                // Follow owns the same-output target view: without the
                // logical move the adapter's follow switch can never prove
                // (visibility is checked against the core view) and the
                // transfer holds forever. The moved view plus a retained
                // follow-view flight (latest per output wins, exactly like
                // selection) lets the existing view machinery actuate and
                // prove the switch; the window flight still settles only on
                // observed membership. Cross-output targets stay untouched:
                // direction-only sends already target the neighbor's
                // current view.
                if retained.target.follow && retained.target.target_output == retained.source_output
                {
                    let output = retained.target.target_output.clone();
                    let workspace = retained.target.target_workspace.clone();
                    self.retain_view_inner(&output, &workspace, format!("{operation_id}-follow"));
                    if report.workspace_mode == crate::workspace_scope::WorkspaceMode::Shared {
                        let canonical = self.mapping.get(&output.0).cloned().unwrap_or_default();
                        let mut live: Vec<String> = report
                            .screens
                            .iter()
                            .map(|screen| screen.output.0.clone())
                            .collect();
                        live.sort();
                        for out in &live {
                            self.mapping.insert(out.clone(), canonical.clone());
                            self.views.insert(out.clone(), workspace.clone());
                        }
                        self.history.observe_shared(&canonical, &workspace);
                    }
                }
                self.note_command(command_seq);
                self.retained.push(retained);
                match focus {
                    Some(target) => CommandApplied::Effect(OneShotEffect::explicit_focus(
                        command_seq,
                        target,
                        self.state_revision + 1,
                    )),
                    None => CommandApplied::NoEffect,
                }
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
    ) -> Result<(RetainedTransfer, Option<WindowId>), (&'static str, &'static str)> {
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
        // Same-domain sends are honest unchanged no-ops (shared-core
        // `RefusalKind::Unchanged`, no plan): never a native transfer,
        // never a mistargeted failure. This shortcut returns exactly the
        // verdict the Engine ordinary transfer below would produce.
        if target_key == source_key {
            return Err((
                RefusalKind::Unchanged.as_str(),
                RefusalKind::Unchanged.message(),
            ));
        }
        // Outstanding whole-workspace migrations own their destination
        // cell until arrival: a window send onto the same cell would
        // double-book the workspace before the moved unit lands.
        if self.retained.iter().any(|r| {
            matches!(&r.target.kind, TransferKind::Migration)
                && r.target.target_output == *target_output
                && r.target.target_workspace == target_workspace
        }) {
            return Err((KIND_SEND_TARGET, MSG_SEND_TARGET));
        }
        // Logical homing prerequisites: the target workspace lives in its
        // ring and neither key is held.
        if !self
            .mapping
            .get(&target_output.0)
            .is_some_and(|ring| ring.iter().any(|id| id == target_workspace))
        {
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
            // Both scopes must be staged (target readable: the adapter
            // can only verify membership it can see).
            if !self
                .staged
                .domains
                .iter()
                .any(|d| d.readable && d.key == target_key)
            {
                return Err((KIND_SEND_TARGET, MSG_SEND_TARGET));
            }
            if !self.staged.domains.iter().any(|d| d.key == source_key) {
                return Err((KIND_SEND_WINDOW, MSG_SEND_WINDOW));
            }
            return Ok((
                RetainedTransfer {
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
                },
                // Membership-only floating boundary: follow names the mover.
                // Stay keeps the native boundary focus (no engine MRU to
                // name), so no invented fallback is emitted.
                follow.then(|| window.clone()),
            ));
        }
        let owner = report.owner.clone();
        let (Some(owner), Some(correlation)) = (
            OwnerId::parse(&owner),
            CorrelationId::parse(&report.request_id),
        ) else {
            return Err((KIND_SNAPSHOT, MSG_SNAPSHOT));
        };
        let Some(source_domain) = self.staged.domains.iter().find(|d| d.key == source_key) else {
            return Err((KIND_SEND_WINDOW, MSG_SEND_WINDOW));
        };
        let Some(target_domain) = self
            .staged
            .domains
            .iter()
            .find(|d| d.readable && d.key == target_key)
        else {
            return Err((KIND_SEND_TARGET, MSG_SEND_TARGET));
        };
        // Source-only in `windows`, target-only in `target_windows`:
        // the engine combines them for pair convergence, so
        // concatenating here would duplicate every row.
        let source_output_domain = OutputDomain {
            id: source_key.output.clone(),
            workspace: source_key.workspace.clone(),
            bounds: source_domain.bounds,
            gap: source_domain.gap,
            adjacent: BTreeMap::new(),
        };
        let target_output_domain = OutputDomain {
            id: target_key.output.clone(),
            workspace: target_key.workspace.clone(),
            bounds: target_domain.bounds,
            gap: target_domain.gap,
            adjacent: BTreeMap::new(),
        };
        let (Some(inset_source), Some(inset_target)) = (
            crate::world_engine::inset_world_domain(&source_output_domain, source_domain.outer_gap),
            crate::world_engine::inset_world_domain(&target_output_domain, target_domain.outer_gap),
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
            windows: self.shielded_windows(&source_key, &source_domain.windows),
            closed_windows: source_domain.closed.clone(),
            directional: None,
            directional_target_outer_gap: None,
            target_domain: Some((
                Engine::canonical_component_domain(&inset_target),
                target_key.clone(),
            )),
            target_windows: self.shielded_windows(&target_key, &target_domain.windows),
            command: CoreCommand::SendToWorkspace {
                window: window.0.clone(),
                target_output: target_output.0.clone(),
                target_workspace: target_workspace.to_owned(),
                follow,
            },
        };
        match self.engine.handle(&event) {
            CoreReply::SendWorkspace(plan) | CoreReply::SendOutput(plan) => {
                // Follow names the mover. Stay names the engine's
                // source-MRU survivor leaf resolved to its window through
                // the plan geometry; an emptied source names nothing.
                let focus = if follow {
                    Some(window.clone())
                } else {
                    plan.focus_leaf.as_ref().and_then(|leaf| {
                        plan.geometry
                            .iter()
                            .find(|g| &g.leaf == leaf)
                            .map(|g| g.window.clone())
                    })
                };
                Ok((
                    RetainedTransfer {
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
                    },
                    focus,
                ))
            }
            // Engine refusals are fixed redacted vocabularies: safe
            // to forward without echoing input.
            CoreReply::Rejected { kind, message } => Err((kind, message)),
            CoreReply::SnapshotInvalid { message, detail } => Err((detail, message)),
            CoreReply::Diverged(reason) => Err((reason.as_str(), reason.message())),
            _ => Err((KIND_SNAPSHOT, MSG_SNAPSHOT)),
        }
    }

    /// Cross-output tiled send for direction-only intents: identical to
    /// [`WorldEngine::execute_send`] except the Engine event carries
    /// `SendToOutput`/`MoveToOutput` (cross-output only). Same-output
    /// targets refuse as `CrossDomainMismatch` so the two ops stay
    /// distinct. Floating workspaces retain membership-only exactly like
    /// the same-output path.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn execute_output_send(
        &mut self,
        report: &WorldReport,
        operation_id: &str,
        window: &WindowId,
        target_output: &OutputId,
        target_workspace: &str,
        follow: bool,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Result<(RetainedTransfer, Option<WindowId>), (&'static str, &'static str)> {
        if self.retained.iter().any(|r| match &r.target.kind {
            TransferKind::Window(w) => w == window,
            TransferKind::View | TransferKind::Migration => false,
        }) {
            return Err((KIND_IN_FLIGHT, MSG_IN_FLIGHT));
        }
        let Some(source_row) = by_id.get(window) else {
            return Err((KIND_SEND_WINDOW, MSG_SEND_WINDOW));
        };
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
        // Outstanding whole-workspace migrations own their destination
        // cell until arrival: a window send onto the same cell would
        // double-book the workspace before the moved unit lands.
        if self.retained.iter().any(|r| {
            matches!(&r.target.kind, TransferKind::Migration)
                && r.target.target_output == *target_output
                && r.target.target_workspace == target_workspace
        }) {
            return Err((KIND_SEND_TARGET, MSG_SEND_TARGET));
        }
        // Logical homing prerequisites: the target workspace lives in its
        // ring and neither key is held.
        if !self
            .mapping
            .get(&target_output.0)
            .is_some_and(|ring| ring.iter().any(|id| id == target_workspace))
        {
            return Err((KIND_SEND_TARGET, MSG_SEND_TARGET));
        }
        if held_scopes.contains(&source_key) || held_scopes.contains(&target_key) {
            return Err((KIND_SCOPE_HELD, MSG_SCOPE_HELD));
        }
        let source_floating = self.floating_scopes.contains(&source_key);
        let target_floating = self.floating_scopes.contains(&target_key);
        if source_floating || target_floating {
            if !self
                .staged
                .domains
                .iter()
                .any(|d| d.readable && d.key == target_key)
            {
                return Err((KIND_SEND_TARGET, MSG_SEND_TARGET));
            }
            if !self.staged.domains.iter().any(|d| d.key == source_key) {
                return Err((KIND_SEND_WINDOW, MSG_SEND_WINDOW));
            }
            return Ok((
                RetainedTransfer {
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
                },
                // Membership-only floating boundary: follow names the mover.
                // Stay keeps the native boundary focus (no engine MRU to
                // name), so no invented fallback is emitted.
                follow.then(|| window.clone()),
            ));
        }
        let owner = report.owner.clone();
        let (Some(owner), Some(correlation)) = (
            OwnerId::parse(&owner),
            CorrelationId::parse(&report.request_id),
        ) else {
            return Err((KIND_SNAPSHOT, MSG_SNAPSHOT));
        };
        let Some(source_domain) = self.staged.domains.iter().find(|d| d.key == source_key) else {
            return Err((KIND_SEND_WINDOW, MSG_SEND_WINDOW));
        };
        let Some(target_domain) = self
            .staged
            .domains
            .iter()
            .find(|d| d.readable && d.key == target_key)
        else {
            return Err((KIND_SEND_TARGET, MSG_SEND_TARGET));
        };
        let source_output_domain = OutputDomain {
            id: source_key.output.clone(),
            workspace: source_key.workspace.clone(),
            bounds: source_domain.bounds,
            gap: source_domain.gap,
            adjacent: BTreeMap::new(),
        };
        let target_output_domain = OutputDomain {
            id: target_key.output.clone(),
            workspace: target_key.workspace.clone(),
            bounds: target_domain.bounds,
            gap: target_domain.gap,
            adjacent: BTreeMap::new(),
        };
        let (Some(inset_source), Some(inset_target)) = (
            crate::world_engine::inset_world_domain(&source_output_domain, source_domain.outer_gap),
            crate::world_engine::inset_world_domain(&target_output_domain, target_domain.outer_gap),
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
            windows: self.shielded_windows(&source_key, &source_domain.windows),
            closed_windows: source_domain.closed.clone(),
            directional: None,
            directional_target_outer_gap: None,
            target_domain: Some((
                Engine::canonical_component_domain(&inset_target),
                target_key.clone(),
            )),
            target_windows: self.shielded_windows(&target_key, &target_domain.windows),
            command: CoreCommand::SendToOutput {
                window: window.0.clone(),
                target_output: target_output.0.clone(),
                target_workspace: target_workspace.to_owned(),
                follow,
            },
        };
        match self.engine.handle(&event) {
            CoreReply::SendWorkspace(plan) | CoreReply::SendOutput(plan) => {
                // Follow names the mover. Stay names the engine's
                // source-MRU survivor leaf resolved to its window through
                // the plan geometry; an emptied source names nothing.
                let focus = if follow {
                    Some(window.clone())
                } else {
                    plan.focus_leaf.as_ref().and_then(|leaf| {
                        plan.geometry
                            .iter()
                            .find(|g| &g.leaf == leaf)
                            .map(|g| g.window.clone())
                    })
                };
                Ok((
                    RetainedTransfer {
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
                    },
                    focus,
                ))
            }
            CoreReply::Rejected { kind, message } => Err((kind, message)),
            CoreReply::SnapshotInvalid { message, detail } => Err((detail, message)),
            CoreReply::Diverged(reason) => Err((reason.as_str(), reason.message())),
            _ => Err((KIND_SNAPSHOT, MSG_SNAPSHOT)),
        }
    }

    /// Numbered send over the scoped ring for `output`: positional
    /// `1..=9`, or `0` for the trailing path (reuse the live trailing
    /// empty, else mint the trailing workspace immediately, exactly like
    /// lifecycle). Commits an ordinary transfer; follow additionally selects
    /// the target view while stays never view-switch.
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
        // Numbered sends are same-output workspace routing: the public
        // output must name the mover's own output. A foreign output would
        // resolve the wrong ring and (on the Create path) mint into the
        // wrong mapping, while the Engine refuses cross-output workspace
        // sends as `CrossDomainMismatch` at the boundary anyway. Refuse up
        // front with those exact semantics and no mutation.
        if source_row.output != *output {
            return CommandApplied::Rejected {
                kind: RefusalKind::CrossDomainMismatch.as_str(),
                message: RefusalKind::CrossDomainMismatch.message(),
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
            let occupied = self.ring_occupied(output);
            let selection = resolve_trailing(&ring, &occupied);
            match selection {
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
                    self.settle_send(report, command_seq, operation_id, result)
                }
                TrailingSelection::Create => {
                    // Trailing workspaces exist immediately in the logical
                    // contract: mint, stage an empty readable domain for the
                    // fresh id, then commit the send. Without the staged
                    // domain the fresh target below would refuse as
                    // `send-target` even though the mapping already names
                    // it. Geometry always derives from the destination
                    // screen (identical to the source domain on the guarded
                    // same output); the source domain is only a fallback
                    // and never a different screen.
                    let created = self.create_trailing_workspace(output, report.default_tiled);
                    let Some(target) = created else {
                        return CommandApplied::Rejected {
                            kind: KIND_CREATION_EXHAUSTED,
                            message: MSG_CREATION_EXHAUSTED,
                        };
                    };
                    let target_key = DomainKey {
                        output: output.clone(),
                        workspace: WorkspaceId(target.clone()),
                    };
                    let staged_geometry = self
                        .staged
                        .screens
                        .iter()
                        .find(|screen| screen.output == *output)
                        .and_then(|screen| {
                            screen
                                .work_area
                                .map(|bounds| (bounds, screen.gap, screen.outer_gap))
                        })
                        .or_else(|| {
                            (source_key.output == *output)
                                .then(|| {
                                    self.staged
                                        .domains
                                        .iter()
                                        .find(|d| d.key == source_key)
                                        .map(|d| (d.bounds, d.gap, d.outer_gap))
                                })
                                .flatten()
                        });
                    let Some((bounds, gap, outer_gap)) = staged_geometry else {
                        return CommandApplied::Rejected {
                            kind: KIND_SEND_TARGET,
                            message: MSG_SEND_TARGET,
                        };
                    };
                    self.staged.domains.push(StagedDomain {
                        key: target_key.clone(),
                        bounds,
                        gap,
                        outer_gap,
                        windows: Vec::new(),
                        focused: WindowId(String::new()),
                        minimized: Vec::new(),
                        unreadable: Vec::new(),
                        closed: Vec::new(),
                        readable: true,
                        hold: None,
                    });
                    if self
                        .tiling
                        .get(&target_key)
                        .is_some_and(|mode| *mode == crate::world_report::TilingMode::Floating)
                    {
                        self.floating_scopes.insert(target_key);
                    }
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
                    self.settle_send(report, command_seq, operation_id, result)
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
            self.settle_send(report, command_seq, operation_id, result)
        }
    }
}
