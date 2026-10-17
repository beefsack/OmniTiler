#![forbid(unsafe_code)]

//! Live user-run drag-diag runners: countdown, cursor capture, guarded
//! HID posts through the shared construction site, JSON reports.
//!
//! One shot each: no inverse, no Space create/destroy, no settings writes
//! (the Amethyst variant refuses a disabled/unbound Desktop-N shortcut
//! instead of enabling it), no focus/restore/pointer mutations, no
//! caffeinate (every run is bounded under a minute). Only the captured
//! cursor window may be acted on; release posts unguarded at the identical
//! point. Countdown and live instructions always go to stderr (live
//! under the launch wrapper's `--stream`); the final JSON (with per-event
//! timestamps) goes to stdout.
//! Reporting/logging failures never strand a release (release precedes the
//! report). SIGINT/SIGTERM latch the local signal-hook flag; the executor
//! polls it between steps and during hold slices and releases first.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use crate::space_drag_diag::{
    ChordParams, DiagBackend, DiagTarget, FullParams, TargetInputs, run_chord_only, run_full,
    run_hold_only,
};

/// Process-wide diagnostic interrupt latch, set by SIGINT/SIGTERM while a
/// diagnostic run arms it. Plain `static` (no unsafe): delivery is
/// installed through the maintained `signal-hook` crate, which takes an
/// `Arc` (hence the lazily-initialized shared flag).
static DIAG_INTERRUPTED: std::sync::LazyLock<Arc<AtomicBool>> =
    std::sync::LazyLock::new(|| Arc::new(AtomicBool::new(false)));

/// RAII signal arm: registers SIGINT/SIGTERM delivery into the latch on
/// creation, unregisters on drop, on every exit path (including unwind).
/// Registration failure refuses BEFORE any post (the runner maps it to a
/// refused report with zero events).
struct InterruptArmed {
    ids: Option<(signal_hook::SigId, signal_hook::SigId)>,
}

impl InterruptArmed {
    fn arm() -> Result<Self, String> {
        DIAG_INTERRUPTED.store(false, Ordering::SeqCst);
        let sigint =
            signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&DIAG_INTERRUPTED))
                .map_err(|e| {
                    format!("SIGINT handler registration refused ({e}); aborting before any post")
                })?;
        let sigterm = signal_hook::flag::register(
            signal_hook::consts::SIGTERM,
            Arc::clone(&DIAG_INTERRUPTED),
        )
        .map_err(|e| {
            let _ = signal_hook::low_level::unregister(sigint);
            format!("SIGTERM handler registration refused ({e}); aborting before any post")
        })?;
        Ok(Self {
            ids: Some((sigint, sigterm)),
        })
    }
}

impl Drop for InterruptArmed {
    fn drop(&mut self) {
        if let Some((sigint, sigterm)) = self.ids.take() {
            let _ = signal_hook::low_level::unregister(sigint);
            let _ = signal_hook::low_level::unregister(sigterm);
        }
    }
}

/// Portable live backend over the shared HID construction site
/// ([`crate::synth::hid_key_post`] / [`crate::synth::hid_mouse_post`] —
/// source `None`, tap `HID`, identical to the transfer executor). Off-macOS
/// the shared posts refuse, so every command refuses before any effect.
pub(crate) struct LiveDiagBackend {
    start: std::time::Instant,
}

impl LiveDiagBackend {
    fn new() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }
}

impl crate::space_drag_diag::DiagBackend for LiveDiagBackend {
    fn now_ms(&mut self) -> u64 {
        self.start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
    }

    fn sleep_ms(&mut self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }

    fn interrupted(&mut self) -> bool {
        DIAG_INTERRUPTED.load(Ordering::SeqCst)
    }

    fn post_chord(
        &mut self,
        down: bool,
        keycode: u32,
        flags: u64,
        tag: Option<i64>,
    ) -> Result<u64, String> {
        let key = u16::try_from(keycode).map_err(|_| format!("keycode {keycode} out of range"))?;
        crate::synth::hid_key_post(key, flags, down, tag)
    }

    fn post_mouse(
        &mut self,
        step: crate::space_drag_transfer::DragStep,
        point: (f64, f64),
        flags: u64,
        tag: Option<i64>,
    ) -> Result<u64, String> {
        let kind = crate::synth::HidMouseKind::from_drag_step(step)
            .ok_or_else(|| "refusing non-mouse step on the mouse post".to_owned())?;
        crate::synth::hid_mouse_post(kind, point, flags, tag)
    }

    fn gates(&mut self, edge: &str) -> Result<(), String> {
        if DIAG_INTERRUPTED.load(Ordering::SeqCst) {
            return Err(format!("{edge} refused: interrupted"));
        }
        crate::space_switch_edges::edge_synth_gates(edge).map_err(|e| e.to_string())
    }
}

/// The captured cursor window (point plus the joined target).
pub(crate) struct CapturedTarget {
    pub point: (f64, f64),
    pub target: DiagTarget,
}

/// AX trait reads for an arbitrary ordinary-app window (user-run
/// diagnostic only — never the owned-helper path): subrole plus
/// native-fullscreen state, over the same resolved window element.
/// Calling-thread AX; unreadable legs are `Err` (unknown, never guessed).
/// No titles, no content, no paths.
///
/// Messaging-timeout note: the timeout is set on the client-side element
/// proxy created fresh in this function (and dropped on return) — it
/// bounds OUR reads only and never mutates the target app. There is no
/// getter for a previous value in the AX API (none invented here), so
/// nothing exists to restore; the proxy's ephemerality is the boundary.
fn read_window_traits(pid: u32, wid: u32) -> Result<(Option<String>, Option<bool>), String> {
    let pid_i32 =
        i32::try_from(pid).map_err(|_| format!("target refused: pid {pid} out of range"))?;
    let app = crate::ffi::AxElement::application(pid_i32)
        .ok_or_else(|| format!("target refused: no AX element for pid {pid}"))?;
    let timeout = crate::ffi::ax_set_messaging_timeout(&app, 2.0);
    if timeout != 0 {
        return Err(format!(
            "target refused: AX messaging timeout unset ({})",
            crate::ffi::ax_error_name(timeout)
        ));
    }
    let windows = app.windows().map_err(crate::ffi::ax_error_name)?;
    let window = windows
        .iter()
        .find(|w| crate::ffi::ax_window_id(w) == Some(wid))
        .ok_or_else(|| format!("target refused: wid {wid} not joined to pid {pid}"))?;
    let _ = crate::ffi::ax_set_messaging_timeout(window, 2.0);
    let subrole =
        crate::ffi::ax_string_attribute(window, "AXSubrole").map_err(crate::ffi::ax_error_name)?;
    // Cheap fullscreen leg on the same element: `Ok(true)` refuses
    // downstream; `Err` degrades to unknown (the runner then reports the
    // honest no-games/no-fullscreen setup requirement instead of a check).
    let fullscreen = crate::ffi::ax_fullscreen_state(window).ok();
    Ok((subrole, fullscreen))
}

/// Capture the cursor once and resolve the ONE window that may be acted
/// on: positional AX owner plus WindowServer hit plus CG join plus bundle
/// plus standard subrole (see [`crate::space_drag_diag::resolve_target`]).
/// Any refusal names the cause; nothing is posted on this path.
pub(crate) fn capture_cursor_target() -> Result<(CapturedTarget, Vec<String>), String> {
    let mut setup_notes = Vec::new();
    let point = crate::synth::cursor_position()
        .ok_or_else(|| "target refused: cursor position unreadable".to_owned())?;
    if !point.0.is_finite() || !point.1.is_finite() {
        return Err("target refused: cursor position non-finite".to_owned());
    }
    let ax = crate::ffi::ax_hit_test_owner(point.0, point.1)?;
    let sls = crate::ffi::sls_window_at_point(point.0, point.1)?;
    let all = crate::ffi::cg_window_list_basic()?;
    let complete = all.len() < crate::config::MAX_WINDOWS;
    let cg_exact = all
        .iter()
        .filter(|w| w.wid == sls && w.pid == ax.0 && w.onscreen && w.layer == 0)
        .count();
    let pid_i32 =
        i32::try_from(ax.0).map_err(|_| format!("target refused: pid {} out of range", ax.0))?;
    let bundle = crate::ffi::bundle_id_for_pid(pid_i32);
    let (subrole, fullscreen) = read_window_traits(ax.0, sls)?;
    if fullscreen.is_none() {
        setup_notes.push(
            "setup requirement (unchecked: fullscreen state unreadable): ordinary windowed TextEdit only — no games, no native-fullscreen journeys"
                .to_owned(),
        );
    }
    let target = crate::space_drag_diag::resolve_target(TargetInputs {
        ax_owner: Some(ax),
        sls_wid: Some(sls),
        cg_exact,
        cg_complete: complete,
        bundle: bundle.as_deref(),
        subrole: subrole.as_deref(),
        fullscreen,
        own_pid: std::process::id(),
    })?;
    Ok((CapturedTarget { point, target }, setup_notes))
}

/// Bounded countdown with interrupt-polled slices: stderr instructions
/// always print (stdout stays JSON-only). An interrupt refuses with
/// nothing posted.
fn countdown(backend: &mut LiveDiagBackend, secs: u64, instruction: &str) -> Result<(), String> {
    let mut remaining = secs;
    while remaining > 0 {
        if backend.interrupted() {
            return Err("interrupted during countdown (nothing posted)".to_owned());
        }
        eprintln!(
            "drag-diag: {instruction} in {remaining}s ... (Ctrl+C cancels pre-post with nothing posted)"
        );
        for _ in 0..10 {
            backend.sleep_ms(100);
            if backend.interrupted() {
                return Err("interrupted during countdown (nothing posted)".to_owned());
            }
        }
        remaining -= 1;
    }
    Ok(())
}

/// Platform gate: HID posts are macOS-only. Runs BEFORE the countdown so
/// off-platform runs refuse with zero wait and zero posts.
fn platform_check(mode: &str) -> Result<(), (i32, String)> {
    #[cfg(not(target_os = "macos"))]
    {
        return Err((
            1,
            refused_body(
                mode,
                "platform",
                "drag-diag is macOS-only (HID posts unavailable)",
                &[],
            ),
        ));
    }
    #[cfg(target_os = "macos")]
    {
        let _ = mode;
        Ok(())
    }
}

fn refused_body(
    mode: &str,
    stage: &str,
    reason: &str,
    events: &[crate::space_drag_diag::DiagEvent],
) -> String {
    serde_json::json!({
        "mode": mode,
        "outcome": format!("{mode}-refused"),
        "stage": stage,
        "error": reason,
        "events": events,
        "posts_total": events.len(),
        "synthetic_events_posted": !events.is_empty(),
        "mutates": true,
    })
    .to_string()
}

/// Residue event summary shared by every failure body: the partial posted
/// log (possibly empty), its length, and whether anything was posted.
/// Never claims complete or released.
fn residue_events(events: &[crate::space_drag_diag::DiagEvent]) -> serde_json::Value {
    serde_json::json!({
        "events": events,
        "posts_total": events.len(),
        "synthetic_events_posted": !events.is_empty(),
    })
}

/// Chord-only: countdown, then exactly one `Ctrl+Right` down/up through
/// the shared HID path. No target, no helper, no ownership: the user
/// physically holds the TextEdit titlebar.
pub fn run_drag_diag_chord(
    countdown_secs: u64,
    untagged: bool,
    allow_untested: bool,
) -> Result<String, (i32, String)> {
    const MODE: &str = "drag-diag-chord";
    if !allow_untested {
        return Err((
            2,
            format!("{MODE} requires --allow-untested (posts one synthetic Ctrl+Right pair)"),
        ));
    }
    let countdown_secs =
        crate::space_drag_diag::validate_countdown(countdown_secs).map_err(|e| (2, e))?;
    platform_check(MODE)?;
    let _armed =
        InterruptArmed::arm().map_err(|e| (1, refused_body(MODE, "signal-arm", &e, &[])))?;
    let mut backend = LiveDiagBackend::new();
    let mut notes = Vec::new();
    if let Err(e) = countdown(
        &mut backend,
        countdown_secs,
        "with your own hand HOLD the TextEdit titlebar NOW; one synthetic Ctrl+Right down/up posts",
    ) {
        return Err((1, refused_body(MODE, "countdown", &e, &[])));
    }
    let params = ChordParams::ctrl_right(!untagged);
    notes.push(format!(
        "chord through the shared HID construction site (source None, tap HID, keycode {:#x}, down flags {:#x}, up flags {:#x}, tag {})",
        params.keycode,
        params.down_flags,
        params.up_flags,
        params.tag.map(|t| format!("{t:#x}")).unwrap_or_else(|| "untagged".to_owned()),
    ));
    let mut events = Vec::new();
    match run_chord_only(&mut backend, &mut events, &params, &mut notes) {
        Ok(pending_recovered) => Ok(serde_json::json!({
            "mode": MODE,
            "outcome": format!("{MODE}-complete"),
            "keycode": params.keycode,
            "down_flags": params.down_flags,
            "up_flags": params.up_flags,
            "tag": params.tag,
            "untagged": untagged,
            "events": events,
            "posts_total": events_len(&events),
            "pending_recovered": pending_recovered,
            "synthetic_events_posted": true,
            "live_evidence": "exactly one Ctrl+Right down/up posted through the transfer-identical HID source/tap/flags/tag",
            "notes": notes,
            "mutates": true,
        })
        .to_string()),
        Err(e) => {
            let mut body = residue_events(&events);
            body["mode"] = serde_json::json!(MODE);
            body["outcome"] = serde_json::json!(format!("{MODE}-residue"));
            body["error"] = serde_json::json!(e.describe());
            body["pending_recovered"] = serde_json::json!(false);
            body["notes"] = serde_json::json!(notes);
            body["mutates"] = serde_json::json!(true);
            Err((1, body.to_string()))
        }
    }
}

/// Hold-only: countdown while the user positions the real cursor, capture
/// once, guard, mouse down plus the small executor drag, bounded hold for
/// the user's REAL `Ctrl+Right`, guaranteed same-point mouse-up.
pub fn run_drag_diag_hold(
    countdown_secs: u64,
    hold_secs: u64,
    allow_untested: bool,
) -> Result<String, (i32, String)> {
    const MODE: &str = "drag-diag-hold";
    if !allow_untested {
        return Err((
            2,
            format!("{MODE} requires --allow-untested (posts synthetic mouse down/dragged/up)"),
        ));
    }
    let countdown_secs =
        crate::space_drag_diag::validate_countdown(countdown_secs).map_err(|e| (2, e))?;
    let hold_secs = crate::space_drag_diag::validate_hold(hold_secs).map_err(|e| (2, e))?;
    let hold_ms = hold_secs.saturating_mul(1_000);
    platform_check(MODE)?;
    let _armed =
        InterruptArmed::arm().map_err(|e| (1, refused_body(MODE, "signal-arm", &e, &[])))?;
    let mut backend = LiveDiagBackend::new();
    let mut notes = Vec::new();
    if let Err(e) = countdown(
        &mut backend,
        countdown_secs,
        "move YOUR cursor onto the TextEdit titlebar NOW; capture follows, then mouse down plus a small drag",
    ) {
        return Err((1, refused_body(MODE, "countdown", &e, &[])));
    }
    let (captured, setup_notes) =
        capture_cursor_target().map_err(|e| (1, refused_body(MODE, "capture", &e, &[])))?;
    notes.extend(setup_notes);
    eprintln!(
        "drag-diag: captured ({}, {}) {} at ({:.0}, {:.0}); mouse down; press YOUR physical Ctrl+Right during the {}s hold (Ctrl+C releases first, then reports)",
        captured.target.pid,
        captured.target.wid,
        captured.target.bundle,
        captured.point.0,
        captured.point.1,
        hold_secs,
    );
    notes.push(format!(
        "captured cursor ({:.0}, {:.0}) joining ({}, {}) {} subrole {}; mouse posts at this point only (source None, tap HID, flags 0, tag {:#x})",
        captured.point.0,
        captured.point.1,
        captured.target.pid,
        captured.target.wid,
        captured.target.bundle,
        captured.target.subrole,
        crate::space_drag_diag::DIAG_OWN_TAG,
    ));
    let mouse_tag = Some(crate::space_drag_diag::DIAG_OWN_TAG);
    let mut events = Vec::new();
    match run_hold_only(
        &mut backend,
        &mut events,
        captured.point,
        mouse_tag,
        hold_ms,
        &mut notes,
    ) {
        Ok(held_ms) => Ok(serde_json::json!({
            "mode": MODE,
            "outcome": format!("{MODE}-complete"),
            "target": captured.target,
            "point": captured.point,
            "hold_secs": hold_secs,
            "held_ms": held_ms,
            "events": events,
            "posts_total": events_len(&events),
            "synthetic_events_posted": true,
            "live_evidence": "synthetic mouse down held across the window for a physical chord, released at the identical point",
            "notes": notes,
            "mutates": true,
        })
        .to_string()),
        Err(e) => {
            let mut body = residue_events(&events);
            body["mode"] = serde_json::json!(MODE);
            body["outcome"] = serde_json::json!(format!("{MODE}-residue"));
            body["target"] = serde_json::json!(captured.target);
            body["point"] = serde_json::json!(captured.point);
            body["hold_secs"] = serde_json::json!(hold_secs);
            body["error"] = serde_json::json!(e.describe());
            body["notes"] = serde_json::json!(notes);
            body["mutates"] = serde_json::json!(true);
            Err((1, body.to_string()))
        }
    }
}

fn events_len(events: &[crate::space_drag_diag::DiagEvent]) -> u32 {
    events.len().min(u32::MAX as usize) as u32
}

/// Full synthetic drag/chord/release at the captured cursor point:
/// normal tagged `Ctrl+Right`, or the Amethyst Desktop-N sequence with
/// `--amethyst` (configured chord read live, refused when
/// disabled/unknown — never a settings write; untagged; keyup flags `0`;
/// anchor stays at the cursor per [`crate::space_drag_diag::ANCHOR_NOTE`]).
pub fn run_drag_diag_full(
    countdown_secs: u64,
    amethyst: bool,
    desktop: u32,
    untagged: bool,
    allow_untested: bool,
) -> Result<String, (i32, String)> {
    const MODE: &str = "drag-diag-full";
    if !allow_untested {
        return Err((
            2,
            format!("{MODE} requires --allow-untested (posts a synthetic drag plus chord)"),
        ));
    }
    let countdown_secs =
        crate::space_drag_diag::validate_countdown(countdown_secs).map_err(|e| (2, e))?;
    if amethyst {
        crate::space_drag_diag::validate_desktop(desktop).map_err(|e| (2, e))?;
    }
    platform_check(MODE)?;
    let _armed =
        InterruptArmed::arm().map_err(|e| (1, refused_body(MODE, "signal-arm", &e, &[])))?;
    let mut backend = LiveDiagBackend::new();
    let mut notes = Vec::new();
    if let Err(e) = countdown(
        &mut backend,
        countdown_secs,
        "move YOUR cursor onto the TextEdit titlebar NOW; capture follows, then the full synthetic drag plus chord plus release",
    ) {
        return Err((1, refused_body(MODE, "countdown", &e, &[])));
    }
    let (captured, setup_notes) =
        capture_cursor_target().map_err(|e| (1, refused_body(MODE, "capture", &e, &[])))?;
    notes.extend(setup_notes);
    let (params, amethyst_detail) = if amethyst {
        use crate::desktop_shortcuts::DesktopShortcutStore as _;
        let symbolic = crate::desktop_shortcuts::desktop_symbolic_id(desktop).ok_or_else(|| {
            (
                2,
                format!(
                    "desktop {desktop} has no symbolic holder id; refusing rather than guessing"
                ),
            )
        })?;
        let mut store = crate::desktop_shortcuts::LiveStore::new();
        let empty: Vec<crate::space_drag_diag::DiagEvent> = Vec::new();
        let entry = store
            .read(symbolic)
            .map_err(|e| (1, refused_body(MODE, "amethyst-read", &e, &empty)))?;
        let (keycode, flags) = crate::space_drag_diag::resolve_amethyst_chord(desktop, entry)
            .map_err(|e| (1, refused_body(MODE, "amethyst-chord", &e, &empty)))?;
        notes.push(format!(
            "amethyst Desktop-{desktop} holder id {symbolic}: enabled chord keycode {keycode:#x} flags {flags:#x}; keyup posts flags 0, all events untagged (Silica parity, reimplemented — never copied)"
        ));
        notes.push(crate::space_drag_diag::ANCHOR_NOTE.to_owned());
        (
            FullParams::amethyst(keycode, flags),
            Some(serde_json::json!({
                "desktop": desktop,
                "symbolic_id": symbolic,
                "keycode": keycode,
                "down_flags": flags,
                "up_flags": 0,
                "enabled": true,
            })),
        )
    } else {
        let params = FullParams::ctrl_right(!untagged);
        notes.push(format!(
            "normal Ctrl+Right drag through the shared HID construction site (source None, tap HID, mouse flags 0, chord keycode {:#x}, tag {})",
            params.chord.keycode,
            params.chord.tag.map(|t| format!("{t:#x}")).unwrap_or_else(|| "untagged".to_owned()),
        ));
        notes.push(crate::space_drag_diag::ANCHOR_NOTE.to_owned());
        (params, None)
    };
    eprintln!(
        "drag-diag: captured ({}, {}) {} at ({:.0}, {:.0}); synthetic drag plus {} chord; release follows (Ctrl+C releases first, then reports)",
        captured.target.pid,
        captured.target.wid,
        captured.target.bundle,
        captured.point.0,
        captured.point.1,
        if amethyst {
            format!("Desktop-{desktop}")
        } else {
            "Ctrl+Right".to_owned()
        },
    );
    let mut events = Vec::new();
    match run_full(&mut backend, &mut events, captured.point, &params, &mut notes) {
        Ok(pending_recovered) => Ok(serde_json::json!({
            "mode": MODE,
            "outcome": format!("{MODE}-complete"),
            "target": captured.target,
            "point": captured.point,
            "amethyst": amethyst_detail,
            "untagged": untagged,
            "events": events,
            "posts_total": events_len(&events),
            "pending_recovered": pending_recovered,
            "synthetic_events_posted": true,
            "live_evidence": "full synthetic drag plus chord plus release at the captured cursor point through the transfer-identical HID source/tap/flags/tag",
            "notes": notes,
            "mutates": true,
        })
        .to_string()),
        Err(e) => {
            let mut body = residue_events(&events);
            body["mode"] = serde_json::json!(MODE);
            body["outcome"] = serde_json::json!(format!("{MODE}-residue"));
            body["target"] = serde_json::json!(captured.target);
            body["point"] = serde_json::json!(captured.point);
            body["amethyst"] = serde_json::json!(amethyst_detail);
            body["error"] = serde_json::json!(e.describe());
            body["notes"] = serde_json::json!(notes);
            body["mutates"] = serde_json::json!(true);
            Err((1, body.to_string()))
        }
    }
}
