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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HookCause {
    Foreground,
    Visibility,
    Other,
    Unknown,
}

impl HookCause {
    /// Closed trace vocabulary for hook invalidation causes. No HWND,
    /// titles, or identity: counts only.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Foreground => "foreground",
            Self::Visibility => "visibility",
            Self::Other => "other",
            Self::Unknown => "unknown",
        }
    }
}

/// Delivery-time read attached to one queued wake edge. `Unknown` means no
/// usable read (destroy, unreadable, or double-absent); `DeliveryRead`
/// carries the pid plus the visible-membership tag as observed when the
/// out-of-context callback delivered, before the owner drain. Delivery can
/// predate HWND reuse, so this never proves the original generation and
/// never decides gates, membership, or causality: it is a diagnostic hint
/// the drain-time join validates against current member state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DeliverySnapshot {
    Unknown,
    DeliveryRead {
        pid: Option<u32>,
        tag: Option<String>,
    },
}

impl DeliverySnapshot {
    pub(super) const fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::DeliveryRead { .. } => "delivery-read",
        }
    }

    pub(super) fn has_tag(&self) -> bool {
        matches!(self, Self::DeliveryRead { tag: Some(_), .. })
    }
}

/// One preserved window wake edge: the exact native event plus a
/// delivery-time HWND read. `cause` keeps the unchanged gate vocabulary
/// (foreground/visibility/other/unknown); `event` keeps the exact kind
/// (create vs destroy vs location) and `delivery` keeps the uncertain
/// delivery read. Destroy always reads `Unknown`: the window is gone and
/// the number may already be reused, so a delivery read must never bind the
/// edge to a new generation. `thread`/`time` ride along for exactness and
/// never enter product logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WakeEdge {
    pub hwnd: isize,
    pub cause: HookCause,
    pub event: u32,
    pub id_object: i32,
    pub thread: u32,
    pub time: u32,
    /// Callback-time monotonic stamp (same `mono_ms` basis as log rows):
    /// when the out-of-context hook callback queued this edge. Primitive
    /// metadata only: the callback never logs. Joins the deferred-vs-delayed
    /// question at the drain, never a causal claim on its own.
    pub queued_mono_ms: u64,
    pub delivery: DeliverySnapshot,
}

impl WakeEdge {
    pub(super) fn kind_str(&self) -> &'static str {
        edge_kind_str(self.event)
    }

    #[cfg(test)]
    pub(super) fn test(hwnd: isize, cause: HookCause) -> Self {
        let event = match cause {
            HookCause::Foreground => EVENT_SYSTEM_FOREGROUND,
            HookCause::Visibility => EVENT_OBJECT_SHOW,
            HookCause::Other => EVENT_OBJECT_LOCATIONCHANGE,
            HookCause::Unknown => 0x800A,
        };
        Self {
            hwnd,
            cause,
            event,
            id_object: OBJID_WINDOW,
            thread: 0,
            time: 0,
            queued_mono_ms: 0,
            delivery: DeliverySnapshot::Unknown,
        }
    }

    #[cfg(test)]
    pub(super) fn test_with_event(hwnd: isize, event: u32, delivery: DeliverySnapshot) -> Self {
        Self {
            hwnd,
            cause: wake_cause(event),
            event,
            id_object: OBJID_WINDOW,
            thread: 7,
            time: 9,
            queued_mono_ms: 0,
            delivery,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) enum HookEvent {
    Wake(WakeEdge),
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

/// Pure wake-cause rule: which cause one delivered WinEvent carries.
/// Total over the [`window_edge_wakes`] set: foreground stays distinct,
/// SHOW/HIDE carry visibility, CREATE/DESTROY/LOCATIONCHANGE fold to other
/// (semantically equivalent topology edges), and anything outside the set
/// (unreachable through the sink) reads unknown without waking.
pub(super) fn wake_cause(event: u32) -> HookCause {
    if event == EVENT_SYSTEM_FOREGROUND {
        HookCause::Foreground
    } else if matches!(event, EVENT_OBJECT_SHOW | EVENT_OBJECT_HIDE) {
        HookCause::Visibility
    } else if matches!(
        event,
        EVENT_OBJECT_CREATE | EVENT_OBJECT_DESTROY | EVENT_OBJECT_LOCATIONCHANGE
    ) {
        HookCause::Other
    } else {
        HookCause::Unknown
    }
}

/// Exact per-event kind vocabulary for trace-only pending-edge rows.
/// Distinct from [`wake_cause`]: create/destroy/location never coalesce
/// here, and the strings never gate or reconcile anything.
pub(super) fn edge_kind_str(event: u32) -> &'static str {
    if event == EVENT_SYSTEM_FOREGROUND {
        "foreground"
    } else if event == EVENT_OBJECT_CREATE {
        "create"
    } else if event == EVENT_OBJECT_DESTROY {
        "destroy"
    } else if event == EVENT_OBJECT_SHOW {
        "show"
    } else if event == EVENT_OBJECT_HIDE {
        "hide"
    } else if event == EVENT_OBJECT_LOCATIONCHANGE {
        "location"
    } else {
        "unknown"
    }
}

/// Delivery-time read for one window edge. Destroy short-circuits to
/// `Unknown` without reading: the window is gone and the number may already
/// be reused, so any successful read could belong to a new generation.
/// Other edges read only the two synchronous USER reads safe in the
/// callback (`GetWindowThreadProcessId` pid plus the visible-membership tag
/// property via the existing safe getter); both fail closed to `None`, and
/// a double-absent read stays `Unknown` rather than fabricating identity.
/// The returned pid/tag is the delivery observation only: out-of-context
/// delivery can predate HWND reuse, so it never proves the original
/// generation and never drives gates or membership. Callback cost is one
/// bounded USER pid read plus one bounded property read (small fixed-size
/// 16-hex render, no heap growth beyond the row itself, no new `unsafe`);
/// both reads run before the queue lock, never while holding it.
fn snapshot_delivery(event: u32, hwnd_u64: u64) -> DeliverySnapshot {
    if event == EVENT_OBJECT_DESTROY {
        return DeliverySnapshot::Unknown;
    }
    let pid = crate::ffi::window_pid(hwnd_u64);
    let tag = crate::product_hide::sys::read_member_tag(hwnd_u64);
    let pid = (pid != 0).then_some(pid);
    if pid.is_none() && tag.is_none() {
        DeliverySnapshot::Unknown
    } else {
        DeliverySnapshot::DeliveryRead { pid, tag }
    }
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
            // The cause rides along for trace-only diagnostics; drain and gate
            // semantics treat every wake identically.
            let cause = wake_cause(ev.event);
            if matches!(cause, HookCause::Visibility) {
                bump_visibility_generation();
            }
            let hwnd_u64 = ev.hwnd;
            let delivery = snapshot_delivery(ev.event, hwnd_u64);
            push_hook(HookEvent::Wake(WakeEdge {
                hwnd: ev.hwnd as isize,
                cause,
                event: ev.event,
                id_object: ev.id_object,
                thread: ev.thread,
                time: ev.time,
                // Delivery timestamp only: same monotonic basis as the log
                // rows, captured without logging (the callback never logs).
                queued_mono_ms: super::trace::log_mono_ms(),
                delivery,
            }));
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

/// Trace-only pending-cause peek: per-cause counts over the queued edges
/// without draining them, so the poll gate and the drain below observe the
/// identical queue. Fixed order (foreground, visibility, other, gesture,
/// unknown), nonzero causes only, at most five entries: bounded by the
/// closed cause vocabulary. No HWND, titles, content, or identity: counts
/// only. Gesture START/END fold to one `gesture` bucket (semantically
/// equivalent movement edges); every wake cause stays distinct and no two
/// causes ever coalesce.
pub(super) fn hook_cause_summary() -> Vec<(&'static str, u64)> {
    let (mut foreground, mut visibility, mut other, mut gesture, mut unknown) =
        (0u64, 0u64, 0u64, 0u64, 0u64);
    if let Ok(queue) = hook_queue().lock() {
        for event in queue.iter() {
            match event {
                HookEvent::Wake(edge) => match edge.cause {
                    HookCause::Foreground => {
                        foreground = foreground.saturating_add(1);
                    }
                    HookCause::Visibility => {
                        visibility = visibility.saturating_add(1);
                    }
                    HookCause::Other => {
                        other = other.saturating_add(1);
                    }
                    HookCause::Unknown => {
                        unknown = unknown.saturating_add(1);
                    }
                },
                HookEvent::MoveSizeStart(..) | HookEvent::MoveSizeEnd(..) => {
                    gesture = gesture.saturating_add(1);
                }
            }
        }
    }
    let mut out = Vec::with_capacity(5);
    for (cause, count) in [
        (HookCause::Foreground.as_str(), foreground),
        (HookCause::Visibility.as_str(), visibility),
        (HookCause::Other.as_str(), other),
        ("gesture", gesture),
        (HookCause::Unknown.as_str(), unknown),
    ] {
        if count > 0 {
            out.push((cause, count));
        }
    }
    out
}

/// Max trace-only pending-edge rows per diagnostic emission: compact and
/// bounded; the remainder folds into an honest omitted count.
pub(super) const MAX_PENDING_EDGE_ROWS: usize = 8;

/// One drained `EVENT_SYSTEM_FOREGROUND` edge with its delivery/owner
/// timestamps for the in-flight focus join. `event_ms` is the WinEvent
/// delivery time (system event-time basis, wraps); `queued_mono_ms` is the
/// hook-callback queue time and `drain_mono_ms` the owner-drain time (both
/// the shared log `mono_ms` basis). Detection, delivery, and observation
/// stay distinct fields: no causal claim is encoded here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ForegroundEdgeMeta {
    pub hwnd: u64,
    pub event_ms: u32,
    pub thread: u32,
    pub queued_mono_ms: u64,
    pub drain_mono_ms: u64,
    /// Delivery-time read from the hook callback (pid plus membership
    /// tag as observed at delivery): the drain-time join below validates
    /// it against current member state exactly like the pending preview.
    pub delivery: DeliverySnapshot,
}

/// Max drained foreground edges retained for the flight join per pass: the
/// generic pending preview above stays capped at eight rows, so correlated
/// foreground edges ride this separate bound instead of being lost to it.
/// Excess folds into an honest omitted count, never a silent drop.
pub(super) const MAX_FOREGROUND_TRACE_EDGES: usize = 32;
/// Max correlated foreground rows rendered on one wake-attribution line.
pub(super) const MAX_FOREGROUND_TRACE_ROWS: usize = 8;

/// One current member candidate for delivery-join validation: the live
/// `(hwnd, pid)` plus the stored visible-membership tag and the opaque
/// session token. The production drain builds these from `member_tokens`
/// plus `member_tags`; tests build them by hand. Creation is deliberately
/// absent: the callback cannot read it, so the join validates pid plus the
/// reuse-discriminating tag only and says so on mismatch.
#[derive(Debug, Clone, Copy)]
pub(super) struct MemberJoinView<'a> {
    pub hwnd: u64,
    pub pid: u32,
    pub token: &'a str,
    pub stored_tag: Option<&'a str>,
}

/// Validate one queued delivery read against current member state. Returns
/// the opaque token on an exact single-candidate pid-plus-tag match and
/// `None` with a bounded reason otherwise: `no-member`, `ambiguous`
/// (several generations share the number), `unknown-delivery` (destroy or
/// an unreadable delivery read, never proof), `pid-mismatch`, or
/// `tag-mismatch` (absent or different stored tag, including a delivery
/// read that predates reuse). No silent bind: a lone candidate still needs
/// the full match, and the reason never carries identity values.
pub(super) fn join_delivery_token(
    hwnd: u64,
    snapshot: &DeliverySnapshot,
    members: &[MemberJoinView<'_>],
) -> (Option<String>, &'static str) {
    let mut candidate: Option<&MemberJoinView<'_>> = None;
    let mut count = 0u32;
    for member in members {
        if member.hwnd == hwnd {
            count += 1;
            candidate = Some(member);
        }
    }
    let Some(single) = (count == 1).then_some(candidate).flatten() else {
        return if count == 0 {
            (None, "no-member")
        } else {
            (None, "ambiguous")
        };
    };
    let DeliverySnapshot::DeliveryRead { pid, tag } = snapshot else {
        return (None, "unknown-delivery");
    };
    let Some(pid) = pid else {
        return (None, "unknown-delivery");
    };
    if *pid != single.pid {
        return (None, "pid-mismatch");
    }
    let Some(tag) = tag else {
        return (None, "unknown-delivery");
    };
    if single.stored_tag != Some(tag.as_str()) {
        return (None, "tag-mismatch");
    }
    (Some(single.token.to_owned()), "matched")
}

/// Trace-only pending-edge peek: one compact row per queued wake edge
/// without draining, so the poll gate and the drain below observe the
/// identical queue. Each row carries the exact edge kind (create vs
/// destroy vs location never coalesce), the unchanged gate cause, the
/// caller-joined managed flag plus the validated token/join reason, the
/// class, and the abstract delivery read (`unknown` vs `delivery-read`
/// plus whether a tag was present). No HWND, pid, thread, time, titles,
/// paths, or tag values: opaque tokens, classes, and bounded vocabulary
/// only. Rows cap at [`MAX_PENDING_EDGE_ROWS`] with the queue lock held
/// only for the bounded copy plus an honest total; native class reads run
/// after the lock is released. Gesture edges are skipped here (they ride
/// the cause summary `gesture` bucket); wake causes never coalesce. No
/// causal claim: rows are pending notifications only, never proof of what
/// changed.
pub(super) fn pending_edge_rows(
    join: impl Fn(u64, &DeliverySnapshot) -> (bool, Option<String>, &'static str),
    class_of: impl Fn(u64) -> Option<String>,
) -> (Vec<serde_json::Value>, usize) {
    let (edges, total) = hook_queue()
        .lock()
        .map(|queue| {
            let total = queue
                .iter()
                .filter(|event| matches!(event, HookEvent::Wake(_)))
                .count();
            let edges: Vec<WakeEdge> = queue
                .iter()
                .filter_map(|event| match event {
                    HookEvent::Wake(edge) => Some(edge.clone()),
                    HookEvent::MoveSizeStart(..) | HookEvent::MoveSizeEnd(..) => None,
                })
                .take(MAX_PENDING_EDGE_ROWS)
                .collect();
            (edges, total)
        })
        .unwrap_or_default();
    let mut rows = Vec::with_capacity(edges.len());
    for edge in &edges {
        let hwnd_u64 = edge.hwnd as u64;
        let (managed, token, join_reason) = join(hwnd_u64, &edge.delivery);
        let class = class_of(hwnd_u64);
        let mut row = serde_json::json!({
            "kind": edge.kind_str(),
            "cause": edge.cause.as_str(),
            "managed": managed,
            "join": join_reason,
            "delivery": edge.delivery.as_str(),
            "tagged": edge.delivery.has_tag(),
        });
        if let Some(token) = token {
            row["token"] = serde_json::Value::from(token);
        } else {
            row["token"] = serde_json::Value::Null;
        }
        if let Some(class) = class {
            if class.is_empty() {
                row["class"] = serde_json::Value::Null;
            } else {
                row["class"] = serde_json::Value::from(class);
            }
        } else {
            row["class"] = serde_json::Value::Null;
        }
        rows.push(row);
    }
    let omitted = total.saturating_sub(rows.len());
    (rows, omitted)
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
    observe_window_inner(hwnd, me, fulls, tokens, false)
}

/// Fresh facts read of a hidden window the caller already proved: the
/// caller holds the exact owned committed hidden claim (ledger plus
/// identity/lifetime gates) for this HWND, and the window still reads
/// invisible. Bypasses only the visible gate; every other read (identity,
/// iconic, frames, cloak, style, class, fullscreen predicate) is the
/// shared path, so the facts carry the same classification as a visible
/// read. The caller decides the safety use; this never proves visibility.
pub(super) fn observe_window_hidden_verified(
    hwnd: HWND,
    me: &ProcessIdentity,
    fulls: &[Rect],
    tokens: &mut TokenMap,
) -> std::result::Result<ObservedWindow, ObserveFailure> {
    observe_window_inner(hwnd, me, fulls, tokens, true)
}

fn observe_window_inner(
    hwnd: HWND,
    me: &ProcessIdentity,
    fulls: &[Rect],
    tokens: &mut TokenMap,
    allow_hidden: bool,
) -> std::result::Result<ObservedWindow, ObserveFailure> {
    use ObserveFailure::{Known, Unknown};
    let hwnd_u64 = hwnd as usize as u64;
    if !allow_hidden && !crate::ffi::is_visible(hwnd_u64) {
        return Err(Unknown);
    }
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
        visible: crate::ffi::is_visible(hwnd_u64),
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

#[cfg(test)]
mod cause_tests {
    use super::*;

    fn clear_queue() {
        if let Ok(mut queue) = hook_queue().lock() {
            queue.clear();
        }
    }

    #[test]
    fn wake_causes_classify_distinctly() {
        // Pure rule: foreground, visibility, topology, and unknown never
        // share a cause. The wake filter itself is unchanged (asserted
        // alongside so a cause edit cannot widen the hook scope).
        assert!(window_edge_wakes(EVENT_SYSTEM_FOREGROUND, OBJID_WINDOW));
        assert_eq!(wake_cause(EVENT_SYSTEM_FOREGROUND), HookCause::Foreground);
        for event in [EVENT_OBJECT_SHOW, EVENT_OBJECT_HIDE] {
            assert!(window_edge_wakes(event, OBJID_WINDOW));
            assert_eq!(wake_cause(event), HookCause::Visibility);
        }
        for event in [
            EVENT_OBJECT_CREATE,
            EVENT_OBJECT_DESTROY,
            EVENT_OBJECT_LOCATIONCHANGE,
        ] {
            assert!(window_edge_wakes(event, OBJID_WINDOW));
            assert_eq!(wake_cause(event), HookCause::Other);
        }
        assert!(!window_edge_wakes(EVENT_OBJECT_SHOW, OBJID_WINDOW + 1));
        assert_eq!(wake_cause(EVENT_SYSTEM_MOVESIZESTART), HookCause::Unknown);
    }

    #[test]
    fn every_cause_survives_summary_and_still_invalidates() {
        // Each cause pushed alone: the non-consuming production poll
        // (`hook_edges_pending`) reads true without draining, and the peek
        // still reports that same cause afterwards. Every cause therefore
        // still trips the pre-setter gate exactly like the old bare wake.
        for (hwnd, cause) in [
            (1, HookCause::Foreground),
            (2, HookCause::Visibility),
            (3, HookCause::Other),
            (4, HookCause::Unknown),
        ] {
            clear_queue();
            push_hook(HookEvent::Wake(WakeEdge::test(hwnd, cause)));
            assert!(hook_edges_pending(), "{cause:?} must invalidate");
            let summary = hook_cause_summary();
            assert_eq!(summary, vec![(cause.as_str(), 1)], "{cause:?} survives");
            assert!(hook_edges_pending(), "{cause:?} peek must not drain");
            clear_queue();
        }
        // Gesture edges fold to one `gesture` bucket and invalidate too;
        // mixed batches keep every wake cause distinct (coalescing only the
        // semantically equivalent movement edges).
        clear_queue();
        push_hook(HookEvent::Wake(WakeEdge::test(11, HookCause::Foreground)));
        push_hook(HookEvent::Wake(WakeEdge::test(12, HookCause::Visibility)));
        push_hook(HookEvent::MoveSizeStart(13, 7));
        push_hook(HookEvent::MoveSizeEnd(13, None, 7));
        assert!(hook_edges_pending());
        assert_eq!(
            hook_cause_summary(),
            vec![("foreground", 1), ("visibility", 1), ("gesture", 2)]
        );
        assert!(hook_edges_pending(), "summary peek must not drain");
        clear_queue();
        assert!(!hook_edges_pending());
        assert!(hook_cause_summary().is_empty());
    }

    #[test]
    fn exact_event_kind_survives_without_coalescing() {
        // CREATE/DESTROY/LOCATION share the gate cause but keep distinct
        // exact kinds plus exact native fields; the cause summary still
        // folds them to `other` so the gate is unchanged. The join runs
        // the production validator with no members, hence `no-member`.
        fn join(hwnd: u64, snapshot: &DeliverySnapshot) -> (bool, Option<String>, &'static str) {
            let (token, reason) = join_delivery_token(hwnd, snapshot, &[]);
            (false, token, reason)
        }
        clear_queue();
        let create = WakeEdge::test_with_event(
            21,
            EVENT_OBJECT_CREATE,
            DeliverySnapshot::DeliveryRead {
                pid: Some(101),
                tag: Some("aa".to_owned()),
            },
        );
        let destroy =
            WakeEdge::test_with_event(21, EVENT_OBJECT_DESTROY, DeliverySnapshot::Unknown);
        let location = WakeEdge::test_with_event(
            21,
            EVENT_OBJECT_LOCATIONCHANGE,
            DeliverySnapshot::DeliveryRead {
                pid: Some(101),
                tag: Some("aa".to_owned()),
            },
        );
        assert_eq!(create.cause, HookCause::Other);
        assert_eq!(destroy.cause, HookCause::Other);
        assert_eq!(location.cause, HookCause::Other);
        assert_eq!(create.kind_str(), "create");
        assert_eq!(destroy.kind_str(), "destroy");
        assert_eq!(location.kind_str(), "location");
        assert_ne!(create, destroy);
        assert_ne!(create, location);
        push_hook(HookEvent::Wake(create.clone()));
        push_hook(HookEvent::Wake(destroy.clone()));
        push_hook(HookEvent::Wake(location.clone()));
        assert_eq!(hook_cause_summary(), vec![("other", 3)]);
        assert!(hook_edges_pending(), "peek must not drain");
        let (rows, omitted) = pending_edge_rows(join, |_| None);
        assert_eq!(omitted, 0);
        let kinds: Vec<&str> = rows
            .iter()
            .map(|row| row["kind"].as_str().unwrap_or_default())
            .collect();
        assert_eq!(kinds, vec!["create", "destroy", "location"]);
        for row in &rows {
            assert_eq!(row["token"], serde_json::Value::Null);
            assert_eq!(row["join"], serde_json::json!("no-member"));
        }
        // Exact native fields survive the queue round-trip.
        assert_eq!(create.event, EVENT_OBJECT_CREATE);
        assert_eq!(create.id_object, OBJID_WINDOW);
        assert_eq!((create.thread, create.time), (7, 9));
        clear_queue();
    }

    #[test]
    fn same_hwnd_distinct_deliveries_never_conflate() {
        // One HWND number, two member-tag generations: both delivery reads
        // stay ambiguous through the production validator, even when a
        // read exactly matches one generation's stored tag.
        let members = [
            MemberJoinView {
                hwnd: 31,
                pid: 201,
                token: "wA",
                stored_tag: Some("tag-a"),
            },
            MemberJoinView {
                hwnd: 31,
                pid: 201,
                token: "wB",
                stored_tag: Some("tag-b"),
            },
        ];
        let join = |hwnd: u64, snapshot: &DeliverySnapshot| {
            let (token, reason) = join_delivery_token(hwnd, snapshot, &members);
            (true, token, reason)
        };
        let first = WakeEdge::test_with_event(
            31,
            EVENT_OBJECT_LOCATIONCHANGE,
            DeliverySnapshot::DeliveryRead {
                pid: Some(201),
                tag: Some("tag-a".to_owned()),
            },
        );
        let second = WakeEdge::test_with_event(
            31,
            EVENT_OBJECT_LOCATIONCHANGE,
            DeliverySnapshot::DeliveryRead {
                pid: Some(201),
                tag: Some("tag-b".to_owned()),
            },
        );
        assert_ne!(first, second, "tag change must be a distinct edge");
        clear_queue();
        push_hook(HookEvent::Wake(first));
        push_hook(HookEvent::Wake(second));
        let (rows, _) = pending_edge_rows(join, |_| Some("SomeClass".to_owned()));
        assert_eq!(rows.len(), 2);
        for row in &rows {
            assert_eq!(row["managed"], serde_json::json!(true));
            assert_eq!(row["token"], serde_json::Value::Null);
            assert_eq!(row["join"], serde_json::json!("ambiguous"));
            assert_eq!(row["delivery"], serde_json::json!("delivery-read"));
            assert_eq!(row["tagged"], serde_json::json!(true));
            assert_eq!(row["class"], serde_json::json!("SomeClass"));
        }
        // No thread, time, pid, or tag values leak into product rows.
        for row in &rows {
            assert!(row.get("thread").is_none());
            assert!(row.get("time").is_none());
            assert!(row.get("pid").is_none());
            assert!(row.get("hwnd").is_none());
        }
        clear_queue();
    }

    #[test]
    fn single_generation_still_validates_pid_and_tag() {
        // A lone candidate is not enough: the delivery read must match the
        // stored pid plus the stored tag. A stale tag (edge predating
        // reuse), a wrong pid, and an unknown delivery all report null
        // with their reason; only the exact match joins.
        let members = [MemberJoinView {
            hwnd: 33,
            pid: 202,
            token: "w7",
            stored_tag: Some("tag-new"),
        }];
        let join = |hwnd: u64, snapshot: &DeliverySnapshot| {
            let (token, reason) = join_delivery_token(hwnd, snapshot, &members);
            (true, token, reason)
        };
        let stale = WakeEdge::test_with_event(
            33,
            EVENT_OBJECT_CREATE,
            DeliverySnapshot::DeliveryRead {
                pid: Some(202),
                tag: Some("tag-old".to_owned()),
            },
        );
        let wrong_pid = WakeEdge::test_with_event(
            33,
            EVENT_OBJECT_LOCATIONCHANGE,
            DeliverySnapshot::DeliveryRead {
                pid: Some(999),
                tag: Some("tag-new".to_owned()),
            },
        );
        let unknown =
            WakeEdge::test_with_event(33, EVENT_OBJECT_DESTROY, DeliverySnapshot::Unknown);
        let fresh = WakeEdge::test_with_event(
            33,
            EVENT_OBJECT_LOCATIONCHANGE,
            DeliverySnapshot::DeliveryRead {
                pid: Some(202),
                tag: Some("tag-new".to_owned()),
            },
        );
        clear_queue();
        push_hook(HookEvent::Wake(stale));
        push_hook(HookEvent::Wake(wrong_pid));
        push_hook(HookEvent::Wake(unknown));
        push_hook(HookEvent::Wake(fresh));
        let (rows, _) = pending_edge_rows(join, |_| None);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0]["token"], serde_json::Value::Null);
        assert_eq!(rows[0]["join"], serde_json::json!("tag-mismatch"));
        assert_eq!(rows[1]["token"], serde_json::Value::Null);
        assert_eq!(rows[1]["join"], serde_json::json!("pid-mismatch"));
        assert_eq!(rows[2]["token"], serde_json::Value::Null);
        assert_eq!(rows[2]["join"], serde_json::json!("unknown-delivery"));
        assert_eq!(rows[3]["token"], serde_json::json!("w7"));
        assert_eq!(rows[3]["join"], serde_json::json!("matched"));
        // An empty class reads as null, never fabricated.
        let (rows, _) = pending_edge_rows(join, |_| Some(String::new()));
        assert_eq!(rows[3]["class"], serde_json::Value::Null);
        clear_queue();
    }

    #[test]
    fn pending_rows_bound_copy_and_count_honestly() {
        // Ten queued wakes copy only the first eight rows; the remainder
        // folds into the omitted count and the queue stays intact.
        fn join(hwnd: u64, snapshot: &DeliverySnapshot) -> (bool, Option<String>, &'static str) {
            let (token, reason) = join_delivery_token(hwnd, snapshot, &[]);
            (false, token, reason)
        }
        clear_queue();
        for hwnd in 50..60 {
            push_hook(HookEvent::Wake(WakeEdge::test_with_event(
                hwnd,
                EVENT_OBJECT_LOCATIONCHANGE,
                DeliverySnapshot::DeliveryRead {
                    pid: Some(300),
                    tag: Some("t".to_owned()),
                },
            )));
        }
        let (rows, omitted) = pending_edge_rows(join, |_| None);
        assert_eq!(rows.len(), MAX_PENDING_EDGE_ROWS);
        assert_eq!(omitted, 10 - MAX_PENDING_EDGE_ROWS);
        assert!(hook_edges_pending(), "diag peek must not drain");
        assert_eq!(hook_cause_summary(), vec![("other", 10)]);
        clear_queue();
    }

    #[test]
    fn destroy_and_unreadable_stay_unknown_and_foreground_preserved() {
        // Destroy short-circuits without reading; an unreadable live edge
        // (no pid, no tag) is also unknown. Foreground keeps its cause.
        // The join runs the production validator: unknown delivery never
        // joins even when the HWND is currently managed.
        let members = [MemberJoinView {
            hwnd: 42,
            pid: 400,
            token: "w9",
            stored_tag: Some("tag-f"),
        }];
        let join = |hwnd: u64, snapshot: &DeliverySnapshot| {
            let (token, reason) = join_delivery_token(hwnd, snapshot, &members);
            (true, token, reason)
        };
        assert_eq!(
            snapshot_delivery(EVENT_OBJECT_DESTROY, 41),
            DeliverySnapshot::Unknown
        );
        assert_eq!(wake_cause(EVENT_SYSTEM_FOREGROUND), HookCause::Foreground);
        assert_eq!(edge_kind_str(EVENT_SYSTEM_FOREGROUND), "foreground");
        clear_queue();
        push_hook(HookEvent::Wake(WakeEdge::test_with_event(
            42,
            EVENT_SYSTEM_FOREGROUND,
            DeliverySnapshot::Unknown,
        )));
        let (rows, _) = pending_edge_rows(join, |_| None);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["kind"], serde_json::json!("foreground"));
        assert_eq!(rows[0]["cause"], serde_json::json!("foreground"));
        assert_eq!(rows[0]["delivery"], serde_json::json!("unknown"));
        assert_eq!(rows[0]["token"], serde_json::Value::Null);
        assert_eq!(rows[0]["join"], serde_json::json!("unknown-delivery"));
        assert_eq!(hook_cause_summary(), vec![("foreground", 1)]);
        assert!(hook_edges_pending(), "diag peek must not drain");
        clear_queue();
    }
}
