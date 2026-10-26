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
    "Resolve-StressAlias", "Get-StressCliArgs", "Convert-StressTiles",
    "Test-StressApplyClean", "Compare-StressCheckpoint",
    "Get-StressTitles", "Get-StressHwndOfCaller", "Get-StressDisplayBaseline",
    "Send-StressFixture", "Start-StressFixture", "Start-StressBootstrap",
    "Set-StressCaseSettings", "Assert-NoPreexistingStressOwner",
    "Wait-StressLog", "Wait-StressFreshApply", "Wait-StressDispatch",
    "Invoke-StressCli", "Wait-StressRequestFile",
    "Wait-StressHwnd", "Wait-StressAdmitted", "Wait-StressDeparted",
    "Invoke-StressFocus", "Get-StressGeometry", "Get-StressRing",
    "Initialize-StressTopology", "Refresh-StressRing",
    "Invoke-StressCheckpoint", "Wait-StressPager", "Invoke-StressCleanup",
    "Invoke-Mock", "Invoke-LiveJourney"
  )) {
  if ($src -notmatch ("function " + $fn)) { throw "missing function $fn" }
}
Write-Output "required functions present=33"

# Required seams are matched against comment-stripped code: real call-site
# literals, not prose. Freshness uses accepted/command-consumed (duplicate
# is never fresh); waits cover transport publication and dispatch joins,
# never per-action native convergence or foreground subject prechecks.
foreach ($must in @("scope-exe tiler-live-fixture.exe", '"workspace", "--select"',
    '"workspace", "--send"', '"workspace", "--stay"', '"workspace", "--previous"',
    '"workspace", "--relative"', '"directional", "--focus"', '"directional", "--move"',
    "--inner-gap 0", "--outer-gap 0",
    "tile --user-start", "member-admitted", "desired_windows_omitted",
    "clear_holds_omitted", "focus_dispatch_omitted", "inflight_events_omitted",
    "foreground_meta_omitted", "visibility_posts_omitted",
    "desired_geometry_omitted", "geometry_status", "desired_transfers",
    '"accepted"', '"command-consumed"', '"duplicate"',
    "pager-cells", "minis_total", "minis_omitted", "focused_total",
    "operation_settled_omitted", "single-pending", "pending",
    "CARGO_TARGET_DIR", "default_tiled", "group-with-neighbor",
    "per-output-local", "leg.json", "world-stress-report", "NoProcessFound",
    "native leg blocked",
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
    "Resolve-StressAlias", "Validate-StressCase", "Invoke-Mock")) {
  if ($null -eq (Get-Command $fn -ErrorAction SilentlyContinue)) {
    throw "dot-sourced command missing: $fn"
  }
}
Write-Output "dot-source commands present"
# Mechanical pre-effect Live refusal proof (offline): -Live must refuse
# before any cargo build, settings write, owner/fixture launch, or run-dir
# creation. The guard string above is asserted in code; here the refusal is
# executed in a child with no effects.
$liveOut = (& pwsh -NoProfile -File $Lib -Live 2>&1 | Out-String)
if ($LASTEXITCODE -eq 0 -or "$liveOut" -notmatch "refuse: world-stress native leg blocked") {
  throw "Live leg not blocked pre-effect: exit=$LASTEXITCODE out=$liveOut"
}
Write-Output "live refusal present"
Write-Output "WORLD-STRESS-CHECK-OK"
