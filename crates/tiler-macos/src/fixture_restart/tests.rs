    use std::cell::{Cell, RefCell};
    use std::path::{Path, PathBuf};

    use crate::config::ProbeModifier;
    use crate::fixture_frame::FramePoint;
    use crate::fixture_leg::{FIXTURE_SCOPE, LEG_MAX_SECONDS, LEG_MIN_SECONDS};
    use crate::input_policy::InputPreset;
    use crate::live_leg::clean_terminal_verdict;

    use super::arrangement::{
        ArrangementEvidence, compare_post_stop_overlay, compare_restart_arrangement,
        compare_restart_base, require_no_native_zoom, require_nudge_preimage_distinct,
        require_post_stop_arrangement, require_tile_multisets_match, require_tile_reflow,
    };
    use super::frame::WindowFrames;
    use super::frames::runtime_started_seen;
    use super::guard::{
        Liveness, RestartGuardInputs, foreign_product_in_apps, restart_guard_verdict,
    };
    use super::orchestration::{
        pre_chord_placement_holds, run_body_then_finish, run_guarded_between_launch,
    };
    use super::store::{
        StoreDirMeta, StoreFile, StoreSnapshot, read_store_snapshot, require_m3_float_rows,
        require_m4_absence, store_hydration_state,
    };
    use super::{
        FixtureRestartOpts, RESTART_CAFFEINATE_MARGIN_SECONDS, RestartMode, restart_caffeine_seconds,
        restart_launch_dir, restart_run_id, validate_restart_opts,
    };

    fn frames(caller: &str, wid: u32, x: f64, y: f64, w: f64, h: f64) -> WindowFrames {
        WindowFrames {
            caller: caller.to_owned(),
            wid,
            ax: FramePoint { x, y, w, h },
            cg: FramePoint { x, y, w, h },
            cg_onscreen: true,
            zoomed: Some(false),
        }
    }

    fn opts(mode: RestartMode, windows: usize, floats: usize) -> FixtureRestartOpts {
        FixtureRestartOpts {
            mode,
            seconds: 90,
            windows,
            floats,
            preset: InputPreset::Compatible,
            modifier: ProbeModifier::Ctrl,
            signer: "designated => test".to_owned(),
        }
    }

    #[test]
    fn mode_and_shape_validate_before_any_effect() {
        assert_eq!(RestartMode::parse("m3").expect("m3"), RestartMode::M3);
        assert_eq!(RestartMode::parse("m4").expect("m4"), RestartMode::M4);
        assert!(RestartMode::parse("m5").is_err());
        assert!(validate_restart_opts(&opts(RestartMode::M3, 4, 2)).is_ok());
        assert!(validate_restart_opts(&opts(RestartMode::M3, 4, 1)).is_err());
        assert!(validate_restart_opts(&opts(RestartMode::M3, 3, 2)).is_err());
        assert!(validate_restart_opts(&opts(RestartMode::M4, 2, 2)).is_err());
        assert!(validate_restart_opts(&opts(RestartMode::M4, 1, 0)).is_err());
        assert!(validate_restart_opts(&opts(RestartMode::M4, 2, 0)).is_ok());
        let mut bad_seconds = opts(RestartMode::M3, 4, 2);
        bad_seconds.seconds = LEG_MIN_SECONDS - 1;
        assert!(validate_restart_opts(&bad_seconds).is_err());
        bad_seconds.seconds = LEG_MAX_SECONDS + 1;
        assert!(validate_restart_opts(&bad_seconds).is_err());
        let mut no_signer = opts(RestartMode::M3, 4, 2);
        no_signer.signer = "  ".to_owned();
        assert!(validate_restart_opts(&no_signer).is_err());
        assert!(validate_restart_opts(&opts(RestartMode::M3, 9, 2)).is_err());
    }

    #[test]
    fn launch_ids_and_output_dirs_are_distinct() {
        let product_dir = Path::new("/tmp/restart/product");
        let first = restart_run_id(100, 200, 1);
        let second = restart_run_id(100, 200, 2);
        assert_ne!(first, second);
        assert!(first.ends_with("-1") && second.ends_with("-2"));
        assert_ne!(
            restart_launch_dir(product_dir, 1),
            restart_launch_dir(product_dir, 2)
        );
        assert_eq!(
            restart_launch_dir(product_dir, 1),
            product_dir.join("launch-1")
        );
    }

    fn guard() -> RestartGuardInputs {
        let store = StoreSnapshot {
            present: true,
            dir: Some(StoreDirMeta {
                mode: 0o700,
                uid: 501,
                ino: 7,
                mtime_ns: 11,
            }),
            files: vec![StoreFile {
                name: "intent-v1-501-100.json".to_owned(),
                len: 4,
                mode: 0o600,
                uid: 501,
                ino: 8,
                mtime_ns: 12,
                bytes: b"row\n".to_vec(),
            }],
        };
        RestartGuardInputs {
            launch_ids_distinct: true,
            product_overlap: false,
            product_overlap_note: None,
            prior_terminal_clean: true,
            prior_liveness: Liveness::Gone,
            prior_lifetime_gone: true,
            fixture_liveness: Liveness::Present,
            fixture_identity_readable: true,
            fixture_bundle_match: true,
            fixture_exe_match: true,
            fixture_launch_match: true,
            fixture_nonce_match: true,
            fixture_join_same: true,
            fixture_join_note: None,
            topology_readable: true,
            current_owned_space: true,
            topology_row_exact: true,
            windows_on_owned_space: true,
            store_expected: store.clone(),
            store_observed: store,
        }
    }

    #[test]
    fn guard_refuses_an_alive_prior_lifetime_before_relaunch() {
        let mut inputs = guard();
        inputs.prior_liveness = Liveness::Present;
        let error = restart_guard_verdict(&inputs).expect_err("alive prior refuses");
        assert!(error.contains("prior-pid"), "{error}");
        inputs.prior_liveness = Liveness::Unreadable("probe failed".to_owned());
        let error = restart_guard_verdict(&inputs).expect_err("unreadable prior refuses");
        assert!(error.contains("prior-pid"), "{error}");
        inputs.prior_liveness = Liveness::Gone;
        inputs.prior_lifetime_gone = false;
        let error = restart_guard_verdict(&inputs).expect_err("live prior lifetime refuses");
        assert!(error.contains("prior-lifetime"), "{error}");
        inputs.prior_lifetime_gone = true;
        assert!(restart_guard_verdict(&inputs).is_ok());
    }

    #[test]
    fn guard_refuses_overlap_identity_topology_and_store_drift() {
        let mut inputs = guard();
        inputs.product_overlap = true;
        assert!(
            restart_guard_verdict(&inputs)
                .expect_err("overlap refuses")
                .contains("product-overlap")
        );
        inputs.product_overlap = false;
        inputs.fixture_exe_match = false;
        assert!(
            restart_guard_verdict(&inputs)
                .expect_err("fixture drift refuses")
                .contains("fixture-identity")
        );
        inputs.fixture_exe_match = true;
        inputs.fixture_join_same = false;
        inputs.fixture_join_note = Some("join read refused".to_owned());
        let error = restart_guard_verdict(&inputs).expect_err("join drift refuses");
        assert!(error.contains("fixture-join"), "{error}");
        inputs.fixture_join_same = true;
        inputs.current_owned_space = false;
        assert!(
            restart_guard_verdict(&inputs)
                .expect_err("space drift refuses")
                .contains("owned-space")
        );
        inputs.current_owned_space = true;
        inputs.topology_row_exact = false;
        assert!(
            restart_guard_verdict(&inputs)
                .expect_err("row drift refuses")
                .contains("owned-space")
        );
        inputs.topology_row_exact = true;
        inputs.windows_on_owned_space = false;
        assert!(
            restart_guard_verdict(&inputs)
                .expect_err("placement drift refuses")
                .contains("window-placement")
        );
        inputs.windows_on_owned_space = true;
        inputs.store_observed.files[0].bytes = b"roW\n".to_vec();
        let error = restart_guard_verdict(&inputs).expect_err("store drift refuses");
        assert!(error.contains("store"), "{error}");
    }

    #[test]
    fn guard_orders_the_first_failure() {
        let mut inputs = guard();
        inputs.product_overlap = true;
        inputs.prior_terminal_clean = false;
        inputs.store_observed.dir.as_mut().expect("dir").mtime_ns = 99;
        let error = restart_guard_verdict(&inputs).expect_err("first stage refuses");
        assert!(error.contains("product-overlap"), "{error}");
        inputs.product_overlap = false;
        let error = restart_guard_verdict(&inputs).expect_err("terminal stage refuses");
        assert!(error.contains("prior-terminal"), "{error}");
        inputs.prior_terminal_clean = true;
        let error = restart_guard_verdict(&inputs).expect_err("store stage refuses");
        assert!(error.contains("store"), "{error}");
        inputs.launch_ids_distinct = false;
        let error = restart_guard_verdict(&inputs).expect_err("identity first");
        assert!(error.contains("launch-identity"), "{error}");
    }

    #[test]
    fn terminal_evidence_requires_a_clean_stop() {
        assert!(
            clean_terminal_verdict(&serde_json::json!({
                "outcome": "stopped", "shutdown_clean": true
            }))
            .is_ok()
        );
        assert!(
            clean_terminal_verdict(&serde_json::json!({
                "outcome": "completed", "shutdown_clean": true
            }))
            .is_ok()
        );
        for body in [
            serde_json::json!({"outcome": "stopped"}),
            serde_json::json!({"outcome": "stopped", "shutdown_clean": false}),
            serde_json::json!({"outcome": "refused", "shutdown_clean": true}),
            serde_json::json!({"shutdown_clean": true}),
            serde_json::json!({}),
        ] {
            assert!(clean_terminal_verdict(&body).is_err(), "{body}");
        }
    }

    #[test]
    fn runtime_started_evidence_is_exact() {
        assert!(runtime_started_seen(
            "tiler-log seq=1 t=2ms pid=0 gen=0 ref=0 kind=runtime outcome=started count=0"
        ));
        assert!(runtime_started_seen(
            "noise\ntiler-log seq=9 t=1ms pid=0 gen=0 ref=0 kind=runtime outcome=started count=0\nmore"
        ));
        for raw in [
            "",
            "kind=runtime outcome=starting",
            "kind=startup outcome=started",
            "kind=runtimeoutcome=started",
        ] {
            assert!(!runtime_started_seen(raw), "{raw:?}");
        }
    }

    #[test]
    fn arrangement_floats_exact_and_tiles_unordered() {
        let before = ArrangementEvidence {
            floats: vec![
                frames("w0", 10, 0.0, 0.0, 400.0, 300.0),
                frames("w1", 11, 500.0, 0.0, 400.0, 300.0),
            ],
            tiles: vec![
                frames("w2", 12, 0.0, 400.0, 300.0, 200.0),
                frames("w3", 13, 300.0, 400.0, 300.0, 200.0),
            ],
            tile_reflowed: true,
        };
        // Exact (identical order) passes.
        assert!(
            compare_restart_arrangement(
                &before,
                &[before.floats.clone(), before.tiles.clone()].concat()
            )
            .is_ok()
        );
        // A swapped tile slot (same multiset, different windows) passes.
        let swapped = vec![
            before.floats[0].clone(),
            before.floats[1].clone(),
            frames("w2", 12, 300.0, 400.0, 300.0, 200.0),
            frames("w3", 13, 0.0, 400.0, 300.0, 200.0),
        ];
        assert!(compare_restart_arrangement(&before, &swapped).is_ok());
        // A float frame change refuses even when tiles match.
        let mut moved_float = swapped.clone();
        moved_float[0].ax.x += 1.0;
        assert!(compare_restart_arrangement(&before, &moved_float).is_err());
        // A float CG-only change refuses (independent legs).
        let mut moved_cg = swapped.clone();
        moved_cg[1].cg.y += 1.0;
        assert!(compare_restart_arrangement(&before, &moved_cg).is_err());
        // A tile rectangle change refuses.
        let mut moved_tile = swapped.clone();
        moved_tile[2].ax.w += 1.0;
        assert!(compare_restart_arrangement(&before, &moved_tile).is_err());
        // Swapping frames between two windows while retaining every caller
        // and WID is not a per-window float match (no role is swapped; the
        // window set is identical).
        let mut crossed = before.floats.clone();
        let (first_float, second_float) = (crossed[0].clone(), crossed[1].clone());
        crossed[0].ax = second_float.ax;
        crossed[0].cg = second_float.cg;
        crossed[1].ax = first_float.ax;
        crossed[1].cg = first_float.cg;
        crossed.extend(before.tiles.clone());
        assert!(compare_restart_arrangement(&before, &crossed).is_err());
        // Duplicate counts matter: two identical tiles before cannot match
        // one identical tile plus a different one after.
        let duplicate = ArrangementEvidence {
            floats: before.floats.clone(),
            tiles: vec![
                frames("w2", 12, 0.0, 400.0, 300.0, 200.0),
                frames("w3", 13, 0.0, 400.0, 300.0, 200.0),
            ],
            tile_reflowed: true,
        };
        let one_duplicate = vec![
            before.floats[0].clone(),
            before.floats[1].clone(),
            frames("w2", 12, 0.0, 400.0, 300.0, 200.0),
            frames("w3", 13, 300.0, 400.0, 300.0, 200.0),
        ];
        assert!(compare_restart_arrangement(&duplicate, &one_duplicate).is_err());
        // Missing window refuses.
        assert!(compare_restart_arrangement(&before, &swapped[..3]).is_err());
    }

    #[test]
    fn maximize_restart_restores_the_exact_base_not_the_overlay() {
        let base = vec![
            frames("w0", 10, 0.0, 0.0, 600.0, 400.0),
            frames("w1", 11, 600.0, 0.0, 600.0, 400.0),
        ];
        let overlay = vec![
            frames("w0", 10, 0.0, 0.0, 1200.0, 800.0),
            frames("w1", 11, 600.0, 0.0, 600.0, 400.0),
        ];
        assert!(compare_restart_base(&base, &overlay, &base, "w0").is_ok());
        // Slots may swap: restoration is the exact base rectangle multiset
        // over the same joined caller+WID set, not the pre-stop slot.
        let swapped = vec![
            frames("w0", 10, 600.0, 0.0, 600.0, 400.0),
            frames("w1", 11, 0.0, 0.0, 600.0, 400.0),
        ];
        assert!(compare_restart_base(&base, &overlay, &swapped, "w0").is_ok());
        // Overlay surviving restart refuses.
        assert!(compare_restart_base(&base, &overlay, &overlay, "w0").is_err());
        // No distinct overlay refuses.
        assert!(compare_restart_base(&base, &base, &base, "w0").is_err());
        // A slot-swapped overlay leaves both base multisets unchanged: it is
        // a no-op arrangement, never a live composed overlay.
        let swapped_overlay = vec![
            frames("w0", 10, 600.0, 0.0, 600.0, 400.0),
            frames("w1", 11, 0.0, 0.0, 600.0, 400.0),
        ];
        assert!(compare_restart_base(&base, &swapped_overlay, &base, "w0").is_err());
        // A missing window refuses.
        assert!(compare_restart_base(&base, &overlay, &swapped[..1], "w0").is_err());
        // A duplicated window entry refuses even when its rectangle matches.
        assert!(
            compare_restart_base(&base, &overlay, &[swapped[0].clone(), swapped[0].clone()], "w0")
                .is_err()
        );
        // A non-target window drifting refuses.
        let mut moved = base.clone();
        moved[1].cg.x += 5.0;
        assert!(compare_restart_base(&base, &overlay, &moved, "w0").is_err());
        // Only one independent read moving is not a proven overlay.
        let mut ax_only = base.clone();
        ax_only[0].ax.w = 1200.0;
        let mut cg_only = base.clone();
        cg_only[0].cg.w = 1200.0;
        assert!(compare_restart_base(&base, &ax_only, &base, "w0").is_err());
        assert!(compare_restart_base(&base, &cg_only, &base, "w0").is_err());
    }

    #[test]
    fn post_stop_capture_proves_the_exact_live_overlay() {
        let overlay = vec![
            frames("w0", 10, 0.0, 0.0, 1200.0, 800.0),
            frames("w1", 11, 600.0, 0.0, 600.0, 400.0),
        ];
        let base = vec![
            frames("w0", 10, 0.0, 0.0, 600.0, 400.0),
            frames("w1", 11, 600.0, 0.0, 600.0, 400.0),
        ];
        assert!(compare_post_stop_overlay(&overlay, &overlay).is_ok());
        // Early restoration back to the base is not a live overlay.
        assert!(compare_post_stop_overlay(&overlay, &base).is_err());
        // Target or peer drift between the products refuses.
        let mut target_drift = overlay.clone();
        target_drift[0].ax.x += 1.0;
        assert!(compare_post_stop_overlay(&overlay, &target_drift).is_err());
        let mut peer_drift = overlay.clone();
        peer_drift[1].cg.y += 1.0;
        assert!(compare_post_stop_overlay(&overlay, &peer_drift).is_err());
        // Missing, duplicated or off-screen windows refuse.
        assert!(compare_post_stop_overlay(&overlay, &overlay[..1]).is_err());
        let mut duplicated = overlay.clone();
        duplicated[1].caller = duplicated[0].caller.clone();
        assert!(compare_post_stop_overlay(&overlay, &duplicated).is_err());
        let mut offscreen = overlay.clone();
        offscreen[0].cg_onscreen = false;
        assert!(compare_post_stop_overlay(&overlay, &offscreen).is_err());
    }

    #[test]
    fn post_stop_arrangement_proves_the_exact_pre_nudge_preimage() {
        let arrangement = ArrangementEvidence {
            floats: vec![frames("w0", 10, 0.0, 0.0, 400.0, 300.0)],
            tiles: vec![
                frames("w1", 11, 0.0, 400.0, 300.0, 200.0),
                frames("w2", 12, 300.0, 400.0, 300.0, 200.0),
            ],
            tile_reflowed: true,
        };
        let exact: Vec<WindowFrames> = arrangement
            .floats
            .iter()
            .chain(arrangement.tiles.iter())
            .cloned()
            .collect();
        assert!(require_post_stop_arrangement("test", &arrangement, &exact).is_ok());
        // The nudge target drifting before the effect refuses.
        let mut target_drift = exact.clone();
        target_drift[1].ax.y += 2.0;
        assert!(require_post_stop_arrangement("test", &arrangement, &target_drift).is_err());
        // A peer drifting (CG-only) refuses too.
        let mut peer_drift = exact.clone();
        peer_drift[2].cg.w += 2.0;
        assert!(require_post_stop_arrangement("test", &arrangement, &peer_drift).is_err());
        // A window ID change refuses even with identical rectangles.
        let mut wid_drift = exact.clone();
        wid_drift[1].wid += 1;
        assert!(require_post_stop_arrangement("test", &arrangement, &wid_drift).is_err());
        // An off-screen peer is not a settled arrangement fact.
        let mut offscreen = exact.clone();
        offscreen[2].cg_onscreen = false;
        assert!(require_post_stop_arrangement("test", &arrangement, &offscreen).is_err());
    }

    #[test]
    fn nudge_frame_must_differ_from_every_pre_stop_tile_in_both_reads() {
        let tiles = vec![
            frames("w1", 11, 0.0, 400.0, 300.0, 200.0),
            frames("w2", 12, 300.0, 400.0, 300.0, 200.0),
        ];
        // A valid ordinary inset of the first tile is distinct from both
        // tiles in AX and CG.
        let valid = FramePoint {
            x: 16.0,
            y: 416.0,
            w: 268.0,
            h: 168.0,
        };
        assert!(require_nudge_preimage_distinct(&valid, &tiles).is_ok());
        // An inset colliding with a sibling tile's AX rectangle refuses.
        let collide_ax = tiles[1].ax;
        assert!(require_nudge_preimage_distinct(&collide_ax, &tiles).is_err());
        // A frame equal to a tile's CG bounds is not distinct either.
        let collide_cg = tiles[0].cg;
        assert!(require_nudge_preimage_distinct(&collide_cg, &tiles).is_err());
    }

    #[test]
    fn m4_requires_unchanged_absence_of_a_row() {
        let absent = StoreSnapshot::absent();
        let evidence = require_m4_absence(&absent).expect("absent store passes");
        assert_eq!(evidence["has_row"], false);
        assert_eq!(evidence["hydration_state"], "unchanged-absence");
        let empty_dir = StoreSnapshot {
            present: true,
            dir: None,
            files: Vec::new(),
        };
        let evidence = require_m4_absence(&empty_dir).expect("directory-only store passes");
        assert_eq!(evidence["hydration_state"], "unchanged-directory-only");
        // Any intent row file is a false success for M4.
        let rows = store_snapshot_with_rows(&[(4242, 10, 777, None, FIXTURE_SCOPE)]);
        assert!(require_m4_absence(&rows).is_err());
    }

    #[test]
    fn m4_native_zoom_true_is_never_a_composed_restore() {
        for phase in ["base", "overlay", "post-stop", "hydration"] {
            assert!(require_no_native_zoom(phase, Some(false)).is_ok());
            // An unreadable attribute is raw evidence, not a zoom claim.
            assert!(require_no_native_zoom(phase, None).is_ok());
            let error = require_no_native_zoom(phase, Some(true)).expect_err("native zoom refuses");
            assert!(error.contains(phase), "{error}");
        }
    }

    #[test]
    fn awake_budget_covers_both_lifetimes_plus_the_fixed_margin() {
        assert_eq!(restart_caffeine_seconds(90), 300);
        assert_eq!(restart_caffeine_seconds(70), 260);
        assert_eq!(
            restart_caffeine_seconds(LEG_MIN_SECONDS),
            LEG_MIN_SECONDS * 2 + RESTART_CAFFEINATE_MARGIN_SECONDS
        );
        assert_eq!(restart_caffeine_seconds(u64::MAX), u64::MAX);
    }

    #[test]
    fn tile_reflow_requires_a_real_tiled_movement() {
        let initial = vec![
            frames("w0", 10, 0.0, 0.0, 400.0, 300.0),
            frames("w1", 11, 500.0, 0.0, 400.0, 300.0),
            frames("w2", 12, 0.0, 400.0, 300.0, 200.0),
            frames("w3", 13, 300.0, 400.0, 300.0, 200.0),
        ];
        let tiles = vec![
            frames("w2", 12, 0.0, 0.0, 600.0, 600.0),
            frames("w3", 13, 600.0, 0.0, 600.0, 600.0),
        ];
        assert!(require_tile_reflow(&initial, &tiles).expect("tile reflowed"));
        assert!(require_tile_reflow(&initial, &initial[2..]).is_err());
    }

    #[test]
    fn captured_historical_split_is_not_the_fresh_survivor_adoption() {
        // Exact frames from the refused 2026-11-27 signed M3 run: the two
        // float frames matched, the historical all-at-once tree left the
        // survivors stacked after the floats were removed, and the restarted
        // product fresh-adopted the same survivors as side-by-side columns.
        // The earlier staging compared the historical split with that fresh
        // tree and refused. See world_layout::adoption::serial_seed (fresh
        // insertion order) and restart_disjoint_adoption_tests
        // (any_shared_id_preserves_retained_topology) for the source cause.
        let floats = vec![
            frames("w0", 20, 8.0, 38.0, 1108.0, 1123.0),
            frames("w1", 21, 1124.0, 38.0, 1108.0, 1123.0),
        ];
        let historical_tiles = vec![
            frames("w2", 22, 8.0, 38.0, 2224.0, 557.0),
            frames("w3", 23, 8.0, 603.0, 2224.0, 558.0),
        ];
        let fresh_survivor_baseline = vec![
            frames("w2", 22, 8.0, 38.0, 1108.0, 1123.0),
            frames("w3", 23, 1124.0, 38.0, 1108.0, 1123.0),
        ];
        // Fresh adoption may assign the columns to either survivor.
        let after_tiles = vec![
            frames("w2", 22, 1124.0, 38.0, 1108.0, 1123.0),
            frames("w3", 23, 8.0, 38.0, 1108.0, 1123.0),
        ];
        let after_all = [floats.clone(), after_tiles.clone()].concat();
        let historical = ArrangementEvidence {
            floats: floats.clone(),
            tiles: historical_tiles.clone(),
            tile_reflowed: true,
        };
        // The restart oracle still refuses the historical arrangement.
        let refusal = compare_restart_arrangement(&historical, &after_all)
            .expect_err("the historical arrangement must not pass");
        assert!(refusal.contains("multiset changed"), "unexpected: {refusal}");
        // The new pre-stop check accepts swapped survivor slots but refuses
        // the historical split before launch one stops.
        require_tile_multisets_match("m3-pre-stop", &fresh_survivor_baseline, &after_tiles)
            .expect("swapped slots are the same multiset");
        assert!(
            require_tile_multisets_match(
                "m3-pre-stop",
                &fresh_survivor_baseline,
                &historical_tiles
            )
            .is_err(),
            "the historical split must not pass the fresh-baseline check"
        );
        let pre_stop = ArrangementEvidence {
            floats,
            tiles: fresh_survivor_baseline,
            tile_reflowed: true,
        };
        compare_restart_arrangement(&pre_stop, &after_all)
            .expect("the corrected staging yields the same arrangement across restart");
    }

    fn store_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("restart-store-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn write_store(dir: &Path, bytes: &[u8]) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(dir).expect("store dir creates");
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .expect("store dir mode");
        let path = dir.join("intent-v1-501-100.json");
        std::fs::write(&path, bytes).expect("store row writes");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .expect("store row mode");
        path
    }

    fn store_snapshot_with_rows(rows: &[(u32, u32, u64, Option<bool>, &str)]) -> StoreSnapshot {
        let entries: Vec<serde_json::Value> = rows
            .iter()
            .map(|(pid, wid, launch, float, bundle)| {
                serde_json::json!({
                    "win": format!("{pid}_{wid}"),
                    "wid": wid,
                    "pid": pid,
                    "launch_date_ns": launch,
                    "bundle_id": bundle,
                    "data": tiler_core::intent_data::encode_intent_data(
                        &tiler_core::intent_data::IntentData::new(*float, None)
                    ),
                })
            })
            .collect();
        store_snapshot_with_entries(entries)
    }

    fn store_snapshot_with_entries(entries: Vec<serde_json::Value>) -> StoreSnapshot {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "session_uid": 501,
            "session_asid": 100,
            "entries": entries,
        }))
        .expect("store envelope encodes");
        StoreSnapshot {
            present: true,
            dir: Some(StoreDirMeta {
                mode: 0o700,
                uid: 501,
                ino: 7,
                mtime_ns: 11,
            }),
            files: vec![StoreFile {
                name: "intent-v1-501-100.json".to_owned(),
                len: bytes.len() as u64,
                mode: 0o600,
                uid: 501,
                ino: 8,
                mtime_ns: 12,
                bytes,
            }],
        }
    }

    #[test]
    fn m3_float_rows_require_exact_binding_and_no_unexpected_float() {
        let binding = crate::intent_store::LiveBinding {
            launch_date_ns: 777,
            bundle_id: FIXTURE_SCOPE.to_owned(),
        };
        let floats = vec![("w0".to_owned(), 10), ("w1".to_owned(), 11)];
        let exact = store_snapshot_with_rows(&[
            (4242, 10, 777, Some(true), FIXTURE_SCOPE),
            (4242, 11, 777, Some(true), FIXTURE_SCOPE),
            (4242, 12, 777, Some(false), FIXTURE_SCOPE),
        ]);
        let evidence =
            require_m3_float_rows(&exact, &floats, 4242, &binding).expect("exact rows pass");
        assert_eq!(evidence["expected_floats"], 2);
        assert_eq!(evidence["observed_float_rows"], 2);
        // Missing one intended float refuses.
        let missing = store_snapshot_with_rows(&[
            (4242, 10, 777, Some(true), FIXTURE_SCOPE),
            (4242, 11, 777, Some(false), FIXTURE_SCOPE),
        ]);
        assert!(require_m3_float_rows(&missing, &floats, 4242, &binding).is_err());
        // An unexpected float row refuses even when both intended rows exist.
        let extra = store_snapshot_with_rows(&[
            (4242, 10, 777, Some(true), FIXTURE_SCOPE),
            (4242, 11, 777, Some(true), FIXTURE_SCOPE),
            (4242, 12, 777, Some(true), FIXTURE_SCOPE),
        ]);
        assert!(require_m3_float_rows(&extra, &floats, 4242, &binding).is_err());
        // A wrong lifetime, PID, WID or bundle is not an exact binding.
        for bad in [
            (4242, 10, 778, Some(true), FIXTURE_SCOPE),
            (4243, 10, 777, Some(true), FIXTURE_SCOPE),
            (4242, 10, 777, Some(true), "com.example.other"),
        ] {
            let drifted =
                store_snapshot_with_rows(&[bad, (4242, 11, 777, Some(true), FIXTURE_SCOPE)]);
            assert!(require_m3_float_rows(&drifted, &floats, 4242, &binding).is_err());
        }
        // A row whose `win` names an intended float but whose `wid` field
        // disagrees is not an exact binding: the decoded row is refused
        // rather than trusted by one field.
        let intent_data = tiler_core::intent_data::encode_intent_data(
            &tiler_core::intent_data::IntentData::new(Some(true), None),
        );
        let wrong_wid = store_snapshot_with_entries(vec![
            serde_json::json!({
                "win": "4242_10",
                "wid": 11,
                "pid": 4242,
                "launch_date_ns": 777,
                "bundle_id": FIXTURE_SCOPE,
                "data": intent_data.clone(),
            }),
            serde_json::json!({
                "win": "4242_11",
                "wid": 11,
                "pid": 4242,
                "launch_date_ns": 777,
                "bundle_id": FIXTURE_SCOPE,
                "data": intent_data,
            }),
        ]);
        assert!(require_m3_float_rows(&wrong_wid, &floats, 4242, &binding).is_err());
        // A float row for a real joined window that is not an intended float
        // is a wrong-WID binding even when every field is self-consistent.
        let wrong_intended = store_snapshot_with_rows(&[
            (4242, 10, 777, Some(true), FIXTURE_SCOPE),
            (4242, 13, 777, Some(true), FIXTURE_SCOPE),
        ]);
        assert!(require_m3_float_rows(&wrong_intended, &floats, 4242, &binding).is_err());
        // Absent store and a store with no row both refuse for M3.
        assert!(require_m3_float_rows(&StoreSnapshot::absent(), &floats, 4242, &binding).is_err());
        let empty = store_snapshot_with_rows(&[]);
        assert!(require_m3_float_rows(&empty, &floats, 4242, &binding).is_err());
    }

    #[test]
    fn store_hydration_label_distinguishes_absence_from_rows() {
        assert_eq!(
            store_hydration_state(&StoreSnapshot::absent()),
            "unchanged-absence"
        );
        let empty_dir = StoreSnapshot {
            present: true,
            dir: None,
            files: Vec::new(),
        };
        assert_eq!(
            store_hydration_state(&empty_dir),
            "unchanged-directory-only"
        );
        let rows = store_snapshot_with_rows(&[(4242, 10, 777, Some(true), FIXTURE_SCOPE)]);
        assert_eq!(store_hydration_state(&rows), "unchanged-rows");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn overlap_guard_excludes_the_controller_and_dead_product_entries() {
        let self_pid = 100;
        let controller_only = vec![
            (self_pid, crate::BUNDLE_ID.to_owned()),
            (300, "com.example.other".to_owned()),
        ];
        assert!(!foreign_product_in_apps(
            &controller_only,
            self_pid,
            crate::BUNDLE_ID,
            |_| true
        ));
        let with_product = vec![
            (self_pid, crate::BUNDLE_ID.to_owned()),
            (200, crate::BUNDLE_ID.to_owned()),
            (300, "com.example.other".to_owned()),
        ];
        assert!(foreign_product_in_apps(
            &with_product,
            self_pid,
            crate::BUNDLE_ID,
            |pid| pid == 200
        ));
        assert!(!foreign_product_in_apps(
            &with_product,
            self_pid,
            crate::BUNDLE_ID,
            |_| false
        ));
    }

    #[test]
    fn store_fingerprint_refuses_byte_and_metadata_drift() {
        let dir = store_dir("drift");
        write_store(&dir, b"row-one\n");
        let first = read_store_snapshot(&dir).expect("fingerprint reads");
        assert!(first.has_row());
        let again = read_store_snapshot(&dir).expect("fingerprint rereads");
        assert_eq!(first, again, "an unread store is unchanged");
        // Exact bytes, changed metadata (mode) still refuses.
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("intent-v1-501-100.json");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640))
            .expect("mode change");
        let mode_drift = read_store_snapshot(&dir).expect("fingerprint reads");
        assert_ne!(first, mode_drift);
        assert!(first.evidence()["digest"] != mode_drift.evidence()["digest"]);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .expect("mode restore");
        // Same length, changed bytes refuses.
        std::fs::write(&path, b"row-two\n").expect("byte change");
        let byte_drift = read_store_snapshot(&dir).expect("fingerprint reads");
        assert_ne!(first, byte_drift);
        // Absent is its own fingerprint and needs no row.
        let absent = read_store_snapshot(&dir.join("missing")).expect("absent fingerprint");
        assert!(!absent.has_row());
        assert!(!absent.evidence()["present"].as_bool().expect("bool"));
        // A symlinked store directory refuses.
        #[cfg(unix)]
        std::os::unix::fs::symlink(&dir, dir.with_extension("link")).expect("symlink");
        assert!(read_store_snapshot(&dir.with_extension("link")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(dir.with_extension("link"));
    }

    #[test]
    fn between_launch_guard_refusal_stops_before_the_effect_and_the_second_launch() {
        let cases: [(&str, fn(&mut RestartGuardInputs)); 3] = [
            ("product-overlap", |inputs| inputs.product_overlap = true),
            ("prior-terminal", |inputs| inputs.prior_terminal_clean = false),
            ("prior-pid", |inputs| inputs.prior_liveness = Liveness::Present),
        ];
        for (stage, mutate) in cases {
            let trace = RefCell::new(Vec::new());
            let mut inputs = guard();
            mutate(&mut inputs);
            let result = run_guarded_between_launch(
                &mut (),
                |_| Ok(inputs.clone()),
                |_| {
                    trace.borrow_mut().push("effect");
                    Ok(())
                },
                |_| {
                    trace.borrow_mut().push("launch");
                    Ok(())
                },
            );
            let error = result.expect_err("an uncertain initial guard refuses before the effect");
            assert!(error.1.contains(stage), "{}", error.1);
            assert!(
                trace.borrow().is_empty(),
                "a refused {stage} guard must not write or open: {:?}",
                trace.borrow()
            );
        }
    }

    #[test]
    fn changed_store_fingerprint_at_the_fresh_guard_stops_before_the_second_launch() {
        let trace = RefCell::new(Vec::new());
        let reads = Cell::new(0u8);
        let result = run_guarded_between_launch(
            &mut (),
            |_| {
                let mut inputs = guard();
                if reads.get() == 1 {
                    inputs.store_observed.files[0].bytes = b"row-after-effect\n".to_vec();
                }
                reads.set(reads.get() + 1);
                Ok(inputs)
            },
            |_| {
                trace.borrow_mut().push("effect");
                Ok(())
            },
            |_| {
                trace.borrow_mut().push("launch");
                Ok(())
            },
        );
        let error = result.expect_err("a changed store fingerprint refuses the second launch");
        assert!(error.1.contains("store"), "{}", error.1);
        assert_eq!(*trace.borrow(), vec!["effect"]);
    }

    #[test]
    fn between_launch_effect_errors_stop_before_the_second_launch() {
        let write_refusal = (
            1,
            "restart effect: authenticated frame write refused (signature drifted)".to_owned(),
        );
        let settle_timeout = (
            1,
            serde_json::json!({
                "stage": "restart-effect",
                "error": "the post-stop nudge did not settle to the exact requested AX and CG frame",
            })
            .to_string(),
        );
        for effect_error in [write_refusal, settle_timeout] {
            let trace = RefCell::new(Vec::new());
            let expected = effect_error.clone();
            let result = run_guarded_between_launch::<(), (), ()>(
                &mut (),
                |_| Ok(guard()),
                |_| {
                    trace.borrow_mut().push("effect");
                    Err(expected.clone())
                },
                |_| {
                    trace.borrow_mut().push("launch");
                    Ok(())
                },
            );
            assert_eq!(
                result.expect_err("a failed effect refuses the second launch"),
                effect_error
            );
            assert_eq!(*trace.borrow(), vec!["effect"]);
        }
    }

    #[test]
    fn successful_between_launch_records_guard_effect_fresh_guard_then_launch() {
        let trace = RefCell::new(Vec::new());
        let reads = Cell::new(0u8);
        let guarded = run_guarded_between_launch(
            &mut (),
            |_| {
                reads.set(reads.get() + 1);
                trace.borrow_mut().push(if reads.get() == 1 {
                    "initial-guard"
                } else {
                    "fresh-guard"
                });
                Ok(guard())
            },
            |_| {
                trace.borrow_mut().push("effect");
                Ok("proof")
            },
            |_| {
                trace.borrow_mut().push("launch");
                Ok("second-launch")
            },
        )
        .expect("clean initial guard, effect and fresh guard allow the second launch");
        assert_eq!(
            *trace.borrow(),
            vec!["initial-guard", "effect", "fresh-guard", "launch"]
        );
        assert_eq!(guarded.proof, "proof");
        assert_eq!(guarded.launch, "second-launch");
        assert_eq!(guarded.initial_guard["launch_ids_distinct"], true);
        assert_eq!(guarded.guard["store_expected"]["present"], true);
    }

    #[test]
    fn finish_runs_exactly_once_on_every_body_outcome() {
        // Success: the caller receives the body evidence and teardown.
        let trace = RefCell::new(Vec::new());
        let (evidence, teardown) = run_body_then_finish(
            (),
            |_| {
                trace.borrow_mut().push("body");
                Ok(serde_json::json!({"proof": true}))
            },
            |_| {
                trace.borrow_mut().push("finish");
                Ok(serde_json::json!({"teardown": true}))
            },
            |_| {
                trace.borrow_mut().push("report");
            },
        )
        .expect("body and finish succeed");
        assert_eq!(evidence["proof"], true);
        assert_eq!(teardown["teardown"], true);
        assert_eq!(*trace.borrow(), vec!["body", "finish"]);

        // Body failure with a clean finish: the original error is not masked
        // and the clean teardown is reported so retained launch evidence is
        // visible beside the refusal.
        let trace = RefCell::new(Vec::new());
        let error = run_body_then_finish::<(), serde_json::Value, serde_json::Value>(
            (),
            |_| {
                trace.borrow_mut().push("body");
                Err((1, "body refused".to_owned()))
            },
            |_| {
                trace.borrow_mut().push("finish");
                Ok(serde_json::json!({"evidence_retained": ["product/launch-1/stderr.log"]}))
            },
            |teardown| {
                trace.borrow_mut().push("report");
                assert!(teardown.contains("launch-1/stderr.log"), "{teardown}");
            },
        )
        .expect_err("body failure is returned");
        assert_eq!(error, (1, "body refused".to_owned()));
        assert_eq!(*trace.borrow(), vec!["body", "finish", "report"]);

        // Body failure with a refused finish: still one finish, and the
        // teardown failure is reported without replacing the cause.
        let trace = RefCell::new(Vec::new());
        let error = run_body_then_finish::<(), serde_json::Value, serde_json::Value>(
            (),
            |_| {
                trace.borrow_mut().push("body");
                Err((1, "body refused".to_owned()))
            },
            |_| {
                trace.borrow_mut().push("finish");
                Err((1, "{\"stage\":\"teardown\"}".to_owned()))
            },
            |teardown| {
                trace.borrow_mut().push("report");
                assert_eq!(teardown, "{\"stage\":\"teardown\"}");
            },
        )
        .expect_err("body failure is still returned");
        assert_eq!(error, (1, "body refused".to_owned()));
        assert_eq!(*trace.borrow(), vec!["body", "finish", "report"]);

        // Successful body with a refused finish: both the restart evidence
        // and the teardown failure are reported.
        let trace = RefCell::new(Vec::new());
        let error = run_body_then_finish::<(), serde_json::Value, serde_json::Value>(
            (),
            |_| {
                trace.borrow_mut().push("body");
                Ok(serde_json::json!({"restart": "evidence"}))
            },
            |_| {
                trace.borrow_mut().push("finish");
                Err((1, "{\"stage\":\"teardown\"}".to_owned()))
            },
            |_| {
                trace.borrow_mut().push("report");
            },
        )
        .expect_err("a refused finish after a successful body is reported");
        assert_eq!(error.0, 1);
        let body: serde_json::Value =
            serde_json::from_str(&error.1).expect("leg-finish error body is JSON");
        assert_eq!(body["stage"], "leg-finish");
        assert_eq!(body["restart"]["restart"], "evidence");
        assert_eq!(body["teardown"]["stage"], "teardown");
        assert_eq!(*trace.borrow(), vec!["body", "finish"]);
    }

    #[test]
    fn failed_second_launch_open_still_finishes_the_owned_run_exactly_once() {
        let trace = RefCell::new(Vec::new());
        let error = run_body_then_finish::<(), serde_json::Value, serde_json::Value>(
            (),
            |_| {
                run_guarded_between_launch::<(), (), ()>(
                    &mut (),
                    |_| Ok(guard()),
                    |_| {
                        trace.borrow_mut().push("effect");
                        Ok(())
                    },
                    |_| {
                        // The route registers the pending second launch
                        // before opening it; the open then fails.
                        trace.borrow_mut().push("pending-launch-two");
                        trace.borrow_mut().push("open-refused");
                        Err((1, "fixture-restart: second launch refused".to_owned()))
                    },
                )
                .map(|_| serde_json::json!({}))
            },
            |_| {
                trace.borrow_mut().push("finish");
                Ok(serde_json::json!({"teardown": true}))
            },
            |_| {
                trace.borrow_mut().push("report");
            },
        )
        .expect_err("a failed open returns the original launch error");
        assert_eq!(
            error,
            (1, "fixture-restart: second launch refused".to_owned())
        );
        let trace = trace.borrow();
        assert_eq!(
            &trace[..],
            [
                "effect",
                "pending-launch-two",
                "open-refused",
                "finish",
                "report"
            ]
        );
        assert_eq!(trace.iter().filter(|step| **step == "finish").count(), 1);
    }

    #[test]
    fn pre_chord_placement_recheck_refuses_owned_space_or_placement_drift() {
        assert!(pre_chord_placement_holds(true, true, true, true));
        for (readable, current, row_exact, placement) in [
            (false, true, true, true),
            (true, false, true, true),
            (true, true, false, true),
            (true, true, true, false),
        ] {
            assert!(
                !pre_chord_placement_holds(readable, current, row_exact, placement),
                "owned Space/topology ({readable}, {current}, {row_exact}) or placement ({placement}) drift must refuse before a chord"
            );
        }
    }
