<#
.SYNOPSIS
  Screenshots of the release build at 100/125/150 % display scaling (end of every phase).
.DESCRIPTION
  Windows scaling is a system setting, so it is simulated: WebView2 renders at the scale
  factor (WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS) and the window gets the physical size
  Windows would give the 1200x760 logical window. PNGs go to artifacts/screens/ (ignored).
.EXAMPLE
  ./scripts/capture-scaled.ps1 -Name phase0
#>
param(
  [string]$Name = 'capture',
  [string]$Exe = "$PSScriptRoot\..\src-tauri\target\release\uninstaller.exe"
)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path $Exe)) { throw "Introuvable : $Exe (lancer d'abord npx tauri build --no-bundle)" }
$outDir = New-Item -ItemType Directory -Force "$PSScriptRoot\..\artifacts\screens"
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System; using System.Runtime.InteropServices;
public static class W {
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out RECT r, int s);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
}
'@

foreach ($scale in 1, 1.25, 1.5) {
  $out = Join-Path $outDir "$Name-$([int]($scale * 100)).png"
  $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--force-device-scale-factor=$scale"
  $p = Start-Process $Exe -PassThru
  while ($p.MainWindowHandle -eq 0) { Start-Sleep -Milliseconds 50; $p.Refresh() }
  $h = $p.MainWindowHandle
  # SWP_SHOWWINDOW; physical size = logical size x scale.
  [void][W]::SetWindowPos($h, [IntPtr]::Zero, 40, 40, [int](1200 * $scale), [int](760 * $scale), 0x0040)
  [void][W]::SetForegroundWindow($h)
  Start-Sleep -Seconds 4
  # DWMWA_EXTENDED_FRAME_BOUNDS: visible frame, without the invisible resize border.
  $r = New-Object W+RECT
  [void][W]::DwmGetWindowAttribute($h, 9, [ref]$r, 16)
  $bmp = New-Object Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
  $g = [Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
  $bmp.Save($out, [Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  "$out : $($r.R - $r.L)x$($r.B - $r.T)"
  [void]$p.CloseMainWindow()
  if (-not $p.WaitForExit(5000)) { Stop-Process -Id $p.Id -Force }
  Start-Sleep -Seconds 2
}
Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
