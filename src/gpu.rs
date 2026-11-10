//! GPU: Auslastung, Grafikspeicher und Temperatur, herstellerneutral (AMD, NVIDIA, Intel).
//!
//! Quellen sind dieselben wie beim Task-Manager, ohne Treiber-SDK und ohne Adminrechte:
//! - Grafikkarte, Name, Speichergroesse, Takt und Temperatur ueber D3DKMT (gdi32.dll, WDDM).
//! - Auslastung und belegter Speicher ueber die Leistungsindikatoren "GPU Engine" und "GPU Adapter Memory".
//!
//! Fehlt etwas (alter Treiber, Remote-Desktop, Basic Render Driver), liefert `Gpu::new` None
//! und das Element bleibt unsichtbar. Die Temperatur fehlt bei vielen Intel-Grafikchips; dann bleibt sie einfach weg.

use std::collections::{HashMap, VecDeque};
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use windows_sys::Wdk::Graphics::Direct3D::*;
use windows_sys::Win32::System::Performance::*;

use crate::stats::{jetzt, push_hist};
use crate::wide;

/// Unter dieser Groesse eigenen Speichers gilt die Karte als integriert; dann zaehlt der geteilte Speicher.
const INTEGRIERT_BIS: u64 = 1 << 30;

pub struct Gpu {
    pub name: String,
    adapter: u32,
    /// "luid_0x00000000_0x0000d1f3" (klein), so steht die Karte in den Instanznamen der Leistungsindikatoren
    luid: String,
    query: isize,
    engine: isize,
    speicher: isize,
    integriert: bool,
    pub vram_total: u64,
    pub vram_used: u64,
    pub load: f32,
    /// Grad Celsius; None, wenn der Treiber keine Temperatur meldet
    pub temp: Option<f32>,
    /// Takt der 3D-Engine in GHz; None, wenn der Treiber keinen meldet
    pub ghz: Option<f32>,
    pub hist: VecDeque<f32>,
    pub zeit: VecDeque<u32>,
}

impl Drop for Gpu {
    fn drop(&mut self) {
        unsafe {
            PdhCloseQuery(self.query);
            D3DKMTCloseAdapter(&D3DKMT_CLOSEADAPTER { hAdapter: self.adapter });
        }
    }
}

unsafe fn abfrage<T>(adapter: u32, typ: KMTQUERYADAPTERINFOTYPE, wert: &mut T) -> bool {
    let mut q = D3DKMT_QUERYADAPTERINFO {
        hAdapter: adapter,
        Type: typ,
        pPrivateDriverData: wert as *mut T as *mut c_void,
        PrivateDriverDataSize: size_of::<T>() as u32,
    };
    D3DKMTQueryAdapterInfo(&mut q) >= 0
}

/// "AMD Radeon RX 7900 XTX" -> "RX 7900 XTX", "NVIDIA GeForce RTX 4070 Laptop GPU" -> "RTX 4070 Laptop",
/// "Intel(R) Iris(R) Xe Graphics" -> "Iris Xe", "AMD Radeon(TM) Graphics" -> "Radeon"
pub fn kurzname(s: &str) -> String {
    let s = s.replace("(R)", " ").replace("(TM)", " ").replace("(tm)", " ");
    let mut w: Vec<&str> = s
        .split_whitespace()
        .filter(|w| !["amd", "nvidia", "intel", "geforce", "gpu", "ati"].contains(&w.to_lowercase().as_str()))
        .collect();
    if w.len() > 1 {
        w.retain(|w| !w.eq_ignore_ascii_case("graphics"));
    }
    if w.len() > 1 && w[0].eq_ignore_ascii_case("radeon") {
        w.remove(0);
    }
    if w.is_empty() {
        "GPU".into()
    } else {
        w.join(" ")
    }
}

/// Eigener Speicher, geteilter Speicher und Name einer Karte.
unsafe fn eckdaten(adapter: u32) -> Option<(u64, u64, String)> {
    let mut seg: D3DKMT_SEGMENTSIZEINFO = zeroed();
    if !abfrage(adapter, KMTQAITYPE_GETSEGMENTSIZE, &mut seg) {
        return None;
    }
    let mut reg: D3DKMT_ADAPTERREGISTRYINFO = zeroed();
    let name = if abfrage(adapter, KMTQAITYPE_ADAPTERREGISTRYINFO, &mut reg) {
        let n = reg.AdapterString.iter().position(|&c| c == 0).unwrap_or(reg.AdapterString.len());
        String::from_utf16_lossy(&reg.AdapterString[..n])
    } else {
        String::new()
    };
    Some((seg.DedicatedVideoMemorySize, seg.SharedSystemMemorySize, name))
}

/// Temperatur in Grad Celsius (Treiber meldet Zehntelgrad, 0 = nicht unterstuetzt).
unsafe fn temperatur(adapter: u32) -> Option<f32> {
    let mut p: D3DKMT_ADAPTER_PERFDATA = zeroed();
    (abfrage(adapter, KMTQAITYPE_ADAPTERPERFDATA, &mut p) && p.Temperature > 0 && p.Temperature < 1500).then(|| p.Temperature as f32 / 10.0)
}

/// Takt in GHz von Knoten 0 (die 3D-Engine), 0 = nicht unterstuetzt.
unsafe fn takt(adapter: u32) -> Option<f32> {
    let mut p: D3DKMT_NODE_PERFDATA = zeroed();
    p.NodeOrdinal = 0;
    // Im Leerlauf meldet der Treiber auch 0 Hz; der Wert bleibt trotzdem stehen, damit die Spalte nicht springt.
    abfrage(adapter, KMTQAITYPE_NODEPERFDATA, &mut p).then(|| p.Frequency as f32 / 1e9)
}

/// Alle Werte eines Wildcard-Zaehlers als (Instanzname, Wert).
unsafe fn werte(zaehler: isize) -> Vec<(String, f64)> {
    let (mut groesse, mut anzahl) = (0u32, 0u32);
    PdhGetFormattedCounterArrayW(zaehler, PDH_FMT_DOUBLE, &mut groesse, &mut anzahl, null_mut());
    if groesse == 0 {
        return Vec::new();
    }
    // u64-Puffer, damit die Eintraege korrekt ausgerichtet sind
    let mut puffer = vec![0u64; (groesse as usize).div_ceil(8)];
    let p = puffer.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
    if PdhGetFormattedCounterArrayW(zaehler, PDH_FMT_DOUBLE, &mut groesse, &mut anzahl, p) != 0 {
        return Vec::new();
    }
    std::slice::from_raw_parts(p, anzahl as usize)
        .iter()
        .filter(|e| e.FmtValue.CStatus == 0 && !e.szName.is_null())
        .map(|e| {
            let n = (0..).take_while(|&i| *e.szName.add(i) != 0).count();
            let name = String::from_utf16_lossy(std::slice::from_raw_parts(e.szName, n)).to_lowercase();
            (name, e.FmtValue.Anonymous.doubleValue)
        })
        .collect()
}

impl Gpu {
    /// Sucht die Karte mit dem meisten eigenen Speicher (bei Laptops die dedizierte statt der integrierten).
    pub fn new() -> Option<Gpu> {
        unsafe {
            let mut e = D3DKMT_ENUMADAPTERS2 { NumAdapters: 0, pAdapters: null_mut() };
            if D3DKMTEnumAdapters2(&mut e) < 0 || e.NumAdapters == 0 {
                return None;
            }
            let mut liste: Vec<D3DKMT_ADAPTERINFO> = vec![zeroed(); e.NumAdapters as usize];
            e.pAdapters = liste.as_mut_ptr();
            if D3DKMTEnumAdapters2(&mut e) < 0 {
                return None;
            }
            liste.truncate(e.NumAdapters as usize);

            let mut beste: Option<(D3DKMT_ADAPTERINFO, u64, u64, String)> = None;
            for a in &liste {
                let Some((eigen, geteilt, name)) = eckdaten(a.hAdapter) else { continue };
                // Microsoft Basic Render Driver und reine Anzeige-Adapter auslassen
                if name.is_empty() || name.contains("Microsoft Basic") || eigen + geteilt == 0 {
                    continue;
                }
                if beste.as_ref().is_none_or(|b| eigen > b.1) {
                    beste = Some((*a, eigen, geteilt, name));
                }
            }
            for a in &liste {
                if beste.as_ref().is_none_or(|b| b.0.hAdapter != a.hAdapter) {
                    D3DKMTCloseAdapter(&D3DKMT_CLOSEADAPTER { hAdapter: a.hAdapter });
                }
            }
            let (a, eigen, geteilt, name) = beste?;
            let luid = format!("luid_0x{:08x}_0x{:08x}", a.AdapterLuid.HighPart as u32, a.AdapterLuid.LowPart);
            let integriert = eigen < INTEGRIERT_BIS;

            let mut q = 0isize;
            if PdhOpenQueryW(null(), 0, &mut q) != 0 {
                D3DKMTCloseAdapter(&D3DKMT_CLOSEADAPTER { hAdapter: a.hAdapter });
                return None;
            }
            let (mut engine, mut speicher) = (0isize, 0isize);
            let pfad_speicher = if integriert { "\\GPU Adapter Memory(*)\\Shared Usage" } else { "\\GPU Adapter Memory(*)\\Dedicated Usage" };
            if PdhAddEnglishCounterW(q, wide("\\GPU Engine(*)\\Utilization Percentage").as_ptr(), 0, &mut engine) != 0
                || PdhAddEnglishCounterW(q, wide(pfad_speicher).as_ptr(), 0, &mut speicher) != 0
            {
                PdhCloseQuery(q);
                D3DKMTCloseAdapter(&D3DKMT_CLOSEADAPTER { hAdapter: a.hAdapter });
                return None;
            }
            PdhCollectQueryData(q);
            Some(Gpu {
                name: kurzname(&name),
                adapter: a.hAdapter,
                luid,
                query: q,
                engine,
                speicher,
                integriert,
                vram_total: if integriert { geteilt } else { eigen },
                vram_used: 0,
                load: 0.0,
                temp: None,
                ghz: None,
                hist: VecDeque::new(),
                zeit: VecDeque::new(),
            })
        }
    }

    pub fn sample(&mut self) {
        unsafe {
            if PdhCollectQueryData(self.query) == 0 {
                // Wie der Task-Manager: je Engine die Prozesse aufsummieren, dann die meistbelastete Engine
                let mut je_engine: HashMap<String, f64> = HashMap::new();
                for (name, v) in werte(self.engine) {
                    if let Some(rest) = name.split_once(&self.luid).map(|x| x.1) {
                        let engine = rest.split("_engtype").next().unwrap_or(rest).to_string();
                        *je_engine.entry(engine).or_default() += v;
                    }
                }
                self.load = je_engine.values().fold(0.0f64, |m, &v| m.max(v)).clamp(0.0, 100.0) as f32;
                self.vram_used = werte(self.speicher).iter().filter(|(n, _)| n.contains(&self.luid)).map(|(_, v)| *v as u64).sum();
            }
            self.temp = temperatur(self.adapter);
            self.ghz = takt(self.adapter);
        }
        push_hist(&mut self.hist, self.load / 100.0);
        push_hist(&mut self.zeit, jetzt());
    }

    pub fn speicher_name(&self) -> &str {
        if self.integriert {
            "geteilt"
        } else {
            "VRAM"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::kurzname;

    #[test]
    fn namen() {
        assert_eq!(kurzname("AMD Radeon RX 7900 XTX"), "RX 7900 XTX");
        assert_eq!(kurzname("NVIDIA GeForce RTX 4070 Laptop GPU"), "RTX 4070 Laptop");
        assert_eq!(kurzname("Intel(R) Iris(R) Xe Graphics"), "Iris Xe");
        assert_eq!(kurzname("Intel(R) UHD Graphics 620"), "UHD 620");
        assert_eq!(kurzname("AMD Radeon(TM) Graphics"), "Radeon");
        assert_eq!(kurzname("NVIDIA RTX A2000"), "RTX A2000");
    }
}

#[cfg(test)]
mod messung {
    /// Zeigt, was auf diesem Rechner erkannt wird: cargo test erkennen -- --ignored --nocapture
    #[test]
    #[ignore]
    fn erkennen() {
        let Some(mut g) = super::Gpu::new() else {
            println!("keine GPU erkannt -> Element bleibt unsichtbar");
            return;
        };
        for _ in 0..3 {
            std::thread::sleep(std::time::Duration::from_secs(1));
            g.sample();
            println!("{} | {} | {:?} GHz | Last {:.1}% | {} {:.2}/{:.2} GB | Temp {:?}", g.name, g.luid, g.ghz, g.load, g.speicher_name(), g.vram_used as f64 / 1073741824.0, g.vram_total as f64 / 1073741824.0, g.temp);
        }
    }
}

