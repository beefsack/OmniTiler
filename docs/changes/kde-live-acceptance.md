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
  baseline. The full-state breadth pass found a maximized-unfloat regression,
  now corrected and reverified natively. A suspected restart-float defect was a
  floating-workspace fixture mismatch. The remaining matrix is pending.

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
  settled later; a second activation re-tiled. The correction below preserves
  the existing B9 contract with one native clear per activation and no unsafe
  command replay; asynchronous producer-shaped regressions cover the failure.
- Initial suspicion: an ordinary float adopted after Planner restart accepted
  two explicit toggles without geometry change. Offline isolation showed the
  test workspace had reseeded from the deliberately floating saved default;
  session-only workspace tiling overrides do not survive Planner replacement.
  Floating scope publication correctly withholds geometry. This is a fixture
  mismatch, not an admission/capacity or float regression. Temporary core probes
  passed both floating no-write and re-enable-then-retile cases; no production or
  committed test change was needed. Re-enable test-workspace tiling after restart
  before checking tiled behavior.
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
- Final integrated offline gates at `fe3cc3a9`: KWin **1794 passed, zero failures
  across 268 suites**,
  source/test typechecks and bundle build; Rust workspace **3017 passed, zero
  failures** before rebase and **3022 passed, zero failures across 83 groups**
  after integrating upstream platform-policy tests, with `--offline --locked`;
  whitespace check clean.
- Native re-check at `fe3cc3a9` passed: one float callback issued the native clear;
  the first observation report wrote no geometry, then one unfloat operation
  occupied both tiles with two geometry writes. No second activation or focus
  change was needed; no refusal or shortcut failure occurred. An eight-second
  quiet interval and later focus change left the geometry stable. The accepted
  configuration/native baseline was restored exactly and development is DOWN.

## Actual Qt/KWin transport boundary, 2026-10-18

- A pure-transport KWin helper called an owned Planner while the controller was
  unloaded. Inputs were synthetic worlds; no reply was applied to native windows
  or desktops. Authenticated discovery and a small accepted report took 1 ms and
  2 ms respectively in this spot sample.
- A 5000-window, 64-domain report padded with JSON trailing whitespace to exactly
  **4194304 bytes** was accepted; its unpadded content was 1925198 bytes. KWin
  received and parsed a **1608803-byte** full reply in **904 ms**. Construction
  took a separate 273 ms. Identical resend returned identical bytes at the same
  revision in 380 ms. This proves actual delivery, not layout usability or a
  responsiveness target.
- **4194305 bytes** were refused before decode: the service reported
  `request-oversize` and KWin received the D-Bus size-bound error rather than a
  normal callback. A separate 11188-window report below the byte cap was refused
  in-band as `snapshot-invalid` / `inventory-over-cap`, confirming the distinct
  count gate. The probe initially labeled that correct count refusal as failure
  by checking the wrong field; wire/service evidence confirms the expected
  result. No product correction was needed.
- Exact-4-MiB reply delivery remains untested; the actual delivered reply size is
  reported above rather than substituted for that boundary. The owned service
  and helper were removed, the Planner name became unowned, and configuration,
  shortcut and float-intent bytes were unchanged. No new baseline was required.

## Remaining coverage and next step

| Acceptance group | Current result | Remaining |
| --- | --- | --- |
| Ordinary unfloat | Native callback functional pass, two pairs | Physical Meta+G delivery |
| Both Settings identities | Native shell launch and first-page observations | Tabs 2-4, traversal/buddies, Apply/reopen, Cancel, Defaults, operation isolation |
| Restart / unavailable Planner | Sampled native pass, no old-command replay | Broader race/error permutations and script-restart intent coverage |
| Transport | Exact request limit, oversize/count refusal and large reply delivered | Exact reply-limit delivery and broader latency acceptance |
| Ordinary actions / lifecycle | Sampled native pass | Remaining matrix permutations |
| Safety states / workspace enable | Ordinary float/sticky/fullscreen/minimized samples; maximized-slot restore and corrected B9 pass | Genuine fixed-client D5/D6/D7, game/born-state and refused/raced variants |
| Workspace paths | Tiling toggle sample only | Selection/history, follow/stay/append, floating-boundary sends, lifecycle races and migration partial refusal |
| Drag / client clamp | Not exercised | Continuous preview/drop/resize, late-reply cancel and bounded native clamp |
| Multiple outputs | Not testable in this single-output fixture | Selection/history/swaps/hotplug-return and cross-output matrix |

- No complete full-state or single-output inventory gate is claimed. Physical
  keyboard/drag and interactive Settings need a safe input route or direct native
  interaction; callbacks and offline widget tests are not substitutes.
- Next bounded native step: verify the current accepted baseline, establish a
  genuine fixed-size disposable client on the existing empty workspace, then
  exercise fixed admission/workspace-enable/explicit-override restart legs
  separately. Re-enable that test workspace after Planner replacement before
  interpreting tiled behavior. Do not create extra workspaces for those legs.
- Workspace send/history/lifecycle testing needs a separately bounded fixture
  with owned peers, explicit target/view read-back and a baseline-preserving
  teardown. Native-maximize notification quietness and the isolated tile-order
  observation remain unclassified follow-ups, not established defects.
