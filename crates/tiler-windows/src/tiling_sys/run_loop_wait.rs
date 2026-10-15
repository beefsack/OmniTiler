use super::*;

// Per-turn WinEvent edge drain for the tile loop: takes the queued hook
// callbacks once, filters own-overlay events, and captures gesture START
// state. Nested `continue`s below stay on the drain loop.
pub(super) struct EdgeDrain {
    pub woke: bool,
    pub batch_total: u64,
    pub wakes: u64,
    pub starts: u64,
    pub ends: u64,
}

pub(super) fn drain_hook_edges(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    log_path: &Path,
) -> EdgeDrain {
    let events = hook_queue()
        .lock()
        .map(|mut queue| std::mem::take(&mut *queue))
        .unwrap_or_default();
    let mut woke = false;
    // Trace-only batch breakdown for the owner-work sample; default
    // runs skip the counting.
    let ow_batch_total: u64 = if state.trace { events.len() as u64 } else { 0 };
    let mut ow_wakes: u64 = 0;
    let mut ow_starts: u64 = 0;
    let mut ow_ends: u64 = 0;
    for event in events {
        match event {
            HookEvent::Wake(raw) => {
                // Own overlay events never wake the loop: no repeated
                // full reconcile off our own move/paint/show.
                if owned_overlay_hwnd(state, RenderCarrier::Border) == Some(raw as u64)
                    || owned_overlay_hwnd(state, RenderCarrier::Underlay) == Some(raw as u64)
                    || owned_overlay_hwnd(state, RenderCarrier::Preview) == Some(raw as u64)
                {
                    continue;
                }
                woke = true;
                if state.trace {
                    ow_wakes += 1;
                }
            }
            HookEvent::MoveSizeStart(raw, start_seq) => {
                if owned_overlay_hwnd(state, RenderCarrier::Border) == Some(raw as u64)
                    || owned_overlay_hwnd(state, RenderCarrier::Underlay) == Some(raw as u64)
                    || owned_overlay_hwnd(state, RenderCarrier::Preview) == Some(raw as u64)
                {
                    continue;
                }
                woke = true;
                if state.trace {
                    ow_starts += 1;
                }
                let hwnd = raw as u64;
                // Shared START capture (Esc snapshot, PRE frame,
                // identity fence); see `capture_gesture_start`. The
                // single post-loop drain below latches edges for
                // both held and just-ended gestures.
                capture_gesture_start(state, me, hwnd, start_seq);
                state.active.insert(hwnd);
                // Move-vs-resize classification, sampled once at START
                // (the WinEvent itself does not distinguish). Hung or
                // unreadable windows classify Unknown: never a trigger.
                let kind = classify_move_size_start(hwnd);
                state.move_kind.insert(hwnd, kind);
                log_json_at(
                    log_path,
                    serde_json::json!({
                        "event": "move-kind",
                        "tick": state.tick,
                        "kind": match kind {
                            MoveSizeKind::Move => "move",
                            MoveSizeKind::Resize => "resize",
                            MoveSizeKind::Unknown => "unknown",
                        },
                    }),
                );
                // START-time pointer for the preview zero gate: a
                // title click with no pointer movement never shows.
                // A failed read fails closed to no preview (the
                // refresh hides on `no-start`).
                if let Some(point) = cursor_pos() {
                    state.gesture_start_cursor.insert(hwnd, point);
                } else {
                    state.gesture_start_cursor.remove(&hwnd);
                }
            }
            HookEvent::MoveSizeEnd(raw, cursor, end_seq) => {
                if owned_overlay_hwnd(state, RenderCarrier::Border) == Some(raw as u64)
                    || owned_overlay_hwnd(state, RenderCarrier::Underlay) == Some(raw as u64)
                    || owned_overlay_hwnd(state, RenderCarrier::Preview) == Some(raw as u64)
                {
                    continue;
                }
                woke = true;
                if state.trace {
                    ow_ends += 1;
                }
                let hwnd = raw as u64;
                // Release cursor carried from the WinEvent callback,
                // never re-read later. A missing capture fails closed
                // to no-change for moves at settle time, so drop any
                // stale entry rather than carrying it forward.
                match cursor {
                    Some(point) => {
                        state.gesture_end_cursor.insert(hwnd, point);
                    }
                    None => {
                        state.gesture_end_cursor.remove(&hwnd);
                    }
                }
                // END-time Esc sequence, first END stands: the drain
                // latches ended gestures only against this snapshot,
                // never against a later counter read.
                if let std::collections::hash_map::Entry::Vacant(e) =
                    state.gesture_end_seq.entry(hwnd)
                {
                    e.insert(end_seq);
                }
                state.active.remove(&hwnd);
                // The START-sampled move/size kind travels to settle:
                // `gesture_tick` reads and clears it exactly once per
                // END, so caption-started moves survive a system
                // rescale (mixed-DPI WM_DPICHANGED) that rect
                // comparison alone would misread as a user resize.
                // Preview and underlay keep reading the held entry
                // until settle runs later this same turn.
            }
        }
    }
    EdgeDrain {
        woke,
        batch_total: ow_batch_total,
        wakes: ow_wakes,
        starts: ow_starts,
        ends: ow_ends,
    }
}
