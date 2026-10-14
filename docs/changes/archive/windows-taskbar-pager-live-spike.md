# Windows taskbar pager live spike

## Goal and scope

- Resolve whether an owned pager child can render inside the Windows 11 taskbar,
  using tray-adjacent placement on each output without reserving work area.
- Compare app-list and right-side anchor movement, native bounds/events, child
  rendering/input/DPI/visibility, and a floating topmost fullscreen comparison.
- Throwaway prototype only; no product code or shell/settings modification.

## Acceptance and approach

- Inspect native taskbar and UI Automation structure; capture bounded geometry
  and event evidence while disposable application icons appear/disappear.
- Review the prototype and recovery paths before its live rendering experiment.
- Test only owned prototype windows and disposable application instances;
  preserve unrelated applications and verify exact cleanup and taskbar baseline.
- Record findings and uncertainties in the indicator research, refresh its
  backlog reference, and verify documentation with `git diff --check`.
- Bounded units: prototype/preflight and read-only structure; live geometry,
  rendering and fullscreen experiment; independent evidence/cleanup review.

## Product decisions

- Existing tray icon stays for settings/toggles. The per-output pager is
  visualization plus click-to-switch, with numbered workspace/window cells.
- AppBar rejected because work-area reservation interacts with maximized windows.
- Injection rejected. Taskbar-child embedding is a feasibility candidate,
  not an adopted or supported hosting contract.

## Outcome

- Windows 11 build 26300.9457, one scaled-DPI output. Native/UIA structure,
  taskbar-filtered location events and 30-second geometry polling inspected.
- Tray-adjacent anchor stayed fixed while the legacy app-list edge had a 55px
  range; current XAML app-content movement and secondary taskbars unverified.
- Child parent/physical rect/DPI and 3/3 hit tests verified; local synthetic
  selection changed. GDI paint did not appear in composed taskbar pixels.
  Visible hosting is the blocker; no production implementation selected.
- Foreground borderless helper occluded taskbar/child without clearing
  `WS_VISIBLE`; the floating topmost comparison visibly overlaid fullscreen.
  Since the child was invisible at rest, visible-child hiding remains unproven.
- Corrected parent-return/coordinate handling and replaced the initial unpainted
  comparator capture with message-pumped evidence. Only corrected results are
  accepted. Separate-process recovery restored a helper nudge while the main
  controller remained alive; post-controller-exit restoration unproven.
- Cleanup: all prototype windows and disposable app/helper instances gone;
  taskbar identity/rect/styles/child set/UIA bounds and app-list edge restored.
  Calculator required exact-owned-process forced cleanup; existing frame host
  preserved. Physical appearance/input and pixel-identical cleanup unverified.
- [Research](../../research/windows-port/taskbar-workspace-indicator.md#live-spike-2026-10-14)
  records findings, product decisions, recommendation and residual risks;
  decisions/backlog updated. Evidence reviewed against source and native records.
- Prototype artifact SHA-256:
  `0d8beb448b56b25d577ab7c3755a8baf19855808d895d2df43222c737f21b57a`.
  Temporary raw geometry/process identifiers and prototype source stay outside
  the repository. Baseline source revision: `977e039`.
- Documentation diff review and `git diff --check` passed.
