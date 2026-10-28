# Windows physical and user-run checklist

- Scope: user-owned physical/native acceptance on a Windows 11 x64 host. Read the [live Windows testing guide](../live-windows-testing.md) before any run. No new implementation is selected by this checklist.
- Status: partial evidence updated through user physical 2026-10-20. No complete row is claimed passed. A row citing a gate ID covers that check partially unless the pass criterion says otherwise.
- Gate: approved [Windows 0.1 gate proposal](windows-0.1-gate-proposal.md) D1-D4 (45 base must-pass / 7 known-issue-allowed / 5 excluded + WX1-WX8 must-pass). Excluded base checks 5, 35, 38, 40, 53 are absent below.
- World route: the v2 world route is delivered and fixture-accepted per the [core closeout](archive/core-world-model-rearchitecture.md#closing-outcome-2026-10-24) (core migration authority with member/view/refill settlement, T2 unchanged-source refusal, atomic cutover, default local migration). The W2M-03/W2M-12 physical matrix below is unrun; fixture evidence is not physical acceptance. Do not transfer legacy-owner evidence automatically after cutover.
- Third-output caveat: two-candidate topology discrimination can require three outputs, not merely two (see W2M-01).
- Removed, not tested: Underlay C (unfocused dragged subject) stays parked; Win+drag is removed, native title-bar drag remains (see WSK-09).
- Deferred policy excluded: D27 5b `Activation request` setting is post-0.1, not a runnable 0.1 check (see WGM-03). Installed lifecycle is blocked on the undecided package/update channel (see WDS-15).
- Limitation note (not a test row): the Authentic Win+G/Win+F11 exact-chord containment gap is carried as a listed limitation under D4; Compatible chords are still fully checked (see Games).
- Newest rule: the 2026-10-17 proof-only conflict rule (warn only on provably present conflict) supersedes older unknown-state warning plans (see WSK-08, WDS-03).

## Batched physical run sheet 2026-10-29

- Use this section for the [batched backlog item](../backlog.md#open-work); the tables below remain the wider acceptance inventory, not extra mandatory steps. All checks below are UNRUN; documentation verification is not physical acceptance.
- Follow the [live Windows guide preflight/recovery](../live-windows-testing.md#required-preflight): retain source/artifact identity, original tiler settings, display/work-area/scale baseline and the tested stop/restore route outside the repository. Ordinary medium integrity only. Keep mouse/Task Manager access available; stop on unexpected effects or uncertain restoration.
- Order: S1-S4 share two fixture windows and one output; S5 changes presets; S6 alone needs real apps; D1 and the optional D2 legs share two outputs. Stop/restore before changing scope or display setup.
- Never press Win+D, Win+T, bare Win+G or bare Win+F11 anywhere below. S5 uses only the Compatible Ctrl+Win+G / Ctrl+Win+F11 alternates plus Authentic inspection without bare-chord presses. No registry/policy writes, games, downloads, elevated apps, secure-desktop or forced-loss probes. Preserve unrelated windows.
- Report each check as PASS/FAIL plus its requested observation; use BLOCKED (with condition) or NOT RUN instead of guessing. Send the run-log filenames and ordered result lines back; no titles, app content or raw keystroke logs.

### Session setup (once, three terminals, repo root)

- T1 fixture server: `mise exec -- cargo run --locked -p tiler-live-fixture -- serve`; keep the single announcement JSON line (pipe name + nonce). No `--dir` needed on Windows; the pipe is kernel-namespace, nothing to delete (see [fixture README](../../crates/tiler-live-fixture/README.md#windows-powershell-start)). `shutdown` exits the server, so Block D re-runs this line for a fresh announcement.
- T2 fixture client, ready to paste:
```powershell
$ann = (Read-Host 'Paste announcement JSON') | ConvertFrom-Json
function Send-Fixture([string]$Json) { mise exec -- cargo run --locked -p tiler-live-fixture -- send --socket $ann.socket --nonce $ann.nonce $Json }
Send-Fixture '{"v":1,"request_id":"spawn1","op":"spawn","windows":[{"id":"a"},{"id":"b"}]}'
Send-Fixture '{"v":1,"request_id":"get-s0","op":"get","ids":["a","b"]}'
```
`--nonce` is auto-injected into payloads that omit it; every `request_id` must be unique per call, so change the suffix each time. Read back only with `get`/`list` (observed predicates, never setter echoes; `list` returns ids only).
- T3 tiler with trace capture: `mise exec -- just dev trace --scope-exe tiler-live-fixture.exe` (`dev` adds `--user-start`/`--trace`). Logs and provenance sidecars land under `%LOCALAPPDATA%\omnitiler\session-1\`; retain each block's pair.
- Tray Settings > Shortcuts: apply Authentic, then rebind `toggle-float` to `Ctrl+Win+G` and `toggle-fullscreen` to `Ctrl+Win+F11`, Apply. Call this the **working binding setup**. Do not press the bare Xbox chords. Compatible alone is NOT suitable for S1-S4/D: it disables Win+digits, arrows and Win+O. Verify keyboard takeover on, workspace scope local, workspace tiling on, switch target cursor, focus-follows-cursor off and cursor-warp off; preserve/restore the originals.
- Stage WS1 with only A/B tiled and WS2 empty (pager plus trace member/view rows). If admission put A/B elsewhere, click each and press Win+Shift+1 (numbered send-follow to WS1), then Win+1. Fixture `get` has no workspace/output field. Stop if this layout cannot be established; do not involve other applications.
- Focus-sensitive steps: run the entire key sequence without returning to a terminal. Confirm focus visually and from the trace's foreground/target rows BEFORE any terminal click. `get` after clicking T2 can check geometry/visibility, but cannot prove the preceding foreground. Re-click the specified fixture before the next sequence.
- Teardown per block: T2 `Send-Fixture '{"v":1,"request_id":"bye1","op":"shutdown"}'`, T3 `mise exec -- just stop`, verify restored baseline and clean ledger.

### Block S (one active output; working binding setup)

- S1 Alt-Tab x10 among fixture windows plus non-Alt-Tab controls (see [return-focus record](windows-workspace-return-focus.md#attribution-evidence-existing-versus-available-mechanisms)).
  - Setup: WS1 A/B tiled. Click B, then A so the most recent two windows are fixtures.
  - Keys: press Alt+Tab and release both keys, 10 times. If the switcher would select a foreign window, keep Alt held and use Tab to select A/B before releasing; do not activate unrelated apps. Controls: Win+1 workspace reselect, Win+Right then Win+Left, click the other fixture once.
  - Expected: each completed switch lands on the selected fixture; controls remain distinguishable from Alt-Tab in the user's ordered notes.
  - Capture BLOCKED: `EVENT_SYSTEM_SWITCHEND` is not subscribed. No current command enables it, and ordinary Alt-Tab has no raw-key trace. Existing foreground rows are supplementary only, not SWITCHEND capture or causal proof. The 10 physical switches can still be recorded, but cannot close the requested capture check.
  - Capture available: keep the normal trace log from T3, identifying S1 and the three controls in the report.
  - Result: UNRUN - record PASS/FAIL + the per-release landing list, naming any non-fixture landing.
- S2 wake-attribution same-tick during spam (per-pass `wake-attribution` carries the tick; the same tick's `world-apply` row carries the consumed command kind).
  - Setup: WS1 A/B; WS2 empty; click A.
  - Keys: alternate Win+2 / Win+1 rapidly for 10 roundtrips at physical rate, no deliberate waits.
  - Expected: remembered focus returns; join wake cause sets to same-tick commands only where both rows are present. Suppressed detail rows and quiet passes with no rows are expected logging behavior, not failure.
  - Capture: trace-log `wake-attribution` plus `world-apply` rows.
  - Result: UNRUN - record PASS/FAIL + same-tick join YES (paired passes matched) / NO (mismatch at tick T) / NOT OBSERVED (no paired rows in this run).
- S3 empty-workspace return WS1 -> empty WS2 -> WS1, with desktop-foreground leg (see [return-focus record](windows-workspace-return-focus.md); decided stay-on-empty focus is not yet implemented, so record actual behavior).
  - Setup: WS1 A/B tiled, WS2 empty; click B.
  - Keys: Win+2, wait until both fixture windows disappear, Win+1; repeat 3 times slowly, without clicking a terminal between keys.
  - Expected: departure hides A+B; return restores remembered B with an accepted setter and foreground proof. If a Progman/desktop-foreground veto refuses instead, record the refusal branch; do not force or retry into it.
  - Exact-desktop leg: after Win+2, click visible blank desktop space (never Win+D), then Win+1. Clicking is an attempt, not exact-shell identity proof. Success requires `desktop_exception:true`, an accepted setter AND B visibly/trace-proven focused before any terminal click. A successful ordinary return without that marker does not exercise the desktop path. If no blank desktop is exposed, leave real windows untouched and mark this leg BLOCKED.
  - Capture: T3 trace's `world-apply` focus dispatch/foreground rows. On failure preserve `refusal_gate`, `fg_veto_kind` and available gate facts; do not force/retry around the veto.
  - Result: UNRUN - record PASS/FAIL + desktop marker/setter/focus outcome (or blocker/veto branch); separately state whether the three ordinary returns restored B.
- S4 floating-window workspace roundtrip (D19/R-FLT-13: float hides while away, frame retained, remembered focus restored).
  - Setup: working binding setup; WS1 A/B; click B and note its current frame.
  - Keys: Ctrl+Win+G (float B), Win+2, Win+1.
  - Expected: B floats with unchanged frame, hides on WS2, returns to the same float frame on WS1 with remembered focus.
  - Capture: T3 trace focus/visibility/frame rows; optional post-sequence geometry snapshot: `Send-Fixture '{"v":1,"request_id":"get-s4","op":"get","ids":["a","b"]}'`. Click B and Ctrl+Win+G once to retile it afterwards.
  - Result: UNRUN - record PASS/FAIL + actual focused window after return. Frame retention/hide failure also makes this check FAIL.
- S5 pending Xbox shortcut conflicts (backlog P1 Xbox item; plan [gaming coexistence](windows-gaming-coexistence.md#physical-acceptance-plan-pending); covers WSK-07/WSK-08).
  - Setup: Settings > Shortcuts > Compatible, Apply; close Settings and click tiled A. For each trial restore A to ordinary tiled/windowed state with the same safe toggle before continuing.
  - Keys: Ctrl+Win+G tap, release; tap again to undo. Repeat with Ctrl+Win+F11. For each chord separately: hold 2 s then release; repeat releasing Ctrl before the action key; repeat releasing Win before the action key; repeat with Win pressed before Ctrl. Release all keys between trials. Plain Win should open Start; dismiss by clicking the fixture.
  - Expected: one toggle per discrete press; no Game Bar/Xbox surface; no stuck keys or late activation.
  - Authentic leg: select Authentic, Apply; inspect tray/conflict rows for float and fullscreen. Warnings only for positively present Game Bar package / configured GamingHomeApp conflict; absent/unknown/failed probes show ordinary no-known-conflict text. Use available Xbox-mode Settings and Game Bar Settings actions: check the intended page/app identity; missing/redirected pages must leave the conflict unresolved. Do not change OS settings or package state; Recheck must not treat navigation alone as remediation. Disable each binding and Apply, then rebind both back to the working setup and Apply: the corresponding active conflict clears. Never test old bare chords physically. Actual-state inspection does not prove all absent/failed probe branches.
  - Capture: T3 log plus the actual package/configuration evidence shown in Settings; restore the working setup for subsequent blocks (original settings at final teardown).
  - Result: UNRUN - record PASS/FAIL + the single worst deviation (repeat toggle, stuck key, prompt, or unexpected warning state).
- S6 launcher geometry (WSA-01; plan [launcher fighting](windows-launcher-geometry-fighting.md#physical-check-plan-pending); backlog P1 launcher item. Fixture cannot substitute; needs real apps and inventory-verified scope).
  - Setup: perform the fixture teardown above. Before starting any new owner, open only the disposable Notepad/Paint documents and the four named launchers for identity discovery; no games/downloads/settings changes. Resolve exact visible top-level exe basenames with the read-only inventory (no titles). Use `children` only if inventory identifies a shared host; replace `<HWND>` with that verified window's numeric handle:
```powershell
mise exec -- cargo run --locked -p tiler-windows -- inventory
mise exec -- cargo run --locked -p tiler-windows -- children --hwnd <HWND>
```
Fill the four launcher basenames from inventory, never guesses. Close only these newly opened test windows before the timed journey. Replace every placeholder below before running:
```powershell
mise exec -- just dev trace --scope-exe <LauncherA.exe> --scope-exe <LauncherB.exe> --scope-exe <LauncherC.exe> --scope-exe <LauncherD.exe> --scope-exe notepad.exe --scope-exe mspaint.exe
```
S6 is BLOCKED until all names are verified and no unrelated windows share their scope. A shared host also requires a verified `--scope-host-child HOST=CHILD` restriction per the live guide; do not broadly admit that host. If identity cannot be isolated, park this check. Never run bare `just dev trace` (empty scope manages everything).
  - Keys/steps (working bindings): cursor on the sole output, Win+1; open Notepad then Paint with disposable documents, click Paint, settle 10 s, physically drag a split through Paint's feasible minimum and back. Win+2; open Steam, Battle.net, Epic Games Launcher, Xbox in that order, 10 s apart, with the last opened launcher focused. Drag one split smaller/back; idle 10 s; Win+1 then Win+2. Close only one test launcher window, inspect the remaining three; Win+1 and repeat the control resize. If fighting begins, capture at most 10 polling ticks, then `mise exec -- just stop`.
  - Expected: no unsolicited repeated writes; minimum redistribution correct; quiet idle; genuine hint changes applied; `window-diag` batches (normal level) show correlated plan/effective/readback.
  - Capture: T3 trace, including normal-level `window-diag` sampled batches and trace `min-hints-detail` / `plan` / `write` / `readback-detail`. Keep `omitted`/`coalesced` counts; sampled rows are not every-poll proof. Stop/restore, close only test-owned windows, verify baseline before Block D.
  - Result: UNRUN - record PASS/FAIL + whether any unsolicited repeated-write burst occurred.

### Block D (two outputs; one re-layout)

- D0 setup: with the prior owner stopped/restored, enable two horizontally adjacent outputs O1 (left), O2 (right), with overlapping vertical spans. Keep the display baseline for restoration. No third output required. Restart T1 with the same `serve` command; in T2 re-run `$ann = ...` with its NEW announcement. Restart T3 with `mise exec -- just dev trace --scope-exe tiler-live-fixture.exe` and restore the working binding setup, local workspaces, cursor switch target, FFC/warp off.
- Put the terminals/cursor on O1, select Win+1, then T2 `Send-Fixture '{"v":1,"request_id":"spawn-d1","op":"spawn","windows":[{"id":"w1"},{"id":"w2"},{"id":"w3"}]}'`. Verify all three admitted on O1 (otherwise use native title-bar moves on these fixtures only to stage them there). Click w2, press Win+Ctrl+Alt+Right; click w3 on O1, press Win+Ctrl+Alt+Right. w1 stays O1; w2/w3 follow onto O2's current workspace.
- On O2, if w2/w3 stack vertically, click w2 and press Win+O ONCE to make a left/right split. If w3 is leftmost, click w3 and press Win+Shift+Right once to put it right of w2. Required end state: `O1=w1`, `O2=H[w2 w3]` (w2 left, w3 right), ordinary tiles, no overlays. Confirm visually and in trace member/view/allocation rows before D1; do not treat fixture snapshots as workspace membership proof.
- D1 cross-output edge focus (spec [edge discriminator](../spec/reference-outcomes.md#cross-output-focus-edge-discriminator); offline D1/D2 in [edge focus](archive/cross-output-edge-focus.md); user case O1=W1 O2=H[W2 W3], W3 last focused).
  - Setup: click w3 and verify active focus, then directly click w1 and verify active focus (O2 last focused w3). Keep the pointer still; no terminal clicks during this sequence.
  - Keys: Win+Right once from w1.
  - Expected: focus selects w2 (edge-nearest per D1/D2), not destination-MRU w3.
  - Capture: T3 directional/focus outcome plus foreground rows, including w3 -> w1 setup and the crossing; confirm w2 visibly focused before touching a terminal.
  - Result: UNRUN - record PASS/FAIL + which window (w2 or w3) actually received focus.
- D2 is **optional**, not a complete P0 dual-output matrix. Run in order below if convenient; omit sleep/wake, hotplug, Explorer restart, owner loss, scope-mode permutations and third-output discrimination from this short session.
- D2a cursor-first placement recheck (backlog P0 dual-output remainder).
  - Setup: after D1, leave w1 focused on O1 by clicking it; use T2 on O2 to prepare the command below, leave the cursor on O2 while w1 remains the last focused managed window. New-window placement must be COSMIC cursor-output/last-focused mode.
  - Keys: T2 `Send-Fixture '{"v":1,"request_id":"spawn-d2","op":"spawn","windows":[{"id":"w4"}]}'`; wait for admission.
  - Expected: newcomer w4 admits on O2 (cursor-first default).
  - Capture: T3 admission rows plus visible placement; afterwards close only w4 with `Send-Fixture '{"v":1,"request_id":"close-d2","op":"close","ids":["w4"]}'` to restore the D1 layout.
  - Result: UNRUN - record PASS/FAIL + the output w4 actually admitted on.
- D2b physical pager selection (W2M-13 selection remainder only).
  - Setup: D1 layout restored, pager enabled on each output, local workspace scope.
  - Keys: click O1 pager's empty-workspace cell, then its occupied cell to return; repeat on O2. Do not press a send while the empty view has no managed focus.
  - Expected: each pair hides/reveals only that output's fixtures and leaves the other output's selected view unchanged. This does not prove the still-unimplemented stay-on-empty focus policy.
  - Capture: visual switch; trace workspace rows.
  - Result: UNRUN - record PASS/FAIL + switched-only-own-output YES/NO per pager.
- D2c no-focus navigation (backlog P1 remaining proof; [delivered behavior](archive/directional-focus-without-window.md), tentative D3 span policy).
  - Setup: D1 layout restored. Cursor O1, Win+2 to its empty view. Click visible blank desktop on O1 so no managed window is focused; if unavailable, BLOCKED without disturbing real windows. Trace must establish no managed foreground before the tested key; otherwise this is not the no-focus path.
  - Keys (i): Win+Left (no output to O1's left). Expected: no-op, no local recovery or forced focus. (ii) From the same no-focus O1 origin, Win+Right toward O2. Expected: crossing selects edge tile w2, not w3. Cursor O1, Win+1 restores w1's view afterwards.
  - Capture: T3 no-subject directional dispatch and resulting foreground rows; verify target before any terminal click.
  - Result: UNRUN - record PASS/FAIL per leg + actual outcome (no-op confirmed / arrival target).
- D2d empty-singleton/reverse-DPI native drag (W2M-06 remainder).
  - Setup: D1 layout restored. Cursor O2, Win+2 selects an empty destination; click singleton w1 on O1.
  - Keys: drag w1 by its native title bar into empty O2 and drop (no Win modifier). Then drag w1 back from O2 into empty O1; if scales differ this is the reverse-DPI-start leg, otherwise mark that subleg NOT RUN. Start another cross-output title-bar drag and press Esc before dropping.
  - Expected: singleton drops occupy the empty destination with matching preview/drop and source emptied; reverse start stays a move, not an erroneous resize; Esc snaps back without membership change.
  - Capture: T3 drag dispatch/START-probe and member/frame rows; no synthetic-input driver.
  - Result: UNRUN - record PASS/FAIL per subleg + preview/drop agreement or erroneous resize (cancel: snap-back YES/NO).
- Final teardown: T2 `Send-Fixture '{"v":1,"request_id":"bye-d","op":"shutdown"}'`; `mise exec -- just stop`; restore original tiler/display settings, confirm no owner/hooks/overlays, all test-hidden windows revealed and clean recovery ledger. Preserve failures/residue; no broad cleanup.

### Command/binding/expectation provenance

- Fixture `serve` + PowerShell `send` helper and `get`/`list`/`shutdown` ops: [fixture README](../../crates/tiler-live-fixture/README.md#windows-powershell-start) and its command reference/protocol sections.
- Dev/stop recipes: [windows.just](../../just/windows.just), [argument assembly](../../scripts/windows-dev.ps1); scope and inventory/children grammar: [CLI](../../crates/tiler-windows/src/main.rs), [scope parser](../../crates/tiler-windows/src/tiling/cli.rs), [title-free inventory](../../crates/tiler-windows/src/tiling_sys/commands.rs).
- Same-tick attribution, cause sets and filtered-detail suppression: [run_loop_wait.rs](../../crates/tiler-windows/src/tiling_sys/run_loop_wait.rs). Quiet/suppressed rows are not failed commands; no inference of one unique waker.
- Win+arrows/Shift-move/Ctrl+Alt-send: [snapkey routing](../../crates/tiler-windows/src/snapkey/mod.rs); Win+digits and Shift+digits: [binding catalog](../../crates/tiler-windows/src/settings/catalog.rs). Win+O retains child order: [orientation record](archive/windows-parent-orientation-toggle.md). Compatible disables navigation/digits/orientation and rebinds float/fullscreen: [presets.rs](../../crates/tiler-windows/src/settings/presets.rs).
- Missing SWITCHEND subscription: [run_loop.rs](../../crates/tiler-windows/src/tiling_sys/run_loop.rs), [wake filter](../../crates/tiler-windows/src/tiling_sys/observe.rs), [attribution limits](windows-workspace-return-focus.md#attribution-evidence-existing-versus-available-mechanisms). Desktop marker is gate proof, not landing: [implementation/acceptance](windows-workspace-return-focus.md#implementation-and-accepted-evidence).
- Compatible Ctrl+Win+G / Ctrl+Win+F11 hold/release-order plus Authentic navigation/refresh steps: [gaming coexistence](windows-gaming-coexistence.md#physical-acceptance-plan-pending). Launcher control/four-app steps: [launcher fighting](windows-launcher-geometry-fighting.md#physical-check-plan-pending).
- Edge-nearest w2: [spec discriminator](../spec/reference-outcomes.md#cross-output-focus-edge-discriminator); float hide/return: [D19 inventory](../backlog.md#open-work), [REQ-FLT-13](../spec/functional-spec.md). Optional placement/drag/pager legs: [dual-output remainder](windows-dual-output-live.md#remaining-units); these do not close entire W2M rows.

### Limitations (explicit, not proven here)

- SWITCHEND capture is blocked on an unimplemented subscription; S1 keeps foreground rows as supplementary evidence only.
- Fixture-only scope proves nothing about real launchers; S6 stays BLOCKED until inventory-verified basenames fill the scope.
- The S3 desktop-forced leg is BLOCKED wherever covering windows hide the desktop; nothing is moved or hidden to expose it.
- The decided stay-on-empty focus behavior is not yet implemented; S2/S3 record actual behavior against the decision without claiming it.

## Single-monitor keyboard/shortcuts

| ID | Action | Pass criterion | Gate IDs | Source |
| --- | --- | --- | --- | --- |
| WSK-01 | Press physical Win+Y on tiled and floating workspaces, both toggle directions; check tray checkbox sync; hold/release/repress; test Compatible project toggle inert and record actual Mixed Reality/OS behavior; Settings Apply/Revert, presets, rebind/disable, Snap-off release | Both-way toggle, tray in sync, other workspace and saved default unchanged, one toggle per discrete down | 20-M (partial) | [record](archive/windows-workspace-tiling-shortcut.md) |
| WSK-02 | Press physical Win+arrows/HJKL to focus; press physical Win+Shift+arrows/HJKL to move; verify follow/stay workspace sends separately | Focus moves without moving windows, Win+Shift moves with local-first ordering, no stuck consumption or OS leakage | 15-M (partial), 46-M (partial) | [sends](archive/windows-workspace-send-follow-stay.md) |
| WSK-03 | Toggle same-axis move between group-with-neighbor and swap-with-neighbor; press physical Win+O on root and nested parent twice | Live reread without tree rebuild; grouping nests the pair, swapping exchanges with shares traveling and focus on mover; lone window no-op | 16-M (partial), 46-M (partial) | [same-axis](archive/windows-same-axis-move.md), [orientation](archive/windows-parent-orientation-toggle.md) |
| WSK-04 | Press physical Win+Ctrl+Tab toggle twice and Win+Ctrl+arrows/HJKL relative steps on one output; check remembered-B return and focused overlapping-float raise | Two-view toggle and ring wrap correct, output-focus-alone records nothing, remembered focus returns | 14-M (partial) | [record](archive/windows-workspace-history-ring.md) |
| WSK-05 | Press physical Win+M maximize/restore, then each directional focus chord on maximized B; repeat clear-then-move and eligible maximized-float half-snaps | Focus fenced until unmaximize, clear-then-move performs ordinary move, refusal logged without structural move | 1-M (partial), 6-M (partial), 18-M (partial) | [record](archive/windows-maximized-directional-navigation.md) |
| WSK-06 | Press physical Win+Alt+H/J/K/L and arrows to grow and Win+Alt+Shift variants to shrink; test aliases; repeat presses for the 12/14/16/18/20 schedule; try outward R-RSZ-02/03 edges, nested splits, minimum clamps and maximized/fullscreen native refusals | Actual accelerating schedule observed, outward edge no-ops, nested resize changes the nearest matching-edge-axis split only, clamps and refusals logged without writes | 6-M (partial) | [record](archive/windows-keyboard-resize.md#goal-and-acceptance) |
| WSK-07 | Select Compatible, apply, physically press/hold/release Ctrl+Win+G and Ctrl+Win+F11 in each modifier order; test plain Win and takeover-off chords | Single tiler toggle each, no Game Bar/Xbox prompt or mode, plain Win opens Start, unowned chords stay native | WX1-M (partial), 6-M (partial), 9-M (partial) | [gaming plan](windows-gaming-coexistence.md#physical-acceptance-plan-pending) |
| WSK-08 | Select Authentic with Game Bar present, absent, unknown and query-failed states; inspect tray warning, conflict rows, Settings navigation and Recheck; apply presets, rebind and disable chords, exercise pending-chord switches and invalid input | Warning and rows only on provably present/configured conflict; navigation never claims remediation; old chord released, new chord adopted, stale edits refuse | WX1-M (partial), 22-M (partial), 46-M (partial), 12-M (partial) | [gaming plan](windows-gaming-coexistence.md#physical-acceptance-plan-pending), [dialogs](archive/windows-shortcut-settings-dialogs.md) |
| WSK-09 | On the post Win-drag removal build (`6308c722`): test physical shortcut delivery and Start masking with held/released left and right Win keys; fast physical Esc and preheld/modifier ordering on native title-bar drag | Keyboard chords deliver once with correct masking, no removed Win-drag containment obligation remains, native drag/Escape/recovery preserved; injected evidence claims no physical delivery | 17-M (partial) | [removal](archive/windows-win-drag-review.md#implementation-evidence-2026-10-18) |
| WSK-10 | Press physical Win+B and Win+T under Authentic then Compatible | Authentic launches browser/Terminal, Compatible inert, missing Terminal logs and continues | 4-W (partial) | [record](archive/windows-launcher-shortcuts.md) |
| WSK-11 | Repeat maximize/native-restore/repress and sticky/native-unstick/repress in both orderings and tiled/floating origins; physically hold/release/repress; resize with takeover off and rebound chords | One toggle per intended activation, repeat/release pairing intact, old chords released, no stuck consumption or OS leakage | 6-M (partial), 46-M (partial) | [gate](windows-0.1-gate-proposal.md#original-checks-1-53), [resize](archive/windows-keyboard-resize.md#failures-discoveries-and-remaining-acceptance) |

## Single-monitor apps/journeys

| ID | Action | Pass criterion | Gate IDs | Source |
| --- | --- | --- | --- | --- |
| WSA-01 | Tile Notepad+Paint control, settle 10 s, physical resize through Paint minimum and back; open Steam, Battle.net, Epic, Xbox in order 10 s apart with trace and window-diag normal; physical resize smaller/restore, idle 10 s, switch away/back; remove one launcher, revisit three, repeat control | No unsolicited repeated writes, minimum redistribution correct, focus stable, quiet idle, genuine hint changes applied | WX8-M (partial), 10-M (partial), 13-M (partial) | [launcher plan](windows-launcher-geometry-fighting.md#physical-check-plan-pending) |
| WSA-02 | Observe Steam/Firefox/UWP admission; test Calculator negative control, Settings restart anchor, partial-zero/either-axis hints, fixed/max intersections and hung hints | Predicate and fixed-origin correct, automatic floats get no geometry/focus/stacking/keep-above writes | 4-W (partial), 10-M (partial), 13-M (partial), WX8-M (partial) | [fixed admission](archive/windows-fixed-size-admission.md), [comparison](../research/windows-port/reference-wm-comparison.md) |
| WSA-03 | Launch Paint under both born-maximized settings; repeat browser/session-restore and fixed variants | Subsequent-admissions-only semantics, overlay parity holds | 18-M (partial) | [record](archive/windows-born-maximized-setting.md) |
| WSA-04 | Exit app-native, born-after-start, borderless and main-window fullscreen; test automatic-retirement and explicit-clear recovery | Managed-slot return, stable siblings, no unsolicited writes | 9-M (partial) | [record](archive/windows-fullscreen-exit-acceptance.md#decisions-and-remaining-evidence) |
| WSA-05 | Numbered/relative WS1-WS2-WS1 sends: ordinary FLOAT and float+maximized/float+fullscreen combos with follow/stay, sole-source and empty-target legs; separately verify ordinary TILED sends | Float combos: membership-only, neither tree reflows, no mover/survivor/overlay writes, destination overlay exit restores float frame. Ordinary tiled sends: tiled sides reflow normally with exact source/destination frames | 15-M (partial), WX8-M (partial) | [record](archive/windows-d16-floating-transfer.md#pending-physical-acceptance) |
| WSA-06 | Minimize/reflow/restore, resize-skewed rejoin, sibling close, swap, gaps/resolution change, restart, minimized close | Same side/group/index where possible, session-only lifetime, no restore focus write | 27-M (partial), WX8-M (partial) | [record](archive/windows-d23-restore-close.md) |
| WSA-07 | Convergence remainder: close/reopen focus, transient unreadable frames, minimize/fullscreen/maximize restoration, rapid commands | Converges without stale writes or unsolicited focus changes; retain exact lifetime and current-observation fences | 27-M (partial) | [gate](windows-0.1-gate-proposal.md#original-checks-1-53), [backlog](../backlog.md#pending-live-checks) |
| WSA-08 | Intentionally float B, natively maximize, toggle float off once; repeat ordinary nonmax unfloat | Maximize clears before fresh admission, focus and frame correct | 7-M (partial) | [record](archive/windows-maximized-intentional-unfloat.md) |
| WSA-09 | Exercise D7 fixed-origin tile-override markers across owner restart: hydrate before admission, toggle, close, hint loss | Markers confer no write/recovery authority; override survives restart, clear is reversible | 8-M (partial) | [record](archive/windows-fixed-size-admission.md) |
| WSA-10 | Disable then enable tiling with fixed, ordinary, intentional/sticky and maximized windows present | Current-hint classification correct, intentional/sticky/explicit-tile intent preserved | 10-M (partial) | [record](archive/windows-fixed-size-admission.md) |
| WSA-11 | Retile a floating workspace with maximized A beside ordinary B, then natively unmaximize and remaximize | A stays maximized over reserved slot, B takes its share, unmaximize lands in slot, quiet settlement | 19-M (partial) | [record](archive/windows-maximized-floating-retile-overlay.md) |
| WSA-12 | Same-output native title-bar drop, zero-move, self/centre/outside cancel, group-edge placement; feel the physical preview follow and press fast physical Esc | Drops place correctly, preview tracks the pointer, Esc/zero-move cancels with snap-back, no stale show | 25-M (partial), 26-K (partial) | [drag](archive/windows-mouse-drag.md) |
| WSA-13 | Fresh exhaustion with largest-splittable fallback, using a few helpers with large declared minimum sizes (add minimum-size support to the owned test window if it lacks it) | Fixture-dependent: first capacity float observed with ordinary-anchor/fallback identity; the 500-arrival replay never exhausted and does not count | WX8-M (partial) | [admission follow-up](windows-best-effort-admission-live.md#blockers-and-exact-next-action) |
| WSA-14 | Newest-first shortest-prefix and protected-member shrink; failed no-space unfloat retains intent, then explicit retry succeeds | Fixture-dependent: minimal-prefix victim sets with per-victim membership/markers and unchanged focus; retry only against a genuinely exhausted destination | WX8-M (partial) | [admission follow-up](windows-best-effort-admission-live.md#blockers-and-exact-next-action) |
| WSA-15 | Exhausted same-output capacity follow/stay; actual overlap/overflow admission with navigation; hidden-repair remaining tree/focus parity | Fixture-dependent: capacity arrivals proven, positive target intersection predicted then executed, hidden and visible legs keep identical victim sets and survivor plans with zero hidden geometry writes | WX8-M (partial), 15-M (partial), 33-M (partial) | [admission follow-up](windows-best-effort-admission-live.md#blockers-and-exact-next-action) |

## Display/shell

| ID | Action | Pass criterion | Gate IDs | Source |
| --- | --- | --- | --- | --- |
| WDS-01 | Drive Snap Layouts/bar/Assist/shake with tiler on, set Mouse Snap takeover off and restore; edit the tiler file externally and Revert | Native affordances behave per setting, external edit preserved/refused truthfully, no unintended writes | 4-W (partial), 22-M (partial), N2-M (partial) | [dialogs](archive/windows-shortcut-settings-dialogs.md) |
| WDS-02 | Open Settings at 100% and 150% on 1080p, visit four tabs by mouse, Ctrl+Tab, Ctrl+Shift+Tab; traverse by Tab/arrows; edit/Apply/Revert/close-reopen; Close/Escape/Alt+F4 with unsaved edits | Every control fits/reachable, focus visible, drafts survive tab switches, unsaved close saves nothing | 12-M (partial), 4-W (partial) | [tabs plan](shared-settings-catalog-windows-tabs.md#physical-check-plan-pending) |
| WDS-03 | Inspect conflict/warning badges on every tab under present, absent, unknown and query-failed Game Bar/GamingHomeApp states per 2026-10-17 proof-only rule | Warn only on positively present/configured conflict; unknown/absent shows ordinary text | 22-M (partial), WX1-M (partial) | [gaming plan](windows-gaming-coexistence.md#physical-acceptance-plan-pending) |
| WDS-04 | With bordered window active, open Start, taskbar flyouts, Task View, Alt+Tab hold and cancel; cover topmost and occluded windows; inspect border/underlay/preview pixels, smooth follow and bands | Border hides over shell surfaces and restores on eligible refocus without focus steal; no stale show on Esc/cancel/suspend/stop | 21-M (partial), 31-M (partial), 25-M (partial) | [record](archive/windows-active-border.md#dogfood-and-bounded-physical-checks), [threads](archive/windows-owner-threading-acceptance.md#remaining-acceptance-and-next-action) |
| WDS-05 | View underlay `#662a82da` over black, white and grey wallpapers at 100% and 150% | User accepts the appearance on each background and scale; no numerical compositing gate | 23-M (partial) | [record](archive/windows-underlay-default.md) |
| WDS-06 | Tile nested layouts and inspect occupied-leaf whole-rectangle preview and nested split placement visually | Rendered geometry matches plan, no visual quality inferred from tests | 36-K (partial) | [gate](windows-0.1-gate-proposal.md#original-checks-1-53) |
| WDS-07 | Compare quiet run-log/console summaries under normal and trace logging; run long trace-off/release session | Readable console, safety/decision/failure evidence immediate, no flood | 24-K (partial) | [record](archive/windows-trace-volume.md) |
| WDS-08 | Exercise hide/reveal bursts and force overflow/exception lanes at normal level | Bounded per-window outcomes with reasons, omitted history explicit, no malformed rows | 41-K (partial) | [record](archive/windows-visibility-diagnostics.md) |
| WDS-09 | Restart actual Explorer, toggle auto-hide on/off, live pager opt-out; click pager cells to switch; check miniature icons, theme, keyboard access and idle cost | Tray and pager recover from fresh geometry, click switches, appearance and cost acceptable | WX2-M (partial), 29-K (partial) | [pager lifecycle](archive/windows-taskbar-pager-lifecycle.md), [pager icons](windows-taskbar-pager-icons.md) |
| WDS-10 | Inspect non-bottom/invalid taskbar geometry, whole-strip overcapacity, physical mouse/icons/theme/accessibility and cost | Current unsupported anchors safely refuse and log without guessing, overcapacity draws nothing with tray fallback; safe refusal does not waive outstanding supported-placement acceptance | WX2-M (partial) | [pager lifecycle](archive/windows-taskbar-pager-lifecycle.md) |
| WDS-11 | Change scale/resolution/work area on one output with maximized/fullscreen/fixed-float windows; verify PMv2 conversion, tray fresh anchors and carrier reanchoring | Fresh conversion, overlays and pager retained, no fullscreen geometry writes, no stale carriers | 4-W (partial), 34-M (partial), 44-M (partial), N4-M (partial), WX3-M (partial) | [foundation](windows-multi-output-foundation.md), [pager lifecycle](archive/windows-taskbar-pager-lifecycle.md) |
| WDS-12 | BLOCKED chevron response: discover a verified chevron/overflow target and click it | BLOCKED/unrun: child discovery returned one classless entry with no verified target and no clicks; verified absence is not a pass | WX2-M (blocked) | [idle follow-up](archive/windows-owner-event-idle.md#scoped-native-follow-up-2026-10-18) |
| WDS-13 | Edit gaps externally 8-12-8 and restore; verify wake/adoption and per-window planned/effective/readback entries | Wake/adoption observed with exact convergence entries; aggregate mismatches alone close nothing, host-vs-adapter cause stays open | N2-M (partial) | [idle follow-up](archive/windows-owner-event-idle.md#scoped-native-follow-up-2026-10-18) |
| WDS-14 | Disable optional visuals and exercise preview/pager unavailability plus carrier creation-refusal and hide/teardown faults once a bounded fixture exists | Tiling continues without stale overlays or silent stop; optional border/underlay-off evidence alone is incomplete; refusal/fault legs remain fixture-dependent | N1-M (partial) | [threads](archive/windows-owner-threading-acceptance.md#optional-visuals-disabled-native-follow-up-2026-10-18) |
| WDS-15 | BLOCKED installed lifecycle: login/startup, Settings/Stop, update channel | Blocked on the undecided package/update channel; development-owner evidence does not count | 37-K (blocked) | [scope](../decisions.md#scope-and-platform-goals) |

## Games

| ID | Action | Pass criterion | Gate IDs | Source |
| --- | --- | --- | --- | --- |
| WGM-01 | Send a real fullscreen game numbered/relative follow and stay; repeat never-tiled born-fullscreen with sole held source and empty target | Arrival retains fullscreen, no tiling writes beyond native move, flags/views/focus/geometry logged | 1-M (partial), WX1-M (partial) | [backlog](../backlog.md#pending-live-checks) |
| WGM-02 | Run a real app/game born fullscreen beside tiles, exit, re-enter and exit again; optionally close before first exit | Untouched game input/frame while fullscreen, fresh tile on first exit, slot retained | 45-M (partial), WX1-M (partial) | [gate](windows-0.1-gate-proposal.md#original-checks-1-53) |
| WGM-03 | Contrast clean app-originated permitted and denied activation, explicit physical selection and urgency mark/clear during active play versus idle/paused, against tiler-off | Denial causes no switch/focus change; record permitted convergence separately from request policy; visible urgency remains BLOCKED on unimplemented presentation/classification, no post-0.1 5b behavior assumed | 1-M (partial), WX1-M (partial) | [record](archive/windows-d27-activation-verification.md#native-evidence-2026-10-14) |
| WGM-04 | Measure active/suspended incremental game CPU, frame-time and input cost against tiler-off baseline per M16 | Aim zero; any residual minimal and imperceptible; idle/suspension evidence does not count | 34-M (partial), WX1-M (partial) | [backlog](../backlog.md#open-work) |
| WGM-05 | Exercise Xbox-mode entry/return with bounded recovery evidence; check fixed/borderless-game no-touch and fullscreen overlay suppression | Clean return and eligible hidden-window recovery, no stuck holds or game geometry/focus/stacking interference; no automatic Xbox-mode heuristic claimed | WX1-M (partial), WX7-M (partial), 13-M (partial), 31-M (partial) | [incident](windows-gaming-coexistence.md#incident-diagnosis-and-prevention-options-2026-10-16), [fixed admission](archive/windows-fixed-size-admission.md) |

## Lifecycle

| ID | Action | Pass criterion | Gate IDs | Source |
| --- | --- | --- | --- | --- |
| WLC-01 | Restart owner with apps alive; verify D25 set/membership/provable-focus, D26 gaps/settings reload, sticky adoption, hydration protection, ordinary/sticky frame retention | Host set kept, membership re-observed without moves, only provable focus restored, no cross-login identity claim | 48-M (partial), 32-M (partial), 39-M (partial), 11-M (partial) | [record](archive/windows-restart-hydration-rejection.md) |
| WLC-02 | Sleep/wake once with tiled session; lock/unlock and sign-out-free session return as separate legs | Topology/lifetimes re-observed, hidden recovery preserved, no replayed writes or stuck holds; stop/restart is not sleep evidence | 30-M (partial), WX4-M (partial) | [plan](../research/windows-port/plan.md) |
| WLC-03 | Sleep/wake with a real fullscreen game and tiled session | Game unimpeded, tiling recovers, no stuck holds | N3-M (partial), WX4-M (partial) | [backlog](../backlog.md#pending-live-checks) |
| WLC-04 | Exercise faults at supported owner/input/render seams and the ledger-backed recovery path; verify write-before-hide ledger, watcher reveal, independent restore, minimize/maximize preimages | Recovery per current design with truthful uncertain-identity residue; uncertain/partial recovery needs a bounded fixture design first, so those legs stay parked; no unconditional no-strand promise | 28-M (partial), 30-M (partial), 50-M (partial), WX7-M (partial) | [record](archive/windows-managed-workspaces.md) |
| WLC-05 | Cover elevated/protected windows and a UAC consent prompt with secure-desktop return | Elevated stays unmanaged, unharmed and not product-hidden; secure desktop gets no geometry/focus/overlay control | WX5-M (partial) | [gate](windows-0.1-gate-proposal.md#approved-windows-specific-extras) |
| WLC-06 | Switch native virtual desktops away and back with Task View/Alt+Tab/taskbar present | Project workspaces and SW_HIDE intact, no native membership relocation, no shell-listing promise | WX6-M (partial) | [presentation](../research/windows-port/alt-tab-hidden-workspaces.md) |
| WLC-07 | Admit, move and close windows on hidden workspaces; reveal and compare plan | Background membership/planning without native writes, safe projection on reveal, no focus steal | 33-M (partial), WX8-M (partial) | [record](archive/windows-managed-workspaces.md) |
| WLC-08 | Cold-start owner over autofocused ordinary windows; maximize/restore with Win+M and send away/return; check retained-overlay minima; open the first-run preset prompt, then gracefully stop | Clean/tolerance-valid layouts keep fit; infeasible fits use sequential long-edge seeds with origin+minimum overlap/overflow allowed, bounded restore timing; pending prompt cancels with no preset written | 3-W (partial), 4-W (partial) | [placement](archive/windows-placement-correctness.md), [first-run](archive/windows-first-run-stop.md) |

## Two-monitor PC

Bounded native evidence 2026-10-20: focused-output workspace CLI routing and
secondary pager carrier geometry/visibility/message-driven selection pass
([record](windows-dual-output-live.md)). Subsequent guarded mouse selection and
occupied secondary hide/show passed. A stale-secondary-proof correction now has
bounded Explorer-restart recovery evidence. Latest event-only recheck also
recovers from a naturally empty secondary census. User physical 2026-10-20:
tiling survived Explorer restart, both pager positions were correct, and primary
anchor churn looked fine once. Display changes and repeated churn remain unproven.
No complete W2M row is inferred from these narrower checks.

W2M-01 remains partial: agent-driven exact-owner directional-handler checks
pass for Paint moves both directions into occupied/empty destinations,
local-before-cross exhaustion both ways, horizontal cross-focus and empty-target
focus no-ops both ways. Firefox occupied transfers pass both directions and
empty-destination right-to-left passes. Exact PMv2 arrival, source reflow,
foreground and unchanged currents were verified. User physical 2026-10-20:
Win+Shift+arrows cross-output moves pass; Win+digit on each output passes except
the [undecided empty-workspace focus case](../spec/reference-outcomes.md#empty-workspace-focus-across-outputs).
Vertical-local focus and output-candidate discrimination remain pending;
some candidate-discrimination legs require a third output. See the bounded
[outcome matrix](windows-dual-output-live.md#directional-handler-outcomes-2026-10-20).

Additional agent evidence is in the [bounded row matrix](windows-dual-output-live.md#additional-two-output-checks-2026-10-20):
W2M-02 tiled horizontal follow/stay passed; W2M-06 occupied one-way drop/cancel
passed; W2M-10 single-hidden-claim loss plus both-output restart passed.
Floating follow/return now passes a bounded Paint handler check with unchanged
frame/current views. User physical 2026-10-20: Win+Ctrl+Alt+arrows explicit
send-follow works as designed; cross-output title-bar drag with drop overlay
passed between outputs with different DPI. Empty-singleton and reverse-DPI-start
legs were not clearly covered; those and complete dual-hidden recovery remain
open. Task Manager stayed untiled (elevated by default; expected unmanageable
class, not a defect). W2M-08/04/07 are parked on input/owner or fixture dependencies.
All rows remain partial or blocked as stated; injected mouse/CLI evidence is
not physical shortcut or physical-user acceptance.

| ID | Action | Pass criterion | Gate IDs | Source |
| --- | --- | --- | --- | --- |
| W2M-01 | Place tiles on both outputs with different currents; Win+Shift+Right/Left to exhaustion and cross, reverse; repeat into empty dest; Win+Right/Left last-focused select and reverse; run one wide source against two outputs on the requested edge with centre-projection, span-overlap and left/top legs | Cross arrives on adjacent output with follow focus, fits work area, leaves source; empty dest switches nothing; vertical focus stays local; correct candidate each discrimination leg; note where a third output is required | 2-M (partial), 43-M (partial), WX3-M (partial); user physical 2026-10-20: Win+Shift+arrows cross-output moves and Win+digit on each output pass except undecided empty-workspace focus; vertical/local-exhaustion/empty/candidate physical matrix not established | [foundation](windows-multi-output-foundation.md#remaining-physical-check), [agent outcomes](windows-dual-output-live.md#directional-handler-outcomes-2026-10-20), [user checks](windows-dual-output-live.md#user-physical-checks-2026-10-20) |
| W2M-02 | Send explicit Win+Ctrl+Alt+arrows/HJKL follow, rebind stay from empty defaults; verify CURRENT dest arrival and source MRU stay | Follow arrives/focuses, stay keeps source selected with tiled-side-only reflow, float frames unchanged | 43-M (partial), WX8-M (partial); user physical 2026-10-20: Win+Ctrl+Alt+arrows send-follow works as designed; rebound stay and full boundary matrix remain open | [record](archive/windows-explicit-output-send.md), [user checks](windows-dual-output-live.md#user-physical-checks-2026-10-20) |
| W2M-03 | Whole-workspace migration including default vs MRU source-refill, cleared/invalidated history, empty migration and occupied target insertion; v2 route delivered and fixture-accepted ([closeout](archive/core-world-model-rearchitecture.md#closing-outcome-2026-10-24)) | Fixture: both migration directions reveal both companions at target with source-empty enumeration and refill receipt/retirement. Physical matrix unrun: MRU occupied refill, cleared/invalidated history, empty migration, occupied-target insertion and sticky breadth; no whole row passed from fixture checks | 42-M (partial), WX8-M (partial) | [closeout](archive/core-world-model-rearchitecture.md#closing-outcome-2026-10-24), [superseded route](archive/windows-full-world-state-replies.md#w2b-cross-stream-proposals) |
| W2M-04 | In local/global-unique/shared modes run previous-twice, full relative ring, select-visible-elsewhere swap-back, per-output select/send follow/stay; restart shared mode with stable-key-first differing from primary/focused output | Histories/rings per scope correct, output-focus-alone records nothing, layouts/shares/float/focus survive swap; shared keeps one logical history/ring/spare and occupied cleanup protection; tentative startup anchor discriminated | 14-M (partial), 15-M (partial), WX3-M (partial) | [record](archive/windows-workspace-scope-hotplug.md#remaining-physical-journeys) |
| W2M-05 | In each scope mode unplug with foreground on removed then survivor output; send explicitly and open new contents while absent; reconnect; repeat mixed-DPI and conditional R-WS-17 second toggle | History discarded/re-primed, reconnect independent of history, then-current local/global displaced return versus shared no displacement mapping; frames/focus continuous; nearest-survivor discrimination needs three outputs, unsupported outcomes stay TBD | 51-M (partial), 14-M (partial), WX3-M (partial) | [record](archive/windows-workspace-scope-hotplug.md#remaining-physical-journeys) |
| W2M-06 | Drag native title bar into occupied and empty destination layouts on the other output; cancel one with Esc; force one failed arrival | Preview/drop agree, source reflows, failed arrival commits no membership | 47-M (partial), 25-M (partial); user physical 2026-10-20: cross-output title-bar drag with drop overlay passed between different-DPI outputs; empty-singleton/reverse-DPI-start not clearly covered, physical cancel/forced failure unproven | [drag](archive/windows-mouse-drag.md), [user checks](windows-dual-output-live.md#user-physical-checks-2026-10-20) |
| W2M-07 | Run hidden/sticky quiet, unreadable-output, stale-reply, immediate/delayed send and forced-refresh sequences across outputs | No departure ping-pong, no replay, quiet settlement | 49-M (partial) | [gate](windows-0.1-gate-proposal.md#original-checks-1-53) |
| W2M-08 | Repeat trailing-empty fill/append/close/send/switch cycles on both outputs | No oscillation or tiling stop; only listed cosmetic issues allowed | 52-K (partial) | [backlog](../backlog.md#pending-live-checks) |
| W2M-09 | Change DPI/resolution/work area across outputs with maximized/fullscreen/fixed-float windows; check border/underlay/preview/pager reanchor and cursor agreement | Fresh conversion, overlays retained, no fullscreen geometry writes | 44-M (partial), N4-M (partial), WX3-M (partial) | [foundation](windows-multi-output-foundation.md) |
| W2M-10 | Recover from exact-owner loss with both output-local workspaces; refresh identities/topology and restore hidden claims | Tiling restored on both outputs without false-clean stop | 50-M (partial), WX7-M (partial) | [record](archive/windows-managed-workspaces.md) |
| W2M-11 | Sleep/wake with both output-local workspaces; refresh identities/topology and restore hidden claims | Tiling restored on both outputs, game leg stays in WLC-03 | 50-M (partial), WX4-M (partial) | [backlog](../backlog.md#pending-live-checks) |
| W2M-12 | T2 capacity-float crossing refusal on two outputs (paired with P1 \| Core + Windows: world migration authority and T2 crossing refusal); v2 refusal delivered ([closeout](archive/core-world-model-rearchitecture.md#closing-outcome-2026-10-24)) | Offline regressions: unsupported crossings preserve source Engine and membership with no remap guessed. Native/physical two-output refusal matrix unrun; no whole row passed from offline checks | 43-M (partial), WX8-M (partial) | [closeout](archive/core-world-model-rearchitecture.md#closing-outcome-2026-10-24), [superseded route](archive/windows-full-world-state-replies.md#w2b-cross-stream-proposals) |
| W2M-13 | Inspect secondary taskbar tray anchors and per-output pager after actual Explorer restart, mixed-DPI changes and physical selection | Exact tray/clock anchors or the recognized no-notification-area hierarchy serve each output; unreadable/unsupported hierarchies safely refuse with tray fallback. Bounded secondary mouse/occupied/restart evidence passes after stale-proof correction. User physical 2026-10-20: tiling survived Explorer restart, primary/secondary pager positions correct, primary anchor churn looked fine once; display changes, physical pager selection and repeated churn remain unproven | WX2-M (partial), WX3-M (partial) | [user checks](windows-dual-output-live.md#user-physical-checks-2026-10-20), [pager lifecycle](archive/windows-taskbar-pager-lifecycle.md), [gate](windows-0.1-gate-proposal.md#approved-windows-specific-extras) |

## Blocker note

- World-route implementation is delivered, not blocked: the atomic cutover and both default-local migration directions have native fixture acceptance; T2 unchanged-source refusal has offline regression coverage per the [core closeout](archive/core-world-model-rearchitecture.md#closing-outcome-2026-10-24). W2M-03/W2M-12 remain unrun as physical checks (MRU occupied refill, invalidated history, empty insertion, sticky breadth, two-output refusal matrix). No new implementation is selected here. Sources: [closeout](archive/core-world-model-rearchitecture.md#closing-outcome-2026-10-24), [superseded W2b record](archive/windows-full-world-state-replies.md#w2b-cross-stream-proposals) and [backlog](../backlog.md#open-work).

## Inventory verification

- Consolidated 2026-10-19: 67 pending rows (keyboard 11, apps/journeys 15, display/shell 15, games 5, lifecycle 8, two-monitor 13). Counts include explicitly blocked/fixture-dependent rows, not gate passes.
- All 52 applicable base gate IDs and WX1-WX8 have source-linked partial mappings; excluded checks are absent. Added links/heading anchors, ASCII additions and `git diff --check` verified; independent source-fidelity review accepted the checklist. Documentation checks establish no native/physical acceptance.
- Unsupported outcomes remain TBD in linked source discriminators. Removed Win+drag/underlay-C work, deferred D27 5b policy implementation and unselected package-channel choices are not invented runnable checks; their relevant acceptance dependencies remain explicit above.
