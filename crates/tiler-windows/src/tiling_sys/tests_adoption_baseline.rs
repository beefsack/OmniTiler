#![forbid(unsafe_code)]
//! R14 adoption-time native baseline regressions (offline). Capture is
//! read-only over the application window; these tests use the crate's own
//! overlapped test window for the real native reads and never touch a window
//! the project does not own.

use tiler_core::geometry::Rect;

use super::adoption_baseline::{
    AdoptionBaseline, CaptureOutcome, capture, load, persist_first, prune_dead, trace_line,
};
use super::fullscreen::{FULLSCREEN_STYLE_BITS, FullscreenMeta};
use super::fullscreen_preimage::{
    FullscreenPreimage, clear as clear_preimage, load as load_preimage,
};
use crate::tiling::{FrameInsets, ObservedTarget, WindowFacts};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn frame() -> Rect {
    rect(-20, 30, 800, 600)
}

fn row(hwnd: u64, tag: &str, chrome: u32, was_maximized: bool, outer: Rect) -> AdoptionBaseline {
    AdoptionBaseline::new(
        hwnd,
        7,
        "creation-a",
        tag,
        "session-a",
        chrome,
        was_maximized,
        outer,
    )
    .expect("row")
}

fn self_creation() -> String {
    crate::native::HeldProcess::open(std::process::id())
        .expect("self process")
        .identity()
        .expect("self identity")
        .process_creation
}

fn temp_store(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "omnitester-adoption-baseline-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn facts() -> WindowFacts {
    WindowFacts {
        visible: true,
        minimized: false,
        maximized: false,
        cloaked: false,
        elevated: false,
        shell: false,
        tool_window: false,
        owned: false,
        captionless_fullscreen: false,
        no_activate: false,
        topmost: false,
        dialog: false,
    }
}

fn observed(hwnd: u64, outer: Rect, facts: WindowFacts) -> super::ObservedWindow {
    super::ObservedWindow {
        hwnd,
        token: "w-test".to_owned(),
        outer,
        visible: outer,
        insets: FrameInsets::measure(outer, outer),
        identity: ObservedTarget {
            hwnd,
            pid: std::process::id(),
            process_creation: self_creation(),
            exe_path: String::new(),
            user_sid: String::new(),
            session_id: 0,
            tag: String::new(),
        },
        facts,
        member_tag: None,
    }
}

/// Own crates's previewable overlapped window plus a real member tag: the
/// same native reads production capture performs, on a window the project
/// owns and never tiles.
fn tagged_window(name: &str) -> (crate::ffi::FfiOwnedTestWindow, u64, String) {
    let window = crate::ffi::FfiOwnedTestWindow::create(
        name, 100, 120, 800, 600,
        // The capture path trusts the observed visibility fact; the fixture
        // stays invisible so no real desktop window appears.
        false,
    )
    .expect("owned window");
    let hwnd = window.hwnd();
    let tag =
        crate::product_hide::sys::ensure_member_tag(hwnd, std::process::id()).expect("member tag");
    (window, hwnd, tag)
}

#[test]
fn baseline_row_validates_and_round_trips_exactly() {
    let normal = row(10, "tag-a", FULLSCREEN_STYLE_BITS, false, frame());
    assert_eq!(
        AdoptionBaseline::from_json(&normal.to_json()),
        Some(normal.clone())
    );
    // Borderless-windowed apps have no owned chrome bits and still have a
    // valid baseline; a maximized entry carries its own actual outer.
    let borderless = row(11, "tag-b", 0, false, frame());
    assert_eq!(
        AdoptionBaseline::from_json(&borderless.to_json()),
        Some(borderless.clone())
    );
    let maximized = row(
        12,
        "tag-c",
        FULLSCREEN_STYLE_BITS,
        true,
        rect(0, 0, 1920, 1040),
    );
    assert_eq!(
        AdoptionBaseline::from_json(&maximized.to_json()),
        Some(maximized)
    );

    // Construction refuses empty identity, foreign chrome bits, and a
    // degenerate outer rectangle.
    assert!(AdoptionBaseline::new(1, 1, "", "tag", "session", 0, false, frame()).is_none());
    assert!(AdoptionBaseline::new(1, 1, "creation", "", "session", 0, false, frame()).is_none());
    assert!(AdoptionBaseline::new(1, 1, "creation", "tag", "", 0, false, frame()).is_none());
    assert!(
        AdoptionBaseline::new(
            1,
            1,
            "creation",
            "tag",
            "session",
            0x8000_0000,
            false,
            frame()
        )
        .is_none()
    );
    assert!(
        AdoptionBaseline::new(
            1,
            1,
            "creation",
            "tag",
            "session",
            0,
            false,
            rect(0, 0, 0, 100)
        )
        .is_none()
    );

    // Decode refuses the same corrupt shapes and a wrong document version.
    let mut wrong_version = normal.to_json();
    wrong_version["v"] = serde_json::json!(2);
    assert!(AdoptionBaseline::from_json(&wrong_version).is_none());
    let mut foreign_bits = normal.to_json();
    foreign_bits["chrome"] = serde_json::json!(0x8000_0000_u32);
    assert!(AdoptionBaseline::from_json(&foreign_bits).is_none());
    let mut missing_outer = normal.to_json();
    missing_outer["outer"] = serde_json::Value::Null;
    assert!(AdoptionBaseline::from_json(&missing_outer).is_none());
}

#[test]
fn first_capture_wins_and_storage_is_lifetime_exact() {
    let dir = temp_store("first-wins");
    let first = row(10, "tag-a", FULLSCREEN_STYLE_BITS, false, frame());
    assert_eq!(persist_first(&dir, &first), Ok(true));
    // Adoption-time evidence is never rewritten by a later observation,
    // even one that would look plausible.
    let later = row(10, "tag-a", 0, true, rect(0, 0, 1920, 1040));
    assert_eq!(persist_first(&dir, &later), Ok(false));
    assert_eq!(
        load(&dir, 10, 7, "creation-a", "tag-a"),
        Some(first.clone())
    );

    // Recycled handle or recycled process: no exact-lifetime match.
    assert!(load(&dir, 10, 7, "creation-b", "tag-a").is_none());
    assert!(load(&dir, 10, 8, "creation-a", "tag-a").is_none());
    assert!(load(&dir, 11, 7, "creation-a", "tag-a").is_none());

    // Same window object with a fresh membership tag is a new generation:
    // the stale row is replaced instead of accumulating forever.
    let reused = row(
        10,
        "tag-b",
        FULLSCREEN_STYLE_BITS,
        false,
        rect(20, 20, 500, 400),
    );
    assert_eq!(persist_first(&dir, &reused), Ok(true));
    assert!(load(&dir, 10, 7, "creation-a", "tag-a").is_none());
    assert_eq!(load(&dir, 10, 7, "creation-a", "tag-b"), Some(reused));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn malformed_store_is_unreadable_and_never_overwritten() {
    let dir = temp_store("malformed");
    let path = dir.join("adoption-baselines.json");
    let corrupt = br#"{"v":1,"rows":[{"v":1}]}"#;
    std::fs::write(&path, corrupt).expect("write corrupt store");
    // A broken slot is never treated as empty: capture fails closed and
    // pruning leaves the bytes for a later manual/next-start attempt.
    assert_eq!(
        persist_first(&dir, &row(10, "tag-a", 0, false, frame())),
        Err(())
    );
    assert_eq!(prune_dead(&dir), 0);
    assert_eq!(std::fs::read(&path).expect("store present"), corrupt);
    // A future document version is equally unreadable.
    std::fs::write(&path, br#"{"v":2,"rows":[]}"#).expect("write future store");
    assert_eq!(
        persist_first(&dir, &row(10, "tag-a", 0, false, frame())),
        Err(())
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn pruning_drops_only_provably_gone_lifetimes() {
    let dir = temp_store("prune");
    let (_window, hwnd, tag) = tagged_window("adoption-prune");
    let creation = self_creation();
    let pid = std::process::id();
    let live = AdoptionBaseline::new(
        hwnd,
        pid,
        &creation,
        &tag,
        "session-a",
        0,
        false,
        rect(100, 120, 800, 600),
    )
    .expect("live row");
    assert_eq!(persist_first(&dir, &live), Ok(true));
    // Closed lifetime: the handle is gone.
    let gone = row(0, "tag-gone", 0, false, frame());
    assert_eq!(persist_first(&dir, &gone), Ok(true));
    // Live window, live process, but an unverifiable tag: not provably dead,
    // so the row waits for a later attempt rather than being deleted. Use a
    // second window so this exercises pruning, not same-window replacement.
    let other = crate::ffi::TestWindow::create_message_only(
        "OmniTiler.AdoptionBaselinePruneTest",
        "adoption-prune-other",
    )
    .expect("second window");
    let unverifiable = AdoptionBaseline::new(
        other.hwnd(),
        pid,
        &creation,
        "ffffffffffffffff",
        "session-a",
        0,
        false,
        rect(100, 120, 800, 600),
    )
    .expect("row");
    assert_eq!(persist_first(&dir, &unverifiable), Ok(true));

    // Same-process HWND reuse: a live window now carries a different tag, so
    // the stored generation can never verify again.
    let (_reused, reused_hwnd, reused_tag) = tagged_window("adoption-prune-reused");
    let stale_tag = if reused_tag == "ffffffffffffffff" {
        "0000000000000000"
    } else {
        "ffffffffffffffff"
    };
    let stale = AdoptionBaseline::new(
        reused_hwnd,
        pid,
        &creation,
        stale_tag,
        "session-a",
        0,
        false,
        rect(100, 120, 800, 600),
    )
    .expect("stale row");
    assert_eq!(persist_first(&dir, &stale), Ok(true));

    assert_eq!(prune_dead(&dir), 2, "gone and reused lifetimes are pruned");
    assert_eq!(load(&dir, hwnd, pid, &creation, &tag), Some(live));
    assert_eq!(
        load(&dir, other.hwnd(), pid, &creation, "ffffffffffffffff"),
        Some(unverifiable)
    );
    assert!(load(&dir, reused_hwnd, pid, &creation, stale_tag).is_none());
    assert!(load(&dir, 0, 7, "creation-a", "tag-gone").is_none());
    // A second pass is a no-op.
    assert_eq!(prune_dead(&dir), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn capture_records_one_settled_windowed_baseline() {
    let dir = temp_store("capture");
    let (_window, hwnd, tag) = tagged_window("adoption-capture");
    let outer = rect(100, 120, 800, 600);
    let creation = self_creation();
    let pid = std::process::id();
    let style = crate::ffi::window_style_checked(hwnd).expect("style");
    let adopted = observed(hwnd, outer, facts());
    assert_eq!(capture(&dir, &adopted), CaptureOutcome::Captured);

    let stored = load(&dir, hwnd, pid, &creation, &tag).expect("baseline row");
    assert_eq!(stored.chrome, style & FULLSCREEN_STYLE_BITS);
    assert!(!stored.was_maximized);
    assert_eq!(stored.outer, outer);
    assert!(!stored.session.is_empty());

    // The next observation cannot overwrite adoption-time evidence.
    let moved = observed(hwnd, rect(300, 300, 500, 400), facts());
    assert_eq!(capture(&dir, &moved), CaptureOutcome::Present);
    assert_eq!(
        load(&dir, hwnd, pid, &creation, &tag).map(|row| row.outer),
        Some(outer)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn capture_refuses_fullscreen_and_project_owned_frames() {
    let dir = temp_store("capture-refuse");
    let (_window, hwnd, tag) = tagged_window("adoption-refuse");
    let outer = rect(100, 120, 800, 600);
    let creation = self_creation();
    let pid = std::process::id();

    // First-seen fullscreen: no adoption baseline, refusal unchanged.
    let mut fullscreen = facts();
    fullscreen.captionless_fullscreen = true;
    assert_eq!(
        capture(&dir, &observed(hwnd, outer, fullscreen)),
        CaptureOutcome::Skipped("fullscreen")
    );
    assert!(load(&dir, hwnd, pid, &creation, &tag).is_none());

    // Project-owned fullscreen marker: the frame is already project state,
    // never an application baseline.
    assert!(super::fullscreen::write_fullscreen_meta(
        hwnd,
        &FullscreenMeta {
            changed: FULLSCREEN_STYLE_BITS,
            was_maximized: true,
            frame: None,
        },
    ));
    assert_eq!(
        capture(&dir, &observed(hwnd, outer, facts())),
        CaptureOutcome::Skipped("owned-marker")
    );
    assert!(load(&dir, hwnd, pid, &creation, &tag).is_none());
    assert!(super::fullscreen::clear_fullscreen_meta(hwnd));

    // Coordinated with R12: a pending chrome preimage for the exact lifetime
    // keeps the adoption baseline absent while the slot still exists.
    let preimage = FullscreenPreimage::new(
        hwnd,
        pid,
        &creation,
        &tag,
        "session-a",
        FULLSCREEN_STYLE_BITS,
        false,
        Some(outer),
    )
    .expect("preimage row");
    super::fullscreen_preimage::persist(&dir, &preimage).expect("persist preimage");
    assert_eq!(
        capture(&dir, &observed(hwnd, outer, facts())),
        CaptureOutcome::Skipped("r12-preimage")
    );
    assert!(load(&dir, hwnd, pid, &creation, &tag).is_none());
    assert_eq!(
        load_preimage(&dir, hwnd, pid, &creation, &tag),
        Some(preimage)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn baseline_slot_is_separate_from_the_r12_preimage_slot() {
    let dir = temp_store("separate-slots");
    let baseline = row(10, "tag-a", FULLSCREEN_STYLE_BITS, false, frame());
    let preimage = FullscreenPreimage::new(
        10,
        7,
        "creation-a",
        "tag-a",
        "session-a",
        FULLSCREEN_STYLE_BITS,
        false,
        Some(frame()),
    )
    .expect("preimage row");
    assert_eq!(persist_first(&dir, &baseline), Ok(true));
    super::fullscreen_preimage::persist(&dir, &preimage).expect("persist preimage");
    assert!(dir.join("adoption-baselines.json").exists());
    assert!(dir.join("fullscreen-preimages.json").exists());
    assert_eq!(load(&dir, 10, 7, "creation-a", "tag-a"), Some(baseline));
    assert_eq!(
        load_preimage(&dir, 10, 7, "creation-a", "tag-a"),
        Some(preimage)
    );
    // Retiring the project chrome preimage never retires adoption evidence.
    clear_preimage(&dir, 10, 7, "creation-a", "tag-a").expect("clear preimage");
    assert!(load_preimage(&dir, 10, 7, "creation-a", "tag-a").is_none());
    assert!(load(&dir, 10, 7, "creation-a", "tag-a").is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn adoption_trace_is_presence_only() {
    let captured = trace_line("w1", CaptureOutcome::Captured);
    assert_eq!(captured["event"], "adoption-baseline");
    assert_eq!(captured["window"], "w1");
    assert_eq!(captured["outcome"], "captured");
    let present = trace_line("w2", CaptureOutcome::Present);
    assert_eq!(present["outcome"], "present");
    let skipped = trace_line("w3", CaptureOutcome::Skipped("fullscreen"));
    assert_eq!(skipped["outcome"], "skipped");
    assert_eq!(skipped["reason"], "fullscreen");

    // Bounded: no raw native identity, geometry, titles, or executable text.
    for line in [captured, present, skipped] {
        let mut keys: Vec<&str> = line
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        let expected: &[&str] = if keys.len() == 4 {
            &["event", "outcome", "reason", "window"]
        } else {
            &["event", "outcome", "window"]
        };
        assert_eq!(
            keys.as_slice(),
            expected,
            "bounded trace vocabulary: {line}"
        );
        for forbidden in ["hwnd", "pid", "creation", "tag", "outer", "title", "exe"] {
            assert!(
                line.get(forbidden).is_none(),
                "trace never carries {forbidden}: {line}"
            );
        }
    }
}
