# macOS managed native workspaces

## Goal and acceptance

Implement the approved native-Space route with SIP enabled, no scripting
addition, no Dock injection and no virtual workspaces. Keep the shared core's
workspace policy and the KDE/Windows defaults and semantics.

- Verify owned-window synthetic title-bar drag plus native switch chord and its
  inverse, with exact membership readback and guaranteed input release.
- Select through Mission Control AX or the proven same-display Dock gesture;
  choose the simpler, lower-jank route on evidence rather than posting time.
- Authentic enables Desktop-N holders with owned preimages and explicit Revert;
  synthetic switch chords bypass interception without bypassing user ownership.
- Map native Spaces per display to independent core workspace layouts, apply
  trailing-empty lifecycle, and retain off-current window membership.
- Provide numbered/relative select, send-follow and send-stay with explicit
  unavoidable host differences and a short shell-neutral physical procedure.
- Pass macOS tests, strict clippy/format, signed build/verify and hosted CI.
  Physical acceptance remains pending until the user runs the procedure.

## Approach and bounded units

1. Implement/review a bounded owned-window drag transfer fixture and synthetic
   input provenance, then verify an eligible scoped native round trip.
2. Implement Authentic Desktop-N enable/Revert with conditional owned restoration
   and fake-backed settings coverage; real preference writes require verified
   backup and a permitted recovery procedure, otherwise defer to physical checks.
3. Integrate per-display mapping, independent layouts, workspace actions/defaults,
   trailing lifecycle and non-current-Space observation using existing core APIs.
4. Review native mutation/recovery boundaries, run final gates and publish the
   physical acceptance guide and durable route decisions.

## Evidence and open questions

- The approved manual drag routes and existing lifecycle/select evidence are in
  [the route survey](../research/macos-port/native-spaces-routes.md).
- MC AX selection has settled readback evidence around 628-660 ms; Dock gesture
  selection works both ways but its end-to-end latency and visual jank are not
  measured. Direct SkyLight transfer and the absent bridged dispatcher are not
  implementation candidates.
- Multi-display, macOS 15 floor, native fullscreen transfer and physical
  switch-back jank remain distinct acceptance questions; no unsupported outcome
  is silently selected.

## Diagnostic milestone (2026-10-16)

**Incomplete: synthetic transfer is not established and product workspaces are
not enabled.** The user-approved route is unchanged. No native preferences were
written, and the current tiling preview's bindings remain unchanged.

Delivered a bounded `spaces-drag-transfer` diagnostic, with either two existing
adjacent user SIDs or `--temporary` for an exactly adopted disposable Space.
One executor serves the native implementation and failure-injected tests:
title-bar grab, 50 ms grab settle, tagged HID switch chord, 400 ms held transition,
mouse-up, then bounded exact membership and current-SID polling. Every
post-down exit attempts release; unresolved input halts further Mission Control
actions and reports residue. Owned-Child identity, the selected focus oracle,
fresh AX positional identity, conservative CG occlusion checks and finite budgets
fence the posts. Pointer restoration uses an eventless warp, not input into the
original application.

The tap passes tagged exact Ctrl+Left/Right pairs without consuming, forwarding
or disturbing user-chord pending state. Untagged identical chords still follow
normal ownership. This is offline-proven only; HID-to-tap tag survival and
Desktop-N bypass remain unverified/unimplemented respectively. The tag is an
event-routing marker, not authentication against other local processes.

### Native evidence and limitations

- Signed LaunchServices runs observed Accessibility trust and unlocked session
  state before each leg. Bounded display-sleep companions were reaped by exact
  PID. The fixture did not arm a tap.
- The first temporary-mode run created a Space but refused before helper spawn
  because adjacency used the pre-create snapshot. Passing the adopted
  post-create snapshot repaired that fixture error. Exact-SID recovery removed
  the temporary Space and restored the original single-Space layout.
- The corrected run created one temporary Space (479 ms), verified independent
  public helper self-join O->T->O, then refused at the positional guard because
  above-normal-layer chrome covered the grab point. **Zero synthetic drag/key
  posts; no inverse dispatched.** This is a safety refusal, not a native
  transfer failure or SIP rejection.
- Exact helper stop/reap/record removal and absence passed. The temporary Space
  was removed (521 ms); final readback matched the original single-Space layout
  and current SID, with Mission Control absent. No helpers, probes, taps or
  display-sleep companions remained from these runs.
- Helper frame equality and protected-window stability read back false despite
  zero synthetic posts. Their causes are unresolved; no protected window was
  written back or focused. Foreground readback did not establish natural return
  after helper exit. Do not claim complete frame/focus restoration.
- Subsequent offline cleanup repair removed Mission Control closure from the
  unresolved-input branch, retained the display-sleep companion through final
  removal, made successful companion reap idempotent, and corrected early helper
  cleanup and synthetic-post reports. No further live run followed that repair.

### Route and consistency status

Selection has **not yet been chosen**: Mission Control AX has settled timing
evidence while Dock-swipe has selection evidence but no end-to-end timing or
visual-jank comparison. Existing evidence favors evaluating the simpler gesture
route without claiming it is already lower-jank. Create/remove still use the
proven Mission Control AX route.

No new macOS workspace defaults or semantics ship in this milestone. The
approved unavoidable host difference remains native drag-follow transfer and a
visible switch back for send-stay; its acceptability is for physical dogfooding.
Numbered/relative bindings must still be compared with KDE/Windows before
enabling them. Per-display numbering/chord attribution, native fullscreen and
multi-display transfer outcomes remain TBD, not inferred from this fixture.

### Verification

- macOS crate: 456 library tests and all eight integration suites pass; strict
  all-target clippy with `-D warnings` and workspace formatting pass.
- Unsafe-block count in the existing FFI module: **93 -> 96** (keyword
  occurrences 147 -> 151, including documentation). The three new blocks are
  maintained-binding AX hit-test/PID acquisition and retained-value ownership.
  No unsafe blocks occur in the new executor, runner or tests; no new dependency
  was added (only a feature on the existing Core Graphics binding).
- Final signed build and separate `just verify` pass for both bundles and the
  macOS 15 deployment floor. Final main SHA-256:
  `47c3ea8ac0718d2ab14d6b553db764269283cee9b835c534558c5ad0c07dd5ac`.
  The live artifact predates the final offline cleanup repair; live main SHA-256:
  `cd17a01575d5ec985e9ce65229027e28bf11df0457d083309a3485d031bf87cc`.

## Continuation and physical acceptance

Do not repeat the blocked automated fixture unchanged. First resolve its
frame/protected-stability diagnostic gaps and arrange a provably clear
owned-helper grab point, or use a short user-run physical discriminator instead
of further automated tuning. The existing
[manual transfer check](../research/macos-port/native-spaces-routes.md#user-manual-transfer-check-2026-10-16)
already establishes the ordinary physical route; it does not prove synthetic
delivery.

Next implementation units remain Authentic Desktop-N owned preimages/explicit
Apply and Revert (real writes deferred to user-run checks unless separately
safe), workspace mapping/independent layouts, lifecycle/actions/bindings,
off-current tracking, and final physical acceptance. No product workspace
command or enable/Revert command exists yet; do not substitute diagnostic
commands for the requested dogfood procedure. The final shell-neutral physical
script must be added here once those commands exist, covering select,
send-follow, send-stay, enable/Revert and the user's switch-back-jank verdict.
Backlog item 1 remains open, not awaiting final physical acceptance yet.
