#![forbid(unsafe_code)]
use super::*;

/// Owner-side launcher backend seam: production spawns detached processes;
/// tests inject a recorder. Launching never runs in the hook callback (the
/// callback only queues through [`SnapClassify`]); the owner spawns here on
/// the owner thread, and `Command::spawn`/`ShellExecuteW` return without
/// waiting, so the pump never blocks.
pub trait LaunchBackend {
    fn launch(&self, kind: LaunchKind) -> std::result::Result<(), String>;
}

/// Production launcher backend: default browser via `ShellExecuteW`, Windows
/// Terminal via the `wt.exe` app execution alias. Launched processes never
/// inherit owner handles and never elevate: stdio is nulled, no handle list
/// is passed, and the spawn runs as the current (normal) user token.
pub struct SystemLaunchBackend;

impl LaunchBackend for SystemLaunchBackend {
    fn launch(&self, kind: LaunchKind) -> std::result::Result<(), String> {
        match kind {
            LaunchKind::Browser => launch_default_browser(),
            LaunchKind::Terminal => launch_windows_terminal(),
        }
    }
}

/// Open the default browser on an empty `https://` URL through the shell
/// `open` verb: the shell resolves the user's default browser, never a
/// hard-coded executable.
fn launch_default_browser() -> std::result::Result<(), String> {
    #[cfg(windows)]
    {
        // SW_SHOWNORMAL = 1 inside the boundary; `Err(rc)` carries the
        // `ShellExecuteW` return, as before.
        crate::ffi::open_url("https://")
            .map_err(|rc| format!("ShellExecuteW open https:// failed: {rc}"))
    }
    #[cfg(not(windows))]
    {
        Err("default browser launch is Windows-only".to_owned())
    }
}

/// Spawn Windows Terminal through its `wt.exe` app execution alias with
/// nulled stdio and no inherited handles. A missing alias is a narrow
/// logged error, never a crash and never a fallback launch.
fn launch_windows_terminal() -> std::result::Result<(), String> {
    std::process::Command::new("wt.exe")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("wt.exe spawn failed: {err}"))
}

/// Launcher dispatch (Win+B browser, Win+T terminal): fresh observation is
/// unnecessary (no window is touched), but the takeover and
/// suspend/elevated fences still apply, so a chord arriving while keyboard
/// takeover is off or the session is suspended/elevated settles without
/// spawning. The fence outcome is injected by the caller: the production
/// `keyboard_tick` arm passes the live `toggle_gate_outcome` read, while
/// tests pass an explicit outcome, so dispatch never depends on ambient
/// foreground state. The chord-time origin rides only for mask/queue parity
/// and is ignored: launchers act on no window. Only the down dispatches;
/// key-ups and repeats are trace-only. A failed spawn logs narrowly and keeps
/// the loop functioning.
pub(super) fn dispatch_launch_intent(
    state: &mut TileLoop,
    intent: QueuedLaunchIntent,
    backend: &dyn LaunchBackend,
    gate: Option<&'static str>,
) {
    let log_path = state.log_path.clone();
    // Key-ups close the pair and passed chords never dispatch: trace-only so
    // held-key traffic stays out of normal logs.
    if !intent.consumed || !intent.announce {
        if state.trace {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "launch",
                    "tick": state.tick,
                    "kind": intent.kind.as_str(),
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
            "event": "launch",
            "tick": tick,
            "correlation": correlation.as_str(),
            "kind": intent.kind.as_str(),
            "edge": intent.edge.as_str(),
            "disposition": "consumed",
            "outcome": outcome,
        })
    };
    // KDE `!enabled` parity: chords arriving while keyboard takeover is off
    // never dispatch (the classifier already passes them through; this
    // covers the exact-owner test-needed route, which bypasses it).
    if !state.keyboard.takeover {
        let line = settle("launch-refused-disabled");
        log_json_at(&log_path, line);
        return;
    }
    // Fresh per-intent suspension/elevation fence (workspace_tick parity),
    // injected precomputed by the caller: newly intercepted
    // suspended/elevated chords settle here with no side effects.
    if let Some(outcome) = gate {
        let line = settle(outcome);
        log_json_at(&log_path, line);
        return;
    }
    match backend.launch(intent.kind) {
        Ok(()) => {
            let line = settle("launched");
            log_json_at(&log_path, line);
        }
        Err(err) => {
            let mut line = settle("launch-failed");
            line["error"] = serde_json::Value::from(err);
            log_json_at(&log_path, line);
        }
    }
}

/// World command for one Win+G: always the ordinary float toggle, including
/// sticky origins. A sticky subject rides the same command so the core clears
/// sticky and tiles through its composed sticky transition; no adapter-side
/// `SetSticky` compensation exists.
fn float_toggle_command(
    token: &str,
    was_floating: bool,
    live_frame: impl FnOnce() -> Option<Rect>,
) -> tiler_core::world_report::WorldCommand {
    tiler_core::world_report::WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: tiler_core::directional::WindowSubject::Window(tiler_core::directional::WindowId(
            token.to_owned(),
        )),
        float_rect: if was_floating { live_frame() } else { None },
    })
}

/// Win+G float toggle (KDE Meta+G parity): fresh observation, exact origin
/// re-resolution, full gates, one world-owned `ToggleFloat` whose applied
/// reply the host diff actuates with fresh proof, then sibling reflow through
/// the shared write path. No compensating toggle and no adapter pre-veto: the
/// core alone resolves workspace-mode and overlay holds.
pub(super) fn dispatch_float_intent(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    intent: crate::snapkey::QueuedFloatIntent,
) {
    let log_path = state.log_path.clone();
    // Key-ups close the pair and passed chords never dispatch: trace-only so
    // held-key traffic stays out of normal logs.
    if !intent.consumed || !intent.announce {
        if state.trace {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "float-toggle",
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
    let settle = |outcome: &'static str| {
        serde_json::json!({
            "event": "float-toggle",
            "tick": tick,
            "correlation": correlation.as_str(),
            "edge": intent.edge.as_str(),
            "disposition": "consumed",
            "outcome": outcome,
        })
    };
    let Some(origin) = intent.origin.clone() else {
        state.snap_advance = None;
        let line = settle("origin-vanished");
        log_json_at(&log_path, line);
        return;
    };
    // Fresh per-intent suspension/elevation fence (workspace_tick parity):
    // newly intercepted suspended/elevated chords settle here with no side
    // effects. The suspend read exempts verified managed overlays; the target
    // revalidation below refuses protected windows.
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
    let ChordObserved { observed, retained } = chord;
    let ChordTarget {
        from,
        member_key,
        loc,
        ..
    } = match resolve_chord_target(state, me, areas, &origin) {
        Ok(target) => target,
        Err(reject) => {
            settle_chord_reject(&log_path, &settle, &origin, &reject);
            return;
        }
    };
    let is_float = engine_is_float(state, &loc.output, &loc.workspace, &from);
    // No adapter pre-veto: the core alone resolves workspace-mode and
    // fullscreen/maximized/overlay holds from its staged facts (R-MAX-10,
    // R-MAX-11, B9), and this seam mutates no local intent table.
    //
    // Only unfloat carries a live frame; on a tiled subject an explicit
    // rectangle would override core's retained or centered placement.
    // Sticky subjects still take core's composed transition.
    let command = float_toggle_command(&from, is_float, || {
        observed
            .iter()
            .find(|w| w.hwnd == member_key.hwnd)
            .map(|w| w.visible)
            .or_else(|| {
                retained
                    .iter()
                    .find(|r| r.key == member_key)
                    .and_then(|row| row.rect)
            })
    });
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
        command,
        super::observation::sample_cursor(),
    );
    // Frame cache only (never intent): a floating applied row publishes the
    // effective float frame for later retained-placement reads.
    if let Some(outcome) = reply.as_ref()
        && let Some(row) = outcome
            .reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == from)
        && row.floating
        && let Some(frame) = row.effective_frame.or(row.allocation)
    {
        state.float_rects.insert(from.clone(), frame);
    }
    let (outcome, target) = match reply.as_ref() {
        None => (
            "float-unverified",
            if is_float { "unfloated" } else { "floated" },
        ),
        Some(outcome) => match outcome.result.as_ref() {
            Some(tiler_core::world_report::CommandResult::Float { no_space: true, .. }) => {
                ("unfloat-no-space", "floated")
            }
            Some(tiler_core::world_report::CommandResult::Float { rect: Some(_), .. }) => {
                ("float-applied", "floated")
            }
            Some(tiler_core::world_report::CommandResult::Float { rect: None, .. }) => {
                ("unfloat-applied", "unfloated")
            }
            Some(tiler_core::world_report::CommandResult::Rejected { .. }) => ("rejected", "float"),
            _ => ("float-applied", "floated"),
        },
    };
    let mut line = settle(outcome);
    line["origin"] = serde_json::Value::from(origin.token.clone());
    line["window"] = serde_json::Value::from(from.clone());
    line["target"] = serde_json::Value::from(target);
    if let Some(tiler_core::world_report::CommandResult::Rejected { kind }) =
        reply.as_ref().and_then(|outcome| outcome.result.as_ref())
    {
        line["cause"] = serde_json::Value::from(kind.as_str());
    }
    log_json_at(&log_path, line);
}

/// Win+O orientation toggle (KDE Meta+O parity, item 4 R-LAY-01): fresh
/// observation, exact origin re-resolution, full gates, one Engine
/// `ToggleOrientation` on the retained session, then sibling reflow through
/// the shared write path. No focus or view mutation: focus stays on the same
/// window and the workspace never switches. Lone root, float/sticky
/// subjects, missing focus, floating workspaces, and focused maximized
/// overlays settle with no writes; a focused fullscreen subject flips its
/// parent while its own row keeps the overlay's no-write fence, and sibling
/// overlays reproject as reserved slots and skip native writes through the
/// existing writable fence.
pub(super) fn dispatch_orientation_intent(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    intent: crate::snapkey::QueuedOrientationIntent,
) {
    let log_path = state.log_path.clone();
    // Key-ups close the pair and passed chords never dispatch: trace-only so
    // held-key traffic stays out of normal logs.
    if !intent.consumed || !intent.announce {
        if state.trace {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "orientation-toggle",
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
    let settle = |outcome: &'static str| {
        serde_json::json!({
            "event": "orientation-toggle",
            "tick": tick,
            "correlation": correlation.as_str(),
            "edge": intent.edge.as_str(),
            "disposition": "consumed",
            "outcome": outcome,
        })
    };
    let Some(origin) = intent.origin.clone() else {
        state.snap_advance = None;
        let line = settle("origin-vanished");
        log_json_at(&log_path, line);
        return;
    };
    // Fresh per-intent suspension/elevation fence (workspace_tick parity):
    // newly intercepted suspended/elevated chords settle here with no side
    // effects. The suspend read exempts verified managed overlays, and the
    // target revalidation below refuses protected windows.
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
    let ChordObserved { observed, retained } = chord;
    let ChordTarget {
        from,
        member_key,
        loc,
        ..
    } = match resolve_chord_target(state, me, areas, &origin) {
        Ok(target) => target,
        Err(reject) => {
            settle_chord_reject(&log_path, &settle, &origin, &reject);
            return;
        }
    };
    // Floating workspaces run no tile layout: the toggle settles with no
    // writes and every frame stays native.
    if !workspace_mode_tiled(state, &loc.output, &loc.workspace) {
        state.snap_advance = None;
        let mut line = settle("workspace-floating");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    // Float/sticky subjects hold no tile slot: the toggle settles with no
    // writes, never retiles them.
    if state.sticky_of(&member_key) {
        let mut line = settle("orientation-refused-sticky");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    if engine_is_float(state, &loc.output, &loc.workspace, &from) {
        let mut line = settle("orientation-refused-floating");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    // Focused maximized overlays refuse before any Engine mutation: a
    // maximized non-fullscreen focused window keeps its native state and its
    // reserved slot. A focused fullscreen member dispatches: the core flips
    // the reserved parent split while the fullscreen row keeps its no-write
    // fence.
    if let Some(refusal) = orientation_overlay_refusal(&observed, &retained, &member_key) {
        let mut line = settle(refusal);
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
    let known_output = areas.iter().any(|a| a.device == loc.output);
    if !known_output {
        let mut line = settle("unknown-output");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    };
    // World-routed toggle: the core owns orientation planning and the host
    // diff applies the accepted reply (sibling reflow included) with fresh
    // proof. No focus or view mutation. The applied reply IS the outcome:
    // changed allocations report applied, identical ones a no-op, and a
    // rejected or lost command reports unverified.
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
        tiler_core::world_report::WorldCommand::Operation(CoreCommand::ToggleOrientation {
            window: tiler_core::directional::WindowSubject::Window(
                tiler_core::directional::WindowId(from.clone()),
            ),
        }),
        super::observation::sample_cursor(),
    );
    let outcome = match reply {
        Some(reply) => {
            let changed = reply.reply.windows.iter().any(|row| {
                row.allocation.is_some_and(|alloc| {
                    observed
                        .iter()
                        .find(|w| w.token == row.lifetime.0)
                        .is_none_or(|w| w.visible != alloc)
                })
            });
            if changed {
                "orientation-applied"
            } else {
                "orientation-noop"
            }
        }
        None => "orientation-unverified",
    };
    let mut line = settle(outcome);
    line["origin"] = serde_json::Value::from(origin.token.clone());
    line["window"] = serde_json::Value::from(from.clone());
    log_json_at(&log_path, line);
}

/// Current-workspace tiling toggle dispatch (Win+Y, KDE Meta+Y parity, item
/// 21): the shortcut arm of the existing tray checkbox path. The toggle acts
/// on the active workspace of the active output through the shared
/// [`toggle_workspace_tiling`] (same per-workspace state, same release
/// domain, same pending-release coverage); the chord-time origin rides only
/// for mask/queue parity and is ignored, so no focus or window policy is
/// added. Only the down dispatches; key-ups and repeats are trace-only.
/// `gate` is the live suspend/elevated fence outcome injected by the
/// `keyboard_tick` call site (see [`dispatch_launch_intent`]), so dispatch
/// never depends on ambient foreground state and owner-dispatch tests stay
/// deterministic on headless runners.
#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch_workspace_tiling_intent(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    intent: crate::snapkey::QueuedWorkspaceTilingIntent,
    gate: Option<&'static str>,
) {
    let log_path = state.log_path.clone();
    // Key-ups close the pair and passed chords never dispatch: trace-only so
    // held-key traffic stays out of normal logs.
    if !intent.consumed || !intent.announce {
        if state.trace {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-tiling-toggle",
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
    let settle = |outcome: &'static str| {
        serde_json::json!({
            "event": "workspace-tiling-toggle",
            "tick": tick,
            "correlation": correlation.as_str(),
            "edge": intent.edge.as_str(),
            "disposition": "consumed",
            "outcome": outcome,
        })
    };
    // KDE `!enabled` parity: chords arriving while keyboard takeover is off
    // never dispatch (the classifier already passes them through; this
    // covers the exact-owner test-needed route, which bypasses it).
    if !state.keyboard.takeover {
        let line = settle("workspace-tiling-refused-disabled");
        log_json_at(&log_path, line);
        return;
    }
    // Fresh per-intent suspension/elevation fence (workspace_tick parity),
    // injected precomputed by the caller: newly intercepted
    // suspended/elevated chords settle here with no side effects.
    if let Some(outcome) = gate {
        let line = settle(outcome);
        log_json_at(&log_path, line);
        return;
    }
    // Same live scope the tray checkbox toggles: the active workspace of the
    // active output. Without a readable scope the chord settles with no side
    // effects (never a guessed fallback).
    let output = state.active_output.clone();
    if output.is_empty() {
        let line = settle("no-scope");
        log_json_at(&log_path, line);
        return;
    }
    toggle_workspace_tiling(state, me, store, dir, fulls, areas, &output);
}

/// Win+Shift+G sticky toggle: the adapter fences (suspend/elevated, fresh
/// observation, origin re-resolution) run here, then the world-owned
/// `ToggleSticky` command decides the sticky-on versus sticky-off direction,
/// the float/tile conversion, and the rehome to the current workspace.
/// Workspace-mode and fullscreen/maximized/overlay holds refuse in the core;
/// failures commit nothing and never retry.
pub(super) fn dispatch_sticky_intent(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    intent: crate::snapkey::QueuedStickyIntent,
) {
    let log_path = state.log_path.clone();
    if !intent.consumed || !intent.announce {
        if state.trace {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "sticky-toggle",
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
    let settle = |outcome: &'static str| {
        serde_json::json!({
            "event": "sticky-toggle",
            "tick": tick,
            "correlation": correlation.as_str(),
            "edge": intent.edge.as_str(),
            "disposition": "consumed",
            "outcome": outcome,
        })
    };
    let Some(origin) = intent.origin.clone() else {
        state.snap_advance = None;
        let line = settle("origin-vanished");
        log_json_at(&log_path, line);
        return;
    };
    // Fresh per-intent suspension/elevation fence (workspace_tick parity):
    // newly intercepted suspended/elevated chords settle here with no side
    // effects; the world command below only runs for a live managed member.
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
        observed, retained, ..
    } = chord;
    let ChordTarget { from, .. } = match resolve_chord_target(state, me, areas, &origin) {
        Ok(target) => target,
        Err(reject) => {
            settle_chord_reject(&log_path, &settle, &origin, &reject);
            return;
        }
    };
    // No adapter pre-veto: the core alone resolves workspace-mode and
    // fullscreen/maximized/overlay holds for the sticky toggle from its
    // staged facts.
    // World-owned toggle: the core resolves the retained sticky state, the
    // float/tile conversion, and the current-workspace rehome from its
    // staged facts, and the applied reply actuates the retained targets
    // through the single apply pass. The applied reply IS the outcome; this
    // seam never mutates the local sticky/float tables or markers.
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
        tiler_core::world_report::WorldCommand::ToggleSticky {
            window: tiler_core::directional::WindowSubject::Window(
                tiler_core::directional::WindowId(from.clone()),
            ),
        },
        super::observation::sample_cursor(),
    );
    let (outcome, target) = super::keyboard_overlays::state_toggle_labels(
        reply.as_ref(),
        &from,
        tiler_core::world_report::StateProperty::Sticky,
        "sticky",
    );
    let mut line = settle(outcome);
    line["origin"] = serde_json::Value::from(origin.token.clone());
    line["window"] = serde_json::Value::from(from.clone());
    line["target"] = serde_json::Value::from(target);
    if let Some(tiler_core::world_report::CommandResult::Rejected { kind }) =
        reply.as_ref().and_then(|outcome| outcome.result.as_ref())
    {
        line["cause"] = serde_json::Value::from(kind.as_str());
    }
    log_json_at(&log_path, line);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiler_core::boundary::CoreCommand;
    use tiler_core::geometry::Rect;

    #[test]
    fn win_g_always_sends_toggle_float() {
        // Sticky origins ride the same command: the core clears sticky and
        // tiles through its composed transition, so no adapter-side
        // `SetSticky(false)` may reappear here.
        let rect = Rect {
            x: 10,
            y: 20,
            w: 300,
            h: 200,
        };
        let command = float_toggle_command("win-a", true, || Some(rect));
        let tiler_core::world_report::WorldCommand::Operation(CoreCommand::ToggleFloat {
            window: tiler_core::directional::WindowSubject::Window(window),
            float_rect,
        }) = command
        else {
            panic!("Win+G must map to ToggleFloat only");
        };
        assert_eq!(window.0, "win-a");
        assert_eq!(float_rect, Some(rect));
    }

    #[test]
    fn win_g_on_a_tile_leaves_placement_to_core() {
        let command = float_toggle_command("win-a", false, || {
            panic!("tiled geometry must not override float placement")
        });
        assert!(matches!(
            command,
            tiler_core::world_report::WorldCommand::Operation(CoreCommand::ToggleFloat {
                float_rect: None,
                ..
            })
        ));
    }
}
