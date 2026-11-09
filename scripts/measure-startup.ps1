<#
.SYNOPSIS
  Cold-start and memory measurement of the release build (docs/perf.md).
.DESCRIPTION
  Starts the app N times. Per run: time until the main window exists, then, after a
  settle delay, working set and private bytes summed over the app and every process
  it spawned (WebView2 browser, renderer, GPU...), plus "own" private bytes: the app
  process and the renderer, the part our code drives (PRODUCT.md budget). Prints one
  Markdown row per run, then the medians.
.EXAMPLE
  npx tauri build --no-bundle; ./scripts/measure-startup.ps1
#>
param(
  [string]$Exe = "$PSScriptRoot\..\src-tauri\target\release\uninstaller.exe",
  [int]$Runs = 5,
  [int]$SettleSeconds = 5
)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path $Exe)) { throw "Introuvable : $Exe (lancer d'abord npx tauri build --no-bundle)" }

# The app and all its descendants, found through parent process ids. Own = the app
# itself or a WebView2 renderer (--type=renderer); the rest is the WebView2 base.
function Get-ProcessTree([int]$RootId) {
  $all = Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, CommandLine
  $ids = [System.Collections.Generic.List[int]]::new()
  $ids.Add($RootId)
  for ($i = 0; $i -lt $ids.Count; $i++) {
    foreach ($p in $all) {
      if ($p.ParentProcessId -eq $ids[$i] -and -not $ids.Contains([int]$p.ProcessId)) { $ids.Add([int]$p.ProcessId) }
    }
  }
  foreach ($p in $all | Where-Object { $ids.Contains([int]$_.ProcessId) }) {
    $proc = Get-Process -Id $p.ProcessId -ErrorAction SilentlyContinue
    if ($proc) {
      $proc | Add-Member Own ($p.ProcessId -eq $RootId -or $p.CommandLine -match '--type=renderer') -PassThru
    }
  }
}

function Get-Median([double[]]$Values) {
  $sorted = $Values | Sort-Object
  $sorted[[int][math]::Floor($sorted.Count / 2)]
}

$rows = foreach ($run in 1..$Runs) {
  $clock = [Diagnostics.Stopwatch]::StartNew()
  $proc = Start-Process $Exe -PassThru
  while ($proc.MainWindowHandle -eq 0) {
    if ($clock.Elapsed.TotalSeconds -gt 20) { Stop-Process -Id $proc.Id -Force; throw "Aucune fenêtre après 20 s" }
    Start-Sleep -Milliseconds 5
    $proc.Refresh()
  }
  $windowMs = $clock.ElapsedMilliseconds
  Start-Sleep -Seconds $SettleSeconds
  $tree = @(Get-ProcessTree $proc.Id)
  $row = [pscustomobject]@{
    Run       = $run
    WindowMs  = $windowMs
    Processes = $tree.Count
    WorkingMB = [math]::Round(($tree | Measure-Object WorkingSet64 -Sum).Sum / 1MB, 1)
    PrivateMB = [math]::Round(($tree | Measure-Object PrivateMemorySize64 -Sum).Sum / 1MB, 1)
    OwnMB     = [math]::Round(($tree | Where-Object Own | Measure-Object PrivateMemorySize64 -Sum).Sum / 1MB, 1)
  }
  [void]$proc.CloseMainWindow()
  if (-not $proc.WaitForExit(5000)) { $tree | Stop-Process -Force -ErrorAction SilentlyContinue }
  Start-Sleep -Seconds 2
  $row
}

"| Lancement | Fenêtre (ms) | Processus | Working set (Mo) | Privé total (Mo) | Privé propre (Mo) |"
"|---|---|---|---|---|---|"
foreach ($r in $rows) { "| $($r.Run) | $($r.WindowMs) | $($r.Processes) | $($r.WorkingMB) | $($r.PrivateMB) | $($r.OwnMB) |" }
"| **Médiane** | **$(Get-Median $rows.WindowMs)** | | **$(Get-Median $rows.WorkingMB)** | **$(Get-Median $rows.PrivateMB)** | **$(Get-Median $rows.OwnMB)** |"
