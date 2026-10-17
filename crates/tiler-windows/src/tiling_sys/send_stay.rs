#![forbid(unsafe_code)]
use super::*;

/// Ordinary trailing lifecycle after a stay send filled the trailing empty:
/// keep one empty with a minimum of two, preserving the active view. Runs
/// without activation or selection, so no history records; removal
/// invalidation still applies (a removed previous clears like any cleanup).
/// Shared by the Engine and native stay tails.
pub(super) fn ensure_trailing_spare(state: &mut TileLoop, output: &str) {
    let active_id = state.workspaces.active_id(output).unwrap_or_default();
    let displaced: Vec<String> = state
        .workspaces
        .displaced_snapshot()
        .values()
        .flat_map(|r| r.workspace_ids.clone())
        .collect();
    let sticky_keys: BTreeSet<crate::workspace::WindowKey> = state.sticky.keys().cloned().collect();
    let (removed, append) = state.workspaces.plan_cleanup_excluding(
        output,
        std::slice::from_ref(&active_id),
        &displaced,
        &sticky_keys,
    );
    state.workspaces.apply_cleanup(output, &removed, append);
}

/// Source-bound stay focus resolution from the Engine plan (item 2): the
/// plan's `focus_domain`/`focus_leaf` names the tiled layout focus after a
/// stay send (source focused-removal MRU, null when no source tiled entry
/// remains). Gathers the Engine snapshot mapping plus exact source/member
/// lifetime and fresh focus eligibility, then routes through
/// [`crate::workspace_owner::classify_stay_focus`]: `Null` is a genuine null
/// plan (no setter; the native removal focus stands, never a guessed
/// fallback), `Stale` is a some-but-unusable mapping (no setter either), and
/// `Ready` carries the MRU token for the single actuation attempt.
fn stay_focus_token(
    state: &TileLoop,
    output: &str,
    source_id: &str,
    plan: &tiler_core::boundary::SendWorkspacePlan,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
) -> (crate::workspace_owner::StayFocusClass, Option<String>) {
    use crate::workspace_owner::{StayFocusClass, classify_stay_focus};
    let plan_null = plan.focus_domain.is_none() || plan.focus_leaf.is_none();
    // Single gather pass (no early returns): every classifier input is real
    // state, so the decision table below stays causal.
    let mut mapped_key: Option<crate::workspace::WindowKey> = None;
    let mut mapped_window: Option<String> = None;
    if !plan_null {
        let focus_domain = plan.focus_domain.clone().expect("plan focus domain");
        let focus_leaf = plan.focus_leaf.clone().expect("plan focus leaf");
        if focus_domain.output.0 == output
            && focus_domain.workspace.0 == source_id
            && let Some(session) = state.engine.session(&focus_domain)
        {
            let snapshot = session.snapshot();
            if let Some(link) = snapshot.windows.iter().find(|link| {
                link.leaf == focus_leaf
                    && link.output == focus_domain.output
                    && link.workspace == focus_domain.workspace
            }) {
                let window = link.window.0.clone();
                if let Some(key) = state
                    .member_tokens
                    .iter()
                    .find(|(_, token)| token.as_str() == window.as_str())
                    .map(|(key, _)| key.clone())
                {
                    mapped_key = Some(key);
                    mapped_window = Some(window);
                }
            }
        }
    }
    let source_members = state.workspaces.workspace_members(output, source_id);
    let in_source = mapped_key
        .as_ref()
        .is_some_and(|key| source_members.contains(key));
    let hidden = mapped_key
        .as_ref()
        .is_some_and(|key| state.workspaces.is_hidden(key));
    // Exact member lifetime: visible MRU matches its fresh observation plus
    // the stored tag; a retained-overlay MRU (maximized/fullscreen) matches
    // its exact session key and rides the actuation's own revalidation.
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let lifetime_ok = match mapped_key.as_ref() {
        Some(key) => match by_hwnd.get(&key.hwnd) {
            Some(fresh) => {
                crate::workspace_owner::member_matches(
                    key,
                    fresh.hwnd,
                    fresh.identity.pid,
                    &fresh.identity.process_creation,
                ) && {
                    let live_tag = crate::product_hide::sys::read_member_tag(key.hwnd);
                    state.member_tags.get(key).is_some_and(|tag| {
                        crate::workspace_owner::visible_lifetime_ok(tag, live_tag.as_deref())
                    })
                }
            }
            None => retained
                .iter()
                .any(|row| row.key == *key && (row.maximized || row.fullscreen)),
        },
        None => false,
    };
    // Fresh focus eligibility mirrors the select path: revealed target focus
    // set over fresh tokens plus verified retained overlay tokens (focus
    // carries no write, KDE `requestFocus` parity).
    let mut fresh_tokens: HashSet<String> = observed.iter().map(|w| w.token.clone()).collect();
    for row in retained {
        if row.maximized || row.fullscreen {
            fresh_tokens.insert(row.token.clone());
        }
    }
    let eligible = mapped_key.as_ref().is_some_and(|key| {
        state
            .workspaces
            .eligible_focus_set(&source_members, &state.member_tokens, &fresh_tokens)
            .contains(key)
    });
    let class = classify_stay_focus(
        plan_null,
        mapped_key.is_some(),
        in_source,
        hidden,
        lifetime_ok,
        eligible,
    );
    match (class, mapped_window) {
        (StayFocusClass::Ready, Some(window)) => (class, Some(window)),
        (StayFocusClass::Null, _) => (StayFocusClass::Null, None),
        _ => (StayFocusClass::Stale, None),
    }
}

/// Engine stay tail for an explicit send-and-stay: the Engine mutation is
/// committed and membership verified by the caller. Fences
/// (lifetime/view/mode/gaps/source-selected) before writes AND focus, then
/// reflows only writable source rows, hides the mover without selecting or
/// revealing the target (hidden target takes no writes), keeps the source
/// selected/visible, applies the source-bound plan focus (MRU; no setter for
/// null), and runs ordinary trailing lifecycle for the next spare. History
/// never records (only verified selects record). Destination admission and
/// structural geometry are identical to follow: only focus and visibility
/// differ. Partial progress never replays: a fenced refusal reports without
/// further setters, and a failed focus reports without retry. When
/// `deferred` is set (D16 slotless deferred carry), the source-only geometry
/// application is skipped entirely: membership-only with zero geometry or
/// overlay writes on either tree, while fences, hide, trailing lifecycle,
/// and the source-bound plan focus stay unchanged.
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace_send_stay(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    observed: &mut [ObservedWindow],
    retained: &[RetainedRow],
    mover_key: &crate::workspace::WindowKey,
    stored: &ProcessIdentity,
    mover_minimized: bool,
    mover_hwnd: u64,
    output: &str,
    source_id: &str,
    target_id: &str,
    source_token: &str,
    target_token: &str,
    ctx: &ActionCtx,
    plan: &tiler_core::boundary::SendWorkspacePlan,
    preflight: SendPreflight,
    source_plan_ms: u64,
    deferred: bool,
) -> SendEffect {
    let fail_at = |outcome: &'static str| SendEffect {
        outcome,
        focus: "none",
        source: None,
        target: None,
        transition_ms: 0,
        observation_ms: 0,
        source_geometry_ms: 0,
        target_geometry_ms: 0,
        hide_ms: 0,
        reveal_ms: 0,
        focus_ms: 0,
        source_plan_ms,
        target_plan_ms: 0,
        source_workspace: source_token.to_owned(),
        target_workspace: target_token.to_owned(),
    };
    // Carried pre-plan fences before source writes: view/mode/gaps/scope/
    // lifetime/membership/overlay re-verified live (item 2 + item 20). Any
    // drift refuses with no writes and no focus attempt (the committed
    // membership stands like the follow hide-failure path; convergence
    // without replay, never a guessed rollback). A lifetime refusal drops
    // the stale membership like the dispatch-time gate.
    if let Some(outcome) = crate::workspace_owner::send_fences_hold(
        preflight.fences,
        preflight.live(
            state, stored, mover_key, mover_hwnd, output, source_id, target_id, fulls,
        ),
    ) {
        if outcome == "identity-changed" {
            drop_member_state(state, mover_key);
        }
        return fail_at(outcome);
    }
    // Source reflow while survivors are still visible and eligible, scoped by
    // the source writable set exactly like follow (mover transferred out,
    // hidden/retained rows never writable): target entries skip honestly as
    // retained, and hidden geometry is never written. Deferred slotless
    // transfers skip this entirely (membership-only).
    let (source, source_ms) = if deferred {
        (None, 0)
    } else {
        let source_writable = writable_tokens(state, output, source_id, observed);
        state.tick += 1;
        let source_tick = state.tick;
        let source_start = Instant::now();
        let source = apply_geometry(
            state,
            ApplyInput {
                me,
                fulls,
                reply: &tiler_core::boundary::CoreReply::SendWorkspace(plan.clone()),
                observed,
                op: "send-source-stay",
                tick: source_tick,
                correlation: ctx.correlation.as_str(),
                skipped: Vec::new(),
                writable: &source_writable,
                output_token: state.workspaces.output_token(output),
                workspace_token: state.workspaces.workspace_token(output, source_id),
                revision: revision_for(state, output, source_id),
            },
        );
        let source_ms = source_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        (source, source_ms)
    };
    // Carried fences again before hide (mover still visible: the tag read is
    // exact). Partial progress never replays.
    if let Some(outcome) = crate::workspace_owner::send_fences_hold(
        preflight.fences,
        preflight.live(
            state, stored, mover_key, mover_hwnd, output, source_id, target_id, fulls,
        ),
    ) {
        if outcome == "identity-changed" {
            drop_member_state(state, mover_key);
        }
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
    // Hide the mover from the source view (commit-before-hide) without
    // selecting or revealing the target. A committed claim is owned even on
    // uncertain post-hide readback: flag it hidden and report the stall
    // without pretending stay success.
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
    // Ordinary lifecycle supplies the next spare when the transfer filled the
    // trailing empty; the source view stays selected throughout (no history
    // record: only verified selects record; trailing maintenance retains the
    // null focus by never touching focus). Item 12 stage B: scoped spare.
    scope_sends::trailing_spare_scoped(state, output);
    // Exact-lifetime focus plan through the tested classifier: genuine null
    // (sole-mover stay emptied the source) resolves Null with ok/no-focus
    // and no setter; a some-but-stale mapping resolves Stale with
    // focus-unverified and no setter either; only Ready actuates once.
    let (class, focus_token) = stay_focus_token(state, output, source_id, plan, observed, retained);
    if class == crate::workspace_owner::StayFocusClass::Null {
        return SendEffect {
            outcome: "ok",
            focus: "no-focus",
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
    if class != crate::workspace_owner::StayFocusClass::Ready {
        return SendEffect {
            outcome: "focus-unverified",
            focus: "vanished",
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
    let focus_token = focus_token.expect("ready stay focus carries its token");
    // Explicit source-selected fence again before focus, then the actuation's
    // own fresh fences (identity/scope/fullscreen/elevated) decide the
    // single setter attempt: no replay on refusal.
    if state.workspaces.active_id(output).as_deref() != Some(source_id) {
        return SendEffect {
            outcome: "focus-unverified",
            focus: "vanished",
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
    let (focus, focus_ms, outcome) = if crate::workspace_owner::focus_before_geometry(
        true,
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        let focus_start = Instant::now();
        let actuation = actuate_focus(state, me, fulls, observed, retained, &focus_token);
        let focus_ms = focus_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        if actuation.outcome == "focus-ok" {
            if let Some(key) = state
                .member_tokens
                .iter()
                .find(|(_, token)| token.as_str() == focus_token.as_str())
                .map(|(key, _)| key.clone())
            {
                state.workspaces.note_foreground(&key);
            }
            ("focus-ok", focus_ms, "ok")
        } else {
            (actuation.outcome, focus_ms, "focus-unverified")
        }
    } else {
        ("focus-skipped-fence", 0, "focus-unverified")
    };
    SendEffect {
        outcome,
        focus,
        source,
        target: None,
        transition_ms: 0,
        observation_ms: 0,
        source_geometry_ms: source_ms,
        target_geometry_ms: 0,
        hide_ms: 0,
        reveal_ms: 0,
        focus_ms,
        source_plan_ms,
        target_plan_ms: 0,
        source_workspace: source_token.to_owned(),
        target_workspace: target_token.to_owned(),
    }
}
