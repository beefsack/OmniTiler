# KDE FIFO input and latest-world application

## Goal and scope

Park every KDE user command in FIFO order and wake the existing capture route.
One fresh capture incorporates native evidence once and drains the whole batch
in core without waiting for native settlement or a capture per action,
resolving implicit subjects from post-action core focus. Return one final full
actuator publication. Commands arriving during capture park for the next
batch. Apply only the newest pending publication, completing an issued native
write before advancing.

Explicit window subjects, the empty directional no-focus marker, internal
drag-verdict and unfloat continuations retain their distinct meaning. Incoming
close and attempt evidence must survive skipped native publications. Physical
key delivery and perceived latency remain separate acceptance checks.

## Material decisions

- The rapid-input decision permits an explicit ingress distinction for current
  core focus at execution: `win:{"currentFocus":true}`, preserving existing
  explicit identities, null refusal and the empty directional no-focus marker.
  The initial null spelling conflicted with immutable reject fixtures and was
  replaced rather than weakening their proof. Implicit maximize uses
  that resolved focus; explicit maximize retains its settled-native gate.
- Close is reported lifecycle evidence, not an outgoing close command. Current
  one-shot effects are only focus, raise and stack. Publication supersession
  must not discard retained incoming evidence or renew issued native writes.

## Acceptance and verification

- RED/GREEN regressions cover mixed bursts, sequential core subjects, no busy
  refusal, close retention, and superseded publications and unissued effects.
- Independent review checks the ingress encoding, ordering and native safety.
- Scoped Rust formatting, tests, build and Clippy plus adapter typecheck, tests
  and unscoped build pass without changing existing v4 wire fixture bytes.
- A bounded fixture-only injected-input burst establishes final native/core
  agreement and no command refusals; physical acceptance remains user-run.

## Approach

1. Specify the smallest explicit subject distinction and reproduce the core
   drain and subject failures before implementing ordered core batch execution.
2. Reproduce adapter failures, integrate a single newest-publication slot and
   preserve incoming evidence independently of native application.
3. Remove superseded directional routing, busy refusal and native rebinding in
   separately verified culls; update command and contract records.
4. Review, run the seven scoped gates and verify the bounded injected burst.

## Current state

Initial regressions produced seven semantic failures and five passing guards.
The first implementation incorrectly required capture-independent execution;
source review found replay-cache, rollback and publication-order defects in
that extra lane. The corrected design removes it, incorporating observations
once and publishing once per drained capture, with no new transport. Those
review findings require regression checks against the corrected transaction.

Corrected broker RED regressions have ten failures and one passing guard
against the superseded queue-time implementation. Tests now supply native
membership evidence for host-owned admission; changing the focused field
alone had not repaired the earlier invalid fixture setup.

The corrected core/broker path now passes the scoped three-package Rust suite
(3164 tests, including both stage5 proofs and unchanged v4 reject fixtures).
Mixed 100-action and current-focus/lifecycle regressions pass. Internal
operation focus now follows prior commands only within the batch; it never
overrides fresh staged focus at the direct command seam outside the batch.
Final review, culls, full gates and native acceptance remain incomplete;
no completed outcome is claimed.

Corrected adapter regressions first produced seven failures with typecheck
passing and no existing test failures. FIFO filing, implicit subject carriage
and the latest pending persistence slot are now implemented. Thirteen focused
adapter cases pass, including distinct publications across a real persistence
flight, exact-byte lost-ACK redelivery, ordered filing, fresh observation after
filing, and requester collection after a timed-out capture. Queue filing is
serialized while an earlier acknowledgement is ambiguous; parked commands
still drain together rather than requiring a capture per action.

The FIFO and migration-producer integration tests now drive the retained
production broker, including queue and per-command take routes, with no
test-side sequencing or concatenated replies. The evaluator's positive smoke
proves two parked commands drain in one capture and identical capture replay
does not execute again. Remaining single-command engine harness callers are
guarded against accidentally modeling a multi-command batch.

All 33 earlier adapter failures were individually classified and now pass.
Subject and batch-publication expectations were updated without weakening
settled explicit identities or native safety checks. The timeout test had
expired asynchronous filing and capture flights before their replies arrived;
changing requester routing did not address that setup. Revised timing separates
a dropped capture from a late reply and verifies both terminal identities,
including the legitimate boundary no-op refusal. Migration fault injection now
follows a readable drain so the test actually exercises unreadable arrival.
The single-command shim refuses multiple parked commands rather than silently
modeling obsolete batching. Adapter typecheck and all 1183 tests pass.

Genuine directional batches also exposed stale implicit focus after successful
tiled focus. Recording the committed target before the next command fixes it.
With topology and unique-command prerequisites asserted, removing that record
produces three core and two service tail refusals; restoring it passes those
regressions. Initial regression fixtures incorrectly inferred core neighbors
from native geometry and reused a consumed command sequence; those fixtures
were corrected rather than changing admission or replay semantics. Implicit
float commands additionally exposed a post-command intent-save identity mismatch:
the placeholder payload identity was compared with the resolved result identity.
Carrying the pre-execution resolved identity through bookkeeping fixes float
and sticky saves; six new core regressions and the scoped Rust suite pass.

Independent review found retained-home source selection and queued intent-save
reservation issues. The central scope check now authorizes the
original press identity before marker conversion. Existing entrypoints already
filter foreign focus, so its new end-to-end scope tests pass both before and
after that defense-in-depth correction.

SEND regressions exposed both placeholder home lookup and a first-command
subject change during native incorporation. The first implicit SEND now selects
the execution subject's pre-observation home from a retained-home snapshot;
later commands use post-prior state. Same-report rehoming still refuses rather
than retargeting. A following SEND also exposed a batch-focus heuristic that
missed an actual command when its target already matched session focus.
Batch-only command provenance now distinguishes that intent from native lag.
Two new discriminating cases failed before these corrections and pass after;
the rehoming guard and existing source-freeze proofs remain green.

Intent-save admission now reserves maximum resolved identity width for implicit
inline saves and distinct explicit plus conservatively distinct implicit saves
for queued batches. Bytes and the existing section row limit are gated before
mutation; final exact-encoding rollback remains unchanged. Two reservation
regressions failed before correction. The scoped Rust suite now passes with
2056 core unit tests. Follow-up review closes the source-selection and
reservation findings with no new concrete defects. Real service-produced saves
with maximum-length identities and 128 synthetic maximum-width rows also fit
their reserves when encoded through the canonical protocol codec; this is not
a claim of 128-window execution.

All seven scoped gates pass: Rust formatting, tests, build and strict Clippy;
adapter typecheck, 1183 tests and unscoped build. The retained broker evaluator's
four tests also pass. Strict Clippy first found eight mechanical lint failures;
their repair briefly introduced a compile error, corrected before all gates
were rerun successfully. Protected fixture bytes remain unchanged. Pure culls
of unreachable legacy bookkeeping support and obsolete harness identity fields
are separately scoped; native injected-input and physical acceptance remain
pending. No completed outcome is claimed.
