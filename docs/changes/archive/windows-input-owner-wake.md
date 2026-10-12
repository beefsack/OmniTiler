# Windows input owner wake

## Goal and scope

- Remove the keyboard-action latency introduced by the dedicated input pump,
  preserving its native drag-delivery improvement and Stage 1 contracts.
- Scope: discrete input-batch notification and the outer owner wait. Preserve
  synchronous classification, bounded queues, freshness, safety controls,
  mask obligations and owner-only foreground actuation.

## Evidence and approach

- Stage 1 offers tagged batches without notifying the owner. Its outer
  `pump_wait` waits on messages only, with a 100 ms fallback timeout; input
  publication therefore adds an owner polling phase.
- Path: `snapkey.rs::llproc` queues locally; `input_pump.rs::PumpCore::step`
  pumps messages with a 10 ms quantum, then `offer_batches` publishes through
  `Outbox::try_put`. The owner checks settings and publishes change-gated
  snapshots before taking tagged batches and dispatching through `keyboard_tick`.
  No owner wake existed in this handoff. Occupied outboxes defer later batches;
  the policy fence is asynchronous, not an owner wait for an input reply.
- Existing scoped trace: maximum callback-to-owner snap queue age 220,712 us;
  maximum offer-time queue age 127,329 us. These are queue metrics, not
  end-to-end latency, and repeated diagnostic samples retain their last value.
- All 27 traced focus dispatches succeeded immediately, with no stale batches
  or drops. Foreground prime/attach/setter/detach/readback ordering is retained;
  focus-settle bounds have not changed.
- Traced owner work peaked at 109 ms. Delivery-gap samples were all zero at
  tick-count millisecond resolution, not evidence of zero native delay. The
  existing focus settle can still block later owner dispatch up to 500 ms;
  this run did not exercise it. Input callbacks continue independently.
- Add a bounded event-driven owner wake on accepted keyboard/mouse intent
  batches. Diagnostics alone must not wake the owner. Keep the timeout fallback
  and prevent lost wakes or focus-settle recursive dispatch/busy spinning.

## Acceptance and verification

- Deterministic offline coverage for successful offer, refusal/retry, coalesced
  keyboard/mouse notification, take/republication interleavings and initially
  nonsignaled wake creation. No threaded contention or creation-failure injection
  is claimed.
- Root mise-selected MSVC locked four-package build/test, all-workspace fmt,
  strict all-target Clippy and `git diff --check`; publication CI.
- Physical acceptance remains pending: same scoped dev trace command, Win+arrow
  responsiveness and edge drag-resize smoothness. No live test evidence is
  inferred from offline checks.

## Work and outcome

- Implemented in `input_pump.rs`: one shared `OwnedHandle` auto-reset event;
  accepted fresh and parked-retry snap/windrag batches call `SetEvent` outside
  callbacks. Diagnostic publication remains unwaked. Event creation failure
  releases the pump reservation and refuses startup before hooks install.
- `tiling_sys.rs::pump_wait_with_intent` includes the event in
  `MsgWaitForMultipleObjectsEx` only for the outer product wait. Existing
  `pump_wait` calls use the same implementation without that event. A pending
  signal survives offer-to-wait and multiple offers coalesce; take-before-notify
  can cause a harmless extra empty wake, not dropped data.
- Review found no production defect in the correction. Regression coverage was
  tightened to demonstrate combined keyboard/mouse coalescing and explicit
  take/republication interleavings rather than claim them from a single offer.
- Final native offline gates, 2026-10-12: Rust 1.99.0,
  `x86_64-pc-windows-msvc`; locked four-package build/test PASS, 1,566 passed,
  zero failed, one pre-existing ignored benchmark. Windows lib: 437 passed;
  Windows integration: 358 passed. All-workspace fmt, locked four-package strict
  all-target Clippy with warnings denied and `git diff --check` PASS.
- The missing owner polling-phase wake is corrected offline. Owner work,
  snapshot transport, the input message quantum and foreground settlement remain
  possible latency contributors. Physical improvement and preserved drag
  smoothness are pending; no new live test was performed.
- Publication CI pending.

## Exact physical recheck

```powershell
mise exec -- just --justfile windows.justfile tile --user-start --trace --scope-exe notepad.exe
```

- With the same scoped window arrangement, tap Win+arrow in each direction,
  then hold/release for repeat actions. Compare first-press responsiveness with
  the earlier Stage 1 run; check for missed actions, leaked Snap or Start.
- Repeat native edge drag-resize and compare smoothness with the earlier
  Stage 1 gain. Report keyboard feel and drag smoothness separately; retain the
  resulting trace for queue-age and owner-work comparison. Queue-age samples
  retain their last value and are not per-press end-to-end measurements.
