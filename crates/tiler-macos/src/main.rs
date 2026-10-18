#![forbid(unsafe_code)]

//! `tiler-macos` capability-probe CLI.
//!
//! `help` / `check` / `preflight` (default) are nonmutating and never launch
//! UI, taps, AX control, or Space moves. All other modes are explicit and
//! bounded; owned mutations validate live probe-target identity first, and
//! every success report carries measured effects (OS status codes, counts,
//! readbacks) — never bare claims.

use tiler_macos::cli::{Command, parse_args};

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let request = match parse_args(&argv) {
        Ok(r) => r,
        Err(message) => {
            eprintln!("error: {message}");
            eprintln!("{}", tiler_macos::cli::help_text());
            std::process::exit(2);
        }
    };
    match run(request) {
        Ok(output) => println!("{output}"),
        Err((code, message)) => {
            eprintln!("{message}");
            std::process::exit(code);
        }
    }
}

fn run(request: tiler_macos::cli::CliRequest) -> Result<String, (i32, String)> {
    match request.command {
        Command::Help => Ok(tiler_macos::cli::help_text()),
        Command::SpacesGrabDiagnose => tiler_macos::space_grab_diagnose::run(),
        Command::McRemoveEmpty {
            space,
            allow_untested,
        } => tiler_macos::workspace_select::remove_empty(space, allow_untested),
        Command::SpacesSelect { display, actions } => {
            tiler_macos::workspace_select::run(display, &actions)
        }
        Command::CgInspect => {
            let windows = tiler_macos::ffi::cg_window_list_basic().map_err(|e| (1, e))?;
            Ok(serde_json::json!({"windows":windows,"session":tiler_macos::ffi::session_state(),"mutates":false}).to_string())
        }
        Command::Preflight => preflight(),
        Command::AxEnumerate { max_windows } => {
            let counts =
                tiler_macos::ax::enumerate_counts(max_windows).map_err(|e| (1, e.to_string()))?;
            serde_json::to_string(&counts).map_err(|e| (1, e.to_string()))
        }
        Command::AxObserve {
            pid,
            nonce,
            timeout_ms,
        } => {
            let cfg = tiler_macos::config::ProbeConfig {
                modifier: request.modifier,
                tap_seconds: tiler_macos::config::DEFAULT_TAP_SECONDS,
                ax_timeout_ms: timeout_ms,
                max_windows: tiler_macos::config::DEFAULT_MAX_WINDOWS,
            };
            cfg.validate().map_err(|e| (2, e))?;
            // Owned-session authentication first: arbitrary/foreign PIDs
            // never reach AX.
            let owned = tiler_macos::ax::authorize_owned_action(pid, &nonce)
                .map_err(|e| (2, e.to_string()))?;
            // Engine idle covers the whole bounded AX wait plus slack; a
            // short idle must never drop events mid-observation.
            let idle = std::time::Duration::from_millis(timeout_ms.saturating_add(10_000));
            let (engine, handle) = tiler_macos::engine::Engine::spawn_thread(idle);
            let report = tiler_macos::ax::observe_app(&owned, pid, timeout_ms, &engine.tx)
                .map_err(|e| (1, e.to_string()))?;
            let _ = engine.try_send(tiler_macos::engine::EngineMessage::Shutdown);
            let state = handle
                .join()
                .map_err(|_| (1, "engine thread failed".to_owned()))?;
            Ok(serde_json::json!({
                "observed_pid": report.pid,
                "timeout_ms": report.timeout_ms,
                "notifications_added": report.notifications_added,
                "notifications_received": report.notifications_received,
                "messaging_timeout_set": report.messaging_timeout_set,
                "engine_ax_events": state.ax_events,
            })
            .to_string())
        }
        Command::AxSession { seconds } => {
            // Bounded Child-holder AX session: focus-restoration proof first,
            // then driven observer/frame/focus legs on the exact held
            // children plus a fixed-size partial-write leg. No cross-command
            // title auth inside; no hotkeys, taps, or Space writes.
            tiler_macos::ax_session::run_ax_session(seconds)
        }
        Command::FocusSession { seconds, appkit } => {
            // Bounded Child-holder SLPS focus session: original preimage
            // with workspace+AX+CG agreement, SLPS focus with agreement,
            // SAME-route restore with agreement. No activation API, taps,
            // keys, SA, or Space writes. Default raw init; --appkit opts
            // into the Accessory lifecycle for discrimination.
            tiler_macos::focus::run_focus_session(seconds, appkit)
        }
        Command::FocusDiagnose { fallback, appkit } => {
            // Read-only focus-oracle diagnosis: workspace frontmost
            // before/after plus the system-wide AX leg beside the
            // independent direct per-application leg with a title-free CG
            // join. IDs/counts/statuses only; no dispatch, no activation,
            // no taps, no Space writes, no attribute writes. Default raw
            // systemwide+direct legs only; --fallback opts into the
            // experimental unproven evidence, --appkit into Accessory init.
            tiler_macos::focus_diagnose::diagnose_focus(fallback, appkit)
        }
        Command::AxSetFrame { pid, nonce, wid } => {
            // Same authenticated spawn record as stop (pid/nonce, expected
            // helper, live bundle-or-exe plus CURRENT live window nonce)
            // plus WID identity join inside.
            let owned = tiler_macos::ax::authorize_owned_action(pid, &nonce)
                .map_err(|e| (2, e.to_string()))?;
            // TODO(ax-set-frame): frame geometry CLI flags; Phase 0 applies a
            // fixed +40pt diagonal nudge to prove set/readback/restore.
            let frame = tiler_macos::ffi::AxFrame {
                x: 240.0,
                y: 240.0,
                w: 480.0,
                h: 320.0,
            };
            let report = tiler_macos::ax::set_frame_owned(&owned, pid, wid, frame)
                .map_err(|e| (1, e.to_string()))?;
            if !report.restored {
                return Err((
                    1,
                    format!(
                        "set succeeded but RESTORE FAILED (window left modified): {}",
                        serde_json::to_string(&report).unwrap_or_default()
                    ),
                ));
            }
            serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
        }
        Command::AxRaise { pid, nonce, wid } => {
            // Same authenticated spawn record as stop; refuses when the
            // focus preimage is foreign (never steals focus) and reports a
            // measured focus readback, never bare OS acceptance.
            let owned = tiler_macos::ax::authorize_owned_action(pid, &nonce)
                .map_err(|e| (2, e.to_string()))?;
            let report =
                tiler_macos::ax::raise_owned(&owned, pid, wid).map_err(|e| (1, e.to_string()))?;
            if !report.focus_verified || !report.focus_restored {
                return Err((
                    1,
                    format!(
                        "raise focus unverified: {}",
                        serde_json::to_string(&report).unwrap_or_default()
                    ),
                ));
            }
            serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
        }
        Command::InputTap { seconds, suppress } => {
            let idle = std::time::Duration::from_secs(seconds.saturating_add(10));
            let (engine, handle) = tiler_macos::engine::Engine::spawn_thread(idle);
            let guard = tiler_macos::input::intercept_bounded(
                request.modifier,
                tiler_macos::input::PROBE_KEY_CODE,
                seconds,
                suppress,
                &engine.tx,
            )
            .map_err(|e| (1, e.to_string()))?;
            let _ = engine.try_send(tiler_macos::engine::EngineMessage::Shutdown);
            let state = handle
                .join()
                .map_err(|_| (1, "engine thread failed".to_owned()))?;
            Ok(serde_json::json!({
                "modifier": request.modifier.as_str(),
                "tap": guard.report,
                "suppress_armed": suppress,
                "tap_released": guard.is_disabled(),
                "engine_input_events": state.input_events,
                "engine_tap_disabled_events": state.tap_disabled_events,
                "contract": if suppress {
                    "armed: exact-chord KeyDown/KeyUp pairs suppressed; unrelated keys, inexact modifiers, lone ups, FlagsChanged always pass"
                } else {
                    "observe-only: Keep always, exact-chord matches counted only"
                },
            })
            .to_string())
        }
        Command::InputSession {
            seconds,
            route,
            post_route,
            stall_once_ms,
            physical_stall,
            direct_disable_hold_ms,
            direct_rearm,
            disable_once,
            watchdog_only,
        } => {
            // Bounded owned input fixture over the REAL native tile source
            // (input LIVE only: no tiling, no frame writes). LaunchServices
            // launch via the `just launch` recipe; the holder validates
            // oracle + ownership before every synthetic post. The
            // `--watchdog-only` fault mode arms the tap with no helper/posts
            // and parks past the hard watchdog (dirty exit is the evidence).
            let request = tiler_macos::input_session::InputSessionRequest {
                seconds,
                route,
                post_route,
                stall_once_ms,
                physical_stall,
                direct_disable_hold_ms,
                direct_rearm,
                disable_once,
                watchdog_only,
                modifier: request.modifier,
            };
            tiler_macos::input_session::run_input_session(request)
        }
        Command::SecureInput => {
            let status = tiler_macos::input::secure_input();
            Ok(serde_json::json!({"secure_input": format!("{status:?}")}).to_string())
        }
        Command::SpacesDiagnose { wid, pid, nonce } => {
            let diagnosis = tiler_macos::spaces::diagnose().map_err(|e| (1, e.to_string()))?;
            let membership = match (wid, pid, nonce) {
                (Some(w), Some(p), Some(n)) => {
                    // Owned path: authenticate the session and join the WID
                    // to the owned PID via CGWindowList (no AX) before the
                    // SLS read.
                    let owned = tiler_macos::ax::authorize_owned_action(p, &n)
                        .map_err(|e| (2, e.to_string()))?;
                    let spaces = tiler_macos::spaces::window_spaces_owned(&owned, p, w)
                        .map_err(|e| (1, e.to_string()))?;
                    serde_json::json!({"wid": w, "spaces": spaces, "authenticated": true})
                }
                (Some(w), _, _) => {
                    let spaces =
                        tiler_macos::spaces::window_spaces(w).map_err(|e| (1, e.to_string()))?;
                    serde_json::json!({"wid": w, "spaces": spaces, "authenticated": false})
                }
                (None, _, _) => serde_json::Value::Null,
            };
            Ok(serde_json::json!({
                "connection": diagnosis.connection,
                "displays": diagnosis.displays,
                "space_count": diagnosis.space_count(),
                "window_membership": membership,
            })
            .to_string())
        }
        Command::CgOwnedWindows { pid, nonce } => {
            // Read-only CGWindowList identity for the owned PID (no AX).
            let owned = tiler_macos::ax::authorize_owned_action(pid, &nonce)
                .map_err(|e| (2, e.to_string()))?;
            let windows =
                tiler_macos::cg::owned_windows(&owned, pid).map_err(|e| (1, e.to_string()))?;
            Ok(serde_json::json!({
                "authenticated": true,
                "pid": pid,
                "windows": windows,
                "ax_used": false,
                "source": "CGWindowListCopyWindowInfo via maintained core-graphics binding (OptionAll, IDs/geometry only)",
            })
            .to_string())
        }
        Command::SpacesMove {
            pid,
            nonce,
            wid,
            space,
            allow_untested,
        } => {
            // Direct-SLS owned move probe (compat disabled, SA omitted).
            // Experimental live (one gated dispatch observed no-effect on an
            // owned fixture; single-shot inverse unverified): the report
            // separates void dispatch from the
            // effect (exact-equality membership readback). Never
            // auto-restores: run spaces-restore separately so the independent
            // restore path is proven on its own (usable after this process
            // exits). Ownership refusals exit 2 like other identity gates.
            let owned = tiler_macos::ax::authorize_owned_action(pid, &nonce)
                .map_err(|e| (2, e.to_string()))?;
            let report = tiler_macos::spaces::move_owned_window_to_space(
                &owned,
                pid,
                &nonce,
                wid,
                space,
                allow_untested,
            )
            .map_err(|e| match e {
                tiler_macos::spaces::SpacesError::Ownership(r) => (2, r),
                other => (1, other.to_string()),
            })?;
            let body = serde_json::json!({
                "report": report,
                "live_evidence": if report.effect_verified {
                    "dispatched plus exact-equality effect readback verified"
                } else {
                    "dispatched but effect UNVERIFIED — run spaces-restore with the preimage before proceeding"
                },
                "restore_hint": format!(
                    "spaces-restore --pid {pid} --nonce {nonce} --wid {} --space {} --allow-untested",
                    report.wid,
                    report.preimage_spaces.first().copied().unwrap_or(0),
                ),
            });
            if !report.effect_verified {
                return Err((1, body.to_string()));
            }
            Ok(body.to_string())
        }
        Command::SpacesRestore {
            pid,
            nonce,
            wid,
            space,
            allow_untested,
        } => {
            // Independent restore with the same full record validation as
            // the move (entry proof plus unchanged-record revalidation at
            // the write boundary and before the readback). Usable after the
            // moving process exits. Live proof still pending.
            let owned = tiler_macos::ax::authorize_owned_action(pid, &nonce)
                .map_err(|e| (2, e.to_string()))?;
            let report = tiler_macos::spaces::restore_owned_window_to_space(
                &owned,
                pid,
                &nonce,
                wid,
                space,
                allow_untested,
            )
            .map_err(|e| match e {
                tiler_macos::spaces::SpacesError::Ownership(r) => (2, r),
                other => (1, other.to_string()),
            })?;
            let body = serde_json::json!({
                "report": report,
                "live_evidence": if report.effect_verified {
                    "restore dispatched plus exact-equality effect readback verified"
                } else {
                    "restore dispatched but effect UNVERIFIED — preserve evidence and report residue"
                },
            });
            if !report.effect_verified {
                return Err((1, body.to_string()));
            }
            Ok(body.to_string())
        }
        Command::SpacesSwitch { pid, nonce, space } => {
            // Explicitly unsupported: no implemented switch route with
            // proven recovery (native legs need Dock injection; surveyed
            // non-SA legs are global synthetic gestures, AX Mission Control
            // automation, or posted system chords). Authenticate first
            // so foreign sessions fail as ownership errors, then report the
            // route. Missing implementation/recovery is never a SIP rejection.
            tiler_macos::ax::authorize_owned_action(pid, &nonce).map_err(|e| (2, e.to_string()))?;
            let status = tiler_macos::spaces::switch_space_status(space);
            Err((
                1,
                serde_json::to_string(&status).unwrap_or_else(|_| status.reason.clone()),
            ))
        }
        Command::SpacesCreate { pid, nonce } => {
            tiler_macos::ax::authorize_owned_action(pid, &nonce).map_err(|e| (2, e.to_string()))?;
            let status = tiler_macos::spaces::create_space_status();
            Err((
                1,
                serde_json::to_string(&status).unwrap_or_else(|_| status.reason.clone()),
            ))
        }
        Command::SpacesDestroy { pid, nonce, space } => {
            tiler_macos::ax::authorize_owned_action(pid, &nonce).map_err(|e| (2, e.to_string()))?;
            let status = tiler_macos::spaces::destroy_space_status(space);
            Err((
                1,
                serde_json::to_string(&status).unwrap_or_else(|_| status.reason.clone()),
            ))
        }
        Command::ProbeSpawn {
            hung_ms,
            slow_ms,
            seconds,
            fixed_size,
            regular_policy,
            close_window_ms,
            transient_panel_ms,
            transient_sheet_ms,
            space_join,
        } => {
            let nonce = tiler_macos::probe_target::mint_nonce().map_err(|e| (1, e))?;
            let spec =
                tiler_macos::probe_target::ProbeTargetSpec::new(&nonce, hung_ms, slow_ms, seconds)
                    .map(|s| {
                        s.with_fixed_size(fixed_size)
                            .with_regular_policy(regular_policy)
                            .with_close_window_ms(close_window_ms)
                            .with_transient_panel_ms(transient_panel_ms)
                            .with_transient_sheet_ms(transient_sheet_ms)
                            .with_space_join(space_join)
                    })
                    .map_err(|e| (2, e))?;
            // Hold the helper stdout pipe open for the session lifetime: the
            // helper prints periodic snapshots, and a closed read end would
            // SIGPIPE-kill it right after `ready` (observed live). The drain
            // thread discards lines; identity already came from the
            // handshake below.
            let mut raw = tiler_macos::probe_target::start_helper_process(&spec)
                .map_err(|e| (1, format!("spawn failed: {e}")))?;
            let bundle = tiler_macos::probe_target::await_handshake(&mut raw, &spec)
                .map_err(|e| (1, format!("spawn failed: {e}")))?;
            if let Err(e) =
                tiler_macos::probe_target::record_spawn(raw.pid, &spec, &raw.helper_path)
            {
                let _ = raw.child.kill();
                let _ = raw.child.wait();
                return Err((1, format!("spawn failed: {e}")));
            }
            // Drain the helper stdout pipe for the session lifetime (moves
            // only the lines channel); the Child and its identity move into
            // the session next. Every helper line is forwarded verbatim to
            // this holder's own stdout (flushed): the downstream key-count
            // snapshots and first-party close markers are the harness's
            // delivery/close evidence, read from the LaunchServices output
            // file — discarding them would blind the fixture AND let the
            // pipe back up. Only JSON the helper itself printed ever passes
            // through here; the holder adds its own ready/final lines.
            let tiler_macos::probe_target::UnreadySpawn {
                child,
                pid: helper_pid,
                helper_path: helper_canonical,
                lines,
                line_drops: _,
            } = raw;
            std::thread::Builder::new()
                .name("probe-drain".to_owned())
                .spawn(move || {
                    use std::io::Write as _;
                    // Fallible forward: a closed holder stdout (EPIPE) must
                    // never panic-kill the drain (which would SIGPIPE-kill
                    // the helper). Keep draining after a closed stdout so
                    // the helper pipe never backs up; Child cleanup stays
                    // with the exact-Child session below on every path.
                    let stdout = std::io::stdout();
                    let mut out = stdout.lock();
                    while let Ok(line) = lines.recv() {
                        let _ = writeln!(out, "{line}");
                        let _ = out.flush();
                    }
                })
                .map_err(|e| (1, format!("spawn failed: drain spawn: {e}")))?;
            let mut session = tiler_macos::probe_target::SpawnSession {
                child,
                pid: helper_pid,
                nonce: spec.nonce.clone(),
                helper_path: helper_canonical,
                bundle,
            };
            // Expose the ready identity while the owned child is alive so
            // bounded companion commands can authenticate the same session.
            // The bundle is the handshake-verified one (probe or helper).
            println!(
                "{}",
                serde_json::json!({
                    "ready": true,
                    "bundle": session.bundle,
                    "pid": session.pid,
                    "nonce": session.nonce,
                })
            );
            // Bounded owned session: wait out the probe lifetime, then stop
            // exactly the spawned child handle (never a PID guess). Every
            // exit below reuses the shared bounded exact-Child cleanup: an
            // already-exited handle is never signalled, reap runs only
            // after an observed exit (never an unbounded wait on a live
            // child), and the record is removed only on a verified exit.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(spec.seconds);
            let mut exited_early = false;
            let mut wait_failed: Option<String> = None;
            while std::time::Instant::now() < deadline {
                match session.child.try_wait() {
                    Ok(Some(_)) => {
                        exited_early = true;
                        break;
                    }
                    Ok(None) => std::thread::sleep(std::time::Duration::from_millis(200)),
                    Err(e) => {
                        wait_failed = Some(format!("probe wait failed: {e}"));
                        break;
                    }
                }
            }
            if let Some(err) = wait_failed {
                let stop = tiler_macos::session::stop_exact_child(&mut session.child, session.pid);
                let wait_note = stop
                    .wait_error
                    .as_deref()
                    .map(|e| format!("; wait state: {e}"))
                    .unwrap_or_default();
                if stop.exited {
                    return Err((
                        1,
                        format!(
                            "{err}; exact child cleaned (reaped={} record_removed={}){wait_note}",
                            stop.reaped, stop.record_removed
                        ),
                    ));
                }
                return Err((
                    1,
                    format!(
                        "{err}; exact child did not exit bounded; record retained for inspection{wait_note}"
                    ),
                ));
            }
            let stop = tiler_macos::session::stop_exact_child(&mut session.child, session.pid);
            if !stop.exited {
                let wait_note = stop
                    .wait_error
                    .as_deref()
                    .map(|e| format!("; wait state: {e}"))
                    .unwrap_or_default();
                return Err((
                    1,
                    format!(
                        "probe stop: exact child did not exit bounded; record retained for inspection{wait_note}"
                    ),
                ));
            }
            Ok(serde_json::json!({
                "bundle": session.bundle,
                "pid": session.pid,
                "nonce": session.nonce,
                "exited_early": exited_early,
                "stopped_exact_child": !exited_early,
            })
            .to_string())
        }
        Command::ProbeSpawnLs {
            hung_ms,
            slow_ms,
            seconds,
            fixed_size,
            close_window_ms,
            transient_panel_ms,
            transient_sheet_ms,
            space_join,
        } => {
            // LaunchServices fixture holder: no Child is held (LaunchServices
            // owns the process), so currency is the spawn record plus the
            // live executable plus the CURRENT window-nonce title — the same
            // cross-command contract `probe-stop` and every owned path use.
            let nonce = tiler_macos::probe_target::mint_nonce().map_err(|e| (1, e))?;
            let spec =
                tiler_macos::probe_target::ProbeTargetSpec::new(&nonce, hung_ms, slow_ms, seconds)
                    .map(|s| {
                        s.with_fixed_size(fixed_size)
                            .with_close_window_ms(close_window_ms)
                            .with_transient_panel_ms(transient_panel_ms)
                            .with_transient_sheet_ms(transient_sheet_ms)
                            .with_space_join(space_join)
                    })
                    .map_err(|e| (2, e))?;
            let session = tiler_macos::probe_target::spawn_verified_ls(&spec, seconds)
                .map_err(|e| (1, format!("LS spawn failed: {e}")))?;
            println!(
                "{}",
                serde_json::json!({
                    "ready": true,
                    "ls": true,
                    "bundle": session.bundle,
                    "pid": session.pid,
                    "nonce": session.nonce,
                })
            );
            // Bounded hold: poll liveness to the deadline, then stop exactly
            // the verified PID (record + live exe + CURRENT window-nonce
            // title re-proof inside `stop_session`). A helper that exits on
            // its own reports honestly with best-effort stale-record removal.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
            let mut exited_early = false;
            while std::time::Instant::now() < deadline {
                if !tiler_macos::ffi::process_exists(session.pid as i32) {
                    exited_early = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            if exited_early {
                let removed = tiler_macos::probe_target::remove_ls_record_if_gone(
                    session.pid,
                    &session.nonce,
                );
                return Ok(serde_json::json!({
                    "bundle": session.bundle,
                    "pid": session.pid,
                    "nonce": session.nonce,
                    "ls": true,
                    "exited_early": true,
                    "stopped_exact_child": false,
                    "record_removed": removed,
                })
                .to_string());
            }
            match tiler_macos::probe_target::stop_session(session.pid, &session.nonce) {
                Ok(()) => Ok(serde_json::json!({
                    "bundle": session.bundle,
                    "pid": session.pid,
                    "nonce": session.nonce,
                    "ls": true,
                    "exited_early": false,
                    "stopped_exact_child": true,
                })
                .to_string()),
                Err(e) => {
                    // Raced self-exit between the last liveness poll and the
                    // stop: clean the stale record when the PID is provably
                    // gone, and report the early exit honestly.
                    if !tiler_macos::ffi::process_exists(session.pid as i32)
                        && tiler_macos::probe_target::remove_ls_record_if_gone(
                            session.pid,
                            &session.nonce,
                        )
                    {
                        return Ok(serde_json::json!({
                            "bundle": session.bundle,
                            "pid": session.pid,
                            "nonce": session.nonce,
                            "ls": true,
                            "exited_early": true,
                            "stopped_exact_child": false,
                            "record_removed": true,
                        })
                        .to_string());
                    }
                    Err((1, format!("LS probe stop: {e}")))
                }
            }
        }
        Command::ProbeStop { pid, nonce } => {
            tiler_macos::probe_target::stop_session(pid, &nonce).map_err(|e| (1, e.to_string()))?;
            Ok(serde_json::json!({
                "stop_pid": pid,
                "verified_exit": true,
            })
            .to_string())
        }
        Command::OwnedFrame { pid, nonce, wid } => {
            // Read-only owned window state (AX + CG frames, minimized,
            // fullscreen, role). No writes on any path.
            let owned = tiler_macos::ax::authorize_owned_action(pid, &nonce)
                .map_err(|e| (2, e.to_string()))?;
            let report = tiler_macos::ax::read_window_owned(&owned, pid, wid).map_err(|e| {
                (
                    1,
                    serde_json::json!({"stage": "owned-frame", "error": e.to_string()}).to_string(),
                )
            })?;
            serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
        }
        Command::OwnedMinimize {
            pid,
            nonce,
            wid,
            minimize,
        } => {
            // Explicit owned minimize/restore with preimage + readback.
            let owned = tiler_macos::ax::authorize_owned_action(pid, &nonce)
                .map_err(|e| (2, e.to_string()))?;
            let report =
                tiler_macos::ax::set_minimized_owned(&owned, pid, wid, minimize).map_err(|e| {
                    (
                        1,
                        serde_json::json!({"stage": "owned-minimize", "error": e.to_string()})
                            .to_string(),
                    )
                })?;
            serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
        }
        Command::OwnedFullscreen {
            pid,
            nonce,
            wid,
            fullscreen,
        } => {
            // Fixture-only owned native-fullscreen enter/exit with preimage
            // + readback. Helper preflight only, never a shipped feature.
            tiler_macos::owned_control::run_owned_fullscreen(pid, &nonce, wid, fullscreen)
        }
        Command::OwnedPost { pid, nonce, chord } => {
            // ONE semantic tile chord through the native session stream
            // (truthfully synthetic, oracle-gated per edge).
            tiler_macos::owned_control::run_owned_post(pid, &nonce, &chord, request.modifier)
        }
        Command::OwnedFocus { pid, nonce, wid } => {
            // SLPS focus dispatch to one exact owned window with agreement.
            tiler_macos::owned_control::run_owned_focus(pid, &nonce, wid, request.modifier)
        }
        Command::TextEditPost { record, nonce, key } => {
            // Diagnostic-only disposable TextEdit tap (never helper or
            // product input): run-owned record proof per edge with
            // unconditional release.
            tiler_macos::textedit_post::run_textedit_post(&record, &nonce, &key)
        }
        Command::OwnedDragPost {
            pid,
            nonce,
            edge,
            x,
            y,
            control,
        } => {
            // PARKED drag-fixture edge: refuses before any live touch until
            // a balanced release contract is proven (see owned_control).
            let x = x
                .parse::<f64>()
                .map_err(|_| (2, "owned-drag-post: --x must be a number".to_owned()))?;
            let y = y
                .parse::<f64>()
                .map_err(|_| (2, "owned-drag-post: --y must be a number".to_owned()))?;
            tiler_macos::owned_control::run_owned_drag_post(pid, &nonce, &edge, x, y, control)
        }
        Command::OwnedCursor { x, y } => {
            // Read-only cursor readback, or a bounded warp with
            // before/after readback (no click, no window target).
            let point = match (x, y) {
                (Some(x), Some(y)) => Some((
                    x.parse::<f64>()
                        .map_err(|_| (2, "owned-cursor: --x must be a number".to_owned()))?,
                    y.parse::<f64>()
                        .map_err(|_| (2, "owned-cursor: --y must be a number".to_owned()))?,
                )),
                _ => None,
            };
            tiler_macos::owned_control::run_owned_cursor(point)
        }
        Command::OwnedSpaceJoin { pid, nonce } => {
            // Public AppKit self-action: helper joins its OWN window to the
            // current Space with exact collectionBehavior restore. No WID,
            // no SLS, no synthetic events. Works after the mover exits via
            // the spawn-record + live CG join (same as spaces-restore).
            tiler_macos::space_join::run_owned_space_join(pid, &nonce)
        }
        Command::SpacesOwnedRecovery {
            original,
            temp,
            seconds,
            allow_untested,
            probe_direct,
        } => {
            // Bounded owned public recovery battery over two existing SIDs:
            // spawns one regular-policy helper, proves O->T->O with exact
            // membership plus exact frame restoration, stops exactly that
            // helper. Creates/destroys no Space, posts no synthetic events.
            // `--probe-direct` additionally runs ONE gated direct-SLS move
            // after O->T->O in a separate short-lived mover child with
            // delayed readbacks plus independent public restore.
            tiler_macos::space_recovery::run_spaces_owned_recovery(
                original,
                temp,
                seconds,
                allow_untested,
                probe_direct,
            )
        }
        Command::SpacesBindings => {
            // Read-only effective symbolic-hotkey table: chord + enabled
            // entries with NO symbolic IDs, plus Ctrl+Left/Right
            // corroboration. No settings reads or writes on any path.
            let report =
                tiler_macos::symbolic_hotkeys::read_bindings().map_err(|e| (1, e.to_string()))?;
            serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
        }
        Command::SpacesDesktopApply { count } => {
            // Explicit Authentic Desktop-N enable with owned preimages:
            // session-effective SkyLight SPI writes with durable
            // preimage-first ordering, per-ID readback, resume-safe
            // backups, and conditional Revert. Startup and tile paths
            // never apply. Fake-backed logic plus the checks above are
            // proven offline; chord/effect/reboot proof needs the
            // user-run physical check.
            tiler_macos::desktop_shortcuts::run_apply(count)
        }
        Command::SpacesDesktopRevert => tiler_macos::desktop_shortcuts::run_revert(),
        Command::SpacesDesktopStatus => tiler_macos::desktop_shortcuts::run_status(),
        Command::SpacesSwitchProbe {
            original,
            temp,
            seconds,
            allow_untested,
        } => {
            // Bounded same-display switch comparison over two EXISTING
            // SIDs: two owned regular-policy helpers bracket arrow legs
            // and the yabai began/ended gesture pair. Creates/destroys no
            // Space, performs no SLS move, posts no tap input.
            tiler_macos::space_switch_probe::run_spaces_switch_probe(
                original,
                temp,
                seconds,
                allow_untested,
            )
        }
        Command::SpacesDragTransfer {
            original,
            temp,
            seconds,
            allow_untested,
            numbered,
        } => {
            // Bounded owned titlebar-drag transfer: `Some` temp runs over
            // two EXISTING SIDs (independent self-join recovery proven
            // first, then one synthetic drag outward with an exact
            // drag-back inverse; creates/destroys no Space); `None`
            // (--temporary) creates exactly one temp on the original's
            // display first and removes only it afterwards. `--numbered`
            // posts the configured Desktop-N chords for the SLS ordinals
            // (default: relative arrows). Performs no
            // SLS move, writes no preferences, never targets Ghostty.
            tiler_macos::space_drag_live::run_spaces_drag_transfer(
                original,
                temp,
                seconds,
                allow_untested,
                numbered,
            )
        }
        Command::DragDiagChord {
            countdown,
            untagged,
            allow_untested,
        } => tiler_macos::space_drag_diag_live::run_drag_diag_chord(
            countdown,
            untagged,
            allow_untested,
        ),
        Command::DragDiagHold {
            countdown,
            hold_secs,
            allow_untested,
        } => tiler_macos::space_drag_diag_live::run_drag_diag_hold(
            countdown,
            hold_secs,
            allow_untested,
        ),
        Command::DragDiagFull {
            countdown,
            amethyst,
            desktop,
            untagged,
            allow_untested,
        } => tiler_macos::space_drag_diag_live::run_drag_diag_full(
            countdown,
            amethyst,
            desktop,
            untagged,
            allow_untested,
        ),
        Command::Tile {
            user_start,
            scopes,
            seconds,
            input,
            preset,
            preset_explicit,
            unscoped,
            excluded,
            log,
            run_dir,
            border,
            drag,
            indicator,
        } => {
            // Tile session: explicit start + product selection validated
            // inside; tap product route, Authentic/Compatible preset.
            // Modifier/preset/border resolve CLI-over-settings-file (an
            // explicitly passed CLI flag wins, else the tentative user
            // settings file, else the built-in default); file exclusions
            // apply to unscoped runs only. A malformed file fails closed
            // (never a silent fallback). Every terminal result also persists
            // to the run directory's `outcome.json` when `--run-dir` was
            // passed, so an exact-run launcher observes completion without
            // process attribution. LaunchServices launch via the `just tile` /
            // `just dev` recipes. Shutdown is bounded and windows stay in
            // place (never restored).
            let write_refused = |reason: &str| {
                let body = serde_json::json!({
                    "outcome": "refused",
                    "reason": reason,
                })
                .to_string();
                tiler_macos::tile::write_run_outcome(&run_dir, &body);
            };
            let (file, _) = tiler_macos::dev_selection::load_settings().map_err(|reason| {
                write_refused(&reason);
                (2, reason)
            })?;
            let resolved = tiler_macos::dev_selection::resolve_dev_config(
                unscoped,
                request.modifier,
                request.modifier_explicit,
                preset,
                preset_explicit,
                &excluded,
                &file,
            )
            .map_err(|reason| {
                write_refused(&reason);
                (2, reason)
            })?;
            let border = tiler_macos::dev_selection::resolve_border_config(
                &border,
                file.active_border.as_ref(),
            )
            .map_err(|reason| {
                write_refused(&reason);
                (2, reason)
            })?;
            let drag =
                tiler_macos::drag_drop::resolve_drag_config(&drag, file.drag_preview.as_ref())
                    .map_err(|reason| {
                        write_refused(&reason);
                        (2, reason)
                    })?;
            let indicator = tiler_macos::workspace_indicator::resolve_indicator_config(
                &indicator,
                file.workspace_indicator.as_ref(),
            )
            .map_err(|reason| {
                write_refused(&reason);
                (2, reason)
            })?;
            let request = tiler_macos::tile::TileRequest {
                user_start,
                scopes,
                seconds,
                input,
                preset: resolved.preset,
                modifier: resolved.modifier,
                unscoped,
                excluded: resolved.excluded,
                log,
                run_dir: run_dir.clone(),
                border,
                drag,
                indicator,
            };
            tiler_macos::tile::run_tile(request).map_err(|(code, body)| {
                // Validation refusals carry plain text (not JSON): persist
                // the refused envelope so the run directory still observes
                // a terminal state. Native terminal JSON bodies persist
                // inside `run_tile` already; rewriting the identical body
                // here is idempotent.
                if code == 2 {
                    write_refused(&body);
                }
                (code, body)
            })
        }
        Command::SpacesSession {
            seconds,
            allow_untested,
        } => tiler_macos::session::run_spaces_session(seconds, allow_untested),
        Command::BorderSession {
            seconds,
            regular_policy,
        } => tiler_macos::border_session::run_border_session(seconds, regular_policy),
        Command::GroupUnderlaySession { seconds } => {
            tiler_macos::group_underlay_session::run_group_underlay_session(seconds)
        }
        Command::McInspect => tiler_macos::mc_lifecycle::run_mc_inspect(),
        Command::WorkspaceIndicatorStatus => tiler_macos::workspace_indicator::run_status(),
        Command::WorkspaceIndicatorProof {
            seconds,
            display,
            allow_untested,
        } => tiler_macos::workspace_indicator_proof::run_workspace_indicator_proof(
            seconds,
            display,
            allow_untested,
        ),
        Command::McLifecycle {
            seconds,
            display,
            allow_untested,
        } => tiler_macos::mc_lifecycle::run_mc_lifecycle(seconds, display, allow_untested),
        Command::McRecover {
            temp,
            original,
            seconds,
            display,
            allow_untested,
        } => tiler_macos::mc_recover::run_mc_recover(
            temp,
            original,
            seconds,
            display,
            allow_untested,
        ),
        Command::McMembers { space } => tiler_macos::mc_members::run_mc_members(space),
        Command::McRestoreFrontmost {
            pid,
            wid,
            space,
            display,
            allow_untested,
        } => tiler_macos::frontmost_restore::run_mc_restore_frontmost(
            pid,
            wid,
            space,
            display,
            allow_untested,
        ),
        Command::ScopeInspect { scopes } => tiler_macos::scope_inspect::run_scope_inspect(scopes),
    }
}

fn preflight() -> Result<String, (i32, String)> {
    let arch = std::env::consts::ARCH;
    let arch_ok = arch == tiler_macos::SUPPORTED_ARCH;
    // Read-only legs for the live session check: the actual window-server
    // session verdict plus the workspace frontmost, so each live leg is
    // inspectable without touching UI, taps, AX, or Space moves.
    let session = tiler_macos::ffi::session_state();
    let frontmost = tiler_macos::ffi::frontmost_app()
        .map(|(pid, bundle)| serde_json::json!({"pid": pid, "bundle": bundle}));
    let report = serde_json::json!({
        "bundle": tiler_macos::BUNDLE_ID,
        "arch": arch,
        "arch_supported": arch_ok,
        "os": std::env::consts::OS,
        "min_macos_major": tiler_macos::MIN_MACOS_MAJOR,
        "sls_symbols": tiler_macos::ffi::sls_connection_available(),
        "secure_input": format!("{:?}", tiler_macos::input::secure_input()),
        "session": session,
        "frontmost": frontmost,
        "ax_trusted": tiler_macos::ffi::ax_is_process_trusted(),
        "post_event_access": tiler_macos::ffi::cg_post_event_access_known(),
        "mutates": false,
        "note": "nonmutating preflight: no UI, taps, AX control, or Space moves",
    });
    serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
}
