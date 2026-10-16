use super::*;
use crate::snapkey::QueuedIntent;

pub(super) fn dispatch_directional_intent(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    intent: QueuedIntent,
    log_path: &Path,
) {
    if !intent.consumed || !intent.announce {
        // Key-ups close the pair and passed chords never dispatch:
        // trace-only, so held-key traffic stays out of normal logs.
        if state.trace {
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "snap",
                    "tick": state.tick,
                    "op": intent.op.as_str(),
                    "direction": direction_name(intent.direction),
                    "edge": intent.edge.as_str(),
                    "disposition": if intent.consumed { "consumed" } else { "passed" },
                    "outcome": if intent.consumed { "key-up" } else { "passed" },
                }),
            );
        }
        return;
    }
    // Fresh complete observation per intent. The chord-time origin
    // must re-resolve: stale intents die here, never on a cached
    // rectangle.
    state.tick += 1;
    let tick = state.tick;
    let correlation = state.correlation();
    let Some(origin) = intent.origin.clone() else {
        // Authentic interception swallows unmanaged/unadmitted/
        // shell/suspended/elevated chords with no origin; settle
        // defensively without dispatch (never acts on protected
        // windows).
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "origin-vanished",
            }),
        );
        return;
    };
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        // Enumeration failure settles the action as failed; the
        // Engine keeps retained state and the next tick retries.
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "observation-failed",
                "origin": origin.token,
            }),
        );
        return;
    };
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    // First-seen maximized windows restore once here: lifetime tags are
    // stamped, and the cleared window converges as eligible next tick.
    clear_maximize_at_admission(state, me, &retained);
    let fresh: Vec<SnapOrigin> = state.snap_origins.values().cloned().collect();
    let foreground_hwnd = Some(unsafe { GetForegroundWindow() } as usize as u64);
    let (from, continued) = match resolve_origin(
        &origin,
        foreground_hwnd,
        &fresh,
        state.snap_advance.as_ref(),
    ) {
        OriginVerdict::Dispatch { token, continued } => (token, continued),
        OriginVerdict::Reject(reason) => {
            // External focus or lifetime mismatch invalidates the
            // chain; never carry a stale advance forward.
            state.snap_advance = None;
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "snap",
                    "tick": tick,
                    "correlation": correlation.as_str(),
                    "op": intent.op.as_str(),
                    "direction": direction_name(intent.direction),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": reason,
                    "origin": origin.token,
                }),
            );
            return;
        }
    };
    // Route through the origin window's own (output, workspace)
    // session: directional ops run on the retained per-workspace
    // domain the mover actually lives in, never a parallel
    // single-display family.
    let Some(member_key) = state
        .member_tokens
        .iter()
        .find(|(_, token)| token.as_str() == from.as_str())
        .map(|(key, _)| key.clone())
    else {
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "unmanaged",
                "origin": origin.token,
            }),
        );
        return;
    };
    if !crate::workspace_owner::member_matches(
        &member_key,
        origin.hwnd,
        origin.pid,
        &origin.creation,
    ) {
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "origin-vanished",
                "origin": origin.token,
            }),
        );
        return;
    }
    let Some(mut loc) = state.workspaces.member_loc(&member_key).cloned() else {
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "unmanaged",
                "origin": origin.token,
            }),
        );
        return;
    };
    // Home truth for lifetime/domain stability checks below.
    let home = loc.clone();
    // Sticky subjects show on every workspace of their output:
    // route in the displayed (active) workspace when homed
    // elsewhere so focus races, half-snaps and clear routing
    // match the visible workspace. Ordinary windows keep strict
    // home-domain routing.
    let sticky_subject = state.sticky.contains_key(&member_key);
    if sticky_subject
        && let Some(active) = state.workspaces.active_id(&home.output)
        && active != loc.workspace
    {
        loc.workspace = active;
    }
    let Some((domain, domain_key)) = workspace_domain_for(
        &loc.output,
        &loc.workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    ) else {
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "unknown-output",
                "origin": origin.token,
            }),
        );
        return;
    };
    // Float-origin subject classification (R-FLT-07..11): sticky
    // and intentional floats share the floating layer for
    // directional focus and half-snaps below; tile origins keep
    // skipping floats exactly as before. Automatic fixed floats
    // (R-SPC-04 D8 membership-only) keep the existing floating
    // refusal: classification takes no geometry, focus, stacking,
    // or keep-above writes. A pruned-domain sticky (no Engine
    // session) routes below and safely misses/retains instead of
    // refusing, since every float actuation revalidates first.
    // Float state reads home truth: a sticky subject routed in its
    // displayed workspace still classifies from its home session.
    let float_subject = engine_is_float(state, &home.output, &home.workspace, &from);
    let automatic_subject = engine_is_automatic_fixed(state, &home.output, &home.workspace, &from);
    if automatic_subject && (sticky_subject || float_subject) {
        let outcome = match intent.op {
            SnapOp::Focus => "focus-refused-floating",
            SnapOp::Move => "move-refused-floating",
        };
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": outcome,
                "origin": origin.token,
                "window": from,
            }),
        );
        return;
    }
    let float_origin = (sticky_subject || float_subject) && !automatic_subject;
    // Floating workspaces run no tile layout: both focus and move
    // refuse with the matching workspace-floating vocabulary (KDE
    // plan-adapter parity). Frames stay native.
    if let Some(outcome) = crate::workspace_owner::directional_workspace_refusal(
        intent.op,
        workspace_mode_tiled(state, &loc.output, &loc.workspace),
    ) {
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": outcome,
                "origin": origin.token,
                "window": from,
            }),
        );
        return;
    }
    // Subject overlay flags from the fresh per-tick snapshot:
    // retained rows carry maximized/fullscreen members while an
    // observed eligible subject holds neither.
    let (subject_maximized, subject_fullscreen) = retained
        .iter()
        .find(|r| r.key == member_key)
        .map(|r| (r.maximized, r.fullscreen))
        .unwrap_or((false, false));
    // Fullscreen precedence (both origins, KDE parity): a
    // fullscreen subject is never cleared or moved. Focus carries
    // no geometry write and stays allowed.
    if intent.op == SnapOp::Move && subject_fullscreen {
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "move-refused-fullscreen",
                "origin": origin.token,
                "window": from,
            }),
        );
        return;
    }
    // G-06 maximized directional focus fence (REQ-MAX-08):
    // directional focus no-ops while the focused window is
    // maximized (leave via unmaximize or Alt+Tab). Covers
    // tile-origin and float-origin subjects; fullscreen stays
    // exempt above.
    if intent.op == SnapOp::Focus
        && crate::tiling::maximized_focus_fence(subject_maximized, subject_fullscreen)
    {
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "focus-refused-maximize",
                "origin": origin.token,
                "window": from,
            }),
        );
        return;
    }
    // G-06 unmaximize-before-move (REQ-MAX-08): authorize the
    // exact subject first, issue exactly one nonactivating
    // clear, then only an observed clear with the same
    // reference, focus and domain continues in this invocation
    // (a cleared float takes the half-snap below, a cleared tile
    // the ordinary move). Any refusal is narrow with no clear
    // and no structural move; a later press retries. Fullscreen
    // precedence is handled above.
    if intent.op == SnapOp::Move
        && crate::tiling::maximized_focus_fence(subject_maximized, subject_fullscreen)
    {
        // Exact subject with retained-overlay support:
        // chord-time membership alone never authorizes the
        // native clear. The expectation must pin the chord-time
        // member key exactly, or a token/observation skew would
        // clear the wrong window.
        let Some(expected) = overlay_expected(state, &observed, &retained, &from) else {
            state.snap_advance = None;
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "snap",
                    "tick": tick,
                    "correlation": correlation.as_str(),
                    "op": intent.op.as_str(),
                    "direction": direction_name(intent.direction),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "move-refused-maximize-clear",
                    "cause": "vanished",
                    "origin": origin.token,
                    "window": from,
                }),
            );
            return;
        };
        if expected.hwnd != member_key.hwnd
            || expected.identity.pid != member_key.pid
            || expected.identity.process_creation != member_key.creation
        {
            state.snap_advance = None;
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "snap",
                    "tick": tick,
                    "correlation": correlation.as_str(),
                    "op": intent.op.as_str(),
                    "direction": direction_name(intent.direction),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "move-refused-maximize-clear",
                    "cause": "identity-raced",
                    "origin": origin.token,
                    "window": from,
                }),
            );
            return;
        }
        // Fresh exact-lifetime revalidation with the process held
        // open across the clear; overlays stay allowed (the
        // subject is maximized by construction here).
        let stored_tag = state.member_tags.get(&member_key).map(String::as_str);
        let proof = state.allowlist.is_some();
        let target = match revalidate_target(
            &expected,
            me,
            fulls,
            &mut state.tokens,
            proof,
            state.allowlist.as_ref(),
            &state.scope,
            stored_tag,
            &state.scope_hosts,
            true,
        ) {
            Ok(target) => target,
            Err(reason) => {
                state.snap_advance = None;
                log_json_at(
                    log_path,
                    serde_json::json!({
                        "event": "snap",
                        "tick": tick,
                        "correlation": correlation.as_str(),
                        "op": intent.op.as_str(),
                        "direction": direction_name(intent.direction),
                        "edge": intent.edge.as_str(),
                        "disposition": "consumed",
                        "outcome": "move-refused-maximize-clear",
                        "cause": reason,
                        "origin": origin.token,
                        "window": from,
                    }),
                );
                return;
            }
        };
        // Home/view stability plus suspension/elevation just
        // before the write: a veto authorizes no clear and no
        // move. Games and elevated foregrounds stay untouched.
        let same_home = state
            .workspaces
            .member_loc(&member_key)
            .is_some_and(|fresh| fresh.output == home.output && fresh.workspace == home.workspace);
        let same_view = if sticky_subject {
            state
                .workspaces
                .active_id(&home.output)
                .is_some_and(|active| active == loc.workspace)
        } else {
            true
        };
        let same_focus_pre = unsafe { GetForegroundWindow() } as usize as u64 == member_key.hwnd;
        let read = suspend_read(state, me, fulls);
        let elevated = foreground_elevated(me);
        // Fresh fullscreen precedence on the revalidated
        // subject: the tick snapshot may predate a native or
        // project-owned fullscreen transition (a managed
        // fullscreen foreground bypasses the suspend veto, and
        // the focus-relaxed revalidation allows fullscreen
        // facts). Owned restoration metadata wins over
        // geometry, matching the fullscreen toggle decision;
        // uncertainty fails closed. The shared stale-retirement
        // sequence below is the same production ordering the B9
        // route drives hermetically (metadata removal only, no
        // geometry write); a captionless non-cover transition
        // never retires. Never cleared, never moved on veto.
        let pre_meta = read_fullscreen_meta(member_key.hwnd);
        let mut native_retire = NativeUnfloatClear;
        let effective_pre = effective_stale_meta_via_effects(
            &mut native_retire,
            state,
            &member_key,
            &expected,
            me,
            pre_meta,
            same_home && same_view && same_focus_pre && !read.veto.block && !elevated,
            fulls,
        );
        let fresh_fullscreen =
            target.window.facts.captionless_fullscreen || effective_pre.is_some();
        if let Some(cause) = crate::tiling::authorize_maximize_clear(
            fresh_fullscreen,
            same_home && same_view,
            same_focus_pre,
            read.veto.block,
            elevated,
        ) {
            state.snap_advance = None;
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "snap",
                    "tick": tick,
                    "correlation": correlation.as_str(),
                    "op": intent.op.as_str(),
                    "direction": direction_name(intent.direction),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "move-refused-maximize-clear",
                    "cause": cause,
                    "origin": origin.token,
                    "window": from,
                }),
            );
            return;
        }
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "move-maximize-clear",
                "result": "issued",
                "origin": origin.token,
                "window": from,
            }),
        );
        let clear = restore_zoom_placement(member_key.hwnd);
        // The held process keeps liveness authority across the
        // clear; recheck the HWND pid afterward against recycling.
        let _ = &target.held;
        if !pid_current(target.window.hwnd, target.window.identity.pid) {
            state.snap_advance = None;
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "snap",
                    "tick": tick,
                    "correlation": correlation.as_str(),
                    "op": intent.op.as_str(),
                    "direction": direction_name(intent.direction),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "move-refused-maximize-clear",
                    "cause": "identity-raced",
                    "origin": origin.token,
                    "window": from,
                }),
            );
            return;
        }
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "move-maximize-clear",
                "result": clear,
                "origin": origin.token,
                "window": from,
            }),
        );
        // Fresh re-observation: pre-clear topology must never
        // drive the move.
        let mut skipped_fresh: Vec<(String, String)> = Vec::new();
        let mut retained_fresh: Vec<RetainedRow> = Vec::new();
        let Some(observed_fresh) =
            state.observe(me, fulls, &mut skipped_fresh, &mut retained_fresh)
        else {
            state.snap_advance = None;
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "snap",
                    "tick": tick,
                    "correlation": correlation.as_str(),
                    "op": intent.op.as_str(),
                    "direction": direction_name(intent.direction),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "observation-failed",
                    "origin": origin.token,
                }),
            );
            return;
        };
        let same_ref = state
            .member_tokens
            .get(&member_key)
            .is_some_and(|token| token.as_str() == from.as_str());
        let fresh_row = retained_fresh.iter().find(|r| r.key == member_key);
        // Eligible again means cleared: observed windows hold no
        // overlay state. Otherwise the fresh retained flags decide.
        let observed_cleared = if observed_fresh.iter().any(|w| w.token == from) {
            true
        } else {
            fresh_row.is_some_and(|row| !row.maximized)
        };
        // Home stability after the clear: pre-clear topology
        // (and, for sticky subjects, the displayed workspace)
        // must never drive the move.
        let same_domain = state
            .workspaces
            .member_loc(&member_key)
            .is_some_and(|fresh| fresh.output == home.output && fresh.workspace == home.workspace);
        let same_view_post = !sticky_subject
            || state
                .workspaces
                .active_id(&home.output)
                .is_some_and(|active| active == loc.workspace);
        let same_focus = unsafe { GetForegroundWindow() } as usize as u64 == member_key.hwnd;
        // Post-clear fullscreen fence through the shared
        // derivation (same as the B9 route): the fresh observed
        // captionless flag plus the retained row flag plus a
        // fresh owned-metadata read. A retained row's flag alone
        // misses an owned transition whose frame never covered a
        // monitor; uncertainty fails closed and a later press
        // retries.
        let fresh_captionless = observed_fresh
            .iter()
            .find(|w| w.token == from)
            .map(|w| w.facts.captionless_fullscreen)
            .unwrap_or(false);
        let fullscreen_now = crate::tiling::postclear_fullscreen_now(
            fresh_captionless,
            fresh_row.is_some_and(|row| row.fullscreen),
            read_fullscreen_meta(member_key.hwnd).is_some(),
        );
        if let Some(cause) = crate::tiling::maximize_clear_refusal(
            clear,
            observed_cleared,
            same_ref,
            same_domain && same_view_post,
            same_focus,
            fullscreen_now,
        ) {
            state.snap_advance = None;
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "snap",
                    "tick": tick,
                    "correlation": correlation.as_str(),
                    "op": intent.op.as_str(),
                    "direction": direction_name(intent.direction),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "move-refused-maximize-clear",
                    "cause": cause,
                    "origin": origin.token,
                    "window": from,
                }),
            );
            return;
        }
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "move-maximize-clear",
                "result": "observed-cleared",
                "origin": origin.token,
                "window": from,
            }),
        );
        // Route below on the fresh observation.
        observed = observed_fresh;
        retained = retained_fresh;
        skipped = skipped_fresh;
    }
    let mut hint_cx = HintCx::new();
    // Float-origin directional commands (R-FLT-07..11): local
    // synchronous routing over same-domain observed floats with
    // exactly one native actuation and no Engine admission. Tile
    // origins skip floats exactly as before.
    if float_origin {
        let direction = direction_name(intent.direction);
        // The subject must still read as the focused observed
        // float; a vanished subject refuses stale with no writes.
        let Some(subject) = observed.iter().find(|w| w.token == from) else {
            state.snap_advance = None;
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "snap",
                    "tick": tick,
                    "correlation": correlation.as_str(),
                    "op": intent.op.as_str(),
                    "direction": direction_name(intent.direction),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": match intent.op {
                        SnapOp::Focus => "focus-float-refused-stale",
                        SnapOp::Move => "move-float-refused-stale",
                    },
                    "origin": origin.token,
                    "window": from,
                }),
            );
            return;
        };
        if intent.op == SnapOp::Focus {
            let candidates = float_focus_candidates(
                state,
                &observed,
                &retained,
                &loc.output,
                &loc.workspace,
                &from,
            );
            let Some(target) = crate::tiling::select_float_focus_target(
                subject.visible.x,
                subject.visible.y,
                direction,
                &candidates,
            ) else {
                // Miss retains focus: single-output has no
                // cross-output leg (multi-output foundation
                // pending) and never cycles workspaces.
                state.snap_advance = None;
                log_json_at(
                    log_path,
                    serde_json::json!({
                        "event": "snap",
                        "tick": tick,
                        "correlation": correlation.as_str(),
                        "op": intent.op.as_str(),
                        "direction": direction_name(intent.direction),
                        "edge": intent.edge.as_str(),
                        "disposition": "consumed",
                        "outcome": "focus-float-retained",
                        "reason": "no-target",
                        "origin": origin.token,
                        "window": from,
                    }),
                );
                return;
            };
            let actuation = actuate_focus(state, me, fulls, &observed, &retained, &target);
            if actuation.outcome == "focus-ok" {
                note_focus_advance(state, &observed, &retained, &target);
                log_json_at(
                    log_path,
                    serde_json::json!({
                        "event": "snap",
                        "tick": tick,
                        "correlation": correlation.as_str(),
                        "op": intent.op.as_str(),
                        "direction": direction_name(intent.direction),
                        "edge": intent.edge.as_str(),
                        "disposition": "consumed",
                        "outcome": "focus-float-applied",
                        "setter_accepted": actuation.setter_accepted,
                        "prime_inserted": actuation.prime_inserted,
                        "attach_ok": actuation.attach_ok,
                        "eventual": actuation.eventual,
                        "deferred": actuation.deferred,
                        "origin": origin.token,
                        "window": from,
                        "continued": continued,
                    }),
                );
            } else {
                state.snap_advance = None;
                log_json_at(
                    log_path,
                    serde_json::json!({
                        "event": "snap",
                        "tick": tick,
                        "correlation": correlation.as_str(),
                        "op": intent.op.as_str(),
                        "direction": direction_name(intent.direction),
                        "edge": intent.edge.as_str(),
                        "disposition": "consumed",
                        "outcome": actuation.outcome,
                        "setter_accepted": actuation.setter_accepted,
                        "prime_inserted": actuation.prime_inserted,
                        "attach_ok": actuation.attach_ok,
                        "eventual": actuation.eventual,
                        "deferred": actuation.deferred,
                        "origin": origin.token,
                        "window": from,
                        "continued": continued,
                    }),
                );
            }
            return;
        }
        // Half-snap: the stateless work-area half for the
        // direction; the window stays floating and focused. No
        // Engine call, no overlay clear, no transfer; repeats
        // request the same half again.
        let Some(area) = areas.iter().find(|a| a.device == loc.output) else {
            state.snap_advance = None;
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "snap",
                    "tick": tick,
                    "correlation": correlation.as_str(),
                    "op": intent.op.as_str(),
                    "direction": direction_name(intent.direction),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "unknown-output",
                    "origin": origin.token,
                }),
            );
            return;
        };
        let Some(half) = crate::tiling::float_half_snap_rect(area.work, state.inner_gap, direction)
        else {
            state.snap_advance = None;
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "snap",
                    "tick": tick,
                    "correlation": correlation.as_str(),
                    "op": intent.op.as_str(),
                    "direction": direction_name(intent.direction),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "move-float-refused-invalid-geometry",
                    "origin": origin.token,
                    "window": from,
                }),
            );
            return;
        };
        // Declared-size constraints refuse without writes (KDE
        // `move-float-refused-constraints` parity): an
        // incompatible half never fights the client. Maximum
        // track rides the same query; unknown stays unknown.
        let hints = min_hint_for(subject, &hint_cx).hints;
        if !crate::tiling::half_snap_constraints_ok(half, hints) {
            state.snap_advance = None;
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "snap",
                    "tick": tick,
                    "correlation": correlation.as_str(),
                    "op": intent.op.as_str(),
                    "direction": direction_name(intent.direction),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "move-float-refused-constraints",
                    "origin": origin.token,
                    "window": from,
                }),
            );
            return;
        }
        let Some(outer) = subject.insets.visible_to_outer(half) else {
            state.snap_advance = None;
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "snap",
                    "tick": tick,
                    "correlation": correlation.as_str(),
                    "op": intent.op.as_str(),
                    "direction": direction_name(intent.direction),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "move-float-refused-invalid-geometry",
                    "origin": origin.token,
                    "window": from,
                }),
            );
            return;
        };
        // Revalidate immediately before the single write, mirroring
        // the shared write path minus the tile-membership fences.
        let member_key_float = crate::workspace::WindowKey {
            hwnd: subject.hwnd,
            pid: subject.identity.pid,
            creation: subject.identity.process_creation.clone(),
        };
        let stored_tag_float = state.member_tags.get(&member_key_float).map(String::as_str);
        let proof_float = state.allowlist.is_some();
        let target = match revalidate_target(
            subject,
            me,
            fulls,
            &mut state.tokens,
            proof_float,
            state.allowlist.as_ref(),
            &state.scope,
            stored_tag_float,
            &state.scope_hosts,
            false,
        ) {
            Ok(target) => target,
            Err(reason) => {
                state.snap_advance = None;
                log_json_at(
                    log_path,
                    serde_json::json!({
                        "event": "snap",
                        "tick": tick,
                        "correlation": correlation.as_str(),
                        "op": intent.op.as_str(),
                        "direction": direction_name(intent.direction),
                        "edge": intent.edge.as_str(),
                        "disposition": "consumed",
                        "outcome": reason,
                        "origin": origin.token,
                        "window": from,
                    }),
                );
                return;
            }
        };
        // User safety at write time: a vetoing foreground that
        // arrived during validation vetoes this write, never rides
        // along. Games stay untouched.
        let read = suspend_read(state, me, fulls);
        if read.veto.block {
            state.snap_advance = None;
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "snap",
                    "tick": tick,
                    "correlation": correlation.as_str(),
                    "op": intent.op.as_str(),
                    "direction": direction_name(intent.direction),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "move-float-skipped-fence",
                    "origin": origin.token,
                    "window": from,
                }),
            );
            return;
        }
        let hwnd = target.window.hwnd as isize as HWND;
        let ok = unsafe {
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                outer.x,
                outer.y,
                outer.w,
                outer.h,
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
        };
        // The held process keeps liveness authority across the
        // write; recheck the HWND pid afterward against recycling.
        let _ = &target.held;
        let pid_ok = pid_current(target.window.hwnd, target.window.identity.pid);
        if proof_float {
            audit_json(
                state,
                serde_json::json!({
                    "event": "proof-write",
                    "tick": tick,
                    "op": "move-float",
                    "window": from,
                    "requested": [half.x, half.y, half.w, half.h],
                    "outer": [outer.x, outer.y, outer.w, outer.h],
                    "flags": "SWP_NOACTIVATE|SWP_NOZORDER",
                    "outcome": if ok != 0 && pid_ok { "written" } else if ok == 0 { "setter-failed" } else { "pid-changed" },
                }),
            );
        }
        if !pid_ok {
            state.snap_advance = None;
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "snap",
                    "tick": tick,
                    "correlation": correlation.as_str(),
                    "op": intent.op.as_str(),
                    "direction": direction_name(intent.direction),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "move-float-refused-stale",
                    "origin": origin.token,
                    "window": from,
                }),
            );
            return;
        }
        if ok == 0 {
            state.snap_advance = None;
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "snap",
                    "tick": tick,
                    "correlation": correlation.as_str(),
                    "op": intent.op.as_str(),
                    "direction": direction_name(intent.direction),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "move-float-write-failed",
                    "origin": origin.token,
                    "window": from,
                }),
            );
            return;
        }
        // The half-snap retains focus on the same subject, exactly
        // like KDE's post-write retention.
        let actuation = actuate_focus(state, me, fulls, &observed, &retained, &from);
        if actuation.outcome != "focus-ok" {
            state.snap_advance = None;
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "snap",
                    "tick": tick,
                    "correlation": correlation.as_str(),
                    "op": intent.op.as_str(),
                    "direction": direction_name(intent.direction),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "move-float-focus-failed",
                    "origin": origin.token,
                    "window": from,
                }),
            );
            return;
        }
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "move-float-applied",
                "origin": origin.token,
                "window": from,
                "continued": continued,
            }),
        );
        return;
    }
    let Some(rows) = assemble_domain_rows(
        state,
        &loc.output,
        &loc.workspace,
        &observed,
        &retained,
        "directional",
        correlation.as_str(),
        &mut hint_cx,
    ) else {
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "deferred",
                "origin": origin.token,
            }),
        );
        return;
    };
    if !rows.iter().any(|r| r.token == from) {
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "unmanaged",
                "origin": origin.token,
            }),
        );
        return;
    }
    // Multi-output directional target (REQ-OUT-01..03, REQ-MOV-08):
    // window-based FULL-rectangle selection into the destination
    // output's current view. `None` keeps the local path below
    // (no candidate, floating target, vertical focus, or an
    // unreadable destination which must never refuse an
    // otherwise valid local command).
    let is_focus = intent.op == SnapOp::Focus;
    let mover_rect = rows.iter().find(|r| r.token == from).map(|r| r.rect);
    let directional_target = mover_rect.and_then(|rect| {
        resolve_directional_target(state, areas, &loc.output, intent.direction, &rect, is_focus)
    });
    if let Some((target_output, target_workspace)) = directional_target {
        let outcome = run_directional_cross(
            state,
            me,
            fulls,
            areas,
            &observed,
            &retained,
            &mut hint_cx,
            &loc,
            &home,
            &member_key,
            &from,
            &rows,
            &target_output,
            &target_workspace,
            &intent,
            &origin,
            tick,
            &correlation,
            correlation.as_str(),
            continued,
            skipped,
            subject_maximized,
            subject_fullscreen,
        );
        if outcome != "focus-ok" && outcome != "move-applied" {
            state.snap_advance = None;
        }
        // `run_directional_cross` logs the full action line.
        return;
    }
    let windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
        .iter()
        .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
        .collect();
    let fp = fingerprint(
        &rows
            .iter()
            .map(|r| (r.token.clone(), r.rect))
            .collect::<Vec<_>>(),
    );
    // `from` is the origin-verified token, never blind foreground.
    let from = WindowId(from);
    // Live gap edits adopt here so the focus/move convergence
    // below sees matching gaps instead of refusing the keypress.
    {
        let revision = revision_for(state, &loc.output, &loc.workspace);
        let outer_gap = state.outer_gap;
        adopt_gaps_for_route(
            state,
            &domain,
            &domain_key,
            outer_gap,
            &windows,
            Some(&from),
            revision,
            fp,
            &correlation,
        );
    }
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &correlation,
        revision_for(state, &loc.output, &loc.workspace),
        fp,
        &domain,
        &domain_key,
        state.outer_gap,
        &windows,
        Some(&from),
    );
    let direction = direction_name(intent.direction).to_owned();
    event.command = match intent.op {
        SnapOp::Focus => CoreCommand::Focus {
            window: from.0.clone(),
            direction,
            cross_output_transfer: false,
            float_subject: false,
        },
        SnapOp::Move => CoreCommand::Move {
            window: from.0.clone(),
            direction,
            cross_output_transfer: false,
            // R-MOV-03: last-good live setting, read per move so
            // Apply changes subsequent moves only. Proof owners
            // carry no live settings and keep the default wrap.
            same_axis_move: same_axis_move_for(state.settings_live.as_ref()),
        },
    };
    // Single-domain observations run the local retained
    // propose/commit path; Core owns direction semantics.
    let reply = handle_engine_event(state, &mut event);
    let outcome = match intent.op {
        SnapOp::Focus => {
            if let CoreReply::FocusDirectional(plan) = &reply {
                let to = plan.to_window.0.clone();
                let actuation = actuate_focus(state, me, fulls, &observed, &retained, &to);
                let outcome = actuation.outcome;
                if outcome == "focus-ok" {
                    note_focus_advance(state, &observed, &retained, &to);
                } else {
                    state.snap_advance = None;
                }
                log_json_at(
                    log_path,
                    serde_json::json!({
                        "event": "snap",
                        "tick": tick,
                        "correlation": correlation.as_str(),
                        "op": intent.op.as_str(),
                        "direction": direction_name(intent.direction),
                        "edge": intent.edge.as_str(),
                        "disposition": "consumed",
                        "outcome": outcome,
                        "setter_accepted": actuation.setter_accepted,
                        "prime_inserted": actuation.prime_inserted,
                        "attach_ok": actuation.attach_ok,
                        "eventual": actuation.eventual,
                        "deferred": actuation.deferred,
                        "origin": origin.token,
                        "window": from.0,
                        "continued": continued,
                    }),
                );
                return;
            }
            reply_outcome(&reply)
        }
        SnapOp::Move => {
            if let CoreReply::MoveDirectional(_) = &reply {
                let writable = writable_tokens(state, &loc.output, &loc.workspace, &observed);
                let summary = apply_geometry(
                    state,
                    ApplyInput {
                        me,
                        fulls,
                        reply: &reply,
                        observed: &observed,
                        op: "move",
                        tick,
                        correlation: correlation.as_str(),
                        skipped,
                        writable: &writable,
                        output_token: state.workspaces.output_token(&loc.output),
                        workspace_token: state
                            .workspaces
                            .workspace_token(&loc.output, &loc.workspace),
                        revision: revision_for(state, &loc.output, &loc.workspace),
                    },
                );
                match summary {
                    Some(s) if !s.readback_ok => "move-unverified",
                    Some(s) if s.mismatched > 0 => "move-mismatch",
                    Some(s) if s.applied > 0 => "move-applied",
                    Some(_) => "move-noop",
                    None => reply_outcome(&reply),
                }
            } else {
                reply_outcome(&reply)
            }
        }
    };
    log_json_at(
        log_path,
        serde_json::json!({
            "event": "snap",
            "tick": tick,
            "correlation": correlation.as_str(),
            "op": intent.op.as_str(),
            "direction": direction_name(intent.direction),
            "edge": intent.edge.as_str(),
            "disposition": "consumed",
            "outcome": outcome,
            "origin": origin.token,
            "window": from.0,
            "continued": continued,
        }),
    );
}
