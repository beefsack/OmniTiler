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
  Both Settings identities opened in the native shell; interactive Settings
  acceptance is incomplete. The initial fixture drift has a fresh recorded
  baseline. No production change or product defect has been established. The
  full-state breadth pass found two float regressions to investigate and repair;
  the remaining matrix is pending.

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
  is tentative, pending user review. Unverified Settings, full-state and
  single-output cases retain their previous pending status.

## Settings native shell re-check, 2026-10-18

- Host-matched builds produced both KCM plugins. Both exact identities opened
  through `kcmshell6` using process-scoped plugin staging, without installation
  or session restart. A follow-up script-KCM run independently verified that the
  source-built staged plugin was mapped into the owned shell process. The
  earlier effect-KCM run retained no mapping proof, only scoped launch and native
  window/render evidence.
- First-page rendering showed the four expected tab titles in order, tiling
  controls, gap defaults from absent keys, the Status panel below that page and
  disabled Apply. This does not establish traversal or Status visibility on the
  other three tabs.
- Live keyboard traversal, label buddies, tab switching, cross-tab drafts,
  Apply/reopen, Cancel, Defaults-without-save and ordinary-Apply operation
  isolation were not tested: no safe desktop input route was available. These
  remain pending, not inferred from offline coverage.
- Settings-only build and CTest passed **27/27** on the same source. These are
  offline/application-local results, not native-shell interaction acceptance.
- Settings values were not changed. The capture tool briefly added its own
  shortcut-service entry; the exact preimage was restored before the next leg.
  Final configuration and shortcut bytes matched the accepted baseline;
  protected windows, desktops, pointer, effects and DOWN lifecycle matched, and
  all owned Settings windows were gone. No product defect was found.

## Full-state breadth observations, 2026-10-18

- Native callback/state checks passed for ordinary admission, focus, movement,
  resize, tiling off/on, float/sticky/fullscreen/minimize safety, maximized-slot
  reservation during workspace enable and native restore, Planner-only restart
  preserving an ordinary float frame, unavailable-Planner quietness, generation
  rediscovery and close/reflow. These are sampled legs, not completion of every
  checklist permutation or physical input/rendering acceptance.
- A command spanning Planner replacement was rejected for generation mismatch
  and was not replayed. Fresh-generation traffic and subsequent actions resumed.
- Defect: maximized intentional unfloat issued its native clear, but synchronous
  read-back still observed maximize and refused the command. The native clear
  settled later; a second activation re-tiled. Investigate event-driven settlement
  under the existing B9 contract, with one native clear per activation and no
  unsafe command replay. Add asynchronous producer-shaped regression coverage.
- Defect: an ordinary float adopted after Planner restart accepted two explicit
  toggles without geometry change. Exact divergent core/adapter behavior is not
  yet isolated; reproduce through the real producer/reply boundary before fixing.
- Native maximize/restore alone produced no world traffic in this sample. A
  single unexplained tile-order change also needs attribution; neither is yet an
  independently reproduced product diagnosis.
- Fixed-client/game legs, physical held keys, sends/migration/drag and workspace
  lifecycle races were not exercised in this unit. No admission/capacity change
  was made.
- Both disposable clients and helpers were removed; controller and Planner were
  stopped. Configuration and shortcut bytes, desktop identities/count, protected
  frames/membership, current/active window and pointer matched the accepted
  baseline. No new baseline or configuration rollback was required.

## Maximized-unfloat correction

- Replace same-stack maximize read-back with one deferred decision on the first
  fresh accepted full-state report after the native clear. Observed clear submits
  one ordinary core unfloat; unconfirmed clear ends the attempt. No new timer,
  repeated setter or alternate tiling policy was added.
- Scope, lifetime, focus, overlay, gesture and busy checks fence the continuation.
  Newer explicit input and terminal report errors cancel it; generation mismatch
  cancels before rediscovery, so a deferred command cannot cross Planner restart.
- The initial implementation waited indefinitely for a native signal. Review
  rejected its unbounded wait, generation survival and incomplete supersession
  fences. The causal correction uses the single-report boundary above; follow-up
  review accepted those repaired conditions. No rejected implementation was
  shipped.
- Producer-shaped tests use QML array-likes, renewed wrappers, braced native IDs,
  real property names and asynchronous settlement. Against the previous source,
  12 new cases failed and the ordinary-unfloat control passed; all 13 pass after
  correction. Silent/refused clear, races, newer input, generation rebind and
  transport failure are covered.
- Final offline gates: KWin **1794 passed, zero failures across 268 suites**,
  source/test typechecks and bundle build; Rust workspace **3017 passed, zero
  failures** with `--offline --locked`; whitespace check clean. Native re-check
  of the corrected maximized-unfloat path is next.
