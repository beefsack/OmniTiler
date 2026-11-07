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
  publication. Earlier authorized evidence survives held receipt. This is a
  contract clarification awaiting user review.
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
