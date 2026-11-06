//! World desired-state publishing (D1-D5): one full snapshot per accepted
//! report.
//!
//! Split from [`crate::world_engine`]: retained-session projection, the
//! planner-owned logical workspace lifecycle, output views, and explicit
//! lifetime/hold rows. Never authorizes writes inside held scopes.

use std::collections::{BTreeMap, BTreeSet};

use crate::boundary::{CoreReply, ProjectionKind, project_retained_tiled_geometry};
use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::session::{DomainKey, RefusalKind};
use crate::world_engine::WorldEngine;
use crate::world_report::{
    CommandResult, FloatOrigin, HoldReason, IntentWrite, OneShotEffect, OutputDesire,
    ScopeAuthority, ScopeStatus, ScreenView, TilingMode, TransferKind,
    WORLD_CONTRACT_VERSION_LEGACY, WindowDesire, WorkspaceDesire, WorldCommand, WorldReply,
    WorldReport, workspace_authority_scope,
};

/// Shared assembled publication rows: every known logical scope plus
/// the one-homing window rows, with scope collapse/sort applied. Both
/// the legacy and v4 publication paths build their envelopes from this
/// single assembly; only the report-dependent lanes (envelope identity,
/// output views, takes, saves) differ per path.
pub(crate) struct PublicationRows {
    pub scopes: Vec<ScopeStatus>,
    pub windows: Vec<WindowDesire>,
}

impl WorldEngine {
    /// Sticky-command save drain plus proven-refusal rollback merge plus
    /// ordinary ToggleFloat fallback. Queued rollback writes (at most one
    /// per window, latest wins) drain exactly once on every non-rejected
    /// reply: fresh command replies, commandless observation replies,
    /// consumed-command replies, and saturated-generation replies all emit
    /// them, because the prior they restore is exact known state, not a
    /// new unsafe save. A matching fresh sticky-command save supersedes
    /// that window's rollback (the successful transition replaces the
    /// deferred record); an unrelated non-matching queued save is dropped
    /// exactly as before and never erases a rollback. Anything else falls
    /// back to the existing ordinary `toggle_float_intent_save`
    /// (consumed/saturated/blocks still suppress there), whose same-window
    /// fresh save likewise wins over a stale rollback. Consumed and
    /// saturated replies suppress only command and ordinary saves. Every
    /// resulting write upserts bounded `sticky_saved` knowledge (`Some`
    /// decodes and stores, `None` clears) for future native rollback;
    /// undecodable rows are ignored. Import and late-input policy are
    /// untouched (see `ingest_staged_intent`). The rollback lane reserves
    /// [`crate::world_sizing::max_intent_rollback_bound`] at the
    /// pre-mutation gate; the merged section stays under
    /// [`crate::world_report::MAX_INTENT_WRITES`] via the existing commit
    /// gate (over-cap rejects without adopting, never a silent drop).
    pub(crate) fn drain_intent_saves(
        &mut self,
        command: Option<&WorldCommand>,
        command_consumed: bool,
        command_result: &Option<CommandResult>,
    ) -> Vec<IntentWrite> {
        let queued = self.sticky_command_save.take();
        // Proven-refusal rollback writes drain on every non-rejected reply:
        // commandless, consumed, and saturated replies still emit them.
        // Rejected envelopes drop the whole staged candidate upstream, so
        // nothing leaks there.
        let mut merged: Vec<IntentWrite> = std::mem::take(&mut self.sticky_rollback_saves)
            .into_values()
            .collect();
        if command_consumed || self.intent_sticky_saturated {
            // Consumed/saturated suppress command and ordinary saves, never
            // rollback: the prior is exact known state, not a new save.
            self.upsert_intent_save_knowledge(&merged);
            return merged;
        }
        if let Some(save) = queued {
            let matches = match command {
                Some(WorldCommand::SetSticky { window, .. }) => window == &save.win,
                Some(WorldCommand::ToggleSticky { window }) => window == &save.win,
                Some(WorldCommand::Operation(crate::boundary::CoreCommand::ToggleFloat {
                    window,
                    ..
                })) => WindowId(window.clone()) == save.win,
                _ => false,
            };
            if matches {
                // A fresh same-window save wins over a pending rollback.
                merged.retain(|write| write.win != save.win);
                merged.push(save);
                self.upsert_intent_save_knowledge(&merged);
                return merged;
            }
        }
        let ordinary = self.toggle_float_intent_save(command, false, command_result);
        if !ordinary.is_empty() {
            // A fresh same-window ordinary save wins over a stale rollback.
            for write in &ordinary {
                merged.retain(|queued| queued.win != write.win);
            }
            merged.extend(ordinary);
        }
        self.upsert_intent_save_knowledge(&merged);
        merged
    }

    /// Bounded latest-wins `sticky_saved` knowledge from published writes.
    /// A cap drop latches [`Self::sticky_saved_incomplete`] fail-closed so
    /// native priors stay unknown even after later prunes.
    fn upsert_intent_save_knowledge(&mut self, writes: &[IntentWrite]) {
        for write in writes {
            match write.data.as_deref() {
                Some(raw) => {
                    let Ok(data) = crate::intent_data::decode_intent_data(raw) else {
                        continue;
                    };
                    if self.sticky_saved.contains_key(&write.win)
                        || self.sticky_saved.len() < crate::world_report::MAX_INTENT_RECORDS
                    {
                        self.sticky_saved.insert(write.win.clone(), data);
                    } else {
                        self.sticky_saved_incomplete = true;
                    }
                }
                None => {
                    self.sticky_saved.remove(&write.win);
                }
            }
        }
    }

    /// At-most-one canonical explicit float/tile save for a successful
    /// non-consumed ToggleFloat: exactly
    /// `{"v":1,"float":<bool>,"stickyPrior":null}` for the command window,
    /// published in the same reply before any dependent adapter write.
    /// The bool is verified from both the command result variant and the
    /// committed post-state, which must agree: a float save needs a
    /// non-automatic exception, an unfloat save needs a tiled link (the
    /// fixed-tile override already committed transactionally through the
    /// lifecycle suppress origin; the save records it, never clears it).
    /// A no-space retry (`rect: None`, `no_space: true`) emits nothing:
    /// the window never left float, so no tile may be manufactured and
    /// no redundant float is published. Consumed, rejected, held,
    /// non-toggle, restore, and bootstrap reports emit nothing, as do
    /// reports under a sticky block or the saturation latch. Duplicate
    /// requests repeat the cached reply (and its save); a later consumed
    /// command report carries no Float result and emits nothing.
    pub(crate) fn toggle_float_intent_save(
        &self,
        command: Option<&WorldCommand>,
        command_consumed: bool,
        command_result: &Option<CommandResult>,
    ) -> Vec<IntentWrite> {
        if command_consumed || self.intent_sticky_saturated {
            return Vec::new();
        }
        let window = match command {
            Some(WorldCommand::Operation(crate::boundary::CoreCommand::ToggleFloat {
                window,
                ..
            })) => WindowId(window.clone()),
            _ => return Vec::new(),
        };
        if self.intent_sticky_blocks.contains(&window) {
            return Vec::new();
        }
        let (result_window, rect, no_space) = match command_result {
            Some(CommandResult::Float {
                window,
                rect,
                no_space,
            }) => (window, rect, no_space),
            _ => return Vec::new(),
        };
        if result_window != &window || *no_space {
            return Vec::new();
        }
        let float = if rect.is_some() {
            if !self.is_exception_window_any(&window)
                || self.engine.is_automatic_fixed_float_any(&window)
            {
                return Vec::new();
            }
            true
        } else {
            if !self.is_tiled_window_any(&window) {
                return Vec::new();
            }
            false
        };
        vec![IntentWrite {
            win: window,
            data: Some(crate::intent_data::encode_intent_data(
                &crate::intent_data::IntentData::new(Some(float), None),
            )),
        }]
    }

    /// One-place hold classification for sweep failures (capacity B).
    /// This is the single replaceable policy: only proven geometric
    /// exhaustion keeps topology and holds as [`HoldReason::ConstraintClamp`].
    /// In the sweep path the input is already validated and the command is
    /// always `Reconcile`, so the two seed/projection refusal shapes mean
    /// the layout ran out of representable pixels
    /// (`extent - gap*(children-1) < children` with no positive child
    /// segment, R-MIN-04/R-INS-09): `malformed-topology` from retained-tree
    /// projection or `seed-failed` from the seed build
    /// (`Session::new` inputs are pre-validated, so only the geometric
    /// admit steps can still fail there). Best-effort admission (D2/D3/D7,
    /// selected 2026-10-17) floats or overlap-admits ordinary geometric
    /// exhaustion before this classifier runs, so a surviving hold here
    /// means a protected remainder (overlays, explicit fixed-tile wins,
    /// unsafe or excluded members), nonpositive usable area, or invalid
    /// topology: never a floatable newcomer. Every other sweep failure
    /// (divergence, pending, partial observation, commit rejection, ...)
    /// holds as [`HoldReason::Unreadable`]: the adapter must not write
    /// there, and recovery retries the full report.
    pub(crate) fn classify_sweep_hold(reply: &CoreReply) -> HoldReason {
        match reply {
            CoreReply::Rejected { kind, .. }
                if *kind == RefusalKind::MalformedTopology.as_str() =>
            {
                HoldReason::ConstraintClamp
            }
            CoreReply::SnapshotInvalid { detail, .. } if *detail == "seed-failed" => {
                HoldReason::ConstraintClamp
            }
            _ => HoldReason::Unreadable,
        }
    }

    /// Assemble the full desired snapshot from retained sessions, the
    /// logical trailing lifecycle, retained transfers/views, and new
    /// effects. Every known logical workspace gets an explicit scope row
    /// (never silent absence); minimized lifetimes are explicit
    /// minimized-hold rows (never omission, never inferred close); held
    /// scopes carry their retained lifetimes as hold rows. Logical
    /// authority means the target of an outstanding migration publishes
    /// Controlled membership desire before any native arrival (frames hold
    /// until arrival proof); cross-output tiled movers (window transfers
    /// and migration members) additionally carry their actionable target
    /// slot (core-owned geometry actuated only as part of the membership
    /// move, never a native remap guess); unchanged reports never flap
    /// the source.
    /// Floating scopes carry explicit floating lifetime rows (never
    /// emptiness, `floating: true` with empty `float_origin`): observed
    /// non-movers publish with no authority, and movers under an
    /// outstanding boundary transfer publish one membership-only row at
    /// the target homing. No floating row ever carries allocation or
    /// frame authority (no frame writes on floats).
    /// A held scope with no retained session carries its staged lifetimes
    /// as complete held rows, so no inventory ever vanishes.
    /// Whether one window rides an outstanding transfer whose source or
    /// target homes on the style-held output. Narrow v4 presentation-style
    /// hold only: legacy passes `None` and never holds here.
    fn style_flight_held(&self, style_held_output: Option<&OutputId>, window: &WindowId) -> bool {
        let Some(held) = style_held_output else {
            return false;
        };
        self.retained.iter().any(|retained| {
            let carries = match &retained.target.kind {
                TransferKind::Window(w) => w == window,
                TransferKind::Migration => retained.members.contains(window),
                TransferKind::View => false,
            };
            carries && (retained.target.target_output == *held || retained.source_output == *held)
        })
    }

    /// Effective born-pause for one home output under the narrow v4
    /// style hold: held homes withhold born-clear tokens like a paused
    /// presentation. Legacy (`None`) is identity.
    fn style_born_paused(
        born_paused: bool,
        style_held_output: Option<&OutputId>,
        home_output: &OutputId,
    ) -> bool {
        born_paused || style_held_output.is_some_and(|held| held == home_output)
    }

    /// Assemble the shared publication rows from staged/retained state.
    /// `born_paused` gates the born-clear token lane (legacy passes the
    /// report presentation pause; v4 passes false: suspension is not a
    /// write hold). `effects` feed the focus record/retain/freeze lanes.
    /// `style_held_output` is the narrow v4 presentation-style hold (v4
    /// path only; legacy passes `None`): born tokens withhold, flight
    /// rows hold, and the minimized tail falls back to it. Never reads a
    /// caller report.
    pub(crate) fn assemble_publication_rows(
        &mut self,
        born_paused: bool,
        effects: &[OneShotEffect],
        held_scopes: &BTreeSet<DomainKey>,
        style_held_output: Option<&OutputId>,
    ) -> PublicationRows {
        self.record_published_focus(effects, held_scopes);
        // Freeze the actually emitted ordinary focus batch: a newer batch
        // supersedes the older one, an effect-free reply retains it.
        self.retain_published_focus(effects);
        // Command-created logicals land after convergence: mint their
        // explicit creation desires here (idempotent with the pre-command
        // ensure) so the same reply authorizes them. Same-reply desires
        // share the stable per-op identity the next report settles. Then
        // drop authorizations that no longer need backing (e.g. minted
        // against a stale view that fresh evidence rehomed): stale state
        // never recreates. Ambiguous rows persist for their visible hold.
        if self.host_owned_workspace_domains() {
            if self.workspace_mode_setting() == crate::workspace_scope::WorkspaceMode::Shared {
                let mut ordered: Vec<String> = self
                    .staged
                    .screens
                    .iter()
                    .map(|screen| screen.output.0.clone())
                    .collect();
                ordered.sort();
                self.ensure_native_creations_shared(&ordered);
            } else {
                let live: Vec<String> = self
                    .staged
                    .screens
                    .iter()
                    .map(|screen| screen.output.0.clone())
                    .collect();
                for output in &live {
                    self.ensure_native_creations(output);
                }
            }
            self.prune_unneeded_creations();
        }
        let staged = self.staged.clone();
        let mut scopes = Vec::with_capacity(staged.domains.len());
        let mut windows = Vec::new();
        // Pre-clear born overlay isolation (REQ-MAX-01/06): homes with an
        // outstanding born binding hold their geometry until the clear
        // lands. Birth-specific only: retained maximized slots (no
        // binding) and keep-maximized mode (no binding) keep their
        // reservations. Publish-only: commands still see the unheld
        // scopes so ToggleFloat refusal stays `not-tiled`.
        let born_held: BTreeSet<DomainKey> = self
            .retained_born
            .iter()
            .map(|held| held.home.clone())
            .collect();
        // Homed lifetimes in readable, unheld, non-floating scopes carry
        // live rows. Minimized and held-scope rows never shadow a live row
        // for the same lifetime: one homing per window.
        let mut live: BTreeSet<String> = BTreeSet::new();
        for domain in &staged.domains {
            if !domain.readable
                || held_scopes.contains(&domain.key)
                || born_held.contains(&domain.key)
                || self.floating_scopes.contains(&domain.key)
            {
                continue;
            }
            for window in &domain.windows {
                live.insert(window.window.0.clone());
            }
            if let Some(session) = self.engine.session(&domain.key) {
                for observed in session.exception_observed() {
                    if observed.output == domain.key.output
                        && observed.workspace == domain.key.workspace
                    {
                        live.insert(observed.window.0.clone());
                    }
                }
            }
        }
        // Backstop dedupe among non-live rows themselves.
        let mut emitted: BTreeSet<String> = BTreeSet::new();
        // Movers with an outstanding transfer whose staged homing has not
        // yet reached the target keep one homing at the target: the
        // logical target is authoritative before any arrival, so
        // membership desire is Controlled. Cross-output tiled movers
        // (window transfers and migration members) whose target scope
        // projects ride their sweep row (actionable slot included);
        // every other flight (no Engine plan, unprojectable scopes)
        // presents one membership-only row at the target homing below.
        // Arrived movers already home at the target and ride their
        // sweep rows below.
        let mut redirect: BTreeMap<String, DomainKey> = BTreeMap::new();
        for retained in &self.retained {
            let (window, key) = match &retained.target.kind {
                TransferKind::Window(window) => (
                    window.clone(),
                    DomainKey {
                        output: retained.target.target_output.clone(),
                        workspace: WorkspaceId(retained.target.target_workspace.clone()),
                    },
                ),
                TransferKind::Migration => continue,
                TransferKind::View => continue,
            };
            // Style-held targets still emit a Hold row below (gated by
            // `style_flight_held`); only non-style holds suppress the
            // mover row here.
            if held_scopes.contains(&key)
                && style_held_output.is_none_or(|held| *held != key.output)
            {
                continue;
            }
            // Narrowly held rows keep their retained homes, never a mover
            // row; exclusions never ride transfers.
            if self.is_held_window(&window) || self.is_excluded_window(&window) {
                continue;
            }
            if self.staged_homing(&window).as_ref() == Some(&key) {
                continue;
            }
            redirect.insert(window.0.clone(), key);
        }
        for retained in &self.retained {
            if !matches!(&retained.target.kind, TransferKind::Migration) {
                continue;
            }
            let key = DomainKey {
                output: retained.target.target_output.clone(),
                workspace: WorkspaceId(retained.target.target_workspace.clone()),
            };
            if held_scopes.contains(&key)
                && style_held_output.is_none_or(|held| *held != key.output)
            {
                continue;
            }
            for member in &retained.members {
                // Narrow holds and exclusions never ride mover rows.
                if self.is_held_window(member) || self.is_excluded_window(member) {
                    continue;
                }
                if self.staged_homing(member).as_ref() == Some(&key) {
                    continue;
                }
                redirect.entry(member.0.clone()).or_insert(key.clone());
            }
        }
        // Fullscreen overlays on tiled links: the native overlay owns
        // pixels, so no automatic frame authority ever publishes even
        // though the tile stays reserved (allocation present, no
        // effective frame). The adapter overlay guard double-checks.
        let fullscreen_windows: BTreeSet<&str> = staged
            .facts
            .values()
            .filter(|fact| fact.fullscreen == Some(true))
            .map(|fact| fact.window.0.as_str())
            .collect();
        for domain in &staged.domains {
            let key = domain.key.clone();
            // Floating workspaces are adapter-owned: the scope is
            // explicitly untouched. Observed lifetimes still publish
            // explicit floating rows (`floating: true`, empty
            // `float_origin`; never silent absence, never a frame
            // write): non-movers with no authority at their observed
            // homing; movers under an outstanding boundary transfer
            // publish one membership-only row at the target homing
            // below, so they emit nothing here (one homing).
            if self.floating_scopes.contains(&key) {
                scopes.push(ScopeStatus {
                    scope: workspace_authority_scope(&key.output, &key.workspace.0),
                    authority: ScopeAuthority::Untouched,
                });
                for window in &domain.windows {
                    // Floating rows here; redirected movers ride the
                    // target homing below (one homing).
                    if redirect.contains_key(&window.window.0) {
                        continue;
                    }
                    if !emitted.insert(window.window.0.clone()) {
                        continue;
                    }
                    let boundary_mover = self.retained.iter().any(|r| {
                        matches!(&r.target.kind, TransferKind::Window(w) if w == &window.window)
                            && r.target.target_output == key.output
                            && r.target.target_workspace == key.workspace.0
                    });
                    windows.push(WindowDesire {
                        lifetime: window.window.clone(),
                        output: window.output.clone(),
                        workspace: window.workspace.0.clone(),
                        allocation: None,
                        effective_frame: None,
                        frame_authority: ScopeAuthority::Untouched,
                        membership_authority: if self
                            .style_flight_held(style_held_output, &window.window)
                        {
                            ScopeAuthority::Hold(HoldReason::Unreadable)
                        } else if boundary_mover {
                            ScopeAuthority::Controlled
                        } else {
                            ScopeAuthority::Untouched
                        },
                        frame_operation: None,
                        clear_operation: None,
                        floating: true,
                        float_origin: None,
                        command_seq: None,
                        visibility: None,
                        visibility_authority: ScopeAuthority::Untouched,
                        state: Vec::new(),
                    });
                }
                for window in &domain.minimized {
                    if redirect.contains_key(&window.0) {
                        continue;
                    }
                    if !emitted.insert(window.0.clone()) {
                        continue;
                    }
                    windows.push(WindowDesire {
                        lifetime: window.clone(),
                        output: key.output.clone(),
                        workspace: key.workspace.0.clone(),
                        allocation: None,
                        effective_frame: None,
                        frame_authority: ScopeAuthority::Hold(HoldReason::Minimized),
                        membership_authority: ScopeAuthority::Untouched,
                        frame_operation: None,
                        clear_operation: None,
                        floating: true,
                        float_origin: None,
                        command_seq: None,
                        visibility: None,
                        visibility_authority: ScopeAuthority::Untouched,
                        state: Vec::new(),
                    });
                }
                continue;
            }
            let authority = if !domain.readable {
                ScopeAuthority::Hold(domain.hold.unwrap_or(HoldReason::Unreadable))
            } else if held_scopes.contains(&key) || born_held.contains(&key) {
                ScopeAuthority::Hold(
                    self.hold_reasons
                        .get(&key)
                        .copied()
                        .unwrap_or(HoldReason::Unreadable),
                )
            } else {
                ScopeAuthority::Controlled
            };
            scopes.push(ScopeStatus {
                scope: workspace_authority_scope(&key.output, &key.workspace.0),
                authority,
            });
            if !matches!(authority, ScopeAuthority::Controlled) {
                // Explicit lifetime rows for held scopes: retained membership
                // with narrow holds, never silent absence. Omission inside a
                // held scope authorizes no write or retirement; these rows
                // make the retained lifetimes visible instead.
                if let Some(session) = self.engine.session(&key) {
                    let reason = match authority {
                        ScopeAuthority::Hold(reason) => reason,
                        _ => HoldReason::Unreadable,
                    };
                    self.push_held_lifetimes(
                        Self::style_born_paused(born_paused, style_held_output, &key.output),
                        session,
                        &key,
                        reason,
                        &live,
                        &mut emitted,
                        &mut windows,
                    );
                } else {
                    // No retained session (fresh-seed geometric exhaustion):
                    // the staged lifetimes ARE the complete held inventory.
                    // One row per homed fact with no allocation and no
                    // write authority, mirroring the retained held rows, so
                    // recovery (fewer windows, larger bounds) re-adopts from
                    // the next full report with no topology churn in between.
                    let reason = match authority {
                        ScopeAuthority::Hold(reason) => reason,
                        _ => HoldReason::Unreadable,
                    };
                    for window in &domain.windows {
                        if live.contains(&window.window.0)
                            || !emitted.insert(window.window.0.clone())
                        {
                            continue;
                        }
                        // Held scopes authorize no frame/member write, but
                        // an outstanding born clear still rides its token:
                        // maximize-clear is a property transition proven by
                        // the window's own birth facts, not a geometry
                        // write, so the adapter can consume it while the
                        // geometry hold stands. The narrow style hold
                        // withholds even this token on its output.
                        let clear_operation = self.born_desire_for(
                            &window.window,
                            Self::style_born_paused(born_paused, style_held_output, &window.output),
                        );
                        windows.push(WindowDesire {
                            lifetime: window.window.clone(),
                            output: window.output.clone(),
                            workspace: window.workspace.0.clone(),
                            allocation: None,
                            effective_frame: None,
                            frame_authority: ScopeAuthority::Hold(reason),
                            membership_authority: ScopeAuthority::Hold(reason),
                            frame_operation: None,
                            clear_operation,
                            floating: false,
                            float_origin: None,
                            command_seq: None,
                            visibility: None,
                            visibility_authority: ScopeAuthority::Untouched,
                            state: Vec::new(),
                        });
                    }
                    for window in &domain.minimized {
                        if live.contains(&window.0) || !emitted.insert(window.0.clone()) {
                            continue;
                        }
                        windows.push(WindowDesire {
                            lifetime: window.clone(),
                            output: key.output.clone(),
                            workspace: key.workspace.0.clone(),
                            allocation: None,
                            effective_frame: None,
                            frame_authority: ScopeAuthority::Hold(HoldReason::Minimized),
                            membership_authority: ScopeAuthority::Hold(reason),
                            frame_operation: None,
                            clear_operation: None,
                            floating: false,
                            float_origin: None,
                            command_seq: None,
                            visibility: None,
                            visibility_authority: ScopeAuthority::Untouched,
                            state: Vec::new(),
                        });
                    }
                    for window in &domain.unreadable {
                        if !emitted.insert(window.0.clone()) {
                            continue;
                        }
                        windows.push(WindowDesire {
                            lifetime: window.clone(),
                            output: key.output.clone(),
                            workspace: key.workspace.0.clone(),
                            allocation: None,
                            effective_frame: None,
                            frame_authority: ScopeAuthority::Hold(reason),
                            membership_authority: ScopeAuthority::Hold(reason),
                            frame_operation: None,
                            clear_operation: None,
                            floating: false,
                            float_origin: None,
                            command_seq: None,
                            visibility: None,
                            visibility_authority: ScopeAuthority::Untouched,
                            state: Vec::new(),
                        });
                    }
                }
                continue;
            }
            let Some(session) = self.engine.session(&key) else {
                continue;
            };
            // Sole-minimized scope (MNZ-03): no tiled members and no
            // exceptions homed here but live minimized slots are: the scope
            // stays controlled and the slots publish explicit minimized
            // rows, never a projection failure hold.
            let tiled_here = session
                .snapshot()
                .windows
                .iter()
                .any(|link| link.output == key.output && link.workspace == key.workspace);
            let exceptions_here = session.exception_observed().iter().any(|observed| {
                observed.output == key.output && observed.workspace == key.workspace
            });
            if !tiled_here && !exceptions_here {
                let slots: Vec<_> = session
                    .minimized_entries()
                    .into_iter()
                    .filter(|(_, output, workspace)| {
                        output == &key.output && workspace == &key.workspace
                    })
                    .collect();
                if !slots.is_empty() {
                    for (window, output, workspace) in slots {
                        if live.contains(&window.0) || !emitted.insert(window.0.clone()) {
                            continue;
                        }
                        windows.push(WindowDesire {
                            lifetime: window,
                            output: output.clone(),
                            workspace: workspace.0.clone(),
                            allocation: None,
                            effective_frame: None,
                            frame_authority: ScopeAuthority::Hold(HoldReason::Minimized),
                            membership_authority: ScopeAuthority::Controlled,
                            frame_operation: None,
                            clear_operation: None,
                            floating: false,
                            float_origin: None,
                            command_seq: None,
                            visibility: None,
                            visibility_authority: ScopeAuthority::Untouched,
                            state: Vec::new(),
                        });
                    }
                    continue;
                }
            }
            // Tiled geometry projects only when the retained session homes
            // tiled links here. An exception-only session (sole tile
            // toggled float, born/fixed floats, all tiles closed or
            // minimized) carries no tree, which is a valid logical state:
            // the scope stays Controlled and the exception/minimized rows
            // below publish with no frame writes, instead of a
            // constraint-clamp hold for a projection that has no subject.
            // A missing plan while tiled links exist keeps the existing
            // hold: geometric exhaustion, never an unreadable scope.
            let frame_hold = staged.gesture_barrier;
            let plan = if tiled_here {
                let shielded = self.shielded_windows(&key, &domain.windows);
                let mut hints = BTreeMap::new();
                for window in &shielded {
                    hints.insert(window.window.clone(), window.hints);
                }
                let focus = session.focus();
                let (focus_domain, focus_leaf) = focus;
                let focus_pair = match (focus_domain, focus_leaf) {
                    (Some(domain), Some(leaf)) => Some((domain, leaf)),
                    _ => None,
                };
                // Project the outer-inset work area, never the raw client
                // area. Inset exhaustion here mirrors the sweep hold (the
                // sweep already held, so this is only a backstop).
                let staged_output = crate::session::OutputDomain {
                    id: key.output.clone(),
                    workspace: key.workspace.clone(),
                    bounds: domain.bounds,
                    gap: domain.gap,
                    adjacent: BTreeMap::new(),
                };
                let Some(inset) =
                    crate::world_engine::inset_world_domain(&staged_output, domain.outer_gap)
                else {
                    if let Some(scope) = scopes.last_mut() {
                        scope.authority = ScopeAuthority::Hold(HoldReason::ConstraintClamp);
                    }
                    for window in &domain.windows {
                        live.remove(&window.window.0);
                    }
                    self.push_held_lifetimes(
                        born_paused,
                        session,
                        &key,
                        HoldReason::ConstraintClamp,
                        &live,
                        &mut emitted,
                        &mut windows,
                    );
                    continue;
                };
                let Some(plan) = project_retained_tiled_geometry(
                    session,
                    &key,
                    inset.bounds,
                    inset.gap,
                    focus_pair,
                    ProjectionKind::Reconcile,
                    &hints,
                    &shielded,
                ) else {
                    if let Some(scope) = scopes.last_mut() {
                        scope.authority = ScopeAuthority::Hold(HoldReason::ConstraintClamp);
                    }
                    // Projection failed on retained topology: geometric
                    // exhaustion, not an unreadable scope. The observed ids are
                    // not live rows, so the retained lifetimes carry explicit
                    // constraint-clamp hold rows instead of vanishing.
                    for window in &domain.windows {
                        live.remove(&window.window.0);
                    }
                    self.push_held_lifetimes(
                        born_paused,
                        session,
                        &key,
                        HoldReason::ConstraintClamp,
                        &live,
                        &mut emitted,
                        &mut windows,
                    );
                    continue;
                };
                Some(plan)
            } else {
                None
            };
            for geometry in plan.iter().flat_map(|plan| plan.geometry.iter()) {
                // Outstanding movers keep one homing at the transfer target.
                // Cross-output tiled movers (window transfers and migration
                // members) publish their actionable target slot here
                // (core-owned geometry, written only as part of the
                // membership move; sweep frame writes stay deferred until
                // membership proves, and arrival still needs the exact live
                // frame plus affinity): a FULL-delta mechanical remap cannot
                // place across asymmetric FULL/work geometries, so the slot
                // must ride the desire itself. Same-output movers need no
                // translation (the view batch proves on the shared screen)
                // and keep the established ordering. Every other scope emits
                // nothing for the mover. Flights the sweep cannot project
                // (floating-boundary sends commit no Engine plan,
                // unprojectable migration scopes) still ride the
                // membership-only redirect row below, and a stale source
                // session must never project a mover the truthful staged
                // homing already placed elsewhere.
                let window_flight = self.retained.iter().any(
                    |r| matches!(&r.target.kind, TransferKind::Window(w) if w == &geometry.window),
                );
                let migration_flight = self.retained.iter().any(|r| {
                    matches!(&r.target.kind, TransferKind::Migration)
                        && r.target.target_output == key.output
                        && r.target.target_workspace == key.workspace.0
                        && r.members.iter().any(|m| m == &geometry.window)
                });
                let redirect_target_is_here = redirect
                    .get(&geometry.window.0)
                    .is_some_and(|target| *target == key);
                let staged_home = self.staged_homing(&geometry.window);
                let cross_output_flight = (window_flight || migration_flight)
                    && redirect_target_is_here
                    && staged_home
                        .as_ref()
                        .is_some_and(|home| home.output != key.output);
                let arrived_here = redirect_target_is_here && staged_home.as_ref() == Some(&key);
                if redirect.contains_key(&geometry.window.0)
                    && !arrived_here
                    && !cross_output_flight
                {
                    continue;
                }
                let flight_elsewhere =
                    window_flight && staged_home.as_ref() != Some(&key) && !cross_output_flight;
                if flight_elsewhere {
                    continue;
                }
                let fullscreen = fullscreen_windows.contains(geometry.window.0.as_str());
                emitted.insert(geometry.window.0.clone());
                // Retained directional clear desire: same one-shot
                // `clear_operation` token as the exception route below.
                // Withheld on gesture barriers, membership holds, and
                // minimized/unreadable facts, and after the attempt is
                // spent (never re-actuated). Born bindings share the
                // lane: a born window already rides its admitted session
                // row, so the desire rides here until proof retires it.
                let fact_unreadable = self
                    .staged
                    .facts
                    .get(&geometry.window)
                    .is_none_or(|fact| fact.minimized == Some(true) || fact.rect.is_none());
                // `maximized: null` with unsupported caps is the
                // contract-valid absent value (macOS provisional
                // contract): ordinary admission, never an unknown hold.
                // Supported-but-null keeps the narrow unknown hold.
                let unknown_maximized = self.capabilities().is_some_and(|caps| caps.maximize)
                    && self
                        .staged
                        .facts
                        .get(&geometry.window)
                        .is_some_and(|fact| fact.maximized.is_none());
                // Core-composed maximize (`caps.maximize == false`): the
                // owned bit selects the truthful output work area as the
                // published frame while the tile allocation below stays
                // the retained slot. Unsupported native `maximized` stays
                // null whether or not the bit is set. R-MAX-11: the
                // overlay writes only after a readable native exit. Raw
                // fullscreen true or unknown, or a pending unattempted
                // fullscreen entry, withholds the overlay while the bit
                // stays retained; a withheld window publishes exactly like
                // an uncomposed row, except a pending entry additionally
                // holds the base frame write (no shared reply with the
                // queued entry). A missing work area fails closed.
                let composed = !self.capabilities().is_some_and(|caps| caps.maximize)
                    && self.composed_maximized.contains(&geometry.window);
                let work_area = self.composed_work_area(&geometry.output);
                let composed_active = composed
                    && self
                        .staged
                        .facts
                        .get(&geometry.window)
                        .is_some_and(|fact| fact.fullscreen == Some(false))
                    && !self.fullscreen_entry_pending(&geometry.window);
                let composed_gated = self.composed_frame_gated(&geometry.window);
                let desired = if composed_active {
                    work_area.unwrap_or(geometry.rect)
                } else {
                    geometry.rect
                };
                let work_missing = composed_active && work_area.is_none();
                let (clear_operation, command_seq) = if self.staged.gesture_barrier
                    || fact_unreadable
                    || unknown_maximized
                    || self.is_held_window(&geometry.window)
                    || self.is_excluded_window(&geometry.window)
                {
                    (None, None)
                } else {
                    match self.retained_clears.iter().find(|retained| {
                        retained.window == geometry.window && retained.pending.is_none()
                    }) {
                        Some(retained) => (
                            Some(retained.operation_id.clone()),
                            Some(retained.command_seq),
                        ),
                        // Born commandless clears ride the token lane
                        // with no command correlation.
                        None => (self.born_desire_for(&geometry.window, born_paused), None),
                    }
                };
                // Core-owned frame policy (stage 3a) over the actually
                // published frame: the composed work-area overlay, else the
                // tile allocation. An exact desired match clears retained
                // memory (completion is desired == observed,
                // never setter success); a suppressed pair publishes a
                // constraint-clamp hold with the desire intact so the adapter
                // skips the write and retries only on genuine change. The
                // burst bound adds a second veto for the same pair: at most
                // two consumed attempts per observed-change burst.
                let observed = staged
                    .facts
                    .get(&geometry.window)
                    .and_then(|fact| fact.rect);
                if observed == Some(desired) {
                    self.frames.remove(&geometry.window);
                }
                let frame_suppressed = observed.is_some_and(|current| {
                    self.frame_suppressed(
                        &geometry.window,
                        &desired,
                        Some(current),
                        self.staged.observation.monotonic_ms,
                    )
                });
                let burst_held = Self::frame_burst_gate(
                    &mut self.frame_bursts,
                    &geometry.window,
                    &desired,
                    observed,
                    self.staged.observation.monotonic_ms,
                );
                windows.push(WindowDesire {
                    lifetime: geometry.window.clone(),
                    output: geometry.output.clone(),
                    workspace: geometry.workspace.0.clone(),
                    allocation: Some(geometry.rect),
                    effective_frame: if fullscreen { None } else { Some(desired) },
                    frame_authority: if fullscreen {
                        ScopeAuthority::Untouched
                    } else if composed_gated {
                        ScopeAuthority::Hold(HoldReason::ConstraintClamp)
                    } else if work_missing {
                        ScopeAuthority::Hold(HoldReason::Unreadable)
                    } else if frame_suppressed || burst_held {
                        ScopeAuthority::Hold(HoldReason::ConstraintClamp)
                    } else if frame_hold {
                        ScopeAuthority::Hold(HoldReason::GestureBarrier)
                    } else if !unknown_maximized {
                        ScopeAuthority::Controlled
                    } else {
                        ScopeAuthority::Hold(HoldReason::Unreadable)
                    },
                    membership_authority: if self
                        .style_flight_held(style_held_output, &geometry.window)
                    {
                        ScopeAuthority::Hold(HoldReason::Unreadable)
                    } else {
                        ScopeAuthority::Controlled
                    },
                    // Tiled rows never carry a float-placement token.
                    frame_operation: None,
                    clear_operation,
                    command_seq,
                    visibility: None,
                    visibility_authority: ScopeAuthority::Untouched,
                    state: Vec::new(),
                    floating: false,
                    float_origin: None,
                });
            }
            // Exception rows (floats, fullscreen, maximized, sticky, game
            // overlays): no automatic frame, focus, stacking, or membership
            // authority. Ordinary floats keep their frames (untouched, never
            // rewritten) while retaining the desired effective frame from
            // the session, so a lost one-shot placement reply never loses
            // the property: unrelated later replies still carry it with no
            // write authority. Game safety overrides parity. A retained D16
            // float send authorizes membership at the target without
            // authorizing any frame change.
            for observed in session.exception_observed() {
                if observed.output != key.output || observed.workspace != key.workspace {
                    continue;
                }
                let in_flight = self.retained.iter().any(
                    |r| matches!(&r.target.kind, TransferKind::Window(w) if w == &observed.window),
                );
                emitted.insert(observed.window.0.clone());
                // Explicit retained float target, if outstanding: the row
                // carries the stable property operation id so the adapter
                // places the exact retained frame exactly once. Ordinary
                // floats carry none and never take frame writes. A
                // core-suppressed pair withholds the token while the
                // retained target stays outstanding (same clamp/transient
                // owner as tiled rows); an exact match clears memory. The
                // published frame is the retained desire (directional
                // half-snaps, explicit placements), falling back to the
                // session geometry only with no outstanding target.
                let retained_float = self
                    .retained_floats
                    .iter()
                    .find(|held| held.window == observed.window);
                let retained_target = retained_float.map(|held| held.target);
                let mut frame_operation = retained_float.map(|held| held.operation_id.clone());
                let mut frame_seq = retained_float.map(|held| held.command_seq);
                if frame_operation.is_some() {
                    let target =
                        retained_target.or_else(|| session.floating_geometry(&observed.window));
                    let fact_rect = staged
                        .facts
                        .get(&observed.window)
                        .and_then(|fact| fact.rect);
                    match (target, fact_rect) {
                        (Some(desired), Some(current)) if desired == current => {
                            self.frames.remove(&observed.window);
                        }
                        (Some(desired), current)
                            if self.frame_suppressed(
                                &observed.window,
                                &desired,
                                current,
                                self.staged.observation.monotonic_ms,
                            ) =>
                        {
                            frame_operation = None;
                            frame_seq = None;
                        }
                        _ => {}
                    }
                }
                // No placement write without readable hints; the binding
                // survives for recovery.
                if self.is_held_window(&observed.window)
                    || self.is_excluded_window(&observed.window)
                    || staged
                        .facts
                        .get(&observed.window)
                        .is_none_or(|fact| fact.hints.is_none())
                {
                    frame_operation = None;
                    frame_seq = None;
                }
                // Explicit ordinary-float classification (D4/D5): every
                // retained exception row in a controlled scope converges
                // adapter float intent. The diagnostic capacity origin
                // rides along for truthful echo; it grants no behavior.
                let float_origin = session
                    .is_capacity_float(&observed.window)
                    .then_some(FloatOrigin::Capacity);
                // One-shot clear token: never re-actuated once spent; the
                // settlement owns every verdict. A float-route clear is
                // also withheld while hints are unreadable (its
                // continuation could not authorize); the binding survives.
                let fact_unreadable = self
                    .staged
                    .facts
                    .get(&observed.window)
                    .is_none_or(|fact| fact.minimized == Some(true) || fact.rect.is_none());
                let directional = self.retained_clears.iter().find(|retained| {
                    retained.window == observed.window && retained.pending.is_none()
                });
                let float_blocked = directional.is_some_and(|retained| retained.float_route)
                    && staged
                        .facts
                        .get(&observed.window)
                        .is_none_or(|fact| fact.hints.is_none());
                let (clear_operation, clear_seq) = if self.staged.gesture_barrier
                    || fact_unreadable
                    || float_blocked
                    || self.is_held_window(&observed.window)
                    || self.is_excluded_window(&observed.window)
                {
                    (None, None)
                } else {
                    match directional {
                        Some(retained) => (
                            Some(retained.operation_id.clone()),
                            Some(retained.command_seq),
                        ),
                        // Born commandless clears ride the token lane
                        // with no command correlation.
                        None => (self.born_desire_for(&observed.window, born_paused), None),
                    }
                };
                // Single command slot for the row. The clear->float continuation
                // resumes under the originating operation id, so the normal
                // case carries one sequence for both tokens. A later
                // NeedClear supersedes any retained same-window float at
                // mint, so a stale placement never rides alongside the new
                // clear; the slot carries the surviving token.
                // Core-composed maximize on a float base: the owned bit
                // selects the truthful output work area under a `Controlled`
                // frame lane (plain floats stay `Untouched`). The session
                // float base is never overwritten by the observed composed
                // frame: convergence never fabricates float geometry from
                // observations, so clear restores the base exactly. No
                // placement token is minted; an unavailable composed write
                // fails closed to the untouched base frame. R-MAX-11: raw
                // fullscreen true or unknown withholds the overlay and any
                // placement token while the bit stays retained; a pending
                // unattempted fullscreen entry withholds the same writes
                // (the shared composed gate below); a withheld window
                // publishes exactly like an uncomposed float.
                let composed = !self.capabilities().is_some_and(|caps| caps.maximize)
                    && self.composed_maximized.contains(&observed.window);
                let work_area = self.composed_work_area(&observed.output);
                let float_base =
                    retained_target.or_else(|| session.floating_geometry(&observed.window));
                let composed_home = DomainKey {
                    output: observed.output.clone(),
                    workspace: observed.workspace.clone(),
                };
                let fullscreen_released = staged
                    .facts
                    .get(&observed.window)
                    .is_some_and(|fact| fact.fullscreen == Some(false));
                let composed_active = composed
                    && fullscreen_released
                    && !self.fullscreen_entry_pending(&observed.window);
                let composed_controlled = composed_active
                    && work_area.is_some()
                    && self.staged.observation.inventory_complete
                    && !self.staged.gesture_barrier
                    && !self.floating_scopes.contains(&composed_home)
                    && !self.hold_reasons.contains_key(&composed_home)
                    && !self.is_held_window(&observed.window)
                    && !self.is_excluded_window(&observed.window)
                    && !in_flight
                    && !self.derived_for(&observed.window).hidden
                    && staged
                        .facts
                        .get(&observed.window)
                        .is_some_and(|fact| fact.hints.is_some() && fact.minimized != Some(true));
                // Shared clamp policy against the actually published frame
                // (like the tiled lane above): a suppressed pair publishes
                // a constraint-clamp hold with the desire intact so the
                // adapter skips the write and retries only on genuine
                // change, never an auto-authorized retry outside policy.
                let overlay_frame = work_area.or(float_base);
                let overlay_observed = staged
                    .facts
                    .get(&observed.window)
                    .and_then(|fact| fact.rect);
                let mut overlay_suppressed = false;
                let mut overlay_burst = false;
                if composed_active && let Some(desired) = overlay_frame {
                    overlay_suppressed = overlay_observed.is_some_and(|current| {
                        self.frame_suppressed(
                            &observed.window,
                            &desired,
                            Some(current),
                            self.staged.observation.monotonic_ms,
                        )
                    });
                    overlay_burst = Self::frame_burst_gate(
                        &mut self.frame_bursts,
                        &observed.window,
                        &desired,
                        overlay_observed,
                        self.staged.observation.monotonic_ms,
                    );
                }
                // No placement write beside the owned overlay while native
                // fullscreen is unreleased, nor while a native entry pends
                // (shared composed gate): the lane carries one frame
                // authority only.
                if composed && !fullscreen_released || self.composed_frame_gated(&observed.window) {
                    frame_operation = None;
                    frame_seq = None;
                }
                let command_seq = frame_seq.or(clear_seq);
                windows.push(WindowDesire {
                    lifetime: observed.window.clone(),
                    output: observed.output.clone(),
                    workspace: observed.workspace.0.clone(),
                    allocation: None,
                    effective_frame: if composed_active {
                        overlay_frame
                    } else {
                        float_base
                    },
                    frame_authority: if composed_controlled && !overlay_suppressed && !overlay_burst
                    {
                        ScopeAuthority::Controlled
                    } else if composed_active && (overlay_suppressed || overlay_burst) {
                        ScopeAuthority::Hold(HoldReason::ConstraintClamp)
                    } else {
                        ScopeAuthority::Untouched
                    },
                    membership_authority: if self
                        .style_flight_held(style_held_output, &observed.window)
                    {
                        ScopeAuthority::Hold(HoldReason::Unreadable)
                    } else if in_flight {
                        ScopeAuthority::Controlled
                    } else {
                        ScopeAuthority::Untouched
                    },
                    frame_operation,
                    clear_operation,
                    command_seq,
                    visibility: None,
                    visibility_authority: ScopeAuthority::Untouched,
                    state: Vec::new(),
                    floating: true,
                    float_origin,
                });
            }
            // Minimized lifetimes (D23 slots): explicit rows with no managed
            // geometry, never silent absence and never an inferred close.
            // The slot stays homed until restore, explicit close, or the
            // retained transfer settles it.
            for (window, output, workspace) in session.minimized_entries() {
                if output != key.output || workspace != key.workspace {
                    continue;
                }
                if live.contains(&window.0) || !emitted.insert(window.0.clone()) {
                    continue;
                }
                // A minimized lifetime under an outstanding membership
                // transfer rides the target homing (one homing), keeping
                // minimized frame authority: the adapter actuates only the
                // desktop set, never geometry or unminimize.
                let (home_output, home_workspace) = redirect
                    .get(&window.0)
                    .map(|target| (target.output.clone(), target.workspace.0.clone()))
                    .unwrap_or((output.clone(), workspace.0.clone()));
                let session_flight_held = self.style_flight_held(style_held_output, &window);
                windows.push(WindowDesire {
                    lifetime: window,
                    output: home_output,
                    workspace: home_workspace,
                    allocation: None,
                    effective_frame: None,
                    frame_authority: ScopeAuthority::Hold(HoldReason::Minimized),
                    membership_authority: if session_flight_held {
                        ScopeAuthority::Hold(HoldReason::Unreadable)
                    } else {
                        ScopeAuthority::Controlled
                    },
                    frame_operation: None,
                    clear_operation: None,
                    floating: false,
                    float_origin: None,
                    command_seq: None,
                    visibility: None,
                    visibility_authority: ScopeAuthority::Untouched,
                    state: Vec::new(),
                });
            }
        }
        // Redirected movers not yet homed at the target: one
        // membership row at the transfer target homing (never allocation).
        // Cross-output migration members with a readable ordinary frame
        // additionally carry a core-owned portable placement in
        // `effective_frame` (FULL-delta translated, constrained to the
        // destination work area): the adapter actuates it as part of the
        // membership move, never a native origin-delta guess. Same-output
        // migration members, floats, sticky, overlays, minimized, and
        // unreadable/incomplete members carry no placement and ride
        // membership only. Arrived movers already carry sweep rows, so the
        // emitted guard keeps exactly one homing.
        // Hidden-lane lifetimes (minimized or host-hidden) never ride this
        // lane: staged hidden facts emit with minimized frame authority
        // below. Raw facts stay distinct; the derived lane decides.
        let minimized_ids: BTreeSet<&str> = staged
            .facts
            .values()
            .filter(|fact| self.derived_for(&fact.window).hidden)
            .map(|fact| fact.window.0.as_str())
            .collect();
        let mut migration_redirect: BTreeSet<String> = BTreeSet::new();
        for retained in &self.retained {
            if !matches!(&retained.target.kind, TransferKind::Migration) {
                continue;
            }
            let key = DomainKey {
                output: retained.target.target_output.clone(),
                workspace: WorkspaceId(retained.target.target_workspace.clone()),
            };
            for member in &retained.members {
                if redirect.get(&member.0).as_ref() == Some(&&key)
                    && self.staged_homing(member).as_ref() != Some(&key)
                {
                    migration_redirect.insert(member.0.clone());
                }
            }
        }
        for (window_id, key) in &redirect {
            if minimized_ids.contains(window_id.as_str()) {
                continue;
            }
            if !emitted.insert(window_id.clone()) {
                continue;
            }
            let window = WindowId(window_id.clone());
            let flight_held = self.style_flight_held(style_held_output, &window);
            let placement = if flight_held {
                None
            } else if migration_redirect.contains(window_id) {
                migration_cross_output_placement(&staged, &window, key)
            } else {
                None
            };
            windows.push(WindowDesire {
                lifetime: window,
                output: key.output.clone(),
                workspace: key.workspace.0.clone(),
                allocation: None,
                effective_frame: placement,
                frame_authority: ScopeAuthority::Untouched,
                membership_authority: if flight_held {
                    ScopeAuthority::Hold(HoldReason::Unreadable)
                } else {
                    ScopeAuthority::Controlled
                },
                frame_operation: None,
                clear_operation: None,
                floating: false,
                float_origin: None,
                command_seq: None,
                visibility: None,
                visibility_authority: ScopeAuthority::Untouched,
                state: Vec::new(),
            });
        }
        // Explicit hidden-lane facts without a retained slot row: the
        // adapter reported them minimized or host-hidden but no session
        // slot emitted a row above (fresh minimize/hide, held scope, or
        // gap). Explicit hidden-hold rows, never silent absence, never
        // inferred close. Raw facts stay distinct; the derived lane
        // decides.
        for fact in staged
            .facts
            .values()
            .filter(|fact| self.derived_for(&fact.window).hidden)
        {
            // Exclusions never publish minimized rows: no slot, no control.
            if self.is_excluded_window(&fact.window) {
                continue;
            }
            if live.contains(&fact.window.0) || !emitted.insert(fact.window.0.clone()) {
                continue;
            }
            // A minimized lifetime under an outstanding membership
            // transfer rides the target homing (one homing), keeping
            // minimized frame authority. The scope-hold lookup follows
            // the resolved homing.
            let Some((home_output, home_workspace)) = redirect
                .get(&fact.window.0)
                .map(|target| (target.output.clone(), target.workspace.clone()))
                .or_else(|| {
                    self.staged_homing(&fact.window)
                        .map(|key| (key.output, key.workspace))
                })
                .or_else(|| {
                    fact.output
                        .clone()
                        .map(|output| (output, WorkspaceId(String::new())))
                })
            else {
                continue;
            };
            // No native membership evidence authorizes an empty logical target.
            if home_workspace.0.is_empty()
                && fact
                    .output
                    .as_ref()
                    .is_some_and(|_| self.native_authoritative())
                && self.effective_logical_receipt(fact).is_none()
            {
                continue;
            }
            // A minimized lifetime homed on a held scope carries that
            // scope's hold on membership. Omission inside a held scope
            // authorizes no write or retirement. The narrow style hold
            // covers held targets absent a staged domain.
            let scope_hold = staged
                .domains
                .iter()
                .find(|domain| {
                    domain.key.output == home_output && domain.key.workspace == home_workspace
                })
                .and_then(|domain| {
                    if !domain.readable {
                        Some(domain.hold.unwrap_or(HoldReason::Unreadable))
                    } else {
                        self.hold_reasons.get(&domain.key).copied()
                    }
                })
                .or_else(|| {
                    style_held_output.and_then(|held| {
                        if *held == home_output {
                            Some(HoldReason::Unreadable)
                        } else {
                            None
                        }
                    })
                });
            windows.push(WindowDesire {
                lifetime: fact.window.clone(),
                output: home_output,
                workspace: home_workspace.0.clone(),
                allocation: None,
                effective_frame: None,
                frame_authority: ScopeAuthority::Hold(HoldReason::Minimized),
                membership_authority: match scope_hold {
                    Some(reason) => ScopeAuthority::Hold(reason),
                    None if self.style_flight_held(style_held_output, &fact.window) => {
                        ScopeAuthority::Hold(HoldReason::Unreadable)
                    }
                    None => ScopeAuthority::Controlled,
                },
                frame_operation: None,
                clear_operation: None,
                floating: false,
                float_origin: None,
                command_seq: None,
                visibility: None,
                visibility_authority: ScopeAuthority::Untouched,
                state: Vec::new(),
            });
        }
        // Commandless persistent above: one mutating
        // reconcile plus central decoration over the assembled rows, after
        // membership/commands are complete. Runs before the sticky row so
        // each lane keeps its exact target authority.
        self.reconcile_and_decorate_native_above_state(held_scopes, &mut windows);
        // Composed sticky visibility: central decoration over the
        // assembled rows (sticky-only, owner-output rows). Independent of
        // the frame/member lanes above; touches visibility fields only.
        self.decorate_sticky_visibility(held_scopes, &mut windows);
        // Native-free selection visibility: view changes outside the
        // selection command path supersede stale scopes first (no new
        // authority), then the pending grant (if any) lands in the
        // retained per-window map from post-command homing, the map
        // decorates the assembled rows (sticky rows keep precedence), and
        // the final arrival snapshot plus per-output scopes freeze from
        // the published frames.
        self.reconcile_selection_scopes(held_scopes);
        self.apply_pending_selection();
        self.admit_selection_late_members();
        self.decorate_selection_visibility(held_scopes, &mut windows);
        self.freeze_selection_arrival(effects, &windows);
        // Native sticky state: central decoration over the assembled rows
        // beside the composed call. Touches state lanes only.
        self.decorate_native_sticky_state(held_scopes, &mut windows);
        // Native fullscreen state: central decoration over the assembled
        // rows beside the sticky row. Touches state lanes only; coexists
        // with the sticky row on one window.
        self.decorate_native_fullscreen_state(held_scopes, &mut windows);
        // Native maximize state: central decoration after the fullscreen
        // row so the paired-clear gate inspects the exact clear row
        // authority. Touches state lanes only; coexists with the
        // sticky/fullscreen rows on one window.
        self.decorate_native_maximize_state(held_scopes, &mut windows);
        let mut by_output: BTreeMap<OutputId, Vec<ScopeStatus>> = BTreeMap::new();
        for status in scopes {
            let output = match &status.scope {
                crate::world_report::AuthorityScope::Output(output)
                | crate::world_report::AuthorityScope::Workspace { output, .. } => output.clone(),
            };
            by_output.entry(output).or_default().push(status);
        }
        let mut scopes = Vec::new();
        for (output, statuses) in by_output {
            let uniform_authority = statuses
                .first()
                .map(|first| first.authority)
                .filter(|auth| statuses.iter().all(|status| status.authority == *auth));
            if let Some(authority) = uniform_authority {
                scopes.push(ScopeStatus {
                    scope: crate::world_report::AuthorityScope::Output(output),
                    authority,
                });
            } else {
                scopes.extend(statuses);
            }
        }
        scopes.sort_by(|a, b| a.scope.cmp(&b.scope));
        PublicationRows { scopes, windows }
    }

    pub(crate) fn publish(
        &mut self,
        report: &WorldReport,
        command_consumed: bool,
        effects: Vec<OneShotEffect>,
        command_result: Option<CommandResult>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> WorldReply {
        let rows =
            self.assemble_publication_rows(report.presentation.paused, &effects, held_scopes, None);
        let scopes = rows.scopes;
        let windows = rows.windows;
        // Omission of a minimized/absent window inside a controlled scope
        // means "no managed geometry", never a close or teardown instruction.
        let workspaces = self.plan_workspaces(&scopes);
        let outputs = self.plan_outputs(report, held_scopes);
        // Per-report warp desire (stage 4b, cross-output only): computed
        // over the final focus effects plus the published window rows, so
        // explicit, admission, and hover-coupling focus share one rule.
        let cursor_warp = self.warp_for_publish(&effects, &windows, held_scopes);
        // Per-report gesture terminal: taken here so the next unrelated
        // report publishes `None`. Duplicates repeat the cached reply.
        let gesture_settled = self.gesture_terminal.take();
        // Per-report operation lifecycle terminals: taken here so the next
        // unrelated report publishes empty. Duplicates repeat the cached
        // reply.
        let operation_settled = std::mem::take(&mut self.operation_settled);
        // Capability-named note: explicit creation desires only; the reply
        // carries one stable op per outstanding authorization.
        let native_creations = self.native_creation_desires();
        // Real-engine canonical intent saves (at most one): a staged
        // sticky-command save drains exactly once on its successful fresh
        // command, else the ordinary ToggleFloat save; the exact-encoding
        // gate below still commits it atomically with the rest of the reply.
        let intent_writes =
            self.drain_intent_saves(report.command.as_ref(), command_consumed, &command_result);
        // Per-report switch terminals: taken here so the next unrelated
        // report publishes empty. Duplicates repeat the cached reply.
        let switch_settled = std::mem::take(&mut self.switch_settled);
        WorldReply {
            contract_version: WORLD_CONTRACT_VERSION_LEGACY,
            adapter_incarnation: report.adapter_incarnation.clone(),
            core_generation: self.core_generation_str.clone(),
            request_id: report.request_id.clone(),
            based_on_observation_seq: report.observation_seq,
            state_revision: self.state_revision,
            ready: self.is_ready(),
            command_consumed,
            scopes,
            workspaces,
            outputs,
            diagnostics: self.staged.diagnostics.clone(),
            windows,
            transfers: self.retained_transfers(),
            effects,
            command_result,
            gesture_settled,
            operation_settled,
            native_creations,
            cursor_warp,
            presentation: crate::world_presentation::WorldPresentation::default(),
            intent_writes,
            switch_settled,
        }
    }

    /// Retained lifetime rows for a held scope: snapshot windows, exceptions,
    /// and minimized slots homed on `key`, each with narrow holds. Skips
    /// live lifetimes (they carry their own rows) and already-emitted ids,
    /// so every window keeps exactly one homing.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn push_held_lifetimes(
        &self,
        born_paused: bool,
        session: &crate::session::Session,
        key: &DomainKey,
        reason: HoldReason,
        live: &BTreeSet<String>,
        emitted: &mut BTreeSet<String>,
        windows: &mut Vec<WindowDesire>,
    ) {
        for link in session.snapshot().windows {
            if link.output != key.output || link.workspace != key.workspace {
                continue;
            }
            // Exclusions never ride held rows: no slot, no control, even
            // when the surrounding scope holds for another lifetime.
            if self.is_excluded_window(&link.window) {
                continue;
            }
            if live.contains(&link.window.0) || !emitted.insert(link.window.0.clone()) {
                continue;
            }
            // Like the fresh-seed held rows above: no frame/member write,
            // but an outstanding born clear still rides its token.
            let clear_operation = self.born_desire_for(&link.window, born_paused);
            windows.push(WindowDesire {
                lifetime: link.window.clone(),
                output: link.output.clone(),
                workspace: link.workspace.0.clone(),
                allocation: None,
                effective_frame: None,
                frame_authority: ScopeAuthority::Hold(reason),
                membership_authority: ScopeAuthority::Hold(reason),
                frame_operation: None,
                clear_operation,
                floating: false,
                float_origin: None,
                command_seq: None,
                visibility: None,
                visibility_authority: ScopeAuthority::Untouched,
                state: Vec::new(),
            });
        }
        for observed in session.exception_observed() {
            if observed.output != key.output || observed.workspace != key.workspace {
                continue;
            }
            if self.is_excluded_window(&observed.window) {
                continue;
            }
            if live.contains(&observed.window.0) || !emitted.insert(observed.window.0.clone()) {
                continue;
            }
            let clear_operation = self.born_desire_for(&observed.window, born_paused);
            windows.push(WindowDesire {
                lifetime: observed.window.clone(),
                output: observed.output.clone(),
                workspace: observed.workspace.0.clone(),
                allocation: None,
                effective_frame: None,
                frame_authority: ScopeAuthority::Hold(reason),
                membership_authority: ScopeAuthority::Hold(reason),
                frame_operation: None,
                clear_operation,
                floating: false,
                float_origin: None,
                command_seq: None,
                visibility: None,
                visibility_authority: ScopeAuthority::Untouched,
                state: Vec::new(),
            });
        }
        for (window, output, workspace) in session.minimized_entries() {
            if output != key.output || workspace != key.workspace {
                continue;
            }
            if self.is_excluded_window(&window) {
                continue;
            }
            if live.contains(&window.0) || !emitted.insert(window.0.clone()) {
                continue;
            }
            windows.push(WindowDesire {
                lifetime: window,
                output: output.clone(),
                workspace: workspace.0.clone(),
                allocation: None,
                effective_frame: None,
                frame_authority: ScopeAuthority::Hold(HoldReason::Minimized),
                membership_authority: ScopeAuthority::Hold(reason),
                frame_operation: None,
                clear_operation: None,
                floating: false,
                float_origin: None,
                command_seq: None,
                visibility: None,
                visibility_authority: ScopeAuthority::Untouched,
                state: Vec::new(),
            });
        }
    }

    /// Retained tiling mode for one logical workspace: absent means tiled.
    /// Seeding keeps every ringed id present, so absence here is only a
    /// backstop. Set through the planner-owned tiling command or the
    /// saved-default seed; the adapter never decides.
    pub(crate) fn workspace_tiling_mode(&self, output: &OutputId, workspace: &str) -> TilingMode {
        self.tiling
            .get(&DomainKey {
                output: output.clone(),
                workspace: WorkspaceId(workspace.to_owned()),
            })
            .copied()
            .unwrap_or(TilingMode::Tiled)
    }

    /// Planner-owned logical workspace rows: every retained ring id gets
    /// one row in ring order with scope, order, and tiling mode. Rings
    /// already converged (bootstrap, lifecycle, displacement, commands),
    /// so rows publish the retained set directly. A row reads
    /// `existence:false` only when the global native-retirement set names
    /// it (completely empty everywhere, unprotected, above the native
    /// minimum): eligibility only, the adapter re-enumerates before
    /// deleting, and the ring plus native binding stay until a complete
    /// readback drops the desktop. Incomplete screens hold their rows
    /// without authorizing writes.
    pub(crate) fn plan_workspaces(&mut self, scopes: &[ScopeStatus]) -> Vec<WorkspaceDesire> {
        let mut outputs: Vec<String> = self.mapping.keys().cloned().collect();
        outputs.sort();
        let staged = self.staged.clone();
        let doomed = self.retirement_deletion_set();
        let mut rows = Vec::new();
        for output in outputs {
            let Some(ring) = self.mapping.get(&output).cloned() else {
                continue;
            };
            let marked = self.trailing_marked_id(&output);
            let held = !staged
                .screens
                .iter()
                .any(|screen| screen.output.0 == output && screen.complete);
            for (index, id) in ring.iter().enumerate() {
                let native_binding = if self.host_owned_workspace_domains() {
                    self.native_binding_for(&output, id)
                } else {
                    None
                };
                // An incompatible fresh shared projection holds its
                // workspace rows through the existing Unreadable lane;
                // ordinary incomplete screens keep IncompleteInventory.
                // Capability-named note: ambiguous creations hold as
                // AmbiguousCreation (producer) and never blind-recreate.
                let hold = if held {
                    Some(HoldReason::IncompleteInventory)
                } else if self.native_order_conflict() {
                    Some(HoldReason::Unreadable)
                } else if self.creation_hold_for(&output, id) {
                    Some(HoldReason::AmbiguousCreation)
                } else {
                    None
                };
                rows.push(WorkspaceDesire {
                    id: id.clone(),
                    output: OutputId(output.clone()),
                    native_binding,
                    scope: if scopes.iter().any(|scope| {
                        scope.scope
                            == crate::world_report::AuthorityScope::Output(OutputId(output.clone()))
                    }) {
                        crate::world_report::AuthorityScope::Output(OutputId(output.clone()))
                    } else {
                        workspace_authority_scope(&OutputId(output.clone()), id)
                    },
                    order: index,
                    tiling_mode: self.workspace_tiling_mode(&OutputId(output.clone()), id),
                    existence: !doomed.contains(&(output.clone(), id.clone())),
                    hold,
                    trailing: marked.as_deref() == Some(id.as_str()),
                });
            }
        }
        rows
    }

    /// Desired output views: retained view targets win, else the core
    /// view (informational, never a focus command). Incomplete screens or
    /// held scopes on the output hold the row: the adapter must not
    /// actuate views it cannot verify.
    pub(crate) fn plan_outputs(
        &self,
        report: &WorldReport,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Vec<OutputDesire> {
        self.plan_outputs_inner(&report.views, None, held_scopes)
    }

    /// Output-view core over explicit inputs: `view_echoes` are the
    /// report's unknown-view proofs (legacy path only; the v4 path passes
    /// none and relies on staged state), and `style_held_output` is the
    /// narrow v4 presentation-style hold (v4 path only; legacy passes
    /// `None`). The unknown-echo guard reads `view_echoes` only, never
    /// staged state.
    pub(crate) fn plan_outputs_inner(
        &self,
        view_echoes: &[ScreenView],
        style_held_output: Option<&OutputId>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Vec<OutputDesire> {
        self.staged
            .screens
            .iter()
            .map(|screen| {
                let pending_view = self.retained.iter().find_map(|r| match &r.target.kind {
                    TransferKind::View if r.target.target_output == screen.output => {
                        Some(r.target.target_workspace.clone())
                    }
                    _ => None,
                });
                let logical_view: Option<String> = pending_view.or_else(|| {
                    self.views
                        .get(&screen.output.0)
                        .filter(|view| {
                            self.mapping
                                .get(&screen.output.0)
                                .is_some_and(|ring| ring.contains(view))
                        })
                        .cloned()
                });
                let tiling_mode = logical_view.as_deref().map_or(TilingMode::Tiled, |view| {
                    self.workspace_tiling_mode(&screen.output, view)
                });
                // An echo naming an unknown workspace never authorizes:
                // the adapter shows something the core does not own, so
                // the row holds instead of adopting it. The host-owned
                // capability tolerates such echoes (observed only). Legacy
                // path only: v4 passes no echoes.
                let ring = self.mapping.get(&screen.output.0);
                let echo_unknown = !view_echoes.is_empty()
                    && view_echoes
                        .iter()
                        .find(|view| view.output == screen.output)
                        .is_some_and(|view| {
                            !view.view.is_empty()
                                && !ring.is_some_and(|ring| ring.iter().any(|id| id == &view.view))
                                && !self.host_owned_workspace_domains()
                        });
                let style_only = style_held_output.is_some_and(|output| *output == screen.output);
                let legacy_hold = !screen.complete
                    || echo_unknown
                    || held_scopes.iter().any(|key| key.output == screen.output);
                let hold = if legacy_hold {
                    Some(HoldReason::IncompleteInventory)
                } else if style_only {
                    Some(HoldReason::Unreadable)
                } else {
                    None
                };
                OutputDesire {
                    output: screen.output.clone(),
                    logical_view,
                    tiling_mode,
                    hold,
                }
            })
            .collect()
    }
}

/// Constrain one preserved-size rect inside the destination work area:
/// the origin clamps into the area when the rect fits, else pins to the
/// area origin (size preserved, never resized). Pure [`Rect`] math, no
/// platform-channel checks.
fn constrain_rect_to_area(rect: Rect, area: Rect) -> Rect {
    let x = if rect.w <= area.w {
        rect.x.clamp(area.x, area.x + area.w - rect.w)
    } else {
        area.x
    };
    let y = if rect.h <= area.h {
        rect.y.clamp(area.y, area.y + area.h - rect.h)
    } else {
        area.y
    };
    Rect {
        x,
        y,
        w: rect.w,
        h: rect.h,
    }
}

/// Portable cross-output placement for one sessionless migration member:
/// the observed source frame FULL-delta translated onto the target output,
/// then constrained to the destination work area. The translation grounds
/// on the RAW observed screen (`staged.fact_output`), never the logical
/// homing: midflight facts may read on another output than the homing
/// (host-owned receipts hold a member at the source while its frame
/// already shows at the target). An already-target frame takes no
/// placement (never a double translation); the membership row alone
/// suffices and the sweep re-tiles. `None` for same-output members
/// (shared-screen view batch proves, ordering unchanged), floats, sticky,
/// overlays, minimized, unreadable frames, unknown homing, and unreadable
/// or incomplete screens (hold, never an invented placement). Ordinary
/// float sends and tiled-to-floating crossings never reach here (migration
/// members only, chosen by the caller).
fn migration_cross_output_placement(
    staged: &crate::world_engine::StagedReport,
    window: &WindowId,
    target: &DomainKey,
) -> Option<Rect> {
    let home = staged.homing.get(window)?;
    if home.output == target.output {
        return None;
    }
    let fact = staged.facts.get(window)?;
    fact.maximized?;
    let derived = staged.derived.get(window)?;
    if derived.hidden
        || derived.hold
        || derived.excluded
        || derived.sticky
        || derived.floating
        || derived.overlay_fullscreen
        || derived.overlay_maximized
        || derived.fit_excluded
    {
        return None;
    }
    let source_rect = fact.rect?;
    if source_rect.w <= 0 || source_rect.h <= 0 {
        return None;
    }
    // Raw observation grounds the source screen: the index is total over
    // staged facts, so a missing entry means unknown geometry and takes
    // no placement, never a guess.
    let observed_output = staged.fact_output.get(window)?.as_ref()?;
    if observed_output == &target.output {
        return None;
    }
    let source_screen = staged
        .screens
        .iter()
        .find(|s| &s.output == observed_output)?;
    let target_screen = staged.screens.iter().find(|s| s.output == target.output)?;
    if !source_screen.complete || !target_screen.complete {
        return None;
    }
    let source_full = source_screen.full_rect?;
    let target_full = target_screen.full_rect?;
    let target_area = target_screen.work_area?;
    if target_area.w <= 0 || target_area.h <= 0 {
        return None;
    }
    let translated = Rect {
        x: source_rect.x - source_full.x + target_full.x,
        y: source_rect.y - source_full.y + target_full.y,
        w: source_rect.w,
        h: source_rect.h,
    };
    let placed = constrain_rect_to_area(translated, target_area);
    crate::bounds::valid_carried_rect(placed.x, placed.y, placed.w, placed.h).then_some(placed)
}
