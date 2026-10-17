#![forbid(unsafe_code)]
//! Throwaway Win+Arrow hook spike (not product input).
//!
//! Hook-only run: installs once with no focus-gated start, stays installed for
//! a fixed 60s even if the helper loses focus, then uninstalls for a ~20s
//! release check. Deleted or rewritten when the spike is done.

use serde::Serialize;
use std::collections::VecDeque;

/// Fixed installed window: focus changes never uninstall early.
pub const HOOK_ENABLED_SECONDS: u64 = 60;
/// Post-uninstall window: one release check, then DONE.
pub const RELEASE_CHECK_SECONDS: u64 = 20;
pub const SPIKE_LEDGER_SECONDS: u64 = 150;
pub const SPIKE_HARD_SECONDS: u64 = 140;
/// Bounded preallocated callback event queue. Saturation fails closed
/// (pass-through) with an explicit loss counter, so omissions can never
/// masquerade as successful suppression.
pub const EVENT_QUEUE_CAP: usize = 512;
pub const NUDGE_PX: i32 = 48;

pub const VK_LEFT: u32 = 37;
pub const VK_UP: u32 = 38;
pub const VK_RIGHT: u32 = 39;
pub const VK_DOWN: u32 = 40;
pub const VK_LWIN: u32 = 91;
pub const VK_RWIN: u32 = 92;
pub const VK_SHIFT: u32 = 16;
pub const VK_CONTROL: u32 = 17;
pub const VK_MENU: u32 = 18;
pub const VK_LSHIFT: u32 = 160;
pub const VK_RSHIFT: u32 = 161;
pub const VK_LCONTROL: u32 = 162;
pub const VK_RCONTROL: u32 = 163;
pub const VK_LMENU: u32 = 164;
pub const VK_RMENU: u32 = 165;
/// Unassigned VK used as the Start-menu mask key, per the documented
/// `A_MenuMaskKey` guidance (unassigned `vkE8` preferred over the Ctrl
/// default to avoid modifier side effects). Never a real modifier: the OS
/// sees a keystroke while Win is held, so a consumed chord no longer looks
/// like a naked Win tap, and no modifier state is perturbed.
pub const VK_MASK: u32 = 0xE8;

pub const FOCUS_NOTE: &str =
    "helper must be physically foreground for action; background chords pass through";

pub const CHORD_NAMES: [&str; 4] = ["left", "right", "up", "down"];
pub const CHORD_VKS: [u32; 4] = [VK_LEFT, VK_RIGHT, VK_UP, VK_DOWN];
pub const CHORD_OFFSETS: [(i32, i32); 4] =
    [(-NUDGE_PX, 0), (NUDGE_PX, 0), (0, -NUDGE_PX), (0, NUDGE_PX)];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Arrow {
    Left = 0,
    Right = 1,
    Up = 2,
    Down = 3,
}

impl Arrow {
    #[must_use]
    pub fn all() -> [Arrow; 4] {
        [Arrow::Left, Arrow::Right, Arrow::Up, Arrow::Down]
    }

    #[must_use]
    pub fn vk(self) -> u32 {
        CHORD_VKS[self as usize]
    }

    #[must_use]
    pub fn offset(self) -> (i32, i32) {
        CHORD_OFFSETS[self as usize]
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        CHORD_NAMES[self as usize]
    }

    #[must_use]
    pub fn from_vk(vk: u32) -> Option<Arrow> {
        CHORD_VKS
            .iter()
            .position(|v| *v == vk)
            .map(|i| Arrow::all()[i])
    }
}

#[must_use]
pub fn is_win_vk(vk: u32) -> bool {
    vk == VK_LWIN || vk == VK_RWIN
}

fn arrow_index(arrow: Arrow) -> usize {
    arrow as usize
}

/// Which key edge an approved Win+Arrow event represents. Repeats are held-key
/// auto-repeats; only downs act on geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Down,
    Up,
    Repeat,
}

/// Classifier outcome for one approved Win+Arrow key event. `None` means the
/// input is unrelated (bare arrows, other keys, injected, extra modifiers) and
/// is never logged. `consumed == false` means an approved chord that passes
/// through untouched (background, disabled, or saturated queue); it is logged
/// but never swallowed. `announce == true` (consumed downs/repeats) requests a
/// geometry action with a foreground recheck outside the callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChordEvent {
    pub arrow: Arrow,
    pub edge: Edge,
    pub foreground: bool,
    pub consumed: bool,
    pub announce: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ChordCounts {
    pub down: u32,
    pub up: u32,
    pub repeat: u32,
    pub consumed: u32,
    pub acted: u32,
    pub passed: u32,
}

/// Pure Win+Arrow chord classifier shared by the hook callback and tests.
///
/// Tracks both Win keys plus Ctrl/Alt/Shift families. Arrow downs with any
/// extra modifier pass through untracked (no arrow_down set), so their paired
/// key-up also passes and never steals an unrelated event.
///
/// Every Win+Arrow down/repeat/up is tracked and reported, foreground or
/// background: background chords pass through logged as passed.
/// `arrow_consumed` records the origin of the current hold: only a hold that
/// started in the foreground (while enabled) can consume, so a
/// background-origin sequence never consumes or acts just because it turns
/// foreground mid-hold, and its paired key-up is never stolen. Foreground
/// transitions never disable the machine: the hook stays installed throughout.
/// `mask_pending` arms the Start-menu mask when a consumed chord lands in the
/// current Win hold; `mask_trigger` remembers the arming arrow for evidence.
/// `hold_masked` records that the E8 pair already went out for this hold, so
/// the second Win-up of an L/R pair does not re-mask unless a naked Win
/// auto-repeat re-arms the trigger first. Mask counters stay zero unless a
/// mask is reserved, attempted, or skipped.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct HookClassify {
    win_l: bool,
    win_r: bool,
    ctrl: bool,
    alt: bool,
    shift: bool,
    arrow_down: [bool; 4],
    arrow_consumed: [bool; 4],
    pub enabled: bool,
    pub chords: [ChordCounts; 4],
    mask_pending: bool,
    mask_trigger: Option<Arrow>,
    hold_masked: bool,
    pub mask_attempted: u32,
    pub mask_ok: u32,
    pub mask_failed: u32,
    pub mask_skipped: u32,
}

impl HookClassify {
    #[must_use]
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            ..Self::default()
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn note_acted(&mut self, arrow: Arrow) {
        self.chords[arrow_index(arrow)].acted += 1;
    }

    fn note_win_down(&mut self, vk: u32) {
        let already = if vk == VK_LWIN {
            self.win_l
        } else {
            self.win_r
        };
        if already && self.hold_masked {
            // Naked Win auto-repeat after our mask re-arms the Start trigger.
            self.hold_masked = false;
            self.mask_pending = true;
        }
        if vk == VK_LWIN {
            self.win_l = true;
        } else {
            self.win_r = true;
        }
    }

    /// Physical Win-up bookkeeping + mask-fire decision. Returns the arming
    /// arrow when the caller must send the E8 pair before passing this
    /// Win-up through. The Win-up itself always passes. Hold flags clear
    /// once both Wins are up, so a plain Win tap never masks. `installed`
    /// is false on the test-only `push` path, which updates state only.
    fn win_up_needs_mask(&mut self, vk: u32, installed: bool) -> Option<Arrow> {
        if vk == VK_LWIN {
            self.win_l = false;
        } else {
            self.win_r = false;
        }
        let trigger = if installed && self.mask_pending && !self.hold_masked {
            self.mask_trigger
        } else {
            None
        };
        if trigger.is_some() {
            self.mask_pending = false;
            self.hold_masked = true;
        }
        if !(self.win_l || self.win_r) {
            self.mask_pending = false;
            self.hold_masked = false;
            self.mask_trigger = None;
        }
        trigger
    }

    pub fn push(
        &mut self,
        vk: u32,
        is_up: bool,
        foreground: bool,
        injected: bool,
    ) -> Option<ChordEvent> {
        if injected {
            return None;
        }
        if vk == VK_LWIN {
            if is_up {
                let _ = self.win_up_needs_mask(vk, false);
            } else {
                self.note_win_down(vk);
            }
            return None;
        }
        if vk == VK_RWIN {
            if is_up {
                let _ = self.win_up_needs_mask(vk, false);
            } else {
                self.note_win_down(vk);
            }
            return None;
        }
        if matches!(
            vk,
            VK_SHIFT
                | VK_CONTROL
                | VK_MENU
                | VK_LSHIFT
                | VK_RSHIFT
                | VK_LCONTROL
                | VK_RCONTROL
                | VK_LMENU
                | VK_RMENU
        ) {
            let held = !is_up;
            match vk {
                VK_CONTROL | VK_LCONTROL | VK_RCONTROL => self.ctrl = held,
                VK_MENU | VK_LMENU | VK_RMENU => self.alt = held,
                _ => self.shift = held,
            }
            // Physical modifiers pass through, so the OS sees them while Win
            // is held and the Start trigger is already disguised.
            self.mask_pending = false;
            return None;
        }
        let Some(arrow) = Arrow::from_vk(vk) else {
            // Ordinary keys pass through and disguise Win at the OS level.
            self.mask_pending = false;
            return None;
        };
        let idx = arrow_index(arrow);
        if is_up {
            if !self.arrow_down[idx] {
                return None;
            }
            self.arrow_down[idx] = false;
            let origin = self.arrow_consumed[idx];
            self.arrow_consumed[idx] = false;
            self.chords[idx].up += 1;
            if self.enabled && foreground && origin {
                self.chords[idx].consumed += 1;
                Some(ChordEvent {
                    arrow,
                    edge: Edge::Up,
                    foreground,
                    consumed: true,
                    announce: false,
                })
            } else {
                self.chords[idx].passed += 1;
                // Passed ups reach the OS, which disguises Win by itself.
                // Consumed ups are swallowed above and keep pending armed.
                self.mask_pending = false;
                Some(ChordEvent {
                    arrow,
                    edge: Edge::Up,
                    foreground,
                    consumed: false,
                    announce: false,
                })
            }
        } else {
            if self.ctrl || self.alt || self.shift || !(self.win_l || self.win_r) {
                // Untracked arrows pass through, so the OS sees Win modify
                // something and the pending mask is redundant.
                self.mask_pending = false;
                return None;
            }
            if self.arrow_down[idx] {
                self.chords[idx].repeat += 1;
                // A background-origin hold never consumes mid-hold, even if it
                // turns foreground; a foreground-origin hold passes while
                // background and resumes consuming on return.
                if self.enabled && foreground && self.arrow_consumed[idx] {
                    self.chords[idx].consumed += 1;
                    self.mask_pending = true;
                    self.mask_trigger = Some(arrow);
                    Some(ChordEvent {
                        arrow,
                        edge: Edge::Repeat,
                        foreground,
                        consumed: true,
                        announce: true,
                    })
                } else {
                    self.chords[idx].passed += 1;
                    self.mask_pending = false;
                    Some(ChordEvent {
                        arrow,
                        edge: Edge::Repeat,
                        foreground,
                        consumed: false,
                        announce: false,
                    })
                }
            } else {
                self.arrow_down[idx] = true;
                let origin = self.enabled && foreground;
                self.arrow_consumed[idx] = origin;
                self.chords[idx].down += 1;
                if origin {
                    self.chords[idx].consumed += 1;
                    self.mask_pending = true;
                    self.mask_trigger = Some(arrow);
                    Some(ChordEvent {
                        arrow,
                        edge: Edge::Down,
                        foreground,
                        consumed: true,
                        announce: true,
                    })
                } else {
                    self.chords[idx].passed += 1;
                    self.mask_pending = false;
                    Some(ChordEvent {
                        arrow,
                        edge: Edge::Down,
                        foreground,
                        consumed: false,
                        announce: false,
                    })
                }
            }
        }
    }
}

/// One approved chord captured by the callback. `tick` is `Instant::now()` read
/// in the callback (cheap, monotonic); the wall-clock stamp is derived outside
/// the callback from anchors taken at install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueuedChord {
    pub arrow: Arrow,
    pub edge: Edge,
    pub foreground: bool,
    pub consumed: bool,
    pub announce: bool,
    pub tick: std::time::Instant,
}

/// One Start-menu mask reservation. `trigger` is the arrow of the arming
/// consumed chord (never a physical Win VK or any other key); `tick` is the
/// Win-up instant the pair is sent for. `inserted` is the SendInput
/// accepted-event count (0..=2) and `release_sent` reports the single bounded
/// E8-up attempt after a partial insert. Both are stamped after the send, so
/// a short write can never be logged as success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueuedMask {
    pub trigger: Arrow,
    pub tick: std::time::Instant,
    pub inserted: u8,
    pub release_sent: bool,
}

/// One bounded queue record: either an approved chord or a mask attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueuedEvent {
    Chord(QueuedChord),
    Mask(QueuedMask),
}

/// Bounded preallocated FIFO from the callback to the message loop.
/// `push` refuses past capacity so the callback fails closed; use
/// `record_drop` + pass-through on saturation.
#[derive(Debug)]
pub struct EventQueue {
    inner: VecDeque<QueuedEvent>,
    pub dropped: u32,
}

impl EventQueue {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: VecDeque::with_capacity(EVENT_QUEUE_CAP),
            dropped: 0,
        }
    }

    #[must_use]
    pub fn is_full(&self) -> bool {
        self.inner.len() >= EVENT_QUEUE_CAP
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
    pub fn push(&mut self, ev: QueuedEvent) -> bool {
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

    pub fn pop_front(&mut self) -> Option<QueuedEvent> {
        self.inner.pop_front()
    }

    /// Newest record, for stamping a mask result right after its send. Safe
    /// because reserve, send, and stamp run synchronously on the hook thread
    /// with no drain interleaving; reentrant injected events never touch the
    /// queue.
    pub fn back_mut(&mut self) -> Option<&mut QueuedEvent> {
        self.inner.back_mut()
    }
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Saturation-aware gate shared by the hook callback and tests. Bookkeeping
/// always advances (modifiers, holds, pair origins), but while the queue is
/// full nothing consumes: the approved chord is counted as a loss and passed
/// through so omissions cannot masquerade as suppression. Injected and
/// unrelated inputs never touch the queue or the loss counter. Returns
/// whether the callback must swallow the key event.
pub fn classify_and_queue(
    machine: &mut HookClassify,
    queue: &mut EventQueue,
    vk: u32,
    is_up: bool,
    foreground: bool,
    injected: bool,
    tick: std::time::Instant,
) -> Option<bool> {
    let saturated = queue.is_full();
    let saved = machine.enabled;
    if saturated {
        machine.set_enabled(false);
    }
    let ev = machine.push(vk, is_up, foreground, injected);
    machine.set_enabled(saved);
    let ev = ev?;
    if saturated {
        queue.record_drop();
        return Some(false);
    }
    let queued = queue.push(QueuedEvent::Chord(QueuedChord {
        arrow: ev.arrow,
        edge: ev.edge,
        foreground: ev.foreground,
        consumed: ev.consumed,
        announce: ev.announce,
        tick,
    }));
    if queued {
        Some(ev.consumed)
    } else {
        queue.record_drop();
        Some(false)
    }
}

/// Win-up mask reservation shared by the hook callback and tests. Reserves
/// queue evidence BEFORE any SendInput: when full nothing is injected and
/// the skip is counted, so a send is never unlogged. Returns true when the
/// caller must send the E8 pair. The Win-up itself always passes through.
pub fn win_up_mask_reserve(
    machine: &mut HookClassify,
    queue: &mut EventQueue,
    vk: u32,
    installed: bool,
    tick: std::time::Instant,
) -> bool {
    let Some(trigger) = machine.win_up_needs_mask(vk, installed) else {
        return false;
    };
    if queue.push(QueuedEvent::Mask(QueuedMask {
        trigger,
        tick,
        inserted: 0,
        release_sent: false,
    })) {
        machine.mask_attempted += 1;
        true
    } else {
        // No room: count the skip and restore the hold so a later Win-up in
        // the same hold can retry. Nothing was injected.
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
    machine: &mut HookClassify,
    queue: &mut EventQueue,
    inserted: u8,
    release_sent: bool,
) {
    if inserted == 2 && !release_sent {
        machine.mask_ok += 1;
    } else {
        machine.mask_failed += 1;
    }
    if let Some(QueuedEvent::Mask(slot)) = queue.back_mut() {
        slot.inserted = inserted;
        slot.release_sent = release_sent;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MethodState {
    #[serde(rename = "enabled")]
    Enabled,
    #[serde(rename = "release-check")]
    ReleaseCheck,
    #[serde(rename = "done")]
    Done,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChordReport {
    pub chord: &'static str,
    pub down: u32,
    pub up: u32,
    pub repeat: u32,
    pub delivered: u32,
    pub consumed: u32,
    pub acted: u32,
    pub passed: u32,
    pub skipped: u32,
    pub last_requested: Option<[i32; 4]>,
    pub last_observed: Option<[i32; 4]>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MethodReport {
    pub method: &'static str,
    pub state: MethodState,
    pub hook_installed: Option<bool>,
    pub hook_error: Option<u32>,
    pub queue_dropped: u32,
    pub events_logged: u64,
    pub events_path: Option<String>,
    pub chords: [ChordReport; 4],
    pub mask_attempted: u32,
    pub mask_ok: u32,
    pub mask_failed: u32,
    pub mask_skipped: u32,
    pub release_ok: bool,
    pub release_error: Option<String>,
    pub note: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RestoreReport {
    pub ok: Option<bool>,
    pub error: Option<String>,
    pub readback: Option<[i32; 4]>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpikeReport {
    pub schema: u32,
    pub owner: crate::model::ProcessIdentity,
    pub helper: crate::test_window::WindowSnapshot,
    pub focus_note: &'static str,
    pub method: MethodReport,
    pub restore: RestoreReport,
    pub exit: String,
    pub exit_error: Option<String>,
}

fn blank_chords() -> [ChordReport; 4] {
    Arrow::all().map(|a| ChordReport {
        chord: a.name(),
        down: 0,
        up: 0,
        repeat: 0,
        delivered: 0,
        consumed: 0,
        acted: 0,
        passed: 0,
        skipped: 0,
        last_requested: None,
        last_observed: None,
    })
}

#[must_use]
pub fn new_hook_method() -> MethodReport {
    MethodReport {
        method: "hook",
        state: MethodState::Enabled,
        hook_installed: None,
        hook_error: None,
        queue_dropped: 0,
        events_logged: 0,
        events_path: None,
        chords: blank_chords(),
        mask_attempted: 0,
        mask_ok: 0,
        mask_failed: 0,
        mask_skipped: 0,
        release_ok: false,
        release_error: None,
        note: String::new(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpikeRunOptions {
    pub helper_hwnd: u64,
    pub report: std::path::PathBuf,
}

pub fn parse_spike_run_args(args: &[String]) -> Result<SpikeRunOptions, String> {
    let usage = "usage: run --helper-hwnd HWND --report PATH";
    let mut helper_hwnd: Option<u64> = None;
    let mut report: Option<std::path::PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--helper-hwnd" => {
                i += 1;
                let v = args.get(i).ok_or(usage)?;
                helper_hwnd = Some(crate::test_window::parse_hwnd(v).ok_or(usage)?);
                i += 1;
            }
            "--report" => {
                i += 1;
                let v = args.get(i).ok_or(usage)?;
                report = Some(std::path::PathBuf::from(v));
                i += 1;
            }
            _ => return Err(usage.to_owned()),
        }
    }
    Ok(SpikeRunOptions {
        helper_hwnd: helper_hwnd.ok_or(usage)?,
        report: report.ok_or(usage)?,
    })
}

/// Sibling per-event log for a spike report. Lives in the machine-bound run
/// dir next to the report; the script expects it there.
#[must_use]
pub fn events_path_for(report: &std::path::Path) -> Option<std::path::PathBuf> {
    report.parent().map(|p| p.join("events.jsonl"))
}

#[cfg(windows)]
#[path = "winarrow/sys.rs"]
pub mod sys;
