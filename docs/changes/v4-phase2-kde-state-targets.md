# V4 phase 2: KDE state targets

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
- Pending: one fixture-only orderly restart followed by sticky-off with
  correlated protocol/storage capture and retained pre-chord focus readbacks.
  This is not native restart acceptance; the broader note remains active.
