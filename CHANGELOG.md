# Changelog

## Unveröffentlicht

- Neu: `werkzeug\installieren.ps1` installiert ohne Administratorrechte nach `%LOCALAPPDATA%\Programs\Sysglow`, mit Startmenü-Eintrag. Mit `-AlleBenutzer` geht es nach `C:\Program Files\Sysglow`.
- Geändert: Liegt die exe unter `C:\Program Files`, steht die `sysglow.ini` in `%APPDATA%\Sysglow`.
- Geändert: Nur `-AlleBenutzer` löst eine Windows-Abfrage nach Administratorrechten aus.
- Neu: `installieren.ps1 -Entfernen` räumt auch Autostart und eigene Startmenü-Verknüpfung auf.
- Behoben: Bei Hybrid-CPUs mit P- und E-Kernen zeigte der Takt zu hohe Werte (z. B. 7,7 GHz). Die Basis kommt jetzt aus dem Leistungsindikator „Processor Frequency“ statt aus der Registry.

## 0.4.0 (08.11.2026)

- Neu: Maus auf einem Graphen (CPU, Netz, Ping) zeigt Uhrzeit und Wert der Messung an dieser Stelle.
- Neu: Klick auf Datum oder Uhrzeit öffnet einen Kalender mit Kalenderwochen links neben dem Widget.

## 0.3.1 (08.11.2026)

- Neu: Haken „Im Startmenü anzeigen“ im Tray-Menü. Er legt die Verknüpfung im Startmenü an oder entfernt sie.

## 0.3.0 (08.11.2026)

- Erste Veröffentlichung auf GitHub: Energieplan bei Spielen, Uhr, Farbschemata.
