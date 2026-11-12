//! Anwendungszustand und Aufbau des Widgets. Jedes Element (kopf, cpu, ...) ist eine eigene Funktion `el_*`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::mem::zeroed;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::config::*;
use crate::gfx::*;
use crate::gpu::Gpu;
use crate::ping::*;
use crate::stats::*;

// Masse bei 100 % Skalierung
const ROW_H: f32 = 15.0;
const PAD: f32 = 6.0;
const GAP: f32 = 6.0;
const SPARK_H: f32 = 18.0;
const NET_SPARK_H: f32 = 14.0;
const BAR_H: f32 = 4.0;
const DISK_BAR_H: f32 = 7.0;
const SEP_H: f32 = 7.0;
const GRAPH_H: f32 = 26.0;

#[derive(Clone, Copy, PartialEq)]
pub enum Hit {
    Uhr,
    Cpu,
    Gpu,
    Ram,
    Disk(usize),
    Net,
    Ping(usize),
    Power,
}

pub struct Host {
    pub cfg: HostCfg,
    status: Shared,
    stop: Arc<AtomicBool>,
    seen: u32,
    hist: VecDeque<f32>,
    zeit: VecDeque<u32>,
}

impl Host {
    pub fn stop(&self) {
        self.stop.store(true, Relaxed);
    }
}

pub struct App {
    pub hwnd: HWND,
    pub cfg_path: PathBuf,
    cfg_mtime: Option<SystemTime>,
    pub settings: Settings,
    pub hosts: Vec<Host>,
    pub stats: Stats,
    /// None, wenn keine passende Grafikkarte erkannt wurde; das Element bleibt dann unsichtbar
    pub gpu: Option<Gpu>,
    /// CPU-Temperatur vom erhoehten Teil (`--temp`); None ohne Einrichtung oder ohne PawnIO
    pub cputemp: crate::cputemp::Leser,
    pub cpu_temp: Option<f32>,
    takte: u32,
    computer: String,
    pub font: HFONT,
    /// aktiver Energieplan (GUID), bekannte Plan-Namen und erkanntes Spiel
    pub plan: Option<String>,
    plan_names: Vec<(String, String)>,
    pub game: Option<String>,
    game_prev: bool,
    pub hover: Option<Hit>,
    hits: Vec<(i32, i32, Hit)>,
    /// Mausposition im Fenster und Flaechen der Graphen aus dem letzten Bild
    cursor: Option<(i32, i32)>,
    graphs: Vec<RECT>,
    pub tracking: bool,
    pub ignore_up: bool,
    pub drag: Option<(POINT, POINT)>,
    pub dragging: bool,
    visible: bool,
    pub user_hidden: bool,
    pub scale: f32,
    /// Skalierung des Systems (DPI); `scale` = dpi x zoom
    dpi: f32,
    size: (i32, i32),
    text: Surface,
    out: Surface,
}

/// Ein Bild im Aufbau: Text, Grafik, Klickbereiche und aktuelle Zeile.
struct Frame {
    ops: Vec<TextOp>,
    dr: Vec<Draw>,
    hits: Vec<(i32, i32, Hit)>,
    graphs: Vec<RECT>,
    y: i32,
    w: i32,
    pad: i32,
    gap: i32,
    rh: i32,
    inner: i32,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Fuehrt f mit der App aus; bei Wiedereintritt (z. B. waehrend eines Menues) wird nichts getan.
pub fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut a| a.as_mut().map(f)))
}

pub fn install(app: App) {
    APP.with(|a| *a.borrow_mut() = Some(app));
}

fn innen(r: &RECT, x: i32, y: i32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

fn mtime(p: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// 2 = Vollbild-App, 3 = Direct3D-Vollbild (Spiel), 4 = Praesentationsmodus
fn notification_state() -> i32 {
    use windows_sys::Win32::UI::Shell::SHQueryUserNotificationState;
    let mut st = 0;
    if unsafe { SHQueryUserNotificationState(&mut st) } == 0 {
        st
    } else {
        0
    }
}

fn fullscreen_active() -> bool {
    matches!(notification_state(), 2..=4)
}

const WOCHENTAG: [&str; 7] = ["So", "Mo", "Di", "Mi", "Do", "Fr", "Sa"];

/// Kalenderwoche nach ISO 8601.
pub fn iso_week(y: i32, m: u32, d: u32) -> u32 {
    let leap = |y: i32| (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let cum = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let doy = |y: i32, m: u32, d: u32| cum[(m - 1) as usize] + d + u32::from(m > 2 && leap(y));
    // Wochentag 1 = Mo .. 7 = So (1.1.0001 war ein Montag)
    let wd = |y: i32, m: u32, d: u32| {
        let yy = (y - 1) as i64;
        let days = yy * 365 + yy / 4 - yy / 100 + yy / 400 + doy(y, m, d) as i64;
        ((days - 1).rem_euclid(7) + 1) as u32
    };
    let w = (doy(y, m, d) as i32 - wd(y, m, d) as i32 + 10) / 7;
    if w < 1 {
        iso_week(y - 1, 12, 31)
    } else if w == 53 && wd(y, 12, 31) < 4 {
        1
    } else {
        w as u32
    }
}

impl App {
    pub unsafe fn new(hwnd: HWND, scale: f32, font: HFONT, cfg_path: PathBuf) -> App {
        App {
            hwnd,
            cfg_path,
            cfg_mtime: None,
            settings: parse_config("").0,
            hosts: Vec::new(),
            stats: Stats::new(),
            gpu: Gpu::new(),
            cputemp: crate::cputemp::Leser::new(),
            cpu_temp: None,
            takte: 0,
            computer: std::env::var("COMPUTERNAME").unwrap_or_else(|_| "PC".into()),
            font,
            plan: None,
            plan_names: Vec::new(),
            game: None,
            game_prev: false,
            hover: None,
            hits: Vec::new(),
            cursor: None,
            graphs: Vec::new(),
            tracking: false,
            ignore_up: false,
            drag: None,
            dragging: false,
            visible: false,
            user_hidden: false,
            scale,
            dpi: scale,
            size: (0, 0),
            text: new_surface(font),
            out: new_surface(null_mut()),
        }
    }

    pub fn px(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }

    pub fn hit_at(&self, y: i32) -> Option<Hit> {
        self.hits.iter().find(|(a, b, _)| y >= *a && y < *b).map(|h| h.2)
    }

    /// Maus bewegt: neu zeichnen, wenn sich das Element oder der Punkt im Graphen geaendert hat.
    pub fn mouse_at(&mut self, pos: Option<(i32, i32)>) {
        let key = |a: &App, p: Option<(i32, i32)>| {
            let (x, y) = p?;
            a.graphs.iter().find(|r| innen(r, x, y)).map(|r| (r.top, x))
        };
        let changed = key(self, pos) != key(self, self.cursor);
        self.cursor = pos;
        let hit = pos.and_then(|p| self.hit_at(p.1));
        if changed || hit != self.hover {
            self.hover = hit;
            self.render();
        }
    }

    /// Index des Verlaufswerts unter der Maus; None, wenn die Maus nicht ueber diesem Graphen steht oder dort noch nichts gemessen wurde.
    fn punkt(&self, r: &RECT, len: usize) -> Option<usize> {
        let (x, _) = self.cursor.filter(|&(x, y)| innen(r, x, y))?;
        let back = (r.right - 1 - x) as usize;
        (back < len).then(|| len - 1 - back)
    }

    /// Merkt die Graph-Flaeche und zeichnet die senkrechte Marke am gewaehlten Punkt.
    fn marke(&self, f: &mut Frame, r: RECT, p: Option<usize>) {
        f.graphs.push(r);
        if let (Some(_), Some((x, _))) = (p, self.cursor) {
            f.dr.push(Draw::Fill(rect(x, r.top, x + self.px(1.0).max(1), r.bottom), 0xFFFFFF, 0.9));
        }
    }

    fn level(&self, pct: f32) -> u32 {
        if pct >= self.settings.red {
            COL_RED
        } else if pct >= self.settings.yellow {
            COL_YELLOW
        } else {
            self.settings.fg
        }
    }

    // ------------------------------------------------------------ Konfiguration
    pub fn load_config(&mut self) {
        if !self.cfg_path.exists() {
            // Erststart: Hosts aus einer alten pingmeter.ini uebernehmen, falls vorhanden
            let old = self.cfg_path.with_file_name("pingmeter.ini");
            let hosts = std::fs::read_to_string(old)
                .ok()
                .map(|t| t.lines().map(|l| l.trim_start_matches('\u{feff}')).filter(|l| l.contains('|')).collect::<Vec<_>>().join("\n") + "\n")
                .filter(|h| !h.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_HOSTS.to_string());
            let _ = std::fs::write(&self.cfg_path, format!("{DEFAULT_SETTINGS}\n{hosts}").replace('\n', "\r\n"));
        }
        let text = std::fs::read_to_string(&self.cfg_path).unwrap_or_default();
        self.cfg_mtime = mtime(&self.cfg_path);
        let (settings, cfgs) = parse_config(&text);
        let mut old: Vec<Host> = self.hosts.drain(..).collect();
        for h in &old {
            h.stop();
        }
        for cfg in cfgs {
            // Verlauf und letzten Wert gleicher Hosts uebernehmen
            let prev = old.iter().position(|h| h.cfg.addr == cfg.addr && h.cfg.name == cfg.name).map(|i| old.remove(i));
            let (hist, zeit, last) = prev.map_or((VecDeque::new(), VecDeque::new(), (Status::Pending, 0)), |h| (h.hist, h.zeit, *h.status.lock().unwrap()));
            let host = Host { cfg, status: Arc::new(Mutex::new((last.0, 0))), stop: Arc::new(AtomicBool::new(false)), seen: 0, hist, zeit };
            spawn_pinger(host.cfg.addr.clone(), &settings, host.status.clone(), host.stop.clone(), self.hwnd as usize);
            self.hosts.push(host);
        }
        unsafe { SetTimer(self.hwnd, 1, (settings.refresh * 1000) as u32, None) };
        crate::termine::quellen_setzen(&settings.kalender);
        self.settings = settings;
        self.hover = None;
        self.zoom_anwenden();
    }

    /// Setzt Skalierung und Schrift passend zu `settings.zoom`; nur bei Aenderung.
    fn zoom_anwenden(&mut self) {
        let scale = self.dpi * self.settings.zoom as f32 / 100.0;
        if (scale - self.scale).abs() < 0.001 {
            return;
        }
        unsafe {
            let alt = self.font;
            self.font = crate::schrift(scale);
            SelectObject(self.text.dc, self.font as HGDIOBJ);
            crate::kalender::schrift(self.font);
            DeleteObject(alt as HGDIOBJ);
        }
        self.scale = scale;
    }

    /// + / - ueber dem Widget: Zoom in 5er-Schritten, in der ini gespeichert.
    pub fn zoom_schritt(&mut self, groesser: bool) {
        use crate::config::{ZOOM_MAX, ZOOM_MIN, ZOOM_SCHRITT};
        let z = self.settings.zoom;
        let neu = if groesser { z + ZOOM_SCHRITT } else { z.saturating_sub(ZOOM_SCHRITT) };
        let neu = (neu / ZOOM_SCHRITT * ZOOM_SCHRITT).clamp(ZOOM_MIN, ZOOM_MAX);
        if neu == z {
            return;
        }
        self.settings.zoom = neu;
        self.set_key("zoom", &neu.to_string());
        self.zoom_anwenden();
        self.render();
    }

    fn write_lines(&mut self, lines: &[String]) {
        if std::fs::write(&self.cfg_path, lines.join("\r\n") + "\r\n").is_ok() {
            self.cfg_mtime = mtime(&self.cfg_path);
        }
    }

    /// Setzt "key = val" in der ini; fehlt die Zeile, wird sie nach der letzten Einstellung eingefuegt.
    pub fn set_key(&mut self, key: &str, val: &str) {
        let Ok(text) = std::fs::read_to_string(&self.cfg_path) else { return };
        let mut lines: Vec<String> = text.lines().map(String::from).collect();
        let is_setting = |l: &str| {
            let l = l.trim_start_matches('\u{feff}').trim();
            !l.starts_with('#') && !l.starts_with(';') && !l.contains('|') && l.contains('=')
        };
        let new = format!("{key} = {val}");
        let found = lines.iter().position(|l| {
            is_setting(l) && l.trim_start_matches('\u{feff}').split('=').next().is_some_and(|k| k.trim().eq_ignore_ascii_case(key))
        });
        match found {
            Some(i) => lines[i] = new,
            None => {
                let at = lines.iter().rposition(|l| is_setting(l)).map_or(0, |i| i + 1);
                lines.insert(at, new);
                for h in &mut self.hosts {
                    if h.cfg.line >= at {
                        h.cfg.line += 1;
                    }
                }
            }
        }
        self.write_lines(&lines);
    }

    /// Schreibt nur die Graph-Spalte der betreffenden Zeile zurueck, Kommentare bleiben erhalten.
    fn save_graph_flag(&mut self, i: usize) {
        let Ok(text) = std::fs::read_to_string(&self.cfg_path) else { return };
        let mut lines: Vec<String> = text.lines().map(String::from).collect();
        let h = &self.hosts[i].cfg;
        let Some(line) = lines.get_mut(h.line) else { return };
        let mut parts: Vec<String> = line.split('|').map(|s| s.trim().to_string()).collect();
        while parts.len() < 5 {
            parts.push(match parts.len() {
                2 => h.yellow.to_string(),
                3 => h.red.to_string(),
                _ => "0".into(),
            });
        }
        parts[4] = if h.graph { "1" } else { "0" }.into();
        *line = parts.join(" | ");
        self.write_lines(&lines);
    }

    pub fn toggle_graph(&mut self, i: usize) {
        if let Some(h) = self.hosts.get_mut(i) {
            h.cfg.graph = !h.cfg.graph;
            self.save_graph_flag(i);
            self.render();
        }
    }

    fn plan_label(&mut self, guid: &str) {
        if !self.plan_names.iter().any(|(g, _)| g == guid) {
            let n = crate::power::plan_name(guid);
            self.plan_names.push((guid.to_string(), n));
        }
    }

    /// Spiel erkennen und bei Start/Ende den Energieplan umschalten (nur beim Wechsel, manuelles Umstellen bleibt).
    fn update_power(&mut self) {
        let st = &self.settings;
        self.game = crate::power::running_game(&st.games).or_else(|| (notification_state() == 3).then(|| "Vollbild-Spiel".to_string()));
        let now = self.game.is_some();
        if st.power_auto && now != self.game_prev {
            let target = if now { st.plan_game.clone() } else { st.plan_normal.clone() };
            crate::power::set_plan(&target);
        }
        self.game_prev = now;
        self.plan = crate::power::active_plan();
    }

    /// Klick auf die Energie-Zeile: zwischen plan_normal und plan_spiel wechseln.
    pub fn toggle_plan(&mut self) {
        let st = &self.settings;
        let target = if self.plan.as_deref() == Some(st.plan_game.as_str()) { st.plan_normal.clone() } else { st.plan_game.clone() };
        crate::power::set_plan(&target);
        self.plan = crate::power::active_plan();
        self.render();
    }

    pub fn set_power_auto(&mut self, on: bool) {
        self.settings.power_auto = on;
        self.set_key("energie_auto", if on { "1" } else { "0" });
        self.render();
    }

    pub fn set_color(&mut self, name: &str) {
        self.set_key("farbe", name);
        self.set_key("akzent", "");
        self.load_config();
        self.render();
    }

    /// Hintergrund: Farbe und/oder Deckkraft setzen. Neue Farbe bei unsichtbarem Hintergrund -> 60 %.
    pub fn set_hintergrund(&mut self, farbe: Option<u32>, alpha: Option<u8>) {
        let alpha = alpha.or_else(|| (farbe.is_some() && self.settings.bg == 0).then(|| crate::config::prozent_alpha(60)));
        if let Some(c) = farbe {
            self.set_key("hintergrund_farbe", &format!("{c:06X}"));
        }
        if let Some(a) = alpha {
            self.set_key("hintergrund", &a.to_string());
        }
        self.load_config();
        self.render();
    }

    pub fn toggle_version(&mut self) {
        let an = !self.settings.version_zeigen;
        self.set_key("version", if an { "1" } else { "0" });
        self.load_config();
        self.render();
    }

    pub fn toggle_module(&mut self, m: &str) {
        self.settings.modules = toggle_module(&self.settings.modules, m);
        let v = self.settings.modules.join(" ");
        self.set_key("module", &v);
        self.hover = None;
        self.render();
    }

    // ------------------------------------------------------------ Ablauf
    pub fn update_visibility(&mut self) {
        let vis = !self.user_hidden && (self.settings.vollbild_zeigen || !fullscreen_active());
        if vis != self.visible {
            self.visible = vis;
            if vis {
                self.render();
            }
            unsafe { ShowWindow(self.hwnd, if vis { SW_SHOWNOACTIVATE } else { SW_HIDE }) };
        }
    }

    pub fn set_user_hidden(&mut self, h: bool) {
        self.user_hidden = h;
        self.update_visibility();
    }

    /// Timer (alle `anzeige_sekunden` Sekunden): Systemwerte messen und neu zeichnen.
    pub fn tick(&mut self) {
        if mtime(&self.cfg_path) != self.cfg_mtime {
            self.load_config();
        }
        self.stats.sample();
        if let Some(g) = &mut self.gpu {
            g.sample();
        }
        self.cpu_temp = self.cputemp.wert();
        self.takte = self.takte.wrapping_add(1);
        if self.cpu_temp.is_none() && self.takte % 100 == 0 {
            self.cputemp.anstossen(); // Dienst neu starten, falls er weggefallen ist
        }
        self.update_power();
        self.update_visibility();
        self.render();
    }

    /// Neues Ping-Ergebnis: Graph nur dann einen Schritt weiter, wenn wirklich gepingt wurde.
    pub fn on_ping(&mut self) {
        let max = self.settings.graph_max;
        for h in &mut self.hosts {
            let (s, seq) = *h.status.lock().unwrap();
            if seq != h.seen {
                h.seen = seq;
                push_hist(&mut h.hist, if let Status::Ok(ms) = s { ms as f32 } else { max });
                push_hist(&mut h.zeit, jetzt());
            }
        }
        self.render();
    }

    // ------------------------------------------------------------ Text-Helfer
    fn measure_font(&self, s: &str, font: HFONT) -> i32 {
        let ws: Vec<u16> = s.encode_utf16().collect();
        let mut sz = SIZE { cx: 0, cy: 0 };
        unsafe {
            let old = if font.is_null() { null_mut() } else { SelectObject(self.text.dc, font as HGDIOBJ) };
            GetTextExtentPoint32W(self.text.dc, ws.as_ptr(), ws.len() as i32, &mut sz);
            if !old.is_null() {
                SelectObject(self.text.dc, old);
            }
        }
        sz.cx
    }

    fn measure(&self, s: &str) -> i32 {
        self.measure_font(s, null_mut())
    }

    /// Text in die aktuelle Zeile; bei `right` ist x die rechte Kante. Gibt die belegte Breite zurueck.
    fn put(&self, f: &mut Frame, s: &str, x: i32, right: bool, maxw: i32, col: u32) -> i32 {
        self.put_font(f, s, x, right, maxw, col, null_mut())
    }

    #[allow(clippy::too_many_arguments)]
    fn put_font(&self, f: &mut Frame, s: &str, x: i32, right: bool, maxw: i32, col: u32, font: HFONT) -> i32 {
        let tw = self.measure_font(s, font).min(maxw.max(0));
        if tw <= 0 {
            return 0;
        }
        let x0 = if right { x - tw } else { x };
        f.ops.push(TextOp { s: s.encode_utf16().collect(), rc: rect(x0, f.y, x0 + tw, f.y + f.rh), col, font });
        tw
    }

    // ------------------------------------------------------------ Elemente
    fn el_kopf(&self, f: &mut Frame) {
        let up = self.stats.uptime;
        let (d, hh, mm) = (up / 86400, up / 3600 % 24, up / 60 % 60);
        let ut = if d > 0 { format!("up {d}d {hh:02}:{mm:02}") } else { format!("up {hh:02}:{mm:02}") };
        let uw = self.put(f, &ut, f.w - f.pad, true, f.inner, self.settings.dim);
        self.put(f, &self.computer, f.pad, false, f.inner - uw - f.gap, self.settings.accent);
        f.y += f.rh;
    }

    fn el_uhr(&self, f: &mut Frame) {
        let t = unsafe {
            let mut t = zeroed::<SYSTEMTIME>();
            windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t);
            t
        };
        let tw = self.put(f, &format!("{:02}:{:02}", t.wHour, t.wMinute), f.w - f.pad, true, f.inner, self.settings.fg);
        let kw = iso_week(t.wYear as i32, t.wMonth as u32, t.wDay as u32);
        let d = format!("{} {:02}.{:02}.{}  KW{}", WOCHENTAG[t.wDayOfWeek as usize % 7], t.wDay, t.wMonth, t.wYear, kw);
        self.put(f, &d, f.pad, false, f.inner - tw - f.gap, self.settings.dim);
        f.hits.push((f.y, f.y + f.rh, Hit::Uhr));
        f.y += f.rh;
    }

    fn el_energie(&self, f: &mut Frame) {
        let st = &self.settings;
        let plan = self.plan.clone().unwrap_or_default();
        let name = self.plan_names.iter().find(|(g, _)| *g == plan).map_or("?".to_string(), |x| x.1.clone());
        let col = if plan == st.plan_game { st.accent } else { st.dim };
        let right = match (&self.game, self.hover == Some(Hit::Power)) {
            (_, true) => "Klick: wechseln".to_string(),
            (Some(g), _) => g.clone(),
            (None, _) if st.power_auto => "auto".to_string(),
            (None, _) => "manuell".to_string(),
        };
        let rcol = if self.game.is_some() { COL_YELLOW } else { st.dim };
        let rw = self.put(f, &right, f.w - f.pad, true, f.inner, rcol);
        let lw = self.put(f, "PWR", f.pad, false, f.inner, st.fg);
        self.put(f, &name, f.pad + lw + f.gap, false, f.inner - lw - rw - 2 * f.gap, col);
        f.hits.push((f.y, f.y + f.rh, Hit::Power));
        f.y += f.rh;
    }

    /// CPU und GPU: oben Name und Last, darunter Takt und Temperatur in festen Spalten, damit beide
    /// Elemente genau untereinander stehen. Fehlt ein Wert (CPU-Temperatur ohne Adminrechte), steht dort "–".
    #[allow(clippy::too_many_arguments)]
    fn werte_zeile(&self, f: &mut Frame, label: &str, name: &str, ghz: Option<f32>, temp: Option<f32>, pct: f32, zeit: Option<u32>) {
        let st = &self.settings;
        let pcol = self.measure("100%");
        self.put(f, &format!("{pct:.0}%"), f.w - f.pad, true, f.inner, self.level(pct));
        let lw = self.put(f, label, f.pad, false, f.inner, st.fg);
        let links = f.pad + lw + f.gap;
        let breite = f.inner - lw - pcol - 2 * f.gap;
        match zeit {
            // Maus auf dem Graphen: Uhrzeit dieser Messung statt Name
            Some(z) => self.put(f, &hms(z), links, false, breite, st.accent),
            None => self.put(f, name, links, false, breite, st.dim),
        };
        f.y += f.rh;
        let gw = self.measure("9.9 GHz");
        let tw = self.measure("100\u{b0}C");
        match ghz {
            Some(g) => self.put(f, &format!("{g:.1} GHz"), links, false, gw, st.accent),
            None => self.put(f, "\u{2013} GHz", links, false, gw, st.dim),
        };
        let tx = links + gw + 2 * f.gap;
        match temp {
            Some(t) => {
                let col = if t >= 90.0 { COL_RED } else if t >= 80.0 { COL_YELLOW } else { st.accent };
                self.put(f, &format!("{t:.0}\u{b0}C"), tx, false, tw, col)
            }
            None => self.put(f, "\u{2013}\u{b0}C", tx, false, tw, st.dim),
        };
    }

    fn el_cpu(&self, f: &mut Frame) {
        let (y0, st) = (f.y, &self.stats);
        let sh = self.px(SPARK_H);
        let gr = rect(f.pad, f.y + 2 * f.rh, f.w - f.pad, f.y + 2 * f.rh + sh);
        let p = self.punkt(&gr, st.cpu_hist.len());
        let pct = p.map_or(st.cpu, |i| st.cpu_hist[i] * 100.0);
        self.werte_zeile(f, "CPU", &st.cpu_name, (st.ghz > 0.0).then_some(st.ghz), self.cpu_temp, pct, p.map(|i| st.zeit_hist[i]));
        f.y += f.rh;
        f.dr.push(Draw::Spark(gr, st.cpu_hist.iter().copied().collect(), self.settings.fg));
        self.marke(f, gr, p);
        f.y += sh + self.px(3.0);
        f.hits.push((y0, f.y, Hit::Cpu));
    }

    /// GPU: Name, Takt, Temperatur und Last, darunter der Verlauf wie bei CPU und Netz. Hover zeigt den Grafikspeicher.
    fn el_gpu(&self, f: &mut Frame) {
        let Some(g) = &self.gpu else { return };
        let y0 = f.y;
        let sh = self.px(SPARK_H);
        let gr = rect(f.pad, f.y + 2 * f.rh, f.w - f.pad, f.y + 2 * f.rh + sh);
        let p = self.punkt(&gr, g.hist.len());
        let pct = p.map_or(g.load, |i| g.hist[i] * 100.0);
        let name = if self.hover == Some(Hit::Gpu) && g.vram_total > 0 {
            format!("{} {:.1}/{:.0} GB", g.speicher_name(), gb(g.vram_used as f64), gb(g.vram_total as f64))
        } else {
            g.name.clone()
        };
        self.werte_zeile(f, "GPU", &name, g.ghz, g.temp, pct, p.map(|i| g.zeit[i]));
        f.y += f.rh;
        f.dr.push(Draw::Spark(gr, g.hist.iter().copied().collect(), self.settings.fg));
        self.marke(f, gr, p);
        f.y += sh + self.px(3.0);
        f.hits.push((y0, f.y, Hit::Gpu));
    }

    fn el_ram(&self, f: &mut Frame) {
        let (y0, st) = (f.y, &self.stats);
        let (t, a) = (st.mem_total, st.mem_avail);
        let pct = if t > 0.0 { ((t - a) / t * 100.0) as f32 } else { 0.0 };
        let detail = if self.hover == Some(Hit::Ram) { format!("frei {:.1} GB", gb(a)) } else { format!("{:.1} / {:.1} GB", gb(t - a), gb(t)) };
        let pcol = self.measure("100%");
        self.put(f, &format!("{pct:.0}%"), f.w - f.pad, true, f.inner, self.level(pct));
        let lw = self.put(f, "RAM", f.pad, false, f.inner, self.settings.fg);
        self.put(f, &detail, f.pad + lw + f.gap, false, f.inner - lw - pcol - 2 * f.gap, self.settings.dim);
        f.y += f.rh;
        let bh = self.px(BAR_H);
        f.dr.push(Draw::Bar(rect(f.pad, f.y, f.w - f.pad, f.y + bh), pct / 100.0, self.level(pct)));
        f.y += bh + self.px(4.0);
        f.hits.push((y0, f.y, Hit::Ram));
    }

    fn el_platten(&self, f: &mut Frame) {
        let pcol = self.measure("100%");
        for (i, d) in self.stats.disks.iter().enumerate() {
            let pct = ((1.0 - d.free as f64 / d.total as f64) * 100.0) as f32;
            let letter = &d.root[..2];
            if self.hover == Some(Hit::Disk(i)) {
                let s = format!("{letter} {:.0} GB frei / {:.0} GB", gb(d.free as f64), gb(d.total as f64));
                self.put(f, &s, f.pad, false, f.inner, self.settings.fg);
            } else {
                self.put(f, &format!("{pct:.0}%"), f.w - f.pad, true, f.inner, self.level(pct));
                let lw = self.put(f, letter, f.pad, false, f.inner, self.settings.fg);
                let bh = self.px(DISK_BAR_H);
                let by = f.y + (f.rh - bh) / 2;
                f.dr.push(Draw::Bar(rect(f.pad + lw + f.gap, by, f.w - f.pad - pcol - f.gap, by + bh), pct / 100.0, self.level(pct)));
            }
            f.hits.push((f.y, f.y + f.rh, Hit::Disk(i)));
            f.y += f.rh;
        }
    }

    fn el_netz(&self, f: &mut Frame) {
        let (y0, st) = (f.y, &self.stats);
        let nh = self.px(NET_SPARK_H);
        let gr = rect(f.pad, f.y + f.rh, f.w - f.pad, f.y + f.rh + nh);
        let p = self.punkt(&gr, st.net_hist.len());
        // Mit Maus auf dem Graphen: Uhrzeit und die Werte dieser Messung
        let (down, up, zeit) = p.map_or((st.down, st.up, None), |i| (st.net_hist[i] as f64, st.up_hist[i] as f64, Some(st.zeit_hist[i])));
        let uw = self.put(f, &format!("\u{2191} {}", rate(up)), f.w - f.pad, true, f.inner, self.settings.accent);
        let zw = zeit.map_or(0, |z| self.put(f, &hms(z), f.pad, false, f.inner, self.settings.fg) + f.gap);
        self.put(f, &format!("\u{2193} {}", rate(down)), f.pad + zw, false, f.inner - uw - zw - f.gap, self.settings.accent);
        f.y += f.rh;
        let max = st.net_hist.iter().fold(1024.0f32, |m, &v| m.max(v));
        f.dr.push(Draw::Spark(gr, st.net_hist.iter().map(|v| v / max).collect(), self.settings.accent));
        self.marke(f, gr, p);
        f.y += nh + self.px(3.0);
        f.hits.push((y0, f.y, Hit::Net));
    }

    fn el_ping(&self, f: &mut Frame) {
        let dotw = self.px(12.0);
        let gmax = self.settings.graph_max;
        for (i, host) in self.hosts.iter().enumerate() {
            let yh = f.y;
            let s = host.status.lock().unwrap().0;
            let col = match s {
                Status::Pending => COL_GRAY,
                Status::Ok(ms) if ms < host.cfg.yellow => self.settings.fg,
                Status::Ok(ms) if ms < host.cfg.red => COL_YELLOW,
                _ => COL_RED,
            };
            let val = match s {
                Status::Pending => "-".to_string(),
                Status::Ok(ms) => format!("{ms} ms"),
                Status::Timeout => "TIMED OUT".to_string(),
            };
            // Graph UNTER der Zeile, damit sich darueber nichts verschiebt
            let gh = self.px(GRAPH_H);
            let gr = rect(f.pad, f.y + f.rh + 1, f.w - f.pad, f.y + f.rh + 1 + gh);
            let p = if host.cfg.graph { self.punkt(&gr, host.hist.len()) } else { None };
            // Mit Maus auf dem Graphen: Uhrzeit und Wert dieser Messung (ein Timeout steht als graph_max im Verlauf)
            let (val, col) = p.map_or((val, col), |k| {
                let v = host.hist[k];
                match v as u32 {
                    _ if v == gmax => ("TIMED OUT".to_string(), COL_RED),
                    ms if ms < host.cfg.yellow => (format!("{ms} ms"), self.settings.fg),
                    ms if ms < host.cfg.red => (format!("{ms} ms"), COL_YELLOW),
                    ms => (format!("{ms} ms"), COL_RED),
                }
            });
            let vw = self.put(f, &val, f.w - f.pad - dotw, true, f.inner, col);
            let zeit = p.map(|k| hms(host.zeit[k]));
            let name = zeit.as_ref().unwrap_or(if self.hover == Some(Hit::Ping(i)) { &host.cfg.addr } else { &host.cfg.name });
            self.put(f, name, f.pad, false, f.inner - dotw - vw - f.gap, self.settings.fg);
            f.dr.push(Draw::Dot((f.w - f.pad) as f32 - DOT_R * self.scale - 1.0, f.y as f32 + f.rh as f32 / 2.0, col));
            f.y += f.rh;
            if host.cfg.graph {
                f.dr.push(Draw::Line(gr, host.hist.iter().map(|v| v / gmax).collect(), self.settings.accent));
                self.marke(f, gr, p);
                f.y += gh + self.px(3.0);
            }
            f.hits.push((yh, f.y, Hit::Ping(i)));
        }
    }

    // ------------------------------------------------------------ Zeichnen
    pub fn render(&mut self) {
        if !self.visible {
            return;
        }
        if let Some(p) = self.plan.clone() {
            self.plan_label(&p);
        }
        let w = self.px(self.settings.width);
        let pad = self.px(PAD);
        let mut f = Frame { ops: Vec::new(), dr: Vec::new(), hits: Vec::new(), graphs: Vec::new(), y: self.px(3.0), w, pad, gap: self.px(GAP), rh: self.px(ROW_H), inner: w - 2 * pad };

        // Elemente ohne Daten (GPU nicht erkannt) ganz weglassen, auch ohne Trennlinie
        let modules: Vec<&String> = self.settings.modules.iter().filter(|m| *m != "gpu" || self.gpu.is_some()).collect();
        // Versionsnummer als eigene Zeile ganz oben, mittig und gedaempft
        let version = self.settings.version_zeigen;
        if version {
            let v = concat!("Sysglow v", env!("CARGO_PKG_VERSION"));
            let vw = self.measure(v).min(f.inner);
            let inner = f.inner;
            self.put(&mut f, v, (w - vw) / 2, false, inner, self.settings.dim);
            f.y += f.rh;
        }
        for (mi, m) in modules.into_iter().enumerate() {
            if mi > 0 || version {
                let sh = self.px(SEP_H);
                f.dr.push(Draw::Fill(rect(pad, f.y + sh / 2, w - pad, f.y + sh / 2 + 1), self.settings.dim, 0.5));
                f.y += sh;
            }
            match m.as_str() {
                "kopf" => self.el_kopf(&mut f),
                "uhr" => self.el_uhr(&mut f),
                "energie" => self.el_energie(&mut f),
                "cpu" => self.el_cpu(&mut f),
                "gpu" => self.el_gpu(&mut f),
                "ram" => self.el_ram(&mut f),
                "platten" => self.el_platten(&mut f),
                "netz" => self.el_netz(&mut f),
                "ping" => self.el_ping(&mut f),
                _ => {}
            }
        }
        let h = (f.y + self.px(3.0)).max(1);

        unsafe {
            if self.size != (w, h) {
                resize_surface(&mut self.text, w, h);
                resize_surface(&mut self.out, w, h);
                self.size = (w, h);
            }
            if self.text.bits.is_null() || self.out.bits.is_null() {
                return;
            }
            compose(&self.text, &self.out, w, h, self.settings.bg, self.settings.bg_farbe, self.scale, &f.dr, &f.ops);

            // Eigene Position oder oben rechts, immer im sichtbaren Bereich
            let pos = match self.settings.pos {
                Some((px, py)) => {
                    let (vx, vy) = (GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN));
                    let (vw, vh) = (GetSystemMetrics(SM_CXVIRTUALSCREEN), GetSystemMetrics(SM_CYVIRTUALSCREEN));
                    POINT { x: px.clamp(vx, (vx + vw - w).max(vx)), y: py.clamp(vy, (vy + vh - h).max(vy)) }
                }
                None => POINT { x: GetSystemMetrics(SM_CXSCREEN) - w - self.settings.margin_right, y: self.settings.margin_top },
            };
            let size = SIZE { cx: w, cy: h };
            let src = POINT { x: 0, y: 0 };
            let bf = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
            UpdateLayeredWindow(self.hwnd, null_mut(), &pos, &size, self.out.dc, &src, 0, &bf, ULW_ALPHA);
        }
        self.hits = f.hits;
        self.graphs = f.graphs;
    }
}

#[cfg(test)]
mod tests {
    use super::iso_week;
    #[test]
    fn kalenderwoche() {
        assert_eq!(iso_week(2026, 11, 7), 45);
        assert_eq!(iso_week(2026, 1, 1), 1);
        assert_eq!(iso_week(2021, 1, 3), 53);
        assert_eq!(iso_week(2024, 12, 30), 1);
        assert_eq!(iso_week(2020, 12, 31), 53);
    }
}
