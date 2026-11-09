//! Install / uninstall logic. Everything lives in the user's profile and HKEY_CURRENT_USER,
//! so no administrator rights are ever needed.

use crate::win;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

pub const APP: &str = "DaedalusEngine";
pub const APP_EXE: &str = "DaedalusEngine.exe";
pub const UNINSTALLER: &str = "uninstall.exe";
pub const VERSION: &str = "V1.1.0";
pub const VERSION_NUM: &str = env!("CARGO_PKG_VERSION");
pub const PUBLISHER: &str = "Johnny.ch.Tsai";
pub const DESC: &str = "DaedalusEngine 電力電子電路模擬器";
const UNINST_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\DaedalusEngine";
const APP_PATHS_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\App Paths\DaedalusEngine.exe";
const CLASSES: &str = r"Software\Classes";
const PROGID: &str = "DaedalusEngine.psimx";
const EXT: &str = ".psimx";

static PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.bin"));
const RAW_SIZE: &str = include_str!(concat!(env!("OUT_DIR"), "/payload_size.txt"));

/// uncompressed size of the payload in bytes
pub fn payload_size() -> u64 {
    RAW_SIZE.trim().parse().unwrap_or(0)
}

/// unpack the embedded archive into (file name, contents)
pub fn unpack() -> Result<Vec<(String, Vec<u8>)>, String> {
    let raw = miniz_oxide::inflate::decompress_to_vec_with_limit(PAYLOAD, payload_size() as usize + 16).map_err(|e| format!("安裝檔內容損毀（{:?}）", e.status))?;
    let mut r = &raw[..];
    let mut u32b = [0u8; 4];
    let mut u64b = [0u8; 8];
    let bad = |_| "安裝檔內容損毀".to_string();
    r.read_exact(&mut u32b).map_err(bad)?;
    let n = u32::from_le_bytes(u32b) as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        r.read_exact(&mut u32b).map_err(bad)?;
        let mut name = vec![0u8; u32::from_le_bytes(u32b) as usize];
        r.read_exact(&mut name).map_err(bad)?;
        r.read_exact(&mut u64b).map_err(bad)?;
        let mut data = vec![0u8; u64::from_le_bytes(u64b) as usize];
        r.read_exact(&mut data).map_err(bad)?;
        let name = String::from_utf8(name).map_err(|_| "安裝檔內容損毀".to_string())?;
        if name.contains(['/', '\\', ':']) || name.starts_with('.') {
            return Err("安裝檔內容損毀（檔名）".into());
        }
        out.push((name, data));
    }
    Ok(out)
}

fn hkcu() -> RegKey {
    RegKey::predef(HKEY_CURRENT_USER)
}

/// an existing installation: (folder, version)
pub fn existing_install() -> Option<(PathBuf, String)> {
    let k = hkcu().open_subkey_with_flags(UNINST_KEY, KEY_READ).ok()?;
    let loc: String = k.get_value("InstallLocation").ok()?;
    let ver: String = k.get_value("DisplayVersion").unwrap_or_default();
    Some((PathBuf::from(loc), ver))
}

/// choices recorded by the existing installation: (desktop shortcut, .psimx association); default on
pub fn existing_prefs() -> (bool, bool) {
    let Ok(k) = hkcu().open_subkey_with_flags(UNINST_KEY, KEY_READ) else { return (true, true) };
    let d: u32 = k.get_value("DesktopShortcut").unwrap_or(1);
    let a: u32 = k.get_value("FileAssociation").unwrap_or(1);
    (d != 0, a != 0)
}

fn ver_tuple(v: &str) -> Vec<u32> {
    v.trim().trim_start_matches(['V', 'v']).split('.').map(|s| s.trim().parse().unwrap_or(0)).collect()
}

/// this installer's version against an installed one: Greater → update, Equal → repair, Less → downgrade
pub fn compare_installed(installed: &str) -> std::cmp::Ordering {
    let (mut a, mut b) = (ver_tuple(VERSION_NUM), ver_tuple(installed));
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a.cmp(&b)
}

/// "V1.0.1" from a registry DisplayVersion ("1.0.1"); "舊版" when unknown
pub fn ver_label(v: &str) -> String {
    if v.trim().is_empty() {
        "舊版".into()
    } else {
        format!("V{}", v.trim().trim_start_matches(['V', 'v']))
    }
}

/// what a repair will put right: missing or altered program files, shortcuts and registry entries
pub fn diagnose(o: &Options) -> Vec<String> {
    let mut v = vec![];
    if let Ok(files) = unpack() {
        for (name, data) in &files {
            match fs::read(o.dir.join(name)) {
                Err(_) => v.push(format!("{} 遺失", name)),
                Ok(d) if d != *data => v.push(format!("{} 已損毀或與此版本不符", name)),
                _ => {}
            }
        }
    }
    if !o.dir.join(UNINSTALLER).exists() {
        v.push(format!("{} 遺失", UNINSTALLER));
    }
    let (sm, dt) = shortcut_paths();
    if sm.map(|p| !p.exists()).unwrap_or(false) {
        v.push("開始功能表捷徑遺失".into());
    }
    if o.desktop && dt.map(|p| !p.exists()).unwrap_or(false) {
        v.push("桌面捷徑遺失".into());
    }
    let exe = o.dir.join(APP_EXE);
    let ap: Option<String> = hkcu().open_subkey_with_flags(APP_PATHS_KEY, KEY_READ).ok().and_then(|k| k.get_value("").ok());
    if ap.as_deref() != Some(exe.display().to_string().as_str()) {
        v.push("應用程式路徑登錄（Win+R 啟動）遺失或不正確".into());
    }
    let un: Option<String> = hkcu().open_subkey_with_flags(UNINST_KEY, KEY_READ).ok().and_then(|k| k.get_value("UninstallString").ok());
    if un.as_deref() != Some(format!("{} --uninstall", q(&o.dir.join(UNINSTALLER))).as_str()) {
        v.push("「已安裝的應用程式」登錄資訊遺失或不正確".into());
    }
    if o.assoc {
        let cmd: Option<String> = hkcu().open_subkey_with_flags(format!(r"{}\{}\shell\open\command", CLASSES, PROGID), KEY_READ).ok().and_then(|k| k.get_value("").ok());
        if cmd.as_deref() != Some(format!("{} \"%1\"", q(&exe)).as_str()) {
            v.push(".psimx 檔案關聯遺失或被變更".into());
        }
    }
    let st = stale_files(&o.dir);
    if !st.is_empty() {
        v.push(format!("{} 個先前更新留下的暫存檔", st.len()));
    }
    v
}

pub fn default_dir() -> PathBuf {
    if let Some((p, _)) = existing_install() {
        return p;
    }
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| std::env::temp_dir());
    base.join("Programs").join(APP)
}

/// a folder the user browsed to: install into "<folder>\DaedalusEngine" unless it already is that
pub fn normalize_pick(p: PathBuf) -> PathBuf {
    if p.file_name().map(|n| n.to_string_lossy().eq_ignore_ascii_case(APP)).unwrap_or(false) {
        p
    } else {
        p.join(APP)
    }
}

/// is the file open by a running process (Windows refuses write access to a running image)?
pub fn in_use(p: &Path) -> bool {
    p.exists() && fs::OpenOptions::new().write(true).open(p).is_err()
}

pub fn app_running(dir: &Path) -> bool {
    in_use(&dir.join(APP_EXE))
}

#[derive(Clone)]
pub struct Options {
    pub dir: PathBuf,
    pub desktop: bool,
    pub assoc: bool,
}

fn check_dir(dir: &Path) -> Result<(), String> {
    if !dir.is_absolute() || dir.parent().is_none() {
        return Err("請選擇完整的安裝資料夾路徑。".into());
    }
    if dir.exists() && !dir.is_dir() {
        return Err(format!("{} 不是資料夾。", dir.display()));
    }
    fs::create_dir_all(dir).map_err(|e| perm_msg(dir, &e))?;
    let probe = dir.join(".daedalus-write-test");
    fs::write(&probe, b"ok").map_err(|e| perm_msg(dir, &e))?;
    let _ = fs::remove_file(probe);
    Ok(())
}

fn perm_msg(dir: &Path, e: &std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        format!("無法寫入 {}：此位置需要系統管理員權限。\n請選擇使用者目錄下的資料夾（例如預設的 %LOCALAPPDATA%\\Programs\\DaedalusEngine）。", dir.display())
    } else {
        format!("無法建立 {}：{}", dir.display(), e)
    }
}

/// write a file atomically; a running copy of the target is renamed out of the way first
fn put_file(path: &Path, data: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("new");
    fs::write(&tmp, data).map_err(|e| format!("寫入 {} 失敗：{}", tmp.display(), e))?;
    if fs::rename(&tmp, path).is_ok() {
        return Ok(());
    }
    // target in use: a running image can still be renamed
    let old = path.with_extension(format!("old{}", std::process::id()));
    fs::rename(path, &old).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("無法取代 {}（{}）。請先關閉 DaedalusEngine 再試一次。", path.display(), e)
    })?;
    fs::rename(&tmp, path).map_err(|e| format!("無法寫入 {}：{}", path.display(), e))
}

/// leftovers of earlier updates (renamed while running) and interrupted writes
fn stale_files(dir: &Path) -> Vec<PathBuf> {
    let mut v = vec![];
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_lowercase();
            let ours = ["daedalusengine.", "daedalus-cli.", "uninstall.", "readme.", "license."].iter().any(|p| n.starts_with(p));
            let ext = n.rsplit('.').next().unwrap_or("");
            let old = ext.strip_prefix("old").map(|d| d.chars().all(|c| c.is_ascii_digit())).unwrap_or(false);
            if ours && (ext == "new" || old) {
                v.push(e.path());
            }
        }
    }
    v
}

fn shortcut_paths() -> (Option<PathBuf>, Option<PathBuf>) {
    (win::programs_dir().map(|p| p.join(format!("{}.lnk", APP))), win::desktop_dir().map(|p| p.join(format!("{}.lnk", APP))))
}

fn dir_size(dir: &Path) -> u64 {
    fs::read_dir(dir).map(|rd| rd.flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum()).unwrap_or(0)
}

/// install (or update in place); `prog(fraction, message)` reports progress
pub fn install(o: &Options, prog: &dyn Fn(f32, &str)) -> Result<Vec<String>, String> {
    let mut warn = vec![];
    prog(0.02, "檢查安裝資料夾…");
    check_dir(&o.dir)?;
    for s in stale_files(&o.dir) {
        let _ = fs::remove_file(s);
    }
    prog(0.08, "解壓縮程式檔案…");
    let files = unpack()?;
    let total: u64 = files.iter().map(|f| f.1.len() as u64).sum::<u64>().max(1);
    let mut done = 0u64;
    for (name, data) in &files {
        prog(0.15 + 0.6 * done as f32 / total as f32, &format!("複製 {}…", name));
        put_file(&o.dir.join(name), data)?;
        done += data.len() as u64;
    }
    prog(0.78, "建立解除安裝程式…");
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let uninst = o.dir.join(UNINSTALLER);
    let same = fs::canonicalize(&me).ok().zip(fs::canonicalize(&uninst).ok()).map(|(a, b)| a == b).unwrap_or(false);
    if !same {
        let data = fs::read(&me).map_err(|e| format!("讀取安裝程式失敗：{}", e))?;
        put_file(&uninst, &data)?;
    }
    // an earlier installation in a different folder: remove its files
    if let Some((old, _)) = existing_install() {
        let differs = fs::canonicalize(&old).ok().map(|p| Some(p) != fs::canonicalize(&o.dir).ok()).unwrap_or(false);
        if differs {
            let w = remove_files(&old);
            warn.extend(w);
        }
    }
    let exe = o.dir.join(APP_EXE);
    prog(0.84, "建立開始功能表捷徑…");
    let (sm, dt) = shortcut_paths();
    if let Some(sm) = &sm {
        if let Err(e) = win::create_shortcut(sm, &exe, &o.dir, DESC) {
            warn.push(format!("無法建立開始功能表捷徑：{}", e));
        }
    }
    if let Some(dt) = &dt {
        if o.desktop {
            prog(0.88, "建立桌面捷徑…");
            if let Err(e) = win::create_shortcut(dt, &exe, &o.dir, DESC) {
                warn.push(format!("無法建立桌面捷徑：{}", e));
            }
        } else if dt.exists() {
            let _ = fs::remove_file(dt);
        }
    }
    prog(0.92, "登錄應用程式…");
    register(o, &exe).map_err(|e| format!("寫入登錄檔失敗：{}", e))?;
    if o.assoc {
        prog(0.96, "關聯 .psimx 檔案…");
        if let Err(e) = associate(&exe) {
            warn.push(format!("無法關聯 .psimx 檔案：{}", e));
        }
    } else {
        unassociate();
    }
    win::notify_assoc_changed();
    prog(1.0, "完成");
    Ok(warn)
}

fn q(p: &Path) -> String {
    format!("\"{}\"", p.display())
}

fn register(o: &Options, exe: &Path) -> std::io::Result<()> {
    let (k, _) = hkcu().create_subkey(UNINST_KEY)?;
    let un = o.dir.join(UNINSTALLER);
    k.set_value("DisplayName", &APP.to_string())?;
    k.set_value("DisplayVersion", &VERSION_NUM.to_string())?;
    k.set_value("Publisher", &PUBLISHER.to_string())?;
    k.set_value("Comments", &DESC.to_string())?;
    k.set_value("InstallLocation", &o.dir.display().to_string())?;
    k.set_value("DisplayIcon", &format!("{},0", exe.display()))?;
    k.set_value("UninstallString", &format!("{} --uninstall", q(&un)))?;
    k.set_value("QuietUninstallString", &format!("{} --uninstall --silent", q(&un)))?;
    k.set_value("InstallDate", &win::today_yyyymmdd())?;
    // 「修改」in Settings › Apps opens the maintenance page (update / repair / uninstall)
    k.set_value("ModifyPath", &format!("{} --maintain", q(&un)))?;
    k.set_value("NoModify", &0u32)?;
    k.set_value("NoRepair", &1u32)?;
    k.set_value("EstimatedSize", &((dir_size(&o.dir) / 1024) as u32))?;
    let mut parts = VERSION_NUM.split('.').map(|s| s.parse::<u32>().unwrap_or(0));
    k.set_value("VersionMajor", &parts.next().unwrap_or(0))?;
    k.set_value("VersionMinor", &parts.next().unwrap_or(0))?;
    k.set_value("DesktopShortcut", &(o.desktop as u32))?;
    k.set_value("FileAssociation", &(o.assoc as u32))?;
    // lets Win+R / Start search find "DaedalusEngine"
    let (ap, _) = hkcu().create_subkey(APP_PATHS_KEY)?;
    ap.set_value("", &exe.display().to_string())?;
    ap.set_value("Path", &o.dir.display().to_string())?;
    Ok(())
}

fn associate(exe: &Path) -> std::io::Result<()> {
    let cls = hkcu().create_subkey(CLASSES)?.0;
    let open = format!("{} \"%1\"", q(exe));
    let icon = format!("{},0", q(exe));
    let (p, _) = cls.create_subkey(PROGID)?;
    p.set_value("", &"DaedalusEngine 電路檔".to_string())?;
    p.set_value("FriendlyTypeName", &"DaedalusEngine 電路檔".to_string())?;
    p.create_subkey("DefaultIcon")?.0.set_value("", &icon)?;
    p.create_subkey(r"shell\open\command")?.0.set_value("", &open)?;
    let (e, _) = cls.create_subkey(EXT)?;
    e.set_value("", &PROGID.to_string())?;
    e.set_value("Content Type", &"application/json".to_string())?;
    e.create_subkey("OpenWithProgids")?.0.set_value(PROGID, &String::new())?;
    let (a, _) = cls.create_subkey(format!(r"Applications\{}", APP_EXE))?;
    a.set_value("FriendlyAppName", &APP.to_string())?;
    a.create_subkey(r"shell\open\command")?.0.set_value("", &open)?;
    a.create_subkey("DefaultIcon")?.0.set_value("", &icon)?;
    a.create_subkey("SupportedTypes")?.0.set_value(EXT, &String::new())?;
    Ok(())
}

/// remove only what this program registered; other programs' entries for .psimx are kept
fn unassociate() {
    let Ok(cls) = hkcu().open_subkey_with_flags(CLASSES, KEY_READ | KEY_WRITE) else { return };
    let _ = cls.delete_subkey_all(PROGID);
    let _ = cls.delete_subkey_all(format!(r"Applications\{}", APP_EXE));
    if let Ok(e) = cls.open_subkey_with_flags(EXT, KEY_READ | KEY_WRITE) {
        if let Ok(ow) = e.open_subkey_with_flags("OpenWithProgids", KEY_READ | KEY_WRITE) {
            let _ = ow.delete_value(PROGID);
            if ow.enum_values().count() == 0 && ow.enum_keys().count() == 0 {
                drop(ow);
                let _ = e.delete_subkey("OpenWithProgids");
            }
        }
        if e.get_value::<String, _>("").map(|v| v == PROGID).unwrap_or(false) {
            let _ = e.delete_value("");
            let _ = e.delete_value("Content Type");
        }
        let empty = e.enum_values().count() == 0 && e.enum_keys().count() == 0;
        drop(e);
        if empty {
            let _ = cls.delete_subkey(EXT);
        }
    }
}

/// delete the program's files from `dir` (never anything else); returns warnings
fn remove_files(dir: &Path) -> Vec<String> {
    let mut warn = vec![];
    let me = std::env::current_exe().ok().and_then(|p| fs::canonicalize(p).ok());
    let mut names: Vec<String> = unpack().map(|f| f.into_iter().map(|x| x.0).collect()).unwrap_or_else(|_| vec![APP_EXE.into()]);
    names.push(UNINSTALLER.into());
    let mut paths: Vec<PathBuf> = names.iter().map(|n| dir.join(n)).collect();
    paths.extend(stale_files(dir));
    for p in paths {
        if !p.exists() || fs::canonicalize(&p).ok() == me {
            continue;
        }
        if let Err(e) = fs::remove_file(&p) {
            warn.push(format!("無法刪除 {}：{}", p.display(), e));
        }
    }
    let _ = fs::remove_dir(dir); // only succeeds when empty
    warn
}

/// the folder to uninstall from: --dir, the registered location, or the uninstaller's own folder
pub fn uninstall_dir(arg: Option<PathBuf>) -> PathBuf {
    arg.or_else(|| existing_install().map(|x| x.0)).or_else(|| std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()))).unwrap_or_default()
}

pub fn settings_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join(APP))
}

pub fn uninstall(dir: &Path, remove_settings: bool, prog: &dyn Fn(f32, &str)) -> Result<Vec<String>, String> {
    prog(0.1, "移除捷徑…");
    let (sm, dt) = shortcut_paths();
    for p in [sm, dt].into_iter().flatten() {
        let _ = fs::remove_file(p);
    }
    prog(0.3, "移除登錄資訊…");
    unassociate();
    let _ = hkcu().delete_subkey_all(APP_PATHS_KEY);
    let _ = hkcu().delete_subkey_all(UNINST_KEY);
    prog(0.5, "刪除程式檔案…");
    let mut warn = remove_files(dir);
    if remove_settings {
        prog(0.8, "刪除使用者設定…");
        if let Some(s) = settings_dir() {
            if s.exists() {
                if let Err(e) = fs::remove_dir_all(&s) {
                    warn.push(format!("無法刪除 {}：{}", s.display(), e));
                }
            }
        }
    }
    win::notify_assoc_changed();
    prog(1.0, "完成");
    Ok(warn)
}

/// the running uninstaller cannot delete itself: let cmd.exe remove it (and the then-empty folder) after we exit
pub fn schedule_self_delete(dir: &Path) {
    use std::os::windows::process::CommandExt;
    let Ok(me) = std::env::current_exe() else { return };
    let in_dir = me.parent().and_then(|p| fs::canonicalize(p).ok()) == fs::canonicalize(dir).ok();
    if !in_dir {
        return;
    }
    let cmd = format!("/C ping -n 4 127.0.0.1 >NUL & del /F /Q \"{}\" & rmdir \"{}\"", me.display(), dir.display());
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let _ = std::process::Command::new("cmd.exe").raw_arg(cmd).current_dir(std::env::temp_dir()).creation_flags(CREATE_NO_WINDOW).spawn();
}

pub fn launch(dir: &Path) -> Result<(), String> {
    std::process::Command::new(dir.join(APP_EXE)).current_dir(dir).spawn().map(|_| ()).map_err(|e| format!("無法啟動 DaedalusEngine：{}", e))
}

/// silent-mode log, for scripted installs
pub fn log(msg: &str) {
    let p = std::env::temp_dir().join("DaedalusEngine-Setup.log");
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(p) {
        use std::io::Write;
        let _ = writeln!(f, "{}", msg);
    }
}
