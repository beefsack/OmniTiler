#![forbid(unsafe_code)]
use super::*;

/// END-bound Esc latch decision: a physical Esc down cancels the gesture
/// only when the observed sequence differs from the START snapshot. Held
/// gestures observe the live counter; just-ended gestures observe the
/// END-callback snapshot, so an Esc that lands after the END (before the
/// owner drain) observes an equal snapshot and never cancels the completed
/// drop. Pure sequence comparison (wrapping-aware by construction: the
/// counter only moves forward one step per physical Esc down).
#[must_use]
pub(super) fn esc_edge_cancels(start_seq: u64, observed_seq: u64) -> bool {
    observed_seq != start_seq
}

/// Shared START capture for both gesture producers (native WinEvent and
/// project Win+Left): the Esc-sequence snapshot, the PRE-gesture rectangle,
/// and the START-time identity snapshot. PRE comes from the last stable
/// observation, never from a mid-gesture frame; live `GetWindowRect` is the
/// fallback only when `stable` has no entry. Probe failure leaves no
/// snapshot and settle falls back to the stored-member check only.
pub(super) fn capture_gesture_start(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    hwnd: u64,
    start_seq: u64,
) {
    // Sequence-bound Esc coordination: the snapshot rode with the START
    // event from callback time instead of being re-read here. Only edges
    // with a newer sequence latch later, so a stale pre-gesture tap never
    // cancels and a same-batch edge is ordered rather than discarded.
    if let std::collections::hash_map::Entry::Vacant(e) = state.gesture_esc_seq.entry(hwnd) {
        e.insert(start_seq);
    }
    if !state.gesture_before.contains_key(&hwnd) {
        let pre = state
            .stable
            .get(&hwnd)
            .copied()
            .or_else(|| live_outer_rect(hwnd).or_else(|| state.stable.get(&hwnd).copied()));
        if let Some(pre) = pre {
            state.gesture_before.insert(hwnd, pre);
        }
    }
    // START-time identity snapshot for the settle-time reuse fence: a
    // same-process HWND reuse between START and END fails closed even when
    // HWND/PID/creation still agree.
    if let std::collections::hash_map::Entry::Vacant(e) = state.gesture_start_key.entry(hwnd) {
        let probe = hwnd as isize as HWND;
        if let Some(live) = window_identity(probe, hwnd, me) {
            e.insert(crate::workspace::WindowKey {
                hwnd,
                pid: live.pid,
                creation: live.process_creation.clone(),
            });
            // Member lifetime tag, not the owned-helper tag (empty for
            // ordinary windows): the settle fence compares this against a
            // fresh member read, exactly like the admission gate.
            state
                .gesture_start_tag
                .insert(hwnd, crate::product_hide::sys::read_member_tag(hwnd));
        }
    }
    // START-frozen preview source binding (first START wins, never
    // rebound): the exact member token, source output/workspace, and
    // accepted revision the preview samples and the final drop resolve
    // against. Unmanaged STARTs store nothing and fail closed in the
    // refresh; drift after START fails the whole gesture closed.
    if !state.gesture_preview_start.contains_key(&hwnd) {
        let start_binding = state
            .member_tokens
            .iter()
            .find(|(key, _)| key.hwnd == hwnd)
            .and_then(|(key, token)| {
                state.workspaces.member_loc(key).cloned().map(|loc| {
                    let revision = revision_for(state, &loc.output, &loc.workspace);
                    PreviewStartBinding {
                        token: token.clone(),
                        output: loc.output,
                        workspace: loc.workspace,
                        revision,
                    }
                })
            });
        match start_binding {
            Some(binding) => {
                state.gesture_preview_start.insert(hwnd, binding);
            }
            None => {
                state.gesture_preview_start.remove(&hwnd);
            }
        }
    }
}

/// One drained project Win+Left down edge: validate the callback-bound
/// candidate against fresh evidence BEFORE any effect or focus, then arm
/// the shared gesture maps so the hold pauses tiling and the release
/// settles through the shared route. After all START fences pass and the
/// arm captures `active`, the mover is focused at press through the
/// existing [`actuate_focus`] authority (R-DRAG-08): a suspend/elevation
/// precheck refuses first (no prime against a known vetoing arrival), then
/// a fresh exact foreground readback avoids a redundant setter when the
/// mover already holds foreground, otherwise one `actuate_focus` attempt
/// decides (its late fence stays the second race guard). A refused press
/// clears only this HWND's arm with no Engine plan and no geometry change;
/// a failed actuation may still have attempted the E8 prime/setter, so only
/// geometry/plan silence is claimed, never zero focus writes. No geometry
/// and no Engine call here. Returns the bound token when the gesture armed.
///
/// The callback snapshot (published origin plus member tag cloned at Down
/// time) must still match the current published entry AND a fresh live
/// probe on every field, closing the callback-to-drain HWND-reuse gap: a
/// different-process reuse fails PID/creation, a same-process reuse fails
/// the lifetime tag, and a retokened member fails the token. The validated
/// snapshot is stored as the bound generation; the Up requires it and never
/// rebinds.
///
/// A refused Down carries no identity: only the closed refusal reason
/// enters the log, never HWNDs or tokens.
pub(super) fn windrag_down(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    edge: &WinDragEdge,
    fulls: &[Rect],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
) -> std::result::Result<String, &'static str> {
    let hwnd = edge.hwnd;
    let Some(snapshot) = edge.snapshot.as_ref() else {
        return Err("no-snapshot");
    };
    // Tiled-only published origins plus current management: floating,
    // sticky, unmanaged, and unknown subjects never arm (their Win+Left
    // passed through natively hook-side).
    if !state.managed.contains(&hwnd) {
        return Err("unmanaged");
    }
    let Some(published) = state.windrag_origins.get(&hwnd) else {
        return Err("unknown-subject");
    };
    if state.active.contains(&hwnd)
        || state.gesture_before.contains_key(&hwnd)
        || state.windrag_bound.contains_key(&hwnd)
    {
        return Err("busy");
    }
    // Fresh live evidence on the owner thread: the snapshot must still
    // match the live window, not just the last publish.
    let probe = hwnd as isize as HWND;
    let Some(live) = window_identity(probe, hwnd, me) else {
        return Err("identity-changed");
    };
    let live_tag = crate::product_hide::sys::read_member_tag(hwnd);
    if !validate_windrag_down(
        snapshot,
        &published.origin,
        &published.tag,
        live.pid,
        &live.process_creation,
        &live_tag,
    ) {
        return Err("stale-snapshot");
    }
    // The snapshot token must still name this exact live member.
    let live_key = crate::workspace::WindowKey {
        hwnd,
        pid: live.pid,
        creation: live.process_creation.clone(),
    };
    if state
        .member_tokens
        .get(&live_key)
        .is_none_or(|token| token != &snapshot.origin.token)
    {
        return Err("no-token");
    }
    capture_gesture_start(state, me, hwnd, edge.esc_seq);
    if !state.gesture_before.contains_key(&hwnd) {
        // No PRE frame (not stable and unreadable live): fail closed with
        // no arm rather than settling without a source allocation.
        state.gesture_esc_seq.remove(&hwnd);
        state.gesture_start_key.remove(&hwnd);
        state.gesture_start_tag.remove(&hwnd);
        state.gesture_preview_start.remove(&hwnd);
        return Err("no-pre");
    }
    state.gesture_producer.insert(hwnd, "windrag");
    state.windrag_start_cursor.insert(hwnd, (edge.x, edge.y));
    state.windrag_bound.insert(hwnd, snapshot.clone());
    // A/B move arm: the project hold classifies as a move, so a focused
    // Win+Left hold reveals the projected group underlay while held (no
    // stage C, no unfocused Engine focus substitution). Cleaned on
    // Up/cancel/suspend/settle like every other gesture map.
    state.move_kind.insert(hwnd, MoveSizeKind::Move);
    state.active.insert(hwnd);
    // R-DRAG-08 press focus: the validated mover activates at press through
    // the same authority as the drop path, after every START fence and the
    // `active` capture above. A suspend/elevation precheck refuses first so
    // a known vetoing fullscreen arrival (or an elevated foreground) is
    // never primed against; the late fence inside `actuate_focus` stays the
    // second guard for arrivals racing this drain. Failure refuses the whole
    // arm via the scoped clear (this HWND's maps only: a concurrently armed
    // neighbour's preview is never disturbed) with no Engine plan and no
    // geometry change. A failed actuation may still have attempted the E8
    // prime/setter, so only geometry/plan silence is claimed.
    if suspend_read(state, me, fulls).veto.block || foreground_elevated(me) {
        clear_windrag_gesture_scoped(state, hwnd, false);
        return Err("focus-skipped-fence");
    }
    let token = snapshot.origin.token.clone();
    if crate::ffi::foreground_window() == hwnd {
        // Already exact foreground: no prime, no setter, no wait.
        let log_path = state.log_path.clone();
        let tick = state.tick;
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "windrag-focus",
                "tick": tick,
                "phase": "press",
                "outcome": "focus-ok",
                "already_foreground": true,
                "prime_inserted": 0,
                "setter_accepted": false,
                "attach_ok": false,
                "eventual": false,
                "duration_ms": 0,
                "window": token.as_str(),
                "producer": "windrag",
            }),
        );
        return Ok(token);
    }
    let focus_start = Instant::now();
    let actuation = actuate_focus(state, me, fulls, observed, retained, &token);
    let duration_ms = focus_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    if actuation.outcome != "focus-ok" {
        clear_windrag_gesture_scoped(state, hwnd, false);
        return Err(actuation.outcome);
    }
    let log_path = state.log_path.clone();
    let tick = state.tick;
    log_json_at(
        &log_path,
        serde_json::json!({
            "event": "windrag-focus",
            "tick": tick,
            "phase": "press",
            "outcome": actuation.outcome,
            "already_foreground": false,
            "prime_inserted": actuation.prime_inserted,
            "setter_accepted": actuation.setter_accepted,
            "attach_ok": actuation.attach_ok,
            "eventual": actuation.eventual,
            "duration_ms": duration_ms,
            "window": token.as_str(),
            "producer": "windrag",
        }),
    );
    Ok(token)
}

/// One drained project Win+Left up edge: carry the callback-bound release
/// pointer to settle and close the hold. Requires the bound generation
/// stored at arm time, so a stray or replayed Up can never rebind or
/// settle a gesture it did not close. The shared `ended` computation below
/// settles it through `gesture_tick` on this same pump.
pub(super) fn windrag_up(state: &mut TileLoop, hwnd: u64, x: i32, y: i32, esc_seq: u64) {
    if !state.windrag_bound.contains_key(&hwnd) {
        return;
    }
    if state.gesture_producer.get(&hwnd).copied() != Some("windrag") {
        return;
    }
    if !state.active.contains(&hwnd) {
        return;
    }
    state.gesture_end_cursor.insert(hwnd, (x, y));
    if let std::collections::hash_map::Entry::Vacant(e) = state.gesture_end_seq.entry(hwnd) {
        e.insert(esc_seq);
    }
    state.active.remove(&hwnd);
    state.move_kind.remove(&hwnd);
}

/// Clear every project-gesture map for one HWND (owner-side Cancel): the
/// hook already disarmed and queued this edge, so the paired Up stays
/// swallowed hook-side while the owner drops the gesture with no plan and
/// no geometry change. Native gestures on the same HWND are untouched:
/// only entries produced by a validated windrag arm are removed.
/// `hide_visible_fallback` keeps the legacy Cancel behavior (a visible
/// singleton overlay hides with the gesture even without an owned binding);
/// the press-failure path passes `false` so a concurrently armed
/// neighbour's preview is never disturbed (at press this HWND owns no
/// preview binding yet).
pub(super) fn clear_windrag_gesture(state: &mut TileLoop, hwnd: u64) -> Option<String> {
    clear_windrag_gesture_scoped(state, hwnd, true)
}

/// Scoped variant of [`clear_windrag_gesture`]; see `hide_visible_fallback`.
pub(super) fn clear_windrag_gesture_scoped(
    state: &mut TileLoop,
    hwnd: u64,
    hide_visible_fallback: bool,
) -> Option<String> {
    if state.gesture_producer.get(&hwnd).copied() != Some("windrag")
        && !state.windrag_bound.contains_key(&hwnd)
    {
        return None;
    }
    let token = state
        .windrag_bound
        .get(&hwnd)
        .map(|bound| bound.origin.token.clone());
    let had_preview = state.preview_bound.remove(&hwnd).is_some();
    state.preview_dead.remove(&hwnd);
    state.gesture_preview_start.remove(&hwnd);
    state.gesture_start_cursor.remove(&hwnd);
    // Owner-side cancel clears the preview with the gesture (the hook
    // already disarmed; the paired Up stays swallowed hook-side).
    if had_preview
        || (hide_visible_fallback && render_carrier_visible(state, RenderCarrier::Preview))
    {
        hide_preview(state, "cancelled");
    }
    state.active.remove(&hwnd);
    state.gesture_before.remove(&hwnd);
    state.gesture_end_cursor.remove(&hwnd);
    state.gesture_start_key.remove(&hwnd);
    state.gesture_start_tag.remove(&hwnd);
    state.gesture_esc_seq.remove(&hwnd);
    state.gesture_end_seq.remove(&hwnd);
    state.esc_latched.remove(&hwnd);
    state.gesture_producer.remove(&hwnd);
    state.windrag_start_cursor.remove(&hwnd);
    state.windrag_bound.remove(&hwnd);
    state.move_kind.remove(&hwnd);
    // Owner-side cancel also drops a queued bridged finish for the same
    // HWND: the gesture is gone, so a late hide ack must never actuate it.
    // No Engine/focus/geometry here; the service logs the refusal when it
    // observes the missing slot, or this drops it immediately when the
    // bridge is present.
    if state
        .render_bridge
        .as_ref()
        .and_then(|bridge| bridge.pending.as_ref())
        .is_some_and(|pending| pending.hwnd == hwnd)
    {
        if let Some(bridge) = state.render_bridge.as_mut() {
            bridge.pending.take();
        }
        state.gesture_start_key.remove(&hwnd);
        state.gesture_start_tag.remove(&hwnd);
    }
    token
}
