param(
  [switch]$Mock,
  [switch]$Live,
  [switch]$Stop,
  [string]$RunDir = "",
  [string]$Case = "",
  [int]$DelayMs = 0,
  [int]$OwnerSeconds = 600
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
# never treats it as fresh. Commands publish as a burst: the single-pending
# request-file transport serializes publication (availability wait only),
# with a cross-family idle fence before each publish (the owner polls
# workspace before directional every tick, so two pending files would
# dispatch out of definition order), and no per-command dispatch or native
# convergence wait. Ordered dispatch outcomes join later at explicit
# observation barriers, in queue order, over a dedicated dispatch cursor;
# the apply cursor is never consumed by dispatch joins, and a settled apply
# must cover the last dispatched tick. Native readbacks (fresh HWND/PID/
# creation validation every checkpoint) and full settlement happen only at
# explicit checkpoints: transient native/pager lag retries to the deadline
# while corruption, missing evidence and identity mismatches fail closed.
# Product refusals/drops surface as test failures, never
# silent repairs. Renderer payload (`pager-cells`) is change-gated: the
# retained latest actually emitted payload at or before the checkpoint
# revision is the evidence (keyed by revision and log index with
# tick/observation provenance), never a required same-revision row.
# Miniature geometry is projected independently from each expected tile
# through the exact renderer contract (pager::miniature) and checked per
# token plus per-cell multisets, alongside desired allocations and
# independent native frames.
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

function Test-StressHasField($Event, [string]$Name) {
  # Field presence across both evidence shapes: live owner-log rows arrive
  # as PSCustomObject (real JSON roundtrip), fault-injection rows may be
  # plain hashtables. Missing on either shape is a refusal, never a pass.
  if ($Event -is [hashtable]) { return $Event.ContainsKey($Name) }
  if ($null -ne $Event.PSObject -and $null -ne $Event.PSObject.Properties) {
    return ($null -ne $Event.PSObject.Properties[$Name])
  }
  return ($null -ne ($Event | Get-Member -Name $Name -MemberType Properties, NoteProperty))
}

function Get-StressField($Event, [string]$Name) {
  if ($Event -is [hashtable]) { return $Event[$Name] }
  return $Event.$Name
}

function Test-StressOmissionZero($Event, [string]$Tag, [string]$Field) {
  # Strict integer zero: missing, null, boolean, floating-point and numeric
  # strings all refuse. A bare cast would coerce $null/$false/"0" to 0 and
  # hide corrupt evidence. Live JSON integers arrive as int/long; either is
  # accepted, anything else fails closed.
  if (-not (Test-StressHasField $Event $Field)) { Fail-Ws "$Tag omission field missing: $Field" }
  $v = Get-StressField $Event $Field
  if ($null -eq $v) { Fail-Ws "$Tag omission null: $Field" }
  if (($v -isnot [int]) -and ($v -isnot [long])) { Fail-Ws "$Tag omission not integer: $Field" }
  if ([int64]$v -ne 0) { Fail-Ws "$Tag omitted $($Field)=$v" }
}

function Test-StressApplyClean($Event, [string]$Tag) {
  # Every known honest omission section of a world-apply row must be present
  # and integer zero; pending intent arrays must be empty; required desired
  # arrays must be present; geometry must be paired with a numeric zero
  # omission. Source: apply_diag_value in
  # crates/tiler-windows/src/tiling_sys/world_tick.rs. A missing field is a
  # refusal, never a pass.
  foreach ($f in @("members_omitted", "views_omitted", "frames_omitted", "clears_omitted",
      "clear_holds_omitted", "desired_operations_omitted", "operation_settled_omitted",
      "focus_effects_omitted", "focus_dispatch_omitted", "inflight_events_omitted",
      "foreground_meta_omitted", "visibility_posts_omitted", "desired_windows_omitted",
      "desired_outputs_omitted", "desired_workspaces_omitted", "held_scopes_omitted",
      "desired_transfers_omitted")) {
    Test-StressOmissionZero $Event $Tag $f
  }
  foreach ($a in @("desired_transfers", "desired_operations", "held_scopes",
      "desired_windows", "desired_outputs", "desired_workspaces", "desired_geometry")) {
    if (-not (Test-StressHasField $Event $a)) { Fail-Ws "$Tag desired array missing: $a" }
  }
  $gprop = Test-StressHasField $Event "desired_geometry_omitted"
  if (-not $gprop) { Fail-Ws "$Tag geometry omission missing/null" }
  $gv = Get-StressField $Event "desired_geometry_omitted"
  if ($null -eq $gv) { Fail-Ws "$Tag geometry omission missing/null" }
  if (($gv -isnot [int]) -and ($gv -isnot [long])) { Fail-Ws "$Tag geometry omission not integer" }
  if ([int64]$gv -ne 0) { Fail-Ws "$Tag geometry omitted $gv" }
  if ("$(Get-StressField $Event 'geometry_status')" -ne "paired") { Fail-Ws "$Tag geometry not paired" }
  if (@(Get-StressField $Event "desired_transfers").Count -gt 0) { Fail-Ws "$Tag pending transfers" }
  if (@(Get-StressField $Event "desired_operations").Count -gt 0) { Fail-Ws "$Tag pending operations" }
  if (@(Get-StressField $Event "held_scopes").Count -gt 0) { Fail-Ws "$Tag held scopes" }
  return $true
}

function Get-StressRowField($Row, [string]$Name) {
  # Desired-row field access across both evidence shapes: fault-injection
  # rows may be plain hashtables while live owner-log rows arrive as
  # PSCustomObject (bracket indexing a PSCustomObject yields $null, so the
  # live journey must use member access).
  if ($Row -is [hashtable]) { return $Row[$Name] }
  return $Row.$Name
}

function Compare-StressCheckpoint($Expected, $Actual) {
  # Pure evidence comparison: throws fail-closed on the first mismatch.
  # $Expected: View, Members (caller ids), Homing (id -> home|away),
  #   Tiles (id -> [x,y,w,h] widths), Foreground (id or $null),
  #   PagerCurrent (home|away), PagerOccupied (subset of home|away).
  # $Actual: NativeOut (wire o#), HomeId/AwayId (stable core ids),
  #   Ring (order-sorted core ids on NativeOut), Tokens (caller -> token),
  #   Desired (token -> accepted desired row), DesiredView, MonitorFull
  #   (full monitor rect [x,y,w,h] for miniature projection),
  #   Frames (caller -> outer/visible [l,t,r,b] + flag),
  #   ForegroundHwnd, ManagedHwnds, Pager (retained renderer entry with dpi),
  #   PagerOutputsTotal/Omitted, Revision.
  # Miniature geometry is projected independently from each expected tile
  # through the exact renderer contract (pager::miniature) and compared per
  # token plus per-cell token multisets: duplicates, omissions and swaps
  # fail, not just degenerate rects.
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
      $r = Get-StressRowField $row $f
      if ($null -eq $r) { Fail-Ws "$id $f missing" }
      if ([int]$r.x -ne [int]$tile[0] -or [int]$r.y -ne [int]$tile[1] -or
          [int]$r.w -ne [int]$tile[2] -or [int]$r.h -ne [int]$tile[3]) {
        Fail-Ws "$id $f [$($r.x),$($r.y),$($r.w),$($r.h)] != tile [$($tile -join ',')]"
      }
    }
    foreach ($h in @("frame_hold", "membership_hold")) {
      $hv = Get-StressRowField $row $h
      if ($null -ne $hv -and "$hv" -ne "") { Fail-Ws "$id $h present" }
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
  # the renderer: change-gated, so the retained latest actually emitted
  # payload at or before this checkpoint revision is the evidence, never a
  # required same-revision row and never fabricated freshness). ws tokens are
  # the core logical ids themselves (workspace_token is identity); only the
  # output token is a runtime wire id. Geometry and focus identity ride the
  # payload (cell/miniature rects plus per-miniature window tokens), never
  # pixels and never native HWND/PID/creation bytes.
  $entry = $Actual.Pager
  if ($null -eq $entry) { Fail-Ws "no matched pager entry" }
  if ([int]$entry.revision -gt [int]$Actual.Revision) {
    Fail-Ws "pager revision $($entry.revision) newer than checkpoint $($Actual.Revision)"
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
  $tokOfCaller = @{}
  foreach ($cid in @($Expected.Members)) { $tokOfCaller["$($Actual.Tokens[$cid])"] = "$cid" }
  $fgTok = $null
  if ($null -ne $Expected.Foreground) { $fgTok = "$($Actual.Tokens[$Expected.Foreground])" }
  if (-not (Test-StressHasField $entry "dpi")) { Fail-Ws "pager dpi missing" }
  $dpi = $entry.dpi
  if (($dpi -isnot [int]) -and ($dpi -isnot [long])) { Fail-Ws "pager dpi not integer" }
  if ([int]$dpi -le 0) { Fail-Ws "pager dpi not positive" }
  $monFull = $Actual.MonitorFull
  if ($null -eq $monFull -or @($monFull).Count -ne 4) { Fail-Ws "pager projection missing monitor full rect" }
  for ($i = 0; $i -lt $cells.Count; $i++) {
    $c = $cells[$i]
    if ([int]$c.ord -ne ($i + 1)) { Fail-Ws "pager ord out of ring order" }
    if ("$($c.ws)" -ne "$($ring[$i])") { Fail-Ws "pager ws $($c.ws) != ring $($ring[$i])" }
    if ($null -eq $c.rect -or $null -eq $c.rect.x) { Fail-Ws "pager cell rect missing at cell $i" }
    $cw = [int]$c.rect.w; $ch = [int]$c.rect.h
    if ($cw -le 0 -or $ch -le 0) { Fail-Ws "pager cell rect degenerate at cell $i" }
    if ([int]$c.minis_omitted -ne 0) { Fail-Ws "pager minis omitted at cell $i" }
    $marks = @($c.minis)
    $rects = @($c.mini_rects)
    $toks = @($c.mini_windows)
    if ([int]$c.minis_total -ne $marks.Count) { Fail-Ws "pager minis_total mismatch at cell $i" }
    if ($rects.Count -ne $marks.Count -or $toks.Count -ne $marks.Count) {
      Fail-Ws "pager mini parallel arrays mismatch at cell $i"
    }
    $trueCount = @($marks | Where-Object { $_ -eq $true }).Count
    if ([int]$c.focused_total -ne $trueCount) { Fail-Ws "pager focused_total mismatch at cell $i" }
    if ((($c.cur -eq $true)) -ne ($i -eq $curViewIdx)) { Fail-Ws "pager current mark wrong at cell $i" }
    if ($i -ge 2) {
      if ([int]$c.minis_total -ne 0) { Fail-Ws "trailing spare cell $i must stay empty" }
      if (($c.cur -eq $true) -or ($c.focused -eq $true)) { Fail-Ws "trailing spare marks must stay clear at cell $i" }
      continue
    }
    $occ = ([int]$c.minis_total -gt 0)
    if ($occ -ne ($wantOccIdx -contains $i)) { Fail-Ws "pager occupied mark wrong at cell $i" }
    if ((($c.focused -eq $true)) -ne ($trueCount -gt 0)) { Fail-Ws "pager focused mark mismatch at cell $i" }
    $cellAlias = if ($i -eq 0) { "home" } else { "away" }
    # Expected per-cell token multiset plus per-token projected geometry:
    # each member homed to this cell must appear exactly once with the
    # miniature rect the renderer contract projects from its expected tile.
    # Duplicates, omissions and swapped positions all fail here, not just
    # degenerate/escaping rects.
    $wantCellToks = @(@($Expected.Members | Where-Object { "$($Expected.Homing[$_])" -eq "$cellAlias" } | ForEach-Object { "$($Actual.Tokens[$_])" } | Sort-Object))
    $gotCellToks = @(@($toks | ForEach-Object { "$_" } | Sort-Object))
    if (($wantCellToks -join ",") -ne ($gotCellToks -join ",")) {
      Fail-Ws "pager mini token multiset wrong at cell $($cellAlias): $($gotCellToks -join ',') != $($wantCellToks -join ',')"
    }
    for ($m = 0; $m -lt $toks.Count; $m++) {
      $mt = "$($toks[$m])"
      if ($mt -eq "") { Fail-Ws "pager mini token missing at cell $i mini $m" }
      $caller = $tokOfCaller[$mt]
      if ($null -eq $caller) { Fail-Ws "pager mini token $mt unknown at cell $i" }
      $tile = $Expected.Tiles[$caller]
      if ($null -eq $tile) { Fail-Ws "no normalized tile for $caller" }
      $wantRect = Get-StressExpectedMini $c.rect $monFull $tile ([int]$dpi)
      $mr = $rects[$m]
      if ($null -eq $mr -or $null -eq $mr.x) { Fail-Ws "pager mini rect missing at cell $i mini $m" }
      if ([int]$mr.x -ne [int]$wantRect[0] -or [int]$mr.y -ne [int]$wantRect[1] -or
          [int]$mr.w -ne [int]$wantRect[2] -or [int]$mr.h -ne [int]$wantRect[3]) {
        Fail-Ws "pager miniature layout wrong at cell $i mini ${m} ($caller): [$($mr.x),$($mr.y),$($mr.w),$($mr.h)] != [$($wantRect -join ',')]"
      }
      $isFg = ($null -ne $fgTok -and $mt -eq "$fgTok")
      if ($isFg -ne ($marks[$m] -eq $true)) {
        Fail-Ws "pager mini focus identity wrong at cell $i mini $m"
      }
    }
  }
  if ($null -ne $Expected.Foreground) {
    $fgHome = $Expected.Homing[$Expected.Foreground] -eq "home"
    $fgIdx = @($ring).IndexOf($(if ($fgHome) { $Actual.HomeId } else { $Actual.AwayId }))
    if ($cells[$fgIdx].focused -ne $true) { Fail-Ws "pager focus mark wrong" }
    $fgCellToks = @($cells[$fgIdx].mini_windows)
    if (@($fgCellToks | Where-Object { "$_" -eq "$fgTok" }).Count -ne 1) {
      Fail-Ws "pager foreground token not exactly once in its cell"
    }
  } else {
    foreach ($c in $cells) {
      if ($c.focused -eq $true) { Fail-Ws "pager focused while null foreground expected" }
    }
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
    # Cleanup responsibility stays inside the spawner: the child handle is
    # exact (no PID guessing), so kill it here and wait bounded for exit.
    # The caller never receives a half-bound proc and must not re-kill.
    try { $proc.Kill() } catch {}
    $exitDl = (Get-Date).AddSeconds(10)
    while ((Get-Date) -lt $exitDl) {
      try { $null = Get-Process -Id $proc.Id -ErrorAction Stop; Start-Sleep -Milliseconds 250 }
      catch {
        if ("$($_.FullyQualifiedErrorId)" -like "NoProcessFound*") { break } else { break }
      }
    }
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
  # owner's process. Fail-closed on identity/ready query failure (any
  # nonzero exit or unreadable reply refuses) before any settings effect.
  # Both queries ride Invoke-LiveNative so offline fault injection can
  # drive the failure paths.
  try {
    $ledDir = ((Invoke-LiveNative $OwnerPayload @("identity") | ConvertFrom-Json).ledger_directory)
  } catch {
    Fail-Ws "$Tag refuse: owner identity query failed: $($_.Exception.Message)"
  }
  if ($null -eq $ledDir -or "$ledDir" -eq "") { Fail-Ws "$Tag refuse: no ledger directory" }
  $led = Join-Path "$ledDir" "ledger.json"
  if (Test-Path -LiteralPath $led -PathType Leaf) {
    Fail-Ws "$Tag refuse: existing owner ledger present at $led; stop/restore first (will not stop another owner)"
  }
  try {
    $j = Invoke-LiveNative $OwnerPayload @("ready") | ConvertFrom-Json
    if ($j.ready -eq $true) {
      Fail-Ws "$Tag refuse: existing owner ready (owner=$($j.owner | ConvertTo-Json -Compress))"
    }
  } catch {
    Fail-Ws "$Tag refuse: ready query failed without a ledger: $($_.Exception.Message)"
  }
  return "$ledDir"
}

function Read-StressOwnerLog([string]$LogPath) {
  # Concurrent-append tolerance for the live owner log: the owner appends
  # JSON lines while the leg polls, so a read can catch a partially written
  # trailing line (observed live: torn world-apply at byte 40). A malformed
  # snapshot retries boundedly — a torn tail completes within ms — while
  # persistent malformation (including a corrupt non-trailing line on the
  # final pass) still fails closed with the shared reader's shape. Never
  # fabricates or skips evidence: only a fully parseable snapshot returns.
  if (-not (Test-Path -LiteralPath $LogPath -PathType Leaf)) { return @() }
  $lastErr = $null
  for ($a = 0; $a -lt 20; $a++) {
    try {
      $out = @()
      foreach ($raw in (Get-Content -LiteralPath $LogPath)) {
        if ("$raw".Trim() -eq "") { continue }
        $out += ($raw | ConvertFrom-Json)
      }
      return $out
    } catch {
      $lastErr = $_
      Start-Sleep -Milliseconds 100
    }
  }
  throw "malformed JSON ${LogPath}: $($lastErr.Exception.Message)"
}

function Wait-StressLog([string]$LogPath, [int]$Mark, [scriptblock]$Pred, [int]$TimeoutSec, [string]$Tag) {
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    $events = @(Read-StressOwnerLog $LogPath)
    for ($i = $Mark; $i -lt $events.Count; $i++) {
      if (& $Pred $events[$i]) { return @{ event = $events[$i]; index = $i } }
    }
    Start-Sleep -Milliseconds 50
  }
  Fail-Ws "$Tag log join timeout"
  return $null
}

function Test-StressApplyPending($Event) {
  # Transient settlement state only: published pending intent or per-window
  # pending holds/ops that a later fresh row may settle. Corruption
  # (omissions, geometry, missing fields) is never pending: it fails fast in
  # Test-StressApplyClean. Returns $true when the row should be skipped until
  # the checkpoint deadline, $false when settled.
  if (@($Event.desired_transfers).Count -gt 0) { return $true }
  if (@($Event.desired_operations).Count -gt 0) { return $true }
  if (@($Event.held_scopes).Count -gt 0) { return $true }
  foreach ($row in @($Event.desired_windows)) {
    if ($null -eq $row) { continue }
    if ("$($row.frame_op)" -ne "" -or "$($row.clear_op)" -ne "") { return $true }
    if ($null -ne $row.frame_hold -and "$($row.frame_hold)" -ne "") { return $true }
    if ($null -ne $row.membership_hold -and "$($row.membership_hold)" -ne "") { return $true }
    if ($row.floating -eq $true) { return $true }
    if ($null -ne $row.float_origin -and "$($row.float_origin)" -ne "") { return $true }
    if ("$($row.frame_authority)" -ne "controlled" -or "$($row.membership_authority)" -ne "controlled") { return $true }
  }
  return $false
}

function Get-StressDpiScale([int]$Px, [int]$Dpi) {
  # Exact port of pager::dpi_scale: ((px * dpi + 48) / 96), clamped 1..16384.
  # Gaps and 1px borders stay physical by construction and never pass here.
  $d = [int64]$Dpi
  if ($d -lt 1) { $d = 1 }
  $q = [Math]::Floor(([int64]$Px * $d + 48) / 96)
  if ($q -lt 1) { return 1 }
  if ($q -gt 16384) { return 16384 }
  return [int]$q
}

function Get-StressExpectedMini($CellRect, $MonitorFull, $WindowRect, [int]$Dpi) {
  # Exact port of pager::miniature: the renderer's scaling/rounding/inset
  # contract. $CellRect is {x,y,w,h} widths from the pager-cells payload,
  # $MonitorFull the full monitor rect [x,y,w,h], $WindowRect the expected
  # tile [x,y,w,h] widths. Returns [x,y,w,h] widths of the expected
  # miniature. Degenerate inputs clamp to a 2px physical minimum inside the
  # inset, exactly like the renderer. All four round sites use
  # MidpointRounding.AwayFromZero: Rust f64::round rounds half away from
  # zero while .NET default rounds half to even.
  $pad = Get-StressDpiScale 4 $Dpi
  $ix = [int]$CellRect.x + $pad
  $iy = [int]$CellRect.y + $pad
  $iw = [int]$CellRect.w - 2 * $pad
  $ih = [int]$CellRect.h - 3 * $pad
  if ($iw -lt 1) { $iw = 1 }
  if ($ih -lt 1) { $ih = 1 }
  $mw = [int]$MonitorFull[2]; $mh = [int]$MonitorFull[3]
  $ww = [int]$WindowRect[2]; $wh = [int]$WindowRect[3]
  if ($mw -le 0 -or $mh -le 0 -or $ww -le 0 -or $wh -le 0) {
    return @($ix, $iy, 2, 2)
  }
  $scale = [Math]::Min([double]$iw / [double]$mw, [double]$ih / [double]$mh)
  if ($scale -lt 0.0) { $scale = 0.0 }
  if ($scale -gt 1.0) { $scale = 1.0 }
  $w = [int][Math]::Round([double]$ww * $scale, [System.MidpointRounding]::AwayFromZero)
  $h = [int][Math]::Round([double]$wh * $scale, [System.MidpointRounding]::AwayFromZero)
  if ($w -lt 2) { $w = 2 }
  if ($h -lt 2) { $h = 2 }
  if ($w -gt $iw) { $w = $iw }
  if ($h -gt $ih) { $h = $ih }
  $x = [int][Math]::Round([double]$ix + ([double]([int]$WindowRect[0] - [int]$MonitorFull[0])) * $scale, [System.MidpointRounding]::AwayFromZero)
  $y = [int][Math]::Round([double]$iy + ([double]([int]$WindowRect[1] - [int]$MonitorFull[1])) * $scale, [System.MidpointRounding]::AwayFromZero)
  $maxX = $ix + $iw - $w
  $maxY = $iy + $ih - $h
  if ($x -lt $ix) { $x = $ix }
  if ($x -gt $maxX) { $x = $maxX }
  if ($y -lt $iy) { $y = $iy }
  if ($y -gt $maxY) { $y = $maxY }
  return @($x, $y, $w, $h)
}

function Test-StressTransientMismatch([string]$Msg) {
  # Precise checkpoint retry classifier under exactly one outer checkpoint
  # deadline. Only intended native/pager lag retries against settled
  # evidence: native actuation lag (frame values, visibility flags,
  # foreground HWND handoff) and change-gated pager emission lag (marks,
  # layout, identity, multiset, late payload join). Every consumed
  # transport/dispatch/settlement bound fails closed with no retry:
  # "settlement timeout" (Wait-StressFreshApply's own bound),
  # "log join timeout", "dispatch-order timeout" and refused dispatch
  # outcomes (Wait-StressDispatchOrdered's bound), and "transport cannot
  # preserve burst order" (Wait-StressTransportIdle's bound). Pending intent
  # observed inside Wait-StressFreshApply is skipped there until its own
  # bounded settlement; no generic "pending" pattern is transient here, so a
  # fatal pending-worded mismatch can never be masked. Unknown messages fail
  # closed.
  if ($Msg -match " outer \[.*\] != tile \[") { return $true }
  if ($Msg -match " visible \[.*\] != outer \[") { return $true }
  if ($Msg -match "not visible while shown|visible while hidden") { return $true }
  if ($Msg -match "^foreground \d+ != \d+$") { return $true }
  if ($Msg -match "is managed while null expected") { return $true }
  if ($Msg -match "join timeout at or before revision") { return $true }
  # Ring/count mismatch against a retained older payload: the settled ring
  # moved past the latest emitted payload (change-gated pager lags the apply
  # by a revision). Retry re-settles to a newer row (see checkpoint catch),
  # never re-polls under the same revision cap.
  if ($Msg -match "^pager cells \d+ != ring \d+$") { return $true }
  if ($Msg -match "pager active \d+ != \d+") { return $true }
  if ($Msg -match "pager (current mark|occupied mark|focus mark) wrong") { return $true }
  if ($Msg -match "pager focused mark mismatch") { return $true }
  if ($Msg -match "pager focused while null foreground expected") { return $true }
  if ($Msg -match "pager miniature layout wrong") { return $true }
  if ($Msg -match "pager mini token multiset wrong") { return $true }
  if ($Msg -match "pager mini focus identity wrong") { return $true }
  if ($Msg -match "pager mini (token|rect) missing") { return $true }
  return $false
}

function Get-StressProcCreationHex($Proc) {
  return ("{0:x16}" -f $Proc.StartTime.ToUniversalTime().ToFileTimeUtc())
}

function Test-StressFixtureProcess([int]$ProcId, $FixtureState, [string]$Tag) {
  # Fresh process-lifetime revalidation: the PID must still exist and its
  # creation stamp must equal the fixture birth binding. A gone process or a
  # reused PID (same number, new creation) fails closed: never authorize a
  # recycled handle.
  try {
    $proc = Get-Process -Id $ProcId -ErrorAction Stop
  } catch {
    Fail-Ws "$Tag fixture process gone for pid $ProcId"
    return $false
  }
  $hex = Get-StressProcCreationHex $proc
  if ($hex -ne "$($FixtureState.creation)") {
    Fail-Ws "$Tag fixture process creation mismatch (PID reused): $hex != $($FixtureState.creation)"
  }
  return $true
}

function Test-StressHwndOwner([uint64]$Hwnd, $FixtureState, [string]$Tag) {
  # Fresh HWND ownership revalidation: the window must currently belong to
  # the fixture PID, and that PID must still carry the bound creation stamp.
  $pidOut = 0u
  $null = [LivePaired]::GetWindowThreadProcessId([IntPtr][long]$Hwnd, [ref]$pidOut)
  if ([int]$pidOut -ne [int]$FixtureState.pid) {
    Fail-Ws "$Tag hwnd $Hwnd owned by pid $pidOut, not fixture $($FixtureState.pid)"
  }
  $null = Test-StressFixtureProcess ([int]$pidOut) $FixtureState $Tag
  return $true
}

function Drain-StressPending($LogPath, $Topo, [int]$TimeoutSec, [string]$Tag) {
  # Causal delivery barrier for explicit observations (admit/close/focus/
  # checkpoint): join every queued burst command in queue order against the
  # dedicated dispatch cursor, then clear the queue. Applies no native
  # convergence wait; a refused outcome fails loudly here, never silently.
  if ($null -eq $Topo.Pending) { return @() }
  if (@($Topo.Pending).Count -eq 0) { return @() }
  if ($null -eq $Topo.DispatchMark) { $Topo.DispatchMark = 0 }
  $ref = [ref]$Topo.DispatchMark
  $outs = Wait-StressDispatchOrdered $LogPath $ref $Topo.Pending $TimeoutSec $Tag
  $Topo.DispatchMark = $ref.Value
  $Topo.Pending.Clear()
  # Fence high-water mark: an intervening observation drain joins consumption
  # and clears the queue, but the checkpoint must still settle at or past the
  # consumed dispatch tick, never on a pre-actuation row. Missing key reads 0.
  $maxTick = 0
  foreach ($o in @($outs)) { if ([int]$o.tick -gt $maxTick) { $maxTick = [int]$o.tick } }
  if ($maxTick -gt [int]$Topo.LastDispatchTick) { $Topo.LastDispatchTick = $maxTick }
  return $outs
}

function Wait-StressTransportIdle([string]$LedgerDir, [string]$Tag, [int]$TimeoutSec = 15) {
  # Cross-family publication fence: the owner polls workspace before
  # directional every tick (run_loop_tick.rs), so two simultaneously pending
  # request files would dispatch workspace-first regardless of definition
  # order. Wait until BOTH single-pending files are consumed before
  # publishing the next command, preserving definition order with transport
  # availability only and zero native convergence wait. Bounded: expiry fails
  # with evidence instead of claiming an unpaced burst the transport cannot
  # honor.
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    $ws = Join-Path $LedgerDir "workspace.request"
    $di = Join-Path $LedgerDir "directional.request"
    $wsBusy = Test-Path -LiteralPath $ws -PathType Leaf
    $diBusy = Test-Path -LiteralPath $di -PathType Leaf
    if (-not $wsBusy -and -not $diBusy) { return }
    Start-Sleep -Milliseconds 50
  }
  Fail-Ws "$Tag transport cannot preserve burst order (request files still pending)"
}

function Wait-StressFreshApply([string]$LogPath, [ref]$Mark, [int]$MinTick, [int]$TimeoutSec, [string]$Tag) {
  # Freshness gate: `accepted` and `command-consumed` are fresh observations
  # (the latter never re-executes its already-consumed command); `duplicate`
  # repeats the cached reply with no new revision and is never fresh. Rows
  # with transient pending intent are skipped until the deadline; corruption
  # (nonzero/missing omissions, unpaired geometry) fails fast via
  # Test-StressApplyClean. Never fabricates settlement: deadline expiry fails.
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  $cursor = [int]$Mark.Value
  $pendingSeen = 0
  while ((Get-Date) -lt $deadline) {
    $events = @(Read-StressOwnerLog $LogPath)
    for ($i = $cursor; $i -lt $events.Count; $i++) {
      $e = $events[$i]
      if ("$($e.event)" -ne "world-apply") { continue }
      if ("$($e.delivery)" -eq "duplicate") { $cursor = $i + 1; continue }
      if ("$($e.delivery)" -ne "accepted" -and "$($e.delivery)" -ne "command-consumed") { continue }
      if ([int]$e.tick -lt $MinTick) { $cursor = $i + 1; continue }
      try {
        $null = Test-StressApplyClean $e "$Tag@$i"
      } catch {
        $msg = "$($_.Exception.Message)"
        if ($msg -match "pending transfers|pending operations|held scopes|present|pending op|floating|authority not controlled|float origin") {
          $pendingSeen++
          $cursor = $i + 1
          continue
        }
        throw
      }
      if (Test-StressApplyPending $e) { $pendingSeen++; $cursor = $i + 1; continue }
      $Mark.Value = $i + 1
      return @{ event = $e; index = $i; skipped_pending = $pendingSeen }
    }
    $cursor = $events.Count
    Start-Sleep -Milliseconds 50
  }
  Fail-Ws "$Tag settlement timeout (skipped $pendingSeen pending rows)"
  return $null
}

function Wait-StressDispatch([string]$LogPath, [ref]$Mark, $Argv, [int]$TimeoutSec, [string]$Tag) {
  # Single-item transport join (kept for offline fault injection only): the
  # live journey never waits per command. See Wait-StressDispatchOrdered for
  # the checkpoint-ordered outcome join.
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

function Get-StressDispatchKey($Argv) {
  # Pure mapping from CLI argv to the (event, op) dispatch-log join key.
  $op = $Argv[1]
  if ($op -eq "--focus") { $op = "focus" }
  elseif ($op -eq "--move") { $op = "move" }
  elseif ($op -eq "--select") { $op = "select" }
  elseif ($op -eq "--send") { $op = "send" }
  elseif ($op -eq "--stay") { $op = "stay" }
  elseif ($op -eq "--previous") { $op = "previous" }
  elseif ($op -eq "--relative") { $op = if ($Argv[2] -eq "next") { "relative-next" } else { "relative-prev" } }
  $edgeEvent = if ($Argv[0] -eq "directional") { "directional" } else { "workspace" }
  return @{ event = $edgeEvent; op = "$op" }
}

function Wait-StressDispatchOrdered([string]$LogPath, [ref]$Mark, $Pending, [int]$TimeoutSec, [string]$Tag) {
  # Checkpoint-ordered outcome join for burst publication: matches each queued
  # CLI request to the next dispatch log line with the same (event, op) key,
  # in queue order, from the checkpoint-start cursor. Transport publication
  # is unpaced per command (only single-pending request-file availability
  # serializes); outcomes join here, never per command. Advances the
  # shared cursor past the last matched line without discarding unmatched
  # evidence between matches. Acceptance is per family, from the product
  # vocabulary: directional queue-acks carry "dispatched" once the intent is
  # queued (cli_directional.rs; refusals are refused/unmanaged), workspace
  # select/send lines carry "ok" only with a verified core reply
  # (sends.rs, cli_workspace.rs; unverified/refused/unmanaged fail). Effect
  # proof stays at the checkpoint (settled apply plus native readbacks); this
  # join proves ordered consumption only. Any other outcome is a test
  # failure at the checkpoint, never a silent repair.
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  $cursor = [int]$Mark.Value
  $outcomes = [System.Collections.ArrayList]@()
  foreach ($item in @($Pending)) {
    $key = Get-StressDispatchKey $item.argv
    $found = $false
    while ((Get-Date) -lt $deadline) {
      $events = @(Read-StressOwnerLog $LogPath)
      for ($i = $cursor; $i -lt $events.Count; $i++) {
        $e = $events[$i]
        if ("$($e.event)" -eq $key.event -and "$($e.edge)" -eq "cli" -and "$($e.op)" -eq $key.op) {
          $cursor = $i + 1
          $null = $outcomes.Add(@{ op = "$($item.op)"; tick = [int]$e.tick; outcome = "$($e.outcome)" })
          $found = $true
          break
        }
      }
      if ($found) { break }
      Start-Sleep -Milliseconds 50
    }
    if (-not $found) { Fail-Ws "$Tag dispatch-order timeout for $($item.op)" }
    $last = $outcomes[$outcomes.Count - 1]
    $accept = if ($key.event -eq "directional") { "dispatched" } else { "ok" }
    if ("$($last.outcome)" -ne $accept) {
      Fail-Ws "$($item.op) dispatch outcome $($last.outcome) at tick $($last.tick): product refused/dropped, no repair"
    }
  }
  $Mark.Value = $cursor
  return ,@($outcomes)
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
    Start-Sleep -Milliseconds 50
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
      $null = Test-StressFixtureProcess ([int]$pidOut) $FixtureState "hwnd-$Id"
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
    $events = @(Read-StressOwnerLog $LogPath)
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
  # the token and whose remaining state is settled, plus the sentinel title
  # gone from native enumeration. Transient pending rows are skipped until
  # the deadline; corruption fails fast. Close is an explicit observation
  # action, so its own join stays per action.
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  $cursor = [int]$Mark.Value
  while ((Get-Date) -lt $deadline) {
    $events = @(Read-StressOwnerLog $LogPath)
    for ($i = $cursor; $i -lt $events.Count; $i++) {
      $e = $events[$i]
      if ("$($e.event)" -ne "world-apply") { continue }
      if ("$($e.delivery)" -ne "accepted" -and "$($e.delivery)" -ne "command-consumed") { continue }
      if ((@($e.desired_windows | Where-Object { "$($_.window)" -eq "$Token" }).Count) -ne 0) { $cursor = $i + 1; continue }
      try {
        $null = Test-StressApplyClean $e "close-$Id@$i"
      } catch {
        $msg = "$($_.Exception.Message)"
        if ($msg -match "pending transfers|pending operations|held scopes|present|pending op|floating|authority not controlled|float origin") {
          $cursor = $i + 1
          continue
        }
        throw
      }
      if (Test-StressApplyPending $e) { $cursor = $i + 1; continue }
      $Mark.Value = $i + 1
      $tick = [int]$e.tick
      $titles = Get-StressTitles
      if ($null -ne (Get-StressHwndOfCaller $titles $Id)) { Fail-Ws "close $Id hwnd still titled" }
      return $tick
    }
    $cursor = @(Read-StressOwnerLog $LogPath).Count
    Start-Sleep -Milliseconds 50
  }
  Fail-Ws "close-$Id settlement timeout"
  return 0
}

function Invoke-StressFocus([string]$FixtureExe, [string]$Socket, [string]$Nonce, [string]$Id, $HwndOf, $FixtureState, [int]$TimeoutSec) {
  $want = $HwndOf[$Id]
  if ($null -eq $want) { Fail-Ws "no hwnd for focus $Id" }
  $null = Test-StressHwndOwner ([uint64]$want) $FixtureState "focus-$Id"
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
      $null = Test-StressFixtureProcess ([int]$pidOut) $FixtureState "focus-$Id"
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
      $full = @([int]$row.work.x, [int]$row.work.y, [int]$row.work.w, [int]$row.work.h)
      if ($null -ne $row.full) {
        $full = @([int]$row.full.x, [int]$row.full.y, [int]$row.full.w, [int]$row.full.h)
      }
      return @{ output = "$($row.output)"; work = @([int]$row.work.x, [int]$row.work.y, [int]$row.work.w, [int]$row.work.h); full = $full }
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
  # command: binds the single native output, the frozen home id, the current
  # away slot (rebinds on lifecycle pruning like the shared simulator), the
  # seen set, the discovered work area, and the current alias ordinals.
  # Verifies the configured zero gaps and the bootstrap roster, skipping
  # earlier fresh rows until the full roster lands.
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
  $full = @([int]$g.work.x, [int]$g.work.y, [int]$g.work.w, [int]$g.work.h)
  if ($null -ne $g.full) { $full = @([int]$g.full.x, [int]$g.full.y, [int]$g.full.w, [int]$g.full.h) }
  $ring = @(Get-StressRing $ev $nativeOut)
  if ("$($ring[0])" -eq "") { Fail-Ws "topology-init: empty ring head" }
  $seen = @{}
  foreach ($id in $ring) { $seen["$id"] = $true }
  $ord = @{ home = (@($ring).IndexOf($ring[0]) + 1); away = (@($ring).IndexOf($ring[1]) + 1) }
  Rec-Ws "topology" @{ out = $nativeOut; work = ($work -join ","); home = $ring[0]; away = $ring[1] }
  return @{
    NativeOut = $nativeOut; HomeId = $ring[0]; AwayId = $ring[1]
    DiscoveredWork = $work; MonitorFull = $full; AliasOrd = $ord
    LastTick = [int]$ev.tick; Revision = [int]$ev.revision; Ring = $ring; Seen = $seen
    LastDispatchTick = 0
    Pending = [System.Collections.ArrayList]@(); DispatchMark = 0; PagerRetained = $null
  }
}

function Refresh-StressRing($ApplyEvent, $Topo) {
  # Re-resolve the ring from a settled fresh apply, mirroring the shared
  # simulator (world_stress.rs refresh_away_slot): home stays frozen at order
  # 0 (it never empties); the away slot at order 1 rebinds when lifecycle
  # prunes an emptied workspace and keeps the trailing spare. Every id joins
  # the seen set; trailing spares past order 1 are allowed and must name seen
  # ids. A moved home or a missing away slot fails loudly.
  $ring = @(Get-StressRing $ApplyEvent $Topo.NativeOut)
  if ("$($ring[0])" -ne "$($Topo.HomeId)") {
    Fail-Ws "home slot moved: $($ring[0]) != $($Topo.HomeId)"
  }
  if ($ring.Count -lt 2) { Fail-Ws "away slot missing" }
  if ($null -eq $Topo.Seen) { $Topo.Seen = @{} }
  foreach ($id in $ring) { $Topo.Seen["$id"] = $true }
  $Topo.AwayId = $ring[1]
  $Topo.Ring = $ring
  $Topo.AliasOrd = @{
    home = (@($ring).IndexOf($Topo.HomeId) + 1)
    away = (@($ring).IndexOf($Topo.AwayId) + 1)
  }
  if ([int]$Topo.AliasOrd.home -ne 1 -or [int]$Topo.AliasOrd.away -ne 2) {
    Fail-Ws "alias ordinals not [home=1, away=2] after rebind"
  }
  return $ring
}

function Invoke-StressCheckpoint($Action, $Case, $AuthoredWork, $Topo, $TokOf, $HwndOf, $FixtureState, [ref]$Mark, [ref]$PagerMark, $LogPath, $OwnerCopy, [scriptblock]$OnRetry = $null, [int]$TimeoutSec = 60) {
  # Bounded checkpoint loop: drain queued burst dispatches once (causal
  # delivery over the dedicated dispatch cursor; the apply cursor is never
  # consumed by dispatch joins), then repeat settled-apply join, fresh
  # native sampling (HWND titles re-resolved and PID/creation revalidated)
  # and retained pager join until Compare passes or the deadline expires.
  # Transient native/pager lag retries against the same or a newer settled
  # row; corruption, missing evidence and identity mismatches fail closed
  # immediately. Commits the apply cursor and topology only on success, so a
  # retry never discards unconsumed evidence.
  $probe = Invoke-LiveNative $OwnerCopy @("ready") | ConvertFrom-Json
  if ($probe.ready -ne $true) { Fail-Ws "owner not ready at checkpoint $($Action.name)" }
  $tiles = Convert-StressTiles $Action.tiles $AuthoredWork $Topo.DiscoveredWork
  $expected = @{
    View = "$($Action.view)"; Members = @($Action.members); Homing = @{};
    Tiles = $tiles; Foreground = $null; PagerCurrent = "$($Action.pager_current)";
    PagerOccupied = @($Action.pager_occupied)
  }
  foreach ($p in $Action.homing.PSObject.Properties) { $expected.Homing[$p.Name] = "$($p.Value)" }
  if ($null -ne $Action.PSObject.Properties["foreground"] -and $null -ne $Action.foreground) {
    $expected.Foreground = "$($Action.foreground)"
  }
  if ($null -eq $Topo.Pending) { $Topo.Pending = [System.Collections.ArrayList]@() }
  $maxDispTick = [int]$Topo.LastTick
  $outs = Drain-StressPending $LogPath $Topo 30 "checkpoint-$($Action.name)"
  foreach ($o in @($outs)) { if ([int]$o.tick -gt $maxDispTick) { $maxDispTick = [int]$o.tick } }
  $needTick = [int]$Topo.LastTick
  if ($maxDispTick -gt $needTick) { $needTick = $maxDispTick }
  # The fence above survives intervening observation drains: Drain records the
  # consumed dispatch high-water mark on the topology, so a checkpoint whose
  # queue was already drained still settles at or past actuation.
  if ([int]$Topo.LastDispatchTick -gt $needTick) { $needTick = [int]$Topo.LastDispatchTick }
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  $tMark = [int]$Mark.Value
  $ev = $null
  $lastErr = "no settled row yet"
  $iter = 0
  while ((Get-Date) -lt $deadline) {
    $iter++
    if ($null -ne $OnRetry) { & $OnRetry $iter }
    if ($null -eq $ev) {
      $remain = [int](($deadline - (Get-Date)).TotalSeconds)
      if ($remain -lt 2) { $remain = 2 }
      try {
        $tRef = [ref]$tMark
        $r = Wait-StressFreshApply $LogPath $tRef $needTick $remain "checkpoint-$($Action.name)"
        $tMark = $tRef.Value
        $ev = $r.event
      } catch {
        $lastErr = "$($_.Exception.Message)"
        if (-not (Test-StressTransientMismatch $lastErr)) { throw }
        Start-Sleep -Milliseconds 200
        continue
      }
      if ([int]$ev.tick -lt $maxDispTick) {
        $lastErr = "apply tick $([int]$ev.tick) behind last dispatched tick $maxDispTick"
        $ev = $null
        continue
      }
    }
    try {
      $ring = Refresh-StressRing $ev $Topo
      $desired = @{}
      foreach ($row in @($ev.desired_windows)) { $desired["$($row.window)"] = $row }
      $viewRow = @($ev.desired_outputs | Where-Object { "$($_.output)" -eq "$($Topo.NativeOut)" } | Select-Object -First 1)
      if ($viewRow.Count -ne 1) { Fail-Ws "checkpoint $($Action.name): no view row for $($Topo.NativeOut)" }
      $geoRow = @($ev.desired_geometry | Where-Object { "$($_.output)" -eq "$($Topo.NativeOut)" } | Select-Object -First 1)
      if ($geoRow.Count -ne 1 -or $null -eq $geoRow[0].work -or $null -eq $geoRow[0].full) {
        Fail-Ws "checkpoint $($Action.name): no full geometry row for $($Topo.NativeOut)"
      }
      $monFull = @([int]$geoRow[0].full.x, [int]$geoRow[0].full.y, [int]$geoRow[0].full.w, [int]$geoRow[0].full.h)
      $managed = @{}
      $titles = Get-StressTitles
      foreach ($id in @($Action.members)) {
        $hwnd = Get-StressHwndOfCaller $titles "$id"
        if ($null -eq $hwnd) { Fail-Ws "checkpoint $($Action.name): no hwnd for $id" }
        if ("$hwnd" -ne "$($HwndOf[$id])") { Fail-Ws "checkpoint $($Action.name): hwnd changed for $id" }
        $null = Test-StressHwndOwner ([uint64]$hwnd) $FixtureState "checkpoint-$($Action.name)"
        $managed[$id] = $hwnd
      }
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
        $null = Test-StressFixtureProcess ([int]$fp) $FixtureState "checkpoint-$($Action.name)"
      }
      $candRev = [int]$ev.revision
      $pr = $Topo.PagerRetained
      $prRef = [ref]$pr
      $prem = [int](($deadline - (Get-Date)).TotalSeconds)
      if ($prem -lt 2) { $prem = 2 }
      if ($prem -gt 5) { $prem = 5 }
      $pager = Wait-StressPager $LogPath $PagerMark $prRef $candRev $Topo.NativeOut $prem "$($Action.name)"
      $Topo.PagerRetained = $prRef.Value
      $actual = @{
        NativeOut = $Topo.NativeOut; HomeId = $Topo.HomeId; AwayId = $Topo.AwayId; Ring = $ring;
        Tokens = $TokOf; Desired = $desired; DesiredView = "$($viewRow[0].logical_view)"; MonitorFull = $monFull; Frames = $frames;
        ForegroundHwnd = $fgHwnd; ManagedHwnds = $managed; Pager = $pager.entry;
        PagerOutputsTotal = $pager.outputs_total; PagerOutputsOmitted = $pager.outputs_omitted;
        Revision = $candRev
      }
      $null = Compare-StressCheckpoint $expected $actual
      $Mark.Value = $tMark
      $Topo.LastTick = [int]$ev.tick
      if ($candRev -gt [int]$Topo.Revision) { $Topo.Revision = $candRev }
      Rec-Ws "checkpoint" @{ name = "$($Action.name)"; revision = $candRev; iterations = $iter }
      return
    } catch {
      $lastErr = "$($_.Exception.Message)"
      if (-not (Test-StressTransientMismatch $lastErr)) { throw }
      # Count mismatch pins a settled apply whose ring postdates the latest
      # pager emission: the matching payload necessarily carries a higher
      # revision, so drop the pinned row and re-wait for a newer settled
      # apply instead of re-polling under the same revision cap.
      if ($lastErr -match "^pager cells \d+ != ring \d+$") { $ev = $null }
      Start-Sleep -Milliseconds 200
    }
  }
  Fail-Ws "checkpoint $($Action.name) not settled: $lastErr"
}

function Wait-StressPager([string]$LogPath, [ref]$PagerMark, [ref]$PagerRetained, [int]$Revision, [string]$NativeOut, [int]$TimeoutSec, [string]$Tag) {
  # Change-gated renderer payload join: the retained latest actually emitted
  # pager-cells payload for the native output at or before this checkpoint
  # revision. A payload change emits one line; identical repeats stay silent,
  # so requiring a same-revision row would falsely fail settled checkpoints.
  # The retained slot keys on (revision, log index) with tick/observation
  # provenance: a later emission at the SAME revision (foreground/render
  # change) supersedes, an older repeat never does. Never fabricates
  # freshness: returns the latest emitted entry at or below the revision
  # (updating the retained slot), or fails at the deadline when no payload
  # was ever emitted for the output. Ambiguous duplicate entries for the
  # output fail loudly.
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    $events = @(Read-StressOwnerLog $LogPath)
    $best = $null
    $bestRev = -1
    $bestIdx = -1
    for ($i = 0; $i -lt $events.Count; $i++) {
      $e = $events[$i]
      if ("$($e.event)" -ne "pager-cells") { continue }
      $rev = 0
      try { $rev = [int]$e.revision } catch { continue }
      if ($rev -gt $Revision) { continue }
      $hits = @(@($e.outputs) | Where-Object { "$($_.output)" -eq "$NativeOut" })
      if ($hits.Count -eq 0) { continue }
      if ($hits.Count -gt 1) { Fail-Ws "pager $Tag ambiguous entries for $NativeOut" }
      if ($rev -gt $bestRev -or ($rev -eq $bestRev -and $i -gt $bestIdx)) {
        $cells = @($hits[0].cells)
        $best = @{
          output = "$($hits[0].output)"; revision = $rev; index = $i
          tick = $e.tick; observation = $e.observation; dpi = $hits[0].dpi
          cells = $cells; cells_total = [int]$hits[0].cells_total
          cells_omitted = [int]$hits[0].cells_omitted; active = [int]$hits[0].active
        }
        $bestOutTotal = $e.outputs_total
        $bestOutOmitted = $e.outputs_omitted
        $bestRev = $rev
        $bestIdx = $i
      }
    }
    if ($null -ne $best) {
      $cur = $PagerRetained.Value
      if ($null -eq $cur -or [int]$cur.entry.revision -lt $bestRev -or
          ([int]$cur.entry.revision -eq $bestRev -and [int]$cur.entry.index -lt $bestIdx)) {
        $PagerRetained.Value = @{ entry = $best; outputs_total = $bestOutTotal; outputs_omitted = $bestOutOmitted }
      }
      $ret = $PagerRetained.Value
      if ([int]$ret.entry.revision -gt $Revision) { Fail-Ws "pager retained revision newer than checkpoint" }
      $PagerMark.Value = $events.Count
      return $ret
    }
    Start-Sleep -Milliseconds 50
  }
  Fail-Ws "pager $Tag join timeout at or before revision $Revision"
  return $null
}

function Test-StressOwnerSame($A, $B) {
  # Exact run-owned owner comparison: PID plus creation lifetime plus
  # executable, with user/session binding when both sides carry them.
  # Null on either side is never equal. Pure and mock-testable.
  if ($null -eq $A -or $null -eq $B) { return $false }
  try {
    if ([int]$A.pid -ne [int]$B.pid) { return $false }
  } catch { return $false }
  if ("$($A.process_creation)" -ine "$($B.process_creation)") { return $false }
  if (-not (Test-ExeEqual "$($A.exe_path)" "$($B.exe_path)")) { return $false }
  $hasSid = ($null -ne $A.PSObject.Properties["user_sid"] -and $null -ne $B.PSObject.Properties["user_sid"] -and
    "$($A.user_sid)" -ne "" -and "$($B.user_sid)" -ne "")
  if ($hasSid -and ("$($A.user_sid)" -ine "$($B.user_sid)")) { return $false }
  $hasSess = ($null -ne $A.PSObject.Properties["session_id"] -and $null -ne $B.PSObject.Properties["session_id"] -and
    "$($A.session_id)" -ne "" -and "$($B.session_id)" -ne "")
  if ($hasSess -and ([int]$A.session_id -ne [int]$B.session_id)) { return $false }
  return $true
}

function Read-StressLedgerOwner([string]$LedgerDir) {
  # Run-owned ledger readback: $null when no ledger binds an owner.
  # Malformed JSON throws so cleanup records a failure, never a skip.
  if ([string]::IsNullOrWhiteSpace($LedgerDir)) { return $null }
  $led = Join-Path $LedgerDir "ledger.json"
  if (-not (Test-Path -LiteralPath $led -PathType Leaf)) { return $null }
  $j = (Get-Content -LiteralPath $led -Raw -ErrorAction Stop) | ConvertFrom-Json
  if ($null -eq $j -or $null -eq $j.owner) { throw "ledger present without owner at $led" }
  return $j.owner
}

function Assert-StressOwnerUnreplaced($Ctx) {
  # Pre-effect revalidation before stop/restore: when a live ledger binds
  # an owner it must equal the run-started identity, else this run must not
  # touch another owner's process. Returns the live ledger owner or $null.
  if ($null -eq $Ctx.Owner) { throw "no bound run owner; will not stop another owner" }
  $live = $null
  if ($null -ne $Ctx.LedgerDir -and "$($Ctx.LedgerDir)" -ne "") {
    $live = Read-StressLedgerOwner "$($Ctx.LedgerDir)"
  }
  if ($null -ne $live -and (-not (Test-StressOwnerSame $live $Ctx.Owner))) {
    throw "owner-replaced (ledger $($live | ConvertTo-Json -Compress) != run $($Ctx.Owner | ConvertTo-Json -Compress)); will not stop another owner"
  }
  return $live
}

function Wait-StressLedgerOwner([string]$LedgerDir, [int]$TimeoutSec, [string]$Tag) {
  # Bounded ledger capture for racing launches: poll until a ledger owner
  # binds or the deadline passes, then return $null (preserved failure at
  # the call site, never an unbounded wait). Mirrors the bounded poll in
  # Invoke-DevCleanup (scripts/windows-dev.ps1) for late-appearing ledgers.
  if ($TimeoutSec -lt 0) { $TimeoutSec = 0 }
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -le $deadline) {
    $cand = $null
    try { $cand = Read-StressLedgerOwner "$LedgerDir" } catch { throw }
    if ($null -ne $cand) { return $cand }
    Start-Sleep -Milliseconds 250
  }
  return $null
}

function Write-StressLeg($Ctx, [string]$LegPath, [string]$OwnerCopy, [int]$OwnerSeconds) {
  # Single recovery ledger: persist bound recovery data BEFORE each effect
  # and refresh after every identity capture, so any later failure or a
  # detached -Stop can tell started effects from unstarted ones. One
  # existing leg.json shape only; additive intent fields never remove keys.
  $leg = @{
    socket = "$($Ctx.Socket)"; nonce = "$($Ctx.Nonce)"
    owner = "$OwnerCopy"; owner_seconds = $OwnerSeconds
    owner_launched = [bool]$Ctx.OwnerLaunched
    owner_launch_intended = [bool]$Ctx.OwnerLaunchIntended
    settings_changed = @($Ctx.SettingsChanged)
    settings_intended = [bool]$Ctx.SettingsConfigIntended
    ledger_dir = "$($Ctx.LedgerDir)"; settings_path = "$($Ctx.SettingsPath)"
    settings_orig = "$($Ctx.GroupOrig.path)"; spi_path = "$($Ctx.GroupOrig.spi_path)"
  }
  if ($null -ne $Ctx.FixtureProc) {
    try { $leg.fixture_pid = [int]$Ctx.FixtureProc.Id } catch {}
  }
  if (-not [string]::IsNullOrWhiteSpace("$($Ctx.FixtureCreation)")) {
    $leg.fixture_creation = "$($Ctx.FixtureCreation)"
  }
  if ($null -ne $Ctx.Owner) {
    $leg.owner_pid = [int]$Ctx.Owner.pid
    $leg.owner_creation = "$($Ctx.Owner.process_creation)"
    if ($null -ne $Ctx.Owner.PSObject.Properties["user_sid"]) {
      $leg.owner_sid = "$($Ctx.Owner.user_sid)"
    }
    if ($null -ne $Ctx.Owner.PSObject.Properties["session_id"]) {
      $leg.owner_session = [int]$Ctx.Owner.session_id
    }
  }
  $leg | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $LegPath -Encoding utf8
}

function Test-StressShouldRestoreSettings($Ctx) {
  # Settings are a global effect: restore only when this run intended or
  # completed configuration. Intent is persisted BEFORE the potentially
  # partially effective config write, so a mid-write throw still restores
  # instead of bypassing. Before-intent failures must not mutate settings.
  # Pure and mock-testable.
  if ($null -eq $Ctx) { return $false }
  if ($null -eq $Ctx.SettingsPath -or $null -eq $Ctx.GroupOrig) { return $false }
  if ($Ctx.SettingsConfigIntended -eq $true) { return $true }
  if ($null -eq $Ctx.SettingsChanged) { return $false }
  return (@($Ctx.SettingsChanged).Count -gt 0)
}

function Invoke-StressCleanup($Ctx, [string]$Tag) {
  # Abort-safe recovery: every phase runs inside its own try so an earlier
  # failure never skips a later one. Only effects this run started are
  # undone: unstarted bindings record skips, never writes. Started effects
  # that fail recovery aggregate and throw nonzero once at the end, with
  # the evidence left in place. Never broad-kills: only exact-PID/creation
  # bound processes are signaled, and stop/restore run only for the
  # run-owned owner identity (ledger revalidated before stop; foreign or
  # reused owners are preserved, never stopped).
  $fails = [System.Collections.ArrayList]@()
  $skips = [System.Collections.ArrayList]@()
  $stopForeign = $false
  try {
    if ($null -eq $Ctx.OwnerPayload) { throw "no bound owner payload (startup never staged)" }
    $launched = ($Ctx.OwnerLaunched -eq $true -or $Ctx.OwnerLaunchIntended -eq $true)
    if ($null -eq $Ctx.Owner -and -not $launched) {
      $null = $skips.Add("owner-stop: unstarted")
    } else {
      if ($null -eq $Ctx.Owner) {
        # Readiness never proved but the launch was intended: bounded ledger
        # capture adopts the live owner only when its exe is this run
        # payload, else the foreign owner is preserved for manual stop.
        $cand = $null
        try { $cand = Wait-StressLedgerOwner "$($Ctx.LedgerDir)" 5 "owner-stop" } catch { throw }
        if (($null -eq $cand) -or (-not (Test-ExeEqual "$($cand.exe_path)" "$($Ctx.OwnerPayload)"))) {
          throw "owner launch intended but unbound (no ledger owner for this payload); exit unconfirmed, preserved for manual stop"
        }
        $Ctx.Owner = $cand
      }
      $null = Assert-StressOwnerUnreplaced $Ctx
      $st = Invoke-LiveNative $Ctx.OwnerPayload @("stop") | ConvertFrom-Json
      if (-not $st.owner_exited) { throw "owner stop no exit" }
    }
  } catch {
    $msg = "$($_.Exception.Message)"
    if ($msg -like "owner-replaced*will not stop another owner*") { $stopForeign = $true }
    if ($msg -like "*unstarted*") { $null = $skips.Add("owner-stop: $msg") }
    else { $null = $fails.Add("owner-stop: $msg") }
  }
  try {
    if ($null -eq $Ctx.FixtureExe -or [string]::IsNullOrWhiteSpace("$($Ctx.Socket)") -or [string]::IsNullOrWhiteSpace("$($Ctx.Nonce)")) {
      $null = $skips.Add("fixture-shutdown: unstarted")
    } else {
      # Bound-target check before the shutdown request: when a proc binding
      # exists it must still be the same exe/creation, else a foreign or
      # reused PID never receives this run's shutdown.
      $validTarget = $true
      if ($null -ne $Ctx.FixtureProc -and -not [string]::IsNullOrWhiteSpace("$($Ctx.FixtureCreation)")) {
        try {
          $chk = Get-Process -Id ([int]$Ctx.FixtureProc.Id) -ErrorAction Stop
          if ("$($chk.Path)" -ine "$($Ctx.FixtureExe)") { throw "fixture PID reused by another exe; will not shutdown" }
          $chkHex = "{0:x16}" -f $chk.StartTime.ToUniversalTime().ToFileTimeUtc()
          if ($chkHex -ne "$($Ctx.FixtureCreation)") { throw "fixture PID reused by another process; will not shutdown" }
        } catch {
          if ("$($_.FullyQualifiedErrorId)" -like "NoProcessFound*") { $validTarget = $false }
          else { throw }
        }
      }
      if ($validTarget) {
        $null = Send-StressFixture $Ctx.FixtureExe $Ctx.Socket $Ctx.Nonce (@{ v = 1; request_id = "bye"; op = "shutdown" } | ConvertTo-Json -Compress)
      } else {
        $null = $skips.Add("fixture-shutdown: target already gone")
      }
    }
  } catch { $null = $fails.Add("fixture-shutdown: $($_.Exception.Message)") }
  try {
    if ($null -eq $Ctx.FixtureProc -and [string]::IsNullOrWhiteSpace("$($Ctx.FixtureCreation)")) {
      $null = $skips.Add("fixture-exit: unstarted")
    } else {
      if ($null -eq $Ctx.FixtureProc) { throw "no bound fixture process" }
      if ([string]::IsNullOrWhiteSpace("$($Ctx.FixtureCreation)")) { throw "no bound fixture creation" }
      $gone = $false
      try { $null = Get-Process -Id ([int]$Ctx.FixtureProc.Id) -ErrorAction Stop }
      catch {
        if ("$($_.FullyQualifiedErrorId)" -like "NoProcessFound*") { $gone = $true } else { throw }
      }
      if (-not $gone) {
        $liveFx = Get-Process -Id ([int]$Ctx.FixtureProc.Id) -ErrorAction Stop
        if ("$($liveFx.Path)" -ine "$($Ctx.FixtureExe)") { throw "fixture PID reused by another exe; will not kill" }
        $hex = "{0:x16}" -f $liveFx.StartTime.ToUniversalTime().ToFileTimeUtc()
        if ($hex -ne "$($Ctx.FixtureCreation)") { throw "fixture PID reused by another process; will not kill" }
        $Ctx.FixtureProc.Kill()
        # Bounded exit wait so a slow exit never reports false residue.
        $fxDl = (Get-Date).AddSeconds(10)
        $gone = $false
        while ((Get-Date) -lt $fxDl) {
          try { $null = Get-Process -Id ([int]$Ctx.FixtureProc.Id) -ErrorAction Stop; Start-Sleep -Milliseconds 250 }
          catch {
            if ("$($_.FullyQualifiedErrorId)" -like "NoProcessFound*") { $gone = $true; break } else { throw }
          }
        }
        if (-not $gone) { throw "fixture process still alive after exact kill" }
      }
    }
  } catch { $null = $fails.Add("fixture-exit: $($_.Exception.Message)") }
  try {
    if ($null -eq $Ctx.OwnerPayload) { throw "no bound owner payload" }
    if ($stopForeign) { throw "skipped: foreign owner preserved, no restore" }
    $launchedRs = ($Ctx.OwnerLaunched -eq $true -or $Ctx.OwnerLaunchIntended -eq $true)
    if ($null -eq $Ctx.Owner -and -not $launchedRs) {
      $null = $skips.Add("owner-restore: unstarted")
    } else {
      if ($null -eq $Ctx.Owner) { throw "no bound run owner; will not restore another owner" }
      $null = Assert-StressOwnerUnreplaced $Ctx
      $r = Invoke-LiveNative $Ctx.OwnerPayload @("restore") | ConvertFrom-Json
      if (-not $r.restored) { throw "restore failed" }
    }
  } catch {
    $msg = "$($_.Exception.Message)"
    if ($msg -like "skipped:*" -or $msg -like "*unstarted*") { $null = $skips.Add("owner-restore: $msg") }
    else { $null = $fails.Add("owner-restore: $msg") }
  }
  try {
    if ($null -eq $Ctx.Owner) {
      $null = $skips.Add("owner-ready: unstarted")
    } else {
      $rr = $null
      try { $rr = (Invoke-LiveNative $Ctx.OwnerPayload @("ready") | ConvertFrom-Json) }
      catch { $rr = $null }
      if ($null -ne $rr -and $rr.ready -eq $true) { throw "owner still ready after stop" }
      $gone = $false
      try {
        $proc = Get-Process -Id ([int]$Ctx.Owner.pid) -ErrorAction Stop
        if ("$($proc.Path)" -ine "$($Ctx.Owner.exe_path)") { $gone = $true }
        else {
          $hex = "{0:x16}" -f $proc.StartTime.ToUniversalTime().ToFileTimeUtc()
          if ($hex -ne "$($Ctx.Owner.process_creation)") { $gone = $true }
        }
      } catch {
        if ("$($_.FullyQualifiedErrorId)" -like "NoProcessFound*") { $gone = $true }
        else { throw }
      }
      if (-not $gone) { throw "owner process still alive" }
    }
  } catch {
    $msg = "$($_.Exception.Message)"
    if ($msg -like "*unstarted*") { $null = $skips.Add("owner-ready: $msg") }
    else { $null = $fails.Add("owner-ready: $msg") }
  }
  try {
    if ($stopForeign) {
      # Foreign owner preserved: never overwrite global settings behind
      # another owner. Recovery stays failed with evidence kept.
      throw "settings-restore: skipped (foreign owner preserved; live settings left untouched)"
    }
    if (-not (Test-StressShouldRestoreSettings $Ctx)) {
      $null = $skips.Add("settings-restore: unstarted (this run never configured settings)")
    } else {
      Copy-Item $Ctx.GroupOrig.path $Ctx.SettingsPath -Force
      $liveH = Get-LiveBytesSha ([IO.File]::ReadAllBytes($Ctx.SettingsPath))
      if ($liveH -ne $Ctx.GroupOrig.sha) { throw "settings differ from preimage after restore" }
    }
  } catch { $null = $fails.Add("settings-restore: $($_.Exception.Message)") }
  try {
    if ($null -eq $Ctx.GroupOrig -or $null -eq $Ctx.GroupOrig.spi_path) {
      $null = $skips.Add("taskbar-spi: unstarted")
    } else {
      $spiWant = (Get-Content -LiteralPath $Ctx.GroupOrig.spi_path -Raw) | ConvertFrom-Json
      Assert-LiveSpiFields $spiWant "final stored"
      $spiNow = Get-LiveTaskbarSpi
      Assert-LiveSpiFields $spiNow "final current"
      if ([int]$spiNow.arranging -ne [int]$spiWant.arranging -or
          [int]$spiNow.pen -ne [int]$spiWant.pen -or
          "$($spiNow.abm)" -ne "$($spiWant.abm)") { throw "taskbar/SPI differ from preimage" }
    }
  } catch { $null = $fails.Add("taskbar-spi: $($_.Exception.Message)") }
  try {
    $bound = @(@($Ctx.OwnerPayload, $Ctx.FixtureExe) | Where-Object { $null -ne $_ -and "$_" -ne "" })
    if ($bound.Count -eq 0) {
      $null = $skips.Add("process-residue: unstarted")
    } else {
      foreach ($exe in $bound) {
        $base = Split-Path -Leaf $exe
        $procs = @(Get-CimInstance Win32_Process -Filter "Name='$base'" -ErrorAction Stop |
          Where-Object { "$($_.ExecutablePath)" -ieq "$exe" })
        if ($procs.Count -gt 0) { throw "owned residue: $exe x$($procs.Count)" }
      }
    }
  } catch { $null = $fails.Add("process-residue: $($_.Exception.Message)") }
  try {
    if ($null -eq $Ctx.LedgerDir -or "$($Ctx.LedgerDir)" -eq "") {
      $null = $skips.Add("ledger: unstarted")
    } else {
      $led = Join-Path $Ctx.LedgerDir "ledger.json"
      if (Test-Path -LiteralPath $led) { throw "ledger present" }
    }
  } catch { $null = $fails.Add("ledger: $($_.Exception.Message)") }
  if ($fails.Count -gt 0) { throw "final ($Tag): $(($fails | ForEach-Object { "$_" }) -join '; ')" }
  $skipText = if ($skips.Count -gt 0) { " skips=[$($skips -join '; ')]" } else { "" }
  Write-Output "$Tag final cleanup ok$skipText"
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
  Fail-Ws "refuse: world-stress native leg parked pending hidden-frame oracle decision (see docs/changes/platform-neutral-stress-acceptance.md)"
  # Pre-effect validation: range gates run before any cargo build, settings
  # write, owner/fixture launch, or run-dir creation.
  if ($DelayMs -lt 0 -or $DelayMs -gt 60000) { Fail-Ws "refuse: DelayMs must be 0..60000" }
  # Product caps bounded runs at lifecycle::MAX_RUN_SECONDS (600); the
  # default above already honors it. Ranges stay pre-effect: no cargo build,
  # settings write, owner/fixture launch, or run-dir creation before this.
  if ($OwnerSeconds -lt 60 -or $OwnerSeconds -gt 600) { Fail-Ws "refuse: OwnerSeconds must be 60..600" }
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
  $legPath = Join-Path $runDir "leg.json"
  $ctx = @{
    OwnerPayload = $null; Owner = $null; OwnerLaunched = $false; OwnerLaunchIntended = $false
    FixtureExe = $null; FixtureProc = $null; FixtureCreation = ""
    Socket = $null; Nonce = $null
    SettingsPath = $null; GroupOrig = $null; LedgerDir = $null
    SettingsChanged = @(); SettingsConfigIntended = $false
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
    # Pre-effect owner refusal before any settings mutation: a live owner
    # must never be disturbed and a refused run must leave settings bytes
    # untouched. LedgerDir binds here; SettingsChanged stays empty until the
    # next line configures.
    $ledgerDir = Assert-NoPreexistingStressOwner $ownerCopy "world-stress"
    $ctx.LedgerDir = $ledgerDir
    # Persist recovery data BEFORE the settings effect so an interruption
    # during config still has a -Stop route (intent outlives partial writes).
    Write-StressLeg $ctx $legPath $ownerCopy $OwnerSeconds
    $ctx.SettingsConfigIntended = $true
    Write-StressLeg $ctx $legPath $ownerCopy $OwnerSeconds
    $caseChanged = Set-StressCaseSettings $settingsPath $case
    $ctx.SettingsChanged = @($caseChanged)
    Write-StressLeg $ctx $legPath $ownerCopy $OwnerSeconds
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
    Write-StressLeg $ctx $legPath $ownerCopy $OwnerSeconds
    Rec-Ws "fixture-announced" @{ pid = $serverPid }
    $authoredWork = @($case.screens[0].work)
    $staged = Get-StressDisplayBaseline
    $boot = Start-StressBootstrap $fixtureCopy $sock $nonce $case $authoredWork $staged.work
    # Refresh the single leg before owner launch (bootstrap done, owner not
    # yet started) so failures here keep a -Stop route.
    Write-StressLeg $ctx $legPath $ownerCopy $OwnerSeconds
    # Bind launch intent BEFORE the broker call: a throw inside the launch
    # still leaves a bounded ledger discovery for abort cleanup.
    $ctx.OwnerLaunchIntended = $true
    Write-StressLeg $ctx $legPath $ownerCopy $OwnerSeconds
    Start-ExplorerGui $ownerCopy "tile --user-start --seconds $OwnerSeconds --trace --scope-exe tiler-live-fixture.exe --inner-gap 0 --outer-gap 0" $binDir
    $ctx.OwnerLaunched = $true
    Write-StressLeg $ctx $legPath $ownerCopy $OwnerSeconds
    $ready = $null
    $deadline = (Get-Date).AddSeconds(30)
    while ((Get-Date) -lt $deadline) {
      try {
        $j = Invoke-LiveNative $ownerCopy @("ready") | ConvertFrom-Json
        if ($j.ready -eq $true) { $ready = $j; break }
      } catch {}
      Start-Sleep -Milliseconds 500
    }
    if ($null -eq $ready) { Fail-Ws "owner ready timeout (launched owner stays bound for abort cleanup)" }
    if (-not (Test-ExeEqual "$($ready.owner.exe_path)" "$ownerCopy")) { Fail-Ws "owner peer mismatch" }
    Assert-ParentIsExplorer ([int]$ready.owner.pid)
    # Full run-owned identity: PID plus creation lifetime plus exe plus
    # user/session when the product reports them. Cleanup revalidates the
    # live ledger against this binding before stop/restore.
    $ownerBind = @{
      pid = [int]$ready.owner.pid
      process_creation = "$($ready.owner.process_creation)"
      exe_path = "$($ready.owner.exe_path)"
    }
    if ($null -ne $ready.owner.PSObject.Properties["user_sid"]) {
      $ownerBind.user_sid = "$($ready.owner.user_sid)"
    }
    if ($null -ne $ready.owner.PSObject.Properties["session_id"]) {
      $ownerBind.session_id = [int]$ready.owner.session_id
    }
    $ctx.Owner = $ownerBind
    Write-StressLeg $ctx $legPath $ownerCopy $OwnerSeconds
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
          $null = Drain-StressPending $logPath $topo 30 "admit-$id"
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
          $null = Drain-StressPending $logPath $topo 30 "close-$id"
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
          $null = Drain-StressPending $logPath $topo 30 "focus-$id"
          Invoke-StressFocus $fixtureCopy $sock $nonce $id $hwndOf @{ pid = $serverPid; creation = $serverCreation } 15
          Rec-Ws "focus" @{ id = $id }
        }
        "checkpoint" {
          Invoke-StressCheckpoint $action $case $authoredWork $topo $tokOf $hwndOf @{ pid = $serverPid; creation = $serverCreation } ([ref]$mark) ([ref]$pagerMark) $logPath $ownerCopy
        }
        default {
          # Burst publication: resolve aliases against the latest observed
          # ring (away rebinds on pruning), fence cross-family order through
          # the transport idle gate (the owner polls workspace before
          # directional every tick, so two pending files would dispatch out
          # of definition order), then publish through the single-pending
          # transport as soon as it is consumed. Ordered outcomes join later
          # at the next observation barrier. No per-command dispatch or native
          # convergence wait; DelayMs above stays the only explicit pacing.
          $ord = @{
            home = (Resolve-StressAlias "home" $topo.Ring $topo.HomeId $topo.AwayId)
            away = (Resolve-StressAlias "away" $topo.Ring $topo.HomeId $topo.AwayId)
          }
          $argv = Get-StressCliArgs $action $ord
          Wait-StressTransportIdle $ledgerDir "$($action.op)"
          $null = Invoke-StressCli $ownerCopy $argv $ledgerDir "$($action.op)"
          $null = $topo.Pending.Add(@{ argv = $argv; op = "$($action.op)" })
          Rec-Ws "command-queued" @{ op = $action.op; argv = ($argv -join " ") }
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
    Owner = $null; OwnerLaunched = $false; OwnerLaunchIntended = $false
    FixtureExe = $null; FixtureProc = $null; FixtureCreation = ""
    Socket = "$($leg.socket)"; Nonce = "$($leg.nonce)"
    SettingsPath = $null; GroupOrig = $null; LedgerDir = $null
    SettingsChanged = @(); SettingsConfigIntended = $false
  }
  if ($null -ne $leg.PSObject.Properties["owner_pid"] -and $null -ne $leg.PSObject.Properties["owner_creation"]) {
    $sctx.Owner = @{ pid = [int]$leg.owner_pid; process_creation = "$($leg.owner_creation)"; exe_path = "$ownerPayload" }
    if ($null -ne $leg.PSObject.Properties["owner_sid"]) { $sctx.Owner.user_sid = "$($leg.owner_sid)" }
    if ($null -ne $leg.PSObject.Properties["owner_session"]) { $sctx.Owner.session_id = [int]$leg.owner_session }
    $sctx.OwnerLaunched = $true
  }
  if ($null -ne $leg.PSObject.Properties["settings_changed"]) {
    $sctx.SettingsChanged = @($leg.settings_changed)
  }
  if ($null -ne $leg.PSObject.Properties["settings_intended"] -and $leg.settings_intended -eq $true) {
    $sctx.SettingsConfigIntended = $true
  }
  if ($null -ne $leg.PSObject.Properties["owner_launched"] -and $leg.owner_launched -eq $true) {
    $sctx.OwnerLaunched = $true
  }
  if ($null -ne $leg.PSObject.Properties["owner_launch_intended"] -and $leg.owner_launch_intended -eq $true) {
    $sctx.OwnerLaunchIntended = $true
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
  # Cleanup failures exit nonzero with recovery evidence preserved: leg.json
  # stays in place and the failure line carries the phase ledger.
  try { Invoke-StressCleanup $sctx "world-stress-stop" } catch {
    Write-Output "stop cleanup: $($_.Exception.Message)"
    exit 1
  }
  exit 0
}
if ($Live) { Fail-Ws "refuse: world-stress native leg parked pending hidden-frame oracle decision (see docs/changes/platform-neutral-stress-acceptance.md)" }
if ("$($MyInvocation.InvocationName)" -eq ".") { return }
Write-Output "windows-world-stress parsed (no action without -Mock/-Live/-Stop)"
