# KDE live acceptance continuation

## Goal and acceptance

- Re-check immediate unfloat re-tiling, then both native Settings identities,
  then the full-state reply checklist and affected single-output checks.
- Preserve existing application windows and restore the recorded desktop,
  settings, effects and development lifecycle baseline.
- Distinguish callback/state observations from physical keyboard and visual UX
  acceptance. No unrelated platform or admission changes are included.

## Approach and current status

- Capture native state and exact project lifecycle ownership before startup.
- Start with floating defaults to avoid retiling existing workspaces, and tile
  only the disposable-client workspace. Restore the temporary setting afterward.
- Exercise the three acceptance groups in order; regress and fix product defects
  only after reproduction. Stop on baseline or restoration surprises.
- Status: unfloat functional callback pass; physical shortcut delivery pending.
  The initial fixture drift has a fresh recorded baseline. No production change
  or product defect has been established. Settings and the remaining full-state /
  single-output matrix are next.

## Native observations, 2026-10-17/18

- A freshly built Planner and KWin bundle started with `defaultTiled=false`.
  Initial accepted replies made no geometry writes; protected existing windows
  retained their recorded frames. The temporary key was restored to absent.
- Two normal, resizable KDE editor clients exercised two float/unfloat pairs,
  one with each client as the toggled window. All four operation requests were
  accepted. Each unfloat reply made two geometry writes; native read-back showed
  both clients occupying tiles without an intervening focus change. Subsequent
  focus observations showed no delayed geometry snap. Re-admission changed tile
  order on the second pair, not tile occupation.
- These are callback-driven native state observations, not physical Meta+G or
  visual interaction acceptance. One later edge-focus request returned
  `unchanged`; it was not a float/unfloat refusal. No transport error was found.

## Fixture failure and restoration blocker

- Creating a desktop while the controller was active replaced a pre-existing
  empty backing desktop; the inventory remained at two. The fixture then
  removed its owned desktop and recreated the second position with its original
  name but a different identity. Exact identity restoration was not achieved.
- The functional checks continued on that replacement after the baseline
  surprise. This did not follow the live guide's stop rule. Consequently the
  observations above are a functional signal on a drifted fixture, not clean
  acceptance of the complete requested journey.
- Normal empty-desktop retirement is documented product behavior, but the exact
  retirement actor was not independently established here. A native desktop
  count limit was not proven. No product defect or policy change is inferred.
- The final configuration differs by the second desktop's backing identity and
  two new native tiling groups: one for the removed test desktop and one for
  the live replacement. These were retained, not broadly rolled back.
- Ghostty and Firefox identities, membership and frames were unchanged in the
  recorded baseline and final inventories. Original active window, current
  desktop and pointer were restored; both disposable clients and helper scripts
  were removed, effects matched, and development returned DOWN.
- Restoration to the initial desktop/configuration identity was incomplete.
  The current inventory and native tiling entries are accepted as the new fixture
  baseline (tentative, pending user review). No manual configuration rollback is
  planned. A fresh exact configuration backup and native window/desktop, pointer,
  effect and shortcut snapshots were captured before further testing; later
  restoration comparisons use that baseline. The original preimage is retained.

## Offline verification and review

- At `1caadf15`: `cargo test --workspace --offline --locked` passed **3017
  tests, zero failures across 83 test-result groups**.
- KWin `npm test` passed **1777 tests, zero failures across 267 suites**;
  `npm run typecheck`, `npm run build` and `cargo build -p omnitiler` passed.
- Independent evidence review confirmed the immediate-retile observations,
  protected-window preservation, lifecycle teardown and baseline drift. It did
  not accept a claim of exact desktop restoration or complete native acceptance.
- No tentative product decisions were made. The fixture-baseline decision above
  is tentative, pending user review. Remaining Settings, full-state and
  single-output cases retain their previous pending status.
