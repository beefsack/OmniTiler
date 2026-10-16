#![cfg(windows)]

use std::path::{Path, PathBuf};
use tiler_windows::settings::{
    BindingState, LiveSettings, LoadOutcome, SETTINGS_FILE_NAME, Settings, compatible_disabled_ids,
    compatible_rebinds, load_from_dir, validate_settings,
};
use tiler_windows::tiling::{CliOverrides, tile_options_defaults};
use tiler_windows::tiling_sys::resolve_first_run_outcome;
use tiler_windows::tray::{FirstRunChoice, settings_for_choice, unresolved_conflicts};

static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct Temp {
    path: PathBuf,
}

impl Temp {
    fn new(name: &str) -> Self {
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "tiler-first-run-{name}-{}-{t}-{n}",
            std::process::id()
        ));
        std::fs::create_dir(&path).expect("temp dir");
        Self { path }
    }

    fn settings_file(&self) -> PathBuf {
        self.path.join(SETTINGS_FILE_NAME)
    }

    fn log_file(&self) -> PathBuf {
        self.path.join("first-run-test.log")
    }

    fn log_text(&self) -> String {
        std::fs::read_to_string(self.log_file()).unwrap_or_default()
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn choice_bytes(choice: FirstRunChoice) -> Vec<u8> {
    let settings = settings_for_choice(choice);
    assert!(validate_settings(&settings).is_ok());
    serde_json::to_vec_pretty(&settings).expect("serialize")
}

/// Drive the real production outcome path: real settings dir, real log file,
/// real atomic publish. `seed` pre-creates a settings file to simulate a
/// mid-prompt appearance (or a lost create race when `file_missing_after` is
/// forced true).
#[allow(clippy::too_many_arguments)]
fn run_outcome(
    sdir: &Path,
    log: &Path,
    choice: Option<FirstRunChoice>,
    file_missing_after: bool,
    stop_during: bool,
    seed: Option<&[u8]>,
) -> tiler_windows::tiling_sys::FirstRunResolution {
    if let Some(bytes) = seed {
        std::fs::write(sdir.join(SETTINGS_FILE_NAME), bytes).expect("seed");
    }
    let options = tile_options_defaults();
    resolve_first_run_outcome(
        sdir,
        choice,
        file_missing_after,
        stop_during,
        &options,
        &CliOverrides::default(),
        Some(LiveSettings::fresh(Settings::default(), None)),
        Some(sdir.to_path_buf()),
        log,
    )
    .expect("outcome resolves")
}

#[test]
fn stop_cancels_pending_authentic_choice_without_publish() {
    let temp = Temp::new("stop-authentic");
    let resolved = run_outcome(
        &temp.path,
        &temp.log_file(),
        Some(FirstRunChoice::Authentic),
        true,
        true,
        None,
    );
    assert_eq!(
        resolved.first_run.as_deref(),
        Some("first-run:cancelled:stopped")
    );
    assert!(
        !temp.settings_file().exists(),
        "cancelled stop publishes no settings file"
    );
    assert!(
        temp.log_text().contains("first-run-cancelled"),
        "cancellation is logged"
    );
}

#[test]
fn stop_cancels_pending_compatible_choice_without_publish() {
    let temp = Temp::new("stop-compatible");
    let resolved = run_outcome(
        &temp.path,
        &temp.log_file(),
        Some(FirstRunChoice::Compatible),
        true,
        true,
        None,
    );
    assert_eq!(
        resolved.first_run.as_deref(),
        Some("first-run:cancelled:stopped")
    );
    assert!(
        !temp.settings_file().exists(),
        "cancelled stop publishes no settings file"
    );
}

#[test]
fn dismissed_choice_writes_nothing_through_production_path() {
    let temp = Temp::new("dismissed");
    let resolved = run_outcome(&temp.path, &temp.log_file(), None, true, false, None);
    assert_eq!(
        resolved.first_run.as_deref(),
        Some("first-run:dismissed:unsaved")
    );
    assert!(!temp.settings_file().exists());
}

#[test]
fn authentic_choice_publishes_through_production_path() {
    let temp = Temp::new("publish-authentic");
    let resolved = run_outcome(
        &temp.path,
        &temp.log_file(),
        Some(FirstRunChoice::Authentic),
        true,
        false,
        None,
    );
    assert_eq!(
        resolved.first_run.as_deref(),
        Some("first-run:authentic:saved")
    );
    let bytes = std::fs::read(temp.settings_file()).expect("published file");
    assert_eq!(bytes, choice_bytes(FirstRunChoice::Authentic));
    match load_from_dir(&temp.path) {
        LoadOutcome::Loaded(settings) => {
            assert!(validate_settings(&settings).is_ok());
            assert_eq!(settings, settings_for_choice(FirstRunChoice::Authentic));
        }
        other => panic!("published file must load, got {other:?}"),
    }
    assert!(!temp.log_text().contains("first-run-cancelled"));
}

#[test]
fn compatible_choice_publishes_rebound_preset_through_production_path() {
    let temp = Temp::new("publish-compatible");
    let resolved = run_outcome(
        &temp.path,
        &temp.log_file(),
        Some(FirstRunChoice::Compatible),
        true,
        false,
        None,
    );
    assert_eq!(
        resolved.first_run.as_deref(),
        Some("first-run:compatible:saved")
    );
    match load_from_dir(&temp.path) {
        LoadOutcome::Loaded(settings) => {
            assert!(validate_settings(&settings).is_ok());
            assert_eq!(
                settings.bindings.len(),
                compatible_disabled_ids().len() + compatible_rebinds().len()
            );
            for (id, chord) in compatible_rebinds() {
                let entry = settings.bindings.get(id).expect("rebind entry");
                assert_eq!(entry.state, BindingState::Rebind, "{id}");
                assert_eq!(entry.chord.as_deref(), Some(chord), "{id}");
            }
            assert!(unresolved_conflicts(&settings, true, false).is_empty());
            assert!(unresolved_conflicts(&settings, true, true).is_empty());
        }
        other => panic!("published file must load, got {other:?}"),
    }
}

#[test]
fn mid_prompt_file_discards_choice_and_keeps_existing_bytes() {
    let temp = Temp::new("appeared");
    let first = choice_bytes(FirstRunChoice::Authentic);
    let resolved = run_outcome(
        &temp.path,
        &temp.log_file(),
        Some(FirstRunChoice::Compatible),
        false,
        false,
        Some(&first),
    );
    assert_eq!(
        resolved.first_run.as_deref(),
        Some("first-run:discarded:present")
    );
    assert_eq!(
        std::fs::read(temp.settings_file()).expect("bytes"),
        first,
        "existing file is never overwritten"
    );
    assert!(temp.log_text().contains("first-run-discarded"));
}

#[test]
fn lost_create_race_discards_choice_and_keeps_winner_bytes() {
    let temp = Temp::new("race-lost");
    // File reported missing at recheck, then a winner lands before publish:
    // the atomic create-if-absent refuses with Pending and the choice is
    // discarded instead of overwriting.
    let winner = choice_bytes(FirstRunChoice::Authentic);
    let resolved = run_outcome(
        &temp.path,
        &temp.log_file(),
        Some(FirstRunChoice::Compatible),
        true,
        false,
        Some(&winner),
    );
    assert_eq!(
        resolved.first_run.as_deref(),
        Some("first-run:discarded:raced")
    );
    assert_eq!(
        std::fs::read(temp.settings_file()).expect("bytes"),
        winner,
        "race winner bytes are never replaced"
    );
    assert!(temp.log_text().contains("first-run-discarded"));
}

#[test]
fn first_run_concurrent_publish_has_single_winner() {
    let temp = Temp::new("concurrent");
    let bodies = [
        choice_bytes(FirstRunChoice::Authentic),
        choice_bytes(FirstRunChoice::Compatible),
    ];
    let dir = &temp.path;
    let results: Vec<_> = std::thread::scope(|scope| {
        bodies
            .iter()
            .enumerate()
            .map(|(i, body)| {
                scope.spawn(move || {
                    tiler_windows::storage::publish_no_overwrite(
                        dir,
                        SETTINGS_FILE_NAME,
                        body,
                        &format!("t-{i}"),
                    )
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|h| h.join().expect("thread"))
            .collect()
    });
    let wins = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(wins, 1, "exactly one publisher wins");
    let final_bytes = std::fs::read(temp.settings_file()).expect("bytes");
    assert!(
        bodies.iter().any(|b| b == &final_bytes),
        "final must equal one full published body"
    );
}
