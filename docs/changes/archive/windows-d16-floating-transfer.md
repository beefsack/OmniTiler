# Windows D16 floating workspace transfer

## Goal and acceptance

- Same-output numbered/relative follow/stay sends carry intentional ordinary
  floats and floating+fullscreen/maximized through shared slotless `MoveDeferred`.
- Membership-only actuation: neither tree reflows; no mover, survivor or overlay
  geometry writes, including drifted tiles and the stay source-only tail.
- Preserve exact floating frame, float intent, overlay state and identity-keyed
  restart memory; destination overlay exit stays floating. Sticky and
  automatic-fixed refusals remain; no setting or shared policy change.

## Approach and verification

- Narrow the intentional-float gate; stamp only the floating mover's maximized
  send flag from the live snapshot. Existing fullscreen/fixed stamps remain.
- Route deferred follow/stay through existing fenced membership/hide/reveal/focus
  tails with geometry application suppressed on both sides.
- Verify real Engine carry, roundtrip, refusals, overlay exit, sole-source,
  empty-target and production application-count seams before offline gates.
- Initial follow integration still applied destination geometry; review rejected
  that acceptance gap. The corrected select route skips application even when
  survivor frames drift. Native outcomes remain pending.

## Delivered outcome and offline evidence (2026-10-13)

- Windows adapter delivery complete; shared core/protocol unchanged. Observed
  float movers also bind their exact overlay snapshot in the carried fences.
- Fifteen new tests cover real Engine wire/carry/roundtrip/exit and production
  routing, numbered/relative follow/stay, drifted survivors, refusals and
  ordinary tiled controls. Ordinary send tests reach membership transfer;
  fake-window native hide fails closed. Combo entry tests prove committed
  deferred planning and zero writes but defer on unreadable native overlay
  readback; they do not prove successful native combo transfer.
- Independent review found no blocker. Follow geometry suppression and the
  observed-combo fence snapshot were corrected before final gates.
- Rust 1.99.0, `x86_64-pc-windows-msvc`: locked build/test for `tiler-core`,
  `tiler-protocol`, `tiler-kwin-effect-ffi`, `tiler-windows` passed: 1,818 passed,
  0 failed, 1 ignored. All-workspace fmt, allowlisted all-target strict clippy
  and `git diff --check` passed. No live testing performed.

## Pending physical acceptance

- Ordinary, floating+fullscreen and floating+maximized WS1 -> WS2 -> WS1:
  numbered/relative x follow/stay; occupied/empty target and sole-window source.
- Compare exact frames and flags on arrival, source/destination tile frames,
  selected workspace and focus; exit each overlay on the destination and verify
  the remembered floating frame without tiled admission.
- Verify sticky/automatic-fixed refusal and intentional-float memory after an
  owner restart. Native overlay/game and physical shortcut outcomes are TBD.

## Verification follow-up (2026-10-14)

- Initially no float transfer was attempted. Scoped lifecycle stop/restore passed,
  but its post-exit restore had zero hidden-window claims. Earlier ordinary-app
  evidence proves workspace reveal and graceful/watcher cleanup, not standalone
  post-exit restore of still-hidden ordinary windows.
- The non-hooking lifecycle `run --hide` proof requires an identity-tagged
  `tiler-test-window.exe`; ordinary Notepad is rejected before hiding. The
  recovery precondition was subsequently satisfied with an agent-owned
  disposable test helper: independent post-exit restore revealed one retained
  hidden claim with the same lifetime and frame.
- Agent-observed numbered ordinary-float follow/stay roundtrips passed with
  scoped helpers. Mover frames were retained; source/destination tile frames
  stayed unchanged across the checked transfer legs. Correlated transfer
  replies were slotless with no geometry application.
- Floating+fullscreen entry and follow-transfer retained the fullscreen frame
  without destination reflow. A genuine destination exit was not dispatched:
  the fixture could not establish the required owned foreground. Fullscreen
  exit, maximized combo, relative/comprehensive matrix and restart memory remain
  pending; unchanged fullscreen frames are not evidence of an exit defect.
- Fixture triage resolved apparent index inconsistency: numbered operations
  share the same one-based positions (0 selects trailing); opaque workspace
  tokens are creation identities. `float-mismatch` is successful floating
  settlement with a frame delta. Native/readback comparisons used one coordinate
  convention rather than mixing DPI-virtualized and physical rectangles.
- All owners and fixtures were cleaned up with unchanged saved settings and
  restored Mouse Snap. No new D16 product defect or physical acceptance claimed.
