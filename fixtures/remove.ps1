# Removes everything the fixture can have left, whatever state a test stopped in.
# Touches only the names in names.ps1; running it twice is harmless.
. "$PSScriptRoot\names.ps1"

foreach ($path in $Install, $Data, $Shortcut) {
    if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Recurse -Force }
}
$Hkcu.DeleteSubKeyTree($UninstallKey, $false)
$Hkcu.DeleteSubKeyTree($OwnKey, $false)
Remove-RunValue
Unregister-ScheduledTask -TaskName $Task -Confirm:$false -ErrorAction SilentlyContinue
