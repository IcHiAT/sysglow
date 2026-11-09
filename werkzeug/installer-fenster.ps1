<#
Kleines Installationsfenster fuer Sysglow: schwarz, gruener Rand, ein Knopf.
Ruft installieren.ps1 aus demselben Ordner auf, braucht keine Administratorrechte.
Start per Doppelklick auf Installer.cmd.

Changelog:
09.11.2026 21:12 - erstellt
09.11.2026 21:20 - Gruen und Rot wie in Sysglow, Hinweiszeile entfernt
09.11.2026 21:30 - UNINSTALL, wenn Sysglow schon installiert ist
09.11.2026 21:38 - Zahlenregen im Hintergrund
#>
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -Name Dpi -Namespace Sysglow -MemberDefinition '[DllImport("user32.dll")] public static extern bool SetProcessDPIAware();'
[void][Sysglow.Dpi]::SetProcessDPIAware()
[Windows.Forms.Application]::EnableVisualStyles()

$grafik = [Drawing.Graphics]::FromHwnd([IntPtr]::Zero)
$faktor = $grafik.DpiX / 96
$grafik.Dispose()
function P([int]$wert) { [int]($wert * $faktor) }

# Farben wie in Sysglow (COL_FG und COL_RED in src/gfx.rs)
$gruen = [Drawing.Color]::FromArgb(124, 255, 178)
$schwarz = [Drawing.Color]::Black
$schrift = New-Object Drawing.Font('Consolas', 10)
$installiert = Test-Path -LiteralPath (Join-Path $env:LOCALAPPDATA 'Programs\Sysglow\Sysglow.exe')

function Neues-Label([string]$text, [int]$y, $farbe, $font) {
    $l = New-Object Windows.Forms.Label
    $l.Text = $text
    $l.ForeColor = $farbe
    $l.Font = $font
    $l.AutoSize = $true
    $l.Location = New-Object Drawing.Point((P 14), (P $y))
    $l.BackColor = $schwarz
    $l
}

$form = New-Object Windows.Forms.Form
$form.Text = 'Sysglow Installer'
$form.FormBorderStyle = 'None'
$form.StartPosition = 'CenterScreen'
$form.ClientSize = New-Object Drawing.Size((P 340), (P 150))
$form.BackColor = $gruen
$form.Padding = New-Object Windows.Forms.Padding((P 2))
$form.KeyPreview = $true
$form.TopMost = $true

$flaeche = New-Object Windows.Forms.Panel
$flaeche.Dock = 'Fill'
$flaeche.BackColor = $schwarz
$form.Controls.Add($flaeche)

$titel = Neues-Label '> SYSGLOW_' 10 $gruen (New-Object Drawing.Font('Consolas', 13, [Drawing.FontStyle]::Bold))
$grundtext = if ($installiert) { 'Bereits installiert.' } else { 'Systemanzeige fuer den Desktop.' }
$info = Neues-Label $grundtext 42 $gruen $schrift
$status = Neues-Label '' 70 $gruen $schrift

$schliessen = New-Object Windows.Forms.Label
$schliessen.Text = 'x'
$schliessen.ForeColor = $gruen
$schliessen.Font = $schrift
$schliessen.TextAlign = 'MiddleCenter'
$schliessen.Location = New-Object Drawing.Point((P 310), (P 6))
$schliessen.Size = New-Object Drawing.Size((P 22), (P 22))
$schliessen.Cursor = 'Hand'
$schliessen.Add_Click({ $form.Close() })

function Neuer-Knopf([string]$text, [int]$x) {
    $k = New-Object Windows.Forms.Button
    $k.Text = $text
    $k.Font = New-Object Drawing.Font('Consolas', 10, [Drawing.FontStyle]::Bold)
    $k.ForeColor = $gruen
    $k.BackColor = $schwarz
    $k.FlatStyle = 'Flat'
    $k.FlatAppearance.BorderColor = $gruen
    $k.FlatAppearance.MouseOverBackColor = [Drawing.Color]::FromArgb(30, 75, 50)
    $k.Location = New-Object Drawing.Point((P $x), (P 104))
    $k.Size = New-Object Drawing.Size((P 100), (P 32))
    $k.Cursor = 'Hand'
    $k
}
$knopf = Neuer-Knopf 'INSTALL' 222
$entfernen = Neuer-Knopf 'UNINSTALL' 14
$entfernen.Visible = $installiert

$flaeche.Controls.AddRange(@($titel, $info, $status, $schliessen, $knopf, $entfernen))

# Zahlenregen im Hintergrund. Die Texte haben schwarzen Grund und bleiben dadurch lesbar.
$doppelt = [Windows.Forms.Control].GetProperty('DoubleBuffered', [Reflection.BindingFlags]'NonPublic,Instance')
$doppelt.SetValue($flaeche, $true, $null)
$regenSchrift = New-Object Drawing.Font('Consolas', 8)
$zeichen = '01ABCDEF'
$zelle = P 13
$pinsel = foreach ($stufe in 190, 130, 90, 60, 38, 22) {
    New-Object Drawing.SolidBrush([Drawing.Color]::FromArgb(($stufe * 124 / 255), ($stufe * 255 / 255), ($stufe * 178 / 255)))
}
$zufall = New-Object Random
$spalten = [Math]::Ceiling((P 340) / $zelle)
$kopf = New-Object 'double[]' $spalten
$tempo = New-Object 'double[]' $spalten
for ($i = 0; $i -lt $spalten; $i++) {
    $kopf[$i] = $zufall.Next(-20, 14)
    $tempo[$i] = 0.12 + $zufall.NextDouble() * 0.28
}
$flaeche.Add_Paint({
    param($sender, $e)
    for ($s = 0; $s -lt $spalten; $s++) {
        $zeile = [int][Math]::Floor($kopf[$s])
        for ($n = 0; $n -lt $pinsel.Count; $n++) {
            $z = $zeile - $n
            if ($z -lt 0) { continue }
            $welches = $zeichen[($s * 7 + $z * 13) % $zeichen.Length]
            $e.Graphics.DrawString([string]$welches, $regenSchrift, $pinsel[$n], [single]($s * $zelle), [single]($z * $zelle))
        }
    }
})
$regenUhr = New-Object Windows.Forms.Timer
$regenUhr.Interval = 60
$regenUhr.Add_Tick({
    $faktorTempo = if ($uhr.Enabled) { 2.5 } else { 1 }
    for ($s = 0; $s -lt $spalten; $s++) {
        $kopf[$s] += $tempo[$s] * $faktorTempo
        if ($kopf[$s] * $zelle -gt $flaeche.Height + 6 * $zelle) {
            $kopf[$s] = $zufall.Next(-12, 0)
            $tempo[$s] = 0.12 + $zufall.NextDouble() * 0.28
        }
    }
    $flaeche.Invalidate()
})

# Fenster ist rahmenlos, also mit gedrueckter Maustaste auf der Flaeche verschieben.
$script:griff = $null
$ziehen = {
    param($sender, $e)
    if ($e.Button -eq 'Left') { $script:griff = $e.Location }
}
$bewegen = {
    param($sender, $e)
    if ($script:griff) {
        $form.Left += $e.X - $script:griff.X
        $form.Top += $e.Y - $script:griff.Y
    }
}
$loslassen = { $script:griff = $null }
foreach ($teil in @($flaeche, $titel, $info)) {
    $teil.Add_MouseDown($ziehen)
    $teil.Add_MouseMove($bewegen)
    $teil.Add_MouseUp($loslassen)
}

$form.Add_KeyDown({ param($sender, $e) if ($e.KeyCode -eq 'Escape') { $form.Close() } })

$script:prozess = $null
$script:fehlerDatei = $null
$uhr = New-Object Windows.Forms.Timer
$uhr.Interval = 300

$uhr.Add_Tick({
    if (-not $script:prozess.HasExited) {
        $status.Text = $script:laufText + ('.' * ((Get-Date).Second % 4))
        return
    }
    $uhr.Stop()
    if (Test-Path -LiteralPath $script:fehlerDatei) {
        $text = (Get-Content -LiteralPath $script:fehlerDatei -TotalCount 1)
        Remove-Item -LiteralPath $script:fehlerDatei -Force
        $status.ForeColor = [Drawing.Color]::FromArgb(255, 92, 92)
        $status.Text = '> fehler'
        $info.Text = $text
        $knopf.Enabled = $true
        $entfernen.Enabled = $true
    } else {
        $status.Text = $script:fertigText
        $info.Text = ''
        $entfernen.Visible = $false
        $knopf.Enabled = $true
        $knopf.Text = 'EXIT'
    }
})

function Start-Lauf([string]$schalter, [string]$laufText, [string]$fertigText) {
    $skript = (Join-Path $PSScriptRoot 'installieren.ps1').Replace("'", "''")
    $script:fehlerDatei = Join-Path $env:TEMP "Sysglow-Fehler-$([guid]::NewGuid()).txt"
    $script:laufText = $laufText
    $script:fertigText = $fertigText
    $befehl = "try { & '$skript' $schalter *> `$null } catch { Set-Content -LiteralPath '$($script:fehlerDatei)' -Value `$_.Exception.Message; exit 1 }"
    $knopf.Enabled = $false
    $entfernen.Enabled = $false
    $info.Text = $grundtext
    $status.ForeColor = $gruen
    $status.Text = $laufText
    try {
        $script:prozess = Start-Process powershell -WindowStyle Hidden -PassThru -ArgumentList "-NoProfile -ExecutionPolicy Bypass -Command `"$befehl`""
        $uhr.Start()
    } catch {
        $status.Text = '> fehler'
        $knopf.Enabled = $true
        $entfernen.Enabled = $true
    }
}

$knopf.Add_Click({
    if ($knopf.Text -eq 'EXIT') { $form.Close(); return }
    Start-Lauf '' '> lade' '> fertig. laeuft.'
})
$entfernen.Add_Click({ Start-Lauf '-Entfernen' '> entferne' '> entfernt.' })

$form.Add_Shown({ $knopf.Focus(); $regenUhr.Start() })
[void]$form.ShowDialog()
$regenUhr.Dispose()
$uhr.Dispose()
