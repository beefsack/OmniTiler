#![cfg_attr(windows, windows_subsystem = "windows")]
#![forbid(unsafe_code)]

#[cfg(windows)]
fn main() {
    match real_main() {
        Ok(()) => {}
        Err((code, message)) => {
            eprintln!("{message}");
            std::process::exit(code);
        }
    }
}

#[cfg(windows)]
fn real_main() -> Result<(), (i32, String)> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (command, rest) = match args.split_first() {
        Some((first, rest)) => (first.as_str(), rest),
        None => ("help", &[][..]),
    };
    let output = match command {
        "--help" | "-h" | "help" => {
            println!(
                "tiler-windows commands:\n  identity  print current process identity as JSON\n  run --seconds N [--trace] [--hide HWND]  bounded owner run\n  ready  report owner readiness as JSON\n  restore  standalone reveal of owned hidden windows and ledger cleanup after owner exit (call explicitly after stop)\n  stop  request graceful exit of verified owner only; leaves ledger/windows for standalone restore\n  emergency-stop  terminate verified owner only; leaves ledger/windows for standalone restore\n  tile --user-start [--seconds N] [--trace] [--no-keyboard-snap-takeover] [--allow-win-l] [--no-mouse-snap-prevention] [--inner-gap N] [--outer-gap N] [--scope-exe NAME ...] [--scope-host-child HOST=CHILD ...]  normal user tiling loop until stop (no hide, geometry left in place; keyboard takeover on by default, unshifted Win+L needs --allow-win-l; session-only mouse-Snap prevention on by default; gaps default to saved %LOCALAPPDATA%\\omnitiler\\settings.json values (8/8) with explicit switches overriding; repeatable --scope-exe restricts management to named exes, empty default manages everything; repeatable --scope-host-child HOST=CHILD admits a listed host only with a live matching hosted child; first run with no settings file prompts authentic/compatible once and persists the choice; owner tray icon with Settings and Stop)\n  tile-proof --allowlist PATH [--seconds N] [--trace]  owned-helpers-only proof loop (refuses without a valid allowlist, never falls back to normal)\n  shortcut-proof --allowlist PATH [--seconds N] [--trace] [--no-mouse-snap-prevention]  owned-helpers-only automated shortcut proof with test-only marked synthetic-input acceptance (never falls back to normal; Win+L stays gated off)
  workspace-proof --allowlist PATH [--seconds N] [--trace] [--no-mouse-snap-prevention]  owned-helpers-only automated workspace proof (select/send/follow with hide/reveal for exactly the allowlist; never falls back to normal)
  Active-border options for tile and proof loops: --no-active-border, --active-border-width N (0..32, default 3), --active-border-gap N (0..64, default 0), --active-border-radius N (0..64, default 6), --active-border-color #rrggbb (fallback #2a82da), --active-border-theme|--no-active-border-theme (system accent, default on: configured color wins unless --active-border-theme finds an accent). Border is on by default; dimensions are logical pixels.
  Group-underlay options for tile and proof loops: --no-group-underlay, --group-underlay-color #aarrggbb (fill #662a82da), --group-underlay-extension N (-1..=32, default -1 follows the border width). Underlay is on by default and shows for Win+Shift hold (either order, extras allowed, stationary allowed) or a focused titlebar move; resize alone never shows it.
  workspace (--select|--send|--stay) INDEX | --previous | --fullscreen | --float | --sticky | (--relative|--send-relative|--stay-relative) (previous|next)  exact-owner out-of-hook control for the normal tile loop only (queues one bounded request 0..9, select focuses / send moves the focused managed window and follows / stay moves without following / previous toggles the previous view / fullscreen toggles fullscreen on the focused managed window / float toggles float and sticky toggles sticky on the focused managed window, both test-needed routes tentative pending user review / relative forms step the scoped ring with select/follow/stay; proof owners refuse)
  resize --direction (left|right|up|down) --mode (outwards|inwards)  test-needed exact-owner keyboard-resize control for the normal tile loop only, tentative pending user review (queues one bounded request; the owner resolves the live foreground as the subject and dispatches through the real keyboard_tick resize arm with all production fences and the repeat tracker; proof owners refuse; synthetic input can never drive a resize)
  directional (--move|--focus) --direction (left|right|up|down)  test-needed exact-owner directional move/focus control for the normal tile loop only, tentative pending user review (queues one bounded request; the owner resolves the live foreground as the subject and dispatches through the real keyboard_tick directional arm with production fences, keyboard takeover not required; proof owners refuse; injected keyboard filtering unchanged)
  send-output (--follow|--stay) --direction (left|right|up|down)  test-needed exact-owner explicit output-send control for the normal tile loop only, tentative pending user review (queues one bounded request; the owner resolves the live foreground as the subject and dispatches through the real SendToOutput production route with production fences, keyboard takeover not required; proof owners refuse; injected keyboard filtering unchanged)
  settings  open the native settings window (gaps, border, underlay, takeover, shortcuts with authentic/compatible presets; Apply validates and saves, Revert discards edits, Close never applies)\n  capture --out PATH --hwnd HWND [--hwnd HWND ...]  read-only frozen-allowlist capture of explicitly listed owned helpers\n  inventory  read-only top-level window list for selecting capture targets (no titles)\n  children --hwnd HWND [--hwnd HWND ...]  read-only child-window report with verified process identity (no titles)\n  inspect --allowlist PATH  read-only fresh-state report for exactly the frozen allowlist (no titles)\n  border-inspect  read-only report of the running owner's process-owned border overlay (geometry/visibility only; no titles, no content, no screen capture)\n             underlay-inspect  read-only report of the running owner's process-owned group-underlay fill
           (geometry/visibility only; no titles, no content, no screen capture)\n  preview-inspect  read-only report of the running owner's
           process-owned drop-preview fill (geometry/visibility only; no titles, no content, no screen capture)"
            );
            return Ok(());
        }
        "identity" => {
            if !rest.is_empty() {
                return Err((2, "usage: tiler-windows identity".to_owned()));
            }
            identity_report().map_err(|e| (1, e.to_string()))?
        }
        "run" => {
            let options =
                tiler_windows::lifecycle::parse_run_args(rest).map_err(|message| (2, message))?;
            tiler_windows::lifecycle::sys::cmd_run(
                options.seconds,
                options.trace,
                options.hide_hwnd,
            )
            .map_err(|e| (1, e.to_string()))?
        }
        "ready" => {
            if !rest.is_empty() {
                return Err((2, "usage: tiler-windows ready".to_owned()));
            }
            tiler_windows::lifecycle::sys::cmd_ready().map_err(|e| (1, e.to_string()))?
        }
        "restore" => {
            if !rest.is_empty() {
                return Err((2, "usage: tiler-windows restore".to_owned()));
            }
            tiler_windows::lifecycle::sys::cmd_restore().map_err(|e| (1, e.to_string()))?
        }
        "stop" => {
            if !rest.is_empty() {
                return Err((2, "usage: tiler-windows stop".to_owned()));
            }
            tiler_windows::lifecycle::sys::cmd_stop(false).map_err(|e| (1, e.to_string()))?
        }
        "tile" => {
            let (base, live, settings_dir) = tiler_windows::tiling_sys::load_normal_tile_base();
            let (options, cli_overrides) =
                tiler_windows::tiling::parse_tile_args_from_with_overrides(rest, &base)
                    .map_err(|message| (2, message))?;
            tiler_windows::tiling_sys::cmd_tile(&options, live, settings_dir, cli_overrides)
                .map_err(|e| (1, e.to_string()))?
        }
        "tile-proof" => {
            let options = tiler_windows::tiling::parse_tile_proof_args(rest)
                .map_err(|message| (2, message))?;
            tiler_windows::tiling_sys::cmd_tile_proof(&options, rest)
                .map_err(|e| (1, e.to_string()))?
        }
        "shortcut-proof" => {
            let options = tiler_windows::tiling::parse_shortcut_proof_args(rest)
                .map_err(|message| (2, message))?;
            tiler_windows::tiling_sys::cmd_shortcut_proof(&options, rest)
                .map_err(|e| (1, e.to_string()))?
        }
        "workspace-proof" => {
            let options = tiler_windows::tiling::parse_workspace_proof_args(rest)
                .map_err(|message| (2, message))?;
            tiler_windows::tiling_sys::cmd_workspace_proof(&options, rest)
                .map_err(|e| (1, e.to_string()))?
        }
        "workspace" => {
            let options = tiler_windows::tiling::parse_workspace_args(rest)
                .map_err(|message| (2, message))?;
            tiler_windows::tiling::verify_workspace_argv_consistency(rest, &options)
                .map_err(|message| (2, message))?;
            tiler_windows::tiling_sys::cmd_workspace(&options).map_err(|e| (1, e.to_string()))?
        }
        "resize" => {
            let options =
                tiler_windows::tiling::parse_resize_args(rest).map_err(|message| (2, message))?;
            tiler_windows::tiling::verify_resize_argv_consistency(rest, &options)
                .map_err(|message| (2, message))?;
            tiler_windows::tiling_sys::cmd_resize(&options).map_err(|e| (1, e.to_string()))?
        }
        "directional" => {
            let options = tiler_windows::tiling::parse_directional_args(rest)
                .map_err(|message| (2, message))?;
            tiler_windows::tiling::verify_directional_argv_consistency(rest, &options)
                .map_err(|message| (2, message))?;
            tiler_windows::tiling_sys::cmd_directional(&options).map_err(|e| (1, e.to_string()))?
        }
        "send-output" => {
            let options = tiler_windows::tiling::parse_send_output_args(rest)
                .map_err(|message| (2, message))?;
            tiler_windows::tiling::verify_send_output_argv_consistency(rest, &options)
                .map_err(|message| (2, message))?;
            tiler_windows::tiling_sys::cmd_send_output(&options).map_err(|e| (1, e.to_string()))?
        }
        "hide-proof" => {
            let options = tiler_windows::tiling::parse_hide_proof_args(rest)
                .map_err(|message| (2, message))?;
            tiler_windows::tiling_sys::cmd_hide_proof(&options, rest)
                .map_err(|e| (1, e.to_string()))?
        }
        "capture" => {
            let options =
                tiler_windows::tiling::parse_capture_args(rest).map_err(|message| (2, message))?;
            tiler_windows::tiling_sys::cmd_capture(&options).map_err(|e| (1, e.to_string()))?
        }
        "inventory" => {
            if !rest.is_empty() {
                return Err((2, "usage: tiler-windows inventory".to_owned()));
            }
            tiler_windows::tiling_sys::cmd_inventory().map_err(|e| (1, e.to_string()))?
        }
        "children" => {
            let options =
                tiler_windows::tiling::parse_children_args(rest).map_err(|message| (2, message))?;
            tiler_windows::tiling_sys::cmd_children(&options).map_err(|e| (1, e.to_string()))?
        }
        "inspect" => {
            let options =
                tiler_windows::tiling::parse_inspect_args(rest).map_err(|message| (2, message))?;
            tiler_windows::tiling_sys::cmd_inspect(&options).map_err(|e| (1, e.to_string()))?
        }
        "border-inspect" => {
            if !rest.is_empty() {
                return Err((2, "usage: tiler-windows border-inspect".to_owned()));
            }
            tiler_windows::tiling_sys::cmd_border_inspect().map_err(|e| (1, e.to_string()))?
        }
        "underlay-inspect" => {
            if !rest.is_empty() {
                return Err((2, "usage: tiler-windows underlay-inspect".to_owned()));
            }
            tiler_windows::tiling_sys::cmd_underlay_inspect().map_err(|e| (1, e.to_string()))?
        }
        "preview-inspect" => {
            if !rest.is_empty() {
                return Err((2, "usage: tiler-windows preview-inspect".to_owned()));
            }
            tiler_windows::tiling_sys::cmd_preview_inspect().map_err(|e| (1, e.to_string()))?
        }
        "settings" => {
            if !rest.is_empty() {
                return Err((2, "usage: tiler-windows settings".to_owned()));
            }
            tiler_windows::settings_ui::cmd_settings().map_err(|e| (1, e.to_string()))?
        }
        "emergency-stop" => {
            if !rest.is_empty() {
                return Err((2, "usage: tiler-windows emergency-stop".to_owned()));
            }
            tiler_windows::lifecycle::sys::cmd_stop(true).map_err(|e| (1, e.to_string()))?
        }
        "watch-owner" => {
            let (pid, creation) = parse_watch_owner_args(rest)?;
            tiler_windows::product_hide::sys::cmd_watch_owner(pid, &creation)
                .map_err(|e| (1, e.to_string()))?
        }
        _ => {
            return Err((
                2,
                "usage: tiler-windows identity|run --seconds N [--trace] [--hide HWND]|ready|restore|stop|emergency-stop|watch-owner --pid PID --creation HEX|tile --user-start [--seconds N] [--trace] [--no-keyboard-snap-takeover] [--allow-win-l] [--no-mouse-snap-prevention] [--inner-gap N] [--outer-gap N] [--scope-exe NAME ...] [--scope-host-child HOST=CHILD ...]|tile-proof --allowlist PATH [--seconds N] [--trace]|shortcut-proof --allowlist PATH [--seconds N] [--trace] [--no-mouse-snap-prevention]|workspace-proof --allowlist PATH [--seconds N] [--trace] [--no-mouse-snap-prevention]|workspace (--select|--send) INDEX|resize --direction DIR --mode MODE|directional (--move|--focus) --direction DIR|send-output (--follow|--stay) --direction DIR|hide-proof --allowlist PATH [--seconds N] [--trace]|capture --out PATH --hwnd HWND|inventory|children --hwnd HWND|inspect --allowlist            PATH|border-inspect|underlay-inspect|preview-inspect|settings"
                    .to_owned(),
            ));
        }
    };
    println!("{output}");
    Ok(())
}

#[cfg(windows)]
fn parse_watch_owner_args(rest: &[String]) -> Result<(u32, String), (i32, String)> {
    let usage = "usage: tiler-windows watch-owner --pid PID --creation HEX".to_owned();
    if rest.len() != 4 || rest[0] != "--pid" || rest[2] != "--creation" || rest[3].is_empty() {
        return Err((2, usage));
    }
    let pid: u32 = rest[1].parse().map_err(|_| (2, usage.clone()))?;
    if pid == 0 {
        return Err((2, usage));
    }
    Ok((pid, rest[3].clone()))
}

#[cfg(windows)]
fn identity_report() -> Result<String, Box<dyn std::error::Error>> {
    let process = tiler_windows::native::current_identity()?;
    let integrity_level = tiler_windows::native::current_integrity_level()?;
    let ledger_directory = tiler_windows::native::ledger_directory()?;
    Ok(serde_json::json!({
        "process": process,
        "integrity_level": integrity_level,
        "ledger_directory": ledger_directory,
    })
    .to_string())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("tiler-windows identity is Windows-only");
    std::process::exit(2);
}
