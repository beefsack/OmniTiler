use super::*;

/// REQ-WS-09 departure memory for an explicit workspace select: the
/// chord-time origin still holds live foreground at dispatch, so the source
/// workspace remembers it before hiding. Exact member key (HWND/PID/creation)
/// plus source membership plus stay-mirrored liveness (fresh observed match,
/// else a verified retained maximized/fullscreen row). Returns the remembered
/// opaque token for one bounded log line; no windows, geometry, or
/// suppression change.
pub(super) fn remember_select_departure(
    state: &mut TileLoop,
    origin: &crate::snapkey::SnapOrigin,
    output: &str,
    source_id: &str,
    foreground_hwnd: u64,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
) -> Option<String> {
    if origin.token.is_empty() || foreground_hwnd != origin.hwnd {
        return None;
    }
    let key = state
        .member_tokens
        .iter()
        .find(|(_, token)| token.as_str() == origin.token.as_str())
        .map(|(key, _)| key.clone())?;
    if !crate::workspace_owner::member_matches(&key, origin.hwnd, origin.pid, &origin.creation) {
        return None;
    }
    let in_source = state
        .workspaces
        .member_loc(&key)
        .is_some_and(|loc| loc.output == output && loc.workspace == source_id);
    if !in_source {
        return None;
    }
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let live = match by_hwnd.get(&key.hwnd) {
        Some(fresh) => {
            fresh.token == origin.token
                && crate::workspace_owner::member_matches(
                    &key,
                    fresh.hwnd,
                    fresh.identity.pid,
                    &fresh.identity.process_creation,
                )
        }
        None => retained
            .iter()
            .any(|row| row.key == key && (row.maximized || row.fullscreen)),
    };
    if !live {
        return None;
    }
    state.workspaces.note_foreground(&key);
    Some(origin.token.clone())
}

/// Switch the visible set from the output's active workspace to `target`:
/// hide the prior set, reveal the target set, preserve Engine sessions for
/// both, prune trailing empties, then re-enumerate fresh, establish the
/// appropriate target focus BEFORE geometry (with fullscreen/elevated fences
/// retained so a real arrival is never stolen from), then reconcile the
/// selected domain geometry with verified readback. Empty targets hide the
/// prior set and take no focus.
/// The pre-switch `observed` serves only the hide phase; focus never uses it
/// because the hidden target set is absent from it by construction.
/// `ctx` carries the stable action correlation even when inner ticks
/// increment; `focus_hint` (send-follow mover) wins when eligible so the
/// caller can reuse the fresh focus without a redundant enumeration.
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace_do_select(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &mut [ObservedWindow],
    output: &str,
    target: &str,
    ctx: &ActionCtx,
    focus_hint: Option<&crate::workspace::WindowKey>,
    suppress_focus: bool,
    skip_geometry: bool,
) -> SelectEffect {
    let none =
        |outcome: &'static str, source_workspace: String, target_workspace: String| SelectEffect {
            outcome,
            geometry: None,
            focus: "none",
            transition_ms: 0,
            observation_ms: 0,
            geometry_ms: 0,
            hide_ms: 0,
            reveal_ms: 0,
            focus_ms: 0,
            plan_ms: 0,
            source_workspace,
            target_workspace,
        };
    let Some(current) = state.workspaces.active_id(output) else {
        return none("unknown-output", "ws?".to_owned(), "ws?".to_owned());
    };
    if current == target {
        state.active_output = output.to_owned();
        let token = state.workspaces.workspace_token(output, target);
        return none("already-active", token.clone(), token);
    }
    let source_token = state.workspaces.workspace_token(output, &current);
    let target_token = state.workspaces.workspace_token(output, target);
    // Item 12 stage B scoped effects: shared selections converge every
    // live output onto the logical id and global-unique targets visible
    // elsewhere swap the two views (native transfer plus history on both
    // outputs). All other targets run the ordinary single-output path below.
    {
        let adopted = state.scope_owner.adopted();
        if adopted == tiler_core::workspace_scope::WorkspaceMode::Shared {
            if let Some(plan) = state
                .scope_owner
                .plan_shared_select(&state.workspaces, target)
            {
                return scope_effects::shared_select(
                    state,
                    me,
                    store,
                    dir,
                    fulls,
                    areas,
                    observed,
                    output,
                    &plan,
                    ctx,
                    focus_hint,
                    suppress_focus,
                    skip_geometry,
                );
            }
        } else if adopted == tiler_core::workspace_scope::WorkspaceMode::GlobalUnique
            && let Some(plan) = state
                .scope_owner
                .swap_plan_for(&state.workspaces, output, target)
        {
            return scope_effects::global_swap_select(
                state,
                me,
                store,
                dir,
                fulls,
                areas,
                observed,
                output,
                &plan,
                ctx,
                focus_hint,
                suppress_focus,
                skip_geometry,
            );
        }
    }
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let leaving = state.workspaces.workspace_members(output, &current);
    let mut hide_failed = false;
    // Windows newly committed to hidden in this transition: on any failure
    // before activation these are safely revealed back (exact identity) so a
    // partial never leaves a doubled visible set or a lost claim behind.
    let mut newly_hidden: Vec<crate::workspace::WindowKey> = Vec::new();
    let hide_start = Instant::now();
    let mut hide_diag_logged: usize = 0;
    let mut hide_attempted: usize = 0;
    for key in &leaving {
        if state.workspaces.is_hidden(key) {
            continue;
        }
        let Some(expected) = state.member_identity.get(key).cloned() else {
            continue;
        };
        // Explicit scope fences every hide: out-of-scope members stay
        // visible and keep membership, never hidden.
        if !scope_allows(&state.scope, &expected.exe_path) {
            continue;
        }
        // Bind the hide to the stored full member identity: a recycled HWND
        // (same number, fresh process) reconciles membership with no writes.
        // Members absent from the tiled observation hide through a fresh
        // identity-only read so retained minimized/maximized/fullscreen
        // members still leave the visible set. Sticky liveness verifies here
        // too: a dead sticky drops instead of hiding, a live one skips below.
        let (live_ok, iconic) = match by_hwnd.get(&key.hwnd) {
            Some(window) => {
                let live_ok = crate::workspace_owner::member_matches(
                    key,
                    window.hwnd,
                    window.identity.pid,
                    &window.identity.process_creation,
                ) && window.identity.exe_path == expected.exe_path
                    && window.identity.user_sid == expected.user_sid
                    && window.identity.session_id == expected.session_id;
                (live_ok, window.facts.minimized)
            }
            None => match crate::product_hide::sys::query_recovery(key.hwnd, me) {
                Ok(snap) => {
                    let live_ok = snap.pid == expected.pid
                        && snap.process == expected
                        && snap.nonce.is_none();
                    (live_ok, snap.iconic)
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.starts_with("absent:") || msg.contains("sid/session mismatch") {
                        drop_member_state(state, key);
                    } else {
                        hide_failed = true;
                    }
                    continue;
                }
            },
        };
        if !live_ok {
            // Recycled HWND: drop the stale membership, never touch the new
            // owner, and retire any same-HWND claim residue without writes.
            drop_member_state(state, key);
            continue;
        }
        // Verified-live sticky members never hide: they stay visible through
        // workspace selects with no `SW_HIDE`, no claim, no backing occupancy.
        if state.sticky.contains_key(key) {
            continue;
        }
        let had_claim = state.hidden_claims.contains_key(key);
        let project_topmost = crate::product_hide::project_topmost_hide_allowed(
            state.float_topmost_prev.get(key).copied(),
            read_topmost_now(key.hwnd),
        );
        let (outcome, detail) = workspace_hide_one(state, me, store, dir, key, &expected, iconic);
        hide_attempted += 1;
        note_hide_diag(
            state,
            ctx,
            key,
            outcome,
            detail,
            project_topmost,
            &mut hide_diag_logged,
        );
        // A committed claim (exact full identity plus tag) is owned even when
        // the post-hide readback reports uncertain: the ledger and the owner
        // table keep it, never a lost window.
        if state.hidden_claims.contains_key(key) || outcome == "hidden" {
            state.workspaces.set_hidden(key, true);
            if !had_claim {
                newly_hidden.push(key.clone());
            }
        } else if outcome == "origin-vanished" || outcome == "identity-changed" {
            drop_member_state(state, key);
        } else if outcome == "scope-excluded" {
            // Explicit scope fence: out-of-scope members stay visible with
            // membership intact, never hidden, never a failure.
        } else {
            hide_failed = true;
        }
    }
    // A failed hide phase never activates: revealing the target beside a
    // still-visible current set would double the visible set and lose claims.
    // Narrow recovery reveals exactly the claims committed above (exact
    // identity, no blind replay, no fabricated rollback) and keeps current.
    let hide_ms = hide_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    if let Some(line) = omission_summary(
        "workspace-hide-summary",
        state.tick,
        &ctx.correlation,
        None,
        hide_attempted,
        hide_diag_logged,
    ) {
        log_json_at(&state.log_path.clone(), line);
    }
    if hide_failed {
        rollback_newly_hidden(state, me, store, &newly_hidden, ctx);
        return SelectEffect {
            outcome: "partial",
            hide_ms,
            source_workspace: source_token.clone(),
            target_workspace: target_token.clone(),
            ..none("partial", source_token.clone(), target_token.clone())
        };
    }
    let entering = state.workspaces.workspace_members(output, target);
    let reveal_start = Instant::now();
    let reveal_attempted = entering
        .iter()
        .filter(|key| state.hidden_claims.contains_key(key))
        .count();
    let mut reveal_diag_logged: usize = 0;
    for key in &entering {
        let Some(record) = state.hidden_claims.get(key).cloned() else {
            state.workspaces.set_hidden(key, false);
            continue;
        };
        if let Err(cause) = workspace_proof_reveal_gate(state, me, key.hwnd) {
            note_reveal_diag(
                state,
                ctx,
                key,
                "refused",
                Some(cause),
                None,
                &mut reveal_diag_logged,
            );
            hide_failed = true;
            continue;
        }
        match crate::product_hide::sys::reveal_product_claim_to(
            store,
            me,
            &record.claim,
            record.iconic,
        ) {
            Ok(teardown) => {
                let outcome = reveal_outcome(&teardown);
                note_reveal_diag(
                    state,
                    ctx,
                    key,
                    outcome,
                    None,
                    None,
                    &mut reveal_diag_logged,
                );
                if matches!(teardown, crate::product_hide::ProductTeardown::Retired) {
                    state.hidden_claims.remove(key);
                    if let Some(token) = state.member_tokens.remove(key) {
                        state.member_rects.remove(&token);
                        state.float_rects.remove(&token);
                        state.hint_logged.remove(&token);
                        state.hint_fallback_uses.remove(&token);
                        drop_diag_sample(state, &token);
                        drop_visibility_sample(state, &token);
                    }
                    if state.restore_wake.as_ref().is_some_and(|w| w.key == *key) {
                        state.restore_wake = None;
                    }
                    state.float_topmost_prev.remove(key);
                    state.floated.remove(key);
                    state.member_identity.remove(key);
                    state.member_tags.remove(key);
                    state.hint_last_good.remove(key);
                    state.workspaces.remove_window(key);
                } else {
                    state.hidden_claims.remove(key);
                    state.workspaces.set_hidden(key, false);
                }
            }
            Err(e) => {
                let (outcome, detail) = reveal_err_outcome(&e.to_string());
                note_reveal_diag(
                    state,
                    ctx,
                    key,
                    outcome,
                    Some(detail),
                    None,
                    &mut reveal_diag_logged,
                );
                hide_failed = true;
            }
        }
    }
    if let Some(line) = omission_summary(
        "workspace-reveal-summary",
        state.tick,
        &ctx.correlation,
        None,
        reveal_attempted,
        reveal_diag_logged,
    ) {
        log_json_at(&state.log_path.clone(), line);
    }
    // A failed reveal phase keeps current as well: the leaving set was
    // hidden successfully, so the safest known view is restored by revealing
    // exactly those affected claims (exact identity only). The switch is not
    // pretended: the outcome stays partial with current kept.
    let reveal_ms = reveal_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    if hide_failed {
        rollback_newly_hidden(state, me, store, &newly_hidden, ctx);
        return SelectEffect {
            outcome: "partial",
            hide_ms,
            reveal_ms,
            source_workspace: source_token.clone(),
            target_workspace: target_token.clone(),
            ..none("partial", source_token.clone(), target_token.clone())
        };
    }
    // Move active to the target id without disturbing order, including
    // appended trailing ids past the Win1..9 ordinal range. This is the only
    // activation point: callers resolve targets without preactivating so the
    // `current == target` guard above stays accurate. Reached only after the
    // hide and reveal effects verified, so the switch is real.
    state.workspaces.activate(output, target);
    state.active_output = output.to_owned();
    // Item 1 history: record the actual completed view transition here,
    // including when later focus/geometry fails below. View evidence (the
    // verified hide/reveal above), not the final action success label,
    // controls history. Same-view activation returned early above and never
    // records; failed hide/reveal returned partial before activation.
    // Item 12 stage B: scoped recording (local into workspaces,
    // global-unique/shared into the scope owner); focus-alone never records.
    {
        let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
        owner.record_completed(spaces, output, target);
    }
    // Trailing maintenance through the shared scope contract (item 12
    // stage B): local keeps the exact legacy per-output sequence, scoped
    // modes converge per-output plans / the one logical ring.
    scope_trailing::trailing_after_select(state, output);
    // Post-reveal fresh observation: the pre-switch `observed` cannot contain
    // the hidden target set, so focusing from it always misses as `vanished`.
    // Re-enumerate, republish origins, establish the appropriate target focus
    // BEFORE geometry (fenced), then reconcile the selected domain geometry
    // with verified readback. Empty targets hide the prior set and take no
    // focus.
    let transition_ms = ctx.start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let observation_start = Instant::now();
    let mut fresh_skipped: Vec<(String, String)> = Vec::new();
    let mut fresh_retained: Vec<RetainedRow> = Vec::new();
    let Some(mut fresh_observed) =
        state.observe(me, fulls, &mut fresh_skipped, &mut fresh_retained)
    else {
        let observation_ms = observation_start
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        return SelectEffect {
            outcome: "observation-failed",
            transition_ms,
            observation_ms,
            hide_ms,
            reveal_ms,
            source_workspace: source_token.clone(),
            target_workspace: target_token.clone(),
            ..none(
                "observation-failed",
                source_token.clone(),
                target_token.clone(),
            )
        };
    };
    let observation_ms = observation_start
        .elapsed()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    publish_managed(state, me, &fresh_observed, &fresh_retained);
    ensure_workspace_assignments(state, me, &mut fresh_observed, &fresh_retained, areas);
    clear_maximize_at_admission(state, me, &fresh_retained);
    workspace_close_cleanup(state);
    // Focus-before-geometry on the verified transition: the revealed target
    // must hold foreground before writes so a transient foreground cannot
    // veto eligible writes on the next tick. All fences retained: a real
    // fullscreen or elevated arrival is never stolen from, and identity,
    // lifetime, scope, and observation gates inside `actuate_focus` still
    // apply. Skipping focus there lets geometry veto honestly.
    let mut focus_outcome: &'static str = "no-focus";
    let mut focus_window: Option<String> = None;
    // Focus duration covers the bounded pumped settle when it runs, so a
    // dominating 500 ms settle attributes to focus, not geometry.
    let mut focus_ms: u64 = 0;
    if suppress_focus {
        // No setter, no priming, no MRU fallback actuation: compare the fresh
        // foreground against the mover hint only. Native reveal may have
        // focused the mover; anything else stays explicitly suppressed.
        let foreground = unsafe { GetForegroundWindow() } as usize as u64;
        let observed = focus_hint.is_some_and(|hint| hint.hwnd != 0 && foreground == hint.hwnd);
        if observed {
            focus_outcome = "focus-ok";
            if let Some(hint) = focus_hint {
                focus_window = state.member_tokens.get(hint).cloned();
                state.workspaces.note_foreground(hint);
            }
        } else {
            focus_outcome = "focus-suppressed";
        }
    } else {
        // Retained-aware focus set: maximized and fullscreen movers are
        // retained, never eligible-observed, so their tokens ride along for focus
        // only (geometry still excludes them via `writable_tokens`; focus carries
        // no write, KDE `requestFocus` parity). The foreground fence below still
        // never steals from a real arrival.
        let members = state.workspaces.workspace_members(output, target);
        let mut fresh_tokens: HashSet<String> =
            fresh_observed.iter().map(|w| w.token.clone()).collect();
        for row in &fresh_retained {
            if row.maximized || row.fullscreen {
                fresh_tokens.insert(row.token.clone());
            }
        }
        let eligible =
            state
                .workspaces
                .eligible_focus_set(&members, &state.member_tokens, &fresh_tokens);
        let focus_key = focus_hint
            .filter(|hint| eligible.contains(*hint))
            .cloned()
            .or_else(|| state.workspaces.focus_target(output, target, &eligible));
        if let Some(ref focus_key) = focus_key {
            let token = state
                .member_tokens
                .get(focus_key)
                .cloned()
                .unwrap_or_default();
            if !token.is_empty() {
                focus_window = Some(token.clone());
                if crate::workspace_owner::focus_before_geometry(
                    true,
                    suspend_read(state, me, fulls).veto.block,
                    foreground_elevated(me),
                ) {
                    let focus_start = Instant::now();
                    let actuation =
                        actuate_focus(state, me, fulls, &fresh_observed, &fresh_retained, &token);
                    focus_ms = focus_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
                    focus_outcome = actuation.outcome;
                    if actuation.outcome == "focus-ok" {
                        state.workspaces.note_foreground(focus_key);
                    }
                } else {
                    // Fenced: a real fullscreen/elevated foreground arrived
                    // during the transition. Never steal; geometry below vetoes
                    // honestly instead of riding along.
                    focus_outcome = "focus-skipped-fence";
                }
            }
        }
    }
    // Bounded focus reconstruction for the next trace: opaque chosen token
    // (null when none) plus outcome under the action correlation.
    log_json_at(
        &state.log_path.clone(),
        serde_json::json!({"event":"select-focus","correlation":ctx.correlation,"window":focus_window,"focus":focus_outcome}),
    );
    let mut geometry: Option<ApplySummary> = None;
    // One shared hint-query budget for the select's row assembly. Floating
    // targets take no geometry: hide/reveal and focus above already ran, and
    // frames stay untouched.
    let mut hint_cx = HintCx::new();
    if workspace_mode_tiled(state, output, target)
        && let Some((domain, domain_key)) =
            workspace_domain_for(output, target, areas, state.inner_gap, state.outer_gap)
        && let Some(rows) = assemble_domain_rows(
            state,
            output,
            target,
            &fresh_observed,
            &fresh_retained,
            "select",
            ctx.correlation.as_str(),
            &mut hint_cx,
        )
    {
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
        // Foreground read after the focus above, so the Engine sees the
        // established target focus when eligible.
        let focused = state
            .focused_token(&fresh_observed)
            .filter(|f| windows.iter().any(|(w, _, _, _)| w == f));
        state.tick += 1;
        let tick = state.tick;
        let correlation =
            CorrelationId::parse(&ctx.correlation).expect("action correlation is a valid token");
        // Live gap edits adopt here so the select convergence below sees
        // matching gaps instead of refusing.
        {
            let revision = revision_for(state, output, target);
            let outer_gap = state.outer_gap;
            adopt_gaps_for_route(
                state,
                &domain,
                &domain_key,
                outer_gap,
                &windows,
                focused.as_ref(),
                revision,
                fp,
                &correlation,
            );
        }
        let plan_start = Instant::now();
        let mut event = crate::tiling::build_reconcile_event_for_floating(
            &state.owner,
            &state.generation,
            &correlation,
            revision_for(state, output, target),
            fp,
            &domain,
            &domain_key,
            state.outer_gap,
            &windows,
            focused.as_ref(),
        );
        let reply = handle_engine_event(state, &mut event);
        let plan_ms = plan_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        // D16 deferred follow: membership-only even with drifted survivor
        // frames. Observation, Engine reconcile, reveal, focus, and fences
        // above all still run; only the native geometry application is
        // skipped, so neither tree takes a write.
        let geometry_ms = if skip_geometry {
            0
        } else {
            let writable = writable_tokens(state, output, target, &fresh_observed);
            let geometry_start = Instant::now();
            let summary = apply_geometry(
                state,
                ApplyInput {
                    me,
                    fulls,
                    reply: &reply,
                    observed: &fresh_observed,
                    op: "reconcile",
                    tick,
                    correlation: ctx.correlation.as_str(),
                    skipped: fresh_skipped,
                    writable: &writable,
                    output_token: state.workspaces.output_token(output),
                    workspace_token: state.workspaces.workspace_token(output, target),
                    revision: revision_for(state, output, target),
                },
            );
            let ms = geometry_start
                .elapsed()
                .as_millis()
                .min(u128::from(u64::MAX)) as u64;
            geometry = summary;
            ms
        };
        return SelectEffect {
            outcome: "ok",
            geometry,
            focus: focus_outcome,
            transition_ms,
            observation_ms,
            geometry_ms,
            hide_ms,
            reveal_ms,
            focus_ms,
            plan_ms,
            source_workspace: source_token.clone(),
            target_workspace: target_token.clone(),
        };
    }
    SelectEffect {
        outcome: "ok",
        geometry,
        focus: focus_outcome,
        transition_ms,
        observation_ms,
        geometry_ms: 0,
        hide_ms,
        reveal_ms,
        focus_ms,
        plan_ms: 0,
        source_workspace: source_token.clone(),
        target_workspace: target_token.clone(),
    }
}
