# Windows taskbar workspace pager

## Goal and acceptance

Deliver a product-integrated, default-enabled graphical pager immediately left
of the system tray. Show every workspace for the taskbar's output, numbered
cells with active colour and darker empty backgrounds, scaled miniature window
outlines, click-to-switch, and live updates. Keep the existing tray pager.
Icons are optional. Never cover the tray or hide/shrink for app-icon crowding.

## Scope and approach

- Choose hosting after bounded source investigation and an authorized live
  overlay probe. External nonactivating topmost hosting is acceptable; no
  Explorer injection, AppBar or tray placeholders.
- Integrate owner snapshots and existing workspace selection. Contain native
  unsafe code, keep production files below the approximate 1,500-line cap.
- Extend resilience and safety after MVP: shell rediscovery/repaint recovery,
  fullscreen suppression, DPI/auto-hide, per-output taskbars and settings.
- Do not alter unrelated windows or settings. Physical games and multi-output
  acceptance require separate evidence, not synthetic claims.

## Bounded units

1. Investigate source integration and hosting, establish a bounded live probe
   with exact-owner cleanup, and recommend the implementation route.
2. Implement pager rendering/model, owner integration and opt-out with focused
   tests; include safe resilience features supported by evidence.
3. Run authorized owned-window live journeys and native gates; repair findings.
4. Independently review native/live behavior, resolve findings, publish and
   inspect cross-platform CI.

## Verification

- Native allowlist build/test, whole-workspace formatting, strict allowlist
  clippy, and `git diff --check`.
- Live evidence binds executable/hash and process identity, baseline geometry,
  cell/window pixels, click result, live refresh and exact cleanup. Keep local
  identities and paths in temporary evidence only, not repository records.
- Cross-platform CI checks cfg boundaries. Unavailable physical or shell
  mutation checks remain explicitly pending.

## Decisions and outcome

- Product implementation now includes numbered GDI cells, uncapped managed
  window miniatures, output-scoped click dispatch, observation-backed frame
  snapshots, a default-on live Settings opt-out, DPI-scaled cells/fonts,
  Explorer rediscovery/retry and conservative fullscreen/elevation hiding.
- Independent native/live-risk review found queued clicks surviving opt-out
  and managed fullscreen, stale tray geometry, capped window miniatures,
  missing real frames and unscaled labels. Repairs share visibility/dispatch
  gates, drop suppressed clicks, read primary tray geometry each pump, render
  every member and prefer real observations, with targeted regression tests.
- Native allowlist build/test/fmt/strict all-target clippy and diff check passed:
  2,134 passed, zero failed, one pre-existing ignored. Final verification found
  four strict-lint issues in the new tests/order; mechanical repairs passed a
  complete fresh gate run without changing production behavior.
- Native product acceptance passed on a primary bottom taskbar at fractional
  scaling: numbered active/empty cells and light miniatures/borders, exact
  tray-left exclusion, nonactivating styles, two-way message-driven workspace
  selection, create/send/close refresh and retained hidden-workspace miniatures.
  Pixel interiors agreed with real window-frame readbacks.
- Sustained pixels/geometry survived a topmost-reassert interval and an
  owned-window TaskbarCreated message. Managed fullscreen hid the pager and
  leaving restored it. Graceful and exact-owner emergency stop, independent
  restore, clean ledger, no pager/test-app residue and unchanged saved settings
  were verified. No actual Explorer restart or physical game result is claimed.
- Cross-platform CI is pending publication. After that, archive this record.
- Secondary tray anchors and non-bottom taskbars safely refuse unknown
  geometry. Physical multi-output, game, real Explorer-restart, auto-hide and
  display-reconfiguration checks remain pending. Icons are omitted.
- Tentative hosting choice: external topmost, nonactivating tool window, not
  Explorer child hosting. A native probe passed active/empty/border pixel
  samples, cell hit-testing, unchanged foreground, tray exclusion and exact
  overlay destruction. Child-host repeatability remains unresolved; no shell
  injection or dependency is needed for the overlay route.
- The probe first exposed DPI-virtualized capture coordinates; repeating with
  PMv2 physical coordinates passed. Product geometry must stay physical.
- Use existing workspace dispatch safety fences, but taskbar clicks address
  their rendered output rather than requiring that output to have focus.
- Icons are omitted: app/package identity lookup is not trivial. Live opt-out
  remains deferred because it would modify shared saved settings; schema,
  hot-apply and suppressed queued-click refusal have offline coverage.
- Whole-strip overcapacity (more fixed-width cells than the entire tray-left
  span) safely refuses; wrapping/another treatment needs product review. This
  is distinct from app-icon crowding, which never changes pager layout.
- Initial repository state was clean.
