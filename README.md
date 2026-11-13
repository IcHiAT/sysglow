# Sysglow

Schlankes Desktop-Widget für Windows im Hacker-Stil – geschrieben in Rust, ohne Laufzeit, ohne Treiber.
Ersetzt Rainmeter-Skins für Systemwerte und Ping.

![Sysglow mit allen Elementen](assets/screenshot.png)

Maus auf einem Graphen zeigt Uhrzeit und Wert der Messung:

![Uhrzeit und Wert beim Hover auf dem CPU-Graphen](assets/screenshot-hover.png)

Klick auf das Datum öffnet den Kalender:

![Kalender links neben dem Widget](assets/screenshot-kalender.png)

## Funktionen

- **Elemente**, jedes einzeln an/aus (Rechtsklick → *Elemente* oder `module =` in der ini):
  - `kopf` – Rechnername (kleine Schrift) und Laufzeit
  - `uhr` – Wochentag, Datum, Kalenderwoche, Uhrzeit
  - `cpu` – Last, aktueller Takt (GHz), Temperatur (optional, siehe unten), Verlaufsgraph
  - `gpu` – Grafikkarte: Last, Takt, Temperatur, Verlaufsgraph; Hover zeigt den Grafikspeicher. Geht mit AMD, NVIDIA und Intel über dieselben Windows-Werte wie der Task-Manager. Ohne erkannte Karte bleibt das Element unsichtbar, ohne Temperatur vom Treiber fehlt nur die Gradzahl
  - `ram` – belegt/gesamt, Balken
  - `platten` – alle festen Laufwerke mit Füllstand
  - `netz` – Download/Upload mit Verlaufsgraph
  - `energie` – aktiver Energieplan und erkanntes Spiel; Klick wechselt den Plan, Doppelklick öffnet die Energieoptionen
  - `ping` – beliebig viele Hosts (ICMP), Farbe nach Schwellwert, optional Graph; die letzte deutliche Spitze steht als kleine rote Zahl im Graphen
  - `todo` – Todo-Liste aus `todo.txt` neben der ini (eine Aufgabe je Zeile, `x ` vorne = erledigt). Klick aufs Kästchen hakt ab, Klick auf den Text öffnet einen Editor, der mit dem Text wächst; Esc oder Klick daneben speichert. Die Datei darf auch ein anderes Programm oder Claude ändern, Sysglow liest sie dann neu
- **Energieplan automatisch**: Startet ein Spiel (Prozess aus `spiele` oder Direct3D-Vollbild), schaltet glow auf `plan_spiel` (Höchstleistung); ist das Spiel beendet, zurück auf `plan_normal` (Ausbalanciert). Umgeschaltet wird nur beim Wechsel – wer zwischendurch von Hand umstellt, wird nicht überstimmt. Funktioniert auch, wenn das Element `energie` ausgeblendet ist; abschalten mit `energie_auto = 0` oder im Menü.
- **CPU-Temperatur** (optional): Windows gibt sie nur mit Adminrechten heraus. Rechtsklick → *CPU-Temperatur lesen (einmal Admin)* fragt einmal nach Adminrechten und legt die Aufgabe *Sysglow CPU-Temperatur* in der Aufgabenplanung an. Danach startet Sysglow den erhöhten Teil (`Sysglow.exe --temp`) selbst ohne Abfrage; er liest die Temperatur alle 2 s über den [PawnIO](https://pawnio.eu)-Treiber, denselben Weg wie LibreHardwareMonitor und FanControl, und beendet sich mit der Anzeige. Voraussetzung: PawnIO ist installiert (kommt z. B. mit FanControl mit oder einzeln von pawnio.eu). Intel ab Core 2 und AMD Ryzen (Zen); AMD ist ungetestet. Noch einmal klicken entfernt die Aufgabe. Ohne Einrichtung steht dort `–°C`.
- **Farben**: Rechtsklick → *Farbe* (grün, cyan, blau, lila, amber, rot, weiß) oder eigene Farbe als `RRGGBB` in der ini
- **Hintergrund**: Rechtsklick → *Hintergrund …* öffnet Schieber für Farbton, Sättigung, Helligkeit und Deckkraft; das Widget ändert sich sofort mit
- Versionsnummer als eigene Zeile ganz oben, abschaltbar unter *Elemente* → *Version*
- Liegt nur auf dem Desktop (unter allen Fenstern), blendet sich bei Vollbild und Spielen aus
- **Autoupdate**: Sysglow prüft täglich auf GitHub nach einer neuen Version (eine verpasste Prüfung holt es beim Start nach), lädt sie, prüft die Prüfsumme und installiert sie, sobald kein Vollbild läuft und eine Minute niemand Maus oder Tastatur benutzt. Rechtsklick → *Nach Update suchen* prüft sofort; der Haken *Autoupdate* schaltet das automatische Installieren ab
- Tray-Icon, verschiebbar per Maus, Position wird gespeichert
- Hover zeigt Details (freier Platz, IP-Adresse), Klick auf einen Ping schaltet dessen Graph
- Hover auf einen Graphen (CPU, GPU, Netz, Ping) zeigt Uhrzeit und Wert dieser Messung, bei CPU und GPU auch Takt und Temperatur
- Klick auf Datum oder Uhrzeit öffnet den Kalender links neben dem Widget: `<` `>`, Mausrad oder Pfeiltasten wechseln den Monat, Klick auf den Titel springt zu heute, Esc oder Klick daneben schließt
- **Termine im Kalender** aus Google-Kalendern oder .ics-Dateien (nur lesen): Urlaub hellorange hinterlegt, Termine blau unterstrichen, Geburtstage pink mit Luftballon; Liste der Einträge unter dem Monat. Das Popup lässt sich verschieben, Rechtsklick darauf schaltet einzelne Kalender an/aus oder lädt neu
- Doppelklick: CPU/GPU/RAM/Netz → Task-Manager, Laufwerk → Explorer, Ping → eigener Befehl
- ini wird beim Speichern automatisch neu geladen
- Sehr sparsam: ca. 770 KB exe, ca. 18 MB RAM, kaum CPU
- Nur normale Windows-Abfragen (keine Injektion) – unauffällig für Anti-Cheat. Einzige Ausnahme ist die optionale CPU-Temperatur über den signierten PawnIO-Treiber

## Installation

1. `Sysglow.exe` von der [Release-Seite](https://github.com/IcHiAT/sysglow/releases/latest) laden und starten. Die exe ist nicht signiert, deshalb kann Windows SmartScreen beim ersten Start warnen („Weitere Informationen“ → „Trotzdem ausführen“).
2. Im kleinen Fenster **INSTALL** klicken. Sysglow landet ohne Administratorrechte in `%LOCALAPPDATA%\Programs\Sysglow\` und bekommt einen Startmenü-Eintrag. **START** lässt es ohne Installation laufen. Ist Sysglow schon installiert, bietet das Fenster **UNINSTALL** und **INSTALL** an; die `sysglow.ini` bleibt in beiden Fällen erhalten.
3. Rechtsklick auf das Widget → *Mit Windows starten* für Autostart.

Liegt eine `sysglow.ini` neben der exe, startet sie direkt als Widget, ohne Fenster.

Läuft unter Windows 10/11. Die Benutzung erfolgt auf eigene Gefahr.

### Installieren per Skript

In einer normalen PowerShell im Ordner des Repos starten:

```powershell
.\werkzeug\installieren.ps1
```

Das Skript holt die neueste `Sysglow.exe`, prüft ihre Prüfsumme, legt sie nach `%LOCALAPPDATA%\Programs\Sysglow\`, richtet einen Startmenü-Eintrag ein und startet Sysglow. Es erscheint keine Windows-Abfrage. Ein erneuter Aufruf aktualisiert, `-Entfernen` deinstalliert, dazu zählen auch Autostart und Startmenü-Eintrag; die `sysglow.ini` bleibt erhalten. Mit `-AlleBenutzer` geht es stattdessen nach `C:\Program Files\Sysglow`, dafür fragt Windows einmal nach Administratorrechten, und die `sysglow.ini` steht dann in `%APPDATA%\Sysglow\`.

## Konfiguration (`sysglow.ini`)

| Schlüssel | Bedeutung | Standard |
|---|---|---|
| `module` | Elemente in Reihenfolge: `kopf uhr cpu gpu ram platten netz energie ping todo` | alle |
| `breite` | Breite in Pixeln (100 % Skalierung) | 220 |
| `zoom` | Größe in Prozent (50–300); Maus aufs Widget, dann `+`/`-` | 100 |
| `anzeige_sekunden` | Sekunden zwischen Messungen und Bildaufbau (CPU, GPU, RAM, Platten, Netz); früher `aktualisierung` | 3 |
| `ping_sekunden` | Sekunden zwischen zwei Pings (Werte und Ping-Graph); früher `intervall` | 3 |
| `timeout` | ms bis „TIMED OUT“ | 3000 |
| `graph_max` | Obergrenze des Ping-Graphen in ms | 500 |
| `gelb` / `rot` | Prozent-Schwellen für CPU, RAM, Platten | 80 / 90 |
| `hintergrund` | Deckkraft des Hintergrunds 0–255 (0 = durchsichtig) | 0 |
| `hintergrund_farbe` | Farbe des Hintergrunds als `RRGGBB` | 000000 |
| `version` | Versionsnummer als eigene Zeile oben (1/0) | 1 |
| `rand_rechts` / `rand_oben` | Abstand zum Rand, solange nicht verschoben | 0 |
| `pos_x` / `pos_y` | gespeicherte Position | – |
| `gesperrt` | 1 = nicht verschiebbar | 0 |
| `farbe` | Schema (`gruen cyan blau lila amber rot weiss`) oder `RRGGBB` | gruen |
| `akzent` | zweite Farbe `RRGGBB` (Netz, Takt, Graphen), leer = zum Schema passend | – |
| `energie_auto` | 1 = Energieplan bei Spielstart/-ende umschalten | 1 |
| `plan_spiel` / `plan_normal` | GUIDs aus `powercfg /list` | Höchstleistung / Ausbalanciert |
| `spiele` | Prozessnamen ohne `.exe`, Leerzeichen getrennt | `r5apex r5apex_dx12` |
| `farbe_termin` / `farbe_urlaub` / `farbe_geburtstag` | Farben `RRGGBB` der Termine | 6CB8FF / FFB070 / FF7EC8 |
| `vollbild_zeigen` | 1 = bei Vollbild nicht ausblenden (zum Testen) | 0 |
| `todo_datei` | Datei der Todo-Liste, absolut oder relativ zur ini | `todo.txt` neben der ini |
| `update_auto` | 1 = Updates selbst installieren (Menü: *Autoupdate*) | 1 |
| `update_geprueft` | Zeitpunkt der letzten Update-Prüfung (wird selbst gesetzt) | – |

Ping-Hosts, eine Zeile pro Host:

```
# Name | Adresse | gelb ab ms | rot ab ms | Graph 0/1 | Doppelklick-Befehl (optional)
Router     | 192.168.1.1 | 40 | 80 | 0 | cmd.exe
Google     | 8.8.8.8     | 40 | 80 | 1
```

Kalender, eine Zeile pro Quelle. Art: `termin`, `urlaub` oder `geburtstag`. Quelle: https/webcal-Adresse (bei Google: Kalendereinstellungen → *Geheime Adresse im iCal-Format*) oder Pfad zu einer .ics-Datei:

```
# kalender = Name | Art | an 0/1 | Adresse oder Datei
kalender = Feiertage | termin     | 1 | https://calendar.google.com/calendar/ical/de.german%23holiday%40group.v.calendar.google.com/public/basic.ics
kalender = Bday      | geburtstag | 1 | <geheime Adresse>
```

## Bauen

Benötigt Rust (stable, Ziel `x86_64-pc-windows-gnu` oder `-msvc`):

```
cargo build --release
```

Ergebnis: `target\release\sysglow.exe`.

## Aufbau des Codes

| Datei | Inhalt |
|---|---|
| `src/main.rs` | Fenster, Maus, Menü, Tray, Autostart |
| `src/installer.rs` | Installationsfenster, Installieren und Entfernen |
| `src/app.rs` | Zustand und je eine Funktion pro Element (`el_kopf`, `el_cpu`, …) |
| `src/config.rs` | ini lesen, Standardwerte, Elemente an/aus |
| `src/stats.rs` | Systemwerte (CPU, Takt, RAM, Platten, Netz) |
| `src/gpu.rs` | Grafikkarte (D3DKMT und Leistungsindikatoren, herstellerneutral) |
| `src/cputemp.rs` | CPU-Temperatur: erhöhter Teil über PawnIO, gemeinsamer Speicher, Aufgabe einrichten. Module in `assets/pawnio/` (PawnIO.Modules 0.2.11, LGPL-2.1) |
| `src/ping.rs` | ICMP-Ping im Hintergrund-Thread pro Host |
| `src/power.rs` | Energieplan lesen/setzen, laufende Spiele erkennen |
| `src/kalender.rs` | Kalender-Popup |
| `src/todo.rs` | Todo-Datei lesen, abhaken, Zeilenumbruch, Editor-Fenster |
| `src/update.rs` | Autoupdate: GitHub-Release prüfen, exe laden, Prüfsumme, exe tauschen |
| `src/gfx.rs` | Zeichnen (Text mit Kontur, Balken, Graphen), Tray-Icon |

Ein neues Element: Namen in `KNOWN_MODULES` (`config.rs`) eintragen, `el_<name>` in `app.rs` schreiben und in `render()` zuordnen, Menütext in `show_menu()` (`main.rs`) ergänzen.

## Name und Icon in der exe

Der Task-Manager zeigt Dateibeschreibung, Herausgeber (`-Firma`, Standard IcHiAT) und Icon der exe, im Autostart-Tab erst nach einigen Sekunden. Der GNU-Toolchain bindet keine Ressourcen ein, deshalb nach jedem Build:

```powershell
.\werkzeug\ressourcen.ps1 -Exe <pfad>\sysglow.exe -Ico .\assets\sysglow.ico -Name Sysglow -Version 0.6.0
```
