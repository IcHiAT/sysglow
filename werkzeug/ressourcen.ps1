# Schreibt Icon und Dateibeschreibung in eine fertig gebaute exe.
# Noetig nach jedem cargo build, weil der GNU-Toolchain keine Ressourcen einbindet.
# Aufruf: .\ressourcen.ps1 -Exe <pfad> -Ico <pfad> -Name <anzeigename> -Version 1.2.3
param(
    [Parameter(Mandatory)][string]$Exe,
    [Parameter(Mandatory)][string]$Ico,
    [Parameter(Mandatory)][string]$Name,
    [Parameter(Mandatory)][string]$Version,
    [string]$Firma = 'IcHiAT'
)
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).ProviderPath
$Ico = (Resolve-Path $Ico).ProviderPath

Add-Type -Namespace Ressourcen -Name Api -MemberDefinition @'
[DllImport("kernel32.dll", SetLastError=true, CharSet=CharSet.Unicode)]
public static extern IntPtr BeginUpdateResourceW(string file, bool deleteExisting);
[DllImport("kernel32.dll", SetLastError=true, CharSet=CharSet.Unicode)]
public static extern bool UpdateResourceW(IntPtr h, IntPtr type, IntPtr name, ushort lang, byte[] data, uint size);
[DllImport("kernel32.dll", SetLastError=true, CharSet=CharSet.Unicode)]
public static extern bool EndUpdateResourceW(IntPtr h, bool discard);
'@

function Ausrichten([System.IO.MemoryStream]$m) { while ($m.Length % 4) { $m.WriteByte(0) } }

# Ein Knoten der VS_VERSIONINFO-Struktur: Laenge, Wertlaenge, Typ, Schluessel, Wert, Kinder.
function Knoten([string]$Key, [byte[]]$Wert, [int]$WertLaenge, [int]$Typ, [byte[][]]$Kinder) {
    $m = New-Object System.IO.MemoryStream
    $b = New-Object System.IO.BinaryWriter($m)
    $b.Write([uint16]0); $b.Write([uint16]$WertLaenge); $b.Write([uint16]$Typ)
    $b.Write([Text.Encoding]::Unicode.GetBytes($Key + [char]0))
    Ausrichten $m
    if ($Wert) { $m.Position = $m.Length; $b.Write($Wert) }
    foreach ($k in $Kinder) { Ausrichten $m; $m.Position = $m.Length; $b.Write($k) }
    $m.Position = 0; $b.Write([uint16]$m.Length)
    ,$m.ToArray()
}
function Text([string]$Key, [string]$Wert) {
    Knoten $Key ([Text.Encoding]::Unicode.GetBytes($Wert + [char]0)) ($Wert.Length + 1) 1 @()
}

$teile = @($Version.Split('.') | ForEach-Object { [uint16]$_ }) + @(0, 0, 0, 0) | Select-Object -First 4
$v = New-Object System.IO.MemoryStream
$vb = New-Object System.IO.BinaryWriter($v)
foreach ($u in 4277077181, 65536) { $vb.Write([uint32]$u) }
foreach ($i in 1, 0, 3, 2, 1, 0, 3, 2) { $vb.Write([uint16]$teile[$i]) }  # Datei- und Produktversion (hoch, niedrig)
foreach ($u in 63, 0, 262148, 1, 0, 0, 0) { $vb.Write([uint32]$u) }

$voll = $teile -join '.'
$strings = @(
    (Text 'CompanyName' $Firma),
    (Text 'FileDescription' $Name), (Text 'ProductName' $Name), (Text 'InternalName' $Name),
    (Text 'FileVersion' $voll), (Text 'ProductVersion' $voll),
    (Text 'OriginalFilename' ([IO.Path]::GetFileName($Exe)))
)
$tabelle = Knoten '000004B0' $null 0 1 $strings
$sfi = Knoten 'StringFileInfo' $null 0 1 @(,$tabelle)
$trans = Knoten 'Translation' ([byte[]](0, 0, 0xB0, 0x04)) 4 0 @()
$vfi = Knoten 'VarFileInfo' $null 0 1 @(,$trans)
[byte[]]$versionsBlock = Knoten 'VS_VERSION_INFO' $v.ToArray() ([int]$v.Length) 0 @($sfi, $vfi)

# ICO zerlegen: jedes Bild wird RT_ICON, das Verzeichnis RT_GROUP_ICON (statt Offset die ID).
$icoDaten = [IO.File]::ReadAllBytes($Ico)
$anzahl = [BitConverter]::ToUInt16($icoDaten, 4)
$g = New-Object System.IO.MemoryStream
$gb = New-Object System.IO.BinaryWriter($g)
$gb.Write($icoDaten, 0, 6)
$bilder = @()
for ($n = 0; $n -lt $anzahl; $n++) {
    $o = 6 + 16 * $n
    $len = [BitConverter]::ToInt32($icoDaten, $o + 8)
    $off = [BitConverter]::ToInt32($icoDaten, $o + 12)
    $gb.Write($icoDaten, $o, 12)
    $gb.Write([uint16]($n + 1))
    $daten = New-Object byte[] $len
    [Array]::Copy($icoDaten, $off, $daten, 0, $len)
    $bilder += ,$daten
}

$h = [Ressourcen.Api]::BeginUpdateResourceW($Exe, $false)
if ($h -eq [IntPtr]::Zero) { throw "BeginUpdateResource: $([ComponentModel.Win32Exception]::new().Message)" }
function Setzen([int]$Typ, [int]$Id, [byte[]]$Daten) {
    if (-not [Ressourcen.Api]::UpdateResourceW($h, [IntPtr]$Typ, [IntPtr]$Id, 0, $Daten, $Daten.Length)) {
        $f = [ComponentModel.Win32Exception]::new().Message
        [void][Ressourcen.Api]::EndUpdateResourceW($h, $true)
        throw "UpdateResource $Typ/${Id}: $f"
    }
}
for ($n = 0; $n -lt $bilder.Count; $n++) { Setzen 3 ($n + 1) $bilder[$n] }
Setzen 14 1 $g.ToArray()
Setzen 16 1 $versionsBlock
if (-not [Ressourcen.Api]::EndUpdateResourceW($h, $false)) { throw "EndUpdateResource: $([ComponentModel.Win32Exception]::new().Message)" }
"OK: $Exe -> $Name $voll, $($bilder.Count) Icon-Groessen"
