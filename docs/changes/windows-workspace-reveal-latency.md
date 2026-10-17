# Windows workspace reveal latency

## Goal and scope

- Diagnose slow workspace selection with three ordinary windows on one workspace and a single application on another.
- Scope: Windows owner dispatch, visibility observation, diagnostics, and reveal ordering. No shared-core or KDE behavior changes.
- Acceptance: distinguish queue/wake delay from in-action work; identify measured and unmeasured intervals; select a remedy only after its safety and latency evidence are sufficient.

## Evidence (2026-10-17)

The supplied dogfooding trace contains six successful workspace actions: one send and five selects. Times below are integer monotonic milliseconds. Receipt and dispatch are derived, not directly logged: dispatch = terminal minus action_ms; receipt = dispatch minus queue_wait_ms. Allow approximately 1-2 ms truncation uncertainty. Window tokens are trace-local identifiers; the single-window workspace is reported as Steam, but its token's executable identity is not established by the trace.

| Action | Direction | Receipt (derived) | Dispatch (derived) / departure | Hide completions | Reveal completions | Focus / terminal | Queue / action duration |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 29 send | 1 to 2 | 28284 | 28288 / not logged | w21 28563; w5 28597; w7 28639; w6 28883 | w21 29062 | 29070 / 29083 | 4 / 795 |
| 34 select | 2 to 1 | 29870 | 29873 / 29876 | w21 30356 | w5 30846; w7 31083; w6 31118 | 31142 / 31166 | 3 / 1293 |
| 36 select | 1 to 2 | 32158 | 32160 / 32164 | w5 32203; w7 32238; w6 32382 | w21 32421 | 32431 / 32469 | 2 / 309 |
| 38 select | 2 to 1 | 33128 | 33130 / 33132 | w21 33175 | w5 33515; w7 33549; w6 33582 | 33600 / 33623 | 2 / 493 |
| 40 select | 1 to 2 | 34633 | 34636 / 34638 | w5 34679; w7 34857; w6 34900 | w21 35075 | 35084 / 35093 | 3 / 457 |
| 42 select | 2 to 1 | 35502 | 35505 / 35508 | w21 35543 | w5 35920; w7 36083; w6 36200 | 36220 / 36242 | 3 / 737 |

- Three-window reveal completion gaps after the final hide: action 34 = 490/237/35 ms; action 38 = 340/34/33 ms; action 42 = 377/163/117 ms. Logged reveal phases are 761/406/656 ms respectively, with small timestamp-boundary differences.
- Earlier recorded ordinary-app select durations were 72-124 ms; physical dogfooding was 171-241 ms (`archive/windows-workspace-action-latency.md`). Different app mixes prevent attributing the increase to a particular commit from these samples alone.
- Owner busy durations are 801619/1298497/313565/497667/464875/741736 us, matching the respective action durations within about 5 ms. The complete hide/reveal sequence executes inside one owner busy section.
- Queue waits are 2-4 ms. There is no between-reveal owner sleep, deadline expiry, or missing wake: `selection.rs` synchronously iterates the entering windows. Render timeout and failure counters remain zero. The cumulative owner-timeout counter plateaus during these actions; it is not an elapsed-duration measurement.

## Causal findings and limits

- Proven: `crates/tiler-windows/src/tiling_sys/selection.rs` reveals windows sequentially via `reveal_product_claim_to`. `crates/tiler-windows/src/product_hide/sys/mod.rs::post_visibility` posts `ShowWindowAsync`, pumps messages and checks visibility, sleeping 25 ms between checks with a 2000 ms per-window bound. Success requires observed visibility.
- This serial strategy predates `1a4f82ad` and `aa766333`. The event-driven owner idle change cannot explain a missing wake between reveals: the owner never returns to its idle wait during the sequence.
- Unproven: the triggering delay's exact location. Per-window ShowWindow start/end, first observed visibility, ledger commit, and synchronous log-write durations are absent. Completion gaps include proof/recovery checks, the visibility call/poll, ledger writes, and diagnostic writes. App response latency is plausible, but synchronous I/O stalls cannot be excluded from this trace.
- No evidence implicates render acknowledgements or restart float protection (`e6e2ba86`). Visibility diagnostics (`aa766333`) include synchronous writes and cannot be categorically excluded without timing those writes. The single-window application's identity and causal contribution remain unverified.
- These findings supersede a missing-owner-wake hypothesis; they do not establish a newly introduced behavioral defect.

## Selected approach (2026-10-17)

- Add the Efficiency principle: prefer host events and wakes over polling; timers require a genuine deadline or an explicit lack of host events.
- Add action-level hide/reveal timing breakdowns for post calls, visibility observation/wait, ledger writes, diagnostic writes, and total action duration. Follow existing action-timing log conventions; at most one simple logging test.
- Batch hides and reveals for Windows workspace select/send. Within each phase, post all eligible operations before jointly confirming visibility through host notifications and an owner wake, without periodic visibility polling.
- Tentatively use one overall 2000 ms deadline per batch, rather than a per-window deadline. State its purpose where defined: a host that never confirms must not stall the owner indefinitely.
- Preserve hide-before-reveal, per-window proof/identity checks, observed-success requirements, ledger ownership and recovery, partial-failure handling, and the existing focus/z-order ordering after confirmation. Keep shared-core and KDE behavior unchanged.
- Deadline scope, event plumbing, partial-failure mapping, and post-batch ordering remain subject to implementation evidence. Stop for any additional material product choice rather than silently changing these semantics.
- Event integration: reuse the owner's existing message pump and SHOW/HIDE notifications, without draining unrelated hook events or reentrant intake/reconcile. An initial visibility snapshot handles notifications preceding the wait; subsequent checks follow notifications, with one final deadline snapshot. No periodic wait slices or polling fallback.
- The overall batch budget starts before native posts, not after serial per-window preparation/post work. Existing durable commit-before-hide and identity guards remain required.

## Units and acceptance

1. Add the exact Efficiency text after Simplicity in the principles document as a separate documentation change.
2. Establish the event/wake integration and an offline fake-observation contract for simultaneous multi-window operations, failure recovery, and ordering.
3. Implement the Windows-only batch path and timing breakdowns; add proportionate behavioral regressions, not extensive log-format coverage.
4. Review identity/ledger/recovery safety and actual diffs; verify current source with all required gates. Record accepted evidence and archive on completion.

## Verification

- The initial diagnosis was offline; live verification is pending user dogfooding.
- Deterministic regressions must cover multi-window hides/reveals without unrelated external events, one batch deadline, failed/missing confirmations, recovery and ledger retention, already-visible/recycled handles, and focus ordering. Use injected observations, not desktop windows.
- Required gates: `mise exec -- cargo build --locked -p tiler-core -p tiler-protocol -p tiler-kwin-effect-ffi -p tiler-windows`; the equivalent `cargo test`; `cargo fmt --all -- --check`; strict all-target clippy with the same locked package set; `git diff --check`.
- Final locked four-package build and test, formatting, strict all-target clippy, and tracked/new-file whitespace checks pass. Tests: 2522 passed, 0 failed, 1 ignored (baseline 2500 passed, plus 22 new tests). The ignored test is the existing offline paint microbenchmark.

### Accepted low-level fixture evidence

- A crate-local batch executor now posts all targets before visibility snapshots, reacts to notification-driven snapshots, and uses one non-resetting deadline. Native post/read/wait/clock operations are injectable; fixture tests make no desktop calls.
- Ten low-level offline tests pass: post ordering, independent confirmations, event-before-wait, shared deadline, post failure isolation, notification-without-state, unrelated wakes, destroyed targets, reveal direction, and post-work accounting. They also pass within the final full gates.

### Integration and safety review

- Workspace select/send and rollback now use joint post/confirm phases, including shared/swap side collections. Native claim preparation and completion are factored with existing single-window paths; unrelated migration, audit, and lifecycle paths retain their prior behavior.
- Timing lines report visibility phase post/wait/ledger/diagnostic-write costs alongside existing action totals; each timing summary excludes its own write to avoid recursive accounting.
- Targeted batch tests report 22 passing; the Windows library suite reports 1071 passing, 0 failing, 1 ignored. Independent review reproduced the 22 targeted passes and accepted the actual diff's post/observe ordering, bounded event-driven wait, ledger/identity handling, narrow rollback, focus ordering, and timing accounting. Final four-package gates passed without warnings or reruns.
- An initial driver change retired vanished member state too broadly and broke send regressions; caller-specific retirement was restored. A fixture bypass in native window-liveness reads was repaired at the native boundary, preserving the production checks. Temporary test ledger collisions were isolated with unique directories and valid generated claim tags.

## Outcome and remaining evidence

- Offline acceptance is complete for Windows select/send batching and timing. Shared-core and KDE behavior are unchanged; unrelated single-window migration/audit/lifecycle behavior remains outside this change.
- Tentative choices retained: separate hide and reveal batch deadlines, observed-state confirmation after host notifications, recoverable claims for unconfirmed windows, and focus/z-order only after the existing success barriers.
- Live verification is pending user dogfooding: confirm real SHOW/HIDE delivery, perceived multi-window latency, partial-failure behavior, and the timing breakdowns with the affected application mix. Retain this record at its current path while that product evidence is pending.
