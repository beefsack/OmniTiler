# V4 phase 4: Windows port

## Purpose, status and gates

- Working plan and review record for porting `tiler-windows` onto the supported
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
- **T5 (Truthful incremental capabilities):** Advertise `above: false` in
  `windows_capabilities()` until its actuator and R12 preimages exist.
  `warp` is now `true` through `WARP_SUPPORTED` after the stage-2 adapter and
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
  flip `above: true` once proved), report honest `StateAttempt`s through
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

### Task 5 live evidence (2026-11-25, clean HEAD 42558707)

Run only from the clean post-Slice-A tree; fixture-only scope, cleanup reported
by the runner in every case. No display identity values or host paths are
recorded here.

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
off-view, so the minimized-preserving reveal path (`SW_SHOWMINNOACTIVE`) is
not exercised; moving the iconic window itself as the sent subject is also
unproven. The strict stress harness still cannot encode this row without a
minimize/restore observation op and a minimized-aware oracle.

Not run in this evidence (explicit TBD, not claimed):

- Multi-monitor/mixed-DPI transitions: the stress harness is single-output by
  contract and refuses a multi-screen report; no multi-output fixture seam.
- Forced-crash/R12 recovery: no isolated throwaway session was used; the
  bounded owned-resource crash lane is unrun.
- Fullscreen/game safety: no fixture op creates fullscreen or paused-game
  conditions; unrun.
- Live settings revisions (refused and `RestartRequired`): no fixture op
  drives revisions through the live owner; unrun.
- End-to-end positive warp on multi-output choreography: the single-output
  harness refuses multi-screen, and the C3 live receipt only proves the
  witness/setter property, not a core-minted cross-output warp; unrun.
- Iconic window as the moved subject and the minimized-preserving reveal
  (`SW_SHOWMINNOACTIVE`): unproven. The iconic-resident mover-send evidence
  above is manual; the strict stress schema has no `minimize`/`restore`
  observation op, and a minimized-aware oracle is still needed before the
  leg can be encoded there.
