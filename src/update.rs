//! Selbst-Update ueber GitHub-Releases.
//!
//! Ein Hintergrund-Thread fragt einmal am Tag `releases/latest` ab. Eine Pruefung, die faellig war, waehrend der PC aus war,
//! holt er kurz nach dem Start nach (Zeitpunkt steht als `update_geprueft` in der ini). Ist das Release neuer, laedt er die
//! exe als `Sysglow.new.exe` neben die laufende und prueft Groesse und SHA-256 gegen die Angaben von GitHub.
//! Installiert wird auf dem Hauptthread: laufende exe in `Sysglow.old.exe` umbenennen (geht bei laufenden Programmen),
//! neue an ihren Platz, neu starten mit `--nach-update`. Die neue exe raeumt die alte beim naechsten Start weg.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Security::Cryptography::{BCryptHash, BCRYPT_SHA256_ALG_HANDLE};
use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;

const API: &str = "https://api.github.com/repos/IcHiAT/sysglow/releases/latest";
pub const SEITE: &str = "https://github.com/IcHiAT/sysglow/releases/latest";
/// Abstand zwischen zwei Pruefungen
const ABSTAND: u64 = 24 * 3600;
/// Nach einem Fehler (offline, GitHub weg) frueher noch einmal versuchen
const NEUER_VERSUCH: u64 = 3600;
/// Wartezeit nach dem Start, damit das Netz nach dem Anmelden steht
const START_PAUSE: u64 = 20;
/// wParam = 1 bei manueller Pruefung, lParam = 1 wenn GitHub geantwortet hat
pub const WM_UPDATE: u32 = 0x8000 + 43; // WM_APP + 43
/// Hauptthread soll das bereitliegende Update automatisch installieren
pub const WM_UPDATE_INSTALLIEREN: u32 = 0x8000 + 44;

#[derive(Clone, PartialEq, Debug)]
pub enum Zustand {
    Unbekannt,
    Aktuell,
    /// neue Version liegt geprueft als `.new.exe` bereit
    Bereit(String),
    /// neue Version da, aber der Ordner der exe ist nicht beschreibbar (z. B. Program Files)
    NurSeite(String),
    Fehler(String),
}

pub static ZUSTAND: Mutex<Zustand> = Mutex::new(Zustand::Unbekannt);
static JETZT: AtomicBool = AtomicBool::new(false);
static FENSTER: AtomicUsize = AtomicUsize::new(0);
static ZULETZT: AtomicU64 = AtomicU64::new(0);
static GESTARTET: AtomicBool = AtomicBool::new(false);

pub fn zustand() -> Zustand {
    ZUSTAND.lock().unwrap().clone()
}

fn jetzt() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Startet den Pruef-Thread (einmal). `zuletzt` = letzte erfolgreiche Pruefung aus der ini.
pub fn starten(hwnd: usize, zuletzt: u64) {
    FENSTER.store(hwnd, Ordering::SeqCst);
    ZULETZT.store(zuletzt, Ordering::SeqCst);
    if !GESTARTET.swap(true, Ordering::SeqCst) {
        std::thread::spawn(schleife);
    }
}

/// Menue "Nach Update suchen": der Thread prueft innerhalb einer Sekunde.
pub fn jetzt_pruefen() {
    JETZT.store(true, Ordering::SeqCst);
}

fn exe_pfade() -> Option<(PathBuf, PathBuf, PathBuf)> {
    let exe = std::env::current_exe().ok()?;
    Some((exe.clone(), exe.with_extension("new.exe"), exe.with_extension("old.exe")))
}

fn schleife() {
    if let Some((_, neu, alt)) = exe_pfade() {
        let _ = std::fs::remove_file(alt);
        let _ = std::fs::remove_file(neu);
    }
    for _ in 0..START_PAUSE {
        if JETZT.load(Ordering::SeqCst) {
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    // verpasste Pruefung (PC war aus) wird sofort nachgeholt, weil der Zeitpunkt schon vorbei ist
    let mut naechste = ZULETZT.load(Ordering::SeqCst) + ABSTAND;
    loop {
        let manuell = JETZT.swap(false, Ordering::SeqCst);
        // liegt schon ein Update bereit, nur auf Wunsch neu pruefen
        let bereit = matches!(zustand(), Zustand::Bereit(_));
        if manuell || (!bereit && jetzt() >= naechste) {
            let z = pruefen();
            let ok = !matches!(z, Zustand::Fehler(_));
            naechste = jetzt() + if ok { ABSTAND } else { NEUER_VERSUCH };
            *ZUSTAND.lock().unwrap() = z;
            let h = FENSTER.load(Ordering::SeqCst);
            if h != 0 {
                unsafe { PostMessageW(h as _, WM_UPDATE, manuell as usize, ok as isize) };
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn pruefen() -> Zustand {
    let antwort = match crate::termine::https_bytes(API) {
        Ok(d) => String::from_utf8_lossy(&d).into_owned(),
        Err(e) => return Zustand::Fehler(e),
    };
    let Some(tag) = json_wert(&antwort, "tag_name", 0).map(|(v, _)| v) else {
        return Zustand::Fehler("Antwort von GitHub unklar".into());
    };
    if !neuer(&tag, env!("CARGO_PKG_VERSION")) {
        return Zustand::Aktuell;
    }
    let version = tag.trim_start_matches('v').to_string();
    let Some(datei) = exe_datei(&antwort) else { return Zustand::NurSeite(version) };
    let Some((_, neu, _)) = exe_pfade() else { return Zustand::NurSeite(version) };
    // Schreibrechte vor dem Download pruefen
    if std::fs::write(&neu, b"").is_err() {
        return Zustand::NurSeite(version);
    }
    let daten = match crate::termine::https_bytes(&datei.url) {
        Ok(d) => d,
        Err(e) => {
            let _ = std::fs::remove_file(&neu);
            return Zustand::Fehler(format!("Download: {e}"));
        }
    };
    if let Err(e) = download_pruefen(&daten, &datei) {
        let _ = std::fs::remove_file(&neu);
        return Zustand::Fehler(e);
    }
    if std::fs::write(&neu, &daten).is_err() {
        let _ = std::fs::remove_file(&neu);
        return Zustand::NurSeite(version);
    }
    Zustand::Bereit(version)
}

fn download_pruefen(daten: &[u8], datei: &Datei) -> Result<(), String> {
    if daten.len() as u64 != datei.groesse || !daten.starts_with(b"MZ") {
        return Err("Download unvollstaendig".into());
    }
    if let Some(soll) = &datei.sha256 {
        if !sha256_hex(daten).eq_ignore_ascii_case(soll) {
            return Err("Pruefsumme falsch".into());
        }
    }
    Ok(())
}

/// Tauscht die laufende exe gegen die bereitliegende und startet die neue. Danach muss sich der Aufrufer beenden.
pub fn installieren() -> Result<(), String> {
    let (exe, neu, alt) = exe_pfade().ok_or("exe-Pfad unbekannt")?;
    if !neu.exists() {
        return Err("kein Update bereit".into());
    }
    let _ = std::fs::remove_file(&alt);
    std::fs::rename(&exe, &alt).map_err(|e| format!("umbenennen: {e}"))?;
    if let Err(e) = std::fs::rename(&neu, &exe) {
        let _ = std::fs::rename(&alt, &exe);
        return Err(format!("ersetzen: {e}"));
    }
    if let Err(e) = std::process::Command::new(&exe).arg("--nach-update").spawn() {
        // zurueck auf den alten Stand
        let _ = std::fs::rename(&exe, &neu);
        let _ = std::fs::rename(&alt, &exe);
        return Err(format!("starten: {e}"));
    }
    Ok(())
}

// ---------------------------------------------------------------- Hilfen

/// "v0.9.2" neuer als "0.9.1"? Nicht lesbare Angaben gelten nie als neuer.
pub fn neuer(tag: &str, jetzt: &str) -> bool {
    let teile = |s: &str| -> Option<Vec<u32>> { s.trim().trim_start_matches(['v', 'V']).split('.').map(|t| t.parse().ok()).collect() };
    match (teile(tag), teile(jetzt)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// Wert zum Schluessel `"key"` ab Position `ab`: Text ohne Anfuehrungszeichen, Zahl oder null; dazu die Endposition.
/// Reicht fuer die flache GitHub-Antwort, ohne JSON-Bibliothek.
fn json_wert(s: &str, key: &str, ab: usize) -> Option<(String, usize)> {
    let muster = format!("\"{key}\"");
    let start = ab + s.get(ab..)?.find(&muster)? + muster.len();
    let rest = s[start..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    let pos = s.len() - rest.len();
    if let Some(r) = rest.strip_prefix('"') {
        let mut wert = String::new();
        let mut zeichen = r.char_indices();
        while let Some((i, c)) = zeichen.next() {
            match c {
                '"' => return Some((wert, pos + 1 + i + 1)),
                '\\' => {
                    if let Some((_, e)) = zeichen.next() {
                        wert.push(if e == 'n' { '\n' } else { e });
                    }
                }
                _ => wert.push(c),
            }
        }
        None
    } else {
        let ende = rest.find([',', '}', ']']).unwrap_or(rest.len());
        Some((rest[..ende].trim().to_string(), pos + ende))
    }
}

#[derive(Debug, PartialEq)]
struct Datei {
    url: String,
    groesse: u64,
    /// Hex, falls GitHub `digest: "sha256:..."` liefert
    sha256: Option<String>,
}

/// Erste .exe unter `assets`. GitHub listet je Datei name, …, size, digest, …, browser_download_url in dieser Reihenfolge.
fn exe_datei(s: &str) -> Option<Datei> {
    let mut ab = s.find("\"assets\"")?;
    loop {
        let (name, ende) = json_wert(s, "name", ab)?;
        ab = ende;
        if !name.to_ascii_lowercase().ends_with(".exe") {
            continue;
        }
        let (groesse, _) = json_wert(s, "size", ab)?;
        let (url, _) = json_wert(s, "browser_download_url", ab)?;
        let sha256 = json_wert(s, "digest", ab).and_then(|(d, e)| (e < s[ab..].find("browser_download_url")? + ab).then_some(d)).and_then(|d| d.strip_prefix("sha256:").map(str::to_string));
        return Some(Datei { url, groesse: groesse.parse().ok()?, sha256 });
    }
}

fn sha256_hex(daten: &[u8]) -> String {
    let mut aus = [0u8; 32];
    unsafe { BCryptHash(BCRYPT_SHA256_ALG_HANDLE, std::ptr::null(), 0, daten.as_ptr(), daten.len() as u32, aus.as_mut_ptr(), 32) };
    aus.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versionen_vergleichen() {
        assert!(neuer("v0.9.2", "0.9.1"));
        assert!(neuer("v0.10.0", "0.9.9"));
        assert!(!neuer("v0.9.1", "0.9.1"));
        assert!(!neuer("v0.9.0", "0.9.1"));
        assert!(!neuer("nightly", "0.9.1"));
    }

    #[test]
    fn release_antwort_lesen() {
        let s = r#"{"tag_name": "v0.9.2", "name":"Sysglow 0.9.2","assets":[
          {"name":"quellen.zip","size":5,"digest":null,"browser_download_url":"https://x/quellen.zip"},
          {"name":"Sysglow.exe","uploader":{"login":"IcHiAT"},"size":931328,
           "digest":"sha256:3be3854b9a0f0e9f53756d4c016ee94394939b593545d6467589e33a255af0cc",
           "browser_download_url":"https://github.com/IcHiAT/sysglow/releases/download/v0.9.2/Sysglow.exe"}],
          "body":"- \"name\":\"x.exe\""}"#;
        assert_eq!(json_wert(s, "tag_name", 0).unwrap().0, "v0.9.2");
        assert_eq!(
            exe_datei(s).unwrap(),
            Datei {
                url: "https://github.com/IcHiAT/sysglow/releases/download/v0.9.2/Sysglow.exe".into(),
                groesse: 931328,
                sha256: Some("3be3854b9a0f0e9f53756d4c016ee94394939b593545d6467589e33a255af0cc".into()),
            }
        );
        // ohne digest: nur Groesse pruefen, nicht den digest der naechsten Datei nehmen
        let ohne = r#"{"assets":[{"name":"Sysglow.exe","size":3,"browser_download_url":"u1"},{"name":"b","digest":"sha256:ff","browser_download_url":"u2"}]}"#;
        assert_eq!(exe_datei(ohne).unwrap().sha256, None);
    }

    #[test]
    fn sha256_stimmt() {
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn download_wird_geprueft() {
        let d = Datei { url: String::new(), groesse: 3, sha256: Some(sha256_hex(b"MZx")) };
        assert!(download_pruefen(b"MZx", &d).is_ok());
        assert!(download_pruefen(b"MZy", &d).is_err());
        assert!(download_pruefen(b"MZ", &d).is_err());
        assert!(download_pruefen(b"<ht", &Datei { sha256: None, ..d }).is_err());
    }
}
