#![forbid(unsafe_code)]

//! CLI contract. `help` and `check`/`preflight` (the default with no args)
//! never launch UI, event taps, AX control, or Space moves. Every other mode
//! is explicit and bounded.

use crate::config::{
    DEFAULT_AX_TIMEOUT_MS, DEFAULT_PROBE_SECONDS, DEFAULT_TAP_SECONDS, ProbeModifier,
};

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
    },
    SpacesSession {
        seconds: u64,
        allow_untested: bool,
    },
    ProbeStop {
        pid: u32,
        nonce: String,
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
         \n\
         nonmutating:\n\
          \x20 help                          this text\n\
          \x20 check|preflight               platform/arch/bundle/SLS-symbol preflight (no UI/hooks)\n\
          \x20 ax-enumerate [--max-windows N]  app/window counts only, no titles/content\n\
          \x20 secure-input                  observe Secure Input state only\n\
           \x20 spaces-diagnose [--wid WID [--pid PID --nonce HEX]]  read-only SLS display/Space/current/membership diagnostics\n\
          \x20 cg-owned-windows --pid PID --nonce HEX  read-only CGWindowList identity for the owned PID (no AX)\n\
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
            \x20 focus-session [--seconds N]\n\
            \x20 \x20 (bounded Child-holder SLPS focus: original workspace+AX+CG\n\
            \x20 \x20 preimage, SAME-route unfocus-first when spawn already\n\
            \x20 \x20 focused (non-vacuous proof), SLPS focus to owned window\n\
            \x20 \x20 with agreement, SAME-route restore with agreement;\n\
            \x20 \x20 no activation API)\n\
            \x20 ax-set-frame --pid PID --nonce HEX --wid WID\n\
           \x20 ax-raise --pid PID --nonce HEX --wid WID\n\
           \x20 \x20 (refuses when focus preimage is foreign; never steals focus)\n\
           \x20 input-register [--seconds N]  Carbon Ctrl chord, finite (max {max_tap}s)\n\
           \x20 input-tap [--seconds N] [--suppress]  CGEventTap Ctrl-chord, finite RAII (max {max_tap}s)\n\
           \x20 \x20 (default observe-only, Keep always; --suppress arms exact-chord\n\
           \x20 \x20 KeyDown/KeyUp pair suppression only, unrelated keys always pass)\n\
           \x20 spaces-move --pid PID --nonce HEX --wid WID --space ID --allow-untested\n\
           \x20 \x20 (direct-SLS owned move with preimage + effect readback; UNTESTED live;\n\
           \x20 \x20 explicit opt-in per invocation; never auto-restores — run spaces-restore\n\
           \x20 \x20 separately to prove independent restore)\n\
           \x20 spaces-restore --pid PID --nonce HEX --wid WID --space ID --allow-untested\n\
           \x20 \x20 (independent restore with the same identity validation; usable after mover exit)\n\
           \x20 spaces-switch --pid PID --nonce HEX --space ID\n\
           \x20 spaces-create --pid PID --nonce HEX\n\
           \x20 spaces-destroy --pid PID --nonce HEX --space ID\n\
           \x20 \x20 (explicitly unsupported: scripting-addition-gated; report the route, never execute)\n\
           \x20 probe-spawn [--hung-ms MS] [--slow-ms MS] [--seconds N]\n\
           \x20 spaces-session [--seconds N] [--allow-untested]\n\
           \x20 \x20 (single bounded Child-holder session: lifecycle proofs, CG identity,\n\
           \x20 \x20 membership/topology/permission readbacks; dispatches no Space writes;\n\
           \x20 \x20 never cross-command title auth)\n\
           \x20 probe-stop --pid PID --nonce HEX (exact-identity stop only)\n",
        bundle = crate::BUNDLE_ID,
        max_tap = crate::config::MAX_TAP_SECONDS,
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
    let mut modifier = ProbeModifier::Meta;
    let mut rest: Vec<String> = Vec::new();
    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            "--ctrl" | "--modifier" => {
                let v = take_value(argv, &mut i, "--ctrl")?;
                modifier = ProbeModifier::parse(&v)?;
            }
            other if other.starts_with("--ctrl=") => {
                modifier = ProbeModifier::parse(other.trim_start_matches("--ctrl="))?;
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
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--seconds" => {
                        j += 1;
                        seconds =
                            parse_u64(args.get(j).ok_or("--seconds needs a value")?, "--seconds")?;
                    }
                    other => return Err(format!("focus-session: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::FocusSession { seconds }
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
                    other => return Err(format!("probe-spawn: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::ProbeSpawn {
                hung_ms,
                slow_ms,
                seconds,
                fixed_size,
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
        other => return Err(format!("unknown command '{other}' (try: help)")),
    };
    let may_mutate = !matches!(
        command,
        Command::Help
            | Command::Preflight
            | Command::AxEnumerate { .. }
            | Command::SecureInput
            | Command::SpacesDiagnose { .. }
            | Command::CgOwnedWindows { .. }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(words: &[&str]) -> Vec<String> {
        std::iter::once("tiler-macos".to_owned())
            .chain(words.iter().map(|s| s.to_string()))
            .collect()
    }

    #[test]
    fn default_is_nonmutating_preflight() {
        let r = parse_args(&argv(&[])).unwrap();
        assert_eq!(r.command, Command::Preflight);
        assert!(!r.may_mutate);
    }

    #[test]
    fn help_and_check_never_mutate() {
        for cmd in [
            "help",
            "check",
            "preflight",
            "secure-input",
            "spaces-diagnose",
        ] {
            let r = parse_args(&argv(&[cmd])).unwrap();
            assert!(!r.may_mutate, "{cmd} must not mutate");
        }
        let r = parse_args(&argv(&["ax-enumerate"])).unwrap();
        assert!(!r.may_mutate);
    }

    #[test]
    fn ctrl_defaults_to_meta() {
        let r = parse_args(&argv(&["preflight"])).unwrap();
        assert_eq!(r.modifier, ProbeModifier::Meta);
        let r = parse_args(&argv(&["--ctrl", "ctrl", "preflight"])).unwrap();
        assert_eq!(r.modifier, ProbeModifier::Ctrl);
    }

    #[test]
    fn owned_modes_require_identity() {
        assert!(parse_args(&argv(&["ax-set-frame", "--pid", "9"])).is_err());
        assert!(parse_args(&argv(&["ax-observe", "--pid", "9"])).is_err());
        assert!(
            parse_args(&argv(&[
                "ax-observe",
                "--pid",
                "9",
                "--nonce",
                "0123456789abcdef",
            ]))
            .unwrap()
            .may_mutate
        );
        let r = parse_args(&argv(&[
            "ax-set-frame",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
            "--wid",
            "3",
        ]))
        .unwrap();
        assert!(r.may_mutate);
    }

    #[test]
    fn input_tap_suppress_is_explicit_opt_in() {
        let r = parse_args(&argv(&["input-tap"])).unwrap();
        assert!(matches!(
            r.command,
            Command::InputTap {
                suppress: false,
                ..
            }
        ));
        let r = parse_args(&argv(&["input-tap", "--suppress"])).unwrap();
        assert!(matches!(
            r.command,
            Command::InputTap { suppress: true, .. }
        ));
        // Registration has no suppression channel at all.
        assert!(parse_args(&argv(&["input-register", "--suppress"])).is_err());
    }

    #[test]
    fn spaces_diagnose_stays_read_only_with_optional_wid() {
        let r = parse_args(&argv(&["spaces-diagnose"])).unwrap();
        assert!(!r.may_mutate);
        assert!(matches!(
            r.command,
            Command::SpacesDiagnose { wid: None, .. }
        ));
        let r = parse_args(&argv(&["spaces-diagnose", "--wid", "7"])).unwrap();
        assert!(!r.may_mutate);
        assert!(matches!(
            r.command,
            Command::SpacesDiagnose { wid: Some(7), .. }
        ));
        // Owned membership read stays read-only too.
        let r = parse_args(&argv(&[
            "spaces-diagnose",
            "--wid",
            "7",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
        ]))
        .unwrap();
        assert!(!r.may_mutate);
        assert!(parse_args(&argv(&["spaces-diagnose", "--nonce", "0123456789abcdef"])).is_err());
    }

    #[test]
    fn cg_owned_windows_is_read_only_with_identity() {
        let r = parse_args(&argv(&[
            "cg-owned-windows",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
        ]))
        .unwrap();
        assert!(!r.may_mutate);
        assert!(matches!(r.command, Command::CgOwnedWindows { .. }));
        assert!(parse_args(&argv(&["cg-owned-windows", "--pid", "9"])).is_err());
    }

    #[test]
    fn spaces_session_is_bounded_holder_with_explicit_opt_in() {
        let r = parse_args(&argv(&["spaces-session"])).unwrap();
        assert!(r.may_mutate);
        assert!(matches!(
            r.command,
            Command::SpacesSession {
                seconds: DEFAULT_PROBE_SECONDS,
                allow_untested: false,
            }
        ));
        let r = parse_args(&argv(&[
            "spaces-session",
            "--seconds",
            "20",
            "--allow-untested",
        ]))
        .unwrap();
        assert!(matches!(
            r.command,
            Command::SpacesSession {
                seconds: 20,
                allow_untested: true,
            }
        ));
        assert!(parse_args(&argv(&["spaces-session", "--bogus"])).is_err());
    }

    #[test]
    fn ax_session_is_bounded_holder_with_seconds_cap() {
        let r = parse_args(&argv(&["ax-session"])).unwrap();
        assert!(r.may_mutate);
        assert!(matches!(
            r.command,
            Command::AxSession {
                seconds: DEFAULT_PROBE_SECONDS,
            }
        ));
        let r = parse_args(&argv(&["ax-session", "--seconds", "20"])).unwrap();
        assert!(matches!(r.command, Command::AxSession { seconds: 20 }));
        assert!(parse_args(&argv(&["ax-session", "--bogus"])).is_err());
    }

    #[test]
    fn focus_session_is_bounded_holder() {
        let r = parse_args(&argv(&["focus-session"])).unwrap();
        assert!(r.may_mutate);
        assert!(matches!(
            r.command,
            Command::FocusSession {
                seconds: DEFAULT_PROBE_SECONDS,
            }
        ));
        let r = parse_args(&argv(&["focus-session", "--seconds", "20"])).unwrap();
        assert!(matches!(r.command, Command::FocusSession { seconds: 20 }));
        assert!(parse_args(&argv(&["focus-session", "--bogus"])).is_err());
    }

    #[test]
    fn probe_spawn_fixed_size_is_explicit_opt_in() {
        let r = parse_args(&argv(&["probe-spawn"])).unwrap();
        assert!(matches!(
            r.command,
            Command::ProbeSpawn {
                fixed_size: false,
                ..
            }
        ));
        let r = parse_args(&argv(&["probe-spawn", "--fixed-size"])).unwrap();
        assert!(matches!(
            r.command,
            Command::ProbeSpawn {
                fixed_size: true,
                ..
            }
        ));
    }

    #[test]
    fn space_write_modes_require_full_identity_and_may_mutate() {
        for args in [
            vec![
                "spaces-move",
                "--pid",
                "9",
                "--nonce",
                "0123456789abcdef",
                "--wid",
                "3",
                "--space",
                "5",
                "--allow-untested",
            ],
            vec![
                "spaces-restore",
                "--pid",
                "9",
                "--nonce",
                "0123456789abcdef",
                "--wid",
                "3",
                "--space",
                "5",
                "--allow-untested",
            ],
            vec![
                "spaces-switch",
                "--pid",
                "9",
                "--nonce",
                "0123456789abcdef",
                "--space",
                "5",
            ],
            vec!["spaces-create", "--pid", "9", "--nonce", "0123456789abcdef"],
            vec![
                "spaces-destroy",
                "--pid",
                "9",
                "--nonce",
                "0123456789abcdef",
                "--space",
                "5",
            ],
        ] {
            let r = parse_args(&argv(&args)).unwrap();
            assert!(r.may_mutate, "{args:?} is write-intent");
        }
        // The untested prerequisite defaults off (explicit opt-in only).
        let r = parse_args(&argv(&[
            "spaces-move",
            "--pid",
            "9",
            "--nonce",
            "0123456789abcdef",
            "--wid",
            "3",
            "--space",
            "5",
        ]))
        .unwrap();
        assert!(matches!(
            r.command,
            Command::SpacesMove {
                allow_untested: false,
                ..
            }
        ));
        assert!(
            parse_args(&argv(&[
                "spaces-move",
                "--pid",
                "9",
                "--nonce",
                "0123456789abcdef",
                "--wid",
                "3",
            ]))
            .is_err()
        );
        assert!(
            parse_args(&argv(&[
                "spaces-switch",
                "--pid",
                "9",
                "--nonce",
                "0123456789abcdef",
            ]))
            .is_err()
        );
    }
}
