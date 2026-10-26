#![forbid(unsafe_code)]

//! Pinned Dock-swipe native Space selector (product select route).
//!
//! All product native Space selection (numbered `1..9`, relative, previous
//! toggle, trailing existing and post-create select, plus the numbered
//! send-stay released return) dispatches through
//! [`execute_dock_swipe_select`]: the pinned-yabai high-velocity
//! session-tap Dock swipe (`space_manager_focus_space_using_gesture` at
//! yabai `dd84572` `src/space_manager.c:927-983`: `abs(target-current)`
//! immediate began/ended pairs, fields `55=30`, `110=23`, `123=1`,
//! `132=1` then `4`, doubles `124=sign`, `129=sign*9999`,
//! `kCGSessionEventTap`, no `Changed`, no sleeps, no interstep waits,
//! async `SPACE_CHANGED` settlement). Same-display only: no warps, no
//! clicks. The rift `>=27` augmented payload is never blindly used (the
//! existing [`crate::space_switch_edges::gesture_payload_allowed`]
//! refusal stays).
//!
//! Threading: every fresh proof, post, and settlement runs OFF the engine
//! and AppKit threads on one serialized selection owner (see
//! [`crate::select_owner`]). Main submission/completion never waits.
//!
//! Settlement is event-driven on the exact SID over one per-job
//! [`SpaceSelectLatch`](crate::ffi::SpaceSelectLatch): the loop reads the
//! fresh native current on EVERY notification until the exact target, one
//! absolute deadline, or cancellation. Intermediate and non-target
//! notifications are normal (nonadjacent swipes cross Spaces that each
//! notify); only the deadline takes a final read. No periodic polling,
//! no MC fallback/replay after uncertain gestures, no visual or
//! input-readiness inference. Refusal/unproven output preserves history,
//! releases `Ended` when `Began` posted, and frees the lane for fresh
//! requests.
//!
//! Own-switch provenance: every gesture event carries
//! [`crate::input_match::OWN_SWITCH_USER_DATA`] as `EventSourceUserData`
//! (see [`crate::space_switch_edges::gesture_event`]). The tap observes key
//! chords only, so gesture events pass by construction; the tag is parity
//! plus diagnostic attribution.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::ffi::{LatchWait, SpaceSelectLatch};
use crate::space_switch_edges::{GesturePairsOutcome, gesture_payload_allowed, swipe_plan};
use crate::workspace_select::{DispatchError, SelectGates, SelectionBackend, SelectionSnapshot};

/// Absolute arrival deadline for one Dock-swipe settlement (ms). Async
/// `SPACE_CHANGED` arrival needs a bound and 3 s is the proven window.
/// Production passes this; tests inject short deadlines.
pub const SELECT_ARRIVAL_DEADLINE_MS: u64 = 3000;

/// Bound for one post-create Mission Control exit wait (ms): the existing
/// MC close poll budget ([`crate::mc_probe::MC_GROUP_POLL_MS`], Hammerspoon
/// 2 s parity) — the genuine bound that keeps a wedged Dock from stalling
/// the select owner (and its shutdown drain). Not stabilization: the gate
/// releases on the actual Dock `AXExposeExit` delivery, retained in the
/// latch generation even when it fires before the create leg's close
/// returns.
pub const MC_EXIT_WAIT_MS: u64 = crate::mc_probe::MC_GROUP_POLL_MS;

/// One runloop pump slice (ms) while awaiting the exit: delivery vehicle
/// only (the owner thread must run its runloop for the observer callback
/// to fire). Short enough to answer cancellation promptly; the total wait
/// stays bounded by [`MC_EXIT_WAIT_MS`].
pub const MC_EXIT_PUMP_MS: u64 = 50;

/// Fresh native full-order row for one display: the raw SLS order (all
/// Space kinds, including fullscreen the traversal crosses — the ordinal
/// mapping the WindowServer walks is preserved exactly), the derived user
/// row the logical mapping resolves against, the current Space, and the
/// snapshot display identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullRow {
    pub display_id: String,
    pub full_order: Vec<u64>,
    pub user_spaces: Vec<u64>,
    pub current: u64,
}

/// Injectable fresh-topology seam for the Dock-swipe executor. Every method
/// is one fresh read set on the calling (owner) thread: SLS rows plus type
/// reads plus the pinned display-identity join over main-published plain
/// data — never an AppKit snapshot call.
pub trait SwipeTopology: Send {
    /// Fresh native display-row count (product single-display rule).
    fn display_count(&mut self) -> Result<usize, String>;
    /// Fresh full-order row for the pinned display (stale publications
    /// refuse here, never guessed).
    fn fresh_full_row(&mut self) -> Result<FullRow, String>;
    /// Live topology revision backing the current row (`None` when the
    /// seam cannot report one — bound-claim fencing then rests on the SID
    /// leg alone).
    fn revision(&mut self) -> Option<u64> {
        None
    }
}

/// Posted-pair counts plus the pending-release sign when a `began` posted
/// without its `ended`, plus the underlying refusal text when a pair
/// refused.
#[derive(Debug, Clone, PartialEq)]
pub struct PairsPosted {
    pub began_posted: usize,
    pub ended_posted: usize,
    pub pending_sign: Option<f64>,
    pub error: Option<String>,
}

/// Injectable gesture-post seam. Production ([`LiveSwipePoster`]) posts
/// through the session tap with the pinned 6-field branch; tests script
/// every leg and never touch the WindowServer.
pub trait SwipePoster: Send {
    /// Post `pairs` immediate began/ended pairs for `sign`.
    fn post_pairs(&mut self, sign: f64, pairs: usize) -> PairsPosted;
    /// Post exactly one pending release edge (`ended`) for `sign`.
    fn release(&mut self, sign: f64) -> Result<(), String>;
}

/// Injectable live-gate seam: fresh session reads on the calling (owner)
/// thread immediately before an effect. Production ([`LiveGateReads`])
/// reads the live session; tests script every leg. Stale main-submit
/// gates are never authoritative here.
pub trait GateReads: Send {
    fn read_gates(&mut self) -> SelectGates;
}

/// Production gates: session lock plus Secure Input plus loginwindow
/// foreground, read fresh on the owner thread. Any-thread C calls plus
/// the reviewed model-object frontmost read (never AppKit layout).
pub struct LiveGateReads;

impl GateReads for LiveGateReads {
    fn read_gates(&mut self) -> SelectGates {
        SelectGates {
            session_unlocked: matches!(
                crate::ffi::session_state().lock,
                crate::ffi::SessionLock::Unlocked
            ),
            secure_active: matches!(crate::ffi::secure_input_known(), Some(true)),
            lock_screen: crate::ffi::frontmost_app()
                .map(|(_, bundle)| crate::focus::is_loginwindow_bundle(&bundle))
                .unwrap_or(true),
        }
    }
}

/// Injectable post-create Mission Control exit gate: the Dock
/// `AXExposeExit` contract (yabai `dd84572` `src/mission_control.c`) that
/// a post-create select must observe before dispatching any gesture.
/// Production ([`LiveMcExitGate`]) arms a thread-affine
/// [`crate::ffi::DockExitWatch`] on the owner thread; tests script every
/// leg and never touch the Dock.
pub(crate) trait McExitGate {
    /// Register Dock `AXExposeExit` BEFORE the create leg opens Mission
    /// Control, so an exit delivered any time after registration advances
    /// it, even before the create leg's close returns.
    /// Failure refuses pre-effect: nothing appended, nothing to adopt.
    fn arm(&mut self) -> Result<(), DispatchError>;
    /// Wait (pumping the owner runloop for deliveries) for the armed exit,
    /// cancellation, or the genuine [`MC_EXIT_WAIT_MS`] bound. One-shot:
    /// disarms before returning on every path. A post-append failure
    /// preserves the append — the shared `execute` already captured the
    /// proof before `select` runs, so returning `Err` here dispatches no
    /// gesture while keeping ownership adoptable: never a silent loss,
    /// never an automatic replay.
    fn await_exit(&mut self, cancel: &AtomicBool) -> Result<(), DispatchError>;
    /// Idempotent teardown for the armed-but-never-awaited path (the
    /// create leg failed, so no select follows).
    fn disarm(&mut self);
}

/// Production post-create MC-exit gate over a thread-affine
/// [`crate::ffi::DockExitWatch`]: armed on the select-owner thread before
/// the pinned create, awaited (pumping that same thread's runloop) at the
/// top of the post-create select. Normal existing selects never arm it
/// and pay no wait, open no observer, and traverse no Mission Control.
pub(crate) struct LiveMcExitGate {
    watch: Option<crate::ffi::DockExitWatch>,
    latch: crate::ffi::SpaceSelectLatch,
    from: u64,
    wait_ms: u64,
}

impl LiveMcExitGate {
    pub(crate) fn new() -> Self {
        let latch = crate::ffi::SpaceSelectLatch::detached();
        let from = latch.generation();
        Self {
            watch: None,
            latch,
            from,
            wait_ms: MC_EXIT_WAIT_MS,
        }
    }
}

impl McExitGate for LiveMcExitGate {
    fn arm(&mut self) -> Result<(), DispatchError> {
        self.disarm();
        let pid = crate::mc_lifecycle::live::dock_pid().map_err(|e| {
            DispatchError::refused(format!("exit watch refused: Dock PID unprovable ({e})"))
        })?;
        let bundle = crate::ffi::bundle_id_for_pid(pid).unwrap_or_default();
        crate::mc_probe::require_dock_bundle(&bundle)
            .map_err(|e| DispatchError::refused(format!("exit watch refused: {e}")))?;
        // Fresh latch per arm: the generation below is the exact retain
        // point — an exit delivered any time after registration advances
        // it, even before the create leg's close returns.
        self.latch = crate::ffi::SpaceSelectLatch::detached();
        self.from = self.latch.generation();
        let watch = crate::ffi::DockExitWatch::arm(
            pid,
            self.latch.clone(),
            crate::mc_probe::MC_AX_TIMEOUT_SECS,
        )
        .map_err(|e| DispatchError::refused(format!("exit watch refused pre-effect: {e}")))?;
        self.watch = Some(watch);
        Ok(())
    }

    fn await_exit(&mut self, cancel: &AtomicBool) -> Result<(), DispatchError> {
        let result = self.await_exit_inner(cancel);
        self.disarm();
        result
    }

    fn disarm(&mut self) {
        if let Some(mut watch) = self.watch.take() {
            watch.disarm();
        }
    }
}

impl LiveMcExitGate {
    fn await_exit_inner(&self, cancel: &AtomicBool) -> Result<(), DispatchError> {
        if self.watch.is_none() {
            return Err(DispatchError::refused(
                "exit gate awaited without an armed watch; refusing",
            ));
        }
        let deadline = Instant::now() + Duration::from_millis(self.wait_ms);
        loop {
            // Retained-signal fast path: an exit delivered before the
            // create leg's close returned already advanced the latch — no
            // wait, no gesture delay.
            if self.latch.generation() != self.from {
                return Ok(());
            }
            if cancel.load(Ordering::SeqCst) || self.latch.cancelled() {
                // Post-arm cancellation: the append is verified and retained
                // with proof while nothing is dispatched. MC exit is
                // unproven, so this holds (never a proven refusal).
                return Err(DispatchError::threw(
                    "trailing post-create select unproven: cancelled while awaiting Mission Control exit; created Space retained with proof; no gesture dispatched",
                ));
            }
            if Instant::now() >= deadline {
                // Wedged-Dock guard: the append is verified and retained
                // with proof; dispatching nothing is a post-actuation
                // hold, never a proven refusal.
                return Err(DispatchError::threw(format!(
                    "trailing post-create select unproven: Mission Control exit (Dock AXExposeExit) unobserved within {}ms (wedged-Dock guard); created Space retained with proof; no gesture dispatched; a fresh action may reuse it, never an automatic replay",
                    self.wait_ms
                )));
            }
            // Event delivery vehicle: the observer callback fires only
            // while this thread pumps its runloop. Bounded slice; the
            // deadline above bounds the total.
            if let Some(watch) = self.watch.as_ref() {
                watch.pump(Duration::from_millis(MC_EXIT_PUMP_MS));
            }
        }
    }
}

/// Injectable pinned single-Space create leg: the exact pre-existing
/// `mc_add_space_pinned` create plus exact-append proof. Production
/// ([`LiveMcCreateEffect`]) runs the live leg; tests script the outcome
/// and never open Mission Control.
pub(crate) trait McCreateEffect {
    /// Run the pinned create on `display` (joined by the exact
    /// `(cgid, managed UUID)` identity) inside `budget`. `Ok` carries the
    /// verified exact append (OS accepted, close shortfall folded as
    /// accepted); `Err(refused)` appends nothing (pre-effect refusal);
    /// `Err(threw)` ran at or after the touch with an unknown effect.
    fn create_append(
        &mut self,
        display: usize,
        cgid: u32,
        uuid: &str,
        budget: &mut crate::mc_probe::ActionBudget,
    ) -> Result<(), DispatchError>;
}

/// Production pinned create: the pre-existing create leg (fresh SLS join,
/// verified pinned identity, one budgeted open/press, exact-append
/// readback) — unchanged semantics, new home so tests can script the leg.
pub(crate) struct LiveMcCreateEffect;

impl McCreateEffect for LiveMcCreateEffect {
    fn create_append(
        &mut self,
        display: usize,
        cgid: u32,
        uuid: &str,
        budget: &mut crate::mc_probe::ActionBudget,
    ) -> Result<(), DispatchError> {
        let outcome =
            crate::mc_lifecycle::live::mc_add_space_pinned(display, cgid, uuid, budget, false)
                .map_err(DispatchError::from_mc)?;
        if let Some(e) = outcome.close_error {
            eprintln!("tiler-macos select pinned create close shortfall accepted ({e})");
        }
        Ok(())
    }
}

/// Production poster: the pinned multi-pair dispatch plus the single
/// pending release, both through the session tap (see
/// [`crate::space_switch_edges::post_gesture_pairs`] /
/// [`post_gesture_release`](crate::space_switch_edges::post_gesture_release)).
pub struct LiveSwipePoster;

impl SwipePoster for LiveSwipePoster {
    fn post_pairs(&mut self, sign: f64, pairs: usize) -> PairsPosted {
        match crate::space_switch_edges::post_gesture_pairs(sign, pairs) {
            GesturePairsOutcome::Paired(report) => PairsPosted {
                began_posted: report.began_posted,
                ended_posted: report.ended_posted,
                pending_sign: None,
                error: None,
            },
            GesturePairsOutcome::Pending {
                report,
                sign,
                error,
            } => PairsPosted {
                began_posted: report.began_posted,
                ended_posted: report.ended_posted,
                pending_sign: Some(sign),
                error: Some(error),
            },
            GesturePairsOutcome::Refused { report, error } => PairsPosted {
                began_posted: report.began_posted,
                ended_posted: report.ended_posted,
                pending_sign: None,
                error: Some(error),
            },
        }
    }

    fn release(&mut self, sign: f64) -> Result<(), String> {
        crate::space_switch_edges::post_gesture_release(sign)
    }
}

/// Production topology over the caller's pinned plain-data legs: fresh SLS
/// rows plus per-SID type reads on this thread, joined against the
/// main-published `(display_index, cgid, managed UUID)` — never an AppKit
/// read, so this thread neither touches nor blocks the main thread.
pub struct LiveSwipeTopology {
    display_index: usize,
    pinned_cgid: u32,
    pinned_uuid: String,
    published: Vec<crate::topology::DisplayGeometry>,
    separate: Option<bool>,
}

impl LiveSwipeTopology {
    #[must_use]
    pub fn pinned(
        display_index: usize,
        pinned_cgid: u32,
        pinned_uuid: &str,
        published: &[crate::topology::DisplayGeometry],
        separate: Option<bool>,
    ) -> Self {
        Self {
            display_index,
            pinned_cgid,
            pinned_uuid: pinned_uuid.to_owned(),
            published: published.to_vec(),
            separate,
        }
    }
}

impl SwipeTopology for LiveSwipeTopology {
    fn display_count(&mut self) -> Result<usize, String> {
        crate::workspace_select::fresh_display_count()
    }

    fn fresh_full_row(&mut self) -> Result<FullRow, String> {
        fresh_pinned_full_row(self.display_index, self.pinned_cgid, &self.pinned_uuid)
    }

    fn revision(&mut self) -> Option<u64> {
        let rows = crate::ffi::sls_display_rows().ok()?;
        Some(crate::topology::fingerprint_snapshot(
            &self.published,
            &rows,
            self.separate,
        ))
    }
}

/// Pure pinned-row assembly over one SLS row plus a type closure: the
/// pinned `(cgid, managed UUID)` joins by exact identifier — never
/// positional, never guessed. Refuses zero CG IDs, missing identities,
/// identifier mismatch (stale publication or display swap), empty rows,
/// missing current, current outside the native order, empty user rows,
/// and unreadable types. Tested with producer-shaped fixtures (see
/// `dock_swipe_select_tests`).
pub(crate) fn assemble_pinned_row(
    spaces: &[u64],
    current: Option<u64>,
    identifier: Option<&str>,
    space_type: &dyn Fn(u64) -> Result<crate::ffi::SlsSpaceType, String>,
    display_index: usize,
    pinned_cgid: u32,
    pinned_uuid: &str,
) -> Result<FullRow, String> {
    if pinned_cgid == 0 {
        return Err("dock-swipe refused: pinned display has no CG display ID".to_owned());
    }
    let identifier = identifier.ok_or_else(|| {
        format!("dock-swipe refused: display {display_index} identity unreadable")
    })?;
    if identifier != pinned_uuid {
        return Err(
            "dock-swipe refused: pinned display UUID no longer names the SLS row".to_owned(),
        );
    }
    if spaces.is_empty() {
        return Err("dock-swipe refused: native row empty".to_owned());
    }
    let current =
        current.ok_or_else(|| "dock-swipe refused: current Space unreadable".to_owned())?;
    if !spaces.contains(&current) {
        return Err("dock-swipe refused: current Space not in native order".to_owned());
    }
    let mut user_spaces = Vec::new();
    for sid in spaces {
        match space_type(*sid)? {
            crate::ffi::SlsSpaceType::User => user_spaces.push(*sid),
            _ => continue,
        }
    }
    if user_spaces.is_empty() {
        return Err("dock-swipe refused: user row unprovable".to_owned());
    }
    Ok(FullRow {
        display_id: identifier.to_owned(),
        full_order: spaces.to_vec(),
        user_spaces,
        current,
    })
}

/// Fresh pinned full-order row over live OS reads (SLS rows plus per-SID
/// type reads on the calling thread). Thin wrapper over
/// [`assemble_pinned_row`].
pub(crate) fn fresh_pinned_full_row(
    display_index: usize,
    pinned_cgid: u32,
    pinned_uuid: &str,
) -> Result<FullRow, String> {
    let rows =
        crate::ffi::sls_display_rows().map_err(|e| format!("dock-swipe refused: rows ({e})"))?;
    let row = rows.get(display_index).ok_or_else(|| {
        format!(
            "dock-swipe refused: display {display_index} unknown ({} rows)",
            rows.len()
        )
    })?;
    assemble_pinned_row(
        &row.spaces,
        row.current,
        row.identifier.as_deref(),
        &|sid| crate::ffi::sls_space_type(sid).map_err(|e| e.to_string()),
        display_index,
        pinned_cgid,
        pinned_uuid,
    )
}

/// One bounded metadata-only dispatch line (phase boundary: intent).
/// Operation identity plus IDs and counts only — never content.
#[must_use]
pub fn format_dock_swipe_dispatch_line(
    op: &str,
    source: u64,
    target: u64,
    steps: usize,
    sign: f64,
    elapsed_ms: u64,
) -> String {
    format!(
        "tiler-macos select route=dock-swipe phase=dispatch op={op} source={source} target={target} steps={steps} sign={sign:+.0} elapsed_ms={elapsed_ms}"
    )
}

/// One bounded metadata-only posted line (phase boundary: actuation).
#[must_use]
pub fn format_dock_swipe_posted_line(
    op: &str,
    source: u64,
    target: u64,
    began: usize,
    ended: usize,
    elapsed_ms: u64,
) -> String {
    format!(
        "tiler-macos select route=dock-swipe phase=posted op={op} source={source} target={target} began={began} ended={ended} elapsed_ms={elapsed_ms}"
    )
}

/// One bounded metadata-only settled line (phase boundary: proof).
/// `confirmed` names whether the native current now reads exactly the
/// target — posting is never read as arrival, and visual/input readiness
/// is never inferred.
#[must_use]
pub fn format_dock_swipe_settled_line(
    op: &str,
    source: u64,
    target: u64,
    native_current: Option<u64>,
    confirmed: bool,
    elapsed_ms: u64,
) -> String {
    format!(
        "tiler-macos select route=dock-swipe phase=settled op={op} source={source} target={target} native_current={native_current:?} confirmed={confirmed} elapsed_ms={elapsed_ms}"
    )
}

/// One bounded metadata-only refused line (phase boundary: refusal).
/// Emitted on every refusal path so no attempted select goes unmeasured.
#[must_use]
pub fn format_dock_swipe_refused_line(
    op: &str,
    source: Option<u64>,
    target: u64,
    reason: &str,
    elapsed_ms: u64,
) -> String {
    format!(
        "tiler-macos select route=dock-swipe phase=refused op={op} source={source:?} target={target} reason={reason} elapsed_ms={elapsed_ms}"
    )
}

/// One Dock-swipe select request: the logical target plus the resolved
/// context. `t_base` chains submit-to-observed timing across threads
/// (logging only); `deadline` bounds the event-driven settlement.
pub struct SwipeRequest<'a> {
    pub target: u64,
    pub os_major: Option<u32>,
    pub before: Option<&'a SelectionSnapshot>,
    pub op: Option<&'a str>,
    pub t_base: Instant,
    pub deadline: Duration,
}

/// Live seams for one Dock-swipe select (all owned by the caller).
pub struct SwipeSeams<'a> {
    pub topo: &'a mut dyn SwipeTopology,
    pub poster: &'a mut dyn SwipePoster,
    pub latch: &'a SpaceSelectLatch,
    pub cancel: &'a AtomicBool,
    pub gates: &'a SelectGates,
}

/// Execute one Dock-swipe select of the existing `target` SID: fresh
/// same-display full-order proof, selector-entry drift fence against
/// `before`, version gate, owner-fresh pre-dispatch gates, immediate pair
/// dispatch, then the event-driven settlement loop — a fresh native read
/// on EVERY latch event until the exact target, one absolute deadline, or
/// cancellation. Intermediate and non-target notifications are normal.
/// The deadline takes one final read (covers lost/early notifications).
/// Cancellation refuses promptly without advancing history. Confirmation
/// re-proves the same display identity, full order, user row, and
/// single-display count — not just the current SID.
///
/// Returns `Ok(changed)` (`false` for the already-there path, which posts
/// nothing). Any `Err` is honest refusal/unproven output: history advances
/// nowhere, nothing replays, and a `began`-without-`ended` partial
/// releases exactly once before returning.
pub fn execute_dock_swipe_select(
    req: SwipeRequest<'_>,
    seams: SwipeSeams<'_>,
) -> Result<bool, DispatchError> {
    let op = req.op.unwrap_or("none");
    let elapsed_ms = || req.t_base.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let refused = |source: Option<u64>, reason: String| -> DispatchError {
        eprintln!(
            "{}",
            format_dock_swipe_refused_line(op, source, req.target, &reason, elapsed_ms())
        );
        DispatchError::refused(reason)
    };
    let threw = |source: Option<u64>, reason: String| -> DispatchError {
        eprintln!(
            "{}",
            format_dock_swipe_refused_line(op, source, req.target, &reason, elapsed_ms())
        );
        DispatchError::threw(reason)
    };
    if !seams.gates.session_unlocked {
        return Err(refused(
            None,
            "dock-swipe refused: session not unlocked pre-dispatch".to_owned(),
        ));
    }
    if seams.gates.secure_active {
        return Err(refused(
            None,
            "dock-swipe refused: Secure Input active pre-dispatch".to_owned(),
        ));
    }
    if seams.gates.lock_screen {
        return Err(refused(
            None,
            "dock-swipe refused: lock screen pre-dispatch".to_owned(),
        ));
    }
    match seams.topo.display_count() {
        Ok(1) => {}
        Ok(n) => {
            return Err(refused(
                None,
                format!("dock-swipe refused: expected exactly one native display, found {n}"),
            ));
        }
        Err(e) => {
            return Err(refused(
                None,
                format!("dock-swipe refused: display count unreadable ({e})"),
            ));
        }
    }
    if let Err(e) = gesture_payload_allowed(req.os_major) {
        return Err(refused(None, format!("dock-swipe refused: {e}")));
    }
    if req.target == 0 {
        return Err(refused(
            None,
            "dock-swipe refused: zero target SID".to_owned(),
        ));
    }
    let row0 = seams
        .topo
        .fresh_full_row()
        .map_err(|e| refused(None, format!("dock-swipe refused: {e}")))?;
    // Selector-entry drift fence: the fresh user row must equal the
    // claim-time `before` (display identity, mapping, and current).
    if let Some(before) = req.before
        && (row0.display_id != before.display
            || row0.user_spaces != before.spaces
            || row0.current != before.current)
    {
        return Err(refused(
            Some(row0.current),
            "dock-swipe refused: fresh user row drifted from claim-time row; no swipe dispatched"
                .to_owned(),
        ));
    }
    if !row0.user_spaces.contains(&req.target) {
        return Err(refused(
            Some(row0.current),
            format!(
                "dock-swipe refused: target Space {} not in user row",
                req.target
            ),
        ));
    }
    let (sign, pairs) = swipe_plan(&row0.full_order, row0.current, req.target)
        .map_err(|e| refused(Some(row0.current), e))?;
    if seams.cancel.load(Ordering::SeqCst) || seams.latch.cancelled() {
        return Err(refused(
            Some(row0.current),
            "dock-swipe refused: cancelled before dispatch".to_owned(),
        ));
    }
    // Immediate pre-post read: already-there proves without posting (the
    // higher-level history contract still observes the change) only when
    // the same display identity, full native order, and user row still
    // hold; a moved source — current OR topology — refuses instead of
    // posting a stale ordinal distance. A target that arrived under a
    // moved topology likewise refuses: the no-post proof is only honest
    // on unchanged topology. Ordinary sequential read/post race accepted
    // past this point.
    match seams.topo.fresh_full_row() {
        Ok(pre)
            if pre.current == req.target
                && pre.display_id == row0.display_id
                && pre.full_order == row0.full_order
                && pre.user_spaces == row0.user_spaces =>
        {
            eprintln!(
                "{}",
                format_dock_swipe_settled_line(
                    op,
                    row0.current,
                    req.target,
                    Some(pre.current),
                    true,
                    elapsed_ms()
                )
            );
            return Ok(false);
        }
        Ok(pre) if pre.current == req.target => {
            return Err(refused(
                Some(pre.current),
                "dock-swipe unproven: target arrived but topology drifted pre-post; history unchanged; no replay"
                    .to_owned(),
            ));
        }
        Ok(pre)
            if pre.current == row0.current
                && pre.display_id == row0.display_id
                && pre.full_order == row0.full_order
                && pre.user_spaces == row0.user_spaces => {}
        Ok(pre) if pre.current != row0.current => {
            return Err(refused(
                Some(pre.current),
                format!(
                    "dock-swipe refused: source Space moved {}->{} pre-post; no swipe dispatched",
                    row0.current, pre.current
                ),
            ));
        }
        Ok(pre) => {
            return Err(refused(
                Some(pre.current),
                "dock-swipe refused: topology drifted pre-post; no swipe dispatched".to_owned(),
            ));
        }
        Err(e) => {
            return Err(refused(
                Some(row0.current),
                format!("dock-swipe refused: pre-post row unreadable ({e})"),
            ));
        }
    }
    if pairs == 0 {
        return Err(refused(
            Some(row0.current),
            format!(
                "dock-swipe refused: plan is a no-op yet native current reads {}; refusing without posting",
                row0.current
            ),
        ));
    }
    eprintln!(
        "{}",
        format_dock_swipe_dispatch_line(op, row0.current, req.target, pairs, sign, elapsed_ms())
    );
    let posted = seams.poster.post_pairs(sign, pairs);
    eprintln!(
        "{}",
        format_dock_swipe_posted_line(
            op,
            row0.current,
            req.target,
            posted.began_posted,
            posted.ended_posted,
            elapsed_ms()
        )
    );
    if let Some(sign) = posted.pending_sign {
        let release = seams.poster.release(sign);
        let reason = format!(
            "dock-swipe unproven: began-without-ended partial (began={} ended={}) release={} ({:?}); no replay",
            posted.began_posted,
            posted.ended_posted,
            release.map(|()| "posted").unwrap_or("refused"),
            posted.error,
        );
        eprintln!(
            "{}",
            format_dock_swipe_settled_line(op, row0.current, req.target, None, false, elapsed_ms())
        );
        return Err(threw(None, reason));
    }
    if posted.began_posted != pairs || posted.ended_posted != pairs {
        if posted.began_posted == 0 && posted.ended_posted == 0 {
            return Err(refused(
                Some(row0.current),
                format!(
                    "dock-swipe refused: posted 0/{pairs} pairs ({:?}); nothing posted, no replay",
                    posted.error
                ),
            ));
        }
        let reason = format!(
            "dock-swipe unproven: partial posting ({}/{} began, {}/{} ended) ({:?}); history unchanged, no replay",
            posted.began_posted, pairs, posted.ended_posted, pairs, posted.error
        );
        eprintln!(
            "{}",
            format_dock_swipe_settled_line(op, row0.current, req.target, None, false, elapsed_ms())
        );
        return Err(threw(None, reason));
    }
    if seams.cancel.load(Ordering::SeqCst) || seams.latch.cancelled() {
        return Err(DispatchError::accepted(
            "dock-swipe unproven: cancelled after posting; no replay".to_owned(),
        ));
    }
    // Event-driven settlement: one absolute deadline; a fresh full-row
    // read on EVERY latch event until the exact target. Intermediate and
    // non-target notifications are normal for nonadjacent swipes. Only
    // the deadline takes a final read. `observed` is claimed once up front
    // (an event racing loop entry makes the first wait return at once);
    // every later wait reuses the generation the previous wait advanced
    // to, and any event between a read and its wait short-circuits it —
    // so no notification is ever lost without polling.
    let absolute = Instant::now() + req.deadline;
    let mut observed = seams.latch.generation();
    let mut last_current: Option<u64>;
    loop {
        // Only the deadline takes a final read: once the absolute bound
        // is reached this iteration settles on one last proof read.
        if Instant::now() >= absolute {
            let probe = seams.topo.fresh_full_row().ok();
            last_current = probe.as_ref().map(|row| row.current);
            let confirmed = matches!(&probe, Some(row1)
                if last_current == Some(req.target)
                    && row1.display_id == row0.display_id
                    && row1.full_order == row0.full_order
                    && row1.user_spaces == row0.user_spaces)
                && matches!(seams.topo.display_count(), Ok(1));
            eprintln!(
                "{}",
                format_dock_swipe_settled_line(
                    op,
                    row0.current,
                    req.target,
                    last_current,
                    confirmed,
                    elapsed_ms()
                )
            );
            if confirmed {
                if seams.cancel.load(Ordering::SeqCst) || seams.latch.cancelled() {
                    return Err(DispatchError::accepted(
                        "dock-swipe unproven: cancelled during settlement; history unchanged; no replay"
                            .to_owned(),
                    ));
                }
                return Ok(row0.current != req.target);
            }
            // Posted but unproven within the bound (delayed arrival): the OS
            // accepted the dispatch. Return Accepted and let the drain's
            // genuine post read hold or settle; never an invented Threw.
            return Err(DispatchError::accepted(
                "dock-swipe unproven: arrival deadline elapsed after posting; no replay",
            ));
        }
        let probe = seams.topo.fresh_full_row().ok();
        last_current = probe.as_ref().map(|row| row.current);
        if last_current == Some(req.target) {
            match probe {
                Some(row1)
                    if row1.display_id == row0.display_id
                        && row1.full_order == row0.full_order
                        && row1.user_spaces == row0.user_spaces =>
                {
                    match seams.topo.display_count() {
                        Ok(1) => {}
                        Ok(n) => {
                            let reason = format!(
                                "dock-swipe unproven: display count moved to {n} during swipe; history unchanged"
                            );
                            eprintln!(
                                "{}",
                                format_dock_swipe_settled_line(
                                    op,
                                    row0.current,
                                    req.target,
                                    last_current,
                                    false,
                                    elapsed_ms()
                                )
                            );
                            // Posted with an unreadable confirmation leg:
                            // Accepted with the drain's genuine post read.
                            return Err(DispatchError::accepted(reason));
                        }
                        Err(e) => {
                            let reason = format!(
                                "dock-swipe unproven: display count unreadable at confirmation ({e}); history unchanged"
                            );
                            eprintln!(
                                "{}",
                                format_dock_swipe_settled_line(
                                    op,
                                    row0.current,
                                    req.target,
                                    last_current,
                                    false,
                                    elapsed_ms()
                                )
                            );
                            return Err(DispatchError::accepted(reason));
                        }
                    }
                    eprintln!(
                        "{}",
                        format_dock_swipe_settled_line(
                            op,
                            row0.current,
                            req.target,
                            last_current,
                            true,
                            elapsed_ms()
                        )
                    );
                    if seams.cancel.load(Ordering::SeqCst) || seams.latch.cancelled() {
                        return Err(DispatchError::accepted(
                            "dock-swipe unproven: cancelled during settlement; history unchanged; no replay"
                                .to_owned(),
                        ));
                    }
                    return Ok(row0.current != req.target);
                }
                _ => {
                    eprintln!(
                        "{}",
                        format_dock_swipe_settled_line(
                            op,
                            row0.current,
                            req.target,
                            last_current,
                            false,
                            elapsed_ms()
                        )
                    );
                    // Posted but topology drifted under the swipe: Accepted
                    // with the drain's genuine post read, never Threw.
                    return Err(DispatchError::accepted(
                        "dock-swipe unproven: topology changed after posting; no replay",
                    ));
                }
            }
        }
        if seams.cancel.load(Ordering::SeqCst) || seams.latch.cancelled() {
            eprintln!(
                "{}",
                format_dock_swipe_settled_line(
                    op,
                    row0.current,
                    req.target,
                    last_current,
                    false,
                    elapsed_ms()
                )
            );
            return Err(DispatchError::accepted(
                "dock-swipe unproven: cancelled during settlement; history unchanged; no replay"
                    .to_owned(),
            ));
        }
        let now = Instant::now();
        match seams
            .latch
            .wait(observed, absolute.saturating_duration_since(now))
        {
            LatchWait::Advanced(next) => {
                observed = next;
            }
            LatchWait::Cancelled => {
                eprintln!(
                    "{}",
                    format_dock_swipe_settled_line(
                        op,
                        row0.current,
                        req.target,
                        last_current,
                        false,
                        elapsed_ms()
                    )
                );
                return Err(DispatchError::accepted(
                    "dock-swipe unproven: cancelled during settlement; history unchanged; no replay"
                        .to_owned(),
                ));
            }
            LatchWait::TimedOut => {}
        }
    }
}

/// Pinned plain-data display legs for one off-main selection backend: the
/// caller-published identity every fresh read re-verifies — never an
/// AppKit call on the owner. Built per job on submission, then moved to
/// the owner.
#[derive(Debug, Clone)]
pub struct PinnedLegs {
    pub display_index: usize,
    pub pinned_cgid: u32,
    pub pinned_uuid: String,
    pub published_displays: Vec<crate::topology::DisplayGeometry>,
    pub published_separate: Option<bool>,
    /// Settlement bound for this job's selects (production: the arrival
    /// deadline; tests inject short bounds).
    pub settle_deadline: Duration,
}

/// Owner-side select backend: fresh SLS proof plus pinned display join
/// plus pinned MC create plus Dock-swipe select — all on the owner thread,
/// zero AppKit touch. Generic over the topology/poster seams so tests
/// script every leg; [`LiveOwnerSelectBackend`] is the production shape.
pub(crate) struct OwnerSelectBackend<T: SwipeTopology, P: SwipePoster> {
    legs: PinnedLegs,
    latch: Arc<SpaceSelectLatch>,
    cancel: Arc<AtomicBool>,
    budget: crate::mc_probe::ActionBudget,
    gate_reads: Box<dyn GateReads>,
    creator: Box<dyn McCreateEffect>,
    exit_gate: Box<dyn McExitGate>,
    exit_armed: bool,
    topo: T,
    poster: P,
    op: Option<String>,
    t_base: Instant,
}

/// Production owner backend over live seams.
pub(crate) type LiveOwnerSelectBackend = OwnerSelectBackend<LiveSwipeTopology, LiveSwipePoster>;

impl<T: SwipeTopology, P: SwipePoster> OwnerSelectBackend<T, P> {
    /// Backend over explicitly pinned legs with injected seams (topology,
    /// poster, gate reads, settle latch, cancellation). `budget_secs`
    /// bounds the pinned MC create leg; settlement carries `settle_deadline`
    /// from the legs.
    pub(crate) fn pinned_with(args: OwnerBackendArgs<T, P>) -> Self {
        Self {
            legs: args.legs,
            latch: args.latch,
            cancel: args.cancel,
            budget: crate::mc_probe::ActionBudget::new(args.budget_secs),
            gate_reads: args.gate_reads,
            creator: args.creator,
            exit_gate: args.exit_gate,
            exit_armed: false,
            topo: args.topo,
            poster: args.poster,
            op: args.op,
            t_base: args.t_base,
        }
    }

    /// Test-only readback of the injected poster (lets offline tests prove
    /// zero gesture posts on refused paths without live effects).
    #[cfg(test)]
    pub(crate) fn test_poster(&self) -> &P {
        &self.poster
    }
}

/// Constructor bundle for [`OwnerSelectBackend::pinned_with`] (keeps the
/// seam count inside the argument-count lint).
pub(crate) struct OwnerBackendArgs<T: SwipeTopology, P: SwipePoster> {
    pub legs: PinnedLegs,
    pub latch: Arc<SpaceSelectLatch>,
    pub cancel: Arc<AtomicBool>,
    pub budget_secs: u64,
    pub gate_reads: Box<dyn GateReads>,
    pub creator: Box<dyn McCreateEffect>,
    pub exit_gate: Box<dyn McExitGate>,
    pub topo: T,
    pub poster: P,
    pub op: Option<String>,
    pub t_base: Instant,
}

impl OwnerSelectBackend<LiveSwipeTopology, LiveSwipePoster> {
    /// Production backend: live seams over the pinned legs.
    pub(crate) fn pinned(
        legs: PinnedLegs,
        latch: Arc<SpaceSelectLatch>,
        cancel: Arc<AtomicBool>,
        budget_secs: u64,
        gate_reads: Box<dyn GateReads>,
        op: Option<String>,
        t_base: Instant,
    ) -> Self {
        let topo = LiveSwipeTopology::pinned(
            legs.display_index,
            legs.pinned_cgid,
            &legs.pinned_uuid,
            &legs.published_displays,
            legs.published_separate,
        );
        Self::pinned_with(OwnerBackendArgs {
            legs,
            latch,
            cancel,
            budget_secs,
            gate_reads,
            creator: Box::new(LiveMcCreateEffect),
            exit_gate: Box::new(LiveMcExitGate::new()),
            topo,
            poster: LiveSwipePoster,
            op,
            t_base,
        })
    }
}

impl<T: SwipeTopology, P: SwipePoster> SelectionBackend for OwnerSelectBackend<T, P> {
    fn snapshot(&mut self) -> Result<SelectionSnapshot, String> {
        let row = self.topo.fresh_full_row()?;
        Ok(SelectionSnapshot {
            display: row.display_id,
            spaces: row.user_spaces,
            current: row.current,
        })
    }

    fn display_count(&mut self) -> Result<usize, String> {
        self.topo.display_count()
    }

    fn revision(&mut self) -> Option<u64> {
        self.topo.revision()
    }

    fn check_output(&mut self, output: &str) -> Result<(), String> {
        if self.legs.pinned_cgid == 0 {
            return Err(
                "product select refuses: pinned display has no CG display ID; output unprovable"
                    .to_owned(),
            );
        }
        let pinned = crate::topology::output_key(self.legs.pinned_cgid);
        if pinned != output {
            return Err(format!(
                "product select refuses: reply target output {output} is not the pinned display {pinned}; no actuation"
            ));
        }
        Ok(())
    }

    fn empty(&mut self, sid: u64) -> Result<bool, String> {
        crate::workspace_select::probe_trailing_emptiness(sid)
    }

    fn create(&mut self, before: &SelectionSnapshot) -> Result<(), DispatchError> {
        // Owner-fresh gates and cancellation fence the create leg itself:
        // a lock, capture, or loginwindow that closed after submission
        // refuses here instead of opening Mission Control into it.
        if self.cancel.load(Ordering::SeqCst) {
            return Err(DispatchError::refused(
                "trailing create refused: cancelled before dispatch",
            ));
        }
        let gates = self.gate_reads.read_gates();
        if !gates.session_unlocked {
            return Err(DispatchError::refused(
                "trailing create refused: session not unlocked pre-dispatch",
            ));
        }
        if gates.secure_active {
            return Err(DispatchError::refused(
                "trailing create refused: Secure Input active pre-dispatch",
            ));
        }
        if gates.lock_screen {
            return Err(DispatchError::refused(
                "trailing create refused: lock screen pre-dispatch",
            ));
        }
        if self
            .snapshot()
            .map_err(|e| DispatchError::refused(format!("pre-create row unreadable ({e})")))?
            != *before
        {
            return Err(DispatchError::refused("pre-create topology drift"));
        }
        // Arm the Dock exit watch BEFORE the create leg opens Mission
        // Control: an exit delivered mid-leg (even before the leg's close
        // returns) is retained in the latch generation, never lost. Arm
        // failure refuses pre-effect — nothing appended, nothing to adopt.
        self.exit_gate
            .arm()
            .map_err(|e| DispatchError::refused(format!("trailing create refused: {e}")))?;
        self.exit_armed = true;
        let append = self.creator.create_append(
            self.legs.display_index,
            self.legs.pinned_cgid,
            &self.legs.pinned_uuid,
            &mut self.budget,
        );
        if append.is_err() {
            // Armed but never awaited (no select follows a failed
            // create): tear the watch down promptly; the backend `Drop`
            // is only the backstop.
            self.exit_armed = false;
            self.exit_gate.disarm();
        }
        append
    }

    fn select(&mut self, before: &SelectionSnapshot, sid: u64) -> Result<(), DispatchError> {
        // Post-create gate: the trailing append left Mission Control
        // open, and the gesture is a no-op while Dock still reports it
        // active (`space_manager_focus_space` refuses under
        // `mission_control_is_active`, which clears only on Dock
        // `AXExposeExit` — never on the AX-group read the close poll
        // uses). Wait for the actual exit before any post; failure keeps
        // the already-captured append proof (ownership stays adoptable)
        // while dispatching nothing. Normal existing selects never armed
        // the watch and pay no wait here.
        if self.exit_armed {
            self.exit_armed = false;
            self.exit_gate.await_exit(&self.cancel)?;
        }
        // Owner-fresh gates (never the stale submit-time snapshot), the
        // per-job settle latch, and the shared cancellation flag travel
        // to the executor; the `before` row fences selector-entry drift.
        let gates = self.gate_reads.read_gates();
        let req = SwipeRequest {
            target: sid,
            os_major: crate::ffi::macos_major(),
            before: Some(before),
            op: self.op.as_deref(),
            t_base: self.t_base,
            deadline: self.legs.settle_deadline,
        };
        let seams = SwipeSeams {
            topo: &mut self.topo,
            poster: &mut self.poster,
            latch: &self.latch,
            cancel: &self.cancel,
            gates: &gates,
        };
        execute_dock_swipe_select(req, seams).map(|_| ())
    }
}
