#![forbid(unsafe_code)]
use super::*;

/// Shared `ToggleFloat` candidate for one domain member: assemble rows and
/// evaluate on a local candidate. The caller commits the Engine only after
/// its native effects verify, so a failed placement never strands state.
#[allow(clippy::too_many_arguments)]
fn prepare_toggle_float(
    state: &mut TileLoop,
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    op: &'static str,
    from: &str,
    loc: &crate::workspace::MemberLoc,
    float_rect: Option<Rect>,
    origin: &SnapOrigin,
    settle: &dyn Fn(&'static str) -> serde_json::Value,
    log_path: &Path,
    correlation: &CorrelationId,
    target: &'static str,
) -> Option<(CoreReply, tiler_core::engine::Engine)> {
    let mut hint_cx = HintCx::new();
    let Some(rows) = assemble_domain_rows(
        state,
        &loc.output,
        &loc.workspace,
        observed,
        retained,
        op,
        correlation.as_str(),
        &mut hint_cx,
    ) else {
        let mut line = settle("deferred");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(log_path, line);
        return None;
    };
    if !rows.iter().any(|r| r.token == from) {
        let mut line = settle("unmanaged");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(log_path, line);
        return None;
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
    let from_id = WindowId(from.to_owned());
    let Some((domain, domain_key)) = workspace_domain_for(
        &loc.output,
        &loc.workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    ) else {
        let mut line = settle("unknown-output");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(log_path, line);
        return None;
    };
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        correlation,
        revision_for(state, &loc.output, &loc.workspace),
        fp,
        &domain,
        &domain_key,
        OUTER_GAP,
        &windows,
        Some(&from_id),
    );
    event.command = CoreCommand::ToggleFloat {
        window: from.to_owned(),
        float_rect,
    };
    let mut candidate = state.engine.clone();
    stamp_engine_event(state, &mut event);
    let reply = candidate.handle(&event);
    if !matches!(reply, CoreReply::Tiled(_)) {
        let mut line = settle(reply_outcome(&reply));
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return None;
    }
    Some((reply, candidate))
}

/// Shared tiled-to-float transition: native placement plus raised band with
/// verified readback, Engine commit, sibling reflow, exact focus retained.
/// With `mark_sticky` the sticky mark lands after the float verifies; a failed
/// mark leaves a normal float for a later retry.
#[allow(clippy::too_many_arguments)]
pub(super) fn apply_float_from_tiled(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    skipped: Vec<(String, String)>,
    member_key: &crate::workspace::WindowKey,
    from: &str,
    loc: &crate::workspace::MemberLoc,
    origin: &SnapOrigin,
    settle: &dyn Fn(&'static str) -> serde_json::Value,
    log_path: &Path,
    tick: u64,
    correlation: &CorrelationId,
    op: &'static str,
    target: &'static str,
    applied: &'static str,
    mark_sticky: bool,
) {
    let Some((reply, candidate)) = prepare_toggle_float(
        state,
        areas,
        observed,
        retained,
        op,
        from,
        loc,
        None,
        origin,
        settle,
        log_path,
        correlation,
        target,
    ) else {
        return;
    };
    let CoreReply::Tiled(plan) = &reply else {
        return;
    };
    let Some(effective) = plan.float_rect else {
        let mut line = settle("float-missing-placement");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return;
    };
    let Some(expected) = observed.iter().find(|w| w.hwnd == member_key.hwnd) else {
        let mut line = settle("origin-vanished");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return;
    };
    let proof_mode = state.allowlist.is_some();
    let audit_pre = (
        expected.identity.pid,
        expected.identity.process_creation.clone(),
        expected.identity.exe_path.clone(),
        expected.identity.user_sid.clone(),
        expected.identity.session_id,
        expected.identity.tag.clone(),
    );
    let member_tag = state.member_tags.get(member_key).map(String::as_str);
    let placement = match revalidate_target(
        expected,
        me,
        fulls,
        &mut state.tokens,
        proof_mode,
        state.allowlist.as_ref(),
        &state.scope,
        member_tag,
        &state.scope_hosts,
        false,
    ) {
        Ok(target) => target,
        Err(reason) => {
            let mut line = settle(reason);
            line["origin"] = serde_json::Value::from(origin.token.clone());
            line["window"] = serde_json::Value::from(from.to_owned());
            line["target"] = serde_json::Value::from(target);
            log_json_at(log_path, line);
            return;
        }
    };
    let Some(outer) = placement.window.insets.visible_to_outer(effective) else {
        let mut line = settle("frame-overflow");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return;
    };
    if suspend_read(state, me, fulls).veto.block {
        let mut line = settle("fullscreen-foreground");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return;
    }
    let hwnd_u64 = placement.window.hwnd;
    let current_topmost = read_topmost_now(hwnd_u64);
    let prior = state
        .float_topmost_prev
        .get(member_key)
        .copied()
        .unwrap_or(current_topmost);
    let (after, flags) = if current_topmost {
        (
            crate::ffi::PositionAfter::NoOrderChange,
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
    } else {
        (crate::ffi::PositionAfter::Topmost, SWP_NOACTIVATE)
    };
    let placed = crate::ffi::set_window_pos_after(
        hwnd_u64, after, outer.x, outer.y, outer.w, outer.h, flags,
    );
    let _ = &placement.held;
    let pid_ok = pid_current(hwnd_u64, placement.window.identity.pid);
    if proof_mode {
        let (pid, creation, exe, sid, session, tag) = audit_pre;
        audit_json(
            state,
            serde_json::json!({
                "event": "proof-write",
                "tick": tick,
                "op": op,
                "window": from,
                "requested": [effective.x, effective.y, effective.w, effective.h],
                "outer": [outer.x, outer.y, outer.w, outer.h],
                "target": {
                    "hwnd": placement.window.hwnd,
                    "pid": pid,
                    "process_creation": creation,
                    "exe_path": exe,
                    "user_sid": sid,
                    "session_id": session,
                    "tag": tag,
                },
                "flags": if current_topmost { "SWP_NOACTIVATE|SWP_NOZORDER" } else { "SWP_NOACTIVATE|TOPMOST" },
                "outcome": if placed && pid_ok { "written" } else if !placed { "setter-failed" } else { "pid-changed" },
            }),
        );
    }
    if !placed || !pid_ok {
        let mut line = settle(if !placed {
            "setter-failed"
        } else {
            "pid-changed"
        });
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return;
    }
    state.float_topmost_prev.insert(member_key.clone(), prior);
    let actual = match observe_window(hwnd_u64 as isize as HWND, me, fulls, &mut state.tokens) {
        Ok(fresh)
            if fresh.token == placement.window.token
                && pid_current(hwnd_u64, fresh.identity.pid) =>
        {
            if readback_outcome(true, &effective, &fresh.visible) == ReadbackOutcome::Match {
                effective
            } else {
                fresh.visible
            }
        }
        _ => {
            let mut line = settle("float-unverified");
            line["origin"] = serde_json::Value::from(origin.token.clone());
            line["window"] = serde_json::Value::from(from.to_owned());
            line["target"] = serde_json::Value::from(target);
            log_json_at(log_path, line);
            return;
        }
    };
    if !prior && !read_topmost_now(hwnd_u64) {
        let mut line = settle("topmost-unverified");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return;
    }
    state.engine = candidate;
    state.float_rects.insert(from.to_owned(), actual);
    // R-SPC-04 D3/D7: a settled float withdraws any explicit tile win (a
    // later unfloat records a fresh one) and any D4 automatic latch: the
    // window is intentionally floating now. Clear the runtime lanes and
    // remove the on-window marker best-effort; a failed removal only logs
    // degraded and never fails the float. A pending first exit resolves
    // through this intentional lane too.
    state.fixed_tile_overrides.remove(member_key);
    state.fixed_origin_overrides.remove(member_key);
    state.fixed_auto_seen.remove(member_key);
    state.born_first_exit.remove(member_key);
    remove_tile_override_mark_best_effort(state, me, member_key, log_path, from);
    // Intentional-float lifetime: row assembly rides this token floating
    // across domain releases and boundary sends even when the Engine session
    // carries no exception yet. Sticky-on keeps its own lane instead. The
    // on-window marker persists only after this verified success so a later
    // owner restart re-adopts the classification; a failed install keeps
    // local intent and native state with the degraded outcome below.
    if !mark_sticky {
        state.floated.insert(member_key.clone());
    }
    if !mark_sticky && !install_float_mark_held(state, me, member_key) {
        let focus = retain_float_focus_validated(state, me, fulls, expected, member_key);
        let writable = writable_tokens(state, &loc.output, &loc.workspace, observed);
        let summary = apply_geometry(
            state,
            ApplyInput {
                me,
                fulls,
                reply: &reply,
                observed,
                op,
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
        let outcome: &'static str = match summary {
            Some(s) if !s.readback_ok => "float-unverified",
            Some(s) if s.mismatched > 0 || actual != effective => "float-mismatch",
            Some(_) => "float-unverified",
            None => reply_outcome(&reply),
        };
        let mut line = settle(outcome);
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        line["focus"] = serde_json::Value::from(focus);
        log_json_at(log_path, line);
        return;
    }
    if mark_sticky && !install_sticky_mark_held(state, me, member_key, false) {
        let focus = retain_float_focus_validated(state, me, fulls, expected, member_key);
        let writable = writable_tokens(state, &loc.output, &loc.workspace, observed);
        let summary = apply_geometry(
            state,
            ApplyInput {
                me,
                fulls,
                reply: &reply,
                observed,
                op,
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
        let outcome: &'static str = match summary {
            Some(s) if !s.readback_ok => "float-unverified",
            Some(s) if s.mismatched > 0 || actual != effective => "float-mismatch",
            Some(_) => "sticky-unverified",
            None => reply_outcome(&reply),
        };
        let mut line = settle(outcome);
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        line["focus"] = serde_json::Value::from(focus);
        log_json_at(log_path, line);
        return;
    }
    if mark_sticky {
        state.sticky.insert(member_key.clone(), false);
    }
    let focus = retain_float_focus_validated(state, me, fulls, expected, member_key);
    let writable = writable_tokens(state, &loc.output, &loc.workspace, observed);
    let summary = apply_geometry(
        state,
        ApplyInput {
            me,
            fulls,
            reply: &reply,
            observed,
            op,
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
    let outcome: &'static str = match summary {
        Some(s) if !s.readback_ok => "float-unverified",
        Some(s) if s.mismatched > 0 || actual != effective => "float-mismatch",
        Some(_) => applied,
        None => reply_outcome(&reply),
    };
    let mut line = settle(outcome);
    line["origin"] = serde_json::Value::from(origin.token.clone());
    line["window"] = serde_json::Value::from(from.to_owned());
    line["target"] = serde_json::Value::from(target);
    line["focus"] = serde_json::Value::from(focus);
    log_json_at(log_path, line);
}

/// Restore a project-raised topmost band under held gates before an unfloat.
/// Returns `None` after logging a failure; otherwise whether a restore ran.
#[allow(clippy::too_many_arguments)]
pub(super) fn restore_raised_band(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    observed: &[ObservedWindow],
    member_key: &crate::workspace::WindowKey,
    from: &str,
    target: &str,
    origin: &SnapOrigin,
    settle: &dyn Fn(&'static str) -> serde_json::Value,
    log_path: &Path,
    tick: u64,
    op: &'static str,
) -> Option<bool> {
    let Some(prior) = state.float_topmost_prev.get(member_key).copied() else {
        return Some(false);
    };
    let current = read_topmost_now(member_key.hwnd);
    if !crate::tiling::float_topmost_restore_needed(prior, current) {
        state.float_topmost_prev.remove(member_key);
        return Some(true);
    }
    let held = match observed.iter().find(|w| w.hwnd == member_key.hwnd) {
        Some(expected) => {
            let member_tag = state.member_tags.get(member_key).map(String::as_str);
            match revalidate_target(
                expected,
                me,
                fulls,
                &mut state.tokens,
                state.allowlist.is_some(),
                state.allowlist.as_ref(),
                &state.scope,
                member_tag,
                &state.scope_hosts,
                false,
            ) {
                Ok(target) => target.held,
                Err(reason) => {
                    let mut line = settle(reason);
                    line["origin"] = serde_json::Value::from(origin.token.clone());
                    line["window"] = serde_json::Value::from(from.to_owned());
                    line["target"] = serde_json::Value::from(target);
                    log_json_at(log_path, line);
                    return None;
                }
            }
        }
        None => match hold_band_target(state, me, member_key) {
            Some(held) => held,
            None => {
                let mut line = settle("identity-changed");
                line["origin"] = serde_json::Value::from(origin.token.clone());
                line["window"] = serde_json::Value::from(from.to_owned());
                line["target"] = serde_json::Value::from(target);
                log_json_at(log_path, line);
                return None;
            }
        },
    };
    let restored = set_topmost_band(member_key.hwnd, prior)
        && read_topmost_now(member_key.hwnd) == prior
        && pid_current(member_key.hwnd, member_key.pid);
    let _ = &held;
    if state.allowlist.is_some() {
        audit_json(
            state,
            serde_json::json!({
                "event": "proof-topmost",
                "tick": tick,
                "op": op,
                "window": from,
                "restored": restored,
            }),
        );
    }
    if !restored {
        let mut line = settle("topmost-unverified");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        line["topmost_restored"] = serde_json::Value::from(false);
        log_json_at(log_path, line);
        return None;
    }
    state.float_topmost_prev.remove(member_key);
    Some(true)
}

/// Clear one sticky mark under the fresh held gate with the guard kept
/// across the `RemoveProp` and its readback. The live value must still
/// equal the pre-sticky state and the pid plus lifetime tag must stay fresh
/// before and after. A verifiably absent marker (destroyed HWND or no
/// property) needs no native write, so runtime may still drop without the
/// hold; a present marker without the hold stays for later recovery. Logs
/// the failure and returns false without mutating.
#[allow(clippy::too_many_arguments)]
pub(super) fn clear_sticky_mark(
    state: &TileLoop,
    me: &ProcessIdentity,
    member_key: &crate::workspace::WindowKey,
    expected_prior: bool,
    from: &str,
    target: &str,
    origin: &SnapOrigin,
    settle: &dyn Fn(&'static str) -> serde_json::Value,
    log_path: &Path,
) -> bool {
    let Some(held) = hold_marker_target(state, me, member_key) else {
        if !crate::ffi::is_window(member_key.hwnd)
            || crate::product_hide::sys::read_sticky_marker(member_key.hwnd).is_none()
        {
            return true;
        }
        let mut line = settle("identity-changed");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return false;
    };
    if !marker_tag_pid_fresh(state, member_key) {
        let _ = &held;
        let mut line = settle("identity-changed");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return false;
    }
    let result = crate::product_hide::sys::remove_sticky_marker(member_key.hwnd, expected_prior);
    let fresh = marker_tag_pid_fresh(state, member_key);
    let _ = &held;
    match result {
        Ok(true) if fresh => true,
        Ok(true) => {
            let mut line = settle("identity-changed");
            line["origin"] = serde_json::Value::from(origin.token.clone());
            line["window"] = serde_json::Value::from(from.to_owned());
            line["target"] = serde_json::Value::from(target);
            log_json_at(log_path, line);
            false
        }
        Ok(false) => {
            let mut line = settle("identity-changed");
            line["origin"] = serde_json::Value::from(origin.token.clone());
            line["window"] = serde_json::Value::from(from.to_owned());
            line["target"] = serde_json::Value::from(target);
            log_json_at(log_path, line);
            false
        }
        Err(_) => {
            let mut line = settle("sticky-unverified");
            line["origin"] = serde_json::Value::from(origin.token.clone());
            line["window"] = serde_json::Value::from(from.to_owned());
            line["target"] = serde_json::Value::from(target);
            log_json_at(log_path, line);
            false
        }
    }
}

/// Clear one intentional-float mark under the fresh held gate with the guard
/// kept across the `RemoveProp` and its readback, on settled unfloat
/// (ordinary unfloat, sticky-off to tile, cross-domain rehome to tile).
/// Corrupt residue on our own property name cleans with the same call (it is
/// inert either way) under the same full identity plus lifetime fences. A
/// verifiably absent marker (destroyed HWND or no property) needs no native
/// write, so runtime may still drop without the hold; a present marker
/// without the hold stays for later recovery. Logs the failure and returns
/// false without mutating.
#[allow(clippy::too_many_arguments)]
pub(super) fn clear_float_mark(
    state: &TileLoop,
    me: &ProcessIdentity,
    member_key: &crate::workspace::WindowKey,
    from: &str,
    target: &str,
    origin: &SnapOrigin,
    settle: &dyn Fn(&'static str) -> serde_json::Value,
    log_path: &Path,
) -> bool {
    let Some(held) = hold_marker_target(state, me, member_key) else {
        if !crate::ffi::is_window(member_key.hwnd)
            || !crate::product_hide::sys::read_float_intent_marker(member_key.hwnd)
        {
            return true;
        }
        let mut line = settle("identity-changed");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return false;
    };
    if !marker_tag_pid_fresh(state, member_key) {
        let _ = &held;
        let mut line = settle("identity-changed");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return false;
    }
    let result = crate::product_hide::sys::remove_float_intent_marker(member_key.hwnd);
    let fresh = marker_tag_pid_fresh(state, member_key);
    let _ = &held;
    match result {
        Ok(()) if fresh => true,
        Ok(()) => {
            let mut line = settle("identity-changed");
            line["origin"] = serde_json::Value::from(origin.token.clone());
            line["window"] = serde_json::Value::from(from.to_owned());
            line["target"] = serde_json::Value::from(target);
            log_json_at(log_path, line);
            false
        }
        Err(_) => {
            let mut line = settle("float-unverified");
            line["origin"] = serde_json::Value::from(origin.token.clone());
            line["window"] = serde_json::Value::from(from.to_owned());
            line["target"] = serde_json::Value::from(target);
            log_json_at(log_path, line);
            false
        }
    }
}

/// Shared float-to-tiled transition: project-raised band restored first under
/// held gates, classification markers cleared between the band and the
/// commit, Engine commit, sibling reflow, exact focus retained. With
/// `unmark_sticky` the sticky mark clears first (sticky-off to tile); the
/// intentional-float mark always clears here (settled unfloat, including a
/// prior-float sticky tiling).
#[allow(clippy::too_many_arguments)]
pub(super) fn apply_unfloat_to_tiled(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    skipped: Vec<(String, String)>,
    member_key: &crate::workspace::WindowKey,
    from: &str,
    loc: &crate::workspace::MemberLoc,
    live_rect: Option<Rect>,
    origin: &SnapOrigin,
    settle: &dyn Fn(&'static str) -> serde_json::Value,
    log_path: &Path,
    tick: u64,
    correlation: &CorrelationId,
    op: &'static str,
    target: &'static str,
    unmark_sticky: bool,
) {
    let float_rect = live_rect;
    let Some((reply, candidate)) = prepare_toggle_float(
        state,
        areas,
        observed,
        retained,
        op,
        from,
        loc,
        float_rect,
        origin,
        settle,
        log_path,
        correlation,
        target,
    ) else {
        return;
    };
    // Capacity re-float: the tile retry found no space, so the window
    // stays floating with its frame. No tile override, no marker
    // removal, no toggle loop: lanes persist, survivors reflow below,
    // and the outcome reports no-space. The raised band stays as-is.
    let capacity_refloat = matches!(&reply, CoreReply::Tiled(plan) if plan.float_window.is_some());
    let topmost_restored = if capacity_refloat {
        // The float continues: keep its raised band exactly as-is.
        false
    } else {
        let Some(restored) = restore_raised_band(
            state, me, fulls, observed, member_key, from, target, origin, settle, log_path, tick,
            op,
        ) else {
            return;
        };
        restored
    };
    if unmark_sticky {
        let expected_prior = state.sticky.get(member_key).copied().unwrap_or(false);
        if !clear_sticky_mark(
            state,
            me,
            member_key,
            expected_prior,
            from,
            target,
            origin,
            settle,
            log_path,
        ) {
            return;
        }
    }
    if capacity_refloat {
        // Repair a foreign marker value; never remove: the window stays
        // floating. Failures keep state with a degraded log.
        let marker_ok = crate::product_hide::sys::read_float_intent_marker(member_key.hwnd)
            || install_float_mark_held(state, me, member_key);
        if !marker_ok {
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "unfloat-capacity",
                    "window": from,
                    "outcome": "marker-unverified",
                }),
            );
        }
    } else if !clear_float_mark(
        state, me, member_key, from, target, origin, settle, log_path,
    ) {
        return;
    }
    // R-SPC-04 D7 provenance: capture BEFORE the winning assignment below
    // withdraws the automatic mark. Only a known automatic-fixed prior wins
    // durable persistence; generic non-fixed unfloats keep runtime
    // suppression only and install no marker.
    let fixed_win = fixed_origin_provenance(state, member_key);
    state.engine = candidate;
    if capacity_refloat {
        // Stays floating: keep lanes and frame, record no win.
        if !state.floated.contains(member_key) {
            state.floated.insert(member_key.clone());
        }
        if let Some(frame) = float_rect {
            state.float_rects.insert(from.to_owned(), frame);
        }
    } else {
        state.sticky.remove(member_key);
        state.floated.remove(member_key);
        state.float_rects.remove(from);
    }
    // R-SPC-04 D3/D7: a settled unfloat is an explicit user tile win for
    // this live client. Record it in the runtime override set (survives
    // domain releases so workspace-enable re-adoption keeps tiling it);
    // fixed-origin wins additionally join the durable set and persist the
    // on-window marker (survives owner restarts; recompute fallback
    // otherwise). The D4 automatic latch withdraws: the window is
    // explicitly tiled now. A failed marker install keeps local intent and
    // native state with a degraded log. A capacity re-float records no
    // win: overrides and latches stand as they were.
    if !capacity_refloat {
        state.fixed_auto_seen.remove(member_key);
        state.fixed_tile_overrides.insert(member_key.clone());
        if fixed_win {
            state.fixed_origin_overrides.insert(member_key.clone());
            if !install_tile_override_mark_held(state, me, member_key) {
                log_json_at(
                    log_path,
                    serde_json::json!({
                        "event": "tile-override",
                        "window": from,
                        "outcome": "marker-unverified",
                    }),
                );
            }
        }
    }
    let focus = match observed.iter().find(|w| w.hwnd == member_key.hwnd) {
        Some(expected) => retain_float_focus_validated(state, me, fulls, expected, member_key),
        None => "float-focus-failed",
    };
    let writable = writable_tokens(state, &loc.output, &loc.workspace, observed);
    let summary = apply_geometry(
        state,
        ApplyInput {
            me,
            fulls,
            reply: &reply,
            observed,
            op,
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
    let outcome: &'static str = match summary {
        Some(s) if !s.readback_ok => "unfloat-unverified",
        Some(s) if s.mismatched > 0 => "unfloat-mismatch",
        Some(_) => "unfloat-applied",
        None => reply_outcome(&reply),
    };
    // A re-float that reflowed its survivors reports no-space instead
    // of a tile success.
    let outcome = if capacity_refloat && outcome == "unfloat-applied" {
        "unfloat-no-space"
    } else {
        outcome
    };
    let mut line = settle(outcome);
    line["origin"] = serde_json::Value::from(origin.token.clone());
    line["window"] = serde_json::Value::from(from.to_owned());
    line["target"] = serde_json::Value::from(target);
    line["focus"] = serde_json::Value::from(focus);
    line["topmost_restored"] = serde_json::Value::from(topmost_restored);
    log_json_at(log_path, line);
}

/// Fresh observation promoted after one observed B9 clear: the ordinary
/// fresh-admission route below runs on this instead of the pre-clear
/// snapshot, so the restored frame (not the maximized one) drives placement.
#[derive(Debug, Clone)]
pub(super) struct FreshFloatObservation {
    pub(super) observed: Vec<ObservedWindow>,
    pub(super) retained: Vec<RetainedRow>,
    pub(super) skipped: Vec<(String, String)>,
}

/// Native effects behind the B9 one-shot clear, split from the guard
/// ordering so hermetic tests can substitute scripted answers while the
/// production route (pre-guards, clear issue, fresh observe, post-guards,
/// admission gating, logs) runs unchanged. [`NativeUnfloatClear`] calls
/// exactly the functions the inline code called; behavior is identical.
/// Message-only owned windows cannot carry this path (never foreground,
/// never enumerated, machine-dependent suspend reads), so scripted stubs are
/// the hermetic route, not the `AdoptWindow` harness.
pub(super) trait UnfloatClearEffects {
    fn revalidate(
        &mut self,
        state: &mut TileLoop,
        member_key: &crate::workspace::WindowKey,
        expected: &ObservedWindow,
        me: &ProcessIdentity,
        fulls: &[Rect],
    ) -> std::result::Result<WriteTarget, &'static str>;
    fn foreground(&mut self) -> u64;
    fn suspend_block(&mut self, state: &mut TileLoop, me: &ProcessIdentity, fulls: &[Rect])
    -> bool;
    fn elevated(&mut self, me: &ProcessIdentity) -> bool;
    fn fullscreen_meta(&mut self, hwnd: u64) -> Option<FullscreenMeta>;
    /// Live stale-retirement probe: `(style, fullscreen)` where `None` on
    /// either leg means unreadable (fail closed). No writes.
    fn retire_probe(&mut self, hwnd: u64, fulls: &[Rect]) -> (Option<u32>, Option<bool>);
    /// Best-effort stale-marker removal with absence readback. No geometry
    /// writes. Returns true only when the marker reads absent afterward.
    fn retire_clear(&mut self, hwnd: u64) -> bool;
    fn restore(&mut self, hwnd: u64) -> &'static str;
    fn pid_current(&mut self, hwnd: u64, pid: u32) -> bool;
    fn observe(
        &mut self,
        state: &mut TileLoop,
        me: &ProcessIdentity,
        fulls: &[Rect],
    ) -> Option<FreshFloatObservation>;
}

/// Production native effects for the B9 clear: exact pass-throughs to the
/// Win32 reads/writes the route requires.
pub(super) struct NativeUnfloatClear;

impl UnfloatClearEffects for NativeUnfloatClear {
    fn revalidate(
        &mut self,
        state: &mut TileLoop,
        member_key: &crate::workspace::WindowKey,
        expected: &ObservedWindow,
        me: &ProcessIdentity,
        fulls: &[Rect],
    ) -> std::result::Result<WriteTarget, &'static str> {
        let stored_tag = state.member_tags.get(member_key).map(String::as_str);
        let proof = state.allowlist.is_some();
        revalidate_target(
            expected,
            me,
            fulls,
            &mut state.tokens,
            proof,
            state.allowlist.as_ref(),
            &state.scope,
            stored_tag,
            &state.scope_hosts,
            true,
        )
    }

    fn foreground(&mut self) -> u64 {
        crate::ffi::foreground_window()
    }

    fn suspend_block(
        &mut self,
        state: &mut TileLoop,
        me: &ProcessIdentity,
        fulls: &[Rect],
    ) -> bool {
        suspend_read(state, me, fulls).veto.block
    }

    fn elevated(&mut self, me: &ProcessIdentity) -> bool {
        foreground_elevated(me)
    }

    fn fullscreen_meta(&mut self, hwnd: u64) -> Option<FullscreenMeta> {
        read_fullscreen_meta(hwnd)
    }

    fn retire_probe(&mut self, hwnd: u64, fulls: &[Rect]) -> (Option<u32>, Option<bool>) {
        (
            live_style_for_retire(hwnd),
            live_overlay_flags(hwnd, fulls).1,
        )
    }

    fn retire_clear(&mut self, hwnd: u64) -> bool {
        clear_fullscreen_meta(hwnd)
    }

    fn restore(&mut self, hwnd: u64) -> &'static str {
        restore_zoom_placement(hwnd)
    }

    fn pid_current(&mut self, hwnd: u64, pid: u32) -> bool {
        pid_current(hwnd, pid)
    }

    fn observe(
        &mut self,
        state: &mut TileLoop,
        me: &ProcessIdentity,
        fulls: &[Rect],
    ) -> Option<FreshFloatObservation> {
        let mut skipped: Vec<(String, String)> = Vec::new();
        let mut retained: Vec<RetainedRow> = Vec::new();
        let observed = state.observe(me, fulls, &mut skipped, &mut retained)?;
        Some(FreshFloatObservation {
            observed,
            retained,
            skipped,
        })
    }
}

/// B9 one-shot clear for an ordinary maximized float toggle (R-FLT-06,
/// COSMIC unmaximize-then-fresh-admit parity): exactly one nonactivating
/// native restore plus fresh re-observation. Only an observed clear with the
/// same reference, domain, and focus and no fullscreen promotes the fresh
/// observation (`Ok`) for ordinary fresh admission below; every other outcome
/// returns its settle `(outcome, cause)` with float membership and preimages
/// retained (no Engine commit, no marker change): the caller settles and ends
/// only this toggle, so a later press retries, with no delayed intent and no
/// automatic retry. The caller gates fullscreen, sticky, tiled, and
/// non-maximized lanes before calling; reused pure gates
/// (`authorize_maximize_clear`, `maximize_clear_refusal`) keep the item-18
/// clear discipline. One native attempt per explicit activation.
#[allow(clippy::too_many_arguments)]
pub(super) fn unfloat_maximize_clear(
    state: &mut TileLoop,
    effects: &mut dyn UnfloatClearEffects,
    me: &ProcessIdentity,
    fulls: &[Rect],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    member_key: &crate::workspace::WindowKey,
    from: &str,
    loc: &crate::workspace::MemberLoc,
    origin_token: &str,
    log_path: &Path,
    tick: u64,
    correlation: &CorrelationId,
) -> std::result::Result<FreshFloatObservation, (&'static str, Option<&'static str>)> {
    // Exact subject with retained-overlay support: chord-time membership
    // alone never authorizes the native clear. The expectation must pin the
    // chord-time member key exactly, or a token/observation skew would clear
    // the wrong window. A maximized float is retained (never eligible), so
    // the retained fallback applies here.
    let Some(expected) = overlay_expected(state, observed, retained, from) else {
        return Err(("float-refused-maximize", None));
    };
    if expected.hwnd != member_key.hwnd
        || expected.identity.pid != member_key.pid
        || expected.identity.process_creation != member_key.creation
    {
        return Err(("float-refused-maximize-clear", Some("identity-raced")));
    }
    // Fresh exact-lifetime revalidation with the process held open across
    // the clear; overlays stay allowed (the subject is maximized by
    // construction here).
    let target = match effects.revalidate(state, member_key, &expected, me, fulls) {
        Ok(target) => target,
        Err(reason) => return Err(("float-refused-maximize-clear", Some(reason))),
    };
    // Home/focus stability plus suspension/elevation just before the write:
    // a veto authorizes no clear and no unfloat. Games and elevated
    // foregrounds stay untouched. Fresh fullscreen precedence on the
    // revalidated subject: the tick snapshot may predate a native or
    // project-owned fullscreen transition. Owned restoration metadata wins
    // over geometry, matching the fullscreen toggle decision; uncertainty
    // fails closed. A stale marker whose chrome provably restored and whose
    // frame provably no longer covers retires here (metadata removal only,
    // no geometry write) so an exit by other means does not strand the
    // clear; a captionless non-cover transition never retires. Never
    // cleared, never unfloated on veto.
    let same_home = state
        .workspaces
        .member_loc(member_key)
        .is_some_and(|fresh| fresh.output == loc.output && fresh.workspace == loc.workspace);
    let same_focus_pre = effects.foreground() == member_key.hwnd;
    let suspend_blocked = effects.suspend_block(state, me, fulls);
    let elevated = effects.elevated(me);
    let pre_meta = effects.fullscreen_meta(member_key.hwnd);
    let effective_pre = effective_stale_meta_via_effects(
        effects,
        state,
        member_key,
        &expected,
        me,
        pre_meta,
        same_home && same_focus_pre && !suspend_blocked && !elevated,
        fulls,
    );
    let fresh_fullscreen = target.window.facts.captionless_fullscreen || effective_pre.is_some();
    if let Some(cause) = crate::tiling::authorize_maximize_clear(
        fresh_fullscreen,
        same_home,
        same_focus_pre,
        suspend_blocked,
        elevated,
    ) {
        return Err(("float-refused-maximize-clear", Some(cause)));
    }
    log_json_at(
        log_path,
        serde_json::json!({
            "event": "float-toggle",
            "tick": tick,
            "correlation": correlation.as_str(),
            "edge": "consumed",
            "disposition": "consumed",
            "outcome": "float-maximize-clear",
            "result": "issued",
            "origin": origin_token,
            "window": from,
        }),
    );
    let clear = effects.restore(member_key.hwnd);
    // The held process keeps liveness authority across the clear; recheck
    // the HWND pid afterward against recycling.
    let _ = &target.held;
    if !effects.pid_current(member_key.hwnd, target.window.identity.pid) {
        return Err(("float-refused-maximize-clear", Some("identity-raced")));
    }
    log_json_at(
        log_path,
        serde_json::json!({
            "event": "float-toggle",
            "tick": tick,
            "correlation": correlation.as_str(),
            "edge": "consumed",
            "disposition": "consumed",
            "outcome": "float-maximize-clear",
            "result": clear,
            "origin": origin_token,
            "window": from,
        }),
    );
    // Fresh re-observation: pre-clear topology must never drive admission.
    let Some(fresh) = effects.observe(state, me, fulls) else {
        return Err(("observation-failed", None));
    };
    let observed_fresh = fresh.observed;
    let retained_fresh = fresh.retained;
    let skipped_fresh = fresh.skipped;
    let same_ref = state
        .member_tokens
        .get(member_key)
        .is_some_and(|token| token.as_str() == from);
    let fresh_row = retained_fresh.iter().find(|r| r.key == *member_key);
    // Eligible again means cleared: a maximized member is retained, never
    // eligible, so observed presence proves the clear. Otherwise the fresh
    // retained flags decide.
    let observed_cleared = if observed_fresh.iter().any(|w| w.token == from) {
        true
    } else {
        fresh_row.is_some_and(|row| !row.maximized)
    };
    // Home stability after the clear: pre-clear topology must never drive
    // admission.
    let same_domain = state
        .workspaces
        .member_loc(member_key)
        .is_some_and(|fresh| fresh.output == loc.output && fresh.workspace == loc.workspace);
    let same_focus = effects.foreground() == member_key.hwnd;
    // Post-clear fullscreen fence through the shared derivation (same as
    // the directional route): the fresh observed captionless flag plus the
    // retained row flag plus a fresh owned-metadata read. A retained row's
    // flag alone misses an owned transition whose frame never covered a
    // monitor; uncertainty fails closed and a later press retries.
    let fresh_captionless = observed_fresh
        .iter()
        .find(|w| w.token == from)
        .map(|w| w.facts.captionless_fullscreen)
        .unwrap_or(false);
    let fullscreen_now = crate::tiling::postclear_fullscreen_now(
        fresh_captionless,
        fresh_row.is_some_and(|row| row.fullscreen),
        effects.fullscreen_meta(member_key.hwnd).is_some(),
    );
    if let Some(cause) = crate::tiling::maximize_clear_refusal(
        clear,
        observed_cleared,
        same_ref,
        same_domain,
        same_focus,
        fullscreen_now,
    ) {
        return Err(("float-refused-maximize-clear", Some(cause)));
    }
    log_json_at(
        log_path,
        serde_json::json!({
            "event": "float-toggle",
            "tick": tick,
            "correlation": correlation.as_str(),
            "edge": "consumed",
            "disposition": "consumed",
            "outcome": "float-maximize-clear",
            "result": "observed-cleared",
            "origin": origin_token,
            "window": from,
        }),
    );
    Ok(FreshFloatObservation {
        observed: observed_fresh,
        retained: retained_fresh,
        skipped: skipped_fresh,
    })
}
