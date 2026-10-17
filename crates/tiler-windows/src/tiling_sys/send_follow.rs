#![forbid(unsafe_code)]
use super::*;

/// Shared send tail for both the Engine plan route and the native
/// cross-boundary route: commit-before-hide the mover, then follow by
/// selecting the target (which reveals it and reconciles it when tiled).
/// The carried overlay snapshot re-verifies live before hide (item 9):
/// a newly arrived fullscreen refuses, other drift defers, with no restore
/// and no replay. Broader view/mode/gap races stay with the select's own
/// fresh fences below. When `deferred` is set (D16 slotless carry), the
/// target select skips its geometry application while keeping observation,
/// reconcile, reveal, focus, and fences.
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace_send_follow(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &mut [ObservedWindow],
    mover_key: &crate::workspace::WindowKey,
    stored: &ProcessIdentity,
    mover_minimized: bool,
    output: &str,
    target_id: &str,
    source_token: &str,
    target_token: &str,
    ctx: &ActionCtx,
    source: Option<ApplySummary>,
    source_ms: u64,
    source_plan_ms: u64,
    preflight: SendPreflight,
    deferred: bool,
) -> SendEffect {
    if let Err(outcome) = preflight.overlay_hold(mover_key.hwnd, fulls) {
        return SendEffect {
            outcome,
            focus: "none",
            source,
            target: None,
            transition_ms: 0,
            observation_ms: 0,
            source_geometry_ms: source_ms,
            target_geometry_ms: 0,
            hide_ms: 0,
            reveal_ms: 0,
            focus_ms: 0,
            source_plan_ms,
            target_plan_ms: 0,
            source_workspace: source_token.to_owned(),
            target_workspace: target_token.to_owned(),
        };
    }
    let mut clock = action_clock();
    let hide_done = hide_phase(
        state,
        me,
        store,
        dir,
        ctx,
        vec![HideAttempt {
            key: mover_key.clone(),
            expected: stored.clone(),
            iconic: mover_minimized,
        }],
        &mut clock,
    );
    log_visibility_timing(
        state,
        ctx,
        "send-hide",
        &hide_done.breakdown,
        &PhaseBreakdown::default(),
    );
    let (hide_outcome, _hide_detail) = match hide_done.results.first() {
        Some(result) => (result.outcome, result.detail),
        None => ("uncertain", None),
    };
    if state.hidden_claims.contains_key(mover_key) {
        state.workspaces.set_hidden(mover_key, true);
    }
    if hide_outcome != "hidden" {
        return SendEffect {
            outcome: hide_outcome,
            focus: "none",
            source,
            target: None,
            transition_ms: 0,
            observation_ms: 0,
            source_geometry_ms: source_ms,
            target_geometry_ms: 0,
            hide_ms: 0,
            reveal_ms: 0,
            focus_ms: 0,
            source_plan_ms,
            target_plan_ms: 0,
            source_workspace: source_token.to_owned(),
            target_workspace: target_token.to_owned(),
        };
    }
    // Follow focuses the verified mover, fullscreen like ordinary.
    let select = workspace_do_select(
        state,
        me,
        store,
        dir,
        fulls,
        areas,
        observed,
        output,
        target_id,
        ctx,
        Some(mover_key),
        false,
        deferred,
    );
    if select.outcome != "ok" {
        return SendEffect {
            outcome: select.outcome,
            focus: select.focus,
            source,
            target: select.geometry,
            transition_ms: select.transition_ms,
            observation_ms: select.observation_ms,
            source_geometry_ms: source_ms,
            target_geometry_ms: select.geometry_ms,
            hide_ms: select.hide_ms,
            reveal_ms: select.reveal_ms,
            focus_ms: select.focus_ms,
            source_plan_ms,
            target_plan_ms: select.plan_ms,
            source_workspace: source_token.to_owned(),
            target_workspace: select.target_workspace.clone(),
        };
    }
    // Verified-only mapping, fullscreen like ordinary: the select already
    // revealed the arrival, so only `focus-ok` reports success.
    let outcome: &'static str = if select.focus == "focus-ok" {
        "ok"
    } else {
        "focus-unverified"
    };
    SendEffect {
        outcome,
        focus: select.focus,
        source,
        target: select.geometry,
        transition_ms: select.transition_ms,
        observation_ms: select.observation_ms,
        source_geometry_ms: source_ms,
        target_geometry_ms: select.geometry_ms,
        hide_ms: select.hide_ms,
        reveal_ms: select.reveal_ms,
        focus_ms: select.focus_ms,
        source_plan_ms,
        target_plan_ms: select.plan_ms,
        source_workspace: source_token.to_owned(),
        target_workspace: select.target_workspace.clone(),
    }
}
