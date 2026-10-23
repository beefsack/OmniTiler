#![forbid(unsafe_code)]
use super::*;

/// Drop-preview refresh for one loop wake while an accepted move gesture
/// holds the loop: a separate owned click-through nonactivating filled
/// target-slot surface ABOVE windows (KWin overlay-item analogue).
///
/// Mechanical renderer only (stage 4a): the core owns the pointer target,
/// the overlay shape, and every content decision. This refresh applies the
/// native fences (single open gesture, Esc, START lifetime identity, fresh
/// observation) then runs the ordinary observation tick — whose capture
/// already carries the Updated ingress — and renders the core-published
/// drop overlay. Refusals (including the core's own zero/outside/float/
/// revision holds) hide with no rectangle. Never focuses, never writes
/// geometry, never touches topology: the only effect is the owned overlay
/// surface, posted in physical coordinates.
///
/// Cross-output points take the same path: the pointer output's current
/// view scopes the core resolution, and the reply carries the destination
/// domain. No adapter output selection, no inset checks, no size gates.
pub(super) fn refresh_drag_preview(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    use crate::active_border_sys::PREVIEW_DEFAULT_ARGB;
    // Suspended (or foreign-elevated) turns never preview: no overlay work
    // runs under a foreign focused game or elevated foreground, and the
    // game lane unsamples the cursor entirely (the core game pause stays
    // the second fence).
    if state.suspended || suspend_read(state, me, fulls).veto.block || foreground_elevated(me) {
        hide_preview(state, "suspended");
        return;
    }
    // Open gestures only: managed, held, with START capture.
    let mut movers: Vec<u64> = state
        .active
        .iter()
        .copied()
        .filter(|hwnd| state.managed.contains(hwnd) && state.gesture_before.contains_key(hwnd))
        .collect();
    movers.sort_unstable();
    // Preview-unrelated holds (no START capture, e.g. unmanaged races)
    // never keep a rectangle.
    for hwnd in state.active.iter().copied().collect::<Vec<_>>() {
        if !movers.contains(&hwnd) && state.preview_bound.remove(&hwnd).is_some() {
            state.gesture_start_cursor.remove(&hwnd);
            state.gesture_started_sent.remove(&hwnd);
        }
    }
    if movers.is_empty() {
        // No open move: late samples after Finish must never render.
        if render_carrier_visible(state, RenderCarrier::Preview) || !state.preview_bound.is_empty()
        {
            hide_preview(state, "idle");
        }
        state.preview_bound.clear();
        return;
    }
    // One gesture at a time by construction; extra entries fail closed
    // with no preview until settle.
    if movers.len() > 1 {
        hide_preview(state, "multi-gesture");
        state.preview_bound.clear();
        for hwnd in movers {
            state.gesture_start_cursor.remove(&hwnd);
            state.gesture_started_sent.remove(&hwnd);
        }
        return;
    }
    let hwnd = movers[0];
    // Logical cancel hides IMMEDIATELY, never waiting for the modal
    // loop to end it.
    if state.esc_latched.contains(&hwnd) {
        hide_preview(state, "esc-cancelled");
        state.preview_bound.remove(&hwnd);
        state.gesture_preview_start.remove(&hwnd);
        state.gesture_start_cursor.remove(&hwnd);
        return;
    }
    // START-frozen source binding (token, output/workspace): bound at
    // START, never rebound. The core owns every drift decision from the
    // staged observation; this only proves the mover is still the START
    // lifetime before spending an observation tick on it.
    let Some(start) = state.gesture_preview_start.get(&hwnd).cloned() else {
        hide_preview(state, "no-start");
        return;
    };
    // Native lifetime fence against the START snapshot: the fresh window
    // must still match the START-time identity snapshot plus the live
    // member lifetime tag and the member token. Anything else hides with
    // no preview and no overlay work; the END path owns the settle.
    let probe = hwnd as isize as HWND;
    let live = window_identity(probe, hwnd, me);
    let live_tag = crate::product_hide::sys::read_member_tag(hwnd);
    let start_key = state.gesture_start_key.get(&hwnd).cloned();
    let identity_ok = match (&start_key, &live) {
        (Some(key), Some(identity)) => crate::native_guards::member_matches(
            key,
            hwnd,
            identity.pid,
            &identity.process_creation,
        ),
        _ => false,
    };
    let tag_ok = state
        .gesture_start_tag
        .get(&hwnd)
        .is_none_or(|stored| stored == &live_tag);
    let token_ok = start_key.as_ref().is_some_and(|key| {
        state
            .member_tokens
            .get(key)
            .is_some_and(|token| token.as_str() == start.token.as_str())
    });
    if !identity_ok || !tag_ok || !token_ok {
        if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
            bound.hover_prior = None;
        }
        hide_preview(state, "identity-changed");
        state.preview_bound.remove(&hwnd);
        return;
    }
    if let Some(entries) = state.allowlist.as_ref()
        && entries.iter().all(|entry| entry.hwnd != hwnd)
    {
        hide_preview(state, "allowlist-changed");
        state.preview_bound.remove(&hwnd);
        return;
    }
    // Fresh observation for the world preview below; enumeration
    // failure hides (never a lingering rectangle on incomplete input).
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        hide_preview(state, "observe-failed");
        return;
    };
    let correlation = state.correlation();
    if !state
        .member_tokens
        .values()
        .any(|token| token == &start.token)
    {
        hide_preview(state, "unmanaged");
        state.preview_bound.remove(&hwnd);
        return;
    }
    // The 100ms pump already coalesces on the latest pointer; every
    // reaching sample re-resolves so fresh size-hint changes update the
    // preview even while the pointer is static.
    state
        .preview_bound
        .entry(hwnd)
        .or_insert(PreviewBound { hover_prior: None });
    // Normalized preview (stage 4a): the tick's capture already carries
    // the Updated ingress for this open gesture, so the observation tick
    // alone resolves the core drop target and publishes the drop-overlay
    // shape. The carried hover prior lives in the core session; the bound
    // below only mirrors the result for change-only rendering.
    let Some(source_wire) = state.wire_of(&start.output) else {
        hide_preview(state, "unknown-output");
        return;
    };
    let source_key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(source_wire),
        workspace: tiler_core::directional::WorkspaceId(start.workspace.clone()),
    };
    let foreground = crate::ffi::foreground_window();
    // One normalized sample per capture, never a raw read: out-of-bound
    // values drop to `None` so they can never refuse a report or steer
    // the core target.
    let cursor = super::observation::sample_cursor();
    let reply = super::world_tick::run_observation_tick(
        state, me, store, dir, &observed, &retained, areas, foreground, fulls, cursor,
    );
    // Source fence on the correlated result (START bound): a stale result
    // after Finish, drift, or a mover swap never renders.
    let Some((rect, next_prior)) = reply
        .as_ref()
        .and_then(|reply| match &reply.command_result {
            Some(tiler_core::world_report::CommandResult::DragPreview {
                rect,
                hover,
                source,
                ..
            }) if source == &source_key => Some((*rect, hover.clone())),
            _ => None,
        })
    else {
        if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
            bound.hover_prior = None;
        }
        hide_preview(state, "refused");
        return;
    };
    if rect.w <= 0 || rect.h <= 0 {
        if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
            bound.hover_prior = None;
        }
        hide_preview(state, "geometry");
        return;
    }
    // Carry the exact sticky hover prior into the next sample and
    // the final drop (the core owns the hover semantics; this
    // only forwards the opaque value).
    let prior_desc = preview_prior_desc(&next_prior);
    if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
        bound.hover_prior = next_prior;
    }
    let live_frame = match observe_window(probe, me, fulls, &mut state.tokens) {
        Ok(window) => window,
        Err(_) => {
            hide_preview(state, "no-target");
            return;
        }
    };
    if state.render_bridge.is_some() {
        let lifetime = WindowLifetime {
            pid: live_frame.identity.pid,
            creation: live_frame.identity.process_creation.clone(),
            member_tag: crate::product_hide::sys::read_member_tag(hwnd),
        };
        post_preview_frame(
            state,
            rect,
            PREVIEW_DEFAULT_ARGB,
            &start.token,
            hwnd,
            lifetime,
            foreground_lifetime(me),
            correlation.as_str(),
            &prior_desc,
        );
        return;
    }
    let outcome =
        state
            .preview_overlay
            .show_fill_above(rect, PREVIEW_DEFAULT_ARGB, &start.token, hwnd);
    let checksum = state.preview_overlay.snapshot()["dib_checksum"].clone();
    let signature = format!(
        "shown:{}:{},{},{},{}:{checksum}",
        start.token, rect.x, rect.y, rect.w, rect.h
    );
    let changed = state.preview_last.as_deref() != Some(signature.as_str());
    if changed {
        state.preview_last = Some(signature);
    }
    let outcome_name = match outcome {
        OverlayOutcome::Shown => "shown",
        OverlayOutcome::Redrew => "redrew",
        OverlayOutcome::Moved => "moved",
        OverlayOutcome::Unchanged => "unchanged",
        OverlayOutcome::Hidden => "hidden",
    };
    if render_same_thread_gate(
        state,
        RenderCarrier::Preview,
        start.token.as_str(),
        Instant::now(),
        changed || state.trace,
        same_thread_sample(
            outcome_name,
            start.token.clone(),
            [rect.x, rect.y, rect.w, rect.h],
            String::new(),
            checksum.as_str().unwrap_or_default().to_owned(),
        ),
    ) {
        return;
    }
    if changed || state.trace {
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "drag-preview",
                "tick": state.tick,
                "correlation": correlation.as_str(),
                "outcome": match outcome {
                    OverlayOutcome::Shown => "shown",
                    OverlayOutcome::Redrew => "redrew",
                    OverlayOutcome::Moved => "moved",
                    OverlayOutcome::Unchanged => "unchanged",
                    OverlayOutcome::Hidden => "hidden",
                },
                "window": start.token,
                "rect": [rect.x, rect.y, rect.w, rect.h],
                "hover_prior": prior_desc,
            }),
        );
    }
}
