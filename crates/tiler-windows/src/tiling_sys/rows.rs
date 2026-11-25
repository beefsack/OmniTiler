#![forbid(unsafe_code)]
use super::*;

/// Drop float runtime state whose membership is gone. No writes, no ledger;
/// the next tick re-derives float rows from the canonical intent snapshot.
/// Classification intent lives in the window's own packed slot and is never
/// pruned here: only the runtime maps, and the snapshot refresh re-reads the
/// slot for every surviving member.
pub(super) fn prune_float_state(state: &mut TileLoop) {
    state
        .float_rects
        .retain(|token, _| state.member_tokens.values().any(|live| live == token));
    // Fixed-size admission lanes are membership-bound like the float lanes:
    // automatic origins never outlive their live client, and overlay
    // snapshots never outlive their token. Intent markers (window
    // properties) die with the window and are never pruned here.
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
    state.born_first_exit.remove(key);
    state.fixed_auto_seen.remove(key);
    // The canonical intent snapshot never outlives membership: the next
    // tick's refresh would prune it anyway, and keeping it here means no
    // stale sticky/float/tile classification survives a same-tick drop or
    // a same-process reuse rebind. The packed slot itself is untouched
    // (properties die with the window; a surviving tag re-reads it).
    state.intent_snapshot.remove(key);
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
    let sync = |state: &mut TileLoop, token: &str, hints: WindowSizeHints, outcome: HintOutcome| {
        if !hints.is_empty() {
            state.hint_logged.insert(token.to_owned(), hints);
        } else if outcome == HintOutcome::NoHint {
            // A successful query with no usable constraint is truthful
            // empty, distinct from unknown: presence carries the empty
            // value so v4 capture can publish `Some(none())`.
            state
                .hint_logged
                .insert(token.to_owned(), WindowSizeHints::none());
        } else {
            state.hint_logged.remove(token);
        }
    };
    for window in observed {
        let key = crate::native_guards::WindowKey {
            hwnd: window.hwnd,
            pid: window.identity.pid,
            creation: window.identity.process_creation.clone(),
        };
        // Query every observed row, not only already-bound members: v4
        // captures hints on the same observation that admits a newcomer, so
        // the first desired row can carry truthful `Some(none())` when the
        // native query proves no usable constraint. Bound members keep
        // their stable token; newcomers carry the observed token.
        let token = state
            .member_tokens
            .get(&key)
            .cloned()
            .unwrap_or_else(|| window.token.clone());
        let fresh = if hint_cx.over_budget() {
            FreshHint::unperformed(HintOutcome::Budget)
        } else {
            min_hint_for(window, &hint_cx)
        };
        let (hints, outcome) = fresh_member_hint(state, &key, &token, fresh, now);
        sync(state, &token, hints, outcome);
    }
    for (key, record) in state.hidden_claims.clone() {
        let Some(token) = state.member_tokens.get(&key).cloned() else {
            continue;
        };
        if record.iconic || record.claim.show.minimized || state.minimized_omitted.contains(&key) {
            continue;
        }
        if !state.member_rects.contains_key(&token) && !state.float_rects.contains_key(&token) {
            continue;
        }
        let fresh = hidden_hint_for(state, &key, &hint_cx);
        let (hints, outcome) = fresh_member_hint(state, &key, &token, fresh, now);
        sync(state, &token, hints, outcome);
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
        let (hints, outcome) = fresh_member_hint(state, key, &row.token, fresh, now);
        sync(state, &row.token, hints, outcome);
    }
    let mut live: HashSet<String> = state.member_tokens.values().cloned().collect();
    live.extend(observed.iter().map(|window| window.token.clone()));
    live.extend(retained.iter().map(|row| row.token.clone()));
    state.hint_logged.retain(|token, _| live.contains(token));
    state
        .hint_last_good
        .retain(|key, _| state.member_tokens.contains_key(key));
    state
        .hint_fallback_uses
        .retain(|token, _| live.contains(token));
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
