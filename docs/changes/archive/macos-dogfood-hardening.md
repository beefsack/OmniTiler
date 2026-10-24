# macOS dogfood hardening and physical checks

## Goal and acceptance

Resolve first-dogfood directional focus, idle trace volume and outer-gap reports;
provide bounded physical timeout/Carbon probes and a short native-Space manual
transfer discriminator. Keep shared Tier 1 behavior and the selected native
Spaces boundary. Workspace transfer product bindings remain deferred.

- Diagnose input receipt, planning, dispatch and observed focus arrival separately.
- Keep real-event trace evidence; emit unchanged routine records only on change.
- Apply the shared 8/8 inner/outer defaults against each display's visible frame.
- Pass macOS regressions, strict all-target clippy, format and signed-bundle
  verification; native evidence uses scoped disposable windows.
- Physical timeout notification, Carbon delivery, shell appearance and manual
  Space-transfer acceptance are user-run checks.

## Approach and current evidence

1. Read first-dogfood logs and compare focus/gap behavior with shared policy and
   KDE/Windows. Add targeted regressions and bounded fixes.
2. Add a physical-input arm to the existing test-only callback-stall fixture;
   reuse Carbon registration and read-only Space membership diagnostics.
3. Review input/live boundaries, verify scoped focus arrival and gap readbacks,
   then publish the concise physical procedure and consistency findings.

## Outcome and verification - 2026-10-16

- **Focus:** logs show 29 forwarded actions, no missing-focus refusals, and focus
  dispatch results for three windows in one application. The early zero counters
  were idle, and the generic evidence log's zero IDs were not a failed focus read.
  Inspection found the native tile setter omitted AXRaise after SLPS front/key
  dispatch, unlike [yabai focus-with-raise](https://github.com/asmvik/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window_manager.c).
  Add that exact-window raise, with ordered dispatch/raise and partial-failure
  regressions. Shared directional selection and exhausted-edge retention stay
  authoritative. This is the root-cause candidate; same-app native arrival still
  needs the physical discriminator below. Dispatch success is not arrival proof.
- **Idle trace:** focus evidence emitted on every controller cadence and the
  heartbeat repeated unchanged counters. Both now emit on change; real events,
  decisions, failures and teardown keep their evidence. This follows the
  [Windows routine-trace precedent](windows-trace-volume.md).
- **Outer gaps:** native domains passed raw visible bounds into the core. Apply
  the existing 8-point outer margin at that handoff, preserving raw visible
  bounds for native eligibility. Inner gap stays 8; menu bar/Dock exclusion comes
  from the display's visible frame, matching the KDE/Windows adapter contract.
- **Physical probes:** `input-session --physical-stall` awaits one real exact
  chord, using the existing test-only callback stall. Only a native timeout
  counter passes timeout acceptance; synthetic recovery then checks suppression.
  The launcher's optional `--stream` shows press instructions immediately, with
  snapshot-bounded stderr reads to avoid racing duplicate output.

Scoped native checks: owned-helper focus/restore agreement in 141/110 ms, no CG
residue; owned AXRaise focus verification in 13 ms. A final 15 s Calculator-only,
input-off trace run moved its top-left to visible origin + (8,8), AX=CG; its
fixed-size resize refused honestly. Focus evidence and heartbeat each emitted
once rather than continuously; shutdown was Clean. Original foreground and
Space layout restored, disposable app cleaned; bounded display-sleep prevention
and probe processes were confirmed absent after teardown. This proves margin positioning
and idle quieting, not all four resizable-window edges or same-app focus arrival.
Those are explicit physical checks below. No unscoped run is native evidence.

Final offline gates: **552 macOS tests pass** (430 library plus integrations),
strict all-target clippy `-D warnings`, workspace fmt, shell syntax, whitespace,
and the **25-check headless launch harness** pass. Concurrent bundle/test builds
initially invalidated a rustdoc dependency artifact; serialized rerun passes,
with no source workaround. Signed probe/helper verification and deployment floor
checks pass with the persistent `OmniTiler Mac Dev` identity and certificate leaf
`040f96377aaf110a77905d453bb76681e7133319`. Executable hashes:

- Probe: `d2741a4e8b42fb6f4a645d32c949dac2b8a82ad37bc4da28ff58bb197b188c9f`
- Helper: `b10c3c031e33538fba3d84063595578f119b57436796fcac17869d5b894de22a`

Bundle IDs remain `io.github.omnitiler.macos.probe` and
`io.github.omnitiler.macos.probe.helper`. Physical timeout notification, Carbon
delivery and the transfer routes remain unaccepted until user results arrive.

## Consistency review

| Choice | Comparison and remaining differences |
| --- | --- |
| Focus oracle / actuation | Native identity acquisition follows the pinned reference routes; shared core selects direction. Added missing yabai-style raise. Same-app native arrival remains a physical check. |
| Tap-only / Carbon diagnostic | Native mechanism differs by host, preserving exact-chord intent. Carbon is not a product fallback or acceptance prerequisite. |
| Authentic / defaults | Supported focus/move, G float, O orientation and Alt grow/Shift+Alt shrink mirror KDE/Windows with Ctrl replacing Meta/Win: 34 exact chords. No persistent native writes needed for current ownership. Compatible leaves four established Ctrl+arrow conflicts native; other arms are not claimed conflict-free. |
| Input eligibility | Actual difference: macOS passes through when foreground eligibility is unknown/stale/fullscreen/out of scope; Windows consumes owned chords first and fences actions afterward. This remains surfaced for consistency review, not silently equated. |
| Classification edges | Standard documents tile; dialogs/panels/sheets float untouched. Accessory owners and elevated-level standard windows are conservative exclusions, not proven universal Tier 1 outcomes; existing discriminators remain TBD. No application-specific exceptions added. |
| Fixed windows | Shared hint-based both-axis default remains; missing bounds do not become invented fixed hints. Hintless non-resizable Calculator can clamp inside a larger tile allocation. Some macOS references float such windows instead; exact user preference remains a discriminator. |
| Preview capability gaps | Workspace/send/output navigation, sticky/maximize/fullscreen bindings, launchers and effects are incomplete relative to KDE/Windows. Native fullscreen stays untouched. This is a preview, not full Tier 1 parity. |

## Decisions

User direction 2026-10-16: macOS tentative choices remain acceptable conditional
on Tier 1 and cross-platform consistency, subject to dogfooding refinement.
Where automated live testing is hard, fiddly or unreliable, prefer user-run
physical checks. Durable wording belongs in [decisions](../../decisions.md) and
the [live guide](../../live-macos-testing.md).

## User-run physical script

Run from the repository root, unlocked and outside Secure Input. Stop any
existing `dev` session first. Accessibility consent for **OmniTilerProbe** may
appear; Input Monitoring only if requested by the selected tap. Grant through
System Settings, then rerun preflight; do not reset TCC. No Screen Recording
permission is needed. Run one probe at a time.

### 1. Setup and physical tap timeout

```sh
mise exec -- just app "OmniTiler Mac Dev"
mise exec -- just verify
mise exec -- just preflight-app
# Require ax_trusted:true and an unlocked session.
mise exec -- just launch -- --timeout 120 --stream -- input-session --route tap --stall-once-ms 1500 --physical-stall --seconds 90
```

Wait for the streamed `physical-stall-exact: press ctrl+h ONCE now` prompt with
the disposable helper frontmost. Press **Ctrl+h once**, release Ctrl immediately,
then promptly tap plain **a/j/k** for about a second (not h); do not click away.
The helper may appear frozen briefly: the one-shot stall is intentional.

Expected JSON: `physical_stall:true`, `tap_report.reenables_after_timeout >= 1`,
a `stall-recovery` leg with `helper_exact_delta:[0,0]`, and `outcome:"completed"`.
The timeout counter records a real native disable notification plus re-enable;
unnotified recovery is not a pass. `stage:"stall-disable"` means the physical
attempt did not prove a timeout: report it, rather than repeat the experiment.
The subsequent suppression check is synthetic, gated to the owned helper.

It auto-cleans; allow up to the 90 s budget plus teardown (hard backstop adds
60 s). Ctrl+C stops only the wrapper, **not** the launched probe: wait for its
bound before another run. If still stuck past that hard bound, use Activity
Monitor to quit the exact OmniTilerProbe instance; do not use name-pattern kills.
The wrapper may return nonzero because it saw the instructional stderr even on
success: read the JSON, not the wrapper exit code.

### 2. Carbon Ctrl+h delivery (retired)

Retired 2026-10-16: the diagnostic-only Carbon hotkey code and `input-register`
command have been removed; the event tap is the sole hotkey route. Do not run
this historical step. Its original instructions and 0/3 physical-delivery result
below are retained as evidence.

```sh
mise exec -- just launch -- --timeout 30 --stream -- input-register --seconds 15
```

Keep the terminal focused. At the prompt press **Ctrl+h three times**. Expected:
`os_accepted:true`, `os_status:0`, `deliveries:3` and `engine_input_events:3`.
Registration alone is not delivery proof. It releases automatically after 15 s;
same wrapper-stop caveat as step 1.

### 3. Three manual native-Space transfer routes

Keep the tiler stopped so Ctrl+Right belongs to macOS. In Mission Control
(Ctrl+Up or the trackpad gesture), reveal the top Space strip and click **+**
to create one extra Desktop on the same display. Use Desktop 1 and adjacent
Desktop 2; if Desktop 2 already existed, retain it and track only the extra
Desktop for cleanup. Do not remove pre-existing Spaces.

Open a disposable **TextEdit document in its own window** on Desktop 1 (not a
tab). These read-only commands identify its `wid`, Space IDs and enabled chords:

```sh
mise exec -- just launch -- --timeout 70 -- scope-inspect --scope com.apple.TextEdit
mise exec -- just launch -- spaces-bindings
```

Set the document's reported window ID, then use this readback before and after
**each** route (it works regardless of foreground or current Space):

```sh
# replace 123 with the disposable document's wid (shell-neutral; no variables)
mise exec -- just launch -- spaces-diagnose --wid 123
```

1. Hold the TextEdit **title bar** in a drag; while still holding, press
   **Ctrl+Right**, wait for the transition, then release. Read back membership.
2. Return the document to Desktop 1 through Mission Control. Note the current
   setting, then temporarily enable **System Settings > Keyboard > Keyboard
   Shortcuts > Mission Control > Switch to Desktop 2** with **Ctrl+2**.
   Confirm via `spaces-bindings`. Read back; drag the title bar, press Ctrl+2
   while holding, wait, release; read back again.
3. Return the document to Desktop 1. Read back; enter Mission Control and drag
   its window onto Desktop 2's thumbnail; leave Mission Control; read back.

For each: report **works?**, source/destination membership, whether focus/current
Space followed, **visual jank**, and anything odd. Expected success is the same
WID's membership changing from the source SID to destination SID, not just the
desktop switching. Return the document with the thumbnail route between legs.

Cleanup: return/close only the disposable document, restore Switch to Desktop 2
to its original enabled state and binding (disable it again if originally off),
return to the original Desktop and remove only the extra **empty** Desktop with
its Mission Control **x**. Run the `--wid` readback before closing the document and
`spaces-diagnose` afterward to confirm return/topology. Leave existing windows
and Spaces in place.

### 4. Dogfood recheck

Create two disposable TextEdit windows with distinct short text, then:

```sh
mise exec -- just dev trace
```

- Click the left document; Ctrl+Right should focus its right neighbor without
  moving either frame. Type a marker to prove which document received it;
  reverse with Ctrl+Left and repeat. Test same-app windows and another app.
  Exhausted edges retain focus; Ctrl+Shift+arrows still move/follow normally.
- Expect **8-point outer margins** on all four visible-work-area edges (menu
  bar/Dock excluded), with the same **8-point inner gaps**. Use resizable
  windows for this, not Calculator's clamped size.
- Leave input/windows unchanged for 10 s: no repeated identical focus-evidence
  or heartbeat lines. Real events must still log promptly.
- Ctrl+C once; require clean owner shutdown. Alternative stop from a second
  terminal: `mise exec -- just dev-stop`. Frames remain where tiled.

Report focus result (including markers), gap appearance, idle logging, the two
probe JSON results and the three transfer outcomes. The unscoped dogfood recheck
is user-run.

### 5. Consume-first parity (2026-10-17)

Physical acceptance is pending. This is the current Windows-matching rule, not
a final cross-platform policy: armed owned chords suppress even when their
actions are refused. Do not test in a game or type test keys into the terminal.
Use only disposable Calculator/TextEdit windows, focused by clicking them.

Run setup from step 1, but **do not repeat its stall probe**. Require
`ax_trusted:true`, an unlocked session and `secure_input:Inactive`. Record the
disposable windows' original frames/current Space. Create a disposable TextEdit
marker with the cursor at its end, and one Calculator window. Keep these windows
on the same existing Desktop for the first leg.

```sh
mise exec -- just tile --user-start --scope com.apple.calculator --input tap --preset authentic --seconds 60
```

Once the run is active, click TextEdit and press **Ctrl+H once**. Require the
marker unchanged, no focus/layout/Space change, and this stderr line:

```text
tiler-macos tile-tap: consumed-refused-basic (consumed-refused-basic=1 consumed-refused-select=0)
```

After bounded teardown, click the same document and press **Ctrl+H once**:
exactly one marker character must now delete, proving native delivery has
returned. Plain `h` is unarmed and should type normally; undo that test edit.
Restore the marker and verify both windows' baseline frames/current Space before
continuing. The refusal log is first/every-50th per category; terminal JSON has
tap suppression counts but **no refusal totals**. Instruction/refusal stderr can
make the wrapper exit nonzero: inspect `outcome` and `shutdown_clean`, not just
the wrapper status. Wait for the bounded child teardown; Ctrl+C alone is not proof.

For the isolated fullscreen leg, fullscreen only the disposable TextEdit window
using its green button, then run:

```sh
mise exec -- just tile --user-start --scope com.apple.TextEdit --input tap --preset authentic --seconds 60
```

Click the fullscreen document, leave the cursor at the marker end, and press
**Ctrl+H once**. Require `consumed-refused-basic`, marker unchanged, no tiling,
focus or Space change. After teardown, exit fullscreen using its green button;
verify the original Desktop, document membership and frame are restored. This
tests a native fullscreen foreground, not a game or anti-cheat environment.

For workspace coverage, reuse the
[select-only procedure](../macos-managed-workspaces.md#select-only-fallback-handover-2026-10-17)
and its Apply/Revert/owned-Space cleanup. **Changed expectation:** on the empty
Space, Ctrl+H is consumed/refused (not passed through), with no stale-focus
actuation; armed numbered/relative selects still navigate. An ineligible select
also suppresses first and reports `consumed-refused-select`.

Lock, Secure Input or unknown session state release interception rather than
refuse after suppression; stop/report if those gates appear. Unarmed chords
always retain native delivery. Close only disposable documents/windows, undo
test text, verify baseline geometry/membership and leave existing Spaces/settings
untouched. These two basic/fullscreen legs do not Apply settings, so require no
Revert. Report changed/unchanged text only, not document content.

### 6. Active-border appearance check (pending, 2026-10-17)

Use two disposable TextEdit documents; note their starting frames and Space.
Leave unrelated windows, existing Spaces, settings and permissions unchanged.
Build/verify the signed app and require `ax_trusted:true` from preflight:

```sh
mise exec -- just app 'OmniTiler Mac Dev'
mise exec -- just verify
mise exec -- just preflight-app
mise exec -- just tile --user-start --scope com.apple.TextEdit --input off --preset authentic --seconds 60
```

Focus A then B: visually judge whether one thin accent-coloured ring hugs the
focused frame. Move and corner-resize it; judge perceived tracking, width, colour
and rounded corners. Native Space switching, fullscreen entry/exit and synthetic
border-region input delivery already have live fixture evidence; no repeat is
needed for this appearance check. Do not infer results after the 60-second
lifetime.

Wait for the clean bounded outcome and verify the ring is gone. Restore the
documents' baseline frames/membership, then close only those disposable documents.
No settings were applied, so no Revert is required. Report appearance and
perceived tracking only; API readbacks are not a visual verdict. Real games,
stacking variants and multi-display checks remain separate follow-ups.

## User physical results 2026-10-16

User-run on one display, following the script above.

| Check | Observed | Verdict |
| --- | --- | --- |
| 1. Physical tap stall | Exact Ctrl+h matched and suppressed (2/2); during the 1,500 ms stall the OS disabled the tap but delivered **no** timeout notification (`unnotified_disables=1`, `reenables_after_timeout=0`); the enabled-state health check detected it and verified same-tap re-enable. Probe reported `stage:"stall-disable"` and refused to claim notification recovery; foreground restored to the original app. The helper's audible beeps on plain letters are expected (it has no text input). | Disable plus health-check recovery observed; timeout notification not delivered. Product route accepted tentatively ([decision](../../decisions.md#scope-and-platform-goals)) |
| 2. Carbon Ctrl+h | Registration accepted (`os_status:0`), three physical presses, `deliveries:0`. Likely cause (unverified): the diagnostic does not run the event loop Carbon dispatch needs. | Not delivered; Carbon is diagnostic-only, not investigated further |
| 3. Manual Space transfer | All three routes moved the window to Desktop 2 | [Route results](../../research/macos-port/native-spaces-routes.md#user-manual-transfer-check--2026-10-16) |
| 4. Dogfood recheck | Three terminal and two TextEdit windows: directional focus (including same-app) and move worked as expected, outer and inner gaps correct, idle log no longer repeats | Pass |

Follow-ups: the `just launch` wrapper exits nonzero even when the probe JSON
reports success; user procedures must be shell-neutral (avoid POSIX-only
variable assignment and functions).
