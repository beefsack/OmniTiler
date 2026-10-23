#![forbid(unsafe_code)]
use super::*;
use crate::snapkey::QueuedIntent;

/// Mechanical foreground verification for the directional snap chain: the
/// world apply already spent the single focus/follow effect, so this only
/// reads the foreground against the expected window (observed or retained
/// overlay) to decide the advance. Never actuates.
fn foreground_matches(
    state: &TileLoop,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    token: &str,
) -> bool {
    overlay_expected(state, observed, retained, token)
        .is_some_and(|expected| crate::ffi::foreground_window() == expected.hwnd)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch_directional_intent(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    intent: QueuedIntent,
    log_path: &Path,
) {
    if !intent.consumed || !intent.announce {
        // Key-ups close the pair and passed chords never dispatch:
        // trace-only, so held-key traffic stays out of normal logs.
        if state.trace {
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "snap",
                    "tick": state.tick,
                    "op": intent.op.as_str(),
                    "direction": direction_name(intent.direction),
                    "edge": intent.edge.as_str(),
                    "disposition": if intent.consumed { "consumed" } else { "passed" },
                    "outcome": if intent.consumed { "key-up" } else { "passed" },
                }),
            );
        }
        return;
    }
    // Fresh complete observation per intent. The chord-time origin
    // must re-resolve: stale intents die here, never on a cached
    // rectangle.
    state.tick += 1;
    let tick = state.tick;
    let correlation = state.correlation();
    let Some(origin) = intent.origin.clone() else {
        // Authentic interception swallows unmanaged/unadmitted/
        // shell/suspended/elevated chords with no origin; settle
        // defensively without dispatch (never acts on protected
        // windows). No origin token exists to echo here.
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "origin-vanished",
            }),
        );
        return;
    };
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        // Enumeration failure settles the action as failed; the
        // Engine keeps retained state and the next tick retries.
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "observation-failed",
                "origin": origin.token,
            }),
        );
        return;
    };
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    // First-seen maximized windows restore once here: lifetime tags are
    // stamped, and the cleared window converges as eligible next tick.
    clear_maximize_at_admission(state, me, &retained);
    let fresh: Vec<SnapOrigin> = state.snap_origins.values().cloned().collect();
    let foreground_hwnd = Some(crate::ffi::foreground_window());
    let (from, continued) = match resolve_origin(
        &origin,
        foreground_hwnd,
        &fresh,
        state.snap_advance.as_ref(),
    ) {
        OriginVerdict::Dispatch { token, continued } => (token, continued),
        OriginVerdict::Reject(reason) => {
            // External focus or lifetime mismatch invalidates the
            // chain; never carry a stale advance forward.
            state.snap_advance = None;
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "snap",
                    "tick": tick,
                    "correlation": correlation.as_str(),
                    "op": intent.op.as_str(),
                    "direction": direction_name(intent.direction),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": reason,
                    "origin": origin.token,
                }),
            );
            return;
        }
    };
    // Route through the origin window's own homed logical workspace.
    // Sticky/current-view routing, the ordered refusal lattice, float
    // candidates/targets, half-snap geometry, maximize-clear need, and
    // tiled focus/move/send are all world-owned now: the adapter names
    // the window and direction only, then mechanically actuates the
    // replied decision with fresh native proof.
    let Some(member_key) = state
        .member_tokens
        .iter()
        .find(|(_, token)| token.as_str() == from.as_str())
        .map(|(key, _)| key.clone())
    else {
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "unmanaged",
                "origin": origin.token,
            }),
        );
        return;
    };
    if !crate::native_guards::member_matches(&member_key, origin.hwnd, origin.pid, &origin.creation)
    {
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "origin-vanished",
                "origin": origin.token,
            }),
        );
        return;
    }
    let Some(home) = state.member_home(&member_key) else {
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "unmanaged",
                "origin": origin.token,
            }),
        );
        return;
    };
    if !areas.iter().any(|a| a.device == home.output) {
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "unknown-output",
                "origin": origin.token,
            }),
        );
        return;
    };
    if !state.member_tokens.values().any(|token| token == &from) {
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "unmanaged",
                "origin": origin.token,
            }),
        );
        return;
    }
    // Fresh veto re-check before dispatch: validation may have taken
    // time, and a vetoing foreground authorizes no dispatch.
    if suspend_read(state, me, fulls).veto.block {
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "suspended",
                "origin": origin.token,
                "window": from,
                "continued": continued,
            }),
        );
        return;
    }
    if foreground_elevated(me) {
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "snap",
                "tick": tick,
                "correlation": correlation.as_str(),
                "op": intent.op.as_str(),
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "elevated-foreground",
                "origin": origin.token,
                "window": from,
                "continued": continued,
            }),
        );
        return;
    }
    // Direction-only world dispatch for every origin, float included:
    // the core owns topology and the full refusal/float/maximize
    // lattice, so the adapter never branches on float state here. The
    // applied reply IS the outcome.
    let direction = direction_name(intent.direction).to_owned();
    let command = match intent.op {
        SnapOp::Focus => tiler_core::world_report::WorldCommand::FocusDirectional {
            window: tiler_core::directional::WindowId(from.clone()),
            direction: direction.clone(),
        },
        SnapOp::Move => tiler_core::world_report::WorldCommand::MoveDirectional {
            window: tiler_core::directional::WindowId(from.clone()),
            direction: direction.clone(),
            same_axis: same_axis_move_for(state.settings_live.as_ref()),
        },
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
        command.clone(),
        super::observation::sample_cursor(),
    );
    // Maximize-clear desires actuate inside the world apply above (the
    // uniform `clear_operation` lane dispatches the native clear at most
    // once and reports a correlated attempt); the commandless core
    // settlement resumes the original intent on proof. This seam never
    // clears or re-dispatches: a NeedClear reply only names the pending
    // desire for its snap chain.
    // World-owned directional outcomes. Refusals settle atomically with
    // no writes; float placements and focus changes actuate through the
    // single spent common effect inside the world apply above, and this
    // seam only verifies the foreground for its snap chain.
    if let Some(tiler_core::world_report::CommandResult::Directional {
        outcome,
        target,
        rect,
        cause,
    }) = reply
        .as_ref()
        .and_then(|reply| reply.command_result.clone())
    {
        use tiler_core::world_directional::DirectionalOutcome as O;
        match intent.op {
            SnapOp::Focus => match outcome {
                O::FocusRefusedFloating
                | O::FocusRefusedWorkspaceFloating
                | O::FocusRefusedMaximize
                | O::FocusFloatRetained => {
                    state.snap_advance = None;
                    let mut line = serde_json::json!({
                        "event": "snap",
                        "tick": tick,
                        "correlation": correlation.as_str(),
                        "op": intent.op.as_str(),
                        "direction": direction_name(intent.direction),
                        "edge": intent.edge.as_str(),
                        "disposition": "consumed",
                        "outcome": outcome.as_str(),
                        "origin": origin.token,
                        "window": from,
                        "continued": continued,
                    });
                    if outcome == O::FocusFloatRetained {
                        line["reason"] = serde_json::Value::from("no-target");
                    }
                    if let Some(cause) = cause {
                        line["cause"] = serde_json::Value::from(cause);
                    }
                    log_json_at(log_path, line);
                    return;
                }
                O::FocusFloatApplied => {
                    let Some(target) = target else {
                        state.snap_advance = None;
                        log_json_at(
                            log_path,
                            serde_json::json!({
                                "event": "snap",
                                "tick": tick,
                                "correlation": correlation.as_str(),
                                "op": intent.op.as_str(),
                                "direction": direction_name(intent.direction),
                                "edge": intent.edge.as_str(),
                                "disposition": "consumed",
                                "outcome": "focus-float-refused-stale",
                                "origin": origin.token,
                                "window": from,
                                "continued": continued,
                            }),
                        );
                        return;
                    };
                    // The world apply above already spent the single focus
                    // effect; this seam only verifies the foreground for its
                    // snap chain and never actuates twice.
                    if foreground_matches(state, &observed, &retained, &target.0) {
                        note_focus_advance(state, &observed, &retained, &target.0);
                        log_json_at(
                            log_path,
                            serde_json::json!({
                                "event": "snap",
                                "tick": tick,
                                "correlation": correlation.as_str(),
                                "op": intent.op.as_str(),
                                "direction": direction_name(intent.direction),
                                "edge": intent.edge.as_str(),
                                "disposition": "consumed",
                                "outcome": "focus-float-applied",
                                "origin": origin.token,
                                "window": from,
                                "continued": continued,
                            }),
                        );
                    } else {
                        state.snap_advance = None;
                        log_json_at(
                            log_path,
                            serde_json::json!({
                                "event": "snap",
                                "tick": tick,
                                "correlation": correlation.as_str(),
                                "op": intent.op.as_str(),
                                "direction": direction_name(intent.direction),
                                "edge": intent.edge.as_str(),
                                "disposition": "consumed",
                                "outcome": "focus-unverified",
                                "origin": origin.token,
                                "window": from,
                                "continued": continued,
                            }),
                        );
                    }
                    return;
                }
                O::FocusFloatRefusedStale => {
                    state.snap_advance = None;
                    let mut line = serde_json::json!({
                        "event": "snap",
                        "tick": tick,
                        "correlation": correlation.as_str(),
                        "op": intent.op.as_str(),
                        "direction": direction_name(intent.direction),
                        "edge": intent.edge.as_str(),
                        "disposition": "consumed",
                        "outcome": outcome.as_str(),
                        "origin": origin.token,
                        "window": from,
                        "continued": continued,
                    });
                    if let Some(cause) = cause {
                        line["cause"] = serde_json::Value::from(cause);
                    }
                    log_json_at(log_path, line);
                    return;
                }
                _ => {
                    state.snap_advance = None;
                    log_json_at(
                        log_path,
                        serde_json::json!({
                            "event": "snap",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "op": intent.op.as_str(),
                            "direction": direction_name(intent.direction),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": outcome.as_str(),
                            "origin": origin.token,
                            "window": from,
                            "continued": continued,
                        }),
                    );
                    return;
                }
            },
            SnapOp::Move => match outcome {
                O::MoveRefusedFloating
                | O::MoveRefusedWorkspaceFloating
                | O::MoveRefusedFullscreen
                | O::MoveFloatRefusedStale
                | O::MoveFloatRefusedInvalidGeometry
                | O::MoveFloatRefusedConstraints
                | O::MoveFloatRefusedUnknownOutput => {
                    state.snap_advance = None;
                    let mut line = serde_json::json!({
                        "event": "snap",
                        "tick": tick,
                        "correlation": correlation.as_str(),
                        "op": intent.op.as_str(),
                        "direction": direction_name(intent.direction),
                        "edge": intent.edge.as_str(),
                        "disposition": "consumed",
                        "outcome": outcome.as_str(),
                        "origin": origin.token,
                        "window": from,
                        "continued": continued,
                    });
                    if let Some(cause) = cause {
                        line["cause"] = serde_json::Value::from(cause);
                    }
                    log_json_at(log_path, line);
                    return;
                }
                O::NeedClear => {
                    // The world apply above may dispatch the one-shot native
                    // clear (at most once under the originating op); the
                    // commandless core settlement resumes the original
                    // intent on proof. This seam never claims dispatch: the
                    // reply carries the pending `clear_operation` desire, the
                    // actual raw attempt (accepted/refused/threw) rides the
                    // `world-apply` diag `clears` row, and terminal
                    // refusal/abandonment rides `operationSettled`. Requested /
                    // desired / attempted is not applied: only observed frame
                    // arrival proves the move, so this seam logs the pending
                    // desire honestly and never advances its snap chain on it.
                    state.snap_advance = None;
                    log_json_at(
                        log_path,
                        serde_json::json!({
                            "event": "snap",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "op": intent.op.as_str(),
                            "direction": direction_name(intent.direction),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": "move-maximize-clear",
                            "result": "pending",
                            "origin": origin.token,
                            "window": from,
                            "continued": continued,
                        }),
                    );
                    return;
                }
                O::MoveFloatApplied => {
                    // The world apply above dispatched the retained
                    // half-snap through the uniform float lane (at most once
                    // under the stable command identity); this seam only
                    // maintains its snap chain on the proven foreground and
                    // never writes a second time. Dispatched is not applied:
                    // only observed frame arrival proves the half.
                    if rect.is_none() {
                        state.snap_advance = None;
                        log_json_at(
                            log_path,
                            serde_json::json!({
                                "event": "snap",
                                "tick": tick,
                                "correlation": correlation.as_str(),
                                "op": intent.op.as_str(),
                                "direction": direction_name(intent.direction),
                                "edge": intent.edge.as_str(),
                                "disposition": "consumed",
                                "outcome": "move-float-refused-invalid-geometry",
                                "origin": origin.token,
                                "window": from,
                                "continued": continued,
                            }),
                        );
                        return;
                    }
                    if foreground_matches(state, &observed, &retained, &from) {
                        note_focus_advance(state, &observed, &retained, &from);
                    } else {
                        state.snap_advance = None;
                    }
                    log_json_at(
                        log_path,
                        serde_json::json!({
                            "event": "snap",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "op": intent.op.as_str(),
                            "direction": direction_name(intent.direction),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": "move-float-dispatched",
                            "core_outcome": "move-float-applied",
                            "origin": origin.token,
                            "window": from,
                            "continued": continued,
                        }),
                    );
                    return;
                }
                _ => {
                    state.snap_advance = None;
                    log_json_at(
                        log_path,
                        serde_json::json!({
                            "event": "snap",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "op": intent.op.as_str(),
                            "direction": direction_name(intent.direction),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": outcome.as_str(),
                            "origin": origin.token,
                            "window": from,
                            "continued": continued,
                        }),
                    );
                    return;
                }
            },
        }
    }
    // No world-owned directional result: the tiled world outcome rides
    // the snapshot, transfers, and the single spent focus effect from the
    // world apply above. This seam verifies arrival and the foreground
    // for its snap chain and never actuates twice.
    let outcome: &'static str = match intent.op {
        SnapOp::Focus => {
            let Some(target) = reply.as_ref().and_then(|reply| {
                reply
                    .effects
                    .iter()
                    .find(|effect| {
                        effect.kind == tiler_core::world_report::EffectKind::Focus
                            && effect.id.starts_with("fx-")
                    })
                    .map(|effect| effect.target.0.clone())
            }) else {
                // No explicit focus effect: the core planned no focus
                // change (local no-op or silent refusal).
                state.snap_advance = None;
                log_json_at(
                    log_path,
                    serde_json::json!({
                        "event": "snap",
                        "tick": tick,
                        "correlation": correlation.as_str(),
                        "op": intent.op.as_str(),
                        "direction": direction_name(intent.direction),
                        "edge": intent.edge.as_str(),
                        "disposition": "consumed",
                        "outcome": "unchanged",
                        "origin": origin.token,
                        "window": from,
                        "continued": continued,
                    }),
                );
                return;
            };
            // The world apply already spent the single focus effect; verify
            // the foreground for the snap chain, never actuate twice.
            if foreground_matches(state, &observed, &retained, &target) {
                note_focus_advance(state, &observed, &retained, &target);
                "focus-ok"
            } else {
                state.snap_advance = None;
                "focus-unverified"
            }
        }
        SnapOp::Move => {
            let Some(reply) = reply.as_ref() else {
                state.snap_advance = None;
                log_json_at(
                    log_path,
                    serde_json::json!({
                        "event": "snap",
                        "tick": tick,
                        "correlation": correlation.as_str(),
                        "op": intent.op.as_str(),
                        "direction": direction_name(intent.direction),
                        "edge": intent.edge.as_str(),
                        "disposition": "consumed",
                        "outcome": "move-unverified",
                        "origin": origin.token,
                        "window": from,
                        "continued": continued,
                    }),
                );
                return;
            };
            // Arrival proof: the mover's desired effective frame (or the
            // proven membership receipt for frameless lanes) reads back
            // live on its homed output. Allocation changes also separate
            // applied from no-op.
            let arrived =
                drop_arrival_verified(state, me, fulls, areas, reply, &from, member_key.hwnd, None);
            if !arrived {
                state.snap_advance = None;
                log_json_at(
                    log_path,
                    serde_json::json!({
                        "event": "snap",
                        "tick": tick,
                        "correlation": correlation.as_str(),
                        "op": intent.op.as_str(),
                        "direction": direction_name(intent.direction),
                        "edge": intent.edge.as_str(),
                        "disposition": "consumed",
                        "outcome": "move-unverified",
                        "origin": origin.token,
                        "window": from,
                        "continued": continued,
                    }),
                );
                return;
            }
            // Frameless-lane arrivals (proven through the membership
            // receipt) carry no frame target to compare: only framed rows
            // distinguish applied from no-op here.
            if let Some(target) = reply
                .windows
                .iter()
                .find(|w| w.lifetime.0 == from)
                .and_then(|row| row.effective_frame)
            {
                let changed = observed
                    .iter()
                    .find(|w| w.token == from)
                    .is_none_or(|w| w.visible != target);
                if !changed {
                    state.snap_advance = None;
                    log_json_at(
                        log_path,
                        serde_json::json!({
                            "event": "snap",
                            "tick": tick,
                            "correlation": correlation.as_str(),
                            "op": intent.op.as_str(),
                            "direction": direction_name(intent.direction),
                            "edge": intent.edge.as_str(),
                            "disposition": "consumed",
                            "outcome": "move-noop",
                            "origin": origin.token,
                            "window": from,
                            "continued": continued,
                        }),
                    );
                    return;
                }
            }
            // Follow focus rides the single spent core effect from the world
            // apply above (the core mints it on every accepted tiled move);
            // the arrival is already placed, so only verify the foreground
            // for the snap chain, never actuate twice.
            if foreground_matches(state, &observed, &retained, &from) {
                note_focus_advance(state, &observed, &retained, &from);
            } else {
                state.snap_advance = None;
            }
            "move-applied"
        }
    };
    log_json_at(
        log_path,
        serde_json::json!({
            "event": "snap",
            "tick": tick,
            "correlation": correlation.as_str(),
            "op": intent.op.as_str(),
            "direction": direction_name(intent.direction),
            "edge": intent.edge.as_str(),
            "disposition": "consumed",
            "outcome": outcome,
            "origin": origin.token,
            "window": from,
            "continued": continued,
        }),
    );
}
