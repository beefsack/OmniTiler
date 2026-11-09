# v4 phase-2 KDE physical reconnect/rehome

- Status: offline implementation and overlapping-displacement return fix
  verified; native acceptance pending, 2026-11-08. Keep active.
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
- The common v4 per-output model replaces aligned lifecycle/D4 and aligned-only
  D5/empty-cell refinements. Displacement/return semantics remain unchanged;
  no acceptance claim depends on the retired aligned grid.

## Baseline audit

| Area | Baseline source evidence | Work identified |
| --- | --- | --- |
| KDE facts | `kwin/src/world-report.ts:2595-2618` emits all-null identity; `kwin/src/kwin-globals.d.ts:123-131` declares manufacturer/model/serialNumber | Read and encode truthful metadata; establish empty-native-value handling and connector provenance |
| Core return | `crates/tiler-core/src/world_model.rs:576-656` returns by live origin-string equality | Physical-identity matching and ambiguity handling are absent from this path |
| Displacement | `world_model.rs:657-803` preserves workspaces and uses retained disconnect-time geometry; `:817-840` binds focusless policy views | Extend proofs to renamed return, ambiguity and repeated topology changes |
| Native bindings | `world_native.rs:64-87,1127-1174` maintains per-output native/logical bindings and a native-ordered bound prefix with unbound tail | Bindings do not implement the full aligned grid |
| Migration/refill | `world_commands.rs:1440-1470` removes the source logical ID and inserts after destination current; `world_model.rs:950-991` excludes the migrated ID from refill | No aligned replacement-cell mechanism exists to apply the tentative D5 exclusion to |
| Aligned helpers | Uncalled aligned ring/trailing/global-removal and history-rebind helpers in `workspace_scope.rs` | Removed separately; common trailing and history paths remain |
| Minimized rehome | `world_observe.rs:1003-1029` resolves hidden membership using native evidence; `session/world.rs:1759-1778` consumes restore slots and checks old home | Destination restore has a guard already. The leave-and-return-before-restore discriminator still needs a reproducing regression before claiming a defect |

- KWin source at `1b798c05c19b1124edb1e16623191fcd2c128fd0`:
  `src/backends/drm/drm_output.cpp:56-59` sources name from connectorName and
  metadata from EDID. This proves the DRM implementation, not a generic
  scripting connector property: other backends use synthetic names. The
  `Output` name property alone does not discriminate those backends.
- `OutputIdentity` (`world_report.rs:1122-1131`) preserves supplied empty
  strings rather than normalizing them to unknown. KDE normalizes native empty
  metadata to null at capture; nonempty literal identifiers remain unchanged.
  Connector stays null; no core or codec normalization changes are needed.

## Workspace-model retirement

- User decision, 2026-11-08: retire aligned lifecycle, aligned D4 destination
  replacement, tentative D5 replacement-cell exclusion and aligned-only
  empty-cell/global-hole refinements in favor of the
  [common v4 per-output model](../decisions/workspaces.md#common-v4-per-output-workspaces).
- Current migration creates no replacement cell; common trailing selection
  reuses an existing empty final spare, and occupying it supplies the next spare.
  These implemented behaviors stay active, without introducing sticky or
  selection policy. Displacement/return decisions are reworded to the common
  model without changing their behavior; old anchors remain valid.
- Historical aligned-only discriminators and the earlier aligned record are
  marked retired. The renamed-return discriminator remains in
  [reference outcomes](../spec/reference-outcomes.md#kde-dual-output-admission-and-workspace-scope-discriminators).
- Cull: removed `aligned_ring_ids`, `aligned_trailing_index`,
  `resolve_aligned_trailing`, `aligned_removal_set` and
  `ScopeHistory::invalidate_aligned_rebind`. Caller search found definitions
  plus one internal helper call only, with no helper-only tests. Post-removal
  source search has no hits. Existing migration/spare tests exercise common
  behavior and remain; stale labels and the unread `WORLD_EVAL_ALIGNED`
  environment selector are removed without changing their assertions.
- Cull verification: Rust format check, filtered common trailing regressions
  and adapter typecheck succeeded. A `workspace_scope` name filter selected
  no tests and is not coverage evidence.

## Bounded implementation and verification

1. Retire aligned rules in decisions and point-of-use docs; separately remove
   uncalled aligned mechanisms with caller evidence. Normalize KDE-native empty
   metadata to null without changing core/codec string preservation.
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

## Accepted offline increment

- KDE capture reads manufacturer/model/serialNumber defensively. Native empty,
  missing, throwing or non-string values become null; nonempty literal
  `unknown` values remain data. Any arm over 256 UTF-8 bytes holds only that
  screen incomplete with null rectangles and identity, without dropping healthy
  peers. Encoder validation fails closed for malformed hand-built fields.
  Connector remains null and logical scale remains exactly 1.
- Core retains disconnect-time physical identity and tuple/connector contention.
  Readable unique tuples match across renamed live handles; a proved connector
  can match unreadable tuples but cannot override a readable different tuple.
  Duplicate/unknown identities, surviving lookalikes and colliding claims hold.
  No live-name fallback or persistent identity mapping was added.
- Targeted core tests cover renamed return, same-name different panel, unknown
  and duplicate identities, connector fallback/mismatch, repeated cycles,
  explicit move-out without pullback, no survivor, and exact workspace contents
  and tiling when S is displaced through T to U. Both separate return orders
  and simultaneous returns now retain exact original affinity; see the
  ownership correction below.
- D23 leave-and-return-before-restore reproduced a stale source slot. Accepted,
  receipt-proved minimized per-window rehome now forgets that slot after sweep
  through the existing revision-neutral cleanup. No new wire envelope or
  adapter tracking was needed. Eleven regressions cover same/cross-output and
  same-turn departure, still-minimized return, stable restore, unknown receipt,
  incomplete inventory, unreadable evidence, unsettled transfer, refused command,
  whole-workspace relocation and close precedence. The topology discriminator
  settles focus on A in a separate accepted report before restoring B, proving
  fresh A/B grouping rather than resurrection of the old B/C slot.
- Source review accepted the metadata/matching/D23 corrections and reproduced
  the chain defect. Further independent source review caught the same-report
  return failure in the first ownership approach and accepted the corrected
  displacement-time affinity invariant. No native behavior is established.
- Final integrated verification, 2026-11-08: all seven scoped commands below
  exited 0 after the latest relevant source/test changes. Rust tests: 3127
  passed, 0 failed/ignored, 1 unsafe-posture check filtered out. KWin tests:
  1160 passed, 0 failed/skipped. Both original chain regressions are enabled
  with every assertion unchanged; three additional chain controls pass.
- No immutable world-wire/stage5 fixture, golden or recording bytes changed.
  No native effect source changed, and no new native-effect or live evidence
  is claimed.

```sh
devenv shell -- cargo fmt --all -- --check
devenv shell -- cargo test -q -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture
devenv shell -- cargo build --locked -p tiler-core -p tiler-protocol -p omnitiler
devenv shell -- cargo clippy -q -p tiler-core -p tiler-protocol -p omnitiler --all-targets -- -D warnings
devenv shell -- npm --prefix kwin run typecheck
devenv shell -- npm --prefix kwin test
devenv shell -- env OMNITILER_SCOPE_CLASS= npm --prefix kwin run build
```

## Corrected: overlapping displacement return order

- Minimal reproduction: start distinct occupied S, T and U outputs with readable
  unique identities. Disconnect S so its exact workspace rides T; disconnect T
  so both units ride U. Return S then T, or T then S, using renamed live handles
  and the original identities. Expected: each exact workspace exists once with
  its then-current contents on its original physical panel (or waits on U while
  that panel is absent). Before correction, S's viewed workspace ID had zero
  hosts when T returned in either order, even after S had reclaimed it.
- Both sequences reproduced failure with their ignore attributes removed:
  `physical_chain_return_s_first_then_t_keeps_exact_affinity` and
  `physical_chain_return_t_first_then_s_keeps_exact_affinity` in
  `world_restored_routing_tests.rs`. Baseline: 0 passed, 2 failed. Both are now
  enabled and pass without changing any assertion or their overlap preconditions.
- Root cause: `world_model.rs` used the full overlapping hosting set as return
  ownership. Chain refresh updated the destination but not original affinity;
  `take_workspace_from_all` let T reclaim S's exact ID as a rider rather than
  leave it with its original owner. The resulting reply had no host for S's ID.
- A return-time strict-subset ownership filter passed both separate return
  orders but failed a new same-report S+T return regression: removing S's record
  before processing T erased the ownership evidence and again left S's unit
  with zero hosts. That approach was rejected, including its inferred equal-set
  winner and batch special case; neither remains in the implementation.
- Correction: `DisplacedEntry.owned` records the original-affinity subset at
  displacement, excluding units already owned by other displaced origins.
  Hosting sets may overlap, but owned sets are disjoint. Returns reclaim only
  owned units, preserving existing tiling/session/member relocation; reclaimed
  IDs leave every remaining hosting/affinity set, and empty records retire.
  No adapter, wire or physical-identity matching changes were needed.
- Five targeted regressions pass: both original separate orders, same-report
  return with either lexical origin order, and explicit member move-out before
  simultaneous return. The moved-out member stays on its chosen workspace;
  ordinary lifecycle may trim the emptied unviewed source unit, as in the prior
  single-hop control. No focus effect or hotplug pull-back is minted.
- No further helper/rule/test is proved obsolete: full hosting sets still serve
  retention and chain destination tracking. No additional cull was made.
- Whole-workspace migration while displaced is not exercised here. Its D9
  association-removal path remains unverified; no migration behavior changed.

## Disconnect rehomed members are not admissions

- User decision 2026-11-09: windows rehomed off a disconnected output are not
  fresh admissions. No automatic-admission focus effect fires and native focus
  stays untouched, consistent with the commandless disconnect rule. When the
  members join an existing non-fresh unit on the survivor no `op-dpv-*` view is
  minted, accepted as correct.
- Cause (core): native receipts home the rehomed members onto the survivor's
  same-native unit, a domain with no retained session that the prior report
  published as an empty controlled unit; `take_admission_focus` (per-domain diff
  plus `prior_empty_controlled`) read the last member as a newcomer and minted
  one automatic-admission effect. Not an adapter defect.
- Fix: per-report `displaced_rehomed` set (cleared in `converge_logical_pre`,
  populated for non-sticky members whose recorded home sat on the removed
  origin's units in a fresh displacement) that `take_admission_focus` spends
  without emitting. Homing, views, mapping, transfers and publication are
  unchanged. Regressions: `disconnect_rehomed_members_mint_no_admission_focus`
  (fails without the fix) and the genuine-newcomer control in
  `world_selection_await_tests.rs`, plus the producer disconnect journey.
- Reference WMs (source-only, pins in the reference-outcomes row): i3 re-attaches
  the disappearing output's workspaces and re-focuses the previously focused
  container; sway evacuates workspaces with no focus change. Neither treats
  moved windows as new.
- Known limit: the spend is consumed only when the survivor domain is iterated
  in the same report; a held, floating or incomplete-inventory survivor scope in
  the disconnect report was reasoned (not reproduced) to leave a later first
  appearance able to mint. A probe with an incomplete disconnect inventory
  stayed quiet, so no extra handling was added.

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
