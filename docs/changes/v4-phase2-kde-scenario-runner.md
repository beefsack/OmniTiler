# V4 phase 2: KDE scenario runner

## Goal and status

- Exercise the genuine KDE producer and ordered native journeys without
  substituting transport receipts for native outcomes. Preserve a usable build.
- Selected approach: A below, delivered in bounded units. The current unit is
  the lifecycle skeleton and exactly one native tiling/reflow scenario using
  two fixtures on an appended workspace. Earlier drafts were rejected for the
  correctness and safety findings below. No native execution, usable runner,
  cull or acceptance is claimed yet.

## Record interpretation

- The [slice table](archive/v4-phase2-kde-port.md#later-dogfoodable-slices)
  places this after scaled metrics and physical reconnect/rehome and before
  remaining v3 removal. The [implementation order](../decisions/contract.md#contract-authority-and-v4-migration)
  and [phase-1 increments](v4-phase1-core-protocol.md#planned-increments)
  also retain ordered scenario execution and the eight OPEN evidence gates.
- Two different boundaries must not be conflated. The immutable
  [scenario format](../../test-fixtures/world-wire/v4/manifest.json) defines
  deterministic core/adapter/renderer/storage models, explicit ordered inputs,
  jq oracles over output/history and pending rather than passed OPEN/TBD steps.
  Its native world is fake; its scope expressly excludes native acceptance.
  The KDE slice additionally names genuine producer and ordered native evidence.
- Existing producer-route proofs are not a general scenario runner:
  [stage5](../../kwin/tests/world-stage5-fixture.test.ts) regenerates and compares
  two recorded presentation chains; [migration](../../kwin/tests/world-aligned-migration-producer.test.ts)
  and [FIFO](../../kwin/tests/world-entry-fifo-engine.test.ts) run specific
  journeys through the real entry, broker and core with scripted native surfaces.
  [Staging](../../crates/tiler-protocol/tests/engine_staging_v4.rs) explicitly
  does not execute scenarios; the [connection transcript](../../crates/tiler-protocol/tests/connection_transcript_v4.rs)
  executes selected messages from one scenario, not its complete oracle format.
- No compatible v3 runner has been identified to port. The archived
  [observation replay](archive/observation-trace-replay.md) targets the old
  directional planner/Reconciler and excludes adapters and native action.
  The legacy world-wire generator rewrites v3 reply fixtures, not scenarios;
  leave it unrun and retain its recorded bytes.

## Selected design

- Use two explicit execution boundaries, without a second policy engine or a
  transport-stamping shim. Reuse Node/TypeScript and existing development tools;
  no new dependency is proposed.
- Offline producer journeys belong beside the current `kwin/tests` producer
  tests: real `startWorldAdapterEntry` -> retained `broker_world_eval` -> real
  core, with controllable native mechanics and observed readbacks. Reuse the
  stage5 bridge pattern, but do not replace its byte proofs. A named command
  must run the ordered journey suite directly as well as through `npm test`.
- A fixture-driven native command belongs under `scripts`, using the real
  installed development lifecycle and fixture endpoint, not the offline broker
  evaluator. It replaces repeated ad hoc setup/readback/teardown for the bounded
  journeys. Keep its native evidence and offline results explicitly separate.
- Native oracles join fixture caller id/sentinel/PID to the exact KWin lifetime
  and read actual frame, output, membership, visibility, foreground and state.
  Compare desired frames with native observations; unknown never matches.
  Setter acceptance, queue/take results, lifecycle receipts and presentation
  reads alone never establish arrival, focus settlement or storage recovery.
- Journeys: admission/reflow; selection with continued tiling; arrival before
  focus/departure; causal focus feedback; maximize/fullscreen/float/above/sticky
  and orderly sticky restart/off; current-scale geometry; reconnect/rehome;
  rapid-input FIFO burst. Scriptable native checks cover only available facts.
  Include resize-refusal diagnostics. Physical hotplug, display-scale changes,
  physical keys, effect/paint and
  crash-class recovery remain separate pending native evidence where unrun.
- Full execution of the immutable shared scenario format is a distinct
  core/model boundary. Do not silently translate its jq assertions into a
  different oracle, fabricate model outcomes, or claim the producer journeys
  execute that corpus. That executor remains explicitly outstanding in phase 1.

## Material choice

| Option | Scope and consequence |
| --- | --- |
| A, recommended | Deliver KDE producer journeys plus a fixture-driven native command with separate result categories; retain the shared core/model executor explicitly as phase-1 work. Replace the obsolete live wrapper and its advertised recipe in a separate verified cull. Smallest direct implementation of the KDE slice, but not completion of shared ordered-fixture conformance. |
| B | Include the complete immutable core/adapter/renderer/storage scenario executor and jq oracle evaluation in this slice, plus A's KDE/native boundaries. Covers both readings of the records, but adds a substantial multi-boundary harness beyond the current KDE journeys. |
| C | Deliver offline producer journeys only and retain manual native recipes. Smaller, but does not provide a durable native runner or replace the ad hoc native procedure. |

## Retirement evidence and limits

- (Superseded by the cull below.) `scripts/live-test.sh` was an inert provenance-carrier wrapper, not a native
  scenario runner. Its setup calls absent `start-test.sh provenance`; the
  [live guide](../live-kwin-testing.md#kwin-script-lifecycle) forbids using it.
  [README](../../README.md#live-test) still advertises it as the primary live
  path. Current executable caller evidence is its own hermetic test; references
  also occur in the guide, physical checklist and archived records.
- Culled (separate verified commit): `scripts/live-test.sh` and
  `scripts/live-test.test.sh`. The wrapper was an inert provenance-carrier
  wrapper, not a native scenario runner: its setup called the absent
  `start-test.sh provenance`. Caller evidence before removal (repository-wide
  `git grep` for the script and test names, excluding archived records): the
  only executable reference was its own hermetic test; non-archived references
  were the README recipe (replaced by the runner recipe), the live guide warning
  and the physical checklist warning (both reconciled), the backlog line (not
  edited) and this note. No justfile, CI workflow, package list or other script
  referenced it. Archived records keep their historical mentions. The
  provenance carrier source (`kwin/src/provenance-entry.ts`) and the
  `start-test.sh` lifecycle stay; auditing the carrier for orphaning is a
  separate unit.
- Retain stage5 production-route tests, current-production `stage5-v2` recordings,
  immutable world-wire/stage5/stage5-v4 bytes and active migration records.
  No fixture regeneration has occurred.

## Acceptance and verification after selection

- Current unit: background-spawn fixture/input servers and the production dev
  startup with actual child/receipt identities; use and exactly remove the
  appended workspace, restore the original current workspace, activate exact
  fixture lifetimes and verify cleanup including process/socket absence.
  Every lifecycle path must have hermetic coverage executing the actual helper
  source, including wrong-value and failure cases. No mutation retries.
- One native scenario only: ordinary two-fixture tiling and close/reflow, with
  exact native frame comparisons against published allocations/output geometry.
  Unknown or wrong values cannot pass. Independent review must clear before
  the single native run. No broader scenario-coverage claim follows.
- Add failing semantic/oracle regressions before implementation; distinguish
  offline, native, pending and not-run results, preserving first divergence.
- Review native ownership, fail-closed joins, bounded waits, non-repeated
  mutations and cleanup before live use. Restore only owned state.
- Run scoped Rust fmt/test/build/clippy for `tiler-core`, `tiler-protocol` and
  `omnitiler`, excluding `workspace_unsafe_posture`; adapter typecheck/test and
  unscoped build; and the runner's own command. Read raw evidence before
  accepting results. No implementation gates have run at this design stop.

## Outcome and accepted evidence

- Delivered: `scripts/kde-scenarios.mjs` (runner), `scripts/kde-scenarios-helper.js`
  (KWin helper template), 57 hermetic tests (`node --test
  scripts/kde-scenarios.test.mjs`) over a fake process boundary that runs the
  real helper source in a VM, and captured real reply shapes in
  `scripts/kde-scenarios-fixtures/{busctl-shapes,proc-stat-shapes}.json`. One
  scenario (ordinary two-fixture tiling and close-reflow on an appended
  workspace). This is the lifecycle skeleton plus one scenario, not the shared
  scenario executor, which stays phase-1 work. The runner is larger than the
  intended trim target; nothing further was removable without dropping safety
  checks or tests.
- Reviews: five independent read-only reviews. Defects found and fixed with
  regression tests: readiness source/dispatch namespace mismatch, dropped final
  unload failures, bare-versus-wrapped `getWindowInfo` values, orphan children,
  and an undrained stderr pipe on spawned children.
- Real-shape lessons, each now covered by a test that replays real shapes: fakes
  must be seeded from captured replies or source, never from the parser. Live
  runs exposed two defects the fakes hid: `/proc/PID/stat` ends in a trailing
  newline (the identity parser rejected it), and the production `rmdir` used
  `rmSync` (fails on directories) and did not tolerate the run dir the fixture
  removes itself on exit. Both fixed; the fake now renders real-shaped stat text
  and real-fs tests cover `rmdir`, exec, spawn and process identity.
- Live result (third attempt; the first two stopped at the defects above): exit 0,
  scenario PASS, 21 checks all passing, no cleanup failures. Both fixtures joined
  by class, pid and title and confirmed through `getWindowInfo`. Native frames
  equal the published allocations exactly: two disjoint 1012x1092 frames inside
  the 2048x1152 output on the appended workspace. After closing the second
  window the remaining one reflowed to a strictly larger allocation equal to its
  native frame. Cleanup verified: fixture, cargo and planner pids gone, fixture
  socket and run dir absent, original current workspace restored, appended
  workspace removed by the run, dev DOWN, no leftover processes.
- Gates: runner tests 57/57; adapter typecheck, adapter tests (1185/1185, which
  include the unscoped build) pass. No Rust changed, so Rust gates were not run.
- The opt-in input daemon (`--input`) was first exercised live by the
  state-targets work below; see that section for the cleanup result.
- Non-blocking notes (unfixed): fixture replies are not correlated by request id;
  the remove-path restore uses a different once-key than the order-20 restore;
  fixtures on different outputs abort the run (fail-closed); helper list decoding
  has no length cap; the fixture server nonce is visible on its argv (no non-argv
  route exists).

## Fractional-scale support

- The helper's integer reader accepted only values within 1e-6 of an integer, so
  logical doubles at a fractional-scale output read as unknown. It now mirrors
  the adapter's ordinary-fact rule (`world-observe.ts` `toInt`): per-component
  `Math.round` (halves toward positive infinity), non-finite or unrepresentable
  unknown, rounded width/height must be positive, bounds checked on the rounded
  value. This is the contract normalization ([scale and rounding](../porting/world-contract.md#scale-and-rounding),
  [native coordinate normalization](../porting/kde-port-notes.md#v4-native-coordinate-normalization)),
  not a tolerance: frame equality against the published integer allocation stays
  exact. Native rows also carry the unrounded frame and the output
  `devicePixelRatio`; the result records the output name, scale, rectangle and
  raw A/B frames, and a frame mismatch names the raw doubles.
- Hermetic: helper rounding on 1.25/1.5/1.0 grids, exact halves, null cases,
  bounds; runner PASS with fractional native frames equal to the allocation after
  normalization; FAIL naming the field when one rounded component differs by 1.
  Runner tests 66/66.
- Live (fractional-scale output, active output, scale 1.25, no display change):
  exit 0, PASS, 21 checks passing, cleanup verified (dev DOWN, no leftover
  processes or sockets, run dir removed, current workspace restored, appended
  workspace removed, unscoped bundle rebuilt after the scoped dev-on). Native
  frames equal the allocations exactly: tiles `[8,52,1012,1092]` and
  `[1028,52,1012,1092]` in a 2048x1152 output, then the reflowed
  `[8,52,2032,1092]`. Limit: this allocation set was integer at this scale (the
  run recorded no raw doubles; the receipt now does), so this live pass
  confirms the runner on a 1.25 output but did not itself witness a fractional
  native frame. The fractional-normalization proof is the hermetic one.

## State-targets scenario

- `--scenario state-targets` (requires `--input`; exit 2 before any io
  otherwise) drives fixture A through maximize, fullscreen, float and sticky
  with real key chords injected by `ydotool` against a runner-owned daemon
  socket, on the appended workspace beside fixture B. Chords come from the
  production shortcut registrations (`kwin/src/world-shortcuts.ts`): maximize
  Meta+M, fullscreen Meta+F11, float Meta+G, sticky Meta+Shift+G. Fixture A is
  natively activated before each chord (own once-key per activation and chord).
- Journey and exact oracles, each read from native facts (`getWindowInfo`
  maximize axes, fullscreen, keepAbove, desktops; presentation `floating`;
  frames through the fractional-aware helper) and the published allocations:
  maximize-on (both axes, A inside the output, B equals its allocation);
  fullscreen-on (A frame exactly the output rectangle); fullscreen-off (maximize
  retained, A inside the output); maximize-off (A exactly its base allocation);
  float-on (floating, keepAbove, B strictly larger and equal to its
  allocation); float-off; sticky-on (empty desktops list, which is how KWin
  reports all-desktop, never the roster; floating, keepAbove, B larger);
  sticky-off; sticky-on, one orderly production stop/start with a fresh-cursor
  readiness gate, sticky restored, sticky-off. The presentation window set must
  be exactly the two fixtures. A missing or unknown state key is PENDING, never
  PASS.
- Unfloat and sticky-off are fresh admission (no old slot;
  [decision](../decisions/window-state.md#fullscreen-and-maximize-overlays),
  REQ-FLT-01): the oracle for those steps is that A and B hold the two base
  allocations in either order, each equal to its published allocation, never the
  same one twice, with the observed order recorded in `receipts.stateSlotOrder`.
  The second live attempt (the first ran the wrong scenario) exposed the earlier wrong "A returns to its original
  slot" oracle (a runner defect, not a product defect).
- Runner defects found live and fixed offline with tests: (1) `liveMain` did not
  pass `--scenario` to the scenario function, so the first live "state-targets"
  run silently executed tiling-reflow; the result now records
  `requestedScenario`, `scenarioRan` and `scenarioMismatch`, and the runner
  exits 1 if the scenario that ran differs from the requested one; a
  `liveMain` test drives both scenarios end to end and fails without the
  pass-through. (2) the float-off slot oracle above.
- Input daemon cleanup, observed live: the real `ydotoold` does not unlink its
  socket on SIGTERM. The runner stops the exact owned pid, verifies it gone, then
  removes only that exact owned socket path and verifies absence
  (`counts.ydotooldSocketUnlinkedByRunner`). Verified after the live run: no
  daemon process, no socket left in the evidence directory.
- Reviews: one independent read-only review of the scenario (found and fixed:
  sticky is the empty desktops list rather than the roster; fullscreen-off frame
  checks; exact presentation set; both planner receipts kept; unlink counter set
  only after the verified removal) and one of the CLI pass-through fix (no other
  dropped argument; an unknown `scenarioRan` fails closed).
- Live (fractional-scale output, 1.25, no display change; third state-targets
  attempt after the two runner defects above): exit 0, PASS, 16 state checks and
  every cleanup check passing, `scenarioRan` state-targets, no cleanup failures.
  Sticky read back as an empty desktops list before and after the restart;
  after float-off, sticky-off and the final sticky-off A held B's former
  allocation and B held A's (consistent fresh admission). Cleanup verified: dev
  DOWN, no leftover fixture, planner, cargo or daemon processes, fixture
  socket and run dir absent, current workspace restored, appended workspace
  removed by the run, unscoped bundle rebuilt after the scoped dev-on.
- Gates: runner tests 91/91 (adapter and Rust unchanged, not run).
- Limits: physical keyboard input is not exercised (injected uinput chords
  through the daemon); chord keycodes are the stable Linux input-event codes.

## Resize-refusal scenario

- `--scenario resize-refusal` (requires `--input`) reproduces the injected
  shortcut evidence in [kde-resize-refusal-diagnostics](kde-resize-refusal-diagnostics.md)
  as a repeatable native check. Chords are the production grow-toward-direction
  registrations (`kwin/src/world-shortcuts.ts`): Meta+Alt+Left and Meta+Alt+Right.
  The target fixture is natively activated before each chord (own once-keys).
- Journey and oracles, on the appended workspace with fixture A left and B right:
  (1) B grows left: A narrower by exactly 12 and B wider by exactly 12 with its
  left edge 12 earlier (12 is the first-press keyboard step, `keyboard_step_px(0)`),
  each native frame equal to its published allocation, disjoint, inside the
  output, and no refusal line; (2) B grows right (outer edge): exactly one new
  Planner `omnitiler:resize-refused` line with `internal=unchanged
  wire=operation-refused dir=right mode=outwards press=0 rowsTotal=2`, subject
  bound to fixture B, frames exactly the step-1 frames; (3) fixture B is closed
  (same close/reflow oracle as tiling-reflow, shared helper), then A grows left
  as a singleton: exactly one line `internal=unchanged wire=operation-refused
  dir=left mode=outwards press=0 rowsTotal=1`, subject bound to fixture A, A
  frame exactly the reflowed frame.
- Planner log: the runner resolves the Planner's stderr file through
  `/proc/<planner pid>/fd/2` (regular file under the temporary directory, name
  pattern checked), snapshots its size before each chord and reads only the
  bytes after it, parsing bounded tokens. An unreadable or unresolvable log is a
  FAIL, never a skip. Receipts keep only token extracts and
  `planner-log-resolved`; the subject id is compared in memory and recorded only
  as `subjectBound`.
- Review: one independent read-only review. Findings fixed: the refusal line was
  not bound to the actuated window (subject, `press`, `rowsTotal` and
  `rowsShown` are now asserted, with fakes using real subject ids); an
  unchecked cross-API frame source difference is fail-closed flake risk only.
- Live (fractional-scale output, no display change): exit 0, PASS, 25 checks
  passing, `scenarioRan` resize-refusal, no cleanup failures. Step 1 frames
  `[8,52,1000,1092]` and `[1016,52,1024,1092]` (from `[8,52,1012,1092]` and
  `[1028,52,1012,1092]`), step 2 and step 3 each produced exactly one
  `unchanged`/`operation-refused` line with the expected tokens. No resize
  defect found; these are the expected no-op outcomes.
  Cleanup verified (dev DOWN, no leftover processes or sockets, current workspace
  restored, appended workspace removed, unscoped bundle rebuilt after the scoped
  dev-on).
- Gates: runner tests 102/102 (adapter and Rust unchanged).
- Limits: nested-group, minimum/maximum-exhaustion and after-populated-workspace
  refusals are not exercised; injected (not physical) keys. The Planner's
  temporary dev log written by `just dev-on` is not removed by this runner.

## Follow-up units

- Genuine-producer offline ordered journeys through the real adapter/broker/core.
- Further fixture-driven native scenarios after the lifecycle and one-scenario
  tool are proved: selection with continued tiling; state targets including
  sticky restart/off (state targets and resize-refusal delivered above); scaled-metric frames at current scales; rapid-input burst. Retain physical-key/hotplug/scale-change/paint
  exclusions.
- Replace the advertised live-test recipe and cull the obsolete wrapper/tests
  in a separate verified commit once the replacement covers their purpose.
- Full shared scenario/model/jq execution remains phase-1 work, not this tool.
