#![forbid(unsafe_code)]
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
