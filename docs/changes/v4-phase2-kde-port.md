# V4 phase 2: KDE port

## Goal and status

- K1 is the first vertical slice: a built KWin producer handshakes with the v4
  core, sends genuine v4 reports, consumes replies and tiles fixture windows.
- Status: connection, opaque persistence wiring, native-fact producer, exact
  v4 reply decoding and binding/ordinary apply have targeted offline evidence.
  Hidden-only presentation is checked offline; visible anchored presentation is
  deferred to focus feedback.
  A genuine producer/real-engine ordinary frame carrier now passes offline.
   Built script/service now tile and reflow native fixtures with the effect
   absent, including service-restart recovery. Full K1 acceptance remains open.
- The [phase-1 note](v4-phase1-core-protocol.md) stays active. Its working-tree
  core/protocol/service/broker cutover is the baseline, not work to redo here.
- Follow the [recorded order](../decisions/contract.md#contract-authority-and-v4-migration):
  handshake, report/reply, selection, arrival, focus feedback, state targets;
  then scaled metrics, physical reconnect/rehome, scenario runner and remaining
  v3 removal. K1 ends after handshake and the functioning report/reply loop.

## Scope and invariants

- Producer lifetime is the sole `adapter`; send a real v4 handshake through
  authenticated `DiscoverWorld`. Send separate revisioned settings through that
  same route, dispatched by `type` through `route_connection_request`.
- Only unbound state or `generation-mismatch` initiates a handshake. Bound wait
  deadlines re-arm the wait without resetting core. Every actual handshake
  starts fresh. Preserve correlation, source/clock/replay fences, sender-bound
  queue ownership and requester-only results.
- One v4 route and decode shape: no legacy conversion, broker alternative or
  generic-over-report engine. Do not alter immutable `test-fixtures/world-wire/v4`,
  Cargo features, ordering at decode, or `preserve_order`.
- Keep classification and policy in core. Report truthful native facts and
  capability declarations, including correlated visibility verdicts where
  consumed. Audit floating feedback so workspace-floating rows publish
  `floating:true` with empty origin, rather than reconstructing policy locally.
- Do not absorb later feature slices merely to preserve old behavior. If a
  later lane is essential to basic tiling, identify the smallest dependency and
  resolve its contract and ordering before implementing it.
- Legacy removal units 7-4/7-5, scaled keyboard/resize/drop-zone plumbing and
  other platform ports are outside K1.

## K1 increments

| Increment | Deliverable | Acceptance |
| --- | --- | --- |
| K1a: connection | Exact handshake/settings codecs and producer binding; fixed native capabilities; fresh generation reset; bound wait timeout re-arm | Strict TypeScript and service-route regressions, including settings readiness and stale generation. Connection alone is preparation, not a dogfoodable slice |
| K1b: report/reply | Native v4 fact capture and canonical producer encoding; exact reply decoding; basic frame application and required attempt feedback; retain transport safety without a legacy bridge | Producer bytes accepted by the real v4 codec; offline entry/capture/apply regressions; floating feedback and visibility correlation audited |
| K1c: producer route proof | Re-record the two deferred stage5 production-route tests using genuine v4 producer bytes and enable them without forcing ignored tests | Both enabled tests execute and pass; preserved scenarios retain their meaning. Never run the legacy `world_wire_fixtures` generator |
| K1d: integrated build | Scoped offline gates, clippy correction pass and independent contract/live-safety review, then isolated native fixture leg | Built KWin, built service and real core exchange handshake/settings/reports/replies and tile fixture windows; live wait/watchdog and service-restart recovery demonstrated; exact teardown verified |

- K1c belongs to K1 because K1b must produce genuine v4 bytes. If the existing
  stage5 path requires a deferred feature, stop and identify that dependency;
  do not silently weaken the fixture or claim deferred proof passes.
- Canonical producer escaping must be checked in the actual KWin JS engine,
  including JSON short escapes. Node behavior alone does not prove it.
- Prove `pid=-1` on a real internal window in nested KWin and enumerate which
  internal windows cross the adapter's normalWindow/dialog and resourceClass
  scope filters. A fixture-scoped report omitting internals is not token-exclusion
  proof: retain a separate read-only native inventory and exercise captured
  internal facts through the real classifier without actuating foreign windows.
- Retain a born-fullscreen exit recheck if relevant native data is produced;
  unsupported outcomes remain TBD in the existing reference-outcome records.

## Later dogfoodable slices

| Slice, in order | Incremental function and acceptance on top of the retained report/reply loop |
| --- | --- |
| Selection | Port v4 selection and its required acknowledgement/correlation; prove workspace selection while ordinary tiling continues |
| Arrival | Port native arrival evidence and settlement; prove a fixture transfer reaches its intended native home, without treating setter acceptance as arrival |
| Focus feedback | Port causal focus results and the native anchored-descriptor consumer; prove authorized focus and refusal observations without replaying one-shot effects, plus native border witness/lifetime joins and frame-event re-resolution |
| State targets | Port maximize/fullscreen/sticky/stacking targets and required attempts/recovery; prove native transitions while preserving base tiling and durable preimages |
| Scaled metrics | Apply the approved per-output metric contract once; recheck ordinary tiling and rendering across scale changes, without threading unrelated scale work |
| Physical reconnect/rehome | Supply physical output facts and prove unplug/reconnect/rehome while keeping the prior functions usable |
| Scenario runner | Exercise the genuine producer and ordered native scenarios without replacing native outcomes with transport receipts |
| Remaining v3 removal | Remove obsolete paths only after migrated-path evidence, retaining immutable fixtures and active migration records |

- Each slice from K1 onward must leave a build usable at its proven level;
  later slices add function without breaking the report/reply foundation.
- K1's intended dogfood level is ordinary fixture-proven tiling at the tested
  topology. Selection, arrival, focus-feedback and state-target completeness
  are not K1 claims. Multi-output, reconnect and scaled-display behavior remain
  weak until scaled metrics and rehome land. This is development use, not release
  acceptance. The actual usable surface must be stated after native verification.

## Verification plan

- Run sequentially through the declared Linux environment. From `kwin/`, use
  `npm run typecheck`, `npm run build` and `npm test` as defined in package.json.
  Build the Planner/service with `cargo build --locked -p omnitiler` through
  devenv. Any changed native component also needs its repository-defined build
  and relevant tests; an optional renderer must not become a tiling dependency.
- Core/service/protocol changes require the widened package-scoped gate in
  [phase 1](v4-phase1-core-protocol.md#unit-7-widened-verification-plan-not-executed),
  including proof that private-bus legs actually execute, tool transcripts,
  strict clippy, formatting, whitespace/ASCII/NUL and local-link checks. Never
  run `workspace_unsafe_posture`, the legacy fixture generator, ignored-test
  forcing or an unfiltered workspace suite. No whole-workspace pass is implied.
- Native tests follow [the live guide](../live-kwin-testing.md) and the
  [fixture reference](../../crates/tiler-live-fixture/README.md): durable
  default (user 2026-11-06) of fresh each-run inventory of occupied
  workspaces through n, tests on workspaces >n, fixture with exact platform
  filter, exact lifecycle receipts, evaluated readiness as cheap evidence,
  independent fixture/native joins and authoritative native frame readback.
  Nested isolation with private config/data/cache/state/runtime/HOME/bus
  applies only to crash-class legs (forced-crash/recovery, ABI-bound effect
  load). The baseline checker stays an optional unchanged tool.
- Prove ordinary admission/reflow and no repeated handshake during bound idle
  watchdog recovery; restart only the isolated service and prove generation
  mismatch leads to one fresh connection and renewed tiling. Record owned
  state evidence; no routine baseline/restoration proof.
- Separate byte acceptance, native state evidence, visual/physical input proof
  and untested conditions. Stop on KWin crash/crash-class surprise, repeated
  non-fixture actuation from filter failure, or a real product/contract decision.

## KDE process identity

- KDE supports `processId`. Own identity is fixed opaque Id `kwin-internal`;
  native window `pid` exactly `-1` maps to the same token. Positive integer
  client PIDs carry decimal Id text; zero, other non-positive or unreadable
  values remain null and retain the existing dependent-classification hold.
  The token satisfies the existing Id grammar without colliding with decimal
  client PIDs. No core/contract change is needed.
- At KWin v6.8.1 (`1b798c05c19b1124edb1e16623191fcd2c128fd0`)
  and v6.6.0 (`e53bb3be7ecf1735c7739b797f94d66de422ab4d`),
  InternalWindow inherits Window::pid returning -1; WaylandWindow and X11Window
  override it for client PIDs. Internal windows are clients in windowList and
  can be normal/resizable without an acceptsInput exclusion in core.
- Reject a D-Bus own-OS-PID lookup: that PID would match none of these internal
  windows, while mapping -1 to null would hold their rows. Reject declaring
  processId unavailable: own exclusion would be disabled and internal windows
  could classify as ordinary members. No asynchronous pre-handshake identity
  step, broker-PID substitution or optional-effect dependency is introduced.
- Executable identity is unavailable in the current script capture path:
  declare it unavailable and use null own/window executable, never substitute
  appId/resourceClass. Own appId remains null without a truthful native source.
- Native PID reads, actual candidate-boundary enumeration and token exclusion
  remain unproved; the
  [minimal broker discriminator](../spec/reference-outcomes.md#v4-broker-connection-discriminator)
  retains TBD native outcomes.

## Accepted evidence so far

- Before the producer edits, `devenv shell -- npm run typecheck --prefix kwin`
  passed and the separately bundled `world-connection.test.ts` passed 16/16.
  This is source/test typechecking and an in-memory connection harness, not
  service/native proof. The current checks below supersede that earlier tree.
  Script build, full adapter tests, widened Rust integration and live legs remain
  unrun.
- Connection review corrected unconditional per-output support at the 6.6 floor,
  unsupported suppression/above claims, asynchronous settings-readiness ordering,
  stale callback fencing, equal-content revision retries and bound reset triggers.
  These corrections are not native capability acceptance.
- The first targeted connection execution failed three entry assertions. One
  genuine stall settled a settings flight before reply correlation; correlation
  now precedes settlement. Two harness corrections drive the queued command and
  select the actual wait watchdog rather than the retired discovery timer. The
  unchanged acceptance assertions then passed in the 16/16 execution above.

## Persistence cutover

- `ReadFloatIntent`/`WriteFloatIntent` retain their method names with one
  version-2 opaque snapshot schema. The private runtime file in `XDG_RUNTIME_DIR`
  now stores `{win,data}` records rather than membership lists. Float/tile
  disjointness, the separate tile side and request-level live pruning are removed;
  the service never interprets core data.
- The approved v4 obligation is already settled: core alone defines/saves intent,
  adapters store and return exact opaque data before the first admissible report,
  and normal within-session restart must retain intent. This work is not asking
  to reopen that behavior or to defer it silently.
- Reject the old version-1 `members`/`tile` storage format at the single opaque
  cutover. It takes the existing degraded/empty path and the next write replaces
  it. No conversion, compatibility route or dual read exists. Explicit float/tile
  choices on windows open during the upgrade reset once; that consequence is
  accepted. Normal v4 within-session persistence, namespace,
  same-UID/current-KWin-owner authorization, private atomic file safety and exact
  live-window binding remain required.
- The adapter hydrates an opaque cache before dispatch, joins records to captured
  live subjects and persists accepted core intent writes; native sticky callbacks
  no longer write intent. The intent lane is required without backfill. Store
  replies require matching correlation, closed shapes and exact counts; seed
  failures empty the snapshot unless newer accepted runtime writes supersede the
  read. Stop cancels flights. Serial full-snapshot persistence gates dependent
  native apply on success and generation, settings, accepted-revision and native
  invalidation freshness. Failure leaves durability owed, including on a later
  unchanged acceptance. This does not establish integrated actuator migration.
- Targeted store tests passed 23/23; six private-bus service tests executed with
  captured method outcomes and no skip diagnostics. They establish actual
  old-format `degraded`/`reason=corrupt`, followed by replacement and an opaque
  reread, not merely parser compilation. The strengthened full `members`/`tile`
  old-file assertions subsequently passed both store and private-bus tests under
  `cargo test -q -p omnitiler old_v1_ -- --nocapture`, with actual degraded,
  stored and reread outcomes. Full widened gates remain pending.
- Before exact reply decoding, `npm run typecheck --prefix kwin` passed. Focused connection,
  store, entry-intent and producer bundles pass all 55 tests sequentially. These
  cover matching acknowledgements, degraded reseeding, cancelled timers, withheld
  native readbacks, failed-write durability, newer accepted revisions and changed
  settings. Entry replies remain transitional shapes, not real v4 service bytes;
  no codec/native acceptance follows from these passes.
- The first expanded check passed 47/54; after test-driving corrections it passed
  42/45 excluding the producer. New reseed tests had not advanced their transport
  generation, and queued-command tests omitted acknowledgements or still sent
  version 3. Matching v4 queue replies and explicit take settlement restored the
  original assertions. No production fence was weakened.
- The earlier maximum-size failure was an invalid native fake/oracle: KWin's
  unbounded maximum is `INT_MAX`, not zero (X11 `GeometryHints::maxSize`, Wayland
  maximum-size mapping and base `Window::maxSize` at the recorded pins).
  The producer preserves zero/sentinel hints verbatim; core sanitizes them at
  use. The fake now supplies the native sentinel and asserts its exact carriage.
- Native `Window::isShown` also depends on script-unreadable
  `hiddenByShowDesktop`. The producer no longer invents shown=true from just
  hidden/minimized: those predicates prove false, otherwise visibility is null.
  At fixed `hostOwned:true`, core selection freezes native selector obligations,
  not per-window LastWrite members (`world_selection.rs`); omission of unreadable
  native inventory does not change that connection capability. Confirmed
  visibility/focus samples are not thereby available for later feature slices.
  Non-host-owned suppression and correlated verdicts are not implemented, and
  the full reply/apply audit must preserve required accounting wherever consumed.

## Exact reply and native consumer integration

- The reply parser now consumes the actual v4 schema: structural authority
  scopes, visibility/state targets, command correlation, effect causes, anchored
  presentation, switch settlement and diagnostics. No old reply shape is
  accepted. Seven focused tests using immutable v4 message fixtures pass,
  including bound transport fields and malformed/missing-lane refusals.
  This is fixture-byte decoding, not genuine producer or native route proof.
- The subsequent typecheck failed on five source errors: one stale scope-string
  comparison and four accesses to retired absolute border geometry. Candidate
  binding/apply edits remove broker-incarnation binding, reset lifetime state on
  every authenticated handshake and compare workspace scopes structurally.
  Their fixture-backed tests subsequently passed all eight binding/apply cases.
  Existing test literals and discovery calls still need coordinated migration;
  earlier passing focused suites are no longer current-tree verification.
- The native presentation consumer still requires the exact absolute
  `border{visible,inner,outer}` DTO in `presentation_consumer.h`; v4 instead
  supplies `subject,outsets,width,radius,witness`. The immutable anchored-border
  scenario requires native frame-event re-resolution and foreground/lifetime
  vetoes. Script-side absolute geometry would omit those native obligations.
- K1 defers visible anchored-border rendering and the native anchored-descriptor
  consumer port to the focus-feedback slice. K1 shows no OmniTiler active border;
  KWin's own focus indication applies until then. Script-side absolute geometry
  is prohibited, and K1 makes no C++ DTO change. An optional loaded effect must
  remain honestly hidden without stale paint or fail-closed noise; tiling must
  not depend on it. Foreground/witness supply and all collateral presentation
  degradation still require explicit verification and reporting. Producer route,
  stage5, independent review, widened gates and isolated live proof remain open.
- The hidden-only formatter now accepts valid visible anchors as intentionally
  disabled border payloads, not malformed traffic. Three offline formatter and
  subscriber tests pass: no drawable border, existing paint replaced by hidden
  state, repeated revision does not repaint or generate malformed diagnostics.
  Group underlay and drag-preview forwarding retain their independent coverage
  and producer gates; their live availability is not established. The existing
  native presentation endpoint has no pager surface.
- K1 does carry a foreground that could satisfy the foreground part of a witness
  join: native capture publishes the known foreground and the v4 encoder preserves
  it. The geometry witness itself is always null; the separate broker
  `coveredInstance`/`coveredInvalid` chain does not fill that geometry witness.
  This does not reverse the visible-border deferral.
- The first full adapter gate built the script successfully, then passed 524 of
  948 tests with 424 failures and no skips. Many failures stop at stale v3
  discovery, reply or recorded fixture shapes; they are not integrated v4 proof.
  Seven typed test files have been migrated, with subsequent local annotation
  corrections now typecheck. Do not infer a current full-suite pass. The shared
  real-engine transport now retains the producer adapter,
  stamps no retired `owner` and strips only broker capture-only lanes without
  fact conversion. A new ordinary frame carrier passes authentic handshake and
  revisioned settings, strict real-engine decoding, frame application/readback,
  external perturbation recovery and converged silence. Persistence is disabled
  in that carrier; it is not D-Bus or native evidence.
- The carrier exposed a producer defect after its first frame write: ordinary
  attempts correctly use `op:""`, but the encoder rejected that permitted value
  and prevented follow-up reports. The encoder now accepts empty-or-Id operation
  fields with nullable pre/readback and rejects pre without an operation. All 18
  producer tests and the carrier pass; the accompanying raw-attempt suite passes
  20/22, with two entry cases still stopping at stale connection harnesses.
  Typecheck also passes after the encoder and test corrections.
- Native proof is prioritized after the shared transport and one genuine carrier,
  before the remaining stale test corpus migration. Live-guide preflight, safety
  review and built scoped artifacts remain prerequisites; nested isolation
  applies only to crash-class legs. Corpus migration and all final gates remain K1 acceptance work.
- Native preflight note (superseded 2026-11-06): the baseline checker is
  host-specific and stays an optional unchanged tool, not a per-leg gate. K1
  live legs use the durable default: fresh each-run inventory of occupied
  workspaces through n, tests on workspaces >n, fixture with exact platform
  filter, and authoritative native readback.

## First native ordinary-tiling evidence

- A fresh native workspace inventory determined the highest occupied ordinal,
  counting observed all-desktop windows across existing desktops. Fixture tests
  used a newly appended desktop beyond it and the exact fixture class filter.
  The optional effect was absent throughout; no native effect was loaded.
- The built service and ES2017 script exchanged correlated real v4 handshake,
  revisioned accepted settings, reports and replies. Cursor-bounded journal
  readiness/discovery/dispatch/applied evidence appeared under the native KWin
  PID. One fixture filled its tile; adding a second produced side-by-side native
  frames. Fixture sentinel/PID joins and socket client-size readbacks agreed with
  independent KWin UUID/frame/client observations.
- A bounded pause/resume of the verified development service exercised the bound
  wait host deadline: it logged `recovery=rearm` without a new handshake. A
  subsequent graceful service replacement, without reloading the script, produced
  generation-mismatch, one new handshake, accepted settings and renewed tiling.
  A third fixture reflowed the layout; closing it returned the two survivors to
  side-by-side frames. Fractional native client/frame quantization differed by
  0.4 logical units from integer targets in the three-window layout; this is not
  scaled-metric or exact asynchronous-settlement acceptance.
- Actual KWin `JSON.stringify` uses short control escapes for backspace, tab,
  newline, form feed and carriage return, plus lowercase Unicode escapes for NUL
  and the tested remaining control character. The custom producer writer still
  needs a live control-string carriage check, not just this native behavior probe.
- Limitations exposed: startup presentation read returned not-ready and was
  logged as malformed; later reads logged stale coverage. Missing optional
  effect/tray endpoints produced D-Bus diagnostics and repeated effect/gesture
  unknown-state refreshes. These did not prevent tiling, but noise-free loaded
  hidden-effect behavior is not proved. Intent seed reads degraded as corrupt;
  this leg does not prove nonempty opaque-intent restart hydration. No internal
  pid=-1 window appeared in the retained inventories, so token/filter-boundary
  exclusion remains unproved.
- The startup presentation error was an adapter codec defect: genuine bound v4
  rejections include `gestureAssoc:null`. The strict decoder now requires that
  field, and the subscriber distinguishes bound not-ready from malformed replies
  while retaining fail-closed hides for unexpected or foreign rejections.
  Repeated unknown effect/gesture facts no longer invalidate idle captures;
  known-to-unknown transitions still publish once, and host-deadline recovery,
  late-callback fences and enabled-later adoption remain intact. Typecheck and
  all 50 focused subscriber, hidden-border, protocol-state, facts-transition and
  real-engine carrier tests pass. Native reverification is pending; absent
  optional endpoints still produce bounded KWin D-Bus transport diagnostics.
- A targeted built-script native leg now verifies the rejection and idle fixes:
  startup logs not-ready rather than malformed, and an observed interval longer
  than two facts host deadlines contains no idle capture or repeated handshake.
  Optional endpoint calls still retry at the bounded cadence; their native D-Bus
  errors are not a noise-free claim. A fixture title carried native backspace
  and unit-separator facts through the actual KWin producer as canonical
  `\u0008` and `\u001f`, with escaped quote/backslash and correlated accepted
  service replies. Native title readback agrees; KWin normalized the requested
  tab/form-feed pair, so those characters are not claimed as carried. A fixture
  resize triggered the fresh capture and reconverged to its native tile.
  Exact fixture/controller/service cleanup and original-view selection passed.
- The migrated entry-intent harness now uses strict v4 reply shapes and verifies
  authored accepts with the production parser. All 63 focused connection, store,
  entry-persistence and producer tests pass, including the 12 durability and
  stale-apply cases. This does not substitute for nonempty native restart proof.
- A new offline recorder drives the actual entry through a retained production
  broker bridge, including handshake/settings and presentation reads. Both
  stage5 causal chains record successfully with coverage joins, found underlay,
  required preview provenance, hidden border and native-open frame barriers.
  Genesis and intermediate captures are retained rather than folded away.
  The first execution exposed a false constant gesture-active fake: removing
  it restores the production snapshot-derived barrier without changing the
   oracle. Typecheck and both chains pass after that correction. The v4
   counterparts now retain all genesis/intermediate captures with gen-only byte
   normalization. Both deferred Rust route tests are enabled and all three
   route/witness tests pass against the production service, including exact
   projection bytes, preview provenance, underlay, coverage and frame barriers.
   Initial replay exposed a fresh-clock/recorded-attempt mismatch and an
   intermediate-to-final witness mismatch; bounded clock settlement and correct
   predecessor-facts joins resolve those without changing fixture bytes or
   production behavior. Subscriber fixture migration remains pending. This is
   not native rendering or D-Bus authorization evidence.
- Teardown unloaded the exact controller, stopped the development service and
  shut down the fixture cleanly. Follow-up checks found no fixture process,
  socket, native windows or script helpers; development status was down and the
  original native view was selected. The appended desktop and stale project
  shortcut records were left as ordinary lifecycle residue. No crash or repeated
  non-fixture actuation was observed. Remaining corpus migration, stage5 producer
   subscriber migration, loaded-effect nested proof, independent review and widened gates
  are still required before K1 acceptance.
