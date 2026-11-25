#![forbid(unsafe_code)]
use super::*;

/// Auto-retire owned fullscreen markers whose chrome the application has
/// already restored, so no clear action is needed to trigger cleanup.
///
/// This is the Task 8 legacy-policy retirement's one intended survivor:
/// core-only observation never mints a fullscreen exit (commands do), so a
/// marker left behind after an external restore would otherwise keep
/// claiming tiler ownership, be replayed over the window by a later explicit
/// exit, and survive as a durable R12 row that a future startup could restore
/// over already-normal chrome. Metadata removal only: no style, frame,
/// activation or z-order write runs here, and a fullscreen/transitional/
/// unreadable/recycled/drifted shape keeps its marker. Candidates need a
/// verified workspace home and a fresh observed or retained row whose live
/// fullscreen fact is positively false; the retirement re-proves the exact
/// lifetime, style restoration and marker equality immediately before
/// clearing (see [`super::fullscreen::retire_restored_fullscreen`]).
pub(super) fn sweep_retire_restored_fullscreen(
    state: &mut TileLoop,
    dir: &Path,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
) {
    let candidates: Vec<(crate::native_guards::WindowKey, String)> = state
        .member_tokens
        .iter()
        .map(|(key, token)| (key.clone(), token.clone()))
        .collect();
    for (key, token) in candidates {
        if state.member_home(&key).is_none() {
            continue;
        }
        let Some(meta) = super::read_fullscreen_meta(key.hwnd) else {
            continue;
        };
        if meta.changed == 0 {
            continue;
        }
        let live_fullscreen =
            match observed
                .iter()
                .find(|window| window.token == token && window.identity.hwnd == key.hwnd)
            {
                Some(window) => Some(window.facts.captionless_fullscreen),
                None => match retained
                    .iter()
                    .find(|row| row.token == token && row.key == key)
                {
                    Some(row) => Some(row.fullscreen),
                    None => continue,
                },
            };
        // Only a positive non-fullscreen read retires: unknown or fullscreen
        // keeps the marker.
        if live_fullscreen != Some(false) {
            continue;
        }
        let Some(tag) = state
            .member_tags
            .get(&key)
            .cloned()
            .filter(|tag| !tag.is_empty())
        else {
            continue;
        };
        if super::fullscreen::retire_restored_fullscreen(dir, &key, &tag, &meta) {
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "fullscreen-retired",
                    "tick": state.tick,
                    "window": token,
                    "reason": "restored",
                }),
            );
        }
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
    // Retire owned fullscreen markers whose chrome the application already
    // restored before this tick's capture reads ownership metadata: metadata
    // only, exact-lifetime fenced, and never a geometry or style write.
    sweep_retire_restored_fullscreen(state, dir, &observed, &retained);
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
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
    // Cache committed floats' live frames; intent persistence is core-owned.
    let foreground = crate::ffi::foreground_window();
    if let Some(reply) = super::world_tick::run_observation_tick(
        state,
        me,
        store,
        dir,
        &observed,
        &retained,
        areas,
        foreground,
        fulls,
        super::observation::sample_cursor(),
    ) {
        let correlation_str = correlation.as_str();
        super::capacity_floats::adopt_floats_from_reply(
            state,
            &reply,
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
        // Focused fullscreen game: geometry for other windows converges,
        // but no cursor work runs here (no sampling, no overlay reads).
        // Core derives suspension from the foreground facts; the adapter
        // no longer blanks presentation.
        None,
    );
}
