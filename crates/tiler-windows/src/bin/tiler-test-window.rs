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
                "tiler-test-window commands:\n  run --receipt PATH [--seconds N] [--passive]  create owned test window (passive stays hidden until show)\n  close HWND [--tag TAG]  post WM_CLOSE to owned window (proof always passes the captured tag; bare form stays for the WinArrow harness)\n  inspect HWND  print owned window snapshot JSON\n  show HWND --tag TAG  admit a passive helper without activation or z-order change (no hide)\n  minimize HWND --tag TAG  minimize without activating\n  restore HWND --tag TAG  restore a minimized helper without activating\n  move HWND --tag TAG --to X,Y,W,H  exact-bound location move without activation or z-order change\n  cloak-preflight HWND --tag TAG  read-only experimental gate + baseline (run before the first cloak; success instructs but never asserts the recovery proof)\n  cloak HWND --tag TAG --seconds N [--delete-tab] --stop-file PATH --log PATH  finite experimental cloak controller with auto-restore; stop file must not preexist, log is created fresh (needs Windows x64 build 26300)\n  uncloak HWND --tag TAG [--added-tab]  standalone exact-identity recovery, foreground-safe; the private uncloak call always runs, even on an uncloaked baseline (that baseline call is the recovery proof)\n  cloak-observe HWND --tag TAG --seconds N --stop-file PATH --log PATH  observation-only PID-scoped event logging, no cloak writes"
            );
            return Ok(());
        }
        "run" => {
            let opts = tiler_windows::test_window::parse_helper_run_args(rest)
                .map_err(|message| (2, message))?;
            tiler_windows::test_window::sys::run_owned_window(
                &opts.receipt,
                opts.seconds,
                opts.passive,
            )
            .map_err(|e| (1, e.to_string()))?
        }
        "close" => {
            let (hwnd, tag) = tiler_windows::test_window::parse_close_args(rest)
                .map_err(|message| (2, message))?;
            tiler_windows::test_window::sys::close_owned(hwnd, tag.as_deref())
                .map_err(|e| (1, e.to_string()))?
        }
        "inspect" => {
            if rest.len() != 1 {
                return Err((2, "usage: tiler-test-window inspect HWND".to_owned()));
            }
            let hwnd = tiler_windows::test_window::parse_hwnd(&rest[0])
                .ok_or((2, "usage: tiler-test-window inspect HWND".to_owned()))?;
            tiler_windows::test_window::sys::inspect_owned(hwnd).map_err(|e| (1, e.to_string()))?
        }
        "show" | "minimize" | "restore" => {
            let tagged = tiler_windows::test_window::parse_tagged_hwnd_args(command, rest)
                .map_err(|message| (2, message))?;
            match command {
                "show" => tiler_windows::test_window::sys::show_owned(tagged.hwnd, &tagged.tag),
                "minimize" => {
                    tiler_windows::test_window::sys::minimize_owned(tagged.hwnd, &tagged.tag)
                }
                _ => tiler_windows::test_window::sys::restore_owned(tagged.hwnd, &tagged.tag),
            }
            .map_err(|e| (1, e.to_string()))?
        }
        "move" => {
            let target = tiler_windows::test_window::parse_move_args(rest)
                .map_err(|message| (2, message))?;
            tiler_windows::test_window::sys::move_owned(&target).map_err(|e| (1, e.to_string()))?
        }
        "cloak-preflight" => {
            let tagged =
                tiler_windows::test_window::parse_tagged_hwnd_args("cloak-preflight", rest)
                    .map_err(|message| (2, message))?;
            tiler_windows::test_window::sys::cloak_preflight(tagged.hwnd, &tagged.tag)
                .map_err(|e| (1, e.to_string()))?
        }
        "cloak" => {
            let parsed =
                tiler_windows::cloak::parse_cloak_args(rest).map_err(|message| (2, message))?;
            tiler_windows::test_window::sys::cloak_owned(&parsed).map_err(|e| (1, e.to_string()))?
        }
        "uncloak" => {
            let parsed =
                tiler_windows::cloak::parse_uncloak_args(rest).map_err(|message| (2, message))?;
            tiler_windows::test_window::sys::uncloak_owned(&parsed)
                .map_err(|e| (1, e.to_string()))?
        }
        "cloak-observe" => {
            let parsed =
                tiler_windows::cloak::parse_observe_args(rest).map_err(|message| (2, message))?;
            tiler_windows::test_window::sys::cloak_observe(&parsed)
                .map_err(|e| (1, e.to_string()))?
        }
        _ => {
            return Err((
                2,
                "usage: tiler-test-window run --receipt PATH [--seconds N] [--passive]|close HWND [--tag TAG]|inspect HWND|show HWND --tag TAG|minimize HWND --tag TAG|restore HWND --tag TAG|move HWND --tag TAG --to X,Y,W,H|cloak-preflight HWND --tag TAG|cloak HWND --tag TAG --seconds N [--delete-tab] --stop-file PATH --log PATH|uncloak HWND --tag TAG [--added-tab]|cloak-observe HWND --tag TAG --seconds N --stop-file PATH --log PATH"
                    .to_owned(),
            ));
        }
    };
    println!("{output}");
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("tiler-test-window is Windows-only");
    std::process::exit(2);
}
