//! Loads and runs libretro cores.
//!
//! [`Core`] owns a loaded core library and drives it: `retro_init`, content
//! loading, one `retro_run` per [`Core::run_frame`], SRAM persistence and
//! teardown on drop. All calls must happen on the thread that created the core.

mod api;
mod callbacks;
pub mod input;
pub mod options;

use std::ffi::{CStr, CString, c_uint, c_void};
use std::fs;
use std::io;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use libloading::Library;
use libretro_sys::*;

use crate::api::CoreApi;
use crate::callbacks::{Shared, enter};
pub use crate::input::{Button, MAX_PORTS, PadState};
pub use crate::options::{CoreOption, CoreOptions};

/// Only one core may be loaded per process: many cores keep global state.
static CORE_LOADED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("another core is already loaded in this process")]
    AlreadyLoaded,
    #[error("cannot load core library {path}: {source}")]
    Library {
        path: PathBuf,
        source: libloading::Error,
    },
    #[error("core does not export {name}: {source}")]
    MissingSymbol {
        name: &'static str,
        source: libloading::Error,
    },
    #[error("core uses libretro API version {0}, expected {RETRO_API_VERSION}")]
    ApiVersion(c_uint),
    #[error("path contains a NUL byte: {0}")]
    InvalidPath(PathBuf),
    #[error("cannot read {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("this core needs content (a ROM) to start")]
    ContentRequired,
    #[error("a game is already loaded")]
    GameAlreadyLoaded,
    #[error("the core failed to load the content")]
    LoadGameFailed,
    #[error("save state failed")]
    SaveState,
}

/// Pixel formats supported for software-rendered frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// 16-bit 0RGB1555 (libretro default).
    Rgb1555,
    /// 32-bit XRGB8888, native endian.
    Xrgb8888,
    /// 16-bit RGB565.
    Rgb565,
}

impl PixelFormat {
    fn from_raw(raw: retro_pixel_format) -> Option<Self> {
        match raw {
            RETRO_PIXEL_FORMAT_0RGB1555 => Some(Self::Rgb1555),
            RETRO_PIXEL_FORMAT_XRGB8888 => Some(Self::Xrgb8888),
            RETRO_PIXEL_FORMAT_RGB565 => Some(Self::Rgb565),
            _ => None,
        }
    }

    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Xrgb8888 => 4,
            Self::Rgb1555 | Self::Rgb565 => 2,
        }
    }
}

/// Video and audio timing of the running content.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AvInfo {
    pub base_width: u32,
    pub base_height: u32,
    pub max_width: u32,
    pub max_height: u32,
    /// Display aspect ratio (width / height), always positive.
    pub aspect_ratio: f32,
    pub fps: f64,
    pub sample_rate: f64,
}

impl AvInfo {
    fn from_raw(raw: &retro_system_av_info) -> Self {
        let mut info = Self {
            fps: raw.timing.fps,
            sample_rate: raw.timing.sample_rate,
            ..Self::default()
        };
        info.set_geometry(&raw.geometry);
        info
    }

    fn set_geometry(&mut self, g: &retro_game_geometry) {
        self.base_width = g.base_width;
        self.base_height = g.base_height;
        // SET_GEOMETRY must not change the maximum size; keep a valid one anyway.
        if self.max_width == 0 || self.max_height == 0 {
            self.max_width = g.max_width;
            self.max_height = g.max_height;
        }
        self.aspect_ratio = if g.aspect_ratio > 0.0 {
            g.aspect_ratio
        } else if g.base_height > 0 {
            g.base_width as f32 / g.base_height as f32
        } else {
            4.0 / 3.0
        };
    }
}

/// Static information about a core.
#[derive(Debug, Clone)]
pub struct SystemInfo {
    pub library_name: String,
    pub library_version: String,
    /// Accepted file extensions, without dots.
    pub valid_extensions: Vec<String>,
    pub need_fullpath: bool,
    pub block_extract: bool,
}

/// A borrowed software-rendered frame, tightly packed.
#[derive(Debug, Clone, Copy)]
pub struct VideoFrame<'a> {
    pub data: &'a [u8],
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
}

impl VideoFrame<'_> {
    pub fn pitch(&self) -> usize {
        self.width as usize * self.format.bytes_per_pixel()
    }
}

/// Paths and settings used when loading a core.
#[derive(Debug, Clone, Default)]
pub struct CoreConfig {
    /// BIOS and system files.
    pub system_dir: PathBuf,
    /// SRAM (`.srm`) files.
    pub save_dir: PathBuf,
    /// Core option values chosen by the user (`key`, `value`).
    pub option_overrides: Vec<(String, String)>,
}

/// A loaded libretro core.
///
/// Not `Send`: a core must stay on the thread that loaded it.
pub struct Core {
    api: CoreApi,
    shared: Box<Shared>,
    info: SystemInfo,
    save_dir: PathBuf,
    core_path: PathBuf,
    game_loaded: bool,
    sram_path: Option<PathBuf>,
    /// Content kept alive while the game runs (cores may reference it).
    _content: Option<(CString, Vec<u8>)>,
    _not_send: PhantomData<*const ()>,
    /// Must be the last field: dropped after everything that points into it.
    _library: Library,
}

impl Core {
    /// Loads the core library, registers callbacks and calls `retro_init`.
    pub fn load(path: &Path, config: CoreConfig) -> Result<Self, CoreError> {
        if CORE_LOADED.swap(true, Ordering::SeqCst) {
            return Err(CoreError::AlreadyLoaded);
        }
        Self::load_inner(path, config).inspect_err(|_| CORE_LOADED.store(false, Ordering::SeqCst))
    }

    fn load_inner(path: &Path, config: CoreConfig) -> Result<Self, CoreError> {
        let cpath = |p: &Path| {
            CString::new(p.to_string_lossy().as_bytes())
                .map_err(|_| CoreError::InvalidPath(p.into()))
        };
        let shared = Box::new(Shared::new(
            cpath(&config.system_dir)?,
            cpath(&config.save_dir)?,
            cpath(path)?,
            CoreOptions::with_overrides(config.option_overrides),
        ));

        // SAFETY: loading a library runs its initializers; libretro cores are
        // expected to be plain shared libraries without harmful side effects.
        let library = unsafe { Library::new(path) }.map_err(|source| CoreError::Library {
            path: path.into(),
            source,
        })?;
        // SAFETY: we treat the library as a libretro core (that is the contract of `load`).
        let api = unsafe { CoreApi::load(&library)? };

        // SAFETY: plain call with no arguments into the core.
        let version = unsafe { (api.api_version)() };
        if version != RETRO_API_VERSION {
            return Err(CoreError::ApiVersion(version));
        }

        callbacks::install_log_sink();

        let mut core = Self {
            api,
            shared,
            info: SystemInfo {
                library_name: String::new(),
                library_version: String::new(),
                valid_extensions: Vec::new(),
                need_fullpath: false,
                block_extract: false,
            },
            save_dir: config.save_dir,
            core_path: path.into(),
            game_loaded: false,
            sram_path: None,
            _content: None,
            _not_send: PhantomData,
            _library: library,
        };

        core.with_core(|api| {
            // SAFETY: callbacks have the exact libretro signatures; `retro_set_environment`
            // must come first because cores may call the environment from inside it.
            unsafe {
                (api.set_environment)(Some(callbacks::environment));
                (api.set_video_refresh)(Some(callbacks::video_refresh));
                (api.set_audio_sample)(Some(callbacks::audio_sample));
                (api.set_audio_sample_batch)(Some(callbacks::audio_sample_batch));
                (api.set_input_poll)(Some(callbacks::input_poll));
                (api.set_input_state)(Some(callbacks::input_state));
                (api.init)();
            }
        });
        core.info = core.read_system_info();
        tracing::info!(
            name = core.info.library_name,
            version = core.info.library_version,
            "core loaded"
        );
        Ok(core)
    }

    /// Calls into the core with the callback state made visible to callbacks.
    fn with_core<R>(&mut self, f: impl FnOnce(&CoreApi) -> R) -> R {
        let _guard = enter(&raw mut *self.shared);
        f(&self.api)
    }

    fn read_system_info(&mut self) -> SystemInfo {
        let mut raw = retro_system_info {
            library_name: std::ptr::null(),
            library_version: std::ptr::null(),
            valid_extensions: std::ptr::null(),
            need_fullpath: false,
            block_extract: false,
        };
        // SAFETY: `raw` is a valid, writable struct.
        self.with_core(|api| unsafe { (api.get_system_info)(&raw mut raw) });
        let text = |p: *const std::ffi::c_char| {
            if p.is_null() {
                String::new()
            } else {
                // SAFETY: the core returns static NUL-terminated strings.
                unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
            }
        };
        SystemInfo {
            library_name: text(raw.library_name),
            library_version: text(raw.library_version),
            valid_extensions: text(raw.valid_extensions)
                .split('|')
                .filter(|e| !e.is_empty())
                .map(str::to_lowercase)
                .collect(),
            need_fullpath: raw.need_fullpath,
            block_extract: raw.block_extract,
        }
    }

    pub fn system_info(&self) -> &SystemInfo {
        &self.info
    }

    /// Loads content. `None` starts the core without content, if it supports that.
    pub fn load_game(&mut self, content: Option<&Path>) -> Result<(), CoreError> {
        if self.game_loaded {
            return Err(CoreError::GameAlreadyLoaded);
        }

        let loaded = match content {
            None => {
                if !self.shared.support_no_game {
                    return Err(CoreError::ContentRequired);
                }
                // SAFETY: NULL game info is allowed when the core declared no-game support.
                self.with_core(|api| unsafe { (api.load_game)(std::ptr::null()) })
            }
            Some(path) => {
                let cpath = CString::new(path.to_string_lossy().as_bytes())
                    .map_err(|_| CoreError::InvalidPath(path.into()))?;
                let data = if self.info.need_fullpath {
                    Vec::new()
                } else {
                    fs::read(path).map_err(|source| CoreError::Io {
                        path: path.into(),
                        source,
                    })?
                };
                let info = retro_game_info {
                    path: cpath.as_ptr(),
                    data: if data.is_empty() {
                        std::ptr::null()
                    } else {
                        data.as_ptr().cast::<c_void>()
                    },
                    size: data.len(),
                    meta: std::ptr::null(),
                };
                // SAFETY: `info` and the buffers it points to are valid during the call
                // and kept alive afterwards in `self._content`.
                let ok = self.with_core(|api| unsafe { (api.load_game)(&raw const info) });
                self._content = Some((cpath, data));
                ok
            }
        };
        if !loaded {
            self._content = None;
            return Err(CoreError::LoadGameFailed);
        }
        self.game_loaded = true;

        let mut raw = retro_system_av_info::default();
        // SAFETY: `raw` is a valid, writable struct.
        self.with_core(|api| unsafe { (api.get_system_av_info)(&raw mut raw) });
        self.shared.av_info = AvInfo::from_raw(&raw);
        self.shared.av_changed = false;

        for port in 0..MAX_PORTS as c_uint {
            // SAFETY: plain integer arguments.
            self.with_core(|api| unsafe {
                (api.set_controller_port_device)(port, RETRO_DEVICE_JOYPAD)
            });
        }

        let stem = content
            .and_then(Path::file_stem)
            .or_else(|| self.core_path.file_stem())
            .map_or_else(|| "content".into(), |s| s.to_string_lossy().into_owned());
        self.sram_path = Some(self.save_dir.join(format!("{stem}.srm")));
        self.load_sram();

        tracing::info!(
            width = self.shared.av_info.base_width,
            height = self.shared.av_info.base_height,
            fps = self.shared.av_info.fps,
            sample_rate = self.shared.av_info.sample_rate,
            "content loaded"
        );
        Ok(())
    }

    /// Mutable view of the core's SRAM, if it has any.
    fn sram(&mut self) -> Option<&mut [u8]> {
        // SAFETY: plain integer argument.
        let size = self.with_core(|api| unsafe { (api.get_memory_size)(RETRO_MEMORY_SAVE_RAM) });
        // SAFETY: plain integer argument.
        let data = self.with_core(|api| unsafe { (api.get_memory_data)(RETRO_MEMORY_SAVE_RAM) });
        if size == 0 || data.is_null() {
            return None;
        }
        // SAFETY: the core owns `size` bytes at `data` while the game is loaded,
        // and we hold `&mut self`, so no core call runs while the slice lives.
        Some(unsafe { std::slice::from_raw_parts_mut(data.cast::<u8>(), size) })
    }

    fn load_sram(&mut self) {
        let Some(path) = self.sram_path.clone() else {
            return;
        };
        let Ok(saved) = fs::read(&path) else { return };
        if let Some(sram) = self.sram() {
            let n = saved.len().min(sram.len());
            sram[..n].copy_from_slice(&saved[..n]);
            tracing::info!(path = %path.display(), bytes = n, "SRAM loaded");
        }
    }

    /// Writes SRAM to the save directory.
    pub fn save_sram(&mut self) -> Result<(), CoreError> {
        let Some(path) = self.sram_path.clone() else {
            return Ok(());
        };
        let Some(data) = self.sram().map(|s| s.to_vec()) else {
            return Ok(());
        };
        let io_err = |source| CoreError::Io {
            path: path.clone(),
            source,
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(io_err)?;
        }
        fs::write(&path, &data).map_err(io_err)?;
        tracing::debug!(path = %path.display(), bytes = data.len(), "SRAM saved");
        Ok(())
    }

    /// Current video/audio timing.
    pub fn av_info(&self) -> AvInfo {
        self.shared.av_info
    }

    /// Returns whether the core changed its timing or geometry since the last call.
    pub fn take_av_changed(&mut self) -> bool {
        std::mem::take(&mut self.shared.av_changed)
    }

    /// Sets the state of a controller port for the next frames.
    pub fn set_pad(&mut self, port: usize, pad: PadState) {
        if let Some(slot) = self.shared.pads.get_mut(port) {
            *slot = pad;
        }
    }

    /// Display refresh rate reported to cores that ask for it.
    pub fn set_target_refresh_rate(&mut self, hz: f32) {
        self.shared.target_refresh_rate = hz;
    }

    /// Runs one frame of emulation. Video and audio are available afterwards.
    pub fn run_frame(&mut self) {
        if !self.game_loaded {
            return;
        }
        self.shared.audio.clear();
        // SAFETY: plain call; callbacks receive state through `with_core`.
        self.with_core(|api| unsafe { (api.run)() });
    }

    /// Last frame produced by the core, if any.
    pub fn frame(&self) -> Option<VideoFrame<'_>> {
        let s = &self.shared;
        (s.frame_width > 0 && s.frame_height > 0).then_some(VideoFrame {
            data: &s.frame,
            width: s.frame_width,
            height: s.frame_height,
            format: s.pixel_format,
        })
    }

    /// Interleaved stereo samples produced by the last [`Core::run_frame`].
    pub fn audio(&self) -> &[i16] {
        &self.shared.audio
    }

    /// True once the core asked to quit (`RETRO_ENVIRONMENT_SHUTDOWN`).
    pub fn shutdown_requested(&self) -> bool {
        self.shared.shutdown_requested
    }

    pub fn reset(&mut self) {
        // SAFETY: plain call.
        self.with_core(|api| unsafe { (api.reset)() });
    }

    pub fn options(&self) -> &CoreOptions {
        &self.shared.options
    }

    /// Changes a core option. Returns false if the key or value is unknown.
    pub fn set_option(&mut self, key: &str, value: &str) -> bool {
        self.shared.options.set(key, value)
    }

    /// Serializes the emulation state (save state).
    pub fn serialize(&mut self) -> Result<Vec<u8>, CoreError> {
        // SAFETY: plain call.
        let size = self.with_core(|api| unsafe { (api.serialize_size)() });
        if size == 0 {
            return Err(CoreError::SaveState);
        }
        let mut buf = vec![0u8; size];
        // SAFETY: `buf` has exactly `size` writable bytes.
        let ok = self.with_core(|api| unsafe { (api.serialize)(buf.as_mut_ptr().cast(), size) });
        ok.then_some(buf).ok_or(CoreError::SaveState)
    }

    /// Restores a state produced by [`Core::serialize`].
    pub fn unserialize(&mut self, state: &[u8]) -> Result<(), CoreError> {
        // SAFETY: `state` is valid for reading `state.len()` bytes.
        let ok =
            self.with_core(|api| unsafe { (api.unserialize)(state.as_ptr().cast(), state.len()) });
        ok.then_some(()).ok_or(CoreError::SaveState)
    }

    /// Saves the emulation state to a file (written atomically).
    pub fn save_state_to(&mut self, path: &Path) -> Result<(), CoreError> {
        let state = self.serialize()?;
        let io_err = |source| CoreError::Io {
            path: path.into(),
            source,
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(io_err)?;
        }
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        fs::write(&tmp, &state).map_err(io_err)?;
        fs::rename(&tmp, path).map_err(io_err)?;
        tracing::info!(path = %path.display(), bytes = state.len(), "state saved");
        Ok(())
    }

    /// Restores the emulation state from a file written by [`Core::save_state_to`].
    pub fn load_state_from(&mut self, path: &Path) -> Result<(), CoreError> {
        let state = fs::read(path).map_err(|source| CoreError::Io {
            path: path.into(),
            source,
        })?;
        self.unserialize(&state)?;
        tracing::info!(path = %path.display(), "state loaded");
        Ok(())
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        if self.game_loaded {
            if let Err(e) = self.save_sram() {
                tracing::error!("cannot save SRAM: {e}");
            }
            // SAFETY: plain call; a game is loaded.
            self.with_core(|api| unsafe { (api.unload_game)() });
        }
        // SAFETY: plain call; `retro_init` succeeded in `load`.
        self.with_core(|api| unsafe { (api.deinit)() });
        CORE_LOADED.store(false, Ordering::SeqCst);
        // `_library` is dropped after this, unloading the core.
    }
}
