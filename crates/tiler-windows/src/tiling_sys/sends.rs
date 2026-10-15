use super::*;

/// Send the focused window to a same-output workspace, then follow or stay.
/// Tiled-to-tiled sends run the retained Engine `MoveToWorkspace` route with
/// source reflow; any boundary touching a floating workspace transfers
/// project native membership instead (no two-domain Engine plan, no
/// floating-side geometry) and reflows only the tiled side through the
/// ordinary follow select. `target` resolves once before transfer (numbered
/// ordinal/trailing, or relative ring step from the mover source); `follow`
/// pins the explicit intent (CLI sends follow). Follow selects/reveals the
/// target and focuses the mover; stay hides the mover without selecting or
/// revealing the target, keeps the source selected/visible, applies the
/// source-bound plan focus (MRU, none for null) and reflows only writable
/// source rows. Refuses unmanaged focus, no-op/foreign transfers, and proof
/// modes without workspace hides.
///
/// Source reflow consumes the existing `SendWorkspace` plan while the source
/// survivors are still visible and eligible in the same action (scoped by the
/// source writable set; hidden/retained rows never take writes). The follow
/// select then reveals the target, establishes mover focus before geometry
/// with all fences retained, and reconciles the destination in the same
/// action. The select's fresh focus/observation is reused: no redundant
/// post-follow enumeration. Real fullscreen/unreadable/identity/incomplete
/// observation continues to block writes honestly; `ok` never implies
/// applied.
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace_do_send(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &mut [ObservedWindow],
    retained: &[RetainedRow],
    output: &str,
    target: SendTarget,
    follow: bool,
    mover_hwnd: u64,
    origin_token: &str,
    origin_pid: u32,
    origin_creation: &str,
    ctx: &ActionCtx,
) -> SendEffect {
    let fail = |outcome: &'static str| SendEffect {
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
        source_workspace: "ws?".to_owned(),
        target_workspace: "ws?".to_owned(),
    };
    let mover_key = state
        .member_tokens
        .iter()
        .find(|(_, t)| t.as_str() == origin_token)
        .map(|(k, _)| k.clone());
    let Some(mover_key) = mover_key else {
        return fail("unmanaged");
    };
    // Full chord-time binding: token plus HWND, PID, and process creation.
    // A recycled HWND or a retargeted token never dispatches.
    if !crate::workspace_owner::member_matches(&mover_key, mover_hwnd, origin_pid, origin_creation)
    {
        return fail("foreground-changed");
    }
    let Some(loc) = state.workspaces.member_loc(&mover_key).cloned() else {
        return fail("unmanaged");
    };
    if loc.output != output || state.workspaces.is_hidden(&mover_key) {
        return fail("unmanaged");
    }
    // Floats never send as movers on the Engine route; floating survivors
    // ride the carried rows. Sticky rides the same slotless float plus an
    // explicit subject gate so a pruned-domain sticky still refuses.
    // Floating-source boundary sends skip this refusal (eligibility derives
    // from the actual source below): their movers ride the native route.
    // D16 ordinary intentional floats and approved floating+fullscreen /
    // floating+maximized combos send tiled-to-tiled through the shared
    // slotless deferred path; sticky and automatic-fixed floats keep
    // refusing here.
    if state.sticky.contains_key(&mover_key) {
        return fail("send-refused-sticky");
    }
    if state.workspaces.is_tiled(&loc.output, &loc.workspace)
        && engine_is_float(state, &loc.output, &loc.workspace, origin_token)
        && !float_send_admits(state, &mover_key, &loc.output, &loc.workspace, origin_token)
    {
        return fail("send-refused-floating");
    }
    // Explicit scope fences every send before the Engine mutation: an
    // out-of-scope mover reports without touching Engine sessions,
    // membership, or layout. The hide path re-fences independently.
    if let Some(stored) = state.member_identity.get(&mover_key) {
        if !scope_allows(&state.scope, &stored.exe_path) {
            return fail("scope-excluded");
        }
        // Listed hosts send only with a live matching hosted child: a newly
        // appearing hosted app never dispatches under another app's
        // membership. The hide path re-fences fresh independently.
        if !hosted_gate_allows(&stored.exe_path, mover_hwnd, stored.pid, &state.scope_hosts) {
            return fail("scope-excluded");
        }
    }
    // Visible lifetime gate: the live member tag must equal the stored tag,
    // so a same-process HWND reuse (same HWND/PID/creation, fresh window)
    // dispatches nothing. The stale membership drops here; the next tick
    // re-admits the live window as brand new with no inheritance.
    let live_tag = live_member_tag(mover_hwnd);
    let lifetime_ok = state.member_tags.get(&mover_key).is_some_and(|stored| {
        crate::workspace_owner::visible_lifetime_ok(stored, live_tag.as_deref())
    });
    if !lifetime_ok {
        drop_member_state(state, &mover_key);
        return fail("identity-changed");
    }
    if state.allowlist.is_some() && !state.workspace_proof {
        return fail("workspace-disabled");
    }
    let target_id =
        match scope_sends::resolve_scoped_send_target(state, output, &loc.workspace, target) {
            // Item 12 stage B: numbered/relative/trailing resolve over the
            // adopted scoped ring (local delegates to the workspace resolvers,
            // so the local path is unchanged); shared trailing creation mints
            // one logical id across all output domains.
            scope_sends::ScopedSendTarget::Id(id) => id,
            scope_sends::ScopedSendTarget::Unknown(outcome) => {
                return fail(outcome);
            }
        };
    if target_id == loc.workspace {
        return fail("no-op");
    }
    // Shared convergence before tokens/fences: outputs lacking the logical
    // target id gain an empty entry so the transfer below preserves the
    // window's output-bound domain instead of refusing.
    scope_sends::ensure_shared_send_entry(state, output, &target_id);
    let source_token = state.workspaces.workspace_token(output, &loc.workspace);
    let target_token = state.workspaces.workspace_token(output, &target_id);
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
        source_workspace: source_token.clone(),
        target_workspace: target_token.clone(),
    };
    // Cross-boundary route from the actual workspace modes: tiled-to-tiled
    // runs the shared two-domain Engine plan below; any floating boundary
    // transfers project native membership instead with no plan and no
    // floating-side geometry, reflows only the tiled side, and follows only
    // after the existing exact membership confirmation.
    let source_tiled = state.workspaces.is_tiled(output, &loc.workspace);
    let target_tiled = state.workspaces.is_tiled(output, &target_id);
    if crate::workspace_owner::send_route(source_tiled, target_tiled)
        == crate::workspace_owner::SendRoute::Native
    {
        return workspace_do_send_native(
            state,
            me,
            store,
            dir,
            fulls,
            areas,
            observed,
            retained,
            output,
            &target_id,
            &mover_key,
            mover_hwnd,
            &source_token,
            &target_token,
            source_tiled,
            target_tiled,
            ctx,
            follow,
        );
    }
    // Full source+target observations including hidden snapshots, so the
    // planned mutation reuses topology instead of remove/reseed. Either side
    // incomplete defers with retained state, never a falsely complete pair.
    // One shared hint-query budget across both assemblies: source and target
    // never independently blow the per-operation bound.
    //
    // Pre-dispatch overlay gate (item 9): a retained maximized/fullscreen
    // member is retained, never eligible, so it must read its overlay flags
    // live (not the retained row) before the Engine plans; a stable
    // maximized/fullscreen snapshot carries without restoring, with
    // retained-row fallback for unreadable frames, never a fabricated value.
    // Observed movers ride the ordinary (false, false) expectation from
    // observation, except D16 admitted floats, which bind their exact live
    // overlay snapshot from `overlay_now` (see below); the post-plan recheck
    // below gates any live race before effects. A tiled maximized/fullscreen member sends (carry, never
    // restore); a newly fullscreened ordinary mover refuses with no writes.
    let mover_observed = observed.iter().any(|w| w.hwnd == mover_hwnd);
    let (snap_overlay, fallback_fullscreen) = if mover_observed {
        if engine_is_float(state, &loc.output, &loc.workspace, origin_token) {
            // D16 admitted float (the dispatch gate already refused sticky,
            // automatic-fixed, and dual-overlay shapes): bind the exact live
            // overlay snapshot so the post-plan fences compare like-for-like.
            // A tiled (false, false) snapshot here would defer or refuse
            // every combo natively. The snapshot fullscreen doubles as the
            // unreadable-frame fallback, like the retained row below.
            match state.overlay_now.get(origin_token) {
                Some((maximized, fullscreen)) => ((*maximized, *fullscreen), *fullscreen),
                None => return fail_at("deferred"),
            }
        } else {
            ((false, false), false)
        }
    } else {
        let Some(row) = retained.iter().find(|r| r.key == mover_key) else {
            return fail_at("origin-vanished");
        };
        // Actual KDE wrapper behavior (workspace-send `non-tiled-focus`):
        // overlay movers never send as ordinary.
        if !crate::workspace_owner::send_retained_overlay_admits(row.maximized, row.fullscreen) {
            return fail_at("origin-vanished");
        }
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
        ((row.maximized, row.fullscreen), row.fullscreen)
    };
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
    let mut hint_cx = HintCx::new();
    let Some(source_rows) = assemble_domain_rows(
        state,
        output,
        &loc.workspace,
        observed,
        retained,
        "send",
        ctx.correlation.as_str(),
        &mut hint_cx,
    ) else {
        return fail_at("deferred");
    };
    let Some(target_rows) = assemble_domain_rows(
        state,
        output,
        &target_id,
        observed,
        retained,
        "send",
        ctx.correlation.as_str(),
        &mut hint_cx,
    ) else {
        return fail_at("deferred");
    };
    if !source_rows.iter().any(|r| r.token == origin_token) {
        return fail_at("unmanaged");
    }
    let Some((source_domain, source_key)) = workspace_domain_for(
        output,
        &loc.workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    ) else {
        return fail_at("unknown-output");
    };
    let Some((target_domain, target_key)) =
        workspace_domain_for(output, &target_id, areas, state.inner_gap, state.outer_gap)
    else {
        return fail_at("unknown-output");
    };
    let fingerprint = {
        let mut pairs: Vec<(String, Rect)> = source_rows
            .iter()
            .chain(target_rows.iter())
            .map(|r| (r.token.clone(), r.rect))
            .collect();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        fingerprint(
            &pairs
                .iter()
                .map(|(t, r)| (t.clone(), *r))
                .collect::<Vec<_>>(),
        )
    };
    let action_correlation =
        CorrelationId::parse(&ctx.correlation).expect("action correlation is a valid token");
    let revision = revision_for(state, output, &loc.workspace);
    // Live gap edits adopt on both domains first, so the pair convergence
    // below never straddles gap generations (an unseen target would
    // otherwise seed fresh with new gaps beside a retained stale source).
    {
        let outer_gap = state.outer_gap;
        let mover = WindowId(origin_token.to_owned());
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
        adopt_gaps_for_route(
            state,
            &source_domain,
            &source_key,
            outer_gap,
            &source_windows,
            Some(&mover),
            revision,
            source_fp,
            &action_correlation,
        );
        let target_windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = target_rows
            .iter()
            .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
            .collect();
        let target_fp = crate::tiling::fingerprint(
            &target_rows
                .iter()
                .map(|r| (r.token.clone(), r.rect))
                .collect::<Vec<_>>(),
        );
        let target_revision = revision_for(state, output, &target_id);
        adopt_gaps_for_route(
            state,
            &target_domain,
            &target_key,
            outer_gap,
            &target_windows,
            None,
            target_revision,
            target_fp,
            &action_correlation,
        );
    }
    let Some(mut event) = crate::workspace_owner::build_send_event(
        &state.owner,
        &state.generation,
        &action_correlation,
        revision,
        fingerprint,
        (source_domain, source_key.clone()),
        (target_domain, target_key.clone()),
        &source_rows,
        &target_rows,
        origin_token,
        state.outer_gap,
        follow,
    ) else {
        return fail_at("refused");
    };
    crate::workspace_owner::stamp_send_target(&mut event, &target_key);
    // D16 combo wire: the focused float mover's exact live maximized flag
    // rides its own send rows so the shared deferred arm binds the approved
    // floating+maximized shape. Fullscreen already rides the fixed-admission
    // stamp for every row; tiled maximized movers and every survivor keep
    // the builder defaults. No other encoding changes.
    {
        let floating = source_rows
            .iter()
            .find(|r| r.token == origin_token)
            .is_some_and(|r| r.floating);
        let maximized = state
            .overlay_now
            .get(origin_token)
            .is_some_and(|(max, _)| *max);
        crate::workspace_owner::stamp_send_float_overlay(
            &mut event,
            origin_token,
            floating,
            maximized,
        );
    }
    let source_plan_start = Instant::now();
    let reply = handle_engine_event(state, &mut event);
    log_engine_placement_trace(
        state,
        ctx.tick,
        ctx.correlation.as_str(),
        "send-to-workspace",
    );
    let source_plan_ms = source_plan_start
        .elapsed()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    let tiler_core::boundary::CoreReply::SendWorkspace(plan) = &reply else {
        let outcome: &'static str = match reply {
            tiler_core::boundary::CoreReply::Rejected { kind, .. } => kind,
            tiler_core::boundary::CoreReply::Diverged(reason) => reason.as_str(),
            tiler_core::boundary::CoreReply::SnapshotInvalid { detail, .. } => detail,
            _ => "refused",
        };
        return fail_at(outcome);
    };
    // Project-owned membership change after revalidation and the planned
    // Engine mutation: exact identity table update, never HWND alone. A
    // tiled maximized/fullscreen member sends too (KDE parity): it is
    // retained rather than eligible, so it resolves through its retained row
    // with a live flag recheck (not the retained row) instead of the eligible
    // observation. Its target allocation is kept by the plan while overlay
    // geometry never writes.
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let Some(stored) = state.member_identity.get(&mover_key).cloned() else {
        return fail_at("unmanaged");
    };
    // Item 9 post-plan recheck with live OS reads (not the retained row):
    // a newly arrived fullscreen refuses, other drift defers. Before any
    // membership change or native effect, for follow and stay alike.
    {
        let (live_max, live_full_opt) = live_overlay_flags(mover_hwnd, fulls);
        if let Err(outcome) = crate::workspace_owner::send_overlay_gate(
            preflight.fences.overlay_maximized,
            preflight.fences.overlay_fullscreen,
            live_max,
            live_full_opt,
            preflight.fallback_fullscreen,
        ) {
            return fail_at(outcome);
        }
    }
    let mover_minimized = if let Some(fresh) = by_hwnd.get(&mover_hwnd) {
        if !crate::workspace_owner::member_matches(
            &mover_key,
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
    } else if let Some(row) = retained.iter().find(|r| r.key == mover_key) {
        // Retained maximized/fullscreen mover: the live overlay gate above
        // already refused a newly arrived fullscreen and deferred drift; only
        // a maximized/fullscreen member proceeds, with no restore and no
        // remaximize (overlay carry).
        if !crate::workspace_owner::send_retained_overlay_admits(row.maximized, row.fullscreen) {
            return fail_at("origin-vanished");
        }
        if stored.pid != mover_key.pid || stored.process_creation != mover_key.creation {
            return fail_at("origin-vanished");
        }
        false
    } else {
        return fail_at("origin-vanished");
    };
    // Post-Engine lifetime recheck: a same-process reuse across the in-memory
    // mutation still refuses before any membership change or hide.
    let post_tag = live_member_tag(mover_hwnd);
    if !state
        .member_tags
        .get(&mover_key)
        .is_some_and(|tag| crate::workspace_owner::visible_lifetime_ok(tag, post_tag.as_deref()))
    {
        return fail_at("identity-changed");
    }
    if !state
        .workspaces
        .assign(mover_key.clone(), output, &target_id, true)
    {
        return fail_at("refused");
    }
    // Verified transfer before follow/stay: absent source, present target.
    let source_now = state.workspaces.workspace_members(output, &loc.workspace);
    let target_now = state.workspaces.workspace_members(output, &target_id);
    if !crate::workspace_owner::verify_membership_transfer(&mover_key, &source_now, &target_now) {
        return fail_at("unverified");
    }
    // Carried pre-plan fences before source writes for both intents
    // (item 2 + item 20): view/mode/gaps/scope/lifetime/membership/overlay
    // re-verified live; any drift refuses with no further effects and no
    // replay. A lifetime refusal drops the stale membership like the
    // dispatch-time gate.
    if let Some(outcome) = crate::workspace_owner::send_fences_hold(
        preflight.fences,
        preflight.live(
            state,
            &stored,
            &mover_key,
            mover_hwnd,
            output,
            &loc.workspace,
            &target_id,
            fulls,
        ),
    ) {
        if outcome == "identity-changed" {
            drop_member_state(state, &mover_key);
        }
        return fail_at(outcome);
    }
    // The Engine echoes the explicit intent: follow consumes the source plan
    // while survivors are visible, then selects/reveals the target; stay
    // fences first, then reflows only writable source rows, hides without
    // selecting, and applies the source-bound plan focus (never the target).
    // D16 slotless deferred transfers are membership-only: no fabricated
    // leaf, no mover geometry, no source/target geometry application
    // (including the stay source-only tail), and no maximize restore or
    // fullscreen toggles. Membership, fences, identity/scope/lifetime,
    // hide/reveal, and follow/stay routing stay unchanged.
    let deferred = matches!(
        plan.operation,
        tiler_core::contract::LifecycleOperation::MoveDeferred { .. }
    );
    if !plan.follow {
        return workspace_send_stay(
            state,
            me,
            store,
            dir,
            fulls,
            observed,
            retained,
            &mover_key,
            &stored,
            mover_minimized,
            mover_hwnd,
            output,
            &loc.workspace,
            &target_id,
            &source_token,
            &target_token,
            ctx,
            plan,
            preflight,
            source_plan_ms,
            deferred,
        );
    }
    // D16 deferred follow is membership-only too: the slotless mover takes
    // no allocation on either tree, so the source plan never applies. The
    // shared follow tail still hides the mover and selects/reveals the
    // target with its own fences.
    if deferred {
        return workspace_send_follow(
            state,
            me,
            store,
            dir,
            fulls,
            areas,
            observed,
            &mover_key,
            &stored,
            mover_minimized,
            output,
            &target_id,
            &source_token,
            &target_token,
            ctx,
            None,
            0,
            source_plan_ms,
            preflight,
            true,
        );
    }
    // Consume the existing source plan while the survivors are still visible
    // and eligible in the same action. The writable set scopes to the source
    // domain (mover already transferred out, hidden/retained rows never
    // writable), so target entries skip honestly as retained and hidden
    // geometry is never written. All write fences stay inside
    // `apply_geometry` (identity/lifetime/scope, fullscreen veto, readback).
    // The internal tick reuses the parent action correlation.
    let source_writable = writable_tokens(state, output, &loc.workspace, observed);
    state.tick += 1;
    let source_tick = state.tick;
    let source_start = Instant::now();
    let source_summary = apply_geometry(
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
            writable: &source_writable,
            output_token: state.workspaces.output_token(output),
            workspace_token: state.workspaces.workspace_token(output, &loc.workspace),
            revision: revision_for(state, output, &loc.workspace),
        },
    );
    let source_ms = source_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let source = source_summary;
    // Hide the mover from the source view (commit-before-hide), then follow
    // by selecting the target (which reveals it). The select reuses the mover
    // as its focus hint and returns its fresh focus/observation: no redundant
    // post-follow enumeration here. A committed claim is owned even on
    // uncertain post-hide readback: flag it hidden and report the stall
    // without pretending follow success.
    workspace_send_follow(
        state,
        me,
        store,
        dir,
        fulls,
        areas,
        observed,
        &mover_key,
        &stored,
        mover_minimized,
        output,
        &target_id,
        &source_token,
        &target_token,
        ctx,
        source,
        source_ms,
        source_plan_ms,
        preflight,
        deferred,
    )
}
