# Born-maximized shared policy

## Goal and scope

Lift R-MAX-06's `tile-on-launch` default / `keep-maximized-overlay`
policy into `tiler-core`, with a conformance-pinned KDE copy. Preserve KDE behavior,
labels, tooltips, the `bornMaximized` key and subsequent-admissions-only
setting adoption. Windows adapter replacement follows in handoff item 23;
its current behavior remains the baseline.

## Acceptance and approach

- Use one shared data table in Rust and KWin tests to pin normalization and
  clear/keep outcomes for ordinary tiled-first born-maximized admission.
  Run the existing KDE behavior regressions before and after the test additions:
  clear once under tile, reserved-slot overlay under keep, unchanged fixed,
  fullscreen, floating, sticky, floating-first and later-maximize lanes;
  live setting flips do not reclassify admitted windows.
- Add shared-core policy and unit tests for both values and default handling.
- No protocol changes, build-time generation or adapter rewiring. KDE rewiring
  is deferred to the post-0.1 Push code down pass.
- Verify workspace tests, strict clippy, fmt, KWin tests, typecheck, bundle
  and Windows compilation. Native CTest applies if native/KCM code changes.
- Record durable implementation ownership and specific Windows replacements
  in decisions and handoff item 23; archive after acceptance.

## Investigation and decision

- KDE selects clear/keep in `kwin/src/plan-adapter.ts`'s
  `clearMaximizeAtAdmission`, before `carriedSnapshot` and planner dispatch.
  It clears native maximize synchronously and refetches observation before
  constructing the planning request.
- `buildRequestPayload` deliberately omits native maximized state. Both
  modes use ordinary shared-engine admission; overlay isolation stays local.
  The existing `DescribePlan` response has no maximize-admission decision.
  Merely carrying the setting would leave the KDE policy copy authoritative.
- Source and existing regression comparison found matching born-policy
  outcomes in KDE and Windows, with host-specific identity, observation and
  actuation mechanisms. No behavior choice between the copies is required.
- User decision 2026-10-13, option C, amends the earlier immediate KDE rewire:
  shared core owns the policy; KDE keeps its existing copy with a shared
  conformance table. Runtime protocol changes and build-time generation are
  declined. Windows will consume core directly under handoff item 23, with the
  same table as acceptance. Host-specific gates remain adapter-owned.

## Verification and outcome

- Delivered `tiler_core::born_maximized::BornMaximized`: exact token parsing,
  `tile-on-launch` default/fallback and eligible tiled-first clear/keep choice.
  Normal dependencies remain zero; JSON decoding is test-only.
- One 8-case table, `test-fixtures/born-maximized.json`, is consumed by core
  unit tests and KWin tests. It covers both values, missing/null, invalid and
  empty strings, numbers and booleans. KWin asserts both normalization and
  the real adapter clear/keep call outcome. Core has four new unit tests;
  KWin has sixteen new conformance checks.
- KDE born-maximized baseline: 13 passed before additions; the same 13 tests
  plus 16 conformance checks passed afterward (29 total). Production KDE,
  settings labels/tooltips/key, protocol, native/KCM and Windows code unchanged.
- Workspace: 1643 passed (915 unit + 728 integration; 0 failed/ignored;
  0 doc tests). KWin: 1491 passed in 219 suites, 0 failed/skipped. Strict
  workspace clippy (`-D warnings`), workspace fmt, typecheck, bundle and
  core portability check passed. Planner test fixture build passed.
- Portable `tiler-windows` all-targets compile check passed on Linux. Native
  Windows compilation remains unverified: the required MSVC target/toolchain
  was unavailable. Run the Windows build gate on the native toolchain before
  claiming native acceptance. Native CTest does not apply to this change.
- Core/conformance implementation complete; archived with the native compile
  gap explicit. KDE rewiring is queued in post-0.1 Push code down; Windows
  replacement and shared-table acceptance are specified in handoff item 23.
