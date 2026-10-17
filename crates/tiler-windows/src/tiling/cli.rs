#![forbid(unsafe_code)]
use super::*;

/// CLI options for the read-only `capture` command: an explicit target list
/// only. There is no implicit all-window capture; every receipt window must
/// be named with a repeated `--hwnd`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureOptions {
    pub out: PathBuf,
    pub hwnds: Vec<u64>,
}

/// Parse `capture --out PATH --hwnd HWND [--hwnd HWND ...]`. At least one
/// `--hwnd` is required; zero targets is a refusal, never an all-window
/// capture.
pub fn parse_capture_args(args: &[String]) -> Result<CaptureOptions, String> {
    let usage = "usage: capture --out PATH --hwnd HWND [--hwnd HWND ...]";
    let mut out: Option<PathBuf> = None;
    let mut hwnds: Vec<u64> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--out" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                out = Some(PathBuf::from(value));
                i += 1;
            }
            "--hwnd" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                let hwnd = crate::test_window::parse_hwnd(value).ok_or_else(|| usage.to_owned())?;
                if !hwnds.contains(&hwnd) {
                    hwnds.push(hwnd);
                }
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    let Some(out) = out else {
        return Err(usage.to_owned());
    };
    if hwnds.is_empty() {
        return Err("refuse: capture requires at least one --hwnd".to_owned());
    }
    Ok(CaptureOptions { out, hwnds })
}

/// CLI options for the normal `tile` command (user dogfood only).
/// `seconds: None` runs until an exact-owner stop request. Terminal windows
/// are ordinary tile targets and require explicit `--user-start`: agents
/// never run this path. `--allowlist` is refused here; proof uses
/// `tile-proof` so lost arguments can never fall back to normal mode.
///
/// Optional repeatable `--scope-exe NAME` restricts management to the named
/// executables (top-level exe basename, case-insensitive; `Calculator` means
/// its `ApplicationFrameHost.exe` host). Empty (default) means no filter:
/// every eligible window is managed. A scoped run fences observation,
/// geometry writes, workspace hide/reveal membership, and focus actuation to
/// the named exes; anything else is reported `scope-excluded` with no writes.
///
/// Keyboard takeover defaults ON (Win+H/J/K/L and Win+arrows focus, Shift
/// variants move, per the KDE catalog): `--no-keyboard-snap-takeover` turns it
/// visibly off. Unshifted Win+L stays gated behind `--allow-win-l` (default
/// off); Win+Shift+L is approved without it.
///
/// Session-only mouse-Snap prevention (`SPI_SETWINARRANGING FALSE` while
/// product tiling is active) defaults ON as well:
/// `--no-mouse-snap-prevention` turns it visibly off. The exact preimage is
/// persisted before the first setter with readback, and restored
/// conditionally on stop (or independent restore after a crash) only while
/// the live value is still ours.
#[derive(Debug, Clone, PartialEq)]
pub struct TileOptions {
    pub seconds: Option<u64>,
    pub trace: bool,
    pub user_start: bool,
    pub no_keyboard_snap_takeover: bool,
    pub allow_win_l: bool,
    pub no_mouse_snap_prevention: bool,
    pub border: crate::active_border::ActiveBorderOptions,
    pub underlay: crate::group_underlay::GroupUnderlayOptions,
    /// Live inner gap (KDE 0..64, default 8). Saved settings supply the base;
    /// `--inner-gap` overrides explicitly.
    pub inner_gap: i32,
    /// Live outer gap (KDE 0..64, default 8). Saved settings supply the base;
    /// `--outer-gap` overrides explicitly.
    pub outer_gap: i32,
    pub scope_exes: Vec<String>,
    /// Explicit host-to-child scope pairs (repeatable `--scope-host-child
    /// HOST=CHILD`). Empty (default) means no child constraint. A listed host
    /// passes observation and hide admission only while a live hosted child
    /// matches, so a newly appearing hosted app can never ride another app's
    /// host executable into management.
    pub scope_hosts: Vec<ScopeHostChild>,
}

/// Basename of one exe path for scope matching: after the last `\` or `/`,
/// lowercased. Empty input maps to empty.
#[must_use]
pub fn scope_exe_basename(exe_path: &str) -> String {
    exe_path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(exe_path)
        .to_ascii_lowercase()
}

/// True when `exe_path` passes the explicit scope filter. An empty scope
/// allows everything (no default filter); a nonempty scope allows only the
/// named basenames (case-insensitive, slash-insensitive).
#[must_use]
pub fn scope_allows(scope: &[String], exe_path: &str) -> bool {
    if scope.is_empty() {
        return true;
    }
    let base = scope_exe_basename(exe_path);
    scope.iter().any(|entry| scope_exe_basename(entry) == base)
}

/// One explicit host-to-child scope pair: top-level windows of `host` pass
/// only while at least one live hosted (different-process) child matches
/// `child`. Stored raw; matching normalizes basenames. Empty pair list means
/// no constraint; a top-level executable that names no pair is unconstrained.
/// A listed host with zero matching hosted children fails closed (a bare
/// host frame shows no app and manages nothing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeHostChild {
    pub host: String,
    pub child: String,
}

/// True when a top-level window passes the explicit hosted-child fence.
/// `hosted` carries the live different-process child executable paths of the
/// top-level window (unreadable children contribute nothing, never a match).
#[must_use]
pub fn hosted_child_allows(top_exe: &str, hosted: &[String], pairs: &[ScopeHostChild]) -> bool {
    if pairs.is_empty() {
        return true;
    }
    let top = scope_exe_basename(top_exe);
    let mut constrained = false;
    for pair in pairs {
        if scope_exe_basename(&pair.host) != top {
            continue;
        }
        constrained = true;
        if hosted
            .iter()
            .any(|exe| scope_exe_basename(exe) == scope_exe_basename(&pair.child))
        {
            return true;
        }
    }
    !constrained
}

/// Parse one `--scope-host-child HOST=CHILD` value into its normalized pair.
/// Exactly one `=` with non-empty sides; anything else is a refusal.
pub fn parse_scope_host_child(value: &str) -> Result<ScopeHostChild, String> {
    let (host, child) = value
        .split_once('=')
        .ok_or_else(|| "refuse: --scope-host-child needs HOST=CHILD".to_owned())?;
    let host = host.trim().to_owned();
    let child = child.trim().to_owned();
    if host.is_empty() || child.is_empty() || child.contains('=') {
        return Err("refuse: --scope-host-child needs HOST=CHILD".to_owned());
    }
    Ok(ScopeHostChild { host, child })
}

/// Parse one live gap value in `0..=64` (KDE `domain-gap.ts` parity).
pub fn parse_gap_value(value: &str) -> Result<i32, String> {
    let usage = "refuse: gap needs 0..=64";
    let parsed: i32 = value.parse().map_err(|_| usage.to_owned())?;
    if !(0..=64).contains(&parsed) {
        return Err(usage.to_owned());
    }
    Ok(parsed)
}

/// Effective defaults for the normal `tile` command: shipped KDE parity
/// (gaps 8/8, border on with theme accent, underlay on, both takeovers on).
/// Saved settings supply an alternate base via
/// [`crate::settings::tile_options_from_settings`]; explicit CLI switches
/// always override the base they start from.
///
/// Explicit switches stay authoritative for the whole run: the owner records
/// them in [`CliOverrides`] at startup and re-applies them over every live
/// settings-file change, so an unrelated file edit never silently drops a
/// CLI lane.
#[must_use]
pub fn tile_options_defaults() -> TileOptions {
    TileOptions {
        seconds: None,
        trace: false,
        user_start: false,
        no_keyboard_snap_takeover: false,
        allow_win_l: false,
        no_mouse_snap_prevention: false,
        border: crate::active_border::ActiveBorderOptions::default(),
        underlay: crate::group_underlay::GroupUnderlayOptions::default(),
        inner_gap: INNER_GAP,
        outer_gap: OUTER_GAP,
        scope_exes: Vec::new(),
        scope_hosts: Vec::new(),
    }
}

/// Parse `tile` args with compiled-in defaults (no settings file).
pub fn parse_tile_args(args: &[String]) -> Result<TileOptions, String> {
    parse_tile_args_from(args, &tile_options_defaults())
}

/// Explicit per-lane CLI overrides captured at startup. Every `Some` value
/// came from an explicit switch and stays authoritative for the whole run:
/// live settings-file changes apply underneath these lanes (see
/// [`apply_cli_overrides`]). Scope, seconds, trace, and the user-start fence
/// are CLI/run-only and never come from the file, so they carry no override.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CliOverrides {
    pub inner_gap: Option<i32>,
    pub outer_gap: Option<i32>,
    pub no_keyboard_snap_takeover: Option<bool>,
    pub allow_win_l: Option<bool>,
    pub no_mouse_snap_prevention: Option<bool>,
    pub border_enabled: Option<bool>,
    pub border_width: Option<f64>,
    pub border_gap: Option<f64>,
    pub border_radius: Option<f64>,
    pub border_color: Option<(u8, u8, u8)>,
    pub border_use_theme: Option<bool>,
    pub underlay_enabled: Option<bool>,
    pub underlay_color: Option<((u8, u8, u8), u8)>,
    pub underlay_extension: Option<f64>,
}

/// Re-apply explicit startup CLI switches over a settings-file base. Called
/// at startup (equivalent to parsing over the base) and on every live
/// settings apply, so unrelated file edits never drop a CLI lane.
pub fn apply_cli_overrides(base: &mut TileOptions, cli: &CliOverrides) {
    if let Some(value) = cli.inner_gap {
        base.inner_gap = value;
    }
    if let Some(value) = cli.outer_gap {
        base.outer_gap = value;
    }
    if let Some(value) = cli.no_keyboard_snap_takeover {
        base.no_keyboard_snap_takeover = value;
    }
    if let Some(value) = cli.allow_win_l {
        base.allow_win_l = value;
    }
    if let Some(value) = cli.no_mouse_snap_prevention {
        base.no_mouse_snap_prevention = value;
    }
    if let Some(value) = cli.border_enabled {
        base.border.enabled = value;
    }
    if let Some(value) = cli.border_width {
        base.border.style.width = value;
    }
    if let Some(value) = cli.border_gap {
        base.border.style.gap = value;
    }
    if let Some(value) = cli.border_radius {
        base.border.style.radius = value;
    }
    if let Some(value) = cli.border_color {
        base.border.style.color = value;
    }
    if let Some(value) = cli.border_use_theme {
        base.border.style.use_theme = value;
    }
    if let Some(value) = cli.underlay_enabled {
        base.underlay.enabled = value;
    }
    if let Some((color, alpha)) = cli.underlay_color {
        base.underlay.style.color = color;
        base.underlay.style.alpha = alpha;
    }
    if let Some(value) = cli.underlay_extension {
        base.underlay.style.extension = value;
    }
}

/// Parse `tile --user-start [...]` starting from a caller-supplied base
/// (normally the persisted settings, so saved values are the defaults and
/// every explicit switch overrides its corresponding value). All existing
/// switches are preserved; `--inner-gap N` and `--outer-gap N` (0..64) are
/// new. `--user-start` always starts false: saved settings can never satisfy
/// the explicit user-start fence, so agents never run this path by accident.
pub fn parse_tile_args_from(
    args: &[String],
    defaults: &TileOptions,
) -> Result<TileOptions, String> {
    parse_tile_args_from_with_overrides(args, defaults).map(|(options, _)| options)
}

/// Parse `tile` args plus the explicit per-lane [`CliOverrides`] (see
/// [`apply_cli_overrides`]): same grammar as [`parse_tile_args_from`], but
/// every explicit switch is also recorded so it stays authoritative across
/// live settings-file changes.
pub fn parse_tile_args_from_with_overrides(
    args: &[String],
    defaults: &TileOptions,
) -> Result<(TileOptions, CliOverrides), String> {
    let usage = "usage: tile --user-start [--seconds N] [--trace] [--no-keyboard-snap-takeover] [--allow-win-l] [--no-mouse-snap-prevention] [--inner-gap 0..=64] [--outer-gap 0..=64] [--scope-exe NAME ...] [--scope-host-child HOST=CHILD ...] [--no-active-border] [--active-border-width 0..=32] [--active-border-gap 0..=64] [--active-border-radius 0..=64] [--active-border-color #rrggbb] [--active-border-theme|--no-active-border-theme] [--no-group-underlay] [--group-underlay-color #aarrggbb] [--group-underlay-extension -1..=32]";
    let mut seconds: Option<u64> = defaults.seconds;
    let mut trace = defaults.trace;
    let mut user_start = false;
    let mut no_keyboard_snap_takeover = defaults.no_keyboard_snap_takeover;
    let mut allow_win_l = defaults.allow_win_l;
    let mut no_mouse_snap_prevention = defaults.no_mouse_snap_prevention;
    let mut scope_exes: Vec<String> = defaults.scope_exes.clone();
    let mut scope_hosts: Vec<ScopeHostChild> = defaults.scope_hosts.clone();
    let mut border = defaults.border;
    let mut underlay = defaults.underlay;
    let mut inner_gap = defaults.inner_gap;
    let mut outer_gap = defaults.outer_gap;
    let mut theme_flags = 0u8;
    let mut cli = CliOverrides::default();
    let mut i = 0;
    while i < args.len() {
        if apply_underlay_arg_tracked(args, &mut i, &mut underlay, &mut cli, usage)? {
            continue;
        }
        match args[i].as_str() {
            "--trace" => {
                trace = true;
                i += 1;
            }
            "--user-start" => {
                user_start = true;
                i += 1;
            }
            "--no-keyboard-snap-takeover" => {
                no_keyboard_snap_takeover = true;
                cli.no_keyboard_snap_takeover = Some(true);
                i += 1;
            }
            "--allow-win-l" => {
                allow_win_l = true;
                cli.allow_win_l = Some(true);
                i += 1;
            }
            "--no-mouse-snap-prevention" => {
                no_mouse_snap_prevention = true;
                cli.no_mouse_snap_prevention = Some(true);
                i += 1;
            }
            "--inner-gap" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                inner_gap = parse_gap_value(value)?;
                cli.inner_gap = Some(inner_gap);
                i += 1;
            }
            "--outer-gap" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                outer_gap = parse_gap_value(value)?;
                cli.outer_gap = Some(outer_gap);
                i += 1;
            }
            "--no-active-border" => {
                border.enabled = false;
                cli.border_enabled = Some(false);
                i += 1;
            }
            "--active-border-width" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.width = crate::active_border::parse_width(value)?;
                cli.border_width = Some(border.style.width);
                i += 1;
            }
            "--active-border-gap" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.gap = crate::active_border::parse_gap(value)?;
                cli.border_gap = Some(border.style.gap);
                i += 1;
            }
            "--active-border-radius" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.radius = crate::active_border::parse_radius(value)?;
                cli.border_radius = Some(border.style.radius);
                i += 1;
            }
            "--active-border-color" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.color = crate::active_border::parse_color(value)?;
                cli.border_color = Some(border.style.color);
                i += 1;
            }
            "--no-active-border-theme" => {
                if theme_flags & 0x02 != 0 {
                    return Err(usage.to_owned());
                }
                theme_flags |= 0x01;
                border.style.use_theme = false;
                cli.border_use_theme = Some(false);
                i += 1;
            }
            "--active-border-theme" => {
                if theme_flags & 0x01 != 0 {
                    return Err(usage.to_owned());
                }
                theme_flags |= 0x02;
                border.style.use_theme = true;
                cli.border_use_theme = Some(true);
                i += 1;
            }
            "--scope-exe" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                if value.trim().is_empty() {
                    return Err("refuse: empty --scope-exe".to_owned());
                }
                let normalized = value.trim().to_owned();
                if !scope_exes
                    .iter()
                    .any(|entry| scope_exe_basename(entry) == scope_exe_basename(&normalized))
                {
                    scope_exes.push(normalized);
                }
                i += 1;
            }
            "--scope-host-child" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                let pair = parse_scope_host_child(value.trim())?;
                if !scope_hosts.iter().any(|entry| {
                    scope_exe_basename(&entry.host) == scope_exe_basename(&pair.host)
                        && scope_exe_basename(&entry.child) == scope_exe_basename(&pair.child)
                }) {
                    scope_hosts.push(pair);
                }
                i += 1;
            }
            "--seconds" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                let parsed: u64 = value.parse().map_err(|_| usage.to_owned())?;
                if parsed == 0 || parsed > crate::lifecycle::MAX_RUN_SECONDS {
                    return Err(format!(
                        "refuse: seconds must be 1..={}",
                        crate::lifecycle::MAX_RUN_SECONDS
                    ));
                }
                seconds = Some(parsed);
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    if !user_start {
        return Err("refuse: tile requires explicit --user-start".to_owned());
    }
    Ok((
        TileOptions {
            seconds,
            trace,
            user_start,
            no_keyboard_snap_takeover,
            allow_win_l,
            no_mouse_snap_prevention,
            border,
            underlay,
            inner_gap,
            outer_gap,
            scope_exes,
            scope_hosts,
        },
        cli,
    ))
}

/// CLI options for the proof-only `tile-proof` command (owned helpers only).
/// `--allowlist PATH` is required and nonempty; missing/empty/malformed input
/// refuses before any lease or write and never falls back to normal mode.
#[derive(Debug, Clone, PartialEq)]
pub struct TileProofOptions {
    pub seconds: Option<u64>,
    pub trace: bool,
    pub allowlist: PathBuf,
    pub border: crate::active_border::ActiveBorderOptions,
    pub underlay: crate::group_underlay::GroupUnderlayOptions,
}

/// Parse `tile-proof --allowlist PATH [--seconds N] [--trace]` plus the shared
/// active-border flags (default on with `--no-active-border`) and the shared
/// group-underlay flags (default on with `--no-group-underlay`).
pub fn parse_tile_proof_args(args: &[String]) -> Result<TileProofOptions, String> {
    let usage = "usage: tile-proof --allowlist PATH [--seconds N] [--trace] [--no-active-border] [--active-border-width 0..=32] [--active-border-gap 0..=64] [--active-border-radius 0..=64] [--active-border-color #rrggbb] [--active-border-theme|--no-active-border-theme] [--no-group-underlay] [--group-underlay-color #aarrggbb] [--group-underlay-extension -1..=32]";
    let mut seconds: Option<u64> = None;
    let mut trace = false;
    let mut allowlist: Option<PathBuf> = None;
    let mut border = crate::active_border::ActiveBorderOptions::default();
    let mut underlay = crate::group_underlay::GroupUnderlayOptions::default();
    let mut theme_flags = 0u8;
    let mut i = 0;
    while i < args.len() {
        if apply_underlay_arg(args, &mut i, &mut underlay, usage)? {
            continue;
        }
        match args[i].as_str() {
            "--trace" => {
                trace = true;
                i += 1;
            }
            "--no-active-border" => {
                border.enabled = false;
                i += 1;
            }
            "--active-border-width" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.width = crate::active_border::parse_width(value)?;
                i += 1;
            }
            "--active-border-gap" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.gap = crate::active_border::parse_gap(value)?;
                i += 1;
            }
            "--active-border-radius" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.radius = crate::active_border::parse_radius(value)?;
                i += 1;
            }
            "--active-border-color" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.color = crate::active_border::parse_color(value)?;
                i += 1;
            }
            "--no-active-border-theme" => {
                if theme_flags & 0x02 != 0 {
                    return Err(usage.to_owned());
                }
                theme_flags |= 0x01;
                border.style.use_theme = false;
                i += 1;
            }
            "--active-border-theme" => {
                if theme_flags & 0x01 != 0 {
                    return Err(usage.to_owned());
                }
                theme_flags |= 0x02;
                border.style.use_theme = true;
                i += 1;
            }
            "--seconds" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                let parsed: u64 = value.parse().map_err(|_| usage.to_owned())?;
                if parsed == 0 || parsed > crate::lifecycle::MAX_RUN_SECONDS {
                    return Err(format!(
                        "refuse: seconds must be 1..={}",
                        crate::lifecycle::MAX_RUN_SECONDS
                    ));
                }
                seconds = Some(parsed);
                i += 1;
            }
            "--allowlist" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                if value.trim().is_empty() {
                    return Err("refuse: empty allowlist".to_owned());
                }
                allowlist = Some(PathBuf::from(value));
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    let Some(allowlist) = allowlist else {
        return Err("refuse: tile-proof requires --allowlist".to_owned());
    };
    Ok(TileProofOptions {
        seconds,
        trace,
        allowlist,
        border,
        underlay,
    })
}

/// CLI options for the proof-only `hide-proof` command (owned helpers only,
/// product nonce mechanism). Requires a nonempty valid `--allowlist`; the
/// owner verifies every frozen entry as an owned helper BEFORE touching the
/// ordinary product nonce APIs, and never falls back to normal mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HideProofOptions {
    pub seconds: Option<u64>,
    pub trace: bool,
    pub allowlist: PathBuf,
}

/// Parse `hide-proof --allowlist PATH [--seconds N] [--trace]`.
pub fn parse_hide_proof_args(args: &[String]) -> Result<HideProofOptions, String> {
    let usage = "usage: hide-proof --allowlist PATH [--seconds N] [--trace]";
    let mut seconds: Option<u64> = None;
    let mut trace = false;
    let mut allowlist: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--trace" => {
                trace = true;
                i += 1;
            }
            "--seconds" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                let parsed: u64 = value.parse().map_err(|_| usage.to_owned())?;
                if parsed == 0 || parsed > crate::lifecycle::MAX_RUN_SECONDS {
                    return Err(format!(
                        "refuse: seconds must be 1..={}",
                        crate::lifecycle::MAX_RUN_SECONDS
                    ));
                }
                seconds = Some(parsed);
                i += 1;
            }
            "--allowlist" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                if value.trim().is_empty() {
                    return Err("refuse: empty allowlist".to_owned());
                }
                allowlist = Some(PathBuf::from(value));
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    let Some(allowlist) = allowlist else {
        return Err("refuse: hide-proof requires --allowlist".to_owned());
    };
    Ok(HideProofOptions {
        seconds,
        trace,
        allowlist,
    })
}

/// Verify the raw received argv against the parsed `hide-proof` options.
/// Every delivery flag must be evidenced; any inconsistency refuses before
/// any lease or write, never falls back.
pub fn verify_hide_proof_argv_consistency(
    raw: &[String],
    parsed: &HideProofOptions,
) -> Result<(), String> {
    let usage = "usage: hide-proof --allowlist PATH [--seconds N] [--trace]";
    let mut allowlist: Option<&str> = None;
    let mut seconds: Option<&str> = None;
    let mut trace = false;
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--trace" => {
                trace = true;
                i += 1;
            }
            "--seconds" => {
                i += 1;
                seconds = Some(raw.get(i).ok_or_else(|| usage.to_owned())?.as_str());
                i += 1;
            }
            "--allowlist" => {
                i += 1;
                allowlist = Some(raw.get(i).ok_or_else(|| usage.to_owned())?.as_str());
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    let Some(allowlist) = allowlist else {
        return Err("refuse: hide-proof requires --allowlist".to_owned());
    };
    if parsed.allowlist.as_path() != std::path::Path::new(allowlist) {
        return Err("error: argv/parsed allowlist mismatch (impossible)".to_owned());
    }
    if parsed.trace != trace {
        return Err("error: argv/parsed trace mismatch (impossible)".to_owned());
    }
    match (parsed.seconds, seconds) {
        (None, None) => {}
        (Some(want), Some(got)) => {
            let got: u64 = got.parse().map_err(|_| usage.to_owned())?;
            if want != got {
                return Err("error: argv/parsed seconds mismatch (impossible)".to_owned());
            }
        }
        _ => return Err("error: argv/parsed seconds mismatch (impossible)".to_owned()),
    }
    Ok(())
}

/// CLI options for the proof-only `shortcut-proof` command (owned helpers
/// only, automated synthetic-input verification). Requires a nonempty valid
/// `--allowlist` at parse and at native start; missing/empty/malformed input
/// refuses before any lease or write and never falls back to normal mode.
///
/// Differences from `tile-proof`: the owner installs the same low-level hook
/// with test-only acceptance of exactly
/// [`crate::snapkey::SHORTCUT_PROOF_MARKER`] in `dwExtraInfo` (product `tile`
/// keeps filtering ALL injected), and keyboard takeover stays ON so marked
/// Win+H/J/K/L/arrows (Shift variants move) dispatch through the retained
/// Engine with fresh observation plus native readback. Unshifted Win+L is
/// NOT offered here (always gated off): live runs must never send Win+L, and
/// the gating is asserted by portable tests. Mouse-Snap prevention defaults
/// ON like product (visible `--no-mouse-snap-prevention` off switch) so the
/// same session-only preimage/restore routines run under the proof geometry
/// gate.
#[derive(Debug, Clone, PartialEq)]
pub struct ShortcutProofOptions {
    pub seconds: Option<u64>,
    pub trace: bool,
    pub allowlist: PathBuf,
    pub no_mouse_snap_prevention: bool,
    pub border: crate::active_border::ActiveBorderOptions,
    pub underlay: crate::group_underlay::GroupUnderlayOptions,
}

/// Parse `shortcut-proof --allowlist PATH [--seconds N] [--trace]
/// [--no-mouse-snap-prevention]` plus the shared active-border flags and the
/// shared group-underlay flags.
/// Any keyboard flag (`--allow-win-l`,
/// `--no-keyboard-snap-takeover`), `--user-start`, or unknown flag is a
/// refusal, never a silent normal run.
pub fn parse_shortcut_proof_args(args: &[String]) -> Result<ShortcutProofOptions, String> {
    let usage = "usage: shortcut-proof --allowlist PATH [--seconds N] [--trace] [--no-mouse-snap-prevention] [--no-active-border] [--active-border-width 0..=32] [--active-border-gap 0..=64] [--active-border-radius 0..=64] [--active-border-color #rrggbb] [--active-border-theme|--no-active-border-theme] [--no-group-underlay] [--group-underlay-color #aarrggbb] [--group-underlay-extension -1..=32]";
    let mut seconds: Option<u64> = None;
    let mut trace = false;
    let mut allowlist: Option<PathBuf> = None;
    let mut no_mouse_snap_prevention = false;
    let mut border = crate::active_border::ActiveBorderOptions::default();
    let mut underlay = crate::group_underlay::GroupUnderlayOptions::default();
    let mut theme_flags = 0u8;
    let mut i = 0;
    while i < args.len() {
        if apply_underlay_arg(args, &mut i, &mut underlay, usage)? {
            continue;
        }
        match args[i].as_str() {
            "--trace" => {
                trace = true;
                i += 1;
            }
            "--no-mouse-snap-prevention" => {
                no_mouse_snap_prevention = true;
                i += 1;
            }
            "--no-active-border" => {
                border.enabled = false;
                i += 1;
            }
            "--active-border-width" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.width = crate::active_border::parse_width(value)?;
                i += 1;
            }
            "--active-border-gap" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.gap = crate::active_border::parse_gap(value)?;
                i += 1;
            }
            "--active-border-radius" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.radius = crate::active_border::parse_radius(value)?;
                i += 1;
            }
            "--active-border-color" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.color = crate::active_border::parse_color(value)?;
                i += 1;
            }
            "--no-active-border-theme" => {
                if theme_flags & 0x02 != 0 {
                    return Err(usage.to_owned());
                }
                theme_flags |= 0x01;
                border.style.use_theme = false;
                i += 1;
            }
            "--active-border-theme" => {
                if theme_flags & 0x01 != 0 {
                    return Err(usage.to_owned());
                }
                theme_flags |= 0x02;
                border.style.use_theme = true;
                i += 1;
            }
            "--seconds" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                let parsed: u64 = value.parse().map_err(|_| usage.to_owned())?;
                if parsed == 0 || parsed > crate::lifecycle::MAX_RUN_SECONDS {
                    return Err(format!(
                        "refuse: seconds must be 1..={}",
                        crate::lifecycle::MAX_RUN_SECONDS
                    ));
                }
                seconds = Some(parsed);
                i += 1;
            }
            "--allowlist" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                if value.trim().is_empty() {
                    return Err("refuse: empty allowlist".to_owned());
                }
                allowlist = Some(PathBuf::from(value));
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    let Some(allowlist) = allowlist else {
        return Err("refuse: shortcut-proof requires --allowlist".to_owned());
    };
    Ok(ShortcutProofOptions {
        seconds,
        trace,
        allowlist,
        no_mouse_snap_prevention,
        border,
        underlay,
    })
}

/// Verify the raw received argv against the parsed `shortcut-proof` options.
/// Same shape as the `tile-proof` check plus the mouse-prevention flag: every
/// delivery flag must be evidenced with argv elements as separate strings.
pub fn verify_shortcut_proof_argv_consistency(
    raw: &[String],
    parsed: &ShortcutProofOptions,
) -> Result<(), String> {
    let usage = "usage: shortcut-proof --allowlist PATH [--seconds N] [--trace] [--no-mouse-snap-prevention] [--no-active-border] [--active-border-width 0..=32] [--active-border-gap 0..=64] [--active-border-radius 0..=64] [--active-border-color #rrggbb] [--active-border-theme|--no-active-border-theme] [--no-group-underlay] [--group-underlay-color #aarrggbb] [--group-underlay-extension -1..=32]";
    let (border_echo, rest) = scan_border_echo(raw, usage)?;
    verify_border_echo(&border_echo, &parsed.border, usage)?;
    let (underlay_echo, rest) = scan_underlay_echo(&rest, usage)?;
    verify_underlay_echo(&underlay_echo, &parsed.underlay, usage)?;
    let raw = rest;
    let mut allowlist: Option<&str> = None;
    let mut seconds: Option<&str> = None;
    let mut trace = false;
    let mut no_mouse = false;
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--trace" => {
                trace = true;
                i += 1;
            }
            "--no-mouse-snap-prevention" => {
                no_mouse = true;
                i += 1;
            }
            "--seconds" => {
                i += 1;
                seconds = Some(raw.get(i).ok_or_else(|| usage.to_owned())?.as_str());
                i += 1;
            }
            "--allowlist" => {
                i += 1;
                allowlist = Some(raw.get(i).ok_or_else(|| usage.to_owned())?.as_str());
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    let Some(allowlist) = allowlist else {
        return Err("refuse: shortcut-proof requires --allowlist".to_owned());
    };
    if parsed.allowlist.as_path() != std::path::Path::new(allowlist) {
        return Err("error: argv/parsed allowlist mismatch (impossible)".to_owned());
    }
    if parsed.trace != trace {
        return Err("error: argv/parsed trace mismatch (impossible)".to_owned());
    }
    if parsed.no_mouse_snap_prevention != no_mouse {
        return Err("error: argv/parsed mouse mismatch (impossible)".to_owned());
    }
    match (parsed.seconds, seconds) {
        (None, None) => {}
        (Some(want), Some(got)) => {
            let got: u64 = got.parse().map_err(|_| usage.to_owned())?;
            if want != got {
                return Err("error: argv/parsed seconds mismatch (impossible)".to_owned());
            }
        }
        _ => return Err("error: argv/parsed seconds mismatch (impossible)".to_owned()),
    }
    Ok(())
}
/// Verify the raw received argv against the parsed `tile-proof` options.
/// Every delivery flag must be evidenced: `--allowlist` present with the exact
/// path (argv elements stay separate strings so spaces survive without
/// quoting), `--trace` presence matching, `--seconds` value matching, and no
/// unknown flags. Any parsed/raw inconsistency is an impossible error: refuse
/// before any lease or write, never fall back.
/// Verify a raw-scanned border echo against parsed border options.
fn verify_border_echo(
    echo: &crate::active_border::BorderArgvEcho,
    border: &crate::active_border::ActiveBorderOptions,
    usage: &str,
) -> Result<(), String> {
    let impossible = |what: &str| format!("error: argv/parsed {what} mismatch (impossible)");
    if echo.no_active_border != !border.enabled {
        return Err(impossible("border-enabled"));
    }
    let check_value = |raw: &Option<String>, want: f64, what: &str| -> Result<(), String> {
        match raw {
            None => Ok(()),
            Some(text) => {
                let got: f64 = text.parse().map_err(|_| usage.to_owned())?;
                if (got - want).abs() > f64::EPSILON {
                    return Err(impossible(what));
                }
                Ok(())
            }
        }
    };
    // Presence must round-trip too: a parsed non-default with no raw flag,
    // or a raw flag collapsing to the default, is still consistent as long
    // as values match; only value mismatches refuse here.
    let defaults = crate::active_border::ActiveBorderOptions::default();
    check_value(&echo.width, border.style.width, "border-width")?;
    check_value(&echo.gap, border.style.gap, "border-gap")?;
    check_value(&echo.radius, border.style.radius, "border-radius")?;
    match (&echo.color, border.style.color) {
        (None, _) => {}
        (Some(text), want) => {
            let got = crate::active_border::parse_color(text).map_err(|_| usage.to_owned())?;
            if got != want {
                return Err(impossible("border-color"));
            }
        }
    }
    if echo.no_theme && echo.theme {
        return Err(impossible("border-theme"));
    }
    if echo.theme && !border.style.use_theme {
        return Err(impossible("border-theme"));
    }
    if echo.no_theme && border.style.use_theme {
        return Err(impossible("border-theme"));
    }
    if !echo.theme && !echo.no_theme && border.style.use_theme != defaults.style.use_theme {
        return Err(impossible("border-theme"));
    }
    // A raw flag that parses to the default is harmless; a missing flag with
    // a non-default parsed value is impossible.
    if echo.width.is_none() && border.style.width != defaults.style.width {
        return Err(impossible("border-width"));
    }
    if echo.gap.is_none() && border.style.gap != defaults.style.gap {
        return Err(impossible("border-gap"));
    }
    if echo.radius.is_none() && border.style.radius != defaults.style.radius {
        return Err(impossible("border-radius"));
    }
    if echo.color.is_none() && border.style.color != defaults.style.color {
        return Err(impossible("border-color"));
    }
    Ok(())
}

/// Apply one shared group-underlay flag (`--no-group-underlay`,
/// `--group-underlay-color #aarrggbb`, `--group-underlay-extension -1..=32`).
/// Returns true when `args[*i]` was an underlay flag (index advanced past its
/// value); false leaves every other flag for the caller. Called at the top of
/// each loop parser so all four commands share one flag shape.
fn apply_underlay_arg(
    args: &[String],
    i: &mut usize,
    underlay: &mut crate::group_underlay::GroupUnderlayOptions,
    usage: &str,
) -> Result<bool, String> {
    match args[*i].as_str() {
        "--no-group-underlay" => {
            underlay.enabled = false;
            *i += 1;
            Ok(true)
        }
        "--group-underlay-color" => {
            *i += 1;
            let value = args.get(*i).ok_or_else(|| usage.to_owned())?;
            let (color, alpha) = crate::group_underlay::parse_color_argb(value)?;
            underlay.style.color = color;
            underlay.style.alpha = alpha;
            *i += 1;
            Ok(true)
        }
        "--group-underlay-extension" => {
            *i += 1;
            let value = args.get(*i).ok_or_else(|| usage.to_owned())?;
            underlay.style.extension = crate::group_underlay::parse_extension(value)?;
            *i += 1;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Tracked underlay-arg scan for the normal `tile` parser: same grammar as
/// [`apply_underlay_arg`] (used by the proof parsers), but every explicit
/// switch is also recorded in [`CliOverrides`] so it stays authoritative
/// across live settings-file changes.
fn apply_underlay_arg_tracked(
    args: &[String],
    i: &mut usize,
    underlay: &mut crate::group_underlay::GroupUnderlayOptions,
    cli: &mut CliOverrides,
    usage: &str,
) -> Result<bool, String> {
    match args[*i].as_str() {
        "--no-group-underlay" => {
            underlay.enabled = false;
            cli.underlay_enabled = Some(false);
            *i += 1;
            Ok(true)
        }
        "--group-underlay-color" => {
            *i += 1;
            let value = args.get(*i).ok_or_else(|| usage.to_owned())?;
            let (color, alpha) = crate::group_underlay::parse_color_argb(value)?;
            underlay.style.color = color;
            underlay.style.alpha = alpha;
            cli.underlay_color = Some((color, alpha));
            *i += 1;
            Ok(true)
        }
        "--group-underlay-extension" => {
            *i += 1;
            let value = args.get(*i).ok_or_else(|| usage.to_owned())?;
            underlay.style.extension = crate::group_underlay::parse_extension(value)?;
            cli.underlay_extension = Some(underlay.style.extension);
            *i += 1;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Scan raw argv border flags into an echo. Unknown non-border flags are left
/// for the caller: this returns the echo plus the non-border remainder.
fn scan_border_echo(
    raw: &[String],
    usage: &str,
) -> Result<(crate::active_border::BorderArgvEcho, Vec<String>), String> {
    let mut echo = crate::active_border::BorderArgvEcho::default();
    let mut rest: Vec<String> = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--no-active-border" => {
                echo.no_active_border = true;
                i += 1;
            }
            "--active-border-width"
            | "--active-border-gap"
            | "--active-border-radius"
            | "--active-border-color" => {
                let flag = raw[i].clone();
                i += 1;
                let value = raw.get(i).ok_or_else(|| usage.to_owned())?.clone();
                match flag.as_str() {
                    "--active-border-width" => echo.width = Some(value),
                    "--active-border-gap" => echo.gap = Some(value),
                    "--active-border-radius" => echo.radius = Some(value),
                    _ => echo.color = Some(value),
                }
                i += 1;
            }
            "--no-active-border-theme" => {
                echo.no_theme = true;
                i += 1;
            }
            "--active-border-theme" => {
                echo.theme = true;
                i += 1;
            }
            _ => {
                rest.push(raw[i].clone());
                i += 1;
            }
        }
    }
    Ok((echo, rest))
}

/// Scan raw argv underlay flags into an echo, mirroring `scan_border_echo`.
fn scan_underlay_echo(
    raw: &[String],
    usage: &str,
) -> Result<(crate::group_underlay::UnderlayArgvEcho, Vec<String>), String> {
    let mut echo = crate::group_underlay::UnderlayArgvEcho::default();
    let mut rest: Vec<String> = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--no-group-underlay" => {
                echo.no_group_underlay = true;
                i += 1;
            }
            "--group-underlay-color" | "--group-underlay-extension" => {
                let flag = raw[i].clone();
                i += 1;
                let value = raw.get(i).ok_or_else(|| usage.to_owned())?.clone();
                if flag == "--group-underlay-color" {
                    echo.color = Some(value);
                } else {
                    echo.extension = Some(value);
                }
                i += 1;
            }
            _ => {
                rest.push(raw[i].clone());
                i += 1;
            }
        }
    }
    Ok((echo, rest))
}

/// Verify a raw-scanned underlay echo against parsed underlay options.
fn verify_underlay_echo(
    echo: &crate::group_underlay::UnderlayArgvEcho,
    underlay: &crate::group_underlay::GroupUnderlayOptions,
    usage: &str,
) -> Result<(), String> {
    let impossible = |what: &str| format!("error: argv/parsed {what} mismatch (impossible)");
    if echo.no_group_underlay != !underlay.enabled {
        return Err(impossible("underlay-enabled"));
    }
    match (&echo.color, underlay.style.color, underlay.style.alpha) {
        (None, _, _) => {}
        (Some(text), want_rgb, want_alpha) => {
            let (got_rgb, got_alpha) =
                crate::group_underlay::parse_color_argb(text).map_err(|_| usage.to_owned())?;
            if got_rgb != want_rgb || got_alpha != want_alpha {
                return Err(impossible("underlay-color"));
            }
        }
    }
    match &echo.extension {
        None => {}
        Some(text) => {
            let got: f64 = text.parse().map_err(|_| usage.to_owned())?;
            if (got - underlay.style.extension).abs() > f64::EPSILON {
                return Err(impossible("underlay-extension"));
            }
        }
    }
    let defaults = crate::group_underlay::GroupUnderlayOptions::default();
    if echo.color.is_none()
        && (underlay.style.color != defaults.style.color
            || underlay.style.alpha != defaults.style.alpha)
    {
        return Err(impossible("underlay-color"));
    }
    if echo.extension.is_none() && underlay.style.extension != defaults.style.extension {
        return Err(impossible("underlay-extension"));
    }
    Ok(())
}

pub fn verify_proof_argv_consistency(
    raw: &[String],
    parsed: &TileProofOptions,
) -> Result<(), String> {
    let usage = "usage: tile-proof --allowlist PATH [--seconds N] [--trace] [--no-active-border] [--active-border-width 0..=32] [--active-border-gap 0..=64] [--active-border-radius 0..=64] [--active-border-color #rrggbb] [--active-border-theme|--no-active-border-theme] [--no-group-underlay] [--group-underlay-color #aarrggbb] [--group-underlay-extension -1..=32]";
    let (border_echo, rest) = scan_border_echo(raw, usage)?;
    verify_border_echo(&border_echo, &parsed.border, usage)?;
    let (underlay_echo, rest) = scan_underlay_echo(&rest, usage)?;
    verify_underlay_echo(&underlay_echo, &parsed.underlay, usage)?;
    let raw = rest;
    let mut allowlist: Option<&str> = None;
    let mut seconds: Option<&str> = None;
    let mut trace = false;
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--trace" => {
                trace = true;
                i += 1;
            }
            "--seconds" => {
                i += 1;
                seconds = Some(raw.get(i).ok_or_else(|| usage.to_owned())?.as_str());
                i += 1;
            }
            "--allowlist" => {
                i += 1;
                allowlist = Some(raw.get(i).ok_or_else(|| usage.to_owned())?.as_str());
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    let Some(allowlist) = allowlist else {
        return Err("refuse: tile-proof requires --allowlist".to_owned());
    };
    if parsed.allowlist.as_path() != std::path::Path::new(allowlist) {
        return Err("error: argv/parsed allowlist mismatch (impossible)".to_owned());
    }
    if parsed.trace != trace {
        return Err("error: argv/parsed trace mismatch (impossible)".to_owned());
    }
    match (parsed.seconds, seconds) {
        (None, None) => {}
        (Some(want), Some(got)) => {
            let got: u64 = got.parse().map_err(|_| usage.to_owned())?;
            if want != got {
                return Err("error: argv/parsed seconds mismatch (impossible)".to_owned());
            }
        }
        _ => return Err("error: argv/parsed seconds mismatch (impossible)".to_owned()),
    }
    Ok(())
}

/// CLI options for the proof-only `workspace-proof` command (owned helpers
/// only, automated synthetic-input verification for workspace digits).
/// Same frozen-allowlist geometry gate as `shortcut-proof` (never falls back
/// to normal), but the owner additionally drives the workspace dispatcher
/// (select/send/follow with hide/reveal) for exactly the allowlisted helpers:
/// every hide verifies `verify_proof_owned` fresh before the write, and the
/// hook accepts exactly [`crate::snapkey::SHORTCUT_PROOF_MARKER`].
/// `shortcut-proof` never hides for workspace intents; `tile-proof` installs
/// no hook at all.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceProofOptions {
    pub seconds: Option<u64>,
    pub trace: bool,
    pub allowlist: PathBuf,
    pub no_mouse_snap_prevention: bool,
    pub border: crate::active_border::ActiveBorderOptions,
    pub underlay: crate::group_underlay::GroupUnderlayOptions,
}

/// Parse `workspace-proof --allowlist PATH [--seconds N] [--trace]
/// [--no-mouse-snap-prevention]` plus the shared active-border flags and the
/// shared group-underlay flags.
/// Any keyboard flag, `--user-start`, or
/// unknown flag is a refusal, never a silent normal run.
pub fn parse_workspace_proof_args(args: &[String]) -> Result<WorkspaceProofOptions, String> {
    let usage = "usage: workspace-proof --allowlist PATH [--seconds N] [--trace] [--no-mouse-snap-prevention] [--no-active-border] [--active-border-width 0..=32] [--active-border-gap 0..=64] [--active-border-radius 0..=64] [--active-border-color #rrggbb] [--active-border-theme|--no-active-border-theme] [--no-group-underlay] [--group-underlay-color #aarrggbb] [--group-underlay-extension -1..=32]";
    let mut seconds: Option<u64> = None;
    let mut trace = false;
    let mut allowlist: Option<PathBuf> = None;
    let mut no_mouse_snap_prevention = false;
    let mut border = crate::active_border::ActiveBorderOptions::default();
    let mut underlay = crate::group_underlay::GroupUnderlayOptions::default();
    let mut theme_flags = 0u8;
    let mut i = 0;
    while i < args.len() {
        if apply_underlay_arg(args, &mut i, &mut underlay, usage)? {
            continue;
        }
        match args[i].as_str() {
            "--trace" => {
                trace = true;
                i += 1;
            }
            "--no-mouse-snap-prevention" => {
                no_mouse_snap_prevention = true;
                i += 1;
            }
            "--no-active-border" => {
                border.enabled = false;
                i += 1;
            }
            "--active-border-width" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.width = crate::active_border::parse_width(value)?;
                i += 1;
            }
            "--active-border-gap" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.gap = crate::active_border::parse_gap(value)?;
                i += 1;
            }
            "--active-border-radius" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.radius = crate::active_border::parse_radius(value)?;
                i += 1;
            }
            "--active-border-color" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                border.style.color = crate::active_border::parse_color(value)?;
                i += 1;
            }
            "--no-active-border-theme" => {
                if theme_flags & 0x02 != 0 {
                    return Err(usage.to_owned());
                }
                theme_flags |= 0x01;
                border.style.use_theme = false;
                i += 1;
            }
            "--active-border-theme" => {
                if theme_flags & 0x01 != 0 {
                    return Err(usage.to_owned());
                }
                theme_flags |= 0x02;
                border.style.use_theme = true;
                i += 1;
            }
            "--seconds" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                let parsed: u64 = value.parse().map_err(|_| usage.to_owned())?;
                if parsed == 0 || parsed > crate::lifecycle::MAX_RUN_SECONDS {
                    return Err(format!(
                        "refuse: seconds must be 1..={}",
                        crate::lifecycle::MAX_RUN_SECONDS
                    ));
                }
                seconds = Some(parsed);
                i += 1;
            }
            "--allowlist" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| usage.to_owned())?;
                if value.trim().is_empty() {
                    return Err("refuse: empty allowlist".to_owned());
                }
                allowlist = Some(PathBuf::from(value));
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    let Some(allowlist) = allowlist else {
        return Err("refuse: workspace-proof requires --allowlist".to_owned());
    };
    Ok(WorkspaceProofOptions {
        seconds,
        trace,
        allowlist,
        no_mouse_snap_prevention,
        border,
        underlay,
    })
}

/// Verify the raw received argv against the parsed `workspace-proof` options.
pub fn verify_workspace_proof_argv_consistency(
    raw: &[String],
    parsed: &WorkspaceProofOptions,
) -> Result<(), String> {
    let usage = "usage: workspace-proof --allowlist PATH [--seconds N] [--trace] [--no-mouse-snap-prevention] [--no-active-border] [--active-border-width 0..=32] [--active-border-gap 0..=64] [--active-border-radius 0..=64] [--active-border-color #rrggbb] [--active-border-theme|--no-active-border-theme] [--no-group-underlay] [--group-underlay-color #aarrggbb] [--group-underlay-extension -1..=32]";
    let (border_echo, rest) = scan_border_echo(raw, usage)?;
    verify_border_echo(&border_echo, &parsed.border, usage)?;
    let (underlay_echo, rest) = scan_underlay_echo(&rest, usage)?;
    verify_underlay_echo(&underlay_echo, &parsed.underlay, usage)?;
    let raw = rest;
    let mut allowlist: Option<&str> = None;
    let mut seconds: Option<&str> = None;
    let mut trace = false;
    let mut no_mouse = false;
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--trace" => {
                trace = true;
                i += 1;
            }
            "--no-mouse-snap-prevention" => {
                no_mouse = true;
                i += 1;
            }
            "--seconds" => {
                i += 1;
                seconds = Some(raw.get(i).ok_or_else(|| usage.to_owned())?.as_str());
                i += 1;
            }
            "--allowlist" => {
                i += 1;
                allowlist = Some(raw.get(i).ok_or_else(|| usage.to_owned())?.as_str());
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    let Some(allowlist) = allowlist else {
        return Err("refuse: workspace-proof requires --allowlist".to_owned());
    };
    if parsed.allowlist.as_path() != std::path::Path::new(allowlist) {
        return Err("error: argv/parsed allowlist mismatch (impossible)".to_owned());
    }
    if parsed.trace != trace {
        return Err("error: argv/parsed trace mismatch (impossible)".to_owned());
    }
    if parsed.no_mouse_snap_prevention != no_mouse {
        return Err("error: argv/parsed mouse mismatch (impossible)".to_owned());
    }
    match (parsed.seconds, seconds) {
        (None, None) => {}
        (Some(want), Some(got)) => {
            let got: u64 = got.parse().map_err(|_| usage.to_owned())?;
            if want != got {
                return Err("error: argv/parsed seconds mismatch (impossible)".to_owned());
            }
        }
        _ => return Err("error: argv/parsed seconds mismatch (impossible)".to_owned()),
    }
    Ok(())
}
