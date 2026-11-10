//! Packs the release build of the application and its documents (README, LICENSE, the copyright notice and
//! disclaimer NOTICE.txt, THIRD-PARTY-NOTICES.txt) into a compressed payload.
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("..").join("..");
    let app = std::env::var("DAEDALUS_APP_EXE").map(PathBuf::from).unwrap_or_else(|_| root.join("target").join("release").join("DaedalusEngine.exe"));
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    match std::fs::read(&app) {
        Ok(b) => files.push(("DaedalusEngine.exe".into(), b)),
        Err(_) => panic!("找不到 {}：請先執行 cargo build -p daedalus-app --release", app.display()),
    }
    let rel = app.parent().unwrap().to_path_buf();
    let cli = rel.join("daedalus-cli.exe");
    match std::fs::read(&cli) {
        Ok(b) => files.push(("daedalus-cli.exe".into(), b)),
        Err(_) => println!("cargo:warning=daedalus-cli.exe not found; installer will not include the command-line runner"),
    }
    for doc in ["README.md", "LICENSE", "NOTICE.txt", "THIRD-PARTY-NOTICES.txt"] {
        if let Ok(b) = std::fs::read(root.join(doc)) {
            files.push((doc.into(), b));
        }
        println!("cargo:rerun-if-changed={}", root.join(doc).display());
    }
    // archive: [u32 count] then per file [u32 name len][name][u64 size][bytes]
    let mut raw = Vec::new();
    raw.write_all(&(files.len() as u32).to_le_bytes()).unwrap();
    for (n, b) in &files {
        raw.write_all(&(n.len() as u32).to_le_bytes()).unwrap();
        raw.write_all(n.as_bytes()).unwrap();
        raw.write_all(&(b.len() as u64).to_le_bytes()).unwrap();
        raw.write_all(b).unwrap();
    }
    let packed = miniz_oxide::deflate::compress_to_vec(&raw, 9);
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    std::fs::write(out.join("payload.bin"), &packed).unwrap();
    std::fs::write(out.join("payload_size.txt"), raw.len().to_string()).unwrap();
    println!("cargo:rerun-if-changed={}", app.display());
    println!("cargo:rerun-if-changed={}", cli.display());
    println!("cargo:rerun-if-env-changed=DAEDALUS_APP_EXE");
    println!("cargo:rerun-if-changed=assets/icon.ico");

    // dependent DLLs only from System32 (no DLL planting from the download folder); a CI-only screenshot build
    // (DAEDALUS_SHOT_BUILD) leaves it out so that Mesa's software opengl32.dll next to the exe is used
    println!("cargo:rerun-if-env-changed=DAEDALUS_SHOT_BUILD");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") && std::env::var("DAEDALUS_SHOT_BUILD").is_err() {
        println!("cargo:rustc-link-arg-bins=/DEPENDENTLOADFLAG:0x800");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "DaedalusEngine");
        res.set("FileDescription", "DaedalusEngine 安裝程式");
        res.set("CompanyName", "Johnny.ch.Tsai");
        res.set("LegalCopyright", "Copyright (C) 2026 Johnny.ch.Tsai. GPL-3.0");
        res.set("OriginalFilename", "DaedalusEngine-Setup.exe");
        // run as the invoking user: never ask for administrator rights
        res.set_manifest(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security><requestedPrivileges><requestedExecutionLevel level="asInvoker" uiAccess="false"/></requestedPrivileges></security>
  </trustInfo>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application><supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/></application>
  </compatibility>
</assembly>"#);
        if let Err(e) = res.compile() {
            println!("cargo:warning=resources not embedded: {}", e);
        }
    }
}
