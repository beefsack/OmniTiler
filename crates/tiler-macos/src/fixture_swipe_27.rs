#![forbid(unsafe_code)]

//! Signed-fixture-only probe of the macOS 27 swipe candidate; no product dispatch.

use std::time::{Duration, Instant};

use core_graphics::display::CGDisplay;
use tiler_core::workspace_scope::{
    PreviousTarget, ScopeHistory, resolve_numbered, resolve_relative_step,
};

use crate::fixture_leg::{SWIPE_27_E1_CHORD, join_fixture_windows, spawn_owned_windows_at};
use crate::fixture_owned_space::{await_joined_on_owned_space, establish_post_only_owned_space_on};
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

struct DisplayPin {
    row: usize,
    uuid: String,
    cg_id: u32,
    bounds: core_graphics::geometry::CGRect,
    pointer: (f64, f64),
    position: Option<[i32; 2]>,
}

impl DisplayPin {
    fn read(entry: &TopologySnapshot, e1: bool) -> Result<Self, String> {
        let (builtin, pointer) = display_and_pointer()?;
        let mut cg_id = CGDisplay::main().id;
        let mut uuid = builtin;
        let mut position = None;
        if e1 {
            let screens = crate::ffi::ns_display_snapshot()?;
            let [a, b] = screens.as_slice() else {
                return Err("E1 requires exactly two extended displays".to_owned());
            };
            let external = match (a.cg_id == cg_id, b.cg_id == cg_id) {
                (true, false) => b,
                (false, true) => a,
                _ => return Err("E1 cannot identify the other display".to_owned()),
            };
            cg_id = external.cg_id;
            uuid = external.uuid.clone().ok_or("external UUID unreadable")?;
            let bounds = CGDisplay::new(cg_id).bounds();
            // Winit converts the initial frame using the current main screen's
            // backing scale. Fit within the other display under either scale.
            let place = |origin: f64, size: f64, window: f64| {
                let lo = (origin * a.scale).max(origin * b.scale).ceil();
                let hi = ((origin + size) * a.scale)
                    .min((origin + size) * b.scale)
                    .floor();
                (hi - lo >= window + 160.0).then(|| (lo + (hi - lo - window) / 2.0) as i32)
            };
            position = Some([
                place(bounds.origin.x, bounds.size.width, 400.0).ok_or("E1 no safe x placement")?,
                place(bounds.origin.y, bounds.size.height, 300.0)
                    .ok_or("E1 no safe y placement")?,
            ]);
        }
        let mut matching = entry
            .displays
            .iter()
            .enumerate()
            .filter(|(_, row)| row.identifier.as_deref() == Some(uuid.as_str()));
        let row = matching.next().ok_or("CG UUID has no native row")?.0;
        if matching.next().is_some()
            || if e1 {
                entry.displays.len() != 2
            } else {
                row != 0
            }
        {
            return Err("native rows do not uniquely pin the gesture display".to_owned());
        }
        let bounds = CGDisplay::new(cg_id).bounds();
        if e1 && bounds.contains(&core_graphics::geometry::CGPoint::new(pointer.0, pointer.1)) {
            return Err("E1 pointer overlaps the target display".to_owned());
        }
        Ok(Self {
            row,
            uuid,
            cg_id,
            bounds,
            pointer,
            position,
        })
    }
}

fn observe(before: &TopologySnapshot, pin: &DisplayPin) -> Result<u64, String> {
    let after = snapshot()?;
    let sid = after
        .displays
        .get(pin.row)
        .and_then(|row| row.current)
        .ok_or("target row current unreadable")?;
    validate_goto_effect(before, &after, pin.row, sid).map_err(|e| e.to_string())?;
    if crate::synth::cursor_position() != Some(pin.pointer) {
        return Err("pointer changed; no further swipe".to_owned());
    }
    if !live::mc_closed_verified().map_err(|e| e.to_string())? {
        return Err("Mission Control opened; no further swipe".to_owned());
    }
    Ok(sid)
}

fn goto(leg: &mut LiveLeg<LiveLegTransport>, row: usize, sid: u64) -> Result<(), String> {
    let mc = leg.transport_mut().space_goto(row, sid)?;
    mc.shortfall_text("fixture setup goto").map_or(Ok(()), Err)
}

fn spawn(
    leg: &mut LiveLeg<LiveLegTransport>,
    pid: u32,
    socket: &str,
    nonce: &str,
    caller: &str,
    sid: u64,
    position: Option<[i32; 2]>,
) -> Result<(String, u32), String> {
    let callers = [caller.to_owned()];
    spawn_owned_windows_at(socket, nonce, &callers, position)?;
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
    pin: &DisplayPin,
    before: &TopologySnapshot,
    (from, to): (u64, u64),
    ((caller, wid), target_wid): (&(String, u32), u32),
) -> Result<(u32, u128), String> {
    let current = || {
        if observe(before, pin)? == from {
            Ok(())
        } else {
            Err(format!("expected current Space {from} before post"))
        }
    };
    current()?;
    driver.focus(caller, *wid).map_err(|(_, e)| e)?;
    crate::fixture_leg::settle_hold(crate::fixture_leg::FOCUS_SETTLE_MS).map_err(|(_, e)| e)?;
    let (sign, steps) =
        crate::space_switch_edges::swipe_plan(&before.displays[pin.row].spaces, from, to)?;
    let plan = prepare_gesture_27(sign, steps)?;
    if pin.position.is_some() {
        plan.locate(objc2_core_foundation::CGPoint::new(
            pin.bounds.origin.x + pin.bounds.size.width / 2.0,
            pin.bounds.origin.y + pin.bounds.size.height / 2.0,
        ));
    }
    let mut started = Instant::now();
    let posted = plan.post_after(|| {
        let bounds = CGDisplay::new(pin.cg_id).bounds();
        if crate::ffi::cg_display_uuid(pin.cg_id).as_deref() != Some(&pin.uuid)
            || bounds.origin.x != pin.bounds.origin.x
            || bounds.origin.y != pin.bounds.origin.y
            || bounds.size.width != pin.bounds.size.width
            || bounds.size.height != pin.bounds.size.height
        {
            return Err("gesture display geometry or UUID changed".to_owned());
        }
        current()?;
        require_user_temp(to).map_err(|e| e.to_string())?;
        if crate::ffi::sls_spaces_for_window(*wid)? != [from]
            || crate::ffi::sls_spaces_for_window(target_wid)? != [to]
        {
            return Err("fixture window no longer belongs to its exact owned Space".to_owned());
        }
        driver.prove_gesture_target(caller, *wid)?;
        crate::space_synthetic_gate::require_window_target(pid, *wid, pin.row, Some(&pin.uuid))?;
        started = Instant::now();
        Ok(())
    })?;
    let mut arrival = None;
    loop {
        let sid = observe(before, pin).map_err(|e| format!("posted {posted}: {e}"))?;
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
    chord: &str,
) -> Result<serde_json::Value, (i32, String)> {
    let setup = |e: String| (1, format!("{chord} setup: {e}"));
    // Check version and Dock gesture preferences before any Space mutation.
    drop(prepare_gesture_27(1.0, 1).map_err(setup)?);
    let entry = snapshot().map_err(setup)?;
    let e1 = chord == SWIPE_27_E1_CHORD;
    let pin = DisplayPin::read(&entry, e1).map_err(setup)?;
    let row = pin.row;
    let established = establish_post_only_owned_space_on(leg, row)?;
    let source = established.sid;
    let a = spawn(leg, pid, socket, nonce, "swipe-a", source, pin.position).map_err(setup)?;
    let created = leg.transport_mut().space_create(row).map_err(setup)?;
    let dest = created.sid;
    leg.note_owned_space(row, dest);
    if let Some(error) = created.mc.shortfall_text("fixture destination creation") {
        return Err(setup(error));
    }
    leg.transport_mut()
        .space_require_empty(dest)
        .map_err(setup)?;
    goto(leg, row, dest).map_err(setup)?;
    let b = spawn(leg, pid, socket, nonce, "swipe-b", dest, pin.position).map_err(setup)?;
    goto(leg, row, source).map_err(setup)?;
    let before = snapshot().map_err(setup)?;
    let mut expected = entry;
    expected.displays[row].spaces.extend([source, dest]);
    validate_goto_effect(&expected, &before, row, source).map_err(|e| setup(e.to_string()))?;
    let driver = FixtureDriver::bind(
        &LiveTransport,
        announcement,
        nonce,
        signer,
        leg.admit(pid, Some(chord))?,
    )?;
    let ring: Vec<_> = before.displays[row]
        .spaces
        .iter()
        .map(u64::to_string)
        .collect();
    let n = u8::try_from(ring.len()).map_err(|e| setup(e.to_string()))?;
    let (source_text, dest_text) = (source.to_string(), dest.to_string());
    let mut history = ScopeHistory::new();
    history.observe(chord, &source_text, &ring);
    let mut completed = Vec::with_capacity(3);
    for (kind, from, to, window, target_wid, exact) in [
        ("numbered", source, dest, &a, b.1, &dest_text),
        ("relative", dest, source, &b, a.1, &source_text),
        ("previous", source, dest, &a, b.1, &dest_text),
    ]
    .into_iter()
    .take(if e1 { 1 } else { 3 })
    {
        let resolved = match kind {
            "numbered" => resolve_numbered(&ring, n),
            "relative" => resolve_relative_step(&ring, &dest_text, -1),
            _ => match history.resolve_previous_for(chord, &ring, &source_text) {
                PreviousTarget::Select(sid) => Some(sid),
                _ => None,
            },
        };
        let result = if resolved.as_ref() == Some(exact) {
            swipe(
                &driver,
                pid,
                &pin,
                &before,
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
                        "stage": chord, "kind": kind, "from": from, "to": to,
                        "error": error, "completed": completed,
                    })
                    .to_string(),
                ));
            }
        }
        history.observe(chord, exact, &ring);
    }
    Ok(serde_json::json!({
        "arm": chord, "display": row, "legs": completed,
        "stable_ms": STABLE.as_millis(), "pointer_unchanged": true,
        "other_displays_unchanged": true, "mission_control_observed": false,
    }))
}
