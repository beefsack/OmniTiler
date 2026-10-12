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
  geometry and focus. D01 and retained carry passed their offline gates before
  the slotless extension (`884f330`, `1a60765`).
- Final gates: `cargo test --workspace` 1538 passed, `npm test --prefix kwin`
  1365 passed across 198 suites, workspace strict clippy, fmt, KWin typecheck/
  bundle and whitespace checks passed. No native or shell source changed;
  corresponding changed-component gates were not needed. Native acceptance
  remains pending.
- Pinned KWin v6.6.3 source: [Window::setDesktops](https://github.com/KDE/kwin/blob/v6.6.3/src/window.cpp#L711-L758)
  and [X11Window::doSetDesktop](https://github.com/KDE/kwin/blob/v6.6.3/src/x11window.cpp#L1506)
  do not clear fullscreen. No source evidence requires a carry workaround.

## Deferred-transfer extension

- The retained-carry fixture's fresh Engine seed did not exercise the real
  synthetic initial hold. Investigation reproduced `FocusMismatch` for a held
  slotless member, motivating the clarified born-fullscreen option (a).
- Shared `MoveDeferred` now carries window/source/target identities without a
  leaf, rehomes the held exception and leaves both trees unchanged. Existing
  tiled send operations/encoding remain intact; no Windows source change was
  required. Plain intentional-floating/sticky send eligibility is unchanged.
- Send observation carries the focused mover's fullscreen flag so shared
  convergence preserves its initial hold; survivor encoding stays unchanged.
  Shared eligibility binds the retained exception and raw native flags; KDE
  checks exact operation/scope/coverage and captured mover flags, then proves
  native arrival before existing follow/stay actuation. Empty source/target
  geometry is supported only for the new operation.
- Deferred sends execute no geometry setters, even when projected survivor
  frames differ from native frames. Native desktop membership moves once; the
  mover remains fullscreen and slotless until D5 exit admits at the destination.
  Ordinary drift reconciliation stays separate from the explicit send.
- Follow retains target tiled MRU/first-tile focus (or null for an empty target)
  as shared bookkeeping; native follow addresses the mover. Stay uses source
  MRU. An initial null-only follow approach broke later tiled reconciliation;
  the resolvable bookkeeping fallback fixes that. A stripped mover fullscreen
  flag retiled the held member before send; mover-only wire propagation fixes
  it without changing survivor encoding.

## Accepted regression evidence

- `crates/tiler-core/tests/slotless_deferred_send.rs` verifies unchanged trees,
  exception homing, follow/stay bookkeeping, eligibility refusals and destination
  first exit. `crates/tiler-protocol/tests/slotless_deferred_send.rs` uses one
  retained Planner through the synthetic hold, send, destination hold and D5
  exit; it also covers sole-held source/empty target and cross-output variants.
- `kwin/tests/workspace-send-slotless-production.test.ts` uses real PlanAdapter
  initial hold and a real send entry sharing one Planner, for follow and stay.
  Instrumented native property setters prove exactly one membership write and
  zero geometry/fullscreen/maximize writes during send. Destination hold and
  source cleanup succeed; first exit admits at the destination.
- Disabling shared deferred eligibility makes both production journeys fail
  with `focus-mismatch` before setters (`slotless-prod-red.log`); restoring it
  makes both pass. Deferred adapter fixtures verify empty geometry, drifted
  survivors, cross-output follow/stay, flag races, and malformed leaf/scope/focus
  replies. Existing entry fixtures cover numbered/relative target resolution;
  diagnostic ordinals alone are not claimed as relative-resolution evidence.
- Review identified and corrected deferred empty-geometry rejection, equality-
  only geometry skipping and a missing deferred mover-shape fence. The corrected
  diff was accepted; no remaining offline acceptance blocker.
- Final typechecking caught two fixture typing errors (nullable active ref and
  untyped operation access). A fixture-only correction restored typechecking;
  the full KWin suite passed again with unchanged product behavior.
- Final evidence logs: `slotless-p3-gate-test2.log`,
  `slotless-final-kwin.log`, `slotless-p3-gate-clippy.log`.

## Outcome and pending native acceptance

- D01 and D28 are complete offline, including slotless born-fullscreen carry.
  Requirement/reference Ours cells and backlog gaps reflect delivered code,
  while native outcomes remain TBD.
- Compare tiler-off/on Wayland and Xwayland admission over maximized/fullscreen
  B with host-approved and denied activation; verify Alt+Tab/taskbar and explicit
  shortcut switching without clearing fullscreen. Check overlay-close visuals,
  survivor refill and input/focus cleanup.
- Must-pass D28: a real fullscreen game, retained and never-tiled, fresh
  numbered/relative follow/stay journeys; verify flag, source/target membership,
  view/focus, geometry writes, pixels and input. Include sole-held source/empty
  target and first fullscreen exit on the destination. Stop and return to the
  user if native KWin drops or corrupts fullscreen.
- Cross-output deferred wire and adapter tests pass; real combined-entry
  cross-output integration and native multi-output/game acceptance remain pending.
  The adapter checks fullscreen shape; authoritative slotless topology remains
  shared-core-owned, without a new host provenance schema.
