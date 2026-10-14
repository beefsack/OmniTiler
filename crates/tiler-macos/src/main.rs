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
        Command::FocusSession { seconds } => {
            // Bounded Child-holder SLPS focus session: original preimage
            // with workspace+AX+CG agreement, SLPS focus with agreement,
            // SAME-route restore with agreement. No activation API, taps,
            // keys, SA, or Space writes.
            tiler_macos::focus::run_focus_session(seconds)
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
        Command::InputRegister { seconds } => {
            let idle = std::time::Duration::from_secs(seconds.saturating_add(10));
            let (engine, handle) = tiler_macos::engine::Engine::spawn_thread(idle);
            let report = tiler_macos::input::register_hotkey_bounded(
                request.modifier,
                tiler_macos::input::PROBE_KEY_CODE,
                seconds,
                &engine.tx,
            )
            .map_err(|e| (1, e.to_string()))?;
            let _ = engine.try_send(tiler_macos::engine::EngineMessage::Shutdown);
            let state = handle
                .join()
                .map_err(|_| (1, "engine thread failed".to_owned()))?;
            Ok(serde_json::json!({
                "modifier": request.modifier.as_str(),
                "seconds": seconds,
                "os_accepted": report.os_accepted,
                "os_status": report.os_status,
                "deliveries": report.deliveries,
                "engine_input_events": state.input_events,
                "note": "deliveries need a live keypress during the bounded wait; zero without one is expected",
            })
            .to_string())
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
            // UNTESTED live: the report separates void dispatch from the
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
        } => {
            let nonce = tiler_macos::probe_target::mint_nonce().map_err(|e| (1, e))?;
            let spec =
                tiler_macos::probe_target::ProbeTargetSpec::new(&nonce, hung_ms, slow_ms, seconds)
                    .map(|s| s.with_fixed_size(fixed_size))
                    .map_err(|e| (2, e))?;
            let mut session = tiler_macos::probe_target::spawn_verified(&spec)
                .map_err(|e| (1, format!("spawn failed: {e}")))?;
            // Expose the ready identity while the owned child is alive so
            // bounded companion commands can authenticate the same session.
            println!(
                "{}",
                serde_json::json!({
                    "ready": true,
                    "bundle": tiler_macos::BUNDLE_ID,
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
                "bundle": tiler_macos::BUNDLE_ID,
                "pid": session.pid,
                "nonce": session.nonce,
                "exited_early": exited_early,
                "stopped_exact_child": !exited_early,
            })
            .to_string())
        }
        Command::ProbeStop { pid, nonce } => {
            tiler_macos::probe_target::stop_session(pid, &nonce).map_err(|e| (1, e.to_string()))?;
            Ok(serde_json::json!({
                "stop_pid": pid,
                "verified_exit": true,
            })
            .to_string())
        }
        Command::SpacesSession {
            seconds,
            allow_untested,
        } => tiler_macos::session::run_spaces_session(seconds, allow_untested),
    }
}

fn preflight() -> Result<String, (i32, String)> {
    let arch = std::env::consts::ARCH;
    let arch_ok = arch == tiler_macos::SUPPORTED_ARCH;
    let report = serde_json::json!({
        "bundle": tiler_macos::BUNDLE_ID,
        "arch": arch,
        "arch_supported": arch_ok,
        "os": std::env::consts::OS,
        "min_macos_major": tiler_macos::MIN_MACOS_MAJOR,
        "sls_symbols": tiler_macos::ffi::sls_connection_available(),
        "secure_input": format!("{:?}", tiler_macos::input::secure_input()),
        "ax_trusted": tiler_macos::ffi::ax_is_process_trusted(),
        "post_event_access": tiler_macos::ffi::cg_post_event_access_known(),
        "mutates": false,
        "note": "nonmutating preflight: no UI, taps, AX control, or Space moves",
    });
    serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
}
