# Group Underlay Move Trigger

- Status: A/B delivered; adapter-wiring citations below refer to the removed
  `kwin/src/plan-adapter-entry.ts` module. The trigger policy persists in
  shared core and the effect FFI consumed via the world route; see the [core
  closeout](archive/core-world-model-rearchitecture.md#closing-outcome-2026-10-24)
  and [world adapter guide](../porting/world-adapter-guide.md).

## Goal And User Decisions

- User decision 2026-09-30: show the group underlay for window movement through
  Meta+Shift hold, Meta+left-drag, and plain title-bar drag of a tiled window.
- Deliver A, then B; C remains parked (2026-10-05). Both modifiers must be held; extra modifiers are
  allowed. The trigger is hard-coded, with no setting. Interactive resize is
  not a trigger; Meta+Shift can independently show the underlay during resize.
- Before and during every stage, reassess simplicity and stop/report if the
  solution grows large or complex. The user is ready to change course.
- Assignment 2026-10-05: implement KDE A then B through the existing shared
  visual policy/effect FFI; C remains parked. No new route, core/protocol
  behavior, settings/default, minimum/admission change or live test.
- Status 2026-10-05: A/B delivered and verified offline; no live verification.
  Stage C remains parked. Preceding uncommitted KDE work and the user's
  devenv/backlog changes were preserved.

## Pre-A/B Boundary (source baseline `ad6d69c`)

- Replaces the approved Meta-only lifetime in `docs/decisions.md`:
  passive `EffectsHandler::mouseChanged` observation, unknown modifier state
  before the first signal, and Rust-owned group visibility policy.
- Qt decodes Meta at `kwin/native-effect/activewindowborder.cpp:1181-1192` and
  passes one boolean through `group_highlight_ffi.h:68` and
  `crates/tiler-kwin-effect-ffi/src/group_highlight.rs:512-521,663-681`.
  Portable visibility lives in `crates/tiler-core/src/visual.rs:112-114`.
- Group state is delivered independently of modifier hold. Focus changes clear
  it before asynchronous refresh (`activewindowborder.cpp:290-303`;
  `kwin/src/active-group-highlight.ts:875-917`).

## Stages

### A: Meta+Shift

- Replace the Meta predicate with both modifier bits held. Preserve existing
  group, focus, endpoint, anchor and first-public-signal gates. Plain Meta+drag
  temporarily loses the underlay until B.
- Touch points: `kwin/native-effect/activewindowborder.cpp:1190,1207-1208`,
  `activewindowborder.h:170`, `group_highlight_ffi.h:68`, and Rust FFI
  `group_highlight.rs:512-521,663-681`. Rename the combined flag/parameters;
  the core policy body and POD layouts need no change.
- Keep diagnostics truthful: update `meta=` naming in the setter, transition,
  status and fallback (`activewindowborder.cpp:939,972,1152,84`), plus static
  pins at `kwin/tests/active-group-native-static.test.ts:99-118`.

### B: Focused-Window Interactive Move

- Reuse native Started/Finished subscriptions
  (`kwin/native-effect/activewindowborder.cpp:613-624`). Classify move at Started
  via the underlying Window's `isInteractiveMove()`, track the exact window,
  and clear on Finish/cancel or removal (`:627-638,227-254`).
- Show only when the dragged window is the actual active window and matches
  the accepted group payload subject. Rust already stores that subject in
  `group_highlight.rs:117-118,475-488`; expose a small comparison boundary if
  needed, rather than deriving group identity in C++.
- C++ supplies observed chord and matching-move flags. Rust combines them:
  `(first_signal_seen && chord_held) || matching_move_active`, followed by
  existing eligibility/endpoint gates. Add a scalar move argument at
  `group_highlight_ffi.h:68`, Rust FFI `:512-521,663-681`, and the C++ caller
  `:1207-1208`; put trigger policy/coverage in `tiler-core/src/visual.rs`.
  Move-start evidence does not depend on the first modifier signal.
- Limitation: non-activating drags of unfocused windows show no drag-triggered
  underlay. A global move flag can show the wrong focused group; recursive
  membership is not proof of the same immediate parent
  (`crates/tiler-core/src/active_group.rs:42-64,116-124`).
- No new press hook or oracle pull: the existing spy captures resize presses
  (`activewindowborder.cpp:666-683`), and the oracle verdict arrives at Finish
  (`:753-765`; `kwin/src/drag-oracle-pull.ts:81-89`). Add bounded trigger
  diagnostics, not per-frame logs.

### C: Dragged-Subject Resolution

- Cover tiled moves that do not activate the dragged window. Extend the
  existing group query/bridge with an explicit dragged subject, separate from
  actual focus; resolve its immediate parent from retained topology without
  changing native or retained focus. Validate payload acceptance and eligibility
  against that live drag subject, with reply invalidation at Finish/removal.
- Touch points: `kwin/src/active-group-highlight.ts:875-917`, existing adapter
  Started/Finished integration in `kwin/src/plan-adapter-entry.ts:5471-5472`,
  protocol command mapping, `crates/tiler-core/src/engine.rs:1479-1515`,
  `boundary.rs:816-900`, `active_group.rs:106-160`, and native/Rust FFI
  subject acceptance (`group_highlight.rs:461-488`). Contract details are
  deliberately unresolved until the C simplicity checkpoint.
- Do not substitute the dragged ID into `focused_window`: the current query
  synchronizes and persists retained focus (`engine.rs:1499-1513`), and the
  resolver validates that focus (`boundary.rs:853-883`).

## Lifetime And Rendering

| Event | Underlay lifetime |
|---|---|
| Second of Meta/Shift pressed | Show valid focused group after modifier observation; either press order works. |
| Either modifier released | Clear chord trigger; a valid active move can keep it visible. |
| Interactive move Started | Show valid dragged-subject group when available: focused-only in B, subject-aware in C. |
| Move continues outside source footprint | Keep source group projected footprint while its state remains valid. |
| Finish/cancel/removal | Clear move trigger immediately, without waiting for oracle/drop settlement; held chord remains independent. |
| Resize, floating window, or root leaf | No resize trigger; no dragged-group underlay for floating/root-leaf moves. |
| Subject/domain/state becomes invalid | Hide unavailable group; later valid state may restore it while a trigger remains active. |

- Use engine-projected source-group union, not live dragged-window bounds.
  Ordinary reconcile is held during tiled moves
  (`kwin/src/plan-adapter-entry.ts:4558-4588`). Existing anchor remapping keeps
  the stored scene footprint below the lowest renderable member even when that
  member moves or stacking changes (`activewindowborder.cpp:395-470,279-285`).
- Drop preview remains independent above windows (z=10), while the source
  underlay stays below members (z=-2; `activewindowborder.cpp:312-329`). No
  destination-group highlighting or renderer redesign is proposed.

## Simplicity Checkpoint

- Apply `docs/principles.md:19-32` before each stage and whenever its approach
  expands: smallest correct scoped solution, not arbitrary size/time limits.
- Especially for C, stop before adopting a broad subject-query framework,
  duplicated group state, new transport, focus side effects, or additional
  lifecycle machinery that makes this visual feature disproportionately complex.
  Report the concrete growth, simpler scoped option and tradeoff; await user
  direction rather than silently expanding or abandoning the remaining stage.

## Verification And Completion

| Stage | Offline coverage | User-owned live rows |
|---|---|---|
| A | Modifier combinations/extras in native logic harness; static observation/diagnostic pins; existing FFI eligibility gates. | Both press orders, either release, extras, unknown initial hold; HJKL/arrows/digit-send coexistence; Meta alone hidden. |
| B | Rust OR/readiness matrix; accepted-subject mismatch; start/finish/removal cleanup; floating/root-leaf exclusion; FFI and native static pins. | Focused Meta+drag and title-bar drag; stationary start, cancel/drop, chord changes mid-drag; resize excluded; inactive non-activating drag hidden; source footprint/raising/preview coexistence. |
| C | Subject distinct from focus without focus mutation; nested immediate-parent selection; domain/eligibility guards; stale replies after finish/removal; return to chord's focused group. | Inactive drags in same/different/nested groups; unchanged actual focus; cross-domain movement; finish/cancel/removal and delayed refresh; preview coexistence. |

- Run affected offline suites and relevant build/type checks after each stage.
  Live runs belong to the user under `docs/live-kwin-testing.md`; this record
  authorizes no agent live mutation.
- Evidence is source-backed, not live proof: inspected KWin checkout
  `upstream KWin source checkout` is 6.7.3; project host/build references 6.7.5. Confirm
  host signal timing, focus behavior and pixels during user verification.
- At completion, promote the delivered lifetime, subject, readiness and resize
  rules to `docs/decisions.md`; record any user-approved scope change,
  accepted evidence and outcome here, then archive.
  Next action: user A/B acceptance; C requires reassessment.

## A/B Outcome And Evidence (2026-10-05)

- A delivered before B: native Meta/Shift level bits use the existing shared
  chord predicate through `visual_group_underlay_chord_held`; extra modifiers
  are ignored and either press order works. Unknown-before-first-signal applies
  only to the chord. Native diagnostics now say `chord=` and `move=`.
- B reuses existing Started/Finished/removal subscriptions and one
  `QPointer<EffectWindow>` move subject. `window()->isInteractiveMove()`
  distinguishes move from resize. Capture does not wait for an asynchronous
  group payload; display waits for actual active focus AND the accepted Rust
  subject. Finish/cancel clears before oracle geometry/verdict reads, and
  exact removal clears the arm. The held chord is independent.
- Shared policy path: `group_highlight::should_show` combines
  `(first_signal_seen && chord_held) || matching_move_active` through the existing
  core trigger, followed by the existing group/focus/endpoint/anchor gates.
  No core/protocol behavior, settings/default, rendering geometry/z-order,
  minimum/admission or Windows source change. C does not resolve a subject
  distinct from focus; no focus/query substitution was added.
- FFI: new chord export and accepted-subject move-match export; existing
  `group_highlight_is_visible` gains a sixth scalar argument. This is a paired
  C signature change, not binary-backward-compatible; native/Rust callers were
  rebuilt together. Existing POD state/status layouts are unchanged. No new
  JS-to-Rust/D-Bus route. A test-only trigger export/wrapper was removed after
  review because the production group FFI already calls the core trigger.
- Review corrected a start-only matching bug before acceptance: a latched
  old move could otherwise survive focus-clear followed by a different accepted
  group. `groupMoveMatchesNow()` now revalidates live focus/subject at every
  refresh and diagnostic; rotation/clear/reaccept regression coverage prevents
  an unrelated group riding the old drag. Capturing before payload acceptance
  also permits recovery once the correct group becomes available mid-hold.
- Locations: `kwin/native-effect/activewindowborder.cpp:716-764,809-830,1250-1294`;
  `crates/tiler-kwin-effect-ffi/src/group_highlight.rs:512-529,646-733,1157-1220`;
  `crates/tiler-kwin-effect-ffi/src/visual.rs:159-171`;
  `kwin/native-effect/group_highlight_ffi.h:61-79`.

### Final Offline Gates

| Gate | Outcome |
|---|---|
| `cargo fmt --all -- --check` | Pass |
| `cargo clippy -p tiler-core -p tiler-kwin-effect-ffi --all-targets -- -D warnings` | Pass |
| `cargo test -p tiler-core -p tiler-kwin-effect-ffi` | Pass, including existing core chord/OR tests, FFI eligibility/readiness/match rotation and POD layout |
| `just build-native-effect` | Pass: host-matched KWin 6.7.5 derivation, all three plugins staged |
| Existing native C++ logic/group tests | Same host SDK, rebuilt `target/kwin-native-k1-test-build`; CTest `^native-effect-(logic|group-highlight)$`: 2/2 pass |
| `npm --prefix kwin test` (includes script build) | 844/844 pass, 119 suites |
| `npm run typecheck --prefix kwin` | Pass |
| `git diff --check` | Pass |

- Native provenance: resolved host-matched KWin 6.7.5 derivation (`kwin-6.7.5.drv`), exact dev output
  (`kwin-6.7.5-dev`). Normal builder
  uses `BUILD_TESTING=OFF`; the existing testing-ON cache was checked against
  that same `KWin_DIR`, rebuilt inside its host derivation SDK with current
  project Cargo/rustc, then selected pure CTests ran. No new harness, system
  install, effect loading, live D-Bus, input or window mutation.
- Independent review found no remaining display/ABI contract blocker. Its
  concrete simplicity finding was resolved by deleting the unused trigger
  export. Historical panic fallback can leave subject bytes with `has_group=0`;
  display still fails closed, so no broader panic-state refactor was introduced.
- Three sequential sessions: reference research, A/B
  implementation (with fresh-match repair and mechanical trim), and
  independent review. No commits or pushes.

### User-Owned Acceptance

- Test system: both Meta/Shift orders, either release and extra modifiers; Meta alone
  hidden; held chord can show during resize; no assumption about initial hold
  before first passive signal.
- Test system: focused title-bar and native Meta+drag moves, stationary start, drop,
  Esc/cancel and subject close. Check arm cleanup, chord handoff/independence,
  projected source footprint, raising and simultaneous drop preview.
- Test system: delayed group acceptance and focus change during a move must never
  show the unrelated new focused group. Resize alone, floating/root-leaf,
  workspace transitions, applet focus, maximize and fullscreen remain suppressed.
  Confirm current loaded build, actual pixels and correlated `chord/move/vis`
  diagnostics; an accepted setter alone is not visual proof.
- Multi-output test system: repeat on each scale/origin, cross-output dragging and
  workspace/anchor transitions, including negative origins where configured.
- Follow `docs/live-kwin-testing.md`; all live cases are user-owned. C remains
  parked. Next action: user single-output test-system acceptance, then multi-output checks; await a separate
  scope decision before any unfocused-subject work.
