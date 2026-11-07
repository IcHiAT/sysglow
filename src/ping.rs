//! Ping: ein Hintergrund-Thread pro Host, schickt ICMP-Echos im eingestellten Intervall.

use std::ffi::c_void;
use std::net::{Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::ptr::null;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::NetworkManagement::IpHelper::*;
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

use crate::config::Settings;

/// Nachricht ans Fenster: neues Ping-Ergebnis da.
pub const WM_PING: u32 = WM_APP + 1;

#[derive(Clone, Copy, PartialEq)]
pub enum Status {
    Pending,
    Ok(u32),
    Timeout,
}

/// Letztes Ergebnis + laufende Nummer (zaehlt bei jedem Ping hoch).
pub type Shared = Arc<Mutex<(Status, u32)>>;

fn resolve(addr: &str) -> Option<u32> {
    let ip: Ipv4Addr = match addr.parse() {
        Ok(ip) => ip,
        Err(_) => (addr, 0).to_socket_addrs().ok()?.find_map(|s| match s {
            SocketAddr::V4(v) => Some(*v.ip()),
            _ => None,
        })?,
    };
    Some(u32::from_ne_bytes(ip.octets()))
}

pub fn spawn_pinger(addr: String, s: &Settings, status: Shared, stop: Arc<AtomicBool>, hwnd: usize) {
    let (interval, timeout) = (Duration::from_secs(s.interval), s.timeout);
    std::thread::spawn(move || unsafe {
        let icmp = IcmpCreateFile();
        if icmp == INVALID_HANDLE_VALUE {
            return;
        }
        let is_ip = addr.parse::<Ipv4Addr>().is_ok();
        let payload = [0x61u8; 32];
        // u64-Puffer, damit ICMP_ECHO_REPLY korrekt ausgerichtet ist
        let mut reply = vec![0u64; 64];
        let mut ip: Option<u32> = None;
        while !stop.load(Relaxed) {
            let t0 = Instant::now();
            if ip.is_none() {
                ip = resolve(&addr);
            }
            let st = match ip {
                None => Status::Timeout,
                Some(a) => {
                    let n = IcmpSendEcho(
                        icmp,
                        a,
                        payload.as_ptr() as *const c_void,
                        payload.len() as u16,
                        null(),
                        reply.as_mut_ptr() as *mut c_void,
                        (reply.len() * 8) as u32,
                        timeout,
                    );
                    let r = &*(reply.as_ptr() as *const ICMP_ECHO_REPLY);
                    if n > 0 && r.Status == 0 {
                        Status::Ok(r.RoundTripTime)
                    } else {
                        if !is_ip {
                            ip = None; // Hostname beim naechsten Mal neu aufloesen
                        }
                        Status::Timeout
                    }
                }
            };
            if stop.load(Relaxed) {
                break;
            }
            {
                let mut g = status.lock().unwrap();
                g.0 = st;
                g.1 = g.1.wrapping_add(1);
            }
            PostMessageW(hwnd as HWND, WM_PING, 0, 0);
            while t0.elapsed() < interval && !stop.load(Relaxed) {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        IcmpCloseHandle(icmp);
    });
}
