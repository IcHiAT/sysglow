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

fn in_programme(exe: &Path) -> bool {
    let klein = |p: &Path| PathBuf::from(p.to_string_lossy().to_lowercase());
    let exe = klein(exe);
    ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"]
        .iter()
        .filter_map(std::env::var_os)
        .any(|ordner| exe.starts_with(klein(Path::new(&ordner))))
}

/// Alle Elemente in Standard-Reihenfolge. Jedes kann in der ini oder im Menue einzeln an/aus.
pub const KNOWN_MODULES: [&str; 8] = ["kopf", "uhr", "cpu", "ram", "platten", "netz", "energie", "ping"];

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
# module         = Elemente in dieser Reihenfolge: kopf uhr cpu ram platten netz energie ping (weglassen = aus)
# breite         = Breite in Pixeln (bei 100 % Skalierung)
# aktualisierung = Sekunden zwischen zwei Messungen von CPU, RAM, Platten, Netz
# intervall      = Sekunden zwischen zwei Pings (Ping-Werte und Ping-Graph)
# timeout        = ms bis \"TIMED OUT\"
# graph_max      = ms, die der Ping-Graph als Obergrenze zeigt
# gelb / rot     = Prozent, ab denen CPU, RAM und Platten gelb bzw. rot werden
# hintergrund    = Deckkraft eines dunklen Hintergrunds, 0 (durchsichtig) bis 255
# rand_rechts / rand_oben = Abstand zum Bildschirmrand, solange nicht verschoben
# pos_x / pos_y  = Position nach dem Verschieben (Menue \"Position zuruecksetzen\" = wieder oben rechts)
# gesperrt       = 1 verhindert Verschieben mit der Maus
# farbe          = gruen cyan blau lila amber rot weiss, oder eigene Farbe als RRGGBB (z. B. 7CFFB2)
# akzent         = optional zweite Farbe als RRGGBB (Netz, Takt, Graphen); leer = passend zum Schema
# energie_auto   = 1: beim Start eines Spiels auf plan_spiel, nach dem Beenden auf plan_normal schalten
# plan_spiel / plan_normal = GUID des Energieplans (powercfg /list), Standard Hoechstleistung / Ausbalanciert
# spiele         = Prozessnamen ohne .exe, mit Leerzeichen getrennt; Direct3D-Vollbild zaehlt immer als Spiel
module = kopf uhr cpu ram platten netz energie ping
breite = 220
aktualisierung = 1
intervall = 5
timeout = 3000
graph_max = 500
gelb = 80
rot = 90
hintergrund = 0
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
    pub refresh: u64,
    pub interval: u64,
    pub timeout: u32,
    pub graph_max: f32,
    pub yellow: f32,
    pub red: f32,
    pub bg: u8,
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
        refresh: 1,
        interval: 5,
        timeout: 3000,
        graph_max: 500.0,
        yellow: 80.0,
        red: 90.0,
        bg: 0,
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
    };
    let (mut pos_x, mut pos_y) = (None, None);
    let (mut color, mut accent) = (String::from("gruen"), None);
    let mut hosts = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let l = raw.trim_start_matches('\u{feff}').trim();
        if l.is_empty() || l.starts_with('#') || l.starts_with(';') {
            continue;
        }
        if l.contains('|') {
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
                "aktualisierung" => s.refresh = v.parse::<u64>().unwrap_or(1).clamp(1, 60),
                "intervall" => s.interval = v.parse::<u64>().unwrap_or(5).max(1),
                "timeout" => s.timeout = v.parse::<u32>().unwrap_or(3000).max(100),
                "graph_max" => s.graph_max = v.parse::<f32>().unwrap_or(500.0).max(1.0),
                "gelb" => s.yellow = v.parse().unwrap_or(80.0),
                "rot" => s.red = v.parse().unwrap_or(90.0),
                "hintergrund" => s.bg = v.parse::<u32>().unwrap_or(0).min(255) as u8,
                "rand_rechts" => s.margin_right = v.parse().unwrap_or(0),
                "rand_oben" => s.margin_top = v.parse().unwrap_or(0),
                "pos_x" => pos_x = v.parse::<i32>().ok(),
                "pos_y" => pos_y = v.parse::<i32>().ok(),
                "gesperrt" => s.locked = v == "1",
                "farbe" => color = v.to_lowercase(),
                "akzent" => accent = parse_hex(v),
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
