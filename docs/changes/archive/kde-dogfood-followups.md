# KDE dogfood follow-ups (R-MAX-06 setting, radius 6, underlay research)

## Goal, scope and acceptance

- Bring KDE to parity with decided cross-platform defaults:
  1. R-MAX-06 born-maximized setting on KDE equivalent to Windows `core.born_maximized`: default `tile-on-launch`, alternative `keep-maximized-overlay` (existing KDE Q3 reserved-slot overlay behaviour); live changes affect subsequent admissions only. Functional names/values match Windows and the decision; WM names only in tooltips. Shared-core policy where it fits (Layering). KDE Settings KCM control consistent with existing settings patterns.
  2. Active border radius default 6 on KDE; explicit user radii preserved; no migration pre-release.
  3. Underlay default visibility (g): research only. Options for a default group-underlay colour/blend visible on black, white and grey on KDE and Windows. Produce options + recommendation in a short research note; do not change the default (user decision).
- Update spec/matrix, backlog entry, change record under docs/changes/ (archive on completion).
- Live checks user-owned: list pending (Settings UI, born-maximized launch, border radius).

## Non-goals

- No change to underlay default value.
- No Windows behaviour changes.
- No live KWin testing.
- No product behaviour beyond the cited decisions.

## Approach

- Investigation: map KDE settings plumbing, Windows reference tokens/labels/tooltips/defaults, and current underlay defaults.
- Implementation (born-maximized + radius): KDE setting + KCM + adapter wiring with subsequent-admissions-only semantics; radius default 6; red/green regressions.
- Underlay research note: options + recommendation, no code change.
- Verification: gates, spec/matrix/backlog/record updates, one independent read-only diff review.

## Verification

- Red/green regressions for born-maximized setting and radius default.
- Gates: cargo test --workspace, clippy -D warnings, fmt --check, npm test --prefix kwin, typecheck; native build + CTest for kwin/native-effect changes.
- One independent read-only diff review.
- Pending live checks listed for user.

## Material decisions

- KDE script/kwinrc key `bornMaximized` (camelCase convention: `sameAxisMove` <-> `core.same_axis_move`); values `tile-on-launch` (default) / `keep-maximized-overlay` exactly match Windows `core.born_maximized` and the R-MAX-06 decision; WM names only in tooltips.
- Adapter-only implementation (no shared-core change): entry-owned live value read per admission; floating-first sightings keep the R-MAX-03 overlay under both settings.
- Radius default 6 via `activeborderconfig.kcfg` only; no migration; explicit radii (incl. 0) preserved.

## Tentative decisions (pending user review)

- `bornMaximized` key spelling; adapter-only implementation (no shared-core
  change); floating-first sets retained across planner recovery, bounded at
  4096 ids oldest-first eviction; entry-side + adapter-side recording OR-queried.

## Blockers

- None.

## Evidence / outcome

- Implemented: KDE born-maximized setting + KCM +
  adapter wiring and radius default 6. Files: `kwin/src/born-maximized.ts`
  (new), `kwin/src/plan-adapter.ts`, `kwin/src/plan-adapter-entry.ts`,
  `kwin/contents/config/main.xml`, `kwin/contents/ui/config.ui`,
  `kwin/native-effect/unifiedsettings.ui`,
  `kwin/native-effect/unifiedsettings_module.cpp`,
  `kwin/native-effect/activeborderconfig.kcfg` (BorderRadius default 6.0),
  tests `kwin/tests/born-maximized.test.ts` (new, 13 tests),
  `kwin/tests/plan-adapter.test.ts`,
  `kwin/tests/native-kcm-static.test.ts`,
  `kwin/native-effect/scriptconfig_module_test.cpp`.
- Semantics: default `tile-on-launch` clears once then ordinarily admits;
  `keep-maximized-overlay` reserves a tile slot with maximize retained;
  subsequent admissions only; floating-first sightings keep the R-MAX-03
  overlay under both settings; explicit radii (incl. 0) preserved, no
  migration.
- Gates green on the final tree: npm typecheck, npm test 1413/1413,
  cargo fmt --check, cargo test --workspace (0 failures), cargo clippy
  --workspace --all-targets -D warnings; native build + ctest 32/32 on
  identical code (docs-only changes since); independent read-only review
  passed with findings repaired.
- Pending live checks (user-owned, not run): Settings UI (KCM control
  Apply/Revert), born-maximized launch both values, border radius
  appearance.
- Underlay research delivered as
  ([options](kde-dogfood-underlay-options.md)), recommendation candidate C
  (#662a82da); no default changed. Spec/matrix/backlog/record updates in
  this round.
