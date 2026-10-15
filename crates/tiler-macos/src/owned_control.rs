#![forbid(unsafe_code)]

//! Cross-command owned test controls for the bounded tile fixture.
//!
//! Disposable-helper fixture controls only (never product settings, never
//! foreign windows):
//! - [`run_owned_focus`]: SLPS focus dispatch to one exact owned window
//!   with selected-oracle agreement readback.
//! - [`run_owned_post`]: ONE semantic tile chord (focus/move/float/grow/
//!   shrink/orientation only — no arbitrary key logging) posted through the native session stream
//!   ([`crate::synth`], truthfully synthetic, never physical), with the
//!   selected oracle naming the exact intended owned target immediately
//!   before EACH edge. Never a Ghostty (or any foreign) event target.
//! - `owned-frame` / `owned-minimize` are thin CLI shells over
//!   [`crate::ax::read_window_owned`] / [`crate::ax::set_minimized_owned`].
//! - `owned-fullscreen` is the fixture-only native-fullscreen shell over
//!   [`crate::ax::set_fullscreen_owned`] (bounded helper enter/exit
//!   preflight; never a shipped product feature).
//!
//! Every command authenticates the spawn record first (pid/nonce, expected
//! helper, live bundle-or-exe plus the CURRENT live window nonce) and
//! refuses protected bundles, reserved PIDs, and wid 0 before touching the
//! desktop. Outputs are metadata-only JSON (IDs, counts, statuses, frames —
//! never titles, content, or paths).

use std::time::Duration;

use serde::Serialize;

use crate::config::ProbeModifier;

/// One semantic chord: catalogue key code plus the Shift (move/shrink)
/// and Alt (grow/shrink) arms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticChord {
    pub key_code: u32,
    pub shift: bool,
    pub alt: bool,
    pub action: &'static str,
}

/// Resolve a semantic chord name to its key code + Shift/Alt arms. Exactly
/// the tile catalogue's letter/arrow focus arms, Shift move arms, Alt grow
/// arms, Alt+Shift shrink arms, the `G` float toggle and the `O`
/// orientation toggle — nothing else (Up/Down arrows are excluded: no
/// user-owned overview restore path exists). Pure; unit-tested.
pub fn resolve_chord(name: &str) -> Option<SemanticChord> {
    let (key_code, shift, alt, action) = match name {
        "focus-h" => (0x04, false, false, "focus"),
        "focus-j" => (0x26, false, false, "focus"),
        "focus-k" => (0x28, false, false, "focus"),
        "focus-l" => (0x25, false, false, "focus"),
        "focus-left" => (0x7B, false, false, "focus"),
        "focus-right" => (0x7C, false, false, "focus"),
        "move-h" => (0x04, true, false, "move"),
        "move-j" => (0x26, true, false, "move"),
        "move-k" => (0x28, true, false, "move"),
        "move-l" => (0x25, true, false, "move"),
        "move-left" => (0x7B, true, false, "move"),
        "move-right" => (0x7C, true, false, "move"),
        "float-g" => (0x05, false, false, "float"),
        "grow-h" => (0x04, false, true, "grow"),
        "grow-j" => (0x26, false, true, "grow"),
        "grow-k" => (0x28, false, true, "grow"),
        "grow-l" => (0x25, false, true, "grow"),
        "grow-left" => (0x7B, false, true, "grow"),
        "grow-right" => (0x7C, false, true, "grow"),
        "shrink-h" => (0x04, true, true, "shrink"),
        "shrink-j" => (0x26, true, true, "shrink"),
        "shrink-k" => (0x28, true, true, "shrink"),
        "shrink-l" => (0x25, true, true, "shrink"),
        "shrink-left" => (0x7B, true, true, "shrink"),
        "shrink-right" => (0x7C, true, true, "shrink"),
        "orientation-o" => (0x1F, false, false, "orientation"),
        _ => return None,
    };
    Some(SemanticChord {
        key_code,
        shift,
        alt,
        action,
    })
}

/// All chord names (for help text and the parser test).
#[must_use]
pub fn chord_names() -> &'static [&'static str] {
    &[
        "focus-h",
        "focus-j",
        "focus-k",
        "focus-l",
        "focus-left",
        "focus-right",
        "move-h",
        "move-j",
        "move-k",
        "move-l",
        "move-left",
        "move-right",
        "float-g",
        "grow-h",
        "grow-j",
        "grow-k",
        "grow-l",
        "grow-left",
        "grow-right",
        "shrink-h",
        "shrink-j",
        "shrink-k",
        "shrink-l",
        "shrink-left",
        "shrink-right",
        "orientation-o",
    ]
}

#[derive(Debug, Clone, Serialize)]
struct PostReport {
    outcome: String,
    pid: u32,
    wid: u32,
    bundle: String,
    chord: String,
    action: String,
    key_code: u32,
    flags: u64,
    down_post_us: u64,
    up_post_us: u64,
    agreement_before: crate::focus::FocusAgreement,
    agreement_edge: crate::focus::FocusAgreement,
    synthetic: bool,
    note: String,
}

#[derive(Debug, Clone, Serialize)]
struct FocusReport {
    outcome: String,
    pid: u32,
    wid: u32,
    bundle: String,
    dispatch: crate::focus::FocusDispatch,
    agreement: crate::focus::FocusAgreement,
    focus_latency_ms: u64,
    synthetic: bool,
    note: String,
}

/// Fixture-only native-fullscreen enter/exit on one exact owned window.
/// Authenticates the spawn record first (pid/nonce, expected helper, live
/// bundle-or-exe plus CURRENT live window nonce) and refuses protected
/// bundles, reserved PIDs, and wid 0 before touching the desktop. The single
/// `AXFullScreen` boolean write runs through
/// [`crate::ax::set_fullscreen_owned`] with preimage + readback proof; an
/// accepted OS status alone never proves it. Exit codes: 2 usage/validation,
/// 1 gate/write/readback failure.
pub fn run_owned_fullscreen(
    pid: u32,
    nonce: &str,
    wid: u32,
    fullscreen: bool,
) -> Result<String, (i32, String)> {
    let err_json = |stage: &str, error: String| -> (i32, String) {
        (
            1,
            serde_json::json!({"stage": stage, "error": error}).to_string(),
        )
    };
    if wid == 0 {
        return Err((2, "owned-fullscreen needs a nonzero --wid".to_owned()));
    }
    let owned = crate::ax::authorize_owned_action(pid, nonce)
        .map_err(|e| err_json("ownership", e.to_string()))?;
    if crate::ownership::is_protected_bundle(&owned.bundle_id) {
        return Err(err_json(
            "ownership",
            "refusing protected bundle (never a terminal)".to_owned(),
        ));
    }
    crate::cg::verify_wid_owned_by_pid(pid, wid).map_err(|e| err_json("cg-join", e.to_string()))?;
    let report = crate::ax::set_fullscreen_owned(&owned, pid, wid, fullscreen)
        .map_err(|e| err_json("owned-fullscreen", e.to_string()))?;
    serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
}

/// Focus one exact owned window via the SLPS route with agreement proof.
/// Exit codes: 2 usage/validation, 1 gate/dispatch/agreement failure.
pub fn run_owned_focus(
    pid: u32,
    nonce: &str,
    wid: u32,
    _modifier: ProbeModifier,
) -> Result<String, (i32, String)> {
    let err_json = |stage: &str, error: String| -> (i32, String) {
        (
            1,
            serde_json::json!({"stage": stage, "error": error}).to_string(),
        )
    };
    if wid == 0 {
        return Err((2, "owned-focus needs a nonzero --wid".to_owned()));
    }
    let owned = crate::ax::authorize_owned_action(pid, nonce)
        .map_err(|e| err_json("ownership", e.to_string()))?;
    if crate::ownership::is_protected_bundle(&owned.bundle_id) {
        return Err(err_json(
            "ownership",
            "refusing protected bundle (never a terminal)".to_owned(),
        ));
    }
    crate::cg::verify_wid_owned_by_pid(pid, wid).map_err(|e| err_json("cg-join", e.to_string()))?;
    let dispatch = crate::focus::dispatch_focus(pid, wid)
        .map_err(|e| err_json("focus-dispatch", e.to_string()))?;
    let (agreement, latency_ms) = crate::focus::await_agreement(pid, wid, Duration::from_secs(5))
        .map_err(|e| err_json("focus-agreement", e.to_string()))?;
    let report = FocusReport {
        outcome: "completed".to_owned(),
        pid,
        wid,
        bundle: owned.bundle_id,
        dispatch,
        agreement,
        focus_latency_ms: latency_ms,
        synthetic: false,
        note: "SLPS dispatch with selected-oracle agreement; no synthetic input".to_owned(),
    };
    serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
}

/// Post ONE semantic chord to the exact owned frontmost window.
/// The selected oracle must name exactly `(pid, wid)` immediately before
/// the down edge AND immediately before the up edge; anything else refuses
/// before the window server is touched. Exit codes: 2 usage/validation, 1
/// gate/post failure with evidence.
pub fn run_owned_post(
    pid: u32,
    nonce: &str,
    chord_name: &str,
    modifier: ProbeModifier,
) -> Result<String, (i32, String)> {
    let err_json = |stage: &str, error: String| -> (i32, String) {
        (
            1,
            serde_json::json!({"stage": stage, "error": error}).to_string(),
        )
    };
    let chord = resolve_chord(chord_name)
        .ok_or_else(|| (2, format!("unknown --chord '{chord_name}' (see help)")))?;
    if !crate::ffi::slps_available() {
        return Err(err_json(
            "slps-unavailable",
            "SLPS focus symbols unavailable; stop".to_owned(),
        ));
    }
    if crate::ffi::ax_is_process_trusted() != Some(true) {
        return Err(err_json(
            "ax-trust",
            "AX trust not observed true; stop".to_owned(),
        ));
    }
    if crate::ffi::secure_input_known() == Some(true) {
        return Err(err_json(
            "secure-input",
            "Secure Input active; refusing post".to_owned(),
        ));
    }
    match crate::ffi::session_state().lock {
        crate::ffi::SessionLock::Unlocked => {}
        crate::ffi::SessionLock::Locked => {
            return Err(err_json(
                "session",
                "session locked; refusing post".to_owned(),
            ));
        }
        crate::ffi::SessionLock::Unknown => {
            return Err(err_json(
                "session",
                "session unknown; refusing post".to_owned(),
            ));
        }
    }
    match crate::ffi::cg_post_event_access_known() {
        Some(true) => {}
        access => {
            return Err(err_json(
                "post-access",
                format!("window server post access not granted ({access:?}); refusing post"),
            ));
        }
    }
    let (mut want_flags, _) = crate::input::cg_flag_bits(modifier);
    if chord.shift {
        want_flags |= crate::native_input::CG_SHIFT_BIT;
    }
    if chord.alt {
        want_flags |= crate::native_input::CG_ALT_BIT;
    }
    // Down edge: fresh owned proof (spawn-record auth plus CG join, no
    // caller-typed WID) plus the boundary's fresh oracle plus Ghostty
    // refusal immediately before the post.
    let target = crate::synth::OwnedPostTarget::from_spawn_record(pid, nonce)
        .map_err(|e| err_json("ownership", e))?;
    let pid = target.pid();
    let wid = target.wid();
    let before = crate::focus::read_agreement(pid, wid)
        .map_err(|e| err_json("oracle-down", e.to_string()))?;
    let down = crate::synth::post_guarded(
        &target,
        chord.key_code as u16,
        want_flags,
        true,
        crate::synth::GuardedPostRoute::Session,
    )
    .map_err(|e| err_json("post-down", e))?;
    std::thread::sleep(Duration::from_millis(80));
    // Up edge: fresh owned proof AND fresh oracle re-taken immediately
    // before the post (per-edge calls preserved).
    let target = crate::synth::OwnedPostTarget::from_spawn_record(pid, nonce)
        .map_err(|e| err_json("ownership", e))?;
    let edge =
        crate::focus::read_agreement(pid, wid).map_err(|e| err_json("oracle-up", e.to_string()))?;
    let up = crate::synth::post_guarded(
        &target,
        chord.key_code as u16,
        want_flags,
        false,
        crate::synth::GuardedPostRoute::Session,
    )
    .map_err(|e| err_json("post-up", e))?;
    // Report bundle: the live owned bundle re-read after the posts (same
    // `live_owned_bundle` convention as the auth path: LaunchServices
    // bundle where present, probe-bundle convention for direct-spawn
    // helpers). Metadata echo only — never an authorization input.
    let bundle =
        crate::ax::live_owned_bundle(pid).map_err(|e| err_json("ownership", e.to_string()))?;
    let report = PostReport {
        outcome: "posted".to_owned(),
        pid,
        wid,
        bundle,
        chord: chord_name.to_owned(),
        action: chord.action.to_owned(),
        key_code: chord.key_code,
        flags: want_flags,
        down_post_us: down.post_latency_us,
        up_post_us: up.post_latency_us,
        agreement_before: before,
        agreement_edge: edge,
        synthetic: true,
        note: "synthetic CG session-stream post (never physical hardware); delivery is proven by the tile effect + helper counters read harness-side, never this dispatch receipt"
            .to_owned(),
    };
    serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_cover_focus_move_float_without_arrows_up_down() {
        assert_eq!(chord_names().len(), 26);
        // Letters on focus/move/grow/shrink arms plus Left/Right on each
        // arm plus G plus O.
        for name in [
            "focus-h",
            "focus-j",
            "focus-k",
            "focus-l",
            "move-h",
            "move-j",
            "move-k",
            "move-l",
            "focus-left",
            "focus-right",
            "move-left",
            "move-right",
            "float-g",
            "grow-h",
            "grow-j",
            "grow-k",
            "grow-l",
            "grow-left",
            "grow-right",
            "shrink-h",
            "shrink-j",
            "shrink-k",
            "shrink-l",
            "shrink-left",
            "shrink-right",
            "orientation-o",
        ] {
            assert!(resolve_chord(name).is_some(), "{name}");
        }
        // Up/Down arrows are excluded (no overview restore path); unknown
        // names refuse.
        for name in [
            "",
            "auto",
            "focus-up",
            "focus-down",
            "move-up",
            "grow-up",
            "shrink-up",
            "h",
            "ctrl-h",
        ] {
            assert!(resolve_chord(name).is_none(), "{name}");
        }
        // Shift/Alt arm mapping: move chords carry shift, focus/float do
        // not; grow chords carry alt without shift, shrink chords carry
        // both; orientation carries neither.
        assert!(!resolve_chord("focus-h").unwrap().shift);
        assert!(!resolve_chord("focus-h").unwrap().alt);
        assert!(resolve_chord("move-h").unwrap().shift);
        assert!(!resolve_chord("move-h").unwrap().alt);
        assert!(!resolve_chord("float-g").unwrap().shift);
        assert!(!resolve_chord("float-g").unwrap().alt);
        assert!(!resolve_chord("grow-h").unwrap().shift);
        assert!(resolve_chord("grow-h").unwrap().alt);
        assert!(resolve_chord("shrink-h").unwrap().shift);
        assert!(resolve_chord("shrink-h").unwrap().alt);
        assert!(!resolve_chord("orientation-o").unwrap().shift);
        assert!(!resolve_chord("orientation-o").unwrap().alt);
        assert_eq!(resolve_chord("focus-h").unwrap().key_code, 0x04);
        assert_eq!(resolve_chord("float-g").unwrap().key_code, 0x05);
        assert_eq!(resolve_chord("orientation-o").unwrap().key_code, 0x1F);
        assert_eq!(resolve_chord("focus-left").unwrap().key_code, 0x7B);
        assert_eq!(resolve_chord("focus-right").unwrap().key_code, 0x7C);
        assert_eq!(resolve_chord("grow-left").unwrap().key_code, 0x7B);
        assert_eq!(resolve_chord("shrink-right").unwrap().key_code, 0x7C);
    }

    #[test]
    fn fullscreen_refuses_wid_zero_before_live_checks() {
        // Pure validation: wid 0 fails closed with usage code 2 and never
        // reaches ownership/CG/AX.
        for want in [true, false] {
            let err =
                super::run_owned_fullscreen(4242, "0123456789abcdef0123456789abcdef", 0, want)
                    .unwrap_err();
            assert_eq!(err.0, 2, "want={want}");
            assert!(err.1.contains("nonzero"), "unexpected: {}", err.1);
        }
    }
}
