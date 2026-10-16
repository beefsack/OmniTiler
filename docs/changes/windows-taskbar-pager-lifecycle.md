# Windows taskbar pager lifecycle

## Goal and scope

- Close primary bottom-taskbar auto-hide, shell restart and live shared-settings
  gaps without Explorer injection, hooks or AppBar registration.
- Keep secondary and non-bottom taskbars safely refused. No shared-core or KDE
  behavior changes. Packaged application artwork remains deferred.

## Approach and acceptance

- Query `SHAppBarMessage(ABM_GETSTATE)` with fresh primary taskbar facts. Hide
  the overlay while auto-hide is enabled, retaining the tray number/menu and
  logging `taskbar-autohide`. Re-evaluate every pump, including reconfiguration
  and `TaskbarCreated` recovery.
- Tentative for user review: hide throughout auto-hide, even while the taskbar
  temporarily shows. Alternative: track its slide-in/out; deferred to avoid
  animation coupling and floating overlays.
- Inspect stale-hide/fresh-geometry handling for `WM_SETTINGCHANGE`,
  `WM_DISPLAYCHANGE` and `WM_DPICHANGED`; add offline regressions.
- Verify actual shell restart, auto-hide on/off, live shared-settings opt-out,
  Calculator miniature appearance and several-minute GDI/USER handle stability.
  Restore changed taskbar and application settings and verify readback.
- Physical display scale/resolution changes remain pending; this change uses
  offline coverage and does not claim physical reconfiguration acceptance.
- Run locked four-package build/test/strict all-target clippy, workspace format
  and diff checks; verify cross-platform CI after integration.

## Current evidence

- Implemented Windows-only auto-hide state/refusal and transition logging.
  Existing reconfiguration handlers immediately hide stale overlays and the
  next pump reads fresh facts; inspection found no defect requiring a fix.
- Independent source review found no blocking correctness or safety findings.
  Five new regressions cover auto-hide poses, frame refusal, bit flags,
  fresh geometry/DPI after disable and stale-hide consumption/rediscovery.
- Locked four-package build/test passed: 2,318 passed, zero failed, one ignored.
  Workspace format and diff checks passed. Strict all-target clippy initially
  found two ignored must-use results in a new test; explicit discard repaired
  them and the full clippy command passed. Targeted pager tests: 48 passed.
  Final integration gates and CI remain pending. Auto-hide/lifecycle delivery
  is independent of the open Calculator appearance finding below.
- Native auto-hide on/off passed using a test-only AppBar state helper: the
  overlay disappeared with `taskbar-autohide`, stayed absent while auto-hide
  remained enabled, and returned when disabled. Original AppBar state restored
  exactly and verified with `ABM_GETSTATE`; production uses only GETSTATE.
- Live shared-settings opt-out hid the overlay and re-enable restored it.
  Original settings bytes were restored and their digest verified unchanged.
- Actual exact-identity Explorer termination and automatic shell restart passed:
  native `TaskbarCreated` receipt is evidenced by tray re-add, followed by a new
  pager window anchored to the changed tray edge. The pager window's own
  broadcast receipt is not separately logged; polling also revalidates facts.
- Resource samples at 30-second intervals over 5.5 minutes plateaued: GDI 17,
  USER 29, process handles 205/206 settling to 204. The interval separates
  accumulated growth from transient paint/worker activity with low probe cost.
- Graceful stop and independent restore completed; no owner, pager window or
  recovery ledger remained. Shell and unrelated terminal remained running.

## Appearance finding requiring resolution

- Calculator was managed and icon lookups reported misses; exact packaged
  artwork remains deferred. A synchronous `PrintWindow` capture showed only
  numbered cells, with no miniature rectangle, despite icon requests requiring
  a drawable miniature. This discrepancy is not yet classified as a product
  defect. Appearance acceptance remains open independently of the verified
  auto-hide, shell recovery and settings lifecycle behavior.
- Screen-band capture evidence was discarded because repeated captures were
  stale; it is not used to claim physical appearance or pixel-exact recovery.
