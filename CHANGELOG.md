# Changelog

## 0.9.1 (13.11.2026)

- Neu: Neue Elemente erscheinen nach einem Update auch mit einer bestehenden `sysglow.ini`. Sysglow merkt sich dazu in `module_bekannt`, welche Elemente die ini schon kennt; nur unbekannte werden einmal eingeschaltet, abgewählte bleiben aus. Damit taucht die Todo-Liste aus 0.9.0 jetzt auch bei älteren Installationen auf.
- Neu: Kommt keine CPU-Temperatur, steht der Grund im Rechtsklick-Menü statt nur „–“ im Widget, z. B. „erst PawnIO installieren (pawnio.eu)“ oder „keine Adminrechte“. Fehlt der PawnIO-Treiber, öffnet der Menüpunkt die Downloadseite, statt sinnlos nach Adminrechten zu fragen.
- Geändert: Ohne Temperaturwert zeigen CPU und GPU an dieser Stelle nichts mehr statt „–°C“.
- Neu: Nach einem Update erscheint einmal ein kleines Fenster in der Mitte mit den Neuerungen (`+` neu, `-` weg, `*` behoben) und dem Sprung von der alten zur neuen Version. Sysglow merkt sich dazu `version_gesehen` in der ini; bei einer Erstinstallation kommt es nicht. Schließen mit OK, Esc oder Enter.
- Neu: Sysglow aktualisiert sich selbst. Einmal am Tag fragt es bei GitHub nach dem neuesten Release; war der PC zur fälligen Zeit aus, holt es die Prüfung kurz nach dem Start nach (`update_geprueft` in der ini). Eine neue Version wird geladen, gegen Größe und SHA-256-Prüfsumme von GitHub geprüft und installiert, sobald kein Vollbild läuft und eine Minute niemand Maus oder Tastatur benutzt hat. Danach startet Sysglow neu und zeigt die Neuerungen. Rechtsklick → *Nach Update suchen* prüft sofort; der Haken *Autoupdate* (`update_auto`, Standard an) schaltet auf „nur anbieten“ um, dann steht *Update auf vX installieren* im Menü. Liegt Sysglow unter Program Files, öffnet der Menüpunkt die Downloadseite.
- Neu: Zu lange Einträge im Neuerungsfenster brechen am Wortende um.
- Geändert: Der Zahlenregen im Installations- und Neuerungsfenster ist kleiner, dichter, schneller und gedimmt weiß; die Schrift steht ohne schwarzen Kasten darüber.

## 0.9.0 (12.11.2026)

- Neu: Hover auf den CPU- oder GPU-Graphen zeigt neben Last und Uhrzeit auch Takt und Temperatur dieser Messung.
- Neu: Im Ping-Graphen steht die letzte deutliche Spitze als kleine rote Zahl (ms). Als Spitze zählt ein Wert, der mindestens 1,5-mal so hoch wie der Median des sichtbaren Verlaufs und mindestens 5 ms darüber liegt. Timeouts zählen nicht.
- Neu: Element `todo` ganz unten, eine umrahmte Liste, die mit dem Text wächst. Die Aufgaben stehen in `todo.txt` neben der `sysglow.ini`: eine Aufgabe je Zeile, `x ` am Anfang heißt erledigt. Die Datei kann jeder bearbeiten, auch Claude; Sysglow liest sie bei jeder Änderung neu. Klick aufs Kästchen hakt ab. Klick auf den Text öffnet einen kleinen Editor über der Box, der beim Schreiben mitwächst; Esc oder Klick daneben speichert. Eine andere Datei lässt sich mit `todo_datei` einstellen.
- Hinweis: Bestehende `sysglow.ini` bekommt `todo` nicht automatisch. Einschalten über Rechtsklick → *Elemente* → *Todo-Liste*.

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
