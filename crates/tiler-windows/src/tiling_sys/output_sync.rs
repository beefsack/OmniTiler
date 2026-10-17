#![forbid(unsafe_code)]
use super::*;

/// True when the foreground window's process is not medium integrity:
/// elevated foreground gates workspace dispatch.
pub(super) fn foreground_elevated(me: &ProcessIdentity) -> bool {
    #[cfg(test)]
    if let Some(elevated) = INJECTED_ELEVATED.with(|slot| slot.get()) {
        let _ = me;
        return elevated;
    }
    let foreground = crate::ffi::foreground_window();
    if foreground == 0 {
        return false;
    }
    let pid = crate::ffi::window_pid(foreground);
    if pid == 0 || pid == me.pid {
        return false;
    }
    let Ok(held) = HeldProcess::open(pid) else {
        return true;
    };
    match held.integrity() {
        Ok(rid) => !is_medium_rid(rid),
        Err(_) => true,
    }
}

/// Actual-foreground hidden member selects its workspace. Only a real
/// foreground change to an exactly verified hidden member switches; the
/// `Wake` event alone never does. Returns the settled select effect or `None`.
pub(super) fn poll_foreground_workspace(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &mut [ObservedWindow],
) -> Option<SelectEffect> {
    let foreground = crate::ffi::foreground_window();
    if foreground == 0 || foreground == state.last_foreground {
        return None;
    }
    state.last_foreground = foreground;
    // Keyed by full member identity: find the HWND's key, then guard full
    // equality before any lookup so a recycled HWND never selects.
    let key = state
        .hidden_claims
        .keys()
        .find(|k| k.hwnd == foreground)
        .cloned()?;
    let record = state.hidden_claims.get(&key).cloned()?;
    // Exact verification: live PID, full process identity, exact nonce.
    let snap = crate::product_hide::sys::query_recovery(foreground, me).ok()?;
    if !crate::workspace_owner::member_matches(
        &key,
        foreground,
        snap.pid,
        &snap.process.process_creation,
    ) || snap.process != record.claim.process
        || snap.nonce.as_deref() != Some(record.claim.tag.as_str())
    {
        return None;
    }
    // Externally foregrounded: this is genuinely the member's workspace.
    let loc = state.workspaces.member_loc(&key).cloned()?;
    if !loc.hidden {
        return None;
    }
    let output = loc.output.clone();
    let workspace = loc.workspace.clone();
    // No queued chord here: the external foreground is the trigger, so the
    // queue wait is zero by construction.
    state.tick += 1;
    let tick = state.tick;
    let start = Instant::now();
    let ctx = ActionCtx {
        correlation: format!("act-{tick}"),
        tick,
        queued_at: start,
        start,
    };
    let effect = workspace_do_select(
        state, me, store, dir, fulls, areas, observed, &output, &workspace, &ctx, None, false,
        false,
    );
    log_workspace_action(
        state,
        &state.log_path.clone(),
        &ActionLine {
            ctx: &ctx,
            op: "select",
            index: 0,
            edge: "foreground",
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
    Some(effect)
}

/// Externally revealed hidden claims without foreground return to hidden
/// under the same claim and identity: never re-admitted, never moved to the
/// wrong domain. Destroyed/recycled claims retire their ledger notes.
///
/// Diagnostics: every actual actuation attempt accumulates into the
/// per-second `rehide-sweep` buckets (one summary line per second at most),
/// so a corrected transient is never silently lost even when no reconcile
/// samples it, and persistent failures cannot flood. `corrected` means the
/// drift was re-hidden; `retired` means the claim died and the retirement
/// reveal outcome rides the detail; anything else names the failing leg.
fn rehide_pass(state: &mut TileLoop, me: &ProcessIdentity, store: &LedgerStore) {
    let foreground = crate::ffi::foreground_window();
    let keys: Vec<crate::workspace::WindowKey> = state.hidden_claims.keys().cloned().collect();
    for key in keys {
        if key.hwnd == foreground {
            continue;
        }
        let Some(record) = state.hidden_claims.get(&key).cloned() else {
            continue;
        };
        let visible = crate::ffi::is_visible(key.hwnd);
        if !visible {
            continue;
        }
        // Same-claim guard: only the exact stored identity returns to hidden.
        let snap = match crate::product_hide::sys::query_recovery(key.hwnd, me) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if !crate::workspace_owner::member_matches(
            &key,
            key.hwnd,
            snap.pid,
            &snap.process.process_creation,
        ) || snap.process != record.claim.process
            || snap.nonce.as_deref() != Some(record.claim.tag.as_str())
        {
            continue;
        }
        // Opaque sample window for the sweep below (null when unknown).
        let token = state.member_tokens.get(&key).cloned().unwrap_or_default();
        let sample = if token.is_empty() {
            None
        } else {
            Some(token.as_str())
        };
        match crate::product_hide::sys::rehide_known_claim(me, &record.claim) {
            Ok(true) => {
                note_rehide_diag(state, sample, "corrected", "rehidden");
            }
            Ok(false) => {
                // Retired: drop the ledger note through the identity-safe
                // reveal path and forget session membership safely. The
                // retirement reveal outcome logs before the state removal.
                match crate::product_hide::sys::reveal_product_claim_to(
                    store,
                    me,
                    &record.claim,
                    record.iconic,
                ) {
                    Ok(teardown) => {
                        note_rehide_diag(state, sample, "retired", reveal_outcome(&teardown));
                    }
                    Err(e) => {
                        let (outcome, _) = reveal_err_outcome(&e.to_string());
                        note_rehide_diag(state, sample, outcome, rehide_retired_detail(outcome));
                    }
                }
                state.hidden_claims.remove(&key);
            }
            Err(e) => {
                let (outcome, detail) = rehide_err_outcome(&e.to_string());
                note_rehide_diag(state, sample, outcome, detail);
            }
        }
    }
    if let Some(line) = flush_rehide_diag(state) {
        log_json_at(&state.log_path.clone(), line);
    }
}
/// Sync monitor arrivals/removals with whole-workspace displacement/return:
/// displaced layouts are never merged, hidden windows never lost, and on
/// return the workspaces come back with then-current contents. Focus
/// preserves the surviving view unless the removed monitor held focus.
/// Mode-aware monitor topology sync (item 12 stage C): thin dispatcher into
/// [`scope_hotplug`]. Local keeps the legacy body below unchanged;
/// global-unique adds scoped history/trailing and shared skips displacement.
pub(super) fn sync_monitor_outputs(state: &mut TileLoop, areas: &[MonitorArea]) {
    scope_hotplug::sync_monitor_outputs(state, areas);
}

pub(super) fn sync_monitor_outputs_local(state: &mut TileLoop, areas: &[MonitorArea]) {
    let current: Vec<String> = areas.iter().map(|a| a.device.clone()).collect();
    // Returning displaced origins are recreated by `reconnect_output` without
    // baseline empties below: never `ensure_output` them here, or a duplicate
    // trailing/minimum pair appears beside the returning workspaces.
    let displaced_origins: Vec<String> = state
        .workspaces
        .displaced_snapshot()
        .keys()
        .cloned()
        .collect();
    for key in &current {
        if displaced_origins.contains(key) {
            continue;
        }
        state.workspaces.ensure_output(key);
    }
    let gone: Vec<String> = state
        .known_outputs
        .iter()
        .filter(|k| !current.contains(k))
        .cloned()
        .collect();
    // Actual-foreground member view at disconnect time: when the removed
    // monitor held focus, the survivor shows that member's workspace instead
    // of an arbitrary one. Otherwise the surviving view keeps focus untouched.
    // The foreground key must also pass the visible lifetime gate, so a
    // same-process HWND reuse racing the disconnect never activates the wrong
    // workspace.
    let fg_workspace: Option<(String, String)> = {
        let fg = crate::ffi::foreground_window();
        let fg_tag = crate::product_hide::sys::read_member_tag(fg);
        state
            .member_tokens
            .iter()
            .find(|(k, _)| {
                k.hwnd == fg
                    && state.member_tags.get(*k).is_some_and(|stored| {
                        crate::workspace_owner::visible_lifetime_ok(stored, fg_tag.as_deref())
                    })
            })
            .and_then(|(k, _)| state.workspaces.member_loc(k).cloned())
            .map(|loc| (loc.output, loc.workspace))
    };
    let mut deferred: Vec<String> = Vec::new();
    for origin in gone {
        // Disconnect-time geometry decides the survivor: the origin's own
        // last-seen rectangle first, never post-disconnect frames as proxy;
        // then active output, then ordering. Never merge into a new split.
        let reference = state
            .last_areas
            .iter()
            .find(|a| a.device == origin)
            .or_else(|| state.last_areas.first())
            .map(|a| tiler_core::workspace::DisplacedRect {
                x: a.full.x,
                y: a.full.y,
                w: a.full.w,
                h: a.full.h,
            });
        let survivors: Vec<tiler_core::workspace::Survivor> = current
            .iter()
            .map(|k| {
                let rect = areas.iter().find(|a| &a.device == k).map(|a| {
                    tiler_core::workspace::DisplacedRect {
                        x: a.full.x,
                        y: a.full.y,
                        w: a.full.w,
                        h: a.full.h,
                    }
                });
                tiler_core::workspace::Survivor {
                    key: k.clone(),
                    rect,
                }
            })
            .collect();
        let active = if state.active_output.is_empty() {
            None
        } else {
            Some(state.active_output.as_str())
        };
        if let Some(dest) =
            tiler_core::workspace::choose_displaced_destination(&survivors, active, reference)
        {
            // Relocate retained Engine sessions first so layout trees,
            // contents, and focus move with the workspaces instead of
            // reseeding on the survivor. Empty or unusable sessions stay;
            // the outcome counts below stay opaque.
            let mut relocated = 0u32;
            let mut retained_count = 0u32;
            // Membership entries move below; Engine sessions follow per
            // workspace id through the existing relocation operation.
            let before: Vec<String> = state.workspaces.workspace_ids(&origin);
            for ws in &before {
                let target_key = tiler_core::session::DomainKey {
                    output: tiler_core::directional::OutputId(dest.clone()),
                    workspace: tiler_core::directional::WorkspaceId(ws.clone()),
                };
                let bounds = areas
                    .iter()
                    .find(|a| a.device == dest)
                    .and_then(|a| tiling_domain_bounds_with(a.work, state.outer_gap));
                let Some(bounds) = bounds else {
                    retained_count += 1;
                    continue;
                };
                let target_domain = tiler_core::session::OutputDomain {
                    id: tiler_core::directional::OutputId(dest.clone()),
                    workspace: tiler_core::directional::WorkspaceId(ws.clone()),
                    bounds,
                    gap: state.inner_gap,
                    adjacent: std::collections::BTreeMap::new(),
                };
                if state.engine.try_relocate_for_target(
                    &target_key,
                    &target_domain,
                    state.outer_gap,
                ) {
                    relocated += 1;
                } else {
                    retained_count += 1;
                }
            }
            state.workspaces.displace_output_to(&origin, &dest);
            if state.active_output == origin {
                state.active_output = dest.clone();
            }
            // The removed monitor held focus: show that member's workspace on
            // the survivor with its actual focus preserved. Any other case
            // keeps the survivor view with no stealing.
            if let Some((fg_output, fg_workspace)) = fg_workspace.clone()
                && fg_output == origin
            {
                state.workspaces.activate(&dest, &fg_workspace);
                if let Some(fg_key) = state
                    .member_tokens
                    .keys()
                    .find(|k| {
                        state
                            .workspaces
                            .member_loc(k)
                            .is_some_and(|l| l.output == dest && l.workspace == fg_workspace)
                    })
                    .cloned()
                {
                    state.workspaces.note_foreground(&fg_key);
                }
            }
            log_json_at(
                &state.log_path,
                serde_json::json!({
                    "event": "workspace-displace",
                    "output": state.workspaces.output_token(&dest),
                    "relocated": relocated,
                    "retained": retained_count,
                }),
            );
            // Hotplug-driven observed change records like any other: observe
            // the survivor's post-actuation view so a later toggle returns
            // to the actual preceding view. `displace_output_to` already
            // discarded the removed origin's history.
            if let Some(active) = state.workspaces.active_id(&dest) {
                state.workspaces.observe_workspace_change(&dest, &active);
            }
        } else {
            // No survivor: defer with known origins kept, never drop the
            // workspaces or their Engine sessions. The disconnected output's
            // history is still discarded with the output.
            state.workspaces.discard_output_history(&origin);
            deferred.push(origin);
        }
    }
    // Reconnect returns displaced workspaces to the exact origin key with
    // then-current contents; Engine sessions relocate back first so the
    // returning monitor shows then-current trees, not saved snapshots.
    // The active view shows the returning workspace only when it holds the
    // focused window, otherwise the surviving view keeps focus with no
    // stealing (best-effort: preserve active output).
    let displaced: Vec<String> = state
        .workspaces
        .displaced_snapshot()
        .keys()
        .cloned()
        .collect();
    for origin in displaced {
        if current.contains(&origin) {
            let dest_key = state
                .workspaces
                .displaced_snapshot()
                .get(&origin)
                .map(|r| r.dest_key.clone());
            if let Some(record) = state.workspaces.displaced_snapshot().get(&origin).cloned() {
                let bounds = areas
                    .iter()
                    .find(|a| a.device == origin)
                    .and_then(|a| tiling_domain_bounds_with(a.work, state.outer_gap));
                if let Some(bounds) = bounds {
                    for ws in &record.workspace_ids {
                        let target_key = tiler_core::session::DomainKey {
                            output: tiler_core::directional::OutputId(origin.clone()),
                            workspace: tiler_core::directional::WorkspaceId(ws.clone()),
                        };
                        let target_domain = tiler_core::session::OutputDomain {
                            id: tiler_core::directional::OutputId(origin.clone()),
                            workspace: tiler_core::directional::WorkspaceId(ws.clone()),
                            bounds,
                            gap: state.inner_gap,
                            adjacent: std::collections::BTreeMap::new(),
                        };
                        state.engine.try_relocate_for_target(
                            &target_key,
                            &target_domain,
                            OUTER_GAP,
                        );
                    }
                }
            }
            state.workspaces.reconnect_output(&origin);
            // Reconnect primes a fresh baseline at the returning view and
            // never consults history for selection (see `reconnect_output`).
            // Record any subsequently observed survivor displacement here so
            // two changes in one handler leave the actual immediately
            // preceding view: the survivor's post-return view observes
            // against its pre-return baseline.
            let dest_key = dest_key.unwrap_or_else(|| state.active_output.clone());
            if !dest_key.is_empty()
                && let Some(active) = state.workspaces.active_id(&dest_key)
            {
                state
                    .workspaces
                    .observe_workspace_change(&dest_key, &active);
            }
        }
    }
    // Deferred no-survivor origins stay known with their last geometry until
    // a survivor exists; their workspaces and Engine sessions are untouched.
    let mut known = current.clone();
    for origin in &deferred {
        if !known.contains(origin) {
            known.push(origin.clone());
        }
    }
    state.known_outputs = known;
    let mut last = areas.to_vec();
    for origin in &deferred {
        if !last.iter().any(|a| &a.device == origin)
            && let Some(area) = state
                .last_areas
                .iter()
                .find(|a| &a.device == origin)
                .cloned()
        {
            last.push(area);
        }
    }
    state.last_areas = last;
    if state.active_output.is_empty()
        && let Some(first) = state.known_outputs.first().cloned()
    {
        state.active_output = first;
    }
}

/// Per-tick workspace maintenance on quiet ticks: monitor sync, new-window
/// assignment, close cleanup, foreground-driven select, and same-claim
/// rehide of externally revealed hidden windows.
pub(super) fn workspace_maintenance(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    sync_monitor_outputs(state, areas);
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        return;
    };
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    // First-seen maximized windows restore once here: lifetime tags are
    // stamped, and the cleared window converges as eligible next tick.
    clear_maximize_at_admission(state, me, &retained);
    workspace_close_cleanup(state);
    // Bounded normal-level visibility/exception snapshots over the
    // maintenance observation just taken (same boundary as `reconcile_tick`):
    // maintenance re-observes after reconcile on the same quiet turn, so
    // without this its fresher facts would bypass diagnostics until the next
    // tick. Mutations below converge next tick; the per-second throttle
    // bounds volume and logging never affects behavior. Opaque tokens only.
    stage_visibility(state, &observed, &retained, &skipped);
    stage_exceptions(state);
    if let Some(event) = flush_visibility(state) {
        log_json_at(&state.log_path.clone(), event);
    }
    if let Some(event) = flush_exceptions(state) {
        log_json_at(&state.log_path.clone(), event);
    }
    // Unconfirmed toggle releases retry here on the maintenance observation
    // (no second enumeration): the fingerprint gate inside fires only on a
    // real topology edge, so quiet ticks never poll or log.
    retry_pending_releases(state, me, fulls, areas, &observed, &retained);
    // Periodic hidden-claim retirement before any foreground-driven switch so
    // dead claims never select.
    audit_hidden_claims(state, me, store);
    // Foreground-driven select only on an actual foreground change to an
    // exactly verified hidden member; event-only wakes never switch.
    if poll_foreground_workspace(state, me, store, dir, fulls, areas, &mut observed).is_some() {
        return;
    }
    rehide_pass(state, me, store);
}
