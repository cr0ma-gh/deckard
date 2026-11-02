//! User configuration for Deckard (`config.toml`).
//!
//! The file lives in `<config dir>/deckard/config.toml` (`~/.config/deckard/` on
//! Linux). Relative paths inside it are resolved against the data directory
//! (`~/.local/share/deckard/` on Linux). A missing file is created with defaults;
//! missing fields take their default value, so old files keep working.

mod core_options;
mod systems;

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use crate::core_options::CoreOptionsStore;
pub use crate::systems::default_systems;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read or write {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("invalid TOML in {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("cannot serialize configuration: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("invalid configuration: {0}")]
    Invalid(String),
}

/// Directory layout. Relative paths are resolved against the data directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Paths {
    /// One subfolder per system (see [`SystemConfig::folder`]).
    pub roms: PathBuf,
    /// Libretro cores (`<name>_libretro.so`).
    pub cores: PathBuf,
    /// BIOS and system files.
    pub system: PathBuf,
    /// SRAM saves.
    pub saves: PathBuf,
    /// Save states.
    pub states: PathBuf,
    /// Cover images: `<thumbnails>/<system id>/<game>.png`.
    pub thumbnails: PathBuf,
    /// Saved core option values, one file per core.
    pub core_options: PathBuf,
}

impl Default for Paths {
    fn default() -> Self {
        Self {
            roms: "roms".into(),
            cores: "cores".into(),
            system: "system".into(),
            saves: "saves".into(),
            states: "states".into(),
            thumbnails: "thumbnails".into(),
            core_options: "core-options".into(),
        }
    }
}

impl Paths {
    /// Returns a copy where every relative path is joined to `base`.
    pub fn resolve(&self, base: &Path) -> Self {
        let r = |p: &PathBuf| {
            if p.is_absolute() {
                p.clone()
            } else {
                base.join(p)
            }
        };
        Self {
            roms: r(&self.roms),
            cores: r(&self.cores),
            system: r(&self.system),
            saves: r(&self.saves),
            states: r(&self.states),
            thumbnails: r(&self.thumbnails),
            core_options: r(&self.core_options),
        }
    }

    /// Directories searched for cores, in order: the user's `cores` folder,
    /// then (on Linux) the distribution's [`SYSTEM_CORE_DIR`].
    pub fn core_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = vec![self.cores.clone()];
        let system = PathBuf::from(SYSTEM_CORE_DIR);
        if cfg!(target_os = "linux") && system != self.cores {
            dirs.push(system);
        }
        dirs
    }

    /// Creates every directory that does not exist yet.
    pub fn create_all(&self) -> Result<(), ConfigError> {
        for dir in [
            &self.roms,
            &self.cores,
            &self.system,
            &self.saves,
            &self.states,
            &self.thumbnails,
            &self.core_options,
        ] {
            fs::create_dir_all(dir).map_err(|source| ConfigError::Io {
                path: dir.clone(),
                source,
            })?;
        }
        Ok(())
    }
}

/// One emulated system shown in the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemConfig {
    /// Short unique id, e.g. `snes`. Also the default folder name.
    pub id: String,
    /// Name shown in the menu.
    pub name: String,
    /// Folder under `paths.roms`; defaults to `id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// Core name (`snes9x` → `snes9x_libretro.so`) or a path to a core file.
    pub core: String,
    /// Accepted file extensions, lowercase, without dots.
    pub extensions: Vec<String>,
}

impl SystemConfig {
    pub fn folder(&self) -> &str {
        self.folder.as_deref().unwrap_or(&self.id)
    }

    /// Path of the core library for this system, searched in `core_dirs`
    /// (see [`Paths::core_dirs`]).
    pub fn core_path(&self, core_dirs: &[PathBuf]) -> PathBuf {
        find_core(&self.core, core_dirs)
    }

    /// True if `path` has one of this system's extensions.
    pub fn accepts(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| self.extensions.iter().any(|x| x.eq_ignore_ascii_case(ext)))
    }
}

/// Resolves a core name to a file: `snes9x` → `<cores>/snes9x_libretro.so`.
/// Values that already look like a file (path separator or library suffix) are
/// taken as paths, relative to `cores_dir` when not absolute.
pub fn core_path(core: &str, cores_dir: &Path) -> PathBuf {
    let suffix = std::env::consts::DLL_SUFFIX;
    let looks_like_file = core.contains(['/', '\\']) || core.ends_with(suffix);
    if looks_like_file {
        let p = PathBuf::from(core);
        if p.is_absolute() {
            p
        } else {
            cores_dir.join(p)
        }
    } else {
        cores_dir.join(format!("{core}_libretro{suffix}"))
    }
}

/// Where Linux distributions install libretro cores (Arch: `libretro-*` packages).
pub const SYSTEM_CORE_DIR: &str = "/usr/lib/libretro";

/// The first existing file for `core` in `dirs`; if none exists, the path in the
/// first directory (so error messages point where the user should put it).
pub fn find_core(core: &str, dirs: &[PathBuf]) -> PathBuf {
    let candidates: Vec<PathBuf> = dirs.iter().map(|d| core_path(core, d)).collect();
    candidates
        .iter()
        .find(|p| p.is_file())
        .or(candidates.first())
        .cloned()
        .unwrap_or_else(|| PathBuf::from(core))
}

/// Post-processing video shader applied over the game framebuffer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShaderMode {
    #[default]
    None,
    Scanlines,
    Crt,
}

impl ShaderMode {
    /// Cycles to the next shader mode: None -> Scanlines -> Crt -> None.
    pub fn next(self) -> Self {
        match self {
            Self::None => Self::Scanlines,
            Self::Scanlines => Self::Crt,
            Self::Crt => Self::None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Scanlines => "scanlines",
            Self::Crt => "crt",
        }
    }
}

/// Video settings (used by the menu in phase 2c and 5a).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoConfig {
    /// Scale by whole multiples only (sharper pixels, may leave borders).
    pub integer_scale: bool,
    /// Bilinear filtering instead of nearest.
    pub smooth: bool,
    pub fullscreen: bool,
    /// Post-processing CRT / retro shader.
    pub shader: ShaderMode,
}

// Not derivable on every platform: fullscreen defaults to true on Linux (the Deck).
#[allow(clippy::derivable_impls)]
impl Default for VideoConfig {
    fn default() -> Self {
        Self {
            integer_scale: false,
            smooth: false,
            fullscreen: cfg!(target_os = "linux"),
            shader: ShaderMode::None,
        }
    }
}

/// The whole `config.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Interface language: `en` or `it`.
    pub language: String,
    pub paths: Paths,
    pub video: VideoConfig,
    /// Number of save state slots per game.
    pub state_slots: u8,
    #[serde(rename = "system")]
    pub systems: Vec<SystemConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            language: "en".into(),
            paths: Paths::default(),
            video: VideoConfig::default(),
            state_slots: 10,
            systems: default_systems(),
        }
    }
}

impl Config {
    /// Default location of `config.toml`.
    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("deckard")
            .join("config.toml")
    }

    /// Default base for relative paths.
    pub fn default_data_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("deckard")
    }

    /// Parses and validates a configuration file's contents.
    pub fn from_toml(text: &str, path: &Path) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(text).map_err(|source| ConfigError::Parse {
            path: path.into(),
            source,
        })?;
        config.validate()?;
        Ok(config)
    }

    /// Loads `path`, writing the defaults there first if it does not exist.
    pub fn load_or_create(path: &Path) -> Result<Self, ConfigError> {
        let io_err = |source| ConfigError::Io {
            path: path.into(),
            source,
        };
        match fs::read_to_string(path) {
            Ok(text) => Self::from_toml(&text, path),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let config = Self::default();
                config.save(path)?;
                tracing::info!(path = %path.display(), "default configuration written");
                Ok(config)
            }
            Err(e) => Err(io_err(e)),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let text = format!(
            "# Deckard configuration. Relative paths are inside the data directory.\n\n{}",
            toml::to_string_pretty(self)?
        );
        write_atomic(path, text.as_bytes())
    }

    /// Checks invariants serde cannot express.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let mut ids = HashSet::new();
        for s in &self.systems {
            if s.id.is_empty() || s.core.is_empty() || s.folder().is_empty() {
                return Err(ConfigError::Invalid(format!(
                    "system '{}': id, folder and core must not be empty",
                    s.id
                )));
            }
            if !ids.insert(&s.id) {
                return Err(ConfigError::Invalid(format!(
                    "duplicate system id '{}'",
                    s.id
                )));
            }
            if s.extensions.is_empty()
                || s.extensions
                    .iter()
                    .any(|e| e.is_empty() || e.starts_with('.'))
            {
                return Err(ConfigError::Invalid(format!(
                    "system '{}': extensions must be non-empty and written without dots",
                    s.id
                )));
            }
        }
        if self.state_slots == 0 {
            return Err(ConfigError::Invalid(
                "state_slots must be at least 1".into(),
            ));
        }
        Ok(())
    }

    pub fn system(&self, id: &str) -> Option<&SystemConfig> {
        self.systems.iter().find(|s| s.id == id)
    }

    /// Finds the system for a content file: by its folder under `roms` first,
    /// then by extension (first match).
    pub fn system_for(&self, content: &Path, roms_dir: &Path) -> Option<&SystemConfig> {
        let by_folder = content.strip_prefix(roms_dir).ok().and_then(|rel| {
            let first = rel.components().next()?.as_os_str().to_str()?;
            self.systems
                .iter()
                .find(|s| s.folder() == first && s.accepts(content))
        });
        by_folder.or_else(|| self.systems.iter().find(|s| s.accepts(content)))
    }
}

/// Writes through a temporary file and a rename, so a crash never leaves a
/// half-written file behind.
pub fn write_atomic(path: &Path, data: &[u8]) -> Result<(), ConfigError> {
    let io_err = |source| ConfigError::Io {
        path: path.into(),
        source,
    };
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(io_err)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, data).map_err(io_err)?;
    fs::rename(&tmp, path).map_err(io_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip_through_toml() {
        let config = Config::default();
        config.validate().unwrap();
        let text = toml::to_string_pretty(&config).unwrap();
        assert_eq!(Config::from_toml(&text, Path::new("x")).unwrap(), config);
    }

    #[test]
    fn missing_fields_take_defaults() {
        let config = Config::from_toml("[video]\nsmooth = true\n", Path::new("x")).unwrap();
        assert!(config.video.smooth);
        assert_eq!(config.paths, Paths::default());
        assert_eq!(config.systems, default_systems());
        assert_eq!(config.state_slots, 10);
    }

    #[test]
    fn shader_mode_cycles_and_roundtrips() {
        assert_eq!(ShaderMode::None.next(), ShaderMode::Scanlines);
        assert_eq!(ShaderMode::Scanlines.next(), ShaderMode::Crt);
        assert_eq!(ShaderMode::Crt.next(), ShaderMode::None);

        let config = Config::from_toml("[video]\nshader = \"crt\"\n", Path::new("x")).unwrap();
        assert_eq!(config.video.shader, ShaderMode::Crt);
    }

    #[test]
    fn user_systems_replace_defaults() {
        let text = r#"
            [[system]]
            id = "test"
            name = "Test"
            core = "deckard_test_core"
            extensions = ["bin"]
        "#;
        let config = Config::from_toml(text, Path::new("x")).unwrap();
        assert_eq!(config.systems.len(), 1);
        assert_eq!(config.systems[0].folder(), "test");
    }

    #[test]
    fn rejects_invalid_systems() {
        let dup = r#"
            [[system]]
            id = "a"
            name = "A"
            core = "c"
            extensions = ["x"]
            [[system]]
            id = "a"
            name = "A2"
            core = "c"
            extensions = ["y"]
        "#;
        assert!(matches!(
            Config::from_toml(dup, Path::new("x")),
            Err(ConfigError::Invalid(_))
        ));
        let dotted = "[[system]]\nid = \"a\"\nname = \"A\"\ncore = \"c\"\nextensions = [\".x\"]\n";
        assert!(matches!(
            Config::from_toml(dotted, Path::new("x")),
            Err(ConfigError::Invalid(_))
        ));
        assert!(matches!(
            Config::from_toml("state_slots = [", Path::new("x")),
            Err(ConfigError::Parse { .. })
        ));
    }

    #[test]
    fn resolves_relative_paths_only() {
        let base = Path::new("/data");
        let mut paths = Paths::default();
        let abs = std::env::temp_dir();
        paths.cores = abs.clone();
        let r = paths.resolve(base);
        assert_eq!(r.roms, base.join("roms"));
        assert_eq!(r.cores, abs);
    }

    #[test]
    fn core_names_and_paths() {
        let dir = Path::new("cores");
        let suffix = std::env::consts::DLL_SUFFIX;
        assert_eq!(
            core_path("snes9x", dir),
            dir.join(format!("snes9x_libretro{suffix}"))
        );
        assert_eq!(
            core_path(&format!("my{suffix}"), dir),
            dir.join(format!("my{suffix}"))
        );
        assert_eq!(core_path("sub/x.so", dir), dir.join("sub/x.so"));
    }

    #[test]
    fn finds_cores_in_the_first_directory_that_has_them() {
        let root = std::env::temp_dir().join(format!("deckard-cores-{}", std::process::id()));
        let (user, system) = (root.join("user"), root.join("system"));
        fs::create_dir_all(&user).unwrap();
        fs::create_dir_all(&system).unwrap();
        let suffix = std::env::consts::DLL_SUFFIX;
        fs::write(system.join(format!("snes9x_libretro{suffix}")), b"x").unwrap();
        let dirs = [user.clone(), system.clone()];

        assert_eq!(
            find_core("snes9x", &dirs),
            system.join(format!("snes9x_libretro{suffix}"))
        );
        fs::write(user.join(format!("snes9x_libretro{suffix}")), b"x").unwrap();
        assert_eq!(
            find_core("snes9x", &dirs),
            user.join(format!("snes9x_libretro{suffix}")),
            "user first"
        );
        assert_eq!(
            find_core("missing", &dirs),
            user.join(format!("missing_libretro{suffix}"))
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn system_core_dir_is_searched_on_linux() {
        let dirs = Paths::default().resolve(Path::new("data")).core_dirs();
        assert_eq!(dirs[0], Path::new("data").join("cores"));
        assert_eq!(dirs.len(), if cfg!(target_os = "linux") { 2 } else { 1 });
    }

    #[test]
    fn finds_system_by_folder_then_extension() {
        let config = Config::default();
        let roms = Path::new("roms");
        // .bin is both Mega Drive and (not by default) others: the folder decides.
        let md = config
            .system_for(&roms.join("megadrive").join("Sonic.bin"), roms)
            .unwrap();
        assert_eq!(md.id, "megadrive");
        let snes = config
            .system_for(Path::new("elsewhere/Mario.SFC"), roms)
            .unwrap();
        assert_eq!(snes.id, "snes");
        assert!(config.system_for(Path::new("notes.txt"), roms).is_none());
    }

    #[test]
    fn load_or_create_writes_defaults() {
        let dir = std::env::temp_dir().join(format!("deckard-config-test-{}", std::process::id()));
        let path = dir.join("config.toml");
        let _ = fs::remove_dir_all(&dir);
        let created = Config::load_or_create(&path).unwrap();
        assert!(path.exists());
        assert_eq!(Config::load_or_create(&path).unwrap(), created);
        fs::remove_dir_all(&dir).unwrap();
    }
}
