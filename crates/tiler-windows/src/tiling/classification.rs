#![forbid(unsafe_code)]
use super::*;

/// Stable opaque per-run window tokens (`w1`, `w2`, ...). Keyed by
/// `(HWND, process creation)` so a recycled HWND never inherits its
/// predecessor's token, even within the same tick. HWND values never enter
/// logs; tokens are the only window naming in production output.
#[derive(Debug, Default)]
pub struct TokenMap {
    next: u64,
    by_window: HashMap<(u64, String), String>,
}

impl TokenMap {
    #[must_use]
    pub fn token_for(&mut self, hwnd: u64, process_creation: &str) -> String {
        let key = (hwnd, process_creation.to_owned());
        if let Some(token) = self.by_window.get(&key) {
            return token.clone();
        }
        self.next += 1;
        let token = format!("w{}", self.next);
        self.by_window.insert(key, token.clone());
        token
    }

    /// Mint a fresh token for an already-known `(HWND, creation)` pair after
    /// a same-process HWND reuse repair: the new window generation must never
    /// inherit the previous generation's Engine identity, layout slot, or
    /// focus. The caller patches the current tick's rows to the returned
    /// token; later ticks mint it back stably via [`TokenMap::token_for`].
    #[must_use]
    pub fn reissue(&mut self, hwnd: u64, process_creation: &str) -> String {
        self.next += 1;
        let token = format!("w{}", self.next);
        self.by_window
            .insert((hwnd, process_creation.to_owned()), token.clone());
        token
    }

    /// Forget entries for windows no longer enumerated so the map stays
    /// bounded. Identity is `(hwnd, creation)` pairs, never HWND alone.
    pub fn retain(&mut self, live: &[ObservedTargetRef<'_>]) {
        self.by_window.retain(|(hwnd, creation), _| {
            live.iter()
                .any(|t| t.hwnd == *hwnd && t.creation == creation)
        });
    }
}

/// Borrowed identity reference for [`TokenMap::retain`].
pub struct ObservedTargetRef<'a> {
    pub hwnd: u64,
    pub creation: &'a str,
}

/// Deterministic FNV-1a 64-bit fingerprint over sorted `(token, rect)` pairs.
#[must_use]
pub fn fingerprint(windows: &[(String, Rect)]) -> u64 {
    let mut sorted: Vec<(&String, &Rect)> = windows.iter().map(|(t, r)| (t, r)).collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut hash: u64 = 0xcbf29ce484222325;
    for (token, rect) in sorted {
        for byte in token.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        for word in [rect.x, rect.y, rect.w, rect.h] {
            for byte in word.to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
    }
    hash
}

/// Pre-queried native facts for one top-level window. All fields are plain
/// observed state; policy in [`classify`] decides eligibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowFacts {
    pub visible: bool,
    pub minimized: bool,
    pub maximized: bool,
    pub cloaked: bool,
    pub elevated: bool,
    pub shell: bool,
    pub tool_window: bool,
    pub owned: bool,
    pub captionless_fullscreen: bool,
    pub no_activate: bool,
    /// Generic Win32 dialog class (`#32770`) without an owner window. Owned
    /// dialogs are reported as `OwnedDialog` instead. The project's own
    /// Settings UI is an ordinary member (see [`workspace_dialog_refuses`]),
    /// never a dialog.
    pub dialog: bool,
}

/// Closed skip vocabulary for ineligible windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    Hidden,
    Minimized,
    Maximized,
    Cloaked,
    Elevated,
    Shell,
    Tool,
    OwnedDialog,
    /// Unowned generic dialog (`#32770`): never a tile target. The
    /// project's own Settings UI is an ordinary slotless member instead.
    Dialog,
    Fullscreen,
    NoActivate,
    Unreadable,
    /// Frozen-allowlist identity stopped matching (vanished HWND, recycled
    /// HWND/PID, or foreign process): a true unknown, never a counted skip.
    /// Produced only by the stateless inspection verdict, never by `classify`.
    IdentityChanged,
}

impl SkipReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::Minimized => "minimized",
            Self::Maximized => "maximized",
            Self::Cloaked => "cloaked",
            Self::Elevated => "elevated",
            Self::Shell => "shell",
            Self::Tool => "tool",
            Self::OwnedDialog => "owned-dialog",
            Self::Dialog => "dialog",
            Self::Fullscreen => "fullscreen",
            Self::NoActivate => "no-activate",
            Self::Unreadable => "unreadable",
            Self::IdentityChanged => "identity-changed",
        }
    }
}

/// Native window class of the project's own Settings UI. Must stay in sync
/// with `crate::settings_ui::SETTINGS_WINDOW_CLASS` (duplicated here so this
/// portable policy compiles everywhere).
pub const OWN_SETTINGS_WINDOW_CLASS: &str = "OmniTilerSettings";

/// Generic Win32 dialog class. Unowned top-level dialogs are never
/// workspace members; owned ones are already excluded as owned dialogs.
/// (Duplicated here so this portable policy compiles everywhere; native
/// gates carry their own copy.)
pub const GENERIC_DIALOG_CLASS: &str = "#32770";

/// True when a top-level window is the project's own Settings UI: its class
/// is the settings class and its executable matches the owner's exactly
/// (slash/case-insensitive, same [`crate::lifecycle::exe_paths_equal`]
/// identity the owner checks use). Class alone never excludes: a foreign app
/// reusing the class name stays eligible, and our own executable with any
/// other class stays eligible.
#[must_use]
pub fn is_own_settings_window(class: &str, exe_path: &str, owner_exe_path: &str) -> bool {
    class == OWN_SETTINGS_WINDOW_CLASS
        && crate::lifecycle::exe_paths_equal(exe_path, owner_exe_path)
}

/// Workspace-membership dialog gate (Windows): true when the window refuses
/// membership as a dialog. Only an unowned generic Win32 dialog
/// (`#32770`) refuses here. The project's own Settings UI (own executable
/// plus settings class, see [`is_own_settings_window`]) is an ordinary
/// slotless workspace member: it never refuses on this gate, follows
/// workspace visibility, and supports sends without being tiled. It runs as
/// a separate Settings process (`tiler-windows settings`, detached from the
/// owner tray), so the callers' same-process gates still refuse every
/// same-process window and proof/control windows keep their own gates.
#[must_use]
pub fn workspace_dialog_refuses(
    class: &str,
    owned: bool,
    exe_path: &str,
    owner_exe_path: &str,
) -> bool {
    if is_own_settings_window(class, exe_path, owner_exe_path) {
        return false;
    }
    !owned && class == GENERIC_DIALOG_CLASS
}

/// Eligibility gate. Terminal windows are ordinary tile targets (KDE
/// parity): no ancestry Schrödinger here. Dimensions alone never classify
/// fullscreen: `captionless_fullscreen` requires a captionless style covering
/// the monitor.
pub fn classify(facts: &WindowFacts) -> Result<(), SkipReason> {
    if !facts.visible {
        return Err(SkipReason::Hidden);
    }
    if facts.minimized {
        return Err(SkipReason::Minimized);
    }
    if facts.cloaked {
        return Err(SkipReason::Cloaked);
    }
    if facts.elevated {
        return Err(SkipReason::Elevated);
    }
    if facts.shell {
        return Err(SkipReason::Shell);
    }
    if facts.tool_window {
        return Err(SkipReason::Tool);
    }
    if facts.owned {
        return Err(SkipReason::OwnedDialog);
    }
    if facts.dialog {
        return Err(SkipReason::Dialog);
    }
    // Overlay states last, fullscreen first: matches `overlay_refusal` so a
    // window holding both reports fullscreen everywhere. Safety skips above
    // keep their relative order.
    if facts.captionless_fullscreen {
        return Err(SkipReason::Fullscreen);
    }
    if facts.maximized {
        return Err(SkipReason::Maximized);
    }
    if facts.no_activate {
        return Err(SkipReason::NoActivate);
    }
    Ok(())
}

/// Maximized-overlay refusal for directional and pointer routes (KDE
/// `move-refused-maximize` / `pointer-refused-maximize` parity): a
/// maximized focused window keeps its tile slot but directional move/resize
/// on it would change its retained position/share, so it is refused
/// fail-closed before any Engine mutation. Focus carries no geometry write
/// and stays allowed. Fullscreen wins when both overlay states hold (item 4
/// owns the fullscreen toggle; this only orders the refusal vocabulary).
#[must_use]
pub const fn overlay_refusal(fullscreen: bool, maximized: bool) -> Option<&'static str> {
    if fullscreen {
        Some("fullscreen")
    } else if maximized {
        Some("maximize")
    } else {
        None
    }
}

/// Intentional-float toggle refusal for an overlay target (KDE
/// `float-refused-fullscreen` / `float-refused-maximize` parity): a
/// fullscreen or maximized focused window never floats, and a focused float
/// that the user maximized or fullscreened natively never unfloats until it
/// reads normal again. Fullscreen wins when both hold. `None` when the route
/// may proceed.
///
/// B9 exception (R-FLT-06): an ordinary maximized float being explicitly
/// tiled takes the adapter one-shot nonactivating clear plus observed-clear
/// route (see [`unfloat_maximize_clear_eligible`]) instead of ending here;
/// this snapshot vocabulary still reports that lane.
#[must_use]
pub const fn float_toggle_refusal(fullscreen: bool, maximized: bool) -> Option<&'static str> {
    if fullscreen {
        Some("float-refused-fullscreen")
    } else if maximized {
        Some("float-refused-maximize")
    } else {
        None
    }
}

/// Sticky-toggle refusal for an overlay target (KDE `sticky-refused-fullscreen`
/// / `sticky-refused-maximize` parity): a fullscreen or maximized focused
/// window never sticks, and a sticky window that the user maximized or
/// fullscreened natively never unsticks until it reads normal again.
/// Fullscreen wins when both hold. `None` when the route may proceed.
#[must_use]
pub const fn sticky_toggle_refusal(fullscreen: bool, maximized: bool) -> Option<&'static str> {
    if fullscreen {
        Some("sticky-refused-fullscreen")
    } else if maximized {
        Some("sticky-refused-maximize")
    } else {
        None
    }
}

/// B9 unfloat lane definition (R-FLT-06, COSMIC unmaximize-then-fresh-admit
/// parity): only an ordinary currently-floating target being explicitly tiled
/// takes the one-shot nonactivating clear plus observed-clear route before
/// fresh Engine admission. Fullscreen floats, sticky origins, tiled targets,
/// and non-maximized floats keep their existing refusal or admission paths;
/// the snapshot vocabulary ([`float_toggle_refusal`] /
/// [`sticky_toggle_refusal`]) still reports those. `is_float` is the Engine
/// exception state at dispatch. Production selects the lane off that live
/// vocabulary plus Engine/sticky state; this pure predicate pins the lane
/// definition for offline regressions without native windows.
#[must_use]
pub const fn unfloat_maximize_clear_eligible(
    fullscreen: bool,
    maximized: bool,
    is_float: bool,
    is_sticky_origin: bool,
) -> bool {
    is_float && !is_sticky_origin && maximized && !fullscreen
}

/// Orientation-toggle refusal for an overlay target (item 4, R-LAY-01): a
/// fullscreen or maximized focused window never toggles its parent axis,
/// matching the move/float/sticky overlay isolation. Fullscreen wins when
/// both hold. `None` when the route may proceed.
#[must_use]
pub const fn orientation_toggle_refusal(fullscreen: bool, maximized: bool) -> Option<&'static str> {
    if fullscreen {
        Some("orientation-refused-fullscreen")
    } else if maximized {
        Some("orientation-refused-maximize")
    } else {
        None
    }
}

/// Directional refusal for a sticky subject window (KDE sticky isolation
/// parity): a sticky focused window never starts directional focus/move, and
/// sticky windows are never directional targets (they ride slotless floats).
/// Focus and move share the sticky subject gate; targets exclude via the
/// floating Engine observation. `None` when the route may proceed.
#[must_use]
pub const fn sticky_directional_refusal(is_sticky: bool, is_move: bool) -> Option<&'static str> {
    if !is_sticky {
        return None;
    }
    if is_move {
        Some("move-refused-sticky")
    } else {
        Some("focus-refused-sticky")
    }
}
