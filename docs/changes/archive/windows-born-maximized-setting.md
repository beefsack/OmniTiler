# Windows born-maximized admission setting

## Goal, scope and acceptance

- Deliver dogfood (d), R-MAX-06: additive schema-v1 `core.born_maximized`.
  Default `tile-on-launch` clears maximize once then ordinarily admits;
  `keep-maximized-overlay` reserves a tile slot without clearing maximize.
- Functional Settings labels and reference-WM tooltips; Apply/Revert, strict
  invalid-file/last-good handling, presets preserve the value. Live adoption
  applies to subsequent admissions only, with no retroactive tree rebuild.
- Preserve fullscreen isolation, existing fixed-client handling, R-MAX-03,
  later user maximizes and separate fullscreen-exit/unfloat work. No KDE edits.

## Approach and verification

- Reuse Windows retained-slot/overlay write fences for first-seen tiled
  admission. Keep floating-to-tiled admission on its existing independent path.
- Cover schema/defaults/exact tokens, native UI staging, production live
  adoption, real retained Engine slot/restore/sibling geometry, repeat/lifetime
  safety and fullscreen/floating exclusions.
- Required native MSVC build/test/fmt/strict clippy and diff checks; publication
  CI separately. User-owned launch/restore, Settings and gaming checks pending.

## Material decisions

- No existing KDE/shared setting IDs were present after synchronization;
  functional IDs are recorded in the R-MAX-06 decision for cross-platform reuse.
- Born-maximized overlay can use the existing Windows retained machinery;
  R-MAX-03 remains handoff item 11 rather than expanding this setting's scope.

## Outcome and accepted evidence

- Windows-only settings/UI/admission delivery; no shared-core/protocol or KDE
  source changes. Native radio groups are independent, with functional labels
  and the selected WM tooltips. Revert uses the existing saved/default reload.
- Overlay choice binds to the exact fresh lifetime after ownership fences on
  first-seen tiled admission. Existing row assembly reserves its slot, excludes
  native geometry writes while maximized and restores ordinary writes on exit.
- Initial live-value gate also affected floating-first retile; corrected to a
  lifetime-scoped binding. Regressions now preserve R-MAX-03 under both values
  and live flips, plus born-fullscreen exits, later maxima and reused lifetimes.
- Real retained Engine coverage proves sibling allocation, overlay write
  exclusion and exact restore to the reserved slot. Production settings adoption
  preserves the retained snapshot and returns no rebuild/wake request. Native
  admission ownership-fence placement was inspected; no live admission run.
- Native MSVC Rust 1.99.0 allowlisted build/test, all-package fmt check,
  allowlisted all-targets strict clippy and diff check passed; independent
  contract review found no blocking issue. Publication CI follows separately.
- Existing Windows fixed-size detection is unwired (handoff item 13): this
  delivery preserves that gap and does not claim D4 fixed-client acceptance.
- Offline-delivered; both-values ordinary launch physically accepted below;
  remaining native legs pending.

## Pending user-owned checks

- With B tiled, launch a maximized ordinary A under each value: default clears
  once then tiles; overlay keeps maximize, allocates B's share and restores A
  into its slot. Repeat maximize/restore and hide/reveal without fighting.
- Apply the opposite value while A is admitted: A stays unchanged, the next
  born-maximized client uses the new value. Stage an edit and Revert; verify the
  saved value and independent workspace/same-axis controls.
- Check browser session-restored launch, fullscreen/borderless game isolation
  and a game launching maximized-windowed; stop and verify cleanup. Game safety
   wins. Physical input/display, session-boundary and recovery remain user-owned.

## User-accepted physical evidence - 2026-10-12

- Scoped normal owner, single output, Notepad/Paint, physical keyboard/mouse;
  base `e506350` plus combined directional, retile-overlay and B9 Windows changes.
  Paint launched maximized under both `core.born_maximized` values; user
  accepts the specified tile-on-launch and keep-maximized-overlay behavior.
- Browser session restore, detailed Apply/Revert/admitted-client adoption,
  fixed-size, game/DPI/multi-output remain pending.

## Second agent-live sweep (2026-10-17)

- Fresh Paint under `tile-on-launch` was observed natively maximized before
  admission, then unmaximized into its tiled slot; the correlated trace recorded
  the one-time admission clear. Under `keep-maximized-overlay`, native maximize
  persisted through later settled samples with retained-overlay observation
  and no admission clear. Row assembly reserves the overlay slot; exclusion
  from writable tiles is not evidence that no slot exists.
- App frame/zoom and admission sequence support these Paint-only launch legs;
  no direct opaque-token-to-HWND mapping was retained. Destination unmaximize,
  subsequent-only adoption, fixed/maximized intersection and game/DPI/output
  matrix are not newly accepted. Saved settings and SPI were restored exactly.
