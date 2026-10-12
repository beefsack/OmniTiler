//! Project-driven Win+Left stationary gesture: portable classifier plus bounded queue.
//!
//! The OS delivers the gesture through a prompt `WH_MOUSE_LL` hook ([`sys`],
//! `cfg(windows)` only); this module owns the portable decision state so
//! deterministic tests cover queueing, down/up consumption, pointer
//! coalescing, invalidation, zero-movement, and owner settle vocabulary
//! without native calls.
//!
//! Contract (selected stationary mover):
//! - Win (either side) + left-button binds a managed TILED mover even when
//!   unfocused. No focused-only restriction, no extra-modifier gate.
//! - Floating/sticky members stay native titlebar-only: the hook never arms
//!   on them (the owner publishes tiled-only origins), so their Win+Left
//!   passes through untouched.
//! - The real window keeps its source allocation mid-gesture; release places
//!   through the shared Engine drop once. Zero movement, Esc, self,
//!   other-centre, outside, and invalidation make no plan and change no
//!   geometry.

/// Bounded hook-to-owner queue capacity. Moves coalesce into the armed slot
/// (latest wins), so only discrete down/up/cancel edges consume capacity.
pub const WINDRAG_QUEUE_CAP: usize = 64;
/// Maximum windrag edges dispatched per owner iteration; the remainder stays
/// queued so a burst cannot starve the tick.
pub const WINDRAG_MAX_DISPATCH_PER_TICK: usize = 8;

use crate::snapkey::SnapOrigin;

/// Down-time bound snapshot: the published tiled origin plus the member
/// lifetime tag, cloned in the hook callback (cheap map clone, no native
/// identity probes). The owner requires it to match fresh HWND/PID/
/// creation/token/tag evidence before arming, so an HWND reuse between the
/// callback and the owner drain cannot rebind: a different-process reuse
/// fails PID/creation, and a same-process reuse fails the tag (window
/// properties die with their window, so the fresh window reads `None`
/// against the snapshot's `Some`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WinDragSnapshot {
    /// Published origin bound at callback time (HWND/PID/creation/token).
    pub origin: SnapOrigin,
    /// Member lifetime tag cloned from the publish map at callback time.
    pub tag: Option<String>,
}

/// One hook-observed windrag edge. `esc_seq` is the keyboard Esc sequence
/// sampled in the hook callback at edge time (END-bound ordering, mirroring
/// the native MoveSizeEnd snapshot); the owner latches cancellation only
/// when it is newer than the down-time snapshot. `snapshot` rides the Down
/// only; the Up closes the armed gesture without rebinding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WinDragEdge {
    /// Bound gesture subject (root HWND under the pointer at down time).
    pub hwnd: u64,
    /// Edge kind.
    pub kind: WinDragKind,
    /// Pointer position for this edge (down point, or release point for up).
    pub x: i32,
    /// Pointer position for this edge (down point, or release point for up).
    pub y: i32,
    /// Keyboard Esc sequence at edge time.
    pub esc_seq: u64,
    /// Down-time bound snapshot (`Some` on Down, `None` otherwise).
    pub snapshot: Option<WinDragSnapshot>,
}

/// Owner-side pure gate for one drained Down: the callback snapshot must
/// still match fresh evidence on every field. Pure over records: no native
/// calls, so deterministic tests pin down-mismatch and same-process reuse
/// directly instead of leaning on the member-lifetime suite.
#[must_use]
pub fn validate_windrag_down(
    snapshot: &WinDragSnapshot,
    fresh_origin: &SnapOrigin,
    fresh_tag: &Option<String>,
    live_pid: u32,
    live_creation: &str,
    live_tag: &Option<String>,
) -> bool {
    snapshot.origin == *fresh_origin
        && snapshot.tag == *fresh_tag
        && snapshot.origin.pid == live_pid
        && snapshot.origin.creation == live_creation
        && snapshot.tag == *live_tag
}

/// Discrete hook edge kinds. Moves never queue: the armed slot keeps the
/// latest pointer (bounded coalescing) and the single up carries the release
/// point bound at callback time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinDragKind {
    Down,
    Up,
    /// Owner-side invalidation has cleared the gesture (identity, revision,
    /// suspension, or destruction loss). Queued so the owner settles once
    /// with no plan; never replayed.
    Cancel,
}

/// Bounded FIFO from the hook callback to the owner. `push` refuses past
/// capacity and counts the loss; saturation fails closed (the armed hook
/// slot is cleared with the loss so a half-gesture can never strand a
/// consumed down without its matching up reaching the owner).
#[derive(Debug, Default)]
pub struct WinDragQueue {
    inner: std::collections::VecDeque<WinDragEdge>,
    /// Saturating count of edges dropped past capacity.
    pub dropped: u32,
}

impl WinDragQueue {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: std::collections::VecDeque::with_capacity(WINDRAG_QUEUE_CAP),
            dropped: 0,
        }
    }

    /// Push one edge; false (plus `dropped + 1`) past capacity. Downs use
    /// this fallible push: a saturated Down passes through natively with
    /// nothing armed, so nothing can strand.
    pub fn push(&mut self, edge: WinDragEdge) -> bool {
        if self.inner.len() >= WINDRAG_QUEUE_CAP {
            self.dropped = self.dropped.saturating_add(1);
            return false;
        }
        self.inner.push_back(edge);
        true
    }

    /// Push one terminal edge (Up/Cancel) with guaranteed delivery: past
    /// capacity the oldest edge is evicted (counted) to make room, so a
    /// lost Up can never leave an owner gesture armed forever, and the
    /// paired Up is always swallowed even when saturated. The owner
    /// discards open project gestures when it observes loss, so an
    /// evicted Down never settles without its journey.
    pub fn push_terminal(&mut self, edge: WinDragEdge) {
        if self.inner.len() >= WINDRAG_QUEUE_CAP {
            self.inner.pop_front();
            self.dropped = self.dropped.saturating_add(1);
        }
        self.inner.push_back(edge);
    }

    pub fn pop_front(&mut self) -> Option<WinDragEdge> {
        self.inner.pop_front()
    }

    /// Count one loss without queueing (saturated Down that passes
    /// through with nothing armed): the owner observes it via
    /// [`dropped`](WinDragQueue::dropped) and discards ambiguity fail-closed.
    pub fn note_drop(&mut self) {
        self.dropped = self.dropped.saturating_add(1);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[must_use]
    pub fn is_full(&self) -> bool {
        self.inner.len() >= WINDRAG_QUEUE_CAP
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }
}

/// Hook-side arm slot: the single open project gesture. Portable so tests
/// pin consumption without native calls: a down consumes iff Win is held
/// and the root HWND is a published tiled origin; the matching up consumes
/// iff it closes the armed subject; moves only refresh the latest pointer.
///
/// Invalidation and suspension disarm the gesture but retain a separate
/// swallow-until-up obligation: a consumed Down must never be followed by
/// a passed-through Up (half-click to the app). The obligation clears only
/// on the Up itself or on uninstall.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WinHookArm {
    bound: Option<WinBound>,
    swallow_up: bool,
}

/// Up verdict: a closing Up carries its bound journey for the owner, a
/// disarmed pairing is swallowed with no edge, and anything else passes
/// through natively.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpOutcome {
    /// Closes the armed subject; the owner settles this bound journey.
    Gesture(WinBound),
    /// Disarmed after a consumed Down; swallow with no owner edge.
    Swallowed,
    /// No open or owed pairing; pass through natively.
    Pass,
}

/// One armed gesture: down-time subject plus down/latest pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WinBound {
    /// Root HWND bound at down time.
    pub hwnd: u64,
    /// Down-time pointer.
    pub start_x: i32,
    /// Down-time pointer.
    pub start_y: i32,
    /// Latest pointer (coalesced moves).
    pub cur_x: i32,
    /// Latest pointer (coalesced moves).
    pub cur_y: i32,
}

impl WinHookArm {
    /// Down verdict: consume (arm + queue) iff Win is held and the subject
    /// is a published tiled origin. Floating/sticky/unmanaged subjects pass
    /// through natively; an already-armed slot passes a second down through
    /// (one gesture at a time, never rebind mid-hold).
    pub fn on_down(&mut self, hwnd: u64, x: i32, y: i32, win_held: bool, eligible: bool) -> bool {
        if !win_held || !eligible || hwnd == 0 || self.bound.is_some() {
            return false;
        }
        self.bound = Some(WinBound {
            hwnd,
            start_x: x,
            start_y: y,
            cur_x: x,
            cur_y: y,
        });
        true
    }

    /// Move: refresh the latest pointer only when armed. Never queues, never
    /// consumes (moves always pass to the OS).
    pub fn on_move(&mut self, x: i32, y: i32) {
        if let Some(bound) = self.bound.as_mut() {
            bound.cur_x = x;
            bound.cur_y = y;
        }
    }

    /// Up verdict: consume iff it closes the armed subject. The release
    /// point is the armed latest pointer bound here at callback time, never
    /// re-read later. Clears the slot either way it was armed. A disarmed
    /// pairing (invalidation/suspension after a consumed Down) is still
    /// swallowed exactly once; anything else passes through.
    pub fn on_up(&mut self, x: i32, y: i32) -> UpOutcome {
        if let Some(bound) = self.bound.take() {
            return UpOutcome::Gesture(WinBound {
                hwnd: bound.hwnd,
                start_x: bound.start_x,
                start_y: bound.start_y,
                cur_x: x,
                cur_y: y,
            });
        }
        if self.swallow_up {
            self.swallow_up = false;
            return UpOutcome::Swallowed;
        }
        UpOutcome::Pass
    }

    /// Abort an armed Down that could not queue (defense in depth; the
    /// caller pre-checks capacity so this is unreachable in practice):
    /// drops the arm with NO swallow obligation, because the Down passed
    /// through natively and the app owns the whole click including its Up.
    pub fn abort_down(&mut self) {
        self.bound = None;
    }

    /// Clear the armed slot when it names `hwnd` (invalidation): identity,
    /// revision, suspension, or destruction loss. Returns true when a
    /// gesture was disarmed (the caller queues a Cancel); the swallow
    /// obligation is retained so the paired Up never reaches the app. No
    /// replay.
    pub fn invalidate(&mut self, hwnd: u64) -> bool {
        if self.bound.is_some_and(|bound| bound.hwnd == hwnd) {
            self.bound = None;
            self.swallow_up = true;
            return true;
        }
        false
    }

    /// Clear any armed slot (suspend/teardown path only). Returns true when
    /// a gesture was disarmed (the caller queues a Cancel); the swallow
    /// obligation is retained like [`WinHookArm::invalidate`].
    pub fn clear(&mut self) -> bool {
        if self.bound.is_some() {
            self.bound = None;
            self.swallow_up = true;
            return true;
        }
        false
    }

    #[must_use]
    pub fn armed_hwnd(&self) -> Option<u64> {
        self.bound.map(|bound| bound.hwnd)
    }

    #[must_use]
    pub fn is_armed(&self) -> bool {
        self.bound.is_some()
    }
}

/// Owner settle from the tracked pointer journey: explicit Esc first (a
/// cancelled project hold keeps source geometry, so pointer equality must
/// never decide this path), then zero movement (no plan, no mutation).
/// Anything else proceeds to the shared Engine drop at the release point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinSettle {
    /// Cancelled: reconcile once with no Engine mutation.
    Cancelled,
    /// Zero pointer movement: reconcile once (reassert source), no plan.
    NoChange,
    /// Release carries a real journey: route one shared Engine drop here.
    Drop { x: i32, y: i32 },
}

#[must_use]
pub const fn settle_pointer_journey(
    start_x: i32,
    start_y: i32,
    end_x: i32,
    end_y: i32,
    esc: bool,
) -> WinSettle {
    if esc {
        return WinSettle::Cancelled;
    }
    if start_x == end_x && start_y == end_y {
        return WinSettle::NoChange;
    }
    WinSettle::Drop { x: end_x, y: end_y }
}

#[cfg(test)]
mod tests {
    use super::{
        UpOutcome, WINDRAG_QUEUE_CAP, WinDragEdge, WinDragKind, WinDragQueue, WinDragSnapshot,
        WinHookArm, WinSettle, settle_pointer_journey, validate_windrag_down,
    };
    use crate::snapkey::SnapOrigin;

    fn origin(hwnd: u64, pid: u32, creation: &str, token: &str) -> SnapOrigin {
        SnapOrigin {
            hwnd,
            token: token.to_owned(),
            pid,
            creation: creation.to_owned(),
        }
    }

    fn snapshot(hwnd: u64, tag: Option<&str>) -> WinDragSnapshot {
        WinDragSnapshot {
            origin: origin(hwnd, 100, "create-1", "w1"),
            tag: tag.map(str::to_owned),
        }
    }

    #[test]
    fn down_consumes_only_win_held_eligible_idle() {
        let mut arm = WinHookArm::default();
        assert!(!arm.on_down(7, 10, 10, false, true));
        assert!(!arm.on_down(7, 10, 10, true, false));
        assert!(!arm.on_down(0, 10, 10, true, true));
        assert!(arm.on_down(7, 10, 10, true, true));
        assert!(!arm.on_down(9, 20, 20, true, true));
        assert_eq!(arm.armed_hwnd(), Some(7));
    }

    #[test]
    fn up_consumes_only_armed_subject_with_latest_release() {
        let mut arm = WinHookArm::default();
        assert_eq!(arm.on_up(5, 5), UpOutcome::Pass);
        assert!(arm.on_down(7, 10, 10, true, true));
        arm.on_move(30, 40);
        arm.on_move(50, 60);
        let bound = match arm.on_up(50, 60) {
            UpOutcome::Gesture(bound) => bound,
            other => panic!("armed up binds, got {other:?}"),
        };
        assert_eq!(bound.hwnd, 7);
        assert_eq!((bound.start_x, bound.start_y), (10, 10));
        assert_eq!((bound.cur_x, bound.cur_y), (50, 60));
        assert!(!arm.is_armed());
        assert_eq!(arm.on_up(50, 60), UpOutcome::Pass);
    }

    #[test]
    fn invalidation_retains_swallow_until_up() {
        let mut arm = WinHookArm::default();
        assert!(arm.on_down(7, 1, 1, true, true));
        assert!(!arm.invalidate(9));
        assert!(arm.is_armed());
        assert!(arm.invalidate(7));
        assert!(!arm.is_armed());
        // Paired Up after a consumed Down is swallowed exactly once, never
        // passed to the app; the next Up passes normally.
        assert_eq!(arm.on_up(9, 9), UpOutcome::Swallowed);
        assert_eq!(arm.on_up(9, 9), UpOutcome::Pass);
    }

    #[test]
    fn clear_retains_swallow_until_up() {
        let mut arm = WinHookArm::default();
        assert!(!arm.clear());
        assert!(arm.on_down(7, 1, 1, true, true));
        assert!(arm.clear());
        assert_eq!(arm.on_up(2, 2), UpOutcome::Swallowed);
        assert_eq!(arm.on_up(2, 2), UpOutcome::Pass);
    }

    #[test]
    fn queue_saturation_fails_closed_with_loss_count() {
        let mut queue = WinDragQueue::new();
        for n in 0..WINDRAG_QUEUE_CAP {
            assert!(queue.push(WinDragEdge {
                hwnd: n as u64,
                kind: WinDragKind::Down,
                x: 0,
                y: 0,
                esc_seq: 0,
                snapshot: None,
            }));
        }
        assert!(queue.is_full());
        assert!(!queue.push(WinDragEdge {
            hwnd: 999,
            kind: WinDragKind::Down,
            x: 0,
            y: 0,
            esc_seq: 0,
            snapshot: None,
        }));
        assert_eq!(queue.dropped, 1);
        assert_eq!(queue.len(), WINDRAG_QUEUE_CAP);
    }

    #[test]
    fn terminal_push_guarantees_delivery_on_saturation() {
        let mut queue = WinDragQueue::new();
        for n in 0..WINDRAG_QUEUE_CAP {
            assert!(queue.push(WinDragEdge {
                hwnd: n as u64,
                kind: WinDragKind::Down,
                x: 0,
                y: 0,
                esc_seq: 0,
                snapshot: None,
            }));
        }
        queue.push_terminal(WinDragEdge {
            hwnd: 7,
            kind: WinDragKind::Up,
            x: 50,
            y: 60,
            esc_seq: 3,
            snapshot: None,
        });
        assert_eq!(queue.len(), WINDRAG_QUEUE_CAP);
        assert_eq!(queue.dropped, 1);
        let mut last = None;
        while let Some(edge) = queue.pop_front() {
            last = Some(edge);
        }
        let last = last.expect("terminal edge delivered");
        assert_eq!(last.kind, WinDragKind::Up);
        assert_eq!((last.hwnd, last.x, last.y), (7, 50, 60));
    }

    #[test]
    fn settle_orders_esc_before_zero_before_drop() {
        assert_eq!(
            settle_pointer_journey(5, 5, 5, 5, true),
            WinSettle::Cancelled
        );
        assert_eq!(
            settle_pointer_journey(5, 5, 5, 5, false),
            WinSettle::NoChange
        );
        assert_eq!(
            settle_pointer_journey(5, 5, 9, 5, false),
            WinSettle::Drop { x: 9, y: 5 }
        );
    }

    #[test]
    fn down_gate_accepts_exact_snapshot_only() {
        let snap = snapshot(7, Some("tag-1"));
        let fresh = origin(7, 100, "create-1", "w1");
        assert!(validate_windrag_down(
            &snap,
            &fresh,
            &Some("tag-1".to_owned()),
            100,
            "create-1",
            &Some("tag-1".to_owned())
        ));
        // HWND reuse by another process: PID differs.
        assert!(!validate_windrag_down(
            &snap,
            &fresh,
            &Some("tag-1".to_owned()),
            200,
            "create-1",
            &Some("tag-1".to_owned())
        ));
        // HWND reuse by another window of the same process: creation differs.
        assert!(!validate_windrag_down(
            &snap,
            &fresh,
            &Some("tag-1".to_owned()),
            100,
            "create-2",
            &Some("tag-1".to_owned())
        ));
        // Same-process HWND reuse: the fresh window never carried the
        // lifetime tag (properties die with their window).
        assert!(!validate_windrag_down(
            &snap,
            &fresh,
            &Some("tag-1".to_owned()),
            100,
            "create-1",
            &None
        ));
        // Stale snapshot against a retokened member.
        let retokened = origin(7, 100, "create-1", "w2");
        assert!(!validate_windrag_down(
            &snap,
            &retokened,
            &Some("tag-1".to_owned()),
            100,
            "create-1",
            &Some("tag-1".to_owned())
        ));
        // Stale published tag (member re-admitted with a new tag).
        assert!(!validate_windrag_down(
            &snap,
            &fresh,
            &Some("tag-2".to_owned()),
            100,
            "create-1",
            &Some("tag-2".to_owned())
        ));
    }

    #[test]
    fn down_arms_before_moves_with_callback_bound_release() {
        // Hook-arm ordering: moves before the Down change nothing; the Down
        // binds the exact subject; the Up carries the callback-bound latest
        // pointer (never a later re-read) into the owner settle vocabulary.
        let mut arm = WinHookArm::default();
        arm.on_move(99, 99);
        assert!(!arm.is_armed());
        assert!(arm.on_down(7, 10, 10, true, true));
        arm.on_move(30, 40);
        let bound = match arm.on_up(30, 40) {
            UpOutcome::Gesture(bound) => bound,
            other => panic!("armed up binds, got {other:?}"),
        };
        assert_eq!(bound.hwnd, 7);
        assert_eq!((bound.start_x, bound.start_y), (10, 10));
        assert_eq!((bound.cur_x, bound.cur_y), (30, 40));
        // The bound journey classifies as a drop at the release point.
        assert_eq!(
            settle_pointer_journey(
                bound.start_x,
                bound.start_y,
                bound.cur_x,
                bound.cur_y,
                false
            ),
            WinSettle::Drop { x: 30, y: 40 }
        );
        // A later move cannot reopen the closed journey.
        arm.on_move(70, 80);
        assert!(!arm.is_armed());
        assert_eq!(arm.on_up(70, 80), UpOutcome::Pass);
    }

    #[test]
    fn settle_orders_cancelled_nochange_drop() {
        // Pure settle classification: Cancelled outranks zero movement (an
        // Esc-cancelled hold never reads as a no-move), zero reads as
        // NoChange, and only a real journey reads as Drop.
        assert_eq!(
            settle_pointer_journey(10, 10, 10, 10, false),
            WinSettle::NoChange
        );
        assert_eq!(
            settle_pointer_journey(10, 10, 10, 10, true),
            WinSettle::Cancelled
        );
        assert_eq!(
            settle_pointer_journey(10, 10, 10, 12, true),
            WinSettle::Cancelled
        );
        assert_eq!(
            settle_pointer_journey(10, 10, 14, 10, false),
            WinSettle::Drop { x: 14, y: 10 }
        );
    }

    #[test]
    fn aborted_down_passes_up_through_with_no_strand() {
        // A Down that armed but could not queue (saturation) carries no
        // swallow obligation: the Up passes through natively, so no
        // half-click strands and no queued owner edge follows.
        let mut arm = WinHookArm::default();
        assert!(arm.on_down(7, 1, 1, true, true));
        arm.abort_down();
        assert!(!arm.is_armed());
        assert_eq!(arm.on_up(1, 1), UpOutcome::Pass);
        assert_eq!(arm.on_up(1, 1), UpOutcome::Pass);
    }
}

/// Prompt `WH_MOUSE_LL` hook for the project-driven Win+Left gesture
/// (`cfg(windows)` only). Mirrors the accepted keyboard-hook discipline:
/// the callback classifies one mouse event, pushes at most one bounded
/// record, and never touches the Engine, geometry, logs, or blocking
/// identity probes. The owner drains edges and revalidates every bound
/// candidate against fresh observation before any effect or focus.
#[cfg(windows)]
pub mod sys {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::sync::Arc;
    use windows_sys::Win32::Foundation::{HMODULE, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GA_ROOT, GetAncestor, HC_ACTION, HHOOK, MSLLHOOKSTRUCT, SetWindowsHookExW,
        UnhookWindowsHookEx, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
        WindowFromPoint,
    };

    use super::{
        UpOutcome, WINDRAG_MAX_DISPATCH_PER_TICK, WinDragEdge, WinDragKind, WinDragQueue,
        WinDragSnapshot, WinHookArm,
    };
    use crate::input_handoff::{
        SharedSnapshot, WindragOrigin, mouse_fresh_gate, record_delivery_gap_ms,
    };
    use crate::snapkey::VK_LWIN;
    use crate::snapkey::VK_RWIN;

    /// Published tiled origins plus member tags. Unified with the handoff
    /// [`WindragOrigin`] so snapshots cross threads without conversion.
    pub use crate::input_handoff::WindragOrigin as WinDragPublished;

    /// Either Win side held right now, sampled from the official async
    /// state. Sampling (not tracked classifier state) supports synthetic
    /// holds: a SendInput Win down sets the async level even though the
    /// keyboard classifier filters injected keys. Extras are ignored: only
    /// Win gates the gesture, preserving the exact supported Win+Left
    /// behavior with no invented extra-modifier restriction.
    ///
    /// Test seam: `test_set_win_held(Some(_))` overrides the native sample
    /// so callback freshness tests drive the production `llproc` with no
    /// `GetAsyncKeyState` or input calls. `None` (production and default)
    /// samples natively.
    fn win_held_now() -> bool {
        #[cfg(test)]
        if let Some(held) = TEST_WIN_HELD.with(|s| *s.borrow()) {
            return held;
        }
        (unsafe { GetAsyncKeyState(VK_LWIN as i32) }) < 0
            || (unsafe { GetAsyncKeyState(VK_RWIN as i32) }) < 0
    }

    /// Root top-level HWND under the pointer. `WindowFromPoint` may hit a
    /// child; the owner manages top-level windows, so the `GA_ROOT`
    /// ancestor is the bound subject. Pure reads, no identity probes.
    ///
    /// Test seam: same contract as [`win_held_now`]; `Some(root)` fixes the
    /// bound subject for the production `llproc` drive.
    fn root_at(x: i32, y: i32) -> Option<u64> {
        #[cfg(test)]
        if let Some(root) = TEST_ROOT.with(|s| *s.borrow()) {
            return root;
        }
        let pt = windows_sys::Win32::Foundation::POINT { x, y };
        let hit = unsafe { WindowFromPoint(pt) };
        if hit.is_null() {
            return None;
        }
        let root = unsafe { GetAncestor(hit, GA_ROOT) };
        let hwnd = if root.is_null() { hit } else { root };
        let raw = hwnd as usize as u64;
        if raw == 0 { None } else { Some(raw) }
    }

    #[cfg(test)]
    thread_local! {
        static TEST_WIN_HELD: RefCell<Option<bool>> = const { RefCell::new(None) };
        static TEST_ROOT: RefCell<Option<Option<u64>>> = const { RefCell::new(None) };
    }

    /// Test-only native-query override for [`win_held_now`]: `Some(held)`
    /// fixes the sample, `None` restores native sampling.
    #[cfg(test)]
    pub fn test_set_win_held(held: Option<bool>) {
        TEST_WIN_HELD.with(|s| *s.borrow_mut() = held);
    }

    /// Test-only native-query override for [`root_at`]: `Some(root)` fixes
    /// the bound subject, `None` restores native reads.
    #[cfg(test)]
    pub fn test_set_root(root: Option<Option<u64>>) {
        TEST_ROOT.with(|s| *s.borrow_mut() = root);
    }

    struct WinMouseHookState {
        /// Applied owner snapshot shared by pointer with the keyboard hook
        /// state: tiled-only managed origins plus the takeover gate. Swapped
        /// on the pump thread outside callbacks; callbacks only read one
        /// entry through the `Arc`. Floating, sticky, unmanaged, and unknown
        /// subjects pass through natively.
        snapshot: SharedSnapshot,
        /// Keyboard hook installed alongside. Fresh downs consume only with
        /// both hooks installed, since a missing keyboard cannot send the
        /// Start-menu mask at release.
        keyboard_installed: bool,
        queue: WinDragQueue,
        arm: WinHookArm,
        /// Callback delivery counters (counts only, no identity): observed
        /// downs/ups plus consumed downs and swallowed-or-gesture ups. The
        /// owner logs them change-only, splitting hook delivery from owner
        /// validation in live evidence.
        downs_seen: u64,
        downs_consumed: u64,
        ups_seen: u64,
        ups_consumed: u64,
    }

    thread_local! {
        static STATE: RefCell<Option<WinMouseHookState>> = const { RefCell::new(None) };
    }

    /// Owner publish: tiled-only origin/tag map plus the takeover gate.
    /// Cheap clone of the last observation's managed set; exactness stays
    /// with the owner's per-edge recheck against fresh evidence.
    pub fn publish(origins: &HashMap<u64, WindragOrigin>, active: bool) {
        STATE.with(|s| {
            if let Some(st) = s.borrow_mut().as_mut() {
                let mut snapshot = (*st.snapshot).clone();
                snapshot.windrag_origins = origins.clone();
                snapshot.active = active;
                st.snapshot = Arc::new(snapshot);
            }
        });
    }

    /// Owner drain: take up to `limit` queued edges, oldest first.
    pub fn drain_up_to(limit: usize) -> Vec<WinDragEdge> {
        STATE.with(|s| {
            let mut out = Vec::new();
            if let Some(st) = s.borrow_mut().as_mut() {
                let take = limit.max(1).min(WINDRAG_MAX_DISPATCH_PER_TICK.max(1));
                while out.len() < take {
                    match st.queue.pop_front() {
                        Some(edge) => out.push(edge),
                        None => break,
                    }
                }
            }
            out
        })
    }

    /// Last published queue loss count, for explicit drop evidence.
    pub fn queue_dropped() -> u32 {
        STATE.with(|s| s.borrow().as_ref().map(|st| st.queue.dropped).unwrap_or(0))
    }

    /// Callback delivery counters `(downs_seen, downs_consumed, ups_seen,
    /// ups_consumed)`: counts only, no identity. The owner logs them
    /// change-only beside the drain, so live evidence splits hook delivery
    /// (the OS delivered the click to the callback) from owner validation
    /// (the bound candidate passed fresh gates). Zeros when the hook is
    /// not installed.
    pub fn hook_stats() -> (u64, u64, u64, u64) {
        STATE.with(|s| {
            s.borrow().as_ref().map_or((0, 0, 0, 0), |st| {
                (
                    st.downs_seen,
                    st.downs_consumed,
                    st.ups_seen,
                    st.ups_consumed,
                )
            })
        })
    }

    /// Clear the hook arm when it names `hwnd` (owner-side invalidation):
    /// identity, revision, suspension, or destruction loss disarms with no
    /// replay and queues a Cancel so the owner clears deterministically.
    /// The swallow obligation is retained so the paired Up never reaches
    /// the app after a consumed Down.
    pub fn invalidate(hwnd: u64) {
        STATE.with(|s| {
            let mut borrow = s.borrow_mut();
            let Some(st) = borrow.as_mut() else {
                return;
            };
            if st.arm.invalidate(hwnd) {
                st.queue.push_terminal(WinDragEdge {
                    hwnd,
                    kind: WinDragKind::Cancel,
                    x: 0,
                    y: 0,
                    esc_seq: crate::snapkey::sys::shared_windrag_esc_seq(),
                    snapshot: None,
                });
            }
        });
    }

    /// Ordered windrag invalidation from the input pump (FIFO between
    /// snapshots): disarms with Cancel like [`invalidate`] and drops the
    /// published origin so later Downs cannot rebind until a fresh snapshot
    /// republishes it. Clone-on-write on the shared map (invalidations are
    /// rare); the swallow obligation is retained.
    pub fn apply_invalidate(hwnd: u64) {
        STATE.with(|s| {
            let mut borrow = s.borrow_mut();
            let Some(st) = borrow.as_mut() else {
                return;
            };
            if st.snapshot.windrag_origins.contains_key(&hwnd) {
                let mut snapshot = (*st.snapshot).clone();
                snapshot.windrag_origins.remove(&hwnd);
                st.snapshot = Arc::new(snapshot);
            }
            if st.arm.invalidate(hwnd) {
                st.queue.push_terminal(WinDragEdge {
                    hwnd,
                    kind: WinDragKind::Cancel,
                    x: 0,
                    y: 0,
                    esc_seq: crate::snapkey::sys::shared_windrag_esc_seq(),
                    snapshot: None,
                });
            }
        });
    }

    /// Suspend/teardown path only: drop all published origins and disarm
    /// with Cancel like [`clear_armed`]. Held-pair swallow obligations are
    /// retained; no new Down binds until a fresh snapshot.
    pub fn apply_clear() {
        STATE.with(|s| {
            let mut borrow = s.borrow_mut();
            let Some(st) = borrow.as_mut() else {
                return;
            };
            if !st.snapshot.windrag_origins.is_empty() {
                let mut snapshot = (*st.snapshot).clone();
                snapshot.windrag_origins.clear();
                st.snapshot = Arc::new(snapshot);
            }
            let hwnd = st.arm.armed_hwnd();
            if st.arm.clear() {
                st.queue.push_terminal(WinDragEdge {
                    hwnd: hwnd.unwrap_or(0),
                    kind: WinDragKind::Cancel,
                    x: 0,
                    y: 0,
                    esc_seq: crate::snapkey::sys::shared_windrag_esc_seq(),
                    snapshot: None,
                });
            }
        });
    }

    /// Apply one whole owner snapshot outside callbacks (input pump only).
    /// Swaps the shared `Arc`: no map clones on the pump, no conversion.
    /// `keyboard_installed` gates fresh consumption alongside the snapshot
    /// gate, since a missing keyboard cannot send the mask. Exactness stays
    /// with the per-edge owner recheck.
    pub fn apply_snapshot(snap: &SharedSnapshot, keyboard_installed: bool) {
        STATE.with(|s| {
            if let Some(st) = s.borrow_mut().as_mut() {
                st.snapshot = Arc::clone(snap);
                st.keyboard_installed = keyboard_installed;
            }
        });
    }

    /// Clear any hook arm (suspend/teardown path only). Like
    /// [`invalidate`]: disarms with no replay, queues a Cancel for the
    /// owner, and retains the swallow obligation.
    pub fn clear_armed() {
        STATE.with(|s| {
            let mut borrow = s.borrow_mut();
            let Some(st) = borrow.as_mut() else {
                return;
            };
            let hwnd = st.arm.armed_hwnd();
            if st.arm.clear() {
                st.queue.push_terminal(WinDragEdge {
                    hwnd: hwnd.unwrap_or(0),
                    kind: WinDragKind::Cancel,
                    x: 0,
                    y: 0,
                    esc_seq: crate::snapkey::sys::shared_windrag_esc_seq(),
                    snapshot: None,
                });
            }
        });
    }

    /// Minimal callback: one event in, at most one bounded queue push out.
    /// No Engine, geometry, or log I/O here. Consumed downs/ups return
    /// nonzero without calling the next hook (no app click, no stranded
    /// half-click); everything else passes through. Moves always pass
    /// (only the armed latest pointer updates). Both physical and
    /// synthetic (unmarked, same as the native caption proof path) mouse
    /// input binds identically; no physical-mask claim is made here.
    unsafe extern "system" fn llproc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code as u32 != HC_ACTION {
            return unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
        }
        // `MSLLHOOKSTRUCT` (stable Win32 ABI): the release/down point rides
        // the event itself, so no later re-read can tear it.
        let hook = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
        let x = hook.pt.x;
        let y = hook.pt.y; // Both physical and synthetic (unmarked, same as the native caption
        // proof path) mouse input binds identically; no physical-mask claim
        // is made here. Injected Win-ups never reach the keyboard reserve,
        // so synthetic holds never send the product mask.
        let _flags = hook.flags;
        match wparam as u32 {
            WM_LBUTTONDOWN => {
                // Trace-only native delivery gap: native event timestamp to
                // callback entry, tick-count domain. Moves skip this
                // entirely; no timing work unless tracing.
                if STATE.with(|s| s.borrow().as_ref().is_some_and(|st| st.snapshot.trace)) {
                    let gap = unsafe { GetTickCount() }.saturating_sub(hook.time);
                    record_delivery_gap_ms(gap);
                }
                let consume = STATE.with(|s| {
                    let mut borrow = s.borrow_mut();
                    let st = borrow.as_mut()?;
                    st.downs_seen += 1;
                    // Both hooks installed plus the snapshot gate: a missing
                    // keyboard cannot send the Start-menu mask, so fresh
                    // downs pass through until it recovers. The publication
                    // fence closes the gate further while a newer policy is
                    // published but not yet applied, so fresh Downs refuse
                    // instead of consuming under stale policy; the paired Up
                    // of a consumed Down still swallows via the arm.
                    if !mouse_fresh_gate(st.snapshot.active, st.keyboard_installed) {
                        return None;
                    }
                    if !win_held_now() {
                        return None;
                    }
                    let hwnd = root_at(x, y)?;
                    let published = st.snapshot.windrag_origins.get(&hwnd)?.clone();
                    if st.queue.is_full() {
                        // Saturated Down passes through with nothing armed:
                        // no gesture, no strand, counted loss.
                        st.queue.note_drop();
                        return None;
                    }
                    if !st.arm.on_down(hwnd, x, y, true, true) {
                        return None;
                    }
                    // Down-time bound snapshot: the published origin plus
                    // the member tag, cloned here (no native identity
                    // probes). The owner requires it to match fresh
                    // evidence before arming, so an HWND reuse between this
                    // callback and the owner drain cannot rebind.
                    let edge = WinDragEdge {
                        hwnd,
                        kind: WinDragKind::Down,
                        x,
                        y,
                        esc_seq: crate::snapkey::sys::shared_windrag_esc_seq(),
                        snapshot: Some(WinDragSnapshot {
                            origin: published.origin.clone(),
                            tag: published.tag.clone(),
                        }),
                    };
                    if !st.queue.push(edge) {
                        // Unreachable: capacity was pre-checked above on
                        // this same thread. Abort without swallow so the
                        // passed-through Down keeps its native Up.
                        st.arm.abort_down();
                        st.queue.note_drop();
                        return None;
                    }
                    st.downs_consumed += 1;
                    Some(())
                });
                if consume.is_some() {
                    // The Win hold still needs its Start-menu mask at
                    // release: arm the shared keyboard reserve on the same
                    // thread. Injected Win-ups never reach the reserve, so
                    // synthetic holds never send the product mask.
                    crate::snapkey::sys::arm_windrag_mask();
                    return 1;
                }
                unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
            }
            WM_MOUSEMOVE => {
                STATE.with(|s| {
                    if let Some(st) = s.borrow_mut().as_mut() {
                        st.arm.on_move(x, y);
                    }
                });
                unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
            }
            WM_LBUTTONUP => {
                // Trace-only native delivery gap for the terminal edge, same
                // contract as the Down path above.
                if STATE.with(|s| s.borrow().as_ref().is_some_and(|st| st.snapshot.trace)) {
                    let gap = unsafe { GetTickCount() }.saturating_sub(hook.time);
                    record_delivery_gap_ms(gap);
                }
                // The paired Up is always swallowed once a Down was
                // consumed, even when saturated: terminal edges use the
                // guaranteed push, so a lost Up can never leave an owner
                // gesture armed forever, and a disarmed pairing still never
                // reaches the app as a half-click.
                let consume = STATE.with(|s| {
                    let mut borrow = s.borrow_mut();
                    let st = borrow.as_mut()?;
                    st.ups_seen += 1;
                    match st.arm.on_up(x, y) {
                        UpOutcome::Gesture(bound) => {
                            st.queue.push_terminal(WinDragEdge {
                                hwnd: bound.hwnd,
                                kind: WinDragKind::Up,
                                x: bound.cur_x,
                                y: bound.cur_y,
                                esc_seq: crate::snapkey::sys::shared_windrag_esc_seq(),
                                snapshot: None,
                            });
                            st.ups_consumed += 1;
                            Some(())
                        }
                        UpOutcome::Swallowed => {
                            st.ups_consumed += 1;
                            Some(())
                        }
                        UpOutcome::Pass => None,
                    }
                });
                if consume.is_some() {
                    return 1;
                }
                unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
            }
            _ => unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) },
        }
    }

    /// Install the low-level mouse hook on the calling thread. The caller
    /// must pump messages so the callback runs. Fails closed like the
    /// keyboard hook.
    pub fn install() -> std::result::Result<HHOOK, String> {
        STATE.with(|s| {
            *s.borrow_mut() = Some(WinMouseHookState {
                snapshot: Arc::new(crate::input_handoff::InputSnapshot {
                    generation: 0,
                    keyboard: crate::snapkey::KeyboardConfig::disabled(),
                    remap: Vec::new(),
                    disabled: Vec::new(),
                    snap_origins: HashMap::new(),
                    windrag_origins: HashMap::new(),
                    active: false,
                    trace: false,
                }),
                keyboard_installed: false,
                queue: WinDragQueue::new(),
                arm: WinHookArm::default(),
                downs_seen: 0,
                downs_consumed: 0,
                ups_seen: 0,
                ups_consumed: 0,
            });
        });
        let hmod: HMODULE = unsafe { GetModuleHandleW(std::ptr::null()) };
        let hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(llproc), hmod, 0) };
        if hook.is_null() {
            STATE.with(|s| *s.borrow_mut() = None);
            return Err("error: SetWindowsHookExW(WH_MOUSE_LL) failed".to_owned());
        }
        Ok(hook)
    }

    /// Release the hook. Process exit releases it in any case, but success
    /// is never fabricated. State mutates only on success: a failed unhook
    /// retains arm/swallow obligations for a creator-side retry.
    pub fn uninstall(hook: &mut HHOOK) -> bool {
        let ok = unsafe { UnhookWindowsHookEx(*hook) } != 0;
        if !ok {
            return false;
        }
        *hook = std::ptr::null_mut();
        STATE.with(|s| *s.borrow_mut() = None);
        true
    }

    /// Test-only keyboard-presence probe (no hooks, no native calls): the
    /// gate fresh Downs require alongside the snapshot gate.
    #[cfg(test)]
    pub fn test_keyboard_installed() -> bool {
        STATE.with(|s| s.borrow().as_ref().is_some_and(|st| st.keyboard_installed))
    }

    /// Test-only applied-snapshot handle (no hooks, no native calls).
    #[cfg(test)]
    pub fn test_snapshot() -> SharedSnapshot {
        STATE.with(|s| {
            s.borrow()
                .as_ref()
                .map(|st| Arc::clone(&st.snapshot))
                .expect("test state installed")
        })
    }

    /// Test-only hook-state installer (no `SetWindowsHookExW`, no native
    /// effects): mirrors [`install`](Self::install)'s TLS init so step tests
    /// drive the production apply/drain logic without hooks.
    #[cfg(test)]
    pub fn test_ensure_state() {
        STATE.with(|s| {
            *s.borrow_mut() = Some(WinMouseHookState {
                snapshot: Arc::new(crate::input_handoff::InputSnapshot {
                    generation: 0,
                    keyboard: crate::snapkey::KeyboardConfig::disabled(),
                    remap: Vec::new(),
                    disabled: Vec::new(),
                    snap_origins: HashMap::new(),
                    windrag_origins: HashMap::new(),
                    active: false,
                    trace: false,
                }),
                keyboard_installed: false,
                queue: WinDragQueue::new(),
                arm: WinHookArm::default(),
                downs_seen: 0,
                downs_consumed: 0,
                ups_seen: 0,
                ups_consumed: 0,
            });
        });
    }

    /// Test-only state teardown (no native calls).
    #[cfg(test)]
    pub fn test_clear_state() {
        STATE.with(|s| *s.borrow_mut() = None);
    }

    /// Test-only edge injection (no hooks, no native calls): push one edge
    /// straight into the hook queue for drain/offer tests.
    #[cfg(test)]
    pub fn test_push_edge(edge: WinDragEdge) {
        STATE.with(|s| {
            if let Some(st) = s.borrow_mut().as_mut() {
                st.queue.push(edge);
            }
        });
    }

    /// Test-only production-callback drive (no hooks, no native input
    /// calls): feeds one fabricated mouse button event through the real
    /// `llproc`. The Win-hold sample and the bound subject come from the
    /// [`test_set_win_held`]/[`test_set_root`] seams; callers must fix both
    /// before driving a Down (an unset seam would perform native reads).
    /// Up/Down outcomes are the production verdicts (`1` = consumed or
    /// swallowed, `CallNextHookEx` result = pass-through).
    ///
    /// # Safety
    ///
    /// Test-only: the fabricated `MSLLHOOKSTRUCT` lives on the stack for
    /// the synchronous call; the callback only reads it and passes it to
    /// `CallNextHookEx` (no hook installed, no effects).
    #[cfg(test)]
    pub unsafe fn test_drive_mouse(
        wparam: u32,
        x: i32,
        y: i32,
    ) -> windows_sys::Win32::Foundation::LRESULT {
        use windows_sys::Win32::Foundation::POINT;
        let hook = MSLLHOOKSTRUCT {
            pt: POINT { x, y },
            mouseData: 0,
            flags: 0,
            time: 0,
            dwExtraInfo: 0,
        };
        unsafe {
            llproc(
                HC_ACTION as i32,
                wparam as usize,
                &hook as *const MSLLHOOKSTRUCT as isize,
            )
        }
    }

    #[cfg(test)]
    mod sys_tests {
        use super::super::WinDragKind;
        use super::*;
        use crate::input_handoff::{
            InputSnapshot, SharedSnapshot, WindragOrigin, fence_test_allocate, fence_test_serial,
            mouse_fresh_gate, note_policy_applied, note_policy_published, policy_fence_pending,
        };
        use crate::snapkey::{KeyboardConfig, SnapOrigin};
        use std::collections::HashMap;
        use std::sync::Arc;

        fn snapshot_with_origins(hwnds: &[u64]) -> SharedSnapshot {
            let mut snap = InputSnapshot {
                generation: 0,
                keyboard: KeyboardConfig {
                    takeover: true,
                    allow_win_l: false,
                },
                remap: Vec::new(),
                disabled: Vec::new(),
                snap_origins: HashMap::new(),
                windrag_origins: HashMap::new(),
                active: true,
                trace: false,
            };
            for hwnd in hwnds {
                let origin = SnapOrigin {
                    hwnd: *hwnd,
                    token: format!("token-{hwnd}"),
                    pid: 1,
                    creation: "c".to_owned(),
                };
                snap.snap_origins.insert(*hwnd, origin.clone());
                snap.windrag_origins.insert(
                    *hwnd,
                    WindragOrigin {
                        origin,
                        tag: Some(format!("tag-{hwnd}")),
                    },
                );
            }
            Arc::new(snap)
        }

        fn install_both(snap: &SharedSnapshot, keyboard_installed: bool) {
            crate::snapkey::sys::test_ensure_state(KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            });
            test_ensure_state();
            crate::snapkey::sys::apply_snapshot(snap, true);
            apply_snapshot(snap, keyboard_installed);
        }

        fn clear_both() {
            test_set_win_held(None);
            test_set_root(None);
            crate::snapkey::sys::test_clear_state();
            test_clear_state();
        }

        #[test]
        fn mouse_down_needs_keyboard_present_then_consumes_with_mask() {
            // Keyboard-absent fresh Down passes (no mask source); once the
            // keyboard installs, the same Down consumes through the real
            // callback and arms the shared Win-up mask obligation. Serial
            // guard: the Down verdict reads the process-global fence, so a
            // concurrent pending fence would flake the gate-open assertions.
            let _guard = fence_test_serial();
            let snap = snapshot_with_origins(&[7]);
            install_both(&snap, false);
            // Tracked physical Win hold so the mask arm can latch (no
            // `SendInput`: only the classifier tracks the down).
            unsafe {
                crate::snapkey::sys::test_drive_llproc(crate::snapkey::VK_LWIN, 0, 0, 0);
            }
            test_set_win_held(Some(true));
            test_set_root(Some(Some(7)));
            assert!(!mouse_fresh_gate(true, false));
            let verdict = unsafe { test_drive_mouse(WM_LBUTTONDOWN, 10, 20) };
            assert_ne!(verdict, 1);
            assert!(drain_up_to(8).is_empty());
            // Keyboard recovers: the gate reopens and the Down consumes.
            apply_snapshot(&snap, true);
            assert!(mouse_fresh_gate(true, true));
            let verdict = unsafe { test_drive_mouse(WM_LBUTTONDOWN, 10, 20) };
            assert_eq!(verdict, 1);
            assert!(crate::snapkey::sys::test_mask_pending());
            let edges = drain_up_to(8);
            assert_eq!(edges.len(), 1);
            assert_eq!(edges[0].kind, WinDragKind::Down);
            assert_eq!(edges[0].hwnd, 7);
            clear_both();
        }

        #[test]
        fn mouse_fence_refuses_fresh_down_but_swallows_consumed_up() {
            // Real-callback freshness: a consumed Down's paired Up stays
            // swallowed while the fence is pending, fresh Downs pass, and
            // the mask obligation survives until the pump applies.
            let _guard = fence_test_serial();
            let base = fence_test_allocate();
            note_policy_applied(base);
            let snap = snapshot_with_origins(&[7]);
            install_both(&snap, true);
            unsafe {
                crate::snapkey::sys::test_drive_llproc(crate::snapkey::VK_LWIN, 0, 0, 0);
            }
            test_set_win_held(Some(true));
            test_set_root(Some(Some(7)));
            let verdict = unsafe { test_drive_mouse(WM_LBUTTONDOWN, 10, 20) };
            assert_eq!(verdict, 1);
            assert!(crate::snapkey::sys::test_mask_pending());
            // Newer policy publishes before the pump applies: fresh refuses
            // fail-closed while the in-flight pair rides its verdict.
            note_policy_published(base + 1);
            assert!(policy_fence_pending());
            assert!(!mouse_fresh_gate(true, true));
            let verdict = unsafe { test_drive_mouse(WM_LBUTTONUP, 11, 21) };
            assert_eq!(verdict, 1);
            // The consumed pair drained in order behind the fence.
            let pair = drain_up_to(8);
            assert_eq!(pair.len(), 2);
            assert_eq!(pair[0].kind, WinDragKind::Down);
            assert_eq!(pair[1].kind, WinDragKind::Up);
            // The next Down is fresh under the fence: passes through, never
            // converted into a swallow, nothing queued behind the pair.
            test_set_root(Some(Some(7)));
            let verdict = unsafe { test_drive_mouse(WM_LBUTTONDOWN, 10, 20) };
            assert_ne!(verdict, 1);
            assert!(crate::snapkey::sys::test_mask_pending());
            assert!(drain_up_to(8).is_empty());
            // Pump applies: fresh consumption resumes.
            note_policy_applied(base + 1);
            assert!(!policy_fence_pending());
            assert!(mouse_fresh_gate(true, true));
            clear_both();
        }
    }
}
