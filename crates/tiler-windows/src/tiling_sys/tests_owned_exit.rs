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
    /// Style bits the readback reports instead of the written style, for
    /// the unobserved-restore leg (`None` reads back the write).
    readback_override: Option<u32>,
    fail_maximize: bool,
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
            readback_override: None,
            fail_maximize: false,
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
        true
    }

    fn style_readback(&mut self, _hwnd: u64) -> u32 {
        self.calls.push("readback");
        self.readback_override.unwrap_or(self.style)
    }

    fn reassert_maximize(&mut self, _hwnd: u64) -> bool {
        self.calls.push("maximize");
        !self.fail_maximize
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
    let meta = super::FullscreenMeta {
        changed,
        was_maximized: false,
        frame: None,
    };
    let mut script = ScriptedOwnedExit::passing(style);
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta);
    assert_eq!(outcome, "restored");
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
    // A preexisting maximize reasserts before metadata removal; the
    // reassert refusal below keeps the metadata instead. A carried frame is
    // ignored: maximized exits restore placement, never geometry.
    let meta = super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: true,
        frame: Some(tiler_core::geometry::Rect {
            x: 500,
            y: 300,
            w: 415,
            h: 300,
        }),
    };
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta);
    assert_eq!(outcome, "restored");
    assert_eq!(
        script.calls,
        vec![
            "is-window",
            "style",
            "restore",
            "frame",
            "readback",
            "maximize",
            "clear"
        ]
    );
    assert!(
        script.restored_rect.is_none(),
        "maximized exit moves nothing"
    );
    assert!(!script.meta_present);
}

#[test]
fn dead_window_throws_before_any_effect() {
    let meta = super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: None,
    };
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    script.alive = false;
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta);
    assert_eq!(outcome, "threw");
    assert_eq!(script.calls, vec!["is-window"]);
    assert!(script.meta_present, "nothing ran, metadata untouched");
}

#[test]
fn style_write_failure_keeps_metadata() {
    let meta = super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: None,
    };
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    script.fail_style_write = true;
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta);
    assert_eq!(outcome, "threw");
    assert_eq!(script.calls, vec!["is-window", "style", "restore"]);
    assert!(script.meta_present, "failed restore keeps the preimage");
}

#[test]
fn frame_failure_keeps_metadata() {
    let meta = super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: None,
    };
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    script.fail_frame = true;
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta);
    assert_eq!(outcome, "threw");
    assert_eq!(script.calls, vec!["is-window", "style", "restore", "frame"]);
    assert!(script.meta_present, "failed restore keeps the preimage");
}

#[test]
fn unobserved_restore_keeps_metadata_for_explicit_retry() {
    // The style readback does not show the owned bits: `dispatched`
    // with metadata kept, and a later explicit press with a verifying
    // readback finishes the same production sequence.
    let meta = super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: None,
    };
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    script.readback_override = Some(UNRELATED);
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta);
    assert_eq!(outcome, "dispatched");
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
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta);
    assert_eq!(outcome, "restored");
    assert!(!script.meta_present, "verified retry finishes the exit");
}

#[test]
fn maximize_reapply_refusal_keeps_metadata() {
    // A refused preexisting-maximize reassert keeps the metadata for a
    // later press; removal never precedes the reassert.
    let meta = super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: true,
        frame: None,
    };
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    script.fail_maximize = true;
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta);
    assert_eq!(outcome, "dispatched");
    assert_eq!(
        script.calls,
        vec![
            "is-window",
            "style",
            "restore",
            "frame",
            "readback",
            "maximize"
        ]
    );
    assert!(
        script.meta_present,
        "refused reassert keeps the preimage for retry"
    );
}

#[test]
fn clear_failure_reports_dispatched_and_keeps_meta() {
    // Verified restoration with a failed removal readback reports
    // `dispatched` and keeps the marker so a later exit still finds its
    // preimage rather than stranding the frame.
    let meta = super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: None,
    };
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    script.fail_clear = true;
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta);
    assert_eq!(outcome, "dispatched");
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
fn ordinary_exit_restores_non_full_monitor_preimage() {
    // The reproduced defect: a small ordinary float (415x300 at 500,300,
    // well inside the monitor) entered fullscreen and exited to the
    // fullscreen frame because the exit never moved the window. The exit
    // must move back to exactly the pre-fullscreen frame.
    let preimage = tiler_core::geometry::Rect {
        x: 500,
        y: 300,
        w: 415,
        h: 300,
    };
    let meta = super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: Some(preimage),
    };
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta);
    assert_eq!(outcome, "restored");
    assert_eq!(
        script.calls,
        vec![
            "is-window",
            "style",
            "restore",
            "frame",
            "rect",
            "readback",
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
fn legacy_exit_without_frame_keeps_style_only_path() {
    // Preimages stored before the frame capture (all four frame properties
    // absent) still exit without stranding: style restores, no move runs.
    let meta = super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: None,
    };
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta);
    assert_eq!(outcome, "restored");
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
    let preimage = tiler_core::geometry::Rect {
        x: 500,
        y: 300,
        w: 415,
        h: 300,
    };
    let meta = super::FullscreenMeta {
        changed: super::FULLSCREEN_STYLE_BITS,
        was_maximized: false,
        frame: Some(preimage),
    };
    let mut script = ScriptedOwnedExit::passing(UNRELATED);
    script.fail_rect = true;
    let outcome = super::exit_fullscreen_owned_with(&mut script, 101, &meta);
    assert_eq!(outcome, "threw");
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
