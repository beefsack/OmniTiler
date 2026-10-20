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

/// Eligibility gate over host facts. Thin normalization over the shared
/// core ordered classification ([`tiler_core::window_policy::classify_geometry`]):
/// terminal windows are ordinary tile targets (KDE parity) and dimensions
/// alone never classify fullscreen (`captionless_fullscreen` requires a
/// captionless style covering the monitor). Priority is the core order:
/// safety gates, overlays (fullscreen before maximized), then `no_activate`.
pub fn classify(facts: &WindowFacts) -> Result<(), SkipReason> {
    tiler_core::window_policy::classify_geometry(
        &safety_facts_of(facts),
        facts.captionless_fullscreen,
        facts.maximized,
    )
    .map_err(SkipReason::from)
}

/// Normalize host facts into neutral core facts. Field mapping:
/// `elevated` to `privileged`, `tool_window` to `tool`, `owned` to
/// `owned_dialog`; `captionless_fullscreen` and `maximized` ride separately
/// so the core orders the overlays after the safety gates.
pub(super) fn safety_facts_of(facts: &WindowFacts) -> tiler_core::window_policy::SafetyFacts {
    tiler_core::window_policy::SafetyFacts {
        visible: facts.visible,
        minimized: facts.minimized,
        cloaked: facts.cloaked,
        privileged: facts.elevated,
        shell: facts.shell,
        tool: facts.tool_window,
        owned_dialog: facts.owned,
        dialog: facts.dialog,
        no_activate: facts.no_activate,
    }
}

impl From<tiler_core::window_policy::EligibilityRefusal> for SkipReason {
    /// Map the neutral core refusal to the existing native diagnostic
    /// vocabulary. Only `privileged` renames (to `elevated`); every other
    /// token is already the log vocabulary.
    fn from(refusal: tiler_core::window_policy::EligibilityRefusal) -> Self {
        use tiler_core::window_policy::EligibilityRefusal as Core;
        match refusal {
            Core::Hidden => Self::Hidden,
            Core::Minimized => Self::Minimized,
            Core::Cloaked => Self::Cloaked,
            Core::Privileged => Self::Elevated,
            Core::Shell => Self::Shell,
            Core::Tool => Self::Tool,
            Core::OwnedDialog => Self::OwnedDialog,
            Core::Dialog => Self::Dialog,
            Core::Fullscreen => Self::Fullscreen,
            Core::Maximized => Self::Maximized,
            Core::NoActivate => Self::NoActivate,
        }
    }
}
