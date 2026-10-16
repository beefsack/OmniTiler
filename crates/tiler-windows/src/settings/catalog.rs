use super::*;

/// Which runtime arm a catalog action belongs to. Directional/workspace arms
/// split on Shift; history arms split on Ctrl (item 1); toggles carry their
/// fixed polarity; resize arms split on Shift with Alt always held (outwards
/// unshifted Win+Alt, inwards Win+Shift+Alt).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingFamily {
    Directional {
        direction: &'static str,
        op: SnapFamilyOp,
    },
    Workspace {
        index: u8,
        op: SnapFamilyOp,
    },
    /// Item 2 numbered send-and-stay: bindable, unbound by default (empty
    /// defaults). Keep means unbound, never Disabled/effective interception;
    /// rebinds ride the shifted digit arm through the stay action.
    WorkspaceStay {
        index: u8,
    },
    /// Item 2 relative send: previous/next ordinal step in the item 1 scoped
    /// ring (never MRU), resolved once before transfer. Follow rows carry
    /// Win+Ctrl+Shift defaults; stay rows are bindable unbound. `prev` picks
    /// the step direction; `follow` pins the explicit intent.
    WorkspaceSendRelative {
        prev: bool,
        follow: bool,
    },
    /// Item 5 explicit directional output send: transfer to the adjacent
    /// output's current workspace in `direction`, selected by shared
    /// FULL-rectangle centre/span/left-top ranking. Follow rows carry
    /// Win+Ctrl+Alt defaults; stay rows are bindable unbound. `follow` pins
    /// the explicit intent.
    WorkspaceOutput {
        direction: &'static str,
        follow: bool,
    },
    /// Item 14 whole-active-workspace output migration: move the active
    /// workspace to the adjacent output in `direction`, selected by the
    /// shared largest-shared-edge rule. All four rows are bindable unbound
    /// (Keep means unbound, never Disabled/effective interception) and
    /// follow-only: a rebound rides Win+Ctrl+Alt+Shift through the migrate
    /// action, pinning `direction` from the canonical letter key.
    WorkspaceMigrate {
        direction: &'static str,
    },
    WorkspaceHistory {
        kind: WorkspaceHistoryKind,
    },
    Toggle {
        kind: ToggleKind,
    },
    Launch {
        kind: LaunchKind,
    },
    Resize {
        direction: &'static str,
        mode: &'static str,
    },
}

/// Item 1 history arm: `Previous` toggles the previous view (Win+Ctrl+Tab),
/// `Prev` steps to the previous ordinal, `Next` steps to the next ordinal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceHistoryKind {
    Previous,
    Prev,
    Next,
}

impl WorkspaceHistoryKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Previous => "previous",
            Self::Prev => "prev",
            Self::Next => "next",
        }
    }
}

/// Focus/move/select/send arm selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapFamilyOp {
    Primary,
    Shifted,
}

impl SnapFamilyOp {
    /// Native Shift polarity of this arm (unshifted focus/select vs shifted
    /// move/send). Rebinds must match it so the classifier's Shift-derived op
    /// keeps meaning what the binding says.
    #[must_use]
    pub const fn wants_shift(self) -> bool {
        match self {
            Self::Primary => false,
            Self::Shifted => true,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Shifted => "shifted",
        }
    }
}

/// Toggle arm selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleKind {
    Float,
    Sticky,
    Maximize,
    Fullscreen,
    Orientation,
    WorkspaceTiling,
}

impl ToggleKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Float => "float",
            Self::Sticky => "sticky",
            Self::Maximize => "maximize",
            Self::Fullscreen => "fullscreen",
            Self::Orientation => "orientation",
            Self::WorkspaceTiling => "workspace-tiling",
        }
    }

    /// Native Shift polarity: only sticky rides the shifted G arm.
    #[must_use]
    pub const fn wants_shift(self) -> bool {
        match self {
            Self::Sticky => true,
            Self::Float
            | Self::Maximize
            | Self::Fullscreen
            | Self::Orientation
            | Self::WorkspaceTiling => false,
        }
    }
}

/// Launcher arm selector: default browser (Win+B) or Windows Terminal
/// (Win+T). Both ride unshifted Win chords with no Ctrl/Alt; only the down
/// dispatches, ups close the pair, held repeats stay swallowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// One catalog row: the KDE action, its default chord(s), and the honest
/// Windows conflict/effect model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BindingDef {
    pub id: &'static str,
    pub text: &'static str,
    pub family: BindingFamily,
    /// Default chord(s). Directional rows carry exactly one chord each
    /// (letters and arrows are separate rows); resize rows carry two (the
    /// letter plus its arrow alias); every other row carries exactly one.
    pub defaults: &'static [&'static str],
    /// False only for rows the Windows hook never intercepts. All catalog
    /// rows (including resize) are currently implemented.
    pub implemented: bool,
    /// Truthful per-physical-chord conflict note, if any.
    pub conflict: Option<&'static str>,
}

/// Directional focus/move rows: one row per physical chord (KDE
/// `planShortcutCatalog` parity: `focus-<dir>` on the letter plus a separate
/// `focus-<dir>-arrow` row on the arrow, same for move). Splitting keeps the
/// per-binding user model truthful: the letter and arrow aliases ride
/// different OS conflicts, so disabling or rebinding one never touches the
/// other. Shift selects the move arm.
const FOCUS_MOVE_ROWS: [(&str, &str, &str, &[&str], SnapFamilyOp); 16] = [
    (
        "focus-left",
        "Focus window left",
        "left",
        &["Win+H"],
        SnapFamilyOp::Primary,
    ),
    (
        "focus-left-arrow",
        "Focus window left",
        "left",
        &["Win+Left"],
        SnapFamilyOp::Primary,
    ),
    (
        "focus-down",
        "Focus window down",
        "down",
        &["Win+J"],
        SnapFamilyOp::Primary,
    ),
    (
        "focus-down-arrow",
        "Focus window down",
        "down",
        &["Win+Down"],
        SnapFamilyOp::Primary,
    ),
    (
        "focus-up",
        "Focus window up",
        "up",
        &["Win+K"],
        SnapFamilyOp::Primary,
    ),
    (
        "focus-up-arrow",
        "Focus window up",
        "up",
        &["Win+Up"],
        SnapFamilyOp::Primary,
    ),
    (
        "focus-right",
        "Focus window right",
        "right",
        &["Win+L"],
        SnapFamilyOp::Primary,
    ),
    (
        "focus-right-arrow",
        "Focus window right",
        "right",
        &["Win+Right"],
        SnapFamilyOp::Primary,
    ),
    (
        "move-left",
        "Move window left",
        "left",
        &["Win+Shift+H"],
        SnapFamilyOp::Shifted,
    ),
    (
        "move-left-arrow",
        "Move window left",
        "left",
        &["Win+Shift+Left"],
        SnapFamilyOp::Shifted,
    ),
    (
        "move-down",
        "Move window down",
        "down",
        &["Win+Shift+J"],
        SnapFamilyOp::Shifted,
    ),
    (
        "move-down-arrow",
        "Move window down",
        "down",
        &["Win+Shift+Down"],
        SnapFamilyOp::Shifted,
    ),
    (
        "move-up",
        "Move window up",
        "up",
        &["Win+Shift+K"],
        SnapFamilyOp::Shifted,
    ),
    (
        "move-up-arrow",
        "Move window up",
        "up",
        &["Win+Shift+Up"],
        SnapFamilyOp::Shifted,
    ),
    (
        "move-right",
        "Move window right",
        "right",
        &["Win+Shift+L"],
        SnapFamilyOp::Shifted,
    ),
    (
        "move-right-arrow",
        "Move window right",
        "right",
        &["Win+Shift+Right"],
        SnapFamilyOp::Shifted,
    ),
];

/// Resize rows: KDE `Meta+Alt+key` (grow/outwards) and
/// `Meta+Alt+Shift+key` (shrink/inwards) plus arrow aliases. Intercepted on
/// Windows through the dedicated resize classifier arm (never a move):
/// each row carries both its letter and arrow defaults (8 logical rows,
/// 16 physical chords). Windows ownership of these chords is unverified in
/// the repository, so both presets keep the defaults; containment is
/// unproven live.
const RESIZE_ROWS: [(&str, &str, &str, &str, &[&str]); 8] = [
    (
        "resize-out-left",
        "Grow window towards left",
        "left",
        "outwards",
        &["Win+Alt+H", "Win+Alt+Left"],
    ),
    (
        "resize-out-down",
        "Grow window towards down",
        "down",
        "outwards",
        &["Win+Alt+J", "Win+Alt+Down"],
    ),
    (
        "resize-out-up",
        "Grow window towards up",
        "up",
        "outwards",
        &["Win+Alt+K", "Win+Alt+Up"],
    ),
    (
        "resize-out-right",
        "Grow window towards right",
        "right",
        "outwards",
        &["Win+Alt+L", "Win+Alt+Right"],
    ),
    (
        "resize-in-left",
        "Shrink window from left",
        "left",
        "inwards",
        &["Win+Alt+Shift+H", "Win+Alt+Shift+Left"],
    ),
    (
        "resize-in-down",
        "Shrink window from down",
        "down",
        "inwards",
        &["Win+Alt+Shift+J", "Win+Alt+Shift+Down"],
    ),
    (
        "resize-in-up",
        "Shrink window from up",
        "up",
        "inwards",
        &["Win+Alt+Shift+K", "Win+Alt+Shift+Up"],
    ),
    (
        "resize-in-right",
        "Shrink window from right",
        "right",
        "inwards",
        &["Win+Alt+Shift+L", "Win+Alt+Shift+Right"],
    ),
];

/// Honest Windows conflict note for keyboard resize chords: ownership is
/// unverified in the repository and containment is unproven live. Never a
/// stock-holder or conflict-free claim.
pub const RESIZE_CONFLICT: &str =
    "Windows shortcut ownership unverified in repository; containment unproven live";

/// Honest Windows conflict note for item 5 explicit output-send chords
/// (Win+Ctrl+Alt, decision 5.3): Windows ownership is unknown and containment
/// is unproven live. Both presets keep the follow defaults; no Compatible
/// disable is selected and no stock holder is claimed.
pub const OUTPUT_SEND_CONFLICT: &str =
    "Windows shortcut ownership unknown; containment unproven live";

/// Build the full binding catalog: directional focus/move (one row per
/// physical chord: letters plus separate arrow rows), resize (8 logical
/// rows, each with its letter plus arrow defaults, intercepted through the
/// dedicated resize arm), float/sticky/maximize/fullscreen toggles, and
/// workspace select/send digits 0-9.
#[must_use]
pub fn binding_catalog() -> Vec<BindingDef> {
    let mut rows = Vec::new();
    for (id, text, direction, defaults, op) in FOCUS_MOVE_ROWS {
        rows.push(BindingDef {
            id,
            text,
            family: BindingFamily::Directional { direction, op },
            defaults,
            implemented: true,
            conflict: directional_conflict(id),
        });
    }
    for (id, text, direction, mode, defaults) in RESIZE_ROWS {
        rows.push(BindingDef {
            id,
            text,
            family: BindingFamily::Resize { direction, mode },
            defaults,
            implemented: true,
            conflict: Some(RESIZE_CONFLICT),
        });
    }
    rows.push(BindingDef {
        id: "toggle-float",
        text: "Toggle floating window",
        family: BindingFamily::Toggle {
            kind: ToggleKind::Float,
        },
        defaults: &["Win+G"],
        implemented: true,
        conflict: chord_conflict("Win+G"),
    });
    rows.push(BindingDef {
        id: "toggle-sticky",
        text: "Toggle sticky floating window",
        family: BindingFamily::Toggle {
            kind: ToggleKind::Sticky,
        },
        defaults: &["Win+Shift+G"],
        implemented: true,
        conflict: chord_conflict("Win+Shift+G"),
    });
    rows.push(BindingDef {
        id: "toggle-maximize",
        text: "Toggle maximize window",
        family: BindingFamily::Toggle {
            kind: ToggleKind::Maximize,
        },
        defaults: &["Win+M"],
        implemented: true,
        conflict: chord_conflict("Win+M"),
    });
    rows.push(BindingDef {
        id: "toggle-fullscreen",
        text: "Toggle fullscreen window",
        family: BindingFamily::Toggle {
            kind: ToggleKind::Fullscreen,
        },
        defaults: &["Win+F11"],
        implemented: true,
        conflict: chord_conflict("Win+F11"),
    });
    rows.push(BindingDef {
        id: "toggle-orientation",
        text: "Toggle parent split orientation",
        family: BindingFamily::Toggle {
            kind: ToggleKind::Orientation,
        },
        defaults: &["Win+O"],
        implemented: true,
        conflict: chord_conflict("Win+O"),
    });
    rows.push(BindingDef {
        id: "toggle-workspace-tiling",
        text: "Toggle tiling for current workspace",
        family: BindingFamily::Toggle {
            kind: ToggleKind::WorkspaceTiling,
        },
        defaults: &["Win+Y"],
        implemented: true,
        conflict: chord_conflict("Win+Y"),
    });
    rows.push(BindingDef {
        id: "launch-browser",
        text: "Open default browser",
        family: BindingFamily::Launch {
            kind: LaunchKind::Browser,
        },
        defaults: &["Win+B"],
        implemented: true,
        conflict: chord_conflict("Win+B"),
    });
    rows.push(BindingDef {
        id: "launch-terminal",
        text: "Open Windows Terminal",
        family: BindingFamily::Launch {
            kind: LaunchKind::Terminal,
        },
        defaults: &["Win+T"],
        implemented: true,
        conflict: chord_conflict("Win+T"),
    });
    for index in 1..=9u8 {
        rows.push(workspace_row(index, SnapFamilyOp::Primary));
    }
    rows.push(workspace_row(0, SnapFamilyOp::Primary));
    for index in 1..=9u8 {
        rows.push(workspace_row(index, SnapFamilyOp::Shifted));
    }
    rows.push(workspace_row(0, SnapFamilyOp::Shifted));
    // Item 2 numbered send-and-stay rows: bindable, unbound by default.
    // Keep means unbound (never Disabled/effective interception); the shifted
    // digit arm plus the explicit stay action route rebinds.
    for index in 1..=9u8 {
        rows.push(BindingDef {
            id: stay_id(index),
            text: "Move window to workspace without following",
            family: BindingFamily::WorkspaceStay { index },
            defaults: &[],
            implemented: true,
            conflict: None,
        });
    }
    rows.push(BindingDef {
        id: stay_id(0),
        text: "Move window to a newly appended workspace without following",
        family: BindingFamily::WorkspaceStay { index: 0 },
        defaults: &[],
        implemented: true,
        conflict: None,
    });
    // Item 2 relative sends: previous/next ordinal step in the item 1 scoped
    // ring. Follow rows carry Win+Ctrl+Shift defaults; stay rows are
    // bindable unbound. Windows Ctrl+Shift arrow ownership is UNKNOWN:
    // defaults keep pending evidenced conflicts with the honest
    // ownership-unknown note (no new Compatible disables).
    for (id, text, prev, follow, chord) in RELATIVE_SEND_ROWS {
        rows.push(BindingDef {
            id,
            text,
            family: BindingFamily::WorkspaceSendRelative { prev, follow },
            defaults: chord,
            implemented: true,
            conflict: if chord.is_empty() {
                None
            } else {
                chord_conflict(chord[0])
            },
        });
    }
    // Item 5 explicit output sends: eight follow rows (letters plus separate
    // arrow rows, Win+Ctrl+Alt defaults kept under both presets) plus four
    // canonical stay rows (one per direction, bindable unbound). Windows
    // Ctrl+Alt ownership is UNKNOWN (decision 5.3): the honest
    // ownership-unknown note, no new Compatible disables, no invented stock
    // holder.
    for (id, text, direction, follow, chord) in OUTPUT_SEND_ROWS {
        rows.push(BindingDef {
            id,
            text,
            family: BindingFamily::WorkspaceOutput { direction, follow },
            defaults: chord,
            implemented: true,
            conflict: if chord.is_empty() {
                None
            } else {
                chord_conflict(chord[0])
            },
        });
    }
    // Item 14 whole-workspace migration: four follow-only rows (one per
    // direction), all bindable unbound. A rebound rides Win+Ctrl+Alt+Shift
    // through the explicit migrate action; the canonical letter slot is
    // shared per direction like the output-send stay rows. Both presets keep
    // these rows unbound; no new Compatible disables and no invented holder.
    for (id, text, direction) in MIGRATION_ROWS {
        rows.push(BindingDef {
            id,
            text,
            family: BindingFamily::WorkspaceMigrate { direction },
            defaults: &[],
            implemented: true,
            conflict: None,
        });
    }
    // Item 1 history rows: one Win+Ctrl+Tab previous-view toggle plus eight
    // Win+Ctrl relative steps (H/K/Left/Up previous, J/L/Down/Right next).
    // Authentic keeps all nine; Compatible disables only Left/Right (native
    // virtual-desktop ownership, unverified in repo); Tab/letters/Up/Down
    // carry the honest ownership-unknown note.
    for (id, text, kind, chord) in HISTORY_ROWS {
        rows.push(BindingDef {
            id,
            text,
            family: BindingFamily::WorkspaceHistory { kind },
            defaults: chord,
            implemented: true,
            conflict: chord_conflict(chord[0]),
        });
    }
    rows
}

/// Item 1 history catalog rows: toggle plus four previous and four next
/// ordinals. Separate rows keep the per-binding model truthful: arrows and
/// letters ride different OS conflicts, so disabling or rebinding one never
/// touches the other.
const HISTORY_ROWS: [(&str, &str, WorkspaceHistoryKind, &[&str]); 9] = [
    (
        "workspace-previous",
        "Toggle previous workspace",
        WorkspaceHistoryKind::Previous,
        &["Win+Ctrl+Tab"],
    ),
    (
        "workspace-prev-h",
        "Previous workspace",
        WorkspaceHistoryKind::Prev,
        &["Win+Ctrl+H"],
    ),
    (
        "workspace-prev-k",
        "Previous workspace",
        WorkspaceHistoryKind::Prev,
        &["Win+Ctrl+K"],
    ),
    (
        "workspace-prev-left-arrow",
        "Previous workspace",
        WorkspaceHistoryKind::Prev,
        &["Win+Ctrl+Left"],
    ),
    (
        "workspace-prev-up-arrow",
        "Previous workspace",
        WorkspaceHistoryKind::Prev,
        &["Win+Ctrl+Up"],
    ),
    (
        "workspace-next-j",
        "Next workspace",
        WorkspaceHistoryKind::Next,
        &["Win+Ctrl+J"],
    ),
    (
        "workspace-next-l",
        "Next workspace",
        WorkspaceHistoryKind::Next,
        &["Win+Ctrl+L"],
    ),
    (
        "workspace-next-down-arrow",
        "Next workspace",
        WorkspaceHistoryKind::Next,
        &["Win+Ctrl+Down"],
    ),
    (
        "workspace-next-right-arrow",
        "Next workspace",
        WorkspaceHistoryKind::Next,
        &["Win+Ctrl+Right"],
    ),
];

/// Item 2 numbered stay ids: ten bindable unbound rows mirroring the
/// numbered follow sends.
fn stay_id(index: u8) -> &'static str {
    match index {
        1 => "stay-workspace-1",
        2 => "stay-workspace-2",
        3 => "stay-workspace-3",
        4 => "stay-workspace-4",
        5 => "stay-workspace-5",
        6 => "stay-workspace-6",
        7 => "stay-workspace-7",
        8 => "stay-workspace-8",
        9 => "stay-workspace-9",
        _ => "stay-workspace-0",
    }
}

/// Item 2 relative-send catalog rows: four previous + four next follow rows
/// with Win+Ctrl+Shift defaults, plus eight matching stay rows bindable
/// unbound. Separate rows keep the per-binding model truthful: rebinding one
/// never touches the other, and arrows/letters ride the honest
/// ownership-unknown note (no new Compatible disables).
const RELATIVE_SEND_ROWS: [(&str, &str, bool, bool, &[&str]); 16] = [
    (
        "send-prev-h",
        "Move window to previous workspace",
        true,
        true,
        &["Win+Ctrl+Shift+H"],
    ),
    (
        "send-prev-k",
        "Move window to previous workspace",
        true,
        true,
        &["Win+Ctrl+Shift+K"],
    ),
    (
        "send-prev-left-arrow",
        "Move window to previous workspace",
        true,
        true,
        &["Win+Ctrl+Shift+Left"],
    ),
    (
        "send-prev-up-arrow",
        "Move window to previous workspace",
        true,
        true,
        &["Win+Ctrl+Shift+Up"],
    ),
    (
        "send-next-j",
        "Move window to next workspace",
        false,
        true,
        &["Win+Ctrl+Shift+J"],
    ),
    (
        "send-next-l",
        "Move window to next workspace",
        false,
        true,
        &["Win+Ctrl+Shift+L"],
    ),
    (
        "send-next-down-arrow",
        "Move window to next workspace",
        false,
        true,
        &["Win+Ctrl+Shift+Down"],
    ),
    (
        "send-next-right-arrow",
        "Move window to next workspace",
        false,
        true,
        &["Win+Ctrl+Shift+Right"],
    ),
    (
        "send-stay-prev-h",
        "Move window to previous workspace without following",
        true,
        false,
        &[],
    ),
    (
        "send-stay-prev-k",
        "Move window to previous workspace without following",
        true,
        false,
        &[],
    ),
    (
        "send-stay-prev-left-arrow",
        "Move window to previous workspace without following",
        true,
        false,
        &[],
    ),
    (
        "send-stay-prev-up-arrow",
        "Move window to previous workspace without following",
        true,
        false,
        &[],
    ),
    (
        "send-stay-next-j",
        "Move window to next workspace without following",
        false,
        false,
        &[],
    ),
    (
        "send-stay-next-l",
        "Move window to next workspace without following",
        false,
        false,
        &[],
    ),
    (
        "send-stay-next-down-arrow",
        "Move window to next workspace without following",
        false,
        false,
        &[],
    ),
    (
        "send-stay-next-right-arrow",
        "Move window to next workspace without following",
        false,
        false,
        &[],
    ),
];

/// Item 5 explicit output-send catalog rows: eight follow rows (four letters
/// plus four separate arrow rows, Win+Ctrl+Alt defaults) and four canonical
/// stay rows (one per direction, bindable unbound). Separate letter/arrow
/// follow rows keep the per-binding model truthful like focus/move: disabling
/// or rebinding one never touches the other. Stay rows carry their canonical
/// letter slot by row identity (shared with the follow arm on the same key),
/// so a rebind routes into the shared hold slot through the explicit stay
/// action.
const OUTPUT_SEND_ROWS: [(&str, &str, &str, bool, &[&str]); 12] = [
    (
        "send-output-left",
        "Send window to output left",
        "left",
        true,
        &["Win+Ctrl+Alt+H"],
    ),
    (
        "send-output-left-arrow",
        "Send window to output left",
        "left",
        true,
        &["Win+Ctrl+Alt+Left"],
    ),
    (
        "send-output-down",
        "Send window to output down",
        "down",
        true,
        &["Win+Ctrl+Alt+J"],
    ),
    (
        "send-output-down-arrow",
        "Send window to output down",
        "down",
        true,
        &["Win+Ctrl+Alt+Down"],
    ),
    (
        "send-output-up",
        "Send window to output up",
        "up",
        true,
        &["Win+Ctrl+Alt+K"],
    ),
    (
        "send-output-up-arrow",
        "Send window to output up",
        "up",
        true,
        &["Win+Ctrl+Alt+Up"],
    ),
    (
        "send-output-right",
        "Send window to output right",
        "right",
        true,
        &["Win+Ctrl+Alt+L"],
    ),
    (
        "send-output-right-arrow",
        "Send window to output right",
        "right",
        true,
        &["Win+Ctrl+Alt+Right"],
    ),
    (
        "send-output-left-stay",
        "Send window to output left without following",
        "left",
        false,
        &[],
    ),
    (
        "send-output-down-stay",
        "Send window to output down without following",
        "down",
        false,
        &[],
    ),
    (
        "send-output-up-stay",
        "Send window to output up without following",
        "up",
        false,
        &[],
    ),
    (
        "send-output-right-stay",
        "Send window to output right without following",
        "right",
        false,
        &[],
    ),
];

/// Item 14 whole-workspace migration catalog rows: one follow-only row per
/// direction, all bindable unbound. Separate direction rows keep the
/// per-binding model truthful like the output sends: rebinding one never
/// touches another. The canonical slot rides the letter key per direction
/// (H/J/K/L); an arrow-chord rebind routes into that slot through the
/// explicit migrate action.
const MIGRATION_ROWS: [(&str, &str, &str); 4] = [
    (
        "migrate-workspace-left",
        "Migrate workspace to output left",
        "left",
    ),
    (
        "migrate-workspace-right",
        "Migrate workspace to output right",
        "right",
    ),
    (
        "migrate-workspace-up",
        "Migrate workspace to output up",
        "up",
    ),
    (
        "migrate-workspace-down",
        "Migrate workspace to output down",
        "down",
    ),
];
fn workspace_row(index: u8, op: SnapFamilyOp) -> BindingDef {
    // Static per-index rows (KDE `workspaceShortcutCatalog` parity: select
    // `Meta+1..0`, send `Meta+Shift+1..0`). Shifted US symbols share the
    // digit virtual key, so they act as the send arm, never a separate row.
    // Conflict comes from the row's own default chord (taskbar owns digits).
    macro_rules! row {
        ($id:expr, $text:expr, $chord:expr) => {
            BindingDef {
                id: $id,
                text: $text,
                family: BindingFamily::Workspace { index, op },
                defaults: $chord,
                implemented: true,
                conflict: chord_conflict($chord[0]),
            }
        };
    }
    match (index, op) {
        (1, SnapFamilyOp::Primary) => row!("workspace-select-1", "Focus workspace 1", &["Win+1"]),
        (2, SnapFamilyOp::Primary) => row!("workspace-select-2", "Focus workspace 2", &["Win+2"]),
        (3, SnapFamilyOp::Primary) => row!("workspace-select-3", "Focus workspace 3", &["Win+3"]),
        (4, SnapFamilyOp::Primary) => row!("workspace-select-4", "Focus workspace 4", &["Win+4"]),
        (5, SnapFamilyOp::Primary) => row!("workspace-select-5", "Focus workspace 5", &["Win+5"]),
        (6, SnapFamilyOp::Primary) => row!("workspace-select-6", "Focus workspace 6", &["Win+6"]),
        (7, SnapFamilyOp::Primary) => row!("workspace-select-7", "Focus workspace 7", &["Win+7"]),
        (8, SnapFamilyOp::Primary) => row!("workspace-select-8", "Focus workspace 8", &["Win+8"]),
        (9, SnapFamilyOp::Primary) => row!("workspace-select-9", "Focus workspace 9", &["Win+9"]),
        (0, SnapFamilyOp::Primary) => row!(
            "workspace-select-0",
            "Focus or create the trailing empty workspace",
            &["Win+0"]
        ),
        (1, SnapFamilyOp::Shifted) => row!(
            "workspace-send-1",
            "Move window to workspace 1",
            &["Win+Shift+1"]
        ),
        (2, SnapFamilyOp::Shifted) => row!(
            "workspace-send-2",
            "Move window to workspace 2",
            &["Win+Shift+2"]
        ),
        (3, SnapFamilyOp::Shifted) => row!(
            "workspace-send-3",
            "Move window to workspace 3",
            &["Win+Shift+3"]
        ),
        (4, SnapFamilyOp::Shifted) => row!(
            "workspace-send-4",
            "Move window to workspace 4",
            &["Win+Shift+4"]
        ),
        (5, SnapFamilyOp::Shifted) => row!(
            "workspace-send-5",
            "Move window to workspace 5",
            &["Win+Shift+5"]
        ),
        (6, SnapFamilyOp::Shifted) => row!(
            "workspace-send-6",
            "Move window to workspace 6",
            &["Win+Shift+6"]
        ),
        (7, SnapFamilyOp::Shifted) => row!(
            "workspace-send-7",
            "Move window to workspace 7",
            &["Win+Shift+7"]
        ),
        (8, SnapFamilyOp::Shifted) => row!(
            "workspace-send-8",
            "Move window to workspace 8",
            &["Win+Shift+8"]
        ),
        (9, SnapFamilyOp::Shifted) => row!(
            "workspace-send-9",
            "Move window to workspace 9",
            &["Win+Shift+9"]
        ),
        (_, SnapFamilyOp::Shifted) => row!(
            "workspace-send-0",
            "Move window to a newly appended workspace",
            &["Win+Shift+0"]
        ),
        (_, SnapFamilyOp::Primary) => row!(
            "workspace-select-0",
            "Focus or create the trailing empty workspace",
            &["Win+0"]
        ),
    }
}

fn directional_conflict(id: &str) -> Option<&'static str> {
    let chord = match id {
        "focus-left" => "Win+H",
        "focus-left-arrow" => "Win+Left",
        "focus-down" => "Win+J",
        "focus-down-arrow" => "Win+Down",
        "focus-up" => "Win+K",
        "focus-up-arrow" => "Win+Up",
        "focus-right" => "Win+L",
        "focus-right-arrow" => "Win+Right",
        "move-left" => "Win+Shift+H",
        "move-left-arrow" => "Win+Shift+Left",
        "move-down" => "Win+Shift+J",
        "move-down-arrow" => "Win+Shift+Down",
        "move-up" => "Win+Shift+K",
        "move-up-arrow" => "Win+Shift+Up",
        "move-right" => "Win+Shift+L",
        "move-right-arrow" => "Win+Shift+Right",
        _ => return None,
    };
    chord_conflict(chord)
}

/// OS conflict for one physical Win-family chord, for the settings UI: what
/// the OS owns at the rebound chord, not the stale original. Documented
/// owners follow the official Microsoft "Keyboard shortcuts in Windows" list
/// (Windows 11 tab; Windows 10 tab for the Win+U Ease-of-Access origin).
/// Chords the list does not document carry the honest unverified note instead
/// of a definitive clean claim; unparsable chords report `None` (not
/// applicable). Win+Alt chords (keyboard resize) report the honest
/// ownership-unknown note: Windows Alt-chord ownership is unverified in the
/// repository and containment is unproven live, never a stock-holder or
/// conflict-free claim.
/// Unshifted Win+Ctrl chords (item 1 history) report the recorded
/// virtual-desktop note for Left/Right and the honest ownership-unknown note
/// for Tab/letters/Up/Down, never a stock-holder or conflict-free claim.
/// Win+Ctrl+Alt chords (item 5 output send) report the honest
/// ownership-unknown note: Windows Ctrl+Alt ownership is UNKNOWN (decision
/// 5.3), no new Compatible disables are selected, and containment
/// is unproven live. Only containment with live trace evidence claims
/// Shifted Win+Ctrl chords (item 2 relative sends) report the honest
/// ownership-unknown note: Windows Ctrl+Shift arrow ownership is UNKNOWN
/// (decision 2.1), no new Compatible disables are selected, and containment
/// is unproven live. Only containment with live trace evidence claims
/// containment (Game Bar, Xbox mode); every other kept chord honestly
/// reports override-needs-takeover with containment unproven.
#[must_use]
pub fn chord_conflict(text: &str) -> Option<&'static str> {
    let chord = parse_chord(text).ok()?;
    if chord.alt && chord.ctrl {
        // Item 5 explicit output-send arm (Win+Ctrl+Alt, decision 5.3):
        // Windows ownership is unknown and containment is unproven live.
        // Never a stock-holder or conflict-free claim; no Compatible disable.
        return Some(OUTPUT_SEND_CONFLICT);
    }
    if chord.alt {
        // Keyboard resize arm (Win+Alt / Win+Shift+Alt, no Ctrl): Windows
        // Alt-chord ownership is unverified in the repository; containment
        // is unproven live. Ctrl+Alt chords also land here with the same
        // honest note rather than a definitive claim.
        return Some(RESIZE_CONFLICT);
    }
    if chord.ctrl {
        // Item 2 relative-send arm: shifted Win+Ctrl. Windows Ctrl+Shift
        // arrow ownership is unknown; keep defaults pending evidenced
        // conflicts with the honest unknown-ownership text (no new
        // Compatible disables, no invented holder claims).
        if chord.shift {
            return Some("Windows shortcut ownership unknown; containment unproven live");
        }
        return Some(match chord.vk {
            VK_LEFT | VK_RIGHT => {
                "Windows virtual desktop switch (ownership unverified in repository); override needs takeover, containment unproven live"
            }
            VK_TAB | 0x48 | 0x4A | 0x4B | 0x4C | VK_UP | VK_DOWN => {
                "No documented conflict in this list; other apps may bind it (Windows virtual-desktop ownership unverified in repository)"
            }
            _ => {
                "No documented conflict in this list; other apps may bind it (Windows virtual-desktop ownership unverified in repository)"
            }
        });
    }
    match (chord.vk, chord.shift) {
        (0x41, false) => Some("Action Center owns Win+A; override needs takeover"),
        (0x42, false) => Some("notification area focus owns Win+B; override needs takeover"),
        (0x43, false) => Some(
            "Copilot owns Win+C (search when Copilot unavailable); override needs takeover, containment unproven live",
        ),
        (0x44, false) => Some("desktop show/hide owns Win+D; override needs takeover"),
        (0x45, false) => Some("File Explorer owns Win+E; override needs takeover"),
        (0x46, false) => Some("Feedback Hub owns Win+F; override needs takeover"),
        (0x48, false) => {
            Some("Voice dictation owns Win+H; override needs takeover, containment unproven live")
        }
        (0x49, false) => Some("Settings owns Win+I; override needs takeover"),
        (0x4A, false) => Some(
            "Recall owns Win+J on supported devices; override needs takeover, containment unproven live",
        ),
        (0x4B, false) => {
            Some("Cast owns Win+K; override needs takeover, containment unproven live")
        }
        (0x4C, false) => Some(
            "OS lock owns unshifted Win+L: the hook cannot reliably intercept it; needs explicit opt-in",
        ),
        (0x4D, false) => Some("minimize-all owns Win+M; override needs takeover"),
        (0x4E, false) => Some("notification center owns Win+N; override needs takeover"),
        (0x4F, false) => Some("orientation lock owns Win+O; override needs takeover"),
        (0x50, false) => Some("project mode owns Win+P; override needs takeover"),
        (0x51, false) => Some("search owns Win+Q; override needs takeover"),
        (0x52, false) => Some("Run dialog owns Win+R; override needs takeover"),
        (0x53, false) => Some("search owns Win+S; override needs takeover"),
        (0x54, false) => Some("taskbar cycle owns Win+T; override needs takeover"),
        (0x55, false) => Some(
            "Accessibility settings owns Win+U; override needs takeover, containment unproven live",
        ),
        (0x56, false) => Some("clipboard history owns Win+V; override needs takeover"),
        (0x57, false) => Some("Widgets owns Win+W; override needs takeover"),
        (0x58, false) => Some("Quick Link menu owns Win+X; override needs takeover"),
        (0x59, false) => Some("Mixed Reality owns Win+Y; override needs takeover"),
        (0x5A, false) => Some("snap layouts owns Win+Z; override needs takeover"),
        (VK_LEFT, false) => Some("Snap owns Win+Left; override needs takeover"),
        (VK_RIGHT, false) => Some("Snap owns Win+Right; override needs takeover"),
        (VK_UP, false) => Some("native maximize owns Win+Up; override needs takeover"),
        (VK_DOWN, false) => Some("native minimize owns Win+Down; override needs takeover"),
        (VK_LEFT, true) | (VK_RIGHT, true) => {
            Some("move between monitors owns Win+Shift+Left/Right; override needs takeover")
        }
        (VK_UP, true) | (VK_DOWN, true) => {
            Some("stretch vertically / restore owns Win+Shift+Up/Down; override needs takeover")
        }
        (0x41, true) => Some(
            "Windows tip focus owns Win+Shift+A; override needs takeover, containment unproven live",
        ),
        (0x47, false) => {
            Some("Xbox Game Bar owns Win+G; the hook cannot fully contain the OS override")
        }
        (0x4D, true) => Some("restore minimized owns Win+Shift+M; override needs takeover"),
        (0x52, true) => Some("Snipping screen recording owns Win+Shift+R; override needs takeover"),
        (0x53, true) => Some("Snipping screenshot owns Win+Shift+S; override needs takeover"),
        (0x54, true) => Some("previous taskbar app owns Win+Shift+T; override needs takeover"),
        (0x56, true) => Some("notifications cycle owns Win+Shift+V; override needs takeover"),
        (0x7A, false) => Some("Xbox mode owns Win+F11; containment is incomplete"),
        (vk, false) if (0x30..=0x39).contains(&vk) => {
            Some("taskbar launch owns Win+digits; override needs takeover")
        }
        (vk, true) if (0x30..=0x39).contains(&vk) => {
            Some("taskbar new instance owns Win+Shift+digits; override needs takeover")
        }
        _ => Some("No documented conflict in this list; other apps may bind it"),
    }
}

/// Native Shift polarity of one catalog binding: the Shift state the
/// classifier must observe for the chord to dispatch this action.
#[must_use]
pub fn binding_wants_shift(def: &BindingDef) -> bool {
    match def.family {
        BindingFamily::Directional { op, .. } | BindingFamily::Workspace { op, .. } => {
            op.wants_shift()
        }
        BindingFamily::WorkspaceStay { .. } | BindingFamily::WorkspaceSendRelative { .. } => true,
        // Item 14 migration rides Win+Ctrl+Alt+Shift unshifted-never: the
        // full-modifier chord is the single native arm.
        BindingFamily::WorkspaceMigrate { .. } => true,
        // Item 5 output sends ride Win+Ctrl+Alt unshifted; a stay row rebound
        // with Shift is accepted via `binding_modifiers_ok` below.
        BindingFamily::WorkspaceOutput { .. } => false,
        BindingFamily::WorkspaceHistory { .. } => false,
        BindingFamily::Toggle { kind } => kind.wants_shift(),
        BindingFamily::Launch { .. } => false,
        BindingFamily::Resize { mode, .. } => mode == "inwards",
    }
}

/// Native Ctrl polarity of one catalog binding: item 1 history arms ride
/// Win+Ctrl; item 2 relative sends ride Win+Ctrl+Shift; item 5 output sends
/// ride Win+Ctrl+Alt([+Shift] for a shifted stay rebind); item 14 migration
/// rows ride Win+Ctrl+Alt+Shift; every other
/// implemented arm rides without Ctrl by default. Numbered stay additionally
/// accepts the Win+Ctrl+Shift backlog arm via [`binding_modifiers_ok`], and
/// the float/fullscreen toggles additionally accept the Compatible Win+Ctrl
/// arm there; those defaults stay Ctrl-free so plain Win rebinds keep working.
/// Carried through routing, duplicate detection, canonical hold slots, and
/// release pins so unbound stay actions reuse the plumbing without rework.
#[must_use]
pub fn binding_wants_ctrl(def: &BindingDef) -> bool {
    matches!(
        def.family,
        BindingFamily::WorkspaceHistory { .. }
            | BindingFamily::WorkspaceSendRelative { .. }
            | BindingFamily::WorkspaceOutput { .. }
            | BindingFamily::WorkspaceMigrate { .. }
    )
}

/// Whether a parsed chord's modifiers match one of the binding's native
/// arms. Every binding rides exactly its [`binding_wants_shift`] /
/// [`binding_wants_ctrl`] / [`binding_wants_alt`] arm, except numbered stay,
/// which additionally accepts the Win+Ctrl+Shift backlog example arm
/// (Shift required, Alt never), output stay, which additionally accepts
/// Win+Ctrl+Alt+Shift (Shift optional, Ctrl+Alt always), and the float /
/// fullscreen toggles, which additionally accept the Compatible Win+Ctrl arm
/// (unshifted, no Alt) alongside the authentic Win arm. Item 14 migration
/// rows accept only their exact Win+Ctrl+Alt+Shift arm. Digit follow and all
/// other arms are untouched: Ctrl stays refused there.
#[must_use]
pub fn binding_modifiers_ok(def: &BindingDef, shift: bool, ctrl: bool, alt: bool) -> bool {
    if shift == binding_wants_shift(def)
        && ctrl == binding_wants_ctrl(def)
        && alt == binding_wants_alt(def)
    {
        return true;
    }
    if matches!(def.family, BindingFamily::WorkspaceStay { .. }) && shift && ctrl && !alt {
        return true;
    }
    if matches!(
        def.family,
        BindingFamily::Toggle {
            kind: ToggleKind::Float | ToggleKind::Fullscreen,
        }
    ) && !shift
        && ctrl
        && !alt
    {
        return true;
    }
    matches!(
        def.family,
        BindingFamily::WorkspaceOutput { follow: false, .. }
    ) && ctrl
        && alt
        && shift
}

/// Native modifier-arm text for rebind errors: the single arm, both stay
/// arms (`Win+Shift or Win+Ctrl+Shift`; output stay `Win+Ctrl+Alt or
/// Win+Ctrl+Alt+Shift`), or both float/fullscreen arms (`Win or Win+Ctrl`).
#[must_use]
pub fn binding_arm_text(def: &BindingDef) -> &'static str {
    if matches!(def.family, BindingFamily::WorkspaceStay { .. }) {
        return "Win+Shift or Win+Ctrl+Shift";
    }
    if matches!(
        def.family,
        BindingFamily::Toggle {
            kind: ToggleKind::Float | ToggleKind::Fullscreen,
        }
    ) {
        return "Win or Win+Ctrl";
    }
    if matches!(
        def.family,
        BindingFamily::WorkspaceOutput { follow: false, .. }
    ) {
        return "Win+Ctrl+Alt or Win+Ctrl+Alt+Shift";
    }
    match (
        binding_wants_shift(def),
        binding_wants_ctrl(def),
        binding_wants_alt(def),
    ) {
        (true, false, false) => "Win+Shift",
        (false, true, false) => "Win+Ctrl",
        (true, true, false) => "Win+Ctrl+Shift",
        (false, false, false) => "Win without Shift",
        (false, false, true) => "Win+Alt",
        (true, false, true) => "Win+Shift+Alt",
        (false, true, true) => "Win+Ctrl+Alt",
        (true, true, true) => "Win+Ctrl+Alt+Shift",
    }
}

/// Native Alt polarity of one catalog binding: keyboard resize arms ride
/// Win+Alt (outwards) and Win+Shift+Alt (inwards); item 5 output sends ride
/// Win+Ctrl+Alt ([+Shift] for a shifted stay rebind); item 14 migration rows
/// ride Win+Ctrl+Alt+Shift; every other arm rides
/// without Alt.
#[must_use]
pub const fn binding_wants_alt(def: &BindingDef) -> bool {
    matches!(
        def.family,
        BindingFamily::Resize { .. }
            | BindingFamily::WorkspaceOutput { .. }
            | BindingFamily::WorkspaceMigrate { .. }
    )
}

/// Explicit classifier action for one catalog binding: VK alone cannot
/// identify focus vs relative select vs relative follow vs output follow
/// arms sharing one key, so the action pins the arm. History toggle/prev/
/// next ride distinct actions; resize outwards/inwards ride distinct resize
/// actions carrying the mode (never a directional move); existing arms keep
/// theirs.
#[must_use]
pub fn binding_action(def: &BindingDef) -> crate::snapkey::ChordAction {
    use crate::snapkey::ChordAction;
    match def.family {
        BindingFamily::Directional { .. } => ChordAction::Directional,
        BindingFamily::Resize { mode, .. } => {
            if mode == "inwards" {
                ChordAction::ResizeIn
            } else {
                ChordAction::ResizeOut
            }
        }
        BindingFamily::Workspace { .. } => ChordAction::WorkspaceDigit,
        BindingFamily::WorkspaceStay { .. } => ChordAction::WorkspaceStayDigit,
        BindingFamily::WorkspaceSendRelative { prev, follow } => match (prev, follow) {
            (true, true) => ChordAction::WorkspaceSendPrev,
            (false, true) => ChordAction::WorkspaceSendNext,
            (true, false) => ChordAction::WorkspaceSendStayPrev,
            (false, false) => ChordAction::WorkspaceSendStayNext,
        },
        BindingFamily::WorkspaceOutput { follow, .. } => {
            if follow {
                ChordAction::WorkspaceOutputFollow
            } else {
                ChordAction::WorkspaceOutputStay
            }
        }
        BindingFamily::WorkspaceMigrate { .. } => ChordAction::WorkspaceMigrate,
        BindingFamily::WorkspaceHistory { kind } => match kind {
            WorkspaceHistoryKind::Previous => ChordAction::WorkspacePrevious,
            WorkspaceHistoryKind::Prev => ChordAction::WorkspacePrev,
            WorkspaceHistoryKind::Next => ChordAction::WorkspaceNext,
        },
        BindingFamily::Toggle { kind } => match kind {
            ToggleKind::Float => ChordAction::Float,
            ToggleKind::Sticky => ChordAction::Sticky,
            ToggleKind::Maximize => ChordAction::Maximize,
            ToggleKind::Fullscreen => ChordAction::Fullscreen,
            ToggleKind::Orientation => ChordAction::Orientation,
            ToggleKind::WorkspaceTiling => ChordAction::WorkspaceTiling,
        },
        BindingFamily::Launch { kind } => match kind {
            LaunchKind::Browser => ChordAction::LaunchBrowser,
            LaunchKind::Terminal => ChordAction::LaunchTerminal,
        },
    }
}

/// Canonical virtual key of one catalog binding (first default chord).
/// History rows carry their canonical Tab/letter/arrow VK. Resize rows carry
/// their letter VK (the arrow alias shares the direction through its own
/// hold slot). An unbound stay row has no default chord but still carries
/// its canonical slot by row identity (numbered stay shares the digit VK
/// with its follow send; relative stay shares the H/K/arrow/J/L key with its
/// follow arm), so a rebind routes into the shared hold slot through its
/// explicit action. Item 5 output stay rows (unbound) claim their canonical
/// letter slot the same way (H/J/K/L per direction, shared with the follow
/// arm on that key). Item 14 migration rows (unbound, follow-only) claim
/// their canonical letter slot per direction the same way.
#[must_use]
pub fn binding_canonical_vk(def: &BindingDef) -> Option<u32> {
    if let Some(first) = def
        .defaults
        .first()
        .and_then(|text| parse_chord(text).ok().map(|chord| chord.vk))
    {
        return Some(first);
    }
    match def.family {
        BindingFamily::WorkspaceStay { index } => Some(VK_0 + u32::from(index)),
        BindingFamily::WorkspaceMigrate { direction } => Some(match direction {
            "left" => 0x48,
            "down" => 0x4A,
            "up" => 0x4B,
            _ => 0x4C,
        }),
        BindingFamily::WorkspaceOutput { direction, .. } => Some(match direction {
            "left" => 0x48,
            "down" => 0x4A,
            "up" => 0x4B,
            _ => 0x4C,
        }),
        BindingFamily::WorkspaceSendRelative { .. } => Some(match def.id {
            "send-stay-prev-h" => 0x48,
            "send-stay-prev-k" => 0x4B,
            "send-stay-prev-left-arrow" => VK_LEFT,
            "send-stay-prev-up-arrow" => VK_UP,
            "send-stay-next-j" => 0x4A,
            "send-stay-next-l" => 0x4C,
            "send-stay-next-down-arrow" => VK_DOWN,
            "send-stay-next-right-arrow" => VK_RIGHT,
            _ => return None,
        }),
        _ => None,
    }
}
