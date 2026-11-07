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
2. Resolve the native preimage storage interface, then implement and verify
   property execution and durable recovery. Remove obsolete local policy in
   separate increments after migrated-caller proof.
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
- Preimage storage is absent. The existing opaque intent file excludes native
  state and cannot be repurposed as core-owned intent. Its same-UID/current-KWin
  authorization, bus/owner namespace and private atomic replace are reusable
  implementation patterns, not proof of native recovery.
- At pinned KWin 6.8.1 (`1b798c05c19b1124edb1e16623191fcd2c128fd0`),
  `src/scripting/scripting.cpp` exposes `readConfig` but no script configuration
  writer or generic durable property store; stopping/unloading deletes the
  script and its child JS engine. The same scripting-surface audit at 6.6 was
  consistent. This rules out retaining JS maps across script unload, not every
  possible native storage mechanism. A separate external preimage API or a
  native storage owner remains an unresolved integration choice.
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
