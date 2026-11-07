# V4 phase 2: KDE selection

## Goal and status

- Port workspace selection and its acknowledgement/correlation to the native
  KDE adapter while retaining K1 ordinary tiling. Status: selection audit pending;
  the direct resize/orientation repair passes offline gates.
- Start from the [K1 closeout](archive/v4-phase2-kde-port.md#k1-final-outcome).
  The [phase-1 note](v4-phase1-core-protocol.md) remains active and unchanged.
- First repair resize and toggle-orientation emitters to use the existing direct
  v4 commands. The codec already accepts both; no codec change is required.

## Scope and non-goals

- Selection command ingress, native selector execution, exact original view
  attempt retention and terminal acknowledgement/correlation belong here.
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
