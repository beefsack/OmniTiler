# Prove the fixture-only owner's PMv2 scale on empty outputs. No display mutations.
[CmdletBinding()]
param([Parameter(Mandatory)][switch]$Live)

if (-not $Live) { throw 'explicit -Live required' }
. (Join-Path $PSScriptRoot 'windows-phase1-proofs.ps1') -DefinitionsOnly
Initialize-P1Artifacts 'output-dpi'

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;
public static class EmptyOutputDpiProof {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int L,T,R,B; }
    public class Output {
        public int X,Y,W,H;
        public uint Dpi;
    }
    private delegate bool MonitorCallback(IntPtr monitor, IntPtr dc, ref Rect r, IntPtr data);
    [DllImport("user32.dll", SetLastError=true)] private static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll", SetLastError=true)] private static extern bool EnumDisplayMonitors(IntPtr dc, IntPtr clip, MonitorCallback callback, IntPtr data);
    [DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)] private static extern IntPtr CreateWindowExW(uint exstyle, string cls, string name, uint style, int x, int y, int w, int h, IntPtr parent, IntPtr menu, IntPtr module, IntPtr data);
    [DllImport("user32.dll")] private static extern IntPtr MonitorFromWindow(IntPtr window, uint flags);
    [DllImport("user32.dll")] private static extern uint GetDpiForWindow(IntPtr window);
    [DllImport("user32.dll")] private static extern bool DestroyWindow(IntPtr window);
    public static Output[] Sample() {
        var previous = SetThreadDpiAwarenessContext(new IntPtr(-4));
        if (previous == IntPtr.Zero) throw new Win32Exception();
        var outputs = new List<Output>();
        try {
            Exception fault = null;
            MonitorCallback sample = delegate(IntPtr monitor, IntPtr dc, ref Rect r, IntPtr data) {
                try {
                    // OS STATIC control, never shown/activated. Use the center so
                    // the physical output join is unambiguous at negative origins too.
                    var w = r.R - r.L; var h = r.B - r.T;
                    var hwnd = CreateWindowExW(0x08000080, "STATIC", null, 0x80000000,
                        r.L + w / 2, r.T + h / 2, 1, 1, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero);
                    if (hwnd == IntPtr.Zero) throw new Win32Exception();
                    try {
                        var dpi = GetDpiForWindow(hwnd);
                        if (dpi == 0 || MonitorFromWindow(hwnd, 0) != monitor) throw new InvalidOperationException("observer did not join the enumerated output");
                        outputs.Add(new Output { X = r.L, Y = r.T, W = w, H = h, Dpi = dpi });
                    } finally {
                        if (!DestroyWindow(hwnd)) throw new Win32Exception();
                    }
                    return true;
                } catch (Exception e) { fault = e; return false; }
            };
            var ok = EnumDisplayMonitors(IntPtr.Zero, IntPtr.Zero, sample, IntPtr.Zero);
            if (fault != null) throw fault;
            if (!ok || outputs.Count == 0) throw new Win32Exception();
            return outputs.ToArray();
        } finally { SetThreadDpiAwarenessContext(previous); }
    }
}
'@

$runDir = Join-Path $script:P1TargetDir "task9/output-dpi-$((Get-Date -Format 'yyyyMMdd-HHmmss-fff'))-$PID"
New-Item -ItemType Directory -Path $runDir -Force | Out-Null
$ctx = New-P1Context $script:P1FixtureCopy $script:P1SettingsPath
$proof = [ordered]@{
  commit = (& git -C $script:P1Repo rev-parse HEAD)
  owner_sha256 = (Get-FileHash -LiteralPath $script:P1OwnerCopy -Algorithm SHA256).Hash
  fixture_sha256 = (Get-FileHash -LiteralPath $script:P1FixtureCopy -Algorithm SHA256).Hash
}
$failure = $null
try {
  $ctx.LedgerDir = Assert-P1CleanStart 'output-dpi'
  $ctx.GroupOrig = Save-LiveSettingsPreimage $script:P1SettingsPath (Join-Path $runDir 'settings-orig.json') (Join-Path $runDir 'taskbar-spi.json') ''
  $ctx.SettingsConfigIntended = $true
  $proof.native = @([EmptyOutputDpiProof]::Sample())
  $boot = Start-P1FixtureAndOwner $script:P1FixtureCopy $ctx 'output-dpi'
  $proof.owner = $boot.launch.bind
  $proof.owner_trace = $boot.launch.log
  $empty = Wait-P1SettledWorld $boot.launch.log 0 {
    param($e)
    return ($e.geometry_status -eq 'paired' -and $e.desired_geometry_omitted -eq 0 -and
      @($e.desired_geometry).Count -eq $proof.native.Count -and @($e.desired_windows).Count -eq 0 -and
      $e.desired_windows_omitted -eq 0)
  } 15 'empty output geometry'
  $proof.owner_report = $empty.event
  foreach ($output in $proof.native) {
    $rows = @($empty.event.desired_geometry | Where-Object {
      $_.full.x -eq $output.X -and $_.full.y -eq $output.Y -and $_.full.w -eq $output.W -and $_.full.h -eq $output.H
    })
    if ($rows.Count -ne 1) { throw 'native output does not join exactly one complete owner row' }
    $row = $rows[0]
    if ($row.complete -ne $true -or $row.scale -ne ($output.Dpi / 96.0) -or $row.identity.connector -ne $true) {
      throw "empty output $($row.output) did not publish its native DPI and connector presence"
    }
  }
} catch { $failure = $_.Exception.Message }
finally {
  try { Invoke-StressCleanup $ctx 'output-dpi'; $proof.cleanup = 'ok' }
  catch { $proof.cleanup = $_.Exception.Message; if ($null -eq $failure) { $failure = $proof.cleanup } }
  $proof.error = $failure
  $proof.status = if ($null -eq $failure) { 'pass' } else { 'fail' }
  $evidence = Join-Path $runDir 'evidence.json'
  $proof | ConvertTo-Json -Depth 16 | Set-Content -LiteralPath $evidence -Encoding utf8
  Write-Output $evidence
  if ($null -ne $failure) { throw "output DPI proof failed: $failure" }
}
