#![forbid(unsafe_code)]

//! Development-only detached-fullscreen selection arm: one signed fixture
//! lifetime over two controller-owned Desktops (home `w0`, away `w1`) on the
//! pinned built-in display. Both exist before launch and `w0` never receives
//! a focus write. The subject enters native fullscreen through the fixture's
//! nonce path, the exact surface/member/output registration is published, and
//! three `select-N` posts are measured: redundant home (zero dispatch, exact
//! refusal), home-to-away (one typed fullscreen-to-user dispatch and same-op
//! terminal), and away-to-home from a focused sibling (one typed
//! user-to-fullscreen dispatch, terminal, and the exact native
//! `AXFocusedWindow` route on `w0`). AX on `w0` is read only while its Space
//! is current. Failure after a proved entry exits through the fixture's
//! windowed request and hands teardown `Retired`/`Fallback`/`Unsafe`; a
//! fallback is restoration evidence only and still fails the run, and a
//! request without a proved windowed transition is never reported as
//! success. Recovery Mission Control steps wait for proved product absence.

use std::path::Path;
use std::time::{Duration, Instant};

use core_graphics::display::CGDisplay;
use serde::Serialize;
use serde_json::{Value, json};

use crate::config::ProbeModifier;
use crate::ffi::{self, SlsSpaceType};
use crate::fixture_gate::FixtureDetachedRegistration;
use crate::fixture_leg::{FOCUS_SETTLE_MS, join_fixture_windows, spawn_owned_windows};
use crate::fixture_post::{FixtureDriver, LiveTransport};
use crate::input_policy::InputPreset;
use crate::live_leg::{FixtureFullscreenExit, LiveLeg, LiveLegTransport};
use crate::mc_lifecycle::live;
use crate::mc_probe::{ActionBudget, TopologySnapshot};
use crate::space_probe::NativeSpaces;

const POLL_MS: u64 = 25;
const READY_WAIT_MS: u64 = 15_000;
const ENTRY_WAIT_MS: u64 = 8_000;
const GOTO_WAIT_MS: u64 = 4_000;
/// The product's native owner job runs under a 30 s budget
/// (`select_owner.rs:519`), so the harness terminal bound must cover the
/// whole job plus its publish/settle margin; a shorter bound would record a
/// legal slow leg as failure and race the harness's own recovery.
const TERMINAL_WAIT_MS: u64 = 35_000;
const FOCUS_WAIT_MS: u64 = 3_000;
const CONTEXT_SETTLE_MS: u64 = 750;
const TERMINAL_SETTLE_MS: u64 = 200;
const EXIT_WAIT_MS: u64 = 8_000;
const DISPATCH_PREFIX: &str = "tiler-macos fixture detached-select dispatch ";
const ACTION_PREFIX: &str = "tiler-macos dev select action=";
const DIRECT_FOCUS_ROUTE: &str = "slps-front-direct-focused";

fn pump_for(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        ffi::pump_runloop_once(POLL_MS);
    }
}

fn wait<T>(label: &str, ms: u64, mut read: impl FnMut() -> Result<Option<T>, String>) -> Result<T, String> {
    let end = Instant::now() + Duration::from_millis(ms);
    loop {
        if let Some(value) = read()? { return Ok(value); }
        if Instant::now() >= end { return Err(format!("{label} unproven within {ms}ms")); }
        ffi::pump_runloop_once(POLL_MS);
    }
}

fn snapshot() -> Result<TopologySnapshot, String> { live::fresh_snapshot().map_err(|e| e.to_string()) }

fn cg_bounds(wid: u32) -> Result<ffi::CgBounds, String> {
    ffi::cg_window_list_basic().map_err(|e| format!("CG inventory unreadable ({e})"))?
        .into_iter().find(|row| row.wid == wid).map(|row| row.bounds)
        .ok_or_else(|| format!("CG row for member {wid} is absent"))
}

fn read_stderr(path: &Path) -> Result<String, String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(String::from_utf8_lossy(&bytes).into_owned()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("product stderr unreadable ({e})")),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LegKind { Redundant, Away, Return }

impl LegKind {
    const fn name(self) -> &'static str {
        match self { Self::Redundant => "redundant", Self::Away => "away", Self::Return => "return" }
    }
}

#[derive(Debug, Clone)]
struct Expect { output: String, surface: u64, source: u64, target: u64, pid: u32, wid: u32 }

impl Expect {
    fn new(setup: &Prepared, pid: u32, sid: u64, source: u64, target: u64) -> Self {
        Self { output: crate::topology::output_key(setup.pin.cg_id), surface: sid, source, target, pid, wid: setup.w0.1 }
    }
    fn tail(&self) -> String {
        let kind = |sid: u64| if sid == self.surface { "fullscreen" } else { "user" };
        format!("output={} source={} source_kind={} target={} target_kind={} member={}_{} surface={}",
            self.output, self.source, kind(self.source), self.target, kind(self.target), self.pid, self.wid, self.surface)
    }
    fn reason(&self) -> String {
        format!("redundant detached fullscreen selection: physical current fullscreen surface {} is the frozen destination and member {}_{} is still on it; no dispatch", self.surface, self.pid, self.wid)
    }
    fn classify(&self, kind: LegKind, raw: &str) -> Result<usize, String> {
        let dispatches: Vec<&str> = raw.lines().filter(|line| line.starts_with(DISPATCH_PREFIX)).collect();
        if kind == LegKind::Redundant {
            let suffix = format!(" refused reason={} complete=false", self.reason());
            let mut terminals = raw.lines().filter_map(|line| line.strip_prefix(ACTION_PREFIX)).filter(|rest| {
                !rest.split_once(' ').is_some_and(|(_, phase)|
                    (phase.starts_with("submitted route=") && phase.ends_with(" complete=false"))
                        || (phase.starts_with("observed route=") && phase.rsplit_once(" complete=false observed_ms=")
                            .is_some_and(|(_, ms)| ms.parse::<u64>().is_ok())))
            });
            let valid = match (dispatches.as_slice(), terminals.next(), terminals.next()) {
                ([], Some(line), None) => line.strip_suffix(&suffix)
                    .and_then(|op| op.strip_prefix("op-"))
                    .is_some_and(|id| id.parse::<u64>().is_ok_and(|id| id != 0)),
                _ => false,
            };
            return valid.then_some(0).ok_or_else(|| "redundant leg needs zero dispatch and one exact op refusal".to_owned());
        }
        let [line] = dispatches.as_slice() else { return Err(format!("{} leg dispatched {}, want one", kind.name(), dispatches.len())) };
        let Some((op, tail)) = line.strip_prefix(DISPATCH_PREFIX).and_then(|rest| rest.strip_prefix("op=")).and_then(|rest| rest.split_once(' ')) else {
            return Err("dispatch line is malformed".to_owned());
        };
        if tail != self.tail() { return Err(format!("{} dispatch tail does not match", kind.name())); }
        let applied: Vec<(&str, bool)> = raw.lines().filter_map(applied).collect();
        match applied.as_slice() {
            [(action, true)] if *action == op => Ok(1),
            _ => Err(format!("{} leg lacks one changed applied terminal", kind.name())),
        }
    }
}

fn applied(line: &str) -> Option<(&str, bool)> {
    let rest = line.strip_prefix(ACTION_PREFIX)?;
    let (op, rest) = rest.split_once(' ')?;
    let (changed, rest) = rest.strip_prefix("changed=")?.split_once(' ')?;
    let rest = rest.strip_prefix("caffeinate_reaped=")?.split_once(' ')?.1;
    if rest != "complete=true" { return None; }
    match changed { "true" => Some((op, true)), "false" => Some((op, false)), _ => None }
}

fn readiness_ordinals(line: &str, head: &str) -> Result<Vec<u8>, String> {
    let mut tokens = line.strip_prefix(head).ok_or("readiness fields do not match")?.split(' ');
    let count: usize = tokens.next().and_then(|t| t.strip_prefix("count=")).and_then(|v| v.parse().ok()).ok_or("readiness count malformed")?;
    let ordinals: Vec<u8> = tokens.map(|t| t.strip_prefix("ordinal=").and_then(|v| v.parse().ok()).filter(|o| (1..=9).contains(o)).ok_or("readiness ordinal malformed")).collect::<Result<_, _>>()?;
    (ordinals.len() == count).then_some(ordinals).ok_or_else(|| "readiness count does not match its ordinals".to_owned())
}

fn await_ready(path: &Path, preset: &str, role: &str, home: u32, away: u32) -> Result<Value, String> {
    let head = format!("{}preset={preset} role={role} gate=owned detached=armed ", crate::workspace_select::FIXTURE_SELECT_READINESS_PREFIX);
    wait("product readiness", READY_WAIT_MS, || {
        let raw = read_stderr(path)?;
        let mut lines = raw.lines().filter(|line| line.starts_with(crate::workspace_select::FIXTURE_SELECT_READINESS_PREFIX));
        let Some(line) = lines.next() else { return Ok(None) };
        if lines.next().is_some() { return Err("product emitted more than one readiness line".to_owned()); }
        let ordinals = readiness_ordinals(line, &head)?;
        if !ordinals.contains(&(home as u8)) || !ordinals.contains(&(away as u8)) {
            return Err("readiness ordinals do not prove both owned Desktops".to_owned());
        }
        Ok(Some(json!({"preset": preset, "role": role, "detached": "armed", "ordinals": ordinals})))
    })
}

fn has_field(line: &str, key: &str, value: &str) -> bool { line.split_whitespace().any(|token| token == format!("{key}={value}")) }

/// No frame-set and no applied focus feedback may name the measured member;
/// `setter=not-called` is the fenced no-write evidence.
fn scan_writes(raw: &str, pid: u32, wid: u32) -> Result<usize, String> {
    let target = format!("{pid}_{wid}");
    let (pid, wid) = (pid.to_string(), wid.to_string());
    let mut fenced = 0;
    for line in raw.lines() {
        if line.starts_with("tiler-macos tile frame-set ") && has_field(line, "pid", &pid) && has_field(line, "wid", &wid) {
            return Err("product frame-set line names the measured member".to_owned());
        }
        if line.starts_with("tiler-diag stage=focus-feedback ") && has_field(line, "target", &target) {
            fenced += 1;
            let setter = line.split_whitespace().find_map(|token| token.strip_prefix("setter=")).unwrap_or("missing");
            if setter != "not-called" { return Err(format!("product focus write names the member: setter={setter}")); }
        }
    }
    Ok(fenced)
}

fn other_equal(start: &TopologySnapshot, now: &TopologySnapshot, pinned: usize) -> Result<(), String> {
    if start.displays.len() != now.displays.len() { return Err("display count changed".to_owned()); }
    for (index, (before, after)) in start.displays.iter().zip(&now.displays).enumerate() {
        if index != pinned && (before.identifier != after.identifier || before.spaces != after.spaces || before.current != after.current) {
            return Err(format!("foreign display {index} changed"));
        }
    }
    Ok(())
}

fn row_order(row: &[u64], expected: &[u64], fullscreen: Option<u64>) -> Result<(), String> {
    if let Some(fs) = fullscreen && row.iter().filter(|sid| **sid == fs).count() != 1 {
        return Err(format!("fullscreen surface {fs} is not one row entry"));
    }
    let ordinary: Vec<u64> = row.iter().copied().filter(|sid| Some(*sid) != fullscreen).collect();
    (ordinary == expected).then_some(()).ok_or_else(|| format!("ordinary order {ordinary:?} is not {expected:?}"))
}

fn ordinal(row: &[u64], sid: u64) -> Result<u32, String> {
    let ordinal = row.iter().position(|entry| *entry == sid).map(|index| index as u32 + 1).ok_or_else(|| format!("Space {sid} is absent"))?;
    (1..=9).contains(&ordinal).then_some(ordinal).ok_or_else(|| format!("Space {sid} ordinal {ordinal} is past Desktop-9"))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct AxMember { wid: u32, role: String, subrole: String, fullscreen: bool, frame: (f64, f64, f64, f64) }

impl AxMember {
    fn valid(&self) -> bool { self.frame.0.is_finite() && self.frame.1.is_finite() && self.frame.2 > 0.0 && self.frame.3 > 0.0 }
}

fn retain_element(pid: u32, wid: u32) -> Result<ffi::AxElement, String> {
    let app = ffi::AxElement::application(i32::try_from(pid).map_err(|_| "fixture pid out of range".to_owned())?)
        .ok_or("no AX application element")?;
    let _ = ffi::ax_set_messaging_timeout(&app, 2.0);
    app.windows().map_err(|code| format!("AXWindows unreadable ({code})"))?
        .into_iter().find(|element| ffi::ax_window_id(element) == Some(wid))
        .ok_or_else(|| format!("window {wid} absent from current-Space AXWindows"))
}

fn read_member(element: &ffi::AxElement) -> Result<AxMember, String> {
    let wid = ffi::ax_window_id(element).ok_or("AX element has no window id")?;
    let frame = ffi::ax_get_frame(element).map_err(|code| format!("AX frame unreadable ({code})"))?;
    Ok(AxMember { wid,
        role: ffi::ax_string_attribute(element, "AXRole").ok().flatten().unwrap_or_default(),
        subrole: ffi::ax_string_attribute(element, "AXSubrole").ok().flatten().unwrap_or_default(),
        fullscreen: ffi::ax_fullscreen_state(element).map_err(|code| format!("AXFullScreen unreadable ({code})"))?,
        frame: (frame.x, frame.y, frame.w, frame.h) })
}

/// Enumerate identity only; attribute-read just the `only` windows (empty
/// means all, valid only where every enumerated window is context-safe).
fn read_rows(pid: u32, only: &[u32]) -> Result<Vec<AxMember>, String> {
    let pid = i32::try_from(pid).map_err(|_| "fixture pid out of range".to_owned())?;
    let app = ffi::AxElement::application(pid).ok_or("no AX application element")?;
    let _ = ffi::ax_set_messaging_timeout(&app, 2.0);
    let windows = app.windows().map_err(|code| format!("AXWindows unreadable ({code})"))?;
    Ok(windows.iter().filter_map(|element| {
        let wid = ffi::ax_window_id(element)?;
        if !only.is_empty() && !only.contains(&wid) { return None; }
        let frame = ffi::ax_get_frame(element).ok()?;
        Some(AxMember { wid,
            role: ffi::ax_string_attribute(element, "AXRole").ok().flatten().unwrap_or_default(),
            subrole: ffi::ax_string_attribute(element, "AXSubrole").ok().flatten().unwrap_or_default(),
            fullscreen: ffi::ax_fullscreen_state(element).unwrap_or(false),
            frame: (frame.x, frame.y, frame.w, frame.h) })
    }).collect())
}

fn row_of(rows: &[AxMember], wid: u32) -> Option<&AxMember> { rows.iter().find(|row| row.wid == wid) }

/// Entry evidence: an exact `w0` row when the current-Space query lists it,
/// otherwise the retained element while the same-owner `AXUnknown` proxy
/// stands in for the off-enumeration window.
fn entry_member(retained: &AxMember, rows: &[AxMember], wid: u32) -> Result<(AxMember, bool), String> {
    if retained.wid != wid || !retained.valid() || !retained.fullscreen {
        return Err("retained AX member is not a valid fullscreen reading".to_owned());
    }
    if let Some(row) = row_of(rows, wid) {
        return row.fullscreen.then(|| (row.clone(), false)).ok_or_else(|| "current-Space AX row is not fullscreen".to_owned());
    }
    Ok((retained.clone(), rows.iter().any(|row| row.role == "AXWindow" && row.subrole == "AXUnknown" && row.wid != wid)))
}

/// Positive windowed-exit proof for a retired surface: the exact surface is
/// absent from every row/current, the pinned display is on the exact home
/// Desktop with `w0` its singleton member, the current-Space AX enumeration
/// lists the exact WID, and its `AXFullScreen` read is an actual `Ok(false)`.
/// An unreadable attribute is not a windowed reading. SLS is re-read after
/// the AX proof so a concurrent current/Space change cannot stand in for it.
fn windowed_exit_member(setup: &Prepared, pid: u32, sid: u64) -> Result<Option<AxMember>, String> {
    let before = snapshot()?;
    if before.all_sids().contains(&sid) || check_topology(setup, &before, setup.home, None).is_err() {
        return Ok(None);
    }
    let member = match retain_element(pid, setup.w0.1) {
        Ok(element) => read_member(&element)?,
        Err(_) => return Ok(None),
    };
    if member.wid != setup.w0.1 || member.fullscreen || !member.valid() {
        return Ok(None);
    }
    match check_topology(setup, &snapshot()?, setup.home, None) {
        Ok(()) => Ok(Some(member)),
        Err(_) => Ok(None),
    }
}

fn focus_matches(agreement: &crate::focus::FocusAgreement, pid: u32, wid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else { return false; };
    agreement.workspace_pid == pid && agreement.ax_pid == pid && agreement.ax_wid == Some(wid)
        && agreement.cg_wid_owned && agreement.evidence_route == DIRECT_FOCUS_ROUTE
}

struct Phases { start: Instant, last: u128, entries: Vec<Value> }

impl Phases {
    fn start() -> Self { Self { start: Instant::now(), last: 0, entries: Vec::new() } }
    fn mark(&mut self, name: &str) -> Result<(), String> {
        let at = self.start.elapsed().as_millis();
        if at < self.last { return Err(format!("phase {name} is not monotonic")); }
        self.last = at;
        self.entries.push(json!({"phase": name, "at_ms": at}));
        Ok(())
    }
    fn value(&self) -> Value { Value::Array(self.entries.clone()) }
}

struct DisplayPin { row: usize, cg_id: u32, uuid: String }

impl DisplayPin {
    fn read(entry: &TopologySnapshot) -> Result<Self, String> {
        let display = CGDisplay::main();
        if !display.is_builtin() { return Err("the pinned CG display is not built-in".to_owned()); }
        let uuid = ffi::cg_display_uuid(display.id).ok_or("built-in display UUID unreadable")?;
        let matching: Vec<usize> = entry.displays.iter().enumerate()
            .filter(|(_, row)| row.identifier.as_deref() == Some(uuid.as_str())).map(|(index, _)| index).collect();
        let [row] = matching.as_slice() else { return Err("native rows do not uniquely pin the built-in display".to_owned()); };
        Ok(Self { row: *row, cg_id: display.id, uuid })
    }
}

fn goto(_leg: &mut LiveLeg<LiveLegTransport>, pin: &DisplayPin, target: u64, prior: &TopologySnapshot, member: Option<(u32, u64)>) -> Result<(), String> {
    let source = prior.displays.get(pin.row).and_then(|row| row.current).ok_or("pinned source current unreadable")?;
    let check_source = || {
        let fresh = snapshot()?;
        if fresh.displays.get(pin.row).and_then(|row| row.current) != Some(source) { return Err("pinned source current moved".to_owned()); }
        if let Some((wid, sid)) = member && ffi::sls_spaces_for_window(wid).map_err(|e| e.to_string())?.as_slice() != [sid].as_slice() {
            return Err("pinned source member left its exact Space".to_owned());
        }
        Ok(())
    };
    let mut budget = ActionBudget::new(45);
    let outcome = live::fixture_acceptance_goto_pinned(target, (pin.row, pin.cg_id, pin.uuid.as_str()), prior, &mut budget, check_source).map_err(|e| e.to_string())?;
    if outcome.mc_open || outcome.close_error.is_some() { return Err("pinned switch left Mission Control open".to_owned()); }
    wait("pinned switch", GOTO_WAIT_MS, || {
        let fresh = snapshot()?;
        let settled = fresh.displays.get(pin.row).and_then(|row| row.current) == Some(target)
            && crate::mc_probe::validate_goto_effect(prior, &fresh, pin.row, target).is_ok();
        Ok(settled.then_some(()))
    })
}

fn await_focus(pid: u32, wid: u32, activate: bool) -> Result<crate::focus::FocusAgreement, String> {
    let mut activated = !activate;
    wait("natural member focus", FOCUS_WAIT_MS, || {
        match crate::focus::read_agreement(pid, wid) {
            Ok(agreement) if focus_matches(&agreement, pid, wid) => Ok(Some(agreement)),
            Ok(agreement) => Err(format!("focus route {} is not the exact AXFocusedWindow route", agreement.evidence_route)),
            Err(_) => {
                if !activated { activated = true; if let Ok(pid) = i32::try_from(pid) { let _ = ffi::activate_app(pid); } }
                Ok(None)
            }
        }
    })
}

fn check_topology(setup: &Prepared, now: &TopologySnapshot, current: u64, fullscreen: Option<u64>) -> Result<(), String> {
    other_equal(&setup.other, now, setup.pin.row)?;
    let row = now.displays.get(setup.pin.row).ok_or("pinned display absent")?;
    if row.current != Some(current) { return Err(format!("pinned current is {:?}, want {current}", row.current)); }
    row_order(&row.spaces, &setup.entry_row, fullscreen)?;
    if let Some(sid) = fullscreen && ffi::sls_space_type(sid).map_err(|e| e.to_string())? != SlsSpaceType::Fullscreen {
        return Err(format!("surface {sid} is not typed fullscreen"));
    }
    let membership = |wid, space| -> Result<bool, String> {
        Ok(ffi::sls_spaces_for_window(wid).map_err(|e| e.to_string())?.as_slice() == [space].as_slice())
    };
    let w0_space = fullscreen.unwrap_or(setup.home);
    if !membership(setup.w0.1, w0_space)? { return Err(format!("subject is not the singleton member of {w0_space}")); }
    if !membership(setup.w1.1, setup.away)? { return Err("sibling is not the singleton member of away".to_owned()); }
    Ok(())
}

fn bracket_focus(setup: &Prepared, pid: u32, wid: u32, current: u64, fullscreen: Option<u64>, activate: bool) -> Result<Value, String> {
    check_topology(setup, &snapshot()?, current, fullscreen)?;
    let agreement = await_focus(pid, wid, activate)?;
    check_topology(setup, &snapshot()?, current, fullscreen)?;
    Ok(json!({"ax_pid": agreement.ax_pid, "ax_wid": agreement.ax_wid, "cg_owned": agreement.cg_wid_owned, "route": agreement.evidence_route}))
}

pub(crate) struct Prepared {
    pin: DisplayPin, home: u64, away: u64, home_ordinal: u32, away_ordinal: u32,
    w0: (String, u32), w1: (String, u32), retained: ffi::AxElement, w1_retained: ffi::AxElement,
    entry_row: Vec<u64>, other: TopologySnapshot, starting: u64,
}

impl Prepared {
    pub(crate) fn owned_sids(&self) -> Vec<u64> { vec![self.home, self.away] }
    pub(crate) const fn starting_sid(&self) -> u64 { self.starting }
    pub(crate) fn select_chord(&self) -> String { format!("select-{}", self.home_ordinal) }
}

fn create_owned(leg: &mut LiveLeg<LiveLegTransport>, display: usize, before: &[u64]) -> Result<u64, String> {
    let created = leg.transport_mut().space_create(display).map_err(|e| format!("owned Desktop creation refused ({e})"))?;
    leg.note_owned_space(display, created.sid);
    if let Some(shortfall) = created.mc.shortfall_text("detached-away owned Desktop create") { return Err(shortfall); }
    leg.transport_mut().space_require_empty(created.sid).map_err(|e| format!("new owned Desktop is not empty ({e})"))?;
    let after = snapshot()?;
    let row = &after.displays.get(display).ok_or("pinned display absent after owned creation")?.spaces;
    let ordinary: Vec<u64> = row.iter().copied().filter(|sid| *sid != created.sid).collect();
    if row.iter().filter(|sid| **sid == created.sid).count() != 1 || ordinary != before {
        return Err("owned Desktop was not exactly one ordered append".to_owned());
    }
    Ok(created.sid)
}

fn spawn_single(leg: &mut LiveLeg<LiveLegTransport>, pid: u32, socket: &str, nonce: &str, caller: &str, sid: u64) -> Result<(String, u32), String> {
    let callers = [caller.to_owned()];
    spawn_owned_windows(socket, nonce, &callers)?;
    let joined = join_fixture_windows(pid, socket, nonce, &callers)?;
    let [(joined_caller, wid)] = joined.as_slice() else { return Err(format!("fixture window {caller} did not join uniquely")); };
    leg.note_owned_window(joined_caller, pid, *wid);
    wait("fixture window placement", ENTRY_WAIT_MS, || {
        Ok((ffi::sls_spaces_for_window(*wid).map_err(|e| e.to_string())?.as_slice() == [sid].as_slice()).then_some(()))
    })?;
    Ok((joined_caller.clone(), *wid))
}

pub(crate) fn prepare(leg: &mut LiveLeg<LiveLegTransport>, pid: u32, socket: &str, nonce: &str, announcement: &str, signer: &str) -> Result<Prepared, (i32, String)> {
    let fail = |stage: &str, error: String| (1, format!("detached-away {stage}: {error}"));
    let snap = |stage: &str| snapshot().map_err(|e| fail(stage, e));
    let pin = DisplayPin::read(&snap("pin")?).map_err(|e| fail("pin", e))?;
    leg.prove_return_on(pin.row).map_err(|(code, error)| (code, format!("detached-away return probe: {error}")))?;
    let start = snap("start")?;
    let start_row = start.displays.get(pin.row).ok_or_else(|| fail("start", "pinned display absent".to_owned()))?;
    let starting = start_row.current.ok_or_else(|| fail("start", "starting current unreadable".to_owned()))?;
    if start_row.spaces.len() + 2 > 9 {
        return Err(fail("start", format!("{} user Desktops leave no room for two owned Desktops", start_row.spaces.len())));
    }
    let home = create_owned(leg, pin.row, &start_row.spaces).map_err(|e| fail("home", e))?;
    let away = create_owned(leg, pin.row, &snap("away")?.displays[pin.row].spaces).map_err(|e| fail("away", e))?;
    goto(leg, &pin, home, &snap("home-goto")?, None).map_err(|e| fail("home-goto", e))?;
    let w0 = spawn_single(leg, pid, socket, nonce, "w0", home).map_err(|e| fail("w0", e))?;
    goto(leg, &pin, away, &snap("away-goto")?, Some((w0.1, home))).map_err(|e| fail("away-goto", e))?;
    let w1 = spawn_single(leg, pid, socket, nonce, "w1", away).map_err(|e| fail("w1", e))?;
    {
        let admission = leg.admit(pid, None).map_err(|(_, error)| fail("w1-focus", error))?;
        FixtureDriver::bind(&LiveTransport, announcement, nonce, signer, admission).map_err(|e| fail("w1-focus", e.1))?
            .focus(&w1.0, w1.1).map_err(|(_, error)| fail("w1-focus", error))?;
        crate::fixture_leg::settle_hold(FOCUS_SETTLE_MS).map_err(|(_, error)| fail("w1-focus", error))?;
    }
    let w1_retained = retain_element(pid, w1.1).map_err(|e| fail("w1-retain", e))?;
    goto(leg, &pin, home, &snap("home-return")?, Some((w1.1, away))).map_err(|e| fail("home-return", e))?;
    await_focus(pid, w0.1, true).map_err(|e| fail("home-focus", e))?;
    let pre_entry = snap("pre-entry")?;
    let entry_row = pre_entry.displays.get(pin.row).ok_or_else(|| fail("pre-entry", "pinned display absent".to_owned()))?.spaces.clone();
    let mut expected = start_row.spaces.clone();
    expected.extend([home, away]);
    if entry_row != expected { return Err(fail("pre-entry", format!("owned Desktops are not the exact ordered append {entry_row:?} != {expected:?}"))); }
    let home_ordinal = ordinal(&entry_row, home).map_err(|e| fail("pre-entry", e))?;
    let away_ordinal = ordinal(&entry_row, away).map_err(|e| fail("pre-entry", e))?;
    let retained = retain_element(pid, w0.1).map_err(|e| fail("retain", e))?;
    if read_member(&retained).map_err(|e| fail("retain", e))?.fullscreen {
        return Err(fail("retain", "subject is already fullscreen while home".to_owned()));
    }
    other_equal(&start, &pre_entry, pin.row).map_err(|e| fail("pre-entry", e))?;
    Ok(Prepared { pin, home, away, home_ordinal, away_ordinal, w0, w1, retained, w1_retained, entry_row, other: start, starting })
}

fn await_entry(leg: &mut LiveLeg<LiveLegTransport>, pid: u32, setup: &Prepared, before: &TopologySnapshot) -> Result<u64, String> {
    let pre = before.displays.get(setup.pin.row).ok_or("pre-entry snapshot lacks the pinned display")?;
    wait("fullscreen entry", ENTRY_WAIT_MS, || {
        let now = snapshot()?;
        let row = now.displays.get(setup.pin.row).ok_or("pinned display absent from entry snapshot")?;
        let added: Vec<u64> = row.spaces.iter().copied().filter(|sid| !pre.spaces.contains(sid)).collect();
        let [sid] = added.as_slice() else { return Ok(None) };
        let typed = ffi::sls_space_type(*sid).map_err(|e| e.to_string())? == SlsSpaceType::Fullscreen;
        let joined = ffi::sls_spaces_for_window(setup.w0.1).map_err(|e| e.to_string())?.as_slice() == [*sid].as_slice();
        let cg = ffi::cg_window_list_basic().map_err(|e| e.to_string())?.iter().any(|window| window.wid == setup.w0.1 && window.pid == pid);
        if !(row.current == Some(*sid) && typed && joined && cg) { return Ok(None); }
        other_equal(&setup.other, &now, setup.pin.row)?;
        row_order(&row.spaces, &setup.entry_row, Some(*sid))?;
        leg.note_fixture_fullscreen(*sid);
        Ok(Some(*sid))
    })
}

fn await_segment(path: &Path, offset: &mut u64, kind: LegKind, expect: &Expect) -> Result<(usize, String), String> {
    let end = Instant::now() + Duration::from_millis(TERMINAL_WAIT_MS);
    loop {
        let raw = read_stderr(path)?;
        if (raw.len() as u64) < *offset { return Err("product stderr shrank".to_owned()); }
        let segment = &raw[*offset as usize..];
        let complete = if kind == LegKind::Redundant {
            segment.lines().any(|line| line.starts_with(ACTION_PREFIX) && line.contains(" refused reason=") && line.ends_with(" complete=false"))
        } else {
            let op = segment.lines().find_map(|line| line.strip_prefix(DISPATCH_PREFIX)).and_then(|rest| rest.strip_prefix("op=")).and_then(|rest| rest.split_once(' ').map(|(op, _)| op));
            op.is_some_and(|op| segment.lines().any(|line| line.starts_with(&format!("{ACTION_PREFIX}{op} changed=")) && line.ends_with(" complete=true")))
        };
        if complete {
            pump_for(TERMINAL_SETTLE_MS);
            let settled = read_stderr(path)?;
            if (settled.len() as u64) < *offset { return Err("product stderr shrank".to_owned()); }
            let segment = settled[*offset as usize..].to_owned();
            let dispatches = expect.classify(kind, &segment)?;
            *offset = settled.len() as u64;
            return Ok((dispatches, segment));
        }
        if Instant::now() >= end { return Err(format!("{} leg terminal absent", kind.name())); }
        ffi::pump_runloop_once(50);
    }
}

/// One controller recovery Mission-Control step is allowed only after the
/// exact product absence is proved. The gate stops the product once and
/// caches that proof; a refused proof refuses every later recovery step
/// without retrying the stop, so no MC is mutated on an unproved absence.
struct RecoveryGate { state: Option<Result<(), String>> }

impl RecoveryGate {
    const fn new() -> Self { Self { state: None } }
    /// Run the quiesce proof at most once and report whether it succeeded.
    fn attempt(&mut self, quiesce: impl FnOnce() -> Result<(), String>) -> bool {
        self.state.get_or_insert_with(quiesce).is_ok()
    }
    fn ensure(&mut self, leg: &mut LiveLeg<LiveLegTransport>) -> Result<(), String> {
        self.attempt(|| leg.quiesce_detached_product());
        match &self.state {
            Some(Ok(())) => Ok(()),
            Some(Err(error)) => Err(format!("recovery MC refused: {error}")),
            None => Err("recovery MC refused: product absence unproved".to_owned()),
        }
    }
    fn evidence(&self) -> Value {
        match &self.state {
            None => json!({"required": false}),
            Some(Ok(())) => json!({"required": true, "proved": true}),
            Some(Err(error)) => json!({"required": true, "proved": false, "error": error}),
        }
    }
}

/// The only acceptance-success exit is a proved windowed retirement. A
/// fallback is restoration evidence only and fails the run even when every
/// measured leg passed.
fn acceptance_exit(outcome: &str, lingering: bool) -> Result<(), String> {
    if lingering || outcome != "retired" {
        return Err(format!(
            "windowed exit outcome '{outcome}' (lingering={lingering}) is not a proved retirement; detached acceptance not proved"
        ));
    }
    Ok(())
}

struct Leg { kind: LegKind, chord: String, caller: &'static str, wid: u32, source: u64, target: u64 }

struct MeasureRun<'a, 'd> {
    driver: &'a FixtureDriver<'d, LiveTransport>,
    pid: u32,
    setup: &'a Prepared,
    stderr: &'a Path,
    offset: &'a mut u64,
    phases: &'a mut Phases,
    modifier: ProbeModifier,
    sid: u64,
    entry_bounds: ffi::CgBounds,
}

fn measure_leg(run: &mut MeasureRun<'_, '_>, leg: &Leg) -> Result<Value, String> {
    let expect = Expect::new(run.setup, run.pid, run.sid, leg.source, leg.target);
    run.phases.mark(&format!("{}-start", leg.kind.name()))?;
    check_topology(run.setup, &snapshot()?, leg.source, Some(run.sid))?;
    if leg.kind == LegKind::Return {
        let sibling = crate::focus::read_agreement(run.pid, run.setup.w1.1);
        if !sibling.is_ok_and(|agreement| focus_matches(&agreement, run.pid, run.setup.w1.1)) {
            run.driver.focus(&run.setup.w1.0, run.setup.w1.1).map_err(|(_, error)| format!("away sibling focus refused ({error})"))?;
            crate::fixture_leg::settle_hold(FOCUS_SETTLE_MS).map_err(|(_, error)| format!("away sibling focus settle: {error}"))?;
        }
        bracket_focus(run.setup, run.pid, run.setup.w1.1, leg.source, Some(run.sid), false)?;
    } else {
        bracket_focus(run.setup, run.pid, run.setup.w0.1, leg.source, Some(run.sid), false)?;
        wait("retained fullscreen member", ENTRY_WAIT_MS, || {
            let member = read_member(&run.setup.retained)?;
            Ok((member.fullscreen && member.valid()).then_some(()))
        })?;
    }
    if leg.kind != LegKind::Return && cg_bounds(run.setup.w0.1)? != run.entry_bounds {
        return Err(format!("{}: fullscreen bounds changed before the post", leg.kind.name()));
    }
    *run.offset = read_stderr(run.stderr)?.len() as u64;
    run.driver.post(leg.caller, leg.wid, &leg.chord, run.modifier).map_err(|(_, error)| format!("{} post refused: {error}", leg.kind.name()))?;
    let (dispatches, segment) = await_segment(run.stderr, run.offset, leg.kind, &expect)?;
    check_topology(run.setup, &snapshot()?, leg.target, Some(run.sid))?;
    if cg_bounds(run.setup.w0.1)? != run.entry_bounds { return Err(format!("{}: fullscreen bounds changed after the post", leg.kind.name())); }
    let ax = match leg.kind {
        LegKind::Redundant => {
            if !read_member(&run.setup.retained)?.fullscreen { return Err("redundant refusal left fullscreen".to_owned()); }
            json!({"fullscreen": true})
        }
        LegKind::Away => {
            let row = read_member(&run.setup.w1_retained)?;
            if row.wid != run.setup.w1.1 || row.fullscreen { return Err("away sibling is not the exact windowed member".to_owned()); }
            json!({"sibling": row})
        }
        LegKind::Return => {
            let rows = read_rows(run.pid, &[run.setup.w0.1])?;
            let row = row_of(&rows, run.setup.w0.1).ok_or("return AXWindows does not list the member")?;
            if !row.fullscreen { return Err("returned member AXFullScreen is not true".to_owned()); }
            bracket_focus(run.setup, run.pid, run.setup.w0.1, leg.target, Some(run.sid), false)?;
            json!({"wid": row.wid, "role": row.role, "subrole": row.subrole, "fullscreen": row.fullscreen, "frame": [row.frame.0, row.frame.1, row.frame.2, row.frame.3]})
        }
    };
    check_topology(run.setup, &snapshot()?, leg.target, Some(run.sid))?;
    if cg_bounds(run.setup.w0.1)? != run.entry_bounds { return Err(format!("{}: bounds moved after AX reads", leg.kind.name())); }
    run.phases.mark(&format!("{}-end", leg.kind.name()))?;
    Ok(json!({"leg": leg.kind.name(), "chord": leg.chord, "dispatch": dispatches, "segment_bytes": segment.len(), "ax": ax}))
}

fn prove_baseline(leg: &mut LiveLeg<LiveLegTransport>, setup: &Prepared, pid: u32, sid: u64, recovery: &mut RecoveryGate) -> Result<(), String> {
    let now = snapshot()?;
    other_equal(&setup.other, &now, setup.pin.row)?;
    let current = now.displays.get(setup.pin.row).and_then(|row| row.current);
    if current != Some(setup.starting) {
        let member = match current {
            Some(s) if s == sid => {
                if !crate::space_synthetic_gate::detached_surface_exact(sid, pid, setup.w0.1)? {
                    return Err("baseline fallback: current fullscreen owner is unproved".to_owned());
                }
                Some((setup.w0.1, sid))
            }
            Some(s) if s == setup.away => Some((setup.w1.1, setup.away)),
            Some(s) if s == setup.home => Some((setup.w0.1, setup.home)),
            _ => return Err(format!("baseline fallback: current {current:?} is neither the surface nor an owned Desktop")),
        };
        recovery.ensure(leg)?;
        goto(leg, &setup.pin, setup.starting, &now, member)?;
    }
    let after = snapshot()?;
    if after.displays.get(setup.pin.row).and_then(|row| row.current) != Some(setup.starting)
        || after.displays.iter().zip(&setup.other.displays).any(|(now, base)| now.current != base.current) {
        return Err("baseline fallback: starting Desktop or a display current did not match".to_owned());
    }
    Ok(())
}

fn exit_fallback(leg: &mut LiveLeg<LiveLegTransport>, setup: &Prepared, pid: u32, sid: u64, recovery: &mut RecoveryGate) -> Result<(&'static str, bool), String> {
    let proved_windowed = || match windowed_exit_member(setup, pid, sid) {
        Ok(Some(_)) => Ok(true),
        Ok(None) => Ok(false),
        Err(error) => Err(error),
    };
    if !snapshot()?.all_sids().contains(&sid) {
        return match proved_windowed() {
            Ok(true) => {
                leg.note_fixture_fullscreen_exit(FixtureFullscreenExit::Retired);
                Ok(("retired", false))
            }
            Ok(false) | Err(_) => {
                let error = "surface retired without a proved windowed member".to_owned();
                leg.note_fixture_fullscreen_exit(FixtureFullscreenExit::Unsafe(error.clone()));
                Err(error)
            }
        };
    }
    if let Err(error) = prove_baseline(leg, setup, pid, sid, recovery) {
        leg.note_fixture_fullscreen_exit(FixtureFullscreenExit::Unsafe(error.clone()));
        return Err(error);
    }
    let outcome = if !snapshot()?.all_sids().contains(&sid) {
        match proved_windowed() {
            Ok(true) => ("retired", false),
            Ok(false) | Err(_) => {
                let error = "surface retired without a proved windowed member".to_owned();
                leg.note_fixture_fullscreen_exit(FixtureFullscreenExit::Unsafe(error.clone()));
                return Err(error);
            }
        }
    } else {
        ("fallback", true)
    };
    leg.note_fixture_fullscreen_exit(if outcome.0 == "retired" { FixtureFullscreenExit::Retired } else { FixtureFullscreenExit::Fallback });
    Ok(outcome)
}

/// Prove the exact surface is current (returning to it when the measured legs
/// left an owned Desktop current) after the product is quiesced, then request
/// windowed mode. Retirement requires the positive windowed-exit proof; a
/// continued surface is only Fallback after the exact starting Desktop and
/// every display current are proved. Anything else is Unsafe and holds
/// teardown. No recovery Mission Control step runs without proved product
/// absence.
fn exit_detached(leg: &mut LiveLeg<LiveLegTransport>, driver: &FixtureDriver<'_, LiveTransport>, setup: &Prepared, pid: u32, sid: u64, recovery: &mut RecoveryGate) -> Result<(&'static str, bool), String> {
    let now = snapshot()?;
    if !now.all_sids().contains(&sid) {
        return match wait("windowed exit", EXIT_WAIT_MS, || windowed_exit_member(setup, pid, sid)) {
            Ok(_) => {
                leg.note_fixture_fullscreen_exit(FixtureFullscreenExit::Retired);
                Ok(("retired", false))
            }
            Err(error) => {
                let note = format!("surface retired without a proved windowed member: {error}");
                leg.note_fixture_fullscreen_exit(FixtureFullscreenExit::Unsafe(note.clone()));
                Err(note)
            }
        };
    }
    let current = now.displays.get(setup.pin.row).and_then(|row| row.current);
    let w0_on_sid = || -> Result<bool, String> {
        Ok(ffi::sls_spaces_for_window(setup.w0.1).map_err(|e| e.to_string())?.as_slice() == [sid].as_slice())
    };
    let mut exact = current == Some(sid) && w0_on_sid()?;
    if !exact {
        let member = if current == Some(setup.away) { Some((setup.w1.1, setup.away)) }
            else if current == Some(setup.home) { Some((setup.w0.1, setup.home)) } else { None };
        let Some(member) = member else { return exit_fallback(leg, setup, pid, sid, recovery); };
        if recovery.ensure(leg).is_ok() && goto(leg, &setup.pin, sid, &now, Some(member)).is_ok() {
            exact = snapshot()?.displays.get(setup.pin.row).and_then(|row| row.current) == Some(sid) && w0_on_sid()?;
        }
    }
    if !exact { return exit_fallback(leg, setup, pid, sid, recovery); }
    if let Err(error) = driver.set_probe_fullscreen(&setup.w0, false) { eprintln!("detached-away windowed request refused: {error}"); }
    let retired = wait("windowed exit", EXIT_WAIT_MS, || windowed_exit_member(setup, pid, sid));
    if retired.is_ok() {
        leg.note_fixture_fullscreen_exit(FixtureFullscreenExit::Retired);
        return Ok(("retired", false));
    }
    if !snapshot()?.all_sids().contains(&sid) {
        let error = format!("surface retired without a proved windowed member: {}", retired.err().unwrap_or_default());
        leg.note_fixture_fullscreen_exit(FixtureFullscreenExit::Unsafe(error.clone()));
        return Err(error);
    }
    exit_fallback(leg, setup, pid, sid, recovery)
}

#[allow(clippy::too_many_lines)]
pub(crate) fn run(leg: &mut LiveLeg<LiveLegTransport>, driver: &FixtureDriver<'_, LiveTransport>, pid: u32,
    setup: &Prepared, product_dir: &Path, modifier: ProbeModifier, preset: InputPreset) -> Result<Value, (i32, String)> {
    let stderr = product_dir.join("stderr.log");
    let mut phases = Phases::start();
    let mut report = json!({
        "arm": "detached-away", "fixture_pid": pid, "home": setup.home, "away": setup.away,
        "home_ordinal": setup.home_ordinal, "away_ordinal": setup.away_ordinal, "starting_sid": setup.starting,
        "output": crate::topology::output_key(setup.pin.cg_id), "readiness_verified": false,
        "entry": Value::Null, "legs": [], "exit": Value::Null,
        "windows": {"w0": {"caller": setup.w0.0, "wid": setup.w0.1}, "w1": {"caller": setup.w1.0, "wid": setup.w1.1}},
    });
    let mut sid = None;
    let mut legs = Vec::new();
    let (mut audit_start, mut offset) = (0u64, 0u64);
    let measured = (|| -> Result<(), String> {
        report["readiness"] = await_ready(&stderr, preset.as_str(), modifier.as_str(), setup.home_ordinal, setup.away_ordinal)?;
        report["readiness_verified"] = json!(true);
        phases.mark("product-ready")?;
        check_topology(setup, &snapshot()?, setup.home, None)?;
        report["home_focus"] = bracket_focus(setup, pid, setup.w0.1, setup.home, None, true)?;
        phases.mark("home-observed")?;
        let before_entry = snapshot()?;
        audit_start = read_stderr(&stderr)?.len() as u64;
        // Mark Unsafe before the request is emitted: a request error or an
        // entry timeout has no exit proof, so teardown must hold every
        // destructive fixture step. No off-current AX read is used as a
        // recovery probe.
        leg.note_fullscreen_entry_requested();
        if let Err(error) = driver.set_probe_fullscreen(&setup.w0, true) {
            let note = format!("fullscreen entry request refused ({error})");
            leg.note_fixture_fullscreen_exit(FixtureFullscreenExit::Unsafe(note.clone()));
            return Err(note);
        }
        phases.mark("fullscreen-request")?;
        let surface = match await_entry(leg, pid, setup, &before_entry) {
            Ok(surface) => surface,
            Err(error) => {
                let note = format!("fullscreen entry: {error}");
                leg.note_fixture_fullscreen_exit(FixtureFullscreenExit::Unsafe(note.clone()));
                return Err(note);
            }
        };
        sid = Some(surface);
        phases.mark("fullscreen-entry")?;
        FixtureDetachedRegistration { surface, member: setup.w0.1, pid, output: crate::topology::output_key(setup.pin.cg_id) }
            .publish(product_dir).map_err(|error| format!("detached registration publish refused ({error})"))?;
        phases.mark("registration")?;
        check_topology(setup, &snapshot()?, surface, Some(surface))?;
        let member = wait("retained fullscreen", ENTRY_WAIT_MS, || {
            let member = read_member(&setup.retained)?;
            Ok((member.fullscreen && member.valid()).then_some(member))
        })?;
        let (entry_ax, proxy) = entry_member(&member, &read_rows(pid, &[])?, setup.w0.1)?;
        let entry_bounds = cg_bounds(setup.w0.1)?;
        check_topology(setup, &snapshot()?, surface, Some(surface))?;
        if cg_bounds(setup.w0.1)? != entry_bounds { return Err("entry: fullscreen bounds moved after AX reads".to_owned()); }
        report["entry_focus"] = bracket_focus(setup, pid, setup.w0.1, surface, Some(surface), false)?;
        report["entry"] = json!({"surface": surface, "ax": entry_ax, "proxy_ignored": proxy, "cg_bounds": entry_bounds});
        pump_for(CONTEXT_SETTLE_MS);
        phases.mark("product-context")?;
        let home_chord = format!("select-{}", setup.home_ordinal);
        let away_chord = format!("select-{}", setup.away_ordinal);
        {
            let mut measure = MeasureRun {
                driver, pid, setup, stderr: &stderr, offset: &mut offset, phases: &mut phases,
                modifier, sid: surface, entry_bounds,
            };
            for leg in [
                Leg { kind: LegKind::Redundant, chord: home_chord.clone(), caller: "w0", wid: setup.w0.1, source: surface, target: surface },
                Leg { kind: LegKind::Away, chord: away_chord, caller: "w0", wid: setup.w0.1, source: surface, target: setup.away },
                Leg { kind: LegKind::Return, chord: home_chord, caller: "w1", wid: setup.w1.1, source: setup.away, target: surface },
            ] {
                legs.push(measure_leg(&mut measure, &leg)?);
                report["legs"] = json!(legs.clone());
            }
        }
        let full = read_stderr(&stderr)?;
        if (full.len() as u64) < audit_start || (full.len() as u64) < offset { return Err("product stderr shrank".to_owned()); }
        let fenced = scan_writes(&full[audit_start as usize..offset as usize], pid, setup.w0.1)?;
        report["no_member_writes"] = json!({"proved": true, "fenced_focus_lines": fenced, "span_start": audit_start, "span_end": offset});
        Ok(())
    })();
    let mut recovery = RecoveryGate::new();
    macro_rules! refuse {
        ($value:expr) => {{
            report["product_absence"] = recovery.evidence();
            report["error"] = json!($value);
            report["phases"] = phases.value();
            return Err((1, report.to_string()));
        }};
    }
    if let Err(error) = measured {
        report["error"] = json!(error);
        // Keep an explicit stop boundary before any recovery effect so the
        // failed leg's elapsed evidence ends at the stop, not after recovery.
        if let Err(phase_error) = phases.mark("stop-boundary") {
            report["phase_error"] = json!(phase_error);
        }
        // Quiesce the exact product before any controller recovery Mission
        // Control step. A failed absence proof is cached and refuses every
        // later recovery step; the exit may still request windowed mode when
        // no MC is needed.
        let _ = recovery.attempt(|| leg.quiesce_detached_product());
        let exit = match sid { Some(surface) => exit_detached(leg, driver, setup, pid, surface, &mut recovery), None => Ok(("none", false)) };
        report["product_absence"] = recovery.evidence();
        if let Ok((outcome, lingering)) = exit { report["exit"] = json!({"outcome": outcome, "sid_lingering": lingering}); }
        report["phases"] = phases.value();
        return Err((1, report.to_string()));
    }
    let exit = match sid { Some(surface) => exit_detached(leg, driver, setup, pid, surface, &mut recovery), None => Ok(("none", false)) };
    report["product_absence"] = recovery.evidence();
    let (outcome, lingering) = match exit { Ok(value) => value, Err(error) => refuse!(error) };
    report["exit"] = json!({"outcome": outcome, "sid_lingering": lingering});
    if let Err(error) = acceptance_exit(outcome, lingering) { refuse!(error); }
    phases.mark("exit").map_err(|error| (1, error))?;
    let surface = sid.ok_or_else(|| (1, "detached-away lost its surface".to_owned()))?;
    let after = snapshot().map_err(|error| (1, error))?;
    if let Err(error) = other_equal(&setup.other, &after, setup.pin.row) { refuse!(error); }
    let row = after.displays.get(setup.pin.row).ok_or((1, "pinned row absent".to_owned()))?;
    if row.current != Some(setup.home) { refuse!("post-exit home current unproved"); }
    if ffi::sls_spaces_for_window(setup.w0.1).map_err(|e| (1, e.to_string()))?.as_slice() != [setup.home].as_slice() {
        refuse!("post-exit subject membership is not the home Desktop");
    }
    if after.all_sids().contains(&surface) { refuse!("fullscreen surface survived its retirement proof"); }
    if let Err(error) = row_order(&row.spaces, &setup.entry_row, None) { refuse!(error); }
    let windowed = windowed_exit_member(setup, pid, surface).map_err(|e| (1, e))?;
    let Some(windowed) = windowed else { refuse!("post-exit windowed AX proof failed") };
    report["post_exit"] = json!({"ax_fullscreen": false, "role": windowed.role, "subrole": windowed.subrole});
    report["phases"] = phases.value();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mc_probe::{DisplaySpaces, TopologySnapshot};

    fn expect(kind: LegKind) -> Expect {
        Expect { output: "macos-display-1".into(), surface: 320,
            source: if kind == LegKind::Return { 319 } else { 320 },
            target: if kind == LegKind::Away { 319 } else { 320 }, pid: 12168, wid: 10747 }
    }
    fn dispatch(expect: &Expect, op: &str) -> String { format!("{DISPATCH_PREFIX}op={op} {}", expect.tail()) }
    fn applied(op: &str) -> String { format!("{ACTION_PREFIX}{op} changed=true caffeinate_reaped=false complete=true") }
    fn snapshot(rows: &[(&[u64], u64)]) -> TopologySnapshot {
        TopologySnapshot { displays: rows.iter().enumerate().map(|(index, (spaces, current))| DisplaySpaces {
            identifier: Some(format!("display-{index}")), spaces: spaces.to_vec(), current: Some(*current) }).collect() }
    }

    #[test]
    fn capture3_replay_pins_readiness_dispatch_terminals_and_relative_rows() {
        let head = format!("{}preset=authentic role=ctrl gate=owned detached=armed ", crate::workspace_select::FIXTURE_SELECT_READINESS_PREFIX);
        let ready = format!("{head}count=2 ordinal=3 ordinal=4");
        assert_eq!(readiness_ordinals(&ready, &head), Ok(vec![3, 4]));
        assert!(readiness_ordinals(&ready.replace("count=2", "count=3"), &head).is_err());
        assert!(readiness_ordinals(&ready.replace("ordinal=4", "ordinal=0"), &head).is_err());
        assert!(readiness_ordinals(&ready.replace("gate=owned", "gate=none"), &head).is_err());
        let away = expect(LegKind::Away);
        let segment = format!("{}\n{}", dispatch(&away, "select-4"), applied("select-4"));
        assert_eq!(away.classify(LegKind::Away, &segment), Ok(1));
        assert!(away.classify(LegKind::Away, &segment.replace("changed=true", "changed=false")).is_err());
        assert!(away.classify(LegKind::Away, &dispatch(&away, "select-4")).is_err());
        let back = expect(LegKind::Return);
        assert_eq!(back.classify(LegKind::Return, &format!("{}\n{}", dispatch(&back, "select-3"), applied("select-3"))), Ok(1));
        let entry = [1, 317, 319];
        row_order(&[1, 317, 320, 319], &entry, Some(320)).unwrap();
        assert!(row_order(&[1, 319, 317, 320], &entry, Some(320)).is_err());
        assert_eq!(ordinal(&[1, 317, 320, 319], 320), Ok(3));
    }

    #[test]
    fn redundant_refusal_is_exact_and_dispatches_nothing() {
        let expect = Expect { surface: 337, source: 337, target: 337, pid: 32796, wid: 11071, ..expect(LegKind::Redundant) };
        let refusal = format!("{ACTION_PREFIX}op-1 refused reason={} complete=false", expect.reason());
        let segment = format!("{ACTION_PREFIX}workspace-select-2 submitted route=dock-swipe-pinned output=macos-display-1 complete=false\n{ACTION_PREFIX}op-1 observed route=dock-swipe complete=false observed_ms=297\n{refusal}");
        assert_eq!(expect.classify(LegKind::Redundant, &segment), Ok(0));
        assert!(expect.classify(LegKind::Redundant, &refusal.replace("op-1", "select-3")).is_err());
        assert!(expect.classify(LegKind::Redundant, &refusal.replace("op-1", "op-0")).is_err());
        assert!(expect.classify(LegKind::Redundant, &format!("{DISPATCH_PREFIX}op=op-1 {}\n{segment}", expect.tail())).is_err());
        assert!(expect.classify(LegKind::Redundant, &segment.replace("surface 337", "surface 319")).is_err());
        assert!(expect.classify(LegKind::Redundant, &format!("{segment}\n{refusal}")).is_err());
    }

    #[test]
    fn native_axes_pin_singletons_omission_proxy_and_direct_focus() {
        let start = snapshot(&[(&[1], 1), (&[156], 156)]);
        assert!(other_equal(&start, &snapshot(&[(&[1, 317, 320, 319], 320), (&[156], 156)]), 0).is_ok());
        assert!(other_equal(&start, &snapshot(&[(&[1], 1), (&[156, 200], 156)]), 0).is_err());
        let retained = AxMember { wid: 10747, role: "AXWindow".into(), subrole: "AXStandardWindow".into(), fullscreen: true, frame: (0.0, 0.0, 2240.0, 1260.0) };
        let sibling = AxMember { wid: 10808, role: "AXWindow".into(), subrole: "AXStandardWindow".into(), fullscreen: false, frame: (0.0, 0.0, 800.0, 632.0) };
        assert!(row_of(&[sibling], 10747).is_none());
        let proxy = AxMember { wid: 10815, role: "AXWindow".into(), subrole: "AXUnknown".into(), fullscreen: false, frame: (0.0, 0.0, 2240.0, 1260.0) };
        assert!(entry_member(&retained, &[proxy], 10747).unwrap().1);
        assert!(entry_member(&AxMember { fullscreen: false, ..retained.clone() }, std::slice::from_ref(&retained), 10747).is_err());
        let agreement = crate::focus::FocusAgreement { workspace_pid: 12168, workspace_bundle: "io.github.omnitiler.live-fixture".into(),
            ax_pid: 12168, ax_wid: Some(10747), cg_wid_owned: true, evidence_route: DIRECT_FOCUS_ROUTE.into() };
        assert!(focus_matches(&agreement, 12168, 10747));
        assert!(!focus_matches(&crate::focus::FocusAgreement { evidence_route: "slps-front-direct-main".into(), ..agreement }, 12168, 10747));
    }

    #[test]
    fn member_writes_and_phases_fail_closed() {
        assert_eq!(scan_writes("tiler-diag stage=focus-feedback pid=9 target=12168_10747 setter=not-called reason=fenced", 12168, 10747), Ok(1));
        assert!(scan_writes("tiler-macos tile frame-set pid=12168 wid=10747 revision=2 outcome=applied", 12168, 10747).is_err());
        assert!(scan_writes("tiler-diag stage=focus-feedback pid=9 target=12168_10747 setter=applied reason=arrival", 12168, 10747).is_err());
        let mut phases = Phases::start();
        phases.mark("a").unwrap();
        phases.mark("b").unwrap();
        assert!(phases.value()[1]["at_ms"].as_u64().unwrap() >= phases.value()[0]["at_ms"].as_u64().unwrap());
    }

    #[test]
    fn fallback_exit_is_never_acceptance_success() {
        // B2: restoration-only fallback fails the run even though every
        // measured leg passed; only a proved retirement is acceptance.
        assert_eq!(acceptance_exit("retired", false), Ok(()));
        assert!(acceptance_exit("fallback", true).is_err());
        assert!(acceptance_exit("fallback", false).is_err());
        assert!(acceptance_exit("none", false).is_err());
    }

    #[test]
    fn recovery_gate_quiesces_once_and_refuses_unproved_absence() {
        // B4: the whole native owner job is bounded at 30 s, and a refused
        // product-absence proof is cached so no later recovery MC can run.
        const { assert!(TERMINAL_WAIT_MS >= 30_000, "terminal bound must cover the owner job") };
        let mut gate = RecoveryGate::new();
        let mut attempts = 0;
        assert!(!gate.attempt(|| {
            attempts += 1;
            Err("owned product still present".to_owned())
        }));
        assert!(!gate.attempt(|| {
            attempts += 1;
            Ok(())
        }), "a cached refusal never retries into a proof");
        assert_eq!(attempts, 1);
        assert_eq!(gate.evidence()["proved"], json!(false));
        let mut proved = RecoveryGate::new();
        assert!(proved.attempt(|| Ok(())));
        assert_eq!(proved.evidence()["proved"], json!(true));
    }
}
