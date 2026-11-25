#![forbid(unsafe_code)]
use super::gesture_tick::stash_gesture_cancel;
use super::*;

/// Bounded pending gesture release for the bridged finish path. Owns the
/// END-captured facts the commit needs (stable gesture id, release cursor,
/// modal truth, PRE-gesture frame, START snapshots, membership routing,
/// hide pair, deadline) so the service revalidates against fresh
/// observations. The core owns the release commit from these facts; this
/// only carries them. No callbacks, no Engine, no framework: one struct,
/// one service function on the owner loop.
pub(super) struct PendingDrop {
    pub(super) hwnd: u64,
    /// Stable gesture id allocated at native START, carried so the core
    /// orders the release by id and the terminal consumes exactly once.
    pub(super) id: u64,
    pub(super) producer: String,
    /// Operation name for logs (`drag-drop` or `pointer-resize`), derived
    /// once at END from the START-sampled kind. Logging only: the core
    /// owns the release intent.
    pub(super) op: &'static str,
    /// END-captured release cursor (never re-read): `None` routes as
    /// no-change for moves.
    pub(super) release_cursor: Option<(i32, i32)>,
    /// START down point when readable (drives the core zero gate on an
    /// implicit bind, exactly like the open-gesture path).
    pub(super) start_cursor: Option<(i32, i32)>,
    /// START-sampled modal truth for the release derivation.
    pub(super) modal_move: bool,
    /// PRE-gesture frame frozen at START: the core release ground truth.
    pub(super) start_frame: Rect,
    /// START-sampled gesture kind (advisory; the release intent derives
    /// from frames).
    pub(super) kind: tiler_core::gesture::GestureKind,
    pub(super) mover_token: String,
    pub(super) start_key: Option<crate::native_guards::WindowKey>,
    pub(super) start_tag: Option<Option<String>>,
    pub(super) member_key: crate::native_guards::WindowKey,
    pub(super) hide_gen: u64,
    pub(super) hide_safety: u64,
    pub(super) deadline: Instant,
}

/// Pending invalidation while a bridged finish waits (or after its hide
/// proves): stop, Esc/cancel, suspend/elevated, or a new gesture for the
/// same HWND. Returns the narrow refusal vocabulary when invalidated, else
/// `None`. Pure decision over owner state plus the suspend/elevated reads
/// (injected in tests, native in production); no Engine, focus, geometry,
/// or pending mutation here. The caller drops the slot.
fn pending_invalidation(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    hwnd: u64,
) -> Option<(&'static str, &'static str)> {
    #[cfg(test)]
    if INJECTED_STOP.with(|slot| slot.get()) {
        return Some(("drag-drop-refused", "preview-finish-stopped"));
    }
    if state.esc_latched.contains(&hwnd) {
        return Some(("gesture-cancelled-esc", "esc"));
    }
    if state.suspended {
        return Some(("drag-drop-refused", "preview-finish-suspended"));
    }
    if suspend_read(state, me, fulls).veto.block || foreground_elevated(me) {
        return Some(("drag-drop-refused", "preview-finish-suspended"));
    }
    // A new gesture for the same HWND supersedes the queued finish: the
    // pending slot owns its START snapshots, but a fresh `active` capture
    // means the window started moving again after END. Fail the stale slot
    // closed; the new gesture settles on its own END.
    if state.active.contains(&hwnd) {
        return Some(("drag-drop-refused", "preview-finish-superseded"));
    }
    None
}

/// Queue one bridged finish as a bounded pending owner operation (END path).
/// Hides the preview (the safety epoch bars any stale-show replay) and
/// stashes the release facts with their hide pair and deadline. Returns
/// `true` when a previous finish still owns the slot (overlapping END
/// race): the newcomer is refused narrowly with no drop and no reconcile,
/// and the pending operation owns the settle. Single slot: one gesture
/// settles at a time. Production END queueing funnels through here; tests
/// exercise it directly with offline fixtures (channels only, no renderer
/// thread).
#[allow(clippy::too_many_arguments)]
pub(super) fn queue_pending_drop(
    state: &mut TileLoop,
    hwnd: &u64,
    id: u64,
    producer: &'static str,
    op: &'static str,
    release_cursor: Option<(i32, i32)>,
    start_cursor: Option<(i32, i32)>,
    modal_move: bool,
    start_frame: Rect,
    kind: tiler_core::gesture::GestureKind,
    mover_token: &str,
    start_key: Option<crate::native_guards::WindowKey>,
    start_tag: Option<Option<String>>,
    member_key: crate::native_guards::WindowKey,
) -> bool {
    hide_render_carrier(state, RenderCarrier::Preview, "finish");
    state
        .render_bridge
        .as_mut()
        .map(|bridge| {
            let index = RenderCarrier::Preview.index();
            let pair = (
                bridge.gens[index],
                bridge.channels.hide_mark(RenderCarrier::Preview),
            );
            if bridge.pending.is_some() {
                true
            } else {
                bridge.pending = Some(PendingDrop {
                    hwnd: *hwnd,
                    id,
                    producer: producer.to_owned(),
                    op,
                    release_cursor,
                    start_cursor,
                    modal_move,
                    start_frame,
                    kind,
                    mover_token: mover_token.to_owned(),
                    start_key,
                    start_tag,
                    member_key,
                    hide_gen: pair.0,
                    hide_safety: pair.1,
                    deadline: Instant::now() + FINISH_WAIT,
                });
                false
            }
        })
        .unwrap_or(false)
}

/// Service the bounded pending gesture drop from render-ack state at loop
/// level (ack-only and full turns alike): no blocking wait, so the owner
/// stays responsive to stop and new events. On a proven `Hidden`
/// acknowledgement the settle re-enters with fresh observations: the
/// core commits the release from the stashed END facts through the
/// ordinary observation tick, and the host diff applies the accepted
/// reply. The drained terminal record IS the outcome: a cached or stale
/// reply is never equated with completion. On timeout the drop is
/// refused narrowly with no drop and no reconcile, but its terminal is
/// still stashed so the core cannot leak the session. Cancel/Esc/
/// suspend/new-gesture/stop invalidate the pending slot the same way
/// (terminal first), with no actuation of their own, so a late ack can
/// never actuate after the gesture was cancelled, suspended, superseded,
/// or stopped.
pub(super) fn service_pending_drop(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    let Some(hide_pair) = state.render_bridge.as_ref().and_then(|bridge| {
        bridge
            .pending
            .as_ref()
            .map(|pending| (pending.hide_gen, pending.hide_safety, pending.deadline))
    }) else {
        return;
    };
    let (hide_gen, hide_safety, deadline) = hide_pair;
    let hwnd = state
        .render_bridge
        .as_ref()
        .and_then(|bridge| bridge.pending.as_ref())
        .map(|pending| pending.hwnd)
        .expect("pending peeked above");
    // Invalidation while waiting (unproven or proven): stop, Esc/cancel,
    // suspend, or a new gesture for the same HWND. The terminal stashes
    // first so the core session cannot leak; only the Esc path converges
    // (one bounded reconcile to snap back), since stop/suspend/supersede
    // own no settle. Checked before the timeout so a cancelled pending
    // never waits out the bound.
    if let Some((outcome, kind)) = pending_invalidation(state, me, fulls, hwnd) {
        let stashed = state
            .render_bridge
            .as_ref()
            .and_then(|bridge| bridge.pending.as_ref())
            .map(|pending| {
                (
                    pending.id,
                    pending.mover_token.clone(),
                    pending.kind,
                    pending.modal_move,
                    pending.start_frame,
                )
            });
        if let Some((id, mover_token, pending_kind, modal_move, start_frame)) = stashed {
            stash_gesture_cancel(
                state,
                hwnd,
                id,
                &mover_token,
                pending_kind,
                modal_move,
                start_frame,
            );
        }
        let mover_token = state
            .render_bridge
            .as_ref()
            .and_then(|bridge| bridge.pending.as_ref())
            .map(|pending| pending.mover_token.clone())
            .unwrap_or_default();
        if let Some(bridge) = state.render_bridge.as_mut() {
            bridge.pending.take();
        }
        state.gesture_start_key.remove(&hwnd);
        state.gesture_start_tag.remove(&hwnd);
        let reconcile = outcome == "gesture-cancelled-esc";
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "gesture",
                "tick": state.tick,
                "op": "drag-drop",
                "disposition": "refused",
                "outcome": outcome,
                "kind": kind,
                "window": mover_token,
                "producer": "settle",
            }),
        );
        if reconcile {
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, store, dir, &fulls_owned, areas);
        }
        return;
    }
    let proven = state
        .render_bridge
        .as_ref()
        .and_then(|bridge| bridge.hidden_ack[RenderCarrier::Preview.index()])
        .is_some_and(|(hgen, safe)| hgen >= hide_gen && safe >= hide_safety);
    if !proven {
        if Instant::now() >= deadline {
            // Timeout: refuse narrowly with no drop and no reconcile, but
            // stash the terminal first so the core session cannot leak:
            // the next eligible observation incorporates it, while the
            // posted safety epoch already bars any stale-show replay. The
            // renderer hides on its next quantum either way.
            let stash = state
                .render_bridge
                .as_ref()
                .and_then(|bridge| bridge.pending.as_ref())
                .map(|pending| {
                    (
                        pending.hwnd,
                        pending.id,
                        pending.mover_token.clone(),
                        pending.kind,
                        pending.modal_move,
                        pending.start_frame,
                    )
                });
            if let Some((phwnd, pid, ptoken, pkind, pmove, pframe)) = stash {
                stash_gesture_cancel(state, phwnd, pid, &ptoken, pkind, pmove, pframe);
                log_json_at(
                    &state.log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "op": "drag-drop",
                        "disposition": "refused",
                        "outcome": "drag-drop-refused",
                        "kind": "preview-finish-unacked",
                        "window": ptoken,
                        "producer": "settle",
                    }),
                );
            }
            if let Some(bridge) = state.render_bridge.as_mut() {
                bridge.pending.take();
            }
            state.gesture_start_key.remove(&hwnd);
            state.gesture_start_tag.remove(&hwnd);
        }
        return;
    }
    // Fresh observation first, slot held: a transient enumeration failure
    // loses nothing (the slot and its deadline survive for the next
    // service), and only a proven hide plus a fresh observation proceeds.
    let log_path = state.log_path.clone();
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        log_json_at(
            &log_path,
            serde_json::json!({"event":"tick-skip","tick":state.tick,"cause":"enum-failed"}),
        );
        return;
    };
    let Some(pending) = state
        .render_bridge
        .as_mut()
        .and_then(|bridge| bridge.pending.take())
    else {
        return;
    };
    state.gesture_start_key.remove(&pending.hwnd);
    state.gesture_start_tag.remove(&pending.hwnd);
    publish_managed(state, me, &observed, &retained);
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    // Fresh revalidation against the stashed START snapshots before any
    // write: a reuse between END and the proven hide stashes a terminal
    // and converges once instead of dropping under a recycled identity.
    let Some(current) = by_hwnd.get(&pending.hwnd) else {
        stash_gesture_cancel(
            state,
            pending.hwnd,
            pending.id,
            &pending.mover_token,
            pending.kind,
            pending.modal_move,
            pending.start_frame,
        );
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, store, dir, &fulls_owned, areas);
        return;
    };
    if let (Some(key), Some(stored)) = (pending.start_key.as_ref(), pending.start_tag.as_ref())
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
                "producer": pending.producer,
            }),
        );
        stash_gesture_cancel(
            state,
            pending.hwnd,
            pending.id,
            &pending.mover_token,
            pending.kind,
            pending.modal_move,
            pending.start_frame,
        );
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, store, dir, &fulls_owned, areas);
        return;
    }
    let member_key = state
        .member_tokens
        .iter()
        .find(|(_, token)| token.as_str() == current.token.as_str())
        .map(|(key, _)| key.clone());
    let Some(member_key) = member_key else {
        stash_gesture_cancel(
            state,
            pending.hwnd,
            pending.id,
            &pending.mover_token,
            pending.kind,
            pending.modal_move,
            pending.start_frame,
        );
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, store, dir, &fulls_owned, areas);
        return;
    };
    // The stashed END-time membership must still name the same member:
    // a token remap between END and the proven hide stashes a terminal
    // instead of dropping under a recycled identity.
    if member_key != pending.member_key {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "gesture",
                "tick": state.tick,
                "op": "gesture",
                "disposition": "refused",
                "outcome": "gesture-refused-identity",
                "window": current.token,
                "producer": pending.producer,
            }),
        );
        stash_gesture_cancel(
            state,
            pending.hwnd,
            pending.id,
            &pending.mover_token,
            pending.kind,
            pending.modal_move,
            pending.start_frame,
        );
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, store, dir, &fulls_owned, areas);
        return;
    }
    if !crate::native_guards::member_matches(
        &member_key,
        current.hwnd,
        current.identity.pid,
        &current.identity.process_creation,
    ) {
        stash_gesture_cancel(
            state,
            pending.hwnd,
            pending.id,
            &pending.mover_token,
            pending.kind,
            pending.modal_move,
            pending.start_frame,
        );
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, store, dir, &fulls_owned, areas);
        return;
    }
    // Stored member-lifetime tag fence (same-process HWND reuse): the live
    // tag must still match the stamped one, even when no START snapshot
    // was captured. A reuse between END and the proven hide stashes a
    // terminal exactly like the START-identity leg above.
    let stored_tag = state
        .member_tags
        .get(&member_key)
        .cloned()
        .unwrap_or_default();
    if !stored_tag.is_empty() {
        let live = live_member_tag(current.hwnd);
        if !crate::native_guards::visible_lifetime_ok(&stored_tag, live.as_deref()) {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "op": "gesture",
                    "disposition": "refused",
                    "outcome": "gesture-refused-identity",
                    "window": current.token,
                    "producer": pending.producer,
                }),
            );
            stash_gesture_cancel(
                state,
                pending.hwnd,
                pending.id,
                &pending.mover_token,
                pending.kind,
                pending.modal_move,
                pending.start_frame,
            );
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, store, dir, &fulls_owned, areas);
            return;
        }
    }
    state.tick += 1;
    let correlation = state.correlation();
    // Normalized release (stage 4a): the core owns the commit from the
    // stashed END facts. The reply-carried terminal IS the outcome: only
    // a freshly accepted reply paired with a matching committed terminal
    // logs completion, never a stale cached reply. No arrival proof, no
    // follow focus here: the host diff inside the observation tick applies
    // the accepted reply with fresh proof, and the core's retained
    // transfers plus focus effects own completion.
    let op = pending.op;
    let release_id = pending.id;
    state.gesture_override = Some(tiler_core::gesture::GestureIngress {
        id: pending.id,
        window: tiler_core::directional::WindowId(current.token.clone()),
        kind: pending.kind,
        cursor: pending.release_cursor,
        start_cursor: pending.start_cursor,
        phase: tiler_core::gesture::GesturePhase::Released,
        esc: false,
        modal_move: pending.modal_move,
        start_frame: pending.start_frame,
    });
    let mover_token = pending.mover_token.clone();
    let producer = pending.producer.clone();
    drop(pending);
    let foreground = live_foreground_window();
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
    // The tick clears the stashed override only on genuine incorporation
    // (freshly accepted reply carrying this id); otherwise it stays for
    // the next eligible observation, so the terminal is never lost.
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
                    "window": mover_token,
                    "producer": producer,
                    // Core commit only, not native observed completion:
                    // arrival proof stays a separate live concern.
                    "committed": true,
                }),
            );
        }
        (Some(_), Some(terminal)) if terminal.id == release_id => {
            // The tick applied, but the core settled without committing:
            // honest refused row, then converge once like the unverified
            // leg below.
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
                    "window": mover_token,
                    "producer": producer,
                    "committed": false,
                }),
            );
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, store, dir, &fulls_owned, areas);
        }
        _ => {
            // No incorporated terminal for this id: the tick never ran the
            // gesture (dispatch failure, stale/duplicate delivery, or
            // rejection). Never claim completion; the override stays
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
                    "window": mover_token,
                    "producer": producer,
                    "committed": false,
                }),
            );
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, store, dir, &fulls_owned, areas);
        }
    }
}
