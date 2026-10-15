use super::*;

/// One Start-menu mask reservation: the arming chord plus the Win-up instant
/// and the SendInput result stamped after the send, so a short write can never
/// be logged as success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueuedMask {
    pub trigger: MaskTrigger,
    pub tick: std::time::Instant,
    pub inserted: u8,
    pub release_sent: bool,
}

/// Managed-window origin bound at chord time. `hwnd`/`pid`/`creation` are the
/// process-lifetime evidence the owner rechecks against fresh observation (a
/// recycled HWND never matches); only `token` may enter logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapOrigin {
    pub hwnd: u64,
    pub token: String,
    pub pid: u32,
    pub creation: String,
}

/// Owner-side verdict for one queued intent against fresh observation.
/// `Dispatch` carries the verified token to act from; `continued` marks an
/// explicitly verified owner-caused focus advance (a prior actuation in the
/// same batch), the only case where the dispatch window may differ from the
/// chord origin. `Reject` carries the settled outcome: `origin-vanished` when
/// the origin HWND/identity is gone or recycled, `foreground-changed` when an
/// external focus move must never be silently retargeted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OriginVerdict {
    Dispatch { token: String, continued: bool },
    Reject(&'static str),
}

fn triple_matches(window: &SnapOrigin, hwnd: u64, token: &str, pid: u32, creation: &str) -> bool {
    window.hwnd == hwnd && window.token == token && window.pid == pid && window.creation == creation
}

/// Bind a chord origin to fresh observation. Pure over the owner's completed
/// observation plus the fresh foreground HWND: no native calls, no Engine.
#[must_use]
pub fn resolve_origin(
    origin: &SnapOrigin,
    foreground_hwnd: Option<u64>,
    fresh: &[SnapOrigin],
    owned_advance: Option<&SnapOrigin>,
) -> OriginVerdict {
    let origin_live = fresh
        .iter()
        .any(|w| triple_matches(w, origin.hwnd, &origin.token, origin.pid, &origin.creation));
    if !origin_live {
        return OriginVerdict::Reject("origin-vanished");
    }
    if foreground_hwnd == Some(origin.hwnd) {
        return OriginVerdict::Dispatch {
            token: origin.token.clone(),
            continued: false,
        };
    }
    if let Some(advance) = owned_advance
        && let Some(fg) = foreground_hwnd
        && let Some(window) = fresh
            .iter()
            .find(|w| triple_matches(w, fg, &advance.token, advance.pid, &advance.creation))
    {
        return OriginVerdict::Dispatch {
            token: window.token.clone(),
            continued: true,
        };
    }
    OriginVerdict::Reject("foreground-changed")
}

/// One approved chord captured by the callback. `origin` is the managed
/// identity bound at chord time (`None` means background/inactive at chord
/// time); `tick` is `Instant::now()` read in the callback (cheap,
/// monotonic); wall-clock anchoring happens outside the callback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedIntent {
    pub op: SnapOp,
    pub direction: Direction,
    pub edge: SnapEdge,
    pub origin: Option<SnapOrigin>,
    pub consumed: bool,
    pub announce: bool,
    pub tick: std::time::Instant,
}

/// One approved workspace chord captured by the callback. `origin` is the
/// managed identity bound at chord time (`None` means background/inactive or
/// unmanaged foreground at chord time); select dispatches without an origin
/// while send requires one. `follow` pins the explicit send intent at down
/// time (direct Shift+digit follows; stay rows rebind through the stay action).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedWorkspaceIntent {
    pub op: WorkspaceOp,
    pub index: u8,
    pub follow: bool,
    pub edge: SnapEdge,
    pub origin: Option<SnapOrigin>,
    pub consumed: bool,
    pub announce: bool,
    pub tick: std::time::Instant,
}

/// One approved item 2 relative workspace-send chord captured by the
/// callback. `delta` is -1 (previous) or +1 (next) in the item 1 scoped ring;
/// `follow` pins the explicit intent at down time. Sends require a managed
/// origin like numbered sends; `origin` rides for mask/queue parity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedWorkspaceSendIntent {
    pub delta: i32,
    pub follow: bool,
    pub edge: SnapEdge,
    pub origin: Option<SnapOrigin>,
    pub consumed: bool,
    pub announce: bool,
    pub tick: std::time::Instant,
}

/// One approved item 5 explicit output-send chord captured by the callback.
/// `direction` pins from the canonical key; `follow` pins the explicit intent
/// at down time. Sends require a managed origin like numbered sends; `origin`
/// rides for mask/queue parity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedWorkspaceOutputIntent {
    pub direction: Direction,
    pub follow: bool,
    pub edge: SnapEdge,
    pub origin: Option<SnapOrigin>,
    pub consumed: bool,
    pub announce: bool,
    pub tick: std::time::Instant,
}

/// One approved workspace history chord captured by the callback (item 1).
/// History selects need no origin: toggle/relative steps act on the chord
/// output's current view, never a managed mover. `origin` rides only for
/// mask/queue parity and is ignored at dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedWorkspaceHistoryIntent {
    pub op: WorkspaceHistoryOp,
    pub edge: SnapEdge,
    pub origin: Option<SnapOrigin>,
    pub consumed: bool,
    pub announce: bool,
    pub tick: std::time::Instant,
}

/// One approved item 14 whole-workspace migration chord captured by the
/// callback. `direction` pins from the canonical letter key at down time.
/// Migration acts on the chord output's active workspace, never a managed
/// mover: `origin` rides only for mask/queue parity and is ignored at
/// dispatch, like history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedWorkspaceMigrateIntent {
    pub direction: Direction,
    pub edge: SnapEdge,
    pub origin: Option<SnapOrigin>,
    pub consumed: bool,
    pub announce: bool,
    pub tick: std::time::Instant,
}

/// One approved maximize chord captured by the callback. `origin` is the
/// managed identity bound at chord time (`None` means background/inactive or
/// unmanaged foreground at chord time); without an origin the toggle never
/// dispatches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedMaximizeIntent {
    pub edge: SnapEdge,
    pub origin: Option<SnapOrigin>,
    pub consumed: bool,
    pub announce: bool,
    pub tick: std::time::Instant,
}

/// One approved fullscreen chord captured by the callback. `origin` is the
/// managed identity bound at chord time (`None` means background/inactive or
/// unmanaged foreground at chord time); without an origin the toggle never
/// dispatches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedFullscreenIntent {
    pub edge: SnapEdge,
    pub origin: Option<SnapOrigin>,
    pub consumed: bool,
    pub announce: bool,
    pub tick: std::time::Instant,
}

/// One approved float chord captured by the callback. `origin` is the
/// managed identity bound at chord time (`None` means background/inactive or
/// unmanaged foreground at chord time); without an origin the toggle never
/// dispatches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedFloatIntent {
    pub edge: SnapEdge,
    pub origin: Option<SnapOrigin>,
    pub consumed: bool,
    pub announce: bool,
    pub tick: std::time::Instant,
}

/// One approved sticky chord captured by the callback. Same origin contract
/// as float; without an origin the toggle never dispatches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedStickyIntent {
    pub edge: SnapEdge,
    pub origin: Option<SnapOrigin>,
    pub consumed: bool,
    pub announce: bool,
    pub tick: std::time::Instant,
}

/// One approved orientation chord captured by the callback (item 4). Same
/// origin contract as maximize; without an origin the toggle never
/// dispatches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedOrientationIntent {
    pub edge: SnapEdge,
    pub origin: Option<SnapOrigin>,
    pub consumed: bool,
    pub announce: bool,
    pub tick: std::time::Instant,
}

/// One approved workspace-tiling chord captured by the callback (Win+Y,
/// item 21). The toggle acts on the current workspace, never a focused
/// window: `origin` rides only for mask/queue parity and is ignored at
/// dispatch, like the launchers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedWorkspaceTilingIntent {
    pub edge: SnapEdge,
    pub origin: Option<SnapOrigin>,
    pub consumed: bool,
    pub announce: bool,
    pub tick: std::time::Instant,
}

/// One approved launcher chord captured by the callback (Win+B browser,
/// Win+T terminal). `kind` pins the explicit intent at down time. Launchers
/// act on no window: `origin` rides only for mask/queue parity and is
/// ignored at dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedLaunchIntent {
    pub kind: LaunchKind,
    pub edge: SnapEdge,
    pub origin: Option<SnapOrigin>,
    pub consumed: bool,
    pub announce: bool,
    pub tick: std::time::Instant,
}

/// One approved keyboard resize chord captured by the callback. `mode` is
/// pinned at down time (outwards/inwards) with the direction; `origin` rides
/// for mask/queue parity. Runtime geometry dispatch is a later unit: the
/// owner currently settles these without acting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedResizeIntent {
    pub mode: ResizeMode,
    pub direction: Direction,
    pub edge: SnapEdge,
    pub origin: Option<SnapOrigin>,
    pub consumed: bool,
    pub announce: bool,
    pub tick: std::time::Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueuedSnapEvent {
    Intent(QueuedIntent),
    Resize(QueuedResizeIntent),
    Workspace(QueuedWorkspaceIntent),
    WorkspaceSend(QueuedWorkspaceSendIntent),
    WorkspaceOutput(QueuedWorkspaceOutputIntent),
    WorkspaceMigrate(QueuedWorkspaceMigrateIntent),
    WorkspaceHistory(QueuedWorkspaceHistoryIntent),
    Maximize(QueuedMaximizeIntent),
    Fullscreen(QueuedFullscreenIntent),
    Float(QueuedFloatIntent),
    Sticky(QueuedStickyIntent),
    Orientation(QueuedOrientationIntent),
    WorkspaceTiling(QueuedWorkspaceTilingIntent),
    Launch(QueuedLaunchIntent),
    Mask(QueuedMask),
}

/// Bounded preallocated FIFO from the callback to the owner. `push` refuses
/// past capacity and counts the loss; the classifier's suppression verdict
/// remains authoritative even when action evidence cannot be queued.
#[derive(Debug)]
pub struct SnapQueue {
    inner: std::collections::VecDeque<QueuedSnapEvent>,
    pub dropped: u32,
}

impl SnapQueue {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: std::collections::VecDeque::with_capacity(INTENT_QUEUE_CAP),
            dropped: 0,
        }
    }

    #[must_use]
    pub fn is_full(&self) -> bool {
        self.inner.len() >= INTENT_QUEUE_CAP
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns false (and counts the loss) when full.
    pub fn push(&mut self, ev: QueuedSnapEvent) -> bool {
        if self.is_full() {
            self.dropped += 1;
            return false;
        }
        self.inner.push_back(ev);
        true
    }

    pub fn record_drop(&mut self) {
        self.dropped += 1;
    }

    /// Drop every queued record for a live config change: stale actions must
    /// never dispatch under a new config. Returns the dropped count; the
    /// bounded loss counter absorbs them like saturation drops.
    pub fn drain_stale(&mut self) -> usize {
        let dropped = self.inner.len();
        self.inner.clear();
        self.dropped = self.dropped.saturating_add(dropped as u32);
        dropped
    }

    pub fn pop_front(&mut self) -> Option<QueuedSnapEvent> {
        self.inner.pop_front()
    }

    /// Newest record, for stamping a mask result right after its send. Safe
    /// because reserve, send, and stamp run synchronously on the hook thread
    /// with no drain interleaving; reentrant injected events never touch the
    /// queue.
    pub fn back_mut(&mut self) -> Option<&mut QueuedSnapEvent> {
        self.inner.back_mut()
    }
}

impl Default for SnapQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Saturation-aware gate shared by the hook callback and tests. Classifier
/// suppression/pairing/mask semantics always advance; when full only the
/// queued action/trace evidence is dropped (bounded `dropped` counter) while
/// the callback verdict still swallows owned chords. Injected and unrelated
/// inputs never touch the queue or the loss counter. `origin` is the
/// callback-bound managed identity (`None` when background/inactive or
/// unmanaged); `gate_active` is the callback-passed shortcut-handling gate
/// (owner-published, takeover-based until documented Xbox detection exists).
/// Fresh downs consume iff the machine is enabled and both the machine gate
/// and the passed gate hold; consumed-hold repeats/ups ride the stored down
/// verdict across later gate transitions, while new chords while the gate is
/// off pass through with their pair. The mask survives across the paired
/// sequence. Action safety stays with the owner's fresh per-intent rechecks.
/// Returns whether the callback must swallow the key event.
#[allow(clippy::too_many_arguments)]
pub fn classify_and_queue(
    machine: &mut SnapClassify,
    queue: &mut SnapQueue,
    vk: u32,
    is_up: bool,
    origin: Option<SnapOrigin>,
    injected: bool,
    tick: std::time::Instant,
    gate_active: bool,
) -> Option<bool> {
    // The passed gate joins the machine gate for this event only: either off
    // keeps fresh downs passing, while armed holds keep their down verdict via
    // the stored origin and the field restores afterwards.
    let saved_gate = machine.gate_active;
    machine.set_gate_active(saved_gate && gate_active);
    let ev = machine.push(vk, is_up, origin.is_some(), injected);
    machine.set_gate_active(saved_gate);
    let ev = ev?;
    let consumed = ev.consumed();
    let event = match ev {
        Classified::Snap(intent) => QueuedSnapEvent::Intent(QueuedIntent {
            op: intent.op,
            direction: intent.direction,
            edge: intent.edge,
            origin,
            consumed: intent.consumed,
            announce: intent.announce,
            tick,
        }),
        Classified::Resize(intent) => QueuedSnapEvent::Resize(QueuedResizeIntent {
            mode: intent.mode,
            direction: intent.direction,
            edge: intent.edge,
            origin,
            consumed: intent.consumed,
            announce: intent.announce,
            tick,
        }),
        Classified::Workspace(intent) => QueuedSnapEvent::Workspace(QueuedWorkspaceIntent {
            op: intent.op,
            index: intent.index,
            follow: intent.follow,
            edge: intent.edge,
            origin,
            consumed: intent.consumed,
            announce: intent.announce,
            tick,
        }),
        Classified::WorkspaceSend(intent) => {
            QueuedSnapEvent::WorkspaceSend(QueuedWorkspaceSendIntent {
                delta: intent.delta,
                follow: intent.follow,
                edge: intent.edge,
                origin,
                consumed: intent.consumed,
                announce: intent.announce,
                tick,
            })
        }
        Classified::WorkspaceOutput(intent) => {
            QueuedSnapEvent::WorkspaceOutput(QueuedWorkspaceOutputIntent {
                direction: intent.direction,
                follow: intent.follow,
                edge: intent.edge,
                origin,
                consumed: intent.consumed,
                announce: intent.announce,
                tick,
            })
        }
        Classified::WorkspaceMigrate(intent) => {
            QueuedSnapEvent::WorkspaceMigrate(QueuedWorkspaceMigrateIntent {
                direction: intent.direction,
                edge: intent.edge,
                origin,
                consumed: intent.consumed,
                announce: intent.announce,
                tick,
            })
        }
        Classified::WorkspaceHistory(intent) => {
            QueuedSnapEvent::WorkspaceHistory(QueuedWorkspaceHistoryIntent {
                op: intent.op,
                edge: intent.edge,
                origin,
                consumed: intent.consumed,
                announce: intent.announce,
                tick,
            })
        }
        Classified::Maximize(intent) => QueuedSnapEvent::Maximize(QueuedMaximizeIntent {
            edge: intent.edge,
            origin,
            consumed: intent.consumed,
            announce: intent.announce,
            tick,
        }),
        Classified::Fullscreen(intent) => QueuedSnapEvent::Fullscreen(QueuedFullscreenIntent {
            edge: intent.edge,
            origin,
            consumed: intent.consumed,
            announce: intent.announce,
            tick,
        }),
        Classified::Float(intent) => QueuedSnapEvent::Float(QueuedFloatIntent {
            edge: intent.edge,
            origin,
            consumed: intent.consumed,
            announce: intent.announce,
            tick,
        }),
        Classified::Sticky(intent) => QueuedSnapEvent::Sticky(QueuedStickyIntent {
            edge: intent.edge,
            origin,
            consumed: intent.consumed,
            announce: intent.announce,
            tick,
        }),
        Classified::Orientation(intent) => QueuedSnapEvent::Orientation(QueuedOrientationIntent {
            edge: intent.edge,
            origin,
            consumed: intent.consumed,
            announce: intent.announce,
            tick,
        }),
        Classified::WorkspaceTiling(intent) => {
            QueuedSnapEvent::WorkspaceTiling(QueuedWorkspaceTilingIntent {
                edge: intent.edge,
                origin,
                consumed: intent.consumed,
                announce: intent.announce,
                tick,
            })
        }
        Classified::Launch(intent) => QueuedSnapEvent::Launch(QueuedLaunchIntent {
            kind: intent.kind,
            edge: intent.edge,
            origin,
            consumed: intent.consumed,
            announce: intent.announce,
            tick,
        }),
    };
    if queue.is_full() {
        queue.record_drop();
        return Some(consumed);
    }
    if queue.push(event) {
        Some(consumed)
    } else {
        queue.record_drop();
        Some(consumed)
    }
}

/// Win-up mask reservation shared by the hook callback and tests. Reserves
/// queue evidence BEFORE any SendInput and returns true when the caller must
/// send the E8 pair. When full only the oldest queued action/trace evidence
/// is dropped (bounded `dropped`) to make room for the necessary mask, so a
/// necessary mask is never skipped; send-result counters still reflect the
/// actual SendInput outcome via [`stamp_mask_result`]. The Win-up itself
/// always passes through.
pub fn win_up_mask_reserve(
    machine: &mut SnapClassify,
    queue: &mut SnapQueue,
    vk: u32,
    installed: bool,
    tick: std::time::Instant,
) -> bool {
    let Some(trigger) = machine.win_up_needs_mask(vk, installed) else {
        return false;
    };
    if queue.is_full() {
        // Graceful saturation: drop oldest action/trace evidence (bounded,
        // no alloc/log, callback stays prompt) to reserve the necessary
        // mask. The dropped action is counted; the mask send still runs and
        // its actual result is stamped.
        if queue.pop_front().is_some() {
            queue.record_drop();
        }
    }
    if queue.push(QueuedSnapEvent::Mask(QueuedMask {
        trigger,
        tick,
        inserted: 0,
        release_sent: false,
    })) {
        machine.mask_attempted += 1;
        true
    } else {
        // Still no room (defensive): count the skip and restore the hold so
        // a later Win-up in the same hold can retry. Nothing was injected.
        machine.mask_skipped += 1;
        if machine.win_l || machine.win_r {
            machine.hold_masked = false;
            machine.mask_pending = true;
            machine.mask_trigger = Some(trigger);
        }
        false
    }
}

/// Records a completed mask send into the reserved slot. `inserted` is the
/// SendInput accepted-event count (0..=2); `release_sent` reports the single
/// bounded E8-up attempt after a partial insert. Only a clean pair counts
/// as success.
pub fn stamp_mask_result(
    machine: &mut SnapClassify,
    queue: &mut SnapQueue,
    inserted: u8,
    release_sent: bool,
) {
    if inserted == 2 && !release_sent {
        machine.mask_ok += 1;
    } else {
        machine.mask_failed += 1;
    }
    if let Some(QueuedSnapEvent::Mask(slot)) = queue.back_mut() {
        slot.inserted = inserted;
        slot.release_sent = release_sent;
    }
}
