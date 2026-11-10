//! Kalender-Popup: oeffnet per Klick auf das Datum, oben links neben dem Widget, Rand in der Schemafarbe.
//! Das Fenster wird einmal angelegt und danach nur ein- und ausgeblendet.
//! Bedienung: < > oder Mausrad = Monat wechseln, Klick auf den Titel = heute, Esc oder Klick daneben = zu.

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

const MONATE: [&str; 12] = ["Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"];
const TAGE: [&str; 7] = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];

fn schaltjahr(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn tage_im_monat(y: i32, m: u32) -> u32 {
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

    fn masse(&self) -> (i32, i32) {
        (2 * self.px(PAD) + 8 * self.px(ZELLE_B), 2 * self.px(PAD) + self.px(KOPF_H) + 7 * self.px(ZELLE_H) + self.px(4.0))
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
        let w = self.masse().0;
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
        let (w, h) = self.masse();
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
                if *t == self.heute {
                    dr.push(Draw::Fill(rect(x + d, yz, x + zb - d, yz + zh), self.fg, 0.3));
                }
                self.mittig(&mut ops, &t.2.to_string(), x, x + zb, yz, yz + zh, col);
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
            compose(&self.text, &self.out, w, h, DECKKRAFT, self.scale, &dr, &ops);
            let size = SIZE { cx: w, cy: h };
            let src = POINT { x: 0, y: 0 };
            let bf = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
            UpdateLayeredWindow(self.hwnd, null_mut(), &self.pos, &size, self.out.dc, &src, 0, &bf, ULW_ALPHA);
        }
    }

    fn zu(&mut self) {
        if self.offen {
            self.offen = false;
            unsafe { ShowWindow(self.hwnd, SW_HIDE) };
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
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
    };
    KAL.with(|s| *s.borrow_mut() = Some(k));
}

/// Neue Grundschrift nach einem Zoom; der Kalender zeichnet mit der Schrift seiner Flaeche.
pub unsafe fn schrift(font: HFONT) {
    mit_kal(|k| {
        SelectObject(k.text.dc, font as HGDIOBJ);
    });
}

/// Klick auf das Datum: oeffnet den Kalender links neben dem Widget (`widget` = Fensterrechteck) oder schliesst ihn.
pub unsafe fn umschalten(widget: RECT, font: HFONT, scale: f32, fg: u32, dim: u32, accent: u32) {
    if KAL.with(|k| k.borrow().is_none()) {
        anlegen(font);
    }
    let hwnd = mit_kal(|k| {
        if k.offen {
            k.zu();
            return None;
        }
        (k.scale, k.fg, k.dim, k.accent) = (scale, fg, dim, accent);
        k.heute = heute();
        (k.jahr, k.monat, _) = k.heute;
        let (w, _) = k.masse();
        let abstand = k.px(ABSTAND);
        let links = GetSystemMetrics(SM_XVIRTUALSCREEN);
        // Links neben dem Widget; steht das Widget am linken Rand, stattdessen rechts daneben
        let x = if widget.left - abstand - w >= links { widget.left - abstand - w } else { widget.right + abstand };
        k.pos = POINT { x, y: widget.top };
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
}
