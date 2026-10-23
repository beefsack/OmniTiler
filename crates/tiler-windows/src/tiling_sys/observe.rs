#![forbid(unsafe_code)]
use super::*;

// Win32 object id for window-scoped events (filters out menu/scrollbar noise).
const OBJID_WINDOW: i32 = 0;

// Hook delivery: out-of-context callbacks run on the installing (loop) thread
// while it pumps messages, so the callback and the loop share one queue lock
// with a strict invariant - the loop never holds it across a message pump -
// and the callback takes it blocking (never try_lock-and-drop, lossless).
//
// Visibility generation: every window-scoped SHOW/HIDE notification bumps a
// bounded process-global counter alongside the unchanged `Wake` queue push
// below (no extra queue, no payload). The joint visibility batch compares
// this counter across its owner wait to decide `Notified` (re-snapshot)
// versus `Unrelated` (wait again on the same fixed deadline): state alone
// confirms, never the event. Unrelated hook edges never bump it, so they can
// neither confirm nor reset the deadline nor spin the wait.
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

#[cfg(not(test))]
pub(super) fn hook_queue() -> &'static Mutex<Vec<HookEvent>> {
    static QUEUE: OnceLock<Mutex<Vec<HookEvent>>> = OnceLock::new();
    QUEUE.get_or_init(|| Mutex::new(Vec::new()))
}

#[cfg(test)]
thread_local! {
    static TEST_HOOK_QUEUE: std::sync::Arc<Mutex<Vec<HookEvent>>> =
        std::sync::Arc::new(Mutex::new(Vec::new()));
}

/// Test storage for [`hook_queue`]: each test thread owns its queue, so
/// parallel drain tests never observe each other's edges.
#[cfg(test)]
pub(super) fn hook_queue() -> std::sync::Arc<Mutex<Vec<HookEvent>>> {
    TEST_HOOK_QUEUE.with(std::sync::Arc::clone)
}

/// Bounded window-visibility generation bumped once per window-scoped
/// SHOW/HIDE callback. The counter wraps silently on overflow: only
/// inequality across one owner wait is ever read, never an absolute value. The `Wake`
/// queue push below is unchanged: this carries no payload and is never
/// drained, so unrelated edges stay queued for the outer loop and intake /
/// reconcile never run reentrantly inside a batch wait.
static VISIBILITY_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn bump_visibility_generation() {
    VISIBILITY_GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}

fn push_hook(event: HookEvent) {
    if let Ok(mut queue) = hook_queue().lock() {
        queue.push(event);
    }
}

/// Pure window-edge wake rule: which delivered WinEvents become loop wakes.
/// Foreground always wakes; object create/destroy/show/hide/location wake
/// only for window objects (`id_object == OBJID_WINDOW`), so menu, client,
/// scrollbar and other sub-object noise never wakes. Child windows (taskbar
/// descendants included) carry `OBJID_WINDOW` with their own HWND, so they
/// wake exactly like top-level windows. Move/size transitions are handled
/// separately above and never pass through here.
pub(super) fn window_edge_wakes(event: u32, id_object: i32) -> bool {
    id_object == OBJID_WINDOW
        && matches!(
            event,
            EVENT_SYSTEM_FOREGROUND
                | EVENT_OBJECT_CREATE
                | EVENT_OBJECT_DESTROY
                | EVENT_OBJECT_SHOW
                | EVENT_OBJECT_HIDE
                | EVENT_OBJECT_LOCATIONCHANGE
        )
}

/// Owner-loop hook-event sink: the `extern` trampoline lives in
/// `crate::ffi` and forwards here; this filtering (which edges become
/// wakes, event-time cursor/sequence capture) is unchanged policy.
pub(super) struct LoopEventSink;

impl crate::ffi::WinEventSink for LoopEventSink {
    fn on_win_event(&self, ev: crate::ffi::WinEvent) {
        if ev.event == EVENT_SYSTEM_MOVESIZESTART {
            // Event-time Esc sequence sampled directly from the shared atomic
            // clock, so a physical Esc between the real START and the owner
            // drain keeps a newer sequence than the snapshot and still latches.
            // Delayed batch sequences never decide this comparison.
            push_hook(HookEvent::MoveSizeStart(
                ev.hwnd as isize,
                crate::snapkey::sys::shared_esc_seq(),
            ));
        } else if ev.event == EVENT_SYSTEM_MOVESIZEEND {
            // KDE Finish pointer: capture the release cursor now, at event
            // time in the callback. The pump only carries it to settle and
            // never re-reads it later. The END-time Esc sequence is captured
            // alongside so post-END edges order correctly at the drain.
            push_hook(HookEvent::MoveSizeEnd(
                ev.hwnd as isize,
                cursor_pos(),
                crate::snapkey::sys::shared_esc_seq(),
            ));
        } else if window_edge_wakes(ev.event, ev.id_object) {
            // Window-scoped SHOW/HIDE additionally bumps the bounded visibility
            // generation (the `Wake` push below is unchanged). All other edges -
            // including foreground/create/destroy/location - leave it alone, so
            // only genuine visibility notifications can wake a joint batch wait.
            if matches!(ev.event, EVENT_OBJECT_SHOW | EVENT_OBJECT_HIDE) {
                bump_visibility_generation();
            }
            push_hook(HookEvent::Wake(ev.hwnd as isize));
        }
    }
}

/// Drain pending loop-thread messages (delivering hook callbacks) without
/// waiting. Returns true when anything flowed: the caller treats that as a
/// wake hint, so work staged on this same thread (hook edges, tray picks,
/// overlay clicks) never strands behind a blocking wait.
pub(super) fn drain_owner_messages() -> bool {
    crate::ffi::drain_messages()
}

/// Non-destructive hook-queue probe: true when drained edges already wait.
/// The blocking wait below still collects them; this only lets the caller
/// skip the block with a zero-timeout collect when work is already staged.
pub(super) fn hook_edges_pending() -> bool {
    hook_queue()
        .lock()
        .map(|queue| !queue.is_empty())
        .unwrap_or(false)
}

/// Product outer wait: pump loop-thread messages (delivering hook callbacks)
/// then wait for more plus the input-pump intent wake, the renderer ack
/// wake, and extra notification handles (directory watches, async
/// completion events). Returns what woke the wait so the loop can drain an ack-only wake
/// without reposting frames, reconciling, or polling settings: renderer acks
/// wake the owner immediately with no polling latency, but an ack alone
/// never triggers owner work by itself, so the ack wake cannot feed back
/// into a post/ack/post busy loop. A pending wake (offered before this wait)
/// returns immediately; bursts coalesce to one wake; the timeout is the
/// caller-computed minimum over the armed deadlines (`u32::MAX` blocks at
/// healthy idle). One thread owns install, pump, and uninstall: no join,
/// no hang.
pub(super) fn pump_wait_with_intent(
    timeout_ms: u32,
    wake: Option<&OwnedHandle>,
    ack_wake: Option<&OwnedHandle>,
    extra: &[std::os::windows::io::RawHandle],
) -> WaitWake {
    drain_owner_messages();
    let mut woke = WaitWake {
        input: false,
        ack: false,
        message: false,
        notify: false,
        timeout: false,
        failed: false,
    };
    {
        const MAX_WAIT: usize = 8;
        let mut raws = [std::ptr::null_mut(); MAX_WAIT];
        let mut count: usize = 0;
        if let Some(handle) = wake {
            raws[0] = handle.as_handle().as_raw_handle();
            count = 1;
        }
        if let Some(handle) = ack_wake {
            raws[count] = handle.as_handle().as_raw_handle();
            count += 1;
        }
        let extra_base = count;
        for raw in extra {
            if count >= MAX_WAIT {
                break;
            }
            raws[count] = *raw;
            count += 1;
        }
        // A satisfied wait object: index 0 is the input wake when
        // present; the ack wake (when present) is the next index;
        // anything at or past `extra_base` is an extra notification
        // handle (directory watch or async completion), all of which
        // demand a full turn.
        let has_input = wake.is_some();
        match crate::ffi::msg_wait(&raws[..count], timeout_ms) {
            crate::ffi::MsgWaitOutcome::Timeout => {
                woke.timeout = true;
            }
            crate::ffi::MsgWaitOutcome::Signaled(index) => {
                if has_input && index == 0 {
                    woke.input = true;
                } else if index < extra_base {
                    woke.ack = true;
                } else {
                    woke.notify = true;
                }
            }
            crate::ffi::MsgWaitOutcome::Message => {
                woke.message = true;
            }
            crate::ffi::MsgWaitOutcome::Failed => {
                // WAIT_FAILED or abandoned: counted separately as `failed`, never
                // merged into `timeout` (genuine 258 only). Control flow keeps the
                // existing fallback: a failed return takes the periodic-work path
                // below like before, with no new recovery and no timing claim (a
                // failed wait returns immediately; not proven delayed).
                woke.failed = true;
            }
        }
    }
    // Dispatched callbacks below may queue hook events: once any
    // message flows, this turn does full owner work even when the wait
    // itself returned an ack or timeout reason.
    if crate::ffi::drain_messages() {
        woke.message = true;
    }
    woke
}

/// What woke the product outer wait: any combination may hold (an ack plus
/// queued messages is common). `timeout` is the genuine deadline expiry
/// (`WAIT_TIMEOUT` 258) only; `failed` is `WAIT_FAILED`/abandoned, counted
/// separately and never merged into timeout. `notify` is any extra
/// notification handle (directory watch or async completion): always a full
/// turn, since the file or result still needs its poll/drain. Ack-only turns
/// drain acks and service the pending finish without reposting, reconciling,
/// or settings/input work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct WaitWake {
    pub(super) input: bool,
    pub(super) ack: bool,
    pub(super) message: bool,
    pub(super) notify: bool,
    pub(super) timeout: bool,
    pub(super) failed: bool,
}

impl WaitWake {
    /// Full owner turn: fresh input, real messages, an extra notification,
    /// the genuine deadline expiry (which preserves the armed retry /
    /// restore cadences), or the pre-existing failed-wait fallback
    /// path (unchanged control flow; only the counter differs).
    /// Hook-callback events collected after the wait also force a full turn
    /// at the call site, so no input starves behind ack traffic.
    #[must_use]
    pub(super) const fn full_turn(self) -> bool {
        self.input || self.message || self.notify || self.timeout || self.failed
    }
}

/// Input pump wait split `(turns, waits, work, msg, timeout, failed)` for
/// one [`pump_waits_line`] report: one coherent diag-slot snapshot.
pub(super) type InputWaitSplit = (u64, u64, u64, u64, u64, u64);
/// Renderer wait split `(waits, work, msg, timeout, failed, acks)` for one
/// [`pump_waits_line`] report: four independent atomic loads plus the ack
/// sequence, so a concurrent bump can skew the sum by one.
pub(super) type RenderWaitSplit = (u64, u64, u64, u64, u64, u64);

/// Trace-only outer-wait return counts (outer product wait only; the
/// focus/proof `pump_wait` path never constructs one of these). Reason hits
/// count set `WaitWake` flags per return, so combinations (ack plus messages
/// is common) can exceed `waits`; `timeout` counts genuine 258 expiries only
/// while `failed` counts `WAIT_FAILED`/abandoned separately; `ack_only`
/// counts the early-return subset. Timeout-cause attribution rides the same
/// counters: `wait_staged` counts staged zero-timeout collects,
/// `src_timeouts` counts genuine timeouts per [`OwnerWaitSource`]
/// discriminant (staged collects land in the `Staged` bucket),
/// `zero_future_subms` counts unstaged zero-requested timeouts whose winning
/// deadline was still in the future at selection (sub-millisecond remainder
/// meeting the millisecond conversion), `zero_already_due` counts unstaged
/// zero-requested timeouts already due at selection, and `early_timeouts`
/// counts timeouts returning before the requested duration elapsed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct OwnerWaitCounts {
    pub(super) waits: u64,
    pub(super) ack_only: u64,
    pub(super) input: u64,
    pub(super) ack: u64,
    pub(super) msg: u64,
    pub(super) notify: u64,
    pub(super) timeout: u64,
    pub(super) failed: u64,
    pub(super) wait_staged: u64,
    pub(super) src_timeouts: [u64; OWNER_WAIT_SOURCE_ORDER.len()],
    pub(super) zero_future_subms: u64,
    pub(super) zero_already_due: u64,
    pub(super) early_timeouts: u64,
}

impl OwnerWaitCounts {
    /// One outer wait return: one `waits` plus one hit per set reason flag.
    /// The accompanying [`OwnerWaitDiag`] is pure bookkeeping (no wake, no
    /// actuation): staged collects count separately, genuine timeouts land
    /// in their cause bucket, and early/zero-requested expiries split out.
    pub(super) fn note_wait(&mut self, wait: WaitWake, diag: OwnerWaitDiag) {
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
        if wait.notify {
            self.notify = self.notify.saturating_add(1);
        }
        if wait.timeout {
            self.timeout = self.timeout.saturating_add(1);
        }
        if wait.failed {
            self.failed = self.failed.saturating_add(1);
        }
        if diag.staged {
            self.wait_staged = self.wait_staged.saturating_add(1);
        }
        if wait.timeout {
            let bucket = diag.source as usize;
            if bucket < self.src_timeouts.len() {
                self.src_timeouts[bucket] = self.src_timeouts[bucket].saturating_add(1);
            }
            if !diag.staged && diag.requested_ms == 0 {
                if diag.already_due {
                    self.zero_already_due = self.zero_already_due.saturating_add(1);
                } else {
                    self.zero_future_subms = self.zero_future_subms.saturating_add(1);
                }
            }
            if diag.elapsed_ms < u64::from(diag.requested_ms) {
                self.early_timeouts = self.early_timeouts.saturating_add(1);
            }
        }
    }

    /// One ack-only early return (a subset of `waits`, never a full turn).
    pub(super) fn note_ack_only(&mut self) {
        self.ack_only = self.ack_only.saturating_add(1);
    }
}

/// Pure trace-only pump-waits report line shared by the periodic throttle
/// (emitted on naturally occurring owner turns only, never tied to a
/// monitor check) and the clean-loop-exit final flush (so quiet runs still
/// leave one measurable accumulated wait split). Takes only read-only
/// snapshots: the owner counts plus the input-diag six-tuple
/// `(turns, waits, work, msg, timeout, failed)` and the renderer six-tuple
/// `(waits, work, msg, timeout, failed, acks)`. No locks, no drains, no
/// syscalls: calling it twice changes nothing, and it cannot wake, arm, or
/// actuate anything. `flushed` marks the lifecycle (`"end-of-run"` on the
/// final flush, `None` on periodic lines).
pub(super) fn pump_waits_line(
    ow: &OwnerWaitCounts,
    input: InputWaitSplit,
    render: RenderWaitSplit,
    flushed: Option<&str>,
) -> serde_json::Value {
    let mut line = serde_json::json!({
        "event": "pump-waits",
        "owner_waits": ow.waits,
        "owner_ack_only": ow.ack_only,
        "owner_input": ow.input,
        "owner_ack": ow.ack,
        "owner_msg": ow.msg,
        "owner_notify": ow.notify,
        "owner_timeout": ow.timeout,
        "owner_failed": ow.failed,
        "owner_wait_staged": ow.wait_staged,
        "owner_src_timeouts": ow.src_timeouts,
        "owner_src_order": OWNER_WAIT_SOURCE_ORDER,
        "owner_zero_future_subms": ow.zero_future_subms,
        "owner_zero_already_due": ow.zero_already_due,
        "owner_early_timeouts": ow.early_timeouts,
        "input_turns": input.0,
        "input_waits": input.1,
        "input_work": input.2,
        "input_msg": input.3,
        "input_timeout": input.4,
        "input_failed": input.5,
        "render_waits": render.0,
        "render_work": render.1,
        "render_msg": render.2,
        "render_timeout": render.3,
        "render_failed": render.4,
        "render_acks": render.5,
        "scope": "outer-only-plus-native-waits",
        "excludes": "focus-proof-nested-waits,stop-short-circuit",
    });
    if let Some(flushed) = flushed {
        line["flushed"] = serde_json::Value::from(flushed);
    }
    line
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
    /// Fresh visible-membership lifetime tag (`MEMBER_TAG_PROP`), read with
    /// the same observation. This is the ONLY membership stamp the receipt
    /// paths may compare against the stored tag: `identity.tag` is the
    /// proof-only owned-helper token (`OmniTilerLifetime`, empty for generic
    /// windows) and never proves managed membership. `None` reads as
    /// unstamped (hold), never as proof.
    pub(super) member_tag: Option<String>,
}

pub(super) fn class_of(hwnd: HWND) -> String {
    crate::ffi::class_name(hwnd as usize as u64)
}

pub(super) fn window_identity(
    hwnd: HWND,
    hwnd_u64: u64,
    me: &ProcessIdentity,
) -> Option<ObservedTarget> {
    let pid = crate::ffi::window_pid(hwnd as usize as u64);
    // The owner never tiles its own process. Its separate-process Settings
    // UI (same executable, settings class) is an ordinary slotless member
    // via the dialog gate below, not here.
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
    let foreground = crate::ffi::foreground_window();
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
    let Some(token) =
        crate::ffi::get_prop(hwnd as usize as u64, crate::test_window::TEST_WINDOW_PROP)
    else {
        return String::new();
    };
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
    if !crate::ffi::is_visible(hwnd as usize as u64) {
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
    if crate::ffi::is_iconic(hwnd_u64) {
        return Err(known(SkipReason::Minimized));
    }
    let Some(outer_raw) = crate::ffi::window_rect(hwnd_u64) else {
        return Err(known(SkipReason::Unreadable));
    };
    let outer = rect_from_win(outer_raw).ok_or_else(|| known(SkipReason::Unreadable))?;
    let visible = match crate::ffi::extended_frame_bounds(hwnd_u64) {
        Some(visible_raw) => {
            rect_from_win(visible_raw).ok_or_else(|| known(SkipReason::Unreadable))?
        }
        None => return Err(known(SkipReason::Unreadable)),
    };
    if !tiler_core::bounds::valid_carried_rect(visible.x, visible.y, visible.w, visible.h) {
        return Err(known(SkipReason::Unreadable));
    }
    let Some(cloaked) = crate::ffi::cloaked(hwnd_u64) else {
        return Err(known(SkipReason::Unreadable));
    };
    let style = crate::ffi::window_style(hwnd_u64);
    let exstyle = crate::ffi::window_exstyle(hwnd_u64);
    let owned = crate::ffi::owner_of(hwnd_u64) != 0;
    let class = class_of(hwnd);
    let captionless = style & WS_CAPTION == 0;
    let facts = WindowFacts {
        visible: true,
        minimized: crate::ffi::is_iconic(hwnd_u64),
        maximized: crate::ffi::is_zoomed(hwnd_u64),
        cloaked,
        elevated: false,
        shell: is_shell_class(&class),
        tool_window: exstyle & WS_EX_TOOLWINDOW != 0,
        owned,
        captionless_fullscreen: is_borderless_fullscreen(captionless, visible, fulls),
        no_activate: exstyle & WS_EX_NOACTIVATE != 0,
        // Own Settings rides the ordinary membership gate (never a
        // dialog); generic unowned `#32770` dialogs still refuse.
        dialog: crate::tiling::workspace_dialog_refuses(
            &class,
            owned,
            &identity.exe_path,
            &me.exe_path,
        ),
    };
    Ok(ObservedWindow {
        hwnd: hwnd_u64,
        token,
        outer,
        visible,
        insets: FrameInsets::measure(outer, visible),
        identity,
        facts,
        member_tag: crate::product_hide::sys::read_member_tag(hwnd_u64),
    })
}
