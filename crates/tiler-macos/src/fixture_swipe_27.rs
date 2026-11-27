#![forbid(unsafe_code)]

//! Signed-fixture-only probe of the macOS 27 swipe candidate; no product dispatch.

use std::time::{Duration, Instant};

use core_graphics::display::CGDisplay;
use tiler_core::workspace_scope::{
    PreviousTarget, ScopeHistory, resolve_numbered, resolve_relative_step,
};

use crate::fixture_leg::{SWIPE_27_E1_CHORD, join_fixture_windows, spawn_owned_windows};
use crate::fixture_owned_space::{await_joined_on_owned_space, establish_post_only_owned_space_on};
use crate::fixture_post::{FixtureDriver, LiveTransport};
use crate::live_leg::{FixtureFullscreenExit, LiveLeg, LiveLegTransport};
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
            // Winit converts runtime positions using the window's current
            // scale. Fit on the target display under either scale.
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
    (socket, nonce): (&str, &str),
    caller: &str,
    sid: u64,
    pin: &DisplayPin,
    driver: &FixtureDriver<'_, LiveTransport>,
) -> Result<(String, u32), String> {
    let callers = [caller.to_owned()];
    spawn_owned_windows(socket, nonce, &callers)?;
    let joined = join_fixture_windows(pid, socket, nonce, &callers)?;
    for (caller, wid) in &joined {
        leg.note_owned_window(caller, pid, *wid);
    }
    let [window] = joined.as_slice() else {
        return Err(format!("fixture window {caller} did not join uniquely"));
    };
    if let Some(position) = pin.position {
        let before = snapshot()?;
        driver.place_window(window, position, || {
            if before
                .displays
                .get(pin.row)
                .and_then(|row| row.identifier.as_deref())
                != Some(&pin.uuid)
                || observe(&before, pin)? != sid
            {
                return Err(format!("target display no longer on owned Space {sid}"));
            }
            Ok(())
        })?;
    }
    await_joined_on_owned_space(&joined, sid).map_err(|observed| {
        format!("fixture window {caller} not on owned Space {sid}: {observed:?}")
    })?;
    Ok(joined
        .into_iter()
        .next()
        .expect("the joined slice has one window"))
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

/// Switch away from the lingering detached Space with exactly one pinned
/// Mission Control select of the leg's original baseline current (never the
/// temp setup Space, never replayed).
fn fallback_to_baseline(
    leg: &LiveLeg<LiveLegTransport>,
    driver: &FixtureDriver<'_, LiveTransport>,
    pid: u32,
    pin: &DisplayPin,
    (caller, wid): &(String, u32),
    before: &TopologySnapshot,
    sid: u64,
) -> Result<(), String> {
    use crate::ffi::{self, SlsSpaceType};
    let target = leg
        .baseline()
        .native
        .displays
        .iter()
        .find(|row| row.index == pin.row)
        .map(|row| row.current)
        .ok_or("baseline lacks the pinned starting current")?;
    if target == sid {
        return Err("baseline starting current is the detached SID".to_owned());
    }
    let prior = snapshot()?;
    let mut retired = prior.clone();
    let Some(row) = retired.displays.get_mut(pin.row) else {
        return Err("pinned display missing from the switch preimage".to_owned());
    };
    row.spaces.retain(|space| *space != sid);
    row.current = Some(target);
    validate_goto_effect(before, &retired, pin.row, target).map_err(|e| e.to_string())?;
    if prior.displays.iter().map(|row| row.spaces.iter().filter(|&&s| s == sid).count()).sum::<usize>()
        > usize::from(prior.locate(sid).is_some_and(|(row, _)| row == pin.row))
    {
        return Err("detached SID is ambiguous or moved off the pinned display".to_owned());
    }
    let Some(pinned) = prior.displays.get(pin.row) else {
        return Err("pinned display missing from the switch preimage".to_owned());
    };
    let prove_sid = || {
        if ffi::sls_space_type(sid)? != SlsSpaceType::Fullscreen
            || ffi::sls_spaces_for_window(*wid)? != [sid]
            || !ffi::cg_window_list_basic()?
                .iter()
                .any(|window| window.wid == *wid && window.pid == pid)
        {
            return Err("detached SID is not the exact owned fixture fullscreen Space".to_owned());
        }
        Ok(())
    };
    // No dispatch when a fresh read already proves the baseline current and
    // no detached current, with the exact SID lingering only in place.
    if pinned.current == Some(target) {
        if pinned.spaces.contains(&sid) {
            prove_sid()?;
        }
        return Ok(());
    }
    if pinned.current != Some(sid) || prior.locate(target).map(|(row, _)| row) != Some(pin.row) {
        return Err("pinned source is not the detached SID on the baseline display".to_owned());
    }
    let check_source = || {
        if snapshot()?.displays.get(pin.row).and_then(|row| row.current) != Some(sid) {
            return Err("detached SID is no longer the pinned current".to_owned());
        }
        prove_sid()?;
        driver.prove_source_incarnation(caller, *wid)
    };
    let mut budget = crate::mc_probe::ActionBudget::new(45);
    let outcome = live::fixture_goto_pinned(
        target,
        (pin.row, pin.cg_id, &pin.uuid),
        &prior,
        &mut budget,
        check_source,
    )
    .map_err(|e| e.to_string())?;
    if outcome.mc_open || outcome.close_error.is_some() {
        return Err("pinned baseline switch lacks an exact Mission Control close".to_owned());
    }
    let mut latest = outcome
        .after
        .ok_or("pinned baseline switch left no post-press snapshot")?;
    let started = Instant::now();
    loop {
        // The same ordered topology, except the tracked SID may retire.
        match validate_goto_effect(&prior, &latest, pin.row, target)
            .or_else(|_| validate_goto_effect(&retired, &latest, pin.row, target))
        {
            Ok(()) => return Ok(()),
            Err(e) if started.elapsed() >= DEADLINE => return Err(e.to_string()),
            Err(_) => {}
        }
        ffi::pump_runloop_once(25);
        latest = snapshot()?;
    }
}

/// Explicit windowed request for the exact owned fixture window first,
/// bounded SLS retirement next, then one guarded pinned switch to the leg
/// baseline on shortfall.
fn exit_fullscreen(
    leg: &mut LiveLeg<LiveLegTransport>,
    driver: &FixtureDriver<'_, LiveTransport>,
    pid: u32,
    pin: &DisplayPin,
    window: &(String, u32),
    before: &TopologySnapshot,
    detached: Option<u64>,
) -> Result<(), String> {
    if let Some(error) = driver.set_probe_fullscreen(window, false).err() {
        eprintln!("fixture fullscreen windowed request refused: {error}");
    }
    let Some(sid) = detached else {
        // No observed entry: no SID to await, so destructive teardown stays
        // withheld (conservative).
        let reason = "fullscreen entry unobserved; exit unproven".to_owned();
        leg.note_fixture_fullscreen_exit(FixtureFullscreenExit::Unsafe(reason.clone()));
        return Err(reason);
    };
    if leg.await_fixture_fullscreen_exit(sid).is_ok() {
        leg.note_fixture_fullscreen_exit(FixtureFullscreenExit::Retired);
        return Ok(());
    }
    match fallback_to_baseline(leg, driver, pid, pin, window, before, sid) {
        Ok(()) => {
            leg.note_fixture_fullscreen_exit(FixtureFullscreenExit::Fallback);
            Ok(())
        }
        Err(switch) => {
            // The Space may retire while the refused switch is reported;
            // re-prove that before declaring the state unsafe.
            if leg.await_fixture_fullscreen_exit(sid).is_ok() {
                leg.note_fixture_fullscreen_exit(FixtureFullscreenExit::Retired);
                return Ok(());
            }
            let reason = format!(
                "fullscreen SID {sid} did not retire; pinned baseline switch refused: {switch}"
            );
            leg.note_fixture_fullscreen_exit(FixtureFullscreenExit::Unsafe(reason.clone()));
            Err(reason)
        }
    }
}

fn prove_fullscreen(
    leg: &mut LiveLeg<LiveLegTransport>,
    driver: &FixtureDriver<'_, LiveTransport>,
    pid: u32,
    pin: &DisplayPin,
    before: &TopologySnapshot,
    (a, b): (&(String, u32), &(String, u32)),
    (source, dest): (u64, u64),
) -> Result<serde_json::Value, String> {
    use crate::ffi::{self, SlsSpaceType};
    for row in &before.displays {
        require_user_temp(row.current.ok_or("starting current unreadable")?)
            .map_err(|e| e.to_string())?;
    }
    driver.focus(&a.0, a.1).map_err(|(_, e)| e)?;
    let mut detached = None;
    let result = (|| {
        driver.set_probe_fullscreen(a, true)?;
        let pid_i32 = i32::try_from(pid).map_err(|e| e.to_string())?;
        let entered_at = Instant::now();
        let (fullscreen, full) = loop {
            let snap = snapshot()?;
            if let Some(sid) = snap.displays.get(pin.row).and_then(|row| row.current)
                && !before.displays[pin.row].spaces.contains(&sid)
                && ffi::sls_space_type(sid)? == SlsSpaceType::Fullscreen
                && ffi::sls_spaces_for_window(a.1)? == [sid]
            {
                leg.note_fixture_fullscreen(sid);
                detached = Some(sid);
                if ffi::ax_focused_window_state(pid_i32, a.1) == Ok(Some(true)) {
                    break (sid, snap);
                }
            }
            if entered_at.elapsed() >= DEADLINE {
                return Err("owned fixture never entered a distinct native fullscreen Space".to_owned());
            }
            ffi::pump_runloop_once(25);
        };
        let mut stripped = full.clone();
        stripped.displays[pin.row].spaces.retain(|&sid| sid != fullscreen);
        stripped.displays[pin.row].current = Some(source);
        validate_goto_effect(before, &stripped, pin.row, source).map_err(|e| e.to_string())?;
        if full.locate(fullscreen).map(|(row, _)| row) != Some(pin.row) {
            return Err("fullscreen Space does not uniquely join the pinned display".to_owned());
        }
        let (guard, changes) = ffi::install_display_change_guard(&ffi::MainWakeSignal::detached())?;
        let clock = Instant::now();
        let mut budget = crate::mc_probe::ActionBudget::new(45);
        let mut evidence = Vec::new();
        for (mc, from, to, subject, arrival) in [
            (true, fullscreen, dest, a, b), (true, dest, fullscreen, b, a),
            (false, fullscreen, dest, a, b), (false, dest, fullscreen, b, a),
        ] {
            let stable = || -> Result<(), String> {
                if ffi::sls_spaces_for_window(a.1)? != [fullscreen]
                    || ffi::sls_space_type(fullscreen)? != SlsSpaceType::Fullscreen
                    || ffi::sls_spaces_for_window(b.1)? != [dest]
                {
                    return Err("exact fixture Space membership or fullscreen type changed".to_owned());
                }
                Ok(())
            };
            let check_source = || {
                let bounds = CGDisplay::new(pin.cg_id).bounds();
                if observe(&full, pin)? != from
                    || ffi::cg_display_uuid(pin.cg_id).as_deref() != Some(pin.uuid.as_str())
                    || bounds.origin.x != pin.bounds.origin.x
                    || bounds.origin.y != pin.bounds.origin.y
                    || bounds.size.width != pin.bounds.size.width
                    || bounds.size.height != pin.bounds.size.height
                    || ffi::ax_focused_window_state(pid_i32, subject.1)? != Some(from == fullscreen)
                {
                    return Err("exact fixture fullscreen probe source changed".to_owned());
                }
                stable()?;
                driver.prove_gesture_target(&subject.0, subject.1)
            };
            // The full comparison pins every other display to its previously
            // verified User current, excluding any foreign fullscreen source.
            let mut prior = full.clone();
            prior.displays[pin.row].current = Some(from);
            ffi::pump_runloop_once(1);
            let _ = changes.take_changed();
            let dispatch_started = clock.elapsed();
            if mc {
                let outcome = live::fixture_goto_pinned(
                    to, (pin.row, pin.cg_id, &pin.uuid), &prior, &mut budget, check_source,
                ).map_err(|e| e.to_string())?;
                if outcome.mc_open || outcome.close_error.is_some() {
                    return Err("pinned AX probe lacks exact Mission Control close".to_owned());
                }
            } else {
                let (sign, steps) = crate::space_switch_edges::swipe_plan(&prior.displays[pin.row].spaces, from, to)?;
                let events = prepare_gesture_27(sign, steps)?;
                events.locate(objc2_core_foundation::CGPoint::new(
                    pin.bounds.origin.x + pin.bounds.size.width / 2.0,
                    pin.bounds.origin.y + pin.bounds.size.height / 2.0,
                ));
                events.post_after(check_source)?;
            }
            let (mut notification, mut current, mut foreground) = (None, None, None);
            let wait = Instant::now();
            loop {
                ffi::pump_runloop_once(10);
                if changes.take_changed() { notification.get_or_insert(clock.elapsed()); }
                let sid = observe(&full, pin)?;
                stable()?;
                if sid == to { current.get_or_insert(clock.elapsed()); }
                else if sid != from || current.is_some() { return Err("fullscreen probe landed on the wrong Space or bounced".to_owned()); }
                if crate::focus::read_agreement(pid, arrival.1).is_ok() { foreground.get_or_insert(clock.elapsed()); }
                if current.is_some_and(|first| clock.elapsed() - first >= STABLE)
                    && (foreground.is_some() && notification.is_some() || wait.elapsed() >= DEADLINE)
                { break; }
                if wait.elapsed() >= DEADLINE { return Err("fullscreen probe did not reach exact stable arrival".to_owned()); }
            }
            let row = serde_json::json!({
                "route": if mc { "mc-pinned" } else { "dock-swipe" },
                "away": to == dest, "dispatch_started_us": dispatch_started.as_micros(),
                "notification_sample_us": notification.map(|t| t.as_micros()),
                "current_sample_us": current.map(|t| t.as_micros()),
                "foreground_sample_us": foreground.map(|t| t.as_micros()),
            });
            eprintln!("fixture fullscreen probe {row}");
            evidence.push(row);
        }
        drop(guard);
        Ok(serde_json::json!({"arm":"fullscreen-host","same_fullscreen_sid":fullscreen,"switches":evidence}))
    })();
    match (result, exit_fullscreen(leg, driver, pid, pin, a, before, detached)) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(exit)) => Err(exit),
        (Err(error), Err(exit)) => Err(format!("{error}; fixture fullscreen exit refused: {exit}")),
    }
}

#[allow(clippy::too_many_arguments)]
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
    let driver = FixtureDriver::bind(
        &LiveTransport,
        announcement,
        nonce,
        signer,
        leg.admit(pid, Some(chord))?,
    )?;
    let a = spawn(leg, pid, (socket, nonce), "swipe-a", source, &pin, &driver).map_err(setup)?;
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
    let b = spawn(leg, pid, (socket, nonce), "swipe-b", dest, &pin, &driver).map_err(setup)?;
    goto(leg, row, source).map_err(setup)?;
    let before = snapshot().map_err(setup)?;
    let mut expected = entry;
    expected.displays[row].spaces.extend([source, dest]);
    validate_goto_effect(&expected, &before, row, source).map_err(|e| setup(e.to_string()))?;
    if chord == crate::fixture_leg::SWIPE_27_FULLSCREEN_CHORD {
        return prove_fullscreen(leg, &driver, pid, &pin, &before, (&a, &b), (source, dest))
            .map_err(setup);
    }
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
