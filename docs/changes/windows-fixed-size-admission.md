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

## Accepted prerequisite evidence and remaining integration

- Observation/settings prerequisite implemented: schema-v1 predicate choices, functional radio labels/tooltips, bounded min/max-track transport and per-axis-zero normalization. Engine admission remains disabled; the minimum-only transport retains legacy both-positive-axis validity and excludes maximums from clamp acceptance.
- Native MSVC Rust 1.99.0 locked four-package build/test, workspace format, four-package all-target strict clippy and diff checks passed. Windows library 277 tests and tiling integration 93 tests passed; native application/query acceptance remains pending.
- Corrected production partial-zero loss, stale automatic enable latches, marker-removal identity-guard lifetime and first-exit mark consumption. A stale-hint fallback/unknown-retry fullscreen-exit repair was rejected and removed; first exit now uses current hints, with unknown treated as non-fixed.
- Corrected a maximize-clear regression: slot seeding could suppress the required ordinary clear. Pending-clear rows now remain slotless, and committed ordinary admission pins survive later hint changes.
- D7 provenance: write `OmniTiler.TileOverride.v1` only when a successful tile win overrides known fixed-origin automatic floating; generic non-fixed unfloats write no tile marker. A hydrated marker supplies provenance even after hint loss, without granting write/recovery authority.
- D7 restart: extend the same exact-lifetime admission fences to retained/maximized rows and hydrate before automatic classification or native effects.
- Complete the fixed-override reserved-overlay intersection and real retained Engine controls, then enable admission and run final full gates. Full item delivery pointers, archived item records and native acceptance remain pending.
