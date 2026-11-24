//! Development-only bounded diagnostic for the D5 exact destroyed-window
//! identity question.
//!
//! One signed fixture process (already launched through LaunchServices) is
//! authenticated from its `serve` announcement, owns exactly two spawned
//! caller windows, and is observed by one dedicated exact-identity
//! `AXUIElementDestroyed` observer in the already-AX-trusted tiler probe
//! bundle. The observer joins both requested wids itself, retains the exact
//! `AxElement`s, and matches each delivered element with `CFEqual` before any
//! attribute read (see [`crate::ffi::ax_observe_destroyed`]). This module
//! drives the fixture over its nonce-gated socket and judges the resulting
//! per-wid events. It is diagnostic-only: no production transport, no roster
//! pruning, no product protocol/state, no settings writes, no input
//! synthesis.
//!
//! Bounded flow (fixed 90 s hard lifetime, per-AX 2 s messaging timeout, no
//! unbounded wait):
//! 1. Refuse outside macOS / without AX trust / with inactive Secure Input
//!    or an unknown session lock, before any mutation.
//! 2. Parse the announcement file (the only nonce source), prove the live
//!    bundle/executable/launch identity and designated requirement against
//!    `--signer`, refuse a dead/foreign/ambiguous announcement.
//! 3. Require an empty fixture, spawn exactly two caller windows, and join
//!    both through fixture-title caller/run-tag plus a live CG `(pid, wid)`
//!    owner entry; refuse an unjoined/foreign wid.
//! 4. Start the observer and require destroyed registration success on both
//!    wids before driving.
//! 5. Positive control: minimize/restore B twice, window hide/show B twice,
//!    app hide/unhide B once (public `NSRunningApplication`), Space
//!    away/back once on a controller-owned Space. Each leg requires the
//!    exact observed state and zero matched destroyed events.
//! 6. Close A only: require exactly one event matched to A, zero matched to
//!    B, zero unmatched on the healthy path, B still AX/CG-live, fixture
//!    process alive, and A absent from fresh AX plus on-screen CG.
//! 7. Stop the observer, report every registration/removal/event and the
//!    cleanup state as one JSON object. A missing leg is reported as
//!    skipped/failed, never as a pass.
//!
//! Space automation uses the existing Mission Control AX route (visible UI
//! automation, no synthetic input); a failed precondition is reported as an
//! honest skip with the reason, and the owned Space is removed on every
//! exit path.

#![forbid(unsafe_code)]

use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use crate::fixture_post::{
    bundle_verdict, fixture_designated_tail, fixture_process_identity, parse_fixture_announcement,
    parse_fixture_title, secure_input_verdict, send_fixture_close, send_fixture_get_many,
    send_fixture_list, send_fixture_set_patch, send_fixture_spawn, socket_snapshot_for,
};

/// Fixed overall hard lifetime cap (seconds).
pub const PROBE_MAX_SECONDS: u64 = 90;
/// Minimum accepted `--seconds` value.
pub const PROBE_MIN_SECONDS: u64 = 1;
/// Default `--seconds` value: the full fixed bound.
pub const PROBE_DEFAULT_SECONDS: u64 = PROBE_MAX_SECONDS;

const AX_MESSAGING_TIMEOUT_SECS: f32 = 2.0;
const REGISTRATION_WAIT: Duration = Duration::from_secs(10);
const OBSERVER_STOP_WAIT: Duration = Duration::from_secs(8);
const SETTLE_POLL: Duration = Duration::from_millis(20);
const ACTION_SETTLE: Duration = Duration::from_millis(600);
const CLOSE_SETTLE: Duration = Duration::from_secs(4);
const STATE_CONFIRM: Duration = Duration::from_secs(4);
const HIDE_CONFIRM: Duration = Duration::from_secs(3);

/// Owned caller ids (valid fixture caller-id shape).
const CALLER_A: &str = "axdestroyed-a";
const CALLER_B: &str = "axdestroyed-b";

/// CLI entry parameters for one bounded destroyed-identity diagnostic.
pub struct AxDestroyedProbeArgs {
    pub announcement_file: String,
    pub signer: String,
    pub seconds: u64,
}

/// Pure `--seconds` bound (no OS touch): nonzero and within the fixed cap.
pub fn probe_seconds_bounded(seconds: u64) -> Result<u64, String> {
    if !(PROBE_MIN_SECONDS..=PROBE_MAX_SECONDS).contains(&seconds) {
        return Err(format!(
            "refusing ax-destroyed-probe: --seconds {seconds} outside {PROBE_MIN_SECONDS}..={PROBE_MAX_SECONDS}; refusing"
        ));
    }
    Ok(seconds)
}

/// Per-window destroyed event tally: `matched_a`/`matched_b` count events
/// whose callback element compared equal to the retained element for that
/// wid; `unmatched` counts everything else (including a theoretical foreign
/// match), never silently discarded.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct EventTally {
    pub matched_a: u32,
    pub matched_b: u32,
    pub unmatched: u32,
}

impl EventTally {
    /// Difference `self - earlier` (saturating; ordering is audited
    /// separately by the callers).
    #[must_use]
    pub fn since(self, earlier: Self) -> Self {
        Self {
            matched_a: self.matched_a.saturating_sub(earlier.matched_a),
            matched_b: self.matched_b.saturating_sub(earlier.matched_b),
            unmatched: self.unmatched.saturating_sub(earlier.unmatched),
        }
    }

    fn total(self) -> u32 {
        self.matched_a
            .saturating_add(self.matched_b)
            .saturating_add(self.unmatched)
    }
}

/// Pure tally over plain events for the two owned wids.
#[must_use]
pub fn tally_events(events: &[crate::ffi::DestroyedEvent], a: u32, b: u32) -> EventTally {
    let mut tally = EventTally::default();
    for event in events {
        match event.matched_wid {
            Some(wid) if wid == a => tally.matched_a += 1,
            Some(wid) if wid == b => tally.matched_b += 1,
            Some(_) | None => tally.unmatched += 1,
        }
    }
    tally
}

/// Pure close-leg verdict: exactly one destroyed matched to A, none matched
/// to B, and no unmatched delivery on the healthy path.
pub fn close_verdict(delta: EventTally) -> Result<(), String> {
    if delta.matched_a != 1 {
        return Err(format!(
            "close leg: expected exactly one destroyed matched to A, saw {}",
            delta.matched_a
        ));
    }
    if delta.matched_b != 0 {
        return Err(format!(
            "close leg: B produced {} matched destroyed event(s)",
            delta.matched_b
        ));
    }
    if delta.unmatched != 0 {
        return Err(format!(
            "close leg: {} unmatched destroyed event(s) on the healthy path",
            delta.unmatched
        ));
    }
    Ok(())
}

/// Pure false-fire verdict for a non-destroy leg: no new event of any kind.
pub fn steady_verdict(delta: EventTally) -> Result<(), String> {
    if delta.total() != 0 {
        return Err(format!(
            "false fire: matched A {}, matched B {}, unmatched {}",
            delta.matched_a, delta.matched_b, delta.unmatched
        ));
    }
    Ok(())
}

/// Pure registration verdict: both owned wids must have a successful
/// `AXUIElementDestroyed` registration. A silent/partial watch refuses.
pub fn registration_verdict(
    registrations: &[crate::ffi::AxDestroyedRegistration],
    a: u32,
    b: u32,
) -> Result<(), String> {
    for wid in [a, b] {
        let Some(entry) = registrations.iter().find(|r| r.wid == wid) else {
            return Err(format!(
                "registration: wid {wid} has no reported registration"
            ));
        };
        if entry.notification != "AXUIElementDestroyed" {
            return Err(format!(
                "registration: wid {wid} reported unexpected notification {}",
                entry.notification
            ));
        }
        if entry.status != 0 {
            return Err(format!(
                "registration: wid {wid} AXUIElementDestroyed refused ({})",
                entry.status_name
            ));
        }
    }
    Ok(())
}

fn reply_ok(reply: &serde_json::Value) -> bool {
    reply.get("ok").and_then(serde_json::Value::as_bool) == Some(true)
}

fn window_entry<'a>(reply: &'a serde_json::Value, id: &str) -> Option<&'a serde_json::Value> {
    reply
        .get("result")
        .and_then(|result| result.get("windows"))
        .and_then(serde_json::Value::as_array)
        .and_then(|windows| {
            windows
                .iter()
                .find(|entry| entry.get("id").and_then(serde_json::Value::as_str) == Some(id))
        })
}

fn window_ok(reply: &serde_json::Value, id: &str) -> bool {
    window_entry(reply, id)
        .and_then(|entry| entry.get("ok"))
        .and_then(serde_json::Value::as_bool)
        == Some(true)
}

fn observed_field<'a>(
    reply: &'a serde_json::Value,
    id: &str,
    field: &str,
) -> Option<&'a serde_json::Value> {
    window_entry(reply, id)
        .and_then(|entry| entry.get("snapshot"))
        .and_then(|snapshot| snapshot.get("observed"))
        .and_then(|observed| observed.get(field))
        .filter(|value| !value.is_null())
}

/// Observed presentation mode string from one raw `get` reply.
#[must_use]
pub fn observed_mode<'a>(reply: &'a serde_json::Value, id: &str) -> Option<&'a str> {
    observed_field(reply, id, "mode").and_then(serde_json::Value::as_str)
}

/// Observed visibility from one raw `get` reply.
#[must_use]
pub fn observed_visible(reply: &serde_json::Value, id: &str) -> Option<bool> {
    observed_field(reply, id, "visible").and_then(serde_json::Value::as_bool)
}

fn list_ids(reply: &serde_json::Value) -> Option<Vec<String>> {
    if !reply_ok(reply) {
        return None;
    }
    reply
        .get("result")
        .and_then(|result| result.get("ids"))
        .and_then(serde_json::Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_str().map(str::to_owned))
                .collect()
        })
}

/// Observer handle. The AX work lives entirely on the observer thread; this
/// handle carries only plain events, a bounded stop signal, and the joined
/// thread handle.
struct Observer {
    events: Receiver<crate::ffi::DestroyedEvent>,
    stop: Option<Sender<()>>,
    done: Receiver<Result<crate::ffi::AxDestroyedOutcome, String>>,
    join: Option<std::thread::JoinHandle<()>>,
    collected: Vec<crate::ffi::DestroyedEvent>,
}

impl Observer {
    fn start(
        pid: u32,
        wids: &[u32],
        timeout: Duration,
    ) -> Result<(Self, Vec<crate::ffi::AxDestroyedRegistration>), String> {
        let pid_i32 = i32::try_from(pid)
            .map_err(|_| format!("destroyed observer: pid {pid} out of range"))?;
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let (events_tx, events_rx) = channel();
        let (stop_tx, stop_rx) = channel();
        let (done_tx, done_rx) = channel();
        let wids_owned = wids.to_vec();
        let join = std::thread::Builder::new()
            .name(format!("ax-destroyed-observe-{pid}"))
            .spawn(move || {
                let outcome = crate::ffi::ax_observe_destroyed(
                    pid_i32,
                    &wids_owned,
                    timeout,
                    AX_MESSAGING_TIMEOUT_SECS,
                    &ready_tx,
                    &events_tx,
                    &stop_rx,
                );
                let _ = done_tx.send(outcome);
            })
            .map_err(|e| format!("destroyed observer thread spawn refused ({e})"))?;
        match ready_rx.recv_timeout(REGISTRATION_WAIT) {
            Ok(Ok(registrations)) => Ok((
                Self {
                    events: events_rx,
                    stop: Some(stop_tx),
                    done: done_rx,
                    join: Some(join),
                    collected: Vec::new(),
                },
                registrations,
            )),
            Ok(Err(error)) => {
                let _ = join.join();
                Err(error)
            }
            Err(_) => {
                // The observer thread is bounded by its own finite runloop;
                // dropping the stop sender makes it exit at the next slice.
                drop(stop_tx);
                Err("destroyed observer did not report registrations within 10s".to_owned())
            }
        }
    }

    fn drain(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            self.collected.push(event);
        }
    }

    fn tally(&self, a: u32, b: u32) -> EventTally {
        tally_events(&self.collected, a, b)
    }

    /// Drain-and-sleep settle window; the observer thread pumps throughout.
    fn settle(&mut self, window: Duration) {
        let deadline = Instant::now() + window;
        loop {
            self.drain();
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            std::thread::sleep(SETTLE_POLL.min(deadline - now));
        }
        self.drain();
    }

    /// Drain until `condition` holds or the window expires. Returns whether
    /// the condition was observed.
    fn settle_until(
        &mut self,
        window: Duration,
        mut condition: impl FnMut(&[crate::ffi::DestroyedEvent]) -> bool,
    ) -> bool {
        let deadline = Instant::now() + window;
        loop {
            self.drain();
            if condition(&self.collected) {
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return condition(&self.collected);
            }
            std::thread::sleep(SETTLE_POLL.min(deadline - now));
        }
    }

    fn finish(&mut self) -> Result<crate::ffi::AxDestroyedOutcome, String> {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        let outcome = match self.done.recv_timeout(OBSERVER_STOP_WAIT) {
            Ok(result) => result,
            Err(_) => Err("destroyed observer did not stop within 8s".to_owned()),
        };
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        self.drain();
        outcome
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct LegReport {
    leg: String,
    status: String,
    detail: String,
    matched_a: u32,
    matched_b: u32,
    unmatched: u32,
    tally_after: EventTally,
}

impl LegReport {
    fn new(leg: &str, before: EventTally, after: EventTally, outcome: Result<(), String>) -> Self {
        let delta = after.since(before);
        let (status, detail) = match outcome {
            Ok(()) => ("ok".to_owned(), String::new()),
            Err(error) => ("fail".to_owned(), error),
        };
        Self {
            leg: leg.to_owned(),
            status,
            detail,
            matched_a: delta.matched_a,
            matched_b: delta.matched_b,
            unmatched: delta.unmatched,
            tally_after: after,
        }
    }

    fn skipped(leg: &str, reason: String, tally: EventTally) -> Self {
        Self {
            leg: leg.to_owned(),
            status: "skipped".to_owned(),
            detail: reason,
            matched_a: 0,
            matched_b: 0,
            unmatched: 0,
            tally_after: tally,
        }
    }

    fn ok(&self) -> bool {
        self.status == "ok"
    }
}

#[derive(Debug, Clone, Serialize)]
struct LivenessReport {
    fixture_alive: bool,
    b_ax_visible: bool,
    b_cg_owned: bool,
    b_cg_onscreen: bool,
    a_ax_absent: bool,
    a_cg_onscreen_absent: bool,
    list_has_a: bool,
    list_has_b: bool,
}

#[derive(Debug, Clone, Serialize)]
struct CleanupReport {
    owned_space: Option<u64>,
    space_removed: Option<bool>,
    space_restore: String,
    windows_closed: Vec<String>,
    errors: Vec<String>,
}

#[derive(Serialize)]
struct ProbeReport {
    outcome: String,
    pid: u32,
    wid_a: u32,
    wid_b: u32,
    seconds: u64,
    elapsed_ms: u64,
    registrations: Vec<crate::ffi::AxDestroyedRegistration>,
    removals: Vec<crate::ffi::AxDestroyedRegistration>,
    other_notifications: Vec<(String, u32)>,
    messaging_timeout_set: bool,
    observer_timed_out: bool,
    events: Vec<crate::ffi::DestroyedEvent>,
    tally: EventTally,
    legs: Vec<LegReport>,
    liveness: LivenessReport,
    cleanup: CleanupReport,
    note: String,
}

fn session_gates() -> Result<(), String> {
    if crate::ffi::ax_is_process_trusted() != Some(true) {
        return Err("ax-trust: AX trust not observed true; stop".to_owned());
    }
    secure_input_verdict(crate::ffi::secure_input_known())?;
    match crate::ffi::session_state().lock {
        crate::ffi::SessionLock::Unlocked => Ok(()),
        crate::ffi::SessionLock::Locked => {
            Err("session: session locked; refusing diagnostic".to_owned())
        }
        crate::ffi::SessionLock::Unknown => {
            Err("session: session unknown; refusing diagnostic".to_owned())
        }
    }
}

fn ax_wids_for(pid: u32) -> Result<Vec<u32>, String> {
    use crate::ffi::{AxElement, ax_error_name, ax_set_messaging_timeout, ax_window_id};
    let pid_i32 = i32::try_from(pid).map_err(|_| format!("pid {pid} out of range"))?;
    let app = AxElement::application(pid_i32)
        .ok_or_else(|| format!("no AX application element for pid {pid}"))?;
    let _ = ax_set_messaging_timeout(&app, AX_MESSAGING_TIMEOUT_SECS);
    let windows = app
        .windows()
        .map_err(|code| format!("AXWindows unreadable ({})", ax_error_name(code)))?;
    let mut wids = Vec::with_capacity(windows.len());
    for window in &windows {
        let _ = ax_set_messaging_timeout(window, AX_MESSAGING_TIMEOUT_SECS);
        if let Some(wid) = ax_window_id(window) {
            wids.push(wid);
        }
    }
    Ok(wids)
}

/// Join both owned caller windows through the fixture's server-controlled
/// title (caller + run tag) and a fresh basic CG `(pid, wid)` owner entry.
/// Every caller must join; a foreign or unjoined wid refuses.
fn join_owned_windows(
    pid: u32,
    socket: &str,
    nonce: &str,
    callers: &[&str],
) -> Result<BTreeMap<String, u32>, String> {
    use crate::ffi::{
        AxElement, ax_error_name, ax_set_messaging_timeout, ax_window_id, ax_window_title,
    };
    let pid_i32 = i32::try_from(pid).map_err(|_| format!("pid {pid} out of range"))?;
    let app = AxElement::application(pid_i32)
        .ok_or_else(|| format!("join: no AX application element for pid {pid}"))?;
    let _ = ax_set_messaging_timeout(&app, AX_MESSAGING_TIMEOUT_SECS);
    let elements = app.windows().map_err(|code| {
        format!(
            "join: AXWindows unreadable for pid {pid} ({})",
            ax_error_name(code)
        )
    })?;
    let cg = crate::ffi::cg_window_list_basic()
        .map_err(|e| format!("join: CG snapshot unreadable ({e})"))?;
    let mut joined: BTreeMap<String, u32> = BTreeMap::new();
    for caller in callers {
        let snapshot = socket_snapshot_for(socket, nonce, caller)
            .map_err(|e| format!("join: caller {caller} snapshot refused ({e})"))?;
        if snapshot.pid != pid {
            return Err(format!(
                "join: caller {caller} snapshot pid {} != announced pid {pid}; refusing",
                snapshot.pid
            ));
        }
        let mut found: Option<u32> = None;
        for element in &elements {
            let Ok(Some(title)) = ax_window_title(element) else {
                continue;
            };
            let Ok((native_caller, native_tag)) = parse_fixture_title(&title) else {
                continue;
            };
            if native_caller != *caller || native_tag != snapshot.run_tag {
                continue;
            }
            let Some(wid) = ax_window_id(element) else {
                continue;
            };
            if wid == 0 || joined.values().any(|known| *known == wid) {
                continue;
            }
            if !cg.iter().any(|entry| entry.pid == pid && entry.wid == wid) {
                continue;
            }
            found = Some(wid);
            break;
        }
        let wid = found.ok_or_else(|| {
            format!(
                "join: caller {caller} has no live AX+CG window join; refusing foreign/unjoined wid"
            )
        })?;
        joined.insert((*caller).to_owned(), wid);
    }
    Ok(joined)
}

/// Send one fixture `set` patch and require the request plus the per-window
/// entry to be accepted (arrival is proven separately by the observed read).
fn fixture_set(
    socket: &str,
    nonce: &str,
    id: &str,
    patch: serde_json::Value,
) -> Result<(), String> {
    let reply = send_fixture_set_patch(socket, nonce, &[id], patch)
        .map_err(|e| format!("set {id} transport refused ({e})"))?;
    if !reply_ok(&reply) || !window_ok(&reply, id) {
        return Err(format!("set {id} refused by the fixture server"));
    }
    Ok(())
}

enum Expected {
    Mode(&'static str),
    Visible(bool),
}

fn fixture_await_observed(
    socket: &str,
    nonce: &str,
    id: &str,
    expected: Expected,
) -> Result<(), String> {
    let deadline = Instant::now() + STATE_CONFIRM;
    loop {
        let reply = send_fixture_get_many(socket, nonce, &[id])
            .map_err(|e| format!("get {id} transport refused ({e})"))?;
        let observed = match expected {
            Expected::Mode(want) => observed_mode(&reply, id) == Some(want),
            Expected::Visible(want) => observed_visible(&reply, id) == Some(want),
        };
        if observed {
            return Ok(());
        }
        let now = Instant::now();
        if now >= deadline {
            let detail = match expected {
                Expected::Mode(want) => {
                    format!("observed mode never became {want} within 4s")
                }
                Expected::Visible(want) => {
                    format!("observed visible never became {want} within 4s")
                }
            };
            return Err(detail);
        }
        std::thread::sleep(SETTLE_POLL.min(deadline - now));
    }
}

fn drive_leg(
    observer: &mut Observer,
    leg: &str,
    a: u32,
    b: u32,
    action: impl FnOnce() -> Result<(), String>,
) -> LegReport {
    let before = observer.tally(a, b);
    let action_result = action();
    observer.settle(ACTION_SETTLE);
    let after = observer.tally(a, b);
    let verdict = action_result.and_then(|()| steady_verdict(after.since(before)));
    LegReport::new(leg, before, after, verdict)
}

fn close_a_leg(
    observer: &mut Observer,
    socket: &str,
    nonce: &str,
    pid: u32,
    a: u32,
    b: u32,
) -> LegReport {
    let before = observer.tally(a, b);
    let action = (|| -> Result<(), String> {
        let reply = send_fixture_close(socket, nonce, &[CALLER_A])
            .map_err(|e| format!("close A transport refused ({e})"))?;
        if !reply_ok(&reply) || !window_ok(&reply, CALLER_A) {
            return Err("close A refused by the fixture server".to_owned());
        }
        // Wait for the expected delivery (bounded) instead of a blind sleep.
        let delivered = observer.settle_until(CLOSE_SETTLE, |events| {
            tally_events(events, a, b).matched_a > before.matched_a
        });
        if !delivered {
            return Err(format!(
                "close leg: no destroyed delivery matched to A within {}s",
                CLOSE_SETTLE.as_secs()
            ));
        }
        Ok(())
    })();
    observer.settle(ACTION_SETTLE);
    let after = observer.tally(a, b);
    let delta = after.since(before);
    let mut verdict = action.and_then(|()| close_verdict(delta));
    // Fresh liveness facts for the same close boundary.
    let ax_after = ax_wids_for(pid);
    match &ax_after {
        Ok(wids) if wids.contains(&a) => {
            verdict = Err(merge_error(
                verdict,
                format!("close leg: A wid {a} still present in fresh AXWindows"),
            ));
        }
        Ok(wids) if !wids.contains(&b) => {
            verdict = Err(merge_error(
                verdict,
                format!("close leg: B wid {b} absent from fresh AXWindows (survivor lost)"),
            ));
        }
        Ok(_) => {}
        Err(e) => {
            verdict = Err(merge_error(
                verdict,
                format!("close leg: fresh AX read failed ({e})"),
            ))
        }
    }
    match crate::ffi::cg_window_list_basic() {
        Ok(entries) => {
            if entries
                .iter()
                .any(|entry| entry.pid == pid && entry.wid == a && entry.onscreen)
            {
                verdict = Err(merge_error(
                    verdict,
                    format!("close leg: A wid {a} still on-screen in CG"),
                ));
            }
            if !entries
                .iter()
                .any(|entry| entry.pid == pid && entry.wid == b)
            {
                verdict = Err(merge_error(
                    verdict,
                    format!("close leg: B wid {b} absent from CG (survivor lost)"),
                ));
            }
        }
        Err(e) => {
            verdict = Err(merge_error(
                verdict,
                format!("close leg: CG read failed ({e})"),
            ))
        }
    }
    if !crate::ffi::process_exists(i32::try_from(pid).unwrap_or(-1)) {
        verdict = Err(merge_error(
            verdict,
            "close leg: fixture process no longer alive".to_owned(),
        ));
    }
    LegReport::new("close-a", before, after, verdict)
}

fn merge_error(previous: Result<(), String>, next: String) -> String {
    match previous {
        Ok(()) => next,
        Err(existing) => format!("{existing}; {next}"),
    }
}

/// Space away/back on one controller-owned Space. Precondition failures are
/// reported as an honest skip; after a Space is owned, restore + removal are
/// attempted on every path and their statuses reported.
fn space_leg(observer: &mut Observer, a: u32, b: u32) -> (LegReport, Option<u64>, Option<bool>) {
    use crate::space_probe::{LiveNativeSpaces, NativeSpaces, remove_owned_space};
    // Per-action Mission Control budget: a single create/goto/goto/remove
    // sequence gets a 30s work window above the fixed 15s cleanup reserve
    // (`MC_MIN_LIFECYCLE_SECS`), all inside the fixed overall lifetime; the
    // owned Space is still removed on every path. A 10s budget would leave
    // zero main-phase time and refused before the first press.
    let mut spaces = LiveNativeSpaces::new(crate::mc_probe::MC_MIN_LIFECYCLE_SECS);
    let display = 0usize;
    let before = observer.tally(a, b);
    let snapshot = match spaces.space_snapshot() {
        Ok(snapshot) => snapshot,
        Err(e) => {
            return (
                LegReport::skipped(
                    "space-away-back",
                    format!("skipped: Space snapshot unreadable ({e})"),
                    before,
                ),
                None,
                None,
            );
        }
    };
    let Some(row) = snapshot.displays.get(display) else {
        return (
            LegReport::skipped(
                "space-away-back",
                format!("skipped: display {display} absent from the Space snapshot"),
                before,
            ),
            None,
            None,
        );
    };
    let starting = row.current;
    let created = match spaces.space_create(display) {
        Ok(created) => created,
        Err(e) => {
            return (
                LegReport::skipped(
                    "space-away-back",
                    format!("skipped: owned Space creation refused ({e})"),
                    before,
                ),
                None,
                None,
            );
        }
    };
    let sid = created.sid;
    let mut failures: Vec<String> = Vec::new();
    if let Some(shortfall) = created.mc.shortfall_text("owned Space create") {
        failures.push(shortfall);
    }
    let current_sid = |spaces: &mut LiveNativeSpaces| -> Result<u64, String> {
        let topo = spaces.space_snapshot()?;
        topo.displays
            .get(display)
            .map(|row| row.current)
            .ok_or_else(|| format!("display {display} absent from the Space snapshot"))
    };
    if let Err(e) = spaces.space_goto(display, sid) {
        failures.push(format!("goto owned Space {sid} refused ({e})"));
    } else {
        match current_sid(&mut spaces) {
            Ok(current) if current == sid => {}
            Ok(current) => failures.push(format!(
                "after goto owned Space, current SID {current} != {sid}"
            )),
            Err(e) => failures.push(format!("post-goto snapshot failed ({e})")),
        }
    }
    observer.settle(ACTION_SETTLE);
    let mid = observer.tally(a, b);
    if let Err(e) = steady_verdict(mid.since(before)) {
        failures.push(format!("space away: {e}"));
    }
    if let Err(e) = spaces.space_goto(display, starting) {
        failures.push(format!("goto starting Space {starting} refused ({e})"));
    } else {
        match current_sid(&mut spaces) {
            Ok(current) if current == starting => {}
            Ok(current) => failures.push(format!(
                "after goto starting Space, current SID {current} != {starting}"
            )),
            Err(e) => failures.push(format!("post-return snapshot failed ({e})")),
        }
    }
    observer.settle(ACTION_SETTLE);
    let after = observer.tally(a, b);
    if let Err(e) = steady_verdict(after.since(before)) {
        failures.push(format!("space away/back: {e}"));
    }
    match current_sid(&mut spaces) {
        Ok(current) if current == starting => {}
        Ok(current) => {
            // Best-effort restore before removal.
            let _ = spaces.space_goto(display, starting);
            failures.push(format!(
                "Space cleanup: current SID {current} != starting {starting}"
            ));
        }
        Err(e) => failures.push(format!("Space cleanup read failed ({e})")),
    }
    let removal = remove_owned_space(&mut spaces, display, sid);
    let removed_ok = removal.is_ok();
    if let Err(e) = removal {
        failures.push(format!("owned Space {sid} removal failed ({e})"));
    }
    let leg = if failures.is_empty() {
        LegReport::new("space-away-back", before, after, Ok(()))
    } else {
        LegReport::new("space-away-back", before, after, Err(failures.join("; ")))
    };
    (leg, Some(sid), Some(removed_ok))
}

fn app_hide_leg(observer: &mut Observer, pid: u32, a: u32, b: u32) -> LegReport {
    let before = observer.tally(a, b);
    let pid_i32 = match i32::try_from(pid) {
        Ok(pid) => pid,
        Err(_) => {
            return LegReport::new(
                "app-hide-unhide",
                before,
                before,
                Err("app hide: pid out of range".to_owned()),
            );
        }
    };
    let mut failures: Vec<String> = Vec::new();
    // The request's immediate return is only dispatch; the bounded
    // `isHidden` readback below is the verification (AppKit hide is async).
    if let Err(e) = crate::ffi::app_set_hidden(pid_i32, true) {
        failures.push(format!("app hide request refused ({e})"));
    }
    // Verify the transition from the public readback, bounded.
    let hidden_deadline = Instant::now() + HIDE_CONFIRM;
    loop {
        match crate::ffi::app_hidden_for_pid(pid_i32) {
            Ok(true) => break,
            Ok(false) => {}
            Err(e) => {
                failures.push(format!("app hidden readback refused ({e})"));
                break;
            }
        }
        if Instant::now() >= hidden_deadline {
            failures.push("app hide: isHidden never observed true within 3s".to_owned());
            break;
        }
        std::thread::sleep(SETTLE_POLL);
    }
    observer.settle(ACTION_SETTLE);
    let mid = observer.tally(a, b);
    if let Err(e) = steady_verdict(mid.since(before)) {
        failures.push(format!("app hidden: {e}"));
    }
    if let Err(e) = crate::ffi::app_set_hidden(pid_i32, false) {
        failures.push(format!("app unhide request refused ({e})"));
    }
    let shown_deadline = Instant::now() + HIDE_CONFIRM;
    loop {
        match crate::ffi::app_hidden_for_pid(pid_i32) {
            Ok(false) => break,
            Ok(true) => {}
            Err(e) => {
                failures.push(format!("app visible readback refused ({e})"));
                break;
            }
        }
        if Instant::now() >= shown_deadline {
            failures.push("app unhide: isHidden never observed false within 3s".to_owned());
            break;
        }
        std::thread::sleep(SETTLE_POLL);
    }
    observer.settle(ACTION_SETTLE);
    let after = observer.tally(a, b);
    if let Err(e) = steady_verdict(after.since(before)) {
        failures.push(format!("app hide/unhide: {e}"));
    }
    if failures.is_empty() {
        LegReport::new("app-hide-unhide", before, after, Ok(()))
    } else {
        LegReport::new("app-hide-unhide", before, after, Err(failures.join("; ")))
    }
}

fn read_liveness(pid: u32, socket: &str, nonce: &str, a: u32, b: u32) -> LivenessReport {
    let ax = ax_wids_for(pid).unwrap_or_default();
    let cg = crate::ffi::cg_window_list_basic().unwrap_or_default();
    let ids = send_fixture_list(socket, nonce)
        .ok()
        .and_then(|reply| list_ids(&reply))
        .unwrap_or_default();
    let b_cg = cg.iter().find(|entry| entry.pid == pid && entry.wid == b);
    LivenessReport {
        fixture_alive: crate::ffi::process_exists(i32::try_from(pid).unwrap_or(-1)),
        b_ax_visible: ax.contains(&b),
        b_cg_owned: b_cg.is_some(),
        b_cg_onscreen: b_cg.is_some_and(|entry| entry.onscreen),
        a_ax_absent: !ax.contains(&a),
        a_cg_onscreen_absent: !cg
            .iter()
            .any(|entry| entry.pid == pid && entry.wid == a && entry.onscreen),
        list_has_a: ids.iter().any(|id| id == CALLER_A),
        list_has_b: ids.iter().any(|id| id == CALLER_B),
    }
}

/// Development-only bounded destroyed-identity diagnostic.
/// Exit codes: 2 usage/validation, 1 gate/proof/diagnostic failure with the
/// JSON evidence report.
#[cfg(target_os = "macos")]
pub fn run_ax_destroyed_probe(args: &AxDestroyedProbeArgs) -> Result<String, (i32, String)> {
    let err_json = |stage: &str, error: String| -> (i32, String) {
        (
            1,
            serde_json::json!({"stage": stage, "error": error, "outcome": "refused"}).to_string(),
        )
    };
    let refuse = |stage: &str, error: String| -> (i32, String) {
        eprintln!("ax-destroyed-probe refused at {stage}: {error}");
        err_json(stage, error)
    };
    if args.announcement_file.is_empty() {
        return Err((
            2,
            "ax-destroyed-probe needs a nonempty --announcement-file".to_owned(),
        ));
    }
    if args.signer.trim().is_empty() {
        return Err((2, "ax-destroyed-probe needs a nonempty --signer".to_owned()));
    }
    let seconds = probe_seconds_bounded(args.seconds).map_err(|e| (2, e))?;
    let start = Instant::now();
    let mut _watchdog =
        crate::watchdog::Watchdog::arm(Duration::from_secs(seconds)).map_err(|e| (1, e))?;
    // Every refusal above this line precedes any mutation.
    session_gates().map_err(|e| refuse("session", e))?;
    let raw = std::fs::read_to_string(&args.announcement_file).map_err(|e| {
        (
            2,
            format!("ax-destroyed-probe announcement file unreadable ({e})"),
        )
    })?;
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.lines().count() != 1 {
        return Err((
            2,
            "ambiguity refused: announcement must be exactly one JSON line".to_owned(),
        ));
    }
    let announced = parse_fixture_announcement(trimmed)
        .map_err(|e| (2, format!("announcement refused: {e}")))?;
    let pid = announced.pid;
    if pid <= 1 {
        return Err((2, format!("refusing reserved pid {pid}")));
    }
    if pid == std::process::id() {
        return Err((2, "refusing the controller itself as subject".to_owned()));
    }
    if i32::try_from(pid).is_err() {
        return Err((2, format!("refusing pid {pid}: out of range")));
    }
    if !crate::ffi::process_exists(pid as i32) {
        return Err(refuse(
            "identity",
            "announcement pid is not a live process; refusing".to_owned(),
        ));
    }
    let identity = fixture_process_identity(pid).map_err(|e| refuse("identity", e))?;
    bundle_verdict(identity.bundle.as_deref())
        .map_err(|e| refuse("identity", format!("identity: {e}")))?;
    let exe = identity.exe.filter(|exe| !exe.is_empty()).ok_or_else(|| {
        refuse(
            "identity",
            "identity: executable path unreadable; refusing".to_owned(),
        )
    })?;
    let _launch = identity
        .launch
        .filter(|launch| !launch.is_empty())
        .ok_or_else(|| {
            refuse(
                "identity",
                "identity: launch date unreadable; refusing".to_owned(),
            )
        })?;
    let tail = fixture_designated_tail(&exe).map_err(|e| refuse("signature", e))?;
    if tail != args.signer.trim() {
        return Err(refuse(
            "signature",
            "signature: designated requirement mismatch (not the expected fixture signer); refusing"
                .to_owned(),
        ));
    }
    let list = send_fixture_list(&announced.socket, &announced.nonce)
        .map_err(|e| refuse("socket", format!("socket: {e}")))?;
    let existing = list_ids(&list).ok_or_else(|| {
        refuse(
            "socket",
            "socket: list reply malformed; refusing".to_owned(),
        )
    })?;
    if !existing.is_empty() {
        return Err(refuse(
            "precondition",
            format!(
                "fixture already owns {} window(s); this diagnostic requires an empty fixture",
                existing.len()
            ),
        ));
    }
    // Mutation starts here: own exactly two fixture windows.
    let spawn = send_fixture_spawn(&announced.socket, &announced.nonce, &[CALLER_A, CALLER_B])
        .map_err(|e| refuse("spawn", format!("spawn refused ({e})")))?;
    if !reply_ok(&spawn) {
        return Err(refuse(
            "spawn",
            "spawn reply refused by the fixture".to_owned(),
        ));
    }
    for caller in [CALLER_A, CALLER_B] {
        if !window_ok(&spawn, caller) {
            let _ = send_fixture_close(&announced.socket, &announced.nonce, &[CALLER_A, CALLER_B]);
            return Err(refuse(
                "spawn",
                format!("fixture refused to spawn owned caller {caller}"),
            ));
        }
    }
    let joined = match join_owned_windows(
        pid,
        &announced.socket,
        &announced.nonce,
        &[CALLER_A, CALLER_B],
    ) {
        Ok(joined) => joined,
        Err(error) => {
            let _ = send_fixture_close(&announced.socket, &announced.nonce, &[CALLER_A, CALLER_B]);
            return Err(refuse("join", error));
        }
    };
    let (Some(&wid_a), Some(&wid_b)) = (joined.get(CALLER_A), joined.get(CALLER_B)) else {
        let _ = send_fixture_close(&announced.socket, &announced.nonce, &[CALLER_A, CALLER_B]);
        return Err(refuse(
            "join",
            "joined wid map lacks an owned caller".to_owned(),
        ));
    };
    if wid_a == wid_b {
        let _ = send_fixture_close(&announced.socket, &announced.nonce, &[CALLER_A, CALLER_B]);
        return Err(refuse(
            "join",
            "both owned callers joined the same wid; refusing".to_owned(),
        ));
    }
    // Observer bound: leave room inside the overall lifetime for teardown.
    let remaining = seconds.saturating_sub(start.elapsed().as_secs()).max(1);
    let observer_timeout = Duration::from_secs(remaining.saturating_sub(2).max(1));
    let (mut observer, registrations) =
        match Observer::start(pid, &[wid_a, wid_b], observer_timeout) {
            Ok(started) => started,
            Err(error) => {
                let _ =
                    send_fixture_close(&announced.socket, &announced.nonce, &[CALLER_A, CALLER_B]);
                return Err(refuse("observer", error));
            }
        };
    if let Err(error) = registration_verdict(&registrations, wid_a, wid_b) {
        let _ = send_fixture_close(&announced.socket, &announced.nonce, &[CALLER_A, CALLER_B]);
        return Err(refuse("observer", format!("{error}; refusing to drive")));
    }

    let mut legs: Vec<LegReport> = Vec::new();
    let socket = announced.socket.clone();
    let nonce = announced.nonce.clone();

    // Positive control: minimize/restore B, zero destroyed.
    for cycle in 1..=2 {
        legs.push(drive_leg(
            &mut observer,
            &format!("minimize-restore-{cycle}"),
            wid_a,
            wid_b,
            || {
                fixture_set(
                    &socket,
                    &nonce,
                    CALLER_B,
                    serde_json::json!({"mode": "minimized"}),
                )?;
                fixture_await_observed(&socket, &nonce, CALLER_B, Expected::Mode("minimized"))?;
                fixture_set(
                    &socket,
                    &nonce,
                    CALLER_B,
                    serde_json::json!({"mode": "windowed"}),
                )?;
                fixture_await_observed(&socket, &nonce, CALLER_B, Expected::Mode("windowed"))
            },
        ));
    }
    // Window hide/show B. The diagnostic expects zero, but a signed-fixture
    // run observed the first orderOut deliver AXUIElementDestroyed for the
    // still-alive element; the leg records that false fire so the result is
    // never read as close-only authority. Split hide from show so a false
    // fire attributes to the exact transition.
    for cycle in 1..=2 {
        legs.push(drive_leg(
            &mut observer,
            &format!("window-hide-{cycle}"),
            wid_a,
            wid_b,
            || {
                fixture_set(
                    &socket,
                    &nonce,
                    CALLER_B,
                    serde_json::json!({"visible": false}),
                )?;
                fixture_await_observed(&socket, &nonce, CALLER_B, Expected::Visible(false))
            },
        ));
        legs.push(drive_leg(
            &mut observer,
            &format!("window-show-{cycle}"),
            wid_a,
            wid_b,
            || {
                fixture_set(
                    &socket,
                    &nonce,
                    CALLER_B,
                    serde_json::json!({"visible": true}),
                )?;
                fixture_await_observed(&socket, &nonce, CALLER_B, Expected::Visible(true))
            },
        ));
    }
    // App hide/unhide (public AppKit; the Cmd+H equivalent, no input).
    legs.push(app_hide_leg(&mut observer, pid, wid_a, wid_b));

    // Close A only; the central identity measurement.
    legs.push(close_a_leg(
        &mut observer,
        &socket,
        &nonce,
        pid,
        wid_a,
        wid_b,
    ));

    // Space away/back on a controller-owned Space, zero destroyed. Run it
    // after the close measurement: switching away can invalidate AX elements,
    // so it must not disturb the exact-close observer registration.
    let (space_report, owned_space, space_removed) = space_leg(&mut observer, wid_a, wid_b);
    legs.push(space_report);

    let liveness = read_liveness(pid, &socket, &nonce, wid_a, wid_b);
    let outcome = observer.finish();
    let events = observer.collected.clone();

    // Cleanup every owned resource: close the surviving owned fixture
    // window; the fixture process itself belongs to the operator.
    let mut cleanup = CleanupReport {
        owned_space,
        space_removed,
        space_restore: if owned_space.is_some() {
            "restored-starting-then-removed-owned (see space leg detail)".to_owned()
        } else {
            "no-owned-space-created (see space leg detail)".to_owned()
        },
        windows_closed: Vec::new(),
        errors: Vec::new(),
    };
    let close_reply = send_fixture_close(&socket, &nonce, &[CALLER_B]);
    match close_reply {
        Ok(reply) if reply_ok(&reply) && window_ok(&reply, CALLER_B) => {
            cleanup.windows_closed.push(CALLER_B.to_owned());
        }
        Ok(_) => cleanup
            .errors
            .push("cleanup: fixture refused to close B".to_owned()),
        Err(e) => cleanup
            .errors
            .push(format!("cleanup: close B transport refused ({e})")),
    }

    let observe_failed = legs.iter().any(|leg| !leg.ok());
    let (registrations, removals, other_notifications, messaging_timeout_set, observer_timed_out) =
        match outcome {
            Ok(outcome) => {
                if let Err(error) = registration_verdict(&outcome.registrations, wid_a, wid_b) {
                    cleanup.errors.push(format!("observer: {error}"));
                }
                (
                    outcome.registrations,
                    outcome.removals,
                    outcome.other_notifications,
                    outcome.messaging_timeout_set,
                    outcome.timed_out,
                )
            }
            Err(error) => {
                cleanup.errors.push(format!("observer teardown: {error}"));
                (registrations, Vec::new(), Vec::new(), false, false)
            }
        };
    let tally = tally_events(&events, wid_a, wid_b);
    let liveness_failed = !liveness.fixture_alive
        || !liveness.b_ax_visible
        || !liveness.b_cg_owned
        || !liveness.a_ax_absent
        || !liveness.a_cg_onscreen_absent
        || liveness.list_has_a
        || !liveness.list_has_b;
    let outcome_label = if observe_failed || liveness_failed || !cleanup.errors.is_empty() {
        "fail"
    } else {
        "pass"
    };
    let report = ProbeReport {
        outcome: outcome_label.to_owned(),
        pid,
        wid_a,
        wid_b,
        seconds,
        elapsed_ms: start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        registrations,
        removals,
        other_notifications,
        messaging_timeout_set,
        observer_timed_out,
        events,
        tally,
        legs,
        liveness,
        cleanup,
        note: "development-only bounded D5 destroyed-identity diagnostic: one exact-identity AXUIElementDestroyed observer over two signed-fixture windows, positive controls before/after an exact close of A; no production transport, no roster pruning, no input synthesis, no settings writes; Space automation is visible Mission Control AX control with owned-Space cleanup".to_owned(),
    };
    let json = serde_json::to_string(&report).map_err(|e| (1, e.to_string()))?;
    _watchdog.disarm();
    if outcome_label == "pass" {
        Ok(json)
    } else {
        Err((1, json))
    }
}

/// Non-macOS fallback: AX/destroyed observation is macOS-only.
#[cfg(not(target_os = "macos"))]
pub fn run_ax_destroyed_probe(_args: &AxDestroyedProbeArgs) -> Result<String, (i32, String)> {
    Err((1, "ax-destroyed-probe is macOS-only".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::{AxDestroyedRegistration, DestroyedEvent};

    fn event(matched: Option<u32>, secondary: Option<u32>) -> DestroyedEvent {
        DestroyedEvent {
            matched_wid: matched,
            get_window_wid: secondary,
            at_ms: 1,
        }
    }

    fn registration(wid: u32, status: i32) -> AxDestroyedRegistration {
        AxDestroyedRegistration {
            wid,
            notification: "AXUIElementDestroyed".to_owned(),
            status,
            status_name: crate::ffi::ax_error_name(status),
        }
    }

    #[test]
    fn ax_destroyed_tally_separates_owned_matches_from_unmatched() {
        let events = vec![
            event(Some(10), None),
            event(Some(10), None),
            event(Some(20), None),
            event(None, Some(10)),
            event(None, None),
        ];
        let tally = tally_events(&events, 10, 20);
        assert_eq!(tally.matched_a, 2);
        assert_eq!(tally.matched_b, 1);
        assert_eq!(tally.unmatched, 2);
        let delta = tally.since(EventTally {
            matched_a: 1,
            matched_b: 1,
            unmatched: 0,
        });
        assert_eq!(
            delta,
            EventTally {
                matched_a: 1,
                matched_b: 0,
                unmatched: 2
            }
        );
    }

    #[test]
    fn ax_destroyed_close_verdict_requires_exactly_one_a_and_no_false_fires() {
        assert!(
            close_verdict(EventTally {
                matched_a: 1,
                matched_b: 0,
                unmatched: 0
            })
            .is_ok()
        );
        assert!(
            close_verdict(EventTally {
                matched_a: 0,
                matched_b: 0,
                unmatched: 0
            })
            .is_err()
        );
        assert!(
            close_verdict(EventTally {
                matched_a: 2,
                matched_b: 0,
                unmatched: 0
            })
            .is_err()
        );
        assert!(
            close_verdict(EventTally {
                matched_a: 1,
                matched_b: 1,
                unmatched: 0
            })
            .is_err()
        );
        assert!(
            close_verdict(EventTally {
                matched_a: 1,
                matched_b: 0,
                unmatched: 1
            })
            .is_err()
        );
    }

    #[test]
    fn ax_destroyed_steady_verdict_rejects_any_new_event() {
        assert!(steady_verdict(EventTally::default()).is_ok());
        for delta in [
            EventTally {
                matched_a: 1,
                ..EventTally::default()
            },
            EventTally {
                matched_b: 1,
                ..EventTally::default()
            },
            EventTally {
                unmatched: 1,
                ..EventTally::default()
            },
        ] {
            assert!(steady_verdict(delta).is_err());
        }
    }

    #[test]
    fn ax_destroyed_registration_verdict_requires_success_on_both_wids() {
        assert!(registration_verdict(&[registration(1, 0), registration(2, 0)], 1, 2).is_ok());
        assert!(registration_verdict(&[registration(1, 0)], 1, 2).is_err());
        assert!(
            registration_verdict(&[registration(1, 0), registration(2, -25204)], 1, 2).is_err()
        );
        let wrong = AxDestroyedRegistration {
            wid: 2,
            notification: "AXWindowMiniaturized".to_owned(),
            status: 0,
            status_name: "kAXErrorSuccess".to_owned(),
        };
        assert!(registration_verdict(&[registration(1, 0), wrong], 1, 2).is_err());
    }

    #[test]
    fn ax_destroyed_seconds_are_bounded() {
        assert_eq!(probe_seconds_bounded(1).unwrap(), 1);
        assert_eq!(
            probe_seconds_bounded(PROBE_MAX_SECONDS).unwrap(),
            PROBE_MAX_SECONDS
        );
        assert!(probe_seconds_bounded(0).is_err());
        assert!(probe_seconds_bounded(PROBE_MAX_SECONDS + 1).is_err());
    }

    #[test]
    fn ax_destroyed_observed_state_parses_fixture_replies() {
        let reply = serde_json::json!({
            "v": 1,
            "request_id": "r",
            "ok": true,
            "result": {"op": "get", "ok": true, "windows": [{
                "id": "axdestroyed-b",
                "ok": true,
                "snapshot": {"observed": {"mode": "minimized", "visible": false}}
            }]}
        });
        assert_eq!(observed_mode(&reply, "axdestroyed-b"), Some("minimized"));
        assert_eq!(observed_visible(&reply, "axdestroyed-b"), Some(false));
        assert_eq!(observed_mode(&reply, "missing"), None);
        assert!(window_ok(&reply, "axdestroyed-b"));
        assert!(!window_ok(&reply, "missing"));
        assert!(reply_ok(&reply));
        let list = serde_json::json!({
            "v": 1, "request_id": "r", "ok": true,
            "result": {"op": "list", "ok": true, "ids": ["a", "b"]}
        });
        assert_eq!(list_ids(&list), Some(vec!["a".to_owned(), "b".to_owned()]));
        assert_eq!(list_ids(&serde_json::json!({"ok": false})), None);
    }
}
