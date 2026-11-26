//! Mechanical split of `fixture_leg.rs` cfg tests (no behavior change).

use super::*;

fn test_opts(chords: Vec<&str>) -> FixtureLegOpts {
    FixtureLegOpts {
        seconds: 90,
        chords: chords.into_iter().map(str::to_owned).collect(),
        preset: InputPreset::Compatible,
        modifier: ProbeModifier::Ctrl,
        signer: "designated => test".to_owned(),
        focus_follows_cursor: false,
        hover_probe: false,
        corrupt_intent_store: false,
    }
}

#[test]
fn leg_bounds_reject_before_effects() {
    // Usage shapes refuse purely: no baseline, no run dir, no launch.
    // The leg max reuses the product bound exactly (no hardcoded
    // mismatch): the seconds ride verbatim into the product flag.
    assert_eq!(LEG_MAX_SECONDS, crate::tile::MAX_TILE_SECONDS);
    assert_eq!(LEG_MIN_SECONDS, 60);
    let mut bad_seconds = test_opts(vec!["focus-h"]);
    bad_seconds.seconds = 10;
    assert!(run_fixture_leg(bad_seconds).unwrap_err().0 == 2);
    let mut over_seconds = test_opts(vec!["focus-h"]);
    over_seconds.seconds = LEG_MAX_SECONDS + 1;
    let err = run_fixture_leg(over_seconds).unwrap_err();
    assert_eq!(err.0, 2);
    let no_chords = test_opts(vec![]);
    assert!(run_fixture_leg(no_chords).unwrap_err().0 == 2);
    let bad_chord = test_opts(vec!["teleport"]);
    let err = run_fixture_leg(bad_chord).unwrap_err();
    assert_eq!(err.0, 2);
    assert!(err.1.contains("teleport"));
    // The hover probe needs one tall anchor plus a stacked pair: fewer
    // chords refuse with usage (exit 2) before any run dir or effect.
    let mut short_hover = test_opts(vec!["focus-h", "focus-j"]);
    short_hover.hover_probe = true;
    let err = run_fixture_leg(short_hover).unwrap_err();
    assert_eq!(err.0, 2);
    assert!(err.1.contains("--hover-probe"), "{}", err.1);
}

#[test]
fn chord_post_waits_for_focus_eligibility() {
    // The product consumes owned chords before dispatch and refuses a
    // basic hit until its eligibility attest catches the fresh focus, so
    // the runner must pace between the fixture-side focus and the post.
    // The leg source is production-only now that its tests live in
    // `fixture_leg_tests.rs`.
    let source = include_str!("fixture_leg.rs");
    let focus_at = source.find("driver.focus(caller, *wid)?;").expect("focus");
    let pace_at = source
        .find("settle_hold(FOCUS_SETTLE_MS)?;")
        .expect("focus pacing");
    let post_at = source
        .find("driver.post(caller, *wid, chord")
        .expect("post");
    assert!(focus_at < pace_at && pace_at < post_at);
    // 500ms constant is documented above the runner's constant; the
    // ordering is the code fact this test pins.
}

#[test]
fn appkit_singleton_precedes_the_baseline() {
    // `NSScreen.screensHaveSeparateSpaces` answers with a stale `false`
    // until AppKit's shared application exists; a baseline captured
    // before that stores an untruthful preimage and the unchanged
    // Mission Control setting then compares unequal at teardown.
    // The leg source is production-only now that its tests live in
    // `fixture_leg_tests.rs`.
    let source = include_str!("fixture_leg.rs");
    let init_at = source
        .find("ensure_appkit_singleton();")
        .expect("fixture leg initializes the AppKit singleton");
    let begin_at = source
        .find("LiveLeg::begin(")
        .expect("fixture leg captures its baseline");
    assert!(
        init_at < begin_at,
        "AppKit singleton must exist before the baseline preimage"
    );
}

#[test]
fn settle_window_is_bounded_and_lifetime_fitted() {
    // Unset/empty/zero: no hold at all (unchanged leg timing).
    assert_eq!(settle_ms_from(None, 90).expect("unset"), 0);
    assert_eq!(settle_ms_from(Some(""), 90).expect("empty"), 0);
    assert_eq!(settle_ms_from(Some("  "), 90).expect("blank"), 0);
    assert_eq!(settle_ms_from(Some("0"), 90).expect("zero"), 0);
    // Valid windows parse exactly (trimmed).
    assert_eq!(settle_ms_from(Some(" 15000 "), 90).expect("valid"), 15_000);
    assert_eq!(settle_ms_from(Some("20000"), 90).expect("max"), 20_000);
    // Non-numeric and over-bound values refuse before any effect.
    assert!(settle_ms_from(Some("1s"), 90).is_err());
    assert!(settle_ms_from(Some("-5"), 90).is_err());
    assert!(settle_ms_from(Some("20001"), 120).is_err());
    // Both holds plus the shutdown margin must fit the product lifetime.
    assert!(settle_ms_from(Some("20000"), 49).is_err());
    assert_eq!(settle_ms_from(Some("20000"), 50).expect("fits"), 20_000);
    assert!(settle_ms_from(Some("15000"), 39).is_err());
    assert_eq!(settle_ms_from(Some("15000"), 40).expect("fits"), 15_000);
}

#[test]
fn product_argv_carries_fixed_scope_and_private_gate_ids() {
    // Production launch args: the fixed fixture scope, the leg seconds
    // verbatim (always product-accepted), the exact-run directory, and
    // the private gate IDs. Never unscoped, never another scope.
    let opts = test_opts(vec!["select-3", "send-3"]);
    let product_dir = std::path::Path::new("/tmp/leg-run/product");
    let argv = product_argv(
        opts.seconds,
        opts.preset,
        opts.modifier,
        opts.focus_follows_cursor,
        product_dir,
        &[1001, 1002],
        Some(11),
    );
    assert_eq!(argv[1], "tile");
    assert!(argv.contains(&"--user-start".to_owned()));
    let scopes: Vec<&String> = argv
        .windows(2)
        .filter(|pair| pair[0] == "--scope")
        .map(|pair| &pair[1])
        .collect();
    assert_eq!(scopes, vec![FIXTURE_SCOPE]);
    assert!(!argv.iter().any(|arg| arg == "--unscoped"));
    let seconds = argv
        .windows(2)
        .find(|pair| pair[0] == "--seconds")
        .map(|pair| pair[1].clone())
        .expect("seconds flag");
    assert_eq!(seconds, opts.seconds.to_string());
    assert!(seconds.parse::<u64>().expect("seconds") <= crate::tile::MAX_TILE_SECONDS);
    let run_dir = argv
        .windows(2)
        .find(|pair| pair[0] == "--run-dir")
        .map(|pair| pair[1].clone())
        .expect("run-dir flag");
    assert_eq!(run_dir, "/tmp/leg-run/product");
    let owned: Vec<u64> = argv
        .windows(2)
        .filter(|pair| pair[0] == "--fixture-owned-space")
        .map(|pair| pair[1].parse().expect("owned SID"))
        .collect();
    assert_eq!(owned, vec![1001, 1002]);
    let starting = argv
        .windows(2)
        .find(|pair| pair[0] == "--fixture-starting-space")
        .map(|pair| pair[1].parse().expect("starting SID"));
    assert_eq!(starting, Some(11));
    // Post-only legs driven by this controller now carry the single
    // created owned SID plus the recorded starting SID, arming the
    // existing product gate (no other argv change).
    let post = test_opts(vec!["focus-h"]);
    let argv = product_argv(
        post.seconds,
        post.preset,
        post.modifier,
        post.focus_follows_cursor,
        product_dir,
        &[2001],
        Some(11),
    );
    let owned: Vec<u64> = argv
        .windows(2)
        .filter(|pair| pair[0] == "--fixture-owned-space")
        .map(|pair| pair[1].parse().expect("owned SID"))
        .collect();
    assert_eq!(owned, vec![2001]);
    let starting = argv
        .windows(2)
        .find(|pair| pair[0] == "--fixture-starting-space")
        .map(|pair| pair[1].parse().expect("starting SID"));
    assert_eq!(starting, Some(11));
    assert!(!argv.iter().any(|arg| arg == "--focus-follows-cursor"));
    let argv = product_argv(
        post.seconds,
        post.preset,
        post.modifier,
        true,
        product_dir,
        &[],
        None,
    );
    assert_eq!(
        argv.iter()
            .filter(|arg| *arg == "--focus-follows-cursor")
            .count(),
        1
    );
}

#[test]
fn completed_envelope_carries_established_owned_space() {
    let evidence = serde_json::json!({
        "display": 0,
        "n": 2,
        "n_kind": "occupied",
        "n_unreadable_windows": 0,
        "sid": 9001,
        "ordinal": 4,
        "starting_sid": 11,
        "spaces_before": [11, 12, 13],
        "spaces_after": [11, 12, 13, 9001],
    });
    let envelope = completed_envelope(
        vec![serde_json::json!({"chord": "move-left"})],
        None,
        Some(evidence.clone()),
        None,
        serde_json::json!({"clean": true}),
    );
    assert_eq!(envelope["outcome"], "completed");
    assert_eq!(envelope["owned_space"], evidence);
    assert!(envelope.get("corrupt_store").is_none());
    // No owned Space (space-mutating/mixed legs) leaves the key absent.
    let envelope = completed_envelope(vec![], None, None, None, serde_json::json!({"clean": true}));
    assert!(envelope.get("owned_space").is_none());
    let store = serde_json::json!({"outcome": "degraded", "diagnostic": "intent-store: test"});
    let envelope = completed_envelope(
        vec![],
        None,
        None,
        Some(store.clone()),
        serde_json::json!({"clean": true}),
    );
    assert_eq!(envelope["corrupt_store"], store);
}

#[test]
fn owned_run_dir_is_private_and_fresh() {
    let dir = create_owned_run_dir("test").expect("run dir creates");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&dir)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
    }
    // A colliding leaf refuses rather than reusing.
    let dir2 = create_owned_run_dir("test").expect("unique leaf creates");
    assert_ne!(dir, dir2);
    let _ = std::fs::remove_dir(&dir);
    let _ = std::fs::remove_dir(&dir2);
}

#[test]
fn scope_is_fixed_to_the_signed_fixture_bundle() {
    assert_eq!(FIXTURE_SCOPE, "io.github.omnitiler.live-fixture");
    assert_eq!(crate::fixture_post::FIXTURE_BUNDLE, FIXTURE_SCOPE);
}

static BUNDLE_TEST_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn bundle_test_parent(tag: &str) -> std::path::PathBuf {
    let id = BUNDLE_TEST_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    std::env::temp_dir().join(format!(
        "fixture-leg-bundle-{tag}-{}-{id}",
        std::process::id()
    ))
}

fn write_test_file(path: &std::path::Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("test directories create");
    }
    std::fs::write(path, contents).expect("test file writes");
}

/// Assemble the signed sibling layout under `parent`: the probe bundle
/// (the controller's own root) plus the fixture sibling, each with its
/// owned marker plus executable. Returns the probe executable path.
fn assemble_sibling_bundles(parent: &std::path::Path) -> std::path::PathBuf {
    let probe_root = parent.join(PRODUCT_BUNDLE_FILE_NAME);
    let probe_exe = probe_root.join(PRODUCT_EXE_REL);
    write_test_file(&probe_exe, "probe");
    write_test_file(
        &probe_root.join(PRODUCT_MARKER_REL),
        &format!("{}\n", crate::BUNDLE_ID),
    );
    let fixture_root = parent.join(FIXTURE_BUNDLE_FILE_NAME);
    write_test_file(&fixture_root.join(FIXTURE_EXE_REL), "fixture");
    write_test_file(
        &fixture_root.join(FIXTURE_MARKER_REL),
        &format!("{FIXTURE_SCOPE}\n"),
    );
    probe_exe
}

#[test]
fn own_bundle_root_pins_app_contents_macos_shape() {
    use std::path::Path;
    assert_eq!(
        own_bundle_root(Path::new(
            "/p/OmniTilerProbe.app/Contents/MacOS/tiler-macos"
        )),
        Some(Path::new("/p/OmniTilerProbe.app").to_path_buf())
    );
    // Developer cargo layout is not a signed bundle layout.
    assert_eq!(
        own_bundle_root(Path::new("/p/target/debug/tiler-macos")),
        None
    );
    // Wrong leaf names refuse.
    assert_eq!(
        own_bundle_root(Path::new(
            "/p/OmniTilerProbe.app/Resources/MacOS/tiler-macos"
        )),
        None
    );
    assert_eq!(
        own_bundle_root(Path::new("/p/OmniTilerProbe.app/Contents/Bin/tiler-macos")),
        None
    );
    assert_eq!(
        own_bundle_root(Path::new(
            "/p/OmniTilerProbe.dir/Contents/MacOS/tiler-macos"
        )),
        None
    );
    assert_eq!(own_bundle_root(Path::new("/tiler-macos")), None);
}

#[test]
fn bundle_paths_resolve_from_signed_exe_layout_regardless_of_cwd() {
    // The parent is a fresh temp directory, never the repository: the
    // resolvers take no working directory, so a LaunchServices launch
    // (whose CWD is not the repo) resolves the same signed siblings.
    let parent = bundle_test_parent("siblings");
    let exe = assemble_sibling_bundles(&parent);
    assert_eq!(
        resolve_product_bundle(&exe, None).expect("product resolves"),
        parent.join(PRODUCT_BUNDLE_FILE_NAME)
    );
    assert_eq!(
        resolve_fixture_bundle(&exe, None).expect("fixture resolves"),
        parent.join(FIXTURE_BUNDLE_FILE_NAME)
    );
    let _ = std::fs::remove_dir_all(&parent);
}

#[test]
fn bundle_paths_fail_closed_outside_signed_layout() {
    use std::path::Path;
    // Developer binary layout: no bundle root to anchor on, no
    // CWD guess. The refusal names the signed layout, never a
    // missing working directory.
    let exe = Path::new("/repo/target/debug/tiler-macos");
    let fixture_err = resolve_fixture_bundle(exe, None).expect_err("fixture refuses");
    assert!(
        fixture_err.contains("signed bundle layout"),
        "{fixture_err}"
    );
    assert!(!fixture_err.contains("working directory"), "{fixture_err}");
    let product_err = resolve_product_bundle(exe, None).expect_err("product refuses");
    assert!(
        product_err.contains("signed bundle layout"),
        "{product_err}"
    );
    assert!(!product_err.contains("working directory"), "{product_err}");
}

#[test]
fn bundle_paths_refuse_missing_sibling_and_foreign_markers() {
    // Probe bundle only: the missing fixture sibling refuses with the
    // build-recipe guidance (never a bare guess elsewhere).
    let parent = bundle_test_parent("probe-only");
    let probe_root = parent.join(PRODUCT_BUNDLE_FILE_NAME);
    write_test_file(&probe_root.join(PRODUCT_EXE_REL), "probe");
    write_test_file(
        &probe_root.join(PRODUCT_MARKER_REL),
        &format!("{}\n", crate::BUNDLE_ID),
    );
    let exe = probe_root.join(PRODUCT_EXE_REL);
    let err = resolve_fixture_bundle(&exe, None).expect_err("missing sibling refuses");
    assert!(err.contains("fixture bundle missing"), "{err}");
    // Foreign product marker refuses before any effect.
    write_test_file(
        &probe_root.join(PRODUCT_MARKER_REL),
        "com.example.foreign\n",
    );
    let err = resolve_product_bundle(&exe, None).expect_err("foreign marker refuses");
    assert!(err.contains("marker mismatch"), "{err}");
    // Foreign fixture marker refuses the same way.
    let fixture_parent = bundle_test_parent("foreign-fixture");
    let foreign_exe = assemble_sibling_bundles(&fixture_parent);
    let foreign_root = fixture_parent.join(FIXTURE_BUNDLE_FILE_NAME);
    write_test_file(
        &foreign_root.join(FIXTURE_MARKER_REL),
        "com.example.foreign\n",
    );
    let err = resolve_fixture_bundle(&foreign_exe, None).expect_err("foreign fixture refuses");
    assert!(err.contains("marker mismatch"), "{err}");
    // A symlinked fixture bundle path refuses.
    let link_parent = bundle_test_parent("links");
    let _ = assemble_sibling_bundles(&link_parent);
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        link_parent.join(FIXTURE_BUNDLE_FILE_NAME),
        link_parent.join("FixtureLink.app"),
    )
    .expect("test symlink");
    let linked = link_parent.join("FixtureLink.app");
    let err = check_fixture_bundle(&linked).expect_err("symlinked fixture refuses");
    assert!(err.contains("symlink"), "{err}");
    let _ = std::fs::remove_dir_all(&parent);
    let _ = std::fs::remove_dir_all(&fixture_parent);
    let _ = std::fs::remove_dir_all(&link_parent);
}

#[test]
fn bundle_overrides_keep_identity_checks() {
    let parent = bundle_test_parent("overrides");
    let exe = assemble_sibling_bundles(&parent);
    let probe_root = parent.join(PRODUCT_BUNDLE_FILE_NAME);
    let fixture_root = parent.join(FIXTURE_BUNDLE_FILE_NAME);
    // Empty override falls back to the executable-anchored default.
    assert_eq!(
        resolve_product_bundle(&exe, Some("")).expect("empty override falls back"),
        probe_root
    );
    assert_eq!(
        resolve_fixture_bundle(&exe, Some("")).expect("empty override falls back"),
        fixture_root
    );
    // Valid overrides resolve with the same checks applied.
    assert_eq!(
        resolve_product_bundle(&exe, Some(&probe_root.to_string_lossy()))
            .expect("valid product override"),
        probe_root
    );
    assert_eq!(
        resolve_fixture_bundle(&exe, Some(&fixture_root.to_string_lossy()))
            .expect("valid fixture override"),
        fixture_root
    );
    // Override to a marker-less directory refuses (product and fixture).
    let bare = parent.join("Bare.app");
    std::fs::create_dir_all(&bare).expect("bare dir creates");
    let err = resolve_product_bundle(&exe, Some(&bare.to_string_lossy()))
        .expect_err("marker-less product override refuses");
    assert!(err.contains("owned marker"), "{err}");
    let err = resolve_fixture_bundle(&exe, Some(&bare.to_string_lossy()))
        .expect_err("marker-less fixture override refuses");
    assert!(err.contains("owned marker"), "{err}");
    // Missing override path refuses with the build-recipe guidance.
    let missing = parent.join("Missing.app");
    let err = resolve_product_bundle(&exe, Some(&missing.to_string_lossy()))
        .expect_err("missing product override refuses");
    assert!(err.contains("product bundle missing"), "{err}");
    let _ = std::fs::remove_dir_all(&parent);
}

#[test]
fn float_sequences_usage_refuses_before_any_effect() {
    // Exactly two same-value chords (subject then sibling), no extra or
    // mixed chord: usage refuses before a baseline, Space, or launch effect.
    for chords in [
        vec![FLOAT_SEQUENCES_CHORD],
        vec![FLOAT_SEQUENCES_CHORD, "focus-h"],
        vec![FLOAT_SEQUENCES_CHORD, FLOAT_SEQUENCES_CHORD, "focus-h"],
    ] {
        let err = run_fixture_leg(test_opts(chords)).unwrap_err();
        assert_eq!(err.0, 2);
        assert!(err.1.contains("exactly two"), "{}", err.1);
    }
    // The alias is a controller arm with a post-only admission scope; the
    // post resolver never sees it.
    assert_eq!(
        crate::live_leg::required_scope(FLOAT_SEQUENCES_CHORD),
        Some(crate::live_leg::AdmissionScope::PostOnly)
    );
}

#[test]
fn apply_send_usage_refuses_before_any_effect() {
    // Exactly one Compatible send: extra chords, Authentic (which writes
    // holders), and the extra probes refuse as usage before a baseline,
    // Space, or launch effect.
    let extra = test_opts(vec![APPLY_SEND_CHORD, "focus-h"]);
    let err = run_fixture_leg(extra).unwrap_err();
    assert_eq!(err.0, 2);
    assert!(err.1.contains("only"), "{}", err.1);
    let mut authentic = test_opts(vec![APPLY_SEND_CHORD]);
    authentic.preset = InputPreset::Authentic;
    let err = run_fixture_leg(authentic).unwrap_err();
    assert_eq!(err.0, 2);
    assert!(err.1.contains("compatible"), "{}", err.1);
    let mut hover = test_opts(vec![APPLY_SEND_CHORD]);
    hover.hover_probe = true;
    assert_eq!(run_fixture_leg(hover).unwrap_err().0, 2);
    let mut corrupt = test_opts(vec![APPLY_SEND_CHORD]);
    corrupt.corrupt_intent_store = true;
    assert_eq!(run_fixture_leg(corrupt).unwrap_err().0, 2);
    // The alias is a controller arm with a space-mutating admission scope
    // (the computed send chord needs it); the post resolver never sees it.
    assert_eq!(
        crate::live_leg::required_scope(APPLY_SEND_CHORD),
        Some(crate::live_leg::AdmissionScope::SpaceMutating)
    );
}
