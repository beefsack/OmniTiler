# KDE command latency and single report ingress

## Goal and scope

Implement approved C7/C10: commands never wait for the native-event debounce;
all adapters use one report ingress accepting a report and ordered command list.
Keep the 120 ms debounce for native-event bursts. No wire, fixture, policy,
or unrelated latency-budget changes.

## Evidence and approach

- Idle commands already file and dispatch immediately. During native bursts,
  `kickAfterFile` and `scheduleDrain` in `kwin/src/world-entry.ts` defer command
  execution behind an armed 120 ms debounce. Broker filing alone does not run
  core. Existing FIFO tests explicitly assert this delay.
- Replace the two public core ingress methods with the existing batched
  implementation under one name. Empty command lists preserve existing inline
  report-command semantics; no wire migration is needed.
- Command dispatch preempts native debounce, while outstanding captures and
  filing acknowledgements retain ordering and existing failure semantics.
- Keep macOS changes mechanical: pass an empty list and select the outcome.

## Acceptance and verification

- Rapid commands and commands interleaved with native bursts execute in order,
  exactly once, without waiting for a 120 ms timer.
- One public report ingress; neutral contract wording; no compatibility shim.
- Rust three-package suite, KWin suite, typecheck and Rust formatting pass.
- Frozen fixture families remain byte-identical.
- Independent diff review and a small live KDE responsiveness trace.

## Outcome

- Replaced both public core report methods with `handle_report(report, queued)`;
  migrated callers mechanically, including macOS and Windows. Inline report
  commands and all wire bytes remain unchanged. Contract wording is now neutral.
- Filed commands and queued tails preempt the native timer onto a zero-delay
  drain. Hydration completion also drains parked commands without debounce;
  existing correctness gates and FIFO filing order remain. Native-only bursts
  still wait 120 ms. A retired timer cannot trigger a trailing capture.
- Regression coverage proves rapid and interleaved commands retain order and
  exactly-once filing, execute before the native timer, respect hydration,
  and ignore the cancelled timer. The hydration test first failed against the
  old kick and passed after the narrow correction. Independent review is clean.
- `cargo test -q -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture`:
  3297 passed, zero failed. `npm test` in `kwin/`: 1275 passed, zero failed.
  `npm run typecheck` and `cargo fmt --all -- --check` passed.
- Frozen `world-wire/v4`, `stage5` and existing `stage5-v4` fixture bytes are
  unchanged. No fixtures were refreshed.
- A small KDE physical-key smoke check dispatched three directional commands
  in 3, 2 and 2 ms from capture (trace timestamps). It exercised idle dispatch,
  not a native burst; deterministic regressions supply the burst proof.
- macOS and Windows edits are mechanical call-site migrations, not compiled or
  runtime-verified on their native platforms by these Linux checks.
