//! Thin wrappers over the Windows shell APIs the installer needs (all per-user, no elevation).

use std::path::{Path, PathBuf};
use windows::core::{Interface, GUID, HSTRING};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{IShellLinkW, SHChangeNotify, SHGetKnownFolderPath, ShellLink, FOLDERID_Desktop, FOLDERID_Programs, KF_FLAG_DEFAULT, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONQUESTION, MB_OK, MB_YESNO};

fn known_folder(id: &GUID) -> Option<PathBuf> {
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s.map(PathBuf::from)
    }
}

/// the user's Start-menu "Programs" folder
pub fn programs_dir() -> Option<PathBuf> {
    known_folder(&FOLDERID_Programs).or_else(|| std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join(r"Microsoft\Windows\Start Menu\Programs")))
}

/// the user's desktop (follows OneDrive / folder redirection)
pub fn desktop_dir() -> Option<PathBuf> {
    known_folder(&FOLDERID_Desktop).or_else(|| std::env::var_os("USERPROFILE").map(|a| PathBuf::from(a).join("Desktop")))
}

pub fn com_init() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
}

/// create a .lnk with the real shell COM object (IShellLinkW + IPersistFile)
pub fn create_shortcut(lnk: &Path, target: &Path, workdir: &Path, desc: &str) -> Result<(), String> {
    com_init();
    unsafe {
        let sl: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(|e| format!("IShellLink: {}", e))?;
        sl.SetPath(&HSTRING::from(target.as_os_str())).map_err(|e| e.to_string())?;
        sl.SetWorkingDirectory(&HSTRING::from(workdir.as_os_str())).map_err(|e| e.to_string())?;
        sl.SetDescription(&HSTRING::from(desc)).map_err(|e| e.to_string())?;
        sl.SetIconLocation(&HSTRING::from(target.as_os_str()), 0).map_err(|e| e.to_string())?;
        let pf: IPersistFile = sl.cast().map_err(|e| e.to_string())?;
        pf.Save(&HSTRING::from(lnk.as_os_str()), true).map_err(|e| format!("{}: {}", lnk.display(), e))?;
    }
    Ok(())
}

/// tell Explorer that file associations changed (refreshes icons)
pub fn notify_assoc_changed() {
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
}

pub fn today_yyyymmdd() -> String {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!("{:04}{:02}{:02}", t.wYear, t.wMonth, t.wDay)
}

pub fn msg_info(title: &str, text: &str) {
    unsafe { MessageBoxW(None, &HSTRING::from(text), &HSTRING::from(title), MB_OK | MB_ICONINFORMATION) };
}

pub fn msg_error(title: &str, text: &str) {
    unsafe { MessageBoxW(None, &HSTRING::from(text), &HSTRING::from(title), MB_OK | MB_ICONERROR) };
}

pub fn msg_yes_no(title: &str, text: &str) -> bool {
    unsafe { MessageBoxW(None, &HSTRING::from(text), &HSTRING::from(title), MB_YESNO | MB_ICONQUESTION) == IDYES }
}
