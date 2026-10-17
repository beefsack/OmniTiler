#![forbid(unsafe_code)]
use super::*;

/// Move one migrating float window across outputs (move-only, never a resize
/// or refit): `SetWindowPos` with `SWP_NOSIZE | SWP_NOZORDER |
/// SWP_NOACTIVATE` at the translated frame. No focus, no z-order, no show
/// effects. Under test the move records into the injected float-move log and
/// the ALL-member native-transfer log and reports success without touching
/// native state.
pub(super) fn move_float_to_output(hwnd_u64: u64, moved: Rect) -> bool {
    #[cfg(test)]
    {
        INJECTED_FLOAT_MOVES.with(|slot| slot.borrow_mut().push((hwnd_u64, moved)));
        INJECTED_NATIVE_TRANSFERS.with(|slot| slot.borrow_mut().push((hwnd_u64, moved, "float")));
        INJECTED_FLOAT_MOVE_OK.with(|slot| slot.get())
    }
    #[cfg(not(test))]
    {
        if !crate::ffi::is_window(hwnd_u64) {
            return false;
        }
        crate::ffi::set_window_pos_after(
            hwnd_u64,
            crate::ffi::PositionAfter::NotTopmost,
            moved.x,
            moved.y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        )
    }
}

/// Move one migrating overlay window across outputs (D8, move-only, never a
/// resize, refit, restore or remaximize): a visible fullscreen member moves
/// with `SetWindowPos` `SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE` at the
/// translated frame, preserving the native fullscreen frame with no extra
/// geometry or focus writes. A maximized member remaps its live
/// `WINDOWPLACEMENT` normal (restore) rect translated to the target,
/// preserving the original restore size and `showCmd` (still maximized,
/// still holding its reserved tile slot), never restoring first and never
/// writing the observed maximized frame as the restore frame. Under test the
/// maximized arm translates the injected normal boundary with the same pure
/// helper and records the target screen rect as `(hwnd, moved, "overlay")`.
#[allow(clippy::too_many_arguments)]
pub(super) fn move_overlay_to_output(
    hwnd_u64: u64,
    moved: Rect,
    maximized: bool,
    source_full: Rect,
    target_full: Rect,
    source_work: Rect,
    target_work: Rect,
    workspace_coords: bool,
) -> bool {
    #[cfg(test)]
    {
        if maximized {
            let Some((source_raw, _)) = live_placement_normal(hwnd_u64) else {
                return false;
            };
            let target_raw = translate_placement_normal_to_target(
                source_raw,
                source_work,
                target_work,
                source_full,
                target_full,
                workspace_coords,
            );
            let target_screen =
                placement_raw_to_screen(target_raw, target_work, target_full, workspace_coords);
            INJECTED_NATIVE_TRANSFERS
                .with(|slot| slot.borrow_mut().push((hwnd_u64, target_screen, "overlay")));
            return INJECTED_NATIVE_MOVE_OK.with(|slot| slot.get());
        }
        INJECTED_NATIVE_TRANSFERS.with(|slot| slot.borrow_mut().push((hwnd_u64, moved, "overlay")));
        INJECTED_NATIVE_MOVE_OK.with(|slot| slot.get())
    }
    #[cfg(not(test))]
    {
        if maximized {
            return remap_maximized_normal_to_output(
                hwnd_u64,
                source_full,
                target_full,
                source_work,
                target_work,
                workspace_coords,
            );
        }
        if !crate::ffi::is_window(hwnd_u64) {
            return false;
        }
        crate::ffi::set_window_pos_after(
            hwnd_u64,
            crate::ffi::PositionAfter::NotTopmost,
            moved.x,
            moved.y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        )
    }
}

/// Remap one concealed member's `WINDOWPLACEMENT` normal rect across outputs
/// (move-only, never a show-state change): reads the live placement to
/// preserve the original restore size and `showCmd`, converts the target
/// screen frame (`moved`, the FULL-delta translation of the retained normal
/// frame) to the raw `rcNormalPosition` basis with the target monitor's
/// `WORK` plus `FULL` inset delta (workspace coordinates for non-tool
/// top-level windows per the `WINDOWPLACEMENT` contract, screen identity for
/// tool windows), posts async (plus `WPF_ASYNCWINDOWPLACEMENT`, never blocking
/// on a hung owner), so a minimized member stays minimized and a hidden member
/// stays hidden. `kind` is `minimized` or `hidden` and is recorded verbatim in
/// the test seam. Under test records the screen `moved` rect and reports the
/// injected verdict without touching native state.
pub(super) fn remap_placement_to_output(
    hwnd_u64: u64,
    moved: Rect,
    kind: &'static str,
    target_work: Rect,
    target_full: Rect,
    workspace_coords: bool,
) -> bool {
    #[cfg(test)]
    {
        let _ = (target_work, target_full, workspace_coords);
        INJECTED_NATIVE_TRANSFERS.with(|slot| slot.borrow_mut().push((hwnd_u64, moved, kind)));
        INJECTED_NATIVE_MOVE_OK.with(|slot| slot.get())
    }
    #[cfg(not(test))]
    {
        let _ = kind;
        remap_placement_normal_rect(hwnd_u64, moved, target_work, target_full, workspace_coords)
    }
}

/// Production placement remap for minimized/hidden members: `GetWindowPlacement`,
/// keep the original normal restore size and `showCmd`, write the target
/// screen origin converted to the raw basis, post async. Returns `false` on
/// any failure (never a partial show-state change).
#[cfg(not(test))]
fn remap_placement_normal_rect(
    hwnd_u64: u64,
    moved_screen: Rect,
    target_work: Rect,
    target_full: Rect,
    workspace_coords: bool,
) -> bool {
    if !crate::ffi::is_window(hwnd_u64) {
        return false;
    }
    let Some(mut placement) = crate::ffi::window_placement(hwnd_u64) else {
        return false;
    };
    let orig_w = placement
        .rcNormalPosition
        .right
        .saturating_sub(placement.rcNormalPosition.left);
    let orig_h = placement
        .rcNormalPosition
        .bottom
        .saturating_sub(placement.rcNormalPosition.top);
    if orig_w <= 0 || orig_h <= 0 {
        return false;
    }
    let sized = Rect {
        x: moved_screen.x,
        y: moved_screen.y,
        w: orig_w,
        h: orig_h,
    };
    let raw = screen_to_placement_raw(sized, target_work, target_full, workspace_coords);
    placement.rcNormalPosition.left = raw.x;
    placement.rcNormalPosition.top = raw.y;
    placement.rcNormalPosition.right = raw.x.saturating_add(raw.w);
    placement.rcNormalPosition.bottom = raw.y.saturating_add(raw.h);
    placement.flags |= WPF_ASYNCWINDOWPLACEMENT;
    crate::ffi::set_window_placement(hwnd_u64, &placement)
}

/// Production maximized-overlay normal remap: single live `GetWindowPlacement`
/// snapshot supplies both the normal (restore) rect and the preserved
/// `showCmd`/`flags`, translated to the target with the source/target `FULL`
/// delta plus the per-monitor inset-delta conversion. Preserves the original
/// restore size and `showCmd`, posts async. Never writes the observed
/// maximized frame as the restore frame, never invents a retained fallback,
/// never changes show-state. Returns `false` on any failure.
#[cfg(not(test))]
fn remap_maximized_normal_to_output(
    hwnd_u64: u64,
    source_full: Rect,
    target_full: Rect,
    source_work: Rect,
    target_work: Rect,
    workspace_coords: bool,
) -> bool {
    // Single causal snapshot: one `GetWindowPlacement` read supplies geometry
    // plus `showCmd`/`flags`, so a state change between two reads cannot mix
    // a stale restore frame with a fresh show-state (or vice versa). Basis
    // rides the same live style/parent read; no retained fallback, no
    // observed-maximized-frame reuse.
    let _ = workspace_coords;
    if !crate::ffi::is_window(hwnd_u64) {
        return false;
    }
    let Some(mut placement) = crate::ffi::window_placement(hwnd_u64) else {
        return false;
    };
    let w = placement
        .rcNormalPosition
        .right
        .saturating_sub(placement.rcNormalPosition.left);
    let h = placement
        .rcNormalPosition
        .bottom
        .saturating_sub(placement.rcNormalPosition.top);
    if w <= 0 || h <= 0 {
        return false;
    }
    let source_raw = Rect {
        x: placement.rcNormalPosition.left,
        y: placement.rcNormalPosition.top,
        w,
        h,
    };
    let exstyle = crate::ffi::window_exstyle(hwnd_u64);
    let top_level = crate::ffi::parent_of(hwnd_u64) == 0;
    let basis = placement_is_workspace_coords(exstyle, top_level);
    let target_raw = translate_placement_normal_to_target(
        source_raw,
        source_work,
        target_work,
        source_full,
        target_full,
        basis,
    );
    placement.rcNormalPosition.left = target_raw.x;
    placement.rcNormalPosition.top = target_raw.y;
    placement.rcNormalPosition.right = target_raw.x.saturating_add(target_raw.w);
    placement.rcNormalPosition.bottom = target_raw.y.saturating_add(target_raw.h);
    placement.flags |= WPF_ASYNCWINDOWPLACEMENT;
    crate::ffi::set_window_placement(hwnd_u64, &placement)
}

/// Drain one bounded batch of workspace history intents (item 1). Toggle and
/// relative steps work on empty workspaces and unmanaged foreground: they
/// select views, never movers, so no managed-focus gate applies. The
/// `--no-keyboard-snap-takeover` off switch disables all product
/// interception (defensive second fence). Fullscreen and elevated foreground
/// gate all history ops like digits. Targets resolve without preactivating
/// (pure `resolve_previous`/`resolve_relative`: no activation, append, or
/// creation); only the verified `workspace_do_select` transition activates,
/// and its observation recording supplies history. Toggle with no recorded
/// change, or a removed/unassigned/out-of-scope previous, settles as
/// `unknown-target` (no-op until the next recorded change, never
/// recreation/ordinal reinterpretation).
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace_history_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    events: Vec<QueuedWorkspaceHistoryIntent>,
    blocked: Option<&'static str>,
) {
    let log_path = state.log_path.clone();
    if let Some(cause) = blocked {
        let stale = events.len() as u32;
        if stale > 0 {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-history-stale","cause": cause, "dropped": stale}),
            );
        }
        return;
    }
    if !state.keyboard.takeover {
        if !events.is_empty() {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-history-stale","cause":"takeover-off","dropped": events.len()}),
            );
        }
        return;
    }
    for intent in events {
        if !intent.consumed || !intent.announce {
            if state.trace {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "workspace-history",
                        "tick": state.tick,
                        "op": intent.op.as_str(),
                        "edge": intent.edge.as_str(),
                        "disposition": if intent.consumed { "consumed" } else { "passed" },
                        "outcome": "key-up",
                    }),
                );
            }
            continue;
        }
        let dispatch_start = Instant::now();
        if suspend_read(state, me, fulls).veto.block {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-history",
                    "op": intent.op.as_str(),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "suspended",
                }),
            );
            continue;
        }
        if foreground_elevated(me) {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-history",
                    "op": intent.op.as_str(),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "elevated-foreground",
                }),
            );
            continue;
        }
        let Some(output) = chord_output(state, areas) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-history",
                    "op": intent.op.as_str(),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "unknown-output",
                }),
            );
            continue;
        };
        state.workspaces.ensure_output(&output);
        state.tick += 1;
        let tick = state.tick;
        let mut skipped: Vec<(String, String)> = Vec::new();
        let mut retained: Vec<RetainedRow> = Vec::new();
        let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-history",
                    "tick": tick,
                    "op": intent.op.as_str(),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "observation-failed",
                }),
            );
            continue;
        };
        publish_managed(state, me, &observed, &retained);
        ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
        clear_maximize_at_admission(state, me, &retained);
        workspace_close_cleanup(state);
        let op = intent.op;
        let edge = intent.edge;
        let ctx = ActionCtx {
            correlation: format!("act-{tick}"),
            tick,
            queued_at: intent.tick,
            start: dispatch_start,
        };
        // Pure resolution: no activation, append, or creation. Previous
        // validates its stable id (clears removed/out-of-scope); relative
        // wraps the full scoped ring including trailing empty and >9.
        // Scoped resolvers keep local equivalent; `workspace_do_select`
        // dispatches shared/global-swap native effects.
        let target: Option<String> = match op {
            WorkspaceHistoryOp::Previous => {
                let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
                owner.resolve_previous(spaces, &output)
            }
            WorkspaceHistoryOp::Prev => {
                state
                    .scope_owner
                    .resolve_relative(&state.workspaces, &output, -1)
            }
            WorkspaceHistoryOp::Next => {
                state
                    .scope_owner
                    .resolve_relative(&state.workspaces, &output, 1)
            }
        };
        match target {
            Some(id) => {
                // REQ-WS-09 departure memory, same as the numbered select.
                let foreground_hwnd = crate::ffi::foreground_window();
                if let Some(source_id) = state.workspaces.active_id(&output)
                    && let Some(origin) = intent.origin.as_ref()
                    && let Some(token) = remember_select_departure(
                        state,
                        origin,
                        &output,
                        &source_id,
                        foreground_hwnd,
                        &observed,
                        &retained,
                    )
                {
                    log_json_at(
                        &state.log_path.clone(),
                        serde_json::json!({"event":"select-departure","correlation":ctx.correlation,"window":token}),
                    );
                }
                let effect = workspace_do_select(
                    state,
                    me,
                    store,
                    dir,
                    fulls,
                    areas,
                    &mut observed,
                    &output,
                    &id,
                    &ctx,
                    None,
                    false,
                    false,
                );
                log_json_at(
                    &log_path,
                    workspace_history_log(state, tick, op.as_str(), edge.as_str(), effect.outcome),
                );
                log_workspace_action(
                    state,
                    &log_path,
                    &ActionLine {
                        ctx: &ctx,
                        op: op.as_str(),
                        index: 0,
                        edge: edge.as_str(),
                        outcome: effect.outcome,
                        focus: effect.focus,
                        timings: &ActionTimings {
                            transition_ms: effect.transition_ms,
                            observation_ms: effect.observation_ms,
                            source_plan_ms: 0,
                            target_plan_ms: effect.plan_ms,
                            source_geometry_ms: 0,
                            target_geometry_ms: effect.geometry_ms,
                            hide_ms: effect.hide_ms,
                            reveal_ms: effect.reveal_ms,
                            focus_ms: effect.focus_ms,
                        },
                        source_workspace: &effect.source_workspace,
                        target_workspace: &effect.target_workspace,
                        source: None,
                        target: effect.geometry.as_ref(),
                    },
                );
            }
            None => {
                log_json_at(
                    &log_path,
                    workspace_history_log(
                        state,
                        tick,
                        op.as_str(),
                        edge.as_str(),
                        "unknown-target",
                    ),
                );
            }
        }
    }
}

/// Bounded history log line: opaque output/workspace tokens plus
/// op/edge/outcome only. No HWNDs, tokens, titles, or app data.
fn workspace_history_log(
    state: &TileLoop,
    tick: u64,
    op: &str,
    edge: &str,
    outcome: &str,
) -> serde_json::Value {
    let output = if state.active_output.is_empty() {
        "o?".to_owned()
    } else {
        state.workspaces.output_token(&state.active_output)
    };
    let workspace = state
        .workspaces
        .active_id(&state.active_output)
        .map(|id| state.workspaces.workspace_token(&state.active_output, &id))
        .unwrap_or_else(|| "ws?".to_owned());
    serde_json::json!({
        "event": "workspace-history",
        "tick": tick,
        "output": output,
        "workspace": workspace,
        "op": op,
        "edge": edge,
        "outcome": outcome,
    })
}

/// Bounded workspace log line: opaque output/workspace tokens plus
/// op/index/edge/outcome only. No HWNDs, tokens, titles, or app data.
pub(super) fn workspace_log(
    state: &TileLoop,
    tick: u64,
    op: &str,
    index: u8,
    edge: &str,
    outcome: &str,
) -> serde_json::Value {
    let output = if state.active_output.is_empty() {
        "o?".to_owned()
    } else {
        state.workspaces.output_token(&state.active_output)
    };
    let workspace = state
        .workspaces
        .active_id(&state.active_output)
        .map(|id| state.workspaces.workspace_token(&state.active_output, &id))
        .unwrap_or_else(|| "ws?".to_owned());
    serde_json::json!({
        "event": "workspace",
        "tick": tick,
        "output": output,
        "workspace": workspace,
        "op": op,
        "index": index,
        "edge": edge,
        "outcome": outcome,
    })
}

pub(super) fn log_workspace_action(_state: &TileLoop, log_path: &Path, line: &ActionLine<'_>) {
    let queue_wait_ms = line
        .ctx
        .start
        .duration_since(line.ctx.queued_at)
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    let action_ms = line
        .ctx
        .start
        .elapsed()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    let phase = |summary: Option<&ApplySummary>| match summary {
        Some(s) => {
            let mut v = serde_json::json!({
                "applied": s.applied,
                "mismatched": s.mismatched,
                "readback_ok": s.readback_ok,
            });
            match s.veto {
                Some(diag) => {
                    v["veto"] = serde_json::json!({
                        "reason": diag.reason,
                        "desktop": diag.desktop,
                        "visible": diag.visible,
                        "captioned": diag.captioned,
                        "dwm_readable": diag.dwm_readable,
                        "cloaked": diag.cloaked,
                    });
                }
                None => {
                    v["veto"] = serde_json::Value::Null;
                }
            }
            v
        }
        None => serde_json::Value::Null,
    };
    log_json_at(
        log_path,
        serde_json::json!({
            "event": "workspace-action",
            "tick": line.ctx.tick,
            "correlation": line.ctx.correlation,
            "op": line.op,
            "index": line.index,
            "edge": line.edge,
            "outcome": line.outcome,
            "focus": line.focus,
            "queue_wait_ms": queue_wait_ms,
            "action_ms": action_ms,
            "transition_ms": line.timings.transition_ms,
            "observation_ms": line.timings.observation_ms,
            "source_plan_ms": line.timings.source_plan_ms,
            "target_plan_ms": line.timings.target_plan_ms,
            "source_geometry_ms": line.timings.source_geometry_ms,
            "target_geometry_ms": line.timings.target_geometry_ms,
            "hide_ms": line.timings.hide_ms,
            "reveal_ms": line.timings.reveal_ms,
            "focus_ms": line.timings.focus_ms,
            "source_workspace": line.source_workspace,
            "target_workspace": line.target_workspace,
            "source": phase(line.source),
            "target": phase(line.target),
        }),
    );
}
