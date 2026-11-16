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

- User 2026-11-16 selected removal of the `submitFloat` busy fence: every
  physical Meta+G press queues FIFO, with execution-time current focus. Two
  presses float then unfloat. Core executes every action; the platform applies
  only the latest received publication, dropping superseded unissued targets.
  Internal drag-verdict and unfloat continuations keep their separate path.
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
  or stale-subject refusals are not. Fixture placement order is not under test:
  observe the joined tiled order and fix the left/right role-to-fixture mapping
  once before input. Physical acceptance remains user-run.

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
- Historical staging stop: native and published frames agree, but B
  is left and A right, the mirror of the required A-left/B-right start. The
  journey stopped before input. No staging shortcut, input daemon or burst ran.
  Mirrored placement is not a product failure; the revised check derives its
  roles from observed order instead of requiring fixture names to fix placement.
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

## Observed-order post-correction burst (2026-11-09)

- PASS, setup: a fresh fixture-only leg appended one workspace, started the
  scoped stack once and passed evaluated ready/discovered plus matched
  observation dispatch/applied checks. Exact sentinel/PID/native-UUID joins
  established caller `a` on the left and `b` on the right. That observed mapping
  was fixed before input: left `a` was role A and right `b` role B. WindowsRunner
  activated the joined left fixture; fixture focus and published foreground
  agreed. No window placement order was imposed. Raw basis: `12-start-full.txt`,
  `15-native-*.txt`, `16-observed-mapping.txt`, `17-match-left.txt`,
  `19-focus-check.txt`, `20-presentation-verify.txt`.
- PASS, command path: one uninterrupted ten-cycle burst used the six chords
  listed above, 340 events at one-millisecond key delay. All 60 ordered callbacks
  and unique queue identities 1 through 60 have matching terminal Take READY
  results with kind `-`. The 115 READY lines include 55 repeated terminal reads,
  not extra commands. Refusal counts are zero correct-outcome, zero defect and
  zero unclassified; there are no internal refusal reasons to classify in this
  leg. This does not retroactively classify the earlier 51 refusals. Raw basis:
  `24-events-tokens.txt`, `24-chord-table.txt`, `25-burst-inject.txt`,
  `26-burst-full.txt`, `28-planner-full.log`, terminal-derived `44-accounting.txt`.
- PASS, final snapshot: independent native frames equal both final allocations;
  fixture `a` alone is focused, matching published foreground. No native focus
  getter or fixture client-size geometry oracle is assumed. Raw basis:
  `29-presentation-final.txt`, `30-final-native-*.txt`, `31-final-get.txt`.
- Both startup and burst journal captures use the unfiltered verified KWin
  owner-PID stream and separate actual `QT_CATEGORY=kwin_scripting` queries.
  Pre-input capture contains 129 declaration-order warnings and two
  non-activatable-name errors; burst capture contains no declaration warnings
  and two non-activatable-name errors. Each category query retains the matching
  two D-Bus errors. Their cause/significance remains unclassified, not benign.
  Raw basis: `12-start-full.txt`, `13-start-category.txt`, `26-burst-full.txt`,
  `27-burst-category.txt`.
- Four burst reads log `presentation-continuity-during-flight` followed by
  `presentation-malformed recovery=hide`. Source ordering identifies the
  payload-formatter refusal and display-hide path, not a command refusal.
  The matching Planner summaries retain correlation but not reply payloads;
  the initial evidence review left the display-path classification unresolved.
  The offline classification below identifies the missing-proof path without
  reconstructing the absent replies or the cause of missing native facts.
  Raw basis: `26-burst-full.txt` lines 125-126, 133-134, 228-229 and 257-258;
  `kwin/src/world-presentation.ts` payload formatting and reply handling.
- NOT PROVEN, latest-only native history: 25 dispatches, 20 applied publications
  and five superseded-during-flight messages are observed. Sparse final native
  samples do not exclude intermediate per-press work or paints. This is injected
  input, not physical-key or perceived-latency acceptance.
- Evidence corrections: an unset shell variable first caused local output
  redirection to fail before the fixture recipe executed; absolute paths repaired
  that pre-effect setup once. Only one fixture process and one burst ran. Review
  replaced queue-based READY wording with terminal-Take accounting and surfaced
  the four omitted presentation-hide diagnostics. No effective mutation repeated.
- PASS, cleanup: controller/Planner, fixture and private input daemon stopped;
  exact owned sockets disappeared; original workspace roster/current and KWin
  owner match their saved preimages. Unscoped non-trace build and independently
  rechecked production DOWN pass. Raw basis: `33-dev-off.txt` through
  `43-dev-status-final.txt`, including saved process/socket absence checkpoints.
  Command settlement and final snapshot acceptance pass; the live cause of
  missing presentation proof, independent native history and physical/perceived
  acceptance keep this note active.

## Presentation refusal classification and diagnostics

- Classification: expected missing-coverage fail-closed behavior, not a refusal
  caused merely by overlapping publications. Continuity is logged only after
  lane parsing; for genuine broker replies, the reachable formatter refusal is
  paired-null `coveredInstance` / `coveredInvalid`. Border parsing is identical
  in both paths, and the other formatter checks accept parser-proven shapes.
  See `kwin/src/world-presentation.ts`, `kwin/src/world-report.ts` and
  `crates/omnitiler/src/world_broker.rs` (`project_read`).
- Coverage belongs to the accepted capture, not to the read flight. The broker
  caches the projection and coverage together; unknown effect facts yield no
  `nativeInvalidWitness` and therefore paired-null coverage. No drawable proof
  is invented or inherited from an older publication. The four historical log
  pairs identify this formatter path, but their absent payloads cannot establish
  why native facts were unknown, or an actual visible dropout duration.
- The retained production broker/adapter regression drains two queued presses
  in one capture, serves genuine null-coverage presentation bytes during a dirty
  read, and proves continuity-before-refusal, `reason=missing-coverage`, one
  display-only hide and no paint. With proven effect coverage, overlapping
  captures paint without a hide and converge with one deduplicated refetch.
  A separate subscriber regression proves hide-after-paint and recovery.
- Both malformed diagnostic sites now emit bounded static reasons, distinguishing
  missing coverage, invalid shapes, wrong lanes, unexpected rejections and
  generation/correlation mismatches. No reply bytes, titles or exception text
  are logged. Paint, hide, ordering and coverage rules are unchanged.
- Diagnostic negative control: removing only the reason tokens produces five
  assertion failures among 36 targeted tests; restoring them passes the full
  adapter suite. Independent source review's narrow test-assertion findings are
  resolved. All seven scoped gates pass: Rust formatting, 3195 tests (both
  stage5 proofs enabled, only `workspace_unsafe_posture` excluded), build and
  strict all-target Clippy; adapter typecheck, 1185 tests and unscoped non-trace
  build. Protected fixture bytes and native effect code are unchanged.
- The diagnostic-only verification above did not include another live burst.
  The bounded follow-up below exercises the reason diagnostics; it is not a
  continuity fix or an effect-loaded flicker acceptance claim.

## Reason-bearing live burst (2026-11-09)

- One fixture-only trace leg appended a workspace beyond the existing roster,
  joined two Wayland fixtures by exact sentinel/PID/native UUID, and passed
  evaluated readiness plus matched observation dispatch/applied checks. Native
  frames and published allocations established `a` left and `b` right before
  WindowsRunner activated the joined left fixture. Fixture focus and published
  foreground agreed immediately before input. One uninterrupted ten-cycle,
  60-chord injection used the observed-order sequence above, with 340 events
  and one-millisecond key delay. No second injection or controller restart ran.
- All 60 callbacks and ordered unique queue identities 1 through 60 have
  terminal Take READY results, with no command refusal. The 103 READY lines
  include 43 repeated terminal reads. Final native outer frames match both
  published allocations; fixture `a` alone is focused, matching foreground.
  Raw basis: `18-observed-mapping.json`, `19-match-left.json`,
  `28-focus-at-input.json`, `30-burst-full.txt`, `32-planner-burst.log`,
  `48-queue-accounting.json`, `49-take-accounting.json`, `50-final-agreement.json`.
- Every malformed read is listed below. Each follows continuity for the same
  request and says `reason=missing-coverage recovery=hide`. Planner egress joins
  the read request to its served revision and the accepted capture at that
  revision, within the one bound generation. Exact served observation numbers
  and reply payloads were not captured for these three reads.

  | Read request | Served revision | Accepted capture | Raw journal / Planner lines |
  | --- | --- | --- | --- |
  | `pres-b-1terqkb` | 12 | `src-c-c70v2u` | `30-burst-full.txt:125`; `32-planner-burst.log:258,294` |
  | `pres-h-e0s1un` | 18 | `src-i-1n4xxdy` | `30-burst-full.txt:202`; `32-planner-burst.log:436,462` |
  | `pres-i-2esoqr` | 20 | `src-k-1wbabfz` | `30-burst-full.txt:222`; `32-planner-burst.log:488,490` |

- Supported cause: native facts were unavailable, rather than proof merely
  waiting behind a newer actuator publication. The native border endpoint was
  unowned before startup, after startup, immediately before input and after
  input. Independent pre/post presentation reads carry paired-null coverage and
  hidden/null border witnesses. The whole-leg owner-PID journal records repeated
  non-activatable-name errors at the existing host-deadline cadence, including
  before input. Endpoint snapshots do not prove uninterrupted absence between
  samples. Successful facts arrival is not separately traced, so no claim of a
  complete facts-message history is made. Raw basis:
  `04-border-owner-initial.json`, `14-border-owner-start.json`,
  `25-border-owner-preburst.json`, `34-border-owner-postburst.json`;
  `15-presentation-before.json`, `22-presentation-preburst.json`,
  `33-presentation-postburst.json`;
  `51-whole-leg-full.txt`, `52-whole-leg-category.txt`.
- Classification: expected fail-closed handling of unavailable native coverage;
  no formatter, ordering or latest-wins proof-loss defect is demonstrated. This
  is not a transient-publication explanation: no working native renderer or
  covering facts chain was established before the burst. The broker binds
  coverage to its accepted capture before the adapter's latest-publication apply
  slot (`crates/omnitiler/src/world_broker.rs:903-904`); that slot cannot supply
  proof absent from the capture. The earlier four reads still lack their own
  contemporaneous facts evidence; this new leg does not retroactively establish
  their endpoint state.
- User-visible hide duration and paint outcome remain UNKNOWN: there is no
  independent rendered-border observation and no `presentation-painted` line.
  The endpoint was unavailable at every sampled point, so this leg does not
  establish a visible border disappearing or continuity with the effect loaded.
  Independent evidence review finds no contradictory product outcome.
- Cleanup: controller/Planner, fixture and private input daemon stopped; owned
  sockets disappeared; original workspace roster/current and KWin owner match
  their preimages. Unscoped non-trace build and final production DOWN pass.
  Raw basis: `39-dev-off.log` through `47-kwin-owner-final.json`. No product code
  or fixture bytes changed in this leg; the seven scoped gates above remain the
  current code verification. A read-only accounting query's raw-input option
  error was corrected after cleanup; no effective live mutation was repeated.

## Fixture-only physical RI evidence (2026-11-16)

- Outcome: PARTIAL. Real KWin shortcuts driven by physical input injection
  establish directional callback/FIFO/terminal accounting and final native
  agreement, not human-keyboard feel or compositor presentation latency.
  No product drop/stall was reproduced and no product code changed.
- Each run used joined fixture sentinel/PID/native UUIDs, an appended workspace,
  exact fixture scope, private settings and intent roots, hover disabled and
  group-with-neighbor moves. The existing supported border effect stayed loaded;
  `ep=1` and `presentation-painted` prove the endpoint path, not visible pixels
  or absence of flicker. Evidence bundles below are externally retained.

| Bundle / leg | Verdict | Raw evidence |
| --- | --- | --- |
| `first8`: four left/right move pairs | PASS, behavior; timing qualified | `input-transcript.txt:1-6`, `kwin-owner-journal-full.log:336-499`, `planner-current.log:76-196`: 8 callbacks/queues and distinct terminal READY results; 7 nonempty batches sum to 8. Six intermediate publications are superseded; final revisions 14/15 have `geom=0`, matching native `H[A,B*]` (`native-post-read.json:1`). |
| `undelivered16`: initial mixed attempt | BLOCKED before product queue | Input exits 0 but no callbacks/queues or terminal results (`kwin-owner-journal-full.log:315-333`, `planner-current.log:60-79`). Native state stays unchanged. Daemon attachment/delivery cause is UNKNOWN; socket appearance and client exit are insufficient. No FIFO failure is inferred. |
| `serial-mixed`: six serial controls, then one 16-press mixed burst | PASS, behavior; native-event timing BLOCKED | Controls alternate left/right and restore `H[A,B*]` (`controls-transcript.txt:1-18`). Burst callbacks 7-22 and queues appear in `kwin-owner-journal-full.log:943-1133`; 10 nonempty batches sum to 16, plus one commandless capture. All 16 distinct Take results are READY (19 raw reads; `planner-current.log:338-503`). Final revisions 42/43 have `geom=0`; `native-post-read.json:1` agrees with desired frames and B focus. |
| `nested8`: four up/down move pairs | PASS, accounting and frames; exact tree qualified | `burst-transcript.txt:1-2`, `burst-kwin-journal.log:2-134`, `planner-current.log:186-286`: 8 callbacks/queues, 6 batches `1+1+1+2+1+2`, 8 distinct READY results from 9 raw Take calls. Five intermediate publications are superseded; revisions 24/25 have `geom=0`. `native-post-read.json:1` matches all three normalized allocations with C foreground. `H[A,V[B,C*]]` is inferred from ordinary admission, frames and the serial vertical swaps leaving A unchanged (`c1-native-read.json:1`, `c2-native-read.json:1`), not directly exposed named core-tree readback. |
| Rapid float toggles, pre-decision investigation | BLOCKED; no live input | At the investigated revision, `submitFloat` fenced a pending queue or in-flight world apply before submission. The 2026-11-16 selection and offline correction below supersede that contract blocker; physical even/odd verification remains pending. See the [minimal discriminator](../spec/reference-outcomes.md#kde-queued-execution-time-subject). |
| D2 cross-output | NOT RUN | No proven empty-target crossing leg was collected before the float contract decision stopped further live work. Same-output success is not crossing evidence. |

### Timing denominator and clock boundary

- All three accepted bursts are net-zero at native application: zero new
  native-frame event timing samples, not zero-millisecond latency. Skipped
  publications count supersession, not frame arrivals.
- The first eight commands' final-equality witness gives callback-to-witness
  bounds, n=8: min 13.240, median 34.050 (middle-pair mean), p95/max 59.457 ms.
  This is not timing of newly changed native frames.
- Six serial controls do change native frames. Their saved `frame-attempt`
  rows are flushed on a later capture (`world-entry.ts:1548-1558`), while
  `attemptMs` has no saved shared clock anchor to callback/journal time
  (`world-report.ts:319-324,597-602`). The following deltas are strictly
  callback-to-delayed-native-readback-evidence upper bounds, not exact write,
  frame-change or compositor arrival measurements:

  | Serial command | Window A / B upper bounds (ms) |
  | --- | --- |
  | 1 | 251.33 / 251.35 |
  | 2 | 250.10 / 250.12 |
  | 3 | 241.23 / 241.26 |
  | 4 | 257.64 / 257.66 |
  | 5 | 243.63 / 243.66 |
  | 6 | 258.95 / 258.97 |

  Raw pairs: `serial-mixed/kwin-owner-journal-full.log:509,552-553,583,626-627,655,698-699,727,770-771,799,842-843,871,914-915`.
  Across 12 window rows: min 241.23, p50 250.12, p95/max 258.97 ms.
  Across six command maxima: min 241.26, p50 250.12, p95/max 258.97 ms.
  Quantiles use nearest rank, `ceil(p*n)`. Exact native-event distribution
  remains BLOCKED with these artifacts; no new timing instrumentation was added.

### Collection limits and remaining acceptance

- The undelivered attempt had an initially backgrounded socket assignment and
  a daemon started immediately before injection; neither establishes its cause.
  The changed approach used a persistent private daemon before setup/build and
  geometric physical controls before the successful mixed burst.
- Rejected setup arguments, helper scope/nonce errors and serial-run fixture
  recreation preceded accepted input. Nested teardown's process-name query
  matched its own shell; exact owned-PID absence receipts were retained instead.
  These are external test-tool failures, not product fixes.
- Full current Planner and unfiltered owner-PID journals, separate scripting
  error queries and runtime-log copies survive. Runtime globs include historical
  logs and are not all current-run streams. First-run copy order is supported by
  file times, without a dedicated contemporaneous order marker; later copy
  receipts and reported ordered tool calls establish stronger provenance.
  Some launch/prefix/absence-command details remain execution-reported.
- Saved teardown includes DOWN, fixture/daemon PID and socket absence, unlocked
  readbacks and helper absence. An initial `unload:false, isScriptLoaded:false`
  proves absence, not successful unload. Later helper receipts report successful
  unload and absence. Explicit key-ups/traps are not independent global
  modifier-state observations.
- This note stays active. Remaining: physical even/odd float bursts after the
  correction below; genuine empty-target D2 crossings; direct nested structural proof
  if required; exact native-event timing; user-owned perceived latency and
  loaded-effect visual flicker checks. No Windows/macOS behavior changed.
- Final sequential offline gates exit 0: Rust formatting, three-package
  all-target build/strict Clippy, permitted Rust tests (3,455 passed across
  71 summaries), KWin typecheck/tests (1,351 passed across 199 suites), dev-loop
  launcher checks (703 passed) and native-effect launcher checks (204 passed).
  Protected fixture families and backlog are unchanged.

## Float FIFO correction (2026-11-16)

- The earlier rapid-float BLOCKED row records the pre-decision investigation,
  not the selected behavior. The clarification is now the
  [platform-independent ordered-core rule](../decisions/commands.md#ordered-core-commands-and-latest-world-application);
  KDE filing includes Meta+G without busy refusal. Native acceptance remains
  incomplete.
- Initial failing-first adapter regressions reject both a second parked press
  and presses during a capture. Removing the fence passes those cases through
  the retained production broker. Review then exposed press-time float-frame
  carriage and eager maximize clearing against a different execution-time
  subject. The pending-work branch now queues without those native reads or
  writes. Idle B9 unfloat and internal drag-verdict continuations retain their
  paths. A source scan found no other ordinary user-command busy fence.
- A start-floating discriminator exposed a shared-core defect: move a float
  to a distinct frame, then queue unfloat and re-float in one capture. Both
  commands filed, but the final publication was tiled. The adapter-free
  regression proves the second command returned another unfloat (`rect=None`)
  rather than re-float at the retained frame (`red-same-batch-float.log:328-340`).
  Earlier attribution to a not-tiled refusal was incorrect.
- Cause and correction: batch domain rebuilding used the initial derived
  floating classification. Convergence before the next toggle could re-float
  the just-unfloated member, so the next toggle unfloated again.
  `world_run_v4.rs` now restages classification and rebuilds domains after
  successful command bookkeeping. Core-only regressions cover both starting
  modes, moved placement, exact per-command results and single-command batches.
- Review caught native echo evidence lost by the newly reused classification
  helper. `world_observe.rs` now preserves the current report's `echoes`.
  A genuine pending-view-switch regression first failed `None` versus the native
  workspace, then passed with the repair. Independent focused review closed
  this finding; per-report intake and mapping gates still bound echo authority.
- The invalid maximized-focus regression setup was repaired, not removed:
  native-preimage capability, settled maximize and a real cross-output neighbor
  establish the focus shift before testing queued float. Tests now assert
  `geometryWrites=[]`, not merely final-frame equality, for net-zero float
  batches. Generic genuine-broker supersession coverage continues to withhold
  stale unissued setters; these are offline native-surface doubles, not live
  rendering proof.
- All eight latest sequential gates exit 0: Rust formatting, all-target
  three-package build and strict Clippy, 3,459 Rust tests across 71 summaries,
  KWin typecheck and 1,357 tests, launcher 703 and native-effect launcher 204.
  One formatting-only failure was repaired before rerunning these gates.
  No frozen fixture edits. Shared behavior now uses post-command classification
  between batch commands; no macOS/Windows native acceptance is claimed.
- Tier 1 tagged source citations and the selected outcome are in the
  [reference discriminator](../spec/reference-outcomes.md#kde-queued-execution-time-subject).
  Physical even/odd Meta+G bursts and bounded D2 crossing verification remain
  pending. Existing timing and visual-acceptance qualifications are unchanged.

## Post-correction float collection stop (2026-11-17)

- Outcome: INCOMPLETE. The four-press tiled-start burst below supplies a bounded
  behavior observation, not compliant completion of float acceptance: fixtures
  were created on the existing current workspace, not an appended workspace
  beyond the roster. No product defect or code correction is established.
  Raw artifacts are retained externally as `ri-float-20261117-01`.
- Exact fixture sentinel/PID/native UUIDs join in `fixture-spawn.json:1` and
  `native-pre-read.json:1`. Private settings disable hover and select
  group-with-neighbor (`settings/settings.json:1`); private intent storage and
  exact fixture scope were used. Physical move-right/left controls reached the
  real shortcuts before the burst (`control-callback-check.txt:3-6`).
- Tiled-start even observation: four Meta+G callbacks and ordered queue
  identities 3-6 appear in `float-even-kwin-journal.log:1-22`. Captures drain
  batches of 1 and 3 (`:5,25`); each distinct identity has terminal Take READY
  with kind `-` (`planner-float-even.log:141-166`). The final projection is tiled
  for both fixtures (`float-even-kwin-journal.log:89-91`); both native frames and
  active fixture agree (`native-post-float-even.json:1`). Revision 13 issued
  intermediate native work; revision 14 was superseded during persistence
  (`float-even-kwin-journal.log:29,55-56`). This is not zero-intermediate-write
  or rendered-history proof.
- STOP, tiled-start odd attempt: input could not connect to the daemon
  (`float-odd-ydotool.stdout:1-2`); the corresponding journal slice has no
  callbacks or queues (`float-odd-kwin-journal.log:1`). Subsequent native readback
  has no fixture windows (`native-post-float-odd.json:1`). The fixture and input
  process exit causes are UNKNOWN. This is an undelivered attempt, not a product
  FIFO failure. No further burst ran. Starting a replacement input daemon after
  the stop was a collection error; it was stopped without another burst.
- NOT RUN: floating-start even/odd and genuine empty-target D2. Two outputs were
  present (`outputs-pre.txt:1-3,28-30`), but the failed input attempt stopped the
  leg before staging or exercising D2. Output presence alone proves no crossing.
- Native-event timing remains OPEN. A genuine `frameGeometryChanged` one-shot
  exists in `kwin/src/world-entry.ts`, but it tracks admission-pending identities
  (`noteAdded`, `noteSend`, `setGeometryFor`), not an independently armed timing
  sample for every shortcut. This burst has no paired `ev=write` / `ev=observed`
  timestamp sample. Its `frame-attempt` rows
  (`float-even-kwin-journal.log:42-43,81-82`) are not native-event latency; no new
  instrumentation was added.
- Collection limits: `kwin-owner-journal-full.log` includes earlier generations,
  not just the current cursor window. The helper omitted exact script-object
  introspection before running the returned ID. Eight file-backed helper unload
  receipts do prove unload and absence (`receipts/helper-lifecycle.log:1-8`).
  The Planner log was preserved, but the runtime dev-log glob was not copied;
  `copy-before-off-receipt.txt:1` is not proof of those missing files. Do not
  treat this collection as a fully conforming live acceptance run.
- Controller/Planner teardown is recorded in `dev-off.txt:38` and
  `dev-status-post.txt:1-9` (unowned Planner, unloaded controller, DOWN).
  Fixture/input process absence and final unlocked state were checked but
  process-absence receipts were not saved. Fixture shutdown was not graceful:
  the process had already exited. Input and fixture exit causes remain open.
- Bounded collection diagnosis: socket refusal and empty native fixture roster
  were already observed before teardown, so `dev-off` did not cause either loss.
  The reported launch commands put both processes in short-lived shell
  background jobs without detachment or retained wait status. That lifetime
  plumbing is defective, but no saved exit code or signal proves that shell
  cleanup caused these exits. The fixture stderr has no terminal cause; the
  input stderr warning is not an exit receipt. No pre-failure kill is reported.
- Current offline verification passes without source or frozen-fixture changes:
  Rust formatting, all-target build and strict Clippy, 3,459 permitted Rust
  tests; KWin typecheck and 1,357 tests; launcher `PASS=702 FAIL=0` and
  native-effect `PASS=204 FAIL=0`. The launcher source is byte-identical to the
  recorded 703 baseline at `0bd14b51`; no intervening test change explains 702.
  The SIGTERM follower-cleanup test introduced in `3b4e63e3` increments PASS once
  per recorded follower PID (`scripts/dev-loop-split.test.sh:2121-2127`), after a
  readiness check requiring only nonempty PID files (`:2085-2089`). Thus its total
  is scheduling-sensitive, not a fixed assertion count. The old per-PID files
  were not retained, so the exact historical one-PID difference is unproven.
- Remaining: compliant even/odd float bursts from both starting modes, genuine
  empty-target D2 crossings, valid native-event timing, user-owned human-keyboard
  feel and loaded-effect visual flicker. The earlier inferred nested grouping
  is still not direct structural proof. This note remains active.

## Appended-workspace retry and native mismatch (2026-11-17)

- Outcome: FAIL on floating-start even native convergence; overall acceptance
  remains incomplete. External bundle `ri-retry-20261117-02` retains the exact
  step script, current-owner cursor journal, native readbacks and process exits.
  No product fix or extra burst followed the first mismatch.
- Setup now appends beyond the complete original roster
  (`desktop-append.json:1`); both fixture sentinel/PID/UUID joins and appended
  membership are retained (`fixture-spawn.stdout:1`, `joins.json:1`,
  `native-spawn.json:1`). Private settings/intent and exact fixture scope are
  used. One persistent input daemon starts before setup (`events.jsonl:1`).
  Single-chord float/tile controls each reach callback, queue, terminal READY
  and native parity before the bursts (`events.jsonl:186,213`).
- Tiled-start even: four callbacks/queue identities 3-6 in press order
  (`tiled-even-journal.stdout:1-28`), with distinct successful READY terminals
  (`planner-final.log:121,140-144`). Final tiled revision 12, both native frames
  and active fixture agree (`tiled-even-after-state.json:1`).
- Tiled-start odd: three callbacks/queue identities 7-9 in press order
  (`tiled-odd-journal.stdout:1-19`), with distinct successful READY terminals
  (`planner-final.log:187,205-207`). Final floating revision 16 and retained
  frame, sibling full tile and active fixture agree
  (`tiled-odd-after-state.json:1`).
- Floating-start even: all four callbacks/queue identities 10-13 reach successful
  READY (`kwin-owner-current.stdout:684-711`,
  `planner-final.log:252,272-276`). Revision 17 issues the intermediate tiled
  geometry (`kwin-owner-current.stdout:700,713,732`); its genuine native frame
  event reaches that tile (`:745`). Revision 18 retains final float `op-13` at
  the original floating frame (`:737`), but is superseded during persistence
  (`:747`). Revision 19 then reports `float-abandoned cause=external` with no
  pending float (`planner-final.log:280-283`). Through revision 22, core remains
  floating at the retained frame while native A remains at the intermediate
  tile (`kwin-owner-current.stdout:858,880`,
  `floating-even-after-native.json:1`). This is a real joined native/core
  mismatch, not input loss. The precise settlement correction remains pending.
- STOP is recorded at `events.jsonl:358`. Floating-start odd and D2 are NOT RUN.
  The existing second output's current had one foreign normal occupant
  (`floating-even-before-state.json:1`), so output presence alone would not
  satisfy D2's empty-target precondition.
- Native-event timing is now sampled by a passive fixture-joined
  `frameGeometryChanged` callback using `Date.now()`, not delayed reads.
  Single float and tile controls reach their final native frames 83 ms and
  50 ms after the callback (`kwin-owner-current.stdout:278,317,350,391`).
  Tiled-even final tile arrives 187 ms after first callback / 175 ms after last
  (`tiled-even-journal.stdout:1,26,102`); tiled-odd final float arrives 94 ms /
  84 ms (`tiled-odd-journal.stdout:1,12,59`). These are wall-clock
  callback-to-native-frame samples, not compositor presentation, animation
  completion, human feel or flicker evidence. The failing leg has no final
  matching float event.
- Teardown is directly receipted: explicit key-ups and unlocked hint
  (`events.jsonl:359-362`, `lock-final.stdout:1`); every helper unload verifies
  `isScriptLoaded=false` and child absence, with the persistent probe removed
  before the older controller (`events.jsonl:367-371`). Fifteen actual runtime
  dev-log files are copied with sizes/hashes before `dev-off`
  (`events.jsonl:372-375`); these include historical logs, while
  `planner-final.log` and `kwin-owner-current.stdout` are current-run evidence.
  Controller is DOWN (`dev-status-post.stdout:1-9`), fixture exits 0 with
  PID/socket absence (`events.jsonl:379-380`), and daemon exits on the owned
  SIGTERM with PID/socket absence (`:460-461`). Original desktop IDs/current
  are retained/restored and the explicitly appended test desktop is removed
  (`:458`, `native-final.json:1`); two controller-created trailing desktops
  remain as normal lifecycle churn, not display-configuration changes.
- Remaining: correct the floating-start latest-world convergence defect,
  rerun all four parity legs, then prove empty-target D2 crossings. Native
  event samples do not close human-keyboard feel, loaded-effect visual flicker
  or the earlier direct nested-structure gap. No source or frozen fixture
  changes were made during this retry.
