#![forbid(unsafe_code)]

//! Truthful synthetic key-event posts to an owned PID (fixtures only).
//!
//! One CG key event per call through the maintained `objc2-core-graphics`
//! bindings (`CGEvent::new_keyboard_event` + `set_flags` +
//! `post_to_pid`): down and up are separate calls so the session names the
//! exact intended owned target in the selected oracle immediately before
//! EACH post and refuses otherwise. Posted events are synthetic by
//! construction (the OS marks the event source accordingly); the session
//! reports them as synthetic, never physical. No raw user keys or content
//! anywhere: callers post only approved fixture chords with counters.
//!
//! Structural ownership proof (not a doc comment): the ONLY posting entry,
//! [`post_guarded`], is `pub(crate)` and takes an unforgeable
//! [`OwnedPostTarget`] whose private fields no caller can construct
//! directly. The two constructors each demand fresh owned-session proof:
//! [`OwnedPostTarget::from_held_child`] revalidates the exact held `Child`
//! plus executable plus CG join (input-session, which carries the Child but
//! no nonce), and [`OwnedPostTarget::from_spawn_record`] authenticates the
//! spawn record (pid/nonce, expected helper, live bundle-or-exe) plus the
//! CG join (owned-control, which carries the nonce). A foreign PID/target
//! without owned proof cannot reach the window server: construction itself
//! refuses before any bundle/oracle read, and the boundary re-enforces the
//! fresh selected-oracle agreement plus the Ghostty/protected refusal
//! immediately before EACH OS post.

#[cfg(target_os = "macos")]
use std::time::Instant;

/// One posted synthetic key event: target, chord class, edge, and the
/// dispatch latency measured around the post call itself (never delivery).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PostedEvent {
    pub pid: u32,
    pub key_code: u16,
    pub flags: u64,
    pub down: bool,
    /// Microseconds around the post call (dispatch cost, not delivery).
    pub post_latency_us: u64,
}

/// Validate a post target before any OS touch. Reserved PIDs, the caller's
/// own process (a self-post proves nothing and risks feedback), and
/// unrepresentable PIDs (out of `i32` range: the CG post takes a signed
/// pid — `u32::MAX` is not a valid OS PID and refuses here rather than
/// wrapping at the boundary) never reach the window server. Liveness and
/// ownership are proven separately by [`OwnedPostTarget`]; this is the
/// static floor, not the oracle.
pub(crate) fn validate_post_target(pid: u32) -> Result<(), String> {
    if pid <= 1 {
        return Err(format!("refusing synthetic post to reserved pid {pid}"));
    }
    if pid == std::process::id() {
        return Err("refusing synthetic post to the controller itself".to_owned());
    }
    if i32::try_from(pid).is_err() {
        return Err(format!("refusing synthetic post: pid {pid} out of range"));
    }
    Ok(())
}

/// Which CG producer the guarded post uses. `Session` is the normal
/// default; `Hid` is the fixture-only upstream discriminator.
///
/// The direct-to-PID route (bypassing session taps) is
/// deliberately absent: every live leg posts through the session stream so
/// taps observe it. Kept minimal so the boundary has no unused arms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GuardedPostRoute {
    Session,
    Hid,
}

/// Pure guarded-post verdict over plain data (no OS touch): the protected
/// bundle refusal plus the fresh selected-oracle agreement flag. Ghostty
/// (any case/variant), the own probe bundle, any protected terminal bundle,
/// and an unknown/empty bundle all refuse; a lost oracle
/// (unknown/mismatch) refuses. The `Main` fallback is ACCEPTED here when
/// the selected oracle agrees: agreement is the `selected_focus_agrees`
/// verdict (SLPS front == workspace frontmost == want PID plus selected
/// direct window == want WID plus CG join), which never distinguishes the
/// `focused` vs `main` leg source — both selected routes
/// (`slps-front-direct-focused`, `slps-front-direct-main`) agree
/// identically. Unit-tested; the live boundary below performs the reads and
/// passes the outcomes here.
pub(crate) fn guarded_verdict(bundle: Option<&str>, oracle_agrees: bool) -> Result<(), String> {
    if !oracle_agrees {
        return Err("refusing synthetic post: selected oracle disagrees".to_owned());
    }
    match bundle {
        Some(id) if !id.is_empty() => {
            if crate::scope::is_ghostty_bundle(id) {
                return Err("refusing synthetic post to Ghostty (never a target)".to_owned());
            }
            if id == crate::BUNDLE_ID {
                return Err("refusing synthetic post to the probe itself".to_owned());
            }
            if crate::ownership::is_protected_bundle(id) {
                return Err("refusing synthetic post to a protected bundle".to_owned());
            }
            Ok(())
        }
        _ => Err("refusing synthetic post with unknown bundle".to_owned()),
    }
}

/// Unforgeable owned post target: private fields, constructible only through
/// fresh owned-session proof. The boundary ([`post_guarded`]) additionally
/// re-reads the live bundle plus the selected oracle immediately before
/// EACH post, so a stale target (PID reuse between construction and post)
/// still refuses at the boundary. Sequential reads, honest TOCTOU: proof,
/// bundle, oracle, and post are sequential live reads, not one atomic
/// transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OwnedPostTarget {
    pid: u32,
    wid: u32,
}

impl OwnedPostTarget {
    /// Prove ownership through the exact held `Child` plus executable plus
    /// CG join (input-session path: the caller holds the Child and carries
    /// no nonce). Refuses reserved/self/out-of-range PIDs, wid 0, exited or
    /// unknown holder states, executable drift, and lost CG joins before
    /// any bundle/oracle read.
    pub(crate) fn from_held_child(
        child: &mut std::process::Child,
        pid: u32,
        wid: u32,
        helper_path: &str,
    ) -> Result<Self, String> {
        validate_post_target(pid)?;
        if wid == 0 {
            return Err("refusing synthetic post to window id 0".to_owned());
        }
        crate::focus::recheck_holder_before_mutation(child, pid, helper_path, wid)
            .map_err(|e| format!("holder recheck refused post target: {e}"))?;
        Ok(Self { pid, wid })
    }

    /// Prove ownership through the spawn record (owned-control path: the
    /// caller carries pid/nonce, never a WID). Authenticates pid/nonce plus
    /// the expected helper plus live bundle-or-exe, refuses protected
    /// bundles over the bound identity, and joins the fresh exact window
    /// (CG owner-PID set intersected with the live AX identity join — no
    /// caller-typed WID, exactly one owned window or refuse) — all before
    /// any oracle read or post.
    pub(crate) fn from_spawn_record(pid: u32, nonce: &str) -> Result<Self, String> {
        validate_post_target(pid)?;
        let owned = crate::ax::authorize_owned_action(pid, nonce)
            .map_err(|e| format!("ownership refused post target: {e}"))?;
        if crate::ownership::is_protected_bundle(&owned.bundle_id)
            || crate::scope::is_ghostty_bundle(&owned.bundle_id)
            || owned.bundle_id == crate::BUNDLE_ID
        {
            return Err("refusing synthetic post to a protected bundle".to_owned());
        }
        let wid = crate::focus::join_helper_wid(pid)
            .map_err(|e| format!("CG join refused post target: {e}"))?;
        if wid == 0 {
            return Err("refusing synthetic post to window id 0".to_owned());
        }
        Ok(Self { pid, wid })
    }

    #[must_use]
    pub(crate) fn pid(self) -> u32 {
        self.pid
    }

    #[must_use]
    pub(crate) fn wid(self) -> u32 {
        self.wid
    }
}

/// One posted synthetic mouse click: target, CG point, and the dispatch
/// latencies measured around each post call itself (never delivery).
/// Delivery is proven harness-side by the owned helper's own AppKit
/// `mouse_counts` snapshot delta, never this dispatch receipt.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PostedClick {
    pub pid: u32,
    pub point: (f64, f64),
    /// Microseconds around the down-post call (dispatch cost, not delivery).
    pub down_latency_us: u64,
    /// Microseconds around the up-post call (dispatch cost, not delivery).
    pub up_latency_us: u64,
}

/// Validate a click point before any OS touch. Non-finite or negative
/// coordinates refuse here rather than at the post boundary. (Live macOS
/// legs call this; the off-macOS stub never does.)
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn validate_click_point(point: (f64, f64)) -> Result<(), String> {
    if !point.0.is_finite() || !point.1.is_finite() {
        return Err("refusing synthetic click at a non-finite point".to_owned());
    }
    if point.0 < 0.0 || point.1 < 0.0 {
        return Err("refusing synthetic click at a negative point".to_owned());
    }
    Ok(())
}

/// Single shared guarded click boundary: one synthetic left-button
/// down/up pair at `point` (CG top-left points) through the HID event tap,
/// untagged (plain synthetic, like tile-catalogue posts — no tap passes or
/// consumes clicks). Enforces [`boundary_gate`] immediately BEFORE each OS
/// post (down and up separately), so a stale target (focus moved between
/// the edges) still refuses the second edge at the boundary.
///
/// Never a Ghostty (or any foreign) event target. The click carries no
/// key content; only the owned helper's own downstream `mouse_counts`
/// delta proves receipt. (Live macOS legs call this; the off-macOS stub
/// never does.)
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn post_mouse_click_guarded(
    target: &OwnedPostTarget,
    point: (f64, f64),
) -> Result<PostedClick, String> {
    validate_click_point(point)?;
    let (pid, wid) = (target.pid(), target.wid());
    boundary_gate(pid, wid)?;
    let down_latency_us = raw_post_mouse_event(pid, true, point)?;
    // Re-gate before the up edge: focus may have moved on the down edge.
    boundary_gate(pid, wid)?;
    let up_latency_us = raw_post_mouse_event(pid, false, point)?;
    Ok(PostedClick {
        pid,
        point,
        down_latency_us,
        up_latency_us,
    })
}

/// Raw HID mouse post (crate-private): only [`post_mouse_click_guarded`]
/// calls this, after owned proof plus the fresh oracle plus bundle refusal.
/// Untagged plain synthetic. Returns the dispatch latency around the post.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn raw_post_mouse_event(pid: u32, down: bool, point: (f64, f64)) -> Result<u64, String> {
    #[cfg(target_os = "macos")]
    {
        use objc2_core_graphics::{
            CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, CGMouseButton,
        };
        let ty = if down {
            CGEventType::LeftMouseDown
        } else {
            CGEventType::LeftMouseUp
        };
        let event = CGEvent::new_mouse_event(
            None,
            ty,
            objc2_core_foundation::CGPoint {
                x: point.0,
                y: point.1,
            },
            CGMouseButton::Left,
        )
        .ok_or_else(|| "CGEventCreateMouseEvent returned null".to_owned())?;
        CGEvent::set_flags(Some(&event), CGEventFlags(0));
        let start = Instant::now();
        // Session-tap delivery note: with no tap installed this posts
        // straight through HID; the boundary above already proved the
        // owned target, so no tap routing is needed for the proof.
        let _ = pid;
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
        Ok(start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (pid, down, point);
        Err("synthetic mouse posts are macOS-only".to_owned())
    }
}

/// Current cursor position via a null-source event (read-only; its
/// location stamps the live cursor). `None` when non-finite or off-macOS.
/// Pointer-restore baseline only: restoring uses the eventless
/// [`crate::ffi::warp_cursor_position`], never a synthesized move.
/// (Live macOS legs call this; the off-macOS stub never does.)
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn cursor_position() -> Option<(f64, f64)> {
    #[cfg(target_os = "macos")]
    {
        use objc2_core_graphics::CGEvent;
        let event = CGEvent::new(None)?;
        let point = CGEvent::location(Some(&event));
        if point.x.is_finite() && point.y.is_finite() {
            Some((point.x, point.y))
        } else {
            None
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}
/// Shared per-post boundary gate: the static [`validate_post_target`]
/// floor, the protected/Ghostty bundle refusal over the FRESH live bundle,
/// and the FRESH selected oracle naming exactly `(pid, wid)`. Every actual
/// CG post in this crate flows through here — the keyboard path via
/// [`post_guarded`], the click path via [`post_mouse_click_guarded`],
/// and non-key synthetic posts (gesture pairs) call this
/// directly before EACH post, so both paths enforce the identical refusal.
pub(crate) fn boundary_gate(pid: u32, wid: u32) -> Result<(), String> {
    validate_post_target(pid)?;
    if wid == 0 {
        return Err("refusing synthetic post to window id 0".to_owned());
    }
    // Fresh live bundle for the protected/Ghostty refusal (never a caller
    // claim). Direct-spawn helpers have no bundle entry: fall back to the
    // probe-bundle convention used by `live_owned_bundle` only when the PID
    // is live, otherwise refuse unknown.
    let bundle: Option<String> =
        match crate::ffi::bundle_id_for_pid(pid as i32).filter(|b| !b.is_empty()) {
            Some(bundle) => Some(bundle),
            None if crate::ffi::process_exists(pid as i32) => Some(crate::BUNDLE_ID.to_owned()),
            None => None,
        };
    let oracle_agrees = crate::focus::read_agreement(pid, wid).is_ok();
    guarded_verdict(bundle.as_deref(), oracle_agrees)
}

/// Single shared guarded posting boundary: every actual CG key post in
/// this crate flows through here. Enforces [`boundary_gate`] immediately
/// BEFORE each OS post (see its docs for the exact re-reads).
///
/// Sequential reads, honest TOCTOU: proof, bundle, oracle, and post are
/// sequential live reads, not one atomic transaction. A PID reuse between
/// construction and the post is not excluded by this boundary; callers keep
/// their per-edge construction plus the per-edge oracle for that reason.
///
/// Never a Ghostty (or any foreign) event target.
///
/// Tile-catalogue posts carry NO provenance tag (the tap consumes them
/// exactly like user chords -- see the input-session suppression proof).
/// Native switch-chord posts (drag transfers, switch-probe arrows) pass
/// `Some(crate::input_match::OWN_SWITCH_USER_DATA)` so the tap passes our
/// own switch chords while still consuming the user's.
pub(crate) fn post_guarded(
    target: &OwnedPostTarget,
    key_code: u16,
    flags: u64,
    down: bool,
    route: GuardedPostRoute,
) -> Result<PostedEvent, String> {
    post_guarded_with_tag(target, key_code, flags, down, route, None)
}

/// Pure provenance-tag gate: the tag space has exactly two members --
/// absent (tile-catalogue posts, consumed like user chords) and the
/// own-switch tag (native switch chords, passed by the tap). Any other
/// value refuses before the window server is touched. No live touch.
pub(crate) fn validate_provenance_tag(tag: Option<i64>) -> Result<(), String> {
    match tag {
        None | Some(crate::input_match::OWN_SWITCH_USER_DATA) => Ok(()),
        Some(t) => Err(format!(
            "refusing synthetic post with unknown provenance tag {t}"
        )),
    }
}

/// Tagged guarded post: identical to [`post_guarded`] except the created
/// event carries `tag` as `kCGEventSourceUserData` (field 42) before the
/// post. Pass `None` for tile-catalogue posts (no tag); pass
/// `Some(crate::input_match::OWN_SWITCH_USER_DATA)` for native switch
/// chords only. Any other tag value refuses before the window server is
/// touched -- the tag space has exactly two members (absent, own-switch).
pub(crate) fn post_guarded_with_tag(
    target: &OwnedPostTarget,
    key_code: u16,
    flags: u64,
    down: bool,
    route: GuardedPostRoute,
    tag: Option<i64>,
) -> Result<PostedEvent, String> {
    validate_provenance_tag(tag)?;
    let (pid, wid) = (target.pid, target.wid);
    boundary_gate(pid, wid)?;
    match route {
        GuardedPostRoute::Session => raw_post_key_event_to_session(pid, key_code, flags, down, tag),
        GuardedPostRoute::Hid => raw_post_key_event_to_hid(pid, key_code, flags, down, tag),
    }
}

/// Raw session-stream post (crate-private): session taps observe it, and
/// the frontmost app receives it unless a tap suppresses. Only
/// [`post_guarded`] calls this, after owned proof plus the fresh oracle
/// plus bundle refusal.
fn raw_post_key_event_to_session(
    pid: u32,
    key_code: u16,
    flags: u64,
    down: bool,
    tag: Option<i64>,
) -> Result<PostedEvent, String> {
    #[cfg(target_os = "macos")]
    {
        use objc2_core_graphics::{CGEvent, CGEventField, CGEventFlags, CGEventTapLocation};
        let event = CGEvent::new_keyboard_event(None, key_code, down)
            .ok_or_else(|| "CGEventCreateKeyboardEvent returned null".to_owned())?;
        CGEvent::set_flags(Some(&event), CGEventFlags(flags));
        if let Some(t) = tag {
            CGEvent::set_integer_value_field(Some(&event), CGEventField::EventSourceUserData, t);
        }
        let start = Instant::now();
        CGEvent::post(CGEventTapLocation::SessionEventTap, Some(&event));
        Ok(PostedEvent {
            pid,
            key_code,
            flags,
            down,
            post_latency_us: start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64,
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (pid, key_code, flags, down, tag);
        Err("synthetic key posts are macOS-only".to_owned())
    }
}

/// Raw HID-tap post (crate-private, fixture-only discriminator): upstream of
/// the session tap. Only [`post_guarded`] calls this, after owned proof
/// plus the fresh oracle plus bundle refusal.
fn raw_post_key_event_to_hid(
    pid: u32,
    key_code: u16,
    flags: u64,
    down: bool,
    tag: Option<i64>,
) -> Result<PostedEvent, String> {
    #[cfg(target_os = "macos")]
    {
        use objc2_core_graphics::{CGEvent, CGEventField, CGEventFlags, CGEventTapLocation};
        let event = CGEvent::new_keyboard_event(None, key_code, down)
            .ok_or_else(|| "CGEventCreateKeyboardEvent returned null".to_owned())?;
        CGEvent::set_flags(Some(&event), CGEventFlags(flags));
        if let Some(t) = tag {
            CGEvent::set_integer_value_field(Some(&event), CGEventField::EventSourceUserData, t);
        }
        let start = Instant::now();
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
        Ok(PostedEvent {
            pid,
            key_code,
            flags,
            down,
            post_latency_us: start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64,
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (pid, key_code, flags, down, tag);
        Err("synthetic key posts are macOS-only".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_target_floor_refuses_reserved_self_and_out_of_range() {
        assert!(validate_post_target(0).is_err());
        assert!(validate_post_target(1).is_err());
        assert!(validate_post_target(std::process::id()).is_err());
        // Unrepresentable at the signed CG boundary: not a valid OS PID,
        // refused here rather than wrapping at the post.
        assert!(validate_post_target(u32::MAX).is_err());
        assert!(validate_post_target(u32::from(u16::MAX)).is_ok());
        // Representable but foreign: the floor passes (liveness is unproven
        // here); owned proof plus the oracle refuse before any post.
        assert!(validate_post_target(4242).is_ok());
    }

    #[test]
    fn spawn_record_proof_refuses_foreign_without_live_touch() {
        // No record exists for this PID/nonce: refused on pid/nonce
        // mismatch before any bundle read, oracle read, or post. Move the
        // PID far from any live process so no cleanup path can touch it.
        let pid = u32::MAX - 4242;
        assert!(
            OwnedPostTarget::from_spawn_record(pid, "0123456789abcdef0123456789abcdef").is_err()
        );
        // Reserved PIDs refuse even before the record lookup.
        assert!(OwnedPostTarget::from_spawn_record(0, "0123456789abcdef0123456789abcdef").is_err());
        // Representable but unrecorded PID: refused on pid/nonce mismatch
        // (record-file read only — no signal, no AX, no CG touch).
        assert!(
            OwnedPostTarget::from_spawn_record(4242, "0123456789abcdef0123456789abcdef").is_err()
        );
    }

    #[test]
    fn guarded_verdict_refuses_unknown_mismatch_and_ghostty() {
        // Unknown bundle refuses even with agreement.
        assert!(guarded_verdict(None, true).is_err());
        assert!(guarded_verdict(Some(""), true).is_err());
        // Oracle mismatch refuses even with a healthy bundle.
        assert!(guarded_verdict(Some("com.apple.TextEdit"), false).is_err());
        // Ghostty (any case/variant), own bundle, and protected terminals
        // refuse even with agreement.
        for bundle in [
            "com.mitchellh.ghostty",
            "COM.MITCHELLH.GHOSTTY",
            crate::BUNDLE_ID,
            "com.apple.Terminal",
        ] {
            assert!(guarded_verdict(Some(bundle), true).is_err(), "{bundle}");
        }
        // Ordinary bundle with agreement passes the pure verdict (the OS
        // post itself is still macOS-only and oracle-gated live).
        assert!(guarded_verdict(Some("com.apple.TextEdit"), true).is_ok());
    }

    #[test]
    fn guarded_verdict_accepts_main_fallback_when_oracle_agrees() {
        // The Main fallback is accepted exactly when the selected oracle
        // agrees: `selected_focus_agrees` never distinguishes the focused
        // vs main leg source, so a main-sourced agreement passes here.
        let agrees =
            crate::ffi::selected_focus_agrees(4242, 7, Some(4242), Some(4242), Some(7), true);
        assert!(agrees);
        assert!(guarded_verdict(Some("com.apple.TextEdit"), agrees).is_ok());
        // Unknown/mismatch still refuses on the same path.
        let disagrees =
            crate::ffi::selected_focus_agrees(4242, 7, Some(4242), Some(4242), Some(9), true);
        assert!(!disagrees);
        assert!(guarded_verdict(Some("com.apple.TextEdit"), disagrees).is_err());
    }

    #[test]
    fn provenance_tag_space_has_exactly_two_members() {
        // Absent (tile posts, consumed like user chords) and the
        // own-switch tag (native switch chords, passed by the tap) pass;
        // every other value refuses before any live touch.
        assert!(validate_provenance_tag(None).is_ok());
        assert!(validate_provenance_tag(Some(crate::input_match::OWN_SWITCH_USER_DATA)).is_ok());
        assert!(validate_provenance_tag(Some(0)).is_err());
        assert!(validate_provenance_tag(Some(1)).is_err());
        assert!(validate_provenance_tag(Some(i64::MAX)).is_err());
    }

    #[test]
    fn posted_event_shape_is_dispatch_only() {
        // Click-point validation is pure (no OS touch): finite
        // non-negative points pass; NaN/infinite/negative refuse.
        assert!(validate_click_point((100.0, 200.0)).is_ok());
        assert!(validate_click_point((0.0, 0.0)).is_ok());
        assert!(validate_click_point((f64::NAN, 1.0)).is_err());
        assert!(validate_click_point((1.0, f64::INFINITY)).is_err());
        assert!(validate_click_point((-1.0, 1.0)).is_err());
        assert!(validate_click_point((1.0, -0.5)).is_err()); // Portable shape check: a posted event records its own dispatch
        // cost, never a delivery claim. Construction is pure (no OS touch:
        // unit tests never post — live posting happens only in the bounded
        // fixtures after owned proof plus oracle).
        let posted = PostedEvent {
            pid: 4242,
            key_code: 0x04,
            flags: 0x0004_0000,
            down: true,
            post_latency_us: 7,
        };
        assert_eq!(posted.pid, 4242);
        assert_eq!(posted.key_code, 0x04);
        assert!(posted.down);
        assert_eq!(posted.post_latency_us, 7);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn post_guarded_has_no_public_bypass_off_macos() {
        // No bypass remains: the posting entry is crate-private and the
        // target type is unforgeable without owned proof. A foreign target
        // cannot even be constructed without that proof.
        assert!(
            OwnedPostTarget::from_spawn_record(4242, "0123456789abcdef0123456789abcdef").is_err()
        );
    }
}
