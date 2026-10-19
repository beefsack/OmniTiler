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
use crate::workspace::{MIN_WORKSPACES, plan_trailing};
use crate::workspace_scope::{
    LiveView, ScopedTopology, WorkspaceMode, aligned_removal_set, aligned_ring_ids,
    aligned_trailing_index, claim_visible_currents, rebuild_global_mapping, rebuild_local_mapping,
    rebuild_shared_order, scoped_ring_ids, scoped_trailing_facts,
};
use crate::world_engine::{RetainedTransfer, WorldEngine};
use crate::world_report::{
    CommandResult, FloatOrigin, HoldReason, OneShotEffect, OutputDesire, ScopeAuthority,
    ScopeStatus, TilingMode, TransferKind, TransferTarget, WORLD_CONTRACT_VERSION, WindowDesire,
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
            } else if matches!(&retained.target.kind, TransferKind::Migration) {
                // Whole-workspace members ride the target homing (one homing)
                // while the flight is outstanding, so delayed/partial
                // observations and sessionless floating carries never vanish:
                // the Engine target session (or the membership-only row when
                // no sweep row exists yet) carries them. Closed and
                // externally-moved members already dropped from `members`,
                // so they never redirect/fight. Minimized members skip the
                // trailing lane below and ride the minimized homing instead.
                // The target native is live, so verification rides the live
                // inventory like policy placements (the homing may not be
                // reported yet).
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
                if !readable && !live_ws.contains(key.workspace.0.as_str()) {
                    continue;
                }
                for member in &retained.members {
                    redirect
                        .entry(member.0.clone())
                        .or_insert((key.clone(), false));
                }
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
                            floating: false,
                            float_origin: None,
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
            // Legacy parity: project the outer-inset work area, never the
            // raw client area. Inset exhaustion here mirrors the sweep
            // hold (the sweep already held, so this is only a backstop).
            let Some(inset) =
                crate::world_engine::inset_world_domain(&domain.domain, domain.outer_gap)
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
            let plan = project_retained_tiled_geometry(
                session,
                &key,
                inset.bounds,
                inset.gap,
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
                // Explicit ordinary-float classification (D4/D5): every
                // retained exception row in a controlled scope converges
                // adapter float intent. The diagnostic capacity origin
                // rides along for truthful echo; it grants no behavior.
                let float_origin = session
                    .is_capacity_float(&observed.window)
                    .then_some(FloatOrigin::Capacity);
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
                let member_hold = window_holds.get(&window.0).copied();
                // A minimized lifetime under an outstanding membership
                // transfer rides the target homing (one homing), keeping
                // minimized frame authority: the adapter actuates only the
                // desktop set, never geometry or unminimize.
                let (home_output, home_workspace) = redirect
                    .get(&window.0)
                    .map(|(target, _)| (target.output.clone(), target.workspace.0.clone()))
                    .unwrap_or((output.clone(), workspace.0.clone()));
                windows.push(WindowDesire {
                    lifetime: window,
                    output: home_output,
                    workspace: home_workspace,
                    allocation: None,
                    effective_frame: None,
                    frame_authority: ScopeAuthority::Hold(HoldReason::Minimized),
                    membership_authority: match member_hold {
                        Some(reason) => ScopeAuthority::Hold(reason),
                        None => ScopeAuthority::Controlled,
                    },
                    frame_operation: None,
                    floating: false,
                    float_origin: None,
                });
            }
        }
        // Redirected movers without a sweep row yet: one membership-only
        // row at the transfer target homing (frame untouched, never
        // allocation or geometry). Ordinary engine-committed sends already
        // carry sweep rows, so the emitted guard keeps exactly one homing.
        // Minimized lifetimes never ride this lane: the minimized loops
        // above and below emit them with minimized frame authority.
        let minimized_ids: BTreeSet<&str> = report
            .minimized
            .iter()
            .map(|entry| entry.window.0.as_str())
            .collect();
        for (window_id, (key, _)) in &redirect {
            if minimized_ids.contains(window_id.as_str()) {
                continue;
            }
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
                floating: false,
                float_origin: None,
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
            // A minimized lifetime under an outstanding membership
            // transfer rides the target homing (one homing), keeping
            // minimized frame authority. The scope-hold lookup follows
            // the resolved homing.
            let (home_output, home_workspace) = redirect
                .get(&entry.window.0)
                .map(|(target, _)| (target.output.clone(), target.workspace.clone()))
                .unwrap_or((entry.output.clone(), entry.workspace.clone()));
            // A minimized lifetime homed on a held scope carries that
            // scope's hold on membership: unreadable scopes use the report
            // hold, sweep-held scopes (including geometric exhaustion) the
            // classified reason. Omission inside a held scope authorizes no
            // write or retirement.
            let scope_hold = report
                .domains
                .iter()
                .find(|domain| {
                    domain.domain.id == home_output && domain.domain.workspace == home_workspace
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
                output: home_output,
                workspace: home_workspace.0.clone(),
                allocation: None,
                effective_frame: None,
                frame_authority: ScopeAuthority::Hold(HoldReason::Minimized),
                membership_authority: match member_hold.or(scope_hold) {
                    Some(reason) => ScopeAuthority::Hold(reason),
                    None => ScopeAuthority::Controlled,
                },
                frame_operation: None,
                floating: false,
                float_origin: None,
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
                floating: false,
                float_origin: None,
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
                floating: false,
                float_origin: None,
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
                floating: false,
                float_origin: None,
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
                floating: false,
                float_origin: None,
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
    /// order for shared mode); sticky all-desktop rows never occupy any
    /// backing cell for spare purposes, and their homed natives are
    /// retained against removal while the world-apply removal gate
    /// (occupant/visible/pin/min-count re-enumeration) may still hold
    /// them, so core never desires a removal that never executes.
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
        if self.is_aligned(report) {
            return self.aligned_workspace_rows(report);
        }
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
            // Sticky-homed cells are retained here too: empty for spare
            // purposes, but the world-apply occupant gate may still hold
            // their homed cell, so core retains instead of desiring a
            // removal that never executes (preserves the
            // pre-occupancy-exclusion keep behavior).
            retained_ids.extend(sticky_homed_natives_for_output(report, &topology.output));
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
            // another output: one ring per native, and visible-current
            // evidence never creates ring ownership; the output row
            // carries the view). The output's own current view keeps its
            // row only when the current is unassigned, so visibility
            // never loses its mode/lifecycle row while staying single-homed.
            // While complete mapping evidence is unavailable the fallback is
            // omitted entirely: unowned natives get no provisional rows
            // under any output (no invented ownership, no generic echo
            // under multiple outputs). Retained ring rows above still
            // publish, held, single-homed.
            if report_complete_for_mapping(report) {
                for native in &topology.native_order {
                    if !ring.iter().any(|id| id == native) && !assigned.contains(native.as_str()) {
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
            }
            // Startup conversion owns this output's pending token: the fresh
            // backing rides exactly one unbound row until its binding proves
            // the native id. Never cancelled as a spare and never minted
            // here (converge owns minting); the logical token is never a
            // native id.
            let converting = report.workspace_mode == WorkspaceMode::PerOutputLocal
                && self.startup_conversion.contains_key(&topology.output);
            if converting {
                if readable_complete
                    && let Some(pending) = self
                        .pending_creations
                        .iter()
                        .find(|pending| pending.output == topology.output)
                {
                    let logical = pending.logical_id.clone();
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
            } else if plan.need_append && readable_complete {
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

    /// Aligned per-output-local workspace rows: every output owns the full
    /// live native ring in native position with per-(output, native)
    /// logical identity. Each output's trailing empty is the first empty
    /// cell after its own last occupied cell; removal eligibility is global
    /// (empty on every output, no output's trailing, not shown anywhere,
    /// not pinned/displaced/unmanaged/in-flight, global floor of two
    /// including trailing). Incomplete reports hold every row with no
    /// removal or creation. Startup adopts the grid: no conversion rows,
    /// no member moves here. Logical ids are core-minted `al-<n>` per
    /// (output, native), stable across rebinding; reported bindings are
    /// read only for protection flags, never for identity (replicated
    /// aligned bindings must not collapse through a generic
    /// native->logical map). Whole-workspace migration rebinds the moved
    /// logical onto the destination trailing native (see
    /// `finish_aligned_migration`); the emptied source cell mints fresh
    /// here and rows keep both identities stable while the flight is
    /// outstanding.
    pub(crate) fn aligned_workspace_rows(&mut self, report: &WorldReport) -> Vec<WorkspaceDesire> {
        let live_order = live_native_order(report);
        let global_complete = report_complete_for_mapping(report);
        // Per-output occupancy plus the global union.
        let mut occupied_by_output: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for topology in &report.topology {
            occupied_by_output.insert(
                topology.output.0.clone(),
                homed_occupancy_for_output(report, &topology.output),
            );
        }
        let occupied_any = homed_occupancy(report);
        let visible_any: BTreeSet<String> = report
            .topology
            .iter()
            .map(|topology| topology.current_native.clone())
            .collect();
        // Global retained set: pinned/displaced/unmanaged bindings anywhere,
        // core-displaced units anywhere, outstanding flight targets, plus
        // sticky-homed natives anywhere. Sticky anchors never occupy for
        // spare purposes; retention keeps core from desiring removal of a
        // homed cell while the world-apply occupant gate
        // (world-apply.ts: occupant/visible/pin/min-count re-enumeration)
        // may still hold it. Ordinary close/unhome releases the hold and
        // normal lifecycle resumes.
        let mut retained_any: BTreeSet<String> = BTreeSet::new();
        for binding in &report.bindings {
            if binding.pinned || binding.displaced || !binding.managed {
                retained_any.insert(binding.native_id.clone());
            }
        }
        for entry in self.displaced.values() {
            retained_any.extend(entry.workspaces.iter().cloned());
        }
        for retained in &self.retained {
            retained_any.insert(retained.target.target_workspace.clone());
            // An outstanding whole-workspace migration keeps its source
            // native too: the emptied cell's fresh replacement must not be
            // removed from under the flight before arrival.
            if matches!(retained.target.kind, TransferKind::Migration) {
                retained_any.insert(retained.source_workspace.clone());
            }
        }
        retained_any.extend(sticky_homed_natives(report));
        // Per-output trailing natives first (unioned for the global pass).
        let mut trailing_any: BTreeSet<String> = BTreeSet::new();
        let mut trailing_by_output: BTreeMap<String, Option<String>> = BTreeMap::new();
        for topology in &report.topology {
            let ring = aligned_ring_ids(&live_order);
            let occupied = occupied_by_output
                .get(&topology.output.0)
                .cloned()
                .unwrap_or_default();
            let trailing = aligned_trailing_index(&ring, &occupied).map(|i| ring[i].clone());
            if let Some(native) = trailing.clone() {
                trailing_any.insert(native);
            }
            trailing_by_output.insert(topology.output.0.clone(), trailing);
        }
        let removal: BTreeSet<String> = if global_complete {
            aligned_removal_set(
                &live_order,
                &occupied_any,
                &trailing_any,
                &visible_any,
                &retained_any,
            )
        } else {
            BTreeSet::new()
        };
        // Mint stable per-(output, native) logical ids for every live cell.
        for topology in &report.topology {
            for native in &live_order {
                let key = (topology.output.0.clone(), native.clone());
                if !self.aligned_logical.contains_key(&key) {
                    if self.creation_counter == u64::MAX {
                        continue;
                    }
                    let next = self.creation_counter.saturating_add(1);
                    self.creation_counter = next;
                    self.aligned_logical.insert(key, format!("al-{next}"));
                }
            }
        }
        // Prune logicals for gone outputs/natives (hotplug return placement
        // is a follow-up step; this unit keeps only live cells).
        {
            let live_outputs: BTreeSet<&str> = report
                .topology
                .iter()
                .map(|topology| topology.output.0.as_str())
                .collect();
            let live_natives: BTreeSet<&str> = live_order.iter().map(String::as_str).collect();
            self.aligned_logical.retain(|(output, native), _| {
                live_outputs.contains(output.as_str()) && live_natives.contains(native.as_str())
            });
        }
        let mut rows = Vec::new();
        // Creation is global (one native desktop is empty on every
        // output at once), so at most one output mints per report: the
        // global floor forces a need even where a trailing empty
        // exists, and outputs already awaiting their own trailing
        // binding keep precedence so their intent never stalls.
        // Incomplete reports hold with no creation; exact per-output
        // token attribution stays.
        let below_floor = live_order.len() < MIN_WORKSPACES;
        let owner: Option<String> = {
            let needy = |output: &str| -> bool {
                let readable = global_complete
                    && report.topology.iter().any(|t| {
                        t.output.0 == output
                            && t.complete
                            && report
                                .domains
                                .iter()
                                .filter(|d| d.domain.id == t.output)
                                .all(|d| d.readable)
                    });
                readable
                    && (below_floor
                        || trailing_by_output
                            .get(output)
                            .is_none_or(|trailing| trailing.is_none()))
            };
            let intent_on = |output: &str| {
                self.pending_migrations
                    .iter()
                    .any(|p| p.target_output.0 == output)
                    || self.pending_sends.iter().any(|p| p.output.0 == output)
            };
            report
                .topology
                .iter()
                .map(|t| t.output.0.clone())
                .find(|output| needy(output) && intent_on(output))
                .or_else(|| {
                    report
                        .topology
                        .iter()
                        .map(|t| t.output.0.clone())
                        .find(|output| needy(output))
                })
        };
        for topology in &report.topology {
            let readable_complete = global_complete
                && topology.complete
                && report
                    .domains
                    .iter()
                    .filter(|d| d.domain.id == topology.output)
                    .all(|d| d.readable);
            let held = !readable_complete;
            for (index, native) in live_order.iter().enumerate() {
                let key = (topology.output.0.clone(), native.clone());
                let Some(logical) = self.aligned_logical.get(&key).cloned() else {
                    continue;
                };
                rows.push(WorkspaceDesire {
                    id: logical.clone(),
                    output: topology.output.clone(),
                    native_binding: Some(native.clone()),
                    logical_id: Some(logical),
                    scope: workspace_scope_key(report.workspace_mode, &topology.output),
                    order: index,
                    tiling_mode: self.workspace_tiling_mode(&topology.output, native),
                    existence: held || !removal.contains(native),
                    hold: held.then_some(HoldReason::IncompleteInventory),
                });
            }
            // Per-output trailing need plus the global floor: no empty
            // cell after the last occupied one, or fewer live natives
            // than the minimum, means this output wants one creation.
            // Only the elected owner mints and rides the unbound row.
            let needs_trailing = trailing_by_output
                .get(&topology.output.0)
                .is_none_or(|trailing| trailing.is_none());
            let needy = needs_trailing || below_floor;
            let is_owner = owner.as_deref() == Some(topology.output.0.as_str());
            if needy && readable_complete && is_owner {
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
            } else if readable_complete && !needy {
                // A non-owner that still wants a creation mints nothing
                // this report (one global creation suffices); no prune
                // either, so an outstanding intent token is never erased.
                // Only an output with no need at all drops its tokens.
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
        let mut retained_ids: BTreeSet<String> = report
            .bindings
            .iter()
            .filter(|binding| binding.pinned || binding.displaced)
            .map(|binding| binding.native_id.clone())
            .collect();
        // Sticky-homed cells are retained here too (see the disjoint note
        // above): empty for spare purposes, but the world-apply occupant
        // gate may still hold their homed cell.
        retained_ids.extend(sticky_homed_natives(report));
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
        if self.is_aligned(report) {
            // Aligned adoption: every output owns the full native-position
            // grid. No partitioning, no binding seeds, no conversion moves.
            // Fail-closed like the disjoint path: partial evidence freezes
            // the retained mapping.
            if report_complete_for_mapping(report) {
                for output in &outputs {
                    self.mapping.insert(output.clone(), live_order.clone());
                }
                let live_outputs: BTreeSet<&str> = outputs.iter().map(String::as_str).collect();
                self.mapping
                    .retain(|output, _| live_outputs.contains(output.as_str()));
                self.shared_order = rebuild_shared_order(&live_order);
                self.startup_conversion.clear();
                self.startup_pending = false;
            }
            // Hotplug displacement/return for aligned mode is a follow-up
            // step: rings and rows read the adopted grid immediately.
            // Verified view observations over the full rings below. While a
            // migration flight is outstanding, a stale source view still
            // naming the vacated native must not re-prime or re-record
            // history against the fresh replacement: skip observation for
            // that output while its current equals the vacated native.
            // Unrelated source views observe normally; the target arrival
            // observes normally.
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
            let vacated: Vec<(String, String)> = self
                .retained
                .iter()
                .filter(|r| matches!(&r.target.kind, TransferKind::Migration))
                .map(|r| (r.source_output.0.clone(), r.source_workspace.clone()))
                .collect();
            for (output, view) in &live_views {
                if vacated
                    .iter()
                    .any(|(o, n)| o == output && n == &view.current)
                {
                    continue;
                }
                self.history.observe(output, &view.current, &view.scope);
            }
            let live_keys: BTreeSet<&str> = live_views.keys().map(String::as_str).collect();
            for key in self
                .history
                .history_outputs()
                .into_iter()
                .filter(|key| !live_keys.contains(key.as_str()))
            {
                self.history.discard_output(&key);
            }
            return;
        }
        // Wired active-screen primary, else deterministic first order
        // (mirrors the hotplug nearest-survivor primary in
        // converge_displacement).
        let primary = report
            .topology
            .iter()
            .find(|topology| topology.primary)
            .map(|topology| topology.output.0.clone())
            .or_else(|| outputs.first().cloned());
        // Fail-closed mapping ownership: partial enumeration or an unreadable
        // domain never moves ownership. The retained mapping, displacement
        // records, and conversion entries all read the frozen mapping
        // meanwhile; history below still observes (existing behavior).
        // Post-startup explicit ownership therefore never regresses on a
        // partial report, and the startup pass runs only on the first
        // complete report.
        if report_complete_for_mapping(report) {
            // Claim unassigned shown currents first (primary first): a
            // distinct pre-existing current belongs to its output; an
            // already-owned shared current stays single-homed for the
            // conversion pass below. Only unassigned ids move, so retained
            // ownership, migration, and displacement stay authoritative.
            // Observed views outrank every binding seed below.
            let mut assignments = self.mapping.clone();
            if report.workspace_mode != WorkspaceMode::Shared {
                let mut currents: Vec<(String, String)> = Vec::new();
                if let Some(primary) = primary.clone() {
                    for topology in &report.topology {
                        if topology.output.0 == primary {
                            currents
                                .push((topology.output.0.clone(), topology.current_native.clone()));
                        }
                    }
                }
                for topology in &report.topology {
                    if primary.as_deref() != Some(topology.output.0.as_str()) {
                        currents.push((topology.output.0.clone(), topology.current_native.clone()));
                    }
                }
                let (claimed, _) = claim_visible_currents(&assignments, &currents, &live_order);
                assignments = claimed;
            }
            // Seed ownership from reported bindings in report order.
            // Retained mapping always wins: a seed never moves an id that is
            // already homed elsewhere, so stale or ambiguous evidence is
            // ignored deterministically instead of stealing. Two narrow
            // exceptions place unhomed ids: exact outstanding pending-token
            // proof (logical plus output match a pending creation) rehomes
            // the bound native unless it is shown anywhere, and a genuine
            // logical id (never the generic native->native fallback echo)
            // fills a still-unassigned native. The fallback echo carries no
            // ownership: at fresh startup the adapter reports no bindings at
            // all, and legacy snapshots homing everything at the primary
            // must not override the observed distinct currents above.
            if !report.bindings.is_empty() {
                let live_set: BTreeSet<&str> = live_order.iter().map(String::as_str).collect();
                for binding in &report.bindings {
                    if !live_set.contains(binding.native_id.as_str()) {
                        continue;
                    }
                    let pending_proof = self.pending_creations.iter().any(|pending| {
                        pending.output == binding.output && pending.logical_id == binding.logical_id
                    });
                    let homed = assignments
                        .values()
                        .any(|ids| ids.iter().any(|id| id == &binding.native_id));
                    if pending_proof {
                        // Never rehome a shown workspace: the fresh backing
                        // proves a newly adopted native, not a viewed one.
                        let shown = report
                            .topology
                            .iter()
                            .any(|topology| topology.current_native == binding.native_id);
                        if shown {
                            continue;
                        }
                        for (other, list) in assignments.iter_mut() {
                            if *other != binding.output.0 {
                                list.retain(|id| id != &binding.native_id);
                            }
                        }
                        let entry = assignments.entry(binding.output.0.clone()).or_default();
                        if !entry.iter().any(|kept| kept == &binding.native_id) {
                            entry.push(binding.native_id.clone());
                        }
                    } else if !homed && binding.logical_id != binding.native_id {
                        assignments
                            .entry(binding.output.0.clone())
                            .or_default()
                            .push(binding.native_id.clone());
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
        }
        // Hotplug displacement/return moves mapping entries between
        // scopes before history observes this report's views, so rings
        // and rows see displaced units under their survivor immediately.
        self.converge_displacement(report);
        // Startup conversion mints fresh backings and, once their bindings
        // prove the native ids, converts members and views.
        self.converge_startup_conversion(report);
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

    /// Per-output-local startup conversion (D2): outputs still showing a
    /// foreign-owned shared current receive their own fresh backing plus
    /// membership-only conversion of their non-sticky windows, without
    /// changing output and preserving float/fullscreen/maximize. Distinct
    /// currents already belong to their outputs via the claim pass above.
    /// Fail-closed like displacement: partial enumeration never mints
    /// backings or moves members. One ring per native throughout: the
    /// shared current is never duplicated, even while conversion pends.
    /// Post-startup explicit views are never converted: fresh backings
    /// mint only while [`WorldEngine::startup_pending`] holds.
    pub(crate) fn converge_startup_conversion(&mut self, report: &WorldReport) {
        if report.workspace_mode != WorkspaceMode::PerOutputLocal {
            return;
        }
        // Aligned mode adopts the grid without conversion or moves; the
        // disjoint conversion below stays for the default model only.
        if self.aligned_per_output {
            self.startup_conversion.clear();
            if report_complete_for_mapping(report) {
                self.startup_pending = false;
            }
            return;
        }
        // Same fail-closed bar as mapping ownership above: partial
        // enumeration or an unreadable domain never mints backings, moves
        // members, or consumes the startup pass.
        if !report_complete_for_mapping(report) {
            return;
        }
        let live_order = live_native_order(report);
        let live: BTreeSet<&str> = live_order.iter().map(String::as_str).collect();
        // Entries whose view moved on (explicit input wins) or whose output
        // is gone clear without actuation.
        let mut gone: Vec<OutputId> = Vec::new();
        for (output, shared) in &self.startup_conversion {
            if current_view(report, output) != Some(shared.as_str()) {
                gone.push(output.clone());
            }
        }
        for output in gone {
            self.startup_conversion.remove(&output);
        }
        let epoch = self.state_revision;
        let mut fresh_views: Vec<(OutputId, String)> = Vec::new();
        let mut fresh_members: Vec<(WindowId, DomainKey, DomainKey)> = Vec::new();
        for topology in &report.topology {
            let output = &topology.output;
            let current = topology.current_native.clone();
            // Owned or unknown currents need nothing: only a live current
            // homed in another scope converts.
            let owned = self
                .mapping
                .get(&output.0)
                .is_some_and(|ids| ids.iter().any(|id| id == &current));
            if owned || !live.contains(current.as_str()) {
                continue;
            }
            let foreign = self
                .mapping
                .values()
                .any(|ids| ids.iter().any(|id| id == &current));
            if !foreign {
                continue;
            }
            if !self.startup_conversion.contains_key(output) {
                if !self.startup_pending {
                    continue;
                }
                // Fresh backing, minted once: the pending logical token
                // rides the workspace rows until its binding proves the
                // native id. The token is never a native id, and nothing
                // moves before the binding lands.
                if self.pending_token(output).is_none() {
                    continue;
                }
                self.startup_conversion
                    .insert(output.clone(), current.clone());
            }
            let Some(shared) = self.startup_conversion.get(output).cloned() else {
                continue;
            };
            if shared != current {
                continue;
            }
            // Fresh binding event: the pending token resolved to a live
            // native under this output. Convert the shared native's
            // non-sticky members within the same output and follow the view.
            let Some(native) = self
                .pending_creations
                .iter()
                .find(|pending| pending.output == *output)
                .and_then(|pending| {
                    report
                        .bindings
                        .iter()
                        .find(|binding| {
                            binding.output == *output && binding.logical_id == pending.logical_id
                        })
                        .map(|binding| binding.native_id.clone())
                })
            else {
                continue;
            };
            if !live.contains(native.as_str()) || native == shared {
                self.startup_conversion.remove(output);
                continue;
            }
            for domain in &report.domains {
                if domain.domain.id != *output || domain.domain.workspace.0 != shared {
                    continue;
                }
                for window in &domain.windows {
                    // Sticky windows belong everywhere already and never
                    // move; outstanding transfers and float placements never
                    // duplicate.
                    if window.sticky {
                        continue;
                    }
                    if self.retained.iter().any(|retained| {
                        matches!(&retained.target.kind, TransferKind::Window(w) if w == &window.window)
                    }) {
                        continue;
                    }
                    if self
                        .retained_floats
                        .iter()
                        .any(|held| held.window == window.window)
                    {
                        continue;
                    }
                    let source = DomainKey {
                        output: output.clone(),
                        workspace: WorkspaceId(shared.clone()),
                    };
                    let dest = DomainKey {
                        output: output.clone(),
                        workspace: WorkspaceId(native.clone()),
                    };
                    fresh_members.push((window.window.clone(), dest, source));
                }
            }
            // Non-sticky minimized lifetimes homed on the shared native
            // convert with the rest: membership-only, same output, never
            // unminimized by the core (the row keeps minimized frame
            // authority at the target; the adapter actuates only the
            // desktop set). Sticky minimized lifetimes are exempt.
            for entry in &report.minimized {
                if entry.output != *output || entry.workspace.0 != shared || entry.sticky {
                    continue;
                }
                if self.retained.iter().any(|retained| {
                    matches!(&retained.target.kind, TransferKind::Window(w) if w == &entry.window)
                }) {
                    continue;
                }
                let source = DomainKey {
                    output: output.clone(),
                    workspace: WorkspaceId(shared.clone()),
                };
                let dest = DomainKey {
                    output: output.clone(),
                    workspace: WorkspaceId(native.clone()),
                };
                fresh_members.push((entry.window.clone(), dest, source));
            }
            fresh_views.push((output.clone(), native));
            self.startup_conversion.remove(output);
        }
        // Policy member transfers: one stable membership-only intent per
        // affected member (the adapter moves them natively under the same
        // output; observation proof settles through the shared path).
        for (index, (window, target, source)) in fresh_members.into_iter().enumerate() {
            let operation_id = format!("op-scm-{epoch}-{index}");
            self.retained
                .retain(|retained| !matches!(&retained.target.kind, TransferKind::Window(w) if w == &window));
            self.retained.push(RetainedTransfer {
                target: TransferTarget {
                    operation_id,
                    kind: TransferKind::Window(window),
                    target_output: target.output,
                    target_workspace: target.workspace.0,
                    follow: false,
                    held: false,
                },
                policy: true,
                source_output: source.output,
                source_workspace: source.workspace.0,
                members: Vec::new(),
                migration_refill: None,
                migration_initial_empty: false,
            });
        }
        for (index, (output, workspace)) in fresh_views.into_iter().enumerate() {
            self.retain_policy_view(&output, &workspace, format!("op-scv-{epoch}-{index}"));
        }
        self.startup_pending = false;
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
                // Aligned mode projects identity per (output, native) from
                // core-retained logicals, never the replicated bindings.
                let logical_view = if self.is_aligned(report) {
                    self.aligned_logical
                        .get(&(topology.output.0.clone(), native_view.to_owned()))
                        .cloned()
                } else {
                    report
                        .bindings
                        .iter()
                        .find(|binding| {
                            binding.output == topology.output && binding.native_id == native_view
                        })
                        .map(|binding| binding.logical_id.clone())
                };
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
    // Minimized lifetimes home explicitly outside domain membership;
    // a minimized window observed at a homing confirms and settles the
    // same retained transfers as a domain member there.
    for entry in &report.minimized {
        if entry.window == *window {
            return Some(DomainKey {
                output: entry.output.clone(),
                workspace: entry.workspace.clone(),
            });
        }
    }
    None
}

/// Complete-enough report for mapping ownership (startup partition,
/// binding seeds, adoption, conversion): full inventory, complete
/// topology on every output, and every reported domain readable. Partial
/// evidence freezes ownership instead of moving it.
pub(crate) fn report_complete_for_mapping(report: &WorldReport) -> bool {
    report.inventory_complete
        && report.topology.iter().all(|topology| topology.complete)
        && report.domains.iter().all(|domain| domain.readable)
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
/// minimized lifetimes. Sticky all-desktop rows never occupy any backing
/// cell: showing an empty cell (including the trailing empty) with only a
/// sticky anchor keeps the spare, so selecting/showing empties creates
/// nothing and the ring end is retained until a real arrival/admission.
/// Minimized entries occupy their homed workspace even though they are
/// absent from domain membership (floats and sticky exceptions ride domain
/// membership above, sticky excluded here too).
pub(crate) fn homed_occupancy(report: &WorldReport) -> BTreeSet<String> {
    let mut occupied = BTreeSet::new();
    for domain in &report.domains {
        if domain.readable && domain.windows.iter().any(|window| !window.sticky) {
            occupied.insert(domain.domain.workspace.0.clone());
        }
    }
    for entry in &report.minimized {
        if !entry.sticky {
            occupied.insert(entry.workspace.0.clone());
        }
    }
    occupied
}

/// Occupancy for one output in the aligned model: natives with a homed
/// non-sticky live row on `output` plus non-sticky minimized lifetimes
/// homed there. Sticky rows never occupy any backing cell, only real
/// arrivals/admissions extend the spare.
pub(crate) fn homed_occupancy_for_output(
    report: &WorldReport,
    output: &OutputId,
) -> BTreeSet<String> {
    let mut occupied = BTreeSet::new();
    for domain in &report.domains {
        if domain.readable
            && domain.domain.id == *output
            && domain.windows.iter().any(|window| !window.sticky)
        {
            occupied.insert(domain.domain.workspace.0.clone());
        }
    }
    for entry in &report.minimized {
        if entry.output == *output && !entry.sticky {
            occupied.insert(entry.workspace.0.clone());
        }
    }
    occupied
}

/// Natives homing a sticky live row (readable) plus sticky minimized
/// lifetimes: empty for spare purposes (see [`homed_occupancy`]), yet retained
/// against removal. World apply also protects referenced window rows, even
/// when an all-desktop native has no explicit desktop memberships. Ordinary
/// close/unhome releases the hold and normal lifecycle resumes.
pub(crate) fn sticky_homed_natives(report: &WorldReport) -> BTreeSet<String> {
    let mut retained = BTreeSet::new();
    for domain in &report.domains {
        if domain.readable && domain.windows.iter().any(|window| window.sticky) {
            retained.insert(domain.domain.workspace.0.clone());
        }
    }
    for entry in &report.minimized {
        if entry.sticky {
            retained.insert(entry.workspace.0.clone());
        }
    }
    retained
}

/// Output-scoped sticky-homed natives for one output (see
/// [`sticky_homed_natives`]).
pub(crate) fn sticky_homed_natives_for_output(
    report: &WorldReport,
    output: &OutputId,
) -> BTreeSet<String> {
    let mut retained = BTreeSet::new();
    for domain in &report.domains {
        if domain.readable
            && domain.domain.id == *output
            && domain.windows.iter().any(|window| window.sticky)
        {
            retained.insert(domain.domain.workspace.0.clone());
        }
    }
    for entry in &report.minimized {
        if entry.output == *output && entry.sticky {
            retained.insert(entry.workspace.0.clone());
        }
    }
    retained
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
