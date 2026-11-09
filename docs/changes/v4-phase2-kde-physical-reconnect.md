# v4 phase-2 KDE physical reconnect/rehome

- Status: native RC partial, leg 2 failed, 2026-11-09; the exact offline leg is
  corrected and passing. Keep active pending the traced physical re-run.
  RC gates the macOS reconnect/rehome slice and 0.1, not the macOS start.
- Goal: supply truthful physical output facts and prove disconnect, reconnect
  and rehome offline without regressing the prior dogfoodable slices.
- Native evidence is bounded below; offline evidence cannot establish physical
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

## 2026-11-09 native reconnect triage

- RC is **PARTIAL; leg 2 FAIL**. These were user-run physical two-output,
  unscoped real-window journeys in per-output workspace mode, not fixture runs.
  A is the survivor, B the removed output; F is a browser and T1-T3 terminals.
  Start: A ws1 active with T1; B ws1 has T2, B ws2 active has F/T3.
- Evidence: unchanged copies of `omnitiler-dev.kQ5Uh3.log` (L1, 391 lines)
  and `omnitiler-dev.hKnZHu.log` (L2, 377 lines), preserved outside the
  repository and byte-compared with the originals before investigation.
  They contain summaries, not report/reply payloads: no physical identities,
  native bindings, logical homes, transfer rows or focus readbacks. `nwinv`
  is a local invalidation watermark, not a topology count
  (`kwin/src/world-state.ts:133-137`). Action-to-revision attribution is unknown.

| RC sub-leg | User-observed native result | Verdict / evidence boundary |
| --- | --- | --- |
| Leg 1: focus removed output, unplug/return | F initially focused. Unplug: A ws1 remains active with T1/T2; A ws2 has F/T3, F hidden and no window focused. Return: exact original layout, F still selected | PARTIAL: return PASS as reported; separate-layout rule diverges if T1/T2 share one tiling unit. No-focus matches the narrow native-rehome/no-admission-focus decision, not the older keep-focused-workspace-visible intent. Unit/focus attribution unproved by L1 |
| Leg 2: focus survivor, explicit move during absence, return | T1 keeps focus on unplug. On A ws2 select T3, send to ws1. Return: A ws1 T1 focused; A ws2 F; B ws1 T2/T3, B ws2 empty (B's active view uncertain) | FAIL: survivor-focus PASS as reported; F return and T3 no-pullback FAIL. L2 proves command/apply ordering, not the causal transfer or its settlement |
| Leg 3: post-return sanity | Ordinary tiling worked | Bounded reported PASS only; full selection/arrival/focus/paint/state-transition permutations NOT RUN |

- Changed connector/identity ambiguity, three-output nearest survivor and both
  minimized D23 variants remain NOT RUN; no whole-RC or MAC-gate pass.

### Ordered evidence and first divergence

- L1: setup `move-directional` dispatch -> planned-focus accepted/proven ->
  apply rev11 `geom=2 fx=1` (:150-158). Later observe captures rev14/15
  are superseded; rev16 applies `geom=4 member=0 views=0 fx=0`
  (:186-207). Later rev22/23 captures are superseded; rev24 applies zero
  membership/view/effect writes (:259-292). These are ordered summaries,
  **not identified unplug/replug boundaries**. They do not prove homing,
  native focus, return matching or a separate-versus-merged tree.
- L2: `select-numbered` ws2 -> dispatch -> planned-focus accepted/proven ->
  apply rev10 `views=1 fx=1` (:118-127); later `send-numbered` ws1 ->
  dispatch -> apply rev14 `geom=2 member=1 views=1 fx=0` (:176-183) ->
  observe/apply rev15 `geom=1 member=0 views=0 fx=0` (:188-189).
  Subsequent rev18/19 observations are superseded, rev20/21 apply with
  `member=0 views=0 fx=0` (:217-269). Unplug/replug cannot be assigned to
  these revisions. `applied` counts accepted setter calls, not native/core
  settlement (`kwin/src/world-entry.ts:4668`, `world-apply.ts:1035-1059`;
  `crates/tiler-core/src/world_observe.rs:942-949`).
- **First reported divergence** is already the unplug-time T2/T1 co-location
  versus the generic never-merge rule; the leg-2 return-specific divergence is
  F staying on A and explicitly sent T3 appearing on B. **First causal
  report/reply divergence is unknown**: neither capture exposes the relevant
  native receipts, retained homes, owned-unit membership or pending transfers.
  Absence of `op-hp-*` text in summaries cannot exclude a core transfer.

### Source-grounded explanation, not a proved live cause

- Native removal can change output without changing desktop membership.
  Core maps `(output, native desktop)` to a logical unit
  (`crates/tiler-core/src/world_native.rs:64-68,120-130`), with distinct
  logical IDs allocated per output (:1266-1298). Known native receipts win
  over retained desire (`world_observe.rs:997-1013`). Thus T2 can join A's
  occupied ws1 and F/T3 A's ws2, emptying B's preserved units. This is
  **member rehoming, not equal-logical-ID deduplication**. The existing
  [producer journey](../../kwin/tests/world-producer-disconnect.test.ts):100-120
  lands only on an empty survivor unit; :154-217 returns empty displaced
  units while all members stay on A. It does not establish native member return.
- Core preserves displaced unit IDs (`world_model.rs:831-856`), matches
  physical identity (:590-632), then reclaims the origin's owned subset and
  transfers only members whose retained homes still qualify (:633-703).
  **Hypothesis H1:** F rehomed to A's native-bound unit, so reclaiming B's
  empty unit cannot return F. **H2:** T3's send resolved/settled differently
  from intent, leaving a return-eligible home or pending transfer. Full native
  membership/readback and transfer payloads are needed to distinguish these.
- **H3, native pullback:** same-output sends skip the output setter
  (`kwin/src/world-apply.ts:901-909,939-947`) and set desktop membership
  (:1025-1059; `world-native.ts:676-692`). Pinned KWin
  `1b798c05c19b1124edb1e16623191fcd2c128fd0` invokes layout restoration at
  `src/workspace.cpp:1509`; `src/placementtracker.cpp:102-134` gates restoration
  on interactive-move/state changes, not desktops, and restores geometry.
  A desktop-only send therefore does not itself invalidate native restoration;
  native geometry/output restoration could pull T3 back without a core
  hotplug transfer. Why F and T3 differed is **unproved**, not explained by
  the summaries or this source-only hypothesis.
- No-focus is consistent with the selected no-admission-focus mechanism
  (`world_model.rs:1257-1265`) and the [2026-11-09 native-rehome case](../spec/reference-outcomes.md#selection-accounting-without-logical-echoes).
  The logs do not establish whether KWin cleared focus or when F became hidden.
  T2 joining T1 is source-permitted by receipt homing, but **not established as
  expected product behavior**: the [generic disconnect rule](../decisions/workspaces.md#kde-common-model-output-disconnection)
  still says never merge. Recommendation pending decision: retain separate
  displaced units rather than silently add a same-native occupied-unit exception;
  reconcile that rule with native receipt authority before choosing a fix.

### Suggested offline regression

- Extend the producer journey with the exact four-member/two-desktop setup
  above, including T2 colliding with occupied A ws1 and T1 focused. Capture
  every native report/reply across removal; assert unit IDs, bindings, members,
  tree/frames, views and focus, not only the final pager. The collision oracle
  remains [TBD pending decision](../spec/reference-outcomes.md#selection-accounting-without-logical-echoes).
- Select A ws2/T3, send to ws1, retain immediate pre/post membership evidence,
  then supply a separate receipt proving membership settlement before return.
  Reconnect the same physical identity: F returns to B ws2, T3 stays A ws1,
  T2 returns B ws1, survivor view/focus remains, no stale snapshot or T3 pullback.
  Split native-restoration inputs from core-transfer outputs to locate the
  first divergence; include an outstanding-send control. Existing
  `physical_return_explicit_send_away_not_pulled_back`
  (`world_restored_routing_tests.rs:3539-3642`) does not exercise this producer/
  native-rehome/occupied-unit combination. This regression is proposed, not run.
- Verification of this record: source/evidence review, local link/anchor checks
  and `git diff --check`; no new native or offline execution evidence.

## 2026-11-10 offline leg-2 discriminator

- Goal: locate the first producer/core divergence without selecting the pending
  same-numbered merge policy. No physical reconnect, display change or native
  acceptance is established by this investigation.
- `kwin/tests/world-producer-disconnect.test.ts` now carries the exact four-member
  setup: T1 A/ws1 focused; T2 B/ws1; F/T3 B/ws2. It removes B, selects A/ws2
  through `select-numbered`, focuses T3 through `windowActivated`, sends T3 to
  ws1 with follow, then supplies quiescent native membership before returning
  B's physical identity under a renamed handle. Selection and send each have
  their own ready take, native current readback and plan/native equality proof.
- Before correction: the new test reaches `F returns to B` and fails
  (`out-left`, expected `out-right2`). Prior assertions prove T3's native ws1
  membership and settled logical workspace persist on A across return; all
  four plan rows match the modeled natives. T2's final return assertion is
  after the first failure and is not execution evidence. The two older journeys
   passed before correction but encoded the same emptied-unit defect; their
   corrected oracles now require separate hosted units and current-content return.
- First offline causal divergence: at removal, native receipts resolve T2 to
  A's occupied ws1 unit and F/T3 to A's ws2 unit
  (`crates/tiler-core/src/world_observe.rs:1006-1013`). Membership reconciliation
  stores that new home (`world_members.rs:177-190`), superseding the displaced
  logical id retained during `world_model.rs:846-857`. Physical return reclaims
  B's owned unit IDs, but member eligibility requires a retained home on exactly
  those IDs (`world_model.rs:633-703`, especially `:680-687`). F no longer
  qualifies; B's unit returns empty. This establishes the offline mechanism,
  not the cause of the physical browser outcome from summary logs alone.
- T3 pullback is not reproduced. The harness return deliberately moves no
  native windows; it does not model KWin placement restoration. The original
  L2 send/apply evidence remains exactly as bounded above: no proof of native
  settlement or physical matching, and no basis to assign unplug/replug to
  specific revisions. A settled offline send excludes a core pullback only
  for this supplied input sequence, not for every possible live ordering.

### Existing-rule correction and open merge question

- [R-OUT-06a](../spec/reference-outcomes/multi-output.md#r-out-06a-occupied-same-index-evacuation-explicit-member-move-return)
  records all four Tier 1 sources at the existing pins. COSMIC directly models
  equal per-output indices and preserves separate workspace objects. i3/sway
  names and Hyprland numbered IDs are global; their distinct-identity analogy
  also moves whole objects, not equal-index member merges. Sway/Hyprland/COSMIC
  return current objects; i3 has no automatic return affinity in this default
  leg. None supplies evidence for a per-member merge-and-later-unmerge rule.
- The correction implements the existing generic separate-layout and
  then-current-content rules. It does not record a user decision on the open
  same-numbered merge question or add a merge exception. The reference row
  stays TBD for that question, with the Tier 1 evidence attached.
- Displacement now retains the existing membership-only policy transfer even
  when KWin has already moved the window's output onto the survivor. Existing
  native-creation and binding machinery supplies a distinct backing; no new
  membership ledger, wire field or receipt precedence is introduced.
- Policy settlement distinguishes unchanged source-native topology evidence
  from a genuine different native receipt using the transfer's existing source
  binding. The former holds until arrival; the latter supersedes the policy
  immediately. Unknown/incomplete evidence holds; native-free behavior remains
  unchanged. Settled members again follow ordinary native truth, including
  movement back onto a formerly used desktop.
- Return snapshots homes before rekeying, so then-current members remain
  eligible for the return transfer. Physical-identity-matched native bindings
  carry before adoption on a renamed handle, avoiding unrelated fresh logicals;
  dead-origin binding rows are removed. Both topology scans leave outstanding
  command-owned window/migration transfers alone. Rehomed members remain exempt
  from automatic-admission focus.
- Eleven core controls cover pending sends before disconnect and at return,
  newcomers, pending-policy replacement, genuine external moves before backing,
  backed-unit movement, unknown/inventory-gap evidence and capability-off input.
  Independent review accepted the production change and five existing chain
  regressions; its send-before-disconnect and known-value-gap coverage gaps
  were subsequently closed. Native KWin placement restoration remains unmodeled.
- A further shared-native ordering control reproduced a return-before-backing
  gap: the unbacked hosted source had no native binding, so settlement dropped
  its return transfer and adopted the survivor's unit. The corrected check uses
  the carried target binding as stale-source evidence only while the source has
  no binding. A backed source stays authoritative. Three controls prove stale
  shared-native return/arrival, genuine different-native supersession, and a
  backed source with a genuine move to the original native. Follow-up independent
  review accepted this repair; no new state or wire semantics were needed.

### Bounded reconnect diagnostics

- Ordinary trace previously emitted `no-subject` for commandless observations;
  it was not a full report/reply dump. The trace-only probe now emits on existing
  capture sends and accepted applies, without extra captures/timers/writes:

  | Event | Evidence boundary |
  | --- | --- |
  | `observe-sent`, `observe-sent-win` | Exact sent source/watermark, output IDs, native current/order, focus and per-window output/logical/native membership |
  | `observe-applied`, `observe-fresh-win` | Fresh pre-apply membership and focus, separately labeled post-apply focus, reply revision and lane totals |
  | `observe-plan`, `observe-bind`, `observe-transfer` | Reply desire, not observed settlement; logical home, native binding, authority and transfer target/held/op/cmdSeq |
  | `member-attempt` | Exact delivered attempts versus separately labeled retained-at-apply attempts; pre/post typed membership, setter, operation and original timestamp |

- Caps: 8 outputs/native orders/bindings/plans/transfers/attempts, 16 observed
  window carriers; every new truncated lane reports omission counts. Bindings
  referenced by shown plans/transfers are prioritized before spare rows, so
  appended test workspaces are not hidden behind the first eight native rows.
  Nonzero omissions mean absence is not proof. All-desktop, multi-desktop and
  unknown native membership remain distinct. IDs are sanitized; titles, app
  names and physical metadata are not logged. Normal builds eliminate this
  probe. Transfer wire rows expose the target, not a separate source; internal
  displaced owned/hosting sets and physical-identity matching remain unlogged.
- Independent source review accepted the offline causal chain and the new
  trace boundary separation. Existing subject-line transfer-op sanitization and
  older subject/frame-state-attempt caps have unrelated diagnostic limitations;
  this investigation uses the new explicit `observe-transfer` and
  `member-attempt` lanes with totals/omissions, not those older summaries.

### Traced user re-run (not executed)

1. Follow [the live guide](../live-kwin-testing.md), fixture scope, lifecycle and
   evaluated readiness. Start from `just dev-status` reporting DOWN; retain a
   pre-start journal cursor and fresh evidence directory outside the checkout.
   Start the [fixture server and `run` helper](../../crates/tiler-live-fixture/README.md#smoke-two-terminals-from-the-repo-root-after-an-env-restart).
   Use two appended native desktop positions p/q beyond the existing roster,
   with independent per-output current views. p/q replace ws1/ws2 in this leg;
   do not commandeer the user's existing numbered workspaces.
2. In the capture terminal (inside the configured development environment):

   ```sh
   CAPTURE_DIR="$(mktemp -d)"
   CURSOR="$(journalctl --user -n 1 -o json --no-pager | jq -er '.__CURSOR')"
   just dev-status
   ```

   In another terminal, start and retain the combined trace:

   ```sh
   just dev-kde trace --scope-class OmniTilerTestWindow 2>&1 | tee "$CAPTURE_DIR/reconnect-trace.log"
   ```

   Export/paste the same capture directory in that terminal first. Use the
   controller receipt's verified KWin owner PID to retain the unfiltered stream
   in the capture terminal:

   ```sh
   RECEIPT="$(cat "$XDG_RUNTIME_DIR/omnitiler-dev/controller-receipt-path")"
   KWIN_PID="$(jq -er '.pid' "$RECEIPT")"
   journalctl --user --no-pager -f --after-cursor="$CURSOR" _PID="$KWIN_PID" -o short-monotonic | tee "$CAPTURE_DIR/kwin-journal.log"
   ```

   Require fresh ready/discovered and a matched accepted dispatch/applied pair;
   stop if inert. Keep the separate unfiltered scripting-error check. No raw bus
   monitor is needed.
3. Spawn four ordinary resizable fixtures and retain exact caller/PID/native
   joins. Stage T1 on A/p focused, T2 on B/p, F/T3 on B/q with B/q current.
   F is a fixture, not a browser-specific claim. Record native windows' outputs,
   desktops, currents and focus plus a `run` get response before proceeding.

   ```sh
   run '{"v":1,"request_id":"rc-spawn","op":"spawn","windows":[{"id":"T1"},{"id":"T2"},{"id":"F"},{"id":"T3"}]}'
   run '{"v":1,"request_id":"rc-before","op":"get","ids":["T1","T2","F","T3"]}'
   ```

4. User physically unplugs B. Record the action time and wait for a quiet
   accepted observation. Retain sent/fresh/reply window rows, binding and lane
   totals, currents and actual focus. Expect separate hosted logical units and
   newly created backing where the survivor's p/q are already bound; prove
   their membership settles without admission focus before continuing. Select
   A/q, then explicitly activate T3 on its observed hosted backing (it need not
   still be native q); do not replace the selection with a different index.
   Send T3 to A/p with follow. Record each action time; prove T3 native membership=p in a
   **later** sent observation, with the corresponding plan on A/p and no
   outstanding T3 transfer. A setter count or ready take alone is insufficient.
   If that proof is missing, stop and retain the unsettled-send evidence rather
   than claiming an explicit-move return failure.
5. User physically reconnects B, without moving/focusing a window meanwhile.
   Retain every source from the first topology change through quiescence. If
   sent/fresh T3 already names B before a core transfer, that is native output
   restoration evidence. If a reply first authorizes T3 to B, retain its exact
   transfer/op/target/attempt chain. This split does not independently prove
   physical-identity matching or the compositor's internal restoration reason.
   Repeat `run get` with a fresh request ID for all four fixtures. Record F/T2
   return and T3 exclusion separately; do not promote partial results to RC PASS.
6. Close only the four owned fixtures and shut down the fixture server. Stop
   the scoped controller with Ctrl-C and perform its verified teardown; retain
   both complete logs. Undo only recorded owned staging changes. A real-browser
   reproduction, if still necessary, is a separate user-owned leg rather than
   silent scope broadening.

### Current offline verification

- `npm --prefix kwin run typecheck`: exit 0.
- Red: `npm --prefix kwin test` before correction had **1223 passed, 1 failed**,
  exit 1, with the sole failure `F returns to B`. Green: **1224 passed, 0 failed,
  0 skipped/todo**. The exact select-2/focus-T3/send-1 action sequence is retained;
  F/T2 return and T3 stays, with native-plan equality and no retained return or
  creation operation at quiescence. The opt-in producer harness models the
  pinned void `createDesktop(position, name)` plus `desktopsChanged` so distinct
  backing can really settle; the default harness remains unchanged.
- Focused trace diagnostics: **16/16**; existing world-entry **93/93** and
  trace/build gating **6/6**. Independent review verified normal-bundle probe
  elimination and no added native effect, capture or timer; the final additive
  binding-cap control also passes in the full run above.
- Scoped Rust: **3191 passed, 0 failed/ignored**, with the unsafe-posture check
  excluded: core **2562**, protocol **288**, omnitiler **341**. Hermetic runner:
  **130/130**. No live checks or immutable fixture changes. These offline
  results do not establish native RC acceptance or the physical T3 pullback cause.
- Final Rust format, scoped locked build and all-target clippy gates exit 0;
  adapter typecheck, full adapter suite, normal-bundle probe elimination and
  `git diff --check` pass. Rust tests use
  `cargo test -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture`;
  runner coverage uses `node --test scripts/kde-scenarios.test.mjs`, not live execution.
