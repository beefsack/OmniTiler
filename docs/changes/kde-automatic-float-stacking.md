# KDE automatic-float ownership and dialog stacking

## Goal and boundaries

- Automatic fixed-size floats and transient dialogs stay above tiled windows
  through focus changes and retiles. Native keep-above ownership follows
  [R12](../decisions/contract.md#native-writes-difficult-to-undo-r12): persist the
  original state before acquiring above, release owned above on explicit tile,
  and restore leftovers on the next start. Pre-existing above stays untouched.
- User decision 2026-10-18: an automatic float with keep-below already set
  receives no stacking writes. Pre-existing keep-above remains untouched.
  Manual Meta+G keep-below clearing/restoration remains unchanged.
- User 2026-10-29 approves: user/app keep-below set after admission wins; release only
  project-owned keep-above. Game safety wins on conflict.
- Current outcome: bounded fixture behavior **PASS, 2026-11-16**; restart
  collection is process-qualified. Full application/physical acceptance remains
  **PARTIAL**. No production fix or Windows/macOS/shared-core change resulted
  from these native checks.

## Acceptance and approach

- Confirm public Wayland keepBelow scripting support in the pinned KWin source.
- Demonstrate regression failures before production edits; cover admission,
  retained classification, ownership, explicit tile/unfloat, teardown/restart,
  later keep-below, and fullscreen safety.
- Reuse the existing keep-above restoration mechanism without changing manual
  float policy. Inspect ownership across exact native-reference replacement.
- Run the KWin suite, typecheck, build, and independent review.
- Current native checks follow the [live testing posture](../live-kwin-testing.md#testing-posture-user-2026-11-15)
  and fixture-first rules. Exercise disposable fixed-size floats and parent-bound
  dialogs through native focus, tile/unfloat, retile, close and same-namespace
  restart. API focus is not physical Alt+Tab/click acceptance. Real application,
  Steam and game checks remain user-observed; unsupported outcomes remain TBD.
- Amend D8, REQ-SPC-04h and R-FLT-12 after verification; retain only accepted
  evidence and remaining native/user checks in the outcome.
- Port regression requirements, not the discarded PlanAdapter implementation:
  pre-existing above gets zero writes/restoration; pre-existing below gets zero
  stacking writes through admission, tile and teardown; later below releases
  owned above without writes; closed/replaced refs receive no cleanup writes.
  Only the fresh ref may acquire ownership. Explicit unfloat releases owned
  above once, tiles once with suppression and never writes below.
- Production-entry tests assert automatic keep-above exactly once, with zero
  frame, focus or keep-below writes; retained/fullscreen observations add no
  stacking writes. Mocks mirror KWin above/below exclusivity.
- Scope lifecycle tests to the selected world-route unload policy. Legacy
  disable-path restoration is not evidence or a decision for script unload.

## Historical implementation and decisions, 2026-10-19

- This section describes the earlier world adapter, not the current v4
  implementation. Its local automatic-above mechanism was subsequently removed;
  current ownership is core-targeted with adapter preimages and recovery.

- `entry.ts` starts only the world adapter. The discarded legacy stacking
  attempt and `0bc59d07` are not shipped-route evidence; no legacy source was
  changed here.
- `world-native.ts` now maintains automatic keep-above separately from sticky
  ownership. Pre-existing above/below is untouched. All writes resolve a live
  stable ID and recheck native overlays, never a stored wrapper; QML array-like
  inventory is covered. Closed references receive no cleanup writes.
- Explicit tile releases owned above; the existing suppress pin prevents
  reacquisition. No automatic keep-below, frame or activation setter is added.
- `world-apply.ts` preserves focused automatic floats against `af-` admission
  effects without suppressing explicit `fx-` commands. Automatic fixed targets
  themselves refuse admission activation, including a real-shaped floating
  reply row. Correlated attempt/result diagnostics separate setter acceptance
  from native active readback.
- User 2026-10-29 approves: later below or external above-clear wins for
  the live window; loss of automatic classification restores owned above once.
  Same-scope focused fullscreen suspends owned above and exit reacquires it;
  live overlay targets receive no stacking writes, including restoration.
- User 2026-10-29 approves: explicit `handle.stop()` best-effort restores
  safe owned above, retaining refused ownership for another explicit call. This
  is not actual KWin unload restoration.
- Source checked locally at KWin `v6.7.5`: `src/window.h` exposes writable,
  script-readable `keepBelow`; `src/scripting/scripting.cpp` exposes only selected
  globals and deletes scripts in `AbstractScript::stop` / `unloadScript`, with no
  JavaScript stop hook. `entry.ts` does not call `handle.stop()`.

## Historical offline verification

- Initial regressions: 15 failures before implementation, then 16 passes.
- Review identified explicit-focus suppression, stale overlay writes,
  classification-loss cleanup and conflated focus diagnostics; corrected
  fixtures reproduced nine failures, then 28 passed. Unused stored references
  were removed. Actual-unload ownership was unresolved at that point; the
  current R12 next-start contract and native evidence below supersede that gap.
- Native admission exposed a further producer mismatch: the first helper's
  `windows: []` reply missed the floating row plus `af-` intent. A real-shaped
  regression reproduced one forbidden activation; a separate regression
  reproduced ownership loss after refused explicit stop. Both fail before the
  corrections and pass after. Suppressed-tile and ordinary/capacity admission
  activation remain covered.
- Final integrated gates: KWin **1951 passed**, Rust workspace **3353 passed**,
  zero failures; typecheck, script build, Rust formatting and strict all-target
  workspace Clippy pass. These gates include the existing surrounding changes.

## Historical native samples and restoration limits

- These earlier samples are retained as historical evidence, not current
  acceptance. Their baseline/capture shortcomings are not prerequisites for
  fixture-scoped checks under the current live testing posture.

- Genuine fixed-size Qt windowed stand-in: fixed hints verified, frame unchanged
  over two samples, keep-above acquired once and retained. **Not accepted**:
  the sampled build issued one admission-focus write. The offline correction
  above was made afterwards and has not been repeated natively.
- Focused fullscreen editor stand-in: fullscreen, focus and frame retained;
  no controller emissions during the sampled interval. The prior float had
  already closed: this does **not** prove concurrent float/fullscreen suspension
  or game coverage safety.
- Genuine Qt window-modal dialog plus disposable terminal on the same
  workspace: modal/above flags retained and two API focus switches produced
  all-zero controller actuation summaries. This is an Alt+Tab-style focus
  cycle, not physical shortcut or visible stacking-order proof.
- At that time pre-existing above/below, later below, explicit tile, restart and
  concurrent game-suspension coverage were offline only. The bounded above,
  tile and restart evidence below supersedes only those arms; below, concurrent
  fullscreen/game and Steam acceptance remain open.
- Captured configuration bytes, shortcut tuples, float-store absence, loaded
  effects, desktop inventory and protected placement were retained. Final
  protected frame/state/membership/output/focus, shortcut tuples and effects
  match; all disposable windows closed, controller unloaded, Planner unowned,
  no keep-above survivor. Script configuration bytes match; float store remains
  logically empty (initial file absence was not independently rechecked).
- **Full restoration not established**: pointer was not captured, per-output
  view reads were null, and the run proceeded despite those baseline gaps.
  Configuration bytes differ by retirement of two original empty desktops;
  surviving IDs/names/order match, original count does not. No exact baseline
  or full live acceptance is claimed. No further native mutation followed.
- Correlated summaries and native readbacks were retained, but complete raw
  request/reply capture was not established. Summary logs cannot replace that
  gate. This limits those historical samples, not the separate evidence below.
- Historical restoration follow-up, 2026-10-19: both retired empty desktops were
  recreated at captured positions with captured names/order and new native IDs;
  protected terminal membership/frame/focus stayed unchanged and the controller
  stayed stopped. Only replacement IDs and default tiling groups changed.

<a id="blocker-and-exact-next-action"></a>

## Bounded native verification, 2026-11-16

- R12 answers the former unload/restart ownership question: durable original
  state outlives the controller, and the **next start**, not script unload,
  restores leftovers. A surviving owned float can remain above while unloaded;
  recovery restores false before a fresh automatic target reacquires true.
  An armed original may remain after explicit unfloat until pruning/recovery;
  slot disappearance is not the unfloat oracle.
- Native evidence joins fixture process/window identity to Planner targets,
  KWin attempts, durable preimages and native readbacks/signals. Stacking indices
  are bottom-to-top, not merely a raised appearance.

| Leg | Verdict and native oracle | Raw evidence ladder |
| --- | --- | --- |
| Automatic fixed-float acquisition, release and close | **PASS.** Fresh fixed float acquires above and stays above focused/reflowing tiles; explicit tile restores native above=false and tiled geometry. A second owned float closes and its slot is pruned. | `native-auto-above-20261116T070202Z/planner-dev.log:11,51`: target true -> applied revision 5; `kwin-tiler-journal.log:315-317`: original persisted before native write; `07-native-acquired.json:87-108`: above=true, index 10 above tiles 5/6. `planner-dev.log:130-147` and `kwin-tiler-journal.log:603-609`: explicit tile/release; `10-native-a1-tiled.json:87-108`: above=false and tiled frame. `planner-dev.log:157,168` and `kwin-tiler-journal.log:694,696,781`: fresh acquisition then close/prune. |
| Pre-existing above across float, tile, close and restart | **PASS, bounded ownership proof.** Native above remains true through float/tile/refloat; close removes only the fixture. No controlled target, attempt or preimage for the pre-owned window. Restart witness records no above change for it. This is source guards plus target/attempt/preimage silence, not independent interception of every setter/no-op. | Acquisition bundle `13-native-u1-tiled.json:39-60`, `14-native-u1-unfloated.json:39-60`, `15-native-u1-closed.json:14-109`; restart bundle `r12-auto-above-20261116T071039Z/kwin-full-journal.log:1092,1128`: pre-owned `stateOps=-`; `10-witness-events.json:1-46` and `11-witness-events.json:46-65`: changes belong only to the project-owned float. |
| R12 combined Planner/controller restart, then Planner-only reload | **Behavior PASS; collection process-qualified.** Same bus/KWin namespace and surviving native identities. Each recovery observes native false, clears the restored slot, then persists a fresh original before automatic reacquisition to true. | `r12-auto-above-20261116T071039Z/09-preimage-unloaded.json`: original above/below=false remains armed after unload. `kwin-full-journal.log:898,912-915,945-946,989-992`: seed -> recovery -> native false -> clear -> stored 0 -> stored 1/native true. `10-witness-events.json:1-46` positively witnesses false then true. Planner-only: `kwin-full-journal.log:1040,1054-1057,1079-1080,1114-1115`, `11-witness-events.json:46-65`, `11-planner-final.log:8,15,20`. |
| Parent-bound resizable XWayland dialog above parent and tiles | **PASS.** Genuine dialog/transient relationship, not a fixed-window substitute. Dialog frame unchanged; above=true and stack index greater than parent/all tiles through API focus parent -> tile -> dialog -> other tile, and tile spawn/close reflows. | `transient-dialog-above-20261116T072128Z/05-native-staged-pre-tiler.json:85-106`: dialog/transient=true, exact parent, initially above/below=false. `kwin-tiler-journal.log:21,126-130`: automatic float without frame/allocation authority, persist -> above attempt accepted. `planner-dev-current.log:11,41`: target -> applied revision 4. `06-native-acquired.json:87-108`, `07-focus-P-native.json:12,87-108`, `08-focus-T1-native.json:12,87-108`, `09-focus-D-native.json:12,87-108`, `10-focus-T2-native.json:12,87-108`, `11-retile-spawn-native.json:63-132`, `12-retile-close-native.json:63-108`: unchanged frame/parent and above ordering; spawn adds a fourth tile and halves a tile's width, close restores it. |

### Progress discriminator and collection limits

- Initial admission attempts were stale while world revisions advanced, then
  applied at revision 5 (fixed float) / 4 (dialog). This is not a retry loop at
  one immutable tuple. A fixed generation/window/target revision and attempt
  without native progress is a stop, not a reason to repeat the same attempt.
  No product nonprogress or failing product oracle was reproduced, so no
  production edit or offline regression fix was needed.
- Restart captures 07/08 did not prove the native transition: the first witness
  was removed before delayed rediscovery, the second missed callbacks despite
  recovery logs. Callback lifetime/GC was a hypothesis, not established cause.
  A top-level keepalive preceded the accepted positive signal captures.
- After the successful restart proof, unloading an older witness while a newer
  controller survived allowed a reused numeric script path to hit the controller;
  the read helper timed out. Exact-name helper cleanup and later controller
  teardown succeeded. The planned `11-native-post-reload.json` does not exist;
  the continuous signal witness and post-stop native read support the bounded
  oracle instead. This collection deviation remains disclosed, not a clean-run
  PASS. Planner tracing was lost through reload; KWin Describe rows and native
  signals, not absent core traces, establish recovery.
- Initial transient-property staging encoded CARDINAL, not WINDOW. Corrected
  staging used `XSetTransientForHint`; native parent/type readback verifies the
  accepted setup. Exact staging-command execution and parts of log-preservation
  ordering were reported rather than independently exported. Saved ordering
  receipts are not a complete executed-command transcript.
- All accepted bundles end with controller DOWN, fixture process/socket absent
  and file-backed helpers unloaded. Restart cleanup includes the timed-out helper;
  it does not claim every helper invocation succeeded. Logs were preserved before
  stop; historical runtime-log copies are not the current-run evidence.
- No compositor restart or changed bus/KWin namespace was exercised. No new
  recovery authority across that boundary is claimed. Pre-existing/later below,
  concurrent fullscreen suspension and physical input remain outside these
  fixture oracles.

### Current verification gates

- Rust formatting, all-target build and strict all-target Clippy pass for the
  selected core/protocol/broker crates. Selected Rust tests: **3455 passed**,
  zero failures; one excluded (`workspace_unsafe_posture`).
- KWin typecheck and test/build pipeline pass: **1351 passed**, zero failures.
- `scripts/dev-loop-split.test.sh`: **703 passed**;
  `scripts/dev-native-effect.test.sh`: **204 passed**, zero failures.
- Documentation-only delta; frozen fixture families and backlog unchanged.

## Remaining acceptance and exact next action

- **PARTIAL; keep this note active.** Native fixture ownership and dialog
  stacking prerequisites pass with the restart collection qualifications above.
  They do not close AS/P0 or establish whole-desktop restoration.
- Next: user-run original Steam reproduction, retaining native state/identity
  and stacking evidence rather than appearance alone.
- Then real application modal/transient dialogs: parent and other tiles remain
  below through actual Alt+Tab/click focus changes and spawn/close retiles.
  API activation does not establish application-specific physical input behavior.
- Real fixed/windowed game plus a concurrently focused fullscreen game:
  observe enter/hold/exit, suspension/coverage safety and preserved user/app flags;
  no interference or stuck above after return. Keep pre-existing/later below
  permutations open until separately observed. No result for these apps or games
  is inferred from the fixture.

## Backlog detail moved 2026-11-01

### Original backlog lines 837-848

````markdown
- P0 | KDE floating dialog pushed behind other windows | Implemented offline
  2026-10-19 on the shipped world route: option A keep-above, focused automatic
  float preservation and correlated diagnostics; no legacy edits, Windows
  unchanged. D8 / REQ-SPC-04h / R-FLT-12 amended. KWin **1951**, Rust **3353**
  pass; strict gates clean. Native stand-ins exposed an admission-focus write,
  now fixed offline; corrected live repeat remains. **BLOCKED**: actual unload
  leaves owned above without recoverable ownership across restart; matching
  existing world behavior is unsafe. External lifecycle ownership question,
  incomplete baseline restoration (pointer/views, two retired empty desktops)
  and capture limits are in the [record](changes/kde-automatic-float-stacking.md).
  Concurrent fullscreen/game, final modal/terminal checks and user's Steam
  retest remain pending; no live acceptance claimed.
````
