#![forbid(unsafe_code)]
use super::*;

/// Flush one honest hide-leg omission summary for the migration target-prior
/// leg: shared by the terminal path and every early-return path, so a fence
/// or effect failure after attempted hides never strands counted attempts
/// without their summary.
pub(super) fn flush_migrate_hide_summary(
    log_path: &Path,
    tick: u64,
    correlation: &str,
    attempted: usize,
    logged: usize,
) {
    if let Some(line) = omission_summary(
        "workspace-hide-summary",
        tick,
        correlation,
        None,
        attempted,
        logged,
    ) {
        log_json_at(log_path, line);
    }
}

/// Flush one honest reveal-leg omission summary in the `migrate` phase:
/// shared by the terminal path and the reveal-failure return, so a failure
/// after attempted reveals never strands counted attempts without theirs.
pub(super) fn flush_migrate_reveal_summary(
    log_path: &Path,
    tick: u64,
    correlation: &str,
    attempted: usize,
    logged: usize,
) {
    if let Some(line) = omission_summary(
        "workspace-reveal-summary",
        tick,
        correlation,
        Some("migrate"),
        attempted,
        logged,
    ) {
        log_json_at(log_path, line);
    }
}

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
/// `diag` is diagnostics-only (tick plus caller migration correlation):
/// per-window lines join the `workspace-migrate` action without changing
/// any effect sequence.
#[allow(clippy::too_many_arguments)]
pub(super) fn migration_view_effects(
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
    diag: &ActionCtx,
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
    // Diagnostics-only counters for the target-prior hide leg below: actual
    // `workspace_hide_one` attempts share the 64-entry per-select hide cap
    // with the remainder reported through one omission summary. The mutations
    // live in the production-only block below, so test builds read only.
    #[cfg_attr(test, allow(unused_mut))]
    let mut hide_attempted: usize = 0;
    #[cfg_attr(test, allow(unused_mut))]
    let mut hide_logged: usize = 0;
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
            flush_migrate_hide_summary(
                &state.log_path,
                state.tick,
                &diag.correlation,
                hide_attempted,
                hide_logged,
            );
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
            let project_topmost = crate::product_hide::project_topmost_hide_allowed(
                state.float_topmost_prev.get(key).copied(),
                read_topmost_now(key.hwnd),
            );
            let (outcome, detail) =
                workspace_hide_one(state, me, store, dir, key, &expected, iconic);
            hide_attempted += 1;
            note_hide_diag(
                state,
                diag,
                key,
                outcome,
                detail,
                project_topmost,
                &mut hide_logged,
            );
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
                flush_migrate_hide_summary(
                    &state.log_path,
                    state.tick,
                    &diag.correlation,
                    hide_attempted,
                    hide_logged,
                );
                return Some(outcome);
            }
        }
    }
    flush_migrate_hide_summary(
        &state.log_path,
        state.tick,
        &diag.correlation,
        hide_attempted,
        hide_logged,
    );
    // Reveal helper: one hidden-claim member whose new active view permits
    // visibility (iconic preserved, never unminimized; flag mirrored onto
    // the candidate). Attempts log per window in the `migrate` phase; retired
    // claims drop member state exactly like the select path.
    let reveal_one = |state: &mut TileLoop,
                      candidate_ws: &mut crate::workspace::ManagedWorkspaces,
                      key: &crate::workspace::WindowKey,
                      diag: &ActionCtx,
                      reveal_attempted: &mut usize,
                      reveal_logged: &mut usize|
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
            let _ = (record, diag, reveal_attempted, reveal_logged);
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
                    *reveal_attempted += 1;
                    note_reveal_diag(
                        state,
                        diag,
                        key,
                        "retired",
                        None,
                        Some("migrate"),
                        reveal_logged,
                    );
                    state.hidden_claims.remove(key);
                    drop_member_state(state, key);
                    candidate_ws.remove_window(key);
                    Ok(())
                }
                Ok(teardown) => {
                    *reveal_attempted += 1;
                    note_reveal_diag(
                        state,
                        diag,
                        key,
                        reveal_outcome(&teardown),
                        None,
                        Some("migrate"),
                        reveal_logged,
                    );
                    state.hidden_claims.remove(key);
                    state.workspaces.set_hidden(key, false);
                    candidate_ws.set_hidden(key, false);
                    Ok(())
                }
                Err(e) => {
                    *reveal_attempted += 1;
                    let (outcome, detail) = reveal_err_outcome(&e.to_string());
                    note_reveal_diag(
                        state,
                        diag,
                        key,
                        outcome,
                        Some(detail),
                        Some("migrate"),
                        reveal_logged,
                    );
                    Err("migrate-unverified")
                }
            }
        }
    };
    // Source refill shows (G37): reveal its hidden members now that the
    // candidate shows the refill.
    let mut reveal_attempted: usize = 0;
    let mut reveal_logged: usize = 0;
    if let Some(refill) = refill {
        let refill_members: Vec<crate::workspace::WindowKey> = candidate_ws
            .workspace_members(source_output, refill)
            .into_iter()
            .collect();
        for key in &refill_members {
            // Minimized refill members stay minimized: the reveal path
            // preserves iconic, so they still converge (no skip).
            if reveal_one(
                state,
                candidate_ws,
                key,
                diag,
                &mut reveal_attempted,
                &mut reveal_logged,
            )
            .is_err()
            {
                flush_migrate_reveal_summary(
                    &state.log_path,
                    state.tick,
                    &diag.correlation,
                    reveal_attempted,
                    reveal_logged,
                );
                return Some("migrate-unverified");
            }
        }
    }
    flush_migrate_reveal_summary(
        &state.log_path,
        state.tick,
        &diag.correlation,
        reveal_attempted,
        reveal_logged,
    );
    // Migrated members preserve their pre-existing hidden/minimized intent:
    // no reveal here (the move carries visibility as-is; an active view does
    // not unhide intentional concealment nor unminimize). Membership was
    // fenced per effect above.
    let _ = tail_areas;
    None
}
