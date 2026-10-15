use super::*;

/// Binding preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// KDE catalog defaults for every binding. The Win+L explicit opt-in is
    /// preserved: `allow_win_l` keeps its current value.
    Authentic,
    /// Authentic minus every known OS-conflicting chord: all focus rows (Win+H
    /// voice, Win+J recall, Win+K cast, Win+L lock, arrows Snap/maximize/
    /// minimize), all move-arrow rows (Win+Shift+arrows monitor-move/stretch),
    /// float (Win+G Game Bar), maximize (Win+M minimize-all), fullscreen
    /// (Win+F11 Xbox mode), orientation (Win+O orientation lock), workspace
    /// tiling (Win+Y Mixed Reality), both
    /// launchers (Win+B notification-area focus, Win+T taskbar cycle), all
    /// workspace digits (Win[/Shift]+digits
    /// taskbar launch/new-instance), and the two item 1 history arrows
    /// (Win+Ctrl+Left/Right native virtual-desktop switch, ownership
    /// unverified in repository) are disabled. What stays is exactly the
    /// undocumented set: letter moves (Win+Shift+H/J/K/L) and sticky
    /// (Win+Shift+G) carry no documented owner in the official list, plus
    /// the seven kept history rows (Tab/letters/Up/Down with the honest
    /// ownership-unknown note) and the eight item 2 relative-send follow
    /// rows (Win+Ctrl+Shift ownership unknown, no new disables selected).
    /// The eight item 5 output follow rows (Win+Ctrl+Alt ownership unknown,
    /// decision 5.3, no new disables selected) are likewise kept.
    /// Both presets keep the keyboard resize defaults: Windows Alt-chord
    /// ownership is unverified in the repository, so neither preset invents
    /// an owner nor claims conflict-free; Compatible disables known
    /// conflicts only. Item 2 stay rows stay unbound under both presets.
    /// Both presets also keep the item 5 output follow defaults with the
    /// honest ownership-unknown note (decision 5.3, no Compatible disables,
    /// no invented stock holder); the four output stay rows stay unbound
    /// under both presets.
    /// Applies as a deterministic reset: all overrides are
    /// dropped first, then the conflicts disable. Manual rebinding stays
    /// available afterwards; no replacement defaults are invented. The Win+L
    /// opt-in is preserved.
    Compatible,
}

/// Apply one preset in place. Core settings are untouched; only binding
/// overrides change. Returns the ids whose override changed.
pub fn apply_preset(settings: &mut Settings, preset: Preset) -> Vec<&'static str> {
    let mut changed = Vec::new();
    match preset {
        Preset::Authentic => {
            for def in binding_catalog() {
                if settings.bindings.remove(def.id).is_some() {
                    changed.push(def.id);
                }
            }
        }
        Preset::Compatible => {
            // Deterministic reset to the known safe set: drop every override
            // first (a stale rebound pointing at an OS-owned chord must not
            // survive the preset), then disable the conflicts.
            for def in binding_catalog() {
                if settings.bindings.remove(def.id).is_some() {
                    changed.push(def.id);
                }
            }
            for id in compatible_disabled_ids() {
                settings.bindings.insert(
                    id.to_owned(),
                    BindingSetting {
                        state: BindingState::Disabled,
                        chord: None,
                    },
                );
                changed.push(id);
            }
        }
    }
    settings.revision = settings.revision.saturating_add(1);
    changed
}

/// Preset decision helper: the ids the compatible preset disables (every
/// known OS-conflicting implemented row; resize rows keep their defaults
/// under both presets since Windows Alt-chord ownership is unverified, and
/// item 5 output follow rows keep theirs since Windows Ctrl+Alt ownership is
/// unknown;
/// see [`Preset::Compatible`]).
#[must_use]
pub const fn compatible_disabled_ids() -> [&'static str; 41] {
    [
        "focus-left",
        "focus-left-arrow",
        "focus-down",
        "focus-down-arrow",
        "focus-up",
        "focus-up-arrow",
        "focus-right",
        "focus-right-arrow",
        "move-left-arrow",
        "move-down-arrow",
        "move-up-arrow",
        "move-right-arrow",
        "toggle-float",
        "toggle-maximize",
        "toggle-fullscreen",
        "toggle-orientation",
        "toggle-workspace-tiling",
        "launch-browser",
        "launch-terminal",
        "workspace-select-1",
        "workspace-select-2",
        "workspace-select-3",
        "workspace-select-4",
        "workspace-select-5",
        "workspace-select-6",
        "workspace-select-7",
        "workspace-select-8",
        "workspace-select-9",
        "workspace-select-0",
        "workspace-send-1",
        "workspace-send-2",
        "workspace-send-3",
        "workspace-send-4",
        "workspace-send-5",
        "workspace-send-6",
        "workspace-send-7",
        "workspace-send-8",
        "workspace-send-9",
        "workspace-send-0",
        "workspace-prev-left-arrow",
        "workspace-next-right-arrow",
    ]
}
