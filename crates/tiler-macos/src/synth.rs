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

/// Pure point-specific mouse-post verdict over plain data (no OS touch):
/// the AX hit-test owner read at the exact post point must name exactly
/// `(target_pid, target_wid)` and the window-server hit window read at that
/// point must equal `target_wid`. A stale pre-tile point (the window moved
/// since the coordinates were captured), a foreign window, or an unreadable
/// leg refuses WITHOUT posting — focus agreement alone never proves the
/// point. Unit-tested; the lone-edge boundary below performs the reads and
/// passes the outcomes here just before its post. (Parked-caller note: the
/// lone-edge production caller is parked with `owned-drag-post`, so live
/// builds hold this for the balanced-contract follow-up plus its unit
/// tests; the verdict itself is unchanged and unweakened.)
#[allow(dead_code)]
pub(crate) fn mouse_point_verdict(
    target_pid: u32,
    target_wid: u32,
    ax_owner: Option<(u32, u32)>,
    cg_hit: Option<u32>,
) -> Result<(), String> {
    if ax_owner != Some((target_pid, target_wid)) {
        return Err(
            "refusing synthetic mouse post: point hit owner mismatch (stale or foreign point)"
                .to_owned(),
        );
    }
    if cg_hit != Some(target_wid) {
        return Err(
            "refusing synthetic mouse post: window-server hit mismatch (stale or foreign point)"
                .to_owned(),
        );
    }
    Ok(())
}

/// Fresh point-specific hit proof at one post point (posting threads only,
/// never a tap callback): the AX hit-test owner plus the window-server hit
/// window, re-read just before the post. Unreadable legs are `None` (the
/// verdict refuses, never guesses). (Parked-caller note: see
/// [`mouse_point_verdict`].)
#[allow(dead_code)]
fn fresh_post_point_proof(point: (f64, f64)) -> (Option<(u32, u32)>, Option<u32>) {
    let ax_owner = crate::ffi::ax_hit_test_owner(point.0, point.1).ok();
    let cg_hit = crate::ffi::sls_window_at_point(point.0, point.1).ok();
    (ax_owner, cg_hit)
}

/// Paired-click safety park (diagnostic-only, terminal evidence): the
/// shared click boundary would post a down/up pair through the global HID
/// tap, whose raw post broadcasts independently of `pid` (no verified
/// per-PID release route exists), so a refused up edge after a posted down
/// would leave an unmatched button-down system-wide. Until a balanced
/// per-PID release contract is proven, the paired click refuses BEFORE any
/// post — refused-before-down posts nothing — and callers surface this
/// reason instead of posting. Lone single edges stay on
/// [`post_mouse_edge_guarded`].
pub const CLICK_PAIR_PARKED_REASON: &str = "parked: paired mouse click refuses before any post (no balanced per-PID release route; a refused up after a posted down would leave an unmatched button-down)";

/// Pure paired-click park verdict over plain data (no OS touch): always
/// refuses with [`CLICK_PAIR_PARKED_REASON`], naming the exact proved
/// target the refusal spared. Unit-tested; [`post_mouse_click_guarded`]
/// consults this before any gate or post.
pub(crate) fn paired_click_parked(pid: u32, wid: u32) -> String {
    format!("{CLICK_PAIR_PARKED_REASON} (pid={pid} wid={wid})")
}

/// Single shared guarded click boundary: HARD PARKED (see
/// [`CLICK_PAIR_PARKED_REASON`]) — one synthetic left-button down/up pair
/// at `point` (CG top-left points) would traverse the HID event tap,
/// untagged (plain synthetic, like tile-catalogue posts — no tap passes or
/// consumes clicks). The fresh point-specific [`mouse_point_verdict`]
/// stays enforced on the lone-edge path ([`post_mouse_edge_guarded`]);
/// this paired entry refuses before any post so no down can strand without
/// its up.
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
    // HARD PARK (unmatched-down safety): a down posted here followed by a
    // refused up (moved focus, moved window, lost oracle) would return Err
    // with the button still down system-wide, and no verified per-PID
    // release route exists to aim the terminal up at exactly this owned
    // target (raw posts broadcast through the tap ignoring pid). Refuse
    // BEFORE any post; diagnostic click legs surface this reason.
    Err(paired_click_parked(pid, wid))
}

/// Raw HID mouse post (crate-private, PARKED with
/// [`post_mouse_click_guarded`]): kept for the balanced-contract
/// follow-up; no live caller while the paired click refuses before any
/// post.
#[allow(dead_code)]
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

/// Shared HID mouse-event kind for drag-family posts: exactly the
/// transfer executor's [`crate::space_drag_transfer::DragStep`] mouse
/// vocabulary. Both the owned-helper transfer backend and the user-run
/// drag diagnostics post through [`hid_mouse_post`], so the order, source,
/// tap, flags, and tag cannot drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HidMouseKind {
    Move,
    Down,
    Dragged,
    Up,
}

impl HidMouseKind {
    /// Executor step mapping (transfer order vocabulary, shared).
    #[must_use]
    pub(crate) fn from_drag_step(step: crate::space_drag_transfer::DragStep) -> Option<Self> {
        match step {
            crate::space_drag_transfer::DragStep::MouseMove => Some(Self::Move),
            crate::space_drag_transfer::DragStep::MouseDown => Some(Self::Down),
            crate::space_drag_transfer::DragStep::MouseDragged => Some(Self::Dragged),
            crate::space_drag_transfer::DragStep::MouseUp => Some(Self::Up),
            _ => None,
        }
    }
}

/// Single shared HID mouse post: source `None`, tap `HID`, caller flags
/// and tag. The ONLY construction site for drag-family mouse events (the
/// transfer executor and every diagnostic mode). Only finiteness is
/// checked here: cursor points may legitimately be negative (displays left
/// of main), so negativity never refuses. Returns the dispatch latency
/// (never a delivery claim).
#[cfg(target_os = "macos")]
pub(crate) fn hid_mouse_post(
    kind: HidMouseKind,
    point: (f64, f64),
    flags: u64,
    tag: Option<i64>,
) -> Result<u64, String> {
    use objc2_core_graphics::{
        CGEvent, CGEventField, CGEventFlags, CGEventTapLocation, CGEventType, CGMouseButton,
    };
    crate::synth::validate_provenance_tag(tag)?;
    if !point.0.is_finite() || !point.1.is_finite() {
        return Err("refusing HID mouse post at a non-finite point".to_owned());
    }
    let ty = match kind {
        HidMouseKind::Move => CGEventType::MouseMoved,
        HidMouseKind::Down => CGEventType::LeftMouseDown,
        HidMouseKind::Dragged => CGEventType::LeftMouseDragged,
        HidMouseKind::Up => CGEventType::LeftMouseUp,
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
    CGEvent::set_flags(Some(&event), CGEventFlags(flags));
    if let Some(t) = tag {
        CGEvent::set_integer_value_field(Some(&event), CGEventField::EventSourceUserData, t);
    }
    let start = Instant::now();
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
    Ok(start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn hid_mouse_post(
    _kind: HidMouseKind,
    _point: (f64, f64),
    _flags: u64,
    _tag: Option<i64>,
) -> Result<u64, String> {
    Err("HID mouse posts are macOS-only".to_owned())
}

/// Single shared HID key post: source `None`, tap `HID`, caller flags and
/// tag (tag space gated to absent/own-switch). The ONLY construction site
/// for drag-family chord events (transfer executor via
/// [`post_guarded_with_tag`], diagnostics directly after their own narrow
/// gates). Returns the dispatch latency (never a delivery claim).
#[cfg(target_os = "macos")]
pub(crate) fn hid_key_post(
    key_code: u16,
    flags: u64,
    down: bool,
    tag: Option<i64>,
) -> Result<u64, String> {
    use objc2_core_graphics::{CGEvent, CGEventField, CGEventFlags, CGEventTapLocation};
    validate_provenance_tag(tag)?;
    let event = CGEvent::new_keyboard_event(None, key_code, down)
        .ok_or_else(|| "CGEventCreateKeyboardEvent returned null".to_owned())?;
    CGEvent::set_flags(Some(&event), CGEventFlags(flags));
    if let Some(t) = tag {
        CGEvent::set_integer_value_field(Some(&event), CGEventField::EventSourceUserData, t);
    }
    let start = Instant::now();
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
    Ok(start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn hid_key_post(
    _key_code: u16,
    _flags: u64,
    _down: bool,
    _tag: Option<i64>,
) -> Result<u64, String> {
    Err("HID key posts are macOS-only".to_owned())
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

/// Raw session-stream post (crate-visible shared primitive): session taps
/// observe it, and the frontmost app receives it unless a tap suppresses.
/// [`post_guarded`] calls this after owned proof plus the fresh oracle plus
/// bundle refusal; the disposable-document diagnostic route calls it only
/// after its own per-edge proof (and gateless for its bypass release, which
/// posts key-up with cleared flags so no modifier can stick). No other
/// caller: reaching the window server always requires a per-edge proof at
/// the call site.
pub(crate) fn raw_post_key_event_to_session(
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
/// plus the fresh oracle plus bundle refusal. Construction is shared with
/// [`hid_key_post`] (same source/tap/flags/tag path).
fn raw_post_key_event_to_hid(
    pid: u32,
    key_code: u16,
    flags: u64,
    down: bool,
    tag: Option<i64>,
) -> Result<PostedEvent, String> {
    #[cfg(target_os = "macos")]
    {
        let _ = pid;
        let post_latency_us = hid_key_post(key_code, flags, down, tag)?;
        Ok(PostedEvent {
            pid,
            key_code,
            flags,
            down,
            post_latency_us,
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (pid, key_code, flags, down, tag);
        Err("synthetic key posts are macOS-only".to_owned())
    }
}

/// One synthetic mouse edge for the drag fixture: left-button down, drag
/// (move with the button held), or up. Exactly the tap's
/// [`crate::ffi::DragMouseKind`] vocabulary, so the posted edge and the
/// consumed edge name the same kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEdge {
    Down,
    Drag,
    Up,
}

impl MouseEdge {
    /// Parse the CLI edge name. Exactly three members; anything else (a
    /// click pair, a scroll, a right button) refuses before any live touch.
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "down" => Ok(Self::Down),
            "drag" => Ok(Self::Drag),
            "up" => Ok(Self::Up),
            _ => Err(format!(
                "unknown mouse edge '{name}' (expected down|drag|up)"
            )),
        }
    }
}

/// One posted synthetic mouse edge: target, CG point, flags, edge, and the
/// dispatch latency around the post call itself (never delivery). Delivery
/// is proven harness-side (tap consume counters, overlay readback, helper
/// state), never this dispatch receipt.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PostedMouseEdge {
    pub pid: u32,
    pub point: (f64, f64),
    pub flags: u64,
    pub edge: MouseEdge,
    /// Microseconds around the post call (dispatch cost, not delivery).
    pub post_latency_us: u64,
}

/// Single shared guarded mouse-edge boundary: one synthetic left-button
/// edge at `point` (CG top-left points) with `flags` through the SESSION
/// event tap, so session taps observe it exactly like a hardware edge. The
/// drag-preview mouse tap consumes a proven Meta+left down and its held
/// edges; anything unproven passes through to the owned window, which
/// ignores non-key input (no click side effects by construction: a lone
/// edge is down-only, drag-only, or up-only — never a down/up click pair).
///
/// Enforces [`boundary_gate`] plus the fresh point-specific
/// [`mouse_point_verdict`] immediately BEFORE the OS post, so a stale
/// target (focus moved, or the window moved since the point was captured —
/// e.g. stale pre-tile coordinates now landing on an unrelated window)
/// refuses at the boundary WITHOUT posting.
/// Never a Ghostty (or any foreign) event target.
///
/// Parked-caller note: the only production caller (`owned-drag-post`)
/// refuses before any live touch until a balanced release contract is
/// proven, so live builds hold this boundary for that follow-up; the
/// per-edge gate plus point proof are unchanged and unweakened.
#[allow(dead_code)]
pub(crate) fn post_mouse_edge_guarded(
    target: &OwnedPostTarget,
    point: (f64, f64),
    flags: u64,
    edge: MouseEdge,
) -> Result<PostedMouseEdge, String> {
    validate_click_point(point)?;
    let (pid, wid) = (target.pid(), target.wid());
    boundary_gate(pid, wid)?;
    // Fresh point-specific hit proof re-read just before the post: the AX
    // owner plus the window-server hit at this exact point must name this
    // exact owned target, or refuse WITHOUT posting.
    let (ax_owner, cg_hit) = fresh_post_point_proof(point);
    mouse_point_verdict(pid, wid, ax_owner, cg_hit)?;
    let post_latency_us = raw_post_mouse_event_to_session(pid, point, flags, edge)?;
    Ok(PostedMouseEdge {
        pid,
        point,
        flags,
        edge,
        post_latency_us,
    })
}

/// Raw session-stream mouse post (crate-private): session taps observe it,
/// and the focused owned app receives what the tap passes. Only
/// [`post_mouse_edge_guarded`] calls this, after owned proof plus the fresh
/// oracle plus bundle refusal. (Parked-caller note: see
/// [`post_mouse_edge_guarded`].)
#[allow(dead_code)]
fn raw_post_mouse_event_to_session(
    pid: u32,
    point: (f64, f64),
    flags: u64,
    edge: MouseEdge,
) -> Result<u64, String> {
    #[cfg(target_os = "macos")]
    {
        use objc2_core_graphics::{
            CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, CGMouseButton,
        };
        let ty = match edge {
            MouseEdge::Down => CGEventType::LeftMouseDown,
            MouseEdge::Drag => CGEventType::LeftMouseDragged,
            MouseEdge::Up => CGEventType::LeftMouseUp,
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
        CGEvent::set_flags(Some(&event), CGEventFlags(flags));
        let start = Instant::now();
        CGEvent::post(CGEventTapLocation::SessionEventTap, Some(&event));
        let _ = pid;
        Ok(start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (pid, point, flags, edge);
        Err("synthetic mouse posts are macOS-only".to_owned())
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

    #[test]
    fn mouse_edge_space_has_exactly_three_members() {
        assert_eq!(MouseEdge::parse("down"), Ok(MouseEdge::Down));
        assert_eq!(MouseEdge::parse("drag"), Ok(MouseEdge::Drag));
        assert_eq!(MouseEdge::parse("up"), Ok(MouseEdge::Up));
        // Click pairs, scrolls, and right buttons refuse before any live
        // touch (the fixture posts lone edges only, never a click pair).
        for name in ["click", "scroll", "right", "move", "", "DOWN"] {
            assert!(MouseEdge::parse(name).is_err(), "{name}");
        }
    }

    #[test]
    fn mouse_point_verdict_accepts_only_the_exact_current_target() {
        // Valid current target: both hit legs name the exact owned window.
        assert!(mouse_point_verdict(4242, 7, Some((4242, 7)), Some(7)).is_ok());
        // Wrong window, same process (e.g. another owned helper under a
        // crossing drag): refuses — cross-helper drag stays parked.
        assert!(mouse_point_verdict(4242, 7, Some((4242, 9)), Some(7)).is_err());
        assert!(mouse_point_verdict(4242, 7, Some((4242, 7)), Some(9)).is_err());
        // Foreign process at the point (e.g. stale pre-tile coordinates now
        // over Ghostty or any unrelated window): refuses on both legs.
        assert!(mouse_point_verdict(4242, 7, Some((9999, 3)), Some(7)).is_err());
        assert!(mouse_point_verdict(4242, 7, Some((4242, 7)), Some(3)).is_err());
        assert!(mouse_point_verdict(4242, 7, Some((9999, 3)), Some(3)).is_err());
        // Unreadable legs refuse (never guessed): missing AX, missing
        // window-server hit, or both.
        assert!(mouse_point_verdict(4242, 7, None, Some(7)).is_err());
        assert!(mouse_point_verdict(4242, 7, Some((4242, 7)), None).is_err());
        assert!(mouse_point_verdict(4242, 7, None, None).is_err());
    }

    #[test]
    fn mouse_point_verdict_refuses_stale_pre_tile_point_over_unrelated_window() {
        // Incident shape: helper tiled to a new frame, but the posted point
        // is a stale pre-tile coordinate now landing on an unrelated
        // (protected) window. The expected-focus oracle still names the
        // helper, yet the point hit names the stranger: refuse WITHOUT
        // posting.
        let target_pid = 4242;
        let target_wid = 7;
        let stranger_pid = 7777;
        let stranger_wid = 42;
        assert!(
            mouse_point_verdict(
                target_pid,
                target_wid,
                Some((stranger_pid, stranger_wid)),
                Some(stranger_wid),
            )
            .is_err()
        );
        // Half-changed legs refuse too (one leg drifted, the other stale).
        assert!(
            mouse_point_verdict(
                target_pid,
                target_wid,
                Some((target_pid, target_wid)),
                Some(stranger_wid),
            )
            .is_err()
        );
        assert!(
            mouse_point_verdict(
                target_pid,
                target_wid,
                Some((stranger_pid, stranger_wid)),
                Some(target_wid),
            )
            .is_err()
        );
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

    #[test]
    fn paired_click_park_refuses_before_any_post_for_every_target() {
        // The paired click is hard-parked: the pure verdict always refuses,
        // naming the exact proved target the refusal spared, before any
        // gate read or post. No down is ever posted, so no up can strand.
        for (pid, wid) in [(4242, 7), (4243, 9), (u16::MAX as u32, 1)] {
            let reason = paired_click_parked(pid, wid);
            assert!(reason.contains("parked"), "unexpected: {reason}");
            assert!(reason.contains("before any post"), "unexpected: {reason}");
            assert!(
                reason.contains(&format!("pid={pid} wid={wid}")),
                "unexpected: {reason}"
            );
        }
        assert!(CLICK_PAIR_PARKED_REASON.contains("parked"));
        assert!(CLICK_PAIR_PARKED_REASON.contains("unmatched"));
    }
}
