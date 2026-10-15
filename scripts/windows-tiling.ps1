param(
)
$ErrorActionPreference = "Stop"
$Repo = Split-Path -Parent $PSScriptRoot
# Owned-helpers-only proof driver (no Stage param: Apps was removed; agents
# never touch non-owned HWNDs). Reuse the Explorer desktop broker (avoids
# Terminal ancestry for every launched payload) plus exact-owner stop/restore
# helpers. Read-only and launch helpers only; this driver never tiles, hides,
# or hooks anything itself.
. (Join-Path $Repo "scripts\windows-dev.ps1")
# No Set-Location anywhere in this driver (tooling rule): git runs with -C,
# cargo with --manifest-path, every other path is absolute. Invoke from the
# repo checkout (the just recipes do); dot-sourcing above only adds the dev
# helpers and takes no parameters.
if (-not (Test-Path -LiteralPath (Join-Path $Repo "Cargo.toml") -PathType Leaf)) {
  throw "driver must run from the repo checkout"
}

# Scoped Phase 2 tiling proof driver (physical desktop, medium integrity,
# no hooks, no hide, no activation, no titles). Single persistent pwsh
# process controls every step and writes one JSON report. Owned helpers only:
# three passive helpers are created first, the frozen allowlist (with lifetime
# tags) is built from their receipts, and the dedicated `tile-proof` native
# command refuses without a valid allowlist and never falls back to normal
# mode. Normal user tiling (Terminal included) is a separate explicit
# `--user-start` procedure and is never invoked here.
#
#   pwsh -NoProfile -File scripts/windows-tiling.ps1
#
# Convergence waits on exact eligibility plus same-tick full-plan desired
# and per-entry native readback evidence (never vacuous). Proof mode and trace
# are verified from `tile-start` before the first admission; nonmoving owners
# run `tile-proof --trace` so the zero-write assertion observes real `write`
# events instead of an empty trace. Any unexpected result stops the stage,
# recovers the owner, and reports; there is no live retry loop. No live window
# titles or content are ever read.

$INNER_GAP = 8
$OUTER_GAP = 8

# No desktop-app specs: agents own helpers only. The user runs normal tiling
# separately with explicit --user-start; this driver never selects Notepad,
# Paint, Calculator, or any other non-owned window.
$EXPECTED_DPI = 120

$steps = [System.Collections.ArrayList]@()
function Rec([string]$Name, $Data) { $null = $steps.Add(@{ name = $Name; data = $Data }) }
function Sha256-String([string]$Text) {
  $bytes = [Text.Encoding]::UTF8.GetBytes($Text)
  $hash = [Security.Cryptography.SHA256]::Create().ComputeHash($bytes)
  ($hash | ForEach-Object { $_.ToString("x2") }) -join ""
}
function Read-CompleteText([string]$Path) {
  # Shared-read so the appending native owner never blocks the observer.
  $fs = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
  try {
    $sr = New-Object IO.StreamReader($fs)
    return $sr.ReadToEnd()
  } finally {
    $fs.Close()
  }
}
function Get-CompleteLines([string]$Path) {
  # Newline-COMPLETE records only: a torn append tail without its terminating
  # newline is deferred until the next observation, so a partial write is never
  # parsed. Marks stay 0-based complete-line offsets, so nothing is lost.
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "missing log $Path" }
  $text = Read-CompleteText $Path
  $complete = @()
  if ([string]::IsNullOrEmpty($text)) { return ,$complete }
  $endsNewline = $text.EndsWith("`n")
  $parts = $text -split "`n"
  for ($i = 0; $i -lt $parts.Count; $i++) {
    $last = ($i -eq ($parts.Count - 1))
    if ($last -and -not $endsNewline) { break }
    $line = "$($parts[$i])" -replace "`r$", ""
    if ($last -and $line -eq "") { break }
    $complete += $line
  }
  return ,$complete
}
function Read-JsonLines([string]$Path) {
  $out = @()
  $n = 0
  foreach ($line in (Get-CompleteLines $Path)) {
    $n++
    if ("$line".Trim() -ne "") {
      try { $out += ($line | ConvertFrom-Json) }
      catch { throw "malformed JSON $Path line ${n}: $($_.Exception.Message)" }
    }
  }
  return $out
}
function Rect-FromArray($a) { return @{ x = [int]$a[0]; y = [int]$a[1]; w = [int]$a[2]; h = [int]$a[3] } }
function Rect-Key($r) { return "$($r.x),$($r.y),$($r.w),$($r.h)" }
function Rect-Overlaps($a1, $a2, $b1, $b2) { return ($a1 -lt $b2) -and ($b1 -lt $a2) }

# Layout-agnostic gap/margin check in physical pixels: every tile edge sits
# on the domain edge or exactly INNER_GAP from a neighbor edge with real
# overlap, and every tile is contained in the domain.
function Test-GapMath($Domain, $Rects) {
  $Rects = @($Rects)
  $failures = @()
  foreach ($r in $Rects) {
    if ($r.x -lt $Domain.x -or $r.y -lt $Domain.y -or
        ($r.x + $r.w) -gt ($Domain.x + $Domain.w) -or
        ($r.y + $r.h) -gt ($Domain.y + $Domain.h)) {
      $failures += "outside-domain $(Rect-Key $r)"
    }
  }
  foreach ($r in $Rects) {
    $right = $r.x + $r.w
    $bottom = $r.y + $r.h
    # Left edge.
    if ($r.x -ne $Domain.x) {
      $hit = $false
      foreach ($s in $Rects) {
        if ((($s.x + $s.w) -eq ($r.x - $INNER_GAP)) -and (Rect-Overlaps $s.y ($s.y + $s.h) $r.y ($r.y + $r.h))) { $hit = $true; break }
      }
      if (-not $hit) { $failures += "left-edge $(Rect-Key $r)" }
    }
    # Right edge.
    if ($right -ne ($Domain.x + $Domain.w)) {
      $hit = $false
      foreach ($s in $Rects) {
        if (($s.x -eq ($right + $INNER_GAP)) -and (Rect-Overlaps $s.y ($s.y + $s.h) $r.y ($r.y + $r.h))) { $hit = $true; break }
      }
      if (-not $hit) { $failures += "right-edge $(Rect-Key $r)" }
    }
    # Top edge.
    if ($r.y -ne $Domain.y) {
      $hit = $false
      foreach ($s in $Rects) {
        if ((($s.y + $s.h) -eq ($r.y - $INNER_GAP)) -and (Rect-Overlaps $s.x ($s.x + $s.w) $r.x ($r.x + $r.w))) { $hit = $true; break }
      }
      if (-not $hit) { $failures += "top-edge $(Rect-Key $r)" }
    }
    # Bottom edge.
    if ($bottom -ne ($Domain.y + $Domain.h)) {
      $hit = $false
      foreach ($s in $Rects) {
        if (($s.y -eq ($bottom + $INNER_GAP)) -and (Rect-Overlaps $s.x ($s.x + $s.w) $r.x ($r.x + $r.w))) { $hit = $true; break }
      }
      if (-not $hit) { $failures += "bottom-edge $(Rect-Key $r)" }
    }
  }
  return @{ ok = ($failures.Count -eq 0); failures = $failures }
}

function Get-Inspect([string]$OwnerPayload, [string]$Allowlist) {
  return (Invoke-Native $OwnerPayload @("inspect", "--allowlist", $Allowlist) | ConvertFrom-Json)
}
function Get-EligibleRects($Inspect) {
  $rects = @()
  foreach ($w in $Inspect.windows) {
    if ($w.eligible -eq $true) { $rects += (Rect-FromArray $w.visible) }
  }
  return ,$rects
}
function Get-VisibleKeys($Inspect) {
  $keys = @()
  foreach ($w in $Inspect.windows) {
    if ($w.eligible -eq $true) { $keys += (Rect-Key (Rect-FromArray $w.visible)) }
  }
  return ,@($keys | Sort-Object)
}
function Get-EligibleHwnds($Inspect) {
  $out = @()
  foreach ($w in $Inspect.windows) {
    if ($w.eligible -eq $true) { $out += [uint64]$w.hwnd }
  }
  return ,@($out | Sort-Object)
}
function Get-IdentityFrames($Inspect) {
  # Sorted identity-bound per-HWND outer+visible frame records for exactly the
  # eligible entries: HWND plus full matching identity fields plus frames. A
  # swap of rectangles between two HWNDs fails even when the sorted rect SET
  # is equal, because each record stays bound to its HWND+identity.
  $rows = @()
  foreach ($w in $Inspect.windows) {
    if ($w.eligible -eq $true) {
      $rows += (@{
        hwnd = [uint64]$w.hwnd
        pid = [uint32]$w.pid
        process_creation = "$($w.process_creation)"
        exe_path = "$($w.exe_path)"
        user_sid = "$($w.user_sid)"
        session_id = [uint32]$w.session_id
        tag = "$($w.tag)"
        outer = @(($w.outer | ForEach-Object { [int]$_ }))
        visible = @(($w.visible | ForEach-Object { [int]$_ }))
      })
    }
  }
  return ,@($rows | Sort-Object { [uint64]$_.hwnd })
}
function Assert-SameIdentityFrames($Before, $After, [string]$Tag) {
  # No @() re-wrap: Get-IdentityFrames already returns one array object, and
  # wrapping it again would nest the array and break indexing/counting.
  $b = Get-IdentityFrames $Before
  $a = Get-IdentityFrames $After
  if ($b.Count -ne $a.Count) { throw "$Tag window count changed: before=$($b.Count) after=$($a.Count)" }
  for ($i = 0; $i -lt $b.Count; $i++) {
    $x = $b[$i]
    $y = $a[$i]
    if ([uint64]$x.hwnd -ne [uint64]$y.hwnd) { throw "$Tag HWND changed at index ${i}" }
    if ([uint32]$x.pid -ne [uint32]$y.pid) { throw "$Tag pid changed hwnd=$($x.hwnd)" }
    if ([uint32]$x.session_id -ne [uint32]$y.session_id) { throw "$Tag session changed hwnd=$($x.hwnd)" }
    foreach ($k in @("process_creation", "exe_path", "user_sid", "tag")) {
      if ("$($x.$k)" -cne "$($y.$k)") { throw "$Tag $k changed hwnd=$($x.hwnd)" }
    }
    $xo = ($x.outer -join ",")
    $yo = ($y.outer -join ",")
    $xv = ($x.visible -join ",")
    $yv = ($y.visible -join ",")
    if ($xo -cne $yo) { throw "$Tag outer frame changed hwnd=$($x.hwnd): before=[$xo] after=[$yo]" }
    if ($xv -cne $yv) { throw "$Tag visible frame changed hwnd=$($x.hwnd): before=[$xv] after=[$yv]" }
  }
}
function Get-PlanSnapshot($Events) {
  # Latest tick carrying BOTH a full Engine plan and its per-entry native
  # readback. A tick with only one half is not evidence yet.
  $plans = @{}
  $details = @{}
  foreach ($e in $Events) {
    if ($e.event -eq "plan") { $plans[[uint64]$e.tick] = $e }
    elseif ($e.event -eq "readback-detail") { $details[[uint64]$e.tick] = $e }
  }
  $ticks = @($plans.Keys | Where-Object { $details.ContainsKey($_) } | Sort-Object -Descending)
  if ($ticks.Count -eq 0) { return $null }
  $tick = [uint64]$ticks[0]
  return @{ tick = $tick; plan = $plans[$tick]; detail = $details[$tick] }
}
# Bounded observation wait for exact convergence: eligible set equals the
# expected HWNDs, the latest same-tick full plan desired set equals the
# matched native readback set equals the inspected eligible frames
# (non-empty; an empty plan never counts), and gap math holds. Observes
# only; never retries mutations.
function Wait-Converged([string]$OwnerPayload, [string]$Allowlist, [array]$Hwnds, [string]$LogPath, [int]$Mark, $Domain, [int]$TimeoutSec) {
  $want = @($Hwnds | ForEach-Object { [uint64]$_ } | Sort-Object)
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  $last = "no observation yet"
  while ((Get-Date) -lt $deadline) {
    $insp = Get-Inspect $OwnerPayload $Allowlist
    $got = Get-EligibleHwnds $insp
    if (($got -join "|") -eq ($want -join "|") -and $want.Count -gt 0) {
      $tail = Get-LogTail $LogPath $Mark
      $snap = Get-PlanSnapshot $tail.events
      if ($snap -ne $null) {
        $desired = @($snap.plan.entries | ForEach-Object { ($_.rect -join ",") } | Sort-Object)
        $bad = @($snap.detail.entries | Where-Object { $_.matched -ne $true })
        $matched = @($snap.detail.entries | Where-Object { $_.matched -eq $true } | ForEach-Object { ($_.readback -join ",") } | Sort-Object)
        $seen = Get-VisibleKeys $insp
        if ($desired.Count -gt 0 -and $bad.Count -eq 0 -and (($desired -join "|") -eq ($matched -join "|")) -and (($matched -join "|") -eq ($seen -join "|"))) {
          $gm = Test-GapMath $Domain (Get-EligibleRects $insp)
          if ($gm.ok) {
            $dpi = ,@($insp.windows | Where-Object { $_.eligible -eq $true } | ForEach-Object { [int]$_.dpi } | Sort-Object -Unique)
            return @{ inspect = $insp; tick = $snap.tick; desired = $desired; readback = $matched; dpi = $dpi }
          }
          $last = "tick=$($snap.tick) gap: $($gm.failures -join '; ')"
        } else {
          $last = "tick=$($snap.tick) desired=[$($desired -join '|')] readback=[$($matched -join '|')] seen=[$($seen -join '|')] unmatched=$($bad.Count)"
        }
      } else {
        $last = "no plan+readback-detail tick yet"
      }
    } else {
      $last = "eligible=[$($got -join '|')] want=[$($want -join '|')]"
    }
    Start-Sleep -Milliseconds 500
  }
  throw "converge timeout hwnds $($want -join ','): $last"
}
function Assert-ProofDpi($Inspect, [string]$Tag) {
  foreach ($w in $Inspect.windows) {
    if ($w.eligible -eq $true -and [int]$w.dpi -ne $EXPECTED_DPI) {
      throw "$Tag dpi=$([int]$w.dpi) hwnd=$($w.hwnd), want $EXPECTED_DPI"
    }
  }
}
function Get-LogTail([string]$LogPath, [int]$FromLine) {
  $lines = Get-CompleteLines $LogPath
  if ($FromLine -ge $lines.Count) { return @{ events = @(); count = $lines.Count } }
  $out = @()
  for ($i = $FromLine; $i -lt $lines.Count; $i++) {
    if ("$($lines[$i])".Trim() -ne "") {
      try { $out += ($lines[$i] | ConvertFrom-Json) }
      catch { throw "malformed JSON $LogPath line $($i + 1): $($_.Exception.Message)" }
    }
  }
  return @{ events = $out; count = $lines.Count }
}
function Get-LogLineCount([string]$LogPath) {
  return (Get-CompleteLines $LogPath).Count
}
function Wait-ObserveTick([string]$LogPath, [int]$TimeoutSec, [string]$Tag) {
  # Observer progress: at least one completed `observe` tick in the log.
  # A torn tail never counts (Get-CompleteLines defers it); malformed records
  # throw instead of passing silently.
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    foreach ($e in (Read-JsonLines $LogPath)) {
      if ($e.event -eq "observe") { return $e }
    }
    Start-Sleep -Milliseconds 500
  }
  throw "$Tag no completed observe tick within ${TimeoutSec}s (see $LogPath)"
}
function Wait-ObserveOrSuspendBlocked([string]$LogPath, [int]$TimeoutSec, [string]$Tag) {
  # Persistent read-only watch over the startup record: returns the first
  # completed `observe` tick, but throws blocked as soon as a fullscreen
  # suspension persists with no observation possible, instead of waiting out
  # the full timeout. A suspend followed by resume (or any observe) never
  # blocks: only a suspension still in force after repeated polls throws.
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  $suspendedPolls = 0
  while ((Get-Date) -lt $deadline) {
    $suspended = $false
    foreach ($e in (Read-JsonLines $LogPath)) {
      if ($e.event -eq "observe") { return $e }
      elseif ($e.event -eq "suspend") { $suspended = $true }
      elseif ($e.event -eq "resume") { $suspended = $false }
    }
    if ($suspended) { $suspendedPolls++ } else { $suspendedPolls = 0 }
    if ($suspendedPolls -ge 4) { throw "$Tag blocked: fullscreen suspension prevented observation" }
    Start-Sleep -Milliseconds 500
  }
  Assert-NotSuspendBlocked $LogPath $Tag
  throw "$Tag no completed observe tick within ${TimeoutSec}s (see $LogPath)"
}
function Get-ProofWriteCount([string]$AuditPath) {
  $n = 0
  foreach ($e in (Read-JsonLines $AuditPath)) {
    if ($e.event -eq "proof-write") { $n++ }
  }
  return $n
}
function Assert-ProofStartArgv([string]$AuditPath, [string[]]$WantFlags, [string]$Tag) {
  # Delivery evidence: the native proof-start must carry every expected flag
  # in its raw argv array (elements stay separate strings, spaces intact).
  $starts = @(Read-JsonLines $AuditPath | Where-Object { $_.event -eq "proof-start" })
  if ($starts.Count -eq 0) { throw "$Tag proof-start missing in $AuditPath" }
  $argv = @($starts[0].argv)
  foreach ($flag in $WantFlags) {
    if (-not ($argv -contains $flag)) { throw "$Tag proof-start argv missing $flag" }
  }
}
function Assert-NotSuspendBlocked([string]$LogPath, [string]$Tag) {
  # A wait timeout with an unresumed fullscreen suspension means the owner
  # never observed: report blocked before mutation, never a nonmoving pass.
  $suspended = $false
  foreach ($e in (Read-JsonLines $LogPath)) {
    if ($e.event -eq "suspend") { $suspended = $true }
    elseif ($e.event -eq "resume") { $suspended = $false }
  }
  if ($suspended) { throw "$Tag blocked: fullscreen suspension prevented observation" }
}

$ownerRunning = $false
$ownerPayload = ""
$ownerPid = 0
$ownerCreation = ""
$invokedArgs = [System.Collections.ArrayList]@()
$createdHelpers = [System.Collections.ArrayList]@()
function Note-Arg([string]$Text) { $null = $invokedArgs.Add($Text) }
function Restore-DeadEmptyLease([string]$Payload, [string]$Tag) {
  # A starting owner can commit an EMPTY lease then die before readiness
  # (e.g. stale-allowlist refusal after commit): `ready` stays false and
  # recovery sees no owner, orphaning the lease and blocking every later run.
  # Reclaim exactly that case via the existing `restore` (native caller-owns
  # plus dead-owner checks stay authoritative): committed owner dead, zero
  # windows, same copied exe, same SID/session as this caller. Anything else
  # (live owner, foreign exe, other session, non-empty lease, unreadable
  # record) is left alone. Returns a note string, or "" when not applied.
  try {
    $ident = Invoke-Native $Payload @("identity") | ConvertFrom-Json
  } catch {
    return ""
  }
  $ledgerFile = Join-Path "$($ident.ledger_directory)" "ledger.json"
  if (-not (Test-Path -LiteralPath $ledgerFile -PathType Leaf)) { return "" }
  try {
    $record = Get-Content -LiteralPath $ledgerFile -Raw | ConvertFrom-Json
  } catch {
    return ""
  }
  $owner = $record.owner
  if ($null -eq $owner) { return "" }
  if ($null -eq $record.windows) { return "" }
  try {
    if (@($record.windows).Count -ne 0) { return "" }
    if (-not (Test-ExeEqual "$($owner.exe_path)" "$Payload")) { return "" }
    if ("$($owner.user_sid)" -cne "$($ident.process.user_sid)") { return "" }
    if ([uint32]$owner.session_id -ne [uint32]$ident.process.session_id) { return "" }
    if (Test-ProcessAliveSameCreation ([int]$owner.pid) "$($owner.process_creation)") { return "" }
  } catch {
    return ""
  }
  try {
    $r = Invoke-Native $Payload @("restore") | ConvertFrom-Json
  } catch {
    return "$Tag dead-lease restore failed: $($_.Exception.Message)"
  }
  if (-not $r.restored) { return "$Tag dead-lease restore not restored" }
  return "$Tag dead-lease reclaimed: owner pid=$($owner.pid) creation=$($owner.process_creation) windows=0"
}
function Start-ProofOwner([string]$Payload, [string]$OwnerArguments, [string]$WorkDir, [string]$Tag) {
  Note-Arg "tile-proof $OwnerArguments"
  Start-ExplorerGui $Payload "tile-proof $OwnerArguments" $WorkDir
  try {
    $ready = Assert-OwnerReady $Payload $Tag
  } catch {
    # The owner may still run despite the readiness failure: probe once for
    # a discoverable same-artifact owner so recovery stays exact. If nothing
    # is discoverable the failure stands with owner state unknown.
    $found = $null
    try {
      $probe = Invoke-Native $Payload @("ready") | ConvertFrom-Json
      if ($probe.ready -eq $true) { $found = $probe }
    } catch {}
    if ($found -ne $null) {
      $script:ownerRunning = $true
      $script:ownerPayload = $Payload
      $script:ownerPid = [int]$found.owner.pid
      $script:ownerCreation = "$($found.owner.process_creation)"
    } else {
      # No live owner, yet a dead exact owner may hold an empty lease from
      # this same copied payload (commit-then-die before readiness). Reclaim
      # only that case; the original readiness failure still stands.
      $reclaim = Restore-DeadEmptyLease $Payload "$Tag-readiness"
      if ($reclaim -ne "") { Rec "readiness-reclaim" @{ note = $reclaim } }
    }
    throw
  }
  $script:ownerRunning = $true
  $script:ownerPayload = $Payload
  $script:ownerPid = [int]$ready.owner.pid
  $script:ownerCreation = "$($ready.owner.process_creation)"
  return $ready
}
function Stop-ProofOwner([string]$Payload, [bool]$Force, [string]$Tag) {
  $st = Stop-OwnerVerified $Payload $Force $Tag
  $alive = Test-ProcessAliveSameCreation $script:ownerPid $script:ownerCreation
  if ($alive) { throw "$Tag owner alive after stop" }
  $r = Invoke-Native $Payload @("restore") | ConvertFrom-Json
  if (-not $r.restored) { throw "$Tag restore failed" }
  Assert-LedgerClean $Payload
  $script:ownerRunning = $false
  return @{ stop = $st; restore = $r }
}
function Recover-Owner([string]$Tag) {
  if (-not $ownerRunning -or $ownerPayload -eq "") { return "no-owner" }
  # Exact-owner recovery only: graceful stop first, exact emergency only if
  # the verified owner is still alive, then standalone restore. No blanket
  # cleanup, no retries.
  $notes = @()
  try {
    $s = Invoke-Native $ownerPayload @("stop") | ConvertFrom-Json
    $notes += "stop exited=$($s.owner_exited)"
  } catch {
    $notes += "stop failed: $($_.Exception.Message)"
  }
  try {
    if (Test-ProcessAliveSameCreation $script:ownerPid $script:ownerCreation) {
      try {
        $e = Invoke-Native $ownerPayload @("emergency-stop") | ConvertFrom-Json
        $notes += "emergency exited=$($e.owner_exited)"
      } catch {
        $notes += "emergency failed: $($_.Exception.Message)"
      }
    } else {
      $notes += "owner gone after stop"
    }
  } catch {
    $notes += "alive check failed: $($_.Exception.Message)"
  }
  try {
    $r = Invoke-Native $ownerPayload @("restore") | ConvertFrom-Json
    $notes += "restore=$($r.restored)"
  } catch {
    $notes += "restore failed: $($_.Exception.Message)"
  }
  try {
    Assert-LedgerClean $ownerPayload
    $notes += "ledger clean"
  } catch {
    $notes += "ledger dirty: $($_.Exception.Message)"
  }
  $script:ownerRunning = $false
  return "recovered-${Tag}: $($notes -join '; ')"
}

function New-ProofRunDir {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $dir = Join-Path $Repo "target\windows-tiling\$stamp-$PID"
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  return $dir
}
# Every proof-relevant source file: git diff misses untracked files, so the
# receipt binds explicit per-file hashes plus one combined source hash.
$SOURCE_FILES = @(
  "crates/tiler-windows/src/tiling.rs",
  "crates/tiler-windows/src/tiling_sys/mod.rs",
  "crates/tiler-windows/src/test_window.rs",
  "crates/tiler-windows/src/lifecycle.rs",
  "crates/tiler-windows/src/main.rs",
  "crates/tiler-windows/src/lib.rs",
  "crates/tiler-windows/src/bin/tiler-test-window.rs",
  "crates/tiler-windows/tests/tiling.rs",
  "crates/tiler-windows/tests/test_window_args.rs",
  "crates/tiler-windows/Cargo.toml",
  "scripts/windows-tiling.ps1",
  "scripts/windows-dev.ps1",
  "justfile",
  "just/windows.just"
)
function Get-SourceHashes {
  $rows = @()
  foreach ($rel in $SOURCE_FILES) {
    $full = Join-Path $Repo $rel
    if (-not (Test-Path -LiteralPath $full -PathType Leaf)) { throw "source missing: $rel" }
    $rows += "$rel=$((Get-FileHash -LiteralPath $full -Algorithm SHA256).Hash)"
  }
  return @{ files = $rows; combined = (Sha256-String ($rows -join "`n")) }
}
function Get-Provenance([string]$ProofDir, [string]$OwnerCopy, [string]$HelperCopy) {
  $commit = (& git -C $Repo rev-parse HEAD | Out-String).Trim()
  $status = (& git -C $Repo status --short | Out-String).Trim()
  $diffText = (& git -C $Repo diff HEAD | Out-String)
  $diffHash = "clean"
  if ("$diffText".Trim() -ne "") { $diffHash = Sha256-String $diffText }
  $os = Get-CimInstance Win32_OperatingSystem
  return @{
    commit        = $commit
    status        = $status
    diff_hash     = $diffHash
    source        = (Get-SourceHashes)
    actor_pid     = $PID
    actor_exe     = (Get-Process -Id $PID).Path
    os_caption    = "$($os.Caption)"
    os_version    = "$($os.Version)"
    os_build      = "$($os.BuildNumber)"
    owner_path    = $OwnerCopy
    owner_sha256  = (Get-FileHash -LiteralPath $OwnerCopy -Algorithm SHA256).Hash
    helper_path   = $HelperCopy
    helper_sha256 = (Get-FileHash -LiteralPath $HelperCopy -Algorithm SHA256).Hash
    run_dir       = $ProofDir
  }
}
function Test-NoProofActors([string]$OwnerCopy, [string]$HelperCopy, [string]$Tag) {
  # Read-only preflight: refuse when proof payloads already run. Never kills.
  $hits = @(Get-Process -Name "tiler-windows", "tiler-test-window" -ErrorAction SilentlyContinue | Where-Object {
      $p = ""
      try { $p = $_.Path } catch {}
      ($p -ieq $OwnerCopy) -or ($p -ieq $HelperCopy)
    })
  if ($hits.Count -ne 0) { throw "$Tag preflight refused: proof actors already running" }
}

function Start-PassiveHelper([string]$HelperBin, [string]$ProofDir, [string]$Name) {
  $receipt = Join-Path $ProofDir "$Name.json"
  if (Test-Path $receipt) { throw "receipt preexists $receipt" }
  $argStr = "run --receipt `"$receipt`" --seconds 600 --passive"
  Note-Arg "helper $argStr"
  Start-ExplorerGui $HelperBin $argStr $ProofDir
  $deadline = (Get-Date).AddSeconds(10)
  while (-not (Test-Path $receipt)) {
    if ((Get-Date) -gt $deadline) { throw "helper receipt timeout $Name" }
    Start-Sleep -Milliseconds 200
  }
  $snap = Get-Content -LiteralPath $receipt -Raw | ConvertFrom-Json
  if (-not (Test-ExeEqual "$($snap.process.exe_path)" "$HelperBin")) { throw "$Name helper peer mismatch" }
  Assert-ParentIsExplorer ([int]$snap.process.pid)
  if ([string]$snap.tag -eq "") { throw "$Name helper missing tag" }
  if ($snap.visible -ne $false) { throw "$Name passive helper visible at create" }
  $null = $createdHelpers.Add(@{
      hwnd = [uint64]$snap.hwnd; tag = "$($snap.tag)"
      pid = [int]$snap.process.pid; creation = "$($snap.process.process_creation)"
    })
  return $snap
}
function Build-Allowlist([string]$Path, [array]$Snaps) {
  $entries = @()
  foreach ($s in $Snaps) {
    if ([string]$s.tag -eq "") { throw "allowlist refused: helper missing lifetime tag" }
    $entries += @{
      hwnd             = [uint64]$s.hwnd
      pid              = [uint32]$s.process.pid
      process_creation = "$($s.process.process_creation)"
      exe_path         = "$($s.process.exe_path)"
      user_sid         = "$($s.process.user_sid)"
      session_id       = [uint32]$s.process.session_id
      tag              = "$($s.tag)"
    }
  }
  @{ windows = $entries } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $Path
}
function Build-SurvivorsAllowlist([string]$OutPath, [string]$SourcePath, [array]$KeepHwnds) {
  # Emergency restart after an exact close cannot reuse the original frozen
  # allowlist: it still names the deliberately closed helper, which the
  # strict proof gate rightly refuses. Derive a survivors-only file from the
  # ORIGINAL frozen entries (exact same identities/tags, no re-enumeration,
  # no new capture, no footprint expansion): keep exactly the two given HWNDs
  # as an exact unique subset. Anything else is a refusal, never a guess.
  if (Test-Path -LiteralPath $OutPath) { throw "survivors allowlist preexists $OutPath, will not overwrite" }
  $keep = @($KeepHwnds | ForEach-Object { [uint64]$_ } | Sort-Object -Unique)
  if ($keep.Count -ne 2) { throw "survivors allowlist refused: keep set must be exactly 2 unique HWNDs" }
  $source = Get-Content -LiteralPath $SourcePath -Raw | ConvertFrom-Json
  $picked = @($source.windows | Where-Object { $keep -contains [uint64]$_.hwnd })
  if ($picked.Count -ne 2) { throw "survivors allowlist refused: kept $($picked.Count), need exactly 2" }
  if ((@($picked | ForEach-Object { [uint64]$_.hwnd } | Sort-Object -Unique)).Count -ne 2) {
    throw "survivors allowlist refused: duplicate HWND in source"
  }
  foreach ($e in $picked) {
    if ([uint64]$e.hwnd -eq 0 -or [uint32]$e.pid -eq 0 -or [string]$e.process_creation -eq "" -or
        [string]$e.exe_path -eq "" -or [string]$e.user_sid -eq "" -or [string]$e.tag -eq "") {
      throw "survivors allowlist refused: frozen entry incomplete hwnd=$($e.hwnd)"
    }
  }
  @{ windows = $picked } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $OutPath
}
function Test-HelperGone($Snap, [string]$Name) {
  $deadline = (Get-Date).AddSeconds(10)
  while ($true) {
    $held = Test-ProcessAliveSameCreation ([int]$Snap.process.pid) "$($Snap.process.process_creation)"
    if (-not $held) { break }
    if ((Get-Date) -gt $deadline) { throw "$Name helper exit timeout" }
    Start-Sleep -Milliseconds 200
  }
}

function Invoke-OwnedStage {
  $stageStart = (Get-Date).ToString("o")
  $proofDir = New-ProofRunDir
  $ownedReport = Join-Path $proofDir "owned-report.json"
  if (Test-Path $ownedReport) { throw "Owned refused: report preexists, will not overwrite" }
  $binDir = Join-Path $proofDir "bin"
  New-Item -ItemType Directory -Force -Path $binDir | Out-Null
  # Copied payloads avoid image locks on target/debug during the proof.
  & cargo build --locked --manifest-path (Join-Path $Repo "Cargo.toml") -p tiler-windows
  if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
  $ownerCopy = Join-Path $binDir "tiler-windows.exe"
  $helperCopy = Join-Path $binDir "tiler-test-window.exe"
  Copy-Item (Join-Path $Repo "target\debug\tiler-windows.exe") $ownerCopy -Force
  Copy-Item (Join-Path $Repo "target\debug\tiler-test-window.exe") $helperCopy -Force
  $prov = Get-Provenance $proofDir $ownerCopy $helperCopy
  Rec "env" $prov
  $ident = (Invoke-Native $ownerCopy @("identity") | ConvertFrom-Json)
  Rec "identity" @{
    session_id = "$($ident.process.session_id)"
    user_sid   = "$($ident.process.user_sid)"
    il         = "$($ident.integrity_level)"
    ledger     = "$($ident.ledger_directory)"
  }
  # Read-only preflight before any helper exists: ledger/request absent and
  # no project actors running. Refuses; never cleans up.
  Assert-LedgerClean $ownerCopy
  Test-NoProofActors $ownerCopy $helperCopy "owned"
  Rec "preflight" @{ ledger = "clean"; actors = "absent" }

  try {
    # Passive helpers first: frozen exact ids exist before any owner runs,
    # and the nonmoving stop/emergency checks below stay in test mode.
    $h1 = Start-PassiveHelper $helperCopy $proofDir "helper1"
    $h2 = Start-PassiveHelper $helperCopy $proofDir "helper2"
    $h3 = Start-PassiveHelper $helperCopy $proofDir "helper3"
    Rec "helpers-created" @(@($h1, $h2, $h3) | ForEach-Object {
        @{ hwnd = $_.hwnd; pid = $_.process.pid; tag = $_.tag; visible = $_.visible }
      })
    $allowPath = Join-Path $proofDir "allowlist.json"
    Build-Allowlist $allowPath @($h1, $h2, $h3)

    # Nonmoving untimed proof owner: graceful stop verified before any
    # mutation. Proofmode plus observer progress are required: the native
    # `tile-start` scope must read proof/trace, at least one completed
    # `observe` tick (EnumWindows success with enumerated/managed counts, even
    # 0 managed) must land in the log, and the proof audit must show ZERO
    # geometry calls. A layout plan is not required: hidden helpers leave the
    # Engine domain empty (Rejected). Global fullscreen suspension with no
    # observation reports blocked instead of passing.
    $ready = Start-ProofOwner $ownerCopy "--allowlist `"$allowPath`" --trace" $binDir "owned-nohide-grace"
    $logPath = "$($ready.log_path)"
    Rec "owner-nohide-log" @{ log_path = $logPath }
    $start = $null
    foreach ($e in (Read-JsonLines $logPath)) { if ($e.event -eq "tile-start") { $start = $e } }
    if (-not $start) { throw "tile-start missing" }
    if ("$($start.mode)" -ne "proof") { throw "owner not in proof mode" }
    if (-not $start.trace) { throw "owner trace not active" }
    $ledgerDir = "$($ident.ledger_directory)"
    $auditPath = Join-Path $ledgerDir "proof-audit-$($ready.owner.process_creation).jsonl"
    if (-not (Test-Path -LiteralPath $auditPath)) { throw "proof audit missing $auditPath" }
    Assert-ProofStartArgv $auditPath @("--allowlist", "--trace") "owned-nohide-grace"
    $obs = Wait-ObserveOrSuspendBlocked $logPath 15 "owned-nohide-grace"
    $st = Stop-ProofOwner $ownerCopy $false "owned-nohide-grace"
    $calls = Get-ProofWriteCount $auditPath
    if ($calls -ne 0) { throw "nonmoving owner made $calls geometry calls (see $auditPath)" }
    Rec "owner-nohide-ready" @{ owner = $ready.owner; log = $logPath; audit = $auditPath; observed = $obs; geometry_calls = 0; mode = "$($start.mode)"; trace = $start.trace }
    Rec "owner-stop-grace" $st

    # Nonmoving untimed owner: emergency path with the same receipt, observer,
    # and zero-geometry checks before any mutation. Per-run logs preserved.
    $ready = Start-ProofOwner $ownerCopy "--allowlist `"$allowPath`" --trace" $binDir "owned-nohide-forced"
    $forcedLog = "$($ready.log_path)"
    Rec "owner-nohide-forced-log" @{ log_path = $forcedLog }
    $forcedStart = $null
    foreach ($e in (Read-JsonLines $forcedLog)) { if ($e.event -eq "tile-start") { $forcedStart = $e } }
    if (-not $forcedStart) { throw "forced tile-start missing" }
    if ("$($forcedStart.mode)" -ne "proof") { throw "forced owner not in proof mode" }
    if (-not $forcedStart.trace) { throw "forced owner trace not active" }
    $forcedAudit = Join-Path $ledgerDir "proof-audit-$($ready.owner.process_creation).jsonl"
    if (-not (Test-Path -LiteralPath $forcedAudit)) { throw "forced proof audit missing $forcedAudit" }
    Assert-ProofStartArgv $forcedAudit @("--allowlist", "--trace") "owned-nohide-forced"
    $forcedObs = Wait-ObserveOrSuspendBlocked $forcedLog 15 "owned-nohide-forced"
    $st = Stop-ProofOwner $ownerCopy $true "owned-nohide-forced"
    $forcedCalls = Get-ProofWriteCount $forcedAudit
    if ($forcedCalls -ne 0) { throw "nonmoving forced owner made $forcedCalls geometry calls (see $forcedAudit)" }
    Rec "owner-stop-forced" @{ stop = $st; log = $forcedLog; audit = $forcedAudit; observed = $forcedObs; geometry_calls = 0 }

    # Mutation phase owner: bounded proof-mode tiling with trace for
    # requested-frame evidence. Production logs stay token-only; raw native
    # ids live only in the scoped proof audit beside the ledger.
    $ready = Start-ProofOwner $ownerCopy "--allowlist `"$allowPath`" --seconds 300 --trace" $binDir "owned-tile"
    $logPath = "$($ready.log_path)"
    Rec "owner-log" @{ log_path = $logPath }
    $mark = (Get-LogLineCount $logPath)
    $start = $null
    foreach ($e in (Read-JsonLines $logPath)) { if ($e.event -eq "tile-start") { $start = $e } }
    if (-not $start) { throw "tile-start missing" }
    if ("$($start.mode)" -ne "proof") { throw "owner not in proof mode" }
    if (-not $start.trace) { throw "owner trace not active" }
    $work = Rect-FromArray $start.work
    $full = Rect-FromArray $start.full
    $domain = @{ x = $work.x + $OUTER_GAP; y = $work.y + $OUTER_GAP; w = $work.w - 2 * $OUTER_GAP; h = $work.h - 2 * $OUTER_GAP }
    # Proof audit (scoped, beside the ledger): startup args/mode/digest/count
    # plus frozen verified identities exist before the first admission. Zero
    # non-owned writes are provable from per-setter requested/target/flags.
    $ledgerDir = "$($ident.ledger_directory)"
    $auditPath = Join-Path $ledgerDir "proof-audit-$($ready.owner.process_creation).jsonl"
    if (-not (Test-Path -LiteralPath $auditPath)) { throw "proof audit missing $auditPath" }
    Assert-ProofStartArgv $auditPath @("--allowlist", "--seconds", "--trace") "owned-tile"
    Rec "proof-audit" @{ path = $auditPath }
    Rec "tile-start" @{
      mode           = "$($start.mode)"
      trace          = $start.trace
      monitors       = $start.monitors
      work           = Rect-Key $work
      full           = Rect-Key $full
      domain         = Rect-Key $domain
      dpi_awareness  = "PerMonitorV2 (owner fail-closed)"
      inner_gap      = $start.inner
      outer_gap      = $start.outer
      allowlist      = (Get-Content -LiteralPath $allowPath -Raw | ConvertFrom-Json)
    }

    # Admission 1: first-time show without activation or z-order change.
    $mark = (Get-LogLineCount $logPath)
    $shown = Invoke-Native $helperCopy @("show", "$($h1.hwnd)", "--tag", "$($h1.tag)") | ConvertFrom-Json
    Note-Arg "helper show $($h1.hwnd)"
    if ([uint64]$shown.foreground -eq [uint64]$h1.hwnd) { throw "admission focused helper1" }
    $conv = Wait-Converged $ownerCopy $allowPath @($h1.hwnd) $logPath $mark $domain 20
    Assert-ProofDpi $conv.inspect "admit-1"
    if ([uint64]$conv.inspect.foreground -eq [uint64]$h1.hwnd) { throw "tiling focused helper1" }
    Rec "admit-1" @{ scope = @([uint64]$h1.hwnd); tick = $conv.tick; visible = (Get-VisibleKeys $conv.inspect); desired = $conv.desired; dpi = $conv.dpi; foreground = $conv.inspect.foreground }

    # Admission 2: two-tile margins plus inner gap, full plan read back.
    $mark = (Get-LogLineCount $logPath)
    $shown = Invoke-Native $helperCopy @("show", "$($h2.hwnd)", "--tag", "$($h2.tag)") | ConvertFrom-Json
    Note-Arg "helper show $($h2.hwnd)"
    if ([uint64]$shown.foreground -eq [uint64]$h2.hwnd) { throw "admission focused helper2" }
    $conv = Wait-Converged $ownerCopy $allowPath @($h1.hwnd, $h2.hwnd) $logPath $mark $domain 20
    Assert-ProofDpi $conv.inspect "admit-2"
    Rec "admit-2" @{ scope = @([uint64]$h1.hwnd, [uint64]$h2.hwnd); tick = $conv.tick; visible = (Get-VisibleKeys $conv.inspect); desired = $conv.desired; dpi = $conv.dpi; foreground = $conv.inspect.foreground }

    # Admission 3: three-tile layout keeps every margin and gap at 8.
    $mark = (Get-LogLineCount $logPath)
    $shown = Invoke-Native $helperCopy @("show", "$($h3.hwnd)", "--tag", "$($h3.tag)") | ConvertFrom-Json
    Note-Arg "helper show $($h3.hwnd)"
    if ([uint64]$shown.foreground -eq [uint64]$h3.hwnd) { throw "admission focused helper3" }
    $conv = Wait-Converged $ownerCopy $allowPath @($h1.hwnd, $h2.hwnd, $h3.hwnd) $logPath $mark $domain 20
    Assert-ProofDpi $conv.inspect "admit-3"
    Rec "admit-3" @{ scope = @([uint64]$h1.hwnd, [uint64]$h2.hwnd, [uint64]$h3.hwnd); tick = $conv.tick; visible = (Get-VisibleKeys $conv.inspect); desired = $conv.desired; dpi = $conv.dpi; foreground = $conv.inspect.foreground }

    # Minimize without focus: exact-process/tag bound, foreground untouched.
    $mark = (Get-LogLineCount $logPath)
    $min = Invoke-Native $helperCopy @("minimize", "$($h2.hwnd)", "--tag", "$($h2.tag)") | ConvertFrom-Json
    Note-Arg "helper minimize $($h2.hwnd)"
    if ([uint64]$min.foreground -eq [uint64]$h2.hwnd) { throw "minimize focused helper2" }
    $conv = Wait-Converged $ownerCopy $allowPath @($h1.hwnd, $h3.hwnd) $logPath $mark $domain 20
    Assert-ProofDpi $conv.inspect "minimize"
    $h2entry = @($conv.inspect.windows | Where-Object { [uint64]$_.hwnd -eq [uint64]$h2.hwnd })[0]
    Rec "minimize-inspect" @{ min_command = $min; h2entry = $h2entry; windows = @($conv.inspect.windows); foreground = $conv.inspect.foreground }
    if ($h2entry.eligible -ne $false -or $h2entry.skip -ne "minimized" -or $h2entry.identity_match -ne $true) {
      throw "minimize not observed via enumeration"
    }
    if ([uint64]$conv.inspect.foreground -eq [uint64]$h2.hwnd) { throw "helper2 foreground after minimize" }
    Rec "minimize" @{ scope = @([uint64]$h1.hwnd, [uint64]$h3.hwnd); tick = $conv.tick; helper2_skip = $h2entry.skip; visible = (Get-VisibleKeys $conv.inspect); desired = $conv.desired; dpi = $conv.dpi; foreground = $conv.inspect.foreground }

    # Restore without focus, then exact close of helper3.
    $mark = (Get-LogLineCount $logPath)
    $rst = Invoke-Native $helperCopy @("restore", "$($h2.hwnd)", "--tag", "$($h2.tag)") | ConvertFrom-Json
    Note-Arg "helper restore $($h2.hwnd)"
    if ([uint64]$rst.foreground -eq [uint64]$h2.hwnd) { throw "restore focused helper2" }
    $conv = Wait-Converged $ownerCopy $allowPath @($h1.hwnd, $h2.hwnd, $h3.hwnd) $logPath $mark $domain 20
    Assert-ProofDpi $conv.inspect "helper-restore"
    Rec "helper-restore" @{ scope = @([uint64]$h1.hwnd, [uint64]$h2.hwnd, [uint64]$h3.hwnd); tick = $conv.tick; visible = (Get-VisibleKeys $conv.inspect); desired = $conv.desired; dpi = $conv.dpi; foreground = $conv.inspect.foreground }
    $closed = Invoke-Native $helperCopy @("close", "$($h3.hwnd)", "--tag", "$($h3.tag)") | ConvertFrom-Json
    Note-Arg "helper close $($h3.hwnd)"
    Test-HelperGone $h3 "helper3"
    # Survivors must re-settle with full desired/readback/gap evidence after
    # the close (no extra window mutations); then the closed HWND must no
    # longer match any frozen identity.
    $mark = (Get-LogLineCount $logPath)
    $conv = Wait-Converged $ownerCopy $allowPath @($h1.hwnd, $h2.hwnd) $logPath $mark $domain 20
    Assert-ProofDpi $conv.inspect "close-survivors"
    $insp = Get-Inspect $ownerCopy $allowPath
    $h3entry = @($insp.windows | Where-Object { [uint64]$_.hwnd -eq [uint64]$h3.hwnd })[0]
    if ($h3entry.identity_match -ne $false) { throw "closed helper3 still matches" }
    Rec "helper-close" @{ closed = $closed; helper3_match = $h3entry.identity_match; survivors_tick = $conv.tick; survivors = (Get-IdentityFrames $conv.inspect); visible = (Get-VisibleKeys $conv.inspect); desired = $conv.desired; dpi = $conv.dpi; foreground = $conv.inspect.foreground }

    # Graceful stop keeps each surviving window's identity-bound outer+visible
    # frames identical: per-HWND comparison, never a bare sorted rect set.
    # The pre baseline is captured immediately before the stop.
    $preGrace = Get-Inspect $ownerCopy $allowPath
    $st = Stop-ProofOwner $ownerCopy $false "owned-tile-grace"
    $postGrace = Get-Inspect $ownerCopy $allowPath
    Assert-SameIdentityFrames $preGrace $postGrace "graceful stop"
    Rec "stop-grace-kept" @{ before = (Get-IdentityFrames $preGrace); after = (Get-IdentityFrames $postGrace); stop = $st.stop }

    # Emergency owner loss keeps each surviving window's identity-bound frames
    # identical. The pre baseline is captured after the restart settles and
    # immediately before the kill: restart geometry is recorded but never
    # treated as stop evidence. The restart uses a survivors-only allowlist
    # derived from the original frozen entries (h3 stays closed): the full
    # original list is kept for grace/closed-h3 evidence only.
    $survivorsPath = Join-Path $proofDir "survivors-allowlist.json"
    Build-SurvivorsAllowlist $survivorsPath $allowPath @($h1.hwnd, $h2.hwnd)
    $forcedArgs = "--allowlist `"$survivorsPath`" --seconds 300 --trace"
    Rec "survivors-allowlist" @{
      path = $survivorsPath; source = $allowPath
      kept = @([uint64]$h1.hwnd, [uint64]$h2.hwnd); count = 2
      sha256 = (Get-FileHash -LiteralPath $survivorsPath -Algorithm SHA256).Hash
      entries = (Get-Content -LiteralPath $survivorsPath -Raw | ConvertFrom-Json)
      args = $forcedArgs
    }
    $ready = Start-ProofOwner $ownerCopy $forcedArgs $binDir "owned-tile-forced"
    $forcedLog = "$($ready.log_path)"
    Rec "owner-forced-log" @{ log_path = $forcedLog }
    $mark = (Get-LogLineCount $forcedLog)
    $forcedConv = Wait-Converged $ownerCopy $survivorsPath @($h1.hwnd, $h2.hwnd) $forcedLog $mark $domain 20
    Assert-ProofDpi $forcedConv.inspect "owned-tile-forced"
    Rec "forced-settled" @{ tick = $forcedConv.tick; visible = (Get-VisibleKeys $forcedConv.inspect); desired = $forcedConv.desired; dpi = $forcedConv.dpi; foreground = $forcedConv.inspect.foreground }
    $preForced = Get-Inspect $ownerCopy $survivorsPath
    $st = Stop-ProofOwner $ownerCopy $true "owned-tile-forced"
    $postForced = Get-Inspect $ownerCopy $survivorsPath
    Assert-SameIdentityFrames $preForced $postForced "emergency stop"
    Rec "stop-forced-kept" @{ before = (Get-IdentityFrames $preForced); after = (Get-IdentityFrames $postForced); stop = $st.stop }

    # Exact-helper cleanup: processes gone, no actors, ledger absent.
    $c1 = Invoke-Native $helperCopy @("close", "$($h1.hwnd)", "--tag", "$($h1.tag)") | ConvertFrom-Json
    $c2 = Invoke-Native $helperCopy @("close", "$($h2.hwnd)", "--tag", "$($h2.tag)") | ConvertFrom-Json
    Note-Arg "helper close $($h1.hwnd)"
    Note-Arg "helper close $($h2.hwnd)"
    Test-HelperGone $h1 "helper1"
    Test-HelperGone $h2 "helper2"
    Assert-LedgerClean $ownerCopy
    $leftovers = @(Get-Process -Name "tiler-test-window" -ErrorAction SilentlyContinue | Where-Object { $_.Path -ieq $helperCopy })
    if ($leftovers.Count -ne 0) { throw "helper actors remain" }
    Rec "cleanup" @{ closed = @($c1, $c2); ledger = "clean"; actors = "absent" }

    $report = @{ status = "pass"; stage = "Owned"; started = $stageStart; ended = (Get-Date).ToString("o"); provenance = $prov; args = $invokedArgs; steps = $steps }
    $report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $ownedReport
    Write-Output "owned-report=$ownedReport"
    Write-Output "status=pass"
  } catch {
    # Close only recorded exact helpers (best effort each); never broad cleanup.
    $closedHelpers = @()
    foreach ($h in $createdHelpers) {
      if (-not (Test-ProcessAliveSameCreation ([int]$h.pid) "$($h.creation)")) {
        # Known-absent helper (e.g. an exact close already proven earlier in
        # this run): log already-closed, not a failed cleanup. A live process
        # whose close fails still records closed=false with the native error,
        # so unknown-HWND reuse never passes silently.
        $closedHelpers += @{ hwnd = $h.hwnd; closed = "already-closed"; tag = $h.tag }
        continue
      }
      try {
        $c = Invoke-Native $helperCopy @("close", "$($h.hwnd)", "--tag", "$($h.tag)") | ConvertFrom-Json
        $closedHelpers += @{ hwnd = $h.hwnd; closed = $true; tag = $h.tag }
      } catch {
        $closedHelpers += @{ hwnd = $h.hwnd; closed = $false; error = "$($_.Exception.Message)" }
      }
    }
    $recovery = Recover-Owner "owned"
    $report = @{ status = "fail"; stage = "Owned"; started = $stageStart; ended = (Get-Date).ToString("o"); provenance = $prov; args = $invokedArgs; steps = $steps; error = "$($_.Exception.Message)"; closed_helpers = $closedHelpers; recovery = $recovery }
    $report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $ownedReport
    Write-Output "owned-report=$ownedReport"
    throw "Owned stage failed: $($_.Exception.Message)"
  }
}

Invoke-OwnedStage
