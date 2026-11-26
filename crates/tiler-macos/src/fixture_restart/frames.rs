//! Live frame capture and the one between-product effect on macOS: exact
//! independent AX plus CG reads, settled-frame waits, the authenticated
//! signed-fixture nudge, and the product's runtime-started record. No chord
//! driver is constructible here.

use std::path::Path;

use crate::fixture_frame::FramePoint;
use crate::fixture_leg::FIXTURE_SCOPE;
use crate::space_probe::NativeSpaces;

use super::arrangement::{
    ArrangementEvidence, require_nudge_preimage_distinct, require_post_stop_arrangement,
};
use super::frame::WindowFrames;
use super::guard::OwnedRestartFixture;
use super::{
    RESTART_PRODUCT_START_DEADLINE_MS, RESTART_READ_INTERVAL_MS, RESTART_SETTLE_DEADLINE_MS,
    RESTART_STABLE_SPAN_MS,
};

/// Capture one exact independent frame set for every joined window: AX frame
/// plus CG bounds plus the CG onscreen fact. A missing/off-screen CG entry or
/// unreadable AX frame refuses; nothing is defaulted. Read-only.
#[cfg(target_os = "macos")]
pub(crate) fn capture_window_frames(
    pid: u32,
    joined: &[(String, u32)],
) -> Result<Vec<WindowFrames>, (i32, String)> {
    let cg = crate::ffi::cg_window_list_basic().map_err(|error| {
        (
            1,
            format!("restart frames: CG snapshot unreadable ({error})"),
        )
    })?;
    let mut frames = Vec::new();
    for (caller, wid) in joined {
        let snapshot = crate::fixture_sample::read_joined_frame(pid, *wid).map_err(|error| {
            (
                1,
                format!("restart frames: {caller} AX unreadable ({error})"),
            )
        })?;
        if snapshot.cg_onscreen != Some(true) {
            return Err((1, format!("restart frames: {caller} is not CG-onscreen")));
        }
        let entry = cg
            .iter()
            .find(|entry| entry.pid == pid && entry.wid == *wid && entry.onscreen)
            .ok_or_else(|| {
                (
                    1,
                    format!("restart frames: {caller} has no live CG owner entry"),
                )
            })?;
        frames.push(WindowFrames {
            caller: caller.clone(),
            wid: *wid,
            ax: snapshot.frame,
            cg: FramePoint {
                x: entry.bounds.x,
                y: entry.bounds.y,
                w: entry.bounds.w,
                h: entry.bounds.h,
            },
            cg_onscreen: true,
            zoomed: snapshot.zoomed,
        });
    }
    Ok(frames)
}

/// Wait for the single authenticated post-stop frame write to land exactly:
/// target AX and CG both equal the requested ordinary frame on the exact
/// target window ID, every other joined window unchanged (including WID), all
/// CG-onscreen. Bounded; a timeout reports the observed target frame and never
/// retries the write.
#[cfg(target_os = "macos")]
fn await_nudged_frames(
    pid: u32,
    joined: &[(String, u32)],
    before: &[WindowFrames],
    target: &WindowFrames,
    requested: &FramePoint,
) -> Result<Vec<WindowFrames>, (i32, String)> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        if crate::space_drag_diag_live::diag_interrupted() {
            return Err((
                1,
                "restart effect: interrupted after the post-stop nudge; refusing".to_owned(),
            ));
        }
        let current = capture_window_frames(pid, joined)?;
        let target_ok = current
            .iter()
            .find(|frame| frame.caller == target.caller)
            .is_some_and(|frame| {
                frame.wid == target.wid
                    && frame.ax == *requested
                    && frame.cg == *requested
                    && frame.cg_onscreen
            });
        let others_ok = before.iter().all(|frame| {
            frame.caller == target.caller
                || current
                    .iter()
                    .find(|candidate| candidate.caller == frame.caller)
                    .is_some_and(|candidate| {
                        candidate.wid == frame.wid
                            && candidate.ax == frame.ax
                            && candidate.cg == frame.cg
                    })
        });
        if target_ok && others_ok {
            return Ok(current);
        }
        if std::time::Instant::now() >= deadline {
            let observed = current
                .iter()
                .find(|frame| frame.caller == target.caller)
                .map(|frame| serde_json::json!({"ax": frame.ax, "cg": frame.cg, "wid": frame.wid}))
                .unwrap_or_else(|| serde_json::json!({"absent": true}));
            return Err((
                1,
                serde_json::json!({
                    "stage": "restart-effect",
                    "error": "the post-stop nudge did not settle to the exact requested AX and CG frame",
                    "target": target.caller,
                    "target_wid": target.wid,
                    "requested": requested,
                    "observed": observed,
                })
                .to_string(),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// The one M3 post-stop effect: nudge one known tile inward by an ordinary
/// inset through the existing signed-fixture frame-write choke point (fresh
/// socket nonce, bundle/exe/launch/signature proof, single AX write). The
/// commit boundary additionally rechecks the owned Space row and the exact
/// expected fixture lifetime before that write. Afterwards, independently
/// read exact AX plus CG frames and require the target changed to the
/// requested frame with every other window unchanged. No product is alive or
/// launched in this phase.
#[cfg(target_os = "macos")]
pub(crate) fn nudge_one_known_tile(
    context: OwnedRestartFixture<'_>,
    arrangement: &ArrangementEvidence,
    signer: &str,
) -> Result<serde_json::Value, (i32, String)> {
    let fixture = context.fixture;
    let joined = context.joined;
    if crate::space_drag_diag_live::diag_interrupted() {
        return Err((
            1,
            "restart effect: interrupted before the post-stop nudge; refusing".to_owned(),
        ));
    }
    let target = arrangement.tiles.first().ok_or_else(|| {
        (
            1,
            "restart effect: no pre-stop tile is available to nudge; refusing".to_owned(),
        )
    })?;
    let before = capture_window_frames(fixture.pid, joined)?;
    // No window may have drifted while no product was alive: prove the
    // exact pre-stop per-window arrangement (caller+WID set, AX and CG
    // frames, CG onscreen) before the one effect.
    require_post_stop_arrangement("m3-pre-nudge", arrangement, &before)
        .map_err(|error| (1, error))?;
    let before_target = before
        .iter()
        .find(|frame| frame.caller == target.caller)
        .ok_or_else(|| {
            (
                1,
                format!(
                    "restart effect: nudge target {} is absent from the independent capture; refusing",
                    target.caller
                ),
            )
        })?;
    const INSET: f64 = 16.0;
    if before_target.ax.w <= INSET * 2.0 || before_target.ax.h <= INSET * 2.0 {
        return Err((
            1,
            format!(
                "restart effect: nudge target {} is too small for an ordinary inset; refusing",
                target.caller
            ),
        ));
    }
    let requested = FramePoint {
        x: before_target.ax.x + INSET,
        y: before_target.ax.y + INSET,
        w: before_target.ax.w - INSET * 2.0,
        h: before_target.ax.h - INSET * 2.0,
    };
    // The requested frame must be distinguishable from every pre-stop tile
    // in both independent reads; otherwise the nudge could reproduce the
    // settled arrangement (or collide with a sibling) and prove nothing.
    require_nudge_preimage_distinct(&requested, &arrangement.tiles)
        .map_err(|error| (1, error))?;
    let args = crate::fixture_frame::FixtureFrameWriteArgs {
        announcement_file: fixture
            .run_dir
            .join("announcement.json")
            .to_string_lossy()
            .into_owned(),
        caller: target.caller.clone(),
        wid: target.wid,
        x: requested.x,
        y: requested.y,
        w: requested.w,
        h: requested.h,
        signer: signer.to_owned(),
    };
    // The signed writer's commit boundary rechecks the owned Space and the
    // exact expected fixture lifetime before its single AX write. Standalone
    // frame-write invocations pass no check and keep their existing proof.
    let fixture_pid = fixture.pid;
    let expected_exe = fixture.exe.clone();
    let expected_launch = fixture.launch.clone();
    let owned_sid = context.owned.sid();
    let owned_spaces = context.owned.spaces_after().to_vec();
    let commit_check: Option<crate::fixture_frame::FixtureFrameCommitCheck> =
        Some(Box::new(move || {
            let identity =
                crate::ffi::ns_app_identity_for_pid(i32::try_from(fixture_pid).unwrap_or(0));
            if identity.bundle.as_deref() != Some(FIXTURE_SCOPE)
                || identity.exe.as_deref() != Some(expected_exe.as_str())
                || identity.launch_date.as_deref() != Some(expected_launch.as_str())
            {
                return Err("restart effect commit: fixture lifetime drifted; refusing".to_owned());
            }
            let mut spaces = crate::space_probe::LiveNativeSpaces::new(
                crate::live_leg::LEG_CAFFEINATE_SECONDS,
            );
            let topology = spaces.space_snapshot().map_err(|error| {
                format!("restart effect commit: owned-Space snapshot unreadable ({error})")
            })?;
            let row = topology
                .displays
                .first()
                .ok_or_else(|| "restart effect commit: no display row".to_owned())?;
            if row.current != owned_sid || row.spaces.as_slice() != owned_spaces.as_slice() {
                return Err(
                    "restart effect commit: owned Space is no longer current or its row changed; refusing"
                        .to_owned(),
                );
            }
            Ok(())
        }));
    let raw = crate::fixture_frame::run_fixture_frame_write_under_interrupt(&args, commit_check)
        .map_err(|(code, body)| {
            (
                code,
                format!("restart effect: authenticated frame write refused ({body})"),
            )
        })?;
    let write: serde_json::Value = serde_json::from_str(&raw).map_err(|error| {
        (
            1,
            format!("restart effect: frame-write report is not JSON ({error})"),
        )
    })?;
    if write.get("outcome").and_then(serde_json::Value::as_str) != Some("written") {
        return Err((
            1,
            serde_json::json!({
                "stage": "restart-effect",
                "error": "the authenticated post-stop frame write did not report a written outcome",
                "write": write,
            })
            .to_string(),
        ));
    }
    let after = await_nudged_frames(fixture.pid, joined, &before, target, &requested)?;
    let after_target = after
        .iter()
        .find(|frame| frame.caller == target.caller)
        .cloned()
        .ok_or_else(|| {
            (
                1,
                format!(
                    "restart effect: nudge target {} vanished after the write; refusing",
                    target.caller
                ),
            )
        })?;
    Ok(serde_json::json!({
        "target_caller": target.caller,
        "target_wid": target.wid,
        "requested": requested,
        "before": before_target,
        "after": after_target,
        "ax_exact": true,
        "cg_exact": true,
        "other_windows_unchanged": true,
        "write": {
            "outcome": "written",
            "position_status": write.get("position_status").cloned(),
            "size_status": write.get("size_status").cloned(),
        },
    }))
}

/// True when one product stderr line is the runtime's own started record.
/// The log is per launch output directory; the record carries pid 0 by
/// design, so only the fixed kind/outcome pair is matched. Pure;
/// unit-tested.
pub(crate) fn runtime_started_seen(raw: &str) -> bool {
    raw.lines()
        .any(|line| line.contains("kind=runtime outcome=started"))
}

/// Bounded poll for the product's runtime-started stderr record. The
/// `tilestarted` announcement is printed before runtime startup, so this is
/// the earliest honest point to begin frame settlement. A log that never
/// arrives refuses rather than capturing pre-runtime frames.
#[cfg(target_os = "macos")]
pub(crate) fn await_product_runtime_started(product_dir: &Path) -> Result<(), (i32, String)> {
    let log = product_dir.join("stderr.log");
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_millis(RESTART_PRODUCT_START_DEADLINE_MS);
    loop {
        if let Ok(raw) = std::fs::read_to_string(&log)
            && runtime_started_seen(&raw)
        {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err((
                1,
                format!(
                    "restart product: no runtime-started evidence in {} within {RESTART_PRODUCT_START_DEADLINE_MS}ms; refusing before capture",
                    log.display()
                ),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Wait for one settled frame set: two consecutive exact captures agreeing
/// for at least [`RESTART_STABLE_SPAN_MS`] within
/// [`RESTART_SETTLE_DEADLINE_MS`]. Every capture is an independent AX plus CG
/// read; a failed read or an unsettled deadline refuses rather than
/// capturing a transient frame.
#[cfg(target_os = "macos")]
pub(crate) fn await_settled_frames(
    pid: u32,
    joined: &[(String, u32)],
) -> Result<Vec<WindowFrames>, (i32, String)> {
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_millis(RESTART_SETTLE_DEADLINE_MS);
    let mut previous: Option<Vec<WindowFrames>> = None;
    let mut stable_since = std::time::Instant::now();
    loop {
        if crate::space_drag_diag_live::diag_interrupted() {
            return Err((
                1,
                "fixture-restart: interrupted (SIGINT/SIGTERM latched); restoring".to_owned(),
            ));
        }
        let current = capture_window_frames(pid, joined)?;
        match &previous {
            Some(last) if *last == current => {
                if stable_since.elapsed().as_millis() as u64 >= RESTART_STABLE_SPAN_MS {
                    return Ok(current);
                }
            }
            _ => {
                previous = Some(current);
                stable_since = std::time::Instant::now();
            }
        }
        if std::time::Instant::now() >= deadline {
            return Err((
                1,
                format!(
                    "fixture-restart: frames did not settle within {RESTART_SETTLE_DEADLINE_MS}ms; refusing"
                ),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(RESTART_READ_INTERVAL_MS));
    }
}

/// One refusal shape for a latched SIGINT/SIGTERM at an explicit pre-stop
/// sequence point (the owned teardown still runs on the error path).
#[cfg(target_os = "macos")]
pub(crate) fn interrupted_restart_refusal() -> (i32, String) {
    (
        1,
        "fixture-restart: interrupted (SIGINT/SIGTERM latched); restoring".to_owned(),
    )
}
