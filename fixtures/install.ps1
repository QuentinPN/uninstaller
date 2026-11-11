# Installs the fixture program for the current user (TESTING.md, level 3): files, its
# data, a Start-menu shortcut, an Uninstall entry, a Run value, a key of its own and a
# disabled task. Its program is a copy of rundll32.exe, which does nothing without
# arguments, so the Run value and the task are harmless if they ever start it.
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\names.ps1"

New-Item -ItemType Directory -Force (Join-Path $Install 'logs'), $Data | Out-Null
Copy-Item "$env:SystemRoot\System32\rundll32.exe" $Exe
Copy-Item "$PSScriptRoot\names.ps1", "$PSScriptRoot\uninstall.ps1" $Install
Set-Content (Join-Path $Install 'logs\app.log') 'started' -Encoding ascii
Set-Content (Join-Path $Data 'settings.ini') "[window]`r`nwidth=800" -Encoding ascii

$shell = New-Object -ComObject WScript.Shell
$link = $shell.CreateShortcut($Shortcut)
$link.TargetPath = $Exe
$link.Save()

$entry = $Hkcu.CreateSubKey($UninstallKey)
$entry.SetValue('DisplayName', $Name)
$entry.SetValue('DisplayVersion', '1.0.0')
$entry.SetValue('Publisher', 'Uninstaller Fixture Labs')
$entry.SetValue('InstallLocation', $Install)
$entry.SetValue('DisplayIcon', $Exe)
$entry.SetValue('UninstallString',
    "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$Install\uninstall.ps1`"")
$entry.SetValue('EstimatedSize', 64, [Microsoft.Win32.RegistryValueKind]::DWord)
$entry.SetValue('NoModify', 1, [Microsoft.Win32.RegistryValueKind]::DWord)
$entry.Close()

$run = $Hkcu.CreateSubKey($RunKey)
$run.SetValue($Name, "`"$Exe`"")
$run.Close()

$own = $Hkcu.CreateSubKey($OwnKey)
$own.SetValue('Theme', 'dark')
$own.Close()

$xml = @"
<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <Principals><Principal id="Author"><LogonType>InteractiveToken</LogonType></Principal></Principals>
  <Settings><Enabled>false</Enabled></Settings>
  <Actions Context="Author"><Exec><Command>$Exe</Command></Exec></Actions>
</Task>
"@
Register-ScheduledTask -TaskName $Task -Xml $xml -Force | Out-Null
