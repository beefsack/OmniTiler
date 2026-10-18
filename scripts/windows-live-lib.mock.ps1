$ErrorActionPreference = "Stop"
# Offline fault-injection mock for scripts/windows-live-lib.ps1.
# No owner launch, no window calls, no processes, no hooks: every live-API
# function is shadowed below and file operations stay inside a fresh temp
# dir. Drivers prove the desktop; this proves the harness logic.
#   pwsh -NoProfile -File scripts/windows-live-lib.mock.ps1
$Repo = Split-Path -Parent $PSScriptRoot
. (Join-Path $Repo "scripts\windows-live-lib.ps1")

$MockRoot = Join-Path ([IO.Path]::GetTempPath()) ("live-lib-mock-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $MockRoot | Out-Null
$script:MockPass = 0
$script:MockFail = 0
function Mock-Ok([string]$Name) { $script:MockPass++; Write-Output "mock pass: $Name" }
function Mock-Bad([string]$Name, [string]$Detail) {
  $script:MockFail++
  Write-Output "mock FAIL: $Name :: $Detail"
}
function Mock-Throws([string]$Name, [scriptblock]$Sb, [string]$Want) {
  try { & $Sb | Out-Null; Mock-Bad $Name "expected throw, none occurred" }
  catch {
    if ("$($_.Exception.Message)" -match $Want) { Mock-Ok $Name }
    else { Mock-Bad $Name "wrong throw: $($_.Exception.Message)" }
  }
}

# ---- live-API shadows (no user32, no processes, no external tools) ----
function Start-Sleep { param([int]$Seconds = 0, [int]$Milliseconds = 0) }
$script:MockProcs = @{}
$script:MockGoneIds = @()
function Get-Process {
  [CmdletBinding()]
  param([int]$Id)
  if ($script:MockGoneIds -contains $Id -or -not $script:MockProcs.ContainsKey($Id)) {
    $er = New-Object Management.Automation.ErrorRecord(
      (New-Object Exception("No process with ID $Id")),
      "NoProcessFoundForGivenId", "ObjectNotFound", $Id)
    $PSCmdlet.ThrowTerminatingError($er)
  }
  $e = $script:MockProcs[$Id]
  return (New-Object PSObject -Property @{ Path = $e.Path; StartTime = $e.StartTime })
}
$script:MockNative = @{}
$script:MockCalls = @()
function Invoke-LiveNative([string]$Exe, [string[]]$CliArgs) {
  $key = "$($CliArgs[0])"
  $script:MockCalls += @("native:$key")
  if (-not $script:MockNative.ContainsKey($key)) { throw "mock: no native response for $key" }
  $r = $script:MockNative[$key]
  if ($r -is [hashtable]) {
    if ($r.ContainsKey("Then")) { & $r.Then }
    if ($r.ContainsKey("Throw")) { throw $r.Throw }
    return $r.Out
  }
  return $r
}
$script:MockSnaps = @{}
function Get-LiveHelperInspect([string]$HelperExe, [uint64]$Hwnd) {
  if (-not $script:MockSnaps.ContainsKey("$Hwnd")) { throw "mock: no snap for $Hwnd" }
  return $script:MockSnaps["$Hwnd"]
}
$script:MockSpi = @{ arranging = 1; pen = 35; abm = "1" }
$script:MockDriftPath = ""
function Get-LiveTaskbarSpi {
  if ($script:MockDriftPath -ne "") {
    Add-Content -LiteralPath $script:MockDriftPath -Value " " -Encoding utf8NoBOM
  }
  return $script:MockSpi
}
$script:MockPaired = @{ ok_outer = $false; ok_visible = $false }
function Read-LivePaired([uint64]$Hwnd) { return $script:MockPaired }
$script:MockCim = @()
function Get-CimInstance { param([string]$ClassName, [string]$Filter) return $script:MockCim }
$script:MockMiseOut = "stopped clean"
$script:MockMiseExit = 0
function mise { $global:LASTEXITCODE = $script:MockMiseExit; return $script:MockMiseOut }
function git { return "mockrev" }

# ---- shared fixtures ----
$MockExe = Join-Path $MockRoot "helper.exe"
$MockDt = (Get-Date).AddDays(-1)
$MockCreation = ("{0:x16}" -f $MockDt.ToUniversalTime().ToFileTimeUtc())
function Mock-Snap([uint64]$Hwnd, [int]$PidNum, [string]$Creation,
    [string]$Tag, [string]$Exe, [bool]$Visible) {
  return (@{ hwnd = $Hwnd; tag = $Tag; visible = $Visible;
    process = @{ pid = $PidNum; process_creation = $Creation;
      exe_path = $Exe; user_sid = "S-MOCK"; session_id = 1 } } |
    ConvertTo-Json -Depth 6 | ConvertFrom-Json)
}

# ---- 1. identity accept + refusals ----
$snap = Mock-Snap 111 4242 $MockCreation "tagA" $MockExe $false
try {
  Assert-LiveHelperIdentity $snap 111 4242 $MockCreation "tagA" $MockExe "id-accept"
  Mock-Ok "identity-accept"
} catch { Mock-Bad "identity-accept" "$($_.Exception.Message)" }
Mock-Throws "identity-refuses-tag" {
  Assert-LiveHelperIdentity $snap 111 4242 $MockCreation "other" $MockExe "id-tag"
} "tag mismatch"
Mock-Throws "identity-refuses-pid" {
  Assert-LiveHelperIdentity $snap 111 9999 $MockCreation "tagA" $MockExe "id-pid"
} "pid changed"
Mock-Throws "identity-refuses-creation" {
  Assert-LiveHelperIdentity $snap 111 4242 "deadbeefdeadbeef" "tagA" $MockExe "id-creation"
} "creation changed"
Mock-Throws "identity-refuses-exe" {
  Assert-LiveHelperIdentity $snap 111 4242 $MockCreation "tagA" "other.exe" "id-exe"
} "exe mismatch"

# ---- 2. OS creation crosscheck ----
$script:MockProcs[4242] = @{ Path = $MockExe; StartTime = $MockDt }
try {
  $null = Assert-LiveOsCreation 4242 $MockCreation $MockExe "os-accept"
  Mock-Ok "os-creation-accept"
} catch { Mock-Bad "os-creation-accept" "$($_.Exception.Message)" }
$script:MockProcs[4243] = @{ Path = "foreign.exe"; StartTime = $MockDt }
Mock-Throws "os-creation-refuses-exe" {
  Assert-LiveOsCreation 4243 $MockCreation $MockExe "os-exe"
} "OS exe mismatch"
Mock-Throws "os-creation-refuses-stale" {
  Assert-LiveOsCreation 4242 "deadbeefdeadbeef" $MockExe "os-stale"
} "OS creation changed"

# ---- 3. exact close paths ----
$script:MockSnaps["111"] = $snap
$script:MockNative["close"] = @{ Out = '{"closed": true}';
  Then = { $script:MockGoneIds += @(4242) } }
try {
  $r = Close-LiveHelperExact $MockExe 111 4242 $MockCreation "tagA" "" "close-exit"
  if ($r -eq "exited") { Mock-Ok "close-exited" } else { Mock-Bad "close-exited" "got $r" }
} catch { Mock-Bad "close-exited" "$($_.Exception.Message)" }
$script:MockGoneIds = @()
$script:MockNative["close"] = '{"closed": true}'
try {
  $r = Close-LiveHelperExact $MockExe 111 4242 $MockCreation "tagA" "" "close-stuck"
  if ($r -eq "still-alive-after-5s") { Mock-Ok "close-still-alive" }
  else { Mock-Bad "close-still-alive" "got $r" }
} catch { Mock-Bad "close-still-alive" "$($_.Exception.Message)" }
$script:MockGoneIds = @(4242)
try {
  $r = Close-LiveHelperExact $MockExe 111 4242 $MockCreation "tagA" "" "close-absent"
  if ($r -eq "absent-unexpected") { Mock-Ok "close-absent-anomaly" }
  else { Mock-Bad "close-absent-anomaly" "got $r" }
} catch { Mock-Bad "close-absent-anomaly" "$($_.Exception.Message)" }
$script:MockGoneIds = @()
Mock-Throws "close-refuses-foreign-pid" {
  Close-LiveHelperExact $MockExe 111 4243 $MockCreation "tagA" "" "close-foreign"
} "not the helper"

# ---- 4. preimage roundtrip + mismatch refusals ----
$liveSettings = Join-Path $MockRoot "settings.json"
'{"revision":7,"core":{"inner_gap":8}}' | Set-Content -LiteralPath $liveSettings -Encoding utf8NoBOM
$origSettings = Join-Path $MockRoot "orig.json"
$spiFile = Join-Path $MockRoot "spi.json"
$steps = Join-Path $MockRoot "steps.jsonl"
try {
  $g = Save-LiveSettingsPreimage $liveSettings $origSettings $spiFile $steps
  $back = [IO.File]::ReadAllBytes($origSettings)
  $eq = ((Get-LiveBytesSha $back) -eq (Get-LiveBytesSha ([IO.File]::ReadAllBytes($liveSettings))))
  if ($eq) { Mock-Ok "preimage-roundtrip-bytes" } else { Mock-Bad "preimage-roundtrip-bytes" "copy differs" }
} catch { Mock-Bad "preimage-roundtrip-bytes" "$($_.Exception.Message)" }
try {
  $g2 = Save-LiveSettingsPreimage $liveSettings $origSettings $spiFile ""
  if ($g2.sha -eq $g.sha) { Mock-Ok "preimage-reuse" } else { Mock-Bad "preimage-reuse" "sha moved" }
} catch { Mock-Bad "preimage-reuse" "$($_.Exception.Message)" }
Add-Content -LiteralPath $liveSettings -Value " " -Encoding utf8NoBOM
Mock-Throws "preimage-refuses-live-drift" {
  Save-LiveSettingsPreimage $liveSettings $origSettings $spiFile ""
} "differ from group orig"
[IO.File]::WriteAllBytes($liveSettings, [IO.File]::ReadAllBytes($origSettings))
"garbage-not-json" | Set-Content -LiteralPath $origSettings -Encoding utf8NoBOM
Mock-Throws "preimage-refuses-corrupt-orig" {
  Save-LiveSettingsPreimage $liveSettings $origSettings $spiFile ""
} "do not parse as JSON"
[IO.File]::WriteAllBytes($origSettings, [IO.File]::ReadAllBytes($liveSettings))
Remove-Item -LiteralPath $spiFile -Force
Mock-Throws "preimage-requires-spi-file" {
  Save-LiveSettingsPreimage $liveSettings $origSettings $spiFile ""
} "spi preimage missing"
$script:MockSpi | ConvertTo-Json -Depth 6 -Compress |
  Set-Content -LiteralPath $spiFile -Encoding utf8NoBOM
$script:MockSpi = @{ arranging = 0; pen = 35; abm = "1" }
Mock-Throws "preimage-refuses-spi-drift" {
  Save-LiveSettingsPreimage $liveSettings $origSettings $spiFile ""
} "taskbar/SPI differs"
$script:MockSpi = @{ arranging = 1; pen = 35; abm = "1" }
try {
  $g = Save-LiveSettingsPreimage $liveSettings $origSettings $spiFile $steps
  Mock-Ok "preimage-restore-after-drift"
} catch { Mock-Bad "preimage-restore-after-drift" "$($_.Exception.Message)" }

# ---- 5. exposure short-circuits before any window call ----
try {
  $p = Test-LiveClientExposure 999 4242 "" "exposure-unproven"
  if ($null -eq $p) { Mock-Ok "exposure-null-without-rect" }
  else { Mock-Bad "exposure-null-without-rect" "returned a point" }
} catch { Mock-Bad "exposure-null-without-rect" "$($_.Exception.Message)" }

# ---- 6. serial admission gate ----
$ownerLog = Join-Path $MockRoot "owner.log"
'{"event":"visibility-transition","entries":[{"state":"observed","window":"tokA"}]}' |
  Set-Content -LiteralPath $ownerLog -Encoding utf8NoBOM
try {
  $tok = Wait-LiveObservedAdmission $ownerLog "" @() 1 5 "admit-1"
  if ($tok -eq "tokA") { Mock-Ok "admission-gates-single-token" }
  else { Mock-Bad "admission-gates-single-token" "got $tok" }
} catch { Mock-Bad "admission-gates-single-token" "$($_.Exception.Message)" }
Add-Content -LiteralPath $ownerLog -Value '{"event":"visibility-transition","entries":[{"state":"observed","window":"tokB"},{"state":"observed","window":"tokC"}]}' -Encoding utf8NoBOM
Mock-Throws "admission-parks-on-jump" {
  Wait-LiveObservedAdmission $ownerLog "" @("tokA") 2 1 "admit-jump"
} "observed jump"
Mock-Throws "token-map-refuses-duplicates" {
  Assert-LiveTokenMap @{ a = "t"; b = "t" } 2 ""
} "duplicate tokens"

# ---- 7. leg cleanup attempts every phase despite stop failure ----
# The fake repo root is created up front so the mandatory just-stop probe
# has a bound root in every leg test below.
$fakeRoot = Join-Path $MockRoot "repo"
New-Item -ItemType Directory -Force -Path $fakeRoot | Out-Null
"root = true" | Set-Content -LiteralPath (Join-Path $fakeRoot "mise.toml") -Encoding utf8NoBOM
$script:MockSnaps["111"] = (Mock-Snap 111 4242 $MockCreation "tagA" $MockExe $false)
$script:MockProcs[4242] = @{ Path = $MockExe; StartTime = $MockDt }
$script:MockGoneIds = @()
$script:MockNative = @{
  close   = @{ Out = '{"closed": true}'; Then = { $script:MockGoneIds += @(4242) } }
  stop    = @{ Throw = "boom-stop" }
  restore = @{ Out = '{"restored": true}' }
  ready   = @{ Out = '{"ready": false}' }
}
$legSteps = Join-Path $MockRoot "leg.jsonl"
"drift" | Add-Content -LiteralPath $liveSettings -Encoding utf8NoBOM
$helpers = @(@{ index = 1; hwnd = [uint64]111; pid = 4242; creation = $MockCreation; tag = "tagA" })
try {
  Restore-LiveLegSafety $legSteps $helpers $MockExe $liveSettings $g $true "owner.exe" $fakeRoot
  Mock-Bad "leg-parks-on-stop-failure" "expected park throw"
} catch {
  if ("$($_.Exception.Message)" -match "leg-cleanup-uncertain" -and
      "$($_.Exception.Message)" -match "owner-stop") { Mock-Ok "leg-parks-on-stop-failure" }
  else { Mock-Bad "leg-parks-on-stop-failure" "$($_.Exception.Message)" }
}
$legText = Get-Content -LiteralPath $legSteps -Raw
foreach ($want in @("owner-restore", "ready-post-stop", "settings-post-leg",
    "taskbar-spi-post-leg", "just-stop")) {
  if ($legText -match $want) { Mock-Ok "leg-still-runs-$want" }
  else { Mock-Bad "leg-still-runs-$want" "phase missing after stop failure" }
}
$liveBack = Get-LiveBytesSha ([IO.File]::ReadAllBytes($liveSettings))
if ($liveBack -eq $g.sha) { Mock-Ok "leg-restores-settings-bytes" }
else { Mock-Bad "leg-restores-settings-bytes" "live differs from orig" }

# ---- 8. leg cleanup survives settings-restore failure ----
$badLive = Join-Path $MockRoot "no-such-dir" "settings.json"
$script:MockNative["stop"] = @{ Out = '{"owner_exited": true}' }
$legSteps2 = Join-Path $MockRoot "leg2.jsonl"
try {
  Restore-LiveLegSafety $legSteps2 @() $MockExe $badLive $g $true "owner.exe" $fakeRoot
  Mock-Bad "leg-parks-on-restore-failure" "expected park throw"
} catch {
  if ("$($_.Exception.Message)" -match "settings-restore") { Mock-Ok "leg-parks-on-restore-failure" }
  else { Mock-Bad "leg-parks-on-restore-failure" "$($_.Exception.Message)" }
}
$legText2 = Get-Content -LiteralPath $legSteps2 -Raw
if ($legText2 -match "owner-stop") { Mock-Ok "leg-still-stops-owner" }
else { Mock-Bad "leg-still-stops-owner" "owner phase skipped after restore failure" }

# ---- 9. final cleanup: success, aggregate, location balance ----
$ledgerDir = Join-Path $MockRoot "session"
New-Item -ItemType Directory -Force -Path $ledgerDir | Out-Null
$outFile = Join-Path $MockRoot "final.json"
$beforeLoc = (Get-Location).Path
$script:MockMiseOut = "stopped clean"
$script:MockMiseExit = 0
try {
  $o = Invoke-LiveFinalCleanup $fakeRoot "owner.exe" $liveSettings $g @("helper.exe") $ledgerDir $outFile
  if ("$o" -match "LIVE-FINAL-CLEANUP-OK") { Mock-Ok "final-cleanup-ok" }
  else { Mock-Bad "final-cleanup-ok" "missing OK marker" }
} catch { Mock-Bad "final-cleanup-ok" "$($_.Exception.Message)" }
if ((Get-Location).Path -eq $beforeLoc) { Mock-Ok "final-balances-location" }
else { Mock-Bad "final-balances-location" "cwd moved" }
$script:MockMiseOut = "nope"
$script:MockMiseExit = 1
$outFile2 = Join-Path $MockRoot "final2.json"
try {
  Invoke-LiveFinalCleanup $fakeRoot "owner.exe" $liveSettings $g @("helper.exe") $ledgerDir $outFile2 | Out-Null
  Mock-Bad "final-aggregates-just-stop" "expected throw"
} catch {
  if ("$($_.Exception.Message)" -match "just-stop") { Mock-Ok "final-aggregates-just-stop" }
  else { Mock-Bad "final-aggregates-just-stop" "$($_.Exception.Message)" }
}
if (Test-Path -LiteralPath $outFile2) { Mock-Ok "final-evidence-always-written" }
else { Mock-Bad "final-evidence-always-written" "no OutFile after failure" }
$script:MockMiseOut = "stopped clean"
$script:MockMiseExit = 0
Mock-Throws "final-refuses-empty-names" {
  Invoke-LiveFinalCleanup $fakeRoot "owner.exe" $liveSettings $g @() $ledgerDir (Join-Path $MockRoot "never.json")
} "no owned names bound"
$neverEv = Get-Content -LiteralPath (Join-Path $MockRoot "never.json") -Raw | ConvertFrom-Json
if ($neverEv.just_stop_clean -eq $true -and $neverEv.settings_byte_equal -eq $true) {
  Mock-Ok "final-empty-names-still-stops-and-restores"
} else {
  Mock-Bad "final-empty-names-still-stops-and-restores" "stop/settings phases skipped"
}

# ---- 10. missing repo root still restores settings and writes evidence ----
$missingRoot = Join-Path $MockRoot "no-such-repo"
$outFile3 = Join-Path $MockRoot "final3.json"
try {
  Invoke-LiveFinalCleanup $missingRoot "owner.exe" $liveSettings $g @("helper.exe") $ledgerDir $outFile3 | Out-Null
  Mock-Bad "final-missing-root" "expected aggregate throw"
} catch {
  if ("$($_.Exception.Message)" -match "location") { Mock-Ok "final-missing-root" }
  else { Mock-Bad "final-missing-root" "$($_.Exception.Message)" }
}
if (Test-Path -LiteralPath $outFile3) {
  $ev3 = Get-Content -LiteralPath $outFile3 -Raw | ConvertFrom-Json
  if ($ev3.settings_restored -eq $true -and $ev3.owner_ready_false -eq $true) {
    Mock-Ok "final-missing-root-still-restores"
  } else {
    Mock-Bad "final-missing-root-still-restores" "restoration phases skipped"
  }
} else {
  Mock-Bad "final-missing-root-still-restores" "no evidence file"
}

# ---- 11. capture drift during SPI readback refuses ----
$driftLive = Join-Path $MockRoot "drift-settings.json"
'{"revision":9,"core":{"inner_gap":4}}' | Set-Content -LiteralPath $driftLive -Encoding utf8NoBOM
$script:MockDriftPath = $driftLive
Mock-Throws "preimage-refuses-capture-drift" {
  Save-LiveSettingsPreimage $driftLive (Join-Path $MockRoot "drift-orig.json") (Join-Path $MockRoot "drift-spi.json") ""
} "drifted during capture"
$script:MockDriftPath = ""

# ---- 12. failed step logging still invokes stop/restore/just ----
function Write-LiveStep { param([string]$StepsFile, [string]$Name, $Data) throw "mock-log-disk-full" }
$script:MockCalls = @()
$script:MockSnaps["111"] = (Mock-Snap 111 4242 $MockCreation "tagA" $MockExe $false)
$script:MockProcs[4242] = @{ Path = $MockExe; StartTime = $MockDt }
$script:MockGoneIds = @()
$script:MockNative = @{
  close   = @{ Out = '{"closed": true}'; Then = { $script:MockGoneIds += @(4242) } }
  stop    = @{ Out = '{"owner_exited": true}' }
  restore = @{ Out = '{"restored": true}' }
  ready   = @{ Out = '{"ready": false}' }
}
$script:MockMiseOut = "stopped clean"
$script:MockMiseExit = 0
$logFailHelpers = @(@{ index = 1; hwnd = [uint64]111; pid = 4242; creation = $MockCreation; tag = "tagA" })
try {
  Restore-LiveLegSafety (Join-Path $MockRoot "logfail.jsonl") $logFailHelpers $MockExe $liveSettings $g $true "owner.exe" $fakeRoot
  Mock-Bad "leg-parks-on-log-failure" "expected park throw"
} catch {
  if ("$($_.Exception.Message)" -match "step-log") { Mock-Ok "leg-parks-on-log-failure" }
  else { Mock-Bad "leg-parks-on-log-failure" "$($_.Exception.Message)" }
}
foreach ($wantCall in @("native:stop", "native:restore", "native:ready")) {
  if ($script:MockCalls -contains $wantCall) { Mock-Ok "leg-log-fail-still-runs-$wantCall" }
  else { Mock-Bad "leg-log-fail-still-runs-$wantCall" "phase skipped: $($script:MockCalls -join ',')" }
}

Remove-Item -LiteralPath $MockRoot -Recurse -Force
Write-Output ("mock summary pass={0} fail={1}" -f $script:MockPass, $script:MockFail)
if ($script:MockFail -gt 0) { throw "windows-live-lib mock has $($script:MockFail) failures" }
Write-Output "LIVE-LIB-MOCK-OK"
