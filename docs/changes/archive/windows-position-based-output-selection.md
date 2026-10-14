# Windows Position-Based Output Selection

## Outcome And Scope

- Handoff item 16 window-transfer dependency delivered offline `fda618f`,
  2026-10-14. Directional move and horizontal cross-focus already used the
  shared selector in the [foundation](../windows-multi-output-foundation.md);
  [explicit sends](windows-explicit-output-send.md) now use the same selector.
- Audit: `workspace_owner::select_directional_target` delegates to shared
  `tiler_core::output_selection::select_for_window`. Native frames and FULL
  rectangles supply centre projection, then span overlap, then x/y/stable-ID
  ties. Work rectangles govern placement, not adjacency. No wrap or fallback
  to output origin distance. Unreadable topology refuses output transfer;
  no candidate is a no-op. Valid local moves retain local-first behavior.
- Pointer drops deliberately use pointer containment (`output_at_point`),
  not directional ranking. Rect-to-output queries verify native arrival.
- Selection stays pinned to dispatch source and target through current-view,
  topology, gap/mode/member/lifetime and arrival fences. Relocating the mover
  never re-ranks its destination. Explicit send uses ordinary admission;
  directional crossing retains R4 nearest-source landing.
- Whole-workspace migration's item 14 dependency is closed offline 2026-10-14
  ([record](windows-workspace-output-migration.md)): shared `select_for_migration`
  chooses largest shared edge, then left/top/identity, never moving-window
  position. This record's original evidence covers window-path integration.

## Evidence

- Three additive Windows selector regressions: stacked candidates in all four
  directions, negative origins, span-overlap fallback, left/top/identity ties
  and exact odd-extent half-open centre containment. Focused tests: Windows
  directional foundation 13 passed; shared selector 14 passed.
- Real-Engine adapter coverage selects a non-left/top destination, retains it
  after mover relocation, rejects topology/view/member/lifetime and late
  arrival/readback drift, and separates FULL adjacency from work-area placement.
- Native locked four-package gates 2026-10-14, rustc 1.99.0, MSVC target:
  build/test, format, strict all-target clippy and whitespace checks pass.
  **1,892 passed, zero failed, one ignored**. Core 615, protocol 190,
  KWin FFI 40, Windows 1,047 (665 library + 382 integration).
- No shared core/protocol/KDE source changes. Synthetic differing physical
  work rectangles do not establish real mixed-DPI/native two-output acceptance.

## Pending Physical Checks

- Test multi-candidate centre/seam/overlap/tie selection with occupied and
  empty destination current workspaces; mirror all directions, including
  negative-coordinate and stacked arrangements.
- Test panel/work-area gaps and mixed-DPI settlement, follow and rebound stay,
  source reflow and target admission; perform scoped stop/restore on both outputs.
- Verify largest-edge migration separately in the physical item 14 journey.
