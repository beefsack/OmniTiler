# Windows workspace reveal latency

## Goal and scope

- Diagnose slow workspace selection with three ordinary windows on one workspace and a single application on another.
- Scope: Windows owner dispatch, visibility observation, diagnostics, and reveal ordering. No shared-core or KDE behavior changes.
- Acceptance: distinguish queue/wake delay from in-action work; identify measured and unmeasured intervals; select a remedy only after its safety and latency evidence are sufficient.

## Before batching: evidence (2026-10-17)

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
- Earlier recorded ordinary-app select durations were 72-124 ms; physical dogfooding was 171-241 ms (`windows-workspace-action-latency.md`). Different app mixes prevent attributing the increase to a particular commit from these samples alone.
- Owner busy durations are 801619/1298497/313565/497667/464875/741736 us, matching the respective action durations within about 5 ms. The complete hide/reveal sequence executes inside one owner busy section.
- Queue waits are 2-4 ms. There is no between-reveal owner sleep, deadline expiry, or missing wake: `selection.rs` synchronously iterates the entering windows. Render timeout and failure counters remain zero. The cumulative owner-timeout counter plateaus during these actions; it is not an elapsed-duration measurement.

## Original causal findings and limits

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

- The initial diagnosis and implementation gates were offline; subsequent user physical acceptance is recorded below.
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

## Outcome

- Offline acceptance is complete for Windows select/send batching and timing. Shared-core and KDE behavior are unchanged; unrelated single-window migration/audit/lifecycle behavior remains outside this change.
- Tentative choices retained: separate hide and reveal batch deadlines, observed-state confirmation after host notifications, recoverable claims for unconfirmed windows, and focus/z-order only after the existing success barriers.
- User physical dogfooding on 2026-10-17 accepts the reported latency fix: "huge improvement"; slow one-by-one workspace reveal resolved. Measured successful switches and timing breakdowns are recorded below. Exceptional timeout/recovery cases retain offline regression coverage; this run did not induce those failures.

### CI portability repair

- CI run `39004877767` passed Windows (3304 passed, 0 failed, 1 ignored across its broader package set), but Linux/macOS strict clippy found six unused batch-module items: the private module was unconditional while its production consumer is Windows-only.
- Restrict the module to `cfg(any(windows, test))`: exclude it from non-Windows production libraries while retaining all ten portable fake-native unit tests. No warning suppression, API expansion, or Windows runtime behavior change. The full local four-package gates pass again after this correction.
- The same run also failed the existing `kwin-66-api-guard` clean-tree/clean-marker checks and native KCM shortcut-selection assertion at `activeborderconfig_shortcut_test.cpp:2875`. Parent run `39002940466` had the identical guard failure but passed native tests; native sources are unchanged across the feature, so the native failure is separate unresolved test evidence, not a batching fix target.
- Final source CI `39005491820` passed Windows (3304 passed, 0 failed, 1 ignored), macOS, Rust, native, licenses, notices, and KWin. The native assertion did not recur; only the pre-existing shell `kwin-66-api-guard` failure remained.

## User physical acceptance and new trace (2026-10-17)

- User ran `just dev trace` after `739a97c0`/`5f1bf4fa` and physically accepted the result quoted above. Offline analysis of that trace finds 20 successful workspace actions: one send and 19 selects, all with successful focus. Queue waits are 1-2 ms.
- The first return reveals two windows; later returns reveal five, against a single-window workspace. The new single-window token is identified as `steamwebhelper`; no token continuity or identical application mix is assumed across runs.
- All table durations are integer ms. H/R fields are `count/post/wait/ledger/log`. The final duration comes from `workspace-action.action_ms`, not the earlier visibility-timing line's partial action duration. Action 19 has a separate send-hide leg before selection; its final 94 ms covers the whole send.

| Action | Direction | H count/post/wait/ledger/log | R count/post/wait/ledger/log | Final action ms |
| --- | --- | --- | --- | --- |
| 19 send-hide leg | 1 to 2 | 1/0/1/3/0 | 0/0/0/0/0 | - |
| 19 select leg | 1 to 2 | 2/0/3/15/0 | 1/0/2/9/0 | 94 |
| 25 | 2 to 1 | 1/0/0/6/0 | 2/1/3/14/0 | 67 |
| 56 | 1 to 2 | 5/0/15/41/0 | 1/0/2/8/0 | 115 |
| 58 | 2 to 1 | 1/0/0/6/0 | 5/0/10/23/0 | 108 |
| 60 | 1 to 2 | 5/0/8/42/0 | 1/0/3/4/0 | 98 |
| 62 | 2 to 1 | 1/0/1/7/0 | 5/0/11/28/0 | 113 |
| 64 | 1 to 2 | 5/0/12/46/0 | 1/0/2/4/0 | 107 |
| 66 | 2 to 1 | 1/0/0/6/0 | 5/0/9/21/0 | 103 |
| 68 | 1 to 2 | 5/0/13/46/0 | 1/0/3/4/0 | 105 |
| 70 | 2 to 1 | 1/0/0/4/0 | 5/0/8/30/0 | 108 |
| 72 | 1 to 2 | 5/0/7/37/0 | 1/0/4/7/0 | 102 |
| 74 | 2 to 1 | 1/0/0/4/0 | 5/0/8/24/0 | 106 |
| 76 | 1 to 2 | 5/0/12/36/0 | 1/0/3/7/0 | 104 |
| 78 | 2 to 1 | 1/0/0/6/0 | 5/0/8/21/0 | 103 |
| 81 | 1 to 2 | 5/0/13/46/0 | 1/0/3/4/0 | 105 |
| 83 | 2 to 1 | 1/0/0/7/0 | 5/0/10/25/0 | 110 |
| 87 | 1 to 2 | 5/0/11/44/0 | 1/0/4/4/0 | 99 |
| 90 | 2 to 1 | 1/0/0/6/0 | 5/0/10/28/0 | 107 |
| 96 | 1 to 2 | 5/0/7/37/0 | 1/0/2/7/0 | 98 |
| 99 | 2 to 1 | 1/0/0/3/0 | 5/0/13/30/0 | 109 |

### Direction-matched comparison

| Operation | Before action ms | After action ms | Before reveal phase ms | After reveal phase ms |
| --- | --- | --- | --- | --- |
| 2 to 1, multi-window reveal | 493/737/1293 (3 windows) | 103-113 (5 windows); 67 (2 windows) | 406/656/761 | 62-74 (5 windows); 27 (2 windows) |
| 1 to 2, single-window reveal | 309/457 | 98-115 | 38/175 | 9-13 |
| Send 1 to 2 | 795 | 94 | 178 | 13 (selection leg) |

- This is direction-matched evidence, not an identical-mix benchmark; window counts and send composition differ. The five-window returns are much faster than the original three-window samples, consistent with the user's physical acceptance.
- Hide waits are 0-15 ms, nonempty reveal waits 2-13 ms; hide ledger writes 3-46 ms, reveal ledger writes 4-30 ms. Post calls are 0 ms except the two-window reveal at 1 ms; log writes are 0 ms. Zero means truncation below the integer-ms reporting resolution, not zero work; timing summaries exclude their own write.
- Wall phase totals include additional preparation/native/snapshot/finish work, so post/wait/ledger/log subtotals do not exhaust them. No synchronous log/ledger stall is established by these samples.

### Anomalies and limits

- No deadline-expiry, partial, refused, uncertain, rollback, or omission-summary lines. All 20 actions succeeded; render/input timeout and input/owner/render failure counters remain zero. The cumulative owner-timeout counter reaches 15 across idle pumps, not batch deadline expiries.
- Twelve owner busy outliers exceed the 50 ms diagnostic threshold: nine correlate with completed actions (97.519-118.859 ms), three with startup/reconcile periods (50.761-55.087 ms). Emission is throttled, so not every action has an outlier line. No associated switch failure is observed.
- One visibility-history summary at tick 98 reports `history_lost:true` for all six entries; neighboring summaries return false. Actions 96 and 99 succeed. The reason and significance of the observation-history loss are not established by this trace.
- Reconcile-only minimum-hint queries report timeouts at ticks 3, 39, and 48 and failed queries at ticks 110 and 114 during vanished-window reconciliation. No action-correlated minimum-hint query reports timeout/failure; no user window-close cause is inferred.
- Routine render queue maximum is 7.592 ms; retained input summaries include an approximately 1.06 s heartbeat gap and approximately 48 ms snap queue age. These summary values are not workspace-action durations; command queue waits remain 1-2 ms.
- Acceptance closes the reported slow sequential-reveal regression. It does not claim exhaustive live testing of hung hosts, induced recovery failures, or every application mix.

## Deadline follow-up review (2026-10-18)

- Native 2,000 ms expiry/partial-recovery leg not run. The current standard
  owned helper exposes no delayed-visibility or stalled-message-loop control;
  no new fault injector or process-suspension experiment was introduced.
- Source still uses one non-resetting 2,000 ms budget per hide/reveal batch,
  starting before posts, with a final observed-state snapshot. The full locked
  four-package test run passes (2,852 passed, 0 failed, 1 ignored), including
  missing-confirmation/shared-deadline and partial-recovery regressions. These
  are offline evidence, not an observed native timeout or a hard wall-time bound
  on the whole workspace action.
  Source `1ae85779`, documentation-only working changes; retained external
  evidence `live-sweep-gates-20261018` contains build/test/clippy logs and
  `gates.json` (formatting and whitespace checks also passed).
- Keep the existing tentative deadline unchanged. Healthy switch durations and
  the accepted physical latency improvement do not close induced-expiry,
  uncertain-claim or rollback acceptance. A bounded delayed-response fixture
  design and independently proven owned-window recovery are prerequisites to a
  future expiry leg; no new fault-injection mechanism is delivered or selected.
- User-approved 2026-10-19: existing tentative 2,000 ms batch deadline kept;
  no expiry-leg mechanism selected.
