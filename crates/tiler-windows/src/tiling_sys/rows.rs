#![forbid(unsafe_code)]
use super::*;

/// Held ownership gates for one classification-marker native write (`SetProp` /
/// `RemoveProp` for sticky, float-intent, or the reserved tile override) on
/// a stored member: the held process identity must equal the stored full
/// identity with matching creation, same user/session as the owner, medium
/// integrity, live lifetime tag, plus scope, hosted-child, and proof fences.
/// The caller keeps the returned hold across the write and its readback and
/// rechecks pid plus lifetime tag immediately before the call (a stable
/// process handle never protects HWND reuse) with a consistent readback
/// after.
pub(super) fn hold_marker_target(
    state: &TileLoop,
    me: &ProcessIdentity,
    key: &crate::native_guards::WindowKey,
) -> Option<HeldProcess> {
    // Test-only mirror of the gates below against a registered fake: the
    // scripted pid/process/liveness/integrity stand in for the OS reads
    // while the predicates (identity, scope, lifetime, integrity) stay
    // real. The self-process hold is only the liveness guard stand-in:
    // its ambient runner level never gates scripted installs.
    // Unregistered HWNDs fall through to the OS exactly as before.
    #[cfg(test)]
    if crate::product_hide::sys::fake::active(key.hwnd) {
        use crate::product_hide::sys::fake;
        let stored = state.member_identity.get(key)?.clone();
        let (facts_ok, tag, integrity_rid) = fake::with(key.hwnd, |w| {
            (
                w.alive
                    && w.pid == key.pid
                    && w.process.pid == stored.pid
                    && w.process.process_creation == key.creation
                    && w.process.process_creation == stored.process_creation
                    && w.process.user_sid == stored.user_sid
                    && w.process.session_id == stored.session_id
                    && crate::lifecycle::exe_paths_equal(&w.process.exe_path, &stored.exe_path),
                w.member_tag.clone(),
                w.integrity_rid,
            )
        })?;
        if !facts_ok {
            return None;
        }
        if stored.user_sid != me.user_sid || stored.session_id != me.session_id {
            return None;
        }
        if !scope_allows(&state.scope, &stored.exe_path) {
            return None;
        }
        if !hosted_gate_allows(&stored.exe_path, key.hwnd, key.pid, &state.scope_hosts) {
            return None;
        }
        if let Some(entries) = state.allowlist.as_ref() {
            let entry = entries.iter().find(|e| e.hwnd == key.hwnd)?;
            if verify_proof_owned(key.hwnd, entry, me).is_err() {
                return None;
            }
        }
        let stored_tag = state.member_tags.get(key)?;
        if !crate::native_guards::visible_lifetime_ok(stored_tag, tag.as_deref()) {
            return None;
        }
        if !pid_current(key.hwnd, key.pid) {
            return None;
        }
        let held = fake::hold_verified(key.pid, &stored).and_then(|r| r.ok())?;
        if !is_medium_rid(integrity_rid) {
            return None;
        }
        return Some(held);
    }
    let stored = state.member_identity.get(key)?;
    let held = HeldProcess::open(key.pid).ok()?;
    let ident = held.identity().ok()?;
    if ident.pid != key.pid
        || ident.process_creation != key.creation
        || stored.pid != key.pid
        || stored.process_creation != key.creation
        || ident.user_sid != stored.user_sid
        || ident.session_id != stored.session_id
    {
        return None;
    }
    if !crate::lifecycle::exe_paths_equal(&ident.exe_path, &stored.exe_path) {
        return None;
    }
    if ident.user_sid != me.user_sid || ident.session_id != me.session_id {
        return None;
    }
    if !held.integrity().is_ok_and(is_medium_rid) {
        return None;
    }
    if !scope_allows(&state.scope, &stored.exe_path) {
        return None;
    }
    if !hosted_gate_allows(&stored.exe_path, key.hwnd, key.pid, &state.scope_hosts) {
        return None;
    }
    if let Some(entries) = state.allowlist.as_ref() {
        let entry = entries.iter().find(|e| e.hwnd == key.hwnd)?;
        if verify_proof_owned(key.hwnd, entry, me).is_err() {
            return None;
        }
    }
    let live_tag = crate::product_hide::sys::read_member_tag(key.hwnd);
    let stored_tag = state.member_tags.get(key)?;
    if !crate::native_guards::visible_lifetime_ok(stored_tag, live_tag.as_deref()) {
        return None;
    }
    if !pid_current(key.hwnd, key.pid) {
        return None;
    }
    Some(held)
}

/// Held ownership gates for a band-only topmost effect on a stored member
/// with no live eligible observation (retained floats, graceful stop).
/// Same bar as [`hold_marker_target`]: the caller keeps the returned hold
/// across the effect and rechecks pid plus band readback after.
/// Geometry effects use `revalidate_target` (fresh eligibility classification);
/// band effects move, size, and activate nothing.
pub(super) fn hold_band_target(
    state: &TileLoop,
    me: &ProcessIdentity,
    key: &crate::native_guards::WindowKey,
) -> Option<HeldProcess> {
    hold_marker_target(state, me, key)
}

/// Fresh pid plus lifetime-tag recheck for one classification-marker write:
/// the live member tag must still equal the stored tag and the HWND must
/// still resolve to the stored pid. Call immediately before the `SetProp` /
/// `RemoveProp` under the held process and again after the readback.
pub(super) fn marker_tag_pid_fresh(state: &TileLoop, key: &crate::native_guards::WindowKey) -> bool {
    state.member_tags.get(key).is_some_and(|stored| {
        crate::native_guards::visible_lifetime_ok(
            stored,
            crate::product_hide::sys::read_member_tag(key.hwnd).as_deref(),
        )
    }) && pid_current(key.hwnd, key.pid)
}

/// Install one sticky marker under the fresh held gate with the guard kept
/// across the write and its readback. Returns true only when the install
/// reads back consistently and the pid plus lifetime tag stay fresh after.
pub(super) fn install_sticky_mark_held(
    state: &TileLoop,
    me: &ProcessIdentity,
    key: &crate::native_guards::WindowKey,
    prior_floating: bool,
) -> bool {
    let Some(held) = hold_marker_target(state, me, key) else {
        return false;
    };
    if !marker_tag_pid_fresh(state, key) {
        let _ = &held;
        return false;
    }
    let ok = crate::product_hide::sys::install_sticky_marker(key.hwnd, prior_floating).is_ok();
    let fresh = marker_tag_pid_fresh(state, key);
    let _ = &held;
    ok && fresh
}

/// Install one intentional-float marker under the fresh held gate with the
/// guard kept across the write and its readback. Only call after the Engine
/// commit plus verified native effects hold: success-only persistence keeps
/// local intent and native state on failure with no rollback. Returns true
/// only when the install reads back consistently and the pid plus lifetime
/// tag stay fresh after.
pub(super) fn install_float_mark_held(
    state: &TileLoop,
    me: &ProcessIdentity,
    key: &crate::native_guards::WindowKey,
) -> bool {
    let Some(held) = hold_marker_target(state, me, key) else {
        return false;
    };
    if !marker_tag_pid_fresh(state, key) {
        let _ = &held;
        return false;
    }
    let ok = crate::product_hide::sys::install_float_intent_marker(key.hwnd).is_ok();
    let fresh = marker_tag_pid_fresh(state, key);
    let _ = &held;
    ok && fresh
}

/// Install one fixed-window tile-override marker (R-SPC-04 D7) under the
/// fresh held gate with the guard kept across the write and its readback.
/// Only call after settled unfloat verified (Engine commit plus native
/// effects hold): success-only persistence keeps local intent (the runtime
/// override set) and native state on failure with a degraded log from the
/// caller. Returns true only when the install reads back consistently and
/// the pid plus lifetime tag stay fresh after.
pub(super) fn install_tile_override_mark_held(
    state: &TileLoop,
    me: &ProcessIdentity,
    key: &crate::native_guards::WindowKey,
) -> bool {
    let Some(held) = hold_marker_target(state, me, key) else {
        return false;
    };
    if !marker_tag_pid_fresh(state, key) {
        let _ = &held;
        return false;
    }
    let ok = crate::product_hide::sys::install_tile_override_marker(key.hwnd).is_ok();
    let fresh = marker_tag_pid_fresh(state, key);
    let _ = &held;
    ok && fresh
}

/// Best-effort removal of one tile-override marker on settled re-float
/// (R-SPC-04 D7: clear on re-float). Mirrors the item-8 clear shape: a
/// verifiably absent marker needs no write; otherwise the held process is
/// bound across the before/after freshness checks plus the effect and its
/// readback, and any failure only logs degraded from the caller and never
/// fails the float itself. No write runs when identity is unsafe. A stale
/// marker stays classification-inert while its window floats (suppress
/// consults tiled admissions only) and dies with the window; verified close
/// and membership prune drop the runtime set.
pub(super) fn remove_tile_override_mark_best_effort(
    state: &TileLoop,
    me: &ProcessIdentity,
    key: &crate::native_guards::WindowKey,
    log_path: &Path,
    window: &str,
) {
    let degraded = |detail: &str| {
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "tile-override",
                "window": window,
                "outcome": "marker-unverified",
                "detail": detail,
            }),
        );
    };
    let Some(held) = hold_marker_target(state, me, key) else {
        // No hold, no write: an already-absent marker is clean; a present
        // one stays for later recovery rather than risking a foreign write.
        if !crate::ffi::is_window(key.hwnd)
            || !crate::product_hide::sys::read_tile_override_marker(key.hwnd)
        {
            return;
        }
        degraded("identity-changed");
        return;
    };
    if !marker_tag_pid_fresh(state, key) {
        let _ = &held;
        degraded("identity-changed");
        return;
    }
    let result = crate::product_hide::sys::remove_tile_override_marker(key.hwnd);
    let fresh = marker_tag_pid_fresh(state, key);
    let _ = &held;
    match result {
        Ok(()) if fresh => {}
        Ok(()) => degraded("identity-changed"),
        Err(error) => degraded(&error.to_string()),
    }
}

/// Hydrate one admitted member's surviving fixed-window tile-override marker
/// into the runtime + durable override sets (R-SPC-04 D7). Read-only: no
/// Engine commit, no marker write, no geometry/focus/topmost effect; the
/// marker stays for later restarts. The full held fence owns every read:
/// [`hold_marker_target`] binds the live process identity (pid, creation,
/// sid/session, exe, medium integrity), scope, hosted-child, proof, and the
/// live lifetime tag plus pid, with a freshness recheck on both sides of the
/// shared-priority marker peeks while the guard stays retained. Anything
/// unverifiable fails closed with no intent and no log: the window simply
/// recomputes ordinary fixed classification with no hold or retry.
/// Missing/degraded/unmatched records return false.
///
/// Returns true exactly when the durable win stands (newly affirmed or
/// already recorded); the `tile-adopted` line logs only on first
/// affirmation so later ticks never re-log. Shared by the slotless-maximized
/// admission site (which must hydrate before any birth probe, latch, clear,
/// slot, or write effect) and the restart-adoption retained pass. Neither
/// caller grants membership, writes, or recovery authority: hydration only
/// classifies rows the admission already accepted.
pub(super) fn hydrate_tile_override_marker(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    key: &crate::native_guards::WindowKey,
    log_path: &Path,
    token: &str,
) -> bool {
    if state.fixed_origin_overrides.contains(key) {
        return true;
    }
    let Some(held) = hold_marker_target(state, me, key) else {
        return false;
    };
    if !marker_tag_pid_fresh(state, key) {
        let _ = &held;
        return false;
    }
    let marker = crate::native_guards::classify_restart_marker(
        crate::product_hide::sys::peek_float_intent_raw(key.hwnd),
        crate::product_hide::sys::peek_sticky_raw(key.hwnd),
        crate::product_hide::sys::peek_tile_override_raw(key.hwnd),
    );
    let fresh = marker_tag_pid_fresh(state, key);
    let _ = &held;
    if !fresh || !matches!(marker, crate::native_guards::RestartMarker::TileOverride) {
        return false;
    }
    state.fixed_tile_overrides.insert(key.clone());
    state.fixed_origin_overrides.insert(key.clone());
    log_json_at(
        log_path,
        serde_json::json!({
            "event": "tile-adopted",
            "window": token,
        }),
    );
    true
}

/// Drop float and sticky runtime state whose membership is gone. No writes,
/// no ledger; the next tick re-derives float rows from the Engine.
/// Classification markers (window properties) are never pruned here: only
/// the runtime maps. A surviving marker re-adopts on the next preamble.
pub(super) fn prune_float_state(state: &mut TileLoop) {
    state
        .float_topmost_prev
        .retain(|key, _| state.member_tokens.contains_key(key));
    state
        .float_rects
        .retain(|token, _| state.member_tokens.values().any(|live| live == token));
    state
        .floated
        .retain(|key| state.member_tokens.contains_key(key));
    state
        .sticky
        .retain(|key, _| state.member_tokens.contains_key(key));
    state
        .restart_reject_hold
        .retain(|key| state.member_tokens.contains_key(key));
    // Fixed-size admission lanes are membership-bound like the float lanes:
    // overrides and automatic origins never outlive their live client, and
    // overlay snapshots never outlive their token. Markers (window
    // properties) die with the window and are never pruned here.
    state
        .fixed_tile_overrides
        .retain(|key| state.member_tokens.contains_key(key));
    state
        .fixed_origin_overrides
        .retain(|key| state.member_tokens.contains_key(key));
    state
        .fixed_auto_seen
        .retain(|key| state.member_tokens.contains_key(key));
    state
        .born_first_exit
        .retain(|key| state.member_tokens.contains_key(key));
    state
        .hint_last_good
        .retain(|key, _| state.member_tokens.contains_key(key));
    state
        .hint_fallback_uses
        .retain(|token, _| state.member_tokens.values().any(|live| live == token));
    state
        .diag_pending
        .retain(|token, _| state.member_tokens.values().any(|live| live == token));
    state
        .diag_emitted
        .retain(|token, _| state.member_tokens.values().any(|live| live == token));
    state
        .diag_class
        .retain(|token, _| state.member_tokens.values().any(|live| live == token));
    state
        .overlay_now
        .retain(|token, _| state.member_tokens.values().any(|live| live == token));
}

/// Drop every runtime table for one dead member: tokens, rects, bands, sticky,
/// identity, tags, hints, and workspace membership. No writes, no ledger.
/// Classification markers (window properties) die with the window and are
/// never pruned here: only the runtime maps.
pub(super) fn drop_member_state(state: &mut TileLoop, key: &crate::native_guards::WindowKey) {
    if let Some(token) = state.member_tokens.remove(key) {
        state.member_rects.remove(&token);
        state.float_rects.remove(&token);
        state.hint_logged.remove(&token);
        state.hint_fallback_uses.remove(&token);
        drop_diag_sample(state, &token);
        drop_visibility_sample(state, &token);
        state.overlay_now.remove(&token);
    }
    if state.restore_wake.as_ref().is_some_and(|w| w.key == *key) {
        state.restore_wake = None;
    }
    state.born_keep_overlay.remove(key);
    state.retile_keep_overlay.remove(key);
    state.born_first_exit.remove(key);
    state.fixed_auto_seen.remove(key);
    state.fixed_origin_overrides.remove(key);
    state.fixed_tile_overrides.remove(key);
    state.float_topmost_prev.remove(key);
    state.floated.remove(key);
    state.sticky.remove(key);
    state.restart_reject_hold.remove(key);
    state.member_identity.remove(key);
    state.member_tags.remove(key);
    state.hint_last_good.remove(key);
    state.vis_diag.uncertain_hides.remove(key);
    state.forget_member(key);
}

/// Bounded world-path hint producer (stage 2b): sample one fresh
/// declared-hint query per bound visible member plus verified hidden and
/// retained-overlay probes, sharing a single aggregate budget. Mirrors the
/// retired row-assembly lanes (visible `min_hint_for`, hidden/retained
/// `hidden_hint_for`, Timeout last-good reuse) with the same native
/// safety fences (identity, lifetime tag, budget); gaming/fullscreen safety
/// stays with the capture classifier (non-rideable rows never carry hints).
/// Syncs `hint_logged` producer-style (empty clears, usable stores) and
/// prunes by live membership. No per-tick hidden enumeration beyond bound
/// members, no global sampling, no logging.
pub(super) fn refresh_world_hints(
    state: &mut TileLoop,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
) {
    let hint_cx = HintCx::new();
    let now = hint_now();
    let sync = |state: &mut TileLoop, token: &str, hints: WindowSizeHints| {
        if hints.is_empty() {
            state.hint_logged.remove(token);
        } else {
            state.hint_logged.insert(token.to_owned(), hints);
        }
    };
    for window in observed {
        let key = crate::native_guards::WindowKey {
            hwnd: window.hwnd,
            pid: window.identity.pid,
            creation: window.identity.process_creation.clone(),
        };
        let Some(bound) = state.member_tokens.get(&key).cloned() else {
            continue;
        };
        if bound != window.token {
            continue;
        }
        let fresh = if hint_cx.over_budget() {
            FreshHint::unperformed(HintOutcome::Budget)
        } else {
            min_hint_for(window, &hint_cx)
        };
        let (hints, _) = fresh_member_hint(state, &key, &bound, fresh, now);
        sync(state, &bound, hints);
    }
    for (key, record) in state.hidden_claims.clone() {
        let Some(token) = state.member_tokens.get(&key).cloned() else {
            continue;
        };
        if record.iconic
            || record.claim.show.minimized
            || state.minimized_omitted.contains(&key)
        {
            continue;
        }
        if state.member_rects.get(&token).is_none() && state.float_rects.get(&token).is_none() {
            continue;
        }
        let fresh = hidden_hint_for(state, &key, &hint_cx);
        let (hints, _) = fresh_member_hint(state, &key, &token, fresh, now);
        sync(state, &token, hints);
    }
    for row in retained {
        let key = &row.key;
        let Some(live) = state.member_tokens.get(key).cloned() else {
            continue;
        };
        if live != row.token {
            continue;
        }
        let first_exit = state.born_first_exit.contains(key);
        if !(first_exit || (row.maximized && !state.hint_logged.contains_key(&row.token))) {
            continue;
        }
        let fresh = hidden_hint_for(state, key, &hint_cx);
        let (hints, _) = fresh_member_hint(state, key, &row.token, fresh, now);
        sync(state, &row.token, hints);
    }
    let live: HashSet<String> = state.member_tokens.values().cloned().collect();
    state
        .hint_logged
        .retain(|token, _| live.contains(token));
    state
        .hint_last_good
        .retain(|key, _| state.member_tokens.contains_key(key));
    state
        .hint_fallback_uses
        .retain(|token, _| live.contains(token));
}

/// One exact-window foreground check plus a single setter with readback.
/// Already-foreground takes no setter. Token-safe outcome only.
fn retain_float_focus(hwnd_u64: u64) -> &'static str {
    let foreground = crate::ffi::foreground_window();
    if foreground == hwnd_u64 {
        return "float-focus-retained";
    }
    if !crate::ffi::set_foreground_window(hwnd_u64) {
        return "float-focus-failed";
    }
    if crate::ffi::foreground_window() == hwnd_u64 {
        "float-focus-retained"
    } else {
        "float-focus-failed"
    }
}

/// Validated float focus retention for the exact toggled window: fresh
/// revalidation holds the target across the prior band calls, then at most
/// one setter with readback. Stale targets fail closed; failures report
/// without claiming success. No timers or retries.
pub(super) fn retain_float_focus_validated(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    expected: &ObservedWindow,
    member_key: &crate::native_guards::WindowKey,
) -> &'static str {
    let member_tag = state.member_tags.get(member_key).map(String::as_str);
    let target = match revalidate_target(
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
        Ok(target) => target,
        Err(_) => return "float-focus-failed",
    };
    let _ = &target.held;
    if !pid_current(target.window.hwnd, target.window.identity.pid) {
        return "float-focus-failed";
    }
    retain_float_focus(target.window.hwnd)
}

/// Record one fresh-query lane Timeout-reuse decision: whether this row's
/// hints rode the last-good fallback, the reused entry's age, the deciding
/// tick, and the lane reason. Read by minimum-hint and per-window
/// diagnostics.
fn note_fallback_use(
    state: &mut TileLoop,
    token: &str,
    reused: bool,
    age_ms: u64,
    reason: &'static str,
) {
    let tick = state.tick;
    state.hint_fallback_uses.insert(
        token.to_owned(),
        HintFallbackUse {
            reused,
            age_ms,
            tick,
            reason,
        },
    );
}

/// Fresh declared-hint query for one live member with unbounded Timeout
/// reuse: a fresh usable hint stores immediately (replacing any previous
/// entry); a successful query with no usable hint clears the stored entry so
/// a later Timeout cannot revive it; unperformed queries (`Invalid`),
/// failures, and budget skips stay unknown without touching the entry; a
/// Timeout reuses the last-good entry while still bound to the live member
/// (token/tag lifecycle plus live lifetime tag, so HWND reuse or a new member
/// never inherits), with no time bound. The native lifetime-tag read runs only
/// when a bound entry exists: absent or identity-moved entries resolve unknown
/// with no extra native read. The returned outcome still reports the fresh
/// query result (a reused Timeout stays Timeout); only the carried hints may
/// ride the fallback. Retained-overlay rows never pass through here, so they
/// neither store nor refresh.
pub(super) fn fresh_member_hint(
    state: &mut TileLoop,
    key: &crate::native_guards::WindowKey,
    token: &str,
    fresh: FreshHint,
    now: Instant,
) -> (WindowSizeHints, HintOutcome) {
    let FreshHint { hints, outcome, .. } = fresh;
    match outcome {
        HintOutcome::Hint => {
            let tag = state.member_tags.get(key).cloned().unwrap_or_default();
            note_last_good(&mut state.hint_last_good, key, token, &tag, hints, now);
            note_fallback_use(state, token, false, 0, "fresh");
            diag_identity_rebound(state, token);
            (hints, outcome)
        }
        HintOutcome::NoHint => {
            invalidate_last_good(&mut state.hint_last_good, key);
            note_fallback_use(state, token, false, 0, "no-hint");
            diag_identity_rebound(state, token);
            (WindowSizeHints::none(), outcome)
        }
        HintOutcome::Invalid => {
            note_fallback_use(state, token, false, 0, "invalid");
            (WindowSizeHints::none(), outcome)
        }
        HintOutcome::Timeout => {
            let stored_tag = state.member_tags.get(key).cloned().unwrap_or_default();
            // Lifecycle-moved entries drop inside the candidate lookup: an
            // entry that existed but no longer binds this token/tag means the
            // member moved on, so the old diagnostic identity breaks too.
            let had_entry = state.hint_last_good.contains_key(key);
            let candidate =
                timeout_candidate(&mut state.hint_last_good, key, token, &stored_tag, now);
            match candidate {
                Ok((cached, age_ms)) => {
                    let live = live_member_tag(key.hwnd);
                    if crate::native_guards::visible_lifetime_ok(&stored_tag, live.as_deref()) {
                        note_fallback_use(state, token, true, age_ms, "reused");
                        diag_identity_rebound(state, token);
                        (cached, outcome)
                    } else {
                        invalidate_last_good(&mut state.hint_last_good, key);
                        note_fallback_use(state, token, false, 0, "identity");
                        diag_identity_broke(state, token);
                        (WindowSizeHints::none(), outcome)
                    }
                }
                Err(reason) => {
                    note_fallback_use(state, token, false, 0, reason);
                    if had_entry {
                        diag_identity_broke(state, token);
                    }
                    (WindowSizeHints::none(), outcome)
                }
            }
        }
        HintOutcome::Failed => {
            note_fallback_use(state, token, false, 0, "failed");
            (WindowSizeHints::none(), outcome)
        }
        HintOutcome::Budget => {
            note_fallback_use(state, token, false, 0, "budget");
            (WindowSizeHints::none(), outcome)
        }
    }
}
