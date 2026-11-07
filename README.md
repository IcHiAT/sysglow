# Sysglow

Schlankes Desktop-Widget für Windows im Hacker-Stil – geschrieben in Rust, ohne Laufzeit, ohne Treiber.
Ersetzt Rainmeter-Skins für Systemwerte und Ping.

```
> SYS://MEINPC                up 05:35
CPU i7-11700KF       4.7 GHz     13%
▁▂▁▃▂▁▁▅▂▁▁▂▁▁▁▂▃▁▁▂▁▁▁▁▂▁▁▁▂▁▁▁▁▂▁▁
RAM 12.2 / 31.9 GB               38%
▮▮▮▮▮▮▮▮▮▯▯▯▯▯▯▯▯▯▯▯▯▯▯▯▯▯▯▯▯▯▯▯▯▯
C: ▮▮▮▮▮▯▯▯▯▯▯▯▯▯▯▯▯▯▯▯          25%
↓ 4 KB/s                    ↑ 13 KB/s
Router                         1 ms ●
Google                        13 ms ●
```

## Funktionen

- **Elemente**, jedes einzeln an/aus (Rechtsklick → *Elemente* oder `module =` in der ini):
  - `kopf` – Rechnername und Laufzeit
  - `cpu` – Last, aktueller Takt (GHz), Verlaufsgraph
  - `ram` – belegt/gesamt, Balken
  - `platten` – alle festen Laufwerke mit Füllstand
  - `netz` – Download/Upload mit Verlaufsgraph
  - `ping` – beliebig viele Hosts (ICMP), Farbe nach Schwellwert, optional Graph
- Liegt nur auf dem Desktop (unter allen Fenstern), blendet sich bei Vollbild und Spielen aus
- Tray-Icon, verschiebbar per Maus, Position wird gespeichert
- Hover zeigt Details (freier Platz, IP-Adresse), Klick auf einen Ping schaltet dessen Graph
- Doppelklick: CPU/RAM/Netz → Task-Manager, Laufwerk → Explorer, Ping → eigener Befehl
- ini wird beim Speichern automatisch neu geladen
- Sehr sparsam: ca. 450 KB exe, ca. 18 MB RAM, kaum CPU
- Nur normale Windows-Abfragen (kein Treiber, keine Injektion) – unauffällig für Anti-Cheat

## Installation

1. `sysglow.exe` in einen eigenen Ordner legen, z. B. `%LOCALAPPDATA%\Programs\Sysglow\`.
2. Starten – beim ersten Start entsteht `sysglow.ini` daneben.
3. Rechtsklick → *Mit Windows starten* für Autostart.

## Konfiguration (`sysglow.ini`)

| Schlüssel | Bedeutung | Standard |
|---|---|---|
| `module` | Elemente in Reihenfolge: `kopf cpu ram platten netz ping` | alle |
| `breite` | Breite in Pixeln (100 % Skalierung) | 220 |
| `aktualisierung` | Sekunden zwischen Messungen von CPU, RAM, Platten, Netz | 1 |
| `intervall` | Sekunden zwischen zwei Pings (Werte und Ping-Graph) | 5 |
| `timeout` | ms bis „TIMED OUT“ | 3000 |
| `graph_max` | Obergrenze des Ping-Graphen in ms | 500 |
| `gelb` / `rot` | Prozent-Schwellen für CPU, RAM, Platten | 80 / 90 |
| `hintergrund` | Deckkraft des dunklen Hintergrunds 0–255 | 0 |
| `rand_rechts` / `rand_oben` | Abstand zum Rand, solange nicht verschoben | 0 |
| `pos_x` / `pos_y` | gespeicherte Position | – |
| `gesperrt` | 1 = nicht verschiebbar | 0 |

Ping-Hosts, eine Zeile pro Host:

```
# Name | Adresse | gelb ab ms | rot ab ms | Graph 0/1 | Doppelklick-Befehl (optional)
Router     | 192.168.1.1 | 40 | 80 | 0 | cmd.exe
Google     | 8.8.8.8     | 40 | 80 | 1
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
| `src/app.rs` | Zustand und je eine Funktion pro Element (`el_kopf`, `el_cpu`, …) |
| `src/config.rs` | ini lesen, Standardwerte, Elemente an/aus |
| `src/stats.rs` | Systemwerte (CPU, Takt, RAM, Platten, Netz) |
| `src/ping.rs` | ICMP-Ping im Hintergrund-Thread pro Host |
| `src/gfx.rs` | Zeichnen (Text mit Kontur, Balken, Graphen), Tray-Icon |

Ein neues Element: Namen in `KNOWN_MODULES` (`config.rs`) eintragen, `el_<name>` in `app.rs` schreiben und in `render()` zuordnen, Menütext in `show_menu()` (`main.rs`) ergänzen.
