# Windows Dual-Output Live Fixes

## Goal and scope

Fix the reported cross-output transfer, secondary pager, workspace-command
routing and fresh-window admission defects, then advance the Windows
[two-monitor checklist](windows-physical-checklist.md#two-monitor-pc).
Windows adapter changes only; no new platform-specific core policy. W2M-03
and W2M-12 remain blocked on their existing world-route contracts.

## Acceptance and approach

- Diagnose each reported failure from supplied traces and current source.
- Keep exact transfer/lifetime/visibility fences; do not accept a materially
  wrong allocation as a successful cross-output arrival.
- Route workspace commands by genuine focus, and ordinary fresh admission
  by the focused output before newcomer activation (approved R-OUT-05).
  Preserve cold-start per-output placement.
- Support secondary pager strips only with proven taskbar exclusion geometry.
- Add targeted offline regressions, run native locked four-package build/test,
  format and strict clippy gates, and distinguish agent-driven observations
  from pending physical keyboard/display checks.

## Current evidence

- Workspace commands preferred a still-present last-switch cache over native
  focus. Foreground-first resolution passes offline tests and an agent-driven
  two-output select journey through the exact-owner CLI using the same output
  resolver. Each output's test windows hid on its own workspace selection.
  Physical digit-chord delivery remains pending.
- Fresh admission uses native frame position. An unshipped correction retained
  pre-newcomer focus and preserved cold-start inventory. Independent review
  caught an intake-before-admission cache pollution race.
- Agent-driven live checks (2026-10-20): startup placements survived owner
  restart on both outputs; focused-side admission without activation passed;
  one activation direction passed, but the converse still admitted on the
  native side. This is an unresolved acceptance failure, not a passing
  two-output admission result. Trace then proved that previously enumerated,
  hidden, memberless newcomers polluted retention before admission. A
  membership-only intake correction fixed that direction, but the opposite
  focused interval did not reliably update retention. Further verification
  also exposed DPI-virtualized monitor bounds in the external harness. Both
  admission prototypes are parked outside the checkout; neither is shipped.
  Next: use physical monitor bounds and correlate genuine focus/retention
  transitions before selecting another admission correction.
- Reported transfer trace shows size scaling near the monitor DPI ratio and
  subsequent reverse scaling. Exact arrival verification correctly refused
  the mismatched frame. Root native settlement is still under investigation;
  an unaware-caller placement probe is not evidence for the PMv2 product path.
- Secondary taskbars are enumerated but deliberately refused with
  `secondary-tray-unproven`; per-output pager support remains undelivered.
- Live sessions ended with verified graceful stop, independent restoration,
  byte-identical settings, unchanged relevant SPI/taskbar state, absent ledger
  and no owned window residue. Detailed evidence remains external.
- Final isolated workspace-routing gates (2026-10-20), Rust/Cargo 1.99.0,
  native MSVC: locked four-package build/tests, workspace format, strict
  all-target clippy and whitespace passed: 2,836 passed, zero failed, one
  ignored (core 1,042; protocol 27; FFI 40; Windows 1,727).

## Remaining units

1. Re-establish a physical-coordinate admission fixture and resolve retained
   focus sampling before another admission implementation.
2. Diagnose PMv2 cross-DPI placement with owned disposable windows; preserve
   exact arrival fences and add a regression for any demonstrated correction.
3. Prove secondary taskbar exclusion and implement per-output pager strips.
4. Run full native gates and bounded checklist legs; park physical-only,
   fixture-dependent and explicitly blocked rows without inferred passes.

## Decisions and limits

- Tentative (user review pending): workspace-command fallback is foreground,
  pointer, last-switch cache, stable output order, consistent with the tray's
  existing focused-output resolver. New-window admission never uses pointer.
- Parked admission prototypes used an HWND-only freshness hint; a successor
  must retain the existing full identity/tag fences and avoid treating an
  enumerated-but-memberless hidden window as genuine established focus.
- Physical shortcuts, monitor changes/hotplug, shell restart, sleep/wake and
  crash recovery have not been accepted by the current journeys.
