#![forbid(unsafe_code)]
use super::chords::is_modifier_vk;
use super::*;

/// Proof-only callback diagnostic: one accepted MARKED synthetic event as the
/// LL callback actually saw it (`vk`/`scan` from `KBDLLHOOKSTRUCT`), plus the
/// classifier modifier state before/after and, for catalog key downs, the
/// async-modifier sample and the preheld-guard verdict that gated the event.
///
/// Recorded only in `shortcut-proof` (the only mode whose hook state carries
/// a marker), only for events carrying exactly `SHORTCUT_PROOF_MARKER`, and
/// written only to the proof audit, never to production logs. Real (unmarked)
/// keystrokes are never recorded, so this cannot become raw keystroke
/// logging. Bounded at [`MARKED_DIAG_CAP`] with an explicit drop counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkedKeyDiag {
    /// `vkCode` as delivered to the callback.
    pub vk: u32,
    /// `scanCode` as delivered to the callback.
    pub scan: u32,
    /// `LLKHF_UP` was set.
    pub is_up: bool,
    /// Either Win key tracked held before this event.
    pub win: bool,
    /// Tracked Shift before this event.
    pub shift_before: bool,
    /// Tracked Shift after this event.
    pub shift_after: bool,
    /// Tracked Ctrl/Alt before this event.
    pub ctrl: bool,
    /// Tracked Alt before this event.
    pub alt: bool,
    /// `GetAsyncKeyState(VK_SHIFT)` at a catalog key down; always false for
    /// non-catalog events (no async sample is taken there).
    pub async_shift: bool,
    /// The preheld guard passed this catalog down through (`true`) instead
    /// of classifying it; always false for non-catalog events.
    pub guard_disagree: bool,
    /// `classify_and_queue` returned `Some(true)` for this event (catalog
    /// downs that passed the guard get this stamped after classification).
    pub consumed: bool,
}

/// Bound on proof-only callback diagnostics per hook lifetime.
pub const MARKED_DIAG_CAP: usize = 64;

/// Bounded FIFO for proof-only callback diagnostics. Past capacity the oldest
/// record is evicted and the loss is counted, so the callback fails closed
/// and omissions can never masquerade as delivery.
#[derive(Debug, Default)]
pub struct MarkedDiagBuf {
    inner: std::collections::VecDeque<MarkedKeyDiag>,
    pub dropped: u32,
}

impl MarkedDiagBuf {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: std::collections::VecDeque::with_capacity(MARKED_DIAG_CAP),
            dropped: 0,
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn push(&mut self, diag: MarkedKeyDiag) {
        if self.inner.len() >= MARKED_DIAG_CAP {
            self.inner.pop_front();
            self.dropped += 1;
        }
        self.inner.push_back(diag);
    }

    pub fn drain(&mut self) -> Vec<MarkedKeyDiag> {
        self.inner.drain(..).collect()
    }

    #[cfg(windows)]
    pub(super) fn back_mut(&mut self) -> Option<&mut MarkedKeyDiag> {
        self.inner.back_mut()
    }
}

/// Proof-audit evidence for one marked callback event. Carries the synthetic
/// test chord's key identity and classifier/guard state only: no HWNDs, PIDs,
/// tokens, titles, or any native identity.
#[must_use]
pub fn marked_diag_evidence(diag: &MarkedKeyDiag) -> serde_json::Value {
    serde_json::json!({
        "vk": diag.vk,
        "scan": diag.scan,
        "edge": if diag.is_up { "up" } else { "down" },
        "win": diag.win,
        "shift_before": diag.shift_before,
        "shift_after": diag.shift_after,
        "ctrl": diag.ctrl,
        "alt": diag.alt,
        "async_shift": diag.async_shift,
        "guard_disagree": diag.guard_disagree,
        "consumed": diag.consumed,
    })
}

/// Proof-only source of a NON-marked modifier callback event. Marked
/// (accepted synthetic) events stay exclusively in [`MarkedKeyDiag`]; this
/// covers the complement both live runs need to distinguish actual delivery
/// from classifier state: marker-rejected injected modifiers (passed through,
/// classifier untouched) and non-injected modifiers (classified as physical,
/// may move tracked state while recording nothing elsewhere).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModSource {
    /// Injected but marker-rejected: passed through, classifier untouched.
    InjectedFiltered,
    /// Non-injected: classified as physical; may move tracked modifier state.
    Physical,
}

impl ModSource {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InjectedFiltered => "injected-filtered",
            Self::Physical => "physical",
        }
    }
}

/// Proof-only callback diagnostic for one non-marked MODIFIER event
/// ([`is_proof_mod_vk`]). No catalog keys, no ordinary keys, no content:
/// modifiers carry no typed text. Recorded only in `shortcut-proof` (the only
/// mode whose hook state carries a marker); the product path records nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModTrafficDiag {
    /// `vkCode` as delivered to the callback.
    pub vk: u32,
    /// `scanCode` as delivered to the callback.
    pub scan: u32,
    /// `LLKHF_UP` was set.
    pub is_up: bool,
    /// How the callback sourced this event.
    pub source: ModSource,
    /// Either Win key tracked held before/after this event.
    pub win_before: bool,
    /// Either Win key tracked held after this event.
    pub win_after: bool,
    /// Tracked Shift before this event.
    pub shift_before: bool,
    /// Tracked Shift after this event.
    pub shift_after: bool,
    /// Tracked Ctrl before this event.
    pub ctrl: bool,
    /// Tracked Alt before this event.
    pub alt: bool,
}

/// True for Win keys plus the Shift/Ctrl/Alt families: the only VKs the
/// proof-only modifier-traffic buffer may record. Everything else (catalog
/// keys, E8 mask, ordinary keys) is never recorded there.
#[must_use]
pub fn is_proof_mod_vk(vk: u32) -> bool {
    is_win_vk(vk) || is_modifier_vk(vk)
}

/// Bound on proof-only modifier-traffic diagnostics per hook lifetime.
pub const MOD_DIAG_CAP: usize = 64;

/// Bounded FIFO for proof-only modifier-traffic diagnostics. Past capacity
/// the oldest record is evicted and the loss is counted, so the callback
/// fails closed and omissions can never masquerade as delivery.
#[derive(Debug, Default)]
pub struct ModDiagBuf {
    inner: std::collections::VecDeque<ModTrafficDiag>,
    pub dropped: u32,
}

impl ModDiagBuf {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: std::collections::VecDeque::with_capacity(MOD_DIAG_CAP),
            dropped: 0,
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn push(&mut self, diag: ModTrafficDiag) {
        if self.inner.len() >= MOD_DIAG_CAP {
            self.inner.pop_front();
            self.dropped += 1;
        }
        self.inner.push_back(diag);
    }

    pub fn drain(&mut self) -> Vec<ModTrafficDiag> {
        self.inner.drain(..).collect()
    }
}

/// Proof-audit evidence for one non-marked modifier callback event. Carries
/// key identity (modifiers only) and tracked before/after state: no HWNDs,
/// PIDs, tokens, titles, or any native identity.
#[must_use]
pub fn mod_diag_evidence(diag: &ModTrafficDiag) -> serde_json::Value {
    serde_json::json!({
        "vk": diag.vk,
        "scan": diag.scan,
        "edge": if diag.is_up { "up" } else { "down" },
        "source": diag.source.as_str(),
        "win_before": diag.win_before,
        "win_after": diag.win_after,
        "shift_before": diag.shift_before,
        "shift_after": diag.shift_after,
        "ctrl": diag.ctrl,
        "alt": diag.alt,
    })
}

/// Where one callback event came from, for the trace-only callback summary.
/// `Physical` is a non-injected event; `ProofMarked` is an injected event
/// carrying exactly [`SHORTCUT_PROOF_MARKER`] (only possible in
/// `shortcut-proof`); `InjectedFiltered` passed through untouched by the
/// product injection filter (including the untagged own E8 reentry).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackSource {
    Physical,
    ProofMarked,
    InjectedFiltered,
}

impl CallbackSource {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Physical => "physical",
            Self::ProofMarked => "proof-marked",
            Self::InjectedFiltered => "injected-filtered",
        }
    }
}

/// Why the callback produced its verdict, for the trace-only summary.
/// `ModifierGuard` is the preheld Ctrl/Alt pass-through; `WinUpPass` is a
/// Win-up (always passes, mask handled separately); `Unclassified` never
/// touched the queue (extra modifiers, Win+L gate, non-catalog, or gate-off
/// fresh chords); `Consumed`/`Passed` mirror `classify_and_queue`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackReason {
    InjectedFiltered,
    ModifierGuard,
    Unclassified,
    Consumed,
    Passed,
    WinUpPass,
}

impl CallbackReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InjectedFiltered => "injected-filtered",
            Self::ModifierGuard => "modifier-guard",
            Self::Unclassified => "unclassified",
            Self::Consumed => "consumed",
            Self::Passed => "passed",
            Self::WinUpPass => "win-up-pass",
        }
    }
}

/// Trace-only callback summary for one hook event. `vk`/`scan` are recorded
/// ONLY for the closed vocabulary below (never typed content, HWNDs, PIDs,
/// tokens, or titles): Win keys, the Shift/Ctrl/Alt families, the unassigned
/// E8 mask, and catalog chord keys with Win held/armed or proof-marked.
/// `duration_us` is the local callback elapsed before passing onward
/// (classification/reserve only, excluding any mask `SendInput` plus the
/// downstream hook chain, whose latency is not ours: mask send latency is
/// reported separately via the hook mask-send aggregate, and time spent in
/// hooks below ours is never observable here).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallbackDiag {
    pub vk: u32,
    pub scan: u32,
    pub is_up: bool,
    pub source: CallbackSource,
    pub reason: CallbackReason,
    pub win_before: bool,
    pub win_after: bool,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// The preheld Ctrl/Alt guard fired for this fresh chord down.
    pub guard_disagree: bool,
    /// A catalog chord observed without tracked Win (preheld Win before
    /// install, or a bare repeat after Win-up on an armed hold).
    pub win_untracked: bool,
    /// `classify_and_queue` returned `Some(true)`.
    pub consumed: bool,
    /// Local callback microseconds before passing onward.
    pub duration_us: u32,
}

/// Bound on trace-only callback summaries per hook lifetime.
pub const CALLBACK_DIAG_CAP: usize = 128;

/// Bounded FIFO for trace-only callback summaries. Past capacity the oldest
/// record is evicted and the loss is counted; privacy-denied chord candidates
/// bump `filtered` without recording any key identity. The owner drains this
/// trace-only (never production logs without `--trace`).
#[derive(Debug, Default)]
pub struct CallbackDiagBuf {
    inner: std::collections::VecDeque<CallbackDiag>,
    pub dropped: u32,
    pub filtered: u32,
}

impl CallbackDiagBuf {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: std::collections::VecDeque::with_capacity(CALLBACK_DIAG_CAP),
            dropped: 0,
            filtered: 0,
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn push(&mut self, diag: CallbackDiag) {
        if self.inner.len() >= CALLBACK_DIAG_CAP {
            self.inner.pop_front();
            self.dropped = self.dropped.saturating_add(1);
        }
        self.inner.push_back(diag);
    }

    /// Privacy skip: a chord candidate denied by [`callback_diag_allows`]
    /// (catalog key without Win/armed/marked). No key identity is recorded.
    pub fn note_filtered(&mut self) {
        self.filtered = self.filtered.saturating_add(1);
    }

    pub fn drain(&mut self) -> Vec<CallbackDiag> {
        self.inner.drain(..).collect()
    }
}

/// True for VKs the trace-only callback buffer may ever record: Win keys plus
/// the Shift/Ctrl/Alt families (closed modifier vocabulary, no content), the
/// unassigned E8 mask, and catalog chord keys. Ordinary keys are never
/// candidates: they skip the buffer entirely without a borrow.
#[must_use]
pub fn is_callback_diag_candidate(vk: u32) -> bool {
    is_proof_mod_vk(vk) || vk == VK_MASK || is_chord_vk(vk)
}

/// Privacy gate for a candidate VK: modifiers/Win/E8 are always safe closed
/// vocabulary; catalog chord keys only with Win held, an armed hold
/// (repeat/up riding the down verdict), or a proof-marked chord. Typed
/// letters without Win are never recorded: letter catalog keys without any
/// of these conditions return false, and ordinary keys never reach here.
#[must_use]
pub fn callback_diag_allows(vk: u32, win_before: bool, key_armed: bool, marked: bool) -> bool {
    if is_proof_mod_vk(vk) || vk == VK_MASK {
        return true;
    }
    if is_chord_vk(vk) {
        return marked || win_before || key_armed;
    }
    false
}

/// A catalog chord observed without tracked Win is relevant on either path
/// (preheld Win before install, or a bare repeat after Win-up on an armed
/// hold): proof-marking never supplies the missing Win edge.
#[must_use]
pub fn callback_win_untracked(vk: u32, win_before: bool) -> bool {
    is_chord_vk(vk) && !win_before
}

/// Owner-log evidence for one trace-only callback summary. Closed key
/// vocabulary plus tracked state and local timing only: no HWNDs, PIDs,
/// tokens, titles, or any native identity.
#[must_use]
pub fn callback_diag_evidence(diag: &CallbackDiag) -> serde_json::Value {
    serde_json::json!({
        "vk": diag.vk,
        "scan": diag.scan,
        "edge": if diag.is_up { "up" } else { "down" },
        "source": diag.source.as_str(),
        "reason": diag.reason.as_str(),
        "win_before": diag.win_before,
        "win_after": diag.win_after,
        "shift": diag.shift,
        "ctrl": diag.ctrl,
        "alt": diag.alt,
        "guard_disagree": diag.guard_disagree,
        "win_untracked": diag.win_untracked,
        "consumed": diag.consumed,
        "duration_us": diag.duration_us,
    })
}
