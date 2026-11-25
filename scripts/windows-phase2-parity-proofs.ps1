# Phase 2 Windows v4 workspace-parity live proofs (fixture-only).
#
# Bounded, independently runnable fixture proofs for the Phase 2 item 9
# workspace-parity legs:
#
#   nospace      - a 16-lifetime serial roster tiles at inner gap 8, then a
#                  live settings gap change to 64 drives the retained
#                  update-gaps D7 repair: a newest-first capacity-float
#                  prefix, unchanged victim frames, no repair focus setter.
#                  Then an explicit unfloat retry observes D5 no-space (or
#                  its tiled branch) and bounded fresh admission probes look
#                  for a fresh D3 capacity float with D4 newcomer focus.
#   perpendicular- A,B then focus A and admit C yields a nested perpendicular
#                  H[V[A,C],B] (or V[H[A,C],B]) group. Two reversed focus
#                  histories (A then C vs C then A) must select the most
#                  recently focused perpendicular child, proving recency and
#                  not geometry. Geometry is unchanged by a focus command.
#   inactive     - honest stageable subset of inactive admission: an exact
#                  send to an inactive destination never switches the view;
#                  and an admission behind a different focused float spends
#                  its automatic focus (no focus effect, no activation).
#                  A true workspace-inactive birth is NOT claimed: Windows
#                  declares no native workspace receipt and homes newcomers
#                  to the effective view, so that branch has no fixture seam.
#   first-float  - the first explicit float applies the core's centered
#                  placement (fresh post-placement effective frame), the
#                  adapter adopts that frame, and the float survives a
#                  workspace hide/reveal at the placed frame, not at its
#                  pre-float tiled frame.
#
# Safety boundary (docs/live-windows-testing.md; this script grants no live
# authority itself):
# - fixture-only scope via `--scope-exe tiler-live-fixture.exe`; no clicks,
#   SendInput, keybd_event, registry, taskbar/SPI, or broad process cleanup;
#   fixture focus is the only activation primitive.
# - The only global file effect is the live settings document: exact bytes
#   are preimaged before any write and restored by the shared abort-safe
#   `Invoke-StressCleanup` in every proof's finally. The nospace leg changes
#   only core.inner_gap/outer_gap (plus revision for the live revision) and
#   never passes `--inner-gap`/`--outer-gap` CLI flags, which would mask the
#   document.
# - The owner is launched without gap CLI flags so the settings document is
#   authoritative. Owner stop/restore is exact-identity only.
#
# The desired-window diagnostic is capped at 16 rows (MAX_APPLY_DIAG_WINDOWS),
# so the shrink roster is capped at 16 to keep every desired row visible.
# That is also enough to make a 64-gap repair evict a newest-first prefix on
# common work areas; a display where 16 gap-8 lifetimes cannot tile is
# refused rather than guessed.
#
# Usage (from the repo root; -Live is mandatory for any effect):
#   pwsh -NoProfile -File scripts/windows-phase2-parity-proofs.ps1 -Live -Proof nospace
#   pwsh -NoProfile -File scripts/windows-phase2-parity-proofs.ps1 -Live -Proof perpendicular
#   pwsh -NoProfile -File scripts/windows-phase2-parity-proofs.ps1 -Live -Proof inactive
#   pwsh -NoProfile -File scripts/windows-phase2-parity-proofs.ps1 -Live -Proof first-float
#   pwsh -NoProfile -File scripts/windows-phase2-parity-proofs.ps1 -Live -Proof all
#   pwsh -NoProfile -File scripts/windows-phase2-parity-proofs.ps1 -DefinitionsOnly
#
# Preconditions: `target/debug/tiler-windows.exe` and
# `target/debug/tiler-live-fixture.exe` are built (or CARGO_TARGET_DIR points
# at their directory), no other live owner/ledger exists, and the standing
# live-test authorization is in force. Evidence lands under
# target/task9/phase2-parity-<leg>-<stamp>-<pid>/.

[CmdletBinding()]
param(
  [Alias("Proof")]
  [ValidateSet("nospace", "perpendicular", "inactive", "first-float", "all")]
  [string]$ParityProof = "all",
  [ValidateRange(60, 600)]
  [int]$OwnerSeconds = 600,
  [ValidateRange(8, 16)]
  [int]$NoSpaceRoster = 16,
  [ValidateRange(1, 8)]
  [int]$FreshProbeCap = 4,
  [switch]$Live,
  [switch]$DefinitionsOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# `windows-world-stress.ps1` carries its own `param` block, so dot-sourcing it
# rebinds same-named caller variables (including $Live). Preserve ours first.
$script:P2OwnerSeconds = $OwnerSeconds
$script:P2Roster = $NoSpaceRoster
$script:P2ProbeCap = $FreshProbeCap
$script:P2Live = [bool]$Live
$script:P2DefinitionsOnly = [bool]$DefinitionsOnly
$script:P2Repo = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot "windows-world-stress.ps1")
$OwnerSeconds = $script:P2OwnerSeconds
$Live = $script:P2Live
$DefinitionsOnly = $script:P2DefinitionsOnly

$script:P2WsExTopmost = [uint32]0x8
$script:P2DiagWindowCap = 16

function Initialize-P2Native {
  if ("P2Native" -as [type]) { return }
  $p2cs = @'
using System;
using System.Runtime.InteropServices;
[StructLayout(LayoutKind.Sequential)] public struct P2Rect { public int l; public int t; public int r; public int b; }
public static class P2Native {
  [DllImport("user32.dll")] public static extern IntPtr GetThreadDpiAwarenessContext();
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] public static extern long GetWindowLongPtrW(IntPtr h, int i);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, ref P2Rect r);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, ref P2Rect r, int s);
  [DllImport("dwmapi.dll", EntryPoint="DwmGetWindowAttribute")] public static extern int DwmGetWindowAttributeInt(IntPtr h, int a, ref int v, int s);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr GetPropW(IntPtr h, string name);
}
'@
  Add-Type -TypeDefinition $p2cs | Out-Null
}

function Get-P2Prop($Object, [string]$Name) {
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

function Get-P2Snap([uint64]$Hwnd) {
  # Read-only native evidence under a PerMonitorV2 thread context.
  $h = [IntPtr][long]$Hwnd
  $prior = [P2Native]::GetThreadDpiAwarenessContext()
  try {
    $null = [P2Native]::SetThreadDpiAwarenessContext([IntPtr](-4))
    $outer = New-Object P2Rect
    $vis = New-Object P2Rect
    $cloak = 0
    $okOuter = [P2Native]::GetWindowRect($h, [ref]$outer)
    $okVis = ([P2Native]::DwmGetWindowAttribute($h, 9, [ref]$vis, [Runtime.InteropServices.Marshal]::SizeOf($vis)) -eq 0)
    $okCloak = ([P2Native]::DwmGetWindowAttributeInt($h, 14, [ref]$cloak, 4) -eq 0)
    $style = [uint32]([P2Native]::GetWindowLongPtrW($h, -16) -band 0xFFFFFFFFL)
    $exstyle = [uint32]([P2Native]::GetWindowLongPtrW($h, -20) -band 0xFFFFFFFFL)
    return @{
      alive = [bool][P2Native]::IsWindow($h)
      visible = [bool][P2Native]::IsWindowVisible($h)
      iconic = [bool][P2Native]::IsIconic($h)
      cloak = $(if ($okCloak) { [int]$cloak } else { -1 })
      cloak_read = $okCloak
      style = $style
      exstyle = $exstyle
      topmost = (($exstyle -band $script:P2WsExTopmost) -ne 0)
      dpi = [int][P2Native]::GetDpiForWindow($h)
      fg = ([P2Native]::GetForegroundWindow() -eq $h)
      ok_outer = $okOuter
      ok_vis = $okVis
      outer = @([int]$outer.l, [int]$outer.t, ($outer.r - $outer.l), ($outer.b - $outer.t))
      vis = @([int]$vis.l, [int]$vis.t, ($vis.r - $vis.l), ($vis.b - $vis.t))
    }
  } finally {
    $null = [P2Native]::SetThreadDpiAwarenessContext($prior)
  }
}

function Test-P2Prop([uint64]$Hwnd, [string]$Name) {
  return ([P2Native]::GetPropW([IntPtr][long]$Hwnd, $Name) -ne [IntPtr]::Zero)
}

function Get-P2Row($Event, [string]$Token) {
  $rows = @(Get-P2Prop $Event "desired_windows")
  $hits = @($rows | Where-Object { "$(Get-P2Prop $_ 'window')" -eq "$Token" })
  if ($hits.Count -eq 0) { return $null }
  return $hits[0]
}

function Get-P2Rect($Rect) {
  if ($null -eq $Rect) { return $null }
  return @(
    [int](Get-P2Prop $Rect "x"),
    [int](Get-P2Prop $Rect "y"),
    [int](Get-P2Prop $Rect "w"),
    [int](Get-P2Prop $Rect "h")
  )
}

function Test-P2RectEqual($A, $B, [int]$Tol = 3) {
  if ($null -eq $A -or $null -eq $B) { return $false }
  if (@($A).Count -ne 4 -or @($B).Count -ne 4) { return $false }
  return ([math]::Abs([int]$A[0] - [int]$B[0]) -le $Tol -and
    [math]::Abs([int]$A[1] - [int]$B[1]) -le $Tol -and
    [math]::Abs([int]$A[2] - [int]$B[2]) -le $Tol -and
    [math]::Abs([int]$A[3] - [int]$B[3]) -le $Tol)
}

function Test-P2SnapVis($Snap, $Rect, [int]$Tol = 3) {
  if ($null -eq $Snap -or $null -eq $Rect) { return $false }
  if (-not $Snap.ok_vis) { return $false }
  return (Test-P2RectEqual $Snap.vis $Rect $Tol)
}

function Test-P2Tiled($Event, [string]$Token) {
  $row = Get-P2Row $Event $Token
  if ($null -eq $row) { return $false }
  $authority = "$(Get-P2Prop $row 'frame_authority')"
  return ($authority -eq "controlled" -and
    $null -ne (Get-P2Prop $row 'effective_frame') -and
    $null -ne (Get-P2Prop $row 'allocation') -and
    (Get-P2Prop $row 'floating') -ne $true)
}

function Test-P2Floating($Event, [string]$Token) {
  $row = Get-P2Row $Event $Token
  if ($null -eq $row) { return $false }
  $authority = "$(Get-P2Prop $row 'frame_authority')"
  return ((Get-P2Prop $row 'floating') -eq $true -and
    $authority -in @("controlled", "untouched"))
}

function Test-P2CapacityFloat($Event, [string]$Token) {
  # D7 repair preserves the victim's live frame as its retained float
  # geometry, so a repaired (or re-tried) capacity row legitimately carries
  # effective_frame; only frame AUTHORITY (and the absence of a placement
  # token) proves no frame write. Fresh admission capacity floats carry no
  # effective frame at all, which this predicate also accepts.
  $row = Get-P2Row $Event $Token
  if ($null -eq $row) { return $false }
  return ((Get-P2Prop $row 'floating') -eq $true -and
    "$(Get-P2Prop $row 'float_origin')" -eq "capacity" -and
    $null -eq (Get-P2Prop $row 'allocation') -and
    "$(Get-P2Prop $row 'frame_authority')" -eq "untouched" -and
    "$(Get-P2Prop $row 'frame_op')" -eq "")
}

function Test-P2Pending($Event) {
  foreach ($field in @("desired_transfers", "desired_operations", "held_scopes")) {
    if (@(Get-P2Prop $Event $field).Count -ne 0) { return $true }
  }
  foreach ($row in @(Get-P2Prop $Event "desired_windows")) {
    if ("$(Get-P2Prop $row 'frame_op')" -ne "" -or
        "$(Get-P2Prop $row 'frame_hold')" -ne "" -or
        "$(Get-P2Prop $row 'membership_hold')" -ne "" -or
        (Get-P2Prop $row 'automatic_clear_pending') -eq $true) {
      return $true
    }
  }
  return $false
}

function Assert-P2NoPending($Event, [string]$Tag) {
  if (Test-P2Pending $Event) { throw "${Tag}: apply still carries pending work" }
  $stats = Get-P2Prop $Event "stats"
  $warps = Get-P2Prop $stats "cursor_warps"
  if ($null -ne $warps -and [int]$warps -ne 0) { throw "${Tag}: cursor warps $warps" }
}

function Read-P2Tail([string]$LogPath, [int]$Count = 8000) {
  # Concurrent-append tolerance for the live owner log, bounded to the tail:
  # a torn final line is retried; a persistent malformed line still fails.
  if (-not (Test-Path -LiteralPath $LogPath -PathType Leaf)) { return @() }
  $lastErr = $null
  for ($attempt = 0; $attempt -lt 8; $attempt++) {
    try {
      $parsed = New-Object System.Collections.ArrayList
      $lines = @(Get-Content -LiteralPath $LogPath -Tail $Count)
      foreach ($raw in $lines) {
        if ("$raw".Trim() -eq "") { continue }
        $null = $parsed.Add(($raw | ConvertFrom-Json))
      }
      return @($parsed)
    } catch {
      $lastErr = $_
      Start-Sleep -Milliseconds 120
    }
  }
  throw "malformed JSON tail ${LogPath}: $($lastErr.Exception.Message)"
}

function Get-P2LatestTick([string]$LogPath) {
  $max = [int64]0
  foreach ($e in @(Read-P2Tail $LogPath)) {
    $t = Get-P2Prop $e "tick"
    if ($null -ne $t -and [int64]$t -gt $max) { $max = [int64]$t }
  }
  return $max
}

function Wait-P2TailMatch([string]$LogPath, [scriptblock]$Pred, [int64]$FloorTick,
    [int]$Timeout, [string]$Tag) {
  $deadline = (Get-Date).AddSeconds($Timeout)
  while ((Get-Date) -lt $deadline) {
    foreach ($e in @(Read-P2Tail $LogPath)) {
      $tick = Get-P2Prop $e "tick"
      if ($null -ne $tick -and [int64]$tick -lt $FloorTick) { continue }
      if (& $Pred $e) { return $e }
    }
    Start-Sleep -Milliseconds 100
  }
  throw "${Tag}: log join timeout"
}

function Wait-P2SettledApply([string]$LogPath, [scriptblock]$Pred, [int64]$FloorTick,
    [int]$Timeout, [string]$Tag) {
  $deadline = (Get-Date).AddSeconds($Timeout)
  $last = $null
  while ((Get-Date) -lt $deadline) {
    foreach ($e in @(Read-P2Tail $LogPath)) {
      if ("$(Get-P2Prop $e 'event')" -ne "world-apply") { continue }
      if ("$(Get-P2Prop $e 'delivery')" -notin @("accepted", "command-consumed")) { continue }
      $tick = Get-P2Prop $e "tick"
      if ($null -ne $tick -and [int64]$tick -lt $FloorTick) { continue }
      if (Test-P2Pending $e) { continue }
      if (& $Pred $e) { $last = $e }
    }
    if ($null -ne $last) { return $last }
    Start-Sleep -Milliseconds 100
  }
  throw "${Tag}: settled apply join timeout"
}

function Wait-P2Cond([scriptblock]$Pred, [int]$Timeout, [string]$Tag) {
  $deadline = (Get-Date).AddSeconds($Timeout)
  while ((Get-Date) -lt $deadline) {
    if (& $Pred) { return $true }
    Start-Sleep -Milliseconds 120
  }
  throw "${Tag}: condition timeout"
}

function Wait-P2Native([uint64]$Hwnd, $Rect, [int]$Tol, [int]$Timeout, [string]$Tag) {
  $deadline = (Get-Date).AddSeconds($Timeout)
  $last = $null
  while ((Get-Date) -lt $deadline) {
    $snap = Get-P2Snap $Hwnd
    $last = $snap
    if (Test-P2SnapVis $snap $Rect $Tol) { return $snap }
    Start-Sleep -Milliseconds 100
  }
  $seen = if ($null -ne $last) { @($last.vis) -join "," } else { "unreadable" }
  throw "${Tag}: DWM visible frame never reached $(@($Rect) -join ',') (tol $Tol); last=$seen"
}

function Get-P2FreshFocus($Event) {
  $route = Get-P2Prop $Event "fresh_route"
  return "$(Get-P2Prop $route 'focused_token')"
}

function Get-P2OutputView($Event, [string]$Output) {
  foreach ($row in @(Get-P2Prop $Event "desired_outputs")) {
    if ("$(Get-P2Prop $row 'output')" -eq "$Output") {
      return "$(Get-P2Prop $row 'logical_view')"
    }
  }
  return ""
}

function Wait-P2View([string]$LogPath, [string]$Output, [string]$View, [int64]$FloorTick,
    [int]$Timeout, [string]$Tag) {
  $pred = {
    param($e)
    return ("$(Get-P2Prop $e 'event')" -eq "world-apply" -and
      "$(Get-P2Prop $e 'delivery')" -in @("accepted", "command-consumed") -and
      (Get-P2OutputView $e $Output) -eq $View)
  }.GetNewClosure()
  return (Wait-P2SettledApply $LogPath $pred $FloorTick $Timeout $Tag)
}

function Assert-P2NoFocusEffects([string]$LogPath, [int64]$FromTick, [int64]$ToTick,
    [string]$Tag) {
  foreach ($e in @(Read-P2Tail $LogPath)) {
    if ("$(Get-P2Prop $e 'event')" -ne "world-apply") { continue }
    $tick = Get-P2Prop $e "tick"
    if ($null -eq $tick) { continue }
    $t = [int64]$tick
    if ($t -lt $FromTick -or $t -gt $ToTick) { continue }
    if (@(Get-P2Prop $e "focus_effects").Count -ne 0) {
      throw "${Tag}: automatic focus effect appeared at tick $t"
    }
    if (@(Get-P2Prop $e "focus_dispatch").Count -ne 0) {
      throw "${Tag}: focus setter was dispatched at tick $t"
    }
  }
}

function Set-P2SettingsGaps([string]$Path, [int]$Inner, [int]$Outer, [switch]$BumpRevision) {
  # Bounded pre-start/live gap staging from supported schema fields only.
  # The live path bumps the revision so the core never refuses a reused
  # revision; every other field is left byte-for-byte in the document shape.
  $raw = [IO.File]::ReadAllText($Path)
  try { $doc = $raw | ConvertFrom-Json } catch { throw "settings do not parse as JSON" }
  $revision = Get-P2Prop $doc "revision"
  if ($null -eq $revision) { throw "settings carry no revision" }
  $core = Get-P2Prop $doc "core"
  if ($null -eq $core) { throw "settings carry no core section" }
  $workspace = Get-P2Prop $core "workspace"
  if ($null -eq $workspace) { throw "settings carry no core.workspace section" }
  $changed = New-Object System.Collections.ArrayList
  if ((Get-P2Prop $workspace "default_tiled") -ne $true) {
    $workspace | Add-Member -NotePropertyName "default_tiled" -NotePropertyValue $true -Force
    $null = $changed.Add("core.workspace.default_tiled=true")
  }
  if ([int](Get-P2Prop $core "inner_gap") -ne $Inner) {
    $core | Add-Member -NotePropertyName "inner_gap" -NotePropertyValue $Inner -Force
    $null = $changed.Add("core.inner_gap=$Inner")
  }
  if ([int](Get-P2Prop $core "outer_gap") -ne $Outer) {
    $core | Add-Member -NotePropertyName "outer_gap" -NotePropertyValue $Outer -Force
    $null = $changed.Add("core.outer_gap=$Outer")
  }
  $newRevision = [int64]$revision
  if ($BumpRevision) {
    $newRevision = $newRevision + 1
    $doc | Add-Member -NotePropertyName "revision" -NotePropertyValue $newRevision -Force
    $null = $changed.Add("revision=$newRevision")
  }
  if ($changed.Count -gt 0) {
    $json = $doc | ConvertTo-Json -Depth 40 -Compress
    [IO.File]::WriteAllText($Path, $json, (New-Object System.Text.UTF8Encoding($false)))
    $back = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
    if ($null -eq (Get-P2Prop $back "revision")) { throw "configured settings lost revision" }
  }
  return @{
    revision = $newRevision
    changed = @($changed)
    sha = (Get-LiveBytesSha ([IO.File]::ReadAllBytes($Path)))
  }
}

function Invoke-P2Cli([string[]]$Argv, [string]$Tag) {
  $raw = Invoke-LiveNative $script:P2OwnerCopy $Argv
  $json = $raw | ConvertFrom-Json
  if ((Get-P2Prop $json "dispatched") -ne $true) {
    throw "${Tag}: command not dispatched: $raw"
  }
  return @{ raw = $raw; json = $json }
}

function Wait-P2WorkspaceDispatch([string]$LogPath, [string]$Op, [int64]$FloorTick,
    [int]$Timeout, [string]$Tag) {
  $pred = {
    param($e)
    return ("$(Get-P2Prop $e 'event')" -eq "workspace" -and
      "$(Get-P2Prop $e 'edge')" -eq "cli" -and
      "$(Get-P2Prop $e 'op')" -eq $Op)
  }.GetNewClosure()
  $dispatch = Wait-P2TailMatch $LogPath $pred $FloorTick $Timeout "$Tag-dispatch"
  $outcome = "$(Get-P2Prop $dispatch 'outcome')"
  if ($outcome -in @("refused", "unmanaged", "suspended", "elevated-foreground", "held")) {
    throw "${Tag}: workspace dispatch outcome $outcome"
  }
  return $dispatch
}

function Get-P2LiveProcess([int]$ProcessId, [string]$Creation, [string]$ExePath) {
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

function Assert-P2CleanStart([string]$Tag) {
  $dir = Assert-NoPreexistingStressOwner $script:P2OwnerCopy $Tag
  foreach ($file in @("above-preimages.json", "fullscreen-preimages.json")) {
    $path = Join-Path $dir $file
    if (Test-Path -LiteralPath $path -PathType Leaf) {
      $rows = @((Get-Content -LiteralPath $path -Raw | ConvertFrom-Json).rows)
      if ($rows.Count -ne 0) { throw "${Tag}: existing R12 recovery rows; no fixture launch" }
    }
  }
  return $dir
}

function Write-P2Log([string]$RunDir, [string]$Message) {
  $line = "$(Get-Date -Format 'HH:mm:ss.fff') $Message"
  Add-Content -LiteralPath (Join-Path $RunDir "driver.log") -Value $line
  Write-Output $line
}

function New-P2Context([string]$FixtureExe, [string]$SettingsPath) {
  return @{
    OwnerPayload = $script:P2OwnerCopy
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

function Start-P2Owner([string]$OwnerCopy, [string]$WorkDir, [string]$Tag, $Ctx) {
  $argLine = "tile --user-start --seconds $($script:P2OwnerSeconds) --trace --scope-exe tiler-live-fixture.exe"
  $Ctx.OwnerLaunchIntended = $true
  $null = Start-ExplorerGui $OwnerCopy $argLine $WorkDir
  $Ctx.OwnerLaunched = $true
  $ready = $null
  $deadline = (Get-Date).AddSeconds(40)
  while ((Get-Date) -lt $deadline) {
    try {
      $j = Invoke-LiveNative $OwnerCopy @("ready") | ConvertFrom-Json
      if ((Get-P2Prop $j "ready") -eq $true) { $ready = $j; break }
    } catch {}
    Start-Sleep -Milliseconds 400
  }
  if ($null -eq $ready) { throw "${Tag}: owner ready timeout (launched owner stays bound for cleanup)" }
  $readyLog = "$(Get-P2Prop $ready 'log_path')"
  if ($readyLog -eq "") { throw "${Tag}: owner ready carried no log path" }
  $readyOwner = Get-P2Prop $ready "owner"
  if (-not (Test-ExeEqual "$(Get-P2Prop $readyOwner 'exe_path')" "$OwnerCopy")) {
    throw "${Tag}: owner peer mismatch"
  }
  Assert-ParentIsExplorer ([int](Get-P2Prop $readyOwner "pid"))
  $bind = @{
    pid = [int](Get-P2Prop $readyOwner "pid")
    process_creation = "$(Get-P2Prop $readyOwner 'process_creation')"
    exe_path = "$(Get-P2Prop $readyOwner 'exe_path')"
  }
  if ($null -ne (Get-P2Prop $readyOwner "user_sid")) { $bind.user_sid = "$(Get-P2Prop $readyOwner 'user_sid')" }
  if ($null -ne (Get-P2Prop $readyOwner "session_id")) { $bind.session_id = [int](Get-P2Prop $readyOwner "session_id") }
  $Ctx.Owner = $bind
  return @{ ready = $ready; bind = $bind; log = $readyLog }
}

function Start-P2FixtureAndOwner([string]$FixtureCopy, $Ctx, [string]$Tag) {
  $started = Start-StressFixture $FixtureCopy
  $Ctx.FixtureProc = $started.proc
  $ann = $started.announcement
  $Ctx.Socket = "$(Get-P2Prop $ann 'socket')"
  $Ctx.Nonce = "$(Get-P2Prop $ann 'nonce')"
  $serverPid = [int](Get-P2Prop $ann "pid")
  $serverProc = Get-Process -Id $serverPid -ErrorAction Stop
  $Ctx.FixtureCreation = "{0:x16}" -f $serverProc.StartTime.ToUniversalTime().ToFileTimeUtc()
  $launch = Start-P2Owner $script:P2OwnerCopy $script:P2BinDir $Tag $Ctx
  $fixtureState = @{ pid = $serverPid; creation = $Ctx.FixtureCreation }
  return @{ start = $started; launch = $launch; fixtureState = $fixtureState; serverPid = $serverPid }
}

function Wait-P2AdmittedFast($Run, [uint64]$Hwnd, [int]$Timeout, [string]$Tag) {
  $deadline = (Get-Date).AddSeconds($Timeout)
  while ((Get-Date) -lt $deadline) {
    foreach ($e in @(Read-P2Tail $Run.LogPath)) {
      if ("$(Get-P2Prop $e 'event')" -ne "member-admitted") { continue }
      if ([uint64](Get-P2Prop $e "hwnd") -ne $Hwnd) { continue }
      if ([int](Get-P2Prop $e "pid") -ne [int]$Run.FixtureState.pid) {
        throw "${Tag}: admitted pid mismatch"
      }
      if ("$(Get-P2Prop $e 'creation')" -ne "$($Run.FixtureState.creation)") {
        throw "${Tag}: admitted creation mismatch"
      }
      $token = "$(Get-P2Prop $e 'window')"
      if ($Run.HwndTok.ContainsKey($Hwnd) -and "$($Run.HwndTok[$Hwnd])" -ne $token) {
        throw "${Tag}: hwnd already bound to another token"
      }
      return $token
    }
    Start-Sleep -Milliseconds 150
  }
  throw "${Tag}: admission join timeout"
}

function New-P2Spec([string]$Id, [int]$X, [int]$Y, [int]$W, [int]$H) {
  return @{ id = $Id; size_px = @($W, $H); position_outer_px = @($X, $Y); decorations = $false }
}

function Spawn-P2Windows($Run, $Specs, [int]$Timeout, [string]$Tag) {
  $body = @{
    v = 1; request_id = (New-StressReqId "spawn"); op = "spawn"
    windows = @($Specs)
  } | ConvertTo-Json -Depth 8 -Compress
  $r = Send-StressFixture $Run.Ctx.FixtureExe $Run.Ctx.Socket $Run.Ctx.Nonce $body
  $entries = @(Get-P2Prop (Get-P2Prop $r "result") "windows")
  $out = @()
  foreach ($spec in @($Specs)) {
    $id = "$($spec.id)"
    $entry = @($entries | Where-Object { "$(Get-P2Prop $_ 'id')" -eq $id }) | Select-Object -First 1
    if ($null -eq $entry -or (Get-P2Prop $entry "ok") -ne $true) {
      throw "${Tag}: fixture spawn refused ${id}: $($r | ConvertTo-Json -Compress -Depth 6)"
    }
    $hwnd = Wait-StressHwnd $id $Run.FixtureState $Timeout
    $token = Wait-P2AdmittedFast $Run $hwnd $Timeout "$Tag-$id"
    if (@($Run.TokOf.Values) -contains $token) { throw "${Tag}: duplicate token for $id" }
    $Run.TokOf[$id] = $token
    $Run.HwndOf[$id] = $hwnd
    $Run.HwndTok[$hwnd] = $token
    $Run.IdOfToken[$token] = $id
    $Run.OpenFrame[$id] = @(
      [int]$spec.position_outer_px[0], [int]$spec.position_outer_px[1],
      [int]$spec.size_px[0], [int]$spec.size_px[1]
    )
    $out += @{ id = $id; token = $token; hwnd = $hwnd }
  }
  return $out
}

function Spawn-P2Window($Run, [string]$Id, [int]$X, [int]$Y, [int]$W, [int]$H,
    [int]$Timeout, [string]$Tag) {
  $spawned = @(Spawn-P2Windows $Run @((New-P2Spec $Id $X $Y $W $H)) $Timeout $Tag)
  return $spawned[0]
}

function Wait-P2TokenRow($Run, [string]$Token, [int64]$FloorTick, [int]$Timeout, [string]$Tag) {
  $pred = {
    param($e)
    return ("$(Get-P2Prop $e 'event')" -eq "world-apply" -and
      "$(Get-P2Prop $e 'delivery')" -in @("accepted", "command-consumed") -and
      $null -ne (Get-P2Row $e $Token))
  }.GetNewClosure()
  return (Wait-P2TailMatch $Run.LogPath $pred $FloorTick $Timeout $Tag)
}

function Wait-P2AllTiled($Run, [string[]]$Tokens, [int]$Timeout, [string]$Tag) {
  $pred = {
    param($e)
    if ("$(Get-P2Prop $e 'event')" -ne "world-apply") { return $false }
    if ("$(Get-P2Prop $e 'delivery')" -notin @("accepted", "command-consumed")) { return $false }
    if ([int](Get-P2Prop $e "desired_windows_omitted") -ne 0) { return $false }
    foreach ($token in $Tokens) {
      if (-not (Test-P2Tiled $e $token)) { return $false }
    }
    return $true
  }.GetNewClosure()
  return (Wait-P2SettledApply $Run.LogPath $pred 0 $Timeout $Tag)
}

function Close-P2Windows($Run, [string[]]$Ids, [int]$Timeout, [string]$Tag) {
  $body = @{
    v = 1; request_id = (New-StressReqId "close"); op = "close"; ids = @($Ids)
  } | ConvertTo-Json -Depth 6 -Compress
  $r = Send-StressFixture $Run.Ctx.FixtureExe $Run.Ctx.Socket $Run.Ctx.Nonce $body
  $entries = @(Get-P2Prop (Get-P2Prop $r "result") "windows")
  foreach ($id in $Ids) {
    $entry = @($entries | Where-Object { "$(Get-P2Prop $_ 'id')" -eq "$id" }) | Select-Object -First 1
    if ($null -eq $entry -or (Get-P2Prop $entry "ok") -ne $true) {
      throw "${Tag}: fixture close refused $id"
    }
  }
  $tokens = @($Ids | ForEach-Object { $Run.TokOf[$_] })
  $pred = {
    param($e)
    if ("$(Get-P2Prop $e 'event')" -ne "world-apply") { return $false }
    if ("$(Get-P2Prop $e 'delivery')" -notin @("accepted", "command-consumed")) { return $false }
    foreach ($token in $tokens) {
      if ($null -ne (Get-P2Row $e $token)) { return $false }
    }
    return $true
  }.GetNewClosure()
  $null = Wait-P2SettledApply $Run.LogPath $pred 0 $Timeout "$Tag-depart"
}

function Invoke-P2Focus($Run, [string]$Id, [int]$Timeout, [string]$Tag) {
  Invoke-StressFocus $Run.Ctx.FixtureExe $Run.Ctx.Socket $Run.Ctx.Nonce $Id $Run.HwndOf $Run.FixtureState $Timeout
  Start-Sleep -Milliseconds 350
  $hwnd = [uint64]$Run.HwndOf[$Id]
  $null = Wait-P2Cond { (Get-P2Snap $hwnd).fg } 10 "$Tag-fg"
}

function Get-P2RingFromApply($Apply, [string]$Output, [string]$Tag) {
  try {
    return @(Get-StressRing $Apply $Output)
  } catch {
    throw "${Tag}: workspace ring unavailable: $($_.Exception.Message)"
  }
}

function Invoke-P2Leg([string]$Name, [scriptblock]$Body) {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $runDir = Join-Path $script:P2TargetDir "task9\phase2-parity-$Name-$stamp-$PID"
  New-Item -ItemType Directory -Force -Path $runDir | Out-Null
  $settingsPath = $script:P2SettingsPath
  $origPath = Join-Path $runDir "settings-orig.json"
  $spiPath = Join-Path $runDir "taskbar-spi.json"
  $evidencePath = Join-Path $runDir "evidence.json"
  $ctx = New-P2Context $script:P2FixtureCopy $settingsPath
  $evidence = [ordered]@{
    proof = "phase2-parity-$Name"
    started_utc = (Get-Date).ToUniversalTime().ToString("o")
    head = ((& git -C $script:P2Repo rev-parse HEAD) | Out-String).Trim()
    git_status = ((& git -C $script:P2Repo status --short) | Out-String).Trim()
    owner_sha256 = (Get-FileHash -LiteralPath $script:P2OwnerCopy -Algorithm SHA256).Hash
    fixture_sha256 = (Get-FileHash -LiteralPath $script:P2FixtureCopy -Algorithm SHA256).Hash
    safety = [ordered]@{
      fixture_only_scope = "--scope-exe tiler-live-fixture.exe"
      synthetic_input = $false
      registry_taskbar_spi_writes = $false
      broad_process_cleanup = $false
      settings_effect = "core.inner_gap/outer_gap staging only; exact bytes preimaged and restored by Invoke-StressCleanup"
      fixture_focus_only_activation = $true
    }
    preconditions = [ordered]@{
      owner_binary = $script:P2OwnerCopy
      fixture_binary = $script:P2FixtureCopy
      settings_path = $settingsPath
      owner_seconds = $script:P2OwnerSeconds
      desired_window_diag_cap = $script:P2DiagWindowCap
      no_space_roster = $script:P2Roster
      fresh_probe_cap = $script:P2ProbeCap
      source_surface = $script:P2SourceSurface
    }
    settings_stages = [ordered]@{}
    stages = [ordered]@{}
  }
  $run = @{
    Name = $Name
    RunDir = $runDir
    EvidencePath = $evidencePath
    Evidence = $evidence
    Ctx = $ctx
    FixtureState = $null
    LogPath = ""
    TokOf = @{}
    HwndOf = @{}
    HwndTok = @{}
    IdOfToken = @{}
    OpenFrame = @{}
    Failure = $null
  }
  $baseInner = if ($Name -eq "nospace") { 8 } else { 0 }
  $baseOuter = 0
  $evidence.stages.preflight = [ordered]@{}
  $failure = $null
  try {
    $ledgerDir = Assert-P2CleanStart "phase2-$Name-preflight"
    $ctx.LedgerDir = $ledgerDir
    $orig = Save-LiveSettingsPreimage $settingsPath $origPath $spiPath ""
    $ctx.GroupOrig = $orig
    $ctx.SettingsConfigIntended = $true
    $stage = Set-P2SettingsGaps $settingsPath $baseInner $baseOuter
    $ctx.SettingsChanged = @($stage.changed)
    $evidence.settings_stages.base = @{
      ledger_dir = "$ledgerDir"
      sha_before = "$($orig.sha)"
      sha_after = "$($stage.sha)"
      inner_gap = $baseInner
      outer_gap = $baseOuter
      revision = $stage.revision
      changed = @($stage.changed)
    }
    $boot = Start-P2FixtureAndOwner $script:P2FixtureCopy $ctx "phase2-$Name"
    $run.FixtureState = $boot.fixtureState
    $run.LogPath = $boot.launch.log
    $evidence.stages.fixture = @{
      pid = $boot.serverPid
      creation = $boot.fixtureState.creation
      owner = $boot.launch.bind
      log_path = $run.LogPath
    }
    & $Body $run
    Write-P2Log $runDir "PASS phase2-parity-$Name"
  } catch {
    $failure = "$($_.Exception.Message)"
    $evidence.failure = $failure
    $evidence.failure_stack = $_.ScriptStackTrace
    Write-P2Log $runDir "FAILURE: $failure"
  } finally {
    try {
      Invoke-StressCleanup $ctx "phase2-parity-$Name"
      $evidence.cleanup = "ok"
      Write-P2Log $runDir "cleanup ok"
    } catch {
      $evidence.cleanup_error = "$($_.Exception.Message)"
      if ($null -eq $failure) { $failure = "cleanup: $($_.Exception.Message)" }
      Write-P2Log $runDir "cleanup FAILED: $($_.Exception.Message)"
    }
    $evidence.finished_utc = (Get-Date).ToUniversalTime().ToString("o")
    if ($null -ne $failure) { $evidence.failure = $failure }
    try {
      $evidence | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $evidencePath -Encoding utf8NoBOM
    } catch {
      Write-P2Log $runDir "evidence write failed: $($_.Exception.Message)"
    }
  }
  if ($null -ne $failure) { throw "phase2-${Name}: $failure" }
}

function Invoke-P2NoSpace {
  Invoke-P2Leg "nospace" {
    param($run)
    $base = Get-StressDisplayBaseline
    $wx = [int]$base.work[0]
    $wy = [int]$base.work[1]
    $ids = New-Object System.Collections.ArrayList
    $admissions = New-Object System.Collections.ArrayList
    for ($i = 1; $i -le $script:P2Roster; $i++) {
      $id = "w{0:d2}" -f $i
      $col = ($i - 1) % 8
      $row = [math]::Floor(($i - 1) / 8)
      $x = $wx + 40 + ($col * 64)
      $y = $wy + 40 + ($row * 64)
      $spawn = Spawn-P2Window $run $id $x $y 320 220 30 "roster-$id"
      $null = Wait-P2TokenRow $run $spawn.token 0 30 "roster-row-$id"
      $null = $ids.Add($id)
      $null = $admissions.Add(@{ id = $id; token = $spawn.token; hwnd = $spawn.hwnd })
    }
    $rosterIds = @($ids)
    $rosterTokens = @($admissions | ForEach-Object { $_.token })
    $pre = Wait-P2AllTiled $run $rosterTokens 90 "pre-gap-tiled"
    Assert-P2NoPending $pre "pre-gap-tiled"
    if ([int](Get-P2Prop $pre "desired_windows_omitted") -ne 0) {
      throw "pre-gap row truncation: roster must not exceed the desired-window diag cap"
    }
    foreach ($token in $rosterTokens) {
      if (-not (Test-P2Tiled $pre $token)) { throw "pre-gap token $token is not tiled" }
      if ("$(Get-P2Prop (Get-P2Row $pre $token) 'float_origin')" -eq "capacity") {
        throw "pre-gap capacity float appeared at inner gap 8; display cannot seat the roster"
      }
    }
    $preFrames = @{}
    foreach ($a in $admissions) {
      $snap = Wait-P2Native ([uint64]$a.hwnd) (Get-P2Rect (Get-P2Prop (Get-P2Row $pre $a.token) "effective_frame")) 3 20 "pre-frame-$($a.id)"
      $preFrames[$a.token] = @($snap.vis)
    }
    $newest = $admissions[$admissions.Count - 1]
    Invoke-P2Focus $run $newest.id 15 "pre-gap-focus"
    $focusApply = Wait-P2SettledApply $run.LogPath {
      param($e)
      return ("$(Get-P2Prop $e 'event')" -eq "world-apply" -and
        (Get-P2FreshFocus $e) -eq $newest.token)
    }.GetNewClosure() 0 20 "pre-gap-focus-join"
    $run.Evidence.stages.pre_gap = @{
      roster = @($admissions)
      frame_count = $preFrames.Count
      newest_token = $newest.token
      focus_tick = [int64](Get-P2Prop $focusApply "tick")
      rows_omitted = [int](Get-P2Prop $pre "desired_windows_omitted")
    }
    Write-P2Log $run.RunDir "pre-gap roster tiled ($($rosterTokens.Count)) at inner gap 8"

    # --- live settings gap change -> retained update-gaps D7 repair ----------
    $settingsTick = (Get-P2LatestTick $run.LogPath) + 1
    $startupCore = Wait-P2TailMatch $run.LogPath {
      param($e)
      return ("$(Get-P2Prop $e 'event')" -eq "settings-core" -and
        "$(Get-P2Prop $e 'scope')" -eq "startup" -and
        "$(Get-P2Prop $e 'status')" -eq "accepted")
    } 0 10 "settings-core-startup"
    # The Windows connection sequences core snapshots independently of the
    # persisted settings document's revision. This run makes one live edit.
    $nextCoreRevision = [int64](Get-P2Prop $startupCore "revision") + 1
    $stage = Set-P2SettingsGaps $run.Ctx.SettingsPath 64 0 -BumpRevision
    $run.Evidence.settings_stages.live_gap = @{
      revision = $stage.revision
      changed = @($stage.changed)
      sha_after = $stage.sha
    }
    $appliedPred = {
      param($e)
      return ("$(Get-P2Prop $e 'event')" -eq "settings-applied" -and
        [int64](Get-P2Prop $e "revision") -eq [int64]$stage.revision)
    }.GetNewClosure()
    $settingsApplied = Wait-P2TailMatch $run.LogPath $appliedPred 0 30 "settings-applied"
    $lanes = @(Get-P2Prop $settingsApplied "lanes")
    if (-not ($lanes -contains "gaps")) {
      throw "settings apply did not adopt the gaps lane: $($lanes -join ',')"
    }
    $corePred = {
      param($e)
      return ("$(Get-P2Prop $e 'event')" -eq "settings-core" -and
        "$(Get-P2Prop $e 'scope')" -eq "live" -and
        [int64](Get-P2Prop $e "revision") -eq $nextCoreRevision)
    }.GetNewClosure()
    $coreApplied = Wait-P2TailMatch $run.LogPath $corePred 0 30 "settings-core"
    if ("$(Get-P2Prop $coreApplied 'status')" -ne "accepted") {
      throw "core settings revision refused: $($coreApplied | ConvertTo-Json -Compress -Depth 6)"
    }
    $repairPred = {
      param($e)
      if ("$(Get-P2Prop $e 'event')" -ne "world-apply") { return $false }
      if ("$(Get-P2Prop $e 'delivery')" -notin @("accepted", "command-consumed")) { return $false }
      foreach ($row in @(Get-P2Prop $e "desired_windows")) {
        if ("$(Get-P2Prop $row 'float_origin')" -eq "capacity") { return $true }
      }
      return $false
    }.GetNewClosure()
    $repair = Wait-P2TailMatch $run.LogPath $repairPred $settingsTick 40 "repair-apply"
    $repairTick = [int64](Get-P2Prop $repair "tick")
    $settledPred = {
      param($e)
      if ("$(Get-P2Prop $e 'event')" -ne "world-apply") { return $false }
      if ("$(Get-P2Prop $e 'delivery')" -notin @("accepted", "command-consumed")) { return $false }
      if ([int64](Get-P2Prop $e "tick") -lt $repairTick) { return $false }
      if ([int](Get-P2Prop $e "desired_windows_omitted") -ne 0) { return $false }
      return $true
    }.GetNewClosure()
    $settled = Wait-P2SettledApply $run.LogPath $settledPred $repairTick 40 "repair-settled"
    $settleTick = [int64](Get-P2Prop $settled "tick")
    if ([int](Get-P2Prop $settled "desired_windows_omitted") -ne 0) {
      throw "repair row truncation: desired window diag cap exceeded"
    }
    $victims = @()
    $survivors = @()
    foreach ($token in $rosterTokens) {
      $row = Get-P2Row $settled $token
      if ($null -eq $row) { throw "repair dropped roster token $token" }
      if ("$(Get-P2Prop $row 'float_origin')" -eq "capacity") {
        $victims += $token
        if (-not (Test-P2CapacityFloat $settled $token)) {
          throw "victim $token is not a clean capacity float: $($row | ConvertTo-Json -Compress)"
        }
      } else {
        $survivors += $token
        if (-not (Test-P2Tiled $settled $token)) {
          throw "survivor $token is not tiled after repair: $($row | ConvertTo-Json -Compress)"
        }
      }
    }
    if ($victims.Count -eq 0) { throw "gap 64 repair did not capacity-float any roster lifetime" }
    if ($survivors.Count -eq 0) { throw "gap 64 repair floated the entire roster; no survivor proof" }
    $expectedSuffix = @($rosterTokens | Select-Object -Skip ($rosterTokens.Count - $victims.Count))
    $victimSet = @($victims | Sort-Object)
    $expectedSet = @($expectedSuffix | Sort-Object)
    if (($victimSet -join ",") -ne ($expectedSet -join ",")) {
      throw "repair victims are not the newest-first admission suffix: victims=$($victims -join ',') expected=$($expectedSuffix -join ',')"
    }
    if ($victims -notcontains $newest.token) {
      throw "newest admission was not among the repair victims"
    }
    Assert-P2NoFocusEffects $run.LogPath $repairTick $settleTick "repair-focus"
    $newestHwnd = [uint64]$run.HwndOf[$newest.id]
    $null = Wait-P2Cond { (Get-P2Snap $newestHwnd).fg } 10 "repair-fg"
    foreach ($token in $victims) {
      $id = $run.IdOfToken[$token]
      $snap = Get-P2Snap ([uint64]$run.HwndOf[$id])
      if (-not $snap.alive -or -not $snap.visible -or $snap.iconic) {
        throw "victim $token is not visibly floating after repair"
      }
      if (-not (Test-P2RectEqual $snap.vis $preFrames[$token] 3)) {
        throw "victim $token native frame changed by repair: $($snap.vis -join ',') != $($preFrames[$token] -join ',')"
      }
    }
    foreach ($token in $survivors) {
      $row = Get-P2Row $settled $token
      $target = Get-P2Rect (Get-P2Prop $row "effective_frame")
      if ($null -eq $target) { $target = Get-P2Rect (Get-P2Prop $row "allocation") }
      $null = Wait-P2Native ([uint64]$run.HwndOf[$run.IdOfToken[$token]]) $target 3 20 "survivor-frame-$token"
    }
    $run.Evidence.stages.shrink_repair = @{
      settings_tick = $settingsTick
      repair_tick = $repairTick
      settle_tick = $settleTick
      settings_applied = $settingsApplied
      settings_core = $coreApplied
      victim_count = $victims.Count
      survivor_count = $survivors.Count
      victims = @($victims)
      survivors = @($survivors)
      newest_token = $newest.token
      victim_frames_unchanged = $true
      survivors_native_match = $true
      no_focus_effect = $true
      repair_rows = @($victims + $survivors | ForEach-Object { Get-P2Row $settled $_ })
    }
    Write-P2Log $run.RunDir "gap-64 repair floated newest-first prefix $($victims.Count)/$($rosterTokens.Count) with native frames unchanged"

    # --- explicit unfloat retry (D5 no-space branch or tiled branch) ---------
    Invoke-P2Focus $run $newest.id 15 "retry-focus"
    $retryFloor = Get-P2LatestTick $run.LogPath
    $null = Invoke-P2Cli @("workspace", "--float") "unfloat-retry"
    $retryPred = {
      param($e)
      return ("$(Get-P2Prop $e 'event')" -eq "float-toggle" -and
        "$(Get-P2Prop $e 'edge')" -eq "cli" -and
        "$(Get-P2Prop $e 'window')" -eq $newest.token -and
        "$(Get-P2Prop $e 'outcome')" -ne "dispatched")
    }.GetNewClosure()
    $retry = Wait-P2TailMatch $run.LogPath $retryPred $retryFloor 30 "unfloat-retry-outcome"
    $retryOutcome = "$(Get-P2Prop $retry 'outcome')"
    $retryStage = [ordered]@{
      outcome = $retryOutcome
      target = "$(Get-P2Prop $retry 'target')"
      tick = [int64](Get-P2Prop $retry "tick")
      no_space = ($retryOutcome -eq "unfloat-no-space")
    }
    if ($retryOutcome -eq "unfloat-no-space") {
      $retryApply = Wait-P2TokenedApply $run $newest.token $retryFloor 20 "retry-apply"
      if (-not (Test-P2CapacityFloat $retryApply $newest.token)) {
        throw "unfloat-no-space did not keep a clean capacity float for $($newest.token)"
      }
      $after = Get-P2Snap $newestHwnd
      if (-not (Test-P2RectEqual $after.vis $preFrames[$newest.token] 3)) {
        throw "unfloat-no-space changed the victim frame"
      }
      Assert-P2NoFocusEffects $run.LogPath $retryFloor ([int64](Get-P2Prop $retryApply "tick")) "retry-focus"
      $retryStage.apply = @(Get-P2Row $retryApply $newest.token)
    } elseif ($retryOutcome -eq "unfloat-applied") {
      $retryApply = Wait-P2SettledApply $run.LogPath {
        param($e)
        return ("$(Get-P2Prop $e 'event')" -eq "world-apply" -and
          (Test-P2Tiled $e $newest.token))
      }.GetNewClosure() $retryFloor 20 "retry-tiled"
      $retryStage.tiled = $true
      $retryStage.apply = @(Get-P2Row $retryApply $newest.token)
    } elseif ($retryOutcome -eq "rejected" -or $retryOutcome -eq "float-unverified") {
      throw "unfloat retry did not settle honestly: $retryOutcome"
    } else {
      throw "unexpected unfloat retry outcome: $retryOutcome"
    }
    $run.Evidence.stages.unfloat_retry = $retryStage
    Write-P2Log $run.RunDir "unfloat retry outcome $retryOutcome"

    # --- bounded fresh admission probe (D3/D4) -------------------------------
    # Close enough oldest victims to keep total lifetimes within the 16-row
    # diagnostic cap, then probe serially. Closing float exceptions never
    # changes the retained tiled tree, so saturation is preserved.
    $probeCount = [Math]::Min($script:P2ProbeCap, $victims.Count)
    $closeIds = @()
    if ($probeCount -gt 0) {
      $victimOrdered = @($rosterTokens | Where-Object { $victims -contains $_ })
      for ($i = 0; $i -lt $probeCount; $i++) {
        $closeIds += $run.IdOfToken[$victimOrdered[$i]]
      }
    }
    $probeStage = [ordered]@{
      closed_victims = @($closeIds)
      attempts = @()
      capacity = $null
    }
    if ($closeIds.Count -gt 0) {
      Close-P2Windows $run $closeIds 30 "probe-close"
    }
    # A focused float suppresses automatic newcomer focus (D01 guard), which
    # would make the D4 join ambiguous; anchor focus on a tiled survivor.
    $anchor = $survivors[0]
    $anchorId = $run.IdOfToken[$anchor]
    Invoke-P2Focus $run $anchorId 15 "probe-anchor"
    $probeFloor = Get-P2LatestTick $run.LogPath
    for ($p = 1; $p -le $probeCount; $p++) {
      $probeId = "p{0:d2}" -f $p
      $col = ($p - 1) % 4
      $x = $wx + 60 + ($col * 70)
      $y = $wy + 80
      $attemptFloor = $probeFloor
      $spawn = Spawn-P2Window $run $probeId $x $y 280 200 30 "probe-$probeId"
      $apply = Wait-P2TokenedSettledApply $run $spawn.token $probeFloor 30 "probe-apply-$probeId"
      $row = Get-P2Row $apply $spawn.token
      $isCapacity = ("$(Get-P2Prop $row 'float_origin')" -eq "capacity")
      $attempt = [ordered]@{
        id = $probeId
        token = $spawn.token
        tick = [int64](Get-P2Prop $apply "tick")
        floating = (Get-P2Prop $row "floating")
        float_origin = "$(Get-P2Prop $row 'float_origin')"
        capacity = $isCapacity
        row = $row
      }
      if ($isCapacity) {
        if (-not (Test-P2CapacityFloat $apply $spawn.token)) {
          throw "fresh probe $probeId capacity row is not clean"
        }
        $snap = Get-P2Snap ([uint64]$spawn.hwnd)
        if (-not (Test-P2RectEqual $snap.vis $run.OpenFrame[$probeId] 8)) {
          throw "fresh capacity float $probeId did not keep its opening frame: $($snap.vis -join ',') vs $($run.OpenFrame[$probeId] -join ',')"
        }
        # D4 mints a one-shot automatic-admission FOCUS effect for the
        # eligible newcomer. Join that effect (fresh_route.focused_token is
        # the pre-admission focus, not the newcomer), then require native
        # foreground.
        $focusPred = {
          param($e)
          if ("$(Get-P2Prop $e 'event')" -ne "world-apply") { return $false }
          foreach ($fx in @(Get-P2Prop $e "focus_effects")) {
            if ("$(Get-P2Prop $fx 'kind')" -eq "automatic-admission" -and
                "$(Get-P2Prop $fx 'target')" -eq $spawn.token) { return $true }
          }
          return $false
        }.GetNewClosure()
        $focusApply = Wait-P2TailMatch $run.LogPath $focusPred $attemptFloor 20 "probe-focus-$probeId"
        $null = Wait-P2Cond { (Get-P2Snap ([uint64]$spawn.hwnd)).fg } 10 "probe-fg-$probeId"
        $attempt.focus = [ordered]@{
          cause = "automatic-admission"
          focused_token = $spawn.token
          focus_tick = [int64](Get-P2Prop $focusApply "tick")
          native_fg = $true
        }
        $probeStage.capacity = $attempt
        $probeStage.attempts += @($attempt)
        $run.Evidence.stages.fresh_probe = $probeStage
        Write-P2Log $run.RunDir "fresh admission probe $probeId capacity-floated and took newcomer focus"
        break
      }
      $probeStage.attempts += @($attempt)
      $probeFloor = [int64](Get-P2Prop $apply "tick")
    }
    $run.Evidence.stages.fresh_probe = $probeStage
    if ($null -eq $probeStage.capacity) {
      Write-P2Log $run.RunDir "fresh admission probe: no capacity float within the bounded cap; D3 not claimed on this run"
    }
  }
}

function Wait-P2TokenedApply($Run, [string]$Token, [int64]$FloorTick, [int]$Timeout, [string]$Tag) {
  $pred = {
    param($e)
    return ("$(Get-P2Prop $e 'event')" -eq "world-apply" -and
      "$(Get-P2Prop $e 'delivery')" -in @("accepted", "command-consumed") -and
      $null -ne (Get-P2Row $e $Token))
  }.GetNewClosure()
  return (Wait-P2TailMatch $Run.LogPath $pred $FloorTick $Timeout $Tag)
}

function Wait-P2TokenedSettledApply($Run, [string]$Token, [int64]$FloorTick, [int]$Timeout, [string]$Tag) {
  $pred = {
    param($e)
    return ("$(Get-P2Prop $e 'event')" -eq "world-apply" -and
      "$(Get-P2Prop $e 'delivery')" -in @("accepted", "command-consumed") -and
      $null -ne (Get-P2Row $e $Token))
  }.GetNewClosure()
  return (Wait-P2SettledApply $Run.LogPath $pred $FloorTick $Timeout $Tag)
}

function Invoke-P2Perpendicular {
  Invoke-P2Leg "perpendicular" {
    param($run)
    $base = Get-StressDisplayBaseline
    $wx = [int]$base.work[0]
    $wy = [int]$base.work[1]
    $a = Spawn-P2Window $run "a" ($wx + 80) ($wy + 80) 640 460 30 "spawn-a"
    $null = Wait-P2TokenRow $run $a.token 0 30 "row-a"
    $b = Spawn-P2Window $run "b" ($wx + 400) ($wy + 200) 600 420 30 "spawn-b"
    $null = Wait-P2TokenRow $run $b.token 0 30 "row-b"
    $null = Wait-P2AllTiled $run @($a.token, $b.token) 60 "pair-tiled"
    Invoke-P2Focus $run "a" 15 "focus-a-first"
    $c = Spawn-P2Window $run "c" ($wx + 120) ($wy + 120) 500 360 30 "spawn-c"
    $null = Wait-P2TokenRow $run $c.token 0 30 "row-c"
    $shapeApply = Wait-P2AllTiled $run @($a.token, $b.token, $c.token) 60 "triple-tiled"
    Assert-P2NoPending $shapeApply "triple-tiled"
    if ([int](Get-P2Prop $shapeApply "desired_windows_omitted") -ne 0) { throw "perpendicular row truncation" }
    $ra = Get-P2Rect (Get-P2Prop (Get-P2Row $shapeApply $a.token) "allocation")
    $rb = Get-P2Rect (Get-P2Prop (Get-P2Row $shapeApply $b.token) "allocation")
    $rc = Get-P2Rect (Get-P2Prop (Get-P2Row $shapeApply $c.token) "allocation")
    $axisAC = Get-P2SplitAxis $ra $rc
    $group = Get-P2Union $ra $rc
    $axisRoot = Get-P2SplitAxis $group $rb
    if ($axisAC -eq "" -or $axisRoot -eq "") {
      throw "triple layout is not two nested splits: a=$($ra -join ',') b=$($rb -join ',') c=$($rc -join ',')"
    }
    if ($axisAC -eq $axisRoot) {
      throw "perpendicular nesting not formed: child axis $axisAC equals root axis $axisRoot"
    }
    $dir = Get-P2DirectionToward $rb $group
    if ($dir -eq "") { throw "cannot derive a direction from B to the A/C group" }
    $run.Evidence.stages.shape = @{
      a = $ra; b = $rb; c = $rc
      child_axis = $axisAC
      root_axis = $axisRoot
      direction = $dir
      claim = "perpendicular nested split: child axis differs from root axis"
    }
    $allocBefore = @{
      a = $ra; b = $rb; c = $rc
    }

    # Recency run 1: A then C, focus B, move toward the pair.
    Invoke-P2Focus $run "a" 15 "history-a1"
    Invoke-P2Focus $run "c" 15 "history-c1"
    Invoke-P2Focus $run "b" 15 "history-b1"
    $floor1 = Get-P2LatestTick $run.LogPath
    $null = Invoke-P2Cli @("directional", "--focus", "--direction", $dir) "perp-focus-1"
    $dispatch1 = Wait-P2DirectionalDispatch $run.LogPath "focus" $floor1 25 "perp-dispatch-1"
    $first = Wait-P2FocusTarget $run $dispatch1 $floor1 25 "perp-target-1"
    if ($first.token -ne $c.token) {
      throw "perpendicular recency run 1 selected $($first.token), expected newest $($c.token)"
    }
    $null = Wait-P2Native ([uint64]$c.hwnd) (Get-P2Rect (Get-P2Prop (Get-P2Row $shapeApply $c.token) "allocation")) 3 20 "perp-native-1"
    # Compare the focus command's own apply rows (a focus-only command logs
    # no later settled reflow; reusing the old shape apply would be a
    # tautology).
    Assert-P2GeometryUnchanged $shapeApply $first.apply @($a.token, $b.token, $c.token) "perp-geometry-1"

    # Recency run 2: C then A, focus B, same direction. Geometry is identical;
    # only the accepted focus history differs.
    Invoke-P2Focus $run "c" 15 "history-c2"
    Invoke-P2Focus $run "a" 15 "history-a2"
    Invoke-P2Focus $run "b" 15 "history-b2"
    $floor2 = Get-P2LatestTick $run.LogPath
    $null = Invoke-P2Cli @("directional", "--focus", "--direction", $dir) "perp-focus-2"
    $dispatch2 = Wait-P2DirectionalDispatch $run.LogPath "focus" $floor2 25 "perp-dispatch-2"
    $second = Wait-P2FocusTarget $run $dispatch2 $floor2 25 "perp-target-2"
    if ($second.token -ne $a.token) {
      throw "perpendicular recency run 2 selected $($second.token), expected newest $($a.token)"
    }
    $null = Wait-P2Native ([uint64]$a.hwnd) (Get-P2Rect (Get-P2Prop (Get-P2Row $shapeApply $a.token) "allocation")) 3 20 "perp-native-2"
    Assert-P2GeometryUnchanged $shapeApply $second.apply @($a.token, $b.token, $c.token) "perp-geometry-2"
    $run.Evidence.stages.recency = @{
      run1 = @{ history = @("a", "c"); expected = $c.token; selected = $first.token; dispatch = $dispatch1; target = $first.row }
      run2 = @{ history = @("c", "a"); expected = $a.token; selected = $second.token; dispatch = $dispatch2; target = $second.row }
      geometry_unchanged = $true
      reversed_history_flips_outcome = $true
    }
    Write-P2Log $run.RunDir "perpendicular recency selected C then A under reversed history; geometry unchanged"
  }
}

function Get-P2SplitAxis($A, $B) {
  # Axis of a sibling split: side-by-side is "h" (horizontal split), stacked
  # is "v". Empty means the rects are not a clean sibling pair.
  if ($null -eq $A -or $null -eq $B) { return "" }
  $sameY = ([int]$A[1] -eq [int]$B[1] -and [int]$A[3] -eq [int]$B[3])
  $sameX = ([int]$A[0] -eq [int]$B[0] -and [int]$A[2] -eq [int]$B[2])
  $xDisjoint = ([int]$A[0] + [int]$A[2] -le [int]$B[0] -or [int]$B[0] + [int]$B[2] -le [int]$A[0])
  $yDisjoint = ([int]$A[1] + [int]$A[3] -le [int]$B[1] -or [int]$B[1] + [int]$B[3] -le [int]$A[1])
  if ($sameY -and $xDisjoint) { return "h" }
  if ($sameX -and $yDisjoint) { return "v" }
  return ""
}

function Get-P2Union($A, $B) {
  if ($null -eq $A -or $null -eq $B) { return $null }
  $x0 = [Math]::Min([int]$A[0], [int]$B[0])
  $y0 = [Math]::Min([int]$A[1], [int]$B[1])
  $x1 = [Math]::Max([int]$A[0] + [int]$A[2], [int]$B[0] + [int]$B[2])
  $y1 = [Math]::Max([int]$A[1] + [int]$A[3], [int]$B[1] + [int]$B[3])
  return @($x0, $y0, ($x1 - $x0), ($y1 - $y0))
}

function Get-P2DirectionToward($From, $To) {
  if ($null -eq $From -or $null -eq $To) { return "" }
  $fx = [int]$From[0] + [int]$From[2] / 2.0
  $fy = [int]$From[1] + [int]$From[3] / 2.0
  $tx = [int]$To[0] + [int]$To[2] / 2.0
  $ty = [int]$To[1] + [int]$To[3] / 2.0
  $dx = $tx - $fx
  $dy = $ty - $fy
  if ([Math]::Abs($dx) -ge [Math]::Abs($dy)) {
    if ($dx -lt 0) { return "left" }
    if ($dx -gt 0) { return "right" }
    return ""
  }
  if ($dy -lt 0) { return "up" }
  if ($dy -gt 0) { return "down" }
  return ""
}

function Wait-P2DirectionalDispatch([string]$LogPath, [string]$Op, [int64]$FloorTick,
    [int]$Timeout, [string]$Tag) {
  $pred = {
    param($e)
    return ("$(Get-P2Prop $e 'event')" -eq "directional" -and
      "$(Get-P2Prop $e 'edge')" -eq "cli" -and
      "$(Get-P2Prop $e 'op')" -eq $Op)
  }.GetNewClosure()
  $dispatch = Wait-P2TailMatch $LogPath $pred $FloorTick $Timeout "$Tag-dispatch"
  $outcome = "$(Get-P2Prop $dispatch 'outcome')"
  if ($outcome -in @("refused", "unmanaged", "suspended", "elevated-foreground")) {
    throw "${Tag}: directional dispatch outcome $outcome"
  }
  return $dispatch
}

function Wait-P2FocusTarget($Run, $Dispatch, [int64]$FloorTick, [int]$Timeout, [string]$Tag) {
  # Join the command apply's focus request/dispatch row and require the native
  # foreground to match. `fresh_route` may still carry the pre-focus subject on
  # the command row, so the request target is the authoritative join.
  $pred = {
    param($e)
    if ("$(Get-P2Prop $e 'event')" -ne "world-apply") { return $false }
    if ("$(Get-P2Prop $e 'delivery')" -notin @("accepted", "command-consumed")) { return $false }
    foreach ($fx in @(Get-P2Prop $e "focus_effects")) {
      if ("$(Get-P2Prop $fx 'kind')" -eq "explicit-command") { return $true }
    }
    foreach ($fx in @(Get-P2Prop $e "focus_dispatch")) {
      if ((Get-P2Prop $fx "dispatched") -eq $true) { return $true }
    }
    return $false
  }.GetNewClosure()
  $apply = Wait-P2TailMatch $Run.LogPath $pred $FloorTick $Timeout "$Tag-apply"
  $target = ""
  foreach ($fx in @(Get-P2Prop $apply "focus_effects")) {
    if ("$(Get-P2Prop $fx 'kind')" -eq "explicit-command") { $target = "$(Get-P2Prop $fx 'target')" }
  }
  if ($target -eq "") {
    foreach ($fx in @(Get-P2Prop $apply "focus_dispatch")) {
      if ((Get-P2Prop $fx "dispatched") -eq $true) { $target = "$(Get-P2Prop $fx 'target')" }
    }
  }
  if ($target -eq "") { throw "${Tag}: no explicit focus target in apply" }
  $id = $Run.IdOfToken[$target]
  if ($null -eq $id) { throw "${Tag}: focus target $target has no fixture id" }
  $null = Wait-P2Cond { (Get-P2Snap ([uint64]$Run.HwndOf[$id])).fg } 15 "$Tag-native"
  $row = Get-P2Row $apply $target
  return @{ token = $target; id = $id; apply = $apply; row = $row }
}

function Assert-P2GeometryUnchanged($Before, $After, [string[]]$Tokens, [string]$Tag) {
  foreach ($token in $Tokens) {
    $ra = Get-P2Rect (Get-P2Prop (Get-P2Row $Before $token) "allocation")
    $rb = Get-P2Rect (Get-P2Prop (Get-P2Row $After $token) "allocation")
    if (-not (Test-P2RectEqual $ra $rb 0)) {
      throw "${Tag}: focus command changed $token geometry $($ra -join ',') -> $($rb -join ',')"
    }
  }
}

function Invoke-P2Inactive {
  Invoke-P2Leg "inactive" {
    param($run)
    $base = Get-StressDisplayBaseline
    $wx = [int]$base.work[0]
    $wy = [int]$base.work[1]
    $a = Spawn-P2Window $run "a" ($wx + 80) ($wy + 80) 620 440 30 "spawn-a"
    $null = Wait-P2TokenRow $run $a.token 0 30 "row-a"
    $b = Spawn-P2Window $run "b" ($wx + 380) ($wy + 180) 580 400 30 "spawn-b"
    $null = Wait-P2TokenRow $run $b.token 0 30 "row-b"
    $pair = Wait-P2AllTiled $run @($a.token, $b.token) 60 "pair-tiled"
    $rowA = Get-P2Row $pair $a.token
    $out = "$(Get-P2Prop $rowA 'out')"
    $homeWorkspace = "$(Get-P2Prop $rowA 'ws')"
    $ring = Get-P2RingFromApply $pair $out "inactive-ring"
    if ($ring.Count -lt 2) { throw "workspace ring has fewer than two entries" }
    $homeIndex = [array]::IndexOf($ring, $homeWorkspace)
    if ($homeIndex -lt 0) { throw "home workspace $homeWorkspace not in ring" }
    $away = $ring[($homeIndex + 1) % $ring.Count]
    if ($away -eq $homeWorkspace) { throw "away workspace resolved to home" }
    $awayOrdinal = $homeIndex + 2
    if ($awayOrdinal -gt $ring.Count) { $awayOrdinal = 1 }
    $homeOrdinal = $homeIndex + 1
    $run.Evidence.stages.ring = @{ output = $out; home = $homeWorkspace; away = $away; ring = @($ring) }

    # --- inactive-destination exact send: no view switch --------------------
    Invoke-P2Focus $run "a" 15 "send-focus-a"
    $sendFloor = Get-P2LatestTick $run.LogPath
    # --stay (not --send): move A without following. --send would select the
    # destination and defeat the no-view-switch claim outright.
    $null = Invoke-P2Cli @("workspace", "--stay", "$awayOrdinal") "send-away"
    $sendDispatch = Wait-P2WorkspaceDispatch $run.LogPath "stay" $sendFloor 30 "send-dispatch"
    $sendApply = Wait-P2TailMatch $run.LogPath {
      param($e)
      if ("$(Get-P2Prop $e 'event')" -ne "world-apply" -or
          "$(Get-P2Prop $e 'delivery')" -notin @("accepted", "command-consumed")) { return $false }
      foreach ($tr in @(Get-P2Prop $e "desired_transfers")) {
        if ("$(Get-P2Prop $tr 'kind')" -eq "window" -and
            "$(Get-P2Prop $tr 'window')" -eq $a.token -and
            "$(Get-P2Prop $tr 'target_workspace')" -eq $away) { return $true }
      }
      return $false
    }.GetNewClosure() ([int64](Get-P2Prop $sendDispatch "tick")) 30 "send-apply"
    $viewAfterSend = Get-P2OutputView $sendApply $out
    if ($viewAfterSend -ne $homeWorkspace) {
      throw "inactive-destination send switched the view: $viewAfterSend != $homeWorkspace"
    }
    $sendTransfer = $null
    foreach ($tr in @(Get-P2Prop $sendApply "desired_transfers")) {
      if ("$(Get-P2Prop $tr 'window')" -eq $a.token) { $sendTransfer = $tr }
    }
    if ((Get-P2Prop $sendTransfer "follow") -eq $true) {
      throw "inactive-destination send carried follow=true"
    }
    $null = Wait-P2Cond { -not (Get-P2Snap ([uint64]$a.hwnd)).visible } 15 "send-a-hidden"
    $sendSettled = Wait-P2SettledApply $run.LogPath {
      param($e)
      return ("$(Get-P2Prop (Get-P2Row $e $a.token) 'ws')" -eq $away -and
        (Get-P2OutputView $e $out) -eq $homeWorkspace)
    }.GetNewClosure() ([int64](Get-P2Prop $sendApply "tick")) 30 "send-settled"
    $run.Evidence.stages.inactive_send = @{
      dispatch = $sendDispatch
      apply_tick = [int64](Get-P2Prop $sendApply "tick")
      settled_tick = [int64](Get-P2Prop $sendSettled "tick")
      transfer = $sendTransfer
      view_after = $viewAfterSend
      view_unchanged = $true
      native_a_hidden = $true
    }
    Write-P2Log $run.RunDir "inactive-destination send stayed on home and hid A without a view switch"

    # --- admission behind a different focused float: automatic focus spent --
    Invoke-P2Focus $run "b" 15 "float-focus-b"
    $floatFloor = Get-P2LatestTick $run.LogPath
    $null = Invoke-P2Cli @("workspace", "--float") "float-b"
    $floatApply = Wait-P2SettledApply $run.LogPath {
      param($e)
      return ("$(Get-P2Prop $e 'event')" -eq "world-apply" -and
        (Test-P2Floating $e $b.token))
    }.GetNewClosure() $floatFloor 30 "float-b-apply"
    $bFloatRow = Get-P2Row $floatApply $b.token
    if ("$(Get-P2Prop $bFloatRow 'float_origin')" -eq "capacity") {
      throw "explicit ordinary float B carried a capacity origin"
    }
    $null = Wait-P2Cond { (Get-P2Snap ([uint64]$b.hwnd)).fg } 15 "float-b-fg"
    $birthFloor = Get-P2LatestTick $run.LogPath
    $c = Spawn-P2Window $run "c" ($wx + 120) ($wy + 120) 500 360 30 "spawn-c-inactive"
    $preBirth = Get-P2Snap ([uint64]$b.hwnd)
    if (-not $preBirth.fg) { throw "c spawn stole foreground before the birth report; staging invalid" }
    $birthApply = Wait-P2SettledApply $run.LogPath {
      param($e)
      return ("$(Get-P2Prop $e 'event')" -eq "world-apply" -and
        (Test-P2Tiled $e $c.token))
    }.GetNewClosure() $birthFloor 30 "birth-c-apply"
    $birthTick = [int64](Get-P2Prop $birthApply "tick")
    $settleFloor = $birthTick
    $birthSettled = Wait-P2SettledApply $run.LogPath {
      param($e)
      return ("$(Get-P2Prop $e 'event')" -eq "world-apply" -and
        (Test-P2Tiled $e $c.token) -and
        (Test-P2Floating $e $b.token))
    }.GetNewClosure() $birthTick 20 "birth-c-settled"
    Assert-P2NoFocusEffects $run.LogPath $birthFloor ([int64](Get-P2Prop $birthSettled "tick")) "inactive-birth-focus"
    $null = Wait-P2Cond { (Get-P2Snap ([uint64]$b.hwnd)).fg } 10 "birth-b-fg"
    $cVis = Wait-P2Native ([uint64]$c.hwnd) (Get-P2Rect (Get-P2Prop (Get-P2Row $birthSettled $c.token) "effective_frame")) 3 20 "birth-c-native"
    if (-not $cVis.visible -or $cVis.iconic) { throw "admitted C is not visible" }
    if ($viewAfterSend -ne (Get-P2OutputView $birthSettled $out)) {
      throw "birth apply switched the view"
    }
    $run.Evidence.stages.focused_float_birth = @{
      b_float_row = $bFloatRow
      c_row = @(Get-P2Row $birthSettled $c.token)
      birth_tick = $birthTick
      Native_foreground_b = $true
      automatic_focus_spent = $true
      focus_effects = @()
      view = (Get-P2OutputView $birthSettled $out)
      branch = "focused-float-ownership"
    }
    $run.Evidence.stages.inactive_limitation = @{
      native_workspace_inactive_birth_staged = $false
      reason = "Windows declares NativeWorkspaceFact::Omitted and homes fresh newcomers to the effective view, so the fixture cannot place a birth on an inactive logical workspace without a native rule API (the KDE proof used a KWin rule)."
      exercised = @(
        "inactive-destination exact send: no view switch, no follow",
        "admission behind a different focused float: automatic focus spent, no focus effect or activation"
      )
      unexercised = "REQ-INS-07b workspace-inactive newcomer focus suppression"
    }
    Write-P2Log $run.RunDir "inactive stageable branches passed; workspace-inactive birth limitation recorded honestly"
  }
}

function Invoke-P2FirstFloat {
  Invoke-P2Leg "first-float" {
    param($run)
    $base = Get-StressDisplayBaseline
    $wx = [int]$base.work[0]
    $wy = [int]$base.work[1]
    $a = Spawn-P2Window $run "a" ($wx + 80) ($wy + 80) 640 460 30 "spawn-a"
    $null = Wait-P2TokenRow $run $a.token 0 30 "row-a"
    $b = Spawn-P2Window $run "b" ($wx + 380) ($wy + 180) 600 420 30 "spawn-b"
    $null = Wait-P2TokenRow $run $b.token 0 30 "row-b"
    $tiled = Wait-P2AllTiled $run @($a.token, $b.token) 60 "pair-tiled"
    $rowA = Get-P2Row $tiled $a.token
    $out = "$(Get-P2Prop $rowA 'out')"
    $homeWorkspace = "$(Get-P2Prop $rowA 'ws')"
    $ring = Get-P2RingFromApply $tiled $out "first-float-ring"
    $homeIndex = [array]::IndexOf($ring, $homeWorkspace)
    if ($homeIndex -lt 0) { throw "home workspace $homeWorkspace not in ring" }
    $away = $ring[($homeIndex + 1) % $ring.Count]
    $awayOrdinal = $homeIndex + 2
    if ($awayOrdinal -gt $ring.Count) { $awayOrdinal = 1 }
    $homeOrdinal = $homeIndex + 1
    Invoke-P2Focus $run "a" 15 "float-focus-a"
    $f0 = Wait-P2Native ([uint64]$a.hwnd) (Get-P2Rect (Get-P2Prop $rowA "effective_frame")) 3 20 "pre-float-a"
    $f0Vis = @($f0.vis)
    $floor = Get-P2LatestTick $run.LogPath
    $null = Invoke-P2Cli @("workspace", "--float") "first-float"
    $appliedPred = {
      param($e)
      if ("$(Get-P2Prop $e 'event')" -ne "world-apply") { return $false }
      if ("$(Get-P2Prop $e 'delivery')" -notin @("accepted", "command-consumed")) { return $false }
      $row = Get-P2Row $e $a.token
      # An ordinary float carries Untouched frame AUTHORITY even while its
      # one-shot placement token (frame_op) authorizes the write. Join the
      # token + concrete effective frame, never a Controlled authority.
      return ($null -ne $row -and
        (Get-P2Prop $row 'floating') -eq $true -and
        "$(Get-P2Prop $row 'frame_authority')" -eq "untouched" -and
        "$(Get-P2Prop $row 'frame_op')" -ne "" -and
        $null -ne (Get-P2Prop $row 'effective_frame'))
    }.GetNewClosure()
    $applied = Wait-P2TailMatch $run.LogPath $appliedPred $floor 30 "float-applied"
    $appliedRow = Get-P2Row $applied $a.token
    if ("$(Get-P2Prop $appliedRow 'float_origin')" -eq "capacity") {
      throw "first explicit float carried a capacity origin"
    }
    $f1 = Get-P2Rect (Get-P2Prop $appliedRow "effective_frame")
    if ($f1 -eq $null) { throw "first float apply carried no effective frame" }
    if (Test-P2RectEqual $f0Vis $f1 0) {
      throw "first float effective frame equals the pre-float tiled frame; no placement occurred"
    }
    $appliedTick = [int64](Get-P2Prop $applied "tick")
    $native1 = Wait-P2Native ([uint64]$a.hwnd) $f1 3 20 "float-native"
    if (-not $native1.visible -or $native1.iconic) { throw "first float is not visible" }
    $settled = Wait-P2SettledApply $run.LogPath {
      param($e)
      return ("$(Get-P2Prop $e 'event')" -eq "world-apply" -and
        (Test-P2Floating $e $a.token) -and
        "$(Get-P2Prop (Get-P2Row $e $a.token) 'frame_authority')" -eq "untouched")
    }.GetNewClosure() $appliedTick 25 "float-settled"
    $settledRow = Get-P2Row $settled $a.token
    $null = Wait-P2Native ([uint64]$a.hwnd) $f1 3 10 "float-settled-native"

    # Adoption must be after the applied placement row and only once for A.
    $adopt = $null
    $adoptDeadline = (Get-Date).AddSeconds(15)
    while ((Get-Date) -lt $adoptDeadline -and $null -eq $adopt) {
      $events = @(Read-P2Tail $run.LogPath)
      $applyIdx = -1
      $adoptIdx = -1
      for ($i = 0; $i -lt $events.Count; $i++) {
        $e = $events[$i]
        if ("$(Get-P2Prop $e 'event')" -eq "world-apply" -and
            [int64](Get-P2Prop $e "tick") -eq $appliedTick) { $applyIdx = $i }
      }
      if ($applyIdx -ge 0) {
        for ($i = $applyIdx + 1; $i -lt $events.Count; $i++) {
          $e = $events[$i]
          if ("$(Get-P2Prop $e 'event')" -eq "float-adopted" -and
              "$(Get-P2Prop $e 'window')" -eq $a.token) { $adoptIdx = $i; break }
        }
        for ($i = 0; $i -lt $applyIdx; $i++) {
          $e = $events[$i]
          if ("$(Get-P2Prop $e 'event')" -eq "float-adopted" -and
              "$(Get-P2Prop $e 'window')" -eq $a.token) {
            throw "float-adopted for A appeared before its first float apply"
          }
        }
      }
      if ($adoptIdx -ge 0) { $adopt = $events[$adoptIdx] }
      else { Start-Sleep -Milliseconds 150 }
    }
    if ($null -eq $adopt) { throw "no post-placement float-adopted event for A" }

    # --- hide/reveal keeps the post-placement frame -------------------------
    $hideFloor = Get-P2LatestTick $run.LogPath
    $null = Invoke-P2Cli @("workspace", "--select", "$awayOrdinal") "hide-away"
    $hideDispatch = Wait-P2WorkspaceDispatch $run.LogPath "select" $hideFloor 30 "hide-dispatch"
    $hideApply = Wait-P2View $run.LogPath $out $away ([int64](Get-P2Prop $hideDispatch "tick")) 30 "hide-view"
    $null = Wait-P2Cond { -not (Get-P2Snap ([uint64]$a.hwnd)).visible } 15 "hide-a"
    $hidden = Get-P2Snap ([uint64]$a.hwnd)
    Start-Sleep -Milliseconds 500
    $showFloor = Get-P2LatestTick $run.LogPath
    $null = Invoke-P2Cli @("workspace", "--select", "$homeOrdinal") "show-home"
    $showDispatch = Wait-P2WorkspaceDispatch $run.LogPath "select" $showFloor 30 "show-dispatch"
    $showApply = Wait-P2View $run.LogPath $out $homeWorkspace ([int64](Get-P2Prop $showDispatch "tick")) 30 "show-view"
    $reveal = Wait-P2Native ([uint64]$a.hwnd) $f1 3 20 "show-a"
    if (-not $reveal.visible -or $reveal.iconic) { throw "revealed float is not visible" }
    $null = Wait-P2Native ([uint64]$a.hwnd) $f1 3 5 "show-a-stable"
    $run.Evidence.stages.first_float = @{
      pre_frame = $f0Vis
      effective_frame = $f1
      applied_tick = $appliedTick
      applied_row = $appliedRow
      settled_row = $settledRow
      native = $native1
      adopted = $adopt
      adoption_after_apply = $true
      hide_tick = [int64](Get-P2Prop $hideApply "tick")
      hidden = $hidden
      show_tick = [int64](Get-P2Prop $showApply "tick")
      reveal = $reveal
      retained_post_placement = $true
      pre_placement_frame_not_reused = (-not (Test-P2RectEqual $f1 $f0Vis 0))
    }
    if ($null -eq $showApply) { throw "show apply missing" }
    Write-P2Log $run.RunDir "first float placed at $($f1 -join ',') and survived hide/reveal at the post-placement frame"
  }
}

function Initialize-P2Artifacts {
  Initialize-P2Native
  $script:P2TargetDir = Get-StressTargetDir
  if (-not (Test-Path -LiteralPath $script:P2TargetDir -PathType Container)) {
    throw "missing build target directory $($script:P2TargetDir)"
  }
  $script:P2TargetDir = (Resolve-Path -LiteralPath $script:P2TargetDir).Path
  $sourceOwner = Join-Path $script:P2TargetDir "debug\tiler-windows.exe"
  $sourceFixture = Join-Path $script:P2TargetDir "debug\tiler-live-fixture.exe"
  if (-not (Test-Path -LiteralPath $sourceOwner -PathType Leaf)) {
    throw "missing $sourceOwner; build tiler-windows and tiler-live-fixture first"
  }
  if (-not (Test-Path -LiteralPath $sourceFixture -PathType Leaf)) {
    throw "missing $sourceFixture; build tiler-windows and tiler-live-fixture first"
  }
  $script:P2SettingsPath = Join-Path $env:LOCALAPPDATA "omnitiler\settings.json"
  if (-not (Test-Path -LiteralPath $script:P2SettingsPath -PathType Leaf)) {
    throw "missing live settings preimage source $script:P2SettingsPath"
  }
  $script:P2BinDir = Join-Path $script:P2TargetDir "task9\phase2-parity-bin-$((Get-Date -Format 'yyyyMMdd-HHmmss'))-$PID"
  New-Item -ItemType Directory -Force -Path $script:P2BinDir | Out-Null
  $script:P2OwnerCopy = Join-Path $script:P2BinDir "tiler-windows.exe"
  $script:P2FixtureCopy = Join-Path $script:P2BinDir "tiler-live-fixture.exe"
  Copy-Item -LiteralPath $sourceOwner -Destination $script:P2OwnerCopy -Force
  Copy-Item -LiteralPath $sourceFixture -Destination $script:P2FixtureCopy -Force
}

function Get-P2SourceSurface {
  $checks = @(
    @("crates/tiler-core/src/world_layout/tree_edit.rs", "admission_placement_for"),
    @("crates/tiler-core/src/world_layout/admission.rs", "select_admission_target"),
    @("crates/tiler-core/src/world_layout/float.rs", "centered_float_rect"),
    @("crates/tiler-core/src/directional.rs", "select_perpendicular_child"),
    @("crates/tiler-core/src/world_model.rs", "take_admission_focus"),
    @("crates/tiler-windows/src/tiling_sys/capacity_floats.rs", "adopt_floats_from_reply"),
    @("crates/tiler-windows/src/tiling_sys/dispatch.rs", "unfloat-no-space"),
    @("crates/tiler-windows/src/tiling_sys/world_tick.rs", "fresh_route")
  )
  $rows = New-Object System.Collections.ArrayList
  foreach ($check in $checks) {
    $path = Join-Path $script:P2Repo $check[0]
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
      throw "source surface missing file $($check[0])"
    }
    if (-not (Select-String -LiteralPath $path -Pattern $check[1] -SimpleMatch -Quiet)) {
      throw "source surface missing $($check[1]) in $($check[0])"
    }
    $null = $rows.Add(@{ file = $check[0]; symbol = $check[1] })
  }
  return @($rows)
}

if ($DefinitionsOnly) { return }
if (-not $Live) {
  Write-Output "phase2-parity: no live authority; pass -Live to run a proof (or -DefinitionsOnly to parse)"
  return
}

$script:P2SourceSurface = Get-P2SourceSurface
Initialize-P2Artifacts

$proofs = if ($ParityProof -eq "all") { @("nospace", "perpendicular", "inactive", "first-float") } else { @($ParityProof) }
$failure = $null
foreach ($name in $proofs) {
  try {
    switch ($name) {
      "nospace" { Invoke-P2NoSpace }
      "perpendicular" { Invoke-P2Perpendicular }
      "inactive" { Invoke-P2Inactive }
      "first-float" { Invoke-P2FirstFloat }
      default { throw "unknown proof $name" }
    }
    Write-Output "PASS phase2-parity-$name"
  } catch {
    $failure = "phase2-parity-${name}: $($_.Exception.Message)"
    break
  }
}
if ($null -ne $failure) {
  Write-Output "FAIL $failure"
  throw "FAIL $failure"
}
Write-Output "PASS phase2-parity: $($proofs -join ', ')"
