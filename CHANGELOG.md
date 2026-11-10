# Changelog

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
