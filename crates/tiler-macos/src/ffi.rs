#![allow(unsafe_code)]
//! Sole `unsafe` module in the crate. Everything else forbids unsafe.
//!
//! Policy: public AX/CF/CG surface uses maintained bindings
//! (`objc2-application-services` for AXUIElement/AXObserver/AXValue,
//! `objc2-core-foundation` for CFString/CFArray/CFDictionary/CFNumber/CFRunLoop,
//! `objc2-app-kit`/`objc2-foundation` for process identity,
//! `objc2-core-graphics` (`CGEvent::tap_enable`/`tap_is_enabled` via the
//! existing `CGEvent` feature) for the tap-state pair, `core-foundation`/
//! `core-graphics` for the event tap itself, `libc` for `dlopen`/`dlsym`/
//! `kill`/`proc_pidpath`). The sole hand-written `extern "C"` declaration is
//! the private `_AXUIElementGetWindow` in the small `private_raw` module
//! (genuinely uncovered private SPI). The
//! SkyLight (SLS), SLPS, and read-only symbolic-hotkey dynamic tables live in
//! this module beside it (not inside it): each entry point is resolved at runtime via
//! maintained `libc::dlopen`/`libc::dlsym` with a null-checked signature, and
//! a single missing symbol fails the table closed. Each `unsafe` block carries
//! an inline justification: null checks before use, bounded lengths, no
//! retained foreign references escaping, and no panics across FFI boundaries
//! (callback bodies never panic).
//!
//! Soundness rule: no raw foreign pointer crosses this module's boundary.
//! Callers receive owned opaque handles (`AxElement`), plain data
//! (`SlsDisplay`, reports), or errors. There is deliberately no public
//! function accepting an arbitrary non-null foreign pointer.
//!
//! Honest boundary note: calling maintained `objc2` AX/CF wrappers still
//! requires isolated `unsafe` blocks at each call site (their APIs are
//! `unsafe` because they touch live AX objects). The win is two hand-written
//! `extern "C"` declarations (the private join above plus the tap-state pair
//! below) instead of dozens for
//! public AX/CF/CG/libc APIs; the remaining `unsafe` is wrapper-call boundary
//! plus the small private dynamic-symbol tables described above. Private SLS
//! top-level results are wrapped as `CFType` and checked-downcast to
//! `CFArray` before any typed view (fail closed on a non-array); nested SLS
//! container kinds are verified before typed use (see
//! `sls_display_spaces`); elements are indexed within `len().min(...)`
//! rather than materialized unbounded. No custom `unsafe impl Send/Sync` anywhere.

#[cfg(target_os = "macos")]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
    mpsc::SyncSender,
};

use serde::{Deserialize, Serialize};

use crate::engine::EngineMessage;

/// AX error codes (SDK `AXError.h`:
/// `ApplicationServices/HIServices/AXError.h`, verified against the CLT
/// header and the maintained `objc2-application-services::AXError`
/// constants). Only named codes are reported by name; unknown codes surface
/// numerically, never guessed. The numeric arms below stay literals so this
/// function classifies identically on every platform (the `objc2` dependency
/// is macOS-only); each arm is pinned to its maintained constant by
/// `tests::ax_error_codes_bind_sdk_constants`.
pub const AX_NOT_IMPLEMENTED: i32 = -25208;

pub fn ax_error_name(code: i32) -> String {
    match code {
        0 => "kAXErrorSuccess".to_owned(),
        -25200 => "kAXErrorFailure".to_owned(),
        -25201 => "kAXErrorIllegalArgument".to_owned(),
        -25202 => "kAXErrorInvalidUIElement".to_owned(),
        -25203 => "kAXErrorInvalidUIElementObserver".to_owned(),
        -25204 => "kAXErrorCannotComplete".to_owned(),
        -25205 => "kAXErrorAttributeUnsupported".to_owned(),
        -25206 => "kAXErrorActionUnsupported".to_owned(),
        -25207 => "kAXErrorNotificationUnsupported".to_owned(),
        -25208 => "kAXErrorNotImplemented".to_owned(),
        -25209 => "kAXErrorNotificationAlreadyRegistered".to_owned(),
        -25210 => "kAXErrorNotificationNotRegistered".to_owned(),
        -25211 => "kAXErrorAPIDisabled".to_owned(),
        -25212 => "kAXErrorNoValue".to_owned(),
        -25213 => "kAXErrorParameterizedAttributeUnsupported".to_owned(),
        -25214 => "kAXErrorNotEnoughPrecision".to_owned(),
        other => format!("AXError {other}"),
    }
}

/// Secure Input observation. `Unknown` (missing symbol) is distinct from
/// `Inactive`: a missing symbol is never reported as inactive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecureState {
    Active,
    Inactive,
    Unknown,
}

/// Actual window-server session lock verdict: the login-done plus on-console
/// legs of the session dictionary plus the screen-lock marker, never the
/// loginwindow-frontmost heuristic alone. `Unknown` (unreadable session or
/// malformed marker) fails closed everywhere it gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionLock {
    /// Readable session, login done, on console, screen unlocked: the
    /// ordinary unlocked desktop.
    Unlocked,
    /// Screen locked, login not done, or off console: no unlocked desktop
    /// to act on.
    Locked,
    /// Unreadable session dictionary or malformed marker: refuse, never
    /// proceed.
    Unknown,
}

/// Raw screen-lock marker leg from the session dictionary: absent (normally
/// unlocked — the marker only appears to report the locked state),
/// a boolean verdict, or a malformed (present but wrong-typed) read.
/// Absence and malformation are distinct: absence proceeds under the other
/// legs, malformation fails closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScreenLockRead {
    /// Marker absent: normally unlocked (the window server only plants the
    /// marker to report the locked state).
    Absent,
    /// Marker reads locked.
    Locked,
    /// Marker reads unlocked.
    Unlocked,
    /// Marker present but wrong-typed: fail closed, never guessed.
    Malformed,
}

/// Read-only window-server session legs plus the lock verdict. Booleans and
/// static key spellings only; never user names or content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionState {
    pub login_done: Option<bool>,
    pub on_console: Option<bool>,
    /// Raw screen-lock marker leg (absence vs malformed distinguished).
    pub screen_lock: ScreenLockRead,
    pub lock: SessionLock,
}

impl SessionState {
    #[must_use]
    pub const fn unlocked(self) -> bool {
        matches!(self.lock, SessionLock::Unlocked)
    }

    #[must_use]
    pub const fn unknown() -> Self {
        Self {
            login_done: None,
            on_console: None,
            screen_lock: ScreenLockRead::Absent,
            lock: SessionLock::Unknown,
        }
    }
}

/// Pure lock verdict over the session legs: a locked screen marker is
/// locked (screen-lock proof, strongest leg); a malformed marker is unknown;
/// otherwise login-done AND on-console is the ordinary unlocked desktop, any
/// false leg is locked, and anything unreadable is unknown (fail closed —
/// never guessed unlocked).
#[must_use]
pub const fn lock_from_session_legs(
    login_done: Option<bool>,
    on_console: Option<bool>,
    screen_lock: ScreenLockRead,
) -> SessionLock {
    match screen_lock {
        ScreenLockRead::Locked => SessionLock::Locked,
        ScreenLockRead::Malformed => SessionLock::Unknown,
        ScreenLockRead::Absent | ScreenLockRead::Unlocked => match (login_done, on_console) {
            (Some(true), Some(true)) => SessionLock::Unlocked,
            (Some(false), _) | (_, Some(false)) => SessionLock::Locked,
            _ => SessionLock::Unknown,
        },
    }
}

/// Read-only focus-diagnose legs: per-leg AX statuses plus IDs/counts only.
/// No titles, content, paths, or owner names anywhere on this surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AxSystemFocusLeg {
    pub system_element: String,
    pub system_timeout_status: Option<i32>,
    pub system_timeout_name: Option<String>,
    pub focused_app_status: String,
    pub focused_pid: Option<i32>,
    pub app_timeout_status: Option<i32>,
    pub app_timeout_name: Option<String>,
    pub app_pid_status: String,
    pub focused_window_status: String,
    pub focused_wid: Option<u32>,
}

/// Direct per-application focus read for exactly one PID (the observed
/// frontmost): window count (no every-window joins: bounded diagnose
/// inspects focused/main/count only), focused/main window joins, and the
/// `AXFrontmost` pair read BEFORE and AFTER all window legs. IDs only;
/// never titles, content, or paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AxAppFocusLeg {
    pub pid: i32,
    pub element: String,
    pub timeout_status: Option<i32>,
    pub timeout_name: Option<String>,
    pub windows_status: String,
    pub windows_count: Option<usize>,
    pub window_join_failures: u64,
    pub joined_wids: Vec<u32>,
    pub focused_status: String,
    pub focused_wid: Option<u32>,
    pub main_status: String,
    pub main_wid: Option<u32>,
    /// `AXFrontmost` read BEFORE the window legs: `Ok(true)` is the
    /// healthy corroborating leg; `Ok(false)` or any `Err` status name is
    /// carried here (unknown/false refuses the fallback, never assumed).
    pub frontmost_before_status: String,
    pub frontmost_before: Option<bool>,
    /// `AXFrontmost` read AFTER the window legs: both sides must be
    /// `Some(true)` and stable for any corroborated candidate. A
    /// before/after disagreement is instability data, never a proven host
    /// flap claim.
    pub frontmost_after_status: String,
    pub frontmost_after: Option<bool>,
}

/// Which native route produced a focus preimage. Serialized in reports so
/// selected-route evidence is always distinguishable from raw diagnostic
/// reads. `Systemwide` is the raw system-wide `AXFocusedApplication` read:
/// diagnostic-only, never production evidence. `SlpsFrontCorroborated` is
/// the experimental unproven corroboration (behind `focus-diagnose
/// --fallback`): diagnostic-only, never production evidence. The two
/// `SlpsFrontDirect*` routes are the selected production oracle: the
/// read-only SLPS front-process PID agreeing with the NSWorkspace frontmost
/// before AND after, plus the direct per-app focused-window read on that PID
/// (`Focused` when `AXFocusedWindow` joined, `Main` when it was
/// unavailable/missing and `AXMainWindow` joined instead) with a fresh CG
/// owner join. No system-wide AX and no `AXFrontmost` gating on any selected
/// path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PreimageRoute {
    /// Raw system-wide `AXFocusedApplication` read. Diagnostic-only.
    Systemwide,
    /// System-wide service leg failed; corroborated fallback instead (see
    /// [`ax_focused_preimage_evidence`]). Diagnostic-only, unproven.
    SlpsFrontCorroborated,
    /// Selected oracle with the direct `AXFocusedWindow` leg joined.
    SlpsFrontDirectFocused,
    /// Selected oracle with the `AXFocusedWindow` leg unavailable/missing
    /// and the `AXMainWindow` leg joined instead.
    SlpsFrontDirectMain,
}

impl PreimageRoute {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Systemwide => "systemwide",
            Self::SlpsFrontCorroborated => "slps-front-corroborated",
            Self::SlpsFrontDirectFocused => "slps-front-direct-focused",
            Self::SlpsFrontDirectMain => "slps-front-direct-main",
        }
    }
}

/// Focus preimage plus its provenance: the focused PID/window with the route
/// that produced them and a per-leg detail string (IDs and status names
/// only — never titles, content, or paths).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreimageEvidence {
    pub pid: i32,
    pub wid: Option<u32>,
    pub route: PreimageRoute,
    pub detail: String,
}

/// Direct per-application focused-window read for exactly one PID (the
/// SLPS/NSWorkspace-agreed frontmost): the `AXFocusedWindow` join plus the
/// `AXMainWindow` fallback join with their status names, the AX identity PID
/// read, and the selected window with its source. `selected_source` is
/// `"focused"` when the focused leg joined nonzero, `"main"` when the
/// focused leg was unavailable/missing and the main leg joined nonzero, and
/// `"none"` otherwise (never a claim). No system-wide AX and no
/// `AXFrontmost` anywhere on this surface: those stay diagnostic-only.
/// IDs and status names only — never titles, content, or paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectFocusRead {
    pub pid: i32,
    pub ax_pid: Option<i32>,
    pub focused_status: String,
    pub focused_wid: Option<u32>,
    pub main_status: String,
    pub main_wid: Option<u32>,
    pub selected_wid: Option<u32>,
    pub selected_source: String,
}

/// Pure direct-window selection over plain data: the joined `AXFocusedWindow`
/// leg wins when present and nonzero; otherwise the joined `AXMainWindow`
/// leg wins when present and nonzero (unavailable/missing focused leg only);
/// otherwise nothing. Zero WIDs are missing, never selected. The main
/// fallback never overrides a usable focused leg, and a non-exact target
/// still refuses downstream in [`selected_focus_agrees`]. Unit-tested; no
/// desktop effect.
#[must_use]
pub fn select_direct_window(
    focused_wid: Option<u32>,
    main_wid: Option<u32>,
) -> (Option<u32>, &'static str) {
    match (focused_wid, main_wid) {
        (Some(wid), _) if wid != 0 => (Some(wid), "focused"),
        (_, Some(wid)) if wid != 0 => (Some(wid), "main"),
        _ => (None, "none"),
    }
}

/// Pure selected-oracle agreement over plain data: true only when the SLPS
/// front-process PID and the workspace frontmost PID both equal `want_pid`,
/// the selected direct window equals `want_wid` (nonzero), and the fresh CG
/// owner join holds. Unknown/missing foreground legs, any PID disagreement,
/// a missing/zero selected window, or a lost CG join refuses — never
/// guessed. Unit-tested; callers perform the live reads and pass the
/// outcomes here. No system-wide AX leg and no `AXFrontmost` leg participate.
#[must_use]
pub fn selected_focus_agrees(
    want_pid: u32,
    want_wid: u32,
    slps_pid: Option<i32>,
    ws_pid: Option<i32>,
    selected_wid: Option<u32>,
    cg_owned: bool,
) -> bool {
    if want_pid <= 1 || want_wid == 0 || !cg_owned {
        return false;
    }
    let Ok(want) = i32::try_from(want_pid) else {
        return false;
    };
    if slps_pid != Some(want) || ws_pid != Some(want) {
        return false;
    }
    selected_wid == Some(want_wid)
}
/// Pure fallback trigger: the experimental corroborated route is attempted
/// ONLY for the exactly observed system-wide service failures: the missing
/// system-wide element (`system-wide AX element unavailable`) or the
/// `AXFocusedApplication` leg unreadable with `kAXErrorCannotComplete`.
/// Every other systemwide outcome — success (even a later disagreement), a
/// messaging-timeout leg, a malformed/downcast leg, a permission leg, or a
/// failure on a different leg such as `AXFocusedWindow` — never triggers.
/// Unit-tested; no desktop effect. Diagnostic-only: production preimage and
/// agreement reads use the selected SLPS+direct oracle and never consult
/// this (nor the system-wide read at all).
#[must_use]
pub fn preimage_fallback_trigger(system_err: &str) -> bool {
    matches!(
        system_err,
        "system-wide AX element unavailable"
            | "AXFocusedApplication unreadable: kAXErrorCannotComplete"
    )
}

/// Pure `AXFrontmost` stability over plain data: true only when the
/// before-leg AND the after-leg both read `Some(true)`. Any unknown, false,
/// or disagreement is instability data (refuse), never a proven host-flap
/// claim. Unit-tested; no desktop effect.
#[must_use]
pub fn frontmost_pair_stable(before: Option<bool>, after: Option<bool>) -> bool {
    before == Some(true) && after == Some(true)
}

/// Pure fallback agreement over plain data: true only when the SLPS
/// front-process PID, the workspace frontmost before AND after, the direct
/// per-app AX identity, a present nonzero focused window, and the fresh CG
/// owner join ALL agree, with `AXFrontmost` observed `Some(true)` BOTH
/// before AND after the window legs. Unknown/false/either-side disagreement,
/// any PID disagreement, workspace instability mid-read, a missing/zero
/// window, or a lost CG join refuses. Unit-tested; callers perform the live
/// reads and pass the outcomes here. The before/after split exists because
/// the diagnose direct leg reads windows/focused/main before frontmost while
/// the first fallback draft read frontmost first: the canonical acquisition
/// is frontmost-before, window legs, frontmost-after, shared by both.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn preimage_fallback_agrees(
    psn_pid: i32,
    ws_before: Option<i32>,
    ws_after: Option<i32>,
    frontmost_before: Option<bool>,
    frontmost_after: Option<bool>,
    ax_pid: Option<i32>,
    ax_wid: Option<u32>,
    cg_owned: bool,
) -> bool {
    if psn_pid <= 1 || !cg_owned {
        return false;
    }
    if ws_before != Some(psn_pid) || ws_after != Some(psn_pid) {
        return false;
    }
    if !frontmost_pair_stable(frontmost_before, frontmost_after) {
        return false;
    }
    if ax_pid != Some(psn_pid) {
        return false;
    }
    matches!(ax_wid, Some(wid) if wid != 0)
}

/// Current UID for owned-state-dir ownership checks. The single `libc`
/// call in the crate lives here (the allowed `unsafe` module) so every
/// other module stays `#![forbid(unsafe_code)]`. `libc` was already a
/// transitive dependency; no new crate enters the lockfile for this.
#[cfg(unix)]
pub fn current_uid() -> u32 {
    // SAFETY: `geteuid` takes no arguments and only reads process state.
    unsafe { libc::geteuid() }
}

/// Non-Unix fallback: UID checks are skipped by callers that gate them on
/// `#[cfg(unix)]`; this only satisfies the shared code path.
#[cfg(not(unix))]
pub fn current_uid() -> u32 {
    0
}

/// Shareable dirty generation for display/Space changes. Pure atomic
/// semantics are identical on every platform; only native observer
/// installation is platform-specific. Clones share the pending change.
#[derive(Debug, Clone, Default)]
pub struct DisplayChangeFlag {
    dirty: Arc<AtomicU64>,
}

impl DisplayChangeFlag {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an external change; bursts coalesce until the next take.
    pub fn mark_changed(&self) {
        self.dirty.fetch_add(1, Ordering::SeqCst);
    }

    /// Take and clear all pending changes.
    #[must_use]
    pub fn take_changed(&self) -> bool {
        self.dirty.swap(0, Ordering::SeqCst) != 0
    }
}

/// Outcome of one bounded per-app AX observation run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AxObserveOutcome {
    pub notifications_added: u32,
    pub notifications_received: u32,
    pub messaging_timeout_set: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AxObserveError {
    InvalidPid(String),
    Unsupported(String),
    System(String),
}

impl std::fmt::Display for AxObserveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPid(r) => write!(f, "invalid pid: {r}"),
            Self::Unsupported(r) => write!(f, "unsupported: {r}"),
            Self::System(r) => write!(f, "AX system error: {r}"),
        }
    }
}

impl std::error::Error for AxObserveError {}

/// App-level session notification names (observed on the application
/// element; each fires for any window of the app). Names per the surveyed
/// AX notification taxonomy (AXSwift `AXNotification`: windowCreated,
/// focusedWindowChanged, windowMoved, windowResized) plus
/// `AXMainWindowChanged` (glide `src/actor/app.rs` `APP_NOTIFICATIONS`, rift
/// `src/actor/app.rs` `APP_NOTIFICATIONS`); registration success
/// is still verified per name at runtime — an unsupported name fails to add.
pub const AX_SESSION_APP_NOTIFICATIONS: &[&str] = &[
    "AXWindowCreated",
    "AXFocusedWindowChanged",
    "AXMainWindowChanged",
    "AXWindowMoved",
    "AXWindowResized",
];

/// Window-level session notification names (observed on the joined owned
/// window element, never a foreign one). `AXUIElementDestroyed` covers the
/// owned window's teardown; `AXMoved`/`AXResized` are the element-level
/// companions of the app-level move/resize notifications;
/// `AXWindowMiniaturized`/`AXWindowDeminiaturized` cover minimize/restore
/// (yabai `src/window.h` `ax_window_notification`, AeroSpace
/// `Sources/AppBundle/tree/MacApp.swift` `AxWindow.new`, glide
/// `src/actor/app.rs` `WINDOW_NOTIFICATIONS`, rift `src/actor/app.rs`
/// `WINDOW_NOTIFICATIONS` — all four subscribe minimize/deminimize on the
/// window element, never app-level only).
pub const AX_SESSION_WINDOW_NOTIFICATIONS: &[&str] = &[
    "AXMoved",
    "AXResized",
    "AXUIElementDestroyed",
    "AXWindowMiniaturized",
    "AXWindowDeminiaturized",
];

/// Outcome of one bounded owned-session observation run. `notifications_added`
/// names exactly what registered; `notifications_received` carries per-name
/// delivery counts (names absent from it delivered zero). Counts only — never
/// titles, frames, or content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AxSessionOutcome {
    pub notifications_added: Vec<String>,
    pub notifications_received: Vec<(String, u32)>,
    pub messaging_timeout_set: bool,
}

/// One managed display's Space membership. IDs only; no names, no content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlsDisplay {
    pub uuid: Option<String>,
    pub spaces: Vec<u64>,
    pub current: Option<u64>,
}

/// One effective symbolic-hotkey entry from public Carbon
/// `CopySymbolicHotKeys`: the effective chord (virtual keycode plus Carbon
/// modifier bits, raw as reported) plus its enabled flag. Carries NO
/// symbolic ID: the SDK documents no way to map an entry back to a
/// preference-pane hotkey, so array index is never an ID. Callers match
/// chords (with the automatic Fn bit stripped) and report corroboration,
/// never attribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolicHotKeyEntry {
    pub keycode: u32,
    pub carbon_modifiers: u32,
    pub enabled: bool,
}

/// Read-only Space type from `SLSSpaceGetType` (signature pinned against
/// yabai `src/misc/extern.h`: `extern int SLSSpaceGetType(int cid,
/// uint64_t sid)`; type ids pinned against yabai `src/space.c`
/// (`space_is_user`, `space_is_fullscreen`, `space_is_system`): 0=user,
/// 4=fullscreen, 2=system). Plain data only, never a pointer across the
/// boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlsSpaceType {
    User,
    Fullscreen,
    System,
    Other(i32),
}

/// Pure mapping of the raw `SLSSpaceGetType` integer to [`SlsSpaceType`].
/// No live state: unknown ids stay `Other`, never guessed into a known
/// kind.
pub fn classify_space_type(raw: i32) -> SlsSpaceType {
    match raw {
        0 => SlsSpaceType::User,
        4 => SlsSpaceType::Fullscreen,
        2 => SlsSpaceType::System,
        other => SlsSpaceType::Other(other),
    }
}

/// Outcome of one bounded event-tap run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TapRunReport {
    pub created: bool,
    /// Exact-chord matches observed (KeyDown/KeyUp with exact modifiers).
    pub matched: u32,
    /// Exact-chord events suppressed (only when armed via `--suppress`;
    /// always 0 in observe-only runs).
    pub suppressed: u32,
    /// Key deliveries seen (KeyDown/KeyUp only; FlagsChanged excluded).
    pub deliveries: u32,
    pub reenabled_after_timeout_disable: bool,
    pub stopped_on_second_disable: bool,
    /// Count of observed `TapDisabledByTimeout` notifications this run.
    pub timeout_disables: u32,
    /// True once a `TapDisabledByUserInput` notification ends the run. A
    /// user disable is never re-enabled: the run stops truthfully instead.
    pub user_disabled: bool,
    /// Whether this run armed exact-chord suppression (`--suppress`).
    pub suppress_armed: bool,
    pub requested_secs: u64,
    pub elapsed_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TapSetupError {
    Denied(String),
    Runloop(String),
}

impl std::fmt::Display for TapSetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Denied(r) => write!(f, "event tap denied: {r}"),
            Self::Runloop(r) => write!(f, "event tap runloop error: {r}"),
        }
    }
}

impl std::error::Error for TapSetupError {}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::ffi::CString;
    use std::ptr::NonNull;
    use std::sync::OnceLock;
    use std::time::{Duration, Instant};

    use objc2_application_services::{AXError, AXObserver, AXUIElement, AXValue, AXValueType};
    use objc2_core_foundation::{
        CFArray, CFDictionary, CFNumber, CFRetained, CFRunLoop, CFString, CFType, CGPoint, CGSize,
        kCFRunLoopDefaultMode,
    };

    /// AX status codes bound to the maintained
    /// `objc2-application-services::AXError` constants (SDK `AXError.h`
    /// values; pinned by `tests::ax_error_codes_bind_sdk_constants`), never
    /// hand-numbered. Match sites use these; the cross-platform
    /// [`super::ax_error_name`] table keeps literals (the `objc2` dependency
    /// is macOS-only) with each arm pinned by the same test.
    pub const AX_FAILURE: i32 = AXError::Failure.0;
    pub const AX_ILLEGAL_ARGUMENT: i32 = AXError::IllegalArgument.0;
    pub const AX_ATTRIBUTE_UNSUPPORTED: i32 = AXError::AttributeUnsupported.0;
    pub const AX_NO_VALUE: i32 = AXError::NoValue.0;

    // -- Sole residual hand declaration (genuinely uncovered private SPI) --
    //
    // `_AXUIElementGetWindow` has no maintained Rust wrapper; it stays
    // hand-declared here with its safe wrapper (`ax_window_id`). Everything
    // else dynamic in this module (SkyLight, SLPS, and read-only
    // symbolic-hotkey entry points below) is resolved via maintained `libc::dlopen`/`libc::dlsym`
    // with null-checked signatures because no maintained Rust wrapper covers
    // those tables; missing symbols fail closed, never guessed. Process
    // signaling/identity (`kill`, `proc_pidpath`) likewise uses `libc`.

    mod private_raw {
        unsafe extern "C" {
            pub fn _AXUIElementGetWindow(element: *const std::ffi::c_void, wid: *mut u32) -> i32;
        }
    }

    // -- Raw foreign-pointer aliases (shared by the dynamic tables below) --

    type RawRef = *mut std::ffi::c_void;
    type RawConstRef = *const std::ffi::c_void;

    // Carbon framework image for the remaining read-only dynamic table
    // (public `CopySymbolicHotKeys` below). The `RegisterEventHotKey`
    // dispatcher-target hotkey route was removed; the SLPS tables resolve
    // their own Carbon image paths inline.
    const CARBON_PATH: &[u8] = b"/System/Library/Frameworks/Carbon.framework/Carbon\0";

    // -- Read-only symbolic-hotkey effective table (public Carbon) --
    //
    // Exact signature from the CLT SDK
    // (`Carbon.framework/.../HIToolbox/Headers/CarbonEvents.h`):
    //   `extern OSStatus CopySymbolicHotKeys(CFArrayRef *outHotKeyArray)`.
    // Per-entry keys are compile-time `CFSTR` literals
    // (`"kHISymbolicHotKeyCode"`, `"kHISymbolicHotKeyModifiers"`,
    // `"kHISymbolicHotKeyEnabled"`), built here with the maintained
    // `CFString::from_str` (same content, no private symbol). The SDK
    // documents "no way to determine which hotkey ... corresponds to a
    // specific dictionary": entries carry chord + enabled only, never a
    // symbolic ID, and array index is never an ID. Raw Carbon modifiers
    // are returned unstripped; callers strip the automatic Fn bit
    // (`1 << 17`, per Hammerspoon `extensions/hotkey/libhotkey.m`)
    // at match time.

    type CopySymbolicHotKeysFn = unsafe extern "C" fn(*mut RawRef) -> i32;

    static SYMBOLIC_TABLE: OnceLock<Option<CopySymbolicHotKeysFn>> = OnceLock::new();

    fn symbolic_table() -> Option<CopySymbolicHotKeysFn> {
        *SYMBOLIC_TABLE.get_or_init(|| {
            // SAFETY: symbol null-checked before transmuting to the exact
            // SDK signature above; missing fails closed.
            unsafe {
                let handle = libc::dlopen(CARBON_PATH.as_ptr().cast(), 0x1);
                if handle.is_null() {
                    return None;
                }
                let name = CString::new("CopySymbolicHotKeys").expect("static");
                let sym = libc::dlsym(handle, name.as_ptr());
                if sym.is_null() {
                    return None;
                }
                Some(std::mem::transmute::<
                    *mut std::ffi::c_void,
                    CopySymbolicHotKeysFn,
                >(sym))
            }
        })
    }

    /// Read-only effective symbolic-hotkey table. Each entry is the
    /// effective chord (virtual keycode, raw Carbon modifiers) plus its
    /// enabled flag — never a symbolic ID. A missing symbol, OS error, or
    /// malformed container is `Err`, never fabricated chords. At most 512
    /// entries are materialized.
    pub fn copy_symbolic_hotkeys() -> Result<Vec<super::SymbolicHotKeyEntry>, String> {
        use objc2_core_foundation::CFDictionary as CFDictUntyped;
        use objc2_core_foundation::CFDictionary;
        use objc2_core_foundation::{CFArray, CFBoolean, CFNumber, CFString, CFType};
        const MAX_ENTRIES: usize = 512;
        let table = symbolic_table().ok_or_else(|| "CopySymbolicHotKeys unavailable".to_owned())?;
        // SAFETY: resolved non-null symbol with the exact SDK signature;
        // the out-pointer is a live stack local; a nonzero status returns
        // `Err` with no use of the out value. Copy rule: success yields a
        // +1 retained array, wrapped below.
        let mut raw: RawRef = std::ptr::null_mut();
        let status = unsafe { table(std::ptr::addr_of_mut!(raw)) };
        if status != 0 {
            return Err(format!("CopySymbolicHotKeys status {status}"));
        }
        if raw.is_null() {
            return Err("CopySymbolicHotKeys returned null".to_owned());
        }
        let top: CFRetained<CFType> =
            unsafe { CFRetained::from_raw(NonNull::new(raw as *mut CFType).expect("non-null")) };
        let untyped: CFRetained<CFArray> = top
            .downcast()
            .map_err(|_| "CopySymbolicHotKeys returned a non-array container".to_owned())?;
        // SAFETY: top-level kind verified by the downcast above; viewed as
        // `CFArray<CFType>` (layouts identical; all elements CF objects).
        let array: CFRetained<CFArray<CFType>> = unsafe { CFRetained::cast_unchecked(untyped) };
        let code_key = CFString::from_str("kHISymbolicHotKeyCode");
        let mods_key = CFString::from_str("kHISymbolicHotKeyModifiers");
        let enabled_key = CFString::from_str("kHISymbolicHotKeyEnabled");
        let mut out = Vec::new();
        for i in 0..array.len().min(MAX_ENTRIES) {
            let Some(item) = array.get(i) else { continue };
            if item.downcast_ref::<CFDictUntyped>().is_none() {
                continue;
            }
            // SAFETY: kind verified above; entries map `CFString` keys to
            // `CFType` values (same cast as the SLS display parser).
            let dict: CFRetained<CFDictionary<CFString, CFType>> =
                unsafe { CFRetained::cast_unchecked(item.clone()) };
            let (Some(code_val), Some(mods_val), Some(enabled_val)) = (
                dict.get(&code_key),
                dict.get(&mods_key),
                dict.get(&enabled_key),
            ) else {
                continue;
            };
            let (Ok(code_num), Ok(mods_num), Ok(enabled_bool)) = (
                code_val.downcast::<CFNumber>(),
                mods_val.downcast::<CFNumber>(),
                enabled_val.downcast::<CFBoolean>(),
            ) else {
                continue;
            };
            let (Some(code), Some(mods)) = (code_num.as_i64(), mods_num.as_i64()) else {
                continue;
            };
            if code < 0 || mods < 0 {
                continue;
            }
            out.push(super::SymbolicHotKeyEntry {
                keycode: code as u32,
                carbon_modifiers: mods as u32,
                enabled: enabled_bool.as_bool(),
            });
        }
        Ok(out)
    }

    /// macOS major release via the maintained `objc2-foundation`
    /// `NSProcessInfo` binding. `None` when unreadable (fail closed at the
    /// version-coupled gate, never guessed).
    pub fn macos_major() -> Option<u32> {
        use objc2_foundation::NSProcessInfo;
        let version = NSProcessInfo::processInfo().operatingSystemVersion();
        u32::try_from(version.majorVersion).ok()
    }

    // -- Typed AX surface (maintained `objc2` bindings) --

    /// Owned AX element handle. Constructible only through this module's
    /// checked constructors; `Drop` is handled by `CFRetained`. Create and use
    /// the handle on its application's AX thread; do not send foreign handles
    /// across threads. `!Send + !Sync` via `PhantomData<Rc>` (no `unsafe
    /// impl`): the compiler rejects moving a handle to another thread.
    pub struct AxElement {
        inner: CFRetained<AXUIElement>,
        _not_send: std::marker::PhantomData<std::rc::Rc<()>>,
    }

    impl AxElement {
        /// AX element for an application PID. Creation itself never fails
        /// visibly (the API does not validate); failures surface on use.
        /// Note: the maintained binding asserts non-null return; a null
        /// return would panic rather than yield `None`. PIDs are pre-validated
        /// (`pid > 0`) and the OS contract is non-null here.
        pub fn application(pid: i32) -> Option<Self> {
            if pid <= 0 {
                return None;
            }
            // SAFETY: maintained `objc2-application-services` binding; pid
            // validated above; retained object owned by `CFRetained`.
            let inner = unsafe { AXUIElement::new_application(pid) };
            Some(Self {
                inner,
                _not_send: std::marker::PhantomData,
            })
        }

        pub fn system_wide() -> Option<Self> {
            // SAFETY: no-arg creator in the maintained binding; retained
            // object owned by `CFRetained`.
            let inner = unsafe { AXUIElement::new_system_wide() };
            Some(Self {
                inner,
                _not_send: std::marker::PhantomData,
            })
        }

        fn copy_attribute(&self, name: &str) -> Result<CFRetained<CFType>, i32> {
            let attr = CFString::from_str(name);
            let mut value: *const CFType = std::ptr::null();
            // SAFETY: live element + live name from the maintained crate;
            // `value` is a valid out-pointer; nonzero status returns Err with
            // no use of `value`. Copy rule: success yields +1 retained.
            let status = unsafe {
                self.inner.copy_attribute_value(
                    &attr,
                    NonNull::new(std::ptr::addr_of_mut!(value)).expect("out-pointer non-null"),
                )
            };
            if status.0 == 0 {
                NonNull::new(value as *mut CFType)
                    .map(|ptr| unsafe { CFRetained::from_raw(ptr) })
                    .ok_or(AX_NOT_IMPLEMENTED)
            } else {
                Err(status.0)
            }
        }

        /// Window elements of this application element (each retained by the
        /// maintained `CFArray::get`; caller owns each via `AxElement`).
        pub fn windows(&self) -> Result<Vec<AxElement>, i32> {
            let value = self.copy_attribute("AXWindows")?;
            let untyped: CFRetained<CFArray> = value.downcast().map_err(|_| AX_NOT_IMPLEMENTED)?;
            // SAFETY: `AXWindows` is documented to be an array of
            // `AXUIElement`; `CFArray` layout is identical across element
            // types, and elements are accessed via the checked `get`.
            let array: CFRetained<CFArray<AXUIElement>> =
                unsafe { CFRetained::cast_unchecked(untyped) };
            let count = array.len().min(crate::config::MAX_WINDOWS);
            let mut out = Vec::with_capacity(count);
            for i in 0..count {
                if let Some(elem) = array.get(i) {
                    out.push(AxElement {
                        inner: elem,
                        _not_send: std::marker::PhantomData,
                    });
                }
            }
            Ok(out)
        }
    }

    /// Whether this process is trusted for accessibility (consent state).
    pub fn ax_is_process_trusted() -> Option<bool> {
        // SAFETY: no-arg getter in the maintained binding, no pointers.
        Some(unsafe { objc2_application_services::AXIsProcessTrusted() })
    }

    /// Best-effort per-element messaging timeout (seconds). Returns the raw
    /// AX status; callers record rather than assume it.
    pub fn ax_set_messaging_timeout(element: &AxElement, seconds: f32) -> i32 {
        // SAFETY: live element from a checked constructor, plain float, in
        // the maintained binding.
        unsafe { element.inner.set_messaging_timeout(seconds) }.0
    }

    /// Identity join: CG window ID for an AX window element (private SPI kept
    /// in `private_raw`; missing/failing join is `None`, never guessed).
    pub fn ax_window_id(element: &AxElement) -> Option<u32> {
        ax_window_id_detailed(element).ok()
    }

    /// Identity join with cause: CG window ID for an AX window element.
    /// `Err` names the `_AXUIElementGetWindow` status instead of `None`, so
    /// callers report the real join failure rather than guessing wedged-OS.
    pub fn ax_window_id_detailed(element: &AxElement) -> Result<u32, String> {
        let mut wid: u32 = 0;
        let ptr = CFRetained::as_ptr(&element.inner).as_ptr() as *const std::ffi::c_void;
        // SAFETY: live element from a checked constructor; valid out-pointer;
        // status gates the read.
        let err = unsafe { private_raw::_AXUIElementGetWindow(ptr, std::ptr::addr_of_mut!(wid)) };
        if err == 0 {
            Ok(wid)
        } else {
            Err(format!("_AXUIElementGetWindow status {err}"))
        }
    }

    /// Exact focus preimage: the currently focused application PID plus its
    /// focused window ID (when queryable). `None` means the preimage could
    /// not be proven (no consent or no focused window): callers must fail
    /// closed before changing focus rather than steal it blindly.
    pub fn ax_focused_preimage() -> Option<(i32, Option<u32>)> {
        ax_focused_preimage_detailed().ok()
    }

    /// Detailed focus preimage with the real AX cause on every leg: each
    /// failure names its `AXError` instead of collapsing to `None`, so
    /// callers never interpret an unreadable preimage as a wedged OS. A
    /// genuinely windowless frontmost (`kAXErrorNoValue` on
    /// `AXFocusedWindow`, e.g. a launcher with no key window) is
    /// `Ok((pid, None))` — the caller still fails closed, but with the
    /// windowless cause named instead of a guess. Both the system-wide
    /// element and the focused application get a finite messaging timeout
    /// before any read (a hung app stalls at most one slice past it); a
    /// timeout that refuses to set is itself an honest `Err`, never
    /// silently ignored.
    pub fn ax_focused_preimage_detailed() -> Result<(i32, Option<u32>), String> {
        let system = AxElement::system_wide()
            .ok_or_else(|| "system-wide AX element unavailable".to_owned())?;
        let timeout_status = ax_set_messaging_timeout(&system, 2.0);
        if timeout_status != 0 {
            return Err(format!(
                "system-wide AX messaging timeout unset: {}",
                ax_error_name(timeout_status)
            ));
        }
        let app_value = system
            .copy_attribute("AXFocusedApplication")
            .map_err(|code| format!("AXFocusedApplication unreadable: {}", ax_error_name(code)))?;
        let app_inner: CFRetained<AXUIElement> = app_value
            .downcast()
            .map_err(|_| "AXFocusedApplication is not an AXUIElement".to_owned())?;
        let app = AxElement {
            inner: app_inner,
            _not_send: std::marker::PhantomData,
        };
        let timeout_status = ax_set_messaging_timeout(&app, 2.0);
        if timeout_status != 0 {
            return Err(format!(
                "focused application AX messaging timeout unset: {}",
                ax_error_name(timeout_status)
            ));
        }
        let mut pid: libc::pid_t = 0;
        // SAFETY: live element; valid out-pointer; status gates the read.
        let err = unsafe {
            app.inner
                .pid(NonNull::new(std::ptr::addr_of_mut!(pid)).expect("non-null"))
        };
        if err.0 != 0 {
            return Err(format!(
                "focused application pid unreadable: {}",
                ax_error_name(err.0)
            ));
        }
        if pid <= 1 {
            return Err(format!(
                "focused application pid {pid} is reserved; refusing"
            ));
        }
        match app.copy_attribute("AXFocusedWindow") {
            Ok(window_value) => {
                let window_inner: CFRetained<AXUIElement> = window_value
                    .downcast()
                    .map_err(|_| "AXFocusedWindow is not an AXUIElement".to_owned())?;
                let window = AxElement {
                    inner: window_inner,
                    _not_send: std::marker::PhantomData,
                };
                match ax_window_id_detailed(&window) {
                    Ok(wid) => Ok((pid as i32, Some(wid))),
                    Err(e) => Err(format!("AXFocusedWindow join: {e}")),
                }
            }
            // `kAXErrorNoValue`: frontmost has no focused window
            // (windowless launcher/background). Named, still caller-fails-closed.
            Err(AX_NO_VALUE) => Ok((pid as i32, None)),
            Err(code) => Err(format!(
                "AXFocusedWindow unreadable: {}",
                ax_error_name(code)
            )),
        }
    }

    /// Whether the calling thread has queried a current run loop. Read-only
    /// observation for the focus-diagnose main-vs-worker comparison.
    /// `CFRunLoop::current` is created-on-demand, so this reports
    /// query-presence only — never pumped/live acceptance, never created,
    /// started, or modified beyond the query itself.
    pub fn runloop_present() -> bool {
        CFRunLoop::current().is_some()
    }

    /// Shared `AXFrontmost` read for the canonical acquisition order
    /// (frontmost-before, window legs, frontmost-after). One safe reader
    /// reused by the diagnose direct leg and the experimental fallback so
    /// the two never diverge in acquisition. No timeout set here: the
    /// caller sets the per-element messaging timeout before any join.
    fn ax_frontmost_read(app: &AxElement) -> (String, Option<bool>) {
        match ax_bool_attribute(app, "AXFrontmost") {
            Ok(v) => (
                if v {
                    "kAXErrorSuccess-true".to_owned()
                } else {
                    "kAXErrorSuccess-false".to_owned()
                },
                Some(v),
            ),
            Err(code) => (ax_error_name(code), None),
        }
    }

    /// Window join with its own per-element messaging timeout first. A
    /// refused timeout is an honest `Err` and the join never runs
    /// unbounded. Shared by the system leg, the direct app leg
    /// (focused/main), and the experimental fallback candidate.
    fn ax_window_join_with_timeout(window: &AxElement) -> Result<u32, String> {
        let st = ax_set_messaging_timeout(window, 2.0);
        if st != 0 {
            return Err(format!(
                "window messaging timeout unset: {}",
                ax_error_name(st)
            ));
        }
        ax_window_id_detailed(window)
    }

    /// Focused/main single-window join for the direct app leg: attribute
    /// read, downcast, then the per-element timeout plus join above. A
    /// refused window timeout stops that leg with its cause, never an
    /// unbounded join.
    fn ax_single_window_join(app: &AxElement, attr: &str) -> (String, Option<u32>) {
        match app.copy_attribute(attr) {
            Ok(v) => match v.downcast::<AXUIElement>() {
                Ok(inner) => {
                    let window = AxElement {
                        inner,
                        _not_send: std::marker::PhantomData,
                    };
                    match ax_window_join_with_timeout(&window) {
                        Ok(wid) => ("kAXErrorSuccess".to_owned(), Some(wid)),
                        Err(e) => (e, None),
                    }
                }
                Err(_) => ("not-an-AXUIElement".to_owned(), None),
            },
            Err(code) => (ax_error_name(code), None),
        }
    }

    /// Read-only system-wide focus leg with per-leg statuses: the same
    /// `AXFocusedApplication` route as [`ax_focused_preimage_detailed`], but
    /// every leg reports its own status instead of failing the whole read
    /// at the first error. Nonmutating except the two finite messaging
    /// timeouts (2s, as in the preimage); never writes attributes or
    /// settings, never fabricates a PID. IDs and status names only.
    pub fn ax_system_focus_leg() -> super::AxSystemFocusLeg {
        let Some(system) = AxElement::system_wide() else {
            return super::AxSystemFocusLeg {
                system_element: "unavailable".to_owned(),
                system_timeout_status: None,
                system_timeout_name: None,
                focused_app_status: "system-wide AX element unavailable".to_owned(),
                focused_pid: None,
                app_timeout_status: None,
                app_timeout_name: None,
                app_pid_status: "unreached".to_owned(),
                focused_window_status: "unreached".to_owned(),
                focused_wid: None,
            };
        };
        let timeout_status = ax_set_messaging_timeout(&system, 2.0);
        let timeout_name = ax_error_name(timeout_status);
        if timeout_status != 0 {
            return super::AxSystemFocusLeg {
                system_element: "ok".to_owned(),
                system_timeout_status: Some(timeout_status),
                system_timeout_name: Some(timeout_name),
                focused_app_status: "unreached (timeout unset)".to_owned(),
                focused_pid: None,
                app_timeout_status: None,
                app_timeout_name: None,
                app_pid_status: "unreached".to_owned(),
                focused_window_status: "unreached".to_owned(),
                focused_wid: None,
            };
        }
        let app_value = match system.copy_attribute("AXFocusedApplication") {
            Ok(v) => v,
            Err(code) => {
                return super::AxSystemFocusLeg {
                    system_element: "ok".to_owned(),
                    system_timeout_status: Some(timeout_status),
                    system_timeout_name: Some(timeout_name),
                    focused_app_status: ax_error_name(code),
                    focused_pid: None,
                    app_timeout_status: None,
                    app_timeout_name: None,
                    app_pid_status: "unreached".to_owned(),
                    focused_window_status: "unreached".to_owned(),
                    focused_wid: None,
                };
            }
        };
        let app_inner: CFRetained<AXUIElement> = match app_value.downcast() {
            Ok(v) => v,
            Err(_) => {
                return super::AxSystemFocusLeg {
                    system_element: "ok".to_owned(),
                    system_timeout_status: Some(timeout_status),
                    system_timeout_name: Some(timeout_name),
                    focused_app_status: "not-an-AXUIElement".to_owned(),
                    focused_pid: None,
                    app_timeout_status: None,
                    app_timeout_name: None,
                    app_pid_status: "unreached".to_owned(),
                    focused_window_status: "unreached".to_owned(),
                    focused_wid: None,
                };
            }
        };
        let app = AxElement {
            inner: app_inner,
            _not_send: std::marker::PhantomData,
        };
        let app_timeout = ax_set_messaging_timeout(&app, 2.0);
        let app_timeout_name = ax_error_name(app_timeout);
        if app_timeout != 0 {
            return super::AxSystemFocusLeg {
                system_element: "ok".to_owned(),
                system_timeout_status: Some(timeout_status),
                system_timeout_name: Some(timeout_name),
                focused_app_status: "kAXErrorSuccess".to_owned(),
                focused_pid: None,
                app_timeout_status: Some(app_timeout),
                app_timeout_name: Some(app_timeout_name),
                app_pid_status: "unreached (timeout unset)".to_owned(),
                focused_window_status: "unreached".to_owned(),
                focused_wid: None,
            };
        }
        let mut pid: libc::pid_t = 0;
        // SAFETY: live element; valid out-pointer; status gates the read.
        let pid_err = unsafe {
            app.inner
                .pid(NonNull::new(std::ptr::addr_of_mut!(pid)).expect("non-null"))
        };
        if pid_err.0 != 0 {
            return super::AxSystemFocusLeg {
                system_element: "ok".to_owned(),
                system_timeout_status: Some(timeout_status),
                system_timeout_name: Some(timeout_name),
                focused_app_status: "kAXErrorSuccess".to_owned(),
                focused_pid: None,
                app_timeout_status: Some(app_timeout),
                app_timeout_name: Some(app_timeout_name),
                app_pid_status: ax_error_name(pid_err.0),
                focused_window_status: "unreached".to_owned(),
                focused_wid: None,
            };
        }
        if pid <= 1 {
            return super::AxSystemFocusLeg {
                system_element: "ok".to_owned(),
                system_timeout_status: Some(timeout_status),
                system_timeout_name: Some(timeout_name),
                focused_app_status: "kAXErrorSuccess".to_owned(),
                focused_pid: Some(pid as i32),
                app_timeout_status: Some(app_timeout),
                app_timeout_name: Some(app_timeout_name),
                app_pid_status: "reserved-pid".to_owned(),
                focused_window_status: "unreached".to_owned(),
                focused_wid: None,
            };
        }
        match app.copy_attribute("AXFocusedWindow") {
            Ok(window_value) => {
                let window_inner: CFRetained<AXUIElement> = match window_value.downcast() {
                    Ok(v) => v,
                    Err(_) => {
                        return super::AxSystemFocusLeg {
                            system_element: "ok".to_owned(),
                            system_timeout_status: Some(timeout_status),
                            system_timeout_name: Some(timeout_name),
                            focused_app_status: "kAXErrorSuccess".to_owned(),
                            focused_pid: Some(pid as i32),
                            app_timeout_status: Some(app_timeout),
                            app_timeout_name: Some(app_timeout_name),
                            app_pid_status: "kAXErrorSuccess".to_owned(),
                            focused_window_status: "not-an-AXUIElement".to_owned(),
                            focused_wid: None,
                        };
                    }
                };
                let window = AxElement {
                    inner: window_inner,
                    _not_send: std::marker::PhantomData,
                };
                // Per-element messaging timeout precedes the join: a
                // refused timeout stops this leg with its cause, never an
                // unbounded join.
                match ax_window_join_with_timeout(&window) {
                    Ok(wid) => super::AxSystemFocusLeg {
                        system_element: "ok".to_owned(),
                        system_timeout_status: Some(timeout_status),
                        system_timeout_name: Some(timeout_name),
                        focused_app_status: "kAXErrorSuccess".to_owned(),
                        focused_pid: Some(pid as i32),
                        app_timeout_status: Some(app_timeout),
                        app_timeout_name: Some(app_timeout_name),
                        app_pid_status: "kAXErrorSuccess".to_owned(),
                        focused_window_status: "kAXErrorSuccess".to_owned(),
                        focused_wid: Some(wid),
                    },
                    Err(e) => super::AxSystemFocusLeg {
                        system_element: "ok".to_owned(),
                        system_timeout_status: Some(timeout_status),
                        system_timeout_name: Some(timeout_name),
                        focused_app_status: "kAXErrorSuccess".to_owned(),
                        focused_pid: Some(pid as i32),
                        app_timeout_status: Some(app_timeout),
                        app_timeout_name: Some(app_timeout_name),
                        app_pid_status: "kAXErrorSuccess".to_owned(),
                        focused_window_status: e,
                        focused_wid: None,
                    },
                }
            }
            Err(AX_NO_VALUE) => super::AxSystemFocusLeg {
                system_element: "ok".to_owned(),
                system_timeout_status: Some(timeout_status),
                system_timeout_name: Some(timeout_name),
                focused_app_status: "kAXErrorSuccess".to_owned(),
                focused_pid: Some(pid as i32),
                app_timeout_status: Some(app_timeout),
                app_timeout_name: Some(app_timeout_name),
                app_pid_status: "kAXErrorSuccess".to_owned(),
                focused_window_status: "kAXErrorNoValue".to_owned(),
                focused_wid: None,
            },
            Err(code) => super::AxSystemFocusLeg {
                system_element: "ok".to_owned(),
                system_timeout_status: Some(timeout_status),
                system_timeout_name: Some(timeout_name),
                focused_app_status: "kAXErrorSuccess".to_owned(),
                focused_pid: Some(pid as i32),
                app_timeout_status: Some(app_timeout),
                app_timeout_name: Some(app_timeout_name),
                app_pid_status: "kAXErrorSuccess".to_owned(),
                focused_window_status: ax_error_name(code),
                focused_wid: None,
            },
        }
    }

    /// Read-only direct per-application focus leg for exactly `pid`: the
    /// independent route that bypasses the system-wide `AXFocusedApplication`
    /// read (`AXUIElementCreateApplication` + `AXWindows` count +
    /// `AXFocusedWindow` / `AXMainWindow` joins plus the `AXFrontmost` pair).
    /// Canonical acquisition is frontmost-before, window legs,
    /// frontmost-after, shared with the experimental fallback via
    /// [`ax_frontmost_read`]. Every `AxElement` lives and dies on the
    /// calling thread. Nonmutating except the finite messaging timeouts
    /// (2s); never writes attributes. IDs, counts, and status names only.
    /// Bounded: windows contribute a count only (no every-window joins),
    /// so the worker bound never pays a per-window worst case.
    pub fn ax_app_focus_leg(pid: i32) -> super::AxAppFocusLeg {
        if pid <= 1 {
            return super::AxAppFocusLeg {
                pid,
                element: "refused-reserved-pid".to_owned(),
                timeout_status: None,
                timeout_name: None,
                windows_status: "unreached".to_owned(),
                windows_count: None,
                window_join_failures: 0,
                joined_wids: Vec::new(),
                focused_status: "unreached".to_owned(),
                focused_wid: None,
                main_status: "unreached".to_owned(),
                main_wid: None,
                frontmost_before_status: "unreached".to_owned(),
                frontmost_before: None,
                frontmost_after_status: "unreached".to_owned(),
                frontmost_after: None,
            };
        }
        let Some(app) = AxElement::application(pid) else {
            return super::AxAppFocusLeg {
                pid,
                element: "unavailable".to_owned(),
                timeout_status: None,
                timeout_name: None,
                windows_status: "unreached".to_owned(),
                windows_count: None,
                window_join_failures: 0,
                joined_wids: Vec::new(),
                focused_status: "unreached".to_owned(),
                focused_wid: None,
                main_status: "unreached".to_owned(),
                main_wid: None,
                frontmost_before_status: "unreached".to_owned(),
                frontmost_before: None,
                frontmost_after_status: "unreached".to_owned(),
                frontmost_after: None,
            };
        };
        let timeout_status = ax_set_messaging_timeout(&app, 2.0);
        let timeout_name = ax_error_name(timeout_status);
        if timeout_status != 0 {
            return super::AxAppFocusLeg {
                pid,
                element: "ok".to_owned(),
                timeout_status: Some(timeout_status),
                timeout_name: Some(timeout_name),
                windows_status: "unreached (timeout unset)".to_owned(),
                windows_count: None,
                window_join_failures: 0,
                joined_wids: Vec::new(),
                focused_status: "unreached".to_owned(),
                focused_wid: None,
                main_status: "unreached".to_owned(),
                main_wid: None,
                frontmost_before_status: "unreached".to_owned(),
                frontmost_before: None,
                frontmost_after_status: "unreached".to_owned(),
                frontmost_after: None,
            };
        }
        // Canonical order: frontmost BEFORE the window legs.
        let (frontmost_before_status, frontmost_before) = ax_frontmost_read(&app);
        // Windows contribute a count only (no every-window joins): the
        // app timeout above already bounds this read.
        let (windows_status, windows_count) = match app.windows() {
            Ok(list) => ("kAXErrorSuccess".to_owned(), Some(list.len())),
            Err(code) => (ax_error_name(code), None),
        };
        // Focused/main joins each carry their own per-element window
        // timeout inside [`ax_single_window_join`].
        let (focused_status, focused_wid) = ax_single_window_join(&app, "AXFocusedWindow");
        let (main_status, main_wid) = ax_single_window_join(&app, "AXMainWindow");
        // Canonical order: frontmost AFTER the window legs; the pair must
        // both read true/stable for any corroborated candidate.
        let (frontmost_after_status, frontmost_after) = ax_frontmost_read(&app);
        super::AxAppFocusLeg {
            pid,
            element: "ok".to_owned(),
            timeout_status: Some(timeout_status),
            timeout_name: Some(timeout_name),
            windows_status,
            windows_count,
            window_join_failures: 0,
            joined_wids: Vec::new(),
            focused_status,
            focused_wid,
            main_status,
            main_wid,
            frontmost_before_status,
            frontmost_before,
            frontmost_after_status,
            frontmost_after,
        }
    }

    /// Window frame in screen points.
    #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    pub struct AxFrame {
        pub x: f64,
        pub y: f64,
        pub w: f64,
        pub h: f64,
    }

    impl AxFrame {
        pub fn valid_for_set(&self) -> bool {
            self.x.is_finite()
                && self.y.is_finite()
                && self.w.is_finite()
                && self.h.is_finite()
                && self.w > 0.0
                && self.h > 0.0
        }
    }

    pub fn ax_get_frame(element: &AxElement) -> Result<AxFrame, i32> {
        let pos_value = element.copy_attribute("AXPosition")?;
        // `CFRetained` drops both values on every path; no manual release.
        let size_value = element.copy_attribute("AXSize")?;
        let pos: CFRetained<AXValue> = pos_value.downcast().map_err(|_| AX_NOT_IMPLEMENTED)?;
        let size: CFRetained<AXValue> = size_value.downcast().map_err(|_| AX_NOT_IMPLEMENTED)?;
        let mut point = CGPoint { x: 0.0, y: 0.0 };
        let mut size_s = CGSize {
            width: 0.0,
            height: 0.0,
        };
        // SAFETY: both values live in maintained wrappers; out-pointers are
        // locals; boolean results gate use.
        let ok_pos = unsafe {
            pos.value(
                AXValueType::CGPoint,
                NonNull::new(std::ptr::addr_of_mut!(point) as *mut std::ffi::c_void)
                    .expect("non-null"),
            )
        };
        let ok_size = unsafe {
            size.value(
                AXValueType::CGSize,
                NonNull::new(std::ptr::addr_of_mut!(size_s) as *mut std::ffi::c_void)
                    .expect("non-null"),
            )
        };
        if ok_pos && ok_size {
            Ok(AxFrame {
                x: point.x,
                y: point.y,
                w: size_s.width,
                h: size_s.height,
            })
        } else {
            Err(AX_NOT_IMPLEMENTED)
        }
    }

    /// Position leg of a frame write. Split from [`ax_set_frame`] so the
    /// partial-write hazard (position accepted, size refused) is explicit at
    /// the boundary: callers must restore the preimage after ANY failed
    /// frame write, never treat the first leg's acceptance as success.
    pub fn ax_set_position(element: &AxElement, frame: &AxFrame) -> Result<(), i32> {
        if !frame.valid_for_set() {
            return Err(AX_ILLEGAL_ARGUMENT);
        }
        let pos_name = CFString::from_str("AXPosition");
        let point = CGPoint {
            x: frame.x,
            y: frame.y,
        };
        // SAFETY: stack point lives for the call; maintained `AXValue::new`
        // copies it into a retained object; null means allocation failure.
        let pos_value = unsafe {
            AXValue::new(
                AXValueType::CGPoint,
                NonNull::new(std::ptr::addr_of!(point) as *mut std::ffi::c_void).expect("non-null"),
            )
        }
        .ok_or(AX_FAILURE)?;
        // SAFETY: live element/name/value in maintained wrappers; status is
        // the only effect read. `Deref` chains `&AXValue` to `&CFType`.
        let status = unsafe { element.inner.set_attribute_value(&pos_name, &pos_value) }.0;
        if status == 0 { Ok(()) } else { Err(status) }
    }

    /// Size leg of a frame write. See [`ax_set_position`]: a size refusal
    /// after an accepted position write is a partial mutation, never a clean
    /// no-op — the caller must restore.
    pub fn ax_set_size(element: &AxElement, frame: &AxFrame) -> Result<(), i32> {
        if !frame.valid_for_set() {
            return Err(AX_ILLEGAL_ARGUMENT);
        }
        let size_name = CFString::from_str("AXSize");
        let size = CGSize {
            width: frame.w,
            height: frame.h,
        };
        // SAFETY: stack size lives for the call; maintained `AXValue::new`
        // copies it into a retained object; null means allocation failure.
        let size_value = unsafe {
            AXValue::new(
                AXValueType::CGSize,
                NonNull::new(std::ptr::addr_of!(size) as *mut std::ffi::c_void).expect("non-null"),
            )
        }
        .ok_or(AX_FAILURE)?;
        // SAFETY: live element/name/value in maintained wrappers; status is
        // the only effect read.
        let status = unsafe { element.inner.set_attribute_value(&size_name, &size_value) }.0;
        if status == 0 { Ok(()) } else { Err(status) }
    }

    pub fn ax_set_frame(element: &AxElement, frame: &AxFrame) -> Result<(), i32> {
        // Ordered legs: position first, then size. A size refusal after an
        // accepted position write leaves the window partially mutated; the
        // `Err` return carries that hazard to the caller, which must attempt
        // preimage restoration on EVERY error path after this call.
        ax_set_position(element, frame)?;
        ax_set_size(element, frame)
    }

    /// Owned-window title (`AXTitle`) for the nonce-title identity fallback.
    /// `Ok(None)` means the attribute was present but not a usable title
    /// (missing/non-string/empty); `Err(code)` means the read itself failed.
    /// Callers compare the exact expected nonce title and never log titles.
    pub fn ax_window_title(element: &AxElement) -> Result<Option<String>, i32> {
        let value = element.copy_attribute("AXTitle")?;
        match value.downcast::<CFString>() {
            Ok(s) => {
                let title = s.to_string();
                if title.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(title))
                }
            }
            Err(_) => Ok(None),
        }
    }

    /// Non-prompting synthetic-input preflight: whether the window server
    /// would accept posted events from this process
    /// (`CGPreflightPostEventAccess` via the maintained
    /// `objc2-core-graphics` binding). Preflight only — never requests access
    /// and never posts. Always `Some` on macOS (the symbol links at build
    /// time); `None` off-macOS (stub reports no state).
    pub fn cg_post_event_access_known() -> Option<bool> {
        Some(objc2_core_graphics::CGPreflightPostEventAccess())
    }

    pub fn ax_raise(element: &AxElement) -> Result<(), i32> {
        let action = CFString::from_str("AXRaise");
        // SAFETY: live element + action name in the maintained binding;
        // status is the only effect read.
        let status = unsafe { element.inner.perform_action(&action) }.0;
        if status == 0 { Ok(()) } else { Err(status) }
    }

    /// Set the application's focused window to exactly `window` (public
    /// `AXFocusedWindow` attribute write on the application element). This
    /// is the documented AX way to move key-window focus inside one app;
    /// it never touches other apps, the window server graces nothing, and
    /// arrival is still verified separately by the focus agreement oracle.
    /// Callers join `window` to the owned PID first and revalidate identity
    /// around the write; a refused write is `Err` status, never assumed.
    pub fn ax_set_focused_window(app: &AxElement, window: &AxElement) -> Result<(), i32> {
        let name = CFString::from_str("AXFocusedWindow");
        // SAFETY: live app/window elements from checked constructors plus a
        // live name in the maintained binding; status is the only effect
        // read. `Deref` chains `&AXUIElement` to `&CFType` (same chain the
        // position/size legs use for `&AXValue`).
        let status = unsafe { app.inner.set_attribute_value(&name, &window.inner) }.0;
        if status == 0 { Ok(()) } else { Err(status) }
    }

    // -- Mission Control AX traversal (public AX on the Dock, no synthetic
    // input, no private SPI) --
    //
    // Shape pinned against the local Hammerspoon checkout
    // (`extensions/spaces/spaces.lua`: Dock application element, then
    // `AXIdentifier == "mc"` group, then `AXChildren` into `"mc.display"`
    // (matched on `AXDisplayID`), then `"mc.spaces"`, then `"mc.spaces.add"`
    // (`doAXPress`) or `"mc.spaces.list"[index]` (`AXPress` to go to a
    // Space, `AXRemoveDesktop` to remove one)). Opens/closes ride the public
    // `NSWorkspace` application toggle below, not Hammerspoon's private
    // `CoreDockSendNotification("com.apple.expose.awake")` nor yabai's
    // `CoreDockSendNotification(CFSTR("com.apple.expose.awake"), 0)`
    // (`src/space_manager.c` `space_manager_toggle_mission_control`).
    // Source: https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/spaces.lua#L84-L152
    // and https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L247-L251
    // Every read below is bounded by the caller's per-element messaging
    // timeout (set before traversal); a refused timeout stops the leg, never
    // an unbounded walk. No titles, no content: identifiers and IDs only.

    /// `AXChildren` of one AX element as owned handles. The top-level kind
    /// is verified as `CFArray`; each entry is then checked-downcast from
    /// `CFType` to `AXUIElement` individually — the private Dock AX tree is
    /// never trusted element-wise, and a non-element entry fails the leg.
    /// An over-long array fails too: callers must never index into a
    /// silently truncated tree.
    pub fn ax_children(element: &AxElement) -> Result<Vec<AxElement>, i32> {
        const MAX_MC_CHILDREN: usize = 256;
        let value = element.copy_attribute("AXChildren")?;
        let untyped: CFRetained<CFArray> = value.downcast().map_err(|_| AX_NOT_IMPLEMENTED)?;
        if untyped.len() > MAX_MC_CHILDREN {
            return Err(AX_NOT_IMPLEMENTED);
        }
        // SAFETY: top-level kind verified as `CFArray` by the downcast
        // above; viewed as `CFArray<CFType>` (every CF object is a `CFType`;
        // layouts identical). Entries are checked-downcast below.
        let array: CFRetained<CFArray<CFType>> = unsafe { CFRetained::cast_unchecked(untyped) };
        let mut out = Vec::with_capacity(array.len());
        for i in 0..array.len() {
            let item = array.get(i).ok_or(AX_NOT_IMPLEMENTED)?;
            let elem: CFRetained<AXUIElement> = item.downcast().map_err(|_| AX_NOT_IMPLEMENTED)?;
            out.push(AxElement {
                inner: elem,
                _not_send: std::marker::PhantomData,
            });
        }
        Ok(out)
    }

    /// `AXIdentifier` string of one AX element. `Ok(None)` is honest unknown
    /// (missing/non-string/empty, or the element does not support the
    /// attribute at all); `Err` is a real read failure. Missing means the
    /// element carries no comparable identifier — callers scanning for one
    /// skip it, never match on it.
    pub fn ax_identifier(element: &AxElement) -> Result<Option<String>, i32> {
        match element.copy_attribute("AXIdentifier") {
            Err(AX_NO_VALUE) | Err(AX_ATTRIBUTE_UNSUPPORTED) => Ok(None),
            Err(code) => Err(code),
            Ok(value) => match value.downcast::<CFString>() {
                Ok(s) => {
                    let id = s.to_string();
                    if id.is_empty() {
                        Ok(None)
                    } else {
                        Ok(Some(id))
                    }
                }
                Err(_) => Ok(None),
            },
        }
    }

    /// `AXDisplayID` numeric read (the `mc.display` match leg: compared
    /// against the CG display ID joined via [`ns_display_snapshot`]).
    /// `Ok(None)` is honest unknown (missing/non-numeric, or the element
    /// does not support the attribute at all); `Err` is a real read failure.
    pub fn ax_display_id(element: &AxElement) -> Result<Option<i64>, i32> {
        match element.copy_attribute("AXDisplayID") {
            Err(AX_NO_VALUE) | Err(AX_ATTRIBUTE_UNSUPPORTED) => Ok(None),
            Err(code) => Err(code),
            Ok(value) => match value.downcast::<CFNumber>() {
                Ok(n) => Ok(n.as_i64()),
                Err(_) => Ok(None),
            },
        }
    }

    /// One named AX action (`AXPress`, `AXRemoveDesktop`) on one already
    /// joined element. The status is the only effect read: callers must read
    /// back Mission Control presence/topology separately — an accepted status
    /// is never state proof. Scoped to the Mission Control probe path
    /// ([`crate::mc_probe`]), which joins every target through the Dock PID
    /// plus fresh per-action SLS/AX joins; no other caller may adopt this
    /// for foreign elements.
    pub fn ax_perform_action(element: &AxElement, name: &str) -> Result<(), i32> {
        let action = CFString::from_str(name);
        // SAFETY: live element + action name in the maintained binding;
        // status is the only effect read.
        let status = unsafe { element.inner.perform_action(&action) }.0;
        if status == 0 { Ok(()) } else { Err(status) }
    }

    struct ObserverRefcon {
        tx: SyncSender<EngineMessage>,
        pid: u32,
        received: Arc<AtomicU32>,
    }

    unsafe extern "C-unwind" fn ax_observer_callback(
        _observer: NonNull<AXObserver>,
        _element: NonNull<AXUIElement>,
        _notification: NonNull<CFString>,
        refcon: *mut std::ffi::c_void,
    ) {
        // SAFETY: `refcon` is the live `Box<ObserverRefcon>` installed below,
        // reclaimed only after notifications are removed and the observer is
        // released. Minimal work: bump + non-blocking send. No panics.
        if refcon.is_null() {
            return;
        }
        let state = unsafe { &*(refcon as *const ObserverRefcon) };
        state.received.fetch_add(1, Ordering::SeqCst);
        let _ = state.tx.try_send(EngineMessage::AxEvent { pid: state.pid });
    }

    /// Bounded per-app AX observation on the calling thread's runloop.
    /// Registers window-created + focused-window-changed notifications with a
    /// finite messaging timeout, pumps the runloop until `timeout`, then
    /// tears everything down. A hung app stalls at most one slice past its
    /// messaging timeout; failures (bad PID, no consent) return fast.
    pub fn ax_observe_bounded(
        pid: i32,
        timeout: Duration,
        messaging_timeout_secs: f32,
        tx: &SyncSender<EngineMessage>,
    ) -> Result<AxObserveOutcome, AxObserveError> {
        if pid <= 1 {
            return Err(AxObserveError::InvalidPid(format!(
                "refusing reserved pid {pid}"
            )));
        }
        let app = AxElement::application(pid)
            .ok_or_else(|| AxObserveError::InvalidPid(format!("bad pid {pid}")))?;
        let messaging_timeout_set = ax_set_messaging_timeout(&app, messaging_timeout_secs) == 0;
        let mut observer_raw: *mut AXObserver = std::ptr::null_mut();
        // SAFETY: out-pointer valid; maintained `AXObserver::create`; nonzero
        // status leaves `observer_raw` null and returns fast.
        let status = unsafe {
            AXObserver::create(
                pid,
                Some(ax_observer_callback),
                NonNull::new(std::ptr::addr_of_mut!(observer_raw)).expect("non-null"),
            )
        };
        if status.0 != 0 || observer_raw.is_null() {
            return Err(AxObserveError::System(format!(
                "AXObserverCreate pid {pid}: {}",
                ax_error_name(status.0)
            )));
        }
        // SAFETY: create-rule +1 retained; owned by `CFRetained` exactly once.
        let observer: CFRetained<AXObserver> =
            unsafe { CFRetained::from_raw(NonNull::new(observer_raw).expect("non-null")) };
        let received = Arc::new(AtomicU32::new(0));
        let refcon = Box::new(ObserverRefcon {
            tx: tx.clone(),
            pid: pid as u32,
            received: received.clone(),
        });
        let refcon_ptr = Box::into_raw(refcon);
        let mut added: u32 = 0;
        // SAFETY: observer/app live in maintained wrappers; names are fresh
        // `CFString`s; refcon valid until reclaimed below after notifications
        // are removed.
        unsafe {
            for name in ["AXWindowCreated", "AXFocusedWindowChanged"] {
                let cf_name = CFString::from_str(name);
                let status = observer.add_notification(
                    &app.inner,
                    &cf_name,
                    refcon_ptr as *mut std::ffi::c_void,
                );
                if status.0 == 0 {
                    added += 1;
                }
            }
            if added == 0 {
                drop(Box::from_raw(refcon_ptr));
                return Err(AxObserveError::System(
                    "no AX notifications registered (bad pid or no consent)".to_owned(),
                ));
            }
            // Note: the maintained `run_loop_source` asserts non-null and
            // would panic on null; a null source here would mean an OS
            // contract violation (live observer must vend a source).
            let source = observer.run_loop_source();
            let rl = CFRunLoop::current().ok_or_else(|| {
                for name in ["AXWindowCreated", "AXFocusedWindowChanged"] {
                    let cf_name = CFString::from_str(name);
                    let _ = observer.remove_notification(&app.inner, &cf_name);
                }
                drop(Box::from_raw(refcon_ptr));
                AxObserveError::System("AX observer runloop unavailable".to_owned())
            })?;
            let mode = kCFRunLoopDefaultMode;
            rl.add_source(Some(&source), mode);
            let deadline = Instant::now() + timeout;
            while Instant::now() < deadline {
                CFRunLoop::run_in_mode(mode, 0.25, false);
            }
            rl.remove_source(Some(&source), mode);
            source.invalidate();
            for name in ["AXWindowCreated", "AXFocusedWindowChanged"] {
                let cf_name = CFString::from_str(name);
                let _ = observer.remove_notification(&app.inner, &cf_name);
            }
            drop(Box::from_raw(refcon_ptr));
        }
        Ok(AxObserveOutcome {
            notifications_added: added,
            notifications_received: received.load(Ordering::SeqCst),
            messaging_timeout_set,
        })
    }

    struct SessionRefcon {
        tx: SyncSender<EngineMessage>,
        pid: u32,
        counts: std::sync::Mutex<std::collections::HashMap<String, u32>>,
    }

    unsafe extern "C-unwind" fn ax_session_callback(
        _observer: NonNull<AXObserver>,
        _element: NonNull<AXUIElement>,
        notification: NonNull<CFString>,
        refcon: *mut std::ffi::c_void,
    ) {
        // SAFETY: `refcon` is the live `Box<SessionRefcon>` installed below,
        // reclaimed only after notifications are removed and the observer is
        // released. `notification` is borrowed (no ownership transfer); only
        // its string copy leaves. Minimal work: count + non-blocking send.
        // No panics.
        if refcon.is_null() {
            return;
        }
        let state = unsafe { &*(refcon as *const SessionRefcon) };
        let name = unsafe { notification.as_ref() }.to_string();
        if let Ok(mut counts) = state.counts.lock() {
            *counts.entry(name).or_insert(0) += 1;
        }
        let _ = state.tx.try_send(EngineMessage::AxEvent { pid: state.pid });
    }

    /// Bounded owned-session observation on the calling thread's runloop.
    /// Registers the app-level session set on the application element plus,
    /// when `window_wid` joins an owned window, the window-level set on that
    /// exact window element (never a foreign one). Pumps the runloop until
    /// `timeout`, then tears everything down and reports per-name delivery
    /// counts. A hung app stalls at most one slice past its messaging
    /// timeout; failures (bad PID, unjoined WID, no consent) return fast.
    /// Every `AxElement` is constructed, used, and dropped on the calling
    /// (observer) thread.
    pub fn ax_observe_session(
        pid: i32,
        window_wid: Option<u32>,
        timeout: Duration,
        messaging_timeout_secs: f32,
        tx: &SyncSender<EngineMessage>,
    ) -> Result<AxSessionOutcome, AxObserveError> {
        if pid <= 1 {
            return Err(AxObserveError::InvalidPid(format!(
                "refusing reserved pid {pid}"
            )));
        }
        let app = AxElement::application(pid)
            .ok_or_else(|| AxObserveError::InvalidPid(format!("bad pid {pid}")))?;
        let messaging_timeout_set = ax_set_messaging_timeout(&app, messaging_timeout_secs) == 0;
        // Join the requested window before creating the observer: an
        // unjoined WID fails here, never as a silent empty watch.
        let window: Option<AxElement> = match window_wid {
            None => None,
            Some(wid) => {
                let windows = app.windows().map_err(|code| {
                    AxObserveError::System(format!("windows: {}", ax_error_name(code)))
                })?;
                match windows.into_iter().find(|w| ax_window_id(w) == Some(wid)) {
                    Some(w) => {
                        let _ = ax_set_messaging_timeout(&w, messaging_timeout_secs);
                        Some(w)
                    }
                    None => {
                        return Err(AxObserveError::System(format!(
                            "wid {wid} not joined to pid {pid}"
                        )));
                    }
                }
            }
        };
        let mut observer_raw: *mut AXObserver = std::ptr::null_mut();
        // SAFETY: out-pointer valid; maintained `AXObserver::create`; nonzero
        // status leaves `observer_raw` null and returns fast.
        let status = unsafe {
            AXObserver::create(
                pid,
                Some(ax_session_callback),
                NonNull::new(std::ptr::addr_of_mut!(observer_raw)).expect("non-null"),
            )
        };
        if status.0 != 0 || observer_raw.is_null() {
            return Err(AxObserveError::System(format!(
                "AXObserverCreate pid {pid}: {}",
                ax_error_name(status.0)
            )));
        }
        // SAFETY: create-rule +1 retained; owned by `CFRetained` exactly once.
        let observer: CFRetained<AXObserver> =
            unsafe { CFRetained::from_raw(NonNull::new(observer_raw).expect("non-null")) };
        let refcon = Box::new(SessionRefcon {
            tx: tx.clone(),
            pid: pid as u32,
            counts: std::sync::Mutex::new(std::collections::HashMap::new()),
        });
        let refcon_ptr = Box::into_raw(refcon);
        let mut added: Vec<String> = Vec::new();
        // SAFETY: observer/app/window live in maintained wrappers; names are
        // fresh `CFString`s; refcon valid until reclaimed below after
        // notifications are removed.
        unsafe {
            for name in AX_SESSION_APP_NOTIFICATIONS {
                let cf_name = CFString::from_str(name);
                let status = observer.add_notification(
                    &app.inner,
                    &cf_name,
                    refcon_ptr as *mut std::ffi::c_void,
                );
                if status.0 == 0 {
                    added.push((*name).to_owned());
                }
            }
            if let Some(window) = window.as_ref() {
                for name in AX_SESSION_WINDOW_NOTIFICATIONS {
                    let cf_name = CFString::from_str(name);
                    let status = observer.add_notification(
                        &window.inner,
                        &cf_name,
                        refcon_ptr as *mut std::ffi::c_void,
                    );
                    if status.0 == 0 {
                        added.push((*name).to_owned());
                    }
                }
            }
            if added.is_empty() {
                drop(Box::from_raw(refcon_ptr));
                return Err(AxObserveError::System(
                    "no AX notifications registered (bad pid or no consent)".to_owned(),
                ));
            }
            // Note: the maintained `run_loop_source` asserts non-null and
            // would panic on null; a null source here would mean an OS
            // contract violation (live observer must vend a source).
            let source = observer.run_loop_source();
            let rl = CFRunLoop::current().ok_or_else(|| {
                for name in added.clone() {
                    let cf_name = CFString::from_str(&name);
                    let _ = observer.remove_notification(&app.inner, &cf_name);
                    if let Some(window) = window.as_ref() {
                        let _ = observer.remove_notification(&window.inner, &cf_name);
                    }
                }
                drop(Box::from_raw(refcon_ptr));
                AxObserveError::System("AX observer runloop unavailable".to_owned())
            })?;
            let mode = kCFRunLoopDefaultMode;
            rl.add_source(Some(&source), mode);
            let deadline = Instant::now() + timeout;
            while Instant::now() < deadline {
                CFRunLoop::run_in_mode(mode, 0.25, false);
            }
            rl.remove_source(Some(&source), mode);
            source.invalidate();
            // Counts are collected while the refcon is still owned (shared
            // borrow of the live Box); reclaim happens last, after removals,
            // so no callback can observe a freed refcon.
            // SAFETY: `refcon_ptr` is the live Box installed above; no
            // callback runs past `invalidate()` on this thread's runloop, and
            // the observer belongs to this thread only.
            let mut received: Vec<(String, u32)> = (&*refcon_ptr)
                .counts
                .lock()
                .map(|counts| {
                    let mut out: Vec<(String, u32)> =
                        counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
                    out.sort_unstable();
                    out
                })
                .unwrap_or_default();
            // Removal targets both elements (a name registered on one is a
            // harmless miss on the other); statuses are teardown-only.
            // `added_names` preserves the registered set for the outcome:
            // added-but-silent names must appear there too.
            let added_names = added.clone();
            for name in added {
                let cf_name = CFString::from_str(&name);
                let _ = observer.remove_notification(&app.inner, &cf_name);
                if let Some(window) = window.as_ref() {
                    let _ = observer.remove_notification(&window.inner, &cf_name);
                }
            }
            // SAFETY: notifications removed and the runloop source
            // invalidated above; no callback can fire afterwards, so the
            // refcon is reclaimed exactly once here.
            drop(Box::from_raw(refcon_ptr));
            Ok(AxSessionOutcome {
                notifications_added: added_names,
                notifications_received: std::mem::take(&mut received),
                messaging_timeout_set,
            })
        }
    }

    // -- Phase 1 thread-affine observer guard (isolated) --
    //
    // Long-lived per-app observer for the Phase 1 runtime worker. Shape
    // pinned against yabai `src/application.c` (`AXObserverCreate` per app +
    // `AXObserverAddNotification` on the application element +
    // `CFRunLoopAddSource(CFRunLoopGetMain(),
    // AXObserverGetRunLoopSource(...), kCFRunLoopDefaultMode)`; teardown
    // invalidates the source) and the rift/glide `Observer` builder shape
    // (`Observer::new(pid)` then install on the current thread's runloop;
    // callbacks carry no layout work; the handle is `!Send` without any
    // `unsafe impl`):
    // - yabai dd845723416f5fe92af49fad5ebab00369e07edd
    //   `src/application.c` (observer create/add/runloop-source wiring)
    // - rift 8114ccc534b5c9a7e6a77ce27568d8739be9216f
    //   `src/sys/observer.rs` + `src/sys/run_loop.rs` (thread-affine
    //   `Observer`, runloop install)
    // - glide ef3b30f88445fe4b517eb4c60023011f6d3725dd
    //   `src/sys/observer.rs` + `src/sys/run_loop.rs` (same builder shape)
    //
    // Differences from those sources (deliberate): the callback here does NO
    // subscription work and NO AX reads — it only bumps an atomic and
    // non-blocking-wakes a bounded channel. Registration covers the shared
    // session sets ([`AX_SESSION_APP_NOTIFICATIONS`] plus per-window
    // [`AX_SESSION_WINDOW_NOTIFICATIONS`]); per-window subscriptions re-sync
    // when newly observed windows appear. There is deliberately NO
    // cross-thread runloop wake handle: rift/glide expose one via `unsafe
    // impl Send` on their `WakeupHandle`, which this crate forbids. Command
    // and stop latency is therefore bounded by the caller's pump slice
    // (correctness never depends on it); document the bound at the call
    // site. Create, pump, sync, focused-read, and teardown all run on the
    // owning worker thread.

    use objc2_core_foundation::CFRunLoopSource;

    struct Phase1Refcon {
        notified: Arc<AtomicU32>,
        wake: SyncSender<()>,
    }

    unsafe extern "C-unwind" fn phase1_observer_callback(
        _observer: NonNull<AXObserver>,
        _element: NonNull<AXUIElement>,
        _notification: NonNull<CFString>,
        refcon: *mut std::ffi::c_void,
    ) {
        // SAFETY: `refcon` is the live `Box<Phase1Refcon>` installed by
        // `Phase1Observer::create`, reclaimed only after notifications are
        // removed and the runloop source is invalidated on the owner thread.
        // Minimal work only: bump plus non-blocking wake. No AX, no layout,
        // no log, no panics.
        if refcon.is_null() {
            return;
        }
        let state = unsafe { &*(refcon as *const Phase1Refcon) };
        state.notified.fetch_add(1, Ordering::SeqCst);
        let _ = state.wake.try_send(());
    }

    struct Phase1WindowWatch {
        wid: u32,
        element: AxElement,
        names: Vec<String>,
    }

    /// Thread-affine Phase 1 observer guard. `!Send + !Sync` via
    /// `PhantomData<Rc>` (no `unsafe impl`): the compiler rejects moving it
    /// off the owning worker thread. Prefer explicit [`Self::shutdown`] for
    /// an honest teardown report; `Drop` repeats the same steps
    /// best-effort when the owner exits without it.
    pub struct Phase1Observer {
        observer: CFRetained<AXObserver>,
        app: AxElement,
        app_names: Vec<String>,
        windows: Vec<Phase1WindowWatch>,
        source: Option<CFRetained<CFRunLoopSource>>,
        refcon: Option<*mut Phase1Refcon>,
        notified: Arc<AtomicU32>,
        wake_rx: std::sync::mpsc::Receiver<()>,
        _not_send: std::marker::PhantomData<std::rc::Rc<()>>,
    }

    impl Drop for Phase1Observer {
        fn drop(&mut self) {
            // SAFETY: same teardown ordering as `shutdown` (removals, then
            // source invalidation, then refcon reclaim); statuses ignored —
            // explicit `shutdown` is the honest path.
            let _ = self.teardown();
        }
    }

    impl Phase1Observer {
        /// Create and install the observer on the calling (owner) thread's
        /// runloop. Registers the app-level session set; per-window
        /// subscriptions arrive via [`Self::sync_windows`]. The messaging
        /// timeout is set BEFORE any registration; refusal fails creation.
        pub fn create(pid: i32, messaging_timeout_secs: f32) -> Result<Self, String> {
            if pid <= 1 {
                return Err(format!("refusing reserved pid {pid}"));
            }
            let app = AxElement::application(pid).ok_or_else(|| format!("bad pid {pid}"))?;
            if ax_set_messaging_timeout(&app, messaging_timeout_secs) != 0 {
                return Err(format!(
                    "pid {pid}: messaging timeout unset before observer registration"
                ));
            }
            let mut observer_raw: *mut AXObserver = std::ptr::null_mut();
            // SAFETY: out-pointer valid; maintained `AXObserver::create`;
            // nonzero status leaves `observer_raw` null and returns fast.
            let status = unsafe {
                AXObserver::create(
                    pid,
                    Some(phase1_observer_callback),
                    NonNull::new(std::ptr::addr_of_mut!(observer_raw)).expect("non-null"),
                )
            };
            if status.0 != 0 || observer_raw.is_null() {
                return Err(format!(
                    "AXObserverCreate pid {pid}: {}",
                    super::ax_error_name(status.0)
                ));
            }
            // SAFETY: create-rule +1 retained; owned exactly once.
            let observer: CFRetained<AXObserver> =
                unsafe { CFRetained::from_raw(NonNull::new(observer_raw).expect("non-null")) };
            let notified = Arc::new(AtomicU32::new(0));
            let (wake_tx, wake_rx) = std::sync::mpsc::sync_channel::<()>(64);
            let refcon = Box::new(Phase1Refcon {
                notified: notified.clone(),
                wake: wake_tx,
            });
            let refcon_ptr = Box::into_raw(refcon);
            let mut app_names: Vec<String> = Vec::new();
            // SAFETY: observer/app live in maintained wrappers; names are
            // fresh `CFString`s; refcon valid until reclaimed in `teardown`
            // after removals + source invalidation.
            unsafe {
                for name in super::AX_SESSION_APP_NOTIFICATIONS {
                    let cf_name = CFString::from_str(name);
                    let status = observer.add_notification(
                        &app.inner,
                        &cf_name,
                        refcon_ptr as *mut std::ffi::c_void,
                    );
                    if status.0 == 0 {
                        app_names.push((*name).to_owned());
                    }
                }
            }
            if app_names.is_empty() {
                // SAFETY: nothing registered and no source installed, so no
                // callback can fire; reclaim exactly once.
                unsafe {
                    drop(Box::from_raw(refcon_ptr));
                }
                return Err(format!(
                    "pid {pid}: no AX notifications registered (bad pid or no consent)"
                ));
            }
            // SAFETY: live observer must vend a runloop source (OS contract);
            // the maintained accessor asserts non-null. Installed on the
            // calling (owner) thread's runloop.
            let source: CFRetained<CFRunLoopSource> = unsafe { observer.run_loop_source() };
            let rl = match CFRunLoop::current() {
                Some(rl) => rl,
                None => {
                    unsafe {
                        for name in app_names {
                            let cf_name = CFString::from_str(&name);
                            let _ = observer.remove_notification(&app.inner, &cf_name);
                        }
                        drop(Box::from_raw(refcon_ptr));
                    }
                    return Err("AX observer runloop unavailable".to_owned());
                }
            };
            // SAFETY: extern-static default-mode read; source/runloop live.
            unsafe {
                let mode = kCFRunLoopDefaultMode;
                rl.add_source(Some(&source), mode);
            }
            Ok(Self {
                observer,
                app,
                app_names,
                windows: Vec::new(),
                source: Some(source),
                refcon: Some(refcon_ptr),
                notified,
                wake_rx,
                _not_send: std::marker::PhantomData,
            })
        }

        /// Pump the owner thread's runloop for at most `slice` (capped):
        /// notifications delivered here only bump the counter (see
        /// `take_notified`). The cap bounds command/stop latency without a
        /// cross-thread wake handle; delivery itself still wakes the slice.
        pub fn pump(&self, slice: Duration) {
            while self.wake_rx.try_recv().is_ok() {}
            // SAFETY: extern-static default-mode read; bounded slice keeps a
            // hung app from stalling the owner past one slice.
            unsafe {
                let mode = kCFRunLoopDefaultMode;
                CFRunLoop::run_in_mode(mode, slice.as_secs_f64().min(0.25), false);
            }
        }

        /// Notifications delivered since the last call (counter reset).
        /// The wake channel is drained so it never accumulates.
        pub fn take_notified(&self) -> u32 {
            while self.wake_rx.try_recv().is_ok() {}
            self.notified.swap(0, Ordering::SeqCst)
        }

        /// Re-sync per-window subscriptions to the currently observed set.
        /// New windows join (unjoined wids are skipped, never watched
        /// foreign); gone windows are removed best-effort. At most 64
        /// watched windows. App-level join/timeout failures are `Err`;
        /// per-window misses are skipped without failing the whole sync.
        pub fn sync_windows(
            &mut self,
            wids: &[u32],
            messaging_timeout_secs: f32,
        ) -> Result<(), String> {
            const MAX_WATCHED: usize = 64;
            let wanted: Vec<u32> = wids.iter().copied().take(MAX_WATCHED).collect();
            if ax_set_messaging_timeout(&self.app, messaging_timeout_secs) != 0 {
                return Err("observer window sync: messaging timeout unset".to_owned());
            }
            let mut windows = self
                .app
                .windows()
                .map_err(|code| format!("observer window sync: {}", super::ax_error_name(code)))?;
            // Drop gone watches (removals best-effort; teardown retries).
            let mut kept = Vec::with_capacity(self.windows.len());
            for watch in std::mem::take(&mut self.windows) {
                if wanted.contains(&watch.wid) {
                    kept.push(watch);
                } else {
                    // SAFETY: live observer/element; statuses teardown-only.
                    unsafe {
                        for name in &watch.names {
                            let cf_name = CFString::from_str(name);
                            let _ = self
                                .observer
                                .remove_notification(&watch.element.inner, &cf_name);
                        }
                    }
                }
            }
            self.windows = kept;
            let Some(refcon_ptr) = self.refcon else {
                return Err("observer window sync: torn down".to_owned());
            };
            for wid in wanted {
                if self.windows.iter().any(|w| w.wid == wid) {
                    continue;
                }
                // Per-candidate join with its own finite bound FIRST: a
                // candidate whose timeout refuses is skipped (narrow abort),
                // never read unbounded. When every candidate refuses, the
                // whole sync fails honestly instead of joining unbounded.
                let mut refused_joins = 0usize;
                let mut joined: Option<usize> = None;
                for (index, window) in windows.iter().enumerate() {
                    if ax_set_messaging_timeout(window, messaging_timeout_secs) != 0 {
                        refused_joins += 1;
                        continue;
                    }
                    if ax_window_id(window) == Some(wid) {
                        joined = Some(index);
                        break;
                    }
                }
                let Some(pos) = joined else {
                    if refused_joins > 0 && refused_joins == windows.len() {
                        return Err(
                            "observer window sync: messaging timeout unset for all candidates"
                                .to_owned(),
                        );
                    }
                    continue;
                };
                let mut names: Vec<String> = Vec::new();
                // SAFETY: live observer/element; refcon installed until
                // `teardown` (checked above).
                unsafe {
                    for name in super::AX_SESSION_WINDOW_NOTIFICATIONS {
                        let cf_name = CFString::from_str(name);
                        let status = self.observer.add_notification(
                            &windows[pos].inner,
                            &cf_name,
                            refcon_ptr as *mut std::ffi::c_void,
                        );
                        if status.0 == 0 {
                            names.push((*name).to_owned());
                        }
                    }
                }
                if names.is_empty() {
                    windows.remove(pos);
                    continue;
                }
                let element = windows.remove(pos);
                self.windows.push(Phase1WindowWatch {
                    wid,
                    element,
                    names,
                });
            }
            Ok(())
        }

        /// Best-effort focused window of the observed app: the direct
        /// `AXFocusedWindow` join, falling back to the `AXMainWindow` join
        /// only when the focused leg is unavailable/missing (same selection
        /// as [`super::select_direct_window`]). `None` is honest unknown —
        /// never a claim of "no focus". The app-level timeout gates the
        /// attribute reads and a per-window timeout gates each identity
        /// join: a join that refuses its bound yields unknown, never an
        /// unbounded read. Global promotion still requires the selected
        /// oracle agreement (SLPS + workspace foreground, exact PID/WID, CG
        /// ownership) — never this leg alone.
        pub fn focused_wid(&self, messaging_timeout_secs: f32) -> Option<u32> {
            if ax_set_messaging_timeout(&self.app, messaging_timeout_secs) != 0 {
                return None;
            }
            let join = |attr: &str| {
                let value = self.app.copy_attribute(attr).ok()?;
                let inner: CFRetained<AXUIElement> = value.downcast().ok()?;
                let window = AxElement {
                    inner,
                    _not_send: std::marker::PhantomData,
                };
                if ax_set_messaging_timeout(&window, messaging_timeout_secs) != 0 {
                    return None;
                }
                ax_window_id(&window).filter(|wid| *wid != 0)
            };
            join("AXFocusedWindow").or_else(|| join("AXMainWindow"))
        }

        /// Honest teardown: removes every registration, invalidates the
        /// runloop source, and reclaims the refcon exactly once. `Err`
        /// names failed removals (OS status names only); the guard is
        /// consumed so teardown runs once (`Drop` becomes a no-op).
        pub fn shutdown(mut self) -> Result<(), String> {
            let failures = self.teardown();
            if failures.is_empty() {
                Ok(())
            } else {
                Err(format!("observer teardown: {}", failures.join("; ")))
            }
        }

        fn teardown(&mut self) -> Vec<String> {
            let mut failures = Vec::new();
            // SAFETY: observer/app/watches live until reclaimed below;
            // statuses recorded, never assumed.
            unsafe {
                for name in std::mem::take(&mut self.app_names) {
                    let cf_name = CFString::from_str(&name);
                    let status = self.observer.remove_notification(&self.app.inner, &cf_name);
                    if status.0 != 0 {
                        failures.push(format!("{name}: {}", super::ax_error_name(status.0)));
                    }
                }
                for watch in std::mem::take(&mut self.windows) {
                    for name in watch.names {
                        let cf_name = CFString::from_str(&name);
                        let status = self
                            .observer
                            .remove_notification(&watch.element.inner, &cf_name);
                        if status.0 != 0 {
                            failures.push(format!("{name}: {}", super::ax_error_name(status.0)));
                        }
                    }
                }
                if let Some(source) = self.source.take() {
                    if let Some(rl) = CFRunLoop::current() {
                        let mode = kCFRunLoopDefaultMode;
                        rl.remove_source(Some(&source), mode);
                    }
                    source.invalidate();
                }
                if let Some(refcon_ptr) = self.refcon.take() {
                    drop(Box::from_raw(refcon_ptr));
                }
            }
            failures
        }
    }

    /// Observe Secure Input state. `None` (missing symbol) is unknown, never
    /// inactive.
    pub fn secure_input_known() -> Option<bool> {
        type FnTy = unsafe extern "C" fn() -> bool;
        // SAFETY: `libc::dlsym` lookups null-checked; the getter takes no
        // args. Failure to resolve means "unknown", reported as `None`.
        unsafe {
            const CANDIDATES: &[&[u8]] = &[
                b"/System/Library/Frameworks/Carbon.framework/Carbon\0",
                b"/System/Library/PrivateFrameworks/HIServices.framework/HIServices\0",
            ];
            for path in CANDIDATES {
                let handle = libc::dlopen(path.as_ptr().cast(), 0x1);
                if handle.is_null() {
                    continue;
                }
                let name = CString::new("IsSecureEventInputEnabled").expect("static");
                let sym = libc::dlsym(handle, name.as_ptr());
                if !sym.is_null() {
                    let f: FnTy = std::mem::transmute::<*mut std::ffi::c_void, FnTy>(sym);
                    return Some(f());
                }
            }
            let self_handle = libc::dlopen(std::ptr::null(), 0x1);
            if !self_handle.is_null() {
                let name = CString::new("IsSecureEventInputEnabled").expect("static");
                let sym = libc::dlsym(self_handle, name.as_ptr());
                if !sym.is_null() {
                    let f: FnTy = std::mem::transmute::<*mut std::ffi::c_void, FnTy>(sym);
                    return Some(f());
                }
            }
            None
        }
    }

    // -- Window-server session lock state (actual, not just loginwindow) --
    //
    // `CGSessionCopyCurrentDictionary` rides the maintained
    // `objc2-core-graphics` `CGSession` binding (no hand declaration, no new
    // `unsafe` beyond the single cast below): a plain C call that is
    // read-only, touches no AppKit, and runs on no runloop, so it is callable
    // from any thread — the tap owner, the per-app AX owners, and the main
    // thread all read it live. Same class as Hammerspoon's
    // `hs.caffeinate.sessionProperties`, which calls it from a Lua C-function
    // context (hammerspoon `extensions/caffeinate/libcaffeinate.m`).
    // CGSession.h uses different literal spellings for these two macros:
    // `kCGSessionLoginDoneKey` → `"kCGSessionLoginDoneKey"`, and
    // `kCGSessionOnConsoleKey` → `"kCGSSessionOnConsoleKey"`.
    // The `CGSSessionScreenIsLocked` boolean
    // marker is undocumented window-server state (no surveyed local checkout
    // names it; typed session-dict marker reads have skhd precedent —
    // `skhd.c` `secure_keyboard_entry_process_info` reads
    // `kCGSSessionSecureInputPID` from the session dictionary): it is read
    // tier2-style with fail-closed typing (absent is normally unlocked, a
    // wrong-typed marker is unknown), and login/console/frontmost gates stay
    // regardless. A NULL dictionary (not running inside a Quartz GUI session)
    // or any missing documented leg is `Unknown` — fail closed, never guessed
    // unlocked.

    /// Live window-server session legs plus the lock verdict. Read-only C
    /// call, callable from any thread (see the note above). Booleans only.
    pub fn session_state() -> SessionState {
        use objc2_core_foundation::{CFBoolean, CFDictionary, CFString, CFType};
        let raw = objc2_core_graphics::CGSessionCopyCurrentDictionary();
        let Some(raw) = raw else {
            return SessionState::unknown();
        };
        // SAFETY: the window-server session dictionary has string keys
        // (same cast as glide `src/sys/session.rs` `current_session_dict`).
        let dict: CFRetained<CFDictionary<CFString, CFType>> =
            unsafe { CFRetained::cast_unchecked(raw) };
        let flag = |key: &'static str| {
            dict.get(&CFString::from_static_str(key))
                .and_then(|value| value.downcast::<CFBoolean>().ok())
                .map(|boolean| boolean.as_bool())
        };
        let login_done = flag("kCGSessionLoginDoneKey");
        let on_console = flag("kCGSSessionOnConsoleKey");
        // Undocumented screen-lock marker: absence is normally unlocked
        // (proceeds under the other legs); presence must be boolean —
        // anything else is malformed (unknown, never guessed).
        let screen_lock = match dict.get(&CFString::from_static_str("CGSSessionScreenIsLocked")) {
            None => ScreenLockRead::Absent,
            Some(value) => match value.downcast::<CFBoolean>() {
                Ok(boolean) => {
                    if boolean.as_bool() {
                        ScreenLockRead::Locked
                    } else {
                        ScreenLockRead::Unlocked
                    }
                }
                Err(_) => ScreenLockRead::Malformed,
            },
        };
        SessionState {
            login_done,
            on_console,
            screen_lock,
            lock: lock_from_session_legs(login_done, on_console, screen_lock),
        }
    }

    /// This process's own bundle identifier via the maintained
    /// `objc2-foundation` `NSBundle` binding. `None` for unbundled
    /// (direct-spawn) processes. The owned helper calls this on its main
    /// thread to select regular activation when running inside the signed
    /// helper `.app`; every other module treats the value as an opaque ID.
    /// No titles, no content.
    pub fn own_bundle_id() -> Option<String> {
        use objc2_foundation::NSBundle;
        NSBundle::mainBundle()
            .bundleIdentifier()
            .map(|id| id.to_string())
    }

    // -- Dynamic SkyLight reads (private symbols; missing => Err) --

    const SKYLIGHT: &[u8] = b"/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight\0";

    struct SlsTable {
        connection: unsafe extern "C" fn() -> i32,
        copy_spaces: unsafe extern "C" fn(i32) -> RawRef,
        copy_displays: unsafe extern "C" fn(i32) -> RawRef,
        current_space: unsafe extern "C" fn(i32, RawConstRef) -> u64,
    }

    static SLS_TABLE: OnceLock<Option<SlsTable>> = OnceLock::new();

    fn sls_table() -> Option<&'static SlsTable> {
        SLS_TABLE
            .get_or_init(|| {
                // SAFETY: every symbol null-checked; signatures match the
                // reference declarations (`int SLSMainConnectionID(void)`,
                // `CFArrayRef SLSCopyManagedDisplaySpaces(int)`,
                // `CFArrayRef SLSCopyManagedDisplays(int)`,
                // `uint64_t SLSManagedDisplayGetCurrentSpace(int, CFStringRef)`).
                // Returned arrays are retained and wrapped by the caller by
                // copy-rule contract (top-level kind not rechecked).
                unsafe {
                    let handle = libc::dlopen(SKYLIGHT.as_ptr().cast(), 0x1);
                    if handle.is_null() {
                        return None;
                    }
                    let sym = |name: &str| {
                        let cname = CString::new(name).expect("static");
                        libc::dlsym(handle, cname.as_ptr())
                    };
                    let connection = sym("SLSMainConnectionID");
                    let copy_spaces = sym("SLSCopyManagedDisplaySpaces");
                    let copy_displays = sym("SLSCopyManagedDisplays");
                    let current_space = sym("SLSManagedDisplayGetCurrentSpace");
                    if connection.is_null()
                        || copy_spaces.is_null()
                        || copy_displays.is_null()
                        || current_space.is_null()
                    {
                        return None;
                    }
                    Some(SlsTable {
                        connection: std::mem::transmute::<
                            *mut std::ffi::c_void,
                            unsafe extern "C" fn() -> i32,
                        >(connection),
                        copy_spaces: std::mem::transmute::<
                            *mut std::ffi::c_void,
                            unsafe extern "C" fn(i32) -> RawRef,
                        >(copy_spaces),
                        copy_displays: std::mem::transmute::<
                            *mut std::ffi::c_void,
                            unsafe extern "C" fn(i32) -> RawRef,
                        >(copy_displays),
                        current_space: std::mem::transmute::<
                            *mut std::ffi::c_void,
                            unsafe extern "C" fn(i32, RawConstRef) -> u64,
                        >(current_space),
                    })
                }
            })
            .as_ref()
    }

    pub fn sls_connection_available() -> bool {
        sls_table().is_some()
    }

    pub fn sls_main_connection_id() -> Option<i32> {
        let table = sls_table()?;
        // SAFETY: resolved non-null symbol, no args.
        Some(unsafe { (table.connection)() })
    }

    const MAX_DISPLAYS: usize = 64;
    const MAX_SPACES_PER_DISPLAY: usize = 256;
    const MAX_MEMBERSHIP_WINDOWS: usize = 16;

    /// Read-only managed display/Space/current read. IDs and counts only;
    /// no names, no window content. Every container null-checked; nested
    /// container kinds verified via `downcast_ref` before typed use. The two
    /// top-level SLS arrays are wrapped by copy-rule contract (kind not
    /// rechecked); UUID strings and Space IDs come from checked per-element
    /// downcasts.
    pub fn sls_display_spaces() -> Result<Vec<SlsDisplay>, String> {
        let table = sls_table().ok_or_else(|| "SLS symbols unavailable".to_owned())?;
        // SAFETY: resolved symbols; retained top-level results are wrapped
        // as `CFType` and checked-downcast to `CFArray` before any typed
        // view; elements are indexed within `len().min(...)` (no unbounded
        // materialization); nested container kinds verified below before
        // typed use; strings via `to_string`.
        unsafe {
            use objc2_core_foundation::CFDictionary as CFDictUntyped;
            let cid = (table.connection)();
            let spaces_raw = (table.copy_spaces)(cid);
            if spaces_raw.is_null() {
                return Err("SLSCopyManagedDisplaySpaces returned null".to_owned());
            }
            let spaces_top: CFRetained<CFType> =
                CFRetained::from_raw(NonNull::new(spaces_raw as *mut CFType).expect("non-null"));
            let spaces_untyped: CFRetained<CFArray> = spaces_top.downcast().map_err(|_| {
                "SLSCopyManagedDisplaySpaces returned a non-array top-level container".to_owned()
            })?;
            // SAFETY: top-level kind verified by the downcast above; viewed
            // as `CFArray<CFType>` (layouts identical; all elements CFTypes).
            let spaces_array: CFRetained<CFArray<CFType>> =
                CFRetained::cast_unchecked(spaces_untyped);
            let displays_raw = (table.copy_displays)(cid);
            let display_uuids: Vec<Option<String>> = if displays_raw.is_null() {
                Vec::new()
            } else {
                let displays_top: CFRetained<CFType> = CFRetained::from_raw(
                    NonNull::new(displays_raw as *mut CFType).expect("non-null"),
                );
                let displays_untyped: CFRetained<CFArray> =
                    displays_top.downcast().map_err(|_| {
                        "SLSCopyManagedDisplays returned a non-array top-level container".to_owned()
                    })?;
                // SAFETY: top-level kind verified; managed-display entries
                // are CFStrings per SkyLight copy-rule contract, checked per
                // element below.
                let displays_array: CFRetained<CFArray<CFType>> =
                    CFRetained::cast_unchecked(displays_untyped);
                let mut uuids = Vec::new();
                for i in 0..displays_array.len().min(MAX_DISPLAYS) {
                    uuids.push(
                        displays_array
                            .get(i)
                            .and_then(|item| item.downcast::<CFString>().ok())
                            .map(|s| s.to_string()),
                    );
                }
                uuids
            };
            let spaces_key = CFString::from_str("Spaces");
            let id_key = CFString::from_str("id64");
            let mut out = Vec::new();
            for index in 0..spaces_array.len().min(MAX_DISPLAYS) {
                let display_item = match spaces_array.get(index) {
                    Some(item) => item,
                    None => continue,
                };
                let mut entry = SlsDisplay {
                    uuid: display_uuids.get(index).cloned().flatten(),
                    spaces: Vec::new(),
                    current: None,
                };
                // Verify dictionary kind untyped, then view typed via a
                // retained clone (owner stays alive; no borrowed raw view).
                if display_item.downcast_ref::<CFDictUntyped>().is_some() {
                    // SAFETY: kind verified above; SLS display entries map
                    // `CFString` keys to `CFType` values; layouts identical.
                    let display_dict: CFRetained<CFDictionary<CFString, CFType>> =
                        CFRetained::cast_unchecked(display_item.clone());
                    if let Some(spaces_val) = display_dict.get(&spaces_key)
                        && spaces_val.downcast_ref::<CFArray>().is_some()
                    {
                        // SAFETY: kind verified; space list holds
                        // dictionaries (each a CFType).
                        let spaces_list: CFRetained<CFArray<CFType>> =
                            CFRetained::cast_unchecked(spaces_val.clone());
                        for j in 0..spaces_list.len().min(MAX_SPACES_PER_DISPLAY) {
                            let space_item = match spaces_list.get(j) {
                                Some(item) => item,
                                None => continue,
                            };
                            if space_item.downcast_ref::<CFDictUntyped>().is_some() {
                                // SAFETY: kind verified; space entries map
                                // `CFString` keys to `CFType` values.
                                let space_dict: CFRetained<CFDictionary<CFString, CFType>> =
                                    CFRetained::cast_unchecked(space_item.clone());
                                if let Some(sid_val) = space_dict.get(&id_key)
                                    && let Ok(num) = sid_val.downcast::<CFNumber>()
                                    && let Some(sid) = num.as_i64()
                                    && sid >= 0
                                {
                                    entry.spaces.push(sid as u64);
                                }
                            }
                        }
                    }
                }
                if let Some(uuid) = entry.uuid.clone() {
                    let uuid_cf = CFString::from_str(&uuid);
                    let uuid_ptr = CFRetained::as_ptr(&uuid_cf).as_ptr() as *const std::ffi::c_void;
                    entry.current = Some((table.current_space)(cid, uuid_ptr));
                }
                out.push(entry);
            }
            Ok(out)
        }
    }

    // -- Read-only per-window Space membership (pinned yabai signature) --
    //
    // Signature pinned against yabai `src/misc/extern.h`:
    //   `CFArrayRef SLSCopySpacesForWindows(int cid, int selector,
    //   CFArrayRef window_list);`
    // Call shape pinned against yabai `src/window.c` (`window_space_list`):
    // selector `0x7`, `window_list` a CFArray of CFNumber/SInt32 window IDs,
    // returning a CFArray of CFNumber Space IDs (NULL on failure, in which
    // case yabai falls back to the window's display Space). Read-only: no
    // SLS write symbol is resolved or called anywhere in this module.
    // Source: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/misc/extern.h#L22-L22
    // and https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window.c#L89-L111
    // (live behavior still UNVERIFIED on a Mac — a
    // missing symbol or null result is `Err`, never guessed data).

    const SLS_SPACES_SELECTOR: i32 = 0x7;

    struct SlsMembership {
        copy_spaces_for_windows: unsafe extern "C" fn(i32, i32, RawConstRef) -> RawRef,
    }

    static SLS_MEMBERSHIP: OnceLock<Option<SlsMembership>> = OnceLock::new();

    fn sls_membership() -> Option<&'static SlsMembership> {
        SLS_MEMBERSHIP
            .get_or_init(|| {
                // SAFETY: symbol null-checked; signature matches the pinned
                // yabai declaration above. Returned array is retained and
                // wrapped by the caller by copy-rule contract.
                unsafe {
                    let handle = libc::dlopen(SKYLIGHT.as_ptr().cast(), 0x1);
                    if handle.is_null() {
                        return None;
                    }
                    let cname = CString::new("SLSCopySpacesForWindows").expect("static");
                    let sym = libc::dlsym(handle, cname.as_ptr());
                    if sym.is_null() {
                        return None;
                    }
                    Some(SlsMembership {
                        copy_spaces_for_windows: std::mem::transmute::<
                            *mut std::ffi::c_void,
                            unsafe extern "C" fn(i32, i32, RawConstRef) -> RawRef,
                        >(sym),
                    })
                }
            })
            .as_ref()
    }

    /// Read-only Space membership for up to [`MAX_MEMBERSHIP_WINDOWS`]
    /// window IDs. Returns the Space IDs owning those windows (IDs only).
    /// Missing symbols, null results, or malformed containers are `Err`
    /// with the cause named — never fabricated membership.
    pub fn sls_spaces_for_windows(wids: &[u32]) -> Result<Vec<u64>, String> {
        if wids.is_empty() || wids.len() > MAX_MEMBERSHIP_WINDOWS {
            return Err(format!(
                "window count out of bound 1..={MAX_MEMBERSHIP_WINDOWS}"
            ));
        }
        let table = sls_table().ok_or_else(|| "SLS symbols unavailable".to_owned())?;
        let membership =
            sls_membership().ok_or_else(|| "SLSCopySpacesForWindows unavailable".to_owned())?;
        // SAFETY: window list built with the maintained `CFNumber`/`CFArray`
        // wrappers; the private call receives a live pointer; the retained
        // top-level result is wrapped as `CFType` and checked-downcast to
        // `CFArray` before any typed view; elements are indexed within
        // `len().min(...)` (no unbounded materialization); Space IDs come
        // from checked per-element downcasts below.
        unsafe {
            let cid = (table.connection)();
            let numbers: Vec<CFRetained<CFNumber>> = wids
                .iter()
                .map(|wid| CFNumber::new_i32(*wid as i32))
                .collect();
            let window_list: CFRetained<CFArray<CFNumber>> =
                CFArray::from_retained_objects(&numbers);
            let list_ptr = CFRetained::as_ptr(&window_list).as_ptr() as *const std::ffi::c_void;
            let spaces_raw =
                (membership.copy_spaces_for_windows)(cid, SLS_SPACES_SELECTOR, list_ptr);
            if spaces_raw.is_null() {
                return Err("SLSCopySpacesForWindows returned null".to_owned());
            }
            let spaces_top: CFRetained<CFType> =
                CFRetained::from_raw(NonNull::new(spaces_raw as *mut CFType).expect("non-null"));
            let spaces_untyped: CFRetained<CFArray> = spaces_top.downcast().map_err(|_| {
                "SLSCopySpacesForWindows returned a non-array top-level container".to_owned()
            })?;
            // SAFETY: top-level kind verified by the downcast above; viewed
            // as `CFArray<CFType>` (layouts identical; all elements CFTypes).
            let spaces: CFRetained<CFArray<CFType>> = CFRetained::cast_unchecked(spaces_untyped);
            let mut out = Vec::new();
            for i in 0..spaces.len().min(MAX_SPACES_PER_DISPLAY) {
                if let Some(item) = spaces.get(i)
                    && let Ok(num) = item.downcast::<CFNumber>()
                    && let Some(sid) = num.as_i64()
                    && sid >= 0
                {
                    out.push(sid as u64);
                }
            }
            Ok(out)
        }
    }

    /// Read-only Space membership for exactly one window ID. Single-element
    /// call over [`sls_spaces_for_windows`]: this matches the pinned yabai
    /// usage (`window_space_list` in `src/window.c` queries one window and
    /// reads the flat Space list), so per-window membership never depends
    /// on the undocumented multi-window result structure. A total SLS
    /// failure is `Err` (nothing publishable as scoped); callers treat an
    /// empty list as unknown home (omit, never destroy).
    /// Source: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window.c
    pub fn sls_spaces_for_window(wid: u32) -> Result<Vec<u64>, String> {
        if wid == 0 {
            return Err("refusing window id 0".to_owned());
        }
        sls_spaces_for_windows(std::slice::from_ref(&wid))
    }

    // -- Read-only Space type (`SLSSpaceGetType`, pinned yabai signature) --
    //
    // Signature pinned against yabai `src/misc/extern.h`:
    //   `extern int SLSSpaceGetType(int cid, uint64_t sid);`
    // Type ids pinned against yabai `src/space.c` (`space_is_user` 0,
    // `space_is_fullscreen` 4, `space_is_system` 2). Read-only: gates the
    // Tier-2 move/restore target and preimage to user Spaces (no proven
    // restore out of fullscreen/system Spaces).
    // Source: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/misc/extern.h#L56-L56
    // and https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space.c#L88-L101
    // (live behavior still UNVERIFIED on a Mac — a missing symbol is `Err`,
    // never a guessed type).

    struct SlsSpaceTypeTable {
        get_type: unsafe extern "C" fn(i32, u64) -> i32,
    }

    static SLS_SPACE_TYPE_TABLE: OnceLock<Option<SlsSpaceTypeTable>> = OnceLock::new();

    fn sls_space_type_table() -> Option<&'static SlsSpaceTypeTable> {
        SLS_SPACE_TYPE_TABLE
            .get_or_init(|| {
                // SAFETY: symbol null-checked before transmuting to its
                // pinned yabai declaration; a missing symbol fails the table
                // closed, never guessed.
                unsafe {
                    let handle = libc::dlopen(SKYLIGHT.as_ptr().cast(), 0x1);
                    if handle.is_null() {
                        return None;
                    }
                    let cname = CString::new("SLSSpaceGetType").expect("static");
                    let sym = libc::dlsym(handle, cname.as_ptr());
                    if sym.is_null() {
                        return None;
                    }
                    Some(SlsSpaceTypeTable {
                        get_type: std::mem::transmute::<
                            *mut std::ffi::c_void,
                            unsafe extern "C" fn(i32, u64) -> i32,
                        >(sym),
                    })
                }
            })
            .as_ref()
    }

    /// Read-only Space type for one Space ID (see [`super::classify_space_type`]).
    /// Missing symbols are `Err` with the cause named — never a guessed type.
    pub fn sls_space_type(sid: u64) -> Result<super::SlsSpaceType, String> {
        let base = sls_table().ok_or_else(|| "SLS symbols unavailable".to_owned())?;
        let table =
            sls_space_type_table().ok_or_else(|| "SLSSpaceGetType unavailable".to_owned())?;
        // SAFETY: resolved non-null symbols; plain integer data; the return
        // value is classified by the pure mapper, never trusted raw.
        unsafe {
            Ok(super::classify_space_type((table.get_type)(
                (base.connection)(),
                sid,
            )))
        }
    }

    // -- Public CGWindowList identity (maintained `core-graphics` binding) --
    //
    // Owned helper window IDs come from `CGWindowListCopyWindowInfo` via the
    // maintained `core-graphics` 0.25.0 crate (pinned in Cargo.lock), never
    // from AX: no Accessibility consent and no AX dependency. The list uses
    // `kCGWindowListOptionAll` (not on-screen-only) so owned windows on
    // inactive Spaces still join; an on-screen-only snapshot cannot serve
    // hidden-membership reads. IDs, PIDs, layers, on-screen flags, bounds,
    // and the window title are surfaced — never owner names or content. The
    // owned helper's title carries its per-run nonce (see
    // [`crate::probe_target`]); a missing title fails closed at the live
    // nonce check, never guessed.

    /// Bounds in screen points (`kCGWindowBounds` X/Y/Width/Height).
    #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    pub struct CgBounds {
        pub x: f64,
        pub y: f64,
        pub w: f64,
        pub h: f64,
    }

    /// One CGWindowList entry: identity plus geometry plus window title.
    /// The title is identity only (the owned helper's nonce title); owner
    /// names are never surfaced.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct CgWindow {
        pub wid: u32,
        pub pid: u32,
        pub layer: i32,
        pub onscreen: bool,
        pub bounds: CgBounds,
        /// `kCGWindowName` when the window server reports one; `None` when
        /// absent (e.g. untitled windows or withheld names). A missing owned
        /// title fails closed at the live nonce check.
        pub title: Option<String>,
    }

    /// Full window list via the maintained `core-graphics` binding, parsed
    /// with maintained `core-foundation` accessors. Bounded to
    /// [`crate::config::MAX_WINDOWS`]; entries missing a positive numeric ID
    /// or PID are skipped (never guessed); a null list is `Err`, never an
    /// empty claim.
    pub fn cg_window_list_all() -> Result<Vec<CgWindow>, String> {
        cg_window_list_inner(true)
    }

    /// Shared parser: `with_title == false` skips the `kCGWindowName` leg
    /// entirely (no title acquisition, not even transiently).
    fn cg_window_list_inner(with_title: bool) -> Result<Vec<CgWindow>, String> {
        use core_foundation::array::CFArray;
        use core_foundation::base::TCFType;
        use core_foundation::boolean::CFBoolean;
        use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
        use core_foundation::number::{CFNumber, CFNumberRef};
        use core_foundation::string::CFString;
        use core_graphics::window as cgwin;

        let list: CFArray =
            cgwin::copy_window_info(cgwin::kCGWindowListOptionAll, cgwin::kCGNullWindowID)
                .ok_or_else(|| "CGWindowListCopyWindowInfo returned null".to_owned())?;
        // SAFETY: `kCGWindow*` are live framework constants;
        // `wrap_under_get_rule` borrows without ownership transfer; the list
        // outlives this call; numeric reads are gated by `to_*` results and
        // every raw pointer is null-checked before wrapping.
        unsafe {
            let key_number = CFString::wrap_under_get_rule(cgwin::kCGWindowNumber);
            let key_pid = CFString::wrap_under_get_rule(cgwin::kCGWindowOwnerPID);
            let key_layer = CFString::wrap_under_get_rule(cgwin::kCGWindowLayer);
            let key_onscreen = CFString::wrap_under_get_rule(cgwin::kCGWindowIsOnscreen);
            let key_bounds = CFString::wrap_under_get_rule(cgwin::kCGWindowBounds);
            let key_name = CFString::wrap_under_get_rule(cgwin::kCGWindowName);
            let raw_key = |key: &CFString| key.as_concrete_TypeRef() as *const std::ffi::c_void;
            let true_ref = CFBoolean::true_value().as_concrete_TypeRef() as *const std::ffi::c_void;
            let as_number =
                |dict: &CFDictionary, key: *const std::ffi::c_void| -> Option<CFNumber> {
                    let raw: *const std::ffi::c_void = *dict.find(key)?;
                    if raw.is_null() {
                        return None;
                    }
                    Some(CFNumber::wrap_under_get_rule(raw as CFNumberRef))
                };
            let bounds_of = |dict: &CFDictionary| -> Option<CgBounds> {
                let raw: *const std::ffi::c_void = *dict.find(raw_key(&key_bounds))?;
                if raw.is_null() {
                    return None;
                }
                let sub: CFDictionary = CFDictionary::wrap_under_get_rule(raw as CFDictionaryRef);
                let get = |name: &str| -> Option<f64> {
                    let k = CFString::new(name);
                    let r: *const std::ffi::c_void =
                        *sub.find(k.as_concrete_TypeRef() as *const std::ffi::c_void)?;
                    if r.is_null() {
                        return None;
                    }
                    CFNumber::wrap_under_get_rule(r as CFNumberRef).to_f64()
                };
                Some(CgBounds {
                    x: get("X")?,
                    y: get("Y")?,
                    w: get("Width")?,
                    h: get("Height")?,
                })
            };
            let mut out = Vec::new();
            for i in 0..list.len() {
                if out.len() >= crate::config::MAX_WINDOWS {
                    break;
                }
                let item = match list.get(i) {
                    Some(v) => v,
                    None => continue,
                };
                let dict_ref: *const std::ffi::c_void = *item;
                if dict_ref.is_null() {
                    continue;
                }
                let dict = CFDictionary::wrap_under_get_rule(dict_ref as CFDictionaryRef);
                let wid = as_number(&dict, raw_key(&key_number))
                    .and_then(|n| n.to_i64())
                    .filter(|v| *v > 0);
                let pid = as_number(&dict, raw_key(&key_pid))
                    .and_then(|n| n.to_i64())
                    .filter(|v| *v > 0);
                let (wid, pid) = match (wid, pid) {
                    (Some(w), Some(p)) => (w as u32, p as u32),
                    _ => continue,
                };
                if wid == 0 {
                    continue;
                }
                let layer = as_number(&dict, raw_key(&key_layer))
                    .and_then(|n| n.to_i32())
                    .unwrap_or(0);
                let onscreen = dict
                    .find(raw_key(&key_onscreen))
                    .map(|v| *v == true_ref)
                    .unwrap_or(false);
                let bounds = bounds_of(&dict).unwrap_or(CgBounds {
                    x: 0.0,
                    y: 0.0,
                    w: 0.0,
                    h: 0.0,
                });
                // Window title (`kCGWindowName`) when the window server
                // reports one; absent (nil/missing/non-string) is `None`,
                // never guessed. Only the owned helper's nonce title is
                // ever compared; foreign titles are opaque here. Skipped
                // entirely when `with_title` is false (no acquisition).
                let title = if with_title {
                    dict.find(raw_key(&key_name))
                        .and_then(|v| {
                            let raw = *v;
                            if raw.is_null() {
                                None
                            } else {
                                Some(CFString::wrap_under_get_rule(
                                    raw as core_foundation::string::CFStringRef,
                                ))
                            }
                        })
                        .map(|s| s.to_string())
                        .filter(|s| !s.is_empty())
                } else {
                    None
                };
                out.push(CgWindow {
                    wid,
                    pid,
                    layer,
                    onscreen,
                    bounds,
                    title,
                });
            }
            Ok(out)
        }
    }

    // -- Tier-2 Space-move writes (direct SLS only; SA and compat DISABLED) --
    //
    // Route determination pinned against yabai `src/space_manager.c`:
    // `space_manager_move_window_list_to_space` /
    // `space_manager_move_window_to_space` try (1) the async bridged operation
    // when `SLSPerformAsynchronousBridgedWindowManagementOperation` resolves
    // (bound in yabai `src/yabai.c`), else (2) direct
    // `SLSMoveWindowsToManagedSpace` when `workspace_use_macos_space_workaround()`
    // is false, else (3) an SA attempt first and then the compat-ID fallback
    // (`SLSSpaceSetCompatID(sid, 0x79616265)` +
    // `SLSSetWindowListWorkspace(..., 0x79616265)` + reset to `0x0`).
    // Source: https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L665-L705
    // Declarations pinned against yabai `src/misc/extern.h`:
    // https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/misc/extern.h#L63-L63
    // (`SLSMoveWindowsToManagedSpace`) and
    // https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/misc/extern.h#L96-L97
    // (compat pair). The workaround gate forcing branch (3) on macOS 15 is in
    // yabai `src/workspace.m`:
    // https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/workspace.m#L17-L26
    // Tier-2 omits every SA attempt. The compat-ID route is additionally
    // DISABLED as a mutation: `SLSSpaceSetCompatID` mutates global Space
    // metadata, only the setter is covered locally (no getter, no exact
    // preimage), the `0x79616265` workspace tag collides with yabai, and a
    // reset-to-zero on an unproven baseline is not restoration — so compat
    // stays symbol-availability diagnostics only, never a callable write.
    // The bridged async operation is likewise detected but NOT executed
    // (async ObjC API with no synchronous effect readback in surveyed
    // source). The sole callable write is direct
    // `SLSMoveWindowsToManagedSpace`: source-derived surveyed route,
    // experimental live (one gated dispatch on an owned fixture observed no
    // effect; single-shot inverse unverified), and
    // bypassed upstream on macOS 12.7+/13.6+/14.5+/15+ via
    // `workspace_use_macos_space_workaround` (yabai `src/workspace.m`),
    // which forces the SA-first branch with the compat-ID fallback after SA
    // failure. It returns void: dispatch is never effect
    // evidence — callers must read back `SLSCopySpacesForWindows`
    // membership afterwards. Live behavior of the path below is experimental
    // (observed no-effect on one owned fixture);
    // dispatch plus readbacks are the evidence.

    struct SlsMoveTable {
        move_direct: Option<unsafe extern "C" fn(i32, RawConstRef, u64)>,
        /// Compat pair presence (diagnostics only; never called).
        compat_pair_present: bool,
        bridged_async: bool,
    }

    static SLS_MOVE_TABLE: OnceLock<Option<SlsMoveTable>> = OnceLock::new();

    fn sls_move_table() -> Option<&'static SlsMoveTable> {
        SLS_MOVE_TABLE
            .get_or_init(|| {
                // SAFETY: the one transmuted symbol is null-checked before
                // transmuting to its pinned yabai declaration; compat/bridged
                // symbols are presence-checked only and never transmuted to a
                // callable type.
                unsafe {
                    let handle = libc::dlopen(SKYLIGHT.as_ptr().cast(), 0x1);
                    if handle.is_null() {
                        return None;
                    }
                    let sym = |name: &str| {
                        let cname = CString::new(name).expect("static");
                        libc::dlsym(handle, cname.as_ptr())
                    };
                    let direct = sym("SLSMoveWindowsToManagedSpace");
                    let set_id = sym("SLSSpaceSetCompatID");
                    let set_ws = sym("SLSSetWindowListWorkspace");
                    let bridged = sym("SLSPerformAsynchronousBridgedWindowManagementOperation");
                    Some(SlsMoveTable {
                        move_direct: if direct.is_null() {
                            None
                        } else {
                            Some(std::mem::transmute::<
                                *mut std::ffi::c_void,
                                unsafe extern "C" fn(i32, RawConstRef, u64),
                            >(direct))
                        },
                        compat_pair_present: !set_id.is_null() && !set_ws.is_null(),
                        bridged_async: !bridged.is_null(),
                    })
                }
            })
            .as_ref()
    }

    /// Which Tier-2 move symbols resolved. Fail-closed readback surface for
    /// route selection; never guessed. `compat_pair` reports presence only:
    /// the compat route is blocked (no exact restoration) and has no callable
    /// wrapper.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct SlsMoveSymbols {
        pub direct: bool,
        pub compat_pair: bool,
        pub bridged_async: bool,
    }

    pub fn sls_move_symbols() -> SlsMoveSymbols {
        match sls_move_table() {
            None => SlsMoveSymbols {
                direct: false,
                compat_pair: false,
                bridged_async: false,
            },
            Some(t) => SlsMoveSymbols {
                direct: t.move_direct.is_some(),
                compat_pair: t.compat_pair_present,
                bridged_async: t.bridged_async,
            },
        }
    }

    /// Direct dispatch. The API returns void: `Ok` means dispatched, never
    /// effected — the caller must read back membership.
    pub fn sls_move_windows_direct(sid: u64, wids: &[u32]) -> Result<(), String> {
        if wids.is_empty() || wids.len() > MAX_MEMBERSHIP_WINDOWS {
            return Err(format!(
                "window count out of bound 1..={MAX_MEMBERSHIP_WINDOWS}"
            ));
        }
        let table = sls_move_table().ok_or_else(|| "SLS symbols unavailable".to_owned())?;
        let direct = table
            .move_direct
            .ok_or_else(|| "SLSMoveWindowsToManagedSpace unavailable".to_owned())?;
        let base = sls_table().ok_or_else(|| "SLS symbols unavailable".to_owned())?;
        // SAFETY: window list built with maintained wrappers; the private
        // call receives a live pointer; void return carries no data.
        unsafe {
            let cid = (base.connection)();
            let numbers: Vec<CFRetained<CFNumber>> = wids
                .iter()
                .map(|wid| CFNumber::new_i32(*wid as i32))
                .collect();
            let window_list: CFRetained<CFArray<CFNumber>> =
                CFArray::from_retained_objects(&numbers);
            let list_ptr = CFRetained::as_ptr(&window_list).as_ptr() as *const std::ffi::c_void;
            direct(cid, list_ptr, sid);
            Ok(())
        }
    }

    /// Bridged-route availability: the async dispatcher presence plus the
    /// ObjC operation-class presence, diagnosed separately and never
    /// dispatched here. `available` is true only when BOTH resolve; a newer
    /// runtime may lack either, and an absent dispatcher is never called.
    /// Pinned against yabai `src/space_manager.c`
    /// `space_manager_move_window_list_to_space` (bridged first branch):
    /// https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L665-L705
    /// which resolves `SLSPerformAsynchronousBridgedWindowManagementOperation`
    /// (bound in yabai `src/yabai.c`) and looks up ObjC class
    /// `SLSBridgedMoveWindowsToManagedSpaceOperation` with selector
    /// `initWithWindows:spaceID:`. Compat stays diagnostics-only elsewhere:
    /// no exact compat-tag preimage exists (setter-only locally, yabai tag
    /// collision), so no tag getter is added and compat is never a callable
    /// write.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct SlsBridgedAvailability {
        pub fn_present: bool,
        pub class_present: bool,
        pub available: bool,
    }

    /// Diagnose bridged availability without dispatching: dispatcher-symbol
    /// presence from the move table plus a null-checked ObjC class lookup.
    /// Never calls the dispatcher, never instantiates the operation.
    pub fn sls_bridged_availability() -> SlsBridgedAvailability {
        let fn_present = sls_move_table().map(|t| t.bridged_async).unwrap_or(false);
        let class_present = bridged_operation_class_present();
        SlsBridgedAvailability {
            fn_present,
            class_present,
            available: fn_present && class_present,
        }
    }

    /// Null-checked ObjC class lookup for the bridged move operation, cached.
    /// Presence only: no selector is resolved and no instance is created.
    fn bridged_operation_class_present() -> bool {
        static PRESENT: OnceLock<bool> = OnceLock::new();
        *PRESENT.get_or_init(|| {
            // SAFETY: `objc_getClass` null-checked before transmuting to its
            // documented signature; the returned class pointer is only
            // null-checked, never dereferenced or messaged. Handles are
            // intentionally never closed (process-lifetime retention).
            unsafe {
                let getter_name = CString::new("objc_getClass").expect("static");
                let self_handle = libc::dlopen(std::ptr::null(), 0x1);
                let mut getter: Option<
                    unsafe extern "C" fn(*const std::ffi::c_char) -> *const std::ffi::c_void,
                > = None;
                if !self_handle.is_null() {
                    let sym = libc::dlsym(self_handle, getter_name.as_ptr());
                    if !sym.is_null() {
                        getter = Some(std::mem::transmute::<
                            *mut std::ffi::c_void,
                            unsafe extern "C" fn(
                                *const std::ffi::c_char,
                            )
                                -> *const std::ffi::c_void,
                        >(sym));
                    }
                }
                if getter.is_none() {
                    const OBJC: &[u8] = b"/usr/lib/libobjc.A.dylib\0";
                    let handle = libc::dlopen(OBJC.as_ptr().cast(), 0x1);
                    if !handle.is_null() {
                        let sym = libc::dlsym(handle, getter_name.as_ptr());
                        if !sym.is_null() {
                            getter = Some(std::mem::transmute::<
                                *mut std::ffi::c_void,
                                unsafe extern "C" fn(
                                    *const std::ffi::c_char,
                                )
                                    -> *const std::ffi::c_void,
                            >(sym));
                        }
                    }
                }
                let Some(get_class) = getter else {
                    return false;
                };
                let class_name =
                    CString::new("SLSBridgedMoveWindowsToManagedSpaceOperation").expect("static");
                !get_class(class_name.as_ptr()).is_null()
            }
        })
    }

    // -- Process identity (live proof, not caller claims) --

    /// Live (pid, bundle-id) pairs from NSWorkspace. Trusted `objc2`
    /// wrappers; no custom bindings. Threading: same read-only model-object
    /// class as [`frontmost_app`] (see its note); callable off the main
    /// thread for discovery snapshots.
    pub fn running_apps() -> Vec<(i32, String)> {
        use objc2_app_kit::NSWorkspace;
        let workspace = NSWorkspace::sharedWorkspace();
        let apps = workspace.runningApplications();
        let count = apps.count();
        let mut out = Vec::with_capacity(count.min(4096));
        for i in 0..count.min(4096) {
            let app = apps.objectAtIndex(i);
            let pid = app.processIdentifier();
            let bundle = app
                .bundleIdentifier()
                .map(|b| b.to_string())
                .unwrap_or_default();
            out.push((pid, bundle));
        }
        out
    }

    pub fn bundle_id_for_pid(pid: i32) -> Option<String> {
        running_apps()
            .into_iter()
            .find(|(p, _)| *p == pid)
            .map(|(_, b)| b)
            .filter(|b| !b.is_empty())
    }

    /// Live process-start token for PID-reuse binding: the
    /// `NSRunningApplication.launchDate` description for `pid` via the
    /// maintained `objc2-app-kit` binding. Plain string data only; `None`
    /// when unreadable or off the main runloop state the table needs. The
    /// scoped backend binds this alongside bundle plus executable path; a
    /// PID reuse with a new launch date refuses instead of authorizing.
    /// Directly spawned helpers have no LaunchServices entry (`None`):
    /// callers fall back to the spawn nonce as the start token.
    pub fn launch_date_for_pid(pid: i32) -> Option<String> {
        use objc2_app_kit::NSRunningApplication;
        if pid <= 1 {
            return None;
        }
        let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?;
        app.launchDate().map(|date| format!("{date:?}"))
    }

    /// Thread-safe process identity triple for PID-reuse binding: bundle ID
    /// plus launch date from one `NSRunningApplication` lookup plus the
    /// executable path. `NSRunningApplication` model-object reads involve no
    /// view, window, runloop, or notification state, so per-app AX owners
    /// may call this (the one justified AppKit exception); every other
    /// AppKit/UI read stays on the main thread. No titles, no content, no
    /// owner names — IDs and currency only. Fields are independently
    /// optional (`None` = unreadable there, never a claim); callers refuse
    /// whatever their contract requires (the scoped backend requires all
    /// three — a missing launch date refuses, never falls back).
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct NsAppIdentity {
        pub bundle: Option<String>,
        pub launch_date: Option<String>,
        pub exe: Option<String>,
    }

    pub fn ns_app_identity_for_pid(pid: i32) -> NsAppIdentity {
        use objc2_app_kit::NSRunningApplication;
        if pid <= 1 {
            return NsAppIdentity {
                bundle: None,
                launch_date: None,
                exe: None,
            };
        }
        let (bundle, launch_date) =
            match NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
                Some(app) => (
                    app.bundleIdentifier().map(|b| b.to_string()),
                    app.launchDate().map(|date| format!("{date:?}")),
                ),
                None => (None, None),
            };
        NsAppIdentity {
            bundle,
            launch_date,
            exe: process_executable_path(pid),
        }
    }

    /// Frontmost application identity (PID plus bundle ID) from the maintained
    /// `objc2-app-kit` binding. No AX involved (works without consent) and no
    /// titles/content: PIDs and bundle IDs only. `None` when unreadable.
    /// Used for focus preimage baselines and frontmost-restore verification.
    ///
    /// Threading (reviewed, not asserted): this is a read-only model-object
    /// query — shared workspace plus `NSRunningApplication` PID/bundle
    /// strings, no view, window, runloop, or notification involvement — and
    /// the maintained binding marks it accordingly: `NSWorkspace::
    /// sharedWorkspace` and `frontmostApplication(&self)` take NO
    /// `MainThreadMarker`, while genuinely main-confined entry points in the
    /// same binding version do (`NSApplication::sharedApplication(mtm)` in
    /// `objc2-app-kit 0.3.2`, which the tile controller and the owned helper
    /// both call with a live marker). Local-source precedent matches:
    /// rift `src/actor/notification_center.rs` and glide
    /// `src/actor/notification_center.rs` both call `NSWorkspace::
    /// sharedWorkspace().frontmostApplication()` from actor (non-main)
    /// contexts — glide in the same constructor that gates its
    /// `NSApplication::sharedApplication(MainThreadMarker::new().unwrap())`
    /// on the main thread, i.e. the same maintained-binding authors mark
    /// main confinement explicitly where it applies. The scoped tile route
    /// still prefers main-thread evidence where it already exists (the
    /// controller publishes frontmost via `update_focus_evidence` and the
    /// effect hook carries it to `NativeGates::session_safe`); the tap
    /// owner (`live_gate`) and the standalone focus dispatch keep this
    /// direct read under the justification above. No blind main-only claim
    /// is made either way: if the binding ever gains a marker here, every
    /// off-main caller fails to compile and must reroute.
    pub fn frontmost_app() -> Option<(i32, String)> {
        use objc2_app_kit::NSWorkspace;
        let workspace = NSWorkspace::sharedWorkspace();
        let app = workspace.frontmostApplication()?;
        let pid = app.processIdentifier();
        if pid <= 1 {
            return None;
        }
        let bundle = app
            .bundleIdentifier()
            .map(|b| b.to_string())
            .unwrap_or_default();
        Some((pid, bundle))
    }

    /// Normal application activation (no synthetic input, no window actions):
    /// asks the running application for `pid` to activate with default
    /// options. Returns whether the request was sent — focus arrival is async
    /// and MUST be verified separately (AX focused preimage or frontmost
    /// poll). Callers restrict targets to the owned helper (focus leg) and
    /// the recorded pre-frontmost app (restore leg); anything else,
    /// especially the controlling terminal's windows, is out of scope.
    pub fn activate_app(pid: i32) -> bool {
        if pid <= 1 {
            return false;
        }
        use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
        match NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
            Some(app) => app.activateWithOptions(NSApplicationActivationOptions::empty()),
            None => false,
        }
    }

    /// Public Mission Control toggle via application launch (no synthetic
    /// input, no AppleScript/System Events, no private Dock SPI). Launches
    /// (or re-activates) the public `Mission Control` app through the
    /// maintained `objc2-app-kit` `NSWorkspace::launchApplication:` binding;
    /// the return only means the launch was sent — callers must read back
    /// Mission Control AX-group presence afterwards, never claim toggle
    /// state from this boolean alone. This replaces Hammerspoon's private
    /// `_coreDesktopNotification("com.apple.expose.awake")` toggle
    /// (`extensions/spaces/libspaces.m`) and yabai's
    /// `CoreDockSendNotification(CFSTR("com.apple.expose.awake"), 0)`
    /// (`src/space_manager.c`) with a public route: launching an app needs
    /// no Accessibility trust and no Automation consent, while the AX
    /// traversal/actions above still require trust (observed, never
    /// prompted). Source: https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/libspaces.m#L241-L246
    /// Live toggle semantics (launch opens; re-launch closes) are UNPROVEN
    /// here: the close path re-reads presence and reports honestly.
    #[allow(deprecated)]
    pub fn mc_launch_toggle() -> bool {
        use objc2_app_kit::NSWorkspace;
        use objc2_foundation::NSString;
        // `launchApplication:` is deprecated upstream in favor of the async
        // `openApplicationAtURL:configuration:completionHandler:` route, which
        // needs a runloop-held completion handler — unsuitable for this
        // bounded probe. The sync launch is still public API (not SPI), so
        // the deprecation is noted, not bypassed.
        let workspace = NSWorkspace::sharedWorkspace();
        let name = NSString::from_str("Mission Control");
        workspace.launchApplication(&name)
    }

    /// Live executable path via maintained `libc::proc_pidpath`. Bounded buffer.
    pub fn process_executable_path(pid: i32) -> Option<String> {
        if pid <= 1 {
            return None;
        }
        let mut buf = vec![0u8; 4096];
        // SAFETY: scratch buffer with valid length; return value gates use.
        let ret = unsafe {
            libc::proc_pidpath(
                pid,
                buf.as_mut_ptr() as *mut std::ffi::c_void,
                buf.len() as u32,
            )
        };
        if ret > 0 {
            String::from_utf8(buf[..ret as usize].to_vec()).ok()
        } else {
            None
        }
    }

    pub fn process_exists(pid: i32) -> bool {
        if pid <= 1 {
            return false;
        }
        // SAFETY: signal 0 performs no delivery; return code is existence.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    /// Send SIGTERM once. Returns whether the signal was delivered (not
    /// whether the process exited; callers poll `process_exists`).
    pub fn terminate_process(pid: i32) -> bool {
        if pid <= 1 {
            return false;
        }
        // SAFETY: exact PID supplied by the caller after identity checks.
        unsafe { libc::kill(pid, 15) == 0 }
    }

    // -- Tier-2 SLPS focus route (yabai `window_manager_focus_window_with_raise`
    // shape, minus AXRaise) --
    //
    // Source pins (local checkouts, no web fetch):
    //   yabai `src/window_manager.c` (`window_manager_make_key_window`,
    //   `window_manager_focus_window_with_raise`) + `src/window_manager.h`
    //   (`kCPSUserGenerated 0x200`) + `src/misc/extern.h`
    //   (`_SLPSSetFrontProcessWithOptions`, `SLPSPostEventRecordTo`,
    //   plus yabai's own PSN sources `_SLPSGetFrontProcess` and
    //   `SLSGetConnectionPSN` — yabai never calls `GetProcessForPID`).
    // Dispatch order: `_SLPSSetFrontProcessWithOptions(psn, wid, 0x200)`
    // then two `SLPSPostEventRecordTo(psn, bytes)` posts with the 0xf8-byte
    // key-window record (`[0x04]=0xf8`, `[0x3a]=0x10`, wid at `0x3c`,
    // `0xff` fill at `0x20..0x30`, `[0x08]=0x01` then `0x02`).
    // PSNs here come from the Carbon SDK `GetProcessForPID(pid, psn)`
    // (`HIServices/Processes.h`: `OSStatus`, `pid_t` first, PSN out-pointer
    // second, deprecated since 10.9) — our choice, not yabai's. All three
    // symbols resolve dynamically; a single missing symbol fails closed —
    // never guessed, never SA-gated.
    //
    // Handle lifetimes: `dlopen` handles below are intentionally never
    // `dlclose`d. The transmuted function pointers stay valid only while
    // their library stays mapped, so each successful `dlopen` leaks its
    // handle by design (process-lifetime retention, no close path).

    /// `ProcessSerialNumber` bits (Carbon: two `UInt32`, high then low).
    /// `#[repr(C)]` to match the FFI layout; plain data only, never a
    /// pointer across the boundary.
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SlpsPsn {
        pub high: u32,
        pub low: u32,
    }

    /// Compile-time guard that `SlpsPsn` still matches the two-`UInt32`
    /// Carbon `ProcessSerialNumber` layout (8 bytes, no padding surprises).
    const _: () = assert!(std::mem::size_of::<SlpsPsn>() == 8);

    /// yabai `kCPSUserGenerated` (window_manager.h).
    pub const SLPS_CPS_USER_GENERATED: u32 = 0x200;
    /// Key-window event-record length (yabai `g_event_bytes` 0xf8).
    pub const SLPS_EVENT_LEN: usize = 0xf8;

    struct SlpsTable {
        set_front: unsafe extern "C" fn(*const SlpsPsn, u32, u32) -> i32,
        post_event: unsafe extern "C" fn(*const SlpsPsn, *const u8) -> i32,
        for_pid: unsafe extern "C" fn(i32, *mut SlpsPsn) -> i32,
    }

    static SLPS_TABLE: OnceLock<Option<SlpsTable>> = OnceLock::new();

    const SLPS_CANDIDATES: &[&[u8]] = &[
        b"/System/Library/Frameworks/ApplicationServices.framework/ApplicationServices\0",
        b"/System/Library/Frameworks/CoreServices.framework/CoreServices\0",
        b"/System/Library/Frameworks/Carbon.framework/Carbon\0",
        b"/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight\0",
        b"/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics\0",
    ];

    fn slps_resolve(name: &str) -> Option<*mut std::ffi::c_void> {
        // SAFETY: `dlsym` lookups null-checked by the caller; no call made
        // here. Candidate frameworks plus the default handle are probed in
        // order; a missing library/symbol is a miss, never an error.
        unsafe {
            let cname = CString::new(name).expect("static");
            let self_handle = libc::dlopen(std::ptr::null(), 0x1);
            if !self_handle.is_null() {
                let sym = libc::dlsym(self_handle, cname.as_ptr());
                if !sym.is_null() {
                    return Some(sym);
                }
            }
            for path in SLPS_CANDIDATES {
                let handle = libc::dlopen(path.as_ptr().cast(), 0x1);
                if handle.is_null() {
                    continue;
                }
                let sym = libc::dlsym(handle, cname.as_ptr());
                if !sym.is_null() {
                    return Some(sym);
                }
            }
            None
        }
    }

    fn slps_table() -> Option<&'static SlpsTable> {
        SLPS_TABLE
            .get_or_init(|| {
                // SAFETY: every symbol null-checked before transmuting to its
                // pinned yabai/Carbon signature; a single missing symbol fails
                // the whole table closed.
                unsafe {
                    let set_front = slps_resolve("_SLPSSetFrontProcessWithOptions")?;
                    let post_event = slps_resolve("SLPSPostEventRecordTo")?;
                    let for_pid = slps_resolve("GetProcessForPID")?;
                    Some(SlpsTable {
                        set_front: std::mem::transmute::<
                            *mut std::ffi::c_void,
                            unsafe extern "C" fn(*const SlpsPsn, u32, u32) -> i32,
                        >(set_front),
                        post_event: std::mem::transmute::<
                            *mut std::ffi::c_void,
                            unsafe extern "C" fn(*const SlpsPsn, *const u8) -> i32,
                        >(post_event),
                        for_pid: std::mem::transmute::<
                            *mut std::ffi::c_void,
                            unsafe extern "C" fn(i32, *mut SlpsPsn) -> i32,
                        >(for_pid),
                    })
                }
            })
            .as_ref()
    }

    /// Whether the full SLPS focus route resolved (all three symbols).
    pub fn slps_available() -> bool {
        slps_table().is_some()
    }

    /// PSN for a PID via Carbon `GetProcessForPID`. Reserved PIDs refused
    /// before any FFI; nonzero OS status is `Err` with the code.
    pub fn slps_psn_for_pid(pid: i32) -> Result<SlpsPsn, String> {
        if pid <= 1 {
            return Err(format!("refusing reserved pid {pid}"));
        }
        let table = slps_table().ok_or_else(|| "SLPS focus symbols unavailable".to_owned())?;
        let mut psn = SlpsPsn { high: 0, low: 0 };
        // SAFETY: resolved non-null symbol; valid out-pointer; status gates
        // the read.
        let status = unsafe { (table.for_pid)(pid, std::ptr::addr_of_mut!(psn)) };
        if status == 0 {
            Ok(psn)
        } else {
            Err(format!("GetProcessForPID pid {pid}: OS status {status}"))
        }
    }

    /// `_SLPSSetFrontProcessWithOptions(psn, wid, mode)`. `Ok` means
    /// dispatched (CGError 0) — focus arrival is async and MUST be verified
    /// separately (workspace + AX readback). Nonzero CGError is `Err`.
    pub fn slps_set_front(psn: &SlpsPsn, wid: u32, mode: u32) -> Result<(), i32> {
        let table = slps_table().ok_or(-25200)?;
        // SAFETY: resolved non-null symbol; `psn` is a live caller-owned
        // value; wid/mode are plain data; status is the only effect read.
        let status = unsafe { (table.set_front)(psn as *const SlpsPsn, wid, mode) };
        if status == 0 { Ok(()) } else { Err(status) }
    }

    /// `SLPSPostEventRecordTo(psn, bytes)`. The record must be exactly
    /// [`SLPS_EVENT_LEN`] bytes (yabai 0xf8); anything else is refused before
    /// any FFI. `Ok` means dispatched, never effected.
    pub fn slps_post_event(psn: &SlpsPsn, bytes: &[u8; SLPS_EVENT_LEN]) -> Result<(), i32> {
        let table = slps_table().ok_or(-25200)?;
        // SAFETY: resolved non-null symbol; both pointers live for the call;
        // status is the only effect read.
        let status = unsafe { (table.post_event)(psn as *const SlpsPsn, bytes.as_ptr()) };
        if status == 0 { Ok(()) } else { Err(status) }
    }

    // -- Read-only SLPS front-process route (yabai
    // `window_manager_focused_application` shape, read half only) --
    //
    // Source pins (local checkouts, no web fetch):
    //   yabai `src/misc/extern.h:76`
    //   (`extern OSStatus _SLPSGetFrontProcess(ProcessSerialNumber *psn)`)
    //   plus its use in `src/window_manager.c:1343-1347`
    //   (`_SLPSGetFrontProcess(&psn)` then `GetProcessPID(&psn, &pid)`) and
    //   `src/process_manager.c:248` (same pair for the front PID).
    //   `GetProcessPID` itself is Carbon `HIServices/Processes.h`
    //   (`OSStatus`, PSN in-pointer first, `pid_t` out-pointer second,
    //   deprecated since 10.9) — the same Carbon precedent as the existing
    //   `GetProcessForPID` choice above, not yabai's (yabai never calls PID
    //   mapping itself). Both symbols resolve dynamically via the same
    //   candidate libraries; a single missing symbol fails this table closed
    //   while the dispatch table above is unaffected (separate
    //   `OnceLock`, separate availability). Both calls are synchronous
    //   read-only getters with no wait parameter — the same bounded class as
    //   [`frontmost_app`] and [`session_state`]. No focus is moved, no event
    //   is posted, no attribute is written on any path here.

    struct SlpsFrontTable {
        get_front: unsafe extern "C" fn(*mut SlpsPsn) -> i32,
        pid_for_psn: unsafe extern "C" fn(*const SlpsPsn, *mut libc::pid_t) -> i32,
    }

    static SLPS_FRONT_TABLE: OnceLock<Option<SlpsFrontTable>> = OnceLock::new();

    fn slps_front_table() -> Option<&'static SlpsFrontTable> {
        SLPS_FRONT_TABLE
            .get_or_init(|| {
                // SAFETY: every symbol null-checked before transmuting to its
                // pinned signature; a single missing symbol fails this table
                // closed (dispatch availability is untouched).
                unsafe {
                    let get_front = slps_resolve("_SLPSGetFrontProcess")?;
                    let pid_for_psn = slps_resolve("GetProcessPID")?;
                    Some(SlpsFrontTable {
                        get_front: std::mem::transmute::<
                            *mut std::ffi::c_void,
                            unsafe extern "C" fn(*mut SlpsPsn) -> i32,
                        >(get_front),
                        pid_for_psn: std::mem::transmute::<
                            *mut std::ffi::c_void,
                            unsafe extern "C" fn(*const SlpsPsn, *mut libc::pid_t) -> i32,
                        >(pid_for_psn),
                    })
                }
            })
            .as_ref()
    }

    /// Whether the read-only front-process route resolved (both symbols).
    /// Independent of [`slps_available`] (dispatch may resolve without this
    /// and vice versa).
    pub fn slps_front_available() -> bool {
        slps_front_table().is_some()
    }

    /// Front-process PID via `_SLPSGetFrontProcess` + `GetProcessPID`.
    /// Reserved PIDs refused before return; nonzero OS statuses are `Err`
    /// with the code. Read-only; callable from any thread (same class as
    /// [`frontmost_app`]).
    pub fn slps_front_pid() -> Result<i32, String> {
        let table = slps_front_table()
            .ok_or_else(|| "SLPS front-process symbols unavailable".to_owned())?;
        let mut psn = SlpsPsn { high: 0, low: 0 };
        // SAFETY: resolved non-null symbol; valid out-pointer; status gates
        // the read.
        let status = unsafe { (table.get_front)(std::ptr::addr_of_mut!(psn)) };
        if status != 0 {
            return Err(format!("_SLPSGetFrontProcess: OS status {status}"));
        }
        let mut pid: libc::pid_t = 0;
        // SAFETY: resolved non-null symbol; PSN is a live local, PID pointer
        // is a valid out-pointer; status gates the read.
        let status = unsafe { (table.pid_for_psn)(&psn, std::ptr::addr_of_mut!(pid)) };
        if status != 0 {
            return Err(format!("GetProcessPID: OS status {status}"));
        }
        if pid <= 1 {
            return Err(format!("front-process pid {pid} is reserved; refusing"));
        }
        Ok(pid as i32)
    }

    /// Experimental corroborated focus preimage with provenance.
    /// Diagnostic-only: production preimage and agreement reads stay strict
    /// systemwide and never call this. The system-wide
    /// [`ax_focused_preimage_detailed`] route is tried first: when it
    /// succeeds its `(pid, wid)` returns with route
    /// [`super::PreimageRoute::Systemwide`]. The experimental fallback
    /// engages ONLY for the exactly observed system-wide service failure
    /// (see [`super::preimage_fallback_trigger`]): the independent
    /// read-only front-process PID above, required to equal the NSWorkspace
    /// frontmost observed before AND after (stable, never fabricated from
    /// workspace alone), plus the direct per-app legs on that PID in the
    /// canonical order — `AXFrontmost` BEFORE the window legs AND AFTER,
    /// both required `Some(true)` and stable, `AXFocusedWindow` joined to
    /// a nonzero WID (per-element timeout first), AX identity PID equal —
    /// plus a fresh title-free CG owner join. Any before/after
    /// disagreement is instability data that refuses (never a proven host
    /// claim), as is any PID disagreement, workspace instability mid-read,
    /// a missing/zero window, or a lost CG join, with the leg named (never
    /// a guess). No fallback when systemwide SUCCEEDS but disagrees; no
    /// dispatch, activation, taps, synthetic input, Space moves, or
    /// attribute writes on any path — the finite messaging timeouts (2s,
    /// as in the preimage) are the only AX setting touched. Every
    /// `AxElement` is constructed, used, and dropped on the calling thread
    /// (same thread-affinity contract as the preimage). Unproven: this
    /// route lacks acceptance and is never production evidence.
    pub fn ax_focused_preimage_evidence() -> Result<super::PreimageEvidence, String> {
        match ax_focused_preimage_detailed() {
            Ok((pid, wid)) => Ok(super::PreimageEvidence {
                pid,
                wid,
                route: super::PreimageRoute::Systemwide,
                detail: "systemwide AXFocusedApplication success".to_owned(),
            }),
            Err(system_err) if !super::preimage_fallback_trigger(&system_err) => Err(system_err),
            Err(system_err) => {
                let (ws_before, ws_bundle) = frontmost_app().ok_or_else(|| {
                    format!("workspace frontmost unreadable ({system_err}); refusing")
                })?;
                if ws_before <= 1 {
                    return Err(format!(
                        "workspace frontmost pid {ws_before} is reserved; refusing"
                    ));
                }
                let psn_pid = slps_front_pid().map_err(|e| format!("{e} ({system_err})"))?;
                if psn_pid != ws_before {
                    return Err(format!(
                        "SLPS front pid {psn_pid} != workspace frontmost pid {ws_before} ({system_err}); refusing without agreement"
                    ));
                }
                let app = AxElement::application(psn_pid).ok_or_else(|| {
                    format!("no AX element for pid {psn_pid} ({system_err}); refusing")
                })?;
                let app_timeout = ax_set_messaging_timeout(&app, 2.0);
                if app_timeout != 0 {
                    return Err(format!(
                        "frontmost application AX messaging timeout unset: {} ({system_err})",
                        ax_error_name(app_timeout)
                    ));
                }
                let mut ax_pid: libc::pid_t = 0;
                // SAFETY: live element; valid out-pointer; status gates read.
                let pid_err = unsafe {
                    app.inner
                        .pid(NonNull::new(std::ptr::addr_of_mut!(ax_pid)).expect("non-null"))
                };
                if pid_err.0 != 0 {
                    return Err(format!(
                        "frontmost AX identity pid unreadable: {} ({system_err})",
                        ax_error_name(pid_err.0)
                    ));
                }
                if ax_pid as i32 != psn_pid {
                    return Err(format!(
                        "AX identity pid {} != SLPS front pid {psn_pid} ({system_err}); refusing without agreement",
                        ax_pid
                    ));
                }
                // Canonical acquisition: frontmost BEFORE the window legs,
                // shared reader with the diagnose direct leg.
                let (front_before_status, front_before) = ax_frontmost_read(&app);
                if front_before != Some(true) {
                    return Err(format!(
                        "AXFrontmost before-window-legs {front_before_status} ({front_before:?}) for pid {psn_pid} ({system_err}); refusing"
                    ));
                }
                let window_value = app.copy_attribute("AXFocusedWindow").map_err(|code| {
                    format!(
                        "AXFocusedWindow unreadable: {} ({system_err})",
                        ax_error_name(code)
                    )
                })?;
                let window_inner: CFRetained<AXUIElement> = window_value
                    .downcast()
                    .map_err(|_| format!("AXFocusedWindow is not an AXUIElement ({system_err})"))?;
                let window = AxElement {
                    inner: window_inner,
                    _not_send: std::marker::PhantomData,
                };
                // Per-element messaging timeout precedes the join: a
                // refused timeout stops this candidate with its cause,
                // never an unbounded join.
                let wid = ax_window_join_with_timeout(&window)
                    .map_err(|e| format!("AXFocusedWindow join: {e} ({system_err})"))?;
                if wid == 0 {
                    return Err(format!("AX focused wid is 0 ({system_err}); refusing"));
                }
                // Canonical acquisition: frontmost AFTER the window legs;
                // both sides must read true/stable.
                let (front_after_status, front_after) = ax_frontmost_read(&app);
                if front_after != Some(true) {
                    return Err(format!(
                        "AXFrontmost after-window-legs {front_after_status} ({front_after:?}) for pid {psn_pid} ({system_err}); refusing without stability"
                    ));
                }
                let cg_owned = cg_window_list_basic()
                    .map(|list| list.iter().any(|w| w.wid == wid && w.pid == psn_pid as u32))
                    .map_err(|e| format!("CG owner join unreadable: {e} ({system_err})"))?;
                if !cg_owned {
                    return Err(format!(
                        "CG join disagrees: wid {wid} not owned by pid {psn_pid} ({system_err}); refusing"
                    ));
                }
                let (ws_after, _) = frontmost_app()
                    .ok_or_else(|| format!("workspace frontmost unreadable after corroboration ({system_err}); refusing"))?;
                if ws_after != ws_before {
                    return Err(format!(
                        "workspace frontmost moved during corroboration ({ws_before} -> {ws_after}); refusing"
                    ));
                }
                if !super::preimage_fallback_agrees(
                    psn_pid,
                    Some(ws_before),
                    Some(ws_after),
                    front_before,
                    front_after,
                    Some(ax_pid as i32),
                    Some(wid),
                    cg_owned,
                ) {
                    return Err(format!(
                        "fallback agreement recheck failed for pid {psn_pid} ({system_err}); refusing"
                    ));
                }
                Ok(super::PreimageEvidence {
                    pid: psn_pid,
                    wid: Some(wid),
                    route: super::PreimageRoute::SlpsFrontCorroborated,
                    detail: format!(
                        "systemwide ({system_err}) superseded by SLPS front pid {psn_pid} == workspace '{ws_bundle}' stable, AXFrontmost before+after true/stable, AXFocusedWindow wid {wid}, CG owned"
                    ),
                })
            }
        }
    }

    // -- Selected focus oracle (production): SLPS + NSWorkspace foreground,
    // direct per-app focused window --
    //
    // FOREGROUND = the read-only SLPS front-process PID (`_SLPSGetFrontProcess`
    // + `GetProcessPID`) agreeing with the NSWorkspace `frontmostApplication`
    // PID, observed before AND after the window legs (stability revalidation).
    // FOCUSED WINDOW = the direct per-app AX read on exactly that PID:
    // `AXFocusedWindow` joined via `_AXUIElementGetWindow`, falling back to
    // `AXMainWindow` only when the focused leg is unavailable/missing — then a
    // fresh title-free CG owner join on the selected `(pid, wid)`. The
    // system-wide `AXFocusedApplication` read is diagnostic-only (kept raw in
    // the diagnose legs) and `AXFrontmost` never gates: neither is queried
    // here. Every `AxElement` is constructed, used, and dropped on the calling
    // thread; every join carries its own finite messaging timeout first (a
    // refused timeout stops that leg with its cause, never an unbounded
    // join). No dispatch, activation, taps, synthetic input, Space moves, or
    // attribute writes on any path — the finite messaging timeouts (2s, as in
    // the preimage) are the only AX setting touched.

    /// Direct per-application focused-window read for exactly `pid`: the
    /// `AXFocusedWindow` join plus, only when that leg is
    /// unavailable/missing, the `AXMainWindow` fallback join, with the AX
    /// identity PID and the pure [`super::select_direct_window`] selection.
    /// No system-wide AX, no `AXFrontmost`: production selection over plain
    /// data via [`super::selected_focus_agrees`]. IDs and status names only.
    pub fn ax_direct_focused_read(pid: i32) -> super::DirectFocusRead {
        let blank = super::DirectFocusRead {
            pid,
            ax_pid: None,
            focused_status: "unreached".to_owned(),
            focused_wid: None,
            main_status: "unreached".to_owned(),
            main_wid: None,
            selected_wid: None,
            selected_source: "none".to_owned(),
        };
        if pid <= 1 {
            return super::DirectFocusRead {
                focused_status: "refused-reserved-pid".to_owned(),
                main_status: "refused-reserved-pid".to_owned(),
                ..blank
            };
        }
        let Some(app) = AxElement::application(pid) else {
            return super::DirectFocusRead {
                focused_status: "unavailable".to_owned(),
                main_status: "unavailable".to_owned(),
                ..blank
            };
        };
        if ax_set_messaging_timeout(&app, 2.0) != 0 {
            return super::DirectFocusRead {
                focused_status: "unreached (timeout unset)".to_owned(),
                main_status: "unreached (timeout unset)".to_owned(),
                ..blank
            };
        }
        let mut ax_pid_value: libc::pid_t = 0;
        // SAFETY: live element; valid out-pointer; status gates the read.
        let pid_err = unsafe {
            app.inner
                .pid(NonNull::new(std::ptr::addr_of_mut!(ax_pid_value)).expect("non-null"))
        };
        let ax_pid = if pid_err.0 == 0 {
            Some(ax_pid_value as i32)
        } else {
            None
        };
        // Focused leg first; main leg only fills a missing focused leg.
        // Each join carries its own per-element window timeout inside
        // [`ax_single_window_join`].
        let (focused_status, focused_wid) = ax_single_window_join(&app, "AXFocusedWindow");
        let (main_status, main_wid) = ax_single_window_join(&app, "AXMainWindow");
        let (selected_wid, selected_source) = super::select_direct_window(focused_wid, main_wid);
        super::DirectFocusRead {
            pid,
            ax_pid,
            focused_status,
            focused_wid,
            main_status,
            main_wid,
            selected_wid,
            selected_source: selected_source.to_owned(),
        }
    }

    /// Selected-oracle focus evidence with provenance: the SLPS front PID
    /// agreeing with the NSWorkspace frontmost before AND after the direct
    /// window legs, the direct per-app selection on that PID (focused, else
    /// main fallback), the AX identity agreement, and the fresh CG owner
    /// join. Any foreground disagreement or instability, identity mismatch,
    /// missing/zero window, or lost CG join refuses with the leg named
    /// (never a guess). Production evidence; the raw system-wide read stays
    /// available separately for diagnosis only.
    pub fn ax_selected_focus_evidence() -> Result<super::PreimageEvidence, String> {
        let (ws_before, ws_bundle) =
            frontmost_app().ok_or_else(|| "workspace frontmost unreadable".to_owned())?;
        if ws_before <= 1 {
            return Err(format!(
                "workspace frontmost pid {ws_before} is reserved; refusing"
            ));
        }
        let slps_before = slps_front_pid()?;
        if slps_before != ws_before {
            return Err(format!(
                "SLPS front pid {slps_before} != workspace frontmost pid {ws_before}; refusing without agreement"
            ));
        }
        let read = ax_direct_focused_read(ws_before);
        match read.ax_pid {
            Some(ax_pid) if ax_pid == ws_before => {}
            other => {
                return Err(format!(
                    "AX identity pid {other:?} != frontmost pid {ws_before} (focused {} main {}); refusing without agreement",
                    read.focused_status, read.main_status
                ));
            }
        }
        let Some(wid) = read.selected_wid else {
            return Err(format!(
                "direct focused window none for pid {ws_before} (focused {} main {}); refusing without a restorable wid",
                read.focused_status, read.main_status
            ));
        };
        if wid == 0 {
            return Err(format!(
                "direct focused wid is 0 for pid {ws_before}; refusing"
            ));
        }
        let cg_owned = cg_window_list_basic()
            .map(|list| {
                list.iter()
                    .any(|w| w.wid == wid && w.pid == ws_before as u32)
            })
            .map_err(|e| format!("CG owner join unreadable: {e}"))?;
        if !cg_owned {
            return Err(format!(
                "CG join disagrees: wid {wid} not owned by pid {ws_before}; refusing"
            ));
        }
        // Foreground revalidation AFTER the window legs: a move mid-read
        // refuses instead of joining a stale window to a new frontmost.
        let (ws_after, _) = frontmost_app().ok_or_else(|| {
            "workspace frontmost unreadable after window legs; refusing".to_owned()
        })?;
        if ws_after != ws_before {
            return Err(format!(
                "workspace frontmost moved during direct read ({ws_before} -> {ws_after}); refusing"
            ));
        }
        let slps_after = slps_front_pid()?;
        if slps_after != ws_before {
            return Err(format!(
                "SLPS front pid moved during direct read ({slps_before} -> {slps_after}); refusing"
            ));
        }
        if !super::selected_focus_agrees(
            ws_before as u32,
            wid,
            Some(slps_after),
            Some(ws_after),
            Some(wid),
            cg_owned,
        ) {
            return Err(format!(
                "selected agreement recheck failed for pid {ws_before} wid {wid}; refusing"
            ));
        }
        let (route, leg) = match read.selected_source.as_str() {
            "main" => (
                super::PreimageRoute::SlpsFrontDirectMain,
                format!(
                    "AXMainWindow wid {wid} (AXFocusedWindow {})",
                    read.focused_status
                ),
            ),
            _ => (
                super::PreimageRoute::SlpsFrontDirectFocused,
                format!("AXFocusedWindow wid {wid}"),
            ),
        };
        Ok(super::PreimageEvidence {
            pid: ws_before,
            wid: Some(wid),
            route,
            detail: format!(
                "SLPS front pid {slps_before} == workspace '{ws_bundle}' stable before+after, {leg}, CG owned"
            ),
        })
    }

    // -- Owned probe window (AppKit boundary; main thread only) --

    /// Pump the current thread's runloop once, bounded, via the maintained
    /// `objc2-core-foundation` wrapper. The single `unsafe` is the
    /// extern-static default-mode read.
    pub fn pump_runloop_once(millis: u64) {
        let mode = unsafe { kCFRunLoopDefaultMode };
        CFRunLoop::run_in_mode(mode, millis.min(1000) as f64 / 1000.0, false);
    }

    /// Drain pending AppKit events without blocking: dequeue whatever is
    /// available (distant past = return immediately, never wait) and dispatch
    /// each through the application. Without this, machinery that arrives as
    /// application events — notably activation requests — is never serviced:
    /// the request reports sent while focus never transfers and AX focus
    /// legs fail. Bounded: at most 64 dequeues per call (`None` ends the
    /// drain early), zero sleep. Maintained `objc2-app-kit` wrappers only.
    pub fn drain_app_events(app: &objc2_app_kit::NSApplication) {
        drain_app_events_inner(app, false);
    }

    /// Same drain as [`drain_app_events`] plus downstream key-delivery
    /// counting into the process-local [`KeyCounts`] (helper fixture only:
    /// the owned probe window's own loop uses this so posted test chords
    /// are observable as AppKit delivery proof; every other caller keeps
    /// the non-counting drain and its counters stay zero and unread).
    pub fn drain_app_events_counted(app: &objc2_app_kit::NSApplication) {
        drain_app_events_inner(app, true);
    }

    fn drain_app_events_inner(app: &objc2_app_kit::NSApplication, count: bool) {
        use objc2_app_kit::NSEventMask;
        use objc2_foundation::{NSDate, NSDefaultRunLoopMode};
        // SAFETY: extern-static default-mode read (same pattern as
        // `pump_runloop_once`); the static is a framework constant.
        let mode = unsafe { NSDefaultRunLoopMode };
        let expiration = NSDate::distantPast();
        for _ in 0..64 {
            let event = app.nextEventMatchingMask_untilDate_inMode_dequeue(
                NSEventMask::Any,
                Some(&expiration),
                mode,
                true,
            );
            match event {
                Some(event) => {
                    if count {
                        count_key_event(&event);
                    }
                    app.sendEvent(&event)
                }
                None => break,
            }
        }
    }

    /// Downstream delivery counters for the owned helper's own key events
    /// (`input-session` fixture only): KeyDown/KeyUp carrying the probe
    /// keycode (`0x04`, the approved `h` chord position) bucketed by the
    /// device-independent modifier nibble (`(flags & 0x1E0000) >> 17`, 16
    /// buckets with bit0=Shift bit1=Ctrl bit2=Alt bit3=Cmd — the session
    /// reads exact/extra/plain classes off with its own want/mask), plus totals for all other keys (counts only, never
    /// codes). Process-local and cumulative; content-free by construction
    /// (no text, no titles, no non-probe codes).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct KeyCounts {
        pub h_down: [u64; 16],
        pub h_up: [u64; 16],
        pub other_down: u64,
        pub other_up: u64,
    }

    impl KeyCounts {
        const fn zero() -> Self {
            Self {
                h_down: [0; 16],
                h_up: [0; 16],
                other_down: 0,
                other_up: 0,
            }
        }
    }

    static KEY_COUNTS: std::sync::Mutex<KeyCounts> = std::sync::Mutex::new(KeyCounts::zero());

    fn count_key_event(event: &objc2_app_kit::NSEvent) {
        use objc2_app_kit::NSEventType;
        const PROBE_KEY: u16 = 0x04;
        const MOD_MASK: u64 = 0x001E_0000;
        let raw = event.r#type().0 as u64;
        let down = raw == NSEventType::KeyDown.0 as u64;
        let up = raw == NSEventType::KeyUp.0 as u64;
        if !down && !up {
            return;
        }
        let key_code = event.keyCode();
        let flags = event.modifierFlags().bits() as u64;
        let Ok(mut counts) = KEY_COUNTS.lock() else {
            return;
        };
        if key_code == PROBE_KEY {
            let bucket = ((flags & MOD_MASK) >> 17) as usize;
            if down {
                counts.h_down[bucket] += 1;
            } else {
                counts.h_up[bucket] += 1;
            }
        } else if down {
            counts.other_down += 1;
        } else {
            counts.other_up += 1;
        }
    }

    /// Cumulative downstream key-delivery snapshot for this process (the
    /// owned helper's own AppKit arrivals; the controller never enables
    /// counting so its snapshot stays zero). The holder diffs successive
    /// snapshots around each tagged post.
    #[must_use]
    pub fn key_counts_snapshot() -> KeyCounts {
        KEY_COUNTS
            .lock()
            .map(|counts| *counts)
            .unwrap_or(KeyCounts::zero())
    }

    /// One owned Space-join self-action poll. Returns a stdout marker when a
    /// nonce-matching trigger was consumed (or `None` when no trigger is
    /// present). Acts SOLELY on the passed owned `window` — never a caller
    /// WID, never another process. Gates before any effect: the private
    /// session dir must verify (UID/`0700`/non-symlink, same as the
    /// controller side) and the window-server session must read unlocked
    /// (`CGSessionCopyCurrentDictionary` legs via [`session_state`]) —
    /// unknown/locked refuses with no effect and no consume. Steps: consume
    /// the request file once (never replayed), capture the
    /// `collectionBehavior` preimage, set `preimage | MoveToActiveSpace`,
    /// `makeKeyAndOrderFront` + self-activate, pump bounded for the
    /// window-server move, restore the exact preimage, verify, write the
    /// atomic result file echoing `{session nonce, request_id}`. A forged
    /// nonce body is ignored (left in place; the authenticated controller
    /// times out rather than this helper acting on it).
    fn poll_space_join_once(
        window: &objc2_app_kit::NSWindow,
        app: &objc2_app_kit::NSApplication,
        own_nonce: &str,
    ) -> Option<String> {
        use objc2_app_kit::NSWindowCollectionBehavior;
        // Same private-dir proof the controller holds before publishing: a
        // deviant dir fails this poll closed (no act, no consume).
        if crate::probe_target::ensure_private_session_dir().is_err() {
            return None;
        }
        // Session-lock gate immediately before acting: unknown or locked
        // refuses with no effect (never a blind activation under lock).
        if !session_state().unlocked() {
            return None;
        }
        let request = crate::probe_target::space_join_request_path(own_nonce);
        // Bounded regular-file read: no symlinks, plain file only, 4 KiB
        // cap (a mid-write partial body fails auth below and is left for a
        // later poll — never consumed, never deleted).
        let meta = std::fs::symlink_metadata(&request).ok()?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            return None;
        }
        if meta.len() > 4096 {
            return None;
        }
        let body = std::fs::read_to_string(&request).ok()?;
        let request_id = match crate::probe_target::space_join_request_auth(&body, own_nonce) {
            Ok(id) => id,
            Err(_) => return None,
        };
        // Consume once before acting: a crash mid-action leaves no trigger
        // to replay and no result; the controller reports unknown rather
        // than this helper double-acting.
        let _ = std::fs::remove_file(&request);
        // SAFETY: live owned window + live app on the helper main thread,
        // maintained `objc2-app-kit` wrappers only; no foreign pointers.
        let preimage = window.collectionBehavior();
        let pre_bits = preimage.bits() as u64;
        let joined = preimage | NSWindowCollectionBehavior::MoveToActiveSpace;
        window.setCollectionBehavior(joined);
        window.makeKeyAndOrderFront(None);
        app.activate();
        objc2::rc::autoreleasepool(|_| {
            drain_app_events(app);
            pump_runloop_once(500);
        });
        window.setCollectionBehavior(preimage);
        objc2::rc::autoreleasepool(|_| {
            drain_app_events(app);
            pump_runloop_once(200);
        });
        let restored_bits = window.collectionBehavior().bits() as u64;
        let restored_ok = crate::probe_target::space_join_restored_ok(pre_bits, restored_bits);
        let result = crate::probe_target::SpaceJoinResult {
            nonce: own_nonce.to_owned(),
            request_id,
            preimage_bits: pre_bits,
            restored_bits,
            restored_verified: restored_ok,
        };
        let body = serde_json::to_string(&result).unwrap_or_default();
        if !body.is_empty() {
            // Unique temp per action (name carries nonce + request_id, both
            // exactly run-owned): `create_new` never truncates a survivor,
            // mode `0600` at creation, then atomic rename onto the result
            // path — no predictable remove-then-write race.
            let result_path = crate::probe_target::space_join_result_path(own_nonce);
            let tmp = result_path.with_extension("tmp");
            let _ = std::fs::remove_file(&tmp);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&tmp)
                {
                    use std::io::Write;
                    if file.write_all(body.as_bytes()).is_ok() && file.sync_all().is_ok() {
                        let _ = std::fs::rename(&tmp, &result_path);
                    } else {
                        let _ = std::fs::remove_file(&tmp);
                    }
                }
            }
            #[cfg(not(unix))]
            {
                if std::fs::write(&tmp, &body).is_ok() {
                    let _ = std::fs::rename(&tmp, &result_path);
                }
            }
        }
        Some(body)
    }

    /// Owned probe window runner. Creates the window on the calling (main)
    /// thread with the given title (the caller's per-run nonce title),
    /// pumps the runloop until it is serviced, invokes `ready`, and
    /// only then applies the optional stall (so a hung target is a *live*
    /// unresponsive window, not a missing one). Never touches other windows.
    /// A nonzero `close_window_ms` closes only this window at that deadline
    /// past `ready` (owned teardown stimulus for the destroyed leg) while the
    /// process keeps running to its deadline.
    ///
    /// No explicit activation at spawn: the window is shown with `orderFront:`
    /// (never `makeKeyAndOrderFront:`) and the app never self-activates.
    /// Regular-policy launch may nevertheless acquire focus, so its holder
    /// captures the original preimage before spawning and restores it before
    /// the measured focus transfer. The default `Accessory` policy (no Dock
    /// icon) keeps background holders out of the Dock; the opt-in `regular_policy`
    /// fixture runs as `Regular`, intending the window server to report the
    /// helper as frontmost after an explicit SLPS focus dispatch
    /// (`focus-session` only — no activation API, self-activation, taps, or
    /// keys anywhere). Bundled helpers (running inside the signed helper
    /// `.app`) always take `Regular` regardless of the flag (decided by the
    /// helper binary from its own bundle ID): a bundled helper is a real
    /// user-visible app, not a background holder. Experimental, live
    /// unproven: whether full agreement actually holds is measured by the
    /// session, never assumed here.
    // Eleven flat fixture params (three opt-ins more than the lint threshold);
    // bundling them into a struct would churn the stub + helper binary for
    // no behavioral gain, so the lint is allowed here with this note.
    #[allow(clippy::too_many_arguments)]
    pub fn run_owned_probe_window(
        seconds: u64,
        hung_ms: u64,
        title: &str,
        fixed_size: bool,
        defer_window_ms: u64,
        close_window_ms: u64,
        regular_policy: bool,
        transient_panel_ms: u64,
        transient_sheet_ms: u64,
        space_join_poll: bool,
        ready: &dyn Fn(),
    ) -> Result<(), String> {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{
            NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSWindow,
            NSWindowStyleMask,
        };
        use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

        let mtm = MainThreadMarker::new()
            .ok_or_else(|| "owned probe window must run on the main thread".to_owned())?;
        // SAFETY: fresh `alloc()` on the main thread; no pointers escape.
        let app = NSApplication::sharedApplication(mtm);
        if regular_policy {
            app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
        } else {
            app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        }
        // Complete the launch sequence without entering `run`: a bare
        // `sharedApplication` plus manual runloop pumping shows the window at
        // the window-server level (CG sees it) but never registers the app
        // with the accessibility system (AX sees zero windows), which fails
        // every owned AX leg. `finishLaunching` publishes the AX hierarchy
        // while the manual bounded pump below keeps lifetime control. It does
        // not activate, so focus is never stolen.
        app.finishLaunching();
        // Deferred creation for the creation-notification leg: pump bounded
        // (events drained, runloop sliced) with no window yet, so a
        // pre-window observer deterministically covers the creation below.
        // Still no focus theft (nothing to show until the window exists).
        let defer_left = defer_window_ms.min(20_000);
        let defer_deadline = Instant::now() + Duration::from_millis(defer_left);
        while Instant::now() < defer_deadline {
            // Each pump iteration drains an autorelease pool: a manual pump
            // without one never finalizes deferred AppKit teardown (e.g. a
            // programmatic window close announces destruction but its surface
            // lingers), and leaks autoreleased objects for the whole run.
            // Real NSApplication loops drain per event; this is that hygiene
            // for the manual loop.
            objc2::rc::autoreleasepool(|_| {
                drain_app_events(&app);
                pump_runloop_once(50);
            });
        }
        let frame = NSRect::new(NSPoint::new(200.0, 200.0), NSSize::new(480.0, 320.0));
        // Fault control for the live partial-write leg: without `Resizable`
        // the window accepts AX position writes but refuses AX size writes,
        // so a frame transaction against it must restore the exact preimage.
        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | if fixed_size {
                NSWindowStyleMask::empty()
            } else {
                NSWindowStyleMask::Resizable
            };
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                mtm.alloc::<NSWindow>(),
                frame,
                style,
                NSBackingStoreType(2),
                false,
            )
        };
        window.setTitle(&NSString::from_str(title));
        window.orderFront(None);
        objc2::rc::autoreleasepool(|_| {
            pump_runloop_once(200);
        });
        ready();
        if hung_ms > 0 {
            std::thread::sleep(Duration::from_millis(hung_ms.min(20_000)));
        }
        // Owned transient fixtures (opt-in, disposable, helper-only): a
        // native `NSPanel` utility window (dialog subrole natively, no role
        // spoof) and/or a true attached sheet on the main owned window
        // (`beginSheet` with no completion work). Both open immediately after
        // `ready` and close only themselves at their deadline while the main
        // owned window stays alive to its seconds deadline. Markers carry
        // counts/visibility only, never titles.
        use objc2_app_kit::NSPanel;
        let panel: Option<objc2::rc::Retained<NSPanel>> = if transient_panel_ms > 0 {
            let panel_frame = NSRect::new(NSPoint::new(300.0, 300.0), NSSize::new(320.0, 180.0));
            let panel_style = NSWindowStyleMask::Titled
                | NSWindowStyleMask::Closable
                | NSWindowStyleMask::UtilityWindow;
            let created = NSPanel::initWithContentRect_styleMask_backing_defer(
                mtm.alloc::<NSPanel>(),
                panel_frame,
                panel_style,
                NSBackingStoreType(2),
                false,
            );
            created.setTitle(&NSString::from_str(&format!("{title} panel")));
            created.setFloatingPanel(true);
            created.orderFront(None);
            objc2::rc::autoreleasepool(|_| {
                pump_runloop_once(200);
            });
            let own_windows = cg_window_list_all()
                .map(|list| {
                    let me = std::process::id();
                    list.iter().filter(|w| w.pid == me).count()
                })
                .unwrap_or(usize::MAX);
            println!(
                "{}",
                serde_json::json!({"transient": "panel", "opened": true, "windows": own_windows, "visible": created.isVisible()})
            );
            let _ = std::io::Write::flush(&mut std::io::stdout());
            Some(created)
        } else {
            None
        };
        let sheet: Option<objc2::rc::Retained<NSWindow>> = if transient_sheet_ms > 0 {
            let sheet_frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(300.0, 140.0));
            let sheet_style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable;
            let created: objc2::rc::Retained<NSWindow> = unsafe {
                NSWindow::initWithContentRect_styleMask_backing_defer(
                    mtm.alloc::<NSWindow>(),
                    sheet_frame,
                    sheet_style,
                    NSBackingStoreType(2),
                    false,
                )
            };
            created.setTitle(&NSString::from_str(&format!("{title} sheet")));
            // SAFETY: live parent + live sheet in maintained wrappers; `None`
            // completion performs no work. Presents a true attached sheet
            // (native `AXSheet` role on the sheet element) without activation.
            window.beginSheet_completionHandler(&created, None);
            objc2::rc::autoreleasepool(|_| {
                pump_runloop_once(300);
            });
            let own_windows = cg_window_list_all()
                .map(|list| {
                    let me = std::process::id();
                    list.iter().filter(|w| w.pid == me).count()
                })
                .unwrap_or(usize::MAX);
            println!(
                "{}",
                serde_json::json!({"transient": "sheet", "opened": true, "windows": own_windows, "sheets": window.sheets().len()})
            );
            let _ = std::io::Write::flush(&mut std::io::stdout());
            Some(created)
        } else {
            None
        };
        // Owned teardown stimulus: close only this window at the deadline,
        // then keep running windowless until the seconds deadline or the
        // holder stop. Programmatic close of the owned window (no user
        // action, no other window touched). With a stall active the close
        // waits for the stall (documented); the session holder uses no stall
        // here, so the deadline is exact.
        let close_at = if close_window_ms > 0 {
            Some(Instant::now() + Duration::from_millis(close_window_ms.min(20_000)))
        } else {
            None
        };
        let mut closed = false;
        let panel_close_at = if transient_panel_ms > 0 {
            Some(Instant::now() + Duration::from_millis(transient_panel_ms.min(20_000)))
        } else {
            None
        };
        let sheet_close_at = if transient_sheet_ms > 0 {
            Some(Instant::now() + Duration::from_millis(transient_sheet_ms.min(20_000)))
        } else {
            None
        };
        let mut panel_closed = false;
        let mut sheet_closed = false;
        let deadline = Instant::now() + Duration::from_secs(seconds.clamp(1, 600));
        // Downstream delivery snapshots for the `input-session` fixture:
        // cumulative AppKit key arrivals printed on the stdout event pipe
        // every 500 ms (the holder diffs them around each tagged post).
        // Counting rides the counted drain below, so delivery proof shares
        // the exact dispatch path (never a side channel).
        let mut last_snapshot = Instant::now();
        // Public Space-join self-action poll (opt-in `--space-join` only):
        // the helper acts SOLELY on its own window below — never a caller
        // WID, never another process. Trigger is the per-run request file
        // (exact nonce match); the controller authenticates via the spawn
        // record before writing it, so this poll sees only owned triggers.
        // Semantics note (verified against maintained `objc2-app-kit` 0.3 +
        // AppKit `NSWindow.h`: `MoveToActiveSpace = 1<<1` with no
        // follows-every-switch guarantee in the header): treat as
        // move-on-activate (order-front + self-activate), never assume a
        // passive follow; the membership effect is proven harness-side via
        // SLS readback, never here.
        let space_join_nonce: Option<String> = if space_join_poll {
            title
                .strip_prefix(crate::probe_target::PROBE_WINDOW_TITLE_PREFIX)
                .map(|s| s.to_owned())
                .filter(|n| crate::ownership::validate_nonce(n).is_ok())
        } else {
            None
        };
        let mut last_join_poll = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .unwrap_or_else(Instant::now);
        while Instant::now() < deadline {
            if let Some(own_nonce) = space_join_nonce.as_ref()
                && last_join_poll.elapsed() >= Duration::from_millis(100)
            {
                last_join_poll = Instant::now();
                if let Some(marker) = poll_space_join_once(&window, &app, own_nonce) {
                    println!("{marker}");
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                }
            }
            if !closed
                && let Some(close_deadline) = close_at
                && Instant::now() >= close_deadline
            {
                window.close();
                closed = true;
                // First-party close report: execution proof, the window's own
                // visibility readback, plus the helper's own window-server
                // readback (own-PID window count after the close). The holder
                // awaits this marker AND an independent CG absence readback;
                // either alone proves nothing.
                let own_windows = cg_window_list_all()
                    .map(|list| {
                        let me = std::process::id();
                        list.iter().filter(|w| w.pid == me).count()
                    })
                    .unwrap_or(usize::MAX);
                println!(
                    "{}",
                    serde_json::json!({"closed": true, "windows": own_windows, "visible": window.isVisible(), "key_counts": key_counts_snapshot()})
                );
                let _ = std::io::Write::flush(&mut std::io::stdout());
            }
            if !panel_closed
                && let (Some(panel_window), Some(panel_deadline)) = (panel.as_ref(), panel_close_at)
                && Instant::now() >= panel_deadline
            {
                // Timer closes only the owned transient panel; the main
                // owned window stays alive.
                panel_window.close();
                panel_closed = true;
                let own_windows = cg_window_list_all()
                    .map(|list| {
                        let me = std::process::id();
                        list.iter().filter(|w| w.pid == me).count()
                    })
                    .unwrap_or(usize::MAX);
                println!(
                    "{}",
                    serde_json::json!({"transient": "panel", "closed": true, "windows": own_windows, "visible": panel_window.isVisible()})
                );
                let _ = std::io::Write::flush(&mut std::io::stdout());
            }
            if !sheet_closed
                && let (Some(sheet_window), Some(sheet_deadline)) = (sheet.as_ref(), sheet_close_at)
                && Instant::now() >= sheet_deadline
            {
                // Timer dismisses only the owned attached sheet; the parent
                // owned window stays alive.
                window.endSheet(sheet_window);
                sheet_window.orderOut(None);
                sheet_closed = true;
                let own_windows = cg_window_list_all()
                    .map(|list| {
                        let me = std::process::id();
                        list.iter().filter(|w| w.pid == me).count()
                    })
                    .unwrap_or(usize::MAX);
                println!(
                    "{}",
                    serde_json::json!({"transient": "sheet", "closed": true, "windows": own_windows, "sheets": window.sheets().len()})
                );
                let _ = std::io::Write::flush(&mut std::io::stdout());
            }
            if last_snapshot.elapsed() >= Duration::from_millis(500) {
                last_snapshot = Instant::now();
                println!(
                    "{}",
                    serde_json::json!({"key_counts": key_counts_snapshot()})
                );
                let _ = std::io::Write::flush(&mut std::io::stdout());
            }
            // Service application events (activation and friends) as well as
            // the bare runloop: focus machinery arrives as NSEvents, and a
            // pump that never dequeues them leaves activation parked.
            objc2::rc::autoreleasepool(|_| {
                drain_app_events_counted(&app);
                pump_runloop_once(50);
            });
        }
        Ok(())
    }

    // -- Bounded event tap (trusted core-graphics wrapper) --

    struct TapShared {
        tx: SyncSender<EngineMessage>,
        tuning: crate::input_match::TapTuning,
        suppress_armed: bool,
        matched: AtomicU32,
        suppressed: AtomicU32,
        deliveries: AtomicU32,
        /// Observed `TapDisabledByTimeout` (re-enable once, then stop on a
        /// second one). Tracked separately from user disables: a user
        /// disable is never re-enabled.
        timeout_disabled: AtomicBool,
        /// Observed `TapDisabledByUserInput` (never re-enabled; the run
        /// stops truthfully instead).
        user_disabled: AtomicBool,
        down_pending: AtomicBool,
        disable_notified: AtomicBool,
    }

    /// Bounded tap for exactly one chord with an independent RAII lifetime.
    /// Default (observe-only): the callback always returns `Keep`; matches
    /// are counted and forwarded as exact-chord engine events, everything
    /// else only counted as deliveries. Armed (`--suppress`): exact-chord
    /// `KeyDown`/`KeyUp` pairs return `Drop` (see
    /// [`crate::input_match::decide_tap_event`]); unrelated keys, inexact
    /// modifiers, lone ups, and `FlagsChanged` always `Keep`. A
    /// timeout-disable re-enables once and notifies the engine exactly once
    /// (informational: a disable is not recovery evidence); a second
    /// timeout-disable stops the run. A user-disable never re-enables: the
    /// run stops truthfully with `user_disabled` set instead (bounded-probe
    /// semantics only — the continuous tile owner recovers both kinds, see
    /// [`crate::input_policy`]).
    /// Drop of the tap invalidates it (RAII).
    pub fn run_event_tap_bounded(
        want_flags: u64,
        flags_mask: u64,
        want_keycode: i64,
        seconds: u64,
        suppress_armed: bool,
        tx: &SyncSender<EngineMessage>,
    ) -> Result<TapRunReport, TapSetupError> {
        use crate::input_match::{TapKeyKind, TapTuning, decide_tap_event};
        use core_foundation::runloop::{CFRunLoop, kCFRunLoopDefaultMode};
        use core_graphics::event::{
            CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
            CallbackResult, EventField,
        };

        let shared = Arc::new(TapShared {
            tx: tx.clone(),
            tuning: TapTuning {
                want_flags,
                flags_mask,
                want_code: want_keycode,
            },
            suppress_armed,
            matched: AtomicU32::new(0),
            suppressed: AtomicU32::new(0),
            deliveries: AtomicU32::new(0),
            timeout_disabled: AtomicBool::new(false),
            user_disabled: AtomicBool::new(false),
            down_pending: AtomicBool::new(false),
            disable_notified: AtomicBool::new(false),
        });
        let callback_shared = shared.clone();
        let tap = CGEventTap::new(
            CGEventTapLocation::Session,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            vec![
                CGEventType::KeyDown,
                CGEventType::KeyUp,
                CGEventType::FlagsChanged,
            ],
            move |_proxy, etype, event| {
                // `CGEventType` has no `PartialEq`; compare discriminants.
                // The two disable kinds stay distinct: a timeout may
                // re-enable once, a user disable never does.
                let kind = etype as u32;
                if kind == CGEventType::TapDisabledByTimeout as u32 {
                    callback_shared.timeout_disabled.store(true, Ordering::SeqCst);
                    return CallbackResult::Keep;
                }
                if kind == CGEventType::TapDisabledByUserInput as u32 {
                    callback_shared.user_disabled.store(true, Ordering::SeqCst);
                    return CallbackResult::Keep;
                }
                let tap_kind = if kind == CGEventType::KeyDown as u32 {
                    TapKeyKind::KeyDown
                } else if kind == CGEventType::KeyUp as u32 {
                    TapKeyKind::KeyUp
                } else if kind == CGEventType::FlagsChanged as u32 {
                    TapKeyKind::Other
                } else {
                    return CallbackResult::Keep;
                };
                if matches!(tap_kind, TapKeyKind::KeyDown | TapKeyKind::KeyUp) {
                    callback_shared.deliveries.fetch_add(1, Ordering::SeqCst);
                }
                let flags = event.get_flags().bits();
                let code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                let mut pending = callback_shared.down_pending.load(Ordering::SeqCst);
                let decision = decide_tap_event(
                    tap_kind,
                    flags,
                    code,
                    callback_shared.tuning,
                    callback_shared.suppress_armed,
                    &mut pending,
                );
                callback_shared.down_pending.store(pending, Ordering::SeqCst);
                if decision.matched {
                    callback_shared.matched.fetch_add(1, Ordering::SeqCst);
                    let _ = callback_shared.tx.try_send(EngineMessage::InputEvent {
                        key_code: code as u32,
                    });
                }
                if decision.suppress {
                    callback_shared.suppressed.fetch_add(1, Ordering::SeqCst);
                    return CallbackResult::Drop;
                }
                CallbackResult::Keep
            },
        )
        .map_err(|()| {
            TapSetupError::Denied(
                "CGEventTapCreate returned null (needs Accessibility consent for the stable bundle identity; grant in System Settings, then retry)"
                    .to_owned(),
            )
        })?;
        let source = tap
            .mach_port()
            .create_runloop_source(0)
            .map_err(|()| TapSetupError::Runloop("runloop source creation failed".to_owned()))?;
        // SAFETY: extern-static mode read; source added/removed symmetrically
        // below; tap Drop invalidates the port (RAII disable).
        unsafe {
            CFRunLoop::get_current().add_source(&source, kCFRunLoopDefaultMode);
        }
        tap.enable();
        let start = Instant::now();
        let deadline = start + Duration::from_secs(seconds.max(1));
        let mut reenabled = false;
        let mut second_disable = false;
        let mut timeout_disables = 0u32;
        while Instant::now() < deadline {
            unsafe {
                CFRunLoop::run_in_mode(kCFRunLoopDefaultMode, Duration::from_millis(250), false);
            }
            // A user disable ends the run truthfully: never re-enabled,
            // never overridden — even after an earlier timeout recovery.
            if shared.user_disabled.load(Ordering::SeqCst) {
                if !shared.disable_notified.swap(true, Ordering::SeqCst) {
                    let _ = shared.tx.try_send(EngineMessage::TapDisabled);
                }
                eprintln!(
                    "tiler-macos input-tap: disabled by user input; refusing without re-enable"
                );
                break;
            }
            if shared.timeout_disabled.load(Ordering::SeqCst) {
                timeout_disables += 1;
                // Notify the engine at most once per run. A disable is
                // lifecycle information only, never recovery evidence.
                if !shared.disable_notified.swap(true, Ordering::SeqCst) {
                    let _ = shared.tx.try_send(EngineMessage::TapDisabled);
                }
                if !reenabled {
                    tap.enable();
                    shared.timeout_disabled.store(false, Ordering::SeqCst);
                    reenabled = true;
                    eprintln!(
                        "tiler-macos input-tap: disabled by timeout; same-tap re-enabled (timeout_disables={timeout_disables})"
                    );
                } else {
                    second_disable = true;
                    break;
                }
            }
        }
        unsafe {
            CFRunLoop::get_current().remove_source(&source, kCFRunLoopDefaultMode);
        }
        Ok(TapRunReport {
            created: true,
            matched: shared.matched.load(Ordering::SeqCst),
            suppressed: shared.suppressed.load(Ordering::SeqCst),
            deliveries: shared.deliveries.load(Ordering::SeqCst),
            reenabled_after_timeout_disable: reenabled,
            stopped_on_second_disable: second_disable,
            timeout_disables,
            user_disabled: shared.user_disabled.load(Ordering::SeqCst),
            suppress_armed: shared.suppress_armed,
            requested_secs: seconds,
            elapsed_secs: start.elapsed().as_secs(),
        })
    }

    // -- Multi-chord continuous tap source (scoped-tile entry point) --
    //
    // The bounded probe tap above (`run_event_tap_bounded`) runs one chord
    // for a fixed duration on the calling thread. The tile input owner needs
    // a continuous owned source instead: one tap over the whole catalogue
    // with a bounded pump per slice, created/driven/destroyed on the owner
    // thread. Matching reuses the exact [`decide_tap_event`] contract
    // (exact chord only, extras fail; repeats forward as repeated KeyDown
    // matches; modifier release clears pending; Caps/Fn bits sit outside
    // the mask so they neither match nor break). Timeout disables re-enable
    // on the same tap inside `pump` and verify before counting (an
    // unverified re-enable surfaces as [`TapPump::NeedsRecreate`] so the
    // caller installs a fresh tap); a user disable surfaces as
    // [`TapPump::UserDisabled`] so the caller releases interception while
    // gated and resumes or recreates when eligible (a logged recoverable
    // state; only a hard creation denial refuses). The callback stays
    // minimal (exact compare,
    // non-blocking forward of exact matches only, atomics) and never
    // touches the core, AX, logging, or blocking calls.

    /// One exact chord in a continuous tap source: tap tuning plus the key
    /// code forwarded on exact match. Plain data; the callback owns matching.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TapChordSpec {
        pub want_flags: u64,
        pub flags_mask: u64,
        pub want_code: i64,
        pub key_code: u32,
    }

    /// One exact-chord tap hit: the matched key code plus the edge. Only
    /// exact matches cross the boundary — never raw deliveries, never titles
    /// or content.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TapHit {
        pub key_code: u32,
        pub down: bool,
        /// Raw `CGEventFlags` bits at the event, so the input owner can
        /// re-apply the exact-chord match before forwarding.
        pub flags: u64,
    }

    /// Health of one bounded tap pump slice.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TapPump {
        /// Slice pumped; interception still held (no disable observed, or a
        /// timeout disable re-enabled and verified inside the pump, counted
        /// in the report).
        Held,
        /// Slice pumped; a timeout disable was re-enabled on the same tap
        /// and verified (`CGEventTapIsEnabled` true), counted in the report.
        /// The owner logs the recovery.
        TimeoutRecovered,
        /// Slice pumped; a timeout disable was re-enabled on the same tap
        /// but did NOT verify (`CGEventTapIsEnabled` still false). The
        /// owner must install a fresh tap (never a second same-tap
        /// enable); a fresh creation refusal fails closed.
        NeedsRecreate,
        /// The tap was disabled by user input / Secure Input: the owner
        /// releases interception while gated and resumes or recreates when
        /// eligible (a logged recoverable state — the notification name
        /// alone never proves consent revocation, and the surveyed skhd
        /// tap-disabled restart re-enables both kinds). Only a hard
        /// creation denial refuses.
        UserDisabled,
    }

    /// Counters for one continuous tap source (terminal-JSON evidence).
    /// Counters are per-guard: the owner carries them across fresh-guard
    /// recreations so terminal evidence is cumulative, never reset by a
    /// recovery. Kinds stay distinct: timeout recoveries count only in
    /// `reenables_after_timeout`, user-input notifications only in
    /// `user_disables`, and enabled-readback-only recoveries (directly
    /// disabled with no OS notification) only in `unnotified_disables`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct TapSourceReport {
        pub matched: u32,
        pub suppressed: u32,
        pub deliveries: u32,
        pub reenables_after_timeout: u32,
        /// Observed `TapDisabledByUserInput` notifications (each one is a
        /// logged recoverable state, released while gated and resumed or
        /// recreated when eligible; never terminal by name alone).
        pub user_disables: u32,
        /// Owner-side enabled-readback-only recoveries: bounded pump turns
        /// that reported no OS notification yet read directly disabled
        /// (`CGEventTapIsEnabled` false). Never counted as a timeout.
        pub unnotified_disables: u32,
    }

    struct TapMultiShared {
        tx: SyncSender<TapHit>,
        chords: Vec<(crate::input_match::TapTuning, u32)>,
        suppress_armed: bool,
        matched: AtomicU32,
        suppressed: AtomicU32,
        deliveries: AtomicU32,
        /// Disable cause observed by the callback: 0 none, 1 timeout
        /// (recovered inside `pump`), 2 user (refused by the caller).
        disable_kind: AtomicU32,
        /// Per-chord KeyDown pending bit (chord count capped at 64).
        pending: AtomicU64,
        reenables: AtomicU32,
        /// Observed user-disable notifications (terminal evidence; each one
        /// is a logged recoverable state for the owner, never terminal by
        /// name alone).
        user_disables: AtomicU32,
        /// Owner-side enabled-readback-only observations, recorded via
        /// [`TapSource::note_unnotified_disabled`]: bounded pump turns with
        /// no OS notification that still read directly disabled. Counted
        /// here, never as a timeout.
        unnotified: AtomicU32,
        /// Gate released (lock screen, Secure Input, unknown session): the
        /// callback passes everything untouched while set, so interception
        /// is released without destroying the tap; clearing resumes
        /// narrowly. The tap `enable()` wrapper has no disable leg, so the
        /// gate lives here instead of in the port state.
        gated: AtomicBool,
        /// Scoped input eligibility (lock-free cached, owner-published from
        /// fresh scope/foreground/Space/showstate facts outside the
        /// callback): while ineligible the callback Keeps/noForwards every
        /// chord (no suppress, no forward, no count). `None` means the
        /// legacy unrestricted path (fixture/tests); `Some` is the normal
        /// scoped tile path (starts ineligible until attested).
        scoped_eligibility: Option<std::sync::Arc<crate::input_eligibility::EligibilityCache>>,
        /// Fixture-only one-shot callback stall (milliseconds, 0 = off):
        /// the first exact-match KeyDown after creation sleeps this long
        /// INSIDE the tap callback to provoke a REAL OS
        /// `TapDisabledByTimeout` (never a manual callback). Set only by
        /// [`TapSource::create_stall_once`], which only the `input-session`
        /// fault-injection leg uses; production tile sources always run 0.
        stall_once_ms: AtomicU64,
    }

    // Direct tap-state via the maintained `objc2-core-graphics` binding
    // (`CGEvent::tap_enable` / `CGEvent::tap_is_enabled` under the existing
    // `CGEvent` feature): the `core-graphics` tap wrapper exposes enable-only,
    // so the DIRECT disable/recovery fixture reads and writes state through
    // the maintained binding below instead of a hand-written `extern "C"`.
    // The live `core-graphics` tap Mach port and the `objc2` `CFMachPort`
    // name the same window-server port object, so the call borrows the live
    // port for the call only (never a foreign pointer across the boundary).
    fn tap_port<'a>(
        tap: &'a core_graphics::event::CGEventTap<'static>,
    ) -> &'a objc2_core_foundation::CFMachPort {
        use core_foundation::base::TCFType as _;
        // SAFETY: the pointer is the live tap Mach port owned by `tap`
        // (borrowed for the call); both wrappers name the same CF object.
        unsafe {
            &*(tap.mach_port().as_concrete_TypeRef() as *const objc2_core_foundation::CFMachPort)
        }
    }

    /// Owned continuous tap over a chord table. Thread-bound by
    /// construction (tap plus runloop source; intentionally `!Send`). `Drop`
    /// removes the runloop source; the tap port invalidates with it.
    pub struct TapSource {
        tap: core_graphics::event::CGEventTap<'static>,
        runloop_source: core_foundation::runloop::CFRunLoopSource,
        shared: Arc<TapMultiShared>,
        _not_send: std::marker::PhantomData<std::rc::Rc<()>>,
    }

    impl TapSource {
        /// Create the tap over `chords` (1..=64 exact chords). Fails closed
        /// when creation is denied (no consent) — a denial is `Err`, never a
        /// silent success. `suppress_armed` arms exact-chord KeyDown/KeyUp
        /// pair suppression for the tile catalogue; unmatched events always
        /// pass (`Keep`).
        pub fn create(
            chords: &[TapChordSpec],
            suppress_armed: bool,
            tx: &SyncSender<TapHit>,
        ) -> Result<Self, TapSetupError> {
            Self::create_inner(chords, suppress_armed, tx, 0, None)
        }

        /// Scoped tile tap over `chords` with lock-free cached eligibility:
        /// while the cache reads ineligible the callback Keeps/noForwards
        /// every chord BEFORE suppression (no suppress, no forward, no
        /// count). The owner publishes fresh scope/foreground/Space/showstate
        /// facts outside the callback; unknown/stale denies. Normal tile
        /// path (starts ineligible until attested); the fixture uses an
        /// explicit unrestricted test gate instead.
        pub fn create_scoped(
            chords: &[TapChordSpec],
            suppress_armed: bool,
            tx: &SyncSender<TapHit>,
            eligibility: std::sync::Arc<crate::input_eligibility::EligibilityCache>,
        ) -> Result<Self, TapSetupError> {
            Self::create_inner(chords, suppress_armed, tx, 0, Some(eligibility))
        }

        /// Fixture-only tap with a one-shot callback stall: identical to
        /// [`TapSource::create`] except the first exact-match KeyDown sleeps
        /// `stall_ms` (1..=2000) inside the callback to provoke a REAL OS
        /// `TapDisabledByTimeout`. Only the `input-session`
        /// `--stall-once-ms` leg uses this; production tile code must never
        /// call it (the tile factory defaults the stall to 0 and exposes no
        /// production path to set it).
        pub fn create_stall_once(
            chords: &[TapChordSpec],
            suppress_armed: bool,
            tx: &SyncSender<TapHit>,
            stall_ms: u64,
        ) -> Result<Self, TapSetupError> {
            if stall_ms == 0 || stall_ms > 2000 {
                return Err(TapSetupError::Runloop(format!(
                    "stall_ms must be 1..=2000, got {stall_ms}"
                )));
            }
            Self::create_inner(chords, suppress_armed, tx, stall_ms, None)
        }

        fn create_inner(
            chords: &[TapChordSpec],
            suppress_armed: bool,
            tx: &SyncSender<TapHit>,
            stall_once_ms: u64,
            scoped_eligibility: Option<std::sync::Arc<crate::input_eligibility::EligibilityCache>>,
        ) -> Result<Self, TapSetupError> {
            use crate::input_match::{TapKeyKind, TapTuning, decide_tap_event};
            use core_foundation::runloop::{CFRunLoop, kCFRunLoopDefaultMode};
            use core_graphics::event::{
                CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
                CGEventType, CallbackResult, EventField,
            };

            if chords.is_empty() {
                return Err(TapSetupError::Runloop("empty chord table".to_owned()));
            }
            if chords.len() > 64 {
                return Err(TapSetupError::Runloop(format!(
                    "chord table too large ({})",
                    chords.len()
                )));
            }
            let table: Vec<(TapTuning, u32)> = chords
                .iter()
                .map(|spec| {
                    (
                        TapTuning {
                            want_flags: spec.want_flags,
                            flags_mask: spec.flags_mask,
                            want_code: spec.want_code,
                        },
                        spec.key_code,
                    )
                })
                .collect();
            let shared = Arc::new(TapMultiShared {
                tx: tx.clone(),
                chords: table,
                suppress_armed,
                matched: AtomicU32::new(0),
                suppressed: AtomicU32::new(0),
                deliveries: AtomicU32::new(0),
                disable_kind: AtomicU32::new(0),
                pending: AtomicU64::new(0),
                reenables: AtomicU32::new(0),
                user_disables: AtomicU32::new(0),
                unnotified: AtomicU32::new(0),
                gated: AtomicBool::new(false),
                scoped_eligibility,
                stall_once_ms: AtomicU64::new(stall_once_ms),
            });
            let callback_shared = shared.clone();
            let tap = CGEventTap::new(
                CGEventTapLocation::Session,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::Default,
                vec![
                    CGEventType::KeyDown,
                    CGEventType::KeyUp,
                    CGEventType::FlagsChanged,
                ],
                move |_proxy, etype, event| {
                    // Gated (lock screen, Secure Input, unknown session):
                    // pass everything untouched — interception released,
                    // nothing matched, nothing forwarded, nothing counted.
                    if callback_shared.gated.load(Ordering::SeqCst) {
                        return CallbackResult::Keep;
                    }
                    // Scoped eligibility BEFORE suppression (lock-free cached
                    // read only, no blocking work): out-of-scope/fullscreen/
                    // wrong-Space/unknown/stale Keeps/noForwards every chord
                    // (no suppress, no forward, no count). Dropping already-
                    // consumed hits would NOT restore passthrough; this gate
                    // prevents the consume itself.
                    if let Some(cache) = callback_shared.scoped_eligibility.as_ref()
                        && !cache.is_eligible()
                    {
                        return CallbackResult::Keep;
                    }
                    // `CGEventType` has no `PartialEq`; compare discriminants.
                    let kind = etype as u32;
                    if kind == CGEventType::TapDisabledByTimeout as u32 {
                        callback_shared.disable_kind.store(1, Ordering::SeqCst);
                        return CallbackResult::Keep;
                    }
                    if kind == CGEventType::TapDisabledByUserInput as u32 {
                        callback_shared.disable_kind.store(2, Ordering::SeqCst);
                        return CallbackResult::Keep;
                    }
                    let tap_kind = if kind == CGEventType::KeyDown as u32 {
                        TapKeyKind::KeyDown
                    } else if kind == CGEventType::KeyUp as u32 {
                        TapKeyKind::KeyUp
                    } else if kind == CGEventType::FlagsChanged as u32 {
                        TapKeyKind::Other
                    } else {
                        return CallbackResult::Keep;
                    };
                    if matches!(tap_kind, TapKeyKind::KeyDown | TapKeyKind::KeyUp) {
                        callback_shared.deliveries.fetch_add(1, Ordering::SeqCst);
                    }
                    let flags = event.get_flags().bits();
                    let code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                    // First exact match wins (tables are duplicate-free, so
                    // at most one arm matches). Minimal per-chord work:
                    // exact compare plus a non-blocking forward.
                    let mut hit: Option<(u32, bool)> = None;
                    for (index, (tuning, key_code)) in
                        callback_shared.chords.iter().enumerate()
                    {
                        let mut pending =
                            (callback_shared.pending.load(Ordering::SeqCst) >> index) & 1 == 1;
                        let decision = decide_tap_event(
                            tap_kind,
                            flags,
                            code,
                            *tuning,
                            callback_shared.suppress_armed,
                            &mut pending,
                        );
                        if pending {
                            callback_shared.pending.fetch_or(1u64 << index, Ordering::SeqCst);
                        } else {
                            callback_shared
                                .pending
                                .fetch_and(!(1u64 << index), Ordering::SeqCst);
                        }
                        if decision.matched {
                            hit = Some((*key_code, decision.suppress));
                            break;
                        }
                    }
                    match hit {
                        Some((key_code, suppress)) => {
                            callback_shared.matched.fetch_add(1, Ordering::SeqCst);
                            let _ = callback_shared.tx.try_send(TapHit {
                                key_code,
                                down: matches!(tap_kind, TapKeyKind::KeyDown),
                                flags,
                            });
                            // Fixture-only one-shot stall: the first
                            // exact-match KeyDown sleeps inside the callback
                            // (consumed atomically, so exactly once) to
                            // provoke a REAL OS `TapDisabledByTimeout`.
                            // Zero in every production run: no sleep.
                            if matches!(tap_kind, TapKeyKind::KeyDown) {
                                let stall =
                                    callback_shared.stall_once_ms.swap(0, Ordering::SeqCst);
                                if stall > 0 {
                                    std::thread::sleep(std::time::Duration::from_millis(
                                        stall.min(2000),
                                    ));
                                }
                            }
                            if suppress {
                                callback_shared.suppressed.fetch_add(1, Ordering::SeqCst);
                                return CallbackResult::Drop;
                            }
                            CallbackResult::Keep
                        }
                        None => CallbackResult::Keep,
                    }
                },
            )
            .map_err(|()| {
                TapSetupError::Denied(
                    "CGEventTapCreate returned null (needs Accessibility consent for the stable bundle identity; grant in System Settings, then retry)"
                        .to_owned(),
                )
            })?;
            let runloop_source = tap.mach_port().create_runloop_source(0).map_err(|()| {
                TapSetupError::Runloop("runloop source creation failed".to_owned())
            })?;
            // SAFETY: extern-static mode read; the source is added here and
            // removed symmetrically in `Drop`; the tap Drop invalidates the
            // port (RAII disable).
            unsafe {
                CFRunLoop::get_current().add_source(&runloop_source, kCFRunLoopDefaultMode);
            }
            tap.enable();
            Ok(Self {
                tap,
                runloop_source,
                shared,
                _not_send: std::marker::PhantomData,
            })
        }

        /// Pump one bounded runloop slice (at most ~250 ms). A timeout
        /// disable re-enables on the same tap and verifies
        /// (`CGEventTapIsEnabled`): verified recoveries return
        /// [`TapPump::TimeoutRecovered`] (counted); an unverified re-enable
        /// returns [`TapPump::NeedsRecreate`] so the owner installs a fresh
        /// tap instead of enabling the same tap twice. A user disable
        /// returns [`TapPump::UserDisabled`] (counted); the owner releases
        /// interception while gated and resumes or recreates when eligible.
        pub fn pump(&mut self, millis: u64) -> TapPump {
            use core_foundation::runloop::{CFRunLoop, kCFRunLoopDefaultMode};
            use std::time::Duration;
            // SAFETY: extern-static mode read (same pattern as the bounded
            // probe tap); the slice bound caps stop latency.
            unsafe {
                CFRunLoop::run_in_mode(
                    kCFRunLoopDefaultMode,
                    Duration::from_millis(millis.min(250)),
                    false,
                );
            }
            match self.shared.disable_kind.swap(0, Ordering::SeqCst) {
                0 => TapPump::Held,
                1 => {
                    self.tap.enable();
                    if self.is_tap_direct_enabled() {
                        self.shared.reenables.fetch_add(1, Ordering::SeqCst);
                        TapPump::TimeoutRecovered
                    } else {
                        TapPump::NeedsRecreate
                    }
                }
                _ => {
                    self.shared.user_disables.fetch_add(1, Ordering::SeqCst);
                    TapPump::UserDisabled
                }
            }
        }

        /// Hold or release interception without destroying the tap: the
        /// Secure Input / lock gate releases while gated and resumes
        /// narrowly when the ordinary unlocked state returns. Implemented
        /// as a callback gate (pass everything untouched) because the
        /// maintained tap wrapper exposes no disable leg.
        pub fn set_enabled(&self, enabled: bool) {
            self.shared.gated.store(!enabled, Ordering::SeqCst);
        }

        /// Fixture-only DIRECT tap disable/enable at the OS level
        /// (maintained `CGEvent::tap_enable`): the tap stops
        /// receiving events while directly disabled, so exact chords pass
        /// through to the frontmost app. Only the `input-session`
        /// DIRECT legs use this (driven by the input
        /// owner thread from the factory's hold flag); production tile code
        /// must never call it.
        pub fn set_tap_direct_enabled(&self, enabled: bool) {
            // SAFETY: live-port borrow inside `tap_port` (see helper above);
            // the maintained binding performs the OS call.
            objc2_core_graphics::CGEvent::tap_enable(tap_port(&self.tap), enabled);
        }

        /// Actual OS tap-state readback (maintained
        /// `CGEvent::tap_is_enabled`): `false`
        /// after a DIRECT disable, `true` after recovery. The DIRECT legs
        /// gate on this reading (never on a callback atomic alone), and the
        /// production owner health-checks it on every bounded pump turn that
        /// reports no OS notification.
        #[must_use]
        pub fn is_tap_direct_enabled(&self) -> bool {
            // SAFETY: same live-port borrow as above; read-only, no writes.
            objc2_core_graphics::CGEvent::tap_is_enabled(tap_port(&self.tap))
        }

        /// Re-enable the SAME tap and verify with an enabled readback
        /// (maintained bindings only, no hand-written FFI): true only when
        /// the tap reads enabled afterwards. The owner calls this for
        /// notification and readback-only recoveries; a false return means
        /// the owner must install a fresh tap instead (never a second
        /// same-tap enable).
        pub fn reenable_same_tap_verified(&self) -> bool {
            self.tap.enable();
            self.is_tap_direct_enabled()
        }

        /// Record one owner-side enabled-readback-only observation (bounded
        /// pump turn with no OS notification that still read directly
        /// disabled). Counted separately from timeouts; the owner carries
        /// the count across fresh-guard recreations.
        pub fn note_unnotified_disabled(&self) {
            self.shared.unnotified.fetch_add(1, Ordering::SeqCst);
        }

        #[must_use]
        pub fn report(&self) -> TapSourceReport {
            TapSourceReport {
                matched: self.shared.matched.load(Ordering::SeqCst),
                suppressed: self.shared.suppressed.load(Ordering::SeqCst),
                deliveries: self.shared.deliveries.load(Ordering::SeqCst),
                reenables_after_timeout: self.shared.reenables.load(Ordering::SeqCst),
                user_disables: self.shared.user_disables.load(Ordering::SeqCst),
                unnotified_disables: self.shared.unnotified.load(Ordering::SeqCst),
            }
        }
    }

    impl Drop for TapSource {
        fn drop(&mut self) {
            use core_foundation::runloop::{CFRunLoop, kCFRunLoopDefaultMode};
            // SAFETY: the source was added in `create` on this thread;
            // removal is symmetric and idempotent; the tap port invalidates
            // on drop (RAII disable).
            unsafe {
                CFRunLoop::get_current().remove_source(&self.runloop_source, kCFRunLoopDefaultMode);
            }
        }
    }

    // -- Scoped-tile read-only substrate (no titles, no writes) --
    //
    // Every reader below is a bounded AX/AppKit/CG read with no mutation and
    // no title acquisition. AX readers assume the caller set a finite
    // messaging timeout on the element first (a refused timeout aborts the
    // narrow operation before any join/read). Attribute-name prior art:
    // - role/subrole/minimized/fullscreen/size-settability: yabai
    //   `src/window.c` (`window_is_minimized`, `window_is_fullscreen` over
    //   `kAXFullscreenAttribute = "AXFullScreen"`, `window_ax_can_resize`
    //   over `AXUIElementIsAttributeSettable(kAXSizeAttribute)`) at
    //   https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window.c
    // - role/subrole/minimized/fullscreen(`AXFullscreen` spelling)/resizable:
    //   rift `src/sys/axuielement.rs` + `src/sys/app.rs` at
    //   https://github.com/acsandmann/rift/blob/8114ccc534b5c9a7e6a77ce27568d8739be9216f/src/sys/axuielement.rs
    // - size settability: glide `src/sys/app.rs` (`is_settable(size)`) at
    //   https://github.com/tmandry/glide/blob/ef3b30f88445fe4b517eb4c60023011f6d3725dd/src/sys/app.rs
    //   and Silica `SIAccessibilityElement.m` (`isSizeWriteable`) at
    //   https://github.com/ianyh/Silica/blob/ebead82a28486e7719b7ed69633ecfed8a4fe2d2/Silica/Sources/SIAccessibilityElement.m
    // - min size (`AXMinSize`, then `AXMinimumSize`): Rectangle
    //   `AXExtension.swift` at
    //   https://github.com/rxhanson/Rectangle/blob/1d46069c27b3b0564bd7cc1e6d97bf6d53093079/Rectangle/Rectangle/Utilities/AXExtension.swift
    //   (no surveyed source reads an AX max size: max stays `None`, TBD).
    // - activation policy gate (`Regular` only): rift `src/sys/app.rs`
    //   (`activationPolicy() != Regular` skips) and glide (same shape).
    // - display/Space-change notifications (`NSApplicationDidChangeScreenParametersNotification`,
    //   `NSWorkspaceActiveSpaceDidChangeNotification`): glide
    //   `src/actor/notification_center.rs` and rift
    //   `src/actor/notification_center.rs` (observer wiring itself is later
    //   work; this unit exposes poll-changed over revisions instead).
    // `AXModal` has NO surveyed prior art: it is probed read-only and
    // `None` on any unsupported read, never defaulted here.

    /// String AX read (`AXRole`/`AXSubrole`). `Ok(None)` is honest unknown
    /// (missing value or non-string); `Err` is a real read failure.
    pub fn ax_string_attribute(element: &AxElement, name: &str) -> Result<Option<String>, i32> {
        match element.copy_attribute(name) {
            // `kAXErrorNoValue`: attribute present but valueless.
            Err(AX_NO_VALUE) => Ok(None),
            Err(code) => Err(code),
            Ok(value) => match value.downcast::<CFString>() {
                Ok(s) => Ok(Some(s.to_string())),
                Err(_) => Ok(None),
            },
        }
    }

    /// Boolean AX read. No defaults: an unreadable attribute is `Err`, so
    /// callers carry `None` (unknown) instead of inventing a state.
    pub fn ax_bool_attribute(element: &AxElement, name: &str) -> Result<bool, i32> {
        use objc2_core_foundation::CFBoolean;
        let value = element.copy_attribute(name)?;
        value
            .downcast::<CFBoolean>()
            .map(|b| b.as_bool())
            .map_err(|_| AX_NOT_IMPLEMENTED)
    }

    /// Boolean AX write (owned minimize/restore leg: `AXMinimized`).
    /// Safety audit: same shape as the frame setters above — stack-lived
    /// inputs only (`CFString` name plus a retained `CFBoolean` built by
    /// value, never a raw pointer the caller owns); the maintained
    /// `AXValue`-free path calls `set_attribute_value` on the live element
    /// with both arguments in maintained wrappers, and the returned status
    /// is the only effect read (callers must read back separately: an
    /// accepted status is never state proof). Scoped to the
    /// owned-test-control path ([`crate::ax::set_minimized_owned`]), which
    /// owns entry/exe/nonce/CG revalidation around every call; no other
    /// caller may adopt this for foreign windows.
    pub fn ax_set_bool_attribute(element: &AxElement, name: &str, value: bool) -> Result<(), i32> {
        use objc2_core_foundation::{CFBoolean, CFString};
        let attr = CFString::from_str(name);
        let flag = CFBoolean::new(value);
        // SAFETY: live element/name/value in maintained wrappers; status is
        // the only effect read.
        let status = unsafe { element.inner.set_attribute_value(&attr, flag) }.0;
        if status == 0 { Ok(()) } else { Err(status) }
    }

    /// `AXSize` settability (`AXUIElementIsAttributeSettable`). Carried fact
    /// only: no admission inference is drawn from it (TBD lane, see
    /// [`crate::classifier`]).
    pub fn ax_is_size_settable(element: &AxElement) -> Result<bool, i32> {
        let attr = CFString::from_str("AXSize");
        let mut raw: u8 = 0;
        // SAFETY: out-pointer is a live local; status gates the read.
        // `Boolean` is an 8-bit Darwin type; nonzero means settable.
        let status = unsafe {
            element.inner.is_attribute_settable(
                &attr,
                NonNull::new(std::ptr::addr_of_mut!(raw)).expect("non-null"),
            )
        };
        if status.0 == 0 {
            Ok(raw != 0)
        } else {
            Err(status.0)
        }
    }

    /// One CGSize AX read. Unsupported/missing shapes are `Ok(None)`, never
    /// guessed; real failures are `Err`.
    fn ax_cgsize_attribute(element: &AxElement, name: &str) -> Result<Option<(f64, f64)>, i32> {
        let value = match element.copy_attribute(name) {
            // NoValue / AttributeUnsupported / NotImplemented: no usable
            // bound here (the caller tries the next name or reports none).
            Err(AX_NO_VALUE) | Err(AX_ATTRIBUTE_UNSUPPORTED) | Err(AX_NOT_IMPLEMENTED) => {
                return Ok(None);
            }
            Err(code) => return Err(code),
            Ok(value) => value,
        };
        let size: CFRetained<AXValue> = value.downcast().map_err(|_| AX_NOT_IMPLEMENTED)?;
        let mut raw = CGSize {
            width: 0.0,
            height: 0.0,
        };
        // SAFETY: value lives in a maintained wrapper; the out-pointer is a
        // live local; the boolean result gates use.
        let ok = unsafe {
            size.value(
                AXValueType::CGSize,
                NonNull::new(std::ptr::addr_of_mut!(raw) as *mut std::ffi::c_void)
                    .expect("non-null"),
            )
        };
        if !ok {
            return Ok(None);
        }
        if raw.width.is_finite() && raw.height.is_finite() && raw.width >= 0.0 && raw.height >= 0.0
        {
            Ok(Some((raw.width, raw.height)))
        } else {
            Ok(None)
        }
    }

    /// Actual minimum size probe: `AXMinSize`, then `AXMinimumSize`
    /// (Rectangle prior art above). Unsupported everywhere is `Ok(None)`.
    /// There is deliberately no max-size probe: no surveyed source reads an
    /// AX max size, so max stays `None` (TBD), never inferred from
    /// settability.
    pub fn ax_min_size(element: &AxElement) -> Result<Option<(f64, f64)>, i32> {
        for name in ["AXMinSize", "AXMinimumSize"] {
            if let Some(min) = ax_cgsize_attribute(element, name)? {
                return Ok(Some(min));
            }
        }
        Ok(None)
    }

    /// Fullscreen state with both surveyed spellings: yabai's `AXFullScreen`
    /// first, rift's `AXFullscreen` as fallback. Still unreadable after both
    /// is `Err` (unknown), never a defaulted state.
    pub fn ax_fullscreen_state(element: &AxElement) -> Result<bool, i32> {
        match ax_bool_attribute(element, "AXFullScreen") {
            Err(AX_ATTRIBUTE_UNSUPPORTED) | Err(AX_NO_VALUE) | Err(AX_NOT_IMPLEMENTED) => {
                ax_bool_attribute(element, "AXFullscreen")
            }
            other => other,
        }
    }

    /// Whether the app owning `pid` is hidden (`NSRunningApplication.isHidden`,
    /// public AppKit, no AX consent involved). Caller-thread read (main);
    /// unknown pids are `Err`, never assumed visible.
    pub fn app_hidden_for_pid(pid: i32) -> Result<bool, String> {
        if pid <= 1 {
            return Err(format!("refusing reserved pid {pid}"));
        }
        use objc2_app_kit::NSRunningApplication;
        match NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
            Some(app) => Ok(app.isHidden()),
            None => Err(format!("no running application for pid {pid}")),
        }
    }

    /// Raw activation policy for `pid` (`NSApplicationActivationPolicy`
    /// discriminant: 0=Regular, 1=Accessory, 2=Prohibited). Mapped to
    /// [`crate::classifier::ActivationPolicy`] by the pure layer; unknown
    /// pids or values are errors there, never guessed policy.
    pub fn activation_policy_raw_for_pid(pid: i32) -> Result<i64, String> {
        if pid <= 1 {
            return Err(format!("refusing reserved pid {pid}"));
        }
        use objc2_app_kit::NSRunningApplication;
        match NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
            Some(app) => Ok(app.activationPolicy().0 as i64),
            None => Err(format!("no running application for pid {pid}")),
        }
    }

    /// One CGWindowList entry without the title leg: identity plus geometry
    /// plus layer/on-screen only. No `kCGWindowName` read, no owner names, no
    /// content — and therefore nothing to log.
    #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    pub struct CgWindowBasic {
        pub wid: u32,
        pub pid: u32,
        pub layer: i32,
        pub onscreen: bool,
        pub bounds: CgBounds,
    }

    /// Title-free window list: the same maintained binding as
    /// [`cg_window_list_all`], parsed with the title leg skipped (no title
    /// acquisition, not even transiently).
    pub fn cg_window_list_basic() -> Result<Vec<CgWindowBasic>, String> {
        cg_window_list_inner(false).map(|list| {
            list.into_iter()
                .map(|w| CgWindowBasic {
                    wid: w.wid,
                    pid: w.pid,
                    layer: w.layer,
                    onscreen: w.onscreen,
                    bounds: w.bounds,
                })
                .collect()
        })
    }

    /// AppKit display in global bottom-left points: full frame plus the
    /// menu/Dock-excluded visible frame plus backing scale. Plain data only.
    #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    pub struct DisplayRect {
        pub x: f64,
        pub y: f64,
        pub w: f64,
        pub h: f64,
    }

    /// One display snapshot row: CG display ID plus the exact managed
    /// display UUID (public `CGDisplayCreateUUIDFromDisplayID`, never a
    /// private symbol) plus AppKit frames plus scale. `uuid` is `None` only
    /// when the public lookup fails (unknown, never guessed). No names, no
    /// content.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct NsDisplay {
        pub cg_id: u32,
        pub uuid: Option<String>,
        pub frame: DisplayRect,
        pub visible: DisplayRect,
        pub scale: f64,
    }

    /// Read-only display snapshot. MUST run on the main thread (AppKit
    /// `NSScreen`); any other thread fails closed before touching UI state.
    /// Points throughout (never physical pixels); the AppKit bottom-left to
    /// CG/AX top-left conversion is pure (see [`crate::topology`]) so mixed
    /// scales need no pixel math.
    pub fn ns_display_snapshot() -> Result<Vec<NsDisplay>, String> {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSScreen;
        let Some(mtm) = MainThreadMarker::new() else {
            return Err("display snapshot requires the main thread".to_owned());
        };
        let screens = NSScreen::screens(mtm);
        let count = screens.len().min(MAX_DISPLAYS);
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let screen = screens.objectAtIndex(i);
            let frame = screen.frame();
            let visible = screen.visibleFrame();
            let cg_id = screen.CGDirectDisplayID();
            out.push(NsDisplay {
                cg_id,
                uuid: cg_display_uuid(cg_id),
                frame: DisplayRect {
                    x: frame.origin.x,
                    y: frame.origin.y,
                    w: frame.size.width,
                    h: frame.size.height,
                },
                visible: DisplayRect {
                    x: visible.origin.x,
                    y: visible.origin.y,
                    w: visible.size.width,
                    h: visible.size.height,
                },
                scale: screen.backingScaleFactor(),
            });
        }
        Ok(out)
    }

    /// Whether the session uses separate Spaces per display (public
    /// `NSScreen.screensHaveSeparateSpaces`, the AppKit reading of the
    /// Mission Control "Displays have separate Spaces" setting that rift
    /// probes as `SLSGetSpaceManagementMode(cid) == 1`). `None` off the main
    /// thread (unknown, never guessed). The pure layer homes unified
    /// (non-separate) sessions conservatively: a single SLS row shares its
    /// current Space across displays; anything else refuses.
    pub fn ns_separate_spaces() -> Option<bool> {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSScreen;
        let mtm = MainThreadMarker::new()?;
        Some(NSScreen::screensHaveSeparateSpaces(mtm))
    }

    /// Managed display UUID for a CG display ID via the PUBLIC
    /// `CGDisplayCreateUUIDFromDisplayID` (CoreGraphics, create-rule +1).
    /// No maintained `objc2` binding covers it, so the public symbol resolves
    /// dynamically exactly like the other uncovered tables here; a missing
    /// symbol or null result is `None`, never a guessed UUID. Stringified
    /// with the maintained `CFUUID::new_string` (`CFUUIDCreateString`).
    pub fn cg_display_uuid(display_id: u32) -> Option<String> {
        use objc2_core_foundation::{CFRetained, CFUUID};
        if display_id == 0 {
            return None;
        }
        let table = cg_uuid_table()?;
        // SAFETY: resolved non-null public symbol; plain integer in;
        // create-rule +1 out (null-checked); owned exactly once below.
        let uuid: CFRetained<CFUUID> = unsafe {
            let raw = (table.create)(display_id);
            CFRetained::from_raw(std::ptr::NonNull::new(raw as *mut CFUUID)?)
        };
        let text = CFUUID::new_string(None, Some(&uuid))?;
        let uuid = text.to_string();
        if uuid.is_empty() { None } else { Some(uuid) }
    }

    struct CgUuidTable {
        create: unsafe extern "C" fn(u32) -> *mut std::ffi::c_void,
    }

    static CG_UUID_TABLE: std::sync::OnceLock<Option<CgUuidTable>> = std::sync::OnceLock::new();

    /// Public CoreGraphics framework only (plus the already-linked image as
    /// fallback). A missing symbol fails closed, never guessed.
    fn cg_uuid_table() -> Option<&'static CgUuidTable> {
        use std::ffi::CString;
        CG_UUID_TABLE
            .get_or_init(|| {
                // SAFETY: `dlsym` lookup null-checked before transmuting to
                // the documented public signature
                // (`CFUUIDRef CGDisplayCreateUUIDFromDisplayID(CGDirectDisplayID)`);
                // no call is made here.
                unsafe {
                    const CANDIDATE: &[u8] =
                        b"/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics\0";
                    let cname = CString::new("CGDisplayCreateUUIDFromDisplayID").expect("static");
                    let probe = |handle: *mut std::ffi::c_void| {
                        if handle.is_null() {
                            return None;
                        }
                        let sym = libc::dlsym(handle, cname.as_ptr());
                        if sym.is_null() {
                            return None;
                        }
                        Some(CgUuidTable {
                            create: std::mem::transmute::<
                                *mut std::ffi::c_void,
                                unsafe extern "C" fn(u32) -> *mut std::ffi::c_void,
                            >(sym),
                        })
                    };
                    let handle = libc::dlopen(CANDIDATE.as_ptr().cast(), 0x1);
                    if let Some(table) = probe(handle) {
                        return Some(table);
                    }
                    let linked = libc::dlopen(std::ptr::null(), 0x1);
                    if let Some(table) = probe(linked) {
                        return Some(table);
                    }
                    None
                }
            })
            .as_ref()
    }

    /// One managed display/Space row keyed by its in-row
    /// `"Display Identifier"` (UUID match per yabai `display_space_list`:
    /// `CFEqual(uuid, identifier)` — exact, never positional). Unlike
    /// [`SlsDisplay`] (whose `uuid` pairs `SLSCopyManagedDisplays` order
    /// positionally), this row carries its own identifier, so multi-display
    /// homing is exact. A missing identifier leaves `current` unknown too
    /// (the current-Space query needs the UUID): `None`, never guessed.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct SlsDisplayRow {
        pub identifier: Option<String>,
        pub spaces: Vec<u64>,
        pub current: Option<u64>,
    }

    /// Read-only managed display/Space/current rows with in-row identifiers.
    /// Same bounds and container-kind verification as
    /// [`sls_display_spaces`]; IDs and counts only.
    pub fn sls_display_rows() -> Result<Vec<SlsDisplayRow>, String> {
        let table = sls_table().ok_or_else(|| "SLS symbols unavailable".to_owned())?;
        // SAFETY: resolved symbols; retained top-level result wrapped as
        // `CFType` and checked-downcast to `CFArray`; elements indexed
        // within `len().min(...)`; nested container kinds verified before
        // typed use; strings via `to_string`. Row identifiers come from
        // checked per-element downcasts of `"Display Identifier"`.
        unsafe {
            use objc2_core_foundation::CFDictionary as CFDictUntyped;
            let cid = (table.connection)();
            let spaces_raw = (table.copy_spaces)(cid);
            if spaces_raw.is_null() {
                return Err("SLSCopyManagedDisplaySpaces returned null".to_owned());
            }
            let spaces_top: CFRetained<CFType> =
                CFRetained::from_raw(NonNull::new(spaces_raw as *mut CFType).expect("non-null"));
            let spaces_untyped: CFRetained<CFArray> = spaces_top.downcast().map_err(|_| {
                "SLSCopyManagedDisplaySpaces returned a non-array top-level container".to_owned()
            })?;
            // SAFETY: top-level kind verified by the downcast above.
            let spaces_array: CFRetained<CFArray<CFType>> =
                CFRetained::cast_unchecked(spaces_untyped);
            let spaces_key = CFString::from_str("Spaces");
            let id_key = CFString::from_str("id64");
            let identifier_key = CFString::from_str("Display Identifier");
            let mut out = Vec::new();
            for index in 0..spaces_array.len().min(MAX_DISPLAYS) {
                let display_item = match spaces_array.get(index) {
                    Some(item) => item,
                    None => continue,
                };
                let mut identifier: Option<String> = None;
                let mut spaces: Vec<u64> = Vec::new();
                if display_item.downcast_ref::<CFDictUntyped>().is_some() {
                    // SAFETY: kind verified above; entries map `CFString`
                    // keys to `CFType` values.
                    let display_dict: CFRetained<CFDictionary<CFString, CFType>> =
                        CFRetained::cast_unchecked(display_item.clone());
                    identifier = display_dict
                        .get(&identifier_key)
                        .and_then(|v| v.downcast::<CFString>().ok())
                        .map(|s| s.to_string())
                        .filter(|s| !s.is_empty());
                    if let Some(spaces_val) = display_dict.get(&spaces_key)
                        && spaces_val.downcast_ref::<CFArray>().is_some()
                    {
                        // SAFETY: kind verified; space list holds dictionaries.
                        let spaces_list: CFRetained<CFArray<CFType>> =
                            CFRetained::cast_unchecked(spaces_val.clone());
                        for j in 0..spaces_list.len().min(MAX_SPACES_PER_DISPLAY) {
                            let space_item = match spaces_list.get(j) {
                                Some(item) => item,
                                None => continue,
                            };
                            if space_item.downcast_ref::<CFDictUntyped>().is_some() {
                                // SAFETY: kind verified; space entries map
                                // `CFString` keys to `CFType` values.
                                let space_dict: CFRetained<CFDictionary<CFString, CFType>> =
                                    CFRetained::cast_unchecked(space_item.clone());
                                if let Some(sid_val) = space_dict.get(&id_key)
                                    && let Ok(num) = sid_val.downcast::<CFNumber>()
                                    && let Some(sid) = num.as_i64()
                                    && sid >= 0
                                {
                                    spaces.push(sid as u64);
                                }
                            }
                        }
                    }
                }
                // The current-Space query needs the row's own identifier;
                // without one the current stays unknown (never guessed from
                // another row).
                let current = match identifier.clone() {
                    Some(uuid) => {
                        let uuid_cf = CFString::from_str(&uuid);
                        let uuid_ptr =
                            CFRetained::as_ptr(&uuid_cf).as_ptr() as *const std::ffi::c_void;
                        Some((table.current_space)(cid, uuid_ptr))
                    }
                    None => None,
                };
                out.push(SlsDisplayRow {
                    identifier,
                    spaces,
                    current,
                });
            }
            Ok(out)
        }
    }

    // -- Main-thread display/Space-change guard (read-only events) --
    //
    // Shape pinned against glide `src/actor/notification_center.rs`
    // (`NotificationCenterInner`: `NSApplicationDidChangeScreenParametersNotification`
    // on the default center, `NSWorkspaceActiveSpaceDidChangeNotification` on
    // the workspace center) at
    // https://github.com/tmandry/glide/blob/ef3b30f88445fe4b517eb4c60023011f6d3725dd/src/actor/notification_center.rs
    // and rift `src/actor/notification_center.rs` (same two names; rift
    // additionally registers with a nil object for screen parameters) at
    // https://github.com/acsandmann/rift/blob/8114ccc534b5c9a7e6a77ce27568d8739be9216f/src/actor/notification_center.rs
    // Differences (deliberate): the callbacks do NO snapshot/resnapshot work
    // and NO AX reads — they only bump one atomic generation. The controller
    // polls [`DisplayChangeFlag::take_changed`] and re-snapshots on the main
    // thread itself. No AppKit object crosses threads: the guard
    // (`!Send + !Sync`, main-thread-owned) holds the observer; only the
    // `Send + Sync` flag (plain atomics) is shared.

    #[repr(C)]
    struct DisplayChangeIvars {
        dirty: Arc<AtomicU64>,
    }

    // SAFETY: opaque observer state passed through the `initWith:` shape
    // (rift/glide prior art above: `#[repr(C)]` struct, `Object` encoding,
    // carried to `set_ivars`); never dereferenced as an ObjC object.
    unsafe impl objc2::Encode for DisplayChangeIvars {
        const ENCODING: objc2::Encoding = objc2::Encoding::Object;
    }

    // Method receivers (`alloc`, `ivars`) resolve through these (rift/glide
    // `notification_center.rs` shape).
    use objc2::{AnyThread, DeclaredClass};

    objc2::define_class! {
        // SAFETY: `NSObject` imposes no subclassing requirements and this
        // class implements no `Drop`; callbacks bump one atomic only (no AX,
        // no layout, no logging, no panics).
        #[unsafe(super(objc2_foundation::NSObject))]
        #[ivars = Box<DisplayChangeIvars>]
        struct DisplayChangeObserver;

        // SAFETY: each signature matches its selector invocation below
        // (`name:` with one `NSNotification` argument).
        impl DisplayChangeObserver {
            #[unsafe(method(displayParametersDidChange:))]
            fn display_parameters_did_change(&self, _notif: &objc2_foundation::NSNotification) {
                // Minimal callback: bump only. No snapshot, no AX, no log.
                self.ivars().dirty.fetch_add(1, Ordering::SeqCst);
            }

            #[unsafe(method(activeSpaceDidChange:))]
            fn active_space_did_change(&self, _notif: &objc2_foundation::NSNotification) {
                // Minimal callback: bump only. No snapshot, no AX, no log.
                self.ivars().dirty.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    /// Main-thread-owned observer guard. `!Send + !Sync` via
    /// `PhantomData<Rc>` (no `unsafe impl`): install, use, and drop on the
    /// main thread. `Drop` unregisters best-effort (`NSNotificationCenter`
    /// removal is thread-safe; explicit drop on the main thread is the
    /// honest path).
    pub struct DisplayChangeGuard {
        observer: objc2::rc::Retained<DisplayChangeObserver>,
        _not_send: std::marker::PhantomData<std::rc::Rc<()>>,
    }

    impl Drop for DisplayChangeGuard {
        fn drop(&mut self) {
            // SAFETY: live observer in maintained wrappers; removals are
            // teardown-only and thread-safe; statuses ignored (explicit
            // main-thread drop is the honest path).
            unsafe {
                use objc2_app_kit::NSWorkspace;
                use objc2_foundation::NSNotificationCenter;
                NSNotificationCenter::defaultCenter().removeObserver(&self.observer);
                NSWorkspace::sharedWorkspace()
                    .notificationCenter()
                    .removeObserver(&self.observer);
            }
        }
    }

    impl DisplayChangeGuard {
        /// Clone the shareable dirty flag for the controller thread.
        #[must_use]
        pub fn flag(&self) -> DisplayChangeFlag {
            DisplayChangeFlag {
                dirty: self.observer.ivars().dirty.clone(),
            }
        }
    }

    /// Install the display/Space-change observer on the calling (main)
    /// thread. Registers `NSApplicationDidChangeScreenParametersNotification`
    /// (default center, nil object — rift shape) and
    /// `NSWorkspaceActiveSpaceDidChangeNotification` (workspace center,
    /// workspace object — glide/rift shape). Returns the main-thread guard
    /// plus the shareable flag. Any other thread fails closed before
    /// touching notification state.
    pub fn install_display_change_guard() -> Result<(DisplayChangeGuard, DisplayChangeFlag), String>
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSWorkspace;
        use objc2_foundation::NSNotificationCenter;
        let Some(_mtm) = MainThreadMarker::new() else {
            return Err("display change guard requires the main thread".to_owned());
        };
        let flag = DisplayChangeFlag::new();
        // Bound before the macro: a struct literal inline would parse as a
        // block inside `msg_send!`.
        let ivars = DisplayChangeIvars {
            dirty: flag.dirty.clone(),
        };
        // Ivars set directly, then the superclass `init` (the `define_class!`
        // doc shape): avoids an `initWith:` round-trip through `msg_send!`.
        let allocated: objc2::rc::Allocated<DisplayChangeObserver> = DisplayChangeObserver::alloc();
        let observer: objc2::rc::Retained<DisplayChangeObserver> =
            unsafe { objc2::msg_send![super(allocated.set_ivars(Box::new(ivars))), init] };
        // SAFETY: observer/centers/names live in maintained wrappers;
        // selectors match the `define_class!` methods above; the observer
        // is retained by the guard (centers hold weak references).
        unsafe {
            use objc2::sel;
            use objc2_app_kit::{
                NSApplicationDidChangeScreenParametersNotification,
                NSWorkspaceActiveSpaceDidChangeNotification,
            };
            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                &observer,
                sel!(displayParametersDidChange:),
                Some(NSApplicationDidChangeScreenParametersNotification),
                None,
            );
            let workspace = NSWorkspace::sharedWorkspace();
            workspace
                .notificationCenter()
                .addObserver_selector_name_object(
                    &observer,
                    sel!(activeSpaceDidChange:),
                    Some(NSWorkspaceActiveSpaceDidChangeNotification),
                    Some(&workspace),
                );
        }
        Ok((
            DisplayChangeGuard {
                observer,
                _not_send: std::marker::PhantomData,
            },
            flag,
        ))
    }
}

#[cfg(target_os = "macos")]
pub use macos::{
    AX_ATTRIBUTE_UNSUPPORTED, AX_FAILURE, AX_ILLEGAL_ARGUMENT, AX_NO_VALUE, AxElement, AxFrame,
    CgBounds, CgWindow, CgWindowBasic, DisplayChangeGuard, DisplayRect, KeyCounts, NsAppIdentity,
    NsDisplay, Phase1Observer, SLPS_CPS_USER_GENERATED, SLPS_EVENT_LEN, SlpsPsn,
    SlsBridgedAvailability, SlsDisplayRow, SlsMoveSymbols, TapChordSpec, TapHit, TapPump,
    TapSource, TapSourceReport, activate_app, activation_policy_raw_for_pid, app_hidden_for_pid,
    ax_app_focus_leg, ax_bool_attribute, ax_children, ax_direct_focused_read, ax_display_id,
    ax_focused_preimage, ax_focused_preimage_detailed, ax_focused_preimage_evidence,
    ax_fullscreen_state, ax_get_frame, ax_identifier, ax_is_process_trusted, ax_is_size_settable,
    ax_min_size, ax_observe_bounded, ax_observe_session, ax_perform_action, ax_raise,
    ax_selected_focus_evidence, ax_set_bool_attribute, ax_set_focused_window, ax_set_frame,
    ax_set_messaging_timeout, ax_set_position, ax_set_size, ax_string_attribute,
    ax_system_focus_leg, ax_window_id, ax_window_id_detailed, ax_window_title, bundle_id_for_pid,
    cg_display_uuid, cg_post_event_access_known, cg_window_list_all, cg_window_list_basic,
    copy_symbolic_hotkeys, drain_app_events, drain_app_events_counted, frontmost_app,
    install_display_change_guard, key_counts_snapshot, launch_date_for_pid, macos_major,
    mc_launch_toggle, ns_app_identity_for_pid, ns_display_snapshot, ns_separate_spaces,
    own_bundle_id, process_executable_path, process_exists, pump_runloop_once,
    run_event_tap_bounded, run_owned_probe_window, runloop_present, running_apps,
    secure_input_known, session_state, slps_available, slps_front_available, slps_front_pid,
    slps_post_event, slps_psn_for_pid, slps_set_front, sls_bridged_availability,
    sls_connection_available, sls_display_rows, sls_display_spaces, sls_main_connection_id,
    sls_move_symbols, sls_move_windows_direct, sls_space_type, sls_spaces_for_window,
    sls_spaces_for_windows, terminate_process,
};

#[cfg(not(target_os = "macos"))]
mod stub {
    use super::*;
    use std::time::Duration;

    pub struct AxElement {
        _priv: (),
        _not_send: std::marker::PhantomData<std::rc::Rc<()>>,
    }

    impl AxElement {
        pub fn application(_pid: i32) -> Option<Self> {
            None
        }

        pub fn system_wide() -> Option<Self> {
            None
        }

        pub fn windows(&self) -> Result<Vec<AxElement>, i32> {
            Err(AX_NOT_IMPLEMENTED)
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    pub struct AxFrame {
        pub x: f64,
        pub y: f64,
        pub w: f64,
        pub h: f64,
    }

    impl AxFrame {
        pub fn valid_for_set(&self) -> bool {
            self.x.is_finite()
                && self.y.is_finite()
                && self.w.is_finite()
                && self.h.is_finite()
                && self.w > 0.0
                && self.h > 0.0
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TapChordSpec {
        pub want_flags: u64,
        pub flags_mask: u64,
        pub want_code: i64,
        pub key_code: u32,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TapHit {
        pub key_code: u32,
        pub down: bool,
        pub flags: u64,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TapPump {
        Held,
        TimeoutRecovered,
        NeedsRecreate,
        UserDisabled,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct TapSourceReport {
        pub matched: u32,
        pub suppressed: u32,
        pub deliveries: u32,
        pub reenables_after_timeout: u32,
        pub user_disables: u32,
        pub unnotified_disables: u32,
    }

    pub struct TapSource {
        _priv: (),
        _not_send: std::marker::PhantomData<std::rc::Rc<()>>,
    }

    // Preserve the native RAII guard's teardown boundary. Stub creation
    // always refuses, so there is no native resource to release here.
    impl Drop for TapSource {
        fn drop(&mut self) {}
    }

    impl TapSource {
        pub fn create(
            _chords: &[TapChordSpec],
            _suppress_armed: bool,
            _tx: &SyncSender<TapHit>,
        ) -> Result<Self, TapSetupError> {
            Err(TapSetupError::Denied("event tap is macOS-only".to_owned()))
        }

        pub fn create_scoped(
            _chords: &[TapChordSpec],
            _suppress_armed: bool,
            _tx: &SyncSender<TapHit>,
            _eligibility: std::sync::Arc<crate::input_eligibility::EligibilityCache>,
        ) -> Result<Self, TapSetupError> {
            Err(TapSetupError::Denied("event tap is macOS-only".to_owned()))
        }

        pub fn create_stall_once(
            _chords: &[TapChordSpec],
            _suppress_armed: bool,
            _tx: &SyncSender<TapHit>,
            _stall_ms: u64,
        ) -> Result<Self, TapSetupError> {
            Err(TapSetupError::Denied("event tap is macOS-only".to_owned()))
        }

        pub fn pump(&mut self, _millis: u64) -> TapPump {
            TapPump::Held
        }

        pub fn set_enabled(&self, _enabled: bool) {}

        pub fn set_tap_direct_enabled(&self, _enabled: bool) {}

        #[must_use]
        pub fn is_tap_direct_enabled(&self) -> bool {
            false
        }

        /// Fail-closed stub: no live tap exists off-macOS, so a same-tap
        /// re-enable can never verify here.
        pub fn reenable_same_tap_verified(&self) -> bool {
            false
        }

        /// Fail-closed stub: no live tap exists off-macOS; the count has
        /// nowhere to land.
        pub fn note_unnotified_disabled(&self) {}

        #[must_use]
        pub fn report(&self) -> TapSourceReport {
            TapSourceReport {
                matched: 0,
                suppressed: 0,
                deliveries: 0,
                reenables_after_timeout: 0,
                user_disables: 0,
                unnotified_disables: 0,
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    pub struct CgBounds {
        pub x: f64,
        pub y: f64,
        pub w: f64,
        pub h: f64,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct CgWindow {
        pub wid: u32,
        pub pid: u32,
        pub layer: i32,
        pub onscreen: bool,
        pub bounds: CgBounds,
        pub title: Option<String>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    pub struct CgWindowBasic {
        pub wid: u32,
        pub pid: u32,
        pub layer: i32,
        pub onscreen: bool,
        pub bounds: CgBounds,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    pub struct DisplayRect {
        pub x: f64,
        pub y: f64,
        pub w: f64,
        pub h: f64,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct NsDisplay {
        pub cg_id: u32,
        pub uuid: Option<String>,
        pub frame: DisplayRect,
        pub visible: DisplayRect,
        pub scale: f64,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct SlsDisplayRow {
        pub identifier: Option<String>,
        pub spaces: Vec<u64>,
        pub current: Option<u64>,
    }

    pub struct DisplayChangeGuard {
        _priv: (),
        _not_send: std::marker::PhantomData<std::rc::Rc<()>>,
    }

    pub fn install_display_change_guard() -> Result<(DisplayChangeGuard, DisplayChangeFlag), String>
    {
        Err("macOS-only".to_owned())
    }

    pub fn cg_display_uuid(_display_id: u32) -> Option<String> {
        None
    }

    pub fn ns_separate_spaces() -> Option<bool> {
        None
    }

    pub fn sls_display_rows() -> Result<Vec<SlsDisplayRow>, String> {
        Err("macOS-only".to_owned())
    }

    pub fn cg_window_list_basic() -> Result<Vec<CgWindowBasic>, String> {
        Err("macOS-only".to_owned())
    }

    pub fn ns_display_snapshot() -> Result<Vec<NsDisplay>, String> {
        Err("macOS-only".to_owned())
    }

    pub fn ax_string_attribute(_element: &AxElement, _name: &str) -> Result<Option<String>, i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn ax_bool_attribute(_element: &AxElement, _name: &str) -> Result<bool, i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn ax_set_bool_attribute(
        _element: &AxElement,
        _name: &str,
        _value: bool,
    ) -> Result<(), i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn ax_is_size_settable(_element: &AxElement) -> Result<bool, i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn ax_min_size(_element: &AxElement) -> Result<Option<(f64, f64)>, i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn ax_fullscreen_state(_element: &AxElement) -> Result<bool, i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn app_hidden_for_pid(_pid: i32) -> Result<bool, String> {
        Err("macOS-only".to_owned())
    }

    pub fn activation_policy_raw_for_pid(_pid: i32) -> Result<i64, String> {
        Err("macOS-only".to_owned())
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct SlsMoveSymbols {
        pub direct: bool,
        pub compat_pair: bool,
        pub bridged_async: bool,
    }

    /// Bridged-route availability off-macOS: never present, never dispatched.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct SlsBridgedAvailability {
        pub fn_present: bool,
        pub class_present: bool,
        pub available: bool,
    }

    pub fn sls_bridged_availability() -> SlsBridgedAvailability {
        SlsBridgedAvailability {
            fn_present: false,
            class_present: false,
            available: false,
        }
    }

    pub fn cg_window_list_all() -> Result<Vec<CgWindow>, String> {
        Err("macOS-only".to_owned())
    }

    pub fn sls_move_symbols() -> SlsMoveSymbols {
        SlsMoveSymbols {
            direct: false,
            compat_pair: false,
            bridged_async: false,
        }
    }

    pub fn sls_move_windows_direct(_sid: u64, _wids: &[u32]) -> Result<(), String> {
        Err("macOS-only".to_owned())
    }

    pub fn sls_space_type(_sid: u64) -> Result<super::SlsSpaceType, String> {
        Err("macOS-only".to_owned())
    }

    pub fn ax_is_process_trusted() -> Option<bool> {
        None
    }

    pub fn ax_set_messaging_timeout(_element: &AxElement, _seconds: f32) -> i32 {
        AX_NOT_IMPLEMENTED
    }

    pub fn ax_window_id(_element: &AxElement) -> Option<u32> {
        None
    }

    pub fn ax_window_id_detailed(_element: &AxElement) -> Result<u32, String> {
        Err("macOS-only".to_owned())
    }

    pub fn ax_focused_preimage() -> Option<(i32, Option<u32>)> {
        None
    }

    pub fn ax_focused_preimage_detailed() -> Result<(i32, Option<u32>), String> {
        Err("macOS-only".to_owned())
    }

    pub fn runloop_present() -> bool {
        false
    }

    pub fn ax_system_focus_leg() -> super::AxSystemFocusLeg {
        super::AxSystemFocusLeg {
            system_element: "macOS-only".to_owned(),
            system_timeout_status: None,
            system_timeout_name: None,
            focused_app_status: "macOS-only".to_owned(),
            focused_pid: None,
            app_timeout_status: None,
            app_timeout_name: None,
            app_pid_status: "macOS-only".to_owned(),
            focused_window_status: "macOS-only".to_owned(),
            focused_wid: None,
        }
    }

    pub fn ax_app_focus_leg(pid: i32) -> super::AxAppFocusLeg {
        super::AxAppFocusLeg {
            pid,
            element: "macOS-only".to_owned(),
            timeout_status: None,
            timeout_name: None,
            windows_status: "macOS-only".to_owned(),
            windows_count: None,
            window_join_failures: 0,
            joined_wids: Vec::new(),
            focused_status: "macOS-only".to_owned(),
            focused_wid: None,
            main_status: "macOS-only".to_owned(),
            main_wid: None,
            frontmost_before_status: "macOS-only".to_owned(),
            frontmost_before: None,
            frontmost_after_status: "macOS-only".to_owned(),
            frontmost_after: None,
        }
    }

    pub fn slps_front_available() -> bool {
        false
    }

    pub fn slps_front_pid() -> Result<i32, String> {
        Err("macOS-only".to_owned())
    }

    pub fn ax_focused_preimage_evidence() -> Result<super::PreimageEvidence, String> {
        Err("macOS-only".to_owned())
    }

    pub fn ax_direct_focused_read(pid: i32) -> super::DirectFocusRead {
        super::DirectFocusRead {
            pid,
            ax_pid: None,
            focused_status: "macOS-only".to_owned(),
            focused_wid: None,
            main_status: "macOS-only".to_owned(),
            main_wid: None,
            selected_wid: None,
            selected_source: "none".to_owned(),
        }
    }

    pub fn ax_selected_focus_evidence() -> Result<super::PreimageEvidence, String> {
        Err("macOS-only".to_owned())
    }

    pub fn ax_get_frame(_element: &AxElement) -> Result<AxFrame, i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn ax_set_frame(_element: &AxElement, _frame: &AxFrame) -> Result<(), i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn ax_set_position(_element: &AxElement, _frame: &AxFrame) -> Result<(), i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn ax_set_size(_element: &AxElement, _frame: &AxFrame) -> Result<(), i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn ax_window_title(_element: &AxElement) -> Result<Option<String>, i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn cg_post_event_access_known() -> Option<bool> {
        None
    }

    pub fn ax_raise(_element: &AxElement) -> Result<(), i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn ax_set_focused_window(_app: &AxElement, _window: &AxElement) -> Result<(), i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn ax_children(_element: &AxElement) -> Result<Vec<AxElement>, i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn ax_identifier(_element: &AxElement) -> Result<Option<String>, i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn ax_display_id(_element: &AxElement) -> Result<Option<i64>, i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn ax_perform_action(_element: &AxElement, _name: &str) -> Result<(), i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn mc_launch_toggle() -> bool {
        false
    }

    pub fn ax_observe_bounded(
        _pid: i32,
        _timeout: Duration,
        _messaging_timeout_secs: f32,
        _tx: &SyncSender<EngineMessage>,
    ) -> Result<AxObserveOutcome, AxObserveError> {
        Err(AxObserveError::Unsupported("macOS-only".to_owned()))
    }

    pub fn ax_observe_session(
        _pid: i32,
        _window_wid: Option<u32>,
        _timeout: Duration,
        _messaging_timeout_secs: f32,
        _tx: &SyncSender<EngineMessage>,
    ) -> Result<AxSessionOutcome, AxObserveError> {
        Err(AxObserveError::Unsupported("macOS-only".to_owned()))
    }

    /// Off-macOS stub for the Phase 1 thread-affine observer guard: creation
    /// always fails fast (AX is macOS-only); every other method is a no-op so
    /// portable code paths compile without live behavior.
    pub struct Phase1Observer {
        _priv: (),
        _not_send: std::marker::PhantomData<std::rc::Rc<()>>,
    }

    impl Phase1Observer {
        pub fn create(_pid: i32, _messaging_timeout_secs: f32) -> Result<Self, String> {
            Err("Phase1Observer is macOS-only".to_owned())
        }

        pub fn pump(&self, _slice: Duration) {}

        pub fn take_notified(&self) -> u32 {
            0
        }

        pub fn sync_windows(
            &mut self,
            _wids: &[u32],
            _messaging_timeout_secs: f32,
        ) -> Result<(), String> {
            Ok(())
        }

        pub fn focused_wid(&self, _messaging_timeout_secs: f32) -> Option<u32> {
            None
        }

        pub fn shutdown(self) -> Result<(), String> {
            Ok(())
        }
    }

    pub fn secure_input_known() -> Option<bool> {
        None
    }

    /// Off-platform session: no window server, so the lock state is unknown
    /// (fail closed everywhere it gates, same as an unreadable live session).
    pub fn session_state() -> SessionState {
        SessionState::unknown()
    }

    /// Off-platform helper: never bundled.
    pub fn own_bundle_id() -> Option<String> {
        None
    }

    pub fn sls_connection_available() -> bool {
        false
    }

    pub fn sls_main_connection_id() -> Option<i32> {
        None
    }

    pub fn sls_display_spaces() -> Result<Vec<SlsDisplay>, String> {
        Err("macOS-only".to_owned())
    }

    pub fn sls_spaces_for_windows(_wids: &[u32]) -> Result<Vec<u64>, String> {
        Err("macOS-only".to_owned())
    }

    pub fn sls_spaces_for_window(_wid: u32) -> Result<Vec<u64>, String> {
        Err("macOS-only".to_owned())
    }

    pub fn copy_symbolic_hotkeys() -> Result<Vec<super::SymbolicHotKeyEntry>, String> {
        Err("macOS-only".to_owned())
    }

    pub fn macos_major() -> Option<u32> {
        None
    }

    pub fn running_apps() -> Vec<(i32, String)> {
        Vec::new()
    }

    pub fn bundle_id_for_pid(_pid: i32) -> Option<String> {
        None
    }

    pub fn frontmost_app() -> Option<(i32, String)> {
        None
    }

    pub fn activate_app(_pid: i32) -> bool {
        false
    }

    pub fn process_executable_path(_pid: i32) -> Option<String> {
        None
    }

    pub fn launch_date_for_pid(_pid: i32) -> Option<String> {
        None
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct NsAppIdentity {
        pub bundle: Option<String>,
        pub launch_date: Option<String>,
        pub exe: Option<String>,
    }

    pub fn ns_app_identity_for_pid(_pid: i32) -> NsAppIdentity {
        NsAppIdentity {
            bundle: None,
            launch_date: None,
            exe: None,
        }
    }

    pub fn process_exists(_pid: i32) -> bool {
        false
    }

    pub fn terminate_process(_pid: i32) -> bool {
        false
    }

    pub fn pump_runloop_once(_millis: u64) {}

    pub fn run_event_tap_bounded(
        _want_flags: u64,
        _flags_mask: u64,
        _want_keycode: i64,
        _seconds: u64,
        _suppress_armed: bool,
        _tx: &SyncSender<EngineMessage>,
    ) -> Result<TapRunReport, TapSetupError> {
        Err(TapSetupError::Denied("macOS-only".to_owned()))
    }

    // Keep the unsupported stub signature and lint policy in parity with macOS.
    #[allow(clippy::too_many_arguments)]
    pub fn run_owned_probe_window(
        _seconds: u64,
        _hung_ms: u64,
        _title: &str,
        _fixed_size: bool,
        _defer_window_ms: u64,
        _close_window_ms: u64,
        _regular_policy: bool,
        _transient_panel_ms: u64,
        _transient_sheet_ms: u64,
        _space_join_poll: bool,
        _ready: &dyn Fn(),
    ) -> Result<(), String> {
        Err("owned probe window is macOS-only".to_owned())
    }

    /// Stub downstream key-delivery counters (always zero off-macOS).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct KeyCounts {
        pub h_down: [u64; 16],
        pub h_up: [u64; 16],
        pub other_down: u64,
        pub other_up: u64,
    }

    #[must_use]
    pub fn key_counts_snapshot() -> KeyCounts {
        KeyCounts {
            h_down: [0; 16],
            h_up: [0; 16],
            other_down: 0,
            other_up: 0,
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SlpsPsn {
        pub high: u32,
        pub low: u32,
    }

    /// Same 8-byte layout guard as the macOS `SlpsPsn` above.
    const _: () = assert!(std::mem::size_of::<SlpsPsn>() == 8);

    pub const SLPS_CPS_USER_GENERATED: u32 = 0x200;
    pub const SLPS_EVENT_LEN: usize = 0xf8;

    pub fn slps_available() -> bool {
        false
    }

    pub fn slps_psn_for_pid(_pid: i32) -> Result<SlpsPsn, String> {
        Err("macOS-only".to_owned())
    }

    pub fn slps_set_front(_psn: &SlpsPsn, _wid: u32, _mode: u32) -> Result<(), i32> {
        Err(AX_NOT_IMPLEMENTED)
    }

    pub fn slps_post_event(_psn: &SlpsPsn, _bytes: &[u8; SLPS_EVENT_LEN]) -> Result<(), i32> {
        Err(AX_NOT_IMPLEMENTED)
    }
}

#[cfg(not(target_os = "macos"))]
pub use stub::{
    AxElement, AxFrame, CgBounds, CgWindow, CgWindowBasic, DisplayChangeGuard, DisplayRect,
    KeyCounts, NsAppIdentity, NsDisplay, Phase1Observer, SLPS_CPS_USER_GENERATED, SLPS_EVENT_LEN,
    SlpsPsn, SlsBridgedAvailability, SlsDisplayRow, SlsMoveSymbols, TapChordSpec, TapHit, TapPump,
    TapSource, TapSourceReport, activate_app, activation_policy_raw_for_pid, app_hidden_for_pid,
    ax_app_focus_leg, ax_bool_attribute, ax_children, ax_direct_focused_read, ax_display_id,
    ax_focused_preimage, ax_focused_preimage_detailed, ax_focused_preimage_evidence,
    ax_fullscreen_state, ax_get_frame, ax_identifier, ax_is_process_trusted, ax_is_size_settable,
    ax_min_size, ax_observe_bounded, ax_observe_session, ax_perform_action, ax_raise,
    ax_selected_focus_evidence, ax_set_bool_attribute, ax_set_focused_window, ax_set_frame,
    ax_set_messaging_timeout, ax_set_position, ax_set_size, ax_string_attribute,
    ax_system_focus_leg, ax_window_id, ax_window_id_detailed, ax_window_title, bundle_id_for_pid,
    cg_display_uuid, cg_post_event_access_known, cg_window_list_all, cg_window_list_basic,
    copy_symbolic_hotkeys, frontmost_app, install_display_change_guard, key_counts_snapshot,
    launch_date_for_pid, macos_major, mc_launch_toggle, ns_app_identity_for_pid,
    ns_display_snapshot, ns_separate_spaces, own_bundle_id, process_executable_path,
    process_exists, pump_runloop_once, run_event_tap_bounded, run_owned_probe_window,
    runloop_present, running_apps, secure_input_known, session_state, slps_available,
    slps_front_available, slps_front_pid, slps_post_event, slps_psn_for_pid, slps_set_front,
    sls_bridged_availability, sls_connection_available, sls_display_rows, sls_display_spaces,
    sls_main_connection_id, sls_move_symbols, sls_move_windows_direct, sls_space_type,
    sls_spaces_for_window, sls_spaces_for_windows, terminate_process,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ax_error_names_cover_known_codes() {
        assert_eq!(ax_error_name(0), "kAXErrorSuccess");
        assert!(ax_error_name(-25204).contains("CannotComplete"));
        assert!(ax_error_name(99999).contains("99999"));
    }

    #[test]
    fn ax_error_codes_bind_sdk_constants() {
        // Every maintained `objc2-application-services::AXError` constant
        // must equal its SDK `AXError.h` value, and `ax_error_name` must
        // classify each one under its exact name. This pins the table
        // against the historic shift that mislabeled actual `NoValue`
        // (-25212) as `NotEnoughPrecision`: any drift fails here, never in
        // a live traversal. The `objc2` binding is macOS-only, so the
        // constant side is cfg-gated while the formatting side runs
        // everywhere (stub reports must classify identically).
        #[cfg(target_os = "macos")]
        {
            use objc2_application_services::AXError;
            assert_eq!(AXError::Success.0, 0);
            assert_eq!(AXError::Failure.0, -25200);
            assert_eq!(AXError::IllegalArgument.0, -25201);
            assert_eq!(AXError::InvalidUIElement.0, -25202);
            assert_eq!(AXError::InvalidUIElementObserver.0, -25203);
            assert_eq!(AXError::CannotComplete.0, -25204);
            assert_eq!(AXError::AttributeUnsupported.0, -25205);
            assert_eq!(AXError::ActionUnsupported.0, -25206);
            assert_eq!(AXError::NotificationUnsupported.0, -25207);
            assert_eq!(AXError::NotImplemented.0, -25208);
            assert_eq!(AXError::NotificationAlreadyRegistered.0, -25209);
            assert_eq!(AXError::NotificationNotRegistered.0, -25210);
            assert_eq!(AXError::APIDisabled.0, -25211);
            assert_eq!(AXError::NoValue.0, -25212);
            assert_eq!(AXError::ParameterizedAttributeUnsupported.0, -25213);
            assert_eq!(AXError::NotEnoughPrecision.0, -25214);
            // The match-site constants bind the same maintained values.
            assert_eq!(super::AX_NO_VALUE, AXError::NoValue.0);
            assert_eq!(
                super::AX_ATTRIBUTE_UNSUPPORTED,
                AXError::AttributeUnsupported.0
            );
            assert_eq!(super::AX_FAILURE, AXError::Failure.0);
            assert_eq!(super::AX_ILLEGAL_ARGUMENT, AXError::IllegalArgument.0);
            assert_eq!(super::AX_NOT_IMPLEMENTED, AXError::NotImplemented.0);
        }
        // Classification/formatting for the full table, all platforms.
        for (code, name) in [
            (0, "kAXErrorSuccess"),
            (-25200, "kAXErrorFailure"),
            (-25201, "kAXErrorIllegalArgument"),
            (-25202, "kAXErrorInvalidUIElement"),
            (-25203, "kAXErrorInvalidUIElementObserver"),
            (-25204, "kAXErrorCannotComplete"),
            (-25205, "kAXErrorAttributeUnsupported"),
            (-25206, "kAXErrorActionUnsupported"),
            (-25207, "kAXErrorNotificationUnsupported"),
            (-25208, "kAXErrorNotImplemented"),
            (-25209, "kAXErrorNotificationAlreadyRegistered"),
            (-25210, "kAXErrorNotificationNotRegistered"),
            (-25211, "kAXErrorAPIDisabled"),
            (-25212, "kAXErrorNoValue"),
            (-25213, "kAXErrorParameterizedAttributeUnsupported"),
            (-25214, "kAXErrorNotEnoughPrecision"),
        ] {
            assert_eq!(ax_error_name(code), name, "code {code}");
        }
        // Unknown codes surface numerically, never guessed.
        assert_eq!(ax_error_name(99999), "AXError 99999");
        assert_eq!(ax_error_name(-25999), "AXError -25999");
    }

    #[test]
    fn fallback_trigger_is_exactly_the_service_failure() {
        // The exactly observed host failures trigger; every other outcome
        // never does (success, window-leg failures, timeout legs,
        // malformed/downcast, permission, unknown text). Diagnostic-only.
        assert!(super::preimage_fallback_trigger(
            "AXFocusedApplication unreadable: kAXErrorCannotComplete"
        ));
        assert!(super::preimage_fallback_trigger(
            "system-wide AX element unavailable"
        ));
        assert!(!super::preimage_fallback_trigger(
            "AXFocusedWindow unreadable: kAXErrorCannotComplete"
        ));
        assert!(!super::preimage_fallback_trigger(
            "AXFocusedWindow join: _AXUIElementGetWindow status -25204"
        ));
        assert!(!super::preimage_fallback_trigger(
            "system-wide AX messaging timeout unset: kAXErrorFailure"
        ));
        assert!(!super::preimage_fallback_trigger(
            "focused application AX messaging timeout unset: kAXErrorFailure"
        ));
        assert!(!super::preimage_fallback_trigger(
            "AXFocusedApplication is not an AXUIElement"
        ));
        assert!(!super::preimage_fallback_trigger(
            "AXFocusedApplication unreadable: kAXErrorFailure"
        ));
        assert!(!super::preimage_fallback_trigger(
            "AXFocusedApplication unreadable: kAXErrorIllegalArgument"
        ));
        assert!(!super::preimage_fallback_trigger(
            "AXFocusedApplication unreadable: kAXErrorInvalidUIElement"
        ));
        assert!(!super::preimage_fallback_trigger(
            "AXFocusedApplication unreadable: kAXErrorNoValue"
        ));
        assert!(!super::preimage_fallback_trigger(
            "AXFocusedApplication unreadable: kAXErrorNotImplemented"
        ));
        assert!(!super::preimage_fallback_trigger(
            "focused application pid unreadable: kAXErrorCannotComplete"
        ));
        assert!(!super::preimage_fallback_trigger("kAXErrorSuccess"));
        assert!(!super::preimage_fallback_trigger(""));
    }

    #[test]
    fn frontmost_pair_requires_both_true_and_stable() {
        // Pure instability regression: any unknown/false/disagreement is
        // instability data that refuses, never a proven host claim.
        use super::frontmost_pair_stable as stable;
        assert!(stable(Some(true), Some(true)));
        assert!(!stable(Some(true), Some(false)));
        assert!(!stable(Some(false), Some(true)));
        assert!(!stable(Some(false), Some(false)));
        assert!(!stable(Some(true), None));
        assert!(!stable(None, Some(true)));
        assert!(!stable(None, None));
    }

    #[test]
    fn fallback_agrees_only_on_full_corroboration() {
        use super::preimage_fallback_agrees as agrees;
        // All legs agree, frontmost before+after both true/stable.
        assert!(agrees(
            100,
            Some(100),
            Some(100),
            Some(true),
            Some(true),
            Some(100),
            Some(7),
            true
        ));
        // Reserved PSN refuses.
        assert!(!agrees(
            1,
            Some(1),
            Some(1),
            Some(true),
            Some(true),
            Some(1),
            Some(7),
            true
        ));
        // PSN/workspace disagreement refuses (never fabricated).
        assert!(!agrees(
            100,
            Some(200),
            Some(200),
            Some(true),
            Some(true),
            Some(100),
            Some(7),
            true
        ));
        // Workspace instability mid-read refuses.
        assert!(!agrees(
            100,
            Some(100),
            Some(200),
            Some(true),
            Some(true),
            Some(100),
            Some(7),
            true
        ));
        assert!(!agrees(
            100,
            None,
            Some(100),
            Some(true),
            Some(true),
            Some(100),
            Some(7),
            true
        ));
        // Unknown/false AXFrontmost on either side refuses.
        assert!(!agrees(
            100,
            Some(100),
            Some(100),
            Some(false),
            Some(true),
            Some(100),
            Some(7),
            true
        ));
        assert!(!agrees(
            100,
            Some(100),
            Some(100),
            Some(true),
            Some(false),
            Some(100),
            Some(7),
            true
        ));
        assert!(!agrees(
            100,
            Some(100),
            Some(100),
            Some(true),
            None,
            Some(100),
            Some(7),
            true
        ));
        assert!(!agrees(
            100,
            Some(100),
            Some(100),
            None,
            Some(true),
            Some(100),
            Some(7),
            true
        ));
        // AX identity mismatch refuses.
        assert!(!agrees(
            100,
            Some(100),
            Some(100),
            Some(true),
            Some(true),
            Some(200),
            Some(7),
            true
        ));
        // Missing/zero window refuses (no wid => no restorable focus).
        assert!(!agrees(
            100,
            Some(100),
            Some(100),
            Some(true),
            Some(true),
            Some(100),
            None,
            true
        ));
        assert!(!agrees(
            100,
            Some(100),
            Some(100),
            Some(true),
            Some(true),
            Some(100),
            Some(0),
            true
        ));
        // Lost CG join refuses.
        assert!(!agrees(
            100,
            Some(100),
            Some(100),
            Some(true),
            Some(true),
            Some(100),
            Some(7),
            false
        ));
    }

    #[test]
    fn preimage_route_names_are_stable() {
        assert_eq!(super::PreimageRoute::Systemwide.as_str(), "systemwide");
        assert_eq!(
            super::PreimageRoute::SlpsFrontCorroborated.as_str(),
            "slps-front-corroborated"
        );
        assert_eq!(
            super::PreimageRoute::SlpsFrontDirectFocused.as_str(),
            "slps-front-direct-focused"
        );
        assert_eq!(
            super::PreimageRoute::SlpsFrontDirectMain.as_str(),
            "slps-front-direct-main"
        );
    }

    #[test]
    fn direct_selection_prefers_focused_then_main() {
        // Joined focused leg wins when present and nonzero.
        assert_eq!(
            super::select_direct_window(Some(7), Some(8)),
            (Some(7), "focused")
        );
        assert_eq!(
            super::select_direct_window(Some(7), None),
            (Some(7), "focused")
        );
        // Unavailable/missing focused leg falls back to a usable main leg.
        assert_eq!(
            super::select_direct_window(None, Some(8)),
            (Some(8), "main")
        );
        // Zero WIDs are missing, never selected (either leg).
        assert_eq!(
            super::select_direct_window(Some(0), Some(8)),
            (Some(8), "main")
        );
        assert_eq!(
            super::select_direct_window(Some(0), Some(0)),
            (None, "none")
        );
        assert_eq!(super::select_direct_window(None, Some(0)), (None, "none"));
        // Unknown on both legs selects nothing.
        assert_eq!(super::select_direct_window(None, None), (None, "none"));
    }

    #[test]
    fn selected_agreement_requires_foreground_and_exact_window() {
        // Full selected agreement: SLPS == workspace == want, exact
        // selected WID, CG owned.
        assert!(super::selected_focus_agrees(
            100,
            7,
            Some(100),
            Some(100),
            Some(7),
            true
        ));
        // SLPS/workspace mismatch refuses even with the right window.
        assert!(!super::selected_focus_agrees(
            100,
            7,
            Some(200),
            Some(100),
            Some(7),
            true
        ));
        assert!(!super::selected_focus_agrees(
            100,
            7,
            Some(100),
            Some(200),
            Some(7),
            true
        ));
        // Unknown foreground legs refuse.
        assert!(!super::selected_focus_agrees(
            100,
            7,
            None,
            Some(100),
            Some(7),
            true
        ));
        assert!(!super::selected_focus_agrees(
            100,
            7,
            Some(100),
            None,
            Some(7),
            true
        ));
        // Non-exact window refuses: selected names another WID, is
        // missing, or the target is zero.
        assert!(!super::selected_focus_agrees(
            100,
            7,
            Some(100),
            Some(100),
            Some(8),
            true
        ));
        assert!(!super::selected_focus_agrees(
            100,
            7,
            Some(100),
            Some(100),
            None,
            true
        ));
        assert!(!super::selected_focus_agrees(
            100,
            0,
            Some(100),
            Some(100),
            Some(0),
            true
        ));
        // Lost CG join refuses.
        assert!(!super::selected_focus_agrees(
            100,
            7,
            Some(100),
            Some(100),
            Some(7),
            false
        ));
        // Reserved want PID refuses before any read.
        assert!(!super::selected_focus_agrees(
            1,
            7,
            Some(1),
            Some(1),
            Some(7),
            true
        ));
    }

    #[test]
    fn frame_validation_rejects_degenerate() {
        let bad = AxFrame {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 10.0,
        };
        assert!(!bad.valid_for_set());
        let good = AxFrame {
            x: 10.0,
            y: 10.0,
            w: 100.0,
            h: 100.0,
        };
        assert!(good.valid_for_set());
    }

    #[test]
    fn space_type_mapping_matches_pinned_yabai_ids() {
        // Pinned yabai `src/space.c`: 0=user, 4=fullscreen, 2=system.
        // Unknown ids stay `Other`, never guessed into a known kind.
        assert_eq!(classify_space_type(0), SlsSpaceType::User);
        assert_eq!(classify_space_type(4), SlsSpaceType::Fullscreen);
        assert_eq!(classify_space_type(2), SlsSpaceType::System);
        assert_eq!(classify_space_type(7), SlsSpaceType::Other(7));
        assert_eq!(classify_space_type(-1), SlsSpaceType::Other(-1));
    }

    #[test]
    fn sls_probe_never_panics() {
        let _ = sls_connection_available();
        let _ = sls_main_connection_id();
        let _ = sls_space_type(1);
        let _ = sls_display_rows();
        let _ = cg_display_uuid(1);
        let _ = ns_separate_spaces();
    }

    #[test]
    fn display_change_flag_coalesces_and_takes() {
        // Pure atomic semantics, all platforms: bursts collapse into one
        // dirty generation; take clears it.
        let flag = DisplayChangeFlag::new();
        assert!(!flag.take_changed());
        flag.mark_changed();
        flag.mark_changed();
        assert!(flag.take_changed());
        assert!(!flag.take_changed());
        // Cloned flags share the generation (controller handoff shape).
        let shared = flag.clone();
        shared.mark_changed();
        assert!(flag.take_changed());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn display_guard_install_requires_main_thread() {
        // Test threads are never the main thread: install must fail closed
        // here without touching notification state. (If this ever ran on the
        // main thread, install would be a harmless read-only observer.)
        if objc2::MainThreadMarker::new().is_none() {
            assert!(install_display_change_guard().is_err());
        }
        // Off-main display reads fail closed too.
        assert!(ns_display_snapshot().is_err());
        assert!(ns_separate_spaces().is_none());
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn stub_fails_before_native_methods() {
        // Off-macOS stub: discovery is empty and element construction fails
        // fast, so owned gates refuse before any native AX path is reached.
        assert!(running_apps().is_empty());
        assert!(bundle_id_for_pid(4242).is_none());
        assert!(AxElement::application(4242).is_none());
        assert!(AxElement::system_wide().is_none());
        assert!(ax_is_process_trusted().is_none());
        assert!(sls_display_spaces().is_err());
    }

    #[test]
    fn post_event_preflight_never_prompts_or_posts() {
        // Read-only preflight: returns the observed state or unknown, never
        // requests access and never posts an event. Calling it twice must
        // agree (no side effects).
        let first = cg_post_event_access_known();
        let second = cg_post_event_access_known();
        assert_eq!(first, second);
    }

    #[test]
    fn session_lock_verdict_needs_all_legs() {
        use ScreenLockRead::{Absent, Locked, Malformed, Unlocked as MarkerUnlocked};
        // Both documented legs true with the marker absent or unlocked: the
        // ordinary unlocked desktop.
        for screen in [Absent, MarkerUnlocked] {
            assert_eq!(
                lock_from_session_legs(Some(true), Some(true), screen),
                SessionLock::Unlocked
            );
        }
        assert!(
            SessionState {
                login_done: Some(true),
                on_console: Some(true),
                screen_lock: Absent,
                lock: SessionLock::Unlocked,
            }
            .unlocked()
        );
        // A locked screen marker is locked even under healthy documented
        // legs (screen-lock proof is the strongest leg).
        for legs in [
            (Some(true), Some(true)),
            (Some(true), None),
            (None, None),
            (Some(false), Some(false)),
        ] {
            assert_eq!(
                lock_from_session_legs(legs.0, legs.1, Locked),
                SessionLock::Locked
            );
        }
        // Any false documented leg: locked, never unlocked.
        for legs in [
            (Some(false), Some(true)),
            (Some(true), Some(false)),
            (Some(false), Some(false)),
            (Some(false), None),
            (None, Some(false)),
        ] {
            assert_eq!(
                lock_from_session_legs(legs.0, legs.1, Absent),
                SessionLock::Locked
            );
        }
        // A malformed marker is unknown even under healthy legs (fail
        // closed, never guessed).
        assert_eq!(
            lock_from_session_legs(Some(true), Some(true), Malformed),
            SessionLock::Unknown
        );
        // Any unreadable documented leg with no locked marker: unknown.
        for legs in [(None, None), (None, Some(true)), (Some(true), None)] {
            for screen in [Absent, MarkerUnlocked] {
                assert_eq!(
                    lock_from_session_legs(legs.0, legs.1, screen),
                    SessionLock::Unknown
                );
            }
        }
        assert!(!SessionState::unknown().unlocked());
        // Absence and malformation stay distinct on the raw leg.
        assert_ne!(Absent, Malformed);
    }

    #[test]
    fn session_state_read_is_repeatable() {
        // Read-only C call with no side effects: two reads agree. Off
        // platform this pins the fail-closed stub (`Unknown`).
        let first = session_state();
        let second = session_state();
        assert_eq!(first, second);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(first.lock, SessionLock::Unknown);
    }
}
