//! Product keyboard takeover: Win+H/J/K/L and Win+arrows for focus/move,
//! plus Win+Alt(+Shift) chords for keyboard resize, plus Win+Ctrl+Alt chords
//! for explicit directional output send (item 5: follow by default, stay
//! bindable unbound).
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

mod chords;
mod classifier;
mod diag;
mod intents;
mod push_command;
mod push_select;
mod queue;
#[cfg(windows)]
pub mod sys;

pub(crate) use chords::catalog_index;
pub use chords::{
    ChordAction, ChordDisable, ChordRemap, INTENT_QUEUE_CAP, KeyboardConfig, MAX_DISPATCH_PER_TICK,
    ResizeIntent, ResizeMode, SHORTCUT_PROOF_MARKER, SnapEdge, SnapIntent, SnapOp, VK_0, VK_9,
    VK_B, VK_CONTROL, VK_DOWN, VK_ESCAPE, VK_F11, VK_G, VK_H, VK_J, VK_K, VK_L, VK_LCONTROL,
    VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_M, VK_MASK, VK_MENU, VK_O, VK_RCONTROL, VK_RIGHT,
    VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_T, VK_TAB, VK_UP, VK_Y, accept_proof_injected,
    direction_name, history_index, history_op_for_vk, is_chord_vk, is_digit_vk, is_float_vk,
    is_fullscreen_vk, is_history_vk, is_launch_browser_vk, is_launch_terminal_vk, is_maximize_vk,
    is_orientation_vk, is_sticky_vk, is_win_vk, is_workspace_tiling_vk, migrate_index,
    relative_send_delta, relative_send_index,
};
pub use diag::{
    CALLBACK_DIAG_CAP, CallbackDiag, CallbackDiagBuf, CallbackReason, CallbackSource,
    MARKED_DIAG_CAP, MOD_DIAG_CAP, MarkedDiagBuf, MarkedKeyDiag, ModDiagBuf, ModSource,
    ModTrafficDiag, callback_diag_allows, callback_diag_evidence, callback_win_untracked,
    is_callback_diag_candidate, is_proof_mod_vk, marked_diag_evidence, mod_diag_evidence,
};
pub use intents::{
    Classified, FloatIntent, FullscreenIntent, LaunchIntent, LaunchKind, MaskTrigger,
    MaximizeIntent, OrientationIntent, SnapClassify, SnapCounts, StickyIntent,
    WorkspaceHistoryIntent, WorkspaceHistoryOp, WorkspaceIntent, WorkspaceMigrateIntent,
    WorkspaceOp, WorkspaceOutputIntent, WorkspaceSendIntent, WorkspaceTilingIntent,
};
pub use queue::{
    OriginVerdict, QueuedFloatIntent, QueuedFullscreenIntent, QueuedIntent, QueuedLaunchIntent,
    QueuedMask, QueuedMaximizeIntent, QueuedOrientationIntent, QueuedResizeIntent, QueuedSnapEvent,
    QueuedStickyIntent, QueuedWorkspaceHistoryIntent, QueuedWorkspaceIntent,
    QueuedWorkspaceMigrateIntent, QueuedWorkspaceOutputIntent, QueuedWorkspaceSendIntent,
    QueuedWorkspaceTilingIntent, SnapOrigin, SnapQueue, classify_and_queue, resolve_origin,
    stamp_mask_result, win_up_mask_reserve,
};

#[cfg(test)]
#[path = "tests_shift_sync.rs"]
mod shift_sync_tests;

#[cfg(test)]
#[path = "tests_windrag_mask.rs"]
mod windrag_mask_tests;

#[cfg(test)]
#[path = "tests_history.rs"]
mod history_tests;
