# Windows app-owned fullscreen exit baseline

## Approved direction

Keep REQ-MAX-05/05b refusal for app-owned fullscreen now. Plan an
adapter-local native-state baseline captured when a window is adopted, kept
separate from the project-owned-fullscreen marker and the R12 crash-safe
native-write preimage. R14 exposes only the
ownership/exitability fact to core; core does not store or infer this native
baseline.

Per-case behavior changes only after user-owned live checks accept restoration.
First-seen fullscreen has no adoption baseline and remains refused;
exclusive fullscreen is also expected to remain refused unless its
per-case test proves a safe, truthful restore.

## Windows adapter implementation

- Ordinary windowed adoption captures the checked caption/thick-frame mask,
  maximize state and actual outer rectangle before the tiler applies a
  frame. The first capture wins for the lifetime; unreadable, first-seen
  fullscreen or already project-owned state is not used as a baseline.
- The adapter stores adoption evidence separately from R12 and prunes dead
  lifetimes alongside R12 recovery. Both stores bind to the exact native
  window lifetime; an adoption row neither stamps the project marker nor
  authorizes a native write.
- Project-owned exit verifies chrome, show state and settled outer geometry
  before retiring its R12 preimage. Normal restoration requires the exact
  saved outer rectangle; maximized restoration verifies the settled
  maximized frame. Failed proof keeps the recovery record.
- App-owned fullscreen still cannot exit via the project command. Merely
  having an adoption row does not change the reported exitability.
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
- Fixture proof: `scripts/windows-fullscreen-baseline-proof.ps1 -Live`
  checks adoption, exact normal outer restoration and untouched app-owned
  fullscreen. The real-app checks above remain user-owned.
