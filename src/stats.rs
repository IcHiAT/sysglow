//! Systemwerte: CPU (Last + Takt), RAM, Platten, Netz, Laufzeit. Nur Windows-Abfragen, keine Treiber.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::NetworkManagement::IpHelper::*;
use windows_sys::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives};
use windows_sys::Win32::System::Performance::*;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::SystemInformation::{GetLocalTime, GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::GetSystemTimes;

use crate::wide;

pub const HIST_MAX: usize = 800;
const CPU_KEY: &str = "HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0";

pub struct Disk {
    pub root: String,
    pub total: u64,
    pub free: u64,
}

pub struct Stats {
    pub cpu_name: String,
    cpu_prev: Option<(u64, u64)>,
    pub cpu: f32,
    pub cpu_hist: VecDeque<f32>,
    /// Uhrzeit jeder Messung (Sekunden seit Mitternacht), parallel zu cpu_hist und den Netz-Verlaeufen
    pub zeit_hist: VecDeque<u32>,
    perf: Option<(isize, isize, isize)>,
    pub ghz: f32,
    pub mem_total: f64,
    pub mem_avail: f64,
    pub disks: Vec<Disk>,
    disk_at: Instant,
    net_prev: Option<(u64, u64, Instant)>,
    pub down: f64,
    pub up: f64,
    pub net_hist: VecDeque<f32>,
    pub up_hist: VecDeque<f32>,
    pub uptime: u64,
}

pub fn push_hist<T>(h: &mut VecDeque<T>, v: T) {
    h.push_back(v);
    while h.len() > HIST_MAX {
        h.pop_front();
    }
}

/// Aktuelle Ortszeit in Sekunden seit Mitternacht.
pub fn jetzt() -> u32 {
    let t = unsafe {
        let mut t = zeroed::<SYSTEMTIME>();
        GetLocalTime(&mut t);
        t
    };
    t.wHour as u32 * 3600 + t.wMinute as u32 * 60 + t.wSecond as u32
}

pub fn hms(s: u32) -> String {
    format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

fn ft(f: &FILETIME) -> u64 {
    ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
}

/// "Intel(R) Core(TM) i7-11700KF CPU @ 3.60GHz" -> "i7-11700KF"
fn short_cpu(s: &str) -> String {
    let s = s.replace("(R)", "").replace("(TM)", "").replace("(tm)", "");
    let s = s.split(" @ ").next().unwrap_or("");
    let mut out = Vec::new();
    for w in s.split_whitespace() {
        let lw = w.to_lowercase();
        if lw == "with" {
            break;
        }
        let ordinal = lw.ends_with("th") && lw.starts_with(|c: char| c.is_ascii_digit());
        if ["amd", "intel", "core", "cpu", "processor", "gen"].contains(&lw.as_str()) || lw.ends_with("-core") || ordinal {
            continue;
        }
        out.push(w);
    }
    out.join(" ")
}

fn cpu_name() -> String {
    let mut buf = [0u16; 128];
    let mut len = (buf.len() * 2) as u32;
    let ok = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            wide(CPU_KEY).as_ptr(),
            wide("ProcessorNameString").as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buf.as_mut_ptr() as *mut c_void,
            &mut len,
        )
    } == ERROR_SUCCESS;
    if !ok {
        return String::new();
    }
    short_cpu(&String::from_utf16_lossy(&buf[..(len as usize / 2).saturating_sub(1)]))
}

/// Leistungsindikatoren fuer den Takt: "Processor Frequency" * "% Processor Performance" / 100.
/// Beide bezogen auf _Total, damit es auch bei Hybrid-CPUs mit P- und E-Kernen stimmt.
fn open_perf() -> Option<(isize, isize, isize)> {
    unsafe {
        let mut q = 0isize;
        if PdhOpenQueryW(null(), 0, &mut q) != 0 {
            return None;
        }
        let (mut leistung, mut frequenz) = (0isize, 0isize);
        let pfad_leistung = wide("\\Processor Information(_Total)\\% Processor Performance");
        let pfad_frequenz = wide("\\Processor Information(_Total)\\Processor Frequency");
        if PdhAddEnglishCounterW(q, pfad_leistung.as_ptr(), 0, &mut leistung) != 0
            || PdhAddEnglishCounterW(q, pfad_frequenz.as_ptr(), 0, &mut frequenz) != 0
        {
            PdhCloseQuery(q);
            return None;
        }
        PdhCollectQueryData(q);
        Some((q, leistung, frequenz))
    }
}

fn read_disks() -> Vec<Disk> {
    let mut v = Vec::new();
    unsafe {
        let mask = GetLogicalDrives();
        for i in 0..26u8 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let root = format!("{}:\\", (b'A' + i) as char);
            let w = wide(&root);
            if GetDriveTypeW(w.as_ptr()) != 3 {
                continue; // nur feste Laufwerke (keine Netz-/Wechseldatentraeger, die haengen koennen)
            }
            let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
            if GetDiskFreeSpaceExW(w.as_ptr(), &mut avail, &mut total, &mut free) != 0 && total > 0 {
                v.push(Disk { root, total, free: avail });
            }
        }
    }
    v
}

/// Summe der Bytes ueber alle aktiven Hardware-Adapter (ohne Filter- und Loopback-Schnittstellen).
fn net_octets() -> Option<(u64, u64)> {
    unsafe {
        let mut t: *mut MIB_IF_TABLE2 = null_mut();
        if GetIfTable2(&mut t) != 0 || t.is_null() {
            return None;
        }
        let rows = std::slice::from_raw_parts((*t).Table.as_ptr(), (*t).NumEntries as usize);
        let (mut rx, mut tx) = (0u64, 0u64);
        for r in rows {
            let f = r.InterfaceAndOperStatusFlags._bitfield;
            if f & 1 != 0 && f & 2 == 0 && r.OperStatus == 1 && r.Type != 24 {
                rx += r.InOctets;
                tx += r.OutOctets;
            }
        }
        FreeMibTable(t as *const c_void);
        Some((rx, tx))
    }
}

impl Stats {
    pub fn new() -> Self {
        Stats {
            cpu_name: cpu_name(),
            cpu_prev: None,
            cpu: 0.0,
            cpu_hist: VecDeque::new(),
            zeit_hist: VecDeque::new(),
            perf: open_perf(),
            ghz: 0.0,
            mem_total: 0.0,
            mem_avail: 0.0,
            disks: read_disks(),
            disk_at: Instant::now(),
            net_prev: None,
            down: 0.0,
            up: 0.0,
            net_hist: VecDeque::new(),
            up_hist: VecDeque::new(),
            uptime: 0,
        }
    }

    pub fn sample(&mut self) {
        unsafe {
            let (mut i, mut k, mut u): (FILETIME, FILETIME, FILETIME) = (zeroed(), zeroed(), zeroed());
            if GetSystemTimes(&mut i, &mut k, &mut u) != 0 {
                let (idle, total) = (ft(&i), ft(&k) + ft(&u));
                if let Some((pi, pt)) = self.cpu_prev {
                    let dt = total.saturating_sub(pt);
                    if dt > 0 {
                        self.cpu = (1.0 - idle.saturating_sub(pi) as f32 / dt as f32).clamp(0.0, 1.0) * 100.0;
                    }
                }
                self.cpu_prev = Some((idle, total));
            }
            if let Some((q, leistung, frequenz)) = self.perf {
                let (mut l, mut f): (PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE) = (zeroed(), zeroed());
                if PdhCollectQueryData(q) == 0
                    && PdhGetFormattedCounterValue(leistung, PDH_FMT_DOUBLE, null_mut(), &mut l) == 0
                    && PdhGetFormattedCounterValue(frequenz, PDH_FMT_DOUBLE, null_mut(), &mut f) == 0
                {
                    self.ghz = (l.Anonymous.doubleValue * f.Anonymous.doubleValue / 100_000.0) as f32;
                }
            }
            let mut m: MEMORYSTATUSEX = zeroed();
            m.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
            if GlobalMemoryStatusEx(&mut m) != 0 {
                self.mem_total = m.ullTotalPhys as f64;
                self.mem_avail = m.ullAvailPhys as f64;
            }
            self.uptime = GetTickCount64() / 1000;
        }
        push_hist(&mut self.cpu_hist, self.cpu / 100.0);
        push_hist(&mut self.zeit_hist, jetzt());
        if self.disk_at.elapsed() >= Duration::from_secs(10) {
            self.disk_at = Instant::now();
            self.disks = read_disks();
        }
        if let Some((rx, tx)) = net_octets() {
            let now = Instant::now();
            if let Some((prx, ptx, t)) = self.net_prev {
                let dt = now.duration_since(t).as_secs_f64().max(0.001);
                self.down = rx.saturating_sub(prx) as f64 / dt;
                self.up = tx.saturating_sub(ptx) as f64 / dt;
            }
            self.net_prev = Some((rx, tx, now));
        }
        push_hist(&mut self.net_hist, self.down as f32);
        push_hist(&mut self.up_hist, self.up as f32);
    }
}

pub fn rate(b: f64) -> String {
    if b >= 1024.0 * 1024.0 {
        format!("{:.1} MB/s", b / 1048576.0)
    } else if b >= 1024.0 {
        format!("{:.0} KB/s", b / 1024.0)
    } else {
        format!("{:.0} B/s", b)
    }
}

pub fn gb(b: f64) -> f64 {
    b / 1073741824.0
}
