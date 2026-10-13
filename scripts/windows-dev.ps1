param(
  [ValidateSet("lifecycle-smoke", "dev", "stop", "proof", "tile", "settings")]
  [string]$Action = "dev",
  [string]$Mode = "",
  [string]$TileArgs = ""
)
$ErrorActionPreference = "Stop"
$Repo = Split-Path -Parent $PSScriptRoot
$Target = Join-Path $Repo "target"
$Scripts = Join-Path $Repo "scripts"
$DevDir = Join-Path $Target "windows-dev"
$PointerFile = Join-Path $DevDir "current-dev-payload.txt"
$OwnerName = "tiler-windows.exe"
$HelperName = "tiler-test-window.exe"

function Fail([string]$Msg) { throw $Msg }
function Invoke-Native([string]$Exe, [string[]]$CliArgs) {
  $out = (& $Exe @CliArgs 2>&1 | Out-String)
  $code = $LASTEXITCODE
  if ($code -ne 0) { Fail "native exit ${code}: $Exe $($CliArgs -join ' ') $out" }
  return "$out".Trim()
}
function Get-CimProc([int]$ProcessId) {
  $p = Get-CimInstance Win32_Process -Filter "ProcessId=$ProcessId"
  if (-not $p) { Fail "unknown: no CIM record pid=$ProcessId" }
  return $p
}
function Assert-ParentIsExplorer([int]$ProcessId) {
  $p = Get-CimProc $ProcessId
  $pp = $p.ParentProcessId
  $par = Get-CimProc $pp
  if ("$($par.Name)" -ine "explorer.exe") { Fail "refuse: parent of $ProcessId is $($par.Name), want explorer.exe" }
}
function Test-ExeEqual([string]$A, [string]$B) {
  $na = "$A".Replace("/", "\")
  $nb = "$B".Replace("/", "\")
  return ($na -ieq $nb)
}
function Test-ProcessAliveSameCreation([int]$ProcessId, [string]$ExpectedCreation) {
  try {
    $proc = Get-Process -Id $ProcessId -ErrorAction Stop
  } catch {
    $fq = "$($_.FullyQualifiedErrorId)"
    if ($fq -like "NoProcessFound*") { return $false }
    throw
  }
  $hex = "{0:x16}" -f $proc.StartTime.ToUniversalTime().ToFileTimeUtc()
  if ($hex -ieq "$ExpectedCreation") { return $true }
  return $false
}
function Start-ExplorerGui([string]$Exe, [string]$ArgString, [string]$WorkDir) {
  if (-not [IO.Path]::IsPathFullyQualified($Exe)) { Fail "refuse: exe not fully qualified: $Exe" }
  if (-not [IO.Path]::IsPathFullyQualified($WorkDir)) { Fail "refuse: workdir not fully qualified: $WorkDir" }
  if (-not (Test-Path -LiteralPath $Exe -PathType Leaf)) { Fail "refuse: missing exe: $Exe" }
  $shell = New-Object -ComObject Shell.Application
  $desktopHandle = 0
  # SWC_DESKTOP + SWFO_NEEDDISPATCH selects Explorer's broker, not the caller's.
  $desktop = $shell.Windows().FindWindowSW(0, 0, 8, [ref]$desktopHandle, 1)
  $desktop.Document.Application.ShellExecute($Exe, $ArgString, $WorkDir, "open", 1)
}
function Get-LedgerDir([string]$Payload) {
  $j = Invoke-Native $Payload @("identity") | ConvertFrom-Json
  return "$($j.ledger_directory)"
}
function Assert-LedgerClean([string]$Payload) {
  $ld = Get-LedgerDir $Payload
  if (Test-Path (Join-Path $ld "ledger.json")) { Fail "ledger not clean" }
  if (Test-Path (Join-Path $ld "stop.request")) { Fail "stop.request residue" }
  if (Test-Path (Join-Path $ld "workspace.request")) { Fail "workspace.request residue" }
}
function Stop-PayloadOwner([string]$Payload) {
  $s = Invoke-Native $Payload @("stop") | ConvertFrom-Json
  if ((-not $s.owner_exited) -and ($s.recovery_required -ne $false)) {
    Fail "stop did not exit owner: $($s | ConvertTo-Json -Compress)"
  }
  $r = Invoke-Native $Payload @("restore") | ConvertFrom-Json
  if (-not $r.restored) { Fail "restore failed: $($r | ConvertTo-Json -Compress)" }
  Assert-LedgerClean $Payload
  return @{ stop = $s; restore = $r }
}
function Wait-Ready([string]$Payload, [int]$TimeoutSec = 5) {
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    $raw = Invoke-Native $Payload @("ready")
    $j = $raw | ConvertFrom-Json
    if ($j.ready -eq $true) { return $j }
    Start-Sleep -Milliseconds 250
  }
  Fail "ready timeout after ${TimeoutSec}s"
}
function Assert-OwnerReady([string]$Payload, [string]$Tag) {
  $ready = Wait-Ready $Payload 5
  if (-not (Test-ExeEqual "$($ready.owner.exe_path)" "$Payload")) { Fail "$Tag owner peer mismatch" }
  Assert-ParentIsExplorer ([int]$ready.owner.pid)
  return $ready
}
function Stop-OwnerVerified([string]$Payload, [bool]$Force, [string]$Tag) {
  if ($Force) { $st = Invoke-Native $Payload @("emergency-stop") | ConvertFrom-Json }
  else { $st = Invoke-Native $Payload @("stop") | ConvertFrom-Json }
  if (-not $st.owner_exited) { Fail "$Tag stop no exit" }
  return $st
}
function Assert-DevMode([string]$Mode) {
  if ($Mode -ne "" -and $Mode -ne "verbose" -and $Mode -ne "trace") {
    Fail "refuse: mode must be ''|'verbose'|'trace', got '$Mode'"
  }
}
function Get-LedgerOwnerFull([string]$Payload) {
  $ld = Get-LedgerDir $Payload
  $lp = Join-Path $ld "ledger.json"
  if (-not (Test-Path -LiteralPath $lp -PathType Leaf)) { return $null }
  $txt = Get-Content -LiteralPath $lp -Raw -ErrorAction Stop
  $j = $txt | ConvertFrom-Json
  return $j.owner
}
function Test-OwnerIdentityEqual([object]$A, [object]$B) {
  if ($null -eq $A -or $null -eq $B) { return $false }
  if ([int]$A.pid -ne [int]$B.pid) { return $false }
  if ("$($A.process_creation)" -ine "$($B.process_creation)") { return $false }
  if ("$($A.user_sid)" -ine "$($B.user_sid)") { return $false }
  if ([int]$A.session_id -ne [int]$B.session_id) { return $false }
  if (-not (Test-ExeEqual "$($A.exe_path)" "$($B.exe_path)")) { return $false }
  return $true
}
function Test-PathWithinDevDir([string]$Candidate) {
  $raw = "$Candidate".Trim()
  if ($raw -eq "") { return $false }
  if (-not [IO.Path]::IsPathFullyQualified($raw)) { return $false }
  try {
    $c = [IO.Path]::GetFullPath($raw).Replace("/", "\").TrimEnd("\")
    $d = [IO.Path]::GetFullPath("$DevDir").Replace("/", "\").TrimEnd("\")
  } catch { return $false }
  return ($c -ilike "$d\*")
}
function Remove-MatchingPointer([string]$Payload) {
  if (-not (Test-Path -LiteralPath $PointerFile -PathType Leaf)) { return }
  $cur = "$((Get-Content -LiteralPath $PointerFile -Raw -ErrorAction Stop))".Trim()
  if (($cur -ne "") -and (Test-ExeEqual $cur $Payload)) {
    Remove-Item -LiteralPath $PointerFile -Force -ErrorAction Stop
  }
}
function Test-Interrupted([object]$Err) {
  if ($null -eq $Err) { return $false }
  try {
    if ($Err -is [System.Management.Automation.PipelineStoppedException]) { return $true }
    $ex = $Err.Exception
    if ($null -ne $ex) {
      if ($ex -is [System.Management.Automation.PipelineStoppedException]) { return $true }
      if ("$($ex.GetType().FullName)" -like "*StopUpstream*") { return $true }
    }
    if ("$($Err.FullyQualifiedErrorId)" -like "PipelineStopped*") { return $true }
  } catch { }
  return $false
}
function Invoke-DevCleanup([string]$Payload, [object]$LaunchedOwner) {
  # Bounded recovery for the foreground dev session: re-read the ledger owner
  # FULL (pid+creation+sid+session+exe), stop/restore only when its exe is
  # this session payload (native stop/restore revalidate full live identity),
  # preserve a foreign owner untouched, and never claim a verified exit when
  # readiness was unproven and no ledger binds the owner.
  $res = @{ ok = $true; verified = $false; error = ""; ledgerHadOwner = $false }
  try {
    $current = Get-LedgerOwnerFull $Payload
    if (($null -eq $LaunchedOwner) -and ($null -eq $current)) {
      # Before-ready path: ledger may appear late (owner racing ready).
      # Bounded 5s poll so a foreign owner appearing late is still preserved,
      # and an absent ledger stays an honest unconfirmed pointer (no stop).
      $pollDeadline = (Get-Date).AddSeconds(5)
      while (($null -eq $current) -and ((Get-Date) -lt $pollDeadline)) {
        Start-Sleep -Milliseconds 250
        $current = Get-LedgerOwnerFull $Payload
      }
    }
    if ($null -ne $current) { $res.ledgerHadOwner = $true }
    if (($null -ne $current) -and (-not (Test-ExeEqual "$($current.exe_path)" $Payload))) {
      throw "owner mismatch: ledger owner exe $($current.exe_path) is not this session payload $Payload; preserved (resolve that owner first; recover this session with: just stop)"
    }
    if (($null -ne $LaunchedOwner) -and $res.ledgerHadOwner -and (-not (Test-OwnerIdentityEqual $current $LaunchedOwner))) {
      throw "owner replaced (launched $($LaunchedOwner | ConvertTo-Json -Compress), ledger $($current | ConvertTo-Json -Compress)); will not stop another owner"
    }
    if (($null -eq $current) -and ($null -eq $LaunchedOwner)) {
      # Absent after the bounded poll: no ledger binds an owner, so there is
      # nothing verified to stop/restore. Keep the pointer for `just stop`.
      return $res
    }
    $null = Stop-PayloadOwner $Payload
    if ($null -ne $LaunchedOwner) {
      if (Test-ProcessAliveSameCreation ([int]$LaunchedOwner.pid) "$($LaunchedOwner.process_creation)") {
        throw "owner still alive after stop (pid $($LaunchedOwner.pid))"
      }
      $res.verified = $true
    } elseif ($res.ledgerHadOwner) {
      # Ready never proven, but the ledger owner exe matched this unique
      # session payload and native stop revalidated its full live identity.
      $res.verified = $true
    }
    if ($res.verified) { Remove-MatchingPointer $Payload }
  } catch {
    $res.ok = $false
    $res.error = "$_"
  }
  return $res
}
function Get-DevExitCode([bool]$Completed, [bool]$WasInterrupt, [hashtable]$Cleanup) {
  # Clean Ctrl+C with verified stop+restore+ledger-clean is success, not
  # failure; genuine body errors stay nonzero. Never force exit 0.
  if ($Completed -and $Cleanup.ok -and $Cleanup.verified) { return 0 }
  if ($WasInterrupt -and $Cleanup.ok -and $Cleanup.verified) { return 0 }
  return 1
}
function Complete-DevSession([bool]$Completed, [object]$BodyRec, [string]$BodyErr, [string]$Payload, [object]$LaunchedOwner) {
  $cleanup = Invoke-DevCleanup $Payload $LaunchedOwner
  $wasInterrupt = Test-Interrupted $BodyRec
  if ((-not $wasInterrupt) -and (-not $Completed) -and ($null -eq $BodyRec) -and ($BodyErr -eq "")) {
    # Real Ctrl+C can bypass catch (no record/error populated); no normal flow
    # reaches finally launched-but-incomplete with both empty.
    $wasInterrupt = $true
  }
  $code = Get-DevExitCode $Completed $wasInterrupt $cleanup
  $lines = @()
  if ($code -eq 0) {
    if ($Completed) { $lines += "dev success: owner exited and ledger clean" }
    else { $lines += "dev interrupted: verified stop+restore, owner exited and ledger clean" }
  } else {
    if ($wasInterrupt) { $lines += "dev interrupted" }
    elseif ($BodyErr -ne "") { $lines += "dev failed: $BodyErr" }
    elseif (-not $Completed) { $lines += "dev failed before completion" }
    if (-not $cleanup.ok) { $lines += "dev cleanup failed: $($cleanup.error)" }
    elseif (-not $cleanup.verified) { $lines += "dev note: owner exit unconfirmed; session pointer kept, recover with: just stop" }
  }
  return @{ code = $code; lines = $lines }
}
function Assert-NoExistingOwner([string]$ProbeExe, [string]$Tag) {
  $ld = Get-LedgerDir $ProbeExe
  $lp = Join-Path $ld "ledger.json"
  if (Test-Path -LiteralPath $lp -PathType Leaf) {
    Fail "$Tag refuse: existing owner ledger present at $lp; stop/restore first (will not stop another owner)"
  }
  try {
    $raw = Invoke-Native $ProbeExe @("ready")
    $j = $raw | ConvertFrom-Json
    if ($j.ready -eq $true) {
      Fail "$Tag refuse: existing owner ready (owner=$($j.owner | ConvertTo-Json -Compress))"
    }
  } catch {
    # Fail-closed: any identity/ready query failure refuses the launch.
    throw
  }
}
function Stream-OwnerLog([string]$LogPath, [int]$OwnerPid, [string]$OwnerCreation) {
  # Incremental tail: one held FileStream shared read/write with the owner,
  # drained per poll. Missing file, open/read/final errors throw to the outer
  # dev try/finally (verified cleanup owns recovery); no warning retries or
  # silent indefinite stream when the log never appears.
  $fs = $null
  $sr = $null
  try {
    if (-not $LogPath -or -not (Test-Path -LiteralPath $LogPath -PathType Leaf)) {
      Fail "refuse: missing owner log: $LogPath"
    }
    $fs = [IO.File]::Open($LogPath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
    try {
      $sr = New-Object IO.StreamReader($fs)
    } catch {
      $fs.Close()
      $fs = $null
      throw
    }
    $fs = $null
    while ($true) {
      $chunk = $sr.ReadToEnd()
      if ($null -ne $chunk -and $chunk.Length -gt 0) { Write-Output $chunk }
      if (-not (Test-ProcessAliveSameCreation $OwnerPid $OwnerCreation)) { break }
      Start-Sleep -Milliseconds 250
    }
    $tail = $sr.ReadToEnd()
    if ($null -ne $tail -and $tail.Length -gt 0) { Write-Output $tail }
  } finally {
    if ($null -ne $sr) { $sr.Close() } elseif ($null -ne $fs) { $fs.Close() }
  }
}
# Dot-source guard: reusable helpers above stay available when sourced;
# direct -File execution continues to the action blocks below.
if ("$($MyInvocation.InvocationName)" -eq ".") { return }
if ($Action -eq "settings") {
  if (-not (Test-Path $Target)) { Fail "missing target/ parent" }
  if (-not (Test-Path $Scripts)) { Fail "missing scripts/ parent" }
  & cargo build --locked -p tiler-windows
  if ($LASTEXITCODE -ne 0) { Fail "cargo build failed" }
  $exe = Join-Path $Target "debug\$OwnerName"
  if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { Fail "refuse: missing exe: $exe" }
  # Same Explorer-broker launch as dev/tile: the caller never becomes the
  # window's parent (protected terminal tree stays out of the actor path).
  # The settings window has no ready endpoint; the recipe returns after launch.
  Start-ExplorerGui $exe "settings" $Target
  Write-Output "settings launched via Explorer broker; close the window to finish (Close never applies)"
  exit 0
}
if ($Action -eq "lifecycle-smoke" -or $Action -eq "dev" -or $Action -eq "stop" -or $Action -eq "tile") {
  if (-not (Test-Path $Target)) { Fail "missing target/ parent" }
  if (-not (Test-Path $Scripts)) { Fail "missing scripts/ parent" }
  $payload = Join-Path $DevDir $OwnerName
  if ($Action -eq "lifecycle-smoke" -or $Action -eq "dev" -or $Action -eq "tile") {
    if ($Action -eq "lifecycle-smoke" -or $Action -eq "dev") { Assert-DevMode $Mode }
    $trace = ($Mode -eq "trace")
    & cargo build --locked -p tiler-windows
    if ($LASTEXITCODE -ne 0) { Fail "cargo build failed" }
    $debugExe = Join-Path $Target "debug\$OwnerName"
    if (-not (Test-Path -LiteralPath $debugExe -PathType Leaf)) { Fail "refuse: missing exe: $debugExe" }
    $DevSessionDir = ""
    $helperDst = Join-Path $DevDir $HelperName
    if ($Action -eq "dev") {
      # Foreground dev runs from a fresh GUID child dir: the unique exe path
      # attributes the launch via the existing caller_owns exe equality.
      # Refuse any existing owner (fail-closed); only dev has these refusal
      # semantics, tile/lifecycle-smoke keep the fixed-payload auto-stop below.
      $null = Assert-NoExistingOwner $debugExe $Action
      $DevSessionDir = Join-Path $DevDir ([guid]::NewGuid().ToString("N"))
      New-Item -ItemType Directory -Force -Path $DevSessionDir | Out-Null
      $payload = Join-Path $DevSessionDir $OwnerName
      $helperDst = Join-Path $DevSessionDir $HelperName
    } else {
      # tile/lifecycle-smoke keep the existing fixed-payload semantics: stop a
      # previous fixed-payload owner, never another exe (native caller_owns
      # revalidates; a GUID dev owner fails safely with owner mismatch).
      if (Test-Path $payload) { $null = Stop-PayloadOwner $payload }
      New-Item -ItemType Directory -Force -Path $DevDir | Out-Null
    }
    Copy-Item (Join-Path $Target "debug\$OwnerName") $payload -Force
    Copy-Item (Join-Path $Target "debug\$HelperName") $helperDst -Force
    # Launch provenance: source + artifact identity bound immediately after
    # build, before the owner starts. The launch commit is the build source,
    # never a claim about an embedded binary identity.
    $provCommit = (& git -C $Repo rev-parse HEAD | Out-String).Trim()
    $provDirtyRaw = (& git -C $Repo status --short | Out-String).Trim()
    $provDirty = ($provDirtyRaw.Length -gt 0)
    $provOwnerHash = (Get-FileHash $payload -Algorithm SHA256).Hash
    $provHelperHash = (Get-FileHash $helperDst -Algorithm SHA256).Hash
    if ($Action -eq "tile") {
      # Normal user tiling only (explicit user dogfood): requires
      # `--user-start` in TileArgs and refuses any `--allowlist`. Proof uses
      # `tile-proof` via scripts/windows-tiling.ps1 so lost arguments can
      # never fall back to tiling the whole desktop.
      if ($TileArgs -notmatch "(^|\s)--user-start(\s|$)") {
        Fail "refuse: tile requires explicit --user-start in TileArgs"
      }
      if ($TileArgs -match "(^|\s)--allowlist(\s|$)") {
        Fail "refuse: tile never takes --allowlist; proof uses tile-proof"
      }
      $runArgs = "tile $TileArgs"
      $tag = "tile"
    } elseif ($Action -eq "dev") {
      # Foreground dev: normal user tiling via the existing tile loop. The
      # script adds --user-start automatically; only trace mode adds --trace.
      # Extra TileArgs pass through as a raw string. --allowlist is refused
      # here like tile (proof uses tile-proof).
      if ($TileArgs -match "(^|\s)--allowlist(\s|$)") {
        Fail "refuse: tile never takes --allowlist; proof uses tile-proof"
      }
      $runArgs = "tile $TileArgs"
      if ($runArgs -notmatch "(^|\s)--user-start(\s|$)") { $runArgs += " --user-start" }
      if ($trace -and ($runArgs -notmatch "(^|\s)--trace(\s|$)")) { $runArgs += " --trace" }
      $runArgs = $runArgs.Trim()
      $tag = "dev"
    } else {
      $runArgs = "run --seconds 600"
      if ($trace) { $runArgs += " --trace" }
      $tag = "lifecycle-smoke"
    }
    if ($Action -eq "dev") {
      # Foreground session in the GUID dir: stream the owner log until the
      # creation-bound owner exits. No CancelKeyPress handler: Ctrl+C runs
      # this try/finally and cleanup below owns the verified stop. Never
      # stops an owner whose ledger exe differs from this session payload;
      # the pointer stays for `just stop` whenever the exit is unconfirmed.
      # Pointer AFTER arg validation but BEFORE the broker launch so `just
      # stop` finds the GUID payload even when ready never proves, without
      # leaving a stale pointer on refused input.
      $payload | Set-Content -LiteralPath $PointerFile -Encoding utf8NoBOM
      $launched = $false
      $launchedOwner = $null
      $completed = $false
      $bodyErr = ""
      $bodyRec = $null
      try {
        $launched = $true
        Start-ExplorerGui $payload $runArgs $DevSessionDir
        $ready = Assert-OwnerReady $payload $tag
        $launchedOwner = $ready.owner
        $provenance = @{
          commit        = $provCommit
          dirty         = $provDirty
          dirty_detail  = $provDirtyRaw
          owner_sha256  = $provOwnerHash
          helper_sha256 = $provHelperHash
          argv          = $runArgs
          payload_path  = $payload
          log_path      = "$($ready.log_path)"
          owner         = $ready.owner
        }
        $provSidecar = "$($ready.log_path).provenance.json"
        $provParent = Split-Path -Parent $provSidecar
        if (-not (Test-Path -LiteralPath $provParent)) { throw "missing provenance parent: $provParent" }
        $provenance | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $provSidecar -Encoding utf8NoBOM
        Write-Output "log_path=$($ready.log_path)"
        Write-Output "owner=$($ready.owner | ConvertTo-Json -Compress)"
        Write-Output "provenance=$($provenance | ConvertTo-Json -Compress -Depth 8)"
        Write-Output "provenance_path=$provSidecar"
        Write-Output "ready=true"
        Stream-OwnerLog "$($ready.log_path)" ([int]$ready.owner.pid) "$($ready.owner.process_creation)"
        $completed = $true
      } catch {
        $bodyErr = "$_"
        $bodyRec = $_
        throw
      } finally {
        if ($launched) {
          $done = Complete-DevSession $completed $bodyRec $bodyErr $payload $launchedOwner
          # Ctrl+C stops PowerShell's output pipeline; report directly to the
          # console and exit with the computed cleanup status.
          foreach ($ln in $done.lines) { [Console]::WriteLine($ln) }
          [Environment]::Exit($done.code)
        }
      }
      exit 0
    }
    Start-ExplorerGui $payload $runArgs $DevDir
    $ready = Assert-OwnerReady $payload $tag
    # Launch provenance: build source commit plus dirty state and artifact
    # hashes, bound to the ready owner/log identity. Source identity only
    # (never a claim about an embedded binary identity); a dirty tree is
    # allowed developer debugging state, recorded not refused. Sidecar-only
    # plus stdout: the owner log stays single-writer.
    $provenance = @{
      commit        = $provCommit
      dirty         = $provDirty
      dirty_detail  = $provDirtyRaw
      owner_sha256  = $provOwnerHash
      helper_sha256 = $provHelperHash
      argv          = $runArgs
      payload_path  = $payload
      log_path      = "$($ready.log_path)"
      owner         = $ready.owner
    }
    $provSidecar = "$($ready.log_path).provenance.json"
    try {
      $provParent = Split-Path -Parent $provSidecar
      if (-not (Test-Path -LiteralPath $provParent)) { throw "missing provenance parent: $provParent" }
      $provenance | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $provSidecar -Encoding utf8NoBOM
    } catch {
      $null = Stop-PayloadOwner $payload
      throw
    }
    Write-Output "log_path=$($ready.log_path)"
    Write-Output "owner=$($ready.owner | ConvertTo-Json -Compress)"
    Write-Output "provenance=$($provenance | ConvertTo-Json -Compress -Depth 8)"
    Write-Output "provenance_path=$provSidecar"
    Write-Output "ready=true"
  } else {
    # stop: the dev pointer selects the exact GUID session payload when
    # present (and only when it stays inside target/windows-dev); otherwise
    # the existing fixed payload. Verified stop+restore first, then the
    # matching pointer is removed.
    $stopPayload = $payload
    $pointerUsed = $false
    if (Test-Path -LiteralPath $PointerFile -PathType Leaf) {
      $pointed = "$((Get-Content -LiteralPath $PointerFile -Raw -ErrorAction Stop))".Trim()
      if (($pointed -eq "") -or (-not (Test-PathWithinDevDir $pointed))) {
        Fail "refuse: invalid stop pointer content: '$pointed'"
      }
      $stopPayload = $pointed
      $pointerUsed = $true
    }
    if (-not (Test-Path -LiteralPath $stopPayload -PathType Leaf)) { Fail "no dev payload $stopPayload" }
    $null = Stop-PayloadOwner $stopPayload
    if ($pointerUsed) { Remove-MatchingPointer $stopPayload }
    Write-Output "stopped clean"
  }
  exit 0
}
if (-not (Test-Path $Target)) { Fail "missing target/ parent" }
if (-not (Test-Path $Scripts)) { Fail "missing scripts/ parent" }
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$proofDir = Join-Path $Target "windows-proof\$stamp-$PID"
New-Item -ItemType Directory -Force -Path $proofDir | Out-Null
$steps = [System.Collections.ArrayList]@()
function Rec([string]$Name, $Data) { $null = $steps.Add(@{ name = $Name; data = $Data }) }
$commit = (& git rev-parse HEAD | Out-String).Trim()
$dirty = (& git status --short | Out-String).Trim()
$actorPid = $PID
$actorExe = (Get-Process -Id $PID).Path
$actorPar = (Get-CimProc $PID).ParentProcessId
$devPayload = Join-Path $DevDir $OwnerName
$devHelper = Join-Path $DevDir $HelperName
if (-not (Test-Path $devPayload)) { Fail "no dev payload $devPayload" }
if (-not (Test-Path $devHelper)) { Fail "no dev helper $devHelper" }
$ident = (Invoke-Native $devPayload @("identity") | ConvertFrom-Json)
$artHash = (Get-FileHash $devPayload -Algorithm SHA256).Hash
$helpHash = (Get-FileHash $devHelper -Algorithm SHA256).Hash
Rec "env" @{ commit = $commit; dirty = $dirty; actorPid = $actorPid; actorExe = $actorExe; actorParent = $actorPar; sid = $ident.process.user_sid; session = $ident.process.session_id; il = $ident.integrity_level; ownerHash = $artHash; helperHash = $helpHash; payloadPath = $devPayload; helperPath = $devHelper; ledgerDirectory = "$($ident.ledger_directory)" }
function Proof-NoHide([string]$Payload, [bool]$Force) {
  $tag = $Force ? "nohide-forced" : "nohide-grace"
  Start-ExplorerGui $Payload "run --seconds 600" (Split-Path -Parent $Payload)
  $ready = Assert-OwnerReady $Payload $tag
  Rec "$tag-ready" $ready
  $st = Stop-OwnerVerified $Payload $Force $tag
  Rec "$tag-stop" $st
  $alive = Test-ProcessAliveSameCreation ([int]$ready.owner.pid) "$($ready.owner.process_creation)"
  if ($alive) { Fail "$tag owner alive" }
  $r = Invoke-Native $Payload @("restore") | ConvertFrom-Json
  Rec "$tag-restore" $r
}
Proof-NoHide $devPayload $false
Proof-NoHide $devPayload $true
$hBin = Join-Path $DevDir $HelperName
$receipt = Join-Path $proofDir "window.json"
if (Test-Path $receipt) { Fail "receipt preexists $receipt" }
Start-ExplorerGui $hBin "run --seconds 600 --receipt `"$receipt`"" $proofDir
$hDeadline = (Get-Date).AddSeconds(5)
while (-not (Test-Path $receipt)) { if ((Get-Date) -gt $hDeadline) { Fail "helper receipt timeout" }; Start-Sleep -Milliseconds 200 }
$hsnap = Get-Content $receipt -Raw | ConvertFrom-Json
Rec "helper-baseline" $hsnap
if (-not (Test-ExeEqual "$($hsnap.process.exe_path)" "$hBin")) { Fail "helper peer mismatch" }
Assert-ParentIsExplorer ([int]$hsnap.process.pid)
$insp = Invoke-Native $hBin @("inspect", "$($hsnap.hwnd)") | ConvertFrom-Json
if ($insp.tag -ne $hsnap.tag -or "$($insp.process.process_creation)" -ne "$($hsnap.process.process_creation)") { Fail "helper baseline mismatch" }
function Proof-Hide([string]$Payload, [string]$HBin, [object]$Base, [bool]$Force) {
  $tag = $Force ? "hide-forced" : "hide-grace"
  Start-ExplorerGui $Payload "run --seconds 120 --hide $($Base.hwnd)" (Split-Path -Parent $Payload)
  $ready = Assert-OwnerReady $Payload $tag
  Rec "$tag-ready" $ready
  $hid = Invoke-Native $HBin @("inspect", "$($Base.hwnd)") | ConvertFrom-Json
  if ($hid.visible -ne $false -or $hid.tag -ne $Base.tag) { Fail "$tag not hidden" }
  $st = Stop-OwnerVerified $Payload $Force $tag
  $alive = Test-ProcessAliveSameCreation ([int]$ready.owner.pid) "$($ready.owner.process_creation)"
  if ($alive) { Fail "$tag owner alive" }
  $still = Invoke-Native $HBin @("inspect", "$($Base.hwnd)") | ConvertFrom-Json
  if ($still.visible -ne $false -or $still.tag -ne $Base.tag -or "$($still.process.process_creation)" -ne "$($Base.process.process_creation)") { Fail "$tag helper changed" }
  Rec "$tag-stop" @{ stop = $st; helper = $still }
  $r = Invoke-Native $Payload @("restore") | ConvertFrom-Json
  $back = Invoke-Native $HBin @("inspect", "$($Base.hwnd)") | ConvertFrom-Json
  if ($back.visible -ne $true -or $back.tag -ne $Base.tag -or [int]$back.process.pid -ne [int]$Base.process.pid -or "$($back.process.process_creation)" -ne "$($Base.process.process_creation)" -or "$($back.process.user_sid)" -ne "$($Base.process.user_sid)" -or [int]$back.process.session_id -ne [int]$Base.process.session_id -or -not (Test-ExeEqual "$($back.process.exe_path)" "$($Base.process.exe_path)")) { Fail "$tag restore readback" }
  if ($back.left -ne $Base.left -or $back.top -ne $Base.top -or $back.right -ne $Base.right -or $back.bottom -ne $Base.bottom) { Fail "$tag rect changed" }
  Rec "$tag-restore" @{ restore = $r; readback = $back }
}
Proof-Hide $devPayload $hBin $hsnap $false
Proof-Hide $devPayload $hBin $hsnap $true
$closed = Invoke-Native $hBin @("close", "$($hsnap.hwnd)", "--tag", "$($hsnap.tag)") | ConvertFrom-Json
Rec "helper-close" $closed
$exitDeadline = (Get-Date).AddSeconds(5)
while ($true) {
  $held = Test-ProcessAliveSameCreation ([int]$hsnap.process.pid) "$($hsnap.process.process_creation)"
  if (-not $held) { break }
  if ((Get-Date) -gt $exitDeadline) { Fail "helper exit timeout" }
  Start-Sleep -Milliseconds 200
}
Assert-LedgerClean $devPayload
$report = @{ steps = $steps }
$report | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $proofDir "report.json")
$report | ConvertTo-Json -Depth 8 | Write-Output
