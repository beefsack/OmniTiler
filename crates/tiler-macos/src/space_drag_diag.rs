#![forbid(unsafe_code)]

//! User-run ordinary-app drag/chord diagnostics (TextEdit dogfooding,
//! not owned-fixture transfer experiments).
//!
//! Three one-shot commands over the transfer-executor-identical HID path
//! (source `None`, tap `HID`, Ctrl+arrow flags plus the own-switch tag —
//! see [`crate::synth::hid_key_post`] / [`crate::synth::hid_mouse_post`],
//! the single shared construction site also used by
//! [`crate::space_drag_live`]):
//! - chord-only: countdown, then exactly one `Ctrl+Right` down/up. No
//!   helper, no ownership, no PID/nonce: the user physically holds the
//!   TextEdit titlebar.
//! - hold-only: countdown while the user positions the real cursor; the
//!   cursor is captured once, guarded, then mouse down plus the same small
//!   executor drag, held (default 6 s, cap 15 s) for a REAL `Ctrl+Right`,
//!   then mouse-up at the identical point.
//! - full: same capture/guards, then the full synthetic drag/chord/release
//!   (normal `Ctrl+Right`, or the Amethyst Desktop-N sequence with
//!   `--amethyst`).
//!
//! Transfer legs keep the owned-helper restriction; these commands do
//! not: they are user-run ordinary-app dogfooding, not owned-fixture
//! automation. Only the captured cursor window may
//! be acted on: positional AX plus WindowServer hits must join ONE
//! standard window, refusing Ghostty (existing case/variant policy), the
//! own process/bundles, unknown/ambiguous, and nonstandard windows.
//!
//! Safety: the narrow session gates run before EVERY synthetic non-release
//! step; release posts unguarded (never blocked by invalidated
//! target/gates). Pending key edges complete exactly once; a held mouse is
//! released first on every exit (explicit paths plus the [`MouseArmed`]
//! RAII guard for unwind plus the polled SIGINT/SIGTERM flag). The hold
//! cap bounds even signal/timeout paths. No SIGKILL/power-loss promise.
//! Reporting/logging failures never strand a release (release precedes the
//! report). One shot each: no inverse, no Space create/destroy, no
//! settings writes, no focus/restore mutations.
//!
//! Amethyst parity (reimplemented behavior, no copied code — Silica is MIT):
//! Amethyst `5cfe2fe` `Amethyst/Model/Window.swift` (`move(toSpaceAtIndex:)`
//! calls `super.move(toSpace:)`) reaches Silica (remote SPM package
//! `https://github.com/ianyh/silica`, branch `master`) `SIWindow.m`
//! `moveToSpaceWithEvent:` plus `SISystemWideElement.m`
//! (inspected at `ebead82`): minimize-button-derived titlebar point
//! (fallback `x + 5`), one point for all mouse events (move, down,
//! dragged), flags `0`, source `NULL`, tap HID, untagged; 50 ms; Desktop-N
//! keydown with the configured symbolic-hotkey flags then keyup flags `0`
//! (back-to-back, both HID); 400 ms; mouse-up. No drag moves during/after
//! the chord; Silica auto-enables a disabled shortcut (THIS task refuses
//! instead and tells the user to configure it manually — never a settings
//! write). Cursor-only is mandatory here, so the anchor stays at the
//! captured cursor rather than relocating to the minimize button
//! ([`ANCHOR_NOTE`]); no auto-switch-back (Amethyst's manager follows when
//! configured — the variant represents the drag sequence only).

use crate::space_drag_transfer::{ANIMATION_HOLD_MS, DragStep, GRAB_SETTLE_MS};

/// Countdown before any capture/post (seconds). `0` skips.
pub const DEFAULT_COUNTDOWN_SECS: u64 = 5;
pub const MAX_COUNTDOWN_SECS: u64 = 30;
/// Drag-hold window for the user's physical chord (seconds).
pub const DEFAULT_HOLD_SECS: u64 = 6;
pub const MAX_HOLD_SECS: u64 = 15;
pub const MIN_HOLD_SECS: u64 = 1;
/// Amethyst variant default target Desktop.
pub const DEFAULT_AMETHYST_DESKTOP: u32 = 2;
/// Silica `eventForSwitchingToSpace:` range (`118 + space - 1`, spaces 1-16).
pub const MAX_AMETHYST_DESKTOP: u32 = 16;

/// Executor-identical chord keycode: `Ctrl+Right` (pinned equal to the
/// transfer arrow legs by construction below).
pub const DIAG_CHORD_KEYCODE: u32 = crate::input_match::SWITCH_KEY_RIGHT;
/// Executor-identical chord flags (CG Control bit).
pub const DIAG_CHORD_FLAGS: u64 = crate::input_match::SWITCH_CTRL_BIT;
/// Executor-identical mouse flags (Silica parity: zero).
pub const DIAG_MOUSE_FLAGS: u64 = 0;
/// Executor-identical mouse/key provenance tag (normal modes).
pub const DIAG_OWN_TAG: i64 = crate::input_match::OWN_SWITCH_USER_DATA;

/// Deliberate anchor substitution (cursor-only is mandatory): reported in
/// every full-mode result.
pub const ANCHOR_NOTE: &str = "cursor-only anchor: all synthetic events post at the captured cursor point, not Silica's minimize-button-derived titlebar point (deliberate substitution; grab validity is proven instead by the AX + WindowServer join at the cursor)";

/// Pure countdown gate: `0..=30` (0 skips). No live touch.
pub fn validate_countdown(secs: u64) -> Result<u64, String> {
    if secs > MAX_COUNTDOWN_SECS {
        return Err(format!(
            "countdown must be 0..={MAX_COUNTDOWN_SECS}, got {secs}"
        ));
    }
    Ok(secs)
}

/// Pure hold gate: `1..=15`, default 6. The cap bounds even
/// signal/timeout paths. No live touch.
pub fn validate_hold(secs: u64) -> Result<u64, String> {
    if !(MIN_HOLD_SECS..=MAX_HOLD_SECS).contains(&secs) {
        return Err(format!(
            "hold must be {MIN_HOLD_SECS}..={MAX_HOLD_SECS}, got {secs}"
        ));
    }
    Ok(secs)
}

/// Pure Desktop-N gate for `--amethyst`: Silica range 1-16. The configured
/// chord is read live afterwards (refused when disabled/unknown). No live
/// touch.
pub fn validate_desktop(desktop: u32) -> Result<u32, String> {
    if !(1..=MAX_AMETHYST_DESKTOP).contains(&desktop) {
        return Err(format!(
            "desktop must be 1..={MAX_AMETHYST_DESKTOP}, got {desktop}"
        ));
    }
    Ok(desktop)
}

/// Pure Amethyst chord resolution over one observed holder: enabled plus a
/// representable keycode, else refuse (telling the user to configure the
/// "Switch to Desktop N" shortcut manually — never a settings write). No
/// live touch; the live runner reads through
/// [`crate::desktop_shortcuts::DesktopShortcutStore`].
pub fn resolve_amethyst_chord(
    desktop: u32,
    entry: crate::desktop_shortcuts::ShortcutEntry,
) -> Result<(u32, u64), String> {
    if !entry.enabled {
        return Err(format!(
            "Switch to Desktop {desktop} is disabled or unbound; enable it manually in System Settings > Keyboard > Keyboard Shortcuts > Mission Control (this diagnostic never writes settings)"
        ));
    }
    if entry.keycode > 255 {
        return Err(format!(
            "Switch to Desktop {desktop} chord keycode {} out of range 0..=255; refusing rather than guessing",
            entry.keycode
        ));
    }
    Ok((entry.keycode, entry.modifiers))
}

/// Plain inputs for the cursor-target verdict (no OS touch here; the live
/// runner performs the reads and passes the outcomes in).
pub struct TargetInputs<'a> {
    pub ax_owner: Option<(u32, u32)>,
    pub sls_wid: Option<u32>,
    /// CG entries exactly equal to `(ax_pid, sls_wid)` that are onscreen
    /// on the normal layer.
    pub cg_exact: usize,
    /// False when the CG snapshot may be truncated (list length at the
    /// cap): completeness unknown, refuse.
    pub cg_complete: bool,
    pub bundle: Option<&'a str>,
    pub subrole: Option<&'a str>,
    /// Native-fullscreen state (`AXFullScreen`/`AXFullscreen` on the
    /// already-resolved window element — one cheap attribute read, no new
    /// touch): `Some(true)` refuses (fullscreen journeys are out of scope);
    /// `None` (unreadable) proceeds with an honestly reported setup note
    /// (ordinary non-fullscreen windows required; no games/fullscreen).
    pub fullscreen: Option<bool>,
    pub own_pid: u32,
}

/// The one window that may be acted on (IDs only, never titles/content).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DiagTarget {
    pub pid: u32,
    pub wid: u32,
    pub bundle: String,
    pub subrole: String,
}

/// Pure cursor-target verdict: AX owner plus WindowServer hit must join
/// exactly one onscreen normal-layer standard non-fullscreen window;
/// Ghostty (any case/variant), the own process/bundles, protected
/// bundles, unknown/ambiguous, nonstandard, and native-fullscreen windows
/// all refuse. An unreadable fullscreen leg proceeds (the live runner
/// reports the honest no-games/no-fullscreen setup requirement instead).
/// No live touch.
pub fn resolve_target(inputs: TargetInputs<'_>) -> Result<DiagTarget, String> {
    let ax = inputs.ax_owner.ok_or_else(|| {
        "target refused: AX hit-test owner unreadable (unknown, never guessed)".to_owned()
    })?;
    let sls = inputs.sls_wid.ok_or_else(|| {
        "target refused: WindowServer hit unreadable (unknown, never guessed)".to_owned()
    })?;
    if ax.0 <= 1 || ax.1 == 0 {
        return Err("target refused: reserved AX hit identity".to_owned());
    }
    if ax.1 != sls {
        return Err(format!(
            "target refused: ambiguous hit (AX owner=({}, {}) but WindowServer wid={sls}; refusing, never the drag target)",
            ax.0, ax.1
        ));
    }
    if !inputs.cg_complete {
        return Err("target refused: CG snapshot completeness unknown".to_owned());
    }
    if inputs.cg_exact != 1 {
        return Err(format!(
            "target refused: CG join is ambiguous ({} exact onscreen normal-layer matches for ({}, {sls}); want exactly 1)",
            inputs.cg_exact, ax.0
        ));
    }
    if ax.0 == inputs.own_pid {
        return Err("target refused: cursor names the diagnostic process itself".to_owned());
    }
    let bundle = inputs.bundle.filter(|b| !b.is_empty()).ok_or_else(|| {
        "target refused: owner bundle unknown (never act on an unidentified window)".to_owned()
    })?;
    if crate::scope::is_ghostty_bundle(bundle) {
        return Err("target refused: Ghostty under cursor (never a target)".to_owned());
    }
    if bundle == crate::BUNDLE_ID || bundle == crate::HELPER_BUNDLE_ID {
        return Err("target refused: own probe/helper bundle under cursor".to_owned());
    }
    if crate::ownership::is_protected_bundle(bundle) {
        return Err(format!(
            "target refused: protected bundle '{bundle}' under cursor"
        ));
    }
    let subrole = inputs.subrole.filter(|s| !s.is_empty()).ok_or_else(|| {
        "target refused: window subrole unreadable (unknown, never guessed)".to_owned()
    })?;
    if subrole != "AXStandardWindow" {
        return Err(format!(
            "target refused: nonstandard window (subrole '{subrole}'; want AXStandardWindow)"
        ));
    }
    if inputs.fullscreen == Some(true) {
        return Err(
            "target refused: native-fullscreen window under cursor (fullscreen journeys are out of scope; use an ordinary windowed TextEdit document)"
                .to_owned(),
        );
    }
    Ok(DiagTarget {
        pid: ax.0,
        wid: sls,
        bundle: bundle.to_owned(),
        subrole: subrole.to_owned(),
    })
}

/// One posted diagnostic event (dispatch receipt with a monotonic
/// timestamp, never a delivery claim).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct DiagEvent {
    pub seq: u32,
    pub kind: &'static str,
    pub keycode: Option<u32>,
    pub point: Option<(f64, f64)>,
    pub flags: u64,
    pub tag: Option<i64>,
    /// Milliseconds since the run start (monotonic).
    pub t_ms: u64,
    /// Microseconds around the post call (dispatch cost, not delivery).
    pub latency_us: u64,
}

/// Diagnostic backend: the executor calls these in transfer order; the
/// fake stands in for the WindowServer in tests, the live backend posts
/// guarded HID events. `sleep_ms` advances a fake clock (tests) or sleeps
/// (live); `interrupted` is the scripted flag (tests) or the SIGINT/SIGTERM
/// latch (live).
pub trait DiagBackend {
    fn now_ms(&mut self) -> u64;
    fn sleep_ms(&mut self, ms: u64);
    fn interrupted(&mut self) -> bool;
    fn post_chord(
        &mut self,
        down: bool,
        keycode: u32,
        flags: u64,
        tag: Option<i64>,
    ) -> Result<u64, String>;
    fn post_mouse(
        &mut self,
        step: DragStep,
        point: (f64, f64),
        flags: u64,
        tag: Option<i64>,
    ) -> Result<u64, String>;
    /// Narrow pre-step safety (live: session gates plus the interrupt
    /// latch; release never consults this).
    fn gates(&mut self, edge: &str) -> Result<(), String>;
}

/// Typed diagnostic outcome. Variants drive reports; the mouse is released
/// before any of these is returned (except pre-post refusals, which never
/// held it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagError {
    /// Refused before effect with nothing held.
    Refused { stage: &'static str, reason: String },
    /// A posted key edge could not be released (mouse already released).
    UnresolvedPending {
        reason: String,
        mouse_released: bool,
    },
    /// The mouse-up release itself failed: the drag may still be held.
    ReleaseFailed { reason: String },
    /// Interrupted (flag) after posts began: mouse released first.
    Interrupted { stage: &'static str },
}

impl DiagError {
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Refused { stage, reason } => format!("refused at {stage}: {reason}"),
            Self::UnresolvedPending {
                reason,
                mouse_released,
            } => format!("{reason}; mouse_released={mouse_released}"),
            Self::ReleaseFailed { reason } => {
                format!("mouse release failed (drag may be held): {reason}")
            }
            Self::Interrupted { stage } => format!("interrupted at {stage} (released first)"),
        }
    }
}

fn refused(stage: &'static str, reason: String) -> DiagError {
    DiagError::Refused { stage, reason }
}

/// Chord-down parameters plus the (possibly different) key-up flags
/// (Amethyst key-up carries flags `0`; normal carries the chord flags).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChordParams {
    pub keycode: u32,
    pub down_flags: u64,
    pub up_flags: u64,
    pub tag: Option<i64>,
}

impl ChordParams {
    /// Normal transfer-identical `Ctrl+Right` chord (tagged unless
    /// `--untagged`).
    #[must_use]
    pub fn ctrl_right(tagged: bool) -> Self {
        Self {
            keycode: DIAG_CHORD_KEYCODE,
            down_flags: DIAG_CHORD_FLAGS,
            up_flags: DIAG_CHORD_FLAGS,
            tag: tagged.then_some(DIAG_OWN_TAG),
        }
    }

    /// Amethyst Desktop-N chord: configured keydown flags, keyup flags
    /// `0`, always untagged (Silica parity).
    #[must_use]
    pub fn amethyst(keycode: u32, flags: u64) -> Self {
        Self {
            keycode,
            down_flags: flags,
            up_flags: 0,
            tag: None,
        }
    }
}

/// ONE chord-only run: gates, exactly one down/up pair with once-only
/// pending-key recovery. No mouse, no target. The down edge arms the key
/// guard, so an ordinary panic before the up edge still attempts the
/// release on unwind. Shared by fake tests and the live runner.
pub fn run_chord_only<B: DiagBackend>(
    backend: &mut B,
    events: &mut Vec<DiagEvent>,
    params: &ChordParams,
    notes: &mut Vec<String>,
) -> Result<bool, DiagError> {
    let mut guard = MouseArmed::new(backend, events, notes, (0.0, 0.0), None);
    guard
        .gate("diag-chord-down")
        .map_err(|e| refused("diag-chord-gates", e))?;
    guard
        .push_chord_down(
            params.keycode,
            params.down_flags,
            params.up_flags,
            params.tag,
        )
        .map_err(|e| refused("diag-chord-down", e))?;
    // The key is now down: the up edge completes exactly once (one retry),
    // never a re-send of down. Unwind between the edges attempts the up
    // edge through the guard first.
    match guard.push_chord_up("chord-up") {
        Ok(()) => {
            guard.disarm_all();
            Ok(false)
        }
        Err(e) => {
            let token = crate::space_drag_transfer::DragPending::KeyDownWithoutUp.token(&e);
            guard.note(format!("diag-chord: {token}; completing once"));
            match guard.push_chord_up("chord-up-recovery") {
                Ok(()) => {
                    guard.note("diag-chord: pending chord-up completed once".to_owned());
                    guard.disarm_all();
                    Ok(true)
                }
                Err(e2) => {
                    let reason = format!("{token}; recovery refused: {e2}");
                    // Once-only means once: disarm so the unwind guard
                    // does not attempt a third up edge, then report.
                    guard.disarm_key();
                    Err(DiagError::UnresolvedPending {
                        reason,
                        mouse_released: true,
                    })
                }
            }
        }
    }
}

/// A posted-but-unreleased key edge: marked at the down-post boundary,
/// cleared by the up post. The recovery reuses exactly these flags/tag
/// (Amethyst key-up carries flags `0`, like the live retry).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingKey {
    keycode: u32,
    up_flags: u64,
    tag: Option<i64>,
}

/// Held synthetic edges with guaranteed release: explicit [`release`] on
/// success paths, best-effort unguarded attempts in [`Drop`] (unwind and
/// forgotten-guard paths — key-up first, then mouse-up, mirroring the live
/// order). Releases never consult gates and never retarget. Drop attempts
/// are panic-contained (a panicking backend cannot abort the unwind).
pub struct MouseArmed<'b, B: DiagBackend + ?Sized> {
    backend: Option<&'b mut B>,
    events: Option<&'b mut Vec<DiagEvent>>,
    notes: Option<&'b mut Vec<String>>,
    point: (f64, f64),
    tag: Option<i64>,
    mouse_held: bool,
    key_pending: Option<PendingKey>,
}

impl<'b, B: DiagBackend + ?Sized> MouseArmed<'b, B> {
    pub fn new(
        backend: &'b mut B,
        events: &'b mut Vec<DiagEvent>,
        notes: &'b mut Vec<String>,
        point: (f64, f64),
        tag: Option<i64>,
    ) -> Self {
        Self {
            backend: Some(backend),
            events: Some(events),
            notes: Some(notes),
            point,
            tag,
            mouse_held: false,
            key_pending: None,
        }
    }

    fn backend(&mut self) -> &mut B {
        self.backend.as_mut().expect("guard holds backend")
    }

    fn events(&mut self) -> &mut Vec<DiagEvent> {
        self.events.as_mut().expect("guard holds events")
    }

    fn notes(&mut self) -> &mut Vec<String> {
        self.notes.as_mut().expect("guard holds notes")
    }

    /// Gates through the held backend (a refusal here posts nothing).
    pub fn gate(&mut self, edge: &str) -> Result<(), String> {
        self.backend().gates(edge)
    }

    pub fn note(&mut self, text: String) {
        self.notes().push(text);
    }

    /// Record that the mouse button is now held (call once the Down post
    /// is known-posted: the pre-step Down below, or [`push_mouse`] with
    /// [`DragStep::MouseDown`], which arms automatically).
    pub fn set_mouse_held(&mut self) {
        self.mouse_held = true;
    }

    /// Disarm every pending edge without posting (success paths after the
    /// matching release already posted).
    pub fn disarm_all(&mut self) {
        self.mouse_held = false;
        self.key_pending = None;
    }

    /// Disarm the pending key WITHOUT posting: the once-only recovery was
    /// already attempted, so the unwind guard must not post a further up
    /// edge. The mouse (if held) still releases through [`release`] or
    /// [`Drop`].
    pub fn disarm_key(&mut self) {
        self.key_pending = None;
    }

    /// One recorded mouse post at the guard point (borrows stay inside, so
    /// the release still fires on error). A successful `MouseDown` arms
    /// the mouse automatically.
    pub fn push_mouse(&mut self, step: DragStep) -> Result<(), String> {
        let point = self.point;
        let tag = self.tag;
        let t_ms = self.backend().now_ms();
        let latency = self
            .backend()
            .post_mouse(step, point, DIAG_MOUSE_FLAGS, tag)?;
        if step == DragStep::MouseDown {
            self.mouse_held = true;
        }
        let kind = match step {
            DragStep::MouseMove => "mouse-move",
            DragStep::MouseDown => "mouse-down",
            DragStep::MouseDragged => "mouse-dragged",
            DragStep::MouseUp => "mouse-up",
            _ => "mouse-unknown",
        };
        let seq = self.events().len() as u32;
        self.events().push(DiagEvent {
            seq,
            kind,
            keycode: None,
            point: Some(point),
            flags: DIAG_MOUSE_FLAGS,
            tag,
            t_ms,
            latency_us: latency,
        });
        Ok(())
    }

    /// One recorded chord down post. Marks the key pending at the post
    /// boundary (immediately after success), so an ordinary panic before
    /// the up edge still attempts the release on unwind with the same
    /// flags/tag.
    pub fn push_chord_down(
        &mut self,
        keycode: u32,
        down_flags: u64,
        up_flags: u64,
        tag: Option<i64>,
    ) -> Result<(), String> {
        let t_ms = self.backend().now_ms();
        let latency = self.backend().post_chord(true, keycode, down_flags, tag)?;
        self.key_pending = Some(PendingKey {
            keycode,
            up_flags,
            tag,
        });
        let seq = self.events().len() as u32;
        self.events().push(DiagEvent {
            seq,
            kind: "chord-down",
            keycode: Some(keycode),
            point: None,
            flags: down_flags,
            tag,
            t_ms,
            latency_us: latency,
        });
        Ok(())
    }

    /// One recorded chord up post against the pending edge. Clears the
    /// pending mark on success; a refusal keeps it (caller retries once,
    /// then [`disarm_key`] before reporting pending).
    pub fn push_chord_up(&mut self, kind: &'static str) -> Result<(), String> {
        let pending = self.key_pending.ok_or_else(|| {
            "refusing chord-up with no pending down edge (never a lone release)".to_owned()
        })?;
        let t_ms = self.backend().now_ms();
        let latency =
            self.backend()
                .post_chord(false, pending.keycode, pending.up_flags, pending.tag)?;
        self.key_pending = None;
        let seq = self.events().len() as u32;
        self.events().push(DiagEvent {
            seq,
            kind,
            keycode: Some(pending.keycode),
            point: None,
            flags: pending.up_flags,
            tag: pending.tag,
            t_ms,
            latency_us: latency,
        });
        Ok(())
    }

    /// Bounded interruptible hold (see [`hold_window`]).
    pub fn hold(&mut self, hold_ms: u64, stage: &'static str) -> Result<u64, DiagError> {
        hold_window(self.backend(), hold_ms, stage)
    }

    fn release_inner(
        backend: &mut B,
        events: &mut Vec<DiagEvent>,
        notes: &mut Vec<String>,
        point: (f64, f64),
        tag: Option<i64>,
        via_drop: bool,
    ) -> Result<(), String> {
        // Unguarded by construction: a refused gate must never block the
        // release, and the point never retargets.
        let t_ms = backend.now_ms();
        match backend.post_mouse(DragStep::MouseUp, point, DIAG_MOUSE_FLAGS, tag) {
            Ok(latency) => {
                let seq = events.len() as u32;
                events.push(DiagEvent {
                    seq,
                    kind: if via_drop {
                        "mouse-up-drop"
                    } else {
                        "mouse-up"
                    },
                    keycode: None,
                    point: Some(point),
                    flags: DIAG_MOUSE_FLAGS,
                    tag,
                    t_ms,
                    latency_us: latency,
                });
                if via_drop {
                    notes.push(
                        "mouse-up posted by the unwind guard (explicit release never ran)"
                            .to_owned(),
                    );
                }
                Ok(())
            }
            Err(e) => {
                notes.push(format!("mouse-up refused: {e}"));
                Err(e)
            }
        }
    }

    /// Explicit mouse release (success paths). Disarms on any attempt; a
    /// failed post is [`DiagError::ReleaseFailed`]. The key must already
    /// be resolved or disarmed (debug-checked).
    pub fn release(&mut self) -> Result<(), DiagError> {
        debug_assert!(
            self.key_pending.is_none(),
            "release with a pending key edge: resolve or disarm_key first"
        );
        if !self.mouse_held {
            return Ok(());
        }
        self.mouse_held = false;
        let (mut backend, mut events, mut notes) =
            (self.backend.take(), self.events.take(), self.notes.take());
        let (backend, events, notes) = (
            backend.as_mut().expect("guard holds backend"),
            events.as_mut().expect("guard holds events"),
            notes.as_mut().expect("guard holds notes"),
        );
        Self::release_inner(backend, events, notes, self.point, self.tag, false).map_err(|e| {
            DiagError::ReleaseFailed {
                reason: format!("mouse-up refused: {e}"),
            }
        })?;
        Ok(())
    }
}

impl<B: DiagBackend + ?Sized> Drop for MouseArmed<'_, B> {
    fn drop(&mut self) {
        let (Some(backend), Some(events), Some(notes)) =
            (self.backend.take(), self.events.take(), self.notes.take())
        else {
            return;
        };
        // Key first (live order), then mouse. Best-effort and
        // panic-contained: each attempt runs under `catch_unwind` so a
        // panicking backend cannot abort the unwind being cleaned up.
        // Neither attempt consults gates.
        if let Some(pending) = self.key_pending.take() {
            let t_ms = backend.now_ms();
            let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                backend.post_chord(false, pending.keycode, pending.up_flags, pending.tag)
            }));
            match attempt {
                Ok(Ok(latency)) => {
                    let seq = events.len() as u32;
                    events.push(DiagEvent {
                        seq,
                        kind: "chord-up-drop",
                        keycode: Some(pending.keycode),
                        point: None,
                        flags: pending.up_flags,
                        tag: pending.tag,
                        t_ms,
                        latency_us: latency,
                    });
                    notes.push(
                        "chord-up posted by the unwind guard (explicit release never ran)"
                            .to_owned(),
                    );
                }
                Ok(Err(e)) => {
                    notes.push(format!("unwind-guard chord-up refused: {e}"));
                }
                Err(_) => {
                    notes.push(
                        "unwind-guard chord-up panicked; swallowed (best effort, no abort)"
                            .to_owned(),
                    );
                }
            }
        }
        if self.mouse_held {
            self.mouse_held = false;
            let point = self.point;
            let tag = self.tag;
            let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                MouseArmed::<B>::release_inner(backend, events, notes, point, tag, true)
            }));
            if attempt.is_err() {
                // `release_inner` only pushes (infallible) around one
                // backend call, so a panic means the backend panicked; the
                // best-effort post already ran or refused inside.
                notes.push(
                    "unwind-guard mouse-up panicked; swallowed (best effort, no abort)".to_owned(),
                );
            }
        }
    }
}

/// Release precedence: a failed release is the honest terminal error —
/// a [`DiagError::ReleaseFailed`] (the drag may still be held) must never
/// be disguised as the [`DiagError`] that merely preceded it.
fn release_takes_precedence(release: Result<(), DiagError>, primary: DiagError) -> DiagError {
    match release {
        Ok(()) => primary,
        Err(r) => r,
    }
}

/// One recorded pre-hold mouse post (before the guard arms): used for the
/// move/down pair that precedes any held state.
fn push_prestep_mouse<B: DiagBackend>(
    backend: &mut B,
    events: &mut Vec<DiagEvent>,
    step: DragStep,
    point: (f64, f64),
    tag: Option<i64>,
) -> Result<(), String> {
    let t_ms = backend.now_ms();
    let latency = backend.post_mouse(step, point, DIAG_MOUSE_FLAGS, tag)?;
    let kind = match step {
        DragStep::MouseMove => "mouse-move",
        DragStep::MouseDown => "mouse-down",
        DragStep::MouseDragged => "mouse-dragged",
        DragStep::MouseUp => "mouse-up",
        _ => "mouse-unknown",
    };
    events.push(DiagEvent {
        seq: events.len() as u32,
        kind,
        keycode: None,
        point: Some(point),
        flags: DIAG_MOUSE_FLAGS,
        tag,
        t_ms,
        latency_us: latency,
    });
    Ok(())
}

/// Bounded interruptible hold: `hold_ms` total in interrupt-polled slices.
/// Returns the held duration. A refused slice sleep is a refusal (nothing
/// posted by the sleep itself).
fn hold_window<B: DiagBackend + ?Sized>(
    backend: &mut B,
    hold_ms: u64,
    stage: &'static str,
) -> Result<u64, DiagError> {
    const SLICE_MS: u64 = 50;
    let start = backend.now_ms();
    loop {
        if backend.interrupted() {
            return Err(DiagError::Interrupted { stage });
        }
        let elapsed = backend.now_ms().saturating_sub(start);
        if elapsed >= hold_ms {
            return Ok(elapsed);
        }
        backend.sleep_ms(SLICE_MS.min(hold_ms.saturating_sub(elapsed).max(1)));
    }
}

/// ONE hold-only run: gates before each non-release step, mouse
/// move/down/dragged at the captured point, bounded hold for the user's
/// physical chord, guaranteed same-point mouse-up. Shared by fake tests
/// and the live runner.
pub fn run_hold_only<B: DiagBackend>(
    backend: &mut B,
    events: &mut Vec<DiagEvent>,
    point: (f64, f64),
    mouse_tag: Option<i64>,
    hold_ms: u64,
    notes: &mut Vec<String>,
) -> Result<u64, DiagError> {
    backend
        .gates("diag-hold-move")
        .map_err(|e| refused("diag-hold-gates", e))?;
    push_prestep_mouse(backend, events, DragStep::MouseMove, point, mouse_tag)
        .map_err(|e| refused("diag-hold-move", e))?;
    backend
        .gates("diag-hold-down")
        .map_err(|e| refused("diag-hold-gates", e))?;
    push_prestep_mouse(backend, events, DragStep::MouseDown, point, mouse_tag)
        .map_err(|e| refused("diag-hold-down", e))?;
    // The mouse is now held: every exit below releases it first.
    let mut guard = MouseArmed::new(backend, events, notes, point, mouse_tag);
    guard.set_mouse_held();
    guard
        .gate("diag-hold-dragged")
        .map_err(|e| refused("diag-hold-gates", e))?;
    guard
        .push_mouse(DragStep::MouseDragged)
        .map_err(|e| refused("diag-hold-dragged", e))?;
    let held_ms = guard.hold(hold_ms, "diag-hold");
    // Release first, then interpret the hold outcome: reporting (or an
    // interrupt/timeout) must never strand the release, and a failed
    // release outranks the hold outcome.
    let release = guard.release();
    match (held_ms, release) {
        (Ok(held), Ok(())) => Ok(held),
        (Err(e), Ok(())) => Err(e),
        (Ok(_), Err(r)) | (Err(_), Err(r)) => Err(r),
    }
}

/// Full synthetic drag/chord/release parameters. Timing reuses the
/// transfer executor constants (no drifting copies): 50 ms grab settle,
/// 400 ms animation hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FullParams {
    pub chord: ChordParams,
    pub mouse_tag: Option<i64>,
}

impl FullParams {
    /// Normal full drag: tagged `Ctrl+Right` chord, tagged mouse.
    #[must_use]
    pub fn ctrl_right(tagged: bool) -> Self {
        let tag = tagged.then_some(DIAG_OWN_TAG);
        Self {
            chord: ChordParams::ctrl_right(tagged),
            mouse_tag: tag,
        }
    }

    /// Amethyst Desktop-N sequence: configured keydown flags, keyup flags
    /// `0`, untagged everything (Silica parity).
    #[must_use]
    pub fn amethyst(keycode: u32, flags: u64) -> Self {
        Self {
            chord: ChordParams::amethyst(keycode, flags),
            mouse_tag: None,
        }
    }

    #[must_use]
    pub const fn grab_settle_ms() -> u64 {
        GRAB_SETTLE_MS
    }

    #[must_use]
    pub const fn hold_ms() -> u64 {
        ANIMATION_HOLD_MS
    }
}

/// ONE full run: gates before each non-release step, mouse
/// move/down/dragged at the captured point, grab settle, chord pair with
/// once-only pending-key recovery, animation hold, guaranteed same-point
/// mouse-up. A failed release outranks whatever preceded it (see
/// [`release_takes_precedence`]). Shared by fake tests and the live runner.
pub fn run_full<B: DiagBackend>(
    backend: &mut B,
    events: &mut Vec<DiagEvent>,
    point: (f64, f64),
    params: &FullParams,
    notes: &mut Vec<String>,
) -> Result<bool, DiagError> {
    backend
        .gates("diag-full-move")
        .map_err(|e| refused("diag-full-gates", e))?;
    push_prestep_mouse(
        backend,
        events,
        DragStep::MouseMove,
        point,
        params.mouse_tag,
    )
    .map_err(|e| refused("diag-full-move", e))?;
    backend
        .gates("diag-full-down")
        .map_err(|e| refused("diag-full-gates", e))?;
    push_prestep_mouse(
        backend,
        events,
        DragStep::MouseDown,
        point,
        params.mouse_tag,
    )
    .map_err(|e| refused("diag-full-down", e))?;
    // The mouse is now held: every exit below releases it first.
    let mut guard = MouseArmed::new(backend, events, notes, point, params.mouse_tag);
    guard.set_mouse_held();
    guard
        .gate("diag-full-dragged")
        .map_err(|e| refused("diag-full-gates", e))?;
    guard
        .push_mouse(DragStep::MouseDragged)
        .map_err(|e| refused("diag-full-dragged", e))?;
    if let Err(e) = guard.hold(FullParams::grab_settle_ms(), "diag-grab-settle") {
        return Err(release_takes_precedence(guard.release(), e));
    }
    guard
        .gate("diag-chord-down")
        .map_err(|e| refused("diag-chord-gates", e))?;
    if let Err(e) = guard.push_chord_down(
        params.chord.keycode,
        params.chord.down_flags,
        params.chord.up_flags,
        params.chord.tag,
    ) {
        return Err(release_takes_precedence(
            guard.release(),
            refused("diag-chord-down", e),
        ));
    }
    // The key is now down: complete the up edge exactly once, then hold,
    // then release. A refused up completes once; a twice-refused up is
    // pending (the mouse still releases). Unwind between the edges
    // attempts the up edge through the guard first.
    let mut pending_recovered = false;
    if let Err(e) = guard.push_chord_up("chord-up") {
        let token = crate::space_drag_transfer::DragPending::KeyDownWithoutUp.token(&e);
        guard.note(format!("diag-chord: {token}; completing once"));
        if let Err(e2) = guard.push_chord_up("chord-up-recovery") {
            let reason = format!("{token}; recovery refused: {e2}");
            // Once-only means once: disarm so the unwind guard does not
            // post a third up edge, then release the mouse (whose failure
            // still outranks the pending key).
            guard.disarm_key();
            return match guard.release() {
                Ok(()) => Err(DiagError::UnresolvedPending {
                    reason,
                    mouse_released: true,
                }),
                Err(r) => Err(r),
            };
        }
        pending_recovered = true;
        guard.note("diag-chord: pending chord-up completed once".to_owned());
    }
    if let Err(e) = guard.hold(FullParams::hold_ms(), "diag-hold-settle") {
        return Err(release_takes_precedence(guard.release(), e));
    }
    guard.release()?;
    Ok(pending_recovered)
}
