# Fullscreen float toggle

## Goal and scope

- User-approved 2026-10-28: explicit float toggle changes base membership
  immediately beneath retained fullscreen; siblings reflow. Toggle again
  fresh-admits to tiling. Exit reveals the last base state.
- Shared core policy and KDE adapter only. No Windows/macOS adapter edits,
  maximize-on-fullscreen changes, automatic fullscreen changes or live rollout.
- D5: no writes to the fullscreen subject, including geometry, focus,
  stacking or native state. Other windows may reflow.

## Acceptance and approach

1. Verify pinned i3/sway double-toggle and record a minimal R-MAX-10 variant
   before implementation.
2. Permit fullscreen float membership changes in shared Session policy;
   preserve maximize refusal and legacy native-route safety fences.
   Retain durable float placement (centered 60% first-time fallback), never
   capture the fullscreen frame as a float placement.
3. Route KDE explicit intent through core; retain fullscreen write guards.
4. Add real-engine/core and realistic KDE regressions for single/double toggle,
   sibling reflow, no subject writes, both exit states, retained frame and
   maximized refusal. Inspect Windows/macOS consumers for follow-up gaps only.
5. Run documented touched-crate and KDE suites, formatting, clippy and diff
   checks; review evidence; archive on delivery. Native acceptance stays pending.

## Evidence and decisions

- Pinned source: i3 `903bcd518df32b0e055b17f5da3f988a0187fd3d`
  `src/floating.c:232-472`; sway
  `1652c54b73f67df17b7b4ab0b0f7048204aa8104`
  `sway/tree/container.c:955-1030,1260-1310,1402-1487`.
  Both enable and disable floating without a fullscreen guard, detach the
  tile on the first toggle, and fresh-insert on the second without clearing
  fullscreen. Exit therefore leaves B tiled; exact native geometry is TBD.
  This confirms the approved double-toggle behavior, with no tentative
  departure from the user decision.
- Initial repository state clean. Existing refusals occur in shared
  `window_policy.rs` and KDE `world-native.ts`; D5 apply guards remain required.
- Implemented narrow fullscreen-subject tolerance for Session float removal
  and fresh admission; other commands keep their existing matching rules.
  Admission filters fullscreen-subject geometry; ordinary internal admission
  focus bookkeeping is not a native focus write. KDE's existing live-state
  actuation guards prevent native fullscreen geometry/focus/stacking writes.
- Independent review caught an unsafe integration in the initial helper
  change: Windows' legacy native float route consumes `float_toggle_refusal`
  before geometry/topmost writes. Retained that helper's original refusal
  behavior rather than opening the native route prematurely. User 2026-10-29
  approves retaining this legacy helper until Windows integration, tracked
  separately in the backlog. No adapter implementation for either changed.
- Corrected initial coverage gaps: real Engine regressions now exercise both
  exits, exact fallback and durable placement, and actual sibling expansion;
  KDE fixture uses consistent fullscreen/output bounds and records native
  state setters as well as geometry/focus/stacking writes.
- An initial all-unfloat geometry filter broke the capacity-float regression;
  narrowed it to the fullscreen subject only. Full core coverage passes.
- Reconciled the functional-spec totals: 152 NORMATIVE, 28 OPEN, 180 rows;
  no new reference scenario was needed for the double-toggle discriminator.

## Verification and outcome

- Delivered offline: implementation, source/diff review and scoped gates
  accepted. Commands below use `devenv shell --impure --` except `just`/`git`.
- After integration with `520b81dd`, affected gates were rerun successfully.
  `cargo test -p tiler-core -p tiler-windows`: 2,193 passed, none failed or
  ignored. Core: 1,021 library plus 446 integration tests; Windows portable:
  327 library plus 399 integration tests. This change leaves Windows/macOS
  adapter implementations unchanged.
- `npm test --prefix kwin`: isolated rerun 794/794 passed, including all five
  new fullscreen cases. Earlier concurrent workspace/KDE run had one existing
  restart fixture `DiscoverWorld` timeout; isolated rerun resolves it as a
  transient, without timeout or oracle changes.
- `cargo test --workspace`: macOS library has 1,362 passed, 9 failed with
  `macOS major unreadable`. `cargo test -p tiler-macos` on unmodified
  `origin/main` at `ed9c6533` reproduces the identical nine failures:
  pre-existing on Linux, unrelated to this change. No macOS code fix included.
- `cargo clippy --workspace --all-targets -- -D warnings`: macOS fails with
  26 library / 16 library-test unused/dead-code and needless-return errors.
  The identical command on unmodified `origin/main` at `ed9c6533` reproduces
  the same diagnostics; pre-existing on Linux, unrelated. Strict clippy with
  `--workspace --exclude tiler-macos --all-targets -- -D warnings` passes.
- `cargo fmt --all -- --check`, `npm run typecheck --prefix kwin`,
  `just check-portable` and `git diff --check` pass.
- Physical/native acceptance pending; no live tests planned for this unit.
- Follow-ups: Windows/macOS adapter integration and native acceptance are
  tracked separately; maximize-on-fullscreen R-MAX-11 remains untouched.
