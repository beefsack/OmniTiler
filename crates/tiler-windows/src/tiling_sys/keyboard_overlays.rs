#![forbid(unsafe_code)]
use super::*;
use crate::snapkey::{QueuedFullscreenIntent, QueuedMaximizeIntent};

/// Core-result labels for one world-owned state toggle. The closed
/// [`tiler_core::world_report::CommandResult`] speaks first (a maximize
/// direction or a typed refusal), then the published state target names the
/// fullscreen/sticky direction, and an absent reply is a dispatch refusal,
/// never a guessed direction.
pub(super) fn state_toggle_labels(
    reply: Option<&super::world_tick::WorldCommandOutcome>,
    window: &str,
    property: tiler_core::world_report::StateProperty,
    toggle: &'static str,
) -> (&'static str, &'static str) {
    let Some(reply) = reply else {
        return ("dispatch-refused", toggle);
    };
    if let Some(result) = reply.result.as_ref() {
        return match result {
            tiler_core::world_report::CommandResult::Maximize { maximized, .. } => {
                let direction = if *maximized { "maximized" } else { "restored" };
                (direction, direction)
            }
            tiler_core::world_report::CommandResult::Rejected { .. } => ("rejected", toggle),
            _ => ("accepted", toggle),
        };
    }
    let value = reply
        .reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .and_then(|row| row.state.iter().find(|target| target.property == property))
        .map(|target| target.value);
    match (property, value) {
        (tiler_core::world_report::StateProperty::Fullscreen, Some(true)) => {
            ("accepted", "fullscreen")
        }
        (tiler_core::world_report::StateProperty::Fullscreen, Some(false)) => {
            ("accepted", "restored")
        }
        (tiler_core::world_report::StateProperty::Sticky, Some(true)) => ("accepted", "sticky-on"),
        (tiler_core::world_report::StateProperty::Sticky, Some(false)) => {
            ("accepted", "sticky-off")
        }
        _ => ("accepted", toggle),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch_maximize_intent(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
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
    // World-owned toggle: the core decides the direction from its staged
    // facts (fullscreen always maximizes with a fullscreen clear), and the
    // applied reply performs the state write through the single state lane.
    // The applied reply IS the outcome; this seam never writes natively.
    let foreground = crate::ffi::foreground_window();
    let reply = super::world_tick::run_world_command(
        state,
        me,
        store,
        dir,
        &observed,
        &retained,
        areas,
        foreground,
        fulls,
        tiler_core::world_report::WorldCommand::ToggleMaximize {
            window: tiler_core::directional::WindowSubject::Window(
                tiler_core::directional::WindowId(from.clone()),
            ),
        },
        super::observation::sample_cursor(),
    );
    let (outcome, target) = state_toggle_labels(
        reply.as_ref(),
        &from,
        tiler_core::world_report::StateProperty::Maximized,
        "maximize",
    );
    let mut line = serde_json::json!({
        "event": "maximize-toggle",
        "tick": tick,
        "correlation": correlation.as_str(),
        "edge": intent.edge.as_str(),
        "disposition": "consumed",
        "outcome": outcome,
        "target": target,
        "origin": origin.token,
        "window": from,
    });
    if let Some(tiler_core::world_report::CommandResult::Rejected { kind }) =
        reply.as_ref().and_then(|outcome| outcome.result.as_ref())
    {
        line["cause"] = serde_json::Value::from(kind.as_str());
    }
    log_json_at(log_path, line);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch_fullscreen_intent(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
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
    // fullscreen still proceeds; app-owned frames refuse in the
    // core with its closed `fullscreen-not-exitable` kind, never
    // guessing restoration.
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
    // immediately before the world command, so the core only sees
    // a freshly proven target. Replaces the stale
    // tick-observation gates: the expected window is the fresh
    // eligible observation when present, otherwise a retained
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
    if let Err(reason) = revalidate_target(
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
    // World-owned toggle: the core decides the direction from its staged
    // facts and owned-fullscreen metadata, refuses an app-owned frame with
    // its closed kind (never guessing restoration), and the applied reply
    // performs the enter/exit through the single state lane. The applied
    // reply IS the outcome; this seam never writes natively.
    let foreground = crate::ffi::foreground_window();
    let reply = super::world_tick::run_world_command(
        state,
        me,
        store,
        dir,
        &observed,
        &retained,
        areas,
        foreground,
        fulls,
        tiler_core::world_report::WorldCommand::ToggleFullscreen {
            window: tiler_core::directional::WindowSubject::Window(
                tiler_core::directional::WindowId(from.clone()),
            ),
        },
        super::observation::sample_cursor(),
    );
    let (outcome, target) = state_toggle_labels(
        reply.as_ref(),
        &from,
        tiler_core::world_report::StateProperty::Fullscreen,
        "fullscreen",
    );
    let mut line = serde_json::json!({
        "event": "fullscreen-toggle",
        "tick": tick,
        "correlation": correlation.as_str(),
        "edge": intent.edge.as_str(),
        "disposition": "consumed",
        "outcome": outcome,
        "target": target,
        "origin": origin.token,
        "window": from,
    });
    if let Some(tiler_core::world_report::CommandResult::Rejected { kind }) =
        reply.as_ref().and_then(|outcome| outcome.result.as_ref())
    {
        line["cause"] = serde_json::Value::from(kind.as_str());
    }
    log_json_at(log_path, line);
}
