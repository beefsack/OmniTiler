# Windows fixed-size admission

## Goal and acceptance

- Deliver Windows handoff items 13 and 17 together using the shared fixed-size predicate and Engine admission opt-in.
- Observe minimum and maximum track bounds in one bounded native query, preserving fresh DPI/frame normalization and failure-as-unknown semantics.
- Add schema-v1 `core.fixed_size_predicate`: `both-axes-fixed` by default, or `either-axis-fixed`; zero is unset per axis. Changes affect subsequent admissions only.
- Preserve lifetime-bound automatic origin, explicit tile override and intentional/sticky intent through observation and domain transfer.
- Classify first born-fullscreen exit and every tiling-enable candidate before native effects. Fixed/maximized admission keeps its floating base regardless of `core.born_maximized`.
- Automatic fixed admission changes membership only: no geometry, focus, stacking or keep-above writes.
- Persist successful fixed-window tile overrides with the existing reserved on-window marker; clear on re-float, verified close or complete-inventory prune. Missing/degraded identity recomputes without retry or authority expansion.

## Scope and approach

- Windows adapter/settings/UI/tests only; reuse shared core APIs. KDE code is outside scope.
- First deliver bounded hint observation, per-axis normalization and predicate settings/UI with admission disabled. Preserve the existing minimum-only Engine rows so observation does not change geometry/clamp behavior.
- Then integrate lifetime/origin, restart markers and native-effect fences before enabling Engine admission.
- Verify shared partial-zero behavior through Windows observations and retained Engine replies, including subsequent-admission controls, fullscreen exit, tiling enable and restart fallback.
- Update matrix/spec/decision/backlog delivery pointers; preserve unsupported reference and live outcomes as TBD.
- Archive one delivery record for item 13 and one for item 17 after acceptance.

## Verification

- Native MSVC toolchain identity, locked build/test for core/protocol/KWin FFI/Windows, workspace format check, package clippy with warnings denied, and diff whitespace check.
- Published CI green. Physical/ordinary-app hint, fullscreen, overlay, restart and no-touch journeys remain user-owned pending.

## Current evidence

- Base: `16e58d2`; Windows restart-intent markers and born-maximized setting are already delivered.
- Research found shared predicate/Engine APIs sufficient; no shared-core or protocol edit is currently required.

## Outcome and accepted evidence

- Observation/settings prerequisite implemented: schema-v1 predicate choices, functional radio labels/tooltips, bounded min/max-track transport and per-axis-zero normalization. That prerequisite kept admission disabled and retained legacy minimum-only Engine rows; runtime admission is enabled below.
- Prerequisite `a41fcb5`: native MSVC Rust 1.99.0 locked four-package build/test, workspace format, four-package all-target strict clippy and diff checks passed. Windows library 277 tests and tiling integration 93 tests passed; all six publication CI jobs green ([run](https://github.com/beefsack/OmniTiler/actions/runs/38132210054)).
- Corrected production partial-zero loss, stale automatic enable latches, marker-removal identity-guard lifetime and first-exit mark consumption. A stale-hint fallback/unknown-retry fullscreen-exit repair was rejected and removed; first exit now uses current hints, with unknown treated as non-fixed.
- Corrected a maximize-clear regression: slot seeding could suppress the required ordinary clear. Pending-clear rows now remain slotless, and committed ordinary admission pins survive later hint changes.
- D7 provenance: write `OmniTiler.TileOverride.v1` only when a successful tile win overrides known fixed-origin automatic floating; generic non-fixed unfloats write no tile marker. A hydrated marker supplies provenance even after hint loss, without granting write/recovery authority.
- D7 restart: the same exact-lifetime admission fences cover retained/maximized rows, with hydration before automatic classification or native effects.
- Runtime delivery enables the shared Engine opt-in after complete observation/origin/no-touch wiring. Predicate live adoption is subsequent-only, with no rebuild. No shared-core/protocol or KDE edits.
- Separate runtime suppression for all explicit tile wins from durable fixed-origin overrides. Generic core override flags cannot prove fixed origin; repeated generic float/unfloat regressions now cover that distinction. Hydration uses a held exact identity with before/after lifetime/PID checks and shared marker priority.
- Tentative Windows D3/D4/Q3 routing: a durable fixed-origin maximized override reserves a tiled overlay under both `core.born_maximized` choices, even after hint loss; automatic fixed births remain slotless floats and generic maximize routes keep their existing behavior. R-SPC-13 native discriminator remains TBD.
- Maximum bounds now reach the shared AR12 clamp assessment: explained maximum clamps stop counting as drift, but never redistribute shares. Converted maximum-only, unhinted and sentinel controls prove this recorded shared policy.
- Final native MSVC Rust 1.99.0 locked four-package build/test, workspace format, four-package all-target strict clippy and diff checks passed. Windows library 325 and integration 336 tests passed. Independent contract review accepted the corrected production paths.
- Owned, invisible message-only tests positively proved held marker install/readback and visible/retained-max hydration under both born modes with hint loss, plus corrupt/tag-mismatch/destroyed controls. Hydration created no Engine session or native frame/style/parent changes. This does not prove ordinary-app restart or game behavior.
- Offline-delivered 2026-10-11, prerequisite `a41fcb5` plus runtime delivery. Publication CI checked separately after push. Item 17 has its own [record](windows-per-axis-zero-predicate.md).

## Pending native acceptance

- Apply/Revert both predicate choices; admit equal full/partial-zero, missing and sentinel hints with fresh DPI/frame conversion and bounded hung-query observations.
- Exit born fullscreen once with changed hints/predicate, then repeat toggles; prior automatic floats restore floating. Verify no geometry/focus/stacking/keep-above interference.
- Disable/enable tiling after ordinary/fixed hint changes; preserve intentional/sticky/explicit tile intent. Test fixed and non-fixed maxima under both born-maximized choices.
- Explicitly tile an automatic fixed float, lose hints, maximize, restart, then enable tiling. Verify marker hydration before automatic origin, reserved-slot restore, re-float clear and lifetime replacement.
- Physical/ordinary-app/game checks remain user-owned under the live Windows guide, with stop/restore verification; no such journey was run for this delivery.
