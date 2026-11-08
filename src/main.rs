//! Sysglow (kurz: glow) – schlankes Desktop-Widget im Hacker-Stil:
//! Kopfzeile, Uhr, CPU, RAM, Platten, Netz, Energieplan und Ping-Zeilen. Jedes Element einzeln an/aus.
//! Schaltet beim Start eines Spiels auf Hoechstleistung und danach zurueck auf Ausbalanciert.
//! Konfiguration: sysglow.ini neben der exe (wird bei Aenderung automatisch neu geladen).
//!
//! Liegt nur auf dem Desktop (unter allen Fenstern) und blendet sich bei Vollbild/Spielen aus.
//! Maus: ziehen = verschieben, Hover = Details (auf Graphen: Uhrzeit und Wert), Klick auf Datum = Kalender, Klick auf Ping = Graph an/aus,
//! Doppelklick = Befehl / Task-Manager / Laufwerk oeffnen, Rechtsklick oder Tray-Icon = Menue.
//!
//! Aufbau: config.rs (ini), stats.rs (Systemwerte), ping.rs (ICMP), power.rs (Energieplan, Spiele), gfx.rs (Zeichnen),
//! app.rs (Zustand + Elemente), main.rs (Fenster, Menue, Tray).

#![windows_subsystem = "windows"]

mod app;
mod config;
mod gfx;
mod kalender;
mod ping;
mod power;
mod startmenue;
mod stats;

use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering::Relaxed};
use std::time::Duration;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::{ShellExecuteW, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use app::{with_app, App, Hit};
use config::{KNOWN_MODULES, THEMES};
use ping::WM_PING;

const FONT_NAME: &str = "Consolas";
const FONT_PT: f32 = 9.0;
const SMALL_FONT_PT: f32 = 7.5;

const WM_TRAY: u32 = WM_APP + 2;
const WM_MOUSELEAVE: u32 = 0x02A3;
const MENU_CONFIG: usize = 1;
const MENU_RELOAD: usize = 2;
const MENU_AUTOSTART: usize = 3;
const MENU_EXIT: usize = 4;
const MENU_TOGGLE: usize = 5;
const MENU_LOCK: usize = 6;
const MENU_RESETPOS: usize = 7;
const MENU_POWER_AUTO: usize = 8;
const MENU_STARTMENUE: usize = 9;
const MENU_COLOR: usize = 200; // + Index in THEMES
const MENU_MODULE: usize = 100; // + Index in KNOWN_MODULES
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "Sysglow";

static EXITING: AtomicBool = AtomicBool::new(false);
static TASKBAR_MSG: AtomicU32 = AtomicU32::new(0);
static TRAY_ICON: AtomicUsize = AtomicUsize::new(0);

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

// ---------------------------------------------------------------- Tray / Autostart / Befehle
unsafe fn tray(hwnd: HWND, msg: u32) {
    let mut nid: NOTIFYICONDATAW = zeroed();
    nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    nid.hIcon = TRAY_ICON.load(Relaxed) as HICON;
    for (d, c) in nid.szTip.iter_mut().zip("Sysglow".encode_utf16()) {
        *d = c;
    }
    Shell_NotifyIconW(msg, &nid);
}

fn autostart_enabled() -> bool {
    unsafe { RegGetValueW(HKEY_CURRENT_USER, wide(RUN_KEY).as_ptr(), wide(RUN_VALUE).as_ptr(), RRF_RT_REG_SZ, null_mut(), null_mut(), null_mut()) == ERROR_SUCCESS }
}

fn set_autostart(on: bool) {
    unsafe {
        if on {
            let exe = std::env::current_exe().unwrap_or_default();
            let v = wide(&format!("\"{}\"", exe.display()));
            RegSetKeyValueW(HKEY_CURRENT_USER, wide(RUN_KEY).as_ptr(), wide(RUN_VALUE).as_ptr(), REG_SZ, v.as_ptr() as *const c_void, (v.len() * 2) as u32);
        } else {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, wide(RUN_KEY).as_ptr(), wide(RUN_VALUE).as_ptr());
        }
    }
}

fn shell_open(cmd: &str) {
    let cmd = cmd.trim();
    if cmd.is_empty() {
        return;
    }
    let (file, args) = if let Some(rest) = cmd.strip_prefix('"') {
        rest.split_once('"').map_or((rest, ""), |(f, a)| (f, a.trim()))
    } else {
        cmd.split_once(' ').map_or((cmd, ""), |(f, a)| (f, a.trim()))
    };
    let (f, a) = (wide(file), wide(args));
    unsafe {
        ShellExecuteW(null_mut(), wide("open").as_ptr(), f.as_ptr(), if args.is_empty() { null() } else { a.as_ptr() }, null(), SW_SHOWNORMAL);
    }
}

// ---------------------------------------------------------------- Menue
unsafe fn show_menu(hwnd: HWND) {
    let Some((hidden, locked, modules, power_auto, color)) =
        with_app(|a| (a.user_hidden, a.settings.locked, a.settings.modules.clone(), a.settings.power_auto, a.settings.color.clone()))
    else {
        return;
    };
    let auto = autostart_enabled();
    let im_startmenue = startmenue::vorhanden("Sysglow");
    let check = |b: bool| if b { MF_CHECKED } else { 0 };
    let sub = CreatePopupMenu();
    for (i, m) in KNOWN_MODULES.iter().enumerate() {
        let label = match *m {
            "kopf" => "Kopfzeile",
            "uhr" => "Datum und Uhrzeit",
            "energie" => "Energieplan",
            "cpu" => "CPU",
            "ram" => "RAM",
            "platten" => "Platten",
            "netz" => "Netz",
            _ => "Ping",
        };
        AppendMenuW(sub, MF_STRING | check(modules.iter().any(|x| x == m)), MENU_MODULE + i, wide(label).as_ptr());
    }
    let colors = CreatePopupMenu();
    for (i, t) in THEMES.iter().enumerate() {
        let mut label = t.0.replace("ue", "ü").replace("ss", "ß");
        label[..1].make_ascii_uppercase();
        AppendMenuW(colors, MF_STRING | check(color == t.0), MENU_COLOR + i, wide(&label).as_ptr());
    }
    let menu = CreatePopupMenu();
    AppendMenuW(menu, MF_STRING, MENU_TOGGLE, wide(if hidden { "Einblenden" } else { "Ausblenden" }).as_ptr());
    AppendMenuW(menu, MF_POPUP, sub as usize, wide("Elemente").as_ptr());
    AppendMenuW(menu, MF_POPUP, colors as usize, wide("Farbe").as_ptr());
    AppendMenuW(menu, MF_STRING | check(power_auto), MENU_POWER_AUTO, wide("Energieplan bei Spielen umschalten").as_ptr());
    AppendMenuW(menu, MF_STRING | check(locked), MENU_LOCK, wide("Position sperren").as_ptr());
    AppendMenuW(menu, MF_STRING, MENU_RESETPOS, wide("Position zurücksetzen").as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    AppendMenuW(menu, MF_STRING, MENU_CONFIG, wide("Konfiguration bearbeiten").as_ptr());
    AppendMenuW(menu, MF_STRING, MENU_RELOAD, wide("Neu laden").as_ptr());
    AppendMenuW(menu, MF_STRING | check(auto), MENU_AUTOSTART, wide("Mit Windows starten").as_ptr());
    AppendMenuW(menu, MF_STRING | check(im_startmenue), MENU_STARTMENUE, wide("Im Startmenü anzeigen").as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    AppendMenuW(menu, MF_STRING, MENU_EXIT, wide("Beenden").as_ptr());
    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, null()) as usize;
    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(menu); // zerstoert die Untermenues mit
    match cmd {
        MENU_TOGGLE => {
            with_app(|a| a.set_user_hidden(!hidden));
        }
        MENU_LOCK => {
            with_app(|a| {
                a.settings.locked = !locked;
                a.set_key("gesperrt", if locked { "0" } else { "1" });
            });
        }
        MENU_RESETPOS => {
            with_app(|a| {
                a.settings.pos = None;
                a.set_key("pos_x", "");
                a.set_key("pos_y", "");
                a.render();
            });
        }
        MENU_CONFIG => {
            if let Some(p) = with_app(|a| a.cfg_path.clone()) {
                let p = wide(&p.to_string_lossy());
                ShellExecuteW(null_mut(), wide("open").as_ptr(), p.as_ptr(), null(), null(), SW_SHOWNORMAL);
            }
        }
        MENU_RELOAD => {
            with_app(|a| {
                a.load_config();
                a.render();
            });
        }
        MENU_AUTOSTART => set_autostart(!auto),
        MENU_STARTMENUE => startmenue::setzen("Sysglow", !im_startmenue),
        MENU_POWER_AUTO => {
            with_app(|a| a.set_power_auto(!power_auto));
        }
        MENU_EXIT => {
            EXITING.store(true, Relaxed);
            DestroyWindow(hwnd);
        }
        c if (MENU_COLOR..MENU_COLOR + THEMES.len()).contains(&c) => {
            with_app(|a| a.set_color(THEMES[c - MENU_COLOR].0));
        }
        c if (MENU_MODULE..MENU_MODULE + KNOWN_MODULES.len()).contains(&c) => {
            with_app(|a| a.toggle_module(KNOWN_MODULES[c - MENU_MODULE]));
        }
        _ => {}
    }
}

// ---------------------------------------------------------------- Fenster
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let mouse_y = (lp >> 16) as i16 as i32;
    let mouse_x = lp as i16 as i32;
    match msg {
        WM_TIMER => {
            with_app(|a| a.tick());
            0
        }
        WM_PING => {
            with_app(|a| a.on_ping());
            0
        }
        WM_DISPLAYCHANGE => {
            with_app(|a| a.render());
            0
        }
        // Immer ganz unten bleiben (ueber dem Desktop, unter allen anderen Fenstern)
        WM_WINDOWPOSCHANGING => {
            let p = &mut *(lp as *mut WINDOWPOS);
            p.hwndInsertAfter = HWND_BOTTOM;
            p.flags &= !SWP_NOZORDER;
            0
        }
        WM_LBUTTONDOWN => {
            with_app(|a| {
                if a.settings.locked {
                    return;
                }
                let (mut cur, mut rc) = (POINT { x: 0, y: 0 }, zeroed::<RECT>());
                GetCursorPos(&mut cur);
                GetWindowRect(hwnd, &mut rc);
                a.drag = Some((cur, POINT { x: rc.left, y: rc.top }));
                a.dragging = false;
                SetCapture(hwnd);
            });
            0
        }
        WM_MOUSEMOVE => {
            with_app(|a| {
                if let Some((c0, w0)) = a.drag {
                    let mut cur = POINT { x: 0, y: 0 };
                    GetCursorPos(&mut cur);
                    let (dx, dy) = (cur.x - c0.x, cur.y - c0.y);
                    if !a.dragging && dx.abs().max(dy.abs()) > a.px(4.0) {
                        a.dragging = true;
                    }
                    if a.dragging {
                        let p = (w0.x + dx, w0.y + dy);
                        a.settings.pos = Some(p);
                        SetWindowPos(hwnd, null_mut(), p.0, p.1, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
                        return;
                    }
                }
                if !a.tracking {
                    let mut t = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                    TrackMouseEvent(&mut t);
                    a.tracking = true;
                }
                a.mouse_at(Some((mouse_x, mouse_y)));
            });
            0
        }
        WM_MOUSELEAVE => {
            with_app(|a| {
                a.tracking = false;
                a.mouse_at(None);
            });
            0
        }
        WM_LBUTTONUP => {
            with_app(|a| {
                let dragged = a.drag.take().is_some() && a.dragging;
                a.dragging = false;
                ReleaseCapture();
                if dragged {
                    a.render(); // klemmt die Position in den sichtbaren Bereich
                    let mut rc = zeroed::<RECT>();
                    GetWindowRect(hwnd, &mut rc);
                    a.settings.pos = Some((rc.left, rc.top));
                    a.set_key("pos_x", &rc.left.to_string());
                    a.set_key("pos_y", &rc.top.to_string());
                } else if a.ignore_up {
                    a.ignore_up = false;
                } else {
                    match a.hit_at(mouse_y) {
                        Some(Hit::Ping(i)) => a.toggle_graph(i),
                        Some(Hit::Power) => a.toggle_plan(),
                        Some(Hit::Uhr) => {
                            let mut rc = zeroed::<RECT>();
                            GetWindowRect(hwnd, &mut rc);
                            let st = &a.settings;
                            kalender::umschalten(rc, a.font, a.scale, st.fg, st.dim, st.accent);
                        }
                        _ => {}
                    }
                }
            });
            0
        }
        WM_CAPTURECHANGED => {
            with_app(|a| {
                a.drag = None;
                a.dragging = false;
            });
            0
        }
        WM_LBUTTONDBLCLK => {
            let cmd = with_app(|a| {
                let c = match a.hit_at(mouse_y)? {
                    Hit::Uhr => {
                        a.ignore_up = true; // zweiter Klick soll den Kalender nicht gleich wieder schliessen
                        return None;
                    }
                    Hit::Ping(i) => {
                        let sc = a.hosts[i].cfg.shortcut.clone();
                        if sc.is_empty() {
                            return None;
                        }
                        a.toggle_graph(i); // ersten Klick des Doppelklicks rueckgaengig machen
                        sc
                    }
                    Hit::Disk(i) => format!("explorer.exe {}", a.stats.disks.get(i)?.root),
                    Hit::Cpu | Hit::Ram | Hit::Net => "taskmgr.exe".to_string(),
                    Hit::Power => {
                        a.toggle_plan(); // ersten Klick des Doppelklicks rueckgaengig machen
                        "control.exe powercfg.cpl".to_string()
                    }
                };
                a.ignore_up = true;
                Some(c)
            })
            .flatten();
            if let Some(c) = cmd {
                shell_open(&c);
            }
            0
        }
        WM_RBUTTONUP => {
            show_menu(hwnd);
            0
        }
        WM_TRAY => {
            match (lp as u32) & 0xFFFF {
                WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(hwnd),
                WM_LBUTTONUP => {
                    with_app(|a| a.set_user_hidden(!a.user_hidden));
                }
                _ => {}
            }
            0
        }
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_QUERYENDSESSION => {
            EXITING.store(true, Relaxed);
            1
        }
        WM_DESTROY => {
            tray(hwnd, NIM_DELETE);
            with_app(|a| a.hosts.iter().for_each(|h| h.stop()));
            // Fenster gehoert dem Desktop: startet Explorer neu, wird es mit zerstoert -> selbst neu starten
            if !EXITING.load(Relaxed) {
                if let Ok(exe) = std::env::current_exe() {
                    let _ = std::process::Command::new(exe).arg("--restart").spawn();
                }
            }
            PostQuitMessage(0);
            0
        }
        _ if msg != 0 && msg == TASKBAR_MSG.load(Relaxed) => {
            tray(hwnd, NIM_ADD); // Explorer neu gestartet -> Tray-Icon neu anlegen
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn main() {
    unsafe {
        if std::env::args().any(|a| a == "--restart") {
            std::thread::sleep(Duration::from_secs(3)); // Explorer Zeit zum Starten geben
        }
        // nur eine Instanz
        CreateMutexW(null(), 0, wide("Local\\Sysglow.SingleInstance").as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_SYSTEM_AWARE);
        let scale = GetDpiForSystem() as f32 / 96.0;

        let inst = GetModuleHandleW(null());
        let class = wide("SysglowWnd");
        let wc = WNDCLASSW {
            style: CS_DBLCLKS,
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
        // Besitzer = Desktop (Progman): so bleibt das Fenster ueber dem Hintergrund, auch bei "ganz unten"
        let progman = FindWindowW(wide("Progman").as_ptr(), null());
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class.as_ptr(),
            wide("Sysglow").as_ptr(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            progman,
            null_mut(),
            inst,
            null(),
        );

        let make_font = |pt: f32| CreateFontW(
            -((pt * scale * 96.0 / 72.0).round() as i32),
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET as _,
            OUT_DEFAULT_PRECIS as _,
            CLIP_DEFAULT_PRECIS as _,
            CLEARTYPE_QUALITY as _,
            0,
            wide(FONT_NAME).as_ptr(),
        );
        let font = make_font(FONT_PT);
        let small_font = make_font(SMALL_FONT_PT);

        let cfg_path = std::env::current_exe().map(|p| p.with_file_name("sysglow.ini")).unwrap_or_else(|_| "sysglow.ini".into());
        let mut app = App::new(hwnd, scale, font, small_font, cfg_path);
        app.load_config(); // startet auch den Mess-Timer
        app.stats.sample();
        app.update_visibility();
        app::install(app);

        TRAY_ICON.store(gfx::make_icon(GetSystemMetrics(SM_CXSMICON)) as usize, Relaxed);
        TASKBAR_MSG.store(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()), Relaxed);
        tray(hwnd, NIM_ADD);

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
