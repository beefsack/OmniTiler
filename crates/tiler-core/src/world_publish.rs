//! World desired-state publishing (D1-D5): one full snapshot per accepted
//! report.
//!
//! Split from [`crate::world_engine`]: retained-session projection, the
//! planner-owned workspace lifecycle (all three modes), output views, and
//! explicit lifetime/hold rows. Never authorizes writes inside held scopes.

use std::collections::{BTreeMap, BTreeSet};

use crate::boundary::{CoreReply, ProjectionKind, project_retained_tiled_geometry};
use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::seed::EngineWindow;
use crate::session::{DomainKey, RefusalKind};
use crate::workspace::plan_trailing;
use crate::workspace_scope::{
    LiveView, ScopedTopology, WorkspaceMode, rebuild_global_mapping, rebuild_local_mapping,
    rebuild_shared_order, scoped_ring_ids, scoped_trailing_facts,
};
use crate::world_engine::WorldEngine;
use crate::world_report::{
    CommandResult, HoldReason, OneShotEffect, OutputDesire, ScopeAuthority, ScopeStatus,
    TilingMode, TransferKind, TransferTarget, WORLD_CONTRACT_VERSION, WindowDesire,
    WorkspaceDesire, WorldReply, WorldReport, workspace_scope_key,
};

impl WorldEngine {
    /// One-place hold classification for sweep failures (capacity B).
    /// This is the single replaceable policy: only proven geometric
    /// exhaustion keeps topology and holds as [`HoldReason::ConstraintClamp`].
    /// In the sweep path the input is already validated and the command is
    /// always `Reconcile`, so the two seed/projection refusal shapes mean
    /// the layout ran out of representable pixels
    /// (`extent - gap*(children-1) < children` with no positive child
    /// segment, R-MIN-04/R-INS-09): `malformed-topology` from admission or
    /// retained-tree projection, and `seed-failed` from the seed build
    /// (`Session::new` inputs are pre-validated, so only the geometric
    /// admit steps can still fail there). Every other sweep failure
    /// (divergence, pending, partial observation, commit rejection, ...)
    /// holds as [`HoldReason::Unreadable`]: the adapter must not write
    /// there, and recovery retries the full report. No admission fallback
    /// lives here; best-effort admission is separate work that replaces or
    /// extends this function, never a hidden second policy.
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
    /// shared trailing policy, retained transfers/views, and new effects.
    /// Every retained session missing from the report becomes an explicit
    /// held scope row (never silent absence); minimized lifetimes are
    /// explicit minimized-hold rows (never omission, never inferred close);
    /// held report scopes carry their retained lifetimes as hold rows.
    /// Floating scopes carry explicit untouched lifetime rows (never
    /// emptiness): observed non-movers publish with no authority, and
    /// movers under an outstanding boundary transfer publish one
    /// membership-only row at the target homing. No floating row ever
    /// carries allocation or frame authority (no frame writes on floats).
    /// A held readable scope with no retained session (fresh-seed geometric
    /// exhaustion) carries its observed lifetimes as complete held rows, so
    /// no inventory ever vanishes. Feedback holds apply narrowly to the
    /// reported window's membership only.
    pub(crate) fn publish(
        &mut self,
        report: &WorldReport,
        command_consumed: bool,
        effects: Vec<OneShotEffect>,
        command_result: Option<CommandResult>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> WorldReply {
        let mut scopes = Vec::with_capacity(report.domains.len());
        let mut windows = Vec::new();
        // Live lifetimes: observed windows in controlled domains plus
        // retained exception windows. Minimized and held-scope rows never
        // shadow a live row for the same lifetime regardless of domain
        // order: one homing per window, so a stale slot (e.g. an
        // externally moved window's old domain) cannot duplicate the live
        // placement.
        let mut live: BTreeSet<String> = BTreeSet::new();
        for domain in &report.domains {
            if !domain.readable || held_scopes.contains(&domain.domain.key()) {
                continue;
            }
            for window in &domain.windows {
                live.insert(window.window.0.clone());
            }
            if let Some(session) = self.engine.session(&domain.domain.key()) {
                for observed in session.exception_observed() {
                    live.insert(observed.window.0.clone());
                }
            }
        }
        // Backstop dedupe among non-live rows themselves.
        let mut emitted: BTreeSet<String> = BTreeSet::new();
        // Movers with an outstanding window transfer into a verifiable
        // target present at the target homing: tiled source rows redirect
        // there (one homing), and the trailing loop below emits the
        // membership-only row when no sweep row exists (boundary first
        // replies and policy placements, where no Engine plan committed).
        // Command targets verify through a readable reported domain;
        // policy (hotplug) targets verify through the live native
        // inventory instead, since the homing may not be reported yet —
        // the adapter resolves it natively and holds when unresolvable.
        // Targets that cannot verify keep the source row so the lifetime
        // never vanishes.
        let live_order = live_native_order(report);
        let live_ws: BTreeSet<&str> = live_order.iter().map(String::as_str).collect();
        let mut redirect: BTreeMap<String, (DomainKey, bool)> = BTreeMap::new();
        for retained in &self.retained {
            if let TransferKind::Window(window) = &retained.target.kind {
                let key = DomainKey {
                    output: retained.target.target_output.clone(),
                    workspace: WorkspaceId(retained.target.target_workspace.clone()),
                };
                if held_scopes.contains(&key) {
                    continue;
                }
                let readable = report
                    .domains
                    .iter()
                    .any(|d| d.readable && d.domain.key() == key);
                if !readable && !(retained.policy && live_ws.contains(key.workspace.0.as_str())) {
                    continue;
                }
                redirect.insert(window.0.clone(), (key, retained.policy));
            }
        }
        // Fullscreen overlays on tiled links: the native overlay owns
        // pixels, so no automatic frame authority ever publishes even
        // though the tile stays reserved (allocation present, no
        // effective frame). The adapter overlay guard double-checks.
        let fullscreen_windows: BTreeSet<&str> = report
            .domains
            .iter()
            .flat_map(|domain| domain.windows.iter())
            .filter(|window| window.fullscreen)
            .map(|window| window.window.0.as_str())
            .collect();
        // Narrow feedback holds by window lifetime: a refused/held attempt
        // holds that window's membership, never the whole system.
        let mut window_holds: BTreeMap<String, HoldReason> = BTreeMap::new();
        for retained in &self.retained {
            let hold = report
                .feedback
                .iter()
                .find(|f| f.operation_id == retained.target.operation_id)
                .and_then(|f| f.hold);
            if let (Some(hold), TransferKind::Window(window)) = (hold, &retained.target.kind) {
                window_holds.entry(window.0.clone()).or_insert(hold);
            }
        }
        for domain in &report.domains {
            let key = domain.domain.key();
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
                    // Movers redirected to a verifiable target homing emit
                    // nothing here (one homing); all other observed
                    // lifetimes carry explicit untouched rows.
                    if redirect.contains_key(&window.window.0) {
                        continue;
                    }
                    if !emitted.insert(window.window.0.clone()) {
                        continue;
                    }
                    windows.push(WindowDesire {
                        lifetime: window.window.clone(),
                        output: window.output.clone(),
                        workspace: window.workspace.0.clone(),
                        allocation: None,
                        effective_frame: None,
                        frame_authority: ScopeAuthority::Untouched,
                        membership_authority: ScopeAuthority::Untouched,
                        frame_operation: None,
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
                } else if domain.readable {
                    // No retained session (fresh-seed geometric exhaustion):
                    // the observed lifetimes ARE the complete held inventory.
                    // One row per observed window with no allocation and no
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
                        let member_hold = window_holds.get(&window.0).copied();
                        windows.push(WindowDesire {
                            lifetime: window,
                            output: output.clone(),
                            workspace: workspace.0.clone(),
                            allocation: None,
                            effective_frame: None,
                            frame_authority: ScopeAuthority::Hold(HoldReason::Minimized),
                            membership_authority: match member_hold {
                                Some(reason) => ScopeAuthority::Hold(reason),
                                None => ScopeAuthority::Controlled,
                            },
                            frame_operation: None,
                        });
                    }
                    continue;
                }
            }
            let shielded = self.shielded_windows(&key, &domain.windows, &{
                let mut map: BTreeMap<&WindowId, &EngineWindow> = BTreeMap::new();
                for candidate in &report.domains {
                    for window in &candidate.windows {
                        map.insert(&window.window, window);
                    }
                }
                map
            });
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
            let frame_hold = report.gesture_barrier;
            let plan = project_retained_tiled_geometry(
                session,
                &key,
                domain.domain.bounds,
                domain.domain.gap,
                focus_pair,
                ProjectionKind::Reconcile,
                &hints,
                &shielded,
            );
            let Some(plan) = plan else {
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
            for geometry in &plan.geometry {
                // Outstanding movers present at the transfer target: the
                // trailing loop emits their membership-only row (one
                // homing), unless the target is this same scope (the
                // engine-committed row below is already correct).
                if redirect
                    .get(&geometry.window.0)
                    .is_some_and(|(target, _)| *target != key)
                {
                    continue;
                }
                let member_hold = window_holds.get(&geometry.window.0).copied();
                let fullscreen = fullscreen_windows.contains(geometry.window.0.as_str());
                emitted.insert(geometry.window.0.clone());
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
                    } else if frame_hold {
                        ScopeAuthority::Hold(HoldReason::GestureBarrier)
                    } else {
                        ScopeAuthority::Controlled
                    },
                    membership_authority: match member_hold {
                        Some(reason) => ScopeAuthority::Hold(reason),
                        None => ScopeAuthority::Controlled,
                    },
                    // Tiled rows never carry a float-placement token.
                    frame_operation: None,
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
                let member_hold = window_holds.get(&observed.window.0).copied();
                emitted.insert(observed.window.0.clone());
                // Explicit retained float target, if outstanding: the row
                // carries the stable property operation id so the adapter
                // places the exact retained frame exactly once. Ordinary
                // floats carry none and never take frame writes.
                let frame_operation = self
                    .retained_floats
                    .iter()
                    .find(|held| held.window == observed.window)
                    .map(|held| held.operation_id.clone());
                windows.push(WindowDesire {
                    lifetime: observed.window.clone(),
                    output: observed.output.clone(),
                    workspace: observed.workspace.0.clone(),
                    allocation: None,
                    effective_frame: session.floating_geometry(&observed.window),
                    frame_authority: ScopeAuthority::Untouched,
                    membership_authority: match member_hold {
                        Some(reason) => ScopeAuthority::Hold(reason),
                        None if in_flight => ScopeAuthority::Controlled,
                        None => ScopeAuthority::Untouched,
                    },
                    frame_operation,
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
                let member_hold = window_holds.get(&window.0).copied();
                windows.push(WindowDesire {
                    lifetime: window,
                    output: output.clone(),
                    workspace: workspace.0.clone(),
                    allocation: None,
                    effective_frame: None,
                    frame_authority: ScopeAuthority::Hold(HoldReason::Minimized),
                    membership_authority: match member_hold {
                        Some(reason) => ScopeAuthority::Hold(reason),
                        None => ScopeAuthority::Controlled,
                    },
                    frame_operation: None,
                });
            }
        }
        // Redirected movers without a sweep row yet: one membership-only
        // row at the transfer target homing (frame untouched, never
        // allocation or geometry). Ordinary engine-committed sends already
        // carry sweep rows, so the emitted guard keeps exactly one homing.
        for (window_id, (key, _)) in &redirect {
            if !emitted.insert(window_id.clone()) {
                continue;
            }
            let member_hold = window_holds.get(window_id).copied();
            let window = WindowId(window_id.clone());
            windows.push(WindowDesire {
                lifetime: window,
                output: key.output.clone(),
                workspace: key.workspace.0.clone(),
                allocation: None,
                effective_frame: None,
                frame_authority: ScopeAuthority::Untouched,
                membership_authority: match member_hold {
                    Some(reason) => ScopeAuthority::Hold(reason),
                    None => ScopeAuthority::Controlled,
                },
                frame_operation: None,
            });
        }
        // Explicit minimized lifetimes without a retained slot row: the
        // adapter reported them minimized but no session slot emitted a
        // row above (fresh minimize, held scope, or gap). Explicit
        // minimized-hold rows, never silent absence, never inferred close.
        for entry in &report.minimized {
            if live.contains(&entry.window.0) || !emitted.insert(entry.window.0.clone()) {
                continue;
            }
            let member_hold = window_holds.get(&entry.window.0).copied();
            // A minimized lifetime homed on a held scope carries that
            // scope's hold on membership: unreadable scopes use the report
            // hold, sweep-held scopes (including geometric exhaustion) the
            // classified reason. Omission inside a held scope authorizes no
            // write or retirement.
            let scope_hold = report
                .domains
                .iter()
                .find(|domain| {
                    domain.domain.id == entry.output && domain.domain.workspace == entry.workspace
                })
                .and_then(|domain| {
                    if !domain.readable {
                        Some(domain.hold.unwrap_or(HoldReason::Unreadable))
                    } else {
                        self.hold_reasons.get(&domain.domain.key()).copied()
                    }
                });
            windows.push(WindowDesire {
                lifetime: entry.window.clone(),
                output: entry.output.clone(),
                workspace: entry.workspace.0.clone(),
                allocation: None,
                effective_frame: None,
                frame_authority: ScopeAuthority::Hold(HoldReason::Minimized),
                membership_authority: match member_hold.or(scope_hold) {
                    Some(reason) => ScopeAuthority::Hold(reason),
                    None => ScopeAuthority::Controlled,
                },
                frame_operation: None,
            });
        }
        // Retained sessions missing from the report: explicit held scope
        // rows so absence never reads as closure or emptiness. Omission
        // inside a held scope authorizes no write or retirement.
        let mut reported_keys: BTreeSet<DomainKey> = BTreeSet::new();
        for domain in &report.domains {
            reported_keys.insert(domain.domain.key());
        }
        let mut missing: Vec<DomainKey> = self
            .engine
            .domain_keys()
            .into_iter()
            .filter(|key| !reported_keys.contains(key))
            .collect();
        missing.sort();
        for key in missing {
            scopes.push(ScopeStatus {
                scope: format!("{}/{}", key.output.0, key.workspace.0),
                authority: ScopeAuthority::Hold(HoldReason::IncompleteInventory),
            });
            // Lifetimes retained here and still observed under stale homing
            // (e.g. a just migrated workspace the adapter has not actuated
            // yet) still need explicit rows or they vanish from the reply
            // entirely. Unlike [`WorldEngine::push_held_lifetimes`], only
            // observed-but-rowless lifetimes emit here: unobserved members
            // stay omitted (a gap authorizes nothing), and the `emitted`
            // guard keeps exactly one homing per window.
            if let Some(session) = self.engine.session(&key) {
                Self::push_observed_unrowed_lifetimes(
                    session,
                    &key,
                    &live,
                    &mut emitted,
                    &mut windows,
                );
            }
        }
        scopes.sort_by(|a, b| a.scope.cmp(&b.scope));
        // Omission of a minimized/absent window inside a controlled scope
        // means "no managed geometry", never a close or teardown instruction.
        let workspaces = self.plan_workspaces(report);
        let outputs = self.plan_outputs(report, held_scopes);
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
            presentation_bytes: 0,
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
            });
        }
    }

    /// Retained lifetime rows for a retained session the report does not
    /// cover (see the missing-session path in [`WorldEngine::publish`]),
    /// limited to members still observed under stale homing: snapshot
    /// windows, exceptions, and minimized slots homed on `key` with narrow
    /// incomplete-inventory holds. Unlike
    /// [`WorldEngine::push_held_lifetimes`] (which skips observed lifetimes
    /// because they carry their own rows), only observed-but-rowless
    /// lifetimes emit here: unobserved members stay omitted, and the
    /// `emitted` guard keeps exactly one homing per window.
    pub(crate) fn push_observed_unrowed_lifetimes(
        session: &crate::session::Session,
        key: &DomainKey,
        live: &BTreeSet<String>,
        emitted: &mut BTreeSet<String>,
        windows: &mut Vec<WindowDesire>,
    ) {
        for link in session.snapshot().windows {
            if link.output != key.output || link.workspace != key.workspace {
                continue;
            }
            if !live.contains(&link.window.0) || !emitted.insert(link.window.0.clone()) {
                continue;
            }
            windows.push(WindowDesire {
                lifetime: link.window.clone(),
                output: link.output.clone(),
                workspace: link.workspace.0.clone(),
                allocation: None,
                effective_frame: None,
                frame_authority: ScopeAuthority::Hold(HoldReason::IncompleteInventory),
                membership_authority: ScopeAuthority::Hold(HoldReason::IncompleteInventory),
                frame_operation: None,
            });
        }
        for observed in session.exception_observed() {
            if observed.output != key.output || observed.workspace != key.workspace {
                continue;
            }
            if !live.contains(&observed.window.0) || !emitted.insert(observed.window.0.clone()) {
                continue;
            }
            windows.push(WindowDesire {
                lifetime: observed.window.clone(),
                output: observed.output.clone(),
                workspace: observed.workspace.0.clone(),
                allocation: None,
                effective_frame: None,
                frame_authority: ScopeAuthority::Hold(HoldReason::IncompleteInventory),
                membership_authority: ScopeAuthority::Hold(HoldReason::IncompleteInventory),
                frame_operation: None,
            });
        }
        for (window, output, workspace) in session.minimized_entries() {
            if output != key.output || workspace != key.workspace {
                continue;
            }
            if !live.contains(&window.0) || !emitted.insert(window.0.clone()) {
                continue;
            }
            windows.push(WindowDesire {
                lifetime: window,
                output: output.clone(),
                workspace: workspace.0.clone(),
                allocation: None,
                effective_frame: None,
                frame_authority: ScopeAuthority::Hold(HoldReason::Minimized),
                membership_authority: ScopeAuthority::Hold(HoldReason::IncompleteInventory),
                frame_operation: None,
            });
        }
    }

    /// Retained tiling mode for one native workspace: absent means tiled.
    /// Seeding (see [`WorldEngine::seed_workspace_tiling`]) keeps every
    /// live native present, so absence here is only a backstop. Set through
    /// the planner-owned tiling command or the saved-default seed; the
    /// adapter never decides.
    pub(crate) fn workspace_tiling_mode(&self, output: &OutputId, workspace: &str) -> TilingMode {
        self.tiling
            .get(&DomainKey {
                output: output.clone(),
                workspace: WorkspaceId(workspace.to_owned()),
            })
            .copied()
            .unwrap_or(TilingMode::Tiled)
    }

    /// Seed planner-owned per-workspace tiling from the reported saved
    /// default (core owns seed policy; the adapter only reports the saved
    /// value). Absent live natives take the current default; present keys
    /// (manual toggles, earlier seeds) never reseed, so live default
    /// changes seed only subsequently observed workspaces. Stale keys for
    /// no-longer-live natives prune unless a pinned/displaced binding
    /// retains them, so a reused native id reseeds from the current
    /// default instead of inheriting a stale mode.
    pub(crate) fn seed_workspace_tiling(&mut self, report: &WorldReport) {
        let seed = if report.default_tiled {
            TilingMode::Tiled
        } else {
            TilingMode::Floating
        };
        let mut live: BTreeSet<DomainKey> = BTreeSet::new();
        for topology in &report.topology {
            for native in &topology.native_order {
                live.insert(DomainKey {
                    output: topology.output.clone(),
                    workspace: WorkspaceId(native.clone()),
                });
            }
        }
        let live_ids: BTreeSet<&str> = live.iter().map(|key| key.workspace.0.as_str()).collect();
        let retained: BTreeSet<String> = report
            .bindings
            .iter()
            .filter(|binding| binding.pinned || binding.displaced)
            .map(|binding| binding.native_id.clone())
            .collect();
        // Prune only workspaces that left the live set entirely (or
        // unretained bindings): keys for removed outputs stay while
        // their workspace survives anywhere, so hotplug displacement
        // carries manual modes instead of reseeding them.
        self.tiling.retain(|key, _| {
            live_ids.contains(key.workspace.0.as_str()) || retained.contains(&key.workspace.0)
        });
        for key in live {
            self.tiling.entry(key).or_insert(seed);
        }
    }

    /// Planner-owned workspace policy (D2): the retained mapping rebuilds
    /// every accepted report through the shared mapping helpers, trailing
    /// lifecycle runs per scope through the shared facts, and rows carry
    /// scope, order, and tiling mode. Rings come from the mapping (shared
    /// lifecycle runs per scope through the shared facts, and rows carry
    /// scope, order, and tiling mode. Rings come from the mapping (shared
    /// order for shared mode); facts exclude sticky all-desktop occupancy
    /// via homed-domain counting (a sticky row occupies only its homed
    /// domain, never every backing workspace).
    pub(crate) fn plan_workspaces(&mut self, report: &WorldReport) -> Vec<WorkspaceDesire> {
        // Occupancy by native workspace id from homed live rows.
        let occupied = homed_occupancy(report);
        // Bind pending creations first: a binding that names the logical id
        // resolves the token and records the native id.
        let mut bound_logical: BTreeSet<String> = BTreeSet::new();
        for binding in &report.bindings {
            if self
                .pending_creations
                .iter()
                .any(|pending| pending.logical_id == binding.logical_id)
            {
                bound_logical.insert(binding.logical_id.clone());
            }
        }
        self.pending_creations
            .retain(|pending| !bound_logical.contains(&pending.logical_id));
        let mut rows = Vec::new();
        // Native workspace -> logical id for bound rows.
        let logical_of: BTreeMap<&str, &str> = report
            .bindings
            .iter()
            .map(|binding| (binding.native_id.as_str(), binding.logical_id.as_str()))
            .collect();
        if report.workspace_mode == WorkspaceMode::Shared {
            rows.extend(self.shared_workspace_rows(report, &occupied, &logical_of));
            return rows;
        }
        // Natives assigned to any scope ring already have a homing row;
        // the adopted-natives fallback below lists only genuinely
        // unassigned ids (never a second row under another output).
        let assigned: BTreeSet<String> = self
            .mapping
            .values()
            .flat_map(|ids| ids.iter().cloned())
            .collect();
        for topology in &report.topology {
            // Lifecycle proceeds only with complete enumeration for this
            // output AND readable observations for all of its domains;
            // otherwise every row holds and nothing is removed or created.
            let readable_complete = topology.complete
                && report
                    .domains
                    .iter()
                    .filter(|d| d.domain.id == topology.output)
                    .all(|d| d.readable);
            let mut ring: Vec<String> = self
                .mapping
                .get(&topology.output.0)
                .cloned()
                .unwrap_or_default();
            // Lifecycle follows native topology order: adopted ids join at
            // the end of the retained mapping, so sort the ring into
            // native position (unknowns last) before planning.
            ring.sort_by_key(|id| {
                topology
                    .native_order
                    .iter()
                    .position(|native| native == id)
                    .unwrap_or(usize::MAX)
            });
            let visible: BTreeSet<String> = [topology.current_native.clone()].into_iter().collect();
            let mut retained_ids: BTreeSet<String> = report
                .bindings
                .iter()
                .filter(|binding| {
                    binding.output == topology.output && (binding.pinned || binding.displaced)
                })
                .map(|binding| binding.native_id.clone())
                .collect();
            // Core-displaced units hosted here are retained against
            // trailing cleanup even when empty and invisible.
            for entry in self.displaced.values() {
                if entry.dest == topology.output.0 {
                    retained_ids.extend(entry.workspaces.iter().cloned());
                }
            }
            let facts = scoped_trailing_facts(&ring, &visible, &occupied, &retained_ids);
            let plan = plan_trailing(&facts);
            let removed: BTreeSet<usize> = plan.remove.into_iter().collect();
            for (index, native) in ring.iter().enumerate() {
                let held = !readable_complete;
                rows.push(WorkspaceDesire {
                    id: native.clone(),
                    output: topology.output.clone(),
                    native_binding: Some(native.clone()),
                    logical_id: logical_of.get(native.as_str()).map(|s| (*s).to_owned()),
                    scope: workspace_scope_key(report.workspace_mode, &topology.output),
                    order: index,
                    tiling_mode: self.workspace_tiling_mode(&topology.output, native),
                    // Desired absence is eligibility only; unknown occupancy
                    // keeps.
                    existence: held || !removed.contains(&index),
                    hold: held.then_some(HoldReason::IncompleteInventory),
                });
            }
            // Native ids observed outside every retained ring (hotplug,
            // adopted natives) are listed stably after the ring, except
            // ids homed in another scope ring (never a second row under
            // another output). The output's own current view always
            // keeps its row even when assigned elsewhere, so visibility
            // never loses its mode/lifecycle row.
            for native in &topology.native_order {
                if !ring.iter().any(|id| id == native)
                    && (!assigned.contains(native.as_str()) || *native == topology.current_native)
                {
                    let held = !readable_complete;
                    rows.push(WorkspaceDesire {
                        id: native.clone(),
                        output: topology.output.clone(),
                        native_binding: Some(native.clone()),
                        logical_id: logical_of.get(native.as_str()).map(|s| (*s).to_owned()),
                        scope: workspace_scope_key(report.workspace_mode, &topology.output),
                        order: rows.len(),
                        tiling_mode: self.workspace_tiling_mode(&topology.output, native),
                        existence: true,
                        hold: held.then_some(HoldReason::IncompleteInventory),
                    });
                }
            }
            if plan.need_append && readable_complete {
                // Exhaustion refuses the new token: no row is appended (the
                // next report retries), never a wrapped or aliased token.
                if let Some(logical) = self.pending_token(&topology.output) {
                    rows.push(WorkspaceDesire {
                        id: logical.clone(),
                        output: topology.output.clone(),
                        native_binding: None,
                        logical_id: Some(logical),
                        scope: workspace_scope_key(report.workspace_mode, &topology.output),
                        order: rows.len(),
                        tiling_mode: seed_tiling_mode(report),
                        existence: true,
                        hold: None,
                    });
                }
            } else if readable_complete {
                // No spare needed: an output-scoped pending token with no
                // binding is cancelled so identical replies never pile up.
                self.pending_creations
                    .retain(|pending| pending.output != topology.output);
            }
        }
        rows
    }

    /// Shared-mode workspace rows (D2): one ring across all outputs, the
    /// same trailing lifecycle, scope `shared`.
    pub(crate) fn shared_workspace_rows(
        &mut self,
        report: &WorldReport,
        occupied: &BTreeSet<String>,
        logical_of: &BTreeMap<&str, &str>,
    ) -> Vec<WorkspaceDesire> {
        let mut rows = Vec::new();
        let readable_complete =
            report.topology.iter().all(|t| t.complete) && report.domains.iter().all(|d| d.readable);
        let visible: BTreeSet<String> = report
            .topology
            .iter()
            .map(|topology| topology.current_native.clone())
            .collect();
        let retained_ids: BTreeSet<String> = report
            .bindings
            .iter()
            .filter(|binding| binding.pinned || binding.displaced)
            .map(|binding| binding.native_id.clone())
            .collect();
        let facts = scoped_trailing_facts(
            &self.shared_order.clone(),
            &visible,
            occupied,
            &retained_ids,
        );
        let plan = plan_trailing(&facts);
        let removed: BTreeSet<usize> = plan.remove.into_iter().collect();
        let output = report
            .topology
            .first()
            .map(|topology| topology.output.clone())
            .unwrap_or(OutputId("shared".to_owned()));
        for (index, native) in self.shared_order.clone().into_iter().enumerate() {
            let held = !readable_complete;
            rows.push(WorkspaceDesire {
                id: native.clone(),
                output: output.clone(),
                native_binding: Some(native.clone()),
                logical_id: logical_of.get(native.as_str()).map(|s| (*s).to_owned()),
                scope: workspace_scope_key(report.workspace_mode, &output),
                order: index,
                tiling_mode: self.workspace_tiling_mode(&output, &native),
                existence: held || !removed.contains(&index),
                hold: held.then_some(HoldReason::IncompleteInventory),
            });
        }
        if plan.need_append && readable_complete {
            // Exhaustion refuses the new token: no row is appended (the
            // next report retries), never a wrapped or aliased token.
            if let Some(logical) = self.pending_token(&output) {
                rows.push(WorkspaceDesire {
                    id: logical.clone(),
                    output: output.clone(),
                    native_binding: None,
                    logical_id: Some(logical),
                    scope: workspace_scope_key(report.workspace_mode, &output),
                    order: rows.len(),
                    tiling_mode: seed_tiling_mode(report),
                    existence: true,
                    hold: None,
                });
            }
        } else if readable_complete {
            self.pending_creations
                .retain(|pending| pending.output != output);
        }
        rows
    }

    /// Planner-owned mapping/history convergence (D2): rebuild the scoped
    /// assignment from live topology plus the retained mapping, adopt
    /// unassigned natives onto the primary, record verified view changes,
    /// and discard histories for disconnected outputs. Pure helpers decide;
    /// this only retains their results.
    pub(crate) fn converge_workspace_policy(&mut self, report: &WorldReport) {
        let live_order = live_native_order(report);
        let outputs: Vec<String> = report
            .topology
            .iter()
            .map(|topology| topology.output.0.clone())
            .collect();
        let primary = outputs.first().cloned();
        // Seed assignments from reported bindings (logical->native grouped
        // by output, in native topology order) so multi-output partitions
        // survive rebuilds; the retained mapping wins for ids both know,
        // and rebuild adopts any leftover live id onto the primary.
        let mut assignments = self.mapping.clone();
        if !report.bindings.is_empty() {
            let mut seeded: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for topology in &report.topology {
                let mut ids: Vec<String> = report
                    .bindings
                    .iter()
                    .filter(|binding| binding.output == topology.output)
                    .map(|binding| binding.native_id.clone())
                    .filter(|id| live_order.iter().any(|live| live == id))
                    .collect();
                ids.sort_by_key(|id| {
                    live_order
                        .iter()
                        .position(|live| live == id)
                        .unwrap_or(usize::MAX)
                });
                ids.dedup();
                if !ids.is_empty() {
                    seeded.insert(topology.output.0.clone(), ids);
                }
            }
            if !seeded.is_empty() {
                // Retained entries for live outputs win member-by-member:
                // keep retained ids that are still live, then append newly
                // bound ids the retained mapping does not know.
                for (output, ids) in seeded {
                    let entry = assignments.entry(output).or_default();
                    entry.retain(|id| live_order.iter().any(|live| live == id));
                    for id in ids {
                        if !entry.iter().any(|kept| kept == &id) {
                            entry.push(id);
                        }
                    }
                }
            }
        }
        let topology = ScopedTopology {
            outputs,
            live_order: live_order.clone(),
            assignments,
            primary,
        };
        let rebuilt = match report.workspace_mode {
            WorkspaceMode::GlobalUnique => rebuild_global_mapping(&topology),
            WorkspaceMode::PerOutputLocal | WorkspaceMode::Shared => {
                rebuild_local_mapping(&topology)
            }
        };
        self.mapping = rebuilt.mapping;
        self.shared_order = rebuild_shared_order(&live_order);
        // Hotplug displacement/return moves mapping entries between
        // scopes before history observes this report's views, so rings
        // and rows see displaced units under their survivor immediately.
        self.converge_displacement(report);
        // Verified view observations: current natives scoped by the fresh
        // mapping rings. Same-workspace views never record; unknown
        // currents record nothing; absent outputs are discarded.
        let mut live_views: BTreeMap<String, LiveView> = BTreeMap::new();
        for topology in &report.topology {
            let scope = scoped_ring_ids(
                report.workspace_mode,
                &self.mapping,
                &self.shared_order,
                &topology.output.0,
                &live_order,
            );
            live_views.insert(
                topology.output.0.clone(),
                LiveView {
                    current: topology.current_native.clone(),
                    scope,
                },
            );
        }
        self.history.observe_all(&live_views);
        if report.workspace_mode == WorkspaceMode::Shared
            && let Some(first) = report.topology.first()
        {
            self.history
                .observe_shared(&live_order, &first.current_native);
        }
    }

    /// Desired output views: retained view targets win; otherwise the mapped
    /// current view (informational, never a focus command). Incomplete
    /// enumeration or unreadable domains for the output hold the row: the
    /// adapter must not actuate views it cannot verify.
    pub(crate) fn plan_outputs(
        &self,
        report: &WorldReport,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Vec<OutputDesire> {
        report
            .topology
            .iter()
            .map(|topology| {
                let pending_view = self.retained.iter().find_map(|r| match &r.target.kind {
                    TransferKind::View if r.target.target_output == topology.output => {
                        Some(r.target.target_workspace.clone())
                    }
                    _ => None,
                });
                let native_view: &str = pending_view
                    .as_deref()
                    .unwrap_or(topology.current_native.as_str());
                let logical_view = report
                    .bindings
                    .iter()
                    .find(|binding| {
                        binding.output == topology.output && binding.native_id == native_view
                    })
                    .map(|binding| binding.logical_id.clone());
                let unreadable = !topology.complete
                    || report
                        .domains
                        .iter()
                        .any(|domain| domain.domain.id == topology.output && !domain.readable)
                    || held_scopes.iter().any(|key| key.output == topology.output);
                OutputDesire {
                    output: topology.output.clone(),
                    logical_view,
                    tiling_mode: self.workspace_tiling_mode(&topology.output, native_view),
                    hold: unreadable.then_some(HoldReason::IncompleteInventory),
                }
            })
            .collect()
    }
}

pub(crate) fn observed_homing(report: &WorldReport, window: &WindowId) -> Option<DomainKey> {
    for domain in &report.domains {
        if domain.windows.iter().any(|entry| entry.window == *window) {
            return Some(domain.domain.key());
        }
    }
    None
}

/// Live native workspace ids in deterministic topology order.
pub(crate) fn live_native_order(report: &WorldReport) -> Vec<String> {
    let mut live = Vec::new();
    for topology in &report.topology {
        for native in &topology.native_order {
            if !live.iter().any(|id| id == native) {
                live.push(native.clone());
            }
        }
    }
    live
}

/// Saved-default seed for pending (not yet native) workspace rows:
/// newly created workspaces take the current reported default, matching
/// [`WorldEngine::seed_workspace_tiling`] for newly observed natives.
fn seed_tiling_mode(report: &WorldReport) -> TilingMode {
    if report.default_tiled {
        TilingMode::Tiled
    } else {
        TilingMode::Floating
    }
}

/// Occupancy by native workspace id from homed live rows plus explicit
/// minimized lifetimes. Sticky all-desktop rows occupy only their homed
/// domain, never every backing workspace. Minimized entries occupy their
/// homed workspace even though they are absent from domain membership
/// (floats and sticky exceptions ride domain membership above).
pub(crate) fn homed_occupancy(report: &WorldReport) -> BTreeSet<String> {
    let mut occupied = BTreeSet::new();
    for domain in &report.domains {
        if domain.readable && !domain.windows.is_empty() {
            occupied.insert(domain.domain.workspace.0.clone());
        }
    }
    for entry in &report.minimized {
        occupied.insert(entry.workspace.0.clone());
    }
    occupied
}

/// Whether a retained migration is confirmed: the target scope lists the
/// workspace while the source scope no longer does.
pub(crate) fn migration_confirmed(
    report: &WorldReport,
    source_output: &OutputId,
    target: &TransferTarget,
) -> bool {
    let lists = |output: &OutputId| {
        report
            .topology
            .iter()
            .find(|topology| &topology.output == output)
            .is_some_and(|topology| {
                topology
                    .native_order
                    .iter()
                    .any(|id| id == &target.target_workspace)
            })
    };
    lists(&target.target_output) && !lists(source_output)
}

pub(crate) fn observed_closed(report: &WorldReport, window: &WindowId) -> bool {
    report
        .domains
        .iter()
        .any(|domain| domain.closed.iter().any(|id| id == window))
}

pub(crate) fn current_view<'a>(report: &'a WorldReport, output: &OutputId) -> Option<&'a str> {
    report
        .topology
        .iter()
        .find(|topology| &topology.output == output)
        .map(|topology| topology.current_native.as_str())
}
