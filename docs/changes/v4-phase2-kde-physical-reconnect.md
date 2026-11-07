# v4 phase-2 KDE physical reconnect/rehome

- Status: partial offline implementation verified; overlapping-displacement
  return defect open and native acceptance pending, 2026-11-08. Keep active.
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
  and tiling when S is displaced through T to U and S alone returns. The chain
  destination refresh does not establish the later return of T; see the open
  defect below.
- D23 leave-and-return-before-restore reproduced a stale source slot. Accepted,
  receipt-proved minimized per-window rehome now forgets that slot after sweep
  through the existing revision-neutral cleanup. No new wire envelope or
  adapter tracking was needed. Eleven regressions cover same/cross-output and
  same-turn departure, still-minimized return, stable restore, unknown receipt,
  incomplete inventory, unreadable evidence, unsettled transfer, refused command,
  whole-workspace relocation and close precedence. The topology discriminator
  settles focus on A in a separate accepted report before restoring B, proving
  fresh A/B grouping rather than resurrection of the old B/C slot.
- Source review accepted the metadata/matching/D23 corrections; a separate
  bounded source/log review confirmed the deferred chain defect and both
  regressions. Native behavior is not established by either review.
- Final integrated verification, 2026-11-08: all seven scoped commands below
  exited 0 after the latest source/test changes. Rust tests: 3122 passed,
  0 failed, 2 known-failing regressions ignored, 1 unsafe-posture check filtered
  out. KWin tests: 1160 passed, 0 failed/skipped. Passing enabled checks do not
  close the defect or establish full offline reconnect acceptance.
- No immutable world-wire/stage5 fixture, golden or recording bytes changed.
  No native effect source changed, and no new native-effect or live evidence
  is claimed.

```sh
devenv shell -- cargo fmt --all -- --check
devenv shell -- cargo test -q -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture --nocapture
devenv shell -- cargo build --locked -p tiler-core -p tiler-protocol -p omnitiler
devenv shell -- cargo clippy -q -p tiler-core -p tiler-protocol -p omnitiler --all-targets -- -D warnings
devenv shell -- npm --prefix kwin run typecheck
devenv shell -- npm --prefix kwin test
devenv shell -- env OMNITILER_SCOPE_CLASS= npm --prefix kwin run build
```

## Open defect: overlapping displacement return order

- Minimal reproduction: start distinct occupied S, T and U outputs with readable
  unique identities. Disconnect S so its exact workspace rides T; disconnect T
  so both units ride U. Return S then T, or T then S, using renamed live handles
  and the original identities. Expected: each exact workspace exists once with
  its then-current contents on its original physical panel (or waits on U while
  that panel is absent). Observed: S's viewed workspace ID has zero hosts when
  T returns in either order, including after S already reclaimed it.
- Both sequences reproduced failure before being marked explicitly ignored:
  `physical_chain_return_s_first_then_t_keeps_exact_affinity` and
  `physical_chain_return_t_first_then_s_keeps_exact_affinity` in
  `world_restored_routing_tests.rs`. The targeted run had 0 passed and 2 failed;
  these are known failing regressions, not passing coverage. The older
  S-only-return regression still passes.
- Source evidence: `world_model.rs` chain refresh updates overlapping records'
  destination but does not partition their workspace sets. Return uses
  `take_workspace_from_all`, which strips other mapping rings without reconciling
  the other displaced records. Exact interior deletion/trim sequencing remains
  unverified; the workspace-loss outcome and overlapping sets are reproduced.
- This is not a small isolated fix: original affinity, hosting, session rekeying
  and explicit move-out/no-pullback invariants need a coherent ownership design.
  No fix is included. Next offline action: resolve that ownership invariant and
  make both regressions pass without weakening their exact identity/content
  assertions. Full offline reconnect acceptance remains open.

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
