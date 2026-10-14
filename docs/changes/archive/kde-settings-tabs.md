# KDE Settings tabs

- Status: delivered 2026-10-15; grouping and pre-0.1 timing are **tentative,
  user review pending**, independently of the pending shared-schema decision.
- Goal: make the existing Settings controls easier to navigate using native Qt
  tabs in the QWidget KCM.
- Preserve every control, widget binding, persisted key, default, validation,
  Apply/Revert/Defaults behavior, shortcut conflict dialog and asynchronous
  shortcut operation. No new settings or shared-schema machinery.
- Implementation: regroup `kwin/native-effect/unifiedsettings.ui` with a
  standard `QTabWidget`, initially on Tiling & Workspaces. The two existing
  layout-text assertions now reference the tab instead of "this page".

## Grouping

| Tab | Existing controls |
| --- | --- |
| Tiling & Workspaces | Workspace mode, same-axis move, fixed-size admission, born-maximized admission, migration source refill, inner and outer gaps. |
| Appearance | Border color/width/radius/gap, theme color, drop-target preview color, group underlay color/extension. |
| Shortcuts | Binding catalog and Keep/Disable choices, Authentic/Compatible presets, Apply Shortcuts/Revert Shortcuts, conflict and operation feedback. |
| Desktop Integration | Existing window-edge conflict status and ElectricBorderTiling, ElectricBorderMaximize, ElectricBorders Fix/Revert controls. |

- Script save/reconfigure Status remains visible below all tabs. Shortcut
  worker status, disabled controls and conflict dialogs retain their existing
  implementation on the Shortcuts tab.

## Accepted evidence

- Settings-only CMake build with `OMNITILER_BUILD_EFFECT=OFF`, warnings denied
  (`-Wall -Wextra -Werror`); CTest **26/26** passed.
- Existing layout-dependent TypeScript suites **30/30** passed; existing gap
  and admission/move settings suites **66/66** passed. No shared code changed.
- Original control names/properties and form rows retained, including all eight
  `kcfg_*` bindings and fourteen label buddies. No C++ or persistence changes.
- Fresh dev/staged `kcmshell6` loaded the rebuilt KCM without a session restart.
  Background screenshots showed all four tabs rendering without clipped
  controls; AT-SPI actions and selected-state changes proved tab switching.
  Script Status stayed visible on each tab. Literal ampersand escaping was
  corrected after the first visual check and reverified on the rebuilt KCM.
- Closed with Cancel; no settings applied. `kwinrc` and shortcut-cleared record
  hashes and modification times matched the baselines afterwards.
- `git diff --check` passed. Grouping/size preference remains user review;
  this check does not establish every theme, translation or small-window size.

## Exact next action

- Open `kcmshell6 kwin/effects/configs/omnitiler-active-border_config`, review
  the four tabs and preferred dialog size, then close with Cancel. Confirm or
  adjust the tentative grouping and pre-0.1 timing. Shared-schema route/timing
  remains a separate pending decision.
