fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "DaedalusEngine");
        res.set("FileDescription", "DaedalusEngine 電力電子電路模擬器");
        res.set("CompanyName", "Johnny.ch.Tsai");
        res.set("LegalCopyright", "Copyright (C) 2026 Johnny.ch.Tsai. GPL-3.0");
        res.set("OriginalFilename", "DaedalusEngine.exe");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icon resource not embedded: {}", e);
        }
    }
    println!("cargo:rerun-if-changed=assets/icon.ico");
}
