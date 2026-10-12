# D16 floating transfer (REQ-WS-05 / R-WS-05)

Goal: sending an ordinary intentional floating window to another workspace
(follow or stay, numbered or relative send) keeps it floating on arrival
with its size; destination tiles untouched; source tiles untouched.

Scope:
- In: ordinary intentional floats; shared core first (serves Windows);
  KDE send adapter (numbered + relative, follow + stay); red/green
  regressions incl. WS1 -> WS2 -> WS1 roundtrip; spec/matrix Ours cells,
  backlog D16 row, Windows handoff item 22 entry.
- Out: sticky floats (stay on all workspaces, current behaviour unless spec
  says otherwise); floating-workspace boundary path keeps working;
  Windows real wiring (compile-only here); no setting (user 2026-10-12 D16 a).
- Position on arrival: keep frame as-is on same output (tentative).

Approach: reuse/extend the D28 slotless `MoveDeferred` path (4c1c3a4 +
77cac6a) for ordinary floats where it fits rather than a parallel path.
Intentional-float memory (restart store) must follow the window.

Acceptance:
- Follow/stay x numbered/relative sends retain float + size, no tile reflow
  either side, source had no slot.
- Existing floating-workspace boundary journey unaffected.
- Gates: cargo test --workspace, clippy -D warnings, fmt --check,
  KWin JS tests + typecheck + bundle build; CTest only if native changed.
- One independent read-only review; live checks user-owned, listed pending.

Plan:
1. Shared core float transfer + regressions (incl. roundtrip).
2. KDE adapter float movers + production-path regressions.
3. Windows compile-only + handoff item 22 entry.
4. Records (spec/matrix, backlog row via Orchestrator line, this note),
   review, full gates, handover.

Tentative decisions:
- Same-output frame kept as-is (no rewrite on arrival).
- Sticky movers keep current refusal (on all workspaces already).
- Reuse MoveDeferred-style slotless transfer for ordinary floats.

Evidence:
- Shared core (`-p tiler-core`, worker): 567 passed, 0 failed. New
  `floating_deferred_send.rs` (8 tests: follow, stay, roundtrip, sticky/
  maximized/automatic refusals, tiled sanity, cross-output follow) plus 2
  updated old-refusal pins. Protocol untouched.
- KDE (`npm test --prefix kwin`, worker): 1396 pass, 0 failed, 203 suites;
  `npm run typecheck` clean. New `workspace-send-float-carry.test.ts`
  (numbered/relative follow/stay, roundtrip, race/refusal pins, real-observer
  dispatch) + `workspace-send-float-production.test.ts` (follow/stay/roundtrip
  with 1 membership / 0 geometry writes per leg, frame kept as-is).
- Windows: `cargo check -p tiler-windows` green, no code change needed
  (core kept signatures; `send-refused-floating` gate untouched).
- Full gates + independent review: done (Lead). cargo test --workspace
  green; clippy -D warnings clean; fmt --check clean; KWin 1397/1397,
  typecheck + bundle build clean; no native code changed (no CTest).
  Read-only review: 1 low parity finding (floating+fullscreen dispatched
  then failed closed); fixed at all 3 gates + 1 pin, suite back to green.

Outcome: delivered offline on `wip/d16-floating-transfer`, parked unpushed.
Native/live acceptance remains user-owned (pending list above).
Backlog line for Orchestrator: advance D16 to delivered-offline /
native-verification-pending; Windows adapter + native acceptance is
handoff item 22 (proposed text in handover).

Pending live checks (user-owned):
- Native WS1 -> WS2 -> WS1 floated roundtrip (frames/focus/follow/stay).
- Numbered + relative sends, destination tiles visually untouched.
- Restart-store float memory follows the window.
