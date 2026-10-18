$ErrorActionPreference = "Stop"
# Reusable Windows live-test harness curation (offline-safe, dot-source only).
#
# Generic helpers shared by bounded owned-window legs: exact identity,
# read-only paired geometry, client-exposure proof, serial admission gating,
# settings-bytes plus taskbar/SPI preimage, and unconditional cleanup ending
# in `mise exec -- just stop` with byte-exact restoration.
#
# Safety contract (docs/live-windows-testing.md grants no authority itself):
# - No clicks, no SendInput/keybd_event/mouse_event, no registry or policy
#   writes anywhere in this file. The only activation primitive is
#   AttachThreadInput plus exactly one SetForegroundWindow, allowed only when
#   the current foreground is a proven exact owned helper of the same leg;
#   any foreign foreground parks the leg with a throw.
# - Every path (evidence root, settings file, ledger dir, payloads) arrives
#   as a parameter. No host-local paths, host names, or evidence-group names.
# - Depends on scripts/windows-dev.ps1 only for Start-ExplorerGui plus the
#   exact-owner stop/restore helpers; this file never launches or stops an
#   owner itself except through the caller-driven cleanup below.
# - Dot-source guard sits after all definitions so sourcing adds helpers only.

function Invoke-LiveNative([string]$Exe, [string[]]$CliArgs) {
  $out = (& $Exe @CliArgs 2>&1 | Out-String)
  $code = $LASTEXITCODE
  if ($code -ne 0) { throw "native exit ${code}: $Exe $($CliArgs -join ' ') $out" }
  return "$out".Trim()
}

function Get-LiveBytesSha([byte[]]$Bytes) {
  return ([BitConverter]::ToString(
    [Security.Cryptography.SHA256]::Create().ComputeHash($Bytes)
  )).Replace("-", "").ToLower()
}

function Write-LiveStep([string]$StepsFile, [string]$Name, $Data) {
  $line = @{ name = $Name; data = $Data; at = (Get-Date -Format o) } |
    ConvertTo-Json -Compress -Depth 14
  Add-Content -LiteralPath $StepsFile -Value $line -Encoding utf8NoBOM
}

if (-not ("LivePaired" -as [type])) {
  $LivePairedCs = @'
using System;
using System.Runtime.InteropServices;
[StructLayout(LayoutKind.Sequential)] public struct LiveRect { public int l; public int t; public int r; public int b; }
[StructLayout(LayoutKind.Sequential)] public struct LivePoint { public int x; public int y; }
public static class LivePaired {
  [DllImport("user32.dll")] public static extern IntPtr GetThreadDpiAwarenessContext();
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern int AreDpiAwarenessContextsEqual(IntPtr a, IntPtr b);
  [DllImport("user32.dll", SetLastError=true)] public static extern int GetWindowRect(IntPtr h, ref LiveRect r);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int attr, ref LiveRect r, int sz);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint p);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(LivePoint p);
  [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr h, uint f);
  [DllImport("user32.dll", SetLastError=true)] public static extern int SendMessageTimeoutW(IntPtr h, uint m, uint w, int l, uint f, uint t, out UIntPtr r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a, uint b, bool c);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
}
'@
  Add-Type -TypeDefinition $LivePairedCs | Out-Null
}

function Read-LivePaired([uint64]$Hwnd) {
  # Read-only outer (GetWindowRect) plus visible (DWMWA_EXTENDED_FRAME_BOUNDS)
  # snapshot under a PerMonitorV2 thread context, restored in finally.
  $pmv2 = [IntPtr](-4)
  $prior = [LivePaired]::GetThreadDpiAwarenessContext()
  try {
    $null = [LivePaired]::SetThreadDpiAwarenessContext($pmv2)
    $aware = ([LivePaired]::AreDpiAwarenessContextsEqual(
      [LivePaired]::GetThreadDpiAwarenessContext(), $pmv2) -ne 0)
    $o = New-Object LiveRect
    $v = New-Object LiveRect
    $okO = ([LivePaired]::GetWindowRect([IntPtr][long]$Hwnd, [ref]$o) -ne 0)
    $okV = ([LivePaired]::DwmGetWindowAttribute(
      [IntPtr][long]$Hwnd, 9, [ref]$v,
      [Runtime.InteropServices.Marshal]::SizeOf($v)) -eq 0)
    $dpi = 0
    try { $dpi = [LivePaired]::GetDpiForWindow([IntPtr][long]$Hwnd) } catch { $dpi = 0 }
    $outer = @($o.l, $o.t, $o.r, $o.b)
    $vis = @($v.l, $v.t, $v.r, $v.b)
    $insets = @(
      ($vis[0] - $outer[0]), ($vis[1] - $outer[1]),
      ($outer[2] - $vis[2]), ($outer[3] - $vis[3])
    )
    return @{
      pmv2 = $aware; alive = [LivePaired]::IsWindow([IntPtr][long]$Hwnd)
      visible_flag = [LivePaired]::IsWindowVisible([IntPtr][long]$Hwnd)
      ok_outer = $okO; ok_visible = $okV; dpi = [int]$dpi
      outer = $outer; visible = $vis; insets = $insets
    }
  } finally {
    $null = [LivePaired]::SetThreadDpiAwarenessContext($prior)
  }
}

if (-not ("LiveSpiTask" -as [type])) {
  $LiveSpiCs = @'
using System;
using System.Runtime.InteropServices;
public static class LiveSpiTask {
  [DllImport("shell32.dll")] public static extern UIntPtr SHAppBarMessage(uint m, ref LiveAppBar d);
  [DllImport("user32.dll", SetLastError=true)] public static extern bool SystemParametersInfo(uint a, uint b, ref int c, uint d);
  [StructLayout(LayoutKind.Sequential)] public struct LiveAppBar { public int cb; public IntPtr hw; public uint cbm; public uint edge; public LiveAppRect rc; public IntPtr lp; }
  [StructLayout(LayoutKind.Sequential)] public struct LiveAppRect { public int l; public int t; public int r; public int b; }
}
'@
  Add-Type -TypeDefinition $LiveSpiCs | Out-Null
}

function Get-LiveTaskbarSpi {
  # Read-only preimage: ABM_GETSTATE plus Mouse Snap (SPI_GETWINARRANGING,
  # 0x0082) and pen-visualization (SPI_GETPENVISUALIZATION, 0x201E).
  # Legs perform no taskbar or SPI writes; this is readback only. Native
  # BOOL failures throw fail-closed instead of recording a default.
  $arr = 0
  $pen = 0
  $okA = [LiveSpiTask]::SystemParametersInfo(0x0082, 0, [ref]$arr, 0)
  if (-not $okA) {
    throw "SPI_GETWINARRANGING failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
  }
  $okP = [LiveSpiTask]::SystemParametersInfo(0x201E, 0, [ref]$pen, 0)
  if (-not $okP) {
    throw "SPI_GETPENVISUALIZATION failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
  }
  $d = New-Object LiveSpiTask+LiveAppBar
  $d.cb = [Runtime.InteropServices.Marshal]::SizeOf($d)
  $s = [LiveSpiTask]::SHAppBarMessage(4, [ref]$d)
  return @{ arranging = $arr; pen = $pen; abm = "$s" }
}

function Assert-LiveSpiFields($Spi, [string]$Where) {
  # A preimage compare is meaningless when any field is missing: refuse
  # instead of comparing against $null.
  if ($null -eq $Spi -or $null -eq $Spi.arranging -or
      $null -eq $Spi.pen -or $null -eq $Spi.abm) {
    throw "$Where spi fields incomplete"
  }
}

function Get-LiveHelperInspect([string]$HelperExe, [uint64]$Hwnd) {
  return (Invoke-LiveNative $HelperExe @("inspect", "$Hwnd") | ConvertFrom-Json)
}

function Assert-LiveHelperIdentity($Snap, [uint64]$Hwnd, [int]$WantPid,
    [string]$Creation, [string]$Tag, [string]$HelperExe, [string]$Where) {
  if ([uint64]$Snap.hwnd -ne $Hwnd) { throw "$Where hwnd changed" }
  if ([int]$Snap.process.pid -ne $WantPid) { throw "$Where pid changed" }
  if ("$($Snap.process.process_creation)" -ne $Creation) {
    throw "$Where creation changed"
  }
  if ("$($Snap.tag)" -ne $Tag) { throw "$Where tag mismatch" }
  if ("$($Snap.process.exe_path)" -ine "$HelperExe") {
    throw "$Where exe mismatch"
  }
}

function Assert-LiveOsCreation([int]$ProcId, [string]$ExpectedCreation,
    [string]$HelperExe, [string]$Where) {
  # OS-level crosscheck beside the native receipt: process path plus
  # StartTime-derived creation hex. Absent processes propagate the native
  # NoProcessFound error for the caller to classify; any mismatch throws.
  $p = Get-Process -Id $ProcId -ErrorAction Stop
  if ("$($p.Path)" -ine "$HelperExe") {
    throw "$Where OS exe mismatch: pid $ProcId is not the helper"
  }
  $hex = ("{0:x16}" -f $p.StartTime.ToUniversalTime().ToFileTimeUtc())
  if ($hex -ine $ExpectedCreation) { throw "$Where OS creation changed" }
  return $p
}

function Invoke-LiveJustStop {
  # Shared `mise exec -- just stop` probe for leg and final cleanup. Run
  # from the repo root; the caller owns Push-Location. Native exit and text
  # are both captured: clean needs exit 0 plus the stopped-clean report.
  $raw = (mise exec -- just stop 2>&1 | Out-String).Trim()
  $code = $LASTEXITCODE
  return @{
    raw = "$raw"; exit = [int]$code
    clean = (([int]$code -eq 0) -and ("$raw" -match "stopped clean"))
  }
}

function Test-LiveClientExposure([uint64]$Hwnd, [int]$WantPid,
    [string]$StepsFile, [string]$Tag) {
  # Read-only exposure proof for one candidate CLIENT point. Returns the
  # verified point or $null. Checks: WindowFromPoint GA_ROOT equals target,
  # root PID equals want, WM_NCHITTEST equals HTCLIENT (1), never caption.
  # No clicks, no keys, no activation.
  $pr = Read-LivePaired $Hwnd
  if (-not $pr.ok_outer -or -not $pr.ok_visible) { return $null }
  $L = [int]$pr.visible[0]
  $T = [int]$pr.visible[1]
  $R = [int]$pr.visible[2]
  $B = [int]$pr.visible[3]
  $W = $R - $L
  $H = $B - $T
  if ($W -lt 40 -or $H -lt 60) { return $null }
  $cands = @(
    @(([int]($L + $W / 2)), ([int]($T + 40))),
    @(([int]($L + 30)), ([int]($T + 40))),
    @(([int]($R - 30)), ([int]($T + 40))),
    @(([int]($L + $W / 2)), ([int]($B - 20)))
  )
  foreach ($c in $cands) {
    $pt = New-Object LivePoint
    $pt.x = $c[0]
    $pt.y = $c[1]
    $hit = [LivePaired]::WindowFromPoint($pt)
    if ($hit -eq [IntPtr]::Zero) { continue }
    $root = [LivePaired]::GetAncestor($hit, 2)
    $rootH = if ($root -eq [IntPtr]::Zero) { $hit } else { $root }
    if ([uint64]$rootH.ToInt64() -ne $Hwnd) { continue }
    $rp = 0u
    $null = [LivePaired]::GetWindowThreadProcessId($rootH, [ref]$rp)
    if ([int]$rp -ne [int]$WantPid) { continue }
    $lp = (([int]$c[1] -shl 16) -bor ([int]$c[0] -band 0xFFFF))
    [UIntPtr]$hr = [UIntPtr]::Zero
    $sent = [LivePaired]::SendMessageTimeoutW($rootH, 132, 0, $lp, 2, 1000, [ref]$hr)
    if ($sent -eq 0) { continue }
    if ([uint64]$hr.ToUInt64() -ne 1) { continue }
    if ($StepsFile -ne "") {
      Write-LiveStep $StepsFile "exposure-probe" @{
        tag = $Tag; at = "$($c[0]),$($c[1])"; result = "exposed-client"
      }
    }
    return @{ x = $c[0]; y = $c[1] }
  }
  return $null
}

function Confirm-LiveForeground([uint64]$Hwnd, [int]$WantPid, [string]$Creation,
    [string]$Htag, [string]$HelperExe, [array]$Helpers, [string]$Tag) {
  # No injected keys or mouse. Prefer the target already foreground.
  # Otherwise AttachThreadInput plus exactly one SetForegroundWindow is
  # allowed ONLY when the current foreground is proven to be another exact
  # owned helper of THIS leg (HWND/PID/creation/tag/exe re-verified). Any
  # foreign foreground (operator Terminal included) parks the leg with a
  # throw: no attach, no setter, no tiler intent. The setter BOOL is never
  # trusted; exact post readback plus identity re-verify decide.
  $pre = Get-LiveHelperInspect $HelperExe $Hwnd
  Assert-LiveHelperIdentity $pre $Hwnd $WantPid $Creation $Htag $HelperExe "$Tag-prefocus"
  $null = Assert-LiveOsCreation $WantPid $Creation $HelperExe "$Tag-pretarget-os"
  $fg = [LivePaired]::GetForegroundWindow()
  $fp = 0u
  $null = [LivePaired]::GetWindowThreadProcessId($fg, [ref]$fp)
  if ([long]$fg.ToInt64() -eq [long]$Hwnd -and [int]$fp -eq [int]$WantPid) {
    return "already-foreground"
  }
  $fgH = [uint64]$fg.ToInt64()
  $match = @($Helpers | Where-Object { [uint64]$_.hwnd -eq $fgH })
  if ($match.Count -eq 0) {
    throw "fg-park: $Tag current foreground is not a leg helper; no attach, no input"
  }
  $m = $match[0]
  $fsnap = Get-LiveHelperInspect $HelperExe $fgH
  Assert-LiveHelperIdentity $fsnap $fgH ([int]$m.pid) $m.creation $m.tag $HelperExe "$Tag-fghelper"
  $null = Assert-LiveOsCreation ([int]$m.pid) $m.creation $HelperExe "$Tag-fghelper-os"
  $fgTid = [LivePaired]::GetWindowThreadProcessId($fg, [ref]$fp)
  if ([uint32]$fgTid -eq 0) { throw "$Tag foreground TID unreadable" }
  $myTid = [LivePaired]::GetCurrentThreadId()
  $attachOk = $false
  if ([uint32]$fgTid -ne [uint32]$myTid) {
    $attachOk = [LivePaired]::AttachThreadInput($myTid, $fgTid, $true)
  }
  try { $null = [LivePaired]::SetForegroundWindow([IntPtr][long]$Hwnd) }
  finally {
    if ($attachOk) { $null = [LivePaired]::AttachThreadInput($myTid, $fgTid, $false) }
  }
  $dl = (Get-Date).AddSeconds(5)
  $cur = [LivePaired]::GetForegroundWindow()
  while ([long]$cur.ToInt64() -ne [long]$Hwnd -and ((Get-Date) -lt $dl)) {
    Start-Sleep -Milliseconds 100
    $cur = [LivePaired]::GetForegroundWindow()
  }
  if ([long]$cur.ToInt64() -ne [long]$Hwnd) {
    throw "fg-blocked: $Tag readback mismatch"
  }
  $post = Get-LiveHelperInspect $HelperExe $Hwnd
  Assert-LiveHelperIdentity $post $Hwnd $WantPid $Creation $Htag $HelperExe "$Tag-postactivate"
  return "foregrounded attach=$attachOk"
}

function Read-LiveOwnerLog([string]$LogPath) {
  if (-not (Test-Path -LiteralPath $LogPath -PathType Leaf)) { return @() }
  $out = @()
  foreach ($raw in (Get-Content -LiteralPath $LogPath)) {
    if ("$raw".Trim() -eq "") { continue }
    try { $out += ($raw | ConvertFrom-Json) }
    catch { throw "malformed JSON ${LogPath}: $($_.Exception.Message)" }
  }
  return $out
}

function Wait-LiveObservedAdmission([string]$LogPath, [string]$StepsFile,
    [string[]]$Known, [int]$WantUnique, [int]$TimeoutSec, [string]$Tag) {
  # Serial admission gate: the unique observed token set must grow to exactly
  # WantUnique with a delta of exactly one new token. Jumps (coalesced
  # batches, foreign admissions, duplicates) park the leg instead of being
  # silently attributed to the just-created helper.
  $dl = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $dl) {
    $all = @(Read-LiveOwnerLog $LogPath |
      Where-Object { $_.event -eq "visibility-transition" } |
      ForEach-Object { @($_.entries) } |
      Where-Object { $_.state -eq "observed" -and "$($_.window)" -ne "" } |
      ForEach-Object { "$($_.window)" } |
      Sort-Object -Unique)
    if ($all.Count -eq $WantUnique) {
      $new = @($all | Where-Object { $Known -notcontains $_ })
      if ($new.Count -ne 1) {
        throw "park: $Tag admission delta $($new.Count) != 1; refusing attribution"
      }
      if ($StepsFile -ne "") {
        Write-LiveStep $StepsFile "admission-gated" @{
          tag = $Tag; token = $new[0]; unique_count = $WantUnique
        }
      }
      return $new[0]
    }
    if ($all.Count -gt $WantUnique) {
      throw "park: $Tag observed jump to $($all.Count), want $WantUnique; refusing attribution"
    }
    Start-Sleep -Milliseconds 500
  }
  throw "park: $Tag admission gate timeout; want $WantUnique unique observed"
}

function Assert-LiveTokenMap($TokOf, [int]$N, [string]$StepsFile) {
  $vals = @($TokOf.Values)
  if ($vals.Count -ne $N) { throw "park: token map count $($vals.Count) != roster $N" }
  if (@($vals | Where-Object { $null -eq $_ -or "$_" -eq "" }).Count -gt 0) {
    throw "park: null token in map"
  }
  if ((@($vals | Sort-Object -Unique)).Count -ne $N) {
    throw "park: duplicate tokens in map"
  }
  if ($StepsFile -ne "") {
    Write-LiveStep $StepsFile "token-map" @{ count = $N; unique = $N }
  }
}

function Save-LiveSettingsPreimage([string]$SettingsPath, [string]$OrigPath,
    [string]$SpiPath, [string]$StepsFile) {
  # Capture once, verify at every leg start: settings bytes must parse as
  # JSON with a revision (a valid group snapshot), plus the read-only
  # taskbar/SPI preimage. Capture parses the captured bytes themselves
  # (never a re-read), writes the exact bytes, re-reads the copy for a byte
  # comparison, and re-reads live after the SPI capture to refuse drift
  # during capture. Reuse requires the stored settings bytes, a valid
  # existing SPI file with complete fields, live settings byte-identity,
  # and a current-SPI match. Any mismatch refuses the fixture: no run
  # proceeds on uncertain restoration. The library never writes taskbar/SPI
  # state.
  if (-not (Test-Path -LiteralPath $OrigPath -PathType Leaf)) {
    $live = [IO.File]::ReadAllBytes($SettingsPath)
    try { $doc = ([Text.Encoding]::UTF8.GetString($live) | ConvertFrom-Json) }
    catch { throw "refuse: live settings do not parse as JSON" }
    if ($null -eq $doc.revision) { throw "refuse: live settings carry no revision" }
    $sha = Get-LiveBytesSha $live
    [IO.File]::WriteAllBytes($OrigPath, $live)
    $backSha = Get-LiveBytesSha ([IO.File]::ReadAllBytes($OrigPath))
    if ($backSha -ne $sha) { throw "refuse: orig copy bytes differ from live capture" }
    $spi = Get-LiveTaskbarSpi
    Assert-LiveSpiFields $spi "refuse: captured"
    $lateH = Get-LiveBytesSha ([IO.File]::ReadAllBytes($SettingsPath))
    if ($lateH -ne $sha) { throw "refuse: live settings drifted during capture" }
    $spi | ConvertTo-Json -Depth 6 -Compress |
      Set-Content -LiteralPath $SpiPath -Encoding utf8NoBOM
    if ($StepsFile -ne "") {
      Write-LiveStep $StepsFile "group-orig-captured" @{
        settings_sha = $sha; revision = $doc.revision; taskbar_spi = $spi
      }
    }
    return @{ path = $OrigPath; sha = $sha; spi_path = $SpiPath }
  }
  $ob = [IO.File]::ReadAllBytes($OrigPath)
  $obH = Get-LiveBytesSha $ob
  try { $odb = (Get-Content -LiteralPath $OrigPath -Raw) | ConvertFrom-Json }
  catch { throw "refuse: group orig settings do not parse as JSON" }
  if ($null -eq $odb.revision) { throw "refuse: group orig settings carry no revision" }
  if (-not (Test-Path -LiteralPath $SpiPath -PathType Leaf)) {
    throw "refuse: spi preimage missing alongside group orig"
  }
  try { $spiWant = (Get-Content -LiteralPath $SpiPath -Raw) | ConvertFrom-Json }
  catch { throw "refuse: spi preimage does not parse as JSON" }
  Assert-LiveSpiFields $spiWant "refuse: stored"
  $liveH = Get-LiveBytesSha ([IO.File]::ReadAllBytes($SettingsPath))
  if ($liveH -ne $obH) {
    throw "refuse: live settings differ from group orig; no fixture on uncertain restoration"
  }
  $spiNow = Get-LiveTaskbarSpi
  Assert-LiveSpiFields $spiNow "refuse: current"
  if ([int]$spiNow.arranging -ne [int]$spiWant.arranging -or
      [int]$spiNow.pen -ne [int]$spiWant.pen -or
      "$($spiNow.abm)" -ne "$($spiWant.abm)") {
    throw "refuse: current taskbar/SPI differs from preimage; no fixture on uncertain restoration"
  }
  if ($StepsFile -ne "") {
    Write-LiveStep $StepsFile "group-orig-verified" @{
      sha = $obH; revision = $odb.revision; taskbar_spi_match = $true
    }
  }
  return @{ path = $OrigPath; sha = $obH; spi_path = $SpiPath }
}

function Close-LiveHelperExact([string]$HelperExe, [uint64]$Hwnd, [int]$ProcId,
    [string]$Creation, [string]$Htag, [string]$StepsFile, [string]$Tag) {
  # Exact-tag close with inspect re-verify before and liveness poll after.
  # Returns "exited", "still-alive-after-5s", or "absent-unexpected" (an
  # already-absent helper is an anomaly, never success).
  try { $null = Assert-LiveOsCreation $ProcId $Creation $HelperExe "$Tag-oscross" }
  catch {
    if ("$($_.FullyQualifiedErrorId)" -like "NoProcessFound*") {
      if ($StepsFile -ne "") {
        Write-LiveStep $StepsFile "helper-close-out" @{
          tag = $Tag; hwnd = $Hwnd; out = "absent-unexpected"
        }
      }
      return "absent-unexpected"
    }
    throw
  }
  $snap = Get-LiveHelperInspect $HelperExe $Hwnd
  Assert-LiveHelperIdentity $snap $Hwnd $ProcId $Creation $Htag $HelperExe "$Tag-preclose"
  $out = Invoke-LiveNative $HelperExe @("close", "$Hwnd", "--tag", "$Htag")
  if ($StepsFile -ne "") {
    Write-LiveStep $StepsFile "helper-close-out" @{ tag = $Tag; hwnd = $Hwnd; out = "$out" }
  }
  $dl = (Get-Date).AddSeconds(5)
  while ((Get-Date) -lt $dl) {
    try { $null = Get-Process -Id $ProcId -ErrorAction Stop; Start-Sleep -Milliseconds 250 }
    catch { return "exited" }
  }
  return "still-alive-after-5s"
}

function Restore-LiveLegSafety([string]$StepsFile, [array]$Helpers,
    [string]$HelperExe, [string]$SettingsPath, $GroupOrig, [bool]$OwnerLaunched,
    [string]$OwnerPayload, [Parameter(Mandatory = $true)][string]$RepoRoot) {
  # Unconditional leg cleanup: every phase runs inside its own try so an
  # earlier failure can never skip a later one, including the step logger
  # itself (Step-Leg never throws; logging failures accumulate as
  # step-log anomalies). Anomalies accumulate and the run parks once at the
  # end. Phases: settings-bytes restore from the preimage, exact
  # owned-helper closes (best effort each), owner stop AND restore AND ready
  # readback, then the shared `mise exec -- just stop` probe, and only then
  # the post-leg settings-byte plus SPI verify against the original, so a
  # final stop cannot invalidate an already-verified state. RepoRoot is
  # required so the just-stop probe cannot be silently skipped. Uncertain
  # restoration parks loudly so no further fixture runs on top of it.
  $anomalies = [System.Collections.ArrayList]@()
  function Step-Leg([string]$Name, $Data) {
    if ($StepsFile -eq "") { return }
    try { Write-LiveStep $StepsFile $Name $Data }
    catch {
      $null = $anomalies.Add(@{ phase = "step-log"; step = $Name;
        error = "$($_.Exception.Message)" })
    }
  }
  try {
    if (($null -ne $GroupOrig) -and (Test-Path -LiteralPath $GroupOrig.path)) {
      Copy-Item $GroupOrig.path $SettingsPath -Force
      Step-Leg "settings-restored-leg" @{}
    } else {
      throw "no-group-orig"
    }
  } catch {
    $null = $anomalies.Add(@{ phase = "settings-restore"; error = "$($_.Exception.Message)" })
    Step-Leg "settings-restore-anomaly" @{ error = "$($_.Exception.Message)" }
  }
  foreach ($hh in $Helpers) {
    try {
      $r = Close-LiveHelperExact $HelperExe ([uint64]$hh.hwnd) ([int]$hh.pid) `
        $hh.creation $hh.tag $StepsFile ("cleanup-$($hh.index)")
      if ($r -ne "exited") {
        $null = $anomalies.Add(@{ phase = "helper-close"; tag = "cleanup-$($hh.index)";
          hwnd = $hh.hwnd; result = $r })
      }
    } catch {
      $null = $anomalies.Add(@{ phase = "helper-close"; tag = "cleanup-$($hh.index)";
        hwnd = $hh.hwnd; error = "$($_.Exception.Message)" })
      Step-Leg "helper-close-anomaly" @{
        index = $hh.index; hwnd = $hh.hwnd; error = "$($_.Exception.Message)"
      }
    }
  }
  if ($OwnerLaunched) {
    try {
      $stopOut = Invoke-LiveNative $OwnerPayload @("stop")
      Step-Leg "owner-stop" @{ out = "$stopOut" }
    } catch {
      $null = $anomalies.Add(@{ phase = "owner-stop"; error = "$($_.Exception.Message)" })
      Step-Leg "owner-stop-anomaly" @{ error = "$($_.Exception.Message)" }
    }
    try {
      $rsOut = Invoke-LiveNative $OwnerPayload @("restore")
      Step-Leg "owner-restore" @{ out = "$rsOut" }
    } catch {
      $null = $anomalies.Add(@{ phase = "owner-restore"; error = "$($_.Exception.Message)" })
      Step-Leg "owner-restore-anomaly" @{ error = "$($_.Exception.Message)" }
    }
    try {
      $rr = (Invoke-LiveNative $OwnerPayload @("ready")) | ConvertFrom-Json
      Step-Leg "ready-post-stop" @{ ready = $rr.ready }
      if ($rr.ready -eq $true) {
        $null = $anomalies.Add(@{ phase = "owner-ready"; error = "owner still ready after stop" })
      }
    } catch {
      $null = $anomalies.Add(@{ phase = "owner-ready"; error = "$($_.Exception.Message)" })
      Step-Leg "owner-ready-anomaly" @{ error = "$($_.Exception.Message)" }
    }
  } else {
    Step-Leg "owner-stop-skipped" @{ reason = "never-launched" }
  }
  $pushed = $false
  try {
    if (-not (Test-Path -LiteralPath (Join-Path $RepoRoot "mise.toml"))) {
      throw "missing mise.toml under repo root"
    }
    Push-Location -LiteralPath $RepoRoot
    $pushed = $true
    $js = Invoke-LiveJustStop
    Step-Leg "just-stop" @{ exit = $js.exit; clean = $js.clean; raw = $js.raw }
    if (-not $js.clean) { throw "just stop unclean (exit $($js.exit))" }
  } catch {
    $null = $anomalies.Add(@{ phase = "just-stop"; error = "$($_.Exception.Message)" })
  } finally {
    if ($pushed) { Pop-Location }
  }
  try {
    if (($null -eq $GroupOrig) -or (-not (Test-Path -LiteralPath $GroupOrig.path))) {
      throw "no-group-orig"
    }
    $liveH = Get-LiveBytesSha ([IO.File]::ReadAllBytes($SettingsPath))
    $byteEq = ($liveH -eq $GroupOrig.sha)
    Step-Leg "settings-post-leg" @{ match = $byteEq; sha = $liveH; want = $GroupOrig.sha }
    if (-not $byteEq) { throw "settings differ from group orig" }
  } catch {
    $null = $anomalies.Add(@{ phase = "settings-post-verify"; error = "$($_.Exception.Message)" })
  }
  try {
    if (($null -eq $GroupOrig) -or (-not (Test-Path -LiteralPath $GroupOrig.spi_path))) {
      throw "no-spi-preimage"
    }
    $spiWant = (Get-Content -LiteralPath $GroupOrig.spi_path -Raw) | ConvertFrom-Json
    Assert-LiveSpiFields $spiWant "post-leg stored"
    $spiNow = Get-LiveTaskbarSpi
    Assert-LiveSpiFields $spiNow "post-leg current"
    $spiEq = ([int]$spiNow.arranging -eq [int]$spiWant.arranging -and
      [int]$spiNow.pen -eq [int]$spiWant.pen -and
      "$($spiNow.abm)" -eq "$($spiWant.abm)")
    Step-Leg "taskbar-spi-post-leg" @{ match = $spiEq; now = $spiNow; want = $spiWant }
    if (-not $spiEq) { throw "taskbar/SPI differ from preimage" }
  } catch {
    $null = $anomalies.Add(@{ phase = "taskbar-spi-post-verify"; error = "$($_.Exception.Message)" })
  }
  if ($anomalies.Count -gt 0) {
    Step-Leg "close-anomalies" @{ items = @($anomalies) }
    throw "park: leg-cleanup-uncertain: $(($anomalies | ForEach-Object { $_.phase }) -join ', ')"
  }
  return @($anomalies)
}

function Invoke-LiveFinalCleanup([string]$RepoRoot, [string]$OwnerPayload,
    [string]$SettingsPath, $GroupOrig, [string[]]$OwnedNames,
    [string]$LedgerDir, [string]$OutFile) {
  # Group final recovery: real `mise exec -- just stop` evidence (native exit
  # plus text), then independent verification. Every phase is isolated so a
  # failure cannot skip restoration or readback; failures aggregate and the
  # evidence file is always attempted in the finally block. No bare success
  # strings: every claim carries the command output or readback behind it.
  # Invalid inputs never abort the run up front: each is recorded as a
  # failure and only its dependent phase is skipped, while every
  # independent restoration phase is still attempted.
  $out = @{}
  $fails = [System.Collections.ArrayList]@()
  $haveNames = ($null -ne $OwnedNames -and @($OwnedNames).Count -gt 0)
  if (-not $haveNames) {
    $null = $fails.Add("names: no owned names bound; residue query skipped")
  }
  $haveOrig = ($null -ne $GroupOrig -and $null -ne $GroupOrig.path -and
    $null -ne $GroupOrig.sha -and $null -ne $GroupOrig.spi_path)
  if (-not $haveOrig) {
    $null = $fails.Add("group-orig: incomplete group orig; settings phases skipped")
  }
  $pushed = $false
  try {
    $out["at"] = (Get-Date).ToString("o")
    try {
      Push-Location -LiteralPath $RepoRoot
      $pushed = $true
    } catch {
      $null = $fails.Add("location: $($_.Exception.Message)")
    }
    try {
      $out["rev"] = ((& git -C $RepoRoot rev-parse HEAD | Out-String).Trim())
    } catch {
      $null = $fails.Add("rev: $($_.Exception.Message)")
      $out["rev"] = ""
    }
    try {
      if (-not $pushed) { throw "no repo location; just stop skipped" }
      if (-not (Test-Path -LiteralPath (Join-Path $RepoRoot "mise.toml"))) {
        throw "missing mise.toml under repo root"
      }
      $js = Invoke-LiveJustStop
      $out["just_stop_raw"] = $js.raw
      $out["just_stop_exit"] = $js.exit
      $out["just_stop_clean"] = $js.clean
      if (-not $js.clean) { throw "just stop unclean (exit $($js.exit))" }
    } catch {
      $null = $fails.Add("just-stop: $($_.Exception.Message)")
    }
    try {
      if (-not $haveOrig) { throw "incomplete group orig" }
      if (-not (Test-Path -LiteralPath $GroupOrig.path)) { throw "group orig file absent" }
      Copy-Item $GroupOrig.path $SettingsPath -Force
      $out["settings_restored"] = $true
    } catch {
      $out["settings_restored"] = $false
      $null = $fails.Add("settings-restore: $($_.Exception.Message)")
    }
    try {
      $readyRaw = Invoke-LiveNative $OwnerPayload @("ready")
      $out["owner_ready_raw"] = $readyRaw
      $out["owner_ready_exit"] = 0
      $readyFalse = ((($readyRaw | ConvertFrom-Json).ready) -eq $false)
      $out["owner_ready_false"] = $readyFalse
      if (-not $readyFalse) { throw "owner still ready" }
    } catch {
      $out["owner_ready_exit"] = "throw"
      $null = $fails.Add("owner-ready: $($_.Exception.Message)")
    }
    try {
      if (-not $haveOrig) { throw "incomplete group orig" }
      $curH = Get-LiveBytesSha ([IO.File]::ReadAllBytes($SettingsPath))
      $out["settings_cur_sha256"] = $curH
      $out["settings_group_sha256"] = $GroupOrig.sha
      $byteEq = ($curH -eq $GroupOrig.sha)
      $out["settings_byte_equal"] = $byteEq
      if (-not $byteEq) { throw "settings differ from group orig" }
    } catch {
      $null = $fails.Add("settings-bytes: $($_.Exception.Message)")
    }
    try {
      if (-not $haveOrig) { throw "incomplete group orig" }
      $spiWant = (Get-Content -LiteralPath $GroupOrig.spi_path -Raw) | ConvertFrom-Json
      Assert-LiveSpiFields $spiWant "final stored"
      $spiNow = Get-LiveTaskbarSpi
      Assert-LiveSpiFields $spiNow "final current"
      $out["taskbar_spi_now"] = $spiNow
      $out["taskbar_spi_preimage"] = $spiWant
      $spiEq = ([int]$spiNow.arranging -eq [int]$spiWant.arranging -and
        [int]$spiNow.pen -eq [int]$spiWant.pen -and
        "$($spiNow.abm)" -eq "$($spiWant.abm)")
      $out["taskbar_spi_match"] = $spiEq
      if (-not $spiEq) { throw "taskbar/SPI differ from preimage" }
    } catch {
      $null = $fails.Add("taskbar-spi: $($_.Exception.Message)")
    }
    try {
      if (-not $haveNames) { throw "no owned names bound" }
      $filter = (@($OwnedNames) | ForEach-Object { "Name='$_'" }) -join " OR "
      $procs = @(Get-CimInstance Win32_Process -Filter $filter |
        ForEach-Object { @{ pid = $_.ProcessId; exe = $_.ExecutablePath; parent = $_.ParentProcessId } })
      $out["owned_process_residue"] = $procs
      $out["owned_process_count"] = $procs.Count
      if ($procs.Count -gt 0) { throw "owned process residue: $($procs.Count)" }
    } catch {
      $null = $fails.Add("process-residue: $($_.Exception.Message)")
    }
    try {
      $led = Join-Path $LedgerDir "ledger.json"
      $present = (Test-Path -LiteralPath $led)
      $out["ledger_present"] = $present
      if ($present) { throw "ledger present" }
    } catch {
      $null = $fails.Add("ledger: $($_.Exception.Message)")
    }
  } finally {
    if ($pushed) {
      try { Pop-Location } catch { $null = $fails.Add("location-pop: $($_.Exception.Message)") }
    }
    $out["failures"] = @($fails)
    try {
      $out | ConvertTo-Json -Depth 8 |
        Set-Content -LiteralPath $OutFile -Encoding utf8NoBOM
      $out["evidence_written"] = $true
    } catch {
      $out["evidence_written"] = $false
      $null = $fails.Add("evidence-write: $($_.Exception.Message)")
      $out["failures"] = @($fails)
    }
  }
  Write-Output ($out | ConvertTo-Json -Depth 8)
  if ($fails.Count -gt 0) { throw "final: $(($fails | ForEach-Object { "$_" }) -join '; ')" }
  Write-Output "LIVE-FINAL-CLEANUP-OK"
}

# Dot-source guard: reusable helpers above stay available when sourced;
# direct -File execution defines helpers and exits without live effects.
if ("$($MyInvocation.InvocationName)" -eq ".") { return }
Write-Output "windows-live-lib parsed (dot-source only; no live action without a driver)"
