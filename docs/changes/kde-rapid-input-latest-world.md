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
- A bounded fixture-only injected-input burst establishes complete ordered
  command settlement and final native/core agreement. Correct semantic refusals
  (unchanged, no neighbour, outer edge or minimum size) are allowed; busy, dropped
  or stale-subject refusals are not. Physical acceptance remains user-run.

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
are verified in separate commits. Legacy admission rejects implicit commands,
so its resolved-identity clone and extra publication argument were redundant;
all five production and six test callers are updated. The harness's retired
identity constants had no importers, its factory's twenty callers pass no
arguments, and its identity field had no readers. Scoped Rust gates and adapter
typecheck, all 1183 tests and the unscoped build remain green after these culls.

## Native acceptance blocker and injected burst (2026-11-08)

- The earlier pointer-activation attempt stopped before command input. The
  replacement fixture-only leg used an appended workspace, two joined Wayland
  fixtures and the exact `OmniTilerTestWindow` filter. Evaluated readiness,
  discovery and matched observation dispatch/applied pairs passed after a fresh
  journal cursor under the verified KWin D-Bus-owner PID.
- PASS, activation: exact sentinel/PID/native-UUID joins preceded KWin
  `WindowsRunner.Match` on the fixture title and `WindowsRunner.Run` with the
  returned `0_{UUID}` identity and empty second argument. Fixture A reported
  `focused:true`, B false, and presentation foreground matched A before input.
  No pointer activation or full-native-roster uniqueness claim is involved.
  Raw basis: `match-a.json`, `match-b.json`, `native-a.json`, `native-b.json`,
  `focus-before.json` and `present-focused.json`.
- Repro: start with A left and B right, both tiled, A focused. Inject ten
  uninterrupted repetitions of focus-right, focus-left, grow-right,
  shrink-right, move-right, move-left using ydotool's one-millisecond key delay.
  These are `Meta+L`, `Meta+H`, `Meta+Alt+L`, `Meta+Alt+Shift+L`,
  `Meta+Shift+L`, `Meta+Shift+H`: 60 chords, 340 key events, one injection.
- PASS, delivery and terminal accounting: all 60 callbacks and queue identities
  1 through 60 appear exactly once, with no missing identity or busy refusal.
  Every identity has a terminal Take result. The initial blanket no-refusal
  check did not pass: only
  identities 1 through 9 are READY; 10 through 60 fail `operation-refused`.
  Repeated terminal reads produce 13 READY and 77 failed log lines, not 90
  distinct commands. The first observed failing prefix ends at command 10,
  the second shrink-right; this is not a minimized reproducer or root-cause
  classification. Raw basis: `burst-events-list.txt`, `journal-burst.txt` and
  `planner-output-final.log` (first failed result at line 122).
- PASS, final snapshot agreement: both joined native frames equal the final
  published allocations, and fixture B alone reports focused, matching the
  final foreground. This proves agreement with the published result, not
  successful execution of all 60 requested actions. Presentation rows are
  desires; native `getWindowInfo` supplies frames and fixture `get` supplies
  focus. Fixture client sizes are not outer-frame measurements. Raw basis:
  `present-after.json`, `native-after-a.json`, `native-after-b.json`,
  `get-after.json`.
- NOT RUN, independent latest-only native-history proof: the trace contains
  29 dispatches, 27 applied publications and two superseded-in-flight messages,
  but sparse native samples cannot exclude per-press intermediate native work
  or paints. One capture per batch is intended; no per-press dispatch/applied
  pairing, full protocol capture or no-pending-operation claim is made.
- Startup repeated 129 declaration-order warnings and two non-activatable-name
  D-Bus errors. Neither recurred in the burst journal window; their significance
  remains unclassified. Raw basis: `start-journal.txt`, `start-errors.txt`,
  `journal-burst.txt`, `journal-burst-errors.txt`. Planner evidence comes from
  its receipt-selected redirected output, not the systemd journal.
- Evidence corrections: an initial fixture-JSON extraction was corrected
  before input; a read-only input-help query first used a missing default socket
  before using the private socket. Review corrected an inaccurate zero-warning
  count and deduplicated terminal results. No mutating call was repeated and
  no controller restart or additional command burst ran.
- Cleanup passed: owned controller, Planner, fixture and input daemon stopped;
  fixture/input sockets and the single appended workspace were removed, original
  workspace roster/current restored, KWin owner unchanged, unscoped non-trace
  bundle rebuilt and final production status DOWN. Existing shortcut records
  were left untouched. Raw basis: `dev-off.log`, `fixture-shutdown.json`,
  `desktops-final.json`, `current-final.json`, `restore-build.log`,
  `dev-status-final.txt`.
- The blanket no-refusal criterion is superseded by semantic classification
  below. Latest-only native-history, physical-key and perceived-latency acceptance
  remain unproved. Injected input does not close physical or perceived acceptance;
  this note remains active.

## Offline refusal diagnosis and capture-boundary correction

- Correct edge, no-neighbour, unchanged and minimum-size outcomes are not
  defects. The retained burst summaries contain only `operation-refused`, not
  the internal reason or complete capture reports, so they cannot classify all
  51 live refusals. The queued resize diagnostic also reads `report.command`,
  which is absent when commands arrive through the queue; its internal-reason
  line is therefore absent from this earlier evidence. No exact live attribution
  is claimed from the reconstructed cases.
- The genuine service/broker/core replay now compares the same 60-command
  sequence as one batch and one command per capture with settled native feedback
  from published frames and issued focus effects. Both legs return 60 READY
  results and the same final focus/frames. Initial test setup incorrectly reused
  stale native reports as a slow-input oracle and assumed native geometry fixed
  core insertion order; those premises were corrected. Presentation foreground
  is not an issued-focus-effect oracle.
- A separate split-batch regression reproduces a real defect: implicit focus
  changes the preserved core subject, then the next capture re-reports unchanged
  lagging native focus. The next implicit resize resolves the preserved subject
  but its internal operation event carries the lagging native focus, causing
  `focus-mismatch`, normalized to `operation-refused`. This is not an edge or
  minimum-size refusal and requires native settlement before another queued
  batch can proceed, contrary to execution-time core-focus semantics.
- RED: the core capture-boundary regression refuses the expected READY resize;
  the service one-batch versus split-batch regression diverges on the first
  post-boundary implicit command. The settled 60-command replay remains green.
  An initial correction marking only in-batch focus provenance leaves these
  failures unchanged because the observation sweep already synchronized the
  session to the lagging capture.
- The owning correction carries the resolved implicit subject as command-scoped
  operation-event focus, guarded to its live homed domain, and restores that
  context after dispatch. Raw native facts, explicit-command gates, changed
  native-focus incorporation and genuine focus loss retain their existing
  semantics. No transport or wire format changes are involved.
- GREEN: 33 focused core regressions and 16 service burst tests pass, including
  the split-batch equality and settled 60-command replay. The scoped three-package
  Rust suite initially passes 3192 tests, including both stage5 proof files; only
  `workspace_unsafe_posture` is excluded. Independent source review finds no
  concrete defect in the correction.
- Queued refusals now log the executing command sequence, resolved subject and
  verbatim internal kind alongside the mapped wire kind. Queued resizes retain
  their bounded staged-context detail under the same command identity, rather
  than looking for an absent inline command. Three additional tests cover queued
  identities, actual refusal kinds and safe formatting. Their initial RED is a
  missing-formatter compile failure, not a second semantic reproduction.
  Independent diagnostics review finds no blocking privacy or correctness defect;
  emitted stderr is inspected but not asserted by the formatter tests.
- Final seven scoped gates pass after mechanical Clippy repairs to two nested
  diagnostic matches and a double-ended test iterator: formatting, 3195 Rust
  tests with both stage5 proof files enabled, build, strict all-target Clippy,
  adapter typecheck, all 1183 adapter tests and unscoped non-trace build. Protected
  wire fixture bytes are unchanged. A bounded post-correction injected burst
  remains pending; no final live outcome is claimed.

## Post-correction staging stop (2026-11-09)

- The correction is committed as `5883b47d`. The fixture-only re-check started
  once on an appended workspace. Evaluated ready/discovered and two observation
  dispatch/applied pairs appear under the verified KWin owner. Exact fixture
  sentinel/PID/native-UUID joins and WindowsRunner activation of A passed;
  fixture focus and presentation foreground agreed. Native resizability is not
  exposed by `getWindowInfo`; only the fixture's default is established.
- FAIL, initial-layout precondition: native and published frames agree, but B
  is left and A right, the mirror of the required A-left/B-right start. The
  journey stopped before input. No staging shortcut, input daemon or burst ran.
  Raw basis: `16-info1.json`, `17-info2.json`, `18-match.json`,
  `20-getfocus.json`, `21-pres2.json`.
- NOT RUN, post-correction command accounting and semantic refusal counts:
  zero commands were injected, queued or settled. There are no new refusals to
  classify, not evidence that all 60 commands succeed. The earlier 51 remain
  individually unclassifiable. Latest-only native history, physical-key and
  perceived-latency acceptance also remain open. Raw basis: `planner-final.log`.
- Diagnostic evidence shortfall: the retained journal has a `kwin_wayland` tag
  filter, not the required unfiltered owner-PID stream; the separate
  `QT_CATEGORY=kwin_scripting` query was omitted. A text grep is not an equivalent
  error check. The captured pre-input window contains 129 declaration-order
  warnings and four non-activatable-name errors, including a later pair; their
  significance remains unclassified. No burst-window comparison exists. Raw
  basis: `14-kwin-full.txt`.
- Setup corrections: a local `busctl` argument-arity error was repaired before
  any D-Bus mutation (`Properties.Set` requires `ssv`, not `ss`); native state
  was unchanged after the rejected command. A journal-cursor formatting error
  was corrected read-only. No effective mutation was repeated. Initial DOWN and
  fixture PID/socket absence are transcript observations, not separately saved
  raw files.
- Cleanup: controller/Planner stopped, fixture shutdown accepted, original
  workspace roster/current and KWin owner match their saved preimages, and the
  unscoped non-trace bundle was rebuilt. Final production status DOWN was also
  independently rechecked. Raw basis: `23-devoff.txt`, `24-shutdown.json`,
  `27-final-roster.json`, `28-owner2.json`, `30-build.txt`, `31-devstatus.txt`.
  The live result remains stopped/incomplete, not PASS; the change note stays
  active.
