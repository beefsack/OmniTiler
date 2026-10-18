$ErrorActionPreference = "Stop"
# Offline-only seam check for scripts/windows-live-lib.ps1.
# No owner launch, no window mutation, no hooks, no SendInput.
#   pwsh -NoProfile -File scripts/windows-live-lib-check.ps1
$Repo = Split-Path -Parent $PSScriptRoot
$Lib = Join-Path $Repo "scripts\windows-live-lib.ps1"
$src = Get-Content -LiteralPath $Lib -Raw

$errs = $null
$tokens = [System.Management.Automation.PSParser]::Tokenize($src, [ref]$errs)
if ($errs.Count -ne 0) {
  foreach ($e in $errs) { Write-Output ("parse error: $($e.Message)") }
  throw "windows-live-lib has $($errs.Count) parse errors"
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
    "Invoke-LiveNative", "Get-LiveBytesSha", "Write-LiveStep",
    "Read-LivePaired", "Get-LiveTaskbarSpi", "Get-LiveHelperInspect",
    "Assert-LiveHelperIdentity", "Assert-LiveOsCreation", "Invoke-LiveJustStop",
    "Test-LiveClientExposure", "Confirm-LiveForeground", "Read-LiveOwnerLog",
    "Wait-LiveObservedAdmission", "Assert-LiveTokenMap",
    "Save-LiveSettingsPreimage", "Close-LiveHelperExact",
    "Restore-LiveLegSafety", "Invoke-LiveFinalCleanup",
    "Assert-LiveSpiFields"
  )) {
  if ($src -notmatch ("function " + $fn)) { throw "missing function $fn" }
}
Write-Output "required functions present=19"

# Required seams are matched against comment-stripped code: real call-site
# literals, not prose. HTCLIENT proof is the SendMessageTimeoutW call with
# message 132 plus the -ne 1 hittest gate in code.
foreach ($must in @("0x0082", "0x201E", "mise exec -- just stop",
    "process_creation", "SHAppBarMessage", "WindowFromPoint",
    "AttachThreadInput", "SendMessageTimeoutW", ", 132,", "-ne 1",
    "NoProcessFound", "Push-Location", "Pop-Location",
    "WriteAllBytes", "just_stop_exit", "-shl 16", "step-log",
    "drifted during capture", "Mandatory")) {
  if ($code -notmatch $must) { throw "missing required seam $must" }
}
Write-Output "required seams present"

foreach ($bad in @("SendInput", "mouse_event", "keybd_event",
    "SetCursorPos", "Set-ItemProperty", "New-ItemProperty",
    "C:/Users", "C:\\Users", "Temp.opencode", "be-gap", "be-dpi",
    "Get-Process.*-Name")) {
  if ($code -match $bad) { throw "forbidden pattern present: $bad" }
}
Write-Output "forbidden patterns absent"

. $Lib
foreach ($fn in @("Invoke-LiveFinalCleanup", "Restore-LiveLegSafety",
    "Confirm-LiveForeground", "Test-LiveClientExposure")) {
  if ($null -eq (Get-Command $fn -ErrorAction SilentlyContinue)) {
    throw "dot-sourced command missing: $fn"
  }
}
Write-Output "dot-source commands present"
Write-Output "LIVE-LIB-CHECK-OK"
