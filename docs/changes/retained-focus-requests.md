# One retained focus request for every cause

## Status, goal and boundary

- Approved design; implementation and verification are partial, not native acceptance. User 2026-11-11
  approved one retained focus request for explicit commands, automatic admission,
  focus-follows-cursor and positive close, including macOS admission actuation
  and Q1-Q3 below. The frozen `afterShown` schema is defined below; runtime
  implementation is present, but full proof-plan mapping and native acceptance
  are incomplete. This is work in progress, not a fully passing workspace.
- Replace issuing-publication-dependent focus with one global pending slot,
  present in every latest publication until satisfied, superseded, ineligible
  or refused. Keep immutable correlation, report-derived settlement and one
  adapter attempt/no-call per id, never a retry. Core stays platform agnostic;
  adapters apply every host-allowed decision [E1].
- Use a new note: this broadens delivery and correlation beyond the accepted
  close-only scope. Preserve the close design's decisions and evidence rather
  than rewriting its historical non-goals [E2].
- No new candidate-selection policy, native focus clearing, automatic unhide,
  unminimize, fullscreen exit, input hook, timer or retry. Explicit selection
  and send-follow keep their existing view/arrival authority; ordinary automatic
  requests gain none. Raise/stack have no current producers [E3]; any future
  producer must retain pending work under the same latest-only rule, not revive
  issuing-only delivery. Persistent keep-above is a separate lane [E4].
- Coordinated in-place pre-release v4 amendment, no dual shape or version bump,
  as approved for this scope and previously for close focus [E2].

## Verified baseline

- Ordinary focus is generated into this run's `effects`, while later snapshots
  publish only their new vector. Historical grants remain internally pending:
  a test explicitly expects empty effects with a pending grant [E3, E5]. Close
  instead validates and republishes its retained request on every publication
  [E6]. Retaining bookkeeping alone does not make latest-only application safe.
- macOS ordinary dispatch requires the current explicit-focus ticket and sequence;
  admission is spent without actuation. Its close lane is ticket-independent but
  currently cancels on *any* command ticket [E7]. Neither gate can survive the
  generalization unchanged.
- The existing admission blocker is canonical B versus native A, not proof of
  a ticket-correlation bug. Its product alternative of preserving suppression
  is superseded by the approved actuation direction; implementation and native
  outcomes remain pending [E8].

## Approved wire direction and retained state

- Replace required nullable `closeFocus` with required nullable `focusRequest`;
  remove Focus from `effects`. The later reconciliation removes the unused
  raise/stack reserve too; `effects` is required empty. Reject mixed old/new
  focus authority, unknown causes and invalid cross-field combinations [E9].

  ```json
  "focusRequest": {
    "id": "focus-42", "target": "B", "rev": "42",
    "cause": "automatic-admission", "out": "L", "ws": "W1",
    "cmdSeq": null, "afterShown": null
  }
  ```

- Closed cause vocabulary: `explicit-command`, `automatic-admission`,
  `focus-follows-cursor`, `positive-close`. Only explicit-command has non-null
  `cmdSeq`; it identifies the originating command, never the currently applying
  ticket. `out/ws` name core's resolved activation scope; close retains its
  frozen source domain. Do not impose close's tiled/source-home restriction on
  explicit sticky/selection/send-follow focus [E6, E10].
- `afterShown:null` replaces ordinary `awaitShown:false`. Explicit selections
  and eligible follow/migration requests instead carry the existing parent
  operation and frozen application dependencies: issuing observation, required
  members with lifetime/home/frame, native-view targets and affected selected/
  departing scopes. This is a projection of existing selection state, not a new
  work id or a new selection policy [E10]. It replaces `awaitShown:true`, not
  the existing visibility, arrival, cross-output hide or terminal rules [E11].
  A later snapshot must contain these dependencies even if the parent transfer
  has settled; never recover them from an old ticket or reconstruct the frozen
  set from later window rows. A scalar boolean and target alone are insufficient.
- Keep one `pending_focus` slot, not one slot per cause. Selection bookkeeping
  may still retain visibility/terminal obligations, but cannot authorize a
  second focus attempt independently. Freeze the final winning request once
  per accepted publication, using a generation-scoped revision-derived opaque
  id. Re-emission never changes id, target, issuing revision, cause, command
  correlation or frozen dependencies. No id-prefix authority [E2, E9, E10].
- Retain core-only foreground/report-order baseline, view basis, cause-specific
  eligibility and first exact-tuple feedback. Adapter accounting uses
  `(adapter incarnation, core generation, id)` plus the immutable issuing
  revision. Reuse close's bounded current-record/retired-revision fence and
  fresh-handshake rule; no unbounded spent-id set or persisted activation [E6].
- Strict codecs, required keys, bootstrap nulls, replay/cache equality, exact and
  upper-bound sizing, section caps and designated v4 fixtures change together.
  Frozen dependencies must fit the existing whole-message bounds, never silently
  truncate. Existing close contract requires matching builds and bounds [E2, E4].

## FIFO commands, implicit subjects and completion

- Incorporate observations once at the capture boundary, then execute every
  command FIFO against post-previous-command core state without waiting for a
  native setter/capture. Explicit ids and empty directional subjects retain
  their distinct meanings. These are existing decisions, not a reason to force
  adapters through intermediate publications [E12].
- Maintain an execution-time logical focus cursor separate from native observed
  foreground and per-workspace history. Accepted focus choices advance it before
  native application, including admission, hover and close. A still-live retained
  request preserves its target across unchanged-baseline reports; stale observed
  A must not overwrite pending B. A newer incorporated focus change supersedes
  B and supplies the next subject. After refusal/ineligibility, clear pending
  authority and use current eligible observed focus (or genuinely no subject),
  not the abandoned target. Never fabricate native foreground [E6, E12, E13].
- Resolve a new implicit command's subject from that cursor *before* retiring
  an older request for a newer focus decision. Otherwise supersession itself would
  erase the execution-time subject. Within a batch each accepted focus choice
  advances the cursor immediately, even though only the final winner publishes.
  Same-capture admission must enter this ordering before commands, rather than
  being backfilled only at final publication [E3, E13]. Non-focus commands never
  cancel pending focus, in the same batch or any later batch (Q1).
- Keep command consumption/results and per-sequence batch settlements separate
  from native focus completion. A result may be null, rejected or issuing-only;
  the retained request is authority. Re-emission does not reexecute a command,
  regenerate its result or need its ticket. Continue settling each queued command
  from its own result, not from final foreground [E12, E14].
- Keep selection arrival readiness and proven foreground distinct, retain the
  original required set and existing parent completion, and preserve focusless
  selection completion. Satisfying the global focus slot does not fabricate
  missing arrival/visibility evidence or a SwitchEvent. Superseding focus cancels
  its unstarted authorization, not unrelated member/visibility accounting
  [E10, E11].

## Settlement and per-cause fences

- For an accepted report, target equality satisfies first. Otherwise correlate
  raw `focusFeedback` by exact `(id,target,issuing rev)` and latch the first row/
  original stamp. Proven refusal or informative safety no-call retires. Accepted
  dispatch with unchanged/non-target readback or lost feedback retains an inert
  row; re-emission never authorizes another call. Late evidence for retired work
  cannot restore it [E6, E11].
- A newer focus decision of any cause, an observed user focus change, or a
  view/output change or target loss making the target ineligible retires the old
  request. Non-focus commands never cancel it, including refused commands without
  a focus decision, in any batch. Satisfaction/refusal and cause-specific
  eligibility settlement remain unchanged.
  A distinct newly qualifying focus event replaces the old slot; observing the
  same pending event again does not mint another id. Same-report precedence is
  explicit-command, automatic-admission, positive-close, focus-follows-cursor
  (Q2); fresh qualifying hover entries rearm with a new id (Q3). Unchanged
  observation revisions do not retire it.
  Freeze the issuing report's truthful foreground baseline for later comparisons,
  never treat that issuing baseline itself as a newer focus event. Keep close's
  approved same-report
  fallback attribution and unknown-foreground retirement; do not infer user input
  from time or promise detection of between-capture A->B->A [E1, E2, E6].
- Revalidate the issuing cause's policy in core, not close's entire automatic
  predicate copied onto explicit focus. Existing selection arrival holds remain
  holds under their rules, not a new automatic-focus cancellation on an expected
  not-yet-shown destination [E10, E11].

| Cause | Origination and retained eligibility |
| --- | --- |
| Explicit command | Preserve direct/directional focus, remembered selection, send-follow and operation focus producers and execution-time subject gates. Explicit focus/send-follow retain the automatic-focus exemptions; independent native identity/session/game safety still applies. Awaited view/visibility authority comes only from the existing selection dependency, not the focus request alone [E10-E12]. |
| Admission | Preserve startup quietness, complete-inventory newcomer selection, last non-withheld newcomer per batch and spending of automatic-fixed newcomers. Ordinary capacity-float newcomer can win; never focus an alternate anchor or focus for repair/eviction. Preserve focused float/sticky, fullscreen/maximized sibling, game, gesture, hidden/minimized/excluded and held-scope fences. A withheld or retired birth cannot rearm on restore [E15, E16]. |
| Focus-follows-cursor | Setting remains off by default. Require genuine observed cursor hit/movement or hover transition, shown/readable membership, inventory and game/gesture fences; static cursor cannot re-steal after keyboard focus. Preserve the normative automatic target restrictions in focus.md:4; current hover code's ordinary-float/overlay eligibility is not approval to broaden them. Each fresh qualifying hover entry after leaving and returning gets a new id; unchanged reports or pointer movement over an already-pending target keep the id and never retry (Q3) [E15-E17]. |
| Positive close | Preserve pre-convergence source tiled MRU selection, whole close-cohort exclusion, ordinary-float-close handling, no alternate survivor, no other-domain focus/clear on sole close, and all shown/readability/lifetime/automatic/fullscreen/game fences. Same-report fallback is not a newer user-focus event. No repeated correction for the same close [E2, E6]. |

## Adapter changes and coordination

- macOS: generalize the existing close envelope -> bounded accounting -> owner
  slot -> fresh native readback -> raw feedback route for every cause. Remove
  ordinary current-ticket eligibility and the blanket command-ticket cancellation;
  core's latest request/null chooses what remains authorized. A command's own
  newly issued request must not cancel itself. Retain native lifetime/process,
  session, watermark, current Space, visibility and last-moment safety checks,
  with cause-appropriate exemptions rather than close-only fullscreen policy
  applied to explicit focus [E7, E15].
- macOS selection deferral uses retained `afterShown`/parent correlation and live
  native binding/arrival proof, not `selection_ticket_seq`. Publish the latest
  request epoch to cancel taken-but-unstarted old work. Spend before attempt or
  terminal safety no-call; stale delivery alone does not spend an otherwise
  unchanged request. Admission now actuates as a normal eligible request, with
  truthful feedback instead of silent suppression [E6, E7, E10].
- KDE requires coordinated adoption: its strict reply key set currently lacks
  even `closeFocus`, and its apply path consumes effects with some `af-` prefix
  guards [E18]. Add the new typed nullable lane/dependency validation, latest-only
  once-per-id accounting, causal feedback and supersession/null cancellation;
  move policy to core and keep only declared mechanical safety checks. Preserve
  native view/arrival accounting and host refusal, without applying skipped
  snapshots or suppressing KWin's own fallback [E2, E11, E18].
- Required handoff/backlog items before any eventual core/protocol publication:
  KDE unified-focus decoder/actuator/accounting/producer-codec journey adoption;
  Windows unified-focus consumer/feedback/arrival adoption; matching v4 fixture
  and conformance migration. KDE adapter implementation remains separate work.
  Do not label a mismatched adapter compatible [E2, E18].

## Proof plan and bounded implementation order

1. Q1-Q3 approval and minimal discriminator rows are recorded in
   `docs/spec/reference-outcomes.md`; reference/native outcomes remain TBD.
   Contracts/decisions describe the approved, unimplemented target. Define the
   exact bounded `afterShown` projection before implementing producers/consumers.
2. Implement shared retained-slot lifecycle/cursor and coordinated codecs/fixtures;
   preserve selection obligations and per-cause gates. Then migrate macOS to the
   common owner/feedback path and coordinate other adapter adoption.
3. For **each** cause, use genuine producer -> core -> byte codec -> production
   adapter dispatch -> mocked owner/backend. Obtain issuing R, do not deliver it
   to the adapter, ingest at least two unchanged-baseline reports, and apply only
   final R+n. Assert identical request correlation, one real mocked setter and
   satisfaction from truthful target observation. Never hand-build/remap the
   focus request, warm the adapter with R, or pass only a fabricated late envelope.
   The existing genuine close journey is the template [E19].
4. Repeat latest-only tests with newer observed focus, newer focus decision, null/newer
   request, cause-specific ineligibility and refusal before first apply: zero old
   calls. Cover accepted-unproven/lost feedback, duplicate replies, transport
   reconnect, lost-accounting fresh handshake, context/lifetime reuse, callback
   lateness, outbox no-call and bounded revision-fence compaction [E6, E19].
   Contrast non-focus/refused commands in the same and later batches: the pending
   request survives unchanged. Cover same-report cause precedence and fresh-hover
   reentry versus unchanged pending hover.
5. Cover selection issuing publication skipped, parent transfer already settled,
   late admissions not entering frozen arrival set, focusless completion, held
   scope recovery, follow/migration and cross-output hide barriers. A later command
   result/ticket must not be needed to actuate a still-live earlier request
   [E10-E12]. Native timing/physical acceptance remains separate.

| Minimal discriminator (promoted to the reference matrix) | Approved outcome / unverified boundary |
| --- | --- |
| Focus A; admit B while native stays A; skip issuing and two unchanged publications; apply only latest; next implicit action | One attempt for retained B; action resolves execution-time core focus, not stale native A. Replace the admission-suppression expectation, not FIFO semantics. Native outcome TBD [E8]. |
| FIFO focus-right then orientation or a refused non-focus command; contrast the trailing command in the next batch; skip issuing publication | Pending target survives unchanged in every batch; later implicit subject reflects the preceding accepted focus choice (Q1). Reference/native outcome TBD. |
| Positive close B selects C while ordinary newcomer N arrives in the same report | One retained winner only: N wins under explicit-command > automatic-admission > positive-close > focus-follows-cursor precedence (Q2). Reference/native outcome TBD. |
| Hover B; move over pending B; settle or refuse; move to C then back to B; repeat unchanged B reports | Movement over pending B and unchanged reports keep the id, never retry; fresh qualifying reentry mints a new id (Q3). Reference/native outcome TBD. |

- Migrate old empty-effects/pending-grant assertions into retained-request
  assertions with unchanged exact-tuple/first-feedback safety checks [E5]. Migrate
  close wire/sequencer/owner/epoch tests without dropping their stronger real
  producer journey [E19]. Update admission-nonactuation tests and batch/hover/
  selection tests to the decided semantics; do not blanket-change the other
  unattributed macOS focus failures to pass [E3, E8, E17].
- Durable target decisions are recorded in `docs/decisions/contract.md`,
  `commands.md`, `focus.md`, `macos.md`, `layout-and-admission.md` and
  `workspaces.md`; the wire/lifecycle target is in
  `docs/porting/world-contract.md#one-retained-focus-request`. The reference matrix
  contains the four discriminators. The port blocker now distinguishes the
  selected resolution from pending implementation/native proof. The earlier close
  note preserves historical implementation/evidence while marking its close-only
  wire and blanket-command supersession as superseded [E2, E4, E8, E12, E15].

## Approved product decisions (User 2026-11-11)

1. **Q1, non-focus/refused commands:** commands without a new focus decision NEVER
   cancel pending focus, in any batch. Only a newer focus decision of any cause,
   an observed user focus change, or a view change or target loss making the
   target ineligible supersedes it. This amends the earlier close-refocus rule
   that newer user focus/commands supersede. Normal satisfaction/refusal and
   eligibility settlement still apply.
2. **Q2, same-report causes:** highest first: explicit-command, then
   automatic-admission, then positive-close, then focus-follows-cursor. A positive
   close choosing tiled MRU C and a fresh ordinary newcomer N in the same report
   retain N, not C. Within-cause FIFO and existing newcomer selection remain.
3. **Q3, fresh hover rearm:** each fresh qualifying hover entry after the pointer
   leaves a live target and returns gets a new request id. Unchanged host reports
   and pointer movement over an already-pending target retain the same id and
   never retry. This replaces lifetime-scoped `ffc-<token>` spent identity.

## Frozen afterShown schema and implementation handoff

- `afterShown` is required nullable. Its non-null object has exactly
  `op,obs,members,views,selected,departing`. `op` is the existing parent operation
  Id; `obs` is the quoted decimal issuing observation Counter. `members` contains
  exactly `{win,out,ws,frame}` rows: WindowId (already a lifetime), frozen logical
  home and required nullable Rect. Null frame waives equality, not readable live
  geometry. `views`, `selected` and `departing` contain exactly `{out,ws}` rows.
  View targets use the selected home's own output, so a redundant target-output
  key is unnecessary. These are frozen inputs, not readiness booleans or evidence.
- Every object key is required; only `afterShown` and member `frame` are nullable.
  Arrays may be empty. Use existing Id/Counter/Rect grammars. Reject unknown or
  duplicate keys, duplicate member lifetimes, duplicate scope pairs and duplicate
  view outputs. Each view target must occur in `selected`. Non-null `afterShown`
  requires `explicit-command`; non-null `cmdSeq` is required exactly for that
  cause. No operation-id prefix is authority. Do not require the parent transfer
  to remain published, nor require frozen members to appear in later window rows.
- Bound `members` to the existing 10,000-window inventory cap; bound each of
  `views`, `selected` and `departing` to the existing 16-output cap. Preserve
  the existing independently bounded 4 MiB reply, strict decoding, exact encoding
  and conservative upper-bound sizing. Reject overflow before accepting mutation;
  never truncate dependencies. Freeze at the existing post-layout selection
  snapshot, retaining departing scopes from that issuing selection, and copy the
  projection into the winning request so parent settlement cannot erase it.
  Switch-member evidence and terminal bookkeeping remain separate core state.
- Partial implementation covers the core retained slot/logical cursor and four
  producers, strict protocol codecs and active v4 fixtures, and the generalized
  macOS owner/feedback route. Four per-cause producer/codec/mocked-owner journeys
  skip the issuing and two unchanged publications. Selection latest-only proofs,
  complete integration/lint verification and coordinated adoption records remain
  incomplete. This note stays active, not archived as implemented.
- Documentation review found no material inconsistencies with the approved
  decisions. Diff whitespace and added-text ASCII checks pass; historical close
  and port evidence is preserved. No runtime or native verification was performed
  for this documentation amendment.

## Implementation blocker: macOS selection arrival evidence

- `RosterEntry.observed` is cached geometry, not a lifetime/visibility/home/frame
  bracket with a capture watermark. `apply_roster_covering` can retain old rows
  when native visibility is unreadable. Reply emission coherence prevents missing
  submitted input; it does not turn cached member facts into fresh arrival reads.
- Source comparison at `cd5e9e82` found that selection release used verified
  Space arrival plus app-lifetime and foreground fences, without per-member
  arrival brackets; feedback arrival was empty. The cached-roster `Proven` gate
  is new, but missing member-bracket gating also predates this amendment. The
  correction adds stricter evidence rather than merely restoring an old bracket.
- Partial correction adds an ordered, owner-thread native bracket read over the
  frozen members only, with identity joins around visibility/home/frame reads.
  Evidence-only internal completions preserve the sampled observation and native
  context generation, never stamp buffered reads at engine apply time or replay
  an old native snapshot. Each owner attempts capture once per current request,
  watermark, context and app lifetime. Complete current captures gate the frozen
  proof; the original proof epoch is checked again before release and dispatch.
- Deferred selection authorization remains bound while an owner command is
  queued. An owner readiness deferral stages no terminal focus feedback and
  retains the unstarted request; actual owner results carry captured arrival
  rows. Terminal pre-queue failures retire only the exact intent; queue reservation
  precedes insertion, and stopped owners remove only their matching outbox row.
  The owner rechecks frozen expected native homes and actual output mapping.
- The two selection journeys now skip genuine core-issued replies, including
  issuing plus two unchanged publications, before first latest delivery through
  the byte codec and production consumer. The parent-settled variant settles
  core-side before that first delivery. Both capture fresh arrival evidence,
  invoke the mock setter once, process its generated feedback and settle from a
  genuine target-focus observation. A fixed late newcomer is admitted while the
  original request is pending without enlarging its frozen arrival set.
- Direct full macOS library verification after the lifecycle and home/output
  corrections: 1,553 passed, zero failed, exit 0. Subsequent focused verification
  reports cover multi-owner missing/all-current convergence, null frozen versus
  unreadable live frames, exact-frame mismatch, mid-read observation and context
  invalidation, owner/WID mismatch, app-generation replacement, and stale queued
  stamp deferral/recovery. The latest reported dispatch module run is 65 passed,
  zero failed, exit 0; capture-attempt accounting is five passed, exit 0. The full
  library must be rerun after these additions.
- Native arrival capture now revalidates process identity and compares the fresh
  AX subject with the exact element retained from the successful native fact
  read. Missing/replaced anchors withhold; omission alone does not retire an
  anchor. Proven closes leave bounded inert tombstones, never rebind an old
  numeric token. Focused anchor and native-fact tests pass; global native token
  reminting remains separate existing identity work. Native timing and physical
  acceptance remain unverified.
- Latest reported full macOS library verification is 1,570 passed, zero failed,
  exit 0. Its integration target remains red: 119 passed, 31 failed, exit 101.
  Protocol library verification remains red: 41 passed, 19 failed, exit 101;
  the stress driver still expects issuing-only Focus effects. These results are
  not acceptance of the coordinated change.
- Core library verification reports 2,140 passed, zero failed. Nine integration
  targets were subsequently migrated to truthful foreground provenance and
  retained-request assertions, preserving quiet-request and routing oracles;
  all targeted reruns pass. Core all-target strict clippy reports exit 0 after
  mechanical warnings were corrected. The combined suite has not been rerun.
- Direct `unsafe_guardrail::workspace_unsafe_posture` verification remains red:
  zero passed, one failed, exit 101, identifying the two Windows probe example
  unsafe roots already tracked in the fixture-consolidation backlog entry. The
  approved unsafe boundary must be preserved, not bypassed to obtain green.
  Protocol and macOS strict clippy must be rerun after the core corrections;
  their prior runs stopped at the core dependency warnings. Native acceptance
  remains unestablished.
- Next work is to complete the remaining proof-plan mapping and full verification.
  KDE and Windows unified-focus adoption must be recorded before coordinated
  publication; old readers are not compatible.

## Rebased work-in-progress evidence

- Current implementation was rebased onto `b9a197c1`, including single report
  ingress `b282f8de` and dead fullscreen flag removal `9fec57c3`. Ten textual
  conflicts preserved the upstream single-engine test and ingress signature,
  retained-focus assertions, truthful foreground fixtures and the shared-roster
  contract. Four newly added test call sites then needed the two-argument ingress.
- Latest verification is reported by the implementation worker against that
  rebased tree with the four call-site fixes: core library 2139 passed, macOS
  library 1570 passed; protocol library 61, world codec 60, close-focus wire 13,
  focus feedback 12, reply decode 17 and selection writer 2 passed. Each test
  command exited 0, with zero failures. The macOS library includes all four
  per-cause skipped-issuing journeys and both genuine selection journeys.
- A complete workspace build is not established. The worker's workspace
  all-target check exited 101 in `omnitiler::float_intent_store` on rustix
  mode-type operations and unavailable `mkfifoat`; attribution is not yet
  established. Strict all-target clippy for core/protocol/macOS exited 101 at
  three lints in unchanged `world_native.rs` (needless borrow, question mark,
  unnecessary get-then-check). Further macOS dead-code/unused-variable and
  protocol unused-mut warnings were observed; clean strict clippy is unproved.
- Earlier protocol stress failures were caused by the effects-to-request
  cutover and migrated: before rebase, all 60 library tests, 60 world-codec
  tests and both selection-writer tests passed. Eleven raw-attempt integration
  failures remain caused by the cutover: their mock consumer still assumes
  one-shot focus effects and needs genuine retained accounting, not relaxed
  assertions. They have not been rerun after rebase.
- The earlier macOS integration result was 119 passed, 31 failed, exit 101:
  classification 18, runtime phase 1 seven, scoped gates one, focus-gap trace
  one and scoped tile four. Attribution remains unknown and these tests have
  not been rerun after rebase. Earlier core integration fixture migrations
  passed targeted checks, but no current full integration result is claimed.
- The two Windows-example unsafe-root violations are pre-existing and already
  tracked under Windows fixture unsafe-root consolidation; the direct guardrail
  check exited 101 (zero passed, one failed). No allowlist waiver was added.
- KDE and Windows adoption is tracked in the backlog. Old consumers are not
  wire-compatible. User-run macOS native acceptance and remaining proof-plan
  coverage stay open. C3's approved retained reply work uses the same model;
  it must preserve this lane while converting other items. C1/C2/C5 may move
  retained state, sizing and codec plumbing; C4 may replace evidence carriers.

## Linux verification and core/protocol review (2026-11-12)

- Status: mechanical repairs verified, but acceptance remains blocked. No focus
  behavior, wire shape or fixture bytes changed in these repairs. The raw-attempt
  consumer migration and the structural findings below need separately bounded
  work before this change can be accepted.
- `cargo build -p tiler-core -p tiler-protocol -p omnitiler --all-targets`,
  `cargo check -p tiler-core -p tiler-protocol -p omnitiler --all-targets` and
  `cargo clippy -p tiler-core -p tiler-protocol -p omnitiler --all-targets -- -D warnings`
  pass after equivalent lint repairs. Two service-test consumers now read the
  retained request rather than removed focus effects, preserving exact target
  and FIFO assertions. `cargo fmt --all -- --check` passes after formatting the
  five core files with focus-cutover drift (16 formatting hunks).
- `cargo test -q -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture`
  exits 101: 3,172 passed and 11 failed across the 53 targets reached, including
  2,139 core library passes and 61 protocol library passes. Cargo stops at
  `raw_attempt_settlement_v4` (21 passed, 11 failed); subsequent targets and
  doc tests are not established by this run. The initial run stopped earlier
  at a stale service assertion (12 passed, one failed in `n5_retained`); after
  that migration two rapid-input assertions exposed the same obsolete consumer.
- All 11 remaining Rust failures reject the active visibility scenario replies
  at strict decoding: retired `closeFocus`, missing `focusRequest`, and old
  focus effects. Beyond decoding, `VisibilityHostDriver::execute_reply` still
  accounts for focus via `seen_effects` and `HeldFocus`, with effect-oriented
  jq oracles. Neither a byte rewrite nor a decode-time shim is a valid repair.
  No genuine recorder for these two adapter-directed visibility scenarios was
  found. A genuine producer/encoder recorder plus the roughly 400-line consumer
  region and affected oracles is a separate bounded migration; retain the
  visibility, first-feedback, no-retry, supersession and ordering checks.
- Fixture integrity is also unresolved in the inherited implementation:
  `05e72b7d` changes 147 files under the frozen `test-fixtures/world-wire/v4`
  family (manifest, messages and four scenarios). For example, its frozen
  `reply-observe.json` now carries `focusRequest`. This is not a valid active
  fixture migration. Recovering the frozen baseline and producing/redirecting
  current vectors into `v4-active` needs a separate reviewed migration; the
  mechanical repairs leave all fixture bytes as found.
- `cargo build --workspace --all-targets` and
  `cargo check --workspace --all-targets` fail with 19 E0425 diagnostics in
  macOS fixture tests: macOS-gated frame/zoom helpers are called from ungated
  tests, introduced before the focus push in `4e275f6a`. Full-workspace strict
  clippy also stops in macOS code (unused imports/items, test bindings and
  structural lints, as well as those missing helpers). Individual macOS lint
  attribution is not fully established; no clean macOS result is claimed.
- The earlier `rustix` errors are host portability, not a reproduced Linux
  regression: `float_intent_store` compiles and tests on Linux; rustix 1.1.4
  excludes `mkfifoat` on Apple targets and uses platform-specific mode types.
  The FIFO test call and production `st_mode`/`S_IFMT` assumptions predate
  this push, also recorded in
  [the earlier macOS workspace limitation](archive/macos-carbon-removal.md#verification).
  Native macOS and Windows compilation and outcomes remain unverified here.
- KWin `npm test`: 1,172 passed, 93 failed and 10 cancelled, unchanged before
  and after the repairs; `npm run typecheck` passes. The current strict reader
  already supports close-only `closeFocus`, contrary to the historical E18
  snapshot, but lacks `focusRequest` (`kwin/src/world-report.ts`,
  `CORE_REPLY_KEYS`). Reader-gate failures are expected until adoption; other
  producer/apply failures require reclassification after that gate is repaired.
  These are mocked/offline suites, not native evidence.

### Frozen family recovery (2026-11-12)

- Restored all 147 files changed by `05e72b7d` under
  `test-fixtures/world-wire/v4` to their exact `7a6d6e12` bytes. Direct
  verification compared complete path sets and every working-tree blob hash
  against that baseline: world-wire v4 547/547, stage5 32/32 and stage5-v4
  52/52 equal, with no missing or extra files. No active vectors changed.
- Recovery resolves the historical-byte corruption, not current conformance.
  The active manifest still points all 143 WorldReply witnesses (115 valid,
  28 reject-wire) at frozen replies. Current reply readers, including
  `reply_v4_decode.rs`, `close_focus_wire.rs` and `world_codec.rs`, still
  expect the amended shape from historical paths. They need a separately
  bounded genuine producer/codec recording into `v4-active` and reader
  migration, preserving strict negative and round-trip oracles. The existing
  active recording documentation covers handshakes, not amended replies.
- No Rust or KWin suite was rerun after this fixture-only restoration. The
  earlier Linux build/lint results above are inherited evidence; their test
  counts do not establish the restored tree. Historical reply-reader failures
  are expected from source inspection but have not been counted. The 11
  visibility-consumer failures, close/hover regression, state consolidation
  and reserved-effect removal remain unresolved.

### Active reply schema recording (2026-11-12)

- Active WorldReply coverage now records 115 valid-wire and 28 reject-wire
  witnesses locally in `v4-active`; none of its 143 reply manifest entries
  points back at frozen history. Valid witnesses and the three context-only
  rejects pass through the genuine typed decoder/encoder. Context-free
  negatives derive from those encoded valid witnesses, retaining exact
  refusal and counterfactual-repair oracles; the active README explains the
  automatic-awaitShown multi-field negative.
- Schema witnesses need codec provenance, not an engine transition for every
  shape: these tests establish strict decoding, canonical encoding and bounds,
  not behavior. Behavioral visibility scenarios separately require genuine
  producer/encoder recording. This avoids rebuilding engine histories for
  wire-only coverage without weakening behavioral provenance.
- Reply readers in `reply_v4_decode.rs`, `engine_twins_reply_v4.rs`,
  `close_focus_wire.rs`, `world_codec.rs` and `target_lanes.rs` now use active
  replies. Unchanged report fixtures remain historical. Reserved raise/stack
  witnesses remain pending their coordinated removal.
- Direct recheck confirms all three frozen families still have equal path
  sets and bytes at `7a6d6e12` (547/32/52 files). The permitted three-package
  suite reaches 53 targets: 3,172 passed, 11 failed, stopping at the unchanged
  visibility-consumer boundary (21 passed, 11 failed). Reply decode/codec
  targets after that boundary remain unrun. Three-package all-target check
  and workspace format check pass. These are schema-migration results, not
  completed retained-focus acceptance.

### Behavioral visibility migration and close/hover correction (2026-11-12)

- Both active visibility scenarios now contain genuine engine-produced,
  codec-encoded adapter replies. Recording retained their ordered host
  evidence, refusal, stale-authorization, supersession and no-retry oracles;
  only genuine ids/revisions, request-lane assertions and modeled geometry
  changed. The valid-wire fallback-hide edge has a separate typed-codec
  consumer test because core currently produces only `fallback:false`.
- The mock visibility consumer now keeps one immutable current request and
  a retired issuing-revision fence, not an unbounded spent-effect set.
  Identical retained delivery is inert; same-id tuple corruption and retired
  requests fail closed. Selection-backed arrival uses frozen `afterShown`
  members rather than adding later shown rows. A held-callback discriminator
  proves an extra shown row cannot enlarge that set.
- The permitted three-package suite now reaches completion: 3,316 passed,
  zero failed across 66 targets, including all 35 raw-attempt tests. The one
  newly reached failure was a report fixture read incorrectly redirected by
  the reply-reader migration; explicitly retaining its unchanged historical
  report path fixes it. No other newly reached failures remain in that run.
- Added the close-B/survivor-C/hover-D discriminator before fixing core.
  Its first full-suite run fails with cursor D versus expected C while the
  retained request already correctly selects C (core library 2,139 passed,
  one failed). Publication now records the pending winner rather than its
  losing hover carrier. The regression skips the issuing reply, then proves
  an implicit non-focus command affects C and retains the original request
  id. Cause precedence and supersession are unchanged. The matching minimal
  action row is in the reference matrix; reference/native outcomes stay TBD.
- After the correction, the permitted suite passes 3,317 tests with zero
  failures across 66 targets; workspace format check passes. Duplicate
  settlement/cursor mechanisms and reserved effects still need removal.

### Pre-reconciliation structural findings and behavior boundary

These findings describe the reviewed baseline; the reconciliation outcome
below records which mechanisms were subsequently removed.

- One wire authority is present: `pending_focus`; internal Focus carriers are
  producer inputs, stripped before encoding. However, `world_focus.rs` retains
  a second historical grant ledger and exact-tuple feedback latch beside
  `PendingFocus.feedback` in `world_close_focus.rs`. Both run per report.
  Consolidation must preserve first-row settlement and selection's independent
  member/visibility completion, rather than add another bespoke lane before C4.
  Estimate: one moderate core/test migration, including the 1,565-line
  `world_focus_settlement_tests.rs` module, not a lint-sized repair.
- `logical_focus_cursor` and `implicit_focus` shadow the same execution subject
  through paired writes and an older-state fallback. Consolidating them is
  structural work under Simplicity/C2; audit test-only divergent setups and all
  command subject readers before removal. No compatibility fallback is needed
  for the in-place pre-release break.
- Source tracing exposes a close/hover discrepancy requiring an explicit
  discriminator: close selects C; a fresh qualifying hover selects D in the
  same report. `drain_focus_candidates` keeps C under Q2, but forwards D's
  carrier; `assemble_publication_rows` calls `record_published_focus`, which
  records D into the cursor again. The wire can therefore request C while the
  execution cursor names D. This path is source-confirmed, not runtime-proven.
  A minimal follow-up row is: close focused B with survivor C while entering D;
  skip the issuing reply; issue an implicit command. Pin both the retained
  winner and execution subject. Do not silently change cause precedence or
  supersession while repairing the duplicate recording path.
- Reserved raise/stack effects still have no producers. Their removal touches
  shared types, codecs, sizing, adapter matches and tests; retaining them is the
  earlier design choice, not evidence that their latest-only delivery works.
  Revisit as a small coordinated contract change under Simplicity/C3, separate
  from the mechanical repairs. Persistent above remains independent.
- `afterShown` is a frozen projection of existing neutral selection inputs
  (lifetime, logical home, desired frame, view/scopes), not native arrival
  evidence or a macOS Space identity. Keep platform readback/evidence separate.
  Its 10,000-member/16-per-scope caps and whole-reply bounds remain enforced;
  no new caps, retry, timer or acknowledgement lane were added.
- Single `handle_report(report, ordered_commands)` ingress and originating
  explicit `cmdSeq` are retained. Wire sizing remains the existing C5 debt;
  large frozen projections also ride existing C2 transactional clones. Neither
  performance budgeting nor native acceptance is established by unit counts.
  C3 remains per-field retained latest-wins, never acknowledged events; the
  intent conversion must preserve this request and its selection-freeze order.

## Reconciliation outcome (2026-11-12)

- Core now has one focus settlement owner (`pending_focus`) and one execution
  subject (`logical_focus_cursor`). Removed the historical retained-grant
  ledger, its feedback latch/publication/pruning passes, `implicit_focus`
  and its older-state fallback. Publication records only the winning pending
  target. Selection still owns independent member/visibility/terminal
  obligations; it does not authorize another focus attempt.
- Retained-slot tests preserve first exact-tuple row/stamp under holds,
  incomplete inventory and barriers, the conservative verdict table, refusal,
  terminal no-call, lost feedback and supersession. Historical multi-grant
  probes were replaced by single-slot pruning coverage; the original-stamp
  assertion now exercises the actual pending owner. No parallel settlement
  lane, acknowledgement or sequence tracking was added.
- No live raise/stack producer or decision requiring those unused kinds was
  found. Removed their enum/codec cases, sizing witnesses, synthetic positive tests
  and the macOS unhandled-effect dispatcher. Persistent `above` is unchanged.
  The shared ephemeral Focus carrier remains an internal producer input;
  genuine wire replies have required `effects:[]`, with no admissible entries.
  Removing that empty envelope field is not part of this amendment.
- Active reply coverage is now 138 local witnesses: 113 valid and 25 reject
  (22 context-free, three host-context). Retired two positive raise/stack
  vectors and three reject derivatives of that removed vocabulary. Direct
  negative tests still reject both retired kinds. Frozen historical vectors
  remain unchanged; all five retired active vectors were newly recorded,
  never historical recordings.
- Latest permitted Rust suite: 3,317 passed, zero failed across 66 targets,
  including doc tests, with only the unsafe-posture guardrail filtered.
  One obsolete non-focus-kind test was deleted; a new historical-reply
  rejection test pins all 115 frozen valid-wire replies. Review also restored
  exact decode-negative oracles for the retired effect correlation shapes
  and removed the unreachable old effect encoder. Three-package all-target
  build, strict clippy
  (`-D warnings`) and workspace format check pass. macOS dispatch deletion
  is mechanical and remains unverified on macOS; no Windows code changed.
- KWin offline tests: 1,193 passed, 72 failed and 10 cancelled (1,275 tests,
  188 suites); typecheck passes. Six failures directly prove strict-reader
  rejection, four directly assert removed focus effects, two need active byte
  re-recording, and 60 have downstream non-application signatures with no
  `focusRequest` consumer. The latter are class-level attribution, not
  individual root proof: adoption must rerun them, including the rotation
  queue case. Ten stage5 children are blocked by the same reader gate.
  Thus the earlier 66 unattributed failures have categories, not 66 proven
  independent diagnoses. No independent non-focus regression is established.
- Durable interface/ownership decisions are in `docs/decisions/contract.md`;
  the contract describes the empty effects boundary and winning cursor.
  KDE adoption must update its strict reply reader, retained-request apply/
  cancellation/accounting and active producer byte/stage5 recordings.
  macOS must absorb the shared-kind removal and reverify its retained request/
  arrival journeys. Coordinated adapter compatibility, native acceptance,
  remaining broader proof-plan coverage and C4 unification remain open.
- Independent source/fixture review found the single-owner, cursor and
  precedence invariants preserved. Its missing negative-oracle and stale
  comment findings were corrected and rechecked. Final build, strict clippy,
  format and permitted Rust suite all pass after those corrections. The KWin
  rerun has the same counts and failure-name set as the classified run.

## KDE adoption guard audit (2026-11-12)

- Adoption is blocked before runtime changes: the adapter's focused-float/sticky
  admission guard has no source-equivalent core gate. In
  `kwin/src/world-apply.ts:2669-2682`, admission over a different observed
  floating/sticky foreground is spent without a setter. Core's
  `take_admission_focus` (`world_model.rs:1359-1536`) does not test that
  foreground shape, and the pre-publication `validate_pending_focus`
  (`world_close_focus.rs:567-688`) does not add the missing gate. Its focused-game
  check is fullscreen-only (`world_cursor.rs:23-33`). The rule is already
  specified in `docs/decisions/focus.md`; deleting the adapter guard now would
  remove behavior rather than merely remove duplicated policy.
- The adapter's target-sticky and ordinary-float-target-maximized arms
  (`world-apply.ts:2699-2721`) also lack explicit counterparts in those core
  admission/retained methods. Upstream classification reachability and genuine
  producer regressions remain unverified; no native outcome or observed failing
  regression is claimed. Hover already has target floating/sticky/fullscreen
  gates (`world_cursor.rs:64-68,96-102`), so ordinary admission-float eligibility
  must not be mistaken for a missing blanket floating veto.
- The sibling fullscreen/maximized admission fence already exists in core
  (`world_model.rs:1506-1514`, `world_close_focus.rs:665-676`). Target fullscreen
  is checked by retained eligibility (`world_close_focus.rs:625-634`). Adapter
  duplication can be removed while preserving declared last-moment live safety,
  once the missing-policy boundary is resolved. No adapter prefix authority,
  precedence, supersession or hover-deduplication mechanism should replace it.
- This audit changed documentation only. Decoder/consumer adoption, four-cause
  skipped-publication proofs, active KDE/stage5 recording and failure-by-failure
  verification have not started. Earlier suite counts above remain earlier
  evidence, not a verification run of this audit. Native testing remains pending.

### Core guard conformance repair (2026-11-12)

- The audit blocker above is resolved in core under the existing
  `docs/decisions/focus.md` rules, not a new product choice. Genuine admission
  producers first demonstrated all three gaps: different floating/sticky
  foreground, sticky target and maximized ordinary-float target. Four new
  discriminators failed before repair (core library 2,141 passed, four failed).
  All three shapes are reachable; no guard was discarded as unreachable.
- Admission now withholds and spends those births in its existing producer;
  the single retained validator applies the corresponding automatic eligibility
  checks. Hover's producer also withholds over a different floating/sticky
  foreground while advancing its existing hover memory, preventing a static
  pointer from stealing focus after the veto clears. Explicit focus/send-follow
  keep their exemption. Positive-close float/sticky fallback remains unreachable
  through the existing tiled fallback predicate, pinned by producer tests; no
  redundant close producer gate was added.
- Thirteen new tests cover origination, retained revalidation, clearing vetoes
  without rearming, hover, close fallback, explicit exemptions and the maximized
  tiled control. The old accepted-admission workspace-memory test now observes
  tiled foreground before admission, preserving its original sticky-memory
  overwrite/return assertions without assuming forbidden activation over an
  active sticky. A negative control removing the two hover gates fails exactly
  the two new hover regressions; the gates were restored afterward.
- Latest worker-run permitted suite completes: 3,330 passed, zero failed across
  66 targets, with only the unsafe-posture guardrail filtered. The runner
  summaries were inspected and aggregated independently. Workspace format check
  is reported passing. Final build/clippy and independent implementation review
  remain required after KDE integration. Minimal discriminator rows are in the
  reference matrix; reference/native outcomes remain TBD.
- KDE runtime migration is in progress, not accepted. Its first full post-reader/
  consumer migration run reports 1,229 passed and 45 failed out of 1,274 tests;
  typecheck passes. Remaining failures need individual attribution, active
  recording and consumer integration checks. Four-cause latest-only proof and
  final verification remain pending.

### KDE integration state (2026-11-12)

- The strict KDE reader now requires nullable `focusRequest` with all four
  causes, explicit command correlation and closed frozen-dependency validation;
  `effects` must be empty and `closeFocus` rejects. The bounded existing retained
  consumer/accounting is generalized, not layered: removed the effects focus
  loop, spent-effect set, close-only path and `af-` policy blocks. Mechanical
  native homing translates logical scopes through accepted workspace bindings,
  never native/logical string equality. Frozen members alone supply arrival
  reads; missing/unreadable readiness defers before spending. Explicit focus
  keeps its automatic-focus exemptions.
- Four genuine broker-produced journeys skip the issuing and three unchanged
  publications, deliver only the latest to a fresh production consumer, and
  assert one native setter plus inert redelivery. The same consumer's actual
  feedback and mock-backend target observation pass through production capture/
  encoding back to core and settle the request. Explicit selection carries its
  frozen dependencies. Hover uses a native hit-test fact captured and encoded
  as `cursorHit`, not a patched request or id-prefix inference.
- Genuine recorders regenerated all 92 stage5 full/s2 artifacts (54/38), with
  only the permitted exact generation-token normalization, and the KDE active
  Observe reply through the byte-test generator. Subsequent no-update checks
  pass: KDE bytes six tests, stage5 34 tests. Complete frozen-family comparisons
  against `7a6d6e12` remain equal: world-wire v4 547 files, stage5 32,
  stage5-v4 52 and KDE v4 20. The post-recording permitted Rust suite completes
  with 3,330 passed, zero failed across 66 targets; its summaries were inspected
  independently. These are offline results, not native evidence.
- Final directly run KWin full suite: 1,283 tests, 1,283 passed, zero failed
  or cancelled; typecheck passes. Migrated synthetic fixtures now carry
  required bindings and retained requests; wire-only current readers use active
  replies rather than historical bytes. Genuine FIFO, fixed-tile admission,
  populated selection and split-edge oracles read the retained lane. Fixed-tile
  focused-float withholding now asserts core publishes no request and native
  focus stays unchanged, rather than expecting a deleted adapter guard log.
- Selection integration repairs pass four targeted genuine journeys. The old
  missing-witness oracle expected actuation while shown evidence was unreadable,
  contradicting frozen readiness. Its replacement proves unspent/no-row
  deferral, identical retained re-publication, later witness/single actuation
  and complete/proven settlement. A separate genuine new-id/null sequence
  retires unstarted requests without a setter or row. Already-target feedback
  now carries its sampled arrival brackets: synchronous own echoes retain zero
  setters and settle complete/proven rather than dropping independent arrival
  evidence. No core policy changed in these repairs.
- Sticky-restart failure before the empty-cursor repair below: the initial commandless capture
  cannot record foreground `win-a` because it has neither session link nor
  exception. A subsequent commandless recovery capture does adopt A, but the
  foreground identity is unchanged, so observation recording does not run again.
  That capture identifies A as a newcomer; the repaired sticky-target guard
  withholds its automatic focus. The execution cursor stays empty and the later
  correctly queued/drained implicit sticky-off refuses `focus-unknown`.
- Historical source establishes the prior subject route, not an observed-focus
  fallback: at `7a6d6e12`, the recovery capture's automatic-admission effect fed
  `world_publish_v4.rs:119` / `world_publish.rs:336` /
  `world_sends.rs:67-89`, recording `implicit_focus` before the next command.
  At `632ee7d9`, `world_run_v4.rs:463-470` records that admission into both
  cursors before commands; `world_close_focus.rs:93-96` resolves cursor first.
  Neither route required admission retirement. General admission recording was
  deliberate, but the restored sticky subject depended on admission that the
  existing automatic sticky guard now forbids. No no-decision observed-foreground
  fallback was found in the older source.
- The per-capture distinction is reproduced by temporary current-core probes;
  the historical route is a source deduction from read-only pinned files, not
  an old-tree execution. Probes were removed and the current example rebuilt.
  Initial no-home is transient: the recovery capture's session inventory contains
  A. Its link-versus-exception form was not captured; no separate persistent
  session-adoption defect or record requiring a tiled link is established.
- Existing required outcome remains implicit sticky-off acting on restored
  foreground A; keeping refusal or weakening the journey is not selected. The
  user chose the read-only empty-cursor rule below, without reviving forbidden
  automatic activation. Automatic activation guards do not prohibit factual
  foreground recording; command-owned recording's no-retry rule is not a blanket
  prohibition on observation ingestion.

### Empty-cursor command subject (2026-11-12)

- User decision 2026-11-12 is recorded in `docs/decisions/focus.md` and the world
  contract. The existing `execution_subject` owner uses a known live, bound,
  current-generation observed foreground only when the logical cursor is empty
  and no pending retained request owns the subject. It is a read-only resolution:
  no extra cursor/ledger, canonical-memory or session-eligibility write, or focus
  request. The nonempty-cursor path is unchanged, including the separate macOS
  suppressed-admission question.
- Discriminators were added before the repair. The first permitted suite failed
  exactly the restored-sticky and guard-withheld positive cases (2,154 core passes,
  two failures). Six new tests now cover restored sticky-off targeting A;
  sticky, focused-float withholding and maximized ordinary-float subjects with
  actual command-target proofs; later host focus on a withheld newcomer without
  rearming; unknown/unbound/closed foreground; wrong-generation ingress refusing
  before execution with no subject/state adoption; live pending authority with an
  empty cursor; and logical B winning over observed A. Wrong-generation transport
  refusal is distinguished from a current-bound command's `focus-unknown`.
- The genuine KDE restart journey passes unchanged, including sticky-off drain
  authority, native setter, command settlement and fresh tiled admission. The
  temporary initial no-home is not a separate persistent adoption defect.

### Original downstream failure attribution

- All 60 original downstream failures share the strict reader gate: the old
  `CORE_REPLY_KEYS` required `closeFocus` and rejected the genuine core's
  `focusRequest`, so replies never reached application. This covers migration
  engine (3), migration producer (1), spare occupancy (2), clamp lifetime (1),
  dynamic lifecycle (7), FIFO engine (8), generation rotation (1), fixed tile
  (4), native homing (4), ordinary frame (1), disconnect (4), producer journeys
  (16), unequal-output R4 (1) and split-edge (7): 60 total.
- Confirmation combines pinned reader/control-flow inspection, running the actual
  baseline and current `parseWorldReply` against the same genuine active-producer
  reply (baseline refuses, current accepts), and matching every original test
  name to a passing current result. Original per-test zero-application signatures
  survive only in the earlier classification, not original runner traces; that
  portion is indirect evidence, not a per-test old-tree replay. The initial
  synthetic key-set probe alone was insufficient and was superseded by the
  actual parser probe.
- Follow-on oracle migration is separate from that original gate: four FIFO
  tests, one fixed-tile test and one split-edge test read the retained lane;
  rotation also uses updated shared reply/hit-test infrastructure. The other
  53 test bodies are unchanged. Sticky restart subsequently exposed the missing
  empty-cursor subject under core guard withholding; the decision above repairs
  that later failure. Missing shown readiness and already-target arrival were
  likewise later integration failures, not independent causes of the original
  60. No unrelated non-focus regression remains established.

### Final offline verification and review

- Direct final commands all exit 0 after the last source edit:
  - `cargo build -p tiler-core -p tiler-protocol -p omnitiler --all-targets`
  - `cargo clippy -p tiler-core -p tiler-protocol -p omnitiler --all-targets -- -D warnings`
  - `cargo fmt --all -- --check`
  - `cargo test -q -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture`:
    3,336 passed, zero failed across 66 targets; one unsafe-posture test filtered.
  - KWin `npm run typecheck` and `npm test`: 1,283 passed, zero failed,
    cancelled or skipped across 189 suites. Runner summaries were read directly.
- Complete filesystem path sets and every blob equal `7a6d6e12`: frozen
  world-wire v4 547 files, stage5 32, stage5-v4 52 and KDE v4 20. The cursor rule
  did not alter active recordings: normal byte/stage5 checks remain green, so
  no additional recording or hand edit was needed.
- Independent complete core-guard/cursor/KDE source, test and fixture review has
  no remaining code findings. Its initial frozen-byte concern compared two
  commits instead of pin versus worktree; worktree verification withdrew it.
  Protected-document hunks were inherited, not changes attributed to this work.
- The first final format check found only new cursor-test formatting drift;
  mechanical formatting repair and all final gates then passed. Offline KDE
  adoption is verified. The broader change stays active for coordinated
  macOS/Windows adoption, native KDE acceptance and other open proof-plan scope;
  no macOS/Windows runtime acceptance or live KDE result is claimed.

### Inventory-hold and pending-subject regressions (2026-11-13)

- The reported hold/clear sequence passes on current core (`a876d659` baseline):
  complete A-focused inventory -> incomplete inventory with a held source domain
  and known B foreground -> complete inventory with unchanged B. The cursor
  remains empty and no request is pending; the existing read-only
  `execution_subject` fallback resolves B. Separate genuine producer regressions
  prove focus-right selects C from B and move-right changes B's allocation while
  retaining B as mover. No production change or additional subject mechanism was
  needed. This is offline core evidence, not a rerun of the historical macOS
  capture or native acceptance.

| Genuine sequence | Current outcome and duration | What ends the pending state / refusal |
| --- | --- | --- |
| Explicit send of focused A to another workspace with follow; no focus feedback; next observation still A | Explicit request exists on the issuing publication only. Next target-equality observation satisfies it without feedback; a later observed S focus supplies the implicit subject | Target equality on the next observation. Missing feedback alone does not wedge this sequence; native send/arrival success is not established by this test |
| Pre-close MRU A; close focused B; known live same-domain fallback C; unchanged C with no feedback, then accepted setter with unchanged C readback | Positive-close request keeps the identical tuple, including after accepted-but-unproven feedback. Stable C can retain it without a time bound, but the homed pending A restores the cursor: implicit focus and move act from A, not C, and do not refuse `focus-unknown` | Equality A, a newer observed non-target focus D, or target close retire in new branches. A newer implicit focus decision supersedes. Existing refusal and adopted-view-change tests cover those retirement rules |
| Known live C while explicit target N remains live in a held domain | Repeated reports retain the request and restore cursor N through `preserve_live_pending_cursor`. Held move refuses `operation-refused`, not `focus-unknown`; after clear, implicit focus acts from N | Clearing the hold restores mutation eligibility; a newer focus decision supersedes. Retention itself has no timeout |
| Known live C; explicit N issued in a held domain; subsequent reports carry N only in `held_windows`, not live facts | Three successive held reports retain the identical request with empty cursor and no execution subject. Implicit focus-directional and move-directional both refuse `focus-unknown`, despite live bound C. This can continue for as long as N stays held-only and C stays at the issuing baseline; no report-count or time bound retires it | Returning live N restores a subject; in this newcomer sequence automatic admission supersedes the explicit row with a fresh N request. Existing equality, newer observed focus, target loss, refusal, view drift and newer-focus-decision rules can also end pending authority. Merely repeating C or refusing a non-focus command does not |

- The last sequence is an unbounded, state-conditional refusal, not proof of a
  persistent post-clear failure. `world_run_v4.rs` clears a cursor whose target
  lacks staged homing; `preserve_live_pending_cursor` cannot restore that target.
  `execution_subject` deliberately refuses the observed fallback while any
  request remains pending. `validate_pending_focus` accepts retained homing and
  exempts explicit requests from automatic hold/readability cancellation. These
  are the existing single-owner and pending-intent rules; no change was selected.
- Decision discriminator and unsupported reference/native outcomes are in
  [the reference matrix](../spec/reference-outcomes.md#inventory-hold-and-pending-execution-subject).
  Alternatives, with no selected outcome:
  - Keep pending ownership/refusal while the target is held-only: preserves the
    requested target and no-retry rule, but stable known foreground does not
    unblock implicit commands until another settlement/recovery fact arrives.
  - Resolve an empty cursor from the pending target: preserves logical intent,
    but an absent/held target still cannot pass existing per-command safety
    fences; this alone need not make the command actionable.
  - Use observed foreground while a request is inert: permits a different
    subject without another focus call, but overrides pending target ownership;
    lost feedback cannot establish whether an adapter has already spent the id.
    Existing home/readability/hold/exclusion fences would still apply.
  - Retire inert intent on a later complete observation with a stable different
    foreground: releases pending authority, but treats an unchanged issuing
    baseline as cancellation and abandons delayed explicit intent. It requires
    a new settlement rule and must distinguish held/incomplete observations.
- Seven new in-crate tests use genuine report/command producers, never assigned
  pending requests or cursors. Direct final offline gates pass: three-package
  all-target build, strict clippy, workspace format check and the permitted Rust
  suite (3,354 passed, zero failed, one unsafe-posture test filtered). KWin
  `npm test` passes 1,296 tests with zero failures/cancellations/skips;
  `npm run typecheck` passes. Complete path sets and blobs equal `7a6d6e12`
  for frozen world-wire v4 (547), stage5 (32), and stage5-v4 (52). No fixture
  recordings changed. No macOS/Windows code or live desktop tests changed or ran.

### Native KDE smoke blocked at bring-up (2026-11-13)

- Baseline `83ab5b91`. Exact sequence: verify dev DOWN; start the Wayland
  fixture server without spawning windows; append a workspace beyond the whole
  native roster and select it through VirtualDesktopManager D-Bus; run
  `just dev trace --scope-class OmniTilerTestWindow`. No physical shortcut or
  planner command was submitted. The fixture's `list` returned `ids:[]`.
- Startup defect: the script loaded and discovery connected, but every capture
  was rejected. Fresh current-KWin-PID journal evidence:
  - `07:41:44.637424 omnitiler:world:ready ... nwinv=0 rev=0`
  - `07:41:44.648269 omnitiler:world:discovered gen=w-1794516104186983667-818714-0`
  - `07:41:44.652817 omnitiler:world:dispatch src=src-1-2v8c5b nwinv=3 cmd=observe`
  - `07:41:44.654491 omnitiler:world:rejected kind=snapshot-invalid msg=cursor-hit-unknown recovery=on-next-event`
  - `07:41:44.768502 omnitiler:world:dispatch src=src-2-keymti nwinv=3 cmd=observe`
  - `07:41:44.770472 omnitiler:world:rejected kind=snapshot-invalid msg=cursor-hit-unknown recovery=on-next-event`
  - Two later event-driven captures, `src-3-1wdesla` / `nwinv=9` and
    `src-4-5a18wl` / `nwinv=10`, had the same rejection. Their trace rows
    report `wins=0/0`. No `applied` row exists in the fresh journal.
- A read-only `ReadWorldPresentation` request
  `{"v":4,"req":"smoke-read-1"}` returned
  `outcome:rejected, kind:not-ready, message:"no accepted capture"`.
  The separate fresh `QT_CATEGORY=kwin_scripting` journal check was empty;
  this does not establish successful strict reply decoding or native application.
- Source inspection identifies a likely scoped-capture mismatch, not a captured
  native target identity: `world-observe.ts` reads the native `windowAt` head's
  UUID independently of its scoped window facts; `world-report.ts:3121-3138`
  mirrors it into `cursorHit`. The strict protocol gate
  (`world_protocol.rs:4923-4937`) rejects a hit absent from reported windows.
  The exact hit UUID/class and raw rejected body were not captured, so that
  causal join remains unverified. No code was changed or restart attempted.

| Requested smoke check | Verdict and evidence boundary |
| --- | --- |
| 1. Bring-up, fixture tile and automatic admission | NOT RUN as a complete sequence. Startup capture acceptance failed as above; no fixture window was spawned, tiled or focused. |
| 2. Directional focus and once-per-id settlement | NOT RUN: no accepted capture; no commands or setter/feedback evidence. |
| 3. Positive close with survivor and reflow | NOT RUN: no accepted capture or fixture tiles. |
| 4. Focus-follows-cursor when enabled | NOT RUN: readiness failed; setting enablement was not established. |
| 5. Focused-float admission guard and implicit subject | NOT RUN: readiness failed; no float/newcomer sequence. |
| 7. Five-command receipt-to-dispatch latency | NOT RUN: no commands submitted; observation timestamps are not command latency. |
| 8. Workspace send-follow then implicit command | NOT RUN: readiness failed; no send or subsequent implicit command. |

- Raw evidence retained outside the repository: `dev-console.log`, copied
  `omnitiler-dev.*.log`, `planner.log`, `readiness.journal`,
  `pre-teardown.journal`, `final.journal`, `presentation-not-ready.bus.json`,
  fixture list/shutdown replies and final lifecycle checks. Logs were copied
  before `just dev-off`. Final status proves planner unowned/controller unloaded;
  no `ydotoold` process was found. Fixture shutdown returned `open_windows:0`
  and its process/socket were absent. The appended workspace remains.
- Native focus, guard, persistence, latency and send acceptance remain open.
  Next: investigate the rejected scoped cursor-hit capture, then rerun this
  smoke sequence from a fresh accepted readiness checkpoint.

### Offline scoped cursor-hit investigation (2026-11-13)

- Source confirms an ingress mismatch: scoped capture drops foreign windows
  entirely (`kwin/src/world-observe.ts:912-917`), but native `windowAt` head
  capture reads its identity without scope or reported-window membership checks
  (`world-observe.ts:1652-1680`). Before repair, the encoder forwarded that
  identity verbatim as `cursorHit`. Protocol decoding rejects
  any hit absent from valid reported windows as `snapshot-invalid` /
  `cursor-hit-unknown` (`crates/tiler-protocol/src/world_protocol.rs:4923-4939`);
  typed core validation independently enforces the same membership requirement
  (`crates/tiler-core/src/world_report_v4_checks.rs:123-136`). Foreground capture
  already treats scope-foreign focus as unknown (`world-observe.ts:871-880`).
- Surviving smoke evidence was reread: `fixture-list.json` contains `ids:[]`,
  and `readiness.journal:134,147-151` confirms fixture scope and two zero-window
  reports followed by this exact rejection. This supports the original hypothesis
  and confirms the source-level failure mechanism, but the rejected bytes and
  hit identity/class remain unavailable; the precise native target is not proven.
- Initial investigation stopped at unspecified unreported-hit handling in the
  v4 report table. Clarification approved 2026-11-13: option A is a defect repair,
  consistent with
  foreign/unbound foreground reporting. Emit a normative hit only when it joins
  the same report's valid window set; otherwise omit it. Preserve the raw native
  `cursorWindow` lane for broker processing and leave strict shared validators
  unchanged. The report table now states empty for unreported/foreign/unbound
  hits (`docs/porting/world-contract.md:355`); no wire shape changed.
- Repair: `kwin/src/world-report.ts:3123-3139` retains raw capture spelling and
  gates only the normative lane on cursor presence and the existing final
  emitted-window identity set (`:3017-3028`). Scope filtering, native hit-test,
  gesture evidence and strict validators are unchanged; no geometric fallback
  or underlying-window substitution was added.
- Core consequence: absent hit produces no hover candidate
  (`crates/tiler-core/src/world_cursor.rs:90-92`) and, when hover processing is
  active, clears last-hover memory without minting focus (`:178-198`). A later
  return can qualify as a fresh entry. A reported nonempty hit additionally
  requires current-view, readable tiled membership and automatic eligibility
  (`:93-128`). Unreported nonempty hits currently refuse before these paths;
  no previously accepted unscoped hit becomes empty under the same-report join.
- Five observer-to-serializer regressions
  (`kwin/tests/world-report-v4.test.ts:491-588`) cover scope-foreign, reported in-scope, explicitly in-scope
  unreadable, unreported panel and cursorless hits, preserving the raw lane.
  Before repair the full KWin suite had 1,297 passes and four new omission
  failures; the reported-hit preservation control already passed. Final
  `npm test` passes 1,301 tests with zero failures, cancellations or skips;
  `npm run typecheck` passes. These new tests prove producer projection, not a
  new strict-reader journey; existing Rust unknown/held-hit rejection tests
  remain enabled (`crates/tiler-protocol/tests/report_v4_decode.rs:1567-1604`).
- Sequential offline verification passes: `cargo fmt --all -- --check`,
  three-package all-target build/check/strict clippy, and
  `cargo test -q -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture`
  (3,354 passed, zero failed across 68 target summaries; one test filtered).
  Independent review accepted the join and unchanged accepted-hit semantics;
  its evidence-overclaim finding was resolved by removing a mock reader helper,
  followed by the final KWin typecheck and full-suite rerun above.
- Read-only platform assessment: macOS emits no hit
  (`crates/tiler-macos/src/world_builder.rs:658-659`); Windows already resolves
  raw hits through exactly-bound member identities and requires cursor presence
  (`crates/tiler-windows/src/tiling_sys/world_capture.rs:861-914,1730-1745`).
  This excludes the same raw-foreign forwarding pattern, but the Windows route
  is legacy (`crates/tiler-windows/src/tiling_sys/world_sequencer.rs:355-403`), not proof of amended v4 readiness or
  final-report membership conformance. Neither platform was changed or tested.
- All fixture families remain untouched. Native acceptance remains open. A later
  smoke rerun must first establish an accepted scoped zero-window capture with
  the cursor over a foreign window, then accepted in-scope capture/application,
  before resuming admission, explicit focus, positive close, hover, float guard,
  implicit subjects, intent persistence, command latency and send-follow checks.

### Native KDE smoke rerun after scoped-hit repair (2026-11-13)

- Baseline `c388285d`; Wayland fixture-only scope with
  `just dev trace --scope-class OmniTilerTestWindow`. Startup is no longer
  blocked. This is partial native evidence, not complete retained-focus
  acceptance. Command legs used KGlobalAccel `invokeShortcut`, not physical
  keys; close stimuli used the fixture protocol. Evidence filenames below
  refer to the externally retained smoke bundle, not repository files.
- Foreign-cursor witness: `cursor-journal.txt:1-6` reports
  `hit-class=plasmashell`, `hit-foreign=true`; the timestamped current-owner
  journal repeats that witness immediately before controller readiness
  (`lead-current-pid.journal:10-15,149-152`). No pointer movement was injected.
  The complete current-owner journal has no `cursor-hit-unknown` or
  `snapshot-invalid`; the separate unfiltered scripting-category query returns
  no entries. Neither absence alone is feature acceptance.

| Check | Result | Direct evidence and boundary |
| --- | --- | --- |
| 1. Startup: empty scope, then fixture application | PASS | `readiness-journal.txt:1-6` has ready/discovered and matched dispatch/applied pairs. `dev-console.log:118-126` explicitly accepts an empty `wins=0/0` capture. Fixture A then has accepted capture, geometry application and matching observation (`:155-184`); `enum1-journal.txt:2-4` joins its sentinel/PID one-to-one to the native window/frame. |
| 2. Admission through retained focusRequest | FAIL, evidence gate | Newcomer B is focused, but already foreground at capture (`dev-console.log:293-304`, `fx=0`). No retained admission request/feedback tuple was captured. Native map activation and retained-request satisfaction cannot be distinguished. This is not a demonstrated product failure. |
| 3. Explicit focus | PASS, command route | `dev-console.log:380-393`: command receipt, dispatch, `reason=explicit-command`, `outcome=setter-accepted proven=true`, and native pre/post foreground B -> A. `check3-focused.txt` agrees. Physical shortcut delivery was NOT RUN. |
| 4. Positive-close focus successor | FAIL, evidence gate | Fixture A closed; B is focused and reflows (`dev-console.log:440-474`, `check4-focused.txt`). B was already foreground at capture, with `fx=0`; no positive-close request/feedback tuple proves retained-request causation. |
| 5a. Hover focus and guards | NOT RUN | Focus-follows-cursor remained off and the cursor static. Zero hover rows are not a passing hover test. |
| 5b. Focused-float admission guard | NOT RUN as a valid discriminator | B floated, then C became native foreground before its admission capture. The required captured floating-foreground precondition was not maintained; no core guard failure is established. |
| 5c. Implicit subject, including a float | PASS, command route | No-target toggle resolves C (`dev-console.log:656-677`, `check5-implicit-state.txt:1-3`); the later no-target toggle resolves floating B (`:869-890`). Directional float control also records `focus-float-retained@no-target` (`:817`), not a focus setter. |
| 6. Intent persistence over restart | FAIL, full native evidence incomplete | Saved floating intent survives in storage and the new core model, with no rewrite; see the [intent rerun](retained-intent-desire.md#native-kde-rerun-after-scoped-hit-repair-2026-11-13). Post-restart native joins are retained, but native above-state settlement and the full trace are not. |
| 7. Command latency samples | PASS, measurement only | Five invocation-to-applied samples below; three focus samples include fresh native post-read proof. No physical-key/display latency or performance budget acceptance. |
| 8. Send-follow then implicit command | PASS, command route | `dev-console.log:1138-1162` records follow transfer and native membership setter/readback. `check8-send-state.txt:1-3` places B in the destination; `:1262-1285` and `check8-implicit-state.txt:1-3` show the following no-target toggle affects B there. The later post-restart native join corroborates the same native home, not immediate send latency. |

- Latency method: wall-clock nanoseconds sampled before launching each
  `invokeShortcut` call, compared with controller `ev=cmd` and correlated
  `observe-applied` millisecond timestamps. Includes process/IPC overhead;
  millisecond quantization and wall-clock limitations apply. Raw sender files
  and `lead-latency.json` preserve the samples.

| Sample / trace lines in dev-console.log | Invocation -> applied | Receipt -> applied | Effect boundary |
| --- | --- | --- | --- |
| L0 focus, 380-393 | 20.6 ms | 18 ms | Setter accepted, fresh native target proven |
| L1 focus, 735-748 | 18.8 ms | 16 ms | Setter accepted, fresh native target proven |
| L2 directional retain, 797-817 | 14.1 ms | 12 ms | Intentional no-op, not native-effect latency |
| L3 focus, 1003-1017 | 20.4 ms | 18 ms | Setter accepted, fresh native target proven |
| T2 float -> tile, 869-890 | 14.2 ms | 12 ms | Applied boundary only, not geometry settlement |

- The initial sub-5 ms native-focus claims are withdrawn: fixture-poll times
  were sampled before the read, not after observing the transition. Only the
  correlated fresh native post-read inside the focus application bounds those
  three effects by the applied checkpoint; no rendered-frame timing was taken.
- Evidence limitations: admission/close equality/no-call branches do not emit
  the retained tuple in normal logs. Source owner for that observability gap is
  `kwin/src/world-apply.ts:2691-2710,2772-2777`; setter-only logging is at
  `:2800-2813`. No load-bearing product defect was established or repaired.
- Restart ran after send-follow and its implicit toggle, not in the requested
  check order. Restart used the detached lifecycle route, so its full trace was
  not captured. Initial trace streams were copied before shutdown; the final
  tmpfs copy occurred after shutdown and contains only the earlier trace. This
  evidence-preservation gap remains, despite later recovery of the full
  current-owner journal. The second enumeration helper also lacks a saved run
  receipt/exit and complete named-plugin receipts, though load/unload and final
  child-roster evidence survive. No helper was carried across the restart.
- Teardown: final direct status is DOWN (planner unowned/controller unloaded),
  no `ydotoold`, fixture process absent, and fixture shutdown reports zero open
  windows (`lead-final-dev-status.txt:1-8`, `fixture-shutdown.json:1`). No
  physical-input, complete four-cause focus or full restart-native acceptance
  is claimed.
- Remaining manual checks: enable focus-follows-cursor and enter/leave/reenter
  joined fixture tiles; repeat over/with a focused float. Preserve a floating
  foreground through newcomer capture or mark the guard precondition unmet.
  For admission and positive close, retain the request/feedback tuple and
  distinguish native target equality from a setter. Repeat the isolated
  persistence leg with native above-state readback and trace retention before
  both shutdowns. Keep these outcomes open rather than infer them from the
  command-route successes.

### Unavailable pending-target command subject (2026-11-13)

- User decision (option 2): after a report newer than an unavailable pending
  focus target's request still shows another live, bound window focused, implicit
  commands use that observed window. Keep the pending request, which may complete
  later; same-batch commands still refuse without a newer report. The 2026-11-12
  empty-subject rule otherwise retains its existing boundaries.
- Implemented in `WorldEngine::execution_subject`: the cursor must be empty,
  the pending target absent/unhomed in the staged capture, and the accepted run's
  revision strictly newer than the request's immutable issuing revision. Staged
  revision and a transient same-batch focus-decision fence survive classification
  restaging; they carry no second subject or settlement authority. Equal observation
  counters on successive accepted reports do not prevent the exception. No adapter,
  wire shape, fixture, retry or request-settlement change was needed.
- Three original failing-before regressions establish the newer-report exception
  and same-batch fence (core: 2,170 passed, three failed). The final suite adds eight
  tests and extends the existing held-live/held-only regressions: known C receives
  an actual published maximize target, the exact original N tuple survives recovery,
  N target equality completes it, and C's command target remains unchanged through
  that completion. Unknown/unbound foreground refuses. A newer accepted report with
  unchanged observation counter still resolves C. Existing inventory hold/clear,
  available-target ownership and no-target regressions remain enabled; only the
  superseded held-only `focus-unknown` expectation becomes observed C subject plus
  the unchanged per-command `scope-held` refusal.
- Independent review caught a same-batch send/move regression in broad stale-cursor
  clearing. Three failing-before send/move probes reproduced it; clearing now occurs
  only when canonical command-focus recording actually fails. A matching existing
  cursor is preserved under a hold, covered by a repeated same-target focus probe.
  That probe initially expected the old id despite a new explicit decision; its
  oracle was corrected to require the newer explicit tuple. Neither correction
  changes existing send/move subject policy. Final review has no unresolved findings.
- Final sequential offline gates all exit 0:
  - `cargo fmt --all -- --check`
  - `cargo build -p tiler-core -p tiler-protocol -p omnitiler --all-targets`
  - `cargo clippy -p tiler-core -p tiler-protocol -p omnitiler --all-targets -- -D warnings`
  - `cargo test -q -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture`:
    3,362 passed, zero failed/ignored across 68 target summaries; one test filtered.
  - KWin `npm run typecheck`; `npm test`: 1,301 passed across 191 suites,
    zero failed/cancelled/skipped.
- The [decision](../decisions/focus.md#unavailable-pending-target-execution-subject),
  [discriminator](../spec/reference-outcomes.md#inventory-hold-and-pending-execution-subject)
  and world contract are updated. This completes the offline core exception, not
  the broader retained-focus adoption. macOS/Windows builds and native behavior
  remain unverified; no platform-specific code changed. Native KDE/macOS acceptance
  and the separate suppressed-admission subject question remain open.

### KDE retained-focus diagnostic acceptance (2026-11-14)

- Goal: close the silent native-equality/no-call evidence gap without changing
  focus behavior. Trace each fresh retained-request evaluation with its immutable
  tuple, publication correlation and actual call/equality/no-call branch; preserve
  once-per-id accounting and trace-off efficiency. Inspect core admission/close
  decisions for the corresponding silent boundary.
- Acceptance: branch diagnostics covered offline; sequential format, three-package
  all-target build/strict clippy/permitted test suite and KWin typecheck/full tests;
  independent diff review. Then fixture-only native admission and positive-close
  checks with joined foreground and retained-request evidence, plus a native
  call-issued control where feasible. Audit saved raw evidence and complete
  helper/process teardown before claiming native acceptance.
- Implemented trace-only `ev=focus-eval`, carrying `gen/req/rev/obs`,
  retained `id/target/issuingRev/cause/cmdSeq`, and `decision/reason`.
  Decisions are `deferred`, `equality`, `no-call`, or `called`; refused and
  thrown setter attempts are `called`, never `no-call`. Core also logs retained
  equality/retirement and pre-mint positive-close equality. The latter truthfully
  uses `id=- issuingRev=- request=not-created`, not an invented request.
- No behavior, wire fields, fixture bytes or dependencies changed. Ordinary KDE
  builds strip the diagnostics; unchanged deferred outcomes deduplicate against
  the bounded current envelope record and generation, including fresh same-gen
  handshakes. Spent/null envelopes stay silent. Core uses cached trace enablement
  before formatting/allocation and best-effort stderr.
- An earlier review caught refused setter feedback mislabeled `no-call`.
  Setter-based labels, core generation correlation and missing retirement-branch
  tests are now present. Fresh independent final-diff review finds no defects.
  Tests cover every new decision branch, trace-off behavior, deduplication and
  fresh handshakes. Earlier intermediate counts and unsuccessful formatting/
  compilation attempts are not final verification evidence.
- Fresh sequential gates all exit 0, with full command/output/exit logs retained:
  `cargo fmt --all -- --check`;
  `cargo build -p tiler-core -p tiler-protocol -p omnitiler --all-targets`;
  `cargo clippy -p tiler-core -p tiler-protocol -p omnitiler --all-targets -- -D warnings`;
  `cargo test -q -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture`;
  then KWin `npm run typecheck` and `npm test`. Rust: 3,400 passed across
  70 summaries, zero failed/ignored, one filtered. KWin: 1,302 passed across
  190 suites, zero failed/cancelled/skipped.
- Native evidence bundle: externally retained focus-diagnostic acceptance.
  Raw `*-commands.jsonl`, `*-native.json`, `*-fixture.jsonl` and
  `*-presentation.json` retain calls/replies/status and exact sentinel/PID/UUID
  joins. Journal sources below are in `final-journal.jsonl` and
  `teardown-journal.jsonl`; core traces are in `saved-planner-log.log`.
  Wayland fixtures only, exact class scope, on a workspace appended beyond
  the whole pre-start roster; no physical-input or display-topology claim.
- Evaluated readiness passes: post-cursor current-owner `ready`, `discovered`,
  `src-1-1o25ph3` and matching `observe-applied` revision 1. Separate unfiltered
  scripting-category checks are empty.

| Native leg | Verdict | Concrete evidence and boundary |
| --- | --- | --- |
| Admission request and native equality | PASS, diagnostic correlation | `settled-b` revision 7 joins B's exact sentinel/PID/UUID and matching tiled frames, with native/fixture/core focus B. `src-6-vkqbfn` revision 6 evaluates `focus-6`, target B, `cause=automatic-admission decision=equality reason=already-target`; core `src-7-xqvrwa` retires that same tuple by equality. C repeats this at revisions 8/9 with `focus-8`. This proves retained admission satisfaction, not that a tiler setter caused native map activation. |
| Positive-close retained successor and native call-issued control | PASS | `stage-a-focused` revision 10 proves A focused. One scoped helper activates C then B in the same evaluation; `before-close-b` revision 11 proves B focused. Closing B succeeds; `after-close-b` captures native fallback C. `src-c-127zonw` revision 12 evaluates `focus-12`, target A, `cause=positive-close decision=called reason=setter-accepted`; the result reports `proven=true`. `close-b-settled` revision 13 independently agrees on native/fixture/core A, matching frames and B absent. Core `src-d-107woso` retires the identical tuple by equality. |
| Positive-close native fallback already equals successor | PASS, no request created | Closing focused A leaves C focused in native/fixture/core state (`close-a-settled`, revision 15). Core `src-e-1x98uyx` revision 14 records C as `cause=positive-close decision=equality reason=already-target request=not-created`, with `id=- issuingRev=-`. No retained request or setter is claimed for this branch. |

- First-window admission additionally exposes a truthful superseded no-call:
  `src-3-d3yl68` revision 3 evaluates `focus-3` with `decision=no-call
  reason=superseded`; core revision 4 then satisfies it by equality. This is
  not an admission-actuation claim or a demonstrated product defect.
- Collection qualifications: some immediate native captures precede application;
  only the explicitly settled checkpoints establish final state. An initial
  scripting-error query used a malformed cursor extraction and failed; the
  corrected extraction selected only the opaque cursor and the recapture
  succeeded. No product mutation was repeated for that collection failure.
- Workspace observations: the pre-existing test workspace named "Focus
  diagnostics" was present initially and retired during ordinary controller
  lifecycle churn. A new test workspace was appended beyond that roster.
  Teardown selected a surviving original workspace; no exact restoration is
  claimed and no workspace-retirement defect is inferred.
- Lifecycle: all 19 nonce-named helpers retain source, plugin, signed load ID,
  pre-load child roster, exact-object introspection and raw run/unload replies.
  Every helper was unloaded with named-plugin false and exact-object absence
  before the next helper or controller teardown. Final checks preserve the
  initial sole script object; three predecessor helper names also read false.
- Teardown: fixture shutdown succeeded; owned native windows, server, socket and
  private directory are absent. Combined tmpfs dev, planner and tray logs were
  copied before teardown. `dev-off` unloaded the controller and stopped the
  planner; terminating the exact verified foreground shell through its cleanup
  trap removed the tray and eight followers. Final status is DOWN, with no
  `ydotoold` and no scripting-category errors. All recorded child PIDs are absent;
  two top-level launch wrappers have exited but remain defunct under their parent
  (`final-wrapper-states.txt`, `final-accounting.json`). Complete PID-absence
  proof was open for those two wrappers at the audit; a later direct check found
  both entries gone. No running test process remained.
- Independent saved-raw-evidence audit accepts the three scoped verdicts,
  evaluated readiness and all 19 helper lifecycles, with the two exited-wrapper
  PID-absence proof explicitly open. An audit-summary parsing error initially
  called wrapped presentation replies revision-null; decoding their embedded
  JSON confirms revisions 8/12/14, without changing any verdict.
- The earlier smoke failures remain historical evidence-gate failures, not
  demonstrated product failures. Enabled hover/float guards, physical shortcuts,
  unavailable-pending-target native command subjects and complete intent-restart
  settlement remain outside this native result. The diagnostic/native checks
  above are verified; the two exited-wrapper entries were later found absent,
  completing teardown accounting.

## Source anchors

- E1: `docs/principles.md:135-171`.
- E2: `docs/changes/core-owned-close-refocus.md:17-20,24-63,126-148,198-234,264-320`.
- E3: `crates/tiler-core/src/world_run_v4.rs:444-509,590-679`;
  `crates/tiler-core/src/world_publish_v4.rs:186-207`.
- E4: `docs/porting/world-contract.md:932-993`.
- E5: `crates/tiler-core/src/world_focus.rs:50-88,133-183`;
  `crates/tiler-core/src/world_focus_settlement_tests.rs:313-329`.
- E6: `crates/tiler-core/src/world_close_focus.rs:80-184,226-355`;
  `docs/porting/world-contract.md:947-983`.
- E7: `crates/tiler-macos/src/world_dispatch.rs:1889-1948,1972-2039,2234-2281,2330-2335,2424-2485,2538-2666`.
- E8: `docs/changes/v4-phase3-macos-port.md:289-310`;
  `docs/spec/reference-outcomes.md:6692`.
- E9: `crates/tiler-core/src/world_report.rs:297-433`;
  `crates/tiler-protocol/src/world_protocol.rs:5319-5385`.
- E10: `crates/tiler-core/src/world_selection.rs:155-196,935-1001,1030-1083,1110-1155`.
- E11: `docs/porting/world-contract.md:1156-1199,1202-1259`.
- E12: `docs/decisions/commands.md:21-24`;
  `crates/tiler-core/src/world_run_v4.rs:364-409,466-509,578-619`.
- E13: `docs/decisions/focus.md:14-22`;
  `crates/tiler-core/src/world_sends.rs:46-91,150-160,197-208`;
  `crates/tiler-core/src/world_operations.rs:43-97`.
- E14: `crates/tiler-core/src/world_report.rs:2434-2483`.
- E15: `docs/decisions/focus.md:4-6`;
  `docs/decisions/layout-and-admission.md:81,95,102-104,165`;
  `docs/decisions/macos.md:79-81`.
- E16: `crates/tiler-core/src/world_model.rs:1313-1464`.
- E17: `crates/tiler-core/src/world_cursor.rs:19-117,120-197`.
- E18: `kwin/src/world-report.ts:207-220,1613-1629`;
  `kwin/src/world-apply.ts:2661-2730,2784-2866`.
- E19: `crates/tiler-macos/src/world_dispatch.rs:5571-5709,5775-5977`;
  `docs/changes/core-owned-close-refocus.md:430-445`.

- Technical baseline evidence was source inspection; product approval is
  recorded above. Partial implementation evidence and its unresolved blocker are
  recorded separately. The full proof plan is not complete; earlier close
  test/native evidence retains its original limits and does not establish the
  generalized design.
