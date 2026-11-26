# Core-designated macOS trailing pager segment

## Goal and scope

- Publish the existing core trailing-lifecycle role explicitly, then project core
  workspace rows into the macOS pager with a separate selectable unbound spare.
- Native Spaces remain the default. Virtual workspaces are a possible post-0.1
  opt-in investigation, not an implementation direction for this change.
- No workspace lifecycle, native creation policy, other shared contract extension,
  new live input harness, or KDE/Windows behavioral change.

## Plan and acceptance

1. Park the rejected occupancy-driven pager draft; restore only its source files.
2. Publish an optional trailing marker on workspace desires and pager rows, with
   mechanical reader/literal compatibility and lifecycle/publication/wire tests.
3. Render core rows directly on macOS. Only a core-designated unbound trailing
   row receives spare presentation; no `native_binding == None` inference and no
   occupancy-driven '+'. Selecting it uses the existing lazy-backing command path.
4. Review the shared shape and both implementations independently. Verify locked
   core/protocol/macOS builds and tests, strict all-target clippy, format and diff
   whitespace; preserve cross-platform/native acceptance limits.

## Decisions and source evidence

- User-approved 2026-10-28: the explicit trailing marker is the only authorized
  shared-contract extension. Existing trailing lifecycle remains authoritative.
- User-accepted 2026-10-29: Rust `trailing: bool`; optional wire key emitted only
  when true, missing means false. Older strict readers need a coordinated
  strict-reader upgrade to accept the new optional key; this is backward reading,
  not old-reader compatibility.
- User-accepted 2026-10-29: the final ring row has the trailing role when empty
  under the existing lifecycle's final-empty lifecycle-row classification,
  independently of binding, view and retention. Shared rings use union
  occupancy; parity padding marks the final row only. Incomplete/conflicting
  evidence fails closed with no positive designation.
- User-accepted 2026-10-29: the macOS spare is both the explicit designation
  and no native binding; a bound trailing row remains an ordinary bound
  native-backed segment.
- User-accepted 2026-10-29, prior art: [workspace-model review](../research/macos-port/workspace-model-review.md)
  pins Defi ADR 0004 (populated trailing becomes ordinary) and paneru (stable query
  slots, no guaranteed trailing spare). Reuse the former invariant, not a new
  workspace store or paneru's optional reaping behavior.
- User-accepted 2026-10-29 (implemented but parked): every core row is
  visible in core order; unbound non-trailing rows remain ordinary pending rows,
  visible but disabled until backed, with Unknown native occupancy styling and
  no '+' label. Current-only mode retains current plus the designated
  spare. Empty/unavailable core pager content hides the item without native inference.
- User-accepted 2026-10-29: exact-id clicks use existing `SelectWorkspace`,
  including the spare; keyboard numbered/trailing actions are unchanged. Coherent
  presentation/desire/generation snapshots and render/topology/binding/designation
  fences precede submission. Bound trailing rows remain ordinary clickable rows
  across marker changes; unbound-spare marker loss or binding arrival invalidates
  stale clicks.

## Evidence and status

- Rejected pager draft parked and only its five source files restored; reverse
  application checked before restoration and forward application checked after.
- Shape review rejected unviewed/unbound/untargeted classification because it
  would fork `workspace::plan_trailing`; implementation uses the lifecycle role.
- Shared marker implemented: one core classifier feeds both publication streams;
  both wire writers and byte accounting include the optional key. KDE readers
  accept missing/new keys without changing behavior; Windows/macOS literal and
  marker-dependent test expectations are mechanical compatibility updates.
- Independent implementation review required shared-union, occupied-tail and
  bound-tail regressions. These are added, including a corrected bound test that
  asserts an actual published native binding. Review permits landing after that
  test-only correction; its focused six tests and full gates pass.
- Locked core/protocol/macOS build passes. Pre-publication full tests: core 998 library,
  protocol 30 library, macOS 1,385 library, plus 630 integration tests across 31
  suites; zero failures. Strict all-target clippy, workspace format and whitespace
  checks pass. Host Windows test targets compile; no native Windows/KDE run.
- KDE parser regressions are added but not executed: the available environment
  lacks its installed test dependencies and TypeScript compiler. Old exact-shape
  readers must upgrade with core; omission is not forward compatibility.
- Shared marker published in `e422f59f`. The publication pull incorporated native
  completeness fencing (`ae073de9`); subsequent targeted checks passed, but full
  macOS checks below expose previously untested baseline regressions.
- The prior macOS projection was independently accepted locally, then parked.
  Review corrections enforce disabled-row refusal, exact-id/freshness fences and
  actual engine-issued spare ids. The exact-id spare test drives a real engine
  flight through creation desire and plan binding without native creation.
- A final source review found bound trailing clicks incorrectly compared the raw
  marker with unbound-spare eligibility. The correction compares effective spare
  predicates after binding equality; a bound-marker-flip regression and renewed
  independent review pass (57 focused tests). Full checks below precede this last
  narrow correction; no full rerun of that corrected candidate is claimed.
- Full candidate evidence: core 1,032 library + 469 integration, protocol 30
  library + 35 codec pass. macOS library: 1,392 pass / 4 fail; integration:
  146 pass / 4 fail. All eight failures reproduce on clean `e422f59f`, not just
  the candidate. Build, strict all-target clippy, format and whitespace pass.
  Composite cargo testing timed out; per-target execution covered the targets,
  with four core suites executed directly from freshly built test binaries.
- Baseline library failures: `cg_alive_unreadable_membership_holds_target_scope`,
  `cg_unreadable_oracle_holds_target_scope`,
  `first_transition_same_report_focus_never_restores_hidden`,
  `unreadable_scope_holds_without_write_authority`. The first two expect
  `Unreadable` but receive `IncompleteInventory` after `ae073de9`; the others
  involve completeness/hidden-focus expectations. The pager projection itself
  changed no shared semantics.
- Baseline integration failures: `skipped_display_parks_layout_then_resumes_exact`,
  `hidden_space_roundtrip_preserves_per_window_slots_across_blind_interval`,
  `skip_then_actual_removal_releases`,
  `space_type_gap_retains_hidden_layout_without_release`. Exact trigger remains
  unknown; no assertion weakening or timeout/retry workaround was applied.
- User-approved 2026-10-29 baseline approach: the eight failures above reproduce
  on clean HEAD after `ae073de9` Windows-workstream native completeness fencing.
  Triage each into a stale macOS expectation, a macOS adapter bug (fix in
  tiler-macos), or a core regression (return to the user; a core change needs
  user approval and Windows-session coordination). No assertion weakening and no
  timeout/retry workarounds. Then restore the parked reviewed pager projection,
  rerun full gates, and publish. The source was preserved and removed from the
  working tree at that checkpoint; this record remains active. Native pixels and
  physical feel remain untested; no live leg was run.

## Baseline triage and restored projection (2026-10-29)

- Clean `d5db9fdf` reproduced exactly the eight listed failures: macOS library
  1,382 passed / 4 failed; integration 146 passed / 4 failed.
- Intended `ae073de9` semantics are explicit in the
  [phase-1 outcome](windows-workspace-return-focus.md#phase-1-outcome-core-ownershipgap-foundations-2026-10-29),
  lines 2725-2732: host-owned domains require complete inventory, even when
  individual facts are known; publish `Hold(IncompleteInventory)`, retain homes,
  and recover without inferred close. Native-derived receipts have the same fence.

| Failure | Verdict and retained safety property |
| --- | --- |
| `cg_alive_unreadable_membership_holds_target_scope` | Stale expectation: assert `IncompleteInventory`, retaining target hold and `scope-held` send refusal. |
| `cg_unreadable_oracle_holds_target_scope` | Stale expectation: same reason-code change, retaining target hold and send refusal. |
| `unreadable_scope_holds_without_write_authority` | Stale partial-tiling expectation: assert all scopes held, no frame authority/geometry, then complete recovery and retained hidden binding. |
| `skipped_display_parks_layout_then_resumes_exact` | Stale partial-tiling expectation: nothing tiles during the gap; exact frames resume without retirement. |
| `hidden_space_roundtrip_preserves_per_window_slots_across_blind_interval` | Stale blind-interval helper-tiling expectation: no writes during the gap; covering helper membership on return restores complete inventory and exact per-window slots. |
| `skip_then_actual_removal_releases` | Stale assumption that topology removal alone completes inventory: add the parked app's covering unknown-home observation; retain no release during skip and release only after actual removal. |
| `space_type_gap_retains_hidden_layout_without_release` | Stale partial-tiling expectation: no writes or focus during the gap; retain bindings and exact-frame recovery. |
| `first_transition_same_report_focus_never_restores_hidden` | Core regression: an incomplete report's first minimized transition still permits a direct-target `Focus` effect on the hidden lifetime. Assertion remains unchanged and failing. |

- Seven tests are updated only in `tiler-macos`; no adapter production bug was
  demonstrated. Independent review accepts the expectation changes, truthful
  covering recovery observations and preserved hold/no-write/no-release/exact-resume
  assertions. It independently reproduces the core blocker and rejects full closure.

### Core blocker: same-report minimized target during an inventory gap

```sh
mise exec -- cargo test --locked -p tiler-macos --lib world_builder::tests::first_transition_same_report_focus_never_restores_hidden -- --nocapture
```

| Step | Report/command | Required outcome |
| --- | --- | --- |
| 1 | Complete inventory: `1001_7` and `1002_8` live on `space-11`, focus `1001_7`. | Establish both live lifetimes. |
| 2 | Same report first marks `1001_7` minimized and frameless, reports `1002_8` focused, makes `space-12` unreadable (`inventory_complete: false`), and commands `Focus(1001_7, left)`. | Reject `focus-unknown` without convergence, or accept with no hidden-target focus effect, geometry or controlled authority. |

- Historical reproduction: step 2 accepts and mints `EffectKind::Focus` on `1001_7`; the complete
  contrast passes. `world_commands.rs:70-103` checks retained session membership
  but not the staged minimized/held facts before minting the effect. The adapter
  supplies the correct minimized, frameless, native-home and incomplete facts.
- Resolved in the v4 implementation (2026-11-10): direct `Focus` rejects an
  explicitly staged minimized/host-hidden target with existing `focus-unknown`
  before retained membership can authorize it. Directional focus refuses the
  hidden source with its existing stale-source result. The guard is per-window,
  not a whole-inventory focus ban. Observations and non-authoritative held layout
  remain; minimized restoration is unchanged. No wire or contract change.
- Verification: the genuine macOS producer regression above passes both complete
  and incomplete contrasts with explicit native raw facts and macOS capabilities.
  Focused core regressions pass for minimized/host-hidden across both inventory
  states, including visible controls. Independent source review accepts the
  bounded guard; directional-source coverage pins existing refusal semantics
  rather than distinguishing the redundant guard from the carrier-index miss.
  Core/protocol build and all behavioral test targets pass; the workspace unsafe
  allowlist test fails on three unchanged fixture attributes. Core strict
  all-target clippy passes; protocol strict clippy fails on two unchanged
  single-element loops. Baseline attribution is source comparison, not a separate
  baseline execution. This resolves the focus defect, not full gate closure,
  pager native acceptance or the broader macOS v4 port.

### Current verification

- Rebased onto `41111fed` (world v3); both `tile` and `world_command_map`
  fixtures explicitly set `NativeOccupancy.count` to `None`. Independent merge
  review passed. Unknown occupancy, trailing-spare designation and exact-id
  selection rules are preserved.
- Builds pass for `tiler-core`, `tiler-protocol` and `tiler-macos`. Fresh test
  results (all Cargo test runs were per-target):
  - Core: 1,073 library + 498 integration passed across all 31 integration targets.
  - Protocol: 30 library + 35 `world_codec` passed.
  - macOS: 1,396 library passed / 1 failed; all 150 integration tests passed.
    The one failure is the same known hidden-focus test reproduced identically
    on clean origin `41111fed`; it is a baseline core regression, preserved and
    outside the pager change.
- Strict all-target clippy for core/protocol/macOS, workspace format and diff
  whitespace checks pass. Host compilation of core/protocol/windows passes;
  no Windows tests or live tests were run. The initial test `--list` discovery
  timed out while compiling, not during a test run.
- Offline verification otherwise passes; this does not claim full semantic
  acceptance, native/live closure, or native pixel/physical-feel acceptance.

## Signed-fixture pager projection (2026-11-26)

- The compatible `fixture-leg` Ctrl+Y run rebuilt the native indicator on a controller-owned Space. The complete rebuild projection reported four rows, one current row, one occupied row and one unknown-occupancy row across the mode changes. This establishes the live projection supplied to AppKit, not the painted highlight or unknown marker. The driver counted zero clicks.
- The compatible toggle run started the product on the owned Space and restored the original Space only after shutdown. The older `workspace-indicator-proof` controller is bound to the probe helper rather than the signed Live Fixture and was not used as substitute evidence.
- `fixture-leg --chord pager-switch --seconds 75 --signer REQUIREMENT` then exercised a public Mission Control switch from the occupied owned Space to a separately checked empty owned Space. Fresh native readback confirmed the destination became current; the running product adopted the topology but repeatedly held the scope for incomplete inventory after the fixture's AX windows left the visible Space. The indicator did not publish a newer rebuild for the destination within the bounded wait. The controller stopped the product and restored the starting Space and original topology without a retained receipt. This is a native shortfall, not a passing switch/rebuild proof; the diagnostic requires both native current Space and a newer indicator generation on each hop.
- Click handling, the painted current highlight and unknown marker, and fullscreen/game suppression remain unverified.

### Empty-destination report shape (documented fixture)

- Single native row: Desktops 1–2 are the baseline; the controller appends Desktops 3–4. One signed fixture window is visible on Desktop 3; Desktop 4 is verified empty. Before the switch, observation 4 is applied and accepted with one frame; indicator generation 2 has current ordinal 3 of 4.
- Public Mission Control switches 3 → 4; fresh native readback proves arrival. Topology updates, then scoped observations 5–10 each publish zero visible windows, emit `non-covering-retained count=1`, and produce `held-incomplete-inventory` and `reconcile-accepted count=0`. No newer rebuild for ordinal 4 appeared within eight seconds. Scope hold coverage matters here: zero frame rows is not proof of either a pager update or an empty authoritative world.
- `runtime_core::snapshot_is_covering` sees only `windows.is_empty()` and `excluded > 0`; zero live rows and zero exclusions are indistinguishable from an AX enumeration gap. `runtime_native` keeps the retained lifetime and sets the `blank-hidden` roster cause plus a coverage gap. The `rehydrate_cg_alive` escape hatch needs complete CG enumeration and a fresh hidden home for every absent roster candidate.
- The adapter's coverage gap marked both inventory and screen reports incomplete. Core correctly held idle native-echo adoption: logical current stayed on Desktop 3, and the indicator deduplicated the unchanged projection. This was not evidence for weakening the core guard.
- Teardown stopped the scoped product, returned to Desktop 1, removed only the two owned Desktops, restored the original row, and cleared the recovery receipt.

### Offline empty-destination diagnosis

- The earlier “no adapter bug demonstrated” conclusion was wrong. Observations 5–10 had `published=0`, `non-covering-retained=1`, `applied=1`, and `held-incomplete-inventory`, with zero accepted frames and no newer indicator rebuild. An unknown SLS home for the sole fixture window gives `applied=0`; the actual signature already pointed to a hold after successful re-home. The log did not include CG completeness or the raw owner IDs to distinguish the remaining possibilities.
- The adapter incorrectly counted auxiliary AppKit/winit CG owner entries, which AX never lists, as unresolved windows. Completeness now checks roster candidates, as on the covering path; unrelated CG owner entries cannot prolong the hold once every retained candidate has live AX, exclusion, or fresh non-current home proof. Unreadable CG and unknown SLS still hold; the latter drops the stale home. Core and wire are unchanged.
- The signed-fixture gate stays open. On the next leg capture correlated CG completeness, owner IDs and bounds, fresh SLS membership, roster IDs/homes, and re-home results. Require verified native arrival on the empty Space, a newer indicator rebuild with the destination current, the painted highlight, and no writes to the hidden fixture window. Unknown homes must still hold without writes.

### Signed-fixture retest (2026-11-27)

- On `aba030cd` (with `5f6d1b02`), the signed controller established owned
  Desktop 3 with one fixture window and verified that owned Desktop 4 was
  empty. Native current changed 3 → 4 → 3; complete indicator rebuilds
  followed at generations 2 → 3 → 4, with the correct current ordinal
  and one occupied row throughout.
- During the eight AX-blank observations on the empty destination, the
  product logged `published=0`, `non-covering-retained=1`, `applied=1`
  and no scope hold. Independent CG/SLS samples joined the subject to
  its original Space throughout; its settled frame stayed
  `(8,38,2224,1123)`. Four auxiliary CG entries had empty SLS membership.
  There were no subject writes while hidden; the first return frame was
  skipped as redundant. The controller verified clean product stop,
  starting Space and row restoration, and no residue or retained receipt.
- This closes the prior empty-destination rebuild failure, not MX6
  acceptance. The ordinary pager launch does not log product-internal CG
  completeness, full roster homes, or per-window re-home verdicts, so the
  external samples and applied counts are not a complete internal join.
  The status-item capture rectangle could not be revalidated, so the
  painted highlight was not captured. Unknown-home behavior, clicks,
  painted unknown markers, and fullscreen/game suppression were not tested.

## Backlog detail moved 2026-11-01

### Original backlog lines 1733-1740

````markdown
- P1 | macOS: core hidden-focus blocker then pager publication (P1 next) | Eight
  failures triaged: seven stale completeness expectations fixed in macOS tests,
  preserving hold/no-write/no-release/exact-resume safety; one core regression
  remains (`first_transition_same_report_focus_never_restores_hidden`). Incomplete
  same-report minimization still mints hidden-target focus. Reviewed pager restored;
  core/protocol and all macOS integrations pass, macOS lib has exactly this one
  failure. Resolve the core blocker, rerun full gates, then publish
  ([repro and evidence](changes/macos-core-trailing-pager.md#core-blocker-same-report-minimized-target-during-an-inventory-gap)).
````
