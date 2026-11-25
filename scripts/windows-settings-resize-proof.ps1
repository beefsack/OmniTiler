# Bounded native Settings resize on a fresh owned process. No input or Apply.
[CmdletBinding()]
param([Parameter(Mandatory)][switch]$Live)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $Live) { throw 'explicit -Live required' }
$repo = Split-Path -Parent $PSScriptRoot
$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $repo 'target' }
$target = (Resolve-Path -LiteralPath $target).Path
$exe = (Resolve-Path -LiteralPath (Join-Path $target 'debug/tiler-windows.exe')).Path
$settings = Join-Path $env:LOCALAPPDATA 'omnitiler/settings.json'
function Get-SettingsHash {
  if (Test-Path -LiteralPath $settings -PathType Leaf) { (Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash }
}

Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
public static class SettingsResizeProof {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int L,T,R,B; }
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X,Y; }
    [StructLayout(LayoutKind.Sequential)] public struct Tracks { public Point Reserved, MaxSize, MaxPos, Min, Max; }
    [DllImport("user32.dll", CharSet=CharSet.Unicode, ExactSpelling=true)] public static extern IntPtr FindWindowW(string c, IntPtr title);
    [DllImport("user32.dll", CharSet=CharSet.Unicode, ExactSpelling=true)] public static extern IntPtr FindWindowExW(IntPtr parent, IntPtr after, string c, IntPtr title);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] public static extern IntPtr GetWindowLongPtrW(IntPtr h, int index);
    [DllImport("user32.dll")] private static extern uint GetDpiForWindow(IntPtr h);
    [DllImport("user32.dll")] private static extern int GetSystemMetricsForDpi(int index, uint dpi);
    [DllImport("user32.dll")] private static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll", SetLastError=true)] private static extern bool GetWindowRect(IntPtr h, out Rect r);
    [DllImport("user32.dll", SetLastError=true)] private static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int w, int height, uint flags);
    [DllImport("user32.dll", EntryPoint="SendMessageTimeoutW", SetLastError=true)] private static extern IntPtr SendTracks(IntPtr h, uint msg, IntPtr w, ref Tracks tracks, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll", SetLastError=true)] public static extern bool PostMessageW(IntPtr h, uint msg, IntPtr w, IntPtr l);
    public static int[] Read(IntPtr h) {
        var prev = SetThreadDpiAwarenessContext(new IntPtr(-4));
        if (prev == IntPtr.Zero) throw new Win32Exception();
        try {
            Rect r;
            if (!GetWindowRect(h, out r)) throw new Win32Exception();
            return new int[] { r.L, r.T, r.R - r.L, r.B - r.T };
        } finally { SetThreadDpiAwarenessContext(prev); }
    }
    public static void Resize(IntPtr h, int w, int height) {
        var prev = SetThreadDpiAwarenessContext(new IntPtr(-4));
        if (prev == IntPtr.Zero) throw new Win32Exception();
        try {
            // NOSENDCHANGING is deliberately absent: Settings gets the real resize.
            // ASYNCWINDOWPOS bounds the sender; readback—not post acceptance—proves it.
            if (!SetWindowPos(h, IntPtr.Zero, 0, 0, w, height, 0x4016)) throw new Win32Exception();
        } finally { SetThreadDpiAwarenessContext(prev); }
    }
    public static int[] ReadTracks(IntPtr h) {
        // The sender normally provides the system defaults before the WndProc
        // optionally substitutes application tracks. Mirror that initial state.
        var dpi = GetDpiForWindow(h);
        if (dpi == 0) throw new Win32Exception();
        Tracks data = new Tracks {
            Min = new Point { X = GetSystemMetricsForDpi(34, dpi), Y = GetSystemMetricsForDpi(35, dpi) },
            Max = new Point { X = GetSystemMetricsForDpi(59, dpi), Y = GetSystemMetricsForDpi(60, dpi) }
        };
        IntPtr result;
        if (SendTracks(h, 0x24, IntPtr.Zero, ref data, 0x2, 2000, out result) == IntPtr.Zero) throw new Win32Exception();
        return new int[] {data.Min.X, data.Min.Y, data.Max.X, data.Max.Y};
    }
}
'@

$class = 'OmniTilerSettings'
if ([SettingsResizeProof]::FindWindowW($class, [IntPtr]::Zero) -ne [IntPtr]::Zero) {
  throw 'Settings already open; no launch or resize performed'
}
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss-fff'
$runDir = Join-Path $target "task9/settings-resize-$stamp-$PID"
New-Item -ItemType Directory -Path $runDir -Force | Out-Null
$original = Get-SettingsHash
$proof = [ordered]@{
  commit = (& git -C $repo rev-parse HEAD)
  owner_sha256 = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
  args = @('settings'); settings_sha_before = $original
}
$child = Start-Process -FilePath $exe -ArgumentList 'settings' -PassThru
$started = $child.StartTime.ToUniversalTime().ToFileTimeUtc()
$hwnd = [IntPtr]::Zero
$failure = $null
function Test-OwnedSettings([IntPtr]$Window) {
  if ($Window -eq [IntPtr]::Zero) { return $false }
  [uint32]$ownerPid = 0
  $null = [SettingsResizeProof]::GetWindowThreadProcessId($Window, [ref]$ownerPid)
  if ($ownerPid -ne $child.Id) { return $false }
  $existing = Get-Process -Id $ownerPid -ErrorAction SilentlyContinue
  return ($null -ne $existing -and $existing.Path -ieq $exe -and
    $existing.StartTime.ToUniversalTime().ToFileTimeUtc() -eq $started)
}
try {
  $deadline = (Get-Date).AddSeconds(10)
  while ((Get-Date) -lt $deadline) {
    $candidate = [SettingsResizeProof]::FindWindowW($class, [IntPtr]::Zero)
    if (Test-OwnedSettings $candidate) { $hwnd = $candidate; break }
    if ($child.HasExited) { throw "Settings exited before creating an owned HWND ($($child.ExitCode))" }
    Start-Sleep -Milliseconds 50
  }
  if (-not (Test-OwnedSettings $hwnd)) { throw 'owned Settings window did not appear' }
  $tab = [SettingsResizeProof]::FindWindowExW($hwnd, [IntPtr]::Zero, 'SysTabControl32', [IntPtr]::Zero)
  if (-not (Test-OwnedSettings $tab)) { throw 'owned Settings tab strip did not initialize' }
  $proof.process = @{ pid = $child.Id; start_filetime = "$started"; hwnd = $hwnd.ToInt64() }
  $proof.style = [SettingsResizeProof]::GetWindowLongPtrW($hwnd, -16).ToInt64()
  if (($proof.style -band 0x00040000) -eq 0) { throw 'Settings lacks WS_THICKFRAME' }
  $proof.before = [SettingsResizeProof]::Read($hwnd)
  $proof.tab_before = [SettingsResizeProof]::Read($tab)
  $proof.tracks = [SettingsResizeProof]::ReadTracks($hwnd)
  $requested = @([int]($proof.before[2] + 136), [int]($proof.before[3] + 92))
  $proof.requested = $requested
  if ($proof.tracks[0] -ge $proof.tracks[2] -or $proof.tracks[1] -ge $proof.tracks[3] -or
      $requested[0] -gt $proof.tracks[2] -or $requested[1] -gt $proof.tracks[3]) {
    throw 'Settings does not expose distinct usable native min/max resize tracks'
  }
  if (-not (Test-OwnedSettings $hwnd)) { throw 'Settings window lifetime changed before resize' }
  [SettingsResizeProof]::Resize($hwnd, $requested[0], $requested[1])
  $deadline = (Get-Date).AddSeconds(5)
  do {
    if (-not ((Test-OwnedSettings $hwnd) -and (Test-OwnedSettings $tab))) { throw 'Settings lifetime changed during resize' }
    $proof.after = [SettingsResizeProof]::Read($hwnd)
    $proof.tab_after = [SettingsResizeProof]::Read($tab)
    if ($proof.after[2] -eq $requested[0] -and $proof.after[3] -eq $requested[1] -and
        $proof.tab_after[2] -gt $proof.tab_before[2] -and $proof.tab_after[3] -gt $proof.tab_before[3]) { break }
    Start-Sleep -Milliseconds 50
  } while ((Get-Date) -lt $deadline)
  if ($proof.after[2] -ne $requested[0] -or $proof.after[3] -ne $requested[1] -or
      $proof.tab_after[2] -le $proof.tab_before[2] -or $proof.tab_after[3] -le $proof.tab_before[3]) {
    throw 'outer and tab geometry did not settle at the enlarged layout'
  }
} catch {
  $failure = $_.Exception.Message
} finally {
  try {
    if (Test-OwnedSettings $hwnd) { $null = [SettingsResizeProof]::PostMessageW($hwnd, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) }
    if (-not $child.WaitForExit(5000)) {
      # Process handle obtained by the launch; never close or stop a peer by name.
      $child.Kill()
      if (-not $child.WaitForExit(5000)) { throw 'owned Settings process did not exit' }
      throw 'Settings needed forced cleanup'
    }
    $proof.exit_code = $child.ExitCode
    if ($proof.exit_code -ne 0) { throw "Settings exit $($proof.exit_code)" }
  } catch { $proof.cleanup_error = $_.Exception.Message }
  $proof.settings_sha_after = Get-SettingsHash
  if ($proof.settings_sha_after -ne $original) { $proof.cleanup_error = 'settings bytes changed during the proof' }
  $proof.error = $failure
  $proof.status = if ($null -eq $failure -and -not $proof.Contains('cleanup_error')) { 'pass' } else { 'fail' }
  $evidence = Join-Path $runDir 'evidence.json'
  $proof | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $evidence -Encoding utf8
  Write-Output $evidence
  if ($proof.status -ne 'pass') { throw "Settings resize proof failed: $failure $($proof['cleanup_error'])" }
}
