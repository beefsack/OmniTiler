# Windows trace volume

## Goal and acceptance

- Bound input-rate and frame-rate diagnostics before writing the run log;
  the development console tails that same file.
- Emit activity-only summaries at most once per second, with counts and
  observed maxima. Emit the first latency outlier immediately and bound
  repeated outliers; retain their counts and maxima in summaries.
- Preserve honest queue/transport/owner-work scopes and all lifecycle,
  decision, window-actuation and failure evidence. Diagnostics must not
  change scheduling or product behavior.
- Verify cadence, idle silence, outlier suppression and aggregate reset
  with deterministic offline tests, then pass native build/test/fmt/clippy
  and diff checks.

## Implementation

1. Audit emitters and the console/file path. Accepted: queue-age logging is
   triggered by each changed pump turn; owner work and windrag transport also
   emit at input rate, and render acknowledgements/status emit at frame rate.
2. Aggregate input ages, owner work and windrag transport on existing owner
   turns, without new timers or diagnostic-induced wakes. Queue age above
   50 ms, dispatch above 10 ms and owner work above 50 ms are diagnostic
   outlier thresholds, not behavioral deadlines. Heartbeat is liveness only.
3. Routine render acknowledgements (including coalesced `Unchanged`), border,
   underlay and drag-preview geometry churn aggregate per carrier. Applied
   state is recorded before diagnostic filtering. Visibility/target/safety/
   proof/reason changes and failures retain immediate evidence. The latest
   applied sample carries generation, proof and safety facts; same-thread
   fallback timings are null rather than claimed as measured zero.
4. Pump wait counters already emit on the two-second maintenance cadence.
   Keyboard callback verdicts, gesture dispositions, loss/stale records,
   native actuation ticks and readback failures retain their evidence; these
   are decisions/outcomes, not routine mouse/frame samples.

## Diagnostic contract

- `input-queue-summary`, `owner-work-summary` and
  `windrag-transport-summary`: at most one per second per active stream.
  `*-outlier`: immediate first breach, at most one repeated line per second
  per stream; every observed breach contributes to summary counts/maxima.
- Queue/transport thresholds are 50 ms, dispatch 10 ms, owner busy 50 ms.
  Heartbeat is liveness only; native tick-count delivery gap is summary
  evidence, never an end-to-end latency claim or alert.
- Owner observations can coalesce pump turns/offers. Input summaries expose
  observed counts and cumulative turn ranges; offer-time queue ages are
  sampled only on fresh offers. Windrag transport samples the last observed
  offer and explicitly excludes unmeasured coalesced offers.
- `render-frame-summary`: at most one per second per active carrier, with
  observed count, actual acknowledgement queue/present maxima and latest
  applied sample. Routine repaint outcome alternation does not evade the
  bound. Safety/proof changes and repeated trace failures do not aggregate.
- Due/trailing flushes use existing owner/maintenance turns, with no new
  scheduling. A final marked end-of-run flush retains sub-window activity.
  Empty windows emit nothing; slow maintenance work may itself be sampled.
- These bounds limit routine telemetry, not genuine decision/failure volume.
  The per-run file remains append-only; no file rotation policy is added.

## Scope

- Windows diagnostic emission and offline regression coverage only.
- Existing cumulative pump-wait diagnostics remain on their two-second
  cadence; low-rate decisions and failure records retain their evidence.
- No new dependencies or per-frame diagnostic mode.

## Evidence

- Source inspection: `scripts/windows-dev.ps1` streams the owner run log;
  source-side bounds therefore apply equally to file and console.
- Binding observability/scheduling constraints:
   [principles](../../principles.md#observability) and
   [Windows owner-threading decisions](../../decisions.md#windows-owner-threading).
- Deterministic clock tests cover thresholds, flood volume, idle silence,
  trailing/end flushes, aggregate reset and throttle persistence across resets.
  File-sink tests cover 3,000 routine frames, 2,000 coalesced acknowledgements,
  alternating routine outcomes, per-carrier independence and immediate
  safety/proof transitions/repeated failures. Same-thread tests cover
  hide/show transitions and honest unmeasured timing.
- Review corrected cross-window outlier throttling and an unbounded
  `Unchanged` acknowledgement path before acceptance. Independent source
  review found no blocking volume, honesty or product-behavior defect.
- Final native offline gates, 2026-10-14: Rust 1.99.0,
  `x86_64-pc-windows-msvc`; locked build/test for `tiler-core`,
  `tiler-protocol`, `tiler-kwin-effect-ffi` and `tiler-windows` PASS.
  Tests: 1,976 passed, zero failed, one pre-existing ignored benchmark;
  +26 regressions over the 1,950-pass baseline (13 input-health, 12 render,
  one additional owner-work test). Windows: 733 library + 398 integration.
  All-workspace fmt check, locked four-package strict all-target Clippy with
  warnings denied and `git diff --check` PASS after formatting/lint fixes.
- Routine input/frame diagnostic volume is bounded in the run file and
  therefore in the console. Long physical dogfood and responsiveness are
  not established by offline evidence.
