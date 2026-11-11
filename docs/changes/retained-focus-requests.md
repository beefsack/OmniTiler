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
  remove Focus from `effects`. Keep the otherwise unused raise/stack shape
  reserved, without treating it as an implemented producer. Reject mixed old/new
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
