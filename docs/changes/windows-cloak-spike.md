# Windows private ApplicationView cloak feasibility spike

Status: tooling implemented; native feasibility and recovery evidence pending.

## Goal and boundaries

Evaluate private `IApplicationView::SetCloak(1, 2)` / `(1, 0)` on one
disposable, identity-bound `tiler-test-window` only. Compare retained taskbar
membership with the optional `DeleteTab` / `AddTab` pairing. Establish readbacks,
selection events, graceful restoration, and independent crash recovery.

The intended post-0.1 experience is retained taskbar/Alt+Tab entries whose
selection switches to the window's workspace and focuses it, without pulling
the window. This spike does not implement that reaction, change product
behavior, select a configurable backend, or change the 0.1 `SW_HIDE` decision.
No input interception, injection, elevation, registry/policy writes, or changes
to ordinary application windows are in scope.

## Approach

- Extend the disposable helper CLI, not the product owner. Reuse its exact-HWND,
  lifetime-tag, peer-executable, process identity, user/session and integrity
  fencing. Recovery must remain usable after the controller exits.
- Use the existing `windows` crate COM ownership support and its re-exported
  interface macro; unavoidable native calls stay under the existing approved
  `tiler-windows::ffi` root. No additional unsafe allow-root is required.
- Use ImmersiveShell `IServiceProvider`, `GetViewForHwnd`, then private cloak.
  The pinned Glaze/komorebi interfaces agree on collection slot 6 and view
  slot 12 (including the view's IInspectable-shaped prefix).
- Restrict the experiment to Windows x64 build 26300. This is an experimental
  fence, not a claim of ABI support: the references do not version-gate these
  interfaces. Unknown builds refuse before private calls. IID checks and
  HRESULT/readback failures fail closed; matching IIDs do not prove ABI layout.
- Keep a finite helper lifetime as the ultimate disposable-window backstop.
  A separate exact-identity uncloak command is the primary recovery route.

## Bounded work and acceptance

1. Inspect pinned references and existing helper/FFI/guardrail contracts.
2. Add bounded cloak/controller, readbacks, observation-only activation logging,
   standalone recovery, parser/unit coverage, and optional taskbar pairing.
3. Review live-effect and identity boundaries independently; run build, tests,
   clippy, formatting and unsafe guardrail gates.
4. Supply exact user-run preflight, baseline, shell-selection, graceful-stop,
   crash and recovery steps with expected machine log lines.
5. Keep this record active until native shell evidence resolves feasibility.

## Questions the evidence must answer

- Does private cloak preserve Win32 visibility and geometry while reporting
  a nonzero DWM cloak reason? Does uncloak restore the baseline?
- Are taskbar, Alt+Tab and Win+Tab entries retained without `DeleteTab`?
- Does selection foreground, activate or uncloak the helper? Which observable
  event could trigger a future workspace switch?
- Does the `DeleteTab` variant change all three shell surfaces? Does `AddTab`
  restore taskbar membership?
- What remains after controller loss, and does independent uncloak restore it?

## Scoped design questions after the spike

- Ledger/recovery: record original cloak and taskbar obligations before writes;
  extend identity-safe independent recovery without replacing `SW_HIDE` state.
  Decide how OS drift and unavailable private COM affect outstanding obligations.
- Visibility gates: distinguish Win32 visible, DWM cloaked, managed hidden,
  native-desktop cloaked and user-minimized states throughout observation,
  admission and settlement. Do not reuse visible-only confirmation as proof.
- Selection: identify foreground/show/uncloak evidence, switch to the selected
  window's existing workspace, and focus it; never pull membership. Define
  races with unsolicited activation and the existing urgency policy.
- Settings: scope and persistence of `SW_HIDE` versus private cloak, default
  unchanged, build refusal/fallback policy, taskbar retention, and recovery
  across configuration changes. Any product/API decision needs separate review.

## Files and safety contract

| File | Responsibility |
| --- | --- |
| `crates/tiler-windows/src/bin/tiler-test-window.rs` | Experimental helper commands; product CLI unchanged |
| `crates/tiler-windows/src/cloak.rs` | Build fence, parsers, reports, verified operation sequence, fresh JSONL logs and stop-file protocol |
| `crates/tiler-windows/src/test_window.rs` | Exact owned-helper identity, original process handle retained across operations, per-write rechecks, controller and independent recovery |
| `crates/tiler-windows/src/ffi/cloak.rs` | Minimal private COM definitions, STA ownership, build read and DWM mask read |
| `crates/tiler-windows/src/ffi/hooks.rs` | PID-scoped out-of-context WinEvent observation; existing product subscription unchanged |
| `crates/tiler-windows/src/{lib.rs,ffi/mod.rs}` | Module wiring under the existing unsafe boundary |
| `crates/tiler-windows/{Cargo.toml,tests/cloak_spike.rs}`, `Cargo.lock` | Existing windows-family feature/dependency wiring; 25 offline contract tests |

- No new unsafe allow-root or guardrail-policy exception: `ffi/cloak.rs`
  inherits the existing `ffi/mod.rs` exception. The unchanged allowlist gate
  checks this arrangement. `windows-core` is a direct declaration of the
  already-locked crate needed by the `windows::core::interface` macro.
- Both source pins were inspected locally. Glaze
  `5709ad0a3c7c386bbc3e38166a865ffc12937515`,
  `packages/wm-platform/src/platform_impl/windows/com.rs`, and komorebi
  `e0709f02bfae4e503bf4640f58ee75ecbbfdbb97`,
  `komorebi/src/com/interfaces.rs`, agree on:
  `CLSID_ImmersiveShell=c2f03a33-21f5-47fa-b4bb-156362a2f239`,
  collection IID `1841c6d7-4f9d-42c0-af41-8747538f10e5`, and view IID
  `372e1d3b-38d3-42e4-a15b-8ab2b178f513`. Neither supplies a per-build
  compatibility map. The collection's three preceding methods and the
  view's nine preceding slots match despite abbreviated versus named methods.
  No build-specific alternate GUID was established; unknown builds refuse,
  not a guessed IID/layout fallback.
- The native setter also checks the build before COM calls. A matching IID
  cannot detect an undocumented same-IID layout change. Build 26300 is an
  experimental target only, not a supported product API commitment.
- Each write and restoration revalidates the original executable, process
  creation identity, PID, HWND, class, lifetime tag, user/session and medium
  integrity. Identity loss forbids writes; unreadable state is not fabricated.
- START is flushed before mutation. The taskbar obligation is flushed before
  `DeleteTab`. Every operation has its own state readback. Restoration attempts
  uncloak and then any owed `AddTab` even when one fails; errors aggregate.
  HRESULT errors are not treated as proof that no mutation occurred.
- The controller restores on timeout or its stop file. Forced process loss
  cannot run that cleanup. The independent `uncloak` command is therefore
  mandatory; the helper's finite destruction timer is the final no-strand
  backstop, not a successful restoration claim. This is not a product ledger
  or watchdog implementation.

## Gates and accepted evidence

- Offline contract tests cover build/identity refusal, missing state, operation
  order, failed or uncertain cloak skipping taskbar removal, error cleanup,
  expired targets forbidding restore writes, independent baseline uncloak,
  log/stop-file gates, and per-step/event report shapes.
- Passed after the final code changes: `mise exec -- cargo build --locked -p
  tiler-windows`; `mise exec -- cargo test --locked -p tiler-windows -p
  tiler-core -p tiler-protocol`; `mise exec -- cargo clippy --locked -p
  tiler-core -p tiler-protocol -p tiler-windows --all-targets -- -D warnings`;
  `mise exec -- cargo fmt --all -- --check`; `mise exec -- cargo test --locked
  -p tiler-core --test unsafe_guardrail` (32 passed); `git diff --check`.
  All 25 new cloak contracts pass. No full macOS-on-Windows test result is
  substituted for native cross-platform CI.
- All ten PowerShell command blocks below passed offline AST parsing and
  CLI/log-contract verification without launching any live executable.
- CI passed for `cebd3382`:
  [run 39241213678](https://github.com/beefsack/OmniTiler/actions/runs/39241213678).
  All eight jobs passed: Rust workspace, Windows, macOS, KWin, native effect
  and settings, shell, licenses, and notices. The combined tree's offline
  gates above also passed after integration. Native evidence remains pending.
- Native smoke preflight stopped before launching anything because a concurrent
  normal product owner could manage the helper. No helper/controller/observer
  was launched, and no window, setting or product owner was changed. There is
  no native cloak, restoration, crash, taskbar or shell-selection evidence yet.
- Live smoke, independent recovery, shell-selection and crash observations are
  pending user-run evidence under the procedure below; offline gates do not
  replace them.
- Review found stale delayed-restoration identity checks and an unsuitable
  console stop mechanism. These were corrected before live execution with
  original-identity rechecks, retained process handles, file-based stop,
  flushed logs and injected sequencing tests. No unsafe exception was added.

## User-run steps

### 1. Preconditions and build

- Follow [live Windows testing](../live-windows-testing.md). These steps control
  only one disposable helper and its experiment controllers. Ordinary windows,
  elevated/protected apps, settings, registry and policies are outside scope.
- First stop normal tiling yourself with `mise exec -- just stop`, or the
  existing owner Stop action. Verify its exit and restoration before continuing.
  Do not run this alongside a product owner that can manage the helper. Do not
  stop an unrelated owner or kill by executable name. Leave normal autostart
  inactive during the crash leg; do not modify unrelated startup entries.
- Use ordinary medium-integrity PowerShell 7 from the repository root. Keep
  mouse/Task Manager access and these recovery commands available. No Win+T,
  Win+G or Win+F11. All Alt+Tab/Win+Tab/taskbar observations are physical,
  user-run; the tooling does not synthesize selection.
- Each helper lives at most 600 seconds. Complete a leg within that limit;
  create a fresh helper/receipt and repeat preflight for later legs. Expiry
  invalidates that leg, not evidence that uncloak restored it.

```powershell
mise exec -- cargo build --locked -p tiler-windows
if ($LASTEXITCODE) { throw 'build failed' }
$exe = (Resolve-Path 'target/debug/tiler-test-window.exe').Path
$evidence = Join-Path ([IO.Path]::GetTempPath()) ('omnitiler-cloak-' + [guid]::NewGuid())
New-Item -ItemType Directory $evidence | Out-Null
git rev-parse HEAD | Set-Content (Join-Path $evidence 'revision.txt')
git diff --stat | Set-Content (Join-Path $evidence 'working-tree.txt')
Get-FileHash $exe -Algorithm SHA256 | Format-List | Out-File (Join-Path $evidence 'artifact.txt')
whoami /user | Out-File (Join-Path $evidence 'user.txt')
whoami /groups | Out-File (Join-Path $evidence 'integrity.txt')
Get-CimInstance Win32_OperatingSystem | Select-Object Version,BuildNumber |
    ConvertTo-Json | Set-Content (Join-Path $evidence 'os.json')
Get-CimInstance Win32_Process -Filter "Name='tiler-windows.exe'" |
    Select-Object ProcessId,CreationDate,ExecutablePath,CommandLine |
    ConvertTo-Json | Set-Content (Join-Path $evidence 'owner-preflight.json')
```

- Check `owner-preflight.json`: no active tiling/lifecycle owner or watcher.
  Stop here if uncertain. Save the current display identities, primary display,
  coordinates, scale and work areas in external `display-baseline.txt` using
  your display settings/read-only diagnostics. Do not change display settings.
  Evidence stays outside Git; do not copy local identities/paths into this note.

### 2. Command plumbing, helper and baseline

The helper is a GUI-subsystem executable. Use explicit process waits and
redirected streams; Ctrl-C is not the stop protocol. These functions preserve
argument boundaries, including paths containing spaces.

```powershell
function Start-Probe([string[]] $Words) {
    $si = [Diagnostics.ProcessStartInfo]::new($exe)
    $si.UseShellExecute = $false
    $si.RedirectStandardOutput = $true
    $si.RedirectStandardError = $true
    foreach ($word in $Words) { $si.ArgumentList.Add($word) }
    [Diagnostics.Process]::Start($si)
}
function Finish-Probe($Process, [string] $Name) {
    if (-not $Process.WaitForExit(150000)) { throw 'timeout; use exact recovery, do not broaden cleanup' }
    $out = $Process.StandardOutput.ReadToEnd()
    $err = $Process.StandardError.ReadToEnd()
    $out | Set-Content (Join-Path $evidence ($Name + '.out.json'))
    $err | Set-Content (Join-Path $evidence ($Name + '.err.txt'))
    if ($Process.ExitCode) { throw "$Name failed: $err" }
    if ($out.Trim()) { $out | ConvertFrom-Json }
}
function Read-Lines([string] $Path) {
    if (Test-Path $Path) {
        Get-Content $Path | ForEach-Object { try { $_ | ConvertFrom-Json } catch {} }
    }
}
function Wait-Line([string] $Path, [scriptblock] $Match) {
    $until = [DateTime]::UtcNow.AddSeconds(10)
    do {
        $row = @(Read-Lines $Path | Where-Object $Match)
        if ($row.Count) { return $row[-1] }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $until)
    throw 'expected log line absent; inspect errors and run exact recovery'
}
function Assert-ExactProcess($Process, $Identity) {
    $Process.Refresh()
    if ($Process.HasExited) { throw 'process already exited; do not kill a replacement' }
    if ($Process.Id -ne $Identity.pid -or
        $Process.StartTime.ToUniversalTime().ToFileTimeUtc().ToString('x16') -ne $Identity.process_creation -or
        $Process.MainModule.FileName -ne $Identity.exe_path -or
        $Process.SessionId -ne $Identity.session_id -or
        [Security.Principal.WindowsIdentity]::GetCurrent().User.Value -ne $Identity.user_sid) {
        throw 'process identity mismatch; no kill authorized'
    }
}
$receipt = Join-Path $evidence 'helper.json'
$helper = Start-Probe @('run','--receipt',$receipt,'--seconds','600','--passive')
$until = [DateTime]::UtcNow.AddSeconds(10)
while (-not (Test-Path $receipt)) {
    if ($helper.HasExited -or [DateTime]::UtcNow -gt $until) { throw 'helper startup failed' }
    Start-Sleep -Milliseconds 100
}
$h = Get-Content $receipt -Raw | ConvertFrom-Json
Assert-ExactProcess $helper $h.process
Finish-Probe (Start-Probe @('show',"$($h.hwnd)",'--tag',$h.tag)) 'show'
$base = Finish-Probe (Start-Probe @('cloak-preflight',"$($h.hwnd)",'--tag',$h.tag)) 'baseline'
```

- Baseline must show `preflight:"ok"`, `visible:true`, `iconic:false`,
  `cloak_mask:0`, `cloak_reason:"none"`, a readable rectangle and foreground.
  Unsupported build/architecture or identity refusal means stop, not override.
- Observe and record the helper's normal taskbar button, Alt+Tab and Win+Tab
  entry before cloak. Return physically to the terminal before a cloak command;
  cloak refuses a foreground helper. Do not move or minimize the helper.

### 3. Prove stop/kill plumbing without hiding, then baseline recovery

```powershell
$obsLog = Join-Path $evidence 'stop-proof.jsonl'
$obsStop = Join-Path $evidence 'stop-proof.stop'
$observer = Start-Probe @('cloak-observe',"$($h.hwnd)",'--tag',$h.tag,'--seconds','30',
    '--stop-file',$obsStop,'--log',$obsLog)
$started = Wait-Line $obsLog { $_.line -eq 'start' }
Assert-ExactProcess $observer $started.controller
New-Item -ItemType File $obsStop | Out-Null
Finish-Probe $observer 'stop-proof'

$killLog = Join-Path $evidence 'kill-proof.jsonl'
$killStop = Join-Path $evidence 'kill-proof.stop'
$observer = Start-Probe @('cloak-observe',"$($h.hwnd)",'--tag',$h.tag,'--seconds','30',
    '--stop-file',$killStop,'--log',$killLog)
$started = Wait-Line $killLog { $_.line -eq 'start' }
Assert-ExactProcess $observer $started.controller
$observer.Kill()
if (-not $observer.WaitForExit(5000)) { throw 'exact observer kill not confirmed' }

$proof = Finish-Probe (Start-Probe @('uncloak',"$($h.hwnd)",'--tag',$h.tag)) 'baseline-uncloak'
```

- Stop proof must end with `stop_source:"stop-file"`. Kill only the exact
  read-only observer just proved; never the helper or terminal. Neither proof
  hides anything. If either fails, do not cloak.
- Baseline uncloak must return `baseline_call:true`, `errors:[]`, mask 0,
  original geometry/visibility. It executes real `SetCloak(1,0)`, not a no-op
  shortcut. This proves native lookup/call and standalone command plumbing;
  restoration from a cloaked state still needs the next bounded proof.

### 4. Short independent restore proof and graceful cloak stop

```powershell
$log = Join-Path $evidence 'restore-proof.jsonl'
$stop = Join-Path $evidence 'restore-proof.stop'
$controller = Start-Probe @('cloak',"$($h.hwnd)",'--tag',$h.tag,'--seconds','15',
    '--stop-file',$stop,'--log',$log)
Wait-Line $log { $_.line -eq 'step' -and $_.op -eq 'cloak' -and $_.ok -and $_.state.cloak_mask -gt 0 }
Finish-Probe (Start-Probe @('uncloak',"$($h.hwnd)",'--tag',$h.tag)) 'independent-uncloak'
New-Item -ItemType File $stop | Out-Null
Finish-Probe $controller 'restore-proof'
```

- Compare every state to `$base.state`: cloak should retain visibility and
  geometry but report a nonzero mask/reason. Independent uncloak must report
  mask 0 and original visibility/geometry. Controller end must have empty
  `errors`/`mismatches` and `stop_source:"stop-file"`.
- On any ambiguity, use the exact uncloak command from section 8 and stop.
  Do not proceed to shell selection or intentional loss without this proof.

### 5. Retained taskbar variant: shell listing and selection

For each row below, start a fresh 120-second cloak leg with a NEW `$name`.
Do one row at a time; if selection already uncloaks the helper, that is a result,
not a reason to silently re-cloak in the same row. Return to the terminal and
finish/restore the row before starting the next.

```powershell
$name = 'retained-listing' # next rows: retained-alt-select, retained-taskbar-select
$log = Join-Path $evidence ($name + '.jsonl')
$stop = Join-Path $evidence ($name + '.stop')
$controller = Start-Probe @('cloak',"$($h.hwnd)",'--tag',$h.tag,'--seconds','120',
    '--stop-file',$stop,'--log',$log)
Wait-Line $log { $_.line -eq 'step' -and $_.op -eq 'cloak' -and $_.ok -and $_.state.cloak_mask -gt 0 }
```

| Row | Physical action and report |
| --- | --- |
| `retained-listing` | Observe taskbar button, Alt+Tab entry, and Win+Tab/Task View entry; dismiss switchers without selecting the helper. Record present/absent and thumbnail/Peek appearance separately. |
| `retained-alt-select` | Select the helper using Alt+Tab. Record whether it becomes visible/foreground, remains cloaked, or gets uncloaked by the shell; record approximate elapsed seconds. |
| `retained-taskbar-select` | Select the helper's taskbar button. Record the same results independently; if no button exists, mark selection unavailable instead of inventing a result. |

After the physical action, finish the leg (unless the timeout already restored):

```powershell
if (-not (Test-Path $stop)) { New-Item -ItemType File $stop | Out-Null }
Finish-Probe $controller $name
Read-Lines $log | Where-Object { $_.line -eq 'event' -and $_.kind -ne 'poll' } |
    Format-List
Finish-Probe (Start-Probe @('cloak-preflight',"$($h.hwnd)",'--tag',$h.tag)) ($name + '-restored')
```

- The log records foreground (`event_id:3`) and cloak/uncloak
  (`32791`/`32792`) WinEvents plus 500ms state polls. Hook timestamps are OS
  event times; state is sampled when processed. No event proves which input
  surface caused it: correlate with the single physical row and its time.
  Absence of an event is not proof no activation occurred.
- No workspace switch or focus request is implemented. Shell-induced focus
  or uncloak is observed only. Geometry/visibility mismatch fails rather than
  moving or restyling anything to make the test pass.

### 6. DeleteTab comparison

- Start a fresh helper if its remaining lifetime cannot cover the leg; repeat
  sections 2-4 with fresh filenames. First prove the explicit recovery call
  `uncloak HWND --tag TAG --added-tab` on the normal baseline and verify the
  taskbar button is still present. `AddTab`'s HRESULT is not a visual readback.
- Repeat the listing, Alt+Tab selection and taskbar selection rows from section
  5 using fresh names `deleted-listing`, `deleted-alt-select`, and
  `deleted-taskbar-select`. Change ONLY the controller argument array:

```powershell
$name = 'deleted-listing'
$log = Join-Path $evidence ($name + '.jsonl')
$stop = Join-Path $evidence ($name + '.stop')
$controller = Start-Probe @('cloak',"$($h.hwnd)",'--tag',$h.tag,'--seconds','120',
    '--delete-tab','--stop-file',$stop,'--log',$log)
Wait-Line $log { $_.line -eq 'step' -and $_.op -eq 'delete-tab' -and $_.ok }
```

- Expect START, cloak STEP, add-tab OBLIGATION, delete-tab STEP, then event
  lines. Observe all three surfaces; selection of an absent entry is
  unavailable, not failure to activate. Stop/finish as in section 5. Require
  uncloak and add-tab STEPs, empty errors/mismatches, baseline state, and user
  confirmation that the taskbar button/listings returned.

### 7. Kill-and-recover (only after the independent proof)

- Use a fresh helper/receipt for each variant if necessary. No product owner
  may run. First cloak-only, then optionally `--delete-tab`; record the variant
  before starting. Do not select the helper before the kill, since that could
  already uncloak it and invalidate the crash-residue question.

```powershell
$log = Join-Path $evidence 'crash-retained.jsonl'
$stop = Join-Path $evidence 'crash-retained.stop'
$controller = Start-Probe @('cloak',"$($h.hwnd)",'--tag',$h.tag,'--seconds','120',
    '--stop-file',$stop,'--log',$log)
Wait-Line $log { $_.line -eq 'step' -and $_.op -eq 'cloak' -and $_.ok -and $_.state.cloak_mask -gt 0 }
$started = @(Read-Lines $log | Where-Object line -eq 'start')[0]
Assert-ExactProcess $controller $started.controller
$controller.Kill()
if (-not $controller.WaitForExit(5000)) { throw 'controller loss not confirmed' }

$residueLog = Join-Path $evidence 'crash-residue.jsonl'
$residueStop = Join-Path $evidence 'crash-residue.stop'
$observer = Start-Probe @('cloak-observe',"$($h.hwnd)",'--tag',$h.tag,'--seconds','1',
    '--stop-file',$residueStop,'--log',$residueLog)
Finish-Probe $observer 'crash-residue'
Finish-Probe (Start-Probe @('uncloak',"$($h.hwnd)",'--tag',$h.tag)) 'crash-recovery'
Finish-Probe (Start-Probe @('cloak-preflight',"$($h.hwnd)",'--tag',$h.tag)) 'crash-restored'
```

- Report residue before recovery: helper alive, visible bit, cloak mask/reason,
  geometry, foreground and taskbar/Alt+Tab/Win+Tab presence. Whether the helper
  stays cloaked after controller loss is TBD until these readings exist.
- For the paired crash row use fresh `crash-deleted.*` filenames, add
  `--delete-tab`, wait for the successful delete-tab STEP before killing, and
  recover with `--added-tab`. Verify taskbar/listing restoration physically.
- The killed log should contain its persisted START/STEP prefix and no clean
  END; it cannot contain events after loss. The independent observer provides
  post-loss readbacks. Do not infer recovery from controller exit alone.

### 8. Exact recovery and final cleanup

- Primary recovery, from the same artifact directory, even if the helper was
  selected and is foreground:

```powershell
Finish-Probe (Start-Probe @('uncloak',"$($h.hwnd)",'--tag',$h.tag)) 'recovery'
# Only if DeleteTab was attempted, use this form instead:
# Finish-Probe (Start-Probe @('uncloak',"$($h.hwnd)",'--tag',$h.tag,'--added-tab')) 'recovery-paired'
```

- Require mask 0, visibility and geometry matching baseline, and taskbar
  confirmation for a paired leg. The command revalidates helper identity;
  a stale HWND or tag must refuse. Never remove identity checks to force it.
- If uncloak fails or identity is uncertain, stop experimentation. Normal
  exact-tag close destroys this disposable window without relying on COM:

```powershell
Finish-Probe (Start-Probe @('close',"$($h.hwnd)",'--tag',$h.tag)) 'helper-close'
if (-not $helper.WaitForExit(5000)) { throw 'helper exit not confirmed; preserve evidence and report' }
$absent = Start-Probe @('inspect',"$($h.hwnd)")
if (-not $absent.WaitForExit(5000)) { throw 'absence query timed out' }
$absent.StandardError.ReadToEnd() | Set-Content (Join-Path $evidence 'helper-absent.txt')
if ($absent.ExitCode -eq 0) { throw 'HWND still resolves; do not mutate a possible replacement' }
```

- If normal close cannot complete, the final disposable-resource fallback is
  exact-process destruction, not a private-COM retry. Use the retained helper
  process object only, recheck it against `$h.process` with `Assert-ExactProcess`,
  call `$helper.Kill()`, and require `$helper.WaitForExit(5000)` to return true.
  Then run the same absence query above. This destroys only the disposable
  helper; it does not prove uncloak restored anything. If identity cannot be
  verified, do not kill a replacement - stop and report.
- Verify all retained controller/observer process objects have exited. Stop a
  still-live controller via its own stop file before cleanup; never kill by
  name. The 600-second helper timer independently destroys the owned window if
  neither command can run; this removes the disposable resource, not restoration
  evidence. If even exit/absence is uncertain, report residue and use the live
  guide's user-owned recovery ladder, not broad cleanup or Explorer restarts.

## Expected logs and what to report

| Evidence | Required reading |
| --- | --- |
| START | Original controller/target identities, exact HWND/tag, baseline, build, intended operations, candidate recovery command and experimental-ABI disclaimer |
| STEP `cloak` | `ok:true`, `phase:"after"`, visible bit, numeric nonzero mask and reason, rectangle, foreground; API success alone is insufficient |
| OBLIGATION / STEP `delete-tab` | Obligation precedes write; state readback retained; shell membership still user-observed |
| EVENT | Exact-target `foreground`, `cloaked`, `uncloaked` or `poll`, elapsed/OS event time and verified readback; no reaction |
| STEP `uncloak` / `add-tab` | Fresh identity, mask 0, unchanged geometry/visibility; AddTab membership needs visual confirmation |
| END | Stop source, restoration attempts, `errors:[]`, `mismatches:[]`, log health; absence after kill is expected, not recovery |

- Report source revision + relevant diff, artifact SHA-256, date, OS build and
  native physical-desktop conditions in external evidence. Do not commit local
  paths, machine/user/process identities or other machine-specific information.
- Supply the three-surface presence table for each variant; each selection row's
  input surface, approximate time, foreground event, cloak transition and visible
  result; graceful stop result; pre-kill and post-kill readbacks; independent
  restore results; paired AddTab confirmation; final helper absence. Report
  unavailable actions and failed/uncertain readings explicitly.
- Pending decision: only after those rows pass, decide whether private cloak is
  a feasible post-0.1 candidate and authorize a scoped configurable-backend
  design addressing the four design areas above. No product adoption is selected
  by building this tool.

## Exact next action

Quiesce normal tiling yourself, then run sections 1-4 before any shell-selection
or crash leg. If that independent restore proof fails, stop and report its log;
do not proceed to product integration. All native outcome rows remain TBD.

Reference research: [Alt+Tab and taskbar presence](../research/windows-port/alt-tab-hidden-workspaces.md).
Live contract: [Windows testing](../live-windows-testing.md).
