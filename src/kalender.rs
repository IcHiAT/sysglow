//! Kalender-Popup: oeffnet per Klick auf das Datum, oben links neben dem Widget, Rand in der Schemafarbe.
//! Das Fenster wird einmal angelegt und danach nur ein- und ausgeblendet.
//! Bedienung: < > oder Mausrad = Monat wechseln, Klick auf den Titel = heute, Esc oder Klick daneben = zu.
//! Termine aus den ini-Zeilen "kalender = ..." werden markiert und unter dem Monat aufgelistet;
//! ziehen = verschieben, Rechtsklick = Kalender an/aus und neu laden.

use std::cell::RefCell;
use std::mem::zeroed;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, VK_LEFT, VK_RIGHT};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::app::iso_week;
use crate::config::{Art, KalQuelle, Settings};
use crate::termine::{self, Termin};
use crate::gfx::*;
use crate::wide;

// Masse bei 100 % Skalierung
const PAD: f32 = 8.0;
const ZELLE_B: f32 = 25.0;
const ZELLE_H: f32 = 17.0;
const KOPF_H: f32 = 20.0;
const ABSTAND: f32 = 6.0;
/// Deckkraft des Hintergrunds (leicht durchscheinend)
const DECKKRAFT: u8 = 215;
/// Hoechstens so viele Zeilen in der Terminliste
const LISTE_MAX: usize = 8;
/// Timer fuer die Geburtstags-Animation (ms)
const TIMER_ANIM: usize = 7;
const ANIM_MS: u32 = 70;
const MENUE_NEU_LADEN: usize = 1000;

const MONATE: [&str; 12] = ["Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"];
const TAGE: [&str; 7] = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];

fn schaltjahr(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn tage_im_monat(y: i32, m: u32) -> u32 {
    match m {
        2 => 28 + u32::from(schaltjahr(y)),
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Wochentag des Monatsersten: 0 = Montag .. 6 = Sonntag.
fn erster_wochentag(y: i32, m: u32) -> u32 {
    let vor = (y - 1) as i64;
    let tage_davor: u32 = (1..m).map(|k| tage_im_monat(y, k)).sum();
    let tage = vor * 365 + vor / 4 - vor / 100 + vor / 400 + tage_davor as i64;
    tage.rem_euclid(7) as u32 // 1.1.0001 war ein Montag
}

/// Die 42 Tage des Monatsblatts als (Jahr, Monat, Tag), beginnend mit dem Montag vor dem Monatsersten.
fn blatt(y: i32, m: u32) -> Vec<(i32, u32, u32)> {
    let (vy, vm) = if m == 1 { (y - 1, 12) } else { (y, m - 1) };
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    let vl = tage_im_monat(vy, vm);
    let vorne = erster_wochentag(y, m);
    let mut v: Vec<_> = (vl - vorne + 1..=vl).map(|d| (vy, vm, d)).collect();
    v.extend((1..=tage_im_monat(y, m)).map(|d| (y, m, d)));
    let rest = 42 - v.len() as u32;
    v.extend((1..=rest).map(|d| (ny, nm, d)));
    v
}

struct Kal {
    hwnd: HWND,
    text: Surface,
    out: Surface,
    scale: f32,
    fg: u32,
    dim: u32,
    accent: u32,
    jahr: i32,
    monat: u32,
    heute: (i32, u32, u32),
    pos: POINT,
    size: (i32, i32),
    offen: bool,
    /// Farben fuer Termin, Urlaub, Geburtstag
    farben: [u32; 3],
    quellen: Vec<KalQuelle>,
    /// Position nach dem Verschieben (gilt bis zum Programmende)
    verschoben: Option<POINT>,
    phase: u32,
    anim: bool,
}

/// Ein Eintrag der Terminliste: alle Tage im Monat, Art, Titel.
struct Eintrag {
    tage: Vec<u32>,
    art: Art,
    titel: String,
}

/// Tage als Text: zusammenhaengende Tage als Spanne ("20.–23."), sonst einzeln ("3./10./17.").
fn tage_text(tage: &[u32]) -> String {
    let mut teile: Vec<String> = Vec::new();
    let mut i = 0;
    while i < tage.len() {
        let mut j = i;
        while j + 1 < tage.len() && tage[j + 1] == tage[j] + 1 {
            j += 1;
        }
        teile.push(if i == j { format!("{}.", tage[i]) } else { format!("{}.–{}.", tage[i], tage[j]) });
        i = j + 1;
    }
    teile.join("/")
}

thread_local! {
    static KAL: RefCell<Option<Kal>> = const { RefCell::new(None) };
}

/// Bei Wiedereintritt (Fenstermeldungen waehrend ShowWindow) wird nichts getan.
fn mit_kal<R>(f: impl FnOnce(&mut Kal) -> R) -> Option<R> {
    KAL.with(|k| k.try_borrow_mut().ok().and_then(|mut k| k.as_mut().map(f)))
}

fn heute() -> (i32, u32, u32) {
    let t = unsafe {
        let mut t = zeroed::<SYSTEMTIME>();
        GetLocalTime(&mut t);
        t
    };
    (t.wYear as i32, t.wMonth as u32, t.wDay as u32)
}

impl Kal {
    fn px(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }

    /// Groesse; `zeilen` = Zeilen der Terminliste (0 = keine Liste).
    fn masse(&self, zeilen: usize) -> (i32, i32) {
        let liste = if zeilen > 0 { self.px(6.0) + zeilen as i32 * self.px(ZELLE_H) } else { 0 };
        (2 * self.px(PAD) + 8 * self.px(ZELLE_B), 2 * self.px(PAD) + self.px(KOPF_H) + 7 * self.px(ZELLE_H) + self.px(4.0) + liste)
    }

    fn farbe(&self, a: Art) -> u32 {
        self.farben[a as usize]
    }

    /// Termine des angezeigten Monats, nach Tag sortiert.
    fn eintraege(&self, termine: &[Termin]) -> Vec<Eintrag> {
        let n = tage_im_monat(self.jahr, self.monat);
        let mut v: Vec<Eintrag> = termine
            .iter()
            .filter_map(|t| {
                let tage: Vec<u32> = (1..=n).filter(|d| t.an((self.jahr, self.monat, *d))).collect();
                (!tage.is_empty()).then(|| Eintrag { tage, art: t.art, titel: t.titel.clone() })
            })
            .collect();
        v.sort_by_key(|e| (e.tage[0], e.tage.len()));
        v
    }

    /// Ab hier liegt die Liste; darueber Kopf und Monatsblatt.
    fn liste_y(&self) -> i32 {
        self.masse(0).1 - self.px(PAD)
    }

    /// Text waagerecht zentriert in den Bereich x0..x1.
    fn mittig(&self, ops: &mut Vec<TextOp>, s: &str, x0: i32, x1: i32, y0: i32, y1: i32, col: u32) {
        let ws: Vec<u16> = s.encode_utf16().collect();
        let mut sz = SIZE { cx: 0, cy: 0 };
        unsafe { GetTextExtentPoint32W(self.text.dc, ws.as_ptr(), ws.len() as i32, &mut sz) };
        let x = x0 + (x1 - x0 - sz.cx) / 2;
        ops.push(TextOp { s: ws, rc: rect(x, y0, x + sz.cx, y1), col, font: null_mut() });
    }

    fn monat_plus(&mut self, d: i32) {
        let t = self.jahr * 12 + self.monat as i32 - 1 + d;
        self.jahr = t.div_euclid(12);
        self.monat = t.rem_euclid(12) as u32 + 1;
        self.zeichnen();
    }

    fn klick(&mut self, x: i32, y: i32) {
        let (pad, kopf, breite) = (self.px(PAD), self.px(KOPF_H), self.px(ZELLE_B) * 3 / 2);
        if y < pad || y >= pad + kopf {
            return;
        }
        let w = self.masse(0).0;
        if x < pad + breite {
            self.monat_plus(-1);
        } else if x >= w - pad - breite {
            self.monat_plus(1);
        } else {
            (self.jahr, self.monat, _) = self.heute;
            self.zeichnen();
        }
    }

    fn zeichnen(&mut self) {
        let (termine, fehler, laedt) = {
            let s = termine::STAND.lock().unwrap();
            (s.termine.clone(), s.fehler.clone(), s.laedt)
        };
        let aktiv = self.quellen.iter().any(|q| q.an);
        let liste = if aktiv { self.eintraege(&termine) } else { Vec::new() };
        let mut zeilen: Vec<(String, u32)> = liste
            .iter()
            .take(if liste.len() > LISTE_MAX { LISTE_MAX - 1 } else { LISTE_MAX })
            .map(|e| {
                (format!("{}  {}", tage_text(&e.tage), e.titel), self.farbe(e.art))
            })
            .collect();
        if liste.len() > zeilen.len() {
            zeilen.push((format!("… und {} weitere", liste.len() - zeilen.len()), self.dim));
        }
        if aktiv {
            if laedt && termine.is_empty() {
                zeilen.push(("Termine werden geladen …".into(), self.dim));
            } else if let Some(f) = fehler.first() {
                zeilen.push((format!("Nicht geladen: {f}"), COL_RED));
            } else if zeilen.is_empty() {
                zeilen.push(("Keine Termine in diesem Monat".into(), self.dim));
            }
        }
        let (w, h) = self.masse(zeilen.len());
        let (pad, kopf, zb, zh) = (self.px(PAD), self.px(KOPF_H), self.px(ZELLE_B), self.px(ZELLE_H));
        let (mut ops, mut dr) = (Vec::new(), Vec::new());

        // Rand in der Schemafarbe
        let d = self.px(1.0).max(1);
        for r in [rect(0, 0, w, d), rect(0, h - d, w, h), rect(0, d, d, h - d), rect(w - d, d, w, h - d)] {
            dr.push(Draw::Fill(r, self.fg, 0.9));
        }

        // Kopf: Pfeile und Monat
        let y = pad;
        self.mittig(&mut ops, "<", pad, pad + zb, y, y + kopf, self.accent);
        self.mittig(&mut ops, ">", w - pad - zb, w - pad, y, y + kopf, self.accent);
        self.mittig(&mut ops, &format!("{} {}", MONATE[self.monat as usize - 1], self.jahr), pad, w - pad, y, y + kopf, self.fg);

        // Spaltenkoepfe
        let y = y + kopf;
        self.mittig(&mut ops, "KW", pad, pad + zb, y, y + zh, self.dim);
        for (i, t) in TAGE.iter().enumerate() {
            let x = pad + zb * (i as i32 + 1);
            self.mittig(&mut ops, t, x, x + zb, y, y + zh, if i >= 5 { self.accent } else { self.dim });
        }
        let y = y + zh;
        dr.push(Draw::Fill(rect(pad, y + self.px(1.0), w - pad, y + self.px(1.0) + d), self.dim, 0.5));
        let y = y + self.px(4.0);

        // Tage, immer sechs Zeilen, damit die Hoehe gleich bleibt
        let mut gb = false;
        for (zeile, woche) in blatt(self.jahr, self.monat).chunks(7).enumerate() {
            let yz = y + zh * zeile as i32;
            self.mittig(&mut ops, &iso_week(woche[0].0, woche[0].1, woche[0].2).to_string(), pad, pad + zb, yz, yz + zh, self.dim);
            for (i, t) in woche.iter().enumerate() {
                let x = pad + zb * (i as i32 + 1);
                let col = if t.1 != self.monat {
                    COL_GRAY
                } else if i >= 5 {
                    self.accent
                } else {
                    self.fg
                };
                let im_monat = t.1 == self.monat;
                let hat = |a: Art| aktiv && termine.iter().any(|e| e.art == a && e.an(*t));
                let (urlaub, termin, geburtstag) = (hat(Art::Urlaub), hat(Art::Termin), hat(Art::Geburtstag));
                let staerke = if im_monat { 1.0 } else { 0.45 };
                if urlaub {
                    dr.push(Draw::Fill(rect(x + d, yz, x + zb - d, yz + zh), self.farbe(Art::Urlaub), 0.32 * staerke));
                }
                if *t == self.heute {
                    dr.push(Draw::Fill(rect(x + d, yz, x + zb - d, yz + zh), self.fg, 0.3));
                }
                if termin {
                    let s = self.px(2.0).max(2);
                    dr.push(Draw::Fill(rect(x + zb / 4, yz + zh - s, x + zb - zb / 4, yz + zh), self.farbe(Art::Termin), 0.95 * staerke));
                }
                let col = if geburtstag && im_monat { self.farbe(Art::Geburtstag) } else { col };
                if geburtstag && im_monat {
                    gb = true;
                    self.ballon(&mut dr, x + zb, yz, (t.2 * 7 + zeile as u32 * 3) as f32);
                }
                self.mittig(&mut ops, &t.2.to_string(), x, x + zb, yz, yz + zh, col);
            }
        }

        // Terminliste unter dem Monatsblatt
        if !zeilen.is_empty() {
            let y0 = self.liste_y();
            dr.push(Draw::Fill(rect(pad, y0 + self.px(2.0), w - pad, y0 + self.px(2.0) + d), self.dim, 0.5));
            for (i, (s, col)) in zeilen.iter().enumerate() {
                let y = y0 + self.px(6.0) + zh * i as i32;
                ops.push(TextOp { s: s.encode_utf16().collect(), rc: rect(pad + self.px(2.0), y, w - pad, y + zh), col: *col, font: null_mut() });
            }
        }

        // Animation nur, solange ein Geburtstag zu sehen ist
        if gb != self.anim {
            self.anim = gb;
            unsafe {
                if gb {
                    SetTimer(self.hwnd, TIMER_ANIM, ANIM_MS, None);
                } else {
                    KillTimer(self.hwnd, TIMER_ANIM);
                }
            }
        }

        unsafe {
            if self.size != (w, h) {
                resize_surface(&mut self.text, w, h);
                resize_surface(&mut self.out, w, h);
                self.size = (w, h);
            }
            if self.text.bits.is_null() || self.out.bits.is_null() {
                return;
            }
            compose(&self.text, &self.out, w, h, DECKKRAFT, 0, self.scale, &dr, &ops);
            let size = SIZE { cx: w, cy: h };
            let src = POINT { x: 0, y: 0 };
            let bf = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
            UpdateLayeredWindow(self.hwnd, null_mut(), &self.pos, &size, self.out.dc, &src, 0, &bf, ULW_ALPHA);
        }
    }

    /// Kleiner Luftballon oben rechts in der Zelle, der auf und ab schwebt, dazu ein Funkeln.
    fn ballon(&self, dr: &mut Vec<Draw>, rechts: i32, oben: i32, versatz: f32) {
        let p = self.phase as f32 * 0.18 + versatz;
        let s = self.scale;
        let cx = rechts as f32 - 4.0 * s;
        let cy = oben as f32 + 4.0 * s + p.sin() * 1.3 * s;
        let pink = self.farbe(Art::Geburtstag);
        dr.push(Draw::Dot(cx, cy, pink));
        let (fx, fy) = (cx.round() as i32, (cy + 3.0 * s).round() as i32);
        dr.push(Draw::Fill(rect(fx, fy, fx + self.px(1.0).max(1), fy + self.px(4.0)), pink, 0.6));
        // Funkeln: ein kleiner Stern links vom Ballon, der auf- und abblendet
        let a = ((p * 1.7).sin() * 0.5 + 0.5).powi(3);
        if a > 0.05 {
            let (sx, sy, r, d) = ((cx - 6.5 * s) as i32, (cy - 2.0 * s) as i32, self.px(2.0).max(2), self.px(1.0).max(1));
            dr.push(Draw::Fill(rect(sx - r, sy, sx + r + d, sy + d), 0xFFFFFF, a));
            dr.push(Draw::Fill(rect(sx, sy - r, sx + d, sy + r + d), 0xFFFFFF, a));
        }
    }

    /// Rechtsklick: Kalender an/aus (schreibt die ini) und neu laden.
    unsafe fn menue(&mut self, x: i32, y: i32) {
        let m = CreatePopupMenu();
        for (i, q) in self.quellen.iter().enumerate() {
            AppendMenuW(m, MF_STRING | if q.an { MF_CHECKED } else { 0 }, i + 1, wide(&q.name).as_ptr());
        }
        if self.quellen.is_empty() {
            AppendMenuW(m, MF_STRING | MF_GRAYED, 0, wide("Keine Kalender in der ini (kalender = ...)").as_ptr());
        }
        AppendMenuW(m, MF_SEPARATOR, 0, null());
        AppendMenuW(m, MF_STRING, MENUE_NEU_LADEN, wide("Termine neu laden").as_ptr());
        let wahl = TrackPopupMenu(m, TPM_RETURNCMD | TPM_RIGHTBUTTON, x, y, 0, self.hwnd, null()) as usize;
        DestroyMenu(m);
        if wahl == MENUE_NEU_LADEN {
            termine::neu_laden();
        } else if (1..=self.quellen.len()).contains(&wahl) {
            let i = wahl - 1;
            if umschalten_in_ini(&self.quellen[i]) {
                self.quellen[i].an = !self.quellen[i].an;
                termine::quellen_setzen(&self.quellen);
                self.zeichnen();
            }
        }
    }

    fn zu(&mut self) {
        if self.offen {
            self.offen = false;
            if self.anim {
                self.anim = false;
                unsafe { KillTimer(self.hwnd, TIMER_ANIM) };
            }
            unsafe { ShowWindow(self.hwnd, SW_HIDE) };
        }
    }
}

/// Setzt die Spalte an/aus der Zeile dieser Quelle in der ini um.
fn umschalten_in_ini(q: &KalQuelle) -> bool {
    let pfad = crate::config::ini_pfad();
    let Ok(text) = std::fs::read_to_string(&pfad) else { return false };
    let mut zeilen: Vec<String> = text.lines().map(String::from).collect();
    let Some(z) = zeilen.get_mut(q.line) else { return false };
    let Some((k, v)) = z.split_once('=') else { return false };
    if !k.trim().trim_start_matches('\u{feff}').eq_ignore_ascii_case("kalender") || crate::config::parse_kalender(v, q.line).as_ref() != Some(q) {
        return false;
    }
    let mut p: Vec<String> = v.splitn(4, '|').map(|s| s.trim().to_string()).collect();
    p[2] = if q.an { "0" } else { "1" }.into();
    *z = format!("{} = {}", k.trim(), p.join(" | "));
    std::fs::write(&pfad, zeilen.join("\r\n") + "\r\n").is_ok()
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        // Ausserhalb der Kopfzeile wie eine Titelleiste: ziehen verschiebt das Fenster
        WM_NCHITTEST => {
            let mut p = POINT { x: lp as i16 as i32, y: (lp >> 16) as i16 as i32 };
            ScreenToClient(hwnd, &mut p);
            let kopf = mit_kal(|k| p.y >= k.px(PAD) && p.y < k.px(PAD) + k.px(KOPF_H)).unwrap_or(true);
            if kopf {
                HTCLIENT as isize
            } else {
                HTCAPTION as isize
            }
        }
        WM_NCLBUTTONDBLCLK => 0,
        // Rechtsklick im Zieh-Bereich: Windows schickt dort kein WM_CONTEXTMENU, also selbst das Menue zeigen
        WM_NCRBUTTONDOWN if wp == HTCAPTION as usize => 0,
        WM_NCRBUTTONUP if wp == HTCAPTION as usize => {
            let (x, y) = (lp as i16 as i32, (lp >> 16) as i16 as i32);
            mit_kal(|k| k.menue(x, y));
            0
        }
        WM_MOVE => {
            let (x, y) = (lp as i16 as i32, (lp >> 16) as i16 as i32);
            mit_kal(|k| k.pos = POINT { x, y });
            0
        }
        WM_EXITSIZEMOVE => {
            mit_kal(|k| k.verschoben = Some(k.pos));
            0
        }
        WM_CONTEXTMENU => {
            let (mut x, mut y) = (lp as i16 as i32, (lp >> 16) as i16 as i32);
            if x == -1 && y == -1 {
                let mut rc = zeroed::<RECT>();
                GetWindowRect(hwnd, &mut rc);
                (x, y) = (rc.left, rc.top);
            }
            mit_kal(|k| k.menue(x, y));
            0
        }
        WM_TIMER if wp == TIMER_ANIM => {
            mit_kal(|k| {
                k.phase = k.phase.wrapping_add(1);
                k.zeichnen();
            });
            0
        }
        termine::WM_TERMINE => {
            mit_kal(|k| {
                if k.offen {
                    k.zeichnen();
                }
            });
            0
        }
        WM_ACTIVATE => {
            if wp & 0xFFFF == WA_INACTIVE as usize {
                mit_kal(|k| k.zu());
            }
            0
        }
        WM_LBUTTONDOWN => {
            let (x, y) = (lp as i16 as i32, (lp >> 16) as i16 as i32);
            mit_kal(|k| k.klick(x, y));
            0
        }
        WM_MOUSEWHEEL => {
            let d = if (wp >> 16) as i16 > 0 { -1 } else { 1 };
            mit_kal(|k| k.monat_plus(d));
            0
        }
        WM_KEYDOWN => {
            match wp as u16 {
                VK_ESCAPE => {
                    mit_kal(|k| k.zu());
                }
                VK_LEFT => {
                    mit_kal(|k| k.monat_plus(-1));
                }
                VK_RIGHT => {
                    mit_kal(|k| k.monat_plus(1));
                }
                _ => {}
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe fn anlegen(font: HFONT) {
    let inst = GetModuleHandleW(null());
    let class = wide("SysglowKalender");
    let wc = WNDCLASSW {
        style: 0,
        lpfnWndProc: Some(wndproc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: inst,
        hIcon: null_mut(),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hbrBackground: null_mut(),
        lpszMenuName: null(),
        lpszClassName: class.as_ptr(),
    };
    RegisterClassW(&wc);
    let hwnd = CreateWindowExW(WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST, class.as_ptr(), wide("Sysglow Kalender").as_ptr(), WS_POPUP, 0, 0, 1, 1, null_mut(), null_mut(), inst, null());
    let k = Kal {
        hwnd,
        text: new_surface(font),
        out: new_surface(null_mut()),
        scale: 1.0,
        fg: 0,
        dim: 0,
        accent: 0,
        jahr: 2000,
        monat: 1,
        heute: (2000, 1, 1),
        pos: POINT { x: 0, y: 0 },
        size: (0, 0),
        offen: false,
        farben: [0; 3],
        quellen: Vec::new(),
        verschoben: None,
        phase: 0,
        anim: false,
    };
    termine::MELDEN_AN.store(hwnd as usize, std::sync::atomic::Ordering::SeqCst);
    KAL.with(|s| *s.borrow_mut() = Some(k));
}

/// Neue Grundschrift nach einem Zoom; der Kalender zeichnet mit der Schrift seiner Flaeche.
pub unsafe fn schrift(font: HFONT) {
    mit_kal(|k| {
        SelectObject(k.text.dc, font as HGDIOBJ);
    });
}

/// Klick auf das Datum: oeffnet den Kalender links neben dem Widget (`widget` = Fensterrechteck) oder schliesst ihn.
pub unsafe fn umschalten(widget: RECT, font: HFONT, scale: f32, st: &Settings) {
    if KAL.with(|k| k.borrow().is_none()) {
        anlegen(font);
    }
    let hwnd = mit_kal(|k| {
        if k.offen {
            k.zu();
            return None;
        }
        (k.scale, k.fg, k.dim, k.accent, k.farben) = (scale, st.fg, st.dim, st.accent, st.farben_termine);
        k.quellen = st.kalender.clone();
        k.heute = heute();
        (k.jahr, k.monat, _) = k.heute;
        let (w, _) = k.masse(0);
        let abstand = k.px(ABSTAND);
        let links = GetSystemMetrics(SM_XVIRTUALSCREEN);
        // Links neben dem Widget; steht das Widget am linken Rand, stattdessen rechts daneben
        let x = if widget.left - abstand - w >= links { widget.left - abstand - w } else { widget.right + abstand };
        k.pos = k.verschoben.unwrap_or(POINT { x, y: widget.top });
        k.zeichnen();
        k.offen = true;
        Some(k.hwnd)
    })
    .flatten();
    if let Some(h) = hwnd {
        ShowWindow(h, SW_SHOW);
        SetForegroundWindow(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wochentag_des_ersten() {
        assert_eq!(erster_wochentag(2026, 11), 6); // 1.11.2026 ist ein Sonntag
        assert_eq!(erster_wochentag(2024, 2), 3); // 1.2.2024 ist ein Donnerstag
        assert_eq!(erster_wochentag(2026, 6), 0); // 1.6.2026 ist ein Montag
    }

    #[test]
    fn blatt_hat_42_tage_ab_montag() {
        let b = blatt(2026, 11);
        assert_eq!(b.len(), 42);
        assert_eq!(b[0], (2026, 10, 26));
        assert_eq!(b[6], (2026, 11, 1));
        assert_eq!(b[41], (2026, 12, 6));
        let jan = blatt(2027, 1);
        assert_eq!(jan[0], (2026, 12, 28));
        assert_eq!(blatt(2026, 6)[0], (2026, 6, 1));
    }

    #[test]
    fn schaltjahre() {
        assert_eq!(tage_im_monat(2024, 2), 29);
        assert_eq!(tage_im_monat(1900, 2), 28);
        assert_eq!(tage_im_monat(2000, 2), 29);
    }

    #[test]
    fn tage_als_text() {
        assert_eq!(tage_text(&[12]), "12.");
        assert_eq!(tage_text(&[20, 21, 22, 23]), "20.–23.");
        assert_eq!(tage_text(&[3, 10, 17, 24]), "3./10./17./24.");
        assert_eq!(tage_text(&[1, 2, 8, 9]), "1.–2./8.–9.");
    }
}
