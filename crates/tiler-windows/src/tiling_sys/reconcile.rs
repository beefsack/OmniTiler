#![forbid(unsafe_code)]
use super::focus_actuation::overlay_expected;
use super::geometry::{ApplyInput, apply_geometry};
use super::rows::assemble_domain_rows;
use super::*;

/// Engine-writable tokens for one domain: eligible observed members only.
/// Retained and hidden rows converge membership but never take geometry
/// writes; hidden workspace geometry is not written until reveal.
///
/// Native HWND/PID/creation plus scope gates pre-filter here; the portable
/// hidden/fresh exclusion rides [`crate::workspace_owner::writable_subset`],
/// the same seam the portable regression covers, so production and tests
/// share one rule without weakening any native check.
pub(super) fn writable_tokens(
    state: &TileLoop,
    output: &str,
    workspace: &str,
    observed: &[ObservedWindow],
) -> HashSet<String> {
    let members = state.workspaces.workspace_members(output, workspace);
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let mut eligible: std::collections::BTreeMap<crate::workspace::WindowKey, String> =
        std::collections::BTreeMap::new();
    for key in &members {
        let Some(window) = by_hwnd.get(&key.hwnd) else {
            continue;
        };
        if !crate::workspace_owner::member_matches(
            key,
            window.hwnd,
            window.identity.pid,
            &window.identity.process_creation,
        ) {
            continue;
        }
        if !scope_allows(&state.scope, &window.identity.exe_path) {
            continue;
        }
        eligible.insert(key.clone(), window.token.clone());
    }
    let fresh: HashSet<String> = observed.iter().map(|w| w.token.clone()).collect();
    // Floats converge Engine membership but never take tiled geometry writes.
    let domain_key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(output.to_owned()),
        workspace: tiler_core::directional::WorkspaceId(workspace.to_owned()),
    };
    let float_tokens: HashSet<String> = state
        .engine
        .session(&domain_key)
        .map(|session| {
            eligible
                .values()
                .filter(|token| session.is_exception(&WindowId(token.to_string())))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    eligible.retain(|_, token| !float_tokens.contains(token));
    // Observation-scoped restart-hydration write barrier: a rejected staged
    // float keeps no Engine exception and no intent lane, so the exception
    // exclusion above cannot see it. Exact lifetime identity, never HWND
    // alone; the next preamble clears and rebuilds the set.
    eligible.retain(|key, _| !state.restart_reject_hold.contains(key));
    crate::workspace_owner::writable_subset(
        &members,
        |k| state.workspaces.is_hidden(k),
        &eligible,
        &fresh,
    )
}

/// Auto-retire positively restored owned-fullscreen markers on verified
/// workspace members through the shared exact-lifetime sequence, so no
/// special clear action is needed to trigger it. Global game/elevated gate
/// only: per-action home/focus do not apply to background retirement, but
/// every candidate still passes the exact member/window-tag/held-identity/
/// scope proof inside the sequence plus the double live restoration proof.
/// Metadata removal only, never a geometry write; fullscreen, transitional,
/// unreadable, recycled, and zero-bit shapes keep their markers. Candidates
/// need verified workspace membership; `overlay_expected` supplies the
/// retained-aware expectation the pin requires.
pub(super) fn sweep_retire_stale_fullscreen(
    effects: &mut dyn UnfloatClearEffects,
    state: &mut TileLoop,
    me: &ProcessIdentity,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    fulls: &[Rect],
) {
    if effects.suspend_block(state, me, fulls) || effects.elevated(me) {
        return;
    }
    let candidates: Vec<(crate::workspace::WindowKey, String)> = state
        .member_tokens
        .iter()
        .map(|(key, token)| (key.clone(), token.clone()))
        .collect();
    for (key, token) in candidates {
        if state.workspaces.member_loc(&key).is_none() {
            continue;
        }
        let Some(expected) = overlay_expected(state, observed, retained, &token) else {
            continue;
        };
        if expected.hwnd != key.hwnd
            || expected.identity.pid != key.pid
            || expected.identity.process_creation != key.creation
        {
            continue;
        }
        let pre = effects.fullscreen_meta(key.hwnd);
        if pre.is_none() {
            continue;
        }
        let _ =
            effective_stale_meta_via_effects(effects, state, &key, &expected, me, pre, true, fulls);
    }
}

/// Run one reconcile tick partitioned per output-local domain: enumerate
/// once, then converge each monitor's current workspace through its own
/// retained Engine session with that monitor's `rcWork` bounds. Hidden
/// workspace sessions are never reconciled, so their membership and layout
/// survive untouched; no layout reseeds across switches.
pub(super) fn reconcile_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    #[cfg(test)]
    if INJECTED_OBSERVE.with(|slot| slot.borrow().is_some()) {
        PENDING_COUNTERS.with(|slot| {
            slot.borrow_mut().reconcile += 1;
        });
        state.tick += 1;
        let _ = (me, fulls, areas);
        return;
    }
    state.tick += 1;
    let tick = state.tick;
    let correlation = state.correlation();
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let log_path = state.log_path.clone();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        log_json_at(
            &log_path,
            serde_json::json!({"event":"tick-skip","tick":tick,"cause":"enum-failed"}),
        );
        return;
    };
    {
        let mut native_retire = NativeUnfloatClear;
        sweep_retire_stale_fullscreen(&mut native_retire, state, me, &observed, &retained, fulls);
    }
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    // First-seen maximized windows restore once here: lifetime tags are
    // stamped, and the cleared window converges as eligible next tick.
    clear_maximize_at_admission(state, me, &retained);
    workspace_close_cleanup(state);
    // Bounded normal-level visibility/exception snapshots over the observation
    // just taken: managed visibility/cloak transitions and exception-lane
    // changes with reasons, deduplicated and throttled to one batched event
    // per second each. Steady ticks emit nothing; logging never affects
    // behavior. Opaque tokens only. Suspended ticks never sample here (the
    // loop returns before reconcile), so transitions inside suspension are
    // unsampled by design, never interpolated.
    stage_visibility(state, &observed, &retained, &skipped);
    stage_exceptions(state);
    if let Some(event) = flush_visibility(state) {
        log_json_at(&log_path, event);
    }
    if let Some(event) = flush_exceptions(state) {
        log_json_at(&log_path, event);
    }
    // Structured observer tick before Engine dispatch: full EnumWindows
    // success with the raw enumerated count and the managed (eligible,
    // allowlisted) count, including zero. A zero managed set is a completed
    // observation, not a failure: the Engine may answer Rejected for an empty
    // domain and the nonmoving proof must not require a layout plan.
    let enumerated = state.last_enumerated;
    let managed = observed.len();
    if state.trace {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "observe",
                "tick": tick,
                "op": "reconcile",
                "enumerated": enumerated,
                "managed": managed,
            }),
        );
    }
    if let Some(audit) = state.audit_path.clone() {
        log_json_at(
            &audit,
            serde_json::json!({
                "event": "proof-observe",
                "tick": tick,
                "op": "reconcile",
                "enumerated": enumerated,
                "managed": managed,
            }),
        );
    }
    // One shared hint-query budget for the whole tick across all outputs:
    // no domain independently blows the per-operation bound.
    let mut hint_cx = HintCx::new();
    for area in areas {
        let output = area.device.clone();
        state.workspaces.ensure_output(&output);
        let Some(active) = state.workspaces.active_id(&output) else {
            continue;
        };
        // Floating workspaces leave frames untouched: no Engine reconcile
        // and no geometry writes for that domain. Membership, hide/reveal,
        // and the independent active border still run.
        if !workspace_mode_tiled(state, &output, &active) {
            continue;
        }
        if tiling_domain_bounds_with(area.work, state.outer_gap).is_none() {
            continue;
        }
        // Incomplete snapshot defers with retained Engine state: a falsely
        // complete observation would drop membership and layout.
        let Some(rows) = assemble_domain_rows(
            state,
            &output,
            &active,
            &observed,
            &retained,
            "reconcile",
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
        let focused = state
            .focused_token(&observed)
            .filter(|f| windows.iter().any(|(w, _, _, _)| w == f));
        let Some((domain, domain_key)) =
            workspace_domain_for(&output, &active, areas, state.inner_gap, state.outer_gap)
        else {
            continue;
        };
        // Retained gap drift adopts through the topology-preserving
        // update-gaps path: plain reconcile refuses domain-mismatch instead
        // of adopting it, and fresh domains still seed via reconcile.
        let gaps_adopted = crate::settings::retained_gaps_match(
            &state.engine,
            &domain_key,
            state.inner_gap,
            state.outer_gap,
        );
        let mut event = crate::tiling::build_reconcile_event_for_floating(
            &state.owner,
            &state.generation,
            &correlation,
            revision_for(state, &output, &active),
            fp,
            &domain,
            &domain_key,
            state.outer_gap,
            &windows,
            focused.as_ref(),
        );
        if !gaps_adopted {
            event.command = CoreCommand::UpdateGaps;
        }
        let reply = handle_engine_event(state, &mut event);
        log_engine_placement_trace(state, tick, correlation.as_str(), "reconcile");
        let writable = writable_tokens(state, &output, &active, &observed);
        apply_geometry(
            state,
            ApplyInput {
                me,
                fulls,
                reply: &reply,
                observed: &observed,
                op: "reconcile",
                tick,
                correlation: correlation.as_str(),
                skipped: skipped.clone(),
                writable: &writable,
                output_token: state.workspaces.output_token(&output),
                workspace_token: state.workspaces.workspace_token(&output, &active),
                revision: revision_for(state, &output, &active),
            },
        );
        // Best-effort adoption: committed capacity floats join the
        // ordinary float lanes through the shared tick helper below.
        adopt_tick_capacity_floats(
            state,
            me,
            tick,
            correlation.as_str(),
            &reply,
            &observed,
            &domain_key,
        );
    }
    // D23: queued closes homed outside the selected set (hidden/inactive
    // workspaces) drain through ordinary observations here, before any later
    // registry pruning can strand them. Engine commits only; hidden domains
    // take no geometry or focus writes.
    fanout_pending_closed(state, me, areas, &observed, &retained, correlation.as_str());
}
