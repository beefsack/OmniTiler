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
      Desired = $desired; DesiredView = 'ws-home'
      Frames = @{
        'win-a' = @{ outer = @(0, 0, 400, 560); visible = @(0, 0, 400, 560); flag = $true; alive = $true; ok_outer = $true; ok_visible = $true }
        'win-b' = @{ outer = @(400, 0, 800, 560); visible = @(400, 0, 800, 560); flag = $true; alive = $true; ok_outer = $true; ok_visible = $true }
      }
      ForegroundHwnd = [uint64]111; ManagedHwnds = @{ 'win-a' = [uint64]111; 'win-b' = [uint64]222 }
      Pager = @{ output = 'o1'; revision = 9; active = 0; cells_total = 2; cells_omitted = 0; cells = @(
        @{ ord = 1; ws = 'ws-home'; cur = $true; focused = $true; minis = @($true, $false); minis_total = 2; minis_omitted = 0; focused_total = 1 },
        @{ ord = 2; ws = 'ws-away'; cur = $false; focused = $false; minis = @(); minis_total = 0; minis_omitted = 0; focused_total = 0 }
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
$bad.Actual.Pager.revision = 8
Expect-Throw { Compare-StressCheckpoint $bad.Expected $bad.Actual } "revision" "stale-pager"

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

$corrupt = New-CleanApply 15 9 "accepted"
$corrupt.desired_transfers = @(@{ op = "op-1"; kind = "window" })
Write-MockLog @(($corrupt | ConvertTo-Json -Depth 8 -Compress))
$m = 0
Expect-Throw { Wait-StressFreshApply $logPath ([ref]$m) 15 2 "transfers" } "pending transfers" "pending-transfer"

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
    outputs = @(@{ output = "o1"; cells = @(
      @{ ord = 1; ws = "ws-home"; cur = $true; focused = $true; minis = @($true); minis_total = 1; minis_omitted = 0; focused_total = 1 },
      @{ ord = 2; ws = "ws-away"; cur = $false; focused = $false; minis = @(); minis_total = 0; minis_omitted = 0; focused_total = 0 }
    ); cells_total = 2; cells_omitted = 0; active = 0 })
    outputs_total = 1; outputs_omitted = 0
  }
}
Write-MockLog @(((New-PagerLine 9) | ConvertTo-Json -Depth 8 -Compress))
$mark = 0
$pg = Wait-StressPager $logPath ([ref]$mark) 9 "o1" 5 "pager"
if ($pg.entry.cells.Count -ne 2) { Fail-Mock "pager cells" }
if ("$($pg.entry.cells[0].ws)" -ne "ws-home") { Fail-Mock "pager ws join" }
Rec-Mock "pager-join-accept" @{ cells = 2 }

$line = New-PagerLine 9
$line.outputs[0].output = "o2"
Write-MockLog @(($line | ConvertTo-Json -Depth 8 -Compress))
$mark = 0
Expect-Throw { Wait-StressPager $logPath ([ref]$mark) 9 "o1" 2 "missing" } "timeout" "pager-output-missing"

$line = New-PagerLine 9
$line.outputs += @(@{ output = "o1"; cells = @(
  @{ ord = 1; ws = "ws-home"; cur = $true; focused = $false; minis = @(); minis_total = 0; minis_omitted = 0; focused_total = 0 }
); cells_total = 1; cells_omitted = 0; active = 0 })
Write-MockLog @(($line | ConvertTo-Json -Depth 8 -Compress))
$mark = 0
Expect-Throw { Wait-StressPager $logPath ([ref]$mark) 9 "o1" 2 "ambiguous" } "ambiguous" "pager-ambiguous"

Remove-Item -LiteralPath $mockRoot -Recurse -Force -ErrorAction SilentlyContinue
$report = @{ status = "pass"; stage = "WorldStressMockFaults"; steps = $WSM_STEPS }
$report | ConvertTo-Json -Depth 8 | Write-Output
