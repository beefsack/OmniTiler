#![forbid(unsafe_code)]
use super::*;

/// Cross-output drag-drop settle (REQ-MOU-03): native title-bar and Win-drag
/// releases landing on another output join that output's CURRENT workspace at
/// the pointer point through the shared Engine drop resolver, and the source
/// membership is removed. The destination is the output under the pointer at
/// release (FULL-rectangle containment, never the directional adjacency
/// selector), so sideways, stacked, and negative-coordinate arrangements all
/// behave alike. The destination view is never hidden or switched: only
/// tiled geometry writes plus mover follow-focus run.
///
/// Producer split (REQ-DRAG-07): a native title-bar frame may physically move
/// before END, so domain decisions use retained project membership and the
/// pointer point, never the live frame monitor. A Win-drag keeps the source
/// stationary with a moving render preview; the drop runs only after the
/// proven preview-hide acknowledgement with fresh revalidation.
pub(super) struct CrossDragSettle {
    pub(super) mover_token: String,
    pub(super) member_key: crate::workspace::WindowKey,
    pub(super) source_output: String,
    pub(super) source_workspace: String,
    pub(super) dest_output: String,
    pub(super) dest_workspace: String,
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) drop_prior: Option<tiler_core::session::DragHoverPrior>,
    pub(super) producer: String,
    /// PRE-gesture allocation of the mover (`gesture_before` at END time):
    /// the authoritative source-capture rectangle for a natively moved
    /// frame. `None` keeps the observed frame (unavailable START capture).
    pub(super) start_rect: Option<Rect>,
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
    let Some(workspace) = state.workspaces.active_id(&dest) else {
        return DropDestination::Refused("no-view");
    };
    if !state.workspaces.is_tiled(&dest, &workspace) {
        return DropDestination::Refused("floating-dest");
    }
    let Some((domain, _)) =
        workspace_domain_for(&dest, &workspace, areas, state.inner_gap, state.outer_gap)
    else {
        return DropDestination::Refused("unknown-output");
    };
    if !crate::tiling::drop_point_in_domain(
        domain.bounds.x,
        domain.bounds.y,
        domain.bounds.w,
        domain.bounds.h,
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
) -> Option<crate::workspace_owner::DragDropFenceSnapshot> {
    let source_area = areas.iter().find(|a| a.device == settle.source_output)?;
    let target_area = areas.iter().find(|a| a.device == settle.dest_output)?;
    let source_bounds = tiling_domain_bounds_with(source_area.work, state.outer_gap)?;
    let target_bounds = tiling_domain_bounds_with(target_area.work, state.outer_gap)?;
    Some(crate::workspace_owner::DragDropFenceSnapshot {
        inner_gap: state.inner_gap,
        outer_gap: state.outer_gap,
        source_output: settle.source_output.clone(),
        source_workspace: settle.source_workspace.clone(),
        target_output: settle.dest_output.clone(),
        target_workspace: settle.dest_workspace.clone(),
        source_tiled: state
            .workspaces
            .is_tiled(&settle.source_output, &settle.source_workspace),
        target_tiled: state
            .workspaces
            .is_tiled(&settle.dest_output, &settle.dest_workspace),
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
    snapshot: &crate::workspace_owner::DragDropFenceSnapshot,
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

/// Shared cross-output row/domain assembly: source plus destination
/// current-view rows, both domain values, and the combined fingerprint.
/// Adopts live gap edits on both domains first so the pair convergence never
/// straddles gap generations. Returns `None` on incomplete rows or unreadable
/// topology: the caller defers with retained state instead of presenting a
/// falsely complete observation. The mover must ride the source rows.
pub(super) struct CrossDragRows {
    pub(super) source_rows: Vec<crate::workspace_owner::OwnerRow>,
    pub(super) dest_rows: Vec<crate::workspace_owner::OwnerRow>,
    pub(super) source_domain: tiler_core::session::OutputDomain,
    pub(super) source_key: tiler_core::session::DomainKey,
    pub(super) dest_domain: tiler_core::session::OutputDomain,
    pub(super) dest_key: tiler_core::session::DomainKey,
    pub(super) combined_fp: u64,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn assemble_cross_drag_rows(
    state: &mut TileLoop,
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    settle: &CrossDragSettle,
    correlation: &CorrelationId,
    correlation_str: &str,
    op: &str,
) -> Option<CrossDragRows> {
    let mut hint_cx = HintCx::new();
    let mut source_rows = assemble_domain_rows(
        state,
        &settle.source_output,
        &settle.source_workspace,
        observed,
        retained,
        op,
        correlation_str,
        &mut hint_cx,
    )?;
    let dest_rows = assemble_domain_rows(
        state,
        &settle.dest_output,
        &settle.dest_workspace,
        observed,
        retained,
        op,
        correlation_str,
        &mut hint_cx,
    )?;
    if !source_rows
        .iter()
        .any(|row| row.token == settle.mover_token)
    {
        return None;
    }
    // Retained-gesture capture rectangle: a natively moved frame travels
    // with the OS modal loop, so the live observed frame may sit outside
    // the source bounds while project membership still homes the mover on
    // the source. The Engine binds homing (never rects), but the source
    // capture row must stay authoritative: substitute the PRE-gesture
    // allocation for the mover row ONLY, under the settle's exact START
    // fences. Destination current rows keep live values, size hints stay
    // fresh from the live frame, and native actuation below still reads
    // the true observed frames.
    if let Some(start_rect) = settle.start_rect {
        for row in source_rows.iter_mut() {
            if row.token == settle.mover_token {
                row.rect = start_rect;
            }
        }
    }
    let (source_domain, source_key) = workspace_domain_for(
        &settle.source_output,
        &settle.source_workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    )?;
    let (dest_domain, dest_key) = workspace_domain_for(
        &settle.dest_output,
        &settle.dest_workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    )?;
    {
        let outer_gap = state.outer_gap;
        let mover = WindowId(settle.mover_token.clone());
        let source_windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = source_rows
            .iter()
            .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
            .collect();
        let source_fp = crate::tiling::fingerprint(
            &source_rows
                .iter()
                .map(|r| (r.token.clone(), r.rect))
                .collect::<Vec<_>>(),
        );
        adopt_gaps_for_route(
            state,
            &source_domain,
            &source_key,
            outer_gap,
            &source_windows,
            Some(&mover),
            revision_for(state, &settle.source_output, &settle.source_workspace),
            source_fp,
            correlation,
        );
        let dest_windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = dest_rows
            .iter()
            .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
            .collect();
        let dest_fp = crate::tiling::fingerprint(
            &dest_rows
                .iter()
                .map(|r| (r.token.clone(), r.rect))
                .collect::<Vec<_>>(),
        );
        adopt_gaps_for_route(
            state,
            &dest_domain,
            &dest_key,
            outer_gap,
            &dest_windows,
            None,
            revision_for(state, &settle.dest_output, &settle.dest_workspace),
            dest_fp,
            correlation,
        );
    }
    // Combined two-domain observation: source rows homed on the source key,
    // destination rows homed on the destination key. Fingerprint covers both
    // so stale destination rects cannot ride.
    let combined_fp = {
        let mut pairs: Vec<(String, Rect)> = source_rows
            .iter()
            .chain(dest_rows.iter())
            .map(|r| (r.token.clone(), r.rect))
            .collect();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        crate::tiling::fingerprint(&pairs)
    };
    Some(CrossDragRows {
        source_rows,
        dest_rows,
        source_domain,
        source_key,
        dest_domain,
        dest_key,
        combined_fp,
    })
}

/// Candidate-engine twin of [`assemble_cross_drag_rows`]: identical rows,
/// domains, START substitution, and combined fingerprint, but live gap edits
/// adopt on the caller's clone via [`adopt_gaps_on_candidate`] so a later
/// refused tail discards the adopt with the plan instead of committing it
/// early to the authoritative Engine. No authoritative Engine mutation.
#[allow(clippy::too_many_arguments)]
fn assemble_cross_drag_rows_on_candidate(
    state: &mut TileLoop,
    candidate: &mut Engine,
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    settle: &CrossDragSettle,
    correlation: &CorrelationId,
    correlation_str: &str,
    op: &str,
) -> Option<CrossDragRows> {
    let mut hint_cx = HintCx::new();
    let mut source_rows = assemble_domain_rows(
        state,
        &settle.source_output,
        &settle.source_workspace,
        observed,
        retained,
        op,
        correlation_str,
        &mut hint_cx,
    )?;
    let dest_rows = assemble_domain_rows(
        state,
        &settle.dest_output,
        &settle.dest_workspace,
        observed,
        retained,
        op,
        correlation_str,
        &mut hint_cx,
    )?;
    if !source_rows
        .iter()
        .any(|row| row.token == settle.mover_token)
    {
        return None;
    }
    if let Some(start_rect) = settle.start_rect {
        for row in source_rows.iter_mut() {
            if row.token == settle.mover_token {
                row.rect = start_rect;
            }
        }
    }
    let (source_domain, source_key) = workspace_domain_for(
        &settle.source_output,
        &settle.source_workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    )?;
    let (dest_domain, dest_key) = workspace_domain_for(
        &settle.dest_output,
        &settle.dest_workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    )?;
    {
        let outer_gap = state.outer_gap;
        let mover = WindowId(settle.mover_token.clone());
        let source_windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = source_rows
            .iter()
            .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
            .collect();
        let source_fp = crate::tiling::fingerprint(
            &source_rows
                .iter()
                .map(|r| (r.token.clone(), r.rect))
                .collect::<Vec<_>>(),
        );
        adopt_gaps_on_candidate(
            candidate,
            state,
            &source_domain,
            &source_key,
            outer_gap,
            &source_windows,
            Some(&mover),
            revision_for(state, &settle.source_output, &settle.source_workspace),
            source_fp,
            correlation,
        );
        let dest_windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = dest_rows
            .iter()
            .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
            .collect();
        let dest_fp = crate::tiling::fingerprint(
            &dest_rows
                .iter()
                .map(|r| (r.token.clone(), r.rect))
                .collect::<Vec<_>>(),
        );
        adopt_gaps_on_candidate(
            candidate,
            state,
            &dest_domain,
            &dest_key,
            outer_gap,
            &dest_windows,
            None,
            revision_for(state, &settle.dest_output, &settle.dest_workspace),
            dest_fp,
            correlation,
        );
    }
    let combined_fp = {
        let mut pairs: Vec<(String, Rect)> = source_rows
            .iter()
            .chain(dest_rows.iter())
            .map(|r| (r.token.clone(), r.rect))
            .collect();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        crate::tiling::fingerprint(&pairs)
    };
    Some(CrossDragRows {
        source_rows,
        dest_rows,
        source_domain,
        source_key,
        dest_domain,
        dest_key,
        combined_fp,
    })
}

/// Build the cross-output pair event for one settle: source plus destination
/// current-view rows, a combined fingerprint, the source-first pair without
/// adjacency entries (pointer-resolved, so the send-style assembly applies),
/// and the `DragDrop` command with the Started source binding. Returns `None`
/// on incomplete rows: the caller defers with retained state instead of
/// presenting a falsely complete observation.
#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
pub(super) fn prepare_cross_drag_event(
    state: &mut TileLoop,
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    settle: &CrossDragSettle,
    correlation: &CorrelationId,
    correlation_str: &str,
    op: &str,
) -> Option<CoreEvent> {
    let rows = assemble_cross_drag_rows(
        state,
        areas,
        observed,
        retained,
        settle,
        correlation,
        correlation_str,
        op,
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
    // Source-first pair with no adjacency entries: the destination is
    // pointer-resolved, so the send-style assembly (no adjacency fence)
    // applies, never the directional candidate selector.
    event.directional = Some(vec![
        (rows.source_domain, rows.source_key.clone()),
        (rows.dest_domain, rows.dest_key),
    ]);
    event.directional_target_outer_gap = Some(state.outer_gap);
    event.command = CoreCommand::DragDrop {
        window: from.0.clone(),
        x: settle.x,
        y: settle.y,
        hover_prior: settle.drop_prior.clone(),
        source: Some(rows.source_key),
    };
    Some(event)
}

/// Candidate-engine twin of [`prepare_cross_drag_event`]: identical pair
/// shape, but row assembly adopts gaps on the caller's clone so event prep
/// never mutates the authoritative Engine. The caller stamps and dispatches
/// on that same clone.
#[allow(clippy::too_many_arguments)]
fn prepare_cross_drag_event_on_candidate(
    state: &mut TileLoop,
    candidate: &mut Engine,
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    settle: &CrossDragSettle,
    correlation: &CorrelationId,
    correlation_str: &str,
    op: &str,
) -> Option<CoreEvent> {
    let rows = assemble_cross_drag_rows_on_candidate(
        state,
        candidate,
        areas,
        observed,
        retained,
        settle,
        correlation,
        correlation_str,
        op,
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
    event.command = CoreCommand::DragDrop {
        window: from.0.clone(),
        x: settle.x,
        y: settle.y,
        hover_prior: settle.drop_prior.clone(),
        source: Some(rows.source_key),
    };
    Some(event)
}

/// Read-only arrival verification for a candidate cross-drop plan: the
/// mover's desired Engine rect must read back on the destination monitor.
/// No membership, focus, or Engine mutation; the caller publishes only after
/// this plus topology/apply/readback (and source reflow for empty flows)
/// verify.
fn cross_arrival_verified(
    state: &TileLoop,
    areas: &[MonitorArea],
    reply: &CoreReply,
    settle: &CrossDragSettle,
) -> bool {
    directional_arrival_verified(
        state,
        areas,
        reply,
        &settle.mover_token,
        settle.member_key.hwnd,
        &settle.dest_output,
    )
}

/// Post-commit follow for a verified cross drop: foreground the mover with no
/// workspace switch. Runs only after the candidate Engine and the project
/// membership published together; performs no logging itself.
fn follow_cross_drop(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    settle: &CrossDragSettle,
) -> FocusActuation {
    let actuation = actuate_focus(state, me, fulls, observed, retained, &settle.mover_token);
    if actuation.outcome == "focus-ok" {
        note_focus_advance(state, observed, retained, &settle.mover_token);
        state.workspaces.note_foreground(&settle.member_key);
    }
    actuation
}

/// Read-only predicted workspace-transfer check: the destination workspace
/// must be known and the post-move sets must satisfy `verify_send_follow`
/// without mutating project membership. The caller performs the real
/// `assign` only after every other tail verifies.
fn predicted_cross_transfer_ok(state: &TileLoop, settle: &CrossDragSettle) -> bool {
    if !state
        .workspaces
        .workspace_ids(&settle.dest_output)
        .iter()
        .any(|id| id == &settle.dest_workspace)
    {
        return false;
    }
    let mut source_now = state
        .workspaces
        .workspace_members(&settle.source_output, &settle.source_workspace);
    let mut target_now = state
        .workspaces
        .workspace_members(&settle.dest_output, &settle.dest_workspace);
    source_now.remove(&settle.member_key);
    target_now.insert(settle.member_key.clone());
    crate::workspace::verify_send_follow(&settle.member_key, &source_now, &target_now)
}

/// Shared arrival-plus-focus tail for a committed cross drop (pair and empty
/// flows): the mover's desired Engine rect must read back on the destination
/// monitor then follow-focus foregrounds the mover with no workspace switch.
/// Runs only after the caller published the candidate Engine and the project
/// membership together. Returns the outcome plus the focus actuation when one
/// ran (for caller logging); performs no logging itself.
#[allow(clippy::too_many_arguments)]
fn confirm_cross_drop(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    reply: &CoreReply,
    settle: &CrossDragSettle,
) -> (&'static str, Option<FocusActuation>) {
    if !cross_arrival_verified(state, areas, reply, settle) {
        return ("move-unverified", None);
    }
    let actuation = follow_cross_drop(state, me, fulls, observed, retained, settle);
    if actuation.outcome != "focus-ok" {
        let outcome = actuation.outcome;
        return (outcome, Some(actuation));
    }
    let source_now = state
        .workspaces
        .workspace_members(&settle.source_output, &settle.source_workspace);
    let target_now = state
        .workspaces
        .workspace_members(&settle.dest_output, &settle.dest_workspace);
    if !crate::workspace::verify_send_follow(&settle.member_key, &source_now, &target_now) {
        return ("unverified", Some(actuation));
    }
    ("drag-drop-applied", Some(actuation))
}

/// Refusal kind for a non-plan drag reply: typed refusals keep their
/// kind; a committed capacity-float transfer (which has no drag
/// remap) reports as capacity rather than an unhandled variant.
pub(super) fn drag_refusal_kind(reply: &CoreReply) -> &'static str {
    match reply {
        CoreReply::Rejected { kind, .. } => kind,
        CoreReply::Diverged(reason) => reason.as_str(),
        CoreReply::SnapshotInvalid { detail, .. } => detail,
        _ if reply_is_capacity_float_transfer(reply) => "capacity",
        _ => "unhandled-variant",
    }
}

/// Finalize an occupied-destination cross drop reply: the pair plan sits on
/// the discarded-on-refusal candidate, so the project membership commits
/// only after the fresh topology, the single combined apply, its readback,
/// and the arrival proof all verify. The authoritative Engine publishes with
/// that same commit, exactly once, before follow-focus. Returns the outcome;
/// refusals keep source membership and the retained Engine with no
/// follow-focus, no replay, and no success claims. Partial physical writes
/// are not atomic: observed geometry reconciles through the existing loop.
#[allow(clippy::too_many_arguments)]
fn finalize_cross_drag_drop(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    candidate: Engine,
    reply: &CoreReply,
    settle: &CrossDragSettle,
    fence: &crate::workspace_owner::DragDropFenceSnapshot,
    tick: u64,
    correlation_str: &str,
    skipped: Vec<(String, String)>,
) -> &'static str {
    let log_path = state.log_path.clone();
    let from = settle.mover_token.clone();
    let log_drop = |outcome: &'static str, extra: serde_json::Value| {
        let mut line = serde_json::Map::new();
        line.insert("event".to_owned(), serde_json::json!("gesture"));
        line.insert("tick".to_owned(), serde_json::json!(tick));
        line.insert("correlation".to_owned(), serde_json::json!(correlation_str));
        line.insert("op".to_owned(), serde_json::json!("drag-drop"));
        line.insert("outcome".to_owned(), serde_json::json!(outcome));
        line.insert("window".to_owned(), serde_json::json!(from));
        line.insert("producer".to_owned(), serde_json::json!(settle.producer));
        line.insert("cross_output".to_owned(), serde_json::json!(true));
        if let serde_json::Value::Object(map) = extra {
            for (k, v) in map {
                line.insert(k, v);
            }
        }
        log_json_at(&log_path, serde_json::Value::Object(line));
    };
    let CoreReply::Tiled(_) = reply else {
        let kind = drag_refusal_kind(reply);
        log_drop(
            "drag-drop-refused",
            serde_json::json!({"disposition": "refused", "kind": kind}),
        );
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return "drag-drop-refused";
    };
    // Commit-last: the candidate holds the plan, so refusals below discard
    // it with source membership and the authoritative Engine untouched and
    // no follow-focus. Per-window write gates still run inside the single
    // apply below.
    let candidate = candidate;
    // Fresh topology at actuation time: exactly one re-enumeration per
    // tail. An output removal or work-area change between pair-build and
    // native writes withholds every write (reconciling with the fresh
    // topology instead); the physical writes below reuse the fresh lists.
    let Some(fresh_areas) = fresh_cross_areas() else {
        log_drop(
            "move-unverified",
            serde_json::json!({"kind": "topology-unknown"}),
        );
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return "move-unverified";
    };
    if !cross_topology_fresh(fence, &fresh_areas) {
        log_drop(
            "move-unverified",
            serde_json::json!({"kind": "topology-changed"}),
        );
        let fresh_fulls: Vec<Rect> = fresh_areas.iter().map(|a| a.full).collect();
        reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
        return "move-unverified";
    }
    let fresh_fulls: Vec<Rect> = fresh_areas.iter().map(|a| a.full).collect();
    // One union-writable combined apply: source survivors reflow and the
    // destination places the arrival together, in DPI-aware physical
    // coordinates with no rescaling.
    let mut union_writable = writable_tokens(
        state,
        &settle.source_output,
        &settle.source_workspace,
        observed,
    );
    union_writable.extend(writable_tokens(
        state,
        &settle.dest_output,
        &settle.dest_workspace,
        observed,
    ));
    let summary = apply_geometry(
        state,
        ApplyInput {
            me,
            fulls: &fresh_fulls,
            reply,
            observed,
            op: "drag-drop",
            tick,
            correlation: correlation_str,
            skipped,
            writable: &union_writable,
            output_token: state.workspaces.output_token(&settle.source_output),
            workspace_token: state
                .workspaces
                .workspace_token(&settle.source_output, &settle.source_workspace),
            revision: revision_for(state, &settle.source_output, &settle.source_workspace),
        },
    );
    let Some(summary) = summary else {
        log_drop("move-unverified", serde_json::json!({}));
        return "move-unverified";
    };
    if !summary.readback_ok {
        log_drop("move-unverified", serde_json::json!({}));
        return "move-unverified";
    }
    if summary.mismatched > 0 {
        log_drop("move-mismatch", serde_json::json!({}));
        return "move-mismatch";
    }
    // Required arrival proof before any commit: a failed proof keeps source
    // membership and the retained Engine with no follow-focus and no replay;
    // the next observation converges (partial writes are never rolled back).
    if !cross_arrival_verified(state, &fresh_areas, reply, settle) {
        log_drop("move-unverified", serde_json::json!({}));
        return "move-unverified";
    }
    // Workspace transfer pre-check on predicted sets (no mutation): a refused
    // assignment keeps source membership with the candidate discarded.
    if !predicted_cross_transfer_ok(state, settle) {
        log_drop("unverified", serde_json::json!({}));
        return "unverified";
    }
    // Verified: commit both authorities once, then follow focus. The mover
    // rode the source writable set while homed on the source, so the union
    // above already covered it.
    if !state.workspaces.assign(
        settle.member_key.clone(),
        &settle.dest_output,
        &settle.dest_workspace,
        false,
    ) {
        log_drop("unverified", serde_json::json!({}));
        return "unverified";
    }
    state.engine = candidate;
    // Required arrival proof plus follow focus with no workspace switch.
    let (outcome, actuation) = confirm_cross_drop(
        state,
        me,
        &fresh_fulls,
        &fresh_areas,
        observed,
        retained,
        reply,
        settle,
    );
    match actuation {
        Some(actuation) => log_drop(
            outcome,
            serde_json::json!({
                "setter_accepted": actuation.setter_accepted,
                "prime_inserted": actuation.prime_inserted,
                "attach_ok": actuation.attach_ok,
                "eventual": actuation.eventual,
                "deferred": actuation.deferred,
            }),
        ),
        None => log_drop(outcome, serde_json::json!({})),
    }
    outcome
}

/// Reflow the source domain on the candidate after a verified destination
/// write: a targeted source reconcile without the mover removes it
/// candidate-side and reflows the survivors, then applies. The authoritative
/// Engine is never used to validate the post-move source; a refused tail
/// discards the candidate with source membership and the retained Engine
/// untouched. Returns the apply summary; `None` on incomplete rows or a
/// non-geometry reply (the caller reports unverified).
#[allow(clippy::too_many_arguments)]
fn apply_source_reflow(
    candidate: &mut Engine,
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    settle: &CrossDragSettle,
    tick: u64,
    correlation: &CorrelationId,
    correlation_str: &str,
) -> Option<ApplySummary> {
    let mut hint_cx = HintCx::new();
    let rows = assemble_domain_rows(
        state,
        &settle.source_output,
        &settle.source_workspace,
        observed,
        retained,
        "drag-drop",
        correlation_str,
        &mut hint_cx,
    )?;
    // Commit-last: the mover is still homed on the source project-side, so
    // the source rows still name it. The reflow plans the post-move source
    // without it: filter it here instead of refusing. A sole-mover source
    // yields no rows and retires below.
    let rows: Vec<_> = rows
        .into_iter()
        .filter(|row| row.token != settle.mover_token)
        .collect();
    let windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
        .iter()
        .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
        .collect();
    let fp = crate::tiling::fingerprint(
        &rows
            .iter()
            .map(|r| (r.token.clone(), r.rect))
            .collect::<Vec<_>>(),
    );
    let (domain, domain_key) = workspace_domain_for(
        &settle.source_output,
        &settle.source_workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    )?;
    // Focus falls to a surviving row: the mover just left, so carrying the
    // live foreground (often the mover itself mid-flight) would mismatch
    // the reconcile focus gate. Empty sources carry none and retire below.
    let focused = rows.first().map(|row| WindowId(row.token.clone()));
    {
        let outer_gap = state.outer_gap;
        adopt_gaps_on_candidate(
            candidate,
            state,
            &domain,
            &domain_key,
            outer_gap,
            &windows,
            focused.as_ref(),
            revision_for(state, &settle.source_output, &settle.source_workspace),
            fp,
            correlation,
        );
    }
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        correlation,
        revision_for(state, &settle.source_output, &settle.source_workspace),
        fp,
        &domain,
        &domain_key,
        state.outer_gap,
        &windows,
        focused.as_ref(),
    );
    event.command = CoreCommand::Reconcile;
    let reply = {
        stamp_engine_event(state, &mut event);
        #[cfg(test)]
        PENDING_COUNTERS.with(|slot| {
            slot.borrow_mut().engine += 1;
        });
        candidate.handle(&event)
    };
    match &reply {
        CoreReply::Tiled(_) | CoreReply::Projection(_) => {}
        _ => return None,
    }
    let mut writable = writable_tokens(
        state,
        &settle.source_output,
        &settle.source_workspace,
        observed,
    );
    // The mover still homes on the source project-side but plans nowhere
    // here: never hand it a source write.
    writable.remove(settle.mover_token.as_str());
    apply_geometry(
        state,
        ApplyInput {
            me,
            fulls,
            reply: &reply,
            observed,
            op: "drag-drop",
            tick,
            correlation: correlation_str,
            skipped: Vec::new(),
            writable: &writable,
            output_token: state.workspaces.output_token(&settle.source_output),
            workspace_token: state
                .workspaces
                .workspace_token(&settle.source_output, &settle.source_workspace),
            revision: revision_for(state, &settle.source_output, &settle.source_workspace),
        },
    )
}

/// Finalize an empty-destination cross drop reply: the singleton admission
/// plan sits on the discarded-on-refusal candidate, so the destination
/// applies first, then the source reconciles without the mover (removal plus
/// survivor reflow) on that same candidate, and both authorities publish
/// together only after the fresh topology, both applies, their readbacks,
/// and the arrival proof verify, before follow-focus. Returns the outcome;
/// refusals keep source membership and the retained Engine with no
/// follow-focus, no replay, and no success claims.
#[allow(clippy::too_many_arguments)]
fn finalize_empty_drag_drop(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    candidate: Engine,
    reply: &CoreReply,
    settle: &CrossDragSettle,
    fence: &crate::workspace_owner::DragDropFenceSnapshot,
    tick: u64,
    correlation: &CorrelationId,
    correlation_str: &str,
    skipped: Vec<(String, String)>,
) -> &'static str {
    let log_path = state.log_path.clone();
    let from = settle.mover_token.clone();
    let log_drop = |outcome: &'static str, extra: serde_json::Value| {
        let mut line = serde_json::Map::new();
        line.insert("event".to_owned(), serde_json::json!("gesture"));
        line.insert("tick".to_owned(), serde_json::json!(tick));
        line.insert("correlation".to_owned(), serde_json::json!(correlation_str));
        line.insert("op".to_owned(), serde_json::json!("drag-drop"));
        line.insert("outcome".to_owned(), serde_json::json!(outcome));
        line.insert("window".to_owned(), serde_json::json!(from));
        line.insert("producer".to_owned(), serde_json::json!(settle.producer));
        line.insert("cross_output".to_owned(), serde_json::json!(true));
        line.insert("empty_destination".to_owned(), serde_json::json!(true));
        if let serde_json::Value::Object(map) = extra {
            for (k, v) in map {
                line.insert(k, v);
            }
        }
        log_json_at(&log_path, serde_json::Value::Object(line));
    };
    let CoreReply::Tiled(_) = reply else {
        let kind = drag_refusal_kind(reply);
        log_drop(
            "drag-drop-refused",
            serde_json::json!({"disposition": "refused", "kind": kind}),
        );
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return "drag-drop-refused";
    };
    let mut candidate = candidate;
    // Fresh topology at actuation time (see the pair finalize): withhold
    // every write on drift and reuse the fresh lists below. No membership
    // or Engine mutation before the verified commit below.
    let Some(fresh_areas) = fresh_cross_areas() else {
        log_drop(
            "move-unverified",
            serde_json::json!({"kind": "topology-unknown"}),
        );
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return "move-unverified";
    };
    if !cross_topology_fresh(fence, &fresh_areas) {
        log_drop(
            "move-unverified",
            serde_json::json!({"kind": "topology-changed"}),
        );
        let fresh_fulls: Vec<Rect> = fresh_areas.iter().map(|a| a.full).collect();
        reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
        return "move-unverified";
    }
    let fresh_fulls: Vec<Rect> = fresh_areas.iter().map(|a| a.full).collect();
    // Destination places the sole arrival first so the mover always has a
    // committed home; the source reflow below never orphans it. The mover
    // is still homed on the source project-side, so the empty destination
    // writable set would filter it: include it explicitly.
    let mut dest_writable =
        writable_tokens(state, &settle.dest_output, &settle.dest_workspace, observed);
    dest_writable.insert(settle.mover_token.clone());
    let dest_summary = apply_geometry(
        state,
        ApplyInput {
            me,
            fulls: &fresh_fulls,
            reply,
            observed,
            op: "drag-drop",
            tick,
            correlation: correlation_str,
            skipped,
            writable: &dest_writable,
            output_token: state.workspaces.output_token(&settle.dest_output),
            workspace_token: state
                .workspaces
                .workspace_token(&settle.dest_output, &settle.dest_workspace),
            revision: revision_for(state, &settle.dest_output, &settle.dest_workspace),
        },
    );
    match dest_summary {
        Some(summary) if summary.readback_ok && summary.mismatched == 0 => {}
        Some(_) => {
            log_drop("move-unverified", serde_json::json!({}));
            return "move-unverified";
        }
        None => {
            log_drop("move-unverified", serde_json::json!({}));
            return "move-unverified";
        }
    }
    // Source survivors reflow without the mover on the same candidate. A
    // refused reflow discards the candidate with source membership and the
    // retained Engine untouched and no follow-focus; the next observation
    // converges (no replay, no fake native rollback).
    match apply_source_reflow(
        &mut candidate,
        state,
        me,
        &fresh_fulls,
        &fresh_areas,
        observed,
        retained,
        settle,
        tick,
        correlation,
        correlation_str,
    ) {
        Some(summary) if summary.readback_ok && summary.mismatched == 0 => {}
        _ => {
            log_drop("move-unverified", serde_json::json!({}));
            let fulls_owned = fulls.to_vec();
            reconcile_tick(state, me, &fulls_owned, areas);
            return "move-unverified";
        }
    }
    // Arrival must verify before either authority publishes; a failed proof
    // keeps source membership and the retained Engine with no follow-focus.
    if !cross_arrival_verified(state, &fresh_areas, reply, settle) {
        log_drop("move-unverified", serde_json::json!({}));
        return "move-unverified";
    }
    if !predicted_cross_transfer_ok(state, settle) {
        log_drop("unverified", serde_json::json!({}));
        return "unverified";
    }
    if !state.workspaces.assign(
        settle.member_key.clone(),
        &settle.dest_output,
        &settle.dest_workspace,
        false,
    ) {
        log_drop("unverified", serde_json::json!({}));
        return "unverified";
    }
    state.engine = candidate;
    let (outcome, actuation) = confirm_cross_drop(
        state,
        me,
        &fresh_fulls,
        &fresh_areas,
        observed,
        retained,
        reply,
        settle,
    );
    match actuation {
        Some(actuation) => log_drop(
            outcome,
            serde_json::json!({
                "setter_accepted": actuation.setter_accepted,
                "prime_inserted": actuation.prime_inserted,
                "attach_ok": actuation.attach_ok,
                "eventual": actuation.eventual,
                "deferred": actuation.deferred,
            }),
        ),
        None => log_drop(outcome, serde_json::json!({})),
    }
    outcome
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
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    settle: &CrossDragSettle,
    fence: &crate::workspace_owner::DragDropFenceSnapshot,
    tick: u64,
    correlation: &CorrelationId,
    correlation_str: &str,
) -> &'static str {
    // Occupancy reads live project membership (no observation assembly):
    // row assembly runs exactly once inside the prepare call below.
    if state
        .workspaces
        .workspace_members(&settle.dest_output, &settle.dest_workspace)
        .is_empty()
    {
        // Empty-flow contradiction fence: no project members but the Engine
        // still names destination windows means divergent bookkeeping;
        // refuse without a drop instead of orphaning membership.
        let dest_key = tiler_core::session::DomainKey {
            output: tiler_core::directional::OutputId(settle.dest_output.clone()),
            workspace: tiler_core::directional::WorkspaceId(settle.dest_workspace.clone()),
        };
        if state
            .engine
            .session(&dest_key)
            .is_some_and(|session| !session.snapshot().windows.is_empty())
        {
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "gesture",
                    "tick": tick,
                    "correlation": correlation_str,
                    "op": "drag-drop",
                    "disposition": "refused",
                    "outcome": "unverified",
                    "window": settle.mover_token,
                    "producer": settle.producer,
                    "cross_output": true,
                }),
            );
            return "unverified";
        }
        let mut hint_cx = HintCx::new();
        let Some(event) = prepare_cross_singleton_event(
            state,
            areas,
            observed,
            settle,
            correlation,
            &mut hint_cx,
            false,
        ) else {
            return "deferred";
        };
        let mut event = event;
        // Candidate dispatch: the singleton admission plans on a clone, so a
        // refused tail discards it with source membership and the
        // authoritative Engine untouched.
        let mut candidate = state.engine.clone();
        let reply = {
            stamp_engine_event(state, &mut event);
            #[cfg(test)]
            PENDING_COUNTERS.with(|slot| {
                slot.borrow_mut().engine += 1;
            });
            candidate.handle(&event)
        };
        return finalize_empty_drag_drop(
            state,
            me,
            fulls,
            areas,
            observed,
            retained,
            candidate,
            &reply,
            settle,
            fence,
            tick,
            correlation,
            correlation_str,
            Vec::new(),
        );
    }
    // Candidate pair route: gap adopts and the drop dispatch all run on the
    // clone; the authoritative Engine publishes once inside the finalize,
    // only after the verified tails commit.
    let mut candidate = state.engine.clone();
    let Some(event) = prepare_cross_drag_event_on_candidate(
        state,
        &mut candidate,
        areas,
        observed,
        retained,
        settle,
        correlation,
        correlation_str,
        "drag-drop",
    ) else {
        return "deferred";
    };
    let mut event = event;
    let reply = {
        stamp_engine_event(state, &mut event);
        #[cfg(test)]
        PENDING_COUNTERS.with(|slot| {
            slot.borrow_mut().engine += 1;
        });
        candidate.handle(&event)
    };
    finalize_cross_drag_drop(
        state,
        me,
        fulls,
        areas,
        observed,
        retained,
        candidate,
        &reply,
        settle,
        fence,
        tick,
        correlation_str,
        Vec::new(),
    )
}

/// Candidate-engine gap adopt for one pair-route domain: the same
/// convergence as [`adopt_gaps_for_route`] but dispatching on the caller's
/// clone, so a later refused tail discards the adopt with the plan instead
/// of committing it early to the authoritative Engine.
#[allow(clippy::too_many_arguments)]
pub(super) fn adopt_gaps_on_candidate(
    candidate: &mut Engine,
    state: &TileLoop,
    domain: &tiler_core::session::OutputDomain,
    domain_key: &tiler_core::session::DomainKey,
    outer_gap: i32,
    windows: &[(WindowId, Rect, WindowSizeHints, bool)],
    focused: Option<&WindowId>,
    revision: u64,
    fingerprint: u64,
    correlation: &CorrelationId,
) -> bool {
    if crate::settings::retained_gaps_match(candidate, domain_key, state.inner_gap, outer_gap) {
        return true;
    }
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        correlation,
        revision,
        fingerprint,
        domain,
        domain_key,
        outer_gap,
        windows,
        focused,
    );
    event.command = CoreCommand::UpdateGaps;
    stamp_engine_event(state, &mut event);
    let adopted = matches!(
        candidate.handle(&event),
        CoreReply::Projection(_) | CoreReply::Tiled(_)
    );
    adopted
        && crate::settings::retained_gaps_match(candidate, domain_key, state.inner_gap, outer_gap)
}

/// Adopt drifted live gaps through the retained topology-preserving
/// update-gaps path before a domain operation (focus/move/select/send/
/// gesture). A live gap edit between the last reconcile and this intent would
/// otherwise make the operation's own convergence refuse with
/// `domain-mismatch` (safe, but the keypress is lost) or, on a domain the
/// Engine has never seen, seed fresh with mixed gap generations. Best effort:
/// returns true when the retained gaps now match the carried values; false
/// leaves the caller's Engine outcome (refusal or fresh seed) as the honest
/// evidence and topology is never destroyed either way.
#[allow(clippy::too_many_arguments)]
pub(super) fn adopt_gaps_for_route(
    state: &mut TileLoop,
    domain: &tiler_core::session::OutputDomain,
    domain_key: &tiler_core::session::DomainKey,
    outer_gap: i32,
    windows: &[(WindowId, Rect, WindowSizeHints, bool)],
    focused: Option<&WindowId>,
    revision: u64,
    fingerprint: u64,
    correlation: &CorrelationId,
) -> bool {
    if crate::settings::retained_gaps_match(&state.engine, domain_key, state.inner_gap, outer_gap) {
        return true;
    }
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        correlation,
        revision,
        fingerprint,
        domain,
        domain_key,
        outer_gap,
        windows,
        focused,
    );
    event.command = CoreCommand::UpdateGaps;
    let adopted = matches!(
        handle_engine_event(state, &mut event),
        CoreReply::Projection(_) | CoreReply::Tiled(_)
    );
    adopted
        && crate::settings::retained_gaps_match(
            &state.engine,
            domain_key,
            state.inner_gap,
            outer_gap,
        )
}
