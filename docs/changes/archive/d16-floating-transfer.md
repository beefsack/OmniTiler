# D16 floating transfer (REQ-WS-05 / R-WS-05)

## Goal and authorized behavior

- Explicit numbered/relative follow/stay sends retain intentional float state
  and size, without source reflow or destination tile changes.
- User 2026-10-13: same-output exact frame, no geometry write. Floating plus
  fullscreen or maximize carries exactly as-is; later overlay exit returns to
  the floating frame on the destination, never tiled admission.
- Sticky and automatic-fixed refusals remain unchanged. No setting.
- Shared core first, then KDE; Windows adapter/native delivery is handoff
  item 22. Existing floating-workspace boundary behavior remains intact.
- [Decision](../../decisions.md#workspaces),
  [reference discriminators](../../spec/reference-outcomes/workspaces.md#d16-combo-and-arrival-frame-discriminators-2026-10-13).

## Approach and delivery

- Reuse D28's additive slotless `MoveDeferred` path (4c1c3a4 + 77cac6a).
  Retained floating-only exception flags and float geometry rehome verbatim;
  neither tree changes. Live overlay state travels natively.
- Shared eligibility adds the observed floating-plus-single-overlay arm with
  the existing sticky/fixed-origin guards. Retained-record and D28 gates stay
  unchanged; no protocol or architecture change.
- KDE's three mover-entry gates and deferred shape fence admit both combos.
  The send wire carries existing fullscreen state for the focused mover and
  existing maximized state only for a floating focused mover. Other member
  encoding stays unchanged. Deferred actuation performs membership only,
  with existing arrival, scope, identity, flag-stability and follow/stay fences.
- Intentional-float restart memory remains window-id keyed. Ordinary floats
  were delivered before the combo follow-up; both are now complete offline.

## Accepted evidence

- Ordinary delivery: shared core 567 passed; KWin final 1397 passed;
  workspace tests, strict clippy, fmt, typecheck and bundle passed. The earlier
  floating-fullscreen entry parity refusal was reconciled at all three gates.
- Combo red/green: shared send initially refused `NotTiled`; KDE regressions
  initially failed dispatch/wire/actuation. Both pass with the combo arms.
- `crates/tiler-core/tests/floating_deferred_send.rs`: 16 tests cover retained
  float geometry/homing, unchanged trees, follow/stay bookkeeping, roundtrip,
  overlay exit, sticky/dual-overlay/fixed-origin refusal, sole-source/empty
  target, ordinary tiled sends and cross-output carry.
- `kwin/tests/workspace-send-float-combo-carry.test.ts`: 37 tests cover both
  combos, deferred shape/flag races, empty geometry and drifted survivors,
  real entry/production observers, and cross-output fences. Diagnostic ordinals
  alone are not relative-target resolution evidence.
- `kwin/tests/workspace-native.test.ts`: eight real shortcut journeys cover
  both combos x numbered/relative x follow/stay with hydrated float intent.
  Each proves resolved target, exact arrival flags/frame, one membership write
  and zero geometry/fullscreen/maximize writes.
- `kwin/tests/workspace-send-float-combo-production.test.ts`: five journeys
  share a real Planner across float, carry and subsequent convergence. Both
  combos x follow/stay preserve native overlay frames and unchanged tiled
  allocations. Simulated native overlay exit restores a distinct remembered
  float frame; a forced destination refresh proves no tiled admission or
  adapter writes. Fullscreen roundtrip preserves both states on both legs.
- Pinned KWin v6.6.3 [Window::setDesktops](https://github.com/KDE/kwin/blob/v6.6.3/src/window.cpp#L711-L758)
  and [X11Window::doSetDesktop](https://github.com/KDE/kwin/blob/v6.6.3/src/x11window.cpp#L1506)
  do not clear fullscreen or maximize. No carry workaround is indicated by
  desktop-assignment source; native settlement remains unverified.
- Final gates: `cargo test --workspace` 1639 passed across 57 result suites,
  zero failed/ignored; workspace all-target strict clippy and fmt passed.
  KWin build plus serial JS suite: 1475 passed across 218 suites, zero
  failed/skipped; both typecheck configurations and bundle passed.
  `cargo check -p tiler-windows` passed with zero Windows source changes.
  No native source changes; CTest not applicable. Independent review found
  no semantic blocker; ambiguous eligibility documentation was clarified.

## Outcome and pending acceptance

- D16 shared-core and KDE delivery is complete offline, including the approved
  floating+fullscreen/maximized follow-up. Windows adapter work remains
  [handoff item 22](../../backlog.md#open-work).
- User-owned native checks: ordinary and combo WS1 -> WS2 -> WS1 roundtrips;
  numbered/relative follow/stay, sole-window source and empty target; exact
  frames/flags/focus, no source/destination reflow, and restart float memory.
- Exit fullscreen/maximize on the destination and verify the floating frame
  returns without tiled admission. Check native Wayland/Xwayland behavior;
  stop and report if native membership movement drops or corrupts the overlay.
