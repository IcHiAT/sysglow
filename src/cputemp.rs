//! CPU-Temperatur über den PawnIO-Treiber (derselbe Weg wie LibreHardwareMonitor und FanControl).
//!
//! Windows gibt die Kerntemperatur nur mit Adminrechten heraus. Deshalb hat Sysglow zwei Teile:
//! - `dienst()` läuft erhöht (`Sysglow.exe --temp`), liest alle 2 s die Temperatur und schreibt sie in
//!   einen gemeinsamen Speicher. Gestartet wird er über eine Aufgabe der Aufgabenplanung mit höchsten
//!   Rechten, darum kommt nach dem einmaligen Einrichten keine Admin-Abfrage mehr.
//! - `Leser` in der normalen Anzeige legt den Speicher an, stößt die Aufgabe an und liest den Wert.
//!
//! Fehlt PawnIO, die Aufgabe oder ein passendes Modul, bleibt der Wert einfach leer.

use crate::wide;
use std::mem::zeroed;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::Memory::{
    CreateFileMappingW, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ, FILE_MAP_WRITE, MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READWRITE,
};
use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenMutexW, WaitForSingleObject, CREATE_NO_WINDOW, INFINITE};

const AUFGABE: &str = "Sysglow CPU-Temperatur";
const SPEICHER: &str = "Local\\Sysglow.CpuTemp";
const ANZEIGE_MUTEX: &str = "Local\\Sysglow.SingleInstance";
static INTEL_MSR: &[u8] = include_bytes!("../assets/pawnio/IntelMSR.bin");
static AMD_17: &[u8] = include_bytes!("../assets/pawnio/AMDFamily17.bin");

/// Inhalt des gemeinsamen Speichers: Temperatur in °C und ein Zähler, der bei jeder Messung steigt.
#[repr(C)]
struct Daten {
    temp: f32,
    zaehler: u32,
}

// ---------------------------------------------------------------- Anzeige-Seite

pub struct Leser {
    map: HANDLE,
    view: *mut Daten,
    letzter: u32,
    gleich: u32,
}

impl Leser {
    pub fn new() -> Leser {
        unsafe {
            let map = CreateFileMappingW(INVALID_HANDLE_VALUE, null(), PAGE_READWRITE, 0, size_of::<Daten>() as u32, wide(SPEICHER).as_ptr());
            let view = if map.is_null() { null_mut() } else { MapViewOfFile(map, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, 0).Value as *mut Daten };
            let l = Leser { map, view, letzter: 0, gleich: 99 };
            l.anstossen();
            l
        }
    }

    /// Startet den erhöhten Teil über die Aufgabenplanung. Gibt es die Aufgabe nicht, passiert nichts.
    pub fn anstossen(&self) {
        let _ = std::process::Command::new("schtasks.exe")
            .args(["/Run", "/TN", AUFGABE])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags_no_window()
            .spawn();
    }

    /// Einmal pro Anzeige-Takt aufrufen. Liefert die Temperatur, solange der Dienst frische Werte schreibt.
    pub fn wert(&mut self) -> Option<f32> {
        if self.view.is_null() {
            return None;
        }
        let d = unsafe { std::ptr::read_volatile(self.view) };
        if d.zaehler == 0 {
            return None;
        }
        if d.zaehler == self.letzter {
            self.gleich += 1;
        } else {
            self.letzter = d.zaehler;
            self.gleich = 0;
        }
        // Dienst schreibt alle 2 s; bleibt der Zähler länger als etwa 10 s stehen, ist er weg
        (self.gleich < 5).then_some(d.temp)
    }

    /// Liefert der Dienst gerade Werte?
    pub fn aktiv(&self) -> bool {
        self.gleich < 5 && self.letzter != 0
    }
}

impl Drop for Leser {
    fn drop(&mut self) {
        unsafe {
            if !self.view.is_null() {
                UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS { Value: self.view as _ });
            }
            if !self.map.is_null() {
                CloseHandle(self.map);
            }
        }
    }
}

trait OhneFenster {
    fn creation_flags_no_window(&mut self) -> &mut Self;
}
impl OhneFenster for std::process::Command {
    fn creation_flags_no_window(&mut self) -> &mut Self {
        use std::os::windows::process::CommandExt;
        self.creation_flags(CREATE_NO_WINDOW)
    }
}

/// Richtet die Aufgabe ein (`an`) oder entfernt sie. Fragt einmal nach Adminrechten.
/// Gibt true zurück, wenn der erhöhte Teil erfolgreich durchlief.
pub fn einrichten(an: bool) -> bool {
    unsafe {
        let exe = std::env::current_exe().unwrap_or_default();
        let datei = wide(&exe.to_string_lossy());
        let args = wide(if an { "--temp-einrichten" } else { "--temp-entfernen" });
        let verb = wide("runas");
        let mut info: SHELLEXECUTEINFOW = zeroed();
        info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = datei.as_ptr();
        info.lpParameters = args.as_ptr();
        if ShellExecuteExW(&mut info) == 0 || info.hProcess.is_null() {
            return false; // abgelehnt
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 1u32;
        GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
        code == 0
    }
}

/// Erhöht: Aufgabe anlegen und gleich starten. Rückgabe = Exit-Code.
pub fn aufgabe_anlegen() -> i32 {
    let exe = std::env::current_exe().unwrap_or_default();
    let benutzer = format!(
        "{}\\{}",
        std::env::var("USERDOMAIN").unwrap_or_default(),
        std::env::var("USERNAME").unwrap_or_default()
    );
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>Liest die CPU-Temperatur für Sysglow. Wird von Sysglow selbst gestartet und beendet sich mit ihm.</Description></RegistrationInfo>
  <Principals><Principal id="Author"><UserId>{}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>HighestAvailable</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <IdleSettings><StopOnIdleEnd>false</StopOnIdleEnd><RestartOnIdle>false</RestartOnIdle></IdleSettings>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author"><Exec><Command>{}</Command><Arguments>--temp</Arguments></Exec></Actions>
</Task>"#,
        esc(&benutzer),
        esc(&exe.to_string_lossy())
    );
    let pfad = std::env::temp_dir().join("sysglow-temp-aufgabe.xml");
    let mut bytes = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    if std::fs::write(&pfad, bytes).is_err() {
        return 2;
    }
    let ok = std::process::Command::new("schtasks.exe")
        .args(["/Create", "/F", "/TN", AUFGABE, "/XML"])
        .arg(&pfad)
        .creation_flags_no_window()
        .status()
        .is_ok_and(|s| s.success());
    let _ = std::fs::remove_file(&pfad);
    if !ok {
        return 3;
    }
    let _ = std::process::Command::new("schtasks.exe").args(["/Run", "/TN", AUFGABE]).creation_flags_no_window().status();
    0
}

/// Erhöht: Aufgabe beenden und löschen.
pub fn aufgabe_entfernen() -> i32 {
    let run = |a: &[&str]| std::process::Command::new("schtasks.exe").args(a).creation_flags_no_window().status().is_ok_and(|s| s.success());
    run(&["/End", "/TN", AUFGABE]);
    if run(&["/Delete", "/F", "/TN", AUFGABE]) { 0 } else { 3 }
}

// ---------------------------------------------------------------- erhöhter Dienst

type FnOpen = unsafe extern "system" fn(*mut HANDLE) -> i32;
type FnLoad = unsafe extern "system" fn(HANDLE, *const u8, usize) -> i32;
type FnExec = unsafe extern "system" fn(HANDLE, *const u8, *const u64, usize, *mut u64, usize, *mut usize) -> i32;
type FnClose = unsafe extern "system" fn(HANDLE) -> i32;

struct Pawn {
    h: HANDLE,
    exec: FnExec,
    close: FnClose,
}

impl Pawn {
    unsafe fn laden(modul: &[u8]) -> Option<Pawn> {
        let mut kandidaten = Vec::new();
        if let Ok(pf) = std::env::var("ProgramFiles") {
            kandidaten.push(format!("{pf}\\PawnIO\\PawnIOLib.dll"));
        }
        kandidaten.push("PawnIOLib.dll".into());
        let lib = kandidaten.iter().map(|p| LoadLibraryW(wide(p).as_ptr())).find(|l| !l.is_null())?;
        let f = |n: &str| GetProcAddress(lib, format!("{n}\0").as_ptr());
        let open: FnOpen = std::mem::transmute(f("pawnio_open")?);
        let load: FnLoad = std::mem::transmute(f("pawnio_load")?);
        let exec: FnExec = std::mem::transmute(f("pawnio_execute")?);
        let close: FnClose = std::mem::transmute(f("pawnio_close")?);
        let mut h: HANDLE = null_mut();
        if open(&mut h) < 0 || h.is_null() {
            return None;
        }
        if load(h, modul.as_ptr(), modul.len()) < 0 {
            close(h);
            return None;
        }
        Some(Pawn { h, exec, close })
    }

    unsafe fn rufen(&self, name: &str, ein: u64) -> Option<u64> {
        let n = format!("{name}\0");
        let mut aus = 0u64;
        let mut anzahl = 0usize;
        ((self.exec)(self.h, n.as_ptr(), &ein, 1, &mut aus, 1, &mut anzahl) >= 0 && anzahl == 1).then_some(aus)
    }
}

impl Drop for Pawn {
    fn drop(&mut self) {
        unsafe { (self.close)(self.h) };
    }
}

enum Cpu {
    /// Intel: TjMax aus MSR 0x1A2, Abstand dazu aus dem Paket-Thermostatus 0x1B1 (oder Kern 0x19C)
    Intel { tjmax: f32 },
    /// AMD Zen (Familie 17h und neuer): Tctl aus dem SMN-Register 0x59800
    Zen,
}

fn hersteller() -> Option<(bool, u32)> {
    #[cfg(target_arch = "x86_64")]
    {
        use std::arch::x86_64::__cpuid;
        let r = __cpuid(0);
        let mut v = [0u8; 12];
        v[0..4].copy_from_slice(&r.ebx.to_le_bytes());
        v[4..8].copy_from_slice(&r.edx.to_le_bytes());
        v[8..12].copy_from_slice(&r.ecx.to_le_bytes());
        let eax = __cpuid(1).eax;
        let mut familie = (eax >> 8) & 0xF;
        if familie == 0xF {
            familie += (eax >> 20) & 0xFF;
        }
        match &v {
            b"GenuineIntel" => Some((true, familie)),
            b"AuthenticAMD" | b"HygonGenuine" => Some((false, familie)),
            _ => None,
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    None
}

unsafe fn messen(p: &Pawn, cpu: &Cpu) -> Option<f32> {
    match cpu {
        Cpu::Intel { tjmax } => {
            let v = p.rufen("ioctl_read_msr", 0x1B1).filter(|v| v & (1 << 31) != 0).or_else(|| p.rufen("ioctl_read_msr", 0x19C).filter(|v| v & (1 << 31) != 0))?;
            Some(tjmax - ((v >> 16) & 0x7F) as f32)
        }
        Cpu::Zen => {
            let v = p.rufen("ioctl_read_smn", 0x0005_9800)?;
            let mut t = ((v >> 21) & 0x7FF) as f32 / 8.0;
            if v & (1 << 19) != 0 {
                t -= 49.0;
            }
            Some(t)
        }
    }
}

/// Öffnet PawnIO und das passende Modul. Für den Dienst und den Test.
unsafe fn verbinden() -> Option<(Pawn, Cpu)> {
    let (intel, familie) = hersteller()?;
    if intel {
        let p = Pawn::laden(INTEL_MSR)?;
        let tj = p.rufen("ioctl_read_msr", 0x1A2).map(|v| ((v >> 16) & 0xFF) as f32).filter(|t| *t > 0.0).unwrap_or(100.0);
        Some((p, Cpu::Intel { tjmax: tj }))
    } else if familie >= 0x17 {
        Some((Pawn::laden(AMD_17)?, Cpu::Zen))
    } else {
        None
    }
}

/// Erhöhter Teil: schreibt alle 2 s die Temperatur, solange die Anzeige läuft.
pub fn dienst() -> i32 {
    unsafe {
        let Some((p, cpu)) = verbinden() else { return 4 };
        let mut zaehler = 0u32;
        loop {
            // Läuft keine Anzeige mehr, beendet sich der Dienst sofort (gibt die exe für Updates frei)
            let m = OpenMutexW(0x0010_0000 /* SYNCHRONIZE */, 0, wide(ANZEIGE_MUTEX).as_ptr());
            if m.is_null() {
                return 0;
            } else {
                CloseHandle(m);
                let map = OpenFileMappingW(FILE_MAP_WRITE, 0, wide(SPEICHER).as_ptr());
                if !map.is_null() {
                    let view = MapViewOfFile(map, FILE_MAP_WRITE, 0, 0, 0);
                    if !view.Value.is_null() {
                        if let Some(t) = messen(&p, &cpu) {
                            zaehler = zaehler.wrapping_add(1).max(1);
                            std::ptr::write_volatile(view.Value as *mut Daten, Daten { temp: t, zaehler });
                        }
                        UnmapViewOfFile(view);
                    }
                    CloseHandle(map);
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    }
}

/// Für den Test: einmal messen (braucht Adminrechte).
#[allow(dead_code)]
pub fn einmal() -> Option<f32> {
    unsafe {
        let (p, cpu) = verbinden()?;
        messen(&p, &cpu)
    }
}
