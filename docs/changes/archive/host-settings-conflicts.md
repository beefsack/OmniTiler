# Host Settings Conflicts (2026-09-29, offline complete)

## Goal and boundary

Identify Plasma 6 host settings that interfere with the tiler, implement the
approved settings-page controls offline, and research uninstall behavior.
No live KWin mutation or host config writes were used for verification. Installed
`kwin_wayland --version` and `plasmashell --version` both report 6.7.5. The test-system report
(`omnitiler-dev.H28tD1.log` and user visual observation) is
native edge preview/move competing with our Meta-drag preview, followed by our
correct retile; this research does not independently replay or attribute that
log. Source defaults below are upstream 6.7.5, **not measured host values**.

## Verified inventory, highest impact first

`kwinrc` means the user's effective KConfig `kwinrc` (a missing key uses its
upstream default). Detect effective values, not just presence in a raw file.
For the first three rows, native KWin reads the setting at startup or on
`org.kde.KWin /KWin reconfigure`: KWin reparses configuration, updates options,
then uses the new values on subsequent interactive moves. A queued reconfigure
request is not confirmation of running KWin's application: read back effective
config, report send as unconfirmed, and check live behavior separately before
claiming KWin pickup. A KWin restart is not required by the upstream
reconfigure path. Source: [schema](https://raw.githubusercontent.com/KDE/kwin/v6.7.5/src/kwin.kcfg),
[move/edge implementation](https://raw.githubusercontent.com/KDE/kwin/v6.7.5/src/window.cpp),
[reconfigure entry](https://raw.githubusercontent.com/KDE/kwin/v6.7.5/src/dbusinterface.cpp),
[reconfigure implementation](https://raw.githubusercontent.com/KDE/kwin/v6.7.5/src/workspace.cpp).

| Priority / exact setting | Default, conflict, desired state | Detect; correction; revert; application |
| --- | --- | --- |
| P0 `kwinrc [Windows] ElectricBorderTiling` | `true`; left/right edges and corners show KWin quick-tile outline and apply native quick tile on drop during a move, even during our Meta-drag. For uncluttered project drag preview: `false`. | Read effective boolean with KConfig (`kreadconfig6 --file kwinrc --group Windows --key ElectricBorderTiling` is a read-only diagnostic); explicitly write `false` on Fix, remove the local key on Revert (effective KDE default `true`). KWin `/KWin reconfigure` applies live. [Edge check](https://raw.githubusercontent.com/KDE/kwin/v6.7.5/src/window.cpp) `checkQuickTilingMaximizationZones`, `finishInteractiveMoveResize`; [schema](https://raw.githubusercontent.com/KDE/kwin/v6.7.5/src/kcms/screenedges/kwinscreenedgesettings.kcfg). |
| P0 `kwinrc [Windows] ElectricBorderMaximize` | `true`; top edge shows native maximize preview and maximizes on drop. For no competing top-edge preview: `false`. | Read effective boolean; explicitly write `false` on Fix, remove the local key on Revert (effective default `true`). Same live reconfigure path. [Edge check](https://raw.githubusercontent.com/KDE/kwin/v6.7.5/src/window.cpp) `checkQuickTilingMaximizationZones`; [schema](https://raw.githubusercontent.com/KDE/kwin/v6.7.5/src/kwin.kcfg). |
| P1 `kwinrc [Windows] ElectricBorders` | `0` (desktop switching at edges disabled). If configured to switch desktop while moving, KWin can carry a project drag to another desktop; this is a conditional conflict, **not** the stock edge-preview cause. Desired `0`. Separate `[ElectricBorders] Top/Left/...` actions default `None` and are suppressed while a window is grabbed; do not bulk-clear. | Read effective integer; show only when nonzero with Fix only; remove the local key on Fix (effective default `0`). KWin `ScreenEdges::reconfigure` on options change applies live. [Screen-edge schema](https://raw.githubusercontent.com/KDE/kwin/v6.7.5/src/kcms/screenedges/kwinscreenedgesettings.kcfg), [handling](https://raw.githubusercontent.com/KDE/kwin/v6.7.5/src/screenedge.cpp), [enum](https://raw.githubusercontent.com/KDE/kwin/v6.7.5/src/screenedge.h). |
| P1 KGlobalAccel `kwin/Window Quick Tile Left/Bottom/Top/Right`, `kwin/Switch Window Left/Down/Up/Right`, `kwin/Window to Previous/Next Screen` | Stock chords respectively `Meta+Left/Down/Up/Right`, `Meta+Alt+Left/Down/Up/Right`, `Meta+Shift+Left/Right`. They steal the project's focus/grow/move arrow aliases, **but do not cause edge-drag previews**. Desired: project's exact bindings own the chords. | Existing KCM keyed live occupancy check and explicit Apply/Force/Revert; no second config-file shortcut setter. Revert restores KDE defaults, not previous custom keys. KGlobalAccel writes/readback take effect live, no KWin restart. [KWin shortcut registration](https://raw.githubusercontent.com/KDE/kwin/v6.7.5/src/useractions.cpp) `initShortcuts`, `docs/../../decisions/reference-and-shortcuts.md#shared-shortcut-defaults-and-conflict-model`, `kwin/native-effect/shortcutreconciler.h`. |
| P1 other exact project-required shortcut occupants | `ksmserver/Lock Session` stock `Meta+L` collides with project focus-right and is explicitly relocated to `Meta+Esc`. Known holders: `kwin/Grid View` on `Meta+G`, `kwin/KrohnkiteMonocleLayout` on `Meta+M`, `KDE Keyboard Layout Switcher/Switch to Next Keyboard Layout` on `Meta+Alt+K`, `KDE Keyboard Layout Switcher/Switch to Last-Used Keyboard Layout` on `Meta+Alt+L`. These latter defaults depend on plugins/layout and are **unverified as universal defaults**; detect actual live occupancy, including unknown holders of the 15 required chords. | Existing keyed KGlobalAccel occupancy and KCM Apply/Force/Revert (including read-only Force preview) already handle the exact required chords; restore KDE defaults for cleared IDs. Live KGlobalAccel, no KWin restart. See `kwin/native-effect/shortcutreconciler.h:167-242`, `kwin/native-effect/unifiedsettings_module.cpp:420-565`, `docs/decisions.md:681-766`. |
| P2 built-in tiling editor, `kwin/Edit Tiles` | `Meta+T` opens KWin's default-enabled Tiling Editor. **No collision with current project shortcuts or automatic Meta-drag**; do not flag merely because enabled. User-rebound `Edit Tiles`, `Window Custom Quick Tile Left/Right/Top/Bottom` (stock unbound), or four corner quick-tile actions (stock unbound) matter only when they actually occupy a project-required chord. | Check exact live shortcut holder by chord using existing keyed KGlobalAccel check; if occupied, the existing Force preview/correction/revert applies. Do not disable `tileseditorEnabled` or delete `[Tiling]` layout data. Live shortcut setter/readback; plugin-enable-key reconfigure behavior unverified and unnecessary here. [Editor action](https://raw.githubusercontent.com/KDE/kwin/v6.7.5/src/plugins/tileseditor/tileseditoreffect.cpp), [default enabled](https://raw.githubusercontent.com/KDE/kwin/v6.7.5/src/plugins/tileseditor/metadata.json), [unbound custom/corner actions](https://raw.githubusercontent.com/KDE/kwin/v6.7.5/src/useractions.cpp). |

Important limitation: KWin 6.7.5 also shows a **Shift-held custom tile
preview** during an interactive move and can apply `QuickTileFlag::Custom` on
release with Shift held. `window.cpp` `updateInteractiveMoveResize` and
`finishInteractiveMoveResize` show this independent branch; no config switch was
verified. It is not the reported plain Meta-drag edge conflict, and the two
electric-border toggles do not disable it. Detect it only from the gesture;
avoid Shift while dropping if it interferes. Config correction/revert and
live-apply mechanism: not applicable/unverified. `ElectricBorderCornerRatio`
defaults `0.25` and changes the side/corner region, not whether native tiling
occurs; it is not a separate disable switch. Border/window snap zones, focus
and placement policy, window rules, and `[Tiling]` editor layouts have no
verified independent conflict with the reported project gesture: do not
surface or rewrite them on this evidence.

## Selected design and outcome

User decision 2026-09-29 (option A, tray follow-up): keep icon left-click
opening the existing menu. While any of the settings-page host conflicts is
present, add a warning overlay and a top-row "Conflicting KDE settings..."
that opens the existing Settings path; remove both when resolved, including
changes made without restarting the tray. Keep snapshot-loss NeedsAttention
status, existing menu rows, and their behavior. No one-time notification or
change to icon click behavior. Shipped offline; panel visuals remain to check.

User decisions 2026-09-29: Fix all three `[Windows]` keys above; Revert restores
KDE defaults with **no** preimage, ownership tracking, journal, or stale
refusal. Simplification: delete the local key for either boolean
Revert and `ElectricBorders` Fix, with no duplicate Borders Revert or retry
state. Settings-page-only alert now; tray indicator/icon opening normal
Settings later, not here. Uninstall should restore defaults, but its mechanism
is not selected. `docs/../../decisions/settings.md#shared-settings-catalog-and-platform-ownership` is authoritative.

The shared native KCM (`kwin/native-effect/unifiedsettings_module.cpp`,
`unifiedsettings.ui`) now reads effective KConfig at page open/load and after
each explicit Fix/Revert. Boolean rows always show the current value with
exactly one Fix/Revert button; `ElectricBorders` shows only when nonzero with
Fix alone. Boolean Fix writes `false`; boolean Revert and Borders Fix remove
the local key. The immediate operation syncs, sends `/KWin reconfigure`,
reparses/readbacks, and logs exactly one normal-level
`omnitiler.window-conflicts: op=<fix|revert> setting=<key>
outcome=<ok|failed> reason=<ok|write-failed|send-failed|readback-mismatch>`
line. `outcome=ok` means persisted/readback matched and a reconfigure send was
queued, **not** proof that the running compositor applied it. Error text is
visible after failure; button visibility follows only the re-read value.
Normal Save, startup, and shortcut logic are
unchanged. Existing shortcut Apply/Force/Revert already detects the 15 exact
required-chord conflicts on this page (`shortcutreconciler.h`,
`docs/../../decisions/reference-and-shortcuts.md#shared-shortcut-defaults-and-conflict-model`); no second binding helper was added.

Offline evidence: `just build-native-effect` passed; host-matched separate
`BUILD_TESTING=ON` native build and full CTest **30/30** passed (new
`native-script-config-windows`: detection, isolation from Save, Fix/Revert
readback, failure display and one log per operation); KWin `npm test`
**805/805** and `npm run typecheck` passed. No Rust or install scripts changed.
No live KWin behavior or configuration was mutated or accepted.

## Tray indicator outcome (offline)

The tray checks the same three stable KDE keys and defaults as
`kwin/native-effect/unifiedsettings_module.cpp` directly.
There is no shared artifact or native settings-page change. Rust reads
effective `kwinrc [Windows]` values with KDE's `kreadconfig6` from the existing
KConfig package; missing keys use KDE defaults. It reads unconditionally at
startup. On the existing one-second watchdog, it checks the user `kwinrc`
mtime and rereads only when that changes (including file creation/removal);
read failures keep the last known indicator. System-wide-only config edits
may wait until the user file changes. KCM Fix/Revert and user-file edits are
visible without restarting the tray. Detection stays separate from
authenticated KWin snapshot freshness and workspace toggle.

Boolean reads use `kreadconfig6 --type bool --default true` exit status
(0=true, 1=false). Read-only host inspection found the typed integer form
emits no stdout, so the user selected the typeless `--default 0` form for
`ElectricBorders`. Its stdout is parsed as an integer; spawn or parse failure
means unknown and keeps the last known indicator.

On conflict, the SNI advertises `OverlayIconName=dialog-warning` and the
existing DBusMenu gets a visible top "Conflicting KDE settings..." row opening
the same unified Settings command as the normal Settings item. Without
conflict, the overlay name is empty and the row hidden. Plasma 6.7.5
[system tray source](https://raw.githubusercontent.com/KDE/plasma-workspace/v6.7.5/applets/systemtray/statusnotifieritemsource.cpp)
reads the overlay for normal and attention icons and refreshes on
`NewOverlayIcon`; the [SNI specification](https://specifications.freedesktop.org/status-notifier-item/latest/status-notifier-item.html)
keeps OverlayIconName independent of Status. No `NewStatus` is emitted for a
conflict-only change. Snapshot loss still reports NeedsAttention; left-click
still opens the menu. The tray emits one
`omnitiler:route-diag component=tray-endpoint stage=projection
event=projected outcome=conflict-updated conflict=<true|false>` line per
observed transition, never per heartbeat; no clean-start false transition.

Rust workspace tests, fmt, strict clippy and
`nix flake check --no-build --offline` are the tray change's offline checks;
native KCM files match HEAD. No live tray or KWin test is claimed. The user
confirmed settings-page Fix works live; Revert and panel overlay transitions
remain user-owned live checks.

## Uninstall feasibility (research only)

With no tracking, reset cannot tell who set a value: resetting a current fix
value also resets a user-made matching value (`false` for either boolean, `0`
for `ElectricBorders`). The latter already equals its KDE default, so deleting
its local key leaves the effective value `0` when no other layer overrides it. It
also cannot identify whether a different value was once set by our UI.
No current delivery route implements this reset.

| Route | Verified uninstall mechanism and per-user reset feasibility |
| --- | --- |
| `just dev` | `justfile:494-681` `dev-off` stops only its verified session resources and re-enables the packaged script. A **new explicit** same-user pre-removal recipe can call a per-user reset command; automatically resetting on every `dev-off` would also alter ordinary worktree teardown. No reset exists. |
| Dogfood | `scripts/dogfood-install.sh:170-174,222-255`: `uninstall` deletes its script directory and expressly leaves `kwinrc` alone; `enable/disable` show a same-user `kwriteconfig6` + `/KWin reconfigure` mechanism. A new explicit reset subcommand or opt-in uninstall flag is technically possible; plain uninstall does not reset. |
| NixOS flake/module | `nixos-module.nix:17-25`, `flake.nix`: disabling removes the system-owned immutable `/etc/xdg/kwinrc` `[Plugins]` entry and packages, not any user's `[Windows]` keys. System activation has no verified per-user session or safe user-home target; no implicit per-user reset. |
| Home Manager | `home-manager-module.nix:32-68` delivers packages/services without a `home.activation` hook or `kwinrc` ownership. [HM activation blocks](https://home-manager.dev/manual/unstable/internals/activation.html) can run as the user on *activation*, but reliably running one **only on removal**, with the session bus alive, is unverified. A pre-removal opt-in user command can run while the package remains available. |
| KDE Store KPackage | The four-file KPackage in `docs/research/distribution-package-feasibility/feasibility.md:18-30` has no uninstall script. [KPackage PackageJob::uninstall](https://codebrowser.dev/kde/kpackage/src/kpackage/packagejob.cpp.html) deletes the package and emits a signal, with no verified package-supplied per-user uninstall callback. It does not reset `[Windows]`; use an explicit pre-remove command if a native KCM companion is installed. |
| Future RPM/DEB/Arch | No package recipes shipped (`docs/research/distribution-package-feasibility/obs.md`). [RPM `%preun`/`%postun`](https://man7.org/linux/man-pages/man7/rpm-scriptlets.7.html) and [Debian `prerm`/`postrm`](https://www.debian.org/doc/debian-policy/ch-maintainerscripts.html) run on remove **and** upgrades under the system package manager, not the selected user's Plasma session; a per-user `kwinrc` edit and live reconfigure from there are unverified/unsafe. Arch per-user hook behavior unverified. Provide user-session pre-removal instructions instead. |

Options (additional rough production LOC; no selection yet):

1. **Explicit user-session reset before removal**, reusing KCM Revert buttons
   or one dedicated reset command: ~0-90 LOC depending on route. Works with
   every package manager if done before uninstall; skip it and fixed values
   remain. Recommend this as the smallest reliably per-user mechanism.
2. **Opt-in uninstall/reset integration** for `just` and dogfood, and optional
   Home Manager user activation if removal detection is proven: ~80-200 LOC per
   route plus tests. More convenient where user-owned lifecycle exists, but
   cannot cover plain KDE Store or system-package removal and may run when
   the session bus is absent. Combine with option 1 for other routes.
3. **Implicit reset on every uninstall** including system package removal:
   ~200-500+ LOC across routes; no verified universal per-user hook, can
   reset another user's settings or run on upgrades, and cannot attribute fix
   ownership. Not recommended.

Decision to ask: must an unattended plain package removal reset per-user
settings even when there is no reliable user-session hook, or is an explicit
pre-remove reset step acceptable for those routes? Recommend the explicit
step; this question does not block the shipped page controls.

## Manual live check (user-owned, not run)

After an authorized installation/session setup, open the same Settings page
from tray, Scripts Configure, and Effects Configure. With effective `true`,
`true`, and nonzero values, see three explained conflicts; for each press Fix
and verify its effective `kwinrc [Windows]` value becomes `false`, `false`, `0`
and the native edge preview/desktop jump no longer competes. Both bool rows
stay visible, switching from Fix to Revert; the Borders row disappears. Revert
each boolean, verify its local key is absent and effective `true` returns, and
confirm edge preview returns. The KCM process (not `[kwin]` dev logs) should emit one
`omnitiler.window-conflicts: op=fix|revert setting=... outcome=ok reason=ok`
line per click. Red flags: an absent/wrong row, shortcut or ordinary Save
changing these keys, a success claim on a failed request, more than one
operation log per click, or native preview persisting after a successful
reconfigure plus confirmed effective value. A queued request alone is not
live behavior proof; no live mutation is authorized by this note.

With a single current KWin snapshot, fresh KDE default host values show the
warning overlay and top conflict row; left-click still opens the menu. Click
the row: it opens the same unified settings page as Settings. Fix both edge
booleans (and `ElectricBorders` if nonzero): without a tray restart, the row
and warning disappear, with one tray `outcome=conflict-updated conflict=false`
line. Revert either boolean: row and warning return, with one corresponding
`conflict=true` line. No repeated line on the next heartbeat. Lose the KWin
snapshot naturally: tray Status remains NeedsAttention even if the conflict
warning remains. Red flags: changing `Status` solely for a host conflict,
left-click opening Settings directly, stale row/overlay after effective values
change, or repeated conflict logs while values are steady.

## Backlog detail moved 2026-11-01

### Original backlog lines 2170-2177

- Automatic uninstall revert of host settings: parked research after 0.1
  (user 2026-10-10), aiming for "uninstall leaves no changed KDE settings".
  Possibly via package manager hooks; no verified per-user hook exists for
  Nix/Home Manager, KDE Store or distro packages. For 0.1, document pressing
  Revert in Settings before removal in install/uninstall docs, package
  descriptions and a post-removal package note (P1 packaging acceptance).
  [research](host-settings-conflicts.md)
