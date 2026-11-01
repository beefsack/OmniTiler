# Shortcut Scope

## Goal

Preserve the initial US-keyboard shortcut catalog and the explicit KCM boundary
for conflicting Plasma-global bindings.

## Scope And Acceptance

- Existing shifted aliases remain. Layout detection, migration, opt-in support,
  and reconciliation are post-release work.
- Non-conflicting registration is automatic. The explicit KCM override alone
  may move KDE lock from `Meta+L` to `Meta+Esc`; Revert restores only bindings
  still owned by that override.
- After MVP, adding a tiling type requires a selectable profile/type for both
  its tiling behavior/algorithm and matching shortcuts. This future requirement
  does not change the initial shortcut catalog or US-keyboard scope.

## Next Action

Choose complete post-release layout support only after initial release.

## Backlog detail moved 2026-11-01

### Original backlog lines 2095-2098

- P2 | Keyboard-layout support | After initial release; US keyboards only
  for MVP. Read-only investigation queued: behavior of `Meta+Shift+1..9` and
  the US shifted-symbol aliases on German, AZERTY, UK and Nordic layouts.
  [change](shortcuts.md)

### Original backlog lines 2129-2131

- P3 | Post-MVP tiling profiles | Re-expose `shortcutProfile` with distinct
  catalogs and live switching when adding new tiling types.
  [change](shortcuts.md)
