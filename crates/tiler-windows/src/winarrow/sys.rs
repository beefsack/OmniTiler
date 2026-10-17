#![forbid(unsafe_code)]
use super::{
    Edge, HOOK_ENABLED_SECONDS, MethodReport, MethodState, RELEASE_CHECK_SECONDS, RestoreReport,
    SPIKE_HARD_SECONDS, SPIKE_LEDGER_SECONDS, SpikeReport, arrow_index, events_path_for,
    new_hook_method, parse_spike_run_args,
};
use crate::model::ProcessIdentity;
use crate::native::HeldProcess;
use crate::test_window::WindowSnapshot;
use std::cell::RefCell;
use std::fs::File;
use std::io::Write as _;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_CONTROL, VK_MENU, VK_SHIFT};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

type DynError = Box<dyn std::error::Error>;
type Result<T> = std::result::Result<T, DynError>;

fn err(msg: impl Into<String>) -> DynError {
    Box::new(std::io::Error::other(msg.into()))
}

const TICK_MS: u32 = 100;

struct HookState {
    helper_hwnd: u64,
    helper_pid: u32,
    helper_tag: u64,
    machine: super::HookClassify,
    queue: super::EventQueue,
}

thread_local! {
    static HOOK: RefCell<Option<HookState>> = const { RefCell::new(None) };
}

fn hook_foreground(st: &HookState) -> bool {
    if crate::ffi::foreground_window() != st.helper_hwnd {
        return false;
    }
    if crate::ffi::window_pid(st.helper_hwnd) != st.helper_pid {
        return false;
    }
    match crate::ffi::get_prop(st.helper_hwnd, "OmniTilerLifetime") {
        Some(v) => v as u64 == st.helper_tag,
        None => false,
    }
}

/// Minimal callback: modifier/chord bookkeeping plus one bounded queue
/// push. No logging, waits, file I/O, or geometry work here. Consumed
/// chords are swallowed; approved-but-passing chords are queued for the
/// log and passed through to the system. A physical Win-up that closes a
/// hold with a consumed chord sends the E8 mask pair first (with no HOOK
/// borrow held); the Win-up itself always passes through.
///
/// Safe policy behind the ffi trampoline: the event arrives pre-copied
/// with no raw pointer, and the verdict selects consume or pass-through.
/// The injected-event guard and the no-borrow-across-`SendInput` RefCell
/// ordering are unchanged from the previous inline callback.
fn handle_winarrow_event(ev: crate::ffi::KeyEvent) -> crate::ffi::HookVerdict {
    use crate::ffi::HookVerdict;
    {
        let vk = ev.vk;
        let flags = ev.flags;
        let injected = flags & (LLKHF_INJECTED | LLKHF_LOWER_IL_INJECTED) != 0;
        // Own-injection guard BEFORE any HOOK borrow: the SendInput pair
        // below reenters this callback on the same thread, and
        // re-borrowing the RefCell would panic. Injected events always
        // pass through untouched.
        if injected {
            return HookVerdict::Pass;
        }
        let is_up = flags & LLKHF_UP != 0;
        if is_up && super::is_win_vk(vk) {
            let tick = Instant::now();
            let fire = HOOK.with(|h| {
                let mut h = h.borrow_mut();
                let st = h.as_mut()?;
                let installed = st.machine.enabled;
                Some(super::win_up_mask_reserve(
                    &mut st.machine,
                    &mut st.queue,
                    vk,
                    installed,
                    tick,
                ))
            });
            if fire == Some(true) {
                let sent = send_mask_pair();
                HOOK.with(|h| {
                    if let Some(st) = h.borrow_mut().as_mut() {
                        super::stamp_mask_result(
                            &mut st.machine,
                            &mut st.queue,
                            sent.inserted,
                            sent.release_sent,
                        );
                    }
                });
            }
            return HookVerdict::Pass;
        }
        // Modifiers may already be held when the hook starts.
        if !is_up
            && super::Arrow::from_vk(vk).is_some()
            && [VK_CONTROL, VK_MENU, VK_SHIFT]
                .iter()
                .any(|key| crate::ffi::async_key_down(u32::from(*key)))
        {
            return HookVerdict::Pass;
        }
        let consume = HOOK.with(|h| {
            let mut h = h.borrow_mut();
            let st = h.as_mut()?;
            let fg = hook_foreground(st);
            let tick = Instant::now();
            super::classify_and_queue(
                &mut st.machine,
                &mut st.queue,
                vk,
                is_up,
                fg,
                injected,
                tick,
            )
        });
        if consume == Some(true) {
            return HookVerdict::Consume;
        }
    }
    HookVerdict::Pass
}

/// Sends the E8 down/up pair: two keyboard INPUT records with the
/// unassigned VK and no scancode masquerading as a Ctrl modifier. Must be
/// called with no HOOK borrow held (see `handle_winarrow_event`). Returns
/// the accepted-event count plus whether the bounded partial-release ran.
fn send_mask_pair() -> crate::ffi::KeyPairSend {
    crate::ffi::send_key_pair(super::VK_MASK as u16)
}

/// Per-event log writer. All filesystem serialization happens on the
/// message-loop thread; the callback only fills the bounded queue.
/// Schema per line: chord lines carry arrow/edge/foreground/disposition/
/// acted/result; mask lines carry the arming trigger arrow plus the
/// inserted count, release flag, and result (never raw Win VKs or other
/// keys); focus lines carry foreground only; phase lines carry the
/// phase name + foreground; drop lines carry the loss count. No HWNDs,
/// titles, or unrelated key data. Timestamps: the callback captures an
/// `Instant` tick; `mono_ms` is elapsed since the install anchor and
/// `wall` is wall-clock anchored at install (no clock reads in the
/// callback beyond the cheap monotonic tick).
struct EventLog {
    file: File,
    wall_start_ms: u64,
    tick_start: Instant,
    seq: u64,
}

impl EventLog {
    fn create(path: &Path) -> std::result::Result<Self, String> {
        if path.exists() {
            return Err("refuse: events log preexists".to_owned());
        }
        let tick_start = Instant::now();
        let wall_start_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .map_err(|e| format!("error: wall clock {e}"))?;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| format!("error: events create: {e}"))?;
        Ok(Self {
            file,
            wall_start_ms,
            tick_start,
            seq: 0,
        })
    }

    fn stamp(&self, tick: Instant) -> (String, u64) {
        let mono = tick.saturating_duration_since(self.tick_start).as_millis() as u64;
        let wall = self.wall_start_ms.saturating_add(mono);
        (format!("{}.{:03}Z", wall / 1000, wall % 1000), mono)
    }

    fn stamp_now(&self) -> (String, u64) {
        self.stamp(Instant::now())
    }

    fn emit(&mut self, value: serde_json::Value) -> std::result::Result<(), String> {
        let mut line =
            serde_json::to_string(&value).map_err(|e| format!("error: events json {e}"))?;
        line.push('\n');
        self.file
            .write_all(line.as_bytes())
            .map_err(|e| format!("error: events write: {e}"))?;
        self.seq += 1;
        Ok(())
    }

    fn flush(&mut self) -> std::result::Result<(), String> {
        self.file
            .flush()
            .map_err(|e| format!("error: events flush: {e}"))
    }

    fn phase(&mut self, name: &'static str, foreground: bool) -> std::result::Result<(), String> {
        let (wall, mono) = self.stamp_now();
        self.emit(serde_json::json!({
            "seq": self.seq, "wall": wall, "mono_ms": mono,
            "kind": "phase", "phase": name, "foreground": foreground,
        }))
    }

    fn focus(&mut self, foreground: bool) -> std::result::Result<(), String> {
        let (wall, mono) = self.stamp_now();
        self.emit(serde_json::json!({
            "seq": self.seq, "wall": wall, "mono_ms": mono,
            "kind": "focus", "foreground": foreground,
        }))
    }

    fn drop(&mut self, lost: u32) -> std::result::Result<(), String> {
        let (wall, mono) = self.stamp_now();
        self.emit(serde_json::json!({
            "seq": self.seq, "wall": wall, "mono_ms": mono,
            "kind": "drop", "lost": lost,
        }))
    }

    fn chord(
        &mut self,
        ev: super::QueuedChord,
        acted: bool,
        result: &str,
    ) -> std::result::Result<(), String> {
        let (wall, mono) = self.stamp(ev.tick);
        let disposition = if ev.consumed { "consumed" } else { "passed" };
        let edge = match ev.edge {
            Edge::Down => "down",
            Edge::Up => "up",
            Edge::Repeat => "repeat",
        };
        self.emit(serde_json::json!({
            "seq": self.seq, "wall": wall, "mono_ms": mono,
            "kind": "chord", "arrow": ev.arrow.name(), "edge": edge,
            "foreground": ev.foreground, "disposition": disposition,
            "acted": acted, "result": result,
        }))
    }

    fn mask(&mut self, ev: super::QueuedMask, result: &str) -> std::result::Result<(), String> {
        let (wall, mono) = self.stamp(ev.tick);
        self.emit(serde_json::json!({
            "seq": self.seq, "wall": wall, "mono_ms": mono,
            "kind": "mask", "trigger": ev.trigger.name(),
            "inserted": ev.inserted, "release_sent": ev.release_sent,
            "result": result,
        }))
    }
}

fn snapshot_owned(
    hwnd: u64,
    helper_exe: &str,
    sid: &str,
    session: u32,
) -> std::result::Result<WindowSnapshot, String> {
    crate::test_window::sys::query_owned(hwnd, helper_exe, sid, session).map_err(|e| e.to_string())
}

fn baseline_matches(base: &WindowSnapshot, snap: &WindowSnapshot) -> bool {
    snap.hwnd == base.hwnd && snap.process == base.process && snap.tag == base.tag
}

fn hold_helper(pid: u32, expected: &ProcessIdentity) -> std::result::Result<HeldProcess, String> {
    let held = HeldProcess::open(pid).map_err(|e| format!("refuse: helper open {e}"))?;
    let live = held
        .identity()
        .map_err(|e| format!("refuse: helper identity {e}"))?;
    if live != *expected || !held.is_alive() {
        return Err("refuse: helper identity changed".to_owned());
    }
    Ok(held)
}

fn foreground_exact(base: &WindowSnapshot) -> bool {
    crate::ffi::foreground_window() == base.hwnd
}

/// Best-effort status title on the verified helper only; skips silently on
/// any identity mismatch. Outside the callback, at most once per second.
/// Never logs title text.
fn set_helper_title(base: &WindowSnapshot, helper_exe: &str, text: &str) {
    let Ok(snap) = snapshot_owned(
        base.hwnd,
        helper_exe,
        &base.process.user_sid,
        base.process.session_id,
    ) else {
        return;
    };
    if !baseline_matches(base, &snap) {
        return;
    }
    let Ok(held) = hold_helper(snap.process.pid, &base.process) else {
        return;
    };
    if !held.is_alive() {
        return;
    }
    crate::ffi::set_window_text(base.hwnd, text);
}

/// Single owned-geometry writer for nudge and final restore.
/// Pre-queries + holds identity, optionally restores normal state,
/// moves without activation, then verifies exact rect + identity + tag.
fn set_owned_rect(
    base: &WindowSnapshot,
    helper_exe: &str,
    want: [i32; 4],
    restoring: bool,
) -> std::result::Result<[i32; 4], String> {
    let pre = snapshot_owned(
        base.hwnd,
        helper_exe,
        &base.process.user_sid,
        base.process.session_id,
    )?;
    if !baseline_matches(base, &pre) {
        return Err("refuse: helper identity changed".to_owned());
    }
    let held = hold_helper(pre.process.pid, &base.process)?;
    if restoring && (crate::ffi::is_iconic(base.hwnd) || crate::ffi::is_zoomed(base.hwnd)) {
        crate::ffi::show_window(base.hwnd, SW_SHOWNOACTIVATE);
        if crate::ffi::is_iconic(base.hwnd) || crate::ffi::is_zoomed(base.hwnd) {
            return Err("refuse: helper still minimized/maximized".to_owned());
        }
    }
    let (ok, code) = crate::ffi::set_window_pos_detailed(
        base.hwnd,
        want[0],
        want[1],
        want[2] - want[0],
        want[3] - want[1],
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
    if !ok {
        return Err(format!("error: SetWindowPos win32-{code}"));
    }
    if !held.is_alive() {
        return Err("refuse: helper exited during move".to_owned());
    }
    let back = snapshot_owned(
        base.hwnd,
        helper_exe,
        &base.process.user_sid,
        base.process.session_id,
    )?;
    if !baseline_matches(base, &back) {
        return Err("refuse: helper identity changed".to_owned());
    }
    let got = [back.left, back.top, back.right, back.bottom];
    if got != want {
        return Err(format!("refuse: readback {got:?} != requested {want:?}"));
    }
    Ok(got)
}

fn nudge(
    base: &WindowSnapshot,
    helper_exe: &str,
    arrow: super::Arrow,
) -> std::result::Result<([i32; 4], [i32; 4]), String> {
    let w = base.right - base.left;
    let h = base.bottom - base.top;
    let (dx, dy) = arrow.offset();
    let want = [
        base.left + dx,
        base.top + dy,
        base.left + dx + w,
        base.top + dy + h,
    ];
    let got = set_owned_rect(base, helper_exe, want, false)?;
    Ok((want, got))
}

fn write_report(path: &Path, report: &SpikeReport) -> std::result::Result<(), String> {
    let json = serde_json::to_string(report).map_err(|e| format!("error: report json {e}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json.as_bytes()).map_err(|e| format!("error: report write: {e}"))?;
    if let Err(e) = crate::ffi::move_file_replace(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        let code = e.raw_os_error().unwrap_or(0);
        return Err(format!("error: report replace win32-{code}"));
    }
    Ok(())
}

fn build_report(
    owner: &ProcessIdentity,
    helper: &WindowSnapshot,
    method: &MethodReport,
    restore: &RestoreReport,
    exit: &str,
    exit_error: &Option<String>,
) -> SpikeReport {
    SpikeReport {
        schema: 3,
        owner: owner.clone(),
        helper: helper.clone(),
        focus_note: super::FOCUS_NOTE,
        method: method.clone(),
        restore: restore.clone(),
        exit: exit.to_owned(),
        exit_error: exit_error.clone(),
    }
}

fn stop_requested(dir: &Path, owner_creation: &str) -> std::result::Result<bool, String> {
    match std::fs::read_to_string(dir.join(crate::lifecycle::STOP_REQUEST_FILE)) {
        Ok(text) => Ok(crate::lifecycle::stop_request_matches(
            &text,
            owner_creation,
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("error: stop read: {e}")),
    }
}

fn pull_hook_counts(method: &mut MethodReport) {
    let Some((counts, dropped, mask)) = HOOK.with(|h| {
        h.borrow().as_ref().map(|s| {
            (
                s.machine.chords,
                s.queue.dropped,
                (
                    s.machine.mask_attempted,
                    s.machine.mask_ok,
                    s.machine.mask_failed,
                    s.machine.mask_skipped,
                ),
            )
        })
    }) else {
        return;
    };
    for (i, c) in counts.iter().enumerate() {
        let r = &mut method.chords[i];
        r.down = c.down;
        r.up = c.up;
        r.repeat = c.repeat;
        // Truthful hook delivery: chord downs plus repeats; key-ups stay
        // in `up` only. `skipped` is owned by the drain (foreground lost
        // between consume and act) and is never overwritten here.
        r.delivered = c.down.saturating_add(c.repeat);
        r.consumed = c.consumed;
        r.acted = c.acted;
        r.passed = c.passed;
    }
    if dropped > method.queue_dropped {
        method.queue_dropped = dropped;
    }
    method.mask_attempted = mask.0;
    method.mask_ok = mask.1;
    method.mask_failed = mask.2;
    method.mask_skipped = mask.3;
}

fn release_now(
    hook: &mut Option<crate::ffi::WinArrowHook>,
    live: &mut bool,
    method: &mut MethodReport,
) -> std::result::Result<(), String> {
    if !*live {
        return Ok(());
    }
    *live = false;
    HOOK.with(|h| {
        if let Some(s) = h.borrow_mut().as_mut() {
            s.machine.set_enabled(false);
        }
    });
    pull_hook_counts(method);
    // `live` implies an installed handle (both set together), so `take`
    // yields `Some` on every reachable path; `None` keeps the failure
    // vocabulary instead of fabricating success.
    let taken = hook.take();
    let ok = match taken {
        Some(mut installed) => {
            let ok = crate::ffi::uninstall_winarrow_hook(&mut installed);
            if !ok {
                *hook = Some(installed);
            }
            ok
        }
        None => false,
    };
    if ok {
        crate::ffi::clear_winarrow_sink();
        HOOK.with(|h| *h.borrow_mut() = None);
        method.release_ok = true;
        method.release_error = None;
        Ok(())
    } else {
        let e = format!("win32-{}", crate::ffi::process::last_error());
        method.release_ok = false;
        method.release_error = Some(e.clone());
        Err(e)
    }
}

/// Drain the bounded queue on the message-loop thread: act on consumed
/// downs with a foreground recheck, then serialize every approved event to
/// the log. Saturation losses are emitted explicitly as drop lines.
fn drain_queue(
    base: &WindowSnapshot,
    helper_exe: &str,
    method: &mut MethodReport,
    log: &mut EventLog,
) -> std::result::Result<(), String> {
    let dropped = HOOK.with(|h| h.borrow().as_ref().map(|s| s.queue.dropped).unwrap_or(0));
    if dropped > method.queue_dropped {
        log.drop(dropped - method.queue_dropped)?;
        method.queue_dropped = dropped;
        method.events_logged += 1;
    }
    loop {
        let ev = HOOK.with(|h| h.borrow_mut().as_mut().and_then(|s| s.queue.pop_front()));
        let Some(ev) = ev else { break };
        match ev {
            super::QueuedEvent::Mask(m) => {
                let result = if m.inserted == 2 && !m.release_sent {
                    "mask-ok".to_owned()
                } else {
                    format!("mask-failed-{}", m.inserted)
                };
                log.mask(m, &result)?;
                method.events_logged += 1;
            }
            super::QueuedEvent::Chord(ch) => {
                let idx = arrow_index(ch.arrow);
                let (acted, result) = if ch.consumed && ch.announce {
                    if !foreground_exact(base) {
                        method.chords[idx].skipped += 1;
                        (false, "recheck-background".to_owned())
                    } else {
                        match nudge(base, helper_exe, ch.arrow) {
                            Ok((want, got)) => {
                                HOOK.with(|h| {
                                    if let Some(s) = h.borrow_mut().as_mut() {
                                        s.machine.note_acted(ch.arrow);
                                    }
                                });
                                method.chords[idx].last_requested = Some(want);
                                method.chords[idx].last_observed = Some(got);
                                (true, "nudge-ok".to_owned())
                            }
                            Err(e) => {
                                let detail = format!("action-error: {e}");
                                log.chord(ch, false, &detail)?;
                                method.events_logged += 1;
                                pull_hook_counts(method);
                                log.flush()?;
                                return Err(e);
                            }
                        }
                    }
                } else if ch.consumed {
                    (false, "consumed-up".to_owned())
                } else {
                    (false, "passed".to_owned())
                };
                log.chord(ch, acted, &result)?;
                method.events_logged += 1;
            }
        }
    }
    pull_hook_counts(method);
    log.flush()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Enabled,
    ReleaseCheck,
}

fn is_helper_gone(e: &str) -> bool {
    e.contains("helper exited")
        || e.contains("helper pid")
        || e.contains("helper identity changed")
        || e.contains("helper open")
        || e.contains("helper identity")
}

fn spike_body(dir: &Path, me: &ProcessIdentity, helper_hwnd: u64, report: &Path) -> Result<bool> {
    let helper_exe =
        crate::test_window::sys::sibling_helper_exe().map_err(|e| err(e.to_string()))?;
    let base = snapshot_owned(helper_hwnd, &helper_exe, &me.user_sid, me.session_id)
        .map_err(|e| err(format!("refuse: helper {e}")))?;
    if !base.visible {
        return Err(err("refuse: helper not visible"));
    }
    if crate::ffi::is_iconic(base.hwnd) {
        return Err(err("refuse: helper minimized"));
    }
    if crate::ffi::is_zoomed(base.hwnd) {
        return Err(err("refuse: helper maximized"));
    }
    if report.exists() {
        return Err(err("refuse: report preexists"));
    }
    let Some(parent) = report.parent() else {
        return Err(err("error: report no parent"));
    };
    if !parent.is_dir() {
        return Err(err("error: report parent missing"));
    }
    let events_path = events_path_for(report).ok_or_else(|| err("error: report no parent"))?;
    let helper_hold = hold_helper(base.process.pid, &base.process).map_err(err)?;
    let mut method = new_hook_method();
    method.events_path = Some(events_path.to_string_lossy().into_owned());
    let mut restore = RestoreReport::default();
    let mut exit = "started".to_owned();
    let mut exit_error: Option<String> = None;
    let mut stopped = false;

    let snap = |m: &MethodReport, r: &RestoreReport, e: &str, ee: &Option<String>| {
        write_report(report, &build_report(me, &base, m, r, e, ee))
    };
    snap(&method, &restore, &exit, &exit_error).map_err(err)?;

    // Prime the thread queue so the loop can run, then a single 100ms
    // thread timer for stop/deadline/drain checks plus a 1s sub-tick for
    // the status title and focus-change evidence.
    crate::ffi::prime_thread_queue();
    let timer_id = crate::ffi::set_thread_timer(TICK_MS);
    if timer_id == 0 {
        return Err(err("error: SetTimer failed"));
    }

    // Install once with no focus gate. Timing-only fallback when the tag
    // is bad or the install fails; the report says so honestly.
    let mut hook: Option<crate::ffi::WinArrowHook> = None;
    let mut live = false;
    let mut released = false;
    let tag_num = u64::from_str_radix(base.tag.trim(), 16).unwrap_or(0);
    if tag_num == 0 {
        method.hook_installed = Some(false);
        method.note.push_str("bad helper tag; timing only. ");
        method.release_ok = true;
        released = true;
    } else {
        HOOK.with(|h| {
            *h.borrow_mut() = Some(HookState {
                helper_hwnd: base.hwnd,
                helper_pid: base.process.pid,
                helper_tag: tag_num,
                machine: super::HookClassify::new(true),
                queue: super::EventQueue::new(),
            });
        });
        crate::ffi::set_winarrow_sink(handle_winarrow_event);
        match crate::ffi::install_winarrow_hook() {
            None => {
                crate::ffi::clear_winarrow_sink();
                let code = crate::ffi::process::last_error();
                method.hook_installed = Some(false);
                method.hook_error = Some(code);
                method
                    .note
                    .push_str(&format!("hook install failed win32-{code}. "));
                method.release_ok = true;
                released = true;
                HOOK.with(|h| *h.borrow_mut() = None);
            }
            Some(h) => {
                hook = Some(h);
                live = true;
                method.hook_installed = Some(true);
                method.note.push_str("hook installed. ");
            }
        }
    }
    let mut log = EventLog::create(&events_path).map_err(|e| {
        if live && !released {
            let mut m = method.clone();
            let mut l = live;
            let _ = release_now(&mut hook, &mut l, &mut m);
        }
        crate::ffi::kill_thread_timer(timer_id);
        HOOK.with(|h| *h.borrow_mut() = None);
        err(e)
    })?;
    method.state = MethodState::Enabled;
    let fg0 = foreground_exact(&base);
    // Honest phase: the fallback never installs, so it must not claim
    // hook-on. The countdown title is live-only.
    let available = live;
    log.phase(
        if available {
            "hook-on"
        } else {
            "hook-unavailable"
        },
        fg0,
    )
    .map_err(|e| {
        if live && !released {
            let mut m = method.clone();
            let mut l = live;
            let _ = release_now(&mut hook, &mut l, &mut m);
        }
        crate::ffi::kill_thread_timer(timer_id);
        HOOK.with(|h| *h.borrow_mut() = None);
        err(e)
    })?;
    method.events_logged += 1;
    if available {
        set_helper_title(
            &base,
            &helper_exe,
            &format!("HOOK ON {HOOK_ENABLED_SECONDS}s - tap Win+Up/Left/Right/Down"),
        );
    } else {
        set_helper_title(&base, &helper_exe, "HOOK UNAVAILABLE - leave keys alone");
    }
    snap(&method, &restore, &exit, &exit_error).map_err(|e| {
        if live && !released {
            let mut m = method.clone();
            let mut l = live;
            let _ = release_now(&mut hook, &mut l, &mut m);
        }
        crate::ffi::kill_thread_timer(timer_id);
        HOOK.with(|h| *h.borrow_mut() = None);
        err(e)
    })?;

    // Fixed 60s installed (focus changes never uninstall) then one ~20s
    // release check with the hook removed.
    let hard = Instant::now() + Duration::from_secs(SPIKE_HARD_SECONDS);
    let mut phase = Phase::Enabled;
    let mut ends_at = Instant::now() + Duration::from_secs(HOOK_ENABLED_SECONDS);
    let mut last_fg = Some(fg0);
    let mut last_tick = Instant::now();
    let loop_out: std::result::Result<bool, String> = (|| {
        loop {
            match crate::ffi::wait_timer_message() {
                crate::ffi::TimerMessage::Quit => {
                    exit = "interrupted".to_owned();
                    break;
                }
                crate::ffi::TimerMessage::Failed => {
                    return Err("error: message loop".to_owned());
                }
                crate::ffi::TimerMessage::Tick => {}
            }
            if stop_requested(dir, &me.process_creation)? {
                stopped = true;
                exit = "stop-request".to_owned();
                break;
            }
            if Instant::now() >= hard {
                exit = "interrupted".to_owned();
                break;
            }
            if !helper_hold.is_alive() {
                return Err("refuse: helper exited".to_owned());
            }
            match snapshot_owned(base.hwnd, &helper_exe, &me.user_sid, me.session_id) {
                Ok(s) if baseline_matches(&base, &s) => {}
                Ok(_) => {
                    return Err("refuse: helper identity changed".to_owned());
                }
                Err(e) => return Err(e),
            }
            drain_queue(&base, &helper_exe, &mut method, &mut log)?;
            if last_tick.elapsed() >= Duration::from_secs(1) {
                last_tick = Instant::now();
                let fg = foreground_exact(&base);
                if last_fg != Some(fg) {
                    log.focus(fg)?;
                    method.events_logged += 1;
                    last_fg = Some(fg);
                }
                if phase == Phase::Enabled && available {
                    let remain = ends_at.saturating_duration_since(Instant::now()).as_secs();
                    set_helper_title(
                        &base,
                        &helper_exe,
                        &format!("HOOK ON {remain}s - tap Win+Up/Left/Right/Down"),
                    );
                }
            }
            if Instant::now() >= ends_at {
                match phase {
                    Phase::Enabled => {
                        if !released {
                            if let Err(e) = release_now(&mut hook, &mut live, &mut method) {
                                exit = "release-failed".to_owned();
                                return Err(format!("error: release {e}"));
                            }
                            released = true;
                        }
                        drain_queue(&base, &helper_exe, &mut method, &mut log)?;
                        let fg = foreground_exact(&base);
                        // A hook that never installed has nothing to release;
                        // keep the neutral title instead of claiming HOOK OFF.
                        if available {
                            log.phase("hook-off", fg)?;
                            set_helper_title(&base, &helper_exe, "HOOK OFF - tap Win+Left once");
                        } else {
                            log.phase("still-unavailable", fg)?;
                            set_helper_title(
                                &base,
                                &helper_exe,
                                "HOOK UNAVAILABLE - leave keys alone",
                            );
                        }
                        method.events_logged += 1;
                        phase = Phase::ReleaseCheck;
                        method.state = MethodState::ReleaseCheck;
                        ends_at = Instant::now() + Duration::from_secs(RELEASE_CHECK_SECONDS);
                        snap(&method, &restore, &exit, &exit_error)?;
                    }
                    Phase::ReleaseCheck => {
                        method.state = MethodState::Done;
                        set_helper_title(&base, &helper_exe, "DONE");
                        let fg = foreground_exact(&base);
                        log.phase("done", fg)?;
                        method.events_logged += 1;
                        if exit == "started" {
                            exit = if stopped {
                                "stop-request".to_owned()
                            } else {
                                "completed".to_owned()
                            };
                        }
                        break;
                    }
                }
            }
        }
        Ok(stopped)
    })();

    let loop_err: Option<String> = match loop_out {
        Ok(s) => {
            stopped = s;
            None
        }
        Err(e) => Some(e),
    };

    // Cleanup always runs once: single release, exact restore, final report.
    // Release failures are never overwritten; cleanup failures combine
    // with the loop error instead of console-only logging.
    if !released && live {
        if let Err(e) = release_now(&mut hook, &mut live, &mut method) {
            if exit_error.is_none() {
                exit_error = Some(format!("error: release {e}"));
            } else {
                let prev = exit_error.take().unwrap_or_default();
                exit_error = Some(format!("{prev} ; release {e}"));
            }
            if exit == "started" {
                exit = "release-failed".to_owned();
            }
        }
        released = true;
    }
    pull_hook_counts(&mut method);
    if method.state != MethodState::Done {
        method.state = MethodState::Done;
    }
    // Final DONE title precedes the exact restore so the cue is visible
    // until the helper closes.
    set_helper_title(&base, &helper_exe, "DONE");
    match set_owned_rect(
        &base,
        &helper_exe,
        [base.left, base.top, base.right, base.bottom],
        true,
    ) {
        Ok(rect) => {
            restore.ok = Some(true);
            restore.readback = Some(rect);
        }
        Err(e) => {
            restore.ok = Some(false);
            restore.error = Some(e.clone());
            if exit_error.is_none() {
                exit_error = Some(format!("error: restore {e}"));
            } else {
                let prev = exit_error.take().unwrap_or_default();
                exit_error = Some(format!("{prev} ; restore {e}"));
            }
            if exit == "started" || exit == "completed" {
                exit = "restore-refused".to_owned();
            }
        }
    }
    if exit == "started" {
        exit = if stopped {
            "stop-request".to_owned()
        } else {
            "completed".to_owned()
        };
    }
    if exit_error.is_none() {
        exit_error = loop_err.clone();
        if let Some(e) = loop_err.clone()
            && exit == "started"
            && is_helper_gone(&e)
        {
            exit = "helper-exited".to_owned();
        }
    } else if let Some(loop_e) = loop_err.clone() {
        // Preserve both: loop error plus cleanup detail is already merged
        // above for release/restore; if loop error is distinct, combine.
        let prev = exit_error.take().unwrap_or_default();
        if !prev.contains(&loop_e) {
            exit_error = Some(format!("{prev} ; {loop_e}"));
        } else {
            exit_error = Some(prev);
        }
        if exit == "started" && is_helper_gone(&loop_e) {
            exit = "helper-exited".to_owned();
        }
    }
    let _ = log.flush();
    crate::ffi::kill_thread_timer(timer_id);
    HOOK.with(|h| *h.borrow_mut() = None);
    let _ = released;
    if let Err(e) = write_report(
        report,
        &build_report(me, &base, &method, &restore, &exit, &exit_error),
    ) {
        return Err(err(e));
    }
    if let Some(e) = exit_error {
        // Release errors are retained; never overwritten by a generic error.
        return Err(err(format!("error: spike {e}")));
    }
    Ok(stopped)
}

pub fn run_spike(helper_hwnd: u64, report: &Path) -> Result<String> {
    let report = report.to_owned();
    crate::lifecycle::sys::run_with_callback(
        SPIKE_LEDGER_SECONDS,
        false,
        None,
        move |dir, me, _store| spike_body(dir, me, helper_hwnd, &report),
    )
}

pub fn parse_and_run(args: &[String]) -> std::result::Result<String, (i32, String)> {
    let opts = parse_spike_run_args(args).map_err(|m| (2, m))?;
    run_spike(opts.helper_hwnd, &opts.report).map_err(|e| (1, e.to_string()))
}
