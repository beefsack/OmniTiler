# v4 phase 1: core and protocol

## Goal

- Implement the approved [v4 world contract](../porting/world-contract.md) in
  core and protocol, using the exact message and ordered-scenario fixtures.
- Replace v3 at cutover without compatibility shapes. Adapter ports follow in
  phases 2-4; temporary adapter build failures are expected.
- Keep all eight OPEN/TBD fixture gates explicit. Offline conformance does not
  establish native recovery, storage survival, fullscreen or rendering evidence.

## Planned increments

1. Exact JSON primitives, handshake and settings vocabulary/codecs, with
   fixture-backed tests for canonical bytes and strict envelope validation.
2. Finish two-way v4 report/reply types and codecs after presentation revision,
   including held rows, all closed catalogs and exact message fixture comparisons;
   remove legacy envelope keys and bump the version without compatibility shapes.
3. Exercise a development-only KDE vertical slice: handshake, report/reply loop,
   selection, arrival, focus feedback, then state targets.
4. Complete scaled metrics, physical reconnect/rehome and the ordered scenario
   runner, informed by KDE findings; preserve the eight evidence gates.
5. Consolidate inherited contract rules and remaining v3 removal, including the
   v3 guide at cutover, and run build/test/clippy gates plus adapter build-failure
   inventory. Sequential platform conformance still precedes live release acceptance.

- Remaining dependency order (User 2026-11-04): finish presentation revision;
  full two-way v4 codecs, legacy envelope removal and version bump; KDE vertical
  slice; scaled metrics, physical reconnect/rehome (including held-output member
  loss), scenario runner and remaining v3 removal. Accepted-report/rejected-command
  handling and offline visibility/focus/selection are complete; native acceptance
  remains open. The KDE slice is development-only until scaled metrics and rehome
  land. Core quality and shared-platform design discussion govern host findings;
  no KDE-specific workaround is authorized. See the
  [migration decision and principles](../decisions/contract.md#contract-authority-and-v4-migration).
- Visibility/focus/selection is split at its missing evidence vocabulary. First
  add required focusFeedback ingress with exclusive native/output arrival shapes,
  nullable raw foreground facts, report-wide bounds, digest/size accounting and
  connection-owned host-arm validation. Nested immutable rows exercise the staged
  envelope; no full v4 envelope or settlement claim follows. Focus grants,
  switchSettled output, general visibility targets and native-free selection
  completion follow separately. This first slice emits no new authority, removes
  no legacy envelope keys and leaves renderer input and version cutover untouched.
- Next add the existing OneShotEffect correlation vocabulary to both reply writers
  and request-time sizing reserves: cause, nullable command sequence and
  awaitShown. Correlation uses typed cause/sequence, not effect-id spelling.
  Existing producers retain their identities and focus behavior, publishing
  awaitShown=false until explicit-selection readiness/settlement is implemented.
  Immutable true/false shape witnesses establish encoding only, not a new grant.
- Focus settlement next consumes ordinary (`awaitShown=false`) grants actually
  published by the engine. Retain only the latest focus batch (currently at most
  admission plus explicit command), across observation-only revisions, and bind
  feedback by the entire opaque id/target/issuing-revision tuple. Preserve the
  first correlated call/no-call facts and original stamp; later contradictory,
  superseded or ended-lifetime evidence cannot replace them. Foreground proof
  requires the reported equality proof and matching post target, not setter
  success. A refused setter with a readable unchanged non-target foreground is
  refusal evidence; unknown, blocked and other unproven facts remain unproven.
  These are historical ordinary-focus verdicts, not current foreground authority.
  Accepted focus memory remains unchanged. This bounded internal prerequisite
  emits no SwitchEvent or visibility authority: frozen switch membership,
  arrival-ready selection grants and visibility accounting remain dependent work.
  The automatic-effect identity bound is handled by the prerequisite below.
- Before arrival-ready grants, close the existing automatic-effect identity
  bound gap: intern each automatically focused live lifetime to its first
  automatic-focus candidate's issuing revision and emit compact af-/ffc-
  identities from that token. Reuse it across hover transitions and observations,
  retain it through holds/exclusions/incomplete gaps, and retire it only with
  lifetime evidence. An admission candidate can be dropped by same-target explicit-focus
  deduplication after its token is allocated; the token is not a first-publication
  receipt. At most one automatic candidate is minted per accepted report, so the
  globally increasing issuing revision gives exact distinct tokens without
  hashing, truncation or another counter. Existing admission-spent and hover
  guards remain unchanged. Update canonical size witnesses to the compact
  producer bounds and verify maximum-length subjects through real producers
  and feedback correlation. Visibility targets and their frozen arrival set
  must precede an awaitShown grant; a show-only selection that still focuses
  without awaiting arrival is not an ordering implementation.
- Next integrate native-free, suppression-capable explicit selections that
  actually resolve a focus target: publish same-output selected-home shows and
  departing-home hides together with an awaitShown focus grant, never an ordinary
  focus alongside newly authorized shows. Preserve per-lifetime visibility
  authorization revisions across observations; new intent, not disagreeing facts,
  changes authorization. Composed sticky remains shown and native-owned paths
  remain unchanged. Freeze arrival members and their final published target
  frames after command restaging/layout, not from pre-layout command inputs.
  Correlate first feedback by the full effect tuple and prove every frozen
  arrival from live lifetime, sample watermark, shown/non-minimized/non-hidden,
  observed output/frame and retained logical home facts. A foreground equality
  flag without the full required arrival set never establishes departure
  readiness. Keep ordinary-focus settlement separate, accepted-choice memory
  unchanged, fallback disabled and raw feedback intact. No SwitchEvent, selector
  flight completion, empty/no-focus selection completion or native-owned arrival
  migration is included in this first integrated route. Existing adapter staging
  rules, not independent hide authority, enforce show-arrive-focus-depart and the
  cross-output awaitShown barrier.
- Selection visibility keeps its active per-output selected/departing scope
  separate from the finite frozen focus grant. A frozen member ending its
  lifetime invalidates that grant without deleting surviving output policy or
  letting a reused lifetime inherit its authorization. Shared-mode fanout keeps
  a scope for every affected output. Reconcile those scopes against the current
  core view before publication, including view changes outside selection commands;
  neither a stale show nor a hide of the newly selected home retains write
  authority. Held screens/windows and incomplete inventories are not output or
  lifetime loss. All controlled desired-shown members on the focus output,
  including composed sticky, belong to its issuing arrival snapshot.
- The next bounded selection slice adds required `switchSettled` encoding and
  per-member accounting for the existing focus-bearing native-free route. Freeze
  visibility targets across all affected outputs separately from the focus
  output's arrival set; later admissions do not join either frozen switch set.
  Correlate raw LastWrite by lifetime, polarity and authorization revision,
  preserve spent evidence, and distinguish proof from setter acceptance. Once
  every frozen member and the focus attempt have terminal accounting, publish
  one event on the existing parent operation and retire its native-free selector
  flight without logical echo authority. Terminal partial-hide failures do not
  retry or roll back arrival. Scopes and stable targets survive completion;
  fallback, empty/no-focus and native-owned completion remain excluded. Verify
  producer-driven success/failure, replay/rollback/lifetime/supersession fences,
  both canonical writers and pre-mutation size bounds before the final gates.

- Intent dependency order: validate/canonically encode the core-owned opaque
  inner record first; then wire outer report/reply records and consume them in
  lifetime-fenced engine restore/save/clear. Inner-data tests alone do not prove
  engine restoration, host storage, or complete message codecs.
- Import consumption is a bounded restore-only increment for non-sticky explicit
  float/tile records before save/clear and state integration. User 2026-11-03
  approved first-successful-admission import: preserve pending records through
  held/unreadable observations, then ignore late records without overwriting
  newer runtime intent. Adapters must return storage in or before the first
  admissible report; native ordering remains a port acceptance check.
- Nonempty publication follows as a save-only increment: successful explicit
  float/unfloat commands save the selected float/tile override in the same reply,
  with a pre-mutation size reservation. Deferred sticky records must not be erased;
  sticky semantics and surviving-lifetime no-intent clears remain separate work.
- Sticky restoration first needs capability-aware native fact authority: composed
  sticky carries raw null rather than a fabricated native bool. This prerequisite
  precedes semantic sticky restore/save, visibility and native-state actuation.
- Semantic sticky restore is a separate core-membership increment: accept every
  sticky-bearing record whole, retain its exact prior mode, and restore sticky
  float classification without native targets or import re-saves. Sticky commands,
  visibility/state targets and canonical sticky save production follow later.
- Outbound visibility/native-state plumbing adds the exact WindowDesire target
  shapes and canonical byte accounting before target-producing policy. Existing
  frame/clear command targets gain typed command correlation; commandless born
  clears keep null. Initial visibility remains null/untouched and state remains
  empty, so this slice grants no new actuation. State-attempt ingress/settlement,
  visibility policy, actual sticky transitions and composed maximize follow in
  dependent increments; no tolerant compatibility shapes are introduced.
- StateAttempt consumption follows as a required, bounded raw lane with exact
  target/revision correlation, capability-conflict diagnostics and spent evidence
  retained before readability holds. Internal bound-target fixtures establish
  confirmation/refusal/ambiguity handling before command producers are added;
  unmatched evidence grants no authority. This slice must not emit new targets,
  fabricate native facts or claim sticky/state command completion.
- State increment includes the approved [core-composed maximize route](v4-migration.md#core-composed-maximize-amendment)
  after lifetime-fenced intent consumption: maximize=true native unchanged;
  maximize=false uses existing frame authority and a runtime-owned overlay bit,
  with no adapter policy or opaque-intent extension. Composed implementation covers
  ordinary tile/float toggle, base restoration, capability grading,
  fullscreen/frame-policy integration, gestures, sends, directional operations and
  intentional unfloat. Empty-destination float drags match the existing ordinary
  path after clearing the bit; ordinary-path geometry loss is not repaired here.
  Final verification and the tentative directional preflight choice are below.
- Sticky commands are split into composed and native increments. The composed
  increment adds neutral set/toggle WorldCommand variants, same-output visibility
  and canonical saves, replacing the ToggleFloat fence only on that route.
  The native increment adds target production and settlement, replacing its
  remaining command fence; sticky-off must not tile before proven clear.
  The selected
  [saved intent after proven native refusal](../spec/reference-outcomes.md#native-sticky-refusal-and-saved-intent):
  rule preserves the required crash-safe pre-write save, then restores the prior
  saved record (or null-clears if none existed) on proven refusal. Ambiguous or
  unproven outcomes retain the requested record for later fact-authority
  reconciliation. Neither path retries automatically. This follows the existing
  failed-transition policy; native outcomes remain TBD. Persistent above and
  remaining state, including core-composed maximize, remain dependent work.
- Next bounded state slice: native fullscreen set/toggle commands, strict codecs,
  per-property target publication and existing correlated settlement. Native
  maximize and persistent above follow; core-composed maximize remains a separate
  capability-routed increment. The [maximize-to-fullscreen discriminator](../spec/reference-outcomes.md#native-fullscreen-entry-from-maximized)
  records the 2026-11-03 selection: enter immediately from maximized, preserving
  maximize as an independent overlay underneath; ordinary exit returns to
  maximized (COSMIC, not Hyprland project exit-to-base). Native route writes only
  fullscreen, with no unmaximize on entry or extra maximize on exit; host flag
  retention and KWin/Windows outcomes remain TBD. The composed bit survives
  fullscreen and its work-area frame is recomputed on exit. R-MAX-11 remains
  the separate explicit fullscreen-to-maximize replacement.
- Before native maximize/above production, harden the adjacent native-sticky
  publication and command-sizing paths: pending same-window transfers must hold
  native write authority, and command estimates must match encoded spelling.
  Keep these corrections separate from new state policy and composed maximize.
- Native maximize/above follows the existing per-property target/attempt lane.
  Keep Maximize results descriptive, not write authority; explicit R-MAX-11
  replacement authorizes the needed fullscreen-clear and maximize targets in one
  reply, with clear-before-maximize native ordering and no intermediate readback.
  User 2026-11-03 selected retaining an unattempted maximize-on/off target but
  holding it while newer fullscreen-on is pending or active, including unchanged
  raw facts, then applying it after exit. No unordered fullscreen-on/maximize
  write authority shares a reply. Settled overlay preservation and R-MAX-11
  replacement remain unchanged; COSMIC save/restore is the nearest analogy,
  not proof of an equivalent external pending-target model or native ordering.
  Persistent above uses the approved float union, one ownership/external-clear
  latch and commandless correlation. Durable preimages remain the adapter's
  mandatory persist-before-write obligation, not a new core fact or wire field.
  Extend target reservations from exact encoder witnesses for actual coexistence;
  composed maximize remains a separate dependent increment.

## Selection integration evidence

- A temporary held-screen recovery probe establishes a genuinely changed target,
  not an observation-only renewal: `w3` changes from `(out-b,ws-2,shown:false,rev:5)`
  to `(out-b,ws-4,shown:true,rev:7)` with changed layout geometry. The other
  output's targets, `w4` authorization and the original frozen arrival set retain
  their revisions/snapshot. The held report itself preserves selection scopes,
  targets and the grant without controlled writes on the held output.
  Source comparison at `e059d15c` shows the member-pruning, logical-homing and
  ring-convergence paths unchanged: held screens are omitted from the legacy
  member sweep's live outputs, and recovery re-admits at the effective view.
  This is source-identity evidence plus the current probe, not baseline execution.
  The selection lane reacts to that pre-existing home/polarity change with fresh
  target authorization. Its corrected held-recovery oracle asserts the exact
  changed home/polarity/revision and unchanged survivor/frozen authorization;
  held-output membership/rehome policy remains in the reconnect/rehome increment,
  not a selection-lane policy change. Probe edits have been removed.
- Review corrections for stale hides after non-selection view changes and active
  output scope separation are present with focused regression evidence. Shared
  fanout and member-close scope survival have focused coverage. Twenty-three
  focused core regressions also cover composed-sticky arrival inclusion, complete
  and partial brackets, freshness/lifetime/frame/home mismatches, late admission,
  first-feedback retention, transaction/replay fences and historical readiness
  without fresh write authority. The real producer reply matches both canonical
  writers in a protocol regression and remains within existing lane reserves.
  Initial failures were a refused-Send fixture setup, not a behavioral red;
  the later stale-hide regression reproduced Controlled authority after a follow
  view change before its central-reconcile correction. A fresh final review has
  no outstanding in-scope finding.
- Final verification passes the exact filtered core/protocol gate (1565 core unit
  tests, all selected integration targets and protocol suites
  49/12/4/15/1/8/2/59/4), core/protocol build, scoped all-target clippy with warnings
  denied, formatting, whitespace, ASCII and local link-path checks. Only
  workspace_unsafe_posture is filtered. Focus-bearing native-free suppression-
  capable selection now publishes stable visibility targets, an awaitShown grant
  and a frozen post-layout arrival set with correlated readiness evidence.
  That first slice did not establish SwitchEvent/selector-flight completion, empty/no-focus
  selection completion, native-owned arrival migration, native adapter ordering
  or full v4 codec/scenario conformance. No adapter, immutable fixture, version
  or approved-policy files change; all eight OPEN/TBD gates remain unchanged.

## Native-free switch accounting evidence

- The focus-bearing suppression-capable selection route now freezes every
  command-granted visibility target across affected outputs separately from the
  focus output's arrival snapshot. First-correlated LastWrite evidence binds the
  live subject, desired polarity and authorization revision; live and lossless
  held rows contribute raw evidence before command rollback and readability
  holds. Target confirmation requires lifetime-matched readable native post
  facts within the issuing/receipt observation bounds, not setter acceptance.
  Verdicts latch at receipt, so a future-dated or contradictory later sample
  cannot upgrade the original attempt. Missing or uncorrelated evidence remains
  pending, not a timeout failure. Late admissions do not join the frozen set.
- Once every frozen target and the focus attempt have accounting, required
  `switchSettled` publishes one event on the existing parent operation. Complete
  requires no failed visibility targets and arrival-gated proven focus; a focus-
  only failure has an empty failed list and completed-with-failures outcome.
  The finite binding and actual native-free selector parent retire without
  logical echo authority, while active scopes, logical homes and stable target
  revisions survive. No retry, fallback, memory rollback or fresh focus grant
  follows. Duplicate requests replay the cached terminal; subsequent reports
  publish an empty event section. Native-owned and empty/no-focus completion
  remain outside this route.
- The approved [terminal partial-hide discriminator](../spec/reference-outcomes.md#rapid-unlanded-workspace-arrival)
  is pinned with three real-engine members: arrival and foreground prove, one
  departure hide succeeds and the other has a readable unchanged refusal. The
  result is completed-with-failures with exactly that failed hide and proven
  focus; its home/revision survives with no retry or rollback. Reference-WM and
  native outcomes remain TBD; no equivalent reference receipt model is claimed.
- Twenty-two additional core regressions cover success, blocked/refused/ambiguous
  targets, missing evidence, historical receipt bounds, held/conflicting rows,
  close/supersession, frozen late membership, Shared fanout, simultaneous output
  completion, rejected-command observation retention, incomplete inventories and
  duplicate delivery. A real producer terminal and synthetic maximum-token
  events match both canonical writers; marginal event sizes are exact. The
  pre-mutation reserve covers 117 bytes per event including its comma and 131
  bytes per failed maximum-length id, with all retained bindings counted.
  Boundary rejection preserves the state and permits a smaller same-sequence
  retry. Review corrected the initial event reserve and added missing lifecycle
  checks. Most regressions were first executed after implementation, not a
  claimed preimplementation behavioral red. Initial test failures corrected
  fixture/oracle setup; the required empty-section golden needed one narrowly
  causal update, not a weakened contract assertion.
- Final verification passes the exact filtered core/protocol test gate (1587
  core unit tests, every selected integration target and protocol suites
  50/12/4/15/2/8/2/59/4), core/protocol build, scoped all-target clippy with
  warnings denied, formatting, whitespace, added ASCII and local link checks.
  Only workspace_unsafe_posture is filtered. This remains a staged wire
  increment, not strict whole-reply decoding, native ordering/delivery or full
  scenario conformance. Adapter constructors/interfaces still need the required
  event lane during their ports; no adapter or immutable fixture changes are
  included. All eight OPEN/TBD gates and the held-output rehome follow-up remain.

## Focusless switch completion decision

- User decision 2026-11-04 resolves the absent-focus question using the existing
  vocabulary: focus=unproven and outcome=complete once all visibility obligations
  are satisfied, with failed=[] when nothing failed. This includes zero-member
  empty-to-empty and native-owned focusless selections. Focus-bearing switches
  still require proven focus; empty/minimized-only keeps no focused client.
  The [canonical decision](../decisions/workspaces.md#focusless-switch-completion)
  and [minimal discriminator](../spec/reference-outcomes.md#empty-workspace-focus-across-outputs)
  preserve the unsupported native/reference terminal outcomes as TBD.
- Implemented by extending the common completion predicate and retained selection
  binding, not a separate focusless completion path. Frozen departure visibility
  obligations require no invented effect or feedback row;
  native-owned selections use actual native selector evidence, never logical
  echo authority. No token, wire, fixture, fake focus, fallback or retry change.
  Focused and focusless success/failure, zero-member completion, actual selector
  release, lifetime/supersession/replay fences and size bounds are verified offline.
- Tier 1 EW-C/H/I/S source-only evidence establishes empty client-focus behavior,
  not an equivalent receipt protocol. The selected representation is a product
  decision, not a claimed native/reference terminal result.

## Focusless switch completion evidence

- The existing retained selection carries an optional focus effect and freezes
  visibility obligations independently. One terminal predicate and one stager
  account for focus-bearing and focusless switches, retire the actual view parent
  and publish one take-once event. Native-free zero-member focusless selections
  complete in the issuing reply; their possible event is reserved before mutation
  alongside terminals from older bindings. Empty/minimized-only selections mint
  no fake focus or feedback requirement. Failed visibility members still produce
  completed-with-failures without retry or rollback.
- Native-owned focusless bindings freeze native selector obligations per affected
  output, not per-window suppression targets. Exact native echoes or correlated
  accepted target posts prove arrival; proven unchanged-preimage refusals terminate
  with failure, while missing, wrong-tuple or logical echoes do not prove a native
  switch. First correlated raw attempts survive holds and command rollback;
  recovery does not upgrade them from later contradictory attempts. An independent
  fresh native target echo remains proof. Shared same-native-id fanout requires
  unanimous output accounting; partial observations hold. External native moves
  supersede the binding without a stale terminal.
- Seventeen additional core regressions cover empty/minimized-only routes,
  visibility success/refusal/missing evidence, native success/refusal and held
  receipt recovery, first-raw retention, Shared unanimity, supersession,
  issuing/replay/rejected-command fences and simultaneous-event sizing. The
  first-raw regression reproduced a later contradictory receipt replacing the
  historical attempt before correction. Other new cases were not established as
  preimplementation behavioral reds. Existing empty-selection flight-retention
  oracles now assert issuing completion and no resurrection; the final integration
  gate exposed one further stale oracle, corrected without production changes.
  A question-mark lint was repaired without changing terminal semantics.
- Independent review found no outstanding in-scope defect. Fresh final verification
  passes the exact filtered core/protocol gate (1604 core unit tests, every selected
  integration target and protocol suites 50/12/4/15/2/8/2/59/4), core/protocol build,
  scoped all-target clippy with warnings denied, formatting, whitespace, added ASCII
  and local-link checks. Only workspace_unsafe_posture is filtered. Native-owned
  focus-bearing arrival migration, native adapter ordering/delivery, full v4 codecs
  and scenario acceptance remain excluded. No adapter, immutable fixture or version
  changes are included; all eight OPEN/TBD gates and held-output rehome follow-up
  remain unchanged.

## Native-owned focus-bearing arrival migration

- Native-owned focus-bearing selections now reuse the existing explicit-selection
  binding and awaitShown focus. The focus output's selected-home arrival members
  and final published frames freeze independently of native selector obligations
  across affected outputs. Per-window visibility targets remain null; no
  suppression write or LastWrite requirement follows from native desktop ownership.
- Native-arm arrival homes compare through the existing native-to-logical binding
  against each frozen target, preserving full lifetime, watermark, readable frame,
  visibility, minimized and host-hidden checks and first-feedback retention. Native
  selector proof remains separate from foreground and arrival proof; logical
  echoes never become native authority. The common terminal/stager, existing
  native refusal handling and lifecycle fences are reused without new wire shapes.
- Twenty-six additional core regressions cover actual grants and complete/proven
  terminals, all-member arrival failures and wrong evidence, native selector
  holds/refusal, Shared fanout, supersession, closes and size bounds. Existing
  rollback/replay checks remain passing. The ordinary-focus grant was reproduced
  before migration; review regressions also reproduced omitted frameless members,
  premature accounting loss on own native refusal and sticky focus missing its
  actual frozen home. Corrections cover split-report refusal/feedback and
  frameless/float/fullscreen subjects. Native-owned focus targets fail readiness
  if absent from the frozen snapshot; that omission is an artificial invariant
  test, not a reproduced producer failure. Other new tests are not claimed as
  preimplementation behavioral reds.
- The Shared partial-then-external-move regression is resolved. Selecting n1,
  observing only the focus output reach n1 while the other remains at n0, then
  observing that other output move to n2 previously left the old binding alive:
  the partial report had removed its output scope. The focused regression
  `host_owned_focus_bearing_shared_partial_then_external_move_drops_binding`
  now proves binding retirement and no late-feedback terminal. Initial covering
  removal broke pending partial proofs; a later filter restored those checks but
  still removed the scope. Moving the departing check before stale removal fixed
  that sequence but retained abandoned accounting on redundant selection back
  home. The retained guard also requires the focus output to prove its selected
  home, preserving both partial progress and supersession. No new state or
  per-output accounting structure was added; native-free and focusless paths are
  unchanged. Own native refusals retain historical accounting for actual delayed
  feedback, without resurrecting the native view flight or retrying actuation.
- A fresh independent review found no blocking finding. Direct final verification
  passes the exact filtered core/protocol gate (1630 core unit tests, every selected
  integration target and protocol suites 50/12/4/15/2/8/2/59/4), core/protocol build,
  scoped all-target clippy with warnings denied, formatting, whitespace, added ASCII
  and local-link checks. Only workspace_unsafe_posture is filtered. This closes the
  planned offline native-owned focus-bearing arrival migration, not native adapter
  delivery, full v4 codecs/scenarios or reconnect/rehome. No adapter, immutable
  fixture, version or new wire vocabulary changes are included; all eight OPEN/TBD
  gates and unsupported native/reference outcomes remain unchanged.

## Common presentation revision publication

- Add the required `WorldPresentation.rev` to the real producer, both canonical
  writers and exact size accounting, with an explicit worst-case envelope reserve.
  Ready replies already recompute presentation at publication; label that result
  with the issuing world revision. Empty not-ready presentations use the current
  world revision without advancing it or granting authority. Cached duplicates
  retain the original bytes. This is an issuance label, not a native draw receipt.
- Preserve independent typed revision values in the writer: standalone message
  fixtures are shape witnesses, not transitions, and may carry a presentation
  revision different from the enclosing world revision. The ordered decoration
  scenario advances the presentation label when recomputing on a newer report.
- Verify real accepted, command-refused, consumed, bootstrap and replay paths;
  quoted counter ordering in both writers; exact marginal sizing through maximum
  counter width; and pre-mutation bounds. No new counter, revision-based input
  refusal, retained-presentation optimization or draw receipt is introduced.
- Anchored border, presentation ingress, renderer input, scaled styles and other
  metric threading remain separate work, as do reconnect/rehome and held-output
  member loss. No adapter, immutable fixture or version changes are included;
  all eight OPEN/TBD gates remain unchanged.
- Implemented and verified through the actual publication paths and shared
  encoder. Three new regressions plus extended refusal/consumed/bootstrap checks
  cover issuing labels, same-observation recomputation, cached replay, independent
  typed revision values, maximum quoted counters and canonical optional-field
  order. The field adds exactly 9 punctuation bytes plus counter digits; the
  explicit 29-byte pre-mutation reservation has a regression-sensitive equality.
  Existing conservative estimate slack remains unchanged. Independent review
  found no blocking issue; its reserve-test weakness was corrected without a
  production change. The bootstrap golden required one narrowly causal field
  addition after its expected byte mismatch; no unrelated oracle was weakened.
- Direct final verification passes the exact filtered core/protocol gate (1632
  core unit tests, all selected integration targets and protocol suites
  50/12/4/15/2/8/2/60/4), core/protocol build, scoped all-target clippy with warnings
  denied, formatting, whitespace, added ASCII and local-link checks. Only
  workspace_unsafe_posture is filtered. This is required-field publication, not
  complete v4 codecs, renderer-input validation, native draw evidence or full
  presentation/scaling acceptance. The phase note remains active.

## Two-way v4 codec sequence

1. `lossless-report-wire-optionals`: retain optional wire spellings separately
   from normalized engine input.
2. `v4-presentation-input-rows`: strict nullable ingress, FrameWitness and
   RendererInput row vocabulary/codecs.
3. `v4-anchored-border-rows`: descriptor vocabulary, row codec, hide reasons and
   exact sizing.
4. `v4-report-two-way-codec`: complete report decode/encode and exact positive,
   held-row and rejection fixture coverage.
5. `v4-reply-two-way-codec`: complete reply decode/encode and exact positive and
   rejection fixture coverage.
6. Required engine bridge: genuine presentation production and RendererInput
   validation before real-engine v4 conformance; no fabricated witness, synthetic
   authority or forced-hidden border substitutes.
7. Public cutover integration: version bump, legacy-envelope removal and affected
   broker/service/tool integration. Verification must cover those paths as well as
   core/protocol; a core/protocol-only result does not establish their acceptance.

- Keyboard-step, resize-minimum and drop-zone scaling, physical reconnect/rehome
  and native proof gates remain deferred. Staged row codecs grant no new engine
  authority and introduce no parallel accepted envelope/version shapes.

### Lossless report wire optionals

- Preserve omitted/null/empty/id cursor-hit spellings; omitted/present clock;
  omitted/present attempt arrays; and omitted/null/object gesture ingress. Keep
  wire metadata in protocol-owned decoding output, not WorldReport equality,
  hashing, command policy or retained engine state. Existing decode_request callers
  continue to receive the same normalized report and errors.
- Verify real decoder capture, immutable optional-row witnesses and test-only
  reconstruction of their exact spellings, including nonempty lanes, maximum
  counters and held-row retention. Equal normalized reports must still hash alike
  and replay as duplicates despite differing wire spellings; changed values remain
  fenced. No complete request writer, v4 whole-envelope decode, version bump,
  legacy-key removal, presentation migration or adapter work belongs to this unit.
- Implemented protocol-owned `DecodedRequest`/`ReportWireOptionals` through
  `decode_request_with_wire`; existing `decode_request` delegates to the same
  strict gates and returns only the unchanged normalized report. Cursor spelling
  retains no duplicate lifetime string. Core input, digest and equality are
  untouched. Production encoding remains deferred to the complete report codec.
- Eight new protocol regressions cover spelling capture, metadata-driven exact
  reconstruction of immutable optional fragments, nonempty lanes, gesture
  objects, maximum counters, malformed-value parity, held-row field order and
  native-workspace/primary preservation. Real-engine duplicate replies retain
  identical bytes across normalized-equivalent spellings; changed values retain
  request-reuse rejection. Initial equality setup changed the cursor as well as
  its hit spelling; correcting that setup established the intended equivalence,
  not a behavioral red. Independent review found no behavioral blocker; a comment
  correction and explicit reconstruction branches closed its two minor findings.
- Direct final verification passes the exact filtered core/protocol test gate
  (1632 core unit tests, every selected integration/protocol target, including the
  eight new optional tests), core/protocol build, scoped all-target clippy with
  warnings denied, formatting, whitespace, added ASCII and local-link checks.
  Only workspace_unsafe_posture is filtered. Immutable v4 envelopes still fail
  their existing whole-message gates: these are isolated fragment witnesses, not
  complete v4 report roundtrips. No fixture, adapter or version changes are
  included, and all eight OPEN/TBD gates remain unchanged.

### v4 presentation input rows

- Add staged core value types and strict canonical row codecs for v4
  PresentationIngress/pager frames, FrameWitness and RendererInput/RenderTarget.
  Preserve nullable raw foreground facts without turning unknown into false.
  Existing WorldReport/PresentationIngress, decoder envelope gates, engine policy,
  report equality and digest remain unchanged until their later integration units.
- Validate closed keys, field types, identity/geometry/counter bounds, explicit
  nulls and canonical key order. FrameWitness `win` and `foreground` require
  nonempty WindowIds; only ingress `foreground` and `moveWindow` permit the
  contract's explicit empty sentinel. Shape validation grants no causal proof,
  renderer freshness verdict or command authority.
- Verify isolated immutable rows: 196 valid presentation rows, 11 expected
  presentation-row refusals, 196 standalone witness objects, 195 valid pager-frame
  rows plus two removed-key refusals, and all three renderer target arms plus
  three malformed target witnesses. Command/input pairing and narrow report holds
  belong to the full report codec; native witness joins and renderer validation
  remain engine/port proof obligations. Check constructed boundary/negative rows
  and unchanged whole-envelope rejection without altering the fixtures.
- No anchored-border reply, settings/style application, renderer consumer, full
  message codec, version bump, legacy-envelope removal, adapter or live work is
  included. All eight OPEN/TBD gates and deferred scaling/rehome work remain.
- Implemented staged core types plus canonical checked encoders and strict row
  decoders. Byte entry points retain duplicate-key rejection and the input-size
  gate; Value entry points consume already-parsed values. Encoders validate their
  constructed bytes before returning them. Neither existing whole-report decoder
  calls these codecs, and the existing v4 whole-envelope rejection is unchanged.
- Ten protocol regressions establish the isolated-row census above, exact bytes,
  per-slot identity boundaries, nullable facts, counters, rectangles, caps,
  duplicate/escaped-duplicate rejection and invalid typed-output refusal.
  Well-formed dangling witnesses remain shape-valid without causal authority.
  Independent review found no behavioral blocker; corrections removed redundant
  forwarding helpers, clarified documentation and tightened identity/error
  assertions. An initial fixture-loader path error was corrected without changing
  fixtures or acceptance oracles; no preimplementation behavioral red is claimed.
- Direct final verification passes the exact filtered core/protocol test gate
  (1632 core unit tests, every selected integration/protocol target, including the
  ten new row tests), core/protocol build, scoped all-target clippy with warnings
  denied, formatting, whitespace, added ASCII and local-link checks. Only
  workspace_unsafe_posture is filtered. This remains row-codec evidence, not
  whole-report conformance, native witness evidence or engine presentation/input
  integration. The phase note stays active.

### v4 anchored border rows

- Add staged anchored-border descriptor and closed hide-reason types, strict
  canonical row codecs and standalone exact row sizing. Reuse FrameWitness;
  `host-hidden` replaces `cloaked` in the staged vocabulary only. Preserve the
  legacy producer, whole-message writers, reservations and envelope/version gates.
- Validate required nullable slots, closed keys/tokens, opaque identities,
  counters, witness rectangles and integer styles. Use the existing i32 wire
  integer representation; width/radius are nonnegative, while outsets are four
  signed integers, not a Rect or the old report-ingress style cap. Preserve
  independently nullable descriptor fields without inventing causal joins or a
  visible/hidden all-present/all-null rule. An empty reason remains representable
  for hidden rows, as in the existing geometry-veto path.
- Verify every valid immutable reply's isolated border bytes, the two dedicated
  border negatives, the complete hide catalog and constructed boundary/type/key
  failures. Exact row sizing must match checked encoding, including null fields,
  maximum identities/counters and integer boundaries. This does not establish
  complete reply acceptance, producer/native witness validity or reserved sizing.
- No genuine producer, renderer consumer, full-message codec, version/legacy
  cutover, settings/style application, adapter, live, scaling or rehome work is
  included. All eight OPEN/TBD evidence gates remain unchanged.
- Implemented staged descriptor/reason types, checked canonical row encoding,
  strict Value/string decoding and an exact standalone size estimator. All 114
  valid reply border fragments round-trip byte-exactly and size-exactly: 101
  visible descriptors/witnesses and 13 hidden null-witness rows cover the empty
  reason and all nine hide tokens. The two dedicated border negatives refuse the
  removed absolute key and retired `cloaked` token with their distinct errors.
- Twelve regressions also cover all 64 visible/hidden descriptor-null masks,
  hidden empty reasons, dangling identities, required keys, integer/style/id/
  counter/rectangle boundaries, checked-output refusal, nested escaped duplicates,
  the input-size gate and unchanged v3/full-v4-report gates. These are isolated
  shape witnesses, not a producer consistency or native causal proof. Independent
  review found no behavioral blocker; documentation and boundary assertions were
  tightened without a production behavior change.
- Direct final verification passes the exact filtered core/protocol test gate
  (1632 core unit tests, every selected integration/protocol target, including the
  twelve new row tests), core/protocol build, scoped all-target clippy with
  warnings denied, formatting, whitespace, added ASCII and local-link checks.
  Only workspace_unsafe_posture is filtered. Full-reply sizing/reservation and
  genuine engine presentation/input integration remain separate. The phase note
  stays active.

### v4 report codec sub-increments

- Split the complete report codec into lossless typed decoding, then canonical
  writing. The decoder increment covers all 126 valid, 81 accepted-held and 82
  rejected report fixtures; the writer increment must round-trip all 207
  accepted envelopes (205 byte-exact, plus two manifest-designated decoder-input
  fixtures compared by decoded value; see the writer outcome below). Neither increment changes the runtime's
  accepted version or bypasses the later genuine engine bridge and cutover.
- Stage an independent typed v4 report using existing row vocabulary, with no
  logical view echo, legacy presentation defaults or conversion to engine input.
  Preserve malformed identifiable rows as held raw fields, with truthful narrow
  identity/diagnostics where available. Required held presentation is not a
  fabricated empty/default ingress. Retain original row ordering for the writer.
- Keep capability-dependent conflicts separate from shape validation: four held
  fixtures depend on handshake context (unavailable managed/executable facts and
  native-free sticky facts/attempts). The context-free decoder must preserve
  their typed facts, not infer capabilities or manufacture a conflict verdict.
  Contextual grading remains an engine integration obligation.
- Prove each rejected fixture's intended cause with its fixed rejection kind/
  message. The untouched fixture is rejected first; only afterwards a minimal
  in-memory repair at the manifest's invalid pointer must decode, as a positive
  counterfactual rather than a way to reach the failure. This avoids extending
  production errors with fixture metadata or counting incidental earlier
  envelope failures as conformance. Immutable fixtures, public v3 paths,
  adapters and version declarations remain unchanged.
- Decoder outcome (increment 4a): the context-free whole-report decoder
  rejects 80 of the 82 negatives from the untouched fixture with the intended
  kind/message. The other two are byte-identical to valid sources with the
  opposite manifest host-ownership context; the explicit host-context wrapper
  rejects them as snapshot-invalid/arrival-host-mismatch under their declared
  context and accepts the valid twins under theirs, per the existing
  wrong-host rule. Of the 81 accepted-held fixtures, 77 hold structurally and 4
  preserve typed capability-dependent facts. Structural gates (attempt section
  totals including held rows, native order/duplicate/occupancy/known-output
  coverage and global budget across held rows, cursor-hit and action-output
  cross-checks, creation id/receipt/aggregate invariants over held rows,
  presentation key/identity/geometry preflight before any hold) run before a
  narrow hold can mask them. The nested creation receipt spelling (omitted/
  null/empty/id) is preserved as protocol-owned wire metadata aligned to valid
  creation rows. The canonical writer and the round trip of all 207 accepted
  envelopes were increment 4b.
- Writer outcome (increment 4b): the staged checked canonical writer emits
  top-level keys in the contract report-table order (`primary` follows
  `gestureIngress`, before `intentRecords`), reconstructs optional-field and
  creation-receipt spellings from the decoded wire metadata, interleaves valid
  and held rows by retained ordinal, and refuses inconsistent metadata with
  fixed snapshot-invalid messages. Held native occupancy items, pager-frame
  items and the witness object are now retained in wire key order so held rows
  re-encode exactly. 205 accepted fixtures re-encode byte-exact; the decoded
  value of every one of the 207 is stable across encode and decode.
- Two accepted fixtures are marked non-canonical in the fixture manifest and
  compared by decoded value only. `report-text-escaping-relationships` is
  decoder input for the short C0 escapes (`\t \n \r \b \f`); the writer spells
  them as lowercase `\u00xx` per the contract, and a byte-level writer test
  keeps that rule witnessed. The KDE adapter builds reports with
  `JSON.stringify`, whose short-escape output has not been tested inside KWin,
  so it is not claimed as proven. `invalid-window-rect-object` carries an
  object-valued window rect in a held row; the contract sets no key order for
  nested objects in held rows, so no required byte form exists for this value,
  and the writer has no byte-level check for malformed nested objects. No
  ordering rule, parser feature or contract text changes.

### v4 reply codec sub-increments

- Split the reply codec into a staged typed decoder (5a), then the canonical
  writer and byte-exact round trip (5b). Neither changes the runtime's accepted
  version, the legacy reply type or its writers, or the engine's reply producer.
- 5a outcome: a staged standalone `WorldReplyV4` reuses the existing reply row
  types and carries only wire facts. The reply `presentation` is the anchored
  border shape with an independent revision. The strict context-free decoder
  accepts all 114 valid reply fixtures and, for the 23 context-free negatives,
  refuses the untouched fixture with the intended fixed kind and message; only
  afterwards does a single-pointer in-memory repair decode, as a positive
  counterfactual. The other 3 negatives need declared handshake capabilities:
  the context-free decoder accepts them (one is byte-identical to a valid
  fixture with a different manifest context), and the explicit host wrapper,
  which takes the existing declared capability facts and infers nothing,
  refuses an above state target without `above`, a non-null `warpCursor`
  without `warp`, and a sticky state target without native sticky, each with a
  distinct fixed snapshot-invalid message named after the manifest rule.
  One-shot raise/stack effects are not gated. The envelope identity fields use
  the same bounded validators as the report decoder.
- Typed values determine every wire spelling except suspended-output presence,
  which is carried as protocol-owned metadata.
- 5b outcome: the staged checked canonical reply writer re-encodes all 114
  valid reply fixtures byte-exact and its output decodes to an equal value.
  Every spelling is determined by the typed value plus the one wire flag: empty
  sentinels, null-when-absent fields, trailing and no-space markers omitted
  unless true, quoted counters and bare integers. It refuses values the decoder
  would refuse (section caps, duplicates, state-target count, effect and
  visibility pairings, invalid ids, inconsistent suspended-output metadata)
  with fixed messages, and a final decode-and-compare backstop turns any
  remaining mismatch into a fixed refusal. No fixture needed a byte form
  outside the contract. No valid fixture carries a non-null `warpCursor`, so
  its spelling is witnessed by constructed tests only. The legacy writers and
  runtime are unchanged.
- Engine fields a genuine reply needs that do not exist yet (unit 6): the
  anchored border (subject, outsets, width, radius, witness) has no producer,
  and the engine publishes the legacy absolute-rect border; staged transfer
  rows carry a command sequence for which the engine retains no source. The
  codec fabricates neither.

### v4 engine bridge sub-increments (unit 6)

- Shape: a parallel core-internal v4 path (`*_v4` validate, stage and publish
  functions reusing the lane helpers below the staging boundary). The broker is
  never switched, no version bump and no dual decode happens in this unit, so a
  single live report shape remains until unit 7. A generic-over-report engine is
  rejected: it would force unifying presentation rows, transfer targets and
  views across about fifteen lanes.
- Sub-increments, each verified and committed separately:
  1. Pure-function v4 twins of validation, structure checks, grading, digest,
     estimates and canonical encode for `WorldReportV4`/`WorldReplyV4`. Evidence:
     the 126 valid, 81 accepted-held and 82 reject report fixtures through
     `decode_report_v4(_for_host)` and the 114 valid and 26 reply fixtures
     (`report_v4_decode.rs`, `reply_v4_decode.rs`). Legacy functions untouched.
  2. v4 staging intake: `StagedReport` from `WorldReportV4` (no views,
     `PresentationRowV4`, `renderer_input`, native and attempt held rows).
     Evidence: scenarios `row-hold-close-recovery`, `scale-identity-primary`,
     `native-domain` and held-row census tests.
   3. Capability-conflict grading on v4 for the four capability-context fixtures
     (`invalid-unavailable-fact-contradiction`,
     `invalid-executable-unavailable-value`, `invalid-sticky-projected-fact`,
     `invalid-sticky-native-free-attempt`): the decoder pins them as valid typed
      rows with zero held rows; the engine grades conflicting windows to held
      rows and diagnoses unsupported state attempts as `conflicting-fact`,
      without consuming the attempts as settlement.
     Evidence: engine tests per fixture; scenarios `raw-fact-availability`,
     `executable-identity`, `native-domain`, `window-state`,
     `raw-attempt-settlement`.
   4. Shared observation boundary: move raw report facts used by lane helpers
      onto `StagedReport`, populate them from both intakes, and make the common
      lane helpers consume staged observation rather than `WorldReport`.
      Includes inventory completeness, raw windows (distinct from classified
      facts and own-process exclusions), explicit closes, native rows, ordered
      valid/held screens, cursor/focus/action/primary facts, gesture/clock,
      all attempt and feedback lanes, intent records and observation watermarks.
      Owns shared observation-dependent command helpers and lifetime pruning,
      their explicit envelope/correlation arguments and legacy caller adaptation,
      not envelope admission or command dispatch. Preserve intent ingestion
      before classification by separating raw capture from classification;
      preserve pre-observation routing by capturing raw facts without changing
      retained homes/views. Evidence: legacy parity and focused lane regressions
       for ordering, holds, lifecycle, first-raw settlement and intent admission.
       No generic-over-report engine or conversion to the legacy report shape.
       Split into 4a raw observation capture and staged intent admission, 4b
       shared lane/lifetime consumers, then 4c observation-dependent command
       helpers and explicit correlation arguments. The first piece separates
       capture from classification in both intakes, carries the current raw
       observation on `StagedReport`, and makes initial intent ingestion consume
       that capture before classification, never during command restaging.
        Other consumers retain their existing sources until their adaptation;
        no settlement migration is claimed by 4a.
        Split 4b at its lifetime boundary: 4b-1 moves retained-authority lifetime
        pruning to staged observations; 4b-2 moves the remaining observation
        lanes. Native/view proof and command dependencies stay with their
        existing owners rather than being pulled into the lifetime increment.
        Split 4b-2 into 4b-2a state, ordinary-focus and frame settlement, then
        the remaining selection/native/gesture/cursor/clear/born/topology
        consumers. Preserve the view-proof, command-helper and run/finish
        ownership boundaries below; this split does not change migration order.
        Next, 4b-2b migrates selection feedback/native-selector receipt capture
        and its arrival receipt watermark. Switch completion stays coupled to
        native/view proof; the other remaining lanes follow separately.
        Then 4b-2c migrates the complete cursor focus/warp family, cursor
        history sampling and the gesture fullscreen-pause observation gate.
         Gesture actuation/correlation and pre-observation fresh-focus routing
         remain with their existing command and assembly dependencies.
         Next, 4b-2d migrates retained clear/born, transfer/migration and float
         settlement's raw receipt and readability inputs. Native/view proof,
          born authority/presentation and command continuations retain their
          existing owners; other observation consumers follow separately.
         Then, 4b-2e migrates gesture observation gates, born authority/lifetime
         raw gates, live topology retention and switch receipt watermark/close
         inputs. These remain independent raw consumers even where the same
         function retains a report for command, proof or presentation work.
          Logical mapping convergence and domain-staging raw inputs remain a
          separate raw-consumer piece; 4b-2 does not close merely because its
          other dependencies have owners.
          Finally, 4b-2f migrates logical convergence/domain staging and the
          remaining independent membership, decorator and frame-publication raw
          gates. Close 4b-2 only after auditing remaining report reads against
          their actual command, proof, presentation or assembly dependencies.
   5. View-proof migration replacing the `views` echo legs (`view_echo_for`,
     staged echo fill, `settle_views_pre`, selection proof) with visibility and
     focus proof. The contract settles the receipt predicates (removed `views`
     text, arrival requirements, frozen switch target sets); only the observe
     staging function is implementation detail.
   6. Genuine presentation producer plus `rendererInput` validation. The contract
     settles the anchored descriptor shape, core-scaled style, unknown witness
     disabling the border, and a stale target refusing the command with
     `stale-render-target` at command handling (not a publish hold). Scenarios
     `anchored-border`, `decorations-target-row`, `arrival-mismatch-*`. The
     native witness-join proof stays an adapter-port gate.
   7. `WorldReplyV4` publish assembly (transfers with an explicit command
     sequence, diagnostics, switch events) plus v4 bootstrap. The contract
     settles the transfer command sequence as the original command sequence,
      null for commandless work. Scenarios `raw-attempt-settlement`,
      `visibility-*`, `window-state`, `persistent-above`,
      `linked-selectors-warp`.
      Also owns the internal v4 accepted-run/finish assembly, command dispatch,
      pre-observation send/migration freezes, ingest-then-classify sequencing,
      command rollback, prior-observation retention replacing lane reads of
      `last_report`, v4 correlation context for shared command helpers, and
      v4 reply/replay bookkeeping and bounds. Current-observation facts belong
      to sub-increment 4; view proofs and presentation belong to 5 and 6.
      Envelope admission and the public `handle_report` switch remain unit 7,
      alongside broker/service/handshake cutover. Internal assembly tests do
      not establish public cutover acceptance.
- Sub-increment 1 outcome: staged pure-function twins in two new core modules,
  `world_report_v4_checks` (`validate_report_v4_reason`,
  `grade_conflicting_windows_v4`, `report_digest_v4`) and `world_sizing_v4`
  (`report_estimate_v4`, `reply_estimate_v4`, `reply_exact_bytes_v4`,
  `reply_upper_bound_v4`), plus a core `WORLD_CONTRACT_VERSION_V4` constant that
  the protocol's staged version constant now points at (the runtime version
  stays 3). Nothing calls them yet. Evidence: over the 207 accepted report
  fixtures (126 valid, 81 accepted-held) validation passes, the digest is stable
  and sensitive to one field per lane family, the estimate bounds the canonical
  encoded bytes, and permissive-capability grading is a no-op; three of the four
  capability-context fixtures grade to exactly one `conflicting-fact` window hold
  (the fourth is an attempt row owned by sub-increment 3); the exact reply size
  equals the canonical encoded length over every valid reply fixture, and the
  reply estimate bounds it, including extreme transfer command sequences and a
  maximal anchored border. Legacy edits are visibility-only. Not built: twins of
  the invalid-screen and invalid-window graders (the decoder already holds those
  rows; only an in-process typed value that bypasses the codec would reach them),
  and a core canonical reply encoder (the protocol writer is the encoder; the
  exact-size function is pinned against it).
- Sub-increment 2 plan (v4 staging intake): new `world_stage_v4` module with
  `WorldEngine::stage_facts_v4(&WorldReportV4)` filling the existing
  `StagedReport`; the per-window classification loop and held-window scope
  resolution move out of `stage_facts` into shared helpers so the legacy and v4
  intake cannot drift (legacy behavior unchanged, proven by legacy-versus-v4
  staged-equality tests over a report without views). New `StagedReport` fields,
  empty for the legacy intake: `held_native`, `held_attempts`, `presentation_v4`
  (valid or held singleton, never defaulted) and `renderer_input`. No echoes are
  staged from v4. Typed diagnostics are staged in report lane order (screen,
  window, native, attempt, presentation) for every held row that has a scope.
  Not in this increment: the `state_offroute` diagnostic (sub-increment 3),
  consumption of the new fields by any lane (native converge, attempt
  settlement, presentation and renderer validation belong to later
  sub-increments), views replacement, and any entry point from
  `handle_report`. A read-only `#[doc(hidden)]` census over a cloned engine lets
  the protocol tests stage every accepted fixture report and the
  `row-hold-close-recovery`, `scale-identity-primary` and `native-domain`
  scenario reports without running them.
- Sub-increment 2 outcome: the parallel intake now builds `StagedReport`
  directly from `WorldReportV4`, retaining the new lanes verbatim and sharing
  classification and held-window scope resolution with the unchanged legacy
  route. Four core regressions pin shared-field parity, legacy defaults, new-lane
  retention and diagnostic order. Two protocol regressions conserve all 207
  accepted fixtures and independently stage 6, 4 and 8 reports from
  `row-hold-close-recovery`, `scale-identity-primary` and `native-domain`;
  the first scenario's invalid-row-identity report refuses decoding and is
  counted separately. These are staging checks, not scenario execution or
  settlement evidence. Review found no blocking issue; the census now states
  its fresh-engine scope invariant and pins exact scenario report counts.
  Direct verification passes the filtered core/protocol gate (1636 core unit
  tests and every selected integration/protocol target), core/protocol build,
  scoped all-target clippy with warnings denied, formatting, whitespace, added
  ASCII, NUL and local-link checks. Only `workspace_unsafe_posture` is filtered.
  No `state_offroute`, new-lane consumption, views replacement, `handle_report`
  wiring, version, adapter or immutable fixture changes are included.
- Integration ownership is assigned above: sub-increment 4 owns the shared raw
  observation boundary and lane adaptation; sub-increment 7 owns the v4 run and
  finish assembly and its envelope/command context. This is internal planning,
  not a change to unit 7 public cutover order. Source checks of `run_accepted`,
  `finish`, state readability, selection retention and held-screen ordering show
  that staged facts alone are insufficient: raw lifetime presence and observation
  watermarks must survive alongside classification, and intent ingestion and
  pre-observation routing must keep their existing order.
- Parked typed-ingress question: the invalid-screen/window grader twins omitted
  from sub-increment 1 still need an explicit refuse-versus-hold wiring decision
  for in-process typed values that bypass the codec. Unit 7 public cutover,
  which wires `handle_report`, owns that decision; staging and internal accepted-
  run assembly do not decide that validation boundary.
- Sub-increment 3 plan: apply the existing v4 conflicting-window grader using
  the engine's bound capabilities before v4 staging; append unsupported state
  attempts' `conflicting-fact` diagnostics in the attempt lane, resolving scope
  through carried windows/held rows and retained homes exactly as the legacy
  `state_offroute` path does. Preserve raw attempts, never treat unsupported
  attempts as settlement. Use `state_property_supported` for both attempt
  support and sticky/maximize grading/classification defaults: absent capability
  binding means no native state support, matching `HostCapabilities::default`.
  Bound behavior is unchanged. Evidence: the four immutable capability-context
  fixtures with their declared handshakes, supported controls, scoped/unscoped
  and retained-home cases, unbound-default regressions and staging coverage of
  `raw-fact-availability`, `executable-identity`, `native-domain`, `window-state`
  and `raw-attempt-settlement`. No runtime/version switch or full scenario run.
- Sub-increment 3 outcome: v4 staging now applies connection-bound window
  grading and emits unsupported state-attempt diagnostics through the same
  scope helper as the legacy intake. Sticky/maximize grading and classification
  use the existing fail-closed state-property support predicate for absent
  capabilities; declared bound-capability behavior is unchanged. Raw attempts
  remain intact and never become manufactured held-attempt rows. Unsupported
  attempt non-consumption is pinned through the existing settlement helper;
  there is still no v4 accepted-run/settlement assembly.
  All four immutable context fixtures are decoded with their manifest-declared
  handshake capabilities. The fourth emits an Attempt diagnostic plus an
  ancillary Window hold: its `maximized:false` conflicts with `maximize=false`,
  which requires null under the composed-maximize contract. An in-memory
  maximize-null counterfactual isolates the intended sticky-attempt conflict;
  the fixture is unchanged. Permissive controls remain unheld and silent.
  The five context scenarios independently stage 4/7/8/19/10 reports under
  their declared handshake segments; these are capability-context checks,
  not sequential retained-state progression or scenario execution. The
  207-fixture census uses explicit support for all four native state properties.
  Review found two test-isolation weaknesses, corrected with single-cause
  conflict cases and a direct unbound classification test. A mutation restoring
  the old permissive classification defaults makes that regression fail;
  restoring the intended defaults passes. New regressions were first run after
  implementation, not established as preimplementation behavioral reds.
  Direct final verification passes the filtered core/protocol gate (1650 core
  unit tests and every selected integration/protocol target, including seven
  protocol staging tests), build, scoped all-target clippy with warnings denied,
  formatting, whitespace, added ASCII, NUL and local-link checks. Only
  `workspace_unsafe_posture` is filtered. No fixture, manifest, adapter, Cargo
  feature, public ingress, version or contract changes are included.
- Sub-increment 4a plan: capture inventory completeness, raw windows (including
  own-process and safety-excluded subjects), explicit closes, native rows,
  ordered valid/held screens, cursor/focus/action/primary facts, gesture/clock,
  attempt/feedback lanes, intent records and the observation watermark before
  classification. Keep envelope admission, request/command correlation and
  dispatch outside the observation record. Resolve held-window scopes and
  classify only after intent ingestion, without changing retained routing state
  during capture or replacing prior staged homing early. Both intakes populate
  the same internal boundary directly; v4-only rows remain verbatim and v4
  echoes remain empty. Verify raw-lane conservation and ordering, raw versus
  classified presence, capture purity, same-report intent classification and
  held/close/late-import lifecycle behavior. Existing settlement and routing
  regressions remain the parity gate; full v4 run assembly remains unbuilt.
- Sub-increment 4a outcome: `StagedReport.observation` now carries the shared
  raw windows, closes, inventory completeness, native rows, cursor/focus/action/
  primary facts, gesture/clock, attempt/feedback lanes, intent records and
  observation watermark. Existing sibling fields retain ordered screens,
  held rows, gesture barrier and v4 presentation/input; these are not duplicated
  inside the raw capture. V4 captures original typed windows before capability
  grading, separately from effective classification and own-process exclusions.
  Legacy accepted-run intake captures once, ingests staged intent once, then
  classifies; bootstrap intent also reads a capture. Classification-only
  restaging and the v4 census never ingest. V4 initial ingest/classify ordering
  is exercised by internal tests, not a new accepted-run entry point.
  Nine new regressions cover lane conservation/order, held ordinals/diagnostics,
  raw/classified separation, capture purity, prior held homes, same-report intent
  classification and hold/close/late-import fences. Review exposed re-ingestion
  during command restaging in the initial implementation; a regression reproduced
  pending intent resurrection after simulated command mutation, then passed after
  separating initial ingestion from classification-only restaging. Earlier
  failures corrected invalid synthetic report setup and held-scope assertions,
  not product behavior or fixture expectations. Follow-up review found no
  blocking issue. Shared lane/lifetime readers and observation-dependent command
  helpers remain 4b/4c; no full v4 settlement or scenario execution is claimed.
  Direct final verification passes the filtered core/protocol gate (1659 core
  unit tests and every selected integration/protocol target), build, scoped
  all-target clippy with warnings denied, formatting, whitespace, added ASCII,
  NUL and local-link checks. Only `workspace_unsafe_posture` is filtered.
  No fixture, manifest, adapter, feature, public ingress, version or contract
  changes are included. A separately discovered grading risk is parked below;
  these checks do not establish its acceptance or full v4 engine acceptance.
- Sub-increment 4b-1 plan: move retained state/focus/automatic-focus/selection
  lifetime pruning and the lifetime-ledger block currently in `finish` onto
  staged raw inventory completeness, window presence, explicit closes and held
  rows. Selection output-loss fences use staged valid/held screens. Extract the
  finish pruning block into a shared internal helper without changing its
  position relative to focus-output retention, membership reconciliation or
  publication. Preserve each ledger's existing live/held/excluded/engine-known
  predicate, including deliberate differences between canonical memory and
  pending intent. Explicit closes win even with contradictory carried presence;
  incomplete inventories and held screens/windows never imply departure.
  Regressions exercise both intakes, raw versus classified presence, holds,
  exclusions, closes, complete loss and incomplete gaps. No settlement, native/
  view proof, presentation, command helper or envelope/replay retention migration
  is included; these remain 4b-2, 4c and their existing dependent increments.
- Sub-increment 4b-1 outcome: retained state, ordinary focus, automatic-focus
  identity, finite selection grants, output scopes and visibility-target lifetime
  pruners now consume staged observation/held-screen fields without a report
  argument. `prune_lifetime_state` extracts the former finish-time ledger block
  and runs at the same position, preserving canonical, exclusion, pending intent,
  rollback-save and composed-maximize predicate differences and lane-pruner order.
  Raw lifetime presence remains distinct from classified eligibility; no complete
  observation snapshot is cloned by the pruners. Twelve new regressions cover
  both intakes, holds, incomplete gaps, closes, current capture, own-process raw
  facts, member-known retention, canonical/intent differences, true output loss
  and capability-graded presence without authority. The automatic-focus exclusion
  test now stages an absent-subject probe without member knowledge, then proves
  exclusion retention and removal separately. Review corrected test isolation,
  documentation and two initial test-oracle assumptions; no product-behavior
  change or preimplementation behavioral red is claimed. Independent review found
  no blocking finding. Final direct verification passes the filtered
  core/protocol gate (1676 core unit tests and every selected integration/protocol
  target), build, scoped all-target clippy with warnings denied, formatting,
  whitespace, added ASCII, NUL and local-link checks. Only
  `workspace_unsafe_posture` is filtered. No new tentative decision, contract,
  fixture, manifest, adapter, feature, public ingress or version change is
  included. V4-driven pruning tests do not establish accepted-run or scenario
  execution; remaining lane consumers stay 4b-2, with command adaptation in 4c.
- Sub-increment 4b-2a plan: move native-state attempt settlement, ordinary-focus
  feedback settlement and mechanical frame settlement onto staged observations.
  Keep their accepted-run positions and post-rollback staged readability joins
  unchanged. State attempts retain first exact-tuple evidence before holds;
  focus feedback retains its first historical verdict independently of current
  readability. Frame counter-evidence still precedes watermark consumption and
  learning, and burst timing uses the captured clock. Use only narrow lane/scalar
  extraction where mutation requires ownership, never a full observation clone.
  Verify both intakes, current versus prior capture, held/incomplete/barrier
  behavior, unsupported state attempts, contradictory redelivery, frame replay,
  learned-floor ordering, burst clock and explicit-close memory retirement.
  No selection/native view proof, presentation, command helper, envelope,
  prior-report retention or v4 accepted-run/scenario assembly is included.
- Sub-increment 4b-2a outcome: state, ordinary-focus and frame settlement now
  consume the staged raw attempt/feedback lanes, completeness, clock and closes,
  with staged gesture barriers and unchanged classified-fact/domain joins.
  Their legacy accepted-run call positions are unchanged. State/focus loops
  borrow their captured lanes; frame mutation clones only its attempt lane once,
  never the full observation. First evidence, frame counter-evidence ordering,
  replay fences and the existing fact-or-member frame-memory pruning predicate
  remain unchanged. Five new dual-intake regressions cover incomplete/barrier/
  held state receipts and readable recovery, same-tuple contradictions, focus
  receipts under holds, frame replay, learned-floor veto/clear/relearn, burst
  timing and explicit-close retirement of all four frame maps. The unsupported
  state-attempt test now drives the actual v4 intake and proves non-consumption.
  Test-only homing/domain seeds isolate settlement; these checks establish no
  v4 accepted-run assembly, native ordering or scenario execution. Review found
  no blocking source issue; follow-up corrected two gate-discrimination oracles
  and a clone-on-Copy lint without production changes. New tests were first run
  after implementation, not established as preimplementation behavioral reds.
  Final direct verification passes the filtered core/protocol gate (1681 core
  unit tests and every selected integration/protocol target), build, scoped
  all-target clippy with warnings denied, formatting, whitespace, added ASCII,
  NUL and local-link checks. Only `workspace_unsafe_posture` is filtered.
  No new tentative decision, contract, fixture, manifest, adapter, feature,
  public ingress or version change is included. Remaining observation lanes
  stay in 4b-2, command helpers in 4c and dependent proof/assembly work with
  its existing owners. The phase note remains active.
- Sub-increment 4b-2b plan: make `settle_selection_feedback` consume staged
  selector attempts, focus feedback and the observation receipt watermark.
  Preserve its pre-command-rollback position, first-exact-tuple latching per
  obligated output, optional-focus behavior and frozen-arrival proof inputs.
  Keep native/home translation, current staged homing and arrival predicates
  unchanged; no new native selector or visibility authority follows from receipt
  capture. Verify both intakes, current capture, wrong tuples, contradictory
  redelivery, held/incomplete/barrier receipt retention, focusless native receipts
  and arrival watermark bounds. `settle_selection_switch`, its fresh-report
  native proof fallback and LastWrite accounting remain unchanged with their
  proof owner; no presentation, command helper, prior-report retention, v4
  accepted-run/scenario assembly, fixture or public ingress change is included.
- Sub-increment 4b-2b outcome: selection feedback settlement now borrows staged
  selector attempts and focus feedback, using the captured observation watermark
  for the unchanged frozen-arrival proof. The legacy caller keeps its position;
  tuple predicates, first-receipt guards, homing/native-map joins and optional
  focus behavior are unchanged, with no new observation clones. Two dual-intake
  regressions cover wrong tuples, receipts under held/incomplete/barrier inputs,
  same-lane focus ordering, contradictory redelivery, focusless per-output view
  capture, native-map translation and future-dated arrival bounds. A later
  covering watermark never re-grades the historical receipt; receipt capture
  alone emits no switch terminal. Direct binding/homing seeds establish helper
  behavior, not v4 accepted-run, codec-shape or scenario acceptance. Review found
  no source blocker; test follow-up corrected selector duplicate-operation shapes
  and strengthened watermark and tuple controls. New tests were first run after
  implementation, not established as preimplementation behavioral reds.
  Final direct verification passes the filtered core/protocol gate (1683 core
  unit tests and every selected integration/protocol target), build, scoped
  all-target clippy with warnings denied, formatting, whitespace, added ASCII,
  NUL and local-link checks. Only `workspace_unsafe_posture` is filtered. No new
  tentative decision, contract, fixture, manifest, adapter, feature, public
  ingress or version change is included. Switch completion/native proof and
  the remaining observation lanes retain their existing owners; the phase note
  remains active.
- Sub-increment 4b-2c plan: move focus-follows-cursor eligibility/effect
  production, cursor warp and their fullscreen-game pause gate onto staged
  cursor, hit, focus, completeness, barrier and screen inputs. Sample retained
  cursor history from the same staged cursor at the existing accepted-run
  position. Move the gesture pause gate's focused-window lookup to the same
  boundary, preserving its distinct self-exclusion (a fullscreen gesture
  subject does not pause itself). Keep effect ordering/identity, static-hover
  suppression, retained-view membership, final-effect warp selection and
  work-area clamping unchanged. Verify both intakes, fresh capture, raw held
  hits without classified eligibility, incomplete/barrier/game/held gates,
  static-cursor and hover transitions, cross-output final-focus warp and
  destination clamping. Existing command/rejected/consumed and gesture update/
  release regressions remain parity evidence. No gesture command/correlation
  migration, fresh-focus routing, native/view proof, presentation descriptor,
  prior-report retention or v4 accepted-run/scenario assembly is included.
- Sub-increment 4b-2c outcome: the complete cursor focus/warp family now consumes
  staged cursor, hit, focus, completeness, barrier and ordered screens; accepted-
  run cursor history samples the same capture at its unchanged position. The
  gesture pause predicate reads staged focus while retaining its distinct
  same-subject exemption. Five hover callers and the warp publication caller
  adapt in place; eligibility, effect ordering/identity and warp geometry remain
  unchanged, with no new observation clones. Two dual-intake regressions cover
  movement-only versus transition-only coupling, static/arrived hover, holds,
  pause recovery, final-effect warp selection, work-area clamping and missing/
  off-screen cursors. A v4 capability-hold test compares an eligible supported
  control with the identical sticky hit under unsupported capability; raw facts
  remain preserved without classified eligibility. A nested legacy-intake test
  pins the cursor/gesture self-exemption difference. Test-only retained-view/
  domain seeds isolate helper behavior, not v4 accepted-run or scenario assembly.
  Review found no source blocker; follow-up corrected initially vacuous hold
  coverage and separated movement from hover-transition controls. New tests
  were first run after implementation, not established as preimplementation
  behavioral reds. Final direct verification passes the filtered core/protocol
  gate (1687 core unit tests and every selected integration/protocol target),
  build, scoped all-target clippy with warnings denied, formatting, whitespace,
  added ASCII, NUL and local-link checks. Only `workspace_unsafe_posture` is
  filtered. No new tentative decision, contract, fixture, manifest, adapter,
  feature, public ingress or version change is included. Remaining gesture,
  clear/born, topology/native and switch-proof dependencies keep their existing
  owners; the phase note remains active.
- Sub-increment 4b-2d plan: move exact-correlated clear, member and view attempt
  lookup plus retained float frame evidence to staged observation. Move the
  same settlement paths' inventory, barrier, current-screen and clear-focus
  reads to staged inputs, preserving receipt-before-hold order, tuple matching,
  replacement/latching policy, replay/DPI fences and close precedence. Remove
  the report argument from born-clear and float settlement where no dependency
  remains; other settlement callers keep report arguments for proof or command
  dependencies. Verify both intakes with current-versus-stale capture controls,
  forged tuples, raw evidence under gaps/holds, recovery and lifecycle/replay
  controls. Tests seed retained authority only to isolate shared helpers; no v4
  accepted-run/scenario assembly is claimed. Native/view proof, born authority
  and presentation, command continuation/correlation, prior-report retention,
  fixture/manifest and public ingress remain unchanged.
- Sub-increment 4b-2d outcome: clear/born, window/follow and migration/refill
  receipt lookups now borrow staged clear/member/view lanes; float settlement
  borrows staged frames. Their inventory/barrier, current-screen and clear-focus
  inputs use the same staged boundary. Born-clear and float settlement no longer
  take a report; remaining report arguments retain native/view proof and command
  dependencies. Tuple predicates, replacement order, receipt-before-hold order,
  replay/DPI fences and close precedence are unchanged, with no new observation
  clones. Ten new dual-intake tests cover exact and forged tuples, contradictory
  passed-report inputs, incomplete/barrier/held receipt retention and readable
  recovery, host-owned view refusal under current-screen gaps, clear focus and
  float-continuation geometry, migration parent/refill receipt capture and member
  replacement, float replay, DPI transitions and explicit close. Migration
  completion/recovery is proven only on the legacy intake: v4 has no view echoes
  yet and retains the correlated receipts without completing. Test-only retained
  authority/homing/view/domain seeds isolate helper behavior, not v4 accepted-run,
  codec-shape or scenario acceptance. Review found no source blocker; follow-up
  added missing branch controls, isolated gate effects from independent holds and
  corrected a migration post-output test tuple. New tests were first run after
  implementation, not established as preimplementation behavioral reds. Final
  direct verification passes the filtered core/protocol gate (1697 core unit
  tests and every selected integration/protocol target), build, scoped all-target
  clippy with warnings denied, formatting, whitespace, added ASCII, NUL and
  local-link checks. Only `workspace_unsafe_posture` is filtered. No new tentative
  decision, contract, fixture, manifest, adapter, feature, public ingress or version
  change is included. 4b-2 is not finished: remaining gesture, born authority,
  topology/native and switch-proof dependencies retain their existing owners;
  the phase note remains active.
- Sub-increment 4b-2e plan: use staged gesture ingress, observation watermark,
  inventory and explicit closes for gesture binding/actuation gates; keep
  command construction, request correlation, homing proof and domain restaging
  unchanged. Use staged inventory/barrier for born authorization and lifetime
  pruning while retaining presentation pause and native proof dependencies.
  Retain live topology from current staged screens with its existing completeness
  predicate. Switch visibility-write receipt grading and close fences use staged
  watermark/closes; native selector proof remains unchanged. Verify both intakes
  with contradictory passed-report controls, replay/start watermark, close versus
  gap/hold, born publish/pruning gates, topology replacement/fencing and first
  switch-write receipt bounds. No v4 accepted-run, proof/presentation migration,
  command-helper correlation migration, prior-report retention or public ingress
  change is included. Remaining logical pre/post convergence and domain-staging
  raw inputs still belong to 4b-2; gesture command helpers belong to 4c,
  native/view proof to 5, presentation to 6, and accepted-run/envelope/prior-report
  assembly to 7.
- Sub-increment 4b-2e outcome: gesture ingress, bind/start receipt watermarks,
  inventory and explicit-close gates now consume staged observations. Born
  authorization/lifetime pruning use staged inventory/barrier; topology retention
  uses staged screens; switch visibility-write receipt grading and close fences
  use staged watermark/closes. Predicates, ordering, retained identity and
  first-receipt latching remain unchanged, with only narrow gesture ingress
  ownership and no observation/lane clones. Command correlation/construction,
  native/view proof, presentation pause and domain/homing restaging retain their
  report dependencies. Eight new dual-intake tests cover contradictory passed
  reports, bind/supersession, same-phase watermark hold versus preview, continuing
  inventory hold versus update preview/release commit, explicit close, born
  touching-migration and lifetime gates, first switch-write receipt bounds and
  topology replacement/fencing. Gesture tests bootstrap Engine sessions through
  the legacy accepted path and use the legacy twin for test-only domain joins;
  v4 legs prove shared consumers after real intake, not v4 accepted-run/domain
  assembly, codec-shape or scenario acceptance. Review found no source blocker;
  follow-up replaced post-capture test mutation and conflated phase/sequence
  controls with source-report staging and eligible paired controls. New tests
  were first run after implementation, not established as preimplementation
  behavioral reds. Final direct verification passes the filtered core/protocol
  gate (1705 core unit tests and every selected integration/protocol target),
  build, scoped all-target clippy with warnings denied, formatting, whitespace,
  added ASCII, NUL and local-link checks. Only `workspace_unsafe_posture` is
  filtered. No new tentative decision, contract, fixture, manifest, adapter,
  feature, public ingress or version change is included. 4b-2 remains open:
  `world_model` logical pre/post convergence and ordered live-output inputs,
  plus `world_observe::build_staged_domains` raw focus/inventory reads, remain
  raw consumers separate from their native/view proof dependencies. 4c is not
  started; the phase note remains active.
- Sub-increment 4b-2f plan: move ordered valid/held output discovery, logical
  pre/post convergence, admission focus and domain-staging focus/readability onto
  staged observations. Include trailing-output marking, member pruning, above
  suspension, selection/sticky/fullscreen/maximize decorator gates, frame
  publication clocks/gates, current accepted-run inventory/focus scalars and the
  current-focus output-memory latch before membership reconciliation.
  Preserve ordering, primary selection, native-order presence, hold precedence,
  focus spending, frame suppression/burst timing and prior-focus comparison.
  Remove internal report arguments only where no dependent work remains.
  Verify both intakes with eligible positive controls and contradictory passed
  reports, then audit all remaining raw reads before deciding 4b-2 closure.
  Native/view proof stays in 5, command/pre-observation routing in 4c/7,
  presentation in 6 and envelope/bootstrap/prior-report retention in 7. No full
  v4 runtime/domain assembly, contract, fixture, version or adapter change is
  included; the phase note remains active.
- Sub-increment 4b-2f outcome: logical pre/post convergence, ordered valid/held
  output discovery, trailing marking, admission focus, domain focus/readability,
  member pruning and the remaining decorator/publication gates now use staged
  observation inputs. The current accepted-run migration-gap/focus checks and
  pre-reconcile focused-output latch use that same capture. Report-free internal
  helpers no longer take a report; proof/command/presentation dependencies keep
  theirs. Ordering, designated/first-valid primary selection, native-order
  presence, hold precedence, at-most-once focus spending and frame timing are
  unchanged. Convergence derives only narrow output-id/completeness/presence
  sets, not copies of screen/native lanes or the whole observation.
  Eighteen new dual-intake tests cover ordering/primary fallback, displacement
  and focused survivor views, native-order parity gates, keyed domain
  readability/focus, admission spending, member presence/pruning, all decorator
  families, above focus-only suspension, native-sticky continuation, output
  planning and focused-output memory. Frame clocks are exercised through real
  publication callers for tiled, float-token and composed-overlay suppression
  plus burst hold/rearm. Raw inputs are set before intake; retained seeds and
  legacy domain/proof joins isolate consumers, not v4 accepted-run/domain or
  scenario assembly. Review corrected lane-copying and weak test controls;
  follow-up found no blocking issue. Initial test failures corrected synthetic
  setup/oracle assumptions, not production policy; no preimplementation
  behavioral red is claimed. Final direct verification passes the filtered
  core/protocol gate (1723 core unit tests and every selected integration/protocol
  target), build, scoped all-target clippy with warnings denied, formatting,
  whitespace, added ASCII, NUL and local-link checks. Only
  `workspace_unsafe_posture` is filtered. No new tentative decision, contract,
  fixture, manifest, adapter, feature, public ingress or version change is
  included. The remaining-read audit closes 4b-2: no independent raw lane remains
  outside the explicit dependent owners below. 4c is next and unstarted; the
  overarching phase note remains active.

| Remaining report dependency after 4b-2 | Existing owner |
| --- | --- |
| Command inputs, issuing correlation and observation-dependent command helpers | 4c |
| Fresh cursor/focused/primary routing shared with pre-capture send/migration freezes and command selection; fresh-placement bootstrap context | 4c/7; do not substitute current staged state before capture |
| Native/view receipts, echoes, ordering and occupancy proof; native creation receipt-clock bounds and provisioning mint clocks/authorization | 5 |
| Presentation ingress/pause, focused presentation and genuine descriptor production | 6 |
| Envelope admission, pre-ready/bootstrap, replay/reply assembly, prior-report/last-view retention and v4 run/finish wiring | 7 (native last-view proof remains 5) |
| Raw capture/classification intake, validation, digest, codec and sizing | Source-report operations, not stranded lane consumers |

- Sub-increment 4c plan: migrate observation-dependent command helpers together
  with their issuing correlation arguments. Current command gates and geometry
  read staged inventory, focus and screens; event construction takes explicit
  typed owner/correlation and command sequence instead of parsing a report.
  Routing helpers take an explicit borrowed raw context so pre-observation send,
  migration and fresh-focus decisions continue consulting prior retained homes
  and views, while post-capture commands consult current observations. Preserve
  freeze positions, fallback order, request identities, command continuation and
  classification-only restaging; no full observation clone per helper or intent
  re-ingestion. Remove unused report arguments and make selection sizing take the
  command input explicitly. Verify dual-intake helper parity, eligible positive
  and held/incomplete controls, staged-versus-stale inputs, typed event identity,
  pre-sweep source freezes and fresh-placement retention. Audit remaining report
  reads against the owner table above. Native/view proof, presentation, prior
  report/bootstrap and v4 run/finish assembly remain with 5, 6 and 7; helper tests
  establish neither v4 accepted-run nor scenario acceptance. No contract, fixture,
  manifest, migration-order, adapter, feature or version change is included.
- Sub-increment 4c outcome: command gates, float-move geometry, view/follow
  fanout and maximize eligibility now read staged screens, focus, inventory,
  barrier and observation watermark. Shared routing leaves take borrowed
  `RawRouting`; pre-capture send/migration freezes and fresh-placement callers
  retain explicit source context and prior homes/views, while command selection
  uses current staged context. `EventIssue` carries the already validated typed
  owner/correlation through sends, migration, local/directional operations,
  gestures, sticky transitions and clear continuations. Original command
  sequences remain separate explicit inputs, including retained continuations.
  Selection reserve sizing takes the command directly; unused report arguments
  are removed. No per-helper observation clone or intent re-ingestion is added.
  Remaining report parameters carry native/view proof, classification/domain
  restaging or run/command-supersession assembly, not independent raw command
  reads; their v4 wiring remains with 5/7. Presentation stays with 6.
  Thirteen new regressions cover both intakes for helper behavior, staged versus
  contradictory inputs, actual event authorization with explicit identity,
  maximize focus/held/inventory/barrier gates, float geometry, Shared view fanout,
  GlobalUnique swaps, eligible migration versus incomplete inventory, source
  routing and fresh-focus precedence. Existing accepted-run regressions pin both
  pre-observation send-home and migration-view freezes. Direct-helper callers
  now stage their source probes before restoring domain joins. Shared follow-send
  fanout has source/full-suite parity evidence, not a new isolated regression.
  Review corrected five sticky owner reads and weak test controls; follow-up
  found no blocking issue. Envelope rejection gates are unchanged, so removing
  unreachable helper parse fallbacks changes no public refusal behavior. Initial
  failures corrected caller arity, synthetic eligibility and test scope, not
  product policy; no preimplementation behavioral red is claimed. A planner-noop
  move probe was replaced by an eligible orientation event, and a redundant
  trailing-send probe was replaced by the existing exact freeze regressions.
  Final direct verification passes the filtered core/protocol gate (1736 core
  unit tests and every selected integration/protocol target), core/protocol
  build, scoped all-target clippy with warnings denied, formatting, whitespace,
  added ASCII, NUL and local-link checks. The first clippy pass required three
  existing-style argument-count annotations; formatting required one import
  reflow. Only `workspace_unsafe_posture` is filtered. No new tentative or parked
  item, contract, fixture, manifest, feature, adapter, public ingress, version or
  migration-order change is included. 4c and the shared observation boundary are
  finished; view-proof migration (5) is next. These helper tests establish no v4
  accepted-run, native delivery, runtime or scenario acceptance; the phase note
  remains active and all eight OPEN/TBD gates remain unchanged.

- Presentation-local style scaling only; keyboard-step, resize-minimum and
- Capability-held cursor grading repair plan: reproduce a valid v4 report whose
  native cursor hit names a newly capability-held window, then keep that window
  held instead of discarding the grading. Preserve the original hit and fact in
  raw observation capture; the effective classified report must give that held
  subject no cursor-target eligibility. Keep wire admission and its existing
  cursor cross-checks unchanged, including unrelated invalid-input refusal.
  Verify direct grading and staging, supported/unrelated-hit controls, retained
  raw facts and existing holds/ordinals/diagnostics. This repair precedes 4b and
  does not alter the migration order or add v4 runtime assembly.
- Capability-held cursor grading repair outcome: the focused staging regression
  first failed because a valid native hit on the conflicting sticky subject
  caused grading to return no normalized report, leaving that typed fact staged.
  Grading now clears only an effective hit naming a newly graded hold, with an
  original cursor present, before the unchanged final structural recheck. Raw
  observation capture and the original report retain the cursor, hit and facts;
  no replacement target is selected. Unknown hits, hits without cursor and an
  unrelated malformed command pair still yield no graded report, preserving the
  existing invalid-typed-input handling without deciding public ingress policy.
  The reproducer now passes. Four additional focused regressions cover direct
  normalization, supported and unaffected-hit controls, malformed originals and
  existing held-row/ordinal conservation. Independent review found no blocking
  issue. Final direct verification passes the filtered core/protocol gate (1664
  core unit tests and every selected integration/protocol target), build, scoped
  all-target clippy with warnings denied, formatting, whitespace, added ASCII,
  NUL and local-link checks. Only `workspace_unsafe_posture` is filtered. The
  effective-hit normalization choice remains tentative below; the earlier
  grading risk is repaired, not an outstanding blocker. No contract, fixture,
  manifest, adapter, feature, version or public-ingress change is included, and
  4b remains unstarted.
- Presentation-local style scaling only; keyboard-step, resize-minimum and
  drop-zone scaling stay deferred.
- Lands with unit 7, not before: contract version 3 to 4 and protocol version,
  broker and service decode and encode switch, handshake and version
  negotiation, adapter report and reply shapes, legacy envelope removal,
  protocol and core harness switches, and live cutover of the v4 scenarios.

## Evidence

- Initial fixture inventory: 518 messages (275 valid, 3 settings-diagnostic,
  81 held-row, 159 rejected), 22 scenarios, 257 steps and eight OPEN/TBD gates.
- Architecture inspection confirms the existing handwritten core reply encoder
  and protocol request decoder are v3. Strict reply decoding and a v4 scenario
  runner must be added; static fixture validation is not codec conformance.
- Connection types and handwritten handshake/settings/error codecs are present.
  Raw settings retain invalid/unknown values for later per-field processing.
  Strict JSON parsing rejects duplicate decoded keys at every nesting depth.
- Connection fixture tests cover all 89 manifest entries in those families:
  38 accepted messages re-encode byte-exactly and 51 negatives fail decoding
  directly. Thirteen focused tests also exercise C0/UTF-8 strings, decimal expansion,
  signed zero, binary64 extremes and 20,000 deterministic bit-pattern samples.
  Decimal-encoding tie witnesses additionally cover `1000000000000000.25`
  choosing `.2`, and `.75` choosing `.8`. Rust Display chose `.3` for the first
  witness; the codec now uses Ryu's shortest formatter with ties-to-even before
  exponent expansion. Correctly rounded parsing uses `serde_json`'s
  `float_roundtrip` feature and is checked against binary64 boundary witnesses.
- `cargo test -q -p tiler-core -p tiler-protocol`,
  `cargo build -p tiler-core -p tiler-protocol`, and
  `cargo clippy -p tiler-core -p tiler-protocol --all-targets -- -D warnings`
  pass. Formatting and `git diff --check` pass.
- Text-bound clarification: error messages and diagnostic text inherit the
  general 4096-byte UTF-8 metadata limit; producers emit fixed, non-echoing error
  messages. The contract now states this explicitly without adding a new cap or
  catalog. The codecs enforce the limit; UTF-8 boundary tests and fixed typed
  error production pass.
- Removed the retired `home-then-cached-live` source from core and protocol;
  implicit sends use retained home followed by the action-context chain. A
  direct decoder regression rejects the retired token. Existing source-selection
  tests now cover the retained route and stale-source refusal.
- Added reusable core settings state for all 20 computational wire settings,
  with catalog validation, last-good/default behavior, revision fencing,
  duplicate diagnostic replay and restart-only requested/effective separation.
  Native selector coupling is ignored without host-owned native domains.
  Protocol integration tests drive settings message fixtures with their required
  context and the bootstrap scenario's settings steps through real core state;
  this is not execution of the whole bootstrap scenario.
- Raw-setting-name clarification: names are UTF-8 <=4096 bytes and longer names
  refuse decoding before field processing. This prevents an accepted unknown
  name from producing an over-bound diagnostic key, without truncation. The
  contract now states this explicit limit. Decoder/encoder boundary tests pass
  for 4096/4097-byte ASCII and multibyte names, including in-bound unknown names
  processed by core and re-encoded in diagnostics. Nested raw-object keys are
  not setting names and retain their raw invalid-value semantics.
- Readiness is connection-latched after accepted initial settings, including
  field diagnostics, and a complete usable report. Later incomplete/unreadable
  reports keep readiness and use narrow scope holds; a new handshake resets it.
- A valid report before initial settings receives an empty correlated not-ready
  reply, not an error. It confers no authority and is not retained; readiness
  requires a complete usable report processed after initial settings acceptance.
  An incomplete report after settings, before readiness, receives the same empty
  reply and mints or retains no authority. A later complete usable report
  initializes and latches readiness. Read-only fixture/scenario inspection found
  no conflicting initial-incomplete row expectation; existing incomplete core
  scenario steps follow complete reports. This is not scenario execution.
- Connection ownership is integrated into the staged engine: handshake binds
  adapter identity and capabilities, settings drive policy, and report-controlled
  owner/capability/settings fields are removed. Tests use real handshake/settings
  setup, including restart-only modes and fixed capabilities. Core and protocol
  startup-order tests cover empty pre-settings replies, envelope fencing and
  command non-consumption. Re-handshake preserves core presentation configuration
  while resetting lifetime state; focused regressions pass.
- The shared first-incomplete regression now verifies empty correlated replies,
  no minting or retained authority, initialization on a later complete report,
  and subsequent held flights without native creation or readiness revocation.
  Full core/protocol tests pass (1088 core unit tests and every integration
  target), as do build, scoped all-target clippy, formatting and diff checks.
  Focused-close reconciliation preserves both the prior-focus hold and unrelated
  sibling-close reflow; focused regressions and the complete suite pass.
- World request ingress now reuses the shared strict JSON parser. A failing-first
  regression demonstrated last-key-wins acceptance; root, escaped-equivalent,
  nested row/hints/command duplicates and trailing tokens now reject directly
  with fixed malformed errors. Valid escaped keys and separate-object key reuse
  still decode. Full core/protocol tests, build, scoped clippy and formatting
  pass; this does not establish the remaining v4 world schema or reply decoder.
- Screen-fact ingress now requires scale/identity, accepts report-level primary,
  and derives per-output gap/outer geometry with checked once-only scaling.
  Identifiable bad scale/identity facts hold narrowly with typed diagnostics;
  retained fields and original row ordinals preserve valid/held screen order.
  Nested valid/held rows re-encode to immutable fixture bytes using shared
  canonical identity key order, UTF-8 escaping and shortest binary64 spelling.
  Checked canonical writers reject nonfinite input rather than encoding null;
  pre-validation estimates still count surrounding carried structure and text.
- Direct typed invalid screen facts and decoded held twins produce equal graded
  reports and engine replies. Two-output regressions preserve healthy authority;
  invalid-only initial reports establish none. Structural screen failures refuse
  before grading or codec holds, including violations hidden behind another bad
  row. The screen section cap counts valid and held rows together. These boundary
  defects were reproduced before correction. Full core/protocol tests pass
  (1120 core unit tests, all integration targets, protocol suites 44/43/4), as do
  build, scoped all-target clippy with warnings denied, formatting and diff checks.
  Full request/reply v4 conformance, physical reconnect identity and non-gap
  metric scaling remain pending.
- User 2026-11-02 resolved self-identity applicability: Settings is ordinary;
  only own-process internal UI is excluded, never a separate process sharing
  executable/bundle. The contract now carries required nullable processId in
  applicationIdentity and WindowFact, with independent unavailableFacts support.
  The [resolved discriminator](../spec/reference-outcomes.md#raw-self-identity-applicability)
  replaces the former Windows Settings-class and macOS appId/executable rules.
  Windows Settings resizability and macOS Settings process distinction are
  separate product follow-ups, not phase-1 implementation.
- Handshake codecs and typed core ingress share process-id/text validation and
  independent executable/process availability checks; refusal preserves the
  existing connection and mints no generation. Test setup preserves caller
  capabilities. Failing-first regressions cover typed grammar bypass, capability
  rewriting and a fixture incorrectly coupling unknown executable to processId.
  Required fields/catalog witnesses are reconciled across the target corpus;
  separate-process Settings and same-executable windows remain controlled, while
  own-process UI is the exclusion witness. Generic-dialog/XAML rules are unchanged.
- The process-identity foundation passes all core/protocol tests (1123 core unit
  tests, all integration targets, protocol suites 49/44/4), build, scoped all-target
  clippy with warnings denied, formatting and diff checks. The 89 connection
  fixture gate remains byte-exact. Static checks cover all 515 canonical messages,
  66 catalogs/346 arms, 21 FactName arms, 437 enum witnesses, 71 records/254
  witnesses and 22 scenarios/257 steps/406 compiled jq assertions; all eight
  OPEN/TBD gates remain unchanged. Raw WindowFact production classification and
  own-process exclusion are the next implementation slice, not established by
  these static fixture checks.
- Authority scopes now flow as typed output/workspace references through real
  publication, canonical encoding and size accounting. Uniform output authority
  uses an output arm; divergent workspace authority remains narrow. Shared
  internal policy projects through real output ids, never a synthetic output.
  Closed-union decoding and two nested fixture-row witnesses pass; this does
  not establish strict decoding of a whole v4 reply. Distinct native workspace
  ids and tagged workspace-reference vocabulary are defined for attempt migration.
- Scope review regressions lock exact row-size accounting and prevent broad
  output control from proving a newly appearing workspace was previously empty:
  the exact workspace must have been published in the prior reply. Scoped
  core/protocol tests (1081 core unit tests), build, clippy, format and diff
  checks pass after these fixes.
- Member and native-selector attempts now use tagged workspace references;
  parallel native pre/post/base fields and untagged forms are removed from
  core/protocol. Native references translate through core bindings using their
  embedded output. Native-free logical selector attempts reject; they are not
  a visibility/focus completion route. Three nested attempt fixture witnesses
  and malformed/removed-arm decoder cases pass.
- Historical logical-selector tests now assert rejection plus independent
  migration/member invariants, or use explicit host-owned synthetic native
  observations for selector-specific refusal behavior. The three-output native
  settlement regression includes a typed post on a third output and verifies
  external movement wins and releases source backing. Full core/protocol tests,
  build, scoped all-target clippy, formatting and diff checks pass after attempt
  migration. Whole-message v4 fixture comparison and native-free visibility/
  focus completion coverage remain pending.
- Adapter build observation: `cargo check -p tiler-macos` fails with three
  references to removed `SendSource::HomeThenCachedLive` in `workspace_send.rs`
  and `world_command_map.rs`. This is expected port work; no adapter changes
  were made. Other adapter builds have not yet been inventoried.
- Raw `WindowFact` now carries nullable native classification/state/identity,
  visibility and last-write facts. Adapter verdict fields and `observedWs` are
  removed, not accepted as a compatibility shape. Core derives ordinary and
  dialog/sheet float admission, fixed-size origins and safety exclusions using
  connection-owned availability; own-process equality alone excludes internal UI.
  Separate-process same-executable/bundle windows and Settings remain ordinary.
- Supported unreadable inputs stay unknown; only declared unavailable predicates
  are skipped. A known float origin makes unrelated transient/modal/layer nulls
  irrelevant. Generic-XAML executable uncertainty holds the border predicate,
  not membership. Unknown maximize on a connection without maximize support
  preserves membership but withholds frame/clear authority. These rules align
  with the [Windows native evidence](windows-v4-native-evidence.md#physical-check-series-closing-summary):
  per-window read failure never changes `unavailableFacts`, and readable elevated
  metadata is not write authority. No contract shape change follows from those
  probes.
- Identifiable invalid/conflicting window rows retain raw fields, original
  ordinals, live presence and known homes. Complete inventory no longer turns a
  held row into closure or re-mints a born-clear operation after recovery.
  Structural violations refuse before grading, including invalid ids masked by
  another bad fact. Known safety/own-process exclusions retire managed slots and
  authority, not native lifetimes; retained fixed-tile overrides and automatic
  origins survive temporary exclusion and reset on actual lifetime termination.
  Unknown sticky refuses affected migration/swap operations rather than silently
  moving a partial member set.
- Nested raw-window fixture witnesses cover accepted and held canonical bytes,
  last-write visibility samples, closed vocabulary and retired keys. Optional
  `nativeWs` omission, null, empty and known-id spellings remain distinct for
  exact encoding/digests while unknown membership stays semantically unknown.
  Test/harness logical membership echoes are replaced by native observations or
  raw member-attempt evidence at simulated writes, not desired-state echoes.
- Raw-window verification: 1145 core unit tests, core integration targets and
  protocol suites 49/52/4 pass with only `workspace_unsafe_posture` filtered.
  Build, scoped all-target clippy with warnings denied, formatting and diff
  checks pass. The unfiltered test command at `e21d11e8` fails that workspace
  guardrail on `allow(unsafe_code)` in the two Windows probe examples; those
  files and the allowlist are unchanged by this increment. This integration
  failure is not a raw-window test pass.
- Static corpus checks remain 518 messages, 515 canonical messages, 66 catalogs /
  346 arms / 437 enum witnesses, 71 records / 254 witnesses and 22 scenarios /
  257 steps / 406 compiled jq assertions. All eight OPEN/TBD gates are unchanged;
  static validation and nested byte witnesses are not full v4 scenario execution.
- The raw-window increment deferred fail-closed refinements recorded in the
  [raw classification discriminators](../spec/reference-outcomes.md#raw-classification-hold-discriminators):
  known floats with unreadable hints overheld rather than defaulting hints.
  The refinement below closes that overhold; unselected nonordinary-kind
  admission stays TBD. Full visibility/focus, state/intent persistence and
  renderer lanes remain separate increments.
- Known-float unreadable-hint refinement: independently proven dialog/sheet,
  transient/modal and retained explicit/automatic-fixed floats now keep membership
  with raw `hints:null`. Existing safety/process holds and tile-override precedence
  remain; ordinary hint-dependent admission still holds. Healthy same-output and
  other-output siblings retain authority without inventing empty constraints.
- Hint-dependent ToggleFloat and directional half-snap remain fail-closed.
  Directional Move refuses before maximize-clear kickoff; unreadable hints also
  withhold outstanding float frame and float-route clear tokens without cancelling
  their bindings. Readable recovery resumes the same operation identity, and
  spent clear attempts remain spent. Retained frame desires without write tokens
  are not actuation or native completion evidence.
- Regression development reproduced two dependent-action gaps after membership
  narrowing: a hints-null directional Move incorrectly returned
  `MoveFloatApplied`, and a commandless hint-read gap republished a retained
  `frame_operation`. Raw-hint guards and publication vetoes close both. Twelve
  focused float-membership tests now cover known origins, ordinary/tile-override
  holds, hint-read cycles, constrained/refused and readable/recovered moves,
  retained tokens and spent-clear fencing. No wire shape or fixture change.
- Final refinement verification: `cargo test -q -p tiler-core -p tiler-protocol
  -- --skip workspace_unsafe_posture` passes (1151 core unit tests, all integration
  targets, protocol suites 49/52/4); only the previously documented unrelated
  workspace unsafe-posture test is filtered. Build, scoped all-target clippy with
  warnings denied, formatting and diff checks pass. Native/reference unreadable
  hint behavior remains TBD; no live acceptance, complete codec conformance or
  whole-scenario execution is claimed.
- Opaque intent data foundation: core now defines `IntentData` with independent
  nullable float override and sticky-prior lanes, strict bounded decoding and
  canonical encoding of exactly `v:1,float,stickyPrior`. All nine semantic states
  remain distinct. Required fields, decoded-key duplicates (including escaped
  equivalents), unknown keys, wrong types/versions, malformed/trailing JSON and
  the 1024-byte UTF-8 bound are tested. No additional crate dependency is needed.
- Eight core data-codec tests and four nested-fixture tests pass. Supported
  intent records and save data re-encode to the immutable fixture strings;
  unsupported inner version returns no usable data. Invalid inner data never
  bypasses independent outer string/type/byte-bound validation. These tests do
  not execute the outer world codec, engine restoration or host persistence.
  Review corrected a required-key documentation error and numeric-token
  terminator diagnostics without changing the accepted data language.
- Intent-data verification: full core/protocol tests pass (1159 core unit tests,
  all integration targets, existing protocol suites 49/52/4 plus four new nested
  intent tests), with only the unrelated `workspace_unsafe_posture` filtered.
  Build, scoped all-target clippy with warnings denied, formatting and diff
  checks pass. Outer intent ingress/publication and real-engine restore/save/clear
  are the next dependent slice. No fixture, wire version or state/overlay change.
- The opaque record remains classification intent only, with no identity or
  maximize/fullscreen fields. The 2026-11-03 composed-maximize amendment below
  resolves the formerly OPEN macOS native-Zoom versus owned-overlay choice
  without extending this record. M3 restart binding remains unproved, and no
  FrameWitness change is introduced.
- Outer intent lanes are staged: `intentRecords` is required ingress with
  closed rows, valid subject ids, <=10000 entries and <=1024 UTF-8 bytes per
  opaque string. The wire preserves decoded string bytes without interpreting
  inner JSON; malformed/unsupported inner records are not transport refusals.
  Typed ingress enforces the same bounds. Report equality/digest includes subject,
  order and exact data; changed data cannot reuse a prior request sequence.
- Both reply writers publish required `intentWrites` after presentation and
  before diagnostics, with opaque save strings or explicit null clears. A typed
  intent-write validator enforces section/id/string bounds at the engine's staged
  publication boundary before exact encoding, commit and reply caching. The legacy
  String-returning encoders remain unchecked, with that validation as a caller
  precondition; complete checked reply decoding remains separate migration work.
- Engine intent writes remain empty and returned records confer no authority in
  this increment. Fifteen protocol integration tests cover immutable nested
  witnesses, wrong/missing/duplicate keys, both lane bounds, multibyte/control
  escaping, exact new-section sizes, request-reuse fencing, oversized-request
  non-mutation and behavior equality with an empty-record twin. The outbound
  rejection helper is exercised directly; the engine cannot yet produce invalid
  nonempty writes, so its rejection branch is source-reviewed rather than
  dynamically injected. Nonempty engine writes need a pre-mutation budget bound
  when lifetime-fenced restore/save/clear is implemented next.
- Outer-lane verification: core/protocol tests pass (1159 core unit tests, all
  integration targets, protocol suites 49/4/15/52/4), with only the unrelated
  `workspace_unsafe_posture` filtered. Build and scoped all-target clippy with
  warnings denied pass. No v4 fixture, version, native state or adapter source
  changes are included; this is not complete bidirectional v4 conformance or
  execution of the intent restart scenario.
- Composed-maximize contract increment: User 2026-11-03 selected a core-owned
  work-area overlay on maximize=false with usable frame authority, generalizing
  the nativeSticky/composed-sticky split. maximize=true remains native unchanged.
  Core retains the base slot/share/allocation or float placement without sibling
  reflow; native maximized stays null, non-null holds the conflicting row, and no
  maximized StateTarget is emitted. External user move/resize clears the bit;
  Window-menu Zoom is ordinary frame drift. Born-maximized is undetectable and
  admits ordinarily. Orderly restart drops composed maximize to base without M3
  dependency or v/float/stickyPrior changes. B9 clears locally, R-MAX-11 exits
  native fullscreen under existing gates before composition, and sends/output/
  work-area changes recompute the overlay frame. Directional and fullscreen
  precedence rules remain; no platform has adapter-owned maximize policy.
- Contract shape inspection found no required capability or immutable fixture
  changes: existing frame/frame_auth expresses composed geometry and restore,
  including float subjects; macOS witnesses already use false/null and existing
  maximize scenarios use native support. The normative frame-authority text now
  distinguishes composed frames from ordinary one-shot float placement. Tier 1
  COSMIC/Hyprland retained-slot overlays support the design; i3/sway have no
  maximize counterpart. Minimal composed-action rows keep unexecuted/native
  results TBD. This is a documentation-only amendment, not engine/codec or live
  acceptance; reassess after rearchitecture/ports if jank/issues appear.
- Independent contract review found no shape/fixture conflict. User 2026-11-03
  resolved its unavailable-frame-authority edge by applying existing refusal
  precedent: refuse a composed maximize request via the existing applicable gate,
  with overlay intent unchanged. No deferred/automatic composition on recovery;
  a fresh press is required. The minimal discriminator retains only unproven
  native refusal/recovery delivery as TBD, not the selected rule or refusal kind.
- Documentation checks: 30 added local links/anchors resolve, added text is ASCII,
  and `git diff --check` is clean. Exactly seven documentation files change; source,
  adapters and immutable v4 fixtures are unchanged. No code tests or live checks
  were run for this documentation amendment; the refusal edge is resolved above.
- Restore-only intent consumption now runs in the real engine for explicit
  float/tile records with stickyPrior=null. A jointly bounded pending/conflict
  ledger carries verified data through incomplete inventories, held rows and
  unreadable facts. Conflicting subjects grant no import authority; malformed,
  unsupported, null-only and sticky-bearing records do not partly restore state.
  Complete inventory absence or explicit close prunes pending data; a carried
  opaque row is not live-window proof. Handshake resets all import state.
- The first-successful-admission fence includes healthy sessionless floating
  workspaces, not just tiled sessions; merely homed held/hidden/overlay-only
  newcomers do not seal it. Late records cannot overwrite runtime choice.
  Restored intentional floats use the observed frame without minting a placement,
  clear or focus operation. Explicit tiles commit the real override even when
  initially non-fixed, preserving it across hint changes and exclusion/recovery.
  Floating-workspace tile intent retains its base until tiling is enabled.
  Successful newer float/unfloat commands supersede restored provenance and
  refresh the exclusion ledger, avoiding stale tile resurrection.
- Independent review exposed orphan retention, float durability, stale tile
  mirrors and minimized/own-process/conflict cleanup gaps; these are corrected
  with regressions. A failing-first floating-newcomer test additionally proved
  that post-sweep membership was too late for the initial latch; post-membership
  floating admission and staged-health gates now close that boundary without
  sealing two consecutive unreadable-hint observations prematurely.
- Restore verification: all 26 focused real-engine regressions and the full
  core/protocol suite pass (1185 core unit tests, every integration target,
  protocol suites 49/4/15/52/4), with only the previously documented unrelated
  `workspace_unsafe_posture` filtered. Build and scoped all-target clippy with
  warnings denied pass. The initial formatting check found three layout-only
  differences; after correction, formatting, diff whitespace, added ASCII and
  added local-link/anchor checks pass. No adapters, immutable fixtures, wire
  shapes or versions change.
- Engine intentWrites remains empty: save/clear publication and its nonempty
  pre-mutation size bound, sticky semantics, host storage survival and native
  ordering acceptance remain pending. The existing whole-candidate rejection on
  a rejected command also remains staged migration work; successful command
  supersession is tested here, not the full v4 accepted-report/rejected-command
  contract. No native identity/recovery or complete intent-restart scenario
  acceptance is claimed, and all eight existing OPEN/TBD gates remain unchanged.
- Save-only publication now emits at most one canonical intent record for a
  successful, non-consumed ToggleFloat: float=true for an intentional float and
  float=false for a genuine tile win, including unfloat from automatic fixed
  origin. The tile override already commits with lifecycle admission; this records
  it rather than clearing the slot. Publication accompanies the command result
  before dependent adapter writes; it proves logical intent, not native execution
  or successful host storage. No-space retries, refusals, bootstrap, mere automatic
  floats, restore-only reports and closes emit no new saves or null clears.
  Duplicate requests repeat the cached save; consumed commands do not save again.
- Pre-admission sticky-bearing records now retain only a lifetime-scoped save
  block, not partially restored sticky semantics. Any valid sticky-bearing row in
  conflicting duplicates also blocks a null-sticky overwrite. Blocks survive
  admission and held/excluded/minimized gaps; later non-sticky rows do not erase
  them, while late post-admission rows remain ignored. Every pending, conflict
  and block allocation counts the same 10000-entry bound. If a required block
  cannot fit, all intent saves are suppressed until handshake reset; runtime
  commands still apply without erasing the deferred record.
  Existing close, own-process and proven-complete-departure cleanup still applies;
  retained session membership remains live proof, not an inferred close.
- The pre-mutation reply bound reserves the single possible save row only for a
  ToggleFloat report: 16 punctuation bytes, the quoted subject and at most 48
  bytes for the escaped canonical data string (194 bytes with a maximum id).
  The existing exact-encoding staged-commit gate remains the backstop. A boundary
  regression keeps the request and saveless bound in range, then proves the save
  reservation alone rejects before state, intent ledgers or command consumption
  change; a smaller same-sequence retry still saves successfully.
- Save verification: 32 new save regressions plus the 26 restore regressions pass;
  four prior command-success assertions now expect the actual canonical save.
  Coverage includes float/tile restart round-trips without re-saving, no-space,
  duplicate/consumed/refused commands, sticky preservation and joint-cap fencing.
  Independent source review found no remaining defect. Full core/protocol tests
  pass (1217 core unit tests, every integration target, protocol suites
  49/4/15/52/4), with exactly the unrelated `workspace_unsafe_posture` filtered.
  Build, scoped all-target clippy with warnings denied, formatting and diff checks
  pass. No adapter, immutable fixture, wire shape or version changes are included.
  Sticky/state integration, actual no-intent clear transitions, host storage and
  native ordering proof, accepted-report/rejected-command conversion and complete
  v4 codec/scenario acceptance remain pending; all eight OPEN/TBD gates remain.
- Sticky fact authority now consumes nativeSticky: without native support, a raw
  sticky=null permits ordinary admission, while either non-null bool becomes a
  narrow conflicting-fact row hold with its raw fields and ordinal retained.
  Native support retains known-bool behavior and supported-null holds. The pure
  classifier also fails closed on unsupported bools if grading is bypassed;
  no native fact is projected from retained intent or rewritten to false.
- A failing-first engine test reproduced the reversed boundary: composed null
  held while unsupported true admitted. Seven focused regressions now cover both
  capability routes, mixed-output authority, retained raw bools/ordinals,
  structural-error and invalid-metadata precedence, pending-intent recovery,
  first-admission and duplicate/reused-request fences, and a composed-null
  ToggleFloat save. Existing synthetic fixtures that carry native sticky bools
  now explicitly declare native support; supplied capabilities and production
  defaults are not rewritten. Composed tests use explicit nativeSticky=false.
- Independent review found no blocking defect. Full core/protocol tests pass
  (1224 core unit tests, every integration target, protocol suites 49/4/15/52/4),
  with exactly the unrelated `workspace_unsafe_posture` filtered. Build, scoped
  all-target clippy with warnings denied, formatting and diff checks pass.
  No adapter, immutable fixture, wire shape/version or intent-data shape changes
  are included. This is a prerequisite, not sticky restore/save completion:
  deferred sticky save blocks remain, no new clear transition exists here,
  and sticky commands, visibility/state targets and native settlement remain
  pending. Downstream swap/migration raw-sticky gates still treat null as unknown
  until their composed-semantic migration; ordinary admission and the tested
  float save do not establish those action routes. All eight OPEN/TBD gates remain.
- Semantic sticky restoration now accepts all six float=null/false/true and
  stickyPrior=false/true combinations as whole records before first admission.
  Every non-null prior restores sticky-float membership, not an ordinary float or
  tile override, and commits the exact prior only after healthy admission. The
  retained prior participates in the joint 10000-entry ledger bound; pending-to-
  retained transfer does not increase occupancy. Record-free observations, holds,
  minimize, temporary exclusions and floating-workspace handoff retain provenance;
  explicit close, own-process proof, proven departure and handshake clean it up.
- The current-mode encoding rule is recorded beside the IntentData table: future
  sticky-on saves float=true with stickyPrior=previous mode; sticky-off saves
  float=prior with stickyPrior=null. This technical encoding does not narrow the
  six accepted inputs or change the selected sticky-off destination behavior.
  Native support still requires readable sticky facts; unsupported non-null facts
  still hold narrowly. Semantic intent never rewrites raw native observations.
- Restore-only reports emit no intent writes, frame/clear operations or focus
  effects for the restored subject. ToggleFloat temporarily refuses for usable
  pending or retained sticky intent through the existing operation gate, preserving
  state and command highwater rather than partly tiling and resurrecting sticky.
  Conflict-only/save-block-only rows confer no such authority; ordinary unrelated
  commands still execute and save. This fence is not the final sticky transition.
- Thirteen focused sticky-restore tests cover all six admissions, both capability
  routes, supported-null and unsupported-bool recovery, unhealthy newcomers,
  hints-null float membership, record-free retention, orphan/lifetime cleanup,
  floating-workspace first appearance/re-enable, late records, joint bounds and
  the narrow refusal fence. Six earlier save expectations now reflect semantic
  restoration; a new save regression checks refusal and an ordinary-window retry.
  Independent review found no blocking defect. Initial clippy checks found
  equivalent classifier branches and get-then-check assertions; equivalent
  condition/assertion cleanup resolved them without changing behavior.
- Final sticky-restore verification: the full core/protocol suite passes (1238
  core unit tests, every integration target, protocol suites 49/4/15/52/4), with
  exactly the previously documented unrelated workspace_unsafe_posture filtered.
  Build, scoped all-target clippy with warnings denied, formatting, diff whitespace,
  added ASCII and added local-link checks pass. No adapter, immutable fixture,
  protocol, wire shape/version or intent-data shape changes are included.
  Sticky commands/publication, visibility/state targets, native settlement,
  downstream composed-sticky swap/migration gates and accepted-report/rejected-
  command conversion remain pending. Same-report sticky import plus a refused
  command still follows whole-candidate rejection; no complete restart scenario,
  host storage survival or native ordering acceptance is claimed. All eight
  OPEN/TBD gates remain unchanged.
- Outbound target plumbing now exposes VisibilityTarget, the four-property
  StateTarget vocabulary and the WindowDesire cmdSeq/visibility/visibility_auth/
  visibility_hold/state lanes. Both canonical reply writers emit the frozen key
  order and quoted counters. Every engine publication branch still emits null
  visibility, untouched visibility authority and an empty state array; this is
  representational plumbing, not visibility/native-state policy or actuation.
- Existing explicit float-placement and directional-clear bindings now retain
  typed originating command sequences. Published tokens carry that sequence
  across observation-only replies and clear-to-float continuation; withheld or
  spent tokens carry null, as do commandless born clears. Tiled continuation no
  longer recovers command correlation by parsing an operation id.
- A failing-first regression exposed an existing shared-row collision: an old
  placement could remain outstanding when a newer directional intent minted a
  maximize-clear for the same window. The clear mint now supersedes that old
  placement, applying the inherited newer-intent rule without changing unrelated
  command behavior or the wire shape. The regression verifies only the new clear
  token and its original sequence survive.
- Target-lane sizing is exact for null/object visibility, all four state
  properties and authorities, multiple state rows and counter boundaries. The
  current engine's null/untouched/empty suffix costs 94 bytes, or at most 112
  with a quoted command sequence. The pre-mutation bound explicitly reserves
  112 per possible published window, conservatively counting current/held,
  staged, retained-session and mover footprints. It does not borrow other
  operation reserves or reserve speculative future targets. A boundary regression
  proves this added reservation alone refuses before mutation and a smaller
  same-sequence retry succeeds; the exact staged encoding gate remains intact.
- Independent review found no production defect but required a stronger fixture
  oracle. Eight immutable reply witnesses now compare actual encoded target
  suffixes directly against raw fixture bytes, including the full state-authority
  matrix, shows, staged/fallback hides and default lanes. Real-engine tests assert
  frozen wire suffixes for explicit clear, observation retention, float continuation
  and commandless birth; shared-writer equality alone is not the fixture oracle.
- Final target-plumbing verification: all core/protocol tests pass (1249 core
  unit tests, every integration target, protocol suites 49/4/15/2/52/4), with only
  the previously documented unrelated workspace_unsafe_posture filtered. Build,
  scoped all-target clippy with warnings denied, formatting, diff whitespace and
  added ASCII checks pass. No adapter, immutable fixture, version or request
  decoder changes are included. StateAttempt ingress/settlement, target-producing
  visibility/state policy, sticky commands/saves replacing the temporary fence,
  remaining state and core-composed maximize remain pending. Future target
  producers must extend the current-default pre-mutation reservation. No native,
  complete message-codec or whole-scenario acceptance is claimed; all eight
  OPEN/TBD gates remain unchanged.
- StateAttempt ingress now requires stateAttempts, even when empty, with exactly
  eight keys per row, the four closed properties, canonical quoted counters and
  required nullable pre/post bools. Core validation bounds the lane at 1024 rows
  and rejects repeated window/property pairs across revisions. Every raw field,
  nullable presence, row order and original stamp participates in the digest and
  request estimate. Immutable nested rows pass through the staged decoder; missing
  lanes, bad shapes/counters, duplicate decoded keys and trailing input refuse.
- Supported attempts match an internal retained window/property/target/revision
  binding and become spent before inventory, gesture or staged-readability holds.
  Accepted calls require both immediate post and readable staged state to equal
  the target; explicit refusal requires unchanged readable pre/post and staged
  preimage. Throws, nulls and mismatches remain possibly executed, never blind
  retries. Retained proof can settle on a later attempt-free report; redelivery
  cannot replace the original evidence. The contract now identifies rev as the
  authorizing reply revision, matching the immutable window-state sequence.
- Unsupported property attempts preserve their raw rows and add only a narrow
  conflicting-fact attempt diagnostic where a reported or retained output is
  known. No output is invented for an unknown subject, and unrelated authority
  remains intact. Diagnostics are rederived during staging; the pre-mutation
  reply bound independently reserves 512 bytes per raw attempt. A boundary test
  isolates that reservation, verifies no mutation on rejection and permits a
  smaller same-sequence retry.
- Internal bindings use one map entry per window/property, with a 40000-entry
  ceiling. Exact-cap coverage confirms Applied settlement at 40000; a distinct
  40001-entry corruption probe fails closed. Close, own-process proof, proven
  complete departure and handshake clean up bindings; incomplete inventory alone
  does not. Settlement reads the final post-migration staged domains and never
  treats a binding alone as live membership proof.
- StateAttempt verification: 18 new core settlement regressions and eight new
  protocol ingress tests pass. Independent source review found no blocking
  findings. Clippy initially found duplicate prune branches and test-helper
  lints; equivalent exactly-once cleanup and test-only lint fixes resolved them.
  Final core/protocol tests pass (1267 core unit tests, every integration target,
  protocol suites 49/4/15/8/2/52/4), with only the previously documented unrelated
  workspace_unsafe_posture filtered. Build, scoped all-target clippy with warnings
  denied, formatting, diff whitespace, added ASCII and local-link checks pass.
- This slice adds no target producer, sticky command/save transition, native
  actuation, visibility policy or new intent write. Production state bindings
  remain empty until target producers arrive; settlement proofs use internal
  bound-target fixtures without a production fixture-seeding API. Future producers
  must enforce binding allocation bounds and reserve their nonempty target lanes.
  Adapter constructors, immutable fixtures and the staged wire version are
  unchanged. No complete message-codec, whole-scenario or native acceptance is
  claimed; all eight OPEN/TBD gates remain unchanged.
- Composed sticky commands now execute SetSticky/ToggleSticky and sticky-clearing
  ToggleFloat through transactional candidate restaging and affected-domain
  convergence. Sticky-on floats and saves the previous mode; sticky-off restores
  that mode, while ToggleFloat always selects a tile base. Both clear routes land
  on the owning output's current workspace, sweeping the source without retaining
  stale minimized slots. Floating destinations do not force a session or acquire
  explicit-float provenance over a tile base; re-enabling tiling restores that base.
- Canonical command saves publish once in the issuing reply, before dependent
  writes. Duplicate requests repeat cached saves; consumed and commandless reports
  do not emit them. Same-state SetSticky consumes without re-saving. All six
  imported sticky shapes preserve their prior mode. Successful transitions
  supersede deferred save blocks, enforce the joint 10000-entry intent bound and
  refuse atomically if required saved-record knowledge cannot fit. Genuine
  no-space clear refusal retains sticky, saved intent and command highwater;
  a smaller same-command retry can tile without a false earlier tile save.
- Composed sticky publishes a same-output shown=true visibility target with a
  retained authorizing reply revision, no native sticky fact or StateTarget.
  Observation-only view changes and suppression recovery keep that revision.
  Held, excluded, minimized, incomplete and gesture-barrier rows grant no visibility
  write authority. Capability-absent maximize and declared-unavailable ownership
  inputs do not create false unreadability holds; supported-null inputs still hold.
- Strict set-sticky/toggle-sticky codec arms and the closed Rejected command-result
  catalog are present without a version or compatibility-shape change. Both
  sticky routes absent yields an accepted StateUnsupported result after existing
  safety gates. Other rejected commands still use staged envelope refusal; this
  increment does not establish the full accepted-report/rejected-command contract.
- Pre-mutation sizing now reserves 183 bytes per possible published target suffix
  (controlled show 169), including the longest retained revision/correlation and
  hold reason. Every possible sticky/float command save reserves its escaped
  canonical record, at most 48 data bytes or 194 bytes with a maximum subject id.
  Exact row estimates and a boundary rejection/smaller same-sequence retry are
  tested; native state rows remain empty and need their own producer reservation.
- Composed verification: 41 new sticky command/publication regressions plus sizing,
  intent-save and codec witnesses pass. Independent review and targeted correction
  review found no remaining blocking findings. Review refinements fixed visibility
  overholds, superseded-block accounting and floating-destination tile-base
  retention. Initial clippy found duplicate insertion branches and an argument-count
  lint; equivalent guards and the existing narrow lint convention resolved them.
  Final core/protocol tests pass (1313 core unit tests, every integration target,
  protocol suites 49/4/15/8/2/53/4), with only the previously documented unrelated
  workspace_unsafe_posture filtered. Build, scoped all-target clippy with warnings
  denied, formatting, diff whitespace, added ASCII and local-link checks pass.
- At the composed boundary, native SetSticky/ToggleSticky and native sticky
  ToggleFloat remain explicitly operation-refused/fenced. Native target production,
  pre-write save settlement, exact-prior rollback on proven refusal and
  ambiguous-outcome reconciliation are
  not implemented by this increment. Persistent above, remaining state including
  composed maximize, downstream composed-sticky swap/migration gates, host storage,
  complete message codecs and whole-scenario/native acceptance remain pending.
  Adapters and immutable fixtures are unchanged; all eight OPEN/TBD gates remain.
- Native sticky commands now mint retained Sticky StateTargets with typed
  originating command sequences and exact authorizing reply revisions. Native-on
  floats before publication; native-off and sticky ToggleFloat pre-save their
  requested record but retain sticky-float membership until correlated Applied
  clear proof. Off honors prior mode; ToggleFloat selects a tile base, including
  adopted sticky. Fresh admission lands on the command's owning-output current
  workspace; floating destinations retain tile base without forcing a session.
- Generic attempt capture remains spent-before-holds and exact-tuple correlated.
  Dependent followups consume proof once, including record-free recovery. A
  deferred Applied clear additionally requires a fresh raw sticky=false before
  landing: an intervening external restick or unreadable fact cannot reuse stale
  proof to tile. No raw fact is rewritten, and spent targets never regain native
  write authority from observation-only reports. A fresh explicit retry re-saves
  the requested record with its retained prior before authorizing a new write.
- Proven refusal restores exact pre-command runtime provenance/home and publishes
  the exact prior saved record, or a null clear when none existed. Nullable
  float/stickyPrior shapes are preserved, not inferred from current membership.
  A bounded per-window rollback queue drains on commandless and consumed replies;
  a newer same-window command save wins, while unrelated commands do not suppress
  rollback. Ambiguous/unproven attempts retain requested intent without a native
  retry or inferred Applied result. Lifetime close, own-process proof, departure
  and handshake retain the existing cleanup boundaries.
- Saved-record knowledge now includes inert null/null input without granting
  restore authority. Conflicting or dropped preimages latch native saved knowledge
  incomplete until generation reset, so later pruning cannot turn unknown prior
  storage into a guessed null clear. Late post-admission inputs remain ignored;
  nullable preimage conflict fencing preserves existing composed admission rules.
- Pre-mutation reservation covers native sticky target suffixes (296-byte
  conservative per-window maximum) plus every pending or queued possible rollback,
  including record-free proof recovery. All nine prior shapes fit 49 escaped data
  bytes; each rollback row reserves 196 bytes including maximum id and comma.
  Queue and intent ledger allocation fail closed before followup adoption. Exact
  40000-binding replacement succeeds; a distinct new binding at the cap refuses
  atomically. Smaller same-sequence retries pass after reply-bound refusal.
- Native verification: 43 producer/followup regressions plus saved-knowledge and
  sizing tests pass. Independent review and correction review found no remaining
  blocking findings after reproducing and fixing the deferred-clear freshness
  defect. Clippy guard/style and test-assertion findings are resolved. Final direct
  core/protocol tests pass (1363 core unit tests, every integration target,
  protocol suites 49/4/15/8/2/53/4), with only the previously documented unrelated
  workspace_unsafe_posture filtered. Build, scoped all-target clippy with warnings
  denied, formatting, diff whitespace, added ASCII and local-link checks pass.
  An earlier unfiltered core run was reported to hit the known unsafe-posture
  failure and is not acceptance evidence; that guardrail remains unchanged and
  unresolved.
- Protocol, adapters and immutable fixtures are unchanged by the native increment.
  This is offline command/settlement evidence, not host storage survival, physical
  native execution, complete message-codec conformance or whole-scenario acceptance.
  Persistent above, remaining state including composed maximize, downstream
  composed-sticky swap/migration gates and general no-intent clear policy remain
  dependent work; all eight OPEN/TBD gates remain unchanged.
- Native fullscreen set/toggle commands now decode strictly and produce retained
  Fullscreen StateTargets with typed command correlation and the existing exact
  authorizing-reply-revision settlement. Fullscreen-on from maximized authorizes
  immediately without a maximize write; ordinary exit authorizes only fullscreen
  clear, preserving independent maximize state. Host flag retention is not proven.
  No raw fact, opaque intent or base membership is fabricated by this producer.
- Fullscreen capability absence consumes StateUnsupported, including raw null;
  supported unreadable state consumes StateUnknown. Active exit requires known
  exitability. Same-state sets consume without a new setter, canceling obsolete
  opposite targets; spent same-value targets are never renewed. Toggles resolve
  from current raw facts, not an unconfirmed target. Fresh commands can authorize
  a new attempt only when the requested native transition is still needed.
- Publication revalidates readability, exitability, safety and same-window pending
  transfers. Healthy recovery resumes the exact unspent tuple; spent proof grants
  no automatic retry. A real send/arrival regression proves transfer Hold and
  same-tuple recovery. Held scopes, two-output isolation, generation reset,
  close cleanup and exact 40000-binding replacement/new-key refusal are covered.
  Sticky and fullscreen rows coexist; the per-window target reserve is now 414
  bytes, backed by exact two-row maximum encoding and command-size witnesses.
- Fullscreen verification: 26 producer regressions, strict command-codec tests,
  digest/validation and sizing tests pass. Failing-first regressions reproduced
  unsupported/null precedence, stale opposite authority, lost exitability,
  pending-transfer authority and spent same-state renewal before correction.
  Independent review closed the fullscreen findings. Final direct core/protocol
  tests pass (1392 core unit tests, every integration target, protocol suites
  49/4/15/8/2/54/4), with only workspace_unsafe_posture filtered as above.
  Build, scoped all-target clippy with warnings denied, formatting, diff whitespace,
  added ASCII and local-link checks pass. Adapters and immutable fixtures remain
  unchanged; no native execution, host storage or whole-scenario acceptance claimed.
- Adjacent native-sticky corrections now hold native Sticky StateTarget authority
  during a pending same-window transfer, retaining the exact unspent tuple and
  resuming after raw arrival proof. Other-window transfers do not gate the target;
  spent attempts remain spent. Membership, saved intent and rollback are unchanged.
  The toggle-sticky fixed command estimate is corrected from 28 to 29 bytes, with
  exact literal, shared string-codec and maximum-id byte witnesses; the 414-byte
  per-window target reserve is unchanged.
- Failing-first regressions reproduced Controlled authority under a pending
  same-window flight and a 31-versus-32 command estimate. The sticky subject is
  minted through the real SetSticky producer; the pending same-window flight uses
  the existing internal harness because the real Send route refuses that sticky
  subject. Standard raw member-attempt evidence proves recovery, and a real send
  of another window proves isolation. This is not sticky-send or native acceptance.
  Independent review found no blocking findings. Final direct tests pass (1394
  core unit tests, every integration target, protocol suites 49/4/15/8/2/54/4),
  with only workspace_unsafe_posture filtered as above. Build, scoped all-target
  clippy with warnings denied, formatting, diff whitespace and added ASCII checks
  pass. Protocol, adapters and immutable fixtures are unchanged. Native maximize,
  persistent above and core-composed maximize remain dependent work; all eight
  OPEN/TBD evidence gates remain unchanged.
- Native maximize subunit is implemented: ToggleMaximize produces typed retained
  Maximized targets, with same-revision/command fullscreen-clear pairing for
  explicit R-MAX-11 replacement. Already-maximized fullscreen replacement needs
  only the clear leg; settled maximize gains no write on fullscreen entry/exit.
  Results remain descriptive. Publication revalidates safety and exact paired
  clear authority, holds ordinary/unmaximize targets under active fullscreen,
  and never renews spent targets. The actual three-property suffix maximum is
  531 bytes (Maximized row 116), verified against the encoder; above was not yet
  produced or reserved at this checkpoint.
- Maximize subunit verification: 20 real-command producer/publication tests pass.
  Failing-first regressions reproduced the both-true missing-key cap overflow and
  unmaximize authority under active fullscreen before correction. Both pending
  maximize-on/off regressions also failed before the newer-fullscreen hold, then
  passed through unchanged facts, active fullscreen and exact-tuple exit recovery.
  Ambiguous fullscreen-on releases maximize at the first readable still-false
  post-attempt report without retrying the spent fullscreen target; unreadable
  facts retain the hold. This characterization passed before and after the
  hold-predicate simplification, not a failing-first bug fix; no settlement state,
  event or counter was added. Independent review found no scoped blocker.
  Direct filtered core/protocol tests pass (1414 core unit tests, every integration target, protocol suites
  49/4/15/8/2/54/4), with only workspace_unsafe_posture filtered as above; build,
  scoped all-target clippy with warnings denied and formatting pass. Diff whitespace,
  added ASCII and local links pass; protocol, adapters and immutable fixtures are
  unchanged. This checkpoint verified native maximize and its fullscreen hold;
  above followed separately below.
  The [pending-maximize/fullscreen discriminator](../spec/reference-outcomes.md#pending-maximize-intent-at-fullscreen-entry)
  now selects retain/hold/apply-after-exit for both pending target values; native
  ordering and host applied outcomes remain TBD.
- Persistent above now uses one commandless retained-state owner and external-clear
  latch over the assembled float union, including capacity and empty workspace-floating
  scopes. Exact StateAttempt proof gates owned release and focused-fullscreen scope
  suspension/resumption. Sticky keeps the same owner; external below/above-clear
  relinquishes it without a stacking write, retaining the latch until lifetime end
  or native preowned-above reset. Latching preserves the original tuple and attempt
  evidence. Preexisting above/below, unreadable facts and native overlays grant no
  new write authority; spent setters never retry. Durable preimages remain the
  adapter's mandatory pre-write obligation, with no new availability fact.
- Above verification: 19 focused tests cover acquisition/proof/release, suspension,
  latch/reset, sticky transitions, capacity/workspace-floating membership and binding
  cap/replacement. Failing-first regressions reproduced first-report floating-scope
  suppression, external clear concurrent with tiling issuing an extra release, and
  pending acquire/release retaining write authority after below became true; a
  pending acquire also incorrectly retained authority after above became true.
  The three suspension/latch/sticky lifecycle tests were first run after implementation,
  not claimed failing-first. Independent review's dynamic stacking findings were
  corrected and re-reviewed with no remaining scoped blocker.
- The actual four-property encoder witness is 626 bytes: the prior 531-byte suffix,
  one comma and a 94-byte commandless Above row. The reserve regression failed at
  531 before correction; structural estimation and canonical encoding now agree
  exactly. The binding cap uses a running count for new acquisitions and permits
  same-key release/resume replacements without growth.
- Final native maximize/above verification passes the exact filtered core/protocol
  test gate (1434 core unit tests, every integration target, protocol suites
  49/4/15/8/2/54/4), with only workspace_unsafe_posture filtered as above. Build,
  scoped all-target clippy with warnings denied, formatting, diff whitespace,
  added ASCII and local links pass. Protocol, adapters and immutable fixtures are
  unchanged. Native host/preimage recovery and all eight OPEN/TBD gates remain
  unproven; core-composed maximize remains the next separate increment.
- Composed maximize is implemented with a lifetime-local bit, retained
  tile allocation/float placement and existing float-placement tokens for base
  restoration. Unsupported native maximize is now capability-graded: null is
  ordinary, while either boolean is conflicting-held. Synthetic native-fact test
  setup declares matching support; explicit composed fixtures retain null facts.
  Initial attempts incorrectly allowed false native claims and echoed a base
  frame without restore authority; failing-first regressions reproduced both and
  the ordinary-route focused checks now pass. No new wire field or intent data
  was added.
- Pending fullscreen-on now withholds the tiled base write and float placement
  token as well as the composed frame. Real-command regressions cover pending,
  active and exited fullscreen, explicit replacement and later work-area changes.
  Explicit committed gestures clear the bit; cancel/no-op/refused terminals and
  clamped readback retain it. Composed floats adopt the actual user release frame,
  not a quiet overlay observation. Directional focus skips local siblings, move
  clears locally, and intentional unfloat fresh-admits without a native clear.
  Tile/float sends preserve the bit and recompute the destination overlay after
  arrival; work-area changes recompute in place with bases retained.
- Review found clear-before-refusal early exits in directional move and intentional
  unfloat; their internal paths now preserve the bit, with direct command-seam
  regression checks. Normal rejected reports already discard the staged candidate,
  so report-level refusal tests alone did not discriminate those internal paths.
  Review also found an unsupported cross-output float-gesture refusal. Healthy
  occupied-destination releases now reuse ordinary logical homing and carry the
  existing float record; unknown or held destinations remain fenced.
- Empty-destination float drags clear the bit before destination admission and
  converge through the existing ordinary observation sweep, with no special
  composed admission or transfer. A failing-first paired regression compares the
  composed and ordinary arms through release and quiet follow-up. Both have the
  same homing, row and retained state. The ordinary arm already loses retained
  float geometry on this crossing (null effective frame, untouched authority);
  that pre-existing defect is recorded, not fixed by composition. No valid empty
  destination retains the bit merely because a layout session is absent.
  An earlier source-adopt plus membership-commit attempt lost the base on the
  next sweep and was reverted; clear-only and restage-only attempts also left
  shielded session state until the ordinary sweep reconverged the affected scopes.
- Directional float move clears before dependent half-snap constraints, matching
  the native proven-clear continuation. A constrained half-snap refusal originally
  only echoed the base without actuation; failing-first coverage now requires its
  existing one-shot base-restore token. Successful half-snaps keep exactly their
  own token, with no native clear. Unavailable geometry remains a preflight under
  the tentative choice below, not an automatically deferred clear.
- Fifty-five focused composed regressions pass, including ordinary empty-output
  parity, constrained base restoration and unreadable-work-area recovery. Fresh
  independent review and its directional follow-up have no outstanding finding.
  Synthetic native
  boolean fixtures declare maximize support in core and protocol test helpers;
  production capability defaults and immutable wire fixtures are unchanged.
- Final composed-increment verification passes the exact filtered core/protocol
  gate (1489 core unit tests, every selected integration target, protocol suites
  49/4/15/8/2/54/4), core/protocol build, scoped all-target clippy with warnings
  denied, formatting, diff whitespace, added/new-file ASCII and 274 local link
  paths across this note and the reference outcomes. Only the previously recorded
  workspace unsafe-posture guardrail is filtered. No adapter or immutable fixture
  changes, native delivery, full scenario execution or baseline float-geometry
  repair are included. The tentative choice below still awaits user review.
- Accepted-report command rejection is implemented and offline-verified.
  The command boundary restores a post-observation pre-command image on refusal,
  publishes the existing typed rejected result and consumes the command sequence.
  Known command kinds retain their catalog spelling; legacy command-local details
  normalize to operation-refused without extending the catalog. Creation-counter
  exhaustion remains a protocol-envelope failure with no candidate adoption.
  Six focused regressions cover new admissions and actual closure, issuing-only
  results, consumed/duplicate replay, frame-attempt incorporation, real post-create
  send refusal rollback against a commandless control, and exhaustion watermarks.
  Existing command-refusal tests now expect accepted typed results and use fresh
  sequences for genuine later presses; envelope validation oracles stay unchanged.
  Failing-first tests reproduced the prior whole-report rejection for unknown
  focus and a legacy local-operation refusal. Independent review found no
  production defect; its native-sticky cap-oracle weakening was corrected with
  an equal-observation commandless control, matching the maximize/composed-sticky
  cap tests. Stale envelope-test prose and unused imports were also corrected.
  The exact filtered core/protocol gate passes (1495 core unit tests, every
  selected integration target and protocol suites 49/4/15/8/2/54/4), with only
  workspace_unsafe_posture filtered. Core/protocol build and scoped all-target
  clippy with warnings denied pass. Initial formatting checks found layout-only
  drift in migrated tests; narrow reconciliation resolves it. Final formatting,
  diff whitespace, added/new-file ASCII and 274 local link-path checks pass.
  No adapter, immutable fixture, protocol source, wire shape/version or approved policy
  changes are included. Full codec/scenario and native acceptance remain pending;
  all eight OPEN/TBD gates remain unchanged.
- Required focusFeedback ingress is implemented with typed raw focus rows and
  exclusive native/output arrival brackets. Existing nullable visibility samples,
  setter/reason catalogs and native workspace references are reused. Missing/null
  lanes, malformed shapes and invalid ids/counters/geometry refuse; readable-shape
  nulls and mismatched facts remain evidence, not readiness verdicts. The engine
  checks the arrival arm against handshake-owned host capability before adoption,
  even for a null home/output; the context-free decoder does not infer that
  capability from native observations. Repeated evidence rows remain permitted.
- A failing-first nested immutable-fixture regression reproduced unknown-field
  refusal while its otherwise-identical staged envelope without focusFeedback
  decoded. The lane is now required, including empty arrays. Core/protocol
  constructors and synthetic request envelopes carry it explicitly; no tolerant
  backfill or immutable fixture edit was introduced. Bounds cover 1024 feedback
  rows and 10000 arrival brackets across the whole report, including split rows.
  Digest coverage includes raw fields, nulls, arm and row/bracket order; existing
  full typed equality remains authoritative for duplicate/reuse detection. The
  required empty lane adds 19 bytes to request accounting; nested marginal costs
  match immutable compact rows and independent output/native literal witnesses.
- Eight focused core regressions and eleven protocol ingress tests pass. Coverage
  includes all four focus-feedback and seven valid arrival fixture rows, eight
  structural arrival refusals, both wrong-host fixture contexts, global bounds,
  request reuse/duplicate caching and no authority from uncorrelated evidence.
  Fresh independent review found no production defect. Its test-only refinements
  now directly swap distinct rows/brackets, span the bracket cap across rows,
  check protocol-level refusal non-mutation and pin existing shared-helper error
  vocabulary without changing other lanes. Initial compile checks found an
  estimator-name collision and omitted test/example constructor fields; narrow
  mechanical corrections resolved both. No semantic workaround was needed.
- Final focus-ingress verification passes the exact filtered core/protocol gate
  (1503 core unit tests, every selected integration target and protocol suites
  49/11/4/15/8/2/54/4), core/protocol build and scoped all-target clippy with warnings
  denied. Only workspace_unsafe_posture remains filtered. Initial formatting
  drift in three changed source files was reconciled with layout-only patches;
  the final formatting, diff whitespace, added/new-file ASCII and 274 local
  link-path checks pass. No adapter, immutable fixture, wire-version
  or approved-policy changes are included. This is nested-row ingress through
  the staged envelope, not full v4 message decoding, focus settlement or selection
  completion. Focus/effect correlation, switchSettled, general visibility targets
  and native-free selection remain dependent work; all eight OPEN/TBD gates stay
  unchanged.

- Typed effect correlation is implemented: OneShotEffect now carries the closed
  cause catalog, nullable quoted command sequence and awaitShown. Explicit focus
  construction, admission and cursor-follow producers populate those fields;
  correlation checks kind/cause/sequence rather than id spelling. Existing effect
  identities and focus behavior remain unchanged. Every engine producer emits
  awaitShown=false; a manually constructed true fixture witness proves encoding
  only, not an arrival-ready selection grant.
- A failing-first real cross-output focus regression reproduced absent cause
  metadata after confirming the effect and both writers' equality. Both writers
  now match all five immutable focus/raise/stack/admission/cursor-follow effect
  row bytes in frozen key order. Producer checks, opaque-id correlation and
  misleading-prefix rejection, consumed-command behavior and exact lane-size
  deltas pass. Full-reply equality was an incorrect test oracle because other
  existing lanes retain conservative slack; effect-lane marginal accounting is
  exact and the full estimate remains a bound. An initially unhomed admission
  test setup was corrected before accepting its producer evidence.
- Pre-mutation sizing now reserves 779 bytes for at most two focus effects.
  Encoder witnesses establish the actual row maxima: cursor follow 389,
  admission 387 and explicit command 294. Only admission plus explicit command
  coexist (682 including their comma); two maximum rows plus a comma conservatively
  cover that pair, and cursor follow never becomes a third effect. Review corrected
  a synthetic correlation-invalid maximum and the cursor prefix/token widths;
  the final witnesses use real producer field combinations. Boundary coverage
  isolates the effect reserve, verifies rejection before revision/highwater
  mutation and permits a smaller same-sequence retry. Fresh independent review
  and its sizing follow-up have no outstanding finding in this slice.
- Final effect-vocabulary verification passes the exact filtered core/protocol
  gate (1510 core unit tests, every selected integration target and protocol suites
  49/11/4/15/8/2/58/4), core/protocol build, scoped all-target clippy with warnings
  denied and formatting. Diff whitespace, added ASCII and 274 local link-path
  checks pass. Only workspace_unsafe_posture remains filtered. No
  adapter, immutable fixture, version or approved-policy changes are included.
  Focus settlement, general visibility targets, explicit-selection readiness,
  native-free selection completion and full reply decoding remain pending; all
  eight OPEN/TBD gates are unchanged.
- Existing cutover gap found while deriving widths: admission/cursor-follow ids
  in world_model.rs/world_cursor.rs prepend af-/ffc- to the full WindowId. At the
  supported 128-byte window-id limit they become 131/132 bytes, beyond the effect-id
  bound. Baseline source and an isolated width/grammar probe establish that this
  predates typed correlation; identity generation is unchanged here. A bounded
  identity mapping preserving the spent-effect fence is still required before
  v4 cutover. This is not full wire conformance or a new failure of the metadata
  slice, and the probe is not retained as a passing conformance test.
- Ordinary focus settlement now retains the actual latest published focus batch,
  preserving the original opaque effect tuple across observation-only revisions.
  The first matching feedback row latches its complete facts and original stamp;
  later contradictory rows cannot upgrade or replace the historical verdict.
  Proven foreground requires foregroundProven plus post equality, independent of
  setter success. Refused requires a refused setter and equal readable non-target
  pre/post foreground; all other attempts remain unproven. Arrival brackets and
  current foreground observations confer no historical ordinary-focus verdict.
  No verdict changes accepted-choice memory or emits effects, retries, hides,
  transfer events or switch completion.
- Twenty focused regressions cover real explicit, admission and cursor-follow
  producers, the full two-effect batch, opaque tuple mismatches, delayed feedback,
  original-stamp/first-row retention, proof/refusal/unknown combinations, held
  presence, close/reuse, supersession, consumed commands, command-only rollback,
  envelope non-adoption and duplicate replay. The initial missing-ledger test
  expressed a compile-time gap, not a behavioral red. Review found a positional
  prune iterator that short-circuit liveness checks could desynchronize. A
  mixed-live/departed direct ledger probe failed before the keyed-set correction
  and passes after it; the seeded departure probes do not establish a native
  lifetime outcome. The production two-effect bound is asserted without silently
  truncating emitted grants. Review findings are resolved.
- Final ordinary-focus verification passes the exact filtered core/protocol gate
  (1530 core unit tests, all selected integration targets and protocol suites
  49/11/4/15/8/2/58/4), core/protocol build, scoped all-target clippy with warnings
  denied and formatting. Whitespace, added/new-file ASCII and local link-path
  checks pass. Only workspace_unsafe_posture is filtered. This is an internal
  ordinary-focus prerequisite, not awaitShown readiness, frozen switch accounting,
  general visibility targets, native-free selection completion or full v4 codec
  conformance. The overlong automatic-effect identity gap and all eight OPEN/TBD
  fixture gates remain unchanged.
- The automatic-effect identity bound gap is now closed. Admission and hover
  producers use compact af-/ffc- tokens interned per live lifetime from the first
  automatic candidate's issuing revision; explicit fx- identities are unchanged.
  Tokens persist across observations, hover transitions, holds, exclusions and
  incomplete gaps without renewing same-lifetime adapter spent identity. Close
  or proven departure retires the mapping; reused lifetime ids get fresh tokens.
  One automatic candidate per report and increasing world revisions provide exact
  distinct tokens, without hashing, truncation or an additional counter. A
  candidate suppressed by same-target explicit focus can allocate a token without
  publishing an automatic effect; a later hover reuses that token safely.
- Both 128-byte-subject admission and hover regressions failed with the former
  131/132-byte effect ids before correction. Twelve real-producer regressions now
  cover valid compact ids, maximum-subject feedback correlation, distinct subjects
  sharing a long prefix, repeated hover identity, held/incomplete/known-lifetime
  retention, close/reuse and stale evidence, deduplication, request replay,
  command-only rollback, envelope non-adoption and revision exhaustion. Exclusion
  pruning has a separate direct fence probe; it is not native lifetime evidence.
  Added protocol tests establish maximum-width compact feedback decoding and
  matching reply-writer bytes. Review corrected candidate/publication wording and
  made synthetic coexistence witnesses use distinct maximum-length subjects,
  matching the real same-target dedup rule; no review finding remains open.
- Compact encoder witnesses establish explicit/admission/hover maxima of
  294/279/281 bytes. The widest admission-plus-explicit pair is 574 including
  its comma, covered by the conservative 589-byte pre-mutation effect reserve.
  Marginal lane sizes remain exact; whole-reply sizing remains a bound. Final
  verification passes the exact filtered core/protocol gate (1542 core unit tests,
  all selected integration targets and protocol suites 49/12/4/15/8/2/59/4), build,
  scoped all-target clippy with warnings denied, formatting, whitespace, ASCII and
  local link-path checks. Only workspace_unsafe_posture is filtered. No adapter,
  immutable fixture or version changes are included. Visibility targets, frozen
  arrival-ready selection and switch completion remain pending; all eight
  OPEN/TBD fixture gates are unchanged.

## Tentative decisions pending user review

- Case: a native cursor hit names a window moved to a capability-conflict hold
  during v4 grading. Chosen internal behavior: preserve the original hit in the
  staged raw observation, but clear that newly ineligible hit in the effective
  classified report before its structural recheck. A held subject grants no
  hover authority; no geometric fallback or replacement target is selected.
  Evidence: the contract's raw native-hit and narrow conflicting-row hold rules,
  the existing hover consumer's requirement for a classified fact, and the
  focused staging regression, which reproduces the conflicting fact surviving
  when normalized cursor validation discards grading. The wire validator remains
  unchanged; malformed original cursor/reference combinations must not become
  acceptable through normalization. Alternative: retain the hit in the effective
  report and bypass its cursor validation. Not selected because that would give
  the normalized report a second structural-validity convention. This internal
  normalization choice is tentative, not an approved contract change.
- Case: a composed-maximized float receives directional move after its screen or
  work area becomes unavailable, or half-snap geometry cannot be computed.
  Chosen behavior: retain the bit, emit the existing directional refusal, mint no
  placement token and perform no deferred clear on recovery; a fresh move may
  clear. Once geometry resolves, the local clear precedes ordinary constraints;
  a constraint refusal restores the retained base with one authorized token.
  Evidence: the approved unavailable-frame-authority refusal and unchanged safety
  gates in the [world contract](../porting/world-contract.md#window-state-policy),
  plus native clear-before-ordinary continuation in `world_clear.rs` and the
  Tier 1 [R-MAX-08](../spec/reference-outcomes/maximize-fullscreen.md#r-max-08-focus-and-move-while-maximized)
  COSMIC clear-then-ordinary model. That source does not establish unavailable
  external frame authority. Native state clear can restore without a core-computed
  frame, whereas composition
  cannot authorize a frame on an unknown scope. The readable-to-unreadable-to-
  recovered regression passes; exact native/reference timing remains TBD.
  Alternatives: clear the bit without an authorized base restore, or retain a
  restore token for later authority recovery. Neither alternative was selected;
  the former leaves the old overlay pixels, the latter adds deferred actuation.
  This is not promoted to an approved decision.
- Case: anchored border descriptor values for the real producer. Chosen behavior:
  `width` and `radius` are the border settings scaled once by the foreground
  output's scale with the existing nearest, halves-upward rule; `outsets` is
  `[g+w, g+w, g+w, g+w]` from the scaled border gap and width (the v3 outer ring
  distance); an unknown or absent witness, or a witness whose subject or
  foreground does not join the observed foreground, hides the border with
  no reason, null subject, outsets, width, radius and witness (the existing
  carried hidden geometry-veto spelling); no hide token is added. Evidence:
  contract presentation text (core-scaled style, `unknown disables border`,
  core-computed outsets), the approved size-scaling and anchored-border
  decisions, the v3 border geometry (`visual.rs` `border_geometry`: frame, then
  gap, then width) and the `anchored-border` scenario (`borderGap` 0, width 3,
  outsets all 3). That scenario cannot tell gap-plus-width from width alone
  because the gap is 0. Alternatives: outsets equal to the scaled width only, or
  a new hide reason for unknown witness; neither selected. This is not promoted
  to an approved decision.
- Case: fixed diagnostic and sizing choices in the staged v4 twins, not yet wired.
  Chosen behavior: the v4 report gate reuses the legacy reason tokens where the
  check is the same and adds `renderer-target-invalid`,
  `presentation-foreground-invalid` and `native-invalid` (held row with a bad id);
  a renderer input without a command is `command-pair-invalid`; pager order and
  pager frames share the 1024 presentation cap the decoder uses; the report
  estimate counts wire-optional lanes as present, so it may overcount but never
  undercount; the reply upper bound raises two reserves for the larger v4 shapes
  (retained transfer row 512 to 544, border 256 to 704). Evidence: the decoder's
  own caps and spellings, the staged reply writer's byte lengths, and the
  fixtures. Alternatives: reuse legacy tokens only, or keep the legacy reserves
  and rely on the transactional exact check. Neither selected; the tokens and
  reserves can change at wiring time without wire effect. This is not promoted to
  an approved decision.
- Case: diagnostic assembly in the unwired v4 staging intake. Chosen behavior:
  append held diagnostics in screen, window, native, attempt, presentation lane
  order, preserving order within each lane. Unsupported typed state-attempt
  diagnostics follow held-attempt diagnostics, in state-attempt input order,
  before presentation; both routes share carried-window/held-window/retained-home
  scope resolution. Held attempt/presentation rows with
  no diagnostic retain their raw row but emit no diagnostic with an invented
  scope. Held windows reuse legacy resolution from their carried output or
  retained home; unresolved rows keep no diagnostic. Evidence: typed held-row
  vocabulary, legacy window-scope behavior, four staging regressions and the
  accepted-fixture/scenario census. Alternative: interleave diagnostics by a
  global row order, or invent a fallback output for scopeless rows. Neither is
  selected; the report has no cross-lane ordinal and scopeless evidence grants
  no scope authority. This is not promoted to an approved decision.
- Case: unbound native-state capability defaults in core staging. Chosen
  behavior: absent capabilities mean unsupported for sticky/maximize grading
  and classification as well as all state-attempt support checks. Evidence:
  `state_property_supported(None, _)` already returns false and
  `HostCapabilities::default` declares no native state support. A bound
  connection still uses its exact capability flags; public unbound reports
  fail the owner gate before grading. Direct grading and classification
  regressions pin absent, unsupported and supported bindings separately.
  Alternative: keep the permissive grading/classification defaults or make
  attempt support permissive too. Neither selected; both would preserve or
  expand unsupported native-state assumptions. This is not promoted to an
  approved decision.

## Adapter compile-failure inventory

- Current Linux-host `cargo check -p tiler-macos --all-targets --message-format
  short` stops in the library with 65 errors and 12 warnings. Reported categories:
  removed report/settings/capability fields in `world_builder.rs`, removed raw
  window verdicts/nullability/native-workspace types in `runtime_core.rs` and
  `world_builder.rs`, removed screen gap/outer/primary fields, retired
  `HomeThenCachedLive`, and retired parallel member/view native-reference fields.
  This is a host compile observation, not a native macOS build or an exhaustive
  all-target inventory: library failure blocks later targets.
- Intent constructor migration is additionally required by source inspection,
  not established as fresh compiler diagnostics behind those earlier failures:
  - macOS `WorldReport`: `drag_drop.rs`, `pager.rs`, `tile.rs`,
    `world_builder.rs`, `world_command_map.rs`, `world_dispatch.rs`.
  - macOS `WorldReply`: `world_dispatch.rs`, `world_sequencer.rs`.
  - Windows `WorldReport` under `tiling_sys`: `world_owner_tests.rs`,
    `world_sequencer.rs`, `world_tick.rs`.
  - Windows `WorldReply` under `tiling_sys`: `tests_pending_drop.rs`,
    `world_diff_tests.rs`, `world_native_tests.rs`, `world_route_tests.rs`,
    `world_tick.rs`.
  Production/fixture constructors need the new intent lane plus the earlier
  staged migrations, not placeholder updates presented as an adapter port.
  Native Windows and KDE compile-failure inventories remain pending.

## Outcome

- Connection foundations, settings state, typed authority scopes and workspace
  attempt-reference slices are implemented and tested. Engine connection/policy
  integration and bounded readiness are implemented and tested.
  Screen metadata ingress, narrow holds and gap/outer scaling are implemented
  and tested; physical reconnect identity and other metric scaling remain pending.
   Raw ordinary/dialog/sheet classification and own-process exclusion are
   consumed by the real engine, with held-row lifecycle and retained-intent
   regressions. Known-float unreadable-hint membership and dependent-action
   narrowing are implemented and tested; unselected special-kind admission stays
   TBD. The workspace unsafe guardrail failure above remains unresolved outside
   these changed sources.
  The bounded opaque intent-data codec and outer intent ingress/publication lanes
  are implemented and fixture-tested; lifetime-fenced non-sticky explicit-float/
   tile restoration and save publication are implemented and engine-tested.
   Semantic sticky restoration and capability-aware sticky fact authority are
    implemented and engine-tested. Outbound visibility/native-state target types,
    encoding, sizing and existing frame/clear command correlation are implemented;
    required StateAttempt ingress and internally bound native-state settlement are
     implemented and offline-tested. Composed sticky commands, canonical saves and
      same-output show targets are implemented and offline-tested; native sticky
       target production, proof-gated clear admission and exact-prior refusal
       rollback are implemented and offline-tested.
        Native fullscreen set/toggle target production, proof settlement and
        dynamic publication gates are implemented and offline-tested; native host
        flag retention remains TBD.
        Native maximize target production, explicit fullscreen replacement and
        newer-fullscreen pending/active holds are implemented and offline-tested;
        persistent above acquisition, owned release/suspension and external-clear
        latching are implemented and offline-tested.
     General no-intent clears, remaining visibility/state policy and host storage
     acceptance remain pending.
   The core-composed maximize increment is implemented and offline-verified,
   including ordinary empty-output float-drag parity; the directional preflight
   choice above remains tentative, and native outcomes remain TBD.
   Accepted-report command rejection now retains trustworthy observations while
   rolling back partial command mutations, consuming the sequence and publishing
   the existing typed rejected result. Envelope failures still adopt nothing.
   Required focusFeedback/arrival ingress is implemented and offline-tested with
   bounds, digest/size accounting and authoritative host-arm fencing; it grants
   no focus authority and does not complete visibility or selection settlement.
   Typed effect cause/command correlation and awaitShown encoding are implemented
   and offline-tested, with coexisting-effect reservation enforced before
   mutation. Compact lifetime-interned automatic-effect identities
   now satisfy the supported opaque-id bound without renewing hover spent identity.
   Latest-batch ordinary-focus feedback correlation and one-shot historical
   verdict settlement are implemented and offline-tested.
   Focus-bearing native-free suppression-capable selections now integrate stable
   visibility targets, frozen arrival sets and awaitShown feedback readiness.
   Native-free switch target accounting, required terminal event publication and
    selector-flight completion are implemented for that route. Focusless completion
    now uses the same accounting rule, including issuing zero-member native-free
    events and native-owned focusless selector proof. Native-owned focus-bearing
    arrival migration is implemented and offline-verified through Native-arm
    home/frame/lifetime proof and the common selection terminal path. The pre-existing
   held-output recovery rehome remains in the
   reconnect/rehome increment.
  The remaining world report/reply paths and version are still staged; no v4
  cutover or full scenario acceptance is claimed. Full adapter build-failure
  inventory remains pending.
