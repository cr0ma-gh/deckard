//! Systems configured out of the box.

use crate::SystemConfig;

fn system(id: &str, name: &str, core: &str, extensions: &[&str]) -> SystemConfig {
    SystemConfig {
        id: id.into(),
        name: name.into(),
        folder: None,
        core: core.into(),
        extensions: extensions.iter().map(|e| (*e).into()).collect(),
    }
}

/// Default systems with widely used, actively maintained cores.
/// Users can edit or replace the list in `config.toml`.
pub fn default_systems() -> Vec<SystemConfig> {
    vec![
        system(
            "nes",
            "Nintendo Entertainment System",
            "fceumm",
            &["nes", "fds", "unf", "unif"],
        ),
        system(
            "snes",
            "Super Nintendo",
            "snes9x",
            &["sfc", "smc", "swc", "fig", "bs"],
        ),
        system("gb", "Game Boy", "gambatte", &["gb", "dmg"]),
        system("gbc", "Game Boy Color", "gambatte", &["gbc"]),
        system("gba", "Game Boy Advance", "mgba", &["gba"]),
        system(
            "megadrive",
            "Sega Mega Drive",
            "genesis_plus_gx",
            &["md", "gen", "smd", "bin"],
        ),
        system(
            "psx",
            "PlayStation",
            "pcsx_rearmed",
            &["cue", "chd", "pbp", "m3u"],
        ),
    ]
}
