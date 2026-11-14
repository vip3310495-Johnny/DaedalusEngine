//! `daedalus-cli skill install [folder]`: writes the circuit-builder Agent Skill (SKILL.md and its scripts) into
//! Claude Code's skills folder, %USERPROFILE%\.claude\skills\daedalus-circuit-builder by default.
use std::path::{Path, PathBuf};

const FILES: &[(&str, &str)] = &[
    ("SKILL.md", include_str!("../../../skills/daedalus-circuit-builder/SKILL.md")),
    ("scripts/psimx_builder.py", include_str!("../../../skills/daedalus-circuit-builder/scripts/psimx_builder.py")),
    ("scripts/verify_psimx.py", include_str!("../../../skills/daedalus-circuit-builder/scripts/verify_psimx.py")),
    ("scripts/shot_psimx.py", include_str!("../../../skills/daedalus-circuit-builder/scripts/shot_psimx.py")),
    ("scripts/elements.json", include_str!("../../../skills/daedalus-circuit-builder/scripts/elements.json")),
    ("references/elements.md", include_str!("../../../skills/daedalus-circuit-builder/references/elements.md")),
    ("examples/buck_converter.py", include_str!("../../../skills/daedalus-circuit-builder/examples/buck_converter.py")),
];

pub fn default_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join(".claude").join("skills").join("daedalus-circuit-builder")
}

pub fn install(dir: &Path) -> Result<usize, String> {
    for (rel, txt) in FILES {
        let p = dir.join(rel);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| format!("無法建立 {}：{}", d.display(), e))?;
        }
        std::fs::write(&p, txt).map_err(|e| format!("無法寫入 {}：{}", p.display(), e))?;
    }
    Ok(FILES.len())
}

pub fn cmd(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("install") => {
            let dir = args.get(1).map(PathBuf::from).unwrap_or_else(default_dir);
            match install(&dir) {
                Ok(n) => {
                    println!("已安裝 daedalus-circuit-builder skill（{} 個檔案）到 {}", n, dir.display());
                    0
                }
                Err(e) => {
                    eprintln!("{}", e);
                    1
                }
            }
        }
        _ => {
            eprintln!("usage: daedalus-cli skill install [folder]   (default {})", default_dir().display());
            1
        }
    }
}
