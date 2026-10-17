#![forbid(unsafe_code)]
use super::*;

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
pub(super) fn hide_backoff(attempts: u32) -> Duration {
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
