//! Game library: scans `roms/<system>/` folders and finds covers.
//!
//! Layout (see `deckard-config`):
//! - games: `<roms>/<system folder>/**/<game>.<ext>` (up to [`MAX_DEPTH`] levels);
//! - covers: `<thumbnails>/<system id>/<game>.png`, or the libretro-thumbnails
//!   layout `<thumbnails>/<system id>/Named_Boxarts/<game>.png`.

mod states;

use std::fs;
use std::path::{Path, PathBuf};

use deckard_config::{Config, Paths, SystemConfig};

pub use crate::states::{StateSlots, state_path};

/// How deep to look inside a system folder (multi-disc games often use subfolders).
pub const MAX_DEPTH: usize = 3;

/// One playable file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Game {
    pub title: String,
    pub path: PathBuf,
    pub thumbnail: Option<PathBuf>,
}

impl Game {
    /// File name without extension: the key for saves, states and covers.
    pub fn stem(&self) -> &str {
        self.path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&self.title)
    }
}

/// A system and its games.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemEntry {
    pub id: String,
    pub name: String,
    pub core_path: PathBuf,
    pub core_available: bool,
    pub games: Vec<Game>,
}

/// Every configured system, in configuration order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Library {
    pub systems: Vec<SystemEntry>,
}

impl Library {
    /// Scans all systems. Missing folders give empty systems, never errors.
    pub fn scan(config: &Config, paths: &Paths) -> Self {
        let systems = config
            .systems
            .iter()
            .map(|system| scan_system(system, paths))
            .collect::<Vec<_>>();
        let games: usize = systems.iter().map(|s| s.games.len()).sum();
        tracing::info!(systems = systems.len(), games, "library scanned");
        Self { systems }
    }

    /// Systems that have at least one game.
    pub fn non_empty(&self) -> impl Iterator<Item = &SystemEntry> {
        self.systems.iter().filter(|s| !s.games.is_empty())
    }

    pub fn system(&self, id: &str) -> Option<&SystemEntry> {
        self.systems.iter().find(|s| s.id == id)
    }
}

fn scan_system(system: &SystemConfig, paths: &Paths) -> SystemEntry {
    let core_path = system.core_path(&paths.core_dirs());
    let core_available = core_path.is_file();
    let folder = paths.roms.join(system.folder());

    let mut files = Vec::new();
    collect_files(&folder, system, 0, &mut files);

    let thumbs = paths.thumbnails.join(&system.id);
    let mut games: Vec<Game> = files
        .into_iter()
        .map(|path| {
            let title = title_from_path(&path);
            let thumbnail = find_thumbnail(&thumbs, &path, &title);
            Game {
                title,
                path,
                thumbnail,
            }
        })
        .collect();
    games.sort_by_cached_key(|g| (g.title.to_lowercase(), g.path.clone()));

    if !games.is_empty() && !core_available {
        tracing::warn!(system = system.id, core = %core_path.display(), "core not found: games cannot start");
    }
    SystemEntry {
        id: system.id.clone(),
        name: system.name.clone(),
        core_path,
        core_available,
        games,
    }
}

fn collect_files(dir: &Path, system: &SystemConfig, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let hidden = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('.'));
        if hidden {
            continue;
        }
        if kind.is_dir() && depth + 1 < MAX_DEPTH {
            collect_files(&path, system, depth + 1, out);
        } else if kind.is_file() && system.accepts(&path) {
            out.push(path);
        }
    }
}

/// Title shown in the menu: the file name without its extension.
/// Region/revision tags like "(USA)" are kept: they tell versions apart.
pub fn title_from_path(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// libretro-thumbnails replaces these characters with `_` in file names.
fn thumbnail_name(title: &str) -> String {
    title
        .chars()
        .map(|c| if "&*/:`<>?\\|\"".contains(c) { '_' } else { c })
        .collect()
}

fn find_thumbnail(dir: &Path, game: &Path, title: &str) -> Option<PathBuf> {
    let stem = game.file_stem()?.to_string_lossy();
    [
        dir.join(format!("{stem}.png")),
        dir.join("Named_Boxarts")
            .join(format!("{}.png", thumbnail_name(title))),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        root: PathBuf,
        paths: Paths,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("deckard-lib-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            let paths = Paths::default().resolve(&root);
            paths.create_all().unwrap();
            Self { root, paths }
        }

        fn touch(&self, rel: impl AsRef<Path>) {
            let p = self.root.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, b"x").unwrap();
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn scans_by_folder_and_extension() {
        let f = Fixture::new("scan");
        f.touch("roms/snes/Zelda (USA).sfc");
        f.touch("roms/snes/aladdin.SMC");
        f.touch("roms/snes/readme.txt");
        f.touch("roms/snes/.hidden.sfc");
        f.touch("roms/snes/hacks/deep/too/deep.sfc"); // depth 3: ignored
        f.touch("roms/snes/hacks/Mario Hack.sfc");
        f.touch("roms/gba/Metroid.gba");
        f.touch(format!(
            "cores/snes9x_libretro{}",
            std::env::consts::DLL_SUFFIX
        ));

        let lib = Library::scan(&Config::default(), &f.paths);
        let snes = lib.system("snes").unwrap();
        let titles: Vec<_> = snes.games.iter().map(|g| g.title.as_str()).collect();
        assert_eq!(
            titles,
            ["aladdin", "Mario Hack", "Zelda (USA)"],
            "sorted, case-insensitive"
        );
        assert!(snes.core_available);
        assert!(!lib.system("gba").unwrap().core_available);
        let non_empty: Vec<_> = lib.non_empty().map(|s| s.id.as_str()).collect();
        assert_eq!(non_empty, ["snes", "gba"]);
    }

    #[test]
    fn missing_rom_folders_are_empty() {
        let f = Fixture::new("empty");
        let lib = Library::scan(&Config::default(), &f.paths);
        assert_eq!(lib.systems.len(), Config::default().systems.len());
        assert_eq!(lib.non_empty().count(), 0);
    }

    #[test]
    fn finds_thumbnails_in_both_layouts() {
        let f = Fixture::new("thumbs");
        f.touch("roms/snes/Mario.sfc");
        f.touch("roms/snes/Q*bert: Arcade.sfc".replace(['*', ':'], "_"));
        f.touch("roms/snes/Plain.sfc");
        f.touch("thumbnails/snes/Mario.png");
        f.touch("thumbnails/snes/Named_Boxarts/Q_bert_ Arcade.png");

        let lib = Library::scan(&Config::default(), &f.paths);
        let snes = lib.system("snes").unwrap();
        let thumb = |title: &str| {
            snes.games
                .iter()
                .find(|g| g.title == title)
                .unwrap()
                .thumbnail
                .clone()
        };
        assert_eq!(
            thumb("Mario"),
            Some(f.paths.thumbnails.join("snes/Mario.png"))
        );
        assert!(thumb("Q_bert_ Arcade").is_some());
        assert_eq!(thumb("Plain"), None);
    }

    #[test]
    fn thumbnail_names_follow_libretro_rules() {
        assert_eq!(thumbnail_name("Q*bert: A/B?"), "Q_bert_ A_B_");
        assert_eq!(
            title_from_path(Path::new("x/Sonic (Europe).md")),
            "Sonic (Europe)"
        );
    }
}
