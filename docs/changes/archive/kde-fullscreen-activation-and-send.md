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

## Backlog detail moved 2026-11-01

### Original backlog lines 1345-1388

````markdown
- P1 | 0.1 triage decisions - implementation and verification (KDE/shared) |
  D01, D03, D09, D10, D12, D13, D16, D22, D23, D25, D26, D27, D28 approved by
  User 2026-10-12. Historical gap assessment at `fafcd31`, D01 refined by
  [research](research/d01-fullscreen-admission.md) at `01be284`. D01 and D28
  are delivered offline ([record](changes/archive/kde-fullscreen-activation-and-send.md));
  native acceptance remains pending. [Selections and caveats](research/release-0.1-triage.md#user-decisions-2026-10-12).
  Verification-only means a matching mechanism exists, not native acceptance.
  Approved units include contingent fixes if live outcomes differ, subject to
  their recorded return-to-user conditions.
  Paths below are repository-relative; line locators are revision-bound.

  | Unit | Gap assessment | One-line reason / source |
  | --- | --- | --- |
  | D01 | delivered offline; native verification pending | KDE automatic convergence re-observes before focus and fences overlay/native-focus races; admission retains slots/state and explicit switching/shortcuts remain effective. Shared fixtures verify host-approved/denied admission and exact CLOSE-05 removal/refill/focus. Joint native admission/overlay-close cleanup pending ([record](changes/archive/kde-fullscreen-activation-and-send.md)). |
  | D03 | verification-only | After-focused `[old,new]` and per-admission focus anchor exist (`crates/tiler-core/src/session.rs:2218-2231`, `crates/tiler-core/src/session/ops/lifecycle.rs:374-384`); chained legs 2-3 need live pointer/focus proof. |
  | D09 | verification-only | R-MOV-06 two-child neighbor uses middle index `len/2` (`crates/tiler-core/src/directional.rs:907-918`, `crates/tiler-core/src/session/ops/move.rs:897-905`); exact native fixture pending. |
  | D10 | verification-only | Orthogonal escape continues through R1 wrap (`crates/tiler-core/src/directional.rs:1003-1007`, `crates/tiler-core/src/session/ops/move.rs:1089-1140`). |
  | D12 | verification-only | Same-axis escape inserts into the ancestor axis (`crates/tiler-core/src/directional.rs:993-1002`, `crates/tiler-core/src/session/ops/move.rs:1080-1087`). |
  | D13 | verification-only | Local move wins before output crossing (`crates/tiler-core/src/directional.rs:1046-1088`); OUT-02 local-wrap multi-output fixture remains live-pending. |
  | D16 | delivered offline on KDE and Windows; native verification pending | Ordinary and floating+fullscreen/maximized sends use slotless MoveDeferred; float frame/intent retained, no geometry/overlay writes or source/destination tile reflow. Windows adapter delivered 2026-10-13 with drifted-survivor application suppression and real entry/Engine regressions; native combo carry/exit remains pending. Sticky/automatic-fixed refusals unchanged; no setting. [Shared/KDE record](changes/archive/d16-floating-transfer.md), [Windows record](changes/archive/windows-d16-floating-transfer.md), [decision](./decisions/workspaces.md#workspace-lifecycle-focus-and-history). |
  | D22 | approved; verification-only | Float-out preserves survivor proportions (50/25/25, float middle 25 -> approximately 67/33); no setting. Removal retains survivor share ratios (`crates/tiler-core/src/cosmic_v1.rs:175-198`, `crates/tiler-core/src/session.rs:2398-2399` at `fafcd31`, revision-bound). COSMIC observed, not source-proven, plus i3/sway Tier 1; thin i3/sway one-family basis disclosed; no evidenced alternative, Hyprland/Tier 2 untraced. Native reflow live check pending. [Decision](./decisions/window-state.md#fullscreen-and-maximize-overlays). |
  | D23 | delivered offline on KDE and Windows; native verification pending | COSMIC `3d55cba0` source gate passed. Shared best-effort neighbor rejoin and additive `closed_windows` serve both adapters. Windows delivered 2026-10-13: retained-allocation divergence replaced with release/reflow/MRU, no-focus restore, sole null focus and hidden-domain close cleanup; session-only. Native gates 1,845 passed, 0 failed, 1 ignored; native journeys pending. [Shared/KDE evidence](changes/archive/d23-rework.md), [Windows evidence](changes/archive/windows-d23-restore-close.md), [decision](./decisions/window-state.md#fullscreen-and-maximize-overlays). |
  | D25 | approved (a); verification-only | Keep host-owned set; re-observe host membership, no moves; focus only where provable; rebuild layout by re-adoption, no saved-layout restore; intentional floats stay floating via existing restart store; no setting. Generic fresh snapshot/rebuild/converge supports membership/provable focus (`kwin/src/plan-adapter-entry.ts:2186-2300`, `crates/tiler-core/src/session/world.rs:746-799` at `fafcd31`, revision-bound). Native dogfood/live checks will assess orderly restart; outcomes pending. [Decision](./decisions/persistence.md#orderly-restart-adoption). |
  | D26 | approved (b); verification-only, no code | Restore already-persisted owned gaps/settings; re-observe host set/membership/maximize, no invented layout restore. Gaps/settings reload exists (`kwin/src/domain-gap.ts:90` at `fafcd31`); intent cannot cross login/bus/KWin namespace (`crates/omnitiler/src/float_intent_store.rs:11,30-34`). Listed 0.1 known limitation: intentional-float memory lost on logout/login, reopened apps new identities; unchanged REQ-RST-01d lifetime. Revisit after 0.1 with shared restart store. Native dogfood/live checks will assess; outcomes pending. [Decision](./decisions/persistence.md#orderly-restart-adoption). |
  | D27 | KDE verification-only; no longer BLOCKED. Windows partial verification; approved 5b implementation pending via P2 Activation request setting | User 2026-10-13 accepts native KWin host exception: valid-token/FromTool/allowed-X11 switches + focuses, missing/stale-token demand-attention; converge without intercept/revert/suppress. No KDE gaming guard; rely on native input freshness. Real-game active and idle/paused check must pass; reopen only on actual active-play switch-away. Windows 2026-10-14 `f4b9f41`: fresh background current/hidden calls denied with no switch/focus change; shell flash observed, visible urgency unproven. Previously attached caller converges; no product classifier/marker. User-approved 2026-10-16 Windows 5b: OS-permitted Switch and focus over fullscreen/games, no urgency-only downgrade; default Mark urgent only/non-permitted unchanged. Not yet implemented; User 2026-10-17 folds the standalone 5b implementation into the planned post-0.1 `Activation request` setting item (P2 0.1 triage settings follow-up). Clean permitted route, mark/clear and game evidence TBD ([Windows evidence/options](changes/archive/windows-d27-activation-verification.md)). Setting KDE applicability deferred to design ([research](research/d27-kwin-activation-trace.md); [decision](./decisions/activation-requests.md#hidden-window-activation-default)). |
  | D28 | delivered offline, including born-fullscreen option (a); native/game verification pending | Explicit numbered/relative follow/stay carries retained fullscreen tiles and never-tiled slotless members. Shared MoveDeferred rehomes the initial hold without tree mutation, source admission or geometry setters; D5 first exit admits on the destination. Fullscreen flag and existing send fences retained. Native KDE journey including a real game is must-pass; stop if KWin drops/corrupts fullscreen. No setting; native outcome TBD ([record](changes/archive/kde-fullscreen-activation-and-send.md); [decision](./decisions/window-state.md#fullscreen-and-maximize-overlays)). |

  - Native verification queue (all units): no live acceptance claimed. Pending
    native journeys: R-WS-05 roundtrip, numbered/relative follow/stay,
    sole-window/empty-target, float/overlay flags and same-output frame (D16);
    float-out reflow shares (D22); best-effort rejoin journeys, internal slot
    retirement and sole-iconic retention (D23; two agent-live sweeps passed
    minimize/reflow/restore and close variants); permitted/mark/clear and game
    outcomes (D27; 5b implementation pending via the P2 `Activation request`
    setting item); orderly owner restart and end-session restore (D25/D26);
    joint native admission/overlay-close cleanup and fullscreen-send game journey
    incl. born-fullscreen sole-held-source/empty-target legs (D01/D28).
    Second agent-live D16 sweep: numbered ordinary-float roundtrip and
    fullscreen stay-carry passed; a small-frame destination-exit defect was
    fixed ([record](changes/archive/windows-floating-fullscreen-frame-restore.md)).
    Shared-core fixes serve Windows via the handoff below; Linux work owns
    KDE/core. D23 source slot: User decision 2026-10-17 option B (forget on
    confirmed rehome; shared-core implementation approved, not implemented;
    [proposal](changes/shared-core-d23-rehome-and-13f-maximize-base.md)).
````
