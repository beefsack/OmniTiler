# V4 phase 2: remaining v3 removal (KDE slice)

## Goal and status

- Remove only what remains after cull-as-you-go, per the
  [implementation order](../../decisions/contract.md#contract-authority-and-v4-migration):
  separate verified cull commits with no-live-path evidence; retain immutable
  recorded fixtures, active migration records and both stage5 proofs.
- Status: KDE/shared dead code with no live caller is culled (units 1-2 below).
  The approved Rust approach is port-then-cull: map legacy assertions to v4,
  port uncovered supported behaviour, then remove unused legacy code separately.
  Core, protocol and stress ports and the separately reviewed legacy
  implementation culls and final verification are complete offline.
  All 33 recorded divergence markers have been classified and independently
  reviewed: 16 adopt approved v4 behaviour, 17 retire obsolete expectations,
  and none require a production fix or remain undecided. The pager-order
  question was resolved in favour of immediate pruning. All 33 markers are gone.

## Acceptance and approach

- Preserve supported behavioural coverage through direct v4 reports and replies;
  remove an expectation only when its lane is explicitly retired or equivalent
  v4 assertions are identified. Do not retain conversion helpers in the end state.
- Resolve differences against active decisions and the approved v4 contract;
  collect genuinely undecided outcomes without choosing or changing production.
- Port core integration/in-source tests, then protocol and stress tests; cull
  legacy entry points, codecs, helpers and types only after remaining callers
  are accounted for. Review each cull separately with no-live-path evidence.
- Verify changed crates with scoped format, tests, all-target build and clippy;
  verify KDE typecheck/tests for KDE changes. No live acceptance is claimed.
- Non-goals: platform ports, new wire fields, fixture rewrites, and unrelated
  stale platform documentation. Keep shared v4 foundation code and stage5 proofs.

## Inventory result

| Cluster | Class | Outcome |
| --- | --- | --- |
| `ObservedDomain.adjacent` carrier and stale "v3 lane / K1b cutover" wording in `kwin/src`, one misnamed test file | Dead, KDE-only | Culled (unit 1) |
| `crates/omnitiler/examples/world_wire_fixtures.rs` | Dead, runtime-broken | Culled (unit 2) |
| `decode_request_legacy`, `decode_request_with_wire_legacy`, `DecodedRequest`, legacy-only row decoders and `encode_reply_legacy` in `tiler-protocol` | Retired codec lane | Coverage ported; separately reviewed cull complete |
| `handle_report_legacy`, `LegacyWorldOutcome`, legacy `run_accepted`/`finish`/`bootstrap_report_reply`, legacy observe/stage helpers, report/reply shapes and sizing, `WORLD_CONTRACT_VERSION_LEGACY` | Retired core lane | Coverage ported; separately reviewed cull complete |
| `run_stress_legacy` and its runtime tests in `world_stress.rs` | Retired runner; shared loader/schema/schedule remain live | Runtime coverage ported to `run_stress_v4`; separate cull complete |
| `world_report/publish/presentation/sizing.rs`, `ReportWireOptionals`, shared decode helpers, retired-key vocabulary, version constants `WORLD_CONTRACT_VERSION[_V4]` | Live shared v4 foundation (not legacy) | Retained |
| `test-fixtures/world-wire` (non-v4, 12 files), `stage5`, `stage5-v2`, `stage5-v4`, the two stage5 proofs, `n5_retained.rs` (Windows exclusions, not a wire v3 test) | Retained by rule | Retained |
| `tiler-windows`/`tiler-macos` still import removed report/reply shapes or bootstrap APIs; their scripts and just recipes carry no v3 protocol code | Deferred adapter consumers | Adapter sources retained; temporarily broken adapters are authorized by the migration order and await their gated ports |
| `justfile`, `just/linux.just`, CI workflows, packaging, nix, `kwin/package.json`, `kwin/native-effect` | Clean (CI "v3" hits are action version pins) | Nothing to cull |

## Unit 1: dead adjacency carrier and stale v3 wording (KDE)

- Caller evidence (repository-wide `git grep`): `ObservedDomain.adjacent` was
  always emitted as `{}` and never serialized: `kwin/src/world-report.ts` has no
  `adjacent` reference, the Rust `decode_report_v4` key list has none, and no
  `test-fixtures/kde-v4/bytes` or `world-wire/v4` report carries it; nothing in
  `kwin/src` reads `domain.adjacent`. Tests only constructed literals; the two
  non-empty literals in `world-drag-arrival.test.ts` fed no assertion.
- Removed the field, its emission and the keys in 13 test files. Unrelated
  `adjacent` meanings (`move-conformance*`, `no-adjacent-output` causes,
  `kwin-globals.d.ts` prose) stay.
- Comment-only rewording where live v4 code was described as a "v3 lane" or
  "K1b cutover" (`world-connection.ts`, `world-entry.ts`, `world-observe.ts`,
  `output-topology-parity.test.ts`). `world-occupancy-v3.test.ts` is renamed
  `world-occupancy-v4.test.ts` (content was already v4; it keeps its v3-refusal
  assertions).
- No behaviour or wire change. Gates: adapter typecheck pass; adapter tests
  1205/1205.

## Unit 2: dead `world_wire_fixtures` generator

- Evidence: it built `v: 3` envelopes and asserted `reply["v"] == 3` through the
  service, while the production broker requires `BROKER_VERSION` 4
  (`world_broker.rs`), so it cannot run; no `[[example]]` entry, justfile,
  recipe, CI, packaging or script referenced it; its private helpers are unused
  elsewhere. It was never run during v4 (recorded in the contract decision).
  The recorded bytes under `test-fixtures/world-wire` are untouched and retained.
- Doc references reconciled: `docs/porting/kde-port-notes.md` (two lines),
  `docs/porting/world-contract-v3.md` (broken link), and the active contract
  decision's generator assumption. Historical archived notes,
  `core-interface-review.md` and `v4-migration.md` are left as written.
- Gates (scoped, `omnitiler`): fmt, build `--all-targets`, clippy
  `--all-targets -D warnings` pass; tests 341 passed with
  `--skip workspace_unsafe_posture`.

## Rust port progress and accepted mappings

- The legacy lane is not a pure deletion: its tests supply much of the
  behavioural matrix. Existing v4 admission/publication tests are not assumed
  equivalent merely because they cover the same topic.
- Core ports cover directional actions, selection/arrival, focus feedback,
  maximize/fullscreen/sticky, saved intent, gesture/routing, admission/close,
  presentation and sizing. Thirteen core integration files are ported; the
  fourteenth, `moving_border_comparison.rs`, is culled separately as a retired
  absolute-versus-anchored prototype. Core admission, command-issue and
  observe/capture suites now use v4 only; all remaining supported core coverage
  has been ported before culling its legacy implementation.
- Focus ports now assert one explicit grant, retained/emitted identity, tuple
  settlement, stale-tuple immunity and no warp from superseded admission focus:
  `explicit_batch_focus_supersedes_admission_grant`,
  `newer_emitted_batch_supersedes_older_and_stale_tuples_ignore`, and
  `warp_follows_single_explicit_grant_not_superseded_admission`.
- `default_engine_suspends_focused_fullscreen` positively proves unconditional
  suspension and preservation of the other output's pager. Restored routing
  retains Found/root-leaf underlay checks on the v4 entry.
- Retired ingress geometry maps to `world_presentation_v4_tests`:
  `genuine_descriptor_positive` (joined witness/anchored descriptor),
  `underlay_found_scaled` (outer union extension), `shell_and_disabled_hides`,
  and `zero_width_and_missing_scale_hide_empty`. Style/scale variants remain
  covered by that module's fractional, boundary and extension tests.
- Pager retention is asserted with live-empty rows after a known frame;
  float/tile source slots are retired. Rowcap coverage now asserts refusal
  before revision/highwater/rollback-queue mutation. Removed logical view echoes
  no longer gate tests; unrelated unknown-output routing remains covered.
- Pager order alone never creates membership. Both formerly marked sequences
  now assert same-report pruning under complete inventory; the new
  `pager_held_without_fact_retains_order_slot` proves the held exception.
  Incomplete-inventory retention remains covered by the existing v4 tests.
- Independent source/diff review accepted these mappings after correcting
  proposals to retain resolved legacy-only tests. A comment-only follow-up
  replaced vague style-test references with exact replacement test names.
- Five additional integration ports preserve 128 tests: `cursor_targeting` (25),
  `world_native_attempt_settlement` (19), `world_native_retirement` (29),
  `world_host_owned_gap_holds` (23), and `world_host_owned_stage1b` (32).
  They construct v4 reports directly. Validation/digest helpers map to v4;
  the three retired view-echo halves leave native-current settlement and hold
  assertions intact. Independent diff/source review found no supported
  assertions dropped in these five files.
- Admission tests retire only dual-cache coexistence; command-issue tests retain
  all 15 v4 assertion sets. Observe/capture retains 85 tests, retiring only the
  legacy capture test whose v4 twin additionally covers the new lanes; the
  cross-intake tail and removed echo/paused arms are retired explicitly.
  Independent source review checked frozen attempt lanes, reserve arithmetic,
  retained joins, staged clock publication and rich capture lanes.
- Observe/capture port verification first exposed insufficient stage-only
  bootstrap setup. Full v4 admission restored the original homing preconditions;
  one accidentally omitted held-screen row was restored from the original test.
  The corrected 85-test suite and scoped crate gates pass. Dead unused contra
  reports were removed so staged-only tests no longer claim passed-report proof.
- Shared fact/geometry builders now belong to `world_v4_support_tests`.
  Presentation/publication/gesture seeds use genuine v4 admission. That seed
  publishes internally, so latest-presentation revision assertions now use the
  actual seeded revision; the no-retention test still distinguishes an
  unretained live frame. Sticky-restore and persistent-above preserve all 41
  existing tests without semantic expectation changes.
- Freeze tests retain 11 explicit v4 assertion sets instead of dual-lane
  equality. Stage tests retain 16 tests, retiring only the legacy-intake
  ignores-new-fields test. Shared classification and every v4 staged lane stay
  covered. Independent source review accepted these ports.
- The five effect-sizing tests now use v4 exact-byte/estimate/bound helpers.
  The reserve-boundary proof retains the no-reserve-fits/reserve-exceeds
  counterfactual, no-mutation and same-sequence retry. Explicit focus
  supersession additionally proves the admission token was minted first.
  Its first port run exposed the already-approved single-grant difference;
  the corrected test asserts that active rule, not a production change.
- Still-active wire-string pins are covered byte-exactly by protocol
  `v4_reply_writer_round_trips_all_valid_wire`,
  `v4_reply_writer_suspended_output_present_and_inconsistent`, and
  `v4_reply_exact_matches_canonical_encoding`; removing legacy core encoder
  assertions does not replace them with byte-length checks alone.
- Separately source-reviewed unused helpers are now culled: directional legacy
  report/accepted builders, test-only `stage_facts`, and the unused conversion
  bridge plus its three self-tests. Shared fact builders and the outcome
  formatter belong to `world_v4_support_tests`; importers and doc pointers now
  point there. Both temporary dead-code allowances disappeared with their helpers.
- Windows/macOS retain consumers of removed legacy shapes/bootstrap APIs.
  Temporarily broken adapters are expressly permitted by the migration order;
  their source is preserved for the gated ports. No Windows/macOS runtime or
  host acceptance is claimed by this removal.

## Divergence classification

The table records the original marker locations under `crates/tiler-core/`;
line numbers identify the pre-resolution evidence, not current comment positions.
There were no markers in `tiler-protocol`. Classes: (a) adopt approved v4;
(b) retire an obsolete expectation; (c) required v4 fix; (d) undecided.
Counts after the pager decision: a=16, b=17, c=0, d=0. Rows 17-18 were
initially one undecided question, resolved by the user on 2026-11-09.

Authority keys:

- F: [FIFO/latest publication](../../decisions/commands.md#kde-rapid-input);
  `src/world_run_v4.rs:477-479,605-608` drops superseded unissued focus.
- S: [unconditional fullscreen suspension](../../decisions/world-rendering.md#focused-fullscreen-presentation-suspension)
  and `world-contract.md:1221-1228`, rejecting opt-in and all-output paused.
- G: `world-contract.md:261-272,1221` moves styles from ingress to settings;
  [R20](../../decisions/contract.md#anchored-border-r20) requires a joined witness
  and anchored descriptors, not legacy inner/outer border rectangles.
- P: `world-contract.md:1216-1225` keeps only live pager sources and retains
  last-known frames; `src/world_presentation_v4.rs:511-608` implements retention.
- V: `world-contract.md:349-353,981-1005` removes ScreenView/logical echoes;
  `src/world_publish_v4.rs:44-45` passes no view echoes.
- B: `world-contract.md:111-125,1182-1199` bounds arrays independently of bytes;
  `src/world_admit_v4.rs:419-444` refuses rowcap overflow before mutation.
- R: `world-contract.md:185-186` defines pre-settings empty correlated readiness;
  `world_admit_v4_tests::pre_ready_bootstrap_emits_no_authority` covers it.

| ID / original marker | Legacy expectation | V4 behaviour / implementation | Class / authority / disposition |
| --- | --- | --- | --- |
| 1 `src/world_cursor_tests.rs:187` | Legacy helpers support admission plus explicit focus grants | Only latest explicit grant, `world_run_v4.rs:605-608` | a / F; helpers removed |
| 2 `src/world_cursor_tests.rs:1053` | Warp ignores earlier cross-output admission among two grants | Only same-output explicit grant survives, `world_run_v4.rs:605-608` | a / F; single-grant no-warp port |
| 3 `src/world_focus_settlement_tests.rs:162` | Builders support two grants in one batch | Explicit grant supersedes admission, `world_run_v4.rs:605-608` | a / F; helpers removed |
| 4 `src/world_focus_settlement_tests.rs:934` | Admission and explicit grants both bind and settle | One explicit grant binds and settles, `world_run_v4.rs:605-608` | a / F; positive v4 settlement port |
| 5 `src/world_focus_settlement_tests.rs:1024` | Stale tuples from earlier two-grant batch are ignored | Earlier single grant is superseded, `world_run_v4.rs:605-608`; v4 port retains stale-tuple immunity | a / F; stale-tuple port |
| 6 `src/world_presentation.rs:903` | Style sizes/enables supplied as ingress facts | Settings supply styles, `world_presentation_v4.rs:188-215,299` | b / G; obsolete helper fields retired |
| 7 `src/world_presentation.rs:1089` | Underlay outer union expanded by ingress sizes | Settings-composed extension, `world_presentation_v4.rs:459-508` | b / G; maps to `underlay_found_scaled` |
| 8 `src/world_presentation.rs:1144` | Ingress underlay-disabled flag hides | Composed setting hides, `world_presentation_v4.rs:299` | b / G; maps to `shell_and_disabled_hides` |
| 9 `src/world_presentation.rs:1213` | Paused flag suppresses projection | No paused ingress; truthful fullscreen suspension, `world_presentation_v4.rs:144` | b / S; paused arm retired |
| 10 `src/world_presentation.rs:1328` | Ingress gap produces inner border rectangle | Anchored outsets/settings/witness, `world_presentation_v4.rs:188-215` | b / G; descriptor twin; revision assertions retained |
| 11 `src/world_presentation.rs:1459` | Legacy bootstrap constructor mirrors unready reply/revision | V4 pre-settings report emits no authority, `world_admit_v4_tests.rs:420` | a / R; maps to pre-ready v4 proof |
| 12 `src/world_presentation.rs:1886` | Ingress sizes produce inner/outer ring | Anchored descriptor, `world_presentation_v4.rs:188-215` | b / G; maps to `genuine_descriptor_positive` |
| 13 `src/world_presentation.rs:1978` | Ingress disabled/zero width hides border | Settings control hide, `world_presentation_v4.rs:199` | b / G; disabled/zero-width twins |
| 14 `src/world_presentation.rs:1988` | Paused yields suspended border/empty pager | No paused lane; local fullscreen suspension, `world_presentation_v4.rs:144` | b / S; paused arm retired |
| 15 `src/world_born_tests.rs:213` | Paused arm prevents born desires | `world_publish_v4.rs:42-44` always uses born_paused=false | b / S; capability-off arms retained |
| 16 `src/world_born_tests.rs:1608` | Game pause holds cohort clear | No pause hold, `world_publish_v4.rs:42-44` | b / S; renamed capability-off proof |
| 17 `src/world_pager_presentation_tests.rs:518` | Unknown order identity retains slot for one report | Complete-inventory absence prunes immediately, `world_presentation_v4.rs:511-536,572-590` | a / User 2026-11-09; same-report prune asserted |
| 18 `src/world_pager_presentation_tests.rs:652` | Re-verified order token retains slot before readmission | Immediate prune, `world_presentation_v4.rs:511-536,572-590`; never renders unadmitted token | a / User 2026-11-09; re-verified-token prune asserted |
| 19 `src/world_pager_presentation_tests.rs:693` | Live/float/tile frame priority; float/tile alone paint | Only live source exists, `world_presentation_v4.rs:543-550` | b / P; obsolete slots retired |
| 20 `src/world_pager_presentation_tests.rs:739` | Unresolved rows do not accumulate frames | Last-known live frame retained until departure, `world_presentation_v4.rs:597-608` | a / P; positive retention assertions |
| 21 `src/world_pager_presentation_tests.rs:789` | Paused report publishes empty content | No paused lane; other-output pager survives local suspension, `world_presentation_v4.rs:144` | b / S; paused expectation retired |
| 22 `src/focused_fullscreen_presentation_tests.rs:81` | Helper supplies style ingress | Styles come from settings, `world_presentation_v4.rs:188-215,299` | b / G; helper fields retired |
| 23 `src/focused_fullscreen_presentation_tests.rs:238` | Default engine never suspends | Unconditional suspension, `world_presentation_v4.rs:144` | a / S; positive default-engine test |
| 24 `src/focused_fullscreen_presentation_tests.rs:267` | Styled ingress makes border visible | Missing joined witness hides, `world_presentation_v4.rs:188-215` | a / G; witness-aware coverage mapping |
| 25 `src/focused_fullscreen_presentation_tests.rs:412` | Restored foreground border visible without witness | Joined witness required, `world_presentation_v4.rs:188-215` | a / G; suspension/restore assertions retained |
| 26 `src/focused_fullscreen_presentation_tests.rs:610` | Opt-in and plain engines differ in presentation | Both unconditionally suspend, `world_presentation_v4.rs:144` | a / S; identical-actuation assertions retained |
| 27 `src/focused_fullscreen_presentation_tests.rs:724` | Legacy visible-border comparison under suspension | Witness-aware v4 border equality, `world_presentation_v4.rs:188-215` | a / G; shape equality retained |
| 28 `src/world_sticky_native_tests.rs:2206` | Full rollback queue holds proof; same-key replacement wins | Pre-mutation rowcap refusal, `world_admit_v4.rs:419-444` | a / B; refusal/no-mutation proof |
| 29 `src/world_restored_routing_tests.rs:982` | Styled ingress drives Found and visible border | Settings drive underlay; border needs witness, `world_presentation_v4.rs:188-215,299` | b / G; direct v4 Found/root-leaf port |
| 30 `src/world_report_tests.rs:332` | Unknown/duplicate logical views fail validation | Views field retired; no lane in `WorldReportV4`, `world_report.rs:1378` | b / V; obsolete gates retired |
| 31 `src/world_logical_contract_tests.rs:3546` | Unknown view echo holds output incomplete | No logical view echo, `world_publish_v4.rs:44-45` | b / V; echo arm retired, unknown-output routing ported |
| 32 `tests/world_classify_applicability.rs:145` | Helper supplies style ingress | Styles come from settings, `world_presentation_v4.rs:188-215,299` | b / G; helper fields retired |
| 33 `tests/world_classify_applicability.rs:209` | Classified XAML foreground shows border without witness | Unjoined witness hides, `world_presentation_v4.rs:188-215` | a / G; classification retained, descriptor twin mapped |

### Pager-order outcome selected

- Pager order-before-facts, rows 17-18: User 2026-11-09 selects immediate pruning
  for complete inventory when no fact or held row exists. Held identities and
  incomplete inventories retain absent identities; pager order alone never
  establishes membership. Native/reference ordering consequences stay TBD. See the
  [minimal discriminator](../../spec/reference-outcomes.md#pager-order-before-window-facts).
  Tests adopt the existing v4 outcome; no production fix is required.

## Unit 3: retired moving-border prototype

- Removed only `crates/tiler-core/tests/moving_border_comparison.rs` (eight tests)
  in a separate cull. Its dual-candidate absolute/anchored comparison and local
  witness/epoch/schedule vocabulary were explicitly test-only, not approved API
  (`kde-world-v2-port.md:81-89`). R20 retires the absolute-border alternative.
- Independent source review classified the eight comparison harness legs as
  retired (b), with no uncovered selected behaviour or undecided product rule.
  Current core twins: `genuine_descriptor_positive`, `witness_hides_via_intake`,
  `known_hide_reasons_via_intake`, and `underlay_found_scaled`.
- Cross-report native safety remains covered at its actual owners:
  `presentationconsumer_test.cpp::displayHideRetainsFenceSoStaleCannotResurrect`,
  `presentation_borderchain_test.cpp` older-coverage/anchor/gesture-boundary
  assertions, `world-entry-freshness.test.ts` stale A-in-B and in-transit focus
  paint tests, and both genuine stage5 Rust suites. Renderer-input revision
  tests are not substituted for border-paint ordering proof.
- No executable caller or explicit Cargo target remains; references were only
  historical records. Scoped core format/test/all-target build/clippy passed
  after deletion; eight prototype tests were removed, no production path changed.
  K5 native acceptance remains OPEN and is not closed by this cull.

## Protocol ports and accepted mappings

- Feedback (12), state attempts (8) and native occupancy (4) retain all 24
  assertion sets through v4 envelopes, decode, engine handling and typed replies.
  Native occupancy distinguishes a known zero from unknown.
- Opaque intent (15) and optional-wire seams (8) preserve verbatim records,
  UTF-8/caps/digest/reuse/oversize proofs, literal row bytes, optional
  absent/null/empty/nonempty distinctions, clocks, held/native/primary lanes and
  exact reconstruction. Independent review corrected a false two-writer parity
  claim: the generic encoder delegates to the v4 encoder. The accepted proofs
  use one canonical writer, independent core byte arithmetic, typed decode
  roundtrip and retained literal byte pins.
- Selection reply tests retain awaitShown, same-revision show/hide, Controlled
  authority, no-repeat and frozen terminal bytes. Target-lane and in-source
  switch-settled pins use typed v4 replies and literal bytes; invalid escaped
  lifetime input retires under the v4 Id alphabet, replaced by the valid
  128-character boundary. The immutable effect suffix proof is unchanged.
- `world_codec.rs` retains all 60 tests through genuine v4 builders and APIs,
  including canonical bytes, bootstrap, duplicate/trailing/version gates,
  multibyte sizing, held/process/masked-owner lanes, no-mutation oversize,
  commands/effects/settlements, scopes and metadata. Scope fixtures remain
  full-byte pins, not merely byte counts. The bootstrap pin is an actual
  pre-settings engine reply with no authority.
- Identifiable malformed presentation, native workspace, WindowType, creation,
  native-current and optional attempt rows adopt narrow holds under the
  [approved contract](../../porting/world-contract.md), especially lines 307-314;
  raw lane/ordinal/identity/field distinctions remain asserted. Structural or
  identity violations still refuse. Empty view-attempt refs retain a View hold
  with operation identity and pre/post/base fields; this is not the retired
  top-level logical `views` lane. Retired switch-view/operation values refuse
  as unknown values. Immutable two-row member/view fixtures assert both rows.
- The whole-report structural ordering proof uses v4 APIs and retains the
  unknown-field-before-version verdict, malformed/duplicate/trailing and
  overbound gates. Coexistence-only v3 arms were removed separately after ports.
- Review caught a repeated stale numeric-native-workspace refusal expectation
  after hold classification; the corrected hold proof and the final 60-test
  suite were independently checked. Other failed initial expectations resolved
  against existing v4 authority, without production or fixture changes.

## Stress ports and separate legacy culls

- The original 30 stress tests map to 13 shared loader/schema/validation tests
  kept in place and 17 legacy-runtime tests represented by 20 v4 runtime tests:
  seven existing tests retained plus 13 added rejection/anchor proofs. All six
  duo/trio schedule-count locks remain. Reordering, subject/foreground, final
  tiles/focus/foreground, checkpoint view/focus/foreground/tiles, pending-view
  settlement, trio admission and explicit-anchor preconditions remain asserted.
  Retired logical-echo insertion is not claimed as covered; its replacement
  exercises the actual v4 frozen pending-view checkpoint.
- Independent review required the mutant loader to call the shared
  `validate_case`, restoring the original validated-input precondition without
  changing the oracle. The separate stress cull removes only the legacy Sim,
  runner, bindings/helpers and 17 runtime tests. Shared loaders, schema,
  validation, schedules, stats, pending/member proofs, command construction and
  normalization remain live for the v4 runner/example.
- A separate reviewed protocol cull removes legacy full-report decoders, reply
  writer, legacy-only row decoders and coexistence arms. `decode_command` stays
  live through v4 fallback, as do shared screen/attempt codecs, held/raw helpers,
  optionals, canonical encoders, retired-key vocabulary and v4 version constants.
  A missed orphan presentation decoder stopped clippy; targeted independent
  review authorized its removal before gates passed.
- The final separately reviewed core cull removes legacy report execution,
  bootstrap/publish/presentation/sizing wrappers, report/reply shapes, graders,
  digest/validation helpers and replay/presentation caches. Two assertions only
  proving the absence of the legacy cache retire; their v4 assertions remain.
  New orphan warnings stopped verification and received a reviewed, bounded
  extension covering only callerless constants, fields and helpers.
- Shared `ScreenView`, `PagerScreen`, facts, v4 report/reply shapes, intent and
  reserve validation, row encoders, target/command/effect/settlement helpers,
  budgets, assemblers, classifications and planning internals remain. No shared
  foundation file was deleted. The unrelated disconnect/rehome changes remain
  intact. The existing unused public freshness classifier is outside this cull.
- Final source-documentation reconciliation retargets links to removed APIs and
  describes the canonical protocol writer and independent core byte arithmetic
  accurately. It changes comments/rustdoc only, not algorithms or test bodies.

## Completion verification

- Supported coverage ports, independent cull reviews and durable decision
  promotion are complete offline. No production behaviour change was needed to
  resolve migration expectations; no undecided removal outcome remains.
- Final gates pass for `tiler-core`, `tiler-protocol` and `omnitiler`: scoped
  `cargo fmt --check`, `cargo test -- --skip workspace_unsafe_posture`,
  `cargo build --all-targets` and `cargo clippy --all-targets -- -D warnings`.
  Core has 2059 library tests; protocol has 60 library tests and all 60 main
  codec tests; service has 220 library tests. All integration suites pass.
  KDE typecheck and all 1205 adapter tests pass.
- Both `stage5_fixture_route_matches_recorded_projections` and
  `stage5_fixture_route_same_source_serial_b` remain enabled and pass through
  the production route on unchanged v4 recordings. Protected world-wire and
  stage5 fixture paths and Cargo manifests/lockfile have no tracked or untracked
  changes. No divergence markers, conversion bridge or temporary helper
  dead-code allowances remain. Diff whitespace checking passes.
- No live testing or native/reference acceptance is claimed. Existing KDE
  user-run checks, K5 and the deferred platform ports remain open under their
  existing gates; this completed removal does not close them.

## Deferred documentation staleness

- Doc staleness found, not edited (outside removal): `README.md` v2-era platform
  status and adapter-guide text; `docs/README.md` "normative ... (v3)";
  `docs/dev-loop.md` "core and Windows use v3"; `docs/porting/kde-port-notes.md`
  v2 header and strict-v2 text; `docs/changes/core-interface-review.md` fixture
  reader table (no kwin test reads the non-v4 corpus any more);
  `docs/research/architecture-review/review.md` cites the culled
  `provenance-entry.ts`. `docs/porting/world-contract-v3.md` is slated for
  deletion at cutover by the contract decision; the cutover is incomplete
  while the Windows and macOS ports are pending.
