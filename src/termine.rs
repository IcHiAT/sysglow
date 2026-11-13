//! Termine aus ICS-Kalendern (z. B. Google "geheime Adresse im iCal-Format" oder eine .ics-Datei).
//! Nur lesen. Ein Hintergrund-Thread laedt alle aktiven Quellen beim Start, alle 30 Minuten
//! und nach jeder Aenderung der ini neu; bei einem Fehler nach einer Minute erneut.

use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use windows_sys::Win32::Networking::WinHttp::*;
use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::config::{Art, KalQuelle};
use crate::wide;

/// Datum als (Jahr, Monat, Tag)
pub type Datum = (i32, u32, u32);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Wdh {
    Keine,
    Taeglich,
    Woechentlich,
    Monatlich,
    Jaehrlich,
}

#[derive(Clone, Debug)]
pub struct Termin {
    pub start: Datum,
    /// erster Tag nach dem Termin
    pub ende: Datum,
    pub titel: String,
    pub art: Art,
    pub wdh: Wdh,
    pub intervall: i64,
    pub bis: Option<Datum>,
    pub anzahl: Option<i64>,
}

/// Fortlaufende Tagesnummer (Tage seit 1.3.0000), fuer Abstaende zwischen Daten.
pub fn tagnr((y, m, d): Datum) -> i64 {
    let (y, m) = if m <= 2 { (y as i64 - 1, m as i64 + 9) } else { (y as i64, m as i64 - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + d as i64 - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy
}

impl Termin {
    fn dauer(&self) -> i64 {
        (tagnr(self.ende) - tagnr(self.start)).max(1)
    }

    /// Liegt der Termin (oder eine seiner Wiederholungen) auf diesem Tag?
    pub fn an(&self, tag: Datum) -> bool {
        let t = tagnr(tag);
        let s = tagnr(self.start);
        let l = self.dauer();
        if t < s || self.bis.is_some_and(|b| t >= tagnr(b) + l) {
            return false;
        }
        let n = self.intervall.max(1);
        // k = Nummer der Wiederholung, die an diesem Tag laeuft (falls eine)
        let k = match self.wdh {
            Wdh::Keine => (t < s + l).then_some(0),
            Wdh::Taeglich | Wdh::Woechentlich => {
                let p = n * if self.wdh == Wdh::Taeglich { 1 } else { 7 };
                ((t - s) % p < l).then_some((t - s) / p)
            }
            Wdh::Monatlich | Wdh::Jaehrlich => {
                // Beginn in diesem oder einem der Vormonate/-jahre, der den Tag abdeckt
                let schritt = if self.wdh == Wdh::Monatlich { 1 } else { 12 };
                let monate = (tag.0 - self.start.0) as i64 * 12 + tag.1 as i64 - self.start.1 as i64;
                let mut treffer = None;
                for zurueck in 0..=1 {
                    let mm = (monate / schritt - zurueck) * schritt;
                    if mm < 0 || (mm / schritt) % n != 0 {
                        continue;
                    }
                    let gesamt = self.start.1 as i64 - 1 + mm;
                    let (y, m) = (self.start.0 + gesamt.div_euclid(12) as i32, gesamt.rem_euclid(12) as u32 + 1);
                    if self.start.2 > crate::kalender::tage_im_monat(y, m) {
                        continue; // 29.2. oder 31. gibt es in diesem Monat nicht
                    }
                    let b = tagnr((y, m, self.start.2));
                    if b <= t && t < b + l {
                        treffer = Some(mm / schritt / n);
                        break;
                    }
                }
                treffer
            }
        };
        k.is_some_and(|k| self.anzahl.is_none_or(|a| k < a))
    }
}

fn datum(v: &str) -> Option<(Datum, bool)> {
    let z: String = v.chars().take(8).collect();
    if z.len() < 8 || !z.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let d = (z[0..4].parse().ok()?, z[4..6].parse().ok()?, z[6..8].parse().ok()?);
    // "mit Uhrzeit nach Mitternacht": Endtag zaehlt mit
    let zeit = v.get(9..15).is_some_and(|t| t != "000000");
    Some((d, zeit))
}

fn naechster(d: Datum) -> Datum {
    if d.2 < crate::kalender::tage_im_monat(d.0, d.1) {
        (d.0, d.1, d.2 + 1)
    } else if d.1 < 12 {
        (d.0, d.1 + 1, 1)
    } else {
        (d.0 + 1, 1, 1)
    }
}

fn entschluesseln(s: &str) -> String {
    let mut o = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') | Some('N') => o.push(' '),
                Some(x) => o.push(x),
                None => {}
            }
        } else {
            o.push(c);
        }
    }
    o.trim().to_string()
}

/// Liest alle VEVENTs. Wiederholungen: FREQ, INTERVAL, UNTIL, COUNT (BYDAY u. a. werden ignoriert).
pub fn parse_ics(text: &str, art: Art) -> Vec<Termin> {
    // Folgezeilen (beginnen mit Leerzeichen oder Tab) wieder anhaengen
    let mut zeilen: Vec<String> = Vec::new();
    for l in text.lines() {
        if let Some(rest) = l.strip_prefix(' ').or_else(|| l.strip_prefix('\t')) {
            if let Some(z) = zeilen.last_mut() {
                z.push_str(rest);
                continue;
            }
        }
        zeilen.push(l.trim_start_matches('\u{feff}').to_string());
    }
    let mut v = Vec::new();
    let mut akt: Option<(Option<Datum>, Option<(Datum, bool)>, String, String, bool)> = None;
    for z in &zeilen {
        let Some((kopf, wert)) = z.split_once(':') else { continue };
        let name = kopf.split(';').next().unwrap_or("").to_ascii_uppercase();
        match (name.as_str(), &mut akt) {
            ("BEGIN", _) if wert.eq_ignore_ascii_case("VEVENT") => akt = Some((None, None, String::new(), String::new(), false)),
            ("END", Some(_)) if wert.eq_ignore_ascii_case("VEVENT") => {
                let (start, ende, titel, regel, abgesagt) = akt.take().unwrap();
                let Some(start) = start else { continue };
                if abgesagt {
                    continue;
                }
                let ende = match ende {
                    Some((e, true)) => naechster(e),
                    Some((e, false)) => e,
                    None => naechster(start),
                };
                let ende = if tagnr(ende) <= tagnr(start) { naechster(start) } else { ende };
                let mut t = Termin { start, ende, titel, art, wdh: Wdh::Keine, intervall: 1, bis: None, anzahl: None };
                for teil in regel.split(';') {
                    let Some((k, w)) = teil.split_once('=') else { continue };
                    match k.to_ascii_uppercase().as_str() {
                        "FREQ" => {
                            t.wdh = match w.to_ascii_uppercase().as_str() {
                                "DAILY" => Wdh::Taeglich,
                                "WEEKLY" => Wdh::Woechentlich,
                                "MONTHLY" => Wdh::Monatlich,
                                "YEARLY" => Wdh::Jaehrlich,
                                _ => Wdh::Keine,
                            }
                        }
                        "INTERVAL" => t.intervall = w.parse().unwrap_or(1),
                        "UNTIL" => t.bis = datum(w).map(|d| d.0),
                        "COUNT" => t.anzahl = w.parse().ok(),
                        _ => {}
                    }
                }
                v.push(t);
            }
            ("DTSTART", Some(a)) => a.0 = datum(wert).map(|d| d.0),
            ("DTEND", Some(a)) => a.1 = datum(wert),
            ("SUMMARY", Some(a)) => a.2 = entschluesseln(wert),
            ("RRULE", Some(a)) => a.3 = wert.to_string(),
            ("STATUS", Some(a)) => a.4 = wert.eq_ignore_ascii_case("CANCELLED"),
            _ => {}
        }
    }
    v
}

// ------------------------------------------------------------ Laden

/// Laedt eine https-Adresse als Text.
fn https_holen(url: &str) -> Result<String, String> {
    https_bytes(url).map(|d| String::from_utf8_lossy(&d).into_owned())
}

/// Laedt eine https-Adresse per WinHTTP (Proxy und Zertifikate wie Windows selbst, folgt Weiterleitungen).
pub fn https_bytes(url: &str) -> Result<Vec<u8>, String> {
    let url = match url.get(..9) {
        Some(p) if p.eq_ignore_ascii_case("webcal://") => format!("https://{}", &url[9..]),
        _ => url.to_string(),
    };
    let wurl = wide(&url);
    unsafe {
        let mut uc: URL_COMPONENTS = std::mem::zeroed();
        uc.dwStructSize = std::mem::size_of::<URL_COMPONENTS>() as u32;
        uc.dwHostNameLength = u32::MAX;
        uc.dwUrlPathLength = u32::MAX;
        uc.dwExtraInfoLength = u32::MAX;
        uc.dwSchemeLength = u32::MAX;
        if WinHttpCrackUrl(wurl.as_ptr(), 0, 0, &mut uc) == 0 {
            return Err("Adresse ungueltig".into());
        }
        let teil = |p: *mut u16, n: u32| -> Vec<u16> {
            let mut v: Vec<u16> = if p.is_null() { Vec::new() } else { std::slice::from_raw_parts(p, n as usize).to_vec() };
            v.push(0);
            v
        };
        let host = teil(uc.lpszHostName, uc.dwHostNameLength);
        let mut pfad = teil(uc.lpszUrlPath, uc.dwUrlPathLength + uc.dwExtraInfoLength);
        if pfad.len() == 1 {
            pfad = wide("/");
        }
        let sicher = uc.nScheme == WINHTTP_INTERNET_SCHEME_HTTPS;

        let sitzung = WinHttpOpen(wide("Sysglow").as_ptr(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, null(), null(), 0);
        if sitzung.is_null() {
            return Err("WinHTTP nicht verfuegbar".into());
        }
        WinHttpSetTimeouts(sitzung, 10_000, 10_000, 15_000, 30_000);
        let mut handles = vec![sitzung];
        let ergebnis = (|| {
            let verb = WinHttpConnect(sitzung, host.as_ptr(), uc.nPort, 0);
            if verb.is_null() {
                return Err("keine Verbindung".to_string());
            }
            handles.push(verb);
            let anf = WinHttpOpenRequest(verb, wide("GET").as_ptr(), pfad.as_ptr(), null(), null(), null(), if sicher { WINHTTP_FLAG_SECURE } else { 0 });
            if anf.is_null() {
                return Err("Anfrage fehlgeschlagen".into());
            }
            handles.push(anf);
            if WinHttpSendRequest(anf, null(), 0, null(), 0, 0, 0) == 0 || WinHttpReceiveResponse(anf, null_mut()) == 0 {
                return Err("offline oder Server nicht erreichbar".into());
            }
            let mut code: u32 = 0;
            let mut groesse = 4u32;
            WinHttpQueryHeaders(anf, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, null(), &mut code as *mut u32 as *mut _, &mut groesse, null_mut());
            if code != 200 {
                return Err(format!("HTTP {code}"));
            }
            let mut daten = Vec::new();
            let mut puffer = vec![0u8; 64 * 1024];
            loop {
                let mut n = 0u32;
                if WinHttpReadData(anf, puffer.as_mut_ptr() as *mut _, puffer.len() as u32, &mut n) == 0 {
                    return Err("Abbruch beim Lesen".into());
                }
                if n == 0 {
                    break;
                }
                daten.extend_from_slice(&puffer[..n as usize]);
                if daten.len() > 20 << 20 {
                    return Err("zu gross".into());
                }
            }
            Ok(daten)
        })();
        for h in handles.into_iter().rev() {
            WinHttpCloseHandle(h);
        }
        ergebnis
    }
}

fn quelle_laden(q: &KalQuelle) -> Result<Vec<Termin>, String> {
    let s = q.quelle.trim();
    let klein = s.to_ascii_lowercase();
    let text = if klein.starts_with("https://") || klein.starts_with("http://") || klein.starts_with("webcal://") {
        https_holen(s)?
    } else {
        let p = std::path::PathBuf::from(s);
        let p = if p.is_relative() { crate::config::ini_pfad().with_file_name(p) } else { p };
        std::fs::read_to_string(&p).map_err(|_| "Datei fehlt".to_string())?
    };
    if !text.contains("BEGIN:VCALENDAR") {
        return Err("kein Kalender (Adresse pruefen)".into());
    }
    Ok(parse_ics(&text, q.art))
}

pub struct Stand {
    pub termine: Vec<Termin>,
    /// "Name: Grund" je Quelle, die nicht geladen werden konnte
    pub fehler: Vec<String>,
    /// true, solange der erste Ladevorgang laeuft
    pub laedt: bool,
}

pub static STAND: Mutex<Stand> = Mutex::new(Stand { termine: Vec::new(), fehler: Vec::new(), laedt: false });
static QUELLEN: Mutex<Vec<KalQuelle>> = Mutex::new(Vec::new());
static GENERATION: AtomicU64 = AtomicU64::new(0);
/// Fenster, das nach dem Laden eine Nachricht bekommt (Kalender-Popup)
pub static MELDEN_AN: AtomicUsize = AtomicUsize::new(0);
pub const WM_TERMINE: u32 = 0x8000 + 41; // WM_APP + 41

/// Neue Quellen aus der ini; startet beim ersten Aufruf den Lade-Thread.
pub fn quellen_setzen(q: &[KalQuelle]) {
    let aktiv: Vec<KalQuelle> = q.iter().filter(|q| q.an).cloned().collect();
    {
        let mut alt = QUELLEN.lock().unwrap();
        if *alt == aktiv && GENERATION.load(Ordering::SeqCst) > 0 {
            return;
        }
        *alt = aktiv;
    }
    if GENERATION.fetch_add(1, Ordering::SeqCst) == 0 {
        std::thread::spawn(lader);
    }
}

/// Sofort neu laden (Menue im Kalender).
pub fn neu_laden() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
}

fn lader() {
    loop {
        let gen = GENERATION.load(Ordering::SeqCst);
        let quellen = QUELLEN.lock().unwrap().clone();
        STAND.lock().unwrap().laedt = true;
        melden();
        let (mut termine, mut fehler) = (Vec::new(), Vec::new());
        for q in &quellen {
            match quelle_laden(q) {
                Ok(t) => termine.extend(t),
                Err(e) => fehler.push(format!("{}: {e}", q.name)),
            }
        }
        let warten = if fehler.is_empty() { 30 * 60 } else { 60 };
        *STAND.lock().unwrap() = Stand { termine, fehler, laedt: false };
        melden();
        for _ in 0..warten {
            if GENERATION.load(Ordering::SeqCst) != gen {
                break;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}

fn melden() {
    let h = MELDEN_AN.load(Ordering::SeqCst);
    if h != 0 {
        unsafe { PostMessageW(h as _, WM_TERMINE, 0, 0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICS: &str = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART;VALUE=DATE:19790314\r\nDTEND;VALUE=DATE:19790315\r\nRRULE:FREQ=YEARLY\r\nSUMMARY:Geburtstag\r\n  Anna\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20260803\r\nDTEND;VALUE=DATE:20260815\r\nSUMMARY:Urlaub\\, Ostsee\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART:20261111T180000Z\r\nDTEND:20261111T190000Z\r\nRRULE:FREQ=WEEKLY;INTERVAL=2;COUNT=3\r\nSUMMARY:Training\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20261120\r\nSTATUS:CANCELLED\r\nSUMMARY:Abgesagt\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    #[test]
    fn liest_ereignisse() {
        let v = parse_ics(ICS, Art::Termin);
        assert_eq!(v.len(), 3);
        assert_eq!(v[0].titel, "Geburtstag Anna");
        assert_eq!(v[1].titel, "Urlaub, Ostsee");
        assert_eq!(v[2].ende, (2026, 11, 12));
    }

    #[test]
    fn wiederholungen() {
        let v = parse_ics(ICS, Art::Termin);
        let (gb, urlaub, training) = (&v[0], &v[1], &v[2]);
        assert!(gb.an((2026, 3, 14)) && !gb.an((2026, 3, 15)) && !gb.an((1978, 3, 14)));
        assert!(urlaub.an((2026, 8, 3)) && urlaub.an((2026, 8, 14)) && !urlaub.an((2026, 8, 15)));
        assert!(training.an((2026, 11, 11)) && !training.an((2026, 11, 18)) && training.an((2026, 11, 25)));
        assert!(training.an((2026, 12, 9)) && !training.an((2026, 12, 23)));
    }

    /// Echter Abruf ueber das Netz: cargo test -- --ignored
    #[test]
    #[ignore]
    fn laedt_oeffentlichen_kalender() {
        let q = KalQuelle { name: "Feiertage".into(), art: Art::Termin, an: true, quelle: "https://calendar.google.com/calendar/ical/de.german%23holiday%40group.v.calendar.google.com/public/basic.ics".into(), line: 0 };
        let v = quelle_laden(&q).unwrap();
        assert!(v.iter().any(|t| t.an((2026, 12, 25))), "{} Termine, kein 25.12.", v.len());
        let falsch = KalQuelle { quelle: "https://calendar.google.com/calendar/ical/gibtsnicht/private-0/basic.ics".into(), ..q };
        assert!(quelle_laden(&falsch).is_err());
    }

    #[test]
    fn schalttag_und_tagnummern() {
        let t = Termin { start: (2024, 2, 29), ende: (2024, 3, 1), titel: String::new(), art: Art::Geburtstag, wdh: Wdh::Jaehrlich, intervall: 1, bis: None, anzahl: None };
        assert!(t.an((2028, 2, 29)) && !t.an((2026, 3, 1)) && !t.an((2026, 2, 28)));
        assert_eq!(tagnr((2026, 3, 1)) - tagnr((2026, 2, 28)), 1);
        assert_eq!(tagnr((2027, 1, 1)) - tagnr((2026, 12, 31)), 1);
    }
}
