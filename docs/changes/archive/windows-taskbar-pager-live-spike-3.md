# Windows taskbar pager live spike 3

## Goal and scope

- Diagnose the earlier parenting/pixel-verification failure and establish a
  repeatable manifested layered-GDI taskbar child without product changes.
- Validate three-cell real-window geometry/icons, injected selection and shell
  clicks, bounded app churn, redraw survival, fullscreen return and paint cost.
- Preserve the per-output pager target and the existing tray settings icon.

## Acceptance and approach

- Inspect raw `SetParent` return/last-error, handle identity, DPI context,
  style order, direct child creation and integrity; verify composed pixels
  against a layered positive control.
- Run ten fresh-process create/render/verify/destroy cycles on the working path.
- Verify two distinct real icons, investigate Calculator identity/icon fallback,
  identity-bound Start and app-button effects, 60-second churn with 100ms
  sampling, borderless-helper occlusion/return and 30-second idle CPU.
- Check product manifest embedding; recommend new Windows pager modules and
  owner snapshot data, documenting unsupported outcomes as remaining gaps.
- Bounded units: diagnosis/repeatability; content/interaction/robustness/cost;
  independent source/evidence/cleanup review. Stop on an unresolved diagnostic
  or restoration ambiguity rather than broadening effects.
- Append research evidence, update backlog, archive this note, review the final
  documentation diff and run `git diff --check`.

## Outcome and evidence

- Ten fresh processes: zero taskbar-host passes; raw `SetParent` returned NULL
  with last-error 87, and direct child creation failed with error 5 throughout.
  This resolves native error attribution, not the underlying cause.
- Initial over-taskbar positive controls failed too. Their pixel results and
  post-failure client-coordinate placement were not accepted as child-rendering
  evidence. One corrected diagnostic proved layered alpha-255 paint/capture in
  the work area with 4,060 pixels per diagnostic color.
- Actual taskbar/child PMv2 contexts matched; explicit thread-context save/set/
  restore and `WS_CHILD` before parenting still failed. Both processes had medium
  integrity and equal session ids; access-control mechanism remains unknown.
- Enumeration inconsistency persists. Independent checks verified the nine
  known descendant handles and baseline geometry/identity. Corrected created
  windows had successful destruction and immediate `IsWindow` false; final
  prototype windows/processes were absent and taskbar baseline preserved.
- Content/icons, shell/cell clicks, churn/redraw, fullscreen return and cost
  validation were blocked by the host fixture. No current success is inferred
  from earlier working artifacts. No further live work is required for this
  bounded negative outcome.
- Product manifest embedding is absent; runtime PMv2 does not supply Windows 8+
  `supportedOS`. No-go until hosting is repeatable; provisional 800-1,400 native
  LOC in new pager modules once proven, plus owner snapshot and manifest work.
- [Research](../../research/windows-port/taskbar-workspace-indicator.md#live-spike-3)
  records hashes, exact technique, raw errors, qualified-control distinction,
  remaining acceptance gaps and module/data recommendation. Backlog updated;
  no product code changed. Baseline revision `b912400`.
- Source, diagnostic evidence and final cleanup independently reviewed;
  documentation diff reviewed and `git diff --check` passed.
