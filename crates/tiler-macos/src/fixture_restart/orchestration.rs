//! Two-launch orchestration: the admitted pre-stop chord phase, the guarded
//! between-product proof/effect and second launch, and the handler finish
//! sequence. The pre-stop driver is scoped to its phase; no chord driver is
//! constructible in the between or hydration stages.

use std::path::Path;

use serde::Serialize;

use crate::fixture_leg::{
    FIXTURE_SCOPE, FOCUS_SETTLE_MS, ISOLATED_STORE_LEAF, create_owned_run_dir, fixture_bundle_path,
    join_fixture_windows, launch_fixture_bundle, launch_product_ls, product_argv,
    product_bundle_path, read_announcement, read_run_nonce, settle_hold, spawn_owned_windows,
};
use crate::fixture_owned_space::{
    PostOnlyOwnedSpace, await_joined_on_owned_space, establish_post_only_owned_space,
    post_only_placement_refusal,
};
use crate::live_leg::{FixtureRecord, LiveLeg, LiveLegTransport, RestartStopEvidence};

use super::arrangement::{
    ArrangementEvidence, compare_post_stop_overlay, compare_restart_arrangement,
    compare_restart_base, partition_arrangement, require_distinct_overlay, require_no_native_zoom,
    require_tile_multisets_match, require_tile_reflow, target_native_zoom,
};
use super::frame::WindowFrames;
use super::frames::{
    await_product_runtime_started, await_settled_frames, capture_window_frames,
    interrupted_restart_refusal, nudge_one_known_tile,
};
use super::guard::{
    OwnedRestartFixture, RestartGuardInputs, foreign_product_overlap, observe_restart_guard,
    owned_topology_facts, require_post_restart_capture_bound, restart_guard_verdict,
};
use super::store::{
    StoreSnapshot, read_store_snapshot, require_m3_float_rows, require_m4_absence,
    store_hydration_state,
};
use super::{
    FixtureRestartOpts, RESTART_CHORD_SETTLE_MS, RESTART_STARTUP_SETTLE_MS, RestartMode,
    restart_launch_dir, restart_run_id,
};

/// The ordered outcome of one guarded between-launch sequence.
#[derive(Debug)]
pub(crate) struct GuardedBetweenLaunch<P, L> {
    pub initial_guard: serde_json::Value,
    pub guard: serde_json::Value,
    pub proof: P,
    pub launch: L,
}

fn guard_stage_refusal(error: String, guard: serde_json::Value) -> (i32, String) {
    (
        1,
        serde_json::json!({
            "stage": "restart-guard",
            "error": error,
            "guard": guard,
        })
        .to_string(),
    )
}

/// Run the initial full guard, the between-product proof/effect, a fresh full
/// guard, then the second launch. Both guard reads use the real ordered guard
/// verdict; a refusal stops before the next stage, so no effect or launch can
/// follow an uncertain fact.
pub(crate) fn run_guarded_between_launch<S, P, L>(
    state: &mut S,
    mut observe_guard: impl FnMut(&mut S) -> Result<RestartGuardInputs, (i32, String)>,
    between_launch_proof: impl FnOnce(&mut S) -> Result<P, (i32, String)>,
    launch_two: impl FnOnce(&mut S) -> Result<L, (i32, String)>,
) -> Result<GuardedBetweenLaunch<P, L>, (i32, String)> {
    let initial = observe_guard(state)?;
    let initial_guard = initial.evidence();
    if let Err(error) = restart_guard_verdict(&initial) {
        return Err(guard_stage_refusal(error, initial_guard));
    }
    let proof = between_launch_proof(state)?;
    let fresh = observe_guard(state)?;
    let guard = fresh.evidence();
    if let Err(error) = restart_guard_verdict(&fresh) {
        return Err(guard_stage_refusal(error, guard));
    }
    let launch = launch_two(state)?;
    Ok(GuardedBetweenLaunch {
        initial_guard,
        guard,
        proof,
        launch,
    })
}

/// Run one two-launch body, then the owned finish exactly once. A successful
/// body whose finish refuses reports both the restart evidence and the
/// teardown failure; a failed body keeps its original error and reports the
/// owned teardown instead of masking it, so retained product launch evidence
/// stays visible beside the refusal.
pub(crate) fn run_body_then_finish<S, E, T>(
    mut state: S,
    body: impl FnOnce(&mut S) -> Result<E, (i32, String)>,
    finish: impl FnOnce(S) -> Result<T, (i32, String)>,
    report_teardown: impl FnOnce(&str),
) -> Result<(E, T), (i32, String)>
where
    E: Serialize,
    T: Serialize,
{
    match body(&mut state) {
        Ok(evidence) => match finish(state) {
            Ok(teardown) => Ok((evidence, teardown)),
            Err(teardown) => {
                let teardown_body = match serde_json::from_str::<serde_json::Value>(&teardown.1) {
                    Ok(value) => value,
                    Err(_) => serde_json::Value::String(teardown.1),
                };
                Err((
                    teardown.0,
                    serde_json::json!({
                        "stage": "leg-finish",
                        "error": "restart evidence captured but teardown refused",
                        "restart": evidence,
                        "teardown": teardown_body,
                    })
                    .to_string(),
                ))
            }
        },
        Err(error) => {
            match finish(state) {
                Ok(teardown) => {
                    // A refused body can still end with a clean owned
                    // teardown; report it so retained product launch
                    // evidence is visible beside the refusal.
                    if let Ok(teardown) = serde_json::to_string(&teardown) {
                        report_teardown(&teardown);
                    }
                }
                Err(teardown) => report_teardown(&teardown.1),
            }
            Err(error)
        }
    }
}

/// Evidence produced by the between-product proof/effect, before the fresh
/// guard and the second launch.
struct BetweenLaunchProof {
    float_rows: Option<serde_json::Value>,
    m4_store_absence: Option<serde_json::Value>,
    post_stop: Option<Vec<WindowFrames>>,
    nudge: Option<serde_json::Value>,
}

/// M3 proves the exact persisted float rows, then nudges one known tile; M4
/// proves the store row's unchanged absence and that the exact overlay
/// survived the first stop. No product is alive and no driver exists here.
#[cfg(target_os = "macos")]
fn run_between_launch_proof(
    opts: &FixtureRestartOpts,
    context: OwnedRestartFixture<'_>,
    pre: &PreStopEvidence,
    store_after_first: &StoreSnapshot,
) -> Result<BetweenLaunchProof, (i32, String)> {
    let fixture = context.fixture;
    let joined = context.joined;
    // M3: the store must hold an exact persisted float row for every intended
    // float window, bound to the authenticated fixture lifetime, with no
    // unexpected float row. Presence alone is not persistence evidence.
    let float_rows = if opts.mode == RestartMode::M3 {
        let fixture_binding = crate::intent_store::live_binding(fixture.pid).ok_or_else(|| {
            (
                1,
                "fixture-restart m3: live fixture lifetime binding is unreadable; refusing"
                    .to_owned(),
            )
        })?;
        Some(
            require_m3_float_rows(
                store_after_first,
                &joined[..opts.floats],
                fixture.pid,
                &fixture_binding,
            )
            .map_err(|error| (1, error))?,
        )
    } else {
        None
    };
    // M4 has no durable row to hydrate: prove the row's absence at the first
    // stop, then capture the still-live overlay while no product is alive.
    let m4_store_absence = if opts.mode == RestartMode::M4 {
        Some(require_m4_absence(store_after_first).map_err(|error| (1, error))?)
    } else {
        None
    };
    let post_stop = if opts.mode == RestartMode::M4 {
        let overlay = pre.overlay.as_ref().ok_or_else(|| {
            (
                1,
                "fixture-restart m4: pre-stop overlay is missing".to_owned(),
            )
        })?;
        let target = joined
            .first()
            .map(|(caller, _)| caller.as_str())
            .ok_or_else(|| (1, "fixture-restart m4: no target window".to_owned()))?;
        let post_stop = capture_window_frames(fixture.pid, joined)?;
        compare_post_stop_overlay(overlay, &post_stop).map_err(|error| {
            (
                1,
                serde_json::json!({
                    "stage": "restart-post-stop",
                    "error": error,
                    "overlay": overlay,
                    "post_stop": post_stop,
                })
                .to_string(),
            )
        })?;
        require_no_native_zoom("post-stop", target_native_zoom(&post_stop, target))
            .map_err(|error| (1, error))?;
        Some(post_stop)
    } else {
        None
    };
    // M3 alone nudges one known tile after the persisted rows are proven.
    // M4 leaves the overlay untouched until the second launch restores base.
    let nudge = if opts.mode == RestartMode::M3 {
        let arrangement = pre.arrangement.as_ref().ok_or_else(|| {
            (
                1,
                "fixture-restart m3: pre-stop arrangement is missing".to_owned(),
            )
        })?;
        Some(nudge_one_known_tile(context, arrangement, &opts.signer)?)
    } else {
        None
    };
    Ok(BetweenLaunchProof {
        float_rows,
        m4_store_absence,
        post_stop,
        nudge,
    })
}

/// Evidence captured from the second launch before it is stopped.
struct LaunchTwoOutcome {
    pid: u32,
    after: Vec<WindowFrames>,
    capture_binding: serde_json::Value,
    comparison: serde_json::Value,
    store_after_restart: StoreSnapshot,
    store_after_stop: StoreSnapshot,
    stop_pid: u32,
    stop_outcome: serde_json::Value,
}

/// Bind the second authenticated instance into its distinct run ID/output
/// directory, capture the hydrated frame set with no driver in scope, compare
/// it, then stop exactly that product. The hydration capture precedes every
/// comparison and the stop.
#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn launch_second_and_capture(
    leg: &mut LiveLeg<LiveLegTransport>,
    opts: &FixtureRestartOpts,
    context: OwnedRestartFixture<'_>,
    pre: &PreStopEvidence,
    store_dir: &Path,
    store_after_first: &StoreSnapshot,
    launch2_run_id: &str,
    launch2_dir: &Path,
) -> Result<LaunchTwoOutcome, (i32, String)> {
    let fixture = context.fixture;
    let joined = context.joined;
    let owned = context.owned;
    let product_bundle = product_bundle_path().map_err(|error| (2, error))?;
    let argv = product_argv(
        opts.seconds,
        opts.preset,
        opts.modifier,
        false,
        launch2_dir,
        &[owned.sid()],
        Some(owned.starting_sid()),
    );
    std::fs::write(launch2_dir.join("run-id"), format!("{launch2_run_id}\n")).map_err(|error| {
        (
            1,
            format!("fixture-restart: second run-id refused ({error})"),
        )
    })?;
    leg.note_pending_launch(crate::live_leg::PendingLaunch {
        run_dir: launch2_dir.to_path_buf(),
        expected_bundle: crate::BUNDLE_ID.to_owned(),
        what: "product",
    });
    let record = match launch_product_ls(
        &product_bundle,
        launch2_dir,
        store_dir,
        &argv,
        launch2_run_id,
        false,
        None,
    ) {
        Ok(record) => record,
        Err(error) => {
            let _ = leg.resolve_pending_absence("product");
            return Err((1, error));
        }
    };
    let launch2_pid = record.pid;
    let launch2_exe = record.exe.clone();
    let launch2_launch = record.launch.clone();
    leg.register_product_ls(record)?;
    await_product_runtime_started(launch2_dir)?;
    settle_hold(RESTART_STARTUP_SETTLE_MS)?;
    // Hydration capture before ANY new input: the driver was dropped with the
    // pre-stop phase and is not constructed here. The joined windows remain
    // owned by the authenticated fixture process, never by the product.
    let after = await_settled_frames(fixture.pid, joined)?;
    // The hydration read is still raw native evidence: the target must not
    // read AXZoomed:true at the moment the base is observed (a native zoom is
    // not composed-overlay restoration).
    if opts.mode == RestartMode::M4 {
        let target = joined
            .first()
            .map(|(caller, _)| caller.as_str())
            .ok_or_else(|| (1, "fixture-restart m4: no target window".to_owned()))?;
        require_no_native_zoom("hydration", target_native_zoom(&after, target))
            .map_err(|error| (1, error))?;
    }
    let capture_binding = require_post_restart_capture_bound(
        leg,
        context,
        launch2_pid,
        &launch2_exe,
        &launch2_launch,
    )?;
    let store_after_restart = read_store_snapshot(store_dir).map_err(|error| (1, error))?;
    if &store_after_restart != store_after_first {
        return Err((
            1,
            serde_json::json!({
                "stage": "restart-store",
                "error": "the isolated store bytes or metadata changed across the restart",
                "store_before": store_after_first.evidence(),
                "store_after": store_after_restart.evidence(),
            })
            .to_string(),
        ));
    }
    let comparison = match opts.mode {
        RestartMode::M3 => {
            let arrangement = pre.arrangement.as_ref().ok_or_else(|| {
                (
                    1,
                    "fixture-restart m3: pre-stop arrangement is missing".to_owned(),
                )
            })?;
            let survivor_baseline = pre.survivor_baseline.as_ref().ok_or_else(|| {
                (
                    1,
                    "fixture-restart m3: fresh survivor baseline is missing".to_owned(),
                )
            })?;
            compare_restart_arrangement(arrangement, &after).map_err(|error| {
                (
                    1,
                    serde_json::json!({
                        "stage": "restart-compare",
                        "error": error,
                        "survivor_baseline": survivor_baseline,
                        "pre_stop": arrangement,
                        "after": after,
                    })
                    .to_string(),
                )
            })?;
            serde_json::json!({
                "outcome": "m3-exact",
                "float_frames_exact": true,
                "tile_ax_multiset_exact": true,
                "tile_cg_multiset_exact": true,
                "tile_reflowed": arrangement.tile_reflowed,
                "survivor_baseline_verified_before_stop": true,
                "survivor_baseline": survivor_baseline,
                "before": arrangement,
                "after": after,
            })
        }
        RestartMode::M4 => {
            let base = pre
                .base
                .as_ref()
                .ok_or_else(|| (1, "fixture-restart m4: settled base is missing".to_owned()))?;
            let overlay = pre.overlay.as_ref().ok_or_else(|| {
                (
                    1,
                    "fixture-restart m4: pre-stop overlay is missing".to_owned(),
                )
            })?;
            let target = joined
                .first()
                .map(|(caller, _)| caller.as_str())
                .ok_or_else(|| (1, "fixture-restart m4: no target window".to_owned()))?;
            compare_restart_base(base, overlay, &after, target).map_err(|error| {
                (
                    1,
                    serde_json::json!({
                        "stage": "restart-compare",
                        "error": error,
                        "base": base,
                        "overlay": overlay,
                        "after": after,
                    })
                    .to_string(),
                )
            })?;
            serde_json::json!({
                "outcome": "m4-base-restored",
                "overlay_distinct": true,
                "overlay_target_cleared": true,
                "same_caller_wid_set": true,
                "base_ax_multiset_exact": true,
                "base_cg_multiset_exact": true,
                "target": target,
                "base": base,
                "overlay": overlay,
                "after": after,
            })
        }
    };
    // Stop exactly the second instance, then prove the store still matches
    // after an orderly stop and before the fixture/private-store teardown.
    let stop2 = leg.stop_product_for_restart().map_err(|error| {
        (
            1,
            serde_json::json!({"stage": "restart-stop-2", "error": error}).to_string(),
        )
    })?;
    let store_after_stop = read_store_snapshot(store_dir).map_err(|error| (1, error))?;
    Ok(LaunchTwoOutcome {
        pid: launch2_pid,
        after,
        capture_binding,
        comparison,
        store_after_restart,
        store_after_stop,
        stop_pid: stop2.pid,
        stop_outcome: stop2.outcome,
    })
}

/// Pre-chord placement verdict for the first startup's settled capture: the
/// baseline is only valid while the owned Space is current with its exact
/// topology row and every joined window is readable on the owned Space.
pub(crate) fn pre_chord_placement_holds(
    topology_readable: bool,
    current_owned_space: bool,
    topology_row_exact: bool,
    placement_readable: bool,
) -> bool {
    topology_readable && current_owned_space && topology_row_exact && placement_readable
}

/// Post-restart phase: prove an initial full guard before any effect; M3 then
/// proves the exact persisted float rows and nudges one known tile, while M4
/// proves the row's absence and that the exact per-window overlay survived the
/// first stop unchanged. A fresh full guard follows that between-product proof
/// and immediately precedes the second launch. Then bind the second
/// authenticated instance into its distinct run ID/output directory, capture
/// the hydrated frame set with no driver in scope, compare, then stop exactly
/// that product. No chord can be posted here.
#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn run_post_restart_phase(
    leg: &mut LiveLeg<LiveLegTransport>,
    opts: &FixtureRestartOpts,
    fixture: &FixtureRecord,
    nonce: &str,
    joined: &[(String, u32)],
    owned: &PostOnlyOwnedSpace,
    pre: &PreStopEvidence,
    prior: &RestartStopEvidence,
    store_dir: &Path,
    store_after_first: &StoreSnapshot,
    launch1_run_id: &str,
    launch2_run_id: &str,
    launch2_dir: &Path,
) -> Result<serde_json::Value, (i32, String)> {
    let launch_ids_distinct = launch1_run_id != launch2_run_id;
    let owned_fixture = OwnedRestartFixture {
        fixture,
        nonce,
        joined,
        owned,
    };
    let guarded = run_guarded_between_launch(
        leg,
        |leg| {
            observe_restart_guard(
                leg,
                owned_fixture,
                prior,
                store_dir,
                store_after_first,
                launch_ids_distinct,
            )
        },
        |_leg| run_between_launch_proof(opts, owned_fixture, pre, store_after_first),
        |leg| {
            launch_second_and_capture(
                leg,
                opts,
                owned_fixture,
                pre,
                store_dir,
                store_after_first,
                launch2_run_id,
                launch2_dir,
            )
        },
    )?;
    let GuardedBetweenLaunch {
        initial_guard,
        guard,
        proof:
            BetweenLaunchProof {
                float_rows,
                m4_store_absence,
                post_stop,
                nudge,
            },
        launch:
            LaunchTwoOutcome {
                pid: launch2_pid,
                after,
                capture_binding,
                comparison,
                store_after_restart,
                store_after_stop,
                stop_pid,
                stop_outcome,
            },
    } = guarded;
    // Raw native AXZoomed evidence for the M4 target at every required
    // phase; the verdicts above refuse AXZoomed:true, these values are the
    // unmodified observations.
    let m4_native_zoom = if opts.mode == RestartMode::M4 {
        let base = pre
            .base
            .as_ref()
            .ok_or_else(|| (1, "fixture-restart m4: settled base is missing".to_owned()))?;
        let overlay = pre.overlay.as_ref().ok_or_else(|| {
            (
                1,
                "fixture-restart m4: pre-stop overlay is missing".to_owned(),
            )
        })?;
        let target = joined
            .first()
            .map(|(caller, _)| caller.as_str())
            .ok_or_else(|| (1, "fixture-restart m4: no target window".to_owned()))?;
        Some(serde_json::json!({
            "base": {"value": target_native_zoom(base, target)},
            "overlay": {"value": target_native_zoom(overlay, target)},
            "post_stop": {"value": post_stop.as_ref().and_then(|frames| target_native_zoom(frames, target))},
            "hydration": {"value": target_native_zoom(&after, target)},
        }))
    } else {
        None
    };
    // The restart proof is the hydration store (checked before any new
    // input); a clean second shutdown may legitimately rewrite the same
    // logical row. Report the post-stop fingerprint, but never fail the
    // restart proof on a post-proof shutdown write.
    let store_unchanged_after_stop = &store_after_stop == store_after_first;
    Ok(serde_json::json!({
        "mode": opts.mode.as_str(),
        "fixture": {
            "pid": fixture.pid,
            "windows": joined
                .iter()
                .map(|(caller, wid)| serde_json::json!({"caller": caller, "wid": wid}))
                .collect::<Vec<_>>(),
        },
        "owned_space": owned.evidence(),
        "launch1": {
            "run_id": launch1_run_id,
            "pid": prior.pid,
            "terminal": prior.outcome,
        },
        "initial_guard": initial_guard,
        "guard": guard,
        "post_stop_nudge": nudge,
        "post_stop": post_stop,
        "m4_store_absence": m4_store_absence,
        "m4_native_zoom": m4_native_zoom,
        "m3_float_rows": float_rows,
        "launch2": {
            "run_id": launch2_run_id,
            "pid": launch2_pid,
            "post_input": false,
            "capture_identity": capture_binding,
        },
        "comparison": comparison,
        "store": {
            "after_first_stop": store_after_first.evidence(),
            "after_restart": store_after_restart.evidence(),
            "after_second_stop": store_after_stop.evidence(),
            "hydration_state": store_hydration_state(store_after_first),
            "unchanged": true,
            "unchanged_after_second_stop": store_unchanged_after_stop,
        },
        "second_stop": {
            "pid": stop_pid,
            "terminal": stop_outcome,
        },
    }))
}

/// Pre-stop evidence held across the relaunch.
struct PreStopEvidence {
    /// M3: settled arrangement after the floats (floats plus tiles).
    arrangement: Option<ArrangementEvidence>,
    /// M3: settled launch-one layout with only the surviving tiles present.
    /// Restart adoption is compared with this fresh tree, not with the
    /// history-dependent pre-stop tile split.
    survivor_baseline: Option<Vec<WindowFrames>>,
    /// M4: settled tiled base, then the active composed overlay.
    base: Option<Vec<WindowFrames>>,
    overlay: Option<Vec<WindowFrames>>,
}

/// Run every pre-stop chord through one admitted driver scoped to this
/// function: the driver (and its admission) cannot be used after the first
/// stop, so the second launch receives no new input by construction. No chord
/// is posted after a settled capture unless the exact owned current
/// Space/topology and joined placement are rechecked first.
#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn run_pre_stop_phase(
    leg: &mut LiveLeg<LiveLegTransport>,
    opts: &FixtureRestartOpts,
    announcement: &str,
    nonce: &str,
    joined: &[(String, u32)],
    owned: &PostOnlyOwnedSpace,
    fixture_pid: u32,
    survivor_baseline: Option<Vec<WindowFrames>>,
) -> Result<PreStopEvidence, (i32, String)> {
    let admission = leg.admit(fixture_pid, None)?;
    let live = crate::fixture_post::LiveTransport;
    let driver = crate::fixture_post::FixtureDriver::bind(
        &live,
        announcement,
        nonce,
        &opts.signer,
        admission,
    )?;
    let initial = await_settled_frames(fixture_pid, joined)?;
    // The first startup's settled capture is only a valid pre-chord baseline
    // if the exact owned Space/topology and joined placement still hold; the
    // same read the pre-launch guard enforces, applied before the first
    // focus/post rather than after it.
    let (topology_readable, current_owned_space, topology_row_exact) =
        owned_topology_facts(leg.transport_mut(), owned);
    let placement = await_joined_on_owned_space(joined, owned.sid());
    if !pre_chord_placement_holds(
        topology_readable,
        current_owned_space,
        topology_row_exact,
        placement.is_ok(),
    ) {
        let observed = placement.err().unwrap_or_default();
        return Err((
            1,
            post_only_placement_refusal(
                owned,
                &observed,
                "restart fixture owned Space/topology or joined placement drifted after the first startup capture; refusing without posting",
                Some(current_owned_space),
            )
            .to_string(),
        ));
    }
    match opts.mode {
        RestartMode::M3 => {
            for (caller, wid) in joined.iter().take(opts.floats) {
                if crate::space_drag_diag_live::diag_interrupted() {
                    return Err(interrupted_restart_refusal());
                }
                driver.focus(caller, *wid)?;
                settle_hold(FOCUS_SETTLE_MS)?;
                if crate::space_drag_diag_live::diag_interrupted() {
                    return Err(interrupted_restart_refusal());
                }
                driver.post(caller, *wid, "float-g", opts.modifier)?;
                settle_hold(RESTART_CHORD_SETTLE_MS)?;
            }
            let settled = await_settled_frames(fixture_pid, joined)?;
            let mut arrangement = partition_arrangement(&settled, opts.floats);
            arrangement.tile_reflowed =
                require_tile_reflow(&initial, &arrangement.tiles).map_err(|error| (1, error))?;
            // The survivors must return to the exact fresh launch-one
            // multisets before the original restart oracle compares them.
            let baseline = survivor_baseline.ok_or_else(|| {
                (
                    1,
                    "fixture-restart m3: fresh survivor baseline is missing before the stop"
                        .to_owned(),
                )
            })?;
            require_tile_multisets_match("m3-pre-stop", &baseline, &arrangement.tiles).map_err(
                |error| {
                    (
                        1,
                        serde_json::json!({
                            "stage": "restart-pre-stop",
                            "error": error,
                            "survivor_baseline": baseline,
                            "pre_stop_tiles": arrangement.tiles,
                        })
                        .to_string(),
                    )
                },
            )?;
            Ok(PreStopEvidence {
                arrangement: Some(arrangement),
                survivor_baseline: Some(baseline),
                base: None,
                overlay: None,
            })
        }
        RestartMode::M4 => {
            let (target_caller, target_wid) = joined
                .first()
                .cloned()
                .ok_or_else(|| (1, "fixture-restart: no joined window for m4".to_owned()))?;
            require_no_native_zoom("base", target_native_zoom(&initial, &target_caller))
                .map_err(|error| (1, error))?;
            if crate::space_drag_diag_live::diag_interrupted() {
                return Err(interrupted_restart_refusal());
            }
            driver.focus(&target_caller, target_wid)?;
            settle_hold(FOCUS_SETTLE_MS)?;
            if crate::space_drag_diag_live::diag_interrupted() {
                return Err(interrupted_restart_refusal());
            }
            driver.post(&target_caller, target_wid, "maximize-m", opts.modifier)?;
            settle_hold(RESTART_CHORD_SETTLE_MS)?;
            let overlay = await_settled_frames(fixture_pid, joined)?;
            require_distinct_overlay(&initial, &overlay, &target_caller)
                .map_err(|error| (1, error))?;
            require_no_native_zoom("overlay", target_native_zoom(&overlay, &target_caller))
                .map_err(|error| (1, error))?;
            Ok(PreStopEvidence {
                arrangement: None,
                survivor_baseline: None,
                base: Some(initial),
                overlay: Some(overlay),
            })
        }
    }
}

/// Spawn, join, register and placement-guard one bounded caller subset on the
/// owned Space. Returns the joined caller+WID pairs in the requested order;
/// every window is born on the owned Space and registered before any action.
#[cfg(target_os = "macos")]
fn spawn_join_owned_callers(
    leg: &mut LiveLeg<LiveLegTransport>,
    fixture_pid: u32,
    socket: &str,
    nonce: &str,
    callers: &[String],
    owned: &PostOnlyOwnedSpace,
    refusal: &str,
) -> Result<Vec<(String, u32)>, (i32, String)> {
    let (topology_readable, current_owned_space, topology_row_exact) =
        owned_topology_facts(leg.transport_mut(), owned);
    if !topology_readable || !current_owned_space || !topology_row_exact {
        return Err((
            1,
            post_only_placement_refusal(
                owned,
                &[],
                "restart fixture owned Space/topology changed before spawning windows; refusing",
                Some(current_owned_space),
            )
            .to_string(),
        ));
    }
    spawn_owned_windows(socket, nonce, callers).map_err(|error| (1, error))?;
    let joined =
        join_fixture_windows(fixture_pid, socket, nonce, callers).map_err(|error| (1, error))?;
    for (caller, wid) in &joined {
        leg.note_owned_window(caller, fixture_pid, *wid);
    }
    if let Err(observed) = await_joined_on_owned_space(&joined, owned.sid()) {
        return Err((
            1,
            post_only_placement_refusal(owned, &observed, refusal, None).to_string(),
        ));
    }
    Ok(joined)
}

/// One two-launch body. The caller always runs owned teardown, even if this
/// body fails before it can complete the restart proof.
#[cfg(target_os = "macos")]
#[allow(clippy::too_many_lines)]
pub(crate) fn run_restart_body_macos(
    leg: &mut LiveLeg<LiveLegTransport>,
    opts: &FixtureRestartOpts,
) -> Result<serde_json::Value, (i32, String)> {
    use crate::live_leg::PendingLaunch;
    let bundle = fixture_bundle_path().map_err(|error| (2, error))?;
    let run_dir = create_owned_run_dir("restart").map_err(|error| (2, error))?;
    let product_dir = run_dir.join("product");
    std::fs::create_dir(&product_dir)
        .map_err(|error| (1, format!("fixture-restart: product dir refused ({error})")))?;
    let launch1_dir = restart_launch_dir(&product_dir, 1);
    let launch2_dir = restart_launch_dir(&product_dir, 2);
    for dir in [&launch1_dir, &launch2_dir] {
        std::fs::create_dir(dir)
            .map_err(|error| (1, format!("fixture-restart: launch dir refused ({error})")))?;
    }
    let store_dir = product_dir.join(ISOLATED_STORE_LEAF);
    leg.note_pending_launch(PendingLaunch {
        run_dir: run_dir.clone(),
        expected_bundle: FIXTURE_SCOPE.to_owned(),
        what: "fixture",
    });
    if let Err(error) = launch_fixture_bundle(&bundle, &run_dir) {
        let _ = leg.resolve_pending_absence("fixture");
        return Err((1, error));
    }
    let announcement = read_announcement(&run_dir).map_err(|error| (1, error))?;
    let parsed = crate::fixture_post::parse_fixture_announcement(&announcement)
        .map_err(|error| (1, format!("fixture-restart: announcement refused: {error}")))?;
    let identity = crate::fixture_post::fixture_process_identity(parsed.pid)
        .map_err(|error| (1, format!("fixture-restart: identity unreadable ({error})")))?;
    if identity.bundle.as_deref() != Some(FIXTURE_SCOPE) {
        return Err((
            1,
            "fixture-restart: live bundle is not the signed fixture scope; refusing".to_owned(),
        ));
    }
    let (exe, launch) = match (identity.exe, identity.launch) {
        (Some(exe), Some(launch)) if !exe.is_empty() && !launch.is_empty() => (exe, launch),
        _ => {
            return Err((
                1,
                "fixture-restart: executable or launch date unreadable (no LaunchServices lifetime); refusing"
                    .to_owned(),
            ));
        }
    };
    let nonce = read_run_nonce(&run_dir).map_err(|error| (1, error))?;
    if nonce != parsed.nonce {
        return Err((
            1,
            "fixture-restart: nonce mismatch (stale or foreign run); refusing".to_owned(),
        ));
    }
    let fixture_record = FixtureRecord {
        pid: parsed.pid,
        exe,
        launch,
        socket_path: parsed.socket.clone(),
        run_dir: run_dir.clone(),
    };
    leg.register_fixture(fixture_record.clone())?;
    leg.transport_mut()
        .set_fixture_control(parsed.socket.clone(), run_dir.clone());
    let callers: Vec<String> = (0..opts.windows).map(|index| format!("w{index}")).collect();
    // The float chords and intent rows use the first `floats` callers; the
    // remaining callers are the tiles that must survive the restart. M4
    // floats nothing, so every caller is spawned before its single launch.
    let (float_siblings, survivors) = callers.split_at(opts.floats);
    // Establish and switch to the owned Space BEFORE spawning any fixture
    // window, matching the single-leg order: windows are born on the owned
    // Space, never on a user Space and then moved.
    let established = establish_post_only_owned_space(leg)?;
    let owned = established.evidence;
    let survivor_joined = spawn_join_owned_callers(
        leg,
        parsed.pid,
        &parsed.socket,
        &nonce,
        survivors,
        &owned,
        "restart fixture survivor windows are not readable on the owned Space; refusing without posting",
    )?;
    // No first product launch may start while a foreign product instance is
    // already alive. This runs before the pending product registration.
    match foreign_product_overlap(leg, parsed.pid) {
        Ok(false) => {}
        Ok(true) => {
            return Err((
                1,
                serde_json::json!({
                    "stage": "restart-launch1-preflight",
                    "error": "another product instance is active before the first launch; refusing",
                })
                .to_string(),
            ));
        }
        Err(error) => {
            return Err((
                1,
                serde_json::json!({
                    "stage": "restart-launch1-preflight",
                    "error": error,
                })
                .to_string(),
            ));
        }
    }
    let fixture_pid = parsed.pid;
    let run_id1 = restart_run_id(std::process::id(), fixture_pid, 1);
    let run_id2 = restart_run_id(std::process::id(), fixture_pid, 2);
    let product_bundle = product_bundle_path().map_err(|error| (2, error))?;
    let argv1 = product_argv(
        opts.seconds,
        opts.preset,
        opts.modifier,
        false,
        &launch1_dir,
        &[owned.sid()],
        Some(owned.starting_sid()),
    );
    std::fs::write(launch1_dir.join("run-id"), format!("{run_id1}\n")).map_err(|error| {
        (
            1,
            format!("fixture-restart: first run-id refused ({error})"),
        )
    })?;
    leg.note_pending_launch(PendingLaunch {
        run_dir: launch1_dir.clone(),
        expected_bundle: crate::BUNDLE_ID.to_owned(),
        what: "product",
    });
    let record1 = match launch_product_ls(
        &product_bundle,
        &launch1_dir,
        &store_dir,
        &argv1,
        &run_id1,
        false,
        None,
    ) {
        Ok(record) => record,
        Err(error) => {
            let _ = leg.resolve_pending_absence("product");
            return Err((1, error));
        }
    };
    leg.register_product_ls(record1)?;
    await_product_runtime_started(&launch1_dir)?;
    settle_hold(RESTART_STARTUP_SETTLE_MS)?;
    let (joined, survivor_baseline) = if opts.mode == RestartMode::M3 {
        // The survivor-only settled layout is only a valid fresh baseline
        // while the exact owned Space/topology and joined placement hold.
        let (topology_readable, current_owned_space, topology_row_exact) =
            owned_topology_facts(leg.transport_mut(), &owned);
        let placement = await_joined_on_owned_space(&survivor_joined, owned.sid());
        if !pre_chord_placement_holds(
            topology_readable,
            current_owned_space,
            topology_row_exact,
            placement.is_ok(),
        ) {
            let observed = placement.err().unwrap_or_default();
            return Err((
                1,
                post_only_placement_refusal(
                    &owned,
                    &observed,
                    "restart fixture owned Space/topology or survivor placement drifted before the fresh baseline capture; refusing without posting",
                    Some(current_owned_space),
                )
                .to_string(),
            ));
        }
        let baseline = await_settled_frames(fixture_pid, &survivor_joined)?;
        // Add the intended float siblings after the fresh baseline. Their
        // join/registration and placement are the only new windows touched;
        // the survivors keep their exact original WIDs.
        let mut ordered = spawn_join_owned_callers(
            leg,
            fixture_pid,
            &parsed.socket,
            &nonce,
            float_siblings,
            &owned,
            "restart fixture float siblings are not readable on the owned Space; refusing without posting",
        )?;
        // Original caller order is preserved for the later float chords and
        // intent rows (both read the first `floats` entries).
        ordered.extend(survivor_joined);
        if let Err(observed) = await_joined_on_owned_space(&ordered, owned.sid()) {
            return Err((
                1,
                post_only_placement_refusal(
                    &owned,
                    &observed,
                    "restart fixture windows are not readable on the owned Space after the float siblings joined; refusing without posting",
                    None,
                )
                .to_string(),
            ));
        }
        (ordered, Some(baseline))
    } else {
        (survivor_joined, None)
    };
    let pre = run_pre_stop_phase(
        leg,
        opts,
        &announcement,
        &nonce,
        &joined,
        &owned,
        fixture_pid,
        survivor_baseline,
    )?;
    let prior = leg.stop_product_for_restart().map_err(|error| {
        (
            1,
            serde_json::json!({"stage": "restart-stop-1", "error": error}).to_string(),
        )
    })?;
    let store_after_first = read_store_snapshot(&store_dir).map_err(|error| (1, error))?;
    run_post_restart_phase(
        leg,
        opts,
        &fixture_record,
        &nonce,
        &joined,
        &owned,
        &pre,
        &prior,
        &store_dir,
        &store_after_first,
        &run_id1,
        &run_id2,
        &launch2_dir,
    )
}
