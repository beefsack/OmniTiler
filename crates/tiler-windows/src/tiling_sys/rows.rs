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
    key: &crate::workspace::WindowKey,
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
        if !crate::workspace_owner::visible_lifetime_ok(stored_tag, tag.as_deref()) {
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
    if !crate::workspace_owner::visible_lifetime_ok(stored_tag, live_tag.as_deref()) {
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
    key: &crate::workspace::WindowKey,
) -> Option<HeldProcess> {
    hold_marker_target(state, me, key)
}

/// Fresh pid plus lifetime-tag recheck for one classification-marker write:
/// the live member tag must still equal the stored tag and the HWND must
/// still resolve to the stored pid. Call immediately before the `SetProp` /
/// `RemoveProp` under the held process and again after the readback.
pub(super) fn marker_tag_pid_fresh(state: &TileLoop, key: &crate::workspace::WindowKey) -> bool {
    state.member_tags.get(key).is_some_and(|stored| {
        crate::workspace_owner::visible_lifetime_ok(
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
    key: &crate::workspace::WindowKey,
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
    key: &crate::workspace::WindowKey,
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
    key: &crate::workspace::WindowKey,
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
    key: &crate::workspace::WindowKey,
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
    key: &crate::workspace::WindowKey,
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
    let marker = crate::workspace_owner::classify_restart_marker(
        crate::product_hide::sys::peek_float_intent_raw(key.hwnd),
        crate::product_hide::sys::peek_sticky_raw(key.hwnd),
        crate::product_hide::sys::peek_tile_override_raw(key.hwnd),
    );
    let fresh = marker_tag_pid_fresh(state, key);
    let _ = &held;
    if !fresh || !matches!(marker, crate::workspace_owner::RestartMarker::TileOverride) {
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
pub(super) fn drop_member_state(state: &mut TileLoop, key: &crate::workspace::WindowKey) {
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
    state.workspaces.remove_window(key);
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
    member_key: &crate::workspace::WindowKey,
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

type DesiredEntry = crate::workspace_owner::PlannedWrite;

pub(super) fn desired_entries(reply: &CoreReply) -> Option<Vec<DesiredEntry>> {
    crate::workspace_owner::planned_writes(reply)
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
fn fresh_member_hint(
    state: &mut TileLoop,
    key: &crate::workspace::WindowKey,
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
                    if crate::workspace_owner::visible_lifetime_ok(&stored_tag, live.as_deref()) {
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

/// Categorical cause for one viewless member that defers domain assembly.
/// Read-only native probes (no writes, no membership mutation); unknown and
/// unreadable evidence maps to hold causes, never to a fabricated row.
fn deferral_cause(
    state: &TileLoop,
    key: &crate::workspace::WindowKey,
    retained: &[RetainedRow],
) -> &'static str {
    if state.hidden_claims.contains_key(key)
        || state.hidden_claims.keys().any(|k| k.hwnd == key.hwnd)
    {
        return "hidden-nosnapshot";
    }
    if state.vis_diag.uncertain_hides.contains(key)
        || state
            .vis_diag
            .uncertain_hides
            .iter()
            .any(|k| k.hwnd == key.hwnd)
    {
        return "uncertain-hide";
    }
    if state.minimized_omitted.contains(key) || live_is_iconic(key.hwnd) {
        return "minimized";
    }
    if !state.last_hwnds.contains(&key.hwnd) {
        return "absent";
    }
    match live_cloaked(key.hwnd) {
        None => return "unreadable-cloak",
        Some(true) => return "cloaked",
        Some(false) => {}
    }
    if live_is_visible(key.hwnd) {
        if retained.iter().any(|row| row.key == *key) {
            return "retained-no-view";
        }
        let stored_ok = state.member_tags.get(key).is_some_and(|stored| {
            !stored.is_empty()
                && crate::workspace_owner::visible_lifetime_ok(
                    stored,
                    live_member_tag(key.hwnd).as_deref(),
                )
        });
        let pid = live_window_pid(key.hwnd);
        if !stored_ok || pid == 0 || pid != key.pid {
            return "unreadable-identity";
        }
        return "visible-no-view";
    }
    let stored_ok = state.member_tags.get(key).is_some_and(|stored| {
        !stored.is_empty()
            && crate::workspace_owner::visible_lifetime_ok(
                stored,
                live_member_tag(key.hwnd).as_deref(),
            )
    });
    let pid = live_window_pid(key.hwnd);
    if !stored_ok || pid == 0 || pid != key.pid {
        return "unreadable-identity";
    }
    if !live_is_window(key.hwnd) {
        return "not-window";
    }
    "external-hide-pending"
}

/// Normal-level transition-only log for one domain deferral or recovery.
/// Deferrals emit on first sight and on reason change with the actual
/// categorical causes (`token:cause` sorted); recovery emits once when a
/// previously deferred domain assembles complete. Identical repeats stay
/// silent. Opaque tokens only, never HWNDs, pids, paths, or titles.
fn note_domain_deferral(
    state: &mut TileLoop,
    output: &str,
    workspace: &str,
    op: &str,
    correlation: &str,
    missing: &[(String, &'static str)],
    deferred: bool,
) {
    let domain = (output.to_owned(), workspace.to_owned());
    if !deferred {
        if state.vis_diag.defer_last.remove(&domain).is_some() {
            let output_token = state.workspaces.output_token(output);
            let workspace_token = state.workspaces.workspace_token(output, workspace);
            log_json_at(
                &state.log_path.clone(),
                serde_json::json!({
                    "event": "reconcile-recovered",
                    "tick": state.tick,
                    "op": op,
                    "correlation": correlation,
                    "output": output_token,
                    "workspace": workspace_token,
                }),
            );
        }
        return;
    }
    let mut parts: Vec<String> = missing
        .iter()
        .map(|(token, cause)| format!("{token}:{cause}"))
        .collect();
    parts.sort();
    let reason = parts.join(",");
    let changed = state
        .vis_diag
        .defer_last
        .get(&domain)
        .is_none_or(|last| *last != reason);
    if !changed {
        return;
    }
    state.vis_diag.defer_last.insert(domain, reason.clone());
    let output_token = state.workspaces.output_token(output);
    let workspace_token = state.workspaces.workspace_token(output, workspace);
    log_json_at(
        &state.log_path.clone(),
        serde_json::json!({
            "event": "reconcile-deferred",
            "tick": state.tick,
            "op": op,
            "correlation": correlation,
            "output": output_token,
            "workspace": workspace_token,
            "reason": reason,
        }),
    );
}

/// Assemble complete Engine rows for one `(output, workspace)` domain: the
/// union of eligible visible members, retained-occupancy members with fresh
/// frames, frame-less retained members with last-known snapshots, and hidden
/// snapshots. Refreshes the member token/rect/identity tables for every row
/// with a fresh read.
///
/// Eligible visible members carry a fresh application-declared minimum-size
/// hint: one bounded `WM_GETMINMAXINFO` query per member converted with its
/// currently measured frame insets. Verified managed hidden members carry a
/// fresh hint the same way (fresh identity fences plus fresh `GetWindowRect`
/// and `DWMWA_EXTENDED_FRAME_BOUNDS` for current insets, never shown or
/// moved); a failed hidden measurement is unknown, never a stale inset.
/// Retained tiled maximized/fullscreen members ride their last-known declared
/// hint (lifetime-bound by Engine token, never derived from the maximized
/// frame); minimized without a frame, cloaked, float, and born-hold rows
/// carry no hint. `hint_cx` shares one
/// aggregate deadline across every domain in
/// the operation, so a send's source plus target never independently blow
/// the per-operation budget; remaining members report unknown with no hint.
/// Every Engine-building path funnels through here, so reconcile, directional
/// actions, workspace send/select, and post-admission ticks all carry hints;
/// hint-only changes still reach projection because no caller skips the
/// Engine on an unchanged rectangle fingerprint.
///
/// Returns `None` when any member lacks a known snapshot: every Engine
/// handle path must defer with retained state instead of converging a
/// falsely complete observation that would drop membership and layout.
#[allow(clippy::too_many_arguments)]
pub(super) fn assemble_domain_rows(
    state: &mut TileLoop,
    output: &str,
    workspace: &str,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    op: &str,
    correlation: &str,
    hint_cx: &mut HintCx,
) -> Option<Vec<crate::workspace_owner::OwnerRow>> {
    let members = state.workspaces.workspace_members(output, workspace);
    let by_token: HashMap<&str, &ObservedWindow> =
        observed.iter().map(|w| (w.token.as_str(), w)).collect();
    let mut views = Vec::new();
    let query_start = Instant::now();
    // One clock read per assembly: every last-good store shares the pre-query
    // tick time, which feeds diagnostic age only and never expires anything.
    let now = hint_now();
    let mut stats = HintStats::default();
    let mut reasons: HashMap<String, &'static str> = HashMap::new();
    // Actual native-query cost per token, recorded from the query seam
    // itself (offered timeout recomputed at the native call, elapsed around
    // the call only): zeroed when budget, identity, or measurement prework
    // refuses before any native query.
    let mut costs: HashMap<String, QueryCost> = HashMap::new();
    // Intentional-float tokens in this domain session: slotless floating
    // rows on the live native frame, no tiled writes, `member_rects` frozen
    // while `float_rects` tracks the draggable frame.
    let domain_key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(output.to_owned()),
        workspace: tiler_core::directional::WorkspaceId(workspace.to_owned()),
    };
    let float_tokens: HashSet<String> = state
        .engine
        .session(&domain_key)
        .map(|session| {
            members
                .iter()
                .filter_map(|key| state.member_tokens.get(key))
                .filter(|token| session.is_exception(&WindowId(token.to_string())))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    // Runtime intent carries the exception across domain releases (which drop
    // every Engine exception) and boundary-send fresh adoption: a released
    // then retiled workspace re-adopts its intentional floats on the next
    // observation instead of tiling them. Lifetime keyed, pruned with
    // membership; sticky/born-fullscreen keep their own lanes.
    let mut float_tokens = float_tokens;
    for token in
        crate::workspace_owner::float_carry_tokens(&members, &state.member_tokens, &state.floated)
    {
        float_tokens.insert(token);
    }
    // Observation-scoped restart-hydration write barrier: rejected staged
    // floats ride the slotless floating lane (live frame, no hint, no tile
    // slot) until the next observation revalidates, so the same tick's
    // reconcile and action writers cannot tile them. Lifetime-keyed like
    // the intent lane above; cleared and rebuilt by every preamble.
    for token in crate::workspace_owner::float_carry_tokens(
        &members,
        &state.member_tokens,
        &state.restart_reject_hold,
    ) {
        float_tokens.insert(token);
    }
    // A slotless maximized row on a floating domain keeps no tile slot, so
    // the R-MAX-03 retile latch still sees it slotless on retile and reserves
    // the overlay slot then. Unknown domains seed as today (assembly implies
    // a live domain).
    let domain_tiled = workspace_mode_known(state, output, workspace).unwrap_or(true);
    for key in &members {
        // Uncertain-hide hold: a member whose durable ledger claim may be kept
        // but whose owner-table entry is missing never takes a row, even when
        // observed visible. Visibility alone never resolves ownership; only
        // authoritative ledger absence clears. This defers with the actual
        // uncertain-hide cause across visible recovery and a second hide.
        if state.vis_diag.uncertain_hides.contains(key)
            || state
                .vis_diag
                .uncertain_hides
                .iter()
                .any(|k| k.hwnd == key.hwnd)
        {
            continue;
        }
        let born = state.born_fullscreen.contains(key);
        if state.workspaces.is_hidden(key) {
            // D23 minimize omission through the hidden lane: a hidden member
            // iconic at hide time (durable `HiddenRecord.iconic`, the only
            // minimize signal for windows the raw observation gate skips as
            // invisible) or observed iconic while hidden omits its row, so
            // convergence stores the `MinimizedSlot` instead of consuming it
            // on hide. Non-iconic hidden members keep their snapshot rows.
            let iconic_hidden = state
                .hidden_claims
                .get(key)
                .is_some_and(|record| record.iconic)
                || state.minimized_omitted.contains(key);
            if iconic_hidden {
                continue;
            }
            let token = state.member_tokens.get(key).cloned();
            let rect = token
                .as_ref()
                .and_then(|t| {
                    crate::workspace_owner::hidden_snapshot_rect(
                        float_tokens.contains(t),
                        state.float_rects.get(t).copied(),
                        state.member_rects.get(t).copied(),
                    )
                })
                // Slotless members carry no snapshot yet: fall back to the
                // fresh retained frame so the observation stays complete
                // without seeding a tile slot. No insert here, ever.
                .or_else(|| retained.iter().find(|r| r.key == *key).and_then(|r| r.rect));
            if let (Some(token), Some(rect)) = (token, rect) {
                let is_float = float_tokens.contains(&token);
                let fresh = hidden_hint_for(state, key, hint_cx);
                costs.insert(token.clone(), fresh.cost);
                let (hints, outcome) = fresh_member_hint(state, key, &token, fresh, now);
                stats.note(outcome);
                reasons.insert(token.clone(), outcome.as_str());
                // R-SPC-04 D5: a first exit rides tiled (`floating = false`)
                // carrying exactly the fresh hidden query output (possibly
                // none), like the live and retained lanes. Explicit sticky
                // and intentional-float lanes keep their float rows.
                let first_exit_hidden = state.born_first_exit.contains(key)
                    && !state.sticky.contains_key(key)
                    && !state.floated.contains(key);
                views.push(crate::workspace_owner::MemberView {
                    key: key.clone(),
                    token,
                    rect,
                    hints,
                    floating: (born || is_float) && !first_exit_hidden,
                });
            }
            continue;
        }
        // Eligible visible read first. Out-of-scope members still ride a row
        // so the observation stays complete: the scope fence lives in
        // `writable_tokens` (no geometry writes), `workspace_hide_one`, and
        // the select/send dispatchers (no hides), never in row assembly
        // (where a missing view would stall convergence as `deferred`).
        if let Some(token) = state.member_tokens.get(key).cloned()
            && let Some(window) = by_token.get(token.as_str())
        {
            // R-SPC-04 D5: a first born-fullscreen exit (non-sticky) takes
            // the tiled lane below with exactly the fresh query output
            // (possibly none), so the classifier floats fixed clients and
            // tiles everything else admission-only. Sticky first exits keep
            // the sticky float lane below. Marks are consumed only on
            // committed classification (see
            // `consume_classified_first_exits`), never here.
            let first_exit = state.born_first_exit.contains(key) && !state.sticky.contains_key(key);
            // Slotless floating row on the live native frame, no hint. The
            // tiled allocation in `member_rects` stays frozen. Engine-known
            // floats, runtime-intent floats, D4-latched fixed max-births, and
            // sticky members ride here; adoption commits the Engine exception
            // inline. D4-latched rows take no tile slot here, ever.
            if state.sticky.contains_key(key)
                || state.fixed_auto_seen.contains(key)
                || (float_tokens.contains(&token) && !first_exit)
            {
                // Automatic fixed floats hold no reserved tile slot: drop any
                // first-contact seed so later passes never mistake it for an
                // allocation. Intentional floats keep their frozen slot for
                // unfloat restore. Both ride the live frame with no hint.
                state.float_rects.insert(token.clone(), window.visible);
                if is_automatic_in(state, &domain_key, &token) {
                    state.member_rects.remove(&token);
                }
                views.push(crate::workspace_owner::MemberView {
                    key: key.clone(),
                    token: token.clone(),
                    rect: window.visible,
                    hints: WindowSizeHints::none(),
                    floating: true,
                });
                continue;
            }
            state.member_rects.insert(token.clone(), window.visible);
            if let Some(identity) = state.member_identity.get_mut(key) {
                identity.pid = window.identity.pid;
                identity.process_creation = window.identity.process_creation.clone();
                identity.exe_path = window.identity.exe_path.clone();
                identity.user_sid = window.identity.user_sid.clone();
                identity.session_id = window.identity.session_id;
            }
            let fresh = if hint_cx.over_budget() {
                FreshHint::unperformed(HintOutcome::Budget)
            } else {
                min_hint_for(window, hint_cx)
            };
            costs.insert(token.clone(), fresh.cost);
            let (hints, outcome) = fresh_member_hint(state, key, &token, fresh, now);
            stats.note(outcome);
            reasons.insert(token.clone(), outcome.as_str());
            views.push(crate::workspace_owner::MemberView {
                key: key.clone(),
                token: token.clone(),
                rect: window.visible,
                hints,
                floating: false,
            });
            continue;
        }
        // Retained occupancy: an overlaid (maximized/fullscreen) member rides
        // its last-known tile rectangle, never the compositor-owned native
        // maximum frame, so tile topology and sibling shares survive the
        // overlay; first sightings fall back to the fresh frame. Retained rows
        // carry no hint and never take writes. Born-held fullscreen members
        // are the slotless exception: they ride the fresh row rect with a
        // floating Engine observation (siblings keep the full tile area) and
        // never write a canonical slot into `member_rects`; the admission
        // hidden-snapshot seed stays untouched for a later hide. Existing
        // skip semantics preserved.
        if let Some(row) = retained.iter().find(|r| r.key == *key) {
            // D23 minimize omission: a frameless non-overlay row for a key
            // observed iconic this tick carries no Engine row. Convergence
            // releases the leaf and stores the `MinimizedSlot`; the frozen
            // `member_rects` slot is left untouched (never refreshed, never
            // carried). Overlay rows (maximized/fullscreen) keep their lane.
            if state.minimized_omitted.contains(key)
                && row.rect.is_none()
                && !row.maximized
                && !row.fullscreen
            {
                continue;
            }
            if born && row.fullscreen {
                if let Some(rect) = row.rect {
                    views.push(crate::workspace_owner::MemberView {
                        key: key.clone(),
                        token: row.token.clone(),
                        rect,
                        hints: WindowSizeHints::none(),
                        floating: true,
                    });
                }
                continue;
            }
            // R-SPC-04 D5: a first born-fullscreen exit (non-sticky) takes
            // the tiled lane below with exactly the fresh query output
            // (possibly none), so the classifier floats fixed clients and
            // tiles everything else admission-only. Marks are consumed only
            // on committed classification (see
            // `consume_classified_first_exits`), never here.
            let first_exit = state.born_first_exit.contains(key) && !state.sticky.contains_key(key);
            if (float_tokens.contains(&row.token) && !first_exit)
                || (state.fixed_auto_seen.contains(key) && !first_exit)
            {
                // Overlaid float (user maximized/fullscreened it natively) or
                // frameless: slotless floating row on the fresh frame, else
                // the last float snapshot, with no hint. Unfloat refuses
                // until the frame reads normal again. D4-latched fixed
                // max-births ride here too: floating beneath native maximize
                // with no tile slot, never a clear.
                let snapshot = state.float_rects.get(&row.token).copied();
                let kept = state.member_rects.get(&row.token).copied();
                let rect = match row.rect {
                    Some(fresh) => Some(crate::tiling::canonical_retained_rect(
                        true,
                        fresh,
                        snapshot.or(kept),
                    )),
                    None => snapshot.or(kept),
                };
                if let Some(rect) = rect {
                    state.float_rects.insert(row.token.clone(), rect);
                    if is_automatic_in(state, &domain_key, &row.token) {
                        state.member_rects.remove(&row.token);
                    }
                    views.push(crate::workspace_owner::MemberView {
                        key: key.clone(),
                        token: row.token.clone(),
                        rect,
                        hints: WindowSizeHints::none(),
                        floating: true,
                    });
                }
                continue;
            }
            let overlay = row.maximized || row.fullscreen;
            let kept = state.member_rects.get(&row.token).copied();
            let rect = match row.rect {
                Some(fresh) => Some(crate::tiling::canonical_retained_rect(overlay, fresh, kept)),
                None => kept,
            };
            // D5: a first exit classifies on exactly the fresh query output
            // (possibly none, which tiles non-fixed admission-only). No
            // fallback, no hold, no skip: unknown never classifies and the
            // mark consumes after the committed result. A maximized row
            // with no hint evidence at all probes the same way, so a fresh
            // max candidate classifies before any clear; ordinary overlay
            // rows keep their last-known hint with no probe.
            let token_matches = state
                .member_tokens
                .get(key)
                .is_some_and(|live| live == &row.token);
            let (hints, reason) =
                if first_exit || (row.maximized && !state.hint_logged.contains_key(&row.token)) {
                    let fresh = hidden_hint_for(state, key, hint_cx);
                    costs.insert(row.token.clone(), fresh.cost);
                    let (hints, outcome) = fresh_member_hint(state, key, &row.token, fresh, now);
                    stats.note(outcome);
                    (hints, outcome.as_str())
                } else {
                    let retained = crate::tiling::retained_overlay_hint(
                        state.hint_logged.get(&row.token).copied(),
                        token_matches,
                        overlay,
                        float_tokens.contains(&row.token),
                        born,
                        true,
                    );
                    let reason = if retained.is_empty() {
                        "retained"
                    } else {
                        "retained-hint"
                    };
                    (retained, reason)
                };
            if let Some(rect) = rect {
                // Slotless maxima on floating domains stay slotless (flag
                // above); every other row seeds or refreshes as before, with
                // one D6 exception: a slotless maximized row pending its
                // one-shot clear must not plant a slot that disables the
                // required clear on the next pass. While the D2 pin holds
                // (Engine admitted it ordinary/tiled) or its fresh probe reads
                // non-fixed/unknown (clear pending, including budget-unknown),
                // ride slotless so the next clear still sees no slot; seed
                // only for fresh-fixed (floats via the Engine), keep-overlay
                // (needs the reserved slot), retile-overlay (R-MAX-03 reserves
                // the same slot under both born values), attempted (one-shot
                // done), ineligible/fullscreen/floating (never clear), or rows
                // that already hold a slot. Durable fixed-origin overrides never
                // defer: they reserve the maximized slot up front (no clear
                // is ever planned for them). No hold or retry is invented: a
                // deferred row re-probes next tick.
                let defer_for_pending_clear = row.maximized
                    && !row.fullscreen
                    && !state.fixed_origin_overrides.contains(key)
                    && !state.member_rects.contains_key(&row.token)
                    && domain_tiled
                    && !state.born_keep_overlay.contains(key)
                    && !state.retile_keep_overlay.contains(key)
                    && row
                        .facts
                        .is_some_and(|facts| classify_focus(&facts).is_ok())
                    && {
                        let prefix =
                            format!("{}:{}:{}:", row.key.hwnd, row.key.creation, row.key.pid);
                        !state
                            .maximize_admission_attempted
                            .iter()
                            .any(|attempted| attempted.starts_with(&prefix))
                    }
                    && (committed_ordinary_pin(state, key, &row.token)
                        || !tiler_core::size_hints::is_fixed_size_with(
                            hints,
                            fixed_predicate_for(state.settings_live.as_ref()),
                        ));
                if tiler_core::window_policy::should_seed_member_slot(
                    domain_tiled,
                    row.maximized,
                    state.member_rects.contains_key(&row.token),
                ) && !defer_for_pending_clear
                {
                    state.member_rects.insert(row.token.clone(), rect);
                }
                // Retained tiled overlay keeps its last-known declared hint
                // so min-bound siblings stay stable; minimized/frameless,
                // cloaked, float, and born-hold rows stay hintless.
                reasons.insert(row.token.clone(), reason);
                views.push(crate::workspace_owner::MemberView {
                    key: key.clone(),
                    token: row.token.clone(),
                    rect,
                    hints,
                    floating: false,
                });
            }
        }
    }
    // D23 minimize omission: a member with no view that was observed
    // iconic this tick, or that hides an iconic claim, is not required in
    // the carried observation, so the Engine releases its leaf (storing the
    // `MinimizedSlot`) instead of stalling as `deferred`. Any other viewless
    // member still defers with retained Engine state: a falsely complete
    // observation would drop membership and layout, and unreadable evidence
    // must never fabricate an empty row.
    let required: std::collections::BTreeSet<crate::workspace::WindowKey> = members
        .iter()
        .filter(|key| {
            views.iter().any(|view| &view.key == *key)
                || (!state.minimized_omitted.contains(*key)
                    && !state
                        .hidden_claims
                        .get(*key)
                        .is_some_and(|record| record.iconic))
        })
        .cloned()
        .collect();
    let Some(mut rows) = crate::workspace_owner::domain_rows(&required, &views) else {
        let view_keys: std::collections::BTreeSet<&crate::workspace::WindowKey> =
            views.iter().map(|view| &view.key).collect();
        let mut missing: Vec<(String, &'static str)> = Vec::new();
        for key in required.iter() {
            if view_keys.contains(key) {
                continue;
            }
            let token = state
                .member_tokens
                .get(key)
                .cloned()
                .unwrap_or_else(|| "unknown".to_owned());
            missing.push((token, deferral_cause(state, key, retained)));
        }
        missing.sort();
        note_domain_deferral(state, output, workspace, op, correlation, &missing, true);
        return None;
    };
    note_domain_deferral(state, output, workspace, op, correlation, &[], false);
    rows.sort_by(|a, b| a.token.cmp(&b.token));
    let elapsed = query_start.elapsed();
    let output_token = state.workspaces.output_token(output);
    let workspace_token = state.workspaces.workspace_token(output, workspace);
    stage_hint_samples(
        state,
        &output_token,
        &workspace_token,
        op,
        correlation,
        &rows,
        &reasons,
        &costs,
        observed,
        elapsed.as_millis().min(u128::from(u64::MAX)) as u64,
    );
    log_min_hint_summary(
        state,
        op,
        correlation,
        output,
        workspace,
        &rows,
        &stats,
        &reasons,
        elapsed,
    );
    Some(rows)
}

/// Bounded minimum-hint observation summary for one row assembly: query
/// outcomes plus the tokens whose fresh hint differs from the last logged
/// value, threaded on the real operation correlation (`tick-N` for reconcile,
/// `act-N` for select/send/directional) with opaque output/workspace tokens
/// so the change joins to the projection adjustment and the
/// `client-clamped` skips (plus overconstrained min-clamped targets) in the
/// existing apply summaries
/// by `(correlation, output, workspace, window)`. Per-window hint values and
/// per-token query reasons ride the trace log only; the normal log carries
/// counts plus changes so steady state stays quiet. The dedupe cache is
/// pruned by actual live membership (every member token across
/// all domains), never by the current domain alone, so multi-output and send
/// domains never re-log unchanged hints. Rows feed the Engine; the cache
/// additionally feeds the overconstrained native target (`max(planned, min)`).
/// Retained overlay rows reuse the cached hint, so syncing here keeps (never
/// drops) that reused value.
#[allow(clippy::too_many_arguments)]
fn log_min_hint_summary(
    state: &mut TileLoop,
    op: &str,
    correlation: &str,
    output: &str,
    workspace: &str,
    rows: &[crate::workspace_owner::OwnerRow],
    stats: &HintStats,
    reasons: &HashMap<String, &'static str>,
    elapsed: Duration,
) {
    let tick = state.tick;
    let mut changed: Vec<String> = Vec::new();
    for row in rows {
        match state.hint_logged.get(&row.token) {
            Some(previous) if *previous == row.hints => {}
            _ => {
                if !row.hints.is_empty() || state.hint_logged.contains_key(&row.token) {
                    changed.push(row.token.clone());
                }
                if row.hints.is_empty() {
                    state.hint_logged.remove(&row.token);
                } else {
                    state.hint_logged.insert(row.token.clone(), row.hints);
                }
            }
        }
    }
    // Prune by actual live membership identity across all domains, not by the
    // current domain's rows: multi-output ticks and send source/target pairs
    // must not evict (and re-log) each other's unchanged hints. The
    // Timeout-reuse cache and its per-token report prune the same way, so
    // last-good state never outlives its live member.
    let live: HashSet<String> = state.member_tokens.values().cloned().collect();
    state
        .hint_logged
        .retain(|token, _| live.contains(token) || rows.iter().any(|row| row.token == *token));
    state
        .hint_last_good
        .retain(|key, _| state.member_tokens.contains_key(key));
    state
        .hint_fallback_uses
        .retain(|token, _| live.contains(token));
    changed.sort();
    if changed.is_empty() && !state.trace {
        return;
    }
    let log_path = state.log_path.clone();
    let output_token = state.workspaces.output_token(output);
    let workspace_token = state.workspaces.workspace_token(output, workspace);
    log_json_at(
        &log_path,
        serde_json::json!({
            "event": "min-hints",
            "tick": tick,
            "correlation": correlation,
            "op": op,
            "output": output_token,
            "workspace": workspace_token,
            "queried": stats.queried,
            "with_hint": stats.with_hint,
            "unknown": stats.queried.saturating_sub(stats.with_hint).saturating_add(stats.budget_skipped),
            "timeout": stats.timeout,
            "failed": stats.failed,
            "invalid": stats.invalid,
            "budget_skipped": stats.budget_skipped,
            "query_ms": elapsed.as_millis().min(u128::from(u64::MAX)) as u64,
            "changed": changed,
        }),
    );
    if state.trace {
        let detail: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "window": row.token,
                    "min": [row.hints.min_w, row.hints.min_h],
                    "reason": reasons.get(&row.token).copied().unwrap_or("retained"),
                })
            })
            .collect();
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "min-hints-detail",
                "tick": tick,
                "correlation": correlation,
                "op": op,
                "output": output_token,
                "workspace": workspace_token,
                "entries": detail,
            }),
        );
    }
}
