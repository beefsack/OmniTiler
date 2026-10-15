#![forbid(unsafe_code)]

//! Synthetic Space-switch edges: arrow chords and the yabai Dock-swipe
//! gesture pair (no SA, no SLS moves, no taps).
//!
//! Every edge rechecks the session gates plus the exact held-helper proof
//! plus the shared [`crate::synth::boundary_gate`] (bundle/protected
//! refusal plus exact selected oracle — identical on the keyboard and
//! gesture paths). A refused second edge after a posted first edge is a
//! pending release ([`PairOutcome::Pending`]): the probe completes that
//! exact pair once via [`post_release_once`], never a re-send of
//! `down`/`began`, never a retry.
//!
//! Prior art (local checkouts first; pinned upstream URLs only):
//! gesture fields + began/ended session posts + same-display scope follow
//! yabai `space_manager_focus_space_using_gesture`
//! (https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/space_manager.c#L927-L1035);
//! the ≥27 coupling follows rift `src/sys/space_switch.rs`
//! (https://github.com/acsandmann/rift/blob/8114ccc534b5c9a7e6a77ce27568d8739be9216f/src/sys/space_switch.rs#L170-L223),
//! whose augmented payload is NOT used here; the effective-table Fn-bit
//! strip follows Hammerspoon `extensions/hotkey/libhotkey.m`
//! (https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/hotkey/libhotkey.m#L208-L241).

use crate::mc_probe::McError;
use crate::symbolic_hotkeys::CG_CTRL_BIT;

/// Yabai pinned gesture fields (same-display branch only).
pub const GESTURE_FIELD_TYPE: u32 = 55;
pub const GESTURE_TYPE_DOCK_CONTROL: i64 = 30;
pub const GESTURE_FIELD_HID_TYPE: u32 = 110;
pub const GESTURE_HID_DOCK_SWIPE: i64 = 23;
pub const GESTURE_FIELD_MOTION: u32 = 123;
pub const GESTURE_MOTION_HORIZONTAL: i64 = 1;
pub const GESTURE_FIELD_PROGRESS: u32 = 124;
pub const GESTURE_FIELD_VELOCITY_X: u32 = 129;
pub const GESTURE_VELOCITY: f64 = 9999.0;
pub const GESTURE_FIELD_PHASE: u32 = 132;
pub const GESTURE_PHASE_BEGAN: i64 = 1;
pub const GESTURE_PHASE_ENDED: i64 = 4;
/// Rift version coupling: the augmented ≥27 payload is never blindly used.
pub const GESTURE_NEW_PAYLOAD_MAJOR: u32 = 27;

/// Pure same-display adjacency over one display's ordered Space list:
/// returns the yabai progress sign (+1.0 T after O, −1.0 T before O) when
/// O and T are exactly adjacent, else `None` (non-adjacent or unmapped —
/// the gesture pair refuses, never a multi-step loop here).
pub fn gesture_sign(spaces_in_order: &[u64], original: u64, temp: u64) -> Option<f64> {
    let oi = spaces_in_order.iter().position(|s| *s == original)?;
    let ti = spaces_in_order.iter().position(|s| *s == temp)?;
    match ti as i64 - oi as i64 {
        1 => Some(1.0),
        -1 => Some(-1.0),
        _ => None,
    }
}

/// Pure version gate: the yabai 6-field branch is the only candidate here.
/// macOS ≥27 refuses (the rift augmented payload needs an exact observed
/// family select; never blindly used). Unknown refuses the same way.
pub fn gesture_payload_allowed(major: Option<u32>) -> Result<(), String> {
    match major {
        Some(m) if m < GESTURE_NEW_PAYLOAD_MAJOR => Ok(()),
        Some(m) => Err(format!(
            "gesture payload refused on macOS major {m}: the rift >=27 augmented payload needs an exact observed family select; the yabai pinned branch is not blindly extended there"
        )),
        None => Err("gesture payload refused: macOS major unreadable".to_owned()),
    }
}

/// Which helper owns a pending release edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slot {
    O,
    T,
}

/// A posted-but-unreleased synthetic edge: `down` (arrow) or `began`
/// (gesture) reached the session stream, but `up`/`ended` was refused.
/// The recovery completes this exact pair once — never a re-send of
/// `down`/`began`, never a retry of the semantic action.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PendingRelease {
    pub(crate) slot: Slot,
    pub(crate) pid: u32,
    pub(crate) wid: u32,
    pub(crate) keycode: u32,
    pub(crate) sign: f64,
    pub(crate) is_gesture: bool,
}

impl PendingRelease {
    /// Report token marking the pending edge: route, edge kind, exact
    /// intended target, and the refusal that left it pending.
    pub(crate) fn token(&self, reason: &str) -> String {
        let (route, edge) = if self.is_gesture {
            ("gesture", "began-without-ended")
        } else {
            ("arrow", "down-without-up")
        };
        format!(
            "pending-release: {route} {edge} pid={} wid={} ({}) refused: {reason}",
            self.pid,
            self.wid,
            match self.slot {
                Slot::O => "O-helper",
                Slot::T => "T-helper",
            },
        )
    }
}

/// Outcome of one down/up (or began/ended) pair: both edges posted, the
/// second edge refused after the first posted (pending release), or the
/// first edge refused (nothing posted).
/// `Paired`/`Pending` are constructed only on macOS (off-mac stubs always
/// refuse), so silence dead_code there only; matching still observes them.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) enum PairOutcome {
    Paired,
    Pending {
        posted: u32,
        pending: PendingRelease,
        error: String,
    },
    Refused(String),
}

/// Pure pending-edge condition: only a refused SECOND edge after a posted
/// first edge leaves a pending release. A refused first edge posts nothing
/// and is never pending.
#[must_use]
pub const fn pending_release_needed(first_posted: bool, second_ok: bool) -> bool {
    first_posted && !second_ok
}

/// Pure release gate: the pending release posts ONLY when every recheck
/// passes — session gates, holder/exe/CG proof, and the exact selected
/// oracle naming the intended target. No exception to the oracle, no blind
/// release.
#[must_use]
pub const fn release_allowed(gates_ok: bool, holder_ok: bool, oracle_exact: bool) -> bool {
    gates_ok && holder_ok && oracle_exact
}

/// Fresh per-EDGE synthetic gates (actual lock + Secure Input + post
/// consent + AX trust): rechecked before EVERY edge (down, up, began,
/// ended — never just once per leg). Any shortfall refuses BEFORE the
/// window server is touched. Never a systemwide AX gate.
pub(crate) fn edge_synth_gates(edge: &str) -> Result<(), McError> {
    let refused = |why: String| McError::Blocked(format!("{edge} refused: {why}"));
    if crate::ffi::ax_is_process_trusted() != Some(true) {
        return Err(refused("AX trust not observed true".to_owned()));
    }
    if crate::ffi::secure_input_known() == Some(true) {
        return Err(refused(
            "Secure Input active (blackout respected)".to_owned(),
        ));
    }
    match crate::ffi::session_state().lock {
        crate::ffi::SessionLock::Unlocked => {}
        crate::ffi::SessionLock::Locked => {
            return Err(refused("window-server session locked".to_owned()));
        }
        crate::ffi::SessionLock::Unknown => {
            return Err(refused("window-server session unknown".to_owned()));
        }
    }
    match crate::ffi::cg_post_event_access_known() {
        Some(true) => Ok(()),
        access => Err(refused(format!(
            "window-server post access not granted ({access:?}) — posts would drop"
        ))),
    }
}

/// One arrow down/up pair through the session stream. Session gates plus
/// the exact held-helper proof plus the shared bundle/oracle boundary run
/// before EACH edge. When `up` is refused after `down` posted, the pair
/// reports [`PairOutcome::Pending`] (complete once via the probe's pending
/// recovery, never retried here).
#[cfg(target_os = "macos")]
pub(crate) fn post_arrow_held(
    child: &mut std::process::Child,
    slot: Slot,
    pid: u32,
    wid: u32,
    helper_path: &str,
    keycode: u32,
) -> PairOutcome {
    use crate::synth::{GuardedPostRoute, OwnedPostTarget, post_guarded};
    let key: u16 = match u16::try_from(keycode) {
        Ok(k) => k,
        Err(_) => {
            return PairOutcome::Refused(format!("arrow keycode {keycode} out of range"));
        }
    };
    if let Err(e) = edge_synth_gates("arrow-down") {
        return PairOutcome::Refused(e.to_string());
    }
    let target = match OwnedPostTarget::from_held_child(child, pid, wid, helper_path) {
        Ok(t) => t,
        Err(e) => return PairOutcome::Refused(format!("arrow down target refused: {e}")),
    };
    if let Err(e) = post_guarded(&target, key, CG_CTRL_BIT, true, GuardedPostRoute::Session) {
        return PairOutcome::Refused(format!("arrow down refused: {e}"));
    }
    // The up edge re-proves everything (gates, holder, boundary): a PID
    // reuse or focus drift between edges still refuses. A refused second
    // edge after the posted first edge is a pending release (combined
    // through the pure condition below, never inferred).
    let second: Result<(), String> = (|| {
        edge_synth_gates("arrow-up").map_err(|e| e.to_string())?;
        let target = OwnedPostTarget::from_held_child(child, pid, wid, helper_path)
            .map_err(|e| format!("arrow up target refused: {e}"))?;
        post_guarded(&target, key, CG_CTRL_BIT, false, GuardedPostRoute::Session)
            .map_err(|e| format!("arrow up refused: {e}"))?;
        Ok(())
    })();
    if pending_release_needed(true, second.is_ok()) {
        return PairOutcome::Pending {
            posted: 1,
            pending: PendingRelease {
                slot,
                pid,
                wid,
                keycode,
                sign: 0.0,
                is_gesture: false,
            },
            error: second.unwrap_err(),
        };
    }
    PairOutcome::Paired
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn post_arrow_held(
    _child: &mut std::process::Child,
    _slot: Slot,
    _pid: u32,
    _wid: u32,
    _helper_path: &str,
    _keycode: u32,
) -> PairOutcome {
    PairOutcome::Refused("arrow posts are macOS-only".to_owned())
}

/// One yabai began/ended gesture event with the exact pinned 6-field
/// branch (same display, no warp/click). Shared by the leg post and the
/// pending-release completion so both emit the identical shape.
#[cfg(target_os = "macos")]
pub(crate) fn gesture_event(
    sign: f64,
    phase: i64,
) -> Option<objc2_core_foundation::CFRetained<objc2_core_graphics::CGEvent>> {
    use objc2_core_graphics::{CGEvent, CGEventField};
    let event = CGEvent::new(None)?;
    CGEvent::set_integer_value_field(
        Some(&event),
        CGEventField(GESTURE_FIELD_TYPE),
        GESTURE_TYPE_DOCK_CONTROL,
    );
    CGEvent::set_integer_value_field(
        Some(&event),
        CGEventField(GESTURE_FIELD_HID_TYPE),
        GESTURE_HID_DOCK_SWIPE,
    );
    CGEvent::set_integer_value_field(
        Some(&event),
        CGEventField(GESTURE_FIELD_MOTION),
        GESTURE_MOTION_HORIZONTAL,
    );
    CGEvent::set_double_value_field(Some(&event), CGEventField(GESTURE_FIELD_PROGRESS), sign);
    CGEvent::set_double_value_field(
        Some(&event),
        CGEventField(GESTURE_FIELD_VELOCITY_X),
        sign * GESTURE_VELOCITY,
    );
    CGEvent::set_integer_value_field(Some(&event), CGEventField(GESTURE_FIELD_PHASE), phase);
    Some(event)
}

/// One same-display yabai Dock-swipe began/ended pair through the session
/// tap: exact 6-field pinned branch, no cursor warp/click. The session
/// gates plus the exact held-helper proof plus the shared bundle/oracle
/// boundary (identical to the keyboard path) run before EACH post
/// including `ended`. When `ended` is refused after `began` posted, the
/// pair reports [`PairOutcome::Pending`] — never a blind `ended`.
#[cfg(target_os = "macos")]
pub(crate) fn post_gesture_held(
    child: &mut std::process::Child,
    slot: Slot,
    pid: u32,
    wid: u32,
    helper_path: &str,
    sign: f64,
) -> PairOutcome {
    use crate::synth::{OwnedPostTarget, boundary_gate};
    use objc2_core_graphics::{CGEvent, CGEventTapLocation};
    fn pending(slot: Slot, pid: u32, wid: u32, sign: f64, error: String) -> PairOutcome {
        PairOutcome::Pending {
            posted: 1,
            pending: PendingRelease {
                slot,
                pid,
                wid,
                keycode: 0,
                sign,
                is_gesture: true,
            },
            error,
        }
    }
    if let Err(e) = edge_synth_gates("gesture-began") {
        return PairOutcome::Refused(e.to_string());
    }
    let target = match OwnedPostTarget::from_held_child(child, pid, wid, helper_path) {
        Ok(t) => t,
        Err(e) => return PairOutcome::Refused(format!("gesture began target refused: {e}")),
    };
    if let Err(e) = boundary_gate(target.pid(), target.wid()) {
        return PairOutcome::Refused(format!("gesture began refused: {e}"));
    }
    let event = match gesture_event(sign, GESTURE_PHASE_BEGAN) {
        Some(e) => e,
        None => return PairOutcome::Refused("gesture event creation returned null".to_owned()),
    };
    CGEvent::post(CGEventTapLocation::SessionEventTap, Some(&event));
    // Re-gate before `ended`: fresh gates, fresh owned proof, fresh shared
    // boundary. A refusal here emits NOTHING further — and the refused
    // second edge after the posted `began` is a pending release.
    let second: Result<(), String> = (|| {
        edge_synth_gates("gesture-ended").map_err(|e| e.to_string())?;
        let target = OwnedPostTarget::from_held_child(child, pid, wid, helper_path)
            .map_err(|e| format!("gesture ended target refused: {e}"))?;
        boundary_gate(target.pid(), target.wid())
            .map_err(|e| format!("gesture ended refused: {e}"))?;
        Ok(())
    })();
    if pending_release_needed(true, second.is_ok()) {
        return pending(slot, pid, wid, sign, second.unwrap_err());
    }
    // Complete the pair with an ended-phase event of the identical shape
    // (same builder as `began`, never a blind post: the gates above passed).
    let event = match gesture_event(sign, GESTURE_PHASE_ENDED) {
        Some(e) => e,
        None => {
            return pending(
                slot,
                pid,
                wid,
                sign,
                "ended event creation returned null".to_owned(),
            );
        }
    };
    CGEvent::post(CGEventTapLocation::SessionEventTap, Some(&event));
    PairOutcome::Paired
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn post_gesture_held(
    _child: &mut std::process::Child,
    _slot: Slot,
    _pid: u32,
    _wid: u32,
    _helper_path: &str,
    _sign: f64,
) -> PairOutcome {
    PairOutcome::Refused("gesture posts are macOS-only".to_owned())
}

/// Post ONLY the pending release edge once (key-up or gesture-ended),
/// after every recheck already passed. Never re-sends `down`/`began`,
/// never retries the semantic action.
pub(crate) fn post_release_once(
    target: &crate::synth::OwnedPostTarget,
    pending: &PendingRelease,
) -> Result<u32, String> {
    if pending.is_gesture {
        #[cfg(target_os = "macos")]
        {
            use objc2_core_graphics::CGEventTapLocation;
            crate::synth::boundary_gate(target.pid(), target.wid())
                .map_err(|e| format!("boundary refuses ended ({e})"))?;
            gesture_event(pending.sign, GESTURE_PHASE_ENDED)
                .map(|event| {
                    objc2_core_graphics::CGEvent::post(
                        CGEventTapLocation::SessionEventTap,
                        Some(&event),
                    );
                })
                .ok_or_else(|| "ended event creation returned null".to_owned())?;
            Ok(1)
        }
        #[cfg(not(target_os = "macos"))]
        {
            Err("gesture release is macOS-only".to_owned())
        }
    } else {
        use crate::synth::{GuardedPostRoute, post_guarded};
        let key = u16::try_from(pending.keycode)
            .map_err(|_| format!("keycode {} out of range", pending.keycode))?;
        post_guarded(target, key, CG_CTRL_BIT, false, GuardedPostRoute::Session)
            .map_err(|e| format!("up post refused ({e})"))?;
        Ok(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbolic_hotkeys::{CG_CTRL_BIT, KEY_LEFT_ARROW, KEY_RIGHT_ARROW};

    #[test]
    fn arrow_keycodes_match_mac_virtual_codes() {
        // Ctrl+arrow legs post CG key codes with the CG Control bit (CG
        // domain — never confused with the Carbon 0x1000 effective-table
        // domain).
        assert_eq!(KEY_RIGHT_ARROW, 0x7C);
        assert_eq!(KEY_LEFT_ARROW, 0x7B);
        assert_eq!(CG_CTRL_BIT, 0x0004_0000);
        assert_eq!(crate::input_policy::KEY_LEFT, KEY_LEFT_ARROW);
        assert_eq!(crate::input_policy::KEY_RIGHT, KEY_RIGHT_ARROW);
    }

    #[test]
    fn gesture_sign_needs_exact_adjacency_in_sls_order() {
        assert_eq!(gesture_sign(&[1, 66], 1, 66), Some(1.0));
        assert_eq!(gesture_sign(&[66, 1], 1, 66), Some(-1.0));
        // Same Space, gaps, or unmapped SIDs refuse (no multi-step loop).
        assert_eq!(gesture_sign(&[1, 66], 1, 1), None);
        assert_eq!(gesture_sign(&[1, 7, 66], 1, 66), None);
        assert_eq!(gesture_sign(&[1], 1, 66), None);
        assert_eq!(gesture_sign(&[], 1, 66), None);
    }

    #[test]
    fn gesture_fields_match_the_pinned_yabai_branch() {
        // Pinned source: space_manager_focus_space_using_gesture sets
        // 55=30, 110=23, 123=1, 124=sign, 129=sign*9999, 132=1 then 4,
        // posted to the session tap per index step (same-display: no warp,
        // no click — those sit behind focus_display only).
        assert_eq!(GESTURE_FIELD_TYPE, 55);
        assert_eq!(GESTURE_TYPE_DOCK_CONTROL, 30);
        assert_eq!(GESTURE_FIELD_HID_TYPE, 110);
        assert_eq!(GESTURE_HID_DOCK_SWIPE, 23);
        assert_eq!(GESTURE_FIELD_MOTION, 123);
        assert_eq!(GESTURE_MOTION_HORIZONTAL, 1);
        assert_eq!(GESTURE_FIELD_PROGRESS, 124);
        assert_eq!(GESTURE_FIELD_VELOCITY_X, 129);
        assert_eq!(GESTURE_VELOCITY, 9999.0);
        assert_eq!(GESTURE_FIELD_PHASE, 132);
        assert_eq!(GESTURE_PHASE_BEGAN, 1);
        assert_eq!(GESTURE_PHASE_ENDED, 4);
    }

    #[test]
    fn version_gate_refuses_newer_payload_without_family_evidence() {
        assert!(gesture_payload_allowed(Some(15)).is_ok());
        assert!(gesture_payload_allowed(Some(26)).is_ok());
        assert!(gesture_payload_allowed(Some(27)).is_err());
        assert!(gesture_payload_allowed(Some(28)).is_err());
        assert!(gesture_payload_allowed(None).is_err());
    }

    #[test]
    fn only_a_refused_second_edge_leaves_a_pending_release() {
        // Blocker 3: per-edge condition — first-edge refusal posts nothing
        // (never pending); only down/began-posted plus up/ended-refused is
        // pending. The live pair paths combine through this predicate.
        assert!(pending_release_needed(true, false));
        assert!(!pending_release_needed(true, true));
        assert!(!pending_release_needed(false, false));
        assert!(!pending_release_needed(false, true));
    }

    #[test]
    fn pending_release_posts_only_with_gates_holder_and_exact_oracle() {
        // Blocker 5: no exception to the oracle — every recheck must pass,
        // otherwise the run parks dirty. No blind release, no Ghostty path.
        assert!(release_allowed(true, true, true));
        assert!(!release_allowed(false, true, true));
        assert!(!release_allowed(true, false, true));
        assert!(!release_allowed(true, true, false));
        assert!(!release_allowed(false, false, false));
    }

    #[test]
    fn pending_token_marks_route_edge_target_and_refusal() {
        // The pending edge is unmistakable in the report: route, edge,
        // exact intended target, and the refusal — never a silent flag.
        let pending = PendingRelease {
            slot: Slot::O,
            pid: 4242,
            wid: 7,
            keycode: KEY_RIGHT_ARROW,
            sign: 0.0,
            is_gesture: false,
        };
        let token = pending.token("arrow up refused: oracle disagrees");
        assert!(token.contains("pending-release"));
        assert!(token.contains("arrow"));
        assert!(token.contains("down-without-up"));
        assert!(token.contains("4242"));
        assert!(token.contains("O-helper"));
        let gesture = PendingRelease {
            slot: Slot::T,
            pid: 4243,
            wid: 9,
            keycode: 0,
            sign: -1.0,
            is_gesture: true,
        };
        let token = gesture.token("gesture ended refused: session locked");
        assert!(token.contains("gesture"));
        assert!(token.contains("began-without-ended"));
        assert!(token.contains("T-helper"));
    }
}
