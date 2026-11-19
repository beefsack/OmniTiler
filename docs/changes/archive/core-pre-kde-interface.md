# Core interface before the KDE rebuild

**Approved (User 2026-11-19); complete offline.** The supported v4
report/command interface is ready for the KDE rebuild. Settings
prepare-before-commit remains deferred (B3). One logical world owner
processes ordered commands and returns one full desired world per admitted
capture, with separate requester results. Incorporating facts, applying
native setters and proving settlement remain different operations. No native
acceptance is claimed here.

The design sections below retain their pre-change source findings for context.
Current rules are in the [world contract](../../porting/world-contract.md).

## A5. One current contract

- **Current (source-verified):** `docs/principles.md:18-21`,
  `docs/porting/world-contract.md:3-14,36-60` and
  `docs/decisions/contract.md:18-35` still describe v3 authority or old port
  statuses. The current decoder requires v4 and the unified `propertyAttempts`
  lane (`crates/tiler-protocol/src/world_protocol.rs:3942-3999`); the broker
  requires v4 in both capture layers, stamps metadata with null inline
  command fields, and delivers results separately
  (`crates/omnitiler/src/world_broker.rs:1148-1167,1206-1244,1704-1788`).
  KWin emits `propertyAttempts` (`kwin/src/world-report.ts:3217-3243`);
  emitting the lane offline does not prove native visibility evidence or apply.
- **Decision (User 2026-11-19, A):** Amended v4 is normative now for
  supported consumers, with the per-host applicability table below. Frozen
  contracts are historical; no dual parsing, shim or old v4 fallback.
- **Rejected:** The all-port authority gate leaves no buildable KDE baseline.
- **Wire:** No new spelling from A5 itself. The docs describe what the active
  wire already accepts, not an assertion that all service endpoints are
  equally strict. Capture and discovery reject duplicates; queue, take,
  wait and presentation-read are the separate KDE endpoint work
  (`docs/changes/archive/core-wire-boundary.md:351-354`). Document the
  exceptions until KDE replaces them. Point the authority decision and
  principles at the current contract; frozen contracts and vectors are history.
- **Discriminator:** Active Discover/capture/result and Stage 5 routes agree
  with the table, including refusal. A v3 message and a frozen v4 report
  missing the required attempts lane both refuse; test their actual tokens
  without changing existing validation precedence. Show an equivalent typed
  transition; do not infer native proof from fixtures.

Applicability table for `docs/porting/world-contract.md`; link the
[handshake](../../porting/world-contract.md#handshake-and-lifetime),
[report fields](../../porting/world-contract.md#worldreport),
[result](../../porting/world-contract.md#commands-and-results) and
[apply/recovery](../../porting/world-contract.md#freshness-and-recovery) rules:

| Route / host | Producer | Actuator | Results and retry | Capability and evidence |
| --- | --- | --- | --- | --- |
| Shared v4 baseline | Bound handshake with capabilities (`sharedNativeRoster` required on wire), revisioned settings, fresh correlated observations; screen/window inventory and completeness, `closed` as positive evidence, `foregroundKnown`, unified `propertyAttempts` (empty when none), separate focus feedback and creation proof | Apply only latest eligible desired state within authority, subject/lifetime, generation and freshness fences; report exact attempts or real no-calls and independent readbacks; never choose substitute targets | Queue ACK is filing only; disposition is separate from world. Retry by the route's identity; capture incorporation does not imply native apply or command execution | Capabilities, not port names, choose selector/roster, above, native sticky, warp and unavailable-fact behavior; no unsupported setters |
| KDE / broker | V4 envelope `v,adapter,gen,source,sourceSeq,nativeInvalid,report`; broker stamps inner correlation, clock and sequences; reports carry unified properties, not inline commands | Apply only eligible bound capture; renderer projection confers no setter authority; prove visibility feedback production in rebuild | Exact remembered source bytes replay before drain; requester-only Take, Wait is wakeup; retry unremembered rejection with a fresh source | Implemented offline; earlier native state-target slice only (`docs/decisions/contract.md:21-22`), not proof of this interface. Shared native roster, capability-gated selector; no KDE warp |
| Typed shared route | Same facts and bound connection using typed values; caller-owned ordered commands; no wire stamps manufactured for JSON | Prepared typed publication and explicit incorporation; same outbound semantics, independent latest-only apply fence | Typed report-only replay with no new command execution; returned per-sequence outcomes, caller owns issuer delivery | Shared implementation to be proved offline here; not a native port |
| macOS | Deferred; build the typed route and prove native observations/receipts on port entry | Same obligations; prove setters and recovery locally | Preserve per-issuer outcomes and in-flight evidence | Deferred; no shim or build accommodation |
| Windows | Deferred; build the typed route and prove capability declarations on port entry | Same obligations; retained warp before advertising it | Preserve per-issuer outcomes and in-flight evidence | Deferred; no shim or build accommodation |

## A4. One production transition

- **Current (source-verified):** `WorldEngine::handle_report` documents a
  sole, index-aligned entry but is structural-only, as is
  `handle_report_with_prepare`; the public gated entry takes a fallible callback
  (`crates/tiler-core/src/world_engine.rs:859-943`). Both protocol `checked`
  helpers use a no-op final prepare and count without validating
  (`crates/tiler-protocol/src/world_admission.rs:83-123,182-209`;
  `crates/tiler-protocol/src/world_reply_write.rs:1353-1410`). The broker
  already prepares bound bytes and projection before adoption and keys drained
  results by sequence (`crates/omnitiler/src/world_broker.rs:900-968,1014-1125,
  1206-1259`). There is no index alignment: inline runs first and may return a
  result with no queued commands; bootstrap and exact replay run none. A fresh
  consumed inline command reconciles but suppresses the whole queued list;
  an already-consumed queued item instead returns `None`, indistinguishable
  from executed without payload. There is no whole-list order fence before
  moving high-water (`crates/tiler-core/src/world_admit_v4.rs:487-566,679-737`;
  `crates/tiler-core/src/world_run_v4.rs:507-541,583-629,683-746`).

  Current callers and cutover:

  | Route | Current entry / disposition |
  | --- | --- |
  | Engine | `handle_report`, `handle_report_with_prepare`, `handle_report_with_gate_and_prepare` (`crates/tiler-core/src/world_engine.rs:875-943`) funnel into the v4 runner (`crates/tiler-core/src/world_admit_v4.rs:280-313`). Retire structural production access. |
  | Protocol | `handle_report_checked` and `handle_report_decoded_checked` (`crates/tiler-protocol/src/world_admission.rs:94-123`) are size-only. Move to the single transition; decoded wire calls must preserve residuals. |
  | Broker / service | `WorldBroker::dispatch_capture` (`crates/omnitiler/src/world_broker.rs:906-968`), reached via `WorldService::evaluate_capture` and Planner D-Bus (`crates/omnitiler/src/world_service.rs:214-238`; `crates/omnitiler/src/planner_service.rs:465-490`). `crates/omnitiler/examples/broker_world_eval.rs:94-103` uses the service. |
  | Raw wire evaluators | `crates/tiler-protocol/examples/world_eval.rs:42-63` uses structural admission and drops residual/dispositions; `crates/omnitiler/examples/raw_world_eval.rs:52-92` keeps residual and sizes but runs validation only after adoption. Both must prepare before commit. |
  | Acceptance / benchmarks | `crates/tiler-core/examples/report_scale.rs:493,537,595,626,764` and `crates/tiler-protocol/examples/world_acceptance.rs:482,565,1269,1291` use the structural entry. `crates/tiler-protocol/src/world_stress_v4.rs:271,757,1403,1432,1454,1518`, via `crates/tiler-protocol/examples/world_stress_acceptance.rs`, uses the size-only helper. Keep structural tests and core-only cost legs explicitly non-production; acceptance claiming a consumer contract must use prepared admission. |
  | Deferred | `crates/tiler-macos/src/world_dispatch.rs:119-179` and `crates/tiler-windows/src/tiling_sys/world_sequencer.rs:74-87,450-485`; review inventory only, not built or reverified here. No compatibility work. |

  The typed structural check currently validates only `report.command`
  (`crates/tiler-core/src/world_report_v4_checks.rs:197-215`); the codec
  rejects removed `SwitchView` as `command-invalid`
  (`crates/tiler-protocol/src/world_protocol.rs:6270`).

- **Decision (User 2026-11-19, A):** One mandatory prepared transition and
  one ordered list. Normalize the wire inline pair to an explicitly
  proof-bearing first entry. Keep that pair, but not the caller-owned queue,
  in report replay identity (C5 D2). A stale entry reports `AlreadyConsumed`;
  stale inline does not suppress fresh queued work. A fresh ready batch with
  a duplicate or descending sequence refuses before incorporation as
  `snapshot-invalid` / `command-order-invalid`; do not sort. No field changes.
  Preserve the first-entry routing vintage: only the first submitted entry
  can use the frozen pre-observation send/migration source; a later command
  resolves after incorporation and prior commands, even if the first entry
  is consumed or refused. Preserve the wire `consumed` bit (Rust
  `command_consumed`) as inline-only evidence, not a batch outcome.
- **Rejected:** Keeping two sources, stale-inline suppression and permissive order leaves an implicit dependency.
- **Batch admissibility:** Every entry passes the same v4 vocabulary,
  subject/ID and renderer-proof rules as today's inline pair, regardless
  of source. Structural invalidity refuses the whole batch as
  `snapshot-invalid` / `command-invalid`: no incorporation, execution or
  high-water advance. Preserve execution-time freshness checks on admissible
  renderer proof. Reuse the broker's 128-command drain cap
  (`crates/omnitiler/src/world_broker.rs:218,1308`); inline counts toward
  128 total entries. Exact report replay does not admit or execute a new
  caller-supplied batch (C5 D2).
- **Return contract:** Deliverable publication (none on whole refusal),
  incorporation (`Incorporated`, `BootstrapIntentOnly`, `Replay`, or
  `Rejected`), and sequence-keyed `Executed(Option<Payload>)`,
  `Refused(kind)`, `AlreadyConsumed` or `NotRun` for supplied identities.
  An invalid or unordered caller-supplied batch gets a batch-level error
  only: no per-sequence dispositions, even for `[7,7]`, and no consumption.
  Bootstrap has no ready authority; exact report replay returns the cached
  publication, no fresh results, and `NotRun` for any resubmitted batch.
  A per-command refusal consumes its sequence and restores only
  that command's partial changes. Whole capture/prepare refusal publishes
  nothing new; drained commands are `NotRun`, restored at the FIFO head.
  An already consumed broker entry must not create a new `ready/null` result
  or be retried as runnable; preserve a prior result if present, otherwise
  record an invariant fault. A disordered or invalid internal drain is
  likewise an invariant fault: log it, fail closed without publishing or
  settling commands, and rotate through the existing broker rediscovery
  path. Never repark the faulting drain; ordinary late prepare failures
  still repark at the FIFO head.
- **Preparation:** One engine transition owns validation and adoption.
  Put reusable typed rules with the model and envelope-only checks with
  protocol/broker; do not fork a weaker native rule set. Require shared
  outbound semantic validation, including
  actual host context (`above-capability-false`, `warp-capability-false`,
  `sticky-capability-false`, `crates/tiler-protocol/src/world_protocol.rs:
  5787-5822`), at final prepare on the ready and bootstrap paths. Guard the
  commit intrinsically, not by a caller's no-op callback. Keep protocol size
  gating by default (C5 D3) and the C5 order: early preflight, staged row
  limits, exact inner count, future reserve, wake rotation, final preparation
  and actual bound check. The typed consumer gets the validated typed world,
  incorporation and outcomes without generating or parsing JSON; a count
  sink checks the default byte policy. Broker prepares bound bytes, projection
  and stage-local bindings before adoption. Validation and encoders must
  agree; exercise decoder parity in tests, not by round trip in production.
  Cached typed replay must derive only from a validated committed reply;
  newly bound envelopes still require bound preparation. Keep structural
  entry points explicit test/benchmark support; no alternate production
  mutator or transport dependency in core.
- **Wire:** Existing JSON shape and report-only replay stay; stale-inline
  coexistence, invalid order and false requester success are observable where
  reachable. Broker normally stamps null inline and allocates monotonic
  global sequences. Neither native consumers nor the broker may infer
  disposition from result nullness or accepted capture from native freshness.
- **Discriminators:** Fresh inline-only with empty queued list; inline with
  renderer proof plus ordered FIFO; stale inline plus fresh queued; stale
  queued vs executed without payload; one refusal between successful commands;
  reversed and repeated sequences; inject removed `SwitchView` alongside
  a valid typed command and assert no execution or high-water advance.
  Test 128/129 entries including inline, and inject an invalid or disordered
  broker drain to prove rotation and rediscovery, not repark. Also cover
  bootstrap then ready, unchanged report plus changed queue, changed
  report at the same sequence, and late prepare rollback followed by
  ordered fresh recovery. Assert revision, replay, intent, dispositions
  and requester results.

## B1. Geometry closed under derivation

- **Current (source-verified):** `crates/tiler-core/src/bounds.rs:13-53` caps
  origins and extents independently. A valid 800-wide screen at x=16384 and
  outer inset 8 permits a child at x=16392 (`crates/tiler-core/src/geometry.rs:
  108-143`); the structural/size-only route can adopt it, but the validating
  writer refuses `unknown-value` (`crates/tiler-protocol/src/world_reply_write.rs:
  277-295`). The broker misclassifies that as size. This is a small legal
  rectangle, not arithmetic overflow. Stale origin caps also exist in
  `crates/tiler-core/src/world_directional.rs:280-345`,
  `kwin/src/world-observe.ts:361`, `kwin/src/world-report.ts:696-710`,
  `kwin/native-effect/gesture_lifecycle.h:114-116` and
  `kwin/native-effect/presentation_consumer.h:127-130`.
- **Decision (User 2026-11-19, A):** One coordinate space with half-open
  rectangles, inclusive endpoint bounds -32768..32768 and positive extent
  cap 16384. Previously legal absolute rectangles still fit. Positive
  contained children and insets share the same bounds; point checks use the
  same axes. Keep size hints and gaps independent. Use wide intermediates and
  checked conversion; audit every carried rectangle, point and target for
  stale caps. No clamping. A genuinely unreadable output must not authorize
  writes or fabricate adjacency, and an independently usable neighbor keeps
  its authority. Touching edges do not overlap; negative origins are ordinary.
- **Rejected:** Enforcing the old -16384..16384 limit at screen ingress would hold some previously accepted outputs.
- **Wire:** Yes, changed accepted numbers or held output behavior; no new
  rectangle shape. Update live readers, writers and the normative range
  together, including KWin. Do not alter pure extent limits to match a
  coordinate bound.
- **Discriminators:** +16384 with inset 8 must publish x=16392. Drive
  observation through the real checked broker, TS reply reader, native
  gesture and presentation consumers, and the next capture reporting the
  frame again. Add KWin TS and native-effect unit coverage. Compare typed
  and checked broker with a -16384 control, exact edges on both sides,
  touching outputs, off-edge input and one unreadable output beside a usable
  neighbor; assert frames, hold scope and writable authority.
  Extend the existing `after_ready_bad_screen_holds_only_affected_output`
  case (`crates/tiler-core/src/world_coverage_gaps_tests.rs:4274-4341`).

## B2. Bounded derived identities

- **Current (source-verified):** A crossing gesture uses
  `gesture-{window}-{start_seq}` for retained operation/transfer correlation
  (`crates/tiler-core/src/world_gesture.rs:88-94,864-905`). Even sequence 2
  gives 138 bytes for a legal 128-byte window token, but validating transfer
  emission requires an opaque ID of at most 128 bytes
  (`crates/tiler-core/src/bounds.rs:13-26`;
  `crates/tiler-protocol/src/world_reply_write.rs:64-68,361-371`).

  Derived-ID inventory:

  | Family | Boundary finding |
  | --- | --- |
  | Layout leaf | `leaf-{window}` plus optional suffix is already 133 bytes for a 128-byte window (`crates/tiler-core/src/world_layout/tree_edit.rs:26-39`; called by `world_layout/admission.rs:136`). It is not just internal: active-group projection can hide the underlay, and gesture hover and underlay reply lanes validate leaf IDs (`crates/tiler-core/src/active_group.rs:134-169`; `crates/tiler-core/src/world_presentation_v4.rs:406-426`; `crates/tiler-protocol/src/world_reply_write.rs:605-637,883-937`). Mint independently of the window. |
  | Generation | `WorldEngine::new` accepts a 64-byte generation, but rebind appends a decimal suffix and refuses `revision-exhausted` when the result exceeds 64 (`crates/tiler-core/src/ids.rs:13-16,43-50`; `crates/tiler-core/src/world_engine.rs:456-473,520-547`). Use bounded rebind generation issuance without narrowing valid input; retaining the refusal would preserve an avoidable failed rehandshake. |
  | Command and policy operations | `op-{cmdSeq}`, fixed `swap-a`/`swap-b`/`refill` children, `op-dpv-{epoch}-{index}`, `op-hp-{epoch}-{index}`, `op-born-{n}` and `op-native-{n}` have bounded numeric components. Refill suffixing is only safe for the specific short migration family, not arbitrary opaque IDs. Correlate parent/child explicitly inside core rather than infer it from token spelling. `born_seq` increments unchecked and native creation saturates; guard exhaustion without wrap or reissuing the ceiling ID. Source: `crates/tiler-core/src/world_commands.rs:459-487,1572-1650`; `world_sends.rs:233-264`; `world_maximize.rs:445`; `world_operations.rs:573-612`; `world_model.rs:1310-1340`; `world_settle.rs:515-542,1209-1215`; `world_born.rs:62-68`; `world_native.rs:2391-2393,3080-3082`. No reachable exhaustion run is claimed. |
  | Other bounded tokens | `ws-{n}`, `grp-{n}`, fitted `fit-l{n}`, `focus-{rev}` and internal `af-{n}`/`fx-{seq}` use numeric parts and fixed prefixes. Core generation and KWin adapter/source mints use their own token/counter domains. Keep passing spellings and fences where the family proves its bound. Source: `crates/tiler-core/src/world_model.rs:1413-1427,1798-1806`; `world_layout/group_alloc.rs:23-71`; `seed.rs:266,339`; `world_close_focus.rs:340,571`; `world_report.rs:466`; `crates/omnitiler/src/world_service.rs:87-102`; `kwin/src/world-entry.ts:396-410,4157-4168`. |
  | Not opaque IDs | Core/output scope strings, decoder duplicate-set keys, KWin binding tuple keys and broker log correlations are internal composites, not `Id` fields; workspace and desktop names are display text. Keep them separate from wire IDs. Source: `crates/tiler-core/src/world_model.rs:113-139,317-320`; `world_report_v4_checks.rs:1151-1154`; `world_presentation.rs:165`; `crates/tiler-protocol/src/world_protocol.rs:5592-5595`; `crates/omnitiler/src/world_service.rs:572-603`; `kwin/src/world-entry.ts:534-542`; `kwin/src/world-apply.ts:382`. |

- **Decision (User 2026-11-19, A):** Use the correlated, generation-local
  core gesture ID with a reserved prefix and independent bounded leaf and
  rebind mints. Core fences reused IDs (`crates/tiler-core/src/world_gesture.rs:
  107-165,204-219,729-781`); KDE maps `(instance, serial)` to a checked
  monotonic core ID (`crates/omnitiler/src/kde_gesture.rs:219-247,461-544`).
  The raw producer serial alone is not generation-unique. Prove maximum
  length, namespace and lifetime uniqueness at mint; keep correlations in
  typed fields. No truncation, client parsing or smaller window IDs. The
  leaf namespace must not consume group serials. Gaps are allowed; no
  new retry protocol or acceptance under a spent ID.
- **Rejected:** A dedicated gesture-operation counter adds state and exhaustion handling without adding identity information.
- **Wire:** Opaque spelling and formerly unusable transfers/leaf projections
  can change; grammar does not. Bounded rebind changes a prior refusal to a
  valid fresh generation. Other proven numeric families retain spelling.
- **Discriminators:** Cross output with a 128-byte lifetime and maximum typed
  counter; validate transfer, terminal and later exact attempt correlation
  on both routes. Test repeated gestures/incarnations, rollback, serial
  exhaustion and old-generation fences. Focus and hover a maximum-ID leaf
  in a projected group. Rehandshake from a maximum-length generation and
  prove a new bounded token; test operation counter limits by injection,
  including refusal without adoption or reuse.

## Validation is not overflow

- **Current (source-verified):** bound and projection writer errors both map
  to `BrokerError::Oversized` (`crates/omnitiler/src/world_broker.rs:2033-2052`),
  as does actual bound overrun. The service maps this to `Unavailable("reply
  exceeds size bound")`; capture D-Bus logging manufactures `reply-oversize`
  for generic `Unavailable`, even for rotation
  (`crates/omnitiler/src/world_service.rs:339-364`;
  `crates/omnitiler/src/planner_service.rs:465-490`). Writer kinds include
  `unsupported-version`, `unknown-value` (rects, IDs and other scalar domain
  failures) and `snapshot-invalid` (e.g. `section-over-cap`, duplicate rows,
  focus mismatches, `visibility-show-fallback`, `intent-data-invalid`,
  `suspended-output-inconsistent`, `effect-focus-removed`). Do not use the
  standalone writer's `snapshot-invalid` / `encode-mismatch` byte-cap error as
  a classification rule; the same token also describes content failures
  (`crates/tiler-protocol/src/world_reply_write.rs:57-88,296-317,416-519,
  1073-1220,1413-1434`).
- **Decision (User 2026-11-19, A):** Distinguish semantic validation and
  byte overruns through protocol, broker, service and D-Bus. Late semantic
  failure is unremembered `com.omnitiler.Planner1.InvalidReply` with fixed
  `reply-invalid` reason; retain the underlying fixed rule in diagnostics
  without echoing payload. Keep remembered in-band `reply-oversize` for
  conservative/exact/reserve byte gates, unremembered
  `Unavailable("reply exceeds size bound")` for actual bound overrun, and
  incoming `oversized` separate from `snapshot-invalid`. Both late errors
  preserve authority, projection and replay, repark the FIFO and require a
  fresh source when the prior source floor was spent.
- **Rejected:** A remembered in-band validation refusal would add a new redelivery rule for a service-generated fault.
- **Wire:** Yes, late invalid output stops masquerading as `reply-oversize`;
  actual byte-limit outcomes and precedence stay unchanged. Distinguish
  request overrun, semantic output failure, reply overrun and rotation in
  service/D-Bus classification and logs.
- **Discriminators:** Force a small invalid rect/ID and a fixed semantic
  structural token in typed prepare; exercise bound and projection error
  translation on the broker. Contrast with a fitting valid reply, an early or
  inner-plus-reserve refusal, and inner-fits/bound-exceeds. Assert classification,
  unchanged authority/projection/pins/watermark/issuer results, exact replay
  policy, and ordered fresh recovery. Reuse the C5 terminal-headroom row.

## Reference outcomes

The [pre-KDE core interface discriminators](../../spec/reference-outcomes.md#pre-kde-core-interface)
record the selected behavior and its offline evidence. Native outcomes
remain TBD. Existing typed replay, size and rapid-toggle rows still apply.

## Units and gates

| Unit | Depends on | Acceptance |
| --- | --- | --- |
| G. Geometry | B1 choice | Replace obsolete absolute +/-16384 checks with the endpoint/extent rule. Prove x=16392 through observation, checked broker, TS reply reader, native gesture and presentation consumers, then next capture; add KWin TS and native-effect tests. Preserve the healthy neighbor. |
| I. Identity | B2 choice | Every touched derived ID meets its bound, collision and replay contract; cross-output gesture validates at the extrema |
| P. Preparation and classification | A4/preparation and error choices; G and I before enforcing contract | One mandatory semantic/host validator with typed and bound prepare; no commit on validation or size failure; keep existing C5 checkpoint and retry distinctions |
| T. Transition, dispositions and caller cutover | P and A4 ordering choice | One prepared entry; bounded validation, explicit outcomes, no structural production bypass. Prove invalid typed batch rollback, broker invariant-fault rotation, ordinary late FIFO repark and cross-client FIFO with per-requester results. |
| C. Contract and evidence | A5 choice; G/I/P/T results for final sign-off | Update current authority and per-host obligations; publish both equivalent examples and only approved rows, prove sequence equivalence |

**G accepted offline:** The same frame at x=16392 passes KWin observation,
the checked broker, TS reply/apply, recapture, and the Qt gesture and
presentation readers. The native probe consumes real `SetPresentation`
dispatch bytes after causal effect observation. Typed and checked broker
tests agree on scoped unreadable-output authority. No native desktop
behavior or final cutover gate is claimed here.

**I accepted offline:** Typed and checked broker routes prove bounded
gesture/leaf identities, exact attempt correlation, and lifetime-ordered
projected tiled rows without changing workspace-ring traversal. Exhaustion
does not reuse or adopt a spent operation. Only approved leaf values changed
in 17 full and 11 s2 active recordings; frozen recordings were unchanged.

**P accepted offline:** Shared semantic and host-capability validation runs
at final prepare on both publication paths. Earlier C5 size and wake fences
keep their precedence. Invalid replies use the unremembered `InvalidReply`
error with `reply-invalid`; actual bound overflow remains `Unavailable`.
Checked capture tests verify rollback, FIFO repark and fresh recovery.
The approved trailing workspace mint skips globally occupied logical IDs,
including retained switch references, without renaming explicit workspaces.
A v4 Queue caller can trigger this collision; current KWin does not issue
that explicit Send. Focused rollback and broker Stage 5 byte checks passed;
P changed no recordings.

**T accepted offline:** One prepared transition admits the ordered inline
entry and caller FIFO, keeps replay report-only, and reports incorporation
and per-command dispositions separately. The checked broker preserves
requester results, reparks ordinary failures, and rotates on invalid or
disordered internal drains. Both checked and typed swap probes confirm that
post-command layout membership prevents stale holds from shadowing live tiles.
The structural cost gate passed at 7.82M/34.63M Ir (4.43x); the separately
measured checked count-and-encode path used 8.38M/36.82M Ir. T changed no
recordings.

**C accepted offline:** The amended v4 contract describes supported host
obligations, endpoint-specific refusal rules, and equivalent typed and
bound-broker examples. Current captures pass; v3 and frozen v4 without
`propertyAttempts` refuse under the existing precedence. Both command
sequences pass all six route/delivery combinations below. Split routes
distinguish modeled eligible writes from skipped intermediate application
while withholding proof before the next command. The assertions cover the
per-command payloads, refusal, exact redelivery, once-only retained focus
identity, and the issuing sequence for B's maximize. A stays unchanged.

Implementation gates (offline): use [C5 migration and cost](core-wire-boundary.md#migration-acceptance-and-cost)
and [completion evidence](core-wire-boundary.md#completion-evidence).
Scoped Rust format, all-target build, strict Clippy and tests for `tiler-core`,
`tiler-protocol`, `omnitiler` (same `workspace_unsafe_posture` exclusion),
KWin typecheck/tests, existing launcher/native-effect/helper gates, both
Stage 5 production routes and exact canonical byte oracles. Keep frozen
`world-wire/v4`, `stage5`, `stage5-v4`, `kde-v4` unchanged; re-record active
families only for approved effects and preserve unrelated bytes. Cover every
property attempt kind and existing preflight, exact, future reserve, bound
failure, rollback and replay tests. Retain `scripts/check-report-cost.sh`
(24M/100M Ir at 50/200 windows, growth <=6.5) and release
`report_scale all 31 3 all` (steady p95 <=1/4/10 ms at 10/50/200; first
reports separate). Compare fresh and warm full broker capture through
returned bound bytes on the same C5 workload; no new latency budget here.
Keep C5 soft 1500/3000 production/test line targets for touched code.

**Sequence-equivalence gate:** With native feedback delayed, run both the
two-toggle control and the noncommuting pair: start focused on A, focus B,
then maximize `currentFocus`; B must maximize and A must remain unchanged.
Use matched initial worlds for combined versus split captures, split with
intermediate apply skipped, and both typed prepared and real checked broker
routes. Compare final logical state, each outcome (include a refusal control)
and allowed correlation relationships, not timestamps or dense IDs. Exact
source redelivery must not rerun commands; incorporation does not prove
native apply. The existing Tier 1 rapid-toggle outcome supports logical
ordering, not native frame timing.

**Cross-client FIFO gate (T):** In two distinct client namespaces of one
authenticated sender, file local 9 (focus unknown window) before local 1
(focus known window). Check each issuer's result: the first is
`failed/focus-unknown`, the second `ready`, in global filing order. Checking
only ACKs or final focus would miss the ordering defect.

## Completion evidence

| Offline gate | Result | Baseline or limit |
| --- | --- | --- |
| Scoped Rust format, all-target build and strict Clippy | Pass | Same packages as C5 |
| Scoped Rust all-target tests | 3,530 passed, 0 failed; one preexisting `workspace_unsafe_posture` exclusion | C5: 3,468 passed under its earlier invocation, without the final all-target scope |
| KWin typecheck and tests with both Qt probes enabled | Pass; 1,383 passed, 0 failed, 0 skipped | G/I: 1,382 passed and one probe skip |
| Stage 5 production routes | Checked broker 5/5, KWin subscriber 34/34; canonical byte oracles pass | Both routes enabled; only approved leaf tokens re-recorded (17 full, 11 s2) |
| Launcher, native-effect and helper scripts | 703, 204 and 58 passed | Unchanged from C5 |
| Frozen fixture families | Byte-identical to the frozen baseline | `world-wire/v4`, `stage5`, `stage5-v4`, `kde-v4` |

Neither known test flake required a retry. The Qt gesture and presentation
probes passed offline, including the connected x=16392 round trip. This is
not native desktop, visibility, or paint acceptance.

| Cost (same workloads as C5) | C5 | Final |
| --- | --- | --- |
| Callgrind, 50/200 windows | 7.72M / 34.07M Ir, 4.41x | 7,819,158.5 / 34,627,717.2 Ir, 4.43x (limits 24M / 100M Ir and 6.5x) |
| Release steady worst p95, 10/50/200 windows | 0.2871 / 1.9622 / 4.4720 ms | 0.2411 / 2.0707 / 5.6019 ms (limits 1 / 4 / 10 ms) |
| Release first reports, 10/50/200 windows | 0.1732-0.3786 / 1.0027-3.3286 / 12.3155-42.5533 ms | 0.1778-0.4167 / 1.0352-3.1294 / 12.5102-73.3692 ms |
| Fresh broker p50/p95 | 203.0 / 211.1 us | 206.0 / 213.6 us |
| Warm broker p50/p95 | 210.5 / 218.7 us | 211.3 / 220.6 us |

Broker times are medians of seven series of 300 full captures through bound
reply bytes, using the same active Stage 5 genesis and live correlation
substitution as C5. Fresh and warm replies were 2,222-2,223 and 2,225-2,236
bytes (C5: 2,221-2,222 and 2,224-2,235). Recorded Stage 5 byte checks pass
separately. No broker wall-clock budget was added.

Approved behavior changes were limited to carried coordinates and scoped
holds; bounded opaque identities; bytewise tiled window order within each
projected domain; the leaf-token-only active recordings; the separated
semantic output error and batch admission outcomes; the trailing workspace
mint skipping an existing explicit destination; and the stale global-swap
hold no longer shadowing a live tile. No frozen recording or visibility
refusal precedence changed.

## Non-goals

No macOS or Windows migration or checks, native acceptance claim, delta/event
protocol, new planner on adapters, layout rewrite, clone optimization project,
or retained cursor-warp redesign. Preserve the settled C5 decisions and leave
its pending visibility diagnostic precedence untouched.

### Deferred to KDE rebuild note

- B3: settings atomicity, including preparation of its bounded reply.
- B4: bound retained membership and prove recovery under the bound.
- B5: broker replay byte budget and issuer namespace lifetime.
- B6: duplicate-key consistency and refusal precedence for remaining endpoints.
- Immutable close delivery token; retire only incorporated evidence, independently of native apply.
- Checked-service latency budget and separate native latency evidence.
- Native KDE delayed ACK, skipped apply, close in flight, causal visibility and creation, and recovery proof.
