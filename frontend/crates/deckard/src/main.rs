//! Deckard: libretro frontend for Steam Deck.

mod app;
mod launcher;
mod pacing;

use std::path::{Path, PathBuf};

use anyhow::bail;
use clap::Parser;
use deckard_config::Config;
use deckard_library::Library;
use tracing_subscriber::EnvFilter;

use crate::launcher::{Game, LaunchRequest};

/// Command line arguments. Without --rom or --core, Deckard opens the menu.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Content (ROM) to play directly. The core is chosen from the configured
    /// systems (by folder under the ROM directory, then by extension) unless
    /// --core is given. Deckard quits when the game ends.
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

    /// Quit after this many frames (for automated checks).
    #[arg(long, hide = true)]
    max_frames: Option<u64>,
}

fn parse_option(s: &str) -> Result<(String, String), String> {
    s.split_once('=')
        .filter(|(k, _)| !k.is_empty())
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .ok_or_else(|| format!("expected key=value, got '{s}'"))
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
    let mut paths = config.paths.resolve(&data_dir);
    if let Some(dir) = &args.system_dir {
        paths.system = dir.clone();
    }
    if let Some(dir) = &args.save_dir {
        paths.saves = dir.clone();
    }
    paths.create_all()?;

    let library = Library::scan(&config, &paths);
    if args.list {
        print_library(&library, &config_path, &paths.roms);
        return Ok(());
    }

    let direct = args.rom.is_some() || args.core.is_some();
    let game = if direct {
        let system = args
            .rom
            .as_deref()
            .and_then(|rom| config.system_for(rom, &paths.roms));
        let core_path = match (&args.core, system) {
            (Some(core), _) => core.clone(),
            (None, Some(system)) => system.core_path(&paths.cores),
            (None, None) => bail!(
                "no configured system accepts this file; add one in {} or pass --core",
                config_path.display()
            ),
        };
        let request = LaunchRequest {
            core_path,
            rom: args.rom.clone(),
            system_id: system.map(|s| s.id.clone()),
            cli_options: args.options.clone(),
        };
        Some(Game::start(request, &config, &paths)?)
    } else {
        None
    };

    let options = app::Options {
        fullscreen: args.fullscreen || (config.video.fullscreen && !args.windowed),
        max_frames: args.max_frames,
        quit_after_game: direct,
    };
    app::run(
        app::Context {
            config,
            paths,
            library,
        },
        game,
        options,
    )
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
    fn cli_is_consistent() {
        use clap::CommandFactory;
        Args::command().debug_assert();
    }
}
