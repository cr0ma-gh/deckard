//! Deckard: libretro frontend for Steam Deck.

mod app;
mod pacing;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use clap::Parser;
use deckard_config::{Config, CoreOptionsStore};
use deckard_core::{Core, CoreConfig};
use deckard_library::Library;
use tracing_subscriber::EnvFilter;

/// Command line arguments.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Content (ROM) to play. The core is chosen from the configured systems
    /// (by folder under the ROM directory, then by extension) unless --core is given.
    #[arg(long)]
    rom: Option<PathBuf>,

    /// Libretro core to use (.so on Linux, .dll on Windows). Without --rom,
    /// starts the core without content, if it supports that.
    #[arg(long)]
    core: Option<PathBuf>,

    /// Print the game library and exit.
    #[arg(long)]
    list: bool,

    /// Configuration file [default: <config dir>/deckard/config.toml].
    #[arg(long)]
    config: Option<PathBuf>,

    /// Base directory for relative paths in the configuration
    /// [default: <data dir>/deckard].
    #[arg(long)]
    data_dir: Option<PathBuf>,

    /// Overrides the configured BIOS/system directory.
    #[arg(long)]
    system_dir: Option<PathBuf>,

    /// Overrides the configured SRAM save directory.
    #[arg(long)]
    save_dir: Option<PathBuf>,

    /// Core option for this run only (not saved), repeatable: -o key=value
    #[arg(short = 'o', long = "option", value_parser = parse_option)]
    options: Vec<(String, String)>,

    /// Start in fullscreen.
    #[arg(long, conflicts_with = "windowed")]
    fullscreen: bool,

    /// Start in a window even if the configuration says fullscreen.
    #[arg(long)]
    windowed: bool,

    /// Quit after this many emulated frames (for automated checks).
    #[arg(long, hide = true)]
    max_frames: Option<u64>,
}

fn parse_option(s: &str) -> Result<(String, String), String> {
    s.split_once('=')
        .filter(|(k, _)| !k.is_empty())
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .ok_or_else(|| format!("expected key=value, got '{s}'"))
}

/// Name used for a core's saved options: `snes9x_libretro.so` → `snes9x`.
fn core_name(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    stem.strip_suffix("_libretro")
        .map_or(stem.clone(), str::to_owned)
}

fn print_library(library: &Library, config_path: &Path, roms: &Path) {
    // Plain stdout: this is the command's output, not a log line.
    println!("Configuration: {}", config_path.display());
    println!("ROM folder:    {}\n", roms.display());
    for system in &library.systems {
        let core = if system.core_available {
            "core ok"
        } else {
            "core missing"
        };
        println!(
            "{} [{}] — {} games, {core}: {}",
            system.name,
            system.id,
            system.games.len(),
            system.core_path.display()
        );
        for game in &system.games {
            let cover = if game.thumbnail.is_some() {
                " [cover]"
            } else {
                ""
            };
            println!("    {}{cover}", game.title);
        }
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,wgpu_core=warn,wgpu_hal=warn".into()),
        )
        .init();

    let args = Args::parse();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting Deckard");

    let config_path = args.config.clone().unwrap_or_else(Config::default_path);
    let config = Config::load_or_create(&config_path)?;
    let data_dir = args
        .data_dir
        .clone()
        .unwrap_or_else(Config::default_data_dir);
    let paths = config.paths.resolve(&data_dir);
    paths.create_all()?;

    if args.list {
        print_library(&Library::scan(&config, &paths), &config_path, &paths.roms);
        return Ok(());
    }

    let system = args
        .rom
        .as_deref()
        .and_then(|rom| config.system_for(rom, &paths.roms));
    let core_path = match (&args.core, system) {
        (Some(core), _) => core.clone(),
        (None, Some(system)) => system.core_path(&paths.cores),
        (None, None) if args.rom.is_some() => {
            bail!(
                "no configured system accepts this file; add one in {} or pass --core",
                config_path.display()
            )
        }
        (None, None) => bail!(
            "nothing to play: the menu arrives in phase 2b; use --rom <file>, --core <file> or --list"
        ),
    };
    if !core_path.is_file() {
        bail!(
            "core not found: {} (put libretro cores in {})",
            core_path.display(),
            paths.cores.display()
        );
    }

    // Saved options, then this run's -o values on top (those are not saved).
    let name = core_name(&core_path);
    let store = CoreOptionsStore::new(&paths.core_options);
    let saved = store.load(&name).unwrap_or_else(|e| {
        tracing::warn!("ignoring saved core options: {e}");
        Vec::new()
    });
    let cli_keys: HashSet<String> = args.options.iter().map(|(k, _)| k.clone()).collect();
    let mut overrides: Vec<_> = saved
        .iter()
        .filter(|(k, _)| !cli_keys.contains(k))
        .cloned()
        .collect();
    overrides.extend(args.options.iter().cloned());

    let core_config = CoreConfig {
        system_dir: args
            .system_dir
            .clone()
            .unwrap_or_else(|| paths.system.clone()),
        save_dir: args.save_dir.clone().unwrap_or_else(|| paths.saves.clone()),
        option_overrides: overrides,
    };
    for dir in [&core_config.system_dir, &core_config.save_dir] {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }

    let mut core = Core::load(&core_path, core_config)?;
    core.load_game(args.rom.as_deref())?;

    let session = app::Session {
        system_id: system.map_or_else(|| name.clone(), |s| s.id.clone()),
        game_stem: args
            .rom
            .as_deref()
            .and_then(Path::file_stem)
            .map_or_else(|| name.clone(), |s| s.to_string_lossy().into_owned()),
        states_dir: paths.states.clone(),
        state_slots: config.state_slots,
    };
    let options = app::Options {
        fullscreen: args.fullscreen || (config.video.fullscreen && !args.windowed),
        max_frames: args.max_frames,
    };
    let core = app::run(core, options, session)?;

    // Keep values changed during the session; -o values stay out, and any
    // previously saved value for those keys is kept as it was.
    let mut to_save: Vec<(&str, &str)> = core
        .options()
        .changed()
        .filter(|(k, _)| !cli_keys.contains(*k))
        .collect();
    to_save.extend(
        saved
            .iter()
            .filter(|(k, _)| cli_keys.contains(k))
            .map(|(k, v)| (k.as_str(), v.as_str())),
    );
    if let Err(e) = store.save(&name, to_save) {
        tracing::warn!("cannot save core options: {e}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_core_options() {
        assert_eq!(parse_option("a=b"), Ok(("a".into(), "b".into())));
        assert_eq!(parse_option("a=b=c"), Ok(("a".into(), "b=c".into())));
        assert!(parse_option("=b").is_err());
        assert!(parse_option("nokey").is_err());
    }

    #[test]
    fn core_names() {
        assert_eq!(core_name(Path::new("cores/snes9x_libretro.so")), "snes9x");
        assert_eq!(
            core_name(Path::new("deckard_test_core.dll")),
            "deckard_test_core"
        );
    }

    #[test]
    fn cli_is_consistent() {
        use clap::CommandFactory;
        Args::command().debug_assert();
    }
}
