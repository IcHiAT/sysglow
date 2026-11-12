//! Schieber-Fenster fuer den Hintergrund: Farbton, Saettigung, Helligkeit, Deckkraft.
//! Das Widget zeigt jede Aenderung sofort; in die ini geschrieben wird beim Loslassen der Maus.

use crate::config::{hsv_rgb, prozent_alpha, rgb_hsv};
use crate::{wide, with_app};
use std::cell::RefCell;
use std::mem::zeroed;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const NAMEN: [&str; 4] = ["Farbton", "Sättigung", "Helligkeit", "Deckkraft"];
const MAX: [u32; 4] = [359, 100, 100, 100];

// Layout in 96-dpi-Punkten
const RAND: f32 = 16.0;
const ZEILE: f32 = 40.0;
const BALKEN_X: f32 = 100.0;
const BALKEN_B: f32 = 220.0;
const BALKEN_H: f32 = 14.0;
const WERT_B: f32 = 56.0;

const GRUND: u32 = 0x1C1F24; // Fensterhintergrund RRGGBB
const SCHRIFT: u32 = 0xD8DEE6;

struct Zustand {
    hwnd: HWND,
    werte: [u32; 4],
    zieht: Option<usize>,
    scale: f32,
    font: HFONT,
}

thread_local! {
    static Z: RefCell<Option<Zustand>> = const { RefCell::new(None) };
}

fn p(scale: f32, v: f32) -> i32 {
    (v * scale).round() as i32
}

/// RRGGBB -> COLORREF (0x00BBGGRR)
fn bgr(c: u32) -> u32 {
    ((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF)
}

fn mischen(a: u32, b: u32, t: f32) -> u32 {
    let k = |sh: u32| {
        let (x, y) = (((a >> sh) & 255) as f32, ((b >> sh) & 255) as f32);
        ((x + (y - x) * t).round() as u32) << sh
    };
    k(16) | k(8) | k(0)
}

/// Farbe an Stelle t (0..1) im Balken der Zeile i
fn balkenfarbe(i: usize, w: &[u32; 4], t: f32) -> u32 {
    match i {
        0 => hsv_rgb((t * 359.0) as u32, 100, 100),
        1 => hsv_rgb(w[0], (t * 100.0) as u32, 100),
        2 => hsv_rgb(w[0], w[1], (t * 100.0) as u32),
        _ => mischen(GRUND, hsv_rgb(w[0], w[1], w[2]), t),
    }
}

fn balken(scale: f32, i: usize) -> RECT {
    let y = RAND + i as f32 * ZEILE + (ZEILE - BALKEN_H) / 2.0 - 6.0;
    RECT { left: p(scale, BALKEN_X), top: p(scale, y), right: p(scale, BALKEN_X + BALKEN_B), bottom: p(scale, y + BALKEN_H) }
}

/// Zeile unter der Maus (ganze Zeilenhoehe, etwas Rand links/rechts)
fn zeile_bei(scale: f32, x: i32, y: i32) -> Option<usize> {
    (0..4).find(|&i| {
        let r = balken(scale, i);
        let oben = p(scale, RAND + i as f32 * ZEILE);
        x >= r.left - p(scale, 8.0) && x <= r.right + p(scale, 8.0) && y >= oben && y < oben + p(scale, ZEILE)
    })
}

fn wert_bei(scale: f32, i: usize, x: i32) -> u32 {
    let r = balken(scale, i);
    let t = ((x - r.left) as f32 / (r.right - r.left - 1).max(1) as f32).clamp(0.0, 1.0);
    (t * MAX[i] as f32).round() as u32
}

/// Werte ans Widget geben; speichern = auch in die ini schreiben
fn anwenden(w: [u32; 4], speichern: bool) {
    let farbe = hsv_rgb(w[0], w[1], w[2]);
    let alpha = prozent_alpha(w[3]);
    if speichern {
        with_app(|a| a.set_hintergrund(Some(farbe), Some(alpha)));
    } else {
        with_app(|a| {
            a.settings.bg = alpha;
            a.settings.bg_farbe = farbe;
            a.render();
        });
    }
}

/// Wert einer Zeile setzen. Wer die Farbe aendert, waehrend der Hintergrund durchsichtig ist, bekommt 60 % Deckkraft.
fn setzen(i: usize, v: u32, speichern: bool) {
    let Some((hwnd, w)) = Z.with(|z| {
        let mut z = z.borrow_mut();
        let z = z.as_mut()?;
        z.werte[i] = v.min(MAX[i]);
        if i < 3 && z.werte[3] == 0 {
            z.werte[3] = 60;
        }
        Some((z.hwnd, z.werte))
    }) else {
        return;
    };
    anwenden(w, speichern);
    unsafe { InvalidateRect(hwnd, null(), 0) };
}

unsafe fn malen(hwnd: HWND, z: &Zustand) {
    let mut ps: PAINTSTRUCT = zeroed();
    let dc = BeginPaint(hwnd, &mut ps);
    let mut rc: RECT = zeroed();
    GetClientRect(hwnd, &mut rc);
    let mdc = CreateCompatibleDC(dc);
    let bmp = CreateCompatibleBitmap(dc, rc.right, rc.bottom);
    let alt_bmp = SelectObject(mdc, bmp);
    let pinsel = GetStockObject(DC_BRUSH);
    let fuellen = |r: &RECT, c: u32| {
        SetDCBrushColor(mdc, bgr(c));
        FillRect(mdc, r, pinsel as HBRUSH);
    };
    fuellen(&rc, GRUND);
    let alt_font = SelectObject(mdc, z.font);
    SetBkMode(mdc, TRANSPARENT as i32);
    SetTextColor(mdc, bgr(SCHRIFT));
    let s = z.scale;
    for i in 0..4 {
        let b = balken(s, i);
        let mut tm: TEXTMETRICW = zeroed();
        GetTextMetricsW(mdc, &mut tm);
        let ty = (b.top + b.bottom) / 2 - tm.tmHeight / 2;
        let name = wide(NAMEN[i]);
        TextOutW(mdc, p(s, RAND), ty, name.as_ptr(), name.len() as i32 - 1);
        let breite = b.right - b.left;
        for x in 0..breite {
            let t = x as f32 / (breite - 1).max(1) as f32;
            fuellen(&RECT { left: b.left + x, top: b.top, right: b.left + x + 1, bottom: b.bottom }, balkenfarbe(i, &z.werte, t));
        }
        // Knopf
        let kx = b.left + (z.werte[i] as f32 / MAX[i] as f32 * (breite - 1) as f32).round() as i32;
        let (kb, ko) = (p(s, 4.0), p(s, 5.0));
        fuellen(&RECT { left: kx - kb, top: b.top - ko, right: kx + kb + 1, bottom: b.bottom + ko }, 0xFFFFFF);
        fuellen(&RECT { left: kx - kb + 2, top: b.top - ko + 2, right: kx + kb - 1, bottom: b.bottom + ko - 2 }, balkenfarbe(i, &z.werte, z.werte[i] as f32 / MAX[i] as f32));
        let wert = if i == 0 { format!("{}°", z.werte[i]) } else { format!("{} %", z.werte[i]) };
        let wert = wide(&wert);
        TextOutW(mdc, b.right + p(s, 14.0), ty, wert.as_ptr(), wert.len() as i32 - 1);
    }
    BitBlt(dc, 0, 0, rc.right, rc.bottom, mdc, 0, 0, SRCCOPY);
    SelectObject(mdc, alt_font);
    SelectObject(mdc, alt_bmp);
    DeleteObject(bmp);
    DeleteDC(mdc);
    EndPaint(hwnd, &ps);
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let (mx, my) = (lp as i16 as i32, (lp >> 16) as i16 as i32);
    let scale = Z.with(|z| z.borrow().as_ref().map(|z| z.scale)).unwrap_or(1.0);
    match msg {
        WM_PAINT => {
            Z.with(|z| {
                if let Some(z) = z.borrow().as_ref() {
                    malen(hwnd, z);
                }
            });
            0
        }
        WM_ERASEBKGND => 1,
        WM_LBUTTONDOWN => {
            if let Some(i) = zeile_bei(scale, mx, my) {
                Z.with(|z| z.borrow_mut().as_mut().map(|z| z.zieht = Some(i)));
                SetCapture(hwnd);
                setzen(i, wert_bei(scale, i, mx), false);
            }
            0
        }
        WM_MOUSEMOVE => {
            if let Some(i) = Z.with(|z| z.borrow().as_ref().and_then(|z| z.zieht)) {
                setzen(i, wert_bei(scale, i, mx), false);
            }
            0
        }
        WM_LBUTTONUP => {
            if let Some(i) = Z.with(|z| z.borrow_mut().as_mut().and_then(|z| z.zieht.take())) {
                ReleaseCapture();
                setzen(i, wert_bei(scale, i, mx), true);
            }
            0
        }
        WM_MOUSEWHEEL => {
            // Mausrad ueber einer Zeile: kleine Schritte
            let mut pt = POINT { x: mx, y: my };
            ScreenToClient(hwnd, &mut pt);
            if let Some(i) = zeile_bei(scale, pt.x, pt.y) {
                let hoch = ((wp >> 16) as i16) > 0;
                let schritt = if i == 0 { 5 } else { 1 };
                let alt = Z.with(|z| z.borrow().as_ref().map(|z| z.werte[i])).unwrap_or(0);
                let neu = if hoch { alt + schritt } else { alt.saturating_sub(schritt) };
                setzen(i, neu, true);
            }
            0
        }
        WM_KEYDOWN if wp == 0x1B => {
            DestroyWindow(hwnd);
            0
        }
        WM_NCDESTROY => {
            if let Some(z) = Z.with(|z| z.borrow_mut().take()) {
                DeleteObject(z.font);
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// Oeffnet das Fenster neben der Maus (oder holt ein offenes nach vorne).
pub unsafe fn oeffnen() {
    if let Some(h) = Z.with(|z| z.borrow().as_ref().map(|z| z.hwnd)) {
        if IsWindow(h) != 0 {
            SetForegroundWindow(h);
            return;
        }
        Z.with(|z| z.borrow_mut().take());
    }
    let Some((bg, farbe)) = with_app(|a| (a.settings.bg, a.settings.bg_farbe)) else {
        return;
    };
    let (h, s, v) = rgb_hsv(farbe);
    let werte = [h, s, v, (bg as u32 * 100 + 127) / 255];
    let scale = GetDpiForSystem() as f32 / 96.0;

    let inst = GetModuleHandleW(null());
    let klasse = wide("SysglowSchieber");
    let wc = WNDCLASSW {
        style: 0,
        lpfnWndProc: Some(proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: inst,
        hIcon: null_mut(),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hbrBackground: null_mut(),
        lpszMenuName: null(),
        lpszClassName: klasse.as_ptr(),
    };
    RegisterClassW(&wc); // beim zweiten Mal schlaegt es fehl, die Klasse gibt es dann schon

    let stil = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU;
    let ex = WS_EX_TOOLWINDOW | WS_EX_TOPMOST;
    let mut r = RECT { left: 0, top: 0, right: p(scale, BALKEN_X + BALKEN_B + WERT_B + RAND), bottom: p(scale, RAND + 4.0 * ZEILE) };
    AdjustWindowRectEx(&mut r, stil, 0, ex);
    let (b, hoehe) = (r.right - r.left, r.bottom - r.top);
    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    let x = (pt.x - b - p(scale, 10.0)).max(0);
    let y = (pt.y - hoehe / 2).max(0);
    let hwnd = CreateWindowExW(ex, klasse.as_ptr(), wide("Sysglow Hintergrund").as_ptr(), stil, x, y, b, hoehe, null_mut(), null_mut(), inst, null());
    if hwnd.is_null() {
        return;
    }
    let font = CreateFontW(
        -p(scale, 13.0),
        0,
        0,
        0,
        FW_NORMAL as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        CLEARTYPE_QUALITY as u32,
        DEFAULT_PITCH as u32,
        wide("Segoe UI").as_ptr(),
    );
    Z.with(|z| *z.borrow_mut() = Some(Zustand { hwnd, werte, zieht: None, scale, font }));
    ShowWindow(hwnd, SW_SHOW);
    SetForegroundWindow(hwnd);
}
