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
  `start-test.sh` lifecycle stayed in that cull; both were audited in the next
  section.

## Orphan audit: provenance carrier and start-test lifecycle

- `scripts/start-test.sh` is retained: it is not an orphan. Executable callers:
  `just/linux.just` (dev-on start, dev-off/recovery stop, rollback stops),
  `scripts/dev-loop-split.test.sh` (fake substitute and call assertions),
  `kwin/tests/scope-class.test.ts` and `kwin/tests/trace.test.ts` (read its
  source), and the scenario runner through `just dev-on`. Documented live
  procedures: README, `docs/dev-loop.md`, `docs/live-kwin-testing.md`.
- `kwin/src/provenance-entry.ts` is culled (separate verified unit). Caller
  evidence (repository-wide `git grep` for `provenance-entry`,
  `provenance-ready` and the `PROVENANCE_*` define names, excluding archived
  records): no executable reference. `kwin/package.json` builds only
  `src/entry.ts`; no justfile recipe, packaging list, test or script names it;
  `start-test.sh` has no `provenance` subcommand (its only "Provenance" mention
  is an unrelated comment on controller action defaults); the only consumer was
  the already-culled `live-test.sh`. Non-archived mentions were prose only
  (`docs/live-kwin-testing.md`, `docs/porting/kde-port-notes.md`, reconciled;
  `custom-tile-runtime.md` records the earlier harness history and is left
  unchanged). Its sole purpose (a single `omnitiler:provenance-ready` line
  identifying a loaded script instance) is covered by the exact-receipt
  controller lifecycle, the helper accounting in the live guide and the
  evaluated-readiness gate that the scenario runner enforces.
- The carrier was only typechecked (`tsconfig.json` includes `src`); it was never
  in the shipped bundle graph, which starts at `src/entry.ts`.
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

## Rapid-burst scenario

- `--scenario rapid-burst` (requires `--input`) checks the tiled directional FIFO
  ([rapid input outcome](archive/kde-rapid-input-geometry.md)) natively: one
  `ydotool key` call injects 60 focus/move chords back to back (focus
  Meta+Left/Right, move Meta+Shift+Left/Right, from the production
  registrations) against two fixtures on the appended workspace. The sequence is
  derived, not hand-written: a fixed-constant generator driven by a model of a
  two-tile row from the observed admission order (the fixture with the smaller
  x is left and is natively activated first); the direction of every chord is
  forced toward the in-row neighbour so none is an edge no-op and none can cross
  to another output; focus flips the focused slot, move swaps the slots and the
  moved fixture keeps focus. The golden label sequence is pinned in a test.
- Oracles, all exact: the KWin journal since a pre-injection cursor (owner pid
  filtered) has exactly 60 `omnitiler:world:command` lines whose `label=` values
  equal the generated sequence in order and no failure or loss line of any kind;
  the settled state (presentation set exactly the two fixtures, each native frame
  equal to its published allocation, disjoint, inside the output, slot occupants
  and the natively active window equal to the model's final state) must hold on
  at least three consecutive polls spanning a minimum settle window with
  unchanged presentation revision and journal, so intermediate states that
  happen to equal the final one cannot pass. Terminal journal faults (extra
  lines, any failure line, a label mismatch at 60) fail immediately. Unknown
  facts are PENDING.
- "All READY" is observed as: every press accepted (exact count and order) and
  zero failure/loss outcomes. KWin logs no per-command ready line, so the
  absence of every failure token plus the exact final native state is the
  observable.
- Review: one independent read-only review. Findings fixed: the first-match
  pass could precede draining (quiescence streak added, with a lagging fake and a
  late-drift negative), golden vector, observed values in checks, additional
  failure tokens, fail-fast on terminal journal states. The model matches the
  product source (two-leaf row swap and neighbour focus, mover keeps focus;
  no crossing while both fixtures remain in the row).
- Live (fractional-scale output, no display change): exit 0, PASS, 25 checks,
  `scenarioRan` rapid-burst, no cleanup failures. 60 accepted presses (27 focus,
  33 move) with labels in the generated order, zero failure lines; final native
  frames equal the published allocations with the fixtures in swapped slots as
  the model predicted and the predicted fixture active. Cleanup verified (dev
  DOWN, no leftover processes or sockets, current workspace restored, appended
  workspace removed, unscoped bundle rebuilt after the scoped dev-on).
- Gates: runner tests 112/112 at that unit (adapter and Rust unchanged).
- Limits: injected (not physical) keys; one 60-press burst at the default key
  delay; two-tile, single-output row only (no crossing, nested groups or floats);
  press timing relative to the Planner round trip is not controlled.

## Selection scenario

- `--scenario selection` (requires `--input`) checks workspace selection while
  ordinary tiling continues ([KDE selection](archive/v4-phase2-kde-selection.md),
  reference-outcomes row "KDE selection before focus feedback"). Two fixtures
  are admitted on the appended workspace X; the user's original workspace O is
  managed-empty under the fixture-only scope class. Meta+O is injected (ydotool),
  then Meta+X, using the numbered `omnitiler-workspace-N` registrations
  (`workspaceShortcutCatalog`); then fixture B is closed and the shared reflow
  oracle runs on X.
- Ordinal gate before any mutation: O is the pre-append roster index of the
  original workspace plus one, X the roster length plus one; both must be 1..9
  (the numbered chords stop at 9), else the run refuses with no mutation.
- Read-only holder preflight before any injection: KGlobalAccel
  `allComponents` plus per-component `allShortcutInfos` must show exactly one
  record for each action, component `kwin`, active key = Meta+N, and no other
  action holding the key (reverse scan). Wrong holder or collision is FAIL;
  unreadable or malformed is PENDING. Offline shapes come from sanitised real
  read-only captures (`scripts/kde-scenarios-fixtures/kglobalaccel-shapes.json`).
- Pointer gate before chord 1, and unchanged again before chord 2: numbered
  selection is cursor-first on a multi-output session with per-output desktops,
  so `workspace.cursorPos` (helper `read` now emits `cursor`) must lie inside the
  fixtures' output rect (inclusive left/top, exclusive right/bottom). Outside is
  FAIL, unknown is PENDING. The runner never moves or clicks the pointer.
- Oracles per leg, all exact: native current desktop equals the target; exactly
  one `omnitiler:world:command` journal line for that chord's label since a
  per-leg cursor and no failure/loss line; the fixtures' desktops remain exactly
  [X]; native frames equal the admission allocations; the presentation set is
  exactly the two fixtures with unchanged allocations, disjoint and inside the
  output. Chord 2 is injected only after leg 1 passes. A whole-journey journal
  check requires zero failure lines. The post-return active window is a receipt
  only, not an oracle.
- Review: two independent read-only reviews. First: NOT CLEARED (no reverse
  key-collision check); fixed with negative tests for O and X collisions and
  positive unrelated rows; re-review CLEARED. Declined: an empty user-owned O
  could in principle be retired by core trailing-workspace planning mid-run; this
  predates the selection scenario in all delivered scenarios, and the
  `current-restored` cleanup verification fails the run if the original
  workspace is gone. Second review of the later pointer gate and helper cursor:
  CLEARED. Follow-up from it: `pointInside` edge test added; helper comment
  corrected.
- Offline: runner tests 130/130 (hermetic, executing the real helper source in a
  VM; wrong-value and failure cases for each oracle class, chord-2 failure with
  cleanup verification, pointer outside/unknown/drift, holder faults).
  Failing-first evidence per class was reported by the implementing Worker, not
  re-run by the Lead. Adapter and Rust unchanged, not run.
- Live (multi-output session, fractional-scale fixtures output, pointer on that
  output, no display change; one run, no retry): exit 0, PASS, 29 checks, no
  failed check, `scenarioRan` selection, no cleanup failures. Ordinals O=1 and
  X=5. Holder checks passed on the live stack; each chord produced exactly one
  command line with the expected label and current desktop equal to the target
  with frames equal to the admission allocations; the pointer was unchanged
  between chords; close-B reflow on X passed; journal-clean zero failure lines.
  Cleanup verified directly (dev DOWN, no ydotoold, no leftover
  processes/sockets, current workspace restored, appended workspace removed,
  unscoped bundle rebuilt after the scoped dev-on).
- Limits: injected (not physical) keys; one populated appended workspace plus the
  managed-empty original only (no populated-to-populated switching); no
  focus-bearing terminal or switch-settled claim (deferred slices); the pointer
  must be on the fixtures' output at run time; native readback of hidden-workspace
  frames is observed live only in this one run.

## Offline genuine-producer journeys

- Delivered in `kwin/tests`: `world-producer-journeys.test.ts` (admission, close
  and reflow, float on/off, unfloat after sibling close, singleton move refusal,
  maximize on/off, fullscreen on/off, stacked maximize-fullscreen-exit,
  populated and empty workspace selection with continued tiling, rapid selection
  return supersession, sticky on/off, orderly sticky stop/restart then off,
  fractional-scale admission and reflow) and `world-producer-disconnect.test.ts`
  (output disconnect and renamed-handle return). The shared harness
  `world-producer-harness.ts` is the fifo test's harness extracted mechanically
  (the fifo tests are unchanged) plus additive, default-off fidelity options
  (desktops, fractional scale, removable outputs, preimage and intent lanes,
  restart). Every journey runs the real `startWorldAdapterEntry` against the
  retained `broker_world_eval` and real core; only native surfaces and lanes the
  retained broker does not serve (native preimage, opaque intent) are faked with
  exact-contract replies. Expectations are derived from decisions and core source,
  not first-run output; each case was shown failing first. Independent reviews
  cleared tier 1 and tiers 2-3 with non-blocking findings, fixed.
- Not the shared scenario format, whose jq oracles are not executed here.
- Limits: fake native surfaces are single-process and quiescent after actuation
  (post-actuation state targets read Hold/unreadable; settled targets are
  asserted on the drain capture); the sticky restart journey resets the core on
  handshake like production, so only native facts and the faked lanes carry
  over; the fractional journeys prove normalisation on the seeded and admitted
  readbacks (actuated echoes are integers); wire scale stays 1 and the adapter
  never reads the output scale; the disconnect fake does not re-home desktops
  when the survivor's current differs from its first desktop; fresh-admission
  placement is pixel-coincident with the old slot on two- and three-window
  layouts, so the unfloat/sticky-off oracles use a sibling-close variant or
  set equality; resize refusal offline covers only the singleton move refusal
  and the boundary-noop tail, not minimum/maximum exhaustion or nested groups.
- Disconnect focus (user decision 2026-11-09): windows rehomed off a
  disconnected output are not fresh admissions. The real core minted one
  automatic-admission focus effect on the last rehomed member (native activation
  of D) because the survivor unit was a previously reported empty controlled
  unit (`take_admission_focus` per-domain diff plus `prior_empty_controlled`);
  fixed in core (see [physical reconnect](v4-phase2-kde-physical-reconnect.md#disconnect-rehomed-members-are-not-admissions)),
  and the journey now asserts no effect on any post-disconnect capture and no
  native activation. No `op-dpv-*` view is minted for this non-fresh-unit
  topology, accepted; the reference-outcomes row is a distinct selected case.
- Arrival before focus/departure: only determinate outcomes are covered (rapid
  select/return supersession: no view write, no effect, no terminal). The
  superseded-arrival remembered-focus half is unobservable on this harness and
  already selected in the Windows return-focus row.
- Gates: `npm --prefix kwin run typecheck`; `npm --prefix kwin test` 1205/1205.
  Script and fixture changes: none. The disconnect focus fix is the one core
  change (physical reconnect note).

## Follow-up units

- Native scenarios: none open. State targets (incl. sticky restart and off),
  selection, rapid-burst, resize-refusal and fractional-scale runs are delivered;
  the 1.25 live runs did not witness a fractional native frame (the hermetic
  rounding proof covers it). Retain physical-key, hotplug, display-scale-change
  and paint exclusions.
- Offline genuine-producer journeys: delivered above. Remaining: three-output
  disconnect (`op-dpv-*` path), resize minimum/maximum/nested refusals, 3+
  window fresh-admission placement oracles.
- Full shared scenario/model/jq execution remains phase-1 work, not this tool.
