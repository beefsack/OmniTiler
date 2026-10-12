//! Product keyboard takeover: Win+H/J/K/L and Win+arrows for focus/move,
//! plus Win+Alt(+Shift) chords for keyboard resize.
//!
//! Portable classifier and bounded intent queue; the low-level hook itself
//! lives in [`sys`] (`cfg(windows)` only) and mirrors the accepted
//! `winarrow` spike techniques (origin pairing, both Win keys, preheld
//! modifiers, saturation fail-closed, vkE8 Start-menu mask at Win-up) without
//! modifying the spike.
//!
//! Exact KDE catalog (`kwin/src/plan-adapter-entry.ts`): unshifted Win+key is
//! focus, Win+Shift+key is move, over letters H/J/K/L plus arrow aliases;
//! Win+Alt+key resizes outwards and Win+Shift+Alt+key resizes inwards over
//! the same catalog (dedicated resize intents, never moves).
//! Unshifted Win+L additionally requires explicit opt-in (`allow_win_l`);
//! Win+Shift+L stays approved. Any Ctrl, any Alt outside the documented
//! resize arms, injected input, or unrelated key passes through untracked
//! and never enters the queue or logs.
//!
//! The callback never touches the Engine, geometry, or logs: it classifies one
//! key event, pushes one bounded record, and (Win-up only, with no hook-state
//! borrow held) sends the vkE8 mask pair. The owner drains intents against a
//! fresh complete observation through the retained Engine.

use tiler_core::directional::Direction;

/// Snapshot of the product keyboard policy for one owner run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyboardConfig {
    /// Master takeover switch (CLI `--no-keyboard-snap-takeover` clears it).
    /// Default on; when off the hook is never installed.
    pub takeover: bool,
    /// Explicit opt-in for unshifted Win+L (CLI `--allow-win-l`).
    /// Default off; Win+Shift+L is unaffected.
    pub allow_win_l: bool,
}

impl KeyboardConfig {
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            takeover: false,
            allow_win_l: false,
        }
    }
}

pub const VK_G: u32 = 0x47;
pub const VK_B: u32 = 0x42;
pub const VK_T: u32 = 0x54;
pub const VK_H: u32 = 0x48;
pub const VK_J: u32 = 0x4A;
pub const VK_K: u32 = 0x4B;
pub const VK_L: u32 = 0x4C;
pub const VK_LEFT: u32 = 37;
pub const VK_UP: u32 = 38;
pub const VK_RIGHT: u32 = 39;
pub const VK_DOWN: u32 = 40;
pub const VK_LWIN: u32 = 91;
pub const VK_RWIN: u32 = 92;
/// Escape: observed as a pass-through down edge for explicit gesture
/// cancellation only. Never consumed, never classified as a chord.
pub const VK_ESCAPE: u32 = 27;
pub const VK_M: u32 = 0x4D;
pub const VK_F11: u32 = 0x7A;
/// Orientation-toggle chord key (Win+O, KDE Meta+O parity, item 4).
/// `vk_for_key_name` in settings recognizes `O`; the classifier treats it
/// as an orientation arm only unshifted with no Ctrl/Alt (never a
/// directional/digit/history arm).
pub const VK_O: u32 = 0x4F;
/// Tab chord key for the previous-view toggle (Win+Ctrl+Tab, item 1).
/// `vk_for_key_name` in settings recognizes `Tab`; the classifier treats it
/// as a history arm only with Ctrl held (never a directional/digit arm).
pub const VK_TAB: u32 = 0x09;
pub const VK_SHIFT: u32 = 16;
pub const VK_CONTROL: u32 = 17;
pub const VK_MENU: u32 = 18;
pub const VK_LSHIFT: u32 = 160;
pub const VK_RSHIFT: u32 = 161;
pub const VK_LCONTROL: u32 = 162;
pub const VK_RCONTROL: u32 = 163;
pub const VK_LMENU: u32 = 164;
pub const VK_RMENU: u32 = 165;
/// Unassigned VK for the Start-menu mask pair, same technique as the spike:
// the OS sees a keystroke while Win is held, so a consumed chord no longer
// looks like a naked Win tap, and no modifier state is perturbed.
pub const VK_MASK: u32 = 0xE8;

/// Fixed test-only injection marker for the `shortcut-proof` command
/// (`dwExtraInfo` on the synthetic `SendInput` records). ASCII "TLRPROOF":
/// recognizable in a debugger, never a real input tag. Product `tile`
/// installs with no marker and filters ALL `LLKHF_INJECTED`; only
/// `shortcut-proof` (frozen allowlist, owned helpers) accepts exactly this
/// value, and the owner recheck still gates every intent against fresh
/// observation. Never add marker acceptance to the product path.
pub const SHORTCUT_PROOF_MARKER: u64 = 0x544C52_50524F4F46;

/// Portable test-only injection decision: true only for the exact fixed
/// marker. Product passes `None` (never accepts); `shortcut-proof` passes
/// `Some(SHORTCUT_PROOF_MARKER)` and the caller additionally requires the
/// event's `dwExtraInfo` to equal the marker. Any other value passes through.
#[must_use]
pub const fn accept_proof_injected(extra_info: u64, proof: Option<u64>) -> bool {
    match proof {
        Some(marker) => marker == SHORTCUT_PROOF_MARKER && extra_info == SHORTCUT_PROOF_MARKER,
        None => false,
    }
}

/// Bounded callback intent queue. Saturation fails closed (pass-through) with
/// an explicit loss counter, so omissions can never masquerade as suppression.
pub const INTENT_QUEUE_CAP: usize = 512;
/// Maximum intents dispatched per owner iteration; the remainder stays queued
/// for the next iteration so an auto-repeat hold cannot starve the tick.
pub const MAX_DISPATCH_PER_TICK: usize = 8;

/// Which Engine route a chord takes. Core owns the semantics; this is only
/// the focus/move selector (unshifted vs Shift). Resize intents never ride
/// this enum: they carry their own [`ResizeMode`] so a resize can never be
/// mistaken for a move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapOp {
    Focus,
    Move,
}

impl SnapOp {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Focus => "focus",
            Self::Move => "move",
        }
    }
}

/// Keyboard resize mode: `Outwards` grows towards the direction
/// (Win+Alt+key), `Inwards` shrinks from it (Win+Shift+Alt+key). Fixed at
/// down time from the Shift state plus the explicit rebound action; later
/// modifier flips never rewrite it. Never carried as [`SnapOp::Move`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeMode {
    Outwards,
    Inwards,
}

impl ResizeMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Outwards => "outwards",
            Self::Inwards => "inwards",
        }
    }
}

/// Classifier outcome for one keyboard resize event (Win+Alt+key outwards,
/// Win+Shift+Alt+key inwards, over H/J/K/L plus arrow aliases). Only downs
/// and repeats dispatch, ups close the pair. Held repeats stay swallowed
/// unless the live Win+Alt(+Shift) combination still matches the pinned
/// mode; only the down dispatches a fresh hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResizeIntent {
    pub mode: ResizeMode,
    pub direction: Direction,
    pub edge: SnapEdge,
    pub foreground: bool,
    pub consumed: bool,
    pub announce: bool,
}
/// One classifier remap entry for a rebound shortcut binding: one rebound
/// physical chord routes into the existing action classifier at its canonical
/// virtual key plus explicit action. Entries carry the full modifier shape
/// (Shift/Ctrl/Alt): the rebind's modifiers must match the binding's native
/// arm, so the classifier's modifier-derived op keeps meaning what the
/// binding says. Downs match the entry strictly; ups resolve through the
/// down-time pin, so a mid-hold modifier flip still closes the pair instead
/// of orphaning a stuck down slot (see [`SnapClassify::push`]). Entries are
/// owner state, never callback state: the owner publishes the table and the
/// callback only reads it through the machine (one cheap scan per event).
/// `action` carries the explicit target arm (item 1 history vs item 2
/// follow/stay vs focus/move/digit/toggle arms sharing one VK); unbound stay
/// rows route into their canonical slot (shared with the follow arm on the
/// same key) through the stay action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChordRemap {
    pub from_vk: u32,
    pub from_shift: bool,
    pub from_ctrl: bool,
    pub from_alt: bool,
    pub action: ChordAction,
    pub to_vk: u32,
}

/// One classifier suppression entry for a disabled or rebound-away chord: a
/// physical chord the owner no longer intercepts passes through untracked.
/// Entries are full-modifier physical chords (virtual key plus the Shift/Ctrl/
/// Alt state that selects the arm). Fresh downs check the rebound table first
/// (a rebound claims its physical chord even when the old default is
/// suppressed), then this table; in-flight consumed holds ride their stored
/// down verdict to their paired key-up regardless of later table changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChordDisable {
    pub vk: u32,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

/// Explicit action routing for one rebound chord: which classifier arm the
/// physical chord enters. VK alone cannot identify focus vs relative select
/// vs relative follow vs output follow arms sharing one key; the action pins
/// it. Item 1 uses the three history actions; item 2 adds numbered stay plus
/// relative follow/stay (prev/next each); existing arms keep their
/// meaning through the same plumbing for later items (unbound stay rows ride
/// their canonical slot through the stay action).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChordAction {
    Directional,
    WorkspaceDigit,
    WorkspaceStayDigit,
    WorkspaceSendPrev,
    WorkspaceSendNext,
    WorkspaceSendStayPrev,
    WorkspaceSendStayNext,
    Maximize,
    Fullscreen,
    Float,
    Sticky,
    Orientation,
    LaunchBrowser,
    LaunchTerminal,
    WorkspacePrevious,
    WorkspacePrev,
    WorkspaceNext,
    ResizeOut,
    ResizeIn,
}

impl ChordAction {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Directional => "directional",
            Self::WorkspaceDigit => "workspace-digit",
            Self::WorkspaceStayDigit => "workspace-stay-digit",
            Self::WorkspaceSendPrev => "workspace-send-prev",
            Self::WorkspaceSendNext => "workspace-send-next",
            Self::WorkspaceSendStayPrev => "workspace-send-stay-prev",
            Self::WorkspaceSendStayNext => "workspace-send-stay-next",
            Self::Maximize => "maximize",
            Self::Fullscreen => "fullscreen",
            Self::Float => "float",
            Self::Sticky => "sticky",
            Self::Orientation => "orientation",
            Self::LaunchBrowser => "launch-browser",
            Self::LaunchTerminal => "launch-terminal",
            Self::WorkspacePrevious => "workspace-previous",
            Self::WorkspacePrev => "workspace-prev",
            Self::WorkspaceNext => "workspace-next",
            Self::ResizeOut => "resize-out",
            Self::ResizeIn => "resize-in",
        }
    }
}

/// Down-time physical routing pinned through the matching up: the canonical
/// key plus the explicit action the down routed to. A mid-hold table change
/// can neither orphan the hold nor leak its pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PinnedRoute {
    canon: u32,
    action: ChordAction,
}

/// Physical routing-table size: every chord virtual key fits in one byte.
const PIN_KEYS: usize = 256;

/// Fresh-down routing outcome for one physical chord (see
/// [`SnapClassify::fresh_route`], a private helper).
enum Route {
    /// Routes into the classifier at the canonical key plus explicit action.
    Remap(ChordAction, u32),
    /// Passes through untracked.
    Suppressed,
    /// Classifies as itself.
    Direct,
}

/// Which key edge an approved chord represents. Repeats are held-key
/// auto-repeats; only downs and repeats dispatch, ups just close the pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapEdge {
    Down,
    Up,
    Repeat,
}

impl SnapEdge {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Down => "down",
            Self::Up => "up",
            Self::Repeat => "repeat",
        }
    }
}

/// Wire direction token shared with the Engine parser (`left`/`right`/
/// `up`/`down`). Single source for classifier evidence and dispatch.
#[must_use]
pub const fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::Left => "left",
        Direction::Right => "right",
        Direction::Up => "up",
        Direction::Down => "down",
    }
}

/// Classifier outcome for one approved catalog key event. `None` means the
/// input is unrelated, injected, extra-modified, or Win+L-gated: it passes
/// through untracked and is never logged. `consumed == false` means an
/// approved chord that passes through (takeover or the shortcut gate off)
/// and never dispatches; saturation drops only the queued evidence while
/// still swallowing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapIntent {
    pub op: SnapOp,
    pub direction: Direction,
    pub edge: SnapEdge,
    pub foreground: bool,
    pub consumed: bool,
    pub announce: bool,
}

/// Catalog key index: 0-3 letters H/J/K/L, 4-7 arrows Left/Down/Up/Right.
pub(crate) fn catalog_index(vk: u32) -> Option<usize> {
    match vk {
        VK_H => Some(0),
        VK_J => Some(1),
        VK_K => Some(2),
        VK_L => Some(3),
        VK_LEFT => Some(4),
        VK_DOWN => Some(5),
        VK_UP => Some(6),
        VK_RIGHT => Some(7),
        _ => None,
    }
}

/// Digit virtual keys share the physical key with US shifted symbols:
/// no separate virtual key exists, Shift only flips select into send.
pub const VK_0: u32 = 0x30;
pub const VK_9: u32 = 0x39;

#[must_use]
pub const fn is_digit_vk(vk: u32) -> bool {
    vk >= VK_0 && vk <= VK_9
}

/// True for any chord key the single classifier owns: directional catalog
/// plus workspace digits plus the maximize, fullscreen, float, sticky,
/// orientation, and launcher toggles plus the item 1 history keys (Tab/H/J/K/L/arrows with
/// Ctrl). Modifiers, Win keys, and ordinary keys are not chord keys.
#[must_use]
pub fn is_chord_vk(vk: u32) -> bool {
    catalog_index(vk).is_some()
        || is_digit_vk(vk)
        || is_maximize_vk(vk)
        || is_fullscreen_vk(vk)
        || is_float_vk(vk)
        || is_orientation_vk(vk)
        || is_launch_browser_vk(vk)
        || is_launch_terminal_vk(vk)
        || is_history_vk(vk)
}

/// True for item 1 history chord keys: Tab plus H/J/K/L plus the four arrows.
/// These classify as history only with Ctrl held (no Shift/Alt); without Ctrl
/// the letters/arrows ride their directional arms and Tab passes through.
#[must_use]
pub const fn is_history_vk(vk: u32) -> bool {
    matches!(
        vk,
        VK_TAB | VK_H | VK_J | VK_K | VK_L | VK_LEFT | VK_DOWN | VK_UP | VK_RIGHT
    )
}

/// History slot index for a canonical history key: 0 Tab (Previous), 1 H,
/// 2 K, 3 Left, 4 Up (Prev), 5 J, 6 L, 7 Down, 8 Right (Next).
#[must_use]
pub const fn history_index(vk: u32) -> Option<usize> {
    match vk {
        VK_TAB => Some(0),
        VK_H => Some(1),
        VK_K => Some(2),
        VK_LEFT => Some(3),
        VK_UP => Some(4),
        VK_J => Some(5),
        VK_L => Some(6),
        VK_DOWN => Some(7),
        VK_RIGHT => Some(8),
        _ => None,
    }
}

/// History op for a canonical history key: Tab toggles the previous view,
/// H/K/Left/Up step to the previous ordinal, J/L/Down/Right step next.
#[must_use]
pub const fn history_op_for_vk(vk: u32) -> Option<WorkspaceHistoryOp> {
    match vk {
        VK_TAB => Some(WorkspaceHistoryOp::Previous),
        VK_H | VK_K | VK_LEFT | VK_UP => Some(WorkspaceHistoryOp::Prev),
        VK_J | VK_L | VK_DOWN | VK_RIGHT => Some(WorkspaceHistoryOp::Next),
        _ => None,
    }
}

/// Item 2 relative-send slot index for a canonical key: 0 H, 1 K, 2 Left,
/// 3 Up (previous), 4 J, 5 L, 6 Down, 7 Right (next). Tab never sends.
#[must_use]
pub const fn relative_send_index(vk: u32) -> Option<usize> {
    match vk {
        VK_H => Some(0),
        VK_K => Some(1),
        VK_LEFT => Some(2),
        VK_UP => Some(3),
        VK_J => Some(4),
        VK_L => Some(5),
        VK_DOWN => Some(6),
        VK_RIGHT => Some(7),
        _ => None,
    }
}

/// Item 2 relative-send step for a canonical key: H/K/Left/Up previous (-1),
/// J/L/Down/Right next (+1). Tab and unknown keys refuse.
#[must_use]
pub const fn relative_send_delta(vk: u32) -> Option<i32> {
    match vk {
        VK_H | VK_K | VK_LEFT | VK_UP => Some(-1),
        VK_J | VK_L | VK_DOWN | VK_RIGHT => Some(1),
        _ => None,
    }
}

/// Whether a live modifier/Win combination still matches a pinned relative
/// send: Win+Ctrl+Shift held, no Alt, Win still held. Same
/// swallow-but-don't-dispatch contract as digit repeats; follow/stay rides
/// the pinned intent, never the live modifiers.
fn relative_send_repeat_live(ctrl: bool, shift: bool, alt: bool, win: bool) -> bool {
    ctrl && shift && !alt && win
}

/// True only for the maximize-toggle chord key (Win+M, KDE Meta+M parity).
/// Shift/Ctrl/Alt select the directional/workspace arms instead: Win+Shift+M
/// and any Ctrl/Alt combination pass through untracked.
#[must_use]
pub const fn is_maximize_vk(vk: u32) -> bool {
    vk == VK_M
}

/// True only for the fullscreen-toggle chord key (Win+F11, KDE Meta+F11
/// parity). Like maximize, any Shift/Ctrl/Alt combination passes through
/// untracked: there is no shifted fullscreen arm.
#[must_use]
pub const fn is_fullscreen_vk(vk: u32) -> bool {
    vk == VK_F11
}

/// True only for the float-toggle chord key (Win+G, KDE Meta+G parity).
/// Unshifted G floats; shifted G is the sticky arm (see [`is_sticky_vk`]).
/// Any Ctrl/Alt combination passes through untracked.
#[must_use]
pub const fn is_float_vk(vk: u32) -> bool {
    vk == VK_G
}

/// True only for the sticky-toggle chord key (Win+Shift+G, KDE Meta+Shift+G
/// parity). Shares the G virtual key with float: Shift selects sticky,
/// unshifted selects float. Any Ctrl/Alt combination passes through
/// untracked.
#[must_use]
pub const fn is_sticky_vk(vk: u32) -> bool {
    vk == VK_G
}

/// True only for the orientation-toggle chord key (Win+O, KDE Meta+O
/// parity, item 4). Like maximize, any Shift/Ctrl/Alt combination passes
/// through untracked: there is no shifted orientation arm.
#[must_use]
pub const fn is_orientation_vk(vk: u32) -> bool {
    vk == VK_O
}

/// True only for the default-browser launcher chord key (Win+B). Like
/// maximize, any Shift/Ctrl/Alt combination passes through untracked: there
/// is no shifted launcher arm.
#[must_use]
pub const fn is_launch_browser_vk(vk: u32) -> bool {
    vk == VK_B
}

/// True only for the Windows Terminal launcher chord key (Win+T). Same
/// unshifted-only contract as the browser launcher.
#[must_use]
pub const fn is_launch_terminal_vk(vk: u32) -> bool {
    vk == VK_T
}

/// Direction for a catalog index. Letters and arrows are exact aliases.
fn index_direction(idx: usize) -> Direction {
    match idx {
        0 | 4 => Direction::Left,
        1 | 5 => Direction::Down,
        2 | 6 => Direction::Up,
        _ => Direction::Right,
    }
}

/// True only for the letter L slot (unshifted Win+L is the OS lock chord).
fn is_letter_l(idx: usize) -> bool {
    idx == 3
}

/// Whether a live modifier/Win combination still matches a pinned directional
/// op: no extra modifiers, Win still held, Shift still selecting the pinned
/// op, and the Win+L opt-in still satisfied. Consumed-hold repeats stay
/// swallowed regardless; only this decides the repeat `announce` (dispatch).
fn snap_repeat_live(shift: bool, ctrl: bool, alt: bool, win: bool, op: SnapOp) -> bool {
    !ctrl && !alt && win && (shift == (op == SnapOp::Move))
}

/// Physical Win+L opt-in fence for a pinned directional op: an unshifted
/// (focus) physical L without opt-in never dispatches, so a Shift+L move hold
/// whose Shift is released mid-hold swallows its repeats without dispatching.
/// Takes the physical chord key: a safe rebound into the canonical L slot is
/// not the OS lock chord and always admits.
fn snap_op_admits(physical_vk: u32, op: SnapOp, allow_win_l: bool) -> bool {
    !(op == SnapOp::Focus && physical_vk == VK_L && !allow_win_l)
}

/// Whether a live modifier/Win combination still matches a pinned workspace
/// op: no extra modifiers, Win still held, Shift still selecting the pinned
/// op. Same swallow-but-don't-dispatch contract as [`snap_repeat_live`].
fn digit_repeat_live(shift: bool, ctrl: bool, alt: bool, win: bool, op: WorkspaceOp) -> bool {
    !ctrl && !alt && win && (shift == (op == WorkspaceOp::Send))
}

/// Whether a live modifier/Win combination still matches a pinned resize
/// mode: Win+Alt held, no Ctrl, Win still held, Shift still selecting the
/// pinned mode (unshifted outwards, Shifted inwards). Same
/// swallow-but-don't-dispatch contract as [`snap_repeat_live`]: the repeat
/// arm stays pinned to the required modifiers/action.
fn resize_repeat_live(shift: bool, ctrl: bool, alt: bool, win: bool, mode: ResizeMode) -> bool {
    alt && !ctrl && win && (shift == (mode == ResizeMode::Inwards))
}

/// Resize mode for a Shift state: unshifted Alt grows outwards, Shift+Alt
/// shrinks inwards.
fn resize_mode_for_shift(shift: bool) -> ResizeMode {
    if shift {
        ResizeMode::Inwards
    } else {
        ResizeMode::Outwards
    }
}

/// Explicit resize action for a mode: the bound action carries the mode, so
/// a resize intent can never be disguised as a move.
fn resize_action_for_mode(mode: ResizeMode) -> ChordAction {
    match mode {
        ResizeMode::Outwards => ChordAction::ResizeOut,
        ResizeMode::Inwards => ChordAction::ResizeIn,
    }
}

/// Resize mode for an explicit bound action.
fn resize_mode_for_action(action: ChordAction) -> Option<ResizeMode> {
    match action {
        ChordAction::ResizeOut => Some(ResizeMode::Outwards),
        ChordAction::ResizeIn => Some(ResizeMode::Inwards),
        _ => None,
    }
}

/// Whether a live modifier/Win combination still matches a pinned history
/// op: Win+Ctrl held, no Shift/Alt, Win still held. Same
/// swallow-but-don't-dispatch contract as directional repeats.
fn history_repeat_live(ctrl: bool, shift: bool, alt: bool, win: bool) -> bool {
    ctrl && !shift && !alt && win
}

/// Collision refusal from the live owner's hold shape: same variant and
/// action identity, refreshed edge/foreground, the collider's own consumed
/// verdict, and never dispatched (`announce` is always false, so the owner
/// drain settles it without acting).
fn collide_refusal(
    shape: Classified,
    edge: SnapEdge,
    foreground: bool,
    consumed: bool,
) -> Classified {
    match shape {
        Classified::Snap(intent) => Classified::Snap(SnapIntent {
            op: intent.op,
            direction: intent.direction,
            edge,
            foreground,
            consumed,
            announce: false,
        }),
        Classified::Resize(intent) => Classified::Resize(ResizeIntent {
            mode: intent.mode,
            direction: intent.direction,
            edge,
            foreground,
            consumed,
            announce: false,
        }),
        Classified::Workspace(intent) => Classified::Workspace(WorkspaceIntent {
            op: intent.op,
            index: intent.index,
            follow: intent.follow,
            edge,
            foreground,
            consumed,
            announce: false,
        }),
        Classified::WorkspaceSend(intent) => Classified::WorkspaceSend(WorkspaceSendIntent {
            delta: intent.delta,
            follow: intent.follow,
            edge,
            foreground,
            consumed,
            announce: false,
        }),
        Classified::WorkspaceHistory(intent) => {
            Classified::WorkspaceHistory(WorkspaceHistoryIntent {
                op: intent.op,
                edge,
                foreground,
                consumed,
                announce: false,
            })
        }
        Classified::Maximize(_) => Classified::Maximize(MaximizeIntent {
            edge,
            foreground,
            consumed,
            announce: false,
        }),
        Classified::Orientation(_) => Classified::Orientation(OrientationIntent {
            edge,
            foreground,
            consumed,
            announce: false,
        }),
        Classified::Fullscreen(_) => Classified::Fullscreen(FullscreenIntent {
            edge,
            foreground,
            consumed,
            announce: false,
        }),
        Classified::Float(_) => Classified::Float(FloatIntent {
            edge,
            foreground,
            consumed,
            announce: false,
        }),
        Classified::Sticky(_) => Classified::Sticky(StickyIntent {
            edge,
            foreground,
            consumed,
            announce: false,
        }),
        Classified::Launch(intent) => Classified::Launch(LaunchIntent {
            kind: intent.kind,
            edge,
            foreground,
            consumed,
            announce: false,
        }),
    }
}

/// Start-menu mask trigger matching a hold shape: a consumed collision
/// refusal keeps the same Win-hold mask obligation as the owner (a redundant
/// E8 pair is safe while a naked Win Start is not).
fn collide_trigger(shape: &Classified) -> Option<MaskTrigger> {
    match *shape {
        Classified::Snap(intent) => Some(MaskTrigger::Snap {
            op: intent.op,
            direction: intent.direction,
        }),
        Classified::Resize(intent) => Some(MaskTrigger::Resize {
            mode: intent.mode,
            direction: intent.direction,
        }),
        Classified::Workspace(intent) => Some(MaskTrigger::Workspace {
            op: intent.op,
            index: intent.index,
        }),
        Classified::WorkspaceSend(intent) => Some(MaskTrigger::WorkspaceSend {
            delta: intent.delta,
            follow: intent.follow,
        }),
        Classified::WorkspaceHistory(intent) => {
            Some(MaskTrigger::WorkspaceHistory { op: intent.op })
        }
        Classified::Maximize(_) => Some(MaskTrigger::Maximize),
        Classified::Orientation(_) => Some(MaskTrigger::Orientation),
        Classified::Fullscreen(_) => Some(MaskTrigger::Fullscreen),
        Classified::Float(_) => Some(MaskTrigger::Float),
        Classified::Sticky(_) => Some(MaskTrigger::Sticky),
        Classified::Launch(intent) => Some(match intent.kind {
            LaunchKind::Browser => MaskTrigger::LaunchBrowser,
            LaunchKind::Terminal => MaskTrigger::LaunchTerminal,
        }),
    }
}

#[must_use]
pub fn is_win_vk(vk: u32) -> bool {
    vk == VK_LWIN || vk == VK_RWIN
}

fn is_modifier_vk(vk: u32) -> bool {
    matches!(
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
    )
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SnapCounts {
    pub down: u32,
    pub up: u32,
    pub repeat: u32,
    pub consumed: u32,
    pub passed: u32,
}

/// Workspace digit op: unshifted selects an existing workspace, Shift sends
/// the focused tiled window. Single definition for hook dispatch and the
/// portable session policy (re-exported through `workspace::DigitOp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceOp {
    Select,
    Send,
}

impl WorkspaceOp {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Select => "select",
            Self::Send => "send",
        }
    }
}

/// Classifier outcome for one workspace digit event. Edges reuse the snap
/// vocabulary: only downs and repeats dispatch, ups close the pair.
/// `follow` is pinned at down time from the explicit action (direct
/// Shift+digit follows; the unbound stay rows rebind through
/// `WorkspaceStayDigit` and stay); later modifier flips never rewrite it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceIntent {
    pub op: WorkspaceOp,
    pub index: u8,
    pub follow: bool,
    pub edge: SnapEdge,
    pub foreground: bool,
    pub consumed: bool,
    pub announce: bool,
}

/// Classifier outcome for one item 2 relative workspace-send event
/// (Win+Ctrl+Shift+H/K/Left/Up previous, J/L/Down/Right next). Only downs
/// and repeats dispatch, ups close the pair. `delta` is -1 (previous) or +1
/// (next) from the canonical key; `follow` is pinned at down time from the
/// explicit action (direct chords follow; the unbound stay rows rebind
/// through the stay actions and stay).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceSendIntent {
    pub delta: i32,
    pub follow: bool,
    pub edge: SnapEdge,
    pub foreground: bool,
    pub consumed: bool,
    pub announce: bool,
}

/// Item 1 history op: `Previous` toggles the previous view (Win+Ctrl+Tab),
/// `Prev` steps to the previous ordinal (Win+Ctrl+H/K/Left/Up), `Next` steps
/// to the next ordinal (Win+Ctrl+J/L/Down/Right). Fixed at down time from the
/// canonical key; later modifier flips never rewrite it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceHistoryOp {
    Previous,
    Prev,
    Next,
}

impl WorkspaceHistoryOp {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Previous => "previous",
            Self::Prev => "prev",
            Self::Next => "next",
        }
    }
}

/// Classifier outcome for one workspace history event (item 1). Only downs
/// dispatch (relative-step repeats re-dispatch while live), ups close the
/// pair; held repeats of the Previous toggle stay swallowed like maximize
/// (no re-toggle, no view pingpong).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceHistoryIntent {
    pub op: WorkspaceHistoryOp,
    pub edge: SnapEdge,
    pub foreground: bool,
    pub consumed: bool,
    pub announce: bool,
}

/// Classifier outcome for one maximize-toggle event (Win+M, KDE Meta+M
/// parity). The toggle carries no direction: only downs and repeats
/// dispatch, ups close the pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaximizeIntent {
    pub edge: SnapEdge,
    pub foreground: bool,
    pub consumed: bool,
    pub announce: bool,
}

/// Classifier outcome for one fullscreen-toggle event (Win+F11, KDE
/// Meta+F11 parity). The toggle carries no direction: only downs and repeats
/// dispatch, ups close the pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FullscreenIntent {
    pub edge: SnapEdge,
    pub foreground: bool,
    pub consumed: bool,
    pub announce: bool,
}

/// Classifier outcome for one float-toggle event (Win+G, KDE Meta+G
/// parity). The toggle carries no direction: only the down dispatches, ups
/// close the pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FloatIntent {
    pub edge: SnapEdge,
    pub foreground: bool,
    pub consumed: bool,
    pub announce: bool,
}

/// Classifier outcome for one sticky-toggle event (Win+Shift+G, KDE
/// Meta+Shift+G parity). Same edge contract as float: only the down
/// dispatches, ups close the pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StickyIntent {
    pub edge: SnapEdge,
    pub foreground: bool,
    pub consumed: bool,
    pub announce: bool,
}

/// Classifier outcome for one orientation-toggle event (Win+O, KDE Meta+O
/// parity, item 4). The toggle carries no direction: only the down
/// dispatches, ups close the pair. Held repeats stay swallowed like
/// maximize/float (a held O would otherwise pingpong the parent axis);
/// repeat re-dispatch selection stays native-journey TBD (tentative).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrientationIntent {
    pub edge: SnapEdge,
    pub foreground: bool,
    pub consumed: bool,
    pub announce: bool,
}

/// Launcher selector: default browser (Win+B) or Windows Terminal (Win+T).
/// The kind pins at down time from the explicit bound action; later modifier
/// flips never rewrite it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LaunchKind {
    Browser,
    Terminal,
}

impl LaunchKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Browser => "browser",
            Self::Terminal => "terminal",
        }
    }
}

/// Classifier outcome for one launcher event (Win+B browser, Win+T
/// terminal). The launcher carries no direction and needs no managed
/// origin: only the down dispatches (the owner spawns without touching any
/// window), ups close the pair, held repeats stay swallowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaunchIntent {
    pub kind: LaunchKind,
    pub edge: SnapEdge,
    pub foreground: bool,
    pub consumed: bool,
    pub announce: bool,
}

/// Unified classifier outcome: exactly one of directional, resize,
/// workspace digit, workspace history, maximize, fullscreen, or
/// float/sticky. One machine, one modifier/mask authority; history shares
/// Win/Shift/Ctrl/Alt tracking, origin pairing, saturation, and the E8 mask
/// with H/J/K/L/arrows and digits. Ctrl selects the history arm (existing
/// digit/directional arms refuse Ctrl); Alt selects the resize arm (other
/// arms refuse Alt).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classified {
    Snap(SnapIntent),
    Resize(ResizeIntent),
    Workspace(WorkspaceIntent),
    WorkspaceSend(WorkspaceSendIntent),
    WorkspaceHistory(WorkspaceHistoryIntent),
    Maximize(MaximizeIntent),
    Fullscreen(FullscreenIntent),
    Float(FloatIntent),
    Sticky(StickyIntent),
    Orientation(OrientationIntent),
    Launch(LaunchIntent),
}

impl Classified {
    #[must_use]
    pub const fn consumed(self) -> bool {
        match self {
            Self::Snap(intent) => intent.consumed,
            Self::Resize(intent) => intent.consumed,
            Self::Workspace(intent) => intent.consumed,
            Self::WorkspaceSend(intent) => intent.consumed,
            Self::WorkspaceHistory(intent) => intent.consumed,
            Self::Maximize(intent) => intent.consumed,
            Self::Fullscreen(intent) => intent.consumed,
            Self::Float(intent) => intent.consumed,
            Self::Sticky(intent) => intent.consumed,
            Self::Orientation(intent) => intent.consumed,
            Self::Launch(intent) => intent.consumed,
        }
    }

    #[must_use]
    pub const fn announce(self) -> bool {
        match self {
            Self::Snap(intent) => intent.announce,
            Self::Resize(intent) => intent.announce,
            Self::Workspace(intent) => intent.announce,
            Self::WorkspaceSend(intent) => intent.announce,
            Self::WorkspaceHistory(intent) => intent.announce,
            Self::Maximize(intent) => intent.announce,
            Self::Fullscreen(intent) => intent.announce,
            Self::Float(intent) => intent.announce,
            Self::Sticky(intent) => intent.announce,
            Self::Orientation(intent) => intent.announce,
            Self::Launch(intent) => intent.announce,
        }
    }
}

/// Which chord armed the Start-menu mask. Digits, history, resize, maximize,
/// fullscreen, and float arm it exactly like directional chords: any consumed
/// chord in the Win hold needs the E8 pair at Win-up, or the OS opens Start.
/// `WinDrag` arms it for the project-driven Win+Left stationary gesture
/// (parity item 7 second unit): the mouse hook consumes the click, so the
/// keyboard classifier never sees a chord, but the Win hold still needs the
/// same mask at release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskTrigger {
    Snap {
        op: SnapOp,
        direction: Direction,
    },
    Resize {
        mode: ResizeMode,
        direction: Direction,
    },
    Workspace {
        op: WorkspaceOp,
        index: u8,
    },
    WorkspaceSend {
        delta: i32,
        follow: bool,
    },
    WorkspaceHistory {
        op: WorkspaceHistoryOp,
    },
    Maximize,
    Fullscreen,
    Float,
    Sticky,
    Orientation,
    LaunchBrowser,
    LaunchTerminal,
    WinDrag,
}

/// Pure product chord classifier. Tracks both Win keys plus the Shift family
/// (chord selector) and Ctrl/Alt families (extra modifiers force
/// pass-through). Per-key down state carries the verdict of the hold: only a
/// hold that started consumed (takeover on with the shortcut gate active) can
/// consume, so a gate-off-origin sequence never consumes mid-hold and its
/// paired key-up is never stolen. Fresh downs consume iff `enabled` (takeover)
/// and the shortcut gate (`gate_active` plus the callback-passed gate) both
/// hold; consumed-hold repeats/ups ride the stored down verdict across later
/// gate transitions, and new chords while the gate is off pass through.
/// Unmanaged/unadmitted/shell foreground, fullscreen suspension, gesture, and
/// elevated foreground still consume while the gate holds (the owner rechecks
/// fresh identity/scope/elevation/fullscreen/gesture before any action, and
/// never manipulates protected windows). The op is fixed at down time from
/// the Shift state, so releasing Shift before the key-up cannot flip focus
/// into move (or select into send): repeats of a consumed hold stay swallowed
/// until the matching up, but only announce (dispatch) while the live
/// modifier/Win combination still matches the pinned op, so a bare-key
/// repeat after Win-up never re-dispatches.
///
/// `mask_pending` arms the Start-menu mask when a consumed chord lands in the
/// current Win hold. Unlike the spike, no passing transition disarms the
/// mask: a redundant E8 pair is safe while a naked Win Start is not, so a
/// consumed hold keeps its mask across foreground/gate changes, extra
/// modifiers, and ordinary keys until Win-up fires once per hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapClassify {
    win_l: bool,
    win_r: bool,
    shift: bool,
    ctrl: bool,
    alt: bool,
    key_down: [bool; 8],
    key_origin: [bool; 8],
    key_op: [Option<SnapOp>; 8],
    digit_down: [bool; 10],
    digit_origin: [bool; 10],
    digit_op: [Option<WorkspaceOp>; 10],
    digit_follow: [bool; 10],
    /// Pinned Ctrl state per digit hold: numbered stay rides Win+Shift and
    /// additionally the Win+Ctrl+Shift backlog arm, so the repeat dispatch
    /// must follow the pinned arm (a mid-hold Ctrl flip swallows without
    /// dispatching, never leaking a repeat or flipping into follow).
    /// Follow/select holds always pin false via the fresh-down guard.
    digit_ctrl: [bool; 10],
    send_down: [bool; 8],
    send_origin: [bool; 8],
    send_follow: [bool; 8],
    send_delta: [i32; 8],
    maximize_down: bool,
    maximize_origin: bool,
    fullscreen_down: bool,
    fullscreen_origin: bool,
    float_down: bool,
    float_origin: bool,
    sticky_down: bool,
    sticky_origin: bool,
    orientation_down: bool,
    orientation_origin: bool,
    launch_browser_down: bool,
    launch_browser_origin: bool,
    launch_terminal_down: bool,
    launch_terminal_origin: bool,
    resize_down: [bool; 8],
    resize_origin: [bool; 8],
    resize_mode: [Option<ResizeMode>; 8],
    pub enabled: bool,
    /// Shortcut-handling gate published by the owner (takeover plus the
    /// eventual pause source, e.g. Xbox detection). Fresh downs consume iff
    /// `enabled && gate_active` (combined with the callback-passed gate in
    /// [`classify_and_queue`]); consumed-hold repeats/ups ride the stored
    /// down verdict across later gate transitions. Defaults on so pure
    /// classifier tests keep working; the hook path sets it per event from
    /// the cached publish.
    pub gate_active: bool,
    pub allow_win_l: bool,
    /// Owner-published rebound-chord table (empty means no rebinds): one rebound
    /// physical chord translates to its canonical virtual key plus explicit
    /// action on entry, so physical tracking, pairing, release, and repeat
    /// safety ride the existing per-key slots unchanged. Full-modifier: the
    /// rebind's Shift/Ctrl/Alt must match the binding's native arm.
    vk_remap: Vec<ChordRemap>,
    /// Owner-published suppression table (empty means everything kept):
    /// disabled or rebound-away physical chords pass through untracked.
    /// Checked after the rebound table, fresh downs only; paired key-ups ride
    /// their stored down verdict.
    vk_disabled: Vec<ChordDisable>,
    /// Down-time physical routing pinned through the matching up: index by
    /// physical virtual key (all chord keys fit in one byte), value is the
    /// canonical key plus explicit action the down routed to. A mid-hold
    /// table change can neither orphan the hold (ups resolve through the pin)
    /// nor leak its pair (repeats ride the stored verdict with dispatch gated
    /// on the chord still routing live), and the Start-menu mask obligation
    /// survives untouched. Only a hold the family actually owns pins: refused
    /// fresh downs (bare, extra-modified, arm-refused, fenced, suppressed,
    /// gated off) leave no pin, so their paired key-up passes through as well.
    phys_pin: [Option<PinnedRoute>; 256],
    /// Live safe-refusal holds per physical key: a different physical routing
    /// to an already-held canonical is swallowed here (consumed, never
    /// dispatched) with its own down-time verdict until its paired key-up,
    /// never touching the canonical slot. Pairs can therefore neither be
    /// stolen nor orphaned, whatever the release order or mid-hold table
    /// changes; each entry clears on its own physical's key-up.
    collide: [Option<Classified>; 256],
    pub counts: [SnapCounts; 8],
    pub resize_counts: [SnapCounts; 8],
    pub digit_counts: [SnapCounts; 10],
    pub max_counts: SnapCounts,
    pub fullscreen_counts: SnapCounts,
    pub float_counts: SnapCounts,
    pub sticky_counts: SnapCounts,
    pub orientation_counts: SnapCounts,
    pub launch_browser_counts: SnapCounts,
    pub launch_terminal_counts: SnapCounts,
    hist_down: [bool; 9],
    hist_origin: [bool; 9],
    hist_op: [Option<WorkspaceHistoryOp>; 9],
    pub hist_counts: [SnapCounts; 9],
    mask_pending: bool,
    mask_trigger: Option<MaskTrigger>,
    hold_masked: bool,
    pub mask_attempted: u32,
    pub mask_ok: u32,
    pub mask_failed: u32,
    pub mask_skipped: u32,
}

/// G-key arm selector: float (Win+G, KDE Meta+G) or sticky (Win+Shift+G,
/// KDE Meta+Shift+G). The op fixes at down time; repeats ride the armed hold
/// regardless of later Shift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GArm {
    Float,
    Sticky,
}

impl SnapClassify {
    #[must_use]
    pub fn new(config: KeyboardConfig) -> Self {
        Self {
            win_l: false,
            win_r: false,
            shift: false,
            ctrl: false,
            alt: false,
            key_down: [false; 8],
            key_origin: [false; 8],
            key_op: [None; 8],
            digit_down: [false; 10],
            digit_origin: [false; 10],
            digit_op: [None; 10],
            digit_follow: [true; 10],
            digit_ctrl: [false; 10],
            send_down: [false; 8],
            send_origin: [false; 8],
            send_follow: [true; 8],
            send_delta: [0; 8],
            maximize_down: false,
            maximize_origin: false,
            fullscreen_down: false,
            fullscreen_origin: false,
            float_down: false,
            float_origin: false,
            sticky_down: false,
            sticky_origin: false,
            orientation_down: false,
            orientation_origin: false,
            launch_browser_down: false,
            launch_browser_origin: false,
            launch_terminal_down: false,
            launch_terminal_origin: false,
            resize_down: [false; 8],
            resize_origin: [false; 8],
            resize_mode: [None; 8],
            enabled: config.takeover,
            gate_active: true,
            allow_win_l: config.allow_win_l,
            vk_remap: Vec::new(),
            vk_disabled: Vec::new(),
            phys_pin: [None; 256],
            collide: [None; 256],
            counts: [SnapCounts::default(); 8],
            resize_counts: [SnapCounts::default(); 8],
            digit_counts: [SnapCounts::default(); 10],
            max_counts: SnapCounts::default(),
            fullscreen_counts: SnapCounts::default(),
            float_counts: SnapCounts::default(),
            sticky_counts: SnapCounts::default(),
            orientation_counts: SnapCounts::default(),
            launch_browser_counts: SnapCounts::default(),
            launch_terminal_counts: SnapCounts::default(),
            hist_down: [false; 9],
            hist_origin: [false; 9],
            hist_op: [None; 9],
            hist_counts: [SnapCounts::default(); 9],
            mask_pending: false,
            mask_trigger: None,
            hold_masked: false,
            mask_attempted: 0,
            mask_ok: 0,
            mask_failed: 0,
            mask_skipped: 0,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_gate_active(&mut self, gate_active: bool) {
        self.gate_active = gate_active;
    }

    /// Publish the owner-side rebound table. Takes effect on the next event;
    /// in-flight held keys keep their down-time slots: consumed holds ride
    /// their stored verdict to the paired key-up, and the Start-menu mask
    /// obligation survives, so no orphan key-up reaches the OS and no naked
    /// Win tap opens Start. Down-time physical routing survives a removed
    /// mapping through the paired release; held repeats keep their consumed
    /// verdict and dispatch only while the pinned route still matches.
    pub fn set_remap(&mut self, remap: Vec<ChordRemap>) {
        self.vk_remap = remap;
    }

    /// Publish the owner-side suppression table (disabled or rebound-away
    /// physical chords). Same in-flight contract as [`SnapClassify::set_remap`].
    pub fn set_disabled(&mut self, disabled: Vec<ChordDisable>) {
        self.vk_disabled = disabled;
    }

    /// True when the physical chord is suppressed: it passes through untracked
    /// instead of entering the classifier. Rebound chords never reach here
    /// (the rebound table wins); paired key-ups never reach here either (they
    /// ride their stored down verdict).
    fn is_suppressed(&self, vk: u32, shift: bool, ctrl: bool, alt: bool) -> bool {
        self.vk_disabled.iter().any(|entry| {
            entry.vk == vk && entry.shift == shift && entry.ctrl == ctrl && entry.alt == alt
        })
    }

    /// True for any physical key the classifier owns: catalog chord keys plus
    /// rebound sources. The callback's preheld-modifier guard and async Shift
    /// sync must treat rebound sources exactly like catalog keys.
    #[must_use]
    pub fn is_owned_physical(&self, vk: u32) -> bool {
        is_chord_vk(vk) || self.vk_remap.iter().any(|entry| entry.from_vk == vk)
    }

    /// Fresh-down routing for one physical chord at its live modifier state:
    /// rebound wins over suppression; suppression passes through; otherwise
    /// the physical key classifies as itself.
    fn fresh_route(&self, physical: u32, shift: bool, ctrl: bool, alt: bool) -> Route {
        if let Some(entry) = self.vk_remap.iter().find(|entry| {
            entry.from_vk == physical
                && entry.from_shift == shift
                && entry.from_ctrl == ctrl
                && entry.from_alt == alt
        }) {
            return Route::Remap(entry.action, entry.to_vk);
        }
        if self.is_suppressed(physical, shift, ctrl, alt) {
            return Route::Suppressed;
        }
        Route::Direct
    }

    /// True when the physical chord still routes to the pinned canonical
    /// route under the live tables. Pinned-hold repeats dispatch only while
    /// this holds; otherwise they stay swallowed without dispatching.
    fn route_matches(&self, physical: u32, pinned: PinnedRoute) -> bool {
        match self.fresh_route(physical, self.shift, self.ctrl, self.alt) {
            Route::Remap(action, target) => action == pinned.action && target == pinned.canon,
            Route::Direct => {
                physical == pinned.canon && self.direct_action(physical) == Some(pinned.action)
            }
            Route::Suppressed => false,
        }
    }

    /// Explicit action for a direct (non-remapped) physical chord at its live
    /// modifiers: item 2 relative send wins on Win+Ctrl+Shift, history wins on
    /// Win+Ctrl (no Shift/Alt), resize wins on Win+Alt([+Shift]) with no Ctrl,
    /// otherwise the existing VK-derived arm. `None` means the chord refuses
    /// (bare, extra-modified, or arm-refused) and leaves no pin. Fresh Alt is
    /// accepted only here on catalog keys through the documented resize arms;
    /// any other Alt chord (unknown key, extra Ctrl) refuses.
    fn direct_action(&self, physical: u32) -> Option<ChordAction> {
        let win = self.win_l || self.win_r;
        if !win {
            return None;
        }
        // Item 2 relative send: Win+Ctrl+Shift with no Alt on a relative key.
        // Direct chords follow; stay rides only the explicit stay actions
        // through the rebound table (unbound by default).
        if self.ctrl
            && self.shift
            && !self.alt
            && let Some(delta) = relative_send_delta(physical)
        {
            return Some(if delta < 0 {
                ChordAction::WorkspaceSendPrev
            } else {
                ChordAction::WorkspaceSendNext
            });
        }
        // Item 1 history: Win+Ctrl with no Shift/Alt on a history key.
        if self.ctrl
            && !self.shift
            && !self.alt
            && let Some(op) = history_op_for_vk(physical)
        {
            return Some(match op {
                WorkspaceHistoryOp::Previous => ChordAction::WorkspacePrevious,
                WorkspaceHistoryOp::Prev => ChordAction::WorkspacePrev,
                WorkspaceHistoryOp::Next => ChordAction::WorkspaceNext,
            });
        }
        // Keyboard resize: Win+Alt+key grows outwards, Win+Shift+Alt+key
        // shrinks inwards, over the directional catalog (letters plus arrow
        // aliases). The mode rides the explicit bound action, never a move.
        // Extra Ctrl refuses here; an exact effective rebind still routes
        // through the rebound table above.
        if self.alt && !self.ctrl && catalog_index(physical).is_some() {
            return Some(resize_action_for_mode(resize_mode_for_shift(self.shift)));
        }
        if self.ctrl || self.alt {
            return None;
        }
        if catalog_index(physical).is_some() {
            return Some(ChordAction::Directional);
        }
        if is_digit_vk(physical) {
            return Some(ChordAction::WorkspaceDigit);
        }
        if is_maximize_vk(physical) && !self.shift {
            return Some(ChordAction::Maximize);
        }
        if is_fullscreen_vk(physical) && !self.shift {
            return Some(ChordAction::Fullscreen);
        }
        if is_orientation_vk(physical) && !self.shift {
            return Some(ChordAction::Orientation);
        }
        if is_launch_browser_vk(physical) && !self.shift {
            return Some(ChordAction::LaunchBrowser);
        }
        if is_launch_terminal_vk(physical) && !self.shift {
            return Some(ChordAction::LaunchTerminal);
        }
        if is_float_vk(physical) {
            return Some(if self.shift {
                ChordAction::Sticky
            } else {
                ChordAction::Float
            });
        }
        None
    }

    fn pin_get(&self, physical: u32) -> Option<PinnedRoute> {
        if (physical as usize) < PIN_KEYS {
            self.phys_pin[physical as usize]
        } else {
            None
        }
    }

    fn pin_set(&mut self, physical: u32, route: PinnedRoute) {
        if (physical as usize) < PIN_KEYS {
            self.phys_pin[physical as usize] = Some(route);
        }
    }

    fn pin_take(&mut self, physical: u32) -> Option<PinnedRoute> {
        if (physical as usize) < PIN_KEYS {
            self.phys_pin[physical as usize].take()
        } else {
            None
        }
    }

    /// Arm the Start-menu mask for a consumed project Win+Left gesture. The
    /// mouse hook consumes the click so no keyboard chord ever lands, but a
    /// physical Win hold still needs the E8 pair at Win-up or the OS opens
    /// Start. Idempotent per hold: a second call keeps the first trigger.
    /// NOOP absent tracked physical Win held: a synthetic Win hold never
    /// enters classifier tracking (injected input is filtered), so arming
    /// for it would persist pending and wrongly mask the next bare physical
    /// Win tap. The existing Win-up reserve fires it once per hold and the
    /// both-Wins terminal clears it, so no stale flag survives the hold.
    pub fn note_windrag_consumed(&mut self) {
        if !(self.win_l || self.win_r) {
            return;
        }
        if self.mask_trigger.is_none() {
            self.mask_trigger = Some(MaskTrigger::WinDrag);
        }
        self.mask_pending = true;
    }

    /// Tracked modifier state for the callback's preheld check
    /// `(ctrl, alt, shift)`. A physically held Ctrl/Alt the classifier never
    /// saw (held before install, or injected past the callback guard) forces
    /// pass-through to preserve unowned chords. Supported Shift binds from
    /// the official async sample instead (see `sync_shift_from_async`), so a
    /// preheld Shift correctly selects move/sticky/send rather than leaking.
    pub fn tracked_modifiers(&self) -> (bool, bool, bool) {
        (self.ctrl, self.alt, self.shift)
    }

    /// Bind supported Shift from the official async sample for a fresh chord
    /// down. Preheld Shift (held before install) still selects move/sticky/
    /// send; a missed Shift-up still resolves to focus. Holds already down
    /// keep their down-time op via stored origin, so syncing here never flips
    /// an armed hold.
    #[cfg(any(windows, test))]
    pub(crate) fn sync_shift_from_async(&mut self, async_shift: bool) {
        self.shift = async_shift;
    }

    /// Whether either Win key is currently held (tracked state).
    pub fn win_held(&self) -> bool {
        self.win_l || self.win_r
    }

    /// Whether a physical key currently holds a down without its up: its own
    /// live safe-refusal hold, else its own down-time pin resolved against
    /// the canonical slot it opened. Never reports another physical's
    /// canonical occupant: an unpressed rebound source reads false even
    /// while its canonical target is held elsewhere.
    pub fn key_is_down(&self, vk: u32) -> bool {
        if (vk as usize) < PIN_KEYS && self.collide[vk as usize].is_some() {
            return true;
        }
        match self.pin_get(vk) {
            Some(pinned) => self.canon_is_down(pinned.action, pinned.canon),
            None => false,
        }
    }

    /// Live shape of the hold owning a canonical route, for a colliding
    /// physical's safe refusal: same variant and action identity as the
    /// owner's down (the op stays pinned at the owner's down time, so a
    /// later modifier flip cannot rewrite it). `None` when no hold owns it.
    /// History arms own separate slots from directional/digit arms sharing
    /// one VK, so cross-arm holds never collide.
    fn held_shape(&self, action: ChordAction, canon: u32) -> Option<Classified> {
        match action {
            ChordAction::Directional => {
                let idx = catalog_index(canon)?;
                if !self.key_down[idx] {
                    return None;
                }
                Some(Classified::Snap(SnapIntent {
                    op: self.key_op[idx].unwrap_or(SnapOp::Focus),
                    direction: index_direction(idx),
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.key_origin[idx],
                    announce: false,
                }))
            }
            ChordAction::ResizeOut | ChordAction::ResizeIn => {
                let idx = catalog_index(canon)?;
                if !self.resize_down[idx] {
                    return None;
                }
                Some(Classified::Resize(ResizeIntent {
                    mode: self.resize_mode[idx].unwrap_or(ResizeMode::Outwards),
                    direction: index_direction(idx),
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.resize_origin[idx],
                    announce: false,
                }))
            }
            ChordAction::WorkspaceDigit | ChordAction::WorkspaceStayDigit => {
                if !is_digit_vk(canon) {
                    return None;
                }
                let slot = (canon - VK_0) as usize;
                if !self.digit_down[slot] {
                    return None;
                }
                Some(Classified::Workspace(WorkspaceIntent {
                    op: self.digit_op[slot].unwrap_or(WorkspaceOp::Select),
                    index: slot as u8,
                    follow: self.digit_follow[slot],
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.digit_origin[slot],
                    announce: false,
                }))
            }
            ChordAction::WorkspaceSendPrev
            | ChordAction::WorkspaceSendNext
            | ChordAction::WorkspaceSendStayPrev
            | ChordAction::WorkspaceSendStayNext => {
                let idx = relative_send_index(canon)?;
                if !self.send_down[idx] {
                    return None;
                }
                Some(Classified::WorkspaceSend(WorkspaceSendIntent {
                    delta: self.send_delta[idx],
                    follow: self.send_follow[idx],
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.send_origin[idx],
                    announce: false,
                }))
            }
            ChordAction::WorkspacePrevious
            | ChordAction::WorkspacePrev
            | ChordAction::WorkspaceNext => {
                let idx = history_index(canon)?;
                if !self.hist_down[idx] {
                    return None;
                }
                Some(Classified::WorkspaceHistory(WorkspaceHistoryIntent {
                    op: self.hist_op[idx].unwrap_or(WorkspaceHistoryOp::Prev),
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.hist_origin[idx],
                    announce: false,
                }))
            }
            ChordAction::Maximize => {
                if !self.maximize_down {
                    return None;
                }
                Some(Classified::Maximize(MaximizeIntent {
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.maximize_origin,
                    announce: false,
                }))
            }
            ChordAction::Orientation => {
                if !self.orientation_down {
                    return None;
                }
                Some(Classified::Orientation(OrientationIntent {
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.orientation_origin,
                    announce: false,
                }))
            }
            ChordAction::Fullscreen => {
                if !self.fullscreen_down {
                    return None;
                }
                Some(Classified::Fullscreen(FullscreenIntent {
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.fullscreen_origin,
                    announce: false,
                }))
            }
            ChordAction::Float => {
                if !self.float_down {
                    return None;
                }
                Some(Classified::Float(FloatIntent {
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.float_origin,
                    announce: false,
                }))
            }
            ChordAction::Sticky => {
                if !self.sticky_down {
                    return None;
                }
                Some(Classified::Sticky(StickyIntent {
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.sticky_origin,
                    announce: false,
                }))
            }
            ChordAction::LaunchBrowser => {
                if !self.launch_browser_down {
                    return None;
                }
                Some(Classified::Launch(LaunchIntent {
                    kind: LaunchKind::Browser,
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.launch_browser_origin,
                    announce: false,
                }))
            }
            ChordAction::LaunchTerminal => {
                if !self.launch_terminal_down {
                    return None;
                }
                Some(Classified::Launch(LaunchIntent {
                    kind: LaunchKind::Terminal,
                    edge: SnapEdge::Down,
                    foreground: true,
                    consumed: self.launch_terminal_origin,
                    announce: false,
                }))
            }
        }
    }

    /// Whether a canonical route currently holds a down without its up.
    /// History arms own separate slots from directional arms sharing one VK;
    /// resize arms share one slot across both modes (one physical key holds
    /// one resize at a time).
    fn canon_is_down(&self, action: ChordAction, vk: u32) -> bool {
        match action {
            ChordAction::Directional => catalog_index(vk).is_some_and(|idx| self.key_down[idx]),
            ChordAction::ResizeOut | ChordAction::ResizeIn => {
                catalog_index(vk).is_some_and(|idx| self.resize_down[idx])
            }
            ChordAction::WorkspaceDigit | ChordAction::WorkspaceStayDigit => {
                is_digit_vk(vk) && self.digit_down[(vk - VK_0) as usize]
            }
            ChordAction::WorkspaceSendPrev
            | ChordAction::WorkspaceSendNext
            | ChordAction::WorkspaceSendStayPrev
            | ChordAction::WorkspaceSendStayNext => {
                relative_send_index(vk).is_some_and(|idx| self.send_down[idx])
            }
            ChordAction::WorkspacePrevious
            | ChordAction::WorkspacePrev
            | ChordAction::WorkspaceNext => {
                history_index(vk).is_some_and(|idx| self.hist_down[idx])
            }
            ChordAction::Maximize => self.maximize_down,
            ChordAction::Orientation => self.orientation_down,
            ChordAction::Fullscreen => self.fullscreen_down,
            ChordAction::Float => self.float_down,
            ChordAction::Sticky => self.sticky_down,
            ChordAction::LaunchBrowser => self.launch_browser_down,
            ChordAction::LaunchTerminal => self.launch_terminal_down,
        }
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
    /// chord when the caller must send the E8 pair before passing this Win-up
    /// through. The Win-up itself always passes. `installed` is false on the
    /// state-only path, which updates bookkeeping without arming a send.
    fn win_up_needs_mask(&mut self, vk: u32, installed: bool) -> Option<MaskTrigger> {
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
        // A project Win+Left hold masks only at the terminal release: with
        // both Win sides genuinely held, the first Win-up keeps the hold
        // masked-pending instead of firing into a still-held Win (which
        // would leave the second release unmasked). Catalog chord triggers
        // keep the existing first-up policy; only WinDrag waits.
        if trigger == Some(MaskTrigger::WinDrag) && (self.win_l || self.win_r) {
            return None;
        }
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
    ) -> Option<Classified> {
        if injected {
            return None;
        }
        if is_win_vk(vk) {
            if is_up {
                let _ = self.win_up_needs_mask(vk, false);
            } else {
                self.note_win_down(vk);
            }
            return None;
        }
        if is_modifier_vk(vk) {
            let held = !is_up;
            match vk {
                VK_CONTROL | VK_LCONTROL | VK_RCONTROL => {
                    self.ctrl = held;
                }
                VK_MENU | VK_LMENU | VK_RMENU => {
                    self.alt = held;
                }
                _ => {
                    self.shift = held;
                }
            }
            return None;
        }
        // Down-time physical routing pinned through the matching up (see
        // `phys_pin`): fresh downs route live (rebound wins over
        // suppression); pinned-hold repeats ride the down-time routing with
        // dispatch gated on the chord still routing live, so a mid-hold
        // table change swallows without dispatching instead of leaking. The
        // Win+L fence keys on the physical chord only: a safe rebound into
        // the canonical L slot works without opt-in, while physical
        // unshifted Win+L stays gated.
        //
        // Physical identity safety: only a hold the family actually owns
        // pins. Unpinned key-ups always pass untracked (no remap/canonical
        // fallback), so an unrelated release can never consume or clear
        // another physical's slot. A fresh physical routing to an
        // already-held canonical is a collision, never the owner's repeat:
        // it is swallowed as a safe consumed refusal from the live owner's
        // hold shape with its own down-time verdict, paired repeat/up
        // included, never touching the canonical slot and never dispatching.
        let physical = vk;
        let pidx = physical as usize;
        if pidx < PIN_KEYS
            && let Some(tmpl) = self.collide[pidx]
        {
            if is_up {
                self.collide[pidx] = None;
                return Some(collide_refusal(
                    tmpl,
                    SnapEdge::Up,
                    foreground,
                    tmpl.consumed(),
                ));
            }
            if tmpl.consumed() && (self.win_l || self.win_r) {
                self.mask_pending = true;
                self.mask_trigger = collide_trigger(&tmpl);
            }
            return Some(collide_refusal(
                tmpl,
                SnapEdge::Repeat,
                foreground,
                tmpl.consumed(),
            ));
        }
        if is_up {
            // The pin proves this physical opened the hold; the family
            // closes its own paired slot below.
            let pinned = self.pin_take(physical)?;
            return self.push_owned(
                physical,
                pinned.canon,
                pinned.action,
                true,
                foreground,
                true,
            );
        }
        if let Some(pinned) = self.pin_get(physical) {
            let live = self.route_matches(physical, pinned);
            return self.push_owned(
                physical,
                pinned.canon,
                pinned.action,
                false,
                foreground,
                live,
            );
        }
        // Fresh down: refuse before pinning, so a refused chord leaves no
        // orphan pin and its paired key-up passes through as well. Win must
        // be held; modifiers select the arm (Ctrl selects history, existing
        // arms refuse Ctrl/Alt inside their family guards).
        if !(self.win_l || self.win_r) {
            return None;
        }
        let (action, canon, remapped) =
            match self.fresh_route(physical, self.shift, self.ctrl, self.alt) {
                Route::Suppressed => return None,
                Route::Remap(action, target) => (action, target, true),
                Route::Direct => {
                    let action = self.direct_action(physical)?;
                    (action, physical, false)
                }
            };
        // Unshifted physical Win+L is the OS lock chord: without explicit
        // opt-in it passes through untracked, so its paired key-up also
        // passes. The fence keys on the physical chord: a safe rebound
        // into the canonical L slot works without opt-in.
        if action == ChordAction::Directional
            && !self.shift
            && physical == VK_L
            && !self.allow_win_l
            && catalog_index(canon) == Some(3)
        {
            return None;
        }
        if self.canon_is_down(action, canon) {
            // Owned by a different physical: swallow as a safe consumed
            // refusal with its own down-time verdict. Passed while the gate
            // is off never holds: it passes through like a suppression.
            if !(self.enabled && self.gate_active) {
                return None;
            }
            let shape = self.held_shape(action, canon)?;
            let down = collide_refusal(shape, SnapEdge::Down, foreground, true);
            if pidx < PIN_KEYS {
                self.collide[pidx] = Some(down);
            }
            self.mask_pending = true;
            self.mask_trigger = collide_trigger(&shape);
            return Some(down);
        }
        if remapped || is_chord_vk(physical) {
            self.pin_set(physical, PinnedRoute { canon, action });
        }
        self.push_owned(physical, canon, action, false, foreground, true)
    }

    /// Owned-chord dispatch for [`SnapClassify::push`]: `physical` opened the
    /// hold (or repeats it), `vk` is its down-time canonical key and `action`
    /// its explicit arm. Family pairing, op pinning, gate riding, and mask
    /// semantics are unchanged; the central guard above guarantees only true
    /// owners arrive here.
    fn push_owned(
        &mut self,
        physical: u32,
        vk: u32,
        action: ChordAction,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        match action {
            ChordAction::WorkspacePrevious
            | ChordAction::WorkspacePrev
            | ChordAction::WorkspaceNext => {
                return self.push_history(physical, vk, action, is_up, foreground, route_live);
            }
            ChordAction::WorkspaceDigit | ChordAction::WorkspaceStayDigit => {
                return self.push_digit(vk, action, is_up, foreground, route_live);
            }
            ChordAction::WorkspaceSendPrev
            | ChordAction::WorkspaceSendNext
            | ChordAction::WorkspaceSendStayPrev
            | ChordAction::WorkspaceSendStayNext => {
                return self.push_relative_send(vk, action, is_up, foreground, route_live);
            }
            ChordAction::Maximize => {
                return self.push_maximize(is_up, foreground, route_live);
            }
            ChordAction::Orientation => {
                return self.push_orientation(is_up, foreground, route_live);
            }
            ChordAction::Fullscreen => {
                return self.push_fullscreen(is_up, foreground, route_live);
            }
            ChordAction::Float => {
                debug_assert!(is_float_vk(vk) || is_sticky_vk(vk));
                return self.push_g(is_up, foreground, route_live);
            }
            ChordAction::Sticky => {
                debug_assert!(is_float_vk(vk) || is_sticky_vk(vk));
                return self.push_g(is_up, foreground, route_live);
            }
            ChordAction::LaunchBrowser => {
                debug_assert!(is_launch_browser_vk(vk));
                return self.push_launch(LaunchKind::Browser, is_up, foreground, route_live);
            }
            ChordAction::LaunchTerminal => {
                debug_assert!(is_launch_terminal_vk(vk));
                return self.push_launch(LaunchKind::Terminal, is_up, foreground, route_live);
            }
            ChordAction::ResizeOut | ChordAction::ResizeIn => {
                return self.push_resize(vk, action, is_up, foreground, route_live);
            }
            ChordAction::Directional => {}
        }
        let physical_lock = physical == VK_L;
        let idx = catalog_index(vk)?;
        let direction = index_direction(idx);
        if is_up {
            if !self.key_down[idx] {
                return None;
            }
            self.key_down[idx] = false;
            let origin = self.key_origin[idx];
            self.key_origin[idx] = false;
            let op = self.key_op[idx].unwrap_or(SnapOp::Focus);
            self.key_op[idx] = None;
            self.counts[idx].up += 1;
            if origin {
                self.counts[idx].consumed += 1;
                Some(Classified::Snap(SnapIntent {
                    op,
                    direction,
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: true,
                    announce: false,
                }))
            } else {
                self.counts[idx].passed += 1;
                Some(Classified::Snap(SnapIntent {
                    op,
                    direction,
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: false,
                    announce: false,
                }))
            }
        } else {
            // Armed-hold repeats classify BEFORE the fresh-chord guard: a
            // consumed hold stays swallowed until its matching up even when
            // Ctrl/Alt arrive mid-hold, Win is released early, or Shift
            // flips. Only the dispatch (`announce`) needs the live
            // combination to still match the pinned op.
            if self.key_down[idx] {
                self.counts[idx].repeat += 1;
                let op = self.key_op[idx].unwrap_or(SnapOp::Focus);
                if self.key_origin[idx] {
                    let live = self.enabled
                        && self.gate_active
                        && route_live
                        && snap_repeat_live(
                            self.shift,
                            self.ctrl,
                            self.alt,
                            self.win_l || self.win_r,
                            op,
                        )
                        && snap_op_admits(physical, op, self.allow_win_l);
                    self.counts[idx].consumed += 1;
                    // Only a Win-held repeat rearms the Start-menu mask: a
                    // bare repeat after Win-up stays swallowed but must not
                    // create a new mask obligation, or a later naked Win tap
                    // would mask Start incorrectly.
                    if self.win_l || self.win_r {
                        self.mask_pending = true;
                        self.mask_trigger = Some(MaskTrigger::Snap { op, direction });
                    }
                    Some(Classified::Snap(SnapIntent {
                        op,
                        direction,
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: true,
                        announce: live,
                    }))
                } else {
                    self.counts[idx].passed += 1;
                    Some(Classified::Snap(SnapIntent {
                        op,
                        direction,
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            } else {
                if self.ctrl || self.alt || !(self.win_l || self.win_r) {
                    return None;
                }
                let op = if self.shift {
                    SnapOp::Move
                } else {
                    SnapOp::Focus
                };
                // Unshifted physical Win+L is the OS lock chord: without explicit
                // opt-in it passes through untracked, so its paired key-up also
                // passes. The fence keys on the physical chord: a safe rebound
                // into the canonical L slot works without opt-in.
                // Established opt-in exception: an ordinary LL hook cannot
                // reliably intercept it; left unchanged.
                if op == SnapOp::Focus && is_letter_l(idx) && physical_lock && !self.allow_win_l {
                    return None;
                }
                self.key_down[idx] = true;
                let origin = self.enabled && self.gate_active && route_live;
                self.key_origin[idx] = origin;
                self.key_op[idx] = Some(op);
                self.counts[idx].down += 1;
                if origin {
                    self.counts[idx].consumed += 1;
                    self.mask_pending = true;
                    self.mask_trigger = Some(MaskTrigger::Snap { op, direction });
                    Some(Classified::Snap(SnapIntent {
                        op,
                        direction,
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: true,
                        announce: true,
                    }))
                } else {
                    self.counts[idx].passed += 1;
                    Some(Classified::Snap(SnapIntent {
                        op,
                        direction,
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            }
        }
    }

    /// Resize half of the unified classifier (Win+Alt+key outwards,
    /// Win+Shift+Alt+key inwards, over the directional catalog letters plus
    /// arrow aliases). Same Win/armed-hold/gate/mask contract as the
    /// directional catalog, but Alt selects the arm: fresh downs require
    /// Win+Alt with no Ctrl, Shift selecting the mode. The mode is fixed at
    /// down time from the explicit action (rebinds keep their action even
    /// when the physical key differs) and rides the paired key-up; repeats of
    /// a consumed hold stay swallowed until the matching up but only announce
    /// while the live Win+Alt(+Shift) combination still matches the pinned
    /// mode, so a bare-key repeat after Win-up never re-dispatches. Extra
    /// Ctrl or a missing Win/Alt passes through untracked with no pin.
    /// `route_live` gates fresh consumption and repeat dispatch: a pinned
    /// hold whose chord no longer routes live (disabled or rebound away
    /// mid-hold) stays swallowed without dispatching until its matching up.
    fn push_resize(
        &mut self,
        vk: u32,
        action: ChordAction,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        let idx = catalog_index(vk)?;
        let mode = resize_mode_for_action(action)?;
        let direction = index_direction(idx);
        if is_up {
            if !self.resize_down[idx] {
                return None;
            }
            self.resize_down[idx] = false;
            let origin = self.resize_origin[idx];
            self.resize_origin[idx] = false;
            let pinned = self.resize_mode[idx].unwrap_or(mode);
            self.resize_mode[idx] = None;
            self.resize_counts[idx].up += 1;
            if origin {
                self.resize_counts[idx].consumed += 1;
                Some(Classified::Resize(ResizeIntent {
                    mode: pinned,
                    direction,
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: true,
                    announce: false,
                }))
            } else {
                self.resize_counts[idx].passed += 1;
                Some(Classified::Resize(ResizeIntent {
                    mode: pinned,
                    direction,
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: false,
                    announce: false,
                }))
            }
        } else {
            // Armed-hold repeats classify BEFORE the fresh-chord guard: a
            // consumed hold stays swallowed until its matching up even when
            // Alt/Shift/Ctrl/Win flip mid-hold. Only the dispatch
            // (`announce`) needs the live combination to still match the
            // pinned mode/action.
            if self.resize_down[idx] {
                self.resize_counts[idx].repeat += 1;
                let pinned = self.resize_mode[idx].unwrap_or(mode);
                if self.resize_origin[idx] {
                    let live = self.enabled
                        && self.gate_active
                        && route_live
                        && resize_repeat_live(
                            self.shift,
                            self.ctrl,
                            self.alt,
                            self.win_l || self.win_r,
                            pinned,
                        );
                    self.resize_counts[idx].consumed += 1;
                    // Only a Win-held repeat rearms the Start-menu mask: a
                    // bare repeat after Win-up stays swallowed but must not
                    // create a new mask obligation.
                    if self.win_l || self.win_r {
                        self.mask_pending = true;
                        self.mask_trigger = Some(MaskTrigger::Resize {
                            mode: pinned,
                            direction,
                        });
                    }
                    Some(Classified::Resize(ResizeIntent {
                        mode: pinned,
                        direction,
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: true,
                        announce: live,
                    }))
                } else {
                    self.resize_counts[idx].passed += 1;
                    Some(Classified::Resize(ResizeIntent {
                        mode: pinned,
                        direction,
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            } else {
                // Fresh down: Win+Alt with no Ctrl; Shift must select the
                // bound mode (outwards unshifted, inwards shifted). The Alt
                // arm is exact: unknown extra Ctrl or a missing Win/Alt
                // refuses with no pin, so the paired key-up passes too.
                if !(self.win_l || self.win_r) || !self.alt || self.ctrl {
                    return None;
                }
                if self.shift != (mode == ResizeMode::Inwards) {
                    return None;
                }
                self.resize_down[idx] = true;
                let origin = self.enabled && self.gate_active && route_live;
                self.resize_origin[idx] = origin;
                self.resize_mode[idx] = Some(mode);
                self.resize_counts[idx].down += 1;
                if origin {
                    self.resize_counts[idx].consumed += 1;
                    self.mask_pending = true;
                    self.mask_trigger = Some(MaskTrigger::Resize { mode, direction });
                    Some(Classified::Resize(ResizeIntent {
                        mode,
                        direction,
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: true,
                        announce: true,
                    }))
                } else {
                    self.resize_counts[idx].passed += 1;
                    Some(Classified::Resize(ResizeIntent {
                        mode,
                        direction,
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            }
        }
    }

    /// History half of the unified classifier (item 1: Win+Ctrl+Tab toggle,
    /// Win+Ctrl+H/K/Left/Up previous, Win+Ctrl+J/L/Down/Right next). Same
    /// Win/armed-hold/gate/mask contract as digits, but Ctrl selects the arm:
    /// fresh downs require Win+Ctrl with no Shift/Alt. The op is fixed at
    /// down time from the explicit action (rebinds keep their action even
    /// when the physical key differs); relative-step repeats ride the armed
    /// hold and dispatch while the live Win+Ctrl combination still matches,
    /// so held-key autorepeat steps repeatedly, while Previous-toggle repeats
    /// stay swallowed (maximize parity, no pingpong). Ups close the pair. Fresh downs
    /// consume iff takeover is on with the shortcut gate active; the owner
    /// rechecks fresh scope/suspension before any select.
    /// `route_live` gates fresh consumption and repeat dispatch: a pinned
    /// hold whose chord no longer routes live stays swallowed without
    /// dispatching until its matching up.
    fn push_history(
        &mut self,
        _physical: u32,
        vk: u32,
        action: ChordAction,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        let idx = history_index(vk)?;
        let op = match action {
            ChordAction::WorkspacePrevious => WorkspaceHistoryOp::Previous,
            ChordAction::WorkspacePrev => WorkspaceHistoryOp::Prev,
            ChordAction::WorkspaceNext => WorkspaceHistoryOp::Next,
            _ => return None,
        };
        if is_up {
            if !self.hist_down[idx] {
                return None;
            }
            self.hist_down[idx] = false;
            let origin = self.hist_origin[idx];
            self.hist_origin[idx] = false;
            self.hist_op[idx] = None;
            self.hist_counts[idx].up += 1;
            if origin {
                self.hist_counts[idx].consumed += 1;
                Some(Classified::WorkspaceHistory(WorkspaceHistoryIntent {
                    op,
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: true,
                    announce: false,
                }))
            } else {
                self.hist_counts[idx].passed += 1;
                Some(Classified::WorkspaceHistory(WorkspaceHistoryIntent {
                    op,
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: false,
                    announce: false,
                }))
            }
        } else {
            // Armed-hold repeats classify before the fresh-chord guard: a
            // consumed hold stays swallowed across mid-hold modifier/Win
            // transitions (only `announce` follows the live combination), a
            // passed hold stays passed.
            if self.hist_down[idx] {
                self.hist_counts[idx].repeat += 1;
                let pinned = self.hist_op[idx].unwrap_or(op);
                if self.hist_origin[idx] {
                    let live = self.enabled
                        && self.gate_active
                        && route_live
                        && history_repeat_live(
                            self.ctrl,
                            self.shift,
                            self.alt,
                            self.win_l || self.win_r,
                        );
                    self.hist_counts[idx].consumed += 1;
                    if self.win_l || self.win_r {
                        self.mask_pending = true;
                        self.mask_trigger = Some(MaskTrigger::WorkspaceHistory { op: pinned });
                    }
                    // The Previous toggle never re-dispatches on hold
                    // (maximize parity): a held Tab would otherwise pingpong
                    // between the two views. Relative steps keep digit-like
                    // repeat dispatch while live.
                    let announce = live && pinned != WorkspaceHistoryOp::Previous;
                    Some(Classified::WorkspaceHistory(WorkspaceHistoryIntent {
                        op: pinned,
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: true,
                        announce,
                    }))
                } else {
                    self.hist_counts[idx].passed += 1;
                    Some(Classified::WorkspaceHistory(WorkspaceHistoryIntent {
                        op: pinned,
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            } else {
                if !self.ctrl || self.shift || self.alt || !(self.win_l || self.win_r) {
                    return None;
                }
                self.hist_down[idx] = true;
                let origin = self.enabled && self.gate_active && route_live;
                self.hist_origin[idx] = origin;
                self.hist_op[idx] = Some(op);
                self.hist_counts[idx].down += 1;
                if origin {
                    self.hist_counts[idx].consumed += 1;
                    self.mask_pending = true;
                    self.mask_trigger = Some(MaskTrigger::WorkspaceHistory { op });
                    Some(Classified::WorkspaceHistory(WorkspaceHistoryIntent {
                        op,
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: true,
                        announce: true,
                    }))
                } else {
                    self.hist_counts[idx].passed += 1;
                    Some(Classified::WorkspaceHistory(WorkspaceHistoryIntent {
                        op,
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            }
        }
    }

    /// Digit half of the unified classifier: same Win/Ctrl/Alt/armed-hold/
    /// gate/mask contract as the directional catalog. Shift flips select into
    /// send and is fixed at down time; the op rides the paired key-up.
    /// Consumed-hold repeats stay swallowed until the matching up but only
    /// announce while the live combination still matches the pinned op. Fresh
    /// downs consume iff takeover is on with the shortcut gate active; the
    /// owner rechecks fresh identity/scope/elevation/fullscreen/gesture
    /// before any action (send without a live managed origin settles as
    /// unmanaged, never acts on protected windows).
    /// `route_live` gates fresh consumption and repeat dispatch: a pinned
    /// hold whose chord no longer routes live stays swallowed without
    /// dispatching until its matching up.
    fn push_digit(
        &mut self,
        vk: u32,
        action: ChordAction,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        if !(VK_0..=VK_9).contains(&vk) {
            return None;
        }
        let slot = (vk - VK_0) as usize;
        // Explicit stay action pins follow=false; the direct digit arm pins
        // follow from Shift (Shift sends and follows, unshifted selects).
        let stay = action == ChordAction::WorkspaceStayDigit;
        if is_up {
            if !self.digit_down[slot] {
                return None;
            }
            self.digit_down[slot] = false;
            let origin = self.digit_origin[slot];
            self.digit_origin[slot] = false;
            let op = self.digit_op[slot].unwrap_or(WorkspaceOp::Select);
            self.digit_op[slot] = None;
            let follow = self.digit_follow[slot];
            self.digit_follow[slot] = true;
            self.digit_ctrl[slot] = false;
            self.digit_counts[slot].up += 1;
            if origin {
                self.digit_counts[slot].consumed += 1;
                Some(Classified::Workspace(WorkspaceIntent {
                    op,
                    index: slot as u8,
                    follow,
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: true,
                    announce: false,
                }))
            } else {
                self.digit_counts[slot].passed += 1;
                Some(Classified::Workspace(WorkspaceIntent {
                    op,
                    index: slot as u8,
                    follow,
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: false,
                    announce: false,
                }))
            }
        } else {
            // Armed-hold repeats classify before the fresh-chord guard, same
            // contract as the directional catalog: consumed holds stay
            // swallowed across mid-hold modifier/Win/Shift transitions (only
            // `announce` follows the live combination), passed holds stay
            // passed.
            if self.digit_down[slot] {
                self.digit_counts[slot].repeat += 1;
                let op = self.digit_op[slot].unwrap_or(WorkspaceOp::Select);
                let follow = self.digit_follow[slot];
                if self.digit_origin[slot] {
                    // Stay repeats dispatch only while the live combination
                    // still matches the pinned arm (Win+Shift plus the pinned
                    // Ctrl); follow/select repeats ride `digit_repeat_live`,
                    // which refuses Ctrl. Either way a mid-hold modifier flip
                    // swallows without dispatching, leaking no repeat and
                    // never flipping a stay into a follow.
                    let live = self.enabled
                        && self.gate_active
                        && route_live
                        && if follow {
                            digit_repeat_live(
                                self.shift,
                                self.ctrl,
                                self.alt,
                                self.win_l || self.win_r,
                                op,
                            )
                        } else {
                            self.shift
                                && self.ctrl == self.digit_ctrl[slot]
                                && !self.alt
                                && (self.win_l || self.win_r)
                        };
                    self.digit_counts[slot].consumed += 1;
                    // Win-held repeats only rearm the mask; bare repeats
                    // after Win-up stay swallowed without a new obligation.
                    if self.win_l || self.win_r {
                        self.mask_pending = true;
                        self.mask_trigger = Some(MaskTrigger::Workspace {
                            op,
                            index: slot as u8,
                        });
                    }
                    Some(Classified::Workspace(WorkspaceIntent {
                        op,
                        index: slot as u8,
                        follow,
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: true,
                        announce: live,
                    }))
                } else {
                    self.digit_counts[slot].passed += 1;
                    Some(Classified::Workspace(WorkspaceIntent {
                        op,
                        index: slot as u8,
                        follow,
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            } else {
                // Digit follow/select refuse Ctrl/Alt outright; numbered stay
                // additionally accepts the Win+Ctrl+Shift backlog arm (Shift
                // required, Alt never). Direct Ctrl+Shift+digit chords still
                // refuse above in `direct_action`: only an explicit stay
                // rebind routes here.
                if !(self.win_l || self.win_r) || self.alt {
                    return None;
                }
                if stay {
                    if !self.shift {
                        return None;
                    }
                } else if self.ctrl {
                    return None;
                }
                let op = if self.shift {
                    WorkspaceOp::Send
                } else {
                    WorkspaceOp::Select
                };
                // Unshifted chords never stay: only shifted sends carry the
                // follow/stay distinction.
                let follow = if op == WorkspaceOp::Send { !stay } else { true };
                self.digit_down[slot] = true;
                let origin = self.enabled && self.gate_active && route_live;
                self.digit_origin[slot] = origin;
                self.digit_op[slot] = Some(op);
                self.digit_follow[slot] = follow;
                self.digit_ctrl[slot] = self.ctrl;
                self.digit_counts[slot].down += 1;
                if origin {
                    self.digit_counts[slot].consumed += 1;
                    self.mask_pending = true;
                    self.mask_trigger = Some(MaskTrigger::Workspace {
                        op,
                        index: slot as u8,
                    });
                    Some(Classified::Workspace(WorkspaceIntent {
                        op,
                        index: slot as u8,
                        follow,
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: true,
                        announce: true,
                    }))
                } else {
                    self.digit_counts[slot].passed += 1;
                    Some(Classified::Workspace(WorkspaceIntent {
                        op,
                        index: slot as u8,
                        follow,
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            }
        }
    }

    /// Item 2 relative-send half of the unified classifier (Win+Ctrl+Shift+
    /// H/K/Left/Up previous, J/L/Down/Right next). Same Win/armed-hold/gate/
    /// mask contract as history, but Shift selects the send arm: fresh downs
    /// require Win+Ctrl+Shift with no Alt. `follow` pins at down time from the
    /// explicit action (direct chords follow; stay rows rebind through the
    /// stay actions); `delta` pins from the canonical key. Repeats ride the
    /// armed hold and dispatch while the live Win+Ctrl+Shift combination still
    /// matches; ups close the pair. Fresh downs consume iff takeover is on
    /// with the shortcut gate active; the owner rechecks fresh scope/
    /// suspension before any send.
    /// `route_live` gates fresh consumption and repeat dispatch: a pinned
    /// hold whose chord no longer routes live stays swallowed without
    /// dispatching until its matching up.
    fn push_relative_send(
        &mut self,
        vk: u32,
        action: ChordAction,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        let idx = relative_send_index(vk)?;
        let follow = matches!(
            action,
            ChordAction::WorkspaceSendPrev | ChordAction::WorkspaceSendNext
        );
        let delta = relative_send_delta(vk)?;
        // Defensive: the action's prev/next must agree with the canonical
        // key's direction; a mismatched rebind (prev action onto a next key)
        // refuses instead of sending the wrong way.
        let action_prev = matches!(
            action,
            ChordAction::WorkspaceSendPrev | ChordAction::WorkspaceSendStayPrev
        );
        if (delta < 0) != action_prev {
            return None;
        }
        let intent = |edge: SnapEdge, consumed: bool, announce: bool| {
            Classified::WorkspaceSend(WorkspaceSendIntent {
                delta,
                follow,
                edge,
                foreground,
                consumed,
                announce,
            })
        };
        if is_up {
            if !self.send_down[idx] {
                return None;
            }
            self.send_down[idx] = false;
            let origin = self.send_origin[idx];
            self.send_origin[idx] = false;
            self.send_follow[idx] = true;
            self.send_delta[idx] = 0;
            // Per-slot counts reuse the history-adjacent vocabulary: count in
            // the directional slot matching the canonical key when available.
            if origin {
                return Some(intent(SnapEdge::Up, true, false));
            }
            return Some(intent(SnapEdge::Up, false, false));
        }
        if self.send_down[idx] {
            let pinned_follow = self.send_follow[idx];
            let pinned_delta = self.send_delta[idx];
            let pinned = Classified::WorkspaceSend(WorkspaceSendIntent {
                delta: pinned_delta,
                follow: pinned_follow,
                edge: SnapEdge::Repeat,
                foreground,
                consumed: true,
                announce: false,
            });
            let _ = pinned;
            if self.send_origin[idx] {
                let live = self.enabled
                    && self.gate_active
                    && route_live
                    && relative_send_repeat_live(
                        self.ctrl,
                        self.shift,
                        self.alt,
                        self.win_l || self.win_r,
                    );
                if self.win_l || self.win_r {
                    self.mask_pending = true;
                    self.mask_trigger = Some(MaskTrigger::WorkspaceSend {
                        delta: pinned_delta,
                        follow: pinned_follow,
                    });
                }
                return Some(Classified::WorkspaceSend(WorkspaceSendIntent {
                    delta: pinned_delta,
                    follow: pinned_follow,
                    edge: SnapEdge::Repeat,
                    foreground,
                    consumed: true,
                    announce: live,
                }));
            }
            return Some(Classified::WorkspaceSend(WorkspaceSendIntent {
                delta: pinned_delta,
                follow: pinned_follow,
                edge: SnapEdge::Repeat,
                foreground,
                consumed: false,
                announce: false,
            }));
        }
        if !self.ctrl || !self.shift || self.alt || !(self.win_l || self.win_r) {
            return None;
        }
        self.send_down[idx] = true;
        let origin = self.enabled && self.gate_active && route_live;
        self.send_origin[idx] = origin;
        self.send_follow[idx] = follow;
        self.send_delta[idx] = delta;
        if origin {
            self.mask_pending = true;
            self.mask_trigger = Some(MaskTrigger::WorkspaceSend { delta, follow });
            Some(intent(SnapEdge::Down, true, true))
        } else {
            Some(intent(SnapEdge::Down, false, false))
        }
    }

    /// Maximize-toggle half of the unified classifier (Win+M, KDE Meta+M
    /// parity): same Win/Ctrl/Alt/armed-hold/gate/mask contract as the
    /// directional catalog. Shift selects the directional move arm instead,
    /// so any held Shift (or Ctrl/Alt, or missing Win) passes a fresh M
    /// through untracked and the paired key-up also passes. A consumed hold
    /// stays swallowed across later Shift/Ctrl/Alt/Win transitions until its
    /// matching up. Only the down dispatches: KDE shortcuts are
    /// discrete per press, so held repeats are swallowed (mask stays armed)
    /// instead of re-toggling. Ups close the pair. Fresh downs consume iff
    /// takeover is on with the shortcut gate active; the owner rechecks fresh
    /// identity/scope/elevation/fullscreen/gesture before any action.
    fn push_maximize(
        &mut self,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        if is_up {
            if !self.maximize_down {
                return None;
            }
            self.maximize_down = false;
            let origin = self.maximize_origin;
            self.maximize_origin = false;
            self.max_counts.up += 1;
            if origin {
                self.max_counts.consumed += 1;
                Some(Classified::Maximize(MaximizeIntent {
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: true,
                    announce: false,
                }))
            } else {
                self.max_counts.passed += 1;
                Some(Classified::Maximize(MaximizeIntent {
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: false,
                    announce: false,
                }))
            }
        } else {
            // Armed-hold repeats classify before the fresh-chord guard: a
            // consumed M hold stays swallowed across later Shift/Ctrl/Alt/Win
            // transitions (never re-dispatched), a passed hold stays passed.
            if self.maximize_down {
                self.max_counts.repeat += 1;
                if self.maximize_origin {
                    // Held repeat: swallowed, never re-dispatched. A Win-held
                    // hold continues to disguise Win, so the mask stays
                    // armed; a bare repeat after Win-up must not create a new
                    // mask obligation.
                    self.max_counts.consumed += 1;
                    if self.win_l || self.win_r {
                        self.mask_pending = true;
                        self.mask_trigger = Some(MaskTrigger::Maximize);
                    }
                    Some(Classified::Maximize(MaximizeIntent {
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: true,
                        announce: false,
                    }))
                } else {
                    self.max_counts.passed += 1;
                    Some(Classified::Maximize(MaximizeIntent {
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            } else {
                if self.ctrl || self.alt || self.shift || !(self.win_l || self.win_r) {
                    return None;
                }
                self.maximize_down = true;
                let origin = self.enabled && self.gate_active && route_live;
                self.maximize_origin = origin;
                self.max_counts.down += 1;
                if origin {
                    self.max_counts.consumed += 1;
                    self.mask_pending = true;
                    self.mask_trigger = Some(MaskTrigger::Maximize);
                    Some(Classified::Maximize(MaximizeIntent {
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: true,
                        announce: true,
                    }))
                } else {
                    self.max_counts.passed += 1;
                    Some(Classified::Maximize(MaximizeIntent {
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            }
        }
    }

    /// Orientation-toggle half of the unified classifier (Win+O, KDE Meta+O
    /// parity, item 4): same Win/Ctrl/Alt/armed-hold/gate/mask contract as
    /// the maximize arm. Any held Shift (or Ctrl/Alt, or missing Win) passes
    /// a fresh O through untracked and the paired key-up also passes. A
    /// consumed hold stays swallowed across later Shift/Ctrl/Alt/Win
    /// transitions until its matching up. Tentative Windows repeat policy:
    /// only the down dispatches; held repeats are swallowed (mask stays
    /// armed) instead of re-toggling. Ups close the pair. Fresh downs
    /// consume iff takeover is on with the shortcut gate active; the owner
    /// rechecks fresh identity/scope/elevation/fullscreen/gesture before any
    /// action.
    fn push_orientation(
        &mut self,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        if is_up {
            if !self.orientation_down {
                return None;
            }
            self.orientation_down = false;
            let origin = self.orientation_origin;
            self.orientation_origin = false;
            self.orientation_counts.up += 1;
            if origin {
                self.orientation_counts.consumed += 1;
                Some(Classified::Orientation(OrientationIntent {
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: true,
                    announce: false,
                }))
            } else {
                self.orientation_counts.passed += 1;
                Some(Classified::Orientation(OrientationIntent {
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: false,
                    announce: false,
                }))
            }
        } else {
            // Armed-hold repeats classify before the fresh-chord guard: a
            // consumed O hold stays swallowed across later Shift/Ctrl/Alt/Win
            // transitions (never re-dispatched), a passed hold stays passed.
            if self.orientation_down {
                self.orientation_counts.repeat += 1;
                if self.orientation_origin {
                    // Held repeat: swallowed, never re-dispatched. A Win-held
                    // hold continues to disguise Win, so the mask stays
                    // armed; a bare repeat after Win-up must not create a new
                    // mask obligation.
                    self.orientation_counts.consumed += 1;
                    if self.win_l || self.win_r {
                        self.mask_pending = true;
                        self.mask_trigger = Some(MaskTrigger::Orientation);
                    }
                    Some(Classified::Orientation(OrientationIntent {
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: true,
                        announce: false,
                    }))
                } else {
                    self.orientation_counts.passed += 1;
                    Some(Classified::Orientation(OrientationIntent {
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            } else {
                if self.ctrl || self.alt || self.shift || !(self.win_l || self.win_r) {
                    return None;
                }
                self.orientation_down = true;
                let origin = self.enabled && self.gate_active && route_live;
                self.orientation_origin = origin;
                self.orientation_counts.down += 1;
                if origin {
                    self.orientation_counts.consumed += 1;
                    self.mask_pending = true;
                    self.mask_trigger = Some(MaskTrigger::Orientation);
                    Some(Classified::Orientation(OrientationIntent {
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: true,
                        announce: true,
                    }))
                } else {
                    self.orientation_counts.passed += 1;
                    Some(Classified::Orientation(OrientationIntent {
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            }
        }
    }

    /// Fullscreen-toggle half of the unified classifier (Win+F11, KDE Meta+F11
    /// parity): same Win/Ctrl/Alt/armed-hold/gate/mask contract as the
    /// maximize arm. Any held Shift (or Ctrl/Alt, or missing Win) passes a
    /// fresh F11 through untracked and the paired key-up also passes. A
    /// consumed hold stays swallowed across later transitions until its
    /// matching up. Only the down dispatches:
    /// held repeats are swallowed (mask stays armed) instead of re-toggling.
    /// Ups close the pair. Fresh downs consume iff takeover is on with the
    /// shortcut gate active; the owner rechecks fresh identity/scope/
    /// elevation/fullscreen/gesture before any action.
    fn push_fullscreen(
        &mut self,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        if is_up {
            if !self.fullscreen_down {
                return None;
            }
            self.fullscreen_down = false;
            let origin = self.fullscreen_origin;
            self.fullscreen_origin = false;
            self.fullscreen_counts.up += 1;
            if origin {
                self.fullscreen_counts.consumed += 1;
                Some(Classified::Fullscreen(FullscreenIntent {
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: true,
                    announce: false,
                }))
            } else {
                self.fullscreen_counts.passed += 1;
                Some(Classified::Fullscreen(FullscreenIntent {
                    edge: SnapEdge::Up,
                    foreground,
                    consumed: false,
                    announce: false,
                }))
            }
        } else {
            // Armed-hold repeats classify before the fresh-chord guard, same
            // contract as maximize: consumed holds stay swallowed, passed
            // holds stay passed.
            if self.fullscreen_down {
                self.fullscreen_counts.repeat += 1;
                if self.fullscreen_origin {
                    // Held repeat: swallowed, never re-dispatched. A Win-held
                    // hold continues to disguise Win, so the mask stays
                    // armed; a bare repeat after Win-up must not create a new
                    // mask obligation.
                    self.fullscreen_counts.consumed += 1;
                    if self.win_l || self.win_r {
                        self.mask_pending = true;
                        self.mask_trigger = Some(MaskTrigger::Fullscreen);
                    }
                    Some(Classified::Fullscreen(FullscreenIntent {
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: true,
                        announce: false,
                    }))
                } else {
                    self.fullscreen_counts.passed += 1;
                    Some(Classified::Fullscreen(FullscreenIntent {
                        edge: SnapEdge::Repeat,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            } else {
                if self.ctrl || self.alt || self.shift || !(self.win_l || self.win_r) {
                    return None;
                }
                self.fullscreen_down = true;
                let origin = self.enabled && self.gate_active && route_live;
                self.fullscreen_origin = origin;
                self.fullscreen_counts.down += 1;
                if origin {
                    self.fullscreen_counts.consumed += 1;
                    self.mask_pending = true;
                    self.mask_trigger = Some(MaskTrigger::Fullscreen);
                    Some(Classified::Fullscreen(FullscreenIntent {
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: true,
                        announce: true,
                    }))
                } else {
                    self.fullscreen_counts.passed += 1;
                    Some(Classified::Fullscreen(FullscreenIntent {
                        edge: SnapEdge::Down,
                        foreground,
                        consumed: false,
                        announce: false,
                    }))
                }
            }
        }
    }

    /// Shared G-key dispatcher (Win+G float, Win+Shift+G sticky, KDE
    /// Meta+G / Meta+Shift+G parity). The op is fixed at down time from the
    /// Shift state, so releasing Shift before the key-up cannot flip float
    /// into sticky (or vice versa); repeats ride the armed hold regardless of
    /// later Shift. Ctrl/Alt or a missing Win passes through untracked.
    /// Ups close whichever hold is armed; untracked ups return `None`.
    fn push_g(&mut self, is_up: bool, foreground: bool, route_live: bool) -> Option<Classified> {
        if is_up {
            if self.float_down {
                return self.push_g_arm(GArm::Float, is_up, foreground, route_live);
            }
            if self.sticky_down {
                return self.push_g_arm(GArm::Sticky, is_up, foreground, route_live);
            }
            return None;
        }
        if self.float_down {
            return self.push_g_arm(GArm::Float, is_up, foreground, route_live);
        }
        if self.sticky_down {
            return self.push_g_arm(GArm::Sticky, is_up, foreground, route_live);
        }
        if self.shift {
            return self.push_g_arm(GArm::Sticky, is_up, foreground, route_live);
        }
        self.push_g_arm(GArm::Float, is_up, foreground, route_live)
    }

    /// Shared G-key arm (float and sticky halves): same Win/Ctrl/Alt/
    /// armed-hold/gate/mask contract as the maximize arm. A fresh down routes
    /// by Shift; held repeats ride the armed hold regardless of later Shift.
    /// Only the down dispatches: held repeats are swallowed (mask stays
    /// armed) instead of re-toggling. Ups close the pair. Fresh downs consume
    /// iff takeover is on with the shortcut gate active; the owner rechecks
    /// fresh identity/scope/elevation/fullscreen/gesture before any action.
    fn push_g_arm(
        &mut self,
        arm: GArm,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        let sticky = arm == GArm::Sticky;
        let shift = self.shift;
        let trigger = if sticky {
            MaskTrigger::Sticky
        } else {
            MaskTrigger::Float
        };
        let (down, origin, counts) = if sticky {
            (
                &mut self.sticky_down,
                &mut self.sticky_origin,
                &mut self.sticky_counts,
            )
        } else {
            (
                &mut self.float_down,
                &mut self.float_origin,
                &mut self.float_counts,
            )
        };
        let intent = |edge: SnapEdge, consumed: bool, announce: bool| {
            if sticky {
                Classified::Sticky(StickyIntent {
                    edge,
                    foreground,
                    consumed,
                    announce,
                })
            } else {
                Classified::Float(FloatIntent {
                    edge,
                    foreground,
                    consumed,
                    announce,
                })
            }
        };
        if is_up {
            if !*down {
                return None;
            }
            *down = false;
            let had_origin = *origin;
            *origin = false;
            counts.up += 1;
            if had_origin {
                counts.consumed += 1;
                Some(intent(SnapEdge::Up, true, false))
            } else {
                counts.passed += 1;
                Some(intent(SnapEdge::Up, false, false))
            }
        } else {
            // Armed-hold repeats classify before the fresh-chord guard: a
            // consumed G hold stays swallowed across later Ctrl/Alt/Win/Shift
            // transitions (never flipping arms), a passed hold stays passed.
            if *down {
                counts.repeat += 1;
                if *origin {
                    // Held repeat: swallowed, never re-dispatched. A Win-held
                    // hold continues to disguise Win, so the mask stays
                    // armed; a bare repeat after Win-up must not create a new
                    // mask obligation. Shift is op-fixed at down time: a
                    // later Shift never flips this repeat into the other arm.
                    counts.consumed += 1;
                    if self.win_l || self.win_r {
                        self.mask_pending = true;
                        self.mask_trigger = Some(trigger);
                    }
                    Some(intent(SnapEdge::Repeat, true, false))
                } else {
                    counts.passed += 1;
                    Some(intent(SnapEdge::Repeat, false, false))
                }
            } else {
                if self.ctrl || self.alt || !(self.win_l || self.win_r) {
                    return None;
                }
                if shift != sticky {
                    // The other arm owns this Shift state: the dispatcher
                    // routes fresh downs there, never here.
                    return None;
                }
                *down = true;
                let has_origin = self.enabled && self.gate_active && route_live;
                *origin = has_origin;
                counts.down += 1;
                if has_origin {
                    counts.consumed += 1;
                    self.mask_pending = true;
                    self.mask_trigger = Some(trigger);
                    Some(intent(SnapEdge::Down, true, true))
                } else {
                    counts.passed += 1;
                    Some(intent(SnapEdge::Down, false, false))
                }
            }
        }
    }

    /// Launcher half of the unified classifier (Win+B browser, Win+T
    /// terminal): same Win/Ctrl/Alt/armed-hold/gate/mask contract as the
    /// maximize arm. Any held Shift (or Ctrl/Alt, or missing Win) passes a
    /// fresh chord through untracked and the paired key-up also passes. A
    /// consumed hold stays swallowed across later Shift/Ctrl/Alt/Win
    /// transitions until its matching up. Only the down dispatches: held
    /// repeats are swallowed (mask stays armed) instead of re-spawning. Ups
    /// close the pair. Fresh downs consume iff takeover is on with the
    /// shortcut gate active; the owner spawns without touching any window.
    fn push_launch(
        &mut self,
        kind: LaunchKind,
        is_up: bool,
        foreground: bool,
        route_live: bool,
    ) -> Option<Classified> {
        let trigger = match kind {
            LaunchKind::Browser => MaskTrigger::LaunchBrowser,
            LaunchKind::Terminal => MaskTrigger::LaunchTerminal,
        };
        let (down, origin, counts) = match kind {
            LaunchKind::Browser => (
                &mut self.launch_browser_down,
                &mut self.launch_browser_origin,
                &mut self.launch_browser_counts,
            ),
            LaunchKind::Terminal => (
                &mut self.launch_terminal_down,
                &mut self.launch_terminal_origin,
                &mut self.launch_terminal_counts,
            ),
        };
        let intent = |edge: SnapEdge, consumed: bool, announce: bool| {
            Classified::Launch(LaunchIntent {
                kind,
                edge,
                foreground,
                consumed,
                announce,
            })
        };
        if is_up {
            if !*down {
                return None;
            }
            *down = false;
            let had_origin = *origin;
            *origin = false;
            counts.up += 1;
            if had_origin {
                counts.consumed += 1;
                Some(intent(SnapEdge::Up, true, false))
            } else {
                counts.passed += 1;
                Some(intent(SnapEdge::Up, false, false))
            }
        } else {
            // Armed-hold repeats classify before the fresh-chord guard: a
            // consumed launcher hold stays swallowed across later
            // Shift/Ctrl/Alt/Win transitions (never re-dispatched), a passed
            // hold stays passed.
            if *down {
                counts.repeat += 1;
                if *origin {
                    // Held repeat: swallowed, never re-dispatched. A Win-held
                    // hold continues to disguise Win, so the mask stays
                    // armed; a bare repeat after Win-up must not create a new
                    // mask obligation.
                    counts.consumed += 1;
                    if self.win_l || self.win_r {
                        self.mask_pending = true;
                        self.mask_trigger = Some(trigger);
                    }
                    Some(intent(SnapEdge::Repeat, true, false))
                } else {
                    counts.passed += 1;
                    Some(intent(SnapEdge::Repeat, false, false))
                }
            } else {
                if self.ctrl || self.alt || self.shift || !(self.win_l || self.win_r) {
                    return None;
                }
                *down = true;
                let has_origin = self.enabled && self.gate_active && route_live;
                *origin = has_origin;
                counts.down += 1;
                if has_origin {
                    counts.consumed += 1;
                    self.mask_pending = true;
                    self.mask_trigger = Some(trigger);
                    Some(intent(SnapEdge::Down, true, true))
                } else {
                    counts.passed += 1;
                    Some(intent(SnapEdge::Down, false, false))
                }
            }
        }
    }
}

#[cfg(test)]
mod shift_sync_tests {
    use super::{KeyboardConfig, SnapOp, VK_LWIN};

    fn enabled() -> KeyboardConfig {
        KeyboardConfig {
            takeover: true,
            allow_win_l: true,
        }
    }

    #[test]
    fn preheld_shift_selects_move_on_fresh_down() {
        // Preheld Shift (held before install, tracked state clear) binds from
        // the async sample so the fresh down resolves move, not focus.
        let mut m = super::SnapClassify::new(enabled());
        super::SnapClassify::push(&mut m, VK_LWIN, false, true, false);
        assert!(!m.tracked_modifiers().2);
        m.sync_shift_from_async(true);
        let down = super::SnapClassify::push(&mut m, super::VK_H, false, true, false)
            .and_then(|ev| match ev {
                super::Classified::Snap(intent) => Some(intent),
                _ => None,
            })
            .expect("fresh down");
        assert_eq!(down.op, SnapOp::Move);
        assert!(down.consumed);
    }

    #[test]
    fn missed_shift_up_resolves_focus_on_fresh_down() {
        // Missed Shift-up (tracked held, physical released) syncs back so a
        // fresh down resolves focus, not a stale move.
        let mut m = super::SnapClassify::new(enabled());
        super::SnapClassify::push(&mut m, VK_LWIN, false, true, false);
        super::SnapClassify::push(&mut m, super::VK_SHIFT, false, true, false);
        assert!(m.tracked_modifiers().2);
        m.sync_shift_from_async(false);
        let down = super::SnapClassify::push(&mut m, super::VK_H, false, true, false)
            .and_then(|ev| match ev {
                super::Classified::Snap(intent) => Some(intent),
                _ => None,
            })
            .expect("fresh down");
        assert_eq!(down.op, SnapOp::Focus);
    }

    #[test]
    fn armed_hold_keeps_down_time_op_across_sync() {
        // Syncing Shift mid-hold never flips the armed op: repeats and the
        // paired up ride the pinned down-time op.
        let mut m = super::SnapClassify::new(enabled());
        super::SnapClassify::push(&mut m, VK_LWIN, false, true, false);
        let down = super::SnapClassify::push(&mut m, super::VK_H, false, true, false)
            .and_then(|ev| match ev {
                super::Classified::Snap(intent) => Some(intent),
                _ => None,
            })
            .expect("fresh down");
        assert_eq!(down.op, SnapOp::Focus);
        m.sync_shift_from_async(true);
        let repeat = super::SnapClassify::push(&mut m, super::VK_H, false, true, false)
            .and_then(|ev| match ev {
                super::Classified::Snap(intent) => Some(intent),
                _ => None,
            })
            .expect("repeat");
        assert_eq!(repeat.op, SnapOp::Focus);
        assert!(repeat.consumed);
        assert!(
            !repeat.announce,
            "shift-flipped repeat swallows without dispatch"
        );
        let up = super::SnapClassify::push(&mut m, super::VK_H, true, true, false)
            .and_then(|ev| match ev {
                super::Classified::Snap(intent) => Some(intent),
                _ => None,
            })
            .expect("paired up");
        assert_eq!(up.op, SnapOp::Focus);
        assert!(up.consumed);
    }
}

#[cfg(test)]
mod windrag_mask_tests {
    use super::{KeyboardConfig, MaskTrigger, SnapClassify, VK_LWIN, VK_RWIN};

    fn enabled() -> KeyboardConfig {
        KeyboardConfig {
            takeover: true,
            allow_win_l: true,
        }
    }

    #[test]
    fn windrag_arms_mask_once_and_both_win_terminal_clears() {
        let mut m = SnapClassify::new(enabled());
        SnapClassify::push(&mut m, VK_LWIN, false, true, false);
        m.note_windrag_consumed();
        m.note_windrag_consumed();
        let first = m.win_up_needs_mask(VK_LWIN, true);
        assert_eq!(first, Some(MaskTrigger::WinDrag));
        // A fresh hold arms and fires exactly once; a stale extra Win-up
        // after the both-Wins terminal cleared the hold never re-fires.
        SnapClassify::push(&mut m, VK_RWIN, false, true, false);
        m.note_windrag_consumed();
        let _ = m.win_up_needs_mask(VK_RWIN, true);
        let second = m.win_up_needs_mask(VK_LWIN, true);
        assert_eq!(second, None);
    }

    #[test]
    fn windrag_mask_state_only_path_leaves_no_stale_flag() {
        // State-only path (installed=false, e.g. synthetic Win traffic the
        // callback filtered) updates bookkeeping without arming a send, and
        // the both-Wins-up terminal clears any pending flag.
        let mut m = SnapClassify::new(enabled());
        SnapClassify::push(&mut m, VK_LWIN, false, true, false);
        m.note_windrag_consumed();
        let _ = m.win_up_needs_mask(VK_LWIN, false);
        let again = m.win_up_needs_mask(VK_LWIN, false);
        assert_eq!(again, None);
    }

    #[test]
    fn injected_win_never_arms_mask() {
        // Injected input returns None before any bookkeeping: a synthetic
        // Win hold leaves no mask obligation, so a later physical Win-up
        // sends no product mask for it.
        let mut m = SnapClassify::new(enabled());
        let out = SnapClassify::push(&mut m, VK_LWIN, false, true, true);
        assert_eq!(out, None);
        let fire = m.win_up_needs_mask(VK_LWIN, true);
        assert_eq!(fire, None);
    }

    #[test]
    fn synthetic_note_cannot_corrupt_next_bare_physical_win() {
        // A synthetic mouse gesture notes while no physical Win is tracked:
        // the note must NOOP, so the next bare physical Win tap opens Start
        // normally with no product mask.
        let mut m = SnapClassify::new(enabled());
        m.note_windrag_consumed();
        SnapClassify::push(&mut m, VK_LWIN, false, true, false);
        let fire = m.win_up_needs_mask(VK_LWIN, true);
        assert_eq!(fire, None);
    }

    #[test]
    fn windrag_mask_waits_for_both_tracked_wins_up() {
        // Genuinely simultaneous both-Win hold: the first Win-up must not
        // fire while the other side is still tracked held; the terminal
        // release fires exactly once, then the hold is clean.
        let mut m = SnapClassify::new(enabled());
        SnapClassify::push(&mut m, VK_LWIN, false, true, false);
        SnapClassify::push(&mut m, VK_RWIN, false, true, false);
        m.note_windrag_consumed();
        assert_eq!(m.win_up_needs_mask(VK_LWIN, true), None);
        assert_eq!(
            m.win_up_needs_mask(VK_RWIN, true),
            Some(MaskTrigger::WinDrag)
        );
        assert_eq!(m.win_up_needs_mask(VK_LWIN, true), None);
        assert_eq!(m.win_up_needs_mask(VK_RWIN, true), None);
    }
}

#[cfg(test)]
mod history_tests {
    use super::{
        ChordAction, ChordDisable, ChordRemap, Classified, KeyboardConfig, MaskTrigger,
        SnapClassify, SnapEdge, VK_CONTROL, VK_DOWN, VK_H, VK_J, VK_K, VK_L, VK_LEFT, VK_LWIN,
        VK_MENU, VK_RIGHT, VK_SHIFT, VK_TAB, VK_UP, WorkspaceHistoryOp,
    };

    fn enabled() -> KeyboardConfig {
        KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        }
    }

    fn ctrl_down(m: &mut SnapClassify) {
        SnapClassify::push(m, VK_CONTROL, false, true, false);
    }

    fn ctrl_up(m: &mut SnapClassify) {
        SnapClassify::push(m, VK_CONTROL, true, true, false);
    }

    fn history_op(m: &mut SnapClassify, vk: u32) -> WorkspaceHistoryOp {
        match SnapClassify::push(m, vk, false, true, false).expect("history down") {
            Classified::WorkspaceHistory(intent) => {
                assert!(intent.consumed && intent.announce);
                intent.op
            }
            other => panic!("expected history, got {other:?}"),
        }
    }

    #[test]
    fn exact_ctrl_tab_routing_with_op_per_key() {
        // Win+Ctrl+Tab toggles, H/K/Left/Up step prev, J/L/Down/Right next.
        // Bare Tab/Chords without Win or Ctrl pass through untracked.
        for (vk, op) in [
            (VK_TAB, WorkspaceHistoryOp::Previous),
            (VK_H, WorkspaceHistoryOp::Prev),
            (VK_K, WorkspaceHistoryOp::Prev),
            (VK_LEFT, WorkspaceHistoryOp::Prev),
            (VK_UP, WorkspaceHistoryOp::Prev),
            (VK_J, WorkspaceHistoryOp::Next),
            (VK_L, WorkspaceHistoryOp::Next),
            (VK_DOWN, WorkspaceHistoryOp::Next),
            (VK_RIGHT, WorkspaceHistoryOp::Next),
        ] {
            let mut m = SnapClassify::new(enabled());
            SnapClassify::push(&mut m, VK_LWIN, false, true, false);
            ctrl_down(&mut m);
            assert_eq!(history_op(&mut m, vk), op);
            match SnapClassify::push(&mut m, vk, true, true, false).expect("history up") {
                Classified::WorkspaceHistory(intent) => {
                    assert_eq!(intent.op, op);
                    assert!(intent.consumed && !intent.announce);
                }
                other => panic!("expected history up, got {other:?}"),
            }
            ctrl_up(&mut m);
            assert!(!m.key_is_down(vk));
        }
        // Bare (no Win) and Alt-modified chords refuse with no pin, so their
        // paired ups also pass. Shift+Ctrl no longer refuses: item 2 routes
        // Win+Ctrl+Shift+H to the relative-send follow arm (history itself
        // still demands unshifted Win+Ctrl).
        let mut m = SnapClassify::new(enabled());
        assert_eq!(SnapClassify::push(&mut m, VK_TAB, false, true, false), None);
        assert_eq!(SnapClassify::push(&mut m, VK_TAB, true, true, false), None);
        let mut m = SnapClassify::new(enabled());
        SnapClassify::push(&mut m, VK_LWIN, false, true, false);
        SnapClassify::push(&mut m, VK_SHIFT, false, true, false);
        ctrl_down(&mut m);
        match SnapClassify::push(&mut m, VK_H, false, true, false).expect("relative send") {
            Classified::WorkspaceSend(intent) => {
                assert_eq!((intent.delta, intent.follow), (-1, true));
                assert!(intent.consumed && intent.announce);
            }
            other => panic!("expected relative send, got {other:?}"),
        }
        let mut m = SnapClassify::new(enabled());
        SnapClassify::push(&mut m, VK_LWIN, false, true, false);
        SnapClassify::push(&mut m, VK_MENU, false, true, false);
        ctrl_down(&mut m);
        assert_eq!(SnapClassify::push(&mut m, VK_H, false, true, false), None);
    }

    #[test]
    fn history_repeat_dispatches_while_ctrl_live() {
        // Held-key autorepeat: repeats dispatch while Win+Ctrl holds, stay
        // swallowed (no dispatch) once Ctrl leaves, pair closes consumed.
        let mut m = SnapClassify::new(enabled());
        SnapClassify::push(&mut m, VK_LWIN, false, true, false);
        ctrl_down(&mut m);
        assert_eq!(history_op(&mut m, VK_J), WorkspaceHistoryOp::Next);
        match SnapClassify::push(&mut m, VK_J, false, true, false).expect("repeat") {
            Classified::WorkspaceHistory(intent) => {
                assert_eq!(
                    (intent.op, intent.edge),
                    (WorkspaceHistoryOp::Next, SnapEdge::Repeat)
                );
                assert!(intent.consumed && intent.announce);
            }
            other => panic!("expected repeat, got {other:?}"),
        }
        ctrl_up(&mut m);
        match SnapClassify::push(&mut m, VK_J, false, true, false).expect("repeat") {
            Classified::WorkspaceHistory(intent) => assert!(intent.consumed && !intent.announce),
            other => panic!("expected swallowed repeat, got {other:?}"),
        }
        match SnapClassify::push(&mut m, VK_J, true, true, false).expect("up") {
            Classified::WorkspaceHistory(intent) => assert!(intent.consumed && !intent.announce),
            other => panic!("expected up, got {other:?}"),
        }
    }

    #[test]
    fn previous_hold_repeat_swallows_without_pingpong() {
        // Win+Ctrl+Tab down dispatches once; held repeats stay consumed and
        // masked but never re-dispatch (maximize parity: otherwise the hold
        // would pingpong between the two views). The paired release closes
        // consumed with no orphan hold behind it.
        let mut m = SnapClassify::new(enabled());
        SnapClassify::push(&mut m, VK_LWIN, false, true, false);
        ctrl_down(&mut m);
        assert_eq!(history_op(&mut m, VK_TAB), WorkspaceHistoryOp::Previous);
        for _ in 0..2 {
            match SnapClassify::push(&mut m, VK_TAB, false, true, false).expect("repeat") {
                Classified::WorkspaceHistory(intent) => {
                    assert_eq!(
                        (intent.op, intent.edge),
                        (WorkspaceHistoryOp::Previous, SnapEdge::Repeat)
                    );
                    assert!(intent.consumed && !intent.announce);
                }
                other => panic!("expected swallowed repeat, got {other:?}"),
            }
        }
        match SnapClassify::push(&mut m, VK_TAB, true, true, false).expect("up") {
            Classified::WorkspaceHistory(intent) => {
                assert_eq!(intent.op, WorkspaceHistoryOp::Previous);
                assert!(intent.consumed && !intent.announce);
            }
            other => panic!("expected up, got {other:?}"),
        }
        assert!(!m.key_is_down(VK_TAB));
        assert_eq!(SnapClassify::push(&mut m, VK_TAB, true, true, false), None);
        // Relative steps still re-dispatch while live (control case).
        let mut m = SnapClassify::new(enabled());
        SnapClassify::push(&mut m, VK_LWIN, false, true, false);
        ctrl_down(&mut m);
        assert_eq!(history_op(&mut m, VK_J), WorkspaceHistoryOp::Next);
        match SnapClassify::push(&mut m, VK_J, false, true, false).expect("repeat") {
            Classified::WorkspaceHistory(intent) => assert!(intent.consumed && intent.announce),
            other => panic!("expected dispatching repeat, got {other:?}"),
        }
    }

    #[test]
    fn history_held_rebind_disable_gate_repeat_but_keep_pair() {
        // Rebound history (Win+Ctrl+U -> Prev/H) dispatches; removing the
        // mapping mid-hold swallows repeats without dispatching while the
        // pair still closes consumed; disabling the canonical mid-hold does
        // the same; fresh presses afterwards pass through.
        let remap = || {
            vec![ChordRemap {
                from_vk: 0x55,
                from_shift: false,
                from_ctrl: true,
                from_alt: false,
                action: ChordAction::WorkspacePrev,
                to_vk: VK_H,
            }]
        };
        let mut m = SnapClassify::new(enabled());
        m.set_remap(remap());
        SnapClassify::push(&mut m, VK_LWIN, false, true, false);
        ctrl_down(&mut m);
        assert_eq!(history_op(&mut m, 0x55), WorkspaceHistoryOp::Prev);
        m.set_remap(Vec::new());
        match SnapClassify::push(&mut m, 0x55, false, true, false).expect("repeat") {
            Classified::WorkspaceHistory(intent) => assert!(intent.consumed && !intent.announce),
            other => panic!("expected swallowed repeat, got {other:?}"),
        }
        match SnapClassify::push(&mut m, 0x55, true, true, false).expect("up") {
            Classified::WorkspaceHistory(intent) => assert!(intent.consumed),
            other => panic!("expected up, got {other:?}"),
        }
        assert_eq!(SnapClassify::push(&mut m, 0x55, false, true, false), None);
        // Canonical disable mid-hold gates dispatch but keeps the pair.
        let mut m = SnapClassify::new(enabled());
        SnapClassify::push(&mut m, VK_LWIN, false, true, false);
        ctrl_down(&mut m);
        assert_eq!(history_op(&mut m, VK_H), WorkspaceHistoryOp::Prev);
        m.set_disabled(vec![ChordDisable {
            vk: VK_H,
            shift: false,
            ctrl: true,
            alt: false,
        }]);
        match SnapClassify::push(&mut m, VK_H, false, true, false).expect("repeat") {
            Classified::WorkspaceHistory(intent) => assert!(intent.consumed && !intent.announce),
            other => panic!("expected swallowed repeat, got {other:?}"),
        }
        match SnapClassify::push(&mut m, VK_H, true, true, false).expect("up") {
            Classified::WorkspaceHistory(intent) => assert!(intent.consumed),
            other => panic!("expected up, got {other:?}"),
        }
    }

    #[test]
    fn history_arms_mask_and_survives_saturation() {
        // A consumed history chord arms the Start-menu mask; saturation drops
        // only queued evidence while the verdict still swallows.
        use super::{SnapQueue, classify_and_queue, win_up_mask_reserve};
        let mut m = SnapClassify::new(enabled());
        let mut q = SnapQueue::new();
        SnapClassify::push(&mut m, VK_LWIN, false, true, false);
        ctrl_down(&mut m);
        let down = SnapClassify::push(&mut m, VK_TAB, false, true, false).expect("down");
        assert!(down.consumed() && down.announce());
        assert!(win_up_mask_reserve(
            &mut m,
            &mut q,
            VK_LWIN,
            true,
            std::time::Instant::now()
        ));
        match q.pop_front().expect("mask") {
            super::QueuedSnapEvent::Mask(mask) => {
                assert_eq!(
                    mask.trigger,
                    MaskTrigger::WorkspaceHistory {
                        op: WorkspaceHistoryOp::Previous
                    }
                )
            }
            other => panic!("expected mask, got {other:?}"),
        }
        // Queue/mask routing: history classifies and queues with announce.
        let mut m = SnapClassify::new(enabled());
        let mut q = SnapQueue::new();
        let tick = std::time::Instant::now();
        assert_eq!(
            classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
            None
        );
        assert_eq!(
            classify_and_queue(&mut m, &mut q, VK_CONTROL, false, None, false, tick, true),
            None
        );
        assert_eq!(
            classify_and_queue(&mut m, &mut q, VK_J, false, None, false, tick, true),
            Some(true)
        );
        match q.pop_front().expect("history") {
            super::QueuedSnapEvent::WorkspaceHistory(queued) => {
                assert_eq!(queued.op, WorkspaceHistoryOp::Next);
                assert!(queued.consumed && queued.announce);
            }
            other => panic!("expected history, got {other:?}"),
        }
    }

    #[test]
    fn history_cross_arm_holds_never_collide() {
        // Win+H (focus) and Win+Ctrl+H (history-prev) share VK_H across
        // separate slots: both hold independently, release in any order.
        let mut m = SnapClassify::new(enabled());
        SnapClassify::push(&mut m, VK_LWIN, false, true, false);
        match SnapClassify::push(&mut m, VK_H, false, true, false).expect("focus") {
            Classified::Snap(intent) => assert!(intent.consumed),
            other => panic!("expected focus, got {other:?}"),
        }
        ctrl_down(&mut m);
        // Same physical H is already pinned to focus: this is the owner's
        // repeat (swallowed, no dispatch flip into history).
        match SnapClassify::push(&mut m, VK_H, false, true, false).expect("repeat") {
            Classified::Snap(intent) => assert!(intent.consumed && !intent.announce),
            other => panic!("expected pinned focus repeat, got {other:?}"),
        }
        ctrl_up(&mut m);
        match SnapClassify::push(&mut m, VK_H, true, true, false).expect("up") {
            Classified::Snap(intent) => assert!(intent.consumed),
            other => panic!("expected focus up, got {other:?}"),
        }
        // Fresh history after the focus hold closed.
        ctrl_down(&mut m);
        assert_eq!(history_op(&mut m, VK_H), WorkspaceHistoryOp::Prev);
    }
}

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
    WorkspaceHistory(QueuedWorkspaceHistoryIntent),
    Maximize(QueuedMaximizeIntent),
    Fullscreen(QueuedFullscreenIntent),
    Float(QueuedFloatIntent),
    Sticky(QueuedStickyIntent),
    Orientation(QueuedOrientationIntent),
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
    fn back_mut(&mut self) -> Option<&mut MarkedKeyDiag> {
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

#[cfg(windows)]
pub mod sys {
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
    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
        VK_CONTROL, VK_MENU, VK_SHIFT,
    };
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
        let foreground = unsafe { GetForegroundWindow() } as usize as u64;
        st.origins.get(&foreground).cloned()
    }

    /// Minimal callback: classification plus one bounded queue push. No
    /// Engine, geometry, or log I/O here. A physical Win-up that closes a
    /// hold with a consumed chord sends the E8 mask pair first (with no SNAP
    /// borrow held); the Win-up itself always passes through.
    ///
    /// Proof-only diagnostics: accepted marked events (shortcut-proof only)
    /// record one bounded [`super::MarkedKeyDiag`] each, so live runs can
    /// distinguish actual callback modifier delivery from classifier state.
    /// Unmarked input is never recorded.
    unsafe extern "system" fn llproc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code as u32 == HC_ACTION {
            let kb = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
            let vk = kb.vkCode;
            let scan = kb.scanCode;
            let flags = kb.flags;
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
                let gap = unsafe { GetTickCount() }.saturating_sub(kb.time);
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
                let extra = kb.dwExtraInfo as u64;
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
                    return unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
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
                return unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
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
                    let async_down = |key: i32| unsafe { GetAsyncKeyState(key) } < 0;
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
                                    .map(|t| {
                                        t.elapsed().as_micros().min(u128::from(u32::MAX)) as u32
                                    })
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
                return 1;
            }
        }
        unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
    }

    struct MaskSend {
        inserted: u8,
        release_sent: bool,
    }

    /// Sends the E8 down/up pair. Must be called with no SNAP borrow held
    /// (see llproc). Returns the accepted-event count plus whether the bounded
    /// partial-release ran. Accepted exception to the no-native-I/O callback
    /// rule: the pair must ride the same Win hold at Win-up time.
    fn send_mask_pair() -> MaskSend {
        unsafe {
            let mut pair: [INPUT; 2] = std::mem::zeroed();
            pair[0].r#type = INPUT_KEYBOARD;
            pair[0].Anonymous.ki = KEYBDINPUT {
                wVk: super::VK_MASK as u16,
                wScan: 0,
                dwFlags: 0,
                time: 0,
                dwExtraInfo: 0,
            };
            pair[1].r#type = INPUT_KEYBOARD;
            pair[1].Anonymous.ki = KEYBDINPUT {
                wVk: super::VK_MASK as u16,
                wScan: 0,
                dwFlags: KEYEVENTF_KEYUP,
                time: 0,
                dwExtraInfo: 0,
            };
            let size = std::mem::size_of::<INPUT>() as i32;
            let inserted = SendInput(2, pair.as_ptr(), size);
            if inserted == 1 {
                // Partial pair: the E8 down reached the system without its
                // up. One bounded E8-up release, nothing else repaired.
                let release = SendInput(1, pair.as_ptr().add(1), size);
                MaskSend {
                    inserted: 1,
                    release_sent: release == 1,
                }
            } else {
                MaskSend {
                    inserted: inserted as u8,
                    release_sent: false,
                }
            }
        }
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
    pub fn install(config: KeyboardConfig) -> std::result::Result<HHOOK, String> {
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
        let hmod = unsafe { GetModuleHandleW(std::ptr::null()) };
        let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(llproc), hmod, 0) };
        if hook.is_null() {
            SNAP.with(|s| *s.borrow_mut() = None);
            return Err("error: SetWindowsHookExW(WH_KEYBOARD_LL) failed".to_owned());
        }
        Ok(hook)
    }

    /// Test-only install for `shortcut-proof`: identical callback, dispatcher,
    /// and mask routines, but synthetic input carrying exactly
    /// [`super::SHORTCUT_PROOF_MARKER`] in `dwExtraInfo` classifies as
    /// physical (still origin-gated by the owner recheck). All other injected
    /// input passes through. Never used by product `tile`.
    pub fn install_proof(config: KeyboardConfig) -> std::result::Result<HHOOK, String> {
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
        let hmod = unsafe { GetModuleHandleW(std::ptr::null()) };
        let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(llproc), hmod, 0) };
        if hook.is_null() {
            SNAP.with(|s| *s.borrow_mut() = None);
            return Err("error: SetWindowsHookExW(WH_KEYBOARD_LL) failed".to_owned());
        }
        Ok(hook)
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
    pub fn uninstall(hook: &mut HHOOK) -> bool {
        let ok = unsafe { UnhookWindowsHookEx(*hook) } != 0;
        if !ok {
            return false;
        }
        *hook = std::ptr::null_mut();
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

    /// Test-only hook-state installer (no `SetWindowsHookExW`, no native
    /// effects): mirrors [`install`](Self::install)'s TLS init so step tests
    /// drive the production apply/drain logic without hooks.
    #[cfg(test)]
    pub fn test_ensure_state(config: KeyboardConfig) {
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

    /// Test-only synthetic `llproc` drive (no hooks, no `SendInput`): feeds
    /// one fabricated low-level event through the real callback. Safe
    /// inputs only: injected-filtered traffic, Esc downs, and ordinary
    /// physical keys. Never drive a Win-up that could reserve the mask
    /// (that path performs a real `SendInput`).
    ///
    /// # Safety
    ///
    /// Test-only: the fabricated `KBDLLHOOKSTRUCT` lives on the stack for
    /// the synchronous call; the callback only reads it and passes it to
    /// `CallNextHookEx` (no hook installed, no effects). Callers must not
    /// drive inputs that trigger `SendInput` (see above).
    #[cfg(test)]
    pub unsafe fn test_drive_llproc(
        vk: u32,
        scan: u32,
        flags: u32,
        extra: u64,
    ) -> windows_sys::Win32::Foundation::LRESULT {
        use windows_sys::Win32::UI::WindowsAndMessaging::KBDLLHOOKSTRUCT;
        let kb = KBDLLHOOKSTRUCT {
            vkCode: vk,
            scanCode: scan,
            flags,
            time: 0,
            dwExtraInfo: extra as usize,
        };
        unsafe { llproc(HC_ACTION as i32, 0, &kb as *const KBDLLHOOKSTRUCT as isize) }
    }

    #[cfg(test)]
    mod sys_tests {
        use super::*;
        use crate::input_handoff::{
            global_esc_clock, global_esc_edge, global_windrag_esc_clock, global_windrag_esc_edge,
        };
        use std::sync::atomic::Ordering;

        fn fresh_state() {
            test_ensure_state(KeyboardConfig {
                takeover: true,
                allow_win_l: false,
            });
        }

        #[test]
        fn own_e8_reentry_passes_through_with_no_state_change() {
            // The mask `SendInput` pair reenters this callback on the same
            // thread as injected E8 with `dwExtraInfo` 0: product filters ALL
            // injected, so the reentry passes through with no borrow held,
            // no queue push, no sequence bump, and no panic.
            fresh_state();
            let tls_before = SNAP.with(|s| {
                s.borrow()
                    .as_ref()
                    .map(|st| {
                        (
                            st.queue.len(),
                            st.queue.dropped,
                            st.esc_seq,
                            st.windrag_esc_seq,
                        )
                    })
                    .unwrap_or((0, 0, 0, 0))
            });
            unsafe {
                test_drive_llproc(super::super::VK_MASK, 0, LLKHF_INJECTED, 0);
                test_drive_llproc(super::super::VK_MASK, 0, LLKHF_INJECTED | LLKHF_UP, 0);
            }
            // Thread-local state untouched (process-global clocks are shared
            // with parallel tests, so only TLS effects assert exactly here).
            let tls_after = SNAP.with(|s| {
                s.borrow()
                    .as_ref()
                    .map(|st| {
                        (
                            st.queue.len(),
                            st.queue.dropped,
                            st.esc_seq,
                            st.windrag_esc_seq,
                        )
                    })
                    .unwrap_or((0, 0, 0, 0))
            });
            assert_eq!(tls_before, tls_after);
            // The reentry is trace-visible (closed E8 vocabulary) but never
            // classifier state: enabling trace records exactly the two
            // filtered summaries and nothing else.
            SNAP.with(|s| {
                if let Some(st) = s.borrow_mut().as_mut() {
                    st.cb_diag_enabled = true;
                }
            });
            unsafe {
                test_drive_llproc(super::super::VK_MASK, 0, LLKHF_INJECTED, 0);
            }
            let diags = drain_callback_diag();
            assert_eq!(diags.len(), 1);
            assert_eq!(diags[0].reason, CallbackReason::InjectedFiltered);
            assert!(!diags[0].consumed);
            test_clear_state();
        }

        #[test]
        fn physical_esc_sets_both_edges_and_bumps_both_clocks() {
            // Serial guard: the global edge assertions race a concurrent
            // pump `ClearEsc` apply (which stores false).
            let _guard = crate::input_handoff::fence_test_serial();
            fresh_state();
            let esc_before = global_esc_clock().load();
            let windrag_before = global_windrag_esc_clock().load();
            unsafe {
                test_drive_llproc(super::super::VK_ESCAPE, 1, 0, 0);
            }
            let (edge, windrag_edge) = SNAP.with(|s| {
                s.borrow()
                    .as_ref()
                    .map(|st| (st.esc_edge, st.windrag_esc_edge))
                    .unwrap_or((false, false))
            });
            assert!(edge && windrag_edge);
            assert!(global_esc_clock().load() >= esc_before.wrapping_add(1));
            assert!(global_windrag_esc_clock().load() >= windrag_before.wrapping_add(1));
            assert!(global_esc_edge().load(Ordering::SeqCst));
            assert!(global_windrag_esc_edge().load(Ordering::SeqCst));
            test_clear_state();
        }

        #[test]
        fn injected_esc_sets_only_the_windrag_edge() {
            // Synthetic Esc (unmarked, like a `SendInput` cancel): never a
            // command, never the native edge, but still cancels an open
            // Win+Left drag that has no native modal loop. Serial guard:
            // same global-edge race as the physical Esc test above.
            let _guard = crate::input_handoff::fence_test_serial();
            fresh_state();
            unsafe {
                test_drive_llproc(super::super::VK_ESCAPE, 1, LLKHF_INJECTED, 0);
            }
            let (edge, windrag_edge, esc_seq_delta) = SNAP.with(|s| {
                s.borrow()
                    .as_ref()
                    .map(|st| (st.esc_edge, st.windrag_esc_edge, st.esc_seq))
                    .unwrap_or((true, false, 1))
            });
            assert!(!edge && windrag_edge);
            assert_eq!(esc_seq_delta, 0);
            assert!(global_windrag_esc_edge().load(Ordering::SeqCst));
            test_clear_state();
        }

        #[test]
        fn untracked_win_up_passes_without_mask_send() {
            // A Win-up with no tracked hold reserves nothing, sends nothing
            // (no real `SendInput`), and always passes through.
            fresh_state();
            let (sends_before, _) = mask_send_stats();
            unsafe {
                test_drive_llproc(super::super::VK_LWIN, 0, LLKHF_UP, 0);
            }
            let (sends_after, _) = mask_send_stats();
            assert_eq!(sends_before, sends_after);
            test_clear_state();
        }

        #[test]
        fn publication_fence_refuses_fresh_chord_through_real_callback() {
            // Real `llproc` through the test-drive seam: while the fence is
            // pending (policy published, pump not yet applied) a fresh chord
            // passes through instead of consuming under stale policy; after
            // the pump applies, fresh chords consume, and a consumed hold
            // rides its stored verdict (repeat, paired Up, mask obligation)
            // across a later fence window. No `SendInput` here: Win-up is
            // never driven (the mask reserve is asserted directly).
            use crate::input_handoff::{
                fence_test_allocate, fence_test_serial, keyboard_fresh_gate, note_policy_applied,
                note_policy_published,
            };
            let _guard = fence_test_serial();
            let base = fence_test_allocate();
            fresh_state();
            SNAP.with(|s| {
                if let Some(st) = s.borrow_mut().as_mut() {
                    st.active = true;
                }
            });
            unsafe {
                test_drive_llproc(super::super::VK_LWIN, 0, 0, 0);
            }
            assert!(SNAP.with(|s| { s.borrow().as_ref().is_some_and(|st| st.machine.win_held()) }));
            // Fence pending: the fresh H down passes through the real
            // callback (never swallowed under stale policy).
            note_policy_published(base + 1);
            assert!(!keyboard_fresh_gate(true));
            let verdict = unsafe { test_drive_llproc(super::super::VK_H, 0, 0, 0) };
            assert_ne!(verdict, 1);
            assert!(SNAP.with(|s| {
                s.borrow()
                    .as_ref()
                    .is_some_and(|st| st.machine.key_is_down(super::super::VK_H))
            }));
            // Pump applies: a new fresh chord consumes through the callback.
            note_policy_applied(base + 1);
            assert!(keyboard_fresh_gate(true));
            let verdict = unsafe { test_drive_llproc(super::super::VK_J, 0, 0, 0) };
            assert_eq!(verdict, 1);
            // A newer policy publishes mid-hold: the repeat still rides the
            // stored consumed verdict, the mask obligation survives, and the
            // paired Up stays swallowed; the passed H pair still closes
            // passing (never stolen).
            note_policy_published(base + 2);
            assert!(!keyboard_fresh_gate(true));
            let verdict = unsafe { test_drive_llproc(super::super::VK_J, 0, 0, 0) };
            assert_eq!(verdict, 1);
            let mask_armed = SNAP.with(|s| {
                let mut borrow = s.borrow_mut();
                let st = borrow.as_mut().expect("hook state");
                super::super::win_up_mask_reserve(
                    &mut st.machine,
                    &mut st.queue,
                    super::super::VK_LWIN,
                    true,
                    std::time::Instant::now(),
                )
            });
            assert!(mask_armed);
            let verdict = unsafe { test_drive_llproc(super::super::VK_J, 0, LLKHF_UP, 0) };
            assert_eq!(verdict, 1);
            let verdict = unsafe { test_drive_llproc(super::super::VK_H, 0, LLKHF_UP, 0) };
            assert_ne!(verdict, 1);
            note_policy_applied(base + 2);
            assert!(keyboard_fresh_gate(true));
            test_clear_state();
        }

        #[test]
        fn proof_marker_matrix_accepts_only_the_exact_marker() {
            assert!(!super::super::accept_proof_injected(0, None));
            assert!(!super::super::accept_proof_injected(
                super::super::SHORTCUT_PROOF_MARKER,
                None
            ));
            assert!(!super::super::accept_proof_injected(
                0,
                Some(super::super::SHORTCUT_PROOF_MARKER)
            ));
            assert!(super::super::accept_proof_injected(
                super::super::SHORTCUT_PROOF_MARKER,
                Some(super::super::SHORTCUT_PROOF_MARKER)
            ));
        }
    }
}
