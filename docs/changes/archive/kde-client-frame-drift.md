# KDE tiled client frame drift

- Status: implemented and verified offline (2026-10-29); native acceptance TBD.
- Goal: promptly observe client-driven tiled frame changes and reconcile the
  retained tile under D2, without adopting client sizes or fighting a resizing
  client. Native buffer/acknowledgement settlement remains TBD.
- Scope: KWin frame-change observation and own-write echo fencing; core-owned
  per-window correction budget layered over existing safety, hint projection,
  clamp learning and write suppression. No live lifecycle or fixture changes.
- Root cause: ordinary frame changes do not invalidate the KWin world snapshot.
  Existing core reconciliation already retains the tile, but suppression keyed
  only to identical frame pairs does not bound a client cycling sizes.
- Approach: subscribe to frame geometry changes and defer synchronous own-setter
  echoes until apply finishes; unsafe/disappeared windows still invalidate
  immediately. Core permits at most two consumed mechanical attempts per window
  lifetime and unchanged desired frame in a continuous observed-change burst
  (initial layout writes count). Exact target echoes preserve the count. A target
  change or >=500ms observed quiet re-arms, without retry timers or polling.
  Missing/unreadable/ineligible observations break continuity without clearing
  the budget; zero clocks cannot prove quiet. Existing clamp/transient gates
  remain additional vetoes. Floating placements do not use the burst budget.
- Acceptance: failing-first offline regressions for prompt observation, safe
  own-write echoes, changing-pair/exact-echo loop bounds, quiet/target/lifetime
  reset and replay handling. Existing float, fullscreen, gesture, hint and clamp
  behavior remains authoritative.
- Units: inspect observation and policy flow; implement bounded core policy and
  mechanical KDE wiring with regressions; review safety and verify repository
  gates; record evidence and archive.
- Verification: core/Windows/protocol tests, workspace strict clippy excluding
  macOS, KWin tests/typecheck, Rust formatting, portable checks and diff hygiene.
- Outcome: KWin observes `frameGeometryChanged`; core retains tile topology and
  sibling shares, and owns the bounded additional suppression. The adapter still
  applies controlled targets mechanically. No wire contract or client-size
  adoption policy changed; existing hint projection and clamp learning remain.
- Regression evidence: four new core tests failed before implementation on
  changing-frame suppression, zero clocks, sibling isolation and replay/budget
  handling. Three new KWin entry tests failed before the frame subscription.
  Final core policy suite covers 59 cases, including accepted exact writes,
  echo/redrift after exhaustion, quiet/target reset, close pruning, floats and
  minimized/unreadable observation gaps. KWin tests cover safe coalesced echoes,
  unsafe immediate invalidation and remove/stop detachment.
- Full-suite correction: initial KWin run exposed 12 stale trace tests that
  treated the frame signal as probe-only. Updated contracts require one
  persistent production listener plus an independently detached trace one-shot;
  ordinary/installed bundles still strip every probe identifier. Formatting of
  new tests was corrected after its first check failed. Independent safety
  review findings (exact-echo/gap coverage and a signal citation) are resolved.
- Final sequential verification passed: Rust core/Windows/protocol tests
  (2,335), strict all-target workspace clippy excluding macOS, KWin tests (859),
  TypeScript typecheck, Rust formatting, portable boundary check and diff hygiene.
  No known macOS baseline failure was encountered by these checks.
- Native acceptance remaining: isolated self-resize and persistently resizing
  fixture on Wayland and X11; exact native buffer/ack and visual outcomes TBD.
  Burst continuity is based on observed frames, not unseen native buffer commits.
