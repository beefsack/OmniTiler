# Windows active border radius default

## Goal, scope and acceptance

- Deliver dogfood (e): Windows outline radius defaults to 6 logical pixels;
  width, gap, colour and theme defaults remain unchanged.
- Cover settings, missing JSON field, CLI/proof-owner defaults and Settings
  Apply/Revert. Preserve explicit saved/CLI radii, including zero.
- Windows only; no shared-core or KDE source changes.

## Approach and outcome

- Updated Windows border/settings constants and CLI help. Missing schema-v1
  `core.border.radius` now uses the same default as a missing border section.
- Existing Settings controls consume the settings default; Revert reloads
  saved/default values without writing, rather than resetting explicit radii.
- Regressions cover defaults, missing field/section, explicit values,
  CLI override/argv consistency, owner options and byte-identical reload.
- Native MSVC Rust 1.99.0: allowlisted build/test, all-package fmt check,
  allowlisted all-targets clippy with warnings denied and diff check passed.
- Offline-delivered; user visual/live-Apply acceptance is recorded below;
  publication CI remains separate from offline evidence.

## Pending user-owned checks

- Open Settings, confirm radius 6 with defaults; Apply a different radius,
  stage another edit and Revert, confirming the saved value and appearance.
- Check default outline appearance at the active display DPI, including
  maximize/fullscreen suppression; stop and verify normal cleanup.

## User-accepted physical evidence - 2026-10-12

- Scoped normal owner, single output, Notepad/Paint, physical keyboard/mouse;
  base `e506350` plus combined directional, retile-overlay and B9 Windows changes.
  User accepts radius 6 as correct and confirms changes apply live through
  Settings Apply. Revert, game/DPI/multi-output remain pending.
