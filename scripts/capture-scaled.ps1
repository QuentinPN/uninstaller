<#
.SYNOPSIS
  Screenshots of the release build at 100/125/150 % display scaling (end of every phase).
.DESCRIPTION
  Windows scaling is a system setting, so it is simulated: WebView2 renders at the scale
  factor (WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS) and the window gets the physical size
  Windows would give the 1200x760 logical window. The window opens off-screen and is
  captured with PrintWindow: no focus change, the user keeps the mouse and keyboard.
  PNGs go to artifacts/screens/ (ignored).
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
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type @'
using System; using System.Runtime.InteropServices;
public static class W {
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint f);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out RECT r, int s);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
}
'@

foreach ($scale in 1, 1.25, 1.5) {
  $out = Join-Path $outDir "$Name-$([int]($scale * 100)).png"
  $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--force-device-scale-factor=$scale"
  # The window shows where Tauri puts it, then moves off-screen (past the right edge of
  # every monitor) without activation: SWP_NOACTIVATE | SWP_SHOWWINDOW.
  $p = Start-Process $Exe -PassThru
  while ($p.MainWindowHandle -eq 0) { Start-Sleep -Milliseconds 50; $p.Refresh() }
  $h = $p.MainWindowHandle
  $x = [Windows.Forms.SystemInformation]::VirtualScreen.Right + 100
  [void][W]::SetWindowPos($h, [IntPtr]::Zero, $x, 0, [int](1200 * $scale), [int](760 * $scale), 0x0050)
  Start-Sleep -Seconds 4
  # PW_RENDERFULLCONTENT captures the WebView2 content without the window on screen.
  $w = New-Object W+RECT
  [void][W]::GetWindowRect($h, [ref]$w)
  $full = New-Object Drawing.Bitmap ($w.R - $w.L), ($w.B - $w.T)
  $g = [Drawing.Graphics]::FromImage($full)
  $dc = $g.GetHdc()
  [void][W]::PrintWindow($h, $dc, 2)
  $g.ReleaseHdc($dc); $g.Dispose()
  # DWMWA_EXTENDED_FRAME_BOUNDS: crop the invisible resize border.
  $r = New-Object W+RECT
  [void][W]::DwmGetWindowAttribute($h, 9, [ref]$r, 16)
  $bmp = $full.Clone((New-Object Drawing.Rectangle ($r.L - $w.L), ($r.T - $w.T), ($r.R - $r.L), ($r.B - $r.T)), $full.PixelFormat)
  $bmp.Save($out, [Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose(); $full.Dispose()
  "$out : $($r.R - $r.L)x$($r.B - $r.T)"
  [void]$p.CloseMainWindow()
  if (-not $p.WaitForExit(5000)) { Stop-Process -Id $p.Id -Force }
  Start-Sleep -Seconds 2
}
Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
