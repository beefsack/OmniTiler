//! Dedicated render thread (stage 2, unit: option B renderer).
//!
//! The renderer creates, pumps, presents and destroys the active border,
//! group underlay and drop-preview carriers on its own thread. The owner
//! keeps observation, Engine, foreign actuation, recovery and all logging.
//!
//! Transport contract (from [`crate::render_handoff`]):
//! - Owner posts replaceable desired frames (latest-wins) plus safety
//!   epochs; the renderer rechecks fresh native facts against the desired
//!   epoch and refuses stale or unreadable targets narrowly.
//! - The renderer publishes immutable overlay identities for owner
//!   observation/WinEvent exclusion and acknowledges applied generations
//!   with outcomes, owned-pixel checksums and bounded failures. Owner logs
//!   consume acks, never desired state.
//! - Preview finish hides with an acknowledgement the owner waits for
//!   outside callbacks with a bounded timeout; a missing ack is reported
//!   truthfully while the bumped safety epoch alone prevents stale-show
//!   replay.
//! - Readiness and creator-side destroy carry bounded failure reporting. A
//!   stuck renderer keeps the process render slot so no overlapping
//!   replacement can start; recovery is process exit.
//!
//! The portable [`RenderCore`] drives a [`Present`] seam: deterministic
//! [`FakePresent`] fixtures cover races without native calls, while the
//! `cfg(windows)` native backend owns the real carriers on the renderer
//! thread. Pixels, follow and z-order are unchanged: the native backend
//! calls the same carrier methods as the same-thread path, so raster
//! checksums stay comparable.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::render_handoff::{
    AppliedContent, BorderDesired, CarrierAck, DesiredFrame, FreshFacts, PreviewDesired,
    RenderCarrier, RenderChannels, RenderOutcome, UnderlayDesired, WindowLifetime, freshness_gate,
    queue_age_us_since, truncate_render_error,
};

/// How long the owner waits for renderer readiness before reporting failure
/// without claiming readiness.
pub const RENDER_READY_WAIT: Duration = Duration::from_secs(2);
/// Bounded stop acknowledgement wait: a missing ack returns failure and
/// retains `Stopping` (never claims clean, never starts an overlapping
/// worker).
pub const RENDER_STOP_WAIT: Duration = Duration::from_secs(5);
/// Bounded preview-finish hide acknowledgement wait: a missing ack is logged
/// truthfully and the owner proceeds, with the bumped safety epoch alone
/// preventing any stale-show replay.
pub const FINISH_WAIT: Duration = Duration::from_millis(500);

/// Native present outcome for one carrier: what actually happened plus the
/// owned-pixel checksum of what was presented (0 when hidden). `resolved`
/// is the actual presented colour (accent-resolved for the border): the ack
/// logs this, never the owner fallback.
pub struct PresentOutcome {
    pub outcome: RenderOutcome,
    pub checksum: u64,
    pub error: Option<String>,
    pub resolved: Option<(u8, u8, u8)>,
}

impl PresentOutcome {
    /// Successful present with a checksum and the actual presented colour.
    #[must_use]
    pub fn shown(outcome: RenderOutcome, checksum: u64, resolved: (u8, u8, u8)) -> Self {
        Self {
            outcome,
            checksum,
            error: None,
            resolved: Some(resolved),
        }
    }

    /// Narrow failure: the carrier hid instead, with bounded detail.
    #[must_use]
    pub fn failed(detail: String) -> Self {
        Self {
            outcome: RenderOutcome::Hidden,
            checksum: 0,
            error: Some(truncate_render_error(&detail)),
            resolved: None,
        }
    }
}

/// Freshness query for one present: the band to follow plus the identities
/// and suppression inputs the renderer rechecks itself.
pub struct FreshQuery<'a> {
    /// Band to follow (border target band; underlay resolved-anchor band).
    pub topmost: bool,
    /// Lifetime the target/anchor window must still match.
    pub expected: &'a WindowLifetime,
    /// Foreground identity that must be unchanged since post time (`None`
    /// means the post-time foreground read was unreadable and refuses
    /// narrowly per the binding contract; used solely for preview frames).
    pub foreground: Option<&'a WindowLifetime>,
    /// Monitor full rects for the captionless-fullscreen predicate.
    pub fulls: &'a [tiler_core::geometry::Rect],
    /// Explicit owner proof behind the managed-fullscreen bypass.
    pub fullscreen_bypass: bool,
}

/// Renderer-resolved underlay anchor with its correctly paired identity: a
/// foreign member carries its own lifetime for individual validation, while
/// the renderer's own border surface carries no foreign lifetime (it
/// validates as renderer-owned: visible, same-group, same-band).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedAnchor {
    Foreign { hwnd: u64, lifetime: WindowLifetime },
    OwnBorder { hwnd: u64 },
}

impl ResolvedAnchor {
    /// Anchor window handle either way.
    #[must_use]
    pub const fn hwnd(&self) -> u64 {
        match self {
            Self::Foreign { hwnd, .. } | Self::OwnBorder { hwnd } => *hwnd,
        }
    }
}

/// Renderer-side present seam. Implementations run entirely on the renderer
/// thread: native reads and owned-window writes only, never owner
/// observation, Engine, foreign writes, or file logging.
pub trait Present {
    /// Fresh native facts for one target/anchor HWND per `query`: liveness,
    /// lifetime identity match (pid/creation/tag against `expected`),
    /// visibility (folding non-iconic and DWM-uncloaked), maximized and
    /// captionless-fullscreen suppression, foreground-identity match, and
    /// band match. Read-only, renderer-local; unreadable reads fail closed.
    fn check_fresh(&self, carrier: RenderCarrier, hwnd: u64, query: &FreshQuery<'_>) -> FreshFacts;
    /// Fresh facts for the renderer's own border surface as an underlay
    /// anchor: renderer-owned validation (visible, same-band) plus the
    /// unchanged-foreground fence. Never a foreign member tag.
    fn check_own_anchor(
        &self,
        hwnd: u64,
        topmost: bool,
        foreground: Option<&WindowLifetime>,
    ) -> FreshFacts;
    /// Fresh lowest-in-z anchor among the individually validated live
    /// candidates (`None` when no candidate validates or the order is
    /// unreadable). The native backend additionally sorts its own border
    /// surface into the comparison only when that surface is visible,
    /// same-group, and same-band.
    fn resolve_anchor(&self, frame: &UnderlayDesired) -> Option<ResolvedAnchor>;
    /// Read the current `WS_EX_TOPMOST` band of one HWND (`None` when
    /// unreadable). Lets the core follow the resolved anchor's band
    /// like-for-like with the same-thread path.
    fn read_band(&self, hwnd: u64) -> Option<bool>;
    /// Currently owned carrier window, if created.
    fn carrier_hwnd(&self, carrier: RenderCarrier) -> Option<u64>;
    /// Whether the owned carrier surface currently reads visible.
    fn carrier_visible(&self, carrier: RenderCarrier) -> bool;
    /// Opaque target/anchor token the owned carrier currently shows, if any.
    /// Lets the core admit the own border as an anchor only for its own
    /// group.
    fn carrier_token(&self, carrier: RenderCarrier) -> Option<String>;
    /// Creator-side ensure: create every missing carrier window hidden.
    /// Fails closed with bounded detail; the core refuses the pending work
    /// narrowly until the next retry.
    fn ensure_created(&mut self) -> Result<(), String>;
    /// Present one border frame for an already-gated target.
    fn show_border(&mut self, frame: &BorderDesired) -> PresentOutcome;
    /// Present one underlay frame beneath the already-resolved anchor, in
    /// the resolved anchor's band (like-for-like with the same-thread path,
    /// which reads the band from the resolved anchor).
    fn show_underlay(
        &mut self,
        frame: &UnderlayDesired,
        anchor: u64,
        topmost: bool,
    ) -> PresentOutcome;
    /// Present one preview frame for an already-gated mover.
    fn show_preview(&mut self, frame: &PreviewDesired) -> PresentOutcome;
    /// Hide one carrier with a creator-side verified hidden readback
    /// (no-op when already hidden). True only when the surface reads hidden
    /// afterwards; a failed hide keeps its state for a bounded retry.
    fn hide(&mut self, carrier: RenderCarrier) -> bool;
    /// Destroy every carrier on the creator thread. True when every carrier
    /// reports destroyed.
    fn destroy_all(&mut self) -> bool;
}

/// Portable renderer core: drains latest-wins frames, honors safety
/// watermarks, gates on fresh facts, presents, and acknowledges applied
/// state. Single-threaded by construction (renderer thread only).
pub struct RenderCore<P> {
    present: P,
    channels: Arc<RenderChannels>,
    acked_safety: [u64; 3],
    /// Next retry time for a retained unverified hide per carrier (`None`
    /// means an immediate attempt is due). A hide stays pending until a
    /// creator-side readback verifies it; the watermark never advances on
    /// an unverified hide, so the hide is retried with bounded backoff
    /// instead of being lost behind a transition-only owner post.
    hide_due: [Option<Instant>; 3],
    /// Consecutive unverified-hide attempts per carrier (backoff input).
    hide_attempts: [u32; 3],
}

/// Bounded hide-retry backoff: 50ms doubling, 1s cap. Queued Shows stay
/// dropped behind the watermark while the hide is pending, so retries never
/// resurrect a cancelled carrier.
fn hide_backoff(attempts: u32) -> Duration {
    Duration::from_millis((50u64 << attempts.min(4)).min(1000))
}

/// True when the desired lifetime carries a membership tag the renderer
/// compared: tag-proven fence strength. Untagged targets stay pid/creation
/// fenced with the weakness reported honestly in the ack.
fn tag_proven(lifetime: &WindowLifetime) -> bool {
    lifetime.member_tag.is_some()
}

impl<P: Present> RenderCore<P> {
    /// New core over shared channels with no applied state.
    pub fn new(present: P, channels: Arc<RenderChannels>) -> Self {
        Self {
            present,
            channels,
            acked_safety: [0, 0, 0],
            hide_due: [None, None, None],
            hide_attempts: [0, 0, 0],
        }
    }

    /// Borrow the present backend (test inspection).
    #[must_use]
    pub fn present(&self) -> &P {
        &self.present
    }

    /// Borrow the present backend mutably (test scripting).
    pub fn present_mut(&mut self) -> &mut P {
        &mut self.present
    }

    /// One renderer quantum: every carrier, hide-watermark first (so a
    /// hide-then-show in the same quantum still presents the fresh show),
    /// then at most one pending frame each.
    pub fn step(&mut self) {
        for carrier in RenderCarrier::all() {
            self.step_carrier(carrier);
        }
    }

    fn step_carrier(&mut self, carrier: RenderCarrier) {
        let index = carrier.index();
        let started = Instant::now();
        let mark = self.channels.hide_mark(carrier);
        let hide_pending = mark > self.acked_safety[index];
        let stamped = self.channels.take_desired_stamped(carrier);
        let taken_at = Instant::now();
        let (frame, queue_age_us) = match stamped {
            Some((frame, posted)) => {
                let age = queue_age_us_since(posted, taken_at);
                (Some(frame), age)
            }
            None => (None, 0),
        };
        if let Some(frame) = frame.as_ref() {
            debug_assert_eq!(frame.carrier(), carrier);
        }
        // Re-check the watermark after the take: a hide posted while the
        // frame waited (or while a previous carrier presented) invalidates
        // it. The watermark advance itself is acknowledged below, so
        // skipping here never loses the hide.
        let frame = frame.filter(|frame| frame.safety() >= self.channels.hide_mark(carrier));
        if !hide_pending && frame.is_none() {
            return;
        }
        if hide_pending && self.hide_attempt_due(index) {
            self.step_hide(carrier, mark, started);
        }
        let Some(frame) = frame else {
            return;
        };
        // Creator-side ensure before any present: a missing carrier window
        // recreates here (hidden), never lazily inside the show. Failure
        // refuses narrowly with bounded detail; the frame is consumed and
        // the owner reposts on its next refresh.
        if let Err(detail) = self.present.ensure_created() {
            let generation = frame.generation();
            let safety = frame.safety();
            self.acked_safety[index] = self.acked_safety[index].max(safety);
            self.post_ack(
                carrier,
                generation,
                safety,
                RenderOutcome::Refused("carrier-lost"),
                0,
                span_us_since(started),
                queue_age_us,
                Some(detail),
                false,
                false,
                None,
            );
            return;
        }
        // Publish-before-show: reconcile the published snapshot with the
        // live carrier windows before presenting, so a recreated identity
        // is excluded before its first present, never after.
        self.sync_id(carrier);
        let started = Instant::now();
        match &frame {
            DesiredFrame::Border(desired) => {
                self.present_border(carrier, desired, started, queue_age_us)
            }
            DesiredFrame::Underlay(desired) => {
                self.present_underlay(carrier, desired, started, queue_age_us)
            }
            DesiredFrame::Preview(desired) => {
                self.present_preview(carrier, desired, started, queue_age_us)
            }
        }
        self.acked_safety[index] = self.acked_safety[index].max(frame.safety());
    }

    /// True when an unverified-hide retry is due (first attempt always).
    fn hide_attempt_due(&self, index: usize) -> bool {
        self.hide_due[index].is_none_or(|due| Instant::now() >= due)
    }

    /// Next renderer-side timer deadline (hide-retry), if any. `None`
    /// means fully idle: no hide is pending, so the pump may wait
    /// indefinitely. No wake is lost: owner posts, hides, and stop
    /// requests always signal the work wake, and broadcast messages arrive
    /// through the message queue itself. Desired frames never need a
    /// timer: their post already signaled the wake.
    #[must_use]
    pub fn next_timer(&self) -> Option<Instant> {
        let mut earliest: Option<Instant> = None;
        for carrier in RenderCarrier::all() {
            let index = carrier.index();
            if self.channels.hide_mark(carrier) > self.acked_safety[index] {
                match self.hide_due[index] {
                    Some(due) => {
                        earliest = Some(earliest.map_or(due, |prev| prev.min(due)));
                    }
                    // Pending with no backoff scheduled: due immediately.
                    None => return Some(Instant::now()),
                }
            }
        }
        earliest
    }

    /// One hide-watermark attempt: only a surface that reads hidden
    /// afterwards advances the watermark with a proven `Hidden` ack. An
    /// unverified hide retains the pending watermark with bounded backoff
    /// and reports `Unknown`, never a known-false visibility and never a
    /// satisfied hide wait.
    fn step_hide(&mut self, carrier: RenderCarrier, mark: u64, started: Instant) {
        let index = carrier.index();
        if self.present.hide(carrier) {
            self.acked_safety[index] = mark;
            self.hide_due[index] = None;
            self.hide_attempts[index] = 0;
            let generation = self.channels.hide_gen(carrier);
            self.post_ack(
                carrier,
                generation,
                mark,
                RenderOutcome::Hidden,
                0,
                span_us_since(started),
                0,
                None,
                true,
                false,
                None,
            );
        } else {
            let attempts = self.hide_attempts[index];
            self.hide_attempts[index] = attempts.saturating_add(1);
            self.hide_due[index] = Some(Instant::now() + hide_backoff(attempts));
            let generation = self.channels.hide_gen(carrier);
            self.post_ack(
                carrier,
                generation,
                mark,
                RenderOutcome::Unknown("hide-failed"),
                0,
                span_us_since(started),
                0,
                Some("error: renderer hide unverified".to_owned()),
                false,
                false,
                None,
            );
        }
    }

    /// Post one acknowledgement with full provenance.
    #[allow(clippy::too_many_arguments)]
    fn post_ack(
        &self,
        carrier: RenderCarrier,
        generation: u64,
        safety: u64,
        outcome: RenderOutcome,
        checksum: u64,
        span_us: u64,
        queue_age_us: u64,
        last_error: Option<String>,
        hidden_proven: bool,
        identity_proven: bool,
        applied: Option<AppliedContent>,
    ) {
        self.channels.post_ack(
            carrier,
            CarrierAck {
                generation,
                safety,
                outcome,
                checksum,
                span_us,
                queue_age_us,
                last_error,
                hidden_proven,
                identity_proven,
                applied,
            },
        );
    }

    /// Verified-hide helper for refusal and failure paths: attempts the
    /// creator-side hide and reports whether the surface reads hidden.
    /// Callers acknowledge `Refused` only when verified, `Unknown`
    /// otherwise, so no ack ever claims a known-false visibility over a
    /// possibly visible surface.
    fn hide_verified(&mut self, carrier: RenderCarrier) -> bool {
        self.present.hide(carrier)
    }

    /// Present one gated border frame with correctly paired identities.
    fn present_border(
        &mut self,
        carrier: RenderCarrier,
        desired: &BorderDesired,
        started: Instant,
        queue_age_us: u64,
    ) {
        let query = FreshQuery {
            topmost: desired.topmost,
            expected: &desired.lifetime,
            foreground: Some(&desired.foreground),
            fulls: &desired.fulls,
            fullscreen_bypass: desired.fullscreen_bypass,
        };
        let facts = self
            .present
            .check_fresh(carrier, desired.target_hwnd, &query);
        let proven = tag_proven(&desired.lifetime);
        match freshness_gate(carrier, &facts) {
            Err(reason) => self.post_refused(
                carrier,
                desired.generation,
                desired.safety,
                started,
                queue_age_us,
                reason,
                proven,
            ),
            Ok(()) => {
                let presented = self.present.show_border(desired);
                let fallback = crate::active_border::render_color(desired.color);
                let applied = AppliedContent::new(
                    &desired.target_token,
                    [
                        desired.outer.x,
                        desired.outer.y,
                        desired.outer.w,
                        desired.outer.h,
                    ],
                    presented
                        .resolved
                        .map(crate::active_border::render_color)
                        .unwrap_or(fallback),
                    Some([desired.style_px.0, desired.style_px.1, desired.style_px.2]),
                    desired.topmost,
                    0,
                );
                self.post_presented(
                    carrier,
                    desired.generation,
                    desired.safety,
                    started,
                    queue_age_us,
                    presented,
                    proven,
                    applied,
                );
            }
        }
    }

    /// Present one gated underlay frame. The resolved anchor carries its own
    /// correctly paired lifetime: foreign members validate individually,
    /// the renderer-owned border validates without any foreign member tag,
    /// and the unreadable-order fallback uses the owner fallback pair. The
    /// placement band follows the resolved anchor like-for-like with the
    /// same-thread path.
    fn present_underlay(
        &mut self,
        carrier: RenderCarrier,
        desired: &UnderlayDesired,
        started: Instant,
        queue_age_us: u64,
    ) {
        let resolved = self.present.resolve_anchor(desired);
        let Some(resolved) = resolved.or_else(|| self.fallback_anchor(desired)) else {
            self.post_refused(
                carrier,
                desired.generation,
                desired.safety,
                started,
                queue_age_us,
                "anchor-missing",
                tag_proven(&desired.anchor_lifetime),
            );
            return;
        };
        let anchor = resolved.hwnd();
        let band = self.present.read_band(anchor).unwrap_or(desired.topmost);
        let facts = match &resolved {
            ResolvedAnchor::Foreign { lifetime, .. } => {
                let query = FreshQuery {
                    topmost: band,
                    expected: lifetime,
                    foreground: Some(&desired.foreground),
                    fulls: &desired.fulls,
                    fullscreen_bypass: false,
                };
                self.present.check_fresh(carrier, anchor, &query)
            }
            ResolvedAnchor::OwnBorder { .. } => {
                self.present
                    .check_own_anchor(anchor, band, Some(&desired.foreground))
            }
        };
        // The own border is renderer-owned: no foreign tag is compared, so
        // the fence is proven by ownership. Foreign anchors prove by tag.
        let proven = match &resolved {
            ResolvedAnchor::Foreign { lifetime, .. } => tag_proven(lifetime),
            ResolvedAnchor::OwnBorder { .. } => true,
        };
        match freshness_gate(carrier, &facts) {
            Err(reason) => self.post_refused(
                carrier,
                desired.generation,
                desired.safety,
                started,
                queue_age_us,
                reason,
                proven,
            ),
            Ok(()) => {
                let presented = self.present.show_underlay(desired, anchor, band);
                let fallback = crate::group_underlay::render_color_argb((
                    (desired.color.1, desired.color.2, desired.color.3),
                    desired.color.0,
                ));
                let applied = AppliedContent::new(
                    &desired.anchor_token,
                    [
                        desired.outer.x,
                        desired.outer.y,
                        desired.outer.w,
                        desired.outer.h,
                    ],
                    presented
                        .resolved
                        .map(crate::active_border::render_color)
                        .unwrap_or(fallback),
                    None,
                    band,
                    desired.candidates.len(),
                );
                self.post_presented(
                    carrier,
                    desired.generation,
                    desired.safety,
                    started,
                    queue_age_us,
                    presented,
                    proven,
                    applied,
                );
            }
        }
    }

    /// Owner-fallback anchor for unreadable z-order: the owner pair, used
    /// only when its own lifetime still reads live and visible.
    fn fallback_anchor(&self, desired: &UnderlayDesired) -> Option<ResolvedAnchor> {
        let band = self
            .present
            .read_band(desired.anchor_hwnd)
            .unwrap_or(desired.topmost);
        let query = FreshQuery {
            topmost: band,
            expected: &desired.anchor_lifetime,
            foreground: Some(&desired.foreground),
            fulls: &desired.fulls,
            fullscreen_bypass: false,
        };
        let fallback =
            self.present
                .check_fresh(RenderCarrier::Underlay, desired.anchor_hwnd, &query);
        if fallback.alive && fallback.identity_ok && fallback.visible {
            Some(ResolvedAnchor::Foreign {
                hwnd: desired.anchor_hwnd,
                lifetime: desired.anchor_lifetime.clone(),
            })
        } else {
            None
        }
    }

    /// Present one gated preview frame. The mover band is never read (the
    /// preview plane is always topmost); mover liveness, mover lifetime,
    /// and the unchanged-foreground fence gate the present.
    fn present_preview(
        &mut self,
        carrier: RenderCarrier,
        desired: &PreviewDesired,
        started: Instant,
        queue_age_us: u64,
    ) {
        let query = FreshQuery {
            topmost: false,
            expected: &desired.lifetime,
            foreground: desired.foreground.as_ref(),
            fulls: &[],
            fullscreen_bypass: false,
        };
        let facts = self
            .present
            .check_fresh(carrier, desired.anchor_hwnd, &query);
        let proven = tag_proven(&desired.lifetime);
        match freshness_gate(carrier, &facts) {
            Err(reason) => self.post_refused(
                carrier,
                desired.generation,
                desired.safety,
                started,
                queue_age_us,
                reason,
                proven,
            ),
            Ok(()) => {
                let presented = self.present.show_preview(desired);
                let fallback = crate::group_underlay::render_color_argb((
                    (desired.color.1, desired.color.2, desired.color.3),
                    desired.color.0,
                ));
                let applied = AppliedContent::new(
                    &desired.anchor_token,
                    [
                        desired.outer.x,
                        desired.outer.y,
                        desired.outer.w,
                        desired.outer.h,
                    ],
                    presented
                        .resolved
                        .map(crate::active_border::render_color)
                        .unwrap_or(fallback),
                    None,
                    true,
                    0,
                );
                self.post_presented(
                    carrier,
                    desired.generation,
                    desired.safety,
                    started,
                    queue_age_us,
                    presented,
                    proven,
                    applied,
                );
            }
        }
    }

    /// Acknowledge a freshness refusal: `Refused` only behind a verified
    /// hide, `Unknown` otherwise. Neither claims a known visibility over a
    /// possibly visible surface, and neither satisfies a hide wait.
    #[allow(clippy::too_many_arguments)]
    fn post_refused(
        &mut self,
        carrier: RenderCarrier,
        generation: u64,
        safety: u64,
        started: Instant,
        queue_age_us: u64,
        reason: &'static str,
        identity_proven: bool,
    ) {
        if self.hide_verified(carrier) {
            self.post_ack(
                carrier,
                generation,
                safety,
                RenderOutcome::Refused(reason),
                0,
                span_us_since(started),
                queue_age_us,
                None,
                true,
                identity_proven,
                None,
            );
        } else {
            self.post_ack(
                carrier,
                generation,
                safety,
                RenderOutcome::Unknown(reason),
                0,
                span_us_since(started),
                queue_age_us,
                Some("error: renderer hide unverified".to_owned()),
                false,
                identity_proven,
                None,
            );
        }
    }

    /// Acknowledge a present result: show-family outcomes carry the fresh
    /// applied content (never a stale note); an internally failed present
    /// proves native hidden when verified, else reports unknown.
    #[allow(clippy::too_many_arguments)]
    fn post_presented(
        &mut self,
        carrier: RenderCarrier,
        generation: u64,
        safety: u64,
        started: Instant,
        queue_age_us: u64,
        presented: PresentOutcome,
        identity_proven: bool,
        applied: AppliedContent,
    ) {
        if matches!(presented.outcome, RenderOutcome::Hidden) {
            if self.hide_verified(carrier) {
                self.post_ack(
                    carrier,
                    generation,
                    safety,
                    RenderOutcome::Hidden,
                    0,
                    span_us_since(started),
                    queue_age_us,
                    presented.error,
                    true,
                    identity_proven,
                    None,
                );
            } else {
                self.post_ack(
                    carrier,
                    generation,
                    safety,
                    RenderOutcome::Unknown("present-failed"),
                    0,
                    span_us_since(started),
                    queue_age_us,
                    presented.error,
                    false,
                    identity_proven,
                    None,
                );
            }
            return;
        }
        self.post_ack(
            carrier,
            generation,
            safety,
            presented.outcome,
            presented.checksum,
            span_us_since(started),
            queue_age_us,
            presented.error,
            false,
            identity_proven,
            Some(applied),
        );
    }

    /// Publish the creator-side carrier snapshot once (all three hidden
    /// carriers, before readiness): owner exclusion is armed before the
    /// first show. First-wins per carrier, so recycled values can never
    /// overwrite the live snapshot here.
    pub fn publish_all(&self) {
        self.channels.publish_overlay_snapshot([
            self.present.carrier_hwnd(RenderCarrier::Border),
            self.present.carrier_hwnd(RenderCarrier::Underlay),
            self.present.carrier_hwnd(RenderCarrier::Preview),
        ]);
    }

    /// Reconcile one published identity with the live carrier window after a
    /// creator-side self-heal: swaps only against the expected old value, so
    /// a stale replacement never wins over a newer snapshot.
    fn sync_id(&self, carrier: RenderCarrier) {
        let live = self.present.carrier_hwnd(carrier);
        let published = self.channels.overlay_ids()[carrier.index()];
        match (published, live) {
            (Some(old), Some(new)) if old != new => {
                self.channels.replace_overlay_id(carrier, old, new);
            }
            (None, Some(new)) => {
                self.channels.publish_overlay_snapshot(match carrier {
                    RenderCarrier::Border => [Some(new), None, None],
                    RenderCarrier::Underlay => [None, Some(new), None],
                    RenderCarrier::Preview => [None, None, Some(new)],
                });
            }
            _ => {}
        }
    }
}

/// Elapsed microseconds since `started`, saturating.
fn span_us_since(started: Instant) -> u64 {
    started.elapsed().as_micros().try_into().unwrap_or(u64::MAX)
}

/// Deterministic scripted backend for renderer-core tests. No threads, no
/// native calls: freshness facts, anchor resolutions, failures and carrier
/// identities are all scripted, while checksums run the real portable
/// painters so raster equivalence is covered, not stubbed.
#[derive(Debug, Default)]
pub struct FakePresent {
    /// Scripted per-HWND freshness (`None` entry falls back to `live_facts`).
    pub fresh: std::collections::HashMap<u64, FreshFacts>,
    /// Default facts for unscripted HWNDs (live by default).
    pub live_facts: Option<FreshFacts>,
    /// Enforced foreground lifetime (`None` = use scripted facts): when
    /// set, `foreground_ok` compares the query foreground against this
    /// instead of trusting the scripted entry, so drift tests script a
    /// foreground switch rather than a fact flip.
    pub foreground: Option<WindowLifetime>,
    /// HWNDs reading captionless-fullscreen (OR-ed into scripted facts for
    /// non-preview carriers).
    pub fullscreen_hwnds: std::collections::HashSet<u64>,
    /// Scripted live accent: themed border shows resolve to this instead of
    /// the fallback, so ack colour evidence covers the renderer-owned
    /// accent path with exact painter bytes.
    pub accent: Option<(u8, u8, u8)>,
    /// Scripted per-HWND band reads (`None` entry reads non-topmost).
    pub bands: std::collections::HashMap<u64, bool>,
    /// Candidate frames offered to `resolve_anchor`, in order (evidence).
    pub resolved_with: Vec<UnderlayDesired>,
    /// Shown frames, in order (evidence).
    pub shown: Vec<DesiredFrame>,
    /// Resolved anchors used for underlay shows, in order (evidence).
    pub shown_anchors: Vec<u64>,
    /// Bands used for underlay shows, in order (evidence: the resolved
    /// anchor band, not the frame fallback).
    pub shown_bands: Vec<bool>,
    /// Return the own border as the underlay anchor when set (evidence for
    /// the renderer-owned anchor path without any foreign member tag).
    pub own_border_anchor: bool,
    /// Scripted facts for the own-border anchor check.
    pub own_facts: Option<FreshFacts>,
    /// Scripted carrier visibility (own-anchor eligibility evidence).
    pub visible: [bool; 3],
    /// Scripted carrier tokens (same-group evidence for the own anchor).
    pub carrier_tokens: [Option<String>; 3],
    /// Hide calls, in order (evidence).
    pub hidden: Vec<RenderCarrier>,
    /// Destroy calls so far.
    pub destroys: u32,
    /// Scripted `destroy_all` result.
    pub destroy_ok: bool,
    /// Scripted carrier identities (`None` = not yet created).
    pub hwnds: [Option<u64>; 3],
    /// Scripted `ensure_created` failure detail (`None` = success).
    pub fail_ensure: Option<String>,
    /// `ensure_created` calls so far (evidence).
    pub ensure_calls: u32,
    /// Scripted per-carrier show failure detail (consumed once each).
    pub fail_shows: std::collections::HashMap<RenderCarrier, String>,
    /// Carriers whose next hide reports unverified (consumed once each).
    pub fail_hides: std::collections::HashSet<RenderCarrier>,
    /// Scripted per-show outcome override (consumed in order).
    pub outcome_script: std::collections::VecDeque<RenderOutcome>,
}

impl FakePresent {
    /// New fake with live defaults, successful destroy, and no identities.
    #[must_use]
    pub fn new() -> Self {
        Self {
            live_facts: Some(FreshFacts {
                alive: true,
                identity_ok: true,
                visible: true,
                zoom_ok: true,
                fullscreen: false,
                foreground_ok: true,
                band_ok: true,
            }),
            destroy_ok: true,
            ..Self::default()
        }
    }

    fn facts_for(&self, hwnd: u64) -> FreshFacts {
        self.fresh.get(&hwnd).copied().unwrap_or_else(|| {
            self.live_facts.unwrap_or(FreshFacts {
                alive: false,
                identity_ok: false,
                visible: false,
                zoom_ok: false,
                fullscreen: false,
                foreground_ok: false,
                band_ok: false,
            })
        })
    }

    fn next_outcome(&mut self, checksum: u64, resolved: (u8, u8, u8)) -> PresentOutcome {
        match self.outcome_script.pop_front() {
            Some(outcome) => PresentOutcome {
                outcome,
                checksum,
                error: None,
                resolved: Some(resolved),
            },
            None => PresentOutcome::shown(RenderOutcome::Shown, checksum, resolved),
        }
    }

    /// Resolve the fake border colour like the native carrier: the live
    /// accent wins when the frame requests the theme, else the fallback.
    fn resolve_border_color(&self, frame: &BorderDesired) -> (u8, u8, u8) {
        if frame.use_theme {
            self.accent.unwrap_or(frame.color)
        } else {
            frame.color
        }
    }

    /// FNV-1a checksum over the ring predicate walk: byte-identical in shape
    /// to the native painter (which evaluates the same
    /// [`crate::active_border::ring_covers`] predicate per pixel), so the
    /// core ack path is covered with the real coverage function.
    fn border_checksum(
        outer_w: i32,
        outer_h: i32,
        width_px: i32,
        radius_px: i32,
        color: (u8, u8, u8),
    ) -> u64 {
        const FNV_OFFSET: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;
        let mut hash = FNV_OFFSET;
        let mut mix = |byte: u8| {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        };
        let (r, g, b) = color;
        for y in 0..outer_h {
            for x in 0..outer_w {
                if crate::active_border::ring_covers(x, y, outer_w, outer_h, width_px, radius_px) {
                    for byte in [b, g, r, 255] {
                        mix(byte);
                    }
                } else {
                    for _ in 0..4 {
                        mix(0);
                    }
                }
            }
        }
        hash
    }
}

impl Present for FakePresent {
    fn check_fresh(&self, carrier: RenderCarrier, hwnd: u64, query: &FreshQuery<'_>) -> FreshFacts {
        // Fixture identity, liveness, visibility, zoom, and band stay
        // scripted through `fresh`/`live_facts`; the enforced foreground
        // and fullscreen sets overlay drift on top, so tests script a
        // switch (new identity, new fullscreen HWND) rather than a fact
        // flip, exactly like the native reads observe it. An unreadable
        // post-time foreground (`None`) refuses narrowly per the binding
        // contract, mirroring the native fence: no unprovable stale show.
        let mut facts = self.facts_for(hwnd);
        if query.foreground.is_none() {
            facts.foreground_ok = false;
        } else if let Some(expected) = self.foreground.as_ref() {
            facts.foreground_ok = query.foreground == Some(expected);
        }
        if carrier != RenderCarrier::Preview && self.fullscreen_hwnds.contains(&hwnd) {
            facts.fullscreen = true;
        }
        facts
    }

    fn check_own_anchor(
        &self,
        hwnd: u64,
        _topmost: bool,
        foreground: Option<&WindowLifetime>,
    ) -> FreshFacts {
        // Renderer-owned validation without any foreign member tag: scripted
        // facts plus the enforced-foreground fence, mirroring the native
        // own-anchor read (visible, same-band, foreground unchanged).
        // Unreadable foreground refuses narrowly like the native fence.
        let mut facts = self.own_facts.unwrap_or_else(|| self.facts_for(hwnd));
        if foreground.is_none() {
            facts.foreground_ok = false;
        } else if let Some(expected) = self.foreground.as_ref() {
            facts.foreground_ok = foreground.is_some_and(|query| query == expected);
        }
        facts
    }

    fn resolve_anchor(&self, frame: &UnderlayDesired) -> Option<ResolvedAnchor> {
        // Default fixture resolution: the first candidate with its own
        // lifetime (the owner orders the fresh lowest anchor first).
        // Unreadable-order cases use empty candidate lists, which exercise
        // the owner-fallback path in the core. The own-border path returns
        // the renderer-owned anchor with no foreign lifetime, so tests
        // prove the paired-identity fix instead of hiding it.
        if self.own_border_anchor {
            return self.hwnds[RenderCarrier::Border.index()]
                .map(|hwnd| ResolvedAnchor::OwnBorder { hwnd });
        }
        frame
            .candidates
            .first()
            .map(|candidate| ResolvedAnchor::Foreign {
                hwnd: candidate.hwnd,
                lifetime: candidate.lifetime.clone(),
            })
    }

    fn read_band(&self, hwnd: u64) -> Option<bool> {
        if hwnd == 0 {
            return None;
        }
        Some(*self.bands.get(&hwnd).unwrap_or(&false))
    }

    fn carrier_visible(&self, carrier: RenderCarrier) -> bool {
        self.visible[carrier.index()]
    }

    fn carrier_token(&self, carrier: RenderCarrier) -> Option<String> {
        self.carrier_tokens[carrier.index()].clone()
    }

    fn ensure_created(&mut self) -> Result<(), String> {
        self.ensure_calls += 1;
        match self.fail_ensure.clone() {
            Some(detail) => Err(detail),
            None => Ok(()),
        }
    }

    fn show_border(&mut self, frame: &BorderDesired) -> PresentOutcome {
        self.shown.push(DesiredFrame::Border(frame.clone()));
        if let Some(detail) = self.fail_shows.remove(&RenderCarrier::Border) {
            return PresentOutcome::failed(detail);
        }
        let resolved = self.resolve_border_color(frame);
        let checksum = Self::border_checksum(
            frame.outer.w,
            frame.outer.h,
            frame.style_px.0,
            frame.style_px.2,
            resolved,
        );
        self.next_outcome(checksum, resolved)
    }

    fn show_underlay(
        &mut self,
        frame: &UnderlayDesired,
        anchor: u64,
        topmost: bool,
    ) -> PresentOutcome {
        self.shown.push(DesiredFrame::Underlay(frame.clone()));
        self.shown_anchors.push(anchor);
        self.shown_bands.push(topmost);
        if let Some(detail) = self.fail_shows.remove(&RenderCarrier::Underlay) {
            return PresentOutcome::failed(detail);
        }
        let mut pixels = vec![0u8; frame.outer.w as usize * frame.outer.h as usize * 4];
        let checksum = crate::group_underlay::paint_fill_argb(
            &mut pixels,
            frame.outer.w,
            frame.outer.h,
            (frame.color.1, frame.color.2, frame.color.3),
            frame.color.0,
        );
        let resolved = (frame.color.1, frame.color.2, frame.color.3);
        self.next_outcome(checksum, resolved)
    }

    fn show_preview(&mut self, frame: &PreviewDesired) -> PresentOutcome {
        self.shown.push(DesiredFrame::Preview(frame.clone()));
        if let Some(detail) = self.fail_shows.remove(&RenderCarrier::Preview) {
            return PresentOutcome::failed(detail);
        }
        let mut pixels = vec![0u8; frame.outer.w as usize * frame.outer.h as usize * 4];
        let checksum = crate::group_underlay::paint_fill_argb(
            &mut pixels,
            frame.outer.w,
            frame.outer.h,
            (frame.color.1, frame.color.2, frame.color.3),
            frame.color.0,
        );
        let resolved = (frame.color.1, frame.color.2, frame.color.3);
        self.next_outcome(checksum, resolved)
    }

    fn hide(&mut self, carrier: RenderCarrier) -> bool {
        self.hidden.push(carrier);
        !self.fail_hides.remove(&carrier)
    }

    fn destroy_all(&mut self) -> bool {
        self.destroys += 1;
        self.destroy_ok
    }

    fn carrier_hwnd(&self, carrier: RenderCarrier) -> Option<u64> {
        self.hwnds[carrier.index()]
    }
}

#[cfg(windows)]
pub mod native {
    //! `cfg(windows)` renderer thread: creator-side carrier lifecycle,
    //! message pump, readiness/stop/failure reporting. Minimal Win32 FFI
    //! `unsafe` at call sites only; all handoff state stays in safe
    //! [`crate::render_handoff`] primitives.

    use super::{FreshQuery, Present, PresentOutcome, RenderCore, ResolvedAnchor};
    use crate::active_border_sys::{
        BorderOverlay, OverlayOutcome, PreviewOverlay, UnderlayOverlay, lowest_in_z,
    };
    use crate::native::HeldProcess;
    use crate::render_handoff::{
        BorderDesired, FreshFacts, PreviewDesired, RenderCarrier, RenderChannels, RenderLifecycle,
        RenderOutcome, RenderStopReport, UnderlayCandidate, UnderlayDesired, WindowLifetime,
        render_slot_release, render_slot_try_reserve,
    };
    use std::os::windows::io::{AsHandle as _, AsRawHandle as _, FromRawHandle as _, OwnedHandle};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{SyncSender, sync_channel},
    };
    use std::time::{Duration, Instant};

    /// `DWMWA_CLOAKED` (14): mirrors the owner anchor probe in `tiling_sys`
    /// (`underlay_anchor_renderable`); cloak-query failure fails closed.
    const DWMWA_CLOAKED: u32 = 14;

    /// Native renderer backend: the three owned carriers live here, on the
    /// renderer thread, for the whole renderer lifetime. Creation,
    /// presentation, hide and destroy all run on this thread (creator
    /// affinity); the owner only posts data.
    pub struct NativePresent {
        border: BorderOverlay,
        underlay: UnderlayOverlay,
        preview: PreviewOverlay,
    }

    impl NativePresent {
        /// New backend with uncreated carriers (windows appear on the
        /// creator-side ensure before readiness, exactly like the
        /// same-thread path creates them on first show).
        #[must_use]
        pub fn new() -> Self {
            Self {
                border: BorderOverlay::default(),
                underlay: UnderlayOverlay::default(),
                preview: PreviewOverlay::default(),
            }
        }

        /// Creator-side ensure: create all three hidden carrier windows up
        /// front so readiness never signals before the carriers exist and
        /// the owner exclusion snapshot is armed before the first show.
        /// Windows are created hidden (never shown here). Fails closed with
        /// bounded detail; the caller retries narrowly with backoff.
        /// (Also serves the [`Present`] seam: the portable core re-ensures
        /// before every present so a mid-life loss recreates before the
        /// show, with the snapshot published first.)
        fn ensure_present(&mut self) -> Result<(), String> {
            self.border
                .ensure_created()
                .map_err(|detail| format!("error: border carrier create failed: {detail}"))?;
            self.underlay
                .ensure_created()
                .map_err(|detail| format!("error: underlay carrier create failed: {detail}"))?;
            self.preview
                .ensure_created()
                .map_err(|detail| format!("error: preview carrier create failed: {detail}"))?;
            Ok(())
        }

        /// All-false facts: unreadable or mismatched reads fail closed.
        fn dead_facts() -> FreshFacts {
            FreshFacts {
                alive: false,
                identity_ok: false,
                visible: false,
                zoom_ok: false,
                fullscreen: false,
                foreground_ok: false,
                band_ok: false,
            }
        }

        /// Identity match against the owner-observed lifetime: fresh pid,
        /// process creation, and visible-membership tag. Renderer read-only
        /// (process query plus a property read); never owner observation,
        /// Engine, or foreign writes. Unreadable fails closed.
        fn lifetime_matches(hwnd_u64: u64, expected: &WindowLifetime) -> bool {
            use windows_sys::Win32::Foundation::HWND;
            let mut pid: u32 = 0;
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
                    hwnd_u64 as usize as HWND,
                    &mut pid,
                );
            }
            if pid == 0 || pid != expected.pid {
                return false;
            }
            let creation_ok = HeldProcess::open(pid)
                .ok()
                .and_then(|held| held.identity().ok())
                .is_some_and(|ident| ident.process_creation == expected.creation);
            if !creation_ok {
                return false;
            }
            crate::product_hide::sys::read_member_tag(hwnd_u64).as_deref()
                == expected.member_tag.as_deref()
        }

        /// DWM cloak read on a live window: uncloaked only; query failure
        /// fails closed (not renderable, not visible).
        fn dwm_uncloaked(hwnd: windows_sys::Win32::Foundation::HWND) -> bool {
            use windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute;
            let mut cloaked: i32 = 0;
            if unsafe {
                DwmGetWindowAttribute(
                    hwnd,
                    DWMWA_CLOAKED,
                    (&mut cloaked as *mut i32).cast(),
                    std::mem::size_of::<i32>() as u32,
                )
            } != 0
            {
                return false;
            }
            cloaked == 0
        }

        /// Fresh renderable probe for one HWND: live, visible, non-iconic,
        /// non-cloaked. Mirrors the owner anchor probe so the renderer
        /// re-resolves over the same population the owner chose from.
        fn anchor_renderable(hwnd_u64: u64) -> bool {
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                IsIconic, IsWindow, IsWindowVisible,
            };
            if hwnd_u64 == 0 {
                return false;
            }
            let hwnd = hwnd_u64 as usize as HWND;
            if unsafe { IsWindow(hwnd) } == 0 {
                return false;
            }
            if unsafe { IsWindowVisible(hwnd) } == 0 {
                return false;
            }
            if unsafe { IsIconic(hwnd) } != 0 {
                return false;
            }
            Self::dwm_uncloaked(hwnd)
        }

        /// One enriched candidate validates individually: the pid fence is
        /// cheap and runs first, then the renderable probe, then maximized
        /// and captionless-fullscreen suppression, then the full lifetime
        /// identity (creation plus membership tag). Any failure fails that
        /// candidate closed; other candidates still resolve.
        fn candidate_renderable(
            candidate: &UnderlayCandidate,
            fulls: &[tiler_core::geometry::Rect],
        ) -> bool {
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::IsZoomed;
            if candidate.hwnd == 0 {
                return false;
            }
            let mut pid: u32 = 0;
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
                    candidate.hwnd as usize as HWND,
                    &mut pid,
                );
            }
            if pid == 0 || pid != candidate.lifetime.pid {
                return false;
            }
            if !Self::anchor_renderable(candidate.hwnd) {
                return false;
            }
            if unsafe { IsZoomed(candidate.hwnd as usize as HWND) } != 0 {
                return false;
            }
            if Self::captionless_fullscreen(candidate.hwnd, fulls).unwrap_or(true) {
                return false;
            }
            Self::lifetime_matches(candidate.hwnd, &candidate.lifetime)
        }

        /// Captionless-fullscreen predicate over the carried monitor fulls:
        /// the same pure [`crate::tiling::is_borderless_fullscreen`] check
        /// the owner path uses, evaluated renderer-side from a fresh style
        /// caption bit plus a fresh DWM extended-frame read. `None` when
        /// either read is unreadable (the caller fails closed).
        fn captionless_fullscreen(
            hwnd_u64: u64,
            fulls: &[tiler_core::geometry::Rect],
        ) -> Option<bool> {
            use windows_sys::Win32::Foundation::{HWND, RECT};
            use windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                GWL_STYLE, GetWindowLongW, WS_CAPTION,
            };
            /// `DWMWA_EXTENDED_FRAME_BOUNDS` (9): mirrors the owner probe.
            const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;
            if hwnd_u64 == 0 {
                return None;
            }
            let hwnd = hwnd_u64 as usize as HWND;
            unsafe {
                windows_sys::Win32::Foundation::SetLastError(0);
            }
            let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
            let style_err = unsafe { windows_sys::Win32::Foundation::GetLastError() };
            if style == 0 && style_err != 0 {
                return None;
            }
            if style & WS_CAPTION != 0 {
                return Some(false);
            }
            let mut frame: RECT = unsafe { std::mem::zeroed() };
            if unsafe {
                DwmGetWindowAttribute(
                    hwnd,
                    DWMWA_EXTENDED_FRAME_BOUNDS,
                    (&mut frame as *mut RECT).cast(),
                    std::mem::size_of::<RECT>() as u32,
                )
            } != 0
            {
                return None;
            }
            let visible = tiler_core::geometry::Rect {
                x: frame.left,
                y: frame.top,
                w: frame.right.saturating_sub(frame.left),
                h: frame.bottom.saturating_sub(frame.top),
            };
            Some(crate::tiling::is_borderless_fullscreen(
                true, visible, fulls,
            ))
        }

        /// Fresh `WS_EX_TOPMOST` band read on one HWND (read-only).
        fn read_topmost(hwnd_u64: u64) -> Option<bool> {
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                GWL_EXSTYLE, GetWindowLongW, WS_EX_TOPMOST,
            };
            if hwnd_u64 == 0 {
                return None;
            }
            let band = unsafe { GetWindowLongW(hwnd_u64 as usize as HWND, GWL_EXSTYLE) } as u32
                & WS_EX_TOPMOST
                != 0;
            Some(band)
        }

        fn map_outcome(
            outcome: OverlayOutcome,
            checksum: u64,
            error: Option<String>,
            resolved: (u8, u8, u8),
        ) -> PresentOutcome {
            match outcome {
                OverlayOutcome::Hidden => PresentOutcome::failed(
                    error.unwrap_or_else(|| "error: renderer present failed".to_owned()),
                ),
                OverlayOutcome::Shown => {
                    PresentOutcome::shown(RenderOutcome::Shown, checksum, resolved)
                }
                OverlayOutcome::Moved => {
                    PresentOutcome::shown(RenderOutcome::Moved, checksum, resolved)
                }
                OverlayOutcome::Redrew => {
                    PresentOutcome::shown(RenderOutcome::Redrew, checksum, resolved)
                }
                OverlayOutcome::Unchanged => {
                    PresentOutcome::shown(RenderOutcome::Unchanged, checksum, resolved)
                }
            }
        }
    }

    impl Default for NativePresent {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Present for NativePresent {
        fn check_fresh(
            &self,
            carrier: RenderCarrier,
            hwnd: u64,
            query: &FreshQuery<'_>,
        ) -> FreshFacts {
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                GetForegroundWindow, IsIconic, IsWindow, IsWindowVisible, IsZoomed,
            };
            if hwnd == 0 {
                return Self::dead_facts();
            }
            let raw = hwnd as usize as HWND;
            if unsafe { IsWindow(raw) } == 0 {
                return Self::dead_facts();
            }
            // Recycled-HWND fence first: a live window with a dead identity
            // keeps `alive` but fails `identity_ok`, so the gate refuses
            // with the narrow recycled-ID reason instead of presenting.
            if !Self::lifetime_matches(hwnd, query.expected) {
                return FreshFacts {
                    alive: true,
                    identity_ok: false,
                    visible: false,
                    zoom_ok: false,
                    fullscreen: false,
                    foreground_ok: false,
                    band_ok: false,
                };
            }
            // Visibility folds non-iconic and DWM-uncloaked, mirroring the
            // owner anchor probe: iconic, cloaked, or cloak-query failure
            // reads not visible (fail closed, never a weaker game-safety
            // gate than the same-thread path). An unreadable fullscreen
            // frame read fails the same way: async safety never trusts a
            // torn frame.
            let fs_read = if carrier == RenderCarrier::Preview {
                Some(false)
            } else {
                Self::captionless_fullscreen(hwnd, query.fulls)
            };
            let visible = unsafe { IsWindowVisible(raw) } != 0
                && unsafe { IsIconic(raw) } == 0
                && Self::dwm_uncloaked(raw)
                && fs_read.is_some();
            let zoom_ok = unsafe { IsZoomed(raw) } == 0;
            // Renderer-evaluated suppression: the captionless-fullscreen
            // predicate over the carried monitor fulls, honoring only the
            // explicit owner bypass behind a tag-proven lifetime. Preview
            // movers are exempt (their plane is above; the foreground fence
            // below still guards the switch).
            let fullscreen = fs_read.is_some_and(|fs| {
                fs && !(query.fullscreen_bypass && query.expected.member_tag.is_some())
            });
            // Foreground fence on every carrier: the live foreground
            // lifetime must still match the desired foreground. A late
            // switch (game, suspend, another window) refuses even when the
            // mover itself is legitimately unfocused (Win+Left holds keep
            // their existing semantics: only a *changed* foreground
            // refuses, never an unfocused mover). An unreadable post-time
            // foreground (`None`) refuses narrowly per the binding
            // contract: no unprovable stale show, while readable unchanged
            // foregrounds keep ordinary drag behavior.
            let foreground_ok = match query.foreground {
                None => false,
                Some(expected) => {
                    let fg = unsafe { GetForegroundWindow() } as usize as u64;
                    fg != 0 && Self::lifetime_matches(fg, expected)
                }
            };
            let band_ok = Self::read_topmost(hwnd).is_some_and(|band| band == query.topmost);
            FreshFacts {
                alive: true,
                identity_ok: true,
                visible,
                zoom_ok,
                fullscreen,
                foreground_ok,
                band_ok,
            }
        }

        fn check_own_anchor(
            &self,
            hwnd: u64,
            topmost: bool,
            foreground: Option<&WindowLifetime>,
        ) -> FreshFacts {
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                GetForegroundWindow, IsWindowVisible,
            };
            if hwnd == 0 {
                return Self::dead_facts();
            }
            let raw = hwnd as usize as HWND;
            if !crate::active_border_sys::overlay_window_alive(hwnd) {
                return Self::dead_facts();
            }
            let visible = unsafe { IsWindowVisible(raw) } != 0;
            let band_ok = Self::read_topmost(hwnd).is_some_and(|band| band == topmost);
            let fg = unsafe { GetForegroundWindow() } as usize as u64;
            let foreground_ok = match foreground {
                None => false,
                Some(expected) => fg != 0 && Self::lifetime_matches(fg, expected),
            };
            FreshFacts {
                alive: true,
                identity_ok: true,
                visible,
                zoom_ok: true,
                fullscreen: false,
                foreground_ok,
                band_ok,
            }
        }

        fn resolve_anchor(&self, frame: &UnderlayDesired) -> Option<ResolvedAnchor> {
            // Each foreign candidate validates individually against fresh
            // native reads (pid fence, renderable probe, maximized and
            // fullscreen suppression, lifetime identity); stale Engine
            // members between reconciles fail closed here, never winning
            // the anchor.
            let mut live: Vec<u64> = frame
                .candidates
                .iter()
                .filter(|candidate| Self::candidate_renderable(candidate, &frame.fulls))
                .map(|candidate| candidate.hwnd)
                .collect();
            // The renderer's own border surface sorts into the same
            // comparison only when visible, same-group, and same-band: a
            // hidden or unrelated border never wins the anchor.
            let own = self
                .border
                .hwnd()
                .filter(|_| self.border.is_visible())
                .filter(|_| {
                    self.border.target_token().as_deref() == Some(frame.anchor_token.as_str())
                })
                .filter(|hwnd| Self::read_topmost(*hwnd) == Some(frame.topmost));
            if let Some(border) = own {
                live.push(border);
            }
            let anchor = lowest_in_z(&live)?;
            if Some(anchor) == own {
                Some(ResolvedAnchor::OwnBorder { hwnd: anchor })
            } else {
                let lifetime = frame
                    .candidates
                    .iter()
                    .find(|candidate| candidate.hwnd == anchor)
                    .map(|candidate| candidate.lifetime.clone())?;
                Some(ResolvedAnchor::Foreign {
                    hwnd: anchor,
                    lifetime,
                })
            }
        }

        fn read_band(&self, hwnd: u64) -> Option<bool> {
            Self::read_topmost(hwnd)
        }

        fn carrier_visible(&self, carrier: RenderCarrier) -> bool {
            match carrier {
                RenderCarrier::Border => self.border.is_visible(),
                RenderCarrier::Underlay => self.underlay.is_visible(),
                RenderCarrier::Preview => self.preview.is_visible(),
            }
        }

        fn carrier_token(&self, carrier: RenderCarrier) -> Option<String> {
            match carrier {
                RenderCarrier::Border => self.border.target_token(),
                RenderCarrier::Underlay => self.underlay.anchor_token(),
                RenderCarrier::Preview => self.preview.anchor_token(),
            }
        }

        fn ensure_created(&mut self) -> Result<(), String> {
            self.ensure_present()
        }

        fn show_border(&mut self, frame: &BorderDesired) -> PresentOutcome {
            // The renderer owns the accent: the frame carries the configured
            // fallback plus the theme choice, and the carrier resolves the
            // live system accent itself (same query/broadcast cache as the
            // same-thread path), so an accent change repaints with no
            // owner-side query. Geometry and style scale stay owner-resolved
            // physical pixels in the frame.
            let outcome = self.border.show_at(
                frame.outer,
                frame.style_px.0,
                frame.style_px.1,
                frame.style_px.2,
                &crate::active_border::ActiveBorderStyle {
                    width: 0.0,
                    gap: 0.0,
                    radius: 0.0,
                    color: frame.color,
                    use_theme: frame.use_theme,
                },
                &frame.target_token,
                frame.topmost,
                frame.target_hwnd,
            );
            let _ = &frame;
            // Actual presented colour read back from the carrier (accent
            // resolved or fallback): the ack logs this, never the owner
            // fallback, so themed presents report honest pixels.
            let resolved = self.border.current_color().unwrap_or(frame.color);
            Self::map_outcome(
                outcome,
                self.border.dib_checksum(),
                self.border.last_error().map(str::to_owned),
                resolved,
            )
        }

        fn show_underlay(
            &mut self,
            frame: &UnderlayDesired,
            anchor: u64,
            topmost: bool,
        ) -> PresentOutcome {
            let outcome = self.underlay.show_fill_at(
                frame.outer,
                frame.color,
                &frame.anchor_token,
                topmost,
                anchor,
            );
            let resolved = self
                .underlay
                .current_fill()
                .map(|(_, r, g, b)| (r, g, b))
                .unwrap_or((frame.color.1, frame.color.2, frame.color.3));
            Self::map_outcome(
                outcome,
                self.underlay.dib_checksum(),
                self.underlay.last_error().map(str::to_owned),
                resolved,
            )
        }

        fn show_preview(&mut self, frame: &PreviewDesired) -> PresentOutcome {
            let outcome = self.preview.show_fill_above(
                frame.outer,
                frame.color,
                &frame.anchor_token,
                frame.anchor_hwnd,
            );
            let resolved = self
                .preview
                .current_fill()
                .map(|(_, r, g, b)| (r, g, b))
                .unwrap_or((frame.color.1, frame.color.2, frame.color.3));
            Self::map_outcome(
                outcome,
                self.preview.dib_checksum(),
                self.preview.last_error().map(str::to_owned),
                resolved,
            )
        }

        fn hide(&mut self, carrier: RenderCarrier) -> bool {
            match carrier {
                RenderCarrier::Border => self.border.hide_verified(),
                RenderCarrier::Underlay => self.underlay.hide_verified(),
                RenderCarrier::Preview => self.preview.hide_verified(),
            }
        }

        fn destroy_all(&mut self) -> bool {
            // Verified creator-side destruction: each carrier keeps its live
            // handle on failure so a bounded retry can re-drive it; success
            // requires every HWND to read destroyed.
            let border_ok = self.border.destroy();
            let underlay_ok = self.underlay.destroy();
            let preview_ok = self.preview.destroy();
            border_ok && underlay_ok && preview_ok
        }

        fn carrier_hwnd(&self, carrier: RenderCarrier) -> Option<u64> {
            match carrier {
                RenderCarrier::Border => self.border.hwnd(),
                RenderCarrier::Underlay => self.underlay.hwnd(),
                RenderCarrier::Preview => self.preview.hwnd(),
            }
        }
    }

    /// Bounded renderer lifecycle event for the owner writer. The renderer
    /// offers through a capacity-8 slot and drops (counted) under owner
    /// stall; readiness and stop completion ride dedicated lossless slots.
    /// The renderer never logs.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct RenderFailEvent {
        /// `"create"`, `"present"` or `"destroy"`.
        pub op: &'static str,
        /// Consecutive attempt number.
        pub attempt: u32,
        /// Backoff seconds until the next attempt (0 when none).
        pub retry_secs: u64,
        /// True when a previous failure recovered.
        pub recovered: bool,
        /// Bounded creator-side cause (`None` on recovery notes).
        pub detail: Option<String>,
    }

    /// Bounded renderer -> owner signals. `ready` and `stopped` are lossless
    /// single slots; `failures` is a bounded FIFO (drops counted, never
    /// blocking the renderer).
    pub struct RenderSignals {
        ready: AtomicBool,
        stopped: std::sync::Mutex<Option<bool>>,
        failures: SyncSender<RenderFailEvent>,
        /// Failure events dropped under owner stall (bounded evidence).
        pub events_dropped: AtomicU64,
    }

    impl RenderSignals {
        /// New signals with an empty bounded failure FIFO.
        #[must_use]
        pub fn new(failures: SyncSender<RenderFailEvent>) -> Self {
            Self {
                ready: AtomicBool::new(false),
                stopped: std::sync::Mutex::new(None),
                failures,
                events_dropped: AtomicU64::new(0),
            }
        }

        /// Note readiness (idempotent, lossless).
        pub fn note_ready(&self) {
            self.ready.store(true, Ordering::SeqCst);
        }

        /// Readiness snapshot.
        #[must_use]
        pub fn is_ready(&self) -> bool {
            self.ready.load(Ordering::SeqCst)
        }

        /// Record one lifecycle event without blocking; counts drops.
        pub fn note_failure(&self, event: RenderFailEvent) {
            if self.failures.try_send(event).is_err() {
                self.events_dropped.fetch_add(1, Ordering::Relaxed);
            }
        }

        /// Record stop completion (terminal, lossless).
        pub fn note_stopped(&self, destroyed_ok: bool) {
            *self.stopped.lock().expect("stopped lock") = Some(destroyed_ok);
        }

        /// Stop completion snapshot.
        #[must_use]
        pub fn stopped(&self) -> Option<bool> {
            *self.stopped.lock().expect("stopped lock")
        }
    }

    /// Create one auto-reset kernel event, initially non-signaled.
    fn create_event() -> Result<OwnedHandle, String> {
        let handle = unsafe {
            windows_sys::Win32::System::Threading::CreateEventW(
                std::ptr::null(),
                0,
                0,
                std::ptr::null(),
            )
        };
        if handle.is_null() {
            let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
            return Err(format!("error: render wake creation failed: win32-{code}"));
        }
        Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
    }

    /// Duplicate one wake handle for the renderer thread: each side owns its
    /// lifetime, so a stuck stop that drops the owner handle can never leave
    /// the renderer signaling a recycled kernel object.
    fn duplicate_event(handle: &OwnedHandle) -> Result<OwnedHandle, String> {
        handle
            .try_clone()
            .map_err(|error| format!("error: render wake duplicate failed: {error}"))
    }

    /// Signal one owned wake best-effort. Never waits, never logs.
    fn signal_owned(handle: &OwnedHandle) {
        unsafe {
            windows_sys::Win32::System::Threading::SetEvent(handle.as_handle().as_raw_handle());
        }
    }

    /// Owner handle for exactly one renderer thread. All logging stays on
    /// the owner: the renderer reports readiness, bounded failure events,
    /// per-carrier acks and generation echoes for the owner writer.
    pub struct RenderHandle {
        channels: Arc<RenderChannels>,
        signals: Arc<RenderSignals>,
        failures: std::sync::mpsc::Receiver<RenderFailEvent>,
        lifecycle: RenderLifecycle,
        thread: Option<std::thread::JoinHandle<RenderStopReport>>,
        /// Completed stop report: makes a second `wait_stopped` return the
        /// same report instead of panicking on the consumed join handle.
        stop_report: Option<RenderStopReport>,
        /// Renderer -> owner ack wake (auto-reset, owner-owned): the owner
        /// outer wait includes it alongside the input wake. Always present:
        /// spawn refuses cleanly when creation fails, so event-driven
        /// startup is never claimed over a polling fallback.
        ack_wake: OwnedHandle,
        /// Owner -> renderer work wake (auto-reset, owner-owned): signaled
        /// after every desired post, hide, and stop request. Always present
        /// for the same reason.
        work_wake: OwnedHandle,
    }

    impl RenderHandle {
        /// Spawn the dedicated renderer thread. `Err` before any product
        /// effect on wake-creation failure, thread-spawn failure (the owner
        /// logs and continues degraded with same-thread overlays), or while
        /// another renderer holds the process slot (a hung or unclean stop,
        /// including a dropped handle, keeps the reserve so no overlapping
        /// worker can start; recovery is process exit); otherwise the worker
        /// starts in `Starting` and the owner must [`RenderHandle::wait_ready`]
        /// with a bounded wait. The slot is never released on `Drop`: only
        /// a clean stop or a spawn failure frees it. Both cross-thread wakes
        /// are duplicated so each thread owns its handle lifetime.
        pub fn spawn() -> Result<Self, String> {
            if !render_slot_try_reserve() {
                return Err("error: render thread already active".to_owned());
            }
            let fail = |cause: String| {
                render_slot_release();
                cause
            };
            let channels = Arc::new(RenderChannels::new());
            let (fail_tx, fail_rx) = sync_channel(8);
            let signals = Arc::new(RenderSignals::new(fail_tx));
            // Event-driven wakes are required, not best-effort: a creation
            // failure refuses the spawn cleanly (like the input wake) so
            // the owner never claims event-driven behavior over polling.
            let ack_wake = create_event().map_err(fail)?;
            let work_wake = create_event().map_err(fail)?;
            let thread_ack = duplicate_event(&ack_wake).map_err(fail)?;
            let thread_work = duplicate_event(&work_wake).map_err(fail)?;
            let thread_channels = Arc::clone(&channels);
            let thread_signals = Arc::clone(&signals);
            let thread = std::thread::Builder::new()
                .name("omnitiler-render".to_owned())
                .spawn(move || {
                    run_renderer(thread_channels, thread_signals, thread_ack, thread_work)
                })
                .map_err(|err| {
                    render_slot_release();
                    format!("error: render thread spawn failed: {err}")
                })?;
            let mut lifecycle = RenderLifecycle::new();
            lifecycle.start().expect("fresh handle starts");
            Ok(Self {
                channels,
                signals,
                failures: fail_rx,
                lifecycle,
                thread: Some(thread),
                stop_report: None,
                ack_wake,
                work_wake,
            })
        }

        /// Shared channels for owner post/poll/wait.
        #[must_use]
        pub fn channels(&self) -> &Arc<RenderChannels> {
            &self.channels
        }

        /// Current lifecycle state.
        #[must_use]
        pub fn state(&self) -> crate::render_handoff::RenderState {
            self.lifecycle.state()
        }

        /// Borrow the renderer -> owner ack wake. The owner outer wait
        /// includes it alongside the input wake.
        #[must_use]
        pub fn ack_wake(&self) -> &OwnedHandle {
            &self.ack_wake
        }

        /// Signal the owner -> renderer work wake (new desired, hide, or
        /// stop): the renderer pump breaks its quantum early. Never waits,
        /// never logs.
        pub fn notify_work(&self) {
            signal_owned(&self.work_wake);
        }

        /// Bounded readiness wait. Drains failure events for the owner
        /// writer while waiting. Returns true when the renderer reported
        /// ready (lifecycle `Running`): carriers exist and the exclusion
        /// snapshot is published. False on timeout or when the creator
        /// thread exited first (lifecycle `create_failed`, no worker holds
        /// resources so a later start is safe) without claiming readiness.
        pub fn wait_ready(
            &mut self,
            timeout: Duration,
            on_event: &mut dyn FnMut(&RenderFailEvent),
        ) -> bool {
            let deadline = Instant::now() + timeout;
            loop {
                self.drain_failures(on_event);
                if self.signals.is_ready() {
                    let _ = self.lifecycle.note_ready();
                    return true;
                }
                if self.lifecycle.state() == crate::render_handoff::RenderState::Running {
                    return true;
                }
                // The creator thread exited before readiness (or reported
                // stop): wire the creator-side failure into the lifecycle
                // instead of waiting out the bound for a dead thread.
                if self.signals.stopped().is_some() {
                    let _ = self.lifecycle.create_failed();
                    return false;
                }
                if Instant::now() >= deadline {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }

        /// Drain pending lifecycle events without waiting (owner tick).
        pub fn drain_failures(&mut self, on_event: &mut dyn FnMut(&RenderFailEvent)) {
            while let Ok(event) = self.failures.try_recv() {
                on_event(&event);
            }
            if self.signals.is_ready() {
                let _ = self.lifecycle.note_ready();
            }
        }

        /// Request graceful stop: closes admission (owner stops posting) and
        /// signals the renderer through the work wake. Idempotent in
        /// `Stopping` so the owner can re-drive after a failure. The renderer
        /// destroys on its creator thread and acks.
        pub fn request_stop(&mut self) -> Result<(), crate::render_handoff::RenderLifecycleError> {
            self.lifecycle.request_stop()?;
            self.channels.request_stop();
            // Wake the pump so a fully idle wait observes the stop promptly;
            // owner call sites must not signal again.
            signal_owned(&self.work_wake);
            Ok(())
        }

        /// Bounded stop wait. Clean requires the destroy acknowledgement
        /// *and* a successful join: a thread that panicked after its ack, or
        /// a join that fails, reports unclean even when the ack claimed
        /// success, so clean is never fabricated from an ack alone. The
        /// renderer records its stop acknowledgement as its terminal act
        /// before returning, so a finished thread joins immediately. With no
        /// ack the thread joins only when already finished (`is_finished`
        /// first, so a wedged thread never blocks the owner past the bound);
        /// a still-running thread keeps its handle (no detach) and the call
        /// returns unclean while retaining `Stopping`, so no overlapping
        /// worker can start and a later call can still join. Only a clean
        /// stop releases the process render slot; every unclean path keeps
        /// the reserve, including a dropped handle. A second call returns
        /// the stored report instead of panicking. Never waits indefinitely
        /// and never holds channel locks while joining.
        pub fn wait_stopped(&mut self, timeout: Duration) -> RenderStopReport {
            if let Some(report) = self.stop_report {
                return report;
            }
            let deadline = Instant::now() + timeout;
            loop {
                if let Some(destroyed_ok) = self.signals.stopped() {
                    // Ack arrived: complete only against a finished thread
                    // whose join succeeds. A thread wedged between its ack
                    // and its return, or one that panicked, keeps unclean:
                    // the handle is retained for a later bounded wait (or
                    // the panic is recorded) instead of blocking this call
                    // past the deadline or fabricating clean.
                    if let Some(thread) = self.thread.take() {
                        if thread.is_finished() {
                            let joined_ok = thread
                                .join()
                                .map(|report| report.destroyed_ok)
                                .unwrap_or(false);
                            let report = self.complete_stop(destroyed_ok && joined_ok);
                            self.stop_report = Some(report);
                            return report;
                        }
                        self.thread = Some(thread);
                    } else {
                        let report = self.complete_stop(destroyed_ok);
                        self.stop_report = Some(report);
                        return report;
                    }
                }
                if Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            if let Some(thread) = self.thread.take() {
                if thread.is_finished() {
                    let destroyed_ok = thread
                        .join()
                        .map(|report| report.destroyed_ok)
                        .unwrap_or(false);
                    let report = self.complete_stop(destroyed_ok);
                    self.stop_report = Some(report);
                    return report;
                }
                // Still running (hung native call): keep the handle so a
                // later bounded wait can still join; never detach.
                self.thread = Some(thread);
            }
            RenderStopReport {
                destroyed_ok: false,
                clean: false,
            }
        }

        /// Fold one destroy acknowledgement into the lifecycle, releasing
        /// the process slot only on a clean stop.
        fn complete_stop(&mut self, destroyed_ok: bool) -> RenderStopReport {
            match self.lifecycle.note_destroyed(destroyed_ok) {
                Ok(report) => {
                    if report.clean {
                        render_slot_release();
                    }
                    report
                }
                Err(_) => RenderStopReport {
                    destroyed_ok,
                    clean: false,
                },
            }
        }
    }

    /// Native renderer-wait return reason: exactly one per wait. Counts
    /// native `MsgWaitForMultipleObjectsEx` returns only, never thread
    /// context switches and never ack posts (acks ride `ack_seq`). `Timeout`
    /// is the genuine hide-retry deadline (`WAIT_TIMEOUT` 258) only; `Failed`
    /// is `WAIT_FAILED` or any unrecognized code, counted separately and never
    /// merged into timeout. Classification is telemetry only: the loop keeps
    /// its existing path with no new recovery and no timing claim (a failed
    /// wait returns immediately; not proven delayed).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum RenderWait {
        Work,
        Message,
        Timeout,
        Failed,
    }

    /// Classify one native renderer-wait return code. Pure (no native calls)
    /// so tests prove the work/message/timeout/failed split without a desktop.
    #[must_use]
    fn classify_render_wait(result: u32) -> RenderWait {
        const WAIT_OBJECT_0: u32 = 0;
        const WAIT_TIMEOUT: u32 = 258;
        if result == WAIT_TIMEOUT {
            RenderWait::Timeout
        } else if result == WAIT_OBJECT_0 {
            RenderWait::Work
        } else if result == WAIT_OBJECT_0 + 1 {
            RenderWait::Message
        } else {
            RenderWait::Failed
        }
    }

    /// One renderer message turn on the creator thread: delivers broadcast
    /// messages (accent/theme) for windows created here, then waits for more,
    /// owner work, or the next hide-retry deadline. Windows created on this
    /// thread require a queue on this thread; no other thread pumps it. The
    /// thread-owned work wake breaks the wait early on new desired frames,
    /// hides, or stop; otherwise the hide-retry deadline (or an indefinite
    /// idle wait) applies. No fixed idle quantum: every wait source is an
    /// explicit wake or deadline. Returns the native wait reason (one per
    /// wait) for the cumulative trace counters.
    fn pump_messages(timeout_ms: u32, work_wake: &OwnedHandle) -> RenderWait {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, MSG, MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW,
            QS_ALLINPUT, TranslateMessage,
        };
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        loop {
            let has = unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) };
            if has == 0 {
                break;
            }
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let result = unsafe {
            let raw = work_wake.as_handle().as_raw_handle();
            MsgWaitForMultipleObjectsEx(1, &raw, timeout_ms, QS_ALLINPUT, 0)
        };
        let reason = classify_render_wait(result);
        loop {
            let has = unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) };
            if has == 0 {
                break;
            }
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        reason
    }

    /// Renderer thread entry: creator-side ensure with narrow bounded retry
    /// (carriers exist before readiness, and the exclusion snapshot is
    /// published before the first show), then pump broadcasts, drain latest
    /// frames, ack applied state, and destroy every carrier on this creator
    /// thread at stop with verified readbacks. Both wakes are thread-owned
    /// duplicates, so neither outlives the owner handle. Returns the
    /// truthful stop report; never logs (owner writer owns all
    /// timing/outcome lines from acks and signals).
    fn run_renderer(
        channels: Arc<RenderChannels>,
        signals: Arc<RenderSignals>,
        ack_wake: OwnedHandle,
        work_wake: OwnedHandle,
    ) -> RenderStopReport {
        let mut core = RenderCore::new(NativePresent::new(), Arc::clone(&channels));
        // Creation attempts ride bounded backoff (2s, 4s, 8s cap) with one
        // failure event per attempt (bounded cause included) for the owner
        // writer; a stop request wins over the next retry. Recovery after
        // attempts reports a recovered event so the owner log pairs
        // unavailable/available.
        let mut attempt: u32 = 0;
        let ready = loop {
            match core.present_mut().ensure_present() {
                Ok(()) => {
                    core.publish_all();
                    if attempt > 0 {
                        signals.note_failure(RenderFailEvent {
                            op: "create",
                            attempt,
                            retry_secs: 0,
                            recovered: true,
                            detail: None,
                        });
                    }
                    signals.note_ready();
                    break true;
                }
                Err(detail) => {
                    attempt += 1;
                    let backoff = (1u64 << attempt.min(3)).min(8);
                    signals.note_failure(RenderFailEvent {
                        op: "create",
                        attempt,
                        retry_secs: backoff,
                        recovered: false,
                        detail: Some(crate::render_handoff::truncate_render_error(&detail)),
                    });
                    let until = Instant::now() + Duration::from_secs(backoff);
                    while Instant::now() < until {
                        if channels.stop_requested() {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    if channels.stop_requested() {
                        break false;
                    }
                }
            }
        };
        if ready {
            // Deadline-driven pump: the wait breaks on owner work, queued
            // broadcast messages, or the next hide-retry deadline - never
            // on a fixed idle quantum. Every ack post signals the owner
            // wake on real ack traffic only (tracked by sequence, never a
            // busy loop); owner work breaks the wait early through its wake.
            let mut last_seq = channels.ack_seq();
            loop {
                let timeout_ms = match core.next_timer() {
                    Some(due) => due
                        .saturating_duration_since(Instant::now())
                        .as_millis()
                        .min(u128::from(u32::MAX)) as u32,
                    // Fully idle: INFINITE. The work wake and the message
                    // queue still wake the pump; nothing polls.
                    None => u32::MAX,
                };
                let reason = pump_messages(timeout_ms, &work_wake);
                // Cumulative native wait returns (one per wait, by reason);
                // ack posts stay on `ack_seq`, never here. A stop wait still
                // counts its return: the break below only skips the step.
                // Failed returns keep the existing path (step, then stop
                // check); only the counter differs.
                match reason {
                    RenderWait::Work => channels.note_wait_work(),
                    RenderWait::Message => channels.note_wait_msg(),
                    RenderWait::Timeout => channels.note_wait_timeout(),
                    RenderWait::Failed => channels.note_wait_failed(),
                }
                if channels.stop_requested() {
                    break;
                }
                core.step();
                let seq = channels.ack_seq();
                if seq != last_seq {
                    last_seq = seq;
                    signal_owned(&ack_wake);
                }
            }
        }
        // Creator-side destruction with bounded retry: failed destroys keep
        // their live handles inside the carriers, so each re-drive retries
        // the same windows. Report truthfully and keep the slot on failure.
        let mut destroyed_ok = core.present_mut().destroy_all();
        for _ in 0..2 {
            if destroyed_ok {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
            destroyed_ok = core.present_mut().destroy_all();
        }
        signals.note_stopped(destroyed_ok);
        RenderStopReport {
            destroyed_ok,
            clean: destroyed_ok,
        }
    }

    /// Handle-level tests without a live renderer thread (no carrier windows,
    /// no native effects): the stop wake is asserted as bare kernel state.
    #[cfg(test)]
    mod native_tests {
        use super::*;

        fn signaled(handle: &OwnedHandle) -> bool {
            use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
            use windows_sys::Win32::System::Threading::WaitForSingleObject;
            let raw = handle.as_handle().as_raw_handle();
            unsafe { WaitForSingleObject(raw, 0) == WAIT_OBJECT_0 }
        }

        fn bare_handle() -> RenderHandle {
            let (fail_tx, fail_rx) = sync_channel(8);
            let mut lifecycle = RenderLifecycle::new();
            lifecycle.start().expect("fresh handle starts");
            RenderHandle {
                channels: Arc::new(RenderChannels::new()),
                signals: Arc::new(RenderSignals::new(fail_tx)),
                failures: fail_rx,
                lifecycle,
                thread: None,
                stop_report: None,
                ack_wake: create_event().expect("ack wake"),
                work_wake: create_event().expect("work wake"),
            }
        }

        #[test]
        fn request_stop_signals_work_wake_without_owner_notify() {
            // Bare stop event: `request_stop` alone (no explicit owner
            // `notify_work`) leaves the work wake pending, so a fully idle
            // renderer wait observes the stop promptly.
            let mut handle = bare_handle();
            assert!(!signaled(&handle.work_wake), "no spurious renderer wake");
            handle.request_stop().expect("stop requested");
            assert!(
                signaled(&handle.work_wake),
                "bare request_stop wakes the renderer"
            );
            assert!(!signaled(&handle.work_wake), "auto-reset consumes the wake");
        }

        #[test]
        fn wait_reason_splits_work_message_and_retry_timeout() {
            // Native return-code split (no desktop): owner work, queued
            // broadcast messages, and the genuine hide-retry deadline (258)
            // map distinctly; `WAIT_FAILED` and any unrecognized code count
            // as `Failed`, never merged into timeout.
            assert_eq!(classify_render_wait(0), RenderWait::Work);
            assert_eq!(classify_render_wait(1), RenderWait::Message);
            assert_eq!(classify_render_wait(258), RenderWait::Timeout);
            assert_eq!(classify_render_wait(0xFFFF_FFFF), RenderWait::Failed);
            assert_eq!(classify_render_wait(7), RenderWait::Failed);
        }

        #[test]
        fn wait_counts_sum_with_acks_tracked_separately() {
            // Wake-evidence shape: wait returns sum by reason while ack posts
            // stay on `ack_seq`, so a trace diff distinguishes a blocking
            // wait from ack traffic (the Stage3 idle question). Failures stay
            // separate from genuine timeouts.
            use crate::render_handoff::{CarrierAck, RenderCarrier, RenderOutcome};
            let channels = RenderChannels::new();
            assert_eq!(channels.wait_counts(), (0, 0, 0, 0, 0));
            assert_eq!(channels.ack_seq(), 0);
            channels.note_wait_work();
            channels.note_wait_msg();
            channels.note_wait_timeout();
            channels.note_wait_timeout();
            channels.note_wait_failed();
            assert_eq!(channels.wait_counts(), (5, 1, 1, 2, 1));
            channels.post_ack(
                RenderCarrier::Border,
                CarrierAck {
                    generation: 1,
                    safety: 1,
                    outcome: RenderOutcome::Hidden,
                    checksum: 0,
                    span_us: 0,
                    queue_age_us: 0,
                    last_error: None,
                    hidden_proven: true,
                    identity_proven: true,
                    applied: None,
                },
            );
            assert_eq!(channels.ack_seq(), 1);
            // An ack never moves the wait counters.
            assert_eq!(channels.wait_counts(), (5, 1, 1, 2, 1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_handoff::{RenderCarrier, RenderOutcome, UnderlayCandidate};
    use tiler_core::geometry::Rect;

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    fn lifetime_for(hwnd: u64) -> WindowLifetime {
        WindowLifetime {
            pid: 4000 + hwnd as u32,
            creation: format!("creation-{hwnd}"),
            member_tag: Some(format!("tag-{hwnd}")),
        }
    }

    fn candidate(hwnd: u64) -> UnderlayCandidate {
        UnderlayCandidate {
            hwnd,
            lifetime: lifetime_for(hwnd),
        }
    }

    fn fulls() -> Vec<Rect> {
        vec![rect(0, 0, 1920, 1080)]
    }

    fn border_frame(generation: u64, safety: u64, x: i32, hwnd: u64) -> DesiredFrame {
        DesiredFrame::Border(BorderDesired {
            generation,
            safety,
            outer: rect(x, 0, 100, 80),
            style_px: (3, 0, 6),
            color: (0x2a, 0x82, 0xda),
            use_theme: false,
            target_token: format!("token-{hwnd}"),
            target_hwnd: hwnd,
            lifetime: lifetime_for(hwnd),
            foreground: lifetime_for(hwnd),
            fulls: fulls(),
            fullscreen_bypass: false,
            topmost: false,
        })
    }

    fn underlay_frame(
        generation: u64,
        safety: u64,
        anchor: u64,
        candidates: Vec<u64>,
    ) -> DesiredFrame {
        DesiredFrame::Underlay(UnderlayDesired {
            generation,
            safety,
            outer: rect(0, 0, 200, 100),
            color: (0x40, 0x80, 0x80, 0x80),
            anchor_token: format!("token-{anchor}"),
            anchor_hwnd: anchor,
            anchor_lifetime: lifetime_for(anchor),
            candidates: candidates.into_iter().map(candidate).collect(),
            foreground: lifetime_for(anchor),
            fulls: fulls(),
            topmost: false,
        })
    }

    fn preview_frame(generation: u64, safety: u64, mover: u64) -> DesiredFrame {
        DesiredFrame::Preview(PreviewDesired {
            generation,
            safety,
            outer: rect(10, 10, 300, 200),
            color: (0x40, 0x2a, 0x82, 0xda),
            anchor_token: format!("token-{mover}"),
            anchor_hwnd: mover,
            lifetime: lifetime_for(mover),
            foreground: Some(lifetime_for(mover)),
        })
    }

    fn dead_facts() -> FreshFacts {
        FreshFacts {
            alive: false,
            identity_ok: false,
            visible: false,
            zoom_ok: false,
            fullscreen: false,
            foreground_ok: false,
            band_ok: false,
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
    fn latest_wins_coalesces_bursts_to_newest_geometry() {
        let channels = Arc::new(RenderChannels::new());
        let mut core = RenderCore::new(FakePresent::new(), Arc::clone(&channels));
        channels.post(border_frame(1, 0, 10, 1001));
        channels.post(border_frame(2, 0, 20, 1001));
        channels.post(border_frame(3, 0, 30, 1001));
        core.step();
        // Exactly one present, carrying the newest geometry.
        assert_eq!(core.present().shown.len(), 1);
        match &core.present().shown[0] {
            DesiredFrame::Border(frame) => assert_eq!(frame.outer.x, 30),
            _ => panic!("border frame presents"),
        }
        let ack = channels
            .take_ack(RenderCarrier::Border)
            .expect("applied ack");
        assert_eq!(ack.generation, 3);
        assert_eq!(ack.outcome, RenderOutcome::Shown);
    }

    #[test]
    fn queue_age_separates_from_present_span_without_sleeps() {
        // Injected post stamp proves the transport wait separates from the
        // present span: an aged queue entry reports a large `queue_age_us`
        // while the present span stays near zero. No sleeps.
        let channels = Arc::new(RenderChannels::new());
        let mut core = RenderCore::new(FakePresent::new(), Arc::clone(&channels));
        let aged = Instant::now() - Duration::from_millis(300);
        channels.post_at(border_frame(1, 0, 10, 1001), aged);
        core.step();
        let ack = channels
            .take_ack(RenderCarrier::Border)
            .expect("applied ack");
        assert_eq!(ack.outcome, RenderOutcome::Shown);
        assert!(
            ack.queue_age_us >= 200_000,
            "aged queue reports queue wait, got {}",
            ack.queue_age_us
        );
        assert!(
            ack.span_us < ack.queue_age_us,
            "present span ({}) stays separable from queue age ({})",
            ack.span_us,
            ack.queue_age_us
        );
        // A fresh post reports near-zero queue age on the same core.
        channels.post(border_frame(2, 0, 20, 1001));
        core.step();
        let fresh = channels.take_ack(RenderCarrier::Border).expect("fresh ack");
        assert!(
            fresh.queue_age_us < 200_000,
            "fresh queue age small, got {}",
            fresh.queue_age_us
        );
    }

    #[test]
    fn hide_cancel_show_race_never_replays_stale_show() {
        let channels = Arc::new(RenderChannels::new());
        let mut core = RenderCore::new(FakePresent::new(), Arc::clone(&channels));
        channels.post(preview_frame(1, 0, 2001));
        let (hide_gen, hide_safety) = channels.hide(RenderCarrier::Preview, 2);
        // A pre-hide frame posted again with its old safety is stale: the
        // transport drops it even if re-offered.
        channels.post(DesiredFrame::Preview(PreviewDesired {
            generation: 1,
            safety: 0,
            outer: rect(10, 10, 300, 200),
            color: (0x40, 0x2a, 0x82, 0xda),
            anchor_token: "token-2001".to_owned(),
            anchor_hwnd: 2001,
            lifetime: lifetime_for(2001),
            foreground: Some(lifetime_for(2001)),
        }));
        core.step();
        assert!(core.present().shown.is_empty());
        let ack = channels.take_ack(RenderCarrier::Preview).expect("hide ack");
        assert_eq!(ack.outcome, RenderOutcome::Hidden);
        assert_eq!(ack.generation, hide_gen);
        assert_eq!(ack.safety, hide_safety);
        // A fresh show after the hide presents normally.
        channels.post(preview_frame(3, hide_safety, 2001));
        core.step();
        assert_eq!(core.present().shown.len(), 1);
        let ack = channels.take_ack(RenderCarrier::Preview).expect("show ack");
        assert_eq!(ack.outcome, RenderOutcome::Shown);
    }

    #[test]
    fn stale_target_refuses_narrowly_and_hides() {
        let channels = Arc::new(RenderChannels::new());
        let mut fake = FakePresent::new();
        fake.fresh.insert(1001, dead_facts());
        let mut core = RenderCore::new(fake, Arc::clone(&channels));
        channels.post(border_frame(1, 0, 10, 1001));
        core.step();
        assert!(core.present().shown.is_empty());
        assert_eq!(core.present().hidden, vec![RenderCarrier::Border]);
        let ack = channels
            .take_ack(RenderCarrier::Border)
            .expect("refusal ack");
        assert_eq!(ack.outcome, RenderOutcome::Refused("target-gone"));
        assert_eq!(ack.generation, 1);
    }

    #[test]
    fn recycled_target_foreground_drift_refuses_border_only() {
        let channels = Arc::new(RenderChannels::new());
        let mut fake = FakePresent::new();
        // Same HWND value, but the foreground moved on: recycled identity.
        fake.fresh.insert(
            1001,
            FreshFacts {
                foreground_ok: false,
                ..live_facts()
            },
        );
        let mut core = RenderCore::new(fake, Arc::clone(&channels));
        channels.post(border_frame(1, 0, 10, 1001));
        core.step();
        let ack = channels
            .take_ack(RenderCarrier::Border)
            .expect("refusal ack");
        assert_eq!(ack.outcome, RenderOutcome::Refused("foreground-changed"));
    }

    #[test]
    fn recycled_hwnd_identity_mismatch_refuses_narrowly() {
        // Same HWND value with a dead lifetime: the renderer fence refuses
        // with the recycled-ID reason instead of presenting stale pixels.
        let channels = Arc::new(RenderChannels::new());
        let mut fake = FakePresent::new();
        fake.fresh.insert(
            1001,
            FreshFacts {
                identity_ok: false,
                ..live_facts()
            },
        );
        let mut core = RenderCore::new(fake, Arc::clone(&channels));
        channels.post(border_frame(1, 0, 10, 1001));
        core.step();
        assert!(core.present().shown.is_empty());
        let ack = channels
            .take_ack(RenderCarrier::Border)
            .expect("refusal ack");
        assert_eq!(ack.outcome, RenderOutcome::Refused("identity-changed"));
        assert_eq!(ack.checksum, 0);
    }

    #[test]
    fn ordinary_mover_preview_presents_without_topmost_band() {
        // An ordinary (non-topmost) gesture mover still previews: the mover
        // band was never a preview requirement, only mover liveness plus
        // the lifetime identity.
        let channels = Arc::new(RenderChannels::new());
        let mut fake = FakePresent::new();
        fake.fresh.insert(
            2001,
            FreshFacts {
                band_ok: false,
                ..live_facts()
            },
        );
        let mut core = RenderCore::new(fake, Arc::clone(&channels));
        channels.post(preview_frame(1, 0, 2001));
        core.step();
        assert_eq!(core.present().shown.len(), 1);
        let ack = channels
            .take_ack(RenderCarrier::Preview)
            .expect("applied ack");
        assert_eq!(ack.outcome, RenderOutcome::Shown);
        assert_ne!(ack.checksum, 0);
    }

    #[test]
    fn underlay_uses_fresh_resolved_anchor() {
        let channels = Arc::new(RenderChannels::new());
        let mut core = RenderCore::new(FakePresent::new(), Arc::clone(&channels));
        // FakePresent resolves to the first candidate: the fresh lowest.
        channels.post(underlay_frame(1, 0, 3001, vec![3002, 3001]));
        core.step();
        assert_eq!(core.present().shown_anchors, vec![3002]);
        let ack = channels
            .take_ack(RenderCarrier::Underlay)
            .expect("applied ack");
        assert_eq!(ack.outcome, RenderOutcome::Shown);
    }

    #[test]
    fn underlay_anchor_missing_hides_narrowly() {
        let channels = Arc::new(RenderChannels::new());
        let mut fake = FakePresent::new();
        fake.live_facts = None;
        let mut core = RenderCore::new(fake, Arc::clone(&channels));
        // No candidates and a dead fallback: nothing may present.
        channels.post(underlay_frame(1, 0, 3001, vec![]));
        core.step();
        assert!(core.present().shown.is_empty());
        let ack = channels
            .take_ack(RenderCarrier::Underlay)
            .expect("refusal ack");
        assert_eq!(ack.outcome, RenderOutcome::Refused("anchor-missing"));
    }

    #[test]
    fn fill_checksum_matches_portable_painter_exactly() {
        // Raster equivalence: the core ack checksum equals a direct call to
        // the same premultiplied-alpha painter the native path uses, so the
        // threaded present is pixel-identical to the same-thread present.
        let channels = Arc::new(RenderChannels::new());
        let mut core = RenderCore::new(FakePresent::new(), Arc::clone(&channels));
        channels.post(underlay_frame(1, 0, 3001, vec![3001]));
        core.step();
        let ack = channels
            .take_ack(RenderCarrier::Underlay)
            .expect("applied ack");
        let mut pixels = vec![0u8; 200 * 100 * 4];
        let direct =
            crate::group_underlay::paint_fill_argb(&mut pixels, 200, 100, (0x80, 0x80, 0x80), 0x40);
        assert_eq!(ack.checksum, direct);
        // And the preview shares the same fill contract.
        channels.post(preview_frame(2, 0, 2001));
        core.step();
        let ack = channels
            .take_ack(RenderCarrier::Preview)
            .expect("applied ack");
        let mut pixels = vec![0u8; 300 * 200 * 4];
        let direct =
            crate::group_underlay::paint_fill_argb(&mut pixels, 300, 200, (0x2a, 0x82, 0xda), 0x40);
        assert_eq!(ack.checksum, direct);
    }

    #[test]
    fn border_checksum_covers_ring_predicate_deterministically() {
        // The ring checksum is stable for identical inputs and sensitive to
        // geometry, colour and style: same predicate the native painter
        // evaluates per pixel.
        let channels = Arc::new(RenderChannels::new());
        let mut core = RenderCore::new(FakePresent::new(), Arc::clone(&channels));
        channels.post(border_frame(1, 0, 10, 1001));
        core.step();
        let first = channels
            .take_ack(RenderCarrier::Border)
            .expect("applied ack")
            .checksum;
        channels.post(border_frame(2, 0, 10, 1001));
        core.step();
        let second = channels
            .take_ack(RenderCarrier::Border)
            .expect("applied ack")
            .checksum;
        assert_eq!(first, second);
        channels.post(border_frame(3, 0, 40, 1001));
        core.step();
        let moved_sum = channels
            .take_ack(RenderCarrier::Border)
            .expect("applied ack")
            .checksum;
        // Same size/style/colour at a moved origin paints identical bytes.
        assert_eq!(moved_sum, first);
        // A hollow ring never checksums like a solid fill of the same box.
        let mut fill = vec![0u8; 100 * 80 * 4];
        let fill_sum =
            crate::group_underlay::paint_fill_argb(&mut fill, 100, 80, (0x2a, 0x82, 0xda), 0xff);
        assert_ne!(first, fill_sum);
    }

    #[test]
    fn applied_state_ack_never_reports_desired_as_rendered() {
        // A scripted present failure hides narrowly and reports the failure;
        // the ack never claims Shown for unrendered pixels.
        let channels = Arc::new(RenderChannels::new());
        let mut fake = FakePresent::new();
        fake.fail_shows.insert(
            RenderCarrier::Preview,
            "error: drop-preview present failed".to_owned(),
        );
        let mut core = RenderCore::new(fake, Arc::clone(&channels));
        channels.post(preview_frame(1, 0, 2001));
        core.step();
        let ack = channels
            .take_ack(RenderCarrier::Preview)
            .expect("failure ack");
        assert_eq!(ack.outcome, RenderOutcome::Hidden);
        assert_eq!(ack.checksum, 0);
        assert_eq!(
            ack.last_error.as_deref(),
            Some("error: drop-preview present failed")
        );
    }

    #[test]
    fn overlay_snapshot_publishes_before_first_present() {
        // Creator-side snapshot arms exclusion before any show: publish
        // first, then present, and the owner sees every identity.
        let channels = Arc::new(RenderChannels::new());
        let mut fake = FakePresent::new();
        fake.hwnds[RenderCarrier::Border.index()] = Some(4242);
        fake.hwnds[RenderCarrier::Underlay.index()] = Some(4343);
        fake.hwnds[RenderCarrier::Preview.index()] = Some(4444);
        let core = RenderCore::new(fake, Arc::clone(&channels));
        assert_eq!(channels.overlay_ids(), [None, None, None]);
        core.publish_all();
        assert_eq!(channels.overlay_ids(), [Some(4242), Some(4343), Some(4444)]);
    }

    #[test]
    fn self_heal_replacement_syncs_without_stale_overwrite() {
        // A creator-side self-heal (externally destroyed surface recreated
        // with a new HWND) swaps the published identity against the expected
        // old value; a stale concurrent snapshot never wins over it.
        let channels = Arc::new(RenderChannels::new());
        let mut fake = FakePresent::new();
        fake.hwnds[RenderCarrier::Border.index()] = Some(4242);
        let mut core = RenderCore::new(fake, Arc::clone(&channels));
        core.publish_all();
        assert_eq!(channels.overlay_ids()[0], Some(4242));
        core.present_mut().hwnds[RenderCarrier::Border.index()] = Some(4545);
        channels.post(border_frame(1, 0, 10, 1001));
        core.step();
        assert_eq!(channels.overlay_ids()[0], Some(4545));
        // A stale republish of the dead value cannot resurrect it.
        channels.publish_overlay_snapshot([Some(4242), None, None]);
        assert_eq!(channels.overlay_ids()[0], Some(4545));
    }

    #[test]
    fn hide_failure_reports_unknown_then_recovers_without_new_transition() {
        // An unverified creator-side hide retains the pending watermark
        // (never advances safety) and reports `Unknown`, never a
        // known-false visibility and never a satisfied hide wait. Without
        // any new owner transition, the bounded retry verifies the hide
        // and the same hide then acknowledges proven `Hidden`.
        let channels = Arc::new(RenderChannels::new());
        let mut fake = FakePresent::new();
        fake.fail_hides.insert(RenderCarrier::Preview);
        let mut core = RenderCore::new(fake, Arc::clone(&channels));
        let (generation, safety) = channels.hide(RenderCarrier::Preview, 2);
        core.step();
        let ack = channels
            .take_ack(RenderCarrier::Preview)
            .expect("hide-failure ack");
        assert_eq!(ack.outcome, RenderOutcome::Unknown("hide-failed"));
        assert!(!ack.hidden_proven);
        assert!(ack.last_error.is_some());
        assert_eq!(ack.applied, None);
        let probe = Arc::clone(&channels);
        assert!(!probe.wait_ack(
            RenderCarrier::Preview,
            generation,
            safety,
            std::time::Duration::from_millis(20)
        ));
        // No new owner transition: the retry verifies and proves Hidden.
        std::thread::sleep(std::time::Duration::from_millis(60));
        core.step();
        assert!(probe.wait_ack(
            RenderCarrier::Preview,
            generation,
            safety,
            std::time::Duration::from_millis(50)
        ));
        let ack = channels
            .take_ack(RenderCarrier::Preview)
            .expect("recovery ack");
        assert_eq!(ack.outcome, RenderOutcome::Hidden);
        assert!(ack.hidden_proven);
        assert_eq!((ack.generation, ack.safety), (generation, safety));
    }

    #[test]
    fn idle_core_arms_no_timer() {
        // Fully idle: no hide pending anywhere, so the pump wait plan is
        // indefinite (work wake and messages still wake; nothing polls).
        let channels = Arc::new(RenderChannels::new());
        let core = RenderCore::new(FakePresent::new(), Arc::clone(&channels));
        assert_eq!(core.next_timer(), None);
        // A presented frame with no hide is equally idle: applied work
        // needs no timer.
        channels.post(border_frame(1, 0, 10, 1001));
        let mut core = RenderCore::new(FakePresent::new(), Arc::clone(&channels));
        core.step();
        assert!(channels.take_ack(RenderCarrier::Border).is_some());
        assert_eq!(core.next_timer(), None);
    }

    #[test]
    fn pending_hide_arms_immediate_timer() {
        // A hide with no backoff scheduled is due immediately: the wait
        // plan breaks at once instead of idling.
        let channels = Arc::new(RenderChannels::new());
        let core = RenderCore::new(FakePresent::new(), Arc::clone(&channels));
        channels.post(border_frame(1, 0, 10, 1001));
        let (_generation, _safety) = channels.hide(RenderCarrier::Border, 2);
        let due = core.next_timer().expect("hide pending");
        assert!(
            due.saturating_duration_since(Instant::now()) < Duration::from_secs(1),
            "pending hide is due immediately, got {due:?}"
        );
    }

    #[test]
    fn failed_hide_backs_off_timer() {
        // An unverified hide arms exactly the bounded backoff: the wait
        // plan sleeps until the retry is due, then success clears the
        // timer back to idle.
        let channels = Arc::new(RenderChannels::new());
        let mut fake = FakePresent::new();
        fake.fail_hides.insert(RenderCarrier::Border);
        let mut core = RenderCore::new(fake, Arc::clone(&channels));
        channels.post(border_frame(1, 0, 10, 1001));
        channels.hide(RenderCarrier::Border, 2);
        let start = Instant::now();
        core.step();
        let due = core.next_timer().expect("retry pending");
        assert!(due >= start, "backoff is in the future");
        assert!(
            due <= start + hide_backoff(0) + Duration::from_millis(500),
            "backoff is bounded, got {due:?}"
        );
        // Recovery (fail_hides consumed once) clears the timer to idle
        // without any new owner transition.
        std::thread::sleep(hide_backoff(0) + Duration::from_millis(20));
        core.step();
        let ack = channels
            .take_ack(RenderCarrier::Border)
            .expect("recovery ack");
        assert_eq!(ack.outcome, RenderOutcome::Hidden);
        assert_eq!(core.next_timer(), None);
    }

    #[test]
    fn present_failure_retries_on_next_post_with_exact_checksum() {
        // A present failure hides with the bounded renderer error (never a
        // `Shown` claim over unrendered pixels); reposting the same content
        // recovers with the exact portable-painter checksum.
        let channels = Arc::new(RenderChannels::new());
        let mut fake = FakePresent::new();
        fake.fail_shows.insert(
            RenderCarrier::Underlay,
            "error: group-underlay present failed".to_owned(),
        );
        let mut core = RenderCore::new(fake, Arc::clone(&channels));
        channels.post(underlay_frame(1, 0, 3001, vec![3001]));
        core.step();
        let ack = channels
            .take_ack(RenderCarrier::Underlay)
            .expect("failure ack");
        assert_eq!(ack.outcome, RenderOutcome::Hidden);
        assert_eq!(ack.checksum, 0);
        assert_eq!(
            ack.last_error.as_deref(),
            Some("error: group-underlay present failed")
        );
        // Same content reposted recovers: the ack checksum equals a direct
        // call to the shared premultiplied-alpha painter.
        channels.post(underlay_frame(2, 0, 3001, vec![3001]));
        core.step();
        let ack = channels
            .take_ack(RenderCarrier::Underlay)
            .expect("recovery ack");
        assert_eq!(ack.outcome, RenderOutcome::Shown);
        let mut pixels = vec![0u8; 200 * 100 * 4];
        let direct =
            crate::group_underlay::paint_fill_argb(&mut pixels, 200, 100, (0x80, 0x80, 0x80), 0x40);
        assert_eq!(ack.checksum, direct);
    }

    #[test]
    fn underlay_own_border_anchor_needs_no_foreign_tag() {
        // The renderer-owned border anchors the underlay without any foreign
        // member lifetime: the paired identity is ownership (visible,
        // same-group token), never the owner fallback pair. A frame whose
        // foreign candidates are all dead still presents over the own
        // border instead of refusing on the fallback mismatch.
        let channels = Arc::new(RenderChannels::new());
        let mut fake = FakePresent::new();
        fake.live_facts = None;
        fake.hwnds[RenderCarrier::Border.index()] = Some(4242);
        fake.visible[RenderCarrier::Border.index()] = true;
        fake.carrier_tokens[RenderCarrier::Border.index()] = Some("token-3001".to_owned());
        fake.own_border_anchor = true;
        fake.own_facts = Some(live_facts());
        let mut core = RenderCore::new(fake, Arc::clone(&channels));
        channels.post(underlay_frame(1, 0, 3001, vec![]));
        core.step();
        assert_eq!(core.present().shown_anchors, vec![4242]);
        let ack = channels
            .take_ack(RenderCarrier::Underlay)
            .expect("applied ack");
        assert_eq!(ack.outcome, RenderOutcome::Shown);
        assert!(ack.identity_proven);
    }

    #[test]
    fn underlay_follows_resolved_anchor_band() {
        // The placement band follows the resolved anchor, like-for-like
        // with the same-thread path: the recorded show band is the
        // anchor's band even when the frame fallback band differs.
        let channels = Arc::new(RenderChannels::new());
        let mut fake = FakePresent::new();
        fake.bands.insert(3002, true);
        let mut core = RenderCore::new(fake, Arc::clone(&channels));
        channels.post(underlay_frame(1, 0, 3001, vec![3002, 3001]));
        core.step();
        assert_eq!(core.present().shown_anchors, vec![3002]);
        assert_eq!(core.present().shown_bands, vec![true]);
        let ack = channels
            .take_ack(RenderCarrier::Underlay)
            .expect("applied ack");
        assert_eq!(ack.outcome, RenderOutcome::Shown);
    }

    #[test]
    fn themed_border_ack_carries_resolved_accent_not_fallback() {
        // Renderer-owned accent: a themed frame with a live accent
        // acknowledges the resolved accent colour and checksum, never the
        // owner fallback the note still carries.
        let channels = Arc::new(RenderChannels::new());
        let mut fake = FakePresent::new();
        fake.accent = Some((0xaa, 0xbb, 0xcc));
        let mut core = RenderCore::new(fake, Arc::clone(&channels));
        let mut frame = border_frame(1, 0, 10, 1001);
        let DesiredFrame::Border(ref mut inner) = frame else {
            panic!("border frame");
        };
        inner.use_theme = true;
        channels.post(frame);
        core.step();
        let ack = channels
            .take_ack(RenderCarrier::Border)
            .expect("applied ack");
        assert_eq!(ack.outcome, RenderOutcome::Shown);
        let applied = ack.applied.expect("applied content");
        assert_eq!(applied.color_hex, "#aabbcc");
        assert_ne!(applied.color_hex, "#2a82da");
    }

    #[test]
    fn foreground_drift_refuses_same_content_everywhere() {
        // Same content reposted after a foreground switch refuses on every
        // carrier: the fence compares foreground lifetime identity, so a
        // late switch (game, suspend) hides instead of presenting.
        let channels = Arc::new(RenderChannels::new());
        let mut fake = FakePresent::new();
        fake.foreground = Some(lifetime_for(1001));
        let mut core = RenderCore::new(fake, Arc::clone(&channels));
        channels.post(border_frame(1, 0, 10, 1001));
        core.step();
        assert_eq!(
            channels
                .take_ack(RenderCarrier::Border)
                .expect("ack")
                .outcome,
            RenderOutcome::Shown
        );
        // Same content, new foreground identity: refuse, no present.
        core.present_mut().foreground = Some(lifetime_for(9000));
        channels.post(border_frame(2, 0, 10, 1001));
        core.step();
        let ack = channels
            .take_ack(RenderCarrier::Border)
            .expect("refusal ack");
        assert_eq!(ack.outcome, RenderOutcome::Refused("foreground-changed"));
        assert!(core.present().shown.len() == 1);
        // Preview fences the same switch with its own narrow reason.
        core.present_mut().foreground = Some(lifetime_for(2001));
        channels.post(preview_frame(3, 0, 2001));
        core.step();
        assert_eq!(
            channels
                .take_ack(RenderCarrier::Preview)
                .expect("ack")
                .outcome,
            RenderOutcome::Shown
        );
        core.present_mut().foreground = Some(lifetime_for(9000));
        channels.post(preview_frame(4, 0, 2001));
        core.step();
        let ack = channels
            .take_ack(RenderCarrier::Preview)
            .expect("refusal ack");
        assert_eq!(
            ack.outcome,
            RenderOutcome::Refused("mover-foreground-changed")
        );
    }

    #[test]
    fn absent_foreground_refuses_preview_narrowly() {
        // Unreadable post-time foreground (`None`) refuses narrowly per the
        // binding contract: no unprovable stale show, while readable
        // unchanged foregrounds keep ordinary unfocused-hold behavior.
        // Ordinary drag still presents when the foreground is readable and
        // unchanged (mover need not be focused).
        let channels = Arc::new(RenderChannels::new());
        let mut fake = FakePresent::new();
        fake.foreground = Some(lifetime_for(2001));
        let mut core = RenderCore::new(fake, Arc::clone(&channels));
        channels.post(preview_frame(1, 0, 2001));
        core.step();
        assert_eq!(
            channels
                .take_ack(RenderCarrier::Preview)
                .expect("ack")
                .outcome,
            RenderOutcome::Shown
        );
        // Same mover with an unreadable foreground posts `None`: refuse with
        // the narrow foreground reason, never a present.
        channels.post(DesiredFrame::Preview(PreviewDesired {
            generation: 2,
            safety: 0,
            outer: rect(10, 10, 300, 200),
            color: (0x40, 0x2a, 0x82, 0xda),
            anchor_token: "token-2001".to_owned(),
            anchor_hwnd: 2001,
            lifetime: lifetime_for(2001),
            foreground: None,
        }));
        core.step();
        assert!(core.present().shown.len() == 1, "refusal shows nothing new");
        let ack = channels
            .take_ack(RenderCarrier::Preview)
            .expect("refusal ack");
        assert_eq!(
            ack.outcome,
            RenderOutcome::Refused("mover-foreground-changed")
        );
        assert_eq!(ack.checksum, 0);
    }

    #[test]
    fn fullscreen_drift_refuses_same_content() {
        // Same border content after the target goes captionless-fullscreen
        // refuses narrowly instead of presenting over the fullscreen frame.
        let channels = Arc::new(RenderChannels::new());
        let fake = FakePresent::new();
        let mut core = RenderCore::new(fake, Arc::clone(&channels));
        channels.post(border_frame(1, 0, 10, 1001));
        core.step();
        assert_eq!(
            channels
                .take_ack(RenderCarrier::Border)
                .expect("ack")
                .outcome,
            RenderOutcome::Shown
        );
        core.present_mut().fullscreen_hwnds.insert(1001);
        channels.post(border_frame(2, 0, 10, 1001));
        core.step();
        let ack = channels
            .take_ack(RenderCarrier::Border)
            .expect("refusal ack");
        assert_eq!(ack.outcome, RenderOutcome::Refused("target-fullscreen"));
    }

    #[test]
    fn ensure_failure_refuses_pending_work_narrowly() {
        // A lost creator surface refuses the pending frame as
        // `carrier-lost` with bounded detail; the next successful ensure
        // presents the reposted frame normally.
        let channels = Arc::new(RenderChannels::new());
        let mut fake = FakePresent::new();
        fake.fail_ensure = Some("error: CreateWindow failed".to_owned());
        let mut core = RenderCore::new(fake, Arc::clone(&channels));
        channels.post(border_frame(1, 0, 10, 1001));
        core.step();
        let ack = channels
            .take_ack(RenderCarrier::Border)
            .expect("refusal ack");
        assert_eq!(ack.outcome, RenderOutcome::Refused("carrier-lost"));
        assert_eq!(
            ack.last_error.as_deref(),
            Some("error: CreateWindow failed")
        );
        assert!(core.present().ensure_calls >= 1);
        core.present_mut().fail_ensure = None;
        channels.post(border_frame(2, 0, 10, 1001));
        core.step();
        assert_eq!(
            channels
                .take_ack(RenderCarrier::Border)
                .expect("ack")
                .outcome,
            RenderOutcome::Shown
        );
    }

    #[test]
    fn applied_content_is_fresh_from_presented_frame() {
        // The ack carries bounded content from the presented frame itself:
        // target, geometry, presented colour, style, and member count ride
        // the ack, so inspection never needs a possibly aged note pairing.
        let channels = Arc::new(RenderChannels::new());
        let mut core = RenderCore::new(FakePresent::new(), Arc::clone(&channels));
        channels.post(underlay_frame(7, 3, 3001, vec![3002, 3001]));
        core.step();
        let ack = channels
            .take_ack(RenderCarrier::Underlay)
            .expect("applied ack");
        assert_eq!((ack.generation, ack.safety), (7, 3));
        let applied = ack.applied.expect("applied content");
        assert_eq!(applied.target, "token-3001");
        assert_eq!(applied.outer, [0, 0, 200, 100]);
        assert_eq!(applied.members, 2);
        assert!(!ack.hidden_proven);
        assert!(ack.identity_proven);
    }

    #[test]
    fn shutdown_drive_destroys_and_reports() {
        let channels = Arc::new(RenderChannels::new());
        let mut core = RenderCore::new(FakePresent::new(), Arc::clone(&channels));
        channels.request_stop();
        assert!(channels.stop_requested());
        assert!(core.present_mut().destroy_all());
        assert_eq!(core.present().destroys, 1);
        // A failing destroy is reported, never claimed.
        core.present_mut().destroy_ok = false;
        assert!(!core.present_mut().destroy_all());
    }
}
