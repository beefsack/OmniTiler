use super::*;

/// Stage the native view publication for one migration (D4/G37): hide the
/// occupied target prior (ledger-authorized, stays listed and hidden) and
/// reveal the source refill plus migrated product-hidden members as the
/// active views permit (preserving minimized/hidden intent, never
/// unminimizing). `candidate_ws` already holds the project reassignment
/// (moved shown on target, refill shown on source); this applies the native
/// hide/show effects fenced per effect and mirrors the hidden flags onto the
/// candidate so publication carries both physical views. Under test the
/// ledger setters are stubbed to project flag moves (native-free); production
/// routes through [`workspace_hide_one`] and `reveal_product_claim_to` with
/// the recovery ledger (never bypassed). Any effect failure reconciles
/// without replay; the caller publishes only after both views verify.
/// Returns `None` when all effects hold, else the refusal outcome.
#[allow(clippy::too_many_arguments)]
fn migration_view_effects(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    candidate_ws: &mut crate::workspace::ManagedWorkspaces,
    source_output: &str,
    refill: Option<&str>,
    target_output: &str,
    target_prior: &str,
    migrating: &BTreeSet<crate::workspace::WindowKey>,
    snapshot: &crate::workspace_owner::MigrateFenceSnapshot,
    tail_areas: &[MonitorArea],
) -> Option<&'static str> {
    // Per-effect fence gate shared by every hide/reveal below: real scope,
    // lifetime, membership plus fresh topology on EACH call (never stubbed,
    // never fixed areas). The project reassignment has not published yet, so
    // membership still homes every migrating member on the source.
    let fences_hold =
        |state: &TileLoop, source_workspace: &str, target_prior: &str| -> Option<&'static str> {
            let scope_ok = migrating.iter().all(|key| {
                state.member_identity.get(key).is_some_and(|stored| {
                    scope_allows(&state.scope, &stored.exe_path)
                        && hosted_gate_allows(
                            &stored.exe_path,
                            key.hwnd,
                            stored.pid,
                            &state.scope_hosts,
                        )
                })
            });
            let lifetime_ok = migrating.iter().all(|key| {
                state.member_tags.get(key).is_some_and(|stored| {
                    crate::workspace_owner::visible_lifetime_ok(
                        stored,
                        live_member_tag(key.hwnd).as_deref(),
                    )
                })
            });
            let membership_ok = migrating.iter().all(|key| {
                state.workspaces.member_loc(key).is_some_and(|fresh| {
                    fresh.output == source_output && fresh.workspace == source_workspace
                })
            });
            let topology_ok = fresh_cross_areas().is_some_and(|fresh| {
                cross_topology_fresh(
                    &crate::workspace_owner::migrate_snapshot_for_drag(snapshot),
                    &fresh,
                )
            });
            let live = crate::workspace_owner::MigrateFenceLive {
                inner_gap: state.inner_gap,
                outer_gap: state.outer_gap,
                source_active: state.workspaces.active_id(source_output),
                target_active: state.workspaces.active_id(target_output),
                source_tiled: state.workspaces.is_tiled(source_output, source_workspace),
                target_tiled: state.workspaces.is_tiled(target_output, target_prior),
                topology_ok,
                scope_ok,
                lifetime_ok,
                membership_ok,
                views_ok: state.workspaces.active_id(source_output).as_deref()
                    == Some(source_workspace)
                    && state.workspaces.active_id(target_output).as_deref() == Some(target_prior),
                owner_ok: state
                    .engine
                    .owner()
                    .is_some_and(|owner| owner.as_str() == snapshot.owner.as_str())
                    && state.engine.generation().is_some_and(|generation| {
                        generation.as_str() == snapshot.generation.as_str()
                    }),
            };
            crate::workspace_owner::migrate_fences_hold(snapshot, &live)
        };
    // Pinned source workspace for the pre-commit view fences (the project
    // reassignment has not published yet, so the source still shows it).
    let source_workspace = snapshot.moved_workspace.as_str();
    let mut newly_hidden: Vec<crate::workspace::WindowKey> = Vec::new();
    // Hide the occupied target prior: visible members take a ledger hide and
    // stay listed but hidden (D4). Already-hidden and minimized members keep
    // their intent with no setter. Sticky never applies to the prior.
    let prior_members: Vec<crate::workspace::WindowKey> = state
        .workspaces
        .workspace_members(target_output, target_prior)
        .into_iter()
        .collect();
    for key in &prior_members {
        if state.workspaces.is_hidden(key) {
            continue;
        }
        if live_is_iconic(key.hwnd) || state.minimized_omitted.contains(key) {
            continue;
        }
        if let Some(outcome) = fences_hold(state, source_workspace, target_prior) {
            return Some(outcome);
        }
        let Some(expected) = state.member_identity.get(key).cloned() else {
            continue;
        };
        #[cfg(test)]
        {
            let _ = (me, store, dir, expected);
            state.workspaces.set_hidden(key, true);
            candidate_ws.set_hidden(key, true);
            newly_hidden.push(key.clone());
        }
        #[cfg(not(test))]
        {
            let iconic = false;
            let (outcome, _) = workspace_hide_one(state, me, store, dir, key, &expected, iconic);
            if outcome == "hidden" || state.hidden_claims.contains_key(key) {
                state.workspaces.set_hidden(key, true);
                candidate_ws.set_hidden(key, true);
                newly_hidden.push(key.clone());
            } else if outcome == "origin-vanished" || outcome == "identity-changed" {
                drop_member_state(state, key);
            } else if outcome == "scope-excluded" {
                // Out-of-scope prior members stay visible with membership
                // intact (same rule as selects), never a failure.
            } else {
                return Some(outcome);
            }
        }
    }
    // Reveal helper: one hidden-claim member whose new active view permits
    // visibility. Preserves minimized intent via the stored iconic flag
    // (never unminimizes). Mirrors the flag onto the candidate.
    let reveal_one = |state: &mut TileLoop,
                      candidate_ws: &mut crate::workspace::ManagedWorkspaces,
                      key: &crate::workspace::WindowKey|
     -> std::result::Result<(), &'static str> {
        let Some(record) = state.hidden_claims.get(key).cloned() else {
            state.workspaces.set_hidden(key, false);
            candidate_ws.set_hidden(key, false);
            return Ok(());
        };
        if let Some(outcome) = fences_hold(state, source_workspace, target_prior) {
            return Err(outcome);
        }
        #[cfg(test)]
        {
            let _ = record;
            state.hidden_claims.remove(key);
            state.workspaces.set_hidden(key, false);
            candidate_ws.set_hidden(key, false);
            Ok(())
        }
        #[cfg(not(test))]
        {
            match crate::product_hide::sys::reveal_product_claim_to(
                store,
                me,
                &record.claim,
                record.iconic,
            ) {
                Ok(crate::product_hide::ProductTeardown::Retired) => {
                    state.hidden_claims.remove(key);
                    drop_member_state(state, key);
                    candidate_ws.remove_window(key);
                    Ok(())
                }
                Ok(_) => {
                    state.hidden_claims.remove(key);
                    state.workspaces.set_hidden(key, false);
                    candidate_ws.set_hidden(key, false);
                    Ok(())
                }
                Err(_) => Err("migrate-unverified"),
            }
        }
    };
    // Source refill shows (G37): reveal its hidden members now that the
    // candidate shows the refill.
    if let Some(refill) = refill {
        let refill_members: Vec<crate::workspace::WindowKey> = candidate_ws
            .workspace_members(source_output, refill)
            .into_iter()
            .collect();
        for key in &refill_members {
            // Minimized refill members stay minimized: the reveal path
            // preserves iconic, so they still converge (no skip).
            if reveal_one(state, candidate_ws, key).is_err() {
                return Some("migrate-unverified");
            }
        }
    }
    // Migrated members preserve their pre-existing hidden/minimized intent:
    // no reveal here (the move carries visibility as-is; an active view does
    // not unhide intentional concealment nor unminimize). Membership was
    // fenced per effect above.
    let _ = tail_areas;
    None
}

/// Single causal follow tail for committed workspace migrations (D6): follow
/// the moved active client only. Empty and sticky-active migrations (no
/// active window) take no setter. A carried fullscreen (live re-read
/// immediately before focus, never the stale pre-plan row) or a minimized
/// client takes no focus setter and only records foreground. Otherwise one
/// [`actuate_focus`] attempt decides; a focus failure after the committed
/// verified transfer reports the focus outcome with the transfer retained.
/// Fresh topology is re-read immediately before the overlay/focus reads so a
/// late fullscreen drift cannot slip into a focus actuation.
#[allow(clippy::too_many_arguments)]
pub(super) fn migration_follow_tail(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    members: &BTreeSet<crate::workspace::WindowKey>,
    migrate_rows: &[crate::workspace_owner::MigrateRow],
    active: Option<tiler_core::directional::WindowId>,
    focus_fulls_fallback: &[Rect],
    log_migrate: &dyn Fn(&'static str, serde_json::Value),
) -> &'static str {
    let Some(active) = active else {
        log_migrate("migrate-applied", serde_json::json!({"focus": "none"}));
        return "migrate-applied";
    };
    let key = members.iter().find(|key| {
        state
            .member_tokens
            .get(*key)
            .is_some_and(|t| t == &active.0)
    });
    // Fresh fullscreen read immediately before any focus setter: the tail
    // topology is re-read here (never the pre-plan row, never stale fulls),
    // so a late fullscreen drift suppresses instead of focusing.
    let fresh_areas = fresh_cross_areas();
    let owned: Vec<Rect>;
    let focus_fulls: &[Rect] = if let Some(areas) = fresh_areas.as_ref() {
        owned = areas.iter().map(|a| a.full).collect();
        &owned
    } else {
        focus_fulls_fallback
    };
    let row_fullscreen = migrate_rows
        .iter()
        .find(|row| row.token == active.0)
        .map(|row| row.fullscreen)
        .unwrap_or(false);
    let fullscreen_live = key
        .map(|key| {
            let (_, live_full_opt) = live_overlay_flags(key.hwnd, focus_fulls);
            live_full_opt.unwrap_or(row_fullscreen)
        })
        .unwrap_or(row_fullscreen);
    if fullscreen_live {
        if let Some(key) = key {
            state.workspaces.note_foreground(key);
        }
        log_migrate(
            "migrate-applied",
            serde_json::json!({"window": active.0, "focus": "suppressed"}),
        );
        return "migrate-applied";
    }
    let minimized =
        key.is_some_and(|key| live_is_iconic(key.hwnd) || state.minimized_omitted.contains(key));
    if minimized {
        log_migrate(
            "migrate-applied",
            serde_json::json!({"window": active.0, "focus": "suppressed"}),
        );
        return "migrate-applied";
    }
    let actuation = actuate_focus(state, me, focus_fulls, observed, retained, &active.0);
    if actuation.outcome != "focus-ok" {
        log_migrate(
            actuation.outcome,
            serde_json::json!({
                "window": active.0,
                "setter_accepted": actuation.setter_accepted,
                "prime_inserted": actuation.prime_inserted,
                "attach_ok": actuation.attach_ok,
                "eventual": actuation.eventual,
                "deferred": actuation.deferred,
            }),
        );
        return actuation.outcome;
    }
    note_focus_advance(state, observed, retained, &active.0);
    if let Some(key) = key {
        state.workspaces.note_foreground(key);
    }
    log_migrate(
        "migrate-applied",
        serde_json::json!({"window": active.0, "setter_accepted": true}),
    );
    "migrate-applied"
}

/// Run one pinned whole-workspace migration through the commit-last native
/// transfer (REQ-WS-12, decisions D1-D9/G37).
///
/// The caller already pinned `(source_output, source_workspace)` as the chord
/// output's active workspace and `target_output` as the adjacent output with
/// `target_prior` as its current workspace; all three stay frozen through
/// transfer and follow (never re-ranked). The target migration domain carries
/// the SAME workspace id on the target output (the prior stays listed and
/// hidden, D4). This snapshots migration fences, dispatches the
/// DISTINCT `MigrateWorkspace` command on a candidate Engine clone with the
/// retained domain (source refill reads the live
/// `core.migration_source_refill` mode), then actuates commit-last exactly
/// like the explicit output-send foundation: fresh topology, one combined
/// apply, its readback, float translation moves, and the arrival proof all
/// verify before the project reassignment and the authoritative Engine
/// publish once. Sticky members stay homed on the source refill; floats ride
/// with class/origin via native output remap (move-only, never a resize).
/// Follow foregrounds the moved active client once (suppressed for a carried
/// fullscreen or a minimized client, which stay minimized); empty and
/// sticky-active migrations take no focus setter. Minimized clients stay
/// minimized. Failures stop narrowly with no replay and no guessed rollback;
/// the retained Engine plan stands and the next observation converges. A
/// focus failure after a committed verified transfer reports the focus
/// outcome with the transfer retained, matching foundation semantics.
/// Returns the workspace-migrate outcome token and logs the full action line.
#[allow(clippy::too_many_arguments)]
pub(super) fn run_workspace_migrate(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    hint_cx: &mut HintCx,
    source_output: &str,
    source_workspace: &str,
    source_rows: &[crate::workspace_owner::OwnerRow],
    migrate_rows: &[crate::workspace_owner::MigrateRow],
    target_output: &str,
    target_prior: &str,
    intent: &QueuedWorkspaceMigrateIntent,
    tick: u64,
    correlation: &CorrelationId,
    correlation_str: &str,
) -> &'static str {
    let log_path = state.log_path.clone();
    let log_migrate = |outcome: &'static str, extra: serde_json::Value| {
        let mut line = serde_json::Map::new();
        line.insert("event".to_owned(), serde_json::json!("workspace-migrate"));
        line.insert("tick".to_owned(), serde_json::json!(tick));
        line.insert("correlation".to_owned(), serde_json::json!(correlation_str));
        line.insert(
            "direction".to_owned(),
            serde_json::json!(crate::snapkey::direction_name(intent.direction)),
        );
        line.insert("edge".to_owned(), serde_json::json!(intent.edge.as_str()));
        line.insert("disposition".to_owned(), serde_json::json!("consumed"));
        line.insert("outcome".to_owned(), serde_json::json!(outcome));
        if let serde_json::Value::Object(map) = extra {
            for (k, v) in map {
                line.insert(k, v);
            }
        }
        log_json_at(&log_path, serde_json::Value::Object(line));
    };
    let Some((source_domain_base, source_key)) = workspace_domain_for(
        source_output,
        source_workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    ) else {
        log_migrate("unknown-output", serde_json::json!({}));
        return "unknown-output";
    };
    let Some((target_domain_base, target_key)) = workspace_domain_for(
        target_output,
        source_workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    ) else {
        log_migrate("unknown-output", serde_json::json!({}));
        return "unknown-output";
    };
    // Fresh user-safety gate before any Engine or project mutation.
    if suspend_read(state, me, fulls).veto.block {
        log_migrate("suspended", serde_json::json!({}));
        return "suspended";
    }
    if foreground_elevated(me) {
        log_migrate("elevated-foreground", serde_json::json!({}));
        return "elevated-foreground";
    }
    if state.allowlist.is_some() && !state.workspace_proof {
        log_migrate("workspace-disabled", serde_json::json!({}));
        return "workspace-disabled";
    }
    // Whole-member scope/lifetime gates before any Engine or project
    // mutation; tail-time fences re-verify live before commit. One drifted
    // member refuses the whole migration (never a partial carry, never a
    // skip): identity drift drops that member's runtime state first.
    let members = state
        .workspaces
        .workspace_members(source_output, source_workspace);
    for key in &members {
        if let Some(stored) = state.member_identity.get(key) {
            if !scope_allows(&state.scope, &stored.exe_path) {
                log_migrate("scope-excluded", serde_json::json!({}));
                return "scope-excluded";
            }
            if !hosted_gate_allows(&stored.exe_path, key.hwnd, stored.pid, &state.scope_hosts) {
                log_migrate("scope-excluded", serde_json::json!({}));
                return "scope-excluded";
            }
        }
        let live_tag = live_member_tag(key.hwnd);
        let lifetime_ok = state.member_tags.get(key).is_some_and(|stored| {
            crate::workspace_owner::visible_lifetime_ok(stored, live_tag.as_deref())
        });
        if !lifetime_ok {
            drop_member_state(state, key);
            log_migrate("identity-changed", serde_json::json!({}));
            return "identity-changed";
        }
    }
    // Focused client binds to a migrating non-sticky member only; empty and
    // sticky-active migrations carry no invented focus (native output
    // switching only, D6).
    let sticky_keys: BTreeSet<crate::workspace::WindowKey> = members
        .iter()
        .filter(|key| state.sticky.contains_key(*key))
        .cloned()
        .collect();
    let focused_token = state
        .focused_token(observed)
        .map(|window| window.0.clone())
        .filter(|token| {
            migrate_rows
                .iter()
                .any(|row| &row.token == token && !row.sticky)
        })
        .unwrap_or_default();
    // Carried pre-plan fences over the pinned pair: the target current view
    // is the pinned prior, the source refill resolves at commit time through
    // the shared G-37 selector (never an ad hoc fallback).
    let source_area = areas.iter().find(|a| a.device == source_output);
    let target_area = areas.iter().find(|a| a.device == target_output);
    let (Some(source_area), Some(target_area)) = (source_area, target_area) else {
        log_migrate("unknown-output", serde_json::json!({}));
        return "unknown-output";
    };
    let source_bounds = match tiling_domain_bounds_with(source_area.work, state.outer_gap) {
        Some(bounds) => bounds,
        None => {
            log_migrate("unknown-output", serde_json::json!({}));
            return "unknown-output";
        }
    };
    let target_bounds = match tiling_domain_bounds_with(target_area.work, state.outer_gap) {
        Some(bounds) => bounds,
        None => {
            log_migrate("unknown-output", serde_json::json!({}));
            return "unknown-output";
        }
    };
    let snapshot = crate::workspace_owner::MigrateFenceSnapshot {
        inner_gap: state.inner_gap,
        outer_gap: state.outer_gap,
        source_output: source_output.to_owned(),
        target_output: target_output.to_owned(),
        moved_workspace: source_workspace.to_owned(),
        source_refill: None,
        target_prior: target_prior.to_owned(),
        source_tiled: state.workspaces.is_tiled(source_output, source_workspace),
        target_tiled: state.workspaces.is_tiled(target_output, target_prior),
        direction: intent.direction,
        source_full: source_area.full,
        target_full: target_area.full,
        source_bounds,
        target_bounds,
        owner: state.owner.as_str().to_owned(),
        generation: state.generation.as_str().to_owned(),
        member_count: migrate_rows.len(),
    };
    let membership_ok = members.iter().all(|key| {
        state.workspaces.member_loc(key).is_some_and(|fresh| {
            fresh.output == source_output && fresh.workspace == source_workspace
        })
    });
    let live = crate::workspace_owner::MigrateFenceLive {
        inner_gap: state.inner_gap,
        outer_gap: state.outer_gap,
        source_active: state.workspaces.active_id(source_output),
        target_active: state.workspaces.active_id(target_output),
        source_tiled: state.workspaces.is_tiled(source_output, source_workspace),
        target_tiled: state.workspaces.is_tiled(target_output, target_prior),
        topology_ok: fresh_cross_areas().is_some_and(|fresh| {
            cross_topology_fresh(
                &crate::workspace_owner::migrate_snapshot_for_drag(&snapshot),
                &fresh,
            )
        }),
        scope_ok: true,
        lifetime_ok: true,
        membership_ok,
        views_ok: state.workspaces.active_id(source_output).as_deref() == Some(source_workspace)
            && state.workspaces.active_id(target_output).as_deref() == Some(target_prior),
        owner_ok: true,
    };
    if let Some(outcome) = crate::workspace_owner::migrate_fences_hold(&snapshot, &live) {
        if outcome == "identity-changed" {
            for key in &members {
                drop_member_state(state, key);
            }
        }
        log_migrate(outcome, serde_json::json!({}));
        return outcome;
    }
    // D4 untiled source: preserve native frames via move-only native remap
    // with no tiled geometry and no Engine dispatch (the Engine is normally
    // absent after release). The candidate Engine stays untouched; the empty
    // plan below carries only the remembered follow focus. A stale retained
    // source session refuses before effects below so the project commit can
    // never straddle a retained Engine domain.
    let source_tiled = state.workspaces.is_tiled(source_output, source_workspace);
    // Candidate Engine for the whole-workspace route: gap adopts and the
    // migrate dispatch below all run on the clone. The authoritative Engine
    // publishes once, only after the verified tails commit.
    let mut candidate = state.engine.clone();
    let (reply, plan) = if !source_tiled {
        let _ = hint_cx;
        let active_window = if focused_token.is_empty() {
            None
        } else {
            Some(tiler_core::directional::WindowId(focused_token.clone()))
        };
        let empty = tiler_core::boundary::MigrateWorkspacePlan {
            base_revision: revision_for(state, source_output, source_workspace),
            direction: intent.direction,
            source: source_key.clone(),
            target: target_key.clone(),
            geometry: Vec::new(),
            focus_domain: None,
            focus_leaf: None,
            active_window,
            members: migrate_rows.iter().filter(|r| !r.sticky).count(),
            floats: migrate_rows
                .iter()
                .filter(|r| !r.sticky && r.floating)
                .count(),
            preconditions: Vec::new(),
        };
        (
            tiler_core::boundary::CoreReply::MigrateWorkspace(empty.clone()),
            empty,
        )
    } else {
        {
            let source_windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = source_rows
                .iter()
                .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
                .collect();
            let source_fp = crate::tiling::fingerprint(
                &source_rows
                    .iter()
                    .map(|r| (r.token.clone(), r.rect))
                    .collect::<Vec<_>>(),
            );
            let focused = if focused_token.is_empty() {
                None
            } else {
                Some(WindowId(focused_token.clone()))
            };
            adopt_gaps_on_candidate(
                &mut candidate,
                state,
                &source_domain_base,
                &source_key,
                state.outer_gap,
                &source_windows,
                focused.as_ref(),
                revision_for(state, source_output, source_workspace),
                source_fp,
                correlation,
            );
        }
        let combined_fp = {
            let mut pairs: Vec<(String, Rect)> = source_rows
                .iter()
                .map(|r| (r.token.clone(), r.rect))
                .collect();
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            crate::tiling::fingerprint(&pairs)
        };
        // The shared hint-query budget is source-only here: the target domain
        // carries no rows (the Engine refuses carried target residue), so the
        // caller's `hint_cx` stays unconsumed by this route.
        let _ = hint_cx;
        let Some(mut migrate_event) = crate::workspace_owner::build_migrate_event(
            &state.owner,
            &state.generation,
            correlation,
            revision_for(state, source_output, source_workspace),
            combined_fp,
            (source_domain_base.clone(), source_key.clone()),
            (target_domain_base.clone(), target_key.clone()),
            migrate_rows,
            &focused_token,
            direction_name(intent.direction),
            state.outer_gap,
        ) else {
            log_migrate("refused", serde_json::json!({}));
            return "refused";
        };
        let reply = {
            stamp_engine_event(state, &mut migrate_event);
            candidate.handle(&migrate_event)
        };
        let plan = match &reply {
            tiler_core::boundary::CoreReply::MigrateWorkspace(plan) => plan.clone(),
            tiler_core::boundary::CoreReply::Rejected { kind, .. } => {
                log_migrate(kind, serde_json::json!({}));
                return kind;
            }
            tiler_core::boundary::CoreReply::Diverged(reason) => {
                let outcome = reason.as_str();
                log_migrate(outcome, serde_json::json!({}));
                return outcome;
            }
            tiler_core::boundary::CoreReply::SnapshotInvalid { detail, .. } => {
                log_migrate(detail, serde_json::json!({}));
                return detail;
            }
            _ => {
                log_migrate("refused", serde_json::json!({}));
                return "refused";
            }
        };
        (reply, plan)
    };
    // Untiled sources carry no Engine domain: a stale retained source session
    // (release missed) refuses before any native effect so the project commit
    // below can never straddle a retained Engine domain. Fail-closed with
    // reconcile, no replay, no publication.
    if !source_tiled && state.engine.session(&source_key).is_some() {
        log_migrate("unverified", serde_json::json!({}));
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return "unverified";
    }
    // Fresh topology before any native effect: the pinned pair must still
    // touch with identical FULL rectangles and inset bounds (never
    // re-ranked off the relocated members).
    let drag_snapshot = crate::workspace_owner::migrate_snapshot_for_drag(&snapshot);
    let Some(fresh_areas) = fresh_cross_areas() else {
        log_migrate("migrate-unverified", serde_json::json!({}));
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return "migrate-unverified";
    };
    if !cross_topology_fresh(&drag_snapshot, &fresh_areas) {
        log_migrate("migrate-unverified", serde_json::json!({}));
        let fresh_fulls: Vec<Rect> = fresh_areas.iter().map(|a| a.full).collect();
        reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
        return "migrate-unverified";
    }
    let fresh_fulls: Vec<Rect> = fresh_areas.iter().map(|a| a.full).collect();
    // One combined apply for tiled plain members only: the Engine plan
    // reflows the relocated domain in DPI-aware physical coordinates with no
    // rescaling. Core still projects retained tiled leaves for carried
    // overlays (`fit_excluded` never removes them), so overlays are excluded
    // here by inspection and ride the native D8 tail below (move-only, never
    // a resize/refit/restore). Floats, hidden and minimized members converge
    // Engine membership but never take geometry writes here.
    let overlay_tokens: HashSet<String> = migrate_rows
        .iter()
        .filter(|row| !row.sticky && (row.fullscreen || row.maximized))
        .map(|row| row.token.clone())
        .collect();
    let float_tokens_migrate: HashSet<String> = migrate_rows
        .iter()
        .filter(|row| !row.sticky && row.floating)
        .map(|row| row.token.clone())
        .collect();
    let mut writable = writable_tokens(state, source_output, source_workspace, observed);
    writable
        .retain(|token| !overlay_tokens.contains(token) && !float_tokens_migrate.contains(token));
    // Untiled source takes no tiled geometry: every non-sticky member moves
    // natively move-only below (native frames preserved, no Engine dispatch).
    if !source_tiled {
        writable.clear();
    }
    // Untiled sources skip the combined tiled apply entirely (no Engine
    // geometry to write or read back); tiled sources run it exactly once.
    if !source_tiled {
        let _ = &reply;
    }
    let summary = if source_tiled {
        apply_geometry(
            state,
            ApplyInput {
                me,
                fulls: &fresh_fulls,
                reply: &reply,
                observed,
                op: "workspace-migrate",
                tick,
                correlation: correlation_str,
                skipped: Vec::new(),
                writable: &writable,
                output_token: state.workspaces.output_token(source_output),
                workspace_token: state
                    .workspaces
                    .workspace_token(source_output, source_workspace),
                revision: revision_for(state, source_output, source_workspace),
            },
        )
    } else {
        // No tiled writes were issued, so there is nothing to read back:
        // report a clean empty apply for the gates below.
        Some(ApplySummary {
            applied: 0,
            mismatched: 0,
            readback_ok: true,
            veto: None,
        })
    };
    let Some(summary) = summary else {
        log_migrate("migrate-unverified", serde_json::json!({}));
        reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
        return "migrate-unverified";
    };
    if !summary.readback_ok {
        log_migrate("migrate-unverified", serde_json::json!({}));
        reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
        return "migrate-unverified";
    }
    if summary.mismatched > 0 {
        // Partial native writes converge on observation without setter
        // replay: reconcile the fresh domains and report, never a guessed
        // rollback.
        reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
        log_migrate("migrate-mismatch", serde_json::json!({}));
        return "migrate-mismatch";
    }
    // ALL-member native output remap (D7/D8, move-only, never a resize or
    // refit): every migrating non-sticky member outside the tiled-plain
    // writable set remaps by the FULL-rectangle origin delta onto the target
    // output. Plain floats and fullscreen overlays move with `SetWindowPos`
    // NOSIZE; maximized overlays remap placement normal rect preserving
    // `showCmd` (still maximized, reserved slot kept); minimized members
    // remap placement normal rect preserving `showCmd` (still minimized);
    // hidden members remap placement preserving hidden state. Sticky members
    // stay on the source and take no move. Each setter re-verifies fresh
    // identity/lifetime/scope/topology/view/overlay fences first; each
    // arrival verifies monitor/identity (overlays) or output+size (floats)
    // or output+show-state (concealed), never the old exact rect. Any
    // failure stops narrowly with reconcile, no replay, no publication; the
    // next observation converges the partial physical moves.
    let row_by_token: HashMap<&str, &crate::workspace_owner::MigrateRow> = migrate_rows
        .iter()
        .map(|row| (row.token.as_str(), row))
        .collect();
    let mut transfers: Vec<(crate::workspace::WindowKey, String)> = Vec::new();
    for key in &members {
        let Some(token) = state.member_tokens.get(key).cloned() else {
            continue;
        };
        let Some(row) = row_by_token.get(token.as_str()) else {
            continue;
        };
        if row.sticky {
            continue;
        }
        if writable.contains(token.as_str()) {
            continue;
        }
        transfers.push((key.clone(), token));
    }
    transfers.sort_by(|a, b| a.1.cmp(&b.1));
    for (key, token) in &transfers {
        let Some(row) = row_by_token.get(token.as_str()).copied() else {
            continue;
        };
        // Per-effect fences before the setter: scope, lifetime, topology,
        // views, owner, overlay. Any drift refuses with reconcile and no
        // replay.
        let scope_ok = state.member_identity.get(key).is_some_and(|stored| {
            scope_allows(&state.scope, &stored.exe_path)
                && hosted_gate_allows(&stored.exe_path, key.hwnd, stored.pid, &state.scope_hosts)
        });
        if !scope_ok {
            reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
            log_migrate("scope-excluded", serde_json::json!({"window": token}));
            return "scope-excluded";
        }
        let lifetime_ok = state.member_tags.get(key).is_some_and(|stored| {
            crate::workspace_owner::visible_lifetime_ok(
                stored,
                live_member_tag(key.hwnd).as_deref(),
            )
        });
        if !lifetime_ok {
            drop_member_state(state, key);
            reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
            log_migrate("identity-changed", serde_json::json!({"window": token}));
            return "identity-changed";
        }
        let Some(tail_areas) = fresh_cross_areas() else {
            reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
            log_migrate("migrate-unverified", serde_json::json!({"window": token}));
            return "migrate-unverified";
        };
        if !cross_topology_fresh(&drag_snapshot, &tail_areas) {
            let tail_fulls: Vec<Rect> = tail_areas.iter().map(|a| a.full).collect();
            reconcile_tick(state, me, &tail_fulls, &tail_areas);
            log_migrate("migrate-unverified", serde_json::json!({"window": token}));
            return "migrate-unverified";
        }
        if state.workspaces.active_id(source_output).as_deref() != Some(source_workspace)
            || state.workspaces.active_id(target_output).as_deref() != Some(target_prior)
        {
            reconcile_tick(state, me, &fresh_fulls, &tail_areas);
            log_migrate("view-changed", serde_json::json!({"window": token}));
            return "view-changed";
        }
        if state
            .engine
            .owner()
            .is_none_or(|owner| owner.as_str() != snapshot.owner.as_str())
            || state
                .engine
                .generation()
                .is_none_or(|generation| generation.as_str() != snapshot.generation.as_str())
        {
            reconcile_tick(state, me, &fresh_fulls, &tail_areas);
            log_migrate("owner-changed", serde_json::json!({"window": token}));
            return "owner-changed";
        }
        let hidden = state.workspaces.is_hidden(key);
        let minimized = live_is_iconic(key.hwnd) || state.minimized_omitted.contains(key);
        // Overlay drift: a visible member whose live flags no longer match
        // the carried row authorizes no move. Hidden/minimized members keep
        // no live frame (`None` fullscreen), so only the maximized bit is
        // fenced there against the retained row.
        if hidden || minimized {
            let (live_max, _) = live_overlay_flags(key.hwnd, &fresh_fulls);
            if live_max != row.maximized {
                reconcile_tick(state, me, &fresh_fulls, &tail_areas);
                log_migrate("migrate-unverified", serde_json::json!({"window": token}));
                return "migrate-unverified";
            }
        } else {
            let (live_max, live_full_opt) = live_overlay_flags(key.hwnd, &fresh_fulls);
            if live_max != row.maximized
                || live_full_opt.unwrap_or(row.fullscreen) != row.fullscreen
            {
                reconcile_tick(state, me, &fresh_fulls, &tail_areas);
                log_migrate("migrate-unverified", serde_json::json!({"window": token}));
                return "migrate-unverified";
            }
        }
        // Source frame: live visible frame when present, else the carried
        // normal frame (retained/member/float snapshot) for concealed
        // members. No frame is never fabricated: missing refuses.
        let frame = observed
            .iter()
            .find(|window| window.token == *token)
            .map(|window| window.visible)
            .or_else(|| retained.iter().find(|r| r.key == *key).and_then(|r| r.rect))
            .or_else(|| state.member_rects.get(token).copied())
            .or_else(|| state.float_rects.get(token).copied())
            .unwrap_or(row.rect);
        let moved = crate::workspace_owner::translate_rect_to_output(
            frame,
            snapshot.source_full,
            snapshot.target_full,
        );
        if output_for_rect(&tail_areas, &moved) != target_output {
            reconcile_tick(state, me, &fresh_fulls, &tail_areas);
            log_migrate("migrate-unverified", serde_json::json!({"window": token}));
            return "migrate-unverified";
        }
        let setter_ok = if hidden {
            let target_area = tail_areas.iter().find(|a| a.device == target_output);
            let target_work = target_area.map(|a| a.work).unwrap_or(snapshot.target_full);
            let target_full = target_area.map(|a| a.full).unwrap_or(snapshot.target_full);
            let basis = live_placement_workspace_coords(key.hwnd);
            remap_placement_to_output(key.hwnd, moved, "hidden", target_work, target_full, basis)
        } else if minimized {
            let target_area = tail_areas.iter().find(|a| a.device == target_output);
            let target_work = target_area.map(|a| a.work).unwrap_or(snapshot.target_full);
            let target_full = target_area.map(|a| a.full).unwrap_or(snapshot.target_full);
            let basis = live_placement_workspace_coords(key.hwnd);
            remap_placement_to_output(
                key.hwnd,
                moved,
                "minimized",
                target_work,
                target_full,
                basis,
            )
        } else if row.fullscreen || row.maximized {
            let source_area = tail_areas.iter().find(|a| a.device == source_output);
            let target_area = tail_areas.iter().find(|a| a.device == target_output);
            let source_work = source_area.map(|a| a.work).unwrap_or(snapshot.source_full);
            let target_work = target_area.map(|a| a.work).unwrap_or(snapshot.target_full);
            let source_full = source_area.map(|a| a.full).unwrap_or(snapshot.source_full);
            let target_full = target_area.map(|a| a.full).unwrap_or(snapshot.target_full);
            let basis = live_placement_workspace_coords(key.hwnd);
            move_overlay_to_output(
                key.hwnd,
                moved,
                row.maximized,
                source_full,
                target_full,
                source_work,
                target_work,
                basis,
            )
        } else if row.floating || !source_tiled {
            // Untiled plain members preserve native frames via the same
            // move-only float path (never a resize/refit); tiled plain
            // members never reach here (they ride the combined apply).
            move_float_to_output(key.hwnd, moved)
        } else {
            reconcile_tick(state, me, &fresh_fulls, &tail_areas);
            log_migrate("migrate-unverified", serde_json::json!({"window": token}));
            return "migrate-unverified";
        };
        if !setter_ok {
            reconcile_tick(state, me, &fresh_fulls, &tail_areas);
            log_migrate("migrate-unverified", serde_json::json!({"window": token}));
            return "migrate-unverified";
        }
        // Arrival proof per member before the next setter: identity still
        // holds (pid/lifetime rechecked live) and the fresh native physical
        // output verifies after the actual setter (never the pre-move stable
        // cache, never translation math). Visible floats verify output plus
        // preserved size; visible overlays verify monitor/identity only,
        // never the old exact rect (D8); concealed members verify fresh
        // placement output plus preserved show-state (still
        // minimized/hidden, maximized slot kept).
        #[cfg(test)]
        let present = state.member_tags.contains_key(key);
        #[cfg(not(test))]
        let present = unsafe { IsWindow(key.hwnd as isize as HWND) } != 0;
        if !present {
            reconcile_tick(state, me, &fresh_fulls, &tail_areas);
            log_migrate("migrate-unverified", serde_json::json!({"window": token}));
            return "migrate-unverified";
        }
        let arrival_ok = if hidden {
            // Concealed hidden arrival: fresh placement readback converted to
            // screen must sit on the target output (physical proof, never
            // translation math) and the project hidden intent must hold.
            let target_area = tail_areas.iter().find(|a| a.device == target_output);
            let target_work = target_area.map(|a| a.work).unwrap_or(snapshot.target_full);
            let target_full = target_area.map(|a| a.full).unwrap_or(snapshot.target_full);
            let basis = live_placement_workspace_coords(key.hwnd);
            state.workspaces.is_hidden(key)
                && fresh_arrival_placement(key.hwnd, target_work, target_full, basis)
                    .is_some_and(|placed| output_for_rect(&tail_areas, &placed) == target_output)
        } else if minimized {
            // Concealed minimized arrival: still minimized plus fresh
            // placement converted to screen on the target.
            let target_area = tail_areas.iter().find(|a| a.device == target_output);
            let target_work = target_area.map(|a| a.work).unwrap_or(snapshot.target_full);
            let target_full = target_area.map(|a| a.full).unwrap_or(snapshot.target_full);
            let basis = live_placement_workspace_coords(key.hwnd);
            (live_is_iconic(key.hwnd) || state.minimized_omitted.contains(key))
                && fresh_arrival_placement(key.hwnd, target_work, target_full, basis)
                    .is_some_and(|placed| output_for_rect(&tail_areas, &placed) == target_output)
        } else if (row.floating || !source_tiled) && !row.fullscreen && !row.maximized {
            // Visible float (or untiled plain via the move-only path)
            // arrival: fresh native visible frame preserves size on the target
            // (causal readback after the setter, never stale cache).
            fresh_arrival_rect(key.hwnd).is_some_and(|fresh| {
                fresh.w == frame.w
                    && fresh.h == frame.h
                    && output_for_rect(&tail_areas, &fresh) == target_output
            })
        } else {
            // Overlay (maximized/fullscreen) arrival: fresh native visible
            // frame on the target monitor plus the live overlay flag, never
            // the old exact rect (native re-fit may change the maximized
            // frame size on the target work area).
            let tail_fulls: Vec<Rect> = tail_areas.iter().map(|a| a.full).collect();
            let (live_max, live_full_opt) = live_overlay_flags(key.hwnd, &tail_fulls);
            let flag_ok = live_max == row.maximized
                && live_full_opt.unwrap_or(row.fullscreen) == row.fullscreen;
            flag_ok
                && fresh_arrival_rect(key.hwnd)
                    .is_some_and(|fresh| output_for_rect(&tail_areas, &fresh) == target_output)
        };
        if !arrival_ok {
            reconcile_tick(state, me, &fresh_fulls, &tail_areas);
            log_migrate("migrate-unverified", serde_json::json!({"window": token}));
            return "migrate-unverified";
        }
    }
    // Required arrival proof before claiming success: every migrating tiled
    // plain member with a planned write must read back at its planned rect on
    // the destination monitor. Overlay/float/concealed arrivals verified per
    // member above, never by their old frame (D8); only writable tiled plain
    // writes gate here. A failed proof reports unverified with reconcile, no
    // commit, no revert, and no replay; the next observation converges.
    let writes = crate::workspace_owner::planned_writes(&reply);
    if let Some(writes) = writes {
        for write in &writes {
            if !writable.contains(write.window.0.as_str()) {
                continue;
            }
            let Some(key) = members.iter().find(|key| {
                state
                    .member_tokens
                    .get(*key)
                    .is_some_and(|t| t == &write.window.0)
            }) else {
                continue;
            };
            let effective = if write.overconstrained {
                let hints = state
                    .hint_logged
                    .get(write.window.0.as_str())
                    .copied()
                    .unwrap_or(WindowSizeHints::none());
                crate::tiling::overconstrained_effective(write.rect, hints)
            } else {
                write.rect
            };
            // Tiled plain arrival: fresh native frame after the combined
            // apply must equal the planned rect on the destination monitor
            // (causal readback, never the pre-move stable cache).
            let Some(visible) = fresh_arrival_rect(key.hwnd) else {
                reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
                log_migrate(
                    "migrate-unverified",
                    serde_json::json!({"window": write.window.0}),
                );
                return "migrate-unverified";
            };
            if visible != effective || output_for_rect(&fresh_areas, &visible) != target_output {
                reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
                log_migrate(
                    "migrate-unverified",
                    serde_json::json!({"window": write.window.0}),
                );
                return "migrate-unverified";
            }
        }
    }
    // Final live fences after effects, before logical commit: suspend,
    // elevated, and the full pinned-snapshot fence set re-verified live
    // against the current fresh topology (current target_scope fences after
    // effects). Any drift refuses with reconcile, no commit, no replay, and
    // no success claim; both retained authorities stand and the next
    // observation converges.
    if suspend_read(state, me, &fresh_fulls).veto.block {
        log_migrate("suspended", serde_json::json!({}));
        return "suspended";
    }
    if foreground_elevated(me) {
        log_migrate("elevated-foreground", serde_json::json!({}));
        return "elevated-foreground";
    }
    {
        let Some(tail_areas) = fresh_cross_areas() else {
            reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
            log_migrate("migrate-unverified", serde_json::json!({}));
            return "migrate-unverified";
        };
        if !cross_topology_fresh(&drag_snapshot, &tail_areas) {
            let tail_fulls: Vec<Rect> = tail_areas.iter().map(|a| a.full).collect();
            reconcile_tick(state, me, &tail_fulls, &tail_areas);
            log_migrate("migrate-unverified", serde_json::json!({}));
            return "migrate-unverified";
        }
        let membership_ok = members.iter().all(|key| {
            state.workspaces.member_loc(key).is_some_and(|fresh| {
                fresh.output == source_output && fresh.workspace == source_workspace
            })
        });
        let views_ok = state.workspaces.active_id(source_output).as_deref()
            == Some(source_workspace)
            && state.workspaces.active_id(target_output).as_deref() == Some(target_prior);
        let scope_ok = members.iter().all(|key| {
            state.member_identity.get(key).is_some_and(|stored| {
                scope_allows(&state.scope, &stored.exe_path)
                    && hosted_gate_allows(
                        &stored.exe_path,
                        key.hwnd,
                        stored.pid,
                        &state.scope_hosts,
                    )
            })
        });
        let lifetime_ok = members.iter().all(|key| {
            state.member_tags.get(key).is_some_and(|stored| {
                crate::workspace_owner::visible_lifetime_ok(
                    stored,
                    live_member_tag(key.hwnd).as_deref(),
                )
            })
        });
        let owner_ok = state
            .engine
            .owner()
            .is_some_and(|owner| owner.as_str() == snapshot.owner.as_str())
            && state
                .engine
                .generation()
                .is_some_and(|generation| generation.as_str() == snapshot.generation.as_str());
        let live = crate::workspace_owner::MigrateFenceLive {
            inner_gap: state.inner_gap,
            outer_gap: state.outer_gap,
            source_active: state.workspaces.active_id(source_output),
            target_active: state.workspaces.active_id(target_output),
            source_tiled: state.workspaces.is_tiled(source_output, source_workspace),
            target_tiled: state.workspaces.is_tiled(target_output, target_prior),
            topology_ok: true,
            scope_ok,
            lifetime_ok,
            membership_ok,
            views_ok,
            owner_ok,
        };
        if let Some(outcome) = crate::workspace_owner::migrate_fences_hold(&snapshot, &live) {
            if outcome == "identity-changed" {
                for key in &members {
                    drop_member_state(state, key);
                }
            }
            reconcile_tick(state, me, &fresh_fulls, &tail_areas);
            log_migrate(outcome, serde_json::json!({}));
            return outcome;
        }
    }
    // Commit-last: the project reassignment runs on a clone through the
    // stable-ID ring primitive (source refill reads the same live G-37
    // mode), then both authorities publish once only after the membership
    // plus both-view verification holds. A refusal below publishes nothing.
    let refill_mode = state
        .settings_live
        .as_ref()
        .map(|live| live.settings.core.migration_source_refill_mode())
        .unwrap_or_default();
    let mut candidate_ws = state.workspaces.clone();
    let Some(outcome) = candidate_ws.migrate_workspace_excluding(
        source_output,
        target_output,
        refill_mode,
        &sticky_keys,
    ) else {
        reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
        log_migrate("unverified", serde_json::json!({}));
        return "unverified";
    };
    let migrating: BTreeSet<crate::workspace::WindowKey> =
        members.difference(&sticky_keys).cloned().collect();
    // Empty-source convergence: when the primitive reports no refill the
    // source keeps an empty order with no current view. Ordinary
    // trailing-empty lifecycle (minimum-two, one empty) converges on the
    // candidate before verification can publish; no ad-hoc empty-source
    // success. Sticky residue re-homes to the converged refill below.
    if outcome.source_refill.is_none() {
        let displaced: Vec<String> = candidate_ws
            .displaced_snapshot()
            .values()
            .flat_map(|r| r.workspace_ids.clone())
            .collect();
        let (removed, append) =
            candidate_ws.plan_cleanup_excluding(source_output, &[], &displaced, &sticky_keys);
        candidate_ws.apply_cleanup(source_output, &removed, append);
        let converged_refill = candidate_ws.active_id(source_output);
        // Re-home sticky residue left dangling on the moved id (the
        // primitive keeps locations untouched when no refill exists) onto
        // the converged refill, preserving hidden state.
        if let Some(refill) = converged_refill.clone() {
            for key in sticky_keys.iter() {
                if let Some(loc) = candidate_ws.member_loc(key).cloned() {
                    if loc.output == source_output && loc.workspace == outcome.moved_id {
                        candidate_ws.assign(key.clone(), source_output, &refill, loc.hidden);
                    }
                } else if state.workspaces.member_loc(key).is_some_and(|fresh| {
                    fresh.output == source_output && fresh.workspace == source_workspace
                }) {
                    let hidden = state.workspaces.is_hidden(key);
                    candidate_ws.assign(key.clone(), source_output, &refill, hidden);
                }
            }
        }
        let source_members = converged_refill
            .as_deref()
            .map(|refill| candidate_ws.workspace_members(source_output, refill))
            .unwrap_or_default();
        let target_members = candidate_ws.workspace_members(target_output, &outcome.moved_id);
        // Native view publication for the converged refill (same D4/G37
        // staging as the non-empty path): hide the prior, reveal the
        // converged refill before verification can publish.
        {
            let Some(view_areas) = fresh_cross_areas() else {
                reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
                log_migrate("migrate-unverified", serde_json::json!({}));
                return "migrate-unverified";
            };
            if let Some(view_outcome) = migration_view_effects(
                state,
                me,
                store,
                dir,
                &mut candidate_ws,
                source_output,
                converged_refill.as_deref(),
                target_output,
                target_prior,
                &migrating,
                &snapshot,
                &view_areas,
            ) {
                reconcile_tick(state, me, &fresh_fulls, &view_areas);
                log_migrate(view_outcome, serde_json::json!({}));
                return view_outcome;
            }
            let prior_members = state
                .workspaces
                .workspace_members(target_output, target_prior);
            let refill_members = converged_refill
                .as_deref()
                .map(|refill| candidate_ws.workspace_members(source_output, refill))
                .unwrap_or_default();
            let views_physical = prior_members.iter().all(|key| {
                candidate_ws.is_hidden(key)
                    || live_is_iconic(key.hwnd)
                    || state.minimized_omitted.contains(key)
            }) && refill_members
                .iter()
                .all(|key| !candidate_ws.is_hidden(key));
            if !views_physical {
                reconcile_tick(state, me, &fresh_fulls, &view_areas);
                log_migrate("migrate-unverified", serde_json::json!({}));
                return "migrate-unverified";
            }
        }
        if !crate::workspace_owner::verify_migration_transfer(
            &migrating,
            &sticky_keys,
            &source_members,
            &target_members,
            &outcome.moved_id,
            converged_refill.as_deref(),
            candidate_ws.active_id(source_output).as_deref(),
            candidate_ws.active_id(target_output).as_deref(),
        ) {
            reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
            log_migrate("unverified", serde_json::json!({}));
            return "unverified";
        }
        // Publish the lifecycle-converged candidate, then run the single
        // shared follow tail (fresh fullscreen/iconic re-read, no stale row).
        state.workspaces = candidate_ws;
        state.engine = candidate;
        // Item 12 stage B: completed migration views record through the
        // scope contract (global-unique converged refill plus target
        // arrival; local stays on the primitive's own invalidation).
        let moved = outcome.moved_id.clone();
        let converged = converged_refill.clone();
        scope_trailing::note_migration_completed(
            state,
            source_output,
            converged.as_deref(),
            target_output,
            &moved,
        );
        return migration_follow_tail(
            state,
            me,
            observed,
            retained,
            &members,
            migrate_rows,
            plan.active_window.clone(),
            &fresh_fulls,
            &log_migrate,
        );
    }
    let source_members = candidate_ws.workspace_members(
        source_output,
        outcome.source_refill.as_deref().unwrap_or(""),
    );
    let target_members = candidate_ws.workspace_members(target_output, &outcome.moved_id);
    // Native view publication (D4/G37): hide the occupied target prior and
    // reveal the source refill fenced per effect before the membership/view
    // verification can publish. Failures reconcile without replay.
    {
        let Some(view_areas) = fresh_cross_areas() else {
            reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
            log_migrate("migrate-unverified", serde_json::json!({}));
            return "migrate-unverified";
        };
        if let Some(view_outcome) = migration_view_effects(
            state,
            me,
            store,
            dir,
            &mut candidate_ws,
            source_output,
            outcome.source_refill.as_deref(),
            target_output,
            target_prior,
            &migrating,
            &snapshot,
            &view_areas,
        ) {
            reconcile_tick(state, me, &fresh_fulls, &view_areas);
            log_migrate(view_outcome, serde_json::json!({}));
            return view_outcome;
        }
        // Both physical views verify before publish: the prior stays listed
        // but hidden (minimized stays minimized), the refill shows.
        let prior_members = state
            .workspaces
            .workspace_members(target_output, target_prior);
        let refill_members = outcome
            .source_refill
            .as_deref()
            .map(|refill| candidate_ws.workspace_members(source_output, refill))
            .unwrap_or_default();
        let views_physical = prior_members.iter().all(|key| {
            candidate_ws.is_hidden(key)
                || live_is_iconic(key.hwnd)
                || state.minimized_omitted.contains(key)
        }) && refill_members
            .iter()
            .all(|key| !candidate_ws.is_hidden(key));
        if !views_physical {
            reconcile_tick(state, me, &fresh_fulls, &view_areas);
            log_migrate("migrate-unverified", serde_json::json!({}));
            return "migrate-unverified";
        }
    }
    if !crate::workspace_owner::verify_migration_transfer(
        &migrating,
        &sticky_keys,
        &source_members,
        &target_members,
        &outcome.moved_id,
        outcome.source_refill.as_deref(),
        candidate_ws.active_id(source_output).as_deref(),
        candidate_ws.active_id(target_output).as_deref(),
    ) {
        reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
        log_migrate("unverified", serde_json::json!({}));
        return "unverified";
    }
    state.workspaces = candidate_ws;
    state.engine = candidate;
    // Item 12 stage B: completed migration views record through the scope
    // contract (global-unique source refill plus target arrival; local stays
    // on the primitive's own invalidation).
    let refill = outcome.source_refill.clone();
    let moved = outcome.moved_id.clone();
    scope_trailing::note_migration_completed(
        state,
        source_output,
        refill.as_deref(),
        target_output,
        &moved,
    );
    // Verified: the single shared follow tail (D6) runs after publish.
    migration_follow_tail(
        state,
        me,
        observed,
        retained,
        &members,
        migrate_rows,
        plan.active_window.clone(),
        &fresh_fulls,
        &log_migrate,
    )
}
