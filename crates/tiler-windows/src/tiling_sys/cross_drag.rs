#![forbid(unsafe_code)]
use super::*;

/// Cross-output drag-drop settle (REQ-MOU-03): native title-bar releases
/// landing on another output join that output's CURRENT workspace at the
/// pointer point through the shared Engine drop resolver, and the source
/// membership is removed. The destination is the output under the pointer at
/// release (FULL-rectangle containment, never the directional adjacency
/// selector), so sideways, stacked, and negative-coordinate arrangements all
/// behave alike. The destination view is never hidden or switched: only
/// tiled geometry writes plus mover follow-focus run.
///
/// Producer split (REQ-DRAG-07): a native title-bar frame may physically move
/// before END, so domain decisions use retained project membership and the
/// pointer point, never the live frame monitor; the drop runs only after the
/// proven preview-hide acknowledgement with fresh revalidation.
pub(super) struct CrossDragSettle {
    pub(super) mover_token: String,
    pub(super) member_key: crate::native_guards::WindowKey,
    pub(super) source_output: String,
    pub(super) source_workspace: String,
    pub(super) dest_output: String,
    pub(super) dest_workspace: String,
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) drop_prior: Option<tiler_core::session::DragHoverPrior>,
    pub(super) producer: String,
}

/// Drop-destination resolution for one move-drop point. `SameOutput` keeps
/// the local route; `Cross` carries the pointer output plus its current
/// workspace; `Refused` carries the honest outcome for unplaceable points
/// (outside every output, unknown view, floating destination, unreadable
/// topology, or inside the monitor but outside the inset work area).
#[derive(Debug, PartialEq, Eq)]
pub(super) enum DropDestination {
    SameOutput,
    Cross { output: String, workspace: String },
    Refused(&'static str),
}

pub(super) fn resolve_drop_destination(
    state: &TileLoop,
    areas: &[MonitorArea],
    source_output: &str,
    x: i32,
    y: i32,
) -> DropDestination {
    let Some(dest) = output_for_point(areas, x, y) else {
        return DropDestination::Refused("outside-output");
    };
    if dest == source_output {
        return DropDestination::SameOutput;
    }
    let Some(workspace) = state.active_view(&dest) else {
        return DropDestination::Refused("no-view");
    };
    if !state.is_tiled_view(&dest, &workspace) {
        return DropDestination::Refused("floating-dest");
    }
    let dest_bounds = areas
        .iter()
        .find(|a| a.device == dest)
        .and_then(|area| tiling_domain_bounds_with(area.work, state.outer_gap));
    let Some(bounds) = dest_bounds else {
        return DropDestination::Refused("unknown-output");
    };
    if !crate::tiling::drop_point_in_domain(
        bounds.x,
        bounds.y,
        bounds.w,
        bounds.h,
        x,
        y,
    ) {
        return DropDestination::Refused("outside-work-area");
    }
    DropDestination::Cross {
        output: dest,
        workspace,
    }
}

/// Snapshot the drag-drop fences for a resolved cross settle, including the
/// live FULL rectangles, inset domain bounds, and owner binding so later
/// tails detect topology or binding drift. `None` on unreadable topology.
pub(super) fn snapshot_cross_drag(
    state: &TileLoop,
    areas: &[MonitorArea],
    settle: &CrossDragSettle,
    overlay_maximized: bool,
    overlay_fullscreen: bool,
) -> Option<crate::native_guards::DragDropFenceSnapshot> {
    let source_area = areas.iter().find(|a| a.device == settle.source_output)?;
    let target_area = areas.iter().find(|a| a.device == settle.dest_output)?;
    let source_bounds = tiling_domain_bounds_with(source_area.work, state.outer_gap)?;
    let target_bounds = tiling_domain_bounds_with(target_area.work, state.outer_gap)?;
    Some(crate::native_guards::DragDropFenceSnapshot {
        inner_gap: state.inner_gap,
        outer_gap: state.outer_gap,
        source_output: settle.source_output.clone(),
        source_workspace: settle.source_workspace.clone(),
        target_output: settle.dest_output.clone(),
        target_workspace: settle.dest_workspace.clone(),
        source_tiled: state.is_tiled_view(&settle.source_output, &settle.source_workspace),
        target_tiled: state.is_tiled_view(&settle.dest_output, &settle.dest_workspace),
        overlay_maximized,
        overlay_fullscreen,
        source_full: source_area.full,
        target_full: target_area.full,
        source_bounds,
        target_bounds,
        owner: state.owner.as_str().to_owned(),
        generation: state.generation.as_str().to_owned(),
    })
}

/// Fresh monitor topology for one cross-operation tail: re-enumerate outputs
/// at actuation time (test seams inject the list) so an output removal or
/// work-area change between pair-build and native writes refuses instead of
/// writing stale geometry. Exactly one enumeration per tail, never per
/// window; `None` on enumeration failure (fail-closed like the tick skip).
pub(super) fn fresh_cross_areas() -> Option<Vec<MonitorArea>> {
    #[cfg(test)]
    if let Some(areas) = INJECTED_AREAS.with(|slot| slot.borrow().clone()) {
        return Some(areas);
    }
    all_monitors().ok()
}

/// Verify fresh monitor topology still covers a fenced cross pair: both
/// snapshot outputs must resolve with identical FULL rectangles and
/// identical inset domain bounds (recomputed with the snapshot gaps, so gap
/// edits stay with the gap fence). Order-independent; unknown outputs,
/// changed rectangles, or uninsettable work areas all fail closed.
#[must_use]
pub(super) fn cross_topology_fresh(
    snapshot: &crate::native_guards::DragDropFenceSnapshot,
    fresh: &[MonitorArea],
) -> bool {
    for (output, full, bounds) in [
        (
            &snapshot.source_output,
            snapshot.source_full,
            snapshot.source_bounds,
        ),
        (
            &snapshot.target_output,
            snapshot.target_full,
            snapshot.target_bounds,
        ),
    ] {
        let Some(area) = fresh.iter().find(|a| &a.device == output) else {
            return false;
        };
        if area.full != full {
            return false;
        }
        if tiling_domain_bounds_with(area.work, snapshot.outer_gap)
            .is_none_or(|live| live != bounds)
        {
            return false;
        }
    }
    true
}

/// Route one fenced cross settle to its Engine shape and finalize: occupied
/// destinations run the pair transaction, empty ones run singleton admission
/// plus source reflow. Shared by the inline same-thread path and the
/// render-ack pending service (both arrive with fresh observations, proven
/// hide, and evaluated fences). Returns the outcome; every terminal logs
/// inside the finalize calls except silent `deferred` (incomplete rows keep
/// retained state exactly like the local route).
#[allow(clippy::too_many_arguments)]
pub(super) fn settle_cross_drag_drop(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    settle: &CrossDragSettle,
    fence: &crate::native_guards::DragDropFenceSnapshot,
    tick: u64,
    _correlation: &CorrelationId,
    correlation_str: &str,
) -> &'static str {
    // World-routed cross settle: the core owns transfer planning outright
    // (occupied pair and empty singleton alike), and the host diff applies
    // the accepted reply with fresh proof. Empty-vs-occupied needs no
    // adapter branch: the core homes the mover on the destination view in
    // both cases. An empty destination the core already homes members on
    // is never treated as empty for logging.
    // Fresh fences before dispatch: membership still homes on the
    // source, suspend/elevated vetoes refuse, and the pointer
    // destination must still resolve.
    let membership_ok = state.member_home(&settle.member_key).is_some_and(|fresh| {
        fresh.output == settle.source_output && fresh.workspace == settle.source_workspace
    });
    if !membership_ok
        || state.suspended
        || suspend_read(state, me, fulls).veto.block
        || foreground_elevated(me)
    {
        let outcome = if !membership_ok {
            "drag-drop-refused"
        } else {
            "move-unverified"
        };
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "gesture",
                "tick": tick,
                "correlation": correlation_str,
                "op": "drag-drop",
                "disposition": "refused",
                "outcome": outcome,
                "window": settle.mover_token,
                "producer": settle.producer,
                "cross_output": true,
            }),
        );
        let fulls_owned = fulls.to_vec();
        super::reconcile::reconcile_tick(state, me, store, dir, &fulls_owned, areas);
        return outcome;
    }
    // Fresh topology at actuation time: exactly one re-enumeration per
    // tail; drift withholds the drop and reconciles with the fresh
    // topology instead.
    let Some(fresh_areas) = fresh_cross_areas() else {
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "gesture",
                "tick": tick,
                "correlation": correlation_str,
                "op": "drag-drop",
                "disposition": "refused",
                "outcome": "move-unverified",
                "kind": "topology-unknown",
                "window": settle.mover_token,
                "producer": settle.producer,
                "cross_output": true,
            }),
        );
        let fulls_owned = fulls.to_vec();
        super::reconcile::reconcile_tick(state, me, store, dir, &fulls_owned, areas);
        return "move-unverified";
    };
    if !cross_topology_fresh(fence, &fresh_areas) {
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "gesture",
                "tick": tick,
                "correlation": correlation_str,
                "op": "drag-drop",
                "disposition": "refused",
                "outcome": "move-unverified",
                "kind": "topology-changed",
                "window": settle.mover_token,
                "producer": settle.producer,
                "cross_output": true,
            }),
        );
        let fresh_fulls: Vec<Rect> = fresh_areas.iter().map(|a| a.full).collect();
        super::reconcile::reconcile_tick(state, me, store, dir, &fresh_fulls, &fresh_areas);
        return "move-unverified";
    }
    let Some(wire) = state.wire_of(&settle.dest_output) else {
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "gesture",
                "tick": tick,
                "correlation": correlation_str,
                "op": "drag-drop",
                "disposition": "refused",
                "outcome": "drag-drop-refused",
                "kind": "unknown-output",
                "window": settle.mover_token,
                "producer": settle.producer,
                "cross_output": true,
            }),
        );
        return "drag-drop-refused";
    };
    let foreground = crate::ffi::foreground_window();
    let reply = super::world_tick::run_world_command(
        state,
        me,
        store,
        dir,
        observed,
        retained,
        areas,
        foreground,
        fulls,
        tiler_core::world_report::WorldCommand::Send {
            window: tiler_core::directional::WindowId(settle.mover_token.clone()),
            target_output: tiler_core::directional::OutputId(wire),
            target_workspace: settle.dest_workspace.clone(),
            follow: true,
        },
    );
    let Some(reply) = reply.as_ref() else {
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "gesture",
                "tick": tick,
                "correlation": correlation_str,
                "op": "drag-drop",
                "disposition": "refused",
                "outcome": "drag-drop-refused",
                "kind": "unverified",
                "window": settle.mover_token,
                "producer": settle.producer,
                "cross_output": true,
            }),
        );
        let fulls_owned = fulls.to_vec();
        super::reconcile::reconcile_tick(state, me, store, dir, &fulls_owned, areas);
        return "drag-drop-refused";
    };
    // Required arrival proof before follow focus: the mover's desired
    // allocation must read back live on the destination monitor.
    let arrived = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == settle.mover_token)
        .and_then(|row| row.allocation)
        .is_some_and(|alloc| {
            state
                .readback_frame(me, fulls, settle.member_key.hwnd)
                .is_some_and(|visible| {
                    visible == alloc && output_for_rect(&fresh_areas, &visible) == settle.dest_output
                })
        });
    if !arrived {
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "gesture",
                "tick": tick,
                "correlation": correlation_str,
                "op": "drag-drop",
                "disposition": "refused",
                "outcome": "move-unverified",
                "window": settle.mover_token,
                "producer": settle.producer,
                "cross_output": true,
            }),
        );
        return "move-unverified";
    }
    // Follow focus without hiding: the arrival is already in the
    // destination current view, so only foreground the mover.
    let actuation = actuate_focus(state, me, fulls, observed, retained, &settle.mover_token);
    if actuation.outcome != "focus-ok" {
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "gesture",
                "tick": tick,
                "correlation": correlation_str,
                "op": "drag-drop",
                "disposition": "applied",
                "outcome": actuation.outcome,
                "window": settle.mover_token,
                "producer": settle.producer,
                "cross_output": true,
                "setter_accepted": actuation.setter_accepted,
                "prime_inserted": actuation.prime_inserted,
                "attach_ok": actuation.attach_ok,
                "eventual": actuation.eventual,
                "deferred": actuation.deferred,
            }),
        );
        return actuation.outcome;
    }
    note_focus_advance(state, observed, retained, &settle.mover_token);
    log_json_at(
        &state.log_path,
        serde_json::json!({
            "event": "gesture",
            "tick": tick,
            "correlation": correlation_str,
            "op": "drag-drop",
            "disposition": "applied",
            "outcome": "drag-drop-applied",
            "window": settle.mover_token,
            "producer": settle.producer,
            "cross_output": true,
            "setter_accepted": actuation.setter_accepted,
            "prime_inserted": actuation.prime_inserted,
            "attach_ok": actuation.attach_ok,
            "eventual": actuation.eventual,
            "deferred": actuation.deferred,
        }),
    );
    "drag-drop-applied"
}
