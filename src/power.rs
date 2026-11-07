//! Energieplan lesen/setzen und laufende Spiele erkennen (Prozessliste + Direct3D-Vollbild).

use std::mem::{size_of, zeroed};
use std::ptr::null_mut;

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
use windows_sys::Win32::System::Power::{PowerGetActiveScheme, PowerReadFriendlyName, PowerSetActiveScheme};

pub const PLAN_BALANCED: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";
pub const PLAN_HIGH: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";

/// "381b4222-f694-41f0-9685-ff5bb260df2e" -> GUID
pub fn parse_guid(s: &str) -> Option<GUID> {
    let h: String = s.trim().trim_matches(|c| c == '{' || c == '}').chars().filter(|c| *c != '-').collect();
    if h.len() != 32 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let b = |i: usize, n: usize| u64::from_str_radix(&h[i..i + n], 16).unwrap_or(0);
    let mut d4 = [0u8; 8];
    for (k, x) in d4.iter_mut().enumerate() {
        *x = b(16 + 2 * k, 2) as u8;
    }
    Some(GUID { data1: b(0, 8) as u32, data2: b(8, 4) as u16, data3: b(12, 4) as u16, data4: d4 })
}

pub fn guid_str(g: &GUID) -> String {
    let d = g.data4;
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        g.data1, g.data2, g.data3, d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7]
    )
}

/// Aktiver Plan als GUID-Text (klein geschrieben).
pub fn active_plan() -> Option<String> {
    unsafe {
        let mut p: *mut GUID = null_mut();
        if PowerGetActiveScheme(null_mut(), &mut p) != 0 || p.is_null() {
            return None;
        }
        let s = guid_str(&*p);
        LocalFree(p as _);
        Some(s)
    }
}

pub fn set_plan(guid: &str) -> bool {
    parse_guid(guid).is_some_and(|g| unsafe { PowerSetActiveScheme(null_mut(), &g) == 0 })
}

/// Anzeigename des Plans, wie in der Systemsteuerung.
pub fn plan_name(guid: &str) -> String {
    let Some(g) = parse_guid(guid) else { return guid.to_string() };
    unsafe {
        let mut buf = [0u16; 128];
        let mut len = (buf.len() * 2) as u32;
        if PowerReadFriendlyName(null_mut(), &g, null_mut(), null_mut(), buf.as_mut_ptr() as *mut u8, &mut len) == 0 {
            let n = (len as usize / 2).min(buf.len());
            let s = String::from_utf16_lossy(&buf[..n]).trim_end_matches('\0').to_string();
            if !s.is_empty() {
                return s;
            }
        }
    }
    guid[..8].to_string()
}

/// Erster laufender Prozess aus der Liste (Vergleich ohne ".exe", ohne Gross/Klein).
pub fn running_game(names: &[String]) -> Option<String> {
    if names.is_empty() {
        return None;
    }
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut e: PROCESSENTRY32W = zeroed();
        e.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut found = None;
        let mut ok = Process32FirstW(snap, &mut e) != 0;
        while ok {
            let n = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            let exe = String::from_utf16_lossy(&e.szExeFile[..n]).to_lowercase();
            let exe = exe.strip_suffix(".exe").unwrap_or(&exe);
            if names.iter().any(|g| g == exe) {
                found = Some(exe.to_string());
                break;
            }
            ok = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
        found
    }
}
