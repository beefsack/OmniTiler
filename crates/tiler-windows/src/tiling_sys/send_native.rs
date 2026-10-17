#![forbid(unsafe_code)]
use super::*;

/// Native cross-boundary send for any boundary touching a floating
/// workspace: transfer project native membership (verified exactly like the
/// Engine route), reflow the tiled source survivors when the source is tiled,
/// then follow through the shared tail so the target reveals and reconciles
/// when tiled (`follow=true`), or hide the mover and keep the source view
/// with its existing native boundary focus (`follow=false`, no target
/// selection, no history record). No two-domain Engine plan, no
/// floating-side geometry; the tiled target admits normally through the
/// ordinary follow select. Sticky movers never transfer as a
/// single-desktop write; intentional per-window float movers ride along only
/// from an actually floating source.
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace_do_send_native(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &mut [ObservedWindow],
    retained: &[RetainedRow],
    output: &str,
    target_id: &str,
    mover_key: &crate::workspace::WindowKey,
    mover_hwnd: u64,
    source_token: &str,
    target_token: &str,
    source_tiled: bool,
    target_tiled: bool,
    ctx: &ActionCtx,
    follow: bool,
) -> SendEffect {
    let fail_at = |outcome: &'static str| SendEffect {
        outcome,
        focus: "none",
        source: None,
        target: None,
        transition_ms: 0,
        observation_ms: 0,
        source_geometry_ms: 0,
        target_geometry_ms: 0,
        hide_ms: 0,
        reveal_ms: 0,
        focus_ms: 0,
        source_plan_ms: 0,
        target_plan_ms: 0,
        source_workspace: source_token.to_owned(),
        target_workspace: target_token.to_owned(),
    };
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let Some(stored) = state.member_identity.get(mover_key).cloned() else {
        return fail_at("unmanaged");
    };
    let source_workspace = state
        .workspaces
        .member_loc(mover_key)
        .map(|loc| loc.workspace.clone())
        .unwrap_or_default();
    let mut snap_overlay = (false, false);
    let mut fallback_fullscreen = false;
    let mover_minimized = if let Some(fresh) = by_hwnd.get(&mover_hwnd) {
        if !crate::workspace_owner::member_matches(
            mover_key,
            fresh.hwnd,
            fresh.identity.pid,
            &fresh.identity.process_creation,
        ) || fresh.identity.exe_path != stored.exe_path
            || fresh.identity.user_sid != stored.user_sid
            || fresh.identity.session_id != stored.session_id
        {
            return fail_at("origin-vanished");
        }
        fresh.facts.minimized
    } else if let Some(row) = retained.iter().find(|r| r.key == *mover_key) {
        if !crate::workspace_owner::send_retained_overlay_admits(row.maximized, row.fullscreen) {
            return fail_at("origin-vanished");
        }
        // Item 9 live snapshot (not the retained row): a stable
        // maximized/fullscreen snapshot carries without restoring; a newly
        // arrived live fullscreen refuses outright, other drift defers, and
        // an unreadable live flag defers a fullscreen snapshot (never trust
        // the fallback before effects). Maximized/fullscreen carry itself
        // never restores.
        let (live_max, live_full_opt) = live_overlay_flags(mover_hwnd, fulls);
        if let Err(outcome) = crate::workspace_owner::send_overlay_gate(
            row.maximized,
            row.fullscreen,
            live_max,
            live_full_opt,
            row.fullscreen,
        ) {
            return fail_at(outcome);
        }
        snap_overlay = (row.maximized, row.fullscreen);
        fallback_fullscreen = row.fullscreen;
        if stored.pid != mover_key.pid || stored.process_creation != mover_key.creation {
            return fail_at("origin-vanished");
        }
        false
    } else {
        return fail_at("origin-vanished");
    };
    let post_tag = crate::product_hide::sys::read_member_tag(mover_hwnd);
    if !state
        .member_tags
        .get(mover_key)
        .is_some_and(|tag| crate::workspace_owner::visible_lifetime_ok(tag, post_tag.as_deref()))
    {
        return fail_at("identity-changed");
    }
    // Item 9 pre-transfer overlay gate with live reads (mirrors the Engine
    // route's pre-assign check): an observed mover that went borderless
    // fullscreen live refuses here with NO membership change; other drift
    // from the snapshot defers the same way. The post-transfer recheck below
    // stays for races across the transfer itself.
    {
        let (live_max, live_full_opt) = live_overlay_flags(mover_hwnd, fulls);
        if let Err(outcome) = crate::workspace_owner::send_overlay_gate(
            snap_overlay.0,
            snap_overlay.1,
            live_max,
            live_full_opt,
            fallback_fullscreen,
        ) {
            return fail_at(outcome);
        }
    }
    if !state
        .workspaces
        .assign(mover_key.clone(), output, target_id, true)
    {
        return fail_at("refused");
    }
    let source_now = state
        .workspaces
        .workspace_members(output, &source_workspace);
    let target_now = state.workspaces.workspace_members(output, target_id);
    if !crate::workspace_owner::verify_membership_transfer(mover_key, &source_now, &target_now) {
        return fail_at("unverified");
    }
    let preflight = SendPreflight {
        fences: crate::workspace_owner::SendFenceSnapshot {
            inner_gap: state.inner_gap,
            outer_gap: state.outer_gap,
            source_tiled,
            target_tiled,
            overlay_maximized: snap_overlay.0,
            overlay_fullscreen: snap_overlay.1,
        },
        fallback_fullscreen,
    };
    // Item 20 pre-effect recheck with live reads: refuse fullscreen, defer
    // drift, before any reflow/hide/focus.
    if let Err(outcome) = preflight.overlay_hold(mover_hwnd, fulls) {
        return fail_at(outcome);
    }
    // Tiled-source survivor reflow before hide/follow: the transfer above
    // already moved the mover out, so a complete reconcile on current
    // observed geometry converges the survivors (no layout hole left in the
    // retained Engine session). No two-domain plan, no floating-side writes:
    // floating sources skip entirely (frames untouched), and the follow
    // select admits the target normally below. Best-effort: an incomplete
    // source observation skips the reflow and the follow still runs, with
    // convergence on the next return select.
    if source_tiled {
        let mut hint_cx = HintCx::new();
        if let Some(source_rows) = assemble_domain_rows(
            state,
            output,
            &source_workspace,
            observed,
            retained,
            "send-source",
            ctx.correlation.as_str(),
            &mut hint_cx,
        ) && let Some((source_domain, source_key)) = workspace_domain_for(
            output,
            &source_workspace,
            areas,
            state.inner_gap,
            state.outer_gap,
        ) {
            let source_windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = source_rows
                .iter()
                .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
                .collect();
            let source_fp = fingerprint(
                &source_rows
                    .iter()
                    .map(|r| (r.token.clone(), r.rect))
                    .collect::<Vec<_>>(),
            );
            let focused = state
                .focused_token(observed)
                .filter(|f| source_windows.iter().any(|(w, _, _, _)| w == f));
            let revision = revision_for(state, output, &source_workspace);
            let action_correlation = CorrelationId::parse(&ctx.correlation)
                .expect("action correlation is a valid token");
            adopt_gaps_for_route(
                state,
                &source_domain,
                &source_key,
                state.outer_gap,
                &source_windows,
                focused.as_ref(),
                revision,
                source_fp,
                &action_correlation,
            );
            let mut event = crate::tiling::build_reconcile_event_for_floating(
                &state.owner,
                &state.generation,
                &action_correlation,
                revision,
                source_fp,
                &source_domain,
                &source_key,
                state.outer_gap,
                &source_windows,
                focused.as_ref(),
            );
            let reply = handle_engine_event(state, &mut event);
            let writable = writable_tokens(state, output, &source_workspace, observed);
            state.tick += 1;
            let source_tick = state.tick;
            apply_geometry(
                state,
                ApplyInput {
                    me,
                    fulls,
                    reply: &reply,
                    observed,
                    op: "send-source",
                    tick: source_tick,
                    correlation: ctx.correlation.as_str(),
                    skipped: Vec::new(),
                    writable: &writable,
                    output_token: state.workspaces.output_token(output),
                    workspace_token: state.workspaces.workspace_token(output, &source_workspace),
                    revision: revision_for(state, output, &source_workspace),
                },
            );
        }
    }
    let effect = if follow {
        workspace_send_follow(
            state,
            me,
            store,
            dir,
            fulls,
            areas,
            observed,
            mover_key,
            &stored,
            mover_minimized,
            output,
            target_id,
            source_token,
            target_token,
            ctx,
            None,
            0,
            0,
            preflight,
            false,
        )
    } else {
        workspace_send_stay_native(
            state,
            me,
            store,
            dir,
            fulls,
            mover_key,
            &stored,
            mover_minimized,
            mover_hwnd,
            output,
            &source_workspace,
            target_id,
            source_token,
            target_token,
            preflight,
            ctx,
        )
    };
    log_json_at(
        &state.log_path,
        serde_json::json!({
            "event": "workspace-send-native",
            "source_tiled": source_tiled,
            "target_tiled": target_tiled,
            "follow": follow,
            "outcome": effect.outcome,
        }),
    );
    effect
}

/// Native stay tail for a floating-boundary send: the mover is already
/// transferred (hidden membership) and the tiled source already reflowed by
/// the caller. Re-verifies the carried fences before hide (item 2 + item 20),
/// then hides the mover from the source view, keeps the source selected and
/// visible with its existing native boundary focus (no target selection, no
/// focus setter, no history record), then runs ordinary trailing lifecycle
/// for the next spare. Floating frames stay untouched; only the tiled side
/// reflowed. Partial progress never replays: a failed hide reports without
/// focus or reveal attempts.
#[allow(clippy::too_many_arguments)]
fn workspace_send_stay_native(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    mover_key: &crate::workspace::WindowKey,
    stored: &ProcessIdentity,
    mover_minimized: bool,
    mover_hwnd: u64,
    output: &str,
    source_id: &str,
    target_id: &str,
    source_token: &str,
    target_token: &str,
    preflight: SendPreflight,
    ctx: &ActionCtx,
) -> SendEffect {
    // Carried pre-transfer fences before hide (item 2 + item 20). The source
    // id rides the caller (membership already points at the target).
    if let Some(outcome) = crate::workspace_owner::send_fences_hold(
        preflight.fences,
        preflight.live(
            state, stored, mover_key, mover_hwnd, output, source_id, target_id, fulls,
        ),
    ) {
        if outcome == "identity-changed" {
            drop_member_state(state, mover_key);
        }
        return SendEffect {
            outcome,
            focus: "none",
            source: None,
            target: None,
            transition_ms: 0,
            observation_ms: 0,
            source_geometry_ms: 0,
            target_geometry_ms: 0,
            hide_ms: 0,
            reveal_ms: 0,
            focus_ms: 0,
            source_plan_ms: 0,
            target_plan_ms: 0,
            source_workspace: source_token.to_owned(),
            target_workspace: target_token.to_owned(),
        };
    }
    let mut clock = action_clock();
    let hide_done = hide_phase(
        state,
        me,
        store,
        dir,
        ctx,
        vec![HideAttempt {
            key: mover_key.clone(),
            expected: stored.clone(),
            iconic: mover_minimized,
        }],
        &mut clock,
    );
    log_visibility_timing(
        state,
        ctx,
        "send-hide",
        &hide_done.breakdown,
        &PhaseBreakdown::default(),
    );
    let (hide_outcome, _hide_detail) = match hide_done.results.first() {
        Some(result) => (result.outcome, result.detail),
        None => ("uncertain", None),
    };
    if state.hidden_claims.contains_key(mover_key) {
        state.workspaces.set_hidden(mover_key, true);
    }
    if hide_outcome != "hidden" {
        return SendEffect {
            outcome: hide_outcome,
            focus: "none",
            source: None,
            target: None,
            transition_ms: 0,
            observation_ms: 0,
            source_geometry_ms: 0,
            target_geometry_ms: 0,
            hide_ms: 0,
            reveal_ms: 0,
            focus_ms: 0,
            source_plan_ms: 0,
            target_plan_ms: 0,
            source_workspace: source_token.to_owned(),
            target_workspace: target_token.to_owned(),
        };
    }
    // Ordinary lifecycle supplies the next spare when the transfer filled the
    // trailing empty; the source view stays selected throughout.
    // Item 12 stage B: scoped trailing spare.
    scope_sends::trailing_spare_scoped(state, output);
    SendEffect {
        outcome: "ok",
        focus: "none",
        source: None,
        target: None,
        transition_ms: 0,
        observation_ms: 0,
        source_geometry_ms: 0,
        target_geometry_ms: 0,
        hide_ms: 0,
        reveal_ms: 0,
        focus_ms: 0,
        source_plan_ms: 0,
        target_plan_ms: 0,
        source_workspace: source_token.to_owned(),
        target_workspace: target_token.to_owned(),
    }
}
