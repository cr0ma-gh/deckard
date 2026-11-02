//! Starting and stopping games: core loading, saved options, save state names.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::bail;
use deckard_config::{Config, CoreOptionsStore, Paths};
use deckard_core::{Core, CoreConfig};
use deckard_library::{StateSlots, state_path};

use crate::pacing::Pacer;

/// What to start.
#[derive(Debug, Clone)]
pub struct LaunchRequest {
    pub core_path: PathBuf,
    pub rom: Option<PathBuf>,
    /// System of the game (for save state folders); the core name is used if unknown.
    pub system_id: Option<String>,
    /// `-o key=value` values for this session only.
    pub cli_options: Vec<(String, String)>,
}

/// Name used for a core's saved options: `snes9x_libretro.so` → `snes9x`.
pub fn core_name(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    stem.strip_suffix("_libretro")
        .map_or(stem.clone(), str::to_owned)
}

/// A running game and everything needed to save its state on exit.
pub struct Game {
    pub core: Core,
    pub pacer: Pacer,
    pub slots: StateSlots,
    pub title: String,
    system_id: String,
    game_stem: String,
    states_dir: PathBuf,
    store: CoreOptionsStore,
    core_name: String,
    cli_keys: HashSet<String>,
    saved_options: Vec<(String, String)>,
    pub frames: u64,
    pub started: Option<Instant>,
    pub underruns_at_start: u64,
}

impl Game {
    /// Loads the core and the content.
    pub fn start(request: LaunchRequest, config: &Config, paths: &Paths) -> anyhow::Result<Self> {
        if !request.core_path.is_file() {
            bail!(
                "core not found: {} (put libretro cores in {})",
                request.core_path.display(),
                paths.cores.display()
            );
        }
        // Saved options, then this session's -o values on top (those are not saved).
        let name = core_name(&request.core_path);
        let store = CoreOptionsStore::new(&paths.core_options);
        let saved_options = store.load(&name).unwrap_or_else(|e| {
            tracing::warn!("ignoring saved core options: {e}");
            Vec::new()
        });
        let cli_keys: HashSet<String> =
            request.cli_options.iter().map(|(k, _)| k.clone()).collect();
        let mut overrides: Vec<_> = saved_options
            .iter()
            .filter(|(k, _)| !cli_keys.contains(k))
            .cloned()
            .collect();
        overrides.extend(request.cli_options.iter().cloned());

        let mut core = Core::load(
            &request.core_path,
            CoreConfig {
                system_dir: paths.system.clone(),
                save_dir: paths.saves.clone(),
                option_overrides: overrides,
            },
        )?;
        core.load_game(request.rom.as_deref())?;

        let game_stem = request
            .rom
            .as_deref()
            .and_then(Path::file_stem)
            .map_or_else(|| name.clone(), |s| s.to_string_lossy().into_owned());
        let title = request.rom.as_deref().map_or_else(
            || core.system_info().library_name.clone(),
            deckard_library::title_from_path,
        );
        let pacer = Pacer::new(core.av_info().fps, None);
        Ok(Self {
            core,
            pacer,
            slots: StateSlots::new(config.state_slots),
            title,
            system_id: request.system_id.unwrap_or_else(|| name.clone()),
            game_stem,
            states_dir: paths.states.clone(),
            store,
            core_name: name,
            cli_keys,
            saved_options,
            frames: 0,
            started: None,
            underruns_at_start: 0,
        })
    }

    pub fn state_path(&self, slot: u8) -> PathBuf {
        state_path(&self.states_dir, &self.system_id, &self.game_stem, slot)
    }

    /// Logs the session and saves option changes; dropping the core then saves SRAM.
    pub fn finish(self, audio_underruns: u64) {
        if let Some(started) = self.started {
            let seconds = started.elapsed().as_secs_f64();
            tracing::info!(
                frames = self.frames,
                seconds = format!("{seconds:.2}"),
                fps = format!("{:.2}", self.frames as f64 / seconds.max(f64::EPSILON)),
                audio_underruns = audio_underruns.saturating_sub(self.underruns_at_start),
                "session ended"
            );
        }
        // Keep values changed during the session; -o values stay out, and any
        // previously saved value for those keys is kept as it was.
        let mut to_save: Vec<(&str, &str)> = self
            .core
            .options()
            .changed()
            .filter(|(k, _)| !self.cli_keys.contains(*k))
            .collect();
        to_save.extend(
            self.saved_options
                .iter()
                .filter(|(k, _)| self.cli_keys.contains(k))
                .map(|(k, v)| (k.as_str(), v.as_str())),
        );
        if let Err(e) = self.store.save(&self.core_name, to_save) {
            tracing::warn!("cannot save core options: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_names() {
        assert_eq!(core_name(Path::new("cores/snes9x_libretro.so")), "snes9x");
        assert_eq!(
            core_name(Path::new("deckard_test_core.dll")),
            "deckard_test_core"
        );
    }

    #[test]
    fn missing_core_is_a_clear_error() {
        let paths = Paths::default().resolve(Path::new("data"));
        let request = LaunchRequest {
            core_path: "data/cores/none_libretro.so".into(),
            rom: None,
            system_id: None,
            cli_options: Vec::new(),
        };
        let err = Game::start(request, &Config::default(), &paths)
            .err()
            .map(|e| e.to_string());
        assert!(err.is_some_and(|e| e.contains("core not found")));
    }
}
