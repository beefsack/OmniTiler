#![forbid(unsafe_code)]
use super::*;

/// Stable id for one ended gesture: the START-allocated id from the
/// gesture tables when they survived, else a fresh allocation. A
/// forgotten START still terminates: a fresh id orders above every
/// consumed id, so its terminal supersedes any leaked session for the
/// same window and consumes cleanly when none exists.
pub(super) fn gesture_end_id(state: &mut TileLoop, hwnd: u64) -> u64 {
    if let Some(id) = state.gesture_seq.get(&hwnd).copied() {
        return id;
    }
    state.next_gesture_seq += 1;
    let id = state.next_gesture_seq;
    state.gesture_seq.insert(hwnd, id);
    id
}

/// Stage a terminal Cancelled ingress for one ended gesture id so the next
/// eligible observation tick incorporates the terminal in core (clearing
/// any retained session and consuming the id). Returns the staged id.
/// The caller then converges (one bounded reconcile, which incorporates
/// through its observation tick) or defers the tick while suspended; the
/// central terminal drain clears the override on incorporation, and a
/// newer native START drops a stale one. Single slot: stashing overwrites
/// any older stashed terminal, which is safe because ids order globally
/// and every terminal consumes forward.
pub(super) fn stash_gesture_cancel(
    state: &mut TileLoop,
    hwnd: u64,
    id: u64,
    token: &str,
    kind: tiler_core::gesture::GestureKind,
    modal_move: bool,
    start_frame: Rect,
) -> u64 {
    let down = state.gesture_start_cursor.get(&hwnd).copied();
    state.gesture_override = Some(tiler_core::gesture::GestureIngress {
        id,
        window: tiler_core::directional::WindowId(token.to_owned()),
        kind,
        cursor: None,
        start_cursor: down,
        phase: tiler_core::gesture::GesturePhase::Cancelled,
        esc: false,
        modal_move,
        start_frame,
    });
    id
}

/// Resolve the mover token for one ended HWND: live membership first, then
/// the START-frozen preview binding. `None` when nothing names the window
/// anymore.
fn gesture_end_token(state: &TileLoop, hwnd: u64, current: Option<&str>) -> Option<String> {
    if let Some(token) = current {
        return Some(token.to_owned());
    }
    state
        .member_tokens
        .iter()
        .find(|(key, _)| key.hwnd == hwnd)
        .map(|(_, token)| token.clone())
        .or_else(|| {
            state
                .gesture_preview_start
                .get(&hwnd)
                .map(|start| start.token.clone())
        })
}

/// Native gesture settle (stage 4a): the title-bar modal loop is the only
/// producer. The core owns the release intent, the drop target, and the
/// commit from the normalized ingress; this tick reports the END facts
/// (exact lifetime, stable gesture id, START-sampled kind, END-captured
/// cursor, PRE-gesture frame, release phase) through the ordinary
/// observation tick and applies the accepted reply. Every path that ends
/// a gesture without a committing tick stashes a terminal Cancelled for
/// its id first, so the next eligible observation incorporates the
/// terminal in core instead of leaking the session. The only adapter
/// decisions are the native fences below (Esc, identity reuse, input
/// completeness, suspend/elevated, and the proven-hide-before-effects
/// ordering). No target, geometry, or content policy lives here: no
/// intent classification, no destination resolution, no arrival readback,
/// no follow focus. The applied reply is never equated with completion:
/// the drained terminal record decides the log.
pub(super) fn gesture_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    ended: &[u64],
) {
    let log_path = state.log_path.clone();
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        log_json_at(
            &log_path,
            serde_json::json!({"event":"tick-skip","tick":state.tick,"cause":"enum-failed"}),
        );
        return;
    };
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    // First-seen maximized windows restore once here: lifetime tags are
    // stamped, and the cleared window converges as eligible next tick.
    clear_maximize_at_admission(state, me, &retained);
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    for hwnd in ended {
        // Every gesture settles as native: the title-bar modal loop is
        // the only producer.
        let producer = "native";
        // Per-gesture START snapshot and END cursor travel with the gesture:
        // read them here; per-path finalization clears the tables after
        // stashing any terminal the core still needs.
        let start_key = state.gesture_start_key.remove(hwnd);
        let start_tag = state.gesture_start_tag.remove(hwnd);
        let end_cursor = state.gesture_end_cursor.remove(hwnd);
        // Modal move/size truth sampled once at START: read and clear here
        // so every END path (settle, cancel, refusal) leaves no stale arm.
        // It rides the ingress below; the core derives the intent.
        let modal_kind = state.move_kind.remove(hwnd);
        state.gesture_esc_seq.remove(hwnd);
        state.gesture_end_seq.remove(hwnd);
        // START-sampled kind for every ingress below (advisory at release:
        // the core derives the intent). Unknown START kinds default to
        // move, matching the preview route; the core still owns the
        // decision. The op name below is logging only.
        let kind = match modal_kind {
            Some(MoveSizeKind::Move) => tiler_core::gesture::GestureKind::Move,
            Some(MoveSizeKind::Resize) => tiler_core::gesture::GestureKind::Resize,
            _ => tiler_core::gesture::GestureKind::Move,
        };
        let modal_move = modal_kind.is_some_and(|kind| kind == MoveSizeKind::Move);
        let op = match kind {
            tiler_core::gesture::GestureKind::Move => "drag-drop",
            tiler_core::gesture::GestureKind::Resize => "pointer-resize",
        };
        // Explicit Esc cancellation first: a cancelled native move restores
        // its pre-gesture frame, so rect equality must never decide this
        // path. The stashed terminal clears any retained core session on
        // the converge tick below; topology unchanged by construction.
        if state.esc_latched.remove(hwnd) {
            let token = by_hwnd.get(hwnd).map(|w| w.token.clone()).or_else(|| {
                state
                    .member_tokens
                    .iter()
                    .find(|(key, _)| key.hwnd == *hwnd)
                    .map(|(_, token)| token.clone())
            });
            let mut line = serde_json::json!({
                "event": "gesture",
                "tick": state.tick,
                "op": "gesture",
                "disposition": "observed",
                "outcome": "gesture-cancelled-esc",
                "producer": producer,
            });
            if let Some(token) = token.clone() {
                line["window"] = serde_json::Value::from(token);
            }
            log_json_at(&log_path, line);
            // Esc cancel clears the preview with the gesture (already
            // hidden at latch time; this covers latch-and-settle races).
            // The terminal stashes first so the start tables are intact
            // for the ingress, then the per-gesture tables clear.
            if let Some(token) = token
                && let Some(before) = state
                    .gesture_before
                    .get(hwnd)
                    .copied()
                    .or_else(|| state.stable.get(hwnd).copied())
            {
                let id = gesture_end_id(state, *hwnd);
                stash_gesture_cancel(state, *hwnd, id, &token, kind, modal_move, before);
            }
            hide_preview(state, "esc-cancelled");
            state.preview_bound.remove(hwnd);
            state.gesture_preview_start.remove(hwnd);
            state.gesture_start_cursor.remove(hwnd);
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, store, dir, &fulls_owned, areas);
            continue;
        }
        let Some(current) = by_hwnd.get(hwnd) else {
            // A maximized/fullscreen member is retained, never eligible, so a
            // gesture ending on one misses the observation above. Refuse
            // explicitly with no Engine mutation; the next ordinary tick
            // converges. Identity-fenced on the stored member identity.
            if let Some(key) = state
                .member_tokens
                .iter()
                .find(|(k, _)| k.hwnd == *hwnd)
                .map(|(k, _)| k.clone())
                && let Some(stored) = state.member_identity.get(&key).cloned()
                && crate::native_guards::member_matches(
                    &key,
                    *hwnd,
                    stored.pid,
                    &stored.process_creation,
                )
                && let Some(row) = retained.iter().find(|r| r.key == key)
                && let Some(cause) = overlay_refusal_for(row)
                && let Some(token) = state.member_tokens.get(&key).cloned()
            {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "op": "gesture",
                        "disposition": "observed",
                        "outcome": if cause == "fullscreen" {
                            "gesture-refused-fullscreen"
                        } else {
                            "gesture-refused-maximize"
                        },
                        "window": token,
                    }),
                );
            }
            // The window is gone from observation: stash a terminal for its
            // id when known so a retained core session cannot leak, then
            // converge once to incorporate it.
            if let Some(token) = gesture_end_token(state, *hwnd, None)
                && let Some(before) = state
                    .gesture_before
                    .get(hwnd)
                    .copied()
                    .or_else(|| state.stable.get(hwnd).copied())
            {
                let id = gesture_end_id(state, *hwnd);
                stash_gesture_cancel(state, *hwnd, id, &token, kind, modal_move, before);
            }
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, store, dir, &fulls_owned, areas);
            continue;
        };
        let before = state
            .gesture_before
            .get(hwnd)
            .copied()
            .or_else(|| state.stable.get(hwnd).copied());
        let Some(before) = before else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "op": "gesture",
                    "disposition": "observed",
                    "outcome": "gesture-no-start",
                    "window": current.token,
                    "producer": producer,
                }),
            );
            // No PRE frame was ever captured: stash a terminal against the
            // live frame so a retained core session cannot leak, then
            // converge once to incorporate it.
            let id = gesture_end_id(state, *hwnd);
            stash_gesture_cancel(
                state,
                *hwnd,
                id,
                &current.token,
                kind,
                modal_move,
                current.visible,
            );
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, store, dir, &fulls_owned, areas);
            continue;
        };
        // START/END reuse fence: the fresh window must still match the
        // START-time snapshot when one was captured. A same-process HWND
        // reuse between START and END keeps HWND/PID/creation but starts
        // without (or with a stale) member lifetime tag, so it fails closed
        // here exactly like every other effect gate. No snapshot falls back
        // to the stored-member check below. The tag comparison is snapshot
        // versus a fresh member-property read (never the owned-helper tag,
        // which is empty for ordinary windows).
        if let (Some(key), Some(stored)) = (start_key.as_ref(), start_tag.as_ref())
            && (!crate::native_guards::member_matches(
                key,
                current.hwnd,
                current.identity.pid,
                &current.identity.process_creation,
            ) || {
                let live = live_member_tag(current.hwnd);
                stored != &live
            })
        {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "op": "gesture",
                    "disposition": "refused",
                    "outcome": "gesture-refused-identity",
                    "window": current.token,
                    "producer": producer,
                }),
            );
            // A reuse between START and END ends the START lifetime: stash
            // a terminal for its id so a retained core session cannot
            // leak, then converge once to incorporate it.
            let id = gesture_end_id(state, *hwnd);
            stash_gesture_cancel(state, *hwnd, id, &current.token, kind, modal_move, before);
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, store, dir, &fulls_owned, areas);
            continue;
        }
        // Release cursor carried from the MOVESIZEEND event callback, never
        // re-read later: an END without a captured cursor rides the
        // ingress as-is.
        let cursor = end_cursor;
        // The gesture window must still name a live member under the full
        // identity, or the END settles nothing on its own. A terminal is
        // still stashed so a retained core session cannot leak.
        let member_key = state
            .member_tokens
            .iter()
            .find(|(_, token)| token.as_str() == current.token.as_str())
            .map(|(key, _)| key.clone());
        let Some(member_key) = member_key else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "op": "gesture",
                    "disposition": "refused",
                    "outcome": "gesture-refused-identity",
                    "window": current.token,
                    "producer": producer,
                }),
            );
            let id = gesture_end_id(state, *hwnd);
            stash_gesture_cancel(state, *hwnd, id, &current.token, kind, modal_move, before);
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, store, dir, &fulls_owned, areas);
            continue;
        };
        if !crate::native_guards::member_matches(
            &member_key,
            current.hwnd,
            current.identity.pid,
            &current.identity.process_creation,
        ) {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "op": "gesture",
                    "disposition": "refused",
                    "outcome": "gesture-refused-identity",
                    "window": current.token,
                    "producer": producer,
                }),
            );
            let id = gesture_end_id(state, *hwnd);
            stash_gesture_cancel(state, *hwnd, id, &current.token, kind, modal_move, before);
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, store, dir, &fulls_owned, areas);
            continue;
        }
        // Native input-completeness short-circuit (no policy): an exact
        // frame match can never commit, and a cursor-less move is no-change
        // by construction. The stashed terminal clears any retained core
        // session on the converge tick below, so no-change ENDs never leak
        // it; a native nudge reasserts to the Engine allocation there.
        if current.visible == before
            || (cursor.is_none() && matches!(kind, tiler_core::gesture::GestureKind::Move))
        {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "op": "gesture",
                    "disposition": "observed",
                    "outcome": "gesture-no-change",
                    "window": current.token,
                    "producer": producer,
                }),
            );
            let id = gesture_end_id(state, *hwnd);
            stash_gesture_cancel(state, *hwnd, id, &current.token, kind, modal_move, before);
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, store, dir, &fulls_owned, areas);
            continue;
        }
        // Bridged finish: hide (the safety epoch bars any stale-show replay)
        // and stash the release as a bounded pending owner operation
        // serviced from render acks at loop level. No blocking wait: the
        // owner stays responsive to stop and new events, and the release
        // commits only after the proven `Hidden` acknowledgement,
        // re-entered with fresh observations. The same-thread path keeps
        // its proven-inline ordering. The core owns the release commit from
        // the stashed END facts; this only carries them.
        if state.render_bridge.is_some() {
            let id = gesture_end_id(state, *hwnd);
            let down = state.gesture_start_cursor.get(hwnd).copied();
            let occupied = queue_pending_drop(
                state,
                hwnd,
                id,
                producer,
                op,
                cursor,
                down,
                modal_move,
                before,
                kind,
                &current.token,
                start_key,
                start_tag,
                member_key,
            );
            state.preview_bound.remove(hwnd);
            state.gesture_preview_start.remove(hwnd);
            state.gesture_start_cursor.remove(hwnd);
            state.gesture_started_sent.remove(hwnd);
            state.gesture_end_cursor.remove(hwnd);
            state.gesture_before.remove(hwnd);
            if occupied {
                // A previous finish is still pending (overlapping END races):
                // the newcomer's own session, if preview-bound, must still
                // clear, so stash its terminal and converge once to
                // incorporate it. The pending operation is untouched.
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "op": op,
                        "disposition": "refused",
                        "outcome": "drag-drop-refused",
                        "kind": "preview-finish-busy",
                        "window": current.token,
                        "producer": producer,
                    }),
                );
                stash_gesture_cancel(state, *hwnd, id, &current.token, kind, modal_move, before);
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, store, dir, &fulls_owned, areas);
            }
            continue;
        }
        // Inline path (same-thread, no render bridge): fresh
        // suspend/elevated gates, proven inline hide, then the shared core
        // release through the ordinary observation tick. While suspended
        // the terminal only stashes: no tick runs, so nothing actuates
        // until the resume observation incorporates it.
        if state.suspended || suspend_read(state, me, fulls).veto.block || foreground_elevated(me) {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "op": op,
                    "disposition": "refused",
                    "outcome": if state.suspended {
                        "suspended"
                    } else {
                        "elevated-foreground"
                    },
                    "window": current.token,
                    "producer": producer,
                }),
            );
            let id = gesture_end_id(state, *hwnd);
            stash_gesture_cancel(state, *hwnd, id, &current.token, kind, modal_move, before);
            if !state.suspended {
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, store, dir, &fulls_owned, areas);
            }
            continue;
        }
        state.tick += 1;
        let correlation = state.correlation();
        // Normalized release (stage 4a): the core owns drop/resize planning
        // from the normalized ingress, and the host diff applies the
        // accepted reply with fresh proof. The drained terminal record IS
        // the outcome: a cached or stale reply is never equated with
        // completion. Reply rendering must never outlive the gesture: the
        // preview is proven hidden before the world release runs, so no
        // late sample can paint over the settled layout. An unacknowledged
        // hide stashes a terminal and converges through one bounded
        // reconcile (incorporating the terminal) instead of dropping it.
        let drop_prior = state
            .preview_bound
            .get(hwnd)
            .and_then(|bound| bound.hover_prior.clone());
        let drop_prior_desc = preview_prior_desc(&drop_prior);
        if !hide_preview_finish(state, "finish") {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "correlation": correlation.as_str(),
                    "op": op,
                    "disposition": "refused",
                    "outcome": "drag-drop-refused",
                    "kind": "preview-finish-unacked",
                    "window": current.token,
                    "producer": producer,
                    "hover_prior": drop_prior_desc,
                }),
            );
            state.preview_bound.remove(hwnd);
            state.gesture_preview_start.remove(hwnd);
            state.gesture_start_cursor.remove(hwnd);
            let id = gesture_end_id(state, *hwnd);
            stash_gesture_cancel(state, *hwnd, id, &current.token, kind, modal_move, before);
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, store, dir, &fulls_owned, areas);
            continue;
        }
        state.preview_bound.remove(hwnd);
        let down = state.gesture_start_cursor.get(hwnd).copied();
        state.gesture_preview_start.remove(hwnd);
        state.gesture_start_cursor.remove(hwnd);
        let foreground = crate::ffi::foreground_window();
        let release_id = gesture_end_id(state, *hwnd);
        state.gesture_override = Some(tiler_core::gesture::GestureIngress {
            id: release_id,
            window: tiler_core::directional::WindowId(current.token.clone()),
            kind,
            cursor,
            start_cursor: down,
            phase: tiler_core::gesture::GesturePhase::Released,
            esc: false,
            modal_move,
            start_frame: before,
        });
        let reply = super::world_tick::run_observation_tick(
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
        );
        // The tick clears the stashed override only on genuine
        // incorporation (freshly accepted reply carrying this id); a tick
        // that never ran the gesture leaves it stashed for the next
        // eligible observation, so the terminal is never lost.
        let settled = reply.as_ref().and_then(|held| held.gesture_settled);
        match (&reply, settled) {
            (Some(_), Some(terminal)) if terminal.id == release_id && terminal.committed => {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "correlation": correlation.as_str(),
                        "op": op,
                        "disposition": "applied",
                        "outcome": if op == "drag-drop" { "drag-drop-committed" } else { "pointer-resize-committed" },
                        "window": current.token,
                        "producer": producer,
                        "hover_prior": drop_prior_desc,
                        // Core commit only: the reply was accepted and the
                        // Engine committed, but this does not prove native
                        // geometry read back. Arrival proof stays a separate
                        // live concern, never inferred here.
                        "committed": true,
                    }),
                );
            }
            (Some(_), Some(terminal)) if terminal.id == release_id => {
                // The tick applied, but the core settled without committing
                // (no-change snap-back, fence hold, or refusal): honest
                // refused row, then converge once like the unverified leg.
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "correlation": correlation.as_str(),
                        "op": op,
                        "disposition": "refused",
                        "outcome": "drag-drop-refused",
                        "kind": "settled-without-commit",
                        "window": current.token,
                        "producer": producer,
                        "hover_prior": drop_prior_desc,
                        "committed": false,
                    }),
                );
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, store, dir, &fulls_owned, areas);
            }
            _ => {
                // No incorporated terminal for this id: the tick never ran
                // the gesture (dispatch failure, stale/duplicate delivery,
                // or rejection). Never claim completion; the override stays
                // stashed for the next eligible observation.
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "correlation": correlation.as_str(),
                        "op": op,
                        "disposition": "refused",
                        "outcome": "drag-drop-refused",
                        "kind": "unverified",
                        "window": current.token,
                        "producer": producer,
                        "hover_prior": drop_prior_desc,
                        "committed": false,
                    }),
                );
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, store, dir, &fulls_owned, areas);
            }
        }
    }
    // Terminal finalization owns per-gesture tables: retain entries for
    // still-open gestures (a second modal loop may hold while this END
    // settles without touching its tables), drop everything for ended
    // HWNDs. Ended HWNDs are exactly those no longer in `active` (the
    // MoveSizeEnd drain removes them there before this tick runs).
    state
        .gesture_before
        .retain(|hwnd, _| state.active.contains(hwnd));
    state
        .move_kind
        .retain(|hwnd, _| state.active.contains(hwnd));
    state.esc_latched.retain(|hwnd| state.active.contains(hwnd));
    state
        .gesture_esc_seq
        .retain(|hwnd, _| state.active.contains(hwnd));
    state
        .gesture_end_seq
        .retain(|hwnd, _| state.active.contains(hwnd));
    state
        .gesture_end_cursor
        .retain(|hwnd, _| state.active.contains(hwnd));
    // A pending bridged finish owns its START snapshots until the loop-level
    // service consumes them: retain exactly that HWND's entries here,
    // alongside any still-open gesture.
    let pending_hwnd = state
        .render_bridge
        .as_ref()
        .and_then(|bridge| bridge.pending.as_ref())
        .map(|pending| pending.hwnd);
    match pending_hwnd {
        Some(hwnd) => {
            state
                .gesture_start_key
                .retain(|key, _| *key == hwnd || state.active.contains(key));
            state
                .gesture_start_tag
                .retain(|key, _| *key == hwnd || state.active.contains(key));
        }
        None => {
            state
                .gesture_start_key
                .retain(|key, _| state.active.contains(key));
            state
                .gesture_start_tag
                .retain(|key, _| state.active.contains(key));
        }
    }
    state
        .preview_bound
        .retain(|hwnd, _| state.active.contains(hwnd));
    state
        .gesture_preview_start
        .retain(|hwnd, _| state.active.contains(hwnd));
    state
        .gesture_start_cursor
        .retain(|hwnd, _| state.active.contains(hwnd));
    state
        .gesture_started_sent
        .retain(|hwnd| state.active.contains(hwnd));
    state
        .gesture_seq
        .retain(|hwnd, _| state.active.contains(hwnd));
    // No gesture remains open: no preview rectangle survives the settle.
    // A still-open gesture keeps rendering on its own cadence.
    if state.active.is_empty() {
        hide_preview(state, "finish");
    }
}
