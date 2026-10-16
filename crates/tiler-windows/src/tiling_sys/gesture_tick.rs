use super::*;

pub(super) fn gesture_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
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
        // Settle producer travels with the gesture: `native` for the
        // title-bar modal loop, `windrag` for the project-driven Win+Left
        // stationary hold. Absent entries settle as native.
        let producer = state.gesture_producer.remove(hwnd).unwrap_or("native");
        // Per-gesture START snapshot and END cursor travel with the gesture:
        // remove them here so each END settles exactly once.
        let start_key = state.gesture_start_key.remove(hwnd);
        let start_tag = state.gesture_start_tag.remove(hwnd);
        let end_cursor = state.gesture_end_cursor.remove(hwnd);
        let start_cursor = state.windrag_start_cursor.remove(hwnd);
        // Modal move/size truth sampled once at START: read and clear here
        // so every END path (settle, cancel, refusal) leaves no stale arm.
        // The native branch below prefers it over rect comparison.
        let modal_kind = state.move_kind.remove(hwnd);
        state.windrag_bound.remove(hwnd);
        state.gesture_esc_seq.remove(hwnd);
        state.gesture_end_seq.remove(hwnd);
        // Explicit Esc cancellation first: a cancelled native move restores
        // its pre-gesture frame, so rect equality must never decide this
        // path. Snap back through one bounded reconcile with no Engine
        // mutation here at all; topology unchanged by construction.
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
            if let Some(token) = token {
                line["window"] = serde_json::Value::from(token);
            }
            log_json_at(&log_path, line);
            // Esc cancel clears the preview with the gesture (already
            // hidden at latch time; this covers latch-and-settle races).
            hide_preview(state, "esc-cancelled");
            state.preview_bound.remove(hwnd);
            state.preview_dead.remove(hwnd);
            state.gesture_preview_start.remove(hwnd);
            state.gesture_start_cursor.remove(hwnd);
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, &fulls_owned, areas);
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
                && crate::workspace_owner::member_matches(
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
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, &fulls_owned, areas);
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
                    "producer": producer,
                }),
            );
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, &fulls_owned, areas);
            continue;
        }
        // Release cursor carried from the MOVESIZEEND event callback, never
        // re-read later: an END without a captured cursor routes as no-change
        // for moves (cursor-less), while resizes classify from geometry
        // alone. The project Win+Left hold bypasses rect classification
        // entirely: its frames stay at source by design, so the tracked
        // POINTER journey (down point versus callback-bound release point)
        // decides, through the same Engine drop route below.
        let intent = if producer == "windrag" {
            let (Some(start), Some(end)) = (start_cursor, end_cursor) else {
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
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, &fulls_owned, areas);
                continue;
            };
            match settle_pointer_journey(start.0, start.1, end.0, end.1, false) {
                WinSettle::Cancelled | WinSettle::NoChange => {
                    // Esc reaches here only via the latch above; a zero
                    // pointer journey makes no plan and changes no geometry.
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
                    let fulls_owned = fulls.to_vec();
                    reconcile_tick(state, me, &fulls_owned, areas);
                    continue;
                }
                WinSettle::Drop { x, y } => GestureIntent::MoveDrop { x, y },
            }
        } else {
            let cursor = end_cursor;
            let Some(mut classified) = classify_gesture(&before, &current.visible, cursor) else {
                // Zero movement (or a cursor-less move): nothing to route.
                // Reconcile once so any native nudge is reasserted to the
                // Engine allocation; topology unchanged.
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
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, &fulls_owned, areas);
                continue;
            };
            // Modal truth over rect comparison (see
            // `tiling::prefer_modal_move`): a caption-started move loop
            // carries user move intent even through a system rescale.
            classified = crate::tiling::prefer_modal_move(
                classified,
                cursor,
                modal_kind.is_some_and(|kind| kind == MoveSizeKind::Move),
            );
            classified
        };
        // The gesture window's own (output, workspace) session owns this
        // settle: same per-workspace routing as directional chords.
        let member_key = state
            .member_tokens
            .iter()
            .find(|(_, token)| token.as_str() == current.token.as_str())
            .map(|(key, _)| key.clone());
        let Some(member_key) = member_key else {
            continue;
        };
        if !crate::workspace_owner::member_matches(
            &member_key,
            current.hwnd,
            current.identity.pid,
            &current.identity.process_creation,
        ) {
            continue;
        }
        // Floats never feed gesture intents to the Engine; native float
        // move/resize stays free. Sticky rides the same slotless float plus
        // an explicit gate so a pruned-domain sticky still refuses.
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
            continue;
        }
        // Distinguish the Engine exception lane (intentional float, born
        // hold, automatic fixed) from a floating workspace mode: the
        // incident row conflated both under one outcome.
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
                continue;
            }
        }
        let Some(loc) = state.workspaces.member_loc(&member_key).cloned() else {
            continue;
        };
        let Some((domain, domain_key)) = workspace_domain_for(
            &loc.output,
            &loc.workspace,
            areas,
            state.inner_gap,
            state.outer_gap,
        ) else {
            continue;
        };
        // Cross-output drops (REQ-MOU-03): the pointer output's current
        // view scopes the drop through the pair route (occupied) or the
        // singleton admission (empty). Same-output points keep the local
        // route below; unplaceable points refuse with snap-back and no
        // transfer. The release cursor is END-captured (never re-read), and
        // domain decisions use retained membership plus this point, never
        // the possibly-moved native frame monitor.
        let cross_dest = if let GestureIntent::MoveDrop { x, y } = intent {
            match resolve_drop_destination(state, areas, &loc.output, x, y) {
                DropDestination::SameOutput => {
                    // Same-output points still sit inside the inset work
                    // area: panel-strip releases refuse before any Engine
                    // call, exactly like the retired same-output fence.
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
                                "producer": producer,
                            }),
                        );
                        let fulls_owned = fulls.to_vec();
                        reconcile_tick(state, me, &fulls_owned, areas);
                        continue;
                    }
                    None
                }
                DropDestination::Cross { output, workspace } => Some((output, workspace, x, y)),
                DropDestination::Refused(kind) => {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "gesture",
                            "tick": state.tick,
                            "op": "drag-drop",
                            "disposition": "refused",
                            "outcome": "drag-drop-refused",
                            "kind": kind,
                            "window": current.token,
                            "producer": producer,
                        }),
                    );
                    let fulls_owned = fulls.to_vec();
                    reconcile_tick(state, me, &fulls_owned, areas);
                    continue;
                }
            }
        } else {
            None
        };
        // Cross-output finishes snapshot their fences now: the bridged path
        // stashes the snapshot with the pending slot, the inline path below
        // re-reads live against it immediately. Unreadable topology refuses
        // with snap-back before any Engine call or queued slot.
        let cross_fence = match &cross_dest {
            Some((output, workspace, _, _)) => {
                let (live_max, live_full_opt) = live_overlay_flags(member_key.hwnd, fulls);
                let retained_full = retained
                    .iter()
                    .find(|r| r.key == member_key)
                    .map(|r| r.fullscreen)
                    .unwrap_or(false);
                let probe = CrossDragSettle {
                    mover_token: current.token.clone(),
                    member_key: member_key.clone(),
                    source_output: loc.output.clone(),
                    source_workspace: loc.workspace.clone(),
                    dest_output: output.clone(),
                    dest_workspace: workspace.clone(),
                    x: 0,
                    y: 0,
                    drop_prior: None,
                    producer: producer.to_owned(),
                    // Fence probe only: no Engine event is built from it.
                    start_rect: None,
                };
                match snapshot_cross_drag(
                    state,
                    areas,
                    &probe,
                    live_max,
                    live_full_opt.unwrap_or(retained_full),
                ) {
                    Some(fence) => Some(fence),
                    None => {
                        log_json_at(
                            &log_path,
                            serde_json::json!({
                                "event": "gesture",
                                "tick": state.tick,
                                "op": "drag-drop",
                                "disposition": "refused",
                                "outcome": "drag-drop-refused",
                                "kind": "unknown-output",
                                "window": current.token,
                                "producer": producer,
                            }),
                        );
                        let fulls_owned = fulls.to_vec();
                        reconcile_tick(state, me, &fulls_owned, areas);
                        continue;
                    }
                }
            }
            None => None,
        };
        // Bridged finish: hide (the safety epoch bars any stale-show replay)
        // and stash the drop as a bounded pending owner operation serviced
        // from render acks at loop level. No blocking wait: the owner stays
        // responsive to stop and new events, and the drop runs only after
        // the proven `Hidden` acknowledgement, re-entered with fresh
        // observations and gates before every focus/Engine/foreign write
        // below. The same-thread path keeps its proven-inline ordering.
        if state.render_bridge.is_some() {
            let pending_dest = match (&cross_dest, &cross_fence) {
                (Some((output, workspace, _, _)), Some(fence)) => Some(PendingCrossDest {
                    output: output.clone(),
                    workspace: workspace.clone(),
                    fence: fence.clone(),
                }),
                _ => None,
            };
            let occupied = queue_pending_drop(
                state,
                hwnd,
                producer,
                intent,
                &current.token,
                start_key,
                start_tag,
                member_key,
                &loc.output,
                &loc.workspace,
                pending_dest,
                state.gesture_before.get(hwnd).copied(),
            );
            state.preview_bound.remove(hwnd);
            state.preview_dead.remove(hwnd);
            state.gesture_preview_start.remove(hwnd);
            state.gesture_start_cursor.remove(hwnd);
            state.gesture_end_cursor.remove(hwnd);
            state.windrag_start_cursor.remove(hwnd);
            state.gesture_before.remove(hwnd);
            if occupied {
                // A previous finish is still pending (overlapping END races):
                // refuse the newcomer narrowly with no drop and no
                // reconcile; the pending operation owns the settle.
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "op": match &intent {
                            GestureIntent::MoveDrop { .. } => "drag-drop",
                            GestureIntent::PointerResize { .. } => "pointer-resize",
                        },
                        "disposition": "refused",
                        "outcome": "drag-drop-refused",
                        "kind": "preview-finish-busy",
                        "window": current.token,
                        "producer": producer,
                    }),
                );
            }
            continue;
        }
        // Inline cross-output settle (same-thread path, no render bridge):
        // fresh suspend/elevated gates, immediate fence re-read, proven
        // inline hide, then the shared Engine route. The pre-drop windrag
        // focus below stays same-output only: cross drops follow-focus on
        // the destination after the transfer instead of foregrounding the
        // source first.
        if let (Some((dest_output, dest_workspace, x, y)), Some(fence)) =
            (cross_dest.as_ref(), cross_fence.as_ref())
        {
            if state.suspended
                || suspend_read(state, me, fulls).veto.block
                || foreground_elevated(me)
            {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "op": "drag-drop",
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
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, &fulls_owned, areas);
                continue;
            }
            let membership_ok = state
                .workspaces
                .member_loc(&member_key)
                .is_some_and(|fresh| {
                    fresh.output == loc.output && fresh.workspace == loc.workspace
                });
            let (live_max, live_full_opt) = live_overlay_flags(member_key.hwnd, fulls);
            let retained_full = retained
                .iter()
                .find(|r| r.key == member_key)
                .map(|r| r.fullscreen)
                .unwrap_or(false);
            let live = drag_drop_live(
                state,
                areas,
                fence,
                &member_key,
                membership_ok,
                live_max,
                live_full_opt.unwrap_or(retained_full),
            );
            if let Some(outcome) = crate::workspace_owner::drag_drop_fences_hold(fence, &live) {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "op": "drag-drop",
                        "disposition": "refused",
                        "outcome": outcome,
                        "window": current.token,
                        "producer": producer,
                    }),
                );
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, &fulls_owned, areas);
                continue;
            }
            // Proven inline hide before any Engine or project mutation: the
            // drop never executes over a possibly visible preview.
            if !hide_preview_finish(state, "finish") {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "gesture",
                        "tick": state.tick,
                        "op": "drag-drop",
                        "disposition": "refused",
                        "outcome": "drag-drop-refused",
                        "kind": "preview-finish-unacked",
                        "window": current.token,
                        "producer": producer,
                    }),
                );
                state.preview_bound.remove(hwnd);
                state.preview_dead.remove(hwnd);
                state.gesture_preview_start.remove(hwnd);
                state.gesture_start_cursor.remove(hwnd);
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, &fulls_owned, areas);
                continue;
            }
            let drop_prior = state
                .preview_bound
                .get(hwnd)
                .and_then(|bound| bound.hover_prior.clone());
            state.preview_bound.remove(hwnd);
            state.preview_dead.remove(hwnd);
            state.gesture_preview_start.remove(hwnd);
            state.gesture_start_cursor.remove(hwnd);
            state.tick += 1;
            let tick = state.tick;
            let correlation = state.correlation();
            let settle = CrossDragSettle {
                mover_token: current.token.clone(),
                member_key: member_key.clone(),
                source_output: loc.output.clone(),
                source_workspace: loc.workspace.clone(),
                dest_output: dest_output.clone(),
                dest_workspace: dest_workspace.clone(),
                x: *x,
                y: *y,
                drop_prior,
                producer: producer.to_owned(),
                start_rect: state.gesture_before.get(hwnd).copied(),
            };
            settle_cross_drag_drop(
                state,
                me,
                fulls,
                areas,
                &observed,
                &retained,
                &settle,
                fence,
                tick,
                &correlation,
                correlation.as_str(),
            );
            continue;
        }
        // Project-driven Win+Left focus binding (KDE mover parity): the
        // gesture owns the mover even when native focus lags (unfocused
        // Win+Left starts without focusing). The Engine binds logical focus
        // to the mover itself, but the native foreground needs the existing
        // safe focus authority (E8-prime plus attach plus one setter with an
        // exact readback; never the underlay `focused_window` value). Every
        // gate above (START identity/lifetime fence, member match,
        // sticky/float refusal, domain routing, same-output fence) passed
        // before this effect. A failed actuation refuses with snap-back and
        // no plan instead of dropping under a diverged foreground.
        if producer == "windrag" {
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
                        "producer": producer,
                    }),
                );
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, &fulls_owned, areas);
                continue;
            }
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "windrag-focus",
                    "tick": state.tick,
                    "phase": "drop",
                    "outcome": "focus-ok",
                    "window": current.token,
                    "producer": producer,
                }),
            );
        }
        state.tick += 1;
        let correlation = state.correlation();
        // Incomplete snapshot defers with retained Engine state.
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
            continue;
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
        // Live gap edits adopt here so the drop convergence below sees
        // matching gaps instead of refusing the gesture.
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
        let event = crate::tiling::build_reconcile_event_for_floating(
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
        // Single-domain observations run the local retained
        // propose/commit path; Core owns gesture semantics.
        // The preview track carried the exact sticky group-edge hover prior
        // across samples through the same resolver; the final drop forwards
        // that exact value (or `None` when no preview resolved, e.g. an
        // unmoved-then-dropped pointer with no samples). Preview/drop
        // agreement is by construction: same observation assembly, same
        // resolver, same carried prior.
        let drop_prior = state
            .preview_bound
            .get(hwnd)
            .and_then(|bound| bound.hover_prior.clone());
        // Closed bounded trace descriptor (`none` or `group-edge:Side`):
        // proves preview-to-drop carry equality without raw NodeIds.
        let drop_prior_desc = preview_prior_desc(&drop_prior);
        // Reply rendering must never outlive the gesture: the preview is
        // proven hidden before the Engine drop runs, so no late sample can
        // paint over the settled layout. With a render bridge this hides
        // with a verified `Hidden` acknowledgement the owner waits for
        // (bounded, outside callbacks) before the Engine drop below. An
        // unacknowledged hide refuses the drop narrowly (tentative stage 2
        // semantics: `drag-drop-refused` with kind `preview-finish-unacked`,
        // converging through one bounded reconcile like any other refusal):
        // the drop never executes over a possibly visible preview, and the
        // bumped safety epoch still bars any stale-show replay.
        if !hide_preview_finish(state, "finish") {
            let op = match &intent {
                GestureIntent::MoveDrop { .. } => "drag-drop",
                GestureIntent::PointerResize { .. } => "pointer-resize",
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
                    "kind": "preview-finish-unacked",
                    "window": current.token,
                    "producer": producer,
                    "hover_prior": drop_prior_desc,
                }),
            );
            state.preview_bound.remove(hwnd);
            state.preview_dead.remove(hwnd);
            state.gesture_preview_start.remove(hwnd);
            state.gesture_start_cursor.remove(hwnd);
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, &fulls_owned, areas);
            continue;
        }
        state.preview_bound.remove(hwnd);
        state.preview_dead.remove(hwnd);
        state.gesture_preview_start.remove(hwnd);
        state.gesture_start_cursor.remove(hwnd);
        let reply = match intent {
            GestureIntent::MoveDrop { x, y } => {
                let mut event = event;
                event.command = CoreCommand::DragDrop {
                    window: current.token.clone(),
                    x,
                    y,
                    hover_prior: drop_prior,
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
                let mut event = event;
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
        let op = match intent {
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
                        "producer": producer,
                        "hover_prior": drop_prior_desc,
                    }),
                );
            }
            _ => {
                // Refusal converges through one bounded reconcile (snap-back
                // to the retained allocation), matching the KDE
                // restore-marker outcome with no ledger. Self, centre-stack,
                // and unsupported targets land here with no plan and no
                // commit; the closed Engine kind rides along for attribution.
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
                        "producer": producer,
                        "hover_prior": drop_prior_desc,
                    }),
                );
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, &fulls_owned, areas);
            }
        }
    }
    state.gesture_before.clear();
    state.esc_latched.clear();
    state.gesture_esc_seq.clear();
    state.gesture_end_seq.clear();
    state.gesture_end_cursor.clear();
    // A pending bridged finish owns its START snapshots until the loop-level
    // service consumes them: retain exactly that HWND's entries here.
    let pending_hwnd = state
        .render_bridge
        .as_ref()
        .and_then(|bridge| bridge.pending.as_ref())
        .map(|pending| pending.hwnd);
    match pending_hwnd {
        Some(hwnd) => {
            state.gesture_start_key.retain(|key, _| *key == hwnd);
            state.gesture_start_tag.retain(|key, _| *key == hwnd);
        }
        None => {
            state.gesture_start_key.clear();
            state.gesture_start_tag.clear();
        }
    }
    state.gesture_producer.clear();
    state.windrag_start_cursor.clear();
    state.windrag_bound.clear();
    state.preview_bound.clear();
    state.preview_dead.clear();
    state.gesture_preview_start.clear();
    state.gesture_start_cursor.clear();
    // No gesture remains open: no preview rectangle survives the settle.
    hide_preview(state, "finish");
}
