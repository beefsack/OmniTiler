#![forbid(unsafe_code)]
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

fn underlay_frame(generation: u64, safety: u64, anchor: u64, candidates: Vec<u64>) -> DesiredFrame {
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
