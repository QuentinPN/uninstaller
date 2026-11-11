# The fixture's own uninstaller, run from its install folder through UninstallString.
# Like many real ones it is incomplete on purpose: it removes its program and its
# Uninstall entry and Run value, but leaves its log folder, its data, its shortcut,
# its key and its task for the leftover scan to find.
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\names.ps1"

Get-ChildItem $Install -File | Remove-Item
$Hkcu.DeleteSubKeyTree($UninstallKey, $false)
Remove-RunValue
