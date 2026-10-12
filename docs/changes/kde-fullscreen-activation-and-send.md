# KDE fullscreen native activation and explicit workspace carry

## Goal and scope

- Deliver approved D01 / REQ-INS-06 native activation ownership and D28 /
  REQ-MAX-09b explicit fullscreen workspace carry offline.
- Retain overlay slots and native fullscreen/maximize state; let KWin own
  automatic admission activation and stacking. Explicit focus shortcuts and
  workspace-send follow/stay semantics remain effective.
- Verify CLOSE-05 removal, survivor refill and focus without changing its policy.
- Native visual/input acceptance, including fullscreen games, remains pending.
  No settings, unrelated activation-request policy or broad layering refactor.

## Acceptance and approach

- Fence divergent automatic KDE focus writes when fullscreen is involved,
  including stale replies and native focus changes during application. Preserve
  native activation ownership for admission over maximized windows as well.
- Explicit numbered/relative follow/stay sends carry retained fullscreen tiles
  to the destination without clearing fullscreen or writing overlay geometry.
- Never-tiled born-fullscreen members also carry, preserving their slotless
  initial hold without source/target reflow. First fullscreen exit admits them
  on the destination under D5, never on the source before transfer.
- Preserve existing observation, identity, flag-stability and arrival fences.
- Prefer existing shared policy: convergence already adopts observed focus and
  workspace transfer already retains tiled allocations. Add host-specific
  actuator guards without adding shared native stacking semantics.
- Check pinned KWin desktop-assignment source before implementation; stop if it
  demonstrates fullscreen carry drops or corrupts the native state.

## Work units and verification

1. Trace focus/send paths and native desktop assignment; identify regression
   fixtures and the smallest implementation boundary.
2. Add failing KDE regressions, implement actuator guards and carry, and add
   shared Rust fixtures for retained slots, sends and overlay close.
3. Review the actual diff and evidence; run Rust workspace tests, strict
   clippy/fmt, KWin tests/typecheck/bundle, and applicable changed-component
   gates. Update requirement/reference cells and backlog gaps.
4. Record accepted evidence and pending native journeys; archive on completion.

## Decisions

- D01 and D28 were approved 2026-10-12. Explicit send-follow is an explicit
  user action, not unsolicited convergence focus.
- The fullscreen fence must not disable explicit focus shortcuts. Maximized
  admission ownership does not change unrelated close/refocus behavior.
- Whole-workspace migration is outside this single-window send change.
- User clarification 2026-10-12 option (a): slotless born-fullscreen carry is
  included, with no source admission, no reflow, and D5 exit on destination.
- The additive deferred-transfer extension is selected: shared window/source/
  target identities without a leaf, exception-member send/focus without tree
  mutation, and KDE native membership movement under existing follow/stay
  semantics. Preserve existing tiled contracts; Windows changes are compile-only.

## Evidence and outcome

- Initial source assessment: retained slots and overlay geometry guards exist;
  divergent KDE plan focus application lacks a fullscreen fence. Shared Rust
  workspace transfer has no fullscreen-specific refusal. Native acceptance TBD.
- D01 guards now re-observe the foreground after geometry application and
  suppress automatic focus over fullscreen/maximized overlays, unavailable or
  changed domains, and newer observed native focus. Explicit focus and send
  semantics remain effective. Regression probes fail without the guard.
- D28 observer and arrival gates now admit flag-stable fullscreen tiled movers;
  geometry skips both source and destination overlays. Restoring the previous
  entry refusal makes the four numbered/relative follow/stay entry tests fail.
- Seven shared Rust fixtures cover host-approved/denied admission with retained
  overlay slots, fullscreen send follow/stay, and exact CLOSE-05 survivor refill
  geometry and focus. Rust production policy is unchanged.
- Latest gates: Rust workspace 1493 passed, KWin JS 1348 passed across 195
  suites, strict workspace clippy, fmt, KWin typecheck/bundle and whitespace
  checks passed. No native or shell source changed. Native acceptance pending.
- Pinned KWin v6.6.3 source: [Window::setDesktops](https://github.com/KDE/kwin/blob/v6.6.3/src/window.cpp#L711-L758)
  and [X11Window::doSetDesktop](https://github.com/KDE/kwin/blob/v6.6.3/src/x11window.cpp#L1506)
  do not clear fullscreen. No source evidence requires a carry workaround.

## Deferred-transfer extension

- The general D28 fullscreen-send target includes no retained-slot qualifier.
  A never-tiled born-fullscreen member currently remains a slotless exception:
  `PlanAdapter.withInitialFullscreenHold` sends a synthetic floating admission;
  `Session::converge_observation` does not retile a still-fullscreen exception
  with fixed-size admission enabled. Send focus synchronization requires a tiled
  link and therefore rejects it with `FocusMismatch` before any native setter.
- The new real-Planner send fixture starts a fresh Engine without the prior
  synthetic hold. Its seed path tiles the fullscreen member before transfer;
  it does not prove sending a production-held born-fullscreen member.
- The never-tiled leg is now selected: carry while preserving the hold, with
  no source admission or source/target reflow. Native game acceptance remains TBD.
- Source inspection confirms a missing additive operation: `LifecycleOperation`
  has deferred admission/removal but no deferred transfer. `SendWorkspacePlan`
  and send wire serializers require `MoveTiled` and a real leaf; the slotless
  member cannot use that contract without inventing a leaf. Convergence alone
  rejects changed exception homing with `CrossDomainMismatch`.
- A temporary shared-core probe confirmed slotless focus synchronization and
  transfer refusal. No production-path hold/send/exit fix or new wire operation
  has been implemented. Retained carry and D01 work remain available for review.
- Selected minimal extension: a deferred-transfer
  operation with window/source/target identities and no leaf, shared exception
  homing/focus handling without tree mutation, and explicit adapter membership
  and follow/stay handling. Preserve existing tiled-send serialization and
  lifetime/identity/flag/arrival fences; no KDE-only transfer bypass.
