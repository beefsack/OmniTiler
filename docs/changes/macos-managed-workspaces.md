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

The original Ctrl-as-Meta default-binding question was resolved by the user's
2026-10-16 Ctrl+Cmd auxiliary-arm choice, documented below and in the
[minimal discriminator](../spec/reference-outcomes.md#macos-native-workspace-discriminator).
The already-approved independent layouts, per-display mapping, trailing
lifecycle and send-stay route do not need to be re-decided.

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
- Diagnostic source milestone `977463e` passed all eight hosted jobs:
  [CI 38866437557](https://github.com/beefsack/OmniTiler/actions/runs/38866437557).
  A preceding revision's Windows portable run exposed an existing fake-backend
  race: the observation-pump fixture released its worker before configuring the
  only readback. The test-only correction configures truth before release;
  production runtime and the existing oracle are unchanged. Targeted and full
  macOS tests, strict clippy and format pass after that correction.

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

## Ownership, catalogue and retained-layout milestone (2026-10-16)

**Still incomplete: native workspace actions remain unarmed.** This milestone
delivers settings ownership, action plumbing and independent layout retention,
not successful synthetic transfer or managed trailing-Space lifecycle.

### Delivered

- Explicit `spaces-desktop-apply [--count N]`, `spaces-desktop-revert` and
  read-only `spaces-desktop-status`. Apply enables the observed Desktop-N
  holder without rewriting its chord (default coverage 1–9, supported 1–10).
  The enable-only SkyLight API follows local Silica usage; no plist is written.
  Native enable effect and logout/reboot behavior remain unverified.
- Exact observable enabled/chord preimages are saved in a private durable
  Application Support backup before any write, with file and parent-directory
  synchronization. Safe standard-library file locks serialize the full
  transaction; fresh publication cannot replace an existing backup. Retry
  preserves original preimages. Revert restores only still-owned values,
  preserves external edits and retains unresolved recovery records. A rebound
  holder is preserved and warned unusable for the default transfer chord.
- Tagged exact Ctrl+digit switch pairs now bypass the tap; identical untagged
  user chords remain consumed. Offline-proven only; native tag delivery and
  numbered synthetic posting remain unverified/unimplemented respectively.
- Workspace catalogue/action plumbing: numbered select/send, relative
  select/send, previous toggle and output-send-follow; numbered/relative stay
  rows are bindable but unbound. Every workspace row is capability-disabled;
  the product does not intercept these new chords or pretend to execute them.
- User-selected auxiliary arm is Ctrl+Cmd with default Ctrl Meta, otherwise
  shared Meta+Ctrl. Numbered actions retain plain Meta and Shift sends. Static
  conflict checks avoid known Ctrl+Cmd+F/Q/Space/D; custom global/common-app
  conflicts still require physical checking. Compatible leaves established
  unshifted Ctrl arrows/digits to macOS without inventing replacements.
- Core domains now retain each native Space's independent layout across manual
  switches. Off-current windows remain members; hidden domains reconcile without
  frame/focus emission or desired-geometry writes. Current-only native effect
  fences remain. Deleted Spaces/displays retire their domains; transient type
  gaps retain hidden domains, and unprovable display mapping/geometry parks
  existing sessions rather than destroying them. Native row order is retained.

### Corrected drag fixture: safety refusal, not transfer proof

The changed fixture checks five titlebar points with fresh CG occlusion and AX
identity, without exemptions for foreign chrome. One bounded native run again
refused before posting: **all five points were covered, zero mouse/key posts,
no inverse dispatched**. Together with the earlier fixed-center refusal,
automated grab-point tuning is stopped pending a changed fixture or physical
discriminator; no production transfer path depends on this failed fixture.

The run's terminal JSON reports independent helper O→T→O recovery first
(selection 673/672 ms, self-join 321/220 ms), temporary create/remove 491/496 ms,
exact temporary-Space removal and original single-Space/current restoration.
Post-run `mc-inspect` reports one original Space, Mission Control absent and an
unlocked trusted session. Exact helper exit/reap/record removal, helper absence
and display-sleep companion reap are reported. These are cleanup evidence, not
synthetic transfer acceptance.

New diagnostics identify a protected-window bounds change despite zero posts,
an unreadable final helper AX frame, and split foreground sources after helper
exit. No protected window was written or focused. Frame stability and natural
foreground return are **not established**. Stop live work under these conditions;
do not relax occlusion or focus guards to get a passing result. The live artifact
SHA-256 was `66b336f70f4d14869159a19474108e8bdad78401a0c64bd6225df92482b578b6`;
it predates the later retained-layout changes.

### Verification and exact continuation

- Fake-backed settings tests cover lock contention, first-preimage retention,
  partial failure/retry, durable-path derivation, sync refusal, external edits,
  malformed recovery data and rebound chords. Catalogue tests cover all Meta
  roles/presets, exact auxiliary arms and duplicate freedom.
- Native-layout fake tests cover nontrivial O→T→O frame preservation, hidden
  close, no hidden effects/focus, deletion, display removal, unreadable type,
  skipped-display parking/recovery and native row ordering.
- The input fixture intermittently submitted focused sequence 2 before worker
  startup sequence 1; this changed core admission versus focus projection and
  made grow-at-edge a legitimate no-op. Waiting for startup observation
  publication fixes the causal test race without changing the geometry oracle.
  The failing target passed 32 consecutive runs after repair.
- Retained-layout tests now distinguish superseded worker frame records from
  the current engine plan, including revision-stamped hidden-write checks.
  This repairs an asynchronous fixture race without weakening exact layout
  preservation or zero-hidden-write oracles.
- Final local macOS gates: **636 tests** (507 library plus all eight integration
  suites), strict all-target crate clippy and workspace format pass. Signed
  build and separate verification pass for both bundles and the deployment
  floor. No full-workspace macOS clippy claim (the known core portability
  blocker remains outside this change).
- Source milestone `c1655e9d` passed hosted macOS, Linux Rust/strict clippy,
  KWin, native, shell and dependency gates in
  [CI 38893345659](https://github.com/beefsack/OmniTiler/actions/runs/38893345659).
  Windows exposed an incorrect Unix-only refusal in the new transaction-lock
  helper. The causal repair uses portable standard-library locking while
  preserving Unix no-follow/private-mode safeguards; the same fake-backed
  ownership assertions run unchanged, rather than being skipped.
  Follow-up `8d445ba1` passes Windows tests too; its Windows strict clippy
  requires explicitly non-truncating lock-file opens. Both platform branches
  now state that existing lock-file contents are preserved.
  `b9338c38` passes Windows and all Linux gates; macOS exposed an existing
  ambient-PID test assumption (a supposedly mismatched PID was identical and
  happened to be alive). That test now exercises the exact pure PID/bundle
  ownership boundary; production authorization is unchanged.
- Unsafe blocks in the existing FFI module: **96 → 100** (four narrowly scoped
  dynamic hotkey SPI resolution/read/enable blocks); no unsafe elsewhere and no
  new dependency. Standard-library locking avoided an additional unsafe block.

Next: resolve the failed owned-helper fixture with a clearly unobstructed
user-arranged helper or a short physical synthetic-transfer discriminator before
building product transfer on it. Do not repeat the unchanged automated run.
Then implement selection (MC AX versus gesture remains an evidence-led choice),
digit posting, native action execution, trailing-Space lifecycle and actual
bindable send-stay configuration. Cross-display attribution/transfer and native
fullscreen remain unsupported outcomes, not guessed policy. Existing independent
domains require no core redesign.

### User-run settings smoke check (not the final workspace dogfood script)

Use only while unlocked, with disposable apps and no game/fullscreen context.
First inspect the JSON; if SPI is unavailable, stop. Run commands one at a time
and wait for each bounded launch to exit (all are shell-neutral):

```text
mise exec -- just launch -- spaces-desktop-status
mise exec -- just launch -- spaces-desktop-apply --count 2
mise exec -- just launch -- spaces-desktop-status
mise exec -- just launch -- spaces-desktop-revert
mise exec -- just launch -- spaces-desktop-status
```

Expected: Apply reports verified per-holder enabled readbacks with a saved
preimage; Revert restores unchanged owned preimages and removes the backup only
when complete. Do not manually edit those holders between Apply and Revert unless
deliberately testing external-edit preservation. On any failure retain the backup
and report JSON; do not repeatedly Apply or manually delete recovery files.
Native switch behavior itself is not proven by these settings readbacks.

The requested final real-app select/send-follow/send-stay/Ctrl+Cmd/jank procedure
cannot truthfully be supplied until those product commands work. Backlog item 1
remains active rather than awaiting final physical acceptance.

## Native settings and occlusion diagnosis checkpoint (2026-10-16)

**Incomplete: synthetic transfer remains unproven; workspace actions stay
unarmed.** This checkpoint establishes session-effective Desktop-N ownership,
not managed-workspace completion or physical shortcut acceptance.

### Accepted evidence

- Five fresh signed LaunchServices invocations performed status, Apply for two
  Desktop-N holders, status, Revert and status. Both holders changed from disabled
  to enabled with unchanged chords and successful SPI readback. The intervening
  fresh process independently corroborated both enabled Ctrl+digit chords.
- Revert restored both observed preimages, reported no failures or external
  edits, and removed the preimage JSON. Final status matched the original
  effective state. The symbolic-hotkey preferences were unchanged after Apply
  and Revert: this proves persistence across processes in the same login session,
  **not logout/reboot persistence or native chord delivery**. The empty lock file
  remains intentionally; no recovery preimage remains.
- The settings artifact SHA-256 was
  `26ffc2521deb6f3600a451a6c93298241339271621217ba889a813afb770294e`.
  The persistent probe/helper bundle IDs and designated requirements were
  verified; every leg observed an unlocked session and reaped its bounded
  display-sleep companion by exact PID.

### Transfer invariant and failed hypotheses

- The fixed helper spawn overlapped an ordinary terminal window. Moving only the
  helper through alternative desktop positions did not produce an eligible grab:
  an onscreen, display-spanning layer-20 Dock surface covered every candidate in
  the conservative CG rectangle test. No foreign window was moved or exempted.
- Explicit Mission Control closure followed by a bounded ten-second observation
  did not resolve that condition: Mission Control AX reported absent while the
  same Dock surface remained. Thus neither spatial arrangement nor waiting for a
  presumed Mission Control tail solved the fixture. No further drag attempts
  followed; **all three diagnostic runs made zero synthetic posts**.
- A separate causal fixture fault was identified: LaunchServices can publish the
  bundled helper identity after spawn, invalidating the early authorization
  snapshot. Final frame observation now re-authorizes just like per-leg setup;
  refusal still reports unknown, never frame equality. The corrected final-frame
  path has not reached live acceptance because the synthetic legs remain blocked.
- Retained source changes are limited to occluder IDs/layer/geometry in refusals,
  reporting Mission Control close shortfalls and fresh final-frame authorization.
  The unsuccessful arrangement search and overlay-wait heuristics are **not**
  retained. Occlusion, exact ownership and input-release guards are unchanged.
- The last transfer diagnostic artifact SHA-256 was
  `303b3f4964af96c4bcb315873dc1a5b40a7e55dfe1b98a382e4ddbc068870cce`;
  it includes those subsequently removed diagnostic heuristics. Its failure report
  names the exact temporary SID; separate narrow recovery reports verify removal
  after each run. Final readback shows the original single-Space layout/current,
  Mission Control absent, unchanged protected-window frames/membership and full
  foreground agreement. Helpers exited/reaped with records removed, and no probe
  or display-sleep companions remain. No Ghostty input or window mutation occurred.

### Verification and next discriminator

- Final retained tree: **637 tests** (508 library plus eight integration suites),
  strict all-target macOS crate clippy, workspace format, signed build and separate
  verification pass. Unsafe blocks remain **100 → 100**, all in `ffi.rs`; no new
  dependency or shared-core change. Final signed main SHA-256:
  `a61315d83a3cff77bd51c9725f36f2ea075bf481cc0ea74a3f76cbdfde3d1871`.
  This final trimmed artifact has no new live-transfer acceptance claim.
- **Open technical question:** does the persistent Dock CG rectangle actually
  intercept pointer input, or is it non-interactive/transparent backing? Current
  observations cannot distinguish them. First obtain a read-only unfiltered CG
  snapshot plus AX point identity before opening Mission Control, then compare
  after closure. Membership-filtered snapshots omit membership-less surfaces and
  cannot answer this. Do not add a Dock exemption or retry transfer unchanged.
- Route choices remain unchanged: synthetic title-bar drag plus native chord for
  transfer; MC AX or Dock gesture selection is not yet chosen. Create/remove stay
  MC AX. No new tentative product decision. Existing host differences remain
  Ctrl+Cmd auxiliary arms, native drag-follow and visible send-stay switch-back,
  plus explicit session-effective Desktop-N Apply/Revert rather than plist writes.
  Cross-display/fullscreen/floor acceptance and physical jank remain TBD.
- Product select/send/follow/stay, trailing lifecycle and the final real-app
  physical script remain outstanding; the settings-only procedure above is not
  that script. Keep backlog item 1 active, not awaiting final physical acceptance.
