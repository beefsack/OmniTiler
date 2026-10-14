# Windows taskbar pager live spike 2

## Goal and scope

- Resolve the taskbar child's invisible composition found in the first spike.
- Compare owned child z-order, manifest-enabled layered rendering, and, if
  needed, DirectComposition; inspect TrafficMonitor's actual rendering path.
- Throwaway prototype only; retain the per-output pager target and existing
  tray settings/toggles. No production implementation or shell configuration.

## Acceptance and approach

- Capture taskbar styles/composition siblings and test the simplest rendering
  candidates with composed-pixel evidence, not paint/API success alone.
- Once visible, verify real app icons, injected mouse-cell delivery, physical
  DPI placement, helper fullscreen yield, shell-control interaction, sustained
  app churn, and rough idle/change CPU and repaint cost.
- Inspect existing product manifest setup and document the exact working
  styles, manifest, z-order and paint technique plus a new-module size estimate.
- Verify prototype/app cleanup and taskbar baseline; update the research and
  backlog, inspect the documentation diff, and run `git diff --check`.
- Bounded units: rendering mechanism and source comparison; working-path
  behavior/cost validation; independent evidence and cleanup review.

## Outcome

- Explicit `HWND_TOP` placement preserved top native sibling order but did not
  expose GDI cells. Taskbar lacks `WS_EX_NOREDIRECTIONBITMAP`; its full-width
  composition bridge has that style.
- Initial layered/swap-chain negative captures are not accepted as feasibility
  failures: capture omitted layered-window inclusion, layered API paths were
  mixed, and swap-chain resources expired before observation. Corrected the
  evidence plumbing; no actual DirectComposition target was tested.
- Corrected `SRCCOPY|CAPTUREBLT` captures verified visible manifested layered
  GDI children with both color-key and opaque-alpha modes; each diagnostic cell
  supplied 4,060 saturated pixels. Plain GDI remained invisible. DirectComposition
  is unnecessary for the successful run.
- Windows 11 build 26300.9711, one scaled-DPI output. Working probe manifest
  embeds Windows 8+ compatibility and PMv2; opaque layered GDI is the simplest
  demonstrated path. Product manifest embedding remains a required build change.
- Earlier validation: one borrowed app icon visibly drawn; three injected cell
  clicks delivered; foreground borderless helper occluded the visible pager and
  it returned after restoration. Current-scale DPI checks passed.
- A 32-second app-churn run reported no disappearance in 33 sparse samples;
  eight paints averaged 1.55ms, max 2.80ms. Prototype CPU 15ms/10s idle and
  94ms/32.8s active; excludes GPU/DWM and includes message-pump polling.
- Remaining acceptance: two distinct icons/window insets, identity-bound
  Start/app-button clicks, exact independent recovery, changing/mixed DPI,
  secondary taskbars, shell/display lifecycle and game/auto-hide behavior.
- Final validation stopped at a `SetParent` projection `E_INVALIDARG` without
  authoritative native last-error evidence. The unchanged successful probe
  also failed factory checks and its pixel positive control. Repeatability
  is the blocker, not a demonstrated universal limitation of layered children.
- Enumeration undercount was independently reconciled with all nine baseline
  handles alive and unchanged; no shell repair. Final prototype/app residue
  absent and Explorer/Terminal identities preserved. Physical appearance/input
  remains unverified. Earlier class-only recovery is not exact-identity proof.
- [Research](../../research/windows-port/taskbar-workspace-indicator.md#live-spike-2)
  and backlog record the exact technique, limits and provisional 800-1,400 LOC
  estimate for a new native pager module. No product code was changed.
- Working probe artifact SHA-256:
  `d1c1e21f385d0fa21d451afb9a521af43a90a7879cfd1d866da5c40b5a90c6f5`.
  Final failed validation artifact SHA-256:
  `d59af828ac5c3e32bccfeb9092614b32d577ac58be0aa00b4484b79ffc349b0b`.
  Baseline revision `eec9c2e`; temporary source/raw records stay outside the repo.
- Evidence/source and documentation reviewed; `git diff --check` passed.
