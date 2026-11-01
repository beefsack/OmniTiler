# Windows active window border

## Goal and outcome

- Deliver a dogfood-grade Windows active border matching KDE settings and
  `tiler-core::visual`, with fresh visible-frame geometry, suppression,
  low-latency following, diagnostics and owned-process teardown.
- Practical machine acceptance is complete. Native gates and independent
  live-behavior review pass. Implementation `b5374ce` is pushed; all four jobs
  in [CI run 37004892347](https://github.com/beefsack/OmniTiler/actions/runs/37004892347)
  pass. This note is archived and parity queue item 1 is done.
- Scope: Windows adapter/rendering and necessary scoped proof support only;
  no group underlay implementation or KDE/principles changes.
- Source baseline: predecessor implementation at `28b8899`; resumed at
  `6678d2e` with uncommitted implementation. Physical Windows 11 x64 build
  26200, one 2560x1440 display, work area 2560x1380, DPI 120 (125%).

## Implementation and decisions

- `active_border.rs` delegates eligibility/expansion/theme selection to core;
  KDE width 3.0, gap/radius 0.0, inner-radius interpretation and scale rounding
  give a 4-physical-pixel outline at DPI 120. Configurable ranges follow KDE.
- `active_border_sys.rs` owns a per-pixel-alpha layered, click-through,
  nonactivating tool window. No foreign attribute mutation. DWM border colour
  was rejected because it cannot provide configurable width/gap.
- Immediately-below-target placement in the target's band matches KWin's
  target-parented Z=-1. Fresh actual visibility/geometry/z-order checks repair
  restore occlusion even when drawing inputs are unchanged. Presentation
  failures hide stale surfaces and recover on subsequent refreshes.
- Theme choice for user review: official `DwmGetColorizationColor` system
  accent analogue of KDE Selection highlight, configured `#2a82da` fallback;
  core availability/positive-alpha gate, cheap owned-window broadcast requery.
  This host returned `0xE3726D6C`, RGB `#726d6c`. DWM shadows tint the ring.
- Default on; flags: `--no-active-border`, `--active-border-width`,
  `--active-border-gap`, `--active-border-radius`, `--active-border-color`,
  `--no-active-border-theme`. CLI help exposes defaults/ranges.
- Event-loop refresh reads fresh foreground/DWM geometry, avoids owned-overlay
  wake feedback, preserves scoped identity fences and emits bounded lifecycle
  diagnostics. Ordinary dialogs remain eligible; targeted shell surfaces hide.
- Durable choices are recorded in `docs/../../decisions/visuals.md#shared-visual-safety`.
  KDE sources: `kwin/native-effect/activeborderconfig.kcfg`,
  `activeborderlogic.h`, `activewindowborder.cpp` (Selection theme and Z=-1).
  Upstream `OutlinedBorderItem` and OpenGL item renderer confirm inner radius
  and rounded scaled thickness.

## Accepted evidence

Run directories contain source delta, copied executable hashes, process/start
identities, observations and cleanup. Pixel sampling is restricted to the
owned ring, not application content; DIB checksums alone are insufficient.

| Run directory under `target/` | Accepted observations |
| --- | --- |
| `windows-active-border/20261002-205625-31076` | Focus and exact DWM-frame expansion, composed ring, active synthetic titlebar move and edge resize, minimize retarget/hide, maximize/restore, explicit theme-off blue, off switch, graceful and exact forced-owner exit plus independent restore. Both exits destroy the overlay. |
| `ab-followup/20261002-211917-14436` | Directional/workspace phases: real marked focus/move dispatch, workspace send/select/return, exact geometry/z-order and composed 8/8. Overall report fails superseded fullscreen/ordinary fixture legs. |
| `ab-followup/20261002-213911-16444` | Ordinary/shell phases: scoped Notepad/Calculator/Paint focus and composed ring, workspace hide/reveal, extras closed; Start/Task View and Alt+Tab hold-cancel. Production scoped Start hides as `no-target`, returns exact/composed. Task View proof records hidden then suspended; Alt+Tab cancels without foreign activation. Overall report fails the superseded fullscreen waiter. |
| `ab-followup/20261002-215521-13468` | Fullscreen phase: exact preformed captionless monitor-covering owned helper, initial suspend, untouched frame/zero overlay, exact style restore, resume with exact adjacent-below geometry and composed 8/8, clean stop/restore. Corrected derived timing metadata. |

- Gesture evidence is synthetic, not physical delivery. Eight move samples
  track the current frame; maximum motion sampling interval 85 ms. Resize
  trails up to two samples, oldest matching-frame sample age 159 ms, maximum
  motion interval 83 ms. These measure sampled staleness, not exact latency or
  a hard upper bound. The approximately 1.4 s total is gesture duration.
- Resize composed evidence is side-only where top/bottom strips leave the
  screen/work area; other shown legs include full 8-point ring sampling.
- Latest direct audits: zero project actors/overlays, ledger/stop/request
  absent, helpers/extras closed, no hidden residue; arranging raw 1, pen raw 35
  using `SPI_GETWINARRANGING` 0x0082 / `SPI_GETPENVISUALIZATION` 0x201E.
  Final post-commit read-only audit:
  `ab-followup/20261002-215521-13468/final-audit.json`, 22:10:40 local.
- Final native gates: locked stable build/test/strict all-target Clippy for
  `tiler-core`, `tiler-protocol`, `tiler-kwin-effect-ffi`, `tiler-windows`;
  workspace rustfmt; `git diff --check`; final harness `-Mock` and PS parser.
  Windows lib 52 tests and border CLI suite 5 pass. Independent review findings
  were corrected; final evidence inspection found no blocking issue.

## Failed approaches and causal repairs

- Original `20261002-193749-13904` failed first activation: fixture omitted E8
  prime. Corrected `20261002-194953-34600` showed the border but then violated
  the helper CLI's foreground-mutation refusal. Preserve that safety gate;
  held-identity syscommands and verified synthetic gestures are the fixture.
- `20261002-201933-21772` found post-restore composed 0/8. Its catch-and-pass
  status is rejected. Repair actual below-target z-order reconciliation and
  make composed failures fatal. Independent review additionally corrected
  gesture held-handle ordering/WAIT_FAILED handling; offline checks accepted
  the causal repair after the owned live run.
- One-shot and repeated external fullscreen writes on a managed helper raced
  with retiling. Stop that strategy. Preform fullscreen before owner launch
  and require stable foreground/geometry. A later reshow timeout marked after
  the valid resume/reshow transition; open the mark before restore and use the
  actual exact/composed state as decisive evidence. Cleanup-induced foreign
  focus in that failed run was not the cause.
- An older ordinary preflight rejected an unrelated Steam window despite
  scoped invocation. The scoped fixture preserves all identity/host fences
  and excludes unrelated targets; no foreign window mutation is authorized by
  its presence. Separate shell probes from fullscreen dependencies.
- Early ad-hoc read-only audits used wrong SPI codes and DPI-unaware queries;
  discard those interpretations. Correct SDK constants and PMv2 reads above
  reproduce the accepted baseline.

## Dogfood and bounded physical checks

- Start: `just tile --user-start --trace`. Border is enabled automatically.
  Stop: `just tile-stop` (graceful stop then independent restore).
- To compare: add `--no-active-border`; for configured blue add
  `--no-active-border-theme --active-border-color '#2a82da'`.
- Physical checks: appearance/gesture feel, physical shell/flyout/Alt+Tab/Task
  View delivery; topmost windows; custom width/gap/radius; other monitors/DPI.
  Synthetic/hosted CI does not establish these observations.

## Next: group underlay

- Reuse the owned layered carrier's create/alpha-paint/place/hide/destroy paths
  in `active_border_sys.rs`; parameterize fill/alpha and anchor only when
  needed. Ring placement must continue to recover actual z-order.
- Existing active-group route: `Engine::session` / `CoreCommand::ActiveGroup` /
  `boundary::resolve_active_group` / `active_group::describe_active_group`.
  Do not rederive projected group geometry in the Windows adapter.
- KDE `activewindowborder.cpp`: underlay Z=-2, parented to the first renderable
  member in bottom-first stacking order, scene-to-parent mapped geometry;
  Meta held plus focus eligibility and anchor control visibility. Config:
  translucent `#40808080`, extension -1 resolves to border width. Core
  `visual.rs` already supplies extension/outer-rect/focus visibility policy.
- Task View proof hides then suspends; Start production hides as no-target;
  Alt+Tab hold-cancel avoids foreign activation. Physical shell occlusion and
  topmost underlay placement still require fresh targeted evidence.
- Exact next action: start parity queue item 2, group underlay, using the
  existing active-group route and owned carrier.
