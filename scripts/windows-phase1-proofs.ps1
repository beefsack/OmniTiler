# Phase 1 targeted Windows live proofs (fixture-only; no synthetic input).
#
# Bounded, independently runnable proofs over the normal `tile` loop:
#
#   graceful  - a graceful exact-owner `stop` itself restores a topmost float
#               and stripped fullscreen chrome (R12 preimages) before any
#               independent `restore` runs.
#   startup   - a restarted owner recovers this run's durable R12 preimages
#               before its first observation (its own trace, then native
#               readback) and re-adopts the durable float intent.
#   crash     - observes the product's own crash watcher restore under the
#               lease (marker identity, ledger, row stores, native bands)
#               before restarting; the restarted boot runs no second restore.
#   foreground- a departure whose arrival target is already foreground hides
#               through the staged focus-proof branch (executed nonactivating
#               async SetWindowPos post), never a fallback or activation path.
#
# Safety boundary (docs/live-windows-testing.md; this script grants no live
# authority): fixture-only scope via `--scope-exe tiler-live-fixture.exe`,
# no clicks/SendInput/keybd_event, no registry or settings writes, no broad
# process cleanup. Fixture focus is the only activation primitive. Every
# phase runs the shared abort-safe cleanup from `finally`, which stops only
# this run's exact owner identity, restores its ledger, shuts down its own
# fixture child, and verifies settings bytes/SPI.
#
# Usage (run from the repo root, no live scope implied):
#   pwsh -NoProfile -File scripts/windows-phase1-proofs.ps1 -Proof graceful
#   pwsh -NoProfile -File scripts/windows-phase1-proofs.ps1 -Proof startup
#   pwsh -NoProfile -File scripts/windows-phase1-proofs.ps1 -Proof crash
#   pwsh -NoProfile -File scripts/windows-phase1-proofs.ps1 -Proof foreground
#   pwsh -NoProfile -File scripts/windows-phase1-proofs.ps1 -Proof all
#
# Preconditions: `target/debug/tiler-windows.exe` and
# `target/debug/tiler-live-fixture.exe` are built (or CARGO_TARGET_DIR points
# at their directory) and there is no other live owner or ledger. The
# foreground leg chooses the next ordinal after the occupied fixture view.
# The operator remains responsible for the standing live-test authorization.

[CmdletBinding()]
param(
  [Alias("Proof")]
  [ValidateSet("graceful", "startup", "crash", "foreground", "all")]
  [string]$Phase1Proof = "all",
  [int]$OwnerSeconds = 300,
  [switch]$DefinitionsOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# `windows-world-stress.ps1` carries its own `param` block, so dot-sourcing it
# rebinds same-named caller variables. Preserve the caller's duration first.
$script:P1OwnerSeconds = $OwnerSeconds
$script:P1Repo = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot "windows-world-stress.ps1")
$OwnerSeconds = $script:P1OwnerSeconds

$script:P1Chrome = [uint32]0x00C40000
$script:P1WsExTopmost = [uint32]0x8
$script:P1WsMaximize = [uint32]0x01000000

if (-not ("P1Native" -as [type])) {
  $p1cs = @'
using System;
using System.Runtime.InteropServices;
[StructLayout(LayoutKind.Sequential)] public struct P1Rect { public int l; public int t; public int r; public int b; }
public static class P1Native {
  [DllImport("user32.dll")] public static extern IntPtr GetThreadDpiAwarenessContext();
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern long GetWindowLongPtrW(IntPtr h, int i);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, ref P1Rect r);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, ref P1Rect r, int s);
  [DllImport("dwmapi.dll", EntryPoint="DwmGetWindowAttribute")] public static extern int DwmGetWindowAttributeInt(IntPtr h, int a, ref int v, int s);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr GetPropW(IntPtr h, string name);
}
'@
  Add-Type -TypeDefinition $p1cs | Out-Null
}

function Get-P1Prop($Object, [string]$Name) {
  # Strict-mode-safe JSON/hashtable property read. Missing is always $null.
  if ($null -eq $Object) { return $null }
  if ($Object -is [System.Collections.IDictionary]) {
    if ($Object.Contains($Name)) { return $Object[$Name] }
    return $null
  }
  $property = $Object.PSObject.Properties[$Name]
  if ($null -eq $property) { return $null }
  return $property.Value
}

function Get-P1FallbackSourceProof {
  # Static provenance for the "staged, not fallback" claim: production core
  # constructs VisibilityTarget with fallback:false only. Test fixtures may
  # set fallback:true; this scan fails if production ever gains one.
  $root = Join-Path $script:P1Repo "crates\tiler-core\src"
  $prodTrue = @()
  $prodFalse = @()
  $testTrue = @()
  $files = @(Get-ChildItem -LiteralPath $root -Recurse -File -Filter "*.rs")
  foreach ($f in $files) {
    $rel = $f.FullName.Substring($root.Length + 1).Replace("\", "/")
    $isTest = ($rel -match '(^|/)tests(/|$)') -or ($f.Name -like "*_tests.rs")
    $lineNo = 0
    foreach ($line in (Get-Content -LiteralPath $f.FullName)) {
      $lineNo++
      if ($line -notmatch 'fallback\s*:\s*(true|false)') { continue }
      $rec = @{ file = $rel; line = $lineNo; value = "$($Matches[1])" }
      if ($isTest) {
        if ($rec.value -eq "true") { $testTrue += $rec }
      } elseif ($rec.value -eq "true") {
        $prodTrue += $rec
      } else {
        $prodFalse += $rec
      }
    }
  }
  if ($prodTrue.Count -ne 0) {
    throw "production core now constructs a fallback VisibilityTarget: $($prodTrue | ConvertTo-Json -Compress)"
  }
  foreach ($required in @("world_selection.rs", "world_sticky.rs")) {
    if (@($prodFalse | Where-Object { $_.file -eq $required }).Count -eq 0) {
      throw "production fallback:false construction missing from $required"
    }
  }
  if ($testTrue.Count -eq 0) {
    throw "no test-only fallback:true constructor found; provenance assumption changed"
  }
  return @{
    production_fallback_false = $prodFalse
    production_fallback_true = $prodTrue
    test_only_fallback_true = $testTrue
  }
}

function Get-P1Snap([uint64]$Hwnd) {
  # Read-only native evidence under a PerMonitorV2 thread context.
  $h = [IntPtr][long]$Hwnd
  $prior = [P1Native]::GetThreadDpiAwarenessContext()
  try {
    $null = [P1Native]::SetThreadDpiAwarenessContext([IntPtr](-4))
    $outer = New-Object P1Rect
    $vis = New-Object P1Rect
    $cloak = 0
    $okOuter = [P1Native]::GetWindowRect($h, [ref]$outer)
    $okVis = ([P1Native]::DwmGetWindowAttribute($h, 9, [ref]$vis, [Runtime.InteropServices.Marshal]::SizeOf($vis)) -eq 0)
    $okCloak = ([P1Native]::DwmGetWindowAttributeInt($h, 14, [ref]$cloak, 4) -eq 0)
    $style = [uint32]([P1Native]::GetWindowLongPtrW($h, -16) -band 0xFFFFFFFFL)
    $exstyle = [uint32]([P1Native]::GetWindowLongPtrW($h, -20) -band 0xFFFFFFFFL)
    return @{
      alive = [bool][P1Native]::IsWindow($h)
      visible = [bool][P1Native]::IsWindowVisible($h)
      iconic = [bool][P1Native]::IsIconic($h)
      cloak = $(if ($okCloak) { [int]$cloak } else { -1 })
      cloak_read = $okCloak
      style = $style
      exstyle = $exstyle
      topmost = (($exstyle -band $script:P1WsExTopmost) -ne 0)
      maximized = (($style -band $script:P1WsMaximize) -ne 0)
      caption = (($style -band 0x00C00000) -eq 0x00C00000)
      thick = (($style -band 0x00040000) -ne 0)
      dpi = [int][P1Native]::GetDpiForWindow($h)
      fg = ([P1Native]::GetForegroundWindow() -eq $h)
      ok_outer = $okOuter
      ok_vis = $okVis
      outer = @([int]$outer.l, [int]$outer.t, [int]$outer.r, [int]$outer.b)
      vis = @([int]$vis.l, [int]$vis.t, [int]$vis.r, [int]$vis.b)
    }
  } finally {
    $null = [P1Native]::SetThreadDpiAwarenessContext($prior)
  }
}

function Test-P1Prop([uint64]$Hwnd, [string]$Name) {
  return ([P1Native]::GetPropW([IntPtr][long]$Hwnd, $Name) -ne [IntPtr]::Zero)
}

function Get-P1Row($Event, [string]$Token) {
  $rows = @(Get-P1Prop $Event "desired_windows")
  $hits = @($rows | Where-Object { "$(Get-P1Prop $_ 'window')" -eq "$Token" })
  if ($hits.Count -eq 0) { return $null }
  return $hits[0]
}

function Get-P1StateAttempt($Event, [string]$Token, [string]$Property, $Target) {
  foreach ($attempt in @(Get-P1Prop $Event "states")) {
    if ("$(Get-P1Prop $attempt 'window')" -ne "$Token") { continue }
    if ("$(Get-P1Prop $attempt 'property')" -ne "$Property") { continue }
    if ((Get-P1Prop $attempt "target") -ne $Target) { continue }
    return $attempt
  }
  return $null
}

function Get-P1RestoreEvent([string]$LogPath) {
  # The tile loop logs startup recovery before the first observation; that
  # row carries no `phase` field and appears only when count > 0.
  foreach ($e in @(Read-StressOwnerLog $LogPath)) {
    if ("$(Get-P1Prop $e 'event')" -ne "durable-preimage-restore") { continue }
    if ("$(Get-P1Prop $e 'outcome')" -ne "restored") { continue }
    if ("$(Get-P1Prop $e 'phase')" -ne "") { continue }
    return $e
  }
  return $null
}

function Wait-P1StateWrite([string]$LogPath, [int]$Mark, [string]$Token, [string]$Property, $Target, [int]$Timeout, [string]$Tag) {
  # A product fullscreen enter emits no `fullscreen::carry` event (that fires
  # for a suppressed carry too) and, once the safety exclusion lands, no
  # app-owned overlay row, so the enter proof joins the accepted state write
  # from the product's own trace: a true target with `pre=false` proves the
  # transition. Native readback still proves the chrome mutation.
  $deadline = (Get-Date).AddSeconds($Timeout)
  while ((Get-Date) -lt $deadline) {
    $events = @(Read-StressOwnerLog $LogPath)
    for ($i = $Mark; $i -lt $events.Count; $i++) {
      $e = $events[$i]
      if ("$(Get-P1Prop $e 'event')" -ne "world-apply") { continue }
      if ("$(Get-P1Prop $e 'delivery')" -notin @("accepted", "command-consumed")) { continue }
      $attempt = Get-P1StateAttempt $e $Token $Property $Target
      if ($null -eq $attempt) { continue }
      if ("$(Get-P1Prop $attempt 'setter')" -ne "accepted") { continue }
      return @{ event = $e; index = $i; attempt = $attempt }
    }
    Start-Sleep -Milliseconds 60
  }
  throw "${Tag}: $Property state-write join timeout"
}

function Test-P1Tiled($Event, [string]$Token) {
  $row = Get-P1Row $Event $Token
  if ($null -eq $row) { return $false }
  return ("$(Get-P1Prop $row 'frame_authority')" -eq "controlled" -and
    $null -ne (Get-P1Prop $row 'effective_frame') -and
    $null -ne (Get-P1Prop $row 'allocation'))
}

function Test-P1Floating($Event, [string]$Token) {
  # Ordinary float rows carry no effective frame (the adapter owns the live
  # float geometry); `floating` plus the untouched/controlled authority is
  # the report contract, never an invented tiled frame.
  $row = Get-P1Row $Event $Token
  if ($null -eq $row) { return $false }
  $authority = "$(Get-P1Prop $row 'frame_authority')"
  return ((Get-P1Prop $row 'floating') -eq $true -and
    $authority -in @("controlled", "untouched"))
}

function Test-P1Untouched($Event, [string]$Token) {
  $row = Get-P1Row $Event $Token
  if ($null -eq $row) { return $false }
  return ("$(Get-P1Prop $row 'frame_authority')" -eq "untouched" -and
    $null -eq (Get-P1Prop $row 'effective_frame') -and
    $null -ne (Get-P1Prop $row 'allocation') -and
    "$(Get-P1Prop $row 'frame_op')" -eq "")
}

function Assert-P1NoPending($Event, [string]$Tag) {
  foreach ($field in @("desired_transfers", "desired_operations", "held_scopes")) {
    if (@(Get-P1Prop $Event $field).Count -ne 0) {
      throw "${Tag}: pending $field $((@(Get-P1Prop $Event $field)) | ConvertTo-Json -Compress)"
    }
  }
  $stats = Get-P1Prop $Event "stats"
  $warps = Get-P1Prop $stats "cursor_warps"
  if ($null -ne $warps -and [int]$warps -ne 0) { throw "${Tag}: cursor warps $warps" }
}

function Wait-P1Cond([scriptblock]$Pred, [int]$Timeout, [string]$Tag) {
  $deadline = (Get-Date).AddSeconds($Timeout)
  while ((Get-Date) -lt $deadline) {
    if (& $Pred) { return $true }
    Start-Sleep -Milliseconds 100
  }
  throw "${Tag}: condition timeout"
}

function Wait-P1Event([string]$LogPath, [int]$Mark, [scriptblock]$Pred, [int]$Timeout, [string]$Tag) {
  $deadline = (Get-Date).AddSeconds($Timeout)
  while ((Get-Date) -lt $deadline) {
    $events = @(Read-StressOwnerLog $LogPath)
    for ($i = $Mark; $i -lt $events.Count; $i++) {
      if (& $Pred $events[$i]) { return @{ event = $events[$i]; index = $i } }
    }
    Start-Sleep -Milliseconds 60
  }
  throw "${Tag}: event join timeout"
}

# Unlike the stress matrix, this proof deliberately includes a settled float.
function Test-P1Pending($Event) {
  foreach ($field in @("desired_transfers", "desired_operations", "held_scopes")) {
    if (@($Event.$field).Count -ne 0) { return $true }
  }
  foreach ($row in @($Event.desired_windows)) {
    if ("$($row.frame_op)" -ne "" -or $row.automatic_clear_pending -ne $false -or
        "$($row.frame_hold)" -ne "" -or "$($row.membership_hold)" -ne "") { return $true }
  }
  return $false
}

function Wait-P1SettledWorld([string]$LogPath, [int]$Mark, [scriptblock]$Pred, [int]$Timeout, [string]$Tag) {
  $deadline = (Get-Date).AddSeconds($Timeout)
  while ((Get-Date) -lt $deadline) {
    $events = @(Read-StressOwnerLog $LogPath)
    for ($i = $Mark; $i -lt $events.Count; $i++) {
      $e = $events[$i]
      if ("$(Get-P1Prop $e 'event')" -ne "world-apply") { continue }
      $delivery = "$(Get-P1Prop $e 'delivery')"
      if ($delivery -notin @("accepted", "command-consumed")) { continue }
      if (Test-P1Pending $e) { continue }
      if (& $Pred $e) { return @{ event = $e; index = $i } }
    }
    Start-Sleep -Milliseconds 60
  }
  throw "${Tag}: settled world-row join timeout"
}

function Get-P1LogMark([string]$LogPath) {
  if (-not (Test-Path -LiteralPath $LogPath -PathType Leaf)) { return 0 }
  return @(Read-StressOwnerLog $LogPath).Count
}

function Get-P1EventsSince([string]$LogPath, [int]$Mark) {
  $events = @(Read-StressOwnerLog $LogPath)
  if ($Mark -ge $events.Count) { return @() }
  return @($events | Select-Object -Skip $Mark)
}

function Read-P1PreimageRows([string]$LedgerDir, [string]$File) {
  $path = Join-Path $LedgerDir $File
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
    return @{ path = $path; present = $false; text = ""; rows = @() }
  }
  $text = (Get-Content -LiteralPath $path -Raw).Trim()
  $doc = $text | ConvertFrom-Json
  if ($null -eq $doc -or $doc.v -ne 1 -or $doc.rows -isnot [array]) {
    throw "unreadable R12 store: $File"
  }
  $rows = @($doc.rows)
  return @{ path = $path; present = $true; text = $text; rows = $rows }
}

function Get-P1SessionRows([string]$LedgerDir, [string]$File, [string]$Session) {
  return @(@((Read-P1PreimageRows $LedgerDir $File).rows) |
      Where-Object { "$(Get-P1Prop $_ 'session')" -eq "$Session" })
}

function Read-P1WatcherMarker([string]$LedgerDir, [string]$OwnerCreation) {
  # `spawn_watcher` publishes `watcher-<owner_creation>.ready` (owner
  # creation, watcher pid, watcher creation) before the owner reports ready;
  # the watcher removes it after its restore and before it exits.
  $path = Join-Path $LedgerDir "watcher-$OwnerCreation.ready"
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return $null }
  $lines = @((Get-Content -LiteralPath $path -Raw) -split "\r?\n" | Where-Object { "$_".Trim() -ne "" })
  if ($lines.Count -lt 3) { return $null }
  $watcherPid = 0
  if (-not [int]::TryParse("$($lines[1])".Trim(), [ref]$watcherPid) -or $watcherPid -eq 0) { return $null }
  return @{
    path = $path
    owner_creation = "$($lines[0])".Trim()
    pid = $watcherPid
    creation = "$($lines[2])".Trim()
  }
}

function Wait-P1WatcherMarker([string]$LedgerDir, [string]$OwnerCreation, [int]$Timeout, [string]$Tag) {
  $deadline = (Get-Date).AddSeconds($Timeout)
  while ((Get-Date) -lt $deadline) {
    $marker = Read-P1WatcherMarker $LedgerDir $OwnerCreation
    if ($null -ne $marker) { return $marker }
    Start-Sleep -Milliseconds 100
  }
  throw "${Tag}: crash watcher ready marker never appeared under $LedgerDir"
}

function Get-P1LiveProcess([int]$ProcessId, [string]$Creation, [string]$ExePath) {
  # Exact identity only: a recycled pid with another creation or image is
  # never the process a marker named.
  try {
    $p = Get-Process -Id $ProcessId -ErrorAction Stop
    $live = "{0:x16}" -f $p.StartTime.ToUniversalTime().ToFileTimeUtc()
    if ($live -ne "$Creation") { return $null }
    if ("$($p.Path)" -ine "$ExePath") { return $null }
    return $p
  } catch {
    return $null
  }
}

function Assert-P1CleanStart([string]$Tag) {
  $dir = Assert-NoPreexistingStressOwner $script:P1OwnerCopy $Tag
  foreach ($file in @("above-preimages.json", "fullscreen-preimages.json")) {
    if (@((Read-P1PreimageRows $dir $file).rows).Count -ne 0) {
      throw "${Tag}: existing R12 recovery rows; no fixture launch"
    }
  }
  return $dir
}

function Wait-P1Rows([string]$LedgerDir, [string]$File, [int]$Count, [int]$Timeout, [string]$Tag) {
  $deadline = (Get-Date).AddSeconds($Timeout)
  $last = $null
  while ((Get-Date) -lt $deadline) {
    $last = Read-P1PreimageRows $LedgerDir $File
    if (@($last.rows).Count -eq $Count) { return $last }
    Start-Sleep -Milliseconds 100
  }
  throw "${Tag}: $File rows never reached $Count; last=$($last.text)"
}

function Wait-P1NativeVis([uint64]$Hwnd, [int]$X, [int]$Y, [int]$W, [int]$H, [int]$Tol, [int]$Timeout, [string]$Tag) {
  $deadline = (Get-Date).AddSeconds($Timeout)
  $last = $null
  while ((Get-Date) -lt $deadline) {
    $snap = Get-P1Snap $Hwnd
    $last = $snap
    if ($snap.ok_vis) {
      $v = $snap.vis
      if ([math]::Abs($v[0] - $X) -le $Tol -and [math]::Abs($v[1] - $Y) -le $Tol -and
          [math]::Abs(($v[2] - $v[0]) - $W) -le $Tol -and [math]::Abs(($v[3] - $v[1]) - $H) -le $Tol) {
        return $snap
      }
    }
    Start-Sleep -Milliseconds 100
  }
  throw "${Tag}: DWM visible frame never reached $X,$Y ${W}x${H} (tol $Tol); last=$($last.vis -join ',')"
}

function Test-P1VisEqual($A, $B, [int]$Tol = 3) {
  if ($null -eq $A -or $null -eq $B) { return $false }
  if (@($A).Count -ne 4 -or @($B).Count -ne 4) { return $false }
  return ([math]::Abs($A[0] - $B[0]) -le $Tol -and
    [math]::Abs($A[1] - $B[1]) -le $Tol -and
    [math]::Abs(($A[2] - $A[0]) - ($B[2] - $B[0])) -le $Tol -and
    [math]::Abs(($A[3] - $A[1]) - ($B[3] - $B[1])) -le $Tol)
}

function Start-P1Owner([string]$OwnerCopy, [string]$WorkDir, [string]$Tag, $Ctx) {
  $argLine = "tile --user-start --seconds $($script:P1OwnerSeconds) --trace --scope-exe tiler-live-fixture.exe --inner-gap 0 --outer-gap 0"
  $Ctx.OwnerLaunchIntended = $true
  $null = Start-ExplorerGui $OwnerCopy $argLine $WorkDir
  $Ctx.OwnerLaunched = $true
  $ready = $null
  $deadline = (Get-Date).AddSeconds(30)
  while ((Get-Date) -lt $deadline) {
    try {
      $j = Invoke-LiveNative $OwnerCopy @("ready") | ConvertFrom-Json
      if ((Get-P1Prop $j "ready") -eq $true) { $ready = $j; break }
    } catch {}
    Start-Sleep -Milliseconds 400
  }
  if ($null -eq $ready) { throw "${Tag}: owner ready timeout (launched owner stays bound for cleanup)" }
  $readyLog = "$(Get-P1Prop $ready 'log_path')"
  if ($readyLog -eq "") { throw "${Tag}: owner ready carried no log path" }
  $readyOwner = Get-P1Prop $ready "owner"
  if (-not (Test-ExeEqual "$(Get-P1Prop $readyOwner 'exe_path')" "$OwnerCopy")) {
    throw "${Tag}: owner peer mismatch"
  }
  Assert-ParentIsExplorer ([int](Get-P1Prop $readyOwner "pid"))
  $bind = @{
    pid = [int](Get-P1Prop $readyOwner "pid")
    process_creation = "$(Get-P1Prop $readyOwner 'process_creation')"
    exe_path = "$(Get-P1Prop $readyOwner 'exe_path')"
  }
  if ($null -ne (Get-P1Prop $readyOwner "user_sid")) { $bind.user_sid = "$(Get-P1Prop $readyOwner 'user_sid')" }
  if ($null -ne (Get-P1Prop $readyOwner "session_id")) { $bind.session_id = [int](Get-P1Prop $readyOwner "session_id") }
  $Ctx.Owner = $bind
  return @{ ready = $ready; bind = $bind; log = $readyLog }
}

function Stop-P1Crash([string]$OwnerCopy, $Bind, [string]$Tag) {
  $p = Get-Process -Id ([int]$Bind.pid) -ErrorAction Stop
  if ("$($p.Path)" -ine "$OwnerCopy") { throw "${Tag}: owner PID reused by another exe" }
  $hex = "{0:x16}" -f $p.StartTime.ToUniversalTime().ToFileTimeUtc()
  if ("$hex" -ine "$($Bind.process_creation)") { throw "${Tag}: owner PID reused by another creation ($hex)" }
  $p.Kill()
  $deadline = (Get-Date).AddSeconds(10)
  while ((Get-Date) -lt $deadline) {
    try {
      $null = Get-Process -Id ([int]$Bind.pid) -ErrorAction Stop
      Start-Sleep -Milliseconds 200
    } catch {
      if ("$($_.FullyQualifiedErrorId)" -like "NoProcessFound*") { return }
      throw
    }
  }
  throw "${Tag}: exact-PID kill left the process alive"
}

function Write-P1Log([string]$RunDir, [string]$Message) {
  $line = "$(Get-Date -Format 'HH:mm:ss.fff') $Message"
  Add-Content -LiteralPath (Join-Path $RunDir "driver.log") -Value $line
  Write-Output $line
}

function New-P1Context([string]$FixtureExe, [string]$SettingsPath) {
  return @{
    OwnerPayload = $script:P1OwnerCopy
    Owner = $null
    OwnerLaunched = $false
    OwnerLaunchIntended = $false
    FixtureExe = $FixtureExe
    FixtureProc = $null
    FixtureCreation = ""
    Socket = $null
    Nonce = $null
    SettingsPath = $SettingsPath
    GroupOrig = $null
    LedgerDir = $null
    SettingsChanged = @()
    SettingsConfigIntended = $false
  }
}

function Start-P1FixtureAndOwner([string]$FixtureCopy, $Ctx, [string]$Tag) {
  $started = Start-StressFixture $FixtureCopy
  $Ctx.FixtureProc = $started.proc
  $ann = $started.announcement
  $Ctx.Socket = "$(Get-P1Prop $ann 'socket')"
  $Ctx.Nonce = "$(Get-P1Prop $ann 'nonce')"
  $serverPid = [int](Get-P1Prop $ann "pid")
  $serverProc = Get-Process -Id $serverPid -ErrorAction Stop
  $Ctx.FixtureCreation = "{0:x16}" -f $serverProc.StartTime.ToUniversalTime().ToFileTimeUtc()
  $launch = Start-P1Owner $script:P1OwnerCopy $script:P1BinDir $Tag $Ctx
  $fixtureState = @{ pid = $serverPid; creation = $Ctx.FixtureCreation }
  return @{ start = $started; launch = $launch; fixtureState = $fixtureState; serverPid = $serverPid }
}

function Spawn-P1Window([string]$FixtureExe, $Ctx, [string]$Id, [int]$X, [int]$Y, [int]$W, [int]$H, [bool]$Decorations) {
  $body = @{
    v = 1; request_id = (New-StressReqId "spawn"); op = "spawn"; windows = @(
      @{ id = "$Id"; size_px = @($W, $H); position_outer_px = @($X, $Y); decorations = $Decorations })
  } | ConvertTo-Json -Depth 6 -Compress
  $r = Send-StressFixture $FixtureExe $Ctx.Socket $Ctx.Nonce $body
  $windows = @(Get-P1Prop (Get-P1Prop $r "result") "windows")
  $entry = @($windows | Where-Object { "$(Get-P1Prop $_ 'id')" -eq "$Id" }) | Select-Object -First 1
  if ($null -eq $entry -or (Get-P1Prop $entry "ok") -ne $true) {
    throw "fixture spawn refused ${Id}: $($r | ConvertTo-Json -Compress -Depth 6)"
  }
}

function Invoke-P1Graceful {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $runDir = Join-Path $script:P1TargetDir "task9\phase1-graceful-$stamp-$PID"
  New-Item -ItemType Directory -Force -Path $runDir | Out-Null
  $settingsPath = $script:P1SettingsPath
  $origPath = Join-Path $runDir "settings-orig.json"
  $spiPath = Join-Path $runDir "taskbar-spi.json"
  $evidencePath = Join-Path $runDir "evidence.json"
  $ctx = New-P1Context $script:P1FixtureCopy $settingsPath
  $evidence = [ordered]@{
    proof = "phase1-graceful-stop"
    started_utc = (Get-Date).ToUniversalTime().ToString("o")
    head = ((& git -C $script:P1Repo rev-parse HEAD) | Out-String).Trim()
    git_status = ((& git -C $script:P1Repo status --short) | Out-String).Trim()
    owner_sha256 = (Get-FileHash -LiteralPath $script:P1OwnerCopy -Algorithm SHA256).Hash
    fixture_sha256 = (Get-FileHash -LiteralPath $script:P1FixtureCopy -Algorithm SHA256).Hash
    safety = [ordered]@{
      fixture_only_scope = "--scope-exe tiler-live-fixture.exe"
      synthetic_input = $false
      independent_restore_called_before_assertions = $false
      independent_restore_note = "Invoke-StressCleanup runs after the post-stop assertions; the proof is the state observed immediately after the exact-owner stop."
    }
    preconditions = [ordered]@{
      owner_binary = $script:P1OwnerCopy
      fixture_binary = $script:P1FixtureCopy
      settings_path = $settingsPath
      workspace_rule = "fixture windows tile on the current fixture home workspace; no numbered workspace switch is used"
      preexisting_owner_checked = $true
    }
    stages = [ordered]@{}
  }
  $failure = $null
  $orig = $null
  try {
    $ledgerDir = Assert-P1CleanStart "phase1-graceful-preflight"
    $ctx.LedgerDir = $ledgerDir
    $orig = Save-LiveSettingsPreimage $settingsPath $origPath $spiPath ""
    $ctx.GroupOrig = $orig
    $ctx.SettingsConfigIntended = $true
    $evidence.stages.preflight = @{ ledger_dir = "$ledgerDir"; settings_sha_before = "$($orig.sha)" }

    $boot = Start-P1FixtureAndOwner $script:P1FixtureCopy $ctx "phase1-graceful"
    $fixtureCopy = $script:P1FixtureCopy
    $logPath = $boot.launch.log
    $bind0 = $boot.launch.bind
    $fixtureState = $boot.fixtureState
    $base = Get-StressDisplayBaseline
    $wx = [int]$base.work[0]
    $wy = [int]$base.work[1]
    Spawn-P1Window $fixtureCopy $ctx "alpha" ($wx + 120) ($wy + 100) 700 460 $true
    Spawn-P1Window $fixtureCopy $ctx "beta" ($wx + 420) ($wy + 220) 660 430 $true
    $hwndA = Wait-StressHwnd "alpha" $fixtureState 30
    $hwndB = Wait-StressHwnd "beta" $fixtureState 30
    $hwndOf = @{ alpha = $hwndA; beta = $hwndB }
    $evidence.stages.fixture = @{ pid = $boot.serverPid; hwnd_alpha = $hwndA; hwnd_beta = $hwndB }
    $snapA0 = Get-P1Snap $hwndA
    $snapB0 = Get-P1Snap $hwndB
    if (-not $snapA0.caption -or -not $snapA0.thick) { throw "alpha did not spawn with caption/thickframe chrome" }
    if (-not $snapB0.caption -or -not $snapB0.thick) { throw "beta did not spawn with caption/thickframe chrome" }

    $tokOf = @{}
    $tokA = Wait-StressAdmitted $logPath $tokOf $hwndA $fixtureState 60 "alpha"
    $tokOf[$tokA] = "alpha"
    $tokB = Wait-StressAdmitted $logPath $tokOf $hwndB $fixtureState 60 "beta"
    $tokOf[$tokB] = "beta"
    $pre = Wait-P1SettledWorld $logPath 0 { param($e) (Test-P1Tiled $e $tokA) -and (Test-P1Tiled $e $tokB) } 60 "pre-tiled"
    Assert-P1NoPending $pre.event "pre-tiled"
    $preA = Get-P1Row $pre.event $tokA
    $preB = Get-P1Row $pre.event $tokB
    $preSnapA = Wait-P1NativeVis $hwndA ([int](Get-P1Prop (Get-P1Prop $preA "effective_frame") "x")) ([int](Get-P1Prop (Get-P1Prop $preA "effective_frame") "y")) ([int](Get-P1Prop (Get-P1Prop $preA "effective_frame") "w")) ([int](Get-P1Prop (Get-P1Prop $preA "effective_frame") "h")) 3 20 "pre-alpha"
    $preSnapB = Wait-P1NativeVis $hwndB ([int](Get-P1Prop (Get-P1Prop $preB "effective_frame") "x")) ([int](Get-P1Prop (Get-P1Prop $preB "effective_frame") "y")) ([int](Get-P1Prop (Get-P1Prop $preB "effective_frame") "w")) ([int](Get-P1Prop (Get-P1Prop $preB "effective_frame") "h")) 3 20 "pre-beta"
    $preVisA = $preSnapA.vis
    $preVisB = $preSnapB.vis
    $evidence.stages.pre_tiled = @{
      frame_alpha = (Get-P1Prop $preA "effective_frame")
      frame_beta = (Get-P1Prop $preB "effective_frame")
      vis_alpha = $preVisA
      vis_beta = $preVisB
      ws_alpha = "$(Get-P1Prop $preA 'ws')"
      ws_beta = "$(Get-P1Prop $preB 'ws')"
    }

    # --- topmost float preimage -------------------------------------------------
    $null = Invoke-StressFocus $fixtureCopy $ctx.Socket $ctx.Nonce "alpha" $hwndOf $fixtureState 15
    $floatMark = Get-P1LogMark $logPath
    $floatOut = Invoke-LiveNative $script:P1OwnerCopy @("workspace", "--float")
    $floatJson = $floatOut | ConvertFrom-Json
    if ((Get-P1Prop $floatJson "dispatched") -ne $true) { throw "workspace --float not dispatched: $floatOut" }
    $floatRow = Wait-P1Event $logPath $floatMark {
      param($e)
      return ("$(Get-P1Prop $e 'event')" -eq "world-apply" -and
        "$(Get-P1Prop $e 'delivery')" -in @("accepted", "command-consumed") -and
        (Test-P1Floating $e $tokA))
    } 25 "float-row"
    $rowsAbove = Wait-P1Rows $ledgerDir "above-preimages.json" 1 15 "float-preimage"
    $rowAbove = @($rowsAbove.rows)[0]
    $null = Wait-P1Cond { (Get-P1Snap $hwndA).topmost } 15 "float-topmost"
    $snapFloat = Get-P1Snap $hwndA
    if (-not $snapFloat.topmost) { throw "float did not raise WS_EX_TOPMOST" }
    if ([uint64](Get-P1Prop $rowAbove "hwnd") -ne $hwndA) { throw "above preimage names another window: $($rowsAbove.text)" }
    if ((Get-P1Prop $rowAbove "priorTopmost") -ne $false) { throw "above preimage priorTopmost is not false: $($rowsAbove.text)" }
    if ("$(Get-P1Prop $rowAbove 'session')" -ne "$($bind0.process_creation)") { throw "above preimage session mismatch: $($rowsAbove.text)" }
    # Floating alpha removes beta from the pair, so beta's final pre-fullscreen
    # tile is the frame it holds after that retile, not the initial split.
    $betaTileRow = Wait-P1SettledWorld $logPath $floatMark {
      param($e)
      return ((Test-P1Floating $e $tokA) -and (Test-P1Tiled $e $tokB))
    } 20 "float-beta-retile"
    $betaTile = Get-P1Row $betaTileRow.event $tokB
    $betaSnap = Wait-P1NativeVis $hwndB ([int](Get-P1Prop (Get-P1Prop $betaTile "effective_frame") "x")) ([int](Get-P1Prop (Get-P1Prop $betaTile "effective_frame") "y")) ([int](Get-P1Prop (Get-P1Prop $betaTile "effective_frame") "w")) ([int](Get-P1Prop (Get-P1Prop $betaTile "effective_frame") "h")) 3 20 "float-beta-frame"
    $preFullscreenVisB = $betaSnap.vis
    $evidence.stages.float = @{
      tick = [int64](Get-P1Prop $floatRow.event "tick")
      native = $snapFloat
      row = $rowAbove
      beta_pre_fullscreen_frame = (Get-P1Prop $betaTile "effective_frame")
      beta_pre_fullscreen_vis = $preFullscreenVisB
    }

    # --- stripped fullscreen chrome preimage -----------------------------------
    $null = Invoke-StressFocus $fixtureCopy $ctx.Socket $ctx.Nonce "beta" $hwndOf $fixtureState 15
    $fsMark = Get-P1LogMark $logPath
    $fsOut = Invoke-LiveNative $script:P1OwnerCopy @("workspace", "--fullscreen")
    $fsJson = $fsOut | ConvertFrom-Json
    if ((Get-P1Prop $fsJson "dispatched") -ne $true) { throw "workspace --fullscreen not dispatched: $fsOut" }
    $fsToggle = Wait-P1Event $logPath $fsMark {
      param($e)
      return ("$(Get-P1Prop $e 'event')" -eq "fullscreen-toggle" -and
        "$(Get-P1Prop $e 'outcome')" -eq "accepted" -and
        "$(Get-P1Prop $e 'target')" -eq "fullscreen" -and
        "$(Get-P1Prop $e 'window')" -eq "$tokB")
    } 25 "fullscreen-toggle"
    $enter = Wait-P1StateWrite $logPath $fsMark $tokB "fullscreen" $true 30 "fullscreen-row"
    if ("$(Get-P1Prop $enter.event 'command')" -ne "toggle-fullscreen") {
      throw "fullscreen state write did not ride the toggle-fullscreen apply: $($enter.event | ConvertTo-Json -Compress -Depth 6)"
    }
    if ((Get-P1Prop $enter.attempt "pre") -ne $false -or (Get-P1Prop $enter.attempt "post") -ne $true) {
      throw "fullscreen state write did not prove pre=false -> post=true: $($enter.attempt | ConvertTo-Json -Compress)"
    }
    # The product's own fullscreen exclusion holds the workspace afterward, so
    # no app-owned overlay row is expected. A stale untouched-overlay
    # classification would still be the pre-review defect: it must never appear.
    foreach ($e in @(Get-P1EventsSince $logPath $enter.index)) {
      if ("$(Get-P1Prop $e 'event')" -ne "world-apply") { continue }
      if (Test-P1Untouched $e $tokB) {
        throw "post-fullscreen apply still treated beta as an app-owned untouched overlay"
      }
    }
    $rowsFs = Wait-P1Rows $ledgerDir "fullscreen-preimages.json" 1 20 "fullscreen-preimage"
    $rowFs = @($rowsFs.rows)[0]
    if ([uint64](Get-P1Prop $rowFs "hwnd") -ne $hwndB) { throw "fullscreen preimage names another window: $($rowsFs.text)" }
    if ((([uint32](Get-P1Prop $rowFs "changed")) -band $script:P1Chrome) -eq 0) { throw "fullscreen preimage carries no chrome bits: $($rowsFs.text)" }
    if ("$(Get-P1Prop $rowFs 'session')" -ne "$($bind0.process_creation)") { throw "fullscreen preimage session mismatch: $($rowsFs.text)" }
    if ((Get-P1Prop $rowFs "wasMaximized") -ne $false) { throw "unexpected maximized fullscreen entry: $($rowsFs.text)" }
    $fsFrame = Get-P1Prop $rowFs "frame"
    if ($null -eq $fsFrame) { throw "normal fullscreen entry carries no restoration frame: $($rowsFs.text)" }
    # The durable row stores the live GetWindowRect outer rect (x=left, y=top,
    # w=right-left, h=bottom-top; fullscreen.rs `live_outer_rect`), never the
    # DWM-visible world frame. Compare against the independently sampled outer
    # rect; the visible frame was already proven against effective_frame by
    # Wait-P1NativeVis above.
    if (-not $betaSnap.ok_outer) {
      throw "beta outer rect unreadable before fullscreen; cannot verify the durable restoration frame"
    }
    $betaOuter = @($betaSnap.outer)
    $expectOuter = @{
      x = [int]$betaOuter[0]
      y = [int]$betaOuter[1]
      w = [int]$betaOuter[2] - [int]$betaOuter[0]
      h = [int]$betaOuter[3] - [int]$betaOuter[1]
    }
    foreach ($k in @("x", "y", "w", "h")) {
      if ([math]::Abs([int](Get-P1Prop $fsFrame $k) - $expectOuter[$k]) -gt 2) {
        throw "fullscreen preimage frame differs from beta's pre-fullscreen outer rect on ${k}: row=$($fsFrame | ConvertTo-Json -Compress) outer=$($betaOuter -join ',')"
      }
    }
    $null = Wait-P1Cond {
      $s = Get-P1Snap $hwndB
      return (($s.style -band $script:P1Chrome) -eq 0 -and (Test-P1Prop $hwndB "OmniTilerFullscreen"))
    } 20 "fullscreen-native"
    $snapFs = Get-P1Snap $hwndB
    if (($snapFs.style -band $script:P1Chrome) -ne 0) { throw "fullscreen did not strip caption/thickframe" }
    if (-not (Test-P1Prop $hwndB "OmniTilerFullscreen")) { throw "fullscreen marker property missing on enter" }
    $evidence.stages.fullscreen = @{
      tick = [int64](Get-P1Prop $enter.event "tick")
      row = $rowFs
      native = $snapFs
      marker = $true
      state_write = $enter.attempt
      toggle = $fsToggle.event
      pre_fullscreen_outer = $betaOuter
      frame_coordinate_space = "GetWindowRect outer (x=left,y=top,w=right-left,h=bottom-top); world effective_frame is the DWM-visible rect"
      command_apply = @{
        command = "$(Get-P1Prop $enter.event 'command')"
        row = (Get-P1Row $enter.event $tokB)
        held_scopes = @(Get-P1Prop $enter.event "held_scopes")
      }
    }

    # --- graceful stop: the only restoration authority before assertions ------
    $aboveBeforeStop = Read-P1PreimageRows $ledgerDir "above-preimages.json"
    $fsBeforeStop = Read-P1PreimageRows $ledgerDir "fullscreen-preimages.json"
    if (@($aboveBeforeStop.rows).Count -ne 1 -or @($fsBeforeStop.rows).Count -ne 1) {
      throw "pre-stop preimages not both durable: above=$($aboveBeforeStop.text) fullscreen=$($fsBeforeStop.text)"
    }
    $preStopA = Get-P1Snap $hwndA
    $preStopB = Get-P1Snap $hwndB
    $evidence.stages.pre_stop = @{
      above_rows = @($aboveBeforeStop.rows).Count
      fullscreen_rows = @($fsBeforeStop.rows).Count
      alpha_topmost = $preStopA.topmost
      beta_chrome = @{ caption = $preStopB.caption; thick = $preStopB.thick }
      beta_vis = $preStopB.vis
      beta_pre_fullscreen_vis = $preFullscreenVisB
    }

    $stopRaw = Invoke-LiveNative $script:P1OwnerCopy @("stop")
    $stop = $stopRaw | ConvertFrom-Json
    $evidence.stages.stop = @{
      cli = $stop
      independent_restore_called = $false
    }
    if ((Get-P1Prop $stop "stopped") -ne $true) { throw "graceful stop did not report stopped: $stopRaw" }
    if ((Get-P1Prop $stop "owner_exited") -ne $true) { throw "graceful stop did not confirm owner_exited: $stopRaw" }

    # Post-stop readbacks come before Invoke-StressCleanup. A `restore` at
    # this point would be an independent authority, which this proof excludes.
    # Wait only for the owner's own synchronous teardown writes/compositor
    # readback to settle; no second process or CLI is involved.
    $null = Wait-P1Cond {
      $a = Get-P1Snap $hwndA
      $b = Get-P1Snap $hwndB
      return ((-not $a.topmost) -and $b.caption -and $b.thick -and
        ($b.style -band $script:P1Chrome) -eq $script:P1Chrome -and
        (-not (Test-P1Prop $hwndB "OmniTilerFullscreen")) -and
        $b.ok_vis -and (Test-P1VisEqual $b.vis $preFullscreenVisB 3))
    } 10 "post-stop-native"
    $postA = Get-P1Snap $hwndA
    $postB = Get-P1Snap $hwndB
    $aboveAfter = Read-P1PreimageRows $ledgerDir "above-preimages.json"
    $fsAfter = Read-P1PreimageRows $ledgerDir "fullscreen-preimages.json"
    if ($postA.topmost) { throw "graceful stop left alpha WS_EX_TOPMOST raised" }
    if (-not $postB.caption -or -not $postB.thick) {
      throw "graceful stop did not restore beta caption/thickframe: style=0x$('{0:x8}' -f $postB.style)"
    }
    if (@($aboveAfter.rows).Count -ne 0) { throw "graceful stop left above preimage rows: $($aboveAfter.text)" }
    if (@($fsAfter.rows).Count -ne 0) { throw "graceful stop left fullscreen preimage rows: $($fsAfter.text)" }
    if (Test-P1Prop $hwndB "OmniTilerFullscreen") { throw "graceful stop left the fullscreen marker" }
    if (-not $postA.visible -or $postA.iconic) { throw "graceful stop left alpha hidden/iconic" }
    if (-not $postB.visible -or $postB.iconic) { throw "graceful stop left beta hidden/iconic" }
    if (-not $postB.ok_vis) { throw "beta DWM visible frame unreadable after stop" }
    if (-not (Test-P1VisEqual $postB.vis $preFullscreenVisB 3)) {
      throw "graceful stop restored beta to $($postB.vis -join ',') but the pre-fullscreen tile was $($preFullscreenVisB -join ',')"
    }

    $events = @(Read-StressOwnerLog $logPath)
    $teardown = @($events | Where-Object {
        "$(Get-P1Prop $_ 'event')" -eq "durable-preimage-restore" -and
        "$(Get-P1Prop $_ 'phase')" -eq "teardown"
      })
    if ($teardown.Count -eq 0) { throw "no teardown durable-preimage-restore log row" }
    $teardownRow = $teardown[$teardown.Count - 1]
    if ("$(Get-P1Prop $teardownRow 'outcome')" -ne "restored") {
      throw "teardown restore outcome was not 'restored': $($teardownRow | ConvertTo-Json -Compress)"
    }
    if ([int](Get-P1Prop $teardownRow "count") -lt 2) {
      throw "teardown restored $([int](Get-P1Prop $teardownRow 'count')) rows, expected the topmost float plus fullscreen chrome"
    }
    $evidence.stages.post_stop = @{
      alpha = $postA
      beta = $postB
      above_rows = @($aboveAfter.rows).Count
      fullscreen_rows = @($fsAfter.rows).Count
      marker_present = (Test-P1Prop $hwndB "OmniTilerFullscreen")
      teardown = $teardownRow
      beta_vis_matches_pre_fullscreen_tile = $true
    }
    Write-P1Log $runDir "graceful stop restored topmost float + fullscreen chrome without independent restore"
  } catch {
    $failure = "$($_.Exception.Message)"
    $evidence.failure = $failure
    $evidence.failure_stack = $_.ScriptStackTrace
    Write-P1Log $runDir "FAILURE: $failure"
  } finally {
    try {
      Invoke-StressCleanup $ctx "phase1-graceful"
      $evidence.cleanup = "ok"
      Write-P1Log $runDir "cleanup ok"
    } catch {
      $evidence.cleanup_error = "$($_.Exception.Message)"
      if (-not $failure) { $failure = "cleanup: $($_.Exception.Message)" }
      Write-P1Log $runDir "cleanup FAILED: $($_.Exception.Message)"
    }
    try {
      if ($null -eq $orig) {
        $evidence.restore = @{ bytes_restored = $false; note = "preimage never captured" }
      } else {
        $postSha = (Get-FileHash -LiteralPath $settingsPath -Algorithm SHA256).Hash
        $evidence.restore = @{ sha_before = "$($orig.sha)"; sha_after = $postSha; bytes_restored = ("$postSha" -eq "$($orig.sha)") }
      }
    } catch {
      if (-not $failure) { $failure = "post-restore hash failed: $($_.Exception.Message)" }
    }
    $evidence.finished_utc = (Get-Date).ToUniversalTime().ToString("o")
    $evidence | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $evidencePath -Encoding utf8
    Write-P1Log $runDir "evidence $evidencePath"
    if ($failure) { throw "FAIL phase1-graceful: $failure" }
  }
}

function Invoke-P1Startup([switch]$Crash) {
  $leg = if ($Crash) { "crash" } else { "startup" }
  $parked = @{}
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $runDir = Join-Path $script:P1TargetDir "task9\phase1-$leg-$stamp-$PID"
  New-Item -ItemType Directory -Force -Path $runDir | Out-Null
  $settingsPath = $script:P1SettingsPath
  $origPath = Join-Path $runDir "settings-orig.json"
  $spiPath = Join-Path $runDir "taskbar-spi.json"
  $evidencePath = Join-Path $runDir "evidence.json"
  $ctx = New-P1Context $script:P1FixtureCopy $settingsPath
  $evidence = [ordered]@{
    proof = "phase1-$leg-before-observe"
    started_utc = (Get-Date).ToUniversalTime().ToString("o")
    head = ((& git -C $script:P1Repo rev-parse HEAD) | Out-String).Trim()
    git_status = ((& git -C $script:P1Repo status --short) | Out-String).Trim()
    owner_sha256 = (Get-FileHash -LiteralPath $script:P1OwnerCopy -Algorithm SHA256).Hash
    fixture_sha256 = (Get-FileHash -LiteralPath $script:P1FixtureCopy -Algorithm SHA256).Hash
    safety = [ordered]@{
      fixture_only_scope = "--scope-exe tiler-live-fixture.exe"
      synthetic_input = $false
      orphan_method = "park only the two verified product-written files during exact-owner stop and ledger cleanup"
      orphan_restore_authority = "the restarted product's own startup recover_durable_preimages; no driver restore runs between reattach and restart"
      crash_method = "kill only the exact bound fixture-scoped owner, then observe watcher recovery"
      crash_restore_authority = "the product's spawned crash watcher restores under the ledger lease before this driver restarts"
    }
    preconditions = [ordered]@{
      owner_binary = $script:P1OwnerCopy
      fixture_binary = $script:P1FixtureCopy
      settings_path = $settingsPath
      preexisting_owner_checked = $true
    }
    stages = [ordered]@{}
  }
  $failure = $null
  $orig = $null
  try {
    $ledgerDir = Assert-P1CleanStart "phase1-$leg-preflight"
    $ctx.LedgerDir = $ledgerDir
    $orig = Save-LiveSettingsPreimage $settingsPath $origPath $spiPath ""
    $ctx.GroupOrig = $orig
    $ctx.SettingsConfigIntended = $true
    $evidence.stages.preflight = @{ ledger_dir = "$ledgerDir"; settings_sha_before = "$($orig.sha)" }

    $boot = Start-P1FixtureAndOwner $script:P1FixtureCopy $ctx "phase1-startup"
    $fixtureCopy = $script:P1FixtureCopy
    $logPath = $boot.launch.log
    $bind0 = $boot.launch.bind
    $fixtureState = $boot.fixtureState
    $base = Get-StressDisplayBaseline
    $wx = [int]$base.work[0]
    $wy = [int]$base.work[1]
    Spawn-P1Window $fixtureCopy $ctx "alpha" ($wx + 120) ($wy + 100) 700 460 $true
    Spawn-P1Window $fixtureCopy $ctx "beta" ($wx + 420) ($wy + 220) 660 430 $true
    $hwndA = Wait-StressHwnd "alpha" $fixtureState 30
    $hwndB = Wait-StressHwnd "beta" $fixtureState 30
    $hwndOf = @{ alpha = $hwndA; beta = $hwndB }
    $evidence.stages.fixture = @{ pid = $boot.serverPid; hwnd_alpha = $hwndA; hwnd_beta = $hwndB }

    $tokOf = @{}
    $tokA = Wait-StressAdmitted $logPath $tokOf $hwndA $fixtureState 60 "alpha"
    $tokOf[$tokA] = "alpha"
    $tokB = Wait-StressAdmitted $logPath $tokOf $hwndB $fixtureState 60 "beta"
    $tokOf[$tokB] = "beta"
    $pre = Wait-P1SettledWorld $logPath 0 { param($e) (Test-P1Tiled $e $tokA) -and (Test-P1Tiled $e $tokB) } 60 "pre-tiled"
    Assert-P1NoPending $pre.event "pre-tiled"
    $preA = Get-P1Row $pre.event $tokA
    $preSnapA = Wait-P1NativeVis $hwndA ([int](Get-P1Prop (Get-P1Prop $preA "effective_frame") "x")) ([int](Get-P1Prop (Get-P1Prop $preA "effective_frame") "y")) ([int](Get-P1Prop (Get-P1Prop $preA "effective_frame") "w")) ([int](Get-P1Prop (Get-P1Prop $preA "effective_frame") "h")) 3 20 "pre-alpha"
    $preVisA = $preSnapA.vis
    $evidence.stages.pre_tiled = @{ frame_alpha = (Get-P1Prop $preA "effective_frame"); vis_alpha = $preVisA }

    # Seed both durable rows: beta topmost float, alpha stripped fullscreen.
    $null = Invoke-StressFocus $fixtureCopy $ctx.Socket $ctx.Nonce "beta" $hwndOf $fixtureState 15
    $floatMark = Get-P1LogMark $logPath
    $floatOut = Invoke-LiveNative $script:P1OwnerCopy @("workspace", "--float")
    $floatJson = $floatOut | ConvertFrom-Json
    if ((Get-P1Prop $floatJson "dispatched") -ne $true) { throw "workspace --float not dispatched: $floatOut" }
    $null = Wait-P1Event $logPath $floatMark { param($e) ("$(Get-P1Prop $e 'event')" -eq "world-apply" -and "$(Get-P1Prop $e 'delivery')" -in @("accepted", "command-consumed") -and (Test-P1Floating $e $tokB)) } 25 "float-row"
    $rowsAbove = Wait-P1Rows $ledgerDir "above-preimages.json" 1 15 "float-preimage"
    $null = Wait-P1Cond { (Get-P1Snap $hwndB).topmost } 15 "float-topmost"
    if (-not (Get-P1Snap $hwndB).topmost) { throw "float did not raise beta WS_EX_TOPMOST" }

    $null = Invoke-StressFocus $fixtureCopy $ctx.Socket $ctx.Nonce "alpha" $hwndOf $fixtureState 15
    $fsMark = Get-P1LogMark $logPath
    $fsOut = Invoke-LiveNative $script:P1OwnerCopy @("workspace", "--fullscreen")
    $fsJson = $fsOut | ConvertFrom-Json
    if ((Get-P1Prop $fsJson "dispatched") -ne $true) { throw "workspace --fullscreen not dispatched: $fsOut" }
    $fsToggle = Wait-P1Event $logPath $fsMark {
      param($e)
      return ("$(Get-P1Prop $e 'event')" -eq "fullscreen-toggle" -and
        "$(Get-P1Prop $e 'outcome')" -eq "accepted" -and
        "$(Get-P1Prop $e 'target')" -eq "fullscreen" -and
        "$(Get-P1Prop $e 'window')" -eq "$tokA")
    } 25 "fullscreen-toggle"
    $enter = Wait-P1StateWrite $logPath $fsMark $tokA "fullscreen" $true 30 "fullscreen-row"
    if ("$(Get-P1Prop $enter.event 'command')" -ne "toggle-fullscreen") {
      throw "fullscreen state write did not ride the toggle-fullscreen apply: $($enter.event | ConvertTo-Json -Compress -Depth 6)"
    }
    if ((Get-P1Prop $enter.attempt "pre") -ne $false -or (Get-P1Prop $enter.attempt "post") -ne $true) {
      throw "fullscreen state write did not prove pre=false -> post=true: $($enter.attempt | ConvertTo-Json -Compress)"
    }
    $rowsFs = Wait-P1Rows $ledgerDir "fullscreen-preimages.json" 1 20 "fullscreen-preimage"
    $rowAbove = @($rowsAbove.rows)[0]
    $rowFs = @($rowsFs.rows)[0]
    if ([uint64](Get-P1Prop $rowAbove "hwnd") -ne $hwndB) { throw "above preimage names another window: $($rowsAbove.text)" }
    if ([uint64](Get-P1Prop $rowFs "hwnd") -ne $hwndA) { throw "fullscreen preimage names another window: $($rowsFs.text)" }
    if ((Get-P1Prop $rowAbove "priorTopmost") -ne $false) { throw "above preimage priorTopmost is not false: $($rowsAbove.text)" }
    if ($null -eq (Get-P1Prop $rowFs "frame")) { throw "fullscreen preimage carries no pre-tile restoration frame: $($rowsFs.text)" }
    $null = Wait-P1Cond {
      $s = Get-P1Snap $hwndA
      return (($s.style -band $script:P1Chrome) -eq 0 -and (Test-P1Prop $hwndA "OmniTilerFullscreen"))
    } 20 "fullscreen-native"
    if (-not (Get-P1Snap $hwndB).topmost) { throw "beta lost its seeded topmost band before the crash" }
    if (((Get-P1Snap $hwndA).style -band $script:P1Chrome) -ne 0) { throw "alpha did not strip chrome before the crash" }
    if (-not (Test-P1Prop $hwndA "OmniTilerFullscreen")) { throw "alpha lost the fullscreen marker before the crash" }
    $preCrashA = Get-P1Snap $hwndA
    $preCrashB = Get-P1Snap $hwndB
    $evidence.stages.seed = @{
      producer = $bind0
      producer_log = $logPath
      above = $rowAbove
      fullscreen = $rowFs
      fullscreen_state_write = $enter.attempt
      fullscreen_toggle = $fsToggle.event
      alpha = $preCrashA
      beta = $preCrashB
      frame_alpha = (Get-P1Prop $preA "effective_frame")
      vis_alpha = $preVisA
    }

    $null = Assert-StressOwnerUnreplaced $ctx
    $ledgerPath = Join-Path $ledgerDir "ledger.json"
    $seededLedger = Get-Content -LiteralPath $ledgerPath -Raw | ConvertFrom-Json
    if ($null -eq $seededLedger.windows -or @($seededLedger.windows).Count -ne 0) {
      throw "refusing to stage R12 recovery with committed hide claims"
    }
    $bind0Creation = "$($bind0.process_creation)"
    if ($Crash) {
      # The product spawns its crash watcher before publishing ready. Record
      # the marker's exact watcher identity, then kill only the owner and let
      # that watcher (never this driver) restore underneath the lease.
      $watcher = Wait-P1WatcherMarker $ledgerDir $bind0Creation 15 "crash-watcher"
      if ($watcher.owner_creation -ne $bind0Creation) {
        throw "crash watcher marker names another owner: $($watcher | ConvertTo-Json -Compress)"
      }
      if ($watcher.pid -eq [int]$bind0.pid) {
        throw "crash watcher marker names the owner pid: $($watcher | ConvertTo-Json -Compress)"
      }
      if ($null -eq (Get-P1LiveProcess $watcher.pid $watcher.creation $script:P1OwnerCopy)) {
        throw "crash watcher live identity mismatch (pid=$($watcher.pid) creation=$($watcher.creation))"
      }
      $markerPath = $watcher.path
      $evidence.stages.crash_watcher = @{ marker = $watcher; live = $true }
      Stop-P1Crash $script:P1OwnerCopy $bind0 "crash"
      # Do not race the watcher by launching a competing owner. Its verified
      # ledger removal, marker removal and live native restoration must all
      # precede launch; the marker is removed only after restore_under_lease
      # succeeded, so its absence plus the empty row stores prove recovery.
      $null = Wait-P1Cond {
        $a = Get-P1Snap $hwndA
        $b = Get-P1Snap $hwndB
        (-not (Test-Path -LiteralPath $ledgerPath)) -and
          (-not (Test-Path -LiteralPath $markerPath)) -and
          $a.caption -and $a.thick -and (-not $b.topmost) -and
          (-not (Test-P1Prop $hwndA "OmniTilerFullscreen")) -and
          (@(Get-P1SessionRows $ledgerDir "above-preimages.json" $bind0Creation).Count -eq 0) -and
          (@(Get-P1SessionRows $ledgerDir "fullscreen-preimages.json" $bind0Creation).Count -eq 0)
      } 25 "watcher-restored"
      $watcherGone = $false
      $watcherDeadline = (Get-Date).AddSeconds(15)
      while ((Get-Date) -lt $watcherDeadline) {
        if ($null -eq (Get-P1LiveProcess $watcher.pid $watcher.creation $script:P1OwnerCopy)) {
          $watcherGone = $true
          break
        }
        Start-Sleep -Milliseconds 100
      }
      if (-not $watcherGone) { throw "crash watcher process survived its own restore" }
      $evidence.stages.crash_recovery = @{
        restored_before_restart = $true
        watcher_marker_gone = (-not (Test-Path -LiteralPath $markerPath))
        watcher_exited = $true
        above_rows = @((Read-P1PreimageRows $ledgerDir "above-preimages.json").rows).Count
        fullscreen_rows = @((Read-P1PreimageRows $ledgerDir "fullscreen-preimages.json").rows).Count
        alpha = (Get-P1Snap $hwndA)
        beta = (Get-P1Snap $hwndB)
      }
    } else {
      # The ordinary crash watcher and synchronous dead-owner reclaim would
      # consume the rows before the new owner entered its loop. To exercise
      # startup's no-ledger recovery, preserve these exact product-written
      # bytes while closing the fixture-scoped owner and clearing its ledger.
      $copies = @{}
      foreach ($entry in @(
        @{ name = "above-preimages.json"; row = $rowAbove },
        @{ name = "fullscreen-preimages.json"; row = $rowFs }
      )) {
        if ("$(Get-P1Prop $entry.row 'session')" -ne "$($bind0.process_creation)" -or
            [int](Get-P1Prop $entry.row "pid") -ne $boot.serverPid -or
            "$(Get-P1Prop $entry.row 'creation')" -ne "$($ctx.FixtureCreation)") {
          throw "refusing to park a preimage outside the exact fixture/owner lifetime"
        }
        $slot = Join-Path $ledgerDir $entry.name
        $saved = "$slot.phase1-$($bind0.process_creation)"
        $hash = (Get-FileHash -LiteralPath $slot -Algorithm SHA256).Hash
        [IO.File]::Move($slot, $saved)
        $parked[$slot] = $saved
        $copies[$slot] = $hash
      }
      $st = Invoke-LiveNative $script:P1OwnerCopy @("stop") | ConvertFrom-Json
      if ((Get-P1Prop $st "stopped") -ne $true -or (Get-P1Prop $st "owner_exited") -ne $true) {
        throw "orphan staging did not stop the exact owner"
      }
      $null = Assert-StressOwnerUnreplaced $ctx
      $clean = Invoke-LiveNative $script:P1OwnerCopy @("restore") | ConvertFrom-Json
      if ((Get-P1Prop $clean "ledger_cleaned") -ne $true -or (Test-Path -LiteralPath $ledgerPath)) {
        throw "orphan staging did not clear the stopped owner's ledger"
      }
      $null = Assert-NoPreexistingStressOwner $script:P1OwnerCopy "phase1-startup-orphan"
      foreach ($slot in @($parked.Keys)) {
        [IO.File]::Move($parked[$slot], $slot)
        $parked.Remove($slot)
        if ((Get-FileHash -LiteralPath $slot -Algorithm SHA256).Hash -ne $copies[$slot]) {
          throw "orphan preimage bytes changed during exact-owner teardown"
        }
      }
      $orphanA = Get-P1Snap $hwndA
      $orphanB = Get-P1Snap $hwndB
      if (-not $orphanB.topmost -or ($orphanA.style -band $script:P1Chrome) -ne 0 -or
          -not (Test-P1Prop $hwndA "OmniTilerFullscreen")) {
        throw "orphan staging did not leave the original live native mutations"
      }
      $evidence.stages.orphan = @{
        ledger_absent = $true
        sha256 = $copies
        alpha = $orphanA
        beta = $orphanB
      }
    }

    # The previous owner/ledger is gone; bind only the new exact owner.
    $ctx.Owner = $null
    $ctx.OwnerLaunched = $false
    $ctx.OwnerLaunchIntended = $false
    $restart = Start-P1Owner $script:P1OwnerCopy $script:P1BinDir "phase1-startup-boot1" $ctx
    $log2 = $restart.log
    $bind1 = $restart.bind
    $evidence.stages.restart = @{ pid = [int]$bind1.pid; creation = "$($bind1.process_creation)"; log = $log2 }

    $deadline = (Get-Date).AddSeconds(45)
    $recovered = $false
    $restoreEvent = $null
    $lastAbove = $null
    $lastFs = $null
    while ((Get-Date) -lt $deadline) {
      # The restarted owner's own trace is the recovery authority; the fixture
      # rows and native bands are the independent readback. Producer-session
      # rows must be gone; a new-session row may legitimately appear later
      # when the layer re-raises the restored float.
      $restoreEvent = Get-P1RestoreEvent $log2
      $lastAbove = Read-P1PreimageRows $ledgerDir "above-preimages.json"
      $lastFs = Read-P1PreimageRows $ledgerDir "fullscreen-preimages.json"
      $producerAbove = @(Get-P1SessionRows $ledgerDir "above-preimages.json" $bind0Creation)
      $producerFs = @(Get-P1SessionRows $ledgerDir "fullscreen-preimages.json" $bind0Creation)
      $sA = Get-P1Snap $hwndA
      $sB = Get-P1Snap $hwndB
      # The startup leg must log its own restore; the crash leg must not (the
      # watcher already restored under the lease before the restart).
      $traceOk = if ($Crash) { $null -eq $restoreEvent } else { $null -ne $restoreEvent }
      $ok = ($traceOk -and
        $producerAbove.Count -eq 0 -and $producerFs.Count -eq 0 -and
        ($sA.style -band $script:P1Chrome) -eq $script:P1Chrome -and
        -not (Test-P1Prop $hwndA "OmniTilerFullscreen") -and
        $sB.alive -and $sB.visible)
      if ($ok) { $recovered = $true; break }
      Start-Sleep -Milliseconds 150
    }
    if (-not $recovered) {
      throw "restart recovery incomplete: restore=$($restoreEvent | ConvertTo-Json -Compress) above=$($lastAbove.text) fullscreen=$($lastFs.text) alpha_style=0x$('{0:x8}' -f (Get-P1Snap $hwndA).style) fullscreen_marker=$((Test-P1Prop $hwndA 'OmniTilerFullscreen'))"
    }
    $recoveredA = Get-P1Snap $hwndA
    $recoveredB = Get-P1Snap $hwndB
    if (-not $recoveredA.caption -or -not $recoveredA.thick) { throw "restart did not restore alpha chrome" }
    if (Test-P1Prop $hwndA "OmniTilerFullscreen") { throw "restart left the fullscreen marker" }
    # A restored float is re-raised by the layer once it is admitted as a
    # float, so beta's final band is not asserted here; the startup restore
    # trace plus the pre-raise band of that new row prove the restoration.

    # Join fresh admissions first: the first accepted apply carrying alpha is
    # emitted during the first loop ticks, which may be later than the
    # recovery poll. Reading the log after these joins includes that apply.
    $tokOf2 = @{}
    $tokA2 = Wait-StressAdmitted $log2 $tokOf2 $hwndA $fixtureState 60 "alpha-boot1"
    $tokOf2[$tokA2] = "alpha"
    $tokB2 = Wait-StressAdmitted $log2 $tokOf2 $hwndB $fixtureState 60 "beta-boot1"
    $tokOf2[$tokB2] = "beta"

    $events = @(Read-StressOwnerLog $log2)
    $restoreIdx = -1
    $firstObserveIdx = -1
    $firstApplyIdx = -1
    $firstAdmitIdx = -1
    $restoreEvent = $null
    for ($i = 0; $i -lt $events.Count; $i++) {
      $e = $events[$i]
      if ($restoreIdx -lt 0 -and "$(Get-P1Prop $e 'event')" -eq "durable-preimage-restore" -and
          "$(Get-P1Prop $e 'outcome')" -eq "restored" -and
          "$(Get-P1Prop $e 'phase')" -eq "") {
        $restoreIdx = $i
        $restoreEvent = $e
      }
      if ($firstObserveIdx -lt 0 -and "$(Get-P1Prop $e 'event')" -eq "observe") { $firstObserveIdx = $i }
      if ($firstApplyIdx -lt 0 -and "$(Get-P1Prop $e 'event')" -eq "world-apply") { $firstApplyIdx = $i }
      if ($firstAdmitIdx -lt 0 -and "$(Get-P1Prop $e 'event')" -eq "member-admitted") { $firstAdmitIdx = $i }
    }
    if ($firstObserveIdx -lt 0) { throw "boot log has no observe row" }
    if ($firstApplyIdx -lt 0) { throw "boot log has no world-apply row" }
    if ($Crash) {
      if ($restoreIdx -ge 0) { throw "startup unexpectedly re-restored rows already cleared by the watcher" }
    } else {
      if ($restoreIdx -lt 0) { throw "boot log has no startup durable-preimage-restore row" }
      if ($restoreIdx -ge $firstObserveIdx) { throw "startup recovery logged after the first observe (restore=$restoreIdx observe=$firstObserveIdx)" }
      if ($restoreIdx -ge $firstApplyIdx) { throw "startup recovery logged after the first world-apply (restore=$restoreIdx apply=$firstApplyIdx)" }
      if ($firstAdmitIdx -ge 0 -and $restoreIdx -ge $firstAdmitIdx) { throw "startup recovery logged after the first admission (restore=$restoreIdx admit=$firstAdmitIdx)" }
      if ([int](Get-P1Prop $restoreEvent "count") -lt 2) {
        throw "startup recovery count $([int](Get-P1Prop $restoreEvent 'count')) did not cover both durable rows"
      }
    }

    $skipHits = @()
    for ($i = 0; $i -lt $events.Count; $i++) {
      $e = $events[$i]
      if ("$(Get-P1Prop $e 'event')" -ne "world-observation-skips") { continue }
      foreach ($s in @(Get-P1Prop $e "skips")) {
        $reason = "$(Get-P1Prop $s 'reason')"
        $window = "$(Get-P1Prop $s 'window')"
        # Foreign sessions legitimately carry fullscreen/maximized skips; only
        # this run's fixture tokens may never be skipped for a stale fact.
        if ($reason -in @("fullscreen", "maximized") -and $window -in @($tokA2, $tokB2)) {
          $skipHits += @{ index = $i; window = $window; reason = $reason }
        }
      }
    }
    if ($skipHits.Count -gt 0) {
      throw "boot observation carried a stale fullscreen/maximized skip: $($skipHits | ConvertTo-Json -Compress)"
    }

    $alphaApplies = @()
    for ($i = 0; $i -lt $events.Count; $i++) {
      $e = $events[$i]
      if ("$(Get-P1Prop $e 'event')" -ne "world-apply") { continue }
      if ("$(Get-P1Prop $e 'delivery')" -notin @("accepted", "command-consumed")) { continue }
      $row = Get-P1Row $e $tokA2
      if ($null -ne $row) { $alphaApplies += @{ event = $e; index = $i; row = $row } }
    }
    if ($alphaApplies.Count -eq 0) { throw "no accepted boot world-apply carried alpha token $tokA2" }
    $firstAlpha = $alphaApplies[0]
    if ("$(Get-P1Prop $firstAlpha.row 'frame_authority')" -ne "controlled") {
      throw "first accepted alpha row was not controlled after recovery: $($firstAlpha.row | ConvertTo-Json -Compress)"
    }
    if ($null -eq (Get-P1Prop $firstAlpha.row "effective_frame")) {
      throw "first accepted alpha row carried no effective frame: $($firstAlpha.row | ConvertTo-Json -Compress)"
    }
    foreach ($hit in $alphaApplies) {
      if ("$(Get-P1Prop $hit.row 'frame_authority')" -eq "untouched") {
        throw "boot apply at index $($hit.index) still treated alpha as an untouched fullscreen overlay"
      }
    }
    $betaApplies = @()
    for ($i = 0; $i -lt $events.Count; $i++) {
      $e = $events[$i]
      if ("$(Get-P1Prop $e 'event')" -ne "world-apply") { continue }
      if ("$(Get-P1Prop $e 'delivery')" -notin @("accepted", "command-consumed")) { continue }
      $row = Get-P1Row $e $tokB2
      if ($null -ne $row) { $betaApplies += @{ event = $e; index = $i; row = $row } }
    }
    if ($betaApplies.Count -eq 0) { throw "no accepted boot world-apply carried beta token $tokB2" }
    $firstBeta = $betaApplies[0]
    # Canonical intent rows are member-scoped, so the restarted owner may tile
    # beta once before the re-adopted durable float intent rebinds; the
    # contract is the settled ordinary float. Topmost recovery itself is
    # proven by the startup restore trace plus the native bands, not by a row
    # shape that a re-raise would race.
    $floatBeta = Wait-P1SettledWorld $log2 0 {
      param($e)
      return (Test-P1Floating $e $tokB2)
    } 30 "beta-float-intent"
    $betaFloat = Get-P1Row $floatBeta.event $tokB2
    if ((Get-P1Prop $betaFloat 'floating') -ne $true) {
      throw "settled beta row is not a float: $($betaFloat | ConvertTo-Json -Compress)"
    }
    # A restored float that the layer re-raises must record the pre-raise band:
    # a true prior band would mean recovery never cleared the old topmost.
    $raiseRow = $null
    $raiseDeadline = (Get-Date).AddSeconds(10)
    while ((Get-Date) -lt $raiseDeadline -and $null -eq $raiseRow) {
      foreach ($row in @(Get-P1SessionRows $ledgerDir "above-preimages.json" "$($bind1.process_creation)")) {
        if ([uint64](Get-P1Prop $row "hwnd") -eq $hwndB) { $raiseRow = $row; break }
      }
      if ($null -eq $raiseRow) { Start-Sleep -Milliseconds 100 }
    }
    if ($null -ne $raiseRow -and (Get-P1Prop $raiseRow "priorTopmost") -ne $false) {
      throw "restarted float re-raise recorded a non-false prior topmost band: $($raiseRow | ConvertTo-Json -Compress)"
    }
    if (-not (Get-P1Snap $hwndA).ok_vis) { throw "alpha DWM frame unreadable after recovery" }

    $finalAbove = Read-P1PreimageRows $ledgerDir "above-preimages.json"
    $finalFs = Read-P1PreimageRows $ledgerDir "fullscreen-preimages.json"
    $evidence.stages.boot = @{
      log = $log2
      ordering = @{
        durable_preimage_restore = $restoreIdx
        first_observe = $firstObserveIdx
        first_member_admitted = $firstAdmitIdx
        first_world_apply = $firstApplyIdx
      }
      restore = $restoreEvent
      restore_count = [int](Get-P1Prop $restoreEvent "count")
      stale_fullscreen_skips = $skipHits
      first_alpha_apply = @{ index = $firstAlpha.index; tick = [int64](Get-P1Prop $firstAlpha.event "tick"); row = $firstAlpha.row }
      first_beta_apply = @{ index = $firstBeta.index; tick = [int64](Get-P1Prop $firstBeta.event "tick"); row = $firstBeta.row }
      beta_float_apply = @{ index = $floatBeta.index; tick = [int64](Get-P1Prop $floatBeta.event "tick"); row = $betaFloat }
      applies_with_alpha = $alphaApplies.Count
      applies_with_beta = $betaApplies.Count
      native_alpha = $recoveredA
      native_beta = $recoveredB
      final_beta = (Get-P1Snap $hwndB)
      above_rows = @($finalAbove.rows).Count
      fullscreen_rows = @($finalFs.rows).Count
      producer_above_rows = @(Get-P1SessionRows $ledgerDir "above-preimages.json" $bind0Creation).Count
      producer_fullscreen_rows = @(Get-P1SessionRows $ledgerDir "fullscreen-preimages.json" $bind0Creation).Count
      float_raise_row = $raiseRow
      marker_present = (Test-P1Prop $hwndA "OmniTilerFullscreen")
    }
    if ($Crash) {
      Write-P1Log $runDir "crash watcher restored under the lease and exited; restarted boot ran no startup restore and re-adopted the float intent"
    } else {
      Write-P1Log $runDir "startup recovery preceded the first observation; first accepted alpha row was controlled and beta re-adopted the durable float"
    }
  } catch {
    $failure = "$($_.Exception.Message)"
    $evidence.failure = $failure
    $evidence.failure_stack = $_.ScriptStackTrace
    Write-P1Log $runDir "FAILURE: $failure"
  } finally {
    if ($parked.Count -gt 0) {
      try {
        $null = Assert-StressOwnerUnreplaced $ctx
        foreach ($slot in @($parked.Keys)) {
          [IO.File]::Move($parked[$slot], $slot)
          $parked.Remove($slot)
        }
      } catch {
        $evidence.parking_cleanup_error = "$($_.Exception.Message)"
        if (-not $failure) { $failure = "preimage parking cleanup: $($_.Exception.Message)" }
      }
    }
    try {
      Invoke-StressCleanup $ctx "phase1-$leg"
      $evidence.cleanup = "ok"
      Write-P1Log $runDir "cleanup ok"
    } catch {
      $evidence.cleanup_error = "$($_.Exception.Message)"
      if (-not $failure) { $failure = "cleanup: $($_.Exception.Message)" }
      Write-P1Log $runDir "cleanup FAILED: $($_.Exception.Message)"
    }
    try {
      if ($null -eq $orig) {
        $evidence.restore = @{ bytes_restored = $false; note = "preimage never captured" }
      } else {
        $postSha = (Get-FileHash -LiteralPath $settingsPath -Algorithm SHA256).Hash
        $evidence.restore = @{ sha_before = "$($orig.sha)"; sha_after = $postSha; bytes_restored = ("$postSha" -eq "$($orig.sha)") }
      }
    } catch {
      if (-not $failure) { $failure = "post-restore hash failed: $($_.Exception.Message)" }
    }
    $evidence.finished_utc = (Get-Date).ToUniversalTime().ToString("o")
    $evidence | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $evidencePath -Encoding utf8
    Write-P1Log $runDir "evidence $evidencePath"
    if ($failure) { throw "FAIL phase1-${leg}: $failure" }
  }
}

function Invoke-P1Foreground {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $runDir = Join-Path $script:P1TargetDir "task9\phase1-foreground-$stamp-$PID"
  New-Item -ItemType Directory -Force -Path $runDir | Out-Null
  $settingsPath = $script:P1SettingsPath
  $origPath = Join-Path $runDir "settings-orig.json"
  $spiPath = Join-Path $runDir "taskbar-spi.json"
  $evidencePath = Join-Path $runDir "evidence.json"
  $ctx = New-P1Context $script:P1FixtureCopy $settingsPath
  $evidence = [ordered]@{
    proof = "phase1-already-foreground-departure-hide"
    started_utc = (Get-Date).ToUniversalTime().ToString("o")
    head = ((& git -C $script:P1Repo rev-parse HEAD) | Out-String).Trim()
    git_status = ((& git -C $script:P1Repo status --short) | Out-String).Trim()
    owner_sha256 = (Get-FileHash -LiteralPath $script:P1OwnerCopy -Algorithm SHA256).Hash
    fixture_sha256 = (Get-FileHash -LiteralPath $script:P1FixtureCopy -Algorithm SHA256).Hash
    safety = [ordered]@{
      fixture_only_scope = "--scope-exe tiler-live-fixture.exe"
      synthetic_input = $false
      activation = "fixture focus IPC only"
    }
    preconditions = [ordered]@{
      owner_binary = $script:P1OwnerCopy
      fixture_binary = $script:P1FixtureCopy
      settings_path = $settingsPath
      workspace_rule = "choose the first ordinal above occupied fixture workspaces on the resolved output"
      preexisting_owner_checked = $true
    }
    fallback_provenance = [ordered]@{
      core_construction = "Production core constructs every VisibilityTarget with fallback:false (crates/tiler-core/src/world_selection.rs and world_sticky.rs); fallback:true appears only in test code (crates/tiler-core/src/world_admit_v4_tests.rs). A live hide under this scenario is therefore the staged branch, not fallback authority."
      note = "The script also proves the focus dispatch was the already-foreground no-call and that the departure hide executed and settled."
      source = $null
    }
    stages = [ordered]@{}
  }
  $failure = $null
  $orig = $null
  try {
    $ledgerDir = Assert-P1CleanStart "phase1-foreground-preflight"
    $ctx.LedgerDir = $ledgerDir
    $orig = Save-LiveSettingsPreimage $settingsPath $origPath $spiPath ""
    $ctx.GroupOrig = $orig
    $ctx.SettingsConfigIntended = $true
    $evidence.stages.preflight = @{ ledger_dir = "$ledgerDir"; settings_sha_before = "$($orig.sha)" }
    $evidence.fallback_provenance.source = Get-P1FallbackSourceProof

    $boot = Start-P1FixtureAndOwner $script:P1FixtureCopy $ctx "phase1-foreground"
    $fixtureCopy = $script:P1FixtureCopy
    $logPath = $boot.launch.log
    $fixtureState = $boot.fixtureState
    $base = Get-StressDisplayBaseline
    $wx = [int]$base.work[0]
    $wy = [int]$base.work[1]
    Spawn-P1Window $fixtureCopy $ctx "alpha" ($wx + 120) ($wy + 100) 700 460 $true
    Spawn-P1Window $fixtureCopy $ctx "beta" ($wx + 420) ($wy + 220) 660 430 $true
    $hwndA = Wait-StressHwnd "alpha" $fixtureState 30
    $hwndB = Wait-StressHwnd "beta" $fixtureState 30
    $hwndOf = @{ alpha = $hwndA; beta = $hwndB }
    $evidence.stages.fixture = @{ pid = $boot.serverPid; hwnd_alpha = $hwndA; hwnd_beta = $hwndB }

    $tokOf = @{}
    $tokA = Wait-StressAdmitted $logPath $tokOf $hwndA $fixtureState 60 "alpha"
    $tokOf[$tokA] = "alpha"
    $tokB = Wait-StressAdmitted $logPath $tokOf $hwndB $fixtureState 60 "beta"
    $pre = Wait-P1SettledWorld $logPath 0 { param($e) (Test-P1Tiled $e $tokA) -and (Test-P1Tiled $e $tokB) } 60 "pre-tiled"
    Assert-P1NoPending $pre.event "pre-tiled"
    $preA = Get-P1Row $pre.event $tokA
    $preB = Get-P1Row $pre.event $tokB
    $preWs = "$(Get-P1Prop $preA 'ws')"
    if ($preWs -ne "$(Get-P1Prop $preB 'ws')") {
      throw "precondition: alpha and beta are not on one workspace"
    }
    $workspaceRows = @(Get-P1Prop $pre.event "desired_workspaces")
    $homeRow = @($workspaceRows | Where-Object { "$(Get-P1Prop $_ 'id')" -eq "$preWs" }) | Select-Object -First 1
    if ($null -eq $homeRow) { throw "cannot resolve fixture home workspace order from desired_workspaces" }
    $output = "$(Get-P1Prop $homeRow 'output')"
    $occupiedIds = @((Get-P1Prop $pre.event "desired_windows") | ForEach-Object { "$(Get-P1Prop $_ 'ws')" })
    $occupied = @($workspaceRows | Where-Object {
      "$(Get-P1Prop $_ 'output')" -eq $output -and $occupiedIds -contains "$(Get-P1Prop $_ 'id')"
    })
    if ($occupied.Count -eq 0) { throw "no occupied workspace inventory for the resolved output" }
    $homeOrder = [int](Get-P1Prop $homeRow "order")
    $homeCli = $homeOrder + 1
    $highest = ($occupied | ForEach-Object { [int](Get-P1Prop $_ "order") } | Measure-Object -Maximum).Maximum
    $targetIndex = [int]$highest + 2 # zero-based max -> next one-based ordinal
    if ($targetIndex -gt 9) { throw "no safe numbered target above occupied workspace $([int]$highest + 1)" }
    $evidence.stages.pre = @{
      frame_alpha = (Get-P1Prop $preA "effective_frame")
      frame_beta = (Get-P1Prop $preB "effective_frame")
      ws = $preWs
      home_order_0based = $homeOrder
      home_cli_ordinal = $homeCli
      target_cli_index = $targetIndex
      native_alpha = (Get-P1Snap $hwndA)
      native_beta = (Get-P1Snap $hwndB)
    }

    # The followed send only stages beta on the spare. When the fixture
    # shows and focuses its off-view alpha, the owner's foreground-follow
    # selection returns home with alpha already foreground.
    $null = Invoke-StressFocus $fixtureCopy $ctx.Socket $ctx.Nonce "beta" $hwndOf $fixtureState 15
    $awayMark = Get-P1LogMark $logPath
    $awayOut = Invoke-LiveNative $script:P1OwnerCopy @("workspace", "--send", "$targetIndex")
    if ((Get-P1Prop ($awayOut | ConvertFrom-Json) "dispatched") -ne $true) { throw "workspace --send $targetIndex not dispatched: $awayOut" }
    $away = Wait-P1SettledWorld $logPath $awayMark {
      param($e)
      $row = Get-P1Row $e $tokB
      if ($null -eq $row) { return $false }
      $ws = "$(Get-P1Prop $row 'ws')"
      $views = @((Get-P1Prop $e "desired_outputs") | Where-Object { "$(Get-P1Prop $_ 'output')" -eq $output })
      return ($ws -notin @("", $preWs) -and $views.Count -eq 1 -and "$(Get-P1Prop $views[0] 'logical_view')" -eq $ws)
    } 25 "beta-on-spare"
    $awayWs = "$(Get-P1Prop (Get-P1Row $away.event $tokB) 'ws')"
    $null = Wait-P1Cond {
      (Get-P1Snap $hwndB).visible -and -not (Get-P1Snap $hwndA).visible
    } 15 "departure-fixture-staged"
    $evidence.stages.away = @{ workspace = $awayWs; cli = $awayOut; native_beta = (Get-P1Snap $hwndB) }
    $selectionMark = Get-P1LogMark $logPath
    $show = Send-StressFixture $fixtureCopy $ctx.Socket $ctx.Nonce (@{
      v = 1; request_id = (New-StressReqId "show-alpha"); op = "set"; ids = @("alpha")
      patch = @{ visible = $true }
    } | ConvertTo-Json -Depth 6 -Compress)
    $showRows = @((Get-P1Prop (Get-P1Prop $show "result") "windows") | Where-Object { "$(Get-P1Prop $_ 'id')" -eq "alpha" })
    if ($showRows.Count -ne 1 -or (Get-P1Prop $showRows[0] "ok") -ne $true) { throw "fixture could not show alpha" }
    $null = Invoke-StressFocus $fixtureCopy $ctx.Socket $ctx.Nonce "alpha" $hwndOf $fixtureState 15
    $focusSnap = Get-P1Snap $hwndA
    if (-not $focusSnap.fg) { throw "fixture did not make alpha the native foreground" }
    $selected = Wait-P1Event $logPath $selectionMark {
      param($e)
      if ("$(Get-P1Prop $e 'event')" -ne "world-apply") { return $false }
      if ("$(Get-P1Prop $e 'delivery')" -notin @("accepted", "command-consumed")) { return $false }
      $views = @((Get-P1Prop $e "desired_outputs") | Where-Object { "$(Get-P1Prop $_ 'output')" -eq $output })
      if ($views.Count -ne 1 -or "$(Get-P1Prop $views[0] 'logical_view')" -ne $preWs) { return $false }
      $row = Get-P1Row $e $tokA
      if ($null -eq $row -or "$(Get-P1Prop $row 'ws')" -ne "$preWs") { return $false }
      $focus = @((Get-P1Prop $e "focus_dispatch") | Where-Object {
        "$(Get-P1Prop $_ 'target')" -eq "$tokA" -and "$(Get-P1Prop $_ 'kind')" -eq "explicit-command" -and
        "$(Get-P1Prop $_ 'hold')" -eq "already-foreground" -and
        (Get-P1Prop $_ "dispatched") -eq $false
      })
      return ($focus.Count -ge 1)
    } 30 "select-already-foreground"
    $selectedRow = Get-P1Row $selected.event $tokA
    $focusRow = @(Get-P1Prop $selected.event "focus_dispatch") | Where-Object {
      "$(Get-P1Prop $_ 'target')" -eq "$tokA" -and "$(Get-P1Prop $_ 'hold')" -eq "already-foreground"
    } | Select-Object -First 1
    if ($null -eq $focusRow) { throw "selection apply carried no already-foreground focus dispatch for alpha" }
    if ((Get-P1Prop $focusRow "dispatched") -ne $false) { throw "already-foreground focus row was not a no-call" }
    if ((Get-P1Prop $focusRow "setter") -ne "skipped") { throw "already-foreground focus row setter was $(Get-P1Prop $focusRow 'setter'), expected skipped" }
    $evidence.stages.selection = @{
      command = (Get-P1Prop $selected.event "command")
      accepted_index = $selected.index
      tick = [int64](Get-P1Prop $selected.event "tick")
      alpha_row = $selectedRow
      native_alpha_after_fixture_focus = $focusSnap
      focus_dispatch = $focusRow
      transfers = @(Get-P1Prop $selected.event "desired_transfers")
      from_ws = $awayWs
      target_ws = $preWs
    }

    $null = Wait-P1Cond { -not (Get-P1Snap $hwndB).visible } 25 "beta-departure-hidden"
    $alphaAfter = Get-P1Snap $hwndA
    $betaAfter = Get-P1Snap $hwndB
    if (-not $alphaAfter.visible) { throw "arrival alpha is not visible after selection" }
    if (-not $alphaAfter.fg) { throw "arrival alpha lost the foreground after selection" }
    if ($betaAfter.visible) { throw "departure beta is still visible after selection" }
    if (-not $betaAfter.alive) { throw "departure beta is not alive (unexpected close)" }

    $hidePost = $null
    $hideApply = $null
    $hideDeadline = (Get-Date).AddSeconds(20)
    while ((Get-Date) -lt $hideDeadline -and $null -eq $hidePost) {
      foreach ($e in @(Get-P1EventsSince $logPath $selectionMark)) {
        if ("$(Get-P1Prop $e 'event')" -ne "world-apply") { continue }
        if ("$(Get-P1Prop $e 'delivery')" -notin @("accepted", "command-consumed")) { continue }
        foreach ($post in @(Get-P1Prop $e "visibility_posts")) {
          if ([uint64](Get-P1Prop $post "hwnd") -eq $hwndB -and
              (Get-P1Prop $post "hide") -eq $true -and
              (Get-P1Prop $post "posted") -eq $true) {
            $hidePost = $post
            $hideApply = $e
            break
          }
        }
        if ($null -ne $hidePost) { break }
      }
      if ($null -eq $hidePost) { Start-Sleep -Milliseconds 100 }
    }
    if ($null -eq $hidePost) { throw "no executed hide post for beta after selection" }
    # Staged native path, proven from the executed post row rather than the
    # configured mode: the nonactivating async hide primitive
    # (SWP_HIDEWINDOW | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER |
    # SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS = 0x4097), never an activation
    # command.
    if ("$(Get-P1Prop $hidePost 'api')" -ne "SetWindowPos") {
      throw "departure hide executed $(Get-P1Prop $hidePost 'api'), not the nonactivating SetWindowPos post: $($hidePost | ConvertTo-Json -Compress)"
    }
    if ([int64](Get-P1Prop $hidePost "flags") -ne 0x4097) {
      throw "departure hide flag word was $([int64](Get-P1Prop $hidePost 'flags')), expected 0x4097"
    }
    if ($null -ne (Get-P1Prop $hidePost "cmd")) {
      throw "departure hide carried ShowWindowAsync cmd $(Get-P1Prop $hidePost 'cmd') instead of the async SetWindowPos post"
    }
    if ([int](Get-P1Prop $hideApply "visibility_posts_omitted") -ne 0) {
      throw "visibility_posts were truncated; cannot prove the beta hide row was complete"
    }
    $stats = Get-P1Prop $hideApply "stats"
    if ([int](Get-P1Prop $stats "staged_hides") -lt 1) {
      throw "hide apply did not report staged_hides (stats=$($stats | ConvertTo-Json -Compress))"
    }
    $hiddenApply = $null
    $hiddenDeadline = (Get-Date).AddSeconds(20)
    while ((Get-Date) -lt $hiddenDeadline -and $null -eq $hiddenApply) {
      foreach ($e in @(Get-P1EventsSince $logPath $selected.index)) {
        if ("$(Get-P1Prop $e 'event')" -ne "world-apply") { continue }
        if ("$(Get-P1Prop $e 'delivery')" -notin @("accepted", "command-consumed")) { continue }
        $s = Get-P1Prop $e "stats"
        if ($null -ne (Get-P1Prop $s "departures_hidden_mono_ms")) { $hiddenApply = $e; break }
      }
      if ($null -eq $hiddenApply) { Start-Sleep -Milliseconds 100 }
    }
    if ($null -eq $hiddenApply) { throw "no settled departures_hidden stamp after selection" }

    Start-Sleep -Milliseconds 700
    $alphaStable = Get-P1Snap $hwndA
    $betaStable = Get-P1Snap $hwndB
    if ($betaStable.visible) { throw "departure beta re-appeared within 700 ms" }
    if (-not $alphaStable.visible -or -not $alphaStable.fg) { throw "arrival alpha lost visibility/foreground within 700 ms" }
    $evidence.stages.hide = @{
      hide_post = $hidePost
      native_post = @{
        api = "$(Get-P1Prop $hidePost 'api')"
        flags = [int64](Get-P1Prop $hidePost "flags")
        cmd = (Get-P1Prop $hidePost "cmd")
        posted = (Get-P1Prop $hidePost "posted")
        activation_free = $true
      }
      hide_apply = @{ tick = [int64](Get-P1Prop $hideApply "tick"); stats = $stats }
      settled_tick = [int64](Get-P1Prop $hiddenApply "tick")
      native_alpha = $alphaAfter
      native_beta = $betaAfter
      stable_alpha = $alphaStable
      stable_beta = $betaStable
      beta_alive = $betaAfter.alive
      proof_chain = @{
        focus = "already-foreground hold with dispatched=false and setter=skipped in the selection apply (core NotCalled{NoReason} + foreground_proven)"
        hide = "staged_hides departure hide executed through the nonactivating async SetWindowPos post (0x4097) with cmd null"
        source = "production core constructs fallback:false only; fallback:true exists only in tests"
      }
      staged_not_fallback = $true
    }
    Write-P1Log $runDir "already-foreground departure hid beta through the staged proof path"
  } catch {
    $failure = "$($_.Exception.Message)"
    $evidence.failure = $failure
    $evidence.failure_stack = $_.ScriptStackTrace
    Write-P1Log $runDir "FAILURE: $failure"
  } finally {
    try {
      Invoke-StressCleanup $ctx "phase1-foreground"
      $evidence.cleanup = "ok"
      Write-P1Log $runDir "cleanup ok"
    } catch {
      $evidence.cleanup_error = "$($_.Exception.Message)"
      if (-not $failure) { $failure = "cleanup: $($_.Exception.Message)" }
      Write-P1Log $runDir "cleanup FAILED: $($_.Exception.Message)"
    }
    try {
      if ($null -eq $orig) {
        $evidence.restore = @{ bytes_restored = $false; note = "preimage never captured" }
      } else {
        $postSha = (Get-FileHash -LiteralPath $settingsPath -Algorithm SHA256).Hash
        $evidence.restore = @{ sha_before = "$($orig.sha)"; sha_after = $postSha; bytes_restored = ("$postSha" -eq "$($orig.sha)") }
      }
    } catch {
      if (-not $failure) { $failure = "post-restore hash failed: $($_.Exception.Message)" }
    }
    $evidence.finished_utc = (Get-Date).ToUniversalTime().ToString("o")
    $evidence | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $evidencePath -Encoding utf8
    Write-P1Log $runDir "evidence $evidencePath"
    if ($failure) { throw "FAIL phase1-foreground: $failure" }
  }
}

# --- top-level staging and dispatch -----------------------------------------

function Initialize-P1Artifacts([string]$ProofGroup) {
  $script:P1TargetDir = Get-StressTargetDir
  if (-not (Test-Path -LiteralPath $script:P1TargetDir -PathType Container)) {
    throw "missing build target directory $($script:P1TargetDir)"
  }
  $script:P1TargetDir = (Resolve-Path -LiteralPath $script:P1TargetDir).Path
  $sourceOwner = Join-Path $script:P1TargetDir "debug\tiler-windows.exe"
  $sourceFixture = Join-Path $script:P1TargetDir "debug\tiler-live-fixture.exe"
  if (-not (Test-Path -LiteralPath $sourceOwner -PathType Leaf)) {
    throw "missing $sourceOwner; build tiler-windows and tiler-live-fixture first"
  }
  if (-not (Test-Path -LiteralPath $sourceFixture -PathType Leaf)) {
    throw "missing $sourceFixture; build tiler-windows and tiler-live-fixture first"
  }
  $script:P1SettingsPath = Join-Path $env:LOCALAPPDATA "omnitiler\settings.json"
  if (-not (Test-Path -LiteralPath $script:P1SettingsPath -PathType Leaf)) {
    throw "missing live settings preimage source $script:P1SettingsPath"
  }
  $script:P1BinDir = Join-Path $script:P1TargetDir "task9\$ProofGroup-bin-$((Get-Date -Format 'yyyyMMdd-HHmmss'))-$PID"
  New-Item -ItemType Directory -Force -Path $script:P1BinDir | Out-Null
  $script:P1OwnerCopy = Join-Path $script:P1BinDir "tiler-windows.exe"
  $script:P1FixtureCopy = Join-Path $script:P1BinDir "tiler-live-fixture.exe"
  Copy-Item -LiteralPath $sourceOwner -Destination $script:P1OwnerCopy -Force
  Copy-Item -LiteralPath $sourceFixture -Destination $script:P1FixtureCopy -Force
}

if ($DefinitionsOnly) { return }
Initialize-P1Artifacts "phase1"

$proofs = if ($Phase1Proof -eq "all") { @("graceful", "startup", "crash", "foreground") } else { @($Phase1Proof) }
$failure = $null
foreach ($name in $proofs) {
  try {
    switch ($name) {
      "graceful" { Invoke-P1Graceful }
      "startup" { Invoke-P1Startup }
      "crash" { Invoke-P1Startup -Crash }
      "foreground" { Invoke-P1Foreground }
      default { throw "unknown proof $name" }
    }
    Write-Output "PASS phase1-$name"
  } catch {
    $failure = "phase1-${name}: $($_.Exception.Message)"
    break
  }
}
if ($null -ne $failure) {
  Write-Output "FAIL $failure"
  throw "FAIL $failure"
}
Write-Output "PASS phase1: $($proofs -join ', ')"
