#![forbid(unsafe_code)]
use super::fullscreen::{OwnedExitOutcome, RetireProof};

/// Scripted owned-exit native effects (test-only): a fake style word plus
/// a scripted metadata store, with a call log pinning production
/// ordering. No live HWND API is ever touched; the suite runs wherever
/// the crate builds.
struct ScriptedOwnedExit {
    alive: bool,
    style: u32,
    fail_style_write: bool,
    fail_frame: bool,
    fail_rect: bool,
    restored_rect: Option<tiler_core::geometry::Rect>,
    /// Scripted live outer rectangle after the move; `restore_rect` records
    /// the placed frame here so a settled read matches by default.
    outer: Option<tiler_core::geometry::Rect>,
    /// Scripted answer for the settled-geometry read (`None` uses `outer`,
    /// the inner `None` scripts an unreadable outer rectangle).
    outer_override: Option<Option<tiler_core::geometry::Rect>>,
    /// Style bits the readback reports instead of the written style, for
    /// the unobserved-restore leg (`None` reads back the write).
    readback_override: Option<u32>,
    fail_maximize: bool,
    /// Scripted answers for each successive work-area settle read; an empty
    /// queue answers `true` (the settled maximized frame).
    settle_results: std::collections::VecDeque<bool>,
    fail_place: bool,
    fail_clear: bool,
    meta_present: bool,
    calls: Vec<&'static str>,
}

impl ScriptedOwnedExit {
    fn passing(style: u32) -> Self {
        ScriptedOwnedExit {
            alive: true,
            style,
            fail_style_write: false,
            fail_frame: false,
            fail_rect: false,
            restored_rect: None,
            outer: None,
            outer_override: None,
            readback_override: None,
            fail_maximize: false,
            settle_results: std::collections::VecDeque::new(),
            fail_place: false,
            fail_clear: false,
            meta_present: true,
            calls: Vec::new(),
        }
    }
}

impl super::OwnedExitEffects for ScriptedOwnedExit {
    fn is_window(&mut self, _hwnd: u64) -> bool {
        self.calls.push("is-window");
        self.alive
    }

    fn current_style(&mut self, _hwnd: u64) -> u32 {
        self.calls.push("style");
        self.style
    }

    fn restore_style(&mut self, _hwnd: u64, next: i32) -> (i32, u32) {
        self.calls.push("restore");
        if self.fail_style_write {
            return (0, 1);
        }
        self.style = next as u32;
        (1, 0)
    }

    fn reapply_frame(&mut self, _hwnd: u64) -> bool {
        self.calls.push("frame");
        !self.fail_frame
    }

    fn restore_rect(&mut self, _hwnd: u64, rect: tiler_core::geometry::Rect) -> bool {
        self.calls.push("rect");
        if self.fail_rect {
            return false;
        }
        self.restored_rect = Some(rect);
        self.outer = Some(rect);
        true
    }

    fn outer_rect(&mut self, _hwnd: u64) -> Option<tiler_core::geometry::Rect> {
        self.calls.push("outer");
        self.outer_override.unwrap_or(self.outer)
    }

    fn style_readback(&mut self, _hwnd: u64) -> u32 {
        self.calls.push("readback");
        self.readback_override.unwrap_or(self.style)
    }

    fn reassert_maximize(&mut self, _hwnd: u64) -> bool {
        self.calls.push("maximize");
        !self.fail_maximize
    }

    fn maximize_settled(&mut self, _hwnd: u64) -> bool {
        self.calls.push("settled");
        self.settle_results.pop_front().unwrap_or(true)
    }

    fn restore_maximized_frame(&mut self, _hwnd: u64) -> bool {
        self.calls.push("place");
        !self.fail_place
    }

    fn clear_meta(&mut self, _hwnd: u64) -> bool {
        self.calls.push("clear");
        if self.fail_clear {
            return false;
        }
        self.meta_present = false;
        true
    }
}

/// Application style bits outside the owned mask, standing in for window
/// state the client changed while fullscreen was held.
const UNRELATED: u32 = 0x1000_0000;

fn non_maximized_meta() -> super::FullscreenMeta {
    super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: None,
    }
}

fn maximized_meta() -> super::FullscreenMeta {
    super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: true,
        frame: None,
    }
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> tiler_core::geometry::Rect {
    tiler_core::geometry::Rect { x, y, w, h }
}

#[test]
fn successful_exit_restores_mask_and_clears_meta() {
    // Leg (d) retirement: a verified style/frame restoration clears the
    // metadata through the production ordering, restoring exactly the
    // owned mask while unrelated client bits survive.
    let changed = super::FULLSCREEN_STYLE_BITS;
    let style = UNRELATED | 0x0000_0001;
    assert_eq!(
        style & changed,
        0,
        "fixture starts with the owned bits cleared"
    );
    let meta = non_maximized_meta();
    let mut script = ScriptedOwnedExit::passing(style);
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta, || true);
    assert_eq!(outcome, OwnedExitOutcome::Restored);
    assert_eq!(
        script.calls,
        vec![
            "is-window",
            "style",
            "restore",
            "frame",
            "readback",
            "clear"
        ],
        "clear runs only after verified restoration"
    );
    assert_eq!(
        script.style,
        style | changed,
        "restore ORs exactly the owned mask over the live style"
    );
    assert_eq!(
        script.style & UNRELATED,
        UNRELATED,
        "unrelated client bits are preserved"
    );
    assert!(
        !script.meta_present,
        "verified restoration drops the metadata"
    );
}

#[test]
fn successful_exit_with_prior_maximize_reasserts_then_clears() {
    // A preexisting maximize reasserts and must prove the work-area frame
    // before metadata removal; a carried frame is ignored because maximized
    // exits restore placement, never geometry.
    let meta = super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: true,
        frame: Some(rect(500, 300, 415, 300)),
    };
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta, || true);
    assert_eq!(outcome, OwnedExitOutcome::Restored);
    assert_eq!(
        script.calls,
        vec![
            "is-window",
            "style",
            "restore",
            "frame",
            "readback",
            "maximize",
            "settled",
            "clear"
        ],
        "the settle proof precedes metadata removal and no move runs"
    );
    assert!(
        script.restored_rect.is_none(),
        "maximized exit moves nothing on the preimage path"
    );
    assert!(!script.meta_present);
}

#[test]
fn maximized_exit_places_settled_work_area_after_unproven_async_reassert() {
    // The reproduced risk: `ShowWindowAsync(SW_MAXIMIZE)` on a window that
    // was already maximized may leave the fullscreen rectangle behind. The
    // first settle read refuses, one bounded corrective move runs, and only
    // the second fresh proof authorizes metadata removal.
    let meta = maximized_meta();
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    script.settle_results = [false, true].into();
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta, || true);
    assert_eq!(outcome, OwnedExitOutcome::Restored);
    assert_eq!(
        script.calls,
        vec![
            "is-window",
            "style",
            "restore",
            "frame",
            "readback",
            "maximize",
            "settled",
            "place",
            "settled",
            "clear"
        ],
        "the corrective move happens between two fresh proofs"
    );
    assert!(!script.meta_present);
}

#[test]
fn maximized_exit_without_work_area_proof_keeps_metadata() {
    // A frame still covering the full monitor (or an asynchronous maximize
    // that has not landed) is never called restored: the metadata stays for
    // a later press or the retirement sweep.
    let meta = maximized_meta();
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    script.settle_results = [false, false].into();
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta, || true);
    assert_eq!(outcome, OwnedExitOutcome::Unproven);
    assert_eq!(
        script.calls,
        vec![
            "is-window",
            "style",
            "restore",
            "frame",
            "readback",
            "maximize",
            "settled",
            "place",
            "settled"
        ],
        "an unsettled frame never reaches metadata removal"
    );
    assert!(script.meta_present, "unproven maximize keeps the preimage");
}

#[test]
fn maximized_exit_refused_corrective_move_keeps_metadata() {
    let meta = maximized_meta();
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    script.settle_results = [false].into();
    script.fail_place = true;
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta, || true);
    assert_eq!(outcome, OwnedExitOutcome::Unproven);
    assert_eq!(
        script.calls,
        vec![
            "is-window",
            "style",
            "restore",
            "frame",
            "readback",
            "maximize",
            "settled",
            "place"
        ],
        "a refused corrective move never re-claims the settled frame"
    );
    assert!(script.meta_present);
}

#[test]
fn dead_window_throws_before_any_effect() {
    let meta = non_maximized_meta();
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    script.alive = false;
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta, || true);
    assert_eq!(outcome, OwnedExitOutcome::NotAttempted);
    assert_eq!(script.calls, vec!["is-window"]);
    assert!(script.meta_present, "nothing ran, metadata untouched");
}

#[test]
fn style_write_failure_keeps_metadata() {
    let meta = non_maximized_meta();
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    script.fail_style_write = true;
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta, || true);
    assert_eq!(
        outcome,
        OwnedExitOutcome::Refused,
        "an explicit setter refusal is not ambiguous"
    );
    assert_eq!(script.calls, vec!["is-window", "style", "restore"]);
    assert!(script.meta_present, "failed restore keeps the preimage");
}

#[test]
fn frame_failure_keeps_metadata() {
    let meta = non_maximized_meta();
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    script.fail_frame = true;
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta, || true);
    assert_eq!(outcome, OwnedExitOutcome::Refused);
    assert_eq!(script.calls, vec!["is-window", "style", "restore", "frame"]);
    assert!(script.meta_present, "failed restore keeps the preimage");
}

#[test]
fn unobserved_restore_keeps_metadata_for_explicit_retry() {
    // The style readback does not show the owned bits: `unproven` with
    // metadata kept, and a later explicit press with a verifying readback
    // finishes the same production sequence.
    let meta = non_maximized_meta();
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    script.readback_override = Some(UNRELATED);
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta, || true);
    assert_eq!(outcome, OwnedExitOutcome::Unproven);
    assert_eq!(
        script.calls,
        vec!["is-window", "style", "restore", "frame", "readback"]
    );
    assert!(
        script.meta_present,
        "unverified restoration never drops the preimage"
    );
    // Later explicit retry on the same press path.
    script.readback_override = None;
    script.calls.clear();
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta, || true);
    assert_eq!(outcome, OwnedExitOutcome::Restored);
    assert!(!script.meta_present, "verified retry finishes the exit");
}

#[test]
fn maximize_reapply_refusal_keeps_metadata() {
    // A refused preexisting-maximize reassert keeps the metadata for a
    // later press; removal never precedes the reassert.
    let meta = maximized_meta();
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    script.fail_maximize = true;
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta, || true);
    assert_eq!(outcome, OwnedExitOutcome::Unproven);
    assert_eq!(
        script.calls,
        vec![
            "is-window",
            "style",
            "restore",
            "frame",
            "readback",
            "maximize"
        ],
        "a refused reassert never reaches a settle proof or removal"
    );
    assert!(
        script.meta_present,
        "refused reassert keeps the preimage for retry"
    );
}

#[test]
fn clear_failure_reports_unproven_and_keeps_meta() {
    // Verified restoration with a failed removal readback reports the
    // ambiguous shape and keeps the marker so a later exit still finds its
    // preimage rather than stranding the frame.
    let meta = non_maximized_meta();
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    script.fail_clear = true;
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta, || true);
    assert_eq!(outcome, OwnedExitOutcome::Unproven);
    assert_eq!(
        script.calls,
        vec![
            "is-window",
            "style",
            "restore",
            "frame",
            "readback",
            "clear"
        ]
    );
    assert!(script.meta_present, "failed removal keeps the marker");
}

#[test]
fn durable_failure_keeps_marker_and_claims_no_restore() {
    // Review #17: a durable row that cannot clear after verified native
    // restoration must keep the native marker (and report the unproven
    // shape), otherwise a stale row can replay the old frame on next start.
    let meta = non_maximized_meta();
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta, || false);
    assert_eq!(outcome, OwnedExitOutcome::Unproven);
    assert_eq!(
        script.calls,
        vec!["is-window", "style", "restore", "frame", "readback"],
        "marker removal never runs when the exact durable row cannot clear"
    );
    assert!(
        script.meta_present,
        "the marker stays so a later sweep retries the durable row"
    );
}

#[test]
fn ordinary_exit_restores_non_full_monitor_preimage() {
    // The reproduced defect: a small ordinary float (415x300 at 500,300,
    // well inside the monitor) entered fullscreen and exited to the
    // fullscreen frame because the exit never moved the window. The exit
    // must move back to exactly the pre-fullscreen frame.
    let preimage = rect(500, 300, 415, 300);
    let meta = super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: Some(preimage),
    };
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta, || true);
    assert_eq!(outcome, OwnedExitOutcome::Restored);
    assert_eq!(
        script.calls,
        vec![
            "is-window",
            "style",
            "restore",
            "frame",
            "rect",
            "readback",
            "outer",
            "clear"
        ],
        "the frame move runs after the style reapply, before verification"
    );
    assert_eq!(script.restored_rect, Some(preimage));
    assert!(
        !script.meta_present,
        "verified restoration drops the metadata"
    );
}

#[test]
fn ordinary_exit_requires_the_settled_actual_outer_geometry() {
    // A confirmed style restore with a drifted, clamped, or unreadable outer
    // rectangle is not a restore: the R12 evidence and window marker stay so
    // a later press or the sweep retries. Only the exact pre-fullscreen frame
    // retires restore evidence.
    let preimage = rect(500, 300, 415, 300);
    let meta = super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: Some(preimage),
    };
    let drifted = rect(500, 300, 414, 300);
    for scripted in [Some(drifted), None] {
        let mut script = ScriptedOwnedExit::passing(UNRELATED);
        script.outer_override = Some(scripted);
        let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta, || true);
        assert_eq!(
            outcome,
            OwnedExitOutcome::Unproven,
            "an unsettled outer rectangle ({scripted:?}) never claims a restore"
        );
        assert_eq!(
            script.calls,
            vec![
                "is-window",
                "style",
                "restore",
                "frame",
                "rect",
                "readback",
                "outer"
            ],
            "the settled read precedes durable retirement and marker removal"
        );
        assert!(
            script.meta_present,
            "unsettled geometry keeps the preimage for retry"
        );
    }
    // The same press path retires once the exact frame reads back.
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    script.outer_override = Some(Some(drifted));
    assert_eq!(
        super::exit_fullscreen_owned_with(&mut script, 101, &meta, || true),
        OwnedExitOutcome::Unproven
    );
    script.outer_override = None;
    script.calls.clear();
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta, || true);
    assert_eq!(outcome, OwnedExitOutcome::Restored);
    assert_eq!(
        script.calls,
        vec![
            "is-window",
            "style",
            "restore",
            "frame",
            "rect",
            "readback",
            "outer",
            "clear"
        ]
    );
    assert!(!script.meta_present);
}

#[test]
fn settled_outer_geometry_requires_the_exact_recorded_frame() {
    let preimage = rect(-20, 30, 800, 600);
    assert!(
        super::fullscreen::outer_rect_settled(preimage, Some(preimage)),
        "the exact recorded outer rectangle settles"
    );
    for actual in [
        rect(-19, 30, 800, 600),
        rect(-20, 31, 800, 600),
        rect(-20, 30, 801, 600),
        rect(-20, 30, 800, 599),
        rect(0, 0, 1920, 1080),
    ] {
        assert!(
            !super::fullscreen::outer_rect_settled(preimage, Some(actual)),
            "drifted or fullscreen geometry never settles: {actual:?}"
        );
    }
    assert!(
        !super::fullscreen::outer_rect_settled(preimage, None),
        "an unreadable outer rectangle is never a settled proof"
    );
}

#[test]
fn legacy_exit_without_frame_keeps_style_only_path() {
    // Preimages stored before the frame capture (all four frame properties
    // absent) still exit without stranding: style restores, no move runs.
    let meta = non_maximized_meta();
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta, || true);
    assert_eq!(outcome, OwnedExitOutcome::Restored);
    assert_eq!(
        script.calls,
        vec![
            "is-window",
            "style",
            "restore",
            "frame",
            "readback",
            "clear"
        ]
    );
    assert!(
        script.restored_rect.is_none(),
        "no preimage frame means no move"
    );
    assert!(!script.meta_present);
}

#[test]
fn rect_restore_failure_keeps_metadata() {
    // A refused pre-fullscreen frame move keeps the metadata for a later
    // press; removal never precedes the move.
    let preimage = rect(500, 300, 415, 300);
    let meta = super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: Some(preimage),
    };
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    script.fail_rect = true;
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta, || true);
    assert_eq!(outcome, OwnedExitOutcome::Refused);
    assert_eq!(
        script.calls,
        vec!["is-window", "style", "restore", "frame", "rect"]
    );
    assert!(
        script.meta_present,
        "failed frame move keeps the preimage for retry"
    );
}

#[test]
fn work_area_settle_comparison_rejects_a_full_monitor_frame() {
    let work = rect(0, 0, 1920, 1040);
    assert!(
        super::fullscreen::frame_settled_on_work_area(work, work),
        "the maximized work-area frame settles"
    );
    assert!(
        super::fullscreen::frame_settled_on_work_area(rect(1, 2, 1918, 1036), work),
        "DWM's invisible-border inset still settles"
    );
    assert!(
        !super::fullscreen::frame_settled_on_work_area(rect(0, 0, 1920, 1080), work),
        "a full-monitor frame still covering the taskbar never settles"
    );
    assert!(
        !super::fullscreen::frame_settled_on_work_area(rect(-3, 0, 1920, 1040), work),
        "a frame extending past the work area never settles"
    );
    assert!(
        !super::fullscreen::frame_settled_on_work_area(rect(100, 100, 300, 300), work),
        "a frame that does not fill the work area is not a maximized settle"
    );
}

#[test]
fn covering_work_area_selects_the_display_under_the_frame_center() {
    let monitors = [
        (rect(0, 0, 1920, 1080), rect(0, 0, 1920, 1040)),
        (rect(1920, 0, 2560, 1440), rect(1920, 0, 2560, 1400)),
    ];
    assert_eq!(
        super::fullscreen::covering_work_area(rect(2500, 400, 300, 300), &monitors),
        Some(rect(1920, 0, 2560, 1400)),
        "the second display's work area is selected by the frame center"
    );
    assert_eq!(
        super::fullscreen::covering_work_area(rect(100, 100, 300, 300), &monitors),
        Some(rect(0, 0, 1920, 1040))
    );
    assert_eq!(
        super::fullscreen::covering_work_area(rect(-500, 100, 100, 100), &monitors),
        None,
        "no covering display is never a borrowed work area"
    );
}

#[test]
fn retire_requires_a_fresh_nonfullscreen_and_settled_maximize_proof() {
    let meta = non_maximized_meta();
    let settled = RetireProof {
        style_restored: true,
        fullscreen: Some(false),
        maximized: false,
        maximize_settled: false,
    };
    assert!(
        super::fullscreen::retire_eligible(&meta, settled),
        "a fresh non-fullscreen read with restored chrome retires"
    );
    for fullscreen in [None, Some(true)] {
        assert!(
            !super::fullscreen::retire_eligible(
                &meta,
                RetireProof {
                    fullscreen,
                    ..settled
                }
            ),
            "unreadable or fullscreen never retires"
        );
    }
    assert!(!super::fullscreen::retire_eligible(
        &meta,
        RetireProof {
            style_restored: false,
            ..settled
        }
    ));
    assert!(!super::fullscreen::retire_eligible(
        &super::FullscreenMeta { changed: 0, ..meta },
        settled
    ));

    // A maximized preimage that still reads maximized needs the work-area
    // proof; a window the app restored to normal is app-owned.
    let maximized = maximized_meta();
    assert!(!super::fullscreen::retire_eligible(
        &maximized,
        RetireProof {
            maximized: true,
            ..settled
        }
    ));
    assert!(super::fullscreen::retire_eligible(
        &maximized,
        RetireProof {
            maximized: true,
            maximize_settled: true,
            ..settled
        }
    ));
    assert!(super::fullscreen::retire_eligible(
        &maximized,
        RetireProof {
            maximized: false,
            ..settled
        }
    ));
}

#[test]
fn retirement_clears_durable_before_properties_and_claims_nothing_on_failure() {
    let log = std::cell::RefCell::new(Vec::new());
    let retired = super::fullscreen::retire_metadata(
        || {
            log.borrow_mut().push("durable");
            true
        },
        || {
            log.borrow_mut().push("properties");
            true
        },
    );
    assert!(retired);
    assert_eq!(
        log.borrow().as_slice(),
        ["durable", "properties"].as_slice(),
        "the durable row leaves before the window properties"
    );

    let log = std::cell::RefCell::new(Vec::new());
    let retired = super::fullscreen::retire_metadata(
        || {
            log.borrow_mut().push("durable");
            false
        },
        || {
            log.borrow_mut().push("properties");
            true
        },
    );
    assert!(
        !retired,
        "a failed durable removal never claims a retirement"
    );
    assert_eq!(
        log.borrow().as_slice(),
        ["durable"].as_slice(),
        "properties survive so a later sweep can retry"
    );
}

#[test]
fn retire_restored_fullscreen_fails_closed_without_a_live_lifetime() {
    use crate::native_guards::WindowKey;
    let dir = std::env::temp_dir();
    // A dead/bogus HWND never authorizes metadata removal; the marker stays
    // for the exact live exit path. Zero owned bits is never a restore proof.
    let dead = WindowKey {
        hwnd: 0x7fff_ffff_fff0,
        pid: 4,
        creation: "creation-x".to_owned(),
    };
    let meta = non_maximized_meta();
    assert!(!super::fullscreen::retire_restored_fullscreen(
        &dir, &dead, "tag-x", &meta
    ));
    let zero = super::FullscreenMeta { changed: 0, ..meta };
    assert!(!super::fullscreen::retire_restored_fullscreen(
        &dir, &dead, "tag-x", &zero
    ));
    assert!(!super::fullscreen::retire_restored_fullscreen(
        &dir, &dead, "", &meta
    ));
}

#[test]
fn frame_codec_round_trips_full_coordinate_range() {
    // The biased storage must distinguish NULL from every real coordinate,
    // including negative multi-monitor origins and `i32` extremes.
    for value in [i32::MIN, -2560, -1, 0, 1, 300, 500, 2560, i32::MAX] {
        let stored = super::encode_frame_value(value);
        assert_ne!(stored, 0, "encoded coordinate is never NULL");
        assert_eq!(super::decode_frame_value(stored), Some(value));
    }
    assert_eq!(
        super::decode_frame_value(0),
        None,
        "NULL never decodes to a coordinate"
    );
}
