use super::*;

/// Resolve one item 5 explicit output-send target through the shared
/// window-based FULL-rectangle selector, pinning the destination output's
/// CURRENT workspace once. All four directions select; explicit sends never
/// wrap and never stay local (`selected == source` is a no-op, never a
/// same-output send). `Ok(None)` is a no-op (no candidate); `Err` refuses
/// (unreadable topology, unknown destination view). Same workspace ids on
/// different outputs are valid; floating destinations stay `Some` so the
/// caller routes the membership-only boundary path instead of refusing.
pub(super) fn resolve_output_send_target(
    state: &TileLoop,
    areas: &[MonitorArea],
    source_output: &str,
    direction: tiler_core::directional::Direction,
    mover_rect: &Rect,
) -> std::result::Result<Option<(String, String)>, &'static str> {
    let outputs = directional_outputs_from_areas(areas);
    let selected = crate::workspace_owner::select_directional_target(
        direction,
        source_output,
        mover_rect,
        &outputs,
    )
    .map_err(|_| "unreadable-topology")?;
    let Some(selected) = selected else {
        return Ok(None);
    };
    if selected == source_output {
        return Ok(None);
    }
    let Some(target_workspace) = state.workspaces.active_id(&selected) else {
        return Err("unknown-output");
    };
    Ok(Some((selected, target_workspace)))
}

/// Output-send fence evaluation with the D16 deferred carry-through: a
/// stable fullscreen snapshot on a D16-admitted float mover lifts the
/// fullscreen refusal (the deferred arm transfers membership-only with no
/// geometry, and follow focuses the verified mover exactly like the
/// same-output route); every other drift still refuses,
/// and a non-deferred plan still verifies arrival. Pure wrapper over
/// [`crate::workspace_owner::output_send_fences_hold`]: the caller passes
/// the pinned snapshot, the fresh live readings, and the pinned mover
/// identity the admission check already approved.
fn output_send_fence_outcome(
    state: &TileLoop,
    snapshot: &crate::workspace_owner::DirectionalFenceSnapshot,
    live: &crate::workspace_owner::DirectionalFenceLive,
    member_key: &crate::workspace::WindowKey,
    from_token: &str,
) -> Option<&'static str> {
    let outcome = crate::workspace_owner::output_send_fences_hold(snapshot, live);
    if outcome != Some("move-refused-fullscreen") {
        return outcome;
    }
    let stable_fullscreen_carry = snapshot.overlay_fullscreen
        && live.overlay_fullscreen
        && live.overlay_maximized == snapshot.overlay_maximized
        && engine_is_float(
            state,
            &snapshot.source_output,
            &snapshot.source_workspace,
            from_token,
        )
        && float_send_admits(
            state,
            member_key,
            &snapshot.source_output,
            &snapshot.source_workspace,
            from_token,
        );
    if stable_fullscreen_carry {
        None
    } else {
        outcome
    }
}

/// Run one resolved explicit output send through the commit-last native
/// transfer (REQ-OUT-04, decisions 5.3/5.4).
///
/// The caller already resolved `(target_output, target_workspace)` as the
/// destination current view and pinned the source `home`; both stay frozen
/// through transfer, follow, and stay (never re-ranked off the relocated
/// mover). This assembles the target current-view rows, dispatches the
/// DISTINCT `SendToOutput` command on a candidate Engine clone (ordinary
/// remembered-leaf/destination-history/root admission, never R4 edge
/// landing), then actuates commit-last exactly like the directional
/// foundation: fresh topology, one combined apply, its readback, and the
/// arrival proof all verify before project membership and the authoritative
/// Engine publish once. Floating boundaries route the membership-only native
/// path (no two-domain plan, mover frame preserved, only tiled sides
/// reflow). Follow focuses the verified mover once; stay focuses the source
/// MRU (null takes no setter) while the target current view keeps the mover
/// visible. Failures stop narrowly with no replay and no guessed rollback;
/// the retained Engine plan stands and the next observation converges. A
/// focus failure after a committed verified transfer reports the focus
/// outcome with the transfer retained, matching foundation semantics.
/// Returns the workspace-output outcome token and logs the full action line.
#[allow(clippy::too_many_arguments)]
pub(super) fn run_output_send(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    hint_cx: &mut HintCx,
    home: &crate::workspace::MemberLoc,
    member_key: &crate::workspace::WindowKey,
    from_token: &str,
    source_rows: &[crate::workspace_owner::OwnerRow],
    target_output: &str,
    target_workspace: &str,
    intent: &crate::snapkey::QueuedWorkspaceOutputIntent,
    origin: &crate::snapkey::SnapOrigin,
    tick: u64,
    correlation: &CorrelationId,
    correlation_str: &str,
) -> &'static str {
    let log_path = state.log_path.clone();
    let follow = intent.follow;
    let log_send = |outcome: &'static str, extra: serde_json::Value| {
        let mut line = serde_json::Map::new();
        line.insert("event".to_owned(), serde_json::json!("workspace-output"));
        line.insert("tick".to_owned(), serde_json::json!(tick));
        line.insert("correlation".to_owned(), serde_json::json!(correlation_str));
        line.insert(
            "direction".to_owned(),
            serde_json::json!(crate::snapkey::direction_name(intent.direction)),
        );
        line.insert("follow".to_owned(), serde_json::json!(follow));
        line.insert("edge".to_owned(), serde_json::json!(intent.edge.as_str()));
        line.insert("disposition".to_owned(), serde_json::json!("consumed"));
        line.insert("outcome".to_owned(), serde_json::json!(outcome));
        line.insert("origin".to_owned(), serde_json::json!(origin.token));
        if let serde_json::Value::Object(map) = extra {
            for (k, v) in map {
                line.insert(k, v);
            }
        }
        log_json_at(&log_path, serde_json::Value::Object(line));
    };
    // Target current-view rows (hidden snapshots included so the planned
    // mutation reuses topology instead of remove/reseed). Incomplete defers
    // with retained state, never a falsely complete pair.
    let Some(target_rows) = assemble_domain_rows(
        state,
        target_output,
        target_workspace,
        observed,
        retained,
        "output-send",
        correlation_str,
        hint_cx,
    ) else {
        log_send("deferred", serde_json::json!({"window": from_token}));
        return "deferred";
    };
    let Some((source_domain_base, source_key)) = workspace_domain_for(
        &home.output,
        &home.workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    ) else {
        log_send("unknown-output", serde_json::json!({"window": from_token}));
        return "unknown-output";
    };
    let Some((target_domain_base, target_key)) = workspace_domain_for(
        target_output,
        target_workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    ) else {
        log_send("unknown-output", serde_json::json!({"window": from_token}));
        return "unknown-output";
    };
    if !state.member_identity.contains_key(member_key) {
        log_send("unmanaged", serde_json::json!({"window": from_token}));
        return "unmanaged";
    }
    // Subject eligibility (decisions 5.3/5.4): sticky movers never send,
    // and per-window float movers never send unless the admitted-float
    // contract holds (`float_send_admits`: Engine exception, never sticky
    // or explicit tile override, exactly zero or one overlay from the
    // live `overlay_now` snapshot) - independent of the source workspace
    // mode. Automatic fixed-size floats admit as ordinary members. A tiled
    // member of a floating workspace stays eligible and routes
    // the native boundary path below.
    if state.sticky.contains_key(member_key) {
        log_send(
            "send-refused-sticky",
            serde_json::json!({"window": from_token}),
        );
        return "send-refused-sticky";
    }
    if engine_is_float(state, &home.output, &home.workspace, from_token)
        && !float_send_admits(state, member_key, &home.output, &home.workspace, from_token)
    {
        log_send(
            "send-refused-floating",
            serde_json::json!({"window": from_token}),
        );
        return "send-refused-floating";
    }
    // Pre-dispatch overlay snapshot (item 9, same contract as workspace
    // send): a retained maximized/fullscreen member resolves through its
    // retained row with a live flag recheck, never as an ordinary mover; an
    // observed mover rides the ordinary (false, false) expectation except
    // admitted floats, which bind their exact live overlay snapshot
    // from `overlay_now` (a tiled (false, false) snapshot here would defer
    // or refuse every combo). A tiled maximized/fullscreen member sends
    // (carry, never restore); a newly fullscreened ordinary mover refuses
    // with no writes. Unadmitted floats never reach here (refused above
    // for lack of admission).
    let mover_hwnd = member_key.hwnd;
    let mover_observed = observed.iter().any(|w| w.hwnd == mover_hwnd);
    let (snap_overlay, _fallback_fullscreen) = if mover_observed {
        if engine_is_float(state, &home.output, &home.workspace, from_token) {
            match state.overlay_now.get(from_token) {
                Some((maximized, fullscreen)) => ((*maximized, *fullscreen), *fullscreen),
                None => {
                    log_send("deferred", serde_json::json!({"window": from_token}));
                    return "deferred";
                }
            }
        } else {
            ((false, false), false)
        }
    } else {
        let Some(row) = retained.iter().find(|r| r.key == *member_key) else {
            log_send("origin-vanished", serde_json::json!({"window": from_token}));
            return "origin-vanished";
        };
        // Overlay movers never send as ordinary (KDE `non-tiled-focus`
        // parity): only a retained maximized/fullscreen member proceeds,
        // with no restore and no remaximize (overlay carry).
        if !crate::workspace_owner::send_retained_overlay_admits(row.maximized, row.fullscreen) {
            log_send("origin-vanished", serde_json::json!({"window": from_token}));
            return "origin-vanished";
        }
        let (live_max, live_full_opt) = live_overlay_flags(mover_hwnd, fulls);
        if let Err(outcome) = crate::workspace_owner::send_overlay_gate(
            row.maximized,
            row.fullscreen,
            live_max,
            live_full_opt,
            row.fullscreen,
        ) {
            log_send(outcome, serde_json::json!({"window": from_token}));
            return outcome;
        }
        ((row.maximized, row.fullscreen), row.fullscreen)
    };
    // Fresh user-safety gate before any Engine or project mutation: a
    // vetoing foreground or an elevated foreground that arrived during
    // validation authorizes no dispatch.
    if suspend_read(state, me, fulls).veto.block {
        log_send("suspended", serde_json::json!({"window": from_token}));
        return "suspended";
    }
    if foreground_elevated(me) {
        log_send(
            "elevated-foreground",
            serde_json::json!({"window": from_token}),
        );
        return "elevated-foreground";
    }
    // All fences fresh at this boundary: membership still homes on the
    // pinned source, overlay re-read live, scope/lifetime/topology/owner
    // recomputed inside `directional_live`.
    let membership_ok = state
        .workspaces
        .member_loc(member_key)
        .is_some_and(|fresh| fresh.output == home.output && fresh.workspace == home.workspace);
    let (live_max, live_full_opt) = live_overlay_flags(member_key.hwnd, fulls);
    let Some(snapshot) = directional_snapshot(
        state,
        areas,
        &home.output,
        &home.workspace,
        target_output,
        target_workspace,
        intent.direction,
        snap_overlay.0,
        snap_overlay.1,
    ) else {
        log_send("unknown-output", serde_json::json!({"window": from_token}));
        return "unknown-output";
    };
    let live = directional_live(
        state,
        areas,
        &snapshot,
        member_key,
        membership_ok,
        live_max,
        live_full_opt.unwrap_or(snap_overlay.1),
    );
    if let Some(outcome) =
        output_send_fence_outcome(state, &snapshot, &live, member_key, from_token)
    {
        if outcome == "identity-changed" {
            drop_member_state(state, member_key);
        }
        log_send(outcome, serde_json::json!({"window": from_token}));
        return outcome;
    }
    // Floating boundaries transfer project membership only (no two-domain
    // Engine plan, no floating-side geometry); the tiled side(s) reflow and
    // the mover frame is preserved. Sticky and unadmitted float subjects
    // never reach here (the tick refuses them before resolving).
    let source_tiled = state.workspaces.is_tiled(&home.output, &home.workspace);
    let target_tiled = state.workspaces.is_tiled(target_output, target_workspace);
    if crate::workspace_owner::send_route(source_tiled, target_tiled)
        == crate::workspace_owner::SendRoute::Native
    {
        return run_output_send_native(
            state,
            me,
            fulls,
            areas,
            observed,
            retained,
            home,
            member_key,
            from_token,
            source_rows,
            &target_rows,
            target_output,
            target_workspace,
            source_tiled,
            target_tiled,
            follow,
            tick,
            correlation,
            correlation_str,
            origin,
            intent,
            &snapshot,
        );
    }
    // Candidate Engine for the whole pair route: gap adopts and the send
    // dispatch below all run on the clone. The authoritative Engine publishes
    // once, only after the verified tails commit. A refused topology/write/
    // readback/arrival/assignment discards the candidate, so source
    // membership and retained Engine placement survive with no follow-focus
    // and no stale MRU mutation.
    let mut candidate = state.engine.clone();
    {
        let outer_gap = state.outer_gap;
        let mover = WindowId(from_token.to_owned());
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
        adopt_gaps_on_candidate(
            &mut candidate,
            state,
            &source_domain_base,
            &source_key,
            outer_gap,
            &source_windows,
            Some(&mover),
            revision_for(state, &home.output, &home.workspace),
            source_fp,
            correlation,
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
        adopt_gaps_on_candidate(
            &mut candidate,
            state,
            &target_domain_base,
            &target_key,
            outer_gap,
            &target_windows,
            None,
            revision_for(state, target_output, target_workspace),
            target_fp,
            correlation,
        );
    }
    // Combined two-domain observation: source rows homed on the source key,
    // target rows homed on the target key. Fingerprint covers both so stale
    // target rects cannot ride.
    let combined_fp = {
        let mut pairs: Vec<(String, Rect)> = source_rows
            .iter()
            .chain(target_rows.iter())
            .map(|r| (r.token.clone(), r.rect))
            .collect();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        crate::tiling::fingerprint(&pairs)
    };
    let from = WindowId(from_token.to_owned());
    // DISTINCT explicit output-send command: the destination output's current
    // workspace rides `target_domain`/`target_windows` with ordinary
    // admission, never the directional R4 pair route.
    let Some(mut send_event) = crate::workspace_owner::build_send_output_event(
        &state.owner,
        &state.generation,
        correlation,
        revision_for(state, &home.output, &home.workspace),
        combined_fp,
        (source_domain_base.clone(), source_key.clone()),
        (target_domain_base.clone(), target_key.clone()),
        source_rows,
        &target_rows,
        &from.0,
        state.outer_gap,
        follow,
    ) else {
        log_send("refused", serde_json::json!({"window": from_token}));
        return "refused";
    };
    crate::workspace_owner::stamp_send_output_target(&mut send_event, &target_key);
    {
        let floating = source_rows
            .iter()
            .find(|r| r.token == from.0)
            .is_some_and(|r| r.floating);
        let maximized = state
            .overlay_now
            .get(from_token)
            .is_some_and(|(max, _)| *max);
        crate::workspace_owner::stamp_send_float_overlay(
            &mut send_event,
            &from.0,
            floating,
            maximized,
        );
    }
    let mut event = send_event;
    let reply = {
        stamp_engine_event(state, &mut event);
        candidate.handle(&event)
    };
    let tiler_core::boundary::CoreReply::SendOutput(plan) = &reply else {
        let outcome: &'static str = match reply {
            tiler_core::boundary::CoreReply::Rejected { kind, .. } => kind,
            tiler_core::boundary::CoreReply::Diverged(reason) => reason.as_str(),
            tiler_core::boundary::CoreReply::SnapshotInvalid { detail, .. } => detail,
            _ => "refused",
        };
        log_send(outcome, serde_json::json!({"window": from.0}));
        return outcome;
    };
    // D16 slotless deferred transfers are membership-only: no fabricated
    // leaf, no mover geometry, no geometry application. Membership, fences,
    // and follow/stay routing stay unchanged.
    let deferred = matches!(
        plan.operation,
        tiler_core::contract::LifecycleOperation::MoveDeferred { .. }
    );
    // The pair plan sits on the discarded-on-refusal candidate, so the
    // project membership commits only after the fresh topology, the single
    // combined apply, its readback, and the arrival proof all verify. The
    // authoritative Engine publishes with that same commit, exactly once.
    // Per-window write gates (lifetime/scope/pid/veto) still run inside the
    // single apply below; a refusal keeps source membership and retained
    // Engine placement with no focus and no replay.
    let fresh_snapshot = crate::workspace_owner::drag_snapshot_for_directional(&snapshot);
    let Some(fresh_areas) = fresh_cross_areas() else {
        log_send("send-unverified", serde_json::json!({"window": from.0}));
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return "send-unverified";
    };
    if !cross_topology_fresh(&fresh_snapshot, &fresh_areas) {
        log_send("send-unverified", serde_json::json!({"window": from.0}));
        let fresh_fulls: Vec<Rect> = fresh_areas.iter().map(|a| a.full).collect();
        reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
        return "send-unverified";
    }
    let fresh_fulls: Vec<Rect> = fresh_areas.iter().map(|a| a.full).collect();
    if !deferred {
        // One union-writable combined apply: source survivors reflow and the
        // target places the arrival together, in DPI-aware physical
        // coordinates with no rescaling. A single pass means a single
        // internal observe/publish, never two mid-operation publishes. The
        // union is read pre-commit (the mover rides the source set while
        // homed on the source); per-window freshness gates inside the apply
        // still decide every write.
        let mut union_writable = writable_tokens(state, &home.output, &home.workspace, observed);
        union_writable.extend(writable_tokens(
            state,
            target_output,
            target_workspace,
            observed,
        ));
        let summary = apply_geometry(
            state,
            ApplyInput {
                me,
                fulls: &fresh_fulls,
                reply: &reply,
                observed,
                op: "send-output",
                tick,
                correlation: correlation_str,
                skipped: Vec::new(),
                writable: &union_writable,
                output_token: state.workspaces.output_token(&home.output),
                workspace_token: state
                    .workspaces
                    .workspace_token(&home.output, &home.workspace),
                revision: revision_for(state, &home.output, &home.workspace),
            },
        );
        let Some(summary) = summary else {
            log_send("send-unverified", serde_json::json!({"window": from.0}));
            return "send-unverified";
        };
        if !summary.readback_ok {
            log_send("send-unverified", serde_json::json!({"window": from.0}));
            return "send-unverified";
        }
        if summary.mismatched > 0 {
            log_send("send-mismatch", serde_json::json!({"window": from.0}));
            return "send-mismatch";
        }
        // Required arrival proof before claiming success: the mover must read
        // back at its planned rect on the destination monitor. A failed proof
        // reports unverified with no commit, no revert, and no replay; the
        // next observation converges.
        if !directional_arrival_verified(
            state,
            &fresh_areas,
            &reply,
            &from.0,
            member_key.hwnd,
            target_output,
        ) {
            log_send("send-unverified", serde_json::json!({"window": from.0}));
            return "send-unverified";
        }
    }
    // Final live fences after writes, before logical commit: suspend,
    // elevated, and the full pinned-snapshot fence set re-verified live
    // against the fresh topology (deferred transfers skip writes but keep
    // this gate). Any drift refuses with no commit, no replay, and no
    // success claim; both retained authorities stand and the next
    // observation converges. The follow/stay tails below inherit this
    // verification (their actuation adds its own lifetime gates).
    if suspend_read(state, me, &fresh_fulls).veto.block {
        log_send("suspended", serde_json::json!({"window": from.0}));
        return "suspended";
    }
    if foreground_elevated(me) {
        log_send("elevated-foreground", serde_json::json!({"window": from.0}));
        return "elevated-foreground";
    }
    let membership_ok = state
        .workspaces
        .member_loc(member_key)
        .is_some_and(|fresh| fresh.output == home.output && fresh.workspace == home.workspace);
    let (final_max, final_full_opt) = live_overlay_flags(member_key.hwnd, &fresh_fulls);
    let final_live = directional_live(
        state,
        &fresh_areas,
        &snapshot,
        member_key,
        membership_ok,
        final_max,
        final_full_opt.unwrap_or(snap_overlay.1),
    );
    if let Some(outcome) =
        output_send_fence_outcome(state, &snapshot, &final_live, member_key, &from.0)
    {
        if outcome == "identity-changed" {
            drop_member_state(state, member_key);
        }
        log_send(outcome, serde_json::json!({"window": from.0}));
        return outcome;
    }
    // Verified: commit both authorities once, then follow/stay focus. The
    // pinned source/target never re-resolve off the relocated mover.
    if !state
        .workspaces
        .assign(member_key.clone(), target_output, target_workspace, false)
    {
        log_send("unverified", serde_json::json!({"window": from.0}));
        return "unverified";
    }
    let source_now = state
        .workspaces
        .workspace_members(&home.output, &home.workspace);
    let target_now = state
        .workspaces
        .workspace_members(target_output, target_workspace);
    if !crate::workspace_owner::verify_directional_transfer(member_key, &source_now, &target_now) {
        log_send("unverified", serde_json::json!({"window": from.0}));
        return "unverified";
    }
    state.engine = candidate;
    // Fresh arrival: mover present on target, absent on source, and the
    // Engine plan verified structurally inside the candidate handle.
    // Membership reads only, so this commutes ahead of the focus tails
    // (which touch focus bookkeeping alone).
    let source_now = state
        .workspaces
        .workspace_members(&home.output, &home.workspace);
    let target_now = state
        .workspaces
        .workspace_members(target_output, target_workspace);
    if !crate::workspace_owner::verify_directional_transfer(member_key, &source_now, &target_now) {
        log_send("unverified", serde_json::json!({"window": from.0}));
        return "unverified";
    }
    if follow {
        // Follow focuses the verified mover once, fullscreen like ordinary;
        // arrival is already in the destination current view.
        let actuation = actuate_focus(state, me, &fresh_fulls, observed, retained, &from.0);
        if actuation.outcome != "focus-ok" {
            log_send(
                actuation.outcome,
                serde_json::json!({
                    "window": from.0,
                    "setter_accepted": actuation.setter_accepted,
                    "prime_inserted": actuation.prime_inserted,
                    "attach_ok": actuation.attach_ok,
                    "eventual": actuation.eventual,
                    "deferred": actuation.deferred,
                    "cross_output": true,
                }),
            );
            return actuation.outcome;
        }
        note_focus_advance(state, observed, retained, &from.0);
        state.workspaces.note_foreground(member_key);
        log_send(
            "send-applied",
            serde_json::json!({
                "window": from.0,
                "setter_accepted": true,
                "cross_output": true,
            }),
        );
        "send-applied"
    } else {
        // Stay keeps the source current view: focus its MRU, or take no
        // setter when the source holds null. The target current view keeps
        // the arrived mover visible either way.
        let fresh_tokens: HashSet<String> = observed.iter().map(|w| w.token.clone()).collect();
        let eligible =
            state
                .workspaces
                .eligible_focus_set(&source_now, &state.member_tokens, &fresh_tokens);
        let stay_key = state
            .workspaces
            .focus_target(&home.output, &home.workspace, &eligible);
        let Some(stay_key) = stay_key else {
            log_send(
                "send-applied",
                serde_json::json!({"window": from.0, "focus": "none"}),
            );
            return "send-applied";
        };
        let Some(stay_token) = state.member_tokens.get(&stay_key).cloned() else {
            log_send(
                "send-applied",
                serde_json::json!({"window": from.0, "focus": "none"}),
            );
            return "send-applied";
        };
        let actuation = actuate_focus(state, me, &fresh_fulls, observed, retained, &stay_token);
        if actuation.outcome != "focus-ok" {
            log_send(
                actuation.outcome,
                serde_json::json!({
                    "window": from.0,
                    "setter_accepted": actuation.setter_accepted,
                    "prime_inserted": actuation.prime_inserted,
                    "attach_ok": actuation.attach_ok,
                    "eventual": actuation.eventual,
                    "deferred": actuation.deferred,
                    "cross_output": true,
                }),
            );
            return actuation.outcome;
        }
        note_focus_advance(state, observed, retained, &stay_token);
        state.workspaces.note_foreground(&stay_key);
        log_send(
            "send-applied",
            serde_json::json!({
                "window": from.0,
                "setter_accepted": true,
                "cross_output": true,
            }),
        );
        "send-applied"
    }
}

/// Membership-only transfer for an explicit output send across a floating
/// workspace boundary: verifies the fresh tails on the original snapshot,
/// projects the transfer onto row copies, reflows only the tiled side(s)
/// through a candidate Engine plus candidate membership, then verifies a
/// final fresh gate before publishing both candidates and follow/stay focus.
///
/// Commit-last: the authoritative membership and Engine are never staged.
/// Source survivors stay homed on the source in the projected rows, the
/// mover rides the projected target rows with its assembled frame and
/// floating eligibility preserved, and both writable sets exclude the mover
/// (membership-only never relocates the mover frame; the target view
/// converges around it). Floating sides keep every frame untouched. Any
/// drift or refused verification reports with no publish and no focus; the
/// retained authorities stand and the next observation converges. Follow
/// foregrounds the verified moved window once, fullscreen like ordinary;
/// stay focuses the source MRU (null
/// takes no setter) while the target current view keeps the mover visible.
/// Partial progress never replays.
#[allow(clippy::too_many_arguments)]
fn run_output_send_native(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    home: &crate::workspace::MemberLoc,
    member_key: &crate::workspace::WindowKey,
    from_token: &str,
    source_rows: &[crate::workspace_owner::OwnerRow],
    target_rows: &[crate::workspace_owner::OwnerRow],
    target_output: &str,
    target_workspace: &str,
    source_tiled: bool,
    target_tiled: bool,
    follow: bool,
    tick: u64,
    correlation: &CorrelationId,
    correlation_str: &str,
    origin: &crate::snapkey::SnapOrigin,
    intent: &crate::snapkey::QueuedWorkspaceOutputIntent,
    snapshot: &crate::workspace_owner::DirectionalFenceSnapshot,
) -> &'static str {
    let log_path = state.log_path.clone();
    let log_send = |outcome: &'static str, extra: serde_json::Value| {
        let mut line = serde_json::Map::new();
        line.insert("event".to_owned(), serde_json::json!("workspace-output"));
        line.insert("tick".to_owned(), serde_json::json!(tick));
        line.insert("correlation".to_owned(), serde_json::json!(correlation_str));
        line.insert(
            "direction".to_owned(),
            serde_json::json!(crate::snapkey::direction_name(intent.direction)),
        );
        line.insert("follow".to_owned(), serde_json::json!(follow));
        line.insert("edge".to_owned(), serde_json::json!(intent.edge.as_str()));
        line.insert("disposition".to_owned(), serde_json::json!("consumed"));
        line.insert("outcome".to_owned(), serde_json::json!(outcome));
        line.insert("origin".to_owned(), serde_json::json!(origin.token));
        if let serde_json::Value::Object(map) = extra {
            for (k, v) in map {
                line.insert(k, v);
            }
        }
        log_json_at(&log_path, serde_json::Value::Object(line));
    };
    // Fresh tails on the ORIGINAL snapshot before any membership commit:
    // suspend/elevated re-read, overlay pre-effect gate, the full live
    // fence set, then fresh topology (pinned target, never re-ranked).
    // Any drift refuses with no commit and no replay.
    if suspend_read(state, me, fulls).veto.block {
        log_send("suspended", serde_json::json!({"window": from_token}));
        return "suspended";
    }
    if foreground_elevated(me) {
        log_send(
            "elevated-foreground",
            serde_json::json!({"window": from_token}),
        );
        return "elevated-foreground";
    }
    let (live_max, live_full_opt) = live_overlay_flags(member_key.hwnd, fulls);
    if let Err(outcome) = crate::workspace_owner::send_overlay_gate(
        snapshot.overlay_maximized,
        snapshot.overlay_fullscreen,
        live_max,
        live_full_opt,
        snapshot.overlay_fullscreen,
    ) {
        log_send(outcome, serde_json::json!({"window": from_token}));
        return outcome;
    }
    let membership_ok = state
        .workspaces
        .member_loc(member_key)
        .is_some_and(|fresh| fresh.output == home.output && fresh.workspace == home.workspace);
    let live = directional_live(
        state,
        areas,
        snapshot,
        member_key,
        membership_ok,
        live_max,
        live_full_opt.unwrap_or(snapshot.overlay_fullscreen),
    );
    if let Some(outcome) = output_send_fence_outcome(state, snapshot, &live, member_key, from_token)
    {
        if outcome == "identity-changed" {
            drop_member_state(state, member_key);
        }
        log_send(outcome, serde_json::json!({"window": from_token}));
        return outcome;
    }
    let fresh_snapshot = crate::workspace_owner::drag_snapshot_for_directional(snapshot);
    let Some(fresh_areas) = fresh_cross_areas() else {
        log_send("send-unverified", serde_json::json!({"window": from_token}));
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return "send-unverified";
    };
    if !cross_topology_fresh(&fresh_snapshot, &fresh_areas) {
        log_send("send-unverified", serde_json::json!({"window": from_token}));
        let fresh_fulls: Vec<Rect> = fresh_areas.iter().map(|a| a.full).collect();
        reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
        return "send-unverified";
    }
    let fresh_fulls: Vec<Rect> = fresh_areas.iter().map(|a| a.full).collect();
    // Project the transfer onto row copies from the already-complete
    // observations: survivors stay homed on the source, the mover rides the
    // target rows with its assembled frame and floating eligibility
    // preserved. The authoritative membership is never staged or re-read
    // below; only the candidates carry the transfer.
    let projected_source: Vec<crate::workspace_owner::OwnerRow> = source_rows
        .iter()
        .filter(|r| r.token != from_token)
        .cloned()
        .collect();
    let Some(mover_row) = source_rows.iter().find(|r| r.token == from_token) else {
        log_send("unmanaged", serde_json::json!({"window": from_token}));
        return "unmanaged";
    };
    let mut projected_target: Vec<crate::workspace_owner::OwnerRow> = target_rows.to_vec();
    projected_target.push(mover_row.clone());
    // Candidate membership carries the transfer: the destination current
    // view is already visible, so the mover arrives visible (hidden stays
    // false, unlike a same-output stay into an inactive workspace, which
    // hides). Verified before any geometry runs; a refusal below publishes
    // nothing.
    let mut candidate_ws = state.workspaces.clone();
    if !candidate_ws.assign(member_key.clone(), target_output, target_workspace, false) {
        log_send("unverified", serde_json::json!({"window": from_token}));
        return "unverified";
    }
    let source_now = candidate_ws.workspace_members(&home.output, &home.workspace);
    let target_now = candidate_ws.workspace_members(target_output, target_workspace);
    if !crate::workspace_owner::verify_directional_transfer(member_key, &source_now, &target_now) {
        log_send("unverified", serde_json::json!({"window": from_token}));
        return "unverified";
    }
    // Tiled-side reflows on the candidate Engine over the projected rows,
    // so a refused apply publishes neither authority: source survivors when
    // the source is tiled (reconciled even when empty), target occupants
    // when the target is tiled. Both writable sets derive from the actual
    // current visible rows with the mover excluded (membership-only never
    // relocates the mover frame; the target view converges around it).
    // Floating sides keep every frame untouched.
    let mut candidate = state.engine.clone();
    if source_tiled {
        let Some((source_domain, source_key)) = workspace_domain_for(
            &home.output,
            &home.workspace,
            &fresh_areas,
            state.inner_gap,
            state.outer_gap,
        ) else {
            log_send("unknown-output", serde_json::json!({"window": from_token}));
            return "unknown-output";
        };
        let source_windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = projected_source
            .iter()
            .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
            .collect();
        let source_fp = fingerprint(
            &projected_source
                .iter()
                .map(|r| (r.token.clone(), r.rect))
                .collect::<Vec<_>>(),
        );
        let focused = state
            .focused_token(observed)
            .filter(|f| source_windows.iter().any(|(w, _, _, _)| w == f));
        let revision = revision_for(state, &home.output, &home.workspace);
        adopt_gaps_on_candidate(
            &mut candidate,
            state,
            &source_domain,
            &source_key,
            state.outer_gap,
            &source_windows,
            focused.as_ref(),
            revision,
            source_fp,
            correlation,
        );
        let mut event = crate::tiling::build_reconcile_event_for_floating(
            &state.owner,
            &state.generation,
            correlation,
            revision,
            source_fp,
            &source_domain,
            &source_key,
            state.outer_gap,
            &source_windows,
            focused.as_ref(),
        );
        let reply = {
            stamp_engine_event(state, &mut event);
            candidate.handle(&event)
        };
        let mut writable = writable_tokens(state, &home.output, &home.workspace, observed);
        writable.remove(from_token);
        state.tick += 1;
        let source_tick = state.tick;
        let summary = apply_geometry(
            state,
            ApplyInput {
                me,
                fulls: &fresh_fulls,
                reply: &reply,
                observed,
                op: "output-send-native",
                tick: source_tick,
                correlation: correlation_str,
                skipped: Vec::new(),
                writable: &writable,
                output_token: state.workspaces.output_token(&home.output),
                workspace_token: state
                    .workspaces
                    .workspace_token(&home.output, &home.workspace),
                revision: revision_for(state, &home.output, &home.workspace),
            },
        );
        if !matches!(summary, Some(s) if s.readback_ok && s.mismatched == 0) {
            let outcome = match summary {
                Some(s) if s.mismatched > 0 => "send-mismatch",
                _ => "send-unverified",
            };
            log_send(outcome, serde_json::json!({"window": from_token}));
            return outcome;
        }
    }
    if target_tiled {
        let Some((target_domain, target_key)) = workspace_domain_for(
            target_output,
            target_workspace,
            &fresh_areas,
            state.inner_gap,
            state.outer_gap,
        ) else {
            log_send("unknown-output", serde_json::json!({"window": from_token}));
            return "unknown-output";
        };
        let target_windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = projected_target
            .iter()
            .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
            .collect();
        let target_fp = fingerprint(
            &projected_target
                .iter()
                .map(|r| (r.token.clone(), r.rect))
                .collect::<Vec<_>>(),
        );
        let revision = revision_for(state, target_output, target_workspace);
        adopt_gaps_on_candidate(
            &mut candidate,
            state,
            &target_domain,
            &target_key,
            state.outer_gap,
            &target_windows,
            None,
            revision,
            target_fp,
            correlation,
        );
        let mut event = crate::tiling::build_reconcile_event_for_floating(
            &state.owner,
            &state.generation,
            correlation,
            revision,
            target_fp,
            &target_domain,
            &target_key,
            state.outer_gap,
            &target_windows,
            None,
        );
        let reply = {
            stamp_engine_event(state, &mut event);
            candidate.handle(&event)
        };
        // Membership-only never relocates the mover frame: the mover rides
        // the observation for topology but takes no geometry write.
        let mut writable = writable_tokens(state, target_output, target_workspace, observed);
        writable.remove(from_token);
        state.tick += 1;
        let target_tick = state.tick;
        let summary = apply_geometry(
            state,
            ApplyInput {
                me,
                fulls: &fresh_fulls,
                reply: &reply,
                observed,
                op: "output-send-native-target",
                tick: target_tick,
                correlation: correlation_str,
                skipped: Vec::new(),
                writable: &writable,
                output_token: state.workspaces.output_token(target_output),
                workspace_token: state
                    .workspaces
                    .workspace_token(target_output, target_workspace),
                revision: revision_for(state, target_output, target_workspace),
            },
        );
        if !matches!(summary, Some(s) if s.readback_ok && s.mismatched == 0) {
            let outcome = match summary {
                Some(s) if s.mismatched > 0 => "send-mismatch",
                _ => "send-unverified",
            };
            log_send(outcome, serde_json::json!({"window": from_token}));
            return outcome;
        }
    }
    // Final fresh gate after reflows, before logical commit: re-enumerate
    // topology against the pinned snapshot plus suspend/elevated and the
    // full live fence set on the original snapshot. Any drift refuses with
    // no publish and no focus; both retained authorities stand and the next
    // observation converges.
    if suspend_read(state, me, &fresh_fulls).veto.block {
        log_send("suspended", serde_json::json!({"window": from_token}));
        return "suspended";
    }
    if foreground_elevated(me) {
        log_send(
            "elevated-foreground",
            serde_json::json!({"window": from_token}),
        );
        return "elevated-foreground";
    }
    let Some(final_areas) = fresh_cross_areas() else {
        log_send("send-unverified", serde_json::json!({"window": from_token}));
        let fulls_owned = fresh_fulls.clone();
        reconcile_tick(state, me, &fulls_owned, areas);
        return "send-unverified";
    };
    if !cross_topology_fresh(&fresh_snapshot, &final_areas) {
        log_send("send-unverified", serde_json::json!({"window": from_token}));
        let final_fulls: Vec<Rect> = final_areas.iter().map(|a| a.full).collect();
        reconcile_tick(state, me, &final_fulls, &final_areas);
        return "send-unverified";
    }
    let final_fulls: Vec<Rect> = final_areas.iter().map(|a| a.full).collect();
    let membership_ok = state
        .workspaces
        .member_loc(member_key)
        .is_some_and(|fresh| fresh.output == home.output && fresh.workspace == home.workspace);
    let (live_max, live_full_opt) = live_overlay_flags(member_key.hwnd, &final_fulls);
    let live = directional_live(
        state,
        &final_areas,
        snapshot,
        member_key,
        membership_ok,
        live_max,
        live_full_opt.unwrap_or(snapshot.overlay_fullscreen),
    );
    if let Some(outcome) = output_send_fence_outcome(state, snapshot, &live, member_key, from_token)
    {
        if outcome == "identity-changed" {
            drop_member_state(state, member_key);
        }
        log_send(outcome, serde_json::json!({"window": from_token}));
        return outcome;
    }
    // Verified: publish both candidates once, then follow/stay focus on the
    // fresh topology.
    state.workspaces = candidate_ws;
    state.engine = candidate;
    if follow {
        // Follow focuses the verified mover, fullscreen like ordinary.
        let actuation = actuate_focus(state, me, &final_fulls, observed, retained, from_token);
        if actuation.outcome != "focus-ok" {
            log_send(
                actuation.outcome,
                serde_json::json!({
                    "window": from_token,
                    "setter_accepted": actuation.setter_accepted,
                    "prime_inserted": actuation.prime_inserted,
                    "attach_ok": actuation.attach_ok,
                    "eventual": actuation.eventual,
                    "deferred": actuation.deferred,
                    "cross_output": true,
                }),
            );
            return actuation.outcome;
        }
        note_focus_advance(state, observed, retained, from_token);
        state.workspaces.note_foreground(member_key);
    } else {
        let fresh_tokens: HashSet<String> = observed.iter().map(|w| w.token.clone()).collect();
        let eligible =
            state
                .workspaces
                .eligible_focus_set(&source_now, &state.member_tokens, &fresh_tokens);
        let stay_key = state
            .workspaces
            .focus_target(&home.output, &home.workspace, &eligible);
        let Some(stay_key) = stay_key else {
            log_send(
                "send-applied",
                serde_json::json!({"window": from_token, "focus": "none"}),
            );
            return "send-applied";
        };
        let Some(stay_token) = state.member_tokens.get(&stay_key).cloned() else {
            log_send(
                "send-applied",
                serde_json::json!({"window": from_token, "focus": "none"}),
            );
            return "send-applied";
        };
        let actuation = actuate_focus(state, me, &final_fulls, observed, retained, &stay_token);
        if actuation.outcome != "focus-ok" {
            log_send(
                actuation.outcome,
                serde_json::json!({
                    "window": from_token,
                    "setter_accepted": actuation.setter_accepted,
                    "prime_inserted": actuation.prime_inserted,
                    "attach_ok": actuation.attach_ok,
                    "eventual": actuation.eventual,
                    "deferred": actuation.deferred,
                    "cross_output": true,
                }),
            );
            return actuation.outcome;
        }
        note_focus_advance(state, observed, retained, &stay_token);
        state.workspaces.note_foreground(&stay_key);
    }
    log_send(
        "send-applied",
        serde_json::json!({
            "window": from_token,
            "setter_accepted": true,
            "cross_output": true,
        }),
    );
    "send-applied"
}
