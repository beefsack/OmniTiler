#![forbid(unsafe_code)]
use super::*;

fn hints(min_w: Option<i32>, min_h: Option<i32>) -> tiler_core::size_hints::WindowSizeHints {
    tiler_core::size_hints::WindowSizeHints {
        min_w,
        min_h,
        max_w: None,
        max_h: None,
    }
}

fn no_insets() -> FrameInsets {
    FrameInsets {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    }
}

#[test]
fn hints_from_outer_carries_both_sides_with_shared_inset_conversion() {
    // Fresh outer tracks convert with the same frame-inset subtraction:
    // a 640x480 client with an 8px frame on every side carries visible
    // 624x464 on both sides.
    let insets = FrameInsets {
        left: 8,
        top: 8,
        right: 8,
        bottom: 8,
    };
    let got = hints_from_outer(Some((640, 480)), Some((640, 480)), insets);
    assert_eq!(
        got,
        tiler_core::size_hints::WindowSizeHints {
            min_w: Some(624),
            min_h: Some(464),
            max_w: Some(624),
            max_h: Some(464),
        }
    );
    // Minimum-only queries keep the historical shape (maximums absent).
    assert_eq!(
        hints_from_outer(Some((640, 480)), None, no_insets()),
        tiler_core::size_hints::WindowSizeHints {
            min_w: Some(640),
            min_h: Some(480),
            max_w: None,
            max_h: None,
        }
    );
    assert_eq!(
        min_hints_from_outer(640, 480, no_insets()),
        hints_from_outer(Some((640, 480)), None, no_insets())
    );
    // Maximum-only queries carry no minimum.
    assert_eq!(
        hints_from_outer(None, Some((640, 480)), no_insets()),
        tiler_core::size_hints::WindowSizeHints {
            min_w: None,
            min_h: None,
            max_w: Some(640),
            max_h: Some(480),
        }
    );
    // Each axis converts independently: an invalid minimum axis keeps a
    // valid maximum axis and vice versa. Explicit zeros ride as unset
    // (G-05); negatives, sentinels, and absurd values map to unknown.
    let partial = hints_from_outer(Some((640, 0)), Some((640, 480)), no_insets());
    assert_eq!(
        (partial.min_w, partial.min_h, partial.max_w, partial.max_h),
        (Some(640), Some(0), Some(640), Some(480))
    );
    let negative = hints_from_outer(Some((-1, 480)), Some((640, 480)), no_insets());
    assert_eq!(
        (
            negative.min_w,
            negative.min_h,
            negative.max_w,
            negative.max_h
        ),
        (None, Some(480), Some(640), Some(480))
    );
    assert!(hints_from_outer(Some((-1, -2)), Some((-3, -4)), no_insets()).is_empty());
    // A bound that vanishes inside its own frame maps to unknown on that
    // axis only; the usable sibling axis still converts.
    let deep = FrameInsets {
        left: 400,
        top: 0,
        right: 400,
        bottom: 0,
    };
    let got = hints_from_outer(Some((640, 480)), Some((640, 480)), deep);
    assert_eq!(
        (got.min_w, got.min_h, got.max_w, got.max_h),
        (None, Some(480), None, Some(480))
    );
    // Minimum projection behavior is unchanged: explicit zeros behave
    // exactly like absent bounds outside the fixed-size predicate.
    let zero_min = hints_from_outer(Some((0, 480)), None, no_insets());
    assert_eq!(
        overconstrained_effective(
            Rect {
                x: 8,
                y: 8,
                w: 400,
                h: 300,
            },
            zero_min
        ),
        Rect {
            x: 8,
            y: 8,
            w: 400,
            h: 480
        }
    );
}

#[test]
fn normalize_outer_bound_keeps_zero_unset_and_rejects_unusable() {
    assert_eq!(normalize_outer_bound(0), Some(0));
    assert_eq!(normalize_outer_bound(1), Some(1));
    assert_eq!(
        normalize_outer_bound(tiler_core::bounds::GEOMETRY_BOUND),
        Some(tiler_core::bounds::GEOMETRY_BOUND)
    );
    // Negative, sentinel, and absurd values are unknown, never a floor.
    assert_eq!(normalize_outer_bound(-1), None);
    assert_eq!(normalize_outer_bound(i32::MIN), None);
    assert_eq!(normalize_outer_bound(i32::MAX), None);
    assert_eq!(
        normalize_outer_bound(tiler_core::bounds::GEOMETRY_BOUND + 1),
        None
    );
    // Pairs validate per axis: one bad axis never discards its sibling.
    assert_eq!(normalize_min_track(640, 0), (Some(640), Some(0)));
    assert_eq!(normalize_min_track(-5, 480), (None, Some(480)));
    assert_eq!(normalize_min_track(0, 0), (Some(0), Some(0)));
    assert_eq!(normalize_max_track(i32::MAX, 480), (None, Some(480)));
}

#[test]
fn converted_hints_classify_like_outer_tracks_under_both_predicates() {
    // Production-connected parity (no speculative helper): converted
    // visible hints classify exactly as the shared predicate decides on
    // the same vectors, because frame insets shift both sides of an axis
    // equally and preserve equality. Covers the G-05 partial-zero table,
    // guards, and one-axis cases through the real conversion path.
    use tiler_core::size_hints::FixedSizePredicate::{BothAxes, EitherAxis};
    use tiler_core::size_hints::is_fixed_size_with;
    let cases = [
        (Some((640, 480)), Some((640, 480)), true, true),
        (Some((640, 480)), None, false, false),
        (Some((640, 0)), Some((640, 0)), false, true),
        (Some((0, 480)), Some((0, 480)), false, true),
        (Some((0, 0)), Some((0, 0)), false, false),
        (None, None, false, false),
        (Some((640, 480)), Some((i32::MAX, i32::MAX)), false, false),
        (Some((640, 480)), Some((640, 960)), false, true),
        (Some((-1, 480)), Some((-1, 480)), false, false),
    ];
    for (min_outer, max_outer, both, either) in cases {
        for insets in [
            FrameInsets {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            FrameInsets {
                left: 8,
                top: 8,
                right: 8,
                bottom: 8,
            },
        ] {
            let hints = hints_from_outer(min_outer, max_outer, insets);
            assert_eq!(
                is_fixed_size_with(hints, BothAxes),
                both,
                "both-axes {min_outer:?}/{max_outer:?} insets {insets:?}"
            );
            assert_eq!(
                is_fixed_size_with(hints, EitherAxis),
                either,
                "either-axis {min_outer:?}/{max_outer:?} insets {insets:?}"
            );
        }
    }
}

#[test]
fn overconstrained_effective_keeps_origin_and_raises_to_minimum() {
    // Planned tile origin is kept; each extent rises to the known native
    // minimum. Real5 shape: a small proportional tile for Paint clamped
    // to its 864x617-class minimum stays at the tile origin.
    let planned = Rect {
        x: 8,
        y: 8,
        w: 400,
        h: 300,
    };
    let got = overconstrained_effective(planned, hints(Some(864), Some(617)));
    assert_eq!(
        got,
        Rect {
            x: 8,
            y: 8,
            w: 864,
            h: 617
        }
    );
    // Satisfiable extents pass through byte-identical.
    assert_eq!(
        overconstrained_effective(planned, hints(Some(100), Some(100))),
        planned
    );
    // Unknown minimums (no hint) keep the planned rect.
    assert_eq!(
        overconstrained_effective(planned, tiler_core::size_hints::WindowSizeHints::none()),
        planned
    );
    // Non-meaningful hints (zero/absurd) behave as absent.
    assert_eq!(
        overconstrained_effective(planned, hints(Some(0), Some(-5))),
        planned
    );
    // Only the violating axis grows.
    let wide = overconstrained_effective(planned, hints(Some(500), None));
    assert_eq!((wide.x, wide.y, wide.w, wide.h), (8, 8, 500, 300));
}

#[test]
fn overconstrained_target_is_refused_stable() {
    // Minimum clamping must not fight every tick: the identical
    // (effective, observed) clamp pair suppresses, while any genuine
    // change re-arms immediately.
    let planned = Rect {
        x: 8,
        y: 8,
        w: 400,
        h: 300,
    };
    let effective = overconstrained_effective(planned, hints(Some(864), Some(617)));
    let observed = effective;
    let mut tracker = RefusedTracker::default();
    let now = std::time::Instant::now();
    // Exact match clears: no skip, no clamp lane.
    tracker.note_match("w1");
    assert!(!tracker.should_skip("w1", &effective, &observed, now));
    // Successful write that reads back at the effective target is a
    // match, not a clamp.
    assert_eq!(
        readback_outcome(true, &effective, &observed),
        ReadbackOutcome::Match
    );
    // App-held larger size pins the identical pair; the same pair then
    // suppresses instead of retrying every tick.
    let held = Rect {
        x: 8,
        y: 8,
        w: 900,
        h: 700,
    };
    assert_eq!(
        readback_outcome(true, &effective, &held),
        ReadbackOutcome::Clamp
    );
    tracker.note_clamp("w1", &effective, &held);
    assert!(tracker.should_skip("w1", &effective, &held, now));
    // A new planned tile or a new observation re-arms.
    let moved = Rect {
        x: 100,
        ..effective
    };
    assert!(!tracker.should_skip("w1", &moved, &held, now));
    assert!(!tracker.should_skip("w1", &effective, &planned, now));
}

#[test]
fn transfer_readback_entry_joins_dpi_outer_insets_without_identity() {
    // Existing desired/readback/matched fields stay byte-identical; the
    // new DPI/outer/inset fields join from caller-supplied live reads.
    // A 96 -> 120 write reports `dpi_changed`; unreadable rides null.
    let effective = Rect {
        x: 1284,
        y: 8,
        w: 1268,
        h: 1364,
    };
    let readback = Rect {
        x: 1285,
        y: 8,
        w: 1587,
        h: 1381,
    };
    let outer = Rect {
        x: 1277,
        y: 8,
        w: 1282,
        h: 1371,
    };
    let outer_back = Rect {
        x: 1278,
        y: 8,
        w: 1601,
        h: 1388,
    };
    let entry = transfer_readback_entry(
        "w22",
        &effective,
        Some(&readback),
        Some(96),
        Some(120),
        Some(&outer),
        Some(&outer_back),
        Some([7, 0, 7, 7]),
    );
    assert_eq!(entry["window"], serde_json::json!("w22"));
    assert_eq!(entry["desired"], serde_json::json!([1284, 8, 1268, 1364]));
    assert_eq!(entry["readback"], serde_json::json!([1285, 8, 1587, 1381]));
    assert_eq!(entry["matched"], serde_json::json!(false));
    assert_eq!(entry["dpi_write"], serde_json::json!(96));
    assert_eq!(entry["dpi_readback"], serde_json::json!(120));
    assert_eq!(entry["dpi_changed"], serde_json::json!(true));
    assert_eq!(entry["outer"], serde_json::json!([1277, 8, 1282, 1371]));
    assert_eq!(
        entry["outer_readback"],
        serde_json::json!([1278, 8, 1601, 1388])
    );
    assert_eq!(entry["insets_readback"], serde_json::json!([7, 0, 7, 7]));
    // Stable DPI on an exact arrival: no change flag, exact match.
    let stable = transfer_readback_entry(
        "w21",
        &effective,
        Some(&effective),
        Some(96),
        Some(96),
        Some(&outer),
        Some(&outer),
        Some([7, 0, 7, 7]),
    );
    assert_eq!(stable["matched"], serde_json::json!(true));
    assert_eq!(stable["dpi_changed"], serde_json::json!(false));
    // Unreadable edges ride null; a missing readback never matches.
    let unknown = transfer_readback_entry("w23", &effective, None, None, None, None, None, None);
    assert_eq!(unknown["matched"], serde_json::json!(false));
    assert_eq!(unknown["dpi_changed"], serde_json::json!(false));
    assert!(unknown["dpi_write"].is_null());
    assert!(unknown["readback"].is_null());
    assert!(unknown["outer"].is_null());
    // Opaque token plus integers only: no HWNDs, pids, paths, titles.
    let serialized = serde_json::to_string(&unknown).unwrap_or_default();
    assert!(!serialized.contains("hwnd"));
    assert!(!serialized.contains("pid"));
}

#[test]
fn dpi_transition_reouter_corrects_under_fresh_frame() {
    // Producer-shaped: the tick9 write aimed outer [3,8,2554,1369] at the
    // intended [8,8,2544,1364] under 96-DPI insets [5,0,5,5], then DPI
    // flipped 96 -> 120. The fresh 120-DPI frame [7,0,7,7] remaps the
    // same intended rectangle to [1,8,2558,1371]: exactly the outer that
    // later arrived exactly once the DPI basis was stable.
    let effective = Rect {
        x: 8,
        y: 8,
        w: 2544,
        h: 1364,
    };
    let fresh = FrameInsets {
        left: 7,
        top: 0,
        right: 7,
        bottom: 7,
    };
    assert_eq!(
        dpi_transition_reouter(Some(96), Some(120), &effective, &fresh),
        Some(Rect {
            x: 1,
            y: 8,
            w: 2558,
            h: 1371
        })
    );
    // Firefox's verified right-to-left transfer used eight-pixel insets.
    assert_eq!(
        dpi_transition_reouter(
            Some(96),
            Some(120),
            &Rect {
                x: 1752,
                y: 8,
                w: 800,
                h: 1364,
            },
            &FrameInsets {
                left: 8,
                top: 0,
                right: 8,
                bottom: 8,
            },
        ),
        Some(Rect {
            x: 1744,
            y: 8,
            w: 816,
            h: 1372,
        })
    );
    // A recompute that equals the attempted outer still reasserts: the
    // first write ran under invalidated state and producer scaling
    // exceeds inset deltas, so equal geometry never proves arrival.
    let stale_same = FrameInsets {
        left: 5,
        top: 0,
        right: 5,
        bottom: 5,
    };
    assert_eq!(
        dpi_transition_reouter(Some(96), Some(120), &effective, &stale_same),
        Some(Rect {
            x: 3,
            y: 8,
            w: 2554,
            h: 1369
        })
    );
    // Stable DPI keeps the existing clamp lane: no correction, so a
    // genuine app-held size still suppresses instead of rewriting.
    assert_eq!(
        dpi_transition_reouter(Some(120), Some(120), &effective, &fresh),
        None
    );
    // Unreadable DPI on either side never guesses: no correction, no
    // false arrival; the tick readback classifies through existing lanes.
    assert_eq!(
        dpi_transition_reouter(None, Some(120), &effective, &fresh),
        None
    );
    assert_eq!(
        dpi_transition_reouter(Some(96), None, &effective, &fresh),
        None
    );
    assert_eq!(dpi_transition_reouter(None, None, &effective, &fresh), None);
    // Unmappable frame under the new state refuses: no correction.
    let huge = Rect {
        x: 0,
        y: 0,
        w: i32::MAX,
        h: i32::MAX,
    };
    assert_eq!(
        dpi_transition_reouter(Some(96), Some(120), &huge, &fresh),
        None
    );
}

#[test]
fn dpi_readback_decision_repeats_rounds_without_false_clamp() {
    // Producer-shaped round trip: a 96-DPI placement reads back mismatched
    // under live 120. The decision is pending (invalidated, never a
    // clamp); the fresh-inset rewrite maps the same intended rectangle
    // through the 120-DPI frame, and the stable re-read arrives exactly.
    let effective = Rect {
        x: 8,
        y: 8,
        w: 2544,
        h: 1364,
    };
    assert_eq!(
        dpi_readback_decision(true, false, Some(96), Some(120)),
        ReadbackOutcome::Pending
    );
    let fresh_120 = FrameInsets {
        left: 7,
        top: 0,
        right: 7,
        bottom: 7,
    };
    assert_eq!(
        dpi_transition_reouter(Some(96), Some(120), &effective, &fresh_120),
        Some(Rect {
            x: 1,
            y: 8,
            w: 2558,
            h: 1371
        })
    );
    assert_eq!(
        dpi_readback_decision(true, true, Some(120), Some(120)),
        ReadbackOutcome::Match
    );
    // Reverse direction is symmetric: a 120-DPI placement invalidated by
    // a flip to 96 stays pending, and the fresh 96-DPI frame remaps the
    // same intended rectangle before the stable re-read arrives exactly.
    assert_eq!(
        dpi_readback_decision(true, false, Some(120), Some(96)),
        ReadbackOutcome::Pending
    );
    let fresh_96 = FrameInsets {
        left: 5,
        top: 0,
        right: 5,
        bottom: 5,
    };
    assert_eq!(
        dpi_transition_reouter(Some(120), Some(96), &effective, &fresh_96),
        Some(Rect {
            x: 3,
            y: 8,
            w: 2554,
            h: 1369
        })
    );
    assert_eq!(
        dpi_readback_decision(true, true, Some(96), Some(96)),
        ReadbackOutcome::Match
    );
    // Exact arrival after a flip needs no rewrite: match wins over the DPI
    // delta even though the bases differ.
    assert_eq!(
        dpi_readback_decision(true, true, Some(96), Some(120)),
        ReadbackOutcome::Match
    );
    // Stable DPI keeps the existing clamp lane: a genuine app-held size
    // still pins instead of retrying every tick.
    assert_eq!(
        dpi_readback_decision(true, false, Some(120), Some(120)),
        ReadbackOutcome::Clamp
    );
    // A failed or refused correction never pins a clamp and never reports
    // success: unwritten mismatches stay pending with or without a DPI
    // delta, and unreadable DPI never guesses a transition.
    assert_eq!(
        dpi_readback_decision(false, false, Some(120), Some(96)),
        ReadbackOutcome::Pending
    );
    assert_eq!(
        dpi_readback_decision(false, false, Some(120), Some(120)),
        ReadbackOutcome::Pending
    );
    assert_eq!(
        dpi_readback_decision(true, false, None, Some(120)),
        ReadbackOutcome::Clamp
    );
    assert_eq!(
        dpi_readback_decision(true, false, Some(120), None),
        ReadbackOutcome::Clamp
    );
    // Changed-DPI second edge is not an arbitrary clamp: a corrected write
    // whose basis no longer matches live DPI stays pending for the next
    // tick instead of pinning.
    assert_eq!(
        dpi_readback_decision(true, false, Some(120), Some(96)),
        ReadbackOutcome::Pending
    );
}

#[test]
fn drop_fence_is_source_bounds_only() {
    // Inside (including origin edge, exclusive far edge).
    assert!(drop_point_in_domain(0, 0, 100, 100, 0, 0));
    assert!(drop_point_in_domain(0, 0, 100, 100, 99, 99));
    // Outside: cross-output and off-work-area refuse alike.
    assert!(!drop_point_in_domain(0, 0, 100, 100, 100, 50));
    assert!(!drop_point_in_domain(0, 0, 100, 100, -1, 50));
    assert!(!drop_point_in_domain(0, 0, 100, 100, 50, 100));
    assert!(!drop_point_in_domain(2560, 0, 100, 100, 50, 50));
    // Degenerate bounds never admit.
    assert!(!drop_point_in_domain(0, 0, 0, 100, 0, 50));
    assert!(!drop_point_in_domain(0, 0, 100, 0, 50, 0));
}

#[test]
fn unfloat_tick275_target_match_with_neighbor_one_pixel_pending() {
    // Source-bound replay of the tick-275 float/unfloat tick (unfloat of
    // w40 with a 13-entry plan, 4 writes, 11 readback mismatches): the
    // unfloat target read back byte-identical while rewritten neighbors
    // diverged by one native pixel and untouched neighbors stayed on stale
    // plan frames. The tick-level mismatch count sums the clamp and pending
    // lanes, so `unfloat-mismatch` does not imply the target mismatched.
    let target = Rect {
        x: 64,
        y: 1051,
        w: 560,
        h: 265,
    };
    assert_eq!(
        readback_outcome(true, &target, &target),
        ReadbackOutcome::Match
    );
    // Rewritten neighbors with one-pixel native divergence (clamp lane).
    let w20 = Rect {
        x: 64,
        y: 64,
        w: 1184,
        h: 594,
    };
    let w20_back = Rect {
        x: 64,
        y: 64,
        w: 1185,
        h: 594,
    };
    assert_eq!(
        readback_outcome(true, &w20, &w20_back),
        ReadbackOutcome::Clamp
    );
    let w102 = Rect {
        x: 64,
        y: 722,
        w: 560,
        h: 265,
    };
    let w102_back = Rect {
        x: 64,
        y: 723,
        w: 560,
        h: 265,
    };
    assert_eq!(
        readback_outcome(true, &w102, &w102_back),
        ReadbackOutcome::Clamp
    );
    // Untouched neighbors (no write behind the delta) stay pending, never
    // clamp -- including a one-pixel delta and a stale-frame delta.
    let w28 = Rect {
        x: 1832,
        y: 1051,
        w: 118,
        h: 265,
    };
    let w28_back = Rect {
        x: 1832,
        y: 1051,
        w: 152,
        h: 265,
    };
    assert_eq!(
        readback_outcome(false, &w28, &w28_back),
        ReadbackOutcome::Pending
    );
    // Counterfactual: the same one-pixel delta without a write stays pending.
    assert_eq!(
        readback_outcome(false, &w20, &w20_back),
        ReadbackOutcome::Pending
    );
    // Integer conversion reproduces the requested outer. The diagnostic's
    // observed_outer is pre-write, so it cannot prove post-write outer bounds.
    let insets = FrameInsets {
        left: 9,
        top: 0,
        right: 9,
        bottom: 9,
    };
    assert_eq!(
        insets.visible_to_outer(w20),
        Some(Rect {
            x: 55,
            y: 64,
            w: 1202,
            h: 603
        })
    );
}
