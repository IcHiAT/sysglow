//! Verknüpfung im Startmenü des Benutzers anlegen und entfernen (Tray-Haken).
//! Der Haken zeigt, ob die Datei existiert; einen gespeicherten Zustand gibt es nicht.

use std::ffi::c_void;
use std::path::PathBuf;
use std::ptr::null_mut;
use windows_sys::core::GUID;
use windows_sys::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};

const CLSID_SHELL_LINK: GUID = GUID::from_u128(0x00021401_0000_0000_c000_000000000046);
const IID_ISHELL_LINK_W: GUID = GUID::from_u128(0x000214f9_0000_0000_c000_000000000046);
const IID_IPERSIST_FILE: GUID = GUID::from_u128(0x0000010b_0000_0000_c000_000000000046);

// Positionen in den COM-Funktionstabellen (windows-sys kennt diese Schnittstellen nicht)
const QUERY_INTERFACE: usize = 0;
const RELEASE: usize = 2;
const SET_WORKING_DIRECTORY: usize = 9;
const SET_ICON_LOCATION: usize = 17;
const SET_PATH: usize = 20;
const SAVE: usize = 6;

type Text = unsafe extern "system" fn(*mut c_void, *const u16) -> i32;
type Icon = unsafe extern "system" fn(*mut c_void, *const u16, i32) -> i32;
type Abfrage = unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> i32;
type Freigabe = unsafe extern "system" fn(*mut c_void) -> u32;
type Speichern = unsafe extern "system" fn(*mut c_void, *const u16, i32) -> i32;

fn w(s: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    s.encode_wide().chain(Some(0)).collect()
}

unsafe fn methode<T: Copy>(objekt: *mut c_void, index: usize) -> T {
    let tabelle = *(objekt as *const *const usize);
    std::mem::transmute_copy(&*tabelle.add(index))
}

fn pfad(name: &str) -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs").join(format!("{name}.lnk")))
}

pub fn vorhanden(name: &str) -> bool {
    pfad(name).is_some_and(|p| p.exists())
}

pub fn setzen(name: &str, an: bool) {
    let Some(lnk) = pfad(name) else { return };
    if !an {
        let _ = std::fs::remove_file(lnk);
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    if let Some(ordner) = lnk.parent() {
        let _ = std::fs::create_dir_all(ordner);
    }
    let ziel = w(exe.as_os_str());
    let ordner = w(exe.parent().unwrap_or(&exe).as_os_str());
    let datei = w(lnk.as_os_str());
    unsafe {
        let init = CoInitializeEx(null_mut(), COINIT_APARTMENTTHREADED as u32);
        let mut link: *mut c_void = null_mut();
        if CoCreateInstance(&CLSID_SHELL_LINK, null_mut(), CLSCTX_INPROC_SERVER, &IID_ISHELL_LINK_W, &mut link) >= 0 {
            methode::<Text>(link, SET_PATH)(link, ziel.as_ptr());
            methode::<Text>(link, SET_WORKING_DIRECTORY)(link, ordner.as_ptr());
            methode::<Icon>(link, SET_ICON_LOCATION)(link, ziel.as_ptr(), 0);
            let mut persist: *mut c_void = null_mut();
            if methode::<Abfrage>(link, QUERY_INTERFACE)(link, &IID_IPERSIST_FILE, &mut persist) >= 0 {
                methode::<Speichern>(persist, SAVE)(persist, datei.as_ptr(), 1);
                methode::<Freigabe>(persist, RELEASE)(persist);
            }
            methode::<Freigabe>(link, RELEASE)(link);
        }
        if init >= 0 {
            CoUninitialize();
        }
    }
}
