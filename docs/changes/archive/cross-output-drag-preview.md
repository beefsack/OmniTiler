# Cross-output drop and live drop preview

## Goal and scope

Ship two user-decided P1 items together offline: a tiled move between outputs inserts at the pointer in the destination domain via the existing core drop policy, removing source membership; a separate native overlay displays the projected target slot above windows during tiled move drags, hidden for snap-back/center and cleared on finish, cancellation or refusal. No live KWin testing, sibling reflow, new placement policy or Meta group-outline lifetime change.

## Approach and acceptance

- Carry the existing exact sticky group-edge hover prior across preview samples and the final drop, with source/revision validation. Preview projects a fresh complete observation including size hints through the same drop resolver, without committing topology. Destination follows pointer/output; avoid a source-scoped restore marker fighting successful destination placement.
- Script samples move steps, backs off when Planner's existing single flight is occupied, fences late replies and clears overlay on finish/loss without delaying the final drop. Native effect owns an independent translucent filled overlay above windows.
- Correlate normal-level preview shown/cleared/backed-off and cross-output drop applied/refused diagnostics and explain tokens in `docs/dev-loop.md`; lean behavior tests and removal of obsolete tests.
- Offline verification: KWin tests/typecheck, Rust workspace tests/fmt/strict clippy, host-matched native build/CTest, `git diff --check`; report counts and production/test line totals. Update decisions and backlog as shipped offline pending live check.

## Bounded units

1. Core/protocol read-only preview and prior carry.
2. Planner destination-scoped cross-output placement and script dispatch/recovery.
3. Native-effect overlay and script preview routing.
4. Integration, behavior tests, documentation and offline verification.

## Offline outcome

Repository initially clean at `da64ac1`; no live KWin testing. Destination is the output under the pointer at Finish, whether or not KWin has reassigned the mover. The KWin script uses one pointer-domain observation builder for preview and drop, and the Rust Engine shares one drag resolver; protocol shares one `DragPayload`/evaluator. Core preview uses fresh size hints and carries exact revision/source-bound sticky hover prior into the drop. Entry coalesces one outstanding preview sample and clears the independent above-window native ImageItem at Finish, refusal, cancellation or loss. Default fill is #2A82DA alpha 64. Cross-output drop uses the destination resolver, forces complete source reconcile and has destination-only failure markers; same-output singleton still snaps back. If native output lags the pointer, the adapter sends the mover to the destination, writes the target desktop, verifies output/desktop and a fresh complete destination observation before ordinary planned geometry. No new arrival timer/retry. Native visual delivery and actual host behavior remain user-owned live checks.

**KWin 6.7.5 source finding** (upstream KDE KWin 6.7.5 release archive, kwin-6.7.5.tar.xz): `src/window.cpp:1074-1115` finishes interactive output change before `interactiveMoveResizeFinished`; output tracks geometry center, not the pointer (`src/window.cpp:3358-3361`, `src/xdgshellwindow.cpp:262-287`). `frameGeometry` writes via `moveResize` (`src/window.h:479`), which may change output but does not call the desktop/restore fixup in `Window::sendToOutput` (`src/window.cpp:3879-3922,4670-4695`). Public `workspace.sendClientToScreen` calls it (`src/scripting/workspace_wrapper.cpp:468-470`). Geometry alone is insufficient for reliable destination membership, hence the existing native transfer capability is used.

**Evidence:** After the simplification edits, KWin `npm test` 788/788 and typecheck; Rust workspace 628 tests, fmt and strict workspace clippy; `git diff --check` pass. The prior `just build-native-effect`, separate host-matched `BUILD_TESTING=ON` build and CTest 29/29 remain current (native files unchanged since). Independent review after transfer/simplification found no blocking issues. A retained-empty parity regression was fixed by converging the preview clone before checking emptiness; the first test fixture accidentally used `store_committed`, which retires empty sessions, then was repaired using `insert_raw`. No live result claimed.

**Size:** initial before simplification: production net +2,484, tests net +2,384. This pass removed 92 production lines relative to the exact +1,650 pre-pass count (previously rounded to ~+1,651): adapter preview/pointer builder -20, entry preview lifecycle -17, protocol drag evaluator -17, Engine drag working -29, Session drag comments -9. Final production net **+1,558**, test net **+2,002** (two untracked suites included, 512+396 lines); excludes docs. Main-file net lines against `da64ac1` (production before -> after; tests before -> after):

| File | Production | Tests |
| --- | ---: | ---: |
| `kwin/src/plan-adapter.ts` | +753 -> +733 | 0 -> 0 |
| `kwin/src/plan-adapter-entry.ts` | +207 -> +190 | 0 -> 0 |
| `crates/tiler-protocol/src/planner_protocol.rs` | +266 -> +249 | +424 -> +424 |
| `crates/tiler-core/src/engine.rs` | +220 -> +191 | +350 -> +350 |
| `crates/tiler-core/src/session.rs` | +35 -> +35 | +144 -> +144 |
| `crates/tiler-core/src/session/ops/drag.rs` | +34 -> +25 | 0 -> 0 |
| `crates/tiler-core/src/boundary.rs` | +32 -> +32 | +10 -> +10 |
| Native effect (`kwin/native-effect/`) | +103 -> +103 | +25 -> +25 |
| KWin test files (including the two untracked suites) | 0 -> 0 | +1,049 -> +1,049 |

**Remaining size versus the ~600-900 production target:** no further cut was found sound without changing the selected behavior. The +733 adapter is approximately 110 lines for pointer-domain selection/projection, 215 for read-only request, prior and stale-reply lifecycle, 240 for native lag transfer with output/desktop and fresh complete destination proof, and 168 for drop dispatch, correlation, terminal markers and source reconcile. The +190 entry owns stepped pointer coalescing, native overlay routing, start/finish lifetime and cancellation. The +249 protocol encodes and validates one preview/drop wire body, serializes the distinct read-only reply and shares its evaluation. The +191 Engine builds retained/fresh working sessions, shares preview/drop policy and commits only drops, including empty-target singleton projection. Session/boundary/drag ops (+92 combined) carry exact sticky hover prior and expose read-only preview with hints. Native effect (+103) provides the independent above-window ImageItem and minimal validation. Lag-transfer output/desktop readback and fresh complete destination observation are distinct proofs; removing either would undermine the KWin 6.7.5 finding. The two preview test suites are already case-matrix consolidated; remaining cases cover different failure modes. All cuts were net deletion without new machinery. No behavior deviation selected.

**Next action:** user-owned live check after host-matched native effect delivery: multi-output test system - occupied and empty destination drops at the pointer (including native-output lag), source membership removal, preview/drop agreement at group edges with sticky carry, center snap-back/hidden preview, wrong-output/refusal recovery; single-output test system - same-output edge/center, preview overlay above windows independent of Meta outline, and Finish/Esc/cancel/no-Finished clearing. Use correlated preview/drop diagnostics without treating setter requests as rendered proof. No agent live KWin/Plasma test occurred.

## Backlog detail moved 2026-11-01

### Original backlog lines 2043-2046

- P2 | Simplify drag preview + cross-output drop code | Post-live cleanup only:
  cut advisory `hover_prior` validation, lag-only fences, verbose comments;
  net deletion, no behavior change.
  [change](cross-output-drag-preview.md)
