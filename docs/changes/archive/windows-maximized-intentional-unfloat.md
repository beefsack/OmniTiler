# Windows maximized intentional unfloat

- Goal: item 15 / B9 R-FLT-06 ordinary maximized floating-to-tiled toggle
  unmaximizes, observes the clear and fresh-admits using the restored frame.
- Scope: Windows adapter and offline regressions, existing Engine admission
  and D3/D7 fixed tile overrides. Sticky/fullscreen/normal-to-float overlay
  refusals remain; workspace-retile Q3 is independent.
- Acceptance: one held nonactivating clear per explicit activation; exact
  lifetime/focus/domain/fullscreen/suspension/elevation fences before and after;
  no admission or override on refused/unobserved clear; later press available.
- Approach: reuse observed-clear policy from directional movement, then route
  through existing transactional unfloat admission and marker settlement.
- Status: delivered offline 2026-10-12 from base `e506350`; native pending.
- Outcome: `dispatch_float_intent` replaces only ordinary-maximized-unfloat
  refusal with `unfloat_maximize_clear`; fresh observations feed the existing
  transactional `apply_unfloat_to_tiled`, including D3/D7 marker settlement.
  No attempt map, deferred intent, timer or automatic retry added.
- Accepted evidence: scripted native effects drive actual clear/observe gates
  and real Engine admission. Success pins ordering/restored frame/sibling
  survival; unobserved clear plus later press, lifetime/focus/domain races,
  post-clear owned-metadata/captionless fullscreen and automatic fixed override
  regressions pass. Native marker write/readback is not proven by invalid HWNDs.
- Review correction: initial tests proved refusals but omitted success/order;
  hermetic native seam and real Engine success added. Post-clear fullscreen
  now re-reads owned metadata as well as fresh captionless/retained flags.
  Independent review accepted the final source and evidence, no offline gaps.
- Shared core/protocol/KDE code, dependencies and settings unchanged.
- Tentative 2026-10-12, pending user review: diagnostics use
  `float-maximize-clear` and `float-refused-maximize-clear` with cause,
  matching the directional-clear vocabulary.
- Verification: locked four-package build/test, all-package format, strict
  allowlisted Clippy and diff whitespace; native journeys remain user-owned.

## Final evidence and remaining checks

- Combined-tree native offline gates: Rust 1.99.0 MSVC host; locked four-package
  build/test pass (1461 tests, 0 failed, Windows lib 340 / tiling integration
  114); all-package format, strict allowlisted Clippy and diff whitespace pass.
- Linux/CI, fixed marker roundtrip and owner-restart acceptance remain pending.
  Ordinary clear/frame/focus was agent-observed below; physical input/feel and
  race/failure journeys remain pending. COSMIC observation confirmation
  remains user-owned, not an implementation gate.
- Ready-to-run scoped commands, Notepad/Calculator/Paint steps, expected
  outcomes and stop/restore/Terminal protection are in the
  [paired native plan](windows-maximized-floating-retile-overlay.md#user-run-native-plan-for-items-11-and-15).
- Exact next action: run that native plan and record actual observed results.

## Agent-observed native evidence - 2026-10-12

- Windows 11 physical desktop, ordinary integrity, explicitly scoped Notepad
  and Paint; Calculator and its hosted-app fence were configured but not
  exercised. Source `e506350` plus the combined Windows directional and
  maximize-admission changes. Artifact SHA-256:
  `F7E97EA00E54F54497601F83961EEC0AA54195109FC2DE75B6014358E10669FA`.
- PASS, ordinary B9: verified Paint foreground, exact-owner `workspace --float`
  produced `float-applied`; native maximize observed; the next CLI float toggle
  produced `float-maximize-clear` issued, dispatched, observed-cleared before
  either geometry write (trace tick 95). `unfloat-applied` retained focus and
  restored the project-raised topmost band. Paint was no longer maximized;
  both Paint and Notepad matched their planned tile rectangles, two writes,
  zero mismatches. Subsequent reconcile applied zero writes with matched
  readback. This proves the normal native route, not physical chord delivery.
- Fixed automatic/intentional override, marker/restart, sticky/fullscreen
  refusal and raced/failed-clear live legs not run; their native cells remain
  TBD. No product defect observed in the exercised journey.
- Cleanup verified: graceful stop and independent restore completed with clean
  ledger; owner absent and readiness false, hooks/overlays released and owned
  mouse-Snap session setting restored. Saved settings restored byte-identically
  by SHA-256; all test windows closed. Non-test inventory matched baseline;
  session Terminal remained visible and unminimized. No keys injected.
