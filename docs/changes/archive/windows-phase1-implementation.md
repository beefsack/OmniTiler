# Windows Phase 1 implementation

- Status: historical v1 record, archived 2026-10-24. Milestones and diagnostics
  below describe the superseded pre-Engine owner/lifecycle path, not current
  proof. Current Windows v2 contract: [world adapter
  guide](../../porting/world-adapter-guide.md) and [core
  closeout](core-world-model-rearchitecture.md#closing-outcome-2026-10-24).

- Goal: native owner/stop/restore and owned-window recovery, followed by physical
  Win+Arrow evidence before Engine-driven geometry.
- Milestone 1: `f767de8`, native ledger model/storage, Windows and Linux CI green.
- Milestone 2 (`c25c7d6`): non-hooking lifecycle, standalone restore, exact-owner emergency
  stop, owned tagged test window and native development/proof loop.
- Baseline: Windows 11 Pro x64 build 26200; one display (model redacted), `\\.\DISPLAY1`,
  2560x1440 at 170 Hz, 125%, origin 0,0. Multi-monitor evidence belongs to the
  other Windows test system; historical KDE display facts remain unchanged.
- Acceptance: four-package Windows build/test, fmt-all, strict clippy and
  whitespace; independent reveal/readbacks after graceful and forced owner loss;
  verified actor exit and no committed ledger or stop request at the end.
- Input, hooks, overlays, ordinary-app management and host-setting changes are
  outside milestone 2. Clean runtime and guest-only Win+L remain Phase 4 work.

## Implementation and simplification

- `native.rs`: one held-process open/liveness path for identity, integrity,
  waiting and exact-handle termination; KnownFolder session storage and Terminal
  ancestry. Identity is PID, creation time, executable, SID and session.
- `lifecycle.rs`: bounded run/ready/stop/emergency-stop/restore; commit before
  hide; recheck process and HWND lifetime tag before actuation and after reveal.
  Stop commands exit only the owner; independent restore retains recovery after
  either graceful or forced loss.
- `test_window.rs`: disposable ordinary tagged HWND, atomic receipt, inspect
  and exact-window graceful close. Owner and helper require medium integrity
  and exclude Windows Terminal and its descendants.
- `windows-dev.ps1`/`just/windows.just` (via the root `justfile`): copied payloads under `target/windows-dev`
  avoid running-image build locks. Explorer desktop broker uses
  `ShellWindows.FindWindowSW(...).Document.Application.ShellExecute`.
  Plain `Shell.Application.ShellExecute` is caller-context and cannot supply
  the required launch ancestry.
- Simplification: removed duplicate process-open code, unused PID accessor,
  custom helper painting and a marginal executable-name comparison test;
  consolidated snapshot/error mapping, repeated owner-stop/ledger checks,
  run-result JSON/logging and always-true restore outcome plumbing.
- Independent review accepted the recovery contract. Corrections retained typed
  snapshot refusals, canonical reveal identity checks, independent owner-death
  checks in hide journeys and full process identity in proof reveal readbacks.
- Kept class/parent/owner/topmost eligibility checks, held handles, lifetime tags
  and Terminal ancestry: they bind writes to the expected disposable window.
  `Win32_Graphics_Gdi` remains required by `WNDCLASSW`/`RegisterClassW` even with
  custom painting removed. Remaining tests cover identity, bounds and recovery.
- Normal `run` remains a bounded poll loop without a message loop.
  Recovery state: `%LOCALAPPDATA%/omnitiler/session-<id>`.

## Verification after simplification

- 2026-10-01: PS7 Core 7.6.6, stable rustc/cargo 1.98.1,
  `x86_64-pc-windows-msvc`.
- `cargo +stable build --locked` and `test --locked` for `tiler-core`,
  `tiler-protocol`, `tiler-kwin-effect-ffi`, `tiler-windows`: pass, 587 tests.
- `cargo fmt --all -- --check`, four-package clippy `--all-targets -- -D warnings`
  and `git diff --check`: pass. PowerShell parse and all four Just recipe dry-runs
  pass; new implementation files checked for trailing whitespace, LF and no BOM.
- Linux workspace tests/clippy and portable checks remain deferred to CI.
  Linux service, KWin consumers, shell/Nix and live KDE checks were not run here.

## Physical owned-window recovery evidence

- Earlier proof: PASS, 2026-09-30 23:44 +10, source `f767de8` plus milestone 2
  working tree. Report: `target/windows-proof/20260930-234429-13472/report.json`.
  No-hide graceful/forced exit then one helper hidden/restored after each kind
  of owner loss; independent reveal preserved process/tag/rectangle. Exact helper
  close passed; no actors, ledger or stop request remained.
- Earlier non-hiding launch failed at readiness because plain Shell.Application
  launched in the Terminal tree. No hide occurred. Causal repair to Explorer's
  desktop broker passed dev/trace/stop and the owned-window proof above.
- Re-proof procedure: one physical-desktop invocation of the existing proof,
  bounded to five minutes; no-hide graceful and exact-owner forced exit first,
  then the one Explorer-launched owned helper, with 600-second helper lifetime
  and 120-second hide-owner runs. Independent restore after each owner loss;
  read back identity/tag/visibility/rectangle, close the exact helper and verify
  actor exit plus absent ledger/request. No Terminal-tree window is a target.
- Re-proof: PASS, once on 2026-10-01 08:36:26-08:36:59 +10, physical Windows
  build 26200, source `f767de8` plus the simplified milestone 2 working tree.
  `just --justfile windows.justfile proof` (historical command) exited 0. Report and helper receipt:
  `target/windows-proof/20261001-083638-9232/{report,window}.json`.
- Actual SHA-256 (payloads under `target/windows-dev`):
  - Owner: `0A351F46163F28401C200FBB839A5E3F5E091830FD5A8DDA398EFA109D18BD95`
  - Helper: `69C09F7DD03DC2BDFEF22344F01DCFAACB231F3075D93AB0E38EF20041B34522`
  - Script: `17CAE731204DA5E4CF445C05FC51FD02C41AF359319CC23940B3F7EE7BFBC68C`
  - Justfile: `59DC899AF950C21D28A80465B661F54816237DD84620A79A980DB942F898DBB1`
  - Report: `5623F09456DBC5AE3AAF896E3B36A31438D862CC45F9091E57E35BB9BC82AD83`
  - Receipt: `10B570B41505EF81E5B13659748BB11B34AB26E9CC6642F8ED6151AB44398A24`
- Medium integrity, same account, session 1. Preflight had no project
  actors/ledger/request. Display query: one primary DISPLAY1, 2560x1440 at 169 Hz,
  logical bounds 2048x1152 and work area 2048x1104, consistent with the accepted
  baseline; no independent fresh scale readback claimed.
- No-hide owners: graceful 19668 (`01dd512c285b84a6`), forced 14572
  (`01dd512c28b0b155`); exact owners exited, independent restore cleaned zero
  windows. Hide owners: graceful 33132 (`01dd512c29348e96`), forced 18228
  (`01dd512c2991e756`); independent identity/liveness checks confirmed exit.
- Helper: 21068 (`01dd512c28fcaa69`), HWND 722736, tag `18da3a34b7a8ec88`,
  rectangle (200,200)-(840,680). Hidden after both owner exits; each standalone
  restore returned `restored:true`, `windows:1`, `ledger_cleaned:true` and read
  back visible with identical full process identity, tag and rectangle.
- Exact helper close and exit passed. Final checks: all five exact actor
  identities absent, zero project actors, `ready:false`, ledger.json and
  stop.request absent; only intentional run.log/ledger.lock and ignored proof
  artifacts remain. Report/receipt independently re-read; absent
  project actors/ledger/request rechecked. No restoration ambiguity or unowned
  target observed; Terminal-tree windows were never targeted.
- All five actors passed the script's Explorer-parent assertion. Parent PID/name
  is not persisted in the report, so parentage evidence is the live assertion,
  not an independent post-hoc readback.
- Native machine readbacks do not substitute for user visual/input acceptance.

## Earlier Sandbox preflight (closed)

- 2026-09-30: read-only identity payload in a guest with networking/clipboard
  disabled and exact read-only payload mapping. Provisioning then black surface;
  guest execution/integrity remained unknown. One scoped WM_CLOSE required a
  user-dismissed confirmation. Sandbox subsequently closed with no actors left.
- No host tiler/window mutation occurred. No capture/input fallback or retry.
  Preflight artifacts were removed; retained SHA-256:
  - Static-CRT binary: `35C772E11F6071879C0CB202B7CE9B268DA32D61B68D7C27DC8F3739C978DEEA`
  - Sandbox config: `6D9C816635EF431C91C4FFEF16F50BB74F90922666CDC4942A266397BBF78C54`
  - Guest script: `494D7C9196DE8E9684A90D5A8C1AACEB133C8F0DFF87F1F796653C6D4C5A8AE2`
  - Sandbox launcher: `7BB5667331572C8A725A3A799AE7DED2912648B4F6EB7380FB51864C804C3E36`
- Debug payloads import VCRUNTIME140/UCRT; static-CRT release imported OS DLLs
  only. This supports, but does not prove, clean runtime closure.

## Original Win+Arrow comparison spike

- Goal: compare `RegisterHotKey` and `WH_KEYBOARD_LL` on the disposable owned
  helper; keep input implementation isolated from product modules.
- Implemented in the throwaway `winarrow` module and `tiler-winarrow-spike`
  binary. A shared lifecycle callback retains the existing owner lease, medium
  integrity, Terminal ancestry and independent same-executable control commands.
- One method per invocation: thread message loop, at most 15 seconds waiting for
  physical helper focus, 40 seconds enabled and 20 seconds released. The launcher
  runs both methods with one helper, a 280-second overall budget and 240-second
  helper lifetime; copied payloads and reports live under `target/windows-winarrow`.
- Hook callback uses cached HWND/PID/lifetime-tag checks and in-memory chord
  classification only; geometry/readbacks and reports run outside the callback.
  Other focus, extra modifiers and injected events pass through. Actions move
  only the verified helper by baseline-relative 48-pixel offsets.
- Release failure exits immediately. Geometry reset uses `SW_SHOWNOACTIVATE`
  and exact process/tag/rectangle readback. Reports contain aggregate per-chord
  setup, delivery, consumption, repeat, action, release and restoration evidence.
- `WM_HOTKEY` has no raw down/up stream; those counts must be reported as
  unavailable for the registration method rather than inferred.
- `just winarrow` is the timed comparison entry point;
  `winarrow-stop <exact-run-directory>` is independent recovery. Recorded owner
  identity is published before input phases; an abort marker prevents continuation
  and coordinates the two controllers without duplicate ledger writes.
- Unacquired methods request no enabled-phase input. Reversal uses single taps
  with helper refocus between chords; holding native Win+Down could minimize the
  helper and redirect repeats to another foreground window.
- Final four-package Windows build/test (602 passed), fmt-all, strict clippy,
  whitespace, PowerShell parse, recipe dry-runs and real absent-PID parameter
  binding passed. Independent reviews covered filtering, release and recovery.
  Linux workspace/portable gates remain deferred to CI.

### No-key machine evidence, 2026-10-01

- Physical Windows build 26200, medium integrity, session 1 (account IDs dropped).
  No keys, synthetic input or focus-forcing were used; no physical shortcut
  acceptance is claimed.
- Full entry point: 10:29:43-10:32:10 +10, report directory
  `target/windows-winarrow/20261001-102943-27412`. Both methods completed;
  all four `RegisterHotKey` attempts failed with 1409 (already registered).
  Hook installation/unhook succeeded. Every chord count was zero; baseline and
  final helper identity/tag/rectangle (200,200)-(840,680) matched. Cleanup passed.
- Active-stop probe: 10:41:27-10:41:33 +10, directory
  `target/windows-winarrow/20261001-104127-active-stop-3448`. Explorer-launched
  owner 13724 (`01dd513d98587f06`), helper 18136 (`01dd513d981eefbb`), HWND
  1573976, tag `18da41042192dcc8`. Hook was installed while the verified owner
  was alive; independent `-Stop -RunDir` produced `stop-request`, successful
  unhook, zero chord counts and exact baseline readback, then restore and helper
  close. Both actors exited; `ready:false`, ledger/request absent, cleanup
  failures empty. Emergency termination was not exercised in this probe.
- SHA-256 of active-stop payloads and report:
  - Spike: `91DFC7597D44226402DFE3D6700073DA7E4D7F457752AF684228AAD7AFC082E3`
  - Helper: `93F1EF37B947277C3D6C43937272C2ADE4C610780579E3632CEC30EF99E52641`
  - Hook report: `99CDDB9FFE9C175AFD4C6ECB85D4A57C5C19BD20762AE379A0DC5A5070FCC00F`
- Tested launcher SHA-256: `8D33DE8D9A2C50A272BB5E4F99A4A5A8DF4EF0B6EBEF8E7D57766608FD172D04`.
  Subsequent launcher-only changes clarify setup failures/safe taps and explicitly
  call stop before normal restore. Parse, prompt-branch checks and idempotent
  native stop/restore passed. Actor preflight/end readbacks also reject remaining
  matching actors without attempting name-based cleanup. Current launcher SHA-256:
  `39A224CC3F5529B69D5EED541A416286314F9C44919717B6BC3002CC1577B635`.
- The final callback additionally checks current Ctrl/Alt/Shift state on physical
  arrow-down, covering modifiers held before hook installation. Final native gates
  remain green (602 tests). Spike SHA-256:
  `0D4B444A8256CD0DD876275A7D2F3203B842F1586EC5AADB672C6F5A1C604C76`.
- Later passive check: `target/windows-winarrow/20261001-113310-15332-narrow-stop`.
  Helper 24456 (`01dd5144f69b7c00`), HWND 1901700, tag `18da43e50a57c648`;
  owner 17864 (`01dd514500703678`). No helper focus within 15 seconds, so no
  hook installed. Creation receipt was (200,200)-(840,680), whereas the spike's
  pre-input capture and reset readback were (506,355)-(1146,835). The user later
  confirmed moving the helper during this run, resolving the offset. This check
  does not establish active-hook recovery for the final artifact. Reports remain
  preserved.
- Both later actors expired. The verified exited owner's ledger contained zero
  windows; independent stop/restore reported owner exited, zero windows restored
  and ledger cleaned. Final readbacks: `ready:false`, no matching actors,
  ledger.json/stop.request absent. No window write was needed for that cleanup.
- No product input mechanism is selected. Physical Snap/Start, key-state,
  repeat/paired-release and disable/exit reversal still need user observations.

### Broker path repair and hands-off rerun, 2026-10-01

- `Start-ExplorerGui` now rejects executable and working-directory paths that
  are not fully qualified, then rejects a missing executable, before activating
  COM. It does not normalize against the caller's working directory. The original
  relative-path dialog's caller remains unidentified; the user closed the dialog.
- Both launcher scripts parse; dev, stop, proof, winarrow and winarrow-stop recipe
  dry-runs pass. Mocked no-COM checks reject relative, drive-relative and
  root-relative paths for both arguments and a missing absolute executable;
  valid absolute paths reach only the mocked COM sentinel. Independent review
  and whitespace checks pass. No Rust source changed in this repair.
- One normal `just --justfile windows.justfile winarrow` (historical command) hands-off rerun completed
  at `target/windows-winarrow/20261001-141827-29400`, source `c25c7d6` plus the
  recorded spike working tree. `machine.json` binds absolute payload paths,
  SHA-256, session 1 and medium integrity (account IDs dropped).
- SHA-256:
  - Spike: `0D4B444A8256CD0DD876275A7D2F3203B842F1586EC5AADB672C6F5A1C604C76`
  - Helper: `B5B5CD0F245004B7A91258F97A9322477AD9F4A54A50E03507703047B2E677E6`
  - Launcher: `39A224CC3F5529B69D5EED541A416286314F9C44919717B6BC3002CC1577B635`
  - Broker script: `7F1B3F2C04E1F09D98DC71B30F6B9BB91B2A59658C790BB852D6CC88F62A9A11`
- Helper 31292 (`01dd515be8a64613`), HWND 3344478, tag `18da4cdb9658e90c`;
  hotkey owner 29920 (`01dd515be8debfe7`), hook owner 20320
  (`01dd515bf238bbfe`). Receipt, both method captures/reset readbacks and final
  helper rectangle all matched (200,200)-(840,680), with identical helper identity
  and tag. No receipt-versus-capture geometry assertion was added.
- Both methods reported no helper focus within 15 seconds and completed without
  registering or installing anything. Hook down/up/repeat and all delivery,
  consumption/action counts were zero; hotkey raw down/up/repeat remain unavailable.
  Release and reset reports succeeded. Normal machine, helper, owner/readiness,
  method, combined and cleanup artifacts are present; cleanup failures are empty.
- Current display readbacks: primary DISPLAY1, native 2560x1440 at 169 Hz, logical
  bounds 2048x1152 and work area 2048x1104 at origin 0,0. These match the recorded
  baseline; scale was not independently measured.
- The invocation used the launcher's own polling; no independent fast watcher
  ran. Saved final method reports establish that neither method entered an input
  phase. End readbacks: `ready:false`, no project actors, ledger.json/stop.request
  absent. No independent recovery was needed. Final focused active-hook recovery
  and physical input acceptance remain outstanding.

## Hook-only physical spike redesign, 2026-10-01

- Comparison finding for this test system: all four Win+Arrow `RegisterHotKey` calls failed
  with 1409 in runs `20261001-102943-27412`, `20261001-142510-31296`,
  `20261001-142831-34004` and `20261001-144119-29552`. Registration owner is
  unknown. Registration-only code, tests and launcher phases are removed.
- The three later runs included user physical input. Run A recorded one consumed
  Win+Up down/up pair and a 48-pixel upward nudge, then reset. Runs A/B released
  early on focus loss; Run B lost its helper before the 240-second lifetime,
  with correct absent-HWND refusal but misleading `completed` outcome. Cause of
  helper exit remains unknown. Run 3's hook note contains only `hook installed.`,
  not early focus-loss release; all counts were zero and reset succeeded. Native
  half-snaps, maximization and Snap Assist observations lack phase correlation.
- Current flow installs the hook without waiting for focus, keeps it installed
  for 60 seconds across focus changes, then removes it for one 20-second release
  check: tap Win+Left once. Fixed helper/ledger lifetime is 150 seconds; launcher
  budget is 170 seconds. Exact-owner independent stop/restore remains available.
- Only approved physical Win+Arrow chords on the verified foreground helper can
  be consumed or acted on. Background chords pass through and are logged; a
  background-origin hold is never stolen after foreground changes. Extra
  modifiers, injected input and unrelated keys remain unlogged and untouched.
- `events.jsonl` records chord down/up/repeat, callback foreground/disposition,
  action result, phase and boolean focus changes with monotonic milliseconds and
  anchored epoch wall time. Callback storage is preallocated and bounded to 512
  events; serialization/actions run outside the callback. Saturation preserves
  bookkeeping, passes input through and records explicit evidence loss.
- Verified-helper title updates outside the callback show the HOOK ON countdown,
  HOOK OFF release instruction and DONE. Failed installation shows unavailable,
  not HOOK ON. At this point no menu masking or dummy input was implemented. Restore refusal
  and release failure have distinct outcomes instead of `completed`.
- Four-package native build/test, fmt-all, strict all-target clippy, script parse,
  winarrow recipe dry-runs and whitespace checks pass; 24 offline spike tests.
  Independent review corrections covered background logging/origin pairing,
  queue saturation bookkeeping, durable failed-action evidence and honest titles.
  Single-method launcher flow shrank; runtime/tests grew for event evidence.
- Full hands-off run: `target/windows-winarrow/20261001-151044-14184`,
  `completed`, installed/released successfully, zero chords/drops. Event timeline:
  hook-on 0 ms, hook-off 60029 ms, done 80119 ms, foreground false throughout.
  Helper 17844 (`01dd516336b8585e`), HWND 524564, tag `18da4fb615624004`;
  owner 33788 (`01dd516336ed73c9`). Receipt, captured and reset rectangles matched
  (200,200)-(840,680); exact helper close and clean end readbacks passed.
- Active independent recovery: `target/windows-winarrow/20261001-151256-19100`.
  Owner 17956 (`01dd516385d74cc4`) was ready with hook installed before the exact
  directory's independent stop command. Report: `stop-request`, release/reset
  successful, zero chords/drops. Helper 21712 (`01dd516385a13c97`), HWND 1508576,
  tag `18da4fd4e85d0a34`; receipt/capture/reset were (200,200)-(840,680).
  Independent stop exited 0, owner exited, ledger restore cleaned zero windows,
  and exact helper close passed; cleanup failures were empty.
- Active-stop verification limitation: the launcher job handle was lost across
  separate PowerShell tool invocations. Its abort-receipt handshake and final
  console result were not captured; recovery of the hook owner/helper is proven,
  but simultaneous two-controller coordination is not. A single persistent
  driver capturing both controller results would close that evidence gap.
- Both runs bind source `c25c7d6` plus the recorded working tree, session 1,
  medium integrity and absolute payloads in `machine.json`. SHA-256:
  - Spike: `3403BB415D8C600A7BD5CFB6E6C0EB7638831EE2EBDD6561B9F6D88ACB623022`
  - Helper: `B23208593E0DE06FB637F463A8A73083632F72CD155A1C53D90895364B9D7D8E`
  - Launcher: `6D83411FEB126E1225E0A4FD05571CAAFE27E1C909024346F34171B08ED2429E`
- End readbacks for both runs: `ready:false`, no project actors, ledger.json and
  stop.request absent. Current logical DISPLAY1 bounds/work area remain
  2048x1152 / 2048x1104; no fresh scale measurement or visual title/input
  acceptance is claimed.

## Menu mask and physical suppression finding, 2026-10-01

- Physical run `target/windows-winarrow/20261001-152130-7208`: the user observed
  correct nudges for every tested Win+Arrow while HOOK ON, with no native Snap.
  Report totals: delivered 17, consumed 34, acted 17, passed 0, dropped 0.
  Every Win release opened Start. HOOK OFF Win+Left produced native Snap and
  Snap Assist; cleanup was clean. This establishes arrow suppression/release on
  this test system, but identifies the remaining lone-Win-tap menu defect.
- Menu-mask choice: send an unassigned `vkE8` down/up pair with `SendInput` before
  forwarding physical Win-up, only after a consumed approved foreground chord
  in that Win hold and while the hook remains installed. Win-up itself passes.
  Plain Win taps never arm masking. Win-repeat and both Win keys are accounted
  for; passed input that already disguises Win makes masking unnecessary.
- AutoHotkey reference was studied, not copied. Its default is left Ctrl
  (`source/globaldata.cpp:66-67`); Ctrl replaced Shift to avoid the Alt+Shift
  language shortcut (`source/keyboard_mouse.cpp:3150-3157`). Its keyboard hook
  sends the mask before forwarding Win-up, while Win is still logically down
  (`source/hook.cpp:2023-2055`). The
  [A_MenuMaskKey documentation](https://www.autohotkey.com/docs/v2/lib/A_MenuMaskKey.htm)
  recommends `vkE8`/`vkFF` for fewer side effects than Ctrl and explains Win
  auto-repeat, unsuppressed-input and modifier conditions. This spike does not
  reproduce AHK's Alt, mouse-hotkey, Send-path or full menu-disguise engine.
- Injected events return before borrowing hook state; mask injection happens
  with no state borrow held, preventing reentrant recursion into the classifier.
  Callback evidence space is reserved before injection. No space means no send
  plus an explicit loss/skip. A short `SendInput` is recorded as failure; a
  one-event partial insert gets one bounded release attempt for the mask key only.
- Timestamped mask records identify the triggering approved chord and inserted
  event count/release result, without logging Win or other raw keys. Reports add
  mask attempted/success/failed/skipped counters. Failures remain visible without
  claiming suppression success; the spike continues and Start may still appear.
- Side effects/caveats: another hook or remapper can observe the synthetic E8
  pair; an unassigned VK may gain a future Windows function. UIPI/other input
  blocking can reject injection; API acceptance alone does not prove visual
  suppression. A mask can follow a consumed chord's focus change, or be redundant
  after third-party injected input. Holding Win across hook removal cannot be
  masked later. Existing release-both-keys-between-taps procedure remains intact.
- Changes are limited to the spike runtime and its offline tests. Four-package
  native locked/offline build/test, fmt-all, strict all-target clippy, script
  parse, winarrow recipe dry-runs and whitespace checks pass; 37 spike tests.
  Independent review accepted injection ordering, state ownership and evidence.
- Full hands-off run: `target/windows-winarrow/20261001-155342-20220`,
  installed/released, `completed`, no error. Phase times: on 0 ms, off 60066 ms,
  done 80105 ms. All chord, mask and drop counters were zero. Helper 18524
  (`01dd5169377e7ea8`), HWND 919386, tag `18da520e62df4048`; owner 15088
  (`01dd516937b7f74e`). Receipt/capture/reset matched (200,200)-(840,680);
  exact helper close and cleanup with no failures passed.
- Active-stop run: `target/windows-winarrow/20261001-155538-16472`. Owner 7432
  (`01dd51697ce0771b`) was verified active before independent stop. Report:
  `stop-request`, release/reset successful, zero chords/masks/drops. Helper 16888
  (`01dd51697c865a89`), HWND 1836044, tag `18da522959ec37a0`; receipt/capture/reset
  matched (200,200)-(840,680). One persistent driver captured `launcher.log` and
  `stop.log`: independent stop exited 0, owner exited, ledger restore cleaned
  zero windows and exact helper close passed. Launcher read the abort/cleanup
  receipt and reported `aborted: external stop requested` (expected recipe exit 1).
  This closes the earlier two-controller handshake evidence gap.
- Both runs bind source `c25c7d6` plus the recorded working tree, session 1,
  medium integrity and absolute payloads. SHA-256:
  - Spike: `DE002B3B002396EFD5F039F58DD6B3600CA9EC9A6711FBABE66C373F984BD8DF`
  - Helper: `607CC9E02629100FB021E2CA0A92F4F4F78A44BED36C8029601A43035B0E04E2`
  - Launcher: `6D83411FEB126E1225E0A4FD05571CAAFE27E1C909024346F34171B08ED2429E`
- End readbacks: `ready:false`, no project actors, ledger.json/stop.request
  absent and cleanup failures empty. Hands-off runs correctly sent no masks;
  physical acceptance follows below.

### Physical menu-mask acceptance, 2026-10-01

- Run: `target/windows-winarrow/20261001-161051-32788`, same spike/helper/launcher
  SHA-256 as the preceding hands-off verification. The user reported all checks
  passed: every HOOK ON Win+Arrow nudged without Start opening on Win release;
  rapid repeated arrow presses with Win held were responsive; a lone Win tap
  opened Start correctly; HOOK OFF Win+Left produced normal native Snap.
- Saved report agrees: delivered 37, consumed 74, acted 37, passed/skipped/dropped
  0; 17 mask attempts, 17 successful pairs, failed/skipped masks 0. All 17 mask
  records report two inserted events and no partial-release attempt. The 98 log
  records contain no failed action, failed mask or evidence-loss record. Per-chord
  repeat counters are zero: this run records rapid press/release pairs, not
  OS-generated held-arrow auto-repeat.
- Phase timestamps: hook-on 0 ms, hook-off 60037 ms, done 80095 ms. Release and
  reset succeeded; `completed`, no exit error, cleanup failures empty. Captured
  and reset rectangle matched (200,200)-(840,680). Owner 34460
  (`01dd516b9cae6b03`), helper 31740 (`01dd516b9c771317`), HWND 2557466,
  tag `18da52fdd3fd38bc` match the saved owner/helper identity records.
- Read-only end reconciliation found no project actors and no ledger.json or
  stop.request. Current readiness is inferred false from that state; the saved
  ready report describes the earlier active owner. No new executable query was
  used for this reconciliation. Visual Start/no-Start and responsiveness findings
  come from the user; the log corroborates consumption, masking and actions.
- WH_KEYBOARD_LL plus vkE8 masking has physical owned-helper evidence on this test system.
  Selected for product Win+Arrow input on 2026-10-01: consume only approved
  chords, mask at Win key-up while Win is still held, retain non-Win defaults
  and opt-in. RegisterHotKey is rejected after all four chords returned 1409
  on this test system (owner unknown). The harness remains experimental, with fixed
  timers, disposable-helper gating and 48-pixel nudge actions.

## Portability follow-up, 2026-10-01

- Only `x86_64-pc-windows-msvc` is installed; no real Linux compile/test gate
  ran locally. Linux workspace tests/clippy remain the CI gate after push.
- Source inspection found Windows-only dependencies and native modules gated
  by `cfg(windows)`, a non-Windows spike binary stub, and portable classifier
  tests. No spike portability defect was found; this is not a Linux pass.
- Native `cargo +stable test --locked -p tiler-windows`, fmt-all and whitespace
  checks pass, including 37 offline spike tests.

## Remaining Phase 1 evidence

- Earlier hidden-window recovery visual confirmation; the lifecycle proof already
  has machine identity, visibility and rectangle readbacks.
- OS-generated held-arrow auto-repeat and broader ordinary-window/background
  journeys; the final physical acceptance covers rapid repeated presses, not
   held-arrow repeat events. Product integration remains Phase 2 work.
- Linux workspace build/test/clippy and current-tree CI evidence remain pending.
- Later game-disable, session/display transitions and second multi-output test-system geometry/DPI;
  clean-runtime evidence in Phase 4.
