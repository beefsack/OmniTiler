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
    CommandResult, FloatOrigin, HoldReason, OneShotEffect, OutputDesire, ScopeAuthority,
    ScopeStatus, TilingMode, TransferKind, WORLD_CONTRACT_VERSION, WindowDesire, WorkspaceDesire,
    WorldReply, WorldReport, workspace_scope_key,
};

impl WorldEngine {
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
    /// Floating scopes carry explicit untouched lifetime rows (never
    /// emptiness): observed non-movers publish with no authority, and
    /// movers under an outstanding boundary transfer publish one
    /// membership-only row at the target homing. No floating row ever
    /// carries allocation or frame authority (no frame writes on floats).
    /// A held scope with no retained session carries its staged lifetimes
    /// as complete held rows, so no inventory ever vanishes.
    pub(crate) fn publish(
        &mut self,
        report: &WorldReport,
        command_consumed: bool,
        effects: Vec<OneShotEffect>,
        command_result: Option<CommandResult>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> WorldReply {
        let staged = self.staged.clone();
        let mut scopes = Vec::with_capacity(staged.domains.len());
        let mut windows = Vec::new();
        // Homed lifetimes in readable, unheld, non-floating scopes carry
        // live rows. Minimized and held-scope rows never shadow a live row
        // for the same lifetime: one homing per window.
        let mut live: BTreeSet<String> = BTreeSet::new();
        for domain in &staged.domains {
            if !domain.readable
                || held_scopes.contains(&domain.key)
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
            if held_scopes.contains(&key) {
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
            if held_scopes.contains(&key) {
                continue;
            }
            for member in &retained.members {
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
            .filter(|fact| fact.fullscreen)
            .map(|fact| fact.window.0.as_str())
            .collect();
        for domain in &staged.domains {
            let key = domain.key.clone();
            // Floating workspaces are adapter-owned: the scope is
            // explicitly untouched. Observed lifetimes still publish
            // explicit untouched rows (never silent absence, never a
            // frame write): non-movers with no authority at their
            // observed homing; movers under an outstanding boundary
            // transfer publish one membership-only row at the target
            // homing below, so they emit nothing here (one homing).
            if self.floating_scopes.contains(&key) {
                scopes.push(ScopeStatus {
                    scope: format!("{}/{}", key.output.0, key.workspace.0),
                    authority: ScopeAuthority::Untouched,
                });
                for window in &domain.windows {
                    // Movers redirected to the target homing emit
                    // nothing here (one homing); all other observed
                    // lifetimes carry explicit untouched rows. A mover the
                    // truthful homing already placed at this floating
                    // target (same-output boundary commit) carries
                    // membership-only proof: Controlled membership with no
                    // frame authority, never a frame write.
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
                        membership_authority: if boundary_mover {
                            ScopeAuthority::Controlled
                        } else {
                            ScopeAuthority::Untouched
                        },
                        frame_operation: None,
                        clear_operation: None,
                        floating: false,
                        float_origin: None,
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
                        floating: false,
                        float_origin: None,
                    });
                }
                continue;
            }
            let authority = if !domain.readable {
                ScopeAuthority::Hold(domain.hold.unwrap_or(HoldReason::Unreadable))
            } else if held_scopes.contains(&key) {
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
                scope: format!("{}/{}", key.output.0, key.workspace.0),
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
                    Self::push_held_lifetimes(
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
                        windows.push(WindowDesire {
                            lifetime: window.window.clone(),
                            output: window.output.clone(),
                            workspace: window.workspace.0.clone(),
                            allocation: None,
                            effective_frame: None,
                            frame_authority: ScopeAuthority::Hold(reason),
                            membership_authority: ScopeAuthority::Hold(reason),
                            frame_operation: None,
                            clear_operation: None,
                            floating: false,
                            float_origin: None,
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
                    Self::push_held_lifetimes(
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
                    Self::push_held_lifetimes(
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
                // spent (never re-actuated).
                let fact_unreadable = self
                    .staged
                    .facts
                    .get(&geometry.window)
                    .is_none_or(|fact| fact.minimized || fact.rect.is_none());
                let clear_operation = if report.gesture_barrier || fact_unreadable {
                    None
                } else {
                    self.retained_clears
                        .iter()
                        .find(|retained| {
                            retained.window == geometry.window && retained.pending.is_none()
                        })
                        .map(|retained| retained.operation_id.clone())
                };
                // Core-owned frame policy (stage 3a): an exact desired match
                // clears retained memory (completion is desired == observed,
                // never setter success); a suppressed pair publishes a
                // constraint-clamp hold with the desire intact so the adapter
                // skips the write and retries only on genuine change.
                let observed = staged
                    .facts
                    .get(&geometry.window)
                    .and_then(|fact| fact.rect);
                if observed == Some(geometry.rect) {
                    self.frames.remove(&geometry.window);
                }
                let frame_suppressed = observed.is_some_and(|current| {
                    self.frame_suppressed(
                        &geometry.window,
                        &geometry.rect,
                        Some(current),
                        report.monotonic_ms,
                    )
                });
                windows.push(WindowDesire {
                    lifetime: geometry.window.clone(),
                    output: geometry.output.clone(),
                    workspace: geometry.workspace.0.clone(),
                    allocation: Some(geometry.rect),
                    effective_frame: if fullscreen {
                        None
                    } else {
                        Some(geometry.rect)
                    },
                    frame_authority: if fullscreen {
                        ScopeAuthority::Untouched
                    } else if frame_suppressed {
                        ScopeAuthority::Hold(HoldReason::ConstraintClamp)
                    } else if frame_hold {
                        ScopeAuthority::Hold(HoldReason::GestureBarrier)
                    } else {
                        ScopeAuthority::Controlled
                    },
                    membership_authority: ScopeAuthority::Controlled,
                    // Tiled rows never carry a float-placement token.
                    frame_operation: None,
                    clear_operation,
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
                let retained_target = self
                    .retained_floats
                    .iter()
                    .find(|held| held.window == observed.window)
                    .map(|held| held.target);
                let mut frame_operation = self
                    .retained_floats
                    .iter()
                    .find(|held| held.window == observed.window)
                    .map(|held| held.operation_id.clone());
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
                                report.monotonic_ms,
                            ) =>
                        {
                            frame_operation = None;
                        }
                        _ => {}
                    }
                }
                // Explicit ordinary-float classification (D4/D5): every
                // retained exception row in a controlled scope converges
                // adapter float intent. The diagnostic capacity origin
                // rides along for truthful echo; it grants no behavior.
                let float_origin = session
                    .is_capacity_float(&observed.window)
                    .then_some(FloatOrigin::Capacity);
                // Retained directional clear desire: the one-shot
                // `clear_operation` token authorizes exactly one native
                // clear for this window. Withheld on held scopes, gesture
                // barriers, minimized/unreadable facts, and after the
                // attempt is spent (never re-actuated); the settlement
                // owns every verdict over the correlated attempt.
                let fact_unreadable = self
                    .staged
                    .facts
                    .get(&observed.window)
                    .is_none_or(|fact| fact.minimized || fact.rect.is_none());
                let clear_operation = if report.gesture_barrier || fact_unreadable {
                    None
                } else {
                    self.retained_clears
                        .iter()
                        .find(|retained| {
                            retained.window == observed.window && retained.pending.is_none()
                        })
                        .map(|retained| retained.operation_id.clone())
                };
                windows.push(WindowDesire {
                    lifetime: observed.window.clone(),
                    output: observed.output.clone(),
                    workspace: observed.workspace.0.clone(),
                    allocation: None,
                    effective_frame: retained_target
                        .or_else(|| session.floating_geometry(&observed.window)),
                    frame_authority: ScopeAuthority::Untouched,
                    membership_authority: if in_flight {
                        ScopeAuthority::Controlled
                    } else {
                        ScopeAuthority::Untouched
                    },
                    frame_operation,
                    clear_operation,
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
                windows.push(WindowDesire {
                    lifetime: window,
                    output: home_output,
                    workspace: home_workspace,
                    allocation: None,
                    effective_frame: None,
                    frame_authority: ScopeAuthority::Hold(HoldReason::Minimized),
                    membership_authority: ScopeAuthority::Controlled,
                    frame_operation: None,
                    clear_operation: None,
                    floating: false,
                    float_origin: None,
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
        // Minimized lifetimes never ride this lane: staged minimized facts
        // emit with minimized frame authority below.
        let minimized_ids: BTreeSet<&str> = staged
            .facts
            .values()
            .filter(|fact| fact.minimized)
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
            let placement = if migration_redirect.contains(window_id) {
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
                membership_authority: ScopeAuthority::Controlled,
                frame_operation: None,
                clear_operation: None,
                floating: false,
                float_origin: None,
            });
        }
        // Explicit minimized facts without a retained slot row: the adapter
        // reported them minimized but no session slot emitted a row above
        // (fresh minimize, held scope, or gap). Explicit minimized-hold
        // rows, never silent absence, never inferred close.
        for fact in staged.facts.values().filter(|fact| fact.minimized) {
            if live.contains(&fact.window.0) || !emitted.insert(fact.window.0.clone()) {
                continue;
            }
            // A minimized lifetime under an outstanding membership
            // transfer rides the target homing (one homing), keeping
            // minimized frame authority. The scope-hold lookup follows
            // the resolved homing.
            let (home_output, home_workspace) = redirect
                .get(&fact.window.0)
                .map(|target| (target.output.clone(), target.workspace.clone()))
                .unwrap_or_else(|| {
                    self.staged_homing(&fact.window)
                        .map(|key| (key.output, key.workspace))
                        .unwrap_or((fact.output.clone(), WorkspaceId(String::new())))
                });
            // No native membership evidence authorizes an empty logical target.
            if home_workspace.0.is_empty()
                && self.native_authoritative(report, &fact.output)
                && self.effective_logical_receipt(report, fact).is_none()
            {
                continue;
            }
            // A minimized lifetime homed on a held scope carries that
            // scope's hold on membership. Omission inside a held scope
            // authorizes no write or retirement.
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
                    None => ScopeAuthority::Controlled,
                },
                frame_operation: None,
                clear_operation: None,
                floating: false,
                float_origin: None,
            });
        }
        scopes.sort_by(|a, b| a.scope.cmp(&b.scope));
        // Omission of a minimized/absent window inside a controlled scope
        // means "no managed geometry", never a close or teardown instruction.
        let workspaces = self.plan_workspaces(report);
        let outputs = self.plan_outputs(report, held_scopes);
        // Per-report warp desire (stage 4b, cross-output only): computed
        // over the final focus effects plus the published window rows, so
        // explicit, admission, and hover-coupling focus share one rule.
        let cursor_warp = self.warp_for_publish(report, &effects, &windows, held_scopes);
        // Per-report gesture terminal: taken here so the next unrelated
        // report publishes `None`. Duplicates repeat the cached reply.
        let gesture_settled = self.gesture_terminal.take();
        // Per-report operation lifecycle terminals: taken here so the next
        // unrelated report publishes empty. Duplicates repeat the cached
        // reply.
        let operation_settled = std::mem::take(&mut self.operation_settled);
        WorldReply {
            contract_version: WORLD_CONTRACT_VERSION,
            adapter_incarnation: report.adapter_incarnation.clone(),
            core_generation: self.core_generation_str.clone(),
            request_id: report.request_id.clone(),
            based_on_observation_seq: report.observation_seq,
            state_revision: self.state_revision,
            ready: true,
            command_consumed,
            scopes,
            workspaces,
            outputs,
            windows,
            transfers: self.retained_transfers(),
            effects,
            command_result,
            gesture_settled,
            operation_settled,
            cursor_warp,
            presentation: crate::world_presentation::WorldPresentation::default(),
        }
    }

    /// Retained lifetime rows for a held scope: snapshot windows, exceptions,
    /// and minimized slots homed on `key`, each with narrow holds. Skips
    /// live lifetimes (they carry their own rows) and already-emitted ids,
    /// so every window keeps exactly one homing.
    pub(crate) fn push_held_lifetimes(
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
            if live.contains(&link.window.0) || !emitted.insert(link.window.0.clone()) {
                continue;
            }
            windows.push(WindowDesire {
                lifetime: link.window.clone(),
                output: link.output.clone(),
                workspace: link.workspace.0.clone(),
                allocation: None,
                effective_frame: None,
                frame_authority: ScopeAuthority::Hold(reason),
                membership_authority: ScopeAuthority::Hold(reason),
                frame_operation: None,
                clear_operation: None,
                floating: false,
                float_origin: None,
            });
        }
        for observed in session.exception_observed() {
            if observed.output != key.output || observed.workspace != key.workspace {
                continue;
            }
            if live.contains(&observed.window.0) || !emitted.insert(observed.window.0.clone()) {
                continue;
            }
            windows.push(WindowDesire {
                lifetime: observed.window.clone(),
                output: observed.output.clone(),
                workspace: observed.workspace.0.clone(),
                allocation: None,
                effective_frame: None,
                frame_authority: ScopeAuthority::Hold(reason),
                membership_authority: ScopeAuthority::Hold(reason),
                frame_operation: None,
                clear_operation: None,
                floating: false,
                float_origin: None,
            });
        }
        for (window, output, workspace) in session.minimized_entries() {
            if output != key.output || workspace != key.workspace {
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
    /// so rows publish the retained set directly. `existence` is always
    /// true: the adapter never deletes logical workspaces. Incomplete
    /// screens hold their rows without authorizing writes.
    pub(crate) fn plan_workspaces(&mut self, report: &WorldReport) -> Vec<WorkspaceDesire> {
        let _ = report;
        let mut outputs: Vec<String> = self.mapping.keys().cloned().collect();
        outputs.sort();
        let staged = self.staged.clone();
        let mut rows = Vec::new();
        for output in outputs {
            let Some(ring) = self.mapping.get(&output).cloned() else {
                continue;
            };
            let held = !staged
                .screens
                .iter()
                .any(|screen| screen.output.0 == output && screen.complete);
            for (index, id) in ring.iter().enumerate() {
                let native_binding = if report.host_owned_workspace_domains {
                    self.native_binding_for(&output, id)
                } else {
                    None
                };
                // An incompatible fresh shared projection holds its
                // workspace rows through the existing Unreadable lane;
                // ordinary incomplete screens keep IncompleteInventory.
                let hold = if held {
                    Some(HoldReason::IncompleteInventory)
                } else if self.native_order_conflict(report) {
                    Some(HoldReason::Unreadable)
                } else {
                    None
                };
                rows.push(WorkspaceDesire {
                    id: id.clone(),
                    output: OutputId(output.clone()),
                    native_binding,
                    scope: workspace_scope_key(report.workspace_mode, &OutputId(output.clone())),
                    order: index,
                    tiling_mode: self.workspace_tiling_mode(&OutputId(output.clone()), id),
                    existence: true,
                    hold,
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
        let _ = report;
        report
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
                // capability tolerates such echoes (observed only).
                let ring = self.mapping.get(&screen.output.0);
                let echo_unknown = report
                    .views
                    .iter()
                    .find(|view| view.output == screen.output)
                    .is_some_and(|view| {
                        !view.view.is_empty()
                            && !ring.is_some_and(|ring| ring.iter().any(|id| id == &view.view))
                            && !report.host_owned_workspace_domains
                    });
                let unreadable = !screen.complete
                    || echo_unknown
                    || held_scopes.iter().any(|key| key.output == screen.output);
                OutputDesire {
                    output: screen.output.clone(),
                    logical_view,
                    tiling_mode,
                    hold: unreadable.then_some(HoldReason::IncompleteInventory),
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
    if fact.minimized
        || fact.sticky
        || fact.floating
        || fact.fullscreen
        || fact.maximized
        || fact.fit_excluded
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
    let observed_output = staged.fact_output.get(window)?;
    if *observed_output == target.output {
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
