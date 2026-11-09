<#
Sysglow nach C:\Program Files\Sysglow installieren, aktualisieren oder entfernen.
Fragt beim Start per Windows-Abfrage nach Administratorrechten. Die braucht nur das Kopieren,
Sysglow selbst laeuft danach ohne.

Parameter:
  -Entfernen   Programmordner und Startmenue-Eintrag loeschen, dazu Autostart und eigene Verknuepfung
               des Kontos, das das Skript startet. Die ini in %APPDATA%\Sysglow bleibt.
  (ohne)       Neueste Version von GitHub holen, Pruefsumme kontrollieren, installieren.

Changelog:
09.11.2026 08:44 - erstellt
09.11.2026 20:59 - -Entfernen raeumt Autostart und eigene Startmenue-Verknuepfung mit auf
09.11.2026 21:02 - fragt selbst nach Administratorrechten
#>
param([switch]$Entfernen)

Clear-Host
$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

Write-Host 'Parameter: -Entfernen = deinstallieren, ohne = installieren oder aktualisieren'

$rolle = [Security.Principal.WindowsBuiltInRole]::Administrator
$ich = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $ich.IsInRole($rolle)) {
    Write-Host 'Zum Kopieren in den Programme-Ordner fragt Windows jetzt nach Administratorrechten.'
    $argumente = @('-NoProfile', '-NoExit', '-ExecutionPolicy', (Get-ExecutionPolicy).ToString(), '-File', "`"$PSCommandPath`"")
    if ($Entfernen) { $argumente += '-Entfernen' }
    try { Start-Process powershell -Verb RunAs -ArgumentList $argumente }
    catch { throw 'Administratorrechte abgelehnt, nichts geaendert.' }
    return
}

$ordner = Join-Path $env:ProgramFiles 'Sysglow'
$exe = Join-Path $ordner 'Sysglow.exe'
$link = Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs\Sysglow.lnk'

function Stop-Sysglow {
    Get-Process -Name 'Sysglow' -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -eq $exe } |
        ForEach-Object { Stop-Process -Id $_.Id -Force; $_.WaitForExit(5000) | Out-Null }
}

function Remove-BenutzerEintraege([string]$runPfad, [string]$linkPfad) {
    $run = (Get-ItemProperty $runPfad -Name 'Sysglow' -ErrorAction SilentlyContinue).Sysglow
    if ($run -and $run.Trim('"') -eq $exe) { Remove-ItemProperty $runPfad -Name 'Sysglow' }
    if (Test-Path $linkPfad) {
        $ziel = (New-Object -ComObject WScript.Shell).CreateShortcut($linkPfad).TargetPath
        if ($ziel -eq $exe) { Remove-Item $linkPfad -Force }
    }
}

if ($Entfernen) {
    Stop-Sysglow
    Remove-BenutzerEintraege 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' (Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Sysglow.lnk')
    if (Test-Path $link) { Remove-Item $link -Force }
    if (Test-Path $ordner) { Remove-Item $ordner -Recurse -Force }
    Write-Host 'Sysglow entfernt.'
    return
}

$kopf = @{ 'User-Agent' = 'sysglow-installieren' }
$release = Invoke-RestMethod 'https://api.github.com/repos/IcHiAT/sysglow/releases/latest' -Headers $kopf
$datei = $release.assets | Where-Object { $_.name -eq 'Sysglow.exe' } | Select-Object -First 1
if (-not $datei) { throw "Keine Sysglow.exe in Release $($release.tag_name) gefunden." }
if ($datei.digest -notmatch '^sha256:([0-9a-f]{64})$') { throw 'Release nennt keine SHA256-Pruefsumme.' }
$soll = $Matches[1]

$temp = Join-Path $env:TEMP "Sysglow-$([guid]::NewGuid()).exe"
try {
    Write-Host "Lade $($release.tag_name) ..."
    Invoke-WebRequest $datei.browser_download_url -OutFile $temp -UseBasicParsing
    if ((Get-FileHash $temp -Algorithm SHA256).Hash -ne $soll) { throw 'Pruefsumme stimmt nicht, nichts installiert.' }

    New-Item -ItemType Directory -Force $ordner | Out-Null
    Stop-Sysglow
    Copy-Item $temp $exe -Force
}
finally {
    if (Test-Path $temp) { Remove-Item $temp -Force }
}

$shell = New-Object -ComObject WScript.Shell
$lnk = $shell.CreateShortcut($link)
$lnk.TargetPath = $exe
$lnk.WorkingDirectory = $ordner
$lnk.IconLocation = "$exe,0"
$lnk.Save()

Write-Host "Installiert: $exe ($($release.tag_name))"
Write-Host 'Bitte ohne Administratorrechte ueber das Startmenue starten.'
