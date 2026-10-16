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
    InputTap {
        seconds: u64,
        suppress: bool,
    },
    InputSession {
        seconds: u64,
        route: crate::input_session::InputSessionRoute,
        post_route: crate::input_session::PostRoute,
        stall_once_ms: u64,
        physical_stall: bool,
        direct_disable_hold_ms: u64,
        direct_rearm: bool,
        disable_once: bool,
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
        space_join: bool,
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
    OwnedSpaceJoin {
        pid: u32,
        nonce: String,
    },
    SpacesOwnedRecovery {
        original: u64,
        temp: u64,
        seconds: u64,
        allow_untested: bool,
        probe_direct: bool,
    },
    SpacesBindings,
    SpacesDesktopApply {
        count: u32,
    },
    SpacesDesktopRevert,
    SpacesDesktopStatus,
    SpacesSwitchProbe {
        original: u64,
        temp: u64,
        seconds: u64,
        allow_untested: bool,
    },
    SpacesDragTransfer {
        original: u64,
        /// `Some` = two preexisting SIDs (creates/destroys no Space);
        /// `None` (`--temporary`) = create exactly one temp internally and
        /// remove only it afterwards.
        temp: Option<u64>,
        seconds: u64,
        allow_untested: bool,
    },
    SpacesSession {
        seconds: u64,
        allow_untested: bool,
    },
    McInspect,
    SpacesGrabDiagnose,
    CgInspect,
    McRemoveEmpty {
        space: u64,
        allow_untested: bool,
    },
    SpacesSelect {
        display: usize,
        actions: Vec<crate::runtime_types::InputSemanticAction>,
    },
    McLifecycle {
        seconds: u64,
        display: usize,
        allow_untested: bool,
    },
    McRecover {
        temp: u64,
        original: u64,
        seconds: u64,
        display: usize,
        allow_untested: bool,
    },
    McRestoreFrontmost {
        pid: u32,
        wid: u32,
        space: u64,
        display: usize,
        allow_untested: bool,
    },
    McMembers {
        space: u64,
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
        preset: crate::input_policy::InputPreset,
        /// True only when `--preset` was passed (settings file otherwise).
        preset_explicit: bool,
        /// Unscoped product mode (ordinary apps; `--scope` refused).
        unscoped: bool,
        /// Exact bundle-ID exclusions (unscoped only, repeatable).
        excluded: Vec<String>,
        /// Foreground log verbosity.
        log: crate::tile::TileLogLevel,
        /// Exact-run directory for graceful lifecycle (created by the
        /// foreground launcher): the owner polls `<dir>/stop-requested`
        /// and publishes its terminal outcome to `<dir>/outcome.json`.
        run_dir: Option<String>,
    },
    ScopeInspect {
        scopes: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliRequest {
    pub modifier: ProbeModifier,
    /// True only when `--ctrl`/`--modifier`/`--meta` was passed explicitly
    /// (the settings file supplies the modifier otherwise).
    pub modifier_explicit: bool,
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
            \x20 input-tap [--seconds N] [--suppress]  CGEventTap Ctrl-chord, finite RAII (max {max_tap}s)\n\
            \x20 \x20 (default observe-only, Keep always; --suppress arms exact-chord\n\
            \x20 \x20 KeyDown/KeyUp pair suppression only, unrelated keys always pass)\n\
             \x20 input-session --route tap [--seconds N] [--post-route session|hid] [--stall-once-ms MS] [--physical-stall] [--direct-disable-hold-ms MS] [--direct-rearm] [--disable-once] [--watchdog-only]\n\
             \x20 \x20 (bounded owned input fixture over the REAL native tile source:\n\
             \x20 \x20 dedicated owner thread, owned helper window, synthetic CG posts\n\
             \x20 \x20 with per-post oracle proof, helper delivery counters; --stall-once-ms\n\
             \x20 \x20 provokes a real OS timeout-disable; --physical-stall\n\
             \x20 \x20 needs --stall-once-ms and awaits ONE physical Ctrl+h press;\n\
             \x20 \x20 --direct-disable-hold-ms\n\
             \x20 \x20 parks the tap DIRECTLY OS-disabled for the exact-post\n\
             \x20 \x20 passthrough plus recovery proof (actual CGEventTapIsEnabled readbacks);\n\
             \x20 \x20 --direct-rearm re-arms EXPLICITLY after an intentional\n\
             \x20 \x20 own-fault DIRECT disable (old guard drops, FRESH source on the same owner);\n\
             \x20 \x20 --disable-once disables the tap directly once with\n\
             \x20 \x20 no fixture hold and no assumed notification kind, then reports the\n\
             \x20 \x20 ACTUAL recovery kind from the counters (user-input vs\n\
             \x20 \x20 unnotified-enabled-readback; never a timeout claim without the\n\
             \x20 \x20 actual timeout counter) and re-proves exact suppression;\n\
            \x20 \x20 --watchdog-only is 2..=5s with no helper/posts: arms the tap,\n\
            \x20 \x20 parks past the hard watchdog, dirty exit is the evidence; input LIVE only,\n\
            \x20 \x20 no tiling, no frame writes)\n\
           \x20 tile --user-start (--scope BUNDLE [--scope BUNDLE ...] | --unscoped [--exclude BUNDLE ...]) [--seconds N] [--input tap|off] [--preset authentic|compatible] [--log normal|verbose|trace] [--ctrl meta|ctrl|alt|shift]\n\
           \x20 \x20 (tile session: explicit --user-start plus a product selection are\n\
           \x20 \x20 mandatory — either a non-empty scoped allowlist (Ghostty/own/\n\
           \x20 \x20 unknown bundles never write) or --unscoped for ordinary apps on\n\
           \x20 \x20 each display's current Space (Ghostty an ordinary candidate;\n\
           \x20 \x20 own bundles plus --exclude stay out). Scoped lifetimes are\n\
           \x20 \x20 finite 1..={max_probe}s (default {default_tile}s); unscoped\n\
           \x20 \x20 --seconds 0 runs unbounded until owner exit or exact-PID stop.\n\
           \x20 \x20 event tap by default, off parks; authentic consumes\n\
            \x20 \x20 the exact supported chords while running, compatible leaves known\n\
            \x20 \x20 conflicting chords (unshifted Ctrl+arrows under the Ctrl meta) to macOS;\n\
           \x20 \x20 modifier/preset/exclusions resolve CLI-over-settings-file;\n\
           \x20 \x20 --run-dir DIR opts into exact-run lifecycle (stop file\n\
           \x20 \x20 polled every cadence, terminal outcome published to\n\
           \x20 \x20 DIR/outcome.json);\n\
           \x20 \x20 no native-setting writes on any path; stop leaves windows in\n\
           \x20 \x20 place, never restores)\n\
            \x20 spaces-move --pid PID --nonce HEX --wid WID --space ID --allow-untested\n\
            \x20 \x20 (direct-SLS owned move with preimage + effect readback; experimental:\n\
            \x20 \x20 one gated dispatch observed no-effect on an owned fixture;\n\
            \x20 \x20 explicit opt-in per invocation; never auto-restores — run spaces-restore\n\
            \x20 \x20 separately; the single-shot inverse is unverified, the separate\n\
            \x20 \x20 public held-Child recovery is proven per run)\n\
           \x20 spaces-restore --pid PID --nonce HEX --wid WID --space ID --allow-untested\n\
           \x20 \x20 (independent restore with the same identity validation; usable after mover exit)\n\
            \x20 spaces-switch --pid PID --nonce HEX --space ID\n\
            \x20 spaces-create --pid PID --nonce HEX\n\
            \x20 spaces-destroy --pid PID --nonce HEX --space ID\n\
            \x20 \x20 (single-shot, explicitly unsupported: no implemented single-shot\n\
            \x20 \x20 route here; report the route, never execute — surveyed non-SA\n\
            \x20 \x20 single-shot alternatives are visible AX Mission Control\n\
            \x20 \x20 automation (create/remove/goto) and global synthetic gestures /\n\
            \x20 \x20 drag+system-chord (switch/move), unimplemented single-shot here,\n\
            \x20 \x20 never a SIP rejection; use the bounded gated probes instead:\n\
            \x20 \x20 spaces-switch-probe, spaces-owned-recovery,\n\
            \x20 \x20 mc-lifecycle/mc-recover/mc-members/mc-restore-frontmost,\n\
            \x20 \x20 spaces-bindings)\n\
               \x20 probe-spawn [--hung-ms MS] [--slow-ms MS] [--seconds N] [--fixed-size] [--regular-policy] [--close-window-ms MS] [--transient-panel-ms MS] [--transient-sheet-ms MS] [--space-join]\n\
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
             \x20 \x20 float-g, grow-h/j/k/l, grow-left/right, shrink-h/j/k/l,\n\
             \x20 \x20 shrink-left/right, orientation-o; truthfully synthetic,\n\
             \x20 \x20 oracle-gated per edge, helper targets only, never Ghostty)\n\
              \x20 owned-focus --pid PID --nonce HEX --wid WID\n\
              \x20 \x20 (SLPS focus dispatch to one exact owned window with agreement)\n\
              \x20 owned-space-join --pid PID --nonce HEX\n\
              \x20 \x20 (public AppKit self-action: helper joins its OWN window\n\
              \x20 \x20 to the current Space via MoveToActiveSpace + self-activate\n\
              \x20 \x20 with exact collectionBehavior restore; no WID, no SLS,\n\
              \x20 \x20 no synthetic events; needs --space-join helper)\n\
            \x20 spaces-owned-recovery --original SID --temp SID [--seconds N] --allow-untested [--probe-direct]\n\
            \x20 \x20 (bounded owned public recovery battery, default 120s, minimum 60s:\n\
            \x20 \x20 spawn one regular-policy helper, prove O->T->O with exact\n\
            \x20 \x20 membership plus exact frame restoration, stop exactly that\n\
            \x20 \x20 helper; creates/destroys no Space, attempts no SLS move,\n\
            \x20 \x20 posts no synthetic events; explicit opt-in per invocation;\n\
            \x20 \x20 --probe-direct additionally runs ONE gated direct-SLS move\n\
            \x20 \x20 after O->T->O in a separate short-lived mover child with\n\
            \x20 \x20 delayed readbacks plus independent public restore)\n\
            \x20 spaces-bindings\n\
            \x20 \x20 (read-only effective symbolic-hotkey table via public\n\
            \x20 \x20 CopySymbolicHotKeys: chord + enabled entries with NO symbolic\n\
            \x20 \x20 IDs (index is never an ID), plus Ctrl+Left/Right\n\
            \x20 \x20 corroboration; no settings reads or writes)\n\
            \x20 spaces-desktop-apply [--count N]\n\
            \x20 \x20 (explicit Authentic Desktop-N enable: enables Switch to\n\
            \x20 \x20 Desktop 1..=N (default 9, max 10) session-effectively\n\
            \x20 \x20 via SkyLight SPI with owned exact preimages; the durable\n\
            \x20 \x20 preimage publishes before the first write and is never\n\
            \x20 \x20 overwritten (a second Apply resumes); every write reads\n\
            \x20 \x20 back; per-ID failures never block siblings; no plist is\n\
            \x20 \x20 written and reboot persistence is unverified)\n\
            \x20 spaces-desktop-revert\n\
            \x20 \x20 (explicit conditional restore of the owned preimages:\n\
            \x20 \x20 entries changed after Apply are preserved and reported,\n\
            \x20 \x20 never overwritten; the preimage file is removed only on\n\
            \x20 \x20 a clean full restore)\n\
            \x20 spaces-desktop-status\n\
            \x20 \x20 (read-only Desktop-N status: preimage presence plus\n\
            \x20 \x20 effective Ctrl+digit corroboration; no settings reads\n\
            \x20 \x20 or writes)\n\
            \x20 spaces-switch-probe --original SID --temp SID [--seconds N] --allow-untested\n\
            \x20 \x20 (bounded same-display switch comparison, default 120s, minimum\n\
            \x20 \x20 60s: two owned regular-policy helpers bracket arrow legs\n\
            \x20 \x20 (enabled Ctrl+Right/Left down/up, exact-helper oracle per\n\
            \x20 \x20 edge) and the yabai began/ended gesture pair (same display,\n\
            \x20 \x20 no warp/click); creates/destroys no Space, performs no SLS\n\
            \x20 \x20 move, posts no tap input; explicit opt-in per invocation)\n\
             \x20 cg-inspect (read-only unfiltered title-free window snapshot)\n\
             \x20 mc-remove-empty --space SID --allow-untested (one exact noncurrent empty user SID, no broad cleanup)\n\
             \x20 spaces-select [--display N] --action ID [--action ID ...]\n\
             \x20 \x20 (select-only batch, e.g. workspace-select-0, workspace-select-1, workspace-previous; changes persist, history lasts this invocation; the product tile arms the 18 existing-Space selects as hotkeys, trailing select-0 stays explicit-only)\n\
             \x20 spaces-grab-diagnose (bounded owned helper, unfiltered CG/AX/input hit snapshots before/after MC; zero synthetic posts)\n\
             \x20 spaces-drag-transfer --original SID (--temp SID | --temporary) [--seconds N] --allow-untested\n\
            \x20 \x20 (bounded owned titlebar-drag transfer, default 120s,\n\
            \x20 \x20 minimum 60s: one owned helper, independent public\n\
            \x20 \x20 self-join recovery proven first, then a synthetic\n\
            \x20 \x20 titlebar-drag plus tagged Ctrl+arrow chord outward with\n\
            \x20 \x20 an exact drag-back inverse; --temp runs over two\n\
            \x20 \x20 preexisting SIDs (creates/destroys no Space),\n\
            \x20 \x20 --temporary creates exactly one temp on the original's\n\
            \x20 \x20 display (baseline must read exactly [original]) and\n\
            \x20 \x20 removes only it afterwards; performs no SLS move,\n\
            \x20 \x20 writes no preferences; explicit\n\
            \x20 \x20 opt-in per invocation)\n\
           \x20 spaces-session [--seconds N] [--allow-untested]\n\
           \x20 \x20 (single bounded Child-holder session: lifecycle proofs, CG identity,\n\
           \x20 \x20 membership/topology/permission readbacks; dispatches no Space writes;\n\
           \x20 \x20 never cross-command title auth)\n\
            \x20 mc-inspect\n\
            \x20 \x20 (read-only Mission Control probe: session lock, AX trust, Dock\n\
            \x20 \x20 identity, MC-group presence, sanitized topology; no launch, no\n\
            \x20 \x20 traversal past presence, no writes, no synthetic events)\n\
              \x20 mc-lifecycle [--seconds N] [--display INDEX] --allow-untested\n\
              \x20 \x20 (bounded temp-Space lifecycle, default 60s, minimum 45s:\n\
              \x20 \x20 create exactly one Space on the display, go to it and back,\n\
              \x20 \x20 remove ONLY the created Space; public NSWorkspace toggle +\n\
              \x20 \x20 public Dock AX, zero window writes, zero synthetic posts;\n\
              \x20 \x20 explicit opt-in per invocation, no default mutation; experimental:\n\
              \x20 \x20 the integrated run is NOT passed (first run failed on the\n\
              \x20 \x20 immediate goto) — only the separate corrected recovery+remove\n\
              \x20 \x20 passed)\n\
             \x20 mc-recover --temp SID --original SID [--seconds N] [--display INDEX] --allow-untested\n\
             \x20 \x20 (bounded recovery for one previously adopted temp: adopt ONLY\n\
             \x20 \x20 the exact ordered [original, temp] topology on the display,\n\
             \x20 \x20 go back to the original (settled readback, never a replay),\n\
             \x20 \x20 verify the natural Ghostty return, enumerate temp residuals\n\
             \x20 \x20 read-only, and remove ONLY that temp when the strict\n\
             \x20 \x20 emptiness gate already proves clear (else parked, never\n\
             \x20 \x20 clean); creates no Space; explicit opt-in per invocation)\n\
             \x20 mc-members --space SID\n\
             \x20 \x20 (read-only Space-members inspection: every live member of\n\
             \x20 \x20 the Space, shell chrome included, with PID/WID/bundle/\n\
             \x20 \x20 layer/onscreen/bounds/membership, plus the read-only\n\
             \x20 \x20 focus-oracle legs and protected Ghostty snapshot; IDs and\n\
             \x20 \x20 geometry only, never titles/content; no launch, no writes)\n\
             \x20 mc-restore-frontmost --pid PID --wid WID --space SID [--display INDEX] --allow-untested\n\
             \x20 \x20 (guarded Ghostty-only frontmost restoration: ONE public\n\
             \x20 \x20 app activation of exactly the captured original PID/WID\n\
             \x20 \x20 (live bundle must read com.mitchellh.ghostty), with a\n\
             \x20 \x20 current-Space guard plus full selected-oracle agreement\n\
             \x20 \x20 and geometry/membership/exe stability readback; no\n\
             \x20 \x20 keystrokes, no window writes, no synthetic events;\n\
             \x20 \x20 explicit opt-in per invocation)\n\
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
    let mut modifier_explicit = false;
    let mut rest: Vec<String> = Vec::new();
    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            "--ctrl" | "--modifier" | "--meta" => {
                let v = take_value(argv, &mut i, "--ctrl")?;
                modifier = ProbeModifier::parse(&v)?;
                modifier_explicit = true;
            }
            other if other.starts_with("--ctrl=") => {
                modifier = ProbeModifier::parse(other.trim_start_matches("--ctrl="))?;
                modifier_explicit = true;
            }
            other if other.starts_with("--meta=") => {
                modifier = ProbeModifier::parse(other.trim_start_matches("--meta="))?;
                modifier_explicit = true;
            }
            other if other.starts_with("--modifier=") => {
                modifier = ProbeModifier::parse(other.trim_start_matches("--modifier="))?;
                modifier_explicit = true;
            }
            _ => rest.push(argv[i].clone()),
        }
        i += 1;
    }
    let (cmd, args) = match rest.split_first() {
        None => {
            return Ok(CliRequest {
                modifier,
                modifier_explicit,
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
        "input-tap" => {
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
                    "--suppress" => {
                        suppress = true;
                    }
                    other => return Err(format!("{cmd}: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::InputTap { seconds, suppress }
        }
        "input-session" => {
            // Bounded owned input fixture over the REAL native tile source:
            // explicit --route tap is mandatory, --stall-once-ms
            // (fault-injection leg provoking a real OS
            // timeout-disable, never a manual callback),
            // --direct-disable-hold-ms (DIRECT OS-disable plus
            // recovery proof with actual CGEventTapIsEnabled readbacks),
            // --direct-rearm (EXPLICIT fresh re-arm after an
            // intentional own-fault DIRECT disable: old guard drops without a
            // same-tap re-enable, fresh source installs on the same owner),
            // --disable-once (one-shot directly-disabled fault
            // with no fixture hold and no assumed notification kind; the leg
            // reports the actual recovery kind from the counters),
            // --physical-stall needs --stall-once-ms (awaits ONE
            // physical Ctrl+h press instead of a synthetic exact post, so the
            // stall consumes real HID load; recovery still re-proves
            // suppression synthetically),
            // --watchdog-only is 2..=5s with no helper/posts
            // (the dirty watchdog exit is the evidence).
            // --post-route session|hid selects the CG producer tap
            // (default session; hid is the fixture-only upstream
            // discriminator, same chords/owned window, no tier change).
            let mut seconds = crate::input_session::DEFAULT_INPUT_SECONDS;
            let mut route: Option<crate::input_session::InputSessionRoute> = None;
            let mut post_route = crate::input_session::PostRoute::Session;
            let mut stall_once_ms = 0u64;
            let mut physical_stall = false;
            let mut direct_disable_hold_ms = 0u64;
            let mut direct_rearm = false;
            let mut disable_once = false;
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
                    "--physical-stall" => {
                        physical_stall = true;
                    }
                    "--direct-rearm" => {
                        direct_rearm = true;
                    }
                    "--disable-once" => {
                        disable_once = true;
                    }
                    other => return Err(format!("input-session: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::InputSession {
                seconds,
                route: route.ok_or("input-session needs --route tap")?,
                post_route,
                stall_once_ms,
                physical_stall,
                direct_disable_hold_ms,
                direct_rearm,
                disable_once,
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
            let mut space_join = false;
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
                    "--space-join" => {
                        space_join = true;
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
                space_join,
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
        "owned-space-join" => {
            // No --wid: the helper acts solely on its own window; a caller
            // WID would be foreign-window selection and is refused by shape.
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
                    other => return Err(format!("owned-space-join: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::OwnedSpaceJoin {
                pid: pid.ok_or("owned-space-join needs --pid")?,
                nonce: nonce.ok_or("owned-space-join needs --nonce")?,
            }
        }
        "spaces-owned-recovery" => {
            // Bounded owned public recovery battery over two EXISTING SIDs:
            // never creates or destroys a Space. Both SIDs mandatory, budget
            // default 120 s (helper lifetime cap), explicit --allow-untested
            // opt-in per invocation (range/minimum enforced in the runner).
            let mut original: Option<u64> = None;
            let mut temp: Option<u64> = None;
            let mut seconds = crate::config::MAX_PROBE_SECONDS;
            let mut allow_untested = false;
            let mut probe_direct = false;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--original" => {
                        j += 1;
                        original = Some(parse_u64(
                            args.get(j).ok_or("--original needs a value")?,
                            "--original",
                        )?);
                    }
                    "--temp" => {
                        j += 1;
                        temp = Some(parse_u64(
                            args.get(j).ok_or("--temp needs a value")?,
                            "--temp",
                        )?);
                    }
                    "--seconds" => {
                        j += 1;
                        seconds =
                            parse_u64(args.get(j).ok_or("--seconds needs a value")?, "--seconds")?;
                    }
                    "--allow-untested" => {
                        allow_untested = true;
                    }
                    "--probe-direct" => {
                        probe_direct = true;
                    }
                    other => return Err(format!("spaces-owned-recovery: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::SpacesOwnedRecovery {
                original: original.ok_or("spaces-owned-recovery needs --original")?,
                temp: temp.ok_or("spaces-owned-recovery needs --temp")?,
                seconds,
                allow_untested,
                probe_direct,
            }
        }
        "spaces-bindings" => {
            if !args.is_empty() {
                return Err(format!("spaces-bindings: unknown arg '{}'", args[0]));
            }
            Command::SpacesBindings
        }
        "spaces-desktop-apply" => {
            // Explicit Authentic Desktop-N enable with owned preimages.
            // --count selects Desktops 1..=N (default 9: numbered
            // select/send); explicit per invocation, never at startup.
            let mut count = crate::desktop_shortcuts::DEFAULT_APPLY_COUNT;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--count" => {
                        j += 1;
                        count = parse_u32(args.get(j).ok_or("--count needs a value")?, "--count")?;
                    }
                    other => return Err(format!("spaces-desktop-apply: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::SpacesDesktopApply { count }
        }
        "spaces-desktop-revert" => {
            if !args.is_empty() {
                return Err(format!("spaces-desktop-revert: unknown arg '{}'", args[0]));
            }
            Command::SpacesDesktopRevert
        }
        "spaces-desktop-status" => {
            if !args.is_empty() {
                return Err(format!("spaces-desktop-status: unknown arg '{}'", args[0]));
            }
            Command::SpacesDesktopStatus
        }
        "spaces-switch-probe" => {
            // Bounded switch comparison over two EXISTING SIDs: never
            // creates or destroys a Space, performs no SLS move.
            let mut original: Option<u64> = None;
            let mut temp: Option<u64> = None;
            let mut seconds = crate::config::MAX_PROBE_SECONDS;
            let mut allow_untested = false;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--original" => {
                        j += 1;
                        original = Some(parse_u64(
                            args.get(j).ok_or("--original needs a value")?,
                            "--original",
                        )?);
                    }
                    "--temp" => {
                        j += 1;
                        temp = Some(parse_u64(
                            args.get(j).ok_or("--temp needs a value")?,
                            "--temp",
                        )?);
                    }
                    "--seconds" => {
                        j += 1;
                        seconds =
                            parse_u64(args.get(j).ok_or("--seconds needs a value")?, "--seconds")?;
                    }
                    "--allow-untested" => {
                        allow_untested = true;
                    }
                    other => return Err(format!("spaces-switch-probe: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::SpacesSwitchProbe {
                original: original.ok_or("spaces-switch-probe needs --original")?,
                temp: temp.ok_or("spaces-switch-probe needs --temp")?,
                seconds,
                allow_untested,
            }
        }
        "spaces-drag-transfer" => {
            // Bounded owned titlebar-drag transfer: either over two
            // EXISTING SIDs (--temp SID: creates/destroys no Space,
            // performs no SLS move, writes no preferences) or with exactly
            // one internally created temp (--temporary: the baseline must
            // read exactly [original] on the display; ONLY the created
            // temp is removed afterwards). --temp and --temporary are
            // mutually exclusive.
            let mut original: Option<u64> = None;
            let mut temp: Option<u64> = None;
            let mut temporary = false;
            let mut seconds = crate::config::MAX_PROBE_SECONDS;
            let mut allow_untested = false;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--original" => {
                        j += 1;
                        original = Some(parse_u64(
                            args.get(j).ok_or("--original needs a value")?,
                            "--original",
                        )?);
                    }
                    "--temp" => {
                        j += 1;
                        temp = Some(parse_u64(
                            args.get(j).ok_or("--temp needs a value")?,
                            "--temp",
                        )?);
                    }
                    "--temporary" => {
                        temporary = true;
                    }
                    "--seconds" => {
                        j += 1;
                        seconds =
                            parse_u64(args.get(j).ok_or("--seconds needs a value")?, "--seconds")?;
                    }
                    "--allow-untested" => {
                        allow_untested = true;
                    }
                    other => return Err(format!("spaces-drag-transfer: unknown arg '{other}'")),
                }
                j += 1;
            }
            if temporary && temp.is_some() {
                return Err("spaces-drag-transfer: --temp and --temporary are exclusive".to_owned());
            }
            if !temporary && temp.is_none() {
                return Err("spaces-drag-transfer needs --temp or --temporary".to_owned());
            }
            Command::SpacesDragTransfer {
                original: original.ok_or("spaces-drag-transfer needs --original")?,
                temp,
                seconds,
                allow_untested,
            }
        }
        "tile" => {
            // Tile session: explicit `--user-start` plus a product selection
            // are mandatory — either a non-empty `--scope` allowlist or
            // `--unscoped` (mutually exclusive). The tap is the product
            // input route, and the preset selects Authentic (consume exact
            // supported chords) or Compatible (leave known conflicts to
            // macOS). Modifier/preset resolve CLI-over-settings-file in
            // `main` (explicitness rides along); exclusions are CLI or file.
            // Scoped lifetimes stay finite; unscoped `--seconds 0` runs
            // unbounded. No native-setting writes on any path.
            let mut user_start = false;
            let mut scopes: Vec<String> = Vec::new();
            let mut seconds = DEFAULT_TILE_SECONDS;
            let mut input = TileInput::default();
            let mut preset = crate::input_policy::InputPreset::default();
            let mut preset_explicit = false;
            let mut unscoped = false;
            let mut excluded: Vec<String> = Vec::new();
            let mut log = crate::tile::TileLogLevel::default();
            let mut run_dir: Option<String> = None;
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
                    "--unscoped" => {
                        unscoped = true;
                    }
                    "--exclude" => {
                        j += 1;
                        excluded.push(args.get(j).ok_or("--exclude needs a value")?.clone());
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
                    "--preset" => {
                        j += 1;
                        preset = crate::input_policy::InputPreset::parse(
                            args.get(j).ok_or("--preset needs a value")?,
                        )?;
                        preset_explicit = true;
                    }
                    "--log" => {
                        j += 1;
                        log = crate::tile::TileLogLevel::parse(
                            args.get(j).ok_or("--log needs a value")?,
                        )?;
                    }
                    "--run-dir" => {
                        j += 1;
                        run_dir = Some(args.get(j).ok_or("--run-dir needs a value")?.clone());
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
                preset,
                preset_explicit,
                unscoped,
                excluded,
                log,
                run_dir,
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
        "mc-inspect" => {
            // Read-only Mission Control inspection: no launch, no writes.
            if !args.is_empty() {
                return Err("usage: tiler-macos mc-inspect".to_owned());
            }
            Command::McInspect
        }
        "mc-lifecycle" => {
            // Bounded temp-Space lifecycle: explicit --allow-untested opt-in
            // per invocation (no default mutation); --display selects the
            // SLS row index (default 0). The 60s default and 45s floor leave
            // a 15s cleanup reserve inside the run budget.
            let mut seconds = 60u64;
            let mut display = 0usize;
            let mut allow_untested = false;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--seconds" => {
                        j += 1;
                        seconds =
                            parse_u64(args.get(j).ok_or("--seconds needs a value")?, "--seconds")?;
                    }
                    "--display" => {
                        j += 1;
                        display =
                            parse_u64(args.get(j).ok_or("--display needs a value")?, "--display")?
                                as usize;
                    }
                    "--allow-untested" => {
                        allow_untested = true;
                    }
                    other => return Err(format!("mc-lifecycle: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::McLifecycle {
                seconds,
                display,
                allow_untested,
            }
        }
        "mc-recover" => {
            // Bounded recovery for one previously adopted temp: the caller
            // names the adopted temp and the preimage original; the runner
            // adopts ONLY the exact ordered [original, temp] topology on
            // the display, goes back to the original, and removes ONLY that
            // temp when the strict gates prove clear. Never creates a
            // Space; explicit --allow-untested opt-in per invocation.
            let mut temp: Option<u64> = None;
            let mut original: Option<u64> = None;
            let mut seconds = 60u64;
            let mut display = 0usize;
            let mut allow_untested = false;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--temp" => {
                        j += 1;
                        temp = Some(parse_u64(
                            args.get(j).ok_or("--temp needs a value")?,
                            "--temp",
                        )?);
                    }
                    "--original" => {
                        j += 1;
                        original = Some(parse_u64(
                            args.get(j).ok_or("--original needs a value")?,
                            "--original",
                        )?);
                    }
                    "--seconds" => {
                        j += 1;
                        seconds =
                            parse_u64(args.get(j).ok_or("--seconds needs a value")?, "--seconds")?;
                    }
                    "--display" => {
                        j += 1;
                        display =
                            parse_u64(args.get(j).ok_or("--display needs a value")?, "--display")?
                                as usize;
                    }
                    "--allow-untested" => {
                        allow_untested = true;
                    }
                    other => return Err(format!("mc-recover: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::McRecover {
                temp: temp.ok_or("mc-recover needs --temp")?,
                original: original.ok_or("mc-recover needs --original")?,
                seconds,
                display,
                allow_untested,
            }
        }
        "mc-members" => {
            // Read-only Space-members inspection: one mandatory --space
            // SID; every live member reported with identity/geometry/
            // membership, no writes on any path.
            let mut space: Option<u64> = None;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--space" => {
                        j += 1;
                        space = Some(parse_u64(
                            args.get(j).ok_or("--space needs a value")?,
                            "--space",
                        )?);
                    }
                    other => return Err(format!("mc-members: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::McMembers {
                space: space.ok_or("mc-members needs --space")?,
            }
        }
        "mc-restore-frontmost" => {
            // Guarded Ghostty-only frontmost restoration: the caller names
            // the exact captured original (PID, WID, Space); the runner
            // activates ONLY that PID when its live bundle reads exactly
            // com.mitchellh.ghostty, the Space is current, and every
            // post readback agrees. Never a general arbitrary-window API:
            // non-Ghostty bundles always refuse, and only one public
            // activation is ever sent (no keystrokes, no window writes).
            let mut pid: Option<u32> = None;
            let mut wid: Option<u32> = None;
            let mut space: Option<u64> = None;
            let mut display = 0usize;
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
                    "--display" => {
                        j += 1;
                        display =
                            parse_u64(args.get(j).ok_or("--display needs a value")?, "--display")?
                                as usize;
                    }
                    "--allow-untested" => {
                        allow_untested = true;
                    }
                    other => return Err(format!("mc-restore-frontmost: unknown arg '{other}'")),
                }
                j += 1;
            }
            Command::McRestoreFrontmost {
                pid: pid.ok_or("mc-restore-frontmost needs --pid")?,
                wid: wid.ok_or("mc-restore-frontmost needs --wid")?,
                space: space.ok_or("mc-restore-frontmost needs --space")?,
                display,
                allow_untested,
            }
        }
        "mc-remove-empty" => {
            let mut space = None;
            let mut allow_untested = false;
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--space" => {
                        j += 1;
                        space = Some(parse_u64(
                            args.get(j).ok_or("--space needs a value")?,
                            "--space",
                        )?);
                    }
                    "--allow-untested" => allow_untested = true,
                    other => return Err(format!("mc-remove-empty: unknown arg '{other}'")),
                }
                j += 1;
            }
            let space = space
                .filter(|sid| *sid != 0)
                .ok_or("mc-remove-empty needs nonzero --space")?;
            if !allow_untested {
                return Err("mc-remove-empty needs --allow-untested".to_owned());
            }
            Command::McRemoveEmpty {
                space,
                allow_untested,
            }
        }
        "spaces-select" => {
            let mut display = 0;
            let mut actions = Vec::new();
            let mut j = 0;
            while j < args.len() {
                match args[j].as_str() {
                    "--display" => {
                        j += 1;
                        display =
                            parse_u32(args.get(j).ok_or("--display needs a value")?, "--display")?
                                as usize;
                    }
                    "--action" => {
                        j += 1;
                        let id = args.get(j).ok_or("--action needs a value")?;
                        let action = crate::workspace_input::parse_workspace_action(id)
                            .ok_or("unknown workspace action ID")?;
                        if !matches!(
                            action,
                            crate::runtime_types::InputSemanticAction::WorkspaceSelect { .. }
                                | crate::runtime_types::InputSemanticAction::WorkspaceRelative { .. }
                                | crate::runtime_types::InputSemanticAction::WorkspacePrevious
                        ) {
                            return Err("spaces-select refuses non-select actions".to_owned());
                        }
                        actions.push(action);
                    }
                    other => return Err(format!("spaces-select: unknown arg '{other}'")),
                }
                j += 1;
            }
            if actions.is_empty() || actions.len() > 16 {
                return Err("spaces-select needs 1..=16 --action IDs".to_owned());
            }
            Command::SpacesSelect { display, actions }
        }
        "cg-inspect" => {
            if !args.is_empty() {
                return Err("cg-inspect takes no arguments".to_owned());
            }
            Command::CgInspect
        }
        "spaces-grab-diagnose" => {
            if !args.is_empty() {
                return Err("spaces-grab-diagnose takes no arguments".to_owned());
            }
            Command::SpacesGrabDiagnose
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
            | Command::SpacesBindings
            | Command::SpacesDesktopStatus
            | Command::CgOwnedWindows { .. }
            | Command::CgInspect
            | Command::OwnedFrame { .. }
            | Command::ScopeInspect { .. }
            | Command::McInspect
            | Command::McMembers { .. }
    );
    Ok(CliRequest {
        modifier,
        modifier_explicit,
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
