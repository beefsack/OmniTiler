# Windows W2 live proof: exact ArrivalBracket evidence for cross-output and
# hidden (parked) arrivals, plus non-normal show-state suppression recovery.
#
# Scope (fixture-only; this file grants no live authority):
#   * one private copied `tile` owner built by the shared Phase 1 staging,
#     launched with `--scope-exe tiler-live-fixture.exe --trace`;
#   * only fixture windows spawned by this run, focused through the fixture's
#     own focus primitive (no clicks, no SendInput, no keybd_event);
#   * no cursor warps, no registry writes, no settings writes beyond the
#     shared byte-exact preimage restore in the abort-safe cleanup;
#   * no synthetic input and no broad process cleanup.
#
# What is proven:
#   A1  Cross-output `send-output --follow` arrival: the world-apply diag's
#       paired `arrival_brackets` row for the mover carries `kind=output`,
#       `out=<destination>`, `foreground_proven=true`, and before/after
#       visibility samples that are visible, lifetime-matched, not
#       minimized, not host-hidden, and not suppression-owned; the published
#       effective frame and the independent DWM visible frame land on the
#       destination output.
#   A2  Hidden (parked) arrival: a same-output `workspace --stay 0` parks the
#       focused mover on a hidden workspace (durable hide claim), and a
#       numbered `workspace --select` reveal produces the same exact paired
#       embodiment row plus the trace hide post followed by the show post;
#       the durable claim is retired.
#   B   Non-normal suppression recovery: a normal, a maximized, and a
#       minimized managed window are hidden by the combined view switch
#       (empty trailing select). The recovery ledger must carry each window's
#       exact show preimage {minimized,maximized}; after killing only the
#       exact bound owner, the product's own crash watcher restores them to
#       normal / maximized / minimized native state. This driver never runs
#       `restore` after the crash.
#
# Delivery is observed, never inferred: accepted/command-consumed world rows
# are fresh observations; `duplicate` and cached diagnostics are not used.
# The `visibility_posts` rows are trace-only evidence of the executed host
# call; they never replace the native read-back.
#
# Usage (repo root; requires the standing live-test authorization, no other
# live owner, and built `target/debug/tiler-windows.exe` plus
# `target/debug/tiler-live-fixture.exe`):
#   pwsh -NoProfile -File scripts/windows-w2-arrival-suppression.ps1 -Live
#
# Without `-Live` this file only refuses; it performs no staging or native
# call.

[CmdletBinding()]
param(
  [switch]$Live
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $Live) {
  throw "refuse: explicit -Live required (fixture-scoped Windows live proof)"
}

# windows-world-stress.ps1 carries its own param block, so dot-sourcing the
# Phase 1 definitions rebinds same-named caller variables (including $Live).
# The explicit gate above already ran; never consult $Live after this point.
. (Join-Path $PSScriptRoot "windows-phase1-proofs.ps1") -DefinitionsOnly

if (-not ("W2Native" -as [type])) {
  $w2cs = @'
using System;
using System.Runtime.InteropServices;
[StructLayout(LayoutKind.Sequential)] public struct W2Point { public int X; public int Y; }
public static class W2Native {
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out W2Point p);
}
'@
  Add-Type -TypeDefinition $w2cs | Out-Null
}

# --- W2-only helpers -------------------------------------------------------

function Get-W2Array($Object, [string]$Name) {
  $value = Get-P1Prop $Object $Name
  if ($null -eq $value) { return @() }
  return @($value)
}

function Test-W2PointInRect($Rect, [int]$X, [int]$Y) {
  if ($null -eq $Rect) { return $false }
  $rx = [int](Get-P1Prop $Rect "x")
  $ry = [int](Get-P1Prop $Rect "y")
  $rw = [int](Get-P1Prop $Rect "w")
  $rh = [int](Get-P1Prop $Rect "h")
  return ($X -ge $rx -and $X -lt ($rx + $rw) -and
    $Y -ge $ry -and $Y -lt ($ry + $rh))
}

function Test-W2RectInside($Inner, $Outer, [int]$Tol = 0) {
  if ($null -eq $Inner -or $null -eq $Outer) { return $false }
  $ix = [int](Get-P1Prop $Inner "x")
  $iy = [int](Get-P1Prop $Inner "y")
  $iw = [int](Get-P1Prop $Inner "w")
  $ih = [int](Get-P1Prop $Inner "h")
  $ox = [int](Get-P1Prop $Outer "x")
  $oy = [int](Get-P1Prop $Outer "y")
  $ow = [int](Get-P1Prop $Outer "w")
  $oh = [int](Get-P1Prop $Outer "h")
  return ($ix -ge ($ox - $Tol) -and $iy -ge ($oy - $Tol) -and
    ($ix + $iw) -le ($ox + $ow + $Tol) -and ($iy + $ih) -le ($oy + $oh + $Tol))
}

function Get-W2Center($Rect) {
  if ($null -eq $Rect) { return @(0, 0) }
  return @(
    ([int](Get-P1Prop $Rect "x") + [int]([math]::Round([int](Get-P1Prop $Rect "w") / 2.0))),
    ([int](Get-P1Prop $Rect "y") + [int]([math]::Round([int](Get-P1Prop $Rect "h") / 2.0)))
  )
}

function Get-W2Direction($From, $To) {
  $cf = Get-W2Center $From
  $ct = Get-W2Center $To
  $dx = $ct[0] - $cf[0]
  $dy = $ct[1] - $cf[1]
  if ([math]::Abs($dx) -ge [math]::Abs($dy)) {
    if ($dx -ge 0) { return "right" } else { return "left" }
  }
  if ($dy -ge 0) { return "down" } else { return "up" }
}

function Get-W2CursorPoint {
  # Physical cursor position under the same PerMonitorV2 thread context the
  # product geometry rows use. Read-only; never warps.
  $prior = [P1Native]::GetThreadDpiAwarenessContext()
  try {
    $null = [P1Native]::SetThreadDpiAwarenessContext([IntPtr](-4))
    $point = New-Object W2Point
    if (-not [W2Native]::GetCursorPos([ref]$point)) {
      throw "GetCursorPos failed"
    }
    return @([int]$point.X, [int]$point.Y)
  } finally {
    $null = [P1Native]::SetThreadDpiAwarenessContext($prior)
  }
}

function Get-W2RingOrdinal($Apply, [string]$Out, [string]$Ws) {
  # The core's numbered selection ring is the report's ordered workspace rows
  # for one output, filtered by its live output membership (the shared-core
  # conformance tests build the same ring). Position + 1 is the CLI ordinal;
  # do not substitute the persisted `order` field.
  $rows = @(Get-W2Array $Apply "desired_workspaces" |
      Where-Object { "$(Get-P1Prop $_ 'output')" -eq $Out })
  for ($i = 0; $i -lt $rows.Count; $i++) {
    if ("$(Get-P1Prop $rows[$i] 'id')" -eq $Ws) { return ($i + 1) }
  }
  return 0
}

function Get-W2ArrivalRows($Apply, [string]$Token) {
  $rows = @(Get-W2Array $Apply "arrival_brackets" |
      Where-Object { "$(Get-P1Prop $_ 'window')" -eq $Token })
  return $rows
}

function Test-W2ArrivalSamples($Row) {
  # Exact paired samples: a ready bracket is visible/lifetime-proven on both
  # reads, not minimized, not host-hidden, and not still suppression-owned.
  $before = Get-P1Prop $Row "before"
  $after = Get-P1Prop $Row "after"
  if ($null -eq $before -or $null -eq $after) { return $false }
  foreach ($sample in @($before, $after)) {
    if ((Get-P1Prop $sample "lifetime_matched") -ne $true) { return $false }
    if ((Get-P1Prop $sample "visible") -ne $true) { return $false }
    if ((Get-P1Prop $sample "minimized") -ne $false) { return $false }
    if ((Get-P1Prop $sample "host_hidden") -ne $false) { return $false }
    if ((Get-P1Prop $sample "suppression_owned") -ne $false) { return $false }
  }
  return $true
}

function Get-W2Posts($Apply) {
  return @(Get-W2Array $Apply "visibility_posts")
}

function Read-W2Ledger([string]$Dir) {
  $path = Join-Path $Dir "ledger.json"
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return $null }
  try {
    $text = [IO.File]::ReadAllText($path)
    if ([string]::IsNullOrWhiteSpace($text)) { return $null }
    return ($text | ConvertFrom-Json)
  } catch {
    return $null
  }
}

function Get-W2LedgerRow($Ledger, [uint64]$Hwnd) {
  if ($null -eq $Ledger) { return $null }
  foreach ($row in (Get-W2Array $Ledger "windows")) {
    $value = Get-P1Prop $row "hwnd"
    if ($null -eq $value) { continue }
    if ([uint64]$value -eq $Hwnd) { return $row }
  }
  return $null
}

function Test-W2LedgerShow($Row, [bool]$Minimized, [bool]$Maximized) {
  if ($null -eq $Row) { return $false }
  $show = Get-P1Prop $Row "show"
  if ($null -eq $show) { return $false }
  return ((Get-P1Prop $show "minimized") -eq $Minimized -and
    (Get-P1Prop $show "maximized") -eq $Maximized)
}

function Test-W2LedgerHidden($Row) {
  # Durable product-claim preimage row (kind is the v4 discriminator).
  return ($null -ne $Row -and "$(Get-P1Prop $Row 'kind')" -eq "product")
}

function Set-W2FixtureMode([string]$FixtureExe, [string]$Socket, [string]$Nonce,
    [string]$Id, [string]$Mode) {
  $body = @{
    v = 1; request_id = (New-StressReqId "mode-$Id"); op = "set"; ids = @($Id)
    patch = @{ mode = $Mode }
  } | ConvertTo-Json -Depth 6 -Compress
  $reply = Send-StressFixture $FixtureExe $Socket $Nonce $body
  if ((Get-P1Prop $reply "ok") -ne $true) {
    throw "fixture set mode $Mode refused for ${Id}: $($reply | ConvertTo-Json -Compress -Depth 8)"
  }
  $entries = @(Get-W2Array (Get-P1Prop $reply "result") "windows" |
      Where-Object { "$(Get-P1Prop $_ 'id')" -eq $Id })
  if ($entries.Count -ne 1) {
    throw "fixture set mode $Mode returned no unique entry for ${Id}: $($reply | ConvertTo-Json -Compress -Depth 8)"
  }
  $entry = $entries[0]
  if ((Get-P1Prop $entry "ok") -ne $true) {
    throw "fixture set mode $Mode failed for ${Id}: $($reply | ConvertTo-Json -Compress -Depth 8)"
  }
  $modeFields = @(Get-W2Array $entry "fields" |
      Where-Object { "$(Get-P1Prop $_ 'field')" -eq "mode" })
  if ($modeFields.Count -ne 1 -or "$(Get-P1Prop $modeFields[0] 'status')" -ne "accepted") {
    throw "fixture mode $Mode was not accepted for ${Id}: $($reply | ConvertTo-Json -Compress -Depth 8)"
  }
  return $entry
}

function Send-W2Output([string]$OwnerCopy, [string]$Direction) {
  $ack = Invoke-LiveNative $OwnerCopy @("send-output", "--follow", "--direction", $Direction) |
    ConvertFrom-Json
  if ((Get-P1Prop $ack "dispatched") -ne $true) {
    throw "send-output --follow --direction $Direction was not dispatched: $($ack | ConvertTo-Json -Compress)"
  }
  return $ack
}

function Invoke-W2Workspace([string]$OwnerCopy, [string]$Verb, [string]$Index) {
  $ack = Invoke-LiveNative $OwnerCopy @("workspace", $Verb, $Index) | ConvertFrom-Json
  if ((Get-P1Prop $ack "dispatched") -ne $true) {
    throw "workspace $Verb $Index was not dispatched: $($ack | ConvertTo-Json -Compress)"
  }
  return $ack
}

# --- proof body ------------------------------------------------------------

function Invoke-W2ArrivalSuppression {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $runDir = Join-Path $script:P1TargetDir "task9\w2-arrival-suppression-$stamp-$PID"
  New-Item -ItemType Directory -Force -Path $runDir | Out-Null
  $settingsPath = $script:P1SettingsPath
  $origPath = Join-Path $runDir "settings-orig.json"
  $spiPath = Join-Path $runDir "taskbar-spi.json"
  $evidencePath = Join-Path $runDir "evidence.json"

  $ctx = New-P1Context $script:P1FixtureCopy $settingsPath
  $evidence = [ordered]@{
    proof = "w2-arrival-suppression"
    started_utc = (Get-Date).ToUniversalTime().ToString("o")
    head = ((& git -C $script:P1Repo rev-parse HEAD) | Out-String).Trim()
    git_status = ((& git -C $script:P1Repo status --short) | Out-String).Trim()
    owner_sha256 = (Get-FileHash -LiteralPath $script:P1OwnerCopy -Algorithm SHA256).Hash
    fixture_sha256 = (Get-FileHash -LiteralPath $script:P1FixtureCopy -Algorithm SHA256).Hash
    safety = [ordered]@{
      fixture_only_scope = "--scope-exe tiler-live-fixture.exe"
      synthetic_input = $false
      cursor_warps = $false
      driver_restore_after_crash = $false
      crash_restore_authority = "the product's spawned crash watcher restores under the ledger lease; this driver never calls restore after the kill"
      settings_restore = "byte-exact shared preimage in Invoke-StressCleanup"
    }
    preconditions = [ordered]@{
      owner_binary = $script:P1OwnerCopy
      fixture_binary = $script:P1FixtureCopy
      settings_path = $settingsPath
      cursor_output_rule = "cursor is read under PMv2 and is never moved; all selects resolve on the output that held the cursor at proof start"
      destination_live_rule = "a resident fixture window is admitted on the destination output before the cross-output send so the session always plans a live target domain"
      suppression_hide_rule = "workspace --select 0 selects the empty trailing; the combined single-shot switch hides iconic and maximized departures with the durable show preimage (the per-window lane vetoes minimized hides)"
    }
    stages = [ordered]@{}
  }

  $failure = $null
  $orig = $null
  $crashed = $false
  $watcher = $null
  $bind0Creation = ""
  try {
    $ledgerDir = Assert-P1CleanStart "w2-arrival-preflight"
    $ctx.LedgerDir = $ledgerDir
    # Use the tile CLI's maximum bounded lifetime for the crash proof.
    $script:P1OwnerSeconds = 600
    $orig = Save-LiveSettingsPreimage $settingsPath $origPath $spiPath ""
    $ctx.GroupOrig = $orig
    $ctx.SettingsConfigIntended = $true
    $evidence.stages.preflight = @{ ledger_dir = "$ledgerDir"; settings_sha_before = "$($orig.sha)" }

    $boot = Start-P1FixtureAndOwner $script:P1FixtureCopy $ctx "w2-arrival"
    $fixtureCopy = $script:P1FixtureCopy
    $logPath = $boot.launch.log
    $bind0 = $boot.launch.bind
    $bind0Creation = "$($bind0.process_creation)"
    $fixtureState = $boot.fixtureState
    $evidence.stages.boot = @{
      server_pid = $boot.serverPid
      owner_pid = [int]$bind0.pid
      owner_creation = $bind0Creation
      log = "$logPath"
    }

    # Exact paired geometry identifies the outputs in the same coordinate
    # space as the cursor and the fixture placements.
    $geomApply = Wait-P1Event $logPath 0 {
      param($e)
      if ("$(Get-P1Prop $e 'event')" -ne "world-apply") { return $false }
      if ("$(Get-P1Prop $e 'delivery')" -notin @("accepted", "command-consumed")) { return $false }
      if ("$(Get-P1Prop $e 'geometry_status')" -ne "paired") { return $false }
      return (@(Get-P1Prop $e "desired_geometry").Count -ge 1)
    } 40 "paired-geometry"

    $geoms = @(Get-W2Array $geomApply.event "desired_geometry")
    $cursor = Get-W2CursorPoint
    $cursorX = [int]$cursor[0]
    $cursorY = [int]$cursor[1]
    $cursorHits = @($geoms | Where-Object {
        $null -ne (Get-P1Prop $_ "full") -and
        (Test-W2PointInRect (Get-P1Prop $_ "full") $cursorX $cursorY)
      })
    if ($cursorHits.Count -ne 1) {
      throw "cursor ($cursorX,$cursorY) matched $($cursorHits.Count) output full rects; geometry=$($geoms | ConvertTo-Json -Compress -Depth 6)"
    }
    $geomA = $cursorHits[0]
    $outA = "$(Get-P1Prop $geomA 'output')"
    $fullA = Get-P1Prop $geomA "full"
    $workA = Get-P1Prop $geomA "work"
    if ($null -eq $workA) { $workA = $fullA }

    $others = @($geoms | Where-Object {
        "$(Get-P1Prop $_ 'output')" -ne $outA -and
        $null -ne (Get-P1Prop $_ "full")
      })
    if ($others.Count -lt 1) {
      throw "cross-output proof requires at least two outputs; cursor output=$outA geometry=$($geoms | ConvertTo-Json -Compress -Depth 6)"
    }
    $geomB = $null
    $bestDistance = [double]::MaxValue
    $centerA = Get-W2Center $fullA
    foreach ($candidate in $others) {
      $centerC = Get-W2Center (Get-P1Prop $candidate "full")
      $distance = [math]::Sqrt(
        [math]::Pow([double]($centerC[0] - $centerA[0]), 2.0) +
        [math]::Pow([double]($centerC[1] - $centerA[1]), 2.0))
      if ($distance -lt $bestDistance) {
        $bestDistance = $distance
        $geomB = $candidate
      }
    }
    $outB = "$(Get-P1Prop $geomB 'output')"
    $fullB = Get-P1Prop $geomB "full"
    $workB = Get-P1Prop $geomB "work"
    if ($null -eq $workB) { $workB = $fullB }
    $directionOut = Get-W2Direction $fullA $fullB
    $directionBack = switch ($directionOut) {
      "left" { "right" }
      "right" { "left" }
      "up" { "down" }
      "down" { "up" }
      default { throw "unresolved direction $directionOut" }
    }
    $evidence.stages.outputs = @{
      cursor_output = $outA
      destination_output = $outB
      forward_direction = $directionOut
      return_direction = $directionBack
      cursor = @($cursorX, $cursorY)
      geometry = $geoms
    }

    # Stage a resident on the destination output first: an empty engine domain
    # cannot plan the pair send, so the destination must already hold one
    # managed fixture member.
    $bw = [int](Get-P1Prop $workB "w")
    $bh = [int](Get-P1Prop $workB "h")
    if ($bw -lt 500 -or $bh -lt 400) {
      throw "destination work area too small for fixture staging: ${bw}x${bh}"
    }
    $residentW = [math]::Min(560, $bw - 180)
    $residentH = [math]::Min(380, $bh - 180)
    $residentX = [int](Get-P1Prop $workB "x") + [int](($bw - $residentW) / 2)
    $residentY = [int](Get-P1Prop $workB "y") + [int](($bh - $residentH) / 2)
    Spawn-P1Window $fixtureCopy $ctx "resident" $residentX $residentY $residentW $residentH $true
    $hwndR = Wait-StressHwnd "resident" $fixtureState 30
    $tokOf = @{}
    $tokR = Wait-StressAdmitted $logPath $tokOf $hwndR $fixtureState 60 "resident"
    $tokOf["resident"] = $tokR

    $aw = [int](Get-P1Prop $workA "w")
    $ah = [int](Get-P1Prop $workA "h")
    if ($aw -lt 500 -or $ah -lt 400) {
      throw "source work area too small for fixture staging: ${aw}x${ah}"
    }
    $moverW = [math]::Min(680, $aw - 180)
    $moverH = [math]::Min(420, $ah - 180)
    $moverX = [int](Get-P1Prop $workA "x") + 80
    $moverY = [int](Get-P1Prop $workA "y") + 60
    Spawn-P1Window $fixtureCopy $ctx "mover" $moverX $moverY $moverW $moverH $true
    $survivorW = [math]::Min(560, $aw - 180)
    $survivorH = [math]::Min(360, $ah - 180)
    $survivorX = $moverX
    $survivorY = [math]::Min(
      [int](Get-P1Prop $workA "y") + $ah - $survivorH - 40,
      $moverY + $moverH + 30)
    if ($survivorY -lt ([int](Get-P1Prop $workA "y") + 30)) {
      $survivorY = [int](Get-P1Prop $workA "y") + 30
    }
    Spawn-P1Window $fixtureCopy $ctx "survivor" $survivorX $survivorY $survivorW $survivorH $true
    $hwndM = Wait-StressHwnd "mover" $fixtureState 30
    $hwndS = Wait-StressHwnd "survivor" $fixtureState 30
    $tokM = Wait-StressAdmitted $logPath $tokOf $hwndM $fixtureState 60 "mover"
    $tokOf["mover"] = $tokM
    $tokS = Wait-StressAdmitted $logPath $tokOf $hwndS $fixtureState 60 "survivor"
    $tokOf["survivor"] = $tokS
    $hwndOf = @{ resident = $hwndR; mover = $hwndM; survivor = $hwndS }
    $evidence.stages.fixture = @{
      hwnd_resident = $hwndR
      hwnd_mover = $hwndM
      hwnd_survivor = $hwndS
      token_resident = $tokR
      token_mover = $tokM
      token_survivor = $tokS
    }

    # Every staged member must be tiled on its own output's current view,
    # with no pending transfer/frame work, before any command is issued.
    $preTiled = Wait-P1SettledWorld $logPath 0 {
      param($e)
      if (-not (Test-P1Tiled $e $tokR) -or
          -not (Test-P1Tiled $e $tokM) -or
          -not (Test-P1Tiled $e $tokS)) { return $false }
      $rowR = Get-P1Row $e $tokR
      $rowM = Get-P1Row $e $tokM
      $rowS = Get-P1Row $e $tokS
      $viewA = ""
      $viewB = ""
      foreach ($row in @(Get-P1Prop $e "desired_outputs")) {
        if ("$(Get-P1Prop $row 'output')" -eq $outA) { $viewA = "$(Get-P1Prop $row 'logical_view')" }
        if ("$(Get-P1Prop $row 'output')" -eq $outB) { $viewB = "$(Get-P1Prop $row 'logical_view')" }
      }
      return ($viewA -ne "" -and $viewB -ne "" -and
        "$(Get-P1Prop $rowM 'out')" -eq $outA -and
        "$(Get-P1Prop $rowS 'out')" -eq $outA -and
        "$(Get-P1Prop $rowR 'out')" -eq $outB -and
        "$(Get-P1Prop $rowM 'ws')" -eq $viewA -and
        "$(Get-P1Prop $rowS 'ws')" -eq $viewA -and
        "$(Get-P1Prop $rowR 'ws')" -eq $viewB)
    } 60 "pre-tiled"
    Assert-P1NoPending $preTiled.event "pre-tiled"
    $viewA = ""
    $viewB = ""
    foreach ($row in @(Get-P1Prop $preTiled.event "desired_outputs")) {
      if ("$(Get-P1Prop $row 'output')" -eq $outA) { $viewA = "$(Get-P1Prop $row 'logical_view')" }
      if ("$(Get-P1Prop $row 'output')" -eq $outB) { $viewB = "$(Get-P1Prop $row 'logical_view')" }
    }
    if ($viewA -eq "" -or $viewB -eq "") { throw "pre-tiled world row carried no desired view" }
    $evidence.stages.pre_tiled = @{
      apply_index = [int]$preTiled.index
      view_source = $viewA
      view_destination = $viewB
      row_resident = (Get-P1Row $preTiled.event $tokR)
      row_mover = (Get-P1Row $preTiled.event $tokM)
      row_survivor = (Get-P1Row $preTiled.event $tokS)
    }

    # --- A1: cross-output follow arrival ---------------------------------
    $null = Invoke-StressFocus $fixtureCopy $ctx.Socket $ctx.Nonce "mover" $hwndOf $fixtureState 15
    $null = Wait-P1Cond { (Get-P1Snap $hwndM).fg } 15 "mover-foreground"
    $forwardMark = Get-P1LogMark $logPath
    $null = Send-W2Output $ctx.OwnerPayload $directionOut
    $forwardApplied = Wait-P1Event $logPath $forwardMark {
      param($e)
      if ("$(Get-P1Prop $e 'event')" -ne "workspace-output") { return $false }
      return ("$(Get-P1Prop $e 'outcome')" -eq "send-applied" -and
        "$(Get-P1Prop $e 'direction')" -eq $directionOut -and
        (Get-P1Prop $e 'follow') -eq $true)
    } 30 "forward-send"
    $forwardArrival = Wait-P1Event $logPath $forwardMark {
      param($e)
      if ("$(Get-P1Prop $e 'event')" -ne "world-apply") { return $false }
      if ("$(Get-P1Prop $e 'delivery')" -notin @("accepted", "command-consumed")) { return $false }
      if ("$(Get-P1Prop $e 'geometry_status')" -ne "paired") { return $false }
      $omitted = Get-P1Prop $e "arrival_brackets_omitted"
      if ($null -eq $omitted -or [int]$omitted -ne 0) { return $false }
      $rows = @(Get-W2ArrivalRows $e $tokM)
      if ($rows.Count -ne 1) { return $false }
      $row = $rows[0]
      $desired = Get-P1Row $e $tokM
      return ("$(Get-P1Prop $row 'kind')" -eq "output" -and
        "$(Get-P1Prop $row 'out')" -eq $outB -and
        (Get-P1Prop $row 'foreground_proven') -eq $true -and
        (Test-W2ArrivalSamples $row) -and
        $null -ne (Get-P1Prop $row 'frame') -and
        $null -ne $desired -and
        "$(Get-P1Prop $desired 'out')" -eq $outB -and
        "$(Get-P1Prop $desired 'ws')" -eq $viewB)
    } 45 "cross-output-arrival"
    $forwardRows = @(Get-W2ArrivalRows $forwardArrival.event $tokM)
    $forwardRow = $forwardRows[0]
    $forwardDesired = Get-P1Row $forwardArrival.event $tokM
    if ($forwardDesired -eq $null) { throw "forward arrival apply lost the mover row" }
    $forwardFrame = Get-P1Prop $forwardRow "frame"
    $effectiveForward = Get-P1Prop $forwardDesired "effective_frame"
    if ($null -eq $forwardFrame) { throw "cross-output arrival bracket carried no frame" }
    if ($null -eq $effectiveForward) { throw "cross-output tiled arrival carried no effective_frame" }
    if (-not (Test-W2RectInside $forwardFrame $fullB 2)) {
      throw "cross-output arrival bracket frame did not land inside the destination full rect"
    }
    if (-not (Test-W2RectInside $effectiveForward $fullB 2)) {
      throw "cross-output effective frame did not land inside the destination full rect"
    }
    $forwardSnap = Wait-P1NativeVis $hwndM `
      ([int](Get-P1Prop $effectiveForward "x")) ([int](Get-P1Prop $effectiveForward "y")) `
      ([int](Get-P1Prop $effectiveForward "w")) ([int](Get-P1Prop $effectiveForward "h")) `
      4 20 "cross-output-native-frame"
    $null = Wait-P1Cond {
      $snap = Get-P1Snap $hwndM
      return ($snap.fg -and $snap.visible -and -not $snap.iconic)
    } 15 "cross-output-foreground"
    $forwardPosts = @(Get-W2Posts $forwardArrival.event)
    $evidence.stages.a1_cross_output = @{
      forward_direction = $directionOut
      send_apply_index = [int]$forwardArrival.index
      send_line_index = [int]$forwardApplied.index
      arrival = $forwardRow
      desired_row = $forwardDesired
      destination_view = $viewB
      effective_frame_inside_destination = (Test-W2RectInside $effectiveForward $fullB 2)
      native_visible_frame = $forwardSnap.vis
      native_snapshot = $forwardSnap
      visibility_posts = $forwardPosts
      assertion = "paired arrival row kind=output out=$outB foreground_proven=true; exact visible/lifetime samples; bracket frame inside $outB; DWM visible frame == effective_frame; mover foreground on $outB"
    }
    Write-P1Log $runDir "A1 cross-output $outA -> $outB ($directionOut) arrival bracket proven"

    # --- A2: hidden parked arrival and reveal ----------------------------
    $returnMark = Get-P1LogMark $logPath
    $null = Send-W2Output $ctx.OwnerPayload $directionBack
    $returnApplied = Wait-P1Event $logPath $returnMark {
      param($e)
      if ("$(Get-P1Prop $e 'event')" -ne "workspace-output") { return $false }
      return ("$(Get-P1Prop $e 'outcome')" -eq "send-applied" -and
        "$(Get-P1Prop $e 'direction')" -eq $directionBack)
    } 30 "return-send"
    $returnArrival = Wait-P1Event $logPath $returnMark {
      param($e)
      if ("$(Get-P1Prop $e 'event')" -ne "world-apply") { return $false }
      if ("$(Get-P1Prop $e 'delivery')" -notin @("accepted", "command-consumed")) { return $false }
      if ("$(Get-P1Prop $e 'geometry_status')" -ne "paired") { return $false }
      $omitted = Get-P1Prop $e "arrival_brackets_omitted"
      if ($null -eq $omitted -or [int]$omitted -ne 0) { return $false }
      $rows = @(Get-W2ArrivalRows $e $tokM)
      if ($rows.Count -ne 1) { return $false }
      $desired = Get-P1Row $e $tokM
      return ("$(Get-P1Prop $rows[0] 'kind')" -eq "output" -and
        "$(Get-P1Prop $rows[0] 'out')" -eq $outA -and
        (Get-P1Prop $rows[0] 'foreground_proven') -eq $true -and
        (Test-W2ArrivalSamples $rows[0]) -and
        $null -ne $desired -and
        "$(Get-P1Prop $desired 'out')" -eq $outA -and
        "$(Get-P1Prop $desired 'ws')" -eq $viewA)
    } 45 "return-arrival"
    $returnDesired = Get-P1Row $returnArrival.event $tokM
    if ($returnDesired -eq $null) { throw "return arrival apply lost the mover row" }
    $preStayWs = "$(Get-P1Prop $returnDesired 'ws')"
    $null = Wait-P1Cond {
      $snap = Get-P1Snap $hwndM
      return ($snap.fg -and $snap.visible)
    } 15 "return-foreground"
    $evidence.stages.a2_return = @{
      direction = $directionBack
      apply_index = [int]$returnArrival.index
      arrival = @(Get-W2ArrivalRows $returnArrival.event $tokM)
      desired_row = $returnDesired
      pre_stay_workspace = $preStayWs
    }

    $stayMark = Get-P1LogMark $logPath
    $null = Invoke-W2Workspace $ctx.OwnerPayload "--stay" "0"
    # The completion line is supporting trace only: a `--stay` line reports
    # the unchanged current position, and the accepted world row below is the
    # authoritative proof that the hovered member was homed away.
    $stayLine = $null
    try {
      $stayLine = Wait-P1Event $logPath $stayMark {
        param($e)
        if ("$(Get-P1Prop $e 'event')" -ne "workspace") { return $false }
        return ("$(Get-P1Prop $e 'op')" -eq "stay" -and
          [int](Get-P1Prop $e 'index') -eq 0 -and
          "$(Get-P1Prop $e 'outcome')" -eq "ok")
      } 10 "stay-line"
    } catch {}
    $stayApply = Wait-P1Event $logPath $stayMark {
      param($e)
      if ("$(Get-P1Prop $e 'event')" -ne "world-apply") { return $false }
      if ("$(Get-P1Prop $e 'delivery')" -notin @("accepted", "command-consumed")) { return $false }
      $row = Get-P1Row $e $tokM
      if ($null -eq $row) { return $false }
      $ws = "$(Get-P1Prop $row 'ws')"
      return ($ws -ne "" -and $ws -ne $preStayWs -and
        "$(Get-P1Prop $row 'out')" -eq $outA)
    } 30 "stay-apply"
    $hiddenRow = Get-P1Row $stayApply.event $tokM
    $hiddenWs = "$(Get-P1Prop $hiddenRow 'ws')"
    if ($hiddenWs -eq "" -or $hiddenWs -eq $preStayWs) {
      throw "stay-send did not move the mover to a distinct workspace"
    }
    $null = Wait-P1Cond {
      $ledger = Read-W2Ledger $ledgerDir
      $ledgerRow = Get-W2LedgerRow $ledger $hwndM
      $snap = Get-P1Snap $hwndM
      return ($null -ne $ledger -and
        (Test-W2LedgerHidden $ledgerRow) -and
        -not $snap.visible -and
        -not $snap.iconic -and
        -not $snap.maximized)
    } 30 "stay-hidden"
    $stayLedgerRow = Get-W2LedgerRow (Read-W2Ledger $ledgerDir) $hwndM
    $stayPosts = @(Get-P1EventsSince $logPath $stayMark | ForEach-Object {
        foreach ($post in (Get-W2Posts $_)) {
          if ([uint64](Get-P1Prop $post "hwnd") -eq $hwndM) { $post }
        }
      })
    $hidePost = @($stayPosts | Where-Object {
        (Get-P1Prop $_ "hide") -eq $true -and (Get-P1Prop $_ "posted") -eq $true
      })
    if ($hidePost.Count -lt 1) {
      throw "stay-send produced no posted hide for the mover: $($stayPosts | ConvertTo-Json -Compress -Depth 6)"
    }

    $ordinal = Get-W2RingOrdinal $stayApply.event $outA $hiddenWs
    if ($ordinal -lt 1 -or $ordinal -gt 9) {
      throw "hidden workspace $hiddenWs has no numbered ring position on $outA"
    }
    $cursorAfterStay = Get-W2CursorPoint
    if (-not (Test-W2PointInRect $fullA ([int]$cursorAfterStay[0]) ([int]$cursorAfterStay[1]))) {
      throw "cursor left the proof output before select; the numbered select would resolve elsewhere"
    }
    $evidence.stages.a2_parked = @{
      stay_line = $(if ($null -ne $stayLine) { $stayLine.event } else { $null })
      apply_index = [int]$stayApply.index
      workspace = $hiddenWs
      ordinal = $ordinal
      ledger_row = $stayLedgerRow
      native_hidden = (Get-P1Snap $hwndM)
      hide_posts = $stayPosts
      cursor_now = $cursorAfterStay
    }

    $revealMark = Get-P1LogMark $logPath
    $null = Invoke-W2Workspace $ctx.OwnerPayload "--select" "$ordinal"
    # Supporting trace only; the accepted arrival apply below proves both the
    # resolved output (A) and the revealed workspace.
    $selectLine = $null
    try {
      $selectLine = Wait-P1Event $logPath $revealMark {
        param($e)
        if ("$(Get-P1Prop $e 'event')" -ne "workspace") { return $false }
        return ("$(Get-P1Prop $e 'op')" -eq "select" -and
          [int](Get-P1Prop $e 'index') -eq $ordinal)
      } 10 "select-line"
    } catch {}
    $revealArrival = Wait-P1Event $logPath $revealMark {
      param($e)
      if ("$(Get-P1Prop $e 'event')" -ne "world-apply") { return $false }
      if ("$(Get-P1Prop $e 'delivery')" -notin @("accepted", "command-consumed")) { return $false }
      if ("$(Get-P1Prop $e 'geometry_status')" -ne "paired") { return $false }
      $omitted = Get-P1Prop $e "arrival_brackets_omitted"
      if ($null -eq $omitted -or [int]$omitted -ne 0) { return $false }
      $rows = @(Get-W2ArrivalRows $e $tokM)
      if ($rows.Count -ne 1) { return $false }
      $row = $rows[0]
      $desired = Get-P1Row $e $tokM
      $view = ""
      foreach ($outRow in @(Get-P1Prop $e "desired_outputs")) {
        if ("$(Get-P1Prop $outRow 'output')" -eq $outA) { $view = "$(Get-P1Prop $outRow 'logical_view')" }
      }
      return ("$(Get-P1Prop $row 'kind')" -eq "output" -and
        "$(Get-P1Prop $row 'out')" -eq $outA -and
        (Get-P1Prop $row 'foreground_proven') -eq $true -and
        (Test-W2ArrivalSamples $row) -and
        $null -ne (Get-P1Prop $row 'frame') -and
        $null -ne $desired -and
        "$(Get-P1Prop $desired 'ws')" -eq $hiddenWs -and
        $view -eq $hiddenWs)
    } 45 "hidden-reveal-arrival"
    $revealRows = @(Get-W2ArrivalRows $revealArrival.event $tokM)
    $revealRow = $revealRows[0]
    $revealDesired = Get-P1Row $revealArrival.event $tokM
    if ($revealDesired -eq $null) { throw "reveal arrival apply lost the mover row" }
    $revealFrame = Get-P1Prop $revealRow "frame"
    $effectiveReveal = Get-P1Prop $revealDesired "effective_frame"
    if ($null -eq $revealFrame) { throw "reveal arrival bracket carried no frame" }
    if ($null -eq $effectiveReveal) { throw "revealed tiled arrival carried no effective_frame" }
    if (-not (Test-W2RectInside $revealFrame $fullA 2)) {
      throw "reveal arrival bracket frame did not land inside the proof output"
    }
    if (-not (Test-W2RectInside $effectiveReveal $fullA 2)) {
      throw "revealed effective frame did not land inside the proof output"
    }
    $revealSnap = Wait-P1NativeVis $hwndM `
      ([int](Get-P1Prop $effectiveReveal "x")) ([int](Get-P1Prop $effectiveReveal "y")) `
      ([int](Get-P1Prop $effectiveReveal "w")) ([int](Get-P1Prop $effectiveReveal "h")) `
      4 20 "hidden-reveal-native-frame"
    $null = Wait-P1Cond {
      $ledger = Read-W2Ledger $ledgerDir
      $snap = Get-P1Snap $hwndM
      return ($null -ne $ledger -and
        $null -eq (Get-W2LedgerRow $ledger $hwndM) -and
        $snap.visible -and -not $snap.iconic)
    } 20 "hidden-reveal-retired"
    $revealPosts = @(Get-P1EventsSince $logPath $revealMark | ForEach-Object {
        foreach ($post in (Get-W2Posts $_)) {
          if ([uint64](Get-P1Prop $post "hwnd") -eq $hwndM) { $post }
        }
      })
    $showPost = @($revealPosts | Where-Object {
        (Get-P1Prop $_ "hide") -eq $false -and (Get-P1Prop $_ "posted") -eq $true
      })
    if ($showPost.Count -lt 1) {
      throw "numbered reveal produced no posted show for the mover: $($revealPosts | ConvertTo-Json -Compress -Depth 6)"
    }
    $evidence.stages.a2_reveal = @{
      select_line = $(if ($null -ne $selectLine) { $selectLine.event } else { $null })
      apply_index = [int]$revealArrival.index
      arrival = $revealRow
      desired_row = $revealDesired
      native_visible_frame = $revealSnap.vis
      native_snapshot = $revealSnap
      show_posts = $revealPosts
      retired_ledger_row = $true
      assertion = "parked hide claim followed by numbered reveal; paired arrival row kind=output out=$outA foreground_proven=true; exact visible/lifetime samples; posted hide then posted show; ledger claim retired"
    }
    Write-P1Log $runDir "A2 parked workspace $hiddenWs revealed at ordinal $ordinal with exact arrival bracket"

    # --- B: non-normal show-state suppression recovery -------------------
    $currentWs = $hiddenWs
    $null = Wait-P1Cond {
      $snap = Get-P1Snap $hwndM
      return ($snap.visible -and $snap.fg)
    } 15 "post-reveal-current-view"

    Spawn-P1Window $fixtureCopy $ctx "normal" ($moverX + 40) ($moverY + 20) $moverW $moverH $true
    Spawn-P1Window $fixtureCopy $ctx "maxi" ($moverX + 70) ($moverY + 40) $moverW $moverH $true
    Spawn-P1Window $fixtureCopy $ctx "mini" ($moverX + 100) ($moverY + 60) $moverW $moverH $true
    $hwndN = Wait-StressHwnd "normal" $fixtureState 30
    $hwndX = Wait-StressHwnd "maxi" $fixtureState 30
    $hwndI = Wait-StressHwnd "mini" $fixtureState 30
    $tokN = Wait-StressAdmitted $logPath $tokOf $hwndN $fixtureState 60 "normal"
    $tokOf["normal"] = $tokN
    $tokX = Wait-StressAdmitted $logPath $tokOf $hwndX $fixtureState 60 "maxi"
    $tokOf["maxi"] = $tokX
    $tokI = Wait-StressAdmitted $logPath $tokOf $hwndI $fixtureState 60 "mini"
    $tokOf["mini"] = $tokI
    $evidence.stages.suppression_fixture = @{
      hwnd_normal = $hwndN
      hwnd_maximized = $hwndX
      hwnd_minimized = $hwndI
      token_normal = $tokN
      token_maximized = $tokX
      token_minimized = $tokI
    }

    # Admit all three on the current (revealed) view before changing show
    # state, so a fixture maximize/minimize can never look born-normal-path.
    $suppressionReady = Wait-P1Event $logPath 0 {
      param($e)
      if ("$(Get-P1Prop $e 'event')" -ne "world-apply") { return $false }
      if ("$(Get-P1Prop $e 'delivery')" -notin @("accepted", "command-consumed")) { return $false }
      $rowN = Get-P1Row $e $tokN
      $rowX = Get-P1Row $e $tokX
      $rowI = Get-P1Row $e $tokI
      return ($null -ne $rowN -and $null -ne $rowX -and $null -ne $rowI -and
        "$(Get-P1Prop $rowN 'ws')" -eq $currentWs -and
        "$(Get-P1Prop $rowX 'ws')" -eq $currentWs -and
        "$(Get-P1Prop $rowI 'ws')" -eq $currentWs -and
        "$(Get-P1Prop $rowN 'out')" -eq $outA -and
        "$(Get-P1Prop $rowX 'out')" -eq $outA -and
        "$(Get-P1Prop $rowI 'out')" -eq $outA)
    } 40 "suppression-admitted"
    $evidence.stages.suppression_admitted = @{ apply_index = [int]$suppressionReady.index }

    $null = Set-W2FixtureMode $fixtureCopy $ctx.Socket $ctx.Nonce "maxi" "maximized"
    $null = Wait-P1Cond { (Get-P1Snap $hwndX).maximized } 15 "maximized-native"
    $null = Set-W2FixtureMode $fixtureCopy $ctx.Socket $ctx.Nonce "mini" "minimized"
    $null = Wait-P1Cond { (Get-P1Snap $hwndI).iconic } 15 "minimized-native"

    # Re-read immediately before the suppression switch: the durable ledger
    # preimage is captured from this native state.
    $snapNPre = Get-P1Snap $hwndN
    $snapXPre = Get-P1Snap $hwndX
    $snapIPre = Get-P1Snap $hwndI
    if (-not $snapNPre.visible -or $snapNPre.iconic -or $snapNPre.maximized) {
      throw "normal fixture window was not in normal show state before suppression"
    }
    if (-not $snapXPre.visible -or $snapXPre.iconic -or -not $snapXPre.maximized) {
      throw "maximized fixture window lost its native maximized state before suppression"
    }
    if (-not $snapIPre.visible -or -not $snapIPre.iconic) {
      throw "minimized fixture window lost its native minimized state before suppression"
    }
    $evidence.stages.suppression_pre = @{
      normal = $snapNPre
      maximized = $snapXPre
      minimized = $snapIPre
    }

    $suppressMark = Get-P1LogMark $logPath
    $null = Invoke-W2Workspace $ctx.OwnerPayload "--select" "0"
    # Supporting trace only: the accepted apply below proves the resolved
    # output A and the new empty view.
    $suppressLine = $null
    try {
      $suppressLine = Wait-P1Event $logPath $suppressMark {
        param($e)
        if ("$(Get-P1Prop $e 'event')" -ne "workspace") { return $false }
        return ("$(Get-P1Prop $e 'op')" -eq "select" -and
          [int](Get-P1Prop $e 'index') -eq 0 -and
          "$(Get-P1Prop $e 'outcome')" -eq "ok")
      } 10 "suppression-select-line"
    } catch {}
    # A trailing select from the occupied revealed trailing mints a fresh
    # empty trailing and switches to it; the three suppression members stay
    # homed to the departing view and are the combined switch's departures.
    $suppressApply = Wait-P1Event $logPath $suppressMark {
      param($e)
      if ("$(Get-P1Prop $e 'event')" -ne "world-apply") { return $false }
      if ("$(Get-P1Prop $e 'delivery')" -notin @("accepted", "command-consumed")) { return $false }
      $view = ""
      foreach ($outRow in @(Get-P1Prop $e "desired_outputs")) {
        if ("$(Get-P1Prop $outRow 'output')" -eq $outA) { $view = "$(Get-P1Prop $outRow 'logical_view')" }
      }
      if ($view -eq "" -or $view -eq $currentWs) { return $false }
      $rowN = Get-P1Row $e $tokN
      $rowX = Get-P1Row $e $tokX
      $rowI = Get-P1Row $e $tokI
      return ($null -ne $rowN -and $null -ne $rowX -and $null -ne $rowI -and
        "$(Get-P1Prop $rowN 'ws')" -eq $currentWs -and
        "$(Get-P1Prop $rowX 'ws')" -eq $currentWs -and
        "$(Get-P1Prop $rowI 'ws')" -eq $currentWs)
    } 30 "suppression-select-apply"
    $emptyWs = ""
    foreach ($outRow in @(Get-P1Prop $suppressApply.event "desired_outputs")) {
      if ("$(Get-P1Prop $outRow 'output')" -eq $outA) { $emptyWs = "$(Get-P1Prop $outRow 'logical_view')" }
    }
    if ($emptyWs -eq "" -or $emptyWs -eq $currentWs) {
      throw "empty-trailing select did not change A's view (got $emptyWs)"
    }

    $null = Wait-P1Cond {
      $ledger = Read-W2Ledger $ledgerDir
      if ($null -eq $ledger) { return $false }
      $rowN = Get-W2LedgerRow $ledger $hwndN
      $rowX = Get-W2LedgerRow $ledger $hwndX
      $rowI = Get-W2LedgerRow $ledger $hwndI
      if (-not (Test-W2LedgerHidden $rowN) -or
          -not (Test-W2LedgerHidden $rowX) -or
          -not (Test-W2LedgerHidden $rowI)) { return $false }
      if (-not (Test-W2LedgerShow $rowN $false $false) -or
          -not (Test-W2LedgerShow $rowX $false $true) -or
          -not (Test-W2LedgerShow $rowI $true $false)) { return $false }
      $snapN = Get-P1Snap $hwndN
      $snapX = Get-P1Snap $hwndX
      $snapI = Get-P1Snap $hwndI
      return (-not $snapN.visible -and -not $snapX.visible -and -not $snapI.visible)
    } 40 "suppression-hidden"
    $hiddenLedger = Read-W2Ledger $ledgerDir
    $ledgerN = Get-W2LedgerRow $hiddenLedger $hwndN
    $ledgerX = Get-W2LedgerRow $hiddenLedger $hwndX
    $ledgerI = Get-W2LedgerRow $hiddenLedger $hwndI
    $evidence.stages.suppression_hidden = @{
      select_line = $(if ($null -ne $suppressLine) { $suppressLine.event } else { $null })
      empty_workspace = $emptyWs
      ledger = $hiddenLedger
      ledger_normal = $ledgerN
      ledger_maximized = $ledgerX
      ledger_minimized = $ledgerI
      native = @{
        normal = (Get-P1Snap $hwndN)
        maximized = (Get-P1Snap $hwndX)
        minimized = (Get-P1Snap $hwndI)
      }
      assertion = "combined empty-trailing switch hid all three with durable show preimages false/false, false/true, true/false"
    }

    # Kill only the exact bound owner; the product watcher restores.
    $watcher = Wait-P1WatcherMarker $ledgerDir $bind0Creation 15 "crash-watcher"
    if ("$($watcher.owner_creation)" -ne $bind0Creation) {
      throw "crash watcher marker names another owner: $($watcher | ConvertTo-Json -Compress)"
    }
    if ([int]$watcher.pid -eq [int]$bind0.pid) {
      throw "crash watcher marker names the owner pid: $($watcher | ConvertTo-Json -Compress)"
    }
    if ($null -eq (Get-P1LiveProcess ([int]$watcher.pid) "$($watcher.creation)" $ctx.OwnerPayload)) {
      throw "crash watcher live identity mismatch (pid=$($watcher.pid) creation=$($watcher.creation))"
    }
    $markerPath = "$($watcher.path)"
    $evidence.stages.crash_watcher = @{ marker = $watcher; live = $true }

    Stop-P1Crash $ctx.OwnerPayload $bind0 "w2-arrival-suppression"
    $crashed = $true
    $null = Wait-P1Cond {
      $snapN = Get-P1Snap $hwndN
      $snapX = Get-P1Snap $hwndX
      $snapI = Get-P1Snap $hwndI
      $ledgerGone = -not (Test-Path -LiteralPath (Join-Path $ledgerDir "ledger.json") -PathType Leaf)
      $markerGone = ($null -eq (Read-P1WatcherMarker $ledgerDir $bind0Creation))
      return ($ledgerGone -and $markerGone -and
        $snapN.alive -and $snapN.visible -and -not $snapN.iconic -and -not $snapN.maximized -and
        $snapX.alive -and $snapX.visible -and -not $snapX.iconic -and $snapX.maximized -and
        $snapI.alive -and $snapI.visible -and $snapI.iconic)
    } 30 "watcher-restored"
    $watcherGone = $false
    $watcherDeadline = (Get-Date).AddSeconds(15)
    while ((Get-Date) -lt $watcherDeadline) {
      if ($null -eq (Get-P1LiveProcess ([int]$watcher.pid) "$($watcher.creation)" $ctx.OwnerPayload)) {
        $watcherGone = $true
        break
      }
      Start-Sleep -Milliseconds 100
    }
    if (-not $watcherGone) { throw "crash watcher process survived its own restore" }
    $restoredN = Get-P1Snap $hwndN
    $restoredX = Get-P1Snap $hwndX
    $restoredI = Get-P1Snap $hwndI
    if (-not $restoredN.visible -or $restoredN.iconic -or $restoredN.maximized) {
      throw "watcher did not restore the normal window to normal state"
    }
    if (-not $restoredX.visible -or $restoredX.iconic -or -not $restoredX.maximized) {
      throw "watcher did not restore the maximized window to maximized state"
    }
    if (-not $restoredI.visible -or -not $restoredI.iconic) {
      throw "watcher did not restore the minimized window to minimized state"
    }
    $evidence.stages.suppression_restored = @{
      ledger_absent = (-not (Test-Path -LiteralPath (Join-Path $ledgerDir "ledger.json")))
      marker_absent = (-not (Test-Path -LiteralPath $markerPath))
      watcher_exited = $true
      normal = $restoredN
      maximized = $restoredX
      minimized = $restoredI
      assertion = "product crash watcher restored normal visible/unminimized/unmaximized, maximized visible/maximized, minimized visible/iconic from the durable show preimages; no driver restore ran"
    }
    Write-P1Log $runDir "B non-normal suppression recovered by the product watcher (normal/maximized/minimized)"
  } catch {
    $failure = "$($_.Exception.Message)"
    $evidence.failure = $failure
    $evidence.failure_stack = $_.ScriptStackTrace
    Write-P1Log $runDir "FAILURE: $failure"
  } finally {
    if ($crashed -and $null -ne $watcher) {
      # The owner is already dead. Give the watcher a bounded window to finish
      # before cleanup shuts the fixture, then stop treating the dead owner as
      # a live binding: cleanup must skip owner stop/restore/ready (a second
      # restore would be wrong) and verify only ledger absence/residue.
      try {
        $settleDeadline = (Get-Date).AddSeconds(15)
        while ((Get-Date) -lt $settleDeadline) {
          if ($null -eq (Get-P1LiveProcess ([int]$watcher.pid) "$($watcher.creation)" $ctx.OwnerPayload)) {
            break
          }
          Start-Sleep -Milliseconds 100
        }
      } catch {}
      $ctx.Owner = $null
      $ctx.OwnerLaunched = $false
      $ctx.OwnerLaunchIntended = $false
    }
    try {
      Invoke-StressCleanup $ctx "w2-arrival-suppression"
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
        $evidence.restore = @{
          sha_before = "$($orig.sha)"
          sha_after = $postSha
          bytes_restored = ("$postSha" -eq "$($orig.sha)")
        }
      }
    } catch {
      if (-not $failure) { $failure = "post-restore hash failed: $($_.Exception.Message)" }
    }
    $evidence.finished_utc = (Get-Date).ToUniversalTime().ToString("o")
    $evidence | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $evidencePath -Encoding utf8
    Write-P1Log $runDir "evidence $evidencePath"
    if ($failure) { throw "FAIL w2-arrival-suppression: $failure" }
  }
}

# --- top-level staging and dispatch ----------------------------------------

Initialize-P1Artifacts "w2-arrival-suppression"
Invoke-W2ArrivalSuppression
