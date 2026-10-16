#![cfg(test)]
//! Offline tests for the drag-transfer executor: argument gates, grab
//! geometry, order-free hit-test, pointer decisions, terminal outcomes, and
//! the shared roundtrip over a scripted backend with per-boundary fault
//! injection. Pure data only -- no live dispatch, no windows touched.

use crate::ffi::{AxFrame, CgBounds, CgWindow, CgWindowBasic};
use crate::mc_probe::GhosttyWindow;
use crate::space_drag_transfer::*;

/// Fake backend: scripted membership/current effect plus per-boundary
/// fault injection plus a full ops log proving the real post/settle/poll
/// ordering. No live touch, no sleeps.
struct FakeDragBackend {
    ops: Vec<String>,
    counts: PostCounts,
    membership: Vec<u64>,
    current: Option<u64>,
    goal: u64,
    fail_mouse_at: Option<DragStep>,
    fail_chord_down: bool,
    fail_key_up_once: bool,
    fail_key_recovery: bool,
    fail_mouse_up: bool,
    fail_setup: bool,
    no_effect: bool,
    freeze_current: bool,
    unreadable: bool,
}

impl FakeDragBackend {
    fn new(start: Vec<u64>, current: Option<u64>) -> Self {
        Self {
            ops: Vec::new(),
            counts: PostCounts::default(),
            membership: start,
            current,
            goal: 0,
            fail_mouse_at: None,
            fail_chord_down: false,
            fail_key_up_once: false,
            fail_key_recovery: false,
            fail_mouse_up: false,
            fail_setup: false,
            no_effect: false,
            freeze_current: false,
            unreadable: false,
        }
    }
}

impl DragBackend for FakeDragBackend {
    fn begin_leg(&mut self, goal: u64) -> Result<(), String> {
        self.ops.push(format!("begin:{goal}"));
        if self.fail_setup {
            return Err("injected leg-setup refusal".to_owned());
        }
        self.goal = goal;
        Ok(())
    }

    fn post_mouse(&mut self, step: DragStep) -> Result<(), String> {
        self.ops.push(format!("mouse:{step:?}"));
        if self.fail_mouse_at == Some(step) {
            self.fail_mouse_at = None;
            return Err(format!("injected {step:?} refusal"));
        }
        if step == DragStep::MouseUp && self.fail_mouse_up {
            return Err("injected mouse-up refusal".to_owned());
        }
        match step {
            DragStep::MouseDown => self.counts.mouse_down += 1,
            DragStep::MouseUp => self.counts.mouse_up += 1,
            _ => {}
        }
        Ok(())
    }

    fn post_key(&mut self, down: bool) -> Result<(), String> {
        self.ops
            .push(format!("key:{}", if down { "down" } else { "up" }));
        if down {
            if self.fail_chord_down {
                return Err("injected chord-down refusal".to_owned());
            }
            self.counts.key_down += 1;
            return Ok(());
        }
        if self.fail_key_up_once {
            self.fail_key_up_once = false;
            return Err("injected chord-up refusal".to_owned());
        }
        if self.fail_key_recovery {
            return Err("injected chord-up recovery refusal".to_owned());
        }
        self.counts.key_up += 1;
        if !self.no_effect {
            self.membership = vec![self.goal];
            if !self.freeze_current {
                self.current = Some(self.goal);
            }
        }
        Ok(())
    }

    fn settle_grab(&mut self) {
        self.ops.push("settle:grab".to_owned());
    }

    fn settle_hold(&mut self) {
        self.ops.push("settle:hold".to_owned());
    }

    fn read_membership(&mut self) -> Option<Vec<u64>> {
        self.ops.push("read:membership".to_owned());
        if self.unreadable {
            return None;
        }
        Some(self.membership.clone())
    }

    fn read_current(&mut self) -> Option<u64> {
        self.ops.push("read:current".to_owned());
        if self.unreadable {
            return None;
        }
        self.current
    }

    fn effect_deadline_ms(&self) -> u64 {
        30
    }

    fn effect_poll_ms(&self) -> u64 {
        1
    }

    fn counts(&self) -> PostCounts {
        self.counts
    }
}

fn ghostty(x: f64, y: f64, w: f64, h: f64) -> GhosttyWindow {
    GhosttyWindow {
        wid: 9,
        pid: 99,
        layer: 0,
        onscreen: true,
        x,
        y,
        w,
        h,
        spaces: None,
    }
}

fn basic(wid: u32, pid: u32, layer: i32, x: f64, y: f64, w: f64, h: f64) -> CgWindowBasic {
    CgWindowBasic {
        wid,
        pid,
        layer,
        onscreen: true,
        bounds: CgBounds { x, y, w, h },
    }
}

#[test]
fn drag_args_require_opt_in_distinct_sids_and_budget() {
    assert!(validate_drag_args(1, 66, 120, false).is_err());
    assert!(validate_drag_args(0, 66, 120, true).is_err());
    assert!(validate_drag_args(1, 1, 120, true).is_err());
    assert!(validate_drag_args(1, 66, 45, true).is_err());
    assert!(validate_drag_args(1, 66, 121, true).is_err());
    assert_eq!(validate_drag_args(1, 66, 120, true).unwrap(), 120);
    assert_eq!(validate_drag_args(1, 66, 60, true).unwrap(), 60);
}

#[test]
fn temporary_drag_args_require_opt_in_original_and_budget() {
    // No temp SID exists yet: opt-in, nonzero original, budget floor/ceiling.
    assert!(validate_drag_args_temporary(1, 120, false).is_err());
    assert!(validate_drag_args_temporary(0, 120, true).is_err());
    assert!(validate_drag_args_temporary(1, 45, true).is_err());
    assert!(validate_drag_args_temporary(1, 121, true).is_err());
    assert_eq!(validate_drag_args_temporary(1, 120, true).unwrap(), 120);
    assert_eq!(validate_drag_args_temporary(1, 60, true).unwrap(), 60);
}

#[test]
fn temporary_baseline_needs_exact_single_original_space() {
    use crate::mc_probe::{DisplaySpaces, TopologySnapshot};
    let display = |spaces: &[u64], current: Option<u64>| DisplaySpaces {
        identifier: Some("uuid-test".to_owned()),
        spaces: spaces.to_vec(),
        current,
    };
    let snapshot = |rows: Vec<DisplaySpaces>| TopologySnapshot { displays: rows };
    // Exact single original Space, current home: pass.
    assert!(
        require_single_space_baseline(&snapshot(vec![display(&[11], Some(11))]), 0, 11).is_ok()
    );
    // Extra preexisting Space: refuse (the created temp would be ambiguous).
    assert!(
        require_single_space_baseline(&snapshot(vec![display(&[11, 12], Some(11))]), 0, 11)
            .is_err()
    );
    // Drifted current: refuse.
    assert!(
        require_single_space_baseline(&snapshot(vec![display(&[11], Some(12))]), 0, 11).is_err()
    );
    // Wrong original: refuse.
    assert!(
        require_single_space_baseline(&snapshot(vec![display(&[12], Some(12))]), 0, 11).is_err()
    );
    // Unknown display: refuse.
    assert!(
        require_single_space_baseline(&snapshot(vec![display(&[11], Some(11))]), 3, 11).is_err()
    );
}

#[test]
fn timing_contract_pins_source_parity_windows() {
    // Silica parity values: 50 ms grab settle, 400 ms animation hold.
    assert_eq!(GRAB_SETTLE_MS, 50);
    assert_eq!(ANIMATION_HOLD_MS, 400);
    // Post-release poll window/slice (live operational values; the fake
    // overrides them fast).
    assert_eq!(EFFECT_SETTLE_MS, 3_000);
    assert_eq!(EFFECT_POLL_MS, 200);
}

#[test]
fn grab_point_is_titlebar_center_and_refuses_degenerate() {
    let frame = AxFrame {
        x: 200.0,
        y: 200.0,
        w: 480.0,
        h: 320.0,
    };
    assert_eq!(titlebar_grab_point(frame), Ok((440.0, 208.0)));
    for bad in [
        AxFrame {
            x: 200.0,
            y: 200.0,
            w: 0.0,
            h: 320.0,
        },
        AxFrame {
            x: 200.0,
            y: 200.0,
            w: 480.0,
            h: 0.0,
        },
        AxFrame {
            x: f64::NAN,
            y: 200.0,
            w: 480.0,
            h: 320.0,
        },
        AxFrame {
            x: 200.0,
            y: 200.0,
            w: 40.0,
            h: 320.0,
        },
        AxFrame {
            x: 200.0,
            y: 200.0,
            w: 480.0,
            h: 10.0,
        },
    ] {
        assert!(titlebar_grab_point(bad).is_err());
    }
}

#[test]
fn hit_test_needs_owned_visible_unoccluded_point() {
    let owned = CgWindow {
        wid: 7,
        pid: 4242,
        layer: 0,
        onscreen: true,
        bounds: CgBounds {
            x: 200.0,
            y: 200.0,
            w: 480.0,
            h: 320.0,
        },
        title: None,
    };
    let point = (440.0, 208.0);
    let foreign = basic(8, 77, 0, 0.0, 0.0, 100.0, 100.0);
    // Owned visible at the point with unrelated windows elsewhere: pass.
    assert!(hit_test_grab(point, &owned, &[foreign], &[]).is_ok());
    // Offscreen or off-layer owned: refuse.
    assert!(
        hit_test_grab(
            point,
            &CgWindow {
                onscreen: false,
                ..owned.clone()
            },
            &[],
            &[]
        )
        .is_err()
    );
    assert!(
        hit_test_grab(
            point,
            &CgWindow {
                layer: 1,
                ..owned.clone()
            },
            &[],
            &[]
        )
        .is_err()
    );
    // Point outside owned bounds: refuse.
    assert!(hit_test_grab((10.0, 10.0), &owned, &[], &[]).is_err());
    // Reserved identity: refuse.
    assert!(
        hit_test_grab(
            point,
            &CgWindow {
                pid: 1,
                ..owned.clone()
            },
            &[],
            &[]
        )
        .is_err()
    );
    assert!(
        hit_test_grab(
            point,
            &CgWindow {
                wid: 0,
                ..owned.clone()
            },
            &[],
            &[]
        )
        .is_err()
    );
    // Above-normal-layer chrome over the point (panel, banner): refuse
    // without needing list order.
    let chrome = basic(3, 100, 25, 400.0, 190.0, 300.0, 60.0);
    assert!(hit_test_grab(point, &owned, &[chrome], &[]).is_err());
    // Ghostty over the point (even occluded): never Ghostty.
    assert!(hit_test_grab(point, &owned, &[], &[ghostty(400.0, 200.0, 300.0, 200.0)]).is_err());
    // Ghostty elsewhere: pass.
    assert!(hit_test_grab(point, &owned, &[], &[ghostty(0.0, 0.0, 100.0, 100.0)]).is_ok());
}

#[test]
fn hit_test_refuses_same_layer_foreign_occluder() {
    // The order-free hole: an unrelated same-layer nonfocused window over
    // the grab point must refuse (list order never proves topmost), while
    // the owned window's own list entry and below-layer entries pass.
    let owned = CgWindow {
        wid: 7,
        pid: 4242,
        layer: 0,
        onscreen: true,
        bounds: CgBounds {
            x: 200.0,
            y: 200.0,
            w: 480.0,
            h: 320.0,
        },
        title: None,
    };
    let point = (440.0, 208.0);
    // Unrelated same-layer window covering the grab point: refuse.
    let occluder = basic(8, 77, 0, 400.0, 190.0, 300.0, 60.0);
    assert!(hit_test_grab(point, &owned, &[occluder], &[]).is_err());
    // Same-PID sibling window covering the point: still refuse (the helper
    // owns exactly one window; a covering sibling is never the target).
    let sibling = basic(9, 4242, 0, 400.0, 190.0, 300.0, 60.0);
    assert!(hit_test_grab(point, &owned, &[sibling], &[]).is_err());
    // The owned window's own list entry covering the point: pass.
    let echo = basic(7, 4242, 0, 200.0, 200.0, 480.0, 320.0);
    assert!(hit_test_grab(point, &owned, &[echo], &[]).is_ok());
    // Below-normal-layer entry covering the point: cannot occlude a
    // normal-layer grab, so it passes.
    let below = basic(3, 100, -20, 400.0, 190.0, 300.0, 60.0);
    assert!(hit_test_grab(point, &owned, &[below], &[]).is_ok());
}

#[test]
fn hit_test_identity_needs_exact_owner() {
    // The AX element-at-position owner must equal the held target.
    assert!(hit_test_identity((4242, 7), (4242, 7)).is_ok());
    // Foreign owner (occluder, menu bar, desktop, chrome): refuse.
    assert!(hit_test_identity((77, 8), (4242, 7)).is_err());
    assert!(hit_test_identity((4242, 9), (4242, 7)).is_err());
    // Reserved hit identities refuse outright.
    assert!(hit_test_identity((1, 7), (4242, 7)).is_err());
    assert!(hit_test_identity((4242, 0), (4242, 7)).is_err());
}

#[test]
fn pointer_decision_restores_only_the_drag_left_cursor() {
    let base = (100.0, 100.0);
    let end = (440.0, 208.0);
    assert_eq!(
        pointer_restore_decision(Some(end), end, base),
        PointerRestore::Restore { to: base }
    );
    assert_eq!(
        pointer_restore_decision(Some(base), end, base),
        PointerRestore::AlreadyHome
    );
    // User-moved or unreadable: left alone.
    assert_eq!(
        pointer_restore_decision(Some((10.0, 10.0)), end, base),
        PointerRestore::LeaveUserMoved
    );
    assert_eq!(
        pointer_restore_decision(None, end, base),
        PointerRestore::LeaveUserMoved
    );
}

#[test]
fn pending_token_marks_edge_and_refusal() {
    let token = DragPending::KeyDownWithoutUp.token("oracle disagrees");
    assert!(token.contains("pending-release"));
    assert!(token.contains("chord-down-without-up"));
    let token = DragPending::MouseDownWithoutUp.token("session locked");
    assert!(token.contains("mouse-down-without-up"));
}

#[test]
fn executor_covers_real_silica_order_with_post_release_poll() {
    // The fake logs every backend call: setup, grab, grab settle, chord,
    // animation hold, release, THEN membership+current reads. The release
    // strictly precedes the first readback -- the corrected ordering.
    let mut backend = FakeDragBackend::new(vec![1], Some(1));
    let mut notes = Vec::new();
    let leg = run_drag_leg(&mut backend, 66, 66, "drag-o-t", &mut notes).unwrap();
    assert_eq!(leg.membership, vec![66]);
    assert_eq!(leg.current, 66);
    let order: Vec<&str> = backend
        .ops
        .iter()
        .map(|op| op.split(':').next().unwrap_or(op))
        .collect();
    let seq = order.join(">");
    assert!(
        seq.starts_with("begin>mouse>mouse>mouse>settle>key>key>settle>mouse>read"),
        "unexpected order: {seq}"
    );
    let up = backend
        .ops
        .iter()
        .position(|op| op == "mouse:MouseUp")
        .unwrap();
    let first_read = backend
        .ops
        .iter()
        .position(|op| op.starts_with("read:"))
        .unwrap();
    assert!(up < first_read, "release must precede readback");
    // Settles bracket the chord: grab settle after dragged, hold after up.
    let dragged = backend
        .ops
        .iter()
        .position(|op| op == "mouse:MouseDragged")
        .unwrap();
    let grab = backend
        .ops
        .iter()
        .position(|op| op == "settle:grab")
        .unwrap();
    let key_up = backend.ops.iter().position(|op| op == "key:up").unwrap();
    let hold = backend
        .ops
        .iter()
        .position(|op| op == "settle:hold")
        .unwrap();
    assert!(dragged < grab && grab < key_up && key_up < hold && hold < up);
}

#[test]
fn shared_roundtrip_completes_outward_and_inverse() {
    let mut backend = FakeDragBackend::new(vec![1], Some(1));
    let mut notes = Vec::new();
    let report = run_drag_roundtrip(&mut backend, 1, 66, &mut notes);
    assert_eq!(report.outcome, "drag-transfer-complete");
    assert!(report.outward.ok);
    assert_eq!(report.outward.membership, Some(vec![66]));
    assert_eq!(report.outward.current, Some(66));
    let inverse = report.inverse.expect("inverse runs after exact outward");
    assert!(inverse.ok);
    assert_eq!(inverse.membership, Some(vec![1]));
    assert_eq!(inverse.current, Some(1));
    assert!(report.releases_balanced);
    assert!(!report.unresolved);
    assert!(report.posts_total > 0);
    assert_eq!(backend.membership, vec![1]);
}

#[test]
fn failed_switch_releases_without_inverse() {
    let mut backend = FakeDragBackend::new(vec![1], Some(1));
    backend.no_effect = true;
    let mut notes = Vec::new();
    let report = run_drag_roundtrip(&mut backend, 1, 66, &mut notes);
    assert_eq!(report.outcome, "drag-transfer-residue");
    assert!(!report.outward.ok);
    // Typed variant with the current readback carried (matched by
    // variant through the record error text, never parsed for control).
    assert!(matches!(
        report.outward.error,
        Some(ref e) if e.contains("no effect")
    ));
    assert!(report.inverse.is_none());
    assert!(report.releases_balanced);
    assert_eq!(backend.counts.key_down, 1);
    assert_eq!(backend.counts.key_up, 1);
}

#[test]
fn current_mismatch_is_no_effect_with_both_readbacks() {
    // Membership lands but the current does not follow: typed NoEffect
    // carrying both readbacks for triage (matched by variant, and the
    // roundtrip refuses its inverse).
    let mut backend = FakeDragBackend::new(vec![1], Some(1));
    backend.freeze_current = true;
    let mut notes = Vec::new();
    let err = run_drag_leg(&mut backend, 66, 66, "drag-o-t", &mut notes).unwrap_err();
    assert!(matches!(
        err,
        LegError::NoEffect {
            observed: Some(_),
            current: Some(1),
            goal: 66,
        }
    ));
    assert!(!err.unresolved());
    let mut backend = FakeDragBackend::new(vec![1], Some(1));
    backend.freeze_current = true;
    let report = run_drag_roundtrip(&mut backend, 1, 66, &mut notes);
    assert_eq!(report.outcome, "drag-transfer-residue");
    assert!(report.inverse.is_none());
    assert!(report.releases_balanced);
}

#[test]
fn every_grab_boundary_refuses_typed_with_release() {
    for step in [
        DragStep::MouseMove,
        DragStep::MouseDown,
        DragStep::MouseDragged,
    ] {
        let mut backend = FakeDragBackend::new(vec![1], Some(1));
        backend.fail_mouse_at = Some(step);
        let mut notes = Vec::new();
        let report = run_drag_roundtrip(&mut backend, 1, 66, &mut notes);
        assert!(!report.outward.ok, "{step:?} must fail the leg");
        assert!(report.inverse.is_none(), "{step:?} must not invert");
        assert!(report.releases_balanced, "{step:?} must release");
        assert!(!report.unresolved);
    }
    let mut backend = FakeDragBackend::new(vec![1], Some(1));
    backend.fail_mouse_at = Some(DragStep::MouseDragged);
    let mut notes = Vec::new();
    assert!(matches!(
        run_drag_leg(&mut backend, 66, 66, "drag-o-t", &mut notes),
        Err(LegError::Refused {
            stage: "grab-dragged",
            ..
        })
    ));
}

#[test]
fn leg_setup_refusal_posts_nothing() {
    let mut backend = FakeDragBackend::new(vec![1], Some(1));
    backend.fail_setup = true;
    let mut notes = Vec::new();
    assert!(matches!(
        run_drag_leg(&mut backend, 66, 66, "drag-o-t", &mut notes),
        Err(LegError::Refused {
            stage: "leg-setup",
            ..
        })
    ));
    assert_eq!(backend.counts.total(), 0);
}

#[test]
fn chord_down_refusal_is_typed_and_released() {
    let mut backend = FakeDragBackend::new(vec![1], Some(1));
    backend.fail_chord_down = true;
    let mut notes = Vec::new();
    assert!(matches!(
        run_drag_leg(&mut backend, 66, 66, "drag-o-t", &mut notes),
        Err(LegError::Refused {
            stage: "chord-down",
            ..
        })
    ));
    assert!(backend.counts().balanced());
}

#[test]
fn refused_key_up_completes_once_then_continues() {
    let mut backend = FakeDragBackend::new(vec![1], Some(1));
    backend.fail_key_up_once = true;
    let mut notes = Vec::new();
    let leg = run_drag_leg(&mut backend, 66, 66, "drag-o-t", &mut notes).unwrap();
    assert_eq!(leg.membership, vec![66]);
    assert!(leg.pending_recovered);
    assert_eq!(backend.counts.key_down, 1, "key-down never re-sent");
    assert_eq!(backend.counts.key_up, 1, "pending release posted once");
    assert!(backend.counts().balanced());
    assert!(notes.iter().any(|n| n.contains("pending-release")));
}

#[test]
fn unresolvable_pending_parks_dirty_typed_with_mouse_released() {
    let mut backend = FakeDragBackend::new(vec![1], Some(1));
    backend.fail_key_up_once = true;
    backend.fail_key_recovery = true;
    let mut notes = Vec::new();
    let err = run_drag_leg(&mut backend, 66, 66, "drag-o-t", &mut notes).unwrap_err();
    assert!(matches!(
        err,
        LegError::UnresolvedPending {
            pending: DragPending::KeyDownWithoutUp,
            mouse_released: true,
            ..
        }
    ));
    assert!(err.unresolved());
    let mut backend = FakeDragBackend::new(vec![1], Some(1));
    backend.fail_key_up_once = true;
    backend.fail_key_recovery = true;
    let report = run_drag_roundtrip(&mut backend, 1, 66, &mut notes);
    assert_eq!(report.outcome, "drag-transfer-residue");
    assert!(report.unresolved);
    assert!(report.inverse.is_none());
}

#[test]
fn mouse_up_failure_parks_release_failed_typed() {
    let mut backend = FakeDragBackend::new(vec![1], Some(1));
    backend.fail_mouse_up = true;
    let mut notes = Vec::new();
    assert!(matches!(
        run_drag_leg(&mut backend, 66, 66, "drag-o-t", &mut notes),
        Err(LegError::ReleaseFailed { .. })
    ));
    let mut backend = FakeDragBackend::new(vec![1], Some(1));
    backend.fail_mouse_up = true;
    let report = run_drag_roundtrip(&mut backend, 1, 66, &mut notes);
    assert_eq!(report.outcome, "drag-transfer-residue");
    assert!(report.inverse.is_none());
    assert!(!report.releases_balanced);
    assert!(report.unresolved);
}

#[test]
fn unreadable_membership_refuses_without_inverse() {
    let mut backend = FakeDragBackend::new(vec![1], Some(1));
    backend.unreadable = true;
    let mut notes = Vec::new();
    assert!(matches!(
        run_drag_leg(&mut backend, 66, 66, "drag-o-t", &mut notes),
        Err(LegError::Refused {
            stage: "readback",
            ..
        })
    ));
    assert!(backend.counts().balanced());
}

#[test]
fn residue_outcome_needs_every_leg_clean() {
    assert_eq!(
        drag_transfer_outcome(true, true, true, false),
        "drag-transfer-complete"
    );
    assert_eq!(
        drag_transfer_outcome(false, true, true, false),
        "drag-transfer-residue"
    );
    assert_eq!(
        drag_transfer_outcome(true, false, true, false),
        "drag-transfer-residue"
    );
    assert_eq!(
        drag_transfer_outcome(true, true, false, false),
        "drag-transfer-residue"
    );
    assert_eq!(
        drag_transfer_outcome(true, true, true, true),
        "drag-transfer-residue"
    );
}

#[test]
fn temp_residue_hint_names_sid_and_narrow_recovery() {
    // Failure evidence for a possibly-resident internally created temp:
    // exact SID plus the narrow recovery command, never a broad cleanup.
    let hint = temp_residue_hint(5, 1);
    assert!(hint.contains('5'));
    assert!(hint.contains("mc-recover --temp 5 --original 1"));
}

#[test]
fn candidate_points_span_titlebar_with_center_compat() {
    let frame = AxFrame {
        x: 200.0,
        y: 200.0,
        w: 480.0,
        h: 320.0,
    };
    let points = candidate_grab_points(frame).unwrap();
    assert_eq!(points.len(), 5);
    // Center candidate matches the legacy single grab point.
    assert!(points.contains(&titlebar_grab_point(frame).unwrap()));
    for (x, y) in points {
        assert!(x >= frame.x && x <= frame.x + frame.w);
        assert_eq!(y, frame.y + 8.0);
    }
    assert!(candidate_grab_points(AxFrame { w: 10.0, ..frame }).is_err());
}

#[test]
fn selector_skips_occluded_center_for_clear_edge() {
    let owned = CgWindow {
        wid: 7,
        pid: 4242,
        layer: 0,
        onscreen: true,
        bounds: CgBounds {
            x: 200.0,
            y: 200.0,
            w: 480.0,
            h: 320.0,
        },
        title: None,
    };
    let frame = AxFrame {
        x: 200.0,
        y: 200.0,
        w: 480.0,
        h: 320.0,
    };
    // Narrow above-normal chrome over the center only: the selector must
    // land on a clear edge, never exempt the chrome.
    let chrome = basic(3, 100, 25, 420.0, 200.0, 60.0, 30.0);
    let point = select_clear_grab_point(frame, &owned, &[chrome], &[], None, (4242, 7)).unwrap();
    assert_ne!(point, titlebar_grab_point(frame).unwrap());
    assert!(hit_test_grab(point, &owned, &[chrome], &[]).is_ok());
    // Full-bar chrome: refuse (tried count named, chrome never exempted).
    let full = basic(3, 100, 25, 200.0, 200.0, 480.0, 30.0);
    let err = select_clear_grab_point(frame, &owned, &[full], &[], None, (4242, 7)).unwrap_err();
    assert!(err.contains("tried 5"));
    // Foreign AX owner at every candidate refuses even when CG is clear.
    let err =
        select_clear_grab_point(frame, &owned, &[], &[], Some((77, 8)), (4242, 7)).unwrap_err();
    assert!(err.contains("foreign"));
}

#[test]
fn frame_mismatch_names_differing_fields() {
    let a = AxFrame {
        x: 200.0,
        y: 200.0,
        w: 480.0,
        h: 320.0,
    };
    assert!(ax_frame_mismatch(a, a).is_none());
    let mut b = a;
    b.x = 200.5;
    assert!(ax_frame_mismatch(a, b).unwrap().contains('x'));
    let mut c = a;
    c.h = 321.0;
    assert!(ax_frame_mismatch(a, c).unwrap().contains('h'));
}

#[test]
fn occluder_detail_names_ids_and_geometry_without_exempting() {
    let owned = CgWindow {
        wid: 7,
        pid: 4242,
        layer: 0,
        onscreen: true,
        bounds: CgBounds {
            x: 200.0,
            y: 200.0,
            w: 480.0,
            h: 320.0,
        },
        title: None,
    };
    let point = (440.0, 208.0);
    // Above-normal chrome names its WID/PID/layer/bounds, still refuses.
    let chrome = basic(3, 100, 25, 400.0, 190.0, 300.0, 60.0);
    let detail = occluder_detail(point, &owned, &[chrome], &[]).unwrap();
    assert!(detail.contains("WID3/PID100"));
    assert!(detail.contains("above-normal"));
    assert!(
        hit_test_grab(point, &owned, &[chrome], &[])
            .unwrap_err()
            .contains("WID3/PID100")
    );
    // Same-layer foreign window names its IDs, still refuses.
    let flat = basic(8, 77, 0, 400.0, 190.0, 300.0, 60.0);
    let detail = occluder_detail(point, &owned, &[flat], &[]).unwrap();
    assert!(detail.contains("WID8/PID77"));
    assert!(detail.contains("same-layer"));
    assert!(
        hit_test_grab(point, &owned, &[flat], &[])
            .unwrap_err()
            .contains("WID8/PID77")
    );
    // Ghostty names its IDs, still refuses.
    let detail =
        occluder_detail(point, &owned, &[], &[ghostty(400.0, 200.0, 300.0, 200.0)]).unwrap();
    assert!(detail.contains("Ghostty"));
    assert!(
        hit_test_grab(point, &owned, &[], &[ghostty(400.0, 200.0, 300.0, 200.0)])
            .unwrap_err()
            .contains("Ghostty")
    );
    // Clear point: no occluder, passes.
    assert!(occluder_detail(point, &owned, &[], &[ghostty(0.0, 0.0, 10.0, 10.0)]).is_none());
}
