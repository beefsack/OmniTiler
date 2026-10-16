use super::*;

/// Bounded pending gesture drop for the bridged finish path. Owns every
/// input the deferred settle needs (END-time intent, START snapshots,
/// membership routing, carried hover prior, hide pair, deadline) so the
/// service revalidates against fresh observations instead of trusting
/// pre-wait facts. No callbacks, no Engine, no framework: one struct, one
/// service function on the owner loop.
pub(super) struct PendingDrop {
    pub(super) hwnd: u64,
    pub(super) producer: String,
    pub(super) intent: GestureIntent,
    pub(super) mover_token: String,
    pub(super) start_key: Option<crate::workspace::WindowKey>,
    pub(super) start_tag: Option<Option<String>>,
    pub(super) member_key: crate::workspace::WindowKey,
    pub(super) output: String,
    pub(super) workspace: String,
    pub(super) drop_prior: Option<tiler_core::session::DragHoverPrior>,
    /// Pointer-resolved cross-output destination plus its END-time fence
    /// snapshot (`None` for same-output finishes, which keep the legacy
    /// source-domain service path).
    pub(super) dest: Option<PendingCrossDest>,
    /// PRE-gesture allocation of the mover for the source-capture row
    /// substitution (`gesture_before` read at queue time; the map entry is
    /// dropped with the gesture before the ack arrives).
    pub(super) source_rect: Option<Rect>,
    pub(super) hide_gen: u64,
    pub(super) hide_safety: u64,
    pub(super) deadline: Instant,
}

/// Pointer-resolved cross-output finish: the destination output plus its
/// current workspace at END and the fence snapshot tails re-verify before
/// any Engine or project mutation.
pub(super) struct PendingCrossDest {
    pub(super) output: String,
    pub(super) workspace: String,
    pub(super) fence: crate::workspace_owner::DragDropFenceSnapshot,
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
/// stashes the drop with its hide pair and deadline. Returns `true` when a
/// previous finish still owns the slot (overlapping END race): the newcomer
/// is refused narrowly with no drop and no reconcile, and the pending
/// operation owns the settle. Single slot: one gesture settles at a time.
/// Production END queueing funnels through here; tests exercise it directly
/// with offline fixtures (channels only, no renderer thread).
#[allow(clippy::too_many_arguments)]
pub(super) fn queue_pending_drop(
    state: &mut TileLoop,
    hwnd: &u64,
    producer: &'static str,
    intent: GestureIntent,
    mover_token: &str,
    start_key: Option<crate::workspace::WindowKey>,
    start_tag: Option<Option<String>>,
    member_key: crate::workspace::WindowKey,
    output: &str,
    workspace: &str,
    dest: Option<PendingCrossDest>,
    source_rect: Option<Rect>,
) -> bool {
    let drop_prior = state
        .preview_bound
        .get(hwnd)
        .and_then(|bound| bound.hover_prior.clone());
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
                    producer: producer.to_owned(),
                    intent,
                    mover_token: mover_token.to_owned(),
                    start_key,
                    start_tag,
                    member_key,
                    output: output.to_owned(),
                    workspace: workspace.to_owned(),
                    drop_prior,
                    dest,
                    source_rect,
                    hide_gen: pair.0,
                    hide_safety: pair.1,
                    deadline: Instant::now() + FINISH_WAIT,
                });
                false
            }
        })
        .unwrap_or(false)
}

/// Service one proven-hide cross-output pending drop: re-resolve the pointer
/// destination, re-verify the END-time fences against fresh state, then run
/// the shared Engine route. No pre-drop focus: the confirm tail
/// follow-focuses on the destination after the transfer, so a cross drop
/// never foregrounds the source first. Every refusal converges without
/// Engine, focus, or geometry writes.
#[allow(clippy::too_many_arguments)]
fn service_cross_pending_drop(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    current: &ObservedWindow,
    member_key: &crate::workspace::WindowKey,
    loc: &crate::workspace::MemberLoc,
    pending: &PendingDrop,
    dest: &PendingCrossDest,
    x: i32,
    y: i32,
) {
    let log_path = state.log_path.clone();
    let from = current.token.clone();
    let tick = state.tick;
    let log_cross = |outcome: &'static str| {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "gesture",
                "tick": tick,
                "op": "drag-drop",
                "disposition": "refused",
                "outcome": outcome,
                "window": from,
                "producer": pending.producer,
                "cross_output": true,
            }),
        );
    };
    // The pointer output must still resolve to the queued destination:
    // topology moved under the pending finish otherwise.
    if output_for_point(areas, x, y).as_deref() != Some(dest.output.as_str()) {
        log_cross("topology-changed");
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return;
    }
    let membership_ok = state
        .workspaces
        .member_loc(member_key)
        .is_some_and(|fresh| fresh.output == loc.output && fresh.workspace == loc.workspace);
    let (live_max, live_full_opt) = live_overlay_flags(member_key.hwnd, fulls);
    let retained_full = retained
        .iter()
        .find(|r| r.key == *member_key)
        .map(|r| r.fullscreen)
        .unwrap_or(false);
    let live = drag_drop_live(
        state,
        areas,
        &dest.fence,
        member_key,
        membership_ok,
        live_max,
        live_full_opt.unwrap_or(retained_full),
    );
    if let Some(outcome) = crate::workspace_owner::drag_drop_fences_hold(&dest.fence, &live) {
        if outcome == "identity-changed" {
            drop_member_state(state, member_key);
        }
        log_cross(outcome);
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return;
    }
    // Fresh suspension fence after the proven hide, before any Engine or
    // project mutation: a fullscreen arrival (or elevated foreground)
    // between END and the ack refuses with no plan and no writes.
    if state.suspended || suspend_read(state, me, fulls).veto.block || foreground_elevated(me) {
        log_cross(if state.suspended {
            "suspended"
        } else {
            "elevated-foreground"
        });
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return;
    }
    state.tick += 1;
    let tick = state.tick;
    let correlation = state.correlation();
    let settle = CrossDragSettle {
        mover_token: current.token.clone(),
        member_key: member_key.clone(),
        source_output: loc.output.clone(),
        source_workspace: loc.workspace.clone(),
        dest_output: dest.output.clone(),
        dest_workspace: dest.workspace.clone(),
        x,
        y,
        drop_prior: pending.drop_prior.clone(),
        producer: pending.producer.clone(),
        start_rect: pending.source_rect,
    };
    settle_cross_drag_drop(
        state,
        me,
        fulls,
        areas,
        observed,
        retained,
        &settle,
        &dest.fence,
        tick,
        &correlation,
        correlation.as_str(),
    );
}

/// Service the bounded pending gesture drop from render-ack state at loop
/// level (ack-only and full turns alike): no blocking wait, so the owner
/// stays responsive to stop and new events. On a proven `Hidden`
/// acknowledgement the settle re-enters below with fresh observations and
/// gates before every focus/Engine/foreign write. On timeout the drop is
/// refused narrowly with no drop and no reconcile: actuation runs only over
/// a proven hide, while the posted safety epoch already bars any stale-show
/// replay. Cancel/Esc/suspend/new-gesture/stop invalidate the pending slot
/// with no Engine/focus/geometry, so a late ack can never actuate after the
/// gesture was cancelled, suspended, superseded, or stopped.
pub(super) fn service_pending_drop(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    let Some(bridge) = state.render_bridge.as_ref() else {
        return;
    };
    let pending = match bridge.pending.as_ref() {
        Some(pending) => pending,
        None => return,
    };
    let (hwnd, hide_gen, hide_safety, deadline) = (
        pending.hwnd,
        pending.hide_gen,
        pending.hide_safety,
        pending.deadline,
    );
    // Invalidation while waiting (unproven or proven): stop, Esc/cancel,
    // suspend, or a new gesture for the same HWND drops the slot with no
    // Engine/focus/geometry. Checked before the timeout so a cancelled
    // pending never waits out the bound.
    if let Some((outcome, kind)) = pending_invalidation(state, me, fulls, hwnd) {
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
        // Esc invalidation converges like the inline Esc path (one bounded
        // reconcile to snap back); stop/suspend/supersede drop without
        // converging, since the loop is stopping/suspended or the new
        // gesture owns the settle.
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
            reconcile_tick(state, me, &fulls_owned, areas);
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
            // Timeout: refuse narrowly. No drop, no reconcile: the drop and
            // its convergence run only over a proven hide, and the safety
            // epoch already bars replay. The renderer hides on its next
            // quantum either way.
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
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "op": "drag-drop",
                    "disposition": "refused",
                    "outcome": "drag-drop-refused",
                    "kind": "preview-finish-unacked",
                    "window": mover_token,
                    "producer": "settle",
                }),
            );
        }
        return;
    }
    let Some(pending) = state
        .render_bridge
        .as_mut()
        .and_then(|bridge| bridge.pending.take())
    else {
        return;
    };
    state.gesture_start_key.remove(&pending.hwnd);
    state.gesture_start_tag.remove(&pending.hwnd);
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
    publish_managed(state, me, &observed, &retained);
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    // Fresh revalidation against the stashed START snapshots before any
    // write: a reuse between END and the proven hide fails closed here
    // exactly like the inline settle path.
    let Some(current) = by_hwnd.get(&pending.hwnd) else {
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return;
    };
    if let (Some(key), Some(stored)) = (pending.start_key.as_ref(), pending.start_tag.as_ref())
        && (!crate::workspace_owner::member_matches(
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
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return;
    }
    let member_key = state
        .member_tokens
        .iter()
        .find(|(_, token)| token.as_str() == current.token.as_str())
        .map(|(key, _)| key.clone());
    let Some(member_key) = member_key else {
        return;
    };
    // The stashed END-time membership must still name the same member:
    // a token remap between END and the proven hide fails closed instead
    // of dropping under a recycled identity.
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
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return;
    }
    if !crate::workspace_owner::member_matches(
        &member_key,
        current.hwnd,
        current.identity.pid,
        &current.identity.process_creation,
    ) {
        return;
    }
    if state.sticky.contains_key(&member_key) {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "gesture",
                "tick": state.tick,
                "op": "gesture",
                "disposition": "observed",
                "outcome": "gesture-refused-sticky",
                "window": current.token,
            }),
        );
        return;
    }
    // Distinguish the Engine exception lane from a floating workspace mode,
    // matching the inline settle path above.
    if let Some(loc) = state.workspaces.member_loc(&member_key).cloned() {
        let engine_float = engine_is_float(state, &loc.output, &loc.workspace, &current.token);
        let tiled = workspace_mode_tiled(state, &loc.output, &loc.workspace);
        if engine_float || !tiled {
            let cause = gesture_float_cause(engine_float, tiled);
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "op": "gesture",
                    "disposition": "observed",
                    "outcome": "gesture-refused-floating",
                    "cause": cause,
                    "window": current.token,
                }),
            );
            return;
        }
    }
    let Some(loc) = state.workspaces.member_loc(&member_key).cloned() else {
        return;
    };
    if loc.output != pending.output || loc.workspace != pending.workspace {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "gesture",
                "tick": state.tick,
                "op": "gesture",
                "disposition": "refused",
                "outcome": "gesture-refused-domain",
                "window": current.token,
                "producer": pending.producer,
            }),
        );
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return;
    }
    let Some((domain, domain_key)) = workspace_domain_for(
        &loc.output,
        &loc.workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    ) else {
        return;
    };
    if let GestureIntent::MoveDrop { x, y } = pending.intent {
        match &pending.dest {
            None => {
                if !drop_point_in_domain(
                    domain.bounds.x,
                    domain.bounds.y,
                    domain.bounds.w,
                    domain.bounds.h,
                    x,
                    y,
                ) {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "gesture",
                            "tick": state.tick,
                            "op": "drag-drop",
                            "disposition": "refused",
                            "outcome": "drag-drop-refused",
                            "kind": "outside-work-area",
                            "window": current.token,
                            "producer": pending.producer,
                        }),
                    );
                    let fulls_owned = fulls.to_vec();
                    reconcile_tick(state, me, &fulls_owned, areas);
                    return;
                }
            }
            Some(dest) => {
                // Cross-output pending settle: the hide already proved, so
                // revalidate everything fresh (destination resolution,
                // fences, suspend/elevated) then run the shared Engine
                // route. No pre-drop focus here: the confirm tail
                // follow-focuses on the destination after the transfer.
                service_cross_pending_drop(
                    state,
                    me,
                    fulls,
                    areas,
                    &observed,
                    &retained,
                    current,
                    &member_key,
                    &loc,
                    &pending,
                    dest,
                    x,
                    y,
                );
                return;
            }
        }
    }
    if pending.producer == "windrag" {
        // Fresh suspension fence after the proven hide, before any focus
        // write: a fullscreen arrival (or elevated foreground) between END
        // and the ack refuses with no focus and no Engine plan. The late
        // fence inside `actuate_focus` stays the second guard for arrivals
        // racing this re-entry.
        if state.suspended || suspend_read(state, me, fulls).veto.block || foreground_elevated(me) {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "op": "gesture",
                    "disposition": "refused",
                    "outcome": "gesture-refused-focus",
                    "window": current.token,
                    "producer": pending.producer,
                }),
            );
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, &fulls_owned, areas);
            return;
        }
        let actuation = actuate_focus(state, me, fulls, &observed, &retained, &current.token);
        if actuation.outcome != "focus-ok" {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "op": "gesture",
                    "disposition": "refused",
                    "outcome": "gesture-refused-focus",
                    "window": current.token,
                    "producer": pending.producer,
                }),
            );
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, &fulls_owned, areas);
            return;
        }
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "windrag-focus",
                "tick": state.tick,
                "phase": "drop",
                "outcome": "focus-ok",
                "window": current.token,
                "producer": pending.producer,
            }),
        );
    }
    state.tick += 1;
    let correlation = state.correlation();
    let mut hint_cx = HintCx::new();
    let Some(rows) = assemble_domain_rows(
        state,
        &loc.output,
        &loc.workspace,
        &observed,
        &retained,
        "gesture",
        correlation.as_str(),
        &mut hint_cx,
    ) else {
        return;
    };
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
    let mover = WindowId(current.token.clone());
    {
        let revision = revision_for(state, &loc.output, &loc.workspace);
        let outer_gap = state.outer_gap;
        adopt_gaps_for_route(
            state,
            &domain,
            &domain_key,
            outer_gap,
            &windows,
            Some(&mover),
            revision,
            fp,
            &correlation,
        );
    }
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &correlation,
        revision_for(state, &loc.output, &loc.workspace),
        fp,
        &domain,
        &domain_key,
        state.outer_gap,
        &windows,
        Some(&mover),
    );
    let drop_prior_desc = preview_prior_desc(&pending.drop_prior);
    let reply = match pending.intent {
        GestureIntent::MoveDrop { x, y } => {
            event.command = CoreCommand::DragDrop {
                window: current.token.clone(),
                x,
                y,
                hover_prior: pending.drop_prior,
                source: None,
            };
            handle_engine_event(state, &mut event)
        }
        GestureIntent::PointerResize {
            direction,
            boundary,
            direction2,
            boundary2,
        } => {
            event.command = CoreCommand::PointerResize {
                window: current.token.clone(),
                direction: direction.to_owned(),
                boundary,
                direction2: direction2.map(str::to_owned),
                boundary2,
            };
            handle_engine_event(state, &mut event)
        }
    };
    let op = match pending.intent {
        GestureIntent::MoveDrop { .. } => "drag-drop",
        GestureIntent::PointerResize { .. } => "pointer-resize",
    };
    match &reply {
        CoreReply::Tiled(_) | CoreReply::Projection(_) | CoreReply::Resize(_) => {
            let tick = state.tick;
            let fulls_owned = fulls.to_vec();
            let writable = writable_tokens(state, &loc.output, &loc.workspace, &observed);
            apply_geometry(
                state,
                ApplyInput {
                    me,
                    fulls: &fulls_owned,
                    reply: &reply,
                    observed: &observed,
                    op,
                    tick,
                    correlation: correlation.as_str(),
                    skipped: Vec::new(),
                    writable: &writable,
                    output_token: state.workspaces.output_token(&loc.output),
                    workspace_token: state
                        .workspaces
                        .workspace_token(&loc.output, &loc.workspace),
                    revision: revision_for(state, &loc.output, &loc.workspace),
                },
            );
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "correlation": correlation.as_str(),
                    "op": op,
                    "disposition": "applied",
                    "outcome": if op == "drag-drop" { "drag-drop-applied" } else { "pointer-resize-applied" },
                    "window": current.token,
                    "producer": pending.producer,
                    "hover_prior": drop_prior_desc,
                }),
            );
        }
        _ => {
            let kind = match &reply {
                CoreReply::Rejected { kind, .. } => *kind,
                CoreReply::Diverged(reason) => reason.as_str(),
                CoreReply::SnapshotInvalid { detail, .. } => *detail,
                _ => "unhandled-variant",
            };
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": state.tick,
                    "correlation": correlation.as_str(),
                    "op": op,
                    "disposition": "refused",
                    "outcome": "drag-drop-refused",
                    "kind": kind,
                    "window": current.token,
                    "producer": pending.producer,
                    "hover_prior": drop_prior_desc,
                }),
            );
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, &fulls_owned, areas);
        }
    }
}
