//! Runs deckard-core against the real `deckard-test-core` library.

// Test helpers may panic freely.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use deckard_core::{Button, Core, CoreConfig, CoreError, PadState, PixelFormat};

/// Only one core may be loaded per process, so tests take turns.
static SERIAL: Mutex<()> = Mutex::new(());

/// Builds the test core once and returns the path of the shared library.
fn test_core_path() -> &'static Path {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        // The test binary lives in target/<profile>/deps/.
        let exe = std::env::current_exe().unwrap();
        let profile_dir = exe.parent().and_then(Path::parent).unwrap().to_path_buf();
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let mut cmd = Command::new(cargo);
        cmd.args(["build", "-q", "-p", "deckard-test-core"]);
        if profile_dir.file_name().is_some_and(|n| n == "release") {
            cmd.arg("--release");
        }
        assert!(
            cmd.status().unwrap().success(),
            "building deckard-test-core failed"
        );
        let name = format!(
            "{}deckard_test_core{}",
            std::env::consts::DLL_PREFIX,
            std::env::consts::DLL_SUFFIX
        );
        profile_dir.join(name)
    })
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("deckard-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn config(dir: &TempDir) -> CoreConfig {
    CoreConfig {
        system_dir: dir.0.join("system"),
        save_dir: dir.0.join("saves"),
        option_overrides: Vec::new(),
    }
}

fn start(config: CoreConfig) -> Core {
    let mut core = Core::load(test_core_path(), config).unwrap();
    core.load_game(None).unwrap();
    core
}

/// Reads the pixel at (x, y) of an XRGB8888 frame.
fn pixel(core: &Core, x: usize, y: usize) -> u32 {
    let frame = core.frame().unwrap();
    let i = (y * frame.width as usize + x) * 4;
    u32::from_le_bytes(frame.data[i..i + 4].try_into().unwrap())
}

fn frame_counter(state: &[u8]) -> u64 {
    u64::from_le_bytes(state[8..16].try_into().unwrap())
}

#[test]
fn reports_system_and_av_info() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new("info");
    let core = start(config(&dir));

    let info = core.system_info();
    assert_eq!(info.library_name, "Deckard Test Core");
    assert_eq!(info.valid_extensions, ["bin", "txt"]);

    let av = core.av_info();
    assert_eq!((av.base_width, av.base_height), (320, 240));
    assert!((av.aspect_ratio - 4.0 / 3.0).abs() < 1e-6);
    assert_eq!(av.fps, 60.0);
    assert_eq!(av.sample_rate, 44_100.0);
}

#[test]
fn produces_video_and_audio() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new("av");
    let mut core = start(config(&dir));

    core.run_frame();
    let frame = core.frame().unwrap();
    assert_eq!((frame.width, frame.height), (320, 240));
    assert_eq!(frame.format, PixelFormat::Xrgb8888);
    assert_eq!(frame.data.len(), 320 * 240 * 4);
    assert_eq!(core.audio().len(), 735 * 2);
    assert!(core.audio().iter().all(|&s| s == 0), "silent without input");

    let mut pad = PadState::default();
    pad.set(Button::A, true);
    core.set_pad(0, pad);
    core.run_frame();
    assert!(core.audio().iter().any(|&s| s != 0), "A plays a tone");
}

#[test]
fn input_moves_the_square() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new("input");
    let mut core = start(config(&dir));

    core.run_frame();
    assert_ne!(
        pixel(&core, 190, 120),
        0x00FF_FFFF,
        "square starts centered at x=160"
    );

    let mut pad = PadState::default();
    pad.set(Button::Right, true);
    core.set_pad(0, pad);
    for _ in 0..10 {
        core.run_frame();
    }
    assert_eq!(pixel(&core, 190, 120), 0x00FF_FFFF, "square moved to x=180");
}

#[test]
fn core_options_defaults_overrides_and_updates() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new("options");

    let mut core = start(config(&dir));
    let opt = core.options().get("deckard_test_speed").unwrap();
    assert_eq!(opt.values, ["1", "2", "4"]);
    assert_eq!(opt.value(), "1");

    assert!(core.set_option("deckard_test_speed", "2"));
    assert!(!core.set_option("deckard_test_speed", "3"));
    core.run_frame();
    let state = core.serialize().unwrap();
    assert_eq!(
        u64::from_le_bytes(state[16..24].try_into().unwrap()),
        2,
        "core saw the update"
    );
    drop(core);

    let mut cfg = config(&dir);
    cfg.option_overrides
        .push(("deckard_test_speed".into(), "4".into()));
    let core = start(cfg);
    assert_eq!(
        core.options().get("deckard_test_speed").unwrap().value(),
        "4"
    );
}

#[test]
fn save_states_round_trip() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new("states");
    let mut core = start(config(&dir));

    core.run_frame();
    let saved = core.serialize().unwrap();
    for _ in 0..5 {
        core.run_frame();
    }
    assert_ne!(core.serialize().unwrap(), saved);
    core.unserialize(&saved).unwrap();
    assert_eq!(core.serialize().unwrap(), saved);
}

#[test]
fn sram_persists_between_sessions() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new("sram");

    let mut core = start(config(&dir));
    for _ in 0..7 {
        core.run_frame();
    }
    drop(core);
    assert!(
        dir.0.join("saves").read_dir().unwrap().count() == 1,
        "one .srm written"
    );

    let mut core = start(config(&dir));
    assert_eq!(frame_counter(&core.serialize().unwrap()), 7);
    core.run_frame();
    assert_eq!(frame_counter(&core.serialize().unwrap()), 8);
}

#[test]
fn only_one_core_per_process() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new("single");
    let _core = Core::load(test_core_path(), config(&dir)).unwrap();
    assert!(matches!(
        Core::load(test_core_path(), config(&dir)),
        Err(CoreError::AlreadyLoaded)
    ));
}

#[test]
fn save_states_to_files() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new("state-files");
    let mut core = start(config(&dir));
    let path = dir.0.join("states").join("snes").join("game.state1");

    core.run_frame();
    core.save_state_to(&path).unwrap();
    assert!(path.is_file());
    let saved = core.serialize().unwrap();
    for _ in 0..5 {
        core.run_frame();
    }
    core.load_state_from(&path).unwrap();
    assert_eq!(core.serialize().unwrap(), saved);

    assert!(matches!(
        core.load_state_from(&dir.0.join("missing.state")),
        Err(CoreError::Io { .. })
    ));
}

#[test]
fn reports_changed_options() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new("changed");
    let mut core = start(config(&dir));
    assert_eq!(core.options().changed().count(), 0);
    core.set_option("deckard_test_speed", "4");
    let changed: Vec<_> = core.options().changed().collect();
    assert_eq!(changed, [("deckard_test_speed", "4")]);
}

#[test]
fn disk_control_query_and_swap() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new("disk-control");
    let mut core = start(config(&dir));

    let info = core.disk_info().expect("disk control should be supported");
    assert_eq!(info.count, 2);
    assert_eq!(info.current, 0);
    assert_eq!(info.label.as_deref(), Some("Disc 1"));
    assert!(!info.ejected);

    // Swap to Disc 2
    assert!(core.change_disk(1));
    let info = core.disk_info().unwrap();
    assert_eq!(info.current, 1);
    assert_eq!(info.label.as_deref(), Some("Disc 2"));
    assert!(!info.ejected);

    // Out of bounds disc index returns false
    assert!(!core.change_disk(99));
}
