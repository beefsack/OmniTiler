# Core wire boundary (C5)

**Approved 2026-11-18; shared boundary completed 2026-11-19.** [C5](../core-v4-review.md#c5-the-json-wire-format-lives-inside-core) preserves behavior except for the changes recorded below. It retains C2's single report transaction and C4's settlement engine without compatibility shims, host policy moves or unrelated port changes.

## Ownership and admission

| Owner | Responsibility |
| --- | --- |
| `tiler-core` | Typed report + ordered command list, world policy, typed holds and refusal reasons, structural limits, replay and one candidate/install point. State owners supply facts and counts (e.g. outstanding settlement slots and failed rows), never JSON byte estimates. |
| `tiler-protocol` | JSON spelling, parsing, held-row wire reconstruction, canonical bytes and 4 MiB request/reply policy. Move the live `world_sizing*` responsibilities here; keep one canonical writer for exact counting/encoding. Keep conservative preflight arithmetic only where it serves a distinct early-refusal or future-reserve purpose. No core dependency on protocol. |
| Broker and adapters | Broker owns transport binding, wire replay, command parking/settlement and delivery. Native adapters produce and consume typed values directly. Neither path takes over core policy. |

- Keep `world_admit_v4` checkpoints and refusal precedence. Use synchronous
  preflight and final preparation over borrowed, read-only inputs behind a
  scoped, format-neutral gate. Check raw wire request length before decode.
  Distinguish conservative preflight and future terminal headroom from the
  actual response bound. Keep structural reserves in core (e.g.
  `switch_settle_reserve` and `operation_settle_slot_count`); protocol converts
  them to bytes, crediting emitted rows once. No fixed worst-case envelope
  charge on actual replies.
- `world_broker.rs::finish_capture_outcome` currently settles commands and
  installs staged gesture pins before the final bound size check (line 1049).
  Make final preparation the last recoverably fallible step: validate the
  typed result, reconcile staged pins using the candidate's active gesture
  ID (not its optional preview ID), and prepare the renderer projection and
  actual bound bytes (associations, source, watermark and clock). One writer
  chooses canonical spelling and preserves key order; construct both outputs
  from the same typed result, without reparsing replies or giving the
  projection write authority. Check the bytes that will be returned.
- Source replay resolves before drain. A fresh attempt may spend broker
  source/request/observation sequencing, not the accepted watermark. On late
  oversize keep the existing unremembered
  `WorldServiceError::Unavailable("reply exceeds size bound")`; discard the
  candidate, staged pins and projection. Leave the last
  accepted world, replay entry, watermark, paint coverage and unexecuted FIFO
  untouched. Retry uses a fresh broker source. The generation-shared group
  allocator may burn IDs as today.
  Wake-epoch exhaustion is namespace rotation, not a retryable gate refusal;
  handle it before publishing any partial settlement.
- On success install the world/replay and prepared broker state, settle only
  the issuing batch, and return the already-prepared bytes. Bootstrap can
  install pending intent without advancing replay; a defensive duplicate
  returns the cached publication with no transition. Neither proves execution
  of the drained batch. Stage under the existing service lock; no broker
  clone, re-encoding after adoption or second commit protocol.
- Replace raw held-row field lookups (`nativeWs`, `pre`, `post`, `kind`) and
  `decode_held_last_write` with typed hold evidence. Preserve the native order
  length even on malformed rows (`held_native_order_len`). Keep raw maps,
  wire ordinals and reconstruction on the protocol side.
  If malformed payload differences currently distinguish replays, preserve
  them with protocol-owned exact evidence, not a digest alone or a raw map in
  core. Preserve narrow holds, diagnostics and capability refusals; do not
  equate a malformed row with no observation. Type core refusal reasons and
  map them to existing tokens and messages at the boundary.

## Wire and command vocabulary

- Replace `frameAttempts`, `memberAttempts`, `viewAttempts`, `clearAttempts`,
  `stateAttempts` and per-window `lastWrite` with one required
  `propertyAttempts` array (`[]` when empty). Use a kind-tagged union: frame,
  membership, native view, maximize-clear, native state and visibility. Keep
  kind-specific subject/correlation, target, raw setter/readbacks and stamps;
  view is output-scoped and no-call dispositions are kind-specific. Translate
  `lastWrite: null` to absence. Decode to the canonical typed vocabulary.
  Count all rows, including held rows: 1,024 each for frame, membership,
  view, clear and state; 10,000 for visibility (the old fact ceiling);
  15,120 total. Preserve duplicate keys and handling: frame by window,
  membership and clear by operation/window, view by operation, and state by
  window/property (strict refusal for malformed or duplicate state).
  Apply each kind's subject and duplicate key, including identifiable held
  rows. Keep fixed section/kind diagnostic order and producer order within
  a kind; first hard error wins. A single wire ordinal reconstructs input,
  not settlement order. Creation keeps its own ordinal; creation and focus
  feedback stay separate, and arrival brackets stay nested read-only proof.
- One visibility attempt per exact window lifetime joins to a live fact or
  held window in the report. A readable sample on a held window must still
  be graded, not discarded or granted authority just for parsing.
  Identifiable malformed evidence beside a matching window holds that scope
  without disabling unrelated windows. Reject unidentifiable rows without
  inventing scope.
  Duplicate keys are a structural check at ingress for every kind and apply
  whether or not the window is present. A duplicate refuses even for absent
  or close-only windows. Rows with no live or held window are ignored only
  after they pass the structural checks, with no authority or diagnostic.
  For visibility, duplicate rows for the same lifetime, including an
  identifiable malformed row, refuse `snapshot-invalid` with
  `duplicate-visibility-attempt`. Malformed visibility samples formerly
  tolerated on held rows (extra keys, zero-padded counters) now hold narrowly
  under the shared rule.
- Preserve `attempts_clock_valid`: check comparable canonical stamps on
  frame, membership, view, clear and creation, including rows later held.
  Do not invent stamps for visibility or state. KWin keeps its per-kind
  delivery bookkeeping; retire only exactly acknowledged evidence so a
  newer sample arriving in flight survives.
- Replace `CoreCommand` string payloads with semantic values: `Direction`,
  a closed resize selector distinguishing inward/outward from axis modes,
  typed window/output/workspace identifiers, and a typed optional second
  pointer axis/boundary pair. Remove empty-string sentinels from typed paths:
  explicit by-id, genuine no-focus directional and execution-time focus are
  distinct. Follow string plumbing into the surrounding `WorldCommand` arms
  where it would otherwise reintroduce parsing. The public tokens and accepted
  forms do not change: decode once on entry (including broker queue ingress),
  and let native producers construct values directly. Preserve refusal
  scope/precedence and FIFO subject resolution. Put explicit subject types
  only on commands that use them; remove the `Implicit` wrapper without a
  second command hierarchy. In particular axis modes
  accepted by today's codec and a same-axis second pointer direction must
  remain representable so domain and held-scope refusals still take
  precedence. Newtyped IDs need not imply early validation for native
  producers where that would change refusal order.

## User decisions 2026-11-18

1. **Wire cutover (A):** Amend pre-release v4 in place. The codec, broker
   and KWin update together; mixed versions are unsupported.
2. **Batch replay (A, final):** Retain C2 report-only core replay matching.
   The conditional B criteria were not met: bounded exact submitted-batch
   identity and broker FIFO repark with next-fresh-capture recovery from a
   changed-batch refusal were not demonstrated. These distinct identities
   explain the boundary and the rejected expansion:
   - **Broker source:** exact incoming envelope bytes replay the remembered
     response and original clock before drain, without attaching new input.
   - **Core request:** Exact report retry returns the cached reply without
     new settlements, even if the separate queued list changed. A changed
     report at the latest sequence still refuses `request-reused`, with
     full equality and high-water fencing. Conditional B would also have
     compared every submitted `(global sequence, typed command)` in order,
     including skipped entries; a 128-entry limit alone would not bound
     variable-length members.
   - **Malformed evidence:** the checked protocol admission path, including
     direct wire calls, compares exact bounded residual evidence before a
     duplicate can be returned. Distinguish decoded content, not whitespace.
     Replace/reset with the one core replay entry, never on final refusal;
     no raw maps in core or independent replay authority.
   [Reference outcome](../../spec/reference-outcomes.md#typed-report-replay-with-a-changed-queued-list).
3. **Native size limits (B, user 2026-11-18):** Keep the protocol size gate
   for in-process callers by default, without round-trip decoding. Revisit
   only if measurements justify removal: 10,000 facts x 4,096-byte titles
   is about 39 MiB of title bytes alone, not a peak estimate. Row counts are
   not a memory bound. Account for variable fields, nested lists, reply
   fan-out and retained copies; check cheap structural bounds before extra
   copying, hashing or expansion. Distinguish caller-owned input from
   added engine work; do not tighten shared behavior silently. In either
   case keep the same semantic checks before adoption, including those
   applied by `decode_reply_v4_for_host`: unsupported above, warp and native
   sticky, plus report grading and defaults such as missing monotonic clock
   and creation receipt null versus absence.
   [Discriminator](../../spec/reference-outcomes.md#typed-native-capture-over-the-json-budget).
4. **Visibility attempts (user 2026-11-18):** Apply the shared
   property-attempt rules above for subject, duplicates and diagnostic order.
   [Discriminator](../../spec/reference-outcomes.md#visibility-attempt-shared-rules).
5. **Final reply and terminal headroom (user 2026-11-18):** Drop the
   estimate-only final refusal. Keep early preflight, staged structural checks,
   the exact canonical inner checkpoint, future reserve, wake-epoch rotation
   and the actual bound check in their existing relative order. Exact inner
   bytes plus terminal headroom over 4 MiB still give a remembered
   `reply-oversize`; actual bound bytes over 4 MiB still give the separate
   unremembered service refusal. Replace estimate/twin assertions with exact
   writer-count equality, add one estimate-only discriminator, and retain
   early, exact and future-reserve rollback coverage. No fixture byte changes
   for these decisions.
   [Discriminator](../../spec/reference-outcomes.md#staged-reply-size-versus-terminal-headroom).
6. **Platform scope (user 2026-11-19):** Finish and prove the core and
   protocol rebuild first. macOS and Windows ports and their checks are
   deferred to separate rebuilds on their native platforms; do not add
   compatibility shims or shape core around the deferred callers. KDE is
   also slated for a rebuild. Migrate the current KWin code here where it
   provides a direct, straightforward consumer proof of the contract;
   defer substantial KWin changes to its rebuild.

## Tentative decisions pending user review

- Decode visibility after native state and before focus for first-hard-error
  precedence. This puts visibility errors after frame, membership, view,
  maximize-clear, creation and state errors. The alternative is visibility
  first, preserving its former window-section precedence. Appending the new
  kind keeps the existing kinds' relative order. Reversal moves visibility
  to the front of the attempt decode order and flips the visibility cases
  in `v4_cross_section_and_kind_refusal_precedence`.

## Migration, acceptance and cost

1. First extract the canonical writer and close the bound-response commit
   gap with unchanged wire bytes. Keep round-trip checks in tests. The
   outer broker envelope is a contract discrepancy: `parse_capture_envelope`
   materializes `serde_json::Value` before strict inner decode, so duplicate
   keys can collapse. Do not treat that as a valid spelling.
2. Next cut over command types and the approved attempt grammar in shared
   Rust and KDE together:
   KWin `world-entry.ts` drains the same evidence, `world-report.ts` writes the
   tagged lane and the service uses the bounded bytes returned by admission.
   `crates/omnitiler/examples/raw_world_eval.rs` remains a checked wire caller.
   Keep the adapter contract typed without codec round trips; existing macOS
   and Windows callers are out of scope. Tabs need no new fields: report
   lifetime-bound setter/readbacks, not tab verdicts or a separate hide
   ledger. Keep future exact group/member clicks distinct from
   execution-time focus. Retain report-only core replay as specified above.
3. Frozen `world-wire/v4`, `stage5`, `stage5-v4` and `kde-v4` stay
   byte-identical, even if their historical shape is rejected by the active
   codec. Use `world-wire/v4-active`, `stage5-v4-active` and `kde-v4-active`
   for approved shape changes; update normative wire tables and active
   artifacts together, including raw transcript inputs outside these
   directories. Re-record, never patch historical bytes. Keep unrelated
   canonical bytes unchanged (key order, escaping, counter strings, replies
   and handshakes for equivalent inputs). Keep both stage5 production-route
   proofs enabled.
4. Reuse `connection_transcript_v4`, `raw_attempt_settlement_v4`, `world_eval_v4`,
   broker, rapid-input, stage5 and KWin byte/producer tests. Queue A then B
   with a gesture and an inner-fits/bound-exceeds response: check the exact
   service error, unchanged read-presentation/coverage, no settlement or
   new pin, then A and B executing once in order on the next fitting fresh
   capture. Include a neighboring fitting case. Exercise readable visibility
   on held A, malformed evidence for B and healthy unrelated C; compare
   holds, diagnostics, settlement and C's authority after reordering.
   Cover absent and close-only subjects, duplicate refusal and mixed-error
   precedence with the decided discriminator. Verify
   exact and conservative bounds, future reserve, next-report recovery,
   FIFO evaluation of every rapid keypress in core with latest-world-only
   platform apply, all six kinds including malformed/capability cases,
   and unchanged command ordering/refusal. Add focused boundary cases only
   for genuine gaps; no new comparison harness. Run scoped Rust format/build/
   Clippy/tests and KWin typecheck/tests, scoped to the supported gate
   packages rather than the deferred native ports. No live test is required.
5. Keep `scripts/check-report-cost.sh`: 24M/100M Ir ceilings at 50/200
   windows and 6.5 growth; compare against C2's 8.11M/35.58M Ir (4.39).
   Run release `report_scale all 31 3 all`: steady p95 <= 1/4/10 ms
   at 10/50/200 windows, first reports separate. Counts after extraction
   exclude moved wire work. Compare before/after full broker capture through
   returned bound bytes on an existing workload. Prefer simplicity absent
   measured need. One report, one publication; no new IPC, timers or
   acknowledgements.
   Border following stays local to an authorized subject; core selects
   preview targets. Serialization does not change paint scheduling.
6. Split the touched codec by responsibility, not just move a monolith
   (`world_protocol.rs` is about 9k lines today). Use the soft 1,500/3,000
   production/test line caps; explain remaining exceptions, no new size gate.
   Treat lost holds, non-atomic oversize refusal, byte drift, stale mixed
   producers and command loss as blocking. Record new behavioural ambiguities
   as minimal TBD sequences before changing them.

Evidence is source inspection plus archived C1/C2/C4 results. No design-phase
build, test, timing or native acceptance is claimed.

Implementation baseline: release Stage 5 genesis through
`WorldService::evaluate_capture`, including the returned bound bytes and
excluding discovery and reply inspection. Seven series of 300 fresh accepted
captures per arm. Median series p50/p95: 319.6/331.6 us with a new broker per
capture, 320.0/327.4 us with a warm broker and advancing source and revision.
Replies were 2,221-2,237 bytes. Compare distributions on the same semantic
workload after cutover.

Atomic publication and the shared canonical writer are in place. The
inner-fits/bound-exceeds case uses the real service final check with a
test-only lower bound and verifies unchanged coverage, pins, watermark and
parked FIFO before fresh recovery. Both historical orderings (protocol table
order and broker sorted objects) match the existing byte oracles. Interim
broker median series p50/p95 was 189.9/196.1 us fresh and 198.6/206.6 us warm
on the baseline workload. Core byte-policy extraction resumes with the
terminal headroom decision above.

The broker and raw evaluator now measure canonical inner bytes through the
protocol gate. The candidate carries that count into the terminal reserve
check; writer-only validation stays at final preparation. Focused tests
confirmed checkpoint order, writer validation deferral and atomic rollback.
Core no longer owns JSON sizing or transport byte limits. Its direct typed
entry checks structure; broker, evaluator and platform ingress retain the
protocol byte gate. Native caller cutover and verification belong to the
deferred platform rebuilds.

Command IDs and subjects are typed. The FIFO resolves each execution-time
focus subject once and uses it for dispatch, diagnostics and bookkeeping.
The active and frozen wire fixtures are unchanged; the scoped Rust suite
passed 3,458 tests, with the workspace unsafe-posture test excluded.

Native and attempt holds now supply typed evidence to core; protocol
retains raw fields for exact reconstruction, byte accounting and replay
comparison. The scoped Rust suite passed 3,464 tests with the workspace
unsafe-posture test excluded, including the Stage 5 production routes and
byte oracles. Active and frozen fixtures are unchanged.

The coordinated Rust and KWin cutover uses required `propertyAttempts`.
Focused checks preserved the broker clock gate, visibility grading and
narrow holds, KWin evidence retirement, and both Stage 5 production
routes. The scoped Rust suite passed 3,466 tests before the final
visibility assertion additions; the three affected suites then passed
99, 27 and 65 tests. KWin typecheck and 1,378 tests passed. The frozen
families are unchanged; only active report inputs were migrated or
re-recorded. KWin hide/show actuation does not yet produce visibility
attempts; that work belongs to the KDE rebuild.

For the final broker comparison, the release harness at `b6f097d0` used
the original active genesis fixture and the workload above. Median series
p50/p95 was 198.5/205.5 us fresh and 205.6/214.7 us warm; replies were
2,220-2,234 bytes. This is the comparison baseline; earlier timings above
are historical context.

## Completion evidence

Independent source review covered wire changes from the capture parser and
size-policy extraction through the six-kind cutover. It found no lost
discriminating coverage, core dependency on protocol, compatibility shim,
arithmetic twin or FIFO regression. Command-result spelling and queue
validation precedence are unchanged. Rapid commands still evaluate in
core order; KWin applies only the latest eligible world.

Two decoder tests cover 10,000/10,001 visibility and 15,120/15,121 total
attempt rows. The total case distinguishes the total guard from later
per-kind checks. The structural duplicate test now sends the same
readable/malformed pair for both absent and live subjects. No production
behavior changed during review. The final test-only lint correction
passed strict Clippy.

Scoped Rust format, all-target build and strict Clippy passed. Rust:
3,468 passed with `workspace_unsafe_posture` excluded; 3,447 at C2,
3,458 after the writer, sizing and typed-command units, 3,464 after typed
holds, 3,466 after the unified cutover, and two additional cap tests at
completion. KWin typecheck and 1,378 tests passed. Launcher 703,
native-effect 204 and helper 58 passed, all unchanged from C2. Neither
known flake occurred. Both Stage 5 production routes passed; all 651
frozen fixtures are unchanged.

Callgrind counted 7.72M/34.07M instructions per report at 50/200
windows with 4.41 growth (C2: 8.11M/35.58M, 4.39). All cost limits
passed. Release `report_scale all 31 3 all` worst steady p95 was
0.2871/1.9622/4.4720 ms at 10/50/200 windows (budgets 1/4/10 ms).
First reports, measured separately, ranged 0.1732-0.3786,
1.0027-3.3286 and 12.3155-42.5533 ms, respectively.

The release broker harness used the re-recorded active genesis and the
same workload as the `b6f097d0` comparison. Median series p50/p95 over
seven series of 300 captures was 203.0/211.1 us fresh and 210.5/218.7 us
warm, compared with 198.5/205.5 and 205.6/214.7 us. The increase was
4.0-5.6 us across the four measures. Replies were 2,221-2,222 bytes
fresh and 2,224-2,235 bytes warm, versus 2,220-2,234 bytes overall at
baseline. The recorded Stage 5 reply bytes are unchanged.

The codec now separates attempt rows, canonical reply output, row sizing,
admission and retained wire evidence. The remaining shared codec is
larger than the soft target because it still contains existing connection,
command, report and reply codecs outside the extracted responsibilities.
The stress harness remains intact, and the decoder suite slightly exceeds
its soft target to keep threshold evidence with the other decoder cases.

## Deferred work

- KDE rebuild: produce lifetime-bound visibility attempts from KWin
  hide/show actuation; no visibility-producer acceptance is claimed here.
- macOS and Windows rebuilds: adopt typed ingress and command construction
  and prove semantics and native-target behavior. No checks were run on
  those ports for final C5 acceptance.
- Duplicate object keys on queue/take/wait/presentation-read still resolve
  last-wins (unchanged since before C5); capture and discovery refuse them as
  malformed. Strictness for the remaining endpoints, and where malformed
  ranks against generation/high-water checks, goes to the protocol
  correctness review.
