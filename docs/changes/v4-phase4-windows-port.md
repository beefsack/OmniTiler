# V4 phase 4: Windows port

## Purpose, status and gates

- Implementation record for porting `tiler-windows` onto the supported
  [v4 world contract](../porting/world-contract.md). The contract, spec and
  [decision register](../decisions.md) own normative behavior; this record
  tracks implementation status, review findings and ordered work until the port
  completes.
- Port/build/live work runs on a Windows host through the
  [Windows dev environment](../windows-dev-environment.md) and
  [live Windows testing guide](../live-windows-testing.md) using the live
  fixture scoped fixture-only. Offline compilation and unit tests do not
  establish live or physical acceptance.
- The shared core per-output scaling prerequisite (scaling resize minima,
  keyboard steps and group-edge zones once per output using `screen.scale`)
  remains open alongside this port; see [R10](../decisions/contract.md#size-units-and-scaling-r10)
  and [v4-phase2-kde-scaled-metrics.md](v4-phase2-kde-scaled-metrics.md).

## Current state and review of `55d14355..9a98994d` (2026-11-25)

### What landed cleanly

- `tiler-windows` compiles against the v4 core and passes its library unit
  suite (1,163 passed, 1 ignored at `9a98994d`); obsolete v3/T2 staged-clone
  remap code and `world_native_receipts.rs` were removed.
- Per-monitor effective DPI (`GetDpiForMonitor` / `MDT_EFFECTIVE_DPI`) is read
  at the monitor enumeration seam (`crates/tiler-windows/src/ffi/enumeration.rs`).
- `drag_preview` and per-command `BatchDisposition` settlements are wired onto
  the v4 reply/transition shapes; retired maximize-clear fields were removed
  from `scripts/windows-world-stress*.ps1`.

### Defects and regressions to fix first

1. **`host_hidden: Some(true)` misrepresents project-suppressed (`SW_HIDE`) windows.**
   `crates/tiler-windows/src/tiling_sys/world_capture.rs` sets `flags: None`
   and `host_hidden: Some(true)` for hidden-claim windows on inactive
   workspaces. In v4, `host_hidden` means DWM-cloaked / OS-held-back, whereas
   project suppression rides `visibility: { visible: Some(false), suppression_owned: Some(true) }`.
   Core (`window_classify.rs`, `world_observe.rs`, `world_selection.rs`) treats
   `host_hidden: Some(true)` as minimized/host-hidden, which evicted every
   inactive-workspace resident into `minimized_slots` (`frame_hold: minimized`),
   blocked `visibility_authority: Controlled` and remembered focus on workspace
   return, led to weakened assertions in `selection_return_core_effect_binds_exact_grant`
   (`world_native_roundtrip_tests.rs`) and `Test-StressApplyPending`
   (`scripts/windows-world-stress.ps1`), and motivated unapproved `tiler-core`
   workarounds in `9a98994d`.
2. **Unapproved `tiler-core` minimize-restore changes in `9a98994d`.**
   `has_minimized_resident` (`queries.rs`, `pair_send.rs`) and the `lone_root`
   restore-ordering branch (`convergence_phases.rs`) were introduced to work
   around Finding 1. They introduce unapproved user-observable restore ordering
   (wrapping a multi-window `current_tree` 50/50 on the workspace's long axis,
   inconsistent between one and multiple minimized residents) with no
   `reference-outcomes.md` row.
3. **Core composed-sticky migration/swap regression in `b3caaa06`.**
   `crates/tiler-core/src/world_commands.rs` (`swap_source_members` and hidden
   migration collection) checks `fact.sticky != Some(true)` when
   `!native_sticky_supported()`. On composed-sticky hosts (`native_sticky: false`),
   sticky windows must carry `fact.sticky == None` and live in core's retained
   sticky intent (`derived_for(window).sticky`), so `fact.sticky != Some(true)`
   incorrectly migrates/swaps composed sticky windows (violating
   [Workspace migration D7](../decisions/workspaces.md#workspace-migration-d7-floats-and-sticky-windows)).
   Meanwhile, `sticky_windows` in `global-unique` swap (`world_commands.rs`)
   still filters `fact.sticky != Some(false)`, treating every Windows window as
   sticky during `layout.swap_workspaces`.
4. **Fabricated visibility receipts in `world_diff.rs`.**
   `collect_visibility_attempts` runs after `apply_world_reply_inner` even on
   an early stale return and fabricates `VisibilityDisposition::Called { Accepted }`,
   `path: Staged`, `pre: None`, and `lifetime_matched: Some(true)` for every
   window whose live visibility happens to match `target.shown`, without
   checking whether any setter ran or verifying lifetime identity.
5. **Broken startup `workspace.mode`, discarded diagnostics, and dropped CLI overrides.**
   `WorldSequencer::bootstrap_inner` calls `connect(..., &[])` at revision 1,
   latching restart-only `workspace.mode` to `per-output-local` before
   `run_loop.rs` sends the real settings snapshot at revision 2 (which core
   returns `Accepted` with a `RestartRequired` diagnostic while keeping
   `per-output-local`). `apply_settings_values` discards `SettingsReply.diagnostics`
   (masking a `"workspace.mode"` catalog-ID typo in
   `world_diff_shared_convergence_tests.rs`). `core_setting_values` ignores
   `cli_overrides` (`--inner-gap`, `--outer-gap`), contradicting
   [Windows Settings persistence](../decisions/settings.md#windows-settings-surface-and-persistence).
6. **`world_diff.rs` guards, legacy `-refill` parsing, and compiler warnings.**
   - `row_on_current_view` fails open on missing output row / `logical_view: None`.
   - `AlreadyAtTarget` emission runs before checking `t.held` and `ensure_fresh`
     and carries an inverted "host-owned" comment.
   - Paired migration source-refill views still match `-refill` suffix strings
     instead of `TransferTargetV4.parent` ([D5 decision](../decisions/workspaces.md#workspace-migration-d5-source-refill)).
   - `tiling_sys/settings.rs` still JSON-round-trips handshake/settings
     in-process instead of calling typed `WorldEngine` connection methods.
   - `cargo check -p tiler-windows --tests` has 6 lib warnings and 9 test
     warnings; 14 maximize-clear tests were removed from
     `tests_directional_world.rs` in `55d14355` pending `StateTarget` adoption.

### Remaining unported v4 contract lanes

- **R20 Anchored Border:** `world_capture.rs` hardcodes `witness: None` and
  `foreground_out: None`, so core hides the border and `focus.rs` never paints.
- **R14 Window-state commands & `StateTarget`/`StateAttempt`:** `world_diff.rs`
  has no state actuator, `world_sequencer.rs` hardcodes `state_attempts: []`,
  and `keyboard_overlays.rs` still toggles maximize/fullscreen locally instead
  of issuing core commands.
- **Unified retained focus (`FocusFeedback` + `ArrivalBracket` + `afterShown`):**
  `world_sequencer.rs` hardcodes `focus_feedback: []`; `world_diff.rs` ignores
  `after_shown` and retains legacy `ViewAttempt` / `SelectionArrivalGrant`
  machinery.
- **Retained intent persistence (`intentDesired` / `intentRecords`):**
  `world_sequencer.rs` hardcodes `intent_records: []`; `reply.intent_desired`
  is unconsumed and duplicate adapter intent tables remain.
- **`dragPreview.id` & `RendererInput`:** `drag_preview.rs` checks
  `preview.source` rather than `preview.id` against the active gesture ID;
  `world_sequencer.rs` hardcodes `renderer_input: None`.
- **R11 Output identity, R10 core scaling, and C3 warp adapter:**
  `OutputIdentity::unknown()` is hardcoded; core `scale_once` does not yet
  scale resize minima, keyboard steps, or group-edge zones. Stage-1
  core/protocol retained `warpRequest` and the stage-2 Windows adapter
  (Raw Input witness, exact-once `SetCursorPos`, `pointerActivity`/`warpSpent`
  plumbing) have landed, and `warp:true` is declared on the clean-HEAD live
  receipt below.

## Tentative decisions (2026-11-25)

Recorded canonically in
[Windows tentative v4 port direction](../decisions/windows.md#tentative-v4-port-direction)
and flagged tentative for later review:

- **T1 (Revert core hidden-target hack):** Revert `9a98994d`'s
  `has_minimized_resident` and `lone_root` core changes; fix Windows
  `host_hidden` capture instead. Revisit genuine minimized-target restore
  ordering separately with a `reference-outcomes.md` row if wanted later.
- **T2 (Keep `AlreadyAtTarget`):** Retain the active v4
  `MemberDisposition::AlreadyAtTarget` contract arm (no new receipt type);
  guard with `!t.held` and `ensure_fresh` and fix the inverted comment.
- **T3 (Fail closed on unknown view):** `row_on_current_view` returns `false`
  (suppresses frame write) when the output row is absent or `logical_view` is
  `None`.
- **T4 (Live reads on suppressed HWNDs):** For project-suppressed (`SW_HIDE`)
  windows, capture performs lightweight live reads (`IsWindowVisible`, DWM
  cloaked, style/owner/class) with fail-closed hold if unreadable, reporting
  `host_hidden: Some(cloaked)` and `visibility: { visible: Some(vis), suppression_owned: Some(true) }`.
- **T5 (Truthful incremental capabilities):** Advertise `above: true` in
  `windows_capabilities()` only after its actuator, R12 preimages, release
  path, and live border follow are proved; the Task 9 section below records
  that proof. `warp` is now `true` through `WARP_SUPPORTED` after the stage-2 adapter and
  the clean-HEAD live receipt below (`fdead7d45e`); `cursorWarp` still
  defaults off.
- **T6 (Direct typed connection calls):** Replace the in-process JSON
  encode/decode round-trip in `tiling_sys/settings.rs` with direct typed
  `WorldEngine` handshake/settings calls while keeping `handle_report_checked`
  for reports.
- **T7 (Windows warp adapter, capability flipped):** Stage 1 landed the
  retained `warpRequest` in core/protocol with Q1 ("newer focus decision to
  another lifetime cancels; same target keeps") and Q2 ("newer or unknown
  `pointerActivity` cancels against the frozen `pointerBaseline`; own warps do
  not advance the generation"). Stage 2 adds the Windows exact-once actuator:
  mouse Raw Input (`RIDEV_INPUTSINK` on the tray HWND) as the raw-input
  pointer witness (injected input conservatively counts; the adapter's own
  `SetCursorPos` does not), report `pointerActivity`/`warpSpent` plumbing,
  spend-before-move by exact request id, and an ignored live probe whose
  clean-HEAD run is recorded below. `warp` flips to `true` in this separate
  commit; `cursorWarp` remains off by default.

## Ordered task plan

### Task 1 — Repair recent regressions and establish a clean baseline

- Fix `world_capture.rs` hidden-claim capture per **T4** (`host_hidden`,
  `visibility`, `owned`, `tool_window`, `window_type`).
- Revert `9a98994d`'s `tiler-core` `has_minimized_resident` / `lone_root`
  changes per **T1**.
- Fix `tiler-core/src/world_commands.rs` composed-sticky exclusion in
  `swap_source_members`, hidden-lane migration, and `global-unique` swap
  `sticky_windows` using `derived_for(window).sticky`; add unit tests covering
  `native_sticky: false` composed sticky windows.
- Fix startup settings sync: pass initial settings (with `cli_overrides`
  applied) into `WorldSequencer::bootstrap` / `connect()` at revision 1 so
  restart-only `workspace.mode` adopts at startup; inspect and log
  `SettingsReply.diagnostics`; fix `"workspace.mode"` -> `"mode"` in
  `world_diff_shared_convergence_tests.rs`; add unit tests for all 18 core wire
  keys, CLI overrides, refused revisions, and `RestartRequired`.
- Switch `tiling_sys/settings.rs` to direct typed `WorldEngine` connection
  calls per **T6** and set `above: false, warp: false` per **T5**.
- Make `row_on_current_view` fail closed per **T3**, guard `AlreadyAtTarget`
  with `!t.held` + `ensure_fresh` per **T2**, and correlate migration refills
  via `TransferTargetV4.parent` instead of `-refill` suffix matching.
- Restore the weakened assertions in `selection_return_core_effect_binds_exact_grant`
  (`world_native_roundtrip_tests.rs`) and `Test-StressApplyPending`
  (`scripts/windows-world-stress.ps1`); clear all compiler/clippy warnings in
  `tiler-windows`; keep formatting-only changes out of semantic commits.

### Task 2 — Real v4 visibility actuation, `FocusFeedback`, and `ArrivalBracket`

- Remove `collect_visibility_attempts`. Actuate per-window `row.visibility`
  under `visibility_authority == Controlled` following the contract's
  [normative apply rule](../porting/world-contract.md#normative-apply-rule)
  (shows first, focus after arrival brackets, hides after proven foreground or
  authorized fallback; at most one write per `(lifetime, rev, target)`),
  reporting genuine `VisibilityAttempt` (`Called` / `NotCalled`) with real
  `pre`/`post` `VisibilitySample`s and verified `lifetime_matched`.
- Produce real `FocusFeedback` + `ArrivalBracket` (output arm) for
  `reply.focus_request`, honor `request.after_shown`, and remove the dead
  native-free `ViewAttempt` emission and `SelectionArrivalGrant` side-machinery.
- Run a post-commit fixture-scoped live `windows-world-stress` check on a clean
  worktree and confirm zero weakened checks.

### Task 3 — R20 Anchored Border witness, `dragPreview.id`, and pager `RendererInput`

- Populate `PresentationIngressV4.witness` (`FrameWitness` causally joined to
  observed foreground lifetime, frame, and watermark) and `foreground_out` in
  `world_capture.rs`, and verify anchored live border following in `focus.rs` /
  render bridge.
- Match `reply.drag_preview.id` against the active gesture ID in
  `drag_preview.rs`.
- Populate `renderer_input` (`rev` + `RenderTarget`) on taskbar pager clicks.

### Task 4 — R14 `StateTarget`/`StateAttempt`, `intentDesired`/`intentRecords`, and R12 preimages

- Route maximize, fullscreen, and sticky toggles through core `WorldCommand`s
  instead of local toggling in `keyboard_overlays.rs`.
- Implement `StateTarget` actuation in `world_diff.rs` (`maximized` including
  `automaticClear`, `fullscreen`, and `above` with R12 crash-safe preimages;
  flipped `above: true` after the Task 9 live proof below), report honest `StateAttempt`s through
  `WorldSequencer`, and restore the maximize-clear test coverage removed from
  `tests_directional_world.rs`.
- Wire `intent_records` preload before first admission and atomic
  `intent_desired` reconciliation before dependent apply (holding on failure,
  zero redundant writes when equal), retiring duplicate adapter intent tables.

### Task 5 — Core scaling prerequisite (R10), R11 output identity, landed C3 warp adapter, and live matrix

- Implement the shared `tiler-core` R10 prerequisite: scale resize minima,
  keyboard steps, and group-edge zones once per output via `screen.scale`.
- Populate Windows `OutputIdentity` (EDID/QDC manufacturer/model/serial with
  connector fallback) for R11.
- Implement the C3 stage-2 Windows warp adapter per **T7**: mouse Raw Input
  witness on the tray HWND, report `pointerActivity`/`warpSpent`, exact-once
  spend by request id, and the ignored live non-increment probe below.
  `warp:true` is declared after the clean-HEAD receipt; `cursorWarp` stays
  off by default.
- Run the fixture-scoped Windows live acceptance matrix: hidden-target sends
  and restores, iconic mover sends, view switches under native hide lag,
  multi-monitor and mixed-DPI transitions, restart and forced-crash recovery
  with R12 preimages, startup/live settings revisions (including refused and
  `RestartRequired`), and fullscreen/game safety legs.

#### C3 stage-2 live Raw Input witness evidence

Ignored, opt-in probe in `ffi::raw_input`
(`live_set_cursor_pos_does_not_advance_raw_input_generation`). Clean-tree
receipt at HEAD `fdead7d45e` (working tree empty) on 2026-11-25:

```
cargo test -p tiler-windows --lib raw_input -- --ignored --nocapture
raw-input witness: SetCursorPos gen 0 -> 0 (unchanged); synthetic move inserted=1, gen 0 -> 1
test ...live_set_cursor_pos_does_not_advance_raw_input_generation ... ok
```

- This is the clean-HEAD run that authorizes the `WARP_SUPPORTED = true` flip
  in the following commit; the stage-2 commits themselves kept it `false`.
- Ownership gate: the probe creates its own visible topmost window, requires
  the foreground HWND plus PID to be that test process, and checks
  `WindowFromPoint`'s root against the probe window immediately before every
  forward `SetCursorPos`. The saved cursor preimage is restored only while the
  probe window still owns the foreground; that cleanup write is not
  target-checked (the saved point is by definition arbitrary) and sends no
  click or key.
- Limit: Windows Raw Input does not distinguish physical from injected input.
  The positive leg proves handler liveness (a delivered mouse record advanced
  the same generation the warp lane re-reads) and asserts it; the flip
  requires both that liveness and the `SetCursorPos` non-increment property.
  Injected motion counts conservatively as user activity.

### Task 5 live evidence (2026-11-25)

Run from clean committed HEADs; fixture-only scope, with cleanup reported for
every case. Each subsection names its exact HEAD. The R11 probe and six-case
matrix below ran at clean HEAD 42558707; later subsections name theirs. No
display identity values or host paths are recorded here.

- R11 read-only probe:
  `cargo test --locked -p tiler-windows --lib live_monitor_identities_are_readable -- --ignored --nocapture`
  passed for two connected displays: manufacturer/model/serial presence `true`
  on both, connector presence `false` on both (presence only; values not read).
- Fixture matrix, `mise exec -- just world-stress -Case <case> -DelayMs <n>`:

| Case | DelayMs | Publications | Added delay | Status |
| --- | --- | --- | --- | --- |
| home-away | 0 | 102 | 0 ms | pass |
| home-away | 10 | 102 | 1490 ms | pass |
| home-away | 50 | 102 | 7450 ms | pass |
| trio-churn | 0 | 102 | 0 ms | pass |
| trio-churn | 10 | 102 | 1560 ms | pass |
| trio-churn | 50 | 102 | 7800 ms | pass |

All six printed `world-stress final cleanup ok`, `status=pass`, and the new
warp-witness/geometry assertions in `Test-StressApplyClean` held at every
accepted apply row.

#### Iconic resident + mover send (manual fixture, 2026-11-25, clean f86c6adf)

Two borderless fixture windows A/B were tiled on one output under the
fixture-only scope. B was minimized through the fixture (`observed.mode =
minimized`, `observed.visible = true`; A stayed windowed and took the full
work area). A fresh accepted row gave B `frame_authority = hold`,
`frame_hold = minimized`, `membership_authority = controlled`,
`membership_hold = null`, `allocation = null`, while A stayed controlled with
no hold. `workspace --send 2` dispatched (`dispatched:true`); the settled row
moved A to the ordinal-2 workspace on the primary output with zero pending
transfers/operations/held scopes, and B remained on ordinal 1 with the same
minimized hold and controlled membership. Selecting 2 then 1 produced fresh
accepted rows and an A hide/reveal roundtrip (`ShowWindowAsync SW_HIDE`,
non-activating `SetWindowPos`); B stayed minimized/visible and was never
moved, hidden, restored, duplicated, or left holding pending intent. Cleanup
stopped the exact owner, shut down the fixture, restored the settings bytes
and SPI state, and left no owned ledger, process, or fixture window.

This proves the migrated minimized-resident path: no automatic restore, no
invented frame/membership hold, and no lingering order workaround during a
mover send and workspace return. Limits: B was never posted a hide while
off-view; the follow-up below sends the iconic window itself and preserves the
hold, but still does not observe a native hide-to-show transition. The strict
stress harness cannot encode this row without a minimize/restore observation
op and a minimized-aware oracle.

A follow-up iconic-subject run at clean `6236e534` reproduced the minimize
hold (frame authority hold, frame hold minimized, membership controlled,
allocation null, zero frame writes to the iconic window) and then sent the
iconic window itself. The first `--send 3` target was rejected because ordinal
3 was outside the per-output ring; that was a test-target choice, not a
product finding. The corrected `--send 2` first remained pending at 30
seconds, but an actuation-aware rerun settled on tick 8 after 11.2 seconds:
alpha moved to `ws-3` controlled with allocation `2560x1380`; beta stayed
`ws-1`, minimized, membership controlled, allocation null, with zero frame
writes and its native rect unchanged. `workspace --select 1` then returned to
`ws-1`; beta remained minimized, visible and native-rect-identical, with no
automatic restore, invented hold, or hybrid allocation. Cleanup stopped and
restored, the ledger was clean, and settings/SPI matched the preimage.

Limits: beta was never natively hidden (it stayed `visible:true` off-view), so
`SW_SHOWMINNOACTIVE` is behaviorally indistinguishable from a no-op reveal
for a visible iconic window. The leg proves preservation and no invention,
not a hide-to-show transition. One earlier return run hit a transient
held-scope (`w24` on `ws-3`, `unreadable`) that produced no writes or hijack
and did not reproduce on the passing run.

#### Multi-output send + positive retained warp (manual fixture, 2026-11-25, clean 1d00b766)

Two connected outputs with different effective scales (primary 1.25,
secondary 1.0) were used read-only; no display mode or DPI change. Both
outputs reported presence-only identity `[manufacturer=true, model=true,
serial=true, connector=false]`; no identity values were read. The settings
preimage was captured and restored byte-exact. Startup settings were accepted
at revision 4; after both fixture windows were admitted and the source
focused, a live settings update at revision 5 enabled `core.cursor_warp`,
which the owner logged as accepted and adopted before dispatch. One fixture
window per output was admitted controlled (`o1/ws-1`, `o2/ws-2`), and the
accepted `desired_geometry` carried exactly two screen rows with positive
scales.

`send-output --follow --direction right` dispatched through the exact-owner
CLI (edge `cli`, production `workspace-output` send-applied tick 7). The
settled accepted row was tick 8: subject `o2/ws-2`, both authorities
controlled, frame `960x1032 @ 3520,355`; native readback matched exactly
(delta 0), inside the destination, visible, windowed, focused; the source
output carried zero desires; no pending transfers or operations; exactly two
sentinel titles (no ghost/duplicate). The same mint apply (tick 7, index 96)
carried `warp-0`, target = subject, revision 12, output `o2`, workspace
`ws-2`, point `[4000,871]`, `pointer_baseline: 0`, `warp_capability: true`,
and `cursor_warps: 1`. The requested point equals the clamp of both the
mint-time and settled subject-frame centre. Actual cursor readback after
settlement was `[4000,871]`; all post-dispatch `pointer_activity` values
remained `0`, so the adapter's setter did not self-count. The retirement ack
`warp_spent: warp-0` is a staged report witness and appears on the next
emitted apply row (index 105), not the mint row; that row split is expected.
The finally path restored the pre-test cursor with bounded `SetCursorPos` and
readback, stopped the exact owner, shut down the fixture, restored the
settings bytes and SPI state, and left no owned ledger, process, request
file, or fixture window.

#### Input audit (2026-11-25, all live runs)

All 18 owner logs from the six stress runs, the iconic-resident run, the
multi-output/warp run, and both settings/fullscreen legs were audited for
physical key input. The audit found zero physical key callbacks: all 976
snap-callback records carried `source:"injected-filtered"`, all 946 command
edges carried `edge:"cli"`, and the only non-CLI command edge was one
`foreground` wake. The filtered synthetic key records produced no command and
no window or workspace state change. The recorded passes are not contaminated
by key input; the initial fullscreen timeout below was a test-predicate gap,
not input.

#### Settings restart-required (manual fixture, 2026-11-25, clean 6236e534)

The preimage had `core.mouse.snap_prevention:true`; the owner was launched
without `--no-mouse-snap-prevention`, and both owners logged
`mouse-snap disabled phase:resume` then `mouse-snap restored phase:teardown`
after a verified readback (a mismatch would log `mismatch` and retain the
claim). The standalone `tile restore` after owner exit reported
`{restored:true, windows:0, ledger_cleaned:true, mouse_snap_restored:false}`:
the teardown had already restored SPI and relinquished the ledger note, so
there was no owned mutation left to restore; this is "nothing owned", not a
failed restore. An independent read-only SPI read after cleanup matched the
preimage exactly (`arranging=1`, `pen=35`, `abm=0`). Limit: the driver did not
sample SPI at stop; equality rests on the product's verified teardown plus the
independent read.

Settings replay: the startup document set `core.workspace.mode=shared` at
revision 4. The owner accepted its initial settings at revision 1 with no
diagnostics and adopted `shared` (both output views reported `ws-1`). A live
change to `per-output-local` was accepted at revision 2 with the single
diagnostic `{key:"mode", kind:"restart-required"}`; the workspace scope stayed
`live-pending` with effective `shared` retained (`saved:"per-output-local"`),
and a selector command did not adopt it (views remained `ws-1`/`ws-1`).
Restarting with the changed document accepted an initial revision 1 with no
diagnostics, adopted `per-output-local`, and produced views `ws-1`/`ws-2`.
The refused-revision path has no live seam; unit tests cover it. Cleanup
restored the settings bytes byte-exact.

#### Fullscreen safety result (manual fixture, 2026-11-25, clean 6236e534)

A borderless fixture window was tiled on `o1/ws-1`, then set fullscreen through
the fixture at 2560x1440. The sole accepted post-transition apply row, tick 5,
carried `held_scopes:[{scope:{kind:"workspace", out:"o1", ws:"ws-1"},
authority:"hold", reason:"unreadable"}]`, `stats.held:2`,
`stats.geometry_writes:0`; both windows `w23` and `w24` had
`frame_authority:"hold"`, `frame_hold:"unreadable"`,
`membership_authority:"hold"`, `membership_hold:"unreadable"`,
`allocation:null`, `frame_op:null`, and no transfers, operations, or frames.
The adapter logged `w24` as `retained-fullscreen` with `reason:"overlay"` and
`overlay:"fullscreen"`, visible and uncloaked. `observe.managed` fell from 2 to
1; that is not proof of unreadability, since retained overlay rows ride the
retained lane while `managed` counts eligible observed windows.

The initial 30-second test predicate timed out because it treated the held
scope as pending and also required a readable-domain `untouched` row
(`frame_authority:"untouched"`, non-null allocation, no frame operation). The
missing `untouched` row is entailed by the scope hold: held scopes publish
lifetime rows with narrow holds, and the fullscreen `Untouched` branch runs
only for readable/controlled scopes. This was a test-predicate gap, not a
product publish gap.

A held-scope-aware rerun then passed the safety row at the same clean HEAD.
On tick 5 both desired rows had `frame_authority:"hold"`,
`frame_hold:"unreadable"`, membership authority `hold`, `allocation:null`,
`frame_op:""`, with `desired_operations:[]`, `desired_transfers:[]`,
`frames:[]`, and `geometry_writes:0`. `w24` was logged
`retained-fullscreen` then overlay, `visible:true`, `cloaked:false`; native
rects were alpha `0,0 2560x1440` and beta unchanged `1280,0 1280x1380`. No
late publish rows or writes followed settlement. A directional CLI send used
`w24` as origin; the post row had no focus effects, beta was not focused (no
hijack), and the next tick retiled both windows with native restoration and
`cursor_warps=0`. The skip-reason surface was `absent` in this run: the trace
carried only the row-level `unreadable` reason without a skip or refusal
event naming the cause. The later Task 9 fullscreen leg supersedes this: the
readable-domain `untouched` overlay row is published with a named
`fullscreen` skip, and the `unreadable` hold did not reproduce on the current
tree. The safety
behavior itself passes: zero tiled writes, zero pending transfers/operations,
visible retained fullscreen, and no hijack. Game/anti-cheat acceptance
remains user-owned; no game was run.

#### Forced-crash/R12 recovery (manual fixture, 2026-11-25, clean 6236e534)

Two fixture windows were admitted under the fixture-only scope with settings
bytes and SPI captured; the exact owner pid/creation/executable/ledger
identity was recorded. `select 2` left `ws-1` with both fixture windows
natively hidden (`visible:false`, rects unchanged) and the ledger present.
The graceful prerequisite passed: `stop` returned
`{stopped:true, owner_exited:true}`, and independent `restore` returned
`{restored:true, windows:0, ledger_cleaned:true}`; both windows were visible
with unchanged rects and the ledger was clean.

The crash leg started a second exact-identity owner, left both fixture windows
hidden with `ledger.json` present, revalidated identity, and ran
`emergency-stop` → `{emergency_stopped:true, owner_exited:true,
recovery_required:true}` with the ledger still present. The owner's
`watch-owner` child performed the independent crash restore: the ledger was
gone in 85 ms and both windows were revealed with identical rects. The CLI
`restore` path refuses while the watcher holds the store (documented
behavior); after the watcher exited it reported
`{restored:true, windows:0, ledger_cleaned:true}`. The watcher exited, no
owner or fixture process remained, settings bytes were byte-exact, and SPI
read back `{arranging:1, pen:35, abm:0}`. No mismatch was observed.

Limits: this crashed the hidden-window/ledger recovery path. Maximized and
normal fullscreen R12 state preimages are crashed separately in Task 9 above;
none remains TBD for the supported rows.
The CLI restore refusal while the watcher holds the lease is the documented
single-store behavior, not a failed restoration.

### Task 9 targeted live verification (2026-11-25, final tree, above:true build)

All legs ran fixture-only under the fixture-only scope. The first legs ran on
the working tree at `d064a653c2` whose only capability change was
`windows_capabilities().above = true`; that proof code was committed as
`717ff7e187`. The matrix and the closing fullscreen/R12 legs ran after the
observation-skip trace landed, on `b533c7e1b5` (owner sha
`E911C336…0069`). Each leg recorded the exact owner binary hash, the preflight
absence of another owner/ledger, and byte-exact settings restore. SPI/taskbar
state was not modified by these legs.

- **Production reveal (strict hide→show).** Evidence
  `target/task9/above-iconic-20261125-164411-9972/evidence.json`, owner sha
  `8715AFB0…365E3`. A native workspace departure hid the target
  (`visible:false`) and the return produced **exactly one** production
  `workspace-reveal` row (`w24`, tick 24, correlation `tick-24`,
  `outcome:"revealed"`, elapsed 6 ms), native `visible:true`, `cloak:0`,
  `iconic:false`, and no second row after 900 ms. The off-view iconic sibling
  produced zero reveal rows and stayed `cloak:0`. This closes the earlier
  diagnostic gap: the row is emitted on the production path, not under
  `cfg(test)`.
- **[R12 crash/restart twice](../decisions/contract.md#native-writes-difficult-to-undo-r12) with above durable intent.** Evidence
  `target/task9/r12-restart-20261125-165332-16652/evidence.json`, owner sha
  `AACA18CA…57D6`. Two exact-PID crash/restart cycles produced one admitted
  row per new session, the member tag stayed `c9bbb36d8ddae0dc` across three
  sessions, `max_rows` stayed 1, both boots kept the expected topmost band,
  release cleared the rows and topmost, no fullscreen rows were orphaned, and
  settings bytes restored.
- **[R20 anchored border](../decisions/contract.md#anchored-border-r20) and DPI independence** ([visual reference](../decisions/visuals.md#anchored-active-border-v4-r20)). Evidence
  `target/task9/r20-border-20261125-172846-20720/evidence.json`. On the
  non-125% output (96 dpi) the ring was sampled from the composed screen DC at
  **8/8** straight-edge points with `style_px [3,0,6]`; on the 125% output
  (120 dpi) a snapped half-work frame (requested physical client `1280×1332`, observed `1280×1333`) was
  sampled at **8/8** points with `style_px [4,0,8]`. In both stages the
  logged overlay outer equaled the live DWM visible frame expanded by the
  core outsets (±1 px), and the ring was refreshed only through a genuine
  hide→show cycle (minimize/restore plus a real fixture focus, retried until
  the core reply and observation joined). The sampled pixels carry the
  configured hue at the coverage the compositor gave them (DWM edge
  shadow/coverage scales all channels equally); the assertion is a
  proportional match with a coverage floor, never an arbitrary near-colour.
  This proves the ring follows the live frame and scales per monitor rather
  than inheriting 125%-only metrics. Limits: the leg floats the target to
  preserve an exact snapped frame; a tiled half-work frame is retiled by the
  product, so the snapped case uses a floating member.
- **[R12](../decisions/contract.md#native-writes-difficult-to-undo-r12) fullscreen chrome preimage crash.** Two exact-PID crash/restart legs
  on `b533c7e1b5`, both PASS with settings bytes restored and no orphaned
  metadata:
  - Normal entry: `target/task9/r12-state-20261125-180527-6252/evidence.json`.
    The durable row recorded `wasMaximized:false` with the pre-fullscreen
    outer `-8,0 2576×1388`; the crash kept the row and marker with chrome
    stripped; restart recovery cleared the row and marker, restored
    caption/thickframe, and moved the window back to the exact tile vis
    `0,0 2560×1380`.
  - Maximized entry: `target/task9/r12-state-20261125-181149-16708/evidence.json`.
    The fixture maximized the member (WS_MAXIMIZE) before the product
    fullscreen toggle, so the durable row recorded `wasMaximized:true` with
    `frame:null`. The crash retained it; restart recovery cleared the row and
    marker, restored chrome, and reasserted maximize (native style carries
    WS_MAXIMIZE, fixture reports `maximized`).
  - A separate probe at
    `target/task9/r12-maxprobe-20261125-181209-13692/evidence.json` records
    the same maximized entry before the crash and the concurrent
    `world-observation-skips` reason (`fullscreen`) for the token.
- **Readable-domain fullscreen overlay and skip reason.** The current
  all-stages fullscreen leg (`target/task9/fullscreen-20261125-181248-12724/evidence.json`)
  passes the application-fullscreen stage with an `untouched` row for the
  fullscreen window (allocation `0,0 1280×1380` unchanged, `frame_op:null`),
  zero held scopes, zero frame writes to it, and a monitor-sized DWM frame.
  A `world-observation-skips` row names the same token with reason
  `fullscreen`, so the retained-overlay lane is diagnosable instead of a bare
  `unreadable` hold. The earlier Task 5 `unreadable` hold does not reproduce
  with the current tree; if any skip recurs, the changed-set trace now names
  the token and the closed-vocabulary reason (unit-tested for emission,
  dedupe, clear, and change).
- **Full phase4 stress/delay matrix on `b533c7e1b5`.** All six supported rows
  PASS (`mise exec -- just world-stress -Case <case> -DelayMs <n>`), each with
  102 publications, 61 burst gaps, `status=pass`, and
  `world-stress final cleanup ok`:
  - `home-away` 0 ms: `target/windows-world-stress/20261125-181457-948`
  - `home-away` 10 ms (added 1490 ms): `20261125-181546-8848`
  - `home-away` 50 ms (added 7450 ms): `20261125-181643-17224`
  - `trio-churn` 0 ms: `20261125-181745-7368`
  - `trio-churn` 10 ms (added 1560 ms): `20261125-181842-23668`
  - `trio-churn` 50 ms (added 7800 ms): `20261125-181939-18168`
  One transient `hide-L09` failure preceded the first `home-away` pass at the
  same HEAD (`20261125-181329-18916`: checkpoint reported a managed
  foreground where null was expected); the immediate rerun passed and the
  other five rows passed first-try. No supported row is TBD.
- **Fullscreen and stale-fullscreen auto-retire.** Evidence
  `target/task9/fullscreen-20261125-181248-12724/evidence.json`; all stages
  passed. Enter/exit produced the expected `state_attempt` rows and native
  caption/thickframe restoration. The stale stage re-entered product
  fullscreen, then restored the external chrome style (`0x00C40000`) without
  any exit command: the adapter logged `fullscreen-retired` at tick 26,
  removed the owned marker, cleared the durable preimage (0 rows), restored
  the style, and retiled the window at tick 26 with zero pending
  transfers/operations. The remediation is therefore **reintroduced in the
  Windows adapter** (detected-on-sweep, not silently subsumed by core); core
  does not need to own the native style sweep. This corrects the earlier Task
  5 narrative: the strict hide→show transition is proven by the reveal leg,
  the readable-domain `untouched` overlay row by the app-fullscreen stage
  above, and stale-retire here.
- **`state_attempts` audit.** The final-tree logs carry `above` target
  `true` on float and `false` on release (one each), fullscreen target `true`
  on enter and `false` on exit, and no state attempt for the iconic or
  off-view sibling. No redundant or spurious state attempts appear in these
  runs.
- **DWM visibility, selection and pager.** The reveal leg records native DWM
  visibility/cloak/iconic and the selection return; pager host/cell/icon rows
  were produced in the same run (no pager crash or missing cell in the
  settled rows). Drag preview was **not** exercised: no input driver was
  used in Task 9, so drag-preview `id` gating remains covered only by unit
  tests and is recorded as a residual rather than live-proven.
- **KWin reference and Windows attestation.** The KWin anchored-border
  behavior remains the reference label for R20: a frame-anchored ring whose
  width/radius scale with the output, drawn outside the live visible frame.
  The Windows implementation reproduces that geometry and scales it per
  monitor (3 px at 96 dpi, 4 px at 120 dpi), and the same decision-register
  R20/R14 references apply. This evidence is Windows-host attestation, not a
  KWin equivalence claim beyond the shared geometry contract; no display
  identity values, host paths, or user window contents are recorded, and all
  fixture windows, owners, ledgers and settings were cleaned up.

Remaining TBD (explicit, not claimed):

- R12 state preimages now include the fullscreen chrome path: normal and
  maximized entry both crash and restart cleanly (Task 9 evidence above),
  in addition to the twice-crashed above-band intent.
- Fullscreen: the safe fail-closed held-scope row passes, the stale-retire
  remediation passes (Task 9), and the readable-domain `untouched` row plus a
  named skip reason are now live-proven. The former `unreadable` hold did not
  reproduce; game/anti-cheat acceptance remains user-owned.
- Live settings refused revision: no live fixture seam exists; unit tests
  cover the refused path. The startup, live `restart-required`, and restart
  adoption rows are proven above.
- Strict iconic hide-to-show is now proven by the Task 9 reveal leg (one
  production `workspace-reveal`, honest DWM visibility, no duplicate). The
  older iconic-subject minimize hold remains documented above, including its
  former `SW_SHOWMINNOACTIVE` limit.
- Full phase4 stress matrix: all six supported `home-away`/`trio-churn`
  delay rows pass at `b533c7e1b5` (paths above); no supported row is TBD and
  no unsupported row is claimed.
- Drag preview: no input driver was used, so `dragPreview.id` gating is
  unit-tested only; live drag interaction remains a residual.
