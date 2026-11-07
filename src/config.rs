//! Konfiguration: sysglow.ini lesen (Einstellungen "key = wert" und Host-Zeilen mit "|").

/// Alle Elemente in Standard-Reihenfolge. Jedes kann in der ini oder im Menue einzeln an/aus.
pub const KNOWN_MODULES: [&str; 6] = ["kopf", "cpu", "ram", "platten", "netz", "ping"];

pub const DEFAULT_SETTINGS: &str = "\
# Sysglow-Konfiguration. Aenderungen werden beim Speichern automatisch uebernommen.
#
# module         = Elemente in dieser Reihenfolge: kopf cpu ram platten netz ping (weglassen = aus)
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
module = kopf cpu ram platten netz ping
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
    };
    let (mut pos_x, mut pos_y) = (None, None);
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
                _ => {}
            }
        }
    }
    s.pos = pos_x.zip(pos_y);
    (s, hosts)
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
