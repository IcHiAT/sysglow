//! Konfiguration: sysglow.ini lesen (Einstellungen "key = wert" und Host-Zeilen mit "|").

use std::path::{Path, PathBuf};

/// Pfad der ini: neben der exe, unter "Program Files" aber in %APPDATA%\Sysglow,
/// weil dort ein normales Konto nicht schreiben darf.
pub fn ini_pfad() -> PathBuf {
    let Ok(exe) = std::env::current_exe() else {
        return "sysglow.ini".into();
    };
    if in_programme(&exe) {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let ordner = PathBuf::from(appdata).join("Sysglow");
            let _ = std::fs::create_dir_all(&ordner);
            return ordner.join("sysglow.ini");
        }
    }
    exe.with_file_name("sysglow.ini")
}

pub fn in_programme(exe: &Path) -> bool {
    let klein = |p: &Path| PathBuf::from(p.to_string_lossy().to_lowercase());
    let exe = klein(exe);
    ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"]
        .iter()
        .filter_map(std::env::var_os)
        .any(|ordner| exe.starts_with(klein(Path::new(&ordner))))
}

/// Alle Elemente in Standard-Reihenfolge. Jedes kann in der ini oder im Menue einzeln an/aus.
pub const KNOWN_MODULES: [&str; 10] = ["kopf", "uhr", "cpu", "gpu", "ram", "platten", "netz", "energie", "ping", "todo"];

pub const ZOOM_MIN: u32 = 50;
pub const ZOOM_MAX: u32 = 300;
pub const ZOOM_SCHRITT: u32 = 5;

/// Farbschemata: Name, Hauptfarbe, Akzentfarbe (0xRRGGBB).
pub const THEMES: [(&str, u32, u32); 7] = [
    ("gruen", 0x7CFFB2, 0x40C8FF),
    ("cyan", 0x5CF2FF, 0x7CFFB2),
    ("blau", 0x6CB8FF, 0xB48CFF),
    ("lila", 0xC792FF, 0xFF8FD8),
    ("amber", 0xFFB547, 0xFFE08A),
    ("rot", 0xFF6B6B, 0xFFB08A),
    ("weiss", 0xE6E6E6, 0x8FD3FF),
];

pub const DEFAULT_SETTINGS: &str = "\
# Sysglow-Konfiguration. Aenderungen werden beim Speichern automatisch uebernommen.
#
# module         = Elemente in dieser Reihenfolge: kopf uhr cpu gpu ram platten netz energie ping todo (weglassen = aus)
# breite         = Breite in Pixeln (bei 100 % Skalierung)
# zoom           = Groesse in Prozent, 50 bis 300 (Maus aufs Widget, dann + oder -)
# anzeige_sekunden = Sekunden zwischen zwei Messungen und Bildaufbau (CPU, GPU, RAM, Platten, Netz)
# ping_sekunden  = Sekunden zwischen zwei Pings (Ping-Werte und Ping-Graph)
# timeout        = ms bis \"TIMED OUT\"
# graph_max      = ms, die der Ping-Graph als Obergrenze zeigt
# gelb / rot     = Prozent, ab denen CPU, RAM und Platten gelb bzw. rot werden
# hintergrund    = Deckkraft des Hintergrunds, 0 (durchsichtig) bis 255
# hintergrund_farbe = Farbe des Hintergrunds als RRGGBB, Standard 000000 (schwarz)
# version        = 1: Versionsnummer als eigene Zeile ganz oben, 0 = aus
# rand_rechts / rand_oben = Abstand zum Bildschirmrand, solange nicht verschoben
# pos_x / pos_y  = Position nach dem Verschieben (Menue \"Position zuruecksetzen\" = wieder oben rechts)
# gesperrt       = 1 verhindert Verschieben mit der Maus
# farbe          = gruen cyan blau lila amber rot weiss, oder eigene Farbe als RRGGBB (z. B. 7CFFB2)
# akzent         = optional zweite Farbe als RRGGBB (Netz, Takt, Graphen); leer = passend zum Schema
# energie_auto   = 1: beim Start eines Spiels auf plan_spiel, nach dem Beenden auf plan_normal schalten
# plan_spiel / plan_normal = GUID des Energieplans (powercfg /list), Standard Hoechstleistung / Ausbalanciert
# spiele         = Prozessnamen ohne .exe, mit Leerzeichen getrennt; Direct3D-Vollbild zaehlt immer als Spiel
# vollbild_zeigen = 1: auch ueber Vollbild-Programmen sichtbar bleiben (zum Testen), Standard 0 = ausblenden
# todo_datei     = Datei der Todo-Liste; leer = todo.txt neben dieser ini (eine Aufgabe je Zeile, \"x \" vorne = erledigt)
# farbe_termin / farbe_urlaub / farbe_geburtstag = Farben der Termine im Kalender als RRGGBB
#
# Termine im Kalender (Klick aufs Datum), eine Zeile je Kalender, nur lesen:
# kalender = Name | Art (termin, urlaub, geburtstag) | 1 = an, 0 = aus | https-Adresse oder .ics-Datei
# Google: Kalender-Einstellungen > \"Geheime Adresse im iCal-Format\" kopieren. Diese Adresse nicht weitergeben.
# kalender = Geburtstage | geburtstag | 1 | https://calendar.google.com/calendar/ical/.../basic.ics
module = kopf uhr cpu gpu ram platten netz energie ping todo
breite = 220
zoom = 100
anzeige_sekunden = 3
ping_sekunden = 3
timeout = 3000
graph_max = 500
gelb = 80
rot = 90
hintergrund = 0
hintergrund_farbe = 000000
version = 1
rand_rechts = 0
rand_oben = 0
gesperrt = 0
farbe = gruen
akzent =
energie_auto = 1
plan_spiel = 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c
plan_normal = 381b4222-f694-41f0-9685-ff5bb260df2e
spiele = r5apex r5apex_dx12
";

/// Prozent -> Alpha 0..255
pub fn prozent_alpha(p: u32) -> u8 {
    ((p.min(100) * 255 + 50) / 100) as u8
}

/// Farbton 0..359, Saettigung und Helligkeit 0..100 -> RRGGBB
pub fn hsv_rgb(h: u32, s: u32, v: u32) -> u32 {
    let (s, v) = (s.min(100) as f32 / 100.0, v.min(100) as f32 / 100.0);
    let c = v * s;
    let hp = (h % 360) as f32 / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let k = |f: f32| ((f + m) * 255.0).round() as u32;
    (k(r) << 16) | (k(g) << 8) | k(b)
}

/// RRGGBB -> (Farbton 0..359, Saettigung 0..100, Helligkeit 0..100)
pub fn rgb_hsv(c: u32) -> (u32, u32, u32) {
    let f = |sh: u32| ((c >> sh) & 255) as f32 / 255.0;
    let (r, g, b) = (f(16), f(8), f(0));
    let max = r.max(g).max(b);
    let d = max - r.min(g).min(b);
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max == 0.0 { 0.0 } else { d / max };
    (h.round() as u32 % 360, (s * 100.0).round() as u32, (max * 100.0).round() as u32)
}

/// Standardfarben der Terminarten: Termin blau, Urlaub hellorange, Geburtstag pink.
pub const FARBE_TERMIN: u32 = 0x6CB8FF;
pub const FARBE_URLAUB: u32 = 0xFFB070;
pub const FARBE_GEBURTSTAG: u32 = 0xFF7EC8;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Art {
    Termin,
    Urlaub,
    Geburtstag,
}

/// Eine Zeile "kalender = Name | Art | an | Quelle".
#[derive(Clone, PartialEq, Debug)]
pub struct KalQuelle {
    pub name: String,
    pub art: Art,
    pub an: bool,
    pub quelle: String,
    /// Zeilennummer in der ini (zum Umschalten an/aus)
    pub line: usize,
}

pub fn parse_kalender(v: &str, line: usize) -> Option<KalQuelle> {
    let p: Vec<&str> = v.splitn(4, '|').map(str::trim).collect();
    let quelle = p.get(3).filter(|q| !q.is_empty())?;
    let art = match p.get(1).map(|a| a.to_lowercase()).as_deref() {
        Some("urlaub") => Art::Urlaub,
        Some("geburtstag" | "geburtstage" | "bday") => Art::Geburtstag,
        _ => Art::Termin,
    };
    Some(KalQuelle { name: p[0].to_string(), art, an: p.get(2).is_none_or(|a| *a != "0"), quelle: quelle.to_string(), line })
}

pub const DEFAULT_HOSTS: &str = "\
# Name | Adresse (IP oder Hostname) | gelb ab ms | rot ab ms | Graph 0/1 | Doppelklick-Befehl (optional)
Router     | 192.168.1.1 | 40 | 80 | 0 | cmd.exe
Google     | 8.8.8.8     | 40 | 80 | 1
Cloudflare | 1.1.1.1     | 40 | 80 | 0
";

#[derive(Clone)]
pub struct Settings {
    pub modules: Vec<String>,
    pub width: f32,
    /// Groesse in Prozent (50..300), wirkt auf alles samt Schrift
    pub zoom: u32,
    pub refresh: u64,
    pub interval: u64,
    pub timeout: u32,
    pub graph_max: f32,
    pub yellow: f32,
    pub red: f32,
    pub bg: u8,
    /// Farbe des Hintergrunds (RRGGBB)
    pub bg_farbe: u32,
    /// Versionszeile ganz oben
    pub version_zeigen: bool,
    pub margin_right: i32,
    pub margin_top: i32,
    pub pos: Option<(i32, i32)>,
    pub locked: bool,
    pub color: String,
    pub fg: u32,
    pub dim: u32,
    pub accent: u32,
    pub power_auto: bool,
    pub plan_game: String,
    pub plan_normal: String,
    pub games: Vec<String>,
    pub kalender: Vec<KalQuelle>,
    /// auch bei Vollbild-Programmen sichtbar bleiben
    pub vollbild_zeigen: bool,
    /// Farben fuer Termin, Urlaub, Geburtstag
    pub farben_termine: [u32; 3],
    /// eigene Datei der Todo-Liste (leer = todo.txt neben der ini)
    pub todo_datei: String,
}

#[derive(Clone)]
pub struct HostCfg {
    pub name: String,
    pub addr: String,
    pub yellow: u32,
    pub red: u32,
    pub graph: bool,
    pub shortcut: String,
    /// Zeilennummer in der ini (zum Zurueckschreiben der Graph-Spalte)
    pub line: usize,
}

pub fn parse_config(text: &str) -> (Settings, Vec<HostCfg>) {
    let mut s = Settings {
        modules: KNOWN_MODULES.iter().map(|m| m.to_string()).collect(),
        width: 220.0,
        zoom: 100,
        refresh: 3,
        interval: 3,
        timeout: 3000,
        graph_max: 500.0,
        yellow: 80.0,
        red: 90.0,
        bg: 0,
        bg_farbe: 0,
        version_zeigen: true,
        margin_right: 0,
        margin_top: 0,
        pos: None,
        locked: false,
        color: "gruen".into(),
        fg: THEMES[0].1,
        dim: dim_of(THEMES[0].1),
        accent: THEMES[0].2,
        power_auto: true,
        plan_game: crate::power::PLAN_HIGH.into(),
        plan_normal: crate::power::PLAN_BALANCED.into(),
        games: vec!["r5apex".into(), "r5apex_dx12".into()],
        kalender: Vec::new(),
        vollbild_zeigen: false,
        farben_termine: [FARBE_TERMIN, FARBE_URLAUB, FARBE_GEBURTSTAG],
        todo_datei: String::new(),
    };
    let (mut pos_x, mut pos_y) = (None, None);
    let (mut color, mut accent) = (String::from("gruen"), None);
    let mut hosts = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let l = raw.trim_start_matches('\u{feff}').trim();
        if l.is_empty() || l.starts_with('#') || l.starts_with(';') {
            continue;
        }
        if let Some(v) = l.split_once('=').filter(|(k, _)| k.trim().eq_ignore_ascii_case("kalender")).map(|(_, v)| v) {
            s.kalender.extend(parse_kalender(v, i));
        } else if l.contains('|') {
            let p: Vec<&str> = l.split('|').map(str::trim).collect();
            if p.len() < 2 || p[1].is_empty() {
                continue;
            }
            let num = |i: usize, d: u32| p.get(i).and_then(|x| x.parse().ok()).unwrap_or(d);
            hosts.push(HostCfg {
                name: p[0].to_string(),
                addr: p[1].to_string(),
                yellow: num(2, 40),
                red: num(3, 80),
                graph: p.get(4).is_some_and(|x| *x == "1"),
                shortcut: p.get(5).map(|x| x.to_string()).unwrap_or_default(),
                line: i,
            });
        } else if let Some((k, v)) = l.split_once('=') {
            let v = v.trim();
            match k.trim().to_lowercase().as_str() {
                "module" => {
                    s.modules = v
                        .split(|c: char| c.is_whitespace() || c == ',')
                        .map(str::to_lowercase)
                        .filter(|m| KNOWN_MODULES.contains(&m.as_str()))
                        .collect()
                }
                "breite" => s.width = v.parse::<f32>().unwrap_or(220.0).clamp(120.0, 800.0),
                // alte Namen "aktualisierung" und "intervall" gelten weiter
                "zoom" => s.zoom = v.trim_end_matches('%').trim().parse::<u32>().unwrap_or(100).clamp(ZOOM_MIN, ZOOM_MAX),
                "anzeige_sekunden" | "aktualisierung" => s.refresh = v.parse::<u64>().unwrap_or(3).clamp(1, 60),
                "ping_sekunden" | "intervall" => s.interval = v.parse::<u64>().unwrap_or(3).max(1),
                "timeout" => s.timeout = v.parse::<u32>().unwrap_or(3000).max(100),
                "graph_max" => s.graph_max = v.parse::<f32>().unwrap_or(500.0).max(1.0),
                "gelb" => s.yellow = v.parse().unwrap_or(80.0),
                "rot" => s.red = v.parse().unwrap_or(90.0),
                "hintergrund" => s.bg = v.parse::<u32>().unwrap_or(0).min(255) as u8,
                "hintergrund_farbe" => s.bg_farbe = parse_hex(v).unwrap_or(0),
                "version" => s.version_zeigen = v != "0",
                "rand_rechts" => s.margin_right = v.parse().unwrap_or(0),
                "rand_oben" => s.margin_top = v.parse().unwrap_or(0),
                "pos_x" => pos_x = v.parse::<i32>().ok(),
                "pos_y" => pos_y = v.parse::<i32>().ok(),
                "gesperrt" => s.locked = v == "1",
                "farbe" => color = v.to_lowercase(),
                "akzent" => accent = parse_hex(v),
                "farbe_termin" => s.farben_termine[0] = parse_hex(v).unwrap_or(FARBE_TERMIN),
                "farbe_urlaub" => s.farben_termine[1] = parse_hex(v).unwrap_or(FARBE_URLAUB),
                "farbe_geburtstag" => s.farben_termine[2] = parse_hex(v).unwrap_or(FARBE_GEBURTSTAG),
                "vollbild_zeigen" => s.vollbild_zeigen = v == "1",
                "todo_datei" => s.todo_datei = v.to_string(),
                "energie_auto" => s.power_auto = v == "1",
                "plan_spiel" if crate::power::parse_guid(v).is_some() => s.plan_game = v.to_lowercase(),
                "plan_normal" if crate::power::parse_guid(v).is_some() => s.plan_normal = v.to_lowercase(),
                "spiele" => {
                    s.games = v
                        .split(|c: char| c.is_whitespace() || c == ',')
                        .filter(|x| !x.is_empty())
                        .map(|x| x.to_lowercase().trim_end_matches(".exe").to_string())
                        .collect()
                }
                _ => {}
            }
        }
    }
    s.pos = pos_x.zip(pos_y);
    let (fg, acc) = match THEMES.iter().find(|t| t.0 == color) {
        Some(t) => (t.1, t.2),
        None => parse_hex(&color).map_or((THEMES[0].1, THEMES[0].2), |c| (c, THEMES[0].2)),
    };
    s.color = color;
    s.fg = fg;
    s.dim = dim_of(fg);
    s.accent = accent.unwrap_or(acc);
    (s, hosts)
}

/// "7CFFB2" oder "#7CFFB2" -> 0x7CFFB2
fn parse_hex(v: &str) -> Option<u32> {
    let v = v.trim().trim_start_matches('#');
    (v.len() == 6).then(|| u32::from_str_radix(v, 16).ok()).flatten()
}

/// Gedaempfte Variante der Hauptfarbe fuer Nebentexte.
fn dim_of(c: u32) -> u32 {
    let ch = |sh: u32| ((((c >> sh) & 255) as f32 * 0.72) as u32) << sh;
    ch(16) | ch(8) | ch(0)
}

/// Schaltet ein Element an/aus; beim Einschalten landet es an seiner Standard-Position.
pub fn toggle_module(modules: &[String], m: &str) -> Vec<String> {
    if modules.iter().any(|x| x == m) {
        return modules.iter().filter(|x| *x != m).cloned().collect();
    }
    let rank = |x: &str| KNOWN_MODULES.iter().position(|k| *k == x).unwrap_or(usize::MAX);
    let at = modules.iter().position(|x| rank(x) > rank(m)).unwrap_or(modules.len());
    let mut v = modules.to_vec();
    v.insert(at, m.to_string());
    v
}

#[cfg(test)]
mod kalender_tests {
    use super::*;

    #[test]
    fn hsv_hin_und_zurueck() {
        assert_eq!(hsv_rgb(0, 100, 100), 0xFF0000);
        assert_eq!(hsv_rgb(120, 100, 100), 0x00FF00);
        assert_eq!(hsv_rgb(0, 0, 0), 0x000000);
        assert_eq!(rgb_hsv(0x0000FF), (240, 100, 100));
        for c in [0x22262Bu32, 0x0E1A2E, 0x808080, 0xFFFFFF, 0x2A1012] {
            let (h, s, v) = rgb_hsv(c);
            let z = hsv_rgb(h, s, v);
            for sh in [16, 8, 0] {
                assert!((((c >> sh) & 255) as i32 - ((z >> sh) & 255) as i32).abs() <= 3, "{c:06X} -> {z:06X}");
            }
        }
    }

    #[test]
    fn kalenderzeilen() {
        let (s, hosts) = parse_config("kalender = Bday | geburtstag | 1 | https://x/a?b=c|d
kalender = Ferien | urlaub | 0 | ferien.ics
farbe_urlaub = 112233
A | 1.1.1.1
");
        assert_eq!(hosts.len(), 1);
        assert_eq!(s.kalender.len(), 2);
        assert_eq!(s.kalender[0].quelle, "https://x/a?b=c|d");
        assert_eq!(s.kalender[0].art, Art::Geburtstag);
        assert!(s.kalender[0].an && !s.kalender[1].an);
        assert!(s.version_zeigen && s.bg_farbe == 0);
        assert_eq!(s.kalender[1].line, 1);
        assert_eq!(s.farben_termine, [FARBE_TERMIN, 0x112233, FARBE_GEBURTSTAG]);
    }
}

#[cfg(test)]
mod pfad_tests {
    use super::*;

    #[test]
    fn programme_nur_echte_unterordner() {
        let basis = std::env::var("ProgramFiles").unwrap();
        assert!(in_programme(Path::new(&format!("{basis}\\Sysglow\\Sysglow.exe"))));
        assert!(in_programme(Path::new(&format!("{}\\Sysglow\\Sysglow.exe", basis.to_uppercase()))));
        assert!(!in_programme(Path::new(&format!("{basis} Extra\\Sysglow.exe"))));
        assert!(!in_programme(Path::new("C:\\Users\\x\\AppData\\Local\\Programs\\Sysglow\\Sysglow.exe")));
    }
}
