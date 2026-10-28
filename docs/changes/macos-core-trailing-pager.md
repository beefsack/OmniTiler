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
- Locked core/protocol/macOS build passes. Latest full tests: core 998 library,
  protocol 30 library, macOS 1,385 library, plus 630 integration tests across 31
  suites; zero failures. Strict all-target clippy, workspace format and whitespace
  checks pass. Host Windows test targets compile; no native Windows/KDE run.
- KDE parser regressions are added but not executed: the available environment
  lacks its installed test dependencies and TypeScript compiler. Old exact-shape
  readers must upgrade with core; omission is not forward compatibility.
- macOS projection pending. Native pixels and physical feel remain untested; no
  live leg is needed for this offline change.
