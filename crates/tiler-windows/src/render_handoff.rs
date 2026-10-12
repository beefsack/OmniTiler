#![forbid(unsafe_code)]
//! Dedicated render thread handoff (stage 2, option B).
//!
//! Owner -> renderer carries replaceable desired frames (latest-wins) plus
//! monotonic safety epochs; renderer -> owner carries acknowledged applied
//! state (generation, outcome, owned-pixel checksum, bounded failure) plus
//! immutable overlay identities. All cross-thread state uses safe standard
//! primitives only (`Mutex`, `Condvar`, atomics, `Arc`): no `unsafe`,
//! no `Send`/`Sync` impls, no raw handles across threads. HWND `u64`s are
//! data only; the creating renderer thread owns every window lifecycle.
//!
//! Contract (from `docs/research/windows-port/owner-threading.md`):
//! - Latest-wins applies to replaceable frames only. Ordered Hide / Cancel /
//!   suspend / reset epochs invalidate pending Shows: a frame whose safety
//!   predates the carrier hide watermark never presents.
//! - Owner retains observation, Engine, foreign actuation and recovery. The
//!   renderer never runs Engine or layout policy; it only rechecks fresh
//!   native liveness/visibility/foreground/band facts against the desired
//!   epoch and refuses stale or unreadable targets narrowly.
//! - Inspection and logs consume acknowledged applied generations and bounded
//!   outcomes, never owner desired state masquerading as rendered pixels.
//! - Where existing behavior requires hide before actuation (preview finish),
//!   the owner waits on the renderer acknowledgement outside callbacks with
//!   a bounded timeout; a missing ack is reported truthfully and the bumped
//!   safety epoch alone prevents any stale-show replay.
//! - Startup and stop need readiness/completion acknowledgement and bounded
//!   failure reporting. A stuck renderer keeps the process render slot so no
//!   overlapping replacement can start; recovery is process exit.
//! - Timing and outcomes travel to the owner writer; the renderer never logs.

use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

use tiler_core::geometry::Rect;

/// Which owned carrier a frame or acknowledgement belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderCarrier {
    Border,
    Underlay,
    Preview,
}

impl RenderCarrier {
    /// Stable carrier index (border 0, underlay 1, preview 2).
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Border => 0,
            Self::Underlay => 1,
            Self::Preview => 2,
        }
    }

    /// Closed log vocabulary for the carrier.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Border => "border",
            Self::Underlay => "underlay",
            Self::Preview => "preview",
        }
    }

    /// All carriers in index order.
    #[must_use]
    pub const fn all() -> [RenderCarrier; 3] {
        [Self::Border, Self::Underlay, Self::Preview]
    }
}

/// Owner-observed process/window lifetime identity carried as safe data in
/// every desired frame. The renderer compares it against fresh native reads
/// (pid, process creation, visible-membership tag) before presenting, so a
/// recycled HWND value can never present as the old target. Plain data only:
/// never dereferenced, never a handle. The opaque Engine token stays the log
/// fence; this struct is the renderer-enforced stale/recycled-ID fence. No
/// shared-core, protocol, or public-settings field is added for this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowLifetime {
    /// Owning pid observed at post time.
    pub pid: u32,
    /// Process creation string observed at post time.
    pub creation: String,
    /// Visible-membership lifetime tag (`None` when absent at post time).
    pub member_tag: Option<String>,
}

/// One group-underlay anchor candidate with its own lifetime identity. The
/// renderer validates each candidate individually against fresh native reads
/// and resolves the fresh lowest renderable member itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnderlayCandidate {
    pub hwnd: u64,
    pub lifetime: WindowLifetime,
}

/// Desired active-border frame: owner-observed geometry/style plus the fresh
/// target identity the renderer rechecks before presenting. `color` is the
/// configured fallback only: the renderer owns the live accent query and
/// resolves the painted colour itself from `use_theme` plus the fallback, so
/// an accent change repaints with no owner-side query. `foreground` is the
/// owner-observed foreground identity at post time (normally the target
/// itself): the renderer refuses when the live foreground identity moved on,
/// so a late switch (game, suspend) hides instead of presenting. `fulls` are
/// the owner monitor full rects (safe data): the renderer re-evaluates the
/// captionless-fullscreen predicate itself instead of trusting owner-tick
/// suppression. `fullscreen_bypass` carries the explicit owner proof behind
/// the verified managed-fullscreen bypass (no silent exemption).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BorderDesired {
    /// Per-carrier monotonic generation pairing this frame with its ack.
    pub generation: u64,
    /// Global safety epoch at post time; stale when below the hide watermark.
    pub safety: u64,
    pub outer: Rect,
    pub style_px: (i32, i32, i32),
    /// Configured fallback colour (renderer resolves the accent itself).
    pub color: (u8, u8, u8),
    /// Theme choice: resolve the live system accent when true.
    pub use_theme: bool,
    /// Opaque Engine token (log fence, never a raw handle).
    pub target_token: String,
    /// Fresh foreground HWND at post time (data only; renderer rechecks).
    pub target_hwnd: u64,
    /// Renderer-enforced lifetime identity for the target.
    pub lifetime: WindowLifetime,
    /// Owner-observed foreground identity at post time.
    pub foreground: WindowLifetime,
    /// Owner monitor full rects for the renderer fullscreen predicate.
    pub fulls: Vec<Rect>,
    /// Explicit owner proof for the managed-fullscreen bypass.
    pub fullscreen_bypass: bool,
    /// Target's `WS_EX_TOPMOST` band at post time (renderer rechecks).
    pub topmost: bool,
}

/// Desired group-underlay frame. `candidates` are the Engine member windows
/// with their lifetime identities: the renderer revalidates each one against
/// fresh native reads and re-resolves the fresh lowest-in-z anchor itself
/// (including its own border surface in the correct lower plane when that
/// surface is visible, same-group, and same-band).
/// `anchor_hwnd`/`anchor_lifetime` is the owner-chosen fallback the renderer
/// uses only when the order is unreadable but the fallback is live.
/// `foreground`/`fulls` fence the frame like the border: a late foreground
/// switch or fullscreen change refuses instead of presenting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnderlayDesired {
    pub generation: u64,
    pub safety: u64,
    pub outer: Rect,
    /// `(alpha, r, g, b)` fill.
    pub color: (u8, u8, u8, u8),
    pub anchor_token: String,
    pub anchor_hwnd: u64,
    pub anchor_lifetime: WindowLifetime,
    pub candidates: Vec<UnderlayCandidate>,
    /// Owner-observed foreground identity at post time.
    pub foreground: WindowLifetime,
    /// Owner monitor full rects for the renderer fullscreen predicate.
    pub fulls: Vec<Rect>,
    pub topmost: bool,
}

/// Desired drop-preview frame: an Engine-resolved target slot above windows.
/// The preview always presents in the topmost plane (like-for-like with the
/// same-thread path); the mover band is never a requirement, only the mover
/// lifetime identity is fenced. `foreground` fences the frame against a late
/// foreground switch (game/suspend): the mover itself need not be focused
/// (unfocused Win+Left holds keep their existing semantics), but a changed
/// foreground identity refuses the present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewDesired {
    pub generation: u64,
    pub safety: u64,
    pub outer: Rect,
    /// `(alpha, r, g, b)` fill.
    pub color: (u8, u8, u8, u8),
    pub anchor_token: String,
    /// Gesture mover HWND (lifetime fence only, never a band requirement).
    pub anchor_hwnd: u64,
    /// Renderer-enforced lifetime identity for the mover.
    pub lifetime: WindowLifetime,
    /// Owner-observed foreground identity at post time (`None` when the
    /// foreground read was unreadable: the renderer refuses narrowly on
    /// this fence, like any other unreadable fact per the binding contract.
    /// Unfocused Win+Left holds keep their semantics when the foreground is
    /// readable and unchanged; only an unprovable (unreadable) foreground
    /// refuses, so no stale show can present without a proven unchanged
    /// foreground. The mover fence still applies independently, and the
    /// owner re-fences on its next refresh).
    pub foreground: Option<WindowLifetime>,
}

/// One replaceable desired frame for any carrier (latest-wins slot payload).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesiredFrame {
    Border(BorderDesired),
    Underlay(UnderlayDesired),
    Preview(PreviewDesired),
}

impl DesiredFrame {
    /// Carrier this frame belongs to.
    #[must_use]
    pub const fn carrier(&self) -> RenderCarrier {
        match self {
            Self::Border(_) => RenderCarrier::Border,
            Self::Underlay(_) => RenderCarrier::Underlay,
            Self::Preview(_) => RenderCarrier::Preview,
        }
    }

    /// Generation pairing this frame with its acknowledgement.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        match self {
            Self::Border(frame) => frame.generation,
            Self::Underlay(frame) => frame.generation,
            Self::Preview(frame) => frame.generation,
        }
    }

    /// Safety epoch at post time.
    #[must_use]
    pub const fn safety(&self) -> u64 {
        match self {
            Self::Border(frame) => frame.safety,
            Self::Underlay(frame) => frame.safety,
            Self::Preview(frame) => frame.safety,
        }
    }
}

/// Acknowledged render outcome. `Refused` carries the closed narrow reason
/// (stale target, unreadable facts, band drift); the renderer hid instead
/// (verified) and visibility reads hidden. `Unknown` means the carrier state
/// could not be proven either way (unverified hide after a failure): the
/// owner must not report a known visibility and must not act on a hide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderOutcome {
    Hidden,
    Shown,
    Moved,
    Redrew,
    Unchanged,
    Refused(&'static str),
    Unknown(&'static str),
}

impl RenderOutcome {
    /// Closed log vocabulary for the outcome.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::Shown => "shown",
            Self::Moved => "moved",
            Self::Redrew => "redrew",
            Self::Unchanged => "unchanged",
            Self::Refused(_) => "refused",
            Self::Unknown(_) => "unknown",
        }
    }
}

/// Bounded applied content carried in every show-path acknowledgement: the
/// geometry, style, actual presented colour, and target the renderer really
/// presented. Inspection and stop snapshots consume this, never a possibly
/// stale posted note and never the owner fallback colour (which differs
/// from the DIB when the theme accent resolved). `target` truncates to 64
/// chars; multineline/control content never enters (tokens are opaque ids).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedContent {
    pub target: String,
    pub outer: [i32; 4],
    /// Actual presented colour hex (`#rrggbb`), accent-resolved for the border.
    pub color_hex: String,
    pub style_px: Option<[i32; 3]>,
    /// Placement band actually used (resolved anchor band for the underlay).
    pub topmost: bool,
    pub members: usize,
}

impl AppliedContent {
    /// New bounded content, truncating the opaque target to 64 chars.
    #[must_use]
    pub fn new(
        target: &str,
        outer: [i32; 4],
        color_hex: String,
        style_px: Option<[i32; 3]>,
        topmost: bool,
        members: usize,
    ) -> Self {
        Self {
            target: target.chars().take(64).collect(),
            outer,
            color_hex,
            style_px,
            topmost,
            members,
        }
    }
}

/// Acknowledged applied state for one carrier: the renderer echoes the
/// applied frame generation plus what it actually presented. Owner
/// inspection and logs consume this, never desired state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarrierAck {
    /// Generation of the applied frame (or of the hide that hid the carrier).
    pub generation: u64,
    /// Safety epoch the renderer had applied up to (hide watermark echoed).
    pub safety: u64,
    pub outcome: RenderOutcome,
    /// FNV-1a checksum over the owned DIB bytes actually presented (0 when
    /// nothing is presented). Same painter contract as the same-thread path,
    /// so checksums stay comparable across the migration.
    pub checksum: u64,
    /// Renderer-side present/move span in microseconds (owner log evidence).
    /// Covers the present call only, never transport queue wait: compare
    /// against `queue_age_us` for the handoff wait.
    pub span_us: u64,
    /// Transport queue age in microseconds: `Instant::now()` at renderer take
    /// minus the owner post stamp. Safe `std::time::Instant` only, no clock
    /// abstraction. Hide-only acks carry 0 (no frame waited).
    pub queue_age_us: u64,
    /// Bounded renderer-side failure detail (`None` on success paths).
    pub last_error: Option<String>,
    /// True only when a creator-side readback verified the surface hidden
    /// behind this ack. A `Hidden` outcome with `false` here, or any
    /// `Unknown` outcome, proves nothing: the owner reports unknown
    /// visibility and a hide wait stays unsatisfied.
    pub hidden_proven: bool,
    /// Fence strength behind the identity check: true when the desired
    /// lifetime carried a membership tag that the renderer compared
    /// (`pid`/`creation`/`tag` all matched). False means pid/creation-only
    /// (untagged target): same-process HWND reuse stays owner-fenced per
    /// tick and is reported honestly, never claimed as tag-proven.
    pub identity_proven: bool,
    /// Bounded content actually presented on show paths (`None` on hides,
    /// refusals, and unknown paths). Always fresh from the presented frame:
    /// a missing note pairing never resurrects an older payload as new.
    pub applied: Option<AppliedContent>,
}

/// Fresh native facts the renderer rechecks on its own thread immediately
/// before presenting. All reads are renderer-local; nothing here runs owner
/// observation, Engine, or foreign writes. `visible` already folds
/// non-iconic and DWM-uncloaked where probed; `zoom_ok` carries the
/// maximized suppression; `fullscreen` carries the renderer-evaluated
/// captionless-fullscreen predicate over the carried monitor fulls (no
/// owner-tick trust); `foreground_ok` compares the live foreground lifetime
/// identity against the desired foreground (a changed foreground, e.g. a
/// game or suspend switch, refuses even when HWNDs coincide).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreshFacts {
    /// Target/anchor window still exists.
    pub alive: bool,
    /// Fresh pid/creation/tag still match the desired lifetime identity.
    pub identity_ok: bool,
    /// Target/anchor still visible (and non-iconic/non-cloaked where probed).
    pub visible: bool,
    /// Target still unmaximized (border/underlay suppression; movers exempt).
    pub zoom_ok: bool,
    /// Target not captionless-fullscreen (border/underlay; movers exempt).
    pub fullscreen: bool,
    /// Live foreground lifetime still matches the desired foreground
    /// (border, underlay, and preview: a late switch refuses everywhere).
    pub foreground_ok: bool,
    /// `WS_EX_TOPMOST` band still matches the desired band (preview exempt:
    /// the mover band is never a preview requirement).
    pub band_ok: bool,
}

/// Pure freshness gate: stale or unreadable facts refuse narrowly with a
/// closed reason. The underlay anchor liveness rides `alive`/`visible`;
/// preview needs its mover alive with matching identity plus an unchanged
/// foreground (its rect is Engine-resolved and its plane is always topmost,
/// so band, zoom, and fullscreen never refuse it).
pub fn freshness_gate(carrier: RenderCarrier, facts: &FreshFacts) -> Result<(), &'static str> {
    if !facts.alive {
        return match carrier {
            RenderCarrier::Border => Err("target-gone"),
            RenderCarrier::Underlay => Err("anchor-missing"),
            RenderCarrier::Preview => Err("mover-gone"),
        };
    }
    if !facts.identity_ok {
        return match carrier {
            RenderCarrier::Border => Err("identity-changed"),
            RenderCarrier::Underlay => Err("anchor-identity-changed"),
            RenderCarrier::Preview => Err("mover-identity-changed"),
        };
    }
    if !facts.visible {
        return match carrier {
            RenderCarrier::Border => Err("target-hidden"),
            RenderCarrier::Underlay => Err("anchor-hidden"),
            RenderCarrier::Preview => Err("mover-gone"),
        };
    }
    if carrier != RenderCarrier::Preview && !facts.zoom_ok {
        return match carrier {
            RenderCarrier::Border => Err("target-maximized"),
            _ => Err("anchor-maximized"),
        };
    }
    if carrier != RenderCarrier::Preview && facts.fullscreen {
        return match carrier {
            RenderCarrier::Border => Err("target-fullscreen"),
            _ => Err("anchor-fullscreen"),
        };
    }
    if !facts.foreground_ok {
        return match carrier {
            RenderCarrier::Border => Err("foreground-changed"),
            RenderCarrier::Underlay => Err("anchor-foreground-changed"),
            RenderCarrier::Preview => Err("mover-foreground-changed"),
        };
    }
    if carrier != RenderCarrier::Preview && !facts.band_ok {
        return Err("band-changed");
    }
    Ok(())
}

/// Renderer lifecycle: exactly one renderer at a time. A failed or missing
/// creator-side destroy retains `Stopping` so no overlapping replacement can
/// start; only a successful destroy acknowledgement completes the stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderState {
    /// No renderer worker.
    Stopped,
    /// Worker requested, carrier creation pending.
    Starting,
    /// Carriers created, frames live.
    Running,
    /// Stop requested, destroy acknowledgement pending.
    Stopping,
}

/// Lifecycle transition refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderLifecycleError {
    /// Start requested while not stopped: no overlapping worker.
    AlreadyActive,
    /// Stop/ack with no matching pending step.
    NothingPending,
}

/// Truthful stop outcome; clean only with a successful acknowledgement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderStopReport {
    /// Creator-side destroy reported success for every carrier.
    pub destroyed_ok: bool,
    /// Clean stop: acknowledgement present and successful.
    pub clean: bool,
}

/// Single-owner renderer lifecycle.
#[derive(Debug)]
pub struct RenderLifecycle {
    state: RenderState,
}

impl Default for RenderLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderLifecycle {
    /// New stopped lifecycle.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: RenderState::Stopped,
        }
    }

    /// Current state.
    #[must_use]
    pub const fn state(&self) -> RenderState {
        self.state
    }

    /// Request the worker. `Stopped -> Starting`; anything else refuses.
    pub fn start(&mut self) -> Result<(), RenderLifecycleError> {
        if self.state != RenderState::Stopped {
            return Err(RenderLifecycleError::AlreadyActive);
        }
        self.state = RenderState::Starting;
        Ok(())
    }

    /// Readiness: carriers created. `Starting -> Running`.
    pub fn note_ready(&mut self) -> Result<(), RenderLifecycleError> {
        if self.state != RenderState::Starting {
            return Err(RenderLifecycleError::NothingPending);
        }
        self.state = RenderState::Running;
        Ok(())
    }

    /// Creation failure: `Starting -> Stopped` (no worker holds resources,
    /// so a later start is safe).
    pub fn create_failed(&mut self) -> Result<(), RenderLifecycleError> {
        if self.state != RenderState::Starting {
            return Err(RenderLifecycleError::NothingPending);
        }
        self.state = RenderState::Stopped;
        Ok(())
    }

    /// Graceful stop request. Idempotent in `Stopping` so the owner can
    /// re-drive destroy attempts after a failure.
    pub fn request_stop(&mut self) -> Result<(), RenderLifecycleError> {
        match self.state {
            RenderState::Running | RenderState::Starting | RenderState::Stopping => {
                self.state = RenderState::Stopping;
                Ok(())
            }
            RenderState::Stopped => Err(RenderLifecycleError::NothingPending),
        }
    }

    /// Destroy acknowledgement. Failure retains `Stopping` (never claims
    /// clean, never frees the worker slot); success completes the stop.
    pub fn note_destroyed(
        &mut self,
        destroyed_ok: bool,
    ) -> Result<RenderStopReport, RenderLifecycleError> {
        if self.state != RenderState::Stopping {
            return Err(RenderLifecycleError::NothingPending);
        }
        if !destroyed_ok {
            return Ok(RenderStopReport {
                destroyed_ok: false,
                clean: false,
            });
        }
        self.state = RenderState::Stopped;
        Ok(RenderStopReport {
            destroyed_ok: true,
            clean: true,
        })
    }
}

/// Process-level renderer slot: exactly one renderer worker per process.
/// Reserved at spawn, released only after creator-side successful destroy
/// plus exit. A hung or unclean stop (including a dropped handle) keeps the
/// reserve so no overlapping worker can start; recovery is process exit,
/// never a duplicate worker. Never released on `Drop`: only an explicit
/// clean-stop or spawn-failure release frees the slot.
static RENDER_SLOT_RESERVED: AtomicU64 = AtomicU64::new(0);

/// Reserve the process render slot. True when newly reserved, false when a
/// worker already holds it (the caller must refuse to spawn).
pub fn render_slot_try_reserve() -> bool {
    RENDER_SLOT_RESERVED
        .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
}

/// Release the process render slot after a clean stop or a spawn failure.
pub fn render_slot_release() {
    RENDER_SLOT_RESERVED.store(0, Ordering::SeqCst);
}

/// True while a renderer worker holds the process slot.
#[must_use]
pub fn render_slot_reserved() -> bool {
    RENDER_SLOT_RESERVED.load(Ordering::SeqCst) != 0
}

#[cfg(test)]
static RENDER_SLOT_TEST_SERIAL: Mutex<()> = Mutex::new(());

/// Serialize tests that mutate the process-global render slot.
#[cfg(test)]
pub fn render_slot_test_serial() -> std::sync::MutexGuard<'static, ()> {
    RENDER_SLOT_TEST_SERIAL
        .lock()
        .expect("render slot test serial")
}

/// Truncate a renderer failure detail for the bounded ack vocabulary.
#[must_use]
pub fn truncate_render_error(message: &str) -> String {
    const MAX: usize = 160;
    let mut out: String = message.chars().take(MAX).collect();
    if message.chars().count() > MAX {
        out.push_str("...");
    }
    out
}

/// Transport queue age in microseconds: renderer take time minus the owner
/// post stamp, saturating (a future stamp reports 0). Safe `std` only.
#[must_use]
pub fn queue_age_us_since(posted: Instant, now: Instant) -> u64 {
    now.saturating_duration_since(posted)
        .as_micros()
        .try_into()
        .unwrap_or(u64::MAX)
}

/// Shared render channels owned by the owner handle and the renderer thread.
///
/// Desired slots are latest-wins: each post replaces the pending frame, so a
/// slow renderer coalesces bursts to the newest geometry without an
/// unbounded backlog. Safety watermarks order hides before shows: a frame
/// whose safety predates the carrier watermark never presents, so no
/// late or reordered Show can resurrect a cancelled, suspended, finished or
/// reset carrier. Acknowledgements are single-entry applied mirrors plus a
/// `Condvar` signal, so the preview-finish wait is event-driven with no
/// polling latency.
pub struct RenderChannels {
    border_desired: Mutex<Option<(BorderDesired, Instant)>>,
    underlay_desired: Mutex<Option<(UnderlayDesired, Instant)>>,
    preview_desired: Mutex<Option<(PreviewDesired, Instant)>>,
    border_ack: Mutex<Option<CarrierAck>>,
    underlay_ack: Mutex<Option<CarrierAck>>,
    preview_ack: Mutex<Option<CarrierAck>>,
    /// Global safety epoch, bumped by every hide/cancel/suspend/reset.
    /// Frames carry the epoch at post time; the renderer refuses frames
    /// older than the carrier watermark.
    safety_epoch: AtomicU64,
    border_hide_mark: AtomicU64,
    underlay_hide_mark: AtomicU64,
    preview_hide_mark: AtomicU64,
    /// Hide generation per carrier: the acknowledgement echoes it, so the
    /// owner finish-wait pairs exactly the hide it requested.
    border_hide_gen: AtomicU64,
    underlay_hide_gen: AtomicU64,
    preview_hide_gen: AtomicU64,
    /// Immutable overlay identities published once the renderer creates each
    /// carrier window. The owner excludes exactly these from observation and
    /// WinEvents; cleared only after verified destruction on stop so a
    /// recycled HWND value can never linger as a stale exclusion.
    overlay_ids: Mutex<[Option<u64>; 3]>,
    /// Bumped on every identity publish or self-heal replacement.
    overlay_version: AtomicU64,
    /// Acknowledgement signal: notified after every ack post. The pending
    /// finish service observes it through the ack wake with a bounded
    /// timeout instead of polling.
    ack_signal: Condvar,
    ack_guard: Mutex<u64>,
    /// Stop requested by the owner; the renderer destroys on its creator
    /// thread and acks. Polled on the renderer quantum: no kernel wait
    /// handle crosses threads here.
    stop_flag: AtomicU64,
}

impl Default for RenderChannels {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderChannels {
    /// New channels with empty slots, zeroed epochs and no overlay IDs.
    #[must_use]
    pub fn new() -> Self {
        Self {
            border_desired: Mutex::new(None),
            underlay_desired: Mutex::new(None),
            preview_desired: Mutex::new(None),
            border_ack: Mutex::new(None),
            underlay_ack: Mutex::new(None),
            preview_ack: Mutex::new(None),
            safety_epoch: AtomicU64::new(0),
            border_hide_mark: AtomicU64::new(0),
            underlay_hide_mark: AtomicU64::new(0),
            preview_hide_mark: AtomicU64::new(0),
            border_hide_gen: AtomicU64::new(0),
            underlay_hide_gen: AtomicU64::new(0),
            preview_hide_gen: AtomicU64::new(0),
            overlay_ids: Mutex::new([None, None, None]),
            overlay_version: AtomicU64::new(0),
            ack_signal: Condvar::new(),
            ack_guard: Mutex::new(0),
            stop_flag: AtomicU64::new(0),
        }
    }

    /// Current global safety epoch (frames posted now carry this value).
    #[must_use]
    pub fn safety(&self) -> u64 {
        self.safety_epoch.load(Ordering::SeqCst)
    }

    /// Hide watermark for one carrier: frames older than this never present.
    #[must_use]
    pub fn hide_mark(&self, carrier: RenderCarrier) -> u64 {
        match carrier {
            RenderCarrier::Border => self.border_hide_mark.load(Ordering::SeqCst),
            RenderCarrier::Underlay => self.underlay_hide_mark.load(Ordering::SeqCst),
            RenderCarrier::Preview => self.preview_hide_mark.load(Ordering::SeqCst),
        }
    }

    /// Hide generation for one carrier: echoed by the hide acknowledgement.
    #[must_use]
    pub fn hide_gen(&self, carrier: RenderCarrier) -> u64 {
        match carrier {
            RenderCarrier::Border => self.border_hide_gen.load(Ordering::SeqCst),
            RenderCarrier::Underlay => self.underlay_hide_gen.load(Ordering::SeqCst),
            RenderCarrier::Preview => self.preview_hide_gen.load(Ordering::SeqCst),
        }
    }

    /// Post one desired frame, replacing any pending frame (latest-wins).
    /// Infallible: slots hold exactly one frame each, so posting never
    /// blocks and never grows a backlog. Stamps `Instant::now()` as the
    /// transport queue entry time; the renderer reports the take-minus-post
    /// age in the ack.
    pub fn post(&self, frame: DesiredFrame) {
        self.post_at(frame, Instant::now());
    }

    /// Post one desired frame with an explicit queue entry time. Production
    /// uses [`RenderChannels::post`]; tests inject an aged stamp to prove
    /// queue age separates from the present span without sleeps.
    pub fn post_at(&self, frame: DesiredFrame, at: Instant) {
        match frame {
            DesiredFrame::Border(frame) => {
                *self.border_desired.lock().expect("border desired lock") = Some((frame, at));
            }
            DesiredFrame::Underlay(frame) => {
                *self.underlay_desired.lock().expect("underlay desired lock") = Some((frame, at));
            }
            DesiredFrame::Preview(frame) => {
                *self.preview_desired.lock().expect("preview desired lock") = Some((frame, at));
            }
        }
    }

    /// Hide one carrier: bump the global safety epoch, stamp the carrier
    /// watermark and hide generation, and drop any pending desired frame so
    /// even an unread queued Show cannot present after this hide. Returns
    /// the `(generation, safety)` pair the renderer acknowledgement echoes,
    /// so the owner can wait for exactly this hide. No acknowledgement is
    /// posted here: only the renderer ack counts as hidden, so a finish-wait
    /// can never be satisfied without renderer action. Cancel, suspend,
    /// finish and reset all route here with their reason logged owner-side;
    /// the ordering guarantee is identical.
    pub fn hide(&self, carrier: RenderCarrier, generation: u64) -> (u64, u64) {
        let safety = self.safety_epoch.fetch_add(1, Ordering::SeqCst) + 1;
        match carrier {
            RenderCarrier::Border => {
                self.border_hide_mark.store(safety, Ordering::SeqCst);
                self.border_hide_gen.store(generation, Ordering::SeqCst);
                *self.border_desired.lock().expect("border desired lock") = None;
            }
            RenderCarrier::Underlay => {
                self.underlay_hide_mark.store(safety, Ordering::SeqCst);
                self.underlay_hide_gen.store(generation, Ordering::SeqCst);
                *self.underlay_desired.lock().expect("underlay desired lock") = None;
            }
            RenderCarrier::Preview => {
                self.preview_hide_mark.store(safety, Ordering::SeqCst);
                self.preview_hide_gen.store(generation, Ordering::SeqCst);
                *self.preview_desired.lock().expect("preview desired lock") = None;
            }
        }
        (generation, safety)
    }

    /// Take the pending desired frame for one carrier, if any. Frames whose
    /// safety predates the hide watermark are dropped here (stale-show
    /// refusal at the transport edge, before any present attempt).
    pub fn take_desired(&self, carrier: RenderCarrier) -> Option<DesiredFrame> {
        self.take_desired_stamped(carrier).map(|(frame, _)| frame)
    }

    /// Take the pending desired frame with its owner post stamp, if any.
    /// Same stale-show drop as [`RenderChannels::take_desired`]; the stamp
    /// lets the renderer report transport queue age honestly.
    pub fn take_desired_stamped(&self, carrier: RenderCarrier) -> Option<(DesiredFrame, Instant)> {
        let mark = self.hide_mark(carrier);
        match carrier {
            RenderCarrier::Border => {
                let (frame, at) = self
                    .border_desired
                    .lock()
                    .expect("border desired lock")
                    .take()?;
                if frame.safety < mark {
                    return None;
                }
                Some((DesiredFrame::Border(frame), at))
            }
            RenderCarrier::Underlay => {
                let (frame, at) = self
                    .underlay_desired
                    .lock()
                    .expect("underlay desired lock")
                    .take()?;
                if frame.safety < mark {
                    return None;
                }
                Some((DesiredFrame::Underlay(frame), at))
            }
            RenderCarrier::Preview => {
                let (frame, at) = self
                    .preview_desired
                    .lock()
                    .expect("preview desired lock")
                    .take()?;
                if frame.safety < mark {
                    return None;
                }
                Some((DesiredFrame::Preview(frame), at))
            }
        }
    }

    /// Post one acknowledgement, replacing the previous entry (the owner
    /// keeps its own applied mirror; only the newest ack matters). Every
    /// post notifies the ack signal so a finish-wait wakes immediately.
    pub fn post_ack(&self, carrier: RenderCarrier, ack: CarrierAck) {
        match carrier {
            RenderCarrier::Border => {
                *self.border_ack.lock().expect("border ack lock") = Some(ack);
            }
            RenderCarrier::Underlay => {
                *self.underlay_ack.lock().expect("underlay ack lock") = Some(ack);
            }
            RenderCarrier::Preview => {
                *self.preview_ack.lock().expect("preview ack lock") = Some(ack);
            }
        }
        *self.ack_guard.lock().expect("ack guard lock") += 1;
        self.ack_signal.notify_all();
    }

    /// Take the latest acknowledgement for one carrier, if any.
    pub fn take_ack(&self, carrier: RenderCarrier) -> Option<CarrierAck> {
        match carrier {
            RenderCarrier::Border => self.border_ack.lock().expect("border ack lock").take(),
            RenderCarrier::Underlay => self.underlay_ack.lock().expect("underlay ack lock").take(),
            RenderCarrier::Preview => self.preview_ack.lock().expect("preview ack lock").take(),
        }
    }

    /// Event-driven wait for a *proven hidden* acknowledgement at or past
    /// `(generation, safety)` on one carrier. Returns true only when an ack
    /// with a verified `Hidden` outcome (`hidden_proven`, i.e. a creator-side
    /// hidden readback) arrived before the bounded timeout; a `Shown`,
    /// `Moved`, `Redrew`, `Unchanged`, `Refused`, `Unknown`, or unverified
    /// `Hidden` ack at the same pair never satisfies a hide wait, and a
    /// timeout claims nothing. Never polls: the renderer notifies the signal
    /// on every ack post. Must run outside callbacks (owner loop context).
    pub fn wait_ack(
        self: &Arc<Self>,
        carrier: RenderCarrier,
        generation: u64,
        safety: u64,
        timeout: Duration,
    ) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        let mut guard = self.ack_guard.lock().expect("ack guard lock");
        loop {
            if Self::hide_proven(self, carrier, generation, safety) {
                return true;
            }
            let now = std::time::Instant::now();
            if now >= deadline {
                return false;
            }
            let remaining = deadline - now;
            let (next, result) = self
                .ack_signal
                .wait_timeout(guard, remaining)
                .expect("ack wait");
            guard = next;
            if result.timed_out() {
                // Re-check once under the lock before reporting: a notify
                // racing the timeout still satisfies the waiter.
                return Self::hide_proven(self, carrier, generation, safety);
            }
        }
    }

    /// True when the latest ack for `carrier` proves the requested hide:
    /// generation and safety at or past the requested pair, a `Hidden`
    /// outcome, *and* the creator-side verified-hidden flag. An unverified
    /// `Hidden` (hide attempted but the readback failed) proves nothing, so
    /// a finish wait stays unsatisfied until a later verified ack lands.
    fn hide_proven(&self, carrier: RenderCarrier, generation: u64, safety: u64) -> bool {
        let satisfied = match carrier {
            RenderCarrier::Border => self.border_ack.lock().expect("border ack lock"),
            RenderCarrier::Underlay => self.underlay_ack.lock().expect("underlay ack lock"),
            RenderCarrier::Preview => self.preview_ack.lock().expect("preview ack lock"),
        };
        satisfied.as_ref().is_some_and(|ack| {
            ack.generation >= generation
                && ack.safety >= safety
                && matches!(ack.outcome, RenderOutcome::Hidden)
                && ack.hidden_proven
        })
    }

    /// Publish the creator-side carrier snapshot atomically: every `Some`
    /// identity fills only an empty slot (first-wins per carrier under one
    /// lock), so a later recycled HWND value can never overwrite the live
    /// identity. Called once with all three hidden carriers before readiness
    /// so owner observation and WinEvent exclusion are armed before the
    /// first show; later self-heal replacements ride
    /// [`RenderChannels::replace_overlay_id`], never this path.
    pub fn publish_overlay_snapshot(&self, ids: [Option<u64>; 3]) {
        let mut current = self.overlay_ids.lock().expect("overlay ids lock");
        for (slot, id) in current.iter_mut().zip(ids) {
            if slot.is_none() {
                *slot = id;
            }
        }
        if current.iter().any(Option::is_some) {
            self.overlay_version.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Replace one published identity after a creator-side self-heal, only
    /// when the currently published value still equals `old`. A concurrent
    /// newer snapshot never loses to a stale replacement; a mismatch leaves
    /// the live exclusion untouched and reports false.
    pub fn replace_overlay_id(&self, carrier: RenderCarrier, old: u64, new: u64) -> bool {
        let mut current = self.overlay_ids.lock().expect("overlay ids lock");
        if current[carrier.index()] == Some(old) {
            current[carrier.index()] = Some(new);
            self.overlay_version.fetch_add(1, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    /// Monotonic overlay-identity version: bumped on every snapshot publish
    /// or replacement so the owner can notice a self-heal without polling
    /// identities per tick.
    #[must_use]
    pub fn overlay_version(&self) -> u64 {
        self.overlay_version.load(Ordering::SeqCst)
    }

    /// Currently published overlay identities, for owner observation and
    /// WinEvent exclusion.
    #[must_use]
    pub fn overlay_ids(&self) -> [Option<u64>; 3] {
        *self.overlay_ids.lock().expect("overlay ids lock")
    }

    /// Monotonic ack sequence: bumped on every ack post. The Windows
    /// renderer endpoint watches it to signal the ack wake only on real
    /// ack traffic, never as a busy loop.
    #[must_use]
    pub fn ack_seq(&self) -> u64 {
        *self.ack_guard.lock().expect("ack guard lock")
    }

    /// Clear every published identity (renderer stop): a destroyed HWND
    /// value may be recycled by the OS, so stale exclusions must not linger.
    pub fn clear_overlay_ids(&self) {
        *self.overlay_ids.lock().expect("overlay ids lock") = [None, None, None];
    }

    /// Request graceful stop (owner side).
    pub fn request_stop(&self) {
        self.stop_flag.store(1, Ordering::SeqCst);
    }

    /// True once the owner requested stop (renderer side).
    #[must_use]
    pub fn stop_requested(&self) -> bool {
        self.stop_flag.load(Ordering::SeqCst) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn lifetime_for(hwnd: u64) -> WindowLifetime {
        WindowLifetime {
            pid: 1000 + hwnd as u32,
            creation: format!("creation-{hwnd}"),
            member_tag: Some(format!("tag-{hwnd}")),
        }
    }

    fn border_frame(generation: u64, safety: u64, x: i32) -> DesiredFrame {
        DesiredFrame::Border(BorderDesired {
            generation,
            safety,
            outer: Rect {
                x,
                y: 0,
                w: 100,
                h: 80,
            },
            style_px: (3, 0, 6),
            color: (0x2a, 0x82, 0xda),
            use_theme: false,
            target_token: format!("token-{generation}"),
            target_hwnd: 1000 + generation,
            lifetime: lifetime_for(1000 + generation),
            foreground: lifetime_for(1000 + generation),
            fulls: vec![Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            }],
            fullscreen_bypass: false,
            topmost: false,
        })
    }

    fn ack_for(generation: u64, safety: u64) -> CarrierAck {
        CarrierAck {
            generation,
            safety,
            outcome: RenderOutcome::Hidden,
            checksum: 0,
            span_us: 7,
            queue_age_us: 0,
            last_error: None,
            hidden_proven: true,
            identity_proven: true,
            applied: None,
        }
    }

    fn live_facts() -> FreshFacts {
        FreshFacts {
            alive: true,
            identity_ok: true,
            visible: true,
            zoom_ok: true,
            fullscreen: false,
            foreground_ok: true,
            band_ok: true,
        }
    }

    #[test]
    fn latest_wins_replaces_pending_frame() {
        let channels = RenderChannels::new();
        channels.post(border_frame(1, 0, 10));
        channels.post(border_frame(2, 0, 20));
        let taken = channels
            .take_desired(RenderCarrier::Border)
            .expect("frame pending");
        assert_eq!(taken.generation(), 2);
        assert!(channels.take_desired(RenderCarrier::Border).is_none());
    }

    #[test]
    fn hide_invalidates_pending_show() {
        let channels = RenderChannels::new();
        channels.post(border_frame(1, 0, 10));
        let (generation, safety) = channels.hide(RenderCarrier::Border, 2);
        assert!(safety > 0);
        assert_eq!(channels.hide_gen(RenderCarrier::Border), generation);
        // The queued pre-hide Show is dropped at the transport edge.
        assert!(channels.take_desired(RenderCarrier::Border).is_none());
        // No acknowledgement is fabricated: only the renderer ack counts,
        // so a finish-wait can never complete without renderer action.
        assert!(channels.take_ack(RenderCarrier::Border).is_none());
    }

    #[test]
    fn show_after_hide_presents_with_newer_safety() {
        let channels = RenderChannels::new();
        channels.post(border_frame(1, 0, 10));
        channels.hide(RenderCarrier::Border, 2);
        let safety = channels.safety();
        channels.post(border_frame(3, safety, 30));
        let taken = channels
            .take_desired(RenderCarrier::Border)
            .expect("fresh show passes");
        assert_eq!(taken.generation(), 3);
    }

    #[test]
    fn hide_all_carriers_bump_shared_epoch() {
        let channels = RenderChannels::new();
        for carrier in RenderCarrier::all() {
            channels.hide(carrier, 1);
        }
        assert_eq!(channels.safety(), 3);
        for carrier in RenderCarrier::all() {
            assert!(channels.hide_mark(carrier) > 0);
        }
    }

    #[test]
    fn wait_ack_returns_on_renderer_notify() {
        let channels = Arc::new(RenderChannels::new());
        let waiter = Arc::clone(&channels);
        let done = std::thread::spawn(move || {
            waiter.wait_ack(RenderCarrier::Preview, 9, 4, Duration::from_secs(5))
        });
        std::thread::sleep(Duration::from_millis(50));
        channels.post_ack(RenderCarrier::Preview, ack_for(9, 4));
        assert!(done.join().expect("waiter joins"));
    }

    #[test]
    fn wait_ack_proves_hidden_not_any_ack() {
        // A present ack at the requested pair never satisfies a hide wait:
        // only a proven Hidden outcome proves hidden.
        let channels = Arc::new(RenderChannels::new());
        channels.post_ack(
            RenderCarrier::Preview,
            CarrierAck {
                generation: 9,
                safety: 4,
                outcome: RenderOutcome::Shown,
                checksum: 0x1234,
                span_us: 7,
                queue_age_us: 0,
                last_error: None,
                hidden_proven: false,
                identity_proven: true,
                applied: None,
            },
        );
        assert!(!channels.wait_ack(RenderCarrier::Preview, 9, 4, Duration::from_millis(50)));
        channels.post_ack(
            RenderCarrier::Preview,
            CarrierAck {
                generation: 9,
                safety: 4,
                outcome: RenderOutcome::Refused("mover-gone"),
                checksum: 0,
                span_us: 7,
                queue_age_us: 0,
                last_error: None,
                hidden_proven: false,
                identity_proven: true,
                applied: None,
            },
        );
        assert!(!channels.wait_ack(RenderCarrier::Preview, 9, 4, Duration::from_millis(50)));
        // An unverified Hidden (hide attempted, readback failed) still
        // proves nothing: the waiter stays unsatisfied until proven.
        channels.post_ack(
            RenderCarrier::Preview,
            CarrierAck {
                generation: 9,
                safety: 4,
                outcome: RenderOutcome::Hidden,
                checksum: 0,
                span_us: 7,
                queue_age_us: 0,
                last_error: Some("error: hide unverified".to_owned()),
                hidden_proven: false,
                identity_proven: true,
                applied: None,
            },
        );
        assert!(!channels.wait_ack(RenderCarrier::Preview, 9, 4, Duration::from_millis(50)));
        channels.post_ack(RenderCarrier::Preview, ack_for(9, 4));
        assert!(channels.wait_ack(RenderCarrier::Preview, 9, 4, Duration::from_millis(50)));
    }

    #[test]
    fn wait_ack_times_out_truthfully_without_ack() {
        let channels = Arc::new(RenderChannels::new());
        assert!(!channels.wait_ack(RenderCarrier::Preview, 9, 4, Duration::from_millis(50)));
    }

    #[test]
    fn wait_ack_ignores_older_generation() {
        let channels = Arc::new(RenderChannels::new());
        channels.post_ack(RenderCarrier::Border, ack_for(3, 3));
        assert!(!channels.wait_ack(RenderCarrier::Border, 9, 3, Duration::from_millis(50)));
    }

    #[test]
    fn overlay_snapshot_publishes_atomically_first_wins() {
        let channels = RenderChannels::new();
        // Creator-side snapshot before readiness arms every exclusion at once.
        channels.publish_overlay_snapshot([Some(111), Some(222), None]);
        assert_eq!(channels.overlay_ids(), [Some(111), Some(222), None]);
        let version = channels.overlay_version();
        assert!(version > 0);
        // A later recycled HWND value never overwrites the live identity.
        channels.publish_overlay_snapshot([Some(999), None, Some(333)]);
        assert_eq!(channels.overlay_ids(), [Some(111), Some(222), Some(333)]);
        // Self-heal replacement swaps only against the expected old value.
        assert!(channels.replace_overlay_id(RenderCarrier::Border, 111, 444));
        assert_eq!(channels.overlay_ids()[0], Some(444));
        assert!(!channels.replace_overlay_id(RenderCarrier::Border, 111, 555));
        assert_eq!(channels.overlay_ids()[0], Some(444));
        assert!(channels.overlay_version() > version);
        channels.clear_overlay_ids();
        assert_eq!(channels.overlay_ids(), [None, None, None]);
    }

    #[test]
    fn freshness_gate_refuses_stale_targets_narrowly() {
        let live = live_facts();
        assert!(freshness_gate(RenderCarrier::Border, &live).is_ok());
        assert_eq!(
            freshness_gate(
                RenderCarrier::Border,
                &FreshFacts {
                    alive: false,
                    ..live
                }
            ),
            Err("target-gone")
        );
        assert_eq!(
            freshness_gate(
                RenderCarrier::Underlay,
                &FreshFacts {
                    alive: false,
                    ..live
                }
            ),
            Err("anchor-missing")
        );
        assert_eq!(
            freshness_gate(
                RenderCarrier::Preview,
                &FreshFacts {
                    alive: false,
                    ..live
                }
            ),
            Err("mover-gone")
        );
        // Recycled HWND values fail the lifetime fence with narrow reasons.
        assert_eq!(
            freshness_gate(
                RenderCarrier::Border,
                &FreshFacts {
                    identity_ok: false,
                    ..live
                }
            ),
            Err("identity-changed")
        );
        assert_eq!(
            freshness_gate(
                RenderCarrier::Underlay,
                &FreshFacts {
                    identity_ok: false,
                    ..live
                }
            ),
            Err("anchor-identity-changed")
        );
        assert_eq!(
            freshness_gate(
                RenderCarrier::Preview,
                &FreshFacts {
                    identity_ok: false,
                    ..live
                }
            ),
            Err("mover-identity-changed")
        );
        // Maximized suppression refuses the below-plane carriers only.
        assert_eq!(
            freshness_gate(
                RenderCarrier::Border,
                &FreshFacts {
                    zoom_ok: false,
                    ..live
                }
            ),
            Err("target-maximized")
        );
        assert!(
            freshness_gate(
                RenderCarrier::Preview,
                &FreshFacts {
                    zoom_ok: false,
                    ..live
                }
            )
            .is_ok()
        );
        assert_eq!(
            freshness_gate(
                RenderCarrier::Border,
                &FreshFacts {
                    foreground_ok: false,
                    ..live
                }
            ),
            Err("foreground-changed")
        );
        assert_eq!(
            freshness_gate(
                RenderCarrier::Underlay,
                &FreshFacts {
                    band_ok: false,
                    ..live
                }
            ),
            Err("band-changed")
        );
        // A foreground drift refuses every carrier: the renderer compares
        // the live foreground lifetime identity on all three planes, so a
        // late switch (game, suspend) hides instead of presenting.
        assert_eq!(
            freshness_gate(
                RenderCarrier::Underlay,
                &FreshFacts {
                    foreground_ok: false,
                    ..live
                }
            ),
            Err("anchor-foreground-changed")
        );
    }

    #[test]
    fn preview_gate_ignores_mover_band() {
        // An ordinary (non-topmost) mover presents: the preview plane is
        // always topmost by construction, so the mover band is never a
        // requirement. Only mover liveness plus the lifetime identity fence the present.
        let ordinary_mover = FreshFacts {
            band_ok: false,
            ..live_facts()
        };
        assert!(freshness_gate(RenderCarrier::Preview, &ordinary_mover).is_ok());
    }

    #[test]
    fn gate_refuses_fullscreen_and_foreground_drift_everywhere() {
        // Captionless fullscreen refuses the below-plane carriers with
        // narrow reasons; the preview plane is exempt.
        assert_eq!(
            freshness_gate(
                RenderCarrier::Border,
                &FreshFacts {
                    fullscreen: true,
                    ..live_facts()
                }
            ),
            Err("target-fullscreen")
        );
        assert_eq!(
            freshness_gate(
                RenderCarrier::Underlay,
                &FreshFacts {
                    fullscreen: true,
                    ..live_facts()
                }
            ),
            Err("anchor-fullscreen")
        );
        assert!(
            freshness_gate(
                RenderCarrier::Preview,
                &FreshFacts {
                    fullscreen: true,
                    ..live_facts()
                }
            )
            .is_ok()
        );
        // A late foreground switch refuses every carrier, including the
        // preview and underlay: only an unchanged foreground presents.
        assert_eq!(
            freshness_gate(
                RenderCarrier::Underlay,
                &FreshFacts {
                    foreground_ok: false,
                    ..live_facts()
                }
            ),
            Err("anchor-foreground-changed")
        );
        assert_eq!(
            freshness_gate(
                RenderCarrier::Preview,
                &FreshFacts {
                    foreground_ok: false,
                    ..live_facts()
                }
            ),
            Err("mover-foreground-changed")
        );
    }

    #[test]
    fn applied_content_bounds_target() {
        let content = AppliedContent::new(
            &"t".repeat(200),
            [1, 2, 3, 4],
            "#abcdef".to_owned(),
            Some([3, 0, 6]),
            false,
            2,
        );
        assert_eq!(content.target.len(), 64);
        assert_eq!(content.outer, [1, 2, 3, 4]);
    }

    #[test]
    fn lifecycle_refuses_overlap_and_stuck_stop() {
        let _serial = render_slot_test_serial();
        assert!(render_slot_try_reserve());
        // Overlapping replacement refuses while the slot is held.
        assert!(!render_slot_try_reserve());
        let mut lifecycle = RenderLifecycle::new();
        assert_eq!(lifecycle.state(), RenderState::Stopped);
        assert!(lifecycle.start().is_ok());
        assert!(lifecycle.start().is_err());
        assert!(lifecycle.note_ready().is_ok());
        assert!(lifecycle.request_stop().is_ok());
        // Failed destroy retains Stopping: no clean claim, no slot release.
        let report = lifecycle.note_destroyed(false).expect("stopping report");
        assert!(!report.clean);
        assert_eq!(lifecycle.state(), RenderState::Stopping);
        assert!(render_slot_reserved());
        // A later successful destroy completes the stop and frees the slot.
        let report = lifecycle.note_destroyed(true).expect("stopping report");
        assert!(report.clean && report.destroyed_ok);
        assert_eq!(lifecycle.state(), RenderState::Stopped);
        render_slot_release();
        assert!(!render_slot_reserved());
    }

    #[test]
    fn render_error_truncates_bounded() {
        let long = "x".repeat(300);
        assert_eq!(truncate_render_error(&long).len(), 163);
        assert_eq!(truncate_render_error("short"), "short");
    }
}
