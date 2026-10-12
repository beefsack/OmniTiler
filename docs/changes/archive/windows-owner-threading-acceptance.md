# Windows owner threading combined acceptance (Stage 3)

- Outcome: single-output offline hardening, scoped agent live evidence and
  publication CI complete. Physical acceptance and deferred legs below
  are not claimed.

## Goal and scope

- Harden target C on one output: combined owner, input and render pumps;
  bounded transport, cross-thread epochs, retry and truthful teardown.
- Correct concrete integration defects with small production fixes and
  deterministic regression/stress fixtures. Preserve one owner authority,
  existing input semantics and recovery contracts.
- Binding design: [owner threading](../../research/windows-port/owner-threading.md).
- Multi-output/mixed-DPI, multi-output foundation, gaming/suspension cost,
  intentional process-loss recovery and physical visual/latency acceptance
  remain separate evidence legs.

## Acceptance

- Stress saturation/coalescing and safety ordering across input and render;
  exercise stale policy/epoch races and retry/teardown fault combinations.
- Missing acknowledgements cannot report clean stop or admit overlapping pumps.
- Inspect idle waits and measure bounded idle CPU and wake activity honestly;
  blocking alone does not establish imperceivable gaming cost.
- Scoped desktop bursts with disposable application windows and repeated
  owner start/stop: clean worker exit, carrier cleanup, reveal and Mouse Snap
  restoration. Label automated observations as agent live evidence.
- Native locked four-package build/test, fmt, strict all-target Clippy,
  whitespace and publication CI.

## Approach and bounded units

1. Audit current transports, owner coordination, failure seams and idle waits.
2. Fix reproduced defects and add deterministic combined stress/fault coverage.
3. Review integration and live-effect boundaries; run native gates and bounded
   desktop lifecycle/burst/idle checks.
4. Record evidence and remaining acceptance, update design and archive.

## Findings and decisions

- Corrected input teardown: repeated stop waits previously panicked on a
  consumed join handle; an acknowledgement could precede a stalled thread and
  cause an unbounded join; a failed join could be discarded as clean. Cache the
  terminal report, join only a finished thread, and require successful release
  plus successful exit. Unclean/missing completion retains the process slot.
- Input and render had fixed 10 ms exchange timers. Input now wakes on accepted
  publication/control, retry flush, owner outbox drain or stop, plus native
  messages and install-retry deadlines. Renderer uses its existing work event
  and native messages, with a timer only for outstanding hide retries.
- An initial indefinite-wait implementation stranded callback work delivered
  during the pre-wait message drain. Review caught it before live testing;
  drain/transfer before blocking and again after return fixes it. A causal fake
  callback seam asserts real classifier queues are empty before the wait.
- Transient try-lock contention on a free outbox could strand retained input
  behind an indefinite wait. A contention-only 5 ms retry preserves delivery;
  true occupancy waits for owner drain notification. Fully idle has no timer.
- Render stop now signals its own work event, closing the bare-stop wake seam.
- Owner retains its 100 ms maintenance wait for existing service/chord sampling
  and its slower observation/reassertion cadence. All pumps block between work;
  this is not a claim that the owner has no periodic maintenance.
- Trace-only `pump-waits` summaries use the existing slow cadence. Counts are
  native wait returns, separated into work/message/timeout/failed, not scheduler
  context switches or acknowledgement counts. Outer owner counts exclude nested
  focus/proof waits. Input diagnostics are one coherent snapshot; renderer
  atomics and cross-pump observations are not a simultaneous snapshot.
- Diagnostic review corrected an initial failed-wait-as-timeout classification:
  only native timeout 258 is a timeout; unexpected/failed returns are separate.
  No wait-failure recovery policy was changed.
- Stage 1 physical input improvement remains user-accepted. Stage 2 has scoped
  automated native evidence, not physical visual acceptance.
- Tentative, pending user review: the existing 500 ms preview hide-proof refusal
  and unsafe guardrail remain as documented in the Stage 2 record and decisions.
  No additional product behavior tradeoff was introduced here.

## Verification and outcome

- Native Rust 1.99.0, MSVC target. Locked four-package build/test (`tiler-core`,
  `tiler-protocol`, `tiler-kwin-effect-ffi`, `tiler-windows`): 1,688 passed,
  zero failed, one pre-existing ignored benchmark. Windows library: 528 passed;
  Windows integration: 364 passed. All-workspace fmt, locked strict four-package
  all-target Clippy with warnings denied and whitespace gates passed.
- Implementation `baee279`, portability correction `2fce63f`:
  [publication CI](https://github.com/beefsack/OmniTiler/actions/runs/38285336533)
  passed all seven jobs (Windows, Rust, KWin, native, shell, macOS and licenses).
  Initial Linux strict Clippy found the Windows-only wait classifier compiled
  unused; `cfg(windows)` fixes the boundary without changing native behavior.
  Post-rebase native gates include the concurrent shared-core restore change;
  it adds one passing test to the pre-rebase 1,687 total. Windows counts are
  unchanged. No Linux physical acceptance is inferred from CI.
- Twenty-six added regression/stress tests cover teardown idempotence, ack with
  stalled exit, panic/failed/missing completion, held process reservation,
  deadline plans and actual wake events, pre-wait callback transfer, real-lock
  contention, combined suspend safety and native wait diagnostic discrimination.
- A 3,000-round deterministic interleaving drives the real input/render cores
  and bounded transports: variable stalls, repeated inbox saturation/refusal,
  invalidation escalation, parked outboxes, latest-wins frames and hide epochs.
  All 199 terminal Ups arrived; queues converged and the latest policy applied.
  Fault seams cover partial hook installation/retry, renderer creation refusal
  and recovery, failed hide then verified retry, unhook/destroy failure and
  missing/late completion. These are offline scripted faults, not native fault
  injection or evidence of physical delivery.
- Source review closed the concrete wake and teardown findings. One fixture
  wiring error (separate sender/pump inboxes) was detected by the stress oracle
  and repaired; the final fixture shares the production handoff. Saturation
  fillers avoid changing process-global Esc state.

## Scoped agent live evidence

- Date: 2026-10-13. Physical Windows 11 desktop, ordinary integrity, one output,
  trace enabled. Disposable Notepad, Paint and Calculator windows were launched
  through the desktop broker and bound by process/window lifetime. An unrelated
  host without a matching Calculator child remained scope-excluded and unchanged.
- Five owner start/stop runs, with workspace hide/reveal readbacks in each.
  Initial lifecycle smoke was followed by a 231.8-second long burst: 238/238
  native move/resize requests accepted and 10/10 close/recreate cycles (five
  Paint, five Notepad). Requested geometry was superseded by tiling, not mistaken
  for settled geometry; final owner readback converged with zero mismatches.
  No semantic trace failure was observed.
- Final instrumented artifact SHA-256:
  `029E00BAEFB1993341B6D0EB830CBEA0E68B83229E0692475AAB4D1F419B0AD3`,
  from `7c35412` plus this delivery's production changes. Subsequent changes
  affect comments, an offline stress filler and non-Windows compilation gating;
  the subsequent rebase also includes an independent shared-core restore change.
  Final trace SHA-256:
  `FFCD1A2CCC448AE933184DD2CA163377BD973A0EFEF1A0E219204C61C640CF96`.
- Final settled idle CPU interval: 32.05 seconds, 328.125 ms process CPU
  (62.5 ms user, 265.625 ms kernel), approximately 1.024% of one core, trace-on.
  Two other steady thread CPU deltas rounded to zero at the native counter's
  resolution; thread-name mapping was unavailable. This is a bounded
  development-build measurement, not a release/trace-off comparison or
  gaming-cost acceptance.
- Fifteen in-window `pump-waits` samples: owner +281 returns (265 maintenance
  timeouts, 15 ack-only, one message); input +0 returns/turns; renderer +30
  returns, all work-event wakes. Input/render timeout deltas were zero, and all
  three failure counters remained zero. Renderer still wakes for owner-posted
  refresh work. Summaries have no timestamp, so exact sampled duration is not
  available; the samples lie inside the CPU interval with slow-cadence boundary
  exclusion. No per-second wake rate is inferred from throttled trace frequency.
- Every run: input unhook and renderer verified destruction/exit reported clean,
  exact owner exited, owned windows were revealed, Mouse Snap baseline readback
  was restored, and no owner/carrier/ledger-claim residue remained. Disposable
  windows were closed afterward. No process-loss recovery result is claimed.

## Remaining acceptance and next action

- User physical checks: border/underlay/preview pixels, smooth follow and bands;
  Win+Shift, native move/resize and Win+drag; real cross-slot/group-edge drop;
  Esc/cancel/suspend/stop without stale show; settings/accent changes; workspace
  hide/reveal; Win+arrow/native edge-resize comparison after the idle-wait change.
- Multi-output/mixed-DPI and managed multi-output foundation remain separate.
  Active/suspended/fullscreen gaming-cost evidence and game compatibility remain
  pending. These results do not select or prove Win+G/F11 containment.
- Existing untagged targets retain the process-fenced native reuse race;
  tagged lifetime proof and honest `id_proven:false` reporting are unchanged.
- Exact next action: physical single-output visual/input checks; retain the
  deferred multi-output/DPI/gaming legs explicitly.
