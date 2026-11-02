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
2. Replace report/reply types and codecs, including held rows, all closed
   catalogs and exact comparisons for every message fixture.
3. Migrate the real world engine: lifetime/settings, raw-fact classification,
   scaled geometry, intent, state, visibility, presentation and settlement.
4. Execute every ordered scenario and assertion against the engine with generic
   offline mechanics for native-model steps; preserve the eight evidence gates.
5. Consolidate inherited contract rules, remove the v3 guide and code paths,
   and run build/test/clippy gates plus adapter build-failure inventory.

- Intent dependency order: validate/canonically encode the core-owned opaque
  inner record first; then wire outer report/reply records and consume them in
  lifetime-fenced engine restore/save/clear. Inner-data tests alone do not prove
  engine restoration, host storage, or complete message codecs.

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
  maximize/fullscreen fields. Native maximize remains the approved contract;
  macOS native Zoom versus core-owned overlay stays an OPEN product decision.
  A later owned overlay would require state/geometry and recovery contract work,
  not a guessed extension of this record. M3 restart binding remains unproved,
  and no FrameWitness change is introduced.
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
  are implemented and fixture-tested; lifetime-fenced engine restore/save/clear
  and host storage remain pending. Engine storage-write authority is still empty.
  The remaining world report/reply paths and version are still staged; no v4
  cutover or full scenario acceptance is claimed. Full adapter build-failure
  inventory remains pending.
