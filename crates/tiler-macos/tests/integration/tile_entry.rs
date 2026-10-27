//! Scoped-tile entry tests (offline, portable).
//!
//! Fakes drive the runtime through the tile catalogue surface: exact
//! matcher/action forwarding per arm (focus/move/grow/shrink/float/
//! orientation with directions, Shift/Alt-arm gating, extras and key-ups
//! ignored), resize sizing-step and repeat geometry outcomes, orientation
//! one-per-press geometry outcomes, CLI/request safety (explicit start,
//! non-empty scope, Ghostty, finite lifetime), and catalogue config
//! validation. No live AX, no taps, no Carbon, no Spaces, no desktop
//! effects — the built tile is never launched here.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;

use tiler_macos::cli::{Command, parse_args};
use tiler_macos::config::ProbeModifier;
use tiler_macos::input::cg_flag_bits;
use tiler_macos::input_match::TapKeyKind;
use tiler_macos::native_input::tile_catalogue;
use tiler_macos::runtime::{
    AxCompletion, AxObservation, InputDirection, ObservedWindow, Runtime, RuntimeConfig,
    RuntimeHandle, ShutdownOutcome, SubmitOutcome,
};
use tiler_macos::runtime_backend::{
    AxBackend, AxBackendFactory, InputSource, InputSourceFactory, RawKeyEvent,
};
use tiler_macos::tile::{TileInput, TileLogLevel, TileRequest};

const APP: u32 = 1001;
const FLUSH: Duration = Duration::from_secs(5);
const WAIT: Duration = Duration::from_secs(5);

// -- Fakes -------------------------------------------------------------------

struct FakeBackend;

impl AxBackend for FakeBackend {
    fn apply_frame(&mut self, _cmd: &tiler_macos::runtime::FrameCommand) -> AxCompletion {
        AxCompletion::Applied { elapsed_ms: 0 }
    }

    fn snapshot(&mut self) -> Result<Vec<ObservedWindow>, String> {
        Ok(vec![
            win(1, 0, 0, 100, 100),
            win(2, 100, 0, 100, 100),
            win(3, 200, 0, 100, 100),
        ])
    }
}

struct FakeFactory;

impl AxBackendFactory for FakeFactory {
    fn create(&self, _pid: u32) -> Result<Box<dyn AxBackend>, String> {
        Ok(Box::new(FakeBackend))
    }
}

struct FakeInput {
    rx: Mutex<Option<mpsc::Receiver<RawKeyEvent>>>,
}

impl InputSourceFactory for FakeInput {
    fn create(&self) -> Result<Box<dyn InputSource>, String> {
        let rx = self
            .rx
            .lock()
            .expect("mutex")
            .take()
            .ok_or_else(|| "taken".to_owned())?;
        Ok(Box::new(FakeSource { rx }))
    }

    fn describe(&self) -> &'static str {
        "fake"
    }
}

struct FakeSource {
    rx: mpsc::Receiver<RawKeyEvent>,
}

impl InputSource for FakeSource {
    fn next(&mut self, stop: &AtomicBool) -> Option<RawKeyEvent> {
        loop {
            match self.rx.recv_timeout(Duration::from_millis(10)) {
                Ok(event) => return Some(event),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if stop.load(Ordering::SeqCst) {
                        return None;
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return None,
            }
        }
    }
}

fn win(wid: u32, x: i32, y: i32, w: i32, h: i32) -> ObservedWindow {
    ObservedWindow {
        wid,
        x,
        y,
        w,
        h,
        floating: false,
        fullscreen: false,
        hints: tiler_macos::runtime::WindowSizeHints::none(),
    }
}

struct Fixture {
    handle: RuntimeHandle,
    input_tx: mpsc::Sender<RawKeyEvent>,
}

fn start_catalogue() -> Fixture {
    let (input_tx, input_rx) = mpsc::channel();
    let config = RuntimeConfig {
        tile_bindings: Some(tile_catalogue()),
        ..RuntimeConfig::default()
    };
    let handle = Runtime::start(
        config,
        Arc::new(FakeFactory),
        Arc::new(FakeInput {
            rx: Mutex::new(Some(input_rx)),
        }),
    )
    .expect("runtime starts");
    Fixture { handle, input_tx }
}

fn observe_focused(handle: &RuntimeHandle, generation: u64, wid: u32) {
    // The worker's own startup observation takes sequence 1, so this
    // superseeding sequence 2 must be QUEUED after it: the startup send
    // happens after `launch_app` reports ready, so without this wait the
    // seq-2 submit can win the inbox race, seq 1 is then rejected as
    // out-of-order, and the focus arrives at admission (`tiled`) instead
    // of as a focus projection — the geometry oracles below assume the
    // projection path (focused tile parked top-right, interior left edge).
    wait_startup_observation(handle);
    assert_eq!(
        handle.submit_observation(AxObservation {
            pid: APP,
            generation,
            seq: 2,
            windows: vec![
                win(1, 0, 0, 100, 100),
                win(2, 100, 0, 100, 100),
                win(3, 200, 0, 100, 100),
            ],
            focused_wid: Some(wid),
        }),
        SubmitOutcome::Accepted
    );
    assert!(handle.flush(FLUSH));
}

fn send_key(fixture: &Fixture, flags: u64, key_code: u32) {
    fixture
        .input_tx
        .send(RawKeyEvent {
            kind: TapKeyKind::KeyDown,
            flags,
            key_code,
        })
        .expect("send key");
}

fn send_key_up(fixture: &Fixture, flags: u64, key_code: u32) {
    fixture
        .input_tx
        .send(RawKeyEvent {
            kind: TapKeyKind::KeyUp,
            flags,
            key_code,
        })
        .expect("send key up");
}

/// Quiescence watermark for the asserting settlement fence below.
/// Currently unexercised here (positive waits park on the condition
/// directly); kept for the phase-1 successor's negative oracles.
#[allow(dead_code)]
fn settle_watermark(handle: &RuntimeHandle) -> Vec<u64> {
    // Every counter a quiescent settle round must leave untouched.
    // `engine_jobs_processed` is deliberately excluded: the fence's own
    // barrier jobs move it. `apps_live` recomputes on read; logger drops
    // and failure evidence ride their own assertions.
    let diagnostics = handle.diagnostics();
    vec![
        diagnostics.frames_emitted,
        diagnostics.frames_coalesced,
        diagnostics.frames_redundant_skipped,
        diagnostics.frames_overlay_skipped,
        diagnostics.observations_published,
        diagnostics.observation_failures,
        diagnostics.core_replies.values().sum(),
        diagnostics.completions.values().sum(),
        diagnostics.stale_dropped,
        diagnostics.terminated_invalidated,
        diagnostics.data_dropped_full,
        diagnostics.result_dropped_full,
        diagnostics.input_forwarded,
        diagnostics.input_ignored,
        diagnostics.input_refused_no_focus,
        diagnostics.input_refused_unsupported,
        diagnostics.input_select_enqueued,
        diagnostics.input_select_refused,
        diagnostics.focus_dispatched,
        diagnostics.focus_dropped_full,
    ]
}

/// Reused by the phase-1 successor for its negative oracles.
#[allow(dead_code)]
fn settle_known(handle: &RuntimeHandle, deadline: std::time::Instant) {
    // Causal settlement fence: one engine barrier plus one per-app worker
    // drain-mark round each, then repeat full rounds ONLY while the causal
    // watermark above moved (new work actually appeared mid-fence). Every
    // fence and every flush MUST succeed: a `false` fence, a failed flush,
    // or a deadline expiry is a genuine failure, never an ignored timeout.
    // Repeats park on worker drain-mark events and engine barriers; the
    // loop body never sleeps and never polls.
    let mut watermark = settle_watermark(handle);
    loop {
        if let Some(generation) = handle.app_generation(APP) {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            assert!(
                !remaining.is_zero(),
                "settle deadline elapsed before fencing app {APP}"
            );
            assert!(
                handle.wait_app_settled_for_test(APP, generation, remaining),
                "app {APP} generation {generation} never settled"
            );
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        assert!(
            !remaining.is_zero(),
            "settle deadline elapsed before engine flush"
        );
        assert!(handle.flush(remaining), "engine flush never completed");
        let next = settle_watermark(handle);
        if next == watermark {
            return;
        }
        watermark = next;
        assert!(
            std::time::Instant::now() < deadline,
            "system never quiesced: {watermark:?}"
        );
    }
}

fn wait_startup_observation(handle: &RuntimeHandle) {
    // The worker publishes exactly one startup observation per launch;
    // observing the publish (queued, FIFO) before submitting sequence 2
    // pins the intended startup-then-focus order. Parks on the shared
    // condition until the worker's own publish notifies — never a poll.
    handle
        .wait_diagnostics_for_test(WAIT, |diagnostics| diagnostics.observations_published >= 1)
        .expect("worker startup observation never published");
}

fn wait_forwarded(handle: &RuntimeHandle, baseline: u64) -> u64 {
    // Parks on the shared condition until the engine's input-forward
    // notifies; the input thread carries the chord without sleeps.
    let diagnostics = handle
        .wait_diagnostics_for_test(WAIT, |diagnostics| diagnostics.input_forwarded > baseline)
        .unwrap_or_else(|| panic!("input never forwarded (baseline {baseline})"));
    diagnostics.input_forwarded
}

/// Wait until the engine has consumed ignored inputs up to `minimum`
/// (each ignored chord increments the counter when the input thread's
/// delivery reaches the engine). The input thread's own ignored-increment
/// notifies the shared condition, so the returned snapshot is the exact
/// post-consumption state — the caller asserts its negative oracle
/// (forwarded counts, reply counts) on that snapshot, never on a fixed
/// window.
fn wait_ignored(
    handle: &RuntimeHandle,
    minimum: u64,
    what: &str,
) -> tiler_macos::runtime::RuntimeDiagnostics {
    handle
        .wait_diagnostics_for_test(WAIT, |diagnostics| diagnostics.input_ignored >= minimum)
        .unwrap_or_else(|| panic!("ignored inputs never consumed ({what})"))
}

fn reply_count(handle: &RuntimeHandle, prefix: &str) -> u64 {
    handle
        .diagnostics()
        .core_replies
        .iter()
        .filter(|(k, _)| k.starts_with(prefix))
        .map(|(_, v)| v)
        .sum()
}

// -- Catalogue forwarding ----------------------------------------------------

#[test]
fn catalogue_forwards_focus_move_float_with_exact_arms() {
    let fixture = start_catalogue();
    let generation = fixture.handle.launch_app(APP).expect("launch");
    observe_focused(&fixture.handle, generation, 1);

    const CTRL: u64 = 0x0004_0000;
    const SHIFT: u64 = 0x0002_0000;
    // H/J/K/L keycodes: H left, J down, K up, L right; G float.
    let (ctrl, _) = cg_flag_bits(ProbeModifier::Ctrl);
    assert_eq!(ctrl, CTRL);

    // Focus arm: Ctrl+H steps focus (observed-focus subject exists).
    let baseline = fixture.handle.diagnostics().input_forwarded;
    send_key(&fixture, CTRL, 0x04);
    wait_forwarded(&fixture.handle, baseline);
    assert!(fixture.handle.flush(FLUSH));
    assert!(reply_count(&fixture.handle, "focus-") >= 1);

    // Move arm: Ctrl+Shift+L moves; bare Ctrl+L would focus instead (sent
    // separately below is NOT done here — one arm per chord).
    let baseline = fixture.handle.diagnostics().input_forwarded;
    send_key(&fixture, CTRL | SHIFT, 0x25);
    wait_forwarded(&fixture.handle, baseline);
    assert!(fixture.handle.flush(FLUSH));
    assert!(reply_count(&fixture.handle, "move-") >= 1);

    // Float arm: Ctrl+G toggles via the existing core command.
    let baseline = fixture.handle.diagnostics().input_forwarded;
    send_key(&fixture, CTRL, 0x05);
    wait_forwarded(&fixture.handle, baseline);
    assert!(fixture.handle.flush(FLUSH));
    assert!(reply_count(&fixture.handle, "toggle-float") >= 1);

    // Arrow aliases ride the same arms.
    let baseline = fixture.handle.diagnostics().input_forwarded;
    send_key(&fixture, CTRL, 0x7B);
    wait_forwarded(&fixture.handle, baseline);
    let baseline = fixture.handle.diagnostics().input_forwarded;
    send_key(&fixture, CTRL | SHIFT, 0x7C);
    wait_forwarded(&fixture.handle, baseline);

    let report = fixture.handle.shutdown(Duration::from_secs(5));
    assert_eq!(report.outcome, ShutdownOutcome::Clean, "{report:?}");
}

#[test]
fn catalogue_ignores_extras_keyups_and_unrelated() {
    let fixture = start_catalogue();
    let generation = fixture.handle.launch_app(APP).expect("launch");
    observe_focused(&fixture.handle, generation, 1);

    const CTRL: u64 = 0x0004_0000;
    const SHIFT: u64 = 0x0002_0000;
    const META: u64 = 0x0010_0000;
    let ignored_baseline = fixture.handle.diagnostics().input_ignored;
    let forwarded_baseline = fixture.handle.diagnostics().input_forwarded;

    // Extra modifier on a focus chord: ignored (Shift would make it move,
    // Meta breaks it entirely).
    send_key(&fixture, CTRL | META, 0x04);
    // Bare Ctrl+H is focus; Ctrl+Shift+H is move — but Ctrl+Shift+Meta+H
    // matches nothing.
    send_key(&fixture, CTRL | SHIFT | META, 0x04);
    // Key-up never forwards (the owner filters edges).
    send_key_up(&fixture, CTRL, 0x04);
    // Unrelated key never forwards.
    send_key(&fixture, CTRL, 0x09);
    // Bare H with no modifier never forwards.
    send_key(&fixture, 0, 0x04);
    assert!(fixture.handle.flush(FLUSH));
    // All five ignored chords must reach the engine (ignored count is the
    // worker-proof arrival signal); the forwarded count must not move.
    let diagnostics = wait_ignored(&fixture.handle, ignored_baseline + 5, "five inexact chords");
    assert_eq!(diagnostics.input_forwarded, forwarded_baseline);
    assert!(
        diagnostics.input_ignored >= ignored_baseline + 5,
        "all five must ignore: {diagnostics:?}"
    );

    // ...while the exact chords still forward afterwards.
    send_key(&fixture, CTRL, 0x04);
    wait_forwarded(&fixture.handle, forwarded_baseline);

    let report = fixture.handle.shutdown(Duration::from_secs(5));
    assert_eq!(report.outcome, ShutdownOutcome::Clean, "{report:?}");
}

// -- CLI / request safety ----------------------------------------------------

fn argv(words: &[&str]) -> Vec<String> {
    std::iter::once("tiler-macos".to_owned())
        .chain(words.iter().map(|s| s.to_string()))
        .collect()
}

fn tile_request(words: &[&str]) -> TileRequest {
    let parsed = parse_args(&argv(words)).expect("parses");
    let Command::Tile {
        user_start,
        scopes,
        seconds,
        input,
        preset,
        preset_explicit: _,
        unscoped,
        excluded,
        diag_stay,
        diag_trailing_click,
        log,
        run_dir,
        fixture_owned_spaces,
        fixture_starting_space,
        border: _,
        pager_highlight: _,
        drag: _,
        indicator: _,
    } = parsed.command
    else {
        panic!("expected tile, got {:?}", parsed.command);
    };
    TileRequest {
        user_start,
        scopes,
        seconds,
        input,
        preset,
        modifier: parsed.modifier,
        unscoped,
        excluded,
        diag_stay,
        diag_trailing_click,
        log,
        run_dir,
        fixture_owned_spaces,
        fixture_starting_space,
        border: tiler_macos::active_border::ActiveBorderOptions::default(),
        pager_highlight: tiler_macos::pager_highlight::PagerHighlightStyle::default(),
        drag: tiler_macos::drag_drop::DragOptions::default(),
        indicator: tiler_macos::workspace_indicator::IndicatorOptions::default(),
    }
}

#[test]
fn tile_cli_safety_end_to_end() {
    // Happy path: explicit start, named scopes, finite lifetime, tap route.
    let request = tile_request(&[
        "tile",
        "--user-start",
        "--scope",
        "com.apple.TextEdit",
        "--scope",
        "com.apple.Calculator",
        "--seconds",
        "30",
        "--input",
        "tap",
    ]);
    assert!(request.validate().is_ok());
    assert_eq!(request.input, TileInput::Tap);

    // Missing --user-start parses but never validates.
    assert!(
        tile_request(&["tile", "--scope", "com.apple.TextEdit"])
            .validate()
            .is_err()
    );
    // Empty scope parses but never validates.
    assert!(tile_request(&["tile", "--user-start"]).validate().is_err());
    // Ghostty (any case/variant) and the own bundle never validate.
    for scope in [
        "com.mitchellh.ghostty",
        "COM.MITCHELLH.GHOSTTY-BETA",
        tiler_macos::BUNDLE_ID,
    ] {
        assert!(
            tile_request(&["tile", "--user-start", "--scope", scope])
                .validate()
                .is_err(),
            "{scope}"
        );
    }
    // Unbounded lifetimes never validate.
    assert!(
        tile_request(&[
            "tile",
            "--user-start",
            "--scope",
            "com.apple.TextEdit",
            "--seconds",
            "0"
        ])
        .validate()
        .is_err()
    );
    assert!(
        tile_request(&[
            "tile",
            "--user-start",
            "--scope",
            "com.apple.TextEdit",
            "--seconds",
            "121"
        ])
        .validate()
        .is_err()
    );
    // The Ctrl-role modifier stays configurable (Ctrl default).
    let request = tile_request(&[
        "--ctrl",
        "ctrl",
        "tile",
        "--user-start",
        "--scope",
        "com.apple.TextEdit",
    ]);
    assert_eq!(request.modifier, ProbeModifier::Ctrl);
    assert!(request.validate().is_ok());
}

#[test]
fn tile_cli_unscoped_end_to_end() {
    // Unscoped product mode: no allowlist, exact exclusions, unbounded
    // opt-in via --seconds 0, foreground verbosity via --log.
    let request = tile_request(&[
        "tile",
        "--user-start",
        "--unscoped",
        "--seconds",
        "0",
        "--exclude",
        "com.example.skip",
        "--log",
        "trace",
    ]);
    assert!(request.unscoped);
    assert!(request.is_unbounded());
    assert_eq!(request.log, TileLogLevel::Trace);
    assert_eq!(request.excluded, vec!["com.example.skip"]);
    assert!(request.validate().is_ok());
    // Bounded unscoped also validates (finite dogfood slices).
    let request = tile_request(&["tile", "--user-start", "--unscoped", "--seconds", "30"]);
    assert!(request.validate().is_ok());
    assert!(!request.is_unbounded());
    // --scope with --unscoped parses but never validates (scoped stays
    // allowlist-only); --exclude without --unscoped never validates either.
    assert!(
        tile_request(&[
            "tile",
            "--user-start",
            "--unscoped",
            "--scope",
            "com.apple.TextEdit",
        ])
        .validate()
        .is_err()
    );
    assert!(
        tile_request(&[
            "tile",
            "--user-start",
            "--scope",
            "com.apple.TextEdit",
            "--exclude",
            "com.example.skip",
        ])
        .validate()
        .is_err()
    );
    // Ghostty needs no scope unscoped (ordinary candidate by rule), while
    // scoped Ghostty never validates.
    assert!(
        tile_request(&["tile", "--user-start", "--unscoped"])
            .validate()
            .is_ok()
    );
    // Unknown --log refuses at parse time; --exclude needs a value.
    assert!(
        parse_args(&argv(&[
            "tile",
            "--user-start",
            "--unscoped",
            "--log",
            "debug"
        ]))
        .is_err()
    );
    assert!(parse_args(&argv(&["tile", "--user-start", "--exclude"])).is_err());
}

// -- Catalogue config validation ----------------------------------------------

#[test]
fn catalogue_config_validates() {
    let good = RuntimeConfig {
        tile_bindings: Some(tile_catalogue()),
        ..RuntimeConfig::default()
    };
    assert!(good.validate().is_ok());
    assert_eq!(good.tile_bindings.as_ref().map(|b| b.len()), Some(34));

    // Shift-as-Meta collapses the focus/move arms to the same native chord.
    // Refuse it before registering rather than silently choosing focus.
    let collapsed = RuntimeConfig {
        modifier: ProbeModifier::Shift,
        tile_bindings: Some(tile_catalogue()),
        ..RuntimeConfig::default()
    };
    assert!(collapsed.validate().is_err());
    // Alt-as-Meta collapses the Alt (resize) arm onto focus/move the same
    // way (grow `Alt+Alt+H` lands on focus `Alt+H`). Only Ctrl (default)
    // and Meta (Cmd) carry the full catalogue distinctly.
    let alt_collapsed = RuntimeConfig {
        modifier: ProbeModifier::Alt,
        tile_bindings: Some(tile_catalogue()),
        ..RuntimeConfig::default()
    };
    assert!(alt_collapsed.validate().is_err());
    for modifier in [ProbeModifier::Ctrl, ProbeModifier::Meta] {
        assert!(
            RuntimeConfig {
                modifier,
                tile_bindings: Some(tile_catalogue()),
                ..RuntimeConfig::default()
            }
            .validate()
            .is_ok()
        );
    }

    let empty = RuntimeConfig {
        tile_bindings: Some(vec![]),
        ..RuntimeConfig::default()
    };
    assert!(empty.validate().is_err());

    // Duplicate (key, shift, alt, aux) refuses.
    let mut duplicated = tile_catalogue();
    duplicated.push(tiler_macos::runtime::TileBinding {
        key_code: 0x04,
        shift: false,
        alt: false,
        aux: false,
        action: tiler_macos::runtime::InputSemanticAction::FocusStep {
            direction: InputDirection::Left,
        },
    });
    let duplicated = RuntimeConfig {
        tile_bindings: Some(duplicated),
        ..RuntimeConfig::default()
    };
    assert!(duplicated.validate().is_err());

    // Out-of-range key refuses.
    let mut bad_key = tile_catalogue();
    bad_key[0].key_code = 256;
    let bad_key = RuntimeConfig {
        tile_bindings: Some(bad_key),
        ..RuntimeConfig::default()
    };
    assert!(bad_key.validate().is_err());
}

// -- Resize and orientation forwarding ---------------------------------------

const ALT: u64 = 0x0008_0000;
const CMD: u64 = 0x0010_0000;
/// `O` keycode: the parent orientation toggle.
const KEY_O: u32 = 0x1F;

fn desired_width(handle: &RuntimeHandle, window: &str) -> Option<i32> {
    handle
        .last_desired_geometry()
        .iter()
        .find(|frame| frame.window == window)
        .map(|frame| frame.w)
}

fn desired_snapshot(handle: &RuntimeHandle) -> Vec<(String, i32, i32, i32, i32)> {
    let mut frames: Vec<(String, i32, i32, i32, i32)> = handle
        .last_desired_geometry()
        .iter()
        .map(|frame| (frame.window.clone(), frame.x, frame.y, frame.w, frame.h))
        .collect();
    frames.sort();
    frames
}

#[test]
fn catalogue_resize_arms_are_exact() {
    // All four directional arms on one key forward; the same key with an
    // extra masked modifier, with no base modifier, or bare matches
    // nothing. `Ctrl+H` (focus) and `Ctrl+Alt+H` (grow) are distinct
    // effective chords, never blanket keycode assumptions.
    const CTRL: u64 = 0x0004_0000;
    const SHIFT: u64 = 0x0002_0000;
    let fixture = start_catalogue();
    let generation = fixture.handle.launch_app(APP).expect("launch");
    observe_focused(&fixture.handle, generation, 1);

    let mut forwarded = fixture.handle.diagnostics().input_forwarded;
    // Focus, move, grow, shrink on H: one forward each, serialized.
    for flags in [CTRL, CTRL | SHIFT, CTRL | ALT, CTRL | ALT | SHIFT] {
        send_key(&fixture, flags, 0x04);
        forwarded = wait_forwarded(&fixture.handle, forwarded);
    }
    assert!(fixture.handle.flush(FLUSH));
    assert!(reply_count(&fixture.handle, "focus-") >= 1);
    assert!(reply_count(&fixture.handle, "move-") >= 1);
    assert!(reply_count(&fixture.handle, "resize-") >= 1);

    // Inexact variants never forward: extra Cmd on any arm, bare Alt+H
    // (no Ctrl role), plain H, and Alt+Shift+H without Ctrl.
    let ignored_baseline = fixture.handle.diagnostics().input_ignored;
    send_key(&fixture, CTRL | CMD, 0x04);
    send_key(&fixture, CTRL | ALT | CMD, 0x04);
    send_key(&fixture, CTRL | SHIFT | ALT | CMD, 0x04);
    send_key(&fixture, ALT, 0x04);
    send_key(&fixture, ALT | SHIFT, 0x04);
    send_key(&fixture, 0, 0x04);
    assert!(fixture.handle.flush(FLUSH));
    // All six ignored chords must reach the engine; forwarded must not move.
    let diagnostics = wait_ignored(&fixture.handle, ignored_baseline + 6, "six inexact chords");
    assert_eq!(diagnostics.input_forwarded, forwarded);
    assert!(
        diagnostics.input_ignored >= ignored_baseline + 6,
        "all six must ignore: {diagnostics:?}"
    );

    let report = fixture.handle.shutdown(Duration::from_secs(5));
    assert_eq!(report.outcome, ShutdownOutcome::Clean, "{report:?}");
}

#[test]
fn catalogue_resize_grow_shrink_moves_frames_by_shared_steps() {
    // Geometry outcomes, not implementation mirrors: grow-left widens the
    // focused tile along its interior boundary, a held repeat grows it
    // further (press-index schedule advances), and shrink-left narrows it
    // again (mode change restarts the shared 12px first-press schedule).
    // The admitted skeleton layout parks the focused tile top-right, so the
    // interior edge is its left one (grow-right would be the outward
    // work-area-edge no-op, refused with no writes per R-RSZ-02).
    const CTRL: u64 = 0x0004_0000;
    const SHIFT: u64 = 0x0002_0000;
    let fixture = start_catalogue();
    let generation = fixture.handle.launch_app(APP).expect("launch");
    observe_focused(&fixture.handle, generation, 1);
    assert!(fixture.handle.flush(FLUSH));

    let w0 = desired_width(&fixture.handle, "1001_1").expect("planned frame for 1001_1");

    // Grow left (H): first press plans and widens by the shared 12px step.
    let baseline = fixture.handle.diagnostics().input_forwarded;
    send_key(&fixture, CTRL | ALT, 0x04);
    wait_forwarded(&fixture.handle, baseline);
    assert!(fixture.handle.flush(FLUSH));
    assert!(reply_count(&fixture.handle, "resize-") >= 1);
    let w1 = desired_width(&fixture.handle, "1001_1").expect("frame after grow");
    assert_eq!(w1, w0 + 12, "first grow press moves the shared 12px step");

    // Held repeat: the schedule advances (second press is the 14px step).
    let baseline = fixture.handle.diagnostics().input_forwarded;
    send_key(&fixture, CTRL | ALT, 0x04);
    wait_forwarded(&fixture.handle, baseline);
    assert!(fixture.handle.flush(FLUSH));
    let w2 = desired_width(&fixture.handle, "1001_1").expect("frame after repeat");
    assert_eq!(w2, w1 + 14, "held repeat advances to the shared 14px step");

    // Shrink left (same edge, inwards): mode change restarts the schedule
    // at 12px, narrowing back toward the start.
    let baseline = fixture.handle.diagnostics().input_forwarded;
    send_key(&fixture, CTRL | ALT | SHIFT, 0x04);
    wait_forwarded(&fixture.handle, baseline);
    assert!(fixture.handle.flush(FLUSH));
    let w3 = desired_width(&fixture.handle, "1001_1").expect("frame after shrink");
    assert_eq!(w3, w2 - 12, "mode change restarts the shared schedule");

    let report = fixture.handle.shutdown(Duration::from_secs(5));
    assert_eq!(report.outcome, ShutdownOutcome::Clean, "{report:?}");
}

#[test]
fn catalogue_orientation_toggles_once_per_press_with_frame_change() {
    // The orientation toggle reflows planned frames, and a held repeat
    // consumes without dispatch until the `O` key comes back up
    // (Windows R-LAY-01 one-per-press parity).
    const CTRL: u64 = 0x0004_0000;
    let fixture = start_catalogue();
    let generation = fixture.handle.launch_app(APP).expect("launch");
    observe_focused(&fixture.handle, generation, 1);
    assert!(fixture.handle.flush(FLUSH));
    let before = desired_snapshot(&fixture.handle);
    assert!(!before.is_empty());

    // First discrete press toggles and reflows frames.
    let baseline = fixture.handle.diagnostics().input_forwarded;
    send_key(&fixture, CTRL, KEY_O);
    wait_forwarded(&fixture.handle, baseline);
    assert!(fixture.handle.flush(FLUSH));
    assert!(reply_count(&fixture.handle, "toggle-orientation-") >= 1);
    let toggled = desired_snapshot(&fixture.handle);
    assert_ne!(toggled, before, "orientation toggle must reflow frames");

    // Held repeat (no key-up between downs): consumed, never dispatched.
    let ignored_baseline = fixture.handle.diagnostics().input_ignored;
    let toggles = reply_count(&fixture.handle, "toggle-orientation-");
    send_key(&fixture, CTRL, KEY_O);
    assert!(fixture.handle.flush(FLUSH));
    // The held repeat must reach the engine as an ignore (ignored count is
    // the arrival signal); nothing may forward or dispatch from it.
    wait_ignored(&fixture.handle, ignored_baseline + 1, "held repeat");
    assert_eq!(fixture.handle.diagnostics().input_forwarded, baseline + 1);
    assert!(
        fixture.handle.diagnostics().input_ignored > ignored_baseline,
        "held repeat must ignore"
    );
    assert_eq!(
        reply_count(&fixture.handle, "toggle-orientation-"),
        toggles,
        "held repeat must not dispatch"
    );

    // Release then repress: toggles again.
    send_key_up(&fixture, CTRL, KEY_O);
    assert!(fixture.handle.flush(FLUSH));
    let baseline = fixture.handle.diagnostics().input_forwarded;
    send_key(&fixture, CTRL, KEY_O);
    wait_forwarded(&fixture.handle, baseline);
    assert!(fixture.handle.flush(FLUSH));
    assert!(reply_count(&fixture.handle, "toggle-orientation-") > toggles);

    let report = fixture.handle.shutdown(Duration::from_secs(5));
    assert_eq!(report.outcome, ShutdownOutcome::Clean, "{report:?}");
}
