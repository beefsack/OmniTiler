use super::*;

/// Drain one bounded batch of item 14 whole-active-workspace migration intents
/// (R-WS-12 D1-D9/G37). Each intent migrates the ACTIVE workspace of the chord
/// output only (follow-only, unbound by default): resolve the adjacent output
/// once through the shared largest-shared-edge FULL-rectangle selector (no
/// candidate is a no-op, unreadable topology refuses, outputs never wrap),
/// then transfer through the commit-last [`run_workspace_migrate`] route with
/// the live G-37 source-refill mode. D4 retains per-output tiling modes:
/// untiled source and untiled target prior are allowed (independent
/// project-owned views, never a both-tiled gate); mode drift fences below.
/// `--no-keyboard-snap-takeover` off, fullscreen/elevated gates,
/// observation failure and unknown-output refusals mirror the output-send
/// tick.
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace_migrate_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    events: Vec<QueuedWorkspaceMigrateIntent>,
    blocked: Option<&'static str>,
) {
    let log_path = state.log_path.clone();
    if let Some(cause) = blocked {
        let stale = events.len() as u32;
        if stale > 0 {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-migrate-stale","cause": cause, "dropped": stale}),
            );
        }
        return;
    }
    if !state.keyboard.takeover {
        if !events.is_empty() {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-migrate-stale","cause":"takeover-off","dropped": events.len()}),
            );
        }
        return;
    }
    for intent in events {
        if !intent.consumed || !intent.announce {
            if state.trace {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "workspace-migrate",
                        "tick": state.tick,
                        "direction": direction_name(intent.direction),
                        "edge": intent.edge.as_str(),
                        "disposition": if intent.consumed { "consumed" } else { "passed" },
                        "outcome": "key-up",
                    }),
                );
            }
            continue;
        }
        let direction = direction_name(intent.direction);
        let edge = intent.edge.as_str();
        if suspend_read(state, me, fulls).veto.block {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-migrate",
                    "direction": direction,
                    "edge": edge,
                    "disposition": "consumed",
                    "outcome": "suspended",
                }),
            );
            continue;
        }
        if foreground_elevated(me) {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-migrate",
                    "direction": direction,
                    "edge": edge,
                    "disposition": "consumed",
                    "outcome": "elevated-foreground",
                }),
            );
            continue;
        }
        let Some(output) = chord_output(state, areas) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-migrate",
                    "direction": direction,
                    "edge": edge,
                    "disposition": "consumed",
                    "outcome": "unknown-output",
                }),
            );
            continue;
        };
        state.workspaces.ensure_output(&output);
        state.tick += 1;
        let tick = state.tick;
        let mut skipped: Vec<(String, String)> = Vec::new();
        let mut retained: Vec<RetainedRow> = Vec::new();
        let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-migrate",
                    "tick": tick,
                    "direction": direction_name(intent.direction),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "observation-failed",
                }),
            );
            continue;
        };
        publish_managed(state, me, &observed, &retained);
        ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
        clear_maximize_at_admission(state, me, &retained);
        workspace_close_cleanup(state);
        // `run_workspace_migrate` logs the full action line; narrow
        // pre-dispatch refusals log here.
        let _ = workspace_migrate_one(
            state,
            me,
            store,
            dir,
            fulls,
            areas,
            &mut observed,
            &retained,
            &output,
            &intent,
            tick,
        );
    }
}

/// Dispatch one observed migration intent on its active output: pin the source
/// active workspace, resolve the adjacent target once through the shared
/// largest-edge selector, then run the commit-last transfer. The pinned
/// source/target pair never re-resolves below.
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace_migrate_one(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &mut [ObservedWindow],
    retained: &[RetainedRow],
    output: &str,
    intent: &QueuedWorkspaceMigrateIntent,
    tick: u64,
) -> &'static str {
    let log_path = state.log_path.clone();
    let log_refuse = |outcome: &'static str| {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "workspace-migrate",
                "tick": tick,
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": outcome,
            }),
        );
    };
    // Item 12 stage B: shared whole-workspace migration refuses per the
    // established contract before any mutation.
    if let Some(outcome) = scope_sends::migration_scope_refusal(state) {
        log_refuse(outcome);
        return outcome;
    }
    let Some(source_workspace) = state.workspaces.active_id(output) else {
        log_refuse("unknown-output");
        return "unknown-output";
    };
    let outputs = directional_outputs_from_areas(areas);
    let selected =
        match crate::workspace_owner::select_migration_target(intent.direction, output, &outputs) {
            Ok(selected) => selected,
            Err(_) => {
                log_refuse("unreadable-topology");
                return "unreadable-topology";
            }
        };
    let Some(target_output) = selected else {
        log_refuse("no-op");
        return "no-op";
    };
    if target_output == output {
        log_refuse("no-op");
        return "no-op";
    }
    state.workspaces.ensure_output(&target_output);
    // The destination's DIFFERENT current workspace stays adapter-owned: it
    // is the pinned prior for insertion (D4: the moved id inserts
    // immediately after it) and for fences, never a carried domain. The
    // target migration domain carries the SAME workspace id on the target
    // output with empty target windows (the Engine refuses carried target
    // residue).
    let Some(target_prior) = state.workspaces.active_id(&target_output) else {
        log_refuse("unknown-output");
        return "unknown-output";
    };
    // D4 mode retention: independent project-owned output views carry
    // their own tiling modes, so untiled source and untiled target prior are
    // both allowed (no both-tiled gate). Mode drift still fences below via
    // the snapshot/live tiled pair; an untiled source takes move-only native
    // transfer with no tiled geometry and no Engine dispatch.
    let correlation = state.correlation();
    let mut hint_cx = HintCx::new();
    let Some(source_rows) = assemble_domain_rows(
        state,
        output,
        &source_workspace,
        observed,
        retained,
        "workspace-migrate",
        correlation.as_str(),
        &mut hint_cx,
    ) else {
        log_refuse("deferred");
        return "deferred";
    };
    let Some(migrate_rows) = migrate_rows_for(
        state,
        output,
        &source_workspace,
        &source_rows,
        observed,
        retained,
        fulls,
    ) else {
        log_refuse("deferred");
        return "deferred";
    };
    run_workspace_migrate(
        state,
        me,
        store,
        dir,
        fulls,
        areas,
        observed,
        retained,
        &mut hint_cx,
        output,
        &source_workspace,
        &source_rows,
        &migrate_rows,
        &target_output,
        &target_prior,
        intent,
        tick,
        &correlation,
        correlation.as_str(),
    )
}

/// Build one [`crate::workspace_owner::MigrateRow`] per source member with
/// live observations, never fabricated placeholders: `floating` rides the
/// assembled Engine row (minimized/hidden omitted rows fall back to the
/// runtime float intent plus the retained Engine exception, so a carried
/// minimized float is never downgraded to tiled), `sticky` rides the runtime
/// sticky set, `maximized` re-reads live via `IsZoomed`
/// ([`live_overlay_flags`]) for visible members and falls back to the
/// retained row for hidden/retained members, `fullscreen` re-reads live with
/// the same fallback, `fixed_auto` rides the automatic-fixed latch, and
/// `fit_excluded` rides the float exception (the only unexplained-fit
/// exclusion the Engine accepts is carried by a
/// floating/sticky/fullscreen/maximized origin). Minimized/iconic-hidden
/// members omitted from the assembled rows carry their retained normal frame
/// (`retained.rect`, else `member_rects`, else `float_rects`) with no hint so
/// the Engine keeps their slot and the native placement tail can remap them.
/// Returns `None` when any member lacks a token/rect snapshot: the caller
/// defers with retained state.
pub(super) fn migrate_rows_for(
    state: &TileLoop,
    output: &str,
    workspace: &str,
    source_rows: &[crate::workspace_owner::OwnerRow],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    fulls: &[Rect],
) -> Option<Vec<crate::workspace_owner::MigrateRow>> {
    let members = state.workspaces.workspace_members(output, workspace);
    let by_token: HashMap<&str, &crate::workspace_owner::OwnerRow> = source_rows
        .iter()
        .map(|row| (row.token.as_str(), row))
        .collect();
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed
        .iter()
        .map(|window| (window.hwnd, window))
        .collect();
    let domain_key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(output.to_owned()),
        workspace: tiler_core::directional::WorkspaceId(workspace.to_owned()),
    };
    let mut rows = Vec::with_capacity(members.len());
    for key in &members {
        let token = state.member_tokens.get(key).cloned()?;
        let sticky = state.sticky.contains_key(key);
        let hidden = state.workspaces.is_hidden(key);
        let omitted = state.minimized_omitted.contains(key);
        let (maximized, fullscreen) = if hidden || omitted {
            // Hidden/minimized members keep no live frame: carry the retained
            // overlay flags so a carried overlay is never downgraded to a
            // plain row (which would drop its slot) nor invented.
            let retained_row = retained.iter().find(|r| r.key == *key);
            match retained_row {
                Some(r) => (r.maximized, r.fullscreen),
                None => {
                    let overlay = state.overlay_now.get(&token);
                    (
                        overlay.is_some_and(|(max, _)| *max),
                        overlay.is_some_and(|(_, full)| *full),
                    )
                }
            }
        } else if by_hwnd.contains_key(&key.hwnd) {
            let (live_max, live_full_opt) = live_overlay_flags(key.hwnd, fulls);
            let fallback_full = retained
                .iter()
                .find(|r| r.key == *key)
                .map(|r| r.fullscreen)
                .or_else(|| state.overlay_now.get(&token).map(|(_, full)| *full))
                .unwrap_or(false);
            (live_max, live_full_opt.unwrap_or(fallback_full))
        } else {
            let r = retained.iter().find(|r| r.key == *key)?;
            let (live_max, live_full_opt) = live_overlay_flags(key.hwnd, fulls);
            (live_max, live_full_opt.unwrap_or(r.fullscreen))
        };
        if let Some(row) = by_token.get(token.as_str()) {
            rows.push(crate::workspace_owner::MigrateRow {
                token: token.clone(),
                rect: row.rect,
                hints: row.hints,
                floating: row.floating,
                fullscreen,
                maximized,
                sticky,
                fixed_auto: state.fixed_auto_seen.contains(key),
                fixed_suppress: false,
                fit_excluded: row.floating,
            });
            continue;
        }
        // Minimized/iconic-hidden omission: the assembled rows skip the
        // frameless non-overlay row so convergence can store the
        // `MinimizedSlot`, but migration must still carry the member (else
        // the Engine refuses `PartialObservation` and the native placement
        // tail could never commit). Carry the retained normal frame with no
        // hint; overlay flags above already preserve max/fullscreen.
        let rect = retained
            .iter()
            .find(|r| r.key == *key)
            .and_then(|r| r.rect)
            .or_else(|| state.member_rects.get(&token).copied())
            .or_else(|| state.float_rects.get(&token).copied())?;
        let floating = state.floated.contains(key)
            || state
                .engine
                .session(&domain_key)
                .is_some_and(|session| session.is_exception(&WindowId(token.clone())));
        rows.push(crate::workspace_owner::MigrateRow {
            token: token.clone(),
            rect,
            hints: WindowSizeHints::none(),
            floating,
            fullscreen,
            maximized,
            sticky,
            fixed_auto: state.fixed_auto_seen.contains(key),
            fixed_suppress: false,
            fit_excluded: floating,
        });
    }
    Some(rows)
}
