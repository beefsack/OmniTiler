# Windows workspace sends for ordinary window states

## Goal and approved behavior

- User 2026-10-16: ordinary Windows windows can be sent to another workspace
  even when born floating, fixed-size, maximized or fullscreen. Preserve their
  floating/maximized/fullscreen state, existing follow/stay semantics and frames.
- The OmniTiler Settings dialog is an ordinary slotless workspace member: it
  follows workspace visibility and supports sends without being tiled.
- This supersedes the Windows own-Settings unmanaged policy and automatic-fixed
  send refusal only. Sticky refusals, proof/owner/control-window exclusions,
  fullscreen/game safety fences and other platform behavior remain unchanged.
- No unsolicited game/fullscreen geometry, state or focus changes. Existing
  explicit-command gaming interpretation remains authoritative.

## Investigation and approach

- Settings was excluded by own-UI/dialog admission, never acquired workspace
  membership and therefore stayed visible across switches. Send stopped as
  `unmanaged` before shared-core transfer planning.
- Settings runs in a separate process of the same executable, so the owner's
  same-process PID refusal was not the cause and remains unchanged. Four
  dialog gates blocked discovery, membership/hide eligibility and retained
  overlay handling; all now admit the Settings class.
- Automatic fixed-size floats were already slotless workspace members and hid
  on switches, but separate adapter/core deferred-send eligibility refused them.
- Reuse existing deferred membership-only transfers and fenced follow/stay
  tails; keep Windows-specific admission separate from KDE/macOS policy.
- Bounded work: investigate exclusions and decisions; implement Windows-only
  eligibility/admission plus regression coverage; independently review native
  hide/reveal and safety boundaries; run offline gates and record results.

## Delivered outcome and accepted offline evidence (2026-10-16)

- Windows enables a default-off shared deferred-send capability for automatic
  fixed floats. Session adoption, pairing and splitting retain the capability
  and automatic-origin markers; existing default behavior stays unchanged on
  KDE/macOS. Existing membership-only follow/stay tails preserve frames and
  overlay flags without mover or survivor geometry writes.
- Settings explicitly advertises equal min/max outer track sizes through
  `WM_GETMINMAXINFO`, using the existing fixed-size observation contract.
  Inferring fixed admission from its non-resizable styles alone was rejected
  as insufficient evidence. Early creation queries, null pointers and unusable
  outer sizes default-handle; initialized queries need no application-state
  dereference. Constructor size/style/DPI logic is unchanged.
- Regression red/green: automatic fixed-float send eligibility failed before
  the fix and passes afterward. Added coverage pins Settings dialog eligibility,
  fixed-hint readiness/conversion under both predicates, automatic transfer
  follow/stay and roundtrip, retained markers/frames/restart memory, unchanged
  trees and geometry-write suppression. Default-off automatic refusal and
  sticky/explicit-override/dual-overlay refusals remain covered.
- Independent source review accepted identity/proof/control exclusions,
  hide/reveal compatibility, default-off platform isolation and unchanged
  fullscreen fences. Review follow-up corrected stale comments and guarded
  fixed-hint advertisement until Settings initialization is complete.
- Locked build and test passed for `tiler-core`, `tiler-protocol`,
  `tiler-kwin-effect-ffi`, `tiler-windows`: 2,210 passed, 0 failed, 1 ignored.
  All-workspace fmt check, the same packages' all-target strict clippy and
  `git diff --check` passed. Native-stub tests prove planning, membership
  routing and fail-closed native tails, not successful hardware hide/reveal.
- Tray/pager entries select workspaces rather than send windows; existing
  keyboard/CLI send entry points share the corrected owner route.
  Born-maximized policy and existing retained/born fullscreen routes are
  unchanged. No unresolved behavioral ambiguity required a new reference row.
- Physical acceptance remains pending; this record stays active.

## Verification

- Locked build/test and all-target strict clippy for `tiler-core`,
  `tiler-protocol`, `tiler-kwin-effect-ffi`, `tiler-windows`; workspace fmt check
  and `git diff --check`. Check CI for the pushed revision.
- Regression coverage must distinguish Settings from owner/proof/control
  windows, automatic fixed-size floats from intentional floats and sticky
  windows, and maximized/fullscreen retained and slotless members. Exercise
  membership, switch visibility, state-preserving follow/stay transfer and
  no geometry writes; preserve existing safety/race refusals.

## Pending physical acceptance

- On WS1, open Settings, floating Notepad, maximized Paint and a non-game
  fullscreen window. For each, send WS1 -> WS2 -> WS1 using numbered and
  relative commands, both follow and stay, with occupied and empty targets.
- Check Settings hides on workspace departure and reappears only on its
  workspace; repeat after sending it. Use both tray and taskbar pager selection
  to verify the same membership visibility (neither currently exposes sends).
- Compare exact frames, floating/maximized/fullscreen flags, selected workspace
  and focus; verify unrelated source/destination tiles do not move. Exit
  maximize/fullscreen on the destination and check the established restored
  frame/admission policy. Repeat an automatic fixed-size born-floating window.
- Verify owner/proof/control windows remain excluded, sticky sends still
  refuse, and games/fullscreen guards remain unchanged: no unsolicited tiling,
  geometry/state writes, focus changes or overlays. Explicit fullscreen sends
  must preserve fullscreen; stop and report corruption rather than retrying.
- Physical outcomes remain unverified.

## Agent-live follow-up (2026-10-17)

- Single-output, medium-integrity run at `dffdf871`, with explicit application
  scope including the separate own-Settings process: Settings remained slotless
  and frame-stable through numbered follow to an occupied target, roundtrip,
  relative next/previous, stay to an empty target, and hide/reveal. Sticky send
  refused with `send-refused-sticky`; owner/control windows remained excluded.
- Settings is the automatic fixed-size fixture, using its delivered equal-track
  hint contract; Calculator tiled normally and is not evidence of automatic
  floating. No second automatic fixed-size application was established.
- An earlier `unmanaged` reply was caused by omitting the Settings executable
  from the explicit test scope, not by a product admission defect. Evidence:
  `sendunit-j1/20261017-064537-23500`, run `01dd5da6ef04b65d` and its provenance
  sidecar. Ordinary numbered follow/stay and relative previous also succeeded.
- Unrelated-tile immobility was checked on selected legs, not every matrix cell.
  Both-pager interactions, comprehensive survivor checks and fullscreen remain
  unverified. Fullscreen was skipped to avoid the Xbox prompt; physical feel
  and shortcut delivery are not established by these scripted checks.
- Locked four-package gates passed: 2,458 passed, 0 failed, 1 ignored; build,
  fmt, strict all-target clippy and diff check passed. No source change.
