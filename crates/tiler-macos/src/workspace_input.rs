#![forbid(unsafe_code)]

//! Workspace input catalogue: numbered, relative, previous, and output
//! chords over the shared workspace semantics.
//!
//! Shared semantics (read-only reference, mirrored chord-for-chord):
//! - Windows `binding_catalog` (`crates/tiler-windows/src/settings/catalog.rs`):
//!   numbered select `Win+1..0`, numbered send `Win+Shift+1..0`, numbered
//!   send-and-stay bindable unbound (`stay-workspace-N`), relative select
//!   `Win+Ctrl+H/K/Left/Up` (previous) / `Win+Ctrl+J/L/Down/Right` (next),
//!   relative send-and-follow `Win+Ctrl+Shift+...`, relative send-and-stay
//!   bindable unbound, previous-view toggle `Win+Ctrl+Tab`, explicit output
//!   send-and-follow `Win+Ctrl+Alt+...`, whole-workspace migration bindable
//!   unbound;
//! - KDE `workspaceShortcutCatalog` (`kwin/src/workspace-native.ts`):
//!   select `Meta+1..0`, move `Meta+Shift+1..0` (plus shifted-symbol
//!   aliases), previous toggle `Meta+Ctrl+Tab`, relative
//!   `Meta+Ctrl+H/...`, relative send `Meta+Ctrl+Shift+...`, stay rows
//!   bindable unbound;
//! - core trailing policy ([`tiler_core::workspace::plan_trailing`]):
//!   `index` 0 means the trailing empty workspace — select focuses or
//!   creates it, send appends for the mover (Windows `workspace-select-0` /
//!   `workspace-send-0` and KDE index-0 parity).
//!
//! Modifier form on macOS: numbered rows keep the plain Meta (`Ctrl+1..0`
//! select, `Ctrl+Shift+1..0` send-and-follow under the default Ctrl role),
//! matching the native Switch to Desktop N default chords that
//! [`crate::desktop_shortcuts`] owns under Authentic. Auxiliary rows
//! (relative select/send, previous toggle, output send) carry the
//! auxiliary arm ([`crate::input_policy::aux_flag_for`]: Command under the
//! Ctrl role, else Control) — `Ctrl+Cmd+Tab` previous, `Ctrl+Cmd+H/K/Left/Up`
//! previous, `Ctrl+Cmd+J/L/Down/Right` next, `+Shift` sends-and-follows,
//! `+Alt` output sends-and-follows — so they stay disjoint from every basic
//! chord under every Meta. Send-and-stay rows (numbered and relative) are
//! bindable but unbound by default (no chord), exactly like Windows/KDE.
//! Whole-workspace migration rows are out of scope here (no approved form).
//!
//! Conflict standing (static sources only, no live queries):
//! - repo-established holders are Mission Control `Ctrl+arrows` and the
//!   native Switch to Desktop N default `Ctrl+digit` chords (see
//!   [`crate::input_policy::conflict_reason_for`]); Compatible leaves those
//!   unshifted Ctrl chords to macOS and keeps everything else, inventing no
//!   replacements;
//! - the Apple-owned inventory (`docs/research/macos-port/plan.md`:
//!   Cmd-Space, Ctrl-Up/Down, Cmd-Tab, Ctrl-Cmd-Q, Fn-Control tiling,
//!   Mission Control/Spaces/System remappables) plus the known-taken
//!   `Ctrl+Cmd+F/Q/Space/D` keys sit outside the workspace set by
//!   construction: the previous toggle is `Meta+Aux+Tab` (never Cmd-Tab or
//!   bare Ctrl-Tab), and no row uses F, Q, Space, or D. A `None` conflict
//!   answer means "no established holder", never "proven conflict-free";
//! - under a Cmd Meta the `Cmd+digit` rows have no repo-established holder
//!   (kept under both presets): common-app digit chords (browser tabs and
//!   friends) are real but outside the repo-established set, so they are
//!   reported here, never silently disabled under Compatible.
//!
//! Execution capability: selects plus numbered sends 1..9 (follow and
//! stay) plus the trailing `index 0` select-or-create and send-0 follow.
//! The 19
//! select rows (numbered existing-Space `1..9`, the trailing select-0, the
//! 8 relative-select
//! rows, and the previous toggle) carry `implemented: true` and dispatch
//! through the product tile without a focused-window subject (empty Spaces
//! navigate). The 10 numbered send-follow rows (`workspace-send-0..9`)
//! plus the 9 numbered send-stay rows (`stay-workspace-1..9`, bindable but
//! unbound by default: no default chord) carry `implemented: true` and
//! dispatch through the managed-send effect owner with a focused
//! managed-window subject. The trailing `index 0` select-or-create row and
//! the trailing send-0 follow row arm through the single reviewable
//! [`TRAILING_CAPABILITY_ENABLED`] gate (`true`: armed end to end through
//! the production admission/drain/command/indicator/lifecycle route).
//! Everything else stays `implemented: false` and is
//! honestly refused by the engine (see [`workspace_unsupported_reason`]):
//! the trailing stay-0 (mapping only, no native wiring), all
//! relative sends/stays, all
//! output sends (cross-display TBD), and whole-workspace migration (no
//! approved form). Arming seam for the armed subset: native single-shot
//! Space switch via the proven
//! [`crate::mc_lifecycle::goto_sid_verified`] MC AX route, reused through
//! [`crate::workspace_select`] (never duplicated); the engine enqueues one
//! pending select without blocking and the main-thread tile controller
//! drains it with fresh topology/readback, advancing per-display previous
//! history only on verified arrival (stale/drift fails narrow without
//! replay, concurrent presses serialize/refuse).
//!
//! Numbered sends (`WorkspaceSend` 1-9 with either `follow`) are named
//! by [`is_armed_send`] and dispatched through the production
//! [`crate::workspace_send`] seam (engine admission, dedicated effect
//! owner over injectable native readers, holder-ownership helpers): the
//! catalogue marks follow rows 1..9 plus stay rows 1..9 `implemented:
//! true`, the engine admits them, and the product tap table carries the
//! follow rows' `Ctrl+Shift+digit` chords (stay rows stay bindable
//! unbound: no default chord, rebind injects the stay action).
//! [`is_armed_send`] is consulted for dispatch, tap categorization, and
//! feedback classification.

use crate::config::ProbeModifier;
use crate::input_policy::{KEY_DOWN, KEY_H, KEY_J, KEY_K, KEY_L, KEY_LEFT, KEY_RIGHT, KEY_UP};
use crate::runtime_types::{InputDirection, InputSemanticAction, TileBinding};

/// Mac virtual keycode for Tab (`CGKeyCode` space, `kVK_Tab` in `Events.h`):
/// the previous-toggle leg.
pub const KEY_TAB: u32 = 0x30;

/// One workspace default chord: exact key plus the Shift, Alt, and
/// auxiliary arms (see [`crate::runtime_types::TileBinding`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceChord {
    pub key_code: u32,
    pub shift: bool,
    pub alt: bool,
    pub aux: bool,
}

/// One workspace catalogue row: the stable binding id, its text, the
/// semantic action, the default chord (`None` = bindable but unbound by
/// default: the send-and-stay rows), and the product execution capability
/// (`implemented` — see the module docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRow {
    pub id: &'static str,
    pub text: &'static str,
    pub action: InputSemanticAction,
    pub default: Option<WorkspaceChord>,
    /// Canonical key slot for bindable-unbound rows (the follow arm's key
    /// on the same physical chord: numbered stay shares the digit, relative
    /// stay shares the direction key). `None` for rows with defaults. A
    /// future rebind resolves through this slot with the stay action.
    pub unbound_key: Option<u32>,
    pub implemented: bool,
}

impl WorkspaceRow {
    /// The armed binding for rows with a default chord (`None` for the
    /// bindable-unbound stay rows).
    #[must_use]
    pub fn binding(&self) -> Option<TileBinding> {
        self.default.map(|chord| TileBinding {
            key_code: chord.key_code,
            shift: chord.shift,
            alt: chord.alt,
            aux: chord.aux,
            action: self.action.clone(),
        })
    }
}

/// Relative-select row shapes in key order: full id plus previous polarity.
/// H/K/Left/Up step previous, J/L/Down/Right step next (Windows/KDE parity).
const REL_SELECT_ROWS: [(&str, u32, bool); 8] = [
    ("workspace-prev-h", KEY_H, true),
    ("workspace-prev-k", KEY_K, true),
    ("workspace-prev-left-arrow", KEY_LEFT, true),
    ("workspace-prev-up-arrow", KEY_UP, true),
    ("workspace-next-j", KEY_J, false),
    ("workspace-next-l", KEY_L, false),
    ("workspace-next-down-arrow", KEY_DOWN, false),
    ("workspace-next-right-arrow", KEY_RIGHT, false),
];

/// Relative send-and-follow row shapes (same keys, shifted arm).
const REL_SEND_ROWS: [(&str, u32, bool); 8] = [
    ("send-prev-h", KEY_H, true),
    ("send-prev-k", KEY_K, true),
    ("send-prev-left-arrow", KEY_LEFT, true),
    ("send-prev-up-arrow", KEY_UP, true),
    ("send-next-j", KEY_J, false),
    ("send-next-l", KEY_L, false),
    ("send-next-down-arrow", KEY_DOWN, false),
    ("send-next-right-arrow", KEY_RIGHT, false),
];

/// Relative send-and-stay row shapes (bindable, unbound by default).
const REL_STAY_ROWS: [(&str, u32, bool); 8] = [
    ("send-stay-prev-h", KEY_H, true),
    ("send-stay-prev-k", KEY_K, true),
    ("send-stay-prev-left-arrow", KEY_LEFT, true),
    ("send-stay-prev-up-arrow", KEY_UP, true),
    ("send-stay-next-j", KEY_J, false),
    ("send-stay-next-l", KEY_L, false),
    ("send-stay-next-down-arrow", KEY_DOWN, false),
    ("send-stay-next-right-arrow", KEY_RIGHT, false),
];

/// Output-send rows: id, key, and cross-output direction (Windows
/// `send-output-*` parity: letters plus separate arrow rows).
const OUTPUT_ROWS: [(&str, u32, InputDirection); 8] = [
    ("send-output-left", KEY_H, InputDirection::Left),
    ("send-output-left-arrow", KEY_LEFT, InputDirection::Left),
    ("send-output-down", KEY_J, InputDirection::Down),
    ("send-output-down-arrow", KEY_DOWN, InputDirection::Down),
    ("send-output-up", KEY_K, InputDirection::Up),
    ("send-output-up-arrow", KEY_UP, InputDirection::Up),
    ("send-output-right", KEY_L, InputDirection::Right),
    ("send-output-right-arrow", KEY_RIGHT, InputDirection::Right),
];

fn select_id(index: u8) -> Option<&'static str> {
    match index {
        1 => Some("workspace-select-1"),
        2 => Some("workspace-select-2"),
        3 => Some("workspace-select-3"),
        4 => Some("workspace-select-4"),
        5 => Some("workspace-select-5"),
        6 => Some("workspace-select-6"),
        7 => Some("workspace-select-7"),
        8 => Some("workspace-select-8"),
        9 => Some("workspace-select-9"),
        0 => Some("workspace-select-0"),
        _ => None,
    }
}

fn select_text(index: u8) -> Option<&'static str> {
    match index {
        1 => Some("Focus workspace 1"),
        2 => Some("Focus workspace 2"),
        3 => Some("Focus workspace 3"),
        4 => Some("Focus workspace 4"),
        5 => Some("Focus workspace 5"),
        6 => Some("Focus workspace 6"),
        7 => Some("Focus workspace 7"),
        8 => Some("Focus workspace 8"),
        9 => Some("Focus workspace 9"),
        0 => Some("Focus or create the trailing empty workspace"),
        _ => None,
    }
}

fn send_id(index: u8) -> Option<&'static str> {
    match index {
        1 => Some("workspace-send-1"),
        2 => Some("workspace-send-2"),
        3 => Some("workspace-send-3"),
        4 => Some("workspace-send-4"),
        5 => Some("workspace-send-5"),
        6 => Some("workspace-send-6"),
        7 => Some("workspace-send-7"),
        8 => Some("workspace-send-8"),
        9 => Some("workspace-send-9"),
        0 => Some("workspace-send-0"),
        _ => None,
    }
}

fn send_text(index: u8) -> Option<&'static str> {
    match index {
        1 => Some("Move window to workspace 1"),
        2 => Some("Move window to workspace 2"),
        3 => Some("Move window to workspace 3"),
        4 => Some("Move window to workspace 4"),
        5 => Some("Move window to workspace 5"),
        6 => Some("Move window to workspace 6"),
        7 => Some("Move window to workspace 7"),
        8 => Some("Move window to workspace 8"),
        9 => Some("Move window to workspace 9"),
        0 => Some("Move window to a newly appended workspace"),
        _ => None,
    }
}

fn stay_id(index: u8) -> Option<&'static str> {
    match index {
        1 => Some("stay-workspace-1"),
        2 => Some("stay-workspace-2"),
        3 => Some("stay-workspace-3"),
        4 => Some("stay-workspace-4"),
        5 => Some("stay-workspace-5"),
        6 => Some("stay-workspace-6"),
        7 => Some("stay-workspace-7"),
        8 => Some("stay-workspace-8"),
        9 => Some("stay-workspace-9"),
        0 => Some("stay-workspace-0"),
        _ => None,
    }
}

fn stay_text(index: u8) -> Option<&'static str> {
    match index {
        1 => Some("Move window to workspace 1 without following"),
        2 => Some("Move window to workspace 2 without following"),
        3 => Some("Move window to workspace 3 without following"),
        4 => Some("Move window to workspace 4 without following"),
        5 => Some("Move window to workspace 5 without following"),
        6 => Some("Move window to workspace 6 without following"),
        7 => Some("Move window to workspace 7 without following"),
        8 => Some("Move window to workspace 8 without following"),
        9 => Some("Move window to workspace 9 without following"),
        0 => Some("Move window to a newly appended workspace without following"),
        _ => None,
    }
}

/// Digit keycode for a numbered index (1-9 map to `1`-`9`, 0 maps to `0`;
/// the native Desktop-N legs, single home in
/// [`crate::desktop_shortcuts`]).
fn digit_keycode(index: u8) -> Option<u32> {
    match index {
        0 => crate::desktop_shortcuts::desktop_digit_keycode(10),
        1..=9 => crate::desktop_shortcuts::desktop_digit_keycode(u32::from(index)),
        _ => None,
    }
}

/// The full workspace catalogue: 10 numbered selects, 10 numbered
/// send-and-follows, 10 numbered send-and-stays (bindable unbound), 8
/// relative selects, 8 relative send-and-follows, 8 relative send-and-stays
/// (bindable unbound), 1 previous toggle, and 8 output send-and-follows.
/// Deterministic order; every row carries its parseable id.
#[must_use]
pub fn workspace_catalogue() -> Vec<WorkspaceRow> {
    let mut rows = Vec::with_capacity(63);
    for index in [1, 2, 3, 4, 5, 6, 7, 8, 9, 0] {
        let key_code = digit_keycode(index).expect("numbered index has a digit leg");
        // Select-only arming: existing Spaces 1-9 dispatch, and the
        // trailing index-0 select-or-create row dispatches through
        // [`TRAILING_CAPABILITY_ENABLED`] (armed: `is_armed_select`
        // admits it).
        rows.push(WorkspaceRow {
            id: select_id(index).expect("numbered index has a select id"),
            text: select_text(index).expect("numbered index has a select text"),
            action: InputSemanticAction::WorkspaceSelect { index },
            default: Some(WorkspaceChord {
                key_code,
                shift: false,
                alt: false,
                aux: false,
            }),
            unbound_key: None,
            implemented: true,
        });
        rows.push(WorkspaceRow {
            id: send_id(index).expect("numbered index has a send id"),
            text: send_text(index).expect("numbered index has a send text"),
            action: InputSemanticAction::WorkspaceSend {
                index,
                follow: true,
            },
            default: Some(WorkspaceChord {
                key_code,
                shift: true,
                alt: false,
                aux: false,
            }),
            unbound_key: None,
            // Armed: numbered send-follow 0..9 dispatches through the
            // managed-send effect owner; the trailing send-0 arms through
            // [`TRAILING_CAPABILITY_ENABLED`] and refuses engine-side
            // while unarmed.
            implemented: true,
        });
        rows.push(WorkspaceRow {
            id: stay_id(index).expect("numbered index has a stay id"),
            text: stay_text(index).expect("numbered index has a stay text"),
            action: InputSemanticAction::WorkspaceSend {
                index,
                follow: false,
            },
            default: None,
            unbound_key: Some(key_code),
            // Armed: numbered stay 1..9 dispatches through the managed-send
            // effect owner (bindable unbound: no default chord, rebind
            // injects the stay action); trailing stay-0 stays unarmed
            // (no trailing append) and refuses engine-side.
            implemented: index != 0,
        });
    }
    for (id, key_code, prev) in REL_SELECT_ROWS {
        rows.push(WorkspaceRow {
            id,
            text: if prev {
                "Previous workspace"
            } else {
                "Next workspace"
            },
            action: InputSemanticAction::WorkspaceRelative { prev },
            default: Some(WorkspaceChord {
                key_code,
                shift: false,
                alt: false,
                aux: true,
            }),
            unbound_key: None,
            // Armed: relative select dispatches with no focused-window
            // subject (empty Spaces navigate).
            implemented: true,
        });
    }
    for (id, key_code, prev) in REL_SEND_ROWS {
        rows.push(WorkspaceRow {
            id,
            text: if prev {
                "Move window to previous workspace"
            } else {
                "Move window to next workspace"
            },
            action: InputSemanticAction::WorkspaceSendRelative { prev, follow: true },
            default: Some(WorkspaceChord {
                key_code,
                shift: true,
                alt: false,
                aux: true,
            }),
            unbound_key: None,
            implemented: false,
        });
    }
    for (id, key_code, prev) in REL_STAY_ROWS {
        rows.push(WorkspaceRow {
            id,
            text: if prev {
                "Move window to previous workspace without following"
            } else {
                "Move window to next workspace without following"
            },
            action: InputSemanticAction::WorkspaceSendRelative {
                prev,
                follow: false,
            },
            default: None,
            unbound_key: Some(key_code),
            implemented: false,
        });
    }
    rows.push(WorkspaceRow {
        id: "workspace-previous",
        text: "Toggle previous workspace",
        action: InputSemanticAction::WorkspacePrevious,
        default: Some(WorkspaceChord {
            key_code: KEY_TAB,
            shift: false,
            alt: false,
            aux: true,
        }),
        unbound_key: None,
        // Armed: the two-view toggle dispatches with no focused-window
        // subject; history advances only on verified arrival.
        implemented: true,
    });
    for (id, key_code, direction) in OUTPUT_ROWS {
        rows.push(WorkspaceRow {
            id,
            text: "Send window to output",
            action: InputSemanticAction::WorkspaceOutput {
                direction,
                follow: true,
            },
            default: Some(WorkspaceChord {
                key_code,
                shift: false,
                alt: true,
                aux: true,
            }),
            unbound_key: None,
            implemented: false,
        });
    }
    rows
}

/// Default-chord bindings for the rows with defaults (45 rows): numbered
/// selects/sends, relative selects/sends, the previous toggle, and the
/// output sends. Stay rows contribute nothing (bindable, unbound).
/// Defaulted is not armed: the product tile arms the basic catalogue plus
/// the implemented select subset ([`crate::workspace_select::bindings`],
/// 19 rows) plus the implemented numbered send subset
/// ([`crate::workspace_send::bindings`], 10 follow chords; stay rows stay
/// bindable unbound) via
/// [`crate::input_policy::effective_bindings`]; every other defaulted row
/// (relative sends, outputs)
/// stays refused engine-side (see [`workspace_unsupported_reason`]).
#[must_use]
pub fn workspace_bindings() -> Vec<TileBinding> {
    workspace_catalogue()
        .iter()
        .filter_map(|row| row.binding())
        .collect()
}

/// Parse one workspace binding id to its semantic action (the single source
/// of truth is the catalogue itself: every row id parses, nothing else
/// does). Used for config/rebind resolution.
#[must_use]
pub fn parse_workspace_action(id: &str) -> Option<InputSemanticAction> {
    workspace_catalogue()
        .iter()
        .find(|row| row.id == id)
        .map(|row| row.action.clone())
}

/// The canonical binding id for one semantic action (`None` for basic
/// actions and for output send-and-stay, which has no approved id yet).
/// Actions shared by a letter row plus its arrow alias resolve to the
/// canonical letter slot (Windows parity); every id still parses forward
/// through [`parse_workspace_action`].
#[must_use]
pub fn workspace_action_id(action: &InputSemanticAction) -> Option<&'static str> {
    workspace_catalogue()
        .iter()
        .find(|row| row.action == *action)
        .map(|row| row.id)
}

/// True only for the armed select-only subset: numbered existing-Space
/// selects `1..9`, both relative selects, and the previous toggle. The
/// trailing `index 0` select-or-create, every send/stay/output action, and
/// every basic action are NOT armed selects (index 0 and transfers refuse
/// with [`workspace_unsupported_reason`]; basics execute elsewhere).
/// Single home for what the engine dispatches without a focused-window
/// subject and what [`crate::workspace_select::bindings`] arms.
#[must_use]
pub const fn is_armed_select(action: &InputSemanticAction) -> bool {
    is_armed_select_with(action, TRAILING_CAPABILITY_ENABLED)
}

/// Capability-explicit armed-select predicate: the production route with
/// `trailing_enabled` threaded from [`TRAILING_CAPABILITY_ENABLED`]
/// (tests pass an explicit local). Trailing `index 0` arms only when the
/// capability is enabled.
#[must_use]
pub const fn is_armed_select_with(action: &InputSemanticAction, trailing_enabled: bool) -> bool {
    match *action {
        InputSemanticAction::WorkspaceSelect { index } => {
            (index >= 1 && index <= 9) || (index == 0 && trailing_enabled)
        }
        InputSemanticAction::WorkspaceRelative { .. } | InputSemanticAction::WorkspacePrevious => {
            true
        }
        _ => false,
    }
}

/// Send-shape predicate for the numbered send product path:
/// `WorkspaceSend` with `index` 0-9 follow plus `index` 1-9 stay:
/// follows ride the
/// `Ctrl+Shift+1..9` shape under the default Ctrl role, stays ride a
/// future rebind with no default chord). This names
/// the action shape the production [`crate::workspace_send`] machinery
/// dispatches, the tap categorizes, and the engine admits: the catalogue
/// marks follow rows 0..9 plus stay rows 1..9 `implemented: true` and
/// [`workspace_unsupported_reason`] / [`workspace_unsupported_outcome`]
/// consult this predicate (returning `None` here, dispatching). Consumers
/// are the send lane, the discard/kind classifier
/// ([`crate::workspace_send::feedback_kind_for`]), and the tests.
/// Everything else - the trailing `index 0` stay, every relative send,
/// every output send, every
/// select, and every basic action — is NOT send-shaped.
#[must_use]
pub const fn is_armed_send(action: &InputSemanticAction) -> bool {
    is_armed_send_with(action, TRAILING_CAPABILITY_ENABLED)
}

/// Capability-explicit send-shape predicate: numbered `1..9` always armed;
/// trailing `index 0` follow arms only when the capability is enabled
/// (stay-0 never arms: out of scope, no approved form).
#[must_use]
pub const fn is_armed_send_with(action: &InputSemanticAction, trailing_enabled: bool) -> bool {
    match *action {
        InputSemanticAction::WorkspaceSend { index, follow } => {
            (index >= 1 && index <= 9) || (index == 0 && follow && trailing_enabled)
        }
        _ => false,
    }
}

/// Single reviewable production capability switch for trailing `index 0`.
/// `true` arms the production route end to end (engine admission, tap
/// catalogue, command map, drain, and indicator submissions all dispatch
/// trailing selects/sends), `false` keeps every product route unarmed.
/// Armed only after the occupancy, lifecycle-removal, and
/// indicator-acceptance reviews land; tests thread an explicit local
/// `trailing_enabled` argument through the same production functions
/// instead of any process-global flag.
pub const TRAILING_CAPABILITY_ENABLED: bool = true;

/// Honest engine refusal reason for one semantic action (`None` for the
/// basic actions, which execute, for the armed select subset, which
/// dispatches, and for the armed numbered sends 1..9 (follow plus stay),
/// which
/// dispatches through the managed-send effect owner). Every other
/// workspace action is recognized but undispatched; the reason names the
/// exact missing execution seam rather than faking support.
#[must_use]
pub fn workspace_unsupported_reason(action: &InputSemanticAction) -> Option<&'static str> {
    workspace_unsupported_reason_with(action, TRAILING_CAPABILITY_ENABLED)
}

/// Capability-explicit refusal reason: trailing `index 0` rows report `None`
/// (dispatched) only when the capability is enabled.
#[must_use]
pub fn workspace_unsupported_reason_with(
    action: &InputSemanticAction,
    trailing_enabled: bool,
) -> Option<&'static str> {
    if is_armed_select_with(action, trailing_enabled)
        || is_armed_send_with(action, trailing_enabled)
    {
        return None;
    }
    match action {
        InputSemanticAction::FocusStep { .. }
        | InputSemanticAction::MoveStep { .. }
        | InputSemanticAction::ResizeStep { .. }
        | InputSemanticAction::ToggleFloat
        | InputSemanticAction::ToggleOrientation
        | InputSemanticAction::DragDrop { .. } => None,
        InputSemanticAction::WorkspaceSelect { .. }
        | InputSemanticAction::WorkspaceRelative { .. }
        | InputSemanticAction::WorkspacePrevious => Some(
            "trailing select-or-create unarmed: automatic trailing Space \
             creation/deletion is unresolved (occupancy unproven — zero-area \
             candidates are unknown, never an append trigger), so no trailing \
             append runs from the product tile; the explicit spaces-select \
             batch retains its own trailing diagnostics",
        ),
        InputSemanticAction::WorkspaceSend { .. }
        | InputSemanticAction::WorkspaceSendRelative { .. } => Some(
            "native window Space move is experimental direct-SLS only with an explicit \
             --allow-untested prerequisite (observed no-effect on one owned fixture; \
             single-shot inverse unverified) and never runs from the product tile",
        ),
        InputSemanticAction::WorkspaceOutput { .. } => Some(
            "explicit cross-display transfer TBD (the Tier-2 move path gates same-display \
             only and refuses cross-display as Blocked); no product output-send execution",
        ),
    }
}

/// Static log outcome token for one refused workspace action (`None` for
/// basic actions, the armed select subset, and the armed numbered
/// sends 1..9 (follow plus stay), all of which dispatch).
/// The engine counts under `input_refused_unsupported` and emits this
/// token; the seam detail rides [`workspace_unsupported_reason`]
/// (docs/tests, never guessed at runtime).
#[must_use]
pub fn workspace_unsupported_outcome(action: &InputSemanticAction) -> Option<&'static str> {
    workspace_unsupported_outcome_with(action, TRAILING_CAPABILITY_ENABLED)
}

/// Capability-explicit outcome token (same rule as the reason above).
#[must_use]
pub fn workspace_unsupported_outcome_with(
    action: &InputSemanticAction,
    trailing_enabled: bool,
) -> Option<&'static str> {
    if is_armed_select_with(action, trailing_enabled)
        || is_armed_send_with(action, trailing_enabled)
    {
        return None;
    }
    match action {
        InputSemanticAction::FocusStep { .. }
        | InputSemanticAction::MoveStep { .. }
        | InputSemanticAction::ResizeStep { .. }
        | InputSemanticAction::ToggleFloat
        | InputSemanticAction::ToggleOrientation
        | InputSemanticAction::DragDrop { .. } => None,
        InputSemanticAction::WorkspaceSelect { .. }
        | InputSemanticAction::WorkspaceRelative { .. }
        | InputSemanticAction::WorkspacePrevious => Some("refused-workspace-switch-unsupported"),
        InputSemanticAction::WorkspaceSend { .. }
        | InputSemanticAction::WorkspaceSendRelative { .. } => {
            Some("refused-workspace-send-unsupported")
        }
        InputSemanticAction::WorkspaceOutput { .. } => Some("refused-workspace-output-unsupported"),
    }
}

/// Effective modifier flags for one workspace binding under one Ctrl-role
/// modifier: the role bit plus the Shift, Alt, and auxiliary arms. Single
/// home for what the tap tunes, the validator dedups, and the tests pin —
/// every consumer must agree exactly.
#[must_use]
pub fn effective_flags_for(modifier: ProbeModifier, binding: &TileBinding) -> u64 {
    let (mut want, _) = crate::input::cg_flag_bits(modifier);
    if binding.shift {
        want |= 0x0002_0000;
    }
    if binding.alt {
        want |= 0x0008_0000;
    }
    if binding.aux {
        want |= crate::input_policy::aux_flag_for(modifier);
    }
    want
}
