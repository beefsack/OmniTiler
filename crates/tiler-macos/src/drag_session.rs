#![forbid(unsafe_code)]

//! Main-thread Meta+left drag session (offline-actuated, live-verified).
//!
//! LIVE ARM PARKED (`live-acceptance-pending`, diagnostic-only): the pure
//! Engine/controller/carrier scaffolding below stays tested offline, but
//! ordinary product runtime installs NO mouse tap —
//! [`DRAG_LIVE_ACCEPTED`] is `false` until a bounded live-acceptance sweep
//! proves a real drag/preview/drop/cancel, and no `drag_preview`
//! enabled/color setting can satisfy it.
//!
//! The Meta+left producer is the mouse event tap
//! ([`crate::ffi::DragTapSource`]): its callback consults only idle/held
//! shape plus the Meta chord under a `try_lock` (consume-first, never a
//! block, no AX/core/log contact). This session owns the shared
//! [`crate::drag_drop::DragController`] on the tile main thread and drives
//! the rest: press-owner proof from published per-PID facts (no AX on this
//! path), per-sample [`crate::drag_drop::DragFences`], Engine preview
//! queries plus drop submits through the [`crate::runtime`] worker (the
//! current Engine, never an independent one), and one filled above-target
//! carrier ([`crate::ffi::GroupUnderlayCarrier`]) with hide-before-mutation
//! ordering.
//!
//! Coordinate convention (see `border_session`): tap points and Engine
//! rects are CG top-left points (the HID post/hit-join space, directly
//! comparable to AX frames); only the carrier frame flips to AppKit
//! bottom-left via the primary height. No gesture ever injects input:
//! the tap suppresses only the owned Meta+left edges while held, every
//! consumed down balances through exactly one release path (drop, Escape,
//! stale fence, or process stop), and the native titlebar arm stays parked
//! ([`crate::drag_native`]).
//!
//! Cross-output is explicitly unsupported in this unit: the runtime worker
//! homes every drag command to the subject's current domain over that
//! domain's full membership (no target-domain rows, no pair contract), so
//! an outside-domain pointer resolves to `cross-domain-mismatch` and snaps
//! back. Same-domain edge/group/interior targeting rides the core
//! resolver; centre stays unsupported (no swap/stacks).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tiler_core::visual::VisualRect;

use crate::border_runtime::BorderAppRecord;
use crate::config::ProbeModifier;
use crate::drag_drop::{
    DragController, DragFences, DragOptions, DragOrdering, DragOverlay, MotionGate, PreviewOutcome,
    meta_held_for,
};
use crate::ffi::{BorderCarrierFlags, DragMouseHit, DragMouseKind};
use crate::input_eligibility::{FACTS_TTL, is_fresh_at};
use crate::runtime_types::window_id;

/// Escape keycode: the keyboard tap forwards it unsuppressed while a drag
/// holds, and the main thread cancels through it (never a mutation).
pub const ESCAPE_KEYCODE: u32 = 53;

/// Bounded Engine preview-query wait on the main thread. A timeout (or a
/// dead engine) cancels the gesture stale — the tap never blocks, and the
/// main thread never waits past this bound.
pub const PREVIEW_QUERY_TIMEOUT: Duration = Duration::from_millis(100);

/// Live acceptance gate for the Meta+left drag-preview arm: `false` until a
/// bounded live-acceptance sweep proves a real drag/preview/drop/cancel
/// against disposable owned helpers. While `false`, ordinary product runtime
/// NEVER installs the mouse tap: [`drag_arm_parked_reason`] parks every
/// combination (even enabled on the tap route), and the live `arm` below
/// refuses. Ordinary `drag_preview` enabled/color settings cannot satisfy
/// this gate (no flag, no settings key — a source change after live proof
/// only). Pure Engine/controller/carrier scaffolding stays tested offline;
/// the parked arm is diagnostic-only.
pub const DRAG_LIVE_ACCEPTED: bool = false;

/// Parked reason while the live drag arm awaits acceptance (terminal
/// evidence; never a silent no-op).
pub const DRAG_PARKED_REASON: &str = "live-acceptance-pending: Meta+left drag-preview arm parked until a bounded live-acceptance sweep proves a real drag/preview/drop/cancel; ordinary tile sessions install no mouse tap";

/// Pure parked verdict for the drag-preview arm (no OS touch): while the
/// live acceptance gate is closed, every combination parks — including the
/// ordinary enabled-on-tap product session. The live `DragRunState` arm
/// path forwards here, so no product configuration can arm the mouse tap
/// while parked.
#[must_use]
pub fn drag_arm_parked_reason(drag_enabled: bool, input_is_tap: bool) -> Option<String> {
    if !DRAG_LIVE_ACCEPTED {
        return Some(DRAG_PARKED_REASON.to_owned());
    }
    if !drag_enabled {
        return Some("disabled".to_owned());
    }
    if !input_is_tap {
        return Some("off-route".to_owned());
    }
    None
}

/// Press-owner proof from published per-PID facts: the frontmost PID's
/// focused window containing the press point, with selected-oracle focus
/// agreement (the Meta chord itself never moves focus).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PressOwner {
    /// Owning process.
    pub pid: u32,
    /// Focused window holding the press.
    pub wid: u32,
    /// Runtime window token (`pid_wid`) for the gesture ingress.
    pub window: String,
    /// Mover fullscreen state (fullscreen denies at the fence).
    pub fullscreen: bool,
    /// Topology revision read with the snapshot (drift cancels).
    pub revision: u64,
    /// PRE-gesture frame frozen at press time (never a mid-gesture read):
    /// the gesture ingress start frame.
    pub start_frame: tiler_core::geometry::Rect,
}

/// Resolve the press owner over plain published facts (no AX, no AppKit)
/// JOINED with a fresh off-tap positional proof: the AX hit-test owner at
/// the exact press point plus the window-server hit window at that point.
/// The frontmost PID's record must name the focused window with
/// selected-oracle agreement, fresh facts on the live revision, a finite
/// positive frame, and the press point inside that frame (edges inclusive,
/// CG top-left points — the tap space). The AX owner must equal exactly
/// `(frontmost_pid, wid)` and the window-server hit must equal `wid`:
/// a real occluder, menu, or foreign window resolves to ITS owner and
/// refuses by identity mismatch — frame containment alone never proves.
/// Every miss refuses with its leg (never guessed); the caller passes the
/// event through untouched.
#[allow(clippy::too_many_arguments)]
pub fn resolve_press_owner(
    record: Option<&BorderAppRecord>,
    frontmost_pid: u32,
    x: i32,
    y: i32,
    now_ms: u64,
    context_revision: u64,
    ax_owner: Option<(u32, u32)>,
    cg_hit_wid: Option<u32>,
) -> Result<PressOwner, &'static str> {
    if frontmost_pid <= 1 {
        return Err("unknown-window");
    }
    let record = record.ok_or("unknown-window")?;
    if record.pid != frontmost_pid {
        return Err("focus-changed");
    }
    let wid = record
        .focused_wid
        .filter(|w| *w != 0)
        .ok_or("unknown-window")?;
    if record.selected != Some((frontmost_pid, Some(wid))) {
        return Err("focus-changed");
    }
    let window = record.window.as_ref().ok_or("unknown-window")?;
    if window.wid != wid {
        return Err("focus-changed");
    }
    if record.revision != context_revision {
        return Err("revision-drift");
    }
    if !is_fresh_at(record.updated_ms, now_ms, FACTS_TTL.as_millis() as u64) {
        return Err("revision-drift");
    }
    let frame = &window.frame;
    for v in [frame.x, frame.y, frame.w, frame.h] {
        if !v.is_finite() {
            return Err("bad-geometry");
        }
    }
    if frame.w <= 0.0 || frame.h <= 0.0 {
        return Err("bad-geometry");
    }
    let (x, y) = (f64::from(x), f64::from(y));
    if x < frame.x || x > frame.x + frame.w || y < frame.y || y > frame.y + frame.h {
        return Err("outside-frame");
    }
    // Fresh positional proof (never frame containment alone): the AX hit
    // owner at the exact point must name this exact window, and the
    // window-server hit window must be this window. Unreadable joins
    // refuse (never guessed).
    if ax_owner != Some((frontmost_pid, wid)) {
        return Err("identity-mismatch");
    }
    if cg_hit_wid != Some(wid) {
        return Err("identity-mismatch");
    }
    Ok(PressOwner {
        pid: frontmost_pid,
        wid,
        window: window_id(frontmost_pid, wid),
        fullscreen: window.ax.fullscreen,
        revision: record.revision,
        start_frame: tiler_core::geometry::Rect {
            x: frame.x as i32,
            y: frame.y as i32,
            w: frame.w as i32,
            h: frame.h as i32,
        },
    })
}

/// Per-sample fence inputs as plain data (the tile loop fills these from
/// live reads each iteration; no OS touch here).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleGuards {
    /// Graphical session unlocked (unknown denies).
    pub session_unlocked: bool,
    /// Secure Input active denies.
    pub secure_input_active: bool,
    /// Product scope still allows the mover.
    pub scope_allowed: bool,
    /// Mover fullscreen, frontmost fullscreen, or ANY visible record
    /// fullscreen (game safety: a fullscreen window on the pointer display
    /// denies even when the mover is windowed; the worker hidden-domain
    /// fence plus Managed-only carrier back this).
    pub fullscreen_active: bool,
    /// Record revision still equals the live revision, facts fresh.
    pub revision_fresh: bool,
    /// Published facts still name the dragged window with selected-oracle
    /// agreement.
    pub focus_matches: bool,
}

/// Map plain guard bits plus the master switch to [`DragFences`].
#[must_use]
pub const fn fences_for(enabled: bool, guards: &SampleGuards) -> DragFences {
    DragFences {
        enabled,
        session_unlocked: guards.session_unlocked,
        secure_input_active: guards.secure_input_active,
        scope_allowed: guards.scope_allowed,
        fullscreen_active: guards.fullscreen_active,
        revision_fresh: guards.revision_fresh,
        focus_matches: guards.focus_matches,
    }
}

/// Outcome of one consumed mouse down on the main thread. The tap already
/// suppressed the edge (consume-first); a refusal arms nothing, so the
/// held button balances through pass-through drag/up edges (an uncompleted
/// press is harmless) while the controller stays idle for the next owned
/// press.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Downcome {
    /// No gesture held and this edge is not ours: ignore.
    Ignore,
    /// Gesture armed on the proven owner.
    Armed { window: String },
    /// Consumed edge refused without arming (fenced, unowned, or stale
    /// controller): capture stays idle.
    Refused { reason: &'static str },
}

/// Intent of one held-pointer sample on the main thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SampleIntent {
    /// No gesture held: ignore.
    Ignore,
    /// Hide the carrier, keep the gesture (zero movement, transient
    /// refusal, or not-yet-moved posture).
    Hide,
    /// Moved pointer with live fences: resolve one world gesture update
    /// for this exact sample.
    Preview {
        ingress: tiler_core::gesture::GestureIngress,
    },
    /// Fence refused mid-gesture (or the Engine query died): overlay
    /// cleared, capture released, dead. Carries the terminal ingress when
    /// a gesture was held so the core clears its retained session.
    Cancelled {
        reason: &'static str,
        ingress: Option<tiler_core::gesture::GestureIngress>,
    },
}

/// Intent of one button release on the main thread. The caller hides the
/// carrier FIRST (hide-before-mutation), then resolves the drop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseIntent {
    /// No gesture held: ignore.
    Ignore,
    /// Final pointer up with the single-flight capture released exactly
    /// once: resolve one world gesture release at this exact sample.
    /// Carries the press revision for the engine-side stale check.
    Drop {
        ingress: tiler_core::gesture::GestureIngress,
        press_revision: u64,
    },
}

/// Main-thread Meta+left drag session: the shared tap controller plus the
/// startup options and Meta mapping. The tap callback never touches the
/// controller (it consults only the lock-free [`crate::drag_drop::DragTapGate`]),
/// so every method below takes the brief blocking lock: a consumed tap edge
/// is never discarded to contention, and an owned down always reaches its
/// balanced terminal edge (drop, Escape, or stale cancel).
pub struct DragSession {
    controller: Arc<Mutex<DragController>>,
    options: DragOptions,
    modifier: ProbeModifier,
}

/// Outcome of one drained input batch (one main-loop iteration): the
/// single Engine contact plus carrier order for this turn. Batches
/// coalesce held motion to the latest sample (intermediate positions
/// drop; the final snapshot is exact). Escape wins over release on an
/// ambiguous turn (cancel, never mutate).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DragPlan {
    /// Nothing held, nothing to do (carrier stays as-is).
    Idle,
    /// Hide the carrier, keep the gesture (zero movement or transient
    /// refusal).
    Hide,
    /// Bind one world gesture session (Started, no overlay at the down
    /// point itself).
    Start(tiler_core::gesture::GestureIngress),
    /// Resolve one world gesture update, then show or hide by its outcome.
    Preview(tiler_core::gesture::GestureIngress),
    /// Carrier hidden FIRST by the caller, then resolve one world gesture
    /// release.
    Drop {
        ingress: tiler_core::gesture::GestureIngress,
        press_revision: u64,
    },
    /// Overlay cleared, capture released, dead: hide, never mutate.
    /// Carries the terminal ingress when a gesture was held so the core
    /// clears its retained session.
    Cancelled {
        reason: &'static str,
        ingress: Option<tiler_core::gesture::GestureIngress>,
    },
}

impl DragSession {
    /// Plan one drained batch: Escape first (cancel wins), then the first
    /// down (arm), the latest drag (preview), then the first up (drop).
    /// Unowned downs refuse without arming; ups without a gesture ignore
    /// (balanced: every consumed down ends through exactly one path).
    pub fn plan_batch(
        &self,
        hits: &[DragMouseHit],
        escape_pressed: bool,
        owner: &Result<PressOwner, &'static str>,
        fences: DragFences,
    ) -> DragPlan {
        if escape_pressed && let Some(ingress) = self.on_escape() {
            return DragPlan::Cancelled {
                reason: "escape",
                ingress: Some(ingress),
            };
        }
        if escape_pressed {
            return DragPlan::Cancelled {
                reason: "escape",
                ingress: None,
            };
        }
        let mut down: Option<DragMouseHit> = None;
        let mut drag: Option<DragMouseHit> = None;
        let mut up: Option<DragMouseHit> = None;
        for hit in hits {
            match hit.kind {
                DragMouseKind::Down => {
                    if down.is_none() {
                        down = Some(*hit);
                    }
                }
                DragMouseKind::Drag => drag = Some(*hit),
                DragMouseKind::Up => {
                    if up.is_none() {
                        up = Some(*hit);
                    }
                }
            }
        }
        if let Some(down_hit) = down {
            match self.on_down(down_hit, owner, fences) {
                HitIntent::Down(Downcome::Armed { .. }) => {}
                HitIntent::Down(Downcome::Refused { reason }) => {
                    // A refused arm holds nothing: a trailing release in
                    // the same batch balances to idle (never a drop). The
                    // tap still owns its capture until the real up, which
                    // it consumes and which ignores here.
                    if let Some(up_hit) = up {
                        self.on_up(up_hit.x, up_hit.y);
                    }
                    return DragPlan::Cancelled {
                        reason,
                        ingress: None,
                    };
                }
                _ => {}
            }
            // Escape pressed with the arming down in the same batch wins
            // over the fresh arm (cancel, never mutate).
            if escape_pressed && let Some(ingress) = self.on_escape() {
                return DragPlan::Cancelled {
                    reason: "escape",
                    ingress: Some(ingress),
                };
            }
            if escape_pressed {
                return DragPlan::Cancelled {
                    reason: "escape",
                    ingress: None,
                };
            }
        }
        // A release in the batch ends the gesture at the final pointer:
        // intermediate motion is stale once the button is up.
        if let Some(up_hit) = up {
            match self.on_up(up_hit.x, up_hit.y) {
                HitIntent::Release(ReleaseIntent::Drop {
                    ingress,
                    press_revision,
                }) => {
                    return DragPlan::Drop {
                        ingress,
                        press_revision,
                    };
                }
                _ => {
                    return DragPlan::Idle;
                }
            }
        }
        if let Some(drag_hit) = drag {
            match self.on_drag(drag_hit.x, drag_hit.y, fences) {
                HitIntent::Sample(SampleIntent::Preview { ingress }) => {
                    return DragPlan::Preview(ingress);
                }
                HitIntent::Sample(SampleIntent::Cancelled { reason, ingress }) => {
                    return DragPlan::Cancelled { reason, ingress };
                }
                HitIntent::Sample(SampleIntent::Hide) => return DragPlan::Hide,
                _ => {}
            }
        }
        // A freshly armed gesture binds its core session before any moved
        // sample (Started binds only, never previews at the down point).
        if down.is_some()
            && up.is_none()
            && drag.is_none()
            && let Some(ingress) = self.start_ingress()
        {
            return DragPlan::Start(ingress);
        }
        if self.active() {
            return DragPlan::Hide;
        }
        DragPlan::Idle
    }
}

impl DragSession {
    /// New idle session over the resolved startup options and Meta
    /// mapping. Creates the main-thread controller (the tap consults only
    /// the separately shared lock-free gate, never this controller).
    #[must_use]
    pub fn new(options: DragOptions, modifier: ProbeModifier) -> Self {
        Self {
            controller: Arc::new(Mutex::new(DragController::new(options))),
            options,
            modifier,
        }
    }

    /// Controller shared with the main-thread drive (blocking locks;
    /// the tap never touches it).
    #[must_use]
    pub fn controller(&self) -> &Arc<Mutex<DragController>> {
        &self.controller
    }

    /// Startup options (master switch plus fill).
    #[must_use]
    pub const fn options(&self) -> DragOptions {
        self.options
    }

    /// `(want, mask)` Meta bits for the tap mask (Control default,
    /// Command opt-in; exact chord).
    #[must_use]
    pub fn meta_bits(&self) -> (u64, u64) {
        crate::input::cg_flag_bits(self.modifier)
    }

    /// Whether the tap flags word holds the run's Meta chord exactly.
    #[must_use]
    pub fn meta_held(&self, flags: u64) -> bool {
        meta_held_for(flags, self.modifier)
    }

    /// True while a gesture holds the single-flight capture.
    #[must_use]
    pub fn active(&self) -> bool {
        self.controller
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .active()
    }

    /// One mouse-tap hit from the drain: down arms on the proven owner,
    /// drag samples (the caller coalesces to the latest per iteration),
    /// up releases exactly once. Unowned kinds and unconsumed edges
    /// ignore; the tap callback already filtered consumption.
    pub fn on_hit(
        &self,
        hit: DragMouseHit,
        owner: &Result<PressOwner, &'static str>,
        fences: DragFences,
    ) -> HitIntent {
        match hit.kind {
            DragMouseKind::Down => self.on_down(hit, owner, fences),
            DragMouseKind::Drag => self.on_drag(hit.x, hit.y, fences),
            DragMouseKind::Up => self.on_up(hit.x, hit.y),
        }
    }

    /// One consumed mouse down: arm exactly on the proven owner with live
    /// fences, else refuse without arming.
    pub fn on_down(
        &self,
        hit: DragMouseHit,
        owner: &Result<PressOwner, &'static str>,
        fences: DragFences,
    ) -> HitIntent {
        let mut controller = self
            .controller
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if controller.active() {
            return HitIntent::Down(Downcome::Ignore);
        }
        let owner = match owner {
            Ok(owner) => owner,
            Err(reason) => return HitIntent::Down(Downcome::Refused { reason }),
        };
        if !self.meta_held(hit.flags) {
            return HitIntent::Down(Downcome::Refused {
                reason: "unowned-press",
            });
        }
        match controller.press(
            &owner.window,
            hit.x,
            hit.y,
            true,
            true,
            fences,
            owner.revision,
            owner.start_frame,
        ) {
            Ok(()) => HitIntent::Down(Downcome::Armed {
                window: owner.window.clone(),
            }),
            Err(reason) => HitIntent::Down(Downcome::Refused { reason }),
        }
    }

    /// One coalesced motion sample: zero movement hides transiently,
    /// moved pointers preview through the world gesture route, fence
    /// refusals cancel dead with a terminal ingress for the held gesture.
    pub fn on_drag(&self, x: i32, y: i32, fences: DragFences) -> HitIntent {
        // Snapshot the terminal ingress BEFORE motion clears a fenced
        // gesture: the core must clear the same retained session.
        let cancelled = self.cancel_ingress();
        let mut controller = self
            .controller
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        match controller.motion(x, y, fences) {
            MotionGate::Ignore => HitIntent::Sample(SampleIntent::Ignore),
            MotionGate::ZeroSnapBack => HitIntent::Sample(SampleIntent::Hide),
            MotionGate::Sample => {
                let held = controller.held_window().map(str::to_owned);
                let gid = controller.gesture_id();
                let frame = controller.start_frame();
                let start = controller.start_cursor();
                drop(controller);
                match (held, gid, frame, start) {
                    (Some(window), Some(id), Some(start_frame), Some(start_cursor)) => {
                        HitIntent::Sample(SampleIntent::Preview {
                            ingress: gesture_ingress(
                                id,
                                &window,
                                tiler_core::gesture::GesturePhase::Updated,
                                Some((x, y)),
                                start_cursor,
                                start_frame,
                            ),
                        })
                    }
                    _ => HitIntent::Sample(SampleIntent::Hide),
                }
            }
            MotionGate::CancelDead(reason) => HitIntent::Sample(SampleIntent::Cancelled {
                reason,
                ingress: cancelled,
            }),
        }
    }

    /// One button release: ends the single-flight capture exactly once.
    /// The caller hides the carrier FIRST, then resolves the world gesture
    /// release. The press revision proven at arm rides the drop intent for
    /// the engine-side stale check.
    pub fn on_up(&self, x: i32, y: i32) -> HitIntent {
        let snapshot = self.gesture_snapshot();
        let mut controller = self
            .controller
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let press_revision = controller.press_revision().unwrap_or(0);
        match controller.release() {
            Some(window) => match snapshot {
                Some((id, _, start_frame, start_cursor)) => {
                    HitIntent::Release(ReleaseIntent::Drop {
                        ingress: gesture_ingress(
                            id,
                            &window,
                            tiler_core::gesture::GesturePhase::Released,
                            Some((x, y)),
                            start_cursor,
                            start_frame,
                        ),
                        press_revision,
                    })
                }
                None => HitIntent::Release(ReleaseIntent::Ignore),
            },
            None => HitIntent::Release(ReleaseIntent::Ignore),
        }
    }

    /// Escape while held: cancels with overlay cleared and fail-safe
    /// release. Returns the terminal ingress when a gesture was
    /// cancelled so the core clears the same retained session.
    pub fn on_escape(&self) -> Option<tiler_core::gesture::GestureIngress> {
        let ingress = self.cancel_ingress();
        let cancelled = self
            .controller
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .escape();
        cancelled.then_some(ingress).flatten()
    }

    /// Stale scope/focus/revision fence: cancels dead with overlay cleared
    /// and fail-safe release. Returns the terminal ingress when a gesture
    /// was dropped so the core clears the same retained session.
    pub fn on_stale(&self) -> Option<tiler_core::gesture::GestureIngress> {
        let ingress = self.cancel_ingress();
        let cancelled = self
            .controller
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .note_stale();
        cancelled.then_some(ingress).flatten()
    }

    /// Record one world gesture preview outcome (promotes Armed to
    /// Dragging; transient refusals keep the gesture; dead replies
    /// terminate with overlay cleared and fail-safe release).
    pub fn note_preview(&self, outcome: &PreviewOutcome) {
        self.controller
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .note_preview(outcome);
    }

    /// Snapshot the held gesture mechanics for one ingress build
    /// (id, window, frozen START frame/cursor, press revision).
    fn gesture_snapshot(&self) -> Option<(u64, String, tiler_core::geometry::Rect, (i32, i32))> {
        let controller = self
            .controller
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        Some((
            controller.gesture_id()?,
            controller.held_window()?.to_owned(),
            controller.start_frame()?,
            controller.start_cursor()?,
        ))
    }

    /// Build the bind ingress for a freshly armed gesture (Started binds
    /// only, never previews at the down point itself).
    #[must_use]
    pub fn start_ingress(&self) -> Option<tiler_core::gesture::GestureIngress> {
        let (id, window, start_frame, start_cursor) = self.gesture_snapshot()?;
        Some(gesture_ingress(
            id,
            &window,
            tiler_core::gesture::GesturePhase::Started,
            Some(start_cursor),
            start_cursor,
            start_frame,
        ))
    }

    /// Build the terminal ingress for the held gesture (Released uses the
    /// release point; Cancelled reuses the START point — the core clears
    /// regardless of cursor).
    #[must_use]
    pub fn cancel_ingress(&self) -> Option<tiler_core::gesture::GestureIngress> {
        let (id, window, start_frame, start_cursor) = self.gesture_snapshot()?;
        Some(gesture_ingress(
            id,
            &window,
            tiler_core::gesture::GesturePhase::Cancelled,
            Some(start_cursor),
            start_cursor,
            start_frame,
        ))
    }

    /// Overlay intent for a resolved Engine preview rect: the filled slot
    /// above the target in the configured fill, or hidden. The live
    /// applier maps `Show` to one carrier update (ordered above the
    /// target window) and `Hide` to `orderOut:`.
    #[must_use]
    pub fn overlay_for(&self, rect: tiler_core::geometry::Rect) -> DragOverlay {
        DragController::overlay_for(
            self.options,
            VisualRect {
                x: rect.x as f64,
                y: rect.y as f64,
                w: rect.w as f64,
                h: rect.h as f64,
            },
            DragOrdering::AboveTarget,
        )
    }
}

/// Build one normalized gesture ingress for the held Meta+left gesture:
/// the stable gesture id minted at press, the exact window lifetime, the
/// START-sampled move kind, the live cursor plus the immutable START
/// cursor/frame, and modal truth (a Meta-started drag carries user move
/// intent). The adapter never mints hover priors or drop targets.
#[must_use]
pub fn gesture_ingress(
    id: u64,
    window: &str,
    phase: tiler_core::gesture::GesturePhase,
    cursor: Option<(i32, i32)>,
    start_cursor: (i32, i32),
    start_frame: tiler_core::geometry::Rect,
) -> tiler_core::gesture::GestureIngress {
    tiler_core::gesture::GestureIngress {
        id,
        window: tiler_core::directional::WindowId(window.to_owned()),
        kind: tiler_core::gesture::GestureKind::Move,
        cursor,
        start_cursor: Some(start_cursor),
        phase,
        esc: false,
        modal_move: true,
        start_frame,
    }
}

/// Intent of one drained mouse-tap hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HitIntent {
    Down(Downcome),
    Sample(SampleIntent),
    Release(ReleaseIntent),
}

/// Carrier flag contract for the live preview panel: click-through,
/// nonactivating, normal level, Managed-only (mirrors
/// [`crate::drag_drop::DragCarrierFlags`] over the readback).
#[must_use]
pub const fn drag_flags_ok(flags: &BorderCarrierFlags) -> bool {
    flags.ignores_mouse
        && !flags.can_key
        && !flags.can_main
        && flags.collection_managed_only
        && flags.level == 0
}

/// Carrier frame for an Engine preview rect: CG top-left integers to AppKit
/// bottom-left points (global flip around the primary height; x passes
/// through untouched). `None` refuses (no primary height, non-finite, or
/// degenerate — the caller hides instead of guessing).
#[must_use]
pub fn preview_outer_appkit(
    rect: tiler_core::geometry::Rect,
    primary_h: Option<f64>,
) -> Option<VisualRect> {
    let primary_h = primary_h.filter(|h| h.is_finite() && *h > 0.0)?;
    let (x, y, w, h) = (
        f64::from(rect.x),
        f64::from(rect.y),
        f64::from(rect.w),
        f64::from(rect.h),
    );
    if !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() {
        return None;
    }
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    Some(VisualRect {
        x,
        y: primary_h - (y + h),
        w,
        h,
    })
}

/// Target window number for above-target ordering: parse the runtime
/// `pid_wid` token. `None` means the target is unjoined: the caller drops
/// the show (hides) rather than rendering frontmost — an unknown target
/// never renders. Ordering itself still reads back via the CG join below,
/// never assumed.
#[must_use]
pub fn preview_target_wid(target: &tiler_core::directional::WindowId) -> Option<u32> {
    crate::runtime_types::parse_window_id(&target.0).map(|(_, wid)| wid)
}

/// Pure above-target ordering verdict over one front-to-back CG window
/// order: both windows must be present and the carrier must precede the
/// target. Foreign windows anywhere move indices but never change the
/// pairwise verdict. The live applier hides on `false` (never assumes the
/// show landed); the signed fixture proves the live readback later.
#[must_use]
pub fn carrier_above_verified(
    order_front_to_back: &[u32],
    carrier_wid: u32,
    target_wid: u32,
) -> bool {
    if carrier_wid == 0 || target_wid == 0 {
        return false;
    }
    let mut carrier_idx: Option<usize> = None;
    let mut target_idx: Option<usize> = None;
    for (i, wid) in order_front_to_back.iter().enumerate() {
        if *wid == carrier_wid && carrier_idx.is_none() {
            carrier_idx = Some(i);
        }
        if *wid == target_wid && target_idx.is_none() {
            target_idx = Some(i);
        }
    }
    match (carrier_idx, target_idx) {
        (Some(carrier), Some(target)) => carrier < target,
        _ => false,
    }
}

#[cfg(target_os = "macos")]
pub use live::{
    DragCarrierGuard, DragCarrierReadback, DragRunState, drag_turn_once, park_drag_for_lock,
};

/// Live carrier edge: macOS-only (AppKit panels are main-thread handles).
#[cfg(target_os = "macos")]
mod live {
    use std::sync::Arc;

    use super::*;

    /// Readback of one carrier application (plain data for terminal
    /// evidence; never titles or content).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct DragCarrierReadback {
        /// Carrier now visible.
        pub visible: bool,
        /// Carrier window number (`-1` when absent).
        pub wid: i64,
        /// Flag contract held (click-through, nonactivating, normal
        /// level, Managed-only).
        pub flags_ok: bool,
        /// Created on this call (first show).
        pub created: bool,
        /// Global CG order verified after the show (carrier above the
        /// target; never assumed from the order call alone).
        pub order_verified: bool,
    }

    /// Owned drag-preview carrier on the tile main thread: created on the
    /// first show, updated only on observed change, hidden on every hide
    /// intent and before any drop mutation, destroyed here at run stop on
    /// every path. No activation ever; the fixture-accepted
    /// create/update/hide/show/destroy surface is the only AppKit touch.
    pub struct DragCarrierGuard {
        carrier: Option<crate::ffi::GroupUnderlayCarrier>,
        mtm: objc2::MainThreadMarker,
    }

    impl DragCarrierGuard {
        /// New empty guard (no AppKit touch until the first show).
        #[must_use]
        pub fn new(mtm: objc2::MainThreadMarker) -> Self {
            Self { carrier: None, mtm }
        }

        /// Show the filled slot above the joined target window. Creates
        /// on first show with the configured fill. An unjoined target
        /// (`None`) drops the show (hides): an unknown target never
        /// renders frontmost. After the order call the global CG order
        /// reads back through [`crate::ffi::cg_carrier_above_target`];
        /// a failed verification hides (never assumes the show landed).
        /// Returns the readback (visible plus flag contract plus the
        /// ordering verdict).
        pub fn show_fill(
            &mut self,
            outer: VisualRect,
            fill: (u8, u8, u8, u8),
            target_wid: Option<u32>,
        ) -> DragCarrierReadback {
            let hidden = |wid: i64, flags_ok: bool, created: bool| DragCarrierReadback {
                visible: false,
                wid,
                flags_ok,
                created,
                order_verified: false,
            };
            let Some(target_wid) = target_wid.filter(|wid| *wid != 0) else {
                // Unknown target: drop the show (hide any stale carrier).
                self.hide();
                let (wid, flags_ok, created) = match self.carrier.as_ref() {
                    Some(carrier) => (
                        carrier.window_number(),
                        drag_flags_ok(&carrier.flags()),
                        false,
                    ),
                    None => (-1, false, false),
                };
                return hidden(wid, flags_ok, created);
            };
            let created = self.carrier.is_none();
            if self.carrier.is_none() {
                match crate::ffi::GroupUnderlayCarrier::create(
                    self.mtm,
                    outer,
                    fill,
                    "OmniTiler drag preview",
                ) {
                    Ok(carrier) => self.carrier = Some(carrier),
                    Err(_) => {
                        return hidden(-1, false, false);
                    }
                }
            }
            let Some(carrier) = self.carrier.as_ref() else {
                return hidden(-1, false, false);
            };
            // Degenerate updates keep the previous frame (never a guessed
            // clamp); a failed update hides rather than showing stale.
            if carrier.update(self.mtm, outer, fill).is_err() {
                carrier.hide(self.mtm);
                return hidden(
                    carrier.window_number(),
                    drag_flags_ok(&carrier.flags()),
                    created,
                );
            }
            carrier.show_above(self.mtm, target_wid as isize);
            // Actual CG order readback (never assumed from the call).
            let verified = crate::ffi::cg_window_list_all()
                .ok()
                .map(|windows| {
                    let (above, _, _) = crate::ffi::cg_carrier_above_target(
                        &windows,
                        carrier.window_number() as isize,
                        target_wid as isize,
                    );
                    above
                })
                .unwrap_or(false);
            if !verified {
                carrier.hide(self.mtm);
            }
            DragCarrierReadback {
                visible: verified && carrier.is_visible(),
                wid: carrier.window_number(),
                flags_ok: drag_flags_ok(&carrier.flags()),
                created,
                order_verified: verified,
            }
        }

        /// Fence hide: `orderOut:` plus the visibility readback.
        pub fn hide(&self) -> bool {
            if let Some(carrier) = self.carrier.as_ref() {
                carrier.hide(self.mtm);
                return !carrier.is_visible();
            }
            true
        }
    }

    impl Drop for DragCarrierGuard {
        fn drop(&mut self) {
            // Exact-PID lifecycle: the owned panel dies with the session
            // on every path (the watchdog forced exit excepted, where the
            // OS reaps the panel with the process).
            if let Some(carrier) = self.carrier.take() {
                carrier.destroy(self.mtm);
            }
        }
    }
    /// Owned Meta+left drag-preview route on the tile main thread (one
    /// [`crate::drag_session::DragSession`] plus one filled carrier plus
    /// one mouse-tap owner thread). Armed only on the tap input route with
    /// the startup drag switch on; the off route and a disabled switch
    /// park with a terminal reason (never a silent no-op). The tap
    /// callback consults only the lock-free tap gate (pre-proven snapshot
    /// plus capture); this thread owns everything else. Shutdown stops
    /// exactly this thread (own `Arc` stop flag plus join — never a
    /// process-wide kill) and destroys the carrier through its guard
    /// `Drop`.
    pub struct DragRunState {
        session: DragSession,
        rx: std::sync::mpsc::Receiver<crate::ffi::DragMouseHit>,
        escape_rx: std::sync::mpsc::Receiver<crate::ffi::TapHit>,
        escape_pending: Arc<std::sync::atomic::AtomicBool>,
        tap_gate: Arc<crate::drag_drop::DragTapGate>,
        carrier: DragCarrierGuard,
        stop: Arc<std::sync::atomic::AtomicBool>,
        gate: Arc<std::sync::atomic::AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
        report_slot: Arc<std::sync::Mutex<crate::ffi::TapSourceReport>>,
        refusal_slot: Arc<std::sync::Mutex<Option<String>>>,
        pub previews: u64,
        pub drops: u64,
        pub cancels: u64,
    }

    impl DragRunState {
        /// Parked terminal reason when the route never armed
        /// (`input_is_tap` names the tap input route; the off route and
        /// a disabled switch park, never a silent no-op). While the live
        /// acceptance gate ([`super::DRAG_LIVE_ACCEPTED`]) is closed this
        /// parks EVERY combination with the `live-acceptance-pending`
        /// reason, so ordinary product runtime installs no mouse tap at
        /// all (diagnostic-only scaffolding; see
        /// [`super::drag_arm_parked_reason`]).
        pub fn parked_reason(drag_enabled: bool, input_is_tap: bool) -> Option<String> {
            super::drag_arm_parked_reason(drag_enabled, input_is_tap)
        }

        /// Arm the route: session plus carrier plus the mouse-tap owner
        /// thread. PARKED while the live acceptance gate
        /// ([`super::DRAG_LIVE_ACCEPTED`]) is closed: refuses without
        /// installing any tap (diagnostic-only; ordinary product runtime
        /// never reaches here because `parked_reason` already parked it).
        /// A tap-creation denial parks the arm with its reason
        /// (keyboard tiling continues; the denial is terminal evidence,
        /// never a silent loss). `escape_armed` stays set while the route
        /// lives so the keyboard tap forwards Escape edges unsuppressed
        /// (the coalesced pending flag means a racing Escape is never
        /// missed; the drag turn consumes the cancel only while a gesture
        /// holds). The mouse channel is unbounded so a consumed edge is
        /// never lost to a full queue.
        pub fn arm(
            drag: DragOptions,
            modifier: ProbeModifier,
            mtm: objc2::MainThreadMarker,
            escape_armed: &Arc<std::sync::atomic::AtomicBool>,
            escape_pending: &Arc<std::sync::atomic::AtomicBool>,
            escape_rx: std::sync::mpsc::Receiver<crate::ffi::TapHit>,
        ) -> Result<Self, String> {
            use std::sync::atomic::Ordering;
            if !super::DRAG_LIVE_ACCEPTED {
                return Err(super::DRAG_PARKED_REASON.to_owned());
            }
            let session = DragSession::new(drag, modifier);
            let (meta_want, meta_mask) = session.meta_bits();
            let tap_gate = Arc::new(crate::drag_drop::DragTapGate::new());
            let (tx, rx) = std::sync::mpsc::channel::<crate::ffi::DragMouseHit>();
            let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let gate = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let report_slot = Arc::new(std::sync::Mutex::new(crate::ffi::TapSourceReport {
                matched: 0,
                suppressed: 0,
                deliveries: 0,
                reenables_after_timeout: 0,
                user_disables: 0,
                unnotified_disables: 0,
            }));
            let refusal_slot = Arc::new(std::sync::Mutex::new(None::<String>));
            // The tap guard is thread-bound (runloop source); the owner
            // thread below is its only home. Creation runs there too, so
            // a denial lands in the refusal slot (never a panic).
            let thread_stop = Arc::clone(&stop);
            let thread_gate = Arc::clone(&gate);
            let thread_report = Arc::clone(&report_slot);
            let thread_refusal = Arc::clone(&refusal_slot);
            let thread_tap_gate = Arc::clone(&tap_gate);
            let thread = std::thread::Builder::new()
                .name("drag-tap".to_owned())
                .spawn(move || {
                    drag_tap_owner(
                        thread_tap_gate,
                        meta_want,
                        meta_mask,
                        tx,
                        thread_stop,
                        thread_gate,
                        thread_report,
                        thread_refusal,
                    );
                })
                .map_err(|reason| format!("drag tap thread spawn refused: {reason}"))?;
            escape_armed.store(true, Ordering::SeqCst);
            Ok(Self {
                session,
                rx,
                escape_rx,
                escape_pending: Arc::clone(escape_pending),
                tap_gate,
                carrier: DragCarrierGuard::new(mtm),
                stop,
                gate,
                thread: Some(thread),
                report_slot,
                refusal_slot,
                previews: 0,
                drops: 0,
                cancels: 0,
            })
        }

        /// Stop exactly the owned tap thread (own stop flag plus join;
        /// never a process-wide signal) and publish its final report.
        /// The carrier destroys through its guard `Drop` at state drop.
        pub fn stop_owner(&mut self) -> (crate::ffi::TapSourceReport, Option<String>) {
            use std::sync::atomic::Ordering;
            self.stop.store(true, Ordering::SeqCst);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
            let report = *self
                .report_slot
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            let refusal = self
                .refusal_slot
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .clone();
            (report, refusal)
        }
    }

    /// Mouse-tap owner thread body (its only home for the thread-bound
    /// guard): create, bounded pump slices, fresh-guard recreation on
    /// unverified re-enable, gate release on lock/Secure Input. A
    /// creation/recreation denial records the refusal and parks (tiling
    /// continues without the preview arm). Never blocks the tap callback
    /// (pump slices bound stop latency) and never touches AX/core/log.
    #[allow(clippy::too_many_arguments)]
    fn drag_tap_owner(
        tap_gate: Arc<crate::drag_drop::DragTapGate>,
        meta_want: u64,
        meta_mask: u64,
        tx: std::sync::mpsc::Sender<crate::ffi::DragMouseHit>,
        stop: Arc<std::sync::atomic::AtomicBool>,
        gate: Arc<std::sync::atomic::AtomicBool>,
        report_slot: Arc<std::sync::Mutex<crate::ffi::TapSourceReport>>,
        refusal_slot: Arc<std::sync::Mutex<Option<String>>>,
    ) {
        use std::sync::atomic::Ordering;
        let publish = |source: &crate::ffi::DragTapSource| {
            *report_slot
                .lock()
                .unwrap_or_else(|poison| poison.into_inner()) = source.report();
        };
        let mut source =
            match crate::ffi::DragTapSource::create(&tap_gate, meta_want, meta_mask, &tx) {
                Ok(source) => source,
                Err(reason) => {
                    *refusal_slot
                        .lock()
                        .unwrap_or_else(|poison| poison.into_inner()) =
                        Some(format!("drag tap refused: {reason}"));
                    return;
                }
            };
        let mut was_released = false;
        loop {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            let released = gate.load(Ordering::SeqCst);
            source.set_enabled(!released);
            if released {
                // Park released while gated; the return recreates fresh
                // (never a second same-tap enable after a stall).
                was_released = true;
                source.pump(100);
                publish(&source);
                continue;
            }
            if was_released {
                was_released = false;
                match crate::ffi::DragTapSource::create(&tap_gate, meta_want, meta_mask, &tx) {
                    Ok(fresh) => {
                        publish(&source);
                        source = fresh;
                    }
                    Err(reason) => {
                        publish(&source);
                        *refusal_slot
                            .lock()
                            .unwrap_or_else(|poison| poison.into_inner()) =
                            Some(format!("drag tap re-create refused: {reason}"));
                        return;
                    }
                }
            }
            match source.pump(100) {
                crate::ffi::TapPump::Held | crate::ffi::TapPump::TimeoutRecovered => {}
                crate::ffi::TapPump::NeedsRecreate => {
                    match crate::ffi::DragTapSource::create(&tap_gate, meta_want, meta_mask, &tx) {
                        Ok(fresh) => {
                            publish(&source);
                            source = fresh;
                        }
                        Err(reason) => {
                            publish(&source);
                            *refusal_slot
                                .lock()
                                .unwrap_or_else(|poison| poison.into_inner()) =
                                Some(format!("drag tap re-create refused: {reason}"));
                            return;
                        }
                    }
                }
                crate::ffi::TapPump::UserDisabled => {
                    was_released = true;
                }
            }
            publish(&source);
        }
        publish(&source);
    }

    /// Plain per-iteration reads for one drag turn (the tile loop fills
    /// these from live state it already holds; no OS touch here).
    struct DragTurnReads {
        session_unlocked: bool,
        secure_active: bool,
        frontmost_pid: Option<u32>,
        scope_allowed: bool,
        context_revision: u64,
        primary_h: Option<f64>,
        now_ms: u64,
    }

    /// Park the drag arm across a lock-screen interval: cancel any held
    /// gesture stale (overlay cleared, fail-safe release), invalidate the
    /// tap snapshot, and hide the carrier. Called on the lock/loginwindow
    /// branches that skip the normal turn below.
    pub fn park_drag_for_lock(state: &mut DragRunState) {
        if state.session.on_stale().is_some() {
            state.cancels += 1;
        }
        state.tap_gate.invalidate();
        state.carrier.hide();
    }

    /// Apply one Engine preview outcome to the carrier: a resolved slot
    /// shows the filled rect above the target window in the configured
    /// fill; every refusal hides. Ordering itself is never assumed here —
    /// the live fixture reads it back via the CG join.
    fn apply_preview_outcome(
        state: &mut DragRunState,
        outcome: &crate::drag_drop::PreviewOutcome,
        primary_h: Option<f64>,
    ) {
        use crate::drag_drop::{DragOrdering, DragOverlay, PreviewOutcome};
        match outcome {
            PreviewOutcome::Show { rect, target, .. } => match state.session.overlay_for(*rect) {
                DragOverlay::ShowFillAbove {
                    fill,
                    ordering: DragOrdering::AboveTarget,
                    ..
                } => match super::preview_outer_appkit(*rect, primary_h) {
                    Some(outer) => {
                        state
                            .carrier
                            .show_fill(outer, fill, super::preview_target_wid(target));
                    }
                    None => {
                        state.carrier.hide();
                    }
                },
                DragOverlay::Hide => {
                    state.carrier.hide();
                }
            },
            PreviewOutcome::HideTransient { .. } | PreviewOutcome::CancelDead { .. } => {
                state.carrier.hide();
            }
        }
    }

    /// One drag turn on the tile main thread: gate the tap, drain both
    /// channels (mouse batches coalesce to the latest sample; Escape wins
    /// over release), fence against published facts, resolve exactly one
    /// Engine contact per turn (preview query or drop submit), and apply
    /// the carrier. The drop hides the carrier BEFORE submitting
    /// (hide-before-mutation); the tap callback never blocks (it only
    /// ever consults the lock-free gate). A disconnected mouse channel
    /// (tap thread death) cancels any held gesture stale rather than
    /// stranding it. Remainder batches stay queued for the next turn
    /// (retain/retry, never silently dropped).
    fn drive_drag_turn(
        state: &mut DragRunState,
        handle: &crate::runtime::RuntimeHandle,
        border_store: &crate::border_runtime::BorderFactsStore,
        reads: &DragTurnReads,
    ) {
        use super::{DragPlan, SampleGuards, fences_for, resolve_press_owner};
        use std::sync::atomic::Ordering;
        // Tap gate follows lock/Secure Input (released = pass untouched).
        state.gate.store(
            !reads.session_unlocked || reads.secure_active,
            Ordering::SeqCst,
        );
        // Escape arrives on the channel plus the coalesced pending flag
        // (the flag survives a full channel; the turn drains both, and the
        // session consumes the cancel only while a gesture holds).
        let mut escape_pressed = state.escape_pending.swap(false, Ordering::SeqCst);
        while state.escape_rx.try_recv().is_ok() {
            escape_pressed = true;
        }
        let mut hits = Vec::new();
        let mut mouse_dead = false;
        loop {
            match state.rx.try_recv() {
                Ok(hit) => {
                    hits.push(hit);
                    if hits.len() >= 1024 {
                        break;
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    mouse_dead = true;
                    break;
                }
            }
        }
        if mouse_dead {
            // Tap thread death: never strand a held gesture. Cancel stale
            // with the overlay cleared and the snapshot invalidated.
            if state.session.on_stale().is_some() {
                state.cancels += 1;
                eprintln!("tiler-macos dev drag tap disconnected; cancelled stale");
            }
            state.tap_gate.invalidate();
            state.carrier.hide();
            if hits.is_empty() && !escape_pressed {
                return;
            }
        }
        if hits.is_empty() && !escape_pressed && !state.session.active() {
            publish_idle_eligibility(state, border_store, reads);
            return;
        }
        // Owner proof for a fresh press: the frontmost PID's published
        // focused record containing the down point, JOINED with a fresh
        // off-tap AX hit-test owner plus the window-server hit window at
        // the exact down point (all three must name the same window;
        // frame containment alone never proves). Held gestures skip this
        // (their owner is press-proven; per-sample focus re-fences below
        // instead).
        let need_owner = !state.session.active()
            && hits
                .iter()
                .any(|hit| hit.kind == crate::ffi::DragMouseKind::Down);
        let owner = if need_owner {
            match (
                reads.frontmost_pid,
                hits.iter()
                    .find(|hit| hit.kind == crate::ffi::DragMouseKind::Down),
            ) {
                (Some(pid), Some(down)) => {
                    let (ax_owner, cg_hit) = fresh_point_proof(down.x, down.y);
                    resolve_press_owner(
                        border_store.snapshot_for(pid).as_ref(),
                        pid,
                        down.x,
                        down.y,
                        reads.now_ms,
                        reads.context_revision,
                        ax_owner,
                        cg_hit,
                    )
                }
                _ => Err("unknown-window"),
            }
        } else {
            Err("focus-changed")
        };
        // Per-sample fences over the armed owner's published record (or
        // the frontmost record before arming): focus agreement, revision
        // plus freshness, mover/frontmost/ANY-visible fullscreen (game
        // safety covers a fullscreen window on the pointer display even
        // when the mover is windowed). Scope rides the loop's
        // frontmost-bundle verdict (same rule as the border tick).
        let owner_pid: Option<u32> = state
            .session
            .controller()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .held_window()
            .map(str::to_owned)
            .and_then(|window| crate::runtime_types::parse_window_id(&window).map(|(pid, _)| pid))
            .or(reads.frontmost_pid);
        let owner_record = owner_pid.and_then(|pid| border_store.snapshot_for(pid));
        let frontmost_record = reads
            .frontmost_pid
            .and_then(|pid| border_store.snapshot_for(pid));
        let (focus_matches, revision_fresh, mover_fullscreen) = match owner_record.as_ref() {
            Some(record) => {
                let wid_ok = record.focused_wid.filter(|w| *w != 0);
                let agreed = match (wid_ok, owner_pid) {
                    (Some(wid), Some(pid)) => {
                        record.pid == pid
                            && record.selected == Some((pid, Some(wid)))
                            && record.window.as_ref().is_some_and(|w| w.wid == wid)
                    }
                    _ => false,
                };
                let frontmost_ok = owner_pid.is_some_and(|pid| reads.frontmost_pid == Some(pid));
                let revision_ok = record.revision == reads.context_revision
                    && crate::input_eligibility::is_fresh_at(
                        record.updated_ms,
                        reads.now_ms,
                        crate::input_eligibility::FACTS_TTL.as_millis() as u64,
                    );
                let fullscreen = record
                    .window
                    .as_ref()
                    .is_some_and(|window| window.ax.fullscreen);
                (agreed && frontmost_ok, revision_ok, fullscreen)
            }
            None => (false, false, false),
        };
        let frontmost_fullscreen = frontmost_record
            .as_ref()
            .and_then(|record| record.window.as_ref())
            .is_some_and(|window| window.ax.fullscreen);
        let guards = SampleGuards {
            session_unlocked: reads.session_unlocked,
            secure_input_active: reads.secure_active,
            scope_allowed: reads.scope_allowed,
            fullscreen_active: mover_fullscreen
                || frontmost_fullscreen
                || border_store.any_fullscreen(),
            revision_fresh,
            focus_matches,
        };
        let fences = fences_for(state.session.options().enabled, &guards);
        // Press revision proven at arm (drives the engine-side stale
        // fence for Started/Updated; Released carries its own).
        let press_revision = state
            .session
            .controller()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .press_revision();
        match state
            .session
            .plan_batch(&hits, escape_pressed, &owner, fences)
        {
            DragPlan::Idle => {}
            DragPlan::Hide => {
                state.carrier.hide();
            }
            DragPlan::Cancelled { reason, ingress } => {
                state.carrier.hide();
                if let Some(ingress) = ingress {
                    // Best-effort terminal: the core clears the same
                    // retained session; a lost reply still cancelled
                    // main-side exactly once.
                    let _ = handle.submit_drag_gesture(ingress, None, super::PREVIEW_QUERY_TIMEOUT);
                }
                state.cancels += 1;
                eprintln!("tiler-macos dev drag cancelled reason={reason}");
            }
            DragPlan::Start(ingress) => {
                match handle.submit_drag_gesture(
                    ingress,
                    press_revision,
                    super::PREVIEW_QUERY_TIMEOUT,
                ) {
                    Some(reply) => {
                        state.previews += 1;
                        state.session.note_preview(&reply.preview);
                        apply_preview_outcome(state, &reply.preview, reads.primary_h);
                        if matches!(
                            reply.preview,
                            crate::drag_drop::PreviewOutcome::CancelDead { .. }
                        ) {
                            state.cancels += 1;
                        }
                    }
                    None => {
                        state.session.on_stale();
                        state.carrier.hide();
                        state.cancels += 1;
                        eprintln!("tiler-macos dev drag start timed out; cancelled stale");
                    }
                }
            }
            DragPlan::Preview(ingress) => {
                match handle.submit_drag_gesture(
                    ingress,
                    press_revision,
                    super::PREVIEW_QUERY_TIMEOUT,
                ) {
                    Some(reply) => {
                        state.previews += 1;
                        state.session.note_preview(&reply.preview);
                        apply_preview_outcome(state, &reply.preview, reads.primary_h);
                        if matches!(
                            reply.preview,
                            crate::drag_drop::PreviewOutcome::CancelDead { .. }
                        ) {
                            state.cancels += 1;
                        }
                    }
                    None => {
                        state.session.on_stale();
                        state.carrier.hide();
                        state.cancels += 1;
                        eprintln!("tiler-macos dev drag preview query timed out; cancelled stale");
                    }
                }
            }
            DragPlan::Drop {
                ingress,
                press_revision,
            } => {
                // Hide-before-mutation: the overlay clears before the
                // core commits or any native frame moves.
                state.carrier.hide();
                match handle.submit_drag_gesture(
                    ingress,
                    Some(press_revision),
                    super::PREVIEW_QUERY_TIMEOUT,
                ) {
                    Some(reply) => match reply.settled {
                        Some(settled) if settled.committed => {
                            state.drops += 1;
                        }
                        _ => {
                            state.cancels += 1;
                            eprintln!("tiler-macos dev drag drop settled without commit");
                        }
                    },
                    None => {
                        state.cancels += 1;
                        eprintln!("tiler-macos dev drag drop timed out; cancelled stale");
                    }
                }
            }
        }
        // While idle, refresh the tap's pre-proven snapshot for the next
        // down (a held gesture owns its capture; no snapshot needed).
        if !state.session.active() {
            publish_idle_eligibility(state, border_store, reads);
        }
    }

    /// Fresh off-tap positional proof at one point (main thread only, never
    /// the tap callback): the AX hit-test owner plus the window-server hit
    /// window. Unreadable legs are `None` (the join refuses, never guesses).
    fn fresh_point_proof(x: i32, y: i32) -> (Option<(u32, u32)>, Option<u32>) {
        let (fx, fy) = (f64::from(x), f64::from(y));
        let ax_owner = crate::ffi::ax_hit_test_owner(fx, fy).ok();
        let cg_hit = crate::ffi::sls_window_at_point(fx, fy).ok();
        (ax_owner, cg_hit)
    }

    /// Publish one pre-proven eligibility snapshot for the next Meta+left
    /// down while idle: the live cursor point joined with a fresh AX+CG
    /// owner proof plus the turn's fences over the frontmost record. The
    /// Meta chord never moves focus, so the cursor owner must equal the
    /// frontmost focused window; anything else (occluder, menu, foreign
    /// window, drifted fences, unreadable proof) invalidates and the next
    /// down passes untouched. Bounded off-tap reads only; the tap callback
    /// never performs them.
    ///
    /// The previous publication is invalidated BEFORE the new proof starts,
    /// so a down racing the proof computation fails open on no snapshot
    /// rather than consuming on a stale point; the fresh snapshot publishes
    /// only after the full join plus fences pass, stamped with the shared
    /// monotonic clock for the tap-side age bound
    /// ([`crate::drag_drop::PRESS_PROOF_MAX_AGE_MS`]).
    fn publish_idle_eligibility(
        state: &mut DragRunState,
        border_store: &crate::border_runtime::BorderFactsStore,
        reads: &DragTurnReads,
    ) {
        use super::{fences_for, resolve_press_owner};
        state.tap_gate.invalidate();
        let Some((fx, fy)) = crate::synth::cursor_position() else {
            return;
        };
        if !fx.is_finite() || !fy.is_finite() {
            state.tap_gate.invalidate();
            return;
        }
        let x = fx.round().clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
        let y = fy.round().clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
        let Some(pid) = reads.frontmost_pid else {
            state.tap_gate.invalidate();
            return;
        };
        let (ax_owner, cg_hit) = fresh_point_proof(x, y);
        // The cursor owner must be the frontmost app (focus at press); a
        // cursor over any other process invalidates. Window identity rides
        // the resolve below (AX owner plus CG hit must equal the focused
        // window exactly).
        if !ax_owner.is_some_and(|(apid, _)| apid == pid) {
            state.tap_gate.invalidate();
            return;
        }
        let owner = resolve_press_owner(
            border_store.snapshot_for(pid).as_ref(),
            pid,
            x,
            y,
            reads.now_ms,
            reads.context_revision,
            ax_owner,
            cg_hit,
        );
        let Ok(proven) = owner else {
            state.tap_gate.invalidate();
            return;
        };
        // Turn fences for the proven owner: focus agreement plus revision
        // freshness plus ANY-visible fullscreen plus the loop verdicts.
        let record = border_store.snapshot_for(pid);
        let revision_fresh = record.as_ref().is_some_and(|record| {
            record.revision == reads.context_revision
                && crate::input_eligibility::is_fresh_at(
                    record.updated_ms,
                    reads.now_ms,
                    crate::input_eligibility::FACTS_TTL.as_millis() as u64,
                )
        });
        let focus_matches = record.as_ref().is_some_and(|record| {
            record.focused_wid.filter(|w| *w != 0) == Some(proven.wid)
                && record.selected == Some((pid, Some(proven.wid)))
        });
        let guards = super::SampleGuards {
            session_unlocked: reads.session_unlocked,
            secure_input_active: reads.secure_active,
            scope_allowed: reads.scope_allowed,
            fullscreen_active: proven.fullscreen || border_store.any_fullscreen(),
            revision_fresh,
            focus_matches,
        };
        let fences = fences_for(state.session.options().enabled, &guards);
        if fences.refusal().is_some() {
            state.tap_gate.invalidate();
            return;
        }
        state.tap_gate.publish(proven.pid, proven.wid, x, y, true);
    }

    /// One Meta+left drag turn (armed routes only): join the frontmost
    /// PID's owner-published facts with the live verdict and drive the
    /// session plus one Engine contact plus the filled carrier. Mirrors
    /// the border tick's join legs (frontmost, scope, revision, primary
    /// height, millisecond clock) plus bounded off-tap AX+CG point joins
    /// for the press proof and the idle eligibility snapshot (never on the
    /// tap callback).
    #[allow(clippy::too_many_arguments)]
    pub fn drag_turn_once(
        selection: &crate::dev_selection::TileScope,
        border_store: &crate::border_runtime::BorderFactsStore,
        facts_store: &crate::input_eligibility::EligibilityFactsStore,
        state: &mut DragRunState,
        snapshot: Option<&crate::topology::TopologySnapshot>,
        handle: &crate::runtime::RuntimeHandle,
    ) {
        let frontmost = crate::ffi::frontmost_app();
        let (_, context_revision) = facts_store.context();
        let reads = DragTurnReads {
            session_unlocked: matches!(
                crate::ffi::session_state().lock,
                crate::ffi::SessionLock::Unlocked
            ),
            secure_active: matches!(crate::ffi::secure_input_known(), Some(true)),
            frontmost_pid: frontmost
                .as_ref()
                .filter(|(pid, _)| *pid > 1)
                .map(|(pid, _)| *pid as u32),
            scope_allowed: frontmost
                .as_ref()
                .is_some_and(|(_, bundle)| selection.allows_effect(Some(bundle))),
            context_revision,
            primary_h: snapshot
                .and_then(|s| crate::border_runtime::snapshot_primary_height(&s.displays)),
            now_ms: border_store.now_ms(),
        };
        drive_drag_turn(state, handle, border_store, &reads);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::border_runtime::{BorderAppRecord, BorderWindowFacts};
    use crate::classifier::{ActivationPolicy, AppFacts, AxFacts, CgFacts};
    use crate::config::ProbeModifier;
    use crate::drag_drop::{DragOptions, MotionGate};
    use crate::ffi::AxFrame;
    use tiler_core::directional::WindowId;

    const CONTROL_FLAGS: u64 = 0x0004_0000;

    fn session() -> DragSession {
        DragSession::new(DragOptions::default(), ProbeModifier::Ctrl)
    }

    fn fences_ok() -> DragFences {
        DragFences {
            enabled: true,
            session_unlocked: true,
            secure_input_active: false,
            scope_allowed: true,
            fullscreen_active: false,
            revision_fresh: true,
            focus_matches: true,
        }
    }

    fn owner_record(pid: u32, wid: u32, now_ms: u64) -> BorderAppRecord {
        BorderAppRecord {
            pid,
            bundle: Some("com.apple.TextEdit".to_owned()),
            focused_wid: Some(wid),
            selected: Some((pid, Some(wid))),
            window: Some(BorderWindowFacts {
                wid,
                frame: AxFrame {
                    x: 100.0,
                    y: 100.0,
                    w: 400.0,
                    h: 300.0,
                },
                ax: AxFacts::standard(),
                cg: CgFacts::normal(),
                app: AppFacts::regular(),
                spaces: vec![11],
            }),
            revision: 9,
            updated_ms: now_ms,
        }
    }

    fn owner_ok() -> Result<PressOwner, &'static str> {
        Ok(PressOwner {
            pid: 4242,
            wid: 7,
            window: window_id(4242, 7),
            fullscreen: false,
            revision: 9,
            start_frame: tiler_core::geometry::Rect {
                x: 100,
                y: 100,
                w: 400,
                h: 300,
            },
        })
    }

    fn down_hit(x: i32, y: i32) -> DragMouseHit {
        DragMouseHit {
            kind: DragMouseKind::Down,
            x,
            y,
            flags: CONTROL_FLAGS,
        }
    }

    fn drag_hit(x: i32, y: i32) -> DragMouseHit {
        DragMouseHit {
            kind: DragMouseKind::Drag,
            x,
            y,
            flags: CONTROL_FLAGS,
        }
    }

    fn up_hit(x: i32, y: i32) -> DragMouseHit {
        DragMouseHit {
            kind: DragMouseKind::Up,
            x,
            y,
            flags: CONTROL_FLAGS,
        }
    }

    #[test]
    fn press_owner_proves_focused_frame_member() {
        let record = owner_record(4242, 7, 1000);
        let owner = resolve_press_owner(
            Some(&record),
            4242,
            200,
            200,
            1000,
            9,
            Some((4242, 7)),
            Some(7),
        )
        .expect("owns");
        assert_eq!(owner.pid, 4242);
        assert_eq!(owner.wid, 7);
        assert_eq!(owner.window, "4242_7");
        assert!(!owner.fullscreen);
        // Edges inclusive.
        assert!(
            resolve_press_owner(
                Some(&record),
                4242,
                100,
                100,
                1000,
                9,
                Some((4242, 7)),
                Some(7)
            )
            .is_ok()
        );
        assert!(
            resolve_press_owner(
                Some(&record),
                4242,
                500,
                400,
                1000,
                9,
                Some((4242, 7)),
                Some(7)
            )
            .is_ok()
        );
        // Outside the frame refuses (genuine origin only).
        assert_eq!(
            resolve_press_owner(
                Some(&record),
                4242,
                50,
                50,
                1000,
                9,
                Some((4242, 7)),
                Some(7)
            ),
            Err("outside-frame")
        );
        // No record / wrong pid refuses.
        assert_eq!(
            resolve_press_owner(None, 4242, 200, 200, 1000, 9, Some((4242, 7)), Some(7)),
            Err("unknown-window")
        );
        assert_eq!(
            resolve_press_owner(
                Some(&record),
                9999,
                200,
                200,
                1000,
                9,
                Some((4242, 7)),
                Some(7)
            ),
            Err("focus-changed")
        );
        assert_eq!(
            resolve_press_owner(
                Some(&record),
                0,
                200,
                200,
                1000,
                9,
                Some((4242, 7)),
                Some(7)
            ),
            Err("unknown-window")
        );
        // Selected-oracle disagreement refuses.
        let mut disagreed = record.clone();
        disagreed.selected = Some((4242, Some(8)));
        assert_eq!(
            resolve_press_owner(
                Some(&disagreed),
                4242,
                200,
                200,
                1000,
                9,
                Some((4242, 7)),
                Some(7)
            ),
            Err("focus-changed")
        );
        // Revision drift (context or stale facts) refuses.
        assert_eq!(
            resolve_press_owner(
                Some(&record),
                4242,
                200,
                200,
                1000,
                10,
                Some((4242, 7)),
                Some(7)
            ),
            Err("revision-drift")
        );
        assert_eq!(
            resolve_press_owner(
                Some(&record),
                4242,
                200,
                200,
                1000 + 5000,
                9,
                Some((4242, 7)),
                Some(7)
            ),
            Err("revision-drift")
        );
        // Degenerate frame refuses.
        let mut bad = record.clone();
        bad.window.as_mut().expect("window").frame.w = 0.0;
        assert_eq!(
            resolve_press_owner(
                Some(&bad),
                4242,
                200,
                200,
                1000,
                9,
                Some((4242, 7)),
                Some(7)
            ),
            Err("bad-geometry")
        );
        // Fresh positional proof refuses without frame excuses: an AX hit
        // on a foreign occluder, a window-server hit elsewhere, or an
        // unreadable join never arms (frame containment alone proves
        // nothing).
        assert_eq!(
            resolve_press_owner(
                Some(&record),
                4242,
                200,
                200,
                1000,
                9,
                Some((4242, 8)),
                Some(7)
            ),
            Err("identity-mismatch")
        );
        assert_eq!(
            resolve_press_owner(
                Some(&record),
                4242,
                200,
                200,
                1000,
                9,
                Some((4242, 7)),
                Some(99)
            ),
            Err("identity-mismatch")
        );
        assert_eq!(
            resolve_press_owner(Some(&record), 4242, 200, 200, 1000, 9, None, Some(7)),
            Err("identity-mismatch")
        );
        assert_eq!(
            resolve_press_owner(
                Some(&record),
                4242,
                200,
                200,
                1000,
                9,
                Some((4242, 7)),
                None
            ),
            Err("identity-mismatch")
        );
    }

    #[test]
    fn fences_map_all_seven_legs_to_stable_tokens() {
        let guards_ok = SampleGuards {
            session_unlocked: true,
            secure_input_active: false,
            scope_allowed: true,
            fullscreen_active: false,
            revision_fresh: true,
            focus_matches: true,
        };
        assert!(fences_for(true, &guards_ok).refusal().is_none());
        let cases = [
            (false, guards_ok, "disabled"),
            (
                true,
                SampleGuards {
                    session_unlocked: false,
                    ..guards_ok
                },
                "session-locked",
            ),
            (
                true,
                SampleGuards {
                    secure_input_active: true,
                    ..guards_ok
                },
                "secure-input-active",
            ),
            (
                true,
                SampleGuards {
                    scope_allowed: false,
                    ..guards_ok
                },
                "scope-revoked",
            ),
            (
                true,
                SampleGuards {
                    fullscreen_active: true,
                    ..guards_ok
                },
                "fullscreen",
            ),
            (
                true,
                SampleGuards {
                    revision_fresh: false,
                    ..guards_ok
                },
                "revision-drift",
            ),
            (
                true,
                SampleGuards {
                    focus_matches: false,
                    ..guards_ok
                },
                "focus-changed",
            ),
        ];
        for (enabled, guards, want) in cases {
            assert_eq!(fences_for(enabled, &guards).refusal(), Some(want));
        }
    }

    #[test]
    fn down_arms_only_on_proven_meta_owner() {
        let session = session();
        // Unowned flags refuse without arming.
        let plain = DragMouseHit {
            kind: DragMouseKind::Down,
            x: 200,
            y: 200,
            flags: 0,
        };
        assert!(matches!(
            session.on_down(plain, &owner_ok(), fences_ok()),
            HitIntent::Down(Downcome::Refused {
                reason: "unowned-press"
            })
        ));
        assert!(!session.active());
        // Unproven owner refuses without arming.
        assert!(matches!(
            session.on_down(down_hit(200, 200), &Err("unknown-window"), fences_ok()),
            HitIntent::Down(Downcome::Refused { .. })
        ));
        assert!(!session.active());
        // Fenced foreground refuses without arming (consume-then-refuse:
        // the tap already suppressed this edge).
        let fenced = DragFences {
            scope_allowed: false,
            ..fences_ok()
        };
        assert!(matches!(
            session.on_down(down_hit(200, 200), &owner_ok(), fenced),
            HitIntent::Down(Downcome::Refused { reason: "fenced" })
        ));
        assert!(!session.active());
        // Proven owner arms.
        assert!(matches!(
            session.on_down(down_hit(200, 200), &owner_ok(), fences_ok()),
            HitIntent::Down(Downcome::Armed { .. })
        ));
        assert!(session.active());
        // A second down while held ignores (single-flight).
        assert!(matches!(
            session.on_down(down_hit(210, 210), &owner_ok(), fences_ok()),
            HitIntent::Down(Downcome::Ignore)
        ));
        assert!(session.on_stale().is_some());
        assert!(!session.active());
    }

    #[test]
    fn drag_samples_preview_and_zero_hides() {
        let session = session();
        // No gesture: motion ignores.
        assert!(matches!(
            session.on_drag(300, 300, fences_ok()),
            HitIntent::Sample(SampleIntent::Ignore)
        ));
        session.on_down(down_hit(200, 200), &owner_ok(), fences_ok());
        // Zero movement hides transiently (snap-back posture, no plan).
        assert!(matches!(
            session.on_drag(200, 200, fences_ok()),
            HitIntent::Sample(SampleIntent::Hide)
        ));
        assert!(session.active());
        // Moved pointer previews through the world gesture route with
        // the session-minted id and frozen START mechanics.
        match session.on_drag(790, 300, fences_ok()) {
            HitIntent::Sample(SampleIntent::Preview { ingress }) => {
                assert_eq!(ingress.window.0, "4242_7");
                assert_eq!(ingress.cursor, Some((790, 300)));
                assert_eq!(ingress.phase, tiler_core::gesture::GesturePhase::Updated);
                assert!(ingress.id != 0);
            }
            other => panic!("moved sample must preview, got {other:?}"),
        }
        // Fence refusal mid-gesture cancels dead.
        match session.on_drag(
            790,
            300,
            DragFences {
                revision_fresh: false,
                ..fences_ok()
            },
        ) {
            HitIntent::Sample(SampleIntent::Cancelled { reason, ingress }) => {
                assert_eq!(reason, "revision-drift");
                assert!(
                    ingress.is_some(),
                    "fenced cancel clears the core session too"
                );
            }
            other => panic!("fenced sample must cancel dead, got {other:?}"),
        }
        assert!(!session.active());
    }

    #[test]
    fn up_releases_exactly_once_for_drop() {
        let session = session();
        // No gesture: release ignores.
        assert!(matches!(
            session.on_up(300, 300),
            HitIntent::Release(ReleaseIntent::Ignore)
        ));
        session.on_down(down_hit(200, 200), &owner_ok(), fences_ok());
        // Final pointer snapshot rides the drop (single-flight capture).
        match session.on_up(790, 300) {
            HitIntent::Release(ReleaseIntent::Drop {
                ingress,
                press_revision,
            }) => {
                assert_eq!(ingress.window.0, "4242_7");
                assert_eq!(ingress.cursor, Some((790, 300)));
                assert_eq!(ingress.phase, tiler_core::gesture::GesturePhase::Released);
                assert_eq!(press_revision, 9);
            }
            other => panic!("release must drop once, got {other:?}"),
        }
        assert!(!session.active());
        // Balanced: a second release ignores (no double drop).
        assert!(matches!(
            session.on_up(790, 300),
            HitIntent::Release(ReleaseIntent::Ignore)
        ));
    }

    #[test]
    fn escape_and_stale_cancel_with_balanced_release() {
        let session = session();
        assert!(session.on_escape().is_none());
        session.on_down(down_hit(200, 200), &owner_ok(), fences_ok());
        assert!(session.on_escape().is_some());
        assert!(!session.active());
        assert!(session.on_escape().is_none());
        // Balanced: the consumed down ends through the Escape path, so a
        // trailing release passes through untouched (never a drop).
        assert!(matches!(
            session.on_up(790, 300),
            HitIntent::Release(ReleaseIntent::Ignore)
        ));
        session.on_down(down_hit(200, 200), &owner_ok(), fences_ok());
        assert!(session.on_stale().is_some());
        assert!(!session.active());
        assert!(matches!(
            session.on_up(790, 300),
            HitIntent::Release(ReleaseIntent::Ignore)
        ));
    }

    #[test]
    fn batch_plan_orders_escape_down_drag_up() {
        let session = session();
        // Down then drag in one batch arms and previews (coalesced).
        match session.plan_batch(
            &[down_hit(200, 200), drag_hit(300, 300), drag_hit(790, 300)],
            false,
            &owner_ok(),
            fences_ok(),
        ) {
            DragPlan::Preview(ingress) => {
                assert_eq!(ingress.window.0, "4242_7");
                assert_eq!(ingress.cursor, Some((790, 300)));
            }
            other => panic!("batch must preview latest, got {other:?}"),
        }
        // A release in the batch drops at the final pointer (intermediate
        // motion is stale once the button is up).
        match session.plan_batch(
            &[drag_hit(400, 400), up_hit(790, 300)],
            false,
            &owner_ok(),
            fences_ok(),
        ) {
            DragPlan::Drop { ingress, .. } => {
                assert_eq!(ingress.window.0, "4242_7");
                assert_eq!(ingress.cursor, Some((790, 300)));
            }
            other => panic!("batch must drop at release, got {other:?}"),
        }
        assert!(!session.active());
    }

    #[test]
    fn batch_plan_escape_wins_over_release() {
        let session = session();
        session.on_down(down_hit(200, 200), &owner_ok(), fences_ok());
        // Escape plus release on one turn cancels (never mutates).
        match session.plan_batch(&[up_hit(790, 300)], true, &owner_ok(), fences_ok()) {
            DragPlan::Cancelled { reason, .. } => assert_eq!(reason, "escape"),
            other => panic!("escape must win, got {other:?}"),
        }
        assert!(!session.active());
    }

    #[test]
    fn batch_plan_escape_wins_over_fresh_arm() {
        let session = session();
        // Down and Escape drained on the same turn: the arm lands, then
        // Escape cancels it (never a preview, never a drop).
        match session.plan_batch(&[down_hit(200, 200)], true, &owner_ok(), fences_ok()) {
            DragPlan::Cancelled { reason, .. } => assert_eq!(reason, "escape"),
            other => panic!("same-batch escape must cancel the fresh arm, got {other:?}"),
        }
        assert!(!session.active());
    }

    #[test]
    fn up_carries_press_revision_for_worker_stale_check() {
        let session = session();
        session.on_down(down_hit(200, 200), &owner_ok(), fences_ok());
        match session.on_up(790, 300) {
            HitIntent::Release(ReleaseIntent::Drop {
                ingress,
                press_revision,
            }) => {
                assert_eq!(ingress.window.0, "4242_7");
                assert_eq!(press_revision, 9);
            }
            other => panic!("release must drop with revision, got {other:?}"),
        }
    }

    #[test]
    fn carrier_above_verdict_needs_both_present_in_order() {
        // Carrier before target verifies; anything else hides.
        assert!(carrier_above_verified(&[5, 7, 9], 5, 7));
        assert!(carrier_above_verified(&[9, 5, 7], 5, 7));
        assert!(!carrier_above_verified(&[7, 5], 5, 7));
        assert!(!carrier_above_verified(&[7], 5, 7));
        assert!(!carrier_above_verified(&[5], 5, 7));
        assert!(!carrier_above_verified(&[], 5, 7));
        assert!(!carrier_above_verified(&[5, 7], 0, 7));
        assert!(!carrier_above_verified(&[5, 7], 5, 0));
        assert!(!carrier_above_verified(&[5, 5, 7], 5, 5));
    }

    #[test]
    fn batch_plan_refused_arm_never_drops() {
        let session = session();
        // Fenced down plus trailing release: refused, then idle (the
        // suppressed edge balances to pass-through, never a drop).
        match session.plan_batch(
            &[down_hit(200, 200), up_hit(790, 300)],
            false,
            &owner_ok(),
            DragFences {
                scope_allowed: false,
                ..fences_ok()
            },
        ) {
            DragPlan::Cancelled { reason, .. } => assert_eq!(reason, "fenced"),
            other => panic!("fenced batch must cancel, got {other:?}"),
        }
        assert!(!session.active());
    }

    #[test]
    fn gesture_ingress_carries_no_hover_prior() {
        use crate::drag_drop::map_preview_rejection;
        let session = session();
        session.on_down(down_hit(200, 200), &owner_ok(), fences_ok());
        // Centre/outside/self refusals hide transiently and keep the gesture.
        for kind in [
            "unsupported-capability",
            "unchanged",
            "cross-domain-mismatch",
        ] {
            session.note_preview(&map_preview_rejection(kind));
            assert!(session.active(), "transient {kind} keeps the gesture");
        }
        // A moved sample offers a gesture Updated ingress with the minted
        // id and frozen START mechanics (never a hover prior, never a
        // guessed target).
        match session.on_drag(790, 300, fences_ok()) {
            HitIntent::Sample(SampleIntent::Preview { ingress }) => {
                assert_eq!(ingress.phase, tiler_core::gesture::GesturePhase::Updated);
                assert!(ingress.id != 0);
                assert_eq!(ingress.cursor, Some((790, 300)));
            }
            other => panic!("moved sample must preview, got {other:?}"),
        }
        // Empty batch on a held gesture hides (carrier stays hidden).
        assert_eq!(
            session.plan_batch(&[], false, &owner_ok(), fences_ok()),
            DragPlan::Hide
        );
        assert!(session.on_stale().is_some());
    }

    #[test]
    fn overlay_uses_configured_fill_above_target() {
        use crate::drag_drop::DragOverlay;
        use tiler_core::geometry::Rect;
        let session = session();
        let rect = Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 80,
        };
        match session.overlay_for(rect) {
            DragOverlay::ShowFillAbove {
                fill,
                ordering: crate::drag_drop::DragOrdering::AboveTarget,
                ..
            } => assert_eq!(fill, crate::drag_drop::DEFAULT_DRAG_FILL_RGBA),
            other => panic!("preview must show filled above, got {other:?}"),
        }
        let off = DragSession::new(
            DragOptions {
                enabled: false,
                fill: crate::drag_drop::DEFAULT_DRAG_FILL_RGBA,
            },
            ProbeModifier::Ctrl,
        );
        assert_eq!(off.overlay_for(rect), crate::drag_drop::DragOverlay::Hide);
        assert_eq!(
            session.overlay_for(Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 80
            }),
            crate::drag_drop::DragOverlay::Hide
        );
    }

    #[test]
    fn preview_outer_appkit_flips_around_primary() {
        use tiler_core::geometry::Rect;
        // CG top-left (10,20,100x80) on a 900pt display flips to
        // AppKit bottom-left y = 900 - (20 + 80) = 800.
        let outer = preview_outer_appkit(
            Rect {
                x: 10,
                y: 20,
                w: 100,
                h: 80,
            },
            Some(900.0),
        )
        .expect("converts");
        assert_eq!(
            (outer.x, outer.y, outer.w, outer.h),
            (10.0, 800.0, 100.0, 80.0)
        );
        assert!(
            preview_outer_appkit(
                Rect {
                    x: 0,
                    y: 0,
                    w: 1,
                    h: 1
                },
                None
            )
            .is_none()
        );
        assert!(
            preview_outer_appkit(
                Rect {
                    x: 0,
                    y: 0,
                    w: 1,
                    h: 1
                },
                Some(0.0)
            )
            .is_none()
        );
        assert!(
            preview_outer_appkit(
                Rect {
                    x: 0,
                    y: 0,
                    w: 0,
                    h: 1
                },
                Some(900.0)
            )
            .is_none()
        );
    }

    #[test]
    fn preview_target_wid_parses_runtime_token() {
        assert_eq!(preview_target_wid(&WindowId("4242_7".to_owned())), Some(7));
        assert_eq!(preview_target_wid(&WindowId("w1".to_owned())), None);
        assert_eq!(preview_target_wid(&WindowId(String::new())), None);
    }

    #[test]
    fn drag_flags_contract_pins_click_through_panel() {
        use crate::ffi::BorderCarrierFlags;
        let ok = BorderCarrierFlags {
            ignores_mouse: true,
            can_key: false,
            can_main: false,
            collection_managed_only: true,
            level: 0,
        };
        assert!(drag_flags_ok(&ok));
        for bad in [
            BorderCarrierFlags {
                ignores_mouse: false,
                ..ok
            },
            BorderCarrierFlags {
                can_key: true,
                ..ok
            },
            BorderCarrierFlags {
                can_main: true,
                ..ok
            },
            BorderCarrierFlags {
                collection_managed_only: false,
                ..ok
            },
            BorderCarrierFlags { level: 3, ..ok },
        ] {
            assert!(!drag_flags_ok(&bad), "{bad:?}");
        }
    }

    #[test]
    fn controller_motion_gate_stays_pure() {
        let session = session();
        session.on_down(down_hit(200, 200), &owner_ok(), fences_ok());
        let controller = session.controller();
        let mut guard = controller.try_lock().expect("session lock");
        assert_eq!(
            guard.motion(200, 200, fences_ok()),
            MotionGate::ZeroSnapBack
        );
    }

    #[test]
    fn meta_held_matches_run_modifier_exactly() {
        let ctrl = DragSession::new(DragOptions::default(), ProbeModifier::Ctrl);
        let meta = DragSession::new(DragOptions::default(), ProbeModifier::Meta);
        assert!(ctrl.meta_held(0x0004_0000));
        assert!(!ctrl.meta_held(0x0010_0000));
        assert!(meta.meta_held(0x0010_0000));
        assert!(!meta.meta_held(0x0004_0000));
        let (want, mask) = ctrl.meta_bits();
        assert_eq!((want, mask), (0x0004_0000, 0x001e_0000));
        let _ = (AxFacts::standard(), ActivationPolicy::Regular);
    }

    #[test]
    fn drag_arm_parks_every_combination_while_acceptance_pending() {
        // Live acceptance gate is closed: the product arm parks with the
        // explicit `live-acceptance-pending` reason on EVERY combination —
        // notably the ordinary enabled-on-tap tile session, which must NOT
        // install any mouse tap. Ordinary drag_preview enabled/color
        // settings cannot satisfy this gate (no flag, no settings key).
        // (The gate constant itself is read through the verdict below, so
        // flipping it to an accepted state fails these assertions — the
        // acceptance unit owns that update.)
        for (enabled, tap) in [(true, true), (true, false), (false, true), (false, false)] {
            let parked = drag_arm_parked_reason(enabled, tap);
            assert!(
                parked
                    .as_deref()
                    .is_some_and(|reason| reason.contains("live-acceptance-pending")),
                "enabled={enabled} tap={tap}: must park live-acceptance-pending, got {parked:?}"
            );
        }
    }
}
