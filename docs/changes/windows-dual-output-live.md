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
- Retain interim foreground-first workspace-command routing (`1e7bfaf4`).
  Admission waits on the cross-platform cursor-first matrix; approved R-OUT-05
  remains in force until Tier 1 evidence supports an override. Preserve
  cold-start per-output placement.
- Support secondary pager strips with exact tray/clock exclusion geometry or
  the narrowly recognized no-notification-area hierarchy described below;
  refuse unreadable and unknown layouts.
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
  Admission is now parked pending the cursor-first matrix; a later fixture
  must use physical monitor bounds and correlate genuine focus/retention.
- Reported transfer trace shows size scaling near the monitor DPI ratio and
  subsequent reverse scaling. Exact arrival verification correctly refused
  the mismatched frame. A separate PMv2 owned-window probe confirmed that a
  DPI transition changes frame insets: the first physical placement missed
  slightly; one identical placement with fresh insets matched exactly.
  This does not explain the much larger Firefox scaling or prove product-path
  correction. No transfer fence or geometry tolerance was changed.
- Secondary pager root cause: discovery enumerated secondary taskbars but
  supplied no tray edge, and layout refused every secondary. Discovery now
  gathers child facts, live DPI and auto-hide state. A pure Windows resolver
  prefers the leftmost valid exact tray/clock edge, otherwise recognizes the
  bounded no-notification-area hierarchy. Any unreadable fact, degenerate
  exact anchor, unknown visible child, unrecognized full-span carrier,
  right-side content or missing positive app-host evidence refuses.
- Secondary pager independent review caught unreadable/vacuous-census
  acceptance and overly broad full-span exemptions; these were corrected
  with pure producer-shaped regressions. Exact-anchor enumeration order is
  also covered: a clock cannot mask a farther-left tray boundary.
- Final-tree native pager smoke (2026-10-20): two visible, topmost carriers
  above the taskbars, correctly DPI-sized with the required two-physical-pixel
  gap. Alternating secondary-2, primary-2, secondary-1 message-driven picks
  selected three distinct output-scoped workspace identities successfully.
  The smoke used empty workspaces: it does not prove occupied-window hide or
  physical mouse delivery. Retained artifact/source-diff identity, raw owner
  log, geometry, stdout and restoration readbacks were inspected. The driver's
  PMv2 bar/carrier geometry is authoritative; separate wrapper monitor
  snapshots were DPI-virtualized and are not physical-coordinate evidence.
- Live sessions ended with verified graceful stop, independent restoration,
  byte-identical settings, unchanged relevant SPI/taskbar state, absent ledger
  and no owned window residue. Detailed evidence remains external.
- Final isolated workspace-routing gates (2026-10-20), Rust/Cargo 1.99.0,
  native MSVC: locked four-package build/tests, workspace format, strict
  all-target clippy and whitespace passed: 2,836 passed, zero failed, one
  ignored (core 1,042; protocol 27; FFI 40; Windows 1,727).
- Final pager gates (2026-10-20), same native route/toolchain: locked
  four-package build/tests, format, strict all-target clippy and whitespace
  pass: 2,849 passed, zero failed, one ignored; independent review accepted.

## Remaining units

1. Wait for Tier 1 cursor-first output-targeting matrix evidence and the
   R-OUT-05 conflict resolution before another admission implementation;
   then re-establish physical-coordinate fixtures and reliable focus sampling.
2. Diagnose PMv2 cross-DPI placement with owned disposable windows; preserve
   exact arrival fences and add a regression for any demonstrated correction.
3. Complete secondary pager physical selection, occupied-workspace,
   Explorer-restart and display-change acceptance (W2M-13 remains partial).
4. Run full native gates and bounded checklist legs; park physical-only,
   fixture-dependent and explicitly blocked rows without inferred passes.

## Decisions and limits

- User 2026-10-20 direction supersedes the tentative foreground, pointer,
  last-switch cache, stable-order policy and "admission never pointer" claim:
  [cross-platform cursor-first targeting](../decisions.md#cursor-first-output-targeting)
  is pending Tier 1 functional-matrix evidence, not an approved replacement for
  R-OUT-05. Interim foreground-first commands (`1e7bfaf4`) stay; admission waits
  on the matrix, including the definition of standard new-window location.
- User 2026-10-20 accepted: secondary bars without a notification area
  may anchor at bar-right minus the existing gap only for the observed narrow
  HWND hierarchy: readable descendants, positive visible left app-button
  host, exact full-bar composition bridge and only zero-area input sites or
  recognized left launcher content. Unknown visible hierarchies refuse.
  HWND-less composited pixels are not observable through this census; this
  is bounded shell-topology support, not a universal visual exclusion proof.
  A quick physical check looked good; broader physical acceptance remains pending.
- Parked admission prototypes used an HWND-only freshness hint; a successor
  must retain the existing full identity/tag fences and avoid treating an
  enumerated-but-memberless hidden window as genuine established focus.
- Physical shortcuts, monitor changes/hotplug, shell restart, sleep/wake and
  crash recovery have not been accepted by the current journeys.
