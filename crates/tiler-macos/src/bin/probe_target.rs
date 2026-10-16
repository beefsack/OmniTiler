#![forbid(unsafe_code)]

//! Owned disposable AppKit probe target (separate process).
//!
//! Prints its identity JSON (`bundle`, `pid`, `nonce`) on stdout, creates one
//! plain window on the main thread, prints a `ready` marker only after the
//! runloop services the window, optionally stalls (hung/slow modes), then
//! exits after `--seconds` or when the parent stops its exact handle. Never
//! touches foreign windows. Non-macOS builds exit 2 (no UI claimed).

use std::io::Write;

fn usage() -> &'static str {
    "usage: tiler-macos-probe-target --nonce HEX [--hung-ms MS] [--slow-ms MS] [--seconds N] [--fixed-size] [--defer-window-ms MS] [--close-window-ms MS] [--regular-policy] [--transient-panel-ms MS] [--transient-sheet-ms MS] [--space-join] [--origin X Y]"
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    match real_main(&argv) {
        Ok(()) => {}
        Err((code, message)) => {
            eprintln!("{message}");
            std::process::exit(code);
        }
    }
}

fn real_main(argv: &[String]) -> Result<(), (i32, String)> {
    let mut nonce: Option<String> = None;
    let mut hung_ms = 0u64;
    let mut slow_ms = 0u64;
    let mut seconds = tiler_macos::config::DEFAULT_PROBE_SECONDS;
    let mut fixed_size = false;
    let mut defer_window_ms = 0u64;
    let mut close_window_ms = 0u64;
    let mut regular_policy = false;
    let mut transient_panel_ms = 0u64;
    let mut transient_sheet_ms = 0u64;
    let mut space_join = false;
    let mut origin: Option<(f64, f64)> = None;
    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            "--nonce" => {
                i += 1;
                nonce = Some(argv.get(i).ok_or((2, usage().to_owned()))?.clone());
            }
            "--hung-ms" => {
                i += 1;
                hung_ms = argv
                    .get(i)
                    .ok_or((2, usage().to_owned()))?
                    .parse::<u64>()
                    .map_err(|_| (2, usage().to_owned()))?;
            }
            "--slow-ms" => {
                i += 1;
                slow_ms = argv
                    .get(i)
                    .ok_or((2, usage().to_owned()))?
                    .parse::<u64>()
                    .map_err(|_| (2, usage().to_owned()))?;
            }
            "--seconds" => {
                i += 1;
                seconds = argv
                    .get(i)
                    .ok_or((2, usage().to_owned()))?
                    .parse::<u64>()
                    .map_err(|_| (2, usage().to_owned()))?;
            }
            "--fixed-size" => {
                fixed_size = true;
            }
            "--defer-window-ms" => {
                i += 1;
                defer_window_ms = argv
                    .get(i)
                    .ok_or((2, usage().to_owned()))?
                    .parse::<u64>()
                    .map_err(|_| (2, usage().to_owned()))?;
            }
            "--close-window-ms" => {
                i += 1;
                close_window_ms = argv
                    .get(i)
                    .ok_or((2, usage().to_owned()))?
                    .parse::<u64>()
                    .map_err(|_| (2, usage().to_owned()))?;
            }
            "--regular-policy" => {
                regular_policy = true;
            }
            "--transient-panel-ms" => {
                i += 1;
                transient_panel_ms = argv
                    .get(i)
                    .ok_or((2, usage().to_owned()))?
                    .parse::<u64>()
                    .map_err(|_| (2, usage().to_owned()))?;
            }
            "--transient-sheet-ms" => {
                i += 1;
                transient_sheet_ms = argv
                    .get(i)
                    .ok_or((2, usage().to_owned()))?
                    .parse::<u64>()
                    .map_err(|_| (2, usage().to_owned()))?;
            }
            "--space-join" => {
                space_join = true;
            }
            "--origin" => {
                i += 1;
                let x = argv
                    .get(i)
                    .ok_or((2, usage().to_owned()))?
                    .parse::<f64>()
                    .map_err(|_| (2, usage().to_owned()))?;
                i += 1;
                let y = argv
                    .get(i)
                    .ok_or((2, usage().to_owned()))?
                    .parse::<f64>()
                    .map_err(|_| (2, usage().to_owned()))?;
                tiler_macos::probe_target::validate_origin(x, y).map_err(|e| (2, e))?;
                origin = Some((x, y));
            }
            other => return Err((2, format!("unknown arg '{other}\n{usage}", usage = usage()))),
        }
        i += 1;
    }
    let nonce = nonce.ok_or((2, usage().to_owned()))?;
    let spec = tiler_macos::probe_target::ProbeTargetSpec::new(&nonce, hung_ms, slow_ms, seconds)
        .map(|s| {
            let mut s = s
                .with_fixed_size(fixed_size)
                .with_defer_window_ms(defer_window_ms)
                .with_close_window_ms(close_window_ms)
                .with_regular_policy(regular_policy)
                .with_transient_panel_ms(transient_panel_ms)
                .with_transient_sheet_ms(transient_sheet_ms)
                .with_space_join(space_join);
            if let Some((x, y)) = origin {
                s = s.with_origin(x, y);
            }
            s
        })
        .map_err(|e| (2, e))?;
    run_target(&spec).map_err(|e| (1, e))
}

fn run_target(spec: &tiler_macos::probe_target::ProbeTargetSpec) -> Result<(), String> {
    // Bundled helpers (running inside the signed helper `.app`) report the
    // stable helper bundle and run with regular activation; direct-spawn
    // helpers report the probe bundle and keep the flag-driven policy. The
    // own-bundle read is main-thread AppKit here (this process's main).
    let own_bundle =
        tiler_macos::ffi::own_bundle_id().unwrap_or_else(|| tiler_macos::BUNDLE_ID.to_owned());
    let bundled = own_bundle == tiler_macos::HELPER_BUNDLE_ID;
    let mut spec = spec.clone();
    if bundled {
        spec = spec.with_regular_policy(true);
    }
    println!(
        "{}",
        serde_json::json!({
            "bundle": own_bundle,
            "pid": std::process::id(),
            "nonce": spec.nonce,
        })
    );
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    // AppKit boundary lives in the single unsafe module; this file stays safe.
    // `ready` fires only after the window is serviced by the runloop, so a
    // hung stall afterwards is a live unresponsive window, not a missing one.
    // The window title carries the per-run nonce so cross-command checks can
    // revalidate the CURRENT live helper (owner PID + exact nonce title),
    // not just a historical stdout handshake.
    tiler_macos::ffi::run_owned_probe_window(
        spec.seconds,
        spec.hung_ms,
        &tiler_macos::probe_target::probe_window_title(&spec.nonce),
        spec.fixed_size,
        spec.defer_window_ms,
        spec.close_window_ms,
        spec.regular_policy,
        spec.transient_panel_ms,
        spec.transient_sheet_ms,
        spec.space_join,
        spec.origin,
        &|| {
            println!("{}", serde_json::json!({"ready": true}));
            let _ = std::io::stdout().flush();
        },
    )
}
