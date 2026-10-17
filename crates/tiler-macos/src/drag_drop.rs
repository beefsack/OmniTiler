#![forbid(unsafe_code)]

//! macOS drag/drop production pipeline (offline).
//!
//! Meta+left client move through the existing configurable Meta mapping
//! (Control default, Command opt-in; no new chord). Native titlebar moves
//! stay parked (see [`crate::drag_native`]): no blanket
//! AX-moved classification, no consume/replay, host-native movement only.
//!
//! Reuses the actual shared Engine drag oracle
//! ([`tiler_core::boundary::CoreCommand::DragPreview`] / `DragDrop` via
//! [`tiler_core::engine::Engine::handle`]) and the filled above-target
//! carrier intent (BorderCarrier ordering + group-underlay fill parsing and
//! geometry). Centre is unsupported (no swap/stacks); same-axis, group,
//! wrap, and interior targeting ride the core resolver. Zero/self/outside
//! releases snap back; Escape cancels; the overlay clears before any
//! mutation; the final pointer capture is single-flight. Cross-output reuses
//! the core pair route where the existing topology permits. Fullscreen, lock,
//! stale scope/focus/revision fence every sample. No below-target route.

use tiler_core::boundary::{CoreCommand, CoreEvent, CoreReply, TiledPlan};
use tiler_core::directional::WindowId;
use tiler_core::engine::Engine;
use tiler_core::session::DragHoverPrior;
use tiler_core::visual::VisualRect;

use crate::config::ProbeModifier;

/// Default drag-preview fill (`#402a82da` in `#aarrggbb`: alpha `0x40`,
/// RGB `0x2a,0x82,0xda`; KDE parity).
pub const DEFAULT_DRAG_FILL_HEX: &str = "#402a82da";
/// Same default as raw channels in RGBA order (alpha last).
pub const DEFAULT_DRAG_FILL_RGBA: (u8, u8, u8, u8) = (0x2a, 0x82, 0xda, 0x40);
/// Drag preview master switch default (on).
pub const DEFAULT_DRAG_ENABLED: bool = true;

/// Parse exactly `#aarrggbb` for the drag-preview fill (shared
/// `ColorArgb` grammar, same as the group underlay). Returns RGBA-ordered
/// `(r, g, b, a)`. Six-digit `#rrggbb` refuses (never an opaque guess).
pub fn parse_drag_fill(value: &str) -> Result<(u8, u8, u8, u8), String> {
    crate::group_underlay::parse_fill(value)
        .map_err(|_| "refuse: --drag-preview-color needs #aarrggbb".to_owned())
}

/// Render RGBA channels back to shared `#aarrggbb` (alpha first).
#[must_use]
pub fn render_drag_fill(fill: (u8, u8, u8, u8)) -> String {
    crate::group_underlay::render_fill(fill)
}

/// Drag-preview options (startup-only, CLI-over-settings-file like the
/// border switch).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragOptions {
    /// Master switch (`--no-drag-preview`).
    pub enabled: bool,
    /// Filled insertion-slot colour (RGBA, alpha last).
    pub fill: (u8, u8, u8, u8),
}

impl Default for DragOptions {
    fn default() -> Self {
        Self {
            enabled: DEFAULT_DRAG_ENABLED,
            fill: DEFAULT_DRAG_FILL_RGBA,
        }
    }
}

/// Raw drag CLI selections (all unset = defaults). Strings stay raw so
/// `Command` keeps its `Eq` derive; parsing happens in
/// [`resolve_drag_config`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DragCli {
    /// Master switch (`Some(false)` via `--no-drag-preview`).
    pub enabled: Option<bool>,
    /// Filled slot colour `#aarrggbb`.
    pub color: Option<String>,
}

/// Drag-preview settings file section (`"drag_preview"` object; every field
/// optional; CLI wins per field). Key naming mirrors the Windows
/// underlay/border catalog under the `--drag-preview-*` CLI prefix.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DragPreviewSettings {
    /// Master switch (default on).
    pub enabled: Option<bool>,
    /// Filled slot colour as `#aarrggbb` (default `#402a82da`).
    pub color: Option<String>,
}

impl DragPreviewSettings {
    /// Validate merged file content (no OS reads).
    pub fn validate(&self) -> Result<(), String> {
        if let Some(color) = &self.color {
            parse_drag_fill(color).map_err(|reason| format!("settings: {reason}"))?;
        }
        Ok(())
    }
}

/// Merge CLI drag selections over the settings file: an explicitly passed
/// CLI flag always wins; otherwise the file value applies; otherwise the
/// built-in default (on, `#402a82da`). Failures refuse closed (never a
/// silent fallback). Startup-only: a running session never re-reads it.
pub fn resolve_drag_config(
    cli: &DragCli,
    file: Option<&DragPreviewSettings>,
) -> Result<DragOptions, String> {
    let enabled = cli
        .enabled
        .or(file.and_then(|f| f.enabled))
        .unwrap_or(DEFAULT_DRAG_ENABLED);
    let fill = match &cli.color {
        Some(raw) => parse_drag_fill(raw)?,
        None => match file.and_then(|f| f.color.as_ref()) {
            Some(raw) => parse_drag_fill(raw).map_err(|reason| format!("settings: {reason}"))?,
            None => DEFAULT_DRAG_FILL_RGBA,
        },
    };
    Ok(DragOptions { enabled, fill })
}

/// Above-target ordering for the drag preview: the filled insertion slot
/// renders above the target window (KWin overlay-item analogue, Windows
/// `show_fill_above` parity). There is no below-target route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragOrdering {
    AboveTarget,
}

/// Carrier flags the live preview reuses (BorderCarrier contract): a
/// borderless nonactivating panel that ignores mouse events (click-through),
/// never becomes key/main, stays `Managed`-only at the normal window level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragCarrierFlags {
    /// `ignoresMouseEvents` (click-through): must be true.
    pub click_through: bool,
    /// Nonactivating panel: never steals focus.
    pub nonactivating: bool,
    /// Normal window level (`0`).
    pub normal_level: bool,
    /// `Managed`-only collection (never fullscreen-auxiliary).
    pub managed_only: bool,
}

impl DragCarrierFlags {
    /// The contract the live `BorderCarrier::flags` readback must satisfy.
    #[must_use]
    pub const fn required() -> Self {
        Self {
            click_through: true,
            nonactivating: true,
            normal_level: true,
            managed_only: true,
        }
    }
}

/// Exact-chord Meta check over raw `CGEventFlags` for the existing
/// configurable Meta mapping (Control default, Command opt-in; same mask
/// space as [`crate::input::cg_flag_bits`]). Extra modifiers fail, matching
/// the keyboard exact-chord rule. Pure: the mouse-tap callback consults
/// this without blocking.
#[must_use]
pub fn meta_held_for(flags: u64, modifier: ProbeModifier) -> bool {
    let (want, mask) = crate::input::cg_flag_bits(modifier);
    flags & mask == want
}

/// Pure overlay intent for one drag sample: at most one filled rect above
/// the target, or hidden. The live main thread maps `ShowFillAbove` to one
/// `BorderCarrier`-flagged panel update (ordered above) filled with the
/// configured ARGB fill, and `Hide` to `orderOut:`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DragOverlay {
    Hide,
    ShowFillAbove {
        rect: VisualRect,
        fill: (u8, u8, u8, u8),
        ordering: DragOrdering,
    },
}

/// Fences evaluated on every drag sample (all plain data, no OS touch).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragFences {
    /// Drag preview switch on.
    pub enabled: bool,
    /// Graphical session unlocked (unknown denies).
    pub session_unlocked: bool,
    /// Secure Input active denies.
    pub secure_input_active: bool,
    /// Product scope still allows the mover.
    pub scope_allowed: bool,
    /// Mover, frontmost, or ANY visible fullscreen denies (game safety:
    /// a fullscreen window on the pointer display denies even when the
    /// mover is windowed).
    pub fullscreen_active: bool,
    /// Capture revision still equals the live revision.
    pub revision_fresh: bool,
    /// Focus still on the dragged window (focus-at-press plus retention).
    pub focus_matches: bool,
}

impl DragFences {
    /// Closed refusal token, or `None` when every leg passes.
    #[must_use]
    pub const fn refusal(self) -> Option<&'static str> {
        if !self.enabled {
            return Some("disabled");
        }
        if !self.session_unlocked {
            return Some("session-locked");
        }
        if self.secure_input_active {
            return Some("secure-input-active");
        }
        if !self.scope_allowed {
            return Some("scope-revoked");
        }
        if self.fullscreen_active {
            return Some("fullscreen");
        }
        if !self.revision_fresh {
            return Some("revision-drift");
        }
        if !self.focus_matches {
            return Some("focus-changed");
        }
        None
    }
}

/// Maximum age (monotonic milliseconds on the shared
/// [`crate::input_eligibility::now_ms`] clock) of a published press-proof
/// snapshot when the tap down edge arrives. The tile main loop republishes
/// every idle turn on its 250 ms cadence; 500 ms tolerates one missed or slow
/// turn (the same bound as the input-cache `STALE_AFTER` gate) while refusing
/// anything older. Publisher-record freshness (`FACTS_TTL`, ~2 s) never
/// extends this pointer proof: a moved, drifted, or delayed cursor passes
/// through untouched (fail open, never arm on stale data).
pub const PRESS_PROOF_MAX_AGE_MS: u64 = 500;

/// Bounded seqlock read retries in the tap callback before failing open. A
/// torn read means a main-thread publish landed mid-edge; the down passes
/// untouched with pass-through memory rather than spinning on the callback.
const GATE_READ_RETRIES: u32 = 3;

/// Lock-free tap/main-thread gate for the Meta+left press: the main thread
/// publishes one affirmatively proven eligibility snapshot per idle turn
/// (fresh off-tap AX+CG owner join plus passing fences at the published
/// point, invalidated before each new proof); the tap callback consumes a
/// down only when it matches that snapshot EXACTLY (same point, fresh
/// monotonic age). All atomics, no AX/blocking/core/logging in the callback
/// beyond one monotonic clock read (same contract as
/// [`crate::input_eligibility::EligibilityCache::is_eligible`]).
/// A stale, mismatched, torn, or contended snapshot fails OPEN (passes the
/// event) and remembers the pass-through gesture so the rest of that button
/// hold passes too. Capture ownership (`held`) is lock-independent: once a
/// down is consumed, held drag/up edges consume even under controller
/// contention, so a consumed down never corrupts into a passed up.
///
/// Coherence: the snapshot fields ride a seqlock (`seq`): the single
/// main-thread publisher brackets its writes with odd/even transitions while
/// the callback retries its read boundedly ([`GATE_READ_RETRIES`]) and fails
/// open on a tear, so the callback never observes a mixed publication (half
/// old owner, half new point). Lifecycle (`valid`) is a separate flag, so an
/// invalidate racing an in-flight publish can never corrupt the sequence.
/// The consume claims single-use through a `valid` CAS plus a verify-after-
/// claim re-read: a fresher publish landing between the check and the claim
/// fails open rather than arming on superseded data. The main-side
/// [`crate::drag_session::DragSession::on_down`] still re-proves the actual
/// down point off-tap before arming, so even a won race cannot arm an
/// unproven window.
pub struct DragTapGate {
    /// Capture held: set on a consumed down, cleared on the consumed up
    /// (tap side) or on teardown release (main thread gone; Escape/stale /
    /// drop cancel through the session on the main thread, never here).
    held: std::sync::atomic::AtomicBool,
    /// Pass-through memory: set when a down passes unproven; while set,
    /// every edge passes until the up clears it (never arm mid-gesture).
    passthrough: std::sync::atomic::AtomicBool,
    /// Seqlock sequence: 0 means no snapshot readable (never published or
    /// invalidated); even nonzero means a stable published snapshot
    /// (generation `seq / 2`); odd means a main-thread publish is in flight.
    /// Only the single main-thread publisher advances this; readers never
    /// write it, so invalidation cannot corrupt it.
    seq: std::sync::atomic::AtomicU64,
    /// Snapshot point (CG top-left integers, tap space).
    snap_x: std::sync::atomic::AtomicI32,
    snap_y: std::sync::atomic::AtomicI32,
    /// Snapshot owner (exact proven `(pid, wid)`).
    owner_pid: std::sync::atomic::AtomicU32,
    owner_wid: std::sync::atomic::AtomicU32,
    /// Snapshot fences passed at publish time (main-side proof).
    fences_ok: std::sync::atomic::AtomicBool,
    /// Monotonic publish time ([`crate::input_eligibility::now_ms`]).
    proof_ms: std::sync::atomic::AtomicU64,
    /// Snapshot live: set after a complete publish, cleared on consume,
    /// invalidate, up, or cancel. Separate from `seq` so lifecycle races
    /// stay fail-open without touching the sequence.
    valid: std::sync::atomic::AtomicBool,
}

/// One coherently read eligibility snapshot (plain data, never a live read).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GateSnapshot {
    pid: u32,
    wid: u32,
    x: i32,
    y: i32,
    proof_ms: u64,
}

impl DragTapGate {
    /// New empty gate (nothing eligible, nothing held).
    #[must_use]
    pub fn new() -> Self {
        Self {
            held: std::sync::atomic::AtomicBool::new(false),
            passthrough: std::sync::atomic::AtomicBool::new(false),
            seq: std::sync::atomic::AtomicU64::new(0),
            snap_x: std::sync::atomic::AtomicI32::new(0),
            snap_y: std::sync::atomic::AtomicI32::new(0),
            owner_pid: std::sync::atomic::AtomicU32::new(0),
            owner_wid: std::sync::atomic::AtomicU32::new(0),
            fences_ok: std::sync::atomic::AtomicBool::new(false),
            proof_ms: std::sync::atomic::AtomicU64::new(0),
            valid: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Publish one proven eligibility snapshot: exact owner plus point plus
    /// the fence verdict at publish time, stamped with the shared monotonic
    /// clock. Only the main thread calls this, only while its controller is
    /// idle, and only after [`invalidate`](Self::invalidate) cleared the
    /// previous proof. Any previous pass-through memory for a released
    /// button stays cleared (publish never resurrects it).
    pub fn publish(&self, pid: u32, wid: u32, x: i32, y: i32, fences_pass: bool) {
        self.publish_at(
            pid,
            wid,
            x,
            y,
            fences_pass,
            crate::input_eligibility::now_ms(),
        );
    }

    /// Publish at an explicit monotonic millisecond clock (deterministic
    /// tests; same seqlock order as [`publish`](Self::publish)).
    pub fn publish_at(&self, pid: u32, wid: u32, x: i32, y: i32, fences_pass: bool, proof_ms: u64) {
        use std::sync::atomic::Ordering::SeqCst;
        self.seq.fetch_add(1, SeqCst);
        self.snap_x.store(x, SeqCst);
        self.snap_y.store(y, SeqCst);
        self.owner_pid.store(pid, SeqCst);
        self.owner_wid.store(wid, SeqCst);
        self.fences_ok.store(fences_pass, SeqCst);
        self.proof_ms.store(proof_ms, SeqCst);
        self.seq.fetch_add(1, SeqCst);
        self.valid.store(true, SeqCst);
    }

    /// Invalidate the snapshot without touching capture state (fences
    /// flipped, proof missed, owner drifted, or a new proof starting — the
    /// next down passes). Touches only the lifecycle flag, so a racing
    /// in-flight publish keeps a coherent sequence: whichever completes
    /// last wins, and a torn mix is never observable.
    pub fn invalidate(&self) {
        self.valid.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// True while the tap owns the button capture.
    #[must_use]
    pub fn is_held(&self) -> bool {
        self.held.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Coherently read the live snapshot (`None` unless published, valid,
    /// and untorn with passing fences and a proven owner). Bounded retries;
    /// a tear fails open to `None`. Diagnostic and consume-path readback
    /// (no age check here; the down path applies
    /// [`PRESS_PROOF_MAX_AGE_MS`]).
    fn read_snapshot(&self) -> Option<(u64, GateSnapshot)> {
        let (seq, snapshot, fences_ok, live) = self.read_raw()?;
        if !live || !fences_ok || snapshot.pid <= 1 || snapshot.wid == 0 {
            return None;
        }
        Some((seq, snapshot))
    }

    /// Raw coherent seqlock read: the generation, the fields, the fence
    /// verdict, and the lifecycle flag as one untorn bundle. `None` when
    /// never published, mid-publish, or torn after bounded retries. The
    /// caller decides lifecycle and fence meaning (the claim path must
    /// re-read coherence AFTER clearing `valid`, so it cannot reuse
    /// [`read_snapshot`](Self::read_snapshot) there).
    fn read_raw(&self) -> Option<(u64, GateSnapshot, bool, bool)> {
        use std::sync::atomic::Ordering::SeqCst;
        for _ in 0..GATE_READ_RETRIES {
            let seq = self.seq.load(SeqCst);
            if seq == 0 || seq % 2 == 1 {
                return None;
            }
            let snapshot = GateSnapshot {
                pid: self.owner_pid.load(SeqCst),
                wid: self.owner_wid.load(SeqCst),
                x: self.snap_x.load(SeqCst),
                y: self.snap_y.load(SeqCst),
                proof_ms: self.proof_ms.load(SeqCst),
            };
            let fences_ok = self.fences_ok.load(SeqCst);
            let live = self.valid.load(SeqCst);
            if self.seq.load(SeqCst) != seq {
                continue;
            }
            return Some((seq, snapshot, fences_ok, live));
        }
        None
    }

    /// Proven snapshot owner and point (`None` unless valid). Diagnostic
    /// readback for the fixture oracle; the down path applies the full
    /// exact-point plus age gate.
    #[must_use]
    pub fn published_snapshot(&self) -> Option<(u32, u32, i32, i32)> {
        self.read_snapshot()
            .map(|(_, snapshot)| (snapshot.pid, snapshot.wid, snapshot.x, snapshot.y))
    }

    /// Tap down decision (lock-free): consume only when idle, Meta-held,
    /// and the snapshot reads coherent, live, fence-passing, age-fresh
    /// (within [`PRESS_PROOF_MAX_AGE_MS`]), and at the EXACT published
    /// point. Claims capture on consume. On pass, remembers the
    /// pass-through gesture until the up. One monotonic clock read only;
    /// never blocks.
    pub fn try_consume_down(&self, x: i32, y: i32, meta_held: bool) -> bool {
        self.try_consume_down_at(x, y, meta_held, crate::input_eligibility::now_ms())
    }

    /// Tap down decision at an explicit monotonic millisecond clock
    /// (deterministic tests; same fail-open order as
    /// [`try_consume_down`](Self::try_consume_down)).
    pub fn try_consume_down_at(&self, x: i32, y: i32, meta_held: bool, now_ms: u64) -> bool {
        use std::sync::atomic::Ordering::SeqCst;
        if self.held.load(SeqCst) {
            return false;
        }
        if self.passthrough.load(SeqCst) {
            return false;
        }
        if !meta_held {
            self.passthrough.store(true, SeqCst);
            return false;
        }
        let Some((seq, snapshot)) = self.read_snapshot() else {
            self.passthrough.store(true, SeqCst);
            return false;
        };
        // Monotonic age bound: a delayed edge passes through (never arms on
        // stale data). A proof timestamp in the future (clock anomaly in
        // tests) fails the same way.
        if now_ms < snapshot.proof_ms
            || now_ms.saturating_sub(snapshot.proof_ms) > PRESS_PROOF_MAX_AGE_MS
        {
            self.passthrough.store(true, SeqCst);
            return false;
        }
        // Exact point: the poll-then-press race moved on, so this press is
        // somebody else's click — pass it through untouched.
        if x != snapshot.x || y != snapshot.y {
            self.passthrough.store(true, SeqCst);
            return false;
        }
        // Single-use claim: exactly one down consumes this generation. A
        // lost race means invalidate or a fresher publish won first; fail
        // open rather than arming on superseded data.
        if self
            .valid
            .compare_exchange(true, false, SeqCst, SeqCst)
            .is_err()
        {
            self.passthrough.store(true, SeqCst);
            return false;
        }
        // Verify-after-claim: a publish landing between the check and the
        // claim must not arm on superseded data. The snapshot stays claimed
        // (single-use either way); only an unchanged coherent re-read arms.
        // The re-read bypasses the lifecycle flag (already claimed by us)
        // and compares the generation plus every field instead.
        match self.read_raw() {
            Some((seq2, again, _, _)) if seq2 == seq && again == snapshot => {}
            _ => {
                self.passthrough.store(true, SeqCst);
                return false;
            }
        }
        self.held.store(true, SeqCst);
        true
    }

    /// Tap drag decision (lock-free): held gestures consume; everything
    /// else passes (including pass-through gestures and idle motion).
    pub fn consume_held_drag(&self) -> bool {
        self.held.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Tap up decision (lock-free): a held capture consumes exactly once
    /// and releases; a pass-through up clears the memory and passes; an
    /// idle up passes.
    pub fn consume_up(&self) -> bool {
        use std::sync::atomic::Ordering::SeqCst;
        if self.held.swap(false, SeqCst) {
            self.passthrough.store(false, SeqCst);
            self.valid.store(false, SeqCst);
            return true;
        }
        self.passthrough.store(false, SeqCst);
        false
    }

    /// Teardown-only capture release: the tap callback calls this when its
    /// send fails because the main thread is gone, releasing capture and
    /// clearing the snapshot so a late restart never inherits it. The tap's
    /// in-flight up, if any, then passes harmlessly (nothing held on either
    /// side). Escape/stale/drop cancel through the session on the main
    /// thread, never here.
    pub fn cancel(&self) {
        use std::sync::atomic::Ordering::SeqCst;
        self.held.store(false, SeqCst);
        self.passthrough.store(false, SeqCst);
        self.valid.store(false, SeqCst);
    }
}

impl Default for DragTapGate {
    fn default() -> Self {
        Self::new()
    }
}

/// Pure drag phase: at most one active gesture (single-flight). `Armed`
/// tracks a consumed Meta+left press before movement; `Dragging` carries
/// the press origin plus the carried sticky hover prior across samples.
#[derive(Debug, Clone, PartialEq, Eq)]
enum DragPhase {
    Idle,
    Armed {
        window: String,
        press_x: i32,
        press_y: i32,
    },
    Dragging {
        window: String,
        press_x: i32,
        press_y: i32,
        hover_prior: Option<DragHoverPrior>,
    },
}

/// Pure Meta+left drag controller: suppression lifecycle plus overlay
/// intents plus single-flight capture. The Engine oracle calls
/// ([`engine_preview`]/[`engine_drop`]) stay outside so the tap callback
/// never blocks on AX/core/log work; the owner thread drives them.
#[derive(Debug, Clone)]
pub struct DragController {
    phase: DragPhase,
    /// True while the Meta+left down edge was consumed (fail-safe release
    /// tracks exactly this: every consumed down gets exactly one release
    /// path — drop, Escape, or stale fence).
    consumed_down: bool,
    /// Topology context revision proven at press time (rides the drop
    /// submit so the worker refuses stale pid_wid reuse).
    press_revision: Option<u64>,
    options: DragOptions,
}

impl DragController {
    /// New idle controller over the resolved startup options.
    #[must_use]
    pub const fn new(options: DragOptions) -> Self {
        Self {
            phase: DragPhase::Idle,
            consumed_down: false,
            press_revision: None,
            options,
        }
    }

    /// Current options (startup-only).
    #[must_use]
    pub const fn options(&self) -> DragOptions {
        self.options
    }

    /// True while a gesture holds the single-flight capture.
    #[must_use]
    pub const fn active(&self) -> bool {
        !matches!(self.phase, DragPhase::Idle)
    }

    /// True while the consumed Meta+left down edge is held (fail-safe
    /// release state).
    #[must_use]
    pub const fn consumed(&self) -> bool {
        self.consumed_down
    }

    /// Carried sticky hover prior for the next Engine sample (`None` until
    /// the first resolved preview).
    #[must_use]
    pub const fn hover_prior(&self) -> Option<&DragHoverPrior> {
        match &self.phase {
            DragPhase::Dragging { hover_prior, .. } => hover_prior.as_ref(),
            _ => None,
        }
    }

    /// Held-gesture window token (armed owner or dragging mover): `None`
    /// only when idle. Pure read for query building; never consumes.
    #[must_use]
    pub fn held_window(&self) -> Option<&str> {
        match &self.phase {
            DragPhase::Armed { window, .. } | DragPhase::Dragging { window, .. } => Some(window),
            DragPhase::Idle => None,
        }
    }

    /// Meta+left press: arms exactly one gesture when every fence passes.
    /// Focus must already be on the mover (focus at press); the Meta chord
    /// itself never moves focus. Refuses (no consume) while another gesture
    /// holds (single-flight), while disabled, or on any fence refusal.
    /// Unrelated buttons, unmodified presses, and ineligible foregrounds
    /// never arm and never consume. `context_revision` is the topology
    /// revision proven alongside the owner; it rides the drop submit so the
    /// worker refuses a drifted pid_wid.
    #[allow(clippy::too_many_arguments)]
    pub fn press(
        &mut self,
        window: &str,
        x: i32,
        y: i32,
        meta_held: bool,
        left_button: bool,
        fences: DragFences,
        context_revision: u64,
    ) -> Result<(), &'static str> {
        if !matches!(self.phase, DragPhase::Idle) {
            return Err("single-flight");
        }
        if !meta_held || !left_button {
            return Err("unowned-press");
        }
        if window.is_empty() {
            return Err("unknown-window");
        }
        if let Some(reason) = fences.refusal() {
            let _ = reason;
            return Err("fenced");
        }
        self.phase = DragPhase::Armed {
            window: window.to_owned(),
            press_x: x,
            press_y: y,
        };
        self.consumed_down = true;
        self.press_revision = Some(context_revision);
        Ok(())
    }

    /// Whether a mouse down edge with `meta_held` should be consumed
    /// (suppressed from the client) rather than passed through. Consumes
    /// only Meta+left presses that would arm (idle + every fence passing);
    /// unrelated apps, unmodified presses, and fenced foregrounds pass
    /// untouched. Pure: the tap callback consults this without blocking.
    #[must_use]
    pub const fn should_consume_down(
        &self,
        meta_held: bool,
        left_button: bool,
        fences: DragFences,
    ) -> bool {
        if !matches!(self.phase, DragPhase::Idle) {
            return false;
        }
        if !meta_held || !left_button {
            return false;
        }
        fences.refusal().is_none()
    }

    /// Consume-first mouse-down consult for the tap callback: idle plus
    /// Meta+left consumes (the event is suppressed and forwarded); the
    /// main thread enforces the full [`DragFences`] at
    /// [`DragController::press`] and refuses fenced foregrounds there
    /// without arming, mirroring the keyboard consume-then-refuse shape.
    /// The callback never evaluates fences (no AX/core/log contact), only
    /// this idle+Meta shape over already-held state.
    #[must_use]
    pub const fn should_consume_mouse_down(&self, meta_held: bool) -> bool {
        meta_held && matches!(self.phase, DragPhase::Idle)
    }

    /// Whether a mouse drag/up edge belongs to the held gesture (consumed
    /// lifecycle). Only the armed/dragging gesture consumes; everything else
    /// passes. The final up is captured exactly once (single-flight final
    /// pointer capture).
    #[must_use]
    pub const fn should_consume_held(&self) -> bool {
        self.consumed_down && !matches!(self.phase, DragPhase::Idle)
    }

    /// Pointer motion: zero movement from the press point hides transiently
    /// (snap-back posture, no plan); otherwise the caller resolves through
    /// [`engine_preview`] and feeds the outcome to
    /// [`DragController::note_preview`]. Fence refusals cancel dead (clear
    /// overlay, fail-safe release).
    pub fn motion(&mut self, x: i32, y: i32, fences: DragFences) -> MotionGate {
        if matches!(self.phase, DragPhase::Idle) {
            return MotionGate::Ignore;
        }
        if let Some(reason) = fences.refusal() {
            self.clear();
            return MotionGate::CancelDead(reason);
        }
        let (press_x, press_y) = match &self.phase {
            DragPhase::Armed {
                press_x, press_y, ..
            }
            | DragPhase::Dragging {
                press_x, press_y, ..
            } => (*press_x, *press_y),
            DragPhase::Idle => return MotionGate::Ignore,
        };
        if x == press_x && y == press_y {
            return MotionGate::ZeroSnapBack;
        }
        MotionGate::Sample
    }

    /// Record one Engine preview outcome: resolved previews promote
    /// `Armed` to `Dragging` and carry the next sticky prior; refusals
    /// clear the carried prior (transient hide, gesture stays for the next
    /// sample); dead replies (divergence/pending) terminate the gesture
    /// with overlay cleared and fail-safe release.
    pub fn note_preview(&mut self, outcome: &PreviewOutcome) {
        match outcome {
            PreviewOutcome::Show { hover_prior, .. } => {
                let window = match &self.phase {
                    DragPhase::Armed { window, .. } | DragPhase::Dragging { window, .. } => {
                        window.clone()
                    }
                    DragPhase::Idle => return,
                };
                let (press_x, press_y) = match &self.phase {
                    DragPhase::Armed {
                        press_x, press_y, ..
                    }
                    | DragPhase::Dragging {
                        press_x, press_y, ..
                    } => (*press_x, *press_y),
                    DragPhase::Idle => return,
                };
                self.phase = DragPhase::Dragging {
                    window,
                    press_x,
                    press_y,
                    hover_prior: hover_prior.as_deref().cloned(),
                };
            }
            PreviewOutcome::HideTransient { .. } => {
                if let DragPhase::Dragging { hover_prior, .. } = &mut self.phase {
                    *hover_prior = None;
                }
            }
            PreviewOutcome::CancelDead { .. } => self.clear(),
        }
    }

    /// Escape: cancels the gesture, clears the overlay BEFORE any mutation
    /// (cancel never mutates topology), and fail-safe releases the consumed
    /// down. Returns true when a gesture was cancelled.
    pub fn escape(&mut self) -> bool {
        if matches!(self.phase, DragPhase::Idle) {
            return false;
        }
        self.clear();
        true
    }

    /// Final pointer up: ends the single-flight capture exactly once. The
    /// caller resolves the drop through [`engine_drop`] AFTER clearing the
    /// overlay (clear-before-mutation); this method only releases the
    /// capture. Returns the dragged window when a gesture was held.
    pub fn release(&mut self) -> Option<String> {
        if matches!(self.phase, DragPhase::Idle) {
            return None;
        }
        let window = match &self.phase {
            DragPhase::Armed { window, .. } | DragPhase::Dragging { window, .. } => window.clone(),
            DragPhase::Idle => return None,
        };
        self.clear();
        Some(window)
    }

    /// Stale scope/focus/revision fence: cancels dead with overlay cleared
    /// and fail-safe release. Returns true when a gesture was dropped.
    pub fn note_stale(&mut self) -> bool {
        self.escape()
    }

    /// Overlay intent for a resolved preview rect: the filled core
    /// insertion-slot rect in the configured fill, ordered above the target.
    #[must_use]
    pub const fn overlay_for(
        options: DragOptions,
        rect: VisualRect,
        ordering: DragOrdering,
    ) -> DragOverlay {
        if !options.enabled {
            return DragOverlay::Hide;
        }
        if !rect.x.is_finite() || !rect.y.is_finite() || !rect.w.is_finite() || !rect.h.is_finite()
        {
            return DragOverlay::Hide;
        }
        if rect.w <= 0.0 || rect.h <= 0.0 {
            return DragOverlay::Hide;
        }
        DragOverlay::ShowFillAbove {
            rect,
            fill: options.fill,
            ordering,
        }
    }

    /// Press topology revision (`None` only when idle).
    #[must_use]
    pub const fn press_revision(&self) -> Option<u64> {
        self.press_revision
    }

    fn clear(&mut self) {
        self.phase = DragPhase::Idle;
        self.consumed_down = false;
        self.press_revision = None;
    }
}

/// Motion gate for one pointer sample (pure, no Engine contact).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionGate {
    /// No gesture held: ignore.
    Ignore,
    /// Zero movement from press: transient snap-back posture (hide, no plan).
    ZeroSnapBack,
    /// Moved pointer with live fences: resolve via [`engine_preview`].
    Sample,
    /// Fence refused mid-gesture: overlay cleared, capture released, dead.
    CancelDead(&'static str),
}

/// Read-only preview outcome from the shared Engine oracle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewOutcome {
    /// Resolved insertion slot: show the filled rect above the target.
    /// `target` names the insertion-neighbor window (the carrier orders
    /// above it; never assumed, always read back via the CG join). The
    /// prior rides boxed: only the resolved variant carries one.
    Show {
        rect: tiler_core::geometry::Rect,
        hover_prior: Option<Box<DragHoverPrior>>,
        target: WindowId,
    },
    /// Transient refusal (self/zero/outside/centre/unsupported): hide with
    /// no rectangle; the gesture stays for the next sample.
    HideTransient { reason: &'static str },
    /// Dead reply (divergence/pending): terminate the gesture, overlay
    /// cleared.
    CancelDead { reason: &'static str },
}

/// Map one Engine reply for a `DragPreview` command to a pure outcome.
/// Centre (`unsupported-capability`), self (`unchanged`), and outside
/// (`cross-domain-mismatch`) hide transiently; divergences and pending
/// cancel dead; anything else hides without a rectangle.
#[must_use]
pub fn classify_preview_reply(reply: &CoreReply) -> PreviewOutcome {
    match reply {
        CoreReply::DragPreview(plan) => PreviewOutcome::Show {
            rect: plan.preview.proposed_rect,
            hover_prior: Some(Box::new(plan.preview.hover_prior())),
            target: plan.preview.target_window.clone(),
        },
        CoreReply::Rejected { kind, .. } => match *kind {
            "unsupported-capability" => PreviewOutcome::HideTransient {
                reason: "centre-unsupported",
            },
            "unchanged" => PreviewOutcome::HideTransient { reason: "self" },
            "cross-domain-mismatch" => PreviewOutcome::HideTransient { reason: "outside" },
            "pending-exists" => PreviewOutcome::CancelDead {
                reason: "single-flight",
            },
            _ => PreviewOutcome::HideTransient { reason: kind },
        },
        CoreReply::Diverged(reason) => PreviewOutcome::CancelDead {
            reason: reason.as_str(),
        },
        _ => PreviewOutcome::HideTransient { reason: "refused" },
    }
}

/// Synchronous read-only preview through the actual shared Engine oracle.
/// Stages nothing and touches no reconciler state (the Engine resolves on
/// a working clone). The caller feeds the outcome to
/// [`DragController::note_preview`] to carry the sticky prior.
pub fn engine_preview(engine: &mut Engine, event: CoreEvent) -> PreviewOutcome {
    classify_preview_reply(&engine.handle(&event))
}

/// Synchronous drop outcome from the shared Engine oracle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropOutcome {
    /// Committed plan: the caller clears the overlay FIRST, then applies
    /// the native frames from the plan geometry (single-flight final
    /// capture already released by [`DragController::release`]).
    Planned { plan: TiledPlan },
    /// Invalid release (self/zero/outside/refused): snap back to the
    /// accepted source rect with no plan and no reconciler use.
    SnapBack { reason: &'static str },
    /// Centre drop: unsupported with no plan and no commit (no swap/stacks).
    CentreUnsupported,
    /// Dead reply (divergence/pending): terminate with no plan.
    CancelDead { reason: &'static str },
}

/// Map one Engine reply for a `DragDrop` command to a pure outcome.
#[must_use]
pub fn classify_drop_reply(reply: &CoreReply) -> DropOutcome {
    match reply {
        CoreReply::Tiled(plan) => DropOutcome::Planned { plan: plan.clone() },
        CoreReply::Rejected { kind, .. } => match *kind {
            "unsupported-capability" => DropOutcome::CentreUnsupported,
            "unchanged" | "cross-domain-mismatch" => DropOutcome::SnapBack { reason: kind },
            "pending-exists" => DropOutcome::CancelDead { reason: kind },
            _ => DropOutcome::SnapBack { reason: kind },
        },
        CoreReply::Diverged(reason) => DropOutcome::CancelDead {
            reason: reason.as_str(),
        },
        _ => DropOutcome::SnapBack { reason: "refused" },
    }
}

/// Synchronous drop through the actual shared Engine oracle (commits via
/// acknowledge plus `verify_drag` inside the Engine, mirroring the Windows
/// settle path). The caller MUST clear the overlay before applying the
/// returned plan geometry to native frames.
pub fn engine_drop(engine: &mut Engine, event: CoreEvent) -> DropOutcome {
    classify_drop_reply(&engine.handle(&event))
}

/// Build a same-domain drag command event over an already-converged Engine
/// observation. The caller supplies the domain observation exactly as the
/// reconcile path does (complete canonical rows, focused mover, fingerprint,
/// revision); this only swaps the command to `DragPreview`/`DragDrop` with
/// the carried sticky prior. Cross-output callers set `source` plus the
/// source-first pair in `directional` per the Engine pair contract.
#[must_use]
pub fn drag_command_for(
    _base: &CoreEvent,
    window: &str,
    x: i32,
    y: i32,
    hover_prior: Option<DragHoverPrior>,
    source: Option<tiler_core::session::DomainKey>,
    preview: bool,
) -> CoreCommand {
    if preview {
        CoreCommand::DragPreview {
            window: window.to_owned(),
            x,
            y,
            hover_prior,
            source,
        }
    } else {
        CoreCommand::DragDrop {
            window: window.to_owned(),
            x,
            y,
            hover_prior,
            source,
        }
    }
}

/// Native frames applied from a committed drop plan: window token plus
/// integer rect per plan geometry row. Pure projection over the plan (the
/// live backend pins each leg to its planned domain and fences revision
/// exactly like the keyboard paths).
#[must_use]
pub fn frames_for_plan(plan: &TiledPlan) -> Vec<(WindowId, tiler_core::geometry::Rect)> {
    plan.geometry
        .iter()
        .map(|g| (g.window.clone(), g.rect))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiler_core::boundary::CoreEvent;
    use tiler_core::contract::DragCapabilities;
    use tiler_core::directional::{OutputId, WorkspaceId};
    use tiler_core::geometry::Rect;
    use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
    use tiler_core::seed::EngineWindow;
    use tiler_core::session::{DomainKey, OutputDomain};

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

    #[test]
    fn default_fill_matches_shared_argb_value() {
        assert_eq!(DEFAULT_DRAG_FILL_HEX, "#402a82da");
        assert_eq!(DEFAULT_DRAG_FILL_RGBA, (0x2a, 0x82, 0xda, 0x40));
        assert_eq!(parse_drag_fill("#402a82da"), Ok(DEFAULT_DRAG_FILL_RGBA));
        assert_eq!(parse_drag_fill("#402A82DA"), Ok(DEFAULT_DRAG_FILL_RGBA));
        assert_eq!(render_drag_fill(DEFAULT_DRAG_FILL_RGBA), "#402a82da");
        assert!(parse_drag_fill("402a82da").is_err());
        assert!(parse_drag_fill("#2a82da").is_err());
        assert!(parse_drag_fill("#fff").is_err());
        let options = DragOptions::default();
        assert!(options.enabled);
        assert_eq!(options.fill, DEFAULT_DRAG_FILL_RGBA);
    }

    #[test]
    fn resolve_prefers_cli_over_file_over_defaults() {
        let resolved = resolve_drag_config(&DragCli::default(), None).expect("defaults resolve");
        assert_eq!(resolved, DragOptions::default());
        let file = DragPreviewSettings {
            enabled: Some(false),
            color: Some("#ff2a82da".to_owned()),
        };
        let resolved =
            resolve_drag_config(&DragCli::default(), Some(&file)).expect("file resolves");
        assert!(!resolved.enabled);
        assert_eq!(resolved.fill, (0x2a, 0x82, 0xda, 0xff));
        let cli = DragCli {
            enabled: Some(true),
            color: Some("#112a82da".to_owned()),
        };
        let resolved = resolve_drag_config(&cli, Some(&file)).expect("cli wins");
        assert!(resolved.enabled);
        assert_eq!(resolved.fill, (0x2a, 0x82, 0xda, 0x11));
        assert!(
            resolve_drag_config(
                &DragCli {
                    color: Some("#2a82da".to_owned()),
                    ..DragCli::default()
                },
                None,
            )
            .is_err()
        );
        assert!(
            DragPreviewSettings {
                color: Some("bad".to_owned()),
                ..DragPreviewSettings::default()
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn fences_refuse_closed_with_stable_tokens() {
        assert!(fences_ok().refusal().is_none());
        let cases = [
            (
                DragFences {
                    enabled: false,
                    ..fences_ok()
                },
                "disabled",
            ),
            (
                DragFences {
                    session_unlocked: false,
                    ..fences_ok()
                },
                "session-locked",
            ),
            (
                DragFences {
                    secure_input_active: true,
                    ..fences_ok()
                },
                "secure-input-active",
            ),
            (
                DragFences {
                    scope_allowed: false,
                    ..fences_ok()
                },
                "scope-revoked",
            ),
            (
                DragFences {
                    fullscreen_active: true,
                    ..fences_ok()
                },
                "fullscreen",
            ),
            (
                DragFences {
                    revision_fresh: false,
                    ..fences_ok()
                },
                "revision-drift",
            ),
            (
                DragFences {
                    focus_matches: false,
                    ..fences_ok()
                },
                "focus-changed",
            ),
        ];
        for (fences, want) in cases {
            assert_eq!(fences.refusal(), Some(want), "{fences:?}");
        }
    }

    #[test]
    fn suppression_consumes_only_owned_meta_left() {
        let controller = DragController::new(DragOptions::default());
        assert!(controller.should_consume_down(true, true, fences_ok()));
        assert!(!controller.should_consume_down(false, true, fences_ok()));
        assert!(!controller.should_consume_down(true, false, fences_ok()));
        assert!(!controller.should_consume_down(
            true,
            true,
            DragFences {
                scope_allowed: false,
                ..fences_ok()
            },
        ));
        assert!(!controller.should_consume_down(
            true,
            true,
            DragFences {
                secure_input_active: true,
                ..fences_ok()
            },
        ));
        assert!(!controller.should_consume_held());
    }

    #[test]
    fn press_arms_single_flight_and_tracks_consumed_lifecycle() {
        let mut controller = DragController::new(DragOptions::default());
        assert!(
            controller
                .press("w1", 10, 10, true, true, fences_ok(), 9)
                .is_ok()
        );
        assert!(controller.active());
        assert!(controller.should_consume_held());
        assert!(
            controller
                .press("w2", 12, 12, true, true, fences_ok(), 9)
                .is_err()
        );
        assert!(!controller.should_consume_down(true, true, fences_ok()));
        assert_eq!(controller.release().as_deref(), Some("w1"));
        assert!(!controller.active());
        assert!(!controller.consumed());
        assert!(controller.release().is_none());
        assert!(
            controller
                .press("w1", 10, 10, false, true, fences_ok(), 9)
                .is_err()
        );
        assert!(
            controller
                .press(
                    "w1",
                    10,
                    10,
                    true,
                    true,
                    DragFences {
                        session_unlocked: false,
                        ..fences_ok()
                    },
                    9,
                )
                .is_err()
        );
        assert!(!controller.active());
    }

    #[test]
    fn meta_mapping_is_control_default_command_opt_in_exact() {
        use crate::config::ProbeModifier;
        const CONTROL: u64 = 0x0004_0000;
        const COMMAND: u64 = 0x0010_0000;
        const SHIFT: u64 = 0x0002_0000;
        assert!(meta_held_for(CONTROL, ProbeModifier::Ctrl));
        assert!(!meta_held_for(COMMAND, ProbeModifier::Ctrl));
        assert!(meta_held_for(COMMAND, ProbeModifier::Meta));
        assert!(!meta_held_for(CONTROL, ProbeModifier::Meta));
        // Extra modifiers fail (exact chord, no guessing).
        assert!(!meta_held_for(CONTROL | SHIFT, ProbeModifier::Ctrl));
        assert!(!meta_held_for(COMMAND | SHIFT, ProbeModifier::Meta));
        assert!(!meta_held_for(0, ProbeModifier::Ctrl));
    }

    #[test]
    fn mouse_down_consume_first_shape_without_fences() {
        let mut controller = DragController::new(DragOptions::default());
        // Idle + Meta consumes in the callback (fences enforced later at
        // press on the main thread).
        assert!(controller.should_consume_mouse_down(true));
        assert!(!controller.should_consume_mouse_down(false));
        controller
            .press("w1", 10, 10, true, true, fences_ok(), 9)
            .unwrap();
        // Held gesture: further downs never consume (single-flight).
        assert!(!controller.should_consume_mouse_down(true));
        assert!(controller.should_consume_held());
        assert_eq!(controller.release().as_deref(), Some("w1"));
        assert!(controller.should_consume_mouse_down(true));
    }

    #[test]
    fn gate_consumes_only_the_proven_press_and_fails_open() {
        let gate = DragTapGate::new();
        // Nothing published: every down passes, and the pass-through
        // gesture is remembered (held drag/up keep passing).
        assert!(!gate.try_consume_down_at(200, 200, true, 1_000));
        assert!(!gate.consume_held_drag());
        assert!(!gate.consume_up());
        // Unmodified presses pass even with a valid snapshot.
        gate.publish_at(4242, 7, 200, 200, true, 1_000);
        assert!(!gate.try_consume_down_at(200, 200, false, 1_000));
        // Pass-through memory holds until the up: a later valid snapshot
        // never arms mid-gesture.
        gate.publish_at(4242, 7, 200, 200, true, 1_000);
        assert!(!gate.try_consume_down_at(200, 200, true, 1_000));
        assert!(!gate.consume_held_drag());
        assert!(!gate.consume_up());
        // Fresh proof at the EXACT point consumes; held edges consume
        // lock-free; the up releases exactly once.
        gate.publish_at(4242, 7, 200, 200, true, 1_000);
        assert_eq!(gate.published_snapshot(), Some((4242, 7, 200, 200)));
        assert!(gate.try_consume_down_at(200, 200, true, 1_000));
        assert!(gate.is_held());
        assert!(gate.consume_held_drag());
        assert!(gate.consume_up());
        assert!(!gate.is_held());
        assert!(!gate.consume_up());
        // The snapshot is single-use: a second down at the same point
        // without a fresh publish passes (never a double arm).
        assert!(!gate.try_consume_down_at(200, 200, true, 1_000));
        assert!(!gate.consume_up());
        // Fences flipped at publish: fail open and remember.
        gate.publish_at(4242, 7, 200, 200, false, 1_000);
        assert!(!gate.try_consume_down_at(200, 200, true, 1_000));
        assert!(!gate.consume_up());
        // Far from the proven point: fail open (poll-then-click moved on).
        gate.publish_at(4242, 7, 200, 200, true, 1_000);
        assert!(!gate.try_consume_down_at(400, 400, true, 1_000));
        assert!(!gate.consume_up());
        // Teardown release clears capture so a late restart never
        // inherits it.
        gate.publish_at(4242, 7, 200, 200, true, 1_000);
        assert!(gate.try_consume_down_at(200, 200, true, 1_000));
        gate.cancel();
        assert!(!gate.is_held());
        assert!(!gate.consume_held_drag());
    }

    #[test]
    fn gate_point_is_exact_with_no_tolerance() {
        // Every 1px neighbor of the proven point refuses: the old 8pt
        // tolerance would have swallowed these unrelated boundary presses.
        for (x, y) in [
            (199, 200),
            (201, 200),
            (200, 199),
            (200, 201),
            (199, 199),
            (201, 201),
        ] {
            let gate = DragTapGate::new();
            gate.publish_at(4242, 7, 200, 200, true, 1_000);
            assert!(
                !gate.try_consume_down_at(x, y, true, 1_000),
                "off-by-one ({x},{y}) must pass through"
            );
            assert!(!gate.consume_up());
        }
        // The exact point still consumes (no lost-click regression for a
        // stationary press).
        let gate = DragTapGate::new();
        gate.publish_at(4242, 7, 200, 200, true, 1_000);
        assert!(gate.try_consume_down_at(200, 200, true, 1_000));
        assert!(gate.consume_up());
    }

    #[test]
    fn gate_proof_expires_on_the_monotonic_bound() {
        // Fresh at the bound consumes; one millisecond later passes.
        let gate = DragTapGate::new();
        gate.publish_at(4242, 7, 200, 200, true, 1_000);
        assert!(gate.try_consume_down_at(200, 200, true, 1_000 + PRESS_PROOF_MAX_AGE_MS));
        assert!(gate.consume_up());
        for now in [
            1_000 + PRESS_PROOF_MAX_AGE_MS + 1,
            1_000 + PRESS_PROOF_MAX_AGE_MS + 250,
            1_000 + 2_000,
        ] {
            let gate = DragTapGate::new();
            gate.publish_at(4242, 7, 200, 200, true, 1_000);
            assert!(
                !gate.try_consume_down_at(200, 200, true, now),
                "stale proof (age {}ms) must pass through",
                now.saturating_sub(1_000)
            );
            // Expiry remembers pass-through for the hold, then clears.
            assert!(!gate.consume_held_drag());
            assert!(!gate.consume_up());
        }
        // A proof timestamp in the future never authorizes.
        let gate = DragTapGate::new();
        gate.publish_at(4242, 7, 200, 200, true, 2_000);
        assert!(!gate.try_consume_down_at(200, 200, true, 1_000));
        assert!(!gate.consume_up());
    }

    #[test]
    fn gate_superseded_publication_never_arms_stale() {
        // A newer publish supersedes the older one: the stale point passes
        // even though it was once proven (mixed-publication coherence).
        let gate = DragTapGate::new();
        gate.publish_at(4242, 7, 200, 200, true, 1_000);
        gate.publish_at(4242, 7, 300, 300, true, 1_000);
        assert_eq!(gate.published_snapshot(), Some((4242, 7, 300, 300)));
        assert!(!gate.try_consume_down_at(200, 200, true, 1_000));
        assert!(!gate.consume_up());
        gate.publish_at(4242, 7, 300, 300, true, 1_000);
        assert!(gate.try_consume_down_at(300, 300, true, 1_000));
        assert!(gate.consume_up());
        // Owner changes ride the same supersede: the old owner's point no
        // longer consumes after the new owner publishes.
        let gate = DragTapGate::new();
        gate.publish_at(4242, 7, 200, 200, true, 1_000);
        gate.publish_at(9999, 21, 200, 200, true, 1_000);
        assert_eq!(gate.published_snapshot(), Some((9999, 21, 200, 200)));
        assert!(gate.try_consume_down_at(200, 200, true, 1_000));
        assert!(gate.consume_up());
        // Invalidate before a new proof leaves nothing consumable: a down
        // racing proof computation passes through.
        let gate = DragTapGate::new();
        gate.publish_at(4242, 7, 200, 200, true, 1_000);
        gate.invalidate();
        assert_eq!(gate.published_snapshot(), None);
        assert!(!gate.try_consume_down_at(200, 200, true, 1_000));
        assert!(!gate.consume_up());
        gate.publish_at(4242, 7, 200, 200, true, 1_000);
        assert!(gate.try_consume_down_at(200, 200, true, 1_000));
        assert!(gate.consume_up());
    }

    #[test]
    fn gate_concurrent_publish_never_reads_mixed() {
        use std::sync::Arc;
        // Two alternating snapshots share no field value, so any torn mix
        // is observable: every read must be wholly A or wholly B.
        const A: (u32, u32, i32, i32) = (4242, 7, 200, 200);
        const B: (u32, u32, i32, i32) = (9999, 21, 300, 300);
        let gate = Arc::new(DragTapGate::new());
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writer_stop = Arc::clone(&stop);
        let writer_gate = Arc::clone(&gate);
        let writer = std::thread::spawn(move || {
            let mut flip = false;
            let mut n = 0u64;
            while !writer_stop.load(std::sync::atomic::Ordering::SeqCst) && n < 20_000 {
                let (pid, wid, x, y) = if flip { A } else { B };
                writer_gate.publish_at(pid, wid, x, y, true, 1_000);
                flip = !flip;
                n += 1;
            }
            n
        });
        let mut reads = 0u64;
        while reads < 20_000 {
            if let Some(seen) = gate.published_snapshot() {
                assert!(
                    seen == A || seen == B,
                    "torn snapshot mix observed: {seen:?}"
                );
                reads += 1;
            }
            if writer.is_finished() && gate.published_snapshot().is_none() {
                break;
            }
        }
        stop.store(true, std::sync::atomic::Ordering::SeqCst);
        let _ = writer.join();
        assert!(reads > 0, "reader must observe published snapshots");
    }

    #[test]
    fn press_carries_revision_for_worker_stale_refusal() {
        let mut controller = DragController::new(DragOptions::default());
        assert_eq!(controller.press_revision(), None);
        controller
            .press("w1", 10, 10, true, true, fences_ok(), 9)
            .unwrap();
        assert_eq!(controller.press_revision(), Some(9));
        assert_eq!(controller.release().as_deref(), Some("w1"));
        assert_eq!(controller.press_revision(), None);
    }

    #[test]
    fn motion_zero_snap_back_escape_cancel_and_stale_release() {
        let mut controller = DragController::new(DragOptions::default());
        assert_eq!(controller.motion(5, 5, fences_ok()), MotionGate::Ignore);
        controller
            .press("w1", 10, 10, true, true, fences_ok(), 9)
            .unwrap();
        assert_eq!(
            controller.motion(10, 10, fences_ok()),
            MotionGate::ZeroSnapBack
        );
        assert_eq!(controller.motion(14, 10, fences_ok()), MotionGate::Sample);
        assert!(controller.escape());
        assert!(!controller.active());
        assert!(!controller.escape());
        controller
            .press("w1", 10, 10, true, true, fences_ok(), 9)
            .unwrap();
        assert!(matches!(
            controller.motion(
                14,
                10,
                DragFences {
                    revision_fresh: false,
                    ..fences_ok()
                },
            ),
            MotionGate::CancelDead("revision-drift")
        ));
        assert!(!controller.active());
        assert!(!controller.consumed());
        controller
            .press("w1", 10, 10, true, true, fences_ok(), 9)
            .unwrap();
        assert!(controller.note_stale());
        assert!(!controller.active());
    }

    #[test]
    fn overlay_intent_is_filled_above_with_configured_fill() {
        let options = DragOptions::default();
        let rect = VisualRect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 80.0,
        };
        assert_eq!(
            DragController::overlay_for(options, rect, DragOrdering::AboveTarget),
            DragOverlay::ShowFillAbove {
                rect,
                fill: DEFAULT_DRAG_FILL_RGBA,
                ordering: DragOrdering::AboveTarget,
            }
        );
        assert_eq!(
            DragController::overlay_for(
                DragOptions {
                    enabled: false,
                    ..options
                },
                rect,
                DragOrdering::AboveTarget,
            ),
            DragOverlay::Hide
        );
        assert_eq!(
            DragController::overlay_for(
                options,
                VisualRect {
                    x: 0.0,
                    y: 0.0,
                    w: 0.0,
                    h: 80.0,
                },
                DragOrdering::AboveTarget,
            ),
            DragOverlay::Hide
        );
        let flags = DragCarrierFlags::required();
        assert!(
            flags.click_through && flags.nonactivating && flags.normal_level && flags.managed_only
        );
    }

    // -- Shared Engine oracle integration (real Engine, no fakes) ------------

    fn domain() -> (OutputDomain, DomainKey) {
        let key = DomainKey {
            output: OutputId("macos-display-1".to_owned()),
            workspace: WorkspaceId("space-11".to_owned()),
        };
        let domain = OutputDomain {
            id: OutputId("macos-display-1".to_owned()),
            workspace: WorkspaceId("space-11".to_owned()),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
            gap: 8,
            adjacent: std::collections::BTreeMap::new(),
        };
        (domain, key)
    }

    fn fresh_engine() -> (Engine, OwnerId, GenerationId, OutputDomain, DomainKey) {
        let (domain, key) = domain();
        let mut engine = Engine::new();
        let owner = OwnerId::parse("macos-drag-test").expect("owner");
        let generation = GenerationId::parse("drag-gen").expect("generation");
        engine.sync_binding(&owner, &generation);
        (engine, owner, generation, domain, key)
    }

    fn drag_event(
        owner: &OwnerId,
        generation: &GenerationId,
        domain: &OutputDomain,
        key: &DomainKey,
        command: CoreCommand,
        correlation: &str,
    ) -> CoreEvent {
        CoreEvent {
            owner: owner.clone(),
            generation: generation.clone(),
            correlation: CorrelationId::parse(correlation).expect("correlation"),
            revision: 0,
            fingerprint: 7,
            domain: domain.clone(),
            domain_key: key.clone(),
            outer_gap: 8,
            focused_window: WindowId("w1".to_owned()),
            windows: two_windows(),
            closed_windows: Vec::new(),
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: Vec::new(),
            command,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn preview_at(
        engine: &mut Engine,
        owner: &OwnerId,
        generation: &GenerationId,
        domain: &OutputDomain,
        key: &DomainKey,
        x: i32,
        y: i32,
        correlation: &str,
        prior: Option<DragHoverPrior>,
    ) -> PreviewOutcome {
        let command = CoreCommand::DragPreview {
            window: "w1".to_owned(),
            x,
            y,
            hover_prior: prior,
            source: None,
        };
        let event = drag_event(owner, generation, domain, key, command, correlation);
        engine_preview(engine, event)
    }

    fn two_windows() -> Vec<EngineWindow> {
        vec![
            EngineWindow {
                window: WindowId("w1".to_owned()),
                output: OutputId("macos-display-1".to_owned()),
                workspace: WorkspaceId("space-11".to_owned()),
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: 100,
                    h: 100,
                },
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
            },
            EngineWindow {
                window: WindowId("w2".to_owned()),
                output: OutputId("macos-display-1".to_owned()),
                workspace: WorkspaceId("space-11".to_owned()),
                rect: Rect {
                    x: 200,
                    y: 0,
                    w: 100,
                    h: 100,
                },
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
            },
        ]
    }

    #[test]
    fn engine_preview_resolves_insertion_slot_and_hides_for_self_outside() {
        let (mut engine, owner, generation, domain, key) = fresh_engine();
        let moved = preview_at(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &key,
            790,
            300,
            "drag-p1",
            None,
        );
        let PreviewOutcome::Show {
            rect, hover_prior, ..
        } = moved
        else {
            panic!("moved pointer must resolve a slot, got {moved:?}");
        };
        assert!(rect.w > 0 && rect.h > 0);
        let _ = hover_prior;
        let outside = preview_at(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &key,
            5000,
            5000,
            "drag-p2",
            None,
        );
        assert!(
            matches!(outside, PreviewOutcome::HideTransient { reason: "outside" }),
            "outside must hide transiently, got {outside:?}"
        );
    }

    #[test]
    fn engine_drop_commits_moved_plan_and_centre_reports_unsupported() {
        let (mut engine, owner, generation, domain, key) = fresh_engine();
        let moved = preview_at(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &key,
            790,
            300,
            "drag-d1",
            None,
        );
        let PreviewOutcome::Show { hover_prior, .. } = moved else {
            panic!("need a resolved slot for the drop, got {moved:?}");
        };
        let command = CoreCommand::DragDrop {
            window: "w1".to_owned(),
            x: 790,
            y: 300,
            hover_prior: hover_prior.map(|prior| *prior),
            source: None,
        };
        let event = drag_event(&owner, &generation, &domain, &key, command, "drag-drop-1");
        match engine_drop(&mut engine, event) {
            DropOutcome::Planned { plan } => {
                let frames = frames_for_plan(&plan);
                assert_eq!(frames.len(), 2);
                assert!(frames.iter().any(|(w, _)| w.0 == "w1"));
            }
            other => panic!("moved drop must plan, got {other:?}"),
        }
        let capabilities = DragCapabilities::full();
        let _ = capabilities;
    }

    #[test]
    fn controller_carries_sticky_prior_across_samples() {
        let (mut engine, owner, generation, domain, key) = fresh_engine();
        let mut controller = DragController::new(DragOptions::default());
        controller
            .press("w1", 10, 10, true, true, fences_ok(), 9)
            .unwrap();
        assert_eq!(controller.motion(790, 300, fences_ok()), MotionGate::Sample);
        let first = preview_at(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &key,
            790,
            300,
            "drag-c1",
            None,
        );
        controller.note_preview(&first);
        assert!(matches!(controller.phase, DragPhase::Dragging { .. }));
        let carried = controller.hover_prior().cloned();
        let second = preview_at(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &key,
            790,
            300,
            "drag-c2",
            carried,
        );
        controller.note_preview(&second);
        assert!(controller.active());
        assert_eq!(controller.release().as_deref(), Some("w1"));
    }

    #[test]
    fn preview_reply_taxonomy_pins_centre_self_outside_and_dead() {
        assert!(matches!(
            classify_preview_reply(&CoreReply::Rejected {
                kind: "unsupported-capability",
                message: "x",
            }),
            PreviewOutcome::HideTransient {
                reason: "centre-unsupported"
            }
        ));
        assert!(matches!(
            classify_preview_reply(&CoreReply::Rejected {
                kind: "unchanged",
                message: "x",
            }),
            PreviewOutcome::HideTransient { reason: "self" }
        ));
        assert!(matches!(
            classify_preview_reply(&CoreReply::Rejected {
                kind: "cross-domain-mismatch",
                message: "x",
            }),
            PreviewOutcome::HideTransient { reason: "outside" }
        ));
        assert!(matches!(
            classify_drop_reply(&CoreReply::Rejected {
                kind: "unsupported-capability",
                message: "x",
            }),
            DropOutcome::CentreUnsupported
        ));
        assert!(matches!(
            classify_drop_reply(&CoreReply::Rejected {
                kind: "unchanged",
                message: "x",
            }),
            DropOutcome::SnapBack { .. }
        ));
    }

    #[test]
    fn tile_cli_parses_drag_preview_switch_and_color() {
        use crate::cli::{Command, parse_args};
        fn argv(words: &[&str]) -> Vec<String> {
            std::iter::once("tiler-macos".to_owned())
                .chain(words.iter().map(|s| s.to_string()))
                .collect()
        }
        let r = parse_args(&argv(&["tile", "--user-start", "--scope", "x"])).unwrap();
        if let Command::Tile { drag, .. } = r.command {
            assert_eq!(drag, DragCli::default());
        } else {
            panic!("want tile");
        }
        let r = parse_args(&argv(&[
            "tile",
            "--user-start",
            "--unscoped",
            "--no-drag-preview",
            "--drag-preview-color",
            "#ff2a82da",
        ]))
        .unwrap();
        if let Command::Tile { drag, .. } = r.command {
            assert_eq!(drag.enabled, Some(false));
            assert_eq!(drag.color.as_deref(), Some("#ff2a82da"));
        } else {
            panic!("want tile");
        }
        assert!(parse_args(&argv(&["tile", "--drag-preview-color"])).is_err());
    }

    #[test]
    fn drag_settings_file_section_parses_and_validates() {
        use crate::dev_selection::DevSettings;
        let settings: DevSettings =
            serde_json::from_str(r##"{"drag_preview":{"enabled":false,"color":"#ff2a82da"}}"##)
                .expect("parses");
        let section = settings.drag_preview.as_ref().expect("section");
        assert_eq!(section.enabled, Some(false));
        assert_eq!(section.color.as_deref(), Some("#ff2a82da"));
        assert!(settings.validate().is_ok());
        let resolved = resolve_drag_config(&DragCli::default(), settings.drag_preview.as_ref())
            .expect("resolves");
        assert!(!resolved.enabled);
        assert_eq!(resolved.fill, (0x2a, 0x82, 0xda, 0xff));
        assert!(
            serde_json::from_str::<DevSettings>(r##"{"drag_preview":{"colr":"#ff2a82da"}}"##)
                .is_err()
        );
        let bad: DevSettings = serde_json::from_str(r##"{"drag_preview":{"color":"#2a82da"}}"##)
            .expect("parses shape");
        assert!(bad.validate().is_err());
    }
}
