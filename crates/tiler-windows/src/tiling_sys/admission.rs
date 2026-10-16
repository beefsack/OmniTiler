use super::*;

/// Otherwise-eligible check for an admission-clear candidate: every safety
/// gate must already pass, so the clear never mutates an elevated, shell,
/// tool, owned, dialog, cloaked, or no-activate window. Maximized is the only
/// permitted overlay state here (fullscreen never clears, checked separately).
pub(super) fn admission_clear_eligible(facts: WindowFacts) -> bool {
    facts.visible
        && !facts.minimized
        && !facts.cloaked
        && !facts.elevated
        && !facts.shell
        && !facts.tool_window
        && !facts.owned
        && !facts.dialog
        && !facts.no_activate
}

/// D2 admission pin shared by the clear seam and the slot-deferral mirror:
/// the live Engine snapshot proves this token was admitted ordinary/tiled
/// (present, neither an exception nor automatic), so the required clear plans
/// regardless of later fresh-hint changes: hint drift must never reclassify
/// retained admission. Either override record (adapter runtime set or core
/// committed override) carves out to fresh classification instead: with no
/// fixed-specific signal surviving hint loss, authoritative fixed wins keep
/// current semantics pending an explicit product decision (no new lifetime
/// state is invented here).
pub(super) fn committed_ordinary_pin(
    state: &TileLoop,
    key: &crate::workspace::WindowKey,
    token: &str,
) -> bool {
    let Some(loc) = state.workspaces.member_loc(key) else {
        return false;
    };
    let domain_key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(loc.output.clone()),
        workspace: tiler_core::directional::WorkspaceId(loc.workspace.clone()),
    };
    let id = tiler_core::directional::WindowId(token.to_owned());
    let Some(session) = state.engine.session(&domain_key) else {
        return false;
    };
    if state.fixed_tile_overrides.contains(key) || session.has_fixed_tile_override(&id) {
        return false;
    }
    !session.is_automatic_fixed_float(&id)
        && !session.is_exception(&id)
        && session
            .snapshot()
            .windows
            .iter()
            .any(|link| link.window == id)
}

/// Fresh classify-before-clear disposition for one retained maximized row
/// (R-SPC-04 D6): the production-connected decision seam behind
/// `clear_maximize_at_admission`, factored so adapter regressions can assert
/// the planned CLEAR vs SKIP without native windows or writes. Mirrors every
/// cheap pre-query gate in the production loop (first-exit deferral, D4
/// latch plus Engine automatic, tiled-domain floating gate, eligibility,
/// slotless one-shot, keep-overlay latch), then the D2 admission pin
/// (committed ordinary/tiled plans the required clear despite later hint
/// drift), then queries fresh hints under the live predicate with
/// failure-as-unknown: fixed floats (no clear), unknown or non-fixed clears,
/// budget defers to the next tick with no hold, no retry, and no attempt
/// recorded. Assumes unattempted (the production loop re-checks the full
/// attempt identity before any restore); records nothing, so repeated peeks
/// are side-effect free.
pub(super) fn admission_clear_planned(
    state: &TileLoop,
    row: &RetainedRow,
    hint_cx: &HintCx,
) -> bool {
    if row.fullscreen || !row.maximized {
        return false;
    }
    if state.born_first_exit.contains(&row.key) {
        return false;
    }
    if state.fixed_auto_seen.contains(&row.key) || member_is_automatic_fixed(state, &row.key) {
        return false;
    }
    // R-SPC-04 D3/D4/D6/D7: durable fixed-origin overrides never clear.
    // They keep their tiled base through the reserved-maximized slot lane
    // under both `core.born_maximized` values, across hint loss and
    // predicate changes, and never acquire automatic origin. Generic
    // non-fixed wins keep the existing clear/overlay rules below.
    if state.fixed_origin_overrides.contains(&row.key) {
        return false;
    }
    let tiled = state
        .workspaces
        .member_loc(&row.key)
        .and_then(|loc| workspace_mode_known(state, &loc.output, &loc.workspace));
    if !matches!(tiled, Some(true)) {
        return false;
    }
    if !row.facts.is_some_and(admission_clear_eligible) {
        return false;
    }
    if state.member_rects.contains_key(&row.token) {
        return false;
    }
    // R-MAX-06 born overlay and R-MAX-03 retile overlay converge through the
    // retained-slot machinery with no native clear, under their own lanes.
    if state.born_keep_overlay.contains(&row.key) || state.retile_keep_overlay.contains(&row.key) {
        return false;
    }
    if committed_ordinary_pin(state, &row.key, &row.token) {
        return true;
    }
    let (hints, outcome) = hidden_hint_for(state, &row.key, hint_cx);
    if outcome == HintOutcome::Budget {
        return false;
    }
    if hints.is_empty() {
        return true;
    }
    !tiler_core::size_hints::is_fixed_size_with(
        hints,
        fixed_predicate_for(state.settings_live.as_ref()),
    )
}

/// One-shot maximize clear at admission (KDE `clearMaximizeAtAdmission`
/// parity). Call after `ensure_workspace_assignments` (lifetime tags stamped)
/// and before the first Engine admission of the tick. For every retained
/// maximized, non-fullscreen row that is otherwise eligible, scoped,
/// hosted-gated, and proof-verified with fresh identity, and that holds no
/// retained tile slot (`member_rects`), restore the native maximize exactly
/// once with no automatic retry. Fullscreen never clears. The cleared window
/// converges as eligible on the next tick.
///
/// R-MAX-06: the keep-overlay policy binds to the exact fresh lifetime at
/// tiled-first slotless admission (`born_keep_overlay`, set only after every
/// ownership fence in `admit_slotless_maximized`). A latched row skips the
/// clear and converges through the existing retained-slot overlay machinery
/// instead (slotless admission seeds the slot on the tiled domain, row
/// assembly rides the canonical retained rect, and the retained-only token
/// takes no geometry write until a native restore lands it in the slot).
/// R-MAX-03 floating-first rows bind `retile_keep_overlay` at the same
/// admission and converge through the same machinery under both
/// `core.born_maximized` values. Pre-existing, born-fullscreen-exit, and
/// later maxima rows never latch; a live flip applies only to subsequent
/// first admissions and never moves an already-bound lifetime.
///
/// Attempts key on HWND plus process creation plus the live lifetime tag, so
/// a recycled HWND re-arms for the fresh window while the same window never
/// retries. Stale attempts prune when the HWND leaves the enumeration or the
/// live occupant's pid/creation moved on.
pub(super) fn clear_maximize_at_admission(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    retained: &[RetainedRow],
) {
    let log_path = state.log_path.clone();
    // Bounded fresh-classification budget for this pass: the same per-query
    // timeout and aggregate constants as row assembly, spent only on slotless
    // maximized candidates (the cheap gates above/below skip everything else
    // before any query).
    let hint_cx = HintCx::new();
    for row in retained {
        if row.fullscreen || !row.maximized {
            continue;
        }
        // R-SPC-04 D5: a first born-fullscreen exit classifies with current
        // hints and predicate before any maximize-clear/restore/slot side
        // effect. Peek only: the mark is consumed after committed
        // classification (see `consume_classified_first_exits`), never here,
        // so a deferred tick can never lose it before assembly reads it.
        // Fixed exits then float automatic and skip the clear via the Engine
        // mark; non-fixed exits tile and take the normal one-shot clear on
        // a later pass.
        if state.born_first_exit.contains(&row.key) {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "maximize-admission-clear",
                    "window": row.token,
                    "outcome": "deferred-first-exit",
                }),
            );
            continue;
        }
        // R-SPC-04 D4/D8: fixed max-births and Engine-marked automatic fixed
        // floats never clear: they float beneath native maximize with no
        // geometry, focus, stacking, or keep-above writes from admission.
        // Core-fixed max birth wins over the `core.born_maximized` clear.
        if state.fixed_auto_seen.contains(&row.key) || member_is_automatic_fixed(state, &row.key) {
            continue;
        }
        // R-SPC-04 D6 classify-before-clear: query fresh hints under the live
        // predicate before any restore. Fixed floats beneath maximize (no
        // clear under either `core.born_maximized` value); unknown or
        // non-fixed clears once (failure-as-unknown tiles, so a persistent
        // query failure still clears instead of holding silently); budget
        // defers to the next tick with no hold and no attempt recorded. The
        // single source is `admission_clear_planned` (which re-validates the
        // cheap gates above to stay production-connected); the gates above
        // run first so obvious skips spend no budget.
        if !admission_clear_planned(state, row, &hint_cx) {
            continue;
        }
        // Floating workspaces preserve the native maximum until the first
        // tiled admission (KDE floating-gate parity). Unassigned rows fail
        // closed: `ensure_workspace_assignments` owns membership and runs
        // before this clear on every path.
        let domain_tiled = state
            .workspaces
            .member_loc(&row.key)
            .and_then(|loc| workspace_mode_known(state, &loc.output, &loc.workspace));
        let tiled = matches!(domain_tiled, Some(true));
        if !row.facts.is_some_and(admission_clear_eligible) {
            continue;
        }
        // Truly no retained tile slot: a slotted member keeps its allocation
        // and never re-clears. Workspace membership alone is not a slot.
        if state.member_rects.contains_key(&row.token) {
            continue;
        }
        // Fresh identity: the live window must still be the retained one with
        // a same-session, medium-integrity identity. Scope, hosted-child, and
        // proof gates match observation admission exactly.
        let hwnd = row.key.hwnd as isize as HWND;
        let mut live_pid: u32 = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut live_pid);
        }
        if live_pid == 0 || live_pid != row.key.pid {
            continue;
        }
        let live = match HeldProcess::open(live_pid).and_then(|held| {
            let ident = held.identity()?;
            let rid = held.integrity()?;
            Ok((ident, rid))
        }) {
            Ok((ident, rid)) => {
                if ident.pid != live_pid
                    || ident.process_creation != row.key.creation
                    || ident.user_sid != me.user_sid
                    || ident.session_id != me.session_id
                    || !is_medium_rid(rid)
                {
                    continue;
                }
                ident
            }
            Err(_) => continue,
        };
        if !scope_allows(&state.scope, &live.exe_path) {
            continue;
        }
        if !hosted_gate_allows(&live.exe_path, row.key.hwnd, live_pid, &state.scope_hosts) {
            continue;
        }
        if let Some(entries) = state.allowlist.as_ref() {
            let Some(entry) = entries.iter().find(|e| e.hwnd == row.key.hwnd) else {
                continue;
            };
            if verify_proof_owned(row.key.hwnd, entry, me).is_err() {
                continue;
            }
        }
        let live_tag = crate::product_hide::sys::read_member_tag(row.key.hwnd);
        let attempt_key = format!(
            "{}:{}:{}:{}",
            row.key.hwnd,
            row.key.creation,
            live_pid,
            live_tag.as_deref().unwrap_or("")
        );
        if !crate::tiling::should_clear_maximize_at_admission_with_mode(
            row.fullscreen,
            row.maximized,
            false,
            state.maximize_admission_attempted.contains(&attempt_key),
            tiled,
            state.born_keep_overlay.contains(&row.key)
                || state.retile_keep_overlay.contains(&row.key),
        ) {
            continue;
        }
        // Mark before the native call so a failed write never retries.
        state.maximize_admission_attempted.insert(attempt_key);
        let outcome = restore_zoom_placement(row.key.hwnd);
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "maximize-admission-clear",
                "window": row.token,
                "outcome": outcome,
            }),
        );
    }
    // Prune attempts whose window left the enumeration or whose HWND now hosts
    // a different pid/creation: the set stays bounded without ever re-arming
    // a live window.
    state.maximize_admission_attempted.retain(|key| {
        let mut parts = key.splitn(4, ':');
        let (Some(hwnd), Some(creation), Some(pid)) = (parts.next(), parts.next(), parts.next())
        else {
            return false;
        };
        let Ok(hwnd_u64) = hwnd.parse::<u64>() else {
            return false;
        };
        if !state.last_hwnds.contains(&hwnd_u64) {
            return false;
        }
        let Ok(pid_u32) = pid.parse::<u32>() else {
            return false;
        };
        let hwnd_ptr = hwnd_u64 as isize as HWND;
        if unsafe { IsWindow(hwnd_ptr) } == 0 {
            return false;
        }
        let mut live_pid: u32 = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd_ptr, &mut live_pid);
        }
        live_pid == pid_u32 && {
            let key = crate::workspace::WindowKey {
                hwnd: hwnd_u64,
                pid: pid_u32,
                creation: creation.to_owned(),
            };
            // A same-process reuse drops membership tables at assignment; a
            // pruned attempt re-arms the fresh window below.
            state.member_tags.contains_key(&key) || {
                HeldProcess::open(live_pid)
                    .and_then(|held| held.identity())
                    .is_ok_and(|ident| ident.process_creation == creation)
            }
        }
    });
}

/// Overlay refusal for one verified member before any Engine mutation (KDE
/// move/pointer isolation parity): fullscreen wins, then maximize. Returns
/// the refusal cause or `None` when the route may proceed.
pub(super) fn overlay_refusal_for(row: &RetainedRow) -> Option<&'static str> {
    crate::tiling::overlay_refusal(row.fullscreen, row.maximized)
}

/// Whether one Engine token is currently an intentional-float exception in
/// its domain session (floated windows hold no tile slot). Born-fullscreen
/// holds are exceptions too; the intentional lane additionally keys off the
/// `float_topmost_prev` preimage map. `false` when the session is unknown.
pub(super) fn engine_is_float(
    state: &TileLoop,
    output: &str,
    workspace: &str,
    token: &str,
) -> bool {
    let key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(output.to_owned()),
        workspace: tiler_core::directional::WorkspaceId(workspace.to_owned()),
    };
    state
        .engine
        .session(&key)
        .is_some_and(|session| session.is_exception(&WindowId(token.to_owned())))
}

/// Whether one Engine token floats by automatic fixed-size classification
/// (R-SPC-04 D8 membership-only) rather than intentional float. Automatic
/// floats take no geometry, focus, stacking, or keep-above writes from
/// classification; only an explicit user tile (override) may write them.
/// `false` when the session is unknown.
pub(super) fn engine_is_automatic_fixed(
    state: &TileLoop,
    output: &str,
    workspace: &str,
    token: &str,
) -> bool {
    let key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(output.to_owned()),
        workspace: tiler_core::directional::WorkspaceId(workspace.to_owned()),
    };
    state
        .engine
        .session(&key)
        .is_some_and(|session| session.is_automatic_fixed_float(&WindowId(token.to_owned())))
}

/// Whether one Engine token floats by automatic fixed-size classification
/// in one domain session. `false` without a retained session.
pub(super) fn is_automatic_in(
    state: &TileLoop,
    domain_key: &tiler_core::session::DomainKey,
    token: &str,
) -> bool {
    state
        .engine
        .session(domain_key)
        .is_some_and(|session| session.is_automatic_fixed_float(&WindowId(token.to_owned())))
}

/// Whether one intentional-float mover may send tiled-to-tiled through the
/// shared slotless deferred path (D16 ordinary plus approved
/// floating+fullscreen / floating+maximized combos, 2026-10-13; Windows
/// automatic fixed-size floats as ordinary members, 2026-10-16). Mirrors
/// the shared [`tiler_core::session::Session::deferred_send_eligible`]
/// arms against adapter state: the mover must be an Engine exception in its
/// source session, never sticky (checked by the caller), and observed with
/// exactly zero or one overlay from the live `overlay_now` snapshot. Dual
/// overlays refuse. Automatic fixed-size origin (retained Engine mark or D4
/// latch) admits like intentional floats; only an explicit tile override
/// still refuses here. Pure in-memory read: no Engine mutation, no native
/// calls.
pub(super) fn float_send_admits(
    state: &TileLoop,
    mover_key: &crate::workspace::WindowKey,
    output: &str,
    workspace: &str,
    token: &str,
) -> bool {
    if !engine_is_float(state, output, workspace, token) {
        return false;
    }
    if state.fixed_tile_overrides.contains(mover_key) {
        return false;
    }
    match state.overlay_now.get(token) {
        Some((maximized, fullscreen)) => !(*maximized && *fullscreen),
        None => false,
    }
}

/// Float-focus candidates for one float-origin subject (R-FLT-07..11, KDE
/// `selectFloatFocusTarget` caller parity): observed rows in encounter order,
/// then verified retained maximized rows in row order, each with the subject
/// excluded. Sticky windows show on every workspace of their output, so an
/// observed/visible sticky homes anywhere on the subject's output qualifies;
/// other-output stickies never do. Ordinary floats keep strict home-domain
/// membership. Retained rows qualify only maximized, never fullscreen, with a
/// usable live frame, the same lifetime (token still maps to the key) and
/// domain, and float/sticky state. Tiles never enter; automatic fixed floats
/// (R-SPC-04 D8 membership-only) never take focus writes so they never enter
/// either. The selected target still revalidates fresh at actuation. Pure
/// observation read, no Engine mutation, no native calls.
pub(super) fn float_focus_candidates(
    state: &TileLoop,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    output: &str,
    workspace: &str,
    from: &str,
) -> Vec<crate::tiling::FloatFocusCandidate> {
    /// Shared per-row gate: sticky qualifies output-wide, ordinary floats only
    /// in the strict subject domain; automatic fixed never qualifies.
    fn gate(
        state: &TileLoop,
        key: &crate::workspace::WindowKey,
        token: &str,
        output: &str,
        workspace: &str,
    ) -> Option<bool> {
        let home = state.workspaces.member_loc(key)?;
        let sticky_candidate = state.sticky.contains_key(key);
        if sticky_candidate {
            if home.output != output {
                return None;
            }
        } else {
            if home.output != output || home.workspace != workspace {
                return None;
            }
            if !engine_is_float(state, &home.output, &home.workspace, token) {
                return None;
            }
        }
        // Automatic fixed floats never take focus writes.
        if engine_is_automatic_fixed(state, &home.output, &home.workspace, token) {
            return None;
        }
        Some(sticky_candidate)
    }
    let mut candidates = Vec::new();
    for window in observed.iter() {
        if window.token == from {
            continue;
        }
        let key = crate::workspace::WindowKey {
            hwnd: window.hwnd,
            pid: window.identity.pid,
            creation: window.identity.process_creation.clone(),
        };
        let Some(sticky) = gate(state, &key, &window.token, output, workspace) else {
            continue;
        };
        candidates.push(crate::tiling::FloatFocusCandidate {
            token: window.token.clone(),
            x: window.visible.x,
            y: window.visible.y,
            sticky,
        });
    }
    // Retained maximized floats/stickies (never eligible-observed): the
    // ordinary-origin TO-maximized focus leg and float-origin ties need them
    // as targets, actuated through the retained-aware focus path.
    for row in retained.iter() {
        if row.token == from {
            continue;
        }
        if !row.maximized || row.fullscreen {
            continue;
        }
        let Some(rect) = row.rect else {
            continue;
        };
        if state
            .member_tokens
            .get(&row.key)
            .is_none_or(|live| *live != row.token)
        {
            continue;
        }
        let Some(sticky) = gate(state, &row.key, &row.token, output, workspace) else {
            continue;
        };
        candidates.push(crate::tiling::FloatFocusCandidate {
            token: row.token.clone(),
            x: rect.x,
            y: rect.y,
            sticky,
        });
    }
    candidates
}

/// Whether one member key floats by automatic fixed-size classification in
/// its workspace domain. Resolves the domain from live membership; `false`
/// without membership or without a retained session.
fn member_is_automatic_fixed(state: &TileLoop, key: &crate::workspace::WindowKey) -> bool {
    let Some(token) = state.member_tokens.get(key).cloned() else {
        return false;
    };
    let Some(loc) = state.workspaces.member_loc(key) else {
        return false;
    };
    engine_is_automatic_fixed(state, &loc.output, &loc.workspace, &token)
}

/// Fixed-origin provenance for one settled user tile win (R-SPC-04 D7):
/// true only when the prior float is known automatic fixed: the live Engine
/// automatic mark, the D4 latch, or the durable override set (verified
/// hydration). The committed core override is deliberately NOT consulted:
/// core records every unfloat win including generic non-fixed ones, so it
/// cannot prove fixed origin. The live on-window marker is deliberately NOT
/// peeked here either: an unfenced native read in the win path could mistake
/// a stale marker for provenance. Verified hydration already records genuine
/// markers into the durable set under the full held fence; a stale marker
/// with no durable record self-heals on the next restart adoption.
/// Generic non-fixed unfloats read false here and persist no marker.
///
/// Call BEFORE the winning Engine assignment commits: the assignment
/// withdraws the automatic mark, so a later read would lose the evidence.
/// Takes no native reads and performs no writes.
pub(super) fn fixed_origin_provenance(state: &TileLoop, key: &crate::workspace::WindowKey) -> bool {
    if state.fixed_auto_seen.contains(key) || state.fixed_origin_overrides.contains(key) {
        return true;
    }
    member_is_automatic_fixed(state, key)
}

/// R-SPC-04 D1 predicate for subsequent fixed-size admissions: the last-good
/// live setting, read per admission so a saved change applies to later
/// admissions only and never reclassifies retained windows (D2). Proof
/// owners carry no live settings (`None`) and keep the both-axes default.
pub(super) fn fixed_predicate_for(
    live: Option<&LiveSettings>,
) -> tiler_core::size_hints::FixedSizePredicate {
    live.map(|live| live.settings.core.fixed_size_predicate_mode())
        .unwrap_or_default()
}

/// Stamp the fixed-size admission observation flags onto one Engine event
/// from live adapter state, then dispatch it. Every production Engine
/// handle path funnels through here so the classifier sees the same
/// lifetime-bound origin and override the adapter owns:
///
/// - `fullscreen` from the per-tick overlay snapshot (D5 routing and
///   brand-new fullscreen holds; sticky/maximized keep adapter lanes);
/// - `fixed_auto` for D4-latched fixed max-births (never inferred from an
///   intentional flag or hints alone);
/// - `fixed_suppress` for explicit tile overrides (D3/D6/D7).
///
/// Stamping changes no rectangle, focus, topology, or revision binding: with
/// the opt-in on, only the portable floating flag binds convergence, so
/// flag-only drift never diverges a session.
pub(super) fn handle_engine_event(state: &mut TileLoop, event: &mut CoreEvent) -> CoreReply {
    #[cfg(test)]
    PENDING_COUNTERS.with(|slot| {
        slot.borrow_mut().engine += 1;
    });
    stamp_engine_event(state, event);
    stamp_closed_windows(state, event);
    let reply = state.engine.handle(event);
    drain_closed_windows(state, event, &reply);
    reply
}

/// D23: carry queued verified closes on one valid single-domain observation
/// (`Reconcile`/`UpdateGaps` only). Ids homed on the event's domain ride as
/// `closed_windows` so convergence forgets their slots; every other queued id
/// stays pending for its own domain's observation (hidden and multi-domain
/// bursts drain through their own legs). An id observed live again is stale
/// (same-token reuse): it drops here so the wire never carries a
/// contradictory live+closed row. Cross-output/pair legs keep their existing
/// empty shape. Never fabricates membership: only queued ids ride, and an
/// unreadable domain (no valid observation) dispatches nothing.
fn stamp_closed_windows(state: &mut TileLoop, event: &mut CoreEvent) {
    if !matches!(
        event.command,
        CoreCommand::Reconcile | CoreCommand::UpdateGaps
    ) {
        return;
    }
    let output = event.domain_key.output.0.as_str();
    let workspace = event.domain_key.workspace.0.as_str();
    let live: HashSet<&str> = event
        .windows
        .iter()
        .map(|window| window.window.0.as_str())
        .chain(
            event
                .target_windows
                .iter()
                .map(|window| window.window.0.as_str()),
        )
        .collect();
    let mut scoped: Vec<WindowId> = Vec::new();
    let mut stale: Vec<String> = Vec::new();
    for (id, homing) in state.pending_closed.iter() {
        if homing.output != output || homing.workspace != workspace {
            continue;
        }
        if live.contains(id.as_str()) {
            stale.push(id.clone());
        } else {
            scoped.push(WindowId(id.clone()));
        }
    }
    for id in stale {
        state.pending_closed.remove(&id);
    }
    scoped.sort_by(|a, b| a.0.cmp(&b.0));
    event.closed_windows = scoped;
}

/// D23: drop delivered closes only on Engine acceptance. Rejected,
/// snapshot-invalid, and diverged observations retain every stamped id for
/// the next ordinary signal (never a retry ledger, never a poll); delivered
/// ids clear only while still homed where they were sent, so an id re-queued
/// under new homing keeps its newer record.
fn drain_closed_windows(state: &mut TileLoop, event: &CoreEvent, reply: &CoreReply) {
    if matches!(
        reply,
        CoreReply::Rejected { .. } | CoreReply::SnapshotInvalid { .. } | CoreReply::Diverged(_)
    ) {
        return;
    }
    let output = event.domain_key.output.0.as_str();
    let workspace = event.domain_key.workspace.0.as_str();
    for window in &event.closed_windows {
        let homed_here = state
            .pending_closed
            .get(&window.0)
            .is_some_and(|homing| homing.output == output && homing.workspace == workspace);
        if homed_here {
            state.pending_closed.remove(&window.0);
        }
    }
}

/// D23 ordinary-observation close-drain fanout: hidden closes wait for a
/// covering select/send, but a workspace may be pruned before one arrives,
/// stranding the Engine slot plus the queue entry. After the tick's active
/// legs, fan out over every queued domain OUTSIDE the selected set through
/// the existing assemble + handle path, so close reports converge (forgetting
/// slots, retiring slot-only sessions) before registry pruning can strand
/// them. Active domains rode the tick legs above and are excluded here;
/// their rejections retain for the next ordinary tick.
///
/// Hidden legs commit Engine state only: no geometry application, no focus
/// actuation, no writes of any kind. Domains that fail to assemble
/// (unreadable evidence) or reject retain their queues, and entries homed
/// on domains with no Engine session drop (no session holds no slot, so no
/// ghost is left). No poll, no retry ledger, no timer: this runs inline on
/// ordinary tick evidence, matching the shared drain-before-retirement
/// contract the KDE refresh chain already implements adapter-side.
pub(super) fn fanout_pending_closed(
    state: &mut TileLoop,
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    correlation: &str,
) {
    if state.pending_closed.is_empty() {
        return;
    }
    let mut active: HashSet<(String, String)> = HashSet::new();
    for area in areas {
        if let Some(id) = state.workspaces.active_id(&area.device) {
            active.insert((area.device.clone(), id));
        }
    }
    let mut domains: BTreeSet<(String, String)> = BTreeSet::new();
    for homing in state.pending_closed.values() {
        let domain = (homing.output.clone(), homing.workspace.clone());
        if !active.contains(&domain) {
            domains.insert(domain);
        }
    }
    let mut hint_cx = HintCx::new();
    for (output, workspace) in domains {
        let Some((domain, domain_key)) =
            workspace_domain_for(&output, &workspace, areas, state.inner_gap, state.outer_gap)
        else {
            continue;
        };
        // No session holds no slot: a queue entry without its session is
        // stale (never seeded, or torn down by a path that already dropped
        // homed entries), so it drops here rather than growing.
        if state.engine.session(&domain_key).is_none() {
            state
                .pending_closed
                .retain(|_, homing| homing.output != output || homing.workspace != workspace);
            continue;
        }
        let Some(rows) = assemble_domain_rows(
            state,
            &output,
            &workspace,
            observed,
            retained,
            "close-drain",
            correlation,
            &mut hint_cx,
        ) else {
            continue;
        };
        let carried: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
            .iter()
            .map(|row| {
                (
                    WindowId(row.token.clone()),
                    row.rect,
                    row.hints,
                    row.floating,
                )
            })
            .collect();
        let fp = fingerprint(
            &rows
                .iter()
                .map(|row| (row.token.clone(), row.rect))
                .collect::<Vec<_>>(),
        );
        let Some(correlation_id) = CorrelationId::parse(correlation) else {
            continue;
        };
        let mut event = crate::tiling::build_reconcile_event_for_floating(
            &state.owner,
            &state.generation,
            &correlation_id,
            revision_for(state, &output, &workspace),
            fp,
            &domain,
            &domain_key,
            state.outer_gap,
            &carried,
            None,
        );
        // Same gap-adoption branch as the active reconcile legs: retained
        // gap drift converges through the topology-preserving update-gaps
        // path, since plain reconcile refuses domain-mismatch instead of
        // adopting it. Without this, drifted gaps reject the drain forever.
        if !crate::settings::retained_gaps_match(
            &state.engine,
            &domain_key,
            state.inner_gap,
            state.outer_gap,
        ) {
            event.command = CoreCommand::UpdateGaps;
        }
        // Engine commit only: deliberately no `apply_geometry`, no focus
        // actuation, no writes. Stamping scopes the domain's queued closes
        // onto the wire; acceptance drains them, rejection retains them.
        handle_engine_event(state, &mut event);
    }
}

/// Stamp the fixed-size admission observation flags onto one Engine event
/// from live adapter state without dispatching. Candidate-engine pre-checks
/// (which must not commit) stamp through here, then handle on their clone.
pub(super) fn stamp_engine_event(state: &TileLoop, event: &mut CoreEvent) {
    let by_token: std::collections::HashMap<&str, &crate::workspace::WindowKey> = state
        .member_tokens
        .iter()
        .map(|(key, token)| (token.as_str(), key))
        .collect();
    let mut flags: std::collections::HashMap<
        WindowId,
        crate::workspace_owner::FixedAdmissionFlags,
    > = std::collections::HashMap::new();
    let mut stamp = |token: &str| {
        let flagged = flags.entry(WindowId(token.to_owned())).or_default();
        flagged.fullscreen = state
            .overlay_now
            .get(token)
            .is_some_and(|(_, fullscreen)| *fullscreen);
        if let Some(key) = by_token.get(token) {
            // Sticky dominates automatic origin (core never marks automatic
            // for a sticky observation): a sticky window never carries the
            // fixed_auto signal, so sticky intent is never mistaken for
            // automatic classification. Sticky/maximized Engine flags stay
            // adapter-owned (false); see the ToggleFloat binding analysis on
            // `stamp_fixed_admission_flags`.
            let sticky = state.sticky.contains_key(*key);
            // Durable fixed-origin overrides never acquire automatic origin:
            // the explicit win dominates even if a stale D4 latch survived
            // beside it (every lane clears the latch on win, but the stamp
            // refuses to assert both regardless).
            let durable = state.fixed_origin_overrides.contains(*key);
            if !sticky && !durable && state.fixed_auto_seen.contains(*key) {
                flagged.fixed_auto = true;
            }
            if state.fixed_tile_overrides.contains(*key) {
                flagged.fixed_suppress = true;
            }
        }
    };
    for window in event.windows.iter() {
        stamp(&window.window.0.clone());
    }
    for window in event.target_windows.iter() {
        stamp(&window.window.0.clone());
    }
    crate::workspace_owner::stamp_fixed_admission_flags(&mut event.windows, &flags);
    crate::workspace_owner::stamp_fixed_admission_flags(&mut event.target_windows, &flags);
}
