param(
  [switch]$Stop,
  [string]$RunDir = ""
)
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "windows-dev.ps1")

$Repo = Split-Path -Parent $PSScriptRoot
$Target = Join-Path $Repo "target"
$SpikeName = "tiler-winarrow-spike.exe"
$HelperName = "tiler-test-window.exe"
$RunRoot = Join-Path $Target "windows-winarrow"
$GlobalBudgetSec = 170
$HelperSeconds = 150

function Winarrow-Json([string]$Path, [string]$Tag) {
  if (-not (Test-Path -LiteralPath $Path)) { Fail "$Tag missing: $Path" }
  try { return (Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json) }
  catch { Fail "$Tag parse error $Path : $($_.Exception.Message)"; return $null }
}

function Winarrow-Deadline([datetime]$D, [string]$Tag) {
  if ((Get-Date) -gt $D) { Fail "$Tag global deadline exceeded" }
}

function Winarrow-WaitFile([string]$Path, [int]$TimeoutSec, [string]$Tag, [datetime]$D) {
  $end = (Get-Date).AddSeconds($TimeoutSec)
  while (-not (Test-Path -LiteralPath $Path)) {
    Winarrow-Deadline $D "$Tag"
    if ((Get-Date) -gt $end) { Fail "$Tag timeout: $Path" }
    Start-Sleep -Milliseconds 200
  }
}

function Winarrow-WaitExit([int]$ProcessId, [string]$Creation, [int]$TimeoutSec, [string]$Tag, [datetime]$D) {
  $end = (Get-Date).AddSeconds($TimeoutSec)
  while (Test-ProcessAliveSameCreation $ProcessId $Creation) {
    Winarrow-Deadline $D "$Tag"
    if ((Get-Date) -gt $end) { Fail "$Tag exit timeout pid=$ProcessId" }
    Start-Sleep -Milliseconds 500
  }
}

function Winarrow-OwnerSame([object]$A, [object]$B) {
  return ([int]$A.pid -eq [int]$B.pid -and
    "$($A.process_creation)" -ieq "$($B.process_creation)" -and
    "$($A.user_sid)" -ieq "$($B.user_sid)" -and
    [int]$A.session_id -eq [int]$B.session_id -and
    (Test-ExeEqual "$($A.exe_path)" "$($B.exe_path)"))
}

function Winarrow-HelperSame([object]$A, [object]$B) {
  return ("$($A.tag)" -eq "$($B.tag)" -and
    [int]$A.hwnd -eq [int]$B.hwnd -and
    [int]$A.process.pid -eq [int]$B.process.pid -and
    "$($A.process.process_creation)" -ieq "$($B.process.process_creation)" -and
    "$($A.process.user_sid)" -ieq "$($B.process.user_sid)" -and
    [int]$A.process.session_id -eq [int]$B.process.session_id -and
    (Test-ExeEqual "$($A.process.exe_path)" "$($B.process.exe_path)"))
}

function Winarrow-State([string]$Report) {
  if (-not (Test-Path -LiteralPath $Report)) { return "" }
  try { return "$((Get-Content -LiteralPath $Report -Raw | ConvertFrom-Json).method.state)" }
  catch { Fail "report parse error $Report : $($_.Exception.Message)"; return "" }
}

function Winarrow-WaitState([string]$Report, [string[]]$Want, [int]$TimeoutSec, [string]$Tag, [datetime]$D, [int]$OwnerPid, [string]$OwnerCreation) {
  $end = (Get-Date).AddSeconds($TimeoutSec)
  while ($true) {
    Winarrow-Deadline $D "$Tag"
    $state = Winarrow-State $Report
    if ("$state" -eq "done") {
      return $state
    }
    if (-not (Test-ProcessAliveSameCreation $OwnerPid $OwnerCreation)) { Fail "$Tag owner exited early" }
    if ($Want -contains $state) { return $state }
    if ((Get-Date) -gt $end) { Fail "$Tag timeout waiting in ($($Want -join ',')); last=$state" }
    Start-Sleep -Milliseconds 500
  }
}

# Idempotent exact-owner recovery. Never kills by name; stop runs only for the
# recorded owner or the exact committed ledger owner (exe+SID+session match).
function Winarrow-CleanupRun([string]$RunDirPath, [string]$Tag) {
  $failures = [System.Collections.ArrayList]@()
  $machine = Winarrow-Json (Join-Path $RunDirPath "machine.json") "machine"
  $payload = "$($machine.payload)"
  $hBin = "$($machine.helper)"
  if (-not (Test-Path -LiteralPath $payload)) { Fail "$Tag missing payload $payload" }
  $ledgerDir = "$($machine.ledgerDirectory)"
  $recorded = $null
  $ownerFile = Join-Path $RunDirPath "current-owner.json"
  if (Test-Path -LiteralPath $ownerFile) { $recorded = Winarrow-Json $ownerFile "owner" }
  $committed = $null
  $ledgerFile = Join-Path $ledgerDir "ledger.json"
  if (Test-Path -LiteralPath $ledgerFile) { $committed = (Winarrow-Json $ledgerFile "ledger").owner }
  if ($null -ne $committed) {
    if (-not (Test-ExeEqual "$($committed.exe_path)" "$payload") -or
        "$($committed.user_sid)" -ine "$($machine.sid)" -or
        [int]$committed.session_id -ne [int]$machine.session) {
      $null = $failures.Add("ledger owner mismatch; no writes attempted")
      @{ tag = $Tag; failures = @($failures) } | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $RunDirPath "cleanup.json")
      Fail "$Tag cleanup failures: $($failures -join '; ')"
    }
    if ($null -ne $recorded -and -not (Winarrow-OwnerSame $committed $recorded)) {
      $null = $failures.Add("ownership ambiguity: ledger owner differs from recorded owner; no writes attempted")
      @{ tag = $Tag; failures = @($failures) } | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $RunDirPath "cleanup.json")
      Fail "$Tag cleanup failures: $($failures -join '; ')"
    }
  }
  $stopTarget = $recorded
  if ($null -eq $stopTarget -and $null -ne $committed) { $stopTarget = $committed }

  if ($null -ne $stopTarget) {
    $ready = Invoke-Native $payload @("ready") | ConvertFrom-Json
    $canStop = (($ready.ready -eq $true) -and (Winarrow-OwnerSame $ready.owner $stopTarget)) -or
      (($null -ne $committed) -and (Winarrow-OwnerSame $committed $stopTarget))
    if ($canStop) {
      try {
        $st = Invoke-Native $payload @("stop") | ConvertFrom-Json
        Write-Output "$Tag stop: $($st | ConvertTo-Json -Compress)"
        if (-not $st.owner_exited) { $null = $failures.Add("graceful stop no exit") }
      } catch { $null = $failures.Add("stop failed: $($_.Exception.Message)") }
      if (Test-ProcessAliveSameCreation ([int]$stopTarget.pid) "$($stopTarget.process_creation)") {
        $re = Invoke-Native $payload @("ready") | ConvertFrom-Json
        if (($re.ready -eq $true) -and (Winarrow-OwnerSame $re.owner $stopTarget)) {
          try {
            $es = Invoke-Native $payload @("emergency-stop") | ConvertFrom-Json
            Write-Output "$Tag emergency: $($es | ConvertTo-Json -Compress)"
            if (-not $es.owner_exited) { $null = $failures.Add("emergency stop no exit") }
          } catch { $null = $failures.Add("emergency stop failed: $($_.Exception.Message)") }
        } else { $null = $failures.Add("refuse emergency: owner identity changed") }
      }
      try {
        $r = Invoke-Native $payload @("restore") | ConvertFrom-Json
        Write-Output "$Tag restore: $($r | ConvertTo-Json -Compress)"
        if (-not $r.restored) { $null = $failures.Add("restore not restored") }
      } catch { $null = $failures.Add("restore failed: $($_.Exception.Message)") }
    } else { Write-Output "$Tag owner already gone; no stop needed" }
    if (Test-ProcessAliveSameCreation ([int]$stopTarget.pid) "$($stopTarget.process_creation)") {
      $null = $failures.Add("owner pid still alive")
    }
  } else { Write-Output "$Tag no recorded owner; owner cleanup skipped" }

  $helperFile = Join-Path $RunDirPath "helper.json"
  if (Test-Path -LiteralPath $helperFile) {
    $hs = Winarrow-Json $helperFile "helper"
    if (-not (Test-ProcessAliveSameCreation ([int]$hs.process.pid) "$($hs.process.process_creation)")) {
      Write-Output "$Tag helper PID absent; no HWND actuation, already closed"
    } else {
      try { $inspected = Invoke-Native $hBin @("inspect", "$($hs.hwnd)") | ConvertFrom-Json }
      catch { $null = $failures.Add("helper inspect failed: $($_.Exception.Message)"); $inspected = $null }
      if ($null -ne $inspected) {
        if (-not (Winarrow-HelperSame $inspected $hs)) { $null = $failures.Add("helper identity changed; close refused") }
        else {
          try {
            $closed = Invoke-Native $hBin @("close", "$($hs.hwnd)") | ConvertFrom-Json
            Write-Output "$Tag helper-close: $($closed | ConvertTo-Json -Compress)"
          } catch { $null = $failures.Add("helper close failed: $($_.Exception.Message)") }
        }
      }
    }
    $exitDeadline = (Get-Date).AddSeconds(10)
    while (Test-ProcessAliveSameCreation ([int]$hs.process.pid) "$($hs.process.process_creation)") {
      if ((Get-Date) -gt $exitDeadline) { $null = $failures.Add("helper exit timeout"); break }
      Start-Sleep -Milliseconds 200
    }
  }

  $readyAfter = Invoke-Native $payload @("ready") | ConvertFrom-Json
  if ($readyAfter.ready -eq $true) { $null = $failures.Add("ready still true after cleanup") }
  if (Test-Path -LiteralPath (Join-Path $ledgerDir "ledger.json")) { $null = $failures.Add("ledger not clean") }
  if (Test-Path -LiteralPath (Join-Path $ledgerDir "stop.request")) { $null = $failures.Add("stop.request residue") }
  $stragglers = Get-CimInstance Win32_Process -Filter "Name='tiler-windows.exe' OR Name='tiler-test-window.exe' OR Name='tiler-winarrow-spike.exe'" -ErrorAction Stop
  if ($stragglers) { $null = $failures.Add("remaining project actors present") }

  @{ tag = $Tag; failures = @($failures) } | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $RunDirPath "cleanup.json")
  if ($failures.Count -gt 0) { Fail "$Tag cleanup failures: $($failures -join '; ')" }
}

function Winarrow-WaitHookInstalled([string]$Report, [int]$TimeoutSec, [string]$Tag, [datetime]$D, [int]$OwnerPid, [string]$OwnerCreation) {
  $end = (Get-Date).AddSeconds($TimeoutSec)
  while ($true) {
    Winarrow-Deadline $D "$Tag"
    if (-not (Test-ProcessAliveSameCreation $OwnerPid $OwnerCreation)) { Fail "$Tag owner exited early" }
    try { $probe = Get-Content -LiteralPath $Report -Raw | ConvertFrom-Json }
    catch { Fail "report parse error $Report : $($_.Exception.Message)"; return $null }
    if ($null -ne $probe.method.hook_installed) { return $probe }
    if ((Get-Date) -gt $end) { Fail "$Tag timeout waiting for hook install" }
    Start-Sleep -Milliseconds 500
  }
}

function Winarrow-RunHook([string]$Payload, [object]$HelperBase, [string]$HookReport, [string]$OwnerFile, [datetime]$Deadline, [string]$RunDirPath) {
  $runArgs = "run --helper-hwnd $($HelperBase.hwnd) --report `"$HookReport`""
  Start-ExplorerGui $Payload $runArgs $RunDirPath | Out-Null
  $ready = Assert-OwnerReady $Payload "winarrow-hook"
  $ready.owner | ConvertTo-Json -Depth 6 | Set-Content $OwnerFile
  $ready.owner | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $RunDirPath "current-owner.json")
  $ready | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $RunDirPath "ready-hook.json")
  Winarrow-WaitFile $HookReport 20 "hook setup report" $Deadline
  $ownerPid = [int]$ready.owner.pid
  $ownerCreation = "$($ready.owner.process_creation)"

  $state = Winarrow-WaitState $HookReport @("enabled", "release-check", "done") 30 "hook install" $Deadline $ownerPid $ownerCreation
  if ($state -eq "done") {
    $probe = Winarrow-Json $HookReport "hook report"
    Write-Host "hook done without enabled phase (timing-only fallback); state=done"
    if ("$($probe.exit)" -ne "completed" -and "$($probe.exit)" -ne "stop-request") { Fail "hook unexpected exit $($probe.exit)" }
  } else {
    $setup = Winarrow-WaitHookInstalled $HookReport 10 "hook install flag" $Deadline $ownerPid $ownerCreation
    Write-Host "Hook installed=$($setup.method.hook_installed) error=$($setup.method.hook_error)"
    if ($setup.method.hook_installed -ne $true) {
      Write-Host "hook acquired nothing: leave keys alone during this 60s phase."
    } else {
      Write-Host "HOOK ON 60s: single-tap Win+Up/Left/Right/Down, release both keys between chords. Stop testing immediately on native Snap/Start or any other app effect; use the printed stop command."
    }
    $state2 = Winarrow-WaitState $HookReport @("release-check", "done") 75 "hook enabled" $Deadline $ownerPid $ownerCreation
    if ($state2 -eq "release-check") {
      Write-Host "HOOK OFF: tap Win+Left once to confirm release, then leave keys alone."
    }
  }
  Winarrow-WaitExit $ownerPid $ownerCreation 40 "hook spike" $Deadline

  $report = Winarrow-Json $HookReport "hook report"
  if ($null -ne $report.exit_error -and "$($report.exit_error)" -ne "") { Fail "hook spike error: $($report.exit_error)" }
  if ($report.method.release_ok -ne $true) { Fail "hook release failed: $($report.method | ConvertTo-Json -Compress -Depth 6)" }
  if ($report.restore.ok -ne $true) { Fail "hook spike restore failed: $($report.restore | ConvertTo-Json -Compress -Depth 6)" }
  if ($report.exit -ne "completed" -and $report.exit -ne "stop-request") { Fail "hook unexpected exit $($report.exit)" }
  $tot = @{ delivered = 0; consumed = 0; acted = 0; skipped = 0; passed = 0 }
  foreach ($c in $report.method.chords) {
    $tot.delivered += [int]$c.delivered
    $tot.consumed += [int]$c.consumed
    $tot.acted += [int]$c.acted
    $tot.skipped += [int]$c.skipped
    $tot.passed += [int]$c.passed
  }
  Write-Host "hook state=$($report.method.state) delivered=$($tot.delivered) consumed=$($tot.consumed) acted=$($tot.acted) passed=$($tot.passed) skipped=$($tot.skipped) dropped=$($report.method.queue_dropped) events=$($report.method.events_logged) release_ok=$($report.method.release_ok)"
  $eventsLog = Join-Path (Split-Path -Parent $HookReport) "events.jsonl"
  if (-not (Test-Path -LiteralPath $eventsLog)) { Fail "hook events log missing: $eventsLog" }
  return $report
}

if ($Stop) {
  if ([string]::IsNullOrWhiteSpace($RunDir)) { Fail "winarrow-stop needs exact -RunDir (printed before launch)" }
  if (-not (Test-Path -LiteralPath $RunDir)) { Fail "no such run dir $RunDir" }
  $machinePath = Join-Path $RunDir "machine.json"
  if (-not (Test-Path -LiteralPath $machinePath)) { Fail "no machine binding $machinePath" }
  $machineStop = Winarrow-Json $machinePath "machine"
  if ([string]::IsNullOrWhiteSpace("$($machineStop.payload)")) { Fail "no payload binding" }
  $abortFile = Join-Path $RunDir "abort.request"
  @{ pid = $PID; time = (Get-Date).ToUniversalTime().ToString("o"); reason = "winarrow-stop" } | ConvertTo-Json -Compress | Set-Content $abortFile
  try {
    Winarrow-CleanupRun $RunDir "winarrow-stop"
    Write-Output "stopped clean runDir=$RunDir"
    exit 0
  } catch {
    $receipt = Join-Path $RunDir "cleanup.json"
    if (-not (Test-Path -LiteralPath $receipt)) {
      try { @{ tag = "winarrow-stop"; failures = @("$($_.Exception.Message)") } | ConvertTo-Json -Depth 4 | Set-Content $receipt } catch {}
    }
    throw
  }
}

if (-not (Test-Path $Target)) { Fail "missing target/ parent" }
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$runDir = Join-Path $RunRoot "$stamp-$PID"
New-Item -ItemType Directory -Force -Path $runDir | Out-Null
$commit = (& git rev-parse HEAD | Out-String).Trim()
$dirty = (& git status --short -- crates/tiler-windows scripts/windows-winarrow.ps1 scripts/windows-dev.ps1 justfile just/windows.just | Out-String).Trim()
$actorPid = $PID
$actorExe = (Get-Process -Id $PID).Path
$actorPar = (Get-CimProc $PID).ParentProcessId
& cargo build --locked -p tiler-windows
if ($LASTEXITCODE -ne 0) { Fail "cargo build failed" }
$globalDeadline = (Get-Date).AddSeconds($GlobalBudgetSec)
$payload = Join-Path $runDir $SpikeName
$hBin = Join-Path $runDir $HelperName
Copy-Item (Join-Path $Target "debug\$SpikeName") $payload -Force
Copy-Item (Join-Path $Target "debug\$HelperName") $hBin -Force
$spikeHash = (Get-FileHash $payload -Algorithm SHA256).Hash
$helpHash = (Get-FileHash $hBin -Algorithm SHA256).Hash
$scriptHash = (Get-FileHash (Join-Path $PSScriptRoot "windows-winarrow.ps1") -Algorithm SHA256).Hash
$ident = Invoke-Native $payload @("identity") | ConvertFrom-Json
$ledgerDir = "$($ident.ledger_directory)"
if (Test-Path (Join-Path $ledgerDir "ledger.json")) { Fail "refuse: ledger committed by another owner" }
if (Test-Path (Join-Path $ledgerDir "stop.request")) { Fail "refuse: stop.request residue" }
$devPayload = Join-Path $Target "windows-dev\tiler-windows.exe"
if (Test-Path -LiteralPath $devPayload) {
  $devReady = Invoke-Native $devPayload @("ready") | ConvertFrom-Json
  if ($devReady.ready -eq $true) { Fail "refuse: dev owner active; stop it first" }
}
$stragglers = Get-CimInstance Win32_Process -Filter "Name='tiler-windows.exe' OR Name='tiler-test-window.exe' OR Name='tiler-winarrow-spike.exe'" -ErrorAction Stop
if ($stragglers) { Fail "winarrow preflight refuse: actor present" }
$helperReceipt = Join-Path $runDir "helper-receipt.json"
$hookReport = Join-Path $runDir "hook.json"
$eventsLog = Join-Path $runDir "events.jsonl"
$machine = @{
  commit = $commit; dirty = $dirty; actorPid = $actorPid; actorExe = $actorExe
  actorParent = $actorPar; sid = $ident.process.user_sid; session = $ident.process.session_id
  il = $ident.integrity_level; spikeHash = $spikeHash; helperHash = $helpHash
  scriptHash = $scriptHash; payload = $payload; helper = $hBin; ledgerDirectory = $ledgerDir
  hookReport = $hookReport; eventsLog = $eventsLog
}
$machine | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $runDir "machine.json")
Write-Output "report=$runDir"
Write-Output "stop: pwsh -NoProfile -File scripts/windows-winarrow.ps1 -Stop -RunDir '$runDir'"

$mainErr = $null
try {
  try {
    Winarrow-Deadline $globalDeadline "winarrow start"
    Start-ExplorerGui $hBin "run --seconds $HelperSeconds --receipt `"$helperReceipt`"" $runDir | Out-Null
    Winarrow-WaitFile $helperReceipt 10 "helper receipt" $globalDeadline
    $hsnap = Winarrow-Json $helperReceipt "helper receipt"
    $hsnap | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $runDir "helper.json")
    if (-not (Test-ExeEqual "$($hsnap.process.exe_path)" "$hBin")) { Fail "helper peer mismatch" }
    Assert-ParentIsExplorer ([int]$hsnap.process.pid)
    $insp = Invoke-Native $hBin @("inspect", "$($hsnap.hwnd)") | ConvertFrom-Json
    if (-not (Winarrow-HelperSame $insp $hsnap)) { Fail "helper baseline mismatch" }
    if ($insp.visible -ne $true) { Fail "helper not visible" }
    $baselineRect = @($hsnap.left, $hsnap.top, $hsnap.right, $hsnap.bottom)

    Write-Host "1. Mouse-click the helper window to focus it. No keyboard focus tricks."
    if (Test-Path -LiteralPath (Join-Path $runDir "abort.request")) { Fail "aborted: external stop requested" }
    Winarrow-Deadline $globalDeadline "winarrow hook"
    $hook = Winarrow-RunHook $payload $hsnap $hookReport (Join-Path $runDir "owner-hook.json") $globalDeadline $runDir
    if ($hook -is [System.Array]) { Fail "hook report captured pipeline strings" }
    if ("$($hook.exit)" -eq "stop-request" -or (Test-Path -LiteralPath (Join-Path $runDir "abort.request"))) { Fail "aborted: external stop requested" }
    $null = Invoke-Native $payload @("stop")
    $hr = Invoke-Native $payload @("restore") | ConvertFrom-Json
    Write-Output "hook restore: $($hr | ConvertTo-Json -Compress)"
    $final = Invoke-Native $hBin @("inspect", "$($hsnap.hwnd)") | ConvertFrom-Json
    if (-not (Winarrow-HelperSame $final $hsnap)) { Fail "helper identity changed at end" }
    if ($final.left -ne $baselineRect[0] -or $final.top -ne $baselineRect[1] -or $final.right -ne $baselineRect[2] -or $final.bottom -ne $baselineRect[3]) { Fail "geometry not reset at end" }
    Write-Output "outcome=hook exit=$($hook.exit) runDir=$runDir"
  } catch {
    $mainErr = $_
    throw
  } finally {
    $abortFile = Join-Path $runDir "abort.request"
    if (Test-Path -LiteralPath $abortFile) {
      $receipt = Join-Path $runDir "cleanup.json"
      $waitEnd = (Get-Date).AddSeconds(25)
      while (-not (Test-Path -LiteralPath $receipt)) {
        Winarrow-Deadline $globalDeadline "winarrow abort wait"
        if ((Get-Date) -gt $waitEnd) { break }
        Start-Sleep -Milliseconds 500
      }
      if (-not (Test-Path -LiteralPath $receipt)) {
        $msg = "winarrow aborted but external cleanup receipt missing"
        if ($null -ne $mainErr) { $msg = "$($mainErr.Exception.Message) ; $msg" }
        Fail $msg
      }
      $crec = Winarrow-Json $receipt "cleanup"
      if (@($crec.failures).Count -gt 0) {
        $msg = "winarrow-stop failures: $($crec.failures -join '; ')"
        if ($null -ne $mainErr) { $msg = "$($mainErr.Exception.Message) ; $msg" }
        Fail $msg
      }
      $readyCheck = Invoke-Native $payload @("ready") | ConvertFrom-Json
      if ($readyCheck.ready -eq $true) { Fail "abort cleanup incomplete: ready still true" }
      if ($null -ne $mainErr) { throw $mainErr }
      Fail "aborted: external stop requested"
    } else {
      try { Winarrow-CleanupRun $runDir "winarrow" }
      catch {
        $cerr = $_
        Write-Host "cleanup failed: $($cerr.Exception.Message)"
        if ($null -eq $mainErr) { throw $cerr }
        else { throw "$($mainErr.Exception.Message) ; cleanup: $($cerr.Exception.Message)" }
      }
    }
  }
  if ($null -ne $mainErr) { throw $mainErr }
} catch { throw }
Write-Output "runDir=$runDir"
