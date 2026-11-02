//! Deckard: libretro frontend for Steam Deck.

mod app;
mod pacing;

use std::path::PathBuf;

use anyhow::{Context, bail};
use clap::Parser;
use deckard_core::{Core, CoreConfig};
use tracing_subscriber::EnvFilter;

/// Command line arguments.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Path to a libretro core (.so on Linux, .dll on Windows).
    #[arg(long)]
    core: Option<PathBuf>,

    /// Content (ROM) to load. Optional for cores that can start without content.
    #[arg(long, requires = "core")]
    rom: Option<PathBuf>,

    /// Directory with BIOS and system files [default: <data dir>/deckard/system].
    #[arg(long)]
    system_dir: Option<PathBuf>,

    /// Directory for SRAM saves [default: <data dir>/deckard/saves].
    #[arg(long)]
    save_dir: Option<PathBuf>,

    /// Core option override, repeatable: -o key=value
    #[arg(short = 'o', long = "option", value_parser = parse_option)]
    options: Vec<(String, String)>,

    /// Start in fullscreen.
    #[arg(long)]
    fullscreen: bool,

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

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,wgpu_core=warn,wgpu_hal=warn".into()),
        )
        .init();

    let args = Args::parse();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting Deckard");

    let Some(core_path) = args.core else {
        bail!(
            "no core given: the menu arrives in phase 2, for now use --core <path> [--rom <path>]"
        );
    };

    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("deckard");
    let config = CoreConfig {
        system_dir: args.system_dir.unwrap_or_else(|| data_dir.join("system")),
        save_dir: args.save_dir.unwrap_or_else(|| data_dir.join("saves")),
        option_overrides: args.options,
    };
    for dir in [&config.system_dir, &config.save_dir] {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }

    let mut core = Core::load(&core_path, config)?;
    core.load_game(args.rom.as_deref())?;

    app::run(
        core,
        app::Options {
            fullscreen: args.fullscreen,
            max_frames: args.max_frames,
        },
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
