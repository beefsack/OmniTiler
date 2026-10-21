#![forbid(unsafe_code)]
use super::*;

/// Cross-output drop-preview refresh for one loop wake: the pointer left the
/// START output, so the preview resolves against the pointer output's
/// current view. Occupied destinations run the pair `DragPreview` resolver
/// (source removal plus destination insertion at the pointer, read-only);
/// empty destinations run the singleton admission projection. Never focuses,
/// never writes geometry, never touches topology: the only effect is the
/// owned overlay surface, posted in physical coordinates so it lands on the
/// destination monitor. Sticky group-edge priors degrade to the default
/// depth across outputs (the carry binds the capture domain while the hover
/// lives in the destination domain); same-output samples keep exact carry.
#[allow(clippy::too_many_arguments)]
fn refresh_cross_drag_preview(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    hwnd: u64,
    x: i32,
    y: i32,
    dest_output: &str,
    start: &PreviewStartBinding,
    start_x: i32,
    start_y: i32,
    live_frame: &ObservedWindow,
) {
    use crate::active_border_sys::PREVIEW_DEFAULT_ARGB;
    let probe = hwnd as isize as HWND;
    let Some(dest_workspace) = state.active_view(dest_output) else {
        hide_preview(state, "no-view");
        return;
    };
    if !state.is_tiled_view(dest_output, &dest_workspace) {
        hide_preview(state, "floating-dest");
        return;
    }
    let dest_bounds = areas
        .iter()
        .find(|a| a.device == dest_output)
        .and_then(|area| tiling_domain_bounds_with(area.work, state.outer_gap));
    let inside_dest = dest_bounds.is_some_and(|bounds| {
        crate::tiling::drop_point_in_domain(
            bounds.x,
            bounds.y,
            bounds.w,
            bounds.h,
            x,
            y,
        )
    });
    // START-side invalidation exactly like the same-output path; only the
    // `inside` scope moves to the destination bounds.
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
    let loc = start_key
        .as_ref()
        .and_then(|key| state.member_home(key));
    let loc_ok = loc
        .as_ref()
        .is_some_and(|loc| loc.output == start.output && loc.workspace == start.workspace);
    let domain_fp = state.preview_domain_fp(&start.output);
    let revision_ok = domain_fp == start.domain_fp;
    let float_hold = start_key
        .as_ref()
        .is_some_and(|key| state.sticky.contains_key(key))
        || engine_is_float(state, &start.output, &start.workspace, &start.token)
        || !workspace_mode_tiled(state, &start.output, &start.workspace);
    match preview_sample_gate(&PreviewFresh {
        dead: false,
        zero: !PreviewBound::sample_allowed(start_x, start_y, x, y, false),
        identity_ok,
        tag_ok,
        token_ok,
        loc_ok,
        revision_ok,
        float_hold,
        inside: inside_dest,
    }) {
        PreviewGate::Proceed => {}
        PreviewGate::Transient(reason) => {
            if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
                bound.hover_prior = None;
            }
            hide_preview(state, reason);
            return;
        }
        PreviewGate::Dead(reason) => {
            if reason == "revision-drift" {
                // Revision drift clears the carried prior first: nothing
                // stale may render or ride into a later drop.
                if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
                    bound.hover_prior = None;
                }
            }
            hide_preview(state, reason);
            state.preview_bound.remove(&hwnd);
            state.preview_dead.insert(hwnd);
            return;
        }
    }
    let member_key = state
        .member_tokens
        .iter()
        .find(|(_, token)| token.as_str() == start.token.as_str())
        .map(|(key, _)| key.clone());
    let Some(member_key) = member_key else {
        hide_preview(state, "unmanaged");
        state.preview_bound.remove(&hwnd);
        state.preview_dead.insert(hwnd);
        return;
    };
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        hide_preview(state, "observe-failed");
        return;
    };
    let correlation = state.correlation();
    let settle = CrossDragSettle {
        mover_token: start.token.clone(),
        member_key,
        source_output: start.output.clone(),
        source_workspace: start.workspace.clone(),
        dest_output: dest_output.to_owned(),
        dest_workspace: dest_workspace.clone(),
        x,
        y,
        drop_prior: state
            .preview_bound
            .get(&hwnd)
            .and_then(|bound| bound.hover_prior.clone()),
        producer: "native".to_owned(),
    };
    state
        .preview_bound
        .entry(hwnd)
        .or_insert(PreviewBound { hover_prior: None });
    // World-routed preview: the core owns the drop resolver outright
    // (occupied pair and empty singleton alike), so the sample dispatches
    // the same `DragPreview` operation as the final drop with the carried
    // hover prior and reads the correlated result. Never commits, never
    // writes geometry: only the owned overlay surface renders.
    let (Some(source_wire), Some(dest_wire)) = (
        state.wire_of(&settle.source_output),
        state.wire_of(dest_output),
    ) else {
        hide_preview(state, "unknown-output");
        return;
    };
    let source_key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(source_wire),
        workspace: tiler_core::directional::WorkspaceId(settle.source_workspace.clone()),
    };
    let dest_key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(dest_wire),
        workspace: tiler_core::directional::WorkspaceId(dest_workspace.clone()),
    };
    let foreground = crate::ffi::foreground_window();
    let reply = super::world_tick::run_world_command(
        state,
        me,
        store,
        dir,
        &observed,
        &retained,
        areas,
        foreground,
        fulls,
        tiler_core::world_report::WorldCommand::Operation(CoreCommand::DragPreview {
            window: settle.mover_token.clone(),
            x: settle.x,
            y: settle.y,
            hover_prior: settle.drop_prior.clone(),
            source: Some(source_key.clone()),
        }),
    );
    let Some(preview) = reply.as_ref().and_then(|reply| match &reply.command_result {
        Some(tiler_core::world_report::CommandResult::DragPreview {
            rect,
            hover,
            domain,
            source,
            ..
        }) if domain == &dest_key && source == &source_key => Some((*rect, hover.clone())),
        _ => None,
    }) else {
        if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
            bound.hover_prior = None;
        }
        hide_preview(state, "refused");
        return;
    };
    let (rect, next_prior) = preview;
    if rect.w <= 0 || rect.h <= 0 {
        if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
            bound.hover_prior = None;
        }
        hide_preview(state, "geometry");
        return;
    }
    {
            let prior_desc = preview_prior_desc(&next_prior);
            if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
                bound.hover_prior = next_prior;
            }
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
                    &settle.mover_token,
                    hwnd,
                    lifetime,
                    foreground_lifetime(me),
                    correlation.as_str(),
                    &prior_desc,
                );
                return;
            }
            let outcome = state.preview_overlay.show_fill_above(
                rect,
                PREVIEW_DEFAULT_ARGB,
                &settle.mover_token,
                hwnd,
            );
            let checksum = state.preview_overlay.snapshot()["dib_checksum"].clone();
            let signature = format!(
                "shown:{}:{},{},{},{}:{checksum}",
                settle.mover_token, rect.x, rect.y, rect.w, rect.h
            );
            let changed = state.preview_last.as_deref() != Some(signature.as_str());
            if changed {
                state.preview_last = Some(signature);
            }
            // Routine same-mover repaints (per mouse move) aggregate into the
            // per-carrier ~1s window; mover transitions log immediately below.
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
                settle.mover_token.as_str(),
                Instant::now(),
                changed || state.trace,
                same_thread_sample(
                    outcome_name,
                    settle.mover_token.clone(),
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
                        "window": settle.mover_token,
                        "rect": [rect.x, rect.y, rect.w, rect.h],
                        "hover_prior": prior_desc,
                        "cross_output": true,
                    }),
                );
            }
    }
}

/// Drop-preview refresh for one loop wake while an accepted move gesture
/// holds the loop: a separate owned click-through nonactivating filled
/// target-slot surface ABOVE windows (KWin overlay-item analogue).
///
/// The native caption modal loop (frame-following) feeds this path.
/// Each sample dispatches the world `DragPreview` operation with the
/// carried sticky group-edge hover prior and renders the correlated
/// result. Refusals (self/centre/outside/zero/Esc) hide with no
/// rectangle; only a real moved pointer with a valid resolved target
/// shows.
///
/// Never focuses, never writes geometry, never touches topology: the
/// only effect is the owned overlay surface. Resize, floating, and
/// sticky gestures never show.
pub(super) fn refresh_drag_preview(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    use crate::active_border_sys::PREVIEW_DEFAULT_ARGB;
    // Open gestures only: managed, held, with START capture. Resize-vs-move
    // for the native loop is decided below from live frames, never from
    // the START hit test (a top-edge caption grab hit-tests as a sizing
    // border).
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
        }
    }
    if movers.is_empty() {
        // No open move: late samples after Finish must never render.
        if render_carrier_visible(state, RenderCarrier::Preview) || !state.preview_bound.is_empty()
        {
            hide_preview(state, "idle");
        }
        state.preview_bound.clear();
        // Gesture-scoped diagnostic: an open managed hold with no move
        // classification (resize/unknown) explains a silent preview.
        // Change-only so a held resize logs once, never per pump.
        let held: Vec<&str> = state
            .active
            .iter()
            .filter(|hwnd| state.managed.contains(hwnd))
            .map(|hwnd| match state.move_kind.get(hwnd) {
                Some(MoveSizeKind::Move) => "move-unheld",
                Some(MoveSizeKind::Resize) => "resize",
                _ => "unknown",
            })
            .collect();
        if !held.is_empty() {
            let signature = format!("idle:{}", held.join(","));
            if state.preview_last.as_deref() != Some(signature.as_str()) {
                state.preview_last = Some(signature);
                log_json_at(
                    &state.log_path,
                    serde_json::json!({
                        "event": "drag-preview",
                        "tick": state.tick,
                        "outcome": "idle",
                        "held": held,
                    }),
                );
            }
        }
        return;
    }
    // One gesture at a time by construction; extra entries fail closed
    // with no rebind until settle.
    if movers.len() > 1 {
        hide_preview(state, "multi-gesture");
        state.preview_bound.clear();
        for hwnd in movers {
            state.preview_dead.insert(hwnd);
            state.gesture_start_cursor.remove(&hwnd);
        }
        return;
    }
    let hwnd = movers[0];
    // Logical cancel hides IMMEDIATELY, never waiting for the modal
    // loop to end it.
    if state.esc_latched.contains(&hwnd) {
        hide_preview(state, "esc-cancelled");
        state.preview_bound.remove(&hwnd);
        state.preview_dead.remove(&hwnd);
        state.gesture_preview_start.remove(&hwnd);
        state.gesture_start_cursor.remove(&hwnd);
        return;
    }
    // A dead gesture never rebinds and never samples again until settle.
    if state.preview_dead.contains(&hwnd) {
        return;
    }
    // START-frozen source binding (token, output/workspace, domain
    // fingerprint): bound at START, never rebound. Drift fails the whole
    // gesture closed below.
    let Some(start) = state.gesture_preview_start.get(&hwnd).cloned() else {
        hide_preview(state, "no-start");
        state.preview_dead.insert(hwnd);
        return;
    };
    // Frame-grounded move gate on both producers: a move keeps the
    // pre-gesture size while the origin travels; a resize changes it. The
    // START hit test cannot decide this (top-edge caption grabs hit-test as
    // sizing borders), so resize holds hide here the moment any edge moves.
    let probe = hwnd as isize as HWND;
    let live_frame = match observe_window(probe, me, fulls, &mut state.tokens) {
        Ok(window) => window,
        Err(_) => {
            hide_preview(state, "no-target");
            state.preview_bound.remove(&hwnd);
            state.preview_dead.insert(hwnd);
            return;
        }
    };
    let modal_move = state
        .move_kind
        .get(&hwnd)
        .is_some_and(|kind| *kind == MoveSizeKind::Move);
    let moved_size = modal_move
        || state.gesture_before.get(&hwnd).is_some_and(|before| {
            preview_same_size(
                before.w,
                before.h,
                live_frame.outer.w,
                live_frame.outer.h,
                live_frame.visible.w,
                live_frame.visible.h,
            )
        });
    if !moved_size {
        hide_preview(state, "resize");
        state.preview_bound.remove(&hwnd);
        state.preview_dead.insert(hwnd);
        return;
    }
    let Some((start_x, start_y)) = state.gesture_start_cursor.get(&hwnd).copied() else {
        hide_preview(state, "no-start");
        state.preview_bound.remove(&hwnd);
        state.preview_dead.insert(hwnd);
        return;
    };
    let Some((x, y)) = cursor_pos() else {
        hide_preview(state, "no-cursor");
        return;
    };
    if let Some(entries) = state.allowlist.as_ref()
        && entries.iter().all(|entry| entry.hwnd != hwnd)
    {
        hide_preview(state, "allowlist-changed");
        state.preview_bound.remove(&hwnd);
        state.preview_dead.insert(hwnd);
        return;
    }
    // Cross-output preview: the pointer left the START output. The pointer
    // output's current view scopes resolution; the same-output path below
    // stays untouched.
    if let Some(dest) = output_for_point(areas, x, y)
        && dest != start.output
    {
        refresh_cross_drag_preview(
            state,
            me,
            store,
            dir,
            fulls,
            areas,
            hwnd,
            x,
            y,
            &dest,
            &start,
            start_x,
            start_y,
            &live_frame,
        );
        return;
    }
    // Fresh invalidation facts against the START binding: identity, tag,
    // remap, membership, domain fingerprint, float, and same-output scope.
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
    let loc = start_key
        .as_ref()
        .and_then(|key| state.member_home(key));
    let loc_ok = loc
        .as_ref()
        .is_some_and(|loc| loc.output == start.output && loc.workspace == start.workspace);
    let domain_fp = state.preview_domain_fp(&start.output);
    let revision_ok = domain_fp == start.domain_fp;
    let float_hold = start_key
        .as_ref()
        .is_some_and(|key| state.sticky.contains_key(key))
        || engine_is_float(state, &start.output, &start.workspace, &start.token)
        || !workspace_mode_tiled(state, &start.output, &start.workspace);
    let start_bounds = areas
        .iter()
        .find(|a| a.device == start.output)
        .and_then(|area| tiling_domain_bounds_with(area.work, state.outer_gap));
    let inside = start_bounds.is_some_and(|bounds| {
        drop_point_in_domain(
            bounds.x,
            bounds.y,
            bounds.w,
            bounds.h,
            x,
            y,
        )
    });
    match preview_sample_gate(&PreviewFresh {
        dead: false,
        zero: !PreviewBound::sample_allowed(start_x, start_y, x, y, false),
        identity_ok,
        tag_ok,
        token_ok,
        loc_ok,
        revision_ok,
        float_hold,
        inside,
    }) {
        PreviewGate::Proceed => {}
        PreviewGate::Transient(reason) => {
            if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
                bound.hover_prior = None;
            }
            hide_preview(state, reason);
            return;
        }
        PreviewGate::Dead(reason) => {
            if reason == "revision-drift" {
                // Fingerprint drift clears the carried prior first: nothing
                // stale may render or ride into a later drop.
                if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
                    bound.hover_prior = None;
                }
            }
            hide_preview(state, reason);
            state.preview_bound.remove(&hwnd);
            state.preview_dead.insert(hwnd);
            return;
        }
    }
    if start_bounds.is_none() {
        hide_preview(state, "no-group");
        state.preview_bound.remove(&hwnd);
        state.preview_dead.insert(hwnd);
        return;
    };
    // Fresh observation for the world preview command below; enumeration
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
        state.preview_dead.insert(hwnd);
        return;
    }
    // The 100ms pump already coalesces on the latest pointer; every
    // reaching sample re-resolves so fresh size-hint changes update the
    // preview even while the pointer is static.
    state
        .preview_bound
        .entry(hwnd)
        .or_insert(PreviewBound { hover_prior: None });
    let carried = state
        .preview_bound
        .get(&hwnd)
        .and_then(|bound| bound.hover_prior.clone());
    let Some(source_wire) = state.wire_of(&start.output) else {
        hide_preview(state, "unknown-output");
        return;
    };
    let source_key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(source_wire),
        workspace: tiler_core::directional::WorkspaceId(start.workspace.clone()),
    };
    let foreground = crate::ffi::foreground_window();
    let reply = super::world_tick::run_world_command(
        state,
        me,
        store,
        dir,
        &observed,
        &retained,
        areas,
        foreground,
        fulls,
        tiler_core::world_report::WorldCommand::Operation(CoreCommand::DragPreview {
            window: start.token.clone(),
            x,
            y,
            hover_prior: carried,
            source: None,
        }),
    );
    // Source fence on the correlated result (START bound): a stale result
    // after Finish, drift, or a mover swap never renders.
    let Some((rect, next_prior)) = reply.as_ref().and_then(|reply| match &reply.command_result {
        Some(tiler_core::world_report::CommandResult::DragPreview {
            rect,
            hover,
            source,
            ..
        }) if source == &source_key => Some((*rect, hover.clone())),
        _ => None,
    }) else {
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
    let outcome = state.preview_overlay.show_fill_above(
        rect,
        PREVIEW_DEFAULT_ARGB,
        &start.token,
        hwnd,
    );
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
