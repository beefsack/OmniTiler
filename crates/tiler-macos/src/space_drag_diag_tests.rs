#![cfg(test)]
//! Offline tests for the user-run drag diagnostics: pure validators,
//! cursor-target refusals, transfer-identical order/source/flags/tag, and
//! the executors over a scripted backend with per-step fault injection
//! (post failures, gate invalidation, interrupt latch at every step,
//! unwind). Pure data only — no live dispatch, no windows touched.

use crate::space_drag_diag::*;
use crate::space_drag_transfer::DragStep;

// Fake backend: scripted clock, per-step posts/gates log, injectable
// failures, and an interrupt latch tripped once the step counter reaches
// `interrupt_at_step` (models SIGINT/SIGTERM during any step or hold
// slice). No live touch, no real sleep.
struct FakeDiagBackend {
    ops: Vec<String>,
    clock: u64,
    steps: usize,
    interrupt_at_step: Option<usize>,
    fail_gates: bool,
    fail_gates_after_mouse_down: bool,
    fail_chord_down: bool,
    fail_chord_up_times: usize,
    panic_once_on_chord_up: bool,
    fail_mouse: Option<DragStep>,
    refuse_mouse_up: bool,
    sleep_jump: u64,
    mouse_down: u32,
    mouse_up: u32,
    key_down: u32,
    key_up: u32,
}

impl FakeDiagBackend {
    fn new() -> Self {
        Self {
            ops: Vec::new(),
            clock: 0,
            steps: 0,
            interrupt_at_step: None,
            fail_gates: false,
            fail_gates_after_mouse_down: false,
            fail_chord_down: false,
            fail_chord_up_times: 0,
            panic_once_on_chord_up: false,
            fail_mouse: None,
            refuse_mouse_up: false,
            sleep_jump: 0,
            mouse_down: 0,
            mouse_up: 0,
            key_down: 0,
            key_up: 0,
        }
    }

    fn bump(&mut self, op: String) {
        self.steps += 1;
        self.ops.push(op);
    }

    fn balanced(&self) -> bool {
        self.mouse_down == self.mouse_up && self.key_down == self.key_up
    }
}

impl DiagBackend for FakeDiagBackend {
    fn now_ms(&mut self) -> u64 {
        self.clock
    }

    fn sleep_ms(&mut self, ms: u64) {
        self.bump(format!("sleep:{ms}"));
        self.clock += ms + self.sleep_jump;
    }

    fn interrupted(&mut self) -> bool {
        matches!(self.interrupt_at_step, Some(at) if self.steps >= at)
    }

    fn gates(&mut self, edge: &str) -> Result<(), String> {
        self.bump(format!("gates:{edge}"));
        if self.fail_gates || (self.fail_gates_after_mouse_down && self.mouse_down > 0) {
            return Err(format!("injected gates refusal at {edge}"));
        }
        Ok(())
    }

    fn post_chord(
        &mut self,
        down: bool,
        keycode: u32,
        flags: u64,
        tag: Option<i64>,
    ) -> Result<u64, String> {
        self.bump(format!(
            "chord:{}:{keycode:#x}:{flags:#x}:{tag:?}",
            if down { "down" } else { "up" }
        ));
        if down {
            if self.fail_chord_down {
                return Err("injected chord-down refusal".to_owned());
            }
            self.key_down += 1;
        } else {
            if self.panic_once_on_chord_up {
                self.panic_once_on_chord_up = false;
                panic!("injected panic on chord-up");
            }
            if self.fail_chord_up_times > 0 {
                self.fail_chord_up_times -= 1;
                return Err("injected chord-up refusal".to_owned());
            }
            self.key_up += 1;
        }
        Ok(3)
    }

    fn post_mouse(
        &mut self,
        step: DragStep,
        point: (f64, f64),
        flags: u64,
        tag: Option<i64>,
    ) -> Result<u64, String> {
        self.bump(format!("mouse:{step:?}:{point:?}:{flags:#x}:{tag:?}"));
        if self.fail_mouse == Some(step) {
            self.fail_mouse = None;
            return Err(format!("injected {step:?} refusal"));
        }
        match step {
            DragStep::MouseDown => self.mouse_down += 1,
            DragStep::MouseUp => {
                if self.refuse_mouse_up {
                    return Err("injected mouse-up refusal".to_owned());
                }
                self.mouse_up += 1;
            }
            _ => {}
        }
        Ok(5)
    }
}

fn target_inputs<'a>(
    ax: Option<(u32, u32)>,
    sls: Option<u32>,
    bundle: Option<&'a str>,
    subrole: Option<&'a str>,
) -> TargetInputs<'a> {
    TargetInputs {
        ax_owner: ax,
        sls_wid: sls,
        cg_exact: 1,
        cg_complete: true,
        bundle,
        subrole,
        fullscreen: Some(false),
        own_pid: 9999,
    }
}

#[test]
fn validators_pin_defaults_and_bounds() {
    assert_eq!(DEFAULT_COUNTDOWN_SECS, 5);
    assert_eq!(DEFAULT_HOLD_SECS, 6);
    assert_eq!(MAX_HOLD_SECS, 15);
    assert_eq!(DEFAULT_AMETHYST_DESKTOP, 2);
    assert!(validate_countdown(0).is_ok());
    assert!(validate_countdown(5).is_ok());
    assert!(validate_countdown(30).is_ok());
    assert!(validate_countdown(31).is_err());
    assert!(validate_hold(1).is_ok());
    assert!(validate_hold(6).is_ok());
    assert!(validate_hold(15).is_ok());
    assert!(validate_hold(0).is_err());
    assert!(validate_hold(16).is_err());
    assert!(validate_desktop(1).is_ok());
    assert!(validate_desktop(2).is_ok());
    assert!(validate_desktop(16).is_ok());
    assert!(validate_desktop(0).is_err());
    assert!(validate_desktop(17).is_err());
}

#[test]
fn amethyst_chord_needs_enabled_representable_holder() {
    use crate::desktop_shortcuts::ShortcutEntry;
    let entry = ShortcutEntry {
        enabled: true,
        keycode: 0x13,
        modifiers: 0x0004_0000,
    };
    assert_eq!(resolve_amethyst_chord(2, entry), Ok((0x13, 0x0004_0000)));
    // Disabled/unbound: refuse with the manual-configure hint (never a
    // settings write like Silica's auto-enable).
    let disabled = ShortcutEntry {
        enabled: false,
        ..entry
    };
    let err = resolve_amethyst_chord(2, disabled).unwrap_err();
    assert!(err.contains("manually"), "unexpected: {err}");
    assert!(err.contains("never writes settings"), "unexpected: {err}");
    // Unrepresentable keycode: refuse, never guess.
    let bad = ShortcutEntry {
        keycode: 999,
        ..entry
    };
    assert!(resolve_amethyst_chord(2, bad).is_err());
    // Desktop symbolic holder parity (Silica 118 + space - 1).
    assert_eq!(crate::desktop_shortcuts::desktop_symbolic_id(2), Some(119));
}

#[test]
fn target_accepts_one_joined_standard_textedit_window() {
    let target = resolve_target(target_inputs(
        Some((4242, 7)),
        Some(7),
        Some("com.apple.TextEdit"),
        Some("AXStandardWindow"),
    ))
    .unwrap();
    assert_eq!(target.pid, 4242);
    assert_eq!(target.wid, 7);
}

#[test]
fn target_refuses_ghostty_own_unknown_ambiguous_nonstandard() {
    // Ghostty in any case/variant.
    for bundle in [
        "com.mitchellh.ghostty",
        "COM.MITCHELLH.GHOSTTY",
        "com.mitchellh.ghostty-beta",
    ] {
        assert!(
            resolve_target(target_inputs(
                Some((4242, 7)),
                Some(7),
                Some(bundle),
                Some("AXStandardWindow")
            ))
            .is_err(),
            "{bundle}"
        );
    }
    // Own process and own bundles.
    assert!(
        resolve_target(TargetInputs {
            own_pid: 4242,
            ..target_inputs(
                Some((4242, 7)),
                Some(7),
                Some("com.apple.TextEdit"),
                Some("AXStandardWindow")
            )
        })
        .is_err()
    );
    for bundle in [crate::BUNDLE_ID, crate::HELPER_BUNDLE_ID] {
        assert!(
            resolve_target(target_inputs(
                Some((4242, 7)),
                Some(7),
                Some(bundle),
                Some("AXStandardWindow")
            ))
            .is_err(),
            "{bundle}"
        );
    }
    // Protected terminal bundles.
    assert!(
        resolve_target(target_inputs(
            Some((4242, 7)),
            Some(7),
            Some("com.apple.Terminal"),
            Some("AXStandardWindow")
        ))
        .is_err()
    );
    // Unknown/empty bundle.
    assert!(
        resolve_target(target_inputs(
            Some((4242, 7)),
            Some(7),
            None,
            Some("AXStandardWindow")
        ))
        .is_err()
    );
    assert!(
        resolve_target(target_inputs(
            Some((4242, 7)),
            Some(7),
            Some(""),
            Some("AXStandardWindow")
        ))
        .is_err()
    );
    // Ambiguous: AX owner vs WindowServer hit disagree.
    assert!(
        resolve_target(target_inputs(
            Some((4242, 7)),
            Some(9),
            Some("com.apple.TextEdit"),
            Some("AXStandardWindow")
        ))
        .unwrap_err()
        .contains("ambiguous")
    );
    // Unknown legs.
    assert!(
        resolve_target(target_inputs(
            None,
            Some(7),
            Some("com.apple.TextEdit"),
            Some("AXStandardWindow")
        ))
        .is_err()
    );
    assert!(
        resolve_target(target_inputs(
            Some((4242, 7)),
            None,
            Some("com.apple.TextEdit"),
            Some("AXStandardWindow")
        ))
        .is_err()
    );
    // Reserved identities.
    assert!(
        resolve_target(target_inputs(
            Some((1, 7)),
            Some(7),
            Some("com.apple.TextEdit"),
            Some("AXStandardWindow")
        ))
        .is_err()
    );
    assert!(
        resolve_target(target_inputs(
            Some((4242, 0)),
            Some(0),
            Some("com.apple.TextEdit"),
            Some("AXStandardWindow")
        ))
        .is_err()
    );
    // CG join: zero or many exact matches refuse; truncated snapshots refuse.
    for (exact, complete) in [(0, true), (2, true), (1, false)] {
        assert!(
            resolve_target(TargetInputs {
                cg_exact: exact,
                cg_complete: complete,
                ..target_inputs(
                    Some((4242, 7)),
                    Some(7),
                    Some("com.apple.TextEdit"),
                    Some("AXStandardWindow")
                )
            })
            .is_err(),
            "exact={exact} complete={complete}"
        );
    }
    // Nonstandard windows (sheets, dialogs, unknown subroles).
    for subrole in [None, Some(""), Some("AXSheet"), Some("AXDialog")] {
        assert!(
            resolve_target(target_inputs(
                Some((4242, 7)),
                Some(7),
                Some("com.apple.TextEdit"),
                subrole
            ))
            .is_err(),
            "{subrole:?}"
        );
    }
    // Native-fullscreen windows refuse; an unreadable fullscreen leg
    // proceeds (the runner reports the honest setup requirement instead).
    assert!(
        resolve_target(TargetInputs {
            fullscreen: Some(true),
            ..target_inputs(
                Some((4242, 7)),
                Some(7),
                Some("com.apple.TextEdit"),
                Some("AXStandardWindow")
            )
        })
        .unwrap_err()
        .contains("fullscreen")
    );
    assert!(
        resolve_target(TargetInputs {
            fullscreen: None,
            ..target_inputs(
                Some((4242, 7)),
                Some(7),
                Some("com.apple.TextEdit"),
                Some("AXStandardWindow")
            )
        })
        .is_ok()
    );
}

#[test]
fn chord_params_match_the_transfer_executor() {
    // Pinned values plus cross-module equality: the diagnostic chord IS the
    // transfer arrow chord (same keycode domain, same CG flags, same tag).
    assert_eq!(DIAG_CHORD_KEYCODE, 0x7C);
    assert_eq!(DIAG_CHORD_KEYCODE, crate::input_match::SWITCH_KEY_RIGHT);
    assert_eq!(DIAG_CHORD_KEYCODE, crate::input_policy::KEY_RIGHT);
    assert_eq!(DIAG_CHORD_KEYCODE, crate::symbolic_hotkeys::KEY_RIGHT_ARROW);
    assert_eq!(DIAG_CHORD_FLAGS, 0x0004_0000);
    assert_eq!(DIAG_CHORD_FLAGS, crate::input_match::SWITCH_CTRL_BIT);
    assert_eq!(DIAG_CHORD_FLAGS, crate::symbolic_hotkeys::CG_CTRL_BIT);
    assert_eq!(DIAG_OWN_TAG, crate::input_match::OWN_SWITCH_USER_DATA);
    assert_eq!(DIAG_MOUSE_FLAGS, 0);
    // Timing reuses the executor constants (no drifting copies).
    assert_eq!(
        FullParams::grab_settle_ms(),
        crate::space_drag_transfer::GRAB_SETTLE_MS
    );
    assert_eq!(
        FullParams::hold_ms(),
        crate::space_drag_transfer::ANIMATION_HOLD_MS
    );
    // Tag space: tagged and untagged both legal; anything else refused
    // before any post.
    assert!(crate::synth::validate_provenance_tag(Some(DIAG_OWN_TAG)).is_ok());
    assert!(crate::synth::validate_provenance_tag(None).is_ok());
    assert!(crate::synth::validate_provenance_tag(Some(0)).is_err());
    // Shared step vocabulary with the transfer executor.
    assert_eq!(
        crate::synth::HidMouseKind::from_drag_step(DragStep::MouseMove),
        Some(crate::synth::HidMouseKind::Move)
    );
    assert_eq!(
        crate::synth::HidMouseKind::from_drag_step(DragStep::MouseDown),
        Some(crate::synth::HidMouseKind::Down)
    );
    assert_eq!(
        crate::synth::HidMouseKind::from_drag_step(DragStep::MouseDragged),
        Some(crate::synth::HidMouseKind::Dragged)
    );
    assert_eq!(
        crate::synth::HidMouseKind::from_drag_step(DragStep::MouseUp),
        Some(crate::synth::HidMouseKind::Up)
    );
    assert_eq!(
        crate::synth::HidMouseKind::from_drag_step(DragStep::ChordDown),
        None
    );
    // Normal params: tagged both edges; untagged drops the tag; Amethyst:
    // configured keydown flags, keyup flags 0, untagged everything.
    let normal = ChordParams::ctrl_right(true);
    assert_eq!(normal.tag, Some(DIAG_OWN_TAG));
    assert_eq!(normal.down_flags, normal.up_flags);
    let untagged = ChordParams::ctrl_right(false);
    assert_eq!(untagged.tag, None);
    let amethyst = ChordParams::amethyst(0x13, 0x0004_0000);
    assert_eq!(amethyst.tag, None);
    assert_eq!(amethyst.down_flags, 0x0004_0000);
    assert_eq!(amethyst.up_flags, 0);
    let full = FullParams::amethyst(0x13, 0x0004_0000);
    assert_eq!(full.mouse_tag, None);
}

#[test]
fn chord_only_posts_exactly_one_pair_with_timestamps() {
    let mut backend = FakeDiagBackend::new();
    let mut events = Vec::new();
    let mut notes = Vec::new();
    let params = ChordParams::ctrl_right(true);
    let recovered = run_chord_only(&mut backend, &mut events, &params, &mut notes).unwrap();
    assert!(!recovered);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].kind, "chord-down");
    assert_eq!(events[1].kind, "chord-up");
    assert_eq!(events[0].seq, 0);
    assert_eq!(events[1].seq, 1);
    for event in &events {
        assert_eq!(event.keycode, Some(0x7C));
        assert_eq!(event.flags, DIAG_CHORD_FLAGS);
        assert_eq!(event.tag, Some(DIAG_OWN_TAG));
        assert!(event.point.is_none());
    }
    assert!(events[1].t_ms >= events[0].t_ms);
    assert!(backend.balanced());
    // Untagged variant: identical except the tag.
    let mut backend = FakeDiagBackend::new();
    let mut events = Vec::new();
    let mut notes = Vec::new();
    run_chord_only(
        &mut backend,
        &mut events,
        &ChordParams::ctrl_right(false),
        &mut notes,
    )
    .unwrap();
    assert!(events.iter().all(|e| e.tag.is_none()));
}

#[test]
fn full_posts_transfer_order_at_one_point_with_settles() {
    let mut backend = FakeDiagBackend::new();
    let mut events = Vec::new();
    let mut notes = Vec::new();
    let point = (100.0, 200.0);
    let recovered = run_full(
        &mut backend,
        &mut events,
        point,
        &FullParams::ctrl_right(true),
        &mut notes,
    )
    .unwrap();
    assert!(!recovered);
    let kinds: Vec<&str> = events.iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        [
            "mouse-move",
            "mouse-down",
            "mouse-dragged",
            "chord-down",
            "chord-up",
            "mouse-up"
        ]
    );
    // One point, executor-identical mouse flags/tag; chord flags/tag.
    for event in &events {
        if let Some(p) = event.point {
            assert_eq!(p, point);
            assert_eq!(event.flags, 0);
            assert_eq!(event.tag, Some(DIAG_OWN_TAG));
        } else {
            assert_eq!(event.flags, DIAG_CHORD_FLAGS);
        }
    }
    // Timestamps monotonic across the sequence.
    for pair in events.windows(2) {
        assert!(pair[1].t_ms >= pair[0].t_ms);
    }
    // Backend op order: gates + posts in transfer order with the grab
    // settle before the chord and the animation hold before release.
    let ops: Vec<&str> = backend
        .ops
        .iter()
        .map(|op| op.split(':').next().unwrap_or(op))
        .collect();
    assert_eq!(
        ops,
        [
            "gates", "mouse", "gates", "mouse", "gates", "mouse", "sleep", "gates", "chord",
            "chord", "sleep", "sleep", "sleep", "sleep", "sleep", "sleep", "sleep", "sleep",
            "mouse"
        ]
    );
    assert!(backend.balanced());
}

#[test]
fn full_amethyst_posts_configured_down_zero_up_untagged() {
    let mut backend = FakeDiagBackend::new();
    let mut events = Vec::new();
    let mut notes = Vec::new();
    run_full(
        &mut backend,
        &mut events,
        (100.0, 200.0),
        &FullParams::amethyst(0x13, 0x0004_0000),
        &mut notes,
    )
    .unwrap();
    // Everything untagged (Silica parity); keydown carries the configured
    // flags, keyup carries flags 0.
    for event in &events {
        assert_eq!(event.tag, None, "kind={}", event.kind);
    }
    let down = events.iter().find(|e| e.kind == "chord-down").unwrap();
    let up = events.iter().find(|e| e.kind == "chord-up").unwrap();
    assert_eq!(down.keycode, Some(0x13));
    assert_eq!(down.flags, 0x0004_0000);
    assert_eq!(up.flags, 0);
    assert!(backend.balanced());
}

#[test]
fn every_post_failure_releases_the_mouse_exactly_once() {
    // Fail each post step in turn: the mouse releases exactly once on
    // every path that held it, and keys stay balanced.
    for fail in [
        None,
        Some(DragStep::MouseMove),
        Some(DragStep::MouseDown),
        Some(DragStep::MouseDragged),
    ] {
        let mut backend = FakeDiagBackend::new();
        backend.fail_mouse = fail;
        let mut events = Vec::new();
        let mut notes = Vec::new();
        let result = run_full(
            &mut backend,
            &mut events,
            (1.0, 2.0),
            &FullParams::ctrl_right(true),
            &mut notes,
        );
        match fail {
            None => {
                result.unwrap();
                assert_eq!(backend.mouse_up, 1);
            }
            Some(DragStep::MouseMove) => {
                assert!(result.is_err());
                assert_eq!(backend.mouse_up, 0, "nothing held before move");
            }
            Some(DragStep::MouseDown) => {
                assert!(result.is_err());
                assert_eq!(backend.mouse_up, 0, "down never posted");
            }
            _ => {
                // Dragged refused after down posted: the unwind guard
                // releases (drop kind), exactly once.
                assert!(result.is_err());
                assert_eq!(backend.mouse_down, 1);
                assert_eq!(backend.mouse_up, 1);
                assert!(events.iter().any(|e| e.kind == "mouse-up-drop"));
            }
        }
        assert!(backend.balanced());
    }
    // Chord-down refused after the grab: mouse releases.
    let mut backend = FakeDiagBackend::new();
    backend.fail_chord_down = true;
    let mut events = Vec::new();
    let mut notes = Vec::new();
    assert!(
        run_full(
            &mut backend,
            &mut events,
            (1.0, 2.0),
            &FullParams::ctrl_right(true),
            &mut notes
        )
        .is_err()
    );
    assert_eq!(backend.mouse_up, 1);
    assert!(backend.balanced());
    // Chord-up refused once: recovered exactly once, balanced.
    let mut backend = FakeDiagBackend::new();
    backend.fail_chord_up_times = 1;
    let mut events = Vec::new();
    let mut notes = Vec::new();
    let recovered = run_full(
        &mut backend,
        &mut events,
        (1.0, 2.0),
        &FullParams::ctrl_right(true),
        &mut notes,
    )
    .unwrap();
    assert!(recovered);
    assert_eq!(backend.key_down, 1, "down never re-sent");
    assert_eq!(backend.key_up, 1, "up completed once");
    assert_eq!(backend.mouse_up, 1);
    assert!(backend.balanced());
    // Chord-up refused twice: pending, mouse still released.
    let mut backend = FakeDiagBackend::new();
    backend.fail_chord_up_times = 5;
    let mut events = Vec::new();
    let mut notes = Vec::new();
    let err = run_full(
        &mut backend,
        &mut events,
        (1.0, 2.0),
        &FullParams::ctrl_right(true),
        &mut notes,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        DiagError::UnresolvedPending {
            mouse_released: true,
            ..
        }
    ));
    assert_eq!(backend.mouse_up, 1);
    // Mouse-up refused: release failure parks dirty (may still be held).
    let mut backend = FakeDiagBackend::new();
    backend.refuse_mouse_up = true;
    let mut events = Vec::new();
    let mut notes = Vec::new();
    let err = run_full(
        &mut backend,
        &mut events,
        (1.0, 2.0),
        &FullParams::ctrl_right(true),
        &mut notes,
    )
    .unwrap_err();
    assert!(matches!(err, DiagError::ReleaseFailed { .. }));
    assert!(notes.iter().any(|n| n.contains("mouse-up refused")));
}

#[test]
fn interrupt_at_every_step_releases_first() {
    // Latch the interrupt at each step index across the full run: the
    // mouse releases exactly when held, keys stay balanced, and the error
    // is typed Interrupted (or a pre-post Refused when nothing posted).
    for at in 0..=30 {
        let mut backend = FakeDiagBackend::new();
        backend.interrupt_at_step = Some(at);
        let mut events = Vec::new();
        let mut notes = Vec::new();
        let result = run_full(
            &mut backend,
            &mut events,
            (1.0, 2.0),
            &FullParams::ctrl_right(true),
            &mut notes,
        );
        assert!(backend.mouse_up <= 1, "at={at}: up posted twice");
        if backend.mouse_down == 1 {
            assert_eq!(backend.mouse_up, 1, "at={at}: held mouse stranded");
        }
        assert!(backend.balanced(), "at={at}: keys unbalanced");
        match result {
            // Latch indices at/past the final release check never fire:
            // only those runs may complete.
            Ok(_) => assert!(at >= 19, "at={at}: completed despite latch"),
            Err(DiagError::Interrupted { .. }) | Err(DiagError::Refused { .. }) => {}
            Err(other) => panic!("at={at}: wrong error {other:?}"),
        }
        // Every mouse-up lands on the identical captured point.
        for event in &events {
            if let Some(p) = event.point {
                assert_eq!(p, (1.0, 2.0), "at={at}");
            }
        }
    }
}

#[test]
fn interrupt_during_hold_releases_within_the_cap() {
    let mut backend = FakeDiagBackend::new();
    backend.interrupt_at_step = Some(6);
    let mut events = Vec::new();
    let mut notes = Vec::new();
    let err = run_hold_only(
        &mut backend,
        &mut events,
        (1.0, 2.0),
        Some(DIAG_OWN_TAG),
        6_000,
        &mut notes,
    )
    .unwrap_err();
    assert!(matches!(err, DiagError::Interrupted { .. }));
    assert_eq!(backend.mouse_up, 1);
    // The hold never ran past the cap even on the interrupt path: the
    // fake clock only advanced through executed slices.
    assert!(backend.clock <= 6_000);
}

#[test]
fn hold_is_capped_and_reports_honestly() {
    // Exact cap: the loop exits on the deadline with bounded slices.
    let mut backend = FakeDiagBackend::new();
    let mut events = Vec::new();
    let mut notes = Vec::new();
    let held = run_hold_only(
        &mut backend,
        &mut events,
        (1.0, 2.0),
        Some(DIAG_OWN_TAG),
        6_000,
        &mut notes,
    )
    .unwrap();
    assert_eq!(held, 6_000);
    assert_eq!(backend.mouse_up, 1);
    assert_eq!(
        backend
            .ops
            .iter()
            .filter(|op| op.starts_with("sleep"))
            .count(),
        120
    );
    // Coarse clock jumps still terminate (bounded iterations) with the
    // overshoot REPORTED, never hidden.
    let mut backend = FakeDiagBackend::new();
    backend.sleep_jump = 10_000;
    let mut events = Vec::new();
    let mut notes = Vec::new();
    let held = run_hold_only(
        &mut backend,
        &mut events,
        (1.0, 2.0),
        Some(DIAG_OWN_TAG),
        400,
        &mut notes,
    )
    .unwrap();
    assert!(held >= 400, "overshoot must be reported, got {held}");
    assert!(backend.ops.len() < 20, "slices must stay bounded");
    assert_eq!(backend.mouse_up, 1);
}

#[test]
fn invalidated_gates_never_block_release() {
    // Gates fail after mouse-down (invalidated target/session): the leg
    // refuses AND the mouse still releases exactly once (drop path).
    let mut backend = FakeDiagBackend::new();
    backend.fail_gates_after_mouse_down = true;
    let mut events = Vec::new();
    let mut notes = Vec::new();
    let err = run_full(
        &mut backend,
        &mut events,
        (1.0, 2.0),
        &FullParams::ctrl_right(true),
        &mut notes,
    )
    .unwrap_err();
    assert!(matches!(err, DiagError::Refused { .. }));
    assert_eq!(backend.mouse_down, 1);
    assert_eq!(backend.mouse_up, 1);
    assert!(events.iter().any(|e| e.kind == "mouse-up-drop"));
}

#[test]
fn unwind_guard_releases_exactly_once() {
    let mut backend = FakeDiagBackend::new();
    let mut events = Vec::new();
    let mut notes = Vec::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut guard = MouseArmed::new(
            &mut backend,
            &mut events,
            &mut notes,
            (1.0, 2.0),
            Some(DIAG_OWN_TAG),
        );
        guard.push_mouse(DragStep::MouseDown).unwrap();
        panic!("simulated unwind while held");
    }));
    assert!(result.is_err());
    assert_eq!(backend.mouse_down, 1);
    assert_eq!(backend.mouse_up, 1, "unwind must not strand the button");
    assert!(events.iter().any(|e| e.kind == "mouse-up-drop"));
    assert!(notes.iter().any(|n| n.contains("unwind guard")));
}

#[test]
fn chord_only_recovers_pending_and_parks_dirty() {
    // Up refused once: recovery posts it exactly once.
    let mut backend = FakeDiagBackend::new();
    backend.fail_chord_up_times = 1;
    let mut events = Vec::new();
    let mut notes = Vec::new();
    let recovered = run_chord_only(
        &mut backend,
        &mut events,
        &ChordParams::ctrl_right(true),
        &mut notes,
    )
    .unwrap();
    assert!(recovered);
    assert_eq!(backend.key_down, 1);
    assert_eq!(backend.key_up, 1);
    assert_eq!(events.last().unwrap().kind, "chord-up-recovery");
    // Up refused twice: pending with no mouse involved.
    let mut backend = FakeDiagBackend::new();
    backend.fail_chord_up_times = 5;
    let mut events = Vec::new();
    let mut notes = Vec::new();
    let err = run_chord_only(
        &mut backend,
        &mut events,
        &ChordParams::ctrl_right(true),
        &mut notes,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        DiagError::UnresolvedPending {
            mouse_released: true,
            ..
        }
    ));
    // Down refused: nothing posted.
    let mut backend = FakeDiagBackend::new();
    backend.fail_chord_down = true;
    let mut events = Vec::new();
    let mut notes = Vec::new();
    assert!(matches!(
        run_chord_only(
            &mut backend,
            &mut events,
            &ChordParams::ctrl_right(true),
            &mut notes
        ),
        Err(DiagError::Refused { .. })
    ));
    assert!(events.is_empty());
}

#[test]
fn release_failure_outranks_interrupt_and_refusal() {
    // A failed mouse release is the honest terminal error: it must never
    // be disguised as the Interrupted/Refused outcome that preceded it
    // (finding: every `let _ = guard.release()` branch).
    // Interrupt during the grab settle + refused mouse-up.
    let mut backend = FakeDiagBackend::new();
    backend.interrupt_at_step = Some(7);
    backend.refuse_mouse_up = true;
    let mut events = Vec::new();
    let mut notes = Vec::new();
    let err = run_full(
        &mut backend,
        &mut events,
        (1.0, 2.0),
        &FullParams::ctrl_right(true),
        &mut notes,
    )
    .unwrap_err();
    assert!(
        matches!(err, DiagError::ReleaseFailed { .. }),
        "want ReleaseFailed, got {err:?}"
    );
    // Chord-down refused + refused mouse-up.
    let mut backend = FakeDiagBackend::new();
    backend.fail_chord_down = true;
    backend.refuse_mouse_up = true;
    let mut events = Vec::new();
    let mut notes = Vec::new();
    let err = run_full(
        &mut backend,
        &mut events,
        (1.0, 2.0),
        &FullParams::ctrl_right(true),
        &mut notes,
    )
    .unwrap_err();
    assert!(
        matches!(err, DiagError::ReleaseFailed { .. }),
        "want ReleaseFailed, got {err:?}"
    );
    // Hold interrupt + refused mouse-up.
    let mut backend = FakeDiagBackend::new();
    backend.interrupt_at_step = Some(5);
    backend.refuse_mouse_up = true;
    let mut events = Vec::new();
    let mut notes = Vec::new();
    let err = run_hold_only(
        &mut backend,
        &mut events,
        (1.0, 2.0),
        Some(DIAG_OWN_TAG),
        6_000,
        &mut notes,
    )
    .unwrap_err();
    assert!(
        matches!(err, DiagError::ReleaseFailed { .. }),
        "want ReleaseFailed, got {err:?}"
    );
    // Sanity: without the release failure the primary errors surface.
    let mut backend = FakeDiagBackend::new();
    backend.interrupt_at_step = Some(7);
    let mut events = Vec::new();
    let mut notes = Vec::new();
    let err = run_full(
        &mut backend,
        &mut events,
        (1.0, 2.0),
        &FullParams::ctrl_right(true),
        &mut notes,
    )
    .unwrap_err();
    assert!(matches!(err, DiagError::Interrupted { .. }));
}

#[test]
fn unwind_between_chord_edges_attempts_key_up_first() {
    // Chord-only: panic between down and up still attempts the up edge
    // with the same flags/tag (Amethyst up flags 0 here) on unwind.
    let mut backend = FakeDiagBackend::new();
    backend.panic_once_on_chord_up = true;
    let mut events = Vec::new();
    let mut notes = Vec::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_chord_only(
            &mut backend,
            &mut events,
            &ChordParams::amethyst(0x13, 0x0004_0000),
            &mut notes,
        )
    }));
    assert!(result.is_err(), "the injected panic must propagate");
    assert_eq!(backend.key_down, 1);
    assert_eq!(
        backend.key_up, 1,
        "unwind guard must attempt the pending key-up"
    );
    let up = events
        .iter()
        .find(|e| e.kind == "chord-up-drop")
        .expect("drop-kind up edge");
    assert_eq!(up.keycode, Some(0x13));
    assert_eq!(up.flags, 0, "Amethyst recovery reuses up flags 0");
    assert_eq!(up.tag, None);
    // Combined: key-up attempt precedes the mouse-up release on unwind.
    let mut backend = FakeDiagBackend::new();
    backend.panic_once_on_chord_up = true;
    let mut events = Vec::new();
    let mut notes = Vec::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_full(
            &mut backend,
            &mut events,
            (1.0, 2.0),
            &FullParams::ctrl_right(true),
            &mut notes,
        )
    }));
    assert!(result.is_err(), "the injected panic must propagate");
    assert_eq!(backend.key_up, 1);
    assert_eq!(backend.mouse_up, 1);
    let kinds: Vec<&str> = events.iter().map(|e| e.kind).collect();
    let key_pos = kinds.iter().position(|k| *k == "chord-up-drop").unwrap();
    let mouse_pos = kinds.iter().position(|k| *k == "mouse-up-drop").unwrap();
    assert!(
        key_pos < mouse_pos,
        "key release must precede mouse release: {kinds:?}"
    );
}
