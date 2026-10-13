param(
)
$ErrorActionPreference = "Stop"
$Repo = Split-Path -Parent $PSScriptRoot
# Reuse Explorer desktop broker plus exact-owner stop/restore helpers. This
# driver never tiles, hooks, or touches non-owned windows; it proves the
# product nonce mechanism over owned helpers only (no workspace policy yet).
. (Join-Path $Repo "scripts\windows-dev.ps1")
if ("$($MyInvocation.InvocationName)" -eq ".") { return }
if (-not (Test-Path -LiteralPath (Join-Path $Repo "Cargo.toml") -PathType Leaf)) {
  throw "driver must run from the repo checkout"
}

$steps = [System.Collections.ArrayList]@()
function Rec([string]$Name, $Data) { $null = $steps.Add(@{ name = $Name; data = $Data }) }
function Sha256-String([string]$Text) {
  $bytes = [Text.Encoding]::UTF8.GetBytes($Text)
  $hash = [Security.Cryptography.SHA256]::Create().ComputeHash($bytes)
  ($hash | ForEach-Object { $_.ToString("x2") }) -join ""
}

function Read-SpiArranging {
  if (-not ("HideProofSpiProbe" -as [type])) {
    $csharp = @"
using System;
using System.Runtime.InteropServices;
public static class HideProofSpiProbe {
  private const uint SPI_GETWINARRANGING = 0x0082;
  [DllImport("user32.dll", SetLastError = true)]
  public static extern bool SystemParametersInfo(uint a, uint b, ref int c, uint d);
  public static int Get() {
    int v = 0;
    if (!SystemParametersInfo(SPI_GETWINARRANGING, 0, ref v, 0)) throw new Exception("SPI_GETWINARRANGING failed");
    return v;
  }
}
"@
    Add-Type -TypeDefinition $csharp | Out-Null
  }
  return ([HideProofSpiProbe]::Get() -ne 0)
}

function Read-PenVisualization {
  if (-not ("HideProofPenProbe" -as [type])) {
    $csharp = @"
using System;
using System.Runtime.InteropServices;
public static class HideProofPenProbe {
  private const uint SPI_GETPENVISUALIZATION = 0x201E;
  [DllImport("user32.dll", SetLastError = true)]
  public static extern bool SystemParametersInfo(uint a, uint b, ref int c, uint d);
  public static int Get() {
    int v = 0;
    if (!SystemParametersInfo(SPI_GETPENVISUALIZATION, 0, ref v, 0)) throw new Exception("SPI_GETPENVISUALIZATION failed");
    return v;
  }
}
"@
    Add-Type -TypeDefinition $csharp | Out-Null
  }
  return [HideProofPenProbe]::Get()
}

function New-HideProofRunDir {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $dir = Join-Path $Repo "target\windows-hide-proof\$stamp-$PID"
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  return $dir
}

$SOURCE_FILES = @(
  "crates/tiler-windows/src/product_hide.rs",
  "crates/tiler-windows/src/tiling_sys.rs",
  "crates/tiler-windows/src/tiling.rs",
  "crates/tiler-windows/src/test_window.rs",
  "crates/tiler-windows/src/lifecycle.rs",
  "crates/tiler-windows/src/main.rs",
  "crates/tiler-windows/src/model.rs",
  "crates/tiler-windows/src/lib.rs",
  "crates/tiler-windows/src/bin/tiler-test-window.rs",
  "crates/tiler-windows/tests/product_hide.rs",
  "crates/tiler-windows/tests/tiling.rs",
  "scripts/windows-hide-proof.ps1",
  "scripts/windows-dev.ps1",
  "justfile",
  "just/windows.just"
)
function Get-SourceHashes {
  $rows = @()
  foreach ($rel in $SOURCE_FILES) {
    $full = Join-Path $Repo $rel
    if (-not (Test-Path -LiteralPath $full -PathType Leaf)) { throw "source missing: $rel" }
    $rows += "$rel=$((Get-FileHash -LiteralPath $full -Algorithm SHA256).Hash)"
  }
  return @{ files = $rows; combined = (Sha256-String ($rows -join "`n")) }
}

$ownerRunning = $false
$ownerPayload = ""
$ownerPid = 0
$ownerCreation = ""
$createdHelpers = [System.Collections.ArrayList]@()
function Recover-HideOwner([string]$Tag) {
  if (-not $ownerRunning -or $ownerPayload -eq "") { return "no-owner" }
  $notes = @()
  try {
    $s = Invoke-Native $ownerPayload @("stop") | ConvertFrom-Json
    $notes += "stop exited=$($s.owner_exited)"
  } catch { $notes += "stop failed: $($_.Exception.Message)" }
  try {
    if (Test-ProcessAliveSameCreation $script:ownerPid $script:ownerCreation) {
      try {
        $e = Invoke-Native $ownerPayload @("emergency-stop") | ConvertFrom-Json
        $notes += "emergency exited=$($e.owner_exited)"
      } catch { $notes += "emergency failed: $($_.Exception.Message)" }
    } else { $notes += "owner gone after stop" }
  } catch { $notes += "alive check failed: $($_.Exception.Message)" }
  try {
    $r = Invoke-Native $ownerPayload @("restore") | ConvertFrom-Json
    $notes += "restore=$($r.restored)"
  } catch { $notes += "restore failed: $($_.Exception.Message)" }
  try { Assert-LedgerClean $ownerPayload; $notes += "ledger clean" }
  catch { $notes += "ledger dirty: $($_.Exception.Message)" }
  $script:ownerRunning = $false
  return "recovered-${Tag}: $($notes -join '; ')"
}

function Start-VisibleHelper([string]$HelperBin, [string]$ProofDir, [string]$Name) {
  $receipt = Join-Path $ProofDir "$Name.json"
  if (Test-Path $receipt) { throw "receipt preexists $receipt" }
  $argStr = "run --receipt `"$receipt`" --seconds 600"
  Start-ExplorerGui $HelperBin $argStr $ProofDir
  $deadline = (Get-Date).AddSeconds(10)
  while (-not (Test-Path $receipt)) {
    if ((Get-Date) -gt $deadline) { throw "helper receipt timeout $Name" }
    Start-Sleep -Milliseconds 200
  }
  $snap = Get-Content -LiteralPath $receipt -Raw | ConvertFrom-Json
  if (-not (Test-ExeEqual "$($snap.process.exe_path)" "$HelperBin")) { throw "$Name helper peer mismatch" }
  Assert-ParentIsExplorer ([int]$snap.process.pid)
  if ([string]$snap.tag -eq "") { throw "$Name helper missing tag" }
  if ($snap.visible -ne $true) { throw "$Name helper not visible at create" }
  $null = $createdHelpers.Add(@{
      hwnd = [uint64]$snap.hwnd; tag = "$($snap.tag)"
      pid = [int]$snap.process.pid; creation = "$($snap.process.process_creation)"
    })
  return $snap
}

function Build-HideAllowlist([string]$Path, [array]$Snaps) {
  $entries = @()
  foreach ($s in $Snaps) {
    if ([string]$s.tag -eq "") { throw "allowlist refused: helper missing lifetime tag" }
    $entries += @{
      hwnd             = [uint64]$s.hwnd
      pid              = [uint32]$s.process.pid
      process_creation = "$($s.process.process_creation)"
      exe_path         = "$($s.process.exe_path)"
      user_sid         = "$($s.process.user_sid)"
      session_id       = [uint32]$s.process.session_id
      tag              = "$($s.tag)"
    }
  }
  @{ windows = $entries } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $Path
}

function Get-HelperInspect([string]$HelperBin, [uint64]$Hwnd) {
  return (Invoke-Native $HelperBin @("inspect", "$Hwnd") | ConvertFrom-Json)
}

function Assert-HelperMatches($Snap, $Base, [string]$Tag, [bool]$WantVisible) {
  if ([uint64]$Snap.hwnd -ne [uint64]$Base.hwnd) { throw "$Tag hwnd changed" }
  if ("$($Snap.tag)" -cne "$($Base.tag)") { throw "$Tag helper tag changed" }
  if ([bool]$Snap.visible -ne $WantVisible) { throw "$Tag visible=$($Snap.visible), want $WantVisible" }
  if ([int]$Snap.process.pid -ne [int]$Base.process.pid) { throw "$Tag pid changed" }
  if ("$($Snap.process.process_creation)" -cne "$($Base.process.process_creation)") { throw "$Tag creation changed" }
  if ("$($Snap.process.user_sid)" -cne "$($Base.process.user_sid)") { throw "$Tag sid changed" }
  if ([int]$Snap.process.session_id -ne [int]$Base.process.session_id) { throw "$Tag session changed" }
  if (-not (Test-ExeEqual "$($Snap.process.exe_path)" "$($Base.process.exe_path)")) { throw "$Tag exe changed" }
  foreach ($k in @("left", "top", "right", "bottom")) {
    if ([int]$Snap.$k -ne [int]$Base.$k) { throw "$Tag geometry $k changed: $($Snap.$k) vs $($Base.$k)" }
  }
}

function Wait-HelperVisible([string]$HelperBin, $Base, [bool]$WantVisible, [int]$TimeoutSec, [string]$Tag) {
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  $last = ""
  while ((Get-Date) -lt $deadline) {
    try { $s = Get-HelperInspect $HelperBin ([uint64]$Base.hwnd) }
    catch { $last = "$($_.Exception.Message)"; Start-Sleep -Milliseconds 250; continue }
    if ([bool]$s.visible -eq $WantVisible -and "$($s.tag)" -ceq "$($Base.tag)") { return $s }
    $last = "visible=$($s.visible)"
    Start-Sleep -Milliseconds 250
  }
  throw "$Tag visibility timeout want=$WantVisible last=$last"
}

function Wait-HideReady([string]$Payload, [int]$TimeoutSec) {
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    $j = Invoke-Native $Payload @("ready") | ConvertFrom-Json
    if ($j.ready -eq $true) { return $j }
    Start-Sleep -Milliseconds 250
  }
  throw "hide ready timeout after ${TimeoutSec}s"
}

function Get-WatcherMarkers([string]$LedgerDir) {
  return @(Get-ChildItem -LiteralPath $LedgerDir -Filter "watcher-*.ready" -ErrorAction SilentlyContinue)
}

function Wait-LedgerClean([string]$Payload, [int]$TimeoutSec, [string]$Tag) {
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  $last = ""
  while ((Get-Date) -lt $deadline) {
    try { Assert-LedgerClean $Payload; return $true }
    catch { $last = "$($_.Exception.Message)"; Start-Sleep -Milliseconds 250; continue }
  }
  throw "$Tag ledger still dirty after ${TimeoutSec}s: $last"
}

function Wait-NoWatcherMarkers([string]$LedgerDir, [int]$TimeoutSec, [string]$Tag) {
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    if ((Get-WatcherMarkers $LedgerDir).Count -eq 0) { return $true }
    Start-Sleep -Milliseconds 250
  }
  throw "$Tag watcher marker residue after ${TimeoutSec}s"
}

function Assert-LedgerV4Receipt([string]$LedgerDir, $ReadyOwner, [array]$Hwnds, [string]$Tag) {
  $ledgerFile = Join-Path $LedgerDir "ledger.json"
  if (-not (Test-Path -LiteralPath $ledgerFile -PathType Leaf)) { throw "$Tag ledger missing" }
  $record = Get-Content -LiteralPath $ledgerFile -Raw | ConvertFrom-Json
  if ([int]$record.v -ne 4) { throw "$Tag ledger v=$($record.v), want 4" }
  if ([int]$record.owner.pid -ne [int]$ReadyOwner.pid) { throw "$Tag ledger owner pid changed" }
  if ("$($record.owner.process_creation)" -cne "$($ReadyOwner.process_creation)") { throw "$Tag ledger owner creation changed" }
  $got = @($record.windows | ForEach-Object { [uint64]$_.hwnd } | Sort-Object)
  $want = @($Hwnds | ForEach-Object { [uint64]$_ } | Sort-Object)
  if (($got -join "|") -ne ($want -join "|")) { throw "$Tag ledger hwnds [$($got -join ',')] want [$($want -join ',')]" }
  foreach ($w in $record.windows) {
    if ("$($w.kind)" -cne "product") { throw "$Tag ledger kind=$($w.kind) hwnd=$($w.hwnd)" }
    if ("$($w.tag)" -notmatch "^[0-9a-f]{16}$" -or "$($w.tag)" -eq "0000000000000000") {
      throw "$Tag ledger bad product tag hwnd=$($w.hwnd)"
    }
  }
  return $record
}

function Invoke-HideProofStage {
  $stageStart = (Get-Date).ToString("o")
  $proofDir = New-HideProofRunDir
  $reportPath = Join-Path $proofDir "hide-report.json"
  if (Test-Path $reportPath) { throw "hide refused: report preexists, will not overwrite" }
  $binDir = Join-Path $proofDir "bin"
  New-Item -ItemType Directory -Force -Path $binDir | Out-Null
  & cargo build --locked --manifest-path (Join-Path $Repo "Cargo.toml") -p tiler-windows
  if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
  $ownerCopy = Join-Path $binDir "tiler-windows.exe"
  $helperCopy = Join-Path $binDir "tiler-test-window.exe"
  Copy-Item (Join-Path $Repo "target\debug\tiler-windows.exe") $ownerCopy -Force
  Copy-Item (Join-Path $Repo "target\debug\tiler-test-window.exe") $helperCopy -Force
  $commit = (& git -C $Repo rev-parse HEAD | Out-String).Trim()
  $status = (& git -C $Repo status --short | Out-String).Trim()
  $prov = @{
    commit = $commit; status = $status; source = (Get-SourceHashes)
    actor_pid = $PID; actor_exe = (Get-Process -Id $PID).Path
    owner_sha256 = (Get-FileHash -LiteralPath $ownerCopy -Algorithm SHA256).Hash
    helper_sha256 = (Get-FileHash -LiteralPath $helperCopy -Algorithm SHA256).Hash
    run_dir = $proofDir
  }
  Rec "env" $prov
  $ident = (Invoke-Native $ownerCopy @("identity") | ConvertFrom-Json)
  $ledgerDir = "$($ident.ledger_directory)"
  Rec "identity" @{
    session_id = "$($ident.process.session_id)"; user_sid = "$($ident.process.user_sid)"
    il = "$($ident.integrity_level)"; ledger = $ledgerDir
    exe = "$($ident.process.exe_path)"; pid = "$($ident.process.pid)"
  }
  $preSpi = Read-SpiArranging
  $prePen = Read-PenVisualization
  Rec "settings-pre" @{ arranging = $preSpi; pen = $prePen }
  if ($preSpi -ne $true) { throw "preflight arranging not raw1" }
  if ([int]$prePen -ne 35) { throw "preflight pen not 35" }
  Assert-LedgerClean $ownerCopy
  $leftovers = @(Get-Process -Name "tiler-windows", "tiler-test-window" -ErrorAction SilentlyContinue | Where-Object {
      $p = ""; try { $p = $_.Path } catch {}
      ($p -ieq $ownerCopy) -or ($p -ieq $helperCopy)
    })
  if ($leftovers.Count -ne 0) { throw "preflight refused: proof actors already running" }
  Rec "preflight" @{ ledger = "clean"; actors = "absent"; arranging = $preSpi; pen = $prePen }

  try {
    # Non-hiding graceful + forced first, per the live guide.
    Start-ExplorerGui $ownerCopy "run --seconds 600" (Split-Path -Parent $ownerCopy)
    $ready = Assert-OwnerReady $ownerCopy "nohide-grace"
    Rec "nohide-grace-ready" $ready
    $st = Stop-OwnerVerified $ownerCopy $false "nohide-grace"
    $alive = Test-ProcessAliveSameCreation ([int]$ready.owner.pid) "$($ready.owner.process_creation)"
    if ($alive) { throw "nohide-grace owner alive" }
    $r = Invoke-Native $ownerCopy @("restore") | ConvertFrom-Json
    Rec "nohide-grace" @{ stop = $st; restore = $r }
    Start-ExplorerGui $ownerCopy "run --seconds 600" (Split-Path -Parent $ownerCopy)
    $ready = Assert-OwnerReady $ownerCopy "nohide-forced"
    Rec "nohide-forced-ready" $ready
    $st = Stop-OwnerVerified $ownerCopy $true "nohide-forced"
    $alive = Test-ProcessAliveSameCreation ([int]$ready.owner.pid) "$($ready.owner.process_creation)"
    if ($alive) { throw "nohide-forced owner alive" }
    $r = Invoke-Native $ownerCopy @("restore") | ConvertFrom-Json
    Rec "nohide-forced" @{ stop = $st; restore = $r }
    Assert-LedgerClean $ownerCopy

    # Two visible owned helpers, then the frozen allowlist.
    $h1 = Start-VisibleHelper $helperCopy $proofDir "helper1"
    $h2 = Start-VisibleHelper $helperCopy $proofDir "helper2"
    Rec "helpers-created" @(@($h1, $h2) | ForEach-Object {
        @{ hwnd = $_.hwnd; pid = $_.process.pid; tag = $_.tag; visible = $_.visible }
      })
    $allowPath = Join-Path $proofDir "allowlist.json"
    Build-HideAllowlist $allowPath @($h1, $h2)
    $b1 = Get-HelperInspect $helperCopy ([uint64]$h1.hwnd)
    $b2 = Get-HelperInspect $helperCopy ([uint64]$h2.hwnd)
    Assert-HelperMatches $b1 $h1 "baseline-1" $true
    Assert-HelperMatches $b2 $h2 "baseline-2" $true

    # Graceful cycle: hide two, prove receipt, auto-reveal on stop.
    $hideArgs = "hide-proof --allowlist `"$allowPath`" --seconds 300 --trace"
    Start-ExplorerGui $ownerCopy $hideArgs $binDir
    $script:ownerRunning = $true
    $script:ownerPayload = $ownerCopy
    $ready = Wait-HideReady $ownerCopy 15
    $script:ownerPid = [int]$ready.owner.pid
    $script:ownerCreation = "$($ready.owner.process_creation)"
    if (-not (Test-ExeEqual "$($ready.owner.exe_path)" "$ownerCopy")) { throw "hide-grace owner peer mismatch" }
    Assert-ParentIsExplorer $script:ownerPid
    Rec "hide-grace-ready" $ready
    $hid1 = Wait-HelperVisible $helperCopy $h1 $false 10 "hide-grace-1"
    $hid2 = Wait-HelperVisible $helperCopy $h2 $false 10 "hide-grace-2"
    Assert-HelperMatches $hid1 $h1 "hide-grace-1" $false
    Assert-HelperMatches $hid2 $h2 "hide-grace-2" $false
    $ledger = Assert-LedgerV4Receipt $ledgerDir $ready.owner @($h1.hwnd, $h2.hwnd) "hide-grace"
    $markers = Get-WatcherMarkers $ledgerDir
    if ($markers.Count -eq 0) { throw "hide-grace watcher marker missing while hidden" }
    Rec "hide-grace-hidden" @{ ledger_v = $ledger.v; windows = $ledger.windows.Count; watcher_markers = $markers.Count }
    $logPath = "$($ready.log_path)"
    $logLines = Get-Content -LiteralPath $logPath
    $logText = $logLines -join "`n"
    if ($logText -notmatch "hide-proof-start" -or $logText -notmatch "hide-proof-hide") {
      throw "hide-grace owner log missing hide events"
    }
    # Production logs stay opaque: no native identity keys (HWNDs, pids,
    # creation strings, SIDs, paths, tags). The allowlist digest is an opaque
    # hash, not an identity, and stays allowed. Raw ids live only in the
    # scoped proof audit beside the ledger.
    foreach ($key in @('"hwnd"', '"pid"', '"process_creation"', '"user_sid"', '"exe_path"', '"tag"')) {
      if ($logText -match [regex]::Escape($key)) { throw "hide-grace production log carries $key" }
    }
    # Exact-owner graceful stop, then auto-reveal readback.
    $frozen = $ready.owner
    $probe = Invoke-Native $ownerCopy @("ready") | ConvertFrom-Json
    if ($probe.ready -ne $true -or [int]$probe.owner.pid -ne [int]$frozen.pid) { throw "hide-grace owner changed before stop" }
    $st = Invoke-Native $ownerCopy @("stop") | ConvertFrom-Json
    if (-not $st.owner_exited) { throw "hide-grace stop no exit" }
    $alive = Test-ProcessAliveSameCreation ([int]$frozen.pid) "$($frozen.process_creation)"
    if ($alive) { throw "hide-grace owner alive" }
    $back1 = Wait-HelperVisible $helperCopy $h1 $true 10 "hide-grace-reveal-1"
    $back2 = Wait-HelperVisible $helperCopy $h2 $true 10 "hide-grace-reveal-2"
    Assert-HelperMatches $back1 $h1 "hide-grace-reveal-1" $true
    Assert-HelperMatches $back2 $h2 "hide-grace-reveal-2" $true
    $r = Invoke-Native $ownerCopy @("restore") | ConvertFrom-Json
    if (-not $r.restored) { throw "hide-grace restore failed" }
    Assert-LedgerClean $ownerCopy
    if ((Get-WatcherMarkers $ledgerDir).Count -ne 0) { throw "hide-grace watcher marker residue" }
    $script:ownerRunning = $false
    Rec "hide-grace" @{ stop = $st; restore = $r; revealed = @($back1.visible, $back2.visible) }

    # Emergency cycle: re-hide the same two (proves nonce release: the second
    # admission would refuse an orphaned claim), force loss, watcher reveals,
    # independent restore stays idempotent. No manual watcher kill: readiness
    # is never removed prematurely.
    Start-ExplorerGui $ownerCopy $hideArgs $binDir
    $script:ownerRunning = $true
    $ready = Wait-HideReady $ownerCopy 15
    $script:ownerPid = [int]$ready.owner.pid
    $script:ownerCreation = "$($ready.owner.process_creation)"
    Rec "hide-forced-ready" $ready
    $hid1 = Wait-HelperVisible $helperCopy $h1 $false 10 "hide-forced-1"
    $hid2 = Wait-HelperVisible $helperCopy $h2 $false 10 "hide-forced-2"
    Assert-HelperMatches $hid1 $h1 "hide-forced-1" $false
    Assert-HelperMatches $hid2 $h2 "hide-forced-2" $false
    $ledger = Assert-LedgerV4Receipt $ledgerDir $ready.owner @($h1.hwnd, $h2.hwnd) "hide-forced"
    Rec "hide-forced-hidden" @{ ledger_v = $ledger.v; windows = $ledger.windows.Count }
    $frozen = $ready.owner
    $probe = Invoke-Native $ownerCopy @("ready") | ConvertFrom-Json
    if ($probe.ready -ne $true -or [int]$probe.owner.pid -ne [int]$frozen.pid) { throw "hide-forced owner changed before stop" }
    $st = Invoke-Native $ownerCopy @("emergency-stop") | ConvertFrom-Json
    if (-not $st.owner_exited) { throw "hide-forced stop no exit" }
    $alive = Test-ProcessAliveSameCreation ([int]$frozen.pid) "$($frozen.process_creation)"
    if ($alive) { throw "hide-forced owner alive" }
    # Automatic watcher reveal (no agent writes): helpers return visible with
    # identity and geometry preserved.
    $back1 = Wait-HelperVisible $helperCopy $h1 $true 15 "hide-forced-watcher-1"
    $back2 = Wait-HelperVisible $helperCopy $h2 $true 15 "hide-forced-watcher-2"
    Assert-HelperMatches $back1 $h1 "hide-forced-watcher-1" $true
    Assert-HelperMatches $back2 $h2 "hide-forced-watcher-2" $true
    # Watcher ledger/marker cleanup races visibility: poll bounded before
    # asserting, then keep the idempotent independent restore below.
    Wait-LedgerClean $ownerCopy 15 "hide-forced-watcher"
    Wait-NoWatcherMarkers $ledgerDir 15 "hide-forced-watcher"
    Rec "hide-forced-watcher" @{ stop = $st; revealed = @($back1.visible, $back2.visible) }
    # Independent restore after the watcher: idempotent, still succeeds.
    $r = Invoke-Native $ownerCopy @("restore") | ConvertFrom-Json
    if (-not $r.restored) { throw "hide-forced independent restore failed" }
    Assert-LedgerClean $ownerCopy
    $script:ownerRunning = $false
    Rec "hide-forced-restore" $r

    # Exact-helper cleanup and end state.
    $c1 = Invoke-Native $helperCopy @("close", "$($h1.hwnd)", "--tag", "$($h1.tag)") | ConvertFrom-Json
    $c2 = Invoke-Native $helperCopy @("close", "$($h2.hwnd)", "--tag", "$($h2.tag)") | ConvertFrom-Json
    $deadline = (Get-Date).AddSeconds(10)
    while ($true) {
      $a = Test-ProcessAliveSameCreation ([int]$h1.process.pid) "$($h1.process.process_creation)"
      $b = Test-ProcessAliveSameCreation ([int]$h2.process.pid) "$($h2.process.process_creation)"
      if (-not $a -and -not $b) { break }
      if ((Get-Date) -gt $deadline) { throw "helper exit timeout" }
      Start-Sleep -Milliseconds 200
    }
    Assert-LedgerClean $ownerCopy
    if ((Get-WatcherMarkers $ledgerDir).Count -ne 0) { throw "end watcher marker residue" }
    $leftovers = @(Get-Process -Name "tiler-windows", "tiler-test-window" -ErrorAction SilentlyContinue | Where-Object {
        $p = ""; try { $p = $_.Path } catch {}
        ($p -ieq $ownerCopy) -or ($p -ieq $helperCopy)
      })
    if ($leftovers.Count -ne 0) { throw "end actors remain" }
    $endSpi = Read-SpiArranging
    $endPen = Read-PenVisualization
    if ($endSpi -ne $true) { throw "end arranging changed" }
    if ([int]$endPen -ne 35) { throw "end pen changed $endPen" }
    if ($endSpi -ne $preSpi -or [int]$endPen -ne [int]$prePen) { throw "end settings changed" }
    Rec "cleanup" @{ closed = @($c1, $c2); ledger = "clean"; actors = "absent"; arranging = $endSpi; pen = $endPen }

    $report = @{ status = "pass"; stage = "HideProof"; started = $stageStart; ended = (Get-Date).ToString("o"); provenance = $prov; steps = $steps }
    $report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $reportPath
    Write-Output "hide-report=$reportPath"
    Write-Output "status=pass"
  } catch {
    $closedHelpers = @()
    foreach ($h in $createdHelpers) {
      if (-not (Test-ProcessAliveSameCreation ([int]$h.pid) "$($h.creation)")) {
        $closedHelpers += @{ hwnd = $h.hwnd; closed = "already-closed"; tag = $h.tag }
        continue
      }
      try {
        $c = Invoke-Native $helperCopy @("close", "$($h.hwnd)", "--tag", "$($h.tag)") | ConvertFrom-Json
        $closedHelpers += @{ hwnd = $h.hwnd; closed = $true; tag = $h.tag }
      } catch {
        $closedHelpers += @{ hwnd = $h.hwnd; closed = $false; error = "$($_.Exception.Message)" }
      }
    }
    $recovery = Recover-HideOwner "hide-proof"
    $report = @{ status = "fail"; stage = "HideProof"; started = $stageStart; ended = (Get-Date).ToString("o"); provenance = $prov; steps = $steps; error = "$($_.Exception.Message)"; closed_helpers = $closedHelpers; recovery = $recovery }
    $report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $reportPath
    Write-Output "hide-report=$reportPath"
    throw "HideProof stage failed: $($_.Exception.Message)"
  }
}

Invoke-HideProofStage
