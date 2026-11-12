//! Todo-Liste: eine Textdatei neben der ini (`todo.txt`), eine Aufgabe je Zeile, `x ` vorne = erledigt.
//! Leere Zeilen und Zeilen mit `#` zaehlen nicht. Die Datei darf jeder bearbeiten (auch Claude);
//! Sysglow liest sie bei jeder Aenderung neu.
//!
//! Im Widget: Klick aufs Kaestchen hakt ab, Klick auf den Rest oeffnet einen kleinen Editor genau ueber der Box.
//! Der Editor waechst mit dem Text. Esc oder Klick daneben speichert und schliesst.

use std::cell::RefCell;
use std::mem::zeroed;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, SetFocus, VK_CONTROL, VK_ESCAPE};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::{wide, with_app};

#[derive(Clone, PartialEq, Debug)]
pub struct Aufgabe {
    pub text: String,
    pub erledigt: bool,
    /// Zeilennummer in der Datei (zum Abhaken)
    pub zeile: usize,
}

/// Datei: `todo_datei` aus der ini (absolut oder relativ zur ini), sonst `todo.txt` neben der ini.
pub fn pfad(ini: &Path, eigen: &str) -> PathBuf {
    let eigen = eigen.trim().trim_matches('"');
    if eigen.is_empty() {
        return ini.with_file_name("todo.txt");
    }
    let p = PathBuf::from(eigen);
    if p.is_absolute() {
        p
    } else {
        ini.with_file_name(p)
    }
}

const ERLEDIGT: [&str; 6] = ["- [x] ", "- [X] ", "[x] ", "[X] ", "x ", "X "];
const OFFEN: [&str; 3] = ["- [ ] ", "[ ] ", "- "];

/// (erledigt, Text ohne Kennzeichen); None fuer leere Zeilen und Kommentare
fn zerlegen(l: &str) -> Option<(bool, &str)> {
    let l = l.trim_start_matches('\u{feff}').trim();
    if l.is_empty() || l.starts_with('#') {
        return None;
    }
    if let Some(r) = ERLEDIGT.iter().find_map(|p| l.strip_prefix(p)) {
        return Some((true, r.trim()));
    }
    Some((false, OFFEN.iter().find_map(|p| l.strip_prefix(p)).unwrap_or(l).trim()))
}

pub fn lesen(text: &str) -> Vec<Aufgabe> {
    text.lines()
        .enumerate()
        .filter_map(|(i, l)| zerlegen(l).filter(|(_, t)| !t.is_empty()).map(|(e, t)| Aufgabe { text: t.to_string(), erledigt: e, zeile: i }))
        .collect()
}

/// Hakt die Aufgabe in Zeile `zeile` ab oder wieder auf; der Rest der Datei bleibt, wie er ist.
pub fn umschalten(text: &str, zeile: usize) -> String {
    let mut zeilen: Vec<String> = text.lines().map(String::from).collect();
    if let Some(l) = zeilen.get_mut(zeile) {
        if let Some((e, t)) = zerlegen(l) {
            *l = if e { t.to_string() } else { format!("x {t}") };
        }
    }
    zeilen.join("\r\n") + "\r\n"
}

/// Bricht Text an Wortgrenzen so um, dass jede Zeile hoechstens `maxw` breit ist (gemessen mit `breite`).
/// Ein einzelnes Wort, das allein zu breit ist, wird zeichenweise geteilt.
pub fn umbrechen(text: &str, maxw: i32, breite: impl Fn(&str) -> i32) -> Vec<String> {
    let mut zeilen = Vec::new();
    let mut akt = String::new();
    for wort in text.split_whitespace() {
        let probe = if akt.is_empty() { wort.to_string() } else { format!("{akt} {wort}") };
        if breite(&probe) <= maxw {
            akt = probe;
            continue;
        }
        if !akt.is_empty() {
            zeilen.push(std::mem::take(&mut akt));
        }
        // zu langes Wort zeichenweise teilen
        for c in wort.chars() {
            akt.push(c);
            if breite(&akt) > maxw && akt.chars().count() > 1 {
                let c = akt.pop().unwrap();
                zeilen.push(std::mem::replace(&mut akt, c.to_string()));
            }
        }
    }
    if !akt.is_empty() || zeilen.is_empty() {
        zeilen.push(akt);
    }
    zeilen
}

// ---------------------------------------------------------------- Editor

const EDIT_ID: usize = 1;
const EN_CHANGE: u32 = 0x0300;
const EM_GETLINECOUNT: u32 = 0x00BA;
const EM_SETSEL: u32 = 0x00B1;
const EM_SETMARGINS: u32 = 0x00D3;
const MIN_ZEILEN: i32 = 3;
/// Hintergrund des Editors (wie beim Hintergrund-Schieber)
const GRUND: u32 = 0x1C1F24;
const HINWEIS: &str = "Esc / Klick daneben = speichern";

struct Editor {
    hwnd: HWND,
    edit: HWND,
    font: HFONT,
    font_klein: HFONT,
    pinsel: HBRUSH,
    datei: PathBuf,
    vorher: String,
    zeile_h: i32,
    rand: i32,
    hinweis_h: i32,
    fg: u32,
    dim: u32,
    bg: u32,
}

thread_local! {
    static ED: RefCell<Option<Editor>> = const { RefCell::new(None) };
}

/// RRGGBB -> COLORREF (0x00BBGGRR)
fn bgr(c: u32) -> u32 {
    ((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF)
}

unsafe fn text_von(edit: HWND) -> String {
    let n = GetWindowTextLengthW(edit);
    let mut buf = vec![0u16; n as usize + 1];
    let n = GetWindowTextW(edit, buf.as_mut_ptr(), buf.len() as i32);
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Hoehe an die Zahl der (umgebrochenen) Zeilen anpassen.
unsafe fn anpassen(hwnd: HWND) {
    let Some((edit, zh, rand, hh)) = ED.with(|e| e.borrow().as_ref().map(|e| (e.edit, e.zeile_h, e.rand, e.hinweis_h))) else { return };
    let zeilen = (SendMessageW(edit, EM_GETLINECOUNT, 0, 0) as i32).max(MIN_ZEILEN);
    let mut rc: RECT = zeroed();
    GetWindowRect(hwnd, &mut rc);
    let b = rc.right - rc.left;
    let h = zeilen * zh + 2 * rand + hh;
    SetWindowPos(hwnd, null_mut(), 0, 0, b, h, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
    SetWindowPos(edit, null_mut(), rand, rand, b - 2 * rand, zeilen * zh, SWP_NOZORDER | SWP_NOACTIVATE);
    InvalidateRect(hwnd, null(), 1);
}

unsafe fn malen(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = zeroed();
    let dc = BeginPaint(hwnd, &mut ps);
    ED.with(|e| {
        let e = e.borrow();
        let Some(e) = e.as_ref() else { return };
        let mut rc: RECT = zeroed();
        GetClientRect(hwnd, &mut rc);
        let pinsel = GetStockObject(DC_BRUSH) as HBRUSH;
        // Rahmen in der Hauptfarbe, innen dunkel
        SetDCBrushColor(dc, bgr(e.fg));
        FillRect(dc, &rc, pinsel);
        let innen = RECT { left: 1, top: 1, right: rc.right - 1, bottom: rc.bottom - 1 };
        SetDCBrushColor(dc, bgr(e.bg));
        FillRect(dc, &innen, pinsel);
        let alt = SelectObject(dc, e.font_klein as HGDIOBJ);
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, bgr(e.dim));
        let mut r = RECT { left: e.rand, top: rc.bottom - e.hinweis_h - e.rand / 2, right: rc.right - e.rand, bottom: rc.bottom - e.rand / 2 };
        let s: Vec<u16> = HINWEIS.encode_utf16().collect();
        DrawTextW(dc, s.as_ptr(), s.len() as i32, &mut r, DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
        SelectObject(dc, alt);
    });
    EndPaint(hwnd, &ps);
}

/// Speichern (nur wenn sich etwas geaendert hat), Fenster schliessen, Widget neu laden.
unsafe fn schliessen(hwnd: HWND) {
    if let Some((edit, datei, vorher)) = ED.with(|e| e.borrow().as_ref().map(|e| (e.edit, e.datei.clone(), e.vorher.clone()))) {
        let neu = text_von(edit);
        if neu != vorher {
            let mut t = neu.replace("\r\n", "\n").trim_end().replace('\n', "\r\n");
            if !t.is_empty() {
                t.push_str("\r\n");
            }
            if let Some(ordner) = datei.parent() {
                let _ = std::fs::create_dir_all(ordner);
            }
            let _ = std::fs::write(&datei, t);
        }
    }
    DestroyWindow(hwnd);
    with_app(|a| {
        a.todo_laden();
        a.render();
    });
}

unsafe extern "system" fn edit_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    match msg {
        WM_KEYDOWN if wp == VK_ESCAPE as usize => {
            PostMessageW(GetParent(hwnd), WM_CLOSE, 0, 0);
            0
        }
        // Strg+A markiert alles (kann das mehrzeilige Eingabefeld nicht von selbst)
        WM_KEYDOWN if wp == b'A' as usize && GetKeyState(VK_CONTROL as i32) < 0 => {
            SendMessageW(hwnd, EM_SETSEL, 0, -1);
            0
        }
        // Esc und Strg+A sonst als Zeichen -> Warnton
        WM_CHAR if wp == 0x1B || wp == 0x01 => 0,
        _ => DefSubclassProc(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            malen(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        WM_COMMAND if (wp >> 16) as u32 == EN_CHANGE => {
            anpassen(hwnd);
            0
        }
        WM_CTLCOLOREDIT => {
            let farben = ED.with(|e| e.borrow().as_ref().map(|e| (e.fg, e.bg, e.pinsel)));
            if let Some((fg, bg, pinsel)) = farben {
                let dc = wp as HDC;
                SetTextColor(dc, bgr(fg));
                SetBkColor(dc, bgr(bg));
                return pinsel as LRESULT;
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        // Klick neben den Editor: speichern und zu
        WM_ACTIVATE if wp & 0xFFFF == WA_INACTIVE as usize => {
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
            0
        }
        WM_CLOSE => {
            schliessen(hwnd);
            0
        }
        WM_NCDESTROY => {
            if let Some(e) = ED.with(|e| e.borrow_mut().take()) {
                DeleteObject(e.font as HGDIOBJ);
                DeleteObject(e.font_klein as HGDIOBJ);
                DeleteObject(e.pinsel as HGDIOBJ);
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// Oeffnet den Editor ueber der Box (`r` in Bildschirmkoordinaten). Ein offener Editor kommt nur nach vorne.
pub unsafe fn bearbeiten(r: RECT, datei: PathBuf, scale: f32, fg: u32, dim: u32) {
    let bg = GRUND;
    if let Some(h) = ED.with(|e| e.borrow().as_ref().map(|e| e.hwnd)) {
        if IsWindow(h) != 0 {
            SetForegroundWindow(h);
            return;
        }
        ED.with(|e| e.borrow_mut().take());
    }
    let inst = GetModuleHandleW(null());
    let klasse = wide("SysglowTodo");
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

    let font = crate::schrift(scale);
    let font_klein = crate::schrift(scale * 0.8);
    let zeile_h = {
        let dc = GetDC(null_mut());
        let alt = SelectObject(dc, font as HGDIOBJ);
        let mut tm: TEXTMETRICW = zeroed();
        GetTextMetricsW(dc, &mut tm);
        SelectObject(dc, alt);
        ReleaseDC(null_mut(), dc);
        tm.tmHeight.max(1)
    };
    let rand = (5.0 * scale).round() as i32;
    let hinweis_h = (12.0 * scale).round() as i32;
    let vorher = std::fs::read_to_string(&datei).unwrap_or_default().replace("\r\n", "\n").replace('\n', "\r\n");
    let b = (r.right - r.left).max((160.0 * scale) as i32);
    let h = MIN_ZEILEN * zeile_h + 2 * rand + hinweis_h;

    let hwnd = CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_TOPMOST, klasse.as_ptr(), wide("Sysglow Todo").as_ptr(), WS_POPUP, r.left, r.top, b, h, null_mut(), null_mut(), inst, null());
    if hwnd.is_null() {
        DeleteObject(font as HGDIOBJ);
        DeleteObject(font_klein as HGDIOBJ);
        return;
    }
    let stil = WS_CHILD | WS_VISIBLE | ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_WANTRETURN as u32;
    let edit = CreateWindowExW(0, wide("EDIT").as_ptr(), null(), stil, rand, rand, b - 2 * rand, MIN_ZEILEN * zeile_h, hwnd, EDIT_ID as _, inst, null());
    SendMessageW(edit, WM_SETFONT, font as WPARAM, 0);
    SendMessageW(edit, EM_SETMARGINS, 3, 0); // links und rechts kein Extra-Rand
    let pinsel = CreateSolidBrush(bgr(bg));
    ED.with(|e| *e.borrow_mut() = Some(Editor { hwnd, edit, font, font_klein, pinsel, datei, vorher: vorher.clone(), zeile_h, rand, hinweis_h, fg, dim, bg }));
    let t = wide(&vorher);
    SetWindowTextW(edit, t.as_ptr());
    SetWindowSubclass(edit, Some(edit_proc), 1, 0);
    anpassen(hwnd);
    // Schreibmarke ans Ende
    SendMessageW(edit, EM_SETSEL, usize::MAX >> 1, (usize::MAX >> 1) as isize);
    ShowWindow(hwnd, SW_SHOW);
    SetForegroundWindow(hwnd);
    SetFocus(edit);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datei_lesen() {
        let t = "# Kommentar\r\nMilch kaufen\r\n\r\nx Steuer\r\n- [ ] Reifen\r\n- [x] Ping fixen\r\n";
        let a = lesen(t);
        assert_eq!(a.len(), 4);
        assert_eq!(a[0], Aufgabe { text: "Milch kaufen".into(), erledigt: false, zeile: 1 });
        assert!(a[1].erledigt && a[1].text == "Steuer" && a[1].zeile == 3);
        assert!(!a[2].erledigt && a[2].text == "Reifen");
        assert!(a[3].erledigt && a[3].text == "Ping fixen");
        // "xbox" ist kein erledigt-Kennzeichen
        assert!(!lesen("xbox kaufen")[0].erledigt);
    }

    #[test]
    fn abhaken() {
        let t = "# K\nMilch\nx Steuer\n";
        assert_eq!(umschalten(t, 1), "# K\r\nx Milch\r\nx Steuer\r\n");
        assert_eq!(umschalten(t, 2), "# K\r\nMilch\r\nSteuer\r\n");
        assert_eq!(umschalten(t, 0), "# K\r\nMilch\r\nx Steuer\r\n");
    }

    #[test]
    fn umbruch() {
        let b = |s: &str| s.chars().count() as i32; // 1 Zeichen = 1 Pixel
        assert_eq!(umbrechen("Milch kaufen und Brot", 12, b), vec!["Milch kaufen", "und Brot"]);
        assert_eq!(umbrechen("abcdefghij", 4, b), vec!["abcd", "efgh", "ij"]);
        assert_eq!(umbrechen("kurz", 50, b), vec!["kurz"]);
        assert_eq!(umbrechen("a  b", 50, b), vec!["a b"]);
    }

    #[test]
    fn pfade() {
        let ini = Path::new("C:\\a\\sysglow.ini");
        assert_eq!(pfad(ini, ""), PathBuf::from("C:\\a\\todo.txt"));
        assert_eq!(pfad(ini, "liste.txt"), PathBuf::from("C:\\a\\liste.txt"));
        assert_eq!(pfad(ini, "\"D:\\x\\t.txt\""), PathBuf::from("D:\\x\\t.txt"));
    }
}
