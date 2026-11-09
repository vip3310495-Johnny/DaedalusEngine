//! Development aid for automated GUI tests (CI): with the environment variable DAEDALUS_SCRIPT=<file> and
//! `--shot <prefix>`, the app replays a script of input events, saves screenshots and dumps its state as JSON.
//! Never active in normal use.
//!
//! One command per line (`#` starts a comment):
//!   wait N                 let N frames pass
//!   fit                    zoom to fit the schematic
//!   wmove X Y / move X Y   move the pointer to schematic (world) / screen coordinates
//!   down B / up B / click B / dblclick     B = left | right | middle
//!   drag X1 Y1 X2 Y2       left-drag between world points
//!   key NAME [ctrl] [shift]                press and release a key (egui key names: R, Space, Enter, Escape, …)
//!   menu TEXT              click the context-menu item whose label starts with TEXT
//!   run                    start the simulation (F5)
//!   wave_add NAME / wave_math NAME EXPR / wave_fft NAME / wave_cursors T1 T2
//!   shot NAME              save <prefix>_NAME.png
//!   dump NAME              save <prefix>_NAME.json (elements, selection, view, mode, waveform screens)
//!   quit

use egui::{Event, Key, Modifiers, PointerButton, Pos2};

pub struct Script {
    pub steps: Vec<Vec<String>>,
    pub i: usize,
    pub wait: u32,
    pub pos: Pos2,
    pub prefix: String,
    /// events to send on the next frame (button / key releases)
    pub later: Vec<Event>,
    pub later_mods: Modifiers,
}

impl Script {
    pub fn load(path: &str, prefix: &str) -> Option<Script> {
        let txt = std::fs::read_to_string(path).ok()?;
        let steps = txt
            .lines()
            .map(|l| l.split('#').next().unwrap_or("").trim())
            .filter(|l| !l.is_empty())
            .map(|l| l.split_whitespace().map(String::from).collect())
            .collect();
        Some(Script { steps, i: 0, wait: 20, pos: Pos2::new(400.0, 300.0), prefix: prefix.to_string(), later: vec![], later_mods: Modifiers::NONE })
    }
}

pub fn button(s: &str) -> PointerButton {
    match s {
        "right" => PointerButton::Secondary,
        "middle" => PointerButton::Middle,
        _ => PointerButton::Primary,
    }
}

pub fn mods(args: &[String]) -> Modifiers {
    let mut m = Modifiers::NONE;
    for a in args {
        match a.as_str() {
            "ctrl" => {
                m.ctrl = true;
                m.command = true;
            }
            "shift" => m.shift = true,
            "alt" => m.alt = true,
            _ => {}
        }
    }
    m
}

pub fn key(name: &str) -> Option<Key> {
    Key::from_name(name)
}
