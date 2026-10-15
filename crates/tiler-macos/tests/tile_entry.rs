//! Scoped-tile entry tests (offline, portable).
//!
//! Fakes drive the runtime through the tile catalogue surface: exact
//! matcher/action forwarding per arm (focus/move/float with directions,
//! Shift-arm gating, extras and key-ups ignored), CLI/request safety
//! (explicit start, non-empty scope, Ghostty, finite lifetime), and
//! catalogue config validation. No live AX, no taps, no Carbon, no Spaces,
//! no desktop effects — the built tile is never launched here.

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
use tiler_macos::tile::{TileInput, TileRequest};

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
    // Sequence 2: the worker's own startup observation takes 1, so this
    // superseedes it (either order converges: a later startup seq is stale
    // and rejects, an earlier one is replaced by this).
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

fn wait_forwarded(handle: &RuntimeHandle, baseline: u64) -> u64 {
    let deadline = std::time::Instant::now() + WAIT;
    loop {
        let forwarded = handle.diagnostics().input_forwarded;
        if forwarded > baseline {
            return forwarded;
        }
        if std::time::Instant::now() >= deadline {
            panic!("input never forwarded (baseline {baseline})");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
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
    std::thread::sleep(Duration::from_millis(100));
    let diagnostics = fixture.handle.diagnostics();
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
    } = parsed.command
    else {
        panic!("expected tile, got {:?}", parsed.command);
    };
    TileRequest {
        user_start,
        scopes,
        seconds,
        input,
        modifier: parsed.modifier,
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

// -- Catalogue config validation ----------------------------------------------

#[test]
fn catalogue_config_validates() {
    let good = RuntimeConfig {
        tile_bindings: Some(tile_catalogue()),
        ..RuntimeConfig::default()
    };
    assert!(good.validate().is_ok());
    assert_eq!(good.tile_bindings.as_ref().map(|b| b.len()), Some(17));

    // Shift-as-Meta collapses the focus/move arms to the same native chord.
    // Refuse it before registering rather than silently choosing focus.
    let collapsed = RuntimeConfig {
        modifier: ProbeModifier::Shift,
        tile_bindings: Some(tile_catalogue()),
        ..RuntimeConfig::default()
    };
    assert!(collapsed.validate().is_err());
    for modifier in [ProbeModifier::Ctrl, ProbeModifier::Meta, ProbeModifier::Alt] {
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

    // Duplicate (key, shift) refuses.
    let mut duplicated = tile_catalogue();
    duplicated.push(tiler_macos::runtime::TileBinding {
        key_code: 0x04,
        shift: false,
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
