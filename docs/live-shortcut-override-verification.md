# Live Shortcut Override Verification (Multi-Output Setup)

## Purpose

User-run acceptance for the current Apply / Force / Revert contract in
`docs/decisions.md#shortcuts-conflicts-and-presets`, `docs/changes/shortcut-override.md`, and
`docs/changes/kde-shortcut-conflicts.md` (full catalog and presets).
Read `docs/live-kwin-testing.md` first. It does not grant authorization.
No agent participates. Use physical keys only: `invokeShortcut` bypasses xkb
and proves nothing about delivery.

Superseded machinery is gone: no journal, no migration, no Finish Apply,
no Restore, no preimage restore, no Force-limited-to-compiled-clear-rows.
Do not follow journal-era steps from Git history.

## Setup (Multi-Output Setup)

- Use a native KDE checkout on a multi-output setup.
- Build and stage the native effect with the documented dev path only:

```sh
just build-native-effect
```

This stages under `target/kwin-native-effect-stage/kwin/...` without
touching KWin, D-Bus, config, or user paths (`README.md`, `justfile:build-native-effect`).

- For session delivery, the documented route is:

```sh
just dev-native-setup
```

It writes only `$XDG_CONFIG_HOME/plasma-workspace/env/60-omnitiler-native-effect.sh`
for this checkout, then requires logout/login, and again after every effect rebuild
(`README.md`, `docs/decisions.md`).
- For this KCM-only change, if the current session already discovers this
  checkout's stage, `just build-native-effect` followed by fully quitting and
  reopening System Settings/kcmshell6 loads the new KCM. Merely leaving and
  re-entering a page may keep its library mapped. No logout is needed for the
  dialog changes; KWin's already-loaded effect remains unchanged until logout/
  login. Initial setup, a changed delivery path, or loading a rebuilt effect
  still requires the session boundary above. KCM loading is a user-owned check.
- Packaging/install beyond that staging path (NixOS module, Home Manager,
flake package) is uncertain here and not prescribed. Report which route was
used rather than inventing store or system install steps.
- Start from a fresh Plasma login.

## Baseline (Read-Only)

- Open the project KCM (verified route in `README.md`):

```sh
kcmshell6 kwin/effects/configs/omnitiler-active-border_config
```

- In its `Shortcuts` group, capture the status label verbatim before any click.
- Read-only cleared-list state lives at
`~/.config/omnitiler/shortcut-clearedrc`
(`kwin/native-effect/shortcutreconciler.cpp:defaultClearedActionsPath`).
Inspect only; do not create, edit, or delete it.
- Ordinary Settings Apply never changes shortcuts
(`kwin/native-effect/unifiedsettings.ui:shortcutDescription`).

## Apply

- Select `Authentic (use canonical)` to stage the full canonical catalog. This
  does not write shortcuts. Existing empty own assignments initialize as
  Disable when the page opens; Authentic deliberately resets those choices.
- Click `Apply Shortcuts` (`kwin/native-effect/unifiedsettings_module.cpp:runShortcutApply`).
  There is no preliminary confirmation. With no conflicts, it applies directly;
  verify the success dialog says shortcuts are applied and active immediately,
  without overall Settings Apply/OK. Use a physical chord before clicking
  either overall button. A transport or partial-apply failure must show its
  original reason in an error dialog, without offering Force.
With Authentic staged it assigns all enabled catalog bindings, including workspace and symbol aliases.
With focus-right enabled under Authentic, it takes `Meta+L` and moves Lock Session to `Meta+Esc`,
plus `Meta+Alt+K`, `Meta+Alt+L`, `Meta+Alt+Left`, `Meta+Alt+Down`,
`Meta+Alt+Up`, `Meta+Alt+Right`, `Meta+G`, `Meta+M`,
`Meta+Left/Down/Up/Right`, and `Meta+Shift+Left/Right`. The four focus arrows
clear KWin Quick Tile defaults; the two move arrows clear KWin's next/previous
screen defaults.
- Authentic success assigns every enabled catalog action; ordinary Keep preserves current assignments. A missing enabled action or
  preflight conflict refuses without writes;
  preserve the reported write count if a later operation fails.
Capture the resulting status verbatim.

## Force Preview (Arbitrary / Legacy / Foreign Holders)

- When Apply refuses on holders, one modal scrollable conflict dialog offers
  `Force Apply` and `Cancel`; no separate inline Force controls remain
  (`unifiedsettings_module.cpp:askShortcutForce`).
- Require the preview to list every current conflicting holder with chord,
component/action display names (or IDs if no display name is available),
found keys, exact required keys removed, and unrelated keys kept
(`unifiedsettings_module.cpp:buildConflictDialogText`).
This includes unknown and legacy `kwin/omnitiler-*` IDs.
Exempt only: project actions, Lock Session, and the authorized System Monitor
`Meta+Esc` holder. The dialog must describe the actual Lock relocation and
retained-but-displaced System Monitor binding, or say Lock is untouched when
out of scope. A `.desktop`-default-only claimant with nothing to clear is still
listed with its found/retained keys and blocks Force until unbound manually;
Force Apply must be disabled and the reason visible.
- Compare the first holder/chord in status and error labels with the dialog's
  first conflict: default-only blockers first, then clearable holders, each
  sorted by component/action (blocker chord as tie-breaker).
- Check a many-holder case, including the previously reported six-versus-26
  discrepancy: compare every current holder on every in-scope chord, scroll
  to the last row, and compare row counts and indexed journal entries. No
  `(+N more)` truncation is acceptable. Do not count exempt or disabled-scope
  holders as conflicts; record any discrepancy rather than inferring its cause.
- Press `Cancel` (or dismiss the conflict dialog) and verify no shortcut or
  cleared-list writes and no followup error/confirmation dialog. The staged
  draft remains; the next Apply builds a fresh preview.
Confirmed Force revalidates owner, project/lock images, and the full holder
snapshot after confirmation; stale state aborts with zero writes, including
zero cleared-list writes if it changed before the new snapshot. If a holder
changes after the cleared-ID list is persisted, Force stops before changing
that holder and retains the list for Revert.
- Cancel, change a row's Keep/Disable checkbox, then Apply again: the new
  dialog must reflect the new draft and bind exact staged choices and action
  presence as well as active keys. If a holder changes externally while the
  modal is open, choosing Force must report a stale error without writes.

## Confirmed Force

- Click Apply Shortcuts for a fresh conflict dialog, then choose `Force Apply`.
  This is the single Force confirmation; no extra question follows. Capture
  the success/error result. Success must say applied and active immediately,
  independent of overall Settings Apply/OK; verify physical delivery now.
- Clearing removes only the required keys and preserves unrelated keys.
The union of cleared component/action IDs is persisted to
`shortcut-clearedrc` before clearing, so an interrupted Force stays revertible.

## Revert (Defaults, Not Preimages)

- Click `Revert Shortcuts` and confirm only the dialog titled
`Revert Shortcuts` (`unifiedsettings_module.cpp:runShortcutRevert`).
It names the recorded cleared count and states custom cleared bindings are lost.
- Revert restores KDE defaults for every non-project cleared entry via
`defaultShortcutKeys` / `setForeignShortcutKeys`; project-owned IDs stay
cleared. Partial failure retains the list for resume; an empty list is a
no-op success.
- Verify a result dialog follows the retained data-loss confirmation: success
  reports immediate activation; failure gives its reason and retains recovery
  state. An empty cleared list also reports a successful no-op result.
- Old `shortcut-override-journalrc` files (including any kcmshell6 legacy path)
are ignored and must remain untouched. No migration, Finish, or Restore exists.

## Physical Checks

With disposable windows, physically press each chord and confirm:

- `Meta+L` moves focus right without locking.
- `Meta+Esc` locks the session (System Monitor displacement is expected).
- `Meta+Alt+K` / `Meta+Alt+L` grow the focused pane outward.
- `Meta+Alt+Left` / `Meta+Alt+Down` / `Meta+Alt+Up` / `Meta+Alt+Right`
  grow the focused pane outward (clearing `kwin/Switch Window
  Left/Down/Up/Right`).
- `Meta+Left/Down/Up/Right` move focus by direction without KWin quick tiling.
- `Meta+Shift+Left/Right` move the focused tiled window by direction without
  KWin moving it to another screen.
- `Meta+G` toggles float; `Meta+M` toggles maximize.
- Then Revert and confirm cleared non-project actions, including Quick Tile
  and next/previous screen, return to their KDE defaults. Project chords
  remain assigned, and old project IDs stay cleared.

## Conflict List, Disable, and Presets (User-Owned KDE Acceptance)

- Record commit, Plasma/KWin version, keyboard layout, and delivery route.
  Check all 129 rows appear with readable canonical/current/default chords;
  distinguish own holders from foreign conflicts. Missing/unavailable queries
  must not claim a clean or applied state. Include workspace digits and
  shifted-symbol aliases on the actual keyboard layout.
- Stage `Compatible (disable conflicting)`: known and discovered foreign
  default/current-holder conflicts become unchecked, other bindings stay Keep,
  and no replacement chords appear. Capture the disabled rows. Do not click Apply Shortcuts;
  use ordinary Settings Apply, and close: shortcut assignments must not change.
- Stage Compatible again and explicitly click Apply Shortcuts. Disabled
  actions have no project assignment; their foreign holders are unchanged.
  In particular, disabling focus-right must not move Lock Session or alter
  Meta+Esc. On a previously Authentic session the existing lock relocation is
  retained until separately reverted; Compatible does not undo earlier Force.
- Disable one otherwise non-conflicting row manually, Apply, close/reopen:
  the row remains Disable. Log out/in through the documented native delivery
  route and verify the disabled chord does not invoke the tiler, while a kept
  focus/move/resize/workspace binding does. Capture any re-registration loss.
- Use an explicitly chosen disposable foreign shortcut with a project chord
  and an unrelated chord. Keep/Apply must refuse, Force must preview exact
  removals, and confirmed Force must retain the unrelated chord. Disable that
  project row instead: Apply must not clear its foreign holder. Change the
  foreign holder after preview and verify stale Force refuses without writes.
- After Force, stage Compatible again: a discovered foreign default conflict
  must still be disabled even when its current assignment was cleared.
  Revert Shortcuts separately restores recorded foreign KDE defaults; own
  Disable choices stay empty. Custom cleared bindings are lost by the existing
  default-restoration contract. No interrupted-Force recovery success is
  claimed until the existing retained-ID/retry checks above are performed.
- Stage Authentic with an in-scope conflict, cancel its dialog, then repeat
  Apply and choose Force: no change on Cancel; on success kept bindings return
  to canonical chords. Without a conflict Apply acts directly.
  A custom rebind made in KDE Shortcuts is shown as current; only this staged
  Authentic Apply deliberately replaces it. First-run prompt/in-page rebind
  controls are deferred; opening Settings alone must not correct shortcuts.

### Pending: M13 Keep preserves custom KDE chords across Apply

- In KDE System Settings > Shortcuts, customize a disposable nonconflicting
  project binding. Reopen project Settings, leave it Keep, click Apply
  Shortcuts, and verify its current assignment and physical delivery remain
  customized. Repeat after closing/reopening and the user-owned session restart.
- Repeat with a canonical chord and an empty assignment checked Keep: each
  remains unchanged. Only staged Authentic followed by explicit Apply/confirmed
  Force returns these bindings to canonical; staging or Cancel never writes.
- After successful Authentic Apply/Force, customize again in KDE Shortcuts
  with the project page still open, then click another Apply. The new custom
  assignment must survive: successful commit consumes the reset intent.
- Give a disposable foreign action the same custom chord plus an unrelated
  chord. Verify the conflict list and Force preview name the custom chord,
  confirmed/revalidated Force preserves the project custom chord and the
  foreign unrelated chord, and stale confirmation refuses. Stage Compatible
  and verify conflicting rows disable while nonconflicting custom rows remain
  unchanged. Ordinary Settings Apply and separate Revert keep their isolation.

### Pending: workspace tiling default Meta+Y

- Record fresh registration separately from upgrading an existing empty/custom
  assignment. Existing Keep assignments remain authoritative; stage Authentic
  and explicitly Apply/confirm Force to request the new canonical Meta+Y chord.
- Press physical Meta+Y twice and compare with the tray checkbox: each toggles
  only the current workspace's tiled/floating flag, reflected in its snapshot
  and tray menu. Another workspace and the saved new-workspace default retain
  their values. Physical input, not `invokeShortcut`, proves delivery.
- Verify the new KCM row's canonical chord, Keep/custom/empty preservation,
  Disable, Authentic reset and restart persistence. Other 36 unbound rows stay
  unbound. Ordinary Save and cancelled conflict dialogs never change keys.
- Compatible keeps Meta+Y when there is no foreign holder; an actual custom
  holder is discovered and stages Disable. Apply/Force/Revert retain the existing
  selection, stale-preview and foreign-default-restoration contract above.
- User-owned pending acceptance; agents run no live KWin cases.

## Diagnostics

Bounded shortcut diagnostics only:

```sh
journalctl --user --no-pager -g "omnitiler.shortcut op="
```

Records carry `op=`, `stage=`, `outcome=` with shortcut component/action IDs and
raw key images. Conflict summaries state counts; each indexed row records all
holder IDs and chords (long rows have numbered chunks), at info/warning levels.
Compare the full list, not just the first refusal line. Friendly labels and
user content are excluded. Logging never gates behavior.
