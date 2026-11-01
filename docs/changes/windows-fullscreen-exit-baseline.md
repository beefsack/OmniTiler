# Windows app-owned fullscreen exit baseline

## Approved direction

Keep REQ-MAX-05/05b refusal for app-owned fullscreen now. Plan an
adapter-local native-state baseline captured when a window is adopted, kept
separate from the project-owned-fullscreen marker. Storage and crash-safe
native-write preimages must be coordinated with R12. R14 exposes only the
ownership/exitability fact to core; core does not store or infer this native
baseline.

Per-case behavior changes only after user-owned live checks accept restoration.
No live checks were run for this record. First-seen fullscreen has no adoption
baseline and remains refused; exclusive fullscreen is also expected to remain
refused unless its per-case test proves a safe, truthful restore.

## Read-only current-state sizing

- `crates/tiler-windows/src/tiling_sys/fullscreen.rs:286-347` captures a
  preimage only for project-owned entry: the owned caption/thick-frame style
  bits, whether the window was maximized, and its outer rectangle when not
  maximized. The metadata is written before the native fullscreen effects.
- Windows admission runs through `assignments.rs::ensure_workspace_assignments`
  and `world_tick.rs::admit_observed`; adoption does not stamp the owned
  fullscreen marker. `fullscreen.rs::read_fullscreen_meta` treats a missing
  marker/preimage as app-owned. The owned marker wins toggle direction and
  causes the owned-exit path, so stamping it on adopted windows would claim an
  exit authority that was not acquired on entry
  (`keyboard_overlays.rs:587-595`).
- Native fullscreen clears are not promoted to tiler-owned clears: the float
  toggle path treats fullscreen facts or owned metadata as a veto
  (`float_toggle.rs:923-958`).
- Current owned-exit success requires setter acceptance and style-bit
  readback, then metadata removal. It does not verify a settled outer
  rectangle after restore (`fullscreen.rs:349-367,428-467`). The new baseline
  path must not claim stronger restore proof without adding and verifying it.
- KDE's existing native route sets the KWin `fullScreen` property to a boolean
  and schedules async observation (`kwin/src/world-native.ts:888-924`). The
  inspected macOS AX helper also sets a fullscreen boolean and requires a
  matching AX readback, but is fixture-only and owned-window scoped
  (`crates/tiler-macos/src/ax.rs:1274-1284,1355-1407,1411-1424`). These host
  setter shapes are comparisons, not cross-platform authorization or proof of
  app-owned restore safety.

## Per-case live-check matrix

| Case | Adoption baseline / action | Required user-owned check before changing refusal |
| --- | --- | --- |
| a. Browser F11 | Adopt ordinary browser, capture baseline, then enter app-owned fullscreen | Restore via project action; verify original chrome, maximize state and settled outer rectangle; verify no tiling/geometry/focus side effects beyond the explicit action |
| b. Video player | Adopt ordinary player, capture baseline, then enter app-owned fullscreen | Same restore proof; include player-specific fullscreen behavior and confirm it does not resume/restart or otherwise alter playback unexpectedly |
| c. Borderless-windowed game | Adopt ordinary borderless game, capture baseline, then enter fullscreen | Same restore proof; verify game remains windowed, correct display/mode and no unasked focus, overlay or input effects |
| d. Exclusive fullscreen game | Do not assume an adoptable or restorable window-manager frame | Verify host can observe and restore safely without mode loss or game interference; otherwise retain refusal |

The check must separately cover a window already fullscreen when first seen:
without an adoption-time baseline it remains refused. Passing one case does not
authorize the others.

## Evidence and follow-up

- Contract boundary: [R14 window-state commands](../decisions/contract.md#window-state-commands-r14)
  separates core exitability from adapter native state.
- Current requirement: [REQ-MAX-05/05b](../spec/functional-spec.md#maximize).
- Implementation work remains under the [R14 backlog](../backlog.md).
- Per-case live checks are user-owned and are not authorized by this design
  record.
