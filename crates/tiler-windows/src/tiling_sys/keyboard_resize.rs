#![forbid(unsafe_code)]
use super::*;

/// Keyboard resize dispatch (Win+Alt grow outwards, Win+Shift+Alt shrink
/// inwards, over H/J/K/L plus arrow aliases): fresh observation, exact
/// origin re-resolution, full gates, one Engine [`CoreCommand::Resize`] on
/// the retained session with the exact KDE `requestResize` repeat tracker
/// (`repeatFocused`/`repeatDirection`/`repeatMode`/`repeatNext` via
/// [`crate::tiling::resize_repeat_next`]; `press_index` 0 is the initial
/// 12px press, then 14/16/18/20px capped), then sibling reflow through the
/// shared write path. No focus, order, or topology mutation beyond the two
/// adjacent shares: focus stays on the same window. Never Move semantics:
/// the command is always `CoreCommand::Resize` carrying the pinned mode, and
/// the subject is always the origin-verified focused mover.
///
/// Pre-dispatch refusals mirror KDE `requestResize` exactly (disabled, busy,
/// floating subject, floating workspace, fullscreen/maximized overlay) plus
/// the Windows production fences (suspend/elevated, exact origin
/// re-resolution, lifetime, proof, scope). Refusals settle with no writes
/// and no repeat-tracker update; only a fence-passing press advances the
/// tracker, before the Engine dispatch (KDE order).
pub(super) fn dispatch_resize_intent(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    intent: crate::snapkey::QueuedResizeIntent,
) {
    let log_path = state.log_path.clone();
    // Key-ups close the pair and passed chords never dispatch: trace-only so
    // held-key traffic stays out of normal logs.
    if !intent.consumed || !intent.announce {
        if state.trace {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "resize",
                    "tick": state.tick,
                    "mode": intent.mode.as_str(),
                    "direction": direction_name(intent.direction),
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
    let settle = |outcome: &'static str| {
        serde_json::json!({
            "event": "resize",
            "tick": tick,
            "correlation": correlation.as_str(),
            "mode": intent.mode.as_str(),
            "direction": direction_name(intent.direction),
            "edge": intent.edge.as_str(),
            "disposition": "consumed",
            "outcome": outcome,
        })
    };
    // KDE `!enabled` parity: chords arriving while keyboard takeover is off
    // never dispatch (the classifier already passes them through; this
    // covers the exact-owner test-needed route, which bypasses it).
    if !state.keyboard.takeover {
        let line = settle("resize-refused-disabled");
        log_json_at(&log_path, line);
        return;
    }
    let Some(origin) = intent.origin.clone() else {
        state.snap_advance = None;
        let line = settle("origin-vanished");
        log_json_at(&log_path, line);
        return;
    };
    // Fresh per-intent suspension/elevation fence (workspace_tick parity):
    // newly intercepted suspended/elevated chords settle here with no side
    // effects.
    if let Some(outcome) = crate::tiling::toggle_gate_outcome(
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        let mut line = settle(outcome);
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    };
    let Some(chord) = observe_chord_intent(state, me, fulls, areas) else {
        let mut line = settle("observation-failed");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    };
    let ChordObserved {
        observed,
        retained,
        skipped: _,
    } = chord;
    let ChordTarget {
        from,
        member_key,
        loc,
    } = match resolve_chord_target(state, me, areas, &origin) {
        Ok(target) => target,
        Err(reject) => {
            settle_chord_reject(&log_path, &settle, &origin, &reject);
            return;
        }
    };
    // Float/sticky subjects hold no tile slot: the resize settles with no
    // writes, never retiles them. Ordered ahead of the workspace fence to
    // match the Snap Move arm and KDE refusal priority (subject before
    // domain).
    if state.sticky.contains_key(&member_key) {
        let mut line = settle("resize-refused-sticky");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    if engine_is_float(state, &loc.output, &loc.workspace, &from) {
        let mut line = settle("resize-refused-floating");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    // Floating workspaces run no tile layout: the resize settles with no
    // writes and every frame stays native (KDE plan-adapter parity).
    if !workspace_mode_tiled(state, &loc.output, &loc.workspace) {
        let mut line = settle("resize-refused-workspace-floating");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    // Focused overlays refuse before any Engine mutation: a maximized or
    // fullscreen focused window keeps its native state and its reserved
    // slot (KDE maximize/fullscreen isolation parity; fullscreen wins).
    if let Some(row) = retained.iter().find(|r| r.key == member_key)
        && let Some(cause) = overlay_refusal_for(row)
    {
        let mut line = settle(if cause == "fullscreen" {
            "resize-refused-fullscreen"
        } else {
            "resize-refused-maximize"
        });
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    // Topology must name a live output for the mover; gaps ride
    // the report screens so no per-route adoption exists.
    if !areas.iter().any(|a| a.device == loc.output) {
        let mut line = settle("unknown-output");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    };
    // KDE `inFlight`/`r4Flight` parity: an outstanding retained window
    // transfer for the mover holds the domain. The core would retain the
    // conflicting intent anyway; the pre-dispatch refusal keeps the
    // outcome vocabulary exact and the repeat tracker untouched.
    let busy = state
        .world
        .engine()
        .retained_transfers()
        .iter()
        .any(|target| match &target.kind {
            tiler_core::world_report::TransferKind::Window(window) => window.0 == from,
            _ => false,
        });
    if busy {
        let mut line = settle("resize-refused-busy");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    if !state.member_tokens.values().any(|token| token == &from) {
        let mut line = settle("unmanaged");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    }
    // Exact KDE repeat tracker: continued only on a focused/direction/mode
    // match, restarted otherwise, advanced before the world dispatch.
    // `from` is the origin-verified token, never blind foreground.
    let direction = direction_name(intent.direction).to_owned();
    let mode = intent.mode.as_str().to_owned();
    let (press_index, repeat) =
        crate::tiling::resize_repeat_next(state.resize_repeat.clone(), &from, &direction, &mode);
    state.resize_repeat = Some(repeat);
    // Single-domain resizes run the world operation route: the core owns
    // direction semantics and the host diff applies the accepted reply
    // with fresh proof. No focus, order, or topology mutation beyond the
    // two adjacent shares.
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
        tiler_core::world_report::WorldCommand::Operation(CoreCommand::Resize {
            window: from.clone(),
            direction,
            mode,
            press_index,
        }),
    );
    let outcome = match reply {
        Some(_) => "resize-applied",
        None => "resize-unverified",
    };
    let mut line = settle(outcome);
    line["origin"] = serde_json::Value::from(origin.token.clone());
    line["window"] = serde_json::Value::from(from);
    line["press_index"] = serde_json::Value::from(press_index);
    log_json_at(&log_path, line);
}
