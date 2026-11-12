# Changelog

## 0.8.1 (12.11.2026)

- Neu: Die Versionsnummer steht als eigene Zeile ganz oben (`Sysglow v0.8.1`). Abschalten unter Rechtsklick → *Elemente* → *Version* oder `version = 0` in der ini.
- Neu: Rechtsklick → *Hintergrund …* öffnet ein kleines Fenster mit vier Schiebern: Farbton, Sättigung, Helligkeit und Deckkraft. Das Widget zeigt jede Änderung sofort; Mausrad über einem Schieber stellt fein ein, Esc schließt. Gespeichert wird in der ini als `hintergrund_farbe` (RRGGBB) und `hintergrund` (0–255).

## 0.8.0 (12.11.2026)

- Neu: Termine im Kalender-Popup. Quellen sind Google-Kalender (geheime iCal-Adresse) oder .ics-Dateien, eine `kalender =`-Zeile je Quelle, nur lesen. Urlaub färbt den Tag hellorange, Termine bekommen einen blauen Strich, Geburtstage sind pink mit wackelndem Luftballon. Unter dem Monat steht eine Liste (Wiederholungen als Einzeltage, z. B. 3./10./17., bis 8 Einträge). Neu geladen wird alle 30 Minuten, nach einem Fehler nach 60 s.
- Neu: Kalender-Popup ist verschiebbar (überall außer der Kopfzeile ziehen). Rechtsklick darauf schaltet Kalender an/aus und lädt neu.
- Neu: `farbe_termin`, `farbe_urlaub`, `farbe_geburtstag` in der ini.
- Neu (Test): `vollbild_zeigen = 1` lässt Sysglow bei Vollbild sichtbar.
- Neu: Maus aufs Widget und `+` oder `-` drücken macht Sysglow größer oder kleiner (5er-Schritte, 50–300 %). Die Größe steht als `zoom` in der `sysglow.ini` und gilt nach dem Neustart weiter. Der Kalender wächst mit.

## 0.7.0 (10.11.2026)

- Neu: Element `gpu` mit Last, Temperatur und Verlaufsgraph; Hover zeigt den belegten Grafikspeicher. Funktioniert mit AMD, NVIDIA und Intel ohne Zusatzsoftware und ohne Administratorrechte. Bei mehreren Karten zählt die mit dem meisten eigenen Speicher.
- Wird keine Grafikkarte erkannt, bleibt das Element unsichtbar und steht im Menü ausgegraut. Liefert der Treiber keine Temperatur, fehlt nur die Gradzahl.
- Neu: CPU und GPU zeigen Takt (GHz), Temperatur und Last in festen Spalten genau untereinander. Der GPU-Takt kommt vom Treiber (D3DKMT). Darstellung in zwei Zeilen: Name und Last oben, Takt und Temperatur darunter. Der GPU-Takt bleibt im Leerlauf stehen (0,0 GHz) statt zu verschwinden.
- Neu: CPU-Temperatur über den PawnIO-Treiber (wie LibreHardwareMonitor/FanControl). Einmal einrichten per Rechtsklick → *CPU-Temperatur lesen (einmal Admin)*; danach startet Sysglow den erhöhten Teil selbst über die Aufgabenplanung. Ohne Einrichtung oder ohne PawnIO steht `–°C`.
- Installer und Update warten bis zu 8 s, bis die exe frei ist (der Temperatur-Teil beendet sich bis zu 2 s nach der Anzeige).
- Geändert: Die ini-Schlüssel heißen jetzt `anzeige_sekunden` (früher `aktualisierung`) und `ping_sekunden` (früher `intervall`). Die alten Namen funktionieren weiter. Standard für beide ist 3 Sekunden.
- Hinweis: Bestehende `sysglow.ini` bekommt `gpu` nicht automatisch. Einschalten über Rechtsklick → *Elemente* → *GPU*.

## 0.6.0 (09.11.2026)

- Neu: `Sysglow.exe` bringt das Installationsfenster selbst mit. Wer die exe aus dem Download startet, bekommt INSTALL und START, bei vorhandener Installation UNINSTALL und INSTALL. PowerShell ist dafür nicht mehr nötig.
- Geändert: Liegt eine `sysglow.ini` neben der exe, startet sie weiter direkt als Widget.

## 0.5.0 (09.11.2026)

- Neu: `werkzeug\installieren.ps1` installiert ohne Administratorrechte nach `%LOCALAPPDATA%\Programs\Sysglow`, mit Startmenü-Eintrag. Mit `-AlleBenutzer` geht es nach `C:\Program Files\Sysglow`.
- Neu: `werkzeug\Installer.cmd` öffnet ein kleines Installationsfenster. Ist Sysglow schon installiert, bietet es auch Uninstall an.
- Geändert: Liegt die exe unter `C:\Program Files`, steht die `sysglow.ini` in `%APPDATA%\Sysglow`.
- Geändert: Nur `-AlleBenutzer` löst eine Windows-Abfrage nach Administratorrechten aus.
- Neu: `installieren.ps1 -Entfernen` räumt auch Autostart und eigene Startmenü-Verknüpfung auf.
- Geändert: Hostname und Uptime stehen in derselben Schriftgröße wie der Rest.
- Behoben: Bei Hybrid-CPUs mit P- und E-Kernen zeigte der Takt zu hohe Werte (z. B. 7,7 GHz). Die Basis kommt jetzt aus dem Leistungsindikator „Processor Frequency“ statt aus der Registry.

## 0.4.0 (08.11.2026)

- Neu: Maus auf einem Graphen (CPU, Netz, Ping) zeigt Uhrzeit und Wert der Messung an dieser Stelle.
- Neu: Klick auf Datum oder Uhrzeit öffnet einen Kalender mit Kalenderwochen links neben dem Widget.

## 0.3.1 (08.11.2026)

- Neu: Haken „Im Startmenü anzeigen“ im Tray-Menü. Er legt die Verknüpfung im Startmenü an oder entfernt sie.

## 0.3.0 (08.11.2026)

- Erste Veröffentlichung auf GitHub: Energieplan bei Spielen, Uhr, Farbschemata.
