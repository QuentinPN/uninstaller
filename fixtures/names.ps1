# Every place the fixture program touches, shared by its three scripts. All per user:
# nothing here needs the administrator, and nothing outside these names is written.
$Name = 'UninstallerFixture'
$Install = Join-Path $env:LOCALAPPDATA "Programs\$Name"
$Exe = Join-Path $Install "$Name.exe"
$Data = Join-Path $env:APPDATA $Name
$Shortcut = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\$Name.lnk"
$UninstallKey = "Software\Microsoft\Windows\CurrentVersion\Uninstall\$Name"
$RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run'
$OwnKey = "Software\$Name"
# The one task name the tests may create (see the scheduled-task test in quarantine.rs).
$Task = 'UninstallerTest'
$Hkcu = [Microsoft.Win32.Registry]::CurrentUser

function Remove-RunValue {
    $run = $Hkcu.OpenSubKey($RunKey, $true)
    if ($run) {
        $run.DeleteValue($Name, $false)
        $run.Close()
    }
}
