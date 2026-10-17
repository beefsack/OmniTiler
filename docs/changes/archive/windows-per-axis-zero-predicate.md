# Windows per-axis-zero fixed-size predicate

## Goal and scope

- Deliver handoff item 17 / G-05 with item 13: zero is unset per axis; a fixed axis requires equal nonzero min/max bounds.
- Reuse shared `FixedSizePredicate`, `is_fixed_size_with` and the Engine opt-in; no adapter policy copy, shared-core/protocol or KDE edits.

## Outcome and evidence

- Delivered offline 2026-10-11, prerequisite `a41fcb5` plus runtime delivery ([item 13 record](windows-fixed-size-admission.md)).
- One bounded native query transports both track sides; fresh inset conversion retains `(640,0)` and `(0,480)` as per-axis-zero hints rather than discarding the whole pair.
- Schema-v1 `core.fixed_size_predicate` defaults to `both-axes-fixed`; `either-axis-fixed` floats through the genuinely fixed nonzero axis. Missing/full-zero/sentinel guards remain shared; live changes do not reclassify retained clients.
- Production-conversion and real retained Engine fixtures cover both partial-zero orientations under both predicates, positive fixed/full-zero/missing/sentinel controls, subsequent-only adoption and first-exit/enable intersections.
- Native MSVC Rust 1.99.0 locked core/protocol/KWin FFI/Windows build/test, workspace format, all-target strict clippy and diff checks passed; 325 Windows library and 336 integration tests passed for the combined delivery. Prerequisite publication CI passed all six jobs; runtime publication CI checked separately after push.
- Native app hint representation, DPI/frame timing, Settings and game no-touch acceptance remain user-owned pending; offline completion does not replace those journeys.

## Second agent-live sweep (2026-10-17)

- Bounded Calculator query retained minimum tracks but zero/unknown maximum
  tracks; ordinary tiling under `both-axes-fixed` was consistent with this
  min-only representation. No fixed-axis equality was present, so neither a
  positive partial-zero discriminator nor native `either-axis-fixed`
  acceptance is claimed. Wider predicate/DPI/game checks remain pending.
