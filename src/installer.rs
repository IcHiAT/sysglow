//! Installationsfenster in der exe: schwarz, gruener Rand, Zahlenregen.
//! Erscheint nur, wenn die exe nicht installiert ist und keine ini neben ihr liegt.
//! Installiert pro Benutzer, ohne PowerShell und ohne Download. Die ini wird nie angefasst.

use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicIsize, AtomicPtr, Ordering::Relaxed};
use std::sync::Mutex;
use std::time::Duration;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::SystemInformation::GetTickCount;
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SetFocus, VK_ESCAPE, VK_RETURN};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::{config, gfx, startmenue, wide};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const NAME: &str = "Sysglow";
const WM_FERTIG: u32 = WM_APP + 1;

const BREITE: f32 = 340.0;
const HOEHE: f32 = 150.0;
const ROT: u32 = gfx::COL_RED;
const GRUEN: u32 = gfx::COL_FG;
const HOVER: u32 = 0x1E4B32;
const SCHWARZ: u32 = 0;
const REGEN_ZEICHEN: &[u8] = b"01ABCDEF";
const REGEN_STUFEN: [u32; 6] = [190, 130, 90, 60, 38, 22];

#[derive(Clone, Copy, PartialEq)]
enum Aktion {
    Install,
    Uninstall,
    Start,
    Exit,
    Schliessen,
}

struct Knopf {
    text: &'static str,
    x: f32,
    aktion: Aktion,
}

struct Fenster {
    scale: f32,
    ziel: PathBuf,
    kopf: Vec<f32>,
    tempo: Vec<f32>,
    zufall: u32,
    takt: u32,
    knoepfe: Vec<Knopf>,
    hover: Option<Aktion>,
    info: String,
    status: String,
    status_rot: bool,
    lauf_text: Option<&'static str>,
    fertig_text: &'static str,
    widget_starten: bool,
    f_titel: HFONT,
    f_text: HFONT,
    f_knopf: HFONT,
    f_regen: HFONT,
}

static ZUSTAND: AtomicPtr<Fenster> = AtomicPtr::new(null_mut());
static ERGEBNIS: Mutex<Option<Result<(), String>>> = Mutex::new(None);

// ---------------------------------------------------------------- Entscheidung beim Start
fn gleich(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
}

fn fenster_noetig(exe: &Path, ziel: Option<&Path>, in_programme: bool, ini_daneben: bool) -> bool {
    !(in_programme || ini_daneben || ziel.is_some_and(|z| gleich(exe, z)))
}

fn zielpfad() -> Option<PathBuf> {
    let basis = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(basis).join(r"Programs\Sysglow\Sysglow.exe"))
}

/// true, wenn beim Start das Fenster statt des Widgets erscheinen soll.
pub fn soll_fenster() -> bool {
    if std::env::args().any(|a| a == "--restart") {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else { return false };
    fenster_noetig(&exe, zielpfad().as_deref(), config::in_programme(&exe), exe.with_file_name("sysglow.ini").exists())
}

// ---------------------------------------------------------------- Installieren / Entfernen
unsafe fn prozesspfad(pid: u32) -> Option<(HANDLE, PathBuf)> {
    use std::os::windows::ffi::OsStringExt;
    let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, pid);
    if h.is_null() {
        return None;
    }
    let mut puffer = [0u16; 1024];
    let mut laenge = puffer.len() as u32;
    if QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, puffer.as_mut_ptr(), &mut laenge) == 0 {
        CloseHandle(h);
        return None;
    }
    Some((h, std::ffi::OsString::from_wide(&puffer[..laenge as usize]).into()))
}

/// Beendet nur Prozesse, die genau aus dem Zielpfad laufen.
fn stoppen(ziel: &Path) {
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut weiter = Process32FirstW(snap, &mut e) != 0;
        while weiter {
            if let Some((h, pfad)) = prozesspfad(e.th32ProcessID) {
                if gleich(&pfad, ziel) && TerminateProcess(h, 0) != 0 {
                    WaitForSingleObject(h, 3000);
                }
                CloseHandle(h);
            }
            weiter = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
    }
}

fn fehlertext(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => "Kein Zugriff auf den Zielordner.".into(),
        _ => "Datei konnte nicht geschrieben werden.".into(),
    }
}

/// Mehrere Versuche, weil Windows eine gerade beendete exe kurz gesperrt haelt. Bis zu 8 s, denn der
/// erhoehte Temperatur-Teil (`--temp`) bemerkt erst nach bis zu 2 s, dass die Anzeige beendet ist.
fn mit_wiederholung<T>(mut f: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    let mut rest = 40;
    loop {
        match f() {
            Err(_) if rest > 0 => {
                rest -= 1;
                std::thread::sleep(Duration::from_millis(200));
            }
            ergebnis => return ergebnis,
        }
    }
}

fn installieren(ziel: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| fehlertext(&e))?;
    stoppen(ziel);
    if let Some(ordner) = ziel.parent() {
        std::fs::create_dir_all(ordner).map_err(|e| fehlertext(&e))?;
    }
    mit_wiederholung(|| std::fs::copy(&exe, ziel)).map_err(|e| fehlertext(&e))?;
    // Der Nutzer hat diese Datei gerade selbst gestartet; die Kopie soll beim Autostart nicht erneut nachfragen.
    let mut zone = ziel.as_os_str().to_owned();
    zone.push(":Zone.Identifier");
    let _ = std::fs::remove_file(zone);
    startmenue::setzen_auf(NAME, ziel);
    std::process::Command::new(ziel)
        .current_dir(ziel.parent().unwrap_or(ziel))
        .spawn()
        .map_err(|_| "Kopiert, aber Start fehlgeschlagen.".to_string())?;
    Ok(())
}

fn autostart_ziel() -> Option<String> {
    let mut puffer = [0u16; 1024];
    let mut bytes = (puffer.len() * 2) as u32;
    let ok = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, wide(RUN_KEY).as_ptr(), wide(NAME).as_ptr(), RRF_RT_REG_SZ, null_mut(), puffer.as_mut_ptr().cast(), &mut bytes)
    };
    (ok == ERROR_SUCCESS).then(|| String::from_utf16_lossy(&puffer[..(bytes as usize / 2).saturating_sub(1)]))
}

fn enthaelt_pfad(befehl: &str, pfad: &Path) -> bool {
    befehl.to_lowercase().contains(&pfad.to_string_lossy().to_lowercase())
}

fn entfernen(ziel: &Path) -> Result<(), String> {
    stoppen(ziel);
    if autostart_ziel().is_some_and(|b| enthaelt_pfad(&b, ziel)) {
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, wide(RUN_KEY).as_ptr(), wide(NAME).as_ptr()) };
    }
    if startmenue::ziel_von(NAME).is_some_and(|p| gleich(&p, ziel)) {
        startmenue::setzen(NAME, false);
    }
    if ziel.exists() {
        mit_wiederholung(|| std::fs::remove_file(ziel)).map_err(|e| fehlertext(&e))?;
    }
    // Nur wenn leer: Eine ini im Ordner laesst ihn stehen.
    if let Some(ordner) = ziel.parent() {
        let _ = std::fs::remove_dir(ordner);
    }
    Ok(())
}

// ---------------------------------------------------------------- Zeichnen
fn bgr(rgb: u32) -> u32 {
    ((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF)
}

unsafe fn flaeche(dc: HDC, r: RECT, farbe: u32) {
    let pinsel = CreateSolidBrush(bgr(farbe));
    FillRect(dc, &r, pinsel);
    DeleteObject(pinsel as HGDIOBJ);
}

unsafe fn text(dc: HDC, font: HFONT, x: i32, y: i32, s: &str, farbe: u32) {
    let t: Vec<u16> = s.encode_utf16().collect();
    SelectObject(dc, font as HGDIOBJ);
    SetTextColor(dc, bgr(farbe));
    TextOutW(dc, x, y, t.as_ptr(), t.len() as i32);
}

impl Fenster {
    fn p(&self, wert: f32) -> i32 {
        (wert * self.scale).round() as i32
    }

    fn rect(&self, x: f32, y: f32, b: f32, h: f32) -> RECT {
        RECT { left: self.p(x), top: self.p(y), right: self.p(x + b), bottom: self.p(y + h) }
    }

    fn knopf_rect(&self, k: &Knopf) -> RECT {
        self.rect(k.x, 106.0, 100.0, 32.0)
    }

    fn schliessen_rect(&self) -> RECT {
        self.rect(312.0, 8.0, 22.0, 22.0)
    }

    fn treffer(&self, x: i32, y: i32) -> Option<Aktion> {
        if self.lauf_text.is_some() {
            return None;
        }
        let drin = |r: RECT| x >= r.left && x < r.right && y >= r.top && y < r.bottom;
        if drin(self.schliessen_rect()) {
            return Some(Aktion::Schliessen);
        }
        self.knoepfe.iter().find(|k| drin(self.knopf_rect(k))).map(|k| k.aktion)
    }

    fn zufall(&mut self) -> f32 {
        self.zufall ^= self.zufall << 13;
        self.zufall ^= self.zufall >> 17;
        self.zufall ^= self.zufall << 5;
        (self.zufall % 10000) as f32 / 10000.0
    }

    fn regen_schritt(&mut self) {
        let faktor = if self.lauf_text.is_some() { 2.5 } else { 1.0 };
        let zeilen = HOEHE / 13.0 + 6.0;
        for s in 0..self.kopf.len() {
            self.kopf[s] += self.tempo[s] * faktor;
            if self.kopf[s] > zeilen {
                self.kopf[s] = -12.0 * self.zufall();
                self.tempo[s] = 0.12 + self.zufall() * 0.28;
            }
        }
    }

    unsafe fn malen(&self, dc: HDC) {
        let (b, h) = (self.p(BREITE), self.p(HOEHE));
        flaeche(dc, RECT { left: 0, top: 0, right: b, bottom: h }, GRUEN);
        let rand = self.p(2.0).max(1);
        flaeche(dc, RECT { left: rand, top: rand, right: b - rand, bottom: h - rand }, SCHWARZ);

        SetBkMode(dc, TRANSPARENT as _);
        let zelle = self.p(13.0);
        for (s, kopf) in self.kopf.iter().enumerate() {
            let zeile = kopf.floor() as i32;
            for (n, stufe) in REGEN_STUFEN.iter().enumerate() {
                let z = zeile - n as i32;
                if z < 0 {
                    continue;
                }
                let zeichen = REGEN_ZEICHEN[(s * 7 + z as usize * 13) % REGEN_ZEICHEN.len()] as char;
                let farbe = ((stufe * 124 / 255) << 16) | (stufe << 8) | (stufe * 178 / 255);
                let px = rand + s as i32 * zelle;
                if px + zelle < b - rand && z * zelle + zelle < h - rand {
                    text(dc, self.f_regen, px, rand + z * zelle, &zeichen.to_string(), farbe);
                }
            }
        }

        // Texte mit schwarzem Grund, damit sie ueber dem Regen lesbar bleiben.
        SetBkMode(dc, OPAQUE as _);
        SetBkColor(dc, 0);
        text(dc, self.f_titel, self.p(16.0), self.p(12.0), "> SYSGLOW_", GRUEN);
        if !self.info.is_empty() {
            text(dc, self.f_text, self.p(16.0), self.p(44.0), &self.info, GRUEN);
        }
        let status = match self.lauf_text {
            Some(t) => format!("{t}{}", ".".repeat((self.takt / 5 % 4) as usize)),
            None => self.status.clone(),
        };
        if !status.is_empty() {
            text(dc, self.f_text, self.p(16.0), self.p(72.0), &status, if self.status_rot { ROT } else { GRUEN });
        }
        text(dc, self.f_text, self.p(318.0), self.p(10.0), "x", if self.hover == Some(Aktion::Schliessen) { 0xFFFFFF } else { GRUEN });

        SetBkMode(dc, TRANSPARENT as _);
        SelectObject(dc, self.f_knopf as HGDIOBJ);
        for k in &self.knoepfe {
            let mut r = self.knopf_rect(k);
            flaeche(dc, r, GRUEN);
            let innen = RECT { left: r.left + 1, top: r.top + 1, right: r.right - 1, bottom: r.bottom - 1 };
            flaeche(dc, innen, if self.hover == Some(k.aktion) { HOVER } else { SCHWARZ });
            let mut t: Vec<u16> = k.text.encode_utf16().collect();
            SetTextColor(dc, bgr(GRUEN));
            DrawTextW(dc, t.as_mut_ptr(), t.len() as i32, &mut r, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        }
    }

    unsafe fn start_lauf(&mut self, hwnd: HWND, aktion: Aktion) {
        let installiere = aktion == Aktion::Install;
        self.lauf_text = Some(if installiere { "> installiere" } else { "> entferne" });
        self.fertig_text = if installiere { "> fertig. laeuft." } else { "> entfernt." };
        self.status_rot = false;
        self.status.clear();
        self.hover = None;
        let ziel = self.ziel.clone();
        static HWND_WERT: AtomicIsize = AtomicIsize::new(0);
        HWND_WERT.store(hwnd as isize, Relaxed);
        std::thread::spawn(move || {
            let ergebnis = if installiere { installieren(&ziel) } else { entfernen(&ziel) };
            *ERGEBNIS.lock().unwrap() = Some(ergebnis);
            unsafe { PostMessageW(HWND_WERT.load(Relaxed) as HWND, WM_FERTIG, 0, 0) };
        });
    }

    unsafe fn ausfuehren(&mut self, hwnd: HWND, aktion: Aktion) {
        match aktion {
            Aktion::Install | Aktion::Uninstall => self.start_lauf(hwnd, aktion),
            Aktion::Start => {
                self.widget_starten = true;
                DestroyWindow(hwnd);
            }
            Aktion::Exit | Aktion::Schliessen => {
                DestroyWindow(hwnd);
            }
        }
    }

    fn lauf_ende(&mut self) {
        self.lauf_text = None;
        match ERGEBNIS.lock().unwrap().take() {
            Some(Ok(())) => {
                self.status = self.fertig_text.into();
                self.info.clear();
                self.knoepfe = vec![Knopf { text: "EXIT", x: 224.0, aktion: Aktion::Exit }];
            }
            Some(Err(meldung)) => {
                self.status = "> fehler".into();
                self.status_rot = true;
                self.info = meldung;
            }
            None => {}
        }
    }
}

unsafe extern "system" fn fenster_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let zeiger = ZUSTAND.load(Relaxed);
    if zeiger.is_null() {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    let f = &mut *zeiger;
    match msg {
        WM_NCHITTEST => {
            let mut pt = POINT { x: (lp & 0xFFFF) as i16 as i32, y: ((lp >> 16) & 0xFFFF) as i16 as i32 };
            ScreenToClient(hwnd, &mut pt);
            // Alles ausser den Knoepfen zieht das rahmenlose Fenster.
            if f.treffer(pt.x, pt.y).is_some() { HTCLIENT as isize } else { HTCAPTION as isize }
        }
        WM_SETCURSOR if f.hover.is_some() && (lp & 0xFFFF) as u32 == HTCLIENT => {
            SetCursor(LoadCursorW(null_mut(), IDC_HAND));
            1
        }
        WM_LBUTTONUP => {
            let (x, y) = ((lp & 0xFFFF) as i16 as i32, ((lp >> 16) & 0xFFFF) as i16 as i32);
            if let Some(aktion) = f.treffer(x, y) {
                f.ausfuehren(hwnd, aktion);
            }
            0
        }
        WM_KEYDOWN => {
            if f.lauf_text.is_none() {
                if wp == VK_ESCAPE as usize {
                    DestroyWindow(hwnd);
                } else if wp == VK_RETURN as usize {
                    if let Some(aktion) = f.knoepfe.last().map(|k| k.aktion) {
                        f.ausfuehren(hwnd, aktion);
                    }
                }
            }
            0
        }
        WM_TIMER => {
            f.takt = f.takt.wrapping_add(1);
            f.regen_schritt();
            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            let unter_maus = WindowFromPoint(pt) == hwnd;
            ScreenToClient(hwnd, &mut pt);
            f.hover = if unter_maus { f.treffer(pt.x, pt.y) } else { None };
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_FERTIG => {
            f.lauf_ende();
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            let (b, h) = (f.p(BREITE), f.p(HOEHE));
            let mem = CreateCompatibleDC(dc);
            let bild = CreateCompatibleBitmap(dc, b, h);
            let alt = SelectObject(mem, bild as HGDIOBJ);
            f.malen(mem);
            BitBlt(dc, 0, 0, b, h, mem, 0, 0, SRCCOPY);
            SelectObject(mem, alt);
            DeleteObject(bild as HGDIOBJ);
            DeleteDC(mem);
            EndPaint(hwnd, &ps);
            0
        }
        WM_DESTROY => {
            KillTimer(hwnd, 1);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe fn schrift(pt: f32, scale: f32, gewicht: i32) -> HFONT {
    CreateFontW(
        -((pt * scale * 96.0 / 72.0).round() as i32),
        0,
        0,
        0,
        gewicht,
        0,
        0,
        0,
        DEFAULT_CHARSET as _,
        OUT_DEFAULT_PRECIS as _,
        CLIP_DEFAULT_PRECIS as _,
        CLEARTYPE_QUALITY as _,
        (FIXED_PITCH | FF_MODERN) as _,
        wide("Consolas").as_ptr(),
    )
}

/// Zeigt das Fenster. true heisst: Danach normal als Widget weiterlaufen (START).
pub unsafe fn zeigen(scale: f32) -> bool {
    let Some(ziel) = zielpfad() else { return true };
    let installiert = ziel.exists();
    let knoepfe = if installiert {
        vec![Knopf { text: "UNINSTALL", x: 16.0, aktion: Aktion::Uninstall }, Knopf { text: "INSTALL", x: 224.0, aktion: Aktion::Install }]
    } else {
        vec![Knopf { text: "START", x: 16.0, aktion: Aktion::Start }, Knopf { text: "INSTALL", x: 224.0, aktion: Aktion::Install }]
    };
    let spalten = (BREITE / 13.0).ceil() as usize;
    let mut f = Box::new(Fenster {
        scale,
        ziel,
        kopf: vec![0.0; spalten],
        tempo: vec![0.0; spalten],
        zufall: GetTickCount() | 1,
        takt: 0,
        knoepfe,
        hover: None,
        info: if installiert { "Bereits installiert.".into() } else { "Systemanzeige fuer den Desktop.".into() },
        status: String::new(),
        status_rot: false,
        lauf_text: None,
        fertig_text: "",
        widget_starten: false,
        f_titel: schrift(13.0, scale, 700),
        f_text: schrift(10.0, scale, 400),
        f_knopf: schrift(10.0, scale, 700),
        f_regen: schrift(8.0, scale, 400),
    });
    for s in 0..spalten {
        f.kopf[s] = -20.0 + f.zufall() * 34.0;
        f.tempo[s] = 0.12 + f.zufall() * 0.28;
    }
    let (b, h) = (f.p(BREITE), f.p(HOEHE));
    ZUSTAND.store(&mut *f, Relaxed);

    let inst = GetModuleHandleW(null());
    let klasse = wide("SysglowSetup");
    let mut wc: WNDCLASSW = std::mem::zeroed();
    wc.lpfnWndProc = Some(fenster_proc);
    wc.hInstance = inst;
    wc.hIcon = gfx::make_icon(32);
    wc.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
    wc.lpszClassName = klasse.as_ptr();
    RegisterClassW(&wc);

    let x = (GetSystemMetrics(SM_CXSCREEN) - b) / 2;
    let y = (GetSystemMetrics(SM_CYSCREEN) - h) / 2;
    let hwnd = CreateWindowExW(WS_EX_TOPMOST, klasse.as_ptr(), wide("Sysglow Installer").as_ptr(), WS_POPUP | WS_VISIBLE, x, y, b, h, null_mut(), null_mut(), inst, null());
    if hwnd.is_null() {
        ZUSTAND.store(null_mut(), Relaxed);
        return true;
    }
    SetForegroundWindow(hwnd);
    SetFocus(hwnd);
    SetTimer(hwnd, 1, 60, None);

    let mut nachricht: MSG = std::mem::zeroed();
    while GetMessageW(&mut nachricht, null_mut(), 0, 0) > 0 {
        TranslateMessage(&nachricht);
        DispatchMessageW(&nachricht);
    }
    ZUSTAND.store(null_mut(), Relaxed);
    for font in [f.f_titel, f.f_text, f.f_knopf, f.f_regen] {
        DeleteObject(font as HGDIOBJ);
    }
    f.widget_starten
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZIEL: &str = r"C:\Users\x\AppData\Local\Programs\Sysglow\Sysglow.exe";

    #[test]
    fn installierte_exe_startet_das_widget() {
        let exe = Path::new(r"c:\users\X\appdata\local\programs\sysglow\SYSGLOW.EXE");
        assert!(!fenster_noetig(exe, Some(Path::new(ZIEL)), false, false));
    }

    #[test]
    fn download_zeigt_das_fenster() {
        let exe = Path::new(r"C:\Users\x\Downloads\Sysglow.exe");
        assert!(fenster_noetig(exe, Some(Path::new(ZIEL)), false, false));
    }

    #[test]
    fn portable_exe_mit_ini_startet_das_widget() {
        let exe = Path::new(r"D:\Tools\Sysglow.exe");
        assert!(!fenster_noetig(exe, Some(Path::new(ZIEL)), false, true));
    }

    #[test]
    fn programme_startet_das_widget() {
        let exe = Path::new(r"C:\Program Files\Sysglow\Sysglow.exe");
        assert!(!fenster_noetig(exe, Some(Path::new(ZIEL)), true, false));
    }

    #[test]
    fn autostart_nur_beim_eigenen_pfad() {
        let ziel = Path::new(ZIEL);
        assert!(enthaelt_pfad(&format!("\"{}\"", ZIEL.to_lowercase()), ziel));
        assert!(!enthaelt_pfad(r#""D:\Tools\Sysglow.exe""#, ziel));
    }
}
