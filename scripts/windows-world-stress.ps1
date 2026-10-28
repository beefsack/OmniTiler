param(
  [switch]$Mock,
  [switch]$Live,
  [switch]$Stop,
  [switch]$KeyboardCoverage,
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
#   pwsh -NoProfile -File scripts/windows-world-stress.ps1 -KeyboardCoverage [-Case <path>]
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
# `mini_frames` carries the exact owner source rect per miniature, so hidden
# proof survives miniature rounding. Miniature geometry is projected
# independently from each visible tile (and from the owner raw source while
# hidden) through the exact renderer contract (pager::miniature) and checked
# per token plus per-cell multisets, alongside desired allocations and
# independent native frames. Hidden members prove exact native geometry
# against the owner raw source at this revision (hide-epoch stability inside
# one continuous hide, reset on genuine reveal or re-admit).
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

function Get-StressShortcutCoverage($Action, $AliasOrd) {
  # Pure offline default-shortcut coverage for one case action. Product
  # command ops map to their catalog row: one default letter chord per
  # direction (never arrow variants), numbered select/send ride Win+N and
  # Win+Shift+N, relative steps ride the single-letter history rows and
  # previous rides the Tab toggle. select-workspace/send resolve home/away
  # through the caller-supplied ordinals; admit/close/focus-window are
  # fixture primitives and checkpoint is an observation. Default chords are
  # catalog candidates, NOT active settings and NOT an injection recipe:
  # the leg sends no keystrokes and normal product injection stays refused
  # by the intentional injected filter.
  switch ("$($Action.op)") {
    "select-workspace" {
      if ("$($Action.workspace)" -ne "home" -and "$($Action.workspace)" -ne "away") { Fail-Ws "unmapped workspace alias $($Action.workspace)" }
      $idx = $AliasOrd["$($Action.workspace)"]
      if ($null -eq $idx) { Fail-Ws "unmapped workspace alias $($Action.workspace)" }
      if (($idx -isnot [int]) -and ($idx -isnot [long])) { Fail-Ws "alias ordinal not integer $idx" }
      if ([int64]$idx -lt 1 -or [int64]$idx -gt 9) { Fail-Ws "alias ordinal out of range $idx" }
      $n = [int][int64]$idx
      return @{ status = "default"; binding_id = "workspace-select-$n"; default_chord = "Win+$n"; configurable = $true }
    }
    "select-numbered" {
      $covIdx = $Action.index
      if (($covIdx -isnot [int]) -and ($covIdx -isnot [long])) { Fail-Ws "select-numbered index not integer" }
      if ([int64]$covIdx -lt 1 -or [int64]$covIdx -gt 9) { Fail-Ws "select-numbered index must be 1..9" }
      $n = [int][int64]$covIdx
      return @{ status = "default"; binding_id = "workspace-select-$n"; default_chord = "Win+$n"; configurable = $true }
    }
    "select-relative" {
      $covDelta = $Action.delta
      if (($covDelta -isnot [int]) -and ($covDelta -isnot [long])) { Fail-Ws "select-relative delta not integer" }
      if ([int64]$covDelta -eq -1) {
        return @{ status = "default"; binding_id = "workspace-prev-h"; default_chord = "Win+Ctrl+H"; configurable = $true }
      }
      if ([int64]$covDelta -eq 1) {
        return @{ status = "default"; binding_id = "workspace-next-j"; default_chord = "Win+Ctrl+J"; configurable = $true }
      }
      Fail-Ws "select-relative delta must be -1/+1"
    }
    "select-previous" {
      return @{ status = "default"; binding_id = "workspace-previous"; default_chord = "Win+Ctrl+Tab"; configurable = $true }
    }
    "send" {
      if ($Action.follow -isnot [bool]) { Fail-Ws "send needs boolean follow" }
      if ("$($Action.workspace)" -ne "home" -and "$($Action.workspace)" -ne "away") { Fail-Ws "unmapped send alias $($Action.workspace)" }
      $idx = $AliasOrd["$($Action.workspace)"]
      if ($null -eq $idx) { Fail-Ws "unmapped send alias $($Action.workspace)" }
      if (($idx -isnot [int]) -and ($idx -isnot [long])) { Fail-Ws "alias ordinal not integer $idx" }
      if ([int64]$idx -lt 1 -or [int64]$idx -gt 9) { Fail-Ws "alias ordinal out of range $idx" }
      $n = [int][int64]$idx
      if ($Action.follow) {
        return @{ status = "default"; binding_id = "workspace-send-$n"; default_chord = "Win+Shift+$n"; configurable = $true }
      }
      return @{ status = "unbound"; binding_id = "stay-workspace-$n"; default_chord = $null; configurable = $true }
    }
    { $_ -in @("focus-directional", "move-directional") } {
      if ("$($Action.direction)" -notin @("left", "right", "up", "down")) { Fail-Ws "$($Action.op) bad direction" }
      $keys = @{ left = "H"; down = "J"; up = "K"; right = "L" }
      $key = $keys["$($Action.direction)"]
      if ("$($Action.op)" -eq "focus-directional") {
        return @{ status = "default"; binding_id = "focus-$($Action.direction)"; default_chord = "Win+$key"; configurable = $true }
      }
      return @{ status = "default"; binding_id = "move-$($Action.direction)"; default_chord = "Win+Shift+$key"; configurable = $true }
    }
    "admit" { return @{ status = "fixture"; binding_id = $null; default_chord = $null; configurable = $false } }
    "close" { return @{ status = "fixture"; binding_id = $null; default_chord = $null; configurable = $false } }
    "focus-window" { return @{ status = "fixture"; binding_id = $null; default_chord = $null; configurable = $false } }
    "checkpoint" { return @{ status = "observation"; binding_id = $null; default_chord = $null; configurable = $false } }
    default { Fail-Ws "unknown op $($Action.op)" }
  }
  return $null
}

function Invoke-KeyboardCoverage {
  # Pure offline shortcut-default report over the shared JSON definitions
  # 1:1 (no forked oracle): -Case selects one case file, otherwise both
  # fixture cases. Alias home/away ride the authored assumption home=1 and
  # away=2. No owner, fixture, settings, or window effect.
  $paths = [System.Collections.ArrayList]@()
  if ($Case -ne "") { $null = $paths.Add($Case) } else {
    $null = $paths.Add((Join-Path $Repo "test-fixtures\world-stress-home-away.json"))
    $null = $paths.Add((Join-Path $Repo "test-fixtures\world-stress-trio-churn.json"))
  }
  $aliasOrd = @{ home = 1; away = 2 }
  $cases = [System.Collections.ArrayList]@()
  foreach ($p in $paths) {
    $case = Get-StressCase $p
    $cmdTotal = Validate-StressCase $case
    $rows = [System.Collections.ArrayList]@()
    $unbound = 0
    foreach ($a in @($case.actions)) {
      $cov = Get-StressShortcutCoverage $a $aliasOrd
      if ("$($cov.status)" -eq "unbound") { $unbound++ }
      $row = @{ op = "$($a.op)"; status = "$($cov.status)"; binding_id = $cov.binding_id; default_chord = $cov.default_chord; configurable = $cov.configurable }
      if ($null -ne $a.seq) { $row.seq = [int]$a.seq }
      $null = $rows.Add($row)
    }
    $null = $cases.Add(@{ name = "$($case.name)"; commands = $cmdTotal; actions = @($case.actions).Count; unbound = $unbound; rows = @($rows) })
  }
  $report = @{
    status = "pass"; stage = "WorldStressKeyboardCoverage";
    alias_assumption = "home=1/away=2 authored assumption; default chords are catalog candidates, not active settings or an injection recipe";
    injection_accepted = $false; blocked_reason = "normal-product-injected-filter";
    cases = @($cases)
  }
  $report | ConvertTo-Json -Depth 8 | Write-Output
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
  #   ForegroundHwnd, ManagedHwnds, Pager (retained renderer entry with dpi
  #   and per-miniature raw source rects `mini_frames`),
  #   PagerOutputsTotal/Omitted, Revision.
  # Hidden members compare the native outer exactly against the owner raw
  # source rect for their token (never the logical tile). Miniature geometry
  # is projected independently from each visible tile (and from the owner raw
  # source while hidden) through the exact renderer contract
  # (pager::miniature) and compared per token plus per-cell token multisets:
  # duplicates, omissions and swaps fail, not just degenerate rects.
  # Desired rects carry {x,y,w,h} widths (world_tick rect_value), never
  # right/bottom edges. Native outer/visible rects are [l,t,r,b].
  $ring = @($Actual.Ring)
  if ($ring.Count -lt 2) { Fail-Ws "ring has fewer than 2 workspaces" }
  if ("$($ring[0])" -ne "$($Actual.HomeId)") { Fail-Ws "home left ring head" }
  if ("$($ring[1])" -ne "$($Actual.AwayId)") { Fail-Ws "away left ring position 2" }
  $viewId = if ($Expected.View -eq "home") { $Actual.HomeId } else { $Actual.AwayId }
  if ("$($Actual.DesiredView)" -ne "$viewId") { Fail-Ws "view $($Actual.DesiredView) != $viewId" }
  $wantList = @()
  foreach ($mid in @($Expected.Members)) {
    $mt = $Actual.Tokens[$mid]
    if ($null -eq $mt -or "$mt" -eq "") { Fail-Ws "null token for $mid" }
    $wantList += "$mt"
  }
  $wantTokens = @($wantList | Sort-Object)
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
    $shownHere = ($Expected.Homing[$id] -eq $Expected.View)
    if ($shownHere) {
      if ([int]$fr.outer[0] -ne [int]$tile[0] -or [int]$fr.outer[1] -ne [int]$tile[1] -or
          [int]$fr.outer[2] -ne ([int]$tile[0] + [int]$tile[2]) -or
          [int]$fr.outer[3] -ne ([int]$tile[1] + [int]$tile[3])) {
        Fail-Ws "$id outer [$($fr.outer -join ',')] != tile [$($tile -join ',')]"
      }
      if (-not $fr.ok_visible) { Fail-Ws "$id visible unreadable while shown" }
      if ($fr.flag -ne $true) { Fail-Ws "$id not visible while shown" }
      if ([int]$fr.visible[0] -ne [int]$fr.outer[0] -or [int]$fr.visible[1] -ne [int]$fr.outer[1] -or
          [int]$fr.visible[2] -ne [int]$fr.outer[2] -or [int]$fr.visible[3] -ne [int]$fr.outer[3]) {
        Fail-Ws "$id visible [$($fr.visible -join ',')] != outer [$($fr.outer -join ',')]"
      }
    } else {
      # Hidden member: exact lifetime identity, hidden visibility, exact
      # owner retained-frame proof. The adapter never writes hidden frames,
      # so exact native geometry is proven only against the actual owner
      # retained frame: the raw source rect the owner used for this token's
      # miniature (`mini_frames` in the revision-stamped pager payload),
      # corroborated by the independently observed native outer under fresh
      # visibility/identity. Never against the logical tile, never copied
      # from the logical target, and never seeded from the native outer
      # itself. A shown resize followed by hide before the next checkpoint
      # accepts: the owner raw at this checkpoint revision is the fresh hide
      # evidence. Reveal reconverges through the shown branch above.
      if ($fr.flag -eq $true) { Fail-Ws "$id visible while hidden" }
      $tok = "$($Actual.Tokens[$id])"
      if ($null -eq $tok -or $tok -eq "") { Fail-Ws "$id null token" }
      $raw = $null
      foreach ($cell in @($Actual.Pager.cells)) {
        $toks = @($cell.mini_windows)
        if (-not (Test-StressHasField $cell "mini_frames")) {
          Fail-Ws "pager hidden $id mini_frames missing"
        }
        $raws = @($cell.mini_frames)
        if ($raws.Count -ne $toks.Count) {
          Fail-Ws "pager mini parallel arrays mismatch at cell $($cell.ord)"
        }
        for ($m = 0; $m -lt $toks.Count; $m++) {
          if ("$($toks[$m])" -eq "$tok") {
            if ($null -ne $raw) { Fail-Ws "$id pager raw token duplicated" }
            $raw = $raws[$m]
          }
        }
      }
      if ($null -eq $raw -or $null -eq $raw.x) { Fail-Ws "$id unknown pager raw frame" }
      $re = @([int]$raw.x, [int]$raw.y, ([int]$raw.x + [int]$raw.w), ([int]$raw.y + [int]$raw.h))
      if ([int]$fr.outer[0] -ne $re[0] -or [int]$fr.outer[1] -ne $re[1] -or
          [int]$fr.outer[2] -ne $re[2] -or [int]$fr.outer[3] -ne $re[3]) {
        if ([int]$Actual.Pager.revision -lt [int]$Actual.Revision) {
          Fail-Ws "pager hidden $id raw revision $([int]$Actual.Pager.revision) older than checkpoint $([int]$Actual.Revision)"
        }
        Fail-Ws "$id hidden outer [$($fr.outer -join ',')] != pager raw [$($re -join ',')]"
      }
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
  # In pager-extended mode the checkpoint proved no newer settled apply
  # exists, so a newer payload (pager-only emission, same owner moment) is
  # the legitimate evidence; otherwise a postdating payload is incoherent.
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
    if (-not (Test-StressHasField $c "mini_frames")) {
      Fail-Ws "pager mini parallel arrays mismatch at cell $i"
    }
    $raws = @($c.mini_frames)
    if ([int]$c.minis_total -ne $marks.Count) { Fail-Ws "pager minis_total mismatch at cell $i" }
    if ($rects.Count -ne $marks.Count -or $toks.Count -ne $marks.Count -or $raws.Count -ne $marks.Count) {
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
    # miniature rect the renderer contract projects from its frame source:
    # the logical tile while visible, the owner raw source rect while
    # hidden (proving the renderer used the claimed source, never the
    # hidden logical tile). Duplicates, omissions and swapped positions
    # all fail here, not just degenerate/escaping rects.
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
      $srcTile = $tile
      if ("$($Expected.Homing[$caller])" -ne "$($Expected.View)") {
        $rf = $raws[$m]
        if ($null -eq $rf -or $null -eq $rf.x) { Fail-Ws "pager hidden $caller raw frame missing at cell $i mini $m" }
        $srcTile = @([int]$rf.x, [int]$rf.y, [int]$rf.w, [int]$rf.h)
      }
      $wantRect = Get-StressExpectedMini $c.rect $monFull $srcTile ([int]$dpi)
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
  if ($Msg -match "not visible while shown") { return $true }
  if ($Msg -match "raw revision \d+ older than checkpoint \d+") { return $true }
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

function Test-StressStalePager([string]$Msg) {
  # Pager-lag family for the probe-or-keep decision (subset of the retry
  # classifier above): the retained payload predates the pinned apply while
  # the resolving emission may ride a newer settled apply (probe must
  # re-settle) or no newer proof at all (probe keeps the pin and the
  # deadline decides). A newer payload without a corresponding settled apply
  # row is never mixed with the pinned world. Native/identity/settlement
  # bounds are excluded: re-polling already re-samples them and no probe
  # can resolve them.
  if ($Msg -match "^pager cells \d+ != ring \d+$") { return $true }
  if ($Msg -match "raw revision \d+ older than checkpoint \d+") { return $true }
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

function Get-StressPubSummary($PubTimes, [int]$DelayMs, [int]$SleepCount) {
  # Pure publication-timing summary over per-command monotonic records.
  # Each record carries seq/op plus Stopwatch start_ms/accepted_ms around
  # queue publication; burst is true only when the previous loop action was
  # also a burst command (no intervening admit/close/focus-window/checkpoint
  # overhead). Per-command dispatch tick/outcome join later at explicit
  # checkpoints in queue order; this summary is timings evidence only with
  # bounded output, never a settlement claim.
  $rows = @($PubTimes | Where-Object { $null -ne $_ })
  $n = $rows.Count
  if ($n -eq 0) {
    return @{ count = 0; delay_ms_each = $DelayMs; added_delay_ms = ([int]$DelayMs * [int]$SleepCount);
      span_ms = 0; rate_per_sec = $null; intervals_ms = @(); burst_intervals_ms = @() }
  }
  $starts = @($rows | ForEach-Object { [double]$_.start_ms })
  $accs = @($rows | ForEach-Object { [double]$_.accepted_ms })
  $span = $accs[$accs.Count - 1] - $starts[0]
  if ($span -lt 0) { $span = 0 }
  $rate = if ($span -gt 0) { [Math]::Round($n / ($span / 1000.0), 3) } else { $null }
  $gaps = [System.Collections.ArrayList]@()
  $bursts = [System.Collections.ArrayList]@()
  for ($i = 1; $i -lt $n; $i++) {
    $g = [Math]::Round($accs[$i] - $accs[$i - 1], 3)
    $null = $gaps.Add($g)
    if ($rows[$i].burst -eq $true) { $null = $bursts.Add($g) }
  }
  return @{
    count = $n; delay_ms_each = $DelayMs
    added_delay_ms = ([int]$DelayMs * [int]$SleepCount)
    span_ms = [Math]::Round($span, 3); rate_per_sec = $rate
    intervals_ms = @($gaps); burst_intervals_ms = @($bursts)
  }
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

function Wait-StressAdmitted([string]$LogPath, $TokOf, [uint64]$Hwnd, $FixtureState, [int]$TimeoutSec, [string]$Tag, $AdmitFence = $null) {
  # Fresh-admission join over production member-admitted rows
  # {window,hwnd,pid,creation,tick}: scans from the log start so bootstrap
  # admissions in any order all bind. Exact fixture PID/creation required.
  # On success records the action-causal admission fence (event tick/index
  # for the exact HWND/PID/creation+token) on the optional fence object
  # (topology or bootstrap fence with LastAdmitTick/LastAdmitIndex keys),
  # analogous to the Drain dispatch high-water mark. The next checkpoint
  # must settle at or past that tick, never on a pre-admission row. Wrong
  # PID/creation fails closed with no fence update. String return preserved.
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    $events = @(Read-StressOwnerLog $LogPath)
    for ($idx = 0; $idx -lt $events.Count; $idx++) {
      $e = $events[$idx]
      if ("$($e.event)" -ne "member-admitted") { continue }
      if ([uint64]$e.hwnd -ne $Hwnd) { continue }
      if ([int]$e.pid -ne [int]$FixtureState.pid) { Fail-Ws "admitted pid mismatch for $Tag" }
      if ("$($e.creation)" -ne "$($FixtureState.creation)") { Fail-Ws "admitted creation mismatch for $Tag" }
      $known = @($TokOf.Values)
      if ($known -contains "$($e.window)") { Fail-Ws "duplicate token for $Tag" }
      if ($null -ne $AdmitFence) {
        try {
          $et = [int]$e.tick
          $cur = 0
          try { $cur = [int]$AdmitFence.LastAdmitTick } catch { $cur = 0 }
          if ($et -gt $cur) { $AdmitFence.LastAdmitTick = $et }
          $AdmitFence.LastAdmitIndex = [int]$idx
        } catch {}
      }
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
    $cursor = $events.Count
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

function Initialize-StressTopology($LogPath, [ref]$Mark, $Case, $TokOf, $BootstrapIds, $AuthoredWork, [int]$AdmitTick = 0) {
  # Baseline observation after bootstrap admissions, before the first
  # command: binds the single native output, the frozen home id, the current
  # away slot (rebinds on lifecycle pruning like the shared simulator), the
  # seen set, the discovered work area, and the current alias ordinals.
  # Verifies the configured zero gaps and the bootstrap roster, skipping
  # earlier fresh rows until the full roster lands. The admission fence tick
  # (max member-admitted tick over the bootstrap binds) is the minimum proof
  # tick, so a pre-roster row can never satisfy the baseline.
  if (@($Case.screens).Count -ne 1) { Fail-Ws "refuse: multi-screen case" }
  $wantList = @()
  foreach ($bid in @($BootstrapIds)) {
    $bt = $TokOf[$bid]
    if ($null -eq $bt -or "$bt" -eq "") { Fail-Ws "topology-init: null token for $bid" }
    $wantList += "$bt"
  }
  $want = @($wantList | Sort-Object)
  $needInitTick = [int]$AdmitTick
  $deadline = (Get-Date).AddSeconds(60)
  $ev = $null
  while ($true) {
    $r = Wait-StressFreshApply $LogPath $Mark $needInitTick 10 "topology-init"
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
    LastDispatchTick = 0; LastAdmitTick = $needInitTick; LastAdmitIndex = -1; HideEpoch = @{}
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

function Test-StressBootstrapLayoutKind($ActualById, $AuthoredById) {
  # Pure pairwise-mirror classifier over caller-id -> [x,y,w,h] width rects.
  # Returns "exact" when every id matches, "pairwise-swap" when exactly two
  # ids carry each other's authored tile, else "other" (the caller refuses;
  # no broader repair is supported).
  $aIds = @($ActualById.Keys | Sort-Object)
  $eIds = @($AuthoredById.Keys | Sort-Object)
  if ($aIds.Count -ne $eIds.Count) { return "other" }
  if (($aIds -join ",") -ne ($eIds -join ",")) { return "other" }
  if ($aIds.Count -eq 0) { return "other" }
  $match = $true
  foreach ($id in $aIds) {
    $a = @($ActualById[$id]); $e = @($AuthoredById[$id])
    if (@($a).Count -ne 4 -or @($e).Count -ne 4) { return "other" }
    if ([int]$a[0] -ne [int]$e[0] -or [int]$a[1] -ne [int]$e[1] -or
        [int]$a[2] -ne [int]$e[2] -or [int]$a[3] -ne [int]$e[3]) { $match = $false; break }
  }
  if ($match) { return "exact" }
  if ($aIds.Count -eq 2) {
    $x = $aIds[0]; $y = $aIds[1]
    $ax = @($ActualById[$x]); $ay = @($ActualById[$y])
    $ex = @($AuthoredById[$x]); $ey = @($AuthoredById[$y])
    if ([int]$ax[0] -eq [int]$ey[0] -and [int]$ax[1] -eq [int]$ey[1] -and
        [int]$ax[2] -eq [int]$ey[2] -and [int]$ax[3] -eq [int]$ey[3] -and
        [int]$ay[0] -eq [int]$ex[0] -and [int]$ay[1] -eq [int]$ex[1] -and
        [int]$ay[2] -eq [int]$ex[2] -and [int]$ay[3] -eq [int]$ex[3]) {
      return "pairwise-swap"
    }
  }
  return "other"
}

function Get-StressBootstrapLayoutState($DesiredAllocs, $NativeWidths, $AuthoredTiles) {
  # Pure setup decision input: classifies the settled desired allocations and
  # reports whether the fresh identity-bound native outers agree per id.
  # Returns @{ desired = "exact"|"pairwise-swap"|"other"; matched = $bool }.
  # The caller refuses on "other" and retries lag while matched is false.
  $kind = Test-StressBootstrapLayoutKind $DesiredAllocs $AuthoredTiles
  $matched = $false
  if ($kind -ne "other") {
    $matched = (Test-StressBootstrapLayoutKind $NativeWidths $DesiredAllocs) -eq "exact"
  }
  return @{ desired = $kind; matched = $matched }
}

function Get-StressBootstrapFixMove([string]$Mover, $ActualRect, $AuthoredRect) {
  # Single-axis step from the mover's proven actual rect toward its authored
  # side. Anything ambiguous (diagonal, zero, malformed) refuses: the setup
  # covers exactly a two-tile pairwise swap, never a search.
  $a = @($ActualRect); $e = @($AuthoredRect)
  if (@($a).Count -ne 4 -or @($e).Count -ne 4) { Fail-Ws "bootstrap fix refuse: malformed rect for $Mover" }
  $dx = [int]$e[0] - [int]$a[0]
  $dy = [int]$e[1] - [int]$a[1]
  if ($dx -gt 0 -and $dy -eq 0) { return "right" }
  if ($dx -lt 0 -and $dy -eq 0) { return "left" }
  if ($dy -gt 0 -and $dx -eq 0) { return "down" }
  if ($dy -lt 0 -and $dx -eq 0) { return "up" }
  Fail-Ws "bootstrap fix refuse: mover $Mover needs no single-axis step"
  return ""
}

function Test-StressBootstrapProofRow($Event, [string]$Tag, [string]$FailWord) {
  # Settled-proof gate for one candidate bootstrap row, mirroring the
  # ReuseSettled/final-probe contract: clean omissions with paired geometry
  # and no pending intent. Pending-worded corruption fails with $FailWord
  # (unproven world past the proof, never a skip here); other corruption
  # rethrows. Never advances past an unproven row.
  try {
    $null = Test-StressApplyClean $Event $Tag
  } catch {
    $msg = "$($_.Exception.Message)"
    if ($msg -match "pending transfers|pending operations|held scopes|present|pending op|floating|authority not controlled|float origin") {
      Fail-Ws "${Tag}: $FailWord"
    }
    throw
  }
  if (Test-StressApplyPending $Event) { Fail-Ws "${Tag}: $FailWord" }
  return $true
}

function Find-StressBootstrapNewer($LogPath, [int]$AfterIndex, $NeedTick, [string]$Tag) {
  # Newer-progress scan after a pinned proof index: repin to the latest clean
  # settled world-apply at/after the fence, fail fast on any newer pending
  # or corrupt row, ignore unrelated lines. Returns $null when nothing newer
  # is relevant. Never mixes unproven rows with the pinned world.
  $events = @(Read-StressOwnerLog $LogPath)
  $found = $null
  for ($ri = $AfterIndex + 1; $ri -lt $events.Count; $ri++) {
    $cand = $events[$ri]
    if ("$($cand.event)" -ne "world-apply") { continue }
    if ("$($cand.delivery)" -ne "accepted" -and "$($cand.delivery)" -ne "command-consumed") { continue }
    if ([int]$cand.tick -lt [int]$NeedTick) { continue }
    $null = Test-StressBootstrapProofRow $cand "$Tag-newer" "newer pending row not settled"
    $found = @{ event = $cand; index = $ri }
  }
  return $found
}

function Read-StressBootstrapProof($LogPath, $NeedTick, [ref]$Cursor, [int]$TimeoutSec, [string]$Tag) {
  # Latest-proof acquisition with ReuseSettled/final-probe semantics: reuse
  # the exact committed row at Cursor-1 when it is a clean settled
  # observation at/after the fence (a quiet owner emits nothing newer, so no
  # wait is needed); otherwise join the next fresh settled row within the
  # bound. Then repin to the latest clean proof (newer pending/corrupt fails
  # fast). Advances Cursor past the pinned row. Never ignores newer progress
  # and never mixes unproven rows with the committed world.
  $tLocal = [int]$Cursor.Value
  $pinned = $null
  $events = @(Read-StressOwnerLog $LogPath)
  $reuseIdx = $tLocal - 1
  if ($reuseIdx -ge 0 -and $reuseIdx -lt $events.Count) {
    $cand = $events[$reuseIdx]
    if ("$($cand.event)" -eq "world-apply" -and
        ("$($cand.delivery)" -eq "accepted" -or "$($cand.delivery)" -eq "command-consumed") -and
        [int]$cand.tick -ge [int]$NeedTick) {
      $null = Test-StressBootstrapProofRow $cand "$Tag-reuse" "committed row not settled"
      $pinned = @{ event = $cand; index = $reuseIdx }
      $tLocal = $reuseIdx + 1
    }
  }
  if ($null -eq $pinned) {
    $tRef = [ref]$tLocal
    $r = Wait-StressFreshApply $LogPath $tRef $NeedTick $TimeoutSec "$Tag-fresh"
    $tLocal = [int]$tRef.Value
    $pinned = @{ event = $r.event; index = ($tLocal - 1) }
  }
  $re = Find-StressBootstrapNewer $LogPath $pinned.index $NeedTick $Tag
  if ($null -ne $re) {
    $pinned = $re
    $tLocal = [int]$re.index + 1
  }
  $Cursor.Value = $tLocal
  return $pinned
}

function Invoke-StressBootstrapLayout($LogPath, $Topo, $TokOf, $HwndOf, $FixtureState, [ref]$Mark, $Case, $AuthoredWork, $OwnerCopy, $LedgerDir, $FixtureExe, $Socket, $Nonce) {
  # Harness-only bootstrap precondition: the runner stages initial_facts
  # physical positions before owner start, then only checks roster/view, so
  # an owner retile can mirror the bootstrap pair (authored win-a right while
  # actual win-a lands left). Establish the exact authored bootstrap layout
  # through the existing exact-owner directional move ONLY when fresh clean
  # desired/native state proves the exact two-tile pairwise swap, reusing the
  # committed proof row when it already covers the fence, repinning to the
  # latest clean proof, and reconfirming after native readbacks; an
  # already-exact layout issues no setup command. Anything else refuses
  # fail-closed: no arbitrary convergence, reordering, search, or other
  # mismatch repair. Setup evidence rides bootstrap-fix/bootstrap-layout
  # labels, outside the 102 stress commands and their publication rate.
  # Two-leaf same-axis moves are R2a SwapNeighbor under the product default
  # group-with-neighbor (plan_local in crates/tiler-core/src/directional.rs;
  # tests r2a_swap_two_child_leaf_neighbor_s1_02_s2_01_s3_01_m1 and
  # flat_swap_leaves_binary_group_and_escape_rules_unchanged), so one move
  # of the misplaced mover toward its authored side swaps the pair.
  $bootIds = @($Case.initial_facts.PSObject.Properties | ForEach-Object { $_.Name } | Sort-Object)
  if ($bootIds.Count -ne 2) { Fail-Ws "bootstrap layout refuse: covers exactly the two-window initial_facts pair" }
  foreach ($id in $bootIds) {
    if ($null -eq $TokOf[$id] -or "$($TokOf[$id])" -eq "") { Fail-Ws "bootstrap layout refuse: null token for $id" }
    if ($null -eq $HwndOf[$id]) { Fail-Ws "bootstrap layout refuse: no hwnd for $id" }
  }
  $facts = @{}
  foreach ($id in $bootIds) { $facts[$id] = @($Case.initial_facts.PSObject.Properties[$id].Value) }
  $authoredTiles = Convert-StressTiles $facts $AuthoredWork $Topo.DiscoveredWork
  $need = [int]$Topo.LastTick
  foreach ($f in @($Topo.LastDispatchTick, $Topo.LastAdmitTick)) {
    try { if ([int]$f -gt $need) { $need = [int]$f } } catch {}
  }
  # Bounded sampling budget shared by proof acquisition and native agreement:
  # remaining-budget accounting below means no fixed inner wait can consume
  # the outer deadline. The later focus (15s), transport-idle (15s) and
  # synthetic-checkpoint (60s) phases keep their existing signatures and
  # bounds and run sequentially after sampling; nothing nests unboundedly.
  $deadline = (Get-Date).AddSeconds(60)
  $tLocal = [int]$Mark.Value
  $pin = $null
  $ev = $null
  $allocs = $null
  $dKind = $null
  $native = $null
  while ($true) {
  if ((Get-Date) -ge $deadline) { Fail-Ws "bootstrap layout not settled: sampling budget exhausted" }
  if ($null -eq $pin) {
    $remain = [int](($deadline - (Get-Date)).TotalSeconds)
    if ($remain -lt 2) { $remain = 2 }
    $pin = Read-StressBootstrapProof $LogPath $need ([ref]$tLocal) $remain "bootstrap-layout"
  }
  $ev = $pin.event
  $null = Refresh-StressRing $ev $Topo
  $desired = @{}
  foreach ($row in @($ev.desired_windows)) { $desired["$($row.window)"] = $row }
  $wantToks = @()
  foreach ($id in $bootIds) { $wantToks += "$($TokOf[$id])" }
  $gotToks = @($desired.Keys | Sort-Object)
  $wantSorted = @($wantToks | Sort-Object)
  if (($gotToks -join ",") -ne ($wantSorted -join ",")) {
    Fail-Ws "bootstrap layout refuse: roster $($gotToks -join ',') != bootstrap $($wantSorted -join ',')"
  }
  $viewRow = @($ev.desired_outputs | Where-Object { "$($_.output)" -eq "$($Topo.NativeOut)" } | Select-Object -First 1)
  if ($viewRow.Count -ne 1 -or "$($viewRow[0].logical_view)" -ne "$($Topo.HomeId)") {
    Fail-Ws "bootstrap layout refuse: bootstrap view is not home"
  }
  $allocs = @{}
  foreach ($id in $bootIds) {
    $row = $desired["$($TokOf[$id])"]
    if ("$($row.out)" -ne "$($Topo.NativeOut)") { Fail-Ws "bootstrap layout refuse: $id out $($row.out)" }
    if ("$($row.ws)" -ne "$($Topo.HomeId)") { Fail-Ws "bootstrap layout refuse: $id not homed home" }
    $al = Get-StressRowField $row "allocation"
    $ef = Get-StressRowField $row "effective_frame"
    if ($null -eq $al -or $null -eq $ef) { Fail-Ws "bootstrap layout refuse: $id missing allocation/effective" }
    if ([int]$al.x -ne [int]$ef.x -or [int]$al.y -ne [int]$ef.y -or
        [int]$al.w -ne [int]$ef.w -or [int]$al.h -ne [int]$ef.h) {
      Fail-Ws "bootstrap layout refuse: $id effective != allocation"
    }
    foreach ($h in @("frame_hold", "membership_hold")) {
      $hv = Get-StressRowField $row $h
      if ($null -ne $hv -and "$hv" -ne "") { Fail-Ws "bootstrap layout refuse: $id $h present" }
    }
    if ($row.floating -ne $false) { Fail-Ws "bootstrap layout refuse: $id floating" }
    if ($null -ne $row.float_origin -and "$($row.float_origin)" -ne "") { Fail-Ws "bootstrap layout refuse: $id float origin" }
    if ("$($row.frame_op)" -ne "" -or "$($row.clear_op)" -ne "") { Fail-Ws "bootstrap layout refuse: $id pending op" }
    if ("$($row.frame_authority)" -ne "controlled" -or "$($row.membership_authority)" -ne "controlled") {
      Fail-Ws "bootstrap layout refuse: $id authority not controlled"
    }
    $allocs[$id] = @([int]$al.x, [int]$al.y, [int]$al.w, [int]$al.h)
  }
  $dKind = Test-StressBootstrapLayoutKind $allocs $authoredTiles
  if ($dKind -eq "other") {
    Fail-Ws "bootstrap layout refuse: desired is neither exact nor a two-tile pairwise swap"
  }
  # Fresh identity-bound native outers must agree with the settled desired
  # before any setup effect; actuation lag retries to the deadline while
  # identity mismatches refuse immediately.
  $native = $null
  $nErr = "no native sample yet"
  while ((Get-Date) -lt $deadline) {
    try {
      $cand = @{}
      $titles = Get-StressTitles
      foreach ($id in $bootIds) {
        $hwnd = Get-StressHwndOfCaller $titles "$id"
        if ($null -eq $hwnd) { throw "bootstrap-layout lag: no hwnd for $id" }
        if ("$hwnd" -ne "$($HwndOf[$id])") { Fail-Ws "bootstrap layout refuse: hwnd changed for $id" }
        $null = Test-StressHwndOwner ([uint64]$hwnd) $FixtureState "bootstrap-layout-$id"
        $pr = Read-LivePaired ([uint64]$hwnd)
        if (-not $pr.alive) { Fail-Ws "bootstrap layout refuse: $id not alive" }
        if (-not $pr.ok_outer) { Fail-Ws "bootstrap layout refuse: $id outer unreadable" }
        if (-not $pr.ok_visible) { Fail-Ws "bootstrap layout refuse: $id visible unreadable while shown" }
        if ($pr.visible_flag -ne $true) { throw "bootstrap-layout lag: $id not visible while shown" }
        $o = @($pr.outer); $v = @($pr.visible)
        if ([int]$v[0] -ne [int]$o[0] -or [int]$v[1] -ne [int]$o[1] -or
            [int]$v[2] -ne [int]$o[2] -or [int]$v[3] -ne [int]$o[3]) {
          throw "bootstrap-layout lag: $id visible != outer"
        }
        $a = @($allocs[$id])
        if ([int]$o[0] -ne [int]$a[0] -or [int]$o[1] -ne [int]$a[1] -or
            [int]$o[2] -ne ([int]$a[0] + [int]$a[2]) -or [int]$o[3] -ne ([int]$a[1] + [int]$a[3])) {
          throw "bootstrap-layout lag: $id outer [$($o -join ',')] != allocation [$($a -join ',')]"
        }
        $cand[$id] = @([int]$o[0], [int]$o[1], ([int]$o[2] - [int]$o[0]), ([int]$o[3] - [int]$o[1]))
      }
      $native = $cand
      break
    } catch {
      $nErr = "$($_.Exception.Message)"
      if ($nErr -notmatch "bootstrap-layout lag:") { throw }
      Start-Sleep -Milliseconds 200
    }
  }
  if ($null -eq $native) { Fail-Ws "bootstrap layout not settled: $nErr" }
  $st = Get-StressBootstrapLayoutState $allocs $native $authoredTiles
  if ($st.desired -ne $dKind -or -not $st.matched) {
    Fail-Ws "bootstrap layout refuse: native/desired do not prove $dKind"
  }
  # Reconfirm after native readbacks and before any setup actuation: rows
  # that landed during sampling repin to the latest clean proof (re-derive
  # and re-verify within the deadline); newer pending/corrupt rows fail
  # fast. Never act from a superseded desired.
  $re = Find-StressBootstrapNewer $LogPath $pin.index $need "bootstrap-layout"
  if ($null -eq $re) { break }
  $pin = $re
  $tLocal = [int]$re.index + 1
  $ev = $null
  $allocs = $null
  $dKind = $null
  $native = $null
  }
  if ($dKind -eq "exact") {
    $Mark.Value = $tLocal
    $Topo.LastTick = [int]$ev.tick
    if ([int]$ev.revision -gt [int]$Topo.Revision) { $Topo.Revision = [int]$ev.revision }
    Rec-Ws "bootstrap-layout" @{ layout = "exact"; revision = [int]$ev.revision; tick = [int]$ev.tick }
    return
  }
  # Pairwise swap: one exact-owner directional move of the misplaced mover
  # toward its authored side. Explicit fixture focus first restores the focus
  # contract (Invoke-StressFocus guards abort when blocked); the CLI route
  # carries op/direction only and the owner resolves the live foreground.
  $mover = $bootIds[0]
  $dir = Get-StressBootstrapFixMove $mover @($allocs[$mover]) @($authoredTiles[$mover])
  Invoke-StressFocus $FixtureExe $Socket $Nonce $mover $HwndOf $FixtureState 15
  $argv = @("directional", "--move", "--direction", "$dir")
  Wait-StressTransportIdle $LedgerDir "bootstrap-fix"
  $null = Invoke-StressCli $OwnerCopy $argv $LedgerDir "bootstrap-fix"
  Rec-Ws "bootstrap-fix" @{ mover = $mover; direction = $dir; argv = ($argv -join " ") }
  $null = $Topo.Pending.Add(@{ argv = $argv; op = "bootstrap-fix-move" })
  $fixTiles = [pscustomobject]@{}
  foreach ($id in $bootIds) { $fixTiles | Add-Member -NotePropertyName "$id" -NotePropertyValue @($facts[$id]) }
  $fixHoming = [pscustomobject]@{}
  foreach ($id in $bootIds) { $fixHoming | Add-Member -NotePropertyName "$id" -NotePropertyValue "home" }
  $fixAction = [pscustomobject]@{
    op = "checkpoint"; name = "bootstrap-layout"; view = "home"; members = @($bootIds);
    homing = $fixHoming; tiles = $fixTiles; focus = "$mover";
    foreground = "$mover"; pager_current = "home"; pager_occupied = @("home")
  }
  $fixPagerMark = 0
  Invoke-StressCheckpoint $fixAction $Case $AuthoredWork $Topo $TokOf $HwndOf $FixtureState $Mark ([ref]$fixPagerMark) $LogPath $OwnerCopy
}

function Wait-StressReady([string]$OwnerCopy, [string]$Name, [int]$TimeoutSec) {
  # Bounded-patience liveness probe: burst publication queues commands without
  # per-command dispatch waits, so at checkpoint entry the owner may not yet
  # have consumed/actuated them and `ready` (which verifies per-window claim
  # snapshots including hidden state) can legitimately read false. Retry
  # `ready:false` to the deadline; unreadable or refused replies throw
  # immediately, and a still-not-ready owner fails closed with the same
  # message (never transient, never a repair).
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    $candidate = Invoke-LiveNative $OwnerCopy @("ready") | ConvertFrom-Json
    if ($candidate.ready -eq $true) { return $candidate }
    Start-Sleep -Milliseconds 250
  }
  Fail-Ws "owner not ready at checkpoint $Name"
}

function Invoke-StressCheckpoint($Action, $Case, $AuthoredWork, $Topo, $TokOf, $HwndOf, $FixtureState, [ref]$Mark, [ref]$PagerMark, $LogPath, $OwnerCopy, [scriptblock]$OnRetry = $null, [int]$TimeoutSec = 60, [switch]$ReuseSettled) {
  # Bounded checkpoint loop: drain queued burst dispatches once (causal
  # delivery over the dedicated dispatch cursor; the apply cursor is never
  # consumed by dispatch joins), then repeat settled-apply join, fresh
  # native sampling (HWND titles re-resolved and PID/creation revalidated)
  # and retained pager join until Compare passes or the deadline expires.
  # Transient native/pager lag retries against the same or a newer settled
  # row; corruption, missing evidence and identity mismatches fail closed
  # immediately. Commits the apply cursor and topology only on success, so a
  # retry never discards unconsumed evidence.
  $probe = Wait-StressReady $OwnerCopy "$($Action.name)" $TimeoutSec
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
  # Action-causal admission fence, analogous to the dispatch high-water mark:
  # every admit/close/focus observation binds before the next checkpoint, and
  # the checkpoint world proof must cover the latest admission tick (from the
  # real member-admitted event for the exact HWND/PID/creation+token), never
  # a pre-admission row. No per-command convergence; the existing explicit
  # observation action fences with no change to the expected oracle.
  $admitFence = 0
  try { if ($null -ne $Topo.LastAdmitTick) { $admitFence = [int]$Topo.LastAdmitTick } } catch { $admitFence = 0 }
  if ($admitFence -gt $needTick) { $needTick = $admitFence }
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  $tMark = [int]$Mark.Value
  $ev = $null
  $resettled = $false
  $lastErr = "no settled row yet"
  $iter = 0
  while ((Get-Date) -lt $deadline) {
    $iter++
    if ($null -ne $OnRetry) { & $OnRetry $iter }
    if ($null -eq $ev) {
      if ($ReuseSettled) {
        # Final re-verification: inspect every newer accepted/
        # command-consumed row after the committed index. Repin to the
        # latest relevant settled proof; fail fast on any newer pending or
        # corrupt row (unproven world past the commit); only with no newer
        # rows at all, re-verify the committed row. Never ignores newer
        # progress and never mixes unproven rows with the committed world.
        $reuseIdx = $tMark - 1
        $logEvents = @(Read-StressOwnerLog $LogPath)
        if ($reuseIdx -lt 0 -or $reuseIdx -ge $logEvents.Count) { Fail-Ws "checkpoint $($Action.name): no committed settled row" }
        $ev = $logEvents[$reuseIdx]
        if ("$($ev.event)" -ne "world-apply") { Fail-Ws "checkpoint $($Action.name): committed row not settled" }
        if ("$($ev.delivery)" -ne "accepted" -and "$($ev.delivery)" -ne "command-consumed") { Fail-Ws "checkpoint $($Action.name): committed row not settled" }
        if ([int]$ev.tick -lt $needTick) { Fail-Ws "checkpoint $($Action.name): committed row behind dispatch fence" }
        try {
          $null = Test-StressApplyClean $ev "$($Action.name)-reuse"
        } catch {
          Fail-Ws "checkpoint $($Action.name): committed row not settled"
        }
        if (Test-StressApplyPending $ev) { Fail-Ws "checkpoint $($Action.name): committed row not settled" }
        $repin = $null
        for ($ri = $tMark; $ri -lt $logEvents.Count; $ri++) {
          $cand = $logEvents[$ri]
          if ("$($cand.event)" -ne "world-apply") { continue }
          if ("$($cand.delivery)" -ne "accepted" -and "$($cand.delivery)" -ne "command-consumed") { continue }
          if ([int]$cand.tick -lt $needTick) { continue }
          try {
            $null = Test-StressApplyClean $cand "$($Action.name)-reuse-newer"
          } catch {
            $msg = "$($_.Exception.Message)"
            if ($msg -match "pending transfers|pending operations|held scopes|present|pending op|floating|authority not controlled|float origin") {
              Fail-Ws "checkpoint $($Action.name): newer pending row not settled"
            }
            throw
          }
          if (Test-StressApplyPending $cand) { Fail-Ws "checkpoint $($Action.name): newer pending row not settled" }
          $repin = @{ event = $cand; index = $ri }
        }
        if ($null -ne $repin) {
          $ev = $repin.event
          $tMark = $repin.index + 1
          $resettled = $true
        }
      } else {
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
      }
      if ([int]$ev.tick -lt $maxDispTick) {
        $lastErr = "apply tick $([int]$ev.tick) behind last dispatched tick $maxDispTick"
        $ev = $null
        $resettled = $true
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
      # Hide-epoch stability: while continuously hidden under the same exact
      # token/lifetime, the native outer must still read the epoch frame
      # (proves no hidden writes and no stale earlier proof). A new hide
      # (revealed since, or re-admitted under a new token/creation) drops
      # the old epoch: the fresh hide evidence (owner raw at this revision,
      # proven in Compare) supersedes, so a shown resize followed by hide
      # before the next checkpoint accepts the resized frame. Epoch updates
      # commit only with the checkpoint below, so a retry never advances
      # evidence. Close drops the epoch explicitly at the close site.
      if ($null -eq $Topo.HideEpoch) { $Topo.HideEpoch = @{} }
      foreach ($id in @($Action.members)) {
        $tokHere = $TokOf[$id]
        $isShown = ("$($expected.Homing[$id])" -eq "$($expected.View)")
        if ($isShown) { continue }
        $ep = $Topo.HideEpoch[$id]
        if ($null -ne $ep -and $null -ne $ep.rect -and $null -ne $ep.token) {
          if ("$($ep.token)" -ne "$tokHere" -or "$($ep.creation)" -ne "$($FixtureState.creation)") {
            $Topo.HideEpoch.Remove($id)
            continue
          }
          $er = @($ep.rect)
          $co = @($frames[$id].outer)
          if ([int]$co[0] -ne [int]$er[0] -or [int]$co[1] -ne [int]$er[1] -or
              [int]$co[2] -ne [int]$er[2] -or [int]$co[3] -ne [int]$er[3]) {
            Fail-Ws "checkpoint $($Action.name): $id hide epoch drift [$($co -join ',')] != [$($er -join ',')]"
          }
        }
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
      # Commit the hide epoch with the checkpoint: hidden members record the
      # proven frame, revealed members leave the epoch. A re-admitted caller
      # binds a new token/creation at admission, so no old proof carries.
      foreach ($id in @($Action.members)) {
        $isShown = ("$($expected.Homing[$id])" -eq "$($expected.View)")
        if ($isShown) {
          if ($Topo.HideEpoch.ContainsKey($id)) { $Topo.HideEpoch.Remove($id) }
        } else {
          $co = @($frames[$id].outer)
          $Topo.HideEpoch[$id] = @{
            rect = @([int]$co[0], [int]$co[1], [int]$co[2], [int]$co[3])
            token = "$($TokOf[$id])"; creation = "$($FixtureState.creation)"
          }
        }
      }
      $Mark.Value = $tMark
      $Topo.LastTick = [int]$ev.tick
      if ($candRev -gt [int]$Topo.Revision) { $Topo.Revision = $candRev }
      Rec-Ws "checkpoint" @{ name = "$($Action.name)"; revision = $candRev; iterations = $iter; pager = [int]$pager.entry.revision; resettled = $resettled }
      return
    } catch {
      $lastErr = "$($_.Exception.Message)"
      if (-not (Test-StressTransientMismatch $lastErr)) { throw }
      # Stale retained pager under a pinned apply: the matching payload may
      # ride a newer settled apply. Probe non-consumingly for one: when found,
      # drop the pinned row and re-settle; otherwise keep the pin and re-poll
      # under the same revision cap. A newer pager without a corresponding
      # settled apply row is never evidence: re-polling never mixes it with
      # the pinned world, and a genuinely late pair still fails at the
      # deadline via settlement timeout. Non-timeout probe failures throw
      # immediately.
      if (Test-StressStalePager $lastErr) {
        $probeMark = $tMark
        $newer = $null
        try {
          $newer = Wait-StressFreshApply $LogPath ([ref]$probeMark) $needTick 2 "$($Action.name)-probe"
        } catch {
          if ("$($_.Exception.Message)" -notmatch "settlement timeout") { throw }
        }
        if ($null -ne $newer) { $ev = $null; $resettled = $true }
      }
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
  # output fail loudly. A newer payload without a corresponding settled
  # apply row is never mixed with an older pinned world: the cap excludes
  # it, and only a re-settle to its own settled row can admit it.
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
    $pubSleeps = 0
    if ($DelayMs -gt 0) { Start-Sleep -Milliseconds $DelayMs; $pubSleeps++ }
    $mark = 0
    $pagerMark = 0
    $tokOf = @{}
    $hwndOf = @{}
    $bootstrapFence = @{ LastAdmitTick = 0; LastAdmitIndex = -1 }
    foreach ($id in $boot.ids) {
      $hwnd = Wait-StressHwnd "$id" @{ pid = $serverPid; creation = $serverCreation } 30
      $hwndOf[$id] = $hwnd
    }
    foreach ($id in $boot.ids) {
      $tok = Wait-StressAdmitted $logPath $tokOf $hwndOf[$id] @{ pid = $serverPid; creation = $serverCreation } 30 "$id" $bootstrapFence
      $tokOf[$id] = $tok
      Rec-Ws "admit" @{ id = $id; hwnd = "$($hwndOf[$id])"; token = $tok }
    }
    $topo = Initialize-StressTopology $logPath ([ref]$mark) $case $tokOf $boot.ids $authoredWork ([int]$bootstrapFence.LastAdmitTick)
    # Harness-only bootstrap precondition: establish the exact authored
    # bootstrap layout through one exact-owner directional move when fresh
    # state proves the pairwise swap, or no setup command when already
    # exact. Runs before the publication clock starts, so setup stays
    # outside the 102 stress commands and their rate.
    $null = Invoke-StressBootstrapLayout $logPath $topo $tokOf $hwndOf @{ pid = $serverPid; creation = $serverCreation } ([ref]$mark) $case $authoredWork $ownerCopy $ledgerDir $fixtureCopy $sock $nonce
    # Pager scan starts at zero: the matching line for the init revision may
    # precede the init row in the log; revisions only grow so the latest
    # match stays correct while checkpoints advance monotonically.
    $pagerMark = 0
    $pubSw = [System.Diagnostics.Stopwatch]::StartNew()
    $pubTimes = [System.Collections.ArrayList]@()
    $pubPrevBurst = $false
    foreach ($action in @($case.actions)) {
      if ($DelayMs -gt 0) { Start-Sleep -Milliseconds $DelayMs; $pubSleeps++ }
      switch ($action.op) {
        "admit" {
          $pubPrevBurst = $false
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
          $tok = Wait-StressAdmitted $logPath $tokOf $hwnd @{ pid = $serverPid; creation = $serverCreation } 30 $id $topo
          $tokOf[$id] = $tok
          Rec-Ws "admit" @{ id = $id; hwnd = "$hwnd"; token = $tok }
        }
        "close" {
          $pubPrevBurst = $false
          $id = "$($action.window)"
          $null = Drain-StressPending $logPath $topo 30 "close-$id"
          $r = Send-StressFixture $fixtureCopy $sock $nonce (@{ v = 1; request_id = (New-StressReqId "close-$id"); op = "close"; ids = @($id) } | ConvertTo-Json -Depth 6 -Compress)
          $entry = @($r.result.windows | Where-Object { "$($_.id)" -eq $id })
          if ($entry.Count -ne 1 -or $entry[0].ok -ne $true) { Fail-Ws "close $id refused" }
          $topo.LastTick = Wait-StressDeparted $logPath ([ref]$mark) $tokOf[$id] $id 30
          $tokOf.Remove($id)
          $hwndOf.Remove($id)
          if ($null -ne $topo.HideEpoch) { $topo.HideEpoch.Remove($id) }
          Rec-Ws "close" @{ id = $id }
        }
        "focus-window" {
          $pubPrevBurst = $false
          $id = "$($action.window)"
          $null = Drain-StressPending $logPath $topo 30 "focus-$id"
          Invoke-StressFocus $fixtureCopy $sock $nonce $id $hwndOf @{ pid = $serverPid; creation = $serverCreation } 15
          Rec-Ws "focus" @{ id = $id }
        }
        "checkpoint" {
          $pubPrevBurst = $false
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
          $pubStart = $pubSw.Elapsed.TotalMilliseconds
          $null = Invoke-StressCli $ownerCopy $argv $ledgerDir "$($action.op)"
          $pubAccepted = $pubSw.Elapsed.TotalMilliseconds
          $null = $pubTimes.Add(@{ seq = [int]$action.seq; op = "$($action.op)";
            start_ms = [Math]::Round($pubStart, 3); accepted_ms = [Math]::Round($pubAccepted, 3);
            burst = $pubPrevBurst })
          $pubPrevBurst = $true
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
    Invoke-StressCheckpoint $finalAction $case $authoredWork $topo $tokOf $hwndOf @{ pid = $serverPid; creation = $serverCreation } ([ref]$mark) ([ref]$pagerMark) $logPath $ownerCopy -ReuseSettled
    Invoke-StressCleanup $ctx "world-stress"
    $cleaned = $true
    $pubSummary = Get-StressPubSummary $pubTimes $DelayMs $pubSleeps
    $pubReport = @{ delay_ms = $DelayMs; summary = $pubSummary; commands = @($pubTimes) }
    $report = @{ status = "pass"; stage = "WorldStress"; started = (Get-Date).ToString("o"); provenance = $prov; steps = $WS_STEPS; publication = $pubReport }
    $report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $reportPath
    Write-Output "world-stress-report=$reportPath"
    Write-Output "publication count=$($pubSummary.count) rate_per_sec=$($pubSummary.rate_per_sec) span_ms=$($pubSummary.span_ms) delay_each_ms=$DelayMs added_delay_ms=$($pubSummary.added_delay_ms) burst_gaps=$(@($pubSummary.burst_intervals_ms).Count)"
    Write-Output "status=pass"
  } catch {
    $pubSummary = $null
    try { $pubSummary = Get-StressPubSummary $pubTimes $DelayMs $pubSleeps } catch { $pubSummary = @{ count = -1; error = "$($_.Exception.Message)" } }
    $report = @{ status = "fail"; stage = "WorldStress"; provenance = $prov; steps = $WS_STEPS; publication = $pubSummary; error = "$($_.Exception.Message)" }
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

if ($KeyboardCoverage -and ($Live -or $Stop -or $Mock)) { Fail-Ws "refuse: -KeyboardCoverage conflicts with -Live/-Stop/-Mock" }
if ($KeyboardCoverage -and $RunDir -ne "") { Fail-Ws "refuse: -KeyboardCoverage takes no -RunDir" }
if ($KeyboardCoverage -and ($DelayMs -ne 0 -or $OwnerSeconds -ne 600)) { Fail-Ws "refuse: -KeyboardCoverage takes no timing args" }
if ($KeyboardCoverage) { Invoke-KeyboardCoverage; exit 0 }
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
if ($Live) { Invoke-LiveJourney; exit 0 }
if ("$($MyInvocation.InvocationName)" -eq ".") { return }
Write-Output "windows-world-stress parsed (no action without -Mock/-Live/-Stop)"
