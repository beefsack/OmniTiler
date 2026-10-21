#![forbid(unsafe_code)]
use super::chords::PinnedRoute;
use super::*;

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
/// portable session policy.
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

/// Classifier outcome for one item 5 explicit directional output-send event
/// (Win+Ctrl+Alt+H/J/K/L plus arrow aliases follow; stay rows rebind through
/// the stay action and stay). Only downs and repeats dispatch, ups close the
/// pair. `direction` pins from the canonical key; `follow` pins at down time
/// from the explicit action (direct chords follow; the unbound stay rows
/// rebind through the stay action and stay).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceOutputIntent {
    pub direction: Direction,
    pub follow: bool,
    pub edge: SnapEdge,
    pub foreground: bool,
    pub consumed: bool,
    pub announce: bool,
}

/// Classifier outcome for one item 14 whole-workspace migration event
/// (rebound Win+Ctrl+Alt+Shift+H/J/K/L or arrow alias, follow-only).
/// Only downs dispatch, ups close the pair; held repeats stay swallowed like
/// maximize (a held migration never re-fires). `direction` pins
/// from the canonical letter key; there is no stay variant (D1 follow-only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceMigrateIntent {
    pub direction: Direction,
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

/// Classifier outcome for one workspace-tiling toggle event (Win+Y, KDE
/// Meta+Y parity, item 21). The toggle carries no direction and acts on the
/// current workspace, never a focused window: only the down dispatches, ups
/// close the pair. Held repeats stay swallowed like orientation (a held Y
/// would otherwise pingpong the workspace mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceTilingIntent {
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
    WorkspaceOutput(WorkspaceOutputIntent),
    WorkspaceMigrate(WorkspaceMigrateIntent),
    WorkspaceHistory(WorkspaceHistoryIntent),
    Maximize(MaximizeIntent),
    Fullscreen(FullscreenIntent),
    Float(FloatIntent),
    Sticky(StickyIntent),
    Orientation(OrientationIntent),
    WorkspaceTiling(WorkspaceTilingIntent),
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
            Self::WorkspaceOutput(intent) => intent.consumed,
            Self::WorkspaceMigrate(intent) => intent.consumed,
            Self::WorkspaceHistory(intent) => intent.consumed,
            Self::Maximize(intent) => intent.consumed,
            Self::Fullscreen(intent) => intent.consumed,
            Self::Float(intent) => intent.consumed,
            Self::Sticky(intent) => intent.consumed,
            Self::Orientation(intent) => intent.consumed,
            Self::WorkspaceTiling(intent) => intent.consumed,
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
            Self::WorkspaceOutput(intent) => intent.announce,
            Self::WorkspaceMigrate(intent) => intent.announce,
            Self::WorkspaceHistory(intent) => intent.announce,
            Self::Maximize(intent) => intent.announce,
            Self::Fullscreen(intent) => intent.announce,
            Self::Float(intent) => intent.announce,
            Self::Sticky(intent) => intent.announce,
            Self::Orientation(intent) => intent.announce,
            Self::WorkspaceTiling(intent) => intent.announce,
            Self::Launch(intent) => intent.announce,
        }
    }
}

/// Which chord armed the Start-menu mask. Digits, history, resize, maximize,
/// fullscreen, and float arm it exactly like directional chords: any consumed
/// chord in the Win hold needs the E8 pair at Win-up, or the OS opens Start.
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
    WorkspaceOutput {
        direction: Direction,
        follow: bool,
    },
    WorkspaceMigrate {
        direction: Direction,
    },
    WorkspaceHistory {
        op: WorkspaceHistoryOp,
    },
    Maximize,
    Fullscreen,
    Float,
    Sticky,
    Orientation,
    WorkspaceTiling,
    LaunchBrowser,
    LaunchTerminal,
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
    pub(super) win_l: bool,
    pub(super) win_r: bool,
    pub(super) shift: bool,
    pub(super) ctrl: bool,
    pub(super) alt: bool,
    pub(super) key_down: [bool; 8],
    pub(super) key_origin: [bool; 8],
    pub(super) key_op: [Option<SnapOp>; 8],
    pub(super) digit_down: [bool; 10],
    pub(super) digit_origin: [bool; 10],
    pub(super) digit_op: [Option<WorkspaceOp>; 10],
    pub(super) digit_follow: [bool; 10],
    /// Pinned Ctrl state per digit hold: numbered stay rides Win+Shift and
    /// additionally the Win+Ctrl+Shift backlog arm, so the repeat dispatch
    /// must follow the pinned arm (a mid-hold Ctrl flip swallows without
    /// dispatching, never leaking a repeat or flipping into follow).
    /// Follow/select holds always pin false via the fresh-down guard.
    pub(super) digit_ctrl: [bool; 10],
    pub(super) send_down: [bool; 8],
    pub(super) send_origin: [bool; 8],
    pub(super) send_follow: [bool; 8],
    pub(super) send_delta: [i32; 8],
    pub(super) output_down: [bool; 8],
    pub(super) output_origin: [bool; 8],
    pub(super) output_follow: [bool; 8],
    /// Pinned Shift state per output hold: follow rides Win+Ctrl+Alt unshifted
    /// while a stay row rebound with Shift pins Shift, so the repeat dispatch
    /// follows the pinned arm (a mid-hold Shift flip swallows without
    /// dispatching, never leaking a repeat or flipping follow into stay).
    /// Direct follow holds always pin false via the fresh-down guard.
    pub(super) output_shift: [bool; 8],
    /// Item 14 migration holds per direction slot (letter canonicals H/L/K/J):
    /// follow-only, so no intent pin is needed beyond the slot itself. Held
    /// repeats stay swallowed, never re-dispatched.
    pub(super) migrate_down: [bool; 4],
    pub(super) migrate_origin: [bool; 4],
    pub(super) maximize_down: bool,
    pub(super) maximize_origin: bool,
    pub(super) fullscreen_down: bool,
    pub(super) fullscreen_origin: bool,
    pub(super) float_down: bool,
    pub(super) float_origin: bool,
    pub(super) sticky_down: bool,
    pub(super) sticky_origin: bool,
    pub(super) orientation_down: bool,
    pub(super) orientation_origin: bool,
    pub(super) workspace_tiling_down: bool,
    pub(super) workspace_tiling_origin: bool,
    pub(super) launch_browser_down: bool,
    pub(super) launch_browser_origin: bool,
    pub(super) launch_terminal_down: bool,
    pub(super) launch_terminal_origin: bool,
    pub(super) resize_down: [bool; 8],
    pub(super) resize_origin: [bool; 8],
    pub(super) resize_mode: [Option<ResizeMode>; 8],
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
    pub(super) vk_remap: Vec<ChordRemap>,
    /// Owner-published suppression table (empty means everything kept):
    /// disabled or rebound-away physical chords pass through untracked.
    /// Checked after the rebound table, fresh downs only; paired key-ups ride
    /// their stored down verdict.
    pub(super) vk_disabled: Vec<ChordDisable>,
    /// Down-time physical routing pinned through the matching up: index by
    /// physical virtual key (all chord keys fit in one byte), value is the
    /// canonical key plus explicit action the down routed to. A mid-hold
    /// table change can neither orphan the hold (ups resolve through the pin)
    /// nor leak its pair (repeats ride the stored verdict with dispatch gated
    /// on the chord still routing live), and the Start-menu mask obligation
    /// survives untouched. Only a hold the family actually owns pins: refused
    /// fresh downs (bare, extra-modified, arm-refused, fenced, suppressed,
    /// gated off) leave no pin, so their paired key-up passes through as well.
    pub(super) phys_pin: [Option<PinnedRoute>; 256],
    /// Live safe-refusal holds per physical key: a different physical routing
    /// to an already-held canonical is swallowed here (consumed, never
    /// dispatched) with its own down-time verdict until its paired key-up,
    /// never touching the canonical slot. Pairs can therefore neither be
    /// stolen nor orphaned, whatever the release order or mid-hold table
    /// changes; each entry clears on its own physical's key-up.
    pub(super) collide: [Option<Classified>; 256],
    pub counts: [SnapCounts; 8],
    pub resize_counts: [SnapCounts; 8],
    pub digit_counts: [SnapCounts; 10],
    pub output_counts: [SnapCounts; 8],
    pub max_counts: SnapCounts,
    pub fullscreen_counts: SnapCounts,
    pub float_counts: SnapCounts,
    pub sticky_counts: SnapCounts,
    pub orientation_counts: SnapCounts,
    pub workspace_tiling_counts: SnapCounts,
    pub launch_browser_counts: SnapCounts,
    pub launch_terminal_counts: SnapCounts,
    pub(super) hist_down: [bool; 9],
    pub(super) hist_origin: [bool; 9],
    pub(super) hist_op: [Option<WorkspaceHistoryOp>; 9],
    pub hist_counts: [SnapCounts; 9],
    pub migrate_counts: [SnapCounts; 4],
    pub(super) mask_pending: bool,
    pub(super) mask_trigger: Option<MaskTrigger>,
    pub(super) hold_masked: bool,
    pub mask_attempted: u32,
    pub mask_ok: u32,
    pub mask_failed: u32,
    pub mask_skipped: u32,
}

/// G-key arm selector: float (Win+G, KDE Meta+G) or sticky (Win+Shift+G,
/// KDE Meta+Shift+G). The op fixes at down time; repeats ride the armed hold
/// regardless of later Shift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GArm {
    Float,
    Sticky,
}
