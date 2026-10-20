# macOS fixture and tiler integration

## Goal and acceptance

Verify the signed macOS live fixture as the disposable window source for
fixture-scoped tiler tests: admission, one to three tiled windows, socket and
native geometry agreement, close/reflow, and clean teardown. Exercise
fixture-only focus/move and managed-workspace actions where existing harness
contracts support them.

## Scope and approach

- Use the dedicated regular-policy fixture bundle and existing exact bundle
  scope, native lifetime binding, and readback routes.
- Establish bounded tap, action and recovery routes before live integration.
- Preserve protected-window records and all unrelated desktop state; retain
  scoped raw evidence outside the repository.
- Fix fixture or harness defects only; product behavior, shared-core changes
  and new dependencies are outside this change.

## Verification and outcome

- Native standalone fixture support was already verified and repaired in the
  [first-use record](macos-live-fixture-smoke.md). Integrated checks used
  the signed regular-policy fixture bundle and exact existing bundle scope.
- Two bounded tap runs completed with one-to-three `Tile` admission, native
  reflow, socket geometry reconvergence, socket focus and clean shutdown.
  Follow-up readback checks closed the initial evidence gaps: native sentinel
  title-to-caller joins, exact `AXFocusedWindow`, socket geometry after both
  closes, run-time artifact hashes and pre/post native Space row/current.
- Full socket and AX/CG snapshots verified one full tile, two split tiles,
  then a full-height tile plus two stacked tiles. Closing the middle then
  first window produced the two- then one-tile inverse. Physical socket outer
  positions divided by scale matched native positions; content widths matched
  outer widths and the measured decorated height delta was 32 logical points.
- Socket focus selected the exact native window twice. An accepted client
  resize reconverged to the retained tiled geometry; its transient frame was
  not sampled. These are not product focus/move hotkey proofs.
- Tap outcomes were `completed`, `shutdown_clean:true`, clean joins and no
  retained workers, input refusal or disable events. No chords were delivered.
  The launcher returned nonzero because stderr contained live diagnostics;
  the terminal JSON and native teardown, not wrapper status, prove completion.
- One transient backend observation failed and a later snapshot published
  successfully; no topology failure or shutdown shortfall was reported.
- Harness-only corrections handled reply parsing, auxiliary offscreen AppKit
  CG entries, and the expected inter-tile gap. No fixture or product source
  changes were needed after first use. Independent evidence review identified
  the initial oracle gaps; the final readbacks verify those gaps explicitly.
- All created fixture/tiler windows and processes, sockets/run directories and
  awake helpers were cleaned. Protected raw CG records, displays and native
  Space row/current matched their preimages. No holders, Space membership or
  settings were mutated; existing resources were left intact.
- Remaining: guarded `owned-post`/`owned-focus` authenticate legacy probe/helper
  records only; diagnostic stay/trailing injections require TextEdit. Product
  focus/move and managed select/send/trailing therefore need physical fixture
  checks or a verified fixture-aware diagnostic ownership contract. No gate was
  weakened and no new authorization scheme was introduced for these checks.
- Point-of-use procedure and coverage are in the [fixture README](../../../crates/tiler-live-fixture/README.md#macos-with-omnitiler-active-2026-10-20).
