#![forbid(unsafe_code)]
use super::*;

/// Run one resolved cross-output directional intent through the retained
/// Engine pair route with complete fences and narrow recovery.
///
/// The caller already resolved `(target_output, target_workspace)` as the
/// destination current view. This assembles the target current-view rows,
/// builds the reciprocal pair event (`cross_output_transfer: true`) on a
/// candidate Engine clone, lets core decide local-first vs cross (sole-root
/// included), then actuates: moves commit project membership and the
/// candidate Engine together only after the fresh topology, the combined
/// apply, its readback, and the arrival proof verify, before following
/// focus; horizontal focus actuates
/// only (no geometry/membership writes, empty targets refuse via core).
/// Failures stop narrowly with no replay and no permanent disabling; the
/// retained Engine plan stands and the next observation converges. Returns
/// the snap outcome token and logs the full action line.
#[allow(clippy::too_many_arguments)]
pub(super) fn run_directional_cross(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    hint_cx: &mut HintCx,
    loc: &crate::workspace::MemberLoc,
    home: &crate::workspace::MemberLoc,
    member_key: &crate::workspace::WindowKey,
    from_token: &str,
    source_rows: &[crate::workspace_owner::OwnerRow],
    target_output: &str,
    target_workspace: &str,
    intent: &crate::snapkey::QueuedIntent,
    origin: &crate::snapkey::SnapOrigin,
    tick: u64,
    correlation: &CorrelationId,
    correlation_str: &str,
    continued: bool,
    skipped: Vec<(String, String)>,
    subject_maximized: bool,
    subject_fullscreen: bool,
) -> &'static str {
    let log_path = state.log_path.clone();
    let is_focus = intent.op == crate::snapkey::SnapOp::Focus;
    let log_snap = |outcome: &'static str, extra: serde_json::Value| {
        let mut line = serde_json::Map::new();
        line.insert("event".to_owned(), serde_json::json!("snap"));
        line.insert("tick".to_owned(), serde_json::json!(tick));
        line.insert("correlation".to_owned(), serde_json::json!(correlation_str));
        line.insert("op".to_owned(), serde_json::json!(intent.op.as_str()));
        line.insert(
            "direction".to_owned(),
            serde_json::json!(crate::snapkey::direction_name(intent.direction)),
        );
        line.insert("edge".to_owned(), serde_json::json!(intent.edge.as_str()));
        line.insert("disposition".to_owned(), serde_json::json!("consumed"));
        line.insert("outcome".to_owned(), serde_json::json!(outcome));
        line.insert("origin".to_owned(), serde_json::json!(origin.token));
        if let serde_json::Value::Object(map) = extra {
            for (k, v) in map {
                line.insert(k, v);
            }
        }
        log_json_at(&log_path, serde_json::Value::Object(line));
    };
    // Target current-view rows (hidden snapshots included so the planned
    // mutation reuses topology instead of remove/reseed). Incomplete defers
    // with retained state, never a falsely complete pair.
    let Some(target_rows) = assemble_domain_rows(
        state,
        target_output,
        target_workspace,
        observed,
        retained,
        "directional",
        correlation_str,
        hint_cx,
    ) else {
        log_snap("deferred", serde_json::json!({"window": from_token}));
        return "deferred";
    };
    let Some((source_domain_base, source_key)) = workspace_domain_for(
        &loc.output,
        &loc.workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    ) else {
        log_snap("unknown-output", serde_json::json!({"window": from_token}));
        return "unknown-output";
    };
    let Some((target_domain_base, target_key)) = workspace_domain_for(
        target_output,
        target_workspace,
        areas,
        state.inner_gap,
        state.outer_gap,
    ) else {
        log_snap("unknown-output", serde_json::json!({"window": from_token}));
        return "unknown-output";
    };
    // The pair re-derives live domain values so a stale view cannot ride.
    let ((source_domain, source_key), (target_domain, target_key)) =
        crate::workspace_owner::directional_pair_with_adjacency(
            (source_domain_base, source_key),
            (target_domain_base, target_key),
            intent.direction,
        );
    let Some(snapshot) = directional_snapshot(
        state,
        areas,
        &loc.output,
        &loc.workspace,
        target_output,
        target_workspace,
        intent.direction,
        subject_maximized,
        subject_fullscreen,
    ) else {
        log_snap("unknown-output", serde_json::json!({"window": from_token}));
        return "unknown-output";
    };
    if !state.member_identity.contains_key(member_key) {
        log_snap("unmanaged", serde_json::json!({"window": from_token}));
        return "unmanaged";
    }
    // Fresh user-safety gate before any Engine or project mutation: a
    // vetoing foreground or an elevated foreground that arrived during
    // validation authorizes no dispatch. Managed overlays bypass like a
    // managed maximize.
    if suspend_read(state, me, fulls).veto.block {
        log_snap("suspended", serde_json::json!({"window": from_token}));
        return "suspended";
    }
    if foreground_elevated(me) {
        log_snap(
            "elevated-foreground",
            serde_json::json!({"window": from_token}),
        );
        return "elevated-foreground";
    }
    // All fences fresh at this boundary: membership still homes on the
    // chord-time home, overlay re-read live, scope/lifetime/topology/owner
    // recomputed inside `directional_live`.
    let membership_ok = state
        .workspaces
        .member_loc(member_key)
        .is_some_and(|fresh| fresh.output == home.output && fresh.workspace == home.workspace);
    let (live_max, live_full_opt) = live_overlay_flags(member_key.hwnd, fulls);
    let live = directional_live(
        state,
        areas,
        &snapshot,
        member_key,
        membership_ok,
        live_max,
        live_full_opt.unwrap_or(subject_fullscreen),
    );
    if let Some(outcome) = crate::workspace_owner::directional_fences_hold(&snapshot, &live) {
        if outcome == "identity-changed" {
            drop_member_state(state, member_key);
        }
        log_snap(outcome, serde_json::json!({"window": from_token}));
        return outcome;
    }
    // Candidate Engine for the whole pair route: the focus-MRU sync, both
    // gap adopts, and the pair dispatch below all run on the clone. The
    // authoritative Engine publishes once, only after the verified tails
    // commit. A refused topology/write/readback/arrival/assignment (or a
    // refused focus fence) discards the candidate, so source membership and
    // retained Engine placement survive with no follow-focus and no stale
    // MRU mutation.
    let mut candidate = state.engine.clone();
    // Focus intents remember the live project MRU on the destination Engine
    // session before planning: external native focus moves advance the
    // project MRU without touching Engine remembered focus, and the plan
    // must aim at the fresh target rather than a stale eligible one. A
    // failed sync (unknown session/member) simply leaves the stale target
    // for the existing eligible-verify below; pair assembly refuses
    // honestly when the destination itself is unusable.
    if is_focus {
        let fresh_tokens: HashSet<String> = observed.iter().map(|w| w.token.clone()).collect();
        let target_members = state
            .workspaces
            .workspace_members(target_output, target_workspace);
        let eligible = state.workspaces.eligible_focus_set(
            &target_members,
            &state.member_tokens,
            &fresh_tokens,
        );
        if let Some(mru) = crate::workspace_owner::directional_focus_target(
            &state.workspaces,
            target_output,
            target_workspace,
            &eligible,
        ) && let Some(token) = state.member_tokens.get(&mru).cloned()
        {
            let _ = candidate.remember_domain_focus(&target_key, &WindowId(token));
        }
    }
    // Live gap edits adopt on both domains first so the pair convergence
    // never straddles gap generations.
    {
        let outer_gap = state.outer_gap;
        let mover = WindowId(from_token.to_owned());
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
            &mut candidate,
            state,
            &source_domain,
            &source_key,
            outer_gap,
            &source_windows,
            Some(&mover),
            revision_for(state, &loc.output, &loc.workspace),
            source_fp,
            correlation,
        );
        let target_windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = target_rows
            .iter()
            .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
            .collect();
        let target_fp = crate::tiling::fingerprint(
            &target_rows
                .iter()
                .map(|r| (r.token.clone(), r.rect))
                .collect::<Vec<_>>(),
        );
        adopt_gaps_on_candidate(
            &mut candidate,
            state,
            &target_domain,
            &target_key,
            outer_gap,
            &target_windows,
            None,
            revision_for(state, target_output, target_workspace),
            target_fp,
            correlation,
        );
    }
    // Combined two-domain observation: source rows homed on the source key,
    // target rows homed on the target key. Fingerprint covers both so stale
    // target rects cannot ride.
    let combined_fp = {
        let mut pairs: Vec<(String, Rect)> = source_rows
            .iter()
            .chain(target_rows.iter())
            .map(|r| (r.token.clone(), r.rect))
            .collect();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        crate::tiling::fingerprint(&pairs)
    };
    let from = WindowId(from_token.to_owned());
    let source_windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = source_rows
        .iter()
        .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
        .collect();
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        correlation,
        revision_for(state, &loc.output, &loc.workspace),
        combined_fp,
        &source_domain,
        &source_key,
        state.outer_gap,
        &source_windows,
        Some(&from),
    );
    for row in &target_rows {
        event.windows.push(tiler_core::seed::EngineWindow {
            window: WindowId(row.token.clone()),
            output: target_key.output.clone(),
            workspace: target_key.workspace.clone(),
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
        (source_domain.clone(), source_key.clone()),
        (target_domain.clone(), target_key.clone()),
    ]);
    event.directional_target_outer_gap = Some(state.outer_gap);
    let direction = crate::snapkey::direction_name(intent.direction).to_owned();
    event.command = match intent.op {
        crate::snapkey::SnapOp::Focus => CoreCommand::Focus {
            window: from.0.clone(),
            direction,
            cross_output_transfer: true,
            float_subject: false,
        },
        crate::snapkey::SnapOp::Move => CoreCommand::Move {
            window: from.0.clone(),
            direction,
            cross_output_transfer: true,
            same_axis_move: same_axis_move_for(state.settings_live.as_ref()),
        },
    };
    let reply = {
        stamp_engine_event(state, &mut event);
        candidate.handle(&event)
    };
    if is_focus {
        let CoreReply::FocusDirectional(plan) = &reply else {
            let outcome = reply_outcome(&reply);
            log_snap(
                outcome,
                serde_json::json!({"window": from.0, "continued": continued}),
            );
            return outcome;
        };
        if plan.cross_operation.is_none() {
            // Local won inside the pair: retain focus state only after the
            // local target is verified foreground, just like cross focus.
            let to = plan.to_window.0.clone();
            let actuation = actuate_focus(state, me, fulls, observed, retained, &to);
            let outcome = actuation.outcome;
            if outcome == "focus-ok" {
                state.engine = candidate;
                note_focus_advance(state, observed, retained, &to);
            }
            log_snap(
                outcome,
                serde_json::json!({
                    "window": from.0,
                    "continued": continued,
                    "setter_accepted": actuation.setter_accepted,
                    "prime_inserted": actuation.prime_inserted,
                    "attach_ok": actuation.attach_ok,
                    "eventual": actuation.eventual,
                    "deferred": actuation.deferred,
                }),
            );
            return outcome;
        }
        // Cross focus: no geometry or membership writes. The Engine plan
        // carries the authoritative remembered target; the adapter verifies
        // it names a known member that is still eligible-visible, then
        // actuates and syncs the project MRU to the Engine choice. A project
        // MRU that disagrees never silently vetoes the Engine target.
        let fresh_tokens: HashSet<String> = observed.iter().map(|w| w.token.clone()).collect();
        let target_members = state
            .workspaces
            .workspace_members(target_output, target_workspace);
        let eligible = state.workspaces.eligible_focus_set(
            &target_members,
            &state.member_tokens,
            &fresh_tokens,
        );
        let to_token = plan.to_window.0.clone();
        let Some(target_key) = state
            .member_tokens
            .iter()
            .find(|(_, token)| token.as_str() == to_token.as_str())
            .map(|(key, _)| key.clone())
        else {
            log_snap("unchanged", serde_json::json!({"window": from.0}));
            return "unchanged";
        };
        if !eligible.contains(&target_key) {
            log_snap("unchanged", serde_json::json!({"window": from.0}));
            return "unchanged";
        }
        // Fresh fences before the single actuation (no membership change is
        // involved, so no Engine/project disagreement risk).
        let membership_ok = state
            .workspaces
            .member_loc(member_key)
            .is_some_and(|fresh| fresh.output == home.output && fresh.workspace == home.workspace);
        let (live_max, live_full_opt) = live_overlay_flags(member_key.hwnd, fulls);
        let live = directional_live(
            state,
            areas,
            &snapshot,
            member_key,
            membership_ok,
            live_max,
            live_full_opt.unwrap_or(subject_fullscreen),
        );
        if let Some(outcome) = crate::workspace_owner::directional_fences_hold(&snapshot, &live) {
            if outcome == "identity-changed" {
                drop_member_state(state, member_key);
            }
            log_snap(outcome, serde_json::json!({"window": from.0}));
            return outcome;
        }
        if suspend_read(state, me, fulls).veto.block || foreground_elevated(me) {
            log_snap("focus-skipped-fence", serde_json::json!({"window": from.0}));
            return "focus-skipped-fence";
        }
        let actuation = actuate_focus(state, me, fulls, observed, retained, &to_token);
        let outcome = actuation.outcome;
        if outcome == "focus-ok" {
            // Commit the candidate (MRU sync plus Engine remembered focus)
            // only with the verified actuation: a refused fence or a failed
            // setter above already returned with the candidate discarded, so
            // no stale MRU mutation survives.
            state.engine = candidate;
            note_focus_advance(state, observed, retained, &to_token);
            state.workspaces.note_foreground(&target_key);
        }
        log_snap(
            outcome,
            serde_json::json!({
                "window": from.0,
                "continued": continued,
                "setter_accepted": actuation.setter_accepted,
                "prime_inserted": actuation.prime_inserted,
                "attach_ok": actuation.attach_ok,
                "eventual": actuation.eventual,
                "deferred": actuation.deferred,
                "cross_output": true,
            }),
        );
        return outcome;
    }
    let CoreReply::MoveDirectional(plan) = &reply else {
        // R4 capacity fallback: the exact wrap proved exhaustion, so core
        // stages the slotless float as `SendOutput`. No safe cross-output
        // remap exists, so this refuses with the source preserved; the
        // candidate is discarded, no Engine mutation survives.
        if matches!(&reply, CoreReply::SendOutput(_)) && reply_is_capacity_float_transfer(&reply) {
            log_snap(
                "float-no-remap",
                serde_json::json!({
                    "window": from.0,
                    "continued": continued,
                    "transfer": "capacity-float",
                    "target_output": state.workspaces.output_token(target_output),
                    "target_workspace": state.workspaces.workspace_token(target_output, target_workspace),
                }),
            );
            return "float-no-remap";
        }
        let outcome = reply_outcome(&reply);
        log_snap(
            outcome,
            serde_json::json!({"window": from.0, "continued": continued}),
        );
        return outcome;
    };
    if plan.cross.is_none() {
        // Local won inside the pair: publish the candidate (same commit the
        // direct dispatch made) then apply source geometry only.
        state.engine = candidate;
        let writable = writable_tokens(state, &loc.output, &loc.workspace, observed);
        let summary = apply_geometry(
            state,
            ApplyInput {
                me,
                fulls,
                reply: &reply,
                observed,
                op: "move",
                tick,
                correlation: correlation_str,
                skipped,
                writable: &writable,
                output_token: state.workspaces.output_token(&loc.output),
                workspace_token: state
                    .workspaces
                    .workspace_token(&loc.output, &loc.workspace),
                revision: revision_for(state, &loc.output, &loc.workspace),
            },
        );
        let outcome = match summary {
            Some(s) if !s.readback_ok => "move-unverified",
            Some(s) if s.mismatched > 0 => "move-mismatch",
            Some(s) if s.applied > 0 => "move-applied",
            Some(_) => "move-noop",
            None => reply_outcome(&reply),
        };
        log_snap(
            outcome,
            serde_json::json!({"window": from.0, "continued": continued}),
        );
        return outcome;
    }
    // Cross move: the pair plan sits on the discarded-on-refusal candidate,
    // so the project membership commits only after the fresh topology, the
    // single combined apply, its readback, and the arrival proof all verify.
    // The authoritative Engine publishes with that same commit, exactly once.
    // Per-window write gates (lifetime/scope/pid/veto) still run inside the
    // single apply below; a refusal keeps source membership and retained
    // Engine placement with no follow-focus and no replay.
    let fresh_snapshot = crate::workspace_owner::drag_snapshot_for_directional(&snapshot);
    let Some(fresh_areas) = fresh_cross_areas() else {
        log_snap("move-unverified", serde_json::json!({"window": from.0}));
        let fulls_owned = fulls.to_vec();
        reconcile_tick(state, me, &fulls_owned, areas);
        return "move-unverified";
    };
    if !cross_topology_fresh(&fresh_snapshot, &fresh_areas) {
        log_snap("move-unverified", serde_json::json!({"window": from.0}));
        let fresh_fulls: Vec<Rect> = fresh_areas.iter().map(|a| a.full).collect();
        reconcile_tick(state, me, &fresh_fulls, &fresh_areas);
        return "move-unverified";
    }
    let fresh_fulls: Vec<Rect> = fresh_areas.iter().map(|a| a.full).collect();
    // One union-writable combined apply: source survivors reflow and the
    // target places the arrival together, in DPI-aware physical coordinates
    // with no rescaling. A single pass means a single internal
    // observe/publish, never two mid-operation publishes. The union is read
    // pre-commit (the mover rides the source set while homed on the source);
    // per-window freshness gates inside the apply still decide every write.
    let mut union_writable = writable_tokens(state, &loc.output, &loc.workspace, observed);
    union_writable.extend(writable_tokens(
        state,
        target_output,
        target_workspace,
        observed,
    ));
    let summary = apply_geometry(
        state,
        ApplyInput {
            me,
            fulls: &fresh_fulls,
            reply: &reply,
            observed,
            op: "move",
            tick,
            correlation: correlation_str,
            skipped,
            writable: &union_writable,
            output_token: state.workspaces.output_token(&loc.output),
            workspace_token: state
                .workspaces
                .workspace_token(&loc.output, &loc.workspace),
            revision: revision_for(state, &loc.output, &loc.workspace),
        },
    );
    let Some(summary) = summary else {
        log_snap("move-unverified", serde_json::json!({"window": from.0}));
        return "move-unverified";
    };
    if !summary.readback_ok {
        log_snap("move-unverified", serde_json::json!({"window": from.0}));
        return "move-unverified";
    }
    if summary.mismatched > 0 {
        log_snap("move-mismatch", serde_json::json!({"window": from.0}));
        return "move-mismatch";
    }
    // Required arrival proof before claiming success: the mover must read
    // back at its planned rect on the destination monitor. A failed proof
    // reports unverified with no commit, no revert, and no replay; the next
    // observation converges.
    if !directional_arrival_verified(
        state,
        &fresh_areas,
        &reply,
        &from.0,
        member_key.hwnd,
        target_output,
    ) {
        log_snap("move-unverified", serde_json::json!({"window": from.0}));
        return "move-unverified";
    }
    // Verified: commit both authorities once, then follow focus.
    if !state
        .workspaces
        .assign(member_key.clone(), target_output, target_workspace, false)
    {
        log_snap("unverified", serde_json::json!({"window": from.0}));
        return "unverified";
    }
    let source_now = state
        .workspaces
        .workspace_members(&loc.output, &loc.workspace);
    let target_now = state
        .workspaces
        .workspace_members(target_output, target_workspace);
    if !crate::workspace_owner::verify_directional_transfer(member_key, &source_now, &target_now) {
        log_snap("unverified", serde_json::json!({"window": from.0}));
        return "unverified";
    }
    state.engine = candidate;
    // Follow focus without visibility hiding: the arrival is already in the
    // destination current view, so only foreground the mover.
    let actuation = actuate_focus(state, me, &fresh_fulls, observed, retained, &from.0);
    if actuation.outcome != "focus-ok" {
        log_snap(
            actuation.outcome,
            serde_json::json!({
                "window": from.0,
                "continued": continued,
                "setter_accepted": actuation.setter_accepted,
                "prime_inserted": actuation.prime_inserted,
                "attach_ok": actuation.attach_ok,
                "eventual": actuation.eventual,
                "deferred": actuation.deferred,
                "cross_output": true,
            }),
        );
        return actuation.outcome;
    }
    note_focus_advance(state, observed, retained, &from.0);
    state.workspaces.note_foreground(member_key);
    // Fresh arrival: mover present on target, absent on source, and the
    // Engine plan verified structurally inside `handle_engine_event`.
    let source_now = state
        .workspaces
        .workspace_members(&loc.output, &loc.workspace);
    let target_now = state
        .workspaces
        .workspace_members(target_output, target_workspace);
    if !crate::workspace_owner::verify_directional_transfer(member_key, &source_now, &target_now) {
        log_snap("unverified", serde_json::json!({"window": from.0}));
        return "unverified";
    }
    log_snap(
        "move-applied",
        serde_json::json!({
            "window": from.0,
            "continued": continued,
            "setter_accepted": actuation.setter_accepted,
            "prime_inserted": actuation.prime_inserted,
            "attach_ok": actuation.attach_ok,
            "eventual": actuation.eventual,
            "deferred": actuation.deferred,
            "cross_output": true,
        }),
    );
    "move-applied"
}
