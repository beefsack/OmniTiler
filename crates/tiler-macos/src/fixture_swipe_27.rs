#![forbid(unsafe_code)]

//! Signed-fixture-only probe of the macOS 27 swipe candidate; no product dispatch.

use std::time::{Duration, Instant};

use core_graphics::display::CGDisplay;
use tiler_core::workspace_scope::{
    PreviousTarget, ScopeHistory, resolve_numbered, resolve_relative_step,
};

use crate::fixture_leg::{SWIPE_27_CHORD, join_fixture_windows, spawn_owned_windows};
use crate::fixture_owned_space::{await_joined_on_owned_space, establish_post_only_owned_space};
use crate::fixture_post::{FixtureDriver, LiveTransport};
use crate::live_leg::{LiveLeg, LiveLegTransport};
use crate::mc_lifecycle::{live, require_user_temp};
use crate::mc_probe::{TopologySnapshot, validate_goto_effect};
use crate::space_probe::NativeSpaces;
use crate::space_switch_candidate_27::prepare_gesture_27;

const STABLE: Duration = Duration::from_millis(400);
const POLL: Duration = Duration::from_millis(25);
const DEADLINE: Duration = Duration::from_millis(2500);

fn snapshot() -> Result<TopologySnapshot, String> {
    live::fresh_snapshot().map_err(|e| e.to_string())
}

fn display_and_pointer() -> Result<(String, (f64, f64)), String> {
    let display = CGDisplay::main();
    if !display.is_builtin() {
        return Err("the main CG display is not built-in".to_owned());
    }
    let uuid = crate::ffi::cg_display_uuid(display.id).ok_or("built-in UUID unreadable")?;
    let (x, y) = crate::synth::cursor_position().ok_or("pointer unreadable")?;
    if !display
        .bounds()
        .contains(&core_graphics::geometry::CGPoint::new(x, y))
    {
        return Err("pointer is not on the built-in display".to_owned());
    }
    Ok((uuid, (x, y)))
}

fn observe(before: &TopologySnapshot, pointer: (f64, f64)) -> Result<u64, String> {
    let after = snapshot()?;
    let sid = after
        .displays
        .first()
        .and_then(|row| row.current)
        .ok_or("row 0 current unreadable")?;
    validate_goto_effect(before, &after, 0, sid).map_err(|e| e.to_string())?;
    if crate::synth::cursor_position() != Some(pointer) {
        return Err("pointer changed; no further swipe".to_owned());
    }
    if !live::mc_closed_verified().map_err(|e| e.to_string())? {
        return Err("Mission Control opened; no further swipe".to_owned());
    }
    Ok(sid)
}

fn goto(leg: &mut LiveLeg<LiveLegTransport>, sid: u64) -> Result<(), String> {
    let mc = leg.transport_mut().space_goto(0, sid)?;
    mc.shortfall_text("fixture setup goto").map_or(Ok(()), Err)
}

fn spawn(
    leg: &mut LiveLeg<LiveLegTransport>,
    pid: u32,
    socket: &str,
    nonce: &str,
    caller: &str,
    sid: u64,
) -> Result<(String, u32), String> {
    let callers = [caller.to_owned()];
    spawn_owned_windows(socket, nonce, &callers)?;
    let joined = join_fixture_windows(pid, socket, nonce, &callers)?;
    for (caller, wid) in &joined {
        leg.note_owned_window(caller, pid, *wid);
    }
    await_joined_on_owned_space(&joined, sid)
        .map_err(|_| format!("fixture window {caller} not on owned Space {sid}"))?;
    joined
        .into_iter()
        .next()
        .ok_or_else(|| format!("fixture window {caller} unjoined"))
}

#[allow(clippy::too_many_arguments)]
fn swipe(
    driver: &FixtureDriver<'_, LiveTransport>,
    pid: u32,
    uuid: &str,
    before: &TopologySnapshot,
    pointer: (f64, f64),
    (from, to): (u64, u64),
    ((caller, wid), target_wid): (&(String, u32), u32),
) -> Result<(u32, u128), String> {
    let current = || {
        if observe(before, pointer)? == from {
            Ok(())
        } else {
            Err(format!("expected current Space {from} before post"))
        }
    };
    current()?;
    driver.focus(caller, *wid).map_err(|(_, e)| e)?;
    crate::fixture_leg::settle_hold(crate::fixture_leg::FOCUS_SETTLE_MS).map_err(|(_, e)| e)?;
    let (sign, steps) =
        crate::space_switch_edges::swipe_plan(&before.displays[0].spaces, from, to)?;
    let plan = prepare_gesture_27(sign, steps)?;
    let mut started = Instant::now();
    let posted = plan.post_after(|| {
        let live_display = display_and_pointer()?;
        if live_display.0 != uuid || live_display.1 != pointer {
            return Err("built-in CG display or pointer changed".to_owned());
        }
        current()?;
        require_user_temp(to).map_err(|e| e.to_string())?;
        if crate::ffi::sls_spaces_for_window(*wid)? != [from]
            || crate::ffi::sls_spaces_for_window(target_wid)? != [to]
        {
            return Err("fixture window no longer belongs to its exact owned Space".to_owned());
        }
        driver.prove_gesture_target(caller, *wid)?;
        crate::space_synthetic_gate::require_window_target(pid, *wid, 0, Some(uuid))?;
        started = Instant::now();
        Ok(())
    })?;
    let mut arrival = None;
    loop {
        let sid = observe(before, pointer).map_err(|e| format!("posted {posted}: {e}"))?;
        let elapsed = started.elapsed();
        if sid == to {
            let first = *arrival.get_or_insert(elapsed);
            if elapsed - first >= STABLE {
                return Ok((posted, first.as_millis()));
            }
        } else if sid != from || arrival.is_some() {
            return Err(format!(
                "posted {posted}: wrong arrival or bounce to {sid}; wanted {to}"
            ));
        }
        if elapsed >= DEADLINE {
            return Err(format!(
                "posted {posted}: current {sid}, no stable arrival on {to}"
            ));
        }
        std::thread::sleep(POLL);
    }
}

pub(crate) fn run(
    leg: &mut LiveLeg<LiveLegTransport>,
    pid: u32,
    socket: &str,
    nonce: &str,
    announcement: &str,
    signer: &str,
) -> Result<serde_json::Value, (i32, String)> {
    let setup = |e: String| (1, format!("swipe-27 setup: {e}"));
    // Check version and Dock gesture preferences before any Space mutation.
    drop(prepare_gesture_27(1.0, 1).map_err(setup)?);
    let (uuid, pointer) = display_and_pointer().map_err(setup)?;
    let entry = snapshot().map_err(setup)?;
    let matching =
        |row: &crate::mc_probe::DisplaySpaces| row.identifier.as_deref() == Some(uuid.as_str());
    if !entry.displays.first().is_some_and(matching)
        || entry.displays.iter().filter(|row| matching(row)).count() != 1
    {
        return Err(setup(
            "built-in CG UUID does not uniquely name native row 0".to_owned(),
        ));
    }
    let established = establish_post_only_owned_space(leg)?;
    let source = established.sid;
    let a = spawn(leg, pid, socket, nonce, "swipe-a", source).map_err(setup)?;
    let created = leg.transport_mut().space_create(0).map_err(setup)?;
    let dest = created.sid;
    leg.note_owned_space(0, dest);
    if let Some(error) = created.mc.shortfall_text("fixture destination creation") {
        return Err(setup(error));
    }
    leg.transport_mut()
        .space_require_empty(dest)
        .map_err(setup)?;
    goto(leg, dest).map_err(setup)?;
    let b = spawn(leg, pid, socket, nonce, "swipe-b", dest).map_err(setup)?;
    goto(leg, source).map_err(setup)?;
    let before = snapshot().map_err(setup)?;
    let mut expected = entry;
    expected.displays[0].spaces.extend([source, dest]);
    validate_goto_effect(&expected, &before, 0, source).map_err(|e| setup(e.to_string()))?;
    let driver = FixtureDriver::bind(
        &LiveTransport,
        announcement,
        nonce,
        signer,
        leg.admit(pid, Some(SWIPE_27_CHORD))?,
    )?;
    let ring: Vec<_> = before.displays[0]
        .spaces
        .iter()
        .map(u64::to_string)
        .collect();
    let n = u8::try_from(ring.len()).map_err(|e| setup(e.to_string()))?;
    let (source_text, dest_text) = (source.to_string(), dest.to_string());
    let mut history = ScopeHistory::new();
    history.observe(SWIPE_27_CHORD, &source_text, &ring);
    let mut completed = Vec::with_capacity(3);
    for (kind, from, to, window, target_wid, exact) in [
        ("numbered", source, dest, &a, b.1, &dest_text),
        ("relative", dest, source, &b, a.1, &source_text),
        ("previous", source, dest, &a, b.1, &dest_text),
    ] {
        let resolved = match kind {
            "numbered" => resolve_numbered(&ring, n),
            "relative" => resolve_relative_step(&ring, &dest_text, -1),
            _ => match history.resolve_previous_for(SWIPE_27_CHORD, &ring, &source_text) {
                PreviousTarget::Select(sid) => Some(sid),
                _ => None,
            },
        };
        let result = if resolved.as_ref() == Some(exact) {
            swipe(
                &driver,
                pid,
                &uuid,
                &before,
                pointer,
                (from, to),
                (window, target_wid),
            )
        } else {
            Err(format!("resolver returned {resolved:?}; expected {to}"))
        };
        match result {
            Ok((events, ms)) => completed.push(serde_json::json!({
                "kind": kind, "from": from, "to": to, "events": events, "arrival_ms": ms,
            })),
            Err(error) => {
                return Err((
                    1,
                    serde_json::json!({
                        "stage": "swipe-27", "kind": kind, "from": from, "to": to,
                        "error": error, "completed": completed,
                    })
                    .to_string(),
                ));
            }
        }
        history.observe(SWIPE_27_CHORD, exact, &ring);
    }
    Ok(serde_json::json!({
        "arm": SWIPE_27_CHORD, "display": 0, "legs": completed,
        "stable_ms": STABLE.as_millis(), "pointer_unchanged": true,
        "other_displays_unchanged": true, "mission_control_observed": false,
    }))
}
