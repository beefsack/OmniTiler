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
    "usage: tiler-macos-probe-target --nonce HEX [--hung-ms MS] [--slow-ms MS] [--seconds N]"
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
            other => return Err((2, format!("unknown arg '{other}\n{usage}", usage = usage()))),
        }
        i += 1;
    }
    let nonce = nonce.ok_or((2, usage().to_owned()))?;
    let spec = tiler_macos::probe_target::ProbeTargetSpec::new(&nonce, hung_ms, slow_ms, seconds)
        .map_err(|e| (2, e))?;
    run_target(&spec).map_err(|e| (1, e))
}

fn run_target(spec: &tiler_macos::probe_target::ProbeTargetSpec) -> Result<(), String> {
    println!(
        "{}",
        serde_json::json!({
            "bundle": tiler_macos::BUNDLE_ID,
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
        &|| {
            println!("{}", serde_json::json!({"ready": true}));
            let _ = std::io::stdout().flush();
        },
    )
}
