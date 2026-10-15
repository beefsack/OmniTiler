#![forbid(unsafe_code)]

//! CLI contract. `help` and `check`/`preflight` (the default with no args)
//! never launch UI, event taps, AX control, or Space moves. Every other mode
//! is explicit and bounded.

use crate::config::{
    DEFAULT_AX_TIMEOUT_MS, DEFAULT_PROBE_SECONDS, DEFAULT_TAP_SECONDS, ProbeModifier,
};
use crate::tile::{DEFAULT_TILE_SECONDS, TileInput};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help,
    Preflight,
    AxEnumerate {
        max_windows: usize,
    },
    AxObserve {
        pid: u32,
        nonce: String,
        timeout_ms: u64,
    },
    AxSession {
        seconds: u64,
    },
    FocusSession {
        seconds: u64,
        appkit: bool,
    },
    FocusDiagnose {
        fallback: bool,
        appkit: bool,
    },
    AxSetFrame {
        pid: u32,
        nonce: String,
        wid: u32,
    },
    AxRaise {
        pid: u32,
        nonce: String,
        wid: u32,
    },
    InputRegister {
        seconds: u64,
    },
    InputTap {
        seconds: u64,
        suppress: bool,
    },
    InputSession {
        seconds: u64,
        route: crate::input_session::InputSessionRoute,
        post_route: crate::input_session::PostRoute,
        stall_once_ms: u64,
        direct_disable_hold_ms: u64,
        direct_rearm: bool,
        watchdog_only: bool,
    },
    SecureInput,
    SpacesDiagnose {
        wid: Option<u32>,
        pid: Option<u32>,
        nonce: Option<String>,
    },
    CgOwnedWindows {
        pid: u32,
        nonce: String,
    },
    SpacesMove {
        pid: u32,
        nonce: String,
        wid: u32,
        space: u64,
        allow_untested: bool,
    },
    SpacesRestore {
        pid: u32,
        nonce: String,
        wid: u32,
        space: u64,
        allow_untested: bool,
    },
    SpacesSwitch {
        pid: u32,
        nonce: String,
        space: u64,
    },
    SpacesCreate {
        pid: u32,
        nonce: String,
    },
    SpacesDestroy {
        pid: u32,
        nonce: String,
        space: u64,
    },
    ProbeSpawn {
        hung_ms: u64,
        slow_ms: u64,
        seconds: u64,
        fixed_size: bool,
        regular_policy: bool,
        close_window_ms: u64,
        transient_panel_ms: u64,
        transient_sheet_ms: u64,
    },
    OwnedFrame {
        pid: u32,
        nonce: String,
        wid: u32,
    },
    OwnedMinimize {
        pid: u32,
        nonce: String,
        wid: u32,
        minimize: bool,
    },
    OwnedFullscreen {
        pid: u32,
        nonce: String,
        wid: u32,
        fullscreen: bool,
    },
    OwnedPost {
        pid: u32,
        nonce: String,
        chord: String,
    },
    OwnedFocus {
        pid: u32,
        nonce: String,
        wid: u32,
    },
    SpacesSession {
        seconds: u64,
        allow_untested: bool,
    },
    ProbeStop {
        pid: u32,
        nonce: String,
    },
    Tile {
        user_start: bool,
        scopes: Vec<String>,
        seconds: u64,
        input: TileInput,
    },
    ScopeInspect {
        scopes: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliRequest {
    pub modifier: ProbeModifier,
    pub command: Command,
    /// True only for modes that may touch UI/taps/AX/Space writes.
    pub may_mutate: bool,
}

pub fn help_text() -> String {
    format!(
        "tiler-macos (Phase 0 capability probe, {bundle})\n\
         \n\
         Default (no args) is `preflight`: nonmutating checks only, no UI/hooks.\n\
         `help` and `check|preflight` never launch UI, taps, AX control, or Space moves.\n\
         \n\
          usage: tiler-macos [--ctrl meta|ctrl|alt|shift] <command> [options]\n\
          \x20 (the Ctrl-role modifier defaults to ctrl; pass --ctrl meta for Cmd)\n\
         \n\
          nonmutating:\n\
           \x20 help                          this text\n\
           \x20 check|preflight               platform/arch/bundle/SLS-symbol preflight (no UI/hooks)\n\
           \x20 ax-enumerate [--max-windows N]  app/window counts only, no titles/content\n\
            \x20 focus-diagnose [--fallback] [--appkit]  read-only focus-oracle diagnosis (workspace+AX+CG legs, IDs/counts/statuses only; default raw systemwide+direct legs only, --fallback opts into experimental unproven evidence, --appkit opts into Accessory init)\n\
          \x20 secure-input                  observe Secure Input state only\n\
           \x20 spaces-diagnose [--wid WID [--pid PID --nonce HEX]]  read-only SLS display/Space/current/membership diagnostics\n\
           \x20 cg-owned-windows --pid PID --nonce HEX  read-only CGWindowList identity for the owned PID (no AX)\n\
           \x20 scope-inspect --scope BUNDLE [--scope BUNDLE ...]  read-only scoped metadata for named apps (AX role/frame/CG join/classify/Space, IDs only; no titles/content/paths; no writes)\n\
           \n\
            bounded owned-probe modes (require spawned probe target identity):\n\
            \x20 ax-observe --pid PID --nonce HEX --timeout-ms MS\n\
            \x20 \x20 (observes the owned session only; arbitrary/foreign PIDs refused)\n\
            \x20 ax-session [--seconds N]\n\
            \x20 \x20 (single bounded Child-holder AX session: focus-restoration proof\n\
            \x20 \x20 first, then pre-window + window-joined observers with driven\n\
            \x20 \x20 move/resize, owned self-activation, raise with readback,\n\
            \x20 \x20 frontmost restore, and a fixed-size partial-write leg;\n\
            \x20 \x20 no cross-command title auth inside)\n\
            \x20 focus-session [--seconds N] [--appkit]\n\
            \x20 \x20 (bounded Child-holder SLPS focus: original workspace+AX+CG\n\
            \x20 \x20 preimage, SAME-route unfocus-first when spawn already\n\
            \x20 \x20 focused (non-vacuous proof), SLPS focus to owned window\n\
            \x20 \x20 with agreement, SAME-route restore with agreement;\n\
            \x20 \x20 no activation API; default raw init, --appkit opts into\n\
            \x20 \x20 Accessory init for discrimination)\n\
            \x20 ax-set-frame --pid PID --nonce HEX --wid WID\n\
           \x20 ax-raise --pid PID --nonce HEX --wid WID\n\
           \x20 \x20 (refuses when focus preimage is foreign; never steals focus)\n\
           \x20 input-register [--seconds N]  Carbon Ctrl chord, finite (max {max_tap}s)\n\
            \x20 input-tap [--seconds N] [--suppress]  CGEventTap Ctrl-chord, finite RAII (max {max_tap}s)\n\
            \x20 \x20 (default observe-only, Keep always; --suppress arms exact-chord\n\
            \x20 \x20 KeyDown/KeyUp pair suppression only, unrelated keys always pass)\n\
             \x20 input-session --route carbon|tap [--seconds N] [--post-route session|hid] [--stall-once-ms MS] [--direct-disable-hold-ms MS] [--direct-rearm] [--watchdog-only]\n\
             \x20 \x20 (bounded owned input fixture over the REAL native tile source:\n\
             \x20 \x20 dedicated owner thread, owned helper window, synthetic CG posts\n\
             \x20 \x20 with per-post oracle proof, helper delivery counters; --stall-once-ms\n\
             \x20 \x20 is tap-only and provokes a real OS timeout-disable; --direct-disable-hold-ms\n\
             \x20 \x20 is tap-only and parks the tap DIRECTLY OS-disabled for the exact-post\n\
             \x20 \x20 passthrough plus recovery proof (actual CGEventTapIsEnabled readbacks);\n\
             \x20 \x20 --direct-rearm is tap-only and re-arms EXPLICITLY after an intentional\n\
             \x20 \x20 own-fault DIRECT disable (old guard drops, FRESH source on the same owner);\n\
            \x20 \x20 --watchdog-only is tap-only 2..=5s with no helper/posts: arms the tap,\n\
            \x20 \x20 parks past the hard watchdog, dirty exit is the evidence; input LIVE only,\n\
            \x20 \x20 no tiling, no frame writes)\n\
           \x20 tile --user-start --scope BUNDLE [--scope BUNDLE ...] [--seconds N] [--input carbon|tap|off]\n\
           \x20 \x20 (bounded scoped tile session over named apps: explicit --user-start\n\
           \x20 \x20 plus a non-empty scope are mandatory, Ghostty/own/unknown bundles\n\
           \x20 \x20 never write, finite lifetime 1..={max_probe}s (default {default_tile}s),\n\
           \x20 \x20 discrete Carbon hotkeys by default, tap only with --input tap,\n\
           \x20 \x20 off parks; stop leaves windows in place, never restores)\n\
           \x20 spaces-move --pid PID --nonce HEX --wid WID --space ID --allow-untested\n\
           \x20 \x20 (direct-SLS owned move with preimage + effect readback; UNTESTED live;\n\
           \x20 \x20 explicit opt-in per invocation; never auto-restores — run spaces-restore\n\
           \x20 \x20 separately to prove independent restore)\n\
           \x20 spaces-restore --pid PID --nonce HEX --wid WID --space ID --allow-untested\n\
           \x20 \x20 (independent restore with the same identity validation; usable after mover exit)\n\
           \x20 spaces-switch --pid PID --nonce HEX --space ID\n\
           \x20 spaces-create --pid PID --nonce HEX\n\
           \x20 spaces-destroy --pid PID --nonce HEX --space ID\n\
           \x20 \x20 (explicitly unsupported: no implemented route with proven recovery;\n\
           \x20 \x20 report the route, never execute — surveyed non-SA alternatives are\n\
           \x20 \x20 visible AX Mission Control automation (create/remove/goto) and global\n\
           \x20 \x20 synthetic gestures / drag+system-chord (switch/move); all unimplemented\n\
           \x20 \x20 here with recovery unproven, never a SIP rejection)\n\
              \x20 probe-spawn [--hung-ms MS] [--slow-ms MS] [--seconds N] [--fixed-size] [--regular-policy] [--close-window-ms MS] [--transient-panel-ms MS] [--transient-sheet-ms MS]\n\
              \x20 \x20 (bounded owned holder: spawns one helper and holds its exact\n\
              \x20 \x20 Child to the deadline; --regular-policy prefers the dedicated\n\
              \x20 \x20 signed helper .app executable (tile-admittable helper bundle);\n\
              \x20 \x20 --close-window-ms self-closes the owned window at that deadline\n\
              \x20 \x20 while the app stays alive to its seconds deadline;\n\
              \x20 \x20 --transient-panel-ms opens a native utility panel that\n\
              \x20 \x20 self-closes at that deadline while the main window stays\n\
              \x20 \x20 alive; --transient-sheet-ms presents a true attached\n\
              \x20 \x20 sheet that dismisses at that deadline, parent stays alive)\n\
             \x20 owned-frame --pid PID --nonce HEX --wid WID\n\
             \x20 \x20 (read-only owned window state: AX + CG frames, minimized,\n\
             \x20 \x20 fullscreen, role; IDs/geometry only, never titles/content)\n\
              \x20 owned-minimize --pid PID --nonce HEX --wid WID (--minimize | --restore)\n\
              \x20 \x20 (explicit owned minimize/restore with preimage + readback proof)\n\
              \x20 owned-fullscreen --pid PID --nonce HEX --wid WID (--enter | --exit)\n\
              \x20 \x20 (fixture-only owned native-fullscreen enter/exit with\n\
              \x20 \x20 preimage + readback proof; helper preflight only, never\n\
              \x20 \x20 a shipped feature)\n\
             \x20 owned-post --pid PID --nonce HEX --chord NAME\n\
             \x20 \x20 (ONE semantic tile chord through the native session stream:\n\
             \x20 \x20 focus-h/j/k/l, focus-left/right, move-h/j/k/l, move-left/right,\n\
             \x20 \x20 float-g; truthfully synthetic, oracle-gated per edge, helper\n\
             \x20 \x20 targets only, never Ghostty)\n\
             \x20 owned-focus --pid PID --nonce HEX --wid WID\n\
             \x20 \x20 (SLPS focus dispatch to one exact owned window with agreement)\n\
           \x20 spaces-session [--seconds N] [--allow-untested]\n\
           \x20 \x20 (single bounded Child-holder session: lifecycle proofs, CG identity,\n\
           \x20 \x20 membership/topology/permission readbacks; dispatches no Space writes;\n\
           \x20 \x20 never cross-command title auth)\n\
            \x20 probe-stop --pid PID --nonce HEX (exact-identity stop only)\n",
        bundle = crate::BUNDLE_ID,
        max_tap = crate::config::MAX_TAP_SECONDS,
        max_probe = crate::config::MAX_PROBE_SECONDS,
        default_tile = DEFAULT_TILE_SECONDS,
    )
}

fn parse_u32(s: &str, what: &str) -> Result<u32, String> {
    s.parse::<u32>()
        .map_err(|_| format!("{what} must be a u32, got '{s}'"))
}

fn parse_u64(s: &str, what: &str) -> Result<u64, String> {
    s.parse::<u64>()
        .map_err(|_| format!("{what} must be a u64, got '{s}'"))
}

fn take_value(args: &[String], i: &mut usize, flag: &str) -> Result<String, String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or_else(|| format!("{flag} needs a value"))
}

/// Pure argv parser (argv[0] = program name). No I/O, no UI, no hooks.
pub fn parse_args(argv: &[String]) -> Result<CliRequest, String> {
    let mut modifier = ProbeModifier::Ctrl;
    let mut rest: Vec<String> = Vec::new();
    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            "--ctrl" | "--modifier" | "--meta" => {
                let v = take_value(argv, &mut i, "--ctrl")?;
                modifier = ProbeModifier::parse(&v)?;
            }
            other if other.starts_with("--ctrl=") => {
                modifier = ProbeModifier::parse(other.trim_start_matches("--ctrl="))?;
            }
            other if other.starts_with("--meta=") => {
                modifier = ProbeModifier::parse(other.trim_start_matches("--meta="))?;
            }
            other if other.starts_with("--modifier=") => {
                modifier = ProbeModifier::parse(other.trim_start_matches("--modifier="))?;
            }
            _ => rest.push(argv[i].clone()),
        }
        i += 1;
    }
    let (cmd, args) = match rest.split_first() {
        None => {
            return Ok(CliRequest {
                modifier,
                command: Command::Preflight,
                may_mutate: false,
            });
        }
        Some((c, a)) => (c.as_str(), a),
    };
    let command = match cmd {
        "help" | "--help" | "-h" => {
            if !args.is_empty() {
                return Err("help takes no arguments".to_owned());
            }
            Command::Help
        }
        "check" | "preflight" => {
            if !args.is_empty() {
                return Err("usage: tiler-macos preflight".to_owned());
            }
            Command::Preflight
        }
        "ax-enumerate" => {
            let mut max_windows = crate::config::DEFAULT_MAX_WINDOWS;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--max-windows" => {
                        j += 1;
                        max_windows = parse_u64(
                            args.get(j).ok_or("--max-windows needs a value")?,
                            "--max-windows",
                        )? as usize;
                    }
                    other => return Err(format!("ax-enumerate: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::AxEnumerate { max_windows }
        }
        "ax-observe" => {
            let (pid, nonce, timeout_ms) = parse_owned_observe(args)?;
            Command::AxObserve {
                pid,
                nonce,
                timeout_ms,
            }
        }
        "ax-session" => {
            let mut seconds = DEFAULT_PROBE_SECONDS;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--seconds" => {
                        j += 1;
                        seconds =
                            parse_u64(args.get(j).ok_or("--seconds needs a value")?, "--seconds")?;
                    }
                    other => return Err(format!("ax-session: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::AxSession { seconds }
        }
        "focus-session" => {
            let mut seconds = DEFAULT_PROBE_SECONDS;
            let mut appkit = false;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--seconds" => {
                        j += 1;
                        seconds =
                            parse_u64(args.get(j).ok_or("--seconds needs a value")?, "--seconds")?;
                    }
                    "--appkit" => {
                        appkit = true;
                    }
                    other => return Err(format!("focus-session: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::FocusSession { seconds, appkit }
        }
        "focus-diagnose" => {
            let mut fallback = false;
            let mut appkit = false;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--fallback" => {
                        fallback = true;
                    }
                    "--appkit" => {
                        appkit = true;
                    }
                    other => return Err(format!("focus-diagnose: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::FocusDiagnose { fallback, appkit }
        }
        "ax-set-frame" => {
            let (pid, nonce, wid) = parse_owned_wid(args, "ax-set-frame")?;
            Command::AxSetFrame { pid, nonce, wid }
        }
        "ax-raise" => {
            let (pid, nonce, wid) = parse_owned_wid(args, "ax-raise")?;
            Command::AxRaise { pid, nonce, wid }
        }
        "input-register" | "input-tap" => {
            let mut seconds = DEFAULT_TAP_SECONDS;
            let mut suppress = false;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--seconds" => {
                        j += 1;
                        seconds =
                            parse_u64(args.get(j).ok_or("--seconds needs a value")?, "--seconds")?;
                    }
                    "--suppress" if cmd == "input-tap" => {
                        suppress = true;
                    }
                    other => return Err(format!("{cmd}: unknown arg '{other}'")),
                }
                j += 1;
            }
            if cmd == "input-register" {
                Command::InputRegister { seconds }
            } else {
                Command::InputTap { seconds, suppress }
            }
        }
        "input-session" => {
            // Bounded owned input fixture over the REAL native tile source:
            // explicit --route (carbon|tap) is mandatory, --stall-once-ms
            // is tap-only (fault-injection leg provoking a real OS
            // timeout-disable, never a manual callback),
            // --direct-disable-hold-ms is tap-only (DIRECT OS-disable plus
            // recovery proof with actual CGEventTapIsEnabled readbacks),
            // --direct-rearm is tap-only (EXPLICIT fresh re-arm after an
            // intentional own-fault DIRECT disable: old guard drops without a
            // same-tap re-enable, fresh source installs on the same owner),
            // --watchdog-only is tap-only 2..=5s with no helper/posts
            // (the dirty watchdog exit is the evidence).
            // --post-route session|hid selects the CG producer tap
            // (default session; hid is the fixture-only upstream
            // discriminator, same chords/owned window, no tier change).
            let mut seconds = crate::input_session::DEFAULT_INPUT_SECONDS;
            let mut route: Option<crate::input_session::InputSessionRoute> = None;
            let mut post_route = crate::input_session::PostRoute::Session;
            let mut stall_once_ms = 0u64;
            let mut direct_disable_hold_ms = 0u64;
            let mut direct_rearm = false;
            let mut watchdog_only = false;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--seconds" => {
                        j += 1;
                        seconds =
                            parse_u64(args.get(j).ok_or("--seconds needs a value")?, "--seconds")?;
                    }
                    "--route" => {
                        j += 1;
                        route = Some(crate::input_session::InputSessionRoute::parse(
                            args.get(j).ok_or("--route needs a value")?,
                        )?);
                    }
                    "--post-route" => {
                        j += 1;
                        post_route = crate::input_session::PostRoute::parse(
                            args.get(j).ok_or("--post-route needs a value")?,
                        )?;
                    }
                    "--stall-once-ms" => {
                        j += 1;
                        stall_once_ms = parse_u64(
                            args.get(j).ok_or("--stall-once-ms needs a value")?,
                            "--stall-once-ms",
                        )?;
                    }
                    "--direct-disable-hold-ms" => {
                        j += 1;
                        direct_disable_hold_ms = parse_u64(
                            args.get(j)
                                .ok_or("--direct-disable-hold-ms needs a value")?,
                            "--direct-disable-hold-ms",
                        )?;
                    }
                    "--watchdog-only" => {
                        watchdog_only = true;
                    }
                    "--direct-rearm" => {
                        direct_rearm = true;
                    }
                    other => return Err(format!("input-session: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::InputSession {
                seconds,
                route: route.ok_or("input-session needs --route carbon|tap")?,
                post_route,
                stall_once_ms,
                direct_disable_hold_ms,
                direct_rearm,
                watchdog_only,
            }
        }
        "secure-input" => {
            if !args.is_empty() {
                return Err("usage: tiler-macos secure-input".to_owned());
            }
            Command::SecureInput
        }
        "spaces-diagnose" => {
            let mut wid: Option<u32> = None;
            let mut pid: Option<u32> = None;
            let mut nonce: Option<String> = None;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--wid" => {
                        j += 1;
                        wid = Some(parse_u32(
                            args.get(j).ok_or("--wid needs a value")?,
                            "--wid",
                        )?);
                    }
                    "--pid" => {
                        j += 1;
                        pid = Some(parse_u32(
                            args.get(j).ok_or("--pid needs a value")?,
                            "--pid",
                        )?);
                    }
                    "--nonce" => {
                        j += 1;
                        nonce = Some(args.get(j).ok_or("--nonce needs a value")?.clone());
                    }
                    other => return Err(format!("spaces-diagnose: unknown arg '{other}'")),
                }
                j += 1;
            }
            if nonce.is_some() && pid.is_none() {
                return Err("spaces-diagnose --nonce needs --pid".to_owned());
            }
            Command::SpacesDiagnose { wid, pid, nonce }
        }
        "cg-owned-windows" => {
            let mut pid: Option<u32> = None;
            let mut nonce: Option<String> = None;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--pid" => {
                        j += 1;
                        pid = Some(parse_u32(
                            args.get(j).ok_or("--pid needs a value")?,
                            "--pid",
                        )?);
                    }
                    "--nonce" => {
                        j += 1;
                        nonce = Some(args.get(j).ok_or("--nonce needs a value")?.clone());
                    }
                    other => return Err(format!("cg-owned-windows: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::CgOwnedWindows {
                pid: pid.ok_or("cg-owned-windows needs --pid")?,
                nonce: nonce.ok_or("cg-owned-windows needs --nonce")?,
            }
        }
        "spaces-move" | "spaces-restore" => {
            let mut pid: Option<u32> = None;
            let mut nonce: Option<String> = None;
            let mut wid: Option<u32> = None;
            let mut space: Option<u64> = None;
            let mut allow_untested = false;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--pid" => {
                        j += 1;
                        pid = Some(parse_u32(
                            args.get(j).ok_or("--pid needs a value")?,
                            "--pid",
                        )?);
                    }
                    "--nonce" => {
                        j += 1;
                        nonce = Some(args.get(j).ok_or("--nonce needs a value")?.clone());
                    }
                    "--wid" => {
                        j += 1;
                        wid = Some(parse_u32(
                            args.get(j).ok_or("--wid needs a value")?,
                            "--wid",
                        )?);
                    }
                    "--space" => {
                        j += 1;
                        space = Some(parse_u64(
                            args.get(j).ok_or("--space needs a value")?,
                            "--space",
                        )?);
                    }
                    "--allow-untested" => {
                        allow_untested = true;
                    }
                    other => return Err(format!("{cmd}: unknown arg '{other}'")),
                }
                j += 1;
            }
            let pid = pid.ok_or(format!("{cmd} needs --pid"))?;
            let nonce = nonce.ok_or(format!("{cmd} needs --nonce"))?;
            let wid = wid.ok_or(format!("{cmd} needs --wid"))?;
            let space = space.ok_or(format!("{cmd} needs --space"))?;
            if cmd == "spaces-move" {
                Command::SpacesMove {
                    pid,
                    nonce,
                    wid,
                    space,
                    allow_untested,
                }
            } else {
                Command::SpacesRestore {
                    pid,
                    nonce,
                    wid,
                    space,
                    allow_untested,
                }
            }
        }
        "spaces-switch" => {
            let mut pid: Option<u32> = None;
            let mut nonce: Option<String> = None;
            let mut space: Option<u64> = None;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--pid" => {
                        j += 1;
                        pid = Some(parse_u32(
                            args.get(j).ok_or("--pid needs a value")?,
                            "--pid",
                        )?);
                    }
                    "--nonce" => {
                        j += 1;
                        nonce = Some(args.get(j).ok_or("--nonce needs a value")?.clone());
                    }
                    "--space" => {
                        j += 1;
                        space = Some(parse_u64(
                            args.get(j).ok_or("--space needs a value")?,
                            "--space",
                        )?);
                    }
                    other => return Err(format!("{cmd}: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::SpacesSwitch {
                pid: pid.ok_or("spaces-switch needs --pid")?,
                nonce: nonce.ok_or("spaces-switch needs --nonce")?,
                space: space.ok_or("spaces-switch needs --space")?,
            }
        }
        "spaces-create" => {
            let mut pid: Option<u32> = None;
            let mut nonce: Option<String> = None;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--pid" => {
                        j += 1;
                        pid = Some(parse_u32(
                            args.get(j).ok_or("--pid needs a value")?,
                            "--pid",
                        )?);
                    }
                    "--nonce" => {
                        j += 1;
                        nonce = Some(args.get(j).ok_or("--nonce needs a value")?.clone());
                    }
                    other => return Err(format!("{cmd}: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::SpacesCreate {
                pid: pid.ok_or("spaces-create needs --pid")?,
                nonce: nonce.ok_or("spaces-create needs --nonce")?,
            }
        }
        "spaces-destroy" => {
            let mut pid: Option<u32> = None;
            let mut nonce: Option<String> = None;
            let mut space: Option<u64> = None;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--pid" => {
                        j += 1;
                        pid = Some(parse_u32(
                            args.get(j).ok_or("--pid needs a value")?,
                            "--pid",
                        )?);
                    }
                    "--nonce" => {
                        j += 1;
                        nonce = Some(args.get(j).ok_or("--nonce needs a value")?.clone());
                    }
                    "--space" => {
                        j += 1;
                        space = Some(parse_u64(
                            args.get(j).ok_or("--space needs a value")?,
                            "--space",
                        )?);
                    }
                    other => return Err(format!("{cmd}: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::SpacesDestroy {
                pid: pid.ok_or("spaces-destroy needs --pid")?,
                nonce: nonce.ok_or("spaces-destroy needs --nonce")?,
                space: space.ok_or("spaces-destroy needs --space")?,
            }
        }
        "probe-spawn" => {
            let mut hung_ms = 0u64;
            let mut slow_ms = 0u64;
            let mut seconds = DEFAULT_PROBE_SECONDS;
            let mut fixed_size = false;
            let mut regular_policy = false;
            let mut close_window_ms = 0u64;
            let mut transient_panel_ms = 0u64;
            let mut transient_sheet_ms = 0u64;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--hung-ms" => {
                        j += 1;
                        hung_ms =
                            parse_u64(args.get(j).ok_or("--hung-ms needs a value")?, "--hung-ms")?;
                    }
                    "--slow-ms" => {
                        j += 1;
                        slow_ms =
                            parse_u64(args.get(j).ok_or("--slow-ms needs a value")?, "--slow-ms")?;
                    }
                    "--seconds" => {
                        j += 1;
                        seconds =
                            parse_u64(args.get(j).ok_or("--seconds needs a value")?, "--seconds")?;
                    }
                    "--fixed-size" => {
                        fixed_size = true;
                    }
                    "--regular-policy" => {
                        regular_policy = true;
                    }
                    "--close-window-ms" => {
                        j += 1;
                        close_window_ms = parse_u64(
                            args.get(j).ok_or("--close-window-ms needs a value")?,
                            "--close-window-ms",
                        )?;
                    }
                    "--transient-panel-ms" => {
                        j += 1;
                        transient_panel_ms = parse_u64(
                            args.get(j).ok_or("--transient-panel-ms needs a value")?,
                            "--transient-panel-ms",
                        )?;
                    }
                    "--transient-sheet-ms" => {
                        j += 1;
                        transient_sheet_ms = parse_u64(
                            args.get(j).ok_or("--transient-sheet-ms needs a value")?,
                            "--transient-sheet-ms",
                        )?;
                    }
                    other => return Err(format!("probe-spawn: unknown arg '{other}'")),
                }
                j += 1;
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
            }
        }
        "probe-stop" => {
            let mut pid: Option<u32> = None;
            let mut nonce: Option<String> = None;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--pid" => {
                        j += 1;
                        pid = Some(parse_u32(
                            args.get(j).ok_or("--pid needs a value")?,
                            "--pid",
                        )?);
                    }
                    "--nonce" => {
                        j += 1;
                        nonce = Some(args.get(j).ok_or("--nonce needs a value")?.clone());
                    }
                    other => return Err(format!("probe-stop: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::ProbeStop {
                pid: pid.ok_or("probe-stop needs --pid")?,
                nonce: nonce.ok_or("probe-stop needs --nonce")?,
            }
        }
        "owned-frame" => {
            let (pid, nonce, wid) = parse_owned_wid(args, "owned-frame")?;
            Command::OwnedFrame { pid, nonce, wid }
        }
        "owned-minimize" => {
            // Exactly one of --minimize / --restore (explicit, never a
            // default): minimize or restore one exact owned window.
            let mut pid: Option<u32> = None;
            let mut nonce: Option<String> = None;
            let mut wid: Option<u32> = None;
            let mut minimize: Option<bool> = None;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--pid" => {
                        j += 1;
                        pid = Some(parse_u32(
                            args.get(j).ok_or("--pid needs a value")?,
                            "--pid",
                        )?);
                    }
                    "--nonce" => {
                        j += 1;
                        nonce = Some(args.get(j).ok_or("--nonce needs a value")?.clone());
                    }
                    "--wid" => {
                        j += 1;
                        wid = Some(parse_u32(
                            args.get(j).ok_or("--wid needs a value")?,
                            "--wid",
                        )?);
                    }
                    "--minimize" => {
                        if minimize.is_some() {
                            return Err("owned-minimize needs exactly one of --minimize/--restore"
                                .to_owned());
                        }
                        minimize = Some(true);
                    }
                    "--restore" => {
                        if minimize.is_some() {
                            return Err("owned-minimize needs exactly one of --minimize/--restore"
                                .to_owned());
                        }
                        minimize = Some(false);
                    }
                    other => return Err(format!("owned-minimize: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::OwnedMinimize {
                pid: pid.ok_or("owned-minimize needs --pid")?,
                nonce: nonce.ok_or("owned-minimize needs --nonce")?,
                wid: wid.ok_or("owned-minimize needs --wid")?,
                minimize: minimize
                    .ok_or("owned-minimize needs exactly one of --minimize/--restore")?,
            }
        }
        "owned-fullscreen" => {
            // Exactly one of --enter / --exit (explicit, never a default):
            // enter or exit native fullscreen on one exact owned window.
            // Fixture-only preflight control; never a shipped feature.
            let mut pid: Option<u32> = None;
            let mut nonce: Option<String> = None;
            let mut wid: Option<u32> = None;
            let mut fullscreen: Option<bool> = None;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--pid" => {
                        j += 1;
                        pid = Some(parse_u32(
                            args.get(j).ok_or("--pid needs a value")?,
                            "--pid",
                        )?);
                    }
                    "--nonce" => {
                        j += 1;
                        nonce = Some(args.get(j).ok_or("--nonce needs a value")?.clone());
                    }
                    "--wid" => {
                        j += 1;
                        wid = Some(parse_u32(
                            args.get(j).ok_or("--wid needs a value")?,
                            "--wid",
                        )?);
                    }
                    "--enter" => {
                        if fullscreen.is_some() {
                            return Err(
                                "owned-fullscreen needs exactly one of --enter/--exit".to_owned()
                            );
                        }
                        fullscreen = Some(true);
                    }
                    "--exit" => {
                        if fullscreen.is_some() {
                            return Err(
                                "owned-fullscreen needs exactly one of --enter/--exit".to_owned()
                            );
                        }
                        fullscreen = Some(false);
                    }
                    other => return Err(format!("owned-fullscreen: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::OwnedFullscreen {
                pid: pid.ok_or("owned-fullscreen needs --pid")?,
                nonce: nonce.ok_or("owned-fullscreen needs --nonce")?,
                wid: wid.ok_or("owned-fullscreen needs --wid")?,
                fullscreen: fullscreen
                    .ok_or("owned-fullscreen needs exactly one of --enter/--exit")?,
            }
        }
        "owned-post" => {
            let mut pid: Option<u32> = None;
            let mut nonce: Option<String> = None;
            let mut chord: Option<String> = None;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--pid" => {
                        j += 1;
                        pid = Some(parse_u32(
                            args.get(j).ok_or("--pid needs a value")?,
                            "--pid",
                        )?);
                    }
                    "--nonce" => {
                        j += 1;
                        nonce = Some(args.get(j).ok_or("--nonce needs a value")?.clone());
                    }
                    "--chord" => {
                        j += 1;
                        chord = Some(args.get(j).ok_or("--chord needs a value")?.clone());
                    }
                    other => return Err(format!("owned-post: unknown arg '{other}'")),
                }
                j += 1;
            }
            let chord = chord.ok_or("owned-post needs --chord")?;
            if crate::owned_control::resolve_chord(&chord).is_none() {
                return Err(format!("owned-post: unknown --chord '{chord}'"));
            }
            Command::OwnedPost {
                pid: pid.ok_or("owned-post needs --pid")?,
                nonce: nonce.ok_or("owned-post needs --nonce")?,
                chord,
            }
        }
        "owned-focus" => {
            let (pid, nonce, wid) = parse_owned_wid(args, "owned-focus")?;
            Command::OwnedFocus { pid, nonce, wid }
        }
        "tile" => {
            // Scoped tile session: explicit `--user-start` plus a non-empty
            // `--scope` allowlist are mandatory (no implicit scope or start);
            // the lifetime is finite and bounded, the input route explicit.
            let mut user_start = false;
            let mut scopes: Vec<String> = Vec::new();
            let mut seconds = DEFAULT_TILE_SECONDS;
            let mut input = TileInput::default();
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--user-start" => {
                        user_start = true;
                    }
                    "--scope" => {
                        j += 1;
                        scopes.push(args.get(j).ok_or("--scope needs a value")?.clone());
                    }
                    "--seconds" => {
                        j += 1;
                        seconds =
                            parse_u64(args.get(j).ok_or("--seconds needs a value")?, "--seconds")?;
                    }
                    "--input" => {
                        j += 1;
                        input = TileInput::parse(args.get(j).ok_or("--input needs a value")?)?;
                    }
                    other => return Err(format!("tile: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::Tile {
                user_start,
                scopes,
                seconds,
                input,
            }
        }
        "spaces-session" => {
            let mut seconds = DEFAULT_PROBE_SECONDS;
            let mut allow_untested = false;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--seconds" => {
                        j += 1;
                        seconds =
                            parse_u64(args.get(j).ok_or("--seconds needs a value")?, "--seconds")?;
                    }
                    "--allow-untested" => {
                        allow_untested = true;
                    }
                    other => return Err(format!("spaces-session: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::SpacesSession {
                seconds,
                allow_untested,
            }
        }
        "scope-inspect" => {
            // Read-only scoped metadata for named apps: non-empty --scope
            // allowlist mandatory; no writes on any path.
            let mut scopes: Vec<String> = Vec::new();
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--scope" => {
                        j += 1;
                        scopes.push(args.get(j).ok_or("--scope needs a value")?.clone());
                    }
                    other => return Err(format!("scope-inspect: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::ScopeInspect { scopes }
        }
        other => return Err(format!("unknown command '{other}' (try: help)")),
    };
    let may_mutate = !matches!(
        command,
        Command::Help
            | Command::Preflight
            | Command::AxEnumerate { .. }
            | Command::FocusDiagnose { .. }
            | Command::SecureInput
            | Command::SpacesDiagnose { .. }
            | Command::CgOwnedWindows { .. }
            | Command::OwnedFrame { .. }
            | Command::ScopeInspect { .. }
    );
    Ok(CliRequest {
        modifier,
        command,
        may_mutate,
    })
}

fn parse_owned_observe(args: &[String]) -> Result<(u32, String, u64), String> {
    let mut pid: Option<u32> = None;
    let mut nonce: Option<String> = None;
    let mut timeout_ms = DEFAULT_AX_TIMEOUT_MS;
    let mut j = 0;
    while j < args.len() {
        match args[j].as_str() {
            "--pid" => {
                j += 1;
                pid = Some(parse_u32(
                    args.get(j).ok_or("--pid needs a value")?,
                    "--pid",
                )?);
            }
            "--nonce" => {
                j += 1;
                nonce = Some(args.get(j).ok_or("--nonce needs a value")?.clone());
            }
            "--timeout-ms" => {
                j += 1;
                timeout_ms = parse_u64(
                    args.get(j).ok_or("--timeout-ms needs a value")?,
                    "--timeout-ms",
                )?;
            }
            other => return Err(format!("ax-observe: unknown arg '{other}'")),
        }
        j += 1;
    }
    Ok((
        pid.ok_or("ax-observe needs --pid")?,
        nonce.ok_or("ax-observe needs --nonce (owned session only)")?,
        timeout_ms,
    ))
}

fn parse_owned_wid(args: &[String], cmd: &str) -> Result<(u32, String, u32), String> {
    let mut pid: Option<u32> = None;
    let mut nonce: Option<String> = None;
    let mut wid: Option<u32> = None;
    let mut j = 0;
    while j < args.len() {
        match args[j].as_str() {
            "--pid" => {
                j += 1;
                pid = Some(parse_u32(
                    args.get(j).ok_or("--pid needs a value")?,
                    "--pid",
                )?);
            }
            "--nonce" => {
                j += 1;
                nonce = Some(args.get(j).ok_or("--nonce needs a value")?.clone());
            }
            "--wid" => {
                j += 1;
                wid = Some(parse_u32(
                    args.get(j).ok_or("--wid needs a value")?,
                    "--wid",
                )?);
            }
            other => return Err(format!("{cmd}: unknown arg '{other}'")),
        }
        j += 1;
    }
    Ok((
        pid.ok_or(format!("{cmd} needs --pid"))?,
        nonce.ok_or(format!("{cmd} needs --nonce"))?,
        wid.ok_or(format!("{cmd} needs --wid"))?,
    ))
}
