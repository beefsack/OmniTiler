# Windows rapid send-follow convergence

## Goal and scope

- Diagnose the rare two-app rapid alternating send-follow failure reported
  2026-10-26: focused arrival retains its singleton full-size frame beneath the
  destination's other window instead of retiling.
- Preserve core geometry/stacking ownership and native lifetime, freshness,
  scope, at-most-once and supersession gates. No timers, arbitrary waits,
  adapter policy or unrelated source-pager layout change.

## Acceptance and approach

1. Join the bounded incident tail's final core membership/slot/frame/raise intent
   to native dispatch, held/deferred work and available convergence wakes.
2. Add a realistic host-shaped desired-behavior regression before the smallest
   event-driven root-cause correction, within Layering and Simplicity.
3. Pass locked core/Windows tests, strict all-target package clippy, touched
   package fmt and diff checks; independently review native behavior boundaries.
4. Verify fixture-only rapid alternating send-follow, short rapid workspace
   toggle and directional-focus regressions with independent native readbacks.
   Stop/restore exact owners and verify fixture/settings/recovery cleanup.

## Evidence and outcome

- Final incident sends (revisions 506/509/510/511/516/521, focus effects
  `fx-179..184`) move terminal `w20` membership with follow but publish no
  allocation/effective frame for that arriving member (`Untouched` frame
  authority). Existing members retain their layout slots; no raise/stack
  operation is emitted. The observation following arrival normally assigns
  the mover's slot. Opaque destination identities change as empty workspaces
  prune; numbered index 2 is not one permanent workspace token.
- Tick 369 proves the singleton full frame was applied. Final send tick 370,
  revision 521, moves `w20` from `ws-48` to `ws-1`; view application succeeds,
  no frame writes are emitted, and focus `fx-184` is skipped `stale`. The mover
  remains foreground: this skip does not explain its incorrect geometry.
  No native geometry refusal, lifetime veto, accepted focus flight or persistent
  held scope explains the missing mover write; no mover target exists yet.
  The trace establishes no raise intent, not a measured native stacking order.
- Pending-edge preview tick 371 contains the destination member's show and
  location edges. The consuming hook drain coalesces with key-up-only handling.
  Only foreground HWNDs are retained; the other drained edges survive solely
  as `drain.woke`. The command lane does no world pass, and the previous rescue
  condition checks only retained foreground edges. The visibility trigger is
  consumed and dropped, not retained behind a later freshness or operation gate.
- No observation follows for about ten seconds despite ordinary pager wakes.
  New unrelated destroy traffic finally triggers tick 372 / revision 522:
  core assigns the mover's right-hand slot beside the existing lower-left
  member, beneath the retained top-band member, and both frame writes are
  accepted with exact readbacks. This is delayed convergence, not a permanent
  native refusal or a need to add core stacking policy.

## Correction and offline verification

- Preserve the consumed hook-work signal separately from generic wake state
  (which also includes chord transitions). After the command lane, service one
  existing observation-only reconcile when hook work drained but that lane
  never entered a world tick. Retained foreground obligations remain unchanged.
- Compare the existing observation watermark at lane entry/exit to avoid a
  redundant pass after real commands. This is a world-tick-entry watermark,
  advanced before dispatch fences, not proof of successful reply application;
  existing Busy/exhaustion behavior and all safety gates remain unchanged.
  No timer, arbitrary wait, command retry/replay, new native operation or core
  policy is added.
- RED: before correction, a key-up-only scheduler probe with consumed hook
  work and no foreground edge advances zero observations instead of one.
  Retained regressions prove exactly one owed pass, no extra pass after a real
  command, and a quiet chord-only key-up. A paired production-composition
  regression sends to a singleton then back to an occupied origin: the owed
  observation settles window/view operations without replay and both native
  readbacks match distinct non-full desired slots. The scheduler harness itself
  does not execute native window enumeration; these are complementary proofs.
- Independent boundary review found no blocking defect. Review corrections
  clarified watermark semantics, removed duplicate quiet coverage and extended
  the geometry regression from empty arrival to occupied return. An initial
  test-resource identity collision was corrected before final parallel gates.
- Final locked core/Windows tests pass: 853 core library and 1,057 Windows
  library passes, one existing ignored Windows test, plus integration/doc
  suites. Strict all-target core/Windows clippy, touched-package fmt and diff
  checks pass. No unrelated platform files changed.

## Fixture-native verification (2026-10-26)

- Fixture-only owner scope; decorated/resizable windows with exact process
  lifetime, native HWND and caller-id joins. Source plus dirty state, built
  payload/fixture hashes, launch provenance, settings byte copies and shell/SPI
  preimages are retained externally. Payload hashes match launch provenance
  and remain stable through each measured leg.
- Eleven genuine rapid numbered send-follow operations with two fixture windows
  cover singleton mover -> occupied return -> singleton. Within-burst production
  dispatch intervals are 156-249 ms; endpoint observation separates the bursts.
  Every send succeeds with the same exact mover foreground before/after, without
  injected input, foreground grants, refocus or retry. Endpoint joins show the
  command's slotless arrival, subsequent core slot and accepted exact mover
  readback: two disjoint split frames on occupied return, full frame on singleton
  arrival, and correct other-member visibility. No later command repairs either
  endpoint before readback.
- An earlier split-to-split run adds 22 rapid send-follow successes, eight genuine
  rapid occupied-workspace selects, and eight directional-focus controls (four
  genuine Left/Right changes, four expected Up/Down edge no-ops). Foreground,
  GUI-thread focus, visibility and fixture reads agree. That run used an
  owned-foreground-verified harmless key pair and explicit owner grant; it does
  not establish ungranted physical-hotkey activation. An initial DPI-virtualized
  helper geometry comparison was excluded and replaced by consistent physical
  readbacks joined to the paired core geometry.
- Exact-owner stop/independent restore and graceful fixture shutdown completed;
  settings bytes and sampled shell/SPI state match preimages, with no created
  process, recovery ledger or control-request residue. Zero restored-window count
  is not used as native visibility evidence.
- Outcome: root cause fixed, offline and fixture-native acceptance complete.
  CLI controls do not synthesize the incident's physical coalesced key-up; the
  scheduler regression supplies that proof. Ordinary-app physical alternating
  Win+Shift+1 / Win+Shift+2 recheck remains user-owned. Cross-output, games and
  genuine mid-switch supersession are not claimed by these legs.
