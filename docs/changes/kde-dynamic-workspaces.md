# KDE dynamic workspace lifecycle

## Goal and scope

Implement the 2026-11-10 [dynamic lifecycle decision](../decisions/workspaces.md#dynamic-workspace-lifecycle-and-kde-pager-backing): immediately back each trailing spare in KDE, prune ordinary unshown empty logical rows except required equal-count padding, and keep native deletion safety and the minimum-two native floor separate from portable core lifecycle. Preserve displaced/held units, stable native IDs and zero membership writes for gap cleanup.

## Acceptance and approach

- Cover separate, shared and global-unique modes with lifecycle regressions and per-step native create/delete/member-write counts.
- Prove eager spare backing, leading/middle prune after leave, shown-empty retention, no oscillation, KDE-only minimum two, and globally empty/unshown native deletion.
- Preserve migration, disconnect/reconnect and complete-inventory identity rules. No membership moves merely to compact gaps.
- Complete sequential core/protocol/CLI/KWin/runner checks, independent review and fixture-scoped native verification. Native verification must preserve helper receipts, full operation state and exact desktop names/IDs/order plus global and per-output currents before/after.

## Shared-roster correction (2026-11-11, offline verification)

- Manual native reproduction found count-only padding selecting different native
  subsets on different outputs. Local retirement markers then suppressed the
  missing counterparts, while sibling-live-binding protection correctly vetoed
  deletion. The resulting fixed point left globally empty native desktops in
  the pager and navigation skipped unmodelled pager entries. Independently
  minted pure-spare backing caused create/create/remove churn without membership
  writes. Earlier offline results below do not establish these cases.
- The approved required `sharedNativeRoster` connection capability separates
  shared native identity from equal logical lengths and linked selectors. The
  implementation chooses joint native retirement identities, holds divergent observations,
  elects one pure-spare creation coordinator and fans backing out to waiting
  spares. Migration still requires fresh backing and after-current insertion;
  native reordering remains observable. No aligned-grid replacement returns.
- KWin `1f543de908`, `src/virtualdesktops.cpp:577-615,990-1003`, keeps one
  `m_desktops` roster in both selector modes. The per-output flag changes
  `m_currentDesktops` selection, not desktop identity ownership. KDE therefore
  declares true independently of native selector coupling.
- Targeted regressions cover D1-D4, split readback, three-output election,
  backing fan-out, external native reorder, migration settlement and dependent
  migration anchors. A chained-migration regression exposed a pin replay-order
  error; dependency-aware placement corrects it. A genuine disconnect journey
  exposed a memberless orphan logical row becoming a second spare after
  displacement. The first cleanup dropped protected rows and left stale
  native bindings; review rejected it. The corrected cleanup protects retained,
  held and hosted rows, requires complete identical survivor evidence and
  known-empty occupancy, and clears dropped bindings for fresh return adoption.
  Serial readbacks exposed one remaining bypass: the removed ring's tail was
  retained unconditionally before those guards. The tail now passes the same
  redundancy proof as every other candidate; only an unprotected, known-empty
  native alias already represented on a survivor drops. Member-carrying,
  displaced, held and retained units remain protected. Independent roster
  journeys retain their original assertions.
- Current connection vectors are encoder-generated under
  `test-fixtures/world-wire/v4-active`, with explicit strict rejection of frozen
  old handshakes and unchanged generic scenario assertions. Explicit capability
  profiles distinguish shared-native KDE from independent native hosts and
  output-local Spaces; `hostOwned` never implies identity sharing. Genuine
  producer bytes use `test-fixtures/kde-v4-active`; both stage5 chains were
  re-recorded under `test-fixtures/stage5-v4-active`, and both production-route
  proofs remain enabled. Existing historical fixture bytes are unchanged.
- Minimal pre-existing close-focus amendment fallout was corrected in broker
  validation skeletons and Rust test inputs: a nonempty focused identity must
  carry known foreground, and every report needs the required field. This is
  distinct from the separately implemented
  [KDE close-focus adoption](kde-retained-close-focus.md), which now permits
  genuine current entry recording without a wire compatibility shim.
- Final sequential offline checks: Rust core/protocol/CLI 3283 passed, zero
  failed or ignored, with only the unsafe-posture test filtered; KWin 1272/1272,
  zero cancelled/skipped; both TypeScript configurations, Rust formatting and
  whitespace checks pass. The native-retirement target passes 52 tests, and the
  genuine disconnect/return target passes all four journeys without weakening
  its assertions. Stage5 includes exact projection, witness-chain, historical
  rejection and delayed-discovery replay checks. Frozen `world-wire/v4`,
  `stage5` and existing `stage5-v4` diffs are empty; the final disconnect fix
  changes none of the active recorded byte expectations.
- The core serial regression delivers one creation receipt per report through
  3 -> 4 -> 5 native identities, pins the intermediate desire and displaced
  binding, then proves quiescence with no orphan backing. Independent review
  verified the final unchanged regression fails with the former unconditional
  tail retention in a disposable source copy. It accepts the whole shared-roster
  lifecycle diff, including protection, fresh return adoption, migration anchors,
  fail-closed evidence and capability boundaries. No offline blocker remains.
  Native lifecycle acceptance and checklist rows 15/52 remain open; the live
  recheck below found a settle-time discrepancy. Retain this active note.

## Live recheck (2026-11-11, incomplete)

- Two-output, per-output-local command-route checks used native roster and
  binding readbacks. These are not physical-key delivery evidence. Artifact
  names below identify the retained raw captures, not committed fixtures.
- Startup emitted `world:ready`, `world:discovered` and accepted/applied
  revision 1. The current Planner log has no malformed-report rejection;
  the initial presentation `not-ready` response is bootstrap, not a malformed
  report (`lead-final-kwin-journal.log`, `lead-final-planner-log`).

| Check | Result and evidence |
|---|---|
| D1, each output | PASS: fills created one native each, with no create/create/remove flash. Revisions 35, 109 and 115 have `mk=1 rm=0`; subsequent settled replies have zero creation/removal (`cont-d1b-transfers.txt`, `lead-final-dev-log`). |
| D2, exact X/Y/Z close sequence | FAIL: revision 66 establishes X on A ordinal 2, Y on B ordinal 2 and Z on B ordinal 3. After close/leave/close/close, revision 74 still has three natives after 30 seconds. Both ordinal-2 rows are logically empty/unshown, but B's native cursor still names their backing while its logical view names ordinal 1. Deletion occurs only after the later send-follow, at revision 102. The captured view mismatch is diagnosed below (`cont-d2full-pres.json`, `cont-d2-settle-pres.json`, `cont-idle30-desktops.txt`, `lead-final-dev-log`:2333-2357,3195-3225). |
| D3, identity and navigation | PASS for the sampled three-native roster: every ID has a live binding on each output; relative commands cycle all three identities and wrap on each output, with native cursor readbacks (`cont-d3-summary.json`, `cont-d1b-transfers.txt`:15-20, `lead-final-dev-log`:1209,1266,1326,1381,1436,1498). |
| D4, sole directional send and leave | PASS for transfer and occupancy protection: the sole source window crosses outputs and the source view leaves. Its backing remains occupied/shown on the destination, so this is not independent globally-empty deletion evidence (`cont-d4a-pres.json`, `lead-final-dev-log`:3016-3123). |
| Previous-workspace send-follow | PASS: sole mover and view arrive on the previous destination workspace; the unshown, globally empty source backing retires once at revision 102 (`cont-d4b-pres.json`, `lead-final-dev-log`:3195-3245). This later result does not repair D2 acceptance. |
| Steps 9/12, both-output fill/close/idle | PASS for the bounded command-route leg: one trailing spare per output after fills, then two natives after closing test windows and leaving the shown empty source. The pre-existing client survives. Revision 126 remains unchanged over 15 seconds, with no dispatch/applied churn (`lead-final-dev-log`:3448,3649,3954-3986, `lead-final-desktops.txt`, `cont-idle-journal.txt`). |

- Independent evidence review rejects the initial D2 settle PASS and accepts
  the discrepancy above. No product correction is included in this recheck.
  The diagnosis below explains the view mismatch; do not treat later unrelated
  activity as settle-time success.
- The exact two-window close-focus leg and complete sole-close readbacks are
  still open; see [close-focus evidence limits](kde-retained-close-focus.md#live-recheck-2026-11-11-incomplete).
  Checklist rows 15/52 remain unchecked: the physical send matrix and complete
  inverse/repeated lifecycle scope are not established by these bounded results.

### D2 diagnosis (2026-11-11, no product correction)

- The first native mismatch follows external activation, not a failed commanded
  view write. Revision 61 reads B's native current as ordinal 3; the focus helper
  activates Z while Z still belongs to ordinal 2, and revision 62 reads ordinal
  2 with Z focused and `views=0`. The later send of Z to ordinal 3 uses
  `follow=false`, so it does not switch B back (`cont-focus-helper.js`:27,
  `lead-final-dev-log`:1901-1906,1972-2041). Revision 66 still publishes ordinal
  3 as B's logical view. Logical state at revisions 61/62 is inferred from the
  endpoints and operations; intermediate presentations were not retained.
- On Z's close, ordinary pruning removes that now natively unshown row.
  `world_model.rs:369-399,442-460` resets the removed retained view to the ring
  head without creating a view transfer. Output publication reads a pending
  view transfer or the retained map (`world_publish.rs:1828-1843`), whereas
  pruning/retirement uses the known native echo (`world_observe.rs:154-176`,
  `world_native.rs:1252-1261`). No idle path reconciles those two views.
- The adapter reads genuine per-output currents
  (`workspace-native.ts:473-497`) and only actuates explicit retained view
  transfers (`world-apply.ts:1755-1846`). Its any-output visibility veto
  (`world-native.ts:884-917`) correctly protects the actually shown ordinal-2
  native. Thus this is a persistent publication/reconciliation defect, not
  evidence that a displayed native should be deleted or a setter retried.
- Earlier global-current staging is not the immediate trigger: the later
  output-local sequence had aligned native/logical views before activation.
  KWin at `1f543de908`, `src/scripting/workspace_wrapper.cpp:260-263` and
  `src/activation.cpp:294-317`, routes the helper through ordinary activation;
  its switch-to-other-desktop branch changes the window's output-local current.
  The native-activation class is user-reachable. The exact physical step-8
  action was not captured, so occurrence in that particular sequence remains
  unverified; no fresh live reproduction is claimed.
- One policy-neutral red regression was appended to
  `world_native_retirement.rs`:
  `idle_external_current_move_leaves_no_unexplained_view_divergence`.
  After three complete idle reports with current and focus moved together,
  it requires either publication matching the native current or an explicit
  view transfer owning convergence. It separately preserves shown-native
  protection. The permitted filtered core/protocol/CLI run fails at the intended
  convergence assertion, with retained publication on the old desktop and no
  view transfer (`d2-diagnosis-red-final.log`:261-275). Independent review accepts
  the invariant and causal diagnosis; no production behavior was changed.
- Recommended follow-up, not an adopted policy: reconcile a complete known
  idle native echo into retained view state when no competing transfer owns
  selection, preserving held/unknown/flight fences. This avoids fighting a
  native user switch. Explicit bounded reassertion is an alternative requiring
  a selection-authority decision; diagnostics alone leave the mismatch, and
  weakening deletion visibility guards is unsafe. Echo adoption can legitimately
  retain the shown empty ordinal-2 desktop; it does not promise deletion of that
  identity or a two-native roster before the user leaves it.

### Idle native selection correction (2026-11-11)

- User 2026-11-11 selected [idle echo adoption](../decisions/workspaces.md#idle-external-native-selection), superseding the diagnosis's unselected alternatives. Core now adopts a complete known output-local native current into its retained view when no view/migration transfer owns selection. It emits no selection transfer or reassertion. Unknown current, missing native rows, incomplete inventories/screens and pending stale echoes do not gain authority.
- View sanity now prefers the known shown logical row over a different ring head after lifecycle invalidates a retained view. Existing unknown fallback is unchanged: absent proof of a current, this change neither invents a selection nor adds a hold. Gesture holds still fence writes/deletion, not observation-only adoption. Shared mirroring, scoped separate rings and history remain unchanged.
- Local retired-native selection first clears the local marker and adopts fresh logical identity; idle reconciliation then observes that live identity. Shown-native deletion protection is unchanged. Closing on the adopted desktop retains it empty while shown; leaving permits ordinary pruning and safe native deletion.
- Fourteen idle regressions include the original red invariant, unknown/incomplete evidence, known-echo fallback after third-location flight retirement, pending view and migration/refill flights, shared/global-unique scope, close/leave retirement and a two-output activation/send-stay/close journey. That journey uses independent synthetic native rosters and adjusted spare placement: it is trace-shaped logic coverage, not exact shared-roster revision-62-to-74 replay. Existing retired-native reselect regressions remain enabled. The reorder regression now independently proves reorder-only stability and simultaneous native-selection adoption rather than treating a changed current as reorder alone.
- Verification: `cargo test -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture` passes 3297 tests, zero failed/ignored; only the unsafe-posture test is filtered. `npm test` in `kwin` passes 1272/1272 with no skips; `npm run typecheck` passes both configurations. Rust formatting passes. Frozen and active recorded fixtures are unchanged; no recording is required by current byte expectations.
- Independent review accepts core adoption, fallback, retired-marker ordering, stale-flight protection and added unknown/scope/migration coverage; it explicitly retains the trace-fidelity limit above. Initial verification found a reversed completeness flag in a new test and an obsolete reorder/current-change oracle; both test inputs/oracles are corrected. Native recheck remains pending; prior D2 failure and checklist rows 15/52 stay open until fresh evidence establishes their scope.

## Units and dependencies

1. Diagnosis and pinned reference comparison (retained below).
2. Portable lifecycle and eager backing regressions/implementation (offline; equal-count exception below).
3. Complete generic native-only retirement publication and adapter integration (offline checks below).
4. Full sequential checks, independent review and native lifecycle verification.

## Pre-decision diagnosis and supporting observation (historical)

The reported log shows ordinary trailing lifecycle appended an unbound logical
spare after the last workspace became occupied. Native creation was not refused:
an empty, unviewed, untargeted spare deliberately requests no backing desktop
(`world_native.rs::logical_needs_backing`). Existing producer coverage explicitly
asserts this distinction. No existing-contract correction is established.

A fixture-scoped observation reproduced native counts 5 -> 6 only on selecting
the logical spare, with one creation and one view write, no membership writes or
removals, then stable repeated readbacks. Filling it left the next spare logical
only. This is supporting observation, not acceptance: full operation-lane state
was not captured, and the baseline recorded global current desktop rather than
per-output currents. Original native IDs/order and global current match the
final readback; desktop-name restoration is not established by those captures.
Repeated read-helper failures and missing lifecycle receipts further prevent an
acceptance claim. Physical acceptance remains open.

Final read-only checks found the dev stack down, no input daemon, and no script
child objects in KWin scripting introspection under the same compositor owner.
This establishes final absence, not reconstruction of missing helper receipts or
the uncaptured original per-output currents.

Making every ordinary spare visible in the KDE pager requires a native-backing
policy decision, not simply repairing an omitted planner invocation. Pruning and
minimum-count placement remain undecided.

## Diagnosis (historical)

- Reported log revision 20 homed an arrival on the final bound logical workspace
  and appended an unbound spare. Later observations retained it; native creation
  counts remained zero. This is the policy encoded in
  `world_native.rs:816-857` and the producer regression
  `world-aligned-spare-occupancy.test.ts:645-724`.
- `world_model.rs:249-269` marks native-bound logical identities retained;
  `workspace.rs:49-50` therefore excludes them from ordinary empty cleanup.
  The core planner runs, but neither ordinary spare backing nor native-bound
  pruning follows from that path. `world-apply.ts:1458-1470` requires an explicit
  native creation desire; it does not infer authority from an unbound row.
- Settings did not prevent logical lifecycle. Exact separate-view mode in the
  original capture is not established. No native creation refusal was found.

## Reference comparison and decision discrepancies

The [functional discriminators](../spec/reference-outcomes/workspaces.md#dynamic-lifecycle-discriminators-2026-11-10-source-only)
record source-only Tier 1 outcomes and exact pins. Leading/middle empty cleanup
after leaving is supported by all four references under their eligibility
guards. All retain an active/shown empty workspace. COSMIC compacts indices;
i3/sway retain names and Hyprland retains numbered identities. Only COSMIC of
these four supplies an automatic trailing spare. No explicit minimum-two or
pager-related gate was found in the examined lifecycle rules; exact counts and
pager behavior remain TBD.

Expectations 1/2 need a current pruning decision, including shown-empty
protection. Expectation 3 matches the logical-spare decision, but eager native
pager growth is not what the current implementation authorizes. Expectation 4
already has core and KDE constants, but its intended ownership is unresolved.
An older active [native-removal entry](../decisions/layout-and-admission.md#native-empty-workspace-removal-and-padding)
and REQ-WS-10 retain global-removal/minimum-two wording, conflicting with the
2026-11-08 retirement of global-hole/removal rules. No entry is silently
reinterpreted here.

## Pre-decision KDE options (historical)

| Option | Native/window churn | Tradeoff |
|---|---|---|
| Logical-only prune, retain native desktops and stable bindings | Design target: zero membership writes and zero native removals; not measured | Per-output ring compacts independently, but KDE pager retains padding. Must prevent native observation from re-adopting a deliberately hidden logical row |
| Retire native desktop only when empty and unshown on every output, unprotected and above native floor | Design target: zero membership writes; one native removal per globally eligible desktop; not measured | Preserves windows elsewhere, but local holes can remain in native pager; surviving native ordinal numbers may shift |
| Never renumber any native desktop hosting windows elsewhere | Zero ordinal changes requires holding any removal before such a desktop; not measured | Cannot combine arbitrary native middle deletion with stable native ordinals. Logical-only compaction remains possible |
| Eager backing for each logical spare | Additional native creation for an unbound spare rather than waiting for selection; not measured | Makes spare visible to native pager, potentially creates padding on other outputs; needs coalescing and anti-oscillation verification |
| Compact native positions by moving occupants | Membership writes proportional to affected occupants; not measured | Risks the reported cross-output churn; unnecessary for logical compaction and not recommended |

KWin @ `1f543de908`, `src/virtualdesktops.cpp:448-480,492-519`, inserts/removes
global objects and renumbers subsequent native ordinal positions. Removal
retains surviving object IDs; ordinal changes are not themselves membership
moves. Its native floor is one (`494-496`), so minimum-two is not a KWin API
requirement. Current fresh KDE occupancy and any-output visibility vetoes live
in `world-native.ts:833-917`, consumed by `world-apply.ts:1592-1654`.

Measured supporting observation, excluding setup/cleanup: filling the bound
end created no native desktop; selecting its spare created one desktop and
changed one view; filling that selected spare created no additional native
desktop. All retained application lines had `member=0`, `rm=0`; five repeated
readbacks were stable. Pruning and eager-spare options were not implemented or
measured. No churn measurements are claimed for them.

Recommendation for consideration: keep minimum-two, if retained for pager UX,
as a KDE native-desktop floor rather than a universal logical-ring rule. A
cross-platform logical minimum needs its own product rationale; empty-spare
semantics alone do not imply two when the shown workspace is empty. Prefer
stable bindings and zero occupant relocation if logical pruning is selected;
globally empty-only native retirement can be a separate decision. None of
these recommendations changes the current contract.

## Open acceptance

- Product policy, native-only retirement authority and the equal-count exception are selected in the 2026-11-10 decisions. Offline suites and independent review are recorded below; native acceptance remains open.
- A fresh acceptance leg must capture per-output currents and full operation
  state, preserve all helper receipts and stop on the first accounting failure.
- Checklist rows 15 and 52 remain open; supporting observations do not close
  physical delivery or inverse fill/retire oscillation acceptance.

### Sticky occupancy boundary (2026-11-10)

- Both KDE native-occupancy counters exclude on-all-desktops windows, including panels, docks and user sticky windows: deleting an otherwise eligible desktop moves none. Ordinary managed/unmanaged occupants and non-sticky subset memberships still count; unreadable membership remains fail-closed.
- Core sticky-home retention is unchanged. A managed window made sticky keeps its retained logical home and protects that desktop; excluding native occupancy is not authority to discard its home.
- Targeted red/green regressions cover both counters and the real capture/core/apply boundary, including retained managed sticky homes. The initial full adapter run passed 1239/1241: one lifecycle assertion depended on sticky panels preserving native stones, and the mutable byte captures still counted sticky occupants. Reconciled lifecycle assertions distinguish two actual creation receipts from a superseded, deleted intermediate, preserve quiescence/zero-member-write proofs, and protect the stone using a genuine non-sticky subset occupant. Genuine recorder refresh changes only native occupancy in the three mutable KDE byte reports; frozen fixtures are unchanged.
- Final adapter suite passes 1241/1241 with no skips; both TypeScript configurations, Rust formatting and whitespace checks pass. No Rust or runner source changed. Independent review accepts the counter exclusion, retained-home boundary, strengthened lifecycle assertions and occupancy-only mutable fixture refresh. Native acceptance and checklist rows 15/52 remain open.

## Implementation checkpoint (2026-11-10, incomplete)

- Portable lifecycle now uses a single-spare invariant instead of an independent minimum-two floor, immediately desires backing for trailing spares, and prunes ordinary empty/unshown rows into native-only retirement records. KWin applies the existing fresh occupant/visibility/minimum vetoes and bootstraps its native floor to two. No new wire fields or format were needed.
- Core tests: initial nine-test run recorded 3 passed / 6 failed before the first correction. Four real-pipeline marker/retirement tests were then red before correction and green after. Two further generic fake-host rotation regressions bring the new test count to 15. The latest complete core run recorded **2590 passed / 0 failed**, baseline 2575 plus 15, across 33 targets including documentation tests.
- The original 20 library failures and later integration failures were reconciled against the changed singleton/spare policy, retaining their actual selection, migration and settlement proofs. Multi-row fixtures now provide real or minimized occupancy where needed; outside-adoption controls provide an already backed empty spare and distinguish unrelated spare creation from creation of the adopted identity. Retirement tests distinguish live ring rows from absence records and establish guards before pruning rather than issuing commands against already removed identities.
- Native-only intents are fenced on incomplete inventory/native coverage, gesture barriers and backing still live on another output. A real-pipeline reselect test exposed a create/delete rotation: an output-local tombstone deleted another output's still-live empty backing. A generic live-binding veto fixed it. Before correction the recurrence test observed repeated create/delete churn with zero transfers; after correction both shared-native and independent-native hosts quiesce, and the adapter pipeline reselect test settles with zero subsequent create/delete/member writes.
- KWin tests: the complete sequential suite now passes **1231/1231** (baseline 1224 plus seven), with no skips. The new pipeline tests exercise eager backing, fill-to-next-spare, shown-empty retention/prune-after-leave, global occupant/any-output view vetoes, reselect/admission and quiescence. Bootstrap is proven with an unoccupied single-desktop host: every reply carries no core creation desire and every capture carries no creation-attempt receipt, isolating adapter floor creation.
- Scenario runner tests recorded **130/130**, protocol **288/288**, and CLI **341/341**; both TypeScript configurations passed type checking. The CLI sender-isolation fixture initially failed because it selected index 2 from an empty singleton world. A minimized occupant with native backing restores both valid numbered targets; all issuer/highwater/retake assertions are unchanged. Targeted red/green and the complete CLI rerun pass. No new native leg has run; checklist rows 15/52 remain open.
- Mutable `kde-v4/bytes` and `stage5-v2` captures were refreshed through genuine production recorders. The stage-5 fake host now fulfills native creation instead of leaving a void-stub attempt pending indefinitely. Genesis consequently has initial/attempt/settled captures; the third is byte-exact and attempt-free, all three retain the coverless hide proof, and later step envelopes remain byte-exact and attempt-free. Existing epoch-stamp oracles and byte normalization rules were not weakened. Frozen `world-wire/v4`, `stage5` and existing `stage5-v4` bytes remain unchanged; both Rust stage-5 proofs remain enabled.

### Implementation choice: locally retired-native markers

Use internal per-output `(output, native id)` retirement markers, not logical bindings outside rings. Each marker stores the removed logical ID solely to correlate its absence record. Removing an ordinary empty/unshown logical row clears its binding and marks its native pair, suppressing ordinary re-adoption while native backing persists. Positive output-local current selection or observed window presence clears that pair; ordinary adoption then creates a fresh logical identity at native order among live bindings. Complete native absence clears the marker too. The existing settled migration source-release path clears only the migrated workspace's vacated source pairs before fresh adoption; this preserves D9, not general readoption without local evidence. Preserve disconnect/displaced behavior; never retire held/displaced units. This is an implementation choice for the selected lifecycle behavior, not an additional user decision.

Global native occupancy never clears a local marker: the producer can repeat the same global counts on every output, so only output-local current/window evidence proves re-adoption. Core fake-host tests cover shared-global and independent per-output native identities. Platform minimums and native ordinal renumbering remain adapter concerns.

### Native retirement authority amendment (User 2026-11-10)

The [approved generic amendment](../decisions/contract.md#native-only-retirement-intent) extends the existing host-owned native-desktop capability to native-only absence records. Core publishes existing `exist:false` rows for each retired output/native pair until deletion or re-adoption. The removed logical ID is correlation only; these rows are not live ring/spare/occupancy members. No new wire key, format or platform selector is introduced.

The [world contract](../porting/world-contract.md#native-only-retirement-with-per-output-rings) keeps core `exist:false` as the sole native-removal authority. Adapters never delete autonomously or from omission; they apply fresh global occupancy, any-output visibility, protection, completeness and minimum-count vetoes. Native-only intents also protect backing still live on another output.

KWin retains `minDesktops: 2` and adds floor bootstrap creation from fresh native enumeration. Ordinary spare backing still needs explicit core creation desires. Existing source refill backing is preserved for the same source logical spare when eager backing predates migration; target migration backing still requires its own fresh creation, with anti-cannibalization assertions retained.

## Independent review and remaining blockers

- Independent diff review found no new wire fields, platform-specific policy in new portable code/contract text, autonomous deletion, or migration/displaced ownership change. The self-equal new-test oracle was replaced with a pre/post ring snapshot comparison; stale test comments and the one-sided retirement name were corrected. Five targeted core regressions and all seven new adapter pipeline tests pass after those test-only changes; both TypeScript configurations pass again.
- User 2026-11-10 selected equal logical ring lengths as an explicit ordinary-empty pruning exception, kept as small and isolated as possible. KDE's global desktop list already makes every pager show the longest list; equal logical lengths model every listed desktop at no extra native-desktop cost. Existing `world_model.rs` capability-based removal clamp and padding remain unchanged. `world_native_retirement.rs::common_plus_extra_spares_only_extra_under_equal_count` now explicitly pins retention of x at 3/3 and pruning of x at 2/2 after the longest ring shrinks. This is logical padding, not the KDE native floor; no new wire shape or KDE-specific core behavior is added.
- Native verification is unrun. Fixture-only window scoping does not protect existing empty native desktop IDs from retirement. Appending a fixture region alone cannot establish exact baseline-ID restoration while lifecycle still scans the full roster. A safe boundary must protect baseline IDs or isolate the managed roster before startup; alternatively a different restoration criterion needs explicit selection. No native baseline, startup, restoration, dev-stack-down or input-daemon-absence claim is made by this checkpoint.

### Final offline verification (2026-11-10)

- `cargo test -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture`: 3221 passed (core 2592, protocol 288, CLI 341), zero failed/ignored after integration with current main; only the named unsafe-posture test filtered. Equal-count retention and shrink are asserted in the existing regression; no new production behavior was needed for the exception.
- KWin complete suite: 1231/1231, no skips. Both TypeScript configurations, scenario runner 130/130, Rust formatting and whitespace checks pass.
- Independent review accepts the capability-based exception, shrink oracle, existing wire shape and native safety fences. Its stale R-WS-10 reference wording finding is corrected. Native/live acceptance remains unrun; retain this active note for that leg.

### Existing assertion reconciliation

| Tests | Changed setup/oracle; retained proof |
|---|---|
| `workspace`; logical bootstrap/capability tests | Two-empty floor becomes one spare; native bootstrap may have a single bound empty row |
| Logical selection/float/queued/migration and `world_float_boundary_tests` | Real/minimized occupants retain required multi-row fixtures; source stayer remains source-homed; no selection/focus gate changed |
| Admission held-scope; renderer ring reorder | Capabilities match fact lanes; mover is observed focus; original hold and stable-ID/ordinal assertions retained |
| Observe capture; prior context; migration refusal | One actual trailing-spare desire; singleton may itself be trailing; no extra third refill row required |
| Selection await | Focusless fixtures retain source via minimized seed or explicit native rows; zero-member/focusless completion remains the oracle; unrelated sticky frame hunks are separate |
| Gap holds; host-owned stage1b | Eager-spare desire replaces obsolete no-creation assertions; outside-adoption fixture pre-backs spare and still proves fresh adopted ID, no rename and zero transfers |
| Modes; native adoption; truthful settlement | Occupants preserve intended identities; independent spare desire distinguished from outside adoption; contradiction and settlement proofs unchanged |
| Native retirement | Live-row helpers exclude absence records; guards established before prune; tombstone IDs correlate removed rows; incomplete/gesture/occupancy/view safety proves host-actuation veto rather than resurrecting logical rows |
| CLI sender isolation | Minimized native occupant preserves valid index 1/2 targets; namespace/highwater/exact-retake assertions unchanged |
| KWin aligned-spare/disconnect | Eager desires joined to actual receipts; stable opaque fake IDs; reselect mints fresh logical ID; ordinal selection derives from live ring; displacement unchanged |
| KWin split-edge | Same logical source spare may already be backed; destination still freshly created; no alias/cannibalization and transfer-identity assertions retained |
| KWin QML list; stage5 recorder/fixture | Native fake has readable names; creation fake actually fulfills request; genesis adds settled third capture without weakening byte/stamp/coverless-hide assertions |
