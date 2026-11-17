# ts6rpc installer for Windows — Discord Rich Presence for TeamSpeak 6
#
#   Install / update (PowerShell):
#     irm https://raw.githubusercontent.com/wildesPepega/ts6rpc/main/install.ps1 | iex
#   Uninstall:
#     & ([scriptblock]::Create((irm https://raw.githubusercontent.com/wildesPepega/ts6rpc/main/install.ps1))) -Uninstall
#
# Installs for the current user only (no admin needed):
#   %LOCALAPPDATA%\Programs\ts6rpc\ts6rpc.exe, a Start menu shortcut and autostart.
param(
    [switch]$Uninstall,
    [switch]$NoAutostart
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'  # makes Invoke-WebRequest much faster on PowerShell 5

$Repo = 'wildesPepega/ts6rpc'
$Dir = Join-Path $env:LOCALAPPDATA 'Programs\ts6rpc'
$Exe = Join-Path $Dir 'ts6rpc.exe'
$Shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'ts6rpc.lnk'
$RunKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'

function Stop-Ts6rpc {
    $p = Get-Process -Name ts6rpc -ErrorAction SilentlyContinue
    if ($p) {
        Write-Host ':: Stopping the running ts6rpc'
        $p | Stop-Process -Force
        Start-Sleep -Milliseconds 800
    }
}

if ($Uninstall) {
    Stop-Ts6rpc
    Remove-ItemProperty -Path $RunKey -Name ts6rpc -ErrorAction SilentlyContinue
    Remove-Item -Force -ErrorAction SilentlyContinue $Shortcut
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $Dir
    Write-Host ":: ts6rpc uninstalled. Your settings are kept in $env:APPDATA\ts6rpc (delete it to remove them too)."
    return
}

if (-not [Environment]::Is64BitOperatingSystem) {
    throw 'ts6rpc needs 64-bit Windows.'
}

# PowerShell 5 may default to TLS 1.0, which GitHub rejects.
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$Tmp = Join-Path $env:TEMP "ts6rpc-$PID.exe"
Write-Host ':: Downloading ts6rpc (latest release)'
Invoke-WebRequest -UseBasicParsing -Uri "https://github.com/$Repo/releases/latest/download/ts6rpc-windows-x86_64.exe" -OutFile $Tmp

Stop-Ts6rpc
New-Item -ItemType Directory -Force -Path $Dir | Out-Null
Move-Item -Force $Tmp $Exe
Write-Host ":: Installed to $Exe"

$ws = New-Object -ComObject WScript.Shell
$lnk = $ws.CreateShortcut($Shortcut)
$lnk.TargetPath = $Exe
$lnk.Description = 'Discord Rich Presence for TeamSpeak 6'
$lnk.Save()

if (-not $NoAutostart) {
    Set-ItemProperty -Path $RunKey -Name ts6rpc -Value "`"$Exe`""
    Write-Host ':: Starts automatically when you log in (switch off in the tray menu: "Start on login")'
}

Start-Process -FilePath $Exe
Write-Host ':: ts6rpc is running. Look for its icon in the notification area (it may be under the ^ arrow).'
Write-Host ''
Write-Host 'On the first start, TeamSpeak asks whether to allow "Discord Rich Presence". Click allow.'
Write-Host "Settings: $env:APPDATA\ts6rpc\config.json"
