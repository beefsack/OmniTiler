# Phase 2 item 9: fixture-only live proof for drag-preview ids and the
# focused-fullscreen presentation suspension (dragPreview.id and
# presentation.suspended_output).
#
# Safety boundary (same as the phase-1 proofs):
# - explicit -Live required; this script grants no standing authority.
# - fixture-only scope via `--scope-exe tiler-live-fixture.exe`.
# - NO synthetic physical input: no SendInput, no keybd_event, no
#   SetCursorPos, no posted move loop, no global hooks. The one native
#   drag in each id leg is an operator's own physical title-bar drag of a
#   fixture window. The script only reads; the operator moves the mouse.
# - the focused-fullscreen suspension leg is driven entirely by a second
#   fixture server copy named `tiler-live-fixture-fs.exe`: its basename is
#   outside the owner's scope, so the focused borderless fullscreen window
#   is an unmanaged foreground exactly like a foreign game. No unrelated
#   window is touched.
# - the owner runs through the normal Explorer-broker launch, so its
#   stderr/core trace is not captured; the proof joins the owner JSON log
#   (production rows enriched with preview_id/gesture_id by the companion
#   evidence-only change) and read-only `preview-inspect`.
# - every phase ends in the shared abort-safe cleanup: exact owner
#   stop+restore, primary fixture shutdown, second fixture exact-handle
#   shutdown, settings bytes/SPI verification. No broad process cleanup.
#
# What is proved live:
#   preview-id: every accepted mid-drag preview carries a positive core preview
#               id equal to the adapter's open gesture id; the id is stable
#               across a held drag, the renderer's shown ack joins the same
#               correlation, and a later drag after suspension gets a strictly
#               newer id (no stale id paints).
#   suspended-output: a focused, unmanaged borderless-fullscreen window makes
#               the owner log `suspend` (cause fullscreen-foreground), the
#               native preview carrier shows no visible rectangle for the whole
#               suspension, and `resume` restores the active lane. The adapter
#               gate's own reply-field case is the unit test
#               `suspended_output_hides_preview_without_painting`; the live
#               gate log row `drag-preview-suppress` is recorded if the core
#               ever emits a retained preview and suspension together.
#
# Usage (operator present; two physical drags required):
#   pwsh -NoProfile -File scripts/windows-phase2-drag-preview-proof.ps1 -Live
#   pwsh -NoProfile -File scripts/windows-phase2-drag-preview-proof.ps1 -Live -OwnerSeconds 480
#
# Preconditions: target/debug/{tiler-windows.exe,tiler-live-fixture.exe}
# built, no other live owner/ledger, live settings present. The operator
# remains responsible for the standing live-test authorization.

[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [switch]$Live,
  [int]$OwnerSeconds = 420,
  [int]$DragTimeoutSec = 75,
  [int]$SuspensionTimeoutSec = 30,
  [switch]$DefinitionsOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Capture caller arguments before dot-sourcing: both dependencies carry
# their own param blocks and rebind same-named variables.
$script:DpLive = [bool]$Live
$script:DpDefinitionsOnly = [bool]$DefinitionsOnly
$script:DpOwnerSeconds = $OwnerSeconds
$script:DpDragTimeout = $DragTimeoutSec
$script:DpSuspensionTimeout = $SuspensionTimeoutSec

. (Join-Path $PSScriptRoot "windows-phase1-proofs.ps1") -OwnerSeconds $OwnerSeconds -DefinitionsOnly

$Live = $script:DpLive
$DefinitionsOnly = $script:DpDefinitionsOnly
$OwnerSeconds = $script:DpOwnerSeconds
$DragTimeoutSec = $script:DpDragTimeout
$SuspensionTimeoutSec = $script:DpSuspensionTimeout

function Get-DpProp($Object, [string]$Name) {
  return Get-P1Prop $Object $Name
}

function Write-DpLog([string]$RunDir, [string]$Message) {
  $line = "$(Get-Date -Format 'HH:mm:ss.fff') $Message"
  Add-Content -LiteralPath (Join-Path $RunDir "driver.log") -Value $line
  Write-Output $line
}

function Get-DpPreviewInspect([string]$OwnerCopy) {
  return (Invoke-LiveNative $OwnerCopy @("preview-inspect") | ConvertFrom-Json)
}

function Get-DpVisiblePreviewCount($Inspect) {
  return @((Get-DpProp $Inspect "overlays") | Where-Object {
      (Get-DpProp $_ "visible") -eq $true
    }).Count
}

function Assert-DpPreviewHidden([string]$OwnerCopy, [string]$Tag) {
  $insp = Get-DpPreviewInspect $OwnerCopy
  $vis = Get-DpVisiblePreviewCount $insp
  if ($vis -ne 0) {
    throw "${Tag}: native preview carrier was visible ($vis overlay(s)) when the presentation must be suspended"
  }
  return $insp
}

function Get-DpPreviewPosts($Events, [string]$Token) {
  return @($Events | Where-Object {
      $event = "$(Get-DpProp $_ 'event')"
      $outcome = "$(Get-DpProp $_ 'outcome')"
      "$(Get-DpProp $_ 'window')" -eq "$Token" -and
      ($event -eq "drag-preview-post" -or
        ($event -eq "drag-preview" -and
          $outcome -in @("shown", "redrew", "moved", "unchanged") -and
          $null -ne (Get-DpProp $_ "preview_id")))
    })
}

function Test-DpPreviewPair($Post) {
  $previewId = [int64](Get-DpProp $Post "preview_id")
  $gestureId = [int64](Get-DpProp $Post "gesture_id")
  return ($previewId -gt 0 -and $gestureId -eq $previewId)
}

function Get-DpPreviewAcks($Events, [string]$Token) {
  return @($Events | Where-Object {
      "$(Get-DpProp $_ 'event')" -eq "drag-preview" -and
      "$(Get-DpProp $_ 'window')" -eq "$Token" -and
      "$(Get-DpProp $_ 'outcome')" -in @("shown", "redrew", "moved", "unchanged")
    })
}

function Join-DpPreviewEvidence($Posts, $Acks) {
  $byCorr = @{}
  foreach ($ack in $Acks) {
    $corr = "$(Get-DpProp $ack 'correlation')"
    if ($corr -eq "") { continue }
    if (-not $byCorr.ContainsKey($corr)) { $byCorr[$corr] = $ack }
  }
  $joined = [System.Collections.ArrayList]@()
  foreach ($post in $Posts) {
    if (-not (Test-DpPreviewPair $post)) { continue }
    $corr = "$(Get-DpProp $post 'correlation')"
    if ($corr -eq "" -or -not $byCorr.ContainsKey($corr)) { continue }
    $null = $joined.Add(@{ post = $post; ack = $byCorr[$corr] })
  }
  return $joined
}

function Wait-DpDragPreview([string]$OwnerCopy, [string]$LogPath, [int]$Mark, [string]$Token, [int]$Timeout, [string]$Tag) {
  # Waits for the operator's physical drag to produce at least two accepted
  # preview posts (same gesture id) and a renderer ack joined by the shared
  # correlation. The overlay is additionally sampled read-only; ack show
  # alone is accepted so a release right after the sample cannot race the
  # proof into a false failure.
  $deadline = (Get-Date).AddSeconds($Timeout)
  $lastPosts = @()
  $lastAcks = @()
  $sawVisible = $false
  $visibleRects = @()
  while ((Get-Date) -lt $deadline) {
    $events = @(Get-P1EventsSince $LogPath $Mark)
    $lastPosts = @(Get-DpPreviewPosts $events $Token)
    $valid = @($lastPosts | Where-Object { Test-DpPreviewPair $_ })
    $lastAcks = @(Get-DpPreviewAcks $events $Token)
    if ($valid.Count -ge 2) {
      $joined = @(Join-DpPreviewEvidence $valid $lastAcks)
      if ($joined.Count -ge 1) {
        try {
          $insp = Get-DpPreviewInspect $OwnerCopy
          $visRows = @((Get-DpProp $insp "overlays") | Where-Object {
              (Get-DpProp $_ "visible") -eq $true
            })
          if ($visRows.Count -gt 0) {
            $sawVisible = $true
            $visibleRects = @($visRows | ForEach-Object { @((Get-DpProp $_ "rect") | ForEach-Object { [int]$_ }) })
          }
        } catch {}
        $paintProven = $sawVisible -or @($joined | Where-Object {
            "$(Get-DpProp $_.ack 'outcome')" -in @("shown", "redrew", "moved")
          }).Count -ge 1
        if ($paintProven) {
          return @{
            posts = $valid
            acks = $lastAcks
            joined = $joined
            events = $events
            visible_seen = $sawVisible
            visible_rects = $visibleRects
          }
        }
      }
    }
    Start-Sleep -Milliseconds 120
  }
  $kinds = @(Get-P1EventsSince $LogPath $Mark | Where-Object {
      "$(Get-DpProp $_ 'event')" -eq "move-kind"
    })
  $kindText = ($kinds | ForEach-Object { "$(Get-DpProp $_ 'kind')" }) -join ","
  throw "${Tag}: no accepted drag-preview evidence within ${Timeout}s (posts=$($lastPosts.Count) acks=$($lastAcks.Count) move-kinds=[$kindText]). Start the drag on the decorated title bar of the fixture window and move while holding; body clicks and resizes do not preview."
}

function Wait-DpPreviewCleared([string]$OwnerCopy, [string]$LogPath, [int]$Mark, [string]$Tag) {
  $deadline = (Get-Date).AddSeconds(30)
  $lastVis = -1
  while ((Get-Date) -lt $deadline) {
    try {
      $lastVis = Get-DpVisiblePreviewCount (Get-DpPreviewInspect $OwnerCopy)
    } catch {
      $lastVis = -1
    }
    $hides = @(Get-P1EventsSince $LogPath $Mark | Where-Object {
        "$(Get-DpProp $_ 'event')" -eq "drag-preview" -and
        "$(Get-DpProp $_ 'outcome')" -eq "hidden"
      })
    if ($lastVis -eq 0 -and $hides.Count -ge 1) {
      return $hides[$hides.Count - 1]
    }
    Start-Sleep -Milliseconds 200
  }
  throw "${Tag}: the preview did not clear after release (visible=$lastVis); release the mouse button and wait for the gesture to settle"
}

function Stop-DpFixture([string]$Exe, $Ctx, $Proc) {
  if ($null -ne $Ctx -and "$(Get-DpProp $Ctx 'Socket')" -ne "" -and "$(Get-DpProp $Ctx 'Nonce')" -ne "") {
    try {
      $null = Send-StressFixture $Exe $Ctx.Socket $Ctx.Nonce (@{
          v = 1; request_id = "dp-fs-bye"; op = "shutdown"
        } | ConvertTo-Json -Compress)
    } catch {}
  }
  if ($null -ne $Proc) {
    try {
      if (-not $Proc.HasExited) {
        $null = $Proc.WaitForExit(5000)
        if (-not $Proc.HasExited) {
          $Proc.Kill()
          $null = $Proc.WaitForExit(10000)
        }
      }
    } catch {}
  }
}

function Get-DpFixtureState([string]$Exe, $Announcement) {
  $serverPid = [int](Get-DpProp $Announcement "pid")
  $p = Get-Process -Id $serverPid -ErrorAction Stop
  if ("$($p.Path)" -ine "$Exe") {
    throw "second fixture announcement pid $serverPid is not $Exe"
  }
  $creation = "{0:x16}" -f $p.StartTime.ToUniversalTime().ToFileTimeUtc()
  return @{ pid = $serverPid; creation = $creation }
}

function Test-DpSuspendedNow([string]$LogPath) {
  $suspended = $false
  foreach ($e in @(Get-P1EventsSince $LogPath 0)) {
    $name = "$(Get-DpProp $e 'event')"
    if ($name -eq "suspend") { $suspended = $true }
    elseif ($name -eq "resume") { $suspended = $false }
  }
  return $suspended
}

function Assert-DpMoveKindBinding($Events, [string]$Token, [int64]$PreviewId, [string]$Tag) {
  $moves = @($Events | Where-Object {
      "$(Get-DpProp $_ 'event')" -eq "move-kind" -and
      "$(Get-DpProp $_ 'window')" -eq "$Token" -and
      "$(Get-DpProp $_ 'kind')" -eq "move"
    })
  if ($moves.Count -eq 0) {
    throw "${Tag}: no move-kind=move START row for the dragged fixture window"
  }
  $startIds = @($moves | ForEach-Object { [int64](Get-DpProp $_ "gesture_id") } | Sort-Object -Unique)
  if ($startIds.Count -ne 1 -or $startIds[0] -ne $PreviewId) {
    throw "${Tag}: START gesture ids [$($startIds -join ',')] do not match the preview id $PreviewId"
  }
  return $moves
}

if ($DefinitionsOnly) { return }
if (-not $Live) { throw "explicit -Live required" }

$needed = (2 * $DragTimeoutSec) + (2 * 30) + $SuspensionTimeoutSec + 60
if ($OwnerSeconds -le $needed) {
  throw "owner duration ${OwnerSeconds}s cannot cover two drag waits, two release waits and suspension (need > ${needed}s); pass -OwnerSeconds $($needed + 60)"
}

Initialize-P1Artifacts "phase2-drag-preview"

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$runDir = Join-Path $script:P1TargetDir "task9\phase2-drag-preview-$stamp-$PID"
New-Item -ItemType Directory -Force -Path $runDir | Out-Null
$settingsPath = $script:P1SettingsPath
$origPath = Join-Path $runDir "settings-orig.json"
$spiPath = Join-Path $runDir "taskbar-spi.json"
$evidencePath = Join-Path $runDir "evidence.json"
$ctx = New-P1Context $script:P1FixtureCopy $settingsPath
$fsCtx = $null
$fsStart = $null
$fsProc = $null
$fsExe = Join-Path $script:P1BinDir "tiler-live-fixture-fs.exe"

$evidence = [ordered]@{
  proof = "phase2-drag-preview-id-and-suspended-output"
  started_utc = (Get-Date).ToUniversalTime().ToString("o")
  head = ((& git -C $script:P1Repo rev-parse HEAD) | Out-String).Trim()
  git_status = ((& git -C $script:P1Repo status --short) | Out-String).Trim()
  owner_sha256 = (Get-FileHash -LiteralPath $script:P1OwnerCopy -Algorithm SHA256).Hash
  fixture_sha256 = (Get-FileHash -LiteralPath $script:P1FixtureCopy -Algorithm SHA256).Hash
  fixture_fs_sha256 = $null
  safety = [ordered]@{
    fixture_only_scope = "--scope-exe tiler-live-fixture.exe"
    synthetic_input = $false
    global_hooks_over_foreign_input = $false
    manual_operator_drag = $true
    drags_required = 2
    owner_launch = "Explorer broker (normal tile --user-start), no redirected stderr"
    suspension_staging = "second copy of the fixture binary named tiler-live-fixture-fs.exe, outside the owner's scope basename; its focused borderless-fullscreen window is unmanaged foreground, not a foreign app"
    cleanup = "exact-owner stop+restore, primary fixture shutdown, second fixture shutdown by exact handle, settings/SPI verification"
  }
  preconditions = [ordered]@{
    owner_binary = $script:P1OwnerCopy
    fixture_binary = $script:P1FixtureCopy
    fixture_fs_binary = $fsExe
    settings_path = $settingsPath
    owner_seconds = $OwnerSeconds
    drag_timeout_sec = $DragTimeoutSec
    suspension_timeout_sec = $SuspensionTimeoutSec
    preexisting_owner_checked = $true
    evidence_source = "owner JSON log rows drag-preview-post (trace), move-kind, drag-preview acks, suspend/resume; read-only preview-inspect"
  }
  gate_note = "hide_for_suspended_output requires a core reply carrying both a retained preview and presentation.suspended_output naming that preview's output. The core clears the preview while an unmanageable fullscreen is focused (game pause/gesture fences), so the live proof asserts the owner's suspension and the absence of any preview paint, while the reply-field gate itself is covered by the unit test suspended_output_hides_preview_without_painting plus the live drag-preview-suppress row if that conjunction ever occurs."
  stages = [ordered]@{}
}
$failure = $null
$orig = $null

try {
  $ledgerDir = Assert-P1CleanStart "phase2-drag-preflight"
  $ctx.LedgerDir = $ledgerDir
  $orig = Save-LiveSettingsPreimage $settingsPath $origPath $spiPath ""
  $ctx.GroupOrig = $orig
  $ctx.SettingsConfigIntended = $true
  $evidence.stages.preflight = @{ ledger_dir = "$ledgerDir"; settings_sha_before = "$($orig.sha)" }

  Copy-Item -LiteralPath $script:P1FixtureCopy -Destination $fsExe -Force
  $evidence.fixture_fs_sha256 = (Get-FileHash -LiteralPath $fsExe -Algorithm SHA256).Hash

  $boot = Start-P1FixtureAndOwner $script:P1FixtureCopy $ctx "phase2-drag-preview"
  $logPath = $boot.launch.log
  $fixtureState = $boot.fixtureState
  $base = Get-StressDisplayBaseline
  $wx = [int]$base.work[0]
  $wy = [int]$base.work[1]

  Spawn-P1Window $script:P1FixtureCopy $ctx "drag-alpha" ($wx + 140) ($wy + 110) 760 520 $true
  Spawn-P1Window $script:P1FixtureCopy $ctx "drag-beta" ($wx + 460) ($wy + 240) 680 460 $true
  $hwndA = Wait-StressHwnd "drag-alpha" $fixtureState 30
  $hwndB = Wait-StressHwnd "drag-beta" $fixtureState 30
  $tokMap = @{}
  $tokA = Wait-StressAdmitted $logPath $tokMap $hwndA $fixtureState 60 "drag-alpha"
  $tokMap[$tokA] = "drag-alpha"
  $tokB = Wait-StressAdmitted $logPath $tokMap $hwndB $fixtureState 60 "drag-beta"
  $pre = Wait-P1SettledWorld $logPath 0 { param($e) (Test-P1Tiled $e $tokA) -and (Test-P1Tiled $e $tokB) } 60 "drag-pair-tiled"
  Assert-P1NoPending $pre.event "drag-pair-tiled"
  $null = Invoke-StressFocus $script:P1FixtureCopy $ctx.Socket $ctx.Nonce "drag-alpha" @{ "drag-alpha" = $hwndA } $fixtureState 15
  $alphaSnap = Get-P1Snap $hwndA
  if (-not ($alphaSnap.visible -and $alphaSnap.fg -and $alphaSnap.caption -and $alphaSnap.thick)) {
    throw "fixture alpha is not a focused decorated visible window; the operator drag target is unavailable"
  }

  # Second fixture server: same binary bytes under an out-of-scope basename.
  $fsStart = Start-StressFixture $fsExe
  $fsProc = $fsStart.proc
  $fsAnn = $fsStart.announcement
  $fsCtx = @{
    Socket = "$(Get-DpProp $fsAnn 'socket')"
    Nonce = "$(Get-DpProp $fsAnn 'nonce')"
  }
  $fsState = Get-DpFixtureState $fsExe $fsAnn
  Spawn-P1Window $fsExe $fsCtx "drag-fs" ($wx + 260) ($wy + 180) 700 480 $true
  $hwndFs = Wait-StressHwnd "drag-fs" $fsState 30
  $null = Invoke-StressFocus $fsExe $fsCtx.Socket $fsCtx.Nonce "drag-fs" @{ "drag-fs" = $hwndFs } $fsState 15
  # Back to alpha for the first operator drag.
  $null = Invoke-StressFocus $script:P1FixtureCopy $ctx.Socket $ctx.Nonce "drag-alpha" @{ "drag-alpha" = $hwndA } $fixtureState 15

  $evidence.stages.fixture = @{
    primary_pid = $boot.serverPid
    alpha_hwnd = $hwndA
    alpha_token = $tokA
    alpha_native = $alphaSnap
    beta_hwnd = $hwndB
    beta_token = $tokB
    alpha_frame = (Get-P1Row $pre.event $tokA).effective_frame
    beta_frame = (Get-P1Row $pre.event $tokB).effective_frame
    fs_pid = $fsState.pid
    fs_hwnd = $hwndFs
    fs_exe = $fsExe
  }

  # ---- Leg 1: open drag -> accepted preview id binding, live paint --------
  $mark1 = Get-P1LogMark $logPath
  Write-DpLog $runDir "DRAG 1 (operator): press and hold the decorated title bar of the fixture window 'OmniTilerTestWindow:drag-alpha' ($hwndA), then drag it over the sibling fixture tile 'drag-beta' and keep moving slowly for about 3 seconds. Do not release until this line says RELEASE 1."
  $leg1 = Wait-DpDragPreview $script:P1OwnerCopy $logPath $mark1 $tokA $DragTimeoutSec "drag1"
  $ids1 = @($leg1.posts | ForEach-Object { [int64](Get-DpProp $_ "preview_id") } | Sort-Object -Unique)
  $gids1 = @($leg1.posts | ForEach-Object { [int64](Get-DpProp $_ "gesture_id") } | Sort-Object -Unique)
  if ($ids1.Count -ne 1 -or $gids1.Count -ne 1 -or $ids1[0] -ne $gids1[0]) {
    throw "drag1: preview ids [$($ids1 -join ',')] and gesture ids [$($gids1 -join ',')] are not one positive matching id"
  }
  if (@($leg1.joined | Where-Object {
        [int64](Get-DpProp $_.post "preview_id") -ne $ids1[0]
      }).Count -ne 0) {
    throw "drag1: a joined renderer ack carried a different preview id than the open gesture"
  }
  $moves1 = Assert-DpMoveKindBinding $leg1.events $tokA $ids1[0] "drag1"
  Write-DpLog $runDir "RELEASE 1: release the mouse button now and wait; the preview should clear."
  $clear1 = Wait-DpPreviewCleared $script:P1OwnerCopy $logPath $mark1 "drag1-release"
  $evidence.stages.preview_id_leg1 = @{
    mark = $mark1
    preview_ids = $ids1
    gesture_ids = $gids1
    posts = @($leg1.posts | Select-Object -Last 50)
    joined_acks = @($leg1.joined | Select-Object -Last 50 | ForEach-Object {
        @{ correlation = "$(Get-DpProp $_.post 'correlation')"; outcome = "$(Get-DpProp $_.ack 'outcome')"; rect = (Get-DpProp $_.post "rect") }
      })
    visible_seen = $leg1.visible_seen
    visible_rects = $leg1.visible_rects
    cleared = $clear1
    move_kind_start_rows = @($moves1)
  }

  # ---- Suspension: focused unmanaged fullscreen, no preview paint --------
  Assert-DpPreviewHidden $script:P1OwnerCopy "suspension-pre" | Out-Null
  if (Test-DpSuspendedNow $logPath) {
    $resumeMark = Get-P1LogMark $logPath
    $null = Wait-P1Event $logPath $resumeMark {
      param($e)
      "$(Get-DpProp $e 'event')" -eq "resume"
    } 15 "pre-suspension-active"
  }
  $markS = Get-P1LogMark $logPath
  $fsReply = Send-StressFixture $fsExe $fsCtx.Socket $fsCtx.Nonce (@{
      v = 1; request_id = (New-StressReqId "dp-fs-fullscreen"); op = "set"; ids = @("drag-fs")
      patch = @{ mode = "borderless_fullscreen"; decorations = $false }
    } | ConvertTo-Json -Depth 6 -Compress)
  if ((Get-DpProp $fsReply "ok") -ne $true) {
    throw "second fixture refused borderless fullscreen: $($fsReply | ConvertTo-Json -Compress -Depth 6)"
  }
  $null = Invoke-StressFocus $fsExe $fsCtx.Socket $fsCtx.Nonce "drag-fs" @{ "drag-fs" = $hwndFs } $fsState $SuspensionTimeoutSec
  $fsSnap = Get-P1Snap $hwndFs
  $fw = [int]$fsSnap.outer[2] - [int]$fsSnap.outer[0]
  $fh = [int]$fsSnap.outer[3] - [int]$fsSnap.outer[1]
  if (-not ($fsSnap.visible -and $fsSnap.fg)) {
    throw "focused-fullscreen staging lost visibility/foreground (visible=$($fsSnap.visible) fg=$($fsSnap.fg))"
  }
  if ($fw -lt ([int]$base.full[2] - 2) -or $fh -lt ([int]$base.full[3] - 2)) {
    throw "second fixture fullscreen window does not cover its monitor (${fw}x${fh} vs $($base.full[2])x$($base.full[3]))"
  }
  $suspend = Wait-P1Event $logPath $markS {
    param($e)
    "$(Get-P1Prop $e 'event')" -eq "suspend" -and
    "$(Get-P1Prop $e 'cause')" -eq "fullscreen-foreground"
  } $SuspensionTimeoutSec "suspended-output-entry"
  $null = Wait-P1Cond {
    try { (Get-DpVisiblePreviewCount (Get-DpPreviewInspect $script:P1OwnerCopy)) -eq 0 } catch { $false }
  } 10 "suspended-output-hidden"
  Start-Sleep -Milliseconds 350
  $suspendedInspect = Assert-DpPreviewHidden $script:P1OwnerCopy "suspended-output"
  $suspEvents = @(Get-P1EventsSince $logPath ([int]$suspend.index))
  $paintedDuring = @($suspEvents | Where-Object {
      "$(Get-DpProp $_ 'event')" -eq "drag-preview" -and
      "$(Get-DpProp $_ 'outcome')" -in @("shown", "redrew", "moved", "unchanged")
    })
  if ($paintedDuring.Count -ne 0) {
    throw "a drag preview repainted during the focused-fullscreen suspension ($($paintedDuring.Count) rows)"
  }

  # End the foreign-fullscreen condition by stopping the second fixture, then
  # put the operator's primary fixture window back in the foreground.
  $markR = Get-P1LogMark $logPath
  Stop-DpFixture $fsExe $fsCtx $fsProc
  $null = Wait-P1Cond { $null -eq (Get-Process -Id $fsState.pid -ErrorAction SilentlyContinue) } 10 "fs-exit"
  $null = Invoke-StressFocus $script:P1FixtureCopy $ctx.Socket $ctx.Nonce "drag-alpha" @{ "drag-alpha" = $hwndA } $fixtureState 15
  $resume = Wait-P1Event $logPath $markR {
    param($e)
    "$(Get-P1Prop $e 'event')" -eq "resume"
  } $SuspensionTimeoutSec "suspended-output-exit"
  $afterResumeInspect = Get-DpPreviewInspect $script:P1OwnerCopy
  if ((Get-DpVisiblePreviewCount $afterResumeInspect) -ne 0) {
    throw "preview overlay still visible after resume (stale paint)"
  }
  $suppressRows = @(Get-P1EventsSince $logPath $markS | Where-Object {
      "$(Get-DpProp $_ 'event')" -eq "drag-preview-suppress"
    })
  $evidence.stages.suspended_output = @{
    mark = $markS
    foreground_fullscreen = $fsSnap
    monitor_full = $base.full
    suspend = $suspend.event
    resume = $resume.event
    suspended_inspect = $suspendedInspect
    after_resume_inspect = $afterResumeInspect
    preview_paints_during_suspension = $paintedDuring.Count
    drag_preview_suppress_rows = $suppressRows
    field_gate_unit_test = "crates/tiler-windows/src/tiling_sys/drag_preview.rs::preview_binding_tests::suspended_output_hides_preview_without_painting"
  }

  # ---- Leg 2: fresh drag after resume -> strictly newer id --------------
  $mark2 = Get-P1LogMark $logPath
  Write-DpLog $runDir "DRAG 2 (operator): press and hold the decorated title bar of 'OmniTilerTestWindow:drag-alpha' again, drag it over the sibling fixture tile, and move slowly for about 3 seconds. This proves a fresh id after suspension."
  $leg2 = Wait-DpDragPreview $script:P1OwnerCopy $logPath $mark2 $tokA $DragTimeoutSec "drag2"
  $ids2 = @($leg2.posts | ForEach-Object { [int64](Get-DpProp $_ "preview_id") } | Sort-Object -Unique)
  $gids2 = @($leg2.posts | ForEach-Object { [int64](Get-DpProp $_ "gesture_id") } | Sort-Object -Unique)
  if ($ids2.Count -ne 1 -or $gids2.Count -ne 1 -or $ids2[0] -ne $gids2[0]) {
    throw "drag2: preview ids [$($ids2 -join ',')] and gesture ids [$($gids2 -join ',')] are not one positive matching id"
  }
  if ($ids2[0] -le $ids1[0]) {
    throw "drag2: preview id $($ids2[0]) is not newer than the pre-suspension id $($ids1[0]); a stale preview id could repaint"
  }
  if (@($leg2.joined | Where-Object {
        [int64](Get-DpProp $_.post "preview_id") -ne $ids2[0]
      }).Count -ne 0) {
    throw "drag2: a joined renderer ack carried an id other than the new gesture's"
  }
  $moves2 = Assert-DpMoveKindBinding $leg2.events $tokA $ids2[0] "drag2"
  Write-DpLog $runDir "RELEASE 2: release the mouse button now."
  $clear2 = Wait-DpPreviewCleared $script:P1OwnerCopy $logPath $mark2 "drag2-release"
  $leg2SuspendedOut = @($leg2.posts | Where-Object {
      $null -ne (Get-DpProp $_ "suspended_out")
    })
  $evidence.stages.preview_id_leg2 = @{
    mark = $mark2
    preview_ids = $ids2
    gesture_ids = $gids2
    previous_preview_id = $ids1[0]
    strictly_newer = ($ids2[0] -gt $ids1[0])
    posts = @($leg2.posts | Select-Object -Last 50)
    joined_acks = @($leg2.joined | Select-Object -Last 50 | ForEach-Object {
        @{ correlation = "$(Get-DpProp $_.post 'correlation')"; outcome = "$(Get-DpProp $_.ack 'outcome')"; rect = (Get-DpProp $_.post "rect") }
      })
    visible_seen = $leg2.visible_seen
    visible_rects = $leg2.visible_rects
    cleared = $clear2
    posts_with_suspended_out = $leg2SuspendedOut.Count
    move_kind_start_rows = @($moves2)
  }
} catch {
  $failure = $_.Exception.Message
} finally {
  try { Stop-DpFixture $fsExe $fsCtx $fsProc } catch {}
  try {
    Invoke-StressCleanup $ctx "phase2-drag-preview"
    $evidence.cleanup = "ok"
  } catch {
    $evidence.cleanup = $_.Exception.Message
    if ($null -eq $failure) { $failure = $evidence.cleanup }
  }
  $evidence.error = $failure
  $evidence.status = if ($null -eq $failure) { "pass" } else { "fail" }
  $evidence.finished_utc = (Get-Date).ToUniversalTime().ToString("o")
  $evidence | ConvertTo-Json -Depth 18 | Set-Content -LiteralPath $evidencePath -Encoding utf8
  Write-Output $evidencePath
  if ($null -ne $failure) { throw "phase2 drag-preview proof failed: $failure" }
}
