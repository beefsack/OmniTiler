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
- Tentative technical shape: Rust `trailing: bool`; optional wire key emitted only
  when true, missing means false. Older strict readers need mechanical acceptance
  of the new optional key; this is backward reading, not old-reader compatibility.
- Tentative publication interpretation: the final ring row has the trailing role
  when empty under the existing lifecycle's non-sticky homed occupancy facts.
  Visibility, binding and retention do not change that lifecycle role. Shared
  rings use union occupancy; parity padding leaves only the final row designated.
  Incomplete/conflicting evidence publishes no positive designation.
- The macOS spare is the intersection of that explicit designation and no native
  binding; a bound trailing row remains an ordinary native-backed segment.
- Prior art: [workspace-model review](../research/macos-port/workspace-model-review.md)
  pins Defi ADR 0004 (populated trailing becomes ordinary) and paneru (stable query
  slots, no guaranteed trailing spare). Reuse the former invariant, not a new
  workspace store or paneru's optional reaping behavior.
- Tentative macOS projection details, implemented but parked: every core row is
  visible in core order; unbound non-trailing rows remain ordinary pending rows,
  disabled until backed. Current-only mode retains current plus the designated
  spare. Unbound rows use Unknown native occupancy styling, not a '+' label.
  Empty/unavailable core pager content hides the item without native inference.
- Exact-id clicks use existing `SelectWorkspace`, including the spare; keyboard
  numbered/trailing actions are unchanged. Coherent presentation/desire/generation
  snapshots and render/topology/binding/designation fences precede submission.
  Bound trailing rows remain ordinary clickable rows across marker changes.

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
- macOS projection is independently accepted locally, but parked, not published.
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
  involve completeness/hidden-focus expectations. No shared semantics changed here.
- Baseline integration failures: `skipped_display_parks_layout_then_resumes_exact`,
  `hidden_space_roundtrip_preserves_per_window_slots_across_blind_interval`,
  `skip_then_actual_removal_releases`,
  `space_type_gap_retains_hidden_layout_without_release`. Exact trigger remains
  unknown; no assertion weakening or timeout/retry workaround was applied.
- Overall full gate remains red, not waived. Reconcile those baseline expectations
  with native completeness semantics, then restore the parked projection and run
  full gates before publication. The source was preserved and removed from the
  working tree; this record remains active. Native pixels and physical feel remain
  untested; no live leg was run.
