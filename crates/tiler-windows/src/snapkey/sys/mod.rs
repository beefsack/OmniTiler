#![forbid(unsafe_code)]
use super::{
    CallbackDiag, CallbackDiagBuf, CallbackReason, CallbackSource, ChordDisable, ChordRemap,
    KeyboardConfig, MarkedDiagBuf, MarkedKeyDiag, ModDiagBuf, QueuedMask, QueuedSnapEvent,
    SnapClassify, SnapOrigin, SnapQueue,
};
use crate::input_handoff::{
    SharedSnapshot, global_esc_clock, global_windrag_esc_clock, keyboard_fresh_gate,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_CONTROL, VK_MENU, VK_SHIFT};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

struct SnapHookState {
    machine: SnapClassify,
    queue: SnapQueue,
    /// Cached managed origins from the owner's last observation plus the
    /// tiling-active gate. The owner republishes every iteration; the
    /// callback only reads one entry. Owner recheck stays exact.
    origins: HashMap<u64, SnapOrigin>,
    active: bool,
    /// Test-only injection marker. `None` on the product path (filter ALL
    /// injected); `Some(SHORTCUT_PROOF_MARKER)` on the `shortcut-proof`
    /// path (accept exactly the fixed marker, still origin-gated).
    proof_marker: Option<u64>,
    /// Proof-only callback diagnostics for accepted marked events. Always
    /// empty on the product path (no marker, no accepted injected
    /// events, nothing recorded).
    diag: MarkedDiagBuf,
    /// Proof-only callback diagnostics for NON-marked modifier events
    /// (injected-filtered plus physical, modifiers only). Always empty
    /// on the product path. Complements `diag` so live runs see the
    /// actual callback modifier sequence, not just accepted synthetic.
    mod_diag: ModDiagBuf,
    /// Trace-only callback summary for product and proof paths (closed
    /// vocabulary only, see [`super::CallbackDiag`]). Drained trace-only
    /// by the owner; never a synchronous log here.
    cb_diag: CallbackDiagBuf,
    /// Trace-only collection switch for `cb_diag` plus the mask-send
    /// aggregate below. False until the owner enables it right after
    /// install from `--trace`: with no trace the hook skips the timing
    /// read and all callback filter/push work, so a default run pays
    /// one cheap flag read per event. Proof `diag`/`mod_diag` are
    /// unaffected (proof-only audit path, always collected).
    cb_diag_enabled: bool,
    /// Trace-only mask-send aggregate: saturating count of `SendInput`
    /// mask pairs plus the max observed send microseconds. Timed with no
    /// SNAP borrow held, outside `duration_us`; drained trace-only by
    /// the owner alongside `cb_diag`.
    mask_sends: u32,
    mask_send_max_us: u32,
    /// Explicit Esc-cancel edge for an open managed gesture: set once on
    /// a physical Esc down edge in the callback (pass-through always,
    /// never consumed). The owner coordinates it by sequence: every
    /// physical Esc down bumps `esc_seq`, and each gesture START snapshots
    /// the current sequence, so only edges with a newer sequence latch.
    /// An edge before START never cancels a later gesture. Hookless
    /// proof paths never set this, so zero/no-change restore there stays
    /// exactly as before.
    esc_edge: bool,
    /// Monotonic sequence of physical Esc down edges, bumped alongside
    /// `esc_edge`. Never reset by take/clear so START snapshots stay
    /// ordered across pumps.
    esc_seq: u64,
    /// Project-gesture cancel edge for an open Win+Left drag: set on
    /// EVERY Esc down (physical or injected, marked or unmarked),
    /// pass-through always, never consumed, never a command, never
    /// classifier state. The product hook filters injected keys from
    /// all commands; this edge is the one deliberate exception so a
    /// synthetic SendInput Esc still cancels the pointer-tracked
    /// project journey (there is no native modal loop to cancel it, so
    /// without this the synthetic proof could never cancel). A physical
    /// Esc sets both this and the native `esc_edge`; a synthetic Esc
    /// sets only this. The owner latches it per open windrag gesture
    /// whose START sequence predates the edge, exactly like the native
    /// path. Hookless runs never set it.
    windrag_esc_edge: bool,
    /// Monotonic sequence of all Esc down edges (physical plus
    /// injected) backing the windrag cancel edge. Never reset.
    windrag_esc_seq: u64,
}

thread_local! {
    static SNAP: RefCell<Option<SnapHookState>> = const { RefCell::new(None) };
}

/// Direct sample of the process-global native Esc clock for WinEvent
/// START/END. Bumped in hook callbacks at Esc-down time; singletons, so
/// pump restarts can never mismatch clocks.
pub fn shared_esc_seq() -> u64 {
    global_esc_clock().load()
}

/// Direct sample of the process-global windrag Esc clock for mouse
/// Down/Up. Same singleton contract as [`shared_esc_seq`].
pub fn shared_windrag_esc_seq() -> u64 {
    global_windrag_esc_clock().load()
}

/// Pump-thread sample of the tracked Win+Shift chord level: `(win, shift)`
/// from the classifier's callback-observed key state on this thread. The
/// input pump samples this between message drains to wake the owner on a
/// bare-chord transition that queues no batch (modifier-only traffic never
/// reaches an outbox). Read-only, never waits, never touches owner state.
/// `(false, false)` when the hook state is absent, so a hookless pump
/// samples a steady level and never signals.
pub fn chord_levels() -> (bool, bool) {
    SNAP.with(|s| {
        s.borrow()
            .as_ref()
            .map(|st| {
                let (_, _, shift) = st.machine.tracked_modifiers();
                (st.machine.win_held(), shift)
            })
            .unwrap_or((false, false))
    })
}

/// Owner publish: cached origin map for the callback. Cheap clone of the
/// last observation's managed set; exactness stays with the owner's
/// per-intent recheck against fresh observation.
pub fn publish_gate(origins: &HashMap<u64, SnapOrigin>, active: bool) {
    SNAP.with(|s| {
        if let Some(st) = s.borrow_mut().as_mut() {
            st.origins = origins.clone();
            st.active = active;
        }
    });
}

/// Owner-side live config apply for settings changes (takeover/binding
/// updates through the ~100ms pump, never the hook callback): publishes
/// the new takeover/Win+L policy plus the rebound-chord and suppression
/// tables, and drains queued stale actions so nothing dispatches under the
/// new config. In-flight held keys keep their down-time slots: consumed
/// holds ride their stored verdict to the paired key-up (no orphan ups
/// reach the OS) and the Start-menu mask obligation survives the change;
/// repeats ride the stored verdict and only announce while the live
/// combination still matches, so no new stale action dispatches. Returns
/// the drained stale count. Zero when the hook is not installed (hookless
/// runs keep their existing pass-through semantics; the next install uses
/// fresh config).
pub fn apply_live_config(
    config: KeyboardConfig,
    remap: &[ChordRemap],
    disabled: &[ChordDisable],
) -> usize {
    SNAP.with(|s| {
        let mut borrow = s.borrow_mut();
        let Some(st) = borrow.as_mut() else {
            return 0;
        };
        st.machine.set_enabled(config.takeover);
        st.machine.allow_win_l = config.allow_win_l;
        st.machine.set_remap(remap.to_vec());
        st.machine.set_disabled(disabled.to_vec());
        st.queue.drain_stale()
    })
}

/// Owner drain: take up to `limit` queued records, oldest first. The
/// remainder stays queued for the next iteration so an auto-repeat hold
/// cannot starve the tick. `limit` is always at least one.
pub fn drain_up_to(limit: usize) -> Vec<QueuedSnapEvent> {
    SNAP.with(|s| {
        let mut out = Vec::new();
        if let Some(st) = s.borrow_mut().as_mut() {
            let take = limit.max(1);
            while out.len() < take {
                match st.queue.pop_front() {
                    Some(ev) => out.push(ev),
                    None => break,
                }
            }
        }
        out
    })
}

pub fn queue_dropped() -> u32 {
    SNAP.with(|s| s.borrow().as_ref().map(|st| st.queue.dropped).unwrap_or(0))
}

/// Owner drain for the explicit Esc-cancel edge: consume-once, true when
/// a physical Esc down edge arrived since the last take. The owner latches
/// it per open managed gesture whose START sequence predates the edge and
/// treats a latched END as an explicit cancellation. Always false when
/// the hook is not installed, so hookless proof paths keep their
/// existing zero/no-change semantics. The sequence counter is never
/// reset here; compare with [`esc_seq`] for START/END coordination.
pub fn take_esc_edge() -> bool {
    SNAP.with(|s| {
        let mut borrow = s.borrow_mut();
        match borrow.as_mut() {
            Some(st) => std::mem::replace(&mut st.esc_edge, false),
            None => false,
        }
    })
}

/// Drop a stale Esc edge without acting (suspend/teardown path only):
/// an edge that never latched onto a gesture must not leak into the next
/// one. Gesture START never calls this; it snapshots [`esc_seq`] instead
/// so a same-batch edge is ordered rather than discarded.
pub fn clear_esc_edge() {
    SNAP.with(|s| {
        if let Some(st) = s.borrow_mut().as_mut() {
            st.esc_edge = false;
        }
    });
}

/// Ordered windrag invalidation from the input pump (FIFO between
/// snapshots): drops the cached origin so later Downs cannot rebind
/// until a fresh snapshot republishes it. Held-pair state is untouched:
/// consumed holds ride their stored verdict to the paired key-up.
pub fn apply_invalidate_windrag(hwnd: u64) {
    SNAP.with(|s| {
        if let Some(st) = s.borrow_mut().as_mut() {
            st.origins.remove(&hwnd);
        }
    });
}

/// Suspend/teardown path only: drop all cached origins. Held pairs keep
/// their stored verdicts; no new chord binds until a fresh snapshot.
pub fn apply_clear_windrag_origins() {
    SNAP.with(|s| {
        if let Some(st) = s.borrow_mut().as_mut() {
            st.origins.clear();
        }
    });
}

/// Apply one whole owner snapshot outside callbacks (input pump only).
/// Whole-payload FIFO order preserved by the caller; never called from a
/// hook callback. Stale queued actions drain only when the classifier
/// policy actually changed (`policy_changed`); origins/gate-only
/// republishes never flush, so held pairs and legitimate queued actions
/// survive map updates. In-flight held keys always keep their down-time
/// verdicts and the mask obligation survives either way. Returns the
/// stale-drained count for the owner ack.
pub fn apply_snapshot(snap: &SharedSnapshot, policy_changed: bool) -> usize {
    SNAP.with(|s| {
        let mut borrow = s.borrow_mut();
        let Some(st) = borrow.as_mut() else {
            return 0;
        };
        let mut drained = 0;
        if policy_changed {
            st.machine.set_enabled(snap.keyboard.takeover);
            st.machine.allow_win_l = snap.keyboard.allow_win_l;
            st.machine.set_remap(snap.remap.clone());
            st.machine.set_disabled(snap.disabled.clone());
            drained = st.queue.drain_stale();
        }
        st.origins = snap.snap_origins.clone();
        st.active = snap.active;
        st.cb_diag_enabled = snap.trace;
        drained
    })
}

/// Drop a stale windrag Esc edge without acting (suspend/teardown path
/// only), alongside [`clear_esc_edge`]. Gesture START snapshots the
/// sequence instead of clearing, so a same-batch edge stays ordered.
pub fn clear_windrag_esc_edge() {
    SNAP.with(|s| {
        if let Some(st) = s.borrow_mut().as_mut() {
            st.windrag_esc_edge = false;
        }
    });
}
/// Monotonic sequence of physical Esc down edges. The owner snapshots
/// this at each gesture START and latches only edges with a newer
/// sequence, so pre-gesture taps never cancel and fast cancel taps that
/// arrive with the END batch still latch. Zero when the hook is not
/// installed.
pub fn esc_seq() -> u64 {
    SNAP.with(|s| s.borrow().as_ref().map(|st| st.esc_seq).unwrap_or(0))
}

/// Owner drain for the project-gesture cancel edge: consume-once, true
/// when any Esc down (physical or injected) arrived since the last take.
/// The owner latches it per open windrag gesture whose down-time
/// sequence predates the edge. Always false when the hook is not
/// installed. The sequence counter is never reset here; compare with
/// [`windrag_esc_seq`].
pub fn take_windrag_esc_edge() -> bool {
    SNAP.with(|s| {
        let mut borrow = s.borrow_mut();
        match borrow.as_mut() {
            Some(st) => std::mem::replace(&mut st.windrag_esc_edge, false),
            None => false,
        }
    })
}

/// Monotonic sequence of all Esc down edges (physical plus injected).
/// The owner snapshots this at each windrag down and latches only edges
/// with a newer sequence. Zero when the hook is not installed.
pub fn windrag_esc_seq() -> u64 {
    SNAP.with(|s| {
        s.borrow()
            .as_ref()
            .map(|st| st.windrag_esc_seq)
            .unwrap_or(0)
    })
}

/// Arm the Start-menu mask for a consumed project Win+Left gesture.
/// Called by the mouse hook on the same thread after it consumes a
/// Win+Left down: the physical Win hold still needs the E8 pair at
/// Win-up. No-op when the keyboard hook is not installed. Injected
/// Win-ups never reach the reserve (the callback filters injected
/// input), so a synthetic hold never sends the product mask and leaves
/// no stale flag: the classifier never saw the synthetic Win at all.
pub fn arm_windrag_mask() {
    SNAP.with(|s| {
        if let Some(st) = s.borrow_mut().as_mut() {
            st.machine.note_windrag_consumed();
        }
    });
}

/// Bind the chord-time origin: the managed identity under the foreground
/// window right now, or `None` when inactive or the foreground is not
/// managed. One `GetForegroundWindow` plus one map lookup; no enumeration,
/// no identity probes. The owner rechecks the triple against fresh
/// observation before anything dispatches.
fn callback_origin(st: &SnapHookState) -> Option<SnapOrigin> {
    if !st.active {
        return None;
    }
    let foreground = crate::ffi::foreground_window();
    st.origins.get(&foreground).cloned()
}

/// Minimal callback: classification plus one bounded queue push. No
/// Engine, geometry, or log I/O here. A physical Win-up that closes a
/// hold with a consumed chord sends the E8 mask pair first (with no SNAP
/// borrow held); the Win-up itself always passes through.
///
/// Safe policy behind the ffi trampoline: the event arrives pre-copied
/// with no raw pointer, and the verdict selects consume or pass-through.
/// Injected-event filtering and the no-borrow-across-`SendInput`
/// RefCell ordering are unchanged from the previous inline callback.
///
/// Proof-only diagnostics: accepted marked events (shortcut-proof only)
/// record one bounded [`super::MarkedKeyDiag`] each, so live runs can
/// distinguish actual callback modifier delivery from classifier state.
/// Unmarked input is never recorded.
fn handle_key_event(ev: crate::ffi::KeyEvent) -> crate::ffi::HookVerdict {
    use crate::ffi::HookVerdict;
    {
        let vk = ev.vk;
        let scan = ev.scan;
        let flags = ev.flags;
        // Trace-only collection switch: one cheap flag read per event.
        // With no trace the hook skips the timing read and all callback
        // filter/push work below (proof `diag`/`mod_diag` are separate
        // and unaffected).
        let diag_on = SNAP.with(|s| s.borrow().as_ref().is_some_and(|st| st.cb_diag_enabled));
        // Local callback start: `duration_us` below covers our own
        // classification/reserve work before passing onward, never the
        // downstream hook chain (nor any mask `SendInput`, which runs
        // with no borrow held after the reserve and is timed
        // separately in the mask-send aggregate).
        let started: Option<Instant> = diag_on.then(Instant::now);
        // Trace-only native delivery gap: native event timestamp to
        // callback entry, tick-count domain. No timing work unless
        // tracing.
        if diag_on {
            let gap = crate::ffi::tick_count().saturating_sub(ev.time);
            crate::input_handoff::record_delivery_gap_ms(gap);
        }
        let injected = flags & (LLKHF_INJECTED | LLKHF_LOWER_IL_INJECTED) != 0;
        // Own-injection guard BEFORE any SNAP borrow: the SendInput pair
        // below reenters this callback on the same thread, and
        // re-borrowing the RefCell would panic. Product filters ALL
        // injected; shortcut-proof accepts exactly the fixed marker
        // (mask pair uses dwExtraInfo 0, so it still passes through).
        // Accepted marked events fall through as physical; everything
        // else passes through untouched.
        if injected {
            let extra = ev.extra;
            let accepted = SNAP.with(|s| {
                s.borrow()
                    .as_ref()
                    .is_some_and(|st| super::accept_proof_injected(extra, st.proof_marker))
            });
            if !accepted {
                // Trace-only: record marker-rejected injected traffic for
                // the closed vocabulary (modifiers/Win, E8, catalog
                // chords with Win/armed), bounded with no state change,
                // so live runs can tell hook-chain/OS echoes (including
                // the untagged own E8 reentry) apart from classifier
                // state. Ordinary keys skip without a borrow and leave no
                // record. The borrow ends before `CallNextHookEx` below.
                // Skipped entirely unless trace collection is enabled.
                if diag_on && super::is_callback_diag_candidate(vk) {
                    SNAP.with(|s| {
                        let mut borrow = s.borrow_mut();
                        let Some(st) = borrow.as_mut() else {
                            return;
                        };
                        if !st.cb_diag_enabled {
                            return;
                        };
                        let win_before = st.machine.win_held();
                        let armed = st.machine.key_is_down(vk);
                        if !super::callback_diag_allows(vk, win_before, armed, false) {
                            st.cb_diag.note_filtered();
                            return;
                        }
                        let (ctrl, alt, shift) = st.machine.tracked_modifiers();
                        let elapsed = started
                            .as_ref()
                            .map(|t| t.elapsed().as_micros().min(u128::from(u32::MAX)) as u32)
                            .unwrap_or(0);
                        st.cb_diag.push(CallbackDiag {
                            vk,
                            scan,
                            is_up: flags & LLKHF_UP != 0,
                            source: CallbackSource::InjectedFiltered,
                            reason: CallbackReason::InjectedFiltered,
                            win_before,
                            win_after: win_before,
                            shift,
                            ctrl,
                            alt,
                            guard_disagree: false,
                            win_untracked: super::callback_win_untracked(vk, win_before),
                            consumed: false,
                            duration_us: elapsed,
                        });
                    });
                }
                // Proof-only: record marker-rejected injected MODIFIER
                // traffic (modifiers only, bounded, no state change) so
                // live runs can tell hook-chain/OS echoes apart from
                // classifier state. The product path (no marker) records
                // nothing there and still passes through untouched.
                if super::is_proof_mod_vk(vk) {
                    SNAP.with(|s| {
                        let mut borrow = s.borrow_mut();
                        let Some(st) = borrow.as_mut() else {
                            return;
                        };
                        if st.proof_marker.is_none() {
                            return;
                        }
                        let (ctrl, alt, shift) = st.machine.tracked_modifiers();
                        let win = st.machine.win_held();
                        st.mod_diag.push(super::ModTrafficDiag {
                            vk,
                            scan,
                            is_up: flags & LLKHF_UP != 0,
                            source: super::ModSource::InjectedFiltered,
                            win_before: win,
                            win_after: win,
                            shift_before: shift,
                            shift_after: shift,
                            ctrl,
                            alt,
                        });
                    });
                }
                // Project-gesture cancel edge: an injected Esc down still
                // cancels an open Win+Left drag even though injected keys
                // never become commands. Pass-through always, never
                // consumed, never classifier state, never a log: the
                // owner latches it per open windrag gesture only. This
                // is the one deliberate injected exception (the native
                // modal loop cancels synthetics natively, but the
                // stationary project hold has no modal loop).
                if vk == super::VK_ESCAPE && flags & LLKHF_UP == 0 {
                    SNAP.with(|s| {
                        if let Some(st) = s.borrow_mut().as_mut() {
                            st.windrag_esc_edge = true;
                            st.windrag_esc_seq = st.windrag_esc_seq.wrapping_add(1);
                        }
                    });
                    // Direct atomic publish in the callback: the owner
                    // settle sees this edge even if a WinEvent END lands
                    // before the pump forwards anything. The sequence
                    // still decides cancellation, so the flag alone never
                    // cancels.
                    global_windrag_esc_clock().bump();
                    crate::input_handoff::global_windrag_esc_edge()
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                }
                return HookVerdict::Pass;
            }
        }
        // Reaching here with `injected` true means the fixed proof marker
        // was accepted above, which is only possible in shortcut-proof:
        // this is the sole condition that records a proof diagnostic.
        // The trace-only callback summary below records closed-vocabulary
        // events on both paths.
        let marked = injected;
        let is_up = flags & LLKHF_UP != 0;
        // Explicit Esc-cancel edge for an open managed gesture: physical
        // Esc down only (auto-repeat re-sets an already-set edge and
        // bumps the sequence again, harmlessly), never consumed, never
        // logged, never set from injected or proof-marked input. The
        // owner snapshots the sequence at START and latches only newer
        // edges; hookless paths never set it so their semantics are
        // unchanged. A physical Esc down additionally sets the
        // project-gesture cancel edge below so open Win+Left drags
        // cancel through the same keypress.
        if !marked && !is_up && vk == super::VK_ESCAPE {
            SNAP.with(|s| {
                if let Some(st) = s.borrow_mut().as_mut() {
                    st.esc_edge = true;
                    st.esc_seq = st.esc_seq.wrapping_add(1);
                    st.windrag_esc_edge = true;
                    st.windrag_esc_seq = st.windrag_esc_seq.wrapping_add(1);
                }
            });
            // Direct atomic publish in the callback (see the injected
            // path above): no pump-forwarding delay can hide an Esc that
            // lands just before a WinEvent END.
            global_esc_clock().bump();
            global_windrag_esc_clock().bump();
            crate::input_handoff::global_esc_edge()
                .store(true, std::sync::atomic::Ordering::SeqCst);
            crate::input_handoff::global_windrag_esc_edge()
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
        if is_up && super::is_win_vk(vk) {
            let tick = Instant::now();
            let fire = SNAP.with(|s| {
                let mut s = s.borrow_mut();
                let st = s.as_mut()?;
                if marked {
                    let (ctrl, alt, shift) = st.machine.tracked_modifiers();
                    st.diag.push(MarkedKeyDiag {
                        vk,
                        scan,
                        is_up: true,
                        win: st.machine.win_held(),
                        shift_before: shift,
                        shift_after: shift,
                        ctrl,
                        alt,
                        async_shift: false,
                        guard_disagree: false,
                        consumed: false,
                    });
                }
                let installed = st.machine.enabled;
                // Trace-only Win-up summary (closed Win vocabulary, always
                // safe): Win-ups always pass; the mask reserve runs in
                // the same borrow, and the borrow ends before any
                // `SendInput`/`CallNextHookEx`. Duration excludes the mask
                // send (timed separately in the mask-send aggregate) plus
                // the downstream chain.
                let cb_win_before = st.machine.win_held();
                // Proof-only: a non-injected Win-up classifies as physical
                // below (via the reserve) while recording nothing
                // elsewhere; capture its before/after here so the actual
                // callback Win sequence stays complete. Marked Win-ups
                // stay exclusively in the marked buffer above.
                let win_before = if !marked && st.proof_marker.is_some() {
                    Some(cb_win_before)
                } else {
                    None
                };
                let fire = Some(super::win_up_mask_reserve(
                    &mut st.machine,
                    &mut st.queue,
                    vk,
                    installed,
                    tick,
                ));
                {
                    if st.cb_diag_enabled {
                        let (ctrl, alt, shift) = st.machine.tracked_modifiers();
                        let elapsed = started
                            .as_ref()
                            .map(|t| t.elapsed().as_micros().min(u128::from(u32::MAX)) as u32)
                            .unwrap_or(0);
                        st.cb_diag.push(CallbackDiag {
                            vk,
                            scan,
                            is_up: true,
                            source: if marked {
                                CallbackSource::ProofMarked
                            } else {
                                CallbackSource::Physical
                            },
                            reason: CallbackReason::WinUpPass,
                            win_before: cb_win_before,
                            win_after: st.machine.win_held(),
                            shift,
                            ctrl,
                            alt,
                            guard_disagree: false,
                            win_untracked: false,
                            consumed: false,
                            duration_us: elapsed,
                        });
                    }
                }
                if let Some(before) = win_before {
                    let (ctrl, alt, shift) = st.machine.tracked_modifiers();
                    st.mod_diag.push(super::ModTrafficDiag {
                        vk,
                        scan,
                        is_up: true,
                        source: super::ModSource::Physical,
                        win_before: before,
                        win_after: st.machine.win_held(),
                        shift_before: shift,
                        shift_after: shift,
                        ctrl,
                        alt,
                    });
                }
                fire
            });
            if fire == Some(true) {
                // Mask `SendInput` runs with no borrow held; time it as
                // separate trace evidence (trace-only, bounded
                // aggregate), never inside `duration_us`.
                let send_started: Option<Instant> = diag_on.then(Instant::now);
                let sent = send_mask_pair();
                if let Some(t0) = send_started.as_ref() {
                    let send_us = t0.elapsed().as_micros().min(u128::from(u32::MAX)) as u32;
                    SNAP.with(|s| {
                        if let Some(st) = s.borrow_mut().as_mut() {
                            st.mask_sends = st.mask_sends.saturating_add(1);
                            st.mask_send_max_us = st.mask_send_max_us.max(send_us);
                        }
                    });
                }
                SNAP.with(|s| {
                    if let Some(st) = s.borrow_mut().as_mut() {
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
        // Modifiers may already be held when the hook starts, or arrive
        // injected past the callback guard: when the physical async state
        // disagrees on Ctrl/Alt for a FRESH chord down, pass through
        // untouched so preheld unowned modifiers cannot be swallowed.
        // Armed-hold repeats skip the guard (paired-held check first) so
        // a consumed sequence stays swallowed until its matching up.
        // Supported Shift binds from the official async sample instead,
        // so a preheld Shift correctly selects move/sticky/send (fresh
        // downs only; armed holds keep their down-time op via stored
        // origin).
        //
        // The guard and the classification share one borrow so a marked
        // catalog down records its async sample, tracked state, and final
        // consume verdict in a single diagnostic.
        let consume = SNAP.with(|s| {
            let mut s = s.borrow_mut();
            let st = s.as_mut()?;
            let mut async_shift = false;
            let mut guard_disagree = false;
            if !is_up && super::is_chord_vk(vk) {
                let (ctrl, alt, shift) = st.machine.tracked_modifiers();
                let async_down = |key: i32| crate::ffi::async_key_down(key as u32);
                let async_ctrl = async_down(VK_CONTROL as i32);
                let async_alt = async_down(VK_MENU as i32);
                async_shift = async_down(VK_SHIFT as i32);
                let paired_held = st.machine.key_is_down(vk);
                guard_disagree = ((async_ctrl && !ctrl) || (async_alt && !alt)) && !paired_held;
                if !guard_disagree && !paired_held && async_shift != shift {
                    st.machine.sync_shift_from_async(async_shift);
                }
                if marked {
                    st.diag.push(MarkedKeyDiag {
                        vk,
                        scan,
                        is_up: false,
                        win: st.machine.win_held(),
                        shift_before: shift,
                        shift_after: shift,
                        ctrl,
                        alt,
                        async_shift,
                        guard_disagree,
                        consumed: false,
                    });
                }
                if guard_disagree {
                    // Trace-only preheld-guard evidence (chord VKs are
                    // candidates; the privacy gate still applies so a
                    // guard without Win/armed/marked counts filtered).
                    // Skipped unless trace collection is enabled.
                    if st.cb_diag_enabled {
                        let win_g = st.machine.win_held();
                        let armed_g = st.machine.key_is_down(vk);
                        if super::callback_diag_allows(vk, win_g, armed_g, marked) {
                            let elapsed = started
                                .as_ref()
                                .map(|t| t.elapsed().as_micros().min(u128::from(u32::MAX)) as u32)
                                .unwrap_or(0);
                            st.cb_diag.push(CallbackDiag {
                                vk,
                                scan,
                                is_up: false,
                                source: if marked {
                                    CallbackSource::ProofMarked
                                } else {
                                    CallbackSource::Physical
                                },
                                reason: CallbackReason::ModifierGuard,
                                win_before: win_g,
                                win_after: win_g,
                                shift,
                                ctrl,
                                alt,
                                guard_disagree: true,
                                win_untracked: super::callback_win_untracked(vk, win_g),
                                consumed: false,
                                duration_us: elapsed,
                            });
                        } else {
                            st.cb_diag.note_filtered();
                        }
                    }
                    return None;
                }
            }
            let (ctrl_b, alt_b, shift_b) = st.machine.tracked_modifiers();
            let win_b = st.machine.win_held();
            // Armed-hold bit BEFORE `classify_and_queue` below: the
            // classifier clears the hold on chord-up, so reading after
            // would lose a physical chord-up riding an armed hold whose
            // Win-up already ran (diagnostic must still record it).
            let armed_before = st.machine.key_is_down(vk);
            // Shortcut-handling gate (no syscalls beyond the single
            // foreground read inside `callback_origin`): fresh downs
            // consume iff enabled with the gate active; consumed-hold
            // pairs ride the stored down verdict. The publication fence
            // closes the gate further while a newer policy is published
            // but not yet applied, so fresh actions refuse (pass through
            // tracked) instead of consuming under stale policy and
            // getting discarded owner-side. Suspended/fullscreen/
            // gesture/elevated/unmanaged never enter this gate (takeover
            // publication only); the owner rechecks those fresh per
            // intent.
            let gate = keyboard_fresh_gate(st.active);
            let fg = callback_origin(st);
            let tick = Instant::now();
            // Reaching here with `injected` true means the fixed proof
            // marker was accepted above: classify as physical. All other
            // injected events returned early and never touch the queue.
            let out = super::classify_and_queue(
                &mut st.machine,
                &mut st.queue,
                vk,
                is_up,
                fg,
                false,
                tick,
                gate,
            );
            if marked {
                let (_, _, shift_a) = st.machine.tracked_modifiers();
                if !is_up && st.machine.is_owned_physical(vk) {
                    // The guard record just pushed is the newest record:
                    // stamp the consume verdict onto it (same borrow, no
                    // interleaving; classify_and_queue cannot push diag).
                    if let Some(last) = st.diag.back_mut() {
                        last.consumed = out == Some(true);
                        last.shift_after = shift_a;
                    }
                } else {
                    st.diag.push(MarkedKeyDiag {
                        vk,
                        scan,
                        is_up,
                        win: win_b,
                        shift_before: shift_b,
                        shift_after: shift_a,
                        ctrl: ctrl_b,
                        alt: alt_b,
                        async_shift,
                        guard_disagree,
                        consumed: out == Some(true),
                    });
                }
            }
            // Proof-only: a non-injected (physical) modifier reaches here
            // with `marked == false`, classifies above (may move tracked
            // state), and records nothing elsewhere. Capture its
            // before/after so live runs see the actual callback modifier
            // sequence. Non-modifier physical input is never recorded:
            // modifiers carry no content. No async sample is taken here
            // and the verdict below never selects focus/move from it.
            if !marked && st.proof_marker.is_some() && super::is_proof_mod_vk(vk) {
                let (_, _, shift_a) = st.machine.tracked_modifiers();
                st.mod_diag.push(super::ModTrafficDiag {
                    vk,
                    scan,
                    is_up,
                    source: super::ModSource::Physical,
                    win_before: win_b,
                    win_after: st.machine.win_held(),
                    shift_before: shift_b,
                    shift_after: shift_a,
                    ctrl: ctrl_b,
                    alt: alt_b,
                });
            }
            // Trace-only callback summary for the closed vocabulary
            // (product and proof paths). Ordinary keys skip without a
            // record; chord keys without Win/armed/marked count
            // filtered (Win-not-tracked ambiguity volume without
            // content). Same borrow, ends before `CallNextHookEx`.
            // Skipped entirely unless trace collection is enabled.
            // `guard_disagree` is always false here (the guard returns
            // early above), so the reason mirrors `classify_and_queue`
            // only. `armed_before`/`win_b` are pre-classification so a
            // chord-up after an early Win-up stays diagnostic.
            if diag_on && st.cb_diag_enabled && super::is_callback_diag_candidate(vk) {
                if !super::callback_diag_allows(vk, win_b, armed_before, marked) {
                    st.cb_diag.note_filtered();
                } else {
                    let (_, _, shift_a) = st.machine.tracked_modifiers();
                    let reason = if out.is_none() {
                        CallbackReason::Unclassified
                    } else if out == Some(true) {
                        CallbackReason::Consumed
                    } else {
                        CallbackReason::Passed
                    };
                    let elapsed = started
                        .as_ref()
                        .map(|t| t.elapsed().as_micros().min(u128::from(u32::MAX)) as u32)
                        .unwrap_or(0);
                    st.cb_diag.push(CallbackDiag {
                        vk,
                        scan,
                        is_up,
                        source: if marked {
                            CallbackSource::ProofMarked
                        } else {
                            CallbackSource::Physical
                        },
                        reason,
                        win_before: win_b,
                        win_after: st.machine.win_held(),
                        shift: shift_a,
                        ctrl: ctrl_b,
                        alt: alt_b,
                        guard_disagree: false,
                        win_untracked: super::callback_win_untracked(vk, win_b),
                        consumed: out == Some(true),
                        duration_us: elapsed,
                    });
                }
            }
            out
        });
        if consume == Some(true) {
            return HookVerdict::Consume;
        }
    }
    HookVerdict::Pass
}

/// Sends the E8 down/up pair. Must be called with no SNAP borrow held
/// (see `handle_key_event`). Returns the accepted-event count plus whether
/// the bounded partial-release ran. Accepted exception to the no-native-I/O
/// callback rule: the pair must ride the same Win hold at Win-up time.
fn send_mask_pair() -> crate::ffi::KeyPairSend {
    crate::ffi::send_key_pair(super::VK_MASK as u16)
}

/// Mask evidence line for the owner log: arming chord plus SendInput
/// result. No raw keys, no HWNDs.
pub fn mask_evidence(mask: &QueuedMask) -> serde_json::Value {
    let result = if mask.inserted == 2 && !mask.release_sent {
        "mask-ok"
    } else {
        "mask-failed"
    };
    let mut value = match mask.trigger {
        super::MaskTrigger::Snap { op, direction } => serde_json::json!({
            "trigger_op": op.as_str(),
            "trigger_direction": super::direction_name(direction),
        }),
        super::MaskTrigger::Resize { mode, direction } => serde_json::json!({
            "trigger_op": mode.as_str(),
            "trigger_direction": super::direction_name(direction),
        }),
        super::MaskTrigger::Workspace { op, index } => serde_json::json!({
            "trigger_op": op.as_str(),
            "trigger_index": index,
        }),
        super::MaskTrigger::WorkspaceHistory { op } => serde_json::json!({
            "trigger_op": op.as_str(),
        }),
        super::MaskTrigger::WorkspaceSend { delta, follow } => serde_json::json!({
            "trigger_op": if delta < 0 { "send-prev" } else { "send-next" },
            "trigger_follow": follow,
        }),
        super::MaskTrigger::WorkspaceOutput { direction, follow } => serde_json::json!({
            "trigger_op": "send-output",
            "trigger_direction": super::direction_name(direction),
            "trigger_follow": follow,
        }),
        super::MaskTrigger::WorkspaceMigrate { direction } => serde_json::json!({
            "trigger_op": "migrate-workspace",
            "trigger_direction": super::direction_name(direction),
        }),
        super::MaskTrigger::Maximize => serde_json::json!({
            "trigger_op": "maximize",
        }),
        super::MaskTrigger::Fullscreen => serde_json::json!({
            "trigger_op": "fullscreen",
        }),
        super::MaskTrigger::Float => serde_json::json!({
            "trigger_op": "float",
        }),
        super::MaskTrigger::Sticky => serde_json::json!({
            "trigger_op": "sticky",
        }),
        super::MaskTrigger::Orientation => serde_json::json!({
            "trigger_op": "orientation",
        }),
        super::MaskTrigger::WorkspaceTiling => serde_json::json!({
            "trigger_op": "workspace-tiling",
        }),
        super::MaskTrigger::LaunchBrowser => serde_json::json!({
            "trigger_op": "launch-browser",
        }),
        super::MaskTrigger::LaunchTerminal => serde_json::json!({
            "trigger_op": "launch-terminal",
        }),
        super::MaskTrigger::WinDrag => serde_json::json!({
            "trigger_op": "windrag",
        }),
    };
    value["inserted"] = serde_json::Value::from(mask.inserted);
    value["release_sent"] = serde_json::Value::from(mask.release_sent);
    value["result"] = serde_json::Value::from(result);
    value
}

/// Install the low-level hook on the calling thread. The caller must pump
/// messages so the callback runs. Fails closed: install failure is an
/// error, never a silent run without the visible takeover.
pub fn install(config: KeyboardConfig) -> std::result::Result<crate::ffi::SnapKeyHook, String> {
    SNAP.with(|s| {
        *s.borrow_mut() = Some(SnapHookState {
            machine: SnapClassify::new(config),
            queue: SnapQueue::new(),
            origins: HashMap::new(),
            active: false,
            proof_marker: None,
            diag: MarkedDiagBuf::new(),
            mod_diag: ModDiagBuf::new(),
            cb_diag: CallbackDiagBuf::new(),
            cb_diag_enabled: false,
            mask_sends: 0,
            mask_send_max_us: 0,
            esc_edge: false,
            esc_seq: 0,
            windrag_esc_edge: false,
            windrag_esc_seq: 0,
        });
    });
    crate::ffi::set_snapkey_sink(handle_key_event);
    match crate::ffi::install_snapkey_hook() {
        Some(hook) => Ok(hook),
        None => {
            crate::ffi::clear_snapkey_sink();
            SNAP.with(|s| *s.borrow_mut() = None);
            Err("error: SetWindowsHookExW(WH_KEYBOARD_LL) failed".to_owned())
        }
    }
}

/// Test-only install for `shortcut-proof`: identical callback, dispatcher,
/// and mask routines, but synthetic input carrying exactly
/// [`super::SHORTCUT_PROOF_MARKER`] in `dwExtraInfo` classifies as
/// physical (still origin-gated by the owner recheck). All other injected
/// input passes through. Never used by product `tile`.
pub fn install_proof(
    config: KeyboardConfig,
) -> std::result::Result<crate::ffi::SnapKeyHook, String> {
    SNAP.with(|s| {
        *s.borrow_mut() = Some(SnapHookState {
            machine: SnapClassify::new(config),
            queue: SnapQueue::new(),
            origins: HashMap::new(),
            active: false,
            proof_marker: Some(super::SHORTCUT_PROOF_MARKER),
            diag: MarkedDiagBuf::new(),
            mod_diag: ModDiagBuf::new(),
            cb_diag: CallbackDiagBuf::new(),
            cb_diag_enabled: false,
            mask_sends: 0,
            mask_send_max_us: 0,
            esc_edge: false,
            esc_seq: 0,
            windrag_esc_edge: false,
            windrag_esc_seq: 0,
        });
    });
    crate::ffi::set_snapkey_sink(handle_key_event);
    match crate::ffi::install_snapkey_hook() {
        Some(hook) => Ok(hook),
        None => {
            crate::ffi::clear_snapkey_sink();
            SNAP.with(|s| *s.borrow_mut() = None);
            Err("error: SetWindowsHookExW(WH_KEYBOARD_LL) failed".to_owned())
        }
    }
}

/// Owner drain: take all pending proof-only callback diagnostics, oldest
/// first. Empty on the product path (nothing is ever recorded there).
pub fn drain_marked_diag() -> Vec<super::MarkedKeyDiag> {
    SNAP.with(|s| {
        s.borrow_mut()
            .as_mut()
            .map(|st| st.diag.drain())
            .unwrap_or_default()
    })
}

/// Cumulative proof-diagnostic evictions (zero on the product path).
pub fn marked_diag_dropped() -> u32 {
    SNAP.with(|s| s.borrow().as_ref().map(|st| st.diag.dropped).unwrap_or(0))
}

/// Owner drain: take all pending proof-only modifier-traffic
/// diagnostics, oldest first. Empty on the product path (nothing is ever
/// recorded there).
pub fn drain_mod_diag() -> Vec<super::ModTrafficDiag> {
    SNAP.with(|s| {
        s.borrow_mut()
            .as_mut()
            .map(|st| st.mod_diag.drain())
            .unwrap_or_default()
    })
}

/// Cumulative modifier-traffic evictions (zero on the product path).
pub fn mod_diag_dropped() -> u32 {
    SNAP.with(|s| {
        s.borrow()
            .as_ref()
            .map(|st| st.mod_diag.dropped)
            .unwrap_or(0)
    })
}

/// Owner drain: take all pending trace-only callback summaries, oldest
/// first. Bounded at [`super::CALLBACK_DIAG_CAP`]; the owner logs these
/// trace-only.
pub fn drain_callback_diag() -> Vec<super::CallbackDiag> {
    SNAP.with(|s| {
        s.borrow_mut()
            .as_mut()
            .map(|st| st.cb_diag.drain())
            .unwrap_or_default()
    })
}

/// Cumulative callback-summary evictions.
pub fn callback_diag_dropped() -> u32 {
    SNAP.with(|s| {
        s.borrow()
            .as_ref()
            .map(|st| st.cb_diag.dropped)
            .unwrap_or(0)
    })
}

/// Cumulative privacy-denied chord candidates (never recorded).
pub fn callback_diag_filtered() -> u32 {
    SNAP.with(|s| {
        s.borrow()
            .as_ref()
            .map(|st| st.cb_diag.filtered)
            .unwrap_or(0)
    })
}

/// Owner switch for trace-only callback collection (`cb_diag` plus the
/// mask-send aggregate). Applied immediately after install from the
/// owner's `--trace` flag; the hook collects nothing until enabled.
/// Proof `diag`/`mod_diag` are unaffected. Narrow setter (not an
/// install signature change) so the portable product surface is
/// untouched.
pub fn set_callback_diag_enabled(enabled: bool) {
    SNAP.with(|s| {
        if let Some(st) = s.borrow_mut().as_mut() {
            st.cb_diag_enabled = enabled;
        }
    });
}

/// Trace-only mask-send aggregate: `(sends, max_send_us)`. Sends counts
/// `SendInput` mask pairs (saturating); `max_send_us` is the max observed
/// send microseconds with no borrow held. Zero unless trace collection
/// is enabled.
pub fn mask_send_stats() -> (u32, u32) {
    SNAP.with(|s| {
        s.borrow()
            .as_ref()
            .map(|st| (st.mask_sends, st.mask_send_max_us))
            .unwrap_or((0, 0))
    })
}

/// Uninstall and drop hook state. Returns the actual unhook result:
/// process exit releases the hook in any case, but success is never
/// fabricated here. State mutates only on success: a failed unhook
/// retains the full classifier state (including arm/swallow obligations),
/// so a creator-side retry still sees the live gesture and no paired Up
/// can leak through as a passed-through click. The Win-up itself always
/// passed through, so no stuck keys are possible; a partial E8 pair
/// resolves to one bounded release attempt at send time.
pub fn uninstall(hook: &mut crate::ffi::SnapKeyHook) -> bool {
    if !crate::ffi::uninstall_snapkey_hook(hook) {
        return false;
    }
    crate::ffi::clear_snapkey_sink();
    SNAP.with(|s| {
        if let Some(st) = s.borrow_mut().as_mut() {
            st.machine.set_enabled(false);
        }
    });
    SNAP.with(|s| *s.borrow_mut() = None);
    true
}

/// Test-only mask-obligation probe (no hooks, no native calls): true
/// while a consumed gesture (chord or windrag) still owes the Win-up E8
/// pair. Driven directly in callback tests that must not perform a real
/// `SendInput`.
#[cfg(test)]
pub fn test_mask_pending() -> bool {
    SNAP.with(|s| {
        s.borrow()
            .as_ref()
            .is_some_and(|st| st.machine.mask_pending)
    })
}

/// Test-only applied-origin snapshot (no hooks, no native calls): the
/// currently applied chord-origin map for FIFO-order assertions.
#[cfg(test)]
pub fn test_keyboard_origins() -> HashMap<u64, SnapOrigin> {
    SNAP.with(|s| {
        s.borrow()
            .as_ref()
            .map(|st| st.origins.clone())
            .unwrap_or_default()
    })
}

/// Pump-thread drain of this thread's hook-state Esc-cancel edges (no
/// hooks: `(false, false)`). The input pump calls this between message
/// drains to wake the owner once per edge; the product owner latches
/// through the process-global edges, so consuming here steals nothing.
/// Legacy same-thread owners never share this thread's state.
pub fn take_pump_esc_edges() -> (bool, bool) {
    SNAP.with(|s| {
        let mut borrow = s.borrow_mut();
        let Some(st) = borrow.as_mut() else {
            return (false, false);
        };
        let edges = (st.esc_edge, st.windrag_esc_edge);
        st.esc_edge = false;
        st.windrag_esc_edge = false;
        edges
    })
}
/// Test-only hook-state installer (no `SetWindowsHookExW`, no native
/// effects): mirrors [`install`](Self::install)'s TLS init so step tests
/// drive the production apply/drain logic without hooks. Also registers
/// the production policy with the ffi trampoline so
/// [`test_drive_llproc`] drives the real callback path.
#[cfg(test)]
pub fn test_ensure_state(config: KeyboardConfig) {
    crate::ffi::set_snapkey_sink(handle_key_event);
    SNAP.with(|s| {
        *s.borrow_mut() = Some(SnapHookState {
            machine: SnapClassify::new(config),
            queue: SnapQueue::new(),
            origins: HashMap::new(),
            active: false,
            proof_marker: None,
            diag: MarkedDiagBuf::new(),
            mod_diag: ModDiagBuf::new(),
            cb_diag: CallbackDiagBuf::new(),
            cb_diag_enabled: false,
            mask_sends: 0,
            mask_send_max_us: 0,
            esc_edge: false,
            esc_seq: 0,
            windrag_esc_edge: false,
            windrag_esc_seq: 0,
        });
    });
}

/// Test-only state teardown (no native calls).
#[cfg(test)]
pub fn test_clear_state() {
    crate::ffi::clear_snapkey_sink();
    SNAP.with(|s| *s.borrow_mut() = None);
}

/// Test-only queue injection (no hooks, no native calls): push one
/// classifier record straight into the hook queue for drain/offer tests.
/// Mirrors saturation accounting.
#[cfg(test)]
pub fn test_push_queued(event: QueuedSnapEvent) -> bool {
    SNAP.with(|s| {
        s.borrow_mut()
            .as_mut()
            .is_some_and(|st| st.queue.push(event))
    })
}

/// Test-only production-callback drive (no hooks, no `SendInput`): feeds
/// one fabricated low-level event through the real trampoline plus the
/// production policy. Safe scalar inputs only: injected-filtered traffic,
/// Esc downs, and ordinary physical keys. Never drive a Win-up that could
/// reserve the mask (that path performs a real `SendInput`). Up/Down
/// outcomes are the production verdicts (`1` = consumed or swallowed,
/// `CallNextHookEx` result = pass-through).
#[cfg(test)]
pub fn test_drive_llproc(
    vk: u32,
    scan: u32,
    flags: u32,
    extra: u64,
) -> windows_sys::Win32::Foundation::LRESULT {
    crate::ffi::drive_snapkey(vk, scan, flags, extra)
}

#[cfg(test)]
mod sys_tests;
