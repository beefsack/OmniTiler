use super::*;

// Win32 object id for window-scoped events (filters out menu/scrollbar noise).
const OBJID_WINDOW: i32 = 0;

// Hook delivery: out-of-context callbacks run on the installing (loop) thread
// while it pumps messages, so the callback and the loop share one queue lock
// with a strict invariant - the loop never holds it across a message pump -
// and the callback takes it blocking (never try_lock-and-drop, lossless).
#[derive(Debug, Clone, Copy)]
pub(super) enum HookEvent {
    Wake(isize),
    /// Esc sequence captured at WinEvent delivery time in the event
    /// callback, carried to START. The pump only carries it to the START
    /// snapshot and never re-reads it later, so an Esc that arrives
    /// between the real START and the owner drain still latches (mirrors
    /// the KDE Finish pointer on MoveSizeEnd).
    MoveSizeStart(isize, u64),
    /// Release cursor captured at WinEvent delivery time in the event
    /// callback, carried to settle. `None` when the read failed: settle
    /// routes cursor-less moves as no-change. Captured here (the KDE Finish
    /// pointer), never re-read later on the pump. The END-time Esc sequence
    /// rides along so a physical Esc AFTER the END (before the owner drain)
    /// can never cancel the completed drop: settle latches ended gestures
    /// only when the END snapshot differs from START.
    MoveSizeEnd(isize, Option<(i32, i32)>, u64),
}

pub(super) fn hook_queue() -> &'static Mutex<Vec<HookEvent>> {
    static QUEUE: OnceLock<Mutex<Vec<HookEvent>>> = OnceLock::new();
    QUEUE.get_or_init(|| Mutex::new(Vec::new()))
}

fn push_hook(event: HookEvent) {
    if let Ok(mut queue) = hook_queue().lock() {
        queue.push(event);
    }
}

pub(super) unsafe extern "system" fn winevent_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if event == EVENT_SYSTEM_MOVESIZESTART {
        // Event-time Esc sequence sampled directly from the shared atomic
        // clock, so a physical Esc between the real START and the owner
        // drain keeps a newer sequence than the snapshot and still latches.
        // Delayed batch sequences never decide this comparison.
        push_hook(HookEvent::MoveSizeStart(
            hwnd as isize,
            crate::snapkey::sys::shared_esc_seq(),
        ));
    } else if event == EVENT_SYSTEM_MOVESIZEEND {
        // KDE Finish pointer: capture the release cursor now, at event
        // time in the callback. The pump only carries it to settle and
        // never re-reads it later. The END-time Esc sequence is captured
        // alongside so post-END edges order correctly at the drain.
        push_hook(HookEvent::MoveSizeEnd(
            hwnd as isize,
            cursor_pos(),
            crate::snapkey::sys::shared_esc_seq(),
        ));
    } else if id_object == OBJID_WINDOW
        && matches!(
            event,
            EVENT_SYSTEM_FOREGROUND
                | EVENT_OBJECT_CREATE
                | EVENT_OBJECT_DESTROY
                | EVENT_OBJECT_SHOW
                | EVENT_OBJECT_HIDE
                | EVENT_OBJECT_LOCATIONCHANGE
        )
    {
        push_hook(HookEvent::Wake(hwnd as isize));
    }
}

/// Product outer wait: pump loop-thread messages (delivering hook callbacks)
/// then wait for more plus the input-pump intent wake and the renderer ack
/// wake. Returns what woke the wait so the loop can drain an ack-only wake
/// without reposting frames, reconciling, or polling settings: renderer acks
/// wake the owner immediately with no polling latency, but an ack alone
/// never triggers owner work by itself, so the ack wake cannot feed back
/// into a post/ack/post busy loop. A pending wake (offered before this wait)
/// returns immediately; bursts coalesce to one wake; the timeout keeps the
/// periodic follow cadence (timed native reassert included). One thread owns
/// install, pump, and uninstall: no join, no hang.
pub(super) fn pump_wait_with_intent(
    timeout_ms: u32,
    wake: Option<&OwnedHandle>,
    ack_wake: Option<&OwnedHandle>,
) -> WaitWake {
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    loop {
        let has = unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) };
        if has == 0 {
            break;
        }
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    let mut woke = WaitWake {
        input: false,
        ack: false,
        message: false,
        timeout: false,
        failed: false,
    };
    unsafe {
        let mut handles = [std::ptr::null_mut(); 2];
        let mut count: u32 = 0;
        if let Some(handle) = wake {
            handles[0] = handle.as_handle().as_raw_handle();
            count = 1;
        }
        if let Some(handle) = ack_wake {
            handles[count as usize] = handle.as_handle().as_raw_handle();
            count += 1;
        }
        let result = if count == 0 {
            MsgWaitForMultipleObjectsEx(0, std::ptr::null(), timeout_ms, QS_ALLINPUT, 0)
        } else {
            MsgWaitForMultipleObjectsEx(count, handles.as_ptr(), timeout_ms, QS_ALLINPUT, 0)
        };
        const WAIT_OBJECT_0: u32 = 0;
        const WAIT_TIMEOUT: u32 = 258;
        if result == WAIT_TIMEOUT {
            woke.timeout = true;
        } else if result < WAIT_OBJECT_0 + count {
            // A satisfied wait object: index 0 is the input wake when
            // present, else the ack wake; index 1 is the ack wake.
            let has_input = wake.is_some();
            if result == WAIT_OBJECT_0 && has_input {
                woke.input = true;
            } else {
                woke.ack = true;
            }
        } else if result == WAIT_OBJECT_0 + count {
            woke.message = true;
        } else {
            // WAIT_FAILED or abandoned: counted separately as `failed`, never
            // merged into `timeout` (genuine 258 only). Control flow keeps the
            // existing fallback: a failed return takes the periodic-work path
            // below like before, with no new recovery and no timing claim (a
            // failed wait returns immediately; not proven delayed).
            woke.failed = true;
        }
    }
    loop {
        let has = unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) };
        if has == 0 {
            break;
        }
        // Dispatched callbacks below may queue hook events: once any
        // message flows, this turn does full owner work even when the wait
        // itself returned an ack or timeout reason.
        woke.message = true;
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    woke
}

/// What woke the product outer wait: any combination may hold (an ack plus
/// queued messages is common). `timeout` is the genuine deadline expiry
/// (`WAIT_TIMEOUT` 258) only; `failed` is `WAIT_FAILED`/abandoned, counted
/// separately and never merged into timeout. Ack-only turns drain acks and
/// service the pending finish without reposting, reconciling, or
/// settings/input work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct WaitWake {
    pub(super) input: bool,
    pub(super) ack: bool,
    pub(super) message: bool,
    pub(super) timeout: bool,
    pub(super) failed: bool,
}

impl WaitWake {
    /// Full owner turn: fresh input, real messages, the genuine periodic
    /// timeout (which preserves the timed follow/reassert cadence), or the
    /// pre-existing failed-wait fallback path (unchanged control flow; only
    /// the counter differs). Hook-callback events collected after the wait
    /// also force a full turn at the call site, so no input starves behind
    /// ack traffic.
    #[must_use]
    pub(super) const fn full_turn(self) -> bool {
        self.input || self.message || self.timeout || self.failed
    }
}

/// Trace-only outer-wait return counts (outer product wait only; the
/// focus/proof `pump_wait` path never constructs one of these). Reason hits
/// count set `WaitWake` flags per return, so combinations (ack plus messages
/// is common) can exceed `waits`; `timeout` counts genuine 258 expiries only
/// while `failed` counts `WAIT_FAILED`/abandoned separately; `ack_only`
/// counts the early-return subset.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct OwnerWaitCounts {
    pub(super) waits: u64,
    pub(super) ack_only: u64,
    pub(super) input: u64,
    pub(super) ack: u64,
    pub(super) msg: u64,
    pub(super) timeout: u64,
    pub(super) failed: u64,
}

impl OwnerWaitCounts {
    /// One outer wait return: one `waits` plus one hit per set reason flag.
    pub(super) fn note_wait(&mut self, wait: WaitWake) {
        self.waits = self.waits.saturating_add(1);
        if wait.input {
            self.input = self.input.saturating_add(1);
        }
        if wait.ack {
            self.ack = self.ack.saturating_add(1);
        }
        if wait.message {
            self.msg = self.msg.saturating_add(1);
        }
        if wait.timeout {
            self.timeout = self.timeout.saturating_add(1);
        }
        if wait.failed {
            self.failed = self.failed.saturating_add(1);
        }
    }

    /// One ack-only early return (a subset of `waits`, never a full turn).
    pub(super) fn note_ack_only(&mut self) {
        self.ack_only = self.ack_only.saturating_add(1);
    }
}

/// Message-only wait for the focus-settle and proof paths: the same pump as
/// [`pump_wait_with_intent`] with no intent or ack wake, so those paths never
/// consume or spin on the owner wakes. The recursive focus-settle pump
/// deliberately excludes the ack wake: ack accounting runs on the outer
/// loop, never inside nested settlement. These nested waits never touch the
/// outer `ow_*` wait counters below: only the product outer wait returns
/// count, so the trace summary distinguishes outer returns from
/// focus/proof waits by construction.
pub(super) fn pump_wait(timeout_ms: u32) {
    pump_wait_with_intent(timeout_ms, None, None);
}

#[derive(Debug, Clone)]
pub(super) struct ObservedWindow {
    pub(super) hwnd: u64,
    pub(super) token: String,
    /// Native outer rectangle from the same `GetWindowRect` read the frame
    /// insets were measured against: one coherent observe pair, never a
    /// second torn read.
    pub(super) outer: Rect,
    pub(super) visible: Rect,
    pub(super) insets: FrameInsets,
    pub(super) identity: ObservedTarget,
    pub(super) facts: WindowFacts,
}

pub(super) fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if n <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..n as usize])
}

pub(super) fn window_identity(
    hwnd: HWND,
    hwnd_u64: u64,
    me: &ProcessIdentity,
) -> Option<ObservedTarget> {
    let mut pid: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
    }
    // The owner never tiles its own process. Its separate-process Settings
    // UI (same executable, settings class) is excluded at the dialog gate
    // below, not here.
    if pid == 0 || pid == me.pid {
        return None;
    }
    let held = HeldProcess::open(pid).ok()?;
    let ident = held.identity().ok()?;
    if ident.pid != pid {
        return None;
    }
    if ident.user_sid != me.user_sid || ident.session_id != me.session_id {
        return None;
    }
    let rid = held.integrity().ok()?;
    if !is_medium_rid(rid) {
        return None;
    }
    Some(ObservedTarget {
        hwnd: hwnd_u64,
        pid,
        process_creation: ident.process_creation,
        exe_path: ident.exe_path,
        user_sid: ident.user_sid,
        session_id: ident.session_id,
        tag: owned_tag(hwnd),
    })
}

/// Fresh foreground lifetime for renderer-posted frames: pid/creation plus
/// the visible-membership tag, or `None` when the foreground is missing,
/// ours, or unreadable. The renderer refuses narrowly on `None` per the
/// binding contract (no unprovable stale show); readable unchanged
/// foregrounds keep ordinary unfocused-hold behavior. The target/mover
/// fence still applies independently, and the owner re-fences on its next
/// refresh.
pub(super) fn foreground_lifetime(me: &ProcessIdentity) -> Option<WindowLifetime> {
    let foreground = unsafe { GetForegroundWindow() } as usize as u64;
    if foreground == 0 {
        return None;
    }
    let identity = window_identity(foreground as isize as HWND, foreground, me)?;
    Some(WindowLifetime {
        pid: identity.pid,
        creation: identity.process_creation,
        member_tag: crate::product_hide::sys::read_member_tag(foreground),
    })
}

/// Owned-helper lifetime tag (`OmniTilerLifetime` property). Empty when
/// absent: generic windows never carry it, so proof allowlists requiring a
/// nonempty tag reject them before any write.
fn owned_tag(hwnd: HWND) -> String {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetPropW;
    let name: Vec<u16> = crate::test_window::TEST_WINDOW_PROP
        .encode_utf16()
        .chain([0])
        .collect();
    let v = unsafe { GetPropW(hwnd, name.as_ptr()) };
    if v.is_null() {
        return String::new();
    }
    let token = v as usize as u64;
    if token == 0 {
        return String::new();
    }
    format!("{token:016x}")
}

/// Proof-only owned-helper verification: sibling executable, owned class,
/// lifetime tag, full process identity, and medium integrity via
/// [`crate::test_window::sys::query_owned`]. Generic windows (including
/// otherwise eligible ordinary apps) fail here even when their rectangles
/// look tileable.
pub(super) fn verify_proof_owned(
    hwnd_u64: u64,
    expected: &AllowEntry,
    me: &ProcessIdentity,
) -> std::result::Result<(), &'static str> {
    let helper_exe =
        crate::test_window::sys::sibling_helper_exe().map_err(|_| "identity-changed")?;
    let snap =
        crate::test_window::sys::query_owned(hwnd_u64, &helper_exe, &me.user_sid, me.session_id)
            .map_err(|_| "identity-changed")?;
    if snap.tag != expected.tag || snap.tag.is_empty() {
        return Err("identity-changed");
    }
    if snap.process.pid != expected.pid
        || snap.process.process_creation != expected.process_creation
        || snap.process.user_sid != expected.user_sid
        || snap.process.session_id != expected.session_id
    {
        return Err("identity-changed");
    }
    if !crate::lifecycle::exe_paths_equal(&snap.process.exe_path, &expected.exe_path) {
        return Err("identity-changed");
    }
    Ok(())
}

/// Identity plus distinct window state known after identity resolution but
/// before any frame query: the stable token and the full frozen-matchable
/// identity survive even when DWM has no frame to read.
#[derive(Debug, Clone)]
pub struct KnownIdentity {
    pub identity: ObservedTarget,
    pub token: String,
}

/// Failure modes of [`observe_window`]. Identity and state stay available
/// independently of geometry: a minimized (or otherwise frameless) window
/// reports its known identity with an accurate reason instead of masquerading
/// as a changed identity, and its token stays stable across ticks.
#[derive(Debug, Clone)]
pub enum ObserveFailure {
    /// Identity never resolved (invisible, vanished, foreign session/SID,
    /// elevated, or unreadable owner): a true unknown, counted in aggregate.
    Unknown,
    /// Identity resolved: retain it, keep its token stable, and skip geometry
    /// with this reason. Boxed: the identity is large and only travels the
    /// cold failure path. `Minimized` is decided from window state before any
    /// frame query; `Unreadable` means the frames failed after identity.
    Known(Box<KnownIdentity>, SkipReason),
}

/// Observe one window fully: identity/state first, geometry second. `IsIconic`
/// is read before any frame query because iconic windows have no DWM frame;
/// a minimized window returns its known identity with `Minimized` rather than
/// an identity error. DWM cloak-query failure fails closed as `Unreadable`,
/// never as eligible. Fullscreen uses whole `rcMonitor` bounds via the
/// portable predicate: captionless covering any monitor's full rect, never
/// dimensions alone.
pub(super) fn observe_window(
    hwnd: HWND,
    me: &ProcessIdentity,
    fulls: &[Rect],
    tokens: &mut TokenMap,
) -> std::result::Result<ObservedWindow, ObserveFailure> {
    use ObserveFailure::{Known, Unknown};
    if unsafe { IsWindowVisible(hwnd) } == 0 {
        return Err(Unknown);
    }
    let hwnd_u64 = hwnd as usize as u64;
    let identity = window_identity(hwnd, hwnd_u64, me).ok_or(Unknown)?;
    let token = tokens.token_for(hwnd_u64, &identity.process_creation);
    // Token clones below are per-failure (cold paths); the final use moves.
    let known = |reason| {
        Known(
            Box::new(KnownIdentity {
                identity: identity.clone(),
                token: token.clone(),
            }),
            reason,
        )
    };
    // Window state before frames: an iconic window has no composited frame,
    // so its DWM queries would fail spuriously below.
    if unsafe { IsIconic(hwnd) } != 0 {
        return Err(known(SkipReason::Minimized));
    }
    let mut outer_raw: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(hwnd, &mut outer_raw) } == 0 {
        return Err(known(SkipReason::Unreadable));
    }
    let outer = rect_from_win(outer_raw).ok_or_else(|| known(SkipReason::Unreadable))?;
    let mut visible_raw: RECT = unsafe { std::mem::zeroed() };
    let visible = if unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&mut visible_raw as *mut RECT).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
    } == 0
    {
        rect_from_win(visible_raw).ok_or_else(|| known(SkipReason::Unreadable))?
    } else {
        return Err(known(SkipReason::Unreadable));
    };
    if !tiler_core::bounds::valid_carried_rect(visible.x, visible.y, visible.w, visible.h) {
        return Err(known(SkipReason::Unreadable));
    }
    let mut cloaked: i32 = 0;
    if unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            (&mut cloaked as *mut i32).cast(),
            std::mem::size_of::<i32>() as u32,
        )
    } != 0
    {
        return Err(known(SkipReason::Unreadable));
    }
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
    let exstyle = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
    let owned = !unsafe { GetWindow(hwnd, GW_OWNER) }.is_null();
    let class = class_of(hwnd);
    let captionless = style & WS_CAPTION == 0;
    let facts = WindowFacts {
        visible: true,
        minimized: unsafe { IsIconic(hwnd) } != 0,
        maximized: unsafe { IsZoomed(hwnd) } != 0,
        cloaked: cloaked != 0,
        elevated: false,
        shell: is_shell_class(&class),
        tool_window: exstyle & WS_EX_TOOLWINDOW != 0,
        owned,
        captionless_fullscreen: is_borderless_fullscreen(captionless, visible, fulls),
        no_activate: exstyle & WS_EX_NOACTIVATE != 0,
        dialog: (!owned && class == DIALOG_CLASS)
            || crate::tiling::is_own_settings_window(&class, &identity.exe_path, &me.exe_path),
    };
    Ok(ObservedWindow {
        hwnd: hwnd_u64,
        token,
        outer,
        visible,
        insets: FrameInsets::measure(outer, visible),
        identity,
        facts,
    })
}
