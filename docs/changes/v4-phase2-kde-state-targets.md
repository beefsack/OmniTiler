# V4 phase 2: KDE state targets

## Status

- Active: implementation complete offline; user-run physical ST acceptance passed
  2026-11-10 apart from the deferred recovery sub-legs. The macOS start gate is
  clear. Desktop/stacking and compositor-boundary recovery and full storage/protocol
  proof remain open (the sticky-from-tile placement defect is fixed and
  re-checked 2026-11-10); see the
  [current results and limits](#2026-11-10-physical-state-target-acceptance).

## Goal and scope

- Port core-authorized maximize, fullscreen, native sticky and persistent above
  targets through the KDE producer, native executor, raw attempts and recovery.
- Preserve ordinary tiling, retained tile/float bases, exact lifetime fencing and
  durable native preimages. Leave a usable build at each verified increment.
- Core already owns state policy and correlated target settlement. Apply native
  mechanics only; descriptive command results and float/sticky classification
  are not write authority.
- Exclude scaled metrics, reconnect/rehome, scenario-runner completion and other
  platform ports. Existing native-border continuity, physical drag/resize and
  real-key resize acceptance remain open.

## Acceptance

- Exact authorized property/value/revision attempts, original timestamps,
  immediate nullable pre/post facts and spent-write retention survive delivery
  loss, observation-only revisions and restart fences without blind retry.
- Fullscreen-clear precedes paired maximize; ordinary fullscreen entry/exit
  preserves the maximize overlay and its underlying base.
- Sticky transitions preserve core intent; above acquisition/release uses only
  explicit state authority and restores only lifetime-verified owned preimages,
  including coupled above/below state. Failed storage disables only its lane.
- Restore verified native leftovers before new dependent writes. Native proof
  must distinguish actual readback from setter acceptance and prove ordinary
  tiling remains usable. Unproved native/physical conditions stay explicit.
- Relevant adapter, service and native offline gates pass with immutable legacy
  and v4 wire fixtures unchanged; scoped Rust gates exclude only the documented
  unsafe-posture case when relevant. No full-workspace claim.

## Approach and bounded units

1. Audit target/capture/attempt/preimage mechanics, then port retained raw state
   evidence without enabling native writes. This prerequisite is verified below.
2. Implement the separate internal Planner preimage API and file, reusing the
   existing private storage, namespace and authorization code. Then implement
   asynchronous persist-before-write execution and startup recovery. Remove
   obsolete local policy separately after migrated-caller proof.
3. Review integrated authorization/recovery behavior independently, then prove
   only native transitions that offline tests cannot establish.
4. Record exact can/cannot outcomes, durable decisions and remaining physical
   checks. Archive only when the slice acceptance is satisfied.

## Source audit and remaining dependency

- Baseline: `69d8433c`. Core already produces and settles all four native
  properties. KDE parses targets but does not consume them in `world-apply.ts`.
  Sticky/fullscreen shortcuts and automatic float stacking still invoke local
  policy in `world-native.ts`; maximize still uses descriptive result handling.
  These paths are not v4 state-target acceptance and remain unchanged here.
- `world-connection.ts` still advertises `above:false`. No capability flip,
  shortcut migration, native property write or recovery implementation is part
  of the raw-evidence prerequisite.
- The separate preimage slot is now implemented and verified offline below.
  The existing opaque intent file excludes native state and is not repurposed.
  Shared same-UID/current-KWin authorization, bus/owner namespace and private
  atomic replace remain implementation mechanisms, not native recovery proof.
- At pinned KWin 6.8.1 (`1b798c05c19b1124edb1e16623191fcd2c128fd0`),
  `src/scripting/scripting.cpp` exposes `readConfig` but no script configuration
  writer or generic durable property store; stopping/unloading deletes the
  script and its child JS engine. The same scripting-surface audit at 6.6 was
  consistent. This rules out retaining JS maps across script unload, not every
  possible native storage mechanism. The existing migration records already
  select a separate adapter-owned external preimage slot; its script-to-Planner
  API is internal KDE plumbing, not a world-contract extension. The optional
  native effect cannot own this required storage.
- Native writes wait for the stored reply. Missing or failed replies disable
  only the affected window/property, without blind retry. Next-start recovery
  completes before dependent writes resume; namespace mismatch reads empty.
- Required storage scope remains exact live UUIDs within the same bus/KWin
  lifetime. A compositor restart invalidates this namespace; stale UUIDs must
  not authorize restoration. Native survival and recovery remain unproved.

## Verified raw-evidence prerequisite

- Added `WorldStateAttempt` and canonical encoding for all four properties;
  `stateAttempts` remains required and emits `[]` when empty. The entry now
  drains retained evidence instead of hardcoding an empty lane.
- Core rejects duplicate `(win,property)` rows even with different revisions
  (`world_report.rs`, `duplicate-state-attempt`). Retention therefore uses one
  latest tuple per window/property; repeated identical target/revision retains
  the first facts and original timestamp. New tuples replace the prior tuple.
  Delivered snapshots retire only by exact full-row equality on an accepted
  bound reply, preserving newer tuples and unrelated properties across old
  acknowledgements. Generation reset clears the lane; the bound is 1024 rows.
- New coverage verifies closed vocabulary, both target values, every nullable
  pre/post combination, counter limits, malformed-row refusal, cloning,
  generation reset, bounded retention, delivery loss and exact acknowledgements.
  Production capture/encoding bytes with all four properties pass the real
  strict codec after handshake/settings and yield a ready correlated reply.
  Injected attempts match no core-issued target: this proves decoding, not
  state settlement, setter execution or native completion.
- Strict TypeScript and the full adapter build/test gate pass: 1079/1079 tests,
  zero skips. Independent source review has no outstanding prerequisite finding.
  An initial duplicate `STATE_PROPERTIES` declaration failed typechecking; the
  implementation now reuses the existing vocabulary. Two test assertions were
  strengthened before acceptance; no native semantic approach was attempted.
- Immutable stage5/stage5-v4/world-wire-v4 fixtures, Rust, capabilities, native
  setters, shortcuts and existing acceptance notes remain unchanged. No live
  work ran. The build remains unscoped/non-trace; native transitions, durable
  preimages and startup restoration remain open. Existing native-border and
  physical-input gates are neither changed nor closed.

## Verified separate preimage storage

- Internal `ReadNativePreimage`/`WriteNativePreimage` methods persist a separate
  version-1 `native-preimage.json` snapshot. The existing intent-store anchored
  no-follow filesystem helpers accept the separate filename; namespace and
  authorization reuse the existing service code. Opaque intent semantics stay
  unchanged. No core, shared protocol, wire fixture or native effect changes.
- One record per UUID carries nullable `maximize_mode`, `fullscreen`,
  `desktops` and paired `stacking{above,below}`. Null means untouched; an empty
  desktop array means all-desktops. Exact maximize axes and coupled stacking
  facts are retained. Only the executor may arm relevant slots or authorize
  restoration after an exact live UUID join; the service returns storage, not
  restoration authority.
- Review corrected the initial all-property snapshot because it lacked
  untouched-property ownership, and corrected opaque-intent version leakage on
  preimage rejections. Pinned KWin rule checks can retain both above and below;
  the store therefore preserves that raw pair rather than rejecting it as
  impossible. A test-local live filter was removed as non-production proof.
- Private-bus legs actually execute: current-owner coupled roundtrip and opaque
  intent coexistence, non-owner rejection, unavailable namespace, new-owner
  mismatch-empty and linked-slot refusal without touching foreign bytes.
  Additional codec/filesystem tests cover partial records, null versus empty,
  closed shapes, bounds, duplicate/normalized identities and private modes.
- Independent security/source review has no serious unresolved finding. Scoped
  core/protocol/service tests, locked build, formatting and all-target clippy
  with warnings denied pass; only `workspace_unsafe_posture` is excluded.
  Service unit tests pass 219/219. No native property execution or restoration
  has run; the client executor and its persist-before-write gates remain next.

## Verified state-target revision correlation

- The original exact-revision rule ignored an R+1 attempt when the issuing reply
  R was lost, despite a Controlled republished target. StateTarget carries no
  original revision; commandless above has no command sequence to recover it.
- Core now correlates `binding.target.revision <= attempt.revision <= latest
  published revision`, retaining exact window/property/value/capability and
  first-evidence/spent rules. Target replacement or withdrawal fences obsolete
  evidence. No counter, history, reply-authority gate, wire, codec or fixture
  change is needed.
- The adapter acts only on a Controlled property row in its held reply; settle
  correlates evidence of a write already performed, not permission to perform
  it. Hold-stamped evidence records/spends without creating authority or a new
  publication. Earlier authorized evidence survives held receipt. The user
  approved this contract clarification on 2026-11-08.
- Fullscreen commands can mint a binding on incomplete inventory while the
  independent publication predicate emits Hold at that same revision. A new
  regression passes under both the original exact check and the interval,
  confirming that accepting such raw evidence is existing settlement behavior.
- Real fullscreen and commandless-above republish tests fail under the original
  exact check and pass under the interval. Regressions also cover initial and
  republished Hold evidence, held receipt and record-free recovery, obsolete
  true/false/true stamps, original/future bounds and immutable first facts/stamp.
  Initial synthetic coverage was replaced with real producer/publication paths;
  mislabeled held-receipt coverage was corrected before acceptance.
- Scoped core/protocol/service tests, locked build, formatting and all-target
  clippy with warnings denied pass; only `workspace_unsafe_posture` is excluded.
  Core unit tests pass 1979/1979. Independent source/contract review has no
  unresolved finding. Immutable window-state and other fixture bytes remain
  unchanged. Adapter Hold/Untouched no-write coverage, native execution and
  recovery remain next; no live work has run for this increment.

## Verified native preimage client prerequisite

- Added the bounded v1 ReadNativePreimage/WriteNativePreimage client separately
  from opaque intent. A usable exact-generation seed precedes all mutation and
  persistence; failed or missing seed disables native lanes without retry.
  Nullable property slots and coupled stacking survive strict closed-shape import.
- Full snapshots serialize without losing other armed properties. Each requested
  lane's immutable preimage is checked before dispatch and after the exact
  correlated stored acknowledgement. Changed, missing, disabled or stale lanes
  cannot receive success. Empty-lane flushes are housekeeping, not native-write
  permission; callers must still revalidate native identity and core authority.
- Failed writes disable only their lanes. Property clear and unrelated successful
  snapshots never resume them; a queued failed-lane duplicate does not dispatch.
  Whole-window cleanup is reserved for an established ended lifetime.
- Review rejected pre-seed mutation, stale queued-generation relabeling and
  count-only acknowledgement as insufficient write prerequisites. These are
  corrected and covered, as are runtime slot validation and strict string types
  before native-id normalization. Independent review has no unresolved finding.
- Typechecking, 1109/1109 adapter tests with zero skips, and the unscoped/non-trace
  build pass. The new client has no production caller yet: no capability,
  shortcut, native setter, wire fixture or live behavior changed. Executor and
  startup-recovery integration remain next; this is not native durability proof.

## Verified native state executor prerequisite

- The executor remains unwired. Initial review found
  commandless above resume suppressed, lagging applied state preventing later
  exits, unbounded spent tracking and invented attempts for skipped writes.
  A correction replaced those paths with captured-pre guards, bounded latest
  markers, silent pre-call cancellation and per-lane revalidation.
- The corrected prototype passes 1138 adapter tests, but independent probes
  still reproduce serious failures: a mixed Hold/Controlled or
  Untouched/Controlled row erases the held property's spent marker, allowing a
  second fullscreen write and replacing its original evidence; a reentrant
  setter changes the acted-on revision from 5 to 6; refused above acquisition
  creates ownership that permits a subsequent release write.
- A third approach rewrote the executor around one spent identity per
  window/property, presence at any authority, and frozen evidence installed
  before invocation. Hold/Untouched preserve spent; an accepted listed-window
  row lacking the property clears it, including empty state. Whole-window
  absence does not clear it. Reentrant completion cannot overwrite newer spent;
  refused or throwing above acquisition creates no ownership.
- Source checks resolve retained-binding gaps: above latches never reopen in
  place, and capability/route changes replace the engine generation. A core
  regression covers an unsettled latched acquire and late attempt evidence.
  Adapter regressions reproduce all seven original failures before correction.
- Follow-up review found missing fences between stacking-restore subcalls and
  completion crossing reset/prune. Corrections revalidate each native call,
  fence old-generation completion, and preserve actual same-generation evidence
  without resurrecting ownership. A global prune fence initially suppressed
  ownership on no-op or unrelated pruning; transient in-flight lane cancellation
  now scopes that fence to the ended lifetime. These cases have regressions.
- The final executor is 1089 lines, down from the 1223-line prototype; shared
  slot helpers and actual ownership identity replace redundant tracking. No
  history or numeric retry counters are needed. Independent follow-up probes
  leave no serious unresolved finding. Typechecking, 1153/1153 adapter tests
  with zero skips, the unscoped/non-trace build, formatting and all 22 above
  core tests pass. This is offline executor acceptance, not production or native
  durability proof. No setters, capabilities or shortcuts are migrated;
  startup recovery and live proof remain next. The accepted interval and
  storage-client prerequisites remain unchanged.

## Verified staged startup recovery

- Added a startup-only recovery coordinator for the seeded touched slots.
  Exact live UUID joins and native desktop-reference resolution restore original
  maximize axes, fullscreen, desktop memberships and coupled stacking. Null
  slots remain untouched; runtime arms never become startup recovery entries.
- Setter acceptance alone cannot complete recovery. A later current capture
  must match the original, then a single housekeeping snapshot must be stored
  before the lane unblocks. Failed, missing or stale acknowledgements leave
  that lane blocked without retry; ordinary tiling does not consult this gate.
- Review corrected incomplete desktop restoration, same-watermark observation
  acceptance and housekeeping retries. Explicit post-batch invalidation requests
  a fresh capture even when a no-op setter emits no native signal. Generation,
  settings, accepted-revision and watermark checks fence native calls; entry
  identity fences reset/prune completion. Malformed live sets cannot prove
  lifetime end, and normalized live UUIDs preserve matching recovery records.
- The entry constructs and tears down the store/coordinator, but does not seed
  the store or invoke recovery yet. Activation remains atomic with disabling
  legacy writers. An initial read-seed insertion changed bus/timer gates; the
  staged seam has no such traffic and leaves existing writer behavior intact.
- Independent follow-up review and executable probes leave no serious finding.
  Typechecking, 1177/1177 adapter tests with zero skips and the unscoped/non-trace
  build pass. The recovery module is 732 lines. Startup/native restoration is
  still unproved live; migration and integrated authorization review remain next.

## Verified atomic migration

- The migration enables the scoped executor/recovery hosts and
  above capability, queues sticky/fullscreen commands and removes the legacy
  result-driven maximize and automatic-stacking callers. Dormant helpers have
  not been culled. Native durability and transitions remain live acceptance work.
- Fourteen new production-entry tests cover persist-before-write, raw attempts,
  recovery ordering, scope and callback fences. A regression reproduced runtime
  arming during a pending recovery clear, which permanently blocked the lane;
  the executor now checks current authority/recovery readiness before arming as
  well as before the setter.
- Integrated review reproduced deletion of a live foreign window's durable
  preimage: a complete scoped observation excludes foreign windows, so its
  absence cannot prove lifetime end. Native cleanup now uses a separately
  validated full native UUID roster as well as the complete fresh-capture gate.
  Malformed/duplicate/unreadable inventories over-retain; live foreign records
  survive every persisted snapshot. Proven-ended runtime slots are cleared and
  persisted. Red/green production-entry tests cover all of these cases.
- Regenerated the five KDE byte goldens through the genuine entry/Rust path.
  Their complete diff is the above capability, removal of local `win-fixed`
  above facts in three reports and its Controlled above target in the Observe
  reply. The retired automatic-stacking admission previously wrote the native
  flag from fixed-float classification; the observer now reports its unchanged
  false value. The unmodified byte oracle passes 6/6; typechecking and the
  unscoped/non-trace build pass.
- Updated only the full/s2 discovery handshake requests in stage5-v2 from fresh
  genuine recordings: `above:false` becomes `above:true`. This is the
  current-production recording lineage, updated because the adapter now owns
  the recoverable above lane. Every other stage5-v2 byte remains unchanged;
  stage5, stage5-v4 and world-wire/v4 remain immutable. The unchanged final
  proofs compare all other envelopes/projections/dispatches with above enabled;
  all 31 stage5 checks pass without masking, overrides or relaxed assertions.
- Independent follow-up review leaves no serious unresolved finding. Final
  typechecking, 1191/1191 adapter tests with zero skips and the unscoped/non-trace
  build pass. Scoped core/protocol/service tests, locked build, formatting and
  all-target clippy with warnings denied pass; only the documented
  `workspace_unsafe_posture` case is excluded. All 38 native-effect offline tests
  pass in a fresh build. Separate obsolete-helper cull and fixture-only native
  proof remain next. No live testing has run.

## Verified obsolete-helper cull

- Removed dormant local sticky/fullscreen/maximize setters, result-driven
  maximize handling, pending-native settlement and automatic-above ownership.
  Caller closure proves their only remaining cancellation/release callers were
  no-ops: no production path could populate the removed state.
- Preserved native float/B9 maximized-unfloat handling and the apply host. B9
  invokes its maximize clear directly, not through the removed maximize helper.
  Migrated entry/executor/recovery scenario coverage remains unchanged; only
  helper-only tests and dead environment fields were removed.
- The five-file cull removes 1748 net lines. Typechecking, 1148/1148 adapter tests
  with zero skips and the unscoped/non-trace build pass. Fixtures, Rust and
  native-effect sources remain unchanged. Fixture-only native proof remains
  next; no live testing has run.

## Native proof blocked by storage admission

- A fixture-only run established exact sentinel/PID/native-UUID joins, scoped
  controller evaluation and accepted two-window layout dispatches. Native state
  transitions and startup restoration were not exercised, so their acceptance
  remains open.
- The generation-bound entry starts `ReadFloatIntent` and `ReadNativePreimage`
  back-to-back. Both share the service's non-queuing `intent_lock`, held across
  asynchronous credential/namespace checks. The captured native seed call
  received `Unavailable: planner is busy` while the intent seed was outstanding.
  KWin did not deliver that D-Bus error to the script callback; the native seed
  subsequently timed out and correctly disabled native lanes without retry.
- Source review confirms the admission collision occurs before reading the
  native preimage file; the separate intent seed's degraded-corrupt result does
  not establish its cause. The same shared admission covers both stores' writes,
  so a correction must address cross-store contention beyond startup timing.
- Controller, Planner and fixture teardown completed, dev status is down and
  the unscoped/non-trace bundle is restored. No native transition, persistence
  or recovery pass is claimed; this note remains active.

## Verified independent storage admission

- Opaque intent and native preimage now use separate service admission locks.
  Each remains bounded, non-queuing and shared across endpoint clones, with
  unchanged same-UID/current-KWin authorization. Both remain independent of the
  world lock. Distinct files and atomic-replace temporary prefixes require no
  cross-store exclusion; no adapter coordination, retries or wire changes were
  introduced.
- A private-bus regression fails before the correction: while the intent lock
  is held across real admission/authorization calls, the native read receives
  busy. It passes after the split while the same-store read still receives
  busy. Independent review finds no serious unresolved correction finding.
- Client audit does not establish one total outstanding call per store across
  generations. Seed supersession cannot cancel a host RPC; a new seed can overlap
  an old write. Intent persists also permit concurrent writes. Native flushes
  serialize until acknowledgement or deadline, but a timed-out RPC can outlive
  that deadline. Same-store busy still degrades/disables native lanes or fails
  intent persistence, without gating ordinary tiling. An old native write's
  timeout can disable its lanes after generation rotation. Occurrence frequency
  is unmeasured; no additional coordination or retry mechanism was added.
- Pinned KWin 6.8.1 scripting source logs D-Bus errors and returns without
  invoking the JS callback; there is no error-callback parameter. Immediate
  correlated error settlement is therefore unavailable through this API. The
  existing bounded timeout and fail-closed result remain unchanged.
- Formatting, scoped core/protocol/service tests, locked build and all-target
  clippy with warnings denied pass; only `workspace_unsafe_posture` is excluded.
  Adapter typechecking, 1148/1148 tests with zero skips and the unscoped/non-trace
  build pass. Native state transitions and startup recovery remain live proof
  work, not established by these offline gates.

## Corrected-code observation; native proof incomplete

- Fresh occupancy included sticky shell windows across the entire existing
  desktop roster; the fixture workspace was appended beyond that roster.
  Native fixture facts were collected only after the exact fixture-class check.
- The helper lifecycle gate failed: it invoked the exact script's `stop`, then
  required `unloadScript` to return true; the latter returned false. Pinned
  KWin 6.8.1 source shows `AbstractScript::stop()` schedules deletion and
  `unloadScript()` returns false when the named script is already absent.
  Subsequent absence checks do not make the failed run accepted evidence.
- The retained corrected-code startup capture contains a correlated successful
  native preimage read and positive `preimage-seed outcome=ok` line, alongside
  the existing handled degraded-corrupt intent read. This is an observation,
  not accepted live feature proof. No native state toggle or recovery journey
  ran, and none of their remaining acceptance conditions is closed.
- Exact controller, Planner, fixture and capture teardown completed; dev status
  is down and the unscoped/non-trace bundle is restored. The native proof remains
  incomplete and this note remains active.

## Fixture native results (2026-11-08)

- PASS, entry: on an appended workspace, fixture-focused ydotool input reached
  the actual maximize/fullscreen callbacks. Native maximize axes changed from
  zero to full maximize, then fullscreen became true with an output-sized frame.
  Input was injected through libinput/xkb, not physical-key acceptance.
- FAIL, exit: ordinary tile -> maximize-on -> fullscreen-on -> fullscreen-off
  returned `state-unknown`; maximize-off returned the same failure. Native
  fullscreen/maximize remained set. This reproducible exit defect is recorded
  only; no product fix was attempted. Raw basis: `leg2-*` native captures and
  `leg2-state-journal-full.log` command/result lines.
- PASS, bounded startup recovery: one orderly production stop/start seeded the
  stored maximize/fullscreen preimage, restored both original properties, then
  logged newer-observation matches and acknowledged persisted slot clears.
  Final independent native readback had fullscreen false, both maximize axes
  zero and the original tiled frames. This proves these two touched-property
  restores, not desktop/stacking recovery or compositor-restart recovery.
- PASS, sticky/above acquisition: an independent ordinary-base sticky-on
  callback produced native all-desktop membership and keep-above true.
  FAIL, sticky release: sticky-off returned `scope-held`; presentation held the
  output as `incomplete-inventory` and both fixture rows as unreadable, while
  native sticky/above remained set. Raw basis: `host-extra-leg2-*` native,
  presentation and callback journal captures. Cause is not established.
- Ending that exact sticky fixture lifetime cleared the hold; the surviving
  ordinary fixture reflowed and later shared-boundary resize callbacks succeeded.
  This is lifetime cleanup, not successful sticky-off or above release.
- NOT RUN: independent float on/off, above release and sticky/stacking startup
  restoration. The earlier float/sticky refusals while fullscreen was stuck are
  not independent state-target evidence. Physical-key acceptance remains open.
- Cleanup passed: fixtures, owned controller/Planner and appended workspace
  removed, original current workspace restored, unscoped non-trace bundle
  rebuilt; final production status DOWN. Both defects remain unfixed and this
  note remains active.

## Exit and sticky-release diagnosis

- Fullscreen exit is blocked by the KDE encoder's unconditional
  `fullscreenExitable:null` (`world-report.ts`, `encodeV4WindowRow`). Core
  requires positive exitability for fullscreen-off and maximize's fullscreen
  clear. The captured command failures precede native preimage or setter work.
  At the pinned KWin source, `fullScreenable` is script-readable and gates the
  XDG fullscreen setter in both directions. Reporting this nullable native fact
  fits the existing neutral-path availability contract; it is not completion
  proof and does not bypass rules or readback.
- Sticky all-desktop membership truthfully has no single `nativeWs`. The v4
  core adoption fence (`world_run_v4.rs`) treats this missing receipt as unknown
  and holds the retained home; `sticky_pregates` then refuses release. Output
  publication maps any held scope to `incomplete-inventory`, so that presentation
  label is not evidence of a separate adapter desktop-enumeration failure.
- The known all-desktop membership decision in
  [layout and admission](../decisions/layout-and-admission.md#native-hidden-representation-and-unknown-sticky-membership)
  now preserves an already retained same-lifetime home without fencing its
  scope, regardless of native or core sticky origin. Unknown/missing membership
  and newcomers without a home keep existing fences. No current-desktop receipt
  may be fabricated.
- These are source diagnoses supported by the retained callback and presentation
  captures, not new regression or native acceptance evidence. Corrections and
  their separate verification appear below.

## Verified fullscreen-exitability correction

- KDE now captures nullable native `fullScreenable` and reports it as
  `fullscreenExitable`. `fullscreenOwner` remains unknown. Missing, malformed
  and throwing native reads remain null without holding an otherwise usable
  report. Core exit policy, native execution and sticky behavior are unchanged.
- Eight production capture/encoding regressions cover positive facts on ordinary,
  fullscreen and maximized windows, false, unavailable and malformed reads,
  throwing getters and absent ownership. Before the correction, four positive
  or negative passthrough assertions fail with the hardcoded null; after it,
  all 1168 adapter tests pass with zero skips.
- Independent source review leaves no blocking finding. Scoped formatting,
  core/protocol/service tests, locked build and all-target clippy with warnings
  denied pass; only `workspace_unsafe_posture` is excluded. Adapter typechecking,
  tests and the unscoped/non-trace build pass. No native-effect or fixture bytes
  changed. Native exit and independent float on/off re-checks remain pending;
  sticky release is addressed by the separate correction below.

## Fixture exit and float re-check (2026-11-08)

- PASS, exit: ordinary tile -> maximize-on -> fullscreen-on -> fullscreen-off
  retained full maximize after clearing native fullscreen; maximize-off then
  cleared both native maximize axes and restored the original tiled frame.
  The ordinary companion retained its tile throughout. Raw basis:
  `base-native-a.json`, `maxon-native-a.json`, `fson-native-a.json`,
  `fsoff-native-a.json`, `maxoff-native-a.json`, companion captures and
  `journal-after-exit.log`. Actual shortcut callbacks dispatched all four
  commands and produced native-state work, without `state-unknown` failures.
- PASS, independent float and above release: from the restored ordinary base,
  float-on produced a floating presentation row and native keep-above true;
  float-off returned the subject to tiled presentation and native keep-above
  false. The companion expanded while the subject floated and both re-tiled
  on release. Raw basis: `floaton-*`, `floatoff-*` native/presentation captures
  and `journal-after-float.log`. This is not sticky release evidence.
- Exact fixture sentinel/PID/native UUID joins preceded transition input.
  Initial pre-map enumeration did not find the fixtures; the post-map native
  joins were collected after controller startup. A pointer-center focus attempt
  did not focus either fixture and sent no command chord. Native activation of
  the exact joined fixture then focused it before each of the six chords,
  confirmed by fixture readback. State commands used injected ydotool input,
  not physical-key acceptance. No full-native-roster uniqueness is claimed.
- Cleanup passed: owned controller, Planner, fixture and input daemon stopped;
  fixture/input sockets removed; appended workspace removed and original
  current workspace restored. The unscoped non-trace bundle was rebuilt and
  final production status was DOWN. No sticky-on ran; sticky release remained
  unverified in this leg, as did sticky/stacking startup restoration
  and physical-key acceptance. This note remains active.

## Verified sticky-release correction

- The v4 adoption fence now exempts positive all-desktop membership only when
  the same lifetime already has a retained home. It neither fabricates a logical
  receipt nor branches on sticky origin. Unknown membership, homeless newcomers,
  native-order conflict and independent inventory/held-window gates stay intact.
- Before the correction, both core-on/off and native-set-sticky/off regressions
  reach the actual off command and fail with `ScopeHeld`. After the correction,
  all five targeted tests pass: off publishes Controlled native clear authority,
  correlated clear settles, prior tiled membership fresh-admits, prior float
  remains float, and the tiled sibling remains readable. Unknown false/null
  sticky membership and all three missing-receipt spellings retain their fences;
  an all-desktop newcomer invents no home.
- Independent review has no blocking finding. The legacy accepted-run fence is
  not changed by this v4 correction; its removal belongs to the separate legacy
  retirement work. No obsolete mechanism with proven caller closure was found
  for a separate cull. Wire, codec, history and fixture bytes are unchanged.
- Scoped formatting, core/protocol/service tests, locked build and all-target
  clippy with warnings denied pass; only `workspace_unsafe_posture` is excluded.
  Core unit tests pass 2018/2018. Adapter typechecking, 1168/1168 tests with zero
  skips and the unscoped/non-trace build pass. Native sticky release and bounded
  sticky/stacking startup restoration remain the next evidence gates; no live
  pass is claimed for this correction yet.

## Fixture sticky-release re-check (2026-11-08)

- PASS, observed native roundtrip: injected `Meta+Shift+G` made the tiled
  subject all-desktop with keep-above true, then returned it to its single
  native home with keep-above false. The subject returned to tiled presentation
  with an allocation matching native readback; the companion remained tiled
  and Controlled, expanding while the subject floated and contracting on release.
  Both output scopes remained Controlled. Raw basis: `native-pre-A.json`,
  `native-after1-A.txt`, `native-after2-A.txt`, matching companion captures,
  `present-before1.txt`, `present-after1b.txt`, `present-after2.txt` and
  `journal-chord1.txt` / `journal-chord2.txt`. Spent state rows still carry
  property-level holds; they did not hold the scope or refuse release.
- FAIL, sticky-intent carry across orderly stop/start: after another sticky-on,
  the subject returned on one native desktop, still floating and above, with no
  sticky target in the retained post-start presentation. The journal reports
  desktop and stacking preimage restoration, slot clears and stored completion,
  followed by runtime above acquisition. Restoring original native preimages
  is distinct from resuming sticky intent; no cause or new policy is inferred.
  Raw basis: `native-preoff-A.txt`, `native-postrestart-A.txt`,
  `present-preoff.txt`, `present-postrestart.txt` and `journal-devon2.txt`.
- Evidence limits: both starts logged ready/discovered and matched
  dispatch/applied under the same compositor lifetime. Exact sentinel/PID joins
  used only the two fixture UUID candidates and `getWindowInfo`; no full-roster
  uniqueness claim. Focus readbacks before the three chords were checked but
  not retained individually; desktop away/back supplied focus rather than exact
  window activation. Input was injected, not physical-key acceptance. No complete
  correlated D-Bus capture ran, so storage/protocol acceptance remains unproved
  despite the journal and native observations. This leg is not full live
  acceptance and does not close physical input or sticky restart persistence.
- Setup initially placed fixtures on the original desktop; they were shut down
  and recreated on the appended workspace before the command journeys. A
  pre-effect desktop-create signature error was corrected. Neither production
  start nor its evaluated-readiness gate failed; no retry around an inert
  controller ran. External intent-store access was rejected as non-owner;
  periodic non-activatable-name errors and load-time declaration warnings were
  retained without attributing them to the corrected fence.
- Cleanup verified: controller/Planner DOWN, fixture and input processes/sockets
  absent, exact appended desktop removed and original roster/current restored;
  the unscoped non-trace bundle was rebuilt. Sticky-intent resumption needs a
  separate bounded diagnosis, and complete capture/focus evidence remains open.
  This broader state-target note stays active.

## Verified sticky restart correction (offline)

- Required outcome follows the opaque-intent contract: non-null `stickyPrior`
  restores sticky-float intent, including native all-desktop membership. Startup
  recovery first restores touched native originals; core then reacquires sticky
  and above through normal authorized targets. Sticky-off retains the saved
  prior-mode semantics. Composed maximize's runtime-only restart rule does not
  apply to sticky.
- Source diagnosis: opaque import restores semantic float and sticky provenance,
  but native sticky publication requires a retained binding that import does not
  establish. Native desktop recovery therefore removes all-desktop membership
  while ordinary float policy reacquires above. This is a defect, not a selected
  partial restoration outcome.
- Healthy sticky import now establishes the same native sticky-on binding as
  live commands, absent-only on both admission routes. Commandless restoration
  publishes null command correlation; live commands retain their exact sequence.
  Existing property authority, spent and settlement gates remain unchanged.
  Refusal restores the imported provenance and saved record without retry.
- RED: a real sticky-on save replayed into a fresh owner restored float but
  emitted no native sticky target. GREEN: six restart regressions cover prior
  tile/float release, false/true native facts, unknown-to-readable import,
  floating-workspace admission, composed no-target behavior, proven refusal
  and ambiguous evidence without retry. All 117 sticky tests pass.
- Initial binding creation omitted the existing incomplete-saved-knowledge gate;
  three coherence tests caught that and the shared gate now remains in force.
  Review corrected invented command correlation and refusal rollback provenance
  before acceptance; independent follow-up review has no unresolved finding.
- Seven scoped gates pass: core/protocol/service formatting, locked offline
  tests and build, all-target clippy with warnings denied, adapter typechecking,
  1168/1168 tests with zero skips and unscoped non-trace build. Core unit tests
  pass 2024/2024; only `workspace_unsafe_posture` is excluded. No wire, codec,
  history, preimage API or fixture bytes changed. Shared binding construction
  replaces duplicated live-on inserts; no further obsolete caller closure was
  established for a separate cull.
- Native restart/off evidence follows below. Offline gates do not establish
  protocol/storage or physical-input acceptance; the broader note remains active.

## Sticky restart live re-check stopped before commands

- The fixture-only controller started on an appended workspace, but the startup
  journal capture used the service launcher PID rather than the KWin D-Bus-owner
  PID and returned no entries. The readiness evidence was invalid; no sticky
  chord, restart journey or post-restart sticky-off ran.
- Diagnostic recapture under the verified owner PID shows the matching adapter,
  generation and dispatch/applied pairs. The original cursor was older than the
  run, so this does not retroactively accept the failed readiness capture.
  Correlated discovery, both store seeds and Describe replies were retained;
  the monitor also captured unrelated bus traffic despite its intended filters.
  No sticky persistence or focus/input acceptance is claimed.
- Cleanup initially misread a scalar desktop property as an array. The corrected
  scalar read restored the original current desktop; the exact appended desktop
  was removed. Fixture, input daemon and monitor processes and sockets are absent,
  the original desktop roster is retained, the unscoped non-trace bundle is
  rebuilt and production status is DOWN. KWin's owner remained unchanged.
- At that stop, native restart/off, retained pre-chord focus and complete
  protocol capture remained unproved. The offline correction stayed verified;
  the bounded native re-check below does not make protocol claims.

## Fixture sticky restart/off re-check (2026-11-08)

- PASS, bounded native journey: tiled A -> sticky-on -> one orderly production
  stop/start -> sticky-off. Before and after restart, A had native all-desktop
  membership, keep-above true and floating presentation with sticky intent.
  Sticky-off returned A to its single native home, keep-above false and fresh
  tiled admission. B remained readable and tiled, expanding while A floated and
  contracting on release. Both output scopes remained Controlled. Prior-float
  restart/off is covered offline, not by this native leg.
- Exact sentinel/PID/native UUID joins preceded both chords. Exact native
  activation and retained `focus-before-on.json` / `focus-before-off.json` show
  A focused immediately before each injected `Meta+Shift+G`. This is not
  physical-key acceptance or full-native-roster UUID uniqueness proof.
- Both starts logged ready/discovered and matched dispatch/applied after fresh
  user-journal cursors under the verified KWin D-Bus-owner PID. The restart
  journal reports desktop and stacking preimage restoration and completion
  before renewed above/sticky work. Independent native readbacks and presentation
  confirm the final restored state; transient native recovery values were not
  sampled independently. Raw basis: `before-*`, `on-*`, `restored-*`, `off-*`,
  `start1-journal.txt`, `start2-journal.txt` and `chords1-journal.txt` /
  `chords2-journal.txt`.
- Planner diagnostics came from the verified owners' redirected output logs
  (`planner1-output.log`, `planner2-output.log`); their journal streams were empty.
  No bus monitor ran during the chords/restart. No protocol request/reply,
  storage-ACK or no-pending-operation claim is made from these diagnostics or
  `ReadWorldPresentation`. Spent property rows retained holds without holding
  the output scopes or blocking sticky-off. Known load-time declaration warnings
  and non-activatable-name errors remained; no evaluation exception was observed.
- Capture defect: both over-captured raw monitor logs were deleted without
  further extraction and are not acceptance evidence. The latest monitor had
  remained running after overcapture detection; it was stopped before any chord.
  No replacement monitor or additional filter attempt ran. A fixture credential
  was inadvertently printed during inspection; cleanup shut down its endpoint.
  No credential is included in repository records.
- Cleanup verified: owned controller/Planner, fixture and input processes are
  absent; fixture/input sockets and the exact appended desktop are removed;
  original desktop roster/current are restored. KWin's owner is unchanged, the
  unscoped non-trace bundle is rebuilt and final production status is DOWN.
  This closes the bounded prior-tile native restart/off observation, not the
  broader state-target slice, protocol/storage proof or physical-input checks.

## 2026-11-09 physical state-target triage

- ST **FAIL; macOS start gate open**. User-run physical keys in an unscoped
  development session: maximize did nothing, including after Authentic Apply;
  independent float on/off reported PASS; sticky failed from tile and float,
  with sibling expansion but no cross-workspace visibility or apparent release.
  A later prior-float journey also reported rejected unfloat after failed sticky.
  Remaining user-run ST steps (reported as steps 4-6) were NOT RUN; no additional
  sub-leg outcomes are inferred from that numbering. No new native journey or
  code correction is evidence from this investigation.
- Source baseline `420a84f6`; documentation parent `874bd25b` changes the RC
  ordering only: RC gates the macOS reconnect/rehome slice and 0.1, not port
  start. ST still gates start. Existing injected fixture passes above are
  bounded native observations, not acceptance of these physical journeys.
- Evidence: unchanged, externally preserved copies; original/copy SHA-256
  equality was supplied with the logs. Investigation independently read and
  hashed the copies. Basenames below are sufficient to identify them without
  retaining local paths. Combined streams are interleaved, not a global clock.

| Log | Lines | SHA-256 | Bounded evidence |
| --- | ---: | --- | --- |
| `omnitiler-dev.puCu01.log` (M1) | 813 | `1e9a141341cd033862a043efa6a57dd23610d36b2ce31f7d21bcf8eb8e83d620` | 11 maximize shortcut-label commands plus one fullscreen (:497); 50 stale state drops: 34 maximized, 12 fullscreen, 4 above |
| `omnitiler-dev.HOFkEy.log` (S1) | 272 | `64c7de42cfb5428371d7b3ad9693d156dec523a702f4221c68e70bd74329f771` | Sticky commands :115,238; 22 stale drops, 11 above/sticky pairs |
| `omnitiler-dev.0uUT7s.log` (M2) | 210 | `d120ab44895303988aa81897e29666c2a365186b0cf05d8355073124762495ba` | User reports two failed Meta+M and two failed Meta+F11; commands :41,82,109,136; 12 stale drops total (first per chord :59,86,113,140; observe drops :64,91,118,145,161,179,192,206) |
| `omnitiler-dev.dVXwsP.log` (S2) | 116 | `2b4a0f9f179a03c8b3b9366964c894eb615973649ab62354d1e611e90497f451` | Prior-float sticky failure/rejected unfloat reported; two captured sticky commands :57,90; 10 stale drops, paired above/sticky at :62-63,69-70,95-96,101-102,112-113; no captured toggle-float or refusal |

### First evidenced failure: native preimage seed, not key delivery

- **Proven service-side condition:** all four logs :6 report
  `op=preimage-read correlation=preimage-seed-1 outcome=degraded stored=0 returned=0 reason=namespace-mismatch`.
  `native_preimage_store.rs:386-401` in `crates/omnitiler/src` rejects a stored
  bus-ID or KWin-owner mismatch before interpreting any old records; the service
  returns that degraded-empty outcome (`planner_service.rs:1127-1142`). This
  proves an incompatible stored namespace, not which identity changed or why.
  No old UUID is restoration authority in the new namespace.
- **Source-proven disabling mechanism:** this is not the usable empty seed
  obtained when no file exists (`native_preimage_store.rs:263-282`). The client
  accepts only `outcome=ok` as usable (`kwin/src/world-native-preimage.ts:523-570`);
  every degraded seed clears retained rows and sets `seedUsable=false`
  (:438-442). A lost callback instead times out degraded (:467-475). Availability
  requires a usable, exact-generation seed (:840-845). Recovery consequently
  blocks the lane (`world-native-recovery.ts:228-249,718-734`). The production
  authorization callback rejects recovery-blocked/unavailable lanes
  (`world-entry.ts:2044-2055`), producing the executor's pre-arm `stale` drop
  (`world-native-state.ts:983-999`). These checks are shared by maximize,
  fullscreen, sticky and above, independent of physical versus injected input.
- **Evidence limit:** the KWin seed settlement and exact first false authorization
  clause were not captured. Degraded callback receipt versus timeout remains
  unproved; other simultaneous settings/gesture/lifetime failures are not
  excluded. The observed service verdict plus deterministic unavailable-seed
  path is the leading common cause, not proof that every `stale` line names
  namespace mismatch. `stale` also covers later fences (:702-720,854-859).
- All four captures have no `native-state-pending`/`native-state-refused` or
  Planner `preimage-write` traffic. Repeated drops before `applied`, including
  commandless observations, strongly locate rejection before arm/flush, not in
  KWin's all-desktop setter or correlated native completion. `world:applied`
  has no native-state counter (`world-entry.ts:4607-4668`); it is not state-write
  or settlement evidence. Readable target publication and geometry work can
  continue while the native lane is disabled.
- The disablement cannot repair its own stale namespace: same-generation seed
  reads do not retry (`world-native-preimage.ts:451-454`), arm requires usable
  seed (:863-866), and flush refuses unusable seed (:1046-1051). A later start
  reading the same incompatible file degrades again. Do not delete storage or
  bypass persist-before-write as a diagnostic workaround. Selection of a safe
  namespace-boundary correction is separate work, preserving stale-UUID fences.
- **Maximize/fullscreen:** M1 :157-167,184-194 and M2 :41-65,82-146 establish
  callback-label command -> queue/dispatch -> native `stale` drop. Thus missing
  registration, wrong action mapping or foreign holder is not the explanation
  for these delivered presses. Settings can display valid bindings and Authentic
  Apply cannot cure a disabled native preimage lane. No at-press holder capture
  exists to classify any additional silent press. The runner holder preflight
  (`scripts/kde-scenarios.mjs:2095-2137`) covers selection digits only, not the
  state chords; its success is not a state-shortcut ownership proof.

### Sticky release and reported rejected unfloat

- **Acquisition:** S1 :115-129 and S2 :57-71 show sticky commands, stored opaque
  intent, sibling geometry work and repeated above/sticky `stale` drops. Core
  sticky-on changes semantic float membership/provenance before native proof
  (`crates/tiler-core/src/world_sticky.rs:493-576,1007-1065`). This explains
  removal from the tree and sibling expansion without native all-desktop
  visibility; unchanged subject position is not evidence that no command ran.
- **Release mechanism, source-proven:** with readable native facts, retained
  sticky provenance makes `sticky_current` true even if native sticky is still
  false (:74-98). Off may replace the unattempted on-binding with a fresh
  `Sticky=false` binding and saved prior mode (:310-343,579-668); it does not
  queue behind the old binding. Dependent runtime release/retile waits for a
  spent, Applied clear and current non-sticky fact (:1454-1490,1546-1600).
  Silent pre-arm drops create no attempt or spent marker
  (`kwin/src/world-native-state.ts:680-684,990-999`). A blocked clear can
  therefore leave semantic sticky-float retained, not an in-flight native call.
- **Ranked release explanation:** (1) off was issued but also dropped at the
  unavailable-seed gate, so required clear proof never arrived; (2) an independent
  core/readability/scope refusal prevented off, leaving the earlier target.
  S1 :238-265 and S2 :90-114 prove the second sticky command and continued
  Controlled state publication, but summaries omit values, command correlation
  and command results. They cannot distinguish these alternatives. Neither
  native setter refusal nor a new core correlation defect is established.
- **Rejected unfloat remains reported FAIL, cause unresolved:** S2 has no
  `toggle-float`, `shortcut-failed`, `command-refused`, `command-busy`,
  `native-fenced` or `float-refused-*` line. This does not disprove the user's
  rejection. Entry-side float refusal can occur before a command is queued
  (`kwin/src/world-native.ts:383-459`); a delivered float command on a sticky
  subject routes a native clear and similarly waits for proof
  (`world_sticky.rs:671-706,1569-1577`). The capture cannot place that press,
  distinguish entry refusal from core refusal, or prove an actual native call
  pending. Rank retained, unattempted clear as the common downstream hypothesis;
  leave capture-boundary/key-route and independent refusal alternatives open.

### Discriminating capture and proposed correction units

- The existing logs explain the common native-lane failure without trace, but
  not the exact seed settlement or rejected-unfloat boundary. For a user-run
  diagnostic capture, retain the pre-start user-journal cursor, then run
  `just dev trace` from the repository root. Record the printed verified KWin
  PID and combined/Planner log locations. After stopping with Ctrl-C, also save
  `journalctl --user --after-cursor "$CURSOR" _PID="$KWIN_PID" -o cat --no-pager`.
  Strip the displayed `-- cursor: ` prefix when assigning `CURSOR`; obtain it
  before startup with `journalctl --user --show-cursor -n 0 --no-pager`.
  This is a proposed user procedure, not an executed journey or storage reset.
  Follow the [live guide](../live-kwin-testing.md), use an appended test workspace
  and ordinary disposable windows, and retain readiness before pressing keys.
- Startup must show `world:ready`, `world:discovered` and matched
  dispatch/applied, plus `world:settings status=accepted` and
  `world:preimage-seed outcome=...`. Expected on the failing path: degraded
  namespace-mismatch (or degraded timeout), no usable recovery seed and repeated
  native-state stale drops. If seed is `ok`, retain any
  `native-recovery-seed outcome=ready pending=N` and recovery completion; inspect
  the other authorization predicates rather than assuming the same cause.
  Missing evaluated readiness is a stop, not a reason to restart around it.
- Use separate known-baseline subjects for these bounded sequences; physically
  release every chord and wait for its result before the next. Do not turn a
  failed diagnostic into acceptance or repeatedly press to force convergence.

| Diagnostic sequence | Required log/visible evidence after each press |
| --- | --- |
| Ordinary tile: Meta+M; Meta+M; Meta+F11; Meta+F11 | Respectively `world:command op=toggle-maximize` twice, then `op=toggle-fullscreen` twice; each queue/dispatch and state pending/refused/dropped. On a working path native maximize on/off, then fullscreen on/off; on the current failing path expect stale drops and unchanged native state. This is not the composed maximize-under-fullscreen acceptance sequence. |
| Ordinary tile: Meta+Shift+G; Meta+Shift+G | Two `op=toggle-sticky` command/dispatch pairs. Record all-desktop/above native facts and sibling frame after each; expected working roundtrip returns tiled. Persistent stale pairs with no setter evidence support blocked acquire/clear. |
| Ordinary tile: Meta+G; Meta+Shift+G; Meta+Shift+G; Meta+G | First `op=toggle-float label=float` must establish ordinary float; two sticky commands should return to prior float; final `toggle-float` should tile. At final rejection retain the exact UI message/time, callback/refusal lines, and whether a float command/dispatch exists. No float dispatch plus `shortcut-failed`/`float-refused-*` locates entry refusal; a dispatched float with terminal failure locates core refusal; a dispatched clear with stale drops supports blocked completion. Complete silence needs at-press holder/capture-boundary evidence. |

- `just dev` follows KWin after bring-up but tails Planner from its first line
  (`just/linux.just:1472-1474`); missing startup KWin lines cannot prove settings
  or recovery never ran. User reports no trace argument on supplied runs; a few
  `world:trace` timing lines in M2 do not provide the missing state payloads.
  Current trace enables KWin probes (`world-entry.ts:480-487`), but Planner
  CaptureWorld still emits summaries only (`planner_service.rs:498-502`,
  `world_service.rs:622-700`). Despite the broader description in the dev recipe,
  do not promise that trace will reveal target values/cmdSeq/attempts/results.
  If the cursor-bounded rerun cannot distinguish release alternatives, add
  bounded native-state gate/target and terminal-result diagnostics in a separate
  correction unit; no bus monitor is part of this procedure.
- Proposed P1 units: (1) reproduce mismatched-preimage seed through the real
  service/client/recovery/executor boundary, then correct namespace-boundary
  admission without adopting stale records or weakening durable-write gates;
  (2) reproduce sticky-on with no native attempt, sticky-off and Meta+G from
  both prior modes, distinguish unattempted binding from native pending work
  and verify release/refusal evidence; (3) repeat physical ST and remaining
  storage/recovery/menu legs after corrections. Namespace treatment beyond the
  existing contract needs an explicit decision before implementation.
- Verification of this record: source/log review, independent copy hashes,
  local link checks and `git diff --check`. No test suite or native command ran;
  previous offline/fixture passes are not refreshed. The state-target note stays
  active; no settled state/wire/history/restart contract is reopened.

### Namespace correction follow-up (2026-11-09)

- The approved [namespace-boundary correction](archive/kde-native-preimage-namespace.md)
  is verified offline: bus/KWin-owner mismatch now returns a usable successful
  empty seed, never old restoration records; first persist replaces the stale
  file. Corrupt/unreadable files remain blocking. R12 and the decision index
  record the policy. The triage above describes the pre-fix baseline.
- The linked record updates the same three traced diagnostic sequences: expect
  a successful seed and fresh native arming, not namespace-driven stale drops.
  No live rerun occurred; ST and the separate sticky/unfloat item remain open.

## 2026-11-10 physical state-target acceptance

- **User-run ST PASS apart from the deferred recovery sub-legs; macOS start gate
  cleared.** Current build included fixes `6c92aa2a` and `262af8b5`. The user ran
  `just dev trace`, physical keys, an appended workspace and two disposable
  windows. These results supersede the 2026-11-09 physical failures for the
  sequences below, not their retained diagnostic evidence.
- Evidence limit: outcomes were reported verbally by the user. Logs from this
  run were not captured for the record. No independent native readback,
  readiness/identity joins, storage acknowledgements or protocol settlement proof
  is claimed. Earlier captured logs remain identified by basename above; they
  are not evidence of this run.

| Leg | User-reported result |
| --- | --- |
| A - overlay stack | PASS: Meta+M on, Meta+F11 on, Meta+F11 off returned to maximized; Meta+M off restored the base tile, sibling unchanged. |
| B - sticky from tile | PASS: Meta+Shift+G floated the subject and kept it above, sibling reflowed, subject visible on other workspaces; second press released all-desktop/above and re-tiled. Placement defect remains below. |
| C - sticky from prior float | PASS: Meta+G floated; sticky-on preserved the float frame; sticky-off stayed floating in the same frame; Meta+G tiled and released above. |
| D - explicit tile from sticky | PASS: sticky from tile, then Meta+G cleared sticky and tiled on the current workspace. |
| E - orderly restart | PASS from both tiled and prior-float origins: Ctrl-C stopped the dev session, then `just dev trace` restarted it; sticky was retained and sticky-off honored the prior mode. Not a KWin restart or isolated native-preimage recovery proof. |
| F - native menu | PASS, user summary: Alt+F3 offered maximize, fullscreen, keep above and all-desktops and they were handled; sticky-on then menu all-desktops-off was handled. Other combinations were reported correct. Each native-origin release was not reported individually, so the detailed release matrix is not closed. |
| Earlier state-chord spam | User reported Meta+G, Meta+M, Meta+F11 and Meta+Shift+G spam all fine. No counts, command correlation or latency evidence; not full rapid-input acceptance. |

### Deferred ST recovery

- **User decision 2026-11-10:** defer ST desktop/stacking recovery and
  compositor-boundary (KWin restart) recovery until after the macOS start. These
  sub-legs no longer gate port start and remain open KDE acceptance, not PASS.
  Leg E proves only the user-observed sticky intent/prior-mode outcome across
  an orderly development-session restart; it does not separately prove the
  transient restoration of native desktop/stacking preimages.
- KDE phase 2 is complete offline and physical ST has passed apart from these
  deferred sub-legs, so the [port start gate](../decisions/contract.md#contract-authority-and-v4-migration)
  conditions are met. This does not close RC, the broader release inventory,
  native-menu permutations or full storage/protocol proof.

### Sticky-from-tile placement defect

- **Open P1 KDE defect, user-reported 2026-11-10; not a macOS start gate:**
  Meta+Shift+G from TILE retains the tiled position, whereas ordinary Meta+G
  uses the centered 60% work-area frame. The
  [intentional-float placement decision](../decisions/window-state.md#floating-membership-and-placement)
  specifies that first-float fallback (later retained placement), and
  [sticky policy](../decisions/window-state.md#sticky-floating) says tiled-origin
  sticky floats first while float-origin preserves frame. Tile-origin sticky
  should therefore use the float placement rather than retain the tile frame.
  No fix or independent reproduction is claimed.
- This note remains active for this defect, the deferred recovery sub-legs,
  detailed native-origin release coverage and full storage/protocol proof.
  No code, tests or live actions were run for this records update.
- **Placement clarification, User 2026-11-10:** the existing producer test
  deliberately retained the tile frame, but this was an implementation choice,
  not a recorded placement decision. Tiled-origin sticky-on now follows
  intentional-float placement: retained float frame, else centered 60% fallback.
  Float-origin keeps its frame; sticky-off still honors prior mode, and ordinary
  float toggle on sticky still tiles. Shared core covers KDE and Windows;
  macOS sticky remains unsupported. State-target, restart and arrival contracts
  are unchanged.
- **Offline correction:** shared sticky transition selects and retains the
  intentional-float placement with ordinary one-shot frame authority. Sticky
  clear and refused acquisition withdraw the placement token. An initial
  implementation left tokens visible to float classification during sticky-off
  and workspace-driven retile; withdrawing before staging restores those paths.
  Frozen-arrival coverage now carries the authorized sticky placement frame
  rather than waiving frame equality, without changing arrival policy.
- **Regression evidence:** before the fix, first-placement native/composed tests
  failed with no centered frame or placement token, and retained placement had
  no write token. Six regressions now pass, covering native/composed first
  placement, distinct retained placement and float-origin no-write preservation;
  existing prior-mode, refusal, retile and sticky-to-tile controls also pass.
  The producer journey now expects the same centered frame as ordinary float.
- **Verification:** `cargo test -p tiler-core -p tiler-protocol -p omnitiler --
  --skip workspace_unsafe_posture`: core 2575, protocol 288, omnitiler 341 passed;
  no failures. `cargo fmt --all -- --check` and KWin typecheck passed; KWin
  1224/1224 and KDE scenario runner 130/130 passed. Independent diff review
  found no blocking issues; targeted sticky coverage passed 203 tests, frozen
  arrival 1 test, and producer journeys 18 tests. The code fix is complete
  offline. No live re-check is claimed; physical placement acceptance remains
  pending.
- A final concurrent verification run passed Rust and the runner but reached
  KWin 1223/1224: an unchanged migration test read its ordinary-float setup as
  still tiled before any sticky operation. That test passed standalone, then
  the full standalone KWin suite passed 1224/1224 without code changes. A
  load-sensitive setup/settlement race is suspected, not proven; no test oracle
  or timeout was weakened.
- **Physical re-check, user-run 2026-11-10: PASS.** All four re-check points
  passed: fresh tiled Meta+Shift+G centres like Meta+G; tile-origin sticky after
  a moved float restores the retained frame; float-origin sticky on/off keeps
  its frame; Meta+G on sticky tiles. Verbal user report; logs not captured for
  the record. The defect is closed.
