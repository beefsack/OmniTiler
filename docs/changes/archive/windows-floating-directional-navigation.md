# Windows floating directional navigation

## Goal and acceptance

- Handoff item 10, R-FLT-07..11 / REQ-FLT-07..11: reuse the existing
  directional focus/move catalog for ordinary/sticky float focus and stateless
  work-area half-snaps. Tile-origin navigation excludes floats.
- Match the recorded COSMIC/KDE top-left axis race, sticky-first ordering,
  first Up/Left tie and last Down/Right tie. Miss retains focus without workspace
  cycling; Windows cross-output actuation remains a separate implementation gap.
- Half-snaps use the raw work area and inner gap, retain float/sticky identity
  and focus, issue one geometry write, and never admit into the Engine tree.
  Quarter/maximize/outward-transfer snap states remain deferred.

## Approach and bounded work

1. Port adapter-native selection and half geometry; use retained Engine float
   classification, existing focus actuation, lifetime/scope/overlay/gesture and
   write-time suspension fences. No shared core/protocol or KDE code edits.
2. Include visible same-output sticky targets across home workspaces and
   retained maximized float targets. Ordinary floats stay workspace-local;
   automatic fixed floats keep D8 no-touch behavior.
3. Pin axis/tie/arithmetic/repeat/constraint policy and real retained Engine
   classification, candidates and post-snap reconciliation offline.
4. Update matrix/spec/decision/backlog evidence, then native application checks.

## Decisions

- [Selected policy](../../decisions.md#window-state-float-sticky-maximize-fullscreen)
  owns geometry, ties, stateless repeats and layering. Item 18 clears a
  maximized float before this ordinary half-snap path.
- Tentative 2026-10-12, pending user review: candidate encounter order is
  eligible observed rows followed by retained maximized rows, with sticky-first
  layering applied to the combined list. Equal-axis ties involving both sets
  can differ from native enumeration order.

## Accepted evidence and outcome

- Delivered offline 2026-10-12 from base `e506350` in
  `crates/tiler-windows/src/tiling.rs`, `src/tiling_sys.rs` and
  `tests/tiling.rs`. Local float logic is adapter-native as selected; ordinary
  tiled navigation and subsequent reconcile continue through the shared Engine.
- Review found omitted retained-maximized targets and non-home sticky targets;
  both corrected. Real Engine fixtures cover those candidates, token mismatch,
  tile/other-output exclusion and float exception/frame preservation across
  reconcile. The Engine's re-float fallback is not overwritten by a half-snap.
- Native MSVC offline gates: rustc 1.99.0, locked four-package build/test,
  all-workspace formatting, strict four-package all-targets clippy, and
  `git diff --check` pass. Combined items 10/18 add 19 tests; Windows library
  328 and tiling integration 111 pass. No native actuation acceptance claimed.
- Native pending: focus/readback/clear timing, client clamp without reassertion,
  held repeats, sticky non-home behavior, mixed DPI and physical preset/Snap
  suppression. Linux/CI gates pending publication. Ready-to-run application
  journey is returned with the delivery handover; only Notepad/Calculator/Paint.

## Next action

- Run the ordinary-application float focus/half-snap and coordinated item-18
   journey for remaining tie/sticky/clamp/preset legs; ordinary physical focus
   and half-snaps are accepted below.

## Native-session result - 2026-10-12

- Directional checks NOT-RUN: no normal-owner directional CLI; injected keys
  are excluded by the product filter. No physical keys or injected test chords
  were sent, so float focus and half-snap native cells remain TBD.
- The combined scoped Notepad/Paint session exercised
  [B9](windows-maximized-intentional-unfloat.md#agent-observed-native-evidence---2026-10-12),
  not item 10. Graceful stop/restore and owner absence verified, saved settings
  byte-identical, test windows closed, non-test baseline unchanged and Terminal
   visible/unminimized. Physical feel/game/DPI/multi-output remain user-owned.

## User-accepted physical evidence - 2026-10-12

- Scoped normal owner, single output, Notepad/Paint, physical keyboard/mouse;
  source base `e506350` plus the combined directional, retile-overlay and B9
  Windows changes. User accepts Win+arrow focus from floats and
  Win+Shift+arrow half-snaps as specified. Exact tie order, non-home sticky,
  fixed-size/client-clamp, preset variants, game/DPI/multi-output remain pending.
- Reference tally: 9 available profiles, 2 applicable but incomparable tie-order
  mechanisms (COSMIC `3d55cba0`, unified top-left race;
  Hyprland `ae50c4d6`, unified center/angle race), 7 without a comparable
  maximize candidate/float-origin verb (bspwm, i3, xmonad, qtile, karousel,
  PaperWM, paneru). Neither applicable profile splits observed/retained rows;
  0 establish observed-first ties. Hyprland's existing pin was unavailable;
  this source evidence is qualified to the inspected revision.
- Basis: COSMIC `src/shell/mod.rs:4154-4204` and
  `src/shell/layout/floating/mod.rs:352-406`; Hyprland
  `src/desktop/state/WindowQuery.cpp:209-247`. Distance already wins across
  classes/layers in Windows; observed-before-retained only breaks equal-axis
  ties. Keep Tentative 2026-10-12, pending user review; no behavior change.
  The existing [tie discriminator](../../spec/reference-outcomes.md#windows-floating-candidate-discriminator-2026-10-12)
  remains reference/native TBD; deeper comparable tie-order research is pending.

## Second agent-live sweep (2026-10-17)

- Scripted directional focus/move and float half-snap acceptance is blocked by
  the deliberate injected-input fence. No directional or snap chord was sent
  through to native Snap. An active scoped-hook/owned-foreground harmless-key
  probe produced no dispatch or geometry change; it does not replace physical
  shortcut evidence. Existing user acceptance stands; remaining tie/sticky/
  clamp/preset/DPI/output legs remain pending. Settings/SPI and ledger restored.
