$ErrorActionPreference = "Stop"
# Offline-only seam check for scripts/windows-world-stress.ps1.
# No owner launch, no window mutation, no hooks, no SendInput.
#   pwsh -NoProfile -File scripts/windows-world-stress-check.ps1
$Repo = Split-Path -Parent $PSScriptRoot
$Lib = Join-Path $Repo "scripts\windows-world-stress.ps1"
$src = Get-Content -LiteralPath $Lib -Raw

$errs = $null
$tokens = [System.Management.Automation.PSParser]::Tokenize($src, [ref]$errs)
if ($errs.Count -ne 0) {
  foreach ($e in $errs) { Write-Output ("parse error: $($e.Message)") }
  throw "windows-world-stress has $($errs.Count) parse errors"
}
Write-Output "parse errors=0"

# Strip comment spans so seam words discussed in comments do not count;
# Add-Type C# bodies (string literals) stay in scope for the scan.
$lines = $src -split "`n"
foreach ($t in ($tokens | Where-Object { $_.Type -eq "Comment" })) {
  for ($ln = $t.StartLine; $ln -le $t.EndLine; $ln++) {
    $idx = $ln - 1
    if ($idx -lt 0 -or $idx -ge $lines.Count) { continue }
    if ($t.StartLine -eq $t.EndLine) {
      $a = [Math]::Max(0, $t.StartColumn - 1)
      $b = [Math]::Min($lines[$idx].Length, $t.EndColumn - 1)
      $len = [Math]::Max(0, $b - $a)
      $lines[$idx] = $lines[$idx].Substring(0, $a) + (" " * $len) +
        $lines[$idx].Substring($a + $len)
    } else {
      $lines[$idx] = ""
    }
  }
}
$code = $lines -join "`n"

foreach ($fn in @(
    "Get-StressCase", "Get-StressTargetDir", "Validate-StressCase",
    "Resolve-StressAlias", "Get-StressCliArgs", "Get-StressDispatchKey", "Convert-StressTiles",
    "Test-StressHasField", "Get-StressField", "Get-StressRowField", "Test-StressOmissionZero",
    "Test-StressApplyClean", "Test-StressApplyPending", "Compare-StressCheckpoint",
    "Get-StressDpiScale", "Get-StressExpectedMini", "Test-StressTransientMismatch",
    "Get-StressProcCreationHex", "Test-StressFixtureProcess", "Test-StressHwndOwner",
    "Drain-StressPending", "Wait-StressTransportIdle",
    "Get-StressTitles", "Get-StressHwndOfCaller", "Get-StressDisplayBaseline",
    "Send-StressFixture", "Start-StressFixture", "Start-StressBootstrap",
    "Set-StressCaseSettings", "Assert-NoPreexistingStressOwner",
    "Read-StressOwnerLog",
    "Test-StressOwnerSame", "Read-StressLedgerOwner", "Wait-StressLedgerOwner",
    "Write-StressLeg",
    "Assert-StressOwnerUnreplaced", "Test-StressShouldRestoreSettings",
    "Wait-StressLog", "Wait-StressFreshApply", "Wait-StressDispatch", "Wait-StressDispatchOrdered",
    "Invoke-StressCli", "Wait-StressRequestFile",
    "Wait-StressHwnd", "Wait-StressAdmitted", "Wait-StressDeparted",
    "Invoke-StressFocus", "Get-StressGeometry", "Get-StressRing",
    "Initialize-StressTopology", "Refresh-StressRing",
    "Invoke-StressCheckpoint", "Wait-StressReady", "Wait-StressPager", "Invoke-StressCleanup",
    "Test-StressStalePager",
    "Invoke-Mock", "Invoke-LiveJourney", "Get-StressPubSummary"
  )) {
  if ($src -notmatch ("function " + $fn)) { throw "missing function $fn" }
}
Write-Output "required functions present=58"

# Required seams are matched against comment-stripped code: real call-site
# literals, not prose. Freshness uses accepted/command-consumed (duplicate
# is never fresh); transport publication is unpaced per command (burst
# queue + ordered checkpoint join), settlement skips transient pending
# until the deadline, and the pager join retains the latest actually
# emitted change-gated payload with geometry/identity.
foreach ($must in @("scope-exe tiler-live-fixture.exe", '"workspace", "--select"',
    '"workspace", "--send"', '"workspace", "--stay"', '"workspace", "--previous"',
    '"workspace", "--relative"', '"directional", "--focus"', '"directional", "--move"',
    "--inner-gap 0", "--outer-gap 0",
    "tile --user-start", "member-admitted", "desired_windows_omitted",
    "clear_holds_omitted", "focus_dispatch_omitted", "inflight_events_omitted",
    "foreground_meta_omitted", "visibility_posts_omitted",
    "desired_geometry_omitted", "geometry_status", "desired_transfers",
    '"accepted"', '"command-consumed"', '"duplicate"', '"dispatched"',
    "pager-cells", "minis_total", "minis_omitted", "focused_total",
    "mini_rects", "mini_frames", "mini_windows", "command-queued",
    "unknown pager raw frame", "pager hidden", "hide epoch drift",
    "hidden outer", "HideEpoch", "pager raw",
    "raw revision", "older than checkpoint",
    "ReuseSettled", "no committed settled row", "committed row",
    "newer pending row not settled",
    "Stopwatch", "start_ms", "accepted_ms", "burst_intervals_ms",
    "publication count=", "added_delay_ms", "delay_each_ms",
    "transport cannot preserve burst order", "DispatchMark",
    "mini token multiset", "miniature layout", "DispatchMark", "not settled",
    "creation mismatch", "fixture process",
    "operation_settled_omitted", "single-pending", "pending",
    "home slot moved", "settlement timeout",
    "CARGO_TARGET_DIR", "default_tiled", "group-with-neighbor",
    "per-output-local", "leg.json", "world-stress-report", "NoProcessFound",
    "owner-replaced", "will not stop another owner", "OwnerLaunched",
    "OwnerLaunchIntended", "SettingsChanged", "SettingsConfigIntended",
    "settings_changed", "settings_intended", "owner_launched",
    "owner_launch_intended", "already gone", "exit 1",
    "owner identity query failed", "ready query failed",
    "DelayMs must be 0..60000", "OwnerSeconds must be 60..600",
    ".process_creation", "GetForegroundWindow", "GetWindowThreadProcessId",
    "OmniTilerTestWindow:") ) {
  if ($code -notmatch [regex]::Escape($must)) { throw "missing required seam $must" }
}
Write-Output "required seams present"

foreach ($bad in @("SendInput", "mouse_event", "keybd_event",
    "SetCursorPos", "SetForegroundWindow", "AttachThreadInput",
    "Stop-Process", "Taskkill", "Set-ItemProperty", "New-ItemProperty",
    "C:/Users", "C:\\Users",
    "keybd_event", "mouse_click", "Get-Process.*-Name", '"fresh"') ) {
  if ($code -match $bad) { throw "forbidden pattern present: $bad" }
}
Write-Output "forbidden patterns absent"

. $Lib
foreach ($fn in @("Get-StressCliArgs", "Convert-StressTiles",
    "Test-StressApplyClean", "Compare-StressCheckpoint",
    "Resolve-StressAlias", "Validate-StressCase",
    "Test-StressOwnerSame", "Read-StressLedgerOwner", "Wait-StressLedgerOwner",
    "Write-StressLeg",
    "Assert-StressOwnerUnreplaced", "Test-StressShouldRestoreSettings",
    "Invoke-Mock", "Get-StressPubSummary", "Wait-StressReady", "Test-StressStalePager")) {
  if ($null -eq (Get-Command $fn -ErrorAction SilentlyContinue)) {
    throw "dot-sourced command missing: $fn"
  }
}
Write-Output "dot-source commands present"
# Preserve strict evidence gates while the pager/world proof gap is unresolved.
$badOut = (& pwsh -NoProfile -File $Lib -Live 2>&1 | Out-String)
if ($LASTEXITCODE -eq 0 -or "$badOut" -notmatch "refuse: world-stress native leg parked") {
  throw "Live pre-effect refusal missing: exit=$LASTEXITCODE out=$badOut"
}
Write-Output "live pre-effect refusal present"
$dGate = $code.IndexOf("DelayMs must be 0..60000")
$dBuild = $code.IndexOf("cargo build")
if ($dGate -lt 0 -or $dBuild -lt 0 -or $dGate -gt $dBuild) { throw "range gate not pre-effect" }
Write-Output "range gate pre-effect"
Write-Output "WORLD-STRESS-CHECK-OK"
