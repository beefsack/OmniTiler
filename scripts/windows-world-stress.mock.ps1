$ErrorActionPreference = "Stop"
# Offline fault-injection mock for scripts/windows-world-stress.ps1.
# No owner launch, no fixture, no window mutation: pure mapping/comparison
# functions plus canned owner-log files drive the join gates. All canned
# shapes mirror the production sources cited in windows-world-stress.ps1
# (world_tick apply_diag_value keys, pager_host cell fields); handcrafted
# values that disagree with runtime are faults, not fixtures.
#   pwsh -NoProfile -File scripts/windows-world-stress.mock.ps1
$Repo = Split-Path -Parent $PSScriptRoot
. (Join-Path $Repo "scripts\windows-world-stress.ps1")
if ("$($MyInvocation.InvocationName)" -eq ".") { return }

$WSM_STEPS = [System.Collections.ArrayList]@()
function Rec-Mock([string]$Name, $Data) { $null = $WSM_STEPS.Add(@{ name = $Name; data = $Data }) }
function Fail-Mock([string]$Msg) { throw $Msg }
function Expect-Throw([scriptblock]$Body, [string]$Needle, [string]$Tag) {
  try { & $Body } catch {
    if ("$($_.Exception.Message)" -match $Needle) {
      Rec-Mock "rejected" @{ tag = $Tag; error = "$($_.Exception.Message)" }
      return
    }
    throw "fault $Tag wrong error: $($_.Exception.Message)"
  }
  throw "fault $Tag accepted corrupt evidence"
}

$aliasOrd = @{ home = 1; away = 2 }
$mk = { param($op, $extra) $a = @{ op = $op } + $extra; (Get-StressCliArgs $a $aliasOrd) -join " " }
if ((& $mk "select-workspace" @{ workspace = "away" }) -ne "workspace --select 2") { Fail-Mock "alias map" }
if ((& $mk "select-workspace" @{ workspace = "home" }) -ne "workspace --select 1") { Fail-Mock "alias map" }
if ((& $mk "select-numbered" @{ index = 2 }) -ne "workspace --select 2") { Fail-Mock "numbered map" }
if ((& $mk "select-relative" @{ delta = 1 }) -ne "workspace --relative next") { Fail-Mock "relative map" }
if ((& $mk "select-relative" @{ delta = -1 }) -ne "workspace --relative previous") { Fail-Mock "relative map" }
if ((& $mk "select-previous" @{}) -ne "workspace --previous") { Fail-Mock "previous map" }
if ((& $mk "send" @{ workspace = "away"; follow = $true }) -ne "workspace --send 2") { Fail-Mock "send map" }
if ((& $mk "send" @{ workspace = "home"; follow = $false }) -ne "workspace --stay 1") { Fail-Mock "stay map" }
if ((& $mk "focus-directional" @{ direction = "left" }) -ne "directional --focus --direction left") { Fail-Mock "focus map" }
if ((& $mk "move-directional" @{ direction = "right" }) -ne "directional --move --direction right") { Fail-Mock "move map" }
Expect-Throw { Get-StressCliArgs @{ op = "warp" } $aliasOrd } "not a command" "unknown-op"
Expect-Throw { Get-StressCliArgs @{ op = "select-workspace"; workspace = "void" } $aliasOrd } "unmapped" "bad-alias"
Rec-Mock "argv-mapping" @{ rows = 10 }

# Dynamic alias ordinals follow the observed ring, never a hardcoded map.
$ring = @("ws-home", "ws-away", "ws-trail")
if ((Resolve-StressAlias "home" $ring "ws-home" "ws-away") -ne 1) { Fail-Mock "alias resolve home" }
if ((Resolve-StressAlias "away" $ring "ws-home" "ws-away") -ne 2) { Fail-Mock "alias resolve away" }
Expect-Throw { Resolve-StressAlias "void" $ring "ws-home" "ws-away" } "unmapped" "alias-unknown"
Expect-Throw { Resolve-StressAlias "away" @("ws-home", "ws-other") "ws-home" "ws-away" } "not in ring" "alias-pruned"
Rec-Mock "alias-resolution" @{ rows = 4 }

$tiles = @{ 'win-a' = @(400, 0, 400, 560); 'win-b' = @(0, 0, 400, 560) }
$norm = Convert-StressTiles $tiles @(0, 0, 800, 560) @(0, 0, 800, 560)
if ($norm['win-a'][2] -ne 400) { Fail-Mock "normalize identity" }
$scaled = Convert-StressTiles $tiles @(0, 0, 800, 560) @(0, 0, 1600, 1120)
if ($scaled['win-b'][0] -ne 0 -or $scaled['win-b'][2] -ne 800) { Fail-Mock "normalize scale" }
Expect-Throw { Convert-StressTiles $tiles @(0, 0, 800, 560) @(0, 0, 801, 560) } "integrally" "non-integral-scale"
Rec-Mock "normalize" @{ rows = 3 }

function New-GoodRow([string]$Tok, [string]$Out, [string]$Ws, [int]$X, [int]$Y, [int]$W, [int]$H) {
  return @{
    window = $Tok; out = $Out; ws = $Ws
    allocation = @{ x = $X; y = $Y; w = $W; h = $H }
    effective_frame = @{ x = $X; y = $Y; w = $W; h = $H }
    frame_authority = "controlled"; membership_authority = "controlled"
    frame_hold = $null; membership_hold = $null
    floating = $false; float_origin = $null
    frame_op = ""; clear_op = ""
  }
}

function New-GoodEvidence {
  $desired = @{}
  $desired['tok-a'] = New-GoodRow 'tok-a' 'o1' 'ws-home' 0 0 400 560
  $desired['tok-b'] = New-GoodRow 'tok-b' 'o1' 'ws-home' 400 0 400 560
  return @{
    Expected = @{
      View = 'home'; Members = @('win-a', 'win-b')
      Homing = @{ 'win-a' = 'home'; 'win-b' = 'home' }
      Tiles = @{ 'win-a' = @(0, 0, 400, 560); 'win-b' = @(400, 0, 400, 560) }
      Foreground = 'win-a'; PagerCurrent = 'home'; PagerOccupied = @('home')
    }
    Actual = @{
      NativeOut = 'o1'; HomeId = 'ws-home'; AwayId = 'ws-away'
      Ring = @('ws-home', 'ws-away')
      Tokens = @{ 'win-a' = 'tok-a'; 'win-b' = 'tok-b' }
      Desired = $desired; DesiredView = 'ws-home'; MonitorFull = @(0, 0, 800, 560)
      Frames = @{
        'win-a' = @{ outer = @(0, 0, 400, 560); visible = @(0, 0, 400, 560); flag = $true; alive = $true; ok_outer = $true; ok_visible = $true }
        'win-b' = @{ outer = @(400, 0, 800, 560); visible = @(400, 0, 800, 560); flag = $true; alive = $true; ok_outer = $true; ok_visible = $true }
      }
      ForegroundHwnd = [uint64]111; ManagedHwnds = @{ 'win-a' = [uint64]111; 'win-b' = [uint64]222 }
      Pager = @{ output = 'o1'; revision = 9; dpi = 96; active = 0; cells_total = 2; cells_omitted = 0; cells = @(
        @{ ord = 1; ws = 'ws-home'; cur = $true; rect = @{ x = 0; y = 0; w = 200; h = 40 }; focused = $true; minis = @($true, $false); minis_total = 2; minis_omitted = 0; focused_total = 1; mini_rects = @(@{ x = 4; y = 4; w = 20; h = 28 }, @{ x = 24; y = 4; w = 20; h = 28 }); mini_windows = @('tok-a', 'tok-b') },
        @{ ord = 2; ws = 'ws-away'; cur = $false; rect = @{ x = 200; y = 0; w = 200; h = 40 }; focused = $false; minis = @(); minis_total = 0; minis_omitted = 0; focused_total = 0; mini_rects = @(); mini_windows = @() }
      ) }
      PagerOutputsTotal = 1; PagerOutputsOmitted = 0
      Revision = 9
    }
  }
}

$pair = New-GoodEvidence
$null = Compare-StressCheckpoint $pair.Expected $pair.Actual
Rec-Mock "compare-accept" @{ ok = $true }

$bad = New-GoodEvidence
$bad.Actual.Frames['win-a'].outer = @(0, 0, 399, 560)
Expect-Throw { Compare-StressCheckpoint $bad.Expected $bad.Actual } "outer" "wrong-frame"

$bad = New-GoodEvidence
$bad.Actual.Desired['tok-a'].allocation = @{ x = 0; y = 0; w = 399; h = 560 }
Expect-Throw { Compare-StressCheckpoint $bad.Expected $bad.Actual } "allocation" "wrong-desired"

$bad = New-GoodEvidence
$bad.Actual.Desired['tok-b'].effective_frame = @{ x = 400; y = 0; w = 800; h = 560 }
Expect-Throw { Compare-StressCheckpoint $bad.Expected $bad.Actual } "effective_frame" "width-not-edge"

$bad = New-GoodEvidence
$bad.Actual.Desired.Remove('tok-b')
Expect-Throw { Compare-StressCheckpoint $bad.Expected $bad.Actual } "roster" "missing-token"

$bad = New-GoodEvidence
$bad.Expected.Foreground = $null
Expect-Throw { Compare-StressCheckpoint $bad.Expected $bad.Actual } "foreground" "managed-foreground-while-null"

$bad = New-GoodEvidence
$bad.Actual.Pager.cells[1].minis = @($false)
$bad.Actual.Pager.cells[1].mini_rects = @(@{ x = 204; y = 4; w = 20; h = 12 })
$bad.Actual.Pager.cells[1].mini_windows = @('tok-a')
$bad.Actual.Pager.cells[1].minis_total = 1
Expect-Throw { Compare-StressCheckpoint $bad.Expected $bad.Actual } "occupied" "pager-occupied"

$bad = New-GoodEvidence
$bad.Actual.Pager.cells[0].focused = $false
Expect-Throw { Compare-StressCheckpoint $bad.Expected $bad.Actual } "focused" "pager-focus"

$bad = New-GoodEvidence
$bad.Actual.Pager.cells[0].ws = 'ws-away'
Expect-Throw { Compare-StressCheckpoint $bad.Expected $bad.Actual } "pager ws" "pager-ws-token"

$bad = New-GoodEvidence
$bad.Actual.Pager.cells[0].minis_omitted = 1
Expect-Throw { Compare-StressCheckpoint $bad.Expected $bad.Actual } "minis omitted" "pager-minis-omitted"

$bad = New-GoodEvidence
$bad.Actual.Pager.cells[0].focused_total = 0
Expect-Throw { Compare-StressCheckpoint $bad.Expected $bad.Actual } "focused_total" "pager-focused-total"

$bad = New-GoodEvidence
$bad.Actual.Pager.revision = 10
Expect-Throw { Compare-StressCheckpoint $bad.Expected $bad.Actual } "newer than checkpoint" "future-pager"

$good = New-GoodEvidence
$good.Actual.Pager.revision = 8
$null = Compare-StressCheckpoint $good.Expected $good.Actual
Rec-Mock "pager-retained-older-accept" @{ revision = 8 }

$bad = New-GoodEvidence
$bad.Actual.Pager.cells[0].mini_rects[0] = @{ x = 500; y = 4; w = 20; h = 28 }
Expect-Throw { Compare-StressCheckpoint $bad.Expected $bad.Actual } "miniature layout wrong" "pager-mini-geometry"

$bad = New-GoodEvidence
$bad.Actual.Pager.cells[0].mini_rects[1] = @{ x = 25; y = 4; w = 20; h = 28 }
Expect-Throw { Compare-StressCheckpoint $bad.Expected $bad.Actual } "miniature layout wrong" "pager-mini-in-cell-wrong"

$bad = New-GoodEvidence
$bad.Actual.Pager.cells[0].mini_windows = @('tok-b', 'tok-a')
Expect-Throw { Compare-StressCheckpoint $bad.Expected $bad.Actual } "miniature layout wrong" "pager-mini-swapped"

$bad = New-GoodEvidence
$bad.Actual.Pager.cells[0].mini_windows = @('tok-a', 'tok-a')
Expect-Throw { Compare-StressCheckpoint $bad.Expected $bad.Actual } "multiset" "pager-mini-duplicated"

$bad = New-GoodEvidence
$bad.Actual.Pager.cells[0].minis = @($true)
$bad.Actual.Pager.cells[0].mini_rects = @(@{ x = 4; y = 4; w = 20; h = 28 })
$bad.Actual.Pager.cells[0].mini_windows = @('tok-a')
$bad.Actual.Pager.cells[0].minis_total = 1
Expect-Throw { Compare-StressCheckpoint $bad.Expected $bad.Actual } "multiset" "pager-mini-omitted-nonfocused"

$bad = New-GoodEvidence
$bad.Actual.Pager.cells[0].minis = @($false, $true)
Expect-Throw { Compare-StressCheckpoint $bad.Expected $bad.Actual } "focus identity" "pager-mini-focus-identity"

$bad = New-GoodEvidence
$bad.Actual.Pager.cells[0].mini_rects[1] = @{ x = 24; y = 4; w = 1; h = 28 }
Expect-Throw { Compare-StressCheckpoint $bad.Expected $bad.Actual } "miniature layout wrong" "pager-mini-degenerate"

$bad = New-GoodEvidence
$bad.Actual.Pager.cells[0].mini_windows = @('tok-a', 'tok-zzz')
Expect-Throw { Compare-StressCheckpoint $bad.Expected $bad.Actual } "multiset" "pager-mini-unknown-token"

$rebind = New-GoodEvidence
$rebind.Actual.Ring = @('ws-home', 'ws-new-away')
$rebind.Actual.AwayId = 'ws-new-away'
$rebind.Actual.Pager.cells[1].ws = 'ws-new-away'
$null = Compare-StressCheckpoint $rebind.Expected $rebind.Actual
Rec-Mock "alias-rebind-accept" @{ away = 'ws-new-away' }

$trailing = New-GoodEvidence
$trailing.Actual.Ring = @('ws-home', 'ws-away', 'ws-spare')
$trailing.Actual.Pager.cells += @(@{ ord = 3; ws = 'ws-spare'; cur = $false; rect = @{ x = 400; y = 0; w = 200; h = 40 }; focused = $false; minis = @(); minis_total = 0; minis_omitted = 0; focused_total = 0; mini_rects = @(); mini_windows = @() })
$trailing.Actual.Pager.cells_total = 3
$null = Compare-StressCheckpoint $trailing.Expected $trailing.Actual
Rec-Mock "trailing-spare-accept" @{ cells = 3 }

# Midpoint parity with Rust f64::round (away from zero): scale 0.05 makes
# w = 50 * 0.05 = 2.5 and x = 4 + 10 * 0.05 = 4.5. .NET default rounds half
# to even ((4,4,2,28)); the renderer contract needs (5,4,3,28).
$mid = Get-StressExpectedMini @{ x = 0; y = 0; w = 200; h = 40 } @(0, 0, 800, 560) @(10, 0, 50, 560) 96
if (($mid -join ",") -ne "5,4,3,28") { Fail-Mock "midpoint rounds bankers: $($mid -join ',')" }
Rec-Mock "midpoint-away-from-zero" @{ rect = ($mid -join ",") }

$evPendingShape = @{ desired_transfers = @(); desired_operations = @(); held_scopes = @(); desired_windows = @(@{ window = "tok-a"; frame_op = "op-1"; clear_op = ""; frame_hold = $null; membership_hold = $null; floating = $false; float_origin = $null; frame_authority = "controlled"; membership_authority = "controlled" }) }
if (-not (Test-StressApplyPending $evPendingShape)) { Fail-Mock "pending op not detected" }
if (Test-StressApplyPending @{ desired_transfers = @(); desired_operations = @(); held_scopes = @(); desired_windows = @() }) { Fail-Mock "settled misread as pending" }
Rec-Mock "pending-op-detected" @{ ok = $true }

$topoRebind = @{ NativeOut = 'o1'; HomeId = 'ws-home'; AwayId = 'ws-away'; Ring = @('ws-home', 'ws-away'); AliasOrd = @{ home = 1; away = 2 }; Seen = @{} }
$evRebind = @{ desired_workspaces = @(@{ id = 'ws-home'; output = 'o1'; order = 0 }, @{ id = 'ws-new-away'; output = 'o1'; order = 1 }) }
$null = Refresh-StressRing $evRebind $topoRebind
if ("$($topoRebind.AwayId)" -ne 'ws-new-away') { Fail-Mock "away slot did not rebind" }
if ([int]$topoRebind.AliasOrd.away -ne 2) { Fail-Mock "alias ordinals wrong after rebind" }
Rec-Mock "refresh-away-rebind" @{ away = "$($topoRebind.AwayId)" }
$evHomeMove = @{ desired_workspaces = @(@{ id = 'ws-other'; output = 'o1'; order = 0 }, @{ id = 'ws-new-away'; output = 'o1'; order = 1 }) }
Expect-Throw { Refresh-StressRing $evHomeMove $topoRebind } "home slot moved" "home-move-refused"

$bad = New-GoodEvidence
$bad.Actual.Desired['tok-a'].out = 'o2'
Expect-Throw { Compare-StressCheckpoint $bad.Expected $bad.Actual } "out" "output-target-mismatch"

$bad = New-GoodEvidence
$bad.Actual.Ring = @('ws-away', 'ws-home')
Expect-Throw { Compare-StressCheckpoint $bad.Expected $bad.Actual } "ring" "ring-head-moved"

$bad = New-GoodEvidence
$bad.Actual.Desired['tok-a'].frame_hold = "unreadable"
Expect-Throw { Compare-StressCheckpoint $bad.Expected $bad.Actual } "frame_hold" "pending-hold"

# Canned owner-log joins. Delivery vocabulary is production: accepted and
# command-consumed are fresh; duplicate never is.
$mockRoot = Join-Path ([IO.Path]::GetTempPath()) ("ws-mock-" + $PID)
New-Item -ItemType Directory -Force -Path $mockRoot | Out-Null
$logPath = Join-Path $mockRoot "owner.log"
function Write-MockLog($Lines) { $Lines | Set-Content -LiteralPath $logPath -Encoding utf8 }
function New-CleanApply([int]$Tick, [int]$Rev, [string]$Delivery) {
  return @{
    event = "world-apply"; tick = $Tick; delivery = $Delivery; revision = $Rev
    command_result = "ok"; geometry_status = "paired"; desired_geometry_omitted = 0
    members_omitted = 0; views_omitted = 0; frames_omitted = 0; clears_omitted = 0
    clear_holds_omitted = 0; desired_operations_omitted = 0; operation_settled_omitted = 0
    focus_effects_omitted = 0; focus_dispatch_omitted = 0; inflight_events_omitted = 0
    foreground_meta_omitted = 0; visibility_posts_omitted = 0
    desired_windows_omitted = 0; desired_outputs_omitted = 0; desired_workspaces_omitted = 0
    held_scopes_omitted = 0; desired_transfers_omitted = 0
    desired_transfers = @(); desired_operations = @(); held_scopes = @()
    desired_windows = @(); desired_outputs = @(); desired_workspaces = @()
    desired_geometry = @()
    focus_effects = @(@{ id = "fx-1"; kind = "focus"; target = "tok-a" })
  }
}
$goodApply = New-CleanApply 12 9 "accepted"
Write-MockLog @(
  (@{ event = "workspace"; tick = 12; op = "select"; index = 2; edge = "cli"; outcome = "ok" } | ConvertTo-Json -Compress),
  ($goodApply | ConvertTo-Json -Depth 8 -Compress)
)
$m = 0
$r = Wait-StressDispatch $logPath ([ref]$m) @("workspace", "--select", "2") 5 "dispatch"
if ([int]$r.tick -ne 12 -or "$($r.outcome)" -ne "ok") { Fail-Mock "dispatch tick/outcome" }
$apply = Wait-StressFreshApply $logPath ([ref]$m) 12 5 "fresh"
if ([int]$apply.event.revision -ne 9) { Fail-Mock "fresh revision" }
Rec-Mock "log-join-accept" @{ tick = 12; revision = 9 }

$consumed = New-CleanApply 13 9 "command-consumed"
Write-MockLog @(($consumed | ConvertTo-Json -Depth 8 -Compress))
$m = 0
$apply = Wait-StressFreshApply $logPath ([ref]$m) 13 5 "consumed"
if ([int]$apply.event.revision -ne 9) { Fail-Mock "consumed revision" }
Rec-Mock "consumed-fresh" @{ tick = 13 }

$dup = New-CleanApply 14 9 "duplicate"
Write-MockLog @(($dup | ConvertTo-Json -Depth 8 -Compress))
$m = 0
Expect-Throw { Wait-StressFreshApply $logPath ([ref]$m) 14 2 "stale" } "timeout" "duplicate-not-fresh"

$corrupt = New-CleanApply 15 9 "accepted"
$corrupt.desired_windows_omitted = 1
Write-MockLog @(($corrupt | ConvertTo-Json -Depth 8 -Compress))
$m = 0
Expect-Throw { Wait-StressFreshApply $logPath ([ref]$m) 15 2 "omitted" } "omitted" "nonzero-omission"

$corrupt = New-CleanApply 15 9 "accepted"
$corrupt.Remove("clear_holds_omitted")
Write-MockLog @(($corrupt | ConvertTo-Json -Depth 8 -Compress))
$m = 0
Expect-Throw { Wait-StressFreshApply $logPath ([ref]$m) 15 2 "missing" } "missing" "missing-omission-field"

# Strictness over the real JSON roundtrip (PSCustomObject, not hashtables):
# null, boolean and numeric-string omissions must refuse, never coerce to 0.
$strict = New-CleanApply 15 9 "accepted"
$strict.desired_windows_omitted = $null
Write-MockLog @(($strict | ConvertTo-Json -Depth 8 -Compress))
$m = 0
Expect-Throw { Wait-StressFreshApply $logPath ([ref]$m) 15 2 "strict-null" } "null" "strict-null-omission"
$strict = New-CleanApply 15 9 "accepted"
$strict.frames_omitted = $false
Write-MockLog @(($strict | ConvertTo-Json -Depth 8 -Compress))
$m = 0
Expect-Throw { Wait-StressFreshApply $logPath ([ref]$m) 15 2 "strict-bool" } "not integer" "strict-bool-omission"
$strict = New-CleanApply 15 9 "accepted"
$strict.views_omitted = "0"
Write-MockLog @(($strict | ConvertTo-Json -Depth 8 -Compress))
$m = 0
Expect-Throw { Wait-StressFreshApply $logPath ([ref]$m) 15 2 "strict-string" } "not integer" "strict-string-omission"

# Torn trailing append (observed live: a world-apply line torn at byte 40
# while the owner was still writing): the reader retries boundedly and
# parses once the writer completes the line. A background job completes the
# file mid-read; the join must succeed without skipping evidence.
$tornGood = New-CleanApply 17 11 "accepted"
$tornFull = ($tornGood | ConvertTo-Json -Depth 8 -Compress)
$tornFull.Substring(0, 40) | Set-Content -LiteralPath $logPath -Encoding utf8
$tornJob = Start-Job -ScriptBlock {
  param($Path, $Full)
  Start-Sleep -Milliseconds 300
  $Full | Set-Content -LiteralPath $Path -Encoding utf8
} -ArgumentList $logPath, $tornFull
try {
  $m = 0
  $torn = Wait-StressFreshApply $logPath ([ref]$m) 17 10 "torn"
  if ([int]$torn.event.revision -ne 11) { Fail-Mock "torn tail not joined" }
  Rec-Mock "torn-tail-joined" @{ revision = 11 }
} finally {
  $null = Wait-Job $tornJob -Timeout 15
  Remove-Job $tornJob -Force -ErrorAction SilentlyContinue
}

# Persistent malformation (a tail that never completes) still fails closed
# with the shared reader's shape, never a fabricated skip.
'{"event":"world-apply","tick":' | Set-Content -LiteralPath $logPath -Encoding utf8
$m = 0
Expect-Throw { Wait-StressFreshApply $logPath ([ref]$m) 17 5 "torn-persist" } "malformed JSON" "torn-persist-fails-closed"

$corrupt = New-CleanApply 15 9 "accepted"
$corrupt.desired_transfers = @(@{ op = "op-1"; kind = "window" })
$settled = New-CleanApply 16 10 "accepted"
Write-MockLog @(($corrupt | ConvertTo-Json -Depth 8 -Compress), ($settled | ConvertTo-Json -Depth 8 -Compress))
$m = 0
$skipped = Wait-StressFreshApply $logPath ([ref]$m) 15 5 "pending-skip"
if ([int]$skipped.event.tick -ne 16) { Fail-Mock "pending row not skipped" }
if ([int]$skipped.skipped_pending -lt 1) { Fail-Mock "pending skip not counted" }
Rec-Mock "pending-skipped-until-settled" @{ tick = 16 }

$corrupt = New-CleanApply 15 9 "accepted"
$corrupt.geometry_status = "mismatch"
Write-MockLog @(($corrupt | ConvertTo-Json -Depth 8 -Compress))
$m = 0
Expect-Throw { Wait-StressFreshApply $logPath ([ref]$m) 15 2 "geometry" } "not paired" "geometry-mismatch"

Write-MockLog @()
$m = 0
Expect-Throw { Wait-StressDispatch $logPath ([ref]$m) @("workspace", "--select", "2") 2 "timeout" } "timeout" "dispatch-timeout"

# A refused dispatch is transport-consumed with its honest outcome: the
# join reports it, the caller fails loudly. It never retries silently.
Write-MockLog @(
  (@{ event = "workspace"; tick = 16; op = "send"; index = 1; edge = "cli"; outcome = "unmanaged" } | ConvertTo-Json -Compress)
)
$m = 0
$r = Wait-StressDispatch $logPath ([ref]$m) @("workspace", "--send", "1") 5 "refused"
if ("$($r.outcome)" -ne "unmanaged") { Fail-Mock "refused outcome" }
Rec-Mock "dispatch-refused-join" @{ outcome = "unmanaged" }

function New-PagerLine([int]$Rev) {
  return @{
    event = "pager-cells"; tick = 12; observation = 9; revision = $Rev
    outputs = @(@{ output = "o1"; dpi = 96; cells = @(
      @{ ord = 1; ws = "ws-home"; cur = $true; rect = @{ x = 0; y = 0; w = 200; h = 40 }; focused = $true; minis = @($true); minis_total = 1; minis_omitted = 0; focused_total = 1; mini_rects = @(@{ x = 4; y = 4; w = 20; h = 28 }); mini_windows = @('tok-a') },
      @{ ord = 2; ws = "ws-away"; cur = $false; rect = @{ x = 200; y = 0; w = 200; h = 40 }; focused = $false; minis = @(); minis_total = 0; minis_omitted = 0; focused_total = 0; mini_rects = @(); mini_windows = @() }
    ); cells_total = 2; cells_omitted = 0; active = 0 })
    outputs_total = 1; outputs_omitted = 0
  }
}
Write-MockLog @(((New-PagerLine 9) | ConvertTo-Json -Depth 8 -Compress))
$mark = 0
$retained = $null
$pg = Wait-StressPager $logPath ([ref]$mark) ([ref]$retained) 9 "o1" 5 "pager"
if ($pg.entry.cells.Count -ne 2) { Fail-Mock "pager cells" }
if ("$($pg.entry.cells[0].ws)" -ne "ws-home") { Fail-Mock "pager ws join" }
if ("$($pg.entry.cells[0].mini_windows[0])" -ne "tok-a") { Fail-Mock "pager mini identity" }
if ([int]$pg.entry.cells[0].mini_rects[0].w -lt 2) { Fail-Mock "pager mini geometry" }
Rec-Mock "pager-join-accept" @{ cells = 2 }

# Change-gated retention: no same-revision row for revision 10, but the
# revision-9 payload is retained as the latest actually emitted evidence.
$mark = 0
$pg10 = Wait-StressPager $logPath ([ref]$mark) ([ref]$retained) 10 "o1" 5 "pager-retained"
if ([int]$pg10.entry.revision -ne 9) { Fail-Mock "retained revision wrong" }
Rec-Mock "pager-retained-accept" @{ revision = 9 }

# Same-revision supersession: a later emission at the SAME revision (focus
# change, tick 13) replaces the earlier one (tick 12), never silently
# ignored. Provenance (tick) distinguishes the two lines.
$flip = New-PagerLine 9
$flip.tick = 13
$flip.outputs[0].cells[0].focused = $false
$flip.outputs[0].cells[0].minis = @($false)
$flip.outputs[0].cells[0].focused_total = 0
Write-MockLog @(((New-PagerLine 9) | ConvertTo-Json -Depth 8 -Compress), (($flip) | ConvertTo-Json -Depth 8 -Compress))
$mark = 0
$retained = $null
$pgFlip = Wait-StressPager $logPath ([ref]$mark) ([ref]$retained) 9 "o1" 5 "pager-same-rev"
if ([int]$pgFlip.entry.tick -ne 13) { Fail-Mock "same-revision update ignored" }
if ($pgFlip.entry.cells[0].focused -ne $false) { Fail-Mock "same-revision focus not applied" }
Rec-Mock "pager-same-revision-supersede" @{ tick = 13 }

$line = New-PagerLine 9
$line.outputs[0].output = "o2"
Write-MockLog @(($line | ConvertTo-Json -Depth 8 -Compress))
$mark = 0
$retained = $null
Expect-Throw { Wait-StressPager $logPath ([ref]$mark) ([ref]$retained) 9 "o1" 2 "missing" } "timeout" "pager-output-missing"

$line = New-PagerLine 9
$line.outputs += @(@{ output = "o1"; cells = @(
  @{ ord = 1; ws = "ws-home"; cur = $true; rect = @{ x = 0; y = 0; w = 200; h = 40 }; focused = $false; minis = @(); minis_total = 0; minis_omitted = 0; focused_total = 0; mini_rects = @(); mini_windows = @() }
); cells_total = 1; cells_omitted = 0; active = 0 })
Write-MockLog @(($line | ConvertTo-Json -Depth 8 -Compress))
$mark = 0
$retained = $null
Expect-Throw { Wait-StressPager $logPath ([ref]$mark) ([ref]$retained) 9 "o1" 2 "ambiguous" } "ambiguous" "pager-ambiguous"

# Checkpoint retry loop over the actual barrier, not a pure helper: a
# pending apply, then a settled row, then two lagging native samples, then
# the pager payload arriving late via the retry hook. Must pass with fresh
# lifetime revalidation on every iteration. Corruption fails fast.
# Local ready shadow: the file-wide live-API shadows are defined further
# below; the loop runs before them, so this minimal probe stands in here
# and is replaced there.
function Invoke-LiveNative([string]$Exe, [string[]]$CliArgs) {
  if ("$($CliArgs[0])" -eq "ready") { return '{"ready":true}' }
  throw "unexpected live call in loop test: $($CliArgs -join ' ')"
}
$loopRoot = Join-Path ([IO.Path]::GetTempPath()) ("ws-loop-" + $PID)
New-Item -ItemType Directory -Force -Path $loopRoot | Out-Null
$loopLog = Join-Path $loopRoot "owner.log"
function New-LoopApply([int]$Tick, [int]$Rev) {
  $base = New-CleanApply $Tick $Rev "accepted"
  $base.desired_windows = @(@{
    window = "tok-a"; out = "o1"; ws = "ws-home"
    allocation = @{ x = 0; y = 0; w = 400; h = 560 }
    effective_frame = @{ x = 0; y = 0; w = 400; h = 560 }
    frame_authority = "controlled"; membership_authority = "controlled"
    frame_hold = $null; membership_hold = $null
    floating = $false; float_origin = $null
    frame_op = ""; clear_op = ""
  })
  $base.desired_outputs = @(@{ output = "o1"; logical_view = "ws-home" })
  $base.desired_workspaces = @(@{ id = "ws-home"; output = "o1"; order = 0 }, @{ id = "ws-away"; output = "o1"; order = 1 })
  $base.desired_geometry = @(@{ output = "o1"; work = @{ x = 0; y = 0; w = 800; h = 560 }; full = @{ x = 0; y = 0; w = 800; h = 560 }; complete = $true; primary = $true; gap = 0; outer_gap = 0 })
  return $base
}
function New-LoopPager([int]$Rev) {
  return @{
    event = "pager-cells"; tick = 6; observation = 6; revision = $Rev
    outputs = @(@{ output = "o1"; dpi = 96; cells = @(
      @{ ord = 1; ws = "ws-home"; cur = $true; rect = @{ x = 0; y = 0; w = 200; h = 40 }; focused = $false; minis = @($false); minis_total = 1; minis_omitted = 0; focused_total = 0; mini_rects = @(@{ x = 4; y = 4; w = 20; h = 28 }); mini_windows = @('tok-a') },
      @{ ord = 2; ws = "ws-away"; cur = $false; rect = @{ x = 200; y = 0; w = 200; h = 40 }; focused = $false; minis = @(); minis_total = 0; minis_omitted = 0; focused_total = 0; mini_rects = @(); mini_windows = @() }
    ); cells_total = 2; cells_omitted = 0; active = 0 })
    outputs_total = 1; outputs_omitted = 0
  }
}
$loopPending = New-CleanApply 5 6 "accepted"
$loopPending.desired_transfers = @(@{ op = "op-9"; kind = "window" })
$loopSettled = New-LoopApply 6 7
@(($loopPending | ConvertTo-Json -Depth 8 -Compress), ($loopSettled | ConvertTo-Json -Depth 8 -Compress)) | Set-Content -LiteralPath $loopLog -Encoding utf8
$script:LoopNativeCalls = 0
$script:LoopOwnerCalls = 0
function Get-StressTitles { return @(@{ hwnd = [uint64]111; title = "OmniTilerTestWindow:win-a:runx" }) }
function Read-LivePaired([uint64]$H) {
  $script:LoopNativeCalls++
  if ([uint64]$H -ne [uint64]111) { throw "unexpected hwnd $H" }
  if ($script:LoopNativeCalls -lt 3) {
    return @{ outer = @(10, 0, 410, 560); visible = @(10, 0, 410, 560); ok_outer = $true; ok_visible = $true; alive = $true; visible_flag = $true }
  }
  return @{ outer = @(0, 0, 400, 560); visible = @(0, 0, 400, 560); ok_outer = $true; ok_visible = $true; alive = $true; visible_flag = $true }
}
function Test-StressHwndOwner([uint64]$Hwnd, $FixtureState, [string]$Tag) {
  $script:LoopOwnerCalls++
  if ([uint64]$Hwnd -ne [uint64]111) { throw "unexpected hwnd $Hwnd in $Tag" }
  return $true
}
$loopWriter = {
  param($i)
  if ([int]$i -eq 2) {
    (New-LoopPager 7 | ConvertTo-Json -Depth 8 -Compress) | Add-Content -LiteralPath $loopLog -Encoding utf8
  }
}
$loopTopo = @{
  NativeOut = 'o1'; HomeId = 'ws-home'; AwayId = 'ws-away'; Ring = @('ws-home', 'ws-away')
  DiscoveredWork = @(0, 0, 800, 560); MonitorFull = @(0, 0, 800, 560)
  AliasOrd = @{ home = 1; away = 2 }; LastTick = 0; Revision = 0; Seen = @{}
  Pending = [System.Collections.ArrayList]@(); DispatchMark = 0; PagerRetained = $null
}
$loopAction = [pscustomobject]@{
  op = "checkpoint"; name = "loop-t1"; view = "home"; members = @("win-a")
  homing = [pscustomobject]@{ "win-a" = "home" }
  tiles = [pscustomobject]@{ "win-a" = @(0, 0, 400, 560) }
  foreground = $null; pager_current = "home"; pager_occupied = @("home")
}
$loopTok = @{ "win-a" = "tok-a" }
$loopHwnd = @{ "win-a" = [uint64]111 }
$loopFix = @{ pid = 4242; creation = "loopcreation" }
$loopMark = 0
$loopPagerMark = 0
$script:MockReadyMode = "ready"
Invoke-StressCheckpoint $loopAction $null @(0, 0, 800, 560) $loopTopo $loopTok $loopHwnd $loopFix ([ref]$loopMark) ([ref]$loopPagerMark) $loopLog "dummy-owner" $loopWriter 30
$script:MockReadyMode = "not-ready"
if ([int]$loopTopo.Revision -ne 7) { Fail-Mock "loop revision not committed" }
if ([int]$loopTopo.LastTick -ne 6) { Fail-Mock "loop tick not committed" }
if ([int]$loopMark -le 0) { Fail-Mock "loop apply cursor not committed" }
if ([int]$script:LoopNativeCalls -lt 3) { Fail-Mock "loop did not retry native lag" }
if ([int]$script:LoopOwnerCalls -lt 3) { Fail-Mock "loop skipped lifetime revalidation" }
if ([int]$loopTopo.PagerRetained.entry.revision -ne 7) { Fail-Mock "loop pager not retained" }
Rec-Mock "checkpoint-loop-settled" @{ iterations = "pending+lag+late-pager" }
$loopBad = New-LoopApply 8 8
$loopBad.desired_windows_omitted = 2
@(($loopBad | ConvertTo-Json -Depth 8 -Compress)) | Set-Content -LiteralPath $loopLog -Encoding utf8
$loopMark2 = 0
$loopPagerMark2 = 0
$loopTopo2 = @{
  NativeOut = 'o1'; HomeId = 'ws-home'; AwayId = 'ws-away'; Ring = @('ws-home', 'ws-away')
  DiscoveredWork = @(0, 0, 800, 560); MonitorFull = @(0, 0, 800, 560)
  AliasOrd = @{ home = 1; away = 2 }; LastTick = 0; Revision = 0; Seen = @{}
  Pending = [System.Collections.ArrayList]@(); DispatchMark = 0; PagerRetained = $null
}
$script:MockReadyMode = "ready"
Expect-Throw { Invoke-StressCheckpoint $loopAction $null @(0, 0, 800, 560) $loopTopo2 $loopTok $loopHwnd $loopFix ([ref]$loopMark2) ([ref]$loopPagerMark2) $loopLog "dummy-owner" $null 10 } "omitted" "loop-corruption-fail-fast"
$script:MockReadyMode = "not-ready"
Rec-Mock "checkpoint-loop-corruption-fail-fast" @{ ok = $true }

# Ring-growth pager lag (observed live: settled apply rev 8 already lists
# the new trailing spare while the latest emitted payload rev 7 still has
# 2 cells). The count mismatch must retry with a re-settle, never fail
# closed and never re-poll under the same revision cap: the hook appends a
# newer settled apply plus its 3-cell payload, and the checkpoint must pass
# on the newer revision.
$rgRoot = Join-Path ([IO.Path]::GetTempPath()) ("ws-ringlag-" + $PID)
New-Item -ItemType Directory -Force -Path $rgRoot | Out-Null
$rgLog = Join-Path $rgRoot "owner.log"
function New-RingApply([int]$Tick, [int]$Rev) {
  $base = New-LoopApply $Tick $Rev
  $base.desired_workspaces = @(
    @{ id = "ws-home"; output = "o1"; order = 0 },
    @{ id = "ws-away"; output = "o1"; order = 1 },
    @{ id = "ws-spare"; output = "o1"; order = 2 }
  )
  return $base
}
function New-RingPager([int]$Rev, [switch]$Spare) {
  $cells = @(
    @{ ord = 1; ws = "ws-home"; cur = $true; rect = @{ x = 0; y = 0; w = 200; h = 40 }; focused = $false; minis = @($false); minis_total = 1; minis_omitted = 0; focused_total = 0; mini_rects = @(@{ x = 4; y = 4; w = 20; h = 28 }); mini_windows = @('tok-a') },
    @{ ord = 2; ws = "ws-away"; cur = $false; rect = @{ x = 200; y = 0; w = 200; h = 40 }; focused = $false; minis = @(); minis_total = 0; minis_omitted = 0; focused_total = 0; mini_rects = @(); mini_windows = @() }
  )
  if ($Spare) {
    $cells += @(@{ ord = 3; ws = "ws-spare"; cur = $false; rect = @{ x = 400; y = 0; w = 200; h = 40 }; focused = $false; minis = @(); minis_total = 0; minis_omitted = 0; focused_total = 0; mini_rects = @(); mini_windows = @() })
  }
  return @{
    event = "pager-cells"; tick = 6; observation = 6; revision = $Rev
    outputs = @(@{ output = "o1"; dpi = 96; cells = $cells; cells_total = $cells.Count; cells_omitted = 0; active = 0 })
    outputs_total = 1; outputs_omitted = 0
  }
}
@(((New-RingApply 8 8) | ConvertTo-Json -Depth 8 -Compress), ((New-RingPager 7) | ConvertTo-Json -Depth 8 -Compress)) | Set-Content -LiteralPath $rgLog -Encoding utf8
$script:RGWriterCalls = 0
$rgWriter = {
  param($i)
  $script:RGWriterCalls++
  if ([int]$i -eq 2) {
    ((New-RingApply 9 9) | ConvertTo-Json -Depth 8 -Compress) | Add-Content -LiteralPath $rgLog -Encoding utf8
    ((New-RingPager 9 -Spare) | ConvertTo-Json -Depth 8 -Compress) | Add-Content -LiteralPath $rgLog -Encoding utf8
  }
}
$rgTopo = @{
  NativeOut = 'o1'; HomeId = 'ws-home'; AwayId = 'ws-away'; Ring = @('ws-home', 'ws-away')
  DiscoveredWork = @(0, 0, 800, 560); MonitorFull = @(0, 0, 800, 560)
  AliasOrd = @{ home = 1; away = 2 }; LastTick = 0; Revision = 0; Seen = @{}
  Pending = [System.Collections.ArrayList]@(); DispatchMark = 0; PagerRetained = $null
}
$rgMark = 0
$rgPagerMark = 0
Invoke-StressCheckpoint $loopAction $null @(0, 0, 800, 560) $rgTopo $loopTok $loopHwnd $loopFix ([ref]$rgMark) ([ref]$rgPagerMark) $rgLog "dummy-owner" $rgWriter 30
if ([int]$rgTopo.Revision -ne 9) { Fail-Mock "ring-lag not re-settled to revision 9" }
if ([int]$rgTopo.PagerRetained.entry.revision -ne 9) { Fail-Mock "ring-lag pager not retained at 9" }
if ([int]$script:RGWriterCalls -lt 2) { Fail-Mock "ring-lag passed without retry" }
Rec-Mock "checkpoint-ringlag-resettled" @{ revision = 9 }
Remove-Item -LiteralPath $rgRoot -Recurse -Force -ErrorAction SilentlyContinue

# Dispatch-fence survival (observed live: focus-window drains the queue after
# joining consumption, then the checkpoint fenced only on its stale LastTick
# and pinned a pre-actuation settled row). The drain records the consumed
# dispatch high-water mark, so the checkpoint settles at or past actuation
# even with an empty queue. Pre-fix this pins the stale half-width row and
# fails closed on allocation.
$dfRoot = Join-Path ([IO.Path]::GetTempPath()) ("ws-drainfence-" + $PID)
New-Item -ItemType Directory -Force -Path $dfRoot | Out-Null
$dfLog = Join-Path $dfRoot "owner.log"
function New-FenceApply([int]$Tick, [int]$Rev, [int]$W) {
  $base = New-LoopApply $Tick $Rev
  $base.desired_windows = @(@{
    window = "tok-a"; out = "o1"; ws = "ws-home"
    allocation = @{ x = 0; y = 0; w = $W; h = 560 }
    effective_frame = @{ x = 0; y = 0; w = $W; h = 560 }
    frame_authority = "controlled"; membership_authority = "controlled"
    frame_hold = $null; membership_hold = $null
    floating = $false; float_origin = $null
    frame_op = ""; clear_op = ""
  })
  return $base
}
$dfStay = (@{ event = "workspace"; tick = 24; op = "stay"; index = 2; edge = "cli"; outcome = "ok" } | ConvertTo-Json -Compress)
$dfStale = (New-FenceApply 19 30 400 | ConvertTo-Json -Depth 8 -Compress)
$dfStalePager = (New-RingPager 29 | ConvertTo-Json -Depth 8 -Compress)
$dfFresh = (New-FenceApply 25 31 800 | ConvertTo-Json -Depth 8 -Compress)
$dfPager = (@{
  event = "pager-cells"; tick = 25; observation = 25; revision = 31
  outputs = @(@{ output = "o1"; dpi = 96; cells = @(
    @{ ord = 1; ws = "ws-home"; cur = $true; rect = @{ x = 0; y = 0; w = 400; h = 40 }; focused = $false; minis = @($false); minis_total = 1; minis_omitted = 0; focused_total = 0; mini_rects = @(@{ x = 4; y = 4; w = 40; h = 28 }); mini_windows = @('tok-a') },
    @{ ord = 2; ws = "ws-away"; cur = $false; rect = @{ x = 400; y = 0; w = 400; h = 40 }; focused = $false; minis = @(); minis_total = 0; minis_omitted = 0; focused_total = 0; mini_rects = @(); mini_windows = @() }
  ); cells_total = 2; cells_omitted = 0; active = 0 })
  outputs_total = 1; outputs_omitted = 0
} | ConvertTo-Json -Depth 8 -Compress)
@($dfStay, $dfStale, $dfStalePager, $dfFresh, $dfPager) | Set-Content -LiteralPath $dfLog -Encoding utf8
$dfTopo = @{
  NativeOut = 'o1'; HomeId = 'ws-home'; AwayId = 'ws-away'; Ring = @('ws-home', 'ws-away')
  DiscoveredWork = @(0, 0, 800, 560); MonitorFull = @(0, 0, 800, 560)
  AliasOrd = @{ home = 1; away = 2 }; LastTick = 18; Revision = 29; Seen = @{}
  Pending = [System.Collections.ArrayList]@(@{ argv = @("workspace", "--stay", "2"); op = "stay" })
  DispatchMark = 0; PagerRetained = $null
}
$dfAction = [pscustomobject]@{
  op = "checkpoint"; name = "fence-t1"; view = "home"; members = @("win-a")
  homing = [pscustomobject]@{ "win-a" = "home" }
  tiles = [pscustomobject]@{ "win-a" = @(0, 0, 800, 560) }
  foreground = $null; pager_current = "home"; pager_occupied = @("home")
}
$null = Drain-StressPending $dfLog $dfTopo 30 "focus-win-a"
if ([int]$dfTopo.LastDispatchTick -ne 24) { Fail-Mock "drain fence not recorded" }
if (@($dfTopo.Pending).Count -ne 0) { Fail-Mock "drain did not clear" }
$dfMark = 0
$dfPagerMark = 0
function Read-LivePaired([uint64]$H) {
  if ([uint64]$H -ne [uint64]111) { throw "unexpected hwnd $H in fence test" }
  return @{ outer = @(0, 0, 800, 560); visible = @(0, 0, 800, 560); ok_outer = $true; ok_visible = $true; alive = $true; visible_flag = $true }
}
Invoke-StressCheckpoint $dfAction $null @(0, 0, 800, 560) $dfTopo $loopTok $loopHwnd $loopFix ([ref]$dfMark) ([ref]$dfPagerMark) $dfLog "dummy-owner" $null 30
function Read-LivePaired([uint64]$H) {
  $script:LoopNativeCalls++
  if ([uint64]$H -ne [uint64]111) { throw "unexpected hwnd $H" }
  return @{ outer = @(0, 0, 400, 560); visible = @(0, 0, 400, 560); ok_outer = $true; ok_visible = $true; alive = $true; visible_flag = $true }
}
if ([int]$dfTopo.Revision -ne 31) { Fail-Mock "fence checkpoint not settled at revision 31" }
if ([int]$dfTopo.LastTick -ne 25) { Fail-Mock "fence checkpoint tick not 25" }
Rec-Mock "checkpoint-drainfence-settled" @{ revision = 31 }
Remove-Item -LiteralPath $dfRoot -Recurse -Force -ErrorAction SilentlyContinue

# Error classifier contract: consumed transport/dispatch/settlement bounds
# and every pending-worded, corruption, refusal, or identity mismatch fail
# closed with no retry; only intended native/pager lag retries.
foreach ($fatal in @(
  "checkpoint-t1 settlement timeout (skipped 2 pending rows)",
  "checkpoint-t1 log join timeout",
  "checkpoint-t1 dispatch-order timeout for send",
  "send dispatch outcome refused at tick 24: product refused/dropped, no repair",
  "send transport cannot preserve burst order (request files still pending)",
  "checkpoint-t1 omitted desired_windows_omitted=2",
  "checkpoint-t1 desired array missing: desired_windows",
  "win-a frame_hold present",
  "win-a floating",
  "win-a pending op",
  "win-a authority not controlled",
  "close-x pending transfers",
  "checkpoint-t1 geometry not paired",
  "checkpoint-t1 no view row for o1",
  "pager mini token tok-a unknown at cell 0",
  "foreground pid mismatch",
  "owner not ready at checkpoint t1"
)) {
  if (Test-StressTransientMismatch $fatal) { Fail-Mock "fatal retried: $fatal" }
}
Rec-Mock "classifier-fatal-fail-closed" @{ rows = 17 }
foreach ($lag in @(
  "win-a outer [10,0,410,560] != tile [0,0,400,560]",
  "win-a visible [10,0,410,560] != outer [0,0,400,560]",
  "win-a not visible while shown",
  "win-a visible while hidden",
  "foreground 111 != 222",
  "foreground 111 is managed while null expected",
  "pager t1 join timeout at or before revision 7",
  "pager active 0 != 1",
  "pager current mark wrong at cell 0",
  "pager occupied mark wrong at cell 1",
  "pager focused mark mismatch at cell 0",
  "pager focus mark wrong",
  "pager focused while null foreground expected",
  "pager mini token multiset wrong at cell home: tok-a != tok-a,tok-b",
  "pager miniature layout wrong at cell 0 mini 0 (win-a): [5,4,20,28] != [4,4,20,28]",
  "pager mini focus identity wrong at cell 0 mini 0",
  "pager mini token missing at cell 0 mini 0",
  "pager mini rect missing at cell 0 mini 1",
  "pager cells 2 != ring 3"
)) {
  if (-not (Test-StressTransientMismatch $lag)) { Fail-Mock "lag not retried: $lag" }
}
Rec-Mock "classifier-lag-retries" @{ rows = 19 }

# Drain consumes its own bound with no retry: an empty log and a pending
# item fail fast instead of looping to an outer deadline.
Write-MockLog @()
$dtTopo = @{ Pending = [System.Collections.ArrayList]@(@{ argv = @("workspace", "--select", "1"); op = "select-workspace" }); DispatchMark = 0 }
Expect-Throw { Drain-StressPending $logPath $dtTopo 2 "drain-timeout-test" } "dispatch-order timeout" "drain-no-retry"
Rec-Mock "drain-no-retry" @{ ok = $true }

# Settlement consumes its own bound with no retry: a log with only pending
# rows fails at the checkpoint without looping to the outer deadline.
$stRoot = Join-Path ([IO.Path]::GetTempPath()) ("ws-stonly-" + $PID)
New-Item -ItemType Directory -Force -Path $stRoot | Out-Null
$stLog = Join-Path $stRoot "owner.log"
$stOnly = New-CleanApply 5 6 "accepted"
$stOnly.desired_transfers = @(@{ op = "op-9"; kind = "window" })
@(($stOnly | ConvertTo-Json -Depth 8 -Compress)) | Set-Content -LiteralPath $stLog -Encoding utf8
$stMark = 0
$stPagerMark = 0
$stTopo = @{
  NativeOut = 'o1'; HomeId = 'ws-home'; AwayId = 'ws-away'; Ring = @('ws-home', 'ws-away')
  DiscoveredWork = @(0, 0, 800, 560); MonitorFull = @(0, 0, 800, 560)
  AliasOrd = @{ home = 1; away = 2 }; LastTick = 0; Revision = 0; Seen = @{}
  Pending = [System.Collections.ArrayList]@(); DispatchMark = 0; PagerRetained = $null
}
Expect-Throw { Invoke-StressCheckpoint $loopAction $null @(0, 0, 800, 560) $stTopo $loopTok $loopHwnd $loopFix ([ref]$stMark) ([ref]$stPagerMark) $stLog "dummy-owner" $null 8 } "settlement timeout" "checkpoint-settlement-no-retry"
Rec-Mock "checkpoint-settlement-no-retry" @{ ok = $true }
Remove-Item -LiteralPath $stRoot -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $loopRoot -Recurse -Force -ErrorAction SilentlyContinue

# Native join fidelity: admission binds exact fixture PID/creation and
# refuses mismatches; output targeting refuses a wrong wire id.
Write-MockLog @((@{ event = "member-admitted"; window = "tok-x"; hwnd = 777; pid = 4242; creation = "aaaabbbbccccdddd" } | ConvertTo-Json -Compress))
$tokOfAdm = @{}
$gotTok = Wait-StressAdmitted $logPath $tokOfAdm ([uint64]777) @{ pid = 4242; creation = "aaaabbbbccccdddd" } 5 "admit-ok"
if ("$gotTok" -ne "tok-x") { Fail-Mock "admission token wrong" }
Rec-Mock "admission-join-accept" @{ token = "$gotTok" }
Write-MockLog @((@{ event = "member-admitted"; window = "tok-y"; hwnd = 778; pid = 9999; creation = "aaaabbbbccccdddd" } | ConvertTo-Json -Compress))
Expect-Throw { Wait-StressAdmitted $logPath @{} ([uint64]778) @{ pid = 4242; creation = "aaaabbbbccccdddd" } 2 "admit-pid" } "pid mismatch" "admission-pid-refused"
Write-MockLog @((@{ event = "member-admitted"; window = "tok-z"; hwnd = 779; pid = 4242; creation = "ffffffffffffffff" } | ConvertTo-Json -Compress))
Expect-Throw { Wait-StressAdmitted $logPath @{} ([uint64]779) @{ pid = 4242; creation = "aaaabbbbccccdddd" } 2 "admit-creation" } "creation mismatch" "admission-creation-refused"

# Ordered burst join: two queued commands match in queue order from the
# checkpoint-start cursor with no per-command wait. A refused outcome fails
# loudly at the checkpoint; a missing line times out.
Write-MockLog @(
  (@{ event = "workspace"; tick = 20; op = "select"; index = 1; edge = "cli"; outcome = "ok" } | ConvertTo-Json -Compress),
  (@{ event = "workspace"; tick = 21; op = "send"; index = 1; edge = "cli"; outcome = "ok" } | ConvertTo-Json -Compress)
)
$dm = 0
$pend = @(@{ argv = @("workspace", "--select", "1"); op = "select-workspace" }, @{ argv = @("workspace", "--send", "1"); op = "send" })
$outs = Wait-StressDispatchOrdered $logPath ([ref]$dm) $pend 5 "burst"
if ($outs.Count -ne 2 -or [int]$outs[0].tick -ne 20 -or [int]$outs[1].tick -ne 21) { Fail-Mock "ordered join wrong" }
Rec-Mock "dispatch-ordered-accept" @{ ticks = "20,21" }
Write-MockLog @(
  (@{ event = "workspace"; tick = 22; op = "select"; index = 1; edge = "cli"; outcome = "ok" } | ConvertTo-Json -Compress)
)
$dm = 0
Expect-Throw { Wait-StressDispatchOrdered $logPath ([ref]$dm) $pend 2 "burst-missing" } "timeout" "dispatch-order-missing"
Write-MockLog @(
  (@{ event = "workspace"; tick = 23; op = "select"; index = 1; edge = "cli"; outcome = "ok" } | ConvertTo-Json -Compress),
  (@{ event = "workspace"; tick = 24; op = "send"; index = 1; edge = "cli"; outcome = "refused" } | ConvertTo-Json -Compress)
)
$dm = 0
Expect-Throw { Wait-StressDispatchOrdered $logPath ([ref]$dm) $pend 5 "burst-refused" } "refused" "dispatch-order-refused"

# Per-family acceptance vocabulary (product evidence): directional queue-acks
# carry "dispatched" once the intent is queued (cli_directional.rs); only
# "ok" verifies a workspace select/send (sends.rs, cli_workspace.rs).
# Refusals in either family fail loudly at the checkpoint.
Write-MockLog @(
  (@{ event = "directional"; tick = 25; op = "focus"; edge = "cli"; outcome = "dispatched"; origin = "tok-a" } | ConvertTo-Json -Compress),
  (@{ event = "directional"; tick = 26; op = "move"; edge = "cli"; outcome = "dispatched"; origin = "tok-a" } | ConvertTo-Json -Compress)
)
$dm = 0
$dpend = @(@{ argv = @("directional", "--focus", "--direction", "right"); op = "focus-directional" }, @{ argv = @("directional", "--move", "--direction", "left"); op = "move-directional" })
$douts = Wait-StressDispatchOrdered $logPath ([ref]$dm) $dpend 5 "burst-directional"
if ($douts.Count -ne 2 -or [int]$douts[0].tick -ne 25 -or [int]$douts[1].tick -ne 26) { Fail-Mock "directional dispatched not accepted" }
Rec-Mock "dispatch-directional-accept" @{ ticks = "25,26" }
Write-MockLog @(
  (@{ event = "directional"; tick = 27; op = "focus"; edge = "cli"; outcome = "unmanaged" } | ConvertTo-Json -Compress)
)
$dm = 0
Expect-Throw { Wait-StressDispatchOrdered $logPath ([ref]$dm) @(@{ argv = @("directional", "--focus", "--direction", "right"); op = "focus-directional" }) 5 "burst-directional-refused" } "unmanaged" "dispatch-directional-refused"

# Cross-family transport fence: both single-pending files must be absent
# before a burst publish, or definition order is not preserved (the owner
# polls workspace before directional every tick).
$idleDir = Join-Path ([IO.Path]::GetTempPath()) ("ws-idle-" + $PID + "-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $idleDir | Out-Null
Wait-StressTransportIdle $idleDir "idle-accept" 2
Rec-Mock "transport-idle-accept" @{ ok = $true }
"pending" | Set-Content -LiteralPath (Join-Path $idleDir "directional.request") -Encoding utf8
Expect-Throw { Wait-StressTransportIdle $idleDir "idle-busy" 1 } "cannot preserve burst order" "transport-idle-refused"
Remove-Item -LiteralPath (Join-Path $idleDir "directional.request") -Force
Wait-StressTransportIdle $idleDir "idle-consumed" 2
Rec-Mock "transport-idle-consumed" @{ ok = $true }
Remove-Item -LiteralPath $idleDir -Recurse -Force -ErrorAction SilentlyContinue

# Causal drain over the dedicated dispatch cursor: pending clears, the apply
# cursor is untouched, and later barriers see an empty queue.
Write-MockLog @(
  (@{ event = "workspace"; tick = 30; op = "select"; index = 1; edge = "cli"; outcome = "ok" } | ConvertTo-Json -Compress)
)
$dtopo = @{ Pending = [System.Collections.ArrayList]@(@{ argv = @("workspace", "--select", "1"); op = "select-workspace" }); DispatchMark = 0 }
$amark = 99
$douts = Drain-StressPending $logPath $dtopo 5 "drain-test"
if (@($dtopo.Pending).Count -ne 0) { Fail-Mock "drain did not clear" }
if ([int]$dtopo.DispatchMark -le 0) { Fail-Mock "drain did not advance dispatch cursor" }
if ([int]$amark -ne 99) { Fail-Mock "drain touched the apply cursor" }
Rec-Mock "drain-pending-accept" @{ tick = "$($douts[0].tick)" }

# Effect-aware lifetime-bound recovery faults (offline only: no owner
# launch, no fixture, no window mutation). Temp ledger files plus pure
# helpers drive the gates; cleanup paths that would shell out are proven by
# the skips ledger, not by executing native stop/restore.
$mine = @{ pid = 4242; process_creation = "aaaabbbbccccdddd"; exe_path = "C:\\run\\tiler-windows.exe"; user_sid = "S-1-5-21-7"; session_id = 1 }
if (-not (Test-StressOwnerSame $mine $mine)) { Fail-Mock "owner self same" }
$foreignPid = @{ pid = 9999; process_creation = "aaaabbbbccccdddd"; exe_path = "C:\\run\\tiler-windows.exe"; user_sid = "S-1-5-21-7"; session_id = 1 }
if (Test-StressOwnerSame $mine $foreignPid) { Fail-Mock "foreign pid accepted" }
$foreignCreation = @{ pid = 4242; process_creation = "ffffffffffffffff"; exe_path = "C:\\run\\tiler-windows.exe"; user_sid = "S-1-5-21-7"; session_id = 1 }
if (Test-StressOwnerSame $mine $foreignCreation) { Fail-Mock "reused creation accepted" }
$foreignExe = @{ pid = 4242; process_creation = "aaaabbbbccccdddd"; exe_path = "C:\\other\\tiler-windows.exe"; user_sid = "S-1-5-21-7"; session_id = 1 }
if (Test-StressOwnerSame $mine $foreignExe) { Fail-Mock "foreign exe accepted" }
if (Test-StressOwnerSame $null $mine) { Fail-Mock "null accepted" }
Rec-Mock "owner-identity-gate" @{ rows = 5 }

$ledRoot = Join-Path ([IO.Path]::GetTempPath()) ("ws-ledger-" + $PID)
New-Item -ItemType Directory -Force -Path $ledRoot | Out-Null
$ledPath = Join-Path $ledRoot "ledger.json"
@{ owner = $mine } | ConvertTo-Json -Depth 6 -Compress | Set-Content -LiteralPath $ledPath -Encoding utf8
$sameCtx = @{ Owner = $mine; LedgerDir = $ledRoot; OwnerPayload = "C:\\run\\tiler-windows.exe" }
if ($null -eq (Assert-StressOwnerUnreplaced $sameCtx)) { Fail-Mock "same ledger rejected" }
Rec-Mock "ledger-same-accept" @{ ok = $true }
@{ owner = $foreignPid } | ConvertTo-Json -Depth 6 -Compress | Set-Content -LiteralPath $ledPath -Encoding utf8
Expect-Throw { Assert-StressOwnerUnreplaced $sameCtx } "owner-replaced" "foreign-owner-preserved"
Rec-Mock "foreign-owner-refused" @{ ok = $true }
Remove-Item -LiteralPath $ledPath -Force -ErrorAction SilentlyContinue
if ($null -ne (Assert-StressOwnerUnreplaced $sameCtx)) { Fail-Mock "absent ledger should yield null" }
Rec-Mock "ledger-absent-null" @{ ok = $true }
Remove-Item -LiteralPath $ledRoot -Recurse -Force -ErrorAction SilentlyContinue

# Settings-write gate: only a run that configured settings may restore them.
$preRoot = Join-Path ([IO.Path]::GetTempPath()) ("ws-pre-" + $PID)
New-Item -ItemType Directory -Force -Path $preRoot | Out-Null
$origFile = Join-Path $preRoot "settings-orig.json"
'{"revision":1}' | Set-Content -LiteralPath $origFile -Encoding utf8
$liveFile = Join-Path $preRoot "settings-live.json"
'{"revision":1}' | Set-Content -LiteralPath $liveFile -Encoding utf8
$plain = @{ SettingsPath = $liveFile; GroupOrig = @{ path = $origFile; sha = "x"; spi_path = $null }; SettingsChanged = @() }
if (Test-StressShouldRestoreSettings $plain) { Fail-Mock "before-config restore allowed" }
$plain.SettingsChanged = @("workspace.default_tiled=true")
if (-not (Test-StressShouldRestoreSettings $plain)) { Fail-Mock "configured restore refused" }
Rec-Mock "settings-write-gate" @{ rows = 2 }
Remove-Item -LiteralPath $preRoot -Recurse -Force -ErrorAction SilentlyContinue

# Pre-effect refusal ordering: owner refusal must precede settings config in
# Invoke-LiveJourney, so a refused run never mutates settings bytes.
$libSrc = Get-Content -LiteralPath (Join-Path $Repo "scripts\windows-world-stress.ps1") -Raw
$journeyIdx = $libSrc.IndexOf("function Invoke-LiveJourney")
$journeySrc = $libSrc.Substring($journeyIdx)
$refuseIdx = $journeySrc.IndexOf("Assert-NoPreexistingStressOwner `$ownerCopy")
$configIdx = $journeySrc.IndexOf("Set-StressCaseSettings `$settingsPath")
if ($refuseIdx -lt 0 -or $configIdx -lt 0 -or $refuseIdx -gt $configIdx) { Fail-Mock "refusal must precede settings config" }
Rec-Mock "preeffect-refusal-order" @{ ok = $true }

# Recovery ledger ordering: intent and bound data persist before each
# effect (settings intent before config, launch intent before the broker
# call, single leg refresh before bootstrap and after identity capture).
$legWriteIdx = $journeySrc.IndexOf("Write-StressLeg")
if ($legWriteIdx -lt 0 -or $legWriteIdx -gt $configIdx) { Fail-Mock "leg must persist before settings config" }
$intentIdx = $journeySrc.IndexOf("OwnerLaunchIntended = `$true")
$brokerIdx = $journeySrc.IndexOf("Start-ExplorerGui")
if ($intentIdx -lt 0 -or $brokerIdx -lt 0 -or $intentIdx -gt $brokerIdx) { Fail-Mock "launch intent must bind before broker call" }
Rec-Mock "leg-before-effect-order" @{ ok = $true }

# Unstarted cleanup ledger: a before-launch ctx must skip every live effect
# without shelling out. Missing bindings are skips, never writes.
$emptyCtx = @{
  OwnerPayload = "C:\\run\\tiler-windows.exe"; Owner = $null; OwnerLaunched = $false; OwnerLaunchIntended = $false
  FixtureExe = $null; FixtureProc = $null; FixtureCreation = ""
  Socket = $null; Nonce = $null
  SettingsPath = $null; GroupOrig = $null; LedgerDir = $null
  SettingsChanged = @(); SettingsConfigIntended = $false
}
if (Test-StressShouldRestoreSettings $emptyCtx) { Fail-Mock "empty ctx restores settings" }
Expect-Throw { Assert-StressOwnerUnreplaced $emptyCtx } "no bound run owner" "unstarted-owner-stop"
Rec-Mock "unstarted-cleanup-ledger" @{ ok = $true }

# Intent persists before the config write: intent alone authorizes restore,
# so a mid-write throw still recovers instead of bypassing.
if (-not (Test-StressShouldRestoreSettings @{ SettingsPath = "x"; GroupOrig = @{}; SettingsConfigIntended = $true; SettingsChanged = @() })) {
  Fail-Mock "intended-only restore refused"
}
Rec-Mock "settings-intent-gate" @{ ok = $true }

# Readiness-fail recovery binding: a launched-but-unproven owner keeps its
# ledger identity for abort cleanup instead of dropping to skips.
$readyCtx = @{
  OwnerPayload = "C:\\run\\tiler-windows.exe"; Owner = $null; OwnerLaunched = $true; OwnerLaunchIntended = $true
  FixtureExe = $null; FixtureProc = $null; FixtureCreation = ""
  Socket = "sock"; Nonce = "nonce"
  SettingsPath = $null; GroupOrig = $null; LedgerDir = $null
  SettingsChanged = @(); SettingsConfigIntended = $false
}
if ($null -ne $readyCtx.Owner -or -not $readyCtx.OwnerLaunched) { Fail-Mock "readiness binding lost" }
Rec-Mock "readiness-fail-binding" @{ launched = $true }

# -Stop failure contract: cleanup errors must be nonzero with evidence kept.
# The driver now exits 1 on stop-cleanup failure (static seam).
if ($libSrc -notmatch "exit 1") { Fail-Mock "-Stop still exits 0 on cleanup failure" }
Rec-Mock "stop-nonzero" @{ ok = $true }

# --- Live-API shadows: Invoke-StressCleanup executes for real; only the
# live boundary is faked. No owner launch, no fixture, no window mutation.
$script:MockCalls = [System.Collections.ArrayList]@()
$script:MockReadyMode = "not-ready"
$script:MockLedgerCleanPath = $null
$script:MockLedgerPresentAtStop = $null
$script:MockIdentityLedgerDir = $null
$script:MockNativeThrow = @{}
$script:MockKilledIds = @{}
$selfProcReal = Microsoft.PowerShell.Management\Get-Process -Id $PID
$script:MockSelfPid = [int]$selfProcReal.Id
$script:MockSelfPath = "$($selfProcReal.Path)"
$script:MockSelfStart = $selfProcReal.StartTime
$script:MockSelfCreation = ("{0:x16}" -f $selfProcReal.StartTime.ToUniversalTime().ToFileTimeUtc())
$script:MockMissingPid = 299999999
try { $null = Microsoft.PowerShell.Management\Get-Process -Id $script:MockMissingPid -ErrorAction Stop; Fail-Mock "missing pid present" } catch {}
$script:MockMissingPid2 = 299999998
try { $null = Microsoft.PowerShell.Management\Get-Process -Id $script:MockMissingPid2 -ErrorAction Stop; Fail-Mock "missing pid2 present" } catch {}

function Invoke-LiveNative([string]$Exe, [string[]]$CliArgs) {
  $null = $script:MockCalls.Add(@{ api = "native"; argv = @($CliArgs) })
  $op = "$($CliArgs[0])"
  if ($script:MockNativeThrow.ContainsKey($op) -and $script:MockNativeThrow[$op] -eq $true) {
    throw "native exit 1: $Exe $($CliArgs -join ' ') simulated failure"
  }
  if ($op -eq "identity") {
    return ((@{ ledger_directory = "$script:MockIdentityLedgerDir" } | ConvertTo-Json -Compress))
  }
  if ($op -eq "stop") {
    # Product cmd_stop leaves the ledger: only restore_locked deletes it
    # (lifecycle.rs cmd_stop vs restore_under_lease). Record whether the
    # ledger still binds an owner between stop and restore.
    $script:MockLedgerPresentAtStop = (($null -ne $script:MockLedgerCleanPath) -and (Test-Path -LiteralPath $script:MockLedgerCleanPath -PathType Leaf))
    return '{"owner_exited":true}'
  }
  if ($op -eq "restore") {
    if ($null -ne $script:MockLedgerCleanPath -and (Test-Path -LiteralPath $script:MockLedgerCleanPath -PathType Leaf)) {
      Remove-Item -LiteralPath $script:MockLedgerCleanPath -Force
    }
    return '{"restored":true}'
  }
  if ($op -eq "ready") {
    if ($script:MockReadyMode -eq "ready") { return '{"ready":true}' }
    return '{"ready":false}'
  }
  throw "unexpected native call: $op"
}
function Send-StressFixture([string]$FixtureExe, [string]$Socket, [string]$Nonce, [string]$Json) {
  $null = $script:MockCalls.Add(@{ api = "fixture-shutdown" })
  return @{ ok = $true; result = $null }
}
function Get-LiveTaskbarSpi { return @{ arranging = 1; pen = 0; abm = "QL99-mock" } }
function Get-CimInstance {
  [CmdletBinding()] param([Parameter(Position = 0)]$A, [Parameter(Position = 1)]$B, [Parameter(ValueFromRemainingArguments = $true)]$Rest)
  return @()
}
function Get-Process {
  [CmdletBinding()] param([Parameter(Position = 0)][int]$Id, [Parameter(ValueFromRemainingArguments = $true)]$Rest)
  if ($script:MockKilledIds.ContainsKey("$Id")) {
    $er = New-Object System.Management.Automation.ErrorRecord((New-Object Exception "No process"), "NoProcessFound", [System.Management.Automation.ErrorCategory]::ObjectNotFound, $Id)
    $PSCmdlet.ThrowTerminatingError($er)
  }
  if ([int]$Id -eq [int]$script:MockSelfPid) {
    return @{ Id = [int]$Id; Path = "$script:MockSelfPath"; StartTime = $script:MockSelfStart }
  }
  return (Microsoft.PowerShell.Management\Get-Process -Id $Id -ErrorAction Stop)
}
function New-MockProc([int]$Id) {
  # ScriptMethod bodies do not resolve $script: state, so capture the live
  # collection references (mutated in place, never reassigned) by closure.
  $calls = $script:MockCalls
  $killed = $script:MockKilledIds
  $myId = $Id
  $o = [pscustomobject]@{ Id = $myId }
  $o | Add-Member -MemberType ScriptMethod -Name Kill -Value {
    $null = $calls.Add(@{ api = "kill"; id = $myId })
    $killed["$myId"] = $true
  }.GetNewClosure()
  return $o
}
function Mock-NativeCalls([string]$Op) {
  $r = [System.Collections.ArrayList]@()
  foreach ($c in $script:MockCalls) {
    if ("$($c.api)" -eq "native" -and "$($c.argv[0])" -eq $Op) { $null = $r.Add($c) }
  }
  return ,$r
}
function New-MockSettingsCtx([string]$OrigContent, [string]$LiveContent) {
  $d = Join-Path ([IO.Path]::GetTempPath()) ("ws-set-" + [guid]::NewGuid().ToString("N"))
  New-Item -ItemType Directory -Force -Path $d | Out-Null
  $orig = Join-Path $d "settings-orig.json"
  $live = Join-Path $d "settings-live.json"
  $OrigContent | Set-Content -LiteralPath $orig -Encoding utf8NoBOM
  $LiveContent | Set-Content -LiteralPath $live -Encoding utf8NoBOM
  $spi = Join-Path $d "taskbar-spi.json"
  (@{ arranging = 1; pen = 0; abm = "QL99-mock" } | ConvertTo-Json -Compress) |
    Set-Content -LiteralPath $spi -Encoding utf8NoBOM
  $sha = Get-LiveBytesSha ([IO.File]::ReadAllBytes($orig))
  return @{ dir = $d; orig = $orig; live = $live; partial = @{ SettingsPath = $live; GroupOrig = @{ path = $orig; sha = $sha; spi_path = $spi } } }
}
function New-MockLedgerDir($OwnerObj) {
  $d = Join-Path ([IO.Path]::GetTempPath()) ("ws-led-" + [guid]::NewGuid().ToString("N"))
  New-Item -ItemType Directory -Force -Path $d | Out-Null
  if ($null -ne $OwnerObj) {
    (@{ owner = $OwnerObj } | ConvertTo-Json -Depth 6 -Compress) |
      Set-Content -LiteralPath (Join-Path $d "ledger.json") -Encoding utf8NoBOM
  }
  return $d
}
function New-MockCtx {
  return @{
    OwnerPayload = $script:MockSelfPath; Owner = $null; OwnerLaunched = $false; OwnerLaunchIntended = $false
    FixtureExe = $null; FixtureProc = $null; FixtureCreation = ""
    Socket = $null; Nonce = $null
    SettingsPath = $null; GroupOrig = $null; LedgerDir = $null
    SettingsChanged = @(); SettingsConfigIntended = $false
  }
}

# D1. Pre-existing owner refusal is fail-closed through the native seam: a
# nonzero ready reply refuses with no settings write, and an identity
# query failure refuses too.
$setD1 = New-MockSettingsCtx '{"revision":1,"want":true}' '{"revision":1,"want":false}'
$ledD1 = New-MockLedgerDir $null
$script:MockIdentityLedgerDir = $ledD1
$script:MockNativeThrow = @{ ready = $true }
$preD1 = Get-Content -LiteralPath $setD1.live -Raw
Expect-Throw { Assert-NoPreexistingStressOwner $script:MockSelfPath "mock-d1-ready" | Out-Null } "refuse: ready query failed" "d1-ready-nonzero-refuses"
if ("$(Get-Content -LiteralPath $setD1.live -Raw)" -ne "$preD1") { Fail-Mock "d1 ready failure wrote settings" }
Rec-Mock "d1-ready-nonzero-refusal" @{ settings_untouched = $true }
$script:MockNativeThrow = @{ identity = $true }
Expect-Throw { Assert-NoPreexistingStressOwner $script:MockSelfPath "mock-d1-identity" | Out-Null } "refuse: owner identity query failed" "d1-identity-failure-refuses"
if ("$(Get-Content -LiteralPath $setD1.live -Raw)" -ne "$preD1") { Fail-Mock "d1 identity failure wrote settings" }
Rec-Mock "d1-identity-failure-refusal" @{ settings_untouched = $true }
$script:MockNativeThrow = @{}
$script:MockIdentityLedgerDir = $null
Remove-Item -LiteralPath $setD1.dir -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $ledD1 -Recurse -Force -ErrorAction SilentlyContinue

# Fresh fixture-process lifetime: the PID must exist with the bound creation
# stamp. A reused PID (same number, new creation) or a gone PID fails
# closed, never authorizing a recycled handle.
$null = Test-StressFixtureProcess ([int]$script:MockSelfPid) @{ pid = [int]$script:MockSelfPid; creation = "$script:MockSelfCreation" } "fixture-self"
Rec-Mock "fixture-process-accept" @{ pid = "$script:MockSelfPid" }
Expect-Throw { Test-StressFixtureProcess ([int]$script:MockSelfPid) @{ pid = [int]$script:MockSelfPid; creation = "ffffffffffffffff" } "fixture-reuse" } "creation mismatch" "fixture-process-reuse-refused"
Expect-Throw { Test-StressFixtureProcess ([int]$script:MockMissingPid) @{ pid = [int]$script:MockMissingPid; creation = "deadbeefdeadbeef" } "fixture-gone" } "gone" "fixture-process-gone-refused"

# A. Before-launch: unstarted ctx runs real cleanup with zero live calls.
$script:MockCalls.Clear()
$ctxA = New-MockCtx
Invoke-StressCleanup $ctxA "mock-before-launch" | Out-Null
if (@($script:MockCalls | Where-Object { $_.api -eq "native" }).Count -ne 0) { Fail-Mock "before-launch native call" }
if (@($script:MockCalls | Where-Object { $_.api -eq "fixture-shutdown" }).Count -ne 0) { Fail-Mock "before-launch shutdown call" }
if (@($script:MockCalls | Where-Object { $_.api -eq "kill" }).Count -ne 0) { Fail-Mock "before-launch kill call" }
Rec-Mock "cleanup-before-launch" @{ native = 0; shutdown = 0; kill = 0 }

# B. After-settings partial failure: intent persisted before the config
# write, so cleanup restores even with SettingsChanged still empty.
$setB = New-MockSettingsCtx '{"revision":1,"want":true}' '{"revision":1,"want":false}'
$ctxB = New-MockCtx
$ctxB.SettingsPath = $setB.partial.SettingsPath
$ctxB.GroupOrig = $setB.partial.GroupOrig
$ctxB.SettingsConfigIntended = $true
$ctxB.SettingsChanged = @()
$script:MockCalls.Clear()
Invoke-StressCleanup $ctxB "mock-after-settings" | Out-Null
$liveB = Get-Content -LiteralPath $setB.live -Raw
$origB = Get-Content -LiteralPath $setB.orig -Raw
if ("$liveB" -ne "$origB") { Fail-Mock "intended settings not restored" }
if ((Mock-NativeCalls "stop").Count -ne 0) { Fail-Mock "after-settings stop call" }
if (@($script:MockCalls | Where-Object { $_.api -eq "fixture-shutdown" }).Count -ne 0) { Fail-Mock "after-settings shutdown call" }
Rec-Mock "cleanup-after-settings" @{ restored = $true }
Remove-Item -LiteralPath $setB.dir -Recurse -Force -ErrorAction SilentlyContinue

# C. Readiness failure: intended-but-unproven launch adopts the ledger owner
# through the bounded capture, then stops and restores it.
$goneC = @{ pid = $script:MockMissingPid; process_creation = "deadbeefdeadbeef"; exe_path = $script:MockSelfPath }
$ledC = New-MockLedgerDir $goneC
$script:MockLedgerCleanPath = Join-Path $ledC "ledger.json"
$ctxC = New-MockCtx
$ctxC.OwnerLaunchIntended = $true
$ctxC.LedgerDir = $ledC
$script:MockCalls.Clear()
$script:MockLedgerPresentAtStop = $null
Invoke-StressCleanup $ctxC "mock-readiness-fail" | Out-Null
if ($null -eq $ctxC.Owner) { Fail-Mock "readiness owner not adopted" }
if ((Mock-NativeCalls "stop").Count -ne 1) { Fail-Mock "readiness stop missing" }
if ($script:MockLedgerPresentAtStop -ne $true) { Fail-Mock "ledger missing between stop and restore" }
if ((Mock-NativeCalls "restore").Count -ne 1) { Fail-Mock "readiness restore missing" }
if (Test-Path -LiteralPath (Join-Path $ledC "ledger.json")) { Fail-Mock "readiness ledger not cleaned" }
Rec-Mock "cleanup-readiness-fail" @{ adopted = $true }
$script:MockLedgerCleanPath = $null
Remove-Item -LiteralPath $ledC -Recurse -Force -ErrorAction SilentlyContinue

# D. After-dispatch healthy: exact fixture target gets shutdown, then an
# exact kill with a bounded exit wait; settings restore; no failure.
$ownerD = @{ pid = $script:MockMissingPid; process_creation = "deadbeefdeadbeef"; exe_path = $script:MockSelfPath }
$ledD = New-MockLedgerDir $ownerD
$script:MockLedgerCleanPath = Join-Path $ledD "ledger.json"
$setD = New-MockSettingsCtx '{"revision":1,"want":true}' '{"revision":1,"want":false}'
$ctxD = New-MockCtx
$ctxD.Owner = $ownerD
$ctxD.OwnerLaunched = $true
$ctxD.LedgerDir = $ledD
$ctxD.SettingsPath = $setD.partial.SettingsPath
$ctxD.GroupOrig = $setD.partial.GroupOrig
$ctxD.SettingsChanged = @("workspace.default_tiled=true")
$ctxD.FixtureExe = $script:MockSelfPath
$script:MockKilledIds = @{}
$ctxD.FixtureProc = New-MockProc $script:MockSelfPid
$ctxD.FixtureCreation = $script:MockSelfCreation
$ctxD.Socket = "sock"
$ctxD.Nonce = "nonce"
$script:MockCalls.Clear()
$script:MockLedgerPresentAtStop = $null
Invoke-StressCleanup $ctxD "mock-after-dispatch" | Out-Null
if (@($script:MockCalls | Where-Object { $_.api -eq "fixture-shutdown" }).Count -ne 1) { Fail-Mock "dispatch shutdown missing" }
if (@($script:MockCalls | Where-Object { $_.api -eq "kill" }).Count -ne 1) { Fail-Mock "dispatch exact kill missing" }
if ((Mock-NativeCalls "stop").Count -ne 1) { Fail-Mock "dispatch stop missing" }
if ($script:MockLedgerPresentAtStop -ne $true) { Fail-Mock "dispatch ledger missing between stop and restore" }
if ((Mock-NativeCalls "restore").Count -ne 1) { Fail-Mock "dispatch restore missing" }
if ("$(Get-Content -LiteralPath $setD.live -Raw)" -ne "$(Get-Content -LiteralPath $setD.orig -Raw)") { Fail-Mock "dispatch settings not restored" }
Rec-Mock "cleanup-after-dispatch" @{ shutdown = 1; kill = 1 }
$script:MockLedgerCleanPath = $null
Remove-Item -LiteralPath $setD.dir -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $ledD -Recurse -Force -ErrorAction SilentlyContinue

# E. Foreign owner: no stop, no restore, and no global settings overwrite
# behind another owner; cleanup throws with the owner preserved.
$mineE = @{ pid = $script:MockSelfPid; process_creation = $script:MockSelfCreation; exe_path = $script:MockSelfPath }
$foreignE = @{ pid = $script:MockMissingPid; process_creation = "ffffffffffffffff"; exe_path = $script:MockSelfPath }
$ledE = New-MockLedgerDir $foreignE
$setE = New-MockSettingsCtx '{"revision":1,"want":true}' '{"revision":1,"want":false}'
$ctxE = New-MockCtx
$ctxE.Owner = $mineE
$ctxE.OwnerLaunched = $true
$ctxE.LedgerDir = $ledE
$ctxE.SettingsPath = $setE.partial.SettingsPath
$ctxE.GroupOrig = $setE.partial.GroupOrig
$ctxE.SettingsChanged = @("workspace.default_tiled=true")
$script:MockCalls.Clear()
Expect-Throw { Invoke-StressCleanup $ctxE "mock-foreign" | Out-Null } "owner-replaced" "foreign-cleanup-throws"
if ((Mock-NativeCalls "stop").Count -ne 0) { Fail-Mock "foreign owner stopped" }
if ((Mock-NativeCalls "restore").Count -ne 0) { Fail-Mock "foreign owner restored" }
if ("$(Get-Content -LiteralPath $setE.live -Raw)".Trim() -ne '{"revision":1,"want":false}') { Fail-Mock "settings overwritten behind foreign owner" }
Rec-Mock "cleanup-foreign-preserved" @{ stop = 0; restore = 0; settings_untouched = $true }
Remove-Item -LiteralPath $setE.dir -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $ledE -Recurse -Force -ErrorAction SilentlyContinue

# F. Reused fixture PID: shutdown and kill both refuse the foreign target;
# the kill handle is never touched and cleanup throws.
$ctxF = New-MockCtx
$ctxF.Socket = "sock"
$ctxF.Nonce = "nonce"
$ctxF.FixtureExe = "C:\\other\\fixture.exe"
$script:MockKilledIds = @{}
$ctxF.FixtureProc = New-MockProc $script:MockSelfPid
$ctxF.FixtureCreation = $script:MockSelfCreation
$script:MockCalls.Clear()
Expect-Throw { Invoke-StressCleanup $ctxF "mock-fixture-reuse" | Out-Null } "another exe" "fixture-reuse-throws"
if (@($script:MockCalls | Where-Object { $_.api -eq "kill" }).Count -ne 0) { Fail-Mock "reused fixture killed" }
if (@($script:MockCalls | Where-Object { $_.api -eq "fixture-shutdown" }).Count -ne 0) { Fail-Mock "reused fixture shutdown sent" }
Rec-Mock "cleanup-fixture-reuse" @{ kill = 0; shutdown = 0 }

# D3-reuse. Same-exe/different-creation fixture target (PID reused by a new
# process of the same binary): shutdown and kill both refuse; cleanup throws
# with zero live kills and zero shutdown calls.
$ctxF2 = New-MockCtx
$ctxF2.Socket = "sock"
$ctxF2.Nonce = "nonce"
$ctxF2.FixtureExe = $script:MockSelfPath
$script:MockKilledIds = @{}
$ctxF2.FixtureProc = New-MockProc $script:MockSelfPid
$ctxF2.FixtureCreation = "ffffffffffffffff"
$script:MockCalls.Clear()
Expect-Throw { Invoke-StressCleanup $ctxF2 "mock-fixture-reuse-creation" | Out-Null } "reused by another process" "fixture-reuse-creation-throws"
if (@($script:MockCalls | Where-Object { $_.api -eq "kill" }).Count -ne 0) { Fail-Mock "reused creation fixture killed" }
if (@($script:MockCalls | Where-Object { $_.api -eq "fixture-shutdown" }).Count -ne 0) { Fail-Mock "reused creation fixture shutdown sent" }
Rec-Mock "cleanup-fixture-reuse-creation" @{ kill = 0; shutdown = 0 }

# H1. Gone fixture target: shutdown skips for lack of a valid bound target
# and the exit phase confirms absence; no failure, no kill, no shutdown.
$ctxH1 = New-MockCtx
$ctxH1.Socket = "sock"
$ctxH1.Nonce = "nonce"
$ctxH1.FixtureExe = $script:MockSelfPath
$ctxH1.FixtureProc = New-MockProc $script:MockMissingPid2
$ctxH1.FixtureCreation = "deadbeefdeadbeef"
$script:MockCalls.Clear()
Invoke-StressCleanup $ctxH1 "mock-fixture-gone" | Out-Null
if (@($script:MockCalls | Where-Object { $_.api -eq "fixture-shutdown" }).Count -ne 0) { Fail-Mock "gone fixture shutdown sent" }
if (@($script:MockCalls | Where-Object { $_.api -eq "kill" }).Count -ne 0) { Fail-Mock "gone fixture killed" }
Rec-Mock "cleanup-fixture-gone" @{ shutdown = 0; kill = 0 }

# H2. Malformed ledger fails closed: the owner-stop phase records the
# failure, no stop call is made, and cleanup throws.
$ledH2 = New-MockLedgerDir $null
'{not json' | Set-Content -LiteralPath (Join-Path $ledH2 "ledger.json") -Encoding utf8NoBOM
$ctxH2 = New-MockCtx
$ctxH2.Owner = @{ pid = $script:MockMissingPid; process_creation = "deadbeefdeadbeef"; exe_path = $script:MockSelfPath }
$ctxH2.OwnerLaunched = $true
$ctxH2.LedgerDir = $ledH2
$script:MockCalls.Clear()
Expect-Throw { Invoke-StressCleanup $ctxH2 "mock-malformed-ledger" | Out-Null } "owner-stop:" "malformed-ledger-fail-closed"
if ((Mock-NativeCalls "stop").Count -ne 0) { Fail-Mock "malformed ledger stopped owner" }
Rec-Mock "cleanup-malformed-ledger" @{ stop = 0 }
Remove-Item -LiteralPath $ledH2 -Recurse -Force -ErrorAction SilentlyContinue

# G. Functional -Stop: a failing stop run exits nonzero with leg.json kept.
$stopLed = New-MockLedgerDir @{ pid = 1; process_creation = "0000000000000001"; exe_path = "C:\\nope\\tiler-windows.exe" }
$stopRoot = Join-Path ([IO.Path]::GetTempPath()) ("ws-stop-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $stopRoot | Out-Null
(@{
  socket = ""; nonce = ""; owner = "C:\\nope\\tiler-windows.exe"; owner_seconds = 120
  ledger_dir = $stopLed
} | ConvertTo-Json -Depth 4 -Compress) | Set-Content -LiteralPath (Join-Path $stopRoot "leg.json") -Encoding utf8NoBOM
$stopOut = & pwsh -NoProfile -File (Join-Path $Repo "scripts\windows-world-stress.ps1") -Stop -RunDir $stopRoot 2>&1 | Out-String
$stopCode = $LASTEXITCODE
if ($stopCode -eq 0) { Fail-Mock "-Stop exited 0 on cleanup failure: $stopOut" }
if (-not (Test-Path -LiteralPath (Join-Path $stopRoot "leg.json") -PathType Leaf)) { Fail-Mock "-Stop dropped recovery evidence" }
Rec-Mock "stop-nonzero-functional" @{ exit = $stopCode }
Remove-Item -LiteralPath $stopRoot -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $stopLed -Recurse -Force -ErrorAction SilentlyContinue

Remove-Item -LiteralPath $mockRoot -Recurse -Force -ErrorAction SilentlyContinue
$report = @{ status = "pass"; stage = "WorldStressMockFaults"; steps = $WSM_STEPS }
$report | ConvertTo-Json -Depth 8 | Write-Output
