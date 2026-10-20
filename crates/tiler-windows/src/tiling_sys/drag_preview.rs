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
    let Some(dest_workspace) = state.workspaces.active_id(dest_output) else {
        hide_preview(state, "no-view");
        return;
    };
    if !state.workspaces.is_tiled(dest_output, &dest_workspace) {
        hide_preview(state, "floating-dest");
        return;
    }
    let dest_domain = workspace_domain_for(
        dest_output,
        &dest_workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    );
    let inside_dest = dest_domain.as_ref().is_some_and(|(domain, _)| {
        crate::tiling::drop_point_in_domain(
            domain.bounds.x,
            domain.bounds.y,
            domain.bounds.w,
            domain.bounds.h,
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
        (Some(key), Some(identity)) => crate::workspace_owner::member_matches(
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
        .and_then(|key| state.workspaces.member_loc(key).cloned());
    let loc_ok = loc
        .as_ref()
        .is_some_and(|loc| loc.output == start.output && loc.workspace == start.workspace);
    let revision = revision_for(state, &start.output, &start.workspace);
    let revision_ok = revision == start.revision;
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
        start_rect: state.gesture_before.get(&hwnd).copied(),
    };
    state
        .preview_bound
        .entry(hwnd)
        .or_insert(PreviewBound { hover_prior: None });
    // Occupied destinations resolve through the pair route; empty ones
    // project the singleton admission. Occupancy reads project membership
    // (no observation assembly, no hint budget): row assembly runs exactly
    // once inside the prepare call below. Both share the Engine drop
    // resolver with the final drop.
    let dest_occupied = !state
        .workspaces
        .workspace_members(dest_output, &dest_workspace)
        .is_empty();
    let mut hint_cx = HintCx::new();
    let event = if dest_occupied {
        prepare_cross_drag_preview(state, areas, &observed, &retained, &settle, &correlation)
    } else {
        prepare_cross_singleton_event(
            state,
            areas,
            &observed,
            &settle,
            &correlation,
            &mut hint_cx,
            true,
        )
    };
    let Some(mut event) = event else {
        hide_preview(state, "incomplete");
        return;
    };
    // Expected pair revision for the reply fence (the convergence persists
    // drift, so a mismatch hides stale rather than rendering it). Singleton
    // previews fence on domain plus mover only: the fresh destination has no
    // prior revision to go stale against.
    let expected_revision =
        if dest_occupied {
            Some(
                revision_for(state, &settle.source_output, &settle.source_workspace).max(
                    revision_for(state, &settle.dest_output, &settle.dest_workspace),
                ),
            )
        } else {
            None
        };
    let dest_key = event.domain_key.clone();
    match handle_engine_event(state, &mut event) {
        CoreReply::DragPreview(plan) => {
            let preview = &plan.preview;
            let revision_ok = expected_revision.is_none_or(|expected| preview.revision == expected);
            if preview.domain != dest_key
                || !revision_ok
                || preview.source_window.0 != settle.mover_token
            {
                hide_preview(state, "stale");
                state.preview_bound.remove(&hwnd);
                state.preview_dead.insert(hwnd);
                return;
            }
            let rect = preview.proposed_rect;
            if rect.w <= 0 || rect.h <= 0 {
                if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
                    bound.hover_prior = None;
                }
                hide_preview(state, "geometry");
                return;
            }
            let next_prior = preview.hover_prior();
            let prior_desc = preview_prior_desc(&Some(next_prior.clone()));
            if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
                bound.hover_prior = Some(next_prior);
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
        CoreReply::Rejected { kind, .. } => {
            log_redacted_dispatch(
                state,
                "drag-preview",
                state.tick,
                correlation.as_str(),
                &settle.mover_token,
                &settle.producer,
                settle.start_rect,
                &event,
                &observed,
                kind,
            );
            if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
                bound.hover_prior = None;
            }
            hide_preview(state, preview_hide_for_refusal(kind));
        }
        CoreReply::Diverged(reason) => {
            log_redacted_dispatch(
                state,
                "drag-preview",
                state.tick,
                correlation.as_str(),
                &settle.mover_token,
                &settle.producer,
                settle.start_rect,
                &event,
                &observed,
                reason.as_str(),
            );
            hide_preview(state, reason.as_str());
            state.preview_bound.remove(&hwnd);
            state.preview_dead.insert(hwnd);
        }
        CoreReply::SnapshotInvalid { detail, .. } => {
            log_redacted_dispatch(
                state,
                "drag-preview",
                state.tick,
                correlation.as_str(),
                &settle.mover_token,
                &settle.producer,
                settle.start_rect,
                &event,
                &observed,
                detail,
            );
            if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
                bound.hover_prior = None;
            }
            hide_preview(state, detail);
        }
        _ => {
            log_redacted_dispatch(
                state,
                "drag-preview",
                state.tick,
                correlation.as_str(),
                &settle.mover_token,
                &settle.producer,
                settle.start_rect,
                &event,
                &observed,
                "refused",
            );
            if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
                bound.hover_prior = None;
            }
            hide_preview(state, "refused");
        }
    }
}

/// Build the pair `DragPreview` event for an occupied destination: same
/// shared row assembly as the final drop, read-only.
pub(super) fn prepare_cross_drag_preview(
    state: &mut TileLoop,
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    settle: &CrossDragSettle,
    correlation: &CorrelationId,
) -> Option<CoreEvent> {
    let rows = assemble_cross_drag_rows(
        state,
        areas,
        observed,
        retained,
        settle,
        correlation,
        correlation.as_str(),
        "drag-preview",
    )?;
    let from = WindowId(settle.mover_token.clone());
    let dest_windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
        .dest_rows
        .iter()
        .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
        .collect();
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        correlation,
        revision_for(state, &settle.dest_output, &settle.dest_workspace),
        rows.combined_fp,
        &rows.dest_domain,
        &rows.dest_key,
        state.outer_gap,
        &dest_windows,
        Some(&from),
    );
    for row in &rows.source_rows {
        event.windows.push(tiler_core::seed::EngineWindow {
            window: WindowId(row.token.clone()),
            output: rows.source_key.output.clone(),
            workspace: rows.source_key.workspace.clone(),
            rect: row.rect,
            floating: row.floating,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: row.hints,
        });
    }
    event.directional = Some(vec![
        (rows.source_domain, rows.source_key.clone()),
        (rows.dest_domain, rows.dest_key),
    ]);
    event.directional_target_outer_gap = Some(state.outer_gap);
    event.command = CoreCommand::DragPreview {
        window: from.0.clone(),
        x: settle.x,
        y: settle.y,
        hover_prior: settle.drop_prior.clone(),
        source: Some(rows.source_key),
    };
    Some(event)
}

/// Build the singleton event for an empty destination: the mover row alone
/// homed on the destination, no pair. The Engine fresh-seeds and either
/// projects the sole admission (preview) or commits it (drop); preview and
/// drop share this exact shape. Fresh destinations carry current gaps, so no
/// gap adoption runs (adoption would converge a session during preview, and
/// the drop seeds fresh the same way).
pub(super) fn prepare_cross_singleton_event(
    state: &mut TileLoop,
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    settle: &CrossDragSettle,
    correlation: &CorrelationId,
    hint_cx: &mut HintCx,
    preview: bool,
) -> Option<CoreEvent> {
    let current = observed.iter().find(|w| w.token == settle.mover_token)?;
    let hints = min_hint_for(current, hint_cx).hints;
    let (dest_domain, dest_key) = workspace_domain_for(
        &settle.dest_output,
        &settle.dest_workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    )?;
    let source_key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(settle.source_output.clone()),
        workspace: tiler_core::directional::WorkspaceId(settle.source_workspace.clone()),
    };
    let mover = WindowId(settle.mover_token.clone());
    // Same retained-capture substitution as the pair route: the singleton
    // seed observation claims the mover at its tile slot, never at a
    // traveled native frame. Hints stay fresh from the live frame.
    let frame = settle.start_rect.unwrap_or(current.visible);
    let fp = crate::tiling::fingerprint(&[(settle.mover_token.clone(), frame)]);
    // Fresh destination: the seed carries current gaps, so no gap adoption
    // runs here (adoption would converge a session during a read-only
    // preview). The drop path below shares this shape.
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        correlation,
        revision_for(state, &settle.dest_output, &settle.dest_workspace),
        fp,
        &dest_domain,
        &dest_key,
        state.outer_gap,
        &[(mover.clone(), frame, hints, false)],
        Some(&mover),
    );
    event.command = if preview {
        CoreCommand::DragPreview {
            window: mover.0.clone(),
            x: settle.x,
            y: settle.y,
            hover_prior: settle.drop_prior.clone(),
            source: Some(source_key),
        }
    } else {
        CoreCommand::DragDrop {
            window: mover.0.clone(),
            x: settle.x,
            y: settle.y,
            hover_prior: settle.drop_prior.clone(),
            source: Some(source_key),
        }
    };
    Some(event)
}

/// Drop-preview refresh for one loop wake while an accepted move gesture
/// holds the loop: a separate owned click-through nonactivating filled
/// target-slot surface ABOVE windows (KWin overlay-item analogue).
///
/// The native caption modal loop (frame-following) feeds this path.
/// Each 100ms pump sample coalesces on pointer movement, assembles a fresh
/// complete canonical domain observation with size hints, and routes the
/// SAME Engine `DragPreview` resolver as the final drop, carrying the exact
/// sticky group-edge hover prior across samples (and into the drop via
/// `preview_bound`). Refusals (self/centre/outside/zero/Esc) hide with no
/// rectangle; only a real moved pointer with a valid resolved target shows.
///
/// Never focuses, never writes geometry, never touches topology or the
/// reconciler: the Engine call is the read-only working-clone preview, and
/// the only effect is the owned overlay surface. Resize, floating, and
/// sticky gestures never show.
pub(super) fn refresh_drag_preview(
    state: &mut TileLoop,
    me: &ProcessIdentity,
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
    // START-frozen source binding (token, output/workspace, revision): bound
    // at START, never rebound. Drift fails the whole gesture closed below.
    let Some(start) = state.gesture_preview_start.get(&hwnd).cloned() else {
        hide_preview(state, "no-start");
        state.preview_dead.insert(hwnd);
        return;
    };
    // Frame-grounded move gate on both producers: a move keeps the
    // pre-gesture size while the origin travels; a resize changes it. The
    // START hit test cannot decide this (top-edge caption grabs hit-test as
    // sizing borders), so resize holds hide here the moment any edge moves.
    // Lane care: `gesture_before` prefers the stable visible (DWM
    // extended-frame) rect with a GetWindowRect-outer fallback, so either
    // lane matching means same-size (accepted review outcome; a true resize
    // moves both). Modal truth wins first: a caption-started move loop
    // carries move intent even through a system rescale (mixed-DPI
    // WM_DPICHANGED), which frame comparison alone would misread as a
    // resize and kill the preview mid-gesture.
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
    // output's current view scopes resolution (occupied pair route or empty
    // singleton projection); the same-output path below stays untouched.
    if let Some(dest) = output_for_point(areas, x, y)
        && dest != start.output
    {
        refresh_cross_drag_preview(
            state,
            me,
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
    // remap, membership, revision, float, and same-output scope. The pure
    // gate below orders them (invalidation fails the whole gesture closed
    // even with a static pointer; zero/outside hide transiently). No
    // pointer-movement short-circuit runs before these checks, so a
    // lingering rectangle can never survive mover death, reuse, or domain
    // drift on a stationary pointer.
    let live = window_identity(probe, hwnd, me);
    let live_tag = crate::product_hide::sys::read_member_tag(hwnd);
    let start_key = state.gesture_start_key.get(&hwnd).cloned();
    let identity_ok = match (&start_key, &live) {
        (Some(key), Some(identity)) => crate::workspace_owner::member_matches(
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
        .and_then(|key| state.workspaces.member_loc(key).cloned());
    let loc_ok = loc
        .as_ref()
        .is_some_and(|loc| loc.output == start.output && loc.workspace == start.workspace);
    let revision = revision_for(state, &start.output, &start.workspace);
    let revision_ok = revision == start.revision;
    let float_hold = start_key
        .as_ref()
        .is_some_and(|key| state.sticky.contains_key(key))
        || engine_is_float(state, &start.output, &start.workspace, &start.token)
        || !workspace_mode_tiled(state, &start.output, &start.workspace);
    let domain = workspace_domain_for(
        &start.output,
        &start.workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    );
    let inside = domain.as_ref().is_some_and(|(domain, _)| {
        drop_point_in_domain(
            domain.bounds.x,
            domain.bounds.y,
            domain.bounds.w,
            domain.bounds.h,
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
    let Some((domain, domain_key)) = domain else {
        hide_preview(state, "no-group");
        state.preview_bound.remove(&hwnd);
        state.preview_dead.insert(hwnd);
        return;
    };
    // Fresh complete canonical domain observation with size hints: the same
    // row assembly as the settle path, so preview and drop resolve over
    // identical inputs through the shared resolver. Enumeration or row
    // failure hides (never a lingering rectangle on incomplete input).
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        hide_preview(state, "observe-failed");
        return;
    };
    let correlation = state.correlation();
    let mut hint_cx = HintCx::new();
    let Some(rows) = assemble_domain_rows(
        state,
        &start.output,
        &start.workspace,
        &observed,
        &retained,
        "drag-preview",
        correlation.as_str(),
        &mut hint_cx,
    ) else {
        hide_preview(state, "incomplete");
        return;
    };
    if !rows.iter().any(|row| row.token == start.token) {
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
    let windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
        .iter()
        .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
        .collect();
    let fp = fingerprint(
        &rows
            .iter()
            .map(|r| (r.token.clone(), r.rect))
            .collect::<Vec<_>>(),
    );
    let mover = WindowId(start.token.clone());
    let carried = state
        .preview_bound
        .get(&hwnd)
        .and_then(|bound| bound.hover_prior.clone());
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &correlation,
        start.revision,
        fp,
        &domain,
        &domain_key,
        OUTER_GAP,
        &windows,
        Some(&mover),
    );
    event.command = CoreCommand::DragPreview {
        window: start.token.clone(),
        x,
        y,
        hover_prior: carried,
        source: None,
    };
    // Correlation/domain/revision/mover fence on the reply, all START
    // bound: a stale reply after Finish, drift, or a mover swap never
    // renders.
    match handle_engine_event(state, &mut event) {
        CoreReply::DragPreview(plan) => {
            let preview = &plan.preview;
            if preview.domain != domain_key
                || preview.revision != start.revision
                || preview.source_window.0 != start.token
            {
                hide_preview(state, "stale");
                state.preview_bound.remove(&hwnd);
                state.preview_dead.insert(hwnd);
                return;
            }
            let rect = preview.proposed_rect;
            if rect.w <= 0 || rect.h <= 0 {
                if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
                    bound.hover_prior = None;
                }
                hide_preview(state, "geometry");
                return;
            }
            // Carry the exact sticky hover prior into the next sample and
            // the final drop (the Engine owns the 32/80 semantics; this
            // only forwards the opaque value).
            let next_prior = preview.hover_prior();
            let prior_desc = preview_prior_desc(&Some(next_prior.clone()));
            if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
                bound.hover_prior = Some(next_prior);
            }
            // Bridged present: post the Engine-resolved slot with the sample
            // correlation and carried prior for the ack-driven log line,
            // fenced on the fresh mover lifetime identity (the mover band
            // is never a requirement: the preview plane is always topmost)
            // plus the foreground identity for the renderer-side switch
            // fence (an unreadable foreground posts `None` and refuses
            // narrowly per the binding contract; readable unchanged
            // foregrounds keep unfocused-hold semantics). The applied
            // outcome logs on the acknowledgement.
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
        CoreReply::Rejected { kind, .. } => {
            if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
                bound.hover_prior = None;
            }
            // Self, centre-stack, and outside refusals show no rectangle.
            hide_preview(state, preview_hide_for_refusal(kind));
        }
        CoreReply::Diverged(reason) => {
            hide_preview(state, reason.as_str());
            state.preview_bound.remove(&hwnd);
            state.preview_dead.insert(hwnd);
        }
        CoreReply::SnapshotInvalid { detail, .. } => {
            if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
                bound.hover_prior = None;
            }
            hide_preview(state, detail);
        }
        _ => {
            if let Some(bound) = state.preview_bound.get_mut(&hwnd) {
                bound.hover_prior = None;
            }
            hide_preview(state, "refused");
        }
    }
}
