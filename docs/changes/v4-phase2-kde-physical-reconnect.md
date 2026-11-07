# v4 phase-2 KDE physical reconnect/rehome

- Status: implementation scope pending workspace-model decision.
- Goal: supply truthful physical output facts and prove disconnect, reconnect
  and rehome offline without regressing the prior dogfoodable slices.
- Native acceptance remains unrun; offline evidence cannot establish physical
  unplug/reconnect, compositor ordering or physical shortcut delivery.

## Scope and acceptance

- Follow [ScreenReport](../porting/world-contract.md#screenreport): live output
  handles are not physical identities; derive return identity in core from
  manufacturer/model/serial, use a proved connector for fallback, and hold
  ambiguous returns. No restart-persistent mapping.
- Preserve then-current displaced workspace contents, explicit per-window
  moves, disconnect-time survivor geometry and focusless policy selections.
- Confirm minimized per-window rehome forgets the old restore slot, including
  leaving and returning while still minimized; whole-workspace relocation is
  not per-window rehome. Reuse existing v4 facts rather than assume the older
  [proposal's](shared-core-d23-rehome-and-13f-maximize-base.md) extra envelope
  is needed.
- Preserve logical-coordinate scale=1 and the settled metric/state-target
  contracts. Physical connector provenance and native metadata readability
  must not be inferred from an output-name spelling.

## Established implementation

| Area | Source evidence | Remaining work |
| --- | --- | --- |
| KDE facts | `kwin/src/world-report.ts:2595-2618` emits all-null identity; `kwin/src/kwin-globals.d.ts:123-131` declares manufacturer/model/serialNumber | Read and encode truthful metadata; establish empty-native-value handling and connector provenance |
| Core return | `crates/tiler-core/src/world_model.rs:576-656` returns by live origin-string equality | Physical-identity matching and ambiguity handling are absent from this path |
| Displacement | `world_model.rs:657-803` preserves workspaces and uses retained disconnect-time geometry; `:817-840` binds focusless policy views | Extend proofs to renamed return, ambiguity and repeated topology changes |
| Native bindings | `world_native.rs:64-87,1127-1174` maintains per-output native/logical bindings and a native-ordered bound prefix with unbound tail | Bindings do not implement the full aligned grid |
| Migration/refill | `world_commands.rs:1440-1470` removes the source logical ID and inserts after destination current; `world_model.rs:950-991` excludes the migrated ID from refill | No aligned replacement-cell mechanism exists to apply the tentative D5 exclusion to |
| Aligned helpers | `workspace_scope.rs:368-428` contains aligned ring/trailing/global-removal helpers without production callers | Disposition depends on the workspace-model decision; absence of callers does not retire approved behavior |
| Minimized rehome | `world_observe.rs:1003-1029` resolves hidden membership using native evidence; `session/world.rs:1759-1778` consumes restore slots and checks old home | Destination restore has a guard already. The leave-and-return-before-restore discriminator still needs a reproducing regression before claiming a defect |

- KWin source at `1b798c05c19b1124edb1e16623191fcd2c128fd0`:
  `src/backends/drm/drm_output.cpp:56-59` sources name from connectorName and
  metadata from EDID. This proves the DRM implementation, not a generic
  scripting connector property: other backends use synthetic names. The
  `Output` name property alone does not discriminate those backends.
- `OutputIdentity` (`world_report.rs:1122-1131`) preserves supplied empty
  strings rather than normalizing them to unknown. Native absent-metadata
  sentinels need an explicit boundary interpretation, not a codec change.
- No new tests have been run and no new behavior is implemented by this record.

## Workspace-model decision

- [Active aligned lifecycle and D4](../decisions/workspaces.md#kde-aligned-workspace-lifecycle)
  require a full native-position grid, holes and trailing-cell migration.
  Current v4 bindings/migration are not that model. This is a broader mismatch
  than the two tentative rules alone.
- [Tentative D5](../decisions/workspaces.md#tentative-aligned-migration-d5-source-refill)
  excludes a newly empty replacement cell. Current v4 migration creates no such
  cell, so that special case currently has no production trigger.
- [Tentative empty-cell lifecycle](../decisions/workspaces.md#tentative-aligned-empty-cell-and-spare-lifecycle)
  combines common behavior with aligned-only global hole/removal policy.
  `workspace_scope.rs:429-447` already reuses an existing empty trailing cell;
  creation when no spare exists is not evidence that showing an existing empty
  cell creates another spare. Keep those questions separate.

| Option | Consequence |
| --- | --- |
| Simplify around v4's common per-output model | Retire aligned-only replacement-cell/global-hole special cases and reconcile the active aligned lifecycle/D4 decisions explicitly; preserve common empty-show reuse and occupy-to-create-spare behavior |
| Restore the active aligned grid | Port grid/trailing-landing/global-hole behavior before judging the tentative D5 and empty-cell refinements; materially larger than physical identity matching |
| Limit this slice to reconnect facts and core return | Leave the aligned decision mismatch explicitly deferred; do not claim aligned lifecycle/migration acceptance |

- Recommendation: simplify around the common v4 model, subject to an explicit
  decision covering active aligned lifecycle/D4 as well as the two tentative
  entries. Do not silently treat a missing port as evidence that old behavior
  was retired. The renamed-return discriminator is recorded in
  [reference outcomes](../spec/reference-outcomes.md#kde-dual-output-admission-and-workspace-scope-discriminators).

## Bounded implementation and verification

1. Resolve workspace-model scope and metadata sentinel interpretation.
2. Produce metadata and implement core physical return matching with targeted
   unknown/duplicate/renamed-output and disconnect/reconnect tests. Keep
   connector null without a proved source; unique metadata does not require
   inventing a connector.
3. Reproduce the approved D23 leave-and-return-before-restore sequence, then
   implement only any demonstrated missing invalidation using accepted v4
   membership evidence. Retain omission/request/whole-workspace negative controls.
4. Review semantic changes independently; run scoped Rust fmt/test/build/clippy
   and adapter typecheck/test/build gates. Existing wire/stage5 bytes remain
   unchanged; new semantic coverage belongs in targeted tests.
5. Cull only proved-obsolete touched mechanisms separately after the relevant
   decision, with no production callers and replacement evidence established.

## Native-only remainder

- Reuse fixture attribution, readiness, evidence and restoration from
  [the physical checklist](kde-physical-checklist.md). Its existing D block
  explicitly excludes hotplug; physical topology changes need a separate journey.
- On two physical outputs, record complete identity rows and native bindings;
  stage distinct fixture workspaces and focus a fixture on the output to remove.
  Unplug it, verify separate displaced contents and retained focus, reconnect,
  and verify then-current contents return without snapshot restoration.
- Repeat with focus on the survivor; it must retain its active view/focus.
  During absence, explicitly move one fixture out of the displaced workspace;
  reconnect and verify that fixture is not pulled back. Exercise a changed
  connector only if physical identity is readable; ambiguity must hold.
- With three outputs, verify nearest-survivor choice from disconnect-time
  geometry. With a supported native minimized-membership move, run D23 both
  restore-on-destination and return-still-minimized variants. Unsupported routes
  remain TBD.
- After topology restoration, recheck ordinary tiling, selection, arrival,
  focus/paint and native state transitions separately; offline state-target
  evidence is not native acceptance. Retain ordered before/after reports and
  first divergence, then restore only recorded owned state.
