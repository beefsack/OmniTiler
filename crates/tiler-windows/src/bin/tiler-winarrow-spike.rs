#![forbid(unsafe_code)]
#![cfg_attr(windows, windows_subsystem = "windows")]

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
                "tiler-winarrow-spike commands:\n  identity  print current process identity as JSON\n  run --helper-hwnd HWND --report PATH  bounded hook-only spike owner run\n  ready  report owner readiness as JSON\n  restore  standalone ledger cleanup after owner exit (call explicitly after stop)\n  stop  request graceful exit of verified owner only; leaves ledger for standalone restore\n  emergency-stop  terminate verified owner only; leaves ledger for standalone restore"
            );
            return Ok(());
        }
        "run" => tiler_windows::winarrow::sys::parse_and_run(rest)?,
        cmd @ ("identity" | "ready" | "restore" | "stop" | "emergency-stop") => {
            if !rest.is_empty() {
                return Err((2, format!("usage: tiler-winarrow-spike {cmd}")));
            }
            match cmd {
                "identity" => identity_report().map_err(|e| (1, e.to_string()))?,
                "ready" => {
                    tiler_windows::lifecycle::sys::cmd_ready().map_err(|e| (1, e.to_string()))?
                }
                "restore" => {
                    tiler_windows::lifecycle::sys::cmd_restore().map_err(|e| (1, e.to_string()))?
                }
                "stop" => tiler_windows::lifecycle::sys::cmd_stop(false)
                    .map_err(|e| (1, e.to_string()))?,
                _ => {
                    tiler_windows::lifecycle::sys::cmd_stop(true).map_err(|e| (1, e.to_string()))?
                }
            }
        }
        _ => {
            return Err((
                2,
                "usage: tiler-winarrow-spike identity|run --helper-hwnd HWND --report PATH|ready|restore|stop|emergency-stop"
                    .to_owned(),
            ));
        }
    };
    println!("{output}");
    Ok(())
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
    eprintln!("tiler-winarrow-spike is Windows-only");
    std::process::exit(2);
}
