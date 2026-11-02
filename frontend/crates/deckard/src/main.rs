//! Deckard: libretro frontend for Steam Deck.

use std::path::PathBuf;

use anyhow::bail;
use clap::Parser;
use tracing_subscriber::EnvFilter;

/// Command line arguments.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Path to a libretro core (.so on Linux, .dll on Windows).
    #[arg(long, requires = "rom")]
    core: Option<PathBuf>,

    /// Path to the content (ROM) to load with the core.
    #[arg(long, requires = "core")]
    rom: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let args = Args::parse();

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        libretro_api = libretro_sys::RETRO_API_VERSION,
        "starting Deckard"
    );

    if let (Some(core), Some(rom)) = (&args.core, &args.rom) {
        // Core loading lands in phase 1 (deckard-core).
        bail!(
            "running content is not implemented yet (core: {}, rom: {})",
            core.display(),
            rom.display()
        );
    }

    tracing::info!("nothing to do yet: the menu UI arrives in phase 2");
    Ok(())
}
