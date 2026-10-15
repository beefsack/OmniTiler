use super::*;

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
/// Workspace-tiling toggle chord key (Win+Y, KDE Meta+Y parity, item 21).
/// `vk_for_key_name` in settings recognizes `Y`; the classifier treats it
/// as a workspace-tiling arm only unshifted with no Ctrl/Alt (never a
/// directional/digit/history arm).
pub const VK_Y: u32 = 0x59;
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
/// relative follow/stay (prev/next each); item 5 adds output follow/stay
/// (direction from the canonical key); existing arms keep their
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
    WorkspaceOutputFollow,
    WorkspaceOutputStay,
    WorkspaceMigrate,
    Maximize,
    Fullscreen,
    Float,
    Sticky,
    Orientation,
    WorkspaceTiling,
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
            Self::WorkspaceOutputFollow => "workspace-output-follow",
            Self::WorkspaceOutputStay => "workspace-output-stay",
            Self::WorkspaceMigrate => "workspace-migrate",
            Self::Maximize => "maximize",
            Self::Fullscreen => "fullscreen",
            Self::Float => "float",
            Self::Sticky => "sticky",
            Self::Orientation => "orientation",
            Self::WorkspaceTiling => "workspace-tiling",
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
pub(super) struct PinnedRoute {
    pub(super) canon: u32,
    pub(super) action: ChordAction,
}

/// Physical routing-table size: every chord virtual key fits in one byte.
pub(super) const PIN_KEYS: usize = 256;

/// Fresh-down routing outcome for one physical chord (see
/// [`SnapClassify::fresh_route`], a private helper).
pub(super) enum Route {
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
/// orientation, workspace-tiling, and launcher toggles plus the item 1 history keys (Tab/H/J/K/L/arrows with
/// Ctrl). Modifiers, Win keys, and ordinary keys are not chord keys.
#[must_use]
pub fn is_chord_vk(vk: u32) -> bool {
    catalog_index(vk).is_some()
        || is_digit_vk(vk)
        || is_maximize_vk(vk)
        || is_fullscreen_vk(vk)
        || is_float_vk(vk)
        || is_orientation_vk(vk)
        || is_workspace_tiling_vk(vk)
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
pub(super) fn relative_send_repeat_live(ctrl: bool, shift: bool, alt: bool, win: bool) -> bool {
    ctrl && shift && !alt && win
}

/// Item 14 migration slot index for a canonical letter key: 0 H (left),
/// 1 L (right), 2 K (up), 3 J (down). Arrow aliases route into their
/// direction's letter slot through the rebound table, so they share the
/// letter's hold; a bare arrow canonical never arrives here (direct
/// Win+Ctrl+Alt+Shift chords refuse) and refuses.
#[must_use]
pub const fn migrate_index(vk: u32) -> Option<usize> {
    match vk {
        VK_H => Some(0),
        VK_L => Some(1),
        VK_K => Some(2),
        VK_J => Some(3),
        _ => None,
    }
}

/// Direction for a migration slot index. Letters are the canonical keys, so
/// no arrow aliasing applies here.
pub(super) fn migrate_direction(idx: usize) -> Direction {
    match idx {
        0 => Direction::Left,
        1 => Direction::Right,
        2 => Direction::Up,
        _ => Direction::Down,
    }
}

/// Whether a live modifier/Win combination still matches a pinned output
/// send (item 5): Win+Ctrl+Alt held, Win still held, Shift still matching the
/// pinned arm (direct follow pins unshifted; a stay row rebound with Shift
/// pins Shift). Same swallow-but-don't-dispatch contract as relative sends;
/// follow/stay rides the pinned intent, never the live modifiers.
pub(super) fn output_repeat_live(
    ctrl: bool,
    shift: bool,
    alt: bool,
    win: bool,
    pinned_shift: bool,
) -> bool {
    ctrl && alt && win && (shift == pinned_shift)
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

/// True only for the workspace-tiling toggle chord key (Win+Y, KDE Meta+Y
/// parity, item 21). Like orientation, any Shift/Ctrl/Alt combination passes
/// through untracked: there is no shifted workspace-tiling arm.
#[must_use]
pub const fn is_workspace_tiling_vk(vk: u32) -> bool {
    vk == VK_Y
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
pub(super) fn index_direction(idx: usize) -> Direction {
    match idx {
        0 | 4 => Direction::Left,
        1 | 5 => Direction::Down,
        2 | 6 => Direction::Up,
        _ => Direction::Right,
    }
}

/// True only for the letter L slot (unshifted Win+L is the OS lock chord).
pub(super) fn is_letter_l(idx: usize) -> bool {
    idx == 3
}

/// Whether a live modifier/Win combination still matches a pinned directional
/// op: no extra modifiers, Win still held, Shift still selecting the pinned
/// op, and the Win+L opt-in still satisfied. Consumed-hold repeats stay
/// swallowed regardless; only this decides the repeat `announce` (dispatch).
pub(super) fn snap_repeat_live(shift: bool, ctrl: bool, alt: bool, win: bool, op: SnapOp) -> bool {
    !ctrl && !alt && win && (shift == (op == SnapOp::Move))
}

/// Physical Win+L opt-in fence for a pinned directional op: an unshifted
/// (focus) physical L without opt-in never dispatches, so a Shift+L move hold
/// whose Shift is released mid-hold swallows its repeats without dispatching.
/// Takes the physical chord key: a safe rebound into the canonical L slot is
/// not the OS lock chord and always admits.
pub(super) fn snap_op_admits(physical_vk: u32, op: SnapOp, allow_win_l: bool) -> bool {
    !(op == SnapOp::Focus && physical_vk == VK_L && !allow_win_l)
}

/// Whether a live modifier/Win combination still matches a pinned workspace
/// op: no extra modifiers, Win still held, Shift still selecting the pinned
/// op. Same swallow-but-don't-dispatch contract as [`snap_repeat_live`].
pub(super) fn digit_repeat_live(
    shift: bool,
    ctrl: bool,
    alt: bool,
    win: bool,
    op: WorkspaceOp,
) -> bool {
    !ctrl && !alt && win && (shift == (op == WorkspaceOp::Send))
}

/// Whether a live modifier/Win combination still matches a pinned resize
/// mode: Win+Alt held, no Ctrl, Win still held, Shift still selecting the
/// pinned mode (unshifted outwards, Shifted inwards). Same
/// swallow-but-don't-dispatch contract as [`snap_repeat_live`]: the repeat
/// arm stays pinned to the required modifiers/action.
pub(super) fn resize_repeat_live(
    shift: bool,
    ctrl: bool,
    alt: bool,
    win: bool,
    mode: ResizeMode,
) -> bool {
    alt && !ctrl && win && (shift == (mode == ResizeMode::Inwards))
}

/// Resize mode for a Shift state: unshifted Alt grows outwards, Shift+Alt
/// shrinks inwards.
pub(super) fn resize_mode_for_shift(shift: bool) -> ResizeMode {
    if shift {
        ResizeMode::Inwards
    } else {
        ResizeMode::Outwards
    }
}

/// Explicit resize action for a mode: the bound action carries the mode, so
/// a resize intent can never be disguised as a move.
pub(super) fn resize_action_for_mode(mode: ResizeMode) -> ChordAction {
    match mode {
        ResizeMode::Outwards => ChordAction::ResizeOut,
        ResizeMode::Inwards => ChordAction::ResizeIn,
    }
}

/// Resize mode for an explicit bound action.
pub(super) fn resize_mode_for_action(action: ChordAction) -> Option<ResizeMode> {
    match action {
        ChordAction::ResizeOut => Some(ResizeMode::Outwards),
        ChordAction::ResizeIn => Some(ResizeMode::Inwards),
        _ => None,
    }
}

/// Whether a live modifier/Win combination still matches a pinned history
/// op: Win+Ctrl held, no Shift/Alt, Win still held. Same
/// swallow-but-don't-dispatch contract as directional repeats.
pub(super) fn history_repeat_live(ctrl: bool, shift: bool, alt: bool, win: bool) -> bool {
    ctrl && !shift && !alt && win
}

/// Collision refusal from the live owner's hold shape: same variant and
/// action identity, refreshed edge/foreground, the collider's own consumed
/// verdict, and never dispatched (`announce` is always false, so the owner
/// drain settles it without acting).
pub(super) fn collide_refusal(
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
        Classified::WorkspaceOutput(intent) => Classified::WorkspaceOutput(WorkspaceOutputIntent {
            direction: intent.direction,
            follow: intent.follow,
            edge,
            foreground,
            consumed,
            announce: false,
        }),
        Classified::WorkspaceMigrate(intent) => {
            Classified::WorkspaceMigrate(WorkspaceMigrateIntent {
                direction: intent.direction,
                edge,
                foreground,
                consumed,
                announce: false,
            })
        }
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
        Classified::WorkspaceTiling(_) => Classified::WorkspaceTiling(WorkspaceTilingIntent {
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
pub(super) fn collide_trigger(shape: &Classified) -> Option<MaskTrigger> {
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
        Classified::WorkspaceOutput(intent) => Some(MaskTrigger::WorkspaceOutput {
            direction: intent.direction,
            follow: intent.follow,
        }),
        Classified::WorkspaceMigrate(intent) => Some(MaskTrigger::WorkspaceMigrate {
            direction: intent.direction,
        }),
        Classified::WorkspaceHistory(intent) => {
            Some(MaskTrigger::WorkspaceHistory { op: intent.op })
        }
        Classified::Maximize(_) => Some(MaskTrigger::Maximize),
        Classified::Orientation(_) => Some(MaskTrigger::Orientation),
        Classified::WorkspaceTiling(_) => Some(MaskTrigger::WorkspaceTiling),
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

pub(super) fn is_modifier_vk(vk: u32) -> bool {
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
