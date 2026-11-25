# R14: fixture adoption, exact normal outer restore, and unchanged app refusal.
[CmdletBinding()]
param([Parameter(Mandatory)][switch]$Live)

if (-not $Live) { throw 'explicit -Live required' }
. (Join-Path $PSScriptRoot 'windows-phase1-proofs.ps1') -DefinitionsOnly
Initialize-P1Artifacts 'fullscreen-baseline'

$runDir = Join-Path $script:P1TargetDir "task9/fullscreen-baseline-$((Get-Date -Format 'yyyyMMdd-HHmmss-fff'))-$PID"
New-Item -ItemType Directory -Path $runDir -Force | Out-Null
$ctx = New-P1Context $script:P1FixtureCopy $script:P1SettingsPath
$proof = [ordered]@{
  commit = (& git -C $script:P1Repo rev-parse HEAD)
  owner_sha256 = (Get-FileHash -LiteralPath $script:P1OwnerCopy -Algorithm SHA256).Hash
  fixture_sha256 = (Get-FileHash -LiteralPath $script:P1FixtureCopy -Algorithm SHA256).Hash
}
$failure = $null
function Request-Fullscreen {
  $ack = Invoke-LiveNative $script:P1OwnerCopy @('workspace','--fullscreen') | ConvertFrom-Json
  if ($ack.dispatched -ne $true) { throw 'fullscreen request was not queued' }
}
try {
  $ctx.LedgerDir = Assert-P1CleanStart 'fullscreen-baseline'
  $ctx.GroupOrig = Save-LiveSettingsPreimage $script:P1SettingsPath (Join-Path $runDir 'settings-orig.json') (Join-Path $runDir 'taskbar-spi.json') ''
  $ctx.SettingsConfigIntended = $true
  $boot = Start-P1FixtureAndOwner $script:P1FixtureCopy $ctx 'fullscreen-baseline'
  $proof.owner = $boot.launch.bind
  $log = $boot.launch.log
  $proof.owner_trace = $log
  $display = Get-StressDisplayBaseline
  Spawn-P1Window $script:P1FixtureCopy $ctx 'subject' ($display.work[0] + 100) ($display.work[1] + 100) 700 460 $true
  $hwnd = Wait-StressHwnd 'subject' $boot.fixtureState 20
  $token = Wait-StressAdmitted $log @{} $hwnd $boot.fixtureState 30 'subject'
  $proof.adoption = (Wait-P1Event $log 0 {
    param($e)
    return ("$(Get-P1Prop $e 'event')" -eq 'adoption-baseline' -and
      "$(Get-P1Prop $e 'window')" -eq $token -and "$(Get-P1Prop $e 'outcome')" -eq 'captured')
  } 15 'adoption-time baseline').event
  if (Test-P1Prop $hwnd 'OmniTilerFullscreen') { throw 'adoption asserted project ownership' }
  $pre = Wait-P1SettledWorld $log 0 { param($e) Test-P1Tiled $e $token } 20 'initial tiled subject'
  $row = Get-P1Row $pre.event $token
  $frame = $row.effective_frame
  $null = Wait-P1NativeVis $hwnd $frame.x $frame.y $frame.w $frame.h 2 15 'initial native tile'
  $null = Invoke-StressFocus $script:P1FixtureCopy $ctx.Socket $ctx.Nonce 'subject' @{subject = $hwnd} $boot.fixtureState 10
  # Keep a decorated, non-maximized fixture window while testing the state
  # lane; an ordinary explicit float does not depend on tiled size hints
  # (which the retained fullscreen row deliberately cannot infer).
  $mark = Get-P1LogMark $log
  $ack = Invoke-LiveNative $script:P1OwnerCopy @('workspace','--float') | ConvertFrom-Json
  if ($ack.dispatched -ne $true) { throw 'float request was not queued' }
  $pre = Wait-P1SettledWorld $log $mark { param($e) Test-P1Floating $e $token } 20 'initial floating subject'
  $frame = (Get-P1Row $pre.event $token).effective_frame
  $null = Wait-P1NativeVis $hwnd $frame.x $frame.y $frame.w $frame.h 2 15 'initial native float'
  $before = Get-P1Snap $hwnd
  if (-not ($before.ok_outer -and $before.visible -and -not $before.iconic -and -not $before.maximized -and $before.caption -and $before.thick)) {
    throw 'normal decorated pre-fullscreen sample not proved'
  }
  $proof.before = $before
  $mark = Get-P1LogMark $log
  Request-Fullscreen
  $proof.owned_enter = (Wait-P1StateWrite $log $mark $token 'fullscreen' $true 20 'owned enter').attempt
  $null = Wait-P1Cond { Test-P1Prop $hwnd 'OmniTilerFullscreen' } 10 'owned fullscreen marker'
  $rows = (Wait-P1Rows $ctx.LedgerDir 'fullscreen-preimages.json' 1 10 'R12 entry').rows
  $proof.r12 = @($rows)[0]
  $saved = $proof.r12.frame
  if ($null -eq $saved -or "$(($before.outer) -join ',')" -ne "$($saved.x),$($saved.y),$($saved.x + $saved.w),$($saved.y + $saved.h)") {
    throw 'R12 write preimage differs from actual pre-effect outer geometry'
  }
  $mark = Get-P1LogMark $log
  Request-Fullscreen
  $proof.owned_exit = (Wait-P1StateWrite $log $mark $token 'fullscreen' $false 20 'owned exit').attempt
  $null = Wait-P1Rows $ctx.LedgerDir 'fullscreen-preimages.json' 0 10 'owned restore retirement'
  $proof.after = Get-P1Snap $hwnd
  if ("$($proof.after.outer -join ',')" -ne "$($before.outer -join ',')" -or -not $proof.after.ok_outer -or
      -not ($proof.after.caption -and $proof.after.thick -and -not $proof.after.maximized -and -not $proof.after.iconic) -or
      (Test-P1Prop $hwnd 'OmniTilerFullscreen')) {
    throw 'owned fullscreen exit did not settle exact outer geometry and normal chrome before retirement'
  }

  $app = @{v = 1; request_id = (New-StressReqId 'app-fullscreen'); op = 'set'; ids = @('subject');
    patch = @{mode = 'borderless_fullscreen'; decorations = $false}} | ConvertTo-Json -Depth 6 -Compress
  $reply = Send-StressFixture $script:P1FixtureCopy $ctx.Socket $ctx.Nonce $app
  if (-not ($reply.ok -and $reply.result.windows[0].ok)) { throw 'fixture app-owned fullscreen request failed' }
  $p = @($pre.event.desired_geometry | Where-Object { $_.primary })[0].full
  $null = Wait-P1NativeVis $hwnd $p.x $p.y $p.w $p.h 1 15 'app-owned monitor cover'
  $proof.app_before = Get-P1Snap $hwnd
  if (Test-P1Prop $hwnd 'OmniTilerFullscreen') { throw 'app-owned fullscreen acquired a project marker' }
  $mark = Get-P1LogMark $log
  Request-Fullscreen
  $proof.app_refusal = (Wait-P1Event $log $mark {
    param($e)
    return ("$(Get-P1Prop $e 'event')" -eq 'fullscreen-toggle' -and "$(Get-P1Prop $e 'edge')" -eq 'cli')
  } 15 'app-owned refusal').event
  if ($proof.app_refusal.cause -ne 'fullscreen-not-exitable' -or $proof.app_refusal.window -ne $token) {
    throw 'app-owned fullscreen did not produce the expected typed refusal for the adopted window'
  }
  $proof.app_after = Get-P1Snap $hwnd
  if ("$($proof.app_after.outer -join ',')" -ne "$($proof.app_before.outer -join ',')" -or
      $proof.app_after.style -ne $proof.app_before.style -or (Test-P1Prop $hwnd 'OmniTilerFullscreen') -or
      @((Read-P1PreimageRows $ctx.LedgerDir 'fullscreen-preimages.json').rows).Count -ne 0) {
    throw 'app refusal changed native state or R12 ownership'
  }
} catch { $failure = $_.Exception.Message }
finally {
  try { Invoke-StressCleanup $ctx 'fullscreen-baseline'; $proof.cleanup = 'ok' }
  catch { $proof.cleanup = $_.Exception.Message; if ($null -eq $failure) { $failure = $proof.cleanup } }
  $proof.error = $failure
  $proof.status = if ($null -eq $failure) { 'pass' } else { 'fail' }
  $evidence = Join-Path $runDir 'evidence.json'
  $proof | ConvertTo-Json -Depth 16 | Set-Content -LiteralPath $evidence -Encoding utf8
  Write-Output $evidence
  if ($null -ne $failure) { throw "fullscreen baseline proof failed: $failure" }
}
