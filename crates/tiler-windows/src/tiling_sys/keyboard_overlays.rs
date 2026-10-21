#![forbid(unsafe_code)]
use super::*;
use crate::snapkey::{QueuedFullscreenIntent, QueuedMaximizeIntent};

#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch_maximize_intent(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    _store: &LedgerStore,
    _dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    intent: QueuedMaximizeIntent,
    log_path: &Path,
) {
    // Win+M toggle (KDE Meta+M parity): key-ups close the pair
    // and passed chords never dispatch, trace-only like the
    // directional arms.
    if !intent.consumed || !intent.announce {
        if state.trace {
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "maximize-toggle",
                    "tick": state.tick,
                    "edge": intent.edge.as_str(),
                    "disposition": if intent.consumed { "consumed" } else { "passed" },
                    "outcome": if intent.consumed { "key-up" } else { "passed" },
                }),
            );
        }
        return;
    }
    state.tick += 1;
    let tick = state.tick;
    let correlation = state.correlation();
    let Some(origin) = intent.origin.clone() else {
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "maximize-toggle",
                "tick": tick,
                "correlation": correlation.as_str(),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "origin-vanished",
            }),
        );
        return;
    };
    // Fresh per-intent suspension/elevation fence (workspace_tick
    // parity): newly intercepted suspended/elevated chords settle
    // here with no side effects. The suspend read exempts verified
    // managed overlays, and the target revalidation below refuses
    // protected windows, so only a live managed member toggles.
    if let Some(outcome) = crate::tiling::toggle_gate_outcome(
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "maximize-toggle",
                "tick": tick,
                "correlation": correlation.as_str(),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": outcome,
                "origin": origin.token,
            }),
        );
        return;
    };
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "maximize-toggle",
                "tick": tick,
                "correlation": correlation.as_str(),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "observation-failed",
                "origin": origin.token,
            }),
        );
        return;
    };
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    // First-seen maximized windows restore once here: lifetime tags are
    // stamped, and the cleared window converges as eligible next tick.
    clear_maximize_at_admission(state, me, &retained);
    let fresh: Vec<SnapOrigin> = state.snap_origins.values().cloned().collect();
    let foreground_hwnd = Some(crate::ffi::foreground_window());
    let (from, _) = match resolve_origin(
        &origin,
        foreground_hwnd,
        &fresh,
        state.snap_advance.as_ref(),
    ) {
        OriginVerdict::Dispatch { token, continued } => (token, continued),
        OriginVerdict::Reject(reason) => {
            state.snap_advance = None;
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "maximize-toggle",
                    "tick": tick,
                    "correlation": correlation.as_str(),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": reason,
                    "origin": origin.token,
                }),
            );
            return;
        }
    };
    let Some(member_key) = state
        .member_tokens
        .iter()
        .find(|(_, token)| token.as_str() == from.as_str())
        .map(|(key, _)| key.clone())
    else {
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "maximize-toggle",
                "tick": tick,
                "correlation": correlation.as_str(),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "unmanaged",
                "origin": origin.token,
            }),
        );
        return;
    };
    if !crate::native_guards::member_matches(&member_key, origin.hwnd, origin.pid, &origin.creation)
    {
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "maximize-toggle",
                "tick": tick,
                "correlation": correlation.as_str(),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "foreground-changed",
                "origin": origin.token,
            }),
        );
        return;
    }
    // Fresh fullscreen/state fences before the native write (KDE
    // `maximize-refused-fullscreen` parity): fullscreen wins and
    // refuses, never toggles. A maximized member restores, any
    // other managed member maximizes.
    let retained_row = retained.iter().find(|r| r.key == member_key).cloned();
    if retained_row.as_ref().is_some_and(|r| r.fullscreen) {
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "maximize-toggle",
                "tick": tick,
                "correlation": correlation.as_str(),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "maximize-refused-fullscreen",
                "origin": origin.token,
                "window": from,
            }),
        );
        return;
    }
    // Visible lifetime gate: the live member tag must equal the
    // stored tag, so a same-process HWND reuse authorizes no
    // native write.
    let live_tag = crate::product_hide::sys::read_member_tag(member_key.hwnd);
    let lifetime_ok = state.member_tags.get(&member_key).is_some_and(|stored| {
        crate::native_guards::visible_lifetime_ok(stored, live_tag.as_deref())
    });
    if !lifetime_ok {
        drop_member_state(state, &member_key);
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "maximize-toggle",
                "tick": tick,
                "correlation": correlation.as_str(),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "identity-changed",
                "origin": origin.token,
            }),
        );
        return;
    }
    let Some(stored) = state.member_identity.get(&member_key).cloned() else {
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "maximize-toggle",
                "tick": tick,
                "correlation": correlation.as_str(),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "unmanaged",
                "origin": origin.token,
            }),
        );
        return;
    };
    // Proof-mode ownership gate before the native write: helpers
    // re-verify against the frozen allowlist exactly like the
    // hide path.
    if let Some(entries) = state.allowlist.as_ref() {
        let owned = entries
            .iter()
            .find(|e| e.hwnd == member_key.hwnd)
            .is_some_and(|entry| verify_proof_owned(member_key.hwnd, entry, me).is_ok());
        if !owned {
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "maximize-toggle",
                    "tick": tick,
                    "correlation": correlation.as_str(),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "identity-changed",
                    "origin": origin.token,
                }),
            );
            return;
        }
    }
    // Explicit scope fences before the native write: out-of-scope
    // members and listed hosts without a live matching child
    // refuse with no writes.
    if !scope_allows(&state.scope, &stored.exe_path)
        || !hosted_gate_allows(
            &stored.exe_path,
            member_key.hwnd,
            stored.pid,
            &state.scope_hosts,
        )
    {
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "maximize-toggle",
                "tick": tick,
                "correlation": correlation.as_str(),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "scope-excluded",
                "origin": origin.token,
                "window": from,
            }),
        );
        return;
    }
    let zoomed = is_zoomed_now(member_key.hwnd);
    let wanted = !zoomed;
    // Discrete-toggle parity (provisional): each fresh discrete
    // Win+M down dispatches exactly one native toggle attempt.
    // Held repeats never reach this drain (the classifier emits
    // them as non-announce trace-only), so no persistent fence is
    // kept: a press after a native restore simply dispatches
    // again. Matches KDE's visible toggle while fixing the sticky
    // `attempted` refusal after a native restore.
    let outcome = toggle_zoom_async(member_key.hwnd, wanted);
    if crate::tiling::restore_wake_arm(outcome, !wanted) {
        state.restore_wake = Some(RestoreWake {
            key: member_key.clone(),
            token: from.clone(),
            until: Instant::now() + Duration::from_millis(crate::tiling::RESTORE_WAKE_MS),
        });
    }
    log_json_at(
        log_path,
        serde_json::json!({
            "event": "maximize-toggle",
            "tick": tick,
            "correlation": correlation.as_str(),
            "edge": intent.edge.as_str(),
            "disposition": "consumed",
            "outcome": outcome,
            "target": if wanted { "maximized" } else { "restored" },
            "origin": origin.token,
            "window": from,
        }),
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch_fullscreen_intent(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    _store: &LedgerStore,
    _dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    intent: QueuedFullscreenIntent,
    log_path: &Path,
) {
    // Win+F11 toggle (KDE Meta+F11 parity): key-ups close the pair
    // and passed chords never dispatch, trace-only like the
    // maximize arm.
    if !intent.consumed || !intent.announce {
        if state.trace {
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "fullscreen-toggle",
                    "tick": state.tick,
                    "edge": intent.edge.as_str(),
                    "disposition": if intent.consumed { "consumed" } else { "passed" },
                    "outcome": if intent.consumed { "key-up" } else { "passed" },
                }),
            );
        }
        return;
    }
    state.tick += 1;
    let tick = state.tick;
    let correlation = state.correlation();
    let Some(origin) = intent.origin.clone() else {
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "fullscreen-toggle",
                "tick": tick,
                "correlation": correlation.as_str(),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "origin-vanished",
            }),
        );
        return;
    };
    // Fresh per-intent suspension/elevation fence (workspace_tick
    // parity): newly intercepted suspended/elevated chords settle
    // here with no side effects. The suspend read exempts the
    // verified managed foreground, so exiting our owned
    // fullscreen still proceeds; app-owned frames refuse below
    // via the metadata decision, never guessing restoration.
    if let Some(outcome) = crate::tiling::toggle_gate_outcome(
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "fullscreen-toggle",
                "tick": tick,
                "correlation": correlation.as_str(),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": outcome,
                "origin": origin.token,
            }),
        );
        return;
    };
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "fullscreen-toggle",
                "tick": tick,
                "correlation": correlation.as_str(),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "observation-failed",
                "origin": origin.token,
            }),
        );
        return;
    };
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    // First-seen maximized windows restore once here: lifetime tags are
    // stamped, and the cleared window converges as eligible next tick.
    clear_maximize_at_admission(state, me, &retained);
    let fresh: Vec<SnapOrigin> = state.snap_origins.values().cloned().collect();
    let foreground_hwnd = Some(crate::ffi::foreground_window());
    let (from, _) = match resolve_origin(
        &origin,
        foreground_hwnd,
        &fresh,
        state.snap_advance.as_ref(),
    ) {
        OriginVerdict::Dispatch { token, continued } => (token, continued),
        OriginVerdict::Reject(reason) => {
            state.snap_advance = None;
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "fullscreen-toggle",
                    "tick": tick,
                    "correlation": correlation.as_str(),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": reason,
                    "origin": origin.token,
                }),
            );
            return;
        }
    };
    let Some(member_key) = state
        .member_tokens
        .iter()
        .find(|(_, token)| token.as_str() == from.as_str())
        .map(|(key, _)| key.clone())
    else {
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "fullscreen-toggle",
                "tick": tick,
                "correlation": correlation.as_str(),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "unmanaged",
                "origin": origin.token,
            }),
        );
        return;
    };
    if !crate::native_guards::member_matches(&member_key, origin.hwnd, origin.pid, &origin.creation)
    {
        state.snap_advance = None;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "fullscreen-toggle",
                "tick": tick,
                "correlation": correlation.as_str(),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": "foreground-changed",
                "origin": origin.token,
            }),
        );
        return;
    }
    // Fresh pre-effect revalidation through the production target
    // gate (focus/overlay classifier, held process
    // integrity/identity, lifetime tag, host-child, proof scope)
    // immediately before any style/`SetWindowPos` effect, using
    // the held handle across effects. Replaces the stale
    // tick-observation gates: the expected window is the fresh
    // eligible observation when present, otherwise a retained
    // overlay construction exactly like `actuate_focus`, and only
    // the geometry classifier is focus-relaxed (overlays allowed).
    let owned_expected: Option<ObservedWindow>;
    let expected = if let Some(found) = observed.iter().find(|w| w.hwnd == member_key.hwnd) {
        found
    } else {
        let Some(row) = retained.iter().find(|r| r.key == member_key) else {
            state.snap_advance = None;
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "fullscreen-toggle",
                    "tick": tick,
                    "correlation": correlation.as_str(),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "deferred",
                    "origin": origin.token,
                }),
            );
            return;
        };
        let Some(stored) = state.member_identity.get(&row.key).cloned() else {
            state.snap_advance = None;
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "fullscreen-toggle",
                    "tick": tick,
                    "correlation": correlation.as_str(),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "unmanaged",
                    "origin": origin.token,
                }),
            );
            return;
        };
        owned_expected = Some(ObservedWindow {
            hwnd: row.key.hwnd,
            token: row.token.clone(),
            outer: Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            visible: Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            insets: FrameInsets::default(),
            identity: crate::tiling::ObservedTarget {
                hwnd: row.key.hwnd,
                pid: row.key.pid,
                process_creation: row.key.creation.clone(),
                exe_path: stored.exe_path,
                user_sid: stored.user_sid,
                session_id: stored.session_id,
                tag: String::new(),
            },
            facts: row.facts.unwrap_or(crate::tiling::WindowFacts {
                visible: true,
                minimized: false,
                maximized: true,
                cloaked: false,
                elevated: false,
                shell: false,
                tool_window: false,
                owned: false,
                captionless_fullscreen: false,
                no_activate: false,
                dialog: false,
            }),
            // Synthetic fallback row: no fresh membership read was taken, so
            // it carries no stamp and can never prove a receipt.
            member_tag: None,
        });
        owned_expected.as_ref().expect("retained expected built")
    };
    let proof_mode = state.allowlist.is_some();
    let member_tag = state.member_tags.get(&member_key).map(String::as_str);
    let target = match revalidate_target(
        expected,
        me,
        fulls,
        &mut state.tokens,
        proof_mode,
        state.allowlist.as_ref(),
        &state.scope,
        member_tag,
        &state.scope_hosts,
        true,
    ) {
        Ok(target) => target,
        Err(reason) => {
            if reason == "identity-changed" {
                drop_member_state(state, &member_key);
            }
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "fullscreen-toggle",
                    "tick": tick,
                    "correlation": correlation.as_str(),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": reason,
                    "origin": origin.token,
                }),
            );
            return;
        }
    };
    // The held process keeps liveness authority across the style
    // and frame effects below; the HWND pid is rechecked by the
    // setters' readbacks, never trusted from the tick.
    let _held = &target.held;
    // Fresh pre-effect state plus our restoration metadata decide
    // the direction: owned metadata wins over geometry, so a press
    // on any owned frame exits even when the frame no longer
    // covers a monitor (external move or partial restore).
    // Entering is always project-owned; an app-owned frame
    // refuses with a bounded reason instead of guessing.
    let now_fullscreen = target.window.facts.captionless_fullscreen;
    let meta = read_fullscreen_meta(member_key.hwnd);
    let (target, outcome) = match fullscreen_toggle_decision(now_fullscreen, meta.is_some()) {
        crate::tiling::FullscreenToggle::RefuseAppOwned => {
            ("fullscreen", "fullscreen-refused-app-owned")
        }
        crate::tiling::FullscreenToggle::Enter => {
            // The revalidation above already proved fresh
            // eligibility (or a maximized overlay, where
            // fullscreen wins): enter with the fresh frame as
            // the output anchor, never a stale tick read.
            let full = output_for_rect(areas, &target.window.visible);
            let full = areas
                .iter()
                .find(|a| a.device == full)
                .map(|area| area.full)
                .or_else(|| areas.first().map(|area| area.full));
            match full {
                Some(full) => ("fullscreen", enter_fullscreen(member_key.hwnd, full)),
                None => ("fullscreen", "unknown-output"),
            }
        }
        crate::tiling::FullscreenToggle::ExitOwned => {
            let meta = meta.expect("decision owns metadata");
            let outcome = exit_fullscreen_owned(member_key.hwnd, &meta);
            // A clean exit to a normal frame restores the current
            // core allocation in the same action; a reasserted
            // maximize converges as a retained overlay next tick.
            if outcome == "restored" && !meta.was_maximized {
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, _store, _dir, &fulls_owned, areas);
            }
            ("restored", outcome)
        }
    };
    log_json_at(
        log_path,
        serde_json::json!({
            "event": "fullscreen-toggle",
            "tick": tick,
            "correlation": correlation.as_str(),
            "edge": intent.edge.as_str(),
            "disposition": "consumed",
            "outcome": outcome,
            "target": target,
            "origin": origin.token,
            "window": from,
        }),
    );
}
