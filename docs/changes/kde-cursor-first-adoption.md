# KDE cursor-first targeting adoption

## Goal and boundaries

- Adopt the existing [output-targeting decision](../decisions/layout-and-admission.md#output-targeting)
  through KDE's current settings/catalog route and command ingress.
- Shared core already resolves cursor-first selections and three-way admission;
  sends retain subject-output authority. No core policy or protocol change.
- Preserve default-off hover, unsupported KDE cursor warp, scope and native
  safety fences. Explicit-send current-view sampling remains separate.
- No macOS or Windows adoption changes; their shared-core targeting is unchanged.

## Acceptance and bounded work

1. Directional focus with no native foreground submits the existing empty subject.
   Other commands retain their subject requirements and scope fences.
2. Normal KDE settings carry `switchTarget`, `freshPlacement` and
   `focusFollowsCursor` with catalog defaults and existing revisioned delivery.
3. Expose those three fields through the existing exporter, generated schema and
   native controls. Preserve Save/reconfigure proof and shortcut ownership.
4. Verify fixture-only [PX legs](kde-v4-acceptance-checklist.md#additional-kde-acceptance-and-diagnostic-legs):
   injected cross-output hover, separate send discriminator, empty-output
   close/hide/switch/send, sole minimize/no-focus navigation and restore.
   Unreachable routing is BLOCKED; unknown visibility is not hide proof.
   Human feel, real applications and display changes remain outside this evidence.

### PX follow-up

- Discriminate failed pointer injection from cursor sampling by comparing
  independent KWin cursor positions before/after relative motion with adapter
  samples. Stationary KWin positions leave physical hover blocked; divergent
  samples require a bounded adapter correction before hover retesting.
- For empty directional focus only, KWin's desktop-type foreground counts as
  no focused client. Other foreign foregrounds still refuse; native foreground
  reporting remains truthful and no native clear-focus is introduced.
- Cover that ingress distinction with regressions, independent review and the
  existing offline gates, then repeat no-neighbor and adjacent-output legs.
  No protocol change, stored user settings or frozen fixture edits.
- Before those live legs, extract the required script/fixture/input/teardown
  operations into tested importable repository helpers under the
  [live tooling rules](../live-kwin-testing.md#live-tooling-user-2026-11-17).
  The short external procedures import them; copied lifecycle code is not used.

## Verification

- Verification coverage: command-ingress regressions, Rust format/build/strict
  clippy/tests, KWin typecheck/tests, and both launcher test suites.
- Preserve frozen world-wire v4, stage5, stage5-v4 and kde-v4 fixtures.
- Native evidence follows the [live guide](../live-kwin-testing.md) and
  [isolated settings procedure](../dev-loop.md#isolated-fixture-settings), with
  joined fixture identities, raw log citations and complete teardown receipts.

## Accepted evidence

- Source inspection: selection resolver in
  `crates/tiler-core/src/world_tiling_views.rs` and execution-cursor regressions
  in `world_logical_contract_tests.rs`; three-way resolver in
  `cursor_targeting.rs` and staged v4 admission tests in
  `world_observe_capture_tests.rs`; subject-home send source and relative ring
  in `world_tiling_views.rs` / `world_commands.rs`.
- The active KDE v4 report encoder omits legacy observer targeting declarations;
  settings use the separate revisioned connection lane. Normal runtime settings
  and native controls now carry the three approved fields through the existing
  catalog/descriptor route.
- Existing hover-motion subscription is settings-acceptance gated; this work
  adopts its normal settings surface, not another motion mechanism.

### Genuine no-focus shortcut routing

- Directional shortcuts now submit the existing empty subject only when native
  foreground is genuinely absent. A scope-filtered foreign foreground still
  refuses; other windowed commands retain their subject gates.
- The first minimal patch treated both null filtered-ID cases as absence and
  failed the existing foreign-focus ingress regression. The correction preserves
  a raw foreground-existence check for shortcuts while native toggles retain
  their scoped reference getter. No protocol or core behavior changed.
- Regressions cover the registered shortcut's exact empty-subject queue payload,
  foreign-focus refusal, in-scope implicit subjects and no-subject move refusal.
- Review found no serious issues. Final offline verification passed: Rust format,
  three-package all-target build and strict clippy; Rust tests 3,460 passed,
  3 ignored; KWin typecheck and 1,361 tests; launcher 703 and native-effect
  launcher 204 checks. Frozen fixtures and core/protocol source were unchanged.
- This establishes command ingress only, not native no-focus navigation or PX
  acceptance.

### Desktop-foreground directional focus follow-up

- The approved KDE desktop-type exception now submits the existing empty
  directional-focus subject through both registered shortcuts and direct
  requests, including unscoped operation where the desktop has a native ID.
  Other foreign foregrounds still refuse. Native foreground reporting,
  scope filtering for other commands and core/protocol shapes are unchanged.
- The first candidate covered only scope-filtered desktop IDs; correction
  checks the raw desktop kind before choosing either empty or managed subject.
  Regressions cover scoped/unscoped desktops, strict non-true markers,
  foreign dock/normal/fullscreen refusal and exact managed implicit subjects.
- Independent source review found no blocking issues. Focused tests passed
  112/112 and test typechecking passed. Full offline gates passed: Rust format,
  three-package all-target build and strict clippy; Rust tests 3,460 passed,
  3 ignored; KWin typecheck and 1,376 tests; launcher 703 and native-effect
  204 checks. The tested source is committed as `732af3ae`; checks used the
  scoped Cargo commands, KWin `npm run typecheck` / `npm test`,
  `scripts/dev-loop-split.test.sh` and `scripts/dev-native-effect.test.sh`.
  Frozen fixtures were unchanged. The later fixture-only no-focus reruns below
  establish the desktop-foreground no-neighbor and adjacent-edge cases.

### Focused live helpers

- Extracted only the current legs' reusable script lifecycle, native fixture
  reads/joins, owned input and fixture processes, isolated dev startup and
  log-before-stop teardown. The scenario runner remains unchanged; external
  procedures import these pieces. A separate cursor-read command is available.
- Independent safety review found no blocking issues after correcting the
  helper load to target the captured KWin owner with a fresh identity check.
  Hermetic channel/native/input/fixture/dev tests passed 58/58 using
  `node --test scripts/kde-live-helpers.test.mjs scripts/kde-live-session.test.mjs`.
- Full offline gates passed again after extraction: Rust format,
  three-package all-target build and strict clippy; Rust 3,460 passed,
  3 ignored; KWin typecheck and 1,376 tests; launcher 703 and native-effect
  204 checks. The locked fixture build also passed. Frozen fixtures and
  core/protocol source remain unchanged.

### Normal runtime settings

- Normal configuration now carries `switchTarget`, `freshPlacement` and
  `focusFollowsCursor` through the existing revisioned settings lane. Defaults
  are cursor output, cursor-output/last-focused admission and disabled hover.
  All catalog targeting alternatives are supported; invalid or throwing readers
  fall back to those defaults. Cursor warp remains unsupported.
- Regressions cover defaults, alternatives, fallback, changed-content revision
  and acknowledgement, unchanged-content suppression, and normal configuration's
  accepted-hover subscription, disable and stop. Existing generation-rotation
  coverage remains. Isolated settings still avoid normal configuration reads.
- The first full KWin run found three current-producer byte comparisons still
  expecting omitted settings. Regeneration updated five active artifacts:
  the KDE settings request plus both Stage 5 discovery requests and replies.
  Only the three existing values and their missing-key diagnostics changed;
  protocol shape and frozen historical families were unchanged.
- Focused verification passed 46 settings/motion/connection tests and 40 active
  fixture tests. Final review found no serious issues. Full verification passed:
  Rust format, three-package all-target build and strict clippy; Rust tests
  3,460 passed, 3 ignored; KWin typecheck and 1,367 tests; launcher 703 and
  native-effect launcher 204 checks.

### Native targeting controls

- The exporter now exposes the same three catalog fields through two enum
  controls and one Boolean checkbox. Generated schema and descriptor entries
  preserve the existing keys, tokens, defaults and descriptor shape. Cursor
  warp remains unsupported; no core or protocol changes were needed.
- Load, dirty tracking, Defaults and exact-key Save include all three fields.
  Save requests live reconfigure without claiming application; failures retain
  the existing retry path. Only workspace mode requires restart. Hidden,
  foreign, shortcut and host settings retain their existing ownership.
- The first native run failed two new assertions that assumed malformed stored
  Boolean text used the false default. KWin and the native control both delegate
  storage decoding to KConfig; the corrected tests preserve that existing
  behavior while separately pinning missing-key false and actual Boolean type.
  Strict protocol and isolated JSON validation remain unchanged.
- Final review found no blocking issues. Verification passed: Rust format,
  three-package all-target build and
  strict clippy; Rust tests 3,460 passed, 3 ignored; KWin typecheck and 1,367
  tests; launcher 703 and native-effect launcher 204 checks. Exporter tests
  passed 14/14, strict exporter clippy passed, and all generated artifacts
  matched the exporter. The isolated settings-only native check passed 27/27,
  including targeting persistence, Defaults/Cancel, reconfigure and retry.
- Frozen fixtures were unchanged. Fixture-only PX acceptance and native
  keyboard/visual acceptance remain; these offline checks do not close them.

### Fixture-only PX evidence

- Follow-up inspection: `kwin/src/world-observe.ts:1637-1650` samples
  `workspace.cursorPos` freshly, rounds finite safe integer coordinates and
  returns unknown on unreadable/out-of-bound input; it does not synthesize
  `(0,0)`. Motion observation uses `cursorPosChanged` only after accepted
  enabled-hover settings. The earlier independent native helpers also read
  `workspace.cursorPos`; the later relative discriminator below establishes
  native motion, not agreement with the active adapter's sample.
  Active construction retains the live workspace object, not its initial
  cursor value (`kwin/src/world-entry.ts:418-432,2232-2233`).

- Relative discriminator after `96ad5635`: **PASS for native motion only**.
  `px-final-live.mjs --live` imported the reviewed helpers; bundle
  `px-followup-xSMnS5` retains the executed script and source revision.
  Independent native positions changed from `(372,997)` to `(584,1103)`
  (`relative-before.json:1`, `relative-after.json:1`, `events.jsonl:298`).
  The procedure stopped at the planned adapter-comparison boundary
  (`events.jsonl:299`); this is not stationary injection or hover acceptance.
  The physical single-key control reached callback/queue/capture
  (`0174-control-journal.stdout:1-15`). Raw adapter coordinate equality was
  not logged; the later hover legs below discriminate behavior instead.
- That discriminator's cleanup completed: key releases/unlocked state
  (`events.jsonl:301-307`), helper absence and runtime logs copied before stop
  (`events.jsonl:310-312`), unloaded controller/unowned Planner
  (`0222-just.stdout:1-9`), fixture exit and daemon exit
  (`events.jsonl:319-335`). `outcome.json:1` has no cleanup errors.

- No-focus reruns after `96ad5635`: `px-final-live.mjs --live` with the
  imported `px-followup-procedures.mjs`; bundle `px-no-focus-rTxFy3` preserves
  both executed sources and the source revision. Fixture joins and native
  before/after state support both **PASS** verdicts:

| Follow-up leg | Accepted result | Raw evidence within `px-no-focus-rTxFy3` |
| --- | --- | --- |
| No-focus no-neighbor | Desktop-type shell foreground admitted the empty subject. Physical left reached capture, found no adjacent output and minted no focus request or setter. A remained minimized; native fixture state and managed home/allocation were unchanged. | `sole-minimized-state.json:1`, `no-neighbor-after-state.json:1`; `0285-no-neighbor-after-journal.stdout:1-15,25-26` |
| No-focus adjacent edge versus MRU | Native frames and tuple allocations established `H[B,C]` on the adjacent output, with C most recently focused. After A minimized to desktop foreground, physical right submitted the empty subject and selected edge B, not C; the native setter was proven and native foreground became B. | `adjacent-staged-state.json:1`, `adjacent-mru-c-state.json:1`, `adjacent-no-focus-precondition-state.json:1`, `adjacent-after-state.json:1`; `0521-adjacent-after-journal.stdout:1-22` |

- No-focus teardown completed: key releases/unlocked state
  (`events.jsonl:693-700`), helper absence and logs copied before stop
  (`events.jsonl:703-705`), unloaded controller/unowned Planner
  (`0526-just.stdout:1-9`), fixture exit and owned daemon exit
  (`events.jsonl:712-728`). All 24 helper receipts recorded successful runs
  and unloads; `outcome.json:1` has no cleanup errors.

- Disabled cross-output hover after `96ad5635`: **PASS**. The imported
  `px-hover-procedures.mjs` driven by `px-final-live.mjs --live` used damped
  relative motion and fresh topmost-hit guards. In bundle `px-hover-OUhSUf`,
  `disabled-before-state.json:1` joins active/hit A on the left;
  `disabled-after-state.json:1` joins hit B on the right while A retains
  native focus. `0365-disabled-hover-journal.stdout:1-5` contains no native
  focus-setter attempt; `events.jsonl:492` records the bounded pass.
- The first undamped routing approach overshot into shell hits and was
  **BLOCKED**, not a hover-policy failure (`px-hover-L4JTFg/events.jsonl:429-535`).
  Damped motion resolved routing. The later enabled setup stopped before
  motion because a command was unavailable
  (`px-hover-OUhSUf/0454-arm-skip.stderr:1`); no enabled verdict follows from
  that pre-effect tooling failure. Both runs completed owned teardown without
  cleanup errors (`px-hover-L4JTFg/events.jsonl:536-571`,
  `px-hover-OUhSUf/events.jsonl:613-648`, each `outcome.json:1`).

- Enabled cross-output hover and retained request after `96ad5635`:
  **PASS**. `px-final-live.mjs --live` imported the same damped procedure
  for enabled-only collection; bundle `px-enabled-hover-j0zR5H` preserves
  both sources, the revision and isolated enabled settings.
  The existing skip hook was invoked through `busctl`, with no new dependency.
  `enabled-before-state.json:1` joins active/hit A on the left.
  A real cross-output motion put the pointer over B; the correlated skipped
  reply issued `focus-9` for B at revision 9
  (`0343-enabled-first-journal-0.stdout:5-16`). A stayed natively focused
  while hit B was observed (`enabled-after-skipped.json:1`).
  A second real motion republished the same `focus-9` and B target at revision
  10, retained `issuingRev=9`, and called the `focus-follows-cursor` setter with
  `proven=true` (`0388-enabled-hover-journal.stdout:18-34`).
  `enabled-after-state.json:1` independently joins active/hit B.
- Enabled teardown completed: full releases/unlocked state
  (`px-enabled-hover-j0zR5H/events.jsonl:521-528`), helper absence and logs
  copied before stop (`events.jsonl:531-533`), unloaded controller/unowned
  Planner (`0393-just.stdout:1-9`), fixture exit and daemon exit
  (`events.jsonl:540-556`). All 18 helper receipts passed; `outcome.json:1`
  has no cleanup errors. No stale/zero adapter bug was reproduced, so no
  production correction was needed. Exact raw coordinate equality is not
  claimed; the correlated hover behavior and native target are established.

- Tested after `f690409e`, with two adjacent outputs, joined XWayland fixtures
  and isolated default-off hover settings. Bundle
  `cursor-px-hover-9527623b5976` contains the raw evidence.
- Keyboard delivery was observed: `0157-single-control-journal.stdout:1-7`
  joins the physical focus-left callback, queue and capture. The edge result
  was `operation-refused` with zero effects and unchanged native focus
  (`:13-16`); this is not no-focus navigation evidence.
- In that earlier run, cross-output hover was **BLOCKED**, not a policy failure.
  Both injected mouse commands returned success, but native cursor samples
  remained `(0,0)` and
  the topmost hit never reached the target fixture
  (`125-disabled-before-native.json:1`, `149-single-control-after-native.json:1`,
  `171-disabled-after-native.json:1`, `events.jsonl:211`).
  Disabled-hover acceptance was not established; enabled hover and retained
  focus-request identity were not reached.
- Teardown completed: key releases and unlocked state
  (`events.jsonl:213-215`, `0180-lock-final.stdout:1`), runtime log copies before
  stop (`events.jsonl:218-221`), controller unloaded and Planner unowned
  (`0183-disabled-error-status.stdout:1-9`), fixture exit and socket absence
  (`events.jsonl:224-225`), helper absence/current-view restoration and daemon
  exit (`events.jsonl:243-266`). No helper remained (`outcome.json:1`).
  Empty-workspace roster churn was observed; exact roster restoration is not
  claimed.

- Independent native legs used bundle `cursor-px-remainder-38db1945acc0`,
  without further pointer injection or opposite-pointer claims:

| Leg | Verdict and native observation | Raw evidence within the remainder bundle |
| --- | --- | --- |
| Sole minimize/restore | **PASS**, bounded retention/restore only. Minimize retained A's workspace and frame with no managed focus; restore retained the same home/allocation. Native activation on restore was not a tiler focus write. | `157-sole-minimized-state.json:1`, `177-sole-restored-state.json:1`; `0179-sole-restore-focus-journal.stdout:2-10` has `cmds=0`, `focusReq=0`, zero effects and no focus-setter attempt. |
| No-focus no-neighbor and adjacent edge versus MRU | **BLOCKED**. In both minimize setups KWin selected `plasmashell`; raw native foreground was not absent. No no-focus directional key was injected and no foreground was artificially cleared. | `events.jsonl:189,453`, `375-adjacent-no-focus-precondition-state.json:1`. |
| Relative send | **OBSERVED**, not opposite-pointer acceptance. A changed workspace on its source output; the other output's current view and B/C stayed unchanged. | `426-subject-home-send-before-state.json:1`, `447-subject-home-send-after-state.json:1`, `events.jsonl:539`. |
| Explicit send sampling | **OBSERVED**, no policy selection. The native ordinal was not the logical current ordinal: the first numbered send actually moved A. Immediate empty selection followed by that numbered send then refused at ingress after managed focus cleared; it did not move B/C. Exact explicit-current-target sampling remains unverified. | `0470-explicit-send-current-after-journal.stdout:2-18`, `0507-immediate-explicit-send-after-journal.stdout:1-20,43,47-59`; native result `506-immediate-explicit-send-after-state.json:1`. |
| Sole-source close | **OBSERVED**. The source view stayed selected and empty, shell focus replaced A, and B/C remained on the other output. | `539-close-before-state.json:1`, `558-close-sole-after-state.json:1`, `events.jsonl:672`. |
| Sole-source hide/show | **OBSERVED** with actual fixture `visible:false`, not inferred from membership. X11 unmap removed D's KWin identity; show required a fresh PID/sentinel/UUID join. The source view stayed selected and shell focus replaced D. | `events.jsonl:756`, `626-hide-after-state.json:1`, `661-show-d-after-state.json:1`. |
| Empty selection and subsequent send | **OBSERVED**. Selection changed only the source output to an empty view; shell focus replaced D, with no managed subject. The later relative send refused at ingress and B/C did not move. Focusless switch settlement reported `complete/unproven`. | `693-empty-switch-before-state.json:1`, `714-empty-switch-after-state.json:1`; `0715-empty-switch-after-journal.stdout:1-25,56-65`; `0737-empty-output-send-after-journal.stdout:3,7-25`; `736-empty-output-send-after-state.json:1`. |

- Collection limits: the adjacent-edge guard mistakenly read tuple allocations
  as named properties, so its `edgeProven:false` is not a layout result. Raw
  allocations/native frames show B left of C
  (`309-adjacent-staged-state.json:1`), but the independent native-foreground
  precondition still blocked navigation. The numbered-send sample likewise
  cannot establish an explicit-current-target case from a native ordinal.
- Workspace membership exclusion and fixture `visible:true` after a workspace
  switch do not prove compositor hide/reveal; that visibility evidence remains
  open. These observations select no new empty-output or sampling policy.
- The remainder teardown also completed: log copies before stop
  (`events.jsonl:888-890`), unloaded controller/unowned Planner
  (`0739-remainder-status.stdout:1-9`), key release/unlocked state
  (`events.jsonl:893-895`, `0741-lock-final.stdout:1`), fixture exit/socket absence,
  helper absence/current-view restoration and daemon exit
  (`events.jsonl:900-942`, `outcome.json:1`).
- PX remains **PARTIAL**: exact explicit-current-target sampling,
  compositor hide/reveal and human/native-control acceptance remain
  open. No production code, protocol, frozen fixture or stored user tiler
  setting was changed by these live legs.
