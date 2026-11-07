//! Zeichnen: Farben, Zeichenbefehle, Zusammensetzen des Bildes (vormultipliziertes BGRA) und Tray-Icon.

use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};

pub const COL_FG: u32 = 0x7CFFB2;
pub const COL_DIM: u32 = 0x5FA585;
pub const COL_CYAN: u32 = 0x40C8FF;
pub const COL_YELLOW: u32 = 0xFFCC4D;
pub const COL_RED: u32 = 0xFF5C5C;
pub const COL_GRAY: u32 = 0x808080;
const OUTLINE_ALPHA: f32 = 0.8;
pub const DOT_R: f32 = 3.0;

pub struct Surface {
    pub dc: HDC,
    pub bits: *mut u32,
}

/// Ein Stueck Text an fester Stelle.
pub struct TextOp {
    pub s: Vec<u16>,
    pub rc: RECT,
    pub col: u32,
}

/// Grafik-Befehle, die die Elemente erzeugen.
pub enum Draw {
    Fill(RECT, u32, f32),
    Bar(RECT, f32, u32),
    Spark(RECT, Vec<f32>, u32),
    Line(RECT, Vec<f32>, u32),
    Dot(f32, f32, u32),
}

pub fn rect(l: i32, t: i32, r: i32, b: i32) -> RECT {
    RECT { left: l, top: t, right: r, bottom: b }
}

pub unsafe fn new_surface(font: HFONT) -> Surface {
    let dc = CreateCompatibleDC(null_mut());
    if !font.is_null() {
        SelectObject(dc, font as HGDIOBJ);
        SetBkMode(dc, TRANSPARENT as _);
        SetTextColor(dc, 0x00FFFFFF);
    }
    Surface { dc, bits: null_mut() }
}

unsafe fn dib(dc: HDC, w: i32, h: i32) -> (HBITMAP, *mut u32) {
    let mut bi: BITMAPINFO = zeroed();
    bi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = w;
    bi.bmiHeader.biHeight = -h; // top-down
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    bi.bmiHeader.biCompression = BI_RGB as _;
    let mut bits: *mut c_void = null_mut();
    let bmp = CreateDIBSection(dc, &bi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    (bmp, bits as *mut u32)
}

pub unsafe fn resize_surface(s: &mut Surface, w: i32, h: i32) {
    let (bmp, bits) = dib(s.dc, w, h);
    let old = SelectObject(s.dc, bmp as HGDIOBJ);
    DeleteObject(old);
    s.bits = bits;
}

/// Alpha-Blending auf vormultipliziertes BGRA.
#[inline]
fn blend(d: &mut u32, rgb: u32, a: f32) {
    if a <= 0.0 {
        return;
    }
    let a = a.min(1.0);
    let inv = 1.0 - a;
    let ch = |sh: u32| -> u32 {
        let dc = ((*d >> sh) & 255) as f32;
        let sc = ((rgb >> sh) & 255) as f32 * a;
        ((sc + dc * inv + 0.5) as u32).min(255)
    };
    let da = ((*d >> 24) & 255) as f32;
    let oa = ((255.0 * a + da * inv + 0.5) as u32).min(255);
    *d = (oa << 24) | (ch(16) << 16) | (ch(8) << 8) | ch(0);
}

fn fill_rect(out: &mut [u32], w: i32, h: i32, r: &RECT, c: u32, a: f32) {
    for y in r.top.max(0)..r.bottom.min(h) {
        for x in r.left.max(0)..r.right.min(w) {
            blend(&mut out[(y * w + x) as usize], c, a);
        }
    }
}

fn seg_dist(px: f32, py: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let t = (((px - a.0) * dx + (py - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    ((px - a.0 - t * dx).powi(2) + (py - a.1 - t * dy).powi(2)).sqrt()
}

/// Setzt das Bild zusammen: Hintergrund, Grafik, Text mit dunkler Kontur.
pub unsafe fn compose(text_s: &Surface, out_s: &Surface, w: i32, h: i32, bg: u8, scale: f32, dr: &[Draw], ops: &[TextOp]) {
    let n = (w * h) as usize;
    let text = std::slice::from_raw_parts_mut(text_s.bits, n);
    let out = std::slice::from_raw_parts_mut(out_s.bits, n);

    // 1) Text als Graustufen-Maske per GDI (ClearType, Deckung = Mittel aus R, G, B)
    text.fill(0);
    for op in ops {
        let mut rc = op.rc;
        DrawTextW(text_s.dc, op.s.as_ptr(), op.s.len() as i32, &mut rc, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS);
    }
    GdiFlush();

    // 2) Hintergrund: mindestens Alpha 1, damit das Fenster Mausereignisse bekommt
    out.fill((bg.max(1) as u32) << 24);

    // 3) Grafik
    for d in dr {
        match d {
            Draw::Fill(r, c, a) => fill_rect(out, w, h, r, *c, *a),
            Draw::Bar(r, f, c) => {
                fill_rect(out, w, h, r, 0, 0.45);
                let seg = ((4.0 * scale).round() as i32).max(3);
                let on = seg - (scale.round() as i32).max(1);
                let fx = r.left + ((r.right - r.left) as f32 * f.clamp(0.0, 1.0)).round() as i32;
                for x in r.left.max(0)..r.right.min(w) {
                    if (x - r.left) % seg >= on {
                        continue;
                    }
                    let a = if x < fx { 0.95 } else { 0.15 };
                    for yy in r.top.max(0)..r.bottom.min(h) {
                        blend(&mut out[(yy * w + x) as usize], *c, a);
                    }
                }
            }
            Draw::Spark(r, v, c) | Draw::Line(r, v, c) => {
                fill_rect(out, w, h, r, 0, 0.35);
                let area = matches!(d, Draw::Spark(..));
                let gh = r.bottom - r.top;
                let len = v.len();
                let mut prev: Option<i32> = None;
                for x in r.left.max(0)..r.right.min(w) {
                    let back = (r.right - 1 - x) as usize;
                    if back >= len {
                        continue;
                    }
                    let val = v[len - 1 - back].clamp(0.0, 1.0);
                    let cy = r.bottom - 1 - (val * (gh - 1) as f32).round() as i32;
                    let (a, b) = if area { (cy, r.bottom - 1) } else { prev.map_or((cy, cy), |p| (p.min(cy), p.max(cy))) };
                    for yy in a.max(0)..=b.min(h - 1) {
                        let alpha = if !area || yy == cy { 1.0 } else { 0.35 };
                        blend(&mut out[(yy * w + x) as usize], *c, alpha);
                    }
                    prev = Some(cy);
                }
            }
            Draw::Dot(cx, cy, c) => {
                let r = DOT_R * scale;
                for py in (cy - r - 2.0) as i32..=(cy + r + 2.0) as i32 {
                    for px in (cx - r - 2.0) as i32..=(cx + r + 2.0) as i32 {
                        if px < 0 || py < 0 || px >= w || py >= h {
                            continue;
                        }
                        let dd = ((px as f32 + 0.5 - cx).powi(2) + (py as f32 + 0.5 - cy).powi(2)).sqrt() - r;
                        blend(&mut out[(py * w + px) as usize], *c, (0.5 - dd).clamp(0.0, 1.0));
                    }
                }
            }
        }
    }

    // 4) Text mit dunkler Kontur, nur im Bereich der Text-Auftraege
    let cov = |x: i32, y: i32| -> f32 {
        if x < 0 || y < 0 || x >= w || y >= h {
            0.0
        } else {
            let p = text[(y * w + x) as usize];
            ((p & 255) + ((p >> 8) & 255) + ((p >> 16) & 255)) as f32 / 765.0
        }
    };
    for op in ops {
        let r = op.rc;
        for y in (r.top - 1).max(0)..(r.bottom + 1).min(h) {
            for x in (r.left - 1).max(0)..(r.right + 1).min(w) {
                let mut o = 0.0f32;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        o = o.max(cov(x + dx, y + dy));
                    }
                }
                if o == 0.0 {
                    continue;
                }
                let p = &mut out[(y * w + x) as usize];
                blend(p, 0, o * OUTLINE_ALPHA);
                if x >= r.left && x < r.right && y >= r.top && y < r.bottom {
                    blend(p, op.col, cov(x, y));
                }
            }
        }
    }
}

/// Tray-Icon: dunkles Quadrat mit gruener Pulslinie.
pub unsafe fn make_icon(size: i32) -> HICON {
    let n = (size * size) as usize;
    let mut px = vec![0u32; n];
    let s = size as f32 / 32.0;
    let pts = [(3.0, 17.0), (10.0, 17.0), (13.0, 8.0), (18.0, 25.0), (21.0, 13.0), (24.0, 17.0), (29.0, 17.0)].map(|(x, y)| (x * s, y * s));
    let (hs, r) = (size as f32 / 2.0, 6.0 * s);
    let lw = (1.4 * s).max(1.0);
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let qx = ((fx - hs).abs() - (hs - r)).max(0.0);
            let qy = ((fy - hs).abs() - (hs - r)).max(0.0);
            let d = (qx * qx + qy * qy).sqrt() - r;
            let p = &mut px[(y * size + x) as usize];
            blend(p, 0x0B1016, (0.5 - d).clamp(0.0, 1.0));
            let dl = pts.windows(2).map(|w| seg_dist(fx, fy, w[0], w[1])).fold(f32::MAX, f32::min);
            blend(p, COL_FG, (lw + 0.5 - dl).clamp(0.0, 1.0));
        }
    }
    let (color, bits) = dib(null_mut(), size, size);
    if bits.is_null() {
        return null_mut();
    }
    std::ptr::copy_nonoverlapping(px.as_ptr(), bits, n);
    let mask_bits = vec![0u8; ((size as usize + 15) / 16 * 2) * size as usize];
    let mask = CreateBitmap(size, size, 1, 1, mask_bits.as_ptr() as *const c_void);
    let ii = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
    let icon = CreateIconIndirect(&ii);
    DeleteObject(color as HGDIOBJ);
    DeleteObject(mask as HGDIOBJ);
    icon
}
