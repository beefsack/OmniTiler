param(
  [switch]$Mock,
  [switch]$Live,
  [switch]$Stop,
  [string]$RunDir = "",
  [int]$OwnerSeconds = 180
)
$ErrorActionPreference = "Stop"
$Repo = Split-Path -Parent $PSScriptRoot
. (Join-Path $Repo "scripts\windows-dev.ps1")
if ("$($MyInvocation.InvocationName)" -eq ".") { return }
# Lean reuse: load ONLY function definitions from the border harness via AST
# (never executes its top-level param/guard/live legs; the invocation guard
# there returns before functions load, so plain dot-sourcing cannot work).
$GuMock = $Mock; $GuLive = $Live; $GuStop = $Stop; $GuRunDir = $RunDir; $GuOwnerSeconds = $OwnerSeconds
$GuBorderPath = Join-Path $Repo "scripts\windows-active-border.ps1"
$AB_STEPS = [System.Collections.ArrayList]@()
$GuWanted = @("Rec-Ab","Fail-Ab","Install-BorderNative","Read-CompleteTextAb","Get-CompleteLinesAb","Get-MarkBeforeActionAb","Wait-BorderOutcome","Assert-HelperIdentityAb","Set-OwnedForegroundAb","Get-OverlayHwndsForOwnerAb","Assert-ExactOuterAb","Assert-ZOrderAb","Wait-ExactAndZAb","Get-LastShownColorAb","Get-SystemAccentAb","Get-ComposedRingSamplesAb","New-HeldProcessAb","Close-HeldProcessAb","Invoke-OwnedSysCommandAb","ConvertTo-AbsoluteAb","Get-NativeRectAb","Get-NativeFrameAb","Get-NativeOverlayRectAb","Find-TitlePointAb","Find-EdgePointAb","Get-VirtualScreenAb","Assert-ButtonReleasedAb","Invoke-ActiveGestureAb","Invoke-ActiveTitleDragAb","Invoke-ActiveEdgeResizeAb","Read-CorrectSpiAb")
$GuTokens = $null; $GuErrs = $null
$GuAst = [System.Management.Automation.Language.Parser]::ParseFile($GuBorderPath, [ref]$GuTokens, [ref]$GuErrs)
if ($GuErrs.Count -ne 0) { throw "border harness parse errors: $($GuErrs.Count)" }
$GuLoaded = @{}
foreach ($GuFn in $GuAst.FindAll({ $args[0] -is [System.Management.Automation.Language.FunctionDefinitionAst] }, $true)) {
  if ($GuWanted -contains $GuFn.Name) { Invoke-Expression $GuFn.Extent.Text; $GuLoaded[$GuFn.Name] = $true }
}
foreach ($GuNeed in $GuWanted) { if (-not $GuLoaded.ContainsKey($GuNeed)) { throw "border helper unavailable: $GuNeed" } }
$Mock = $GuMock; $Live = $GuLive; $Stop = $GuStop; $RunDir = $GuRunDir; $OwnerSeconds = $GuOwnerSeconds

$GU_STEPS = [System.Collections.ArrayList]@()
function Rec-Gu([string]$Name, $Data) { $null = $GU_STEPS.Add(@{ name = $Name; data = $Data }) }
function Fail-Gu([string]$Msg) { throw $Msg }

$VK_LWIN = 0x5B; $VK_RWIN = 0x5C; $VK_SHIFT = 0x10; $VK_LSHIFT = 0xA0; $VK_RSHIFT = 0xA1
$VK_CTRL = 0x11; $VK_ESC = 0x1B
$SC_MAXIMIZE = 0xF030; $SC_RESTORE = 0xF120

function Get-GuLines([string]$Path) { return (Get-CompleteLinesAb $Path) }
function Get-GuMark([string]$LogPath) { return (Get-CompleteLinesAb $LogPath).Count }

function Wait-UnderlayOutcome([string]$LogPath, [int]$Mark, [string[]]$Outcomes, [string]$Reason, [int]$TimeoutSec, [string]$Tag) {
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    $lines = Get-CompleteLinesAb $LogPath
    for ($i = $Mark; $i -lt $lines.Count; $i++) {
      if ("$($lines[$i])".Trim() -eq "") { continue }
      $e = $lines[$i] | ConvertFrom-Json
      if ("$($e.event)" -ne "group-underlay") { continue }
      if ($Outcomes -notcontains "$($e.outcome)") { continue }
      if (($Reason -ne "") -and ("$($e.reason)" -ne $Reason)) { continue }
      return @{ event = $e; count = $lines.Count }
    }
    Start-Sleep -Milliseconds 200
  }
  Fail-Gu "$Tag no group-underlay outcome=($($Outcomes -join '|')) reason=$Reason after mark $Mark"
  return $null
}

function Get-UnderlayHwnds([int]$OwnerPid, [string]$Class) {
  $found = [System.Collections.ArrayList]@()
  $cb = {
    param([IntPtr]$h, [IntPtr]$l)
    try {
      $pidOut = [uint32]0
      $null = [ActiveBorderNative]::GetWindowThreadProcessId($h, [ref]$pidOut)
      if ([uint32]$pidOut -eq [uint32]$script:guOwnerPid) {
        if ([ActiveBorderNative]::ClassOf($h.ToInt64()) -eq $script:guClass) {
          $null = $script:guFound.Add($h.ToInt64())
        }
      }
    } catch {}
    return $true
  }
  $script:guOwnerPid = $OwnerPid; $script:guClass = $Class; $script:guFound = $found
  $null = [ActiveBorderNative]::EnumWindows($cb, [IntPtr]::Zero)
  return ,$found
}

function Assert-UnionOuterGu([int[]]$OverlayRect, [int[]]$UnionXYWH, [int]$Pad, [string]$Tag) {
  # Overlay rect [x,y,w,h] must equal union expanded by Pad on every side.
  $ex = [int]$UnionXYWH[0] - $Pad; $ey = [int]$UnionXYWH[1] - $Pad
  $ew = [int]$UnionXYWH[2] + 2 * $Pad; $eh = [int]$UnionXYWH[3] + 2 * $Pad
  $got = "$($OverlayRect -join ',')"; $want = "$ex,$ey,$ew,$eh"
  if ($got -ne $want) { Fail-Gu "$Tag outer mismatch got=$got want=$want union=$($UnionXYWH -join ',') pad=$Pad" }
}

function Get-UnionOfFramesGu([long[]]$Hwnds) {
  $l = [int]::MaxValue; $t = [int]::MaxValue; $r = [int]::MinValue; $b = [int]::MinValue
  foreach ($h in $Hwnds) {
    $f = [ActiveBorderNative]::FrameOf([long]$h)
    if ($null -eq $f) { Fail-Gu "union DWM frame unreadable hwnd=$h" }
    if ([int]$f[0] -lt $l) { $l = [int]$f[0] }; if ([int]$f[1] -lt $t) { $t = [int]$f[1] }
    if ([int]$f[2] -gt $r) { $r = [int]$f[2] }; if ([int]$f[3] -gt $b) { $b = [int]$f[3] }
  }
  return @($l, $t, ($r - $l), ($b - $t))
}

function Assert-BelowAllGu([long]$UnderlayHwnd, [long[]]$Members, [string]$Tag) {
  [ActiveBorderNative]::EnsurePMv2()
  $order = [System.Collections.ArrayList]@()
  $cb = { param([IntPtr]$h, [IntPtr]$l) $null = $script:zOrd.Add($h.ToInt64()); return $true }
  $script:zOrd = $order
  $null = [ActiveBorderNative]::EnumWindows($cb, [IntPtr]::Zero)
  $idx = @{}
  for ($i = 0; $i -lt $order.Count; $i++) { if (-not $idx.ContainsKey([long]$order[$i])) { $idx[[long]$order[$i]] = $i } }
  if (-not $idx.ContainsKey($UnderlayHwnd)) { Fail-Gu "$Tag underlay absent from z-order" }
  foreach ($m in $Members) {
    if (-not $idx.ContainsKey([long]$m)) { Fail-Gu "$Tag member absent from z-order hwnd=$m" }
    if ([int]$idx[$UnderlayHwnd] -le [int]$idx[[long]$m]) { Fail-Gu "$Tag underlay not below member $m (ui=$($idx[$UnderlayHwnd]) mi=$($idx[[long]$m]))" }
  }
}

function Get-UnderlayExposedPointsGu([int[]]$OverlayRect, [long]$FocusedHwnd, [long]$OtherHwnd, [int]$BorderPx, [string]$Tag) {
  # Underlay-only proof points in the INTER-MEMBER gap strip between the two
  # live tiled frames: inside the union bbox (so always on-screen, no screen
  # edge or taskbar occlusion) yet covered by no member frame and no app
  # content. The focused side keeps BorderPx+1 clearance (yellow ring); the
  # unfocused side keeps 1px. DWM shadow stays calibrated by the hidden-vs-
  # shown baseline on the same points. Fails with frame evidence when the
  # live layout has no usable gap (touching/overlapping/fullscreen), never
  # with ring/shadow-contaminated edge pixels.
  [ActiveBorderNative]::EnsurePMv2()
  $fa = [ActiveBorderNative]::FrameOf($FocusedHwnd)
  $fb = [ActiveBorderNative]::FrameOf($OtherHwnd)
  if ($null -eq $fa -or $null -eq $fb) { Fail-Gu "$Tag member DWM frame unreadable" }
  $ax1 = [int]$fa[0]; $ay1 = [int]$fa[1]; $ax2 = [int]$fa[2]; $ay2 = [int]$fa[3]
  $bx1 = [int]$fb[0]; $by1 = [int]$fb[1]; $bx2 = [int]$fb[2]; $by2 = [int]$fb[3]
  $ox = [int]$OverlayRect[0]; $oy = [int]$OverlayRect[1]; $ow = [int]$OverlayRect[2]; $oh = [int]$OverlayRect[3]
  $pts = @()
  $inOuter = {
    param([int]$X, [int]$Y)
    return ($X -ge ($ox + 4) -and $X -lt ($ox + $ow - 4) -and $Y -ge ($oy + 4) -and $Y -lt ($oy + $oh - 4))
  }
  if ($ax2 -le $bx1 -and ($bx1 - $ax2) -ge 6) {
    $x0 = $ax2 + [int]$BorderPx + 1; $x1 = $bx1 - 1
    $y0 = [math]::Max($ay1, $by1); $y1 = [math]::Min($ay2, $by2)
    $overlap = $y1 - $y0
    if (($x1 - $x0) -lt 2 -or $overlap -lt 200) { Fail-Gu "$Tag vertical gap unusable x0=$x0 x1=$x1 overlap=$overlap fa=$($fa -join ',') fb=$($fb -join ',')" }
    $xa = $x0 + [int](($x1 - $x0) / 3); $xb = $x0 + [int](2 * ($x1 - $x0) / 3)
    if ($xb -le $xa) { $xb = $xa + 1 }
    $ya = $y0 + [int]($overlap / 2) - [int][math]::Min(200, $overlap / 4)
    $yb = $y0 + [int]($overlap / 2) + [int][math]::Min(200, $overlap / 4)
    $pts += ,@($xa, $ya); $pts += ,@($xb, $ya); $pts += ,@($xa, $yb); $pts += ,@($xb, $yb)
  } elseif ($bx2 -le $ax1 -and ($ax1 - $bx2) -ge 6) {
    $x0 = $bx2 + 1; $x1 = $ax1 - [int]$BorderPx - 1
    $y0 = [math]::Max($ay1, $by1); $y1 = [math]::Min($ay2, $by2)
    $overlap = $y1 - $y0
    if (($x1 - $x0) -lt 2 -or $overlap -lt 200) { Fail-Gu "$Tag vertical gap unusable x0=$x0 x1=$x1 overlap=$overlap fa=$($fa -join ',') fb=$($fb -join ',')" }
    $xa = $x0 + [int](($x1 - $x0) / 3); $xb = $x0 + [int](2 * ($x1 - $x0) / 3)
    if ($xb -le $xa) { $xb = $xa + 1 }
    $ya = $y0 + [int]($overlap / 2) - [int][math]::Min(200, $overlap / 4)
    $yb = $y0 + [int]($overlap / 2) + [int][math]::Min(200, $overlap / 4)
    $pts += ,@($xa, $ya); $pts += ,@($xb, $ya); $pts += ,@($xa, $yb); $pts += ,@($xb, $yb)
  } elseif ($ay2 -le $by1 -and ($by1 - $ay2) -ge 6) {
    $yy0 = $ay2 + [int]$BorderPx + 1; $yy1 = $by1 - 1
    $xx0 = [math]::Max($ax1, $bx1); $xx1 = [math]::Min($ax2, $bx2)
    $overlap = $xx1 - $xx0
    if (($yy1 - $yy0) -lt 2 -or $overlap -lt 200) { Fail-Gu "$Tag horizontal gap unusable fa=$($fa -join ',') fb=$($fb -join ',')" }
    $xxa = $xx0 + [int]($overlap / 2) - [int][math]::Min(200, $overlap / 4)
    $xxb = $xx0 + [int]($overlap / 2) + [int][math]::Min(200, $overlap / 4)
    $yya = $yy0 + [int](($yy1 - $yy0) / 3); $yyb = $yy0 + [int](2 * ($yy1 - $yy0) / 3)
    if ($yyb -le $yya) { $yyb = $yya + 1 }
    $pts += ,@($xxa, $yya); $pts += ,@($xxb, $yya); $pts += ,@($xxa, $yyb); $pts += ,@($xxb, $yyb)
  } elseif ($by2 -le $ay1 -and ($ay1 - $by2) -ge 6) {
    $yy0 = $by2 + 1; $yy1 = $ay1 - [int]$BorderPx - 1
    $xx0 = [math]::Max($ax1, $bx1); $xx1 = [math]::Min($ax2, $bx2)
    $overlap = $xx1 - $xx0
    if (($yy1 - $yy0) -lt 2 -or $overlap -lt 200) { Fail-Gu "$Tag horizontal gap unusable fa=$($fa -join ',') fb=$($fb -join ',')" }
    $xxa = $xx0 + [int]($overlap / 2) - [int][math]::Min(200, $overlap / 4)
    $xxb = $xx0 + [int]($overlap / 2) + [int][math]::Min(200, $overlap / 4)
    $yya = $yy0 + [int](($yy1 - $yy0) / 3); $yyb = $yy0 + [int](2 * ($yy1 - $yy0) / 3)
    if ($yyb -le $yya) { $yyb = $yya + 1 }
    $pts += ,@($xxa, $yya); $pts += ,@($xxb, $yya); $pts += ,@($xxa, $yyb); $pts += ,@($xxb, $yyb)
  } else {
    Fail-Gu "$Tag no usable inter-member gap fa=$($fa -join ',') fb=$($fb -join ',')"
  }
  foreach ($p in $pts) {
    $px = [int]$p[0]; $py = [int]$p[1]
    if (-not (& $inOuter $px $py)) { Fail-Gu "$Tag gap point $px,$py escapes outer $($OverlayRect -join ',')" }
    foreach ($f in @($fa, $fb)) {
      if ($px -ge [int]$f[0] -and $px -lt [int]$f[2] -and $py -ge [int]$f[1] -and $py -lt [int]$f[3]) { Fail-Gu "$Tag gap point $px,$py inside member frame $($f -join ',')" }
    }
  }
  return $pts
}

function Read-ScreenPixelsGu([object[]]$Pts, [string]$Tag) {
  [ActiveBorderNative]::EnsurePMv2()
  $hdc = [ActiveBorderNative]::GetDC([IntPtr]::Zero)
  if ($hdc -eq [IntPtr]::Zero) { Fail-Gu "$Tag GetDC failed" }
  try {
    $reads = @()
    foreach ($p in $Pts) {
      $c = [ActiveBorderNative]::GetPixel($hdc, [int]$p[0], [int]$p[1])
      if ($c -eq 0xFFFFFFFF) { Fail-Gu "$Tag GetPixel failed at $($p[0]),$($p[1])" }
      $reads += @{ x = [int]$p[0]; y = [int]$p[1]; r = [int]($c -band 0xFF); g = [int](($c -shr 8) -band 0xFF); b = [int](($c -shr 16) -band 0xFF) }
    }
    return $reads
  } finally { $null = [ActiveBorderNative]::ReleaseDC([IntPtr]::Zero, $hdc) }
}

function Assert-UnderlayBlendGu([object[]]$Baseline, [object[]]$Shown, [hashtable]$Fill, [string]$Tag, [int]$Tolerance = 35, [int]$MinDelta = 12) {
  # Real alpha-fill proof: same underlay-only points hidden then shown must
  # move toward the premultiplied blend out=(c*a+base*(255-a))/255 within a
  # calibrated DWM tolerance, with a meaningful delta. Never claims DIB
  # equals screen. Rejects vacuous no-change evidence.
  $a = [int]$Fill.a
  $passes = 0; $detail = @(); $allSame = $true
  for ($i = 0; $i -lt $Baseline.Count; $i++) {
    $bb = $Baseline[$i]; $ss = $Shown[$i]
    $expR = [int][math]::Round(([int]$Fill.r * $a + [int]$bb.r * (255 - $a)) / 255.0)
    $expG = [int][math]::Round(([int]$Fill.g * $a + [int]$bb.g * (255 - $a)) / 255.0)
    $expB = [int][math]::Round(([int]$Fill.b * $a + [int]$bb.b * (255 - $a)) / 255.0)
    $dr = [math]::Abs([int]$ss.r - $expR); $dg = [math]::Abs([int]$ss.g - $expG); $db = [math]::Abs([int]$ss.b - $expB)
    $delta = [math]::Max([math]::Abs([int]$ss.r - [int]$bb.r), [math]::Max([math]::Abs([int]$ss.g - [int]$bb.g), [math]::Abs([int]$ss.b - [int]$bb.b)))
    if ($delta -ne 0) { $allSame = $false }
    $ok = ($dr -le $Tolerance) -and ($dg -le $Tolerance) -and ($db -le $Tolerance) -and ($delta -ge $MinDelta)
    if ($ok) { $passes++ }
    $detail += "pt$($i)=$($ss.x),$($ss.y) base=#$('{0:x2}{1:x2}{2:x2}' -f $bb.r,$bb.g,$bb.b) shown=#$('{0:x2}{1:x2}{2:x2}' -f $ss.r,$ss.g,$ss.b) exp=#$('{0:x2}{1:x2}{2:x2}' -f $expR,$expG,$expB) d=$delta"
  }
  if ($allSame) { Fail-Gu "$Tag vacuous: all shown pixels equal baseline (no composition evidence)" }
  if ($passes -lt 3) { Fail-Gu "$Tag blend mismatch passes=$passes/4 tol=$Tolerance minDelta=$MinDelta fill=#$('{0:x2}{1:x2}{2:x2}{3:x2}' -f $a,$Fill.r,$Fill.g,$Fill.b) :: $($detail -join ' | ')" }
  return @{ passes = $passes; detail = $detail }
}

function Get-UnderlayVisibleCountGu($Insp) {
  # `present` stays true after the first show (carrier HWND kept alive
  # hidden for the next target); visibility is the per-overlay flag.
  if (-not $Insp.present) { return 0 }
  return @(@($Insp.overlays) | Where-Object { $_.visible -eq $true }).Count
}
function Wait-UnderlayHiddenGu([string]$OwnerBin, [int]$TimeoutSec, [string]$Tag) {
  # Chord-edge wake drives hide on the 100ms pump cadence; poll inspect with
  # 100ms resolution and return elapsed ms so legs prove fast hide instead
  # of the old 2s slow poll. Timeout stays bounded.
  $sw = [Diagnostics.Stopwatch]::StartNew()
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    $insp = Invoke-Native $OwnerBin @("underlay-inspect") | ConvertFrom-Json
    if ((Get-UnderlayVisibleCountGu $insp) -eq 0) { return @{ insp = $insp; elapsed_ms = [int]$sw.ElapsedMilliseconds } }
    Start-Sleep -Milliseconds 100
  }
  Fail-Gu "$Tag still visible after ${TimeoutSec}s"
}
function Hold-ChordBothGu([int[]]$DownVks, [string]$Tag) {
  # Back-to-back downs (single sleep): Win-down alone invites Start to steal
  # foreground before Shift lands, so both sides go down within ~ms in array
  # order, then one settle sleep.
  foreach ($vk in $DownVks) {
    if ([ActiveBorderNative]::SendKey([uint16]$vk, $false) -ne 1) { Fail-Gu "$Tag chord down vk=$vk not inserted" }
  }
  Start-Sleep -Milliseconds 600
}
function Hold-ChordGu([int[]]$DownVks, [string]$Tag) {
  foreach ($vk in $DownVks) {
    if ([ActiveBorderNative]::SendKey([uint16]$vk, $false) -ne 1) { Fail-Gu "$Tag chord down vk=$vk not inserted" }
  }
  Start-Sleep -Milliseconds 400
}
function Release-ChordGu([int[]]$UpVks) {
  foreach ($vk in $UpVks) {
    try { $null = [ActiveBorderNative]::SendKey([uint16]$vk, $true) } catch {}
  }
  Start-Sleep -Milliseconds 400
}

function Invoke-GuMock {
  Rec-Gu "scope" @{ helpers = "owned-only"; never = @("terminal"); kills = "exact-owner-only"; registry = "none" }
  $help = & cargo run --locked --manifest-path (Join-Path $Repo "Cargo.toml") -p tiler-windows --bin tiler-windows -- help 2>$null
  if ("$help" -notmatch "underlay-inspect") { Fail-Gu "mock CLI help missing underlay-inspect" }
  if ("$help" -notmatch "no-group-underlay") { Fail-Gu "mock CLI help missing group-underlay flags" }
  $src = Get-Content -LiteralPath (Join-Path $Repo "scripts\windows-group-underlay.ps1") -Raw
  foreach ($need in @("Get-UnderlayHwnds", "Wait-UnderlayOutcome", "Get-UnderlayVisibleCountGu", "Get-UnderlayExposedPointsGu", "Assert-UnderlayBlendGu", "OmniTilerGroupUnderlay", "underlay-inspect", "0x0082", "0x201E")) {
    if ($src -notmatch [regex]::Escape($need)) { Fail-Gu "mock harness missing $need" }
  }
  $tsrc = Get-Content -LiteralPath (Join-Path $Repo "crates\tiler-windows\src\tiling_sys.rs") -Raw
  if ($tsrc -notmatch "border_overlay\.is_visible\(\)") { Fail-Gu "mock product missing border visible gate" }
  if ($tsrc -notmatch "underlay_anchor_renderable") { Fail-Gu "mock product missing renderable anchor gate" }
  if ($tsrc -notmatch "underlay_chord_last") { Fail-Gu "mock product missing chord-edge wake state" }
  if ($tsrc -notmatch "underlay_chord_last\s*!=") { Fail-Gu "mock product missing chord-edge gate" }
  Rec-Gu "product-fix" @{ border_visible_gate = $true; anchor_renderable = $true; chord_edge_wake = $true }
  & cargo test --locked --manifest-path (Join-Path $Repo "Cargo.toml") -p tiler-windows --test tiling 2>$null | Out-Null
  if ($LASTEXITCODE -ne 0) { Fail-Gu "mock cargo test tiling failed" }
  Rec-Gu "portable-tests" @{ suite = "tiling"; result = "pass" }
  $report = @{ status = "pass"; stage = "GroupUnderlayMock"; steps = $GU_STEPS }
  $report | ConvertTo-Json -Depth 8 | Write-Output
}

function Invoke-GuLive {
  Install-BorderNative
  if ($OwnerSeconds -lt 1 -or $OwnerSeconds -gt 600) { Fail-Gu "refuse: OwnerSeconds 1..=600" }
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $runDir = Join-Path $Repo "target\windows-group-underlay\$stamp-$PID"
  New-Item -ItemType Directory -Force -Path $runDir | Out-Null
  $reportPath = Join-Path $runDir "group-underlay-report.json"
  if (Test-Path $reportPath) { Fail-Gu "report preexists" }
  $binDir = Join-Path $runDir "bin"; New-Item -ItemType Directory -Force -Path $binDir | Out-Null
  & cargo build --locked --manifest-path (Join-Path $Repo "Cargo.toml") -p tiler-windows
  if ($LASTEXITCODE -ne 0) { Fail-Gu "cargo build failed" }
  $ownerCopy = Join-Path $binDir "tiler-windows.exe"; $helperCopy = Join-Path $binDir "tiler-test-window.exe"
  Copy-Item (Join-Path $Repo "target\debug\tiler-windows.exe") $ownerCopy -Force
  Copy-Item (Join-Path $Repo "target\debug\tiler-test-window.exe") $helperCopy -Force
  $commit = (& git -C $Repo rev-parse HEAD | Out-String).Trim()
  $status = (& git -C $Repo status --short | Out-String).Trim()
  $ownerHash = (Get-FileHash -LiteralPath $ownerCopy -Algorithm SHA256).Hash
  $helperHash = (Get-FileHash -LiteralPath $helperCopy -Algorithm SHA256).Hash
  $me = Invoke-Native $ownerCopy @("identity") | ConvertFrom-Json
  Rec-Gu "env" @{ commit = $commit; status = $status; run_dir = $runDir; owner_sha256 = $ownerHash; helper_sha256 = $helperHash; pid = $PID; sid = "$($me.process.user_sid)"; session = $me.process.session_id }
  Assert-LedgerClean $ownerCopy
  $created = [System.Collections.ArrayList]@()
  $ownerPid = 0; $ready = $null; $logPath = ""
  $guErr = $null
  try {
    $snaps = @()
    foreach ($n in 1..2) {
      $receipt = Join-Path $runDir "helper$n.json"
      Start-ExplorerGui $helperCopy "run --receipt `"$receipt`" --seconds $OwnerSeconds --passive" $runDir
      $deadline = (Get-Date).AddSeconds(10)
      while (-not (Test-Path $receipt)) { if ((Get-Date) -gt $deadline) { Fail-Gu "helper receipt timeout $n" }; Start-Sleep -Milliseconds 200 }
      $snap = Get-Content -LiteralPath $receipt -Raw | ConvertFrom-Json
      if ("$($snap.process.exe_path)" -ine "$helperCopy") { Fail-Gu "helper$n peer mismatch" }
      Assert-ParentIsExplorer ([int]$snap.process.pid)
      if ([string]$snap.tag -eq "") { Fail-Gu "helper$n missing tag" }
      $null = $created.Add(@{ hwnd = [uint64]$snap.hwnd; tag = "$($snap.tag)" })
      $shown = Invoke-Native $helperCopy @("show", "$($snap.hwnd)", "--tag", "$($snap.tag)") | ConvertFrom-Json
      $fresh = Assert-HelperIdentityAb $helperCopy $snap "helper$n-postshow"
      $snaps += $fresh
    }
    Rec-Gu "helpers-created" @($snaps | ForEach-Object { @{ hwnd = $_.hwnd; pid = $_.process.pid } })
    $allowPath = Join-Path $runDir "allowlist.json"
    $entries = @(); foreach ($s in $snaps) { $entries += @{ hwnd = [uint64]$s.hwnd; pid = [uint32]$s.process.pid; process_creation = "$($s.process.process_creation)"; exe_path = "$($s.process.exe_path)"; user_sid = "$($s.process.user_sid)"; session_id = [uint32]$s.process.session_id; tag = "$($s.tag)" } }
    @{ windows = $entries } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $allowPath
    Start-ExplorerGui $ownerCopy "tile-proof --allowlist `"$allowPath`" --seconds $OwnerSeconds --trace" $binDir
    $deadline = (Get-Date).AddSeconds(15)
    while ((Get-Date) -lt $deadline) {
      $j = Invoke-Native $ownerCopy @("ready") | ConvertFrom-Json
      if ($j.ready -eq $true) { $ready = $j; break }
      Start-Sleep -Milliseconds 250
    }
    if ($null -eq $ready) { Fail-Gu "proof owner ready timeout" }
    Assert-ParentIsExplorer ([int]$ready.owner.pid)
    $ownerPid = [int]$ready.owner.pid; $logPath = "$($ready.log_path)"
    Rec-Gu "owner-ready" @{ pid = $ownerPid; creation = "$($ready.owner.process_creation)"; log = $logPath }
    Start-Sleep -Milliseconds 2500
    $hA = $snaps[0]; $hB = $snaps[1]
    $spiA = [ActiveBorderNative]::SpiGet(0x0082); $spiP = [ActiveBorderNative]::SpiGet(0x201E)
    Rec-Gu "settings-pre" @{ arranging_raw = $spiA; pen_raw = $spiP }
    if ([int]$spiA -ne 1) { Fail-Gu "arranging raw $spiA != 1 (GET0x0082)" }
    if ([int]$spiP -ne 35) { Fail-Gu "pen raw $spiP != 35 (GET0x201E)" }
    $dpiA = [ActiveBorderNative]::GetDpiForWindow([IntPtr][long]$hA.hwnd)
    if ([int]$dpiA -ne 120) { Fail-Gu "helper DPI $dpiA != 120" }
    Rec-Gu "dpi" @{ a = [int]$dpiA }
    # Focus A for deterministic group focus. One mechanical retry: a Start
    # menu left open by an earlier chord release blocks activation; ESC
    # dismisses it. Records the blocking class for the report.
    try {
      $freshA = Set-OwnedForegroundAb $helperCopy $hA "gu-focus"
    } catch {
      $fgBlock = [ActiveBorderNative]::GetForegroundWindow().ToInt64()
      $clsBlock = [ActiveBorderNative]::ClassOf($fgBlock)
      Rec-Gu "focus-retry" @{ fg = $fgBlock; class = $clsBlock; first_error = "$($_.Exception.Message)" }
      if ([ActiveBorderNative]::SendKey([uint16]$VK_ESC, $false) -ne 1) { Fail-Gu "focus-retry ESC down not inserted" }
      Start-Sleep -Milliseconds 150
      if ([ActiveBorderNative]::SendKey([uint16]$VK_ESC, $true) -ne 1) { Fail-Gu "focus-retry ESC up not inserted" }
      Start-Sleep -Milliseconds 800
      $freshA = Set-OwnedForegroundAb $helperCopy $hA "gu-focus-retry"
    }
    Start-Sleep -Milliseconds 1500
    # Leg0: default hidden, no trigger.
    $insp0 = Invoke-Native $ownerCopy @("underlay-inspect") | ConvertFrom-Json
    Rec-Gu "leg0-default-hidden" @{ visible = (Get-UnderlayVisibleCountGu $insp0); foreground = $insp0.foreground }
    if ((Get-UnderlayVisibleCountGu $insp0) -ne 0) { Fail-Gu "leg0 underlay visible without trigger" }
    # Leg1: Win-alone and Shift-alone hidden. Win-alone release summons
    # Start (tap semantics), so ESC-dismiss + refocus after each single-key
    # leg; later legs need deterministic helper focus.
    $mark = Get-GuMark $logPath
    Hold-ChordGu @($VK_LWIN) "leg1-win"
    Start-Sleep -Milliseconds 1200
    $inspW = Invoke-Native $ownerCopy @("underlay-inspect") | ConvertFrom-Json
    Release-ChordGu @($VK_LWIN)
    Rec-Gu "leg1-win-alone" @{ visible = (Get-UnderlayVisibleCountGu $inspW) }
    if ((Get-UnderlayVisibleCountGu $inspW) -ne 0) { Fail-Gu "leg1 Win-alone showed underlay" }
    if ([ActiveBorderNative]::SendKey([uint16]$VK_ESC, $false) -ne 1) { Fail-Gu "leg1 ESC down not inserted" }
    Start-Sleep -Milliseconds 150
    if ([ActiveBorderNative]::SendKey([uint16]$VK_ESC, $true) -ne 1) { Fail-Gu "leg1 ESC up not inserted" }
    Start-Sleep -Milliseconds 800
    $freshA = Set-OwnedForegroundAb $helperCopy $hA "leg1-restore"
    Hold-ChordGu @($VK_LSHIFT) "leg1-shift"
    Start-Sleep -Milliseconds 1200
    $inspS = Invoke-Native $ownerCopy @("underlay-inspect") | ConvertFrom-Json
    Release-ChordGu @($VK_LSHIFT)
    Rec-Gu "leg1-shift-alone" @{ visible = (Get-UnderlayVisibleCountGu $inspS) }
    if ((Get-UnderlayVisibleCountGu $inspS) -ne 0) { Fail-Gu "leg1 Shift-alone showed underlay" }
    $freshA = Set-OwnedForegroundAb $helperCopy $hA "leg1-restore2"
    # Leg2: Win->Shift chord shown; geometry/z/click-through + 100ms cadence.
    # Back-to-back downs: Win-down alone invites Start to steal foreground.
    $mark = Get-GuMark $logPath
    $swShow = [Diagnostics.Stopwatch]::StartNew()
    Hold-ChordBothGu @($VK_LWIN, $VK_LSHIFT) "leg2-chord"
    try {
      try {
        $ev = Wait-UnderlayOutcome $logPath $mark @("shown", "redrew") "" 20 "leg2"
      } catch {
        $fgD = [ActiveBorderNative]::GetForegroundWindow().ToInt64()
        $clsD = [ActiveBorderNative]::ClassOf($fgD)
        $tailD = Get-CompleteLinesAb $logPath
        $guD = @()
        for ($i = $mark; $i -lt $tailD.Count; $i++) {
          if ("$($tailD[$i])".Trim() -eq "") { continue }
          $e = $tailD[$i] | ConvertFrom-Json
          if ("$($e.event)" -eq "group-underlay") { $guD += "$($e.outcome)/$($e.reason)" }
        }
        Rec-Gu "leg2-diag" @{ foreground = $fgD; class = $clsD; focus_want = $freshA.hwnd; tail = @($guD | Select-Object -Last 12) }
        throw
      }
      $shown = $ev.event
      $showMs = [int]$swShow.ElapsedMilliseconds
      Rec-Gu "leg2-chord-shown" @{ outer = $shown.outer; color = $shown.color; members = $shown.members; dib = "$($shown.dib_checksum)"; show_ms = $showMs }
      if ($showMs -gt 1800) { Fail-Gu "leg2 bare-chord show latency ${showMs}ms > 1800ms (chord-edge wake missing; slow poll is 2000ms)" }
      if ("$($shown.dib_checksum)" -eq "0000000000000000") { Fail-Gu "leg2 zero dib checksum" }
      if ("$($shown.color)" -ne "#662a82da") { Fail-Gu "leg2 default color $($shown.color) != #662a82da" }
      $union = Get-UnionOfFramesGu @([long]$freshA.hwnd, [long]$hB.hwnd)
      $insp = Invoke-Native $ownerCopy @("underlay-inspect") | ConvertFrom-Json
      if ((Get-UnderlayVisibleCountGu $insp) -eq 0) { Fail-Gu "leg2 underlay-inspect not visible while shown" }
      $orect = @($insp.overlays[0].rect)
      $null = Assert-UnionOuterGu $orect $union 8 "leg2-exact"
      $uhwnds = @(Get-UnderlayHwnds $ownerPid "OmniTilerGroupUnderlay") | Where-Object { $_ -ne 0 }
      if (@($uhwnds).Count -ne 1) { Fail-Gu "leg2 underlay hwnd count $($uhwnds.Count) != 1" }
      $uh = [long]$uhwnds[0]
      $null = Assert-BelowAllGu $uh @([long]$freshA.hwnd, [long]$hB.hwnd) "leg2-z"
      # Below yellow border too when visible.
      $b = Invoke-Native $ownerCopy @("border-inspect") | ConvertFrom-Json
      if ($b.present) {
        $bhList = @(Get-UnderlayHwnds $ownerPid "OmniTilerActiveBorder") | Where-Object { $_ -ne 0 }
        if (@($bhList).Count -ge 1) { $null = Assert-BelowAllGu $uh @([long]$bhList[0]) "leg2-z-border" }
      }
      # Default pad-8 keeps exact-outer/z proof only: its mid-pad samples
      # overlap the yellow ring and DWM shadow, so no blend claim here. The
      # substantive alpha proof runs on the wide-extension leg below.
      Rec-Gu "leg2-geometry" @{ overlay = ($orect -join ","); union = ($union -join ","); pad = 8; z_below_members = $true; show_ms = $showMs }
      $fgMid = [ActiveBorderNative]::GetForegroundWindow().ToInt64()
      if ([uint64]$fgMid -ne [uint64]$freshA.hwnd) { Fail-Gu "leg2 chord stole focus fg=$fgMid" }
      $ex = [ActiveBorderNative]::GetWindowLongW([IntPtr]$uh, -20)
      $WS_EX_TRANSPARENT = 0x20; $WS_EX_NOACTIVATE = 0x08000000
      if (($ex -band $WS_EX_TRANSPARENT) -eq 0 -or ($ex -band $WS_EX_NOACTIVATE) -eq 0) { Fail-Gu "leg2 click-through styles missing ex=0x$($ex.ToString('X'))" }
      Rec-Gu "leg2-input" @{ foreground_stable = $true; click_through = $true }
    } finally { Release-ChordGu @($VK_LSHIFT, $VK_LWIN) }
    $hideRes = Wait-UnderlayHiddenGu $ownerCopy 12 "leg2-release"
    $inspIdle = $hideRes.insp
    Rec-Gu "leg2-release-hidden" @{ visible = (Get-UnderlayVisibleCountGu $inspIdle); hide_ms = [int]$hideRes.elapsed_ms }
    if ([int]$hideRes.elapsed_ms -gt 1800) { Fail-Gu "leg2 release hide latency $($hideRes.elapsed_ms)ms > 1800ms" }
    # A chord release may also summon Start; dismiss + restore focus for leg4.
    if ([ActiveBorderNative]::SendKey([uint16]$VK_ESC, $false) -ne 1) { Fail-Gu "leg2 ESC down not inserted" }
    Start-Sleep -Milliseconds 150
    if ([ActiveBorderNative]::SendKey([uint16]$VK_ESC, $true) -ne 1) { Fail-Gu "leg2 ESC up not inserted" }
    Start-Sleep -Milliseconds 800
    $freshA = Set-OwnedForegroundAb $helperCopy $hA "leg2-restore"
    # Leg3: Shift->Win order + extras (back-to-back, Shift first).
    $mark = Get-GuMark $logPath
    Hold-ChordBothGu @($VK_RSHIFT, $VK_RWIN, $VK_CTRL) "leg3-chord"
    try {
      $ev3 = Wait-UnderlayOutcome $logPath $mark @("shown", "redrew") "" 20 "leg3"
      Rec-Gu "leg3-shift-win-extra" @{ outer = $ev3.event.outer; outcome = "$($ev3.event.outcome)" }
    } finally { Release-ChordGu @($VK_CTRL, $VK_RWIN, $VK_RSHIFT) }
    Start-Sleep -Milliseconds 1200
    if ([ActiveBorderNative]::SendKey([uint16]$VK_ESC, $false) -ne 1) { Fail-Gu "leg3 ESC down not inserted" }
    Start-Sleep -Milliseconds 150
    if ([ActiveBorderNative]::SendKey([uint16]$VK_ESC, $true) -ne 1) { Fail-Gu "leg3 ESC up not inserted" }
    Start-Sleep -Milliseconds 800
    $freshA = Set-OwnedForegroundAb $helperCopy $hA "leg3-restore"
    # Leg4: native titlebar move without chord; source union fixed; drop clears.
    # Focused-subject B only: the gesture helper requires foreground, so an
    # unfocused-subject C probe is not possible here; C stays parked and is
    # reported as not proven. START classification is sampled via WM_NCHITTEST.
    $unionPre = Get-UnionOfFramesGu @([long]$freshA.hwnd, [long]$hB.hwnd)
    $memberPre = Get-NativeFrameAb ([long]$freshA.hwnd)
    if ($null -eq $memberPre) { Fail-Gu "leg4 member pre-frame unreadable" }
    $memberPreKey = ($memberPre -join ",")
    $mark = Get-GuMark $logPath
    $drag = Invoke-ActiveTitleDragAb $helperCopy $freshA 120 60 $logPath $mark "leg4-move" $ownerPid
    $tail = Get-CompleteLinesAb $logPath
    $uEvts = @()
    for ($i = $mark; $i -lt $tail.Count; $i++) {
      if ("$($tail[$i])".Trim() -eq "") { continue }
      $e = $tail[$i] | ConvertFrom-Json
      if ("$($e.event)" -eq "group-underlay" -and @("shown", "redrew", "moved") -contains "$($e.outcome)") { $uEvts += $e }
    }
    if (@($uEvts).Count -eq 0) { Fail-Gu "leg4 no underlay during native move (START classification may have missed; see timing)" }
    $first = $uEvts[0]
    # Real displacement: moving member PRE-frame (l,t,r,b) vs later trace
    # frames. moved_steps alone is not enough; the frame must actually leave
    # its original allocation.
    $displaced = 0
    foreach ($st in @($drag.trace)) {
      if ("$($st.frame)" -eq "unreadable") { continue }
      if ("$($st.frame)" -ne $memberPreKey) { $displaced++ }
    }
    if ($displaced -lt 2) { Fail-Gu "leg4 no real member displacement (displaced=$displaced/8 pre=$memberPreKey)" }
    Rec-Gu "leg4-native-move" @{ underlay_outer = $first.outer; union_pre = ($unionPre -join ","); member_pre = $memberPreKey; moved_steps = $drag.moved_steps; displaced_steps = $displaced; outcome = "$($first.outcome)"; c_probe = "not-proven-parked" }
    # Source union fixed: every shown outer during drag equals chord outer (pad8 over pre union).
    $expX = [int]$unionPre[0] - 8; $expY = [int]$unionPre[1] - 8; $expW = [int]$unionPre[2] + 16; $expH = [int]$unionPre[3] + 16
    foreach ($e in $uEvts) {
      $got = "$($e.outer -join ',')"; $want = "$expX,$expY,$expW,$expH"
      if ($got -ne $want) { Fail-Gu "leg4 source union drift got=$got want=$want" }
    }
    $dropRes = Wait-UnderlayHiddenGu $ownerCopy 12 "leg4-drop"
    $inspDrop = $dropRes.insp
    Rec-Gu "leg4-drop-clears" @{ visible = (Get-UnderlayVisibleCountGu $inspDrop); hide_ms = [int]$dropRes.elapsed_ms }
    # Leg5: resize-alone hidden; resize+chord shown.
    $mark = Get-GuMark $logPath
    $rsz = Invoke-ActiveEdgeResizeAb $helperCopy $freshA 80 60 $logPath $mark "leg5-resize" $ownerPid
    $tail = Get-CompleteLinesAb $logPath
    $uR = @()
    for ($i = $mark; $i -lt $tail.Count; $i++) {
      if ("$($tail[$i])".Trim() -eq "") { continue }
      $e = $tail[$i] | ConvertFrom-Json
      if ("$($e.event)" -eq "group-underlay" -and @("shown", "redrew", "moved") -contains "$($e.outcome)") { $uR += $e }
    }
    Rec-Gu "leg5-resize-alone" @{ underlay_events = @($uR).Count }
    if (@($uR).Count -ne 0) { Fail-Gu "leg5 resize-alone showed underlay" }
    Start-Sleep -Milliseconds 1200
    # Resize + chord: hold chord then edge resize.
    $mark = Get-GuMark $logPath
    Hold-ChordGu @($VK_LWIN, $VK_LSHIFT) "leg5-chord"
    try {
      $rsz2 = Invoke-ActiveEdgeResizeAb $helperCopy $freshA 60 40 $logPath $mark "leg5-resize-chord" $ownerPid
      $tail = Get-CompleteLinesAb $logPath
      $uRC = @()
      for ($i = $mark; $i -lt $tail.Count; $i++) {
        if ("$($tail[$i])".Trim() -eq "") { continue }
        $e = $tail[$i] | ConvertFrom-Json
        if ("$($e.event)" -eq "group-underlay" -and @("shown", "redrew", "moved") -contains "$($e.outcome)") { $uRC += $e }
      }
      Rec-Gu "leg5-resize-chord" @{ underlay_events = @($uRC).Count }
      if (@($uRC).Count -eq 0) { Fail-Gu "leg5 resize+chord stayed hidden" }
    } finally { Release-ChordGu @($VK_LWIN, $VK_LSHIFT) }
    Start-Sleep -Milliseconds 1200
    # Leg6: focus retarget B + maximize suppression.
    $freshB = Set-OwnedForegroundAb $helperCopy $hB "leg6-focusB"
    $mark = Get-GuMark $logPath
    Hold-ChordGu @($VK_LWIN, $VK_LSHIFT) "leg6-chordB"
    try {
      $evB = Wait-UnderlayOutcome $logPath $mark @("shown", "redrew") "" 20 "leg6"
      Rec-Gu "leg6-retarget" @{ target = "$($evB.event.target)"; outer = $evB.event.outer }
    } finally { Release-ChordGu @($VK_LWIN, $VK_LSHIFT) }
    $null = Invoke-OwnedSysCommandAb $helperCopy $freshB $SC_MAXIMIZE "leg6-max"
    Start-Sleep -Milliseconds 800
    $mark = Get-GuMark $logPath
    Hold-ChordGu @($VK_LWIN, $VK_LSHIFT) "leg6-maxchord"
    try {
      Start-Sleep -Milliseconds 1500
      $inspMax = Invoke-Native $ownerCopy @("underlay-inspect") | ConvertFrom-Json
      Rec-Gu "leg6-max-suppressed" @{ visible = (Get-UnderlayVisibleCountGu $inspMax) }
      if ((Get-UnderlayVisibleCountGu $inspMax) -ne 0) { Fail-Gu "leg6 maximized showed underlay" }
    } finally { Release-ChordGu @($VK_LWIN, $VK_LSHIFT) }
    $null = Invoke-OwnedSysCommandAb $helperCopy $freshB $SC_RESTORE "leg6-restore"
    Start-Sleep -Milliseconds 800
    $freshA = Set-OwnedForegroundAb $helperCopy $hA "leg6-restore-focus"
    # Leg6b: modifier matrix - release either side while the other stays held.
    # Chord shows; release Win only (Shift held) hides; re-hold shows;
    # release Shift only (Win held) hides. Proves both sides are required.
    $mark = Get-GuMark $logPath
    Hold-ChordBothGu @($VK_LWIN, $VK_LSHIFT) "leg6b-chord"
    $ev6b = Wait-UnderlayOutcome $logPath $mark @("shown", "redrew") "" 20 "leg6b"
    Rec-Gu "leg6b-shown" @{ outcome = "$($ev6b.event.outcome)" }
    try {
      if ([ActiveBorderNative]::SendKey([uint16]$VK_LWIN, $true) -ne 1) { Fail-Gu "leg6b Win up not inserted" }
      Start-Sleep -Milliseconds 1200
      $insp6b1 = Invoke-Native $ownerCopy @("underlay-inspect") | ConvertFrom-Json
      Rec-Gu "leg6b-win-released" @{ visible = (Get-UnderlayVisibleCountGu $insp6b1) }
      if ((Get-UnderlayVisibleCountGu $insp6b1) -ne 0) { Fail-Gu "leg6b Shift-alone (Win released) showed underlay" }
      if ([ActiveBorderNative]::SendKey([uint16]$VK_LWIN, $false) -ne 1) { Fail-Gu "leg6b Win re-down not inserted" }
      $mark = Get-GuMark $logPath
      $ev6b2 = Wait-UnderlayOutcome $logPath $mark @("shown", "redrew") "" 20 "leg6b-rehold"
      Rec-Gu "leg6b-rehold" @{ outcome = "$($ev6b2.event.outcome)" }
      if ([ActiveBorderNative]::SendKey([uint16]$VK_LSHIFT, $true) -ne 1) { Fail-Gu "leg6b Shift up not inserted" }
      Start-Sleep -Milliseconds 1200
      $insp6b2 = Invoke-Native $ownerCopy @("underlay-inspect") | ConvertFrom-Json
      Rec-Gu "leg6b-shift-released" @{ visible = (Get-UnderlayVisibleCountGu $insp6b2) }
      if ((Get-UnderlayVisibleCountGu $insp6b2) -ne 0) { Fail-Gu "leg6b Win-alone (Shift released) showed underlay" }
    } finally { Release-ChordGu @($VK_LWIN, $VK_LSHIFT) }
    Start-Sleep -Milliseconds 800
    if ([ActiveBorderNative]::SendKey([uint16]$VK_ESC, $false) -ne 1) { Fail-Gu "leg6b ESC down not inserted" }
    Start-Sleep -Milliseconds 150
    if ([ActiveBorderNative]::SendKey([uint16]$VK_ESC, $true) -ne 1) { Fail-Gu "leg6b ESC up not inserted" }
    Start-Sleep -Milliseconds 800
    $freshA = Set-OwnedForegroundAb $helperCopy $hA "leg6b-restore"
    # Leg6c: END-then-chord (sequential handoff, no Win held across the
    # drag). A concurrent Win-hold across Invoke-ActiveTitleDragAb collapsed
    # observation in run 20261002-235902 (allowlist-changed from tick 52,
    # managed 2->0, chord read idle after drag end), so the overlap variant
    # is parked pending that invariant; this leg proves END clearing is
    # move-scoped by showing the chord works on the post-move union.
    $mark = Get-GuMark $logPath
    $drag6c = Invoke-ActiveTitleDragAb $helperCopy $freshA 80 40 $logPath $mark "leg6c-move" $ownerPid
    # Post-drop foreground is NOT verified by the gesture helper (its last
    # check predates mouse-up). Two runs saw a foreign foreground stick from
    # the drop (allowlist-changed, helpers stay managed=2): capture it with
    # class, then one bounded ESC-dismiss + refocus retry per the established
    # Start-steal precedent. A second stuck foreign foreground fails loudly
    # with the class as mechanism evidence, never as a product claim.
    $fgDrop = [ActiveBorderNative]::GetForegroundWindow().ToInt64()
    if ([uint64]$fgDrop -ne [uint64]$freshA.hwnd) {
      $clsDrop = [ActiveBorderNative]::ClassOf($fgDrop)
      Rec-Gu "leg6c-drop-fg" @{ fg = $fgDrop; class = $clsDrop; want = [long]$freshA.hwnd }
      if ([ActiveBorderNative]::SendKey([uint16]$VK_ESC, $false) -ne 1) { Fail-Gu "leg6c drop ESC down not inserted" }
      Start-Sleep -Milliseconds 150
      if ([ActiveBorderNative]::SendKey([uint16]$VK_ESC, $true) -ne 1) { Fail-Gu "leg6c drop ESC up not inserted" }
      Start-Sleep -Milliseconds 800
      $freshA = Set-OwnedForegroundAb $helperCopy $hA "leg6c-drop-restore"
      $fgDrop2 = [ActiveBorderNative]::GetForegroundWindow().ToInt64()
      if ([uint64]$fgDrop2 -ne [uint64]$freshA.hwnd) {
        Fail-Gu "leg6c foreground stuck foreign fg=$fgDrop2 class=$([ActiveBorderNative]::ClassOf($fgDrop2)) first_fg=$fgDrop first_class=$clsDrop"
      }
      Rec-Gu "leg6c-drop-retry" @{ ok = $true; first_class = $clsDrop }
    }
    $unionPost6c = Get-UnionOfFramesGu @([long]$freshA.hwnd, [long]$hB.hwnd)
    $mark = Get-GuMark $logPath
    Hold-ChordBothGu @($VK_LWIN, $VK_LSHIFT) "leg6c-chord"
    try {
      $ev6c = Wait-UnderlayOutcome $logPath $mark @("shown", "redrew") "" 20 "leg6c"
      $got6c = "$($ev6c.event.outer -join ',')"
      $exp6c = "$([int]$unionPost6c[0] - 8),$([int]$unionPost6c[1] - 8),$([int]$unionPost6c[2] + 16),$([int]$unionPost6c[3] + 16)"
      Rec-Gu "leg6c-end-then-chord" @{ outer = $ev6c.event.outer; union_post = ($unionPost6c -join ","); moved = $drag6c.moved_steps }
      if ($got6c -ne $exp6c) { Fail-Gu "leg6c post-move union drift got=$got6c want=$exp6c" }
    } finally { Release-ChordGu @($VK_LWIN, $VK_LSHIFT) }
    $drop6c = Wait-UnderlayHiddenGu $ownerCopy 12 "leg6c-release"
    Rec-Gu "leg6c-release-hidden" @{ hide_ms = [int]$drop6c.elapsed_ms }
    $freshA = Set-OwnedForegroundAb $helperCopy $hA "leg6c-restore"
    # Leg4b: repeat native drag (fast START classification sample 2/2).
    # A single START miss from WM_NCHITTEST event-drain would flake here.
    $mark = Get-GuMark $logPath
    $drag2 = Invoke-ActiveTitleDragAb $helperCopy $freshA 60 40 $logPath $mark "leg4b-move" $ownerPid
    $tail = Get-CompleteLinesAb $logPath
    $uEvts2 = @()
    for ($i = $mark; $i -lt $tail.Count; $i++) {
      if ("$($tail[$i])".Trim() -eq "") { continue }
      $e = $tail[$i] | ConvertFrom-Json
      if ("$($e.event)" -eq "group-underlay" -and @("shown", "redrew", "moved") -contains "$($e.outcome)") { $uEvts2 += $e }
    }
    Rec-Gu "leg4b-repeat" @{ underlay_events = @($uEvts2).Count; moved_steps = $drag2.moved_steps }
    if (@($uEvts2).Count -eq 0) { Fail-Gu "leg4b repeat drag missed START classification" }
    $drop4b = Wait-UnderlayHiddenGu $ownerCopy 12 "leg4b-drop"
    Rec-Gu "leg4b-drop" @{ hide_ms = [int]$drop4b.elapsed_ms }
    $freshA = Set-OwnedForegroundAb $helperCopy $hA "leg4b-restore"
    # Leg4c: stationary START + brief chord tap + ESC cancel, one manual hold.
    # Mouse down on the focused titlebar with NO movement must already show
    # the fill (START classification, not displacement). A concurrent
    # long Win-hold across a helper drag collapsed observation in run
    # 20261002-235902, so the mid-move overlap uses one brief tap (~300ms)
    # with foreground/class diagnostics; ESC must cancel the system move and
    # clear it. C stays parked (focused subject only).
    $outerPre4c = Get-NativeRectAb ([long]$freshA.hwnd)
    $titlePt = Find-TitlePointAb ([long]$freshA.hwnd) $outerPre4c
    if ($null -eq $titlePt) { Fail-Gu "leg4c no title point" }
    $dpi4c = [ActiveBorderNative]::GetDpiForWindow([IntPtr][long]$freshA.hwnd)
    $vs4c = Get-VirtualScreenAb ([uint32]$dpi4c)
    $ax4c = ConvertTo-AbsoluteAb ([int]$titlePt[0]) ([int]$vs4c[0]) ([int]$vs4c[2])
    $ay4c = ConvertTo-AbsoluteAb ([int]$titlePt[1]) ([int]$vs4c[1]) ([int]$vs4c[3])
    $saved4c = $null
    try { $pt0 = New-Object ActiveBorderNative+POINT; if ([ActiveBorderNative]::GetCursorPos([ref]$pt0)) { $saved4c = @($pt0.x, $pt0.y) } } catch {}
    $held4c = New-HeldProcessAb ([uint32]$freshA.process.pid) "leg4c"
    $heldCr4c = [ActiveBorderNative]::HeldCreation($held4c)
    $MOUSE_MOVE = 0x0001; $MOUSE_ABS = 0x8000; $MOUSE_DOWN = 0x0002; $MOUSE_UP = 0x0004
    $mark = Get-GuMark $logPath
    try {
      if ([ActiveBorderNative]::SendMouse(($MOUSE_MOVE -bor $MOUSE_ABS), $ax4c, $ay4c) -eq 0) { Fail-Gu "leg4c cursor move not inserted" }
      Start-Sleep -Milliseconds 250
      $fgD = [ActiveBorderNative]::GetForegroundWindow().ToInt64()
      if ([uint64]$fgD -ne [uint64]$freshA.hwnd) { Fail-Gu "leg4c lost foreground before down" }
      if ([ActiveBorderNative]::SendMouse($MOUSE_DOWN, 0, 0) -eq 0) { Fail-Gu "leg4c button down not inserted" }
      Start-Sleep -Milliseconds 1200
      $inspStat = Invoke-Native $ownerCopy @("underlay-inspect") | ConvertFrom-Json
      $statVis = (Get-UnderlayVisibleCountGu $inspStat)
      $fgStat = [ActiveBorderNative]::GetForegroundWindow().ToInt64()
      Rec-Gu "leg4c-stationary" @{ visible = $statVis; fg = $fgStat; fg_class = [ActiveBorderNative]::ClassOf($fgStat) }
      if ($statVis -eq 0) { Fail-Gu "leg4c stationary hold showed nothing (START classification missed)" }
      # Brief chord tap mid-hold: down-down, 300ms, inspect, up-up.
      if ([ActiveBorderNative]::SendKey([uint16]$VK_LWIN, $false) -ne 1) { Fail-Gu "leg4c chord Win down not inserted" }
      if ([ActiveBorderNative]::SendKey([uint16]$VK_LSHIFT, $false) -ne 1) { Fail-Gu "leg4c chord Shift down not inserted" }
      Start-Sleep -Milliseconds 300
      $inspBoth = Invoke-Native $ownerCopy @("underlay-inspect") | ConvertFrom-Json
      $fgBoth = [ActiveBorderNative]::GetForegroundWindow().ToInt64()
      Rec-Gu "leg4c-tap-held" @{ visible = (Get-UnderlayVisibleCountGu $inspBoth); fg = $fgBoth; fg_class = [ActiveBorderNative]::ClassOf($fgBoth) }
      try { $null = [ActiveBorderNative]::SendKey([uint16]$VK_LWIN, $true) } catch {}
      try { $null = [ActiveBorderNative]::SendKey([uint16]$VK_LSHIFT, $true) } catch {}
      Start-Sleep -Milliseconds 800
      $inspHand = Invoke-Native $ownerCopy @("underlay-inspect") | ConvertFrom-Json
      $fgHand = [ActiveBorderNative]::GetForegroundWindow().ToInt64()
      Rec-Gu "leg4c-handoff" @{ visible_after_chord_clear = (Get-UnderlayVisibleCountGu $inspHand); fg = $fgHand; fg_class = [ActiveBorderNative]::ClassOf($fgHand) }
      if ((Get-UnderlayVisibleCountGu $inspHand) -eq 0) {
        $tailH = Get-CompleteLinesAb $logPath
        $guH = @()
        for ($i = $mark; $i -lt $tailH.Count; $i++) {
          if ("$($tailH[$i])".Trim() -eq "") { continue }
          $e = $tailH[$i] | ConvertFrom-Json
          if ("$($e.event)" -eq "group-underlay") { $guH += "$($e.outcome)/$($e.reason)" }
        }
        Rec-Gu "leg4c-handoff-diag" @{ tail = @($guH | Select-Object -Last 12) }
        Fail-Gu "leg4c chord clear hid surviving move"
      }
      if ([ActiveBorderNative]::SendKey([uint16]$VK_ESC, $false) -ne 1) { Fail-Gu "leg4c ESC down not inserted" }
      Start-Sleep -Milliseconds 150
      if ([ActiveBorderNative]::SendKey([uint16]$VK_ESC, $true) -ne 1) { Fail-Gu "leg4c ESC up not inserted" }
      Start-Sleep -Milliseconds 800
      $inspCanc = Invoke-Native $ownerCopy @("underlay-inspect") | ConvertFrom-Json
      Rec-Gu "leg4c-esc-cancel" @{ visible = (Get-UnderlayVisibleCountGu $inspCanc) }
      if ((Get-UnderlayVisibleCountGu $inspCanc) -ne 0) { Fail-Gu "leg4c ESC cancel left underlay shown" }
      if (-not [ActiveBorderNative]::HeldAlive($held4c)) { Fail-Gu "leg4c held process exited" }
      if ([ActiveBorderNative]::HeldCreation($held4c) -ne $heldCr4c) { Fail-Gu "leg4c held creation changed" }
    } finally {
      try { $null = [ActiveBorderNative]::SendMouse($MOUSE_UP, 0, 0) } catch {}
      Assert-ButtonReleasedAb "leg4c-release"
      try { $null = [ActiveBorderNative]::SendKey([uint16]$VK_LWIN, $true) } catch {}
      try { $null = [ActiveBorderNative]::SendKey([uint16]$VK_LSHIFT, $true) } catch {}
      if ($null -ne $saved4c) {
        try {
          $rx = ConvertTo-AbsoluteAb ([int]$saved4c[0]) ([int]$vs4c[0]) ([int]$vs4c[2]); $ry = ConvertTo-AbsoluteAb ([int]$saved4c[1]) ([int]$vs4c[1]) ([int]$vs4c[3])
          $null = [ActiveBorderNative]::SendMouse(($MOUSE_MOVE -bor $MOUSE_ABS), $rx, $ry)
        } catch {}
      }
      try { Close-HeldProcessAb $held4c } catch {}
    }
    $drop4c = Wait-UnderlayHiddenGu $ownerCopy 12 "leg4c-drop"
    Rec-Gu "leg4c-drop" @{ hide_ms = [int]$drop4c.elapsed_ms }
    $freshA = Set-OwnedForegroundAb $helperCopy $hA "leg4c-restore"
    # Disabled-flag owner.
    $st1 = Stop-PayloadOwner $ownerCopy
    Rec-Gu "owner-stop-for-flag" @{ exited = $st1.stop.owner_exited; restored = $st1.restore.restored }
    Start-ExplorerGui $ownerCopy "tile-proof --allowlist `"$allowPath`" --seconds $OwnerSeconds --trace --no-group-underlay" $binDir
    $ready2 = $null; $deadline = (Get-Date).AddSeconds(15)
    while ((Get-Date) -lt $deadline) {
      $j = Invoke-Native $ownerCopy @("ready") | ConvertFrom-Json
      if ($j.ready -eq $true) { $ready2 = $j; break }
      Start-Sleep -Milliseconds 250
    }
    if ($null -eq $ready2) { Fail-Gu "disabled owner ready timeout" }
    $ownerPid = [int]$ready2.owner.pid; $logPath = "$($ready2.log_path)"
    Rec-Gu "disabled-ready" @{ pid = $ownerPid }
    $null = Set-OwnedForegroundAb $helperCopy $hA "gu-flag-focus"
    Hold-ChordGu @($VK_LWIN, $VK_LSHIFT) "gu-flag-chord"
    try {
      Start-Sleep -Milliseconds 1500
      $inspD = Invoke-Native $ownerCopy @("underlay-inspect") | ConvertFrom-Json
      Rec-Gu "disabled-flag" @{ visible = (Get-UnderlayVisibleCountGu $inspD) }
      if ((Get-UnderlayVisibleCountGu $inspD) -ne 0) { Fail-Gu "disabled flag showed underlay" }
    } finally { Release-ChordGu @($VK_LWIN, $VK_LSHIFT) }
    # Wide-extension alpha composition leg: distinctive translucent red with
    # explicit 32px extension exposes an underlay-only band clear of the
    # yellow ring and DWM shadow. Same points hidden then shown must follow
    # the premultiplied blend; bounds/z/foreground must stay stable.
    $null = Stop-PayloadOwner $ownerCopy
    Start-ExplorerGui $ownerCopy "tile-proof --allowlist `"$allowPath`" --seconds $OwnerSeconds --trace --group-underlay-extension 32 --group-underlay-color #80ff0000" $binDir
    $readyC = $null; $deadline = (Get-Date).AddSeconds(15)
    while ((Get-Date) -lt $deadline) {
      $j = Invoke-Native $ownerCopy @("ready") | ConvertFrom-Json
      if ($j.ready -eq $true) { $readyC = $j; break }
      Start-Sleep -Milliseconds 250
    }
    if ($null -eq $readyC) { Fail-Gu "composition owner ready timeout" }
    $ownerPid = [int]$readyC.owner.pid; $logPath = "$($readyC.log_path)"
    Rec-Gu "composition-ready" @{ pid = $ownerPid }
    $freshA = Set-OwnedForegroundAb $helperCopy $hA "comp-focus"
    Start-Sleep -Milliseconds 1500
    $mark = Get-GuMark $logPath
    Hold-ChordBothGu @($VK_LWIN, $VK_LSHIFT) "comp-chord"
    try {
      $evComp = Wait-UnderlayOutcome $logPath $mark @("shown", "redrew") "" 20 "comp"
      if ("$($evComp.event.color)" -ne "#80ff0000") { Fail-Gu "comp color $($evComp.event.color) != #80ff0000" }
      $unionC = Get-UnionOfFramesGu @([long]$freshA.hwnd, [long]$hB.hwnd)
      $inspC = Invoke-Native $ownerCopy @("underlay-inspect") | ConvertFrom-Json
      if ((Get-UnderlayVisibleCountGu $inspC) -eq 0) { Fail-Gu "comp inspect not visible" }
      $orectC = @($inspC.overlays[0].rect)
      $dpiC = [ActiveBorderNative]::GetDpiForWindow([IntPtr][long]$freshA.hwnd)
      $extPhys = [int][math]::Round(32 * [int]$dpiC / 96.0)
      $padC = 0 + 4 + $extPhys
      $null = Assert-UnionOuterGu $orectC $unionC $padC "comp-exact-wide"
      $fgComp = [ActiveBorderNative]::GetForegroundWindow().ToInt64()
      $ptsC = Get-UnderlayExposedPointsGu $orectC ([long]$freshA.hwnd) ([long]$hB.hwnd) 4 "comp-points"
    } finally { Release-ChordGu @($VK_LWIN, $VK_LSHIFT) }
    $hideC = Wait-UnderlayHiddenGu $ownerCopy 12 "comp-hide"
    $baseC = Read-ScreenPixelsGu $ptsC "comp-base"
    $unionC2 = Get-UnionOfFramesGu @([long]$freshA.hwnd, [long]$hB.hwnd)
    if (($unionC2 -join ",") -ne ($unionC -join ",")) { Fail-Gu "comp union drifted across toggle" }
    $mark = Get-GuMark $logPath
    Hold-ChordBothGu @($VK_LWIN, $VK_LSHIFT) "comp-chord2"
    try {
      $evComp2 = Wait-UnderlayOutcome $logPath $mark @("shown", "redrew") "" 20 "comp2"
      $inspC2 = Invoke-Native $ownerCopy @("underlay-inspect") | ConvertFrom-Json
      $orectC2 = @($inspC2.overlays[0].rect)
      if (($orectC2 -join ",") -ne ($orectC -join ",")) { Fail-Gu "comp outer drifted across toggle" }
      $fgComp2 = [ActiveBorderNative]::GetForegroundWindow().ToInt64()
      if ([uint64]$fgComp2 -ne [uint64]$freshA.hwnd) { Fail-Gu "comp chord stole focus" }
      $uhC = @(Get-UnderlayHwnds $ownerPid "OmniTilerGroupUnderlay") | Where-Object { $_ -ne 0 }
      if (@($uhC).Count -ne 1) { Fail-Gu "comp underlay hwnd count" }
      $null = Assert-BelowAllGu ([long]$uhC[0]) @([long]$freshA.hwnd, [long]$hB.hwnd) "comp-z"
      $shownC = Read-ScreenPixelsGu $ptsC "comp-shown"
    } finally { Release-ChordGu @($VK_LWIN, $VK_LSHIFT) }
    $blend = Assert-UnderlayBlendGu $baseC $shownC @{ r = 255; g = 0; b = 0; a = 128 } "comp-blend"
    Rec-Gu "composition-blend" @{ outer = ($orectC -join ","); union = ($unionC -join ","); pad = $padC; passes = $blend.passes; base = $baseC; shown = $shownC }
    $hideC2 = Wait-UnderlayHiddenGu $ownerCopy 12 "comp-hide2"
    if ([ActiveBorderNative]::SendKey([uint16]$VK_ESC, $false) -ne 1) { Fail-Gu "comp ESC down not inserted" }
    Start-Sleep -Milliseconds 150
    if ([ActiveBorderNative]::SendKey([uint16]$VK_ESC, $true) -ne 1) { Fail-Gu "comp ESC up not inserted" }
    Start-Sleep -Milliseconds 800
    $freshA = Set-OwnedForegroundAb $helperCopy $hA "comp-restore"
    # Exact-owner crash cleanup after actual surface: restart enabled owner, show, emergency-stop.
    $null = Stop-PayloadOwner $ownerCopy
    Start-ExplorerGui $ownerCopy "tile-proof --allowlist `"$allowPath`" --seconds $OwnerSeconds --trace" $binDir
    $ready3 = $null; $deadline = (Get-Date).AddSeconds(15)
    while ((Get-Date) -lt $deadline) {
      $j = Invoke-Native $ownerCopy @("ready") | ConvertFrom-Json
      if ($j.ready -eq $true) { $ready3 = $j; break }
      Start-Sleep -Milliseconds 250
    }
    if ($null -eq $ready3) { Fail-Gu "crash owner ready timeout" }
    $ownerPid = [int]$ready3.owner.pid; $logPath = "$($ready3.log_path)"
    $null = Set-OwnedForegroundAb $helperCopy $hA "gu-crash-focus"
    $mark = Get-GuMark $logPath
    Hold-ChordGu @($VK_LWIN, $VK_LSHIFT) "gu-crash-chord"
    try {
      $evC = Wait-UnderlayOutcome $logPath $mark @("shown", "redrew") "" 20 "gu-crash"
      Rec-Gu "crash-surface" @{ outer = $evC.event.outer }
    } finally { Release-ChordGu @($VK_LWIN, $VK_LSHIFT) }
    # Root-leaf via member removal: close B, chord on single-helper focus
    # must stay hidden with reason root-leaf (one helper cannot parent a
    # group). Covers removal clearing without building a move-subject
    # close framework (moving-subject removal stays a gap).
    try { $null = Invoke-Native $helperCopy @("close", "$($hB.hwnd)", "--tag", "$($hB.tag)") } catch {}
    Start-Sleep -Milliseconds 1200
    $freshA = Set-OwnedForegroundAb $helperCopy $hA "gu-rootleaf-focus"
    $mark = Get-GuMark $logPath
    Hold-ChordBothGu @($VK_LWIN, $VK_LSHIFT) "gu-rootleaf-chord"
    try {
      Start-Sleep -Milliseconds 1500
      $inspR = Invoke-Native $ownerCopy @("underlay-inspect") | ConvertFrom-Json
      Rec-Gu "root-leaf-hidden" @{ visible = (Get-UnderlayVisibleCountGu $inspR) }
      if ((Get-UnderlayVisibleCountGu $inspR) -ne 0) { Fail-Gu "root-leaf single helper showed underlay" }
      $tailR = Get-CompleteLinesAb $logPath
      $foundRoot = $false
      for ($i = $mark; $i -lt $tailR.Count; $i++) {
        if ("$($tailR[$i])".Trim() -eq "") { continue }
        $e = $tailR[$i] | ConvertFrom-Json
        if ("$($e.event)" -eq "group-underlay" -and "$($e.outcome)" -eq "hidden" -and "$($e.reason)" -eq "root-leaf") { $foundRoot = $true; break }
      }
      Rec-Gu "root-leaf-reason" @{ found = $foundRoot }
      if (-not $foundRoot) { Fail-Gu "root-leaf hid without root-leaf reason" }
    } finally { Release-ChordGu @($VK_LWIN, $VK_LSHIFT) }
    $ownerFrozen = $ready3.owner
    $es = Invoke-Native $ownerCopy @("emergency-stop") | ConvertFrom-Json
    Rec-Gu "crash-stop" @{ exited = $es.owner_exited }
    $leftU = @(Get-UnderlayHwnds $ownerPid "OmniTilerGroupUnderlay") | Where-Object { $_ -ne 0 }
    $leftB = @(Get-UnderlayHwnds $ownerPid "OmniTilerActiveBorder") | Where-Object { $_ -ne 0 }
    Rec-Gu "crash-residue" @{ underlay_hwnds = @($leftU).Count; border_hwnds = @($leftB).Count }
    if (@($leftU).Count -ne 0 -or @($leftB).Count -ne 0) { Fail-Gu "crash overlay residue remains" }
    $rs = Invoke-Native $ownerCopy @("restore") | ConvertFrom-Json
    Rec-Gu "crash-restore" @{ restored = $rs.restored }
    Assert-LedgerClean $ownerCopy
    $spi2A = [ActiveBorderNative]::SpiGet(0x0082); $spi2P = [ActiveBorderNative]::SpiGet(0x201E)
    Rec-Gu "settings-post" @{ arranging_raw = $spi2A; pen_raw = $spi2P }
    if ([int]$spi2A -ne 1 -or [int]$spi2P -ne 35) { Fail-Gu "post SPI drift" }
  } catch {
    $guErr = $_
    $partial = @{ status = "fail"; stage = "GroupUnderlayLive"; error = "$($_.Exception.Message)"; steps = $GU_STEPS }
    try { $partial | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $reportPath } catch {}
  }
  finally {
    try { Release-ChordGu @($VK_LWIN, $VK_RWIN, $VK_SHIFT, $VK_LSHIFT, $VK_RSHIFT, $VK_CTRL) } catch {}
    foreach ($c in @($created)) {
      try { $null = Invoke-Native $helperCopy @("close", "$($c.hwnd)", "--tag", "$($c.tag)") } catch {}
    }
    try {
      $p = Invoke-Native $ownerCopy @("stop") | ConvertFrom-Json
      $r = Invoke-Native $ownerCopy @("restore") | ConvertFrom-Json
      Rec-Gu "final-stop" @{ exited = $p.owner_exited; restored = $r.restored }
    } catch {}
  }
  # Exact end audit: runs on success AND failure (after finally cleanup).
  # Scoped only: path-bound actor check plus exact-owner overlay classes;
  # no global enumeration, no ledger/stop/workspace residue, SPI raw1/raw35.
  $procs = @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $ownerCopy -or $_.Path -eq $helperCopy })
  $lu = @(); $lb = @()
  try { $lu = @(Get-UnderlayHwnds $ownerPid "OmniTilerGroupUnderlay") | Where-Object { $_ -ne 0 } } catch {}
  try { $lb = @(Get-UnderlayHwnds $ownerPid "OmniTilerActiveBorder") | Where-Object { $_ -ne 0 } } catch {}
  $ledgerOk = $true; try { Assert-LedgerClean $ownerCopy } catch { $ledgerOk = $false }
  Rec-Gu "end-audit" @{ actors = @($procs).Count; underlay_hwnds = @($lu).Count; border_hwnds = @($lb).Count; ledger_clean = $ledgerOk }
  if ($null -ne $guErr) {
    $fail = @{ status = "fail"; stage = "GroupUnderlayLive"; error = "$($guErr.Exception.Message)"; steps = $GU_STEPS }
    try { $fail | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $reportPath } catch {}
    throw $guErr
  }
  if (@($procs).Count -ne 0) { Fail-Gu "end actors remain" }
  if (@($lu).Count -ne 0 -or @($lb).Count -ne 0) { Fail-Gu "end overlay residue" }
  if (-not $ledgerOk) { Fail-Gu "end ledger not clean" }
  Assert-LedgerClean $ownerCopy
  $report = @{ status = "pass"; stage = "GroupUnderlayLive"; steps = $GU_STEPS }
  $report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $reportPath
  $report | ConvertTo-Json -Depth 10 | Write-Output
}

if ($Mock) { Invoke-GuMock; exit 0 }
if ($Live) { Invoke-GuLive; exit 0 }
if ($Stop) {
  if ($RunDir -eq "") { throw "refuse: -RunDir required for -Stop" }
  $bin = Join-Path $RunDir "bin\tiler-windows.exe"
  $p = Invoke-Native $bin @("stop") | ConvertFrom-Json
  $r = Invoke-Native $bin @("restore") | ConvertFrom-Json
  Write-Output (@{ stop = $p; restore = $r } | ConvertTo-Json -Depth 6)
  exit 0
}
throw "usage: windows-group-underlay.ps1 -Mock | -Live [-OwnerSeconds N] | -Stop -RunDir <dir>"
