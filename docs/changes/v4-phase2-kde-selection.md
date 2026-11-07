# V4 phase 2: KDE selection

## Goal and status

- Port workspace selection and its acknowledgement/correlation to the native
  KDE adapter while retaining K1 ordinary tiling. Status: direct
  resize/orientation repair passes offline gates; selection native proof and
  open-binding verification remain pending.
- Start from the [K1 closeout](archive/v4-phase2-kde-port.md#k1-final-outcome).
  The [phase-1 note](v4-phase1-core-protocol.md) remains active and unchanged.
- First repair resize and toggle-orientation emitters to use the existing direct
  v4 commands. The codec already accepts both; no codec change is required.

## Scope and non-goals

- Product decision: this slice covers native selector/readback, exact original
  view-attempt correlation and focusless terminal proof. Focus-bearing terminal
  completion stays in the arrival/focus-feedback slices, preserving their order.
  Core completion is unchanged; no acknowledgements or feedback are fabricated.
- Retain the authenticated v4 connection, native observation fences, opaque
  intent storage, core-owned policy and ordinary tile/reflow path.
- Arrival, focus feedback, state targets, scaled metrics, reconnect/rehome,
  visible active border and legacy removal remain separate later slices.
- Immutable wire and legacy fixtures remain unchanged. No C++ presentation DTO
  or script-side absolute border geometry change is planned.

## Approach and bounded units

1. Repair both resize/orientation emitter paths and add direct-v4 regressions.
2. Audit selection against the [contract](../porting/world-contract.md), then
   implement only missing native selection and acknowledgement mechanics.
3. Run the full adapter and scoped Rust gates, then prove selection and continued
   ordinary tiling using fixture-scoped native state and correlated full wire.
4. Resolve one independent integrated review, record exact capabilities and
   remaining degradation, and archive only after acceptance.

## Acceptance and verification

- Direct resize/orientation payloads reach the genuine v4 core without a retired
  operation wrapper; preserve resize press accounting and existing safety gates.
- Selection reaches the intended native desktop with authoritative native
  observations and correlated view attempts/terminal events, not setter success.
- Ordinary fixture tiling and reflow continue across selection. No new skips.
- An open populated-selection binding must not block later selections, commands,
  capture/reply flow or tiling. Retention must remain bounded under repeated
  populated switches; missing/pending requester outcomes are reported exactly.
- Sequential declared-environment adapter typecheck/build/full tests and widened
  scoped Rust tests/build/clippy/formatting remain green; unsafe workspace tests,
  ignored forcing and legacy fixture generation are excluded.
- Native work follows the [live guide](../live-kwin-testing.md) and
  [fixture reference](../../crates/tiler-live-fixture/README.md): fresh occupied
  workspace inventory, fixture-only filter, evaluated readiness, independent
  fixture/native joins and complete correlated request/reply capture.

## Evidence and open limits

- Both entry and shortcut emitters now send direct `resize` and
  `toggle-orientation`, already supported by `decode_command_v4`. Strict
  TypeScript and the full adapter build/test gate pass: 984 tests, no failures
  or skips. The genuine-core carrier observes 12px then 14px boundary changes
  and an orientation flip through production callbacks, queue and real engine.
  These are native-shaped offline readbacks, not live geometry proof.
- K1's can/cannot and degraded list remain the baseline until new verification
  supersedes specific items. Native selection and resize/orientation behavior
  are not yet proved by this slice.

## Selection dependency

- Source audit finds the native selector path and native-only view attempts
  already connected: `world-apply.ts` spends retained view operations before
  `switchView`, records native pre/post/base, and the v4 producer publishes them.
  Core latches the exact parent/output/workspace tuple in
  `settle_selection_feedback` and checks authoritative native echo or raw attempt
  proof in `selection_native_proof`. This is source evidence, not new native proof.
- Focusless native selections can terminate through the existing shared
  predicate with `focus:unproven`. Focus-bearing selections cannot yet terminate:
  `selection_terminal` requires correlated focus feedback when an effect exists,
  while the KDE v4 producer currently emits an empty `focusFeedback` lane.
  Proven focus completion also requires arrival readiness. Both ports belong to
  later slices; no synthetic success, changed core predicate or local settlement
  is permitted here.
- The accepted scope above retains populated-workspace focus settlement as
  deferred. Native switching and non-wedging bounded retention still require
  direct proof before acceptance.

## Open-binding evidence

- Core source replaces the prior view flight per output in `retain_view_inner`,
  prunes overlapping selection bindings on changed selection, and stores at
  most one selection binding per focus output. Native target/raw maps are
  bounded by the affected outputs; supersession silently drops the old parent.
- A new direct-v4 core regression passes four alternating populated switches,
  each with a fresh prior native echo and native-tagged attempt. It verifies one
  retained binding/scope/view flight, empty feedback and terminal lanes, fresh
  command correlation, subsequent observation acceptance and a real orientation
  frame change while the final focus-bearing binding remains open.
- Initial harness failures used a stale native current (making a selection
  redundant), then a non-focused orientation subject (legitimate refusal).
  Coherent native echoes and the actual focused subject correct those inputs;
  no product behavior or completion rule changed.
- Widened scoped Rust tests pass with the new regression and both stage5 proofs
  enabled. Actual private-bus ownership, opaque storage, queue/take and wait-wake
  outcomes execute without skip diagnostics. Scoped build, strict all-target
  clippy and formatting pass; native proof and integrated review remain open.
- Degraded protocol outcome: accepted select replies can carry `result:null`
  and `switchSettled:[]` indefinitely while their focus-bearing binding is open.
  The empty terminal lane is not a failed terminal or a selection-success
  receipt. A superseded open parent emits no terminal. Queue/take readiness is
  report-result delivery, not switch completion; native proof remains required.
