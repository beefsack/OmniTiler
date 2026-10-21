#![forbid(unsafe_code)]
use super::focus_actuation::overlay_expected;
use super::*;

/// Auto-retire positively restored owned-fullscreen markers on verified
/// workspace members through the shared exact-lifetime sequence, so no
/// special clear action is needed to trigger it. Global game/elevated gate
/// only: per-action home/focus do not apply to background retirement, but
/// every candidate still passes the exact member/window-tag/held-identity/
/// scope proof inside the sequence plus the double live restoration proof.
/// Metadata removal only, never a geometry write; fullscreen, transitional,
/// unreadable, recycled, and zero-bit shapes keep their markers. Candidates
/// need verified workspace membership; `overlay_expected` supplies the
/// retained-aware expectation the pin requires.
pub(super) fn sweep_retire_stale_fullscreen(
    effects: &mut dyn UnfloatClearEffects,
    state: &mut TileLoop,
    me: &ProcessIdentity,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    fulls: &[Rect],
) {
    if effects.suspend_block(state, me, fulls) || effects.elevated(me) {
        return;
    }
    let candidates: Vec<(crate::native_guards::WindowKey, String)> = state
        .member_tokens
        .iter()
        .map(|(key, token)| (key.clone(), token.clone()))
        .collect();
    for (key, token) in candidates {
        if state.member_home(&key).is_none() {
            continue;
        }
        let Some(expected) = overlay_expected(state, observed, retained, &token) else {
            continue;
        };
        if expected.hwnd != key.hwnd
            || expected.identity.pid != key.pid
            || expected.identity.process_creation != key.creation
        {
            continue;
        }
        let pre = effects.fullscreen_meta(key.hwnd);
        if pre.is_none() {
            continue;
        }
        let _ =
            effective_stale_meta_via_effects(effects, state, &key, &expected, me, pre, true, fulls);
    }
}

/// Run one reconcile tick through the single world owner: enumerate once,
/// run the mechanical admission preamble plus visibility diagnostics, then
/// dispatch one observation report (capture/sequence/deliver/apply/ack).
/// Policy (rings, views, homing, history, trailing, closes) is core-owned;
/// the adapter retains only native caches plus the last applied receipts.
pub(super) fn reconcile_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    #[cfg(test)]
    if INJECTED_OBSERVE.with(|slot| slot.borrow().is_some()) {
        state.tick += 1;
        let _ = (me, fulls, areas);
        return;
    }
    state.tick += 1;
    let tick = state.tick;
    let correlation = state.correlation();
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let log_path = state.log_path.clone();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        log_json_at(
            &log_path,
            serde_json::json!({"event":"tick-skip","tick":tick,"cause":"enum-failed"}),
        );
        return;
    };
    {
        let mut native_retire = NativeUnfloatClear;
        sweep_retire_stale_fullscreen(&mut native_retire, state, me, &observed, &retained, fulls);
    }
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    // First-seen maximized windows restore once here: lifetime tags are
    // stamped, and the cleared window converges as eligible next tick.
    clear_maximize_at_admission(state, me, &retained);
    workspace_close_cleanup(state);
    // Bounded normal-level visibility/exception snapshots over the observation
    // just taken: managed visibility/cloak transitions and exception-lane
    // changes with reasons, deduplicated and throttled to one batched event
    // per second each. Steady ticks emit nothing; logging never affects
    // behavior. Opaque tokens only. Suspended ticks never sample here (the
    // loop returns before reconcile), so transitions inside suspension are
    // unsampled by design, never interpolated.
    stage_visibility(state, &observed, &retained, &skipped);
    stage_exceptions(state);
    if let Some(event) = flush_visibility(state) {
        log_json_at(&log_path, event);
    }
    if let Some(event) = flush_exceptions(state) {
        log_json_at(&log_path, event);
    }
    // Structured observer tick before Engine dispatch: full EnumWindows
    // success with the raw enumerated count and the managed (eligible,
    // allowlisted) count, including zero. A zero managed set is a completed
    // observation, not a failure: the Engine may answer Rejected for an empty
    // domain and the nonmoving proof must not require a layout plan.
    let enumerated = state.last_enumerated;
    let managed = observed.len();
    if state.trace {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "observe",
                "tick": tick,
                "op": "reconcile",
                "enumerated": enumerated,
                "managed": managed,
            }),
        );
    }
    if let Some(audit) = state.audit_path.clone() {
        log_json_at(
            &audit,
            serde_json::json!({
                "event": "proof-observe",
                "tick": tick,
                "op": "reconcile",
                "enumerated": enumerated,
                "managed": managed,
            }),
        );
    }
    // One shared observation tick through the single world owner: the core
    // converges every monitor from this tick's screens plus window facts,
    // and the host diff applies every accepted reply with fresh proof.
    // Committed floats join the ordinary float lanes with markers.
    let foreground = crate::ffi::foreground_window();
    if super::world_tick::run_observation_tick(
        state, me, store, dir, &observed, &retained, areas, foreground, fulls,
    )
    .is_some()
    {
        let correlation_str = correlation.as_str();
        super::capacity_floats::adopt_floats_from_reply(
            state,
            me,
            tick,
            correlation_str,
            &observed,
        );
    }
}

/// Narrow observation lane for an unmanageable fullscreen foreground: one
/// observation through the same world owner, with no other effects.
///
/// The unmanageable foreground window itself is unmanaged (never admitted,
/// never homed), so the core authorizes no frame for it; observation-only
/// reports emit no focus effects. Unknown foreground (`0`) refuses with no
/// planning: fail closed to full suspension upstream.
pub(super) fn reconcile_limited_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    foreground_hw: u64,
) {
    if foreground_hw == 0 {
        return;
    }
    state.tick += 1;
    let tick = state.tick;
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let log_path = state.log_path.clone();
    let Some(observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        log_json_at(
            &log_path,
            serde_json::json!({"event":"tick-skip","tick":tick,"cause":"enum-failed"}),
        );
        return;
    };
    let _ = super::world_tick::run_observation_tick(
        state,
        me,
        store,
        dir,
        &observed,
        &retained,
        areas,
        foreground_hw,
        fulls,
    );
}
