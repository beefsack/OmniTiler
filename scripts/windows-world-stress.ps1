param(
  [switch]$Mock,
  [switch]$Live,
  [switch]$Stop,
  [string]$RunDir = "",
  [string]$Case = "",
  [int]$DelayMs = 0,
  [int]$OwnerSeconds = 1200
)
$ErrorActionPreference = "Stop"
$Repo = Split-Path -Parent $PSScriptRoot
. (Join-Path $Repo "scripts\windows-dev.ps1")
. (Join-Path $Repo "scripts\windows-live-lib.ps1")

# Fixture-only native world-stress leg over the normal `tile` loop.
# Reuses the shared JSON definitions 1:1 (no forked oracle); each action
# maps to the existing exact-owner CLI controls, fixture spawn/close/focus,
# or an explicit checkpoint observation. No synthetic input, no
# AttachThreadInput, no clicks; fixture focus is the only activation
# primitive and a failed readback aborts the leg.
#
#   pwsh -NoProfile -File scripts/windows-world-stress.ps1 -Mock
#   pwsh -NoProfile -File scripts/windows-world-stress.ps1 -Live [-Case <path>] [-DelayMs N]
#   pwsh -NoProfile -File scripts/windows-world-stress.ps1 -Stop -RunDir '<dir>'
#
# Safety: explicit flags mandatory. Scope is fixture-only
# (`--scope-exe tiler-live-fixture.exe`); unrelated windows are untouched.
# The owner payload runs privately via the Explorer broker; the fixture
# server stays a direct child because its single announcement line needs a
# captured stdout pipe (a broker launch cannot carry it). Exact PID/creation
# binding preserves ownership either way. Every phase ends in the shared
# abort-safe cleanup (exact owner stop+restore, fixture shutdown, settings
# byte restore/SPI verification/residue) which also runs from `finally` on
# every failure including startup.
#
# Delivery semantics (crates/tiler-windows/src/tiling_sys/world_tick.rs):
# `accepted` is a freshly accepted reply (new revision); `command-consumed`
# is a freshly accepted observation reply whose already-consumed command
# never re-executed. Both are fresh observations. `duplicate` repeats the
# cached reply with no new revision and is never fresh acceptance: the leg
# never treats it as fresh. Per-command waits cover transport publication
# only (single-pending request-file availability plus the dispatch log
# line); native readbacks and full settlement happen only at explicit
# checkpoints. Product refusals/drops surface as test failures, never
# silent repairs. Renderer payload (`pager-cells`) is marks, not pixels:
# geometry evidence is desired allocations plus independent native frames.
# Tentative live-pending points are marked TENTATIVE.

$WS_STEPS = [System.Collections.ArrayList]@()
$script:WsReq = 0
function Rec-Ws([string]$Name, $Data) { $null = $WS_STEPS.Add(@{ name = $Name; data = $Data }) }
function Fail-Ws([string]$Msg) { throw $Msg }
function New-StressReqId([string]$Prefix) {
  $script:WsReq++
  return "$Prefix-$script:WsReq"
}

function Get-StressCase([string]$Path) {
  if ($Path -eq "") { $Path = Join-Path $Repo "test-fixtures\world-stress-home-away.json" }
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { Fail-Ws "missing case $Path" }
  return (Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json)
}

function Get-StressTargetDir {
  # Build output honors CARGO_TARGET_DIR; never a hardcoded target/ path.
  if ("$env:CARGO_TARGET_DIR" -ne "") { return "$env:CARGO_TARGET_DIR" }
  return (Join-Path $Repo "target")
}

function Validate-StressCase($Case) {
  # Load-time contract gate: refuse anything the native leg cannot honor
  # before any live effect. Single-screen cases only; every command carries
  # seq; observation ops carry no seq.
  if ($null -eq $Case.name -or "$($Case.name)" -eq "") { Fail-Ws "case missing name" }
  if (@($Case.screens).Count -ne 1) { Fail-Ws "refuse: multi-screen case" }
  $screen = @($Case.screens)[0]
  foreach ($f in @("out", "work")) {
    if ($null -eq $screen.$f) { Fail-Ws "case screen missing $f" }
  }
  if (@($screen.work).Count -ne 4) { Fail-Ws "case screen work must be [x,y,w,h]" }
  if ($null -eq $Case.settings -or $null -eq $Case.settings.default_tiled) {
    Fail-Ws "case settings missing default_tiled"
  }
  $known = [System.Collections.ArrayList]@()
  foreach ($p in $Case.initial_facts.PSObject.Properties) { $null = $known.Add($p.Name) }
  foreach ($w in @($Case.windows)) {
    if ($known -notcontains "$w") { $null = $known.Add("$w") }
  }
  $seqs = [System.Collections.ArrayList]@()
  foreach ($a in @($Case.actions)) {
    if ($null -eq $a.op -or "$($a.op)" -eq "") { Fail-Ws "action missing op" }
    $isCmd = "$($a.op)" -notmatch "^(admit|close|focus-window|checkpoint)$"
    if ($isCmd -and $null -eq $a.seq) { Fail-Ws "$($a.op) needs seq" }
    if ((-not $isCmd) -and ($null -ne $a.seq)) { Fail-Ws "$($a.op) must not carry seq" }
    if ($isCmd) { $null = $seqs.Add([int]$a.seq) }
    switch ("$($a.op)") {
      "select-workspace" {
        if ("$($a.workspace)" -ne "home" -and "$($a.workspace)" -ne "away") {
          Fail-Ws "select-workspace workspace must name the home/away alias"
        }
        if ("$($a.output)" -ne "$($screen.out)") { Fail-Ws "select-workspace output must name the case screen" }
      }
      "select-numbered" {
        if ([int]$a.index -lt 1 -or [int]$a.index -gt 9) { Fail-Ws "select-numbered index must be 1..9" }
      }
      "select-relative" {
        if ([int]$a.delta -ne -1 -and [int]$a.delta -ne 1) { Fail-Ws "select-relative delta must be -1/+1" }
      }
      "select-previous" { }
      "send" {
        if ($known -notcontains "$($a.window)") { Fail-Ws "send unknown window $($a.window)" }
        if ("$($a.workspace)" -ne "home" -and "$($a.workspace)" -ne "away") {
          Fail-Ws "send workspace must name the home/away alias"
        }
        if ("$($a.output)" -ne "$($screen.out)") { Fail-Ws "send output must name the case screen" }
        if ($null -eq $a.follow) { Fail-Ws "send needs follow" }
      }
      { $_ -in @("focus-directional", "move-directional") } {
        if ($known -notcontains "$($a.window)") { Fail-Ws "$($a.op) unknown window $($a.window)" }
        if ("$($a.direction)" -notin @("left", "right", "up", "down")) { Fail-Ws "$($a.op) bad direction" }
        if ("$($a.op)" -eq "move-directional" -and "$($a.same_axis)" -ne "group-with-neighbor") {
          Fail-Ws "move-directional same_axis must be group-with-neighbor"
        }
        if ($null -eq $a.expect_effect -or $null -eq $a.expect_target) {
          Fail-Ws "$($a.op) needs expect_effect/expect_target"
        }
        if ($known -notcontains "$($a.expect_target)") { Fail-Ws "$($a.op) unknown expect_target" }
      }
      "admit" {
        if ($null -eq $a.rect -or @($a.rect).Count -ne 4) { Fail-Ws "admit needs rect [x,y,w,h]" }
      }
      "close" {
        if ($known -notcontains "$($a.window)") { Fail-Ws "close unknown window $($a.window)" }
      }
      "focus-window" {
        if ($known -notcontains "$($a.window)") { Fail-Ws "focus-window unknown window $($a.window)" }
      }
      "checkpoint" {
        foreach ($f in @("name", "view", "members", "homing", "focus", "pager_current", "pager_occupied")) {
          $prop = $a.PSObject.Properties[$f]
          if ($null -eq $prop -or $null -eq $prop.Value) { Fail-Ws "checkpoint missing $f" }
        }
        if ($null -eq $a.PSObject.Properties["foreground"]) { Fail-Ws "checkpoint missing foreground" }
        if ($null -eq $a.PSObject.Properties["tiles"]) { Fail-Ws "checkpoint missing tiles" }
      }
      default { Fail-Ws "unknown op $($a.op)" }
    }
  }
  for ($i = 1; $i -lt $seqs.Count; $i++) {
    if ($seqs[$i] -le $seqs[$i - 1]) { Fail-Ws "seqs not strictly increasing" }
  }
  if ($seqs.Count -lt 100) { Fail-Ws "volume $($seqs.Count) < 100" }
  return $seqs.Count
}

# --- Pure mapping + comparison seams (mock-testable, no live effects) ---

function Resolve-StressAlias([string]$Alias, $Ring, [string]$HomeId, [string]$AwayId) {
  # Dynamic alias resolution against the latest observed ring: home/away
  # bind once to stable core ids, ordinals follow the ring. Any refresh
  # that moves the binding fails loudly instead of selecting the wrong view.
  $id = if ($Alias -eq "home") { $HomeId } elseif ($Alias -eq "away") { $AwayId } else { "" }
  if ($id -eq "") { Fail-Ws "unmapped workspace alias $Alias" }
  $idx = @($Ring).IndexOf($id)
  if ($idx -lt 0) { Fail-Ws "alias $Alias ($id) not in ring" }
  return ($idx + 1)
}

function Get-StressCliArgs($Action, $AliasOrd) {
  # One case command to exact-owner CLI argv. select-workspace/send aliases
  # resolve through the caller-supplied current ordinals (see
  # Resolve-StressAlias); numbered/relative/previous ride the core-resolved
  # forms verbatim. The adapter never takes an explicit output.
  switch ($Action.op) {
    "select-workspace" {
      $idx = $AliasOrd[$Action.workspace]
      if ($null -eq $idx) { Fail-Ws "unmapped workspace alias $($Action.workspace)" }
      return @("workspace", "--select", "$idx")
    }
    "select-numbered" { return @("workspace", "--select", "$($Action.index)") }
    "select-relative" {
      if ([int]$Action.delta -gt 0) { return @("workspace", "--relative", "next") }
      return @("workspace", "--relative", "previous")
    }
    "select-previous" { return @("workspace", "--previous") }
    "send" {
      $idx = $AliasOrd[$Action.workspace]
      if ($null -eq $idx) { Fail-Ws "unmapped send alias $($Action.workspace)" }
      if ($Action.follow) { return @("workspace", "--send", "$idx") }
      return @("workspace", "--stay", "$idx")
    }
    "focus-directional" { return @("directional", "--focus", "--direction", "$($Action.direction)") }
    "move-directional" { return @("directional", "--move", "--direction", "$($Action.direction)") }
    default { Fail-Ws "not a command op $($Action.op)" }
  }
  return $null
}

function Convert-StressTiles($Tiles, $AuthoredWork, $DiscoveredWork) {
  # Mirrors tiler-protocol normalize_tiles_to_work: identity fast path,
  # otherwise every half must scale integrally or the leg refuses.
  $ax = $AuthoredWork[0]; $ay = $AuthoredWork[1]
  $aw = $AuthoredWork[2]; $ah = $AuthoredWork[3]
  $dx = $DiscoveredWork[0]; $dy = $DiscoveredWork[1]
  $dw = $DiscoveredWork[2]; $dh = $DiscoveredWork[3]
  $out = @{}
  $entries = @()
  if ($Tiles -is [hashtable]) {
    foreach ($k in $Tiles.Keys) { $entries += @{ Name = "$k"; Value = $Tiles[$k] } }
  } else {
    foreach ($p in $Tiles.PSObject.Properties) { $entries += @{ Name = "$($p.Name)"; Value = $p.Value } }
  }
  foreach ($p in $entries) {
    $t = @($p.Value)
    if ($aw -eq $dw -and $ah -eq $dh -and $ax -eq $dx -and $ay -eq $dy) {
      $out[$p.Name] = @([int]$t[0], [int]$t[1], [int]$t[2], [int]$t[3])
      continue
    }
    if ($aw -le 0 -or $ah -le 0 -or $dw -le 0 -or $dh -le 0) { Fail-Ws "non-positive work area" }
    $nx = ($t[0] - $ax) * $dw; $ny = ($t[1] - $ay) * $dh
    $nw = $t[2] * $dw; $nh = $t[3] * $dh
    foreach ($pair in @(@($nx, $aw), @($ny, $ah), @($nw, $aw), @($nh, $ah))) {
      if ($pair[0] % $pair[1] -ne 0) { Fail-Ws "tile $($p.Name) does not scale integrally" }
    }
    $out[$p.Name] = @([int]($nx / $aw + $dx), [int]($ny / $ah + $dy), [int]($nw / $aw), [int]($nh / $ah))
  }
  return $out
}

function Test-StressApplyClean($Event, [string]$Tag) {
  # Every known honest omission section of a world-apply row must be present
  # and zero; pending intent arrays must be empty; geometry must be paired
  # with a numeric zero omission. Source: apply_diag_value in
  # crates/tiler-windows/src/tiling_sys/world_tick.rs. A missing field is a
  # refusal, never a pass.
  foreach ($f in @("members_omitted", "views_omitted", "frames_omitted", "clears_omitted",
      "clear_holds_omitted", "desired_operations_omitted", "operation_settled_omitted",
      "focus_effects_omitted", "focus_dispatch_omitted", "inflight_events_omitted",
      "foreground_meta_omitted", "visibility_posts_omitted", "desired_windows_omitted",
      "desired_outputs_omitted", "desired_workspaces_omitted", "held_scopes_omitted",
      "desired_transfers_omitted")) {
    $prop = $Event.PSObject.Properties[$f]
    if ($null -eq $prop) { Fail-Ws "$Tag omission field missing: $f" }
    try { $v = [int64]$prop.Value } catch { Fail-Ws "$Tag omission not numeric: $f" }
    if ($v -ne 0) { Fail-Ws "$Tag omitted $($f)=$($prop.Value)" }
  }
  $gprop = $Event.PSObject.Properties["desired_geometry_omitted"]
  if ($null -eq $gprop -or $null -eq $gprop.Value) { Fail-Ws "$Tag geometry omission missing/null" }
  try { $gv = [int64]$gprop.Value } catch { Fail-Ws "$Tag geometry omission not numeric" }
  if ($gv -ne 0) { Fail-Ws "$Tag geometry omitted $gv" }
  if ("$($Event.geometry_status)" -ne "paired") { Fail-Ws "$Tag geometry not paired" }
  if (@($Event.desired_transfers).Count -gt 0) { Fail-Ws "$Tag pending transfers" }
  if (@($Event.desired_operations).Count -gt 0) { Fail-Ws "$Tag pending operations" }
  if (@($Event.held_scopes).Count -gt 0) { Fail-Ws "$Tag held scopes" }
  return $true
}

function Compare-StressCheckpoint($Expected, $Actual) {
  # Pure evidence comparison: throws fail-closed on the first mismatch.
  # $Expected: View, Members (caller ids), Homing (id -> home|away),
  #   Tiles (id -> [x,y,w,h] widths), Foreground (id or $null),
  #   PagerCurrent (home|away), PagerOccupied (subset of home|away).
  # $Actual: NativeOut (wire o#), HomeId/AwayId (stable core ids),
  #   Ring (order-sorted core ids on NativeOut), Tokens (caller -> token),
  #   Desired (token -> accepted desired row), DesiredView,
  #   Frames (caller -> outer/visible [l,t,r,b] + flag),
  #   ForegroundHwnd, ManagedHwnds, Pager (matched renderer entry),
  #   PagerOutputsTotal/Omitted, Revision.
  # Desired rects carry {x,y,w,h} widths (world_tick rect_value), never
  # right/bottom edges. Native outer/visible rects are [l,t,r,b].
  $ring = @($Actual.Ring)
  if ($ring.Count -lt 2) { Fail-Ws "ring has fewer than 2 workspaces" }
  if ("$($ring[0])" -ne "$($Actual.HomeId)") { Fail-Ws "home left ring head" }
  if ("$($ring[1])" -ne "$($Actual.AwayId)") { Fail-Ws "away left ring position 2" }
  $viewId = if ($Expected.View -eq "home") { $Actual.HomeId } else { $Actual.AwayId }
  if ("$($Actual.DesiredView)" -ne "$viewId") { Fail-Ws "view $($Actual.DesiredView) != $viewId" }
  $wantTokens = @($Expected.Members | ForEach-Object { $Actual.Tokens[$_] } | Sort-Object)
  $gotTokens = @($Actual.Desired.Keys | Sort-Object)
  if (($wantTokens -join ",") -ne ($gotTokens -join ",")) {
    Fail-Ws "roster $($gotTokens -join ',') != $($wantTokens -join ',')"
  }
  foreach ($id in $Expected.Members) {
    $tok = $Actual.Tokens[$id]
    if ($null -eq $tok -or "$tok" -eq "") { Fail-Ws "null token for $id" }
    $row = $Actual.Desired[$tok]
    if ($null -eq $row) { Fail-Ws "no desired row for $id" }
    $aliasId = if ($Expected.Homing[$id] -eq "home") { $Actual.HomeId } else { $Actual.AwayId }
    if ("$($row.ws)" -ne "$aliasId") { Fail-Ws "$id homed $($row.ws) != $aliasId" }
    if ("$($row.out)" -ne "$($Actual.NativeOut)") { Fail-Ws "$id out $($row.out) != $($Actual.NativeOut)" }
    $tile = $Expected.Tiles[$id]
    if ($null -eq $tile) { Fail-Ws "no normalized tile for $id" }
    foreach ($f in @("allocation", "effective_frame")) {
      $r = $row[$f]
      if ($null -eq $r) { Fail-Ws "$id $f missing" }
      if ([int]$r.x -ne [int]$tile[0] -or [int]$r.y -ne [int]$tile[1] -or
          [int]$r.w -ne [int]$tile[2] -or [int]$r.h -ne [int]$tile[3]) {
        Fail-Ws "$id $f [$($r.x),$($r.y),$($r.w),$($r.h)] != tile [$($tile -join ',')]"
      }
    }
    foreach ($h in @("frame_hold", "membership_hold")) {
      if ($null -ne $row[$h] -and "$($row[$h])" -ne "") { Fail-Ws "$id $h present" }
    }
    if ($row.floating -ne $false) { Fail-Ws "$id floating" }
    if ($null -ne $row.float_origin -and "$($row.float_origin)" -ne "") { Fail-Ws "$id float origin" }
    if ("$($row.frame_op)" -ne "" -or "$($row.clear_op)" -ne "") { Fail-Ws "$id pending op" }
    if ("$($row.frame_authority)" -ne "controlled" -or "$($row.membership_authority)" -ne "controlled") {
      Fail-Ws "$id authority not controlled"
    }
    $fr = $Actual.Frames[$id]
    if ($null -eq $fr -or $null -eq $fr.outer -or $null -eq $fr.visible) { Fail-Ws "no native frame for $id" }
    if (-not $fr.alive) { Fail-Ws "$id not alive" }
    if (-not $fr.ok_outer) { Fail-Ws "$id outer unreadable" }
    if ([int]$fr.outer[0] -ne [int]$tile[0] -or [int]$fr.outer[1] -ne [int]$tile[1] -or
        [int]$fr.outer[2] -ne ([int]$tile[0] + [int]$tile[2]) -or
        [int]$fr.outer[3] -ne ([int]$tile[1] + [int]$tile[3])) {
      Fail-Ws "$id outer [$($fr.outer -join ',')] != tile [$($tile -join ',')]"
    }
    $shownHere = ($Expected.Homing[$id] -eq $Expected.View)
    if ($shownHere) {
      if (-not $fr.ok_visible) { Fail-Ws "$id visible unreadable while shown" }
      if ($fr.flag -ne $true) { Fail-Ws "$id not visible while shown" }
      if ([int]$fr.visible[0] -ne [int]$fr.outer[0] -or [int]$fr.visible[1] -ne [int]$fr.outer[1] -or
          [int]$fr.visible[2] -ne [int]$fr.outer[2] -or [int]$fr.visible[3] -ne [int]$fr.outer[3]) {
        Fail-Ws "$id visible [$($fr.visible -join ',')] != outer [$($fr.outer -join ',')]"
      }
    } else {
      if ($fr.flag -eq $true) { Fail-Ws "$id visible while hidden" }
    }
  }
  if ($null -eq $Expected.Foreground) {
    if (@($Actual.ManagedHwnds.Values) -contains $Actual.ForegroundHwnd) {
      Fail-Ws "foreground $($Actual.ForegroundHwnd) is managed while null expected"
    }
  } else {
    $wantHwnd = $Actual.ManagedHwnds[$Expected.Foreground]
    if ($null -eq $wantHwnd) { Fail-Ws "no hwnd for foreground $($Expected.Foreground)" }
    if ([uint64]$Actual.ForegroundHwnd -ne [uint64]$wantHwnd) {
      Fail-Ws "foreground $($Actual.ForegroundHwnd) != $wantHwnd"
    }
  }
  # Renderer payload join (pager-cells trace over the actual frames sent to
  # the renderer: one bounded entry per payload change). ws tokens are the
  # core logical ids themselves (workspace_token is identity); only the
  # output token is a runtime wire id. Marks only, never pixels.
  $entry = $Actual.Pager
  if ($null -eq $entry) { Fail-Ws "no matched pager entry" }
  if ([int]$entry.revision -ne [int]$Actual.Revision) {
    Fail-Ws "pager revision $($entry.revision) != $($Actual.Revision)"
  }
  if ("$($entry.output)" -ne "$($Actual.NativeOut)") { Fail-Ws "pager output mismatch" }
  if ([int]$Actual.PagerOutputsOmitted -ne 0) { Fail-Ws "pager outputs omitted" }
  if ([int]$entry.cells_omitted -ne 0) { Fail-Ws "pager cells omitted" }
  $cells = @($entry.cells)
  if ([int]$entry.cells_total -ne $cells.Count) { Fail-Ws "pager cells_total mismatch" }
  if ($cells.Count -ne $ring.Count) { Fail-Ws "pager cells $($cells.Count) != ring $($ring.Count)" }
  $curViewIdx = @($ring).IndexOf($viewId)
  if ([int]$entry.active -ne $curViewIdx) { Fail-Ws "pager active $($entry.active) != $curViewIdx" }
  $wantOccIdx = @($Expected.PagerOccupied | ForEach-Object {
    $oid = if ($_ -eq "home") { $Actual.HomeId } else { $Actual.AwayId }
    @($ring).IndexOf("$oid")
  })
  for ($i = 0; $i -lt $cells.Count; $i++) {
    $c = $cells[$i]
    if ([int]$c.ord -ne ($i + 1)) { Fail-Ws "pager ord out of ring order" }
    if ("$($c.ws)" -ne "$($ring[$i])") { Fail-Ws "pager ws $($c.ws) != ring $($ring[$i])" }
    if ([int]$c.minis_omitted -ne 0) { Fail-Ws "pager minis omitted at cell $i" }
    if ([int]$c.minis_total -ne @($c.minis).Count) { Fail-Ws "pager minis_total mismatch at cell $i" }
    $trueCount = @($c.minis | Where-Object { $_ -eq $true }).Count
    if ([int]$c.focused_total -ne $trueCount) { Fail-Ws "pager focused_total mismatch at cell $i" }
    if ((($c.cur -eq $true)) -ne ($i -eq $curViewIdx)) { Fail-Ws "pager current mark wrong at cell $i" }
    $occ = ([int]$c.minis_total -gt 0)
    if ($occ -ne ($wantOccIdx -contains $i)) { Fail-Ws "pager occupied mark wrong at cell $i" }
    if ((($c.focused -eq $true)) -ne ($trueCount -gt 0)) { Fail-Ws "pager focused mark mismatch at cell $i" }
  }
  if ($null -ne $Expected.Foreground) {
    # TENTATIVE: mini focused marks are read verbatim from the renderer
    # payload; joining them to real foreground holds for the observed
    # fixture legs and is re-proven live.
    $fgHome = $Expected.Homing[$Expected.Foreground] -eq "home"
    $fgIdx = @($ring).IndexOf($(if ($fgHome) { $Actual.HomeId } else { $Actual.AwayId }))
    if ($cells[$fgIdx].focused -ne $true) { Fail-Ws "pager focus mark wrong" }
  }
  return $true
}

# --- Live seams (native effects only under -Live) ---

if (-not ("StressNative" -as [type])) {
  $csharp = @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class StressNative {
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc p, IntPtr l);
  public delegate bool EnumWindowsProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint p);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr h);
}
"@
  Add-Type -TypeDefinition $csharp | Out-Null
}

function Get-StressTitles {
  $found = [System.Collections.ArrayList]@()
  $cb = [StressNative+EnumWindowsProc]{
    param([IntPtr]$h, [IntPtr]$l)
    $sb = New-Object Text.StringBuilder 512
    $n = [StressNative]::GetWindowTextW($h, $sb, 512)
    if ($n -gt 0) {
      $t = $sb.ToString()
      if ($t.StartsWith("OmniTilerTestWindow:")) { $null = $found.Add(@{ hwnd = [uint64]$h.ToInt64(); title = $t }) }
    }
    return $true
  }
  $null = [StressNative]::EnumWindows($cb, [IntPtr]::Zero)
  return $found
}

function Get-StressHwndOfCaller($Titles, [string]$Id) {
  foreach ($t in $Titles) {
    $parts = "$($t.title)".Split(":")
    if ($parts.Count -ge 3 -and $parts[0] -eq "OmniTilerTestWindow" -and $parts[1] -eq $Id) {
      return [uint64]$t.hwnd
    }
  }
  return $null
}

function Get-StressDisplayBaseline {
  # Read-only primary-screen baseline for staging (virtualized coordinates;
  # exact geometry comes from the product geometry rows, never this).
  Add-Type -AssemblyName System.Windows.Forms | Out-Null
  $scr = [System.Windows.Forms.Screen]::PrimaryScreen
  $wa = $scr.WorkingArea
  $bo = $scr.Bounds
  return @{
    work = @([int]$wa.X, [int]$wa.Y, [int]$wa.Width, [int]$wa.Height)
    full = @([int]$bo.X, [int]$bo.Y, [int]$bo.Width, [int]$bo.Height)
  }
}

function Send-StressFixture([string]$FixtureExe, [string]$Socket, [string]$Nonce, [string]$Json) {
  # Stdout carries exactly the reply lines; stderr carries logs (never
  # parsed). Fixture reply shape: {ok, result:{windows:[...]}}.
  $lines = @(& $FixtureExe send --socket $Socket --nonce $Nonce $Json 2>$null | Out-String -Stream |
    Where-Object { "$_".TrimStart().StartsWith("{") })
  if ($LASTEXITCODE -ne 0) { Fail-Ws "fixture send exit ${LASTEXITCODE}: $Json" }
  if ($lines.Count -eq 0) { return @{ ok = $true; result = $null } }
  if ($lines.Count -ne 1) { Fail-Ws "fixture send returned $($lines.Count) lines: $Json" }
  return ($lines[0] | ConvertFrom-Json)
}

function Start-StressFixture([string]$FixtureExe) {
  $psi = New-Object Diagnostics.ProcessStartInfo
  $psi.FileName = $FixtureExe
  $psi.Arguments = "serve"
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.UseShellExecute = $false
  $psi.CreateNoWindow = $true
  $proc = New-Object Diagnostics.Process
  $proc.StartInfo = $psi
  $null = $proc.Start()
  try {
    $task = $proc.StandardOutput.ReadLineAsync()
    if (-not $task.Wait(60000)) { throw "fixture announcement timeout" }
    $line = $task.Result
    if ([string]::IsNullOrWhiteSpace($line)) { throw "fixture announcement missing" }
    return @{ proc = $proc; announcement = ($line | ConvertFrom-Json) }
  } catch {
    try { $proc.Kill() } catch {}
    Fail-Ws "fixture serve failed: $($_.Exception.Message)"
    return $null
  }
}

function Start-StressBootstrap([string]$FixtureExe, [string]$Socket, [string]$Nonce, $Case, $AuthoredWork, $StagedWork) {
  # Spawn the initial_facts bootstrap pair before owner startup, in
  # deterministic id order, staged on the discovered screen with the shared
  # split orientation, borderless like the shared layout. Later `admit`
  # actions spawn single windows with their arrival rects.
  $ids = @($Case.initial_facts.PSObject.Properties | ForEach-Object { $_.Name } | Sort-Object)
  if ($ids.Count -eq 0) { Fail-Ws "no initial facts to bootstrap" }
  $tiles = @{}
  foreach ($id in $ids) {
    $tiles[$id] = @($Case.initial_facts.PSObject.Properties[$id].Value)
  }
  $staged = Convert-StressTiles $tiles $AuthoredWork $StagedWork
  $windows = @()
  foreach ($id in $ids) {
    $t = $staged[$id]
    $windows += @{
      id = "$id"
      size_px = @([int]$t[2], [int]$t[3])
      position_outer_px = @([int]$t[0], [int]$t[1])
      decorations = $false
    }
  }
  $body = @{ v = 1; request_id = (New-StressReqId "bootstrap"); op = "spawn"; windows = $windows } |
    ConvertTo-Json -Depth 6 -Compress
  $r = Send-StressFixture $FixtureExe $Socket $Nonce $body
  $entries = @($r.result.windows)
  if ($entries.Count -ne $ids.Count) { Fail-Ws "bootstrap spawn returned $($entries.Count) entries" }
  foreach ($id in $ids) {
    $e = @($entries | Where-Object { "$($_.id)" -eq "$id" })
    if ($e.Count -ne 1 -or $e[0].ok -ne $true) {
      Fail-Ws "bootstrap spawn $id refused: $($r | ConvertTo-Json -Depth 4 -Compress)"
    }
  }
  Rec-Ws "bootstrap" @{ ids = ($ids -join ","); staged_work = ($StagedWork -join ",") }
  return @{ ids = $ids; staged = $staged }
}

function Set-StressCaseSettings([string]$SettingsPath, $Case) {
  # Bounded pre-start settings configuration from supported product fields
  # only (crates/tiler-windows/src/settings/schema.rs): tiled-only cases
  # need default_tiled=true, the group-with-neighbor product default made
  # explicit for move-directional, and per-output-local scope. Gaps ride the
  # explicit tile CLI switches instead. Everything else (including absent
  # fields) stays untouched; the preimage restores exact bytes afterwards.
  $raw = Get-Content -LiteralPath $SettingsPath -Raw
  try { $doc = $raw | ConvertFrom-Json } catch { Fail-Ws "refuse: live settings do not parse as JSON" }
  if ($null -eq $doc.revision) { Fail-Ws "refuse: live settings carry no revision" }
  if ($null -eq $doc.core) { Fail-Ws "refuse: live settings carry no core section" }
  if ($null -eq $doc.core.workspace) { Fail-Ws "refuse: live settings carry no workspace section" }
  $changed = [System.Collections.ArrayList]@()
  if ("$($doc.core.workspace.default_tiled)" -ne "True") {
    $doc.core.workspace | Add-Member -NotePropertyName "default_tiled" -NotePropertyValue $true -Force
    $null = $changed.Add("workspace.default_tiled=true")
  }
  if ("$($doc.core.same_axis_move)" -ne "group-with-neighbor") {
    $doc.core | Add-Member -NotePropertyName "same_axis_move" -NotePropertyValue "group-with-neighbor" -Force
    $null = $changed.Add("same_axis_move=group-with-neighbor")
  }
  if ("$($doc.core.workspace.mode)" -ne "per-output-local") {
    $doc.core.workspace | Add-Member -NotePropertyName "mode" -NotePropertyValue "per-output-local" -Force
    $null = $changed.Add("workspace.mode=per-output-local")
  }
  if ($changed.Count -gt 0) {
    ($doc | ConvertTo-Json -Depth 20 -Compress) | Set-Content -LiteralPath $SettingsPath -Encoding utf8NoBOM
    try { $back = (Get-Content -LiteralPath $SettingsPath -Raw) | ConvertFrom-Json } catch {
      Fail-Ws "refuse: configured settings do not parse as JSON"
    }
    if ($null -eq $back.revision) { Fail-Ws "refuse: configured settings lost revision" }
  }
  Rec-Ws "case-settings" @{ changed = @($changed) }
  return @($changed)
}

function Assert-NoPreexistingStressOwner([string]$OwnerPayload, [string]$Tag) {
  # Refuse before launch when any live owner exists: never stop another
  # owner's process. Fail-closed on identity/ready query failure with a
  # present ledger.
  $ledDir = ((& $OwnerPayload @("identity") | ConvertFrom-Json).ledger_directory)
  if ($null -eq $ledDir -or "$ledDir" -eq "") { Fail-Ws "$Tag refuse: no ledger directory" }
  $led = Join-Path "$ledDir" "ledger.json"
  if (Test-Path -LiteralPath $led -PathType Leaf) {
    Fail-Ws "$Tag refuse: existing owner ledger present at $led; stop/restore first (will not stop another owner)"
  }
  try {
    $raw = (& $OwnerPayload @("ready") 2>&1 | Out-String)
    $code = $LASTEXITCODE
    if ($code -eq 0) {
      $j = "$raw".Trim() | ConvertFrom-Json
      if ($j.ready -eq $true) {
        Fail-Ws "$Tag refuse: existing owner ready (owner=$($j.owner | ConvertTo-Json -Compress))"
      }
    }
  } catch {
    Fail-Ws "$Tag refuse: ready query failed without a ledger: $($_.Exception.Message)"
  }
  return "$ledDir"
}

function Wait-StressLog([string]$LogPath, [int]$Mark, [scriptblock]$Pred, [int]$TimeoutSec, [string]$Tag) {
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    $events = @(Read-LiveOwnerLog $LogPath)
    for ($i = $Mark; $i -lt $events.Count; $i++) {
      if (& $Pred $events[$i]) { return @{ event = $events[$i]; index = $i } }
    }
    Start-Sleep -Milliseconds 400
  }
  Fail-Ws "$Tag log join timeout"
  return $null
}

function Wait-StressFreshApply([string]$LogPath, [ref]$Mark, [int]$MinTick, [int]$TimeoutSec, [string]$Tag) {
  # Freshness gate: `accepted` and `command-consumed` are fresh observations
  # (the latter never re-executes its already-consumed command); `duplicate`
  # repeats the cached reply with no new revision and is never fresh. Every
  # known omission section must read zero and pending intent must be empty.
  $pred = {
    param($e)
    "$($e.event)" -eq "world-apply" -and "$($e.delivery)" -ne "duplicate" -and
      ("$($e.delivery)" -eq "accepted" -or "$($e.delivery)" -eq "command-consumed") -and
      [int]$e.tick -ge $MinTick
  }.GetNewClosure()
  $r = Wait-StressLog $LogPath $Mark.Value $pred $TimeoutSec $Tag
  $Mark.Value = [int]$r.index + 1
  $null = Test-StressApplyClean $r.event $Tag
  return $r
}

function Wait-StressDispatch([string]$LogPath, [ref]$Mark, $Argv, [int]$TimeoutSec, [string]$Tag) {
  # Transport publication consumption: the dispatch log line for this exact
  # CLI request with any honest outcome. A non-ok outcome is a useful test
  # failure at the call site, never a silent repair and never a retry.
  $op = $Argv[1]
  if ($op -eq "--focus") { $op = "focus" }
  elseif ($op -eq "--move") { $op = "move" }
  elseif ($op -eq "--select") { $op = "select" }
  elseif ($op -eq "--send") { $op = "send" }
  elseif ($op -eq "--stay") { $op = "stay" }
  elseif ($op -eq "--previous") { $op = "previous" }
  elseif ($op -eq "--relative") { $op = if ($Argv[2] -eq "next") { "relative-next" } else { "relative-prev" } }
  $edgeEvent = if ($Argv[0] -eq "directional") { "directional" } else { "workspace" }
  $pred = {
    param($e) "$($e.event)" -eq $edgeEvent -and "$($e.edge)" -eq "cli" -and "$($e.op)" -eq "$op"
  }.GetNewClosure()
  $r = Wait-StressLog $LogPath $Mark.Value $pred $TimeoutSec "$Tag-dispatch"
  $Mark.Value = [int]$r.index + 1
  return @{ tick = [int]$r.event.tick; outcome = "$($r.event.outcome)"; count = [int]$r.index + 1 }
}

function Invoke-StressCli([string]$OwnerBin, $Argv, [string]$LedgerDir, [string]$Tag) {
  # Single-pending transport: the CLI refuses with nonzero exit while a
  # request file is queued, so capture output without throwing, wait for
  # availability, and retry boundedly. Never paced on native convergence.
  $reqFile = if ($Argv[0] -eq "directional") { "directional.request" } else { "workspace.request" }
  Rec-Ws "transport" @{ mode = "single-pending"; control = $Argv[0] }
  $attempts = 0
  while ($true) {
    $attempts++
    $raw = (& $OwnerBin @Argv 2>&1 | Out-String)
    $code = $LASTEXITCODE
    $text = "$raw".Trim()
    if ($code -eq 0) {
      try { $j = $text | ConvertFrom-Json } catch {
        Fail-Ws "$Tag unreadable success output: $text"
      }
      if ($j.dispatched -eq $true) { return $j }
      Fail-Ws "$Tag not dispatched (exit 0 without dispatched): $text"
    }
    if ("$text" -match "pending" -and $attempts -lt 4) {
      Wait-StressRequestFile $LedgerDir $reqFile 15 "$Tag-$attempts"
      continue
    }
    Fail-Ws "$Tag not dispatched (attempt $attempts, exit $code): $text"
  }
  return $null
}

function Wait-StressRequestFile([string]$LedgerDir, [string]$Name, [int]$TimeoutSec, [string]$Tag) {
  $path = Join-Path $LedgerDir $Name
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return }
    Start-Sleep -Milliseconds 200
  }
  Fail-Ws "$Tag single-pending request file still present: $Name"
}

function Wait-StressHwnd([string]$Id, $FixtureState, [int]$TimeoutSec) {
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    $titles = Get-StressTitles
    $hwnd = Get-StressHwndOfCaller $titles $Id
    if ($null -ne $hwnd) {
      $pidOut = 0u
      $null = [LivePaired]::GetWindowThreadProcessId([IntPtr][long]$hwnd, [ref]$pidOut)
      if ([int]$pidOut -ne [int]$FixtureState.pid) {
        Fail-Ws "hwnd $hwnd for $Id owned by pid $pidOut, not fixture"
      }
      return [uint64]$hwnd
    }
    Start-Sleep -Milliseconds 400
  }
  Fail-Ws "hwnd join timeout for $Id"
  return $null
}

function Wait-StressAdmitted([string]$LogPath, $TokOf, [uint64]$Hwnd, $FixtureState, [int]$TimeoutSec, [string]$Tag) {
  # Fresh-admission join over production member-admitted rows
  # {window,hwnd,pid,creation}: scans from the log start so bootstrap
  # admissions in any order all bind. Exact fixture PID/creation required.
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    $events = @(Read-LiveOwnerLog $LogPath)
    foreach ($e in $events) {
      if ("$($e.event)" -ne "member-admitted") { continue }
      if ([uint64]$e.hwnd -ne $Hwnd) { continue }
      if ([int]$e.pid -ne [int]$FixtureState.pid) { Fail-Ws "admitted pid mismatch for $Tag" }
      if ("$($e.creation)" -ne "$($FixtureState.creation)") { Fail-Ws "admitted creation mismatch for $Tag" }
      $known = @($TokOf.Values)
      if ($known -contains "$($e.window)") { Fail-Ws "duplicate token for $Tag" }
      return "$($e.window)"
    }
    Start-Sleep -Milliseconds 500
  }
  Fail-Ws "admission join timeout for $Tag"
  return $null
}

function Wait-StressDeparted([string]$LogPath, [ref]$Mark, [string]$Token, [string]$Id, [int]$TimeoutSec) {
  # Departure join: a fresh accepted observation whose desired roster omits
  # the token, plus the sentinel title gone from native enumeration.
  $pred = {
    param($e)
    ("$($e.delivery)" -eq "accepted" -or "$($e.delivery)" -eq "command-consumed") -and
      "$($e.event)" -eq "world-apply" -and
      (@($e.desired_windows | Where-Object { "$($_.window)" -eq "$Token" }).Count -eq 0)
  }.GetNewClosure()
  $r = Wait-StressLog $LogPath $Mark.Value $pred $TimeoutSec "close-$Id"
  $Mark.Value = [int]$r.index + 1
  $null = Test-StressApplyClean $r.event "close-$Id"
  $titles = Get-StressTitles
  if ($null -ne (Get-StressHwndOfCaller $titles $Id)) { Fail-Ws "close $Id hwnd still titled" }
  return [int]$r.event.tick
}

function Invoke-StressFocus([string]$FixtureExe, [string]$Socket, [string]$Nonce, [string]$Id, $HwndOf, $FixtureState, [int]$TimeoutSec) {
  $want = $HwndOf[$Id]
  if ($null -eq $want) { Fail-Ws "no hwnd for focus $Id" }
  $r = Send-StressFixture $FixtureExe $Socket $Nonce (@{ v = 1; request_id = (New-StressReqId "focus-$Id"); op = "set"; ids = @($Id); patch = @{ focus = $true } } | ConvertTo-Json -Depth 6 -Compress)
  $entry = @($r.result.windows | Where-Object { "$($_.id)" -eq "$Id" })
  if ($entry.Count -ne 1 -or $entry[0].ok -ne $true) { Fail-Ws "focus $Id refused" }
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    $g = Send-StressFixture $FixtureExe $Socket $Nonce (@{ v = 1; request_id = (New-StressReqId "get-$Id"); op = "get"; ids = @($Id) } | ConvertTo-Json -Depth 6 -Compress)
    $snap = @($g.result.windows | Where-Object { "$($_.id)" -eq "$Id" })
    $focused = $false
    if ($snap.Count -eq 1 -and $null -ne $snap[0].snapshot -and $null -ne $snap[0].snapshot.observed) {
      $focused = ($snap[0].snapshot.observed.focused -eq $true)
    }
    $fg = [StressNative]::GetForegroundWindow()
    $pidOut = 0u
    $null = [StressNative]::GetWindowThreadProcessId($fg, [ref]$pidOut)
    if ($focused -and [uint64]$fg.ToInt64() -eq [uint64]$want -and [int]$pidOut -eq [int]$FixtureState.pid) {
      return
    }
    Start-Sleep -Milliseconds 300
  }
  Fail-Ws "focus acquisition failed for $Id (no fallback input; aborting)"
}

function Get-StressGeometry($ApplyEvent, [string]$NativeOut) {
  $rows = @($ApplyEvent.desired_geometry)
  if ($NativeOut -ne "") {
    $rows = @($rows | Where-Object { "$($_.output)" -eq "$NativeOut" })
  } else {
    $rows = @($rows | Where-Object { $_.primary -eq $true })
  }
  foreach ($row in $rows) {
    if ($null -ne $row.work -and $row.complete -eq $true) {
      return @{ output = "$($row.output)"; work = @([int]$row.work.x, [int]$row.work.y, [int]$row.work.w, [int]$row.work.h) }
    }
  }
  return $null
}

function Get-StressRing($ApplyEvent, [string]$NativeOut) {
  $ordered = @($ApplyEvent.desired_workspaces | Where-Object { "$($_.output)" -eq "$NativeOut" } | Sort-Object { [int]$_.order })
  if ($ordered.Count -lt 2) { Fail-Ws "ring has fewer than 2 workspaces" }
  return @($ordered | ForEach-Object { "$($_.id)" })
}

function Initialize-StressTopology($LogPath, [ref]$Mark, $Case, $TokOf, $BootstrapIds, $AuthoredWork) {
  # Baseline observation after bootstrap admissions, before the first
  # command: binds the single native output, the stable home/away core ids
  # (ring head positions), the discovered work area, and the current alias
  # ordinals. Verifies the configured zero gaps and the bootstrap roster,
  # skipping earlier fresh rows until the full roster lands.
  if (@($Case.screens).Count -ne 1) { Fail-Ws "refuse: multi-screen case" }
  $want = @($BootstrapIds | ForEach-Object { $TokOf[$_] } | Sort-Object)
  $deadline = (Get-Date).AddSeconds(60)
  $ev = $null
  while ($true) {
    $r = Wait-StressFreshApply $LogPath $Mark 0 10 "topology-init"
    $ev = $r.event
    $got = @($ev.desired_windows | ForEach-Object { "$($_.window)" } | Sort-Object)
    if (($got -join ",") -eq ($want -join ",")) { break }
    if ((Get-Date) -ge $deadline) {
      Fail-Ws "topology-init roster timeout: $($got -join ',') != $($want -join ',')"
    }
  }
  $geo = @($ev.desired_geometry | Where-Object { $_.primary -eq $true })
  if ($geo.Count -ne 1) { Fail-Ws "topology-init: want 1 primary geometry row" }
  $g = $geo[0]
  if ($g.complete -ne $true) { Fail-Ws "topology-init: primary geometry incomplete" }
  if ([int]$g.gap -ne 0 -or [int]$g.outer_gap -ne 0) { Fail-Ws "topology-init: gaps not zero" }
  $nativeOut = "$($g.output)"
  $work = @([int]$g.work.x, [int]$g.work.y, [int]$g.work.w, [int]$g.work.h)
  $ring = @(Get-StressRing $ev $nativeOut)
  $ord = @{ home = (@($ring).IndexOf($ring[0]) + 1); away = (@($ring).IndexOf($ring[1]) + 1) }
  Rec-Ws "topology" @{ out = $nativeOut; work = ($work -join ","); home = $ring[0]; away = $ring[1] }
  return @{
    NativeOut = $nativeOut; HomeId = $ring[0]; AwayId = $ring[1]
    DiscoveredWork = $work; AliasOrd = $ord
    LastTick = [int]$ev.tick; Revision = [int]$ev.revision; Ring = $ring
  }
}

function Refresh-StressRing($ApplyEvent, $Topo) {
  # Re-resolve the ring from a fresh apply; the home/away head binding is
  # stable and any refresh fails loudly in Compare-StressCheckpoint.
  $ring = @(Get-StressRing $ApplyEvent $Topo.NativeOut)
  $Topo.Ring = $ring
  $Topo.AliasOrd = @{
    home = (@($ring).IndexOf($Topo.HomeId) + 1)
    away = (@($ring).IndexOf($Topo.AwayId) + 1)
  }
  if ([int]$Topo.AliasOrd.home -le 0 -or [int]$Topo.AliasOrd.away -le 0) {
    Fail-Ws "alias left ring (home $($Topo.HomeId), away $($Topo.AwayId))"
  }
  return $ring
}

function Invoke-StressCheckpoint($Action, $Case, $AuthoredWork, $Topo, $TokOf, $HwndOf, $FixtureState, [ref]$Mark, [ref]$PagerMark, $LogPath, $OwnerCopy) {
  $probe = Invoke-LiveNative $OwnerCopy @("ready") | ConvertFrom-Json
  if ($probe.ready -ne $true) { Fail-Ws "owner not ready at checkpoint $($Action.name)" }
  $tiles = Convert-StressTiles $Action.tiles $AuthoredWork $Topo.DiscoveredWork
  $managed = @{}
  $titles = Get-StressTitles
  foreach ($id in @($Action.members)) {
    $hwnd = Get-StressHwndOfCaller $titles "$id"
    if ($null -eq $hwnd) { Fail-Ws "checkpoint $($Action.name): no hwnd for $id" }
    if ("$hwnd" -ne "$($HwndOf[$id])") { Fail-Ws "checkpoint $($Action.name): hwnd changed for $id" }
    $managed[$id] = $hwnd
  }
  $expected = @{
    View = "$($Action.view)"; Members = @($Action.members); Homing = @{};
    Tiles = $tiles; Foreground = $null; PagerCurrent = "$($Action.pager_current)";
    PagerOccupied = @($Action.pager_occupied)
  }
  foreach ($p in $Action.homing.PSObject.Properties) { $expected.Homing[$p.Name] = "$($p.Value)" }
  if ($null -ne $Action.PSObject.Properties["foreground"] -and $null -ne $Action.foreground) {
    $expected.Foreground = "$($Action.foreground)"
  }
  $r = Wait-StressFreshApply $LogPath $Mark $Topo.LastTick 30 "checkpoint-$($Action.name)"
  $ev = $r.event
  $Topo.LastTick = [int]$ev.tick
  if ([int]$ev.revision -gt [int]$Topo.Revision) { $Topo.Revision = [int]$ev.revision }
  $ring = Refresh-StressRing $ev $Topo
  $desired = @{}
  foreach ($row in @($ev.desired_windows)) { $desired["$($row.window)"] = $row }
  $viewRow = @($ev.desired_outputs | Where-Object { "$($_.output)" -eq "$($Topo.NativeOut)" } | Select-Object -First 1)
  if ($viewRow.Count -ne 1) { Fail-Ws "checkpoint $($Action.name): no view row for $($Topo.NativeOut)" }
  $frames = @{}
  foreach ($id in @($Action.members)) {
    $pr = Read-LivePaired ([uint64]$managed[$id])
    if (-not $pr.alive) { Fail-Ws "checkpoint $($Action.name): $id not alive" }
    if (-not $pr.ok_outer) { Fail-Ws "checkpoint $($Action.name): $id outer unreadable" }
    $shownHere = ("$($expected.Homing[$id])" -eq "$($expected.View)")
    if ($shownHere -and -not $pr.ok_visible) {
      Fail-Ws "checkpoint $($Action.name): $id visible unreadable while shown"
    }
    $frames[$id] = @{ outer = $pr.outer; visible = $pr.visible; flag = $pr.visible_flag; alive = $pr.alive; ok_outer = $pr.ok_outer; ok_visible = $pr.ok_visible }
  }
  $fg = [StressNative]::GetForegroundWindow()
  $fgHwnd = [uint64]$fg.ToInt64()
  $fp = 0u
  $null = [StressNative]::GetWindowThreadProcessId($fg, [ref]$fp)
  if ($null -ne $expected.Foreground) {
    if ($fgHwnd -ne [uint64]$managed[$expected.Foreground]) { Fail-Ws "checkpoint $($Action.name): foreground mismatch" }
    if ([int]$fp -ne [int]$FixtureState.pid) { Fail-Ws "checkpoint $($Action.name): foreground pid mismatch" }
  }
  $pager = Wait-StressPager $LogPath $PagerMark $Topo.Revision $Topo.NativeOut 15 "$($Action.name)"
  $actual = @{
    NativeOut = $Topo.NativeOut; HomeId = $Topo.HomeId; AwayId = $Topo.AwayId; Ring = $ring;
    Tokens = $TokOf; Desired = $desired; DesiredView = "$($viewRow[0].logical_view)"; Frames = $frames;
    ForegroundHwnd = $fgHwnd; ManagedHwnds = $managed; Pager = $pager.entry;
    PagerOutputsTotal = $pager.outputs_total; PagerOutputsOmitted = $pager.outputs_omitted;
    Revision = $Topo.Revision
  }
  $null = Compare-StressCheckpoint $expected $actual
  Rec-Ws "checkpoint" @{ name = "$($Action.name)"; revision = $Topo.Revision }
}

function Wait-StressPager([string]$LogPath, [ref]$PagerMark, [int]$Revision, [string]$NativeOut, [int]$TimeoutSec, [string]$Tag) {
  # Renderer payload join: the latest pager-cells line at this revision
  # carrying exactly one entry for the native output token. ws tokens are
  # core ids and join the ring; omissions are validated by the caller.
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    $events = @(Read-LiveOwnerLog $LogPath)
    $best = $null
    $bestIdx = -1
    for ($i = $PagerMark.Value; $i -lt $events.Count; $i++) {
      $e = $events[$i]
      if ("$($e.event)" -ne "pager-cells") { continue }
      if ([int]$e.revision -ne $Revision) { continue }
      $hits = @(@($e.outputs) | Where-Object { "$($_.output)" -eq "$NativeOut" })
      if ($hits.Count -eq 0) { continue }
      if ($hits.Count -gt 1) { Fail-Ws "pager $Tag ambiguous entries for $NativeOut" }
      $best = @{
        output = "$($hits[0].output)"; revision = $Revision
        cells = @($hits[0].cells); cells_total = [int]$hits[0].cells_total
        cells_omitted = [int]$hits[0].cells_omitted; active = [int]$hits[0].active
      }
      $bestOutTotal = $e.outputs_total
      $bestOutOmitted = $e.outputs_omitted
      $bestIdx = $i
    }
    if ($null -ne $best) {
      $PagerMark.Value = $bestIdx + 1
      return @{ entry = $best; outputs_total = $bestOutTotal; outputs_omitted = $bestOutOmitted }
    }
    Start-Sleep -Milliseconds 500
  }
  Fail-Ws "pager $Tag join timeout at revision $Revision"
  return $null
}

function Invoke-StressCleanup($Ctx, [string]$Tag) {
  # Abort-safe recovery: every phase runs inside its own try so an earlier
  # failure never skips a later one. Missing bindings (startup failure)
  # record as failures, never silent skips. Failures aggregate and throw
  # nonzero once at the end. Never broad-kills: only exact-PID/creation
  # bound processes are signaled, and only the exact owner payload is
  # stopped/restored.
  $fails = [System.Collections.ArrayList]@()
  try {
    if ($null -eq $Ctx.OwnerPayload -or $null -eq $Ctx.Owner) { throw "no bound owner (startup never launched)" }
    $st = Invoke-LiveNative $Ctx.OwnerPayload @("stop") | ConvertFrom-Json
    if (-not $st.owner_exited) { throw "owner stop no exit" }
  } catch { $null = $fails.Add("owner-stop: $($_.Exception.Message)") }
  try {
    if ($null -eq $Ctx.FixtureExe -or $null -eq $Ctx.Socket -or $null -eq $Ctx.Nonce) {
      throw "no bound fixture endpoint (startup never announced)"
    }
    $null = Send-StressFixture $Ctx.FixtureExe $Ctx.Socket $Ctx.Nonce (@{ v = 1; request_id = "bye"; op = "shutdown" } | ConvertTo-Json -Compress)
  } catch { $null = $fails.Add("fixture-shutdown: $($_.Exception.Message)") }
  try {
    if ($null -eq $Ctx.FixtureProc) { throw "no bound fixture process" }
    $gone = $false
    try { $null = Get-Process -Id ([int]$Ctx.FixtureProc.Id) -ErrorAction Stop }
    catch {
      if ("$($_.FullyQualifiedErrorId)" -like "NoProcessFound*") { $gone = $true } else { throw }
    }
    if (-not $gone) {
      $hex = "{0:x16}" -f (Get-Process -Id ([int]$Ctx.FixtureProc.Id) -ErrorAction Stop).StartTime.ToUniversalTime().ToFileTimeUtc()
      if ($hex -ne "$($Ctx.FixtureCreation)") { throw "fixture PID reused by another process; will not kill" }
      $Ctx.FixtureProc.Kill()
      $gone = $false
      try { $null = Get-Process -Id ([int]$Ctx.FixtureProc.Id) -ErrorAction Stop }
      catch {
        if ("$($_.FullyQualifiedErrorId)" -like "NoProcessFound*") { $gone = $true } else { throw }
      }
      if (-not $gone) { throw "fixture process still alive after exact kill" }
    }
  } catch { $null = $fails.Add("fixture-exit: $($_.Exception.Message)") }
  try {
    if ($null -eq $Ctx.OwnerPayload) { throw "no bound owner payload" }
    $r = Invoke-LiveNative $Ctx.OwnerPayload @("restore") | ConvertFrom-Json
    if (-not $r.restored) { throw "restore failed" }
  } catch { $null = $fails.Add("owner-restore: $($_.Exception.Message)") }
  try {
    if ($null -eq $Ctx.Owner) { throw "no bound owner identity" }
    $rr = (Invoke-LiveNative $Ctx.OwnerPayload @("ready") | ConvertFrom-Json)
    if ($rr.ready -eq $true) { throw "owner still ready after stop" }
    throw "ready true-check bypassed without process proof"
  } catch {
    $msg = "$($_.Exception.Message)"
    if ($msg -like "owner still ready after stop*") {
      $null = $fails.Add("owner-ready: $msg")
    } else {
      $gone = $false
      try {
        $proc = Get-Process -Id ([int]$Ctx.Owner.pid) -ErrorAction Stop
        $hex = "{0:x16}" -f $proc.StartTime.ToUniversalTime().ToFileTimeUtc()
        if ($hex -ne "$($Ctx.Owner.process_creation)") { $gone = $true }
      } catch {
        if ("$($_.FullyQualifiedErrorId)" -like "NoProcessFound*") { $gone = $true }
        else { $null = $fails.Add("owner-ready: $msg") }
      }
      if (-not $gone) { $null = $fails.Add("owner-ready: owner process still alive") }
    }
  }
  try {
    if ($null -eq $Ctx.SettingsPath -or $null -eq $Ctx.GroupOrig) { throw "no settings preimage" }
    Copy-Item $Ctx.GroupOrig.path $Ctx.SettingsPath -Force
    $liveH = Get-LiveBytesSha ([IO.File]::ReadAllBytes($Ctx.SettingsPath))
    if ($liveH -ne $Ctx.GroupOrig.sha) { throw "settings differ from preimage after restore" }
  } catch { $null = $fails.Add("settings-restore: $($_.Exception.Message)") }
  try {
    if ($null -eq $Ctx.GroupOrig -or $null -eq $Ctx.GroupOrig.spi_path) { throw "no SPI preimage" }
    $spiWant = (Get-Content -LiteralPath $Ctx.GroupOrig.spi_path -Raw) | ConvertFrom-Json
    Assert-LiveSpiFields $spiWant "final stored"
    $spiNow = Get-LiveTaskbarSpi
    Assert-LiveSpiFields $spiNow "final current"
    if ([int]$spiNow.arranging -ne [int]$spiWant.arranging -or
        [int]$spiNow.pen -ne [int]$spiWant.pen -or
        "$($spiNow.abm)" -ne "$($spiWant.abm)") { throw "taskbar/SPI differ from preimage" }
  } catch { $null = $fails.Add("taskbar-spi: $($_.Exception.Message)") }
  try {
    foreach ($exe in @($Ctx.OwnerPayload, $Ctx.FixtureExe)) {
      if ($null -eq $exe) { continue }
      $base = Split-Path -Leaf $exe
      $procs = @(Get-CimInstance Win32_Process -Filter "Name='$base'" -ErrorAction Stop |
        Where-Object { "$($_.ExecutablePath)" -ieq "$exe" })
      if ($procs.Count -gt 0) { throw "owned residue: $exe x$($procs.Count)" }
    }
  } catch { $null = $fails.Add("process-residue: $($_.Exception.Message)") }
  try {
    if ($null -eq $Ctx.LedgerDir) { throw "no ledger directory bound" }
    $led = Join-Path $Ctx.LedgerDir "ledger.json"
    if (Test-Path -LiteralPath $led) { throw "ledger present" }
  } catch { $null = $fails.Add("ledger: $($_.Exception.Message)") }
  if ($fails.Count -gt 0) { throw "final ($Tag): $(($fails | ForEach-Object { "$_" }) -join '; ')" }
  Write-Output "$Tag final cleanup ok"
}

function Invoke-Mock {
  $case = Get-StressCase $Case
  Rec-Ws "case" @{ name = $case.name; actions = @($case.actions).Count }
  $n = Validate-StressCase $case
  Rec-Ws "contract" @{ commands = $n }
  $aliasOrd = @{ home = 1; away = 2 }
  $live = [System.Collections.ArrayList]@()
  foreach ($p in $case.initial_facts.PSObject.Properties) { $null = $live.Add($p.Name) }
  foreach ($a in @($case.actions)) {
    switch ($a.op) {
      { $_ -in @("select-workspace", "select-numbered", "select-relative", "select-previous", "send", "focus-directional", "move-directional") } {
        $argv = Get-StressCliArgs $a $aliasOrd
        if ($argv.Count -eq 0) { Fail-Ws "empty argv for $($a.op)" }
        Rec-Ws "argv" @{ op = $a.op; argv = ($argv -join " ") }
      }
      "admit" { $null = $live.Add($a.window) }
      "close" { $null = $live.Remove($a.window) }
      "focus-window" {
        if ($live -notcontains $a.window) { Fail-Ws "focus-window $($a.window) not live" }
      }
      "checkpoint" { }
      default { Fail-Ws "unknown op $($a.op)" }
    }
  }
  $tiles = @{ 'win-a' = @(400, 0, 400, 560) }
  $norm = Convert-StressTiles $tiles @(0, 0, 800, 560) @(0, 0, 800, 560)
  if ($norm['win-a'][2] -ne 400) { Fail-Ws "normalize identity broke" }
  $scaled = Convert-StressTiles $tiles @(0, 0, 800, 560) @(0, 0, 1600, 1120)
  if ($scaled['win-a'][2] -ne 800) { Fail-Ws "normalize scale broke" }
  try { $null = Convert-StressTiles $tiles @(0, 0, 800, 560) @(0, 0, 801, 560); Fail-Ws "non-integral scale accepted" } catch {}
  $report = @{ status = "pass"; stage = "WorldStressMock"; steps = $WS_STEPS }
  $report | ConvertTo-Json -Depth 8 | Write-Output
}

function Invoke-LiveJourney {
  Fail-Ws "refuse: world-stress native leg blocked on recovery and evidence defects; no live effects (see docs/changes/platform-neutral-stress-acceptance.md)"
  if ($DelayMs -lt 0 -or $DelayMs -gt 60000) { Fail-Ws "refuse: DelayMs must be 0..60000" }
  if ($OwnerSeconds -lt 60 -or $OwnerSeconds -gt 3600) { Fail-Ws "refuse: OwnerSeconds must be 60..3600" }
  $case = Get-StressCase $Case
  $cmdTotal = Validate-StressCase $case
  $targetDir = Get-StressTargetDir
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $runDir = Join-Path $targetDir "windows-world-stress\$stamp-$PID"
  New-Item -ItemType Directory -Force -Path $runDir | Out-Null
  $reportPath = Join-Path $runDir "world-stress-report.json"
  if (Test-Path $reportPath) { Fail-Ws "report preexists, will not overwrite" }
  $binDir = Join-Path $runDir "bin"
  New-Item -ItemType Directory -Force -Path $binDir | Out-Null
  $ctx = @{
    OwnerPayload = $null; Owner = $null
    FixtureExe = $null; FixtureProc = $null; FixtureCreation = ""
    Socket = $null; Nonce = $null
    SettingsPath = $null; GroupOrig = $null; LedgerDir = $null
  }
  $cleaned = $false
  try {
    & cargo build --locked --manifest-path (Join-Path $Repo "Cargo.toml") -p tiler-windows -p tiler-live-fixture
    if ($LASTEXITCODE -ne 0) { Fail-Ws "cargo build failed" }
    $ownerCopy = Join-Path $binDir "tiler-windows.exe"
    $fixtureCopy = Join-Path $binDir "tiler-live-fixture.exe"
    Copy-Item (Join-Path $targetDir "debug\tiler-windows.exe") $ownerCopy -Force
    Copy-Item (Join-Path $targetDir "debug\tiler-live-fixture.exe") $fixtureCopy -Force
    $ctx.OwnerPayload = $ownerCopy
    $ctx.FixtureExe = $fixtureCopy
    $commit = (& git -C $Repo rev-parse HEAD | Out-String).Trim()
    $status = (& git -C $Repo status --short | Out-String).Trim()
    # Baseline (untracked report only, never committed): source identity,
    # artifact hashes, display, actor session/integrity/identity, settings.
    $display = Get-StressDisplayBaseline
    $actorIdent = (& $ownerCopy @("identity") | ConvertFrom-Json)
    $settingsPath = Join-Path $env:LOCALAPPDATA "omnitiler\settings.json"
    $ctx.SettingsPath = $settingsPath
    $groupOrig = Save-LiveSettingsPreimage $settingsPath (Join-Path $runDir "settings-orig.json") (Join-Path $runDir "taskbar-spi.json") ""
    $ctx.GroupOrig = $groupOrig
    $prov = @{
      commit = $commit; status = $status; case = $case.name; commands = $cmdTotal
      actor_pid = $PID; actor_exe = (Get-Process -Id $PID).Path
      actor_identity = $actorIdent
      display = $display
      settings_sha = $groupOrig.sha
      owner_sha256 = (Get-FileHash -LiteralPath $ownerCopy -Algorithm SHA256).Hash
      fixture_sha256 = (Get-FileHash -LiteralPath $fixtureCopy -Algorithm SHA256).Hash
      run_dir = $runDir; delay_ms = $DelayMs; target_dir = $targetDir
    }
    Rec-Ws "env" $prov
    $caseChanged = Set-StressCaseSettings $settingsPath $case
    $ledgerDir = Assert-NoPreexistingStressOwner $ownerCopy "world-stress"
    $ctx.LedgerDir = $ledgerDir
    $started = Start-StressFixture $fixtureCopy
    $ctx.FixtureProc = $started.proc
    $ann = $started.announcement
    $sock = "$($ann.socket)"; $nonce = "$($ann.nonce)"
    $ctx.Socket = $sock
    $ctx.Nonce = $nonce
    $serverPid = [int]$ann.pid
    $serverProc = Get-Process -Id $serverPid -ErrorAction Stop
    $serverCreation = "{0:x16}" -f $serverProc.StartTime.ToUniversalTime().ToFileTimeUtc()
    $ctx.FixtureCreation = $serverCreation
    Rec-Ws "fixture-announced" @{ pid = $serverPid }
    $authoredWork = @($case.screens[0].work)
    $staged = Get-StressDisplayBaseline
    $boot = Start-StressBootstrap $fixtureCopy $sock $nonce $case $authoredWork $staged.work
    @{ socket = $sock; nonce = $nonce; owner = $ownerCopy; owner_seconds = $OwnerSeconds } |
      ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $runDir "leg.json") -Encoding utf8
    Start-ExplorerGui $ownerCopy "tile --user-start --seconds $OwnerSeconds --trace --scope-exe tiler-live-fixture.exe --inner-gap 0 --outer-gap 0" $binDir
    $ready = $null
    $deadline = (Get-Date).AddSeconds(30)
    while ((Get-Date) -lt $deadline) {
      try {
        $j = Invoke-LiveNative $ownerCopy @("ready") | ConvertFrom-Json
        if ($j.ready -eq $true) { $ready = $j; break }
      } catch {}
      Start-Sleep -Milliseconds 500
    }
    if ($null -eq $ready) { Fail-Ws "owner ready timeout" }
    if (-not (Test-ExeEqual "$($ready.owner.exe_path)" "$ownerCopy")) { Fail-Ws "owner peer mismatch" }
    Assert-ParentIsExplorer ([int]$ready.owner.pid)
    $ownerBind = @{
      pid = [int]$ready.owner.pid
      process_creation = "$($ready.owner.process_creation)"
      exe_path = "$($ready.owner.exe_path)"
    }
    $ctx.Owner = $ownerBind
    @{ socket = $sock; nonce = $nonce; owner = $ownerCopy; owner_seconds = $OwnerSeconds;
       owner_pid = $ownerBind.pid; owner_creation = $ownerBind.process_creation;
       fixture_pid = $serverPid; fixture_creation = $serverCreation;
       ledger_dir = $ledgerDir; settings_path = $settingsPath;
       settings_orig = $groupOrig.path; spi_path = $groupOrig.spi_path } |
      ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $runDir "leg.json") -Encoding utf8
    $logPath = "$($ready.log_path)"
    Rec-Ws "owner-ready" @{ pid = $ownerBind.pid; log = $logPath; settings_changed = @($caseChanged) }
    if ($DelayMs -gt 0) { Start-Sleep -Milliseconds $DelayMs }
    $mark = 0
    $pagerMark = 0
    $tokOf = @{}
    $hwndOf = @{}
    foreach ($id in $boot.ids) {
      $hwnd = Wait-StressHwnd "$id" @{ pid = $serverPid; creation = $serverCreation } 30
      $hwndOf[$id] = $hwnd
    }
    foreach ($id in $boot.ids) {
      $tok = Wait-StressAdmitted $logPath $tokOf $hwndOf[$id] @{ pid = $serverPid; creation = $serverCreation } 30 "$id"
      $tokOf[$id] = $tok
      Rec-Ws "admit" @{ id = $id; hwnd = "$($hwndOf[$id])"; token = $tok }
    }
    $topo = Initialize-StressTopology $logPath ([ref]$mark) $case $tokOf $boot.ids $authoredWork
    # Pager scan starts at zero: the matching line for the init revision may
    # precede the init row in the log; revisions only grow so the latest
    # match stays correct while checkpoints advance monotonically.
    $pagerMark = 0
    foreach ($action in @($case.actions)) {
      if ($DelayMs -gt 0) { Start-Sleep -Milliseconds $DelayMs }
      switch ($action.op) {
        "admit" {
          $id = "$($action.window)"
          $rect = @($action.rect)
          $body = @{ v = 1; request_id = (New-StressReqId "spawn-$id"); op = "spawn"; windows = @(@{
              id = $id; size_px = @([int]$rect[2], [int]$rect[3]);
              position_outer_px = @([int]$rect[0], [int]$rect[1]); decorations = $false }) } |
            ConvertTo-Json -Depth 6 -Compress
          $r = Send-StressFixture $fixtureCopy $sock $nonce $body
          $entry = @($r.result.windows | Where-Object { "$($_.id)" -eq $id })
          if ($entry.Count -ne 1 -or $entry[0].ok -ne $true) { Fail-Ws "spawn $id refused: $($r | ConvertTo-Json -Depth 4 -Compress)" }
          $hwnd = Wait-StressHwnd $id @{ pid = $serverPid; creation = $serverCreation } 30
          $hwndOf[$id] = $hwnd
          $tok = Wait-StressAdmitted $logPath $tokOf $hwnd @{ pid = $serverPid; creation = $serverCreation } 30 $id
          $tokOf[$id] = $tok
          Rec-Ws "admit" @{ id = $id; hwnd = "$hwnd"; token = $tok }
        }
        "close" {
          $id = "$($action.window)"
          $r = Send-StressFixture $fixtureCopy $sock $nonce (@{ v = 1; request_id = (New-StressReqId "close-$id"); op = "close"; ids = @($id) } | ConvertTo-Json -Depth 6 -Compress)
          $entry = @($r.result.windows | Where-Object { "$($_.id)" -eq $id })
          if ($entry.Count -ne 1 -or $entry[0].ok -ne $true) { Fail-Ws "close $id refused" }
          $topo.LastTick = Wait-StressDeparted $logPath ([ref]$mark) $tokOf[$id] $id 30
          $tokOf.Remove($id)
          $hwndOf.Remove($id)
          Rec-Ws "close" @{ id = $id }
        }
        "focus-window" {
          $id = "$($action.window)"
          Invoke-StressFocus $fixtureCopy $sock $nonce $id $hwndOf @{ pid = $serverPid; creation = $serverCreation } 15
          Rec-Ws "focus" @{ id = $id }
        }
        "checkpoint" {
          Invoke-StressCheckpoint $action $case $authoredWork $topo $tokOf $hwndOf @{ pid = $serverPid; creation = $serverCreation } ([ref]$mark) ([ref]$pagerMark) $logPath $ownerCopy
        }
        default {
          $argv = Get-StressCliArgs $action $topo.AliasOrd
          $raw = Invoke-StressCli $ownerCopy $argv $ledgerDir "$($action.op)"
          $disp = Wait-StressDispatch $logPath ([ref]$mark) $argv 30 "$($action.op)"
          if ("$($disp.outcome)" -ne "ok") {
            Fail-Ws "$($action.op) dispatch outcome $($disp.outcome) at tick $($disp.tick): product refused/dropped, no repair"
          }
          $topo.LastTick = $disp.tick
          Rec-Ws "command" @{ op = $action.op; tick = $disp.tick; outcome = $disp.outcome }
        }
      }
    }
    $end = $case.final
    $finalAction = [pscustomobject]@{
      op = "checkpoint"; name = "final"; view = "$($end.view)"; members = @($end.members);
      homing = $end.homing; tiles = $end.tiles; focus = "$($end.focus)";
      foreground = $end.foreground; pager_current = "$($end.pager_current)";
      pager_occupied = @($end.pager_occupied)
    }
    Invoke-StressCheckpoint $finalAction $case $authoredWork $topo $tokOf $hwndOf @{ pid = $serverPid; creation = $serverCreation } ([ref]$mark) ([ref]$pagerMark) $logPath $ownerCopy
    Invoke-StressCleanup $ctx "world-stress"
    $cleaned = $true
    $report = @{ status = "pass"; stage = "WorldStress"; started = (Get-Date).ToString("o"); provenance = $prov; steps = $WS_STEPS }
    $report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $reportPath
    Write-Output "world-stress-report=$reportPath"
    Write-Output "status=pass"
  } catch {
    $report = @{ status = "fail"; stage = "WorldStress"; provenance = $prov; steps = $WS_STEPS; error = "$($_.Exception.Message)" }
    try { $report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $reportPath } catch {}
    Write-Output "world-stress-report=$reportPath"
    throw "WorldStress stage failed: $($_.Exception.Message)"
  } finally {
    if (-not $cleaned) {
      try { Invoke-StressCleanup $ctx "world-stress-abort" } catch {
        Write-Output "abort cleanup: $($_.Exception.Message)"
      }
    }
  }
}

if ($Mock) { Invoke-Mock; exit 0 }
if ($Stop) {
  if ($RunDir -eq "") { Fail-Ws "Stop requires -RunDir" }
  $leg = Get-Content -LiteralPath (Join-Path $RunDir "leg.json") -Raw | ConvertFrom-Json
  $ownerPayload = "$($leg.owner)"
  $sctx = @{
    OwnerPayload = $ownerPayload
    Owner = $null
    FixtureExe = $null; FixtureProc = $null; FixtureCreation = ""
    Socket = "$($leg.socket)"; Nonce = "$($leg.nonce)"
    SettingsPath = $null; GroupOrig = $null; LedgerDir = $null
  }
  if ($null -ne $leg.PSObject.Properties["owner_pid"] -and $null -ne $leg.PSObject.Properties["owner_creation"]) {
    $sctx.Owner = @{ pid = [int]$leg.owner_pid; process_creation = "$($leg.owner_creation)"; exe_path = "$ownerPayload" }
  }
  if ($null -ne $leg.PSObject.Properties["ledger_dir"]) { $sctx.LedgerDir = "$($leg.ledger_dir)" }
  if (($null -ne $leg.PSObject.Properties["settings_path"]) -and ($null -ne $leg.PSObject.Properties["settings_orig"])) {
    $sctx.SettingsPath = "$($leg.settings_path)"
    $sctx.GroupOrig = @{ path = "$($leg.settings_orig)"; sha = ""; spi_path = "$($leg.spi_path)" }
    try {
      $ob = [IO.File]::ReadAllBytes("$($leg.settings_orig)")
      $sctx.GroupOrig.sha = (Get-LiveBytesSha $ob)
    } catch {}
  }
  $binOwner = Split-Path -Parent $ownerPayload
  $fx = Join-Path $binOwner "tiler-live-fixture.exe"
  if (Test-Path -LiteralPath $fx -PathType Leaf) {
    $sctx.FixtureExe = $fx
    if ($null -ne $leg.PSObject.Properties["fixture_pid"]) {
      try { $sctx.FixtureProc = Get-Process -Id ([int]$leg.fixture_pid) -ErrorAction Stop } catch {}
      if ($null -ne $leg.PSObject.Properties["fixture_creation"]) {
        $sctx.FixtureCreation = "$($leg.fixture_creation)"
      }
    }
  }
  try { Invoke-StressCleanup $sctx "world-stress-stop" } catch { Write-Output "stop cleanup: $($_.Exception.Message)" }
  exit 0
}
if ($Live) { Fail-Ws "refuse: world-stress native leg blocked on recovery and evidence defects; no live effects (see docs/changes/platform-neutral-stress-acceptance.md)" }
if ("$($MyInvocation.InvocationName)" -eq ".") { return }
Write-Output "windows-world-stress parsed (no action without -Mock/-Live/-Stop)"
