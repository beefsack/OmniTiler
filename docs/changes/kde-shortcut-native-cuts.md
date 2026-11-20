# KDE shortcut native cuts

A KDE shortcut captures the native foreground and selector state when KWin handles the shortcut, not when the broker later drains the queue. Core still resolves each implicit subject after prior FIFO actions. See the [contract](../porting/world-contract.md#ordered-command-ingress) and [reference outcomes](../spec/reference-outcomes.md#kde-queued-execution-time-subject).

## Design and decisions

- Share an unsent cut only while subject evidence is continuous. Foreground and selector transitions split cuts even when proved own; own changes remain absent from `focusChanges`. Gaps do not drop a coherent press.
- Serialize local segments through ready incorporation with the existing Queue and Describe transport. External commands still join the broker's whole FIFO in admission order. No new provenance, wire fields, cancel, or compensation setter.
- Refuse an unfiled press only when its cut cannot be proved or preserved at saturation. An irrecoverable filed cut resets the generation and drops parked external work as well.
- Sealed implicit toggles send `rect:null`. Only the resolved, previously settled floating lifetime can contribute its eligible cut frame. Explicit commands and the by-id maximized-unfloat continuation retain their existing meaning.

## Acceptance

Exercise the registered shortcut through the real broker, including D → toggle → T before Queue ACK, D → toggle → proved-own T → toggle, directional focus → toggle before native arrival, and unchanged native focus through a double toggle. Cover delivery faults, exact evidence retirement and float-frame retention. Keep frozen fixtures identical and measure unchanged and alternating-focus bursts at 50 and 200 windows. The real KDE fixture leg verifies D alone toggles and T stays focused without a stale setter; subjective jank and latency feel require in-person testing.

## Evidence

Before implementation, the registered `toggle-float` callback filed D while
the genuine broker Queue ACK was held. When T activated before the ACK,
the first Describe reported `win-b` (T) instead of `win-a` (D). The new
producer-to-broker regression failed at that boundary.

The unfiltered serial Rust gate passed 3,569 tests across 63 binaries
with no failures, ignored tests, or filters. Format and all-target Clippy
with warnings denied passed. The frozen `world-wire/v4`, `stage5`,
`stage5-v4` and `kde-v4` fixture trees match `ff6eaddc`.

KWin typecheck passed. The unfiltered KWin suite passed 1,434 tests
across 204 suites with no failures or skips; gesture and presentation
consumer binaries were supplied. The native QV4 focus-scope CTest passed
1/1. The genuine broker regressions cover the three focus-ordering
discriminators, the unchanged double toggle, and requester results across
Queue ACK loss, exact ready replay, queue full, NotRun, and generation reset.

### Release measurements

Matched `rapid_input_burst` checked-service workload (30 samples each). Times
are ms from first Queue to last publication or last requester Take; the byte
totals are the largest sampled aggregate Describe requests/replies. All
ordinary single-revision controls remained in the unfiltered Rust gate.

| Windows | Case | Revisions | Publication p50 / p95 | Last Take p50 / p95 | Describe bytes request / reply |
| --- | --- | ---: | ---: | ---: | ---: |
| 50 | Unchanged control | 1 | 6.675 / 7.122 | 6.707 / 7.149 | 36,266 / 18,252 |
| 50 | Sealed unchanged | 1 | 6.709 / 6.840 | 6.745 / 6.876 | 36,287 / 18,252 |
| 50 | Sealed alternating | 5 | 15.225 / 15.400 | 15.240 / 15.405 | 181,583 / 91,585 |
| 200 | Unchanged control | 1 | 27.524 / 28.537 | 27.612 / 28.564 | 142,467 / 68,444 |
| 200 | Sealed unchanged | 1 | 27.687 / 27.942 | 27.778 / 28.001 | 142,488 / 68,444 |
| 200 | Sealed alternating | 5 | 60.838 / 61.146 | 60.856 / 61.184 | 712,588 / 342,551 |

The unchanged path adds 21 Describe request bytes in this workload. Five
transition-separated cuts used about five times the Describe bytes and took
2.28× / 2.21× the control median publication time at 50 / 200 windows.
These are service-side measurements of prebuilt cuts, not physical
keypress latency, KWin callback cost, or subjective native responsiveness.

The `check-report-cost.sh` Callgrind gate passed: 7,805,555.5 Ir/report at
50 windows and 34,597,438 at 200 (4.43× growth). In the first serial
`report_scale all 31 3 all` repetition, the one-output unchanged
median/p95 was 0.5843/0.6370 ms at 50 windows and 2.4162/2.4387 ms at 200.

### Live KDE

Fixture bundle: `live-unit-b-20261120T075210Z`.

- `leg.log` records the physical Meta+G callback on D (`clientCmd=1`), the
  held Queue ACK, native activation of T, and release of the ACK, in that
  order. The resulting Describe sent the frozen D foreground and did not
  carry T's activation.
- The requester result named D. Revision 4 reports D floating and T
  tiled; revision 5 repeats those memberships and incorporates T's later
  activation. Apply of revision 4 was fenced; neither revision requested
  focus. T was still the native foreground, with no focus setter after
  its activation.
- `summary.json` and `native-creation-cleanup.json` record dev teardown,
  restoration of workspace 1 and the operator terminal, and removal of
  both run-owned desktops (one appended by the fixture, one identified
  by the adapter's same-turn creation readback).

This establishes the physical shortcut/activation ordering and settled
membership for this leg. The real-host proved-own overlap and subjective
jank and latency feel remain unverified.
