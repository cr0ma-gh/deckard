//! Callbacks handed to the core, and the state they read and write.
//!
//! libretro callbacks carry no context pointer, so the state lives behind a
//! thread-local pointer that [`enter`] sets for the duration of each call into
//! the core. Every callback catches panics: unwinding into C is undefined behavior.

use std::cell::Cell;
use std::collections::HashSet;
use std::ffi::{CStr, CString, c_char, c_int, c_uint, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use libretro_sys::*;

use crate::input::{MAX_PORTS, PadState};
use crate::options::CoreOptions;
use crate::{AvInfo, PixelFormat};

/// State shared between [`crate::Core`] and the callbacks.
pub(crate) struct Shared {
    pub system_dir: CString,
    pub save_dir: CString,
    pub libretro_path: CString,
    pub pixel_format: PixelFormat,
    pub options: CoreOptions,
    pub av_info: AvInfo,
    pub av_changed: bool,
    pub support_no_game: bool,
    pub shutdown_requested: bool,
    pub target_refresh_rate: f32,
    pub disk_control: Option<crate::disk::DiskControl>,
    pub pads: [PadState; MAX_PORTS],
    /// Last frame, tightly packed (`width * bytes_per_pixel` per row).
    pub frame: Vec<u8>,
    pub frame_width: u32,
    pub frame_height: u32,
    /// Interleaved stereo samples produced during the current `retro_run`.
    pub audio: Vec<i16>,
    unknown_commands: HashSet<c_uint>,
}

impl Shared {
    pub fn new(
        system_dir: CString,
        save_dir: CString,
        libretro_path: CString,
        options: CoreOptions,
    ) -> Self {
        Self {
            system_dir,
            save_dir,
            libretro_path,
            pixel_format: PixelFormat::Rgb1555,
            options,
            av_info: AvInfo::default(),
            av_changed: false,
            support_no_game: false,
            shutdown_requested: false,
            target_refresh_rate: 60.0,
            disk_control: None,
            pads: [PadState::default(); MAX_PORTS],
            frame: Vec::new(),
            frame_width: 0,
            frame_height: 0,
            audio: Vec::with_capacity(4096),
            unknown_commands: HashSet::new(),
        }
    }
}

thread_local! {
    static CURRENT: Cell<*mut Shared> = const { Cell::new(ptr::null_mut()) };
}

/// Restores the previous thread-local pointer when dropped.
pub(crate) struct EnterGuard {
    previous: *mut Shared,
}

impl Drop for EnterGuard {
    fn drop(&mut self) {
        CURRENT.with(|c| c.set(self.previous));
    }
}

/// Makes `shared` visible to callbacks until the guard is dropped.
///
/// The caller must not touch `shared` through any other path while the guard lives.
pub(crate) fn enter(shared: *mut Shared) -> EnterGuard {
    let previous = CURRENT.with(|c| c.replace(shared));
    EnterGuard { previous }
}

/// Runs `f` on the current shared state, returning `default` if there is none
/// or if `f` panics.
fn with_shared<R>(default: R, f: impl FnOnce(&mut Shared) -> R) -> R {
    let ptr = CURRENT.with(Cell::get);
    if ptr.is_null() {
        tracing::warn!("libretro callback called outside of a core call");
        return default;
    }
    // SAFETY: `enter` set this pointer from a live `Box<Shared>`, and the owner does
    // not access it while the core call (and thus this callback) is in progress.
    let shared = unsafe { &mut *ptr };
    match catch_unwind(AssertUnwindSafe(|| f(shared))) {
        Ok(r) => r,
        Err(_) => {
            tracing::error!("panic inside a libretro callback");
            default
        }
    }
}

// ---------------------------------------------------------------------------
// Log interface (formatting happens in log_shim.c)
// ---------------------------------------------------------------------------

unsafe extern "C" {
    fn deckard_log_set_sink(sink: Option<unsafe extern "C" fn(level: c_int, msg: *const c_char)>);
    fn deckard_log_printf(level: c_int, fmt: *const c_char, ...);
}

unsafe extern "C" fn log_sink(level: c_int, msg: *const c_char) {
    let _ = catch_unwind(|| {
        if msg.is_null() {
            return;
        }
        // SAFETY: the shim always passes a NUL-terminated buffer.
        let text = unsafe { CStr::from_ptr(msg) }.to_string_lossy();
        let text = text.trim_end();
        match level {
            RETRO_LOG_DEBUG => tracing::debug!(target: "core", "{text}"),
            RETRO_LOG_INFO => tracing::info!(target: "core", "{text}"),
            RETRO_LOG_WARN => tracing::warn!(target: "core", "{text}"),
            _ => tracing::error!(target: "core", "{text}"),
        }
    });
}

pub(crate) fn install_log_sink() {
    // SAFETY: stores a function pointer; `log_sink` matches the C signature.
    unsafe { deckard_log_set_sink(Some(log_sink)) };
}

// ---------------------------------------------------------------------------
// Video / audio / input callbacks
// ---------------------------------------------------------------------------

/// Value of `RETRO_HW_FRAME_BUFFER_VALID`: a hardware-rendered frame.
const HW_FRAME_BUFFER_VALID: *const c_void = usize::MAX as *const c_void;

pub(crate) unsafe extern "C" fn video_refresh(
    data: *const c_void,
    width: c_uint,
    height: c_uint,
    pitch: usize,
) {
    with_shared((), |s| {
        if data.is_null() {
            return; // Frame dupe: keep the previous frame.
        }
        if data == HW_FRAME_BUFFER_VALID {
            tracing::warn!("hardware-rendered frame received, not supported yet");
            return;
        }
        let row_bytes = width as usize * s.pixel_format.bytes_per_pixel();
        if pitch < row_bytes {
            tracing::warn!(width, pitch, "invalid frame pitch");
            return;
        }
        s.frame.clear();
        s.frame.reserve(row_bytes * height as usize);
        for row in 0..height as usize {
            // SAFETY: the core guarantees `height` rows of `pitch` bytes at `data`,
            // and `row_bytes <= pitch`.
            let line = unsafe {
                std::slice::from_raw_parts(data.cast::<u8>().add(row * pitch), row_bytes)
            };
            s.frame.extend_from_slice(line);
        }
        s.frame_width = width;
        s.frame_height = height;
    });
}

pub(crate) unsafe extern "C" fn audio_sample(left: i16, right: i16) {
    with_shared((), |s| s.audio.extend_from_slice(&[left, right]));
}

pub(crate) unsafe extern "C" fn audio_sample_batch(data: *const i16, frames: usize) -> usize {
    with_shared(0, |s| {
        if data.is_null() {
            return 0;
        }
        // SAFETY: the core guarantees `frames` interleaved stereo frames at `data`.
        let samples = unsafe { std::slice::from_raw_parts(data, frames * 2) };
        s.audio.extend_from_slice(samples);
        frames
    })
}

pub(crate) unsafe extern "C" fn input_poll() {
    // Input is sampled by the frontend right before `retro_run`.
}

pub(crate) unsafe extern "C" fn input_state(
    port: c_uint,
    device: c_uint,
    index: c_uint,
    id: c_uint,
) -> i16 {
    with_shared(0, |s| match s.pads.get(port as usize) {
        Some(pad) => pad.query(device, index, id),
        None => 0,
    })
}

// ---------------------------------------------------------------------------
// Environment
// ---------------------------------------------------------------------------

/// Writes `value` to `data` as `T`. Returns false for a NULL pointer.
///
/// # Safety
/// `data` must be NULL or valid for writing a `T`.
unsafe fn write<T>(data: *mut c_void, value: T) -> bool {
    if data.is_null() {
        return false;
    }
    // SAFETY: non-null and valid per the caller's contract.
    unsafe { data.cast::<T>().write(value) };
    true
}

/// Reads a `T` from `data`. Returns None for a NULL pointer.
///
/// # Safety
/// `data` must be NULL or valid for reading a `T`.
unsafe fn read<T: Copy>(data: *mut c_void) -> Option<T> {
    // SAFETY: valid per the caller's contract when non-null.
    (!data.is_null()).then(|| unsafe { data.cast::<T>().read() })
}

/// # Safety
/// `ptr` must be NULL or a valid C string.
unsafe fn str_from(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    // SAFETY: non-null and valid per the caller's contract.
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

pub(crate) unsafe extern "C" fn environment(cmd: c_uint, data: *mut c_void) -> bool {
    // SAFETY: each arm reads/writes `data` as the type libretro.h documents for `cmd`.
    with_shared(false, |s| unsafe { handle_environment(s, cmd, data) })
}

/// # Safety
/// `data` must match the type libretro.h documents for `cmd` (the core's contract).
unsafe fn handle_environment(s: &mut Shared, cmd: c_uint, data: *mut c_void) -> bool {
    // SAFETY: the core passes `data` with the type libretro.h documents for `cmd`.
    unsafe {
        match cmd {
            RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY
            | RETRO_ENVIRONMENT_GET_CORE_ASSETS_DIRECTORY => write(data, s.system_dir.as_ptr()),
            RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY => write(data, s.save_dir.as_ptr()),
            RETRO_ENVIRONMENT_GET_LIBRETRO_PATH => write(data, s.libretro_path.as_ptr()),

            RETRO_ENVIRONMENT_SET_PIXEL_FORMAT => {
                match read::<retro_pixel_format>(data).and_then(PixelFormat::from_raw) {
                    Some(format) => {
                        s.pixel_format = format;
                        true
                    }
                    None => false,
                }
            }

            RETRO_ENVIRONMENT_GET_LOG_INTERFACE => write(
                data,
                retro_log_callback {
                    log: Some(deckard_log_printf),
                },
            ),

            RETRO_ENVIRONMENT_GET_CAN_DUPE => write(data, true),
            RETRO_ENVIRONMENT_GET_OVERSCAN => write(data, false),
            RETRO_ENVIRONMENT_GET_FASTFORWARDING => write(data, false),
            RETRO_ENVIRONMENT_GET_LANGUAGE => write(data, RETRO_LANGUAGE_ENGLISH),
            RETRO_ENVIRONMENT_GET_INPUT_MAX_USERS => write(data, MAX_PORTS as c_uint),
            RETRO_ENVIRONMENT_GET_TARGET_REFRESH_RATE => write(data, s.target_refresh_rate),
            // Bit 0: video enabled, bit 1: audio enabled.
            RETRO_ENVIRONMENT_GET_AUDIO_VIDEO_ENABLE => write(data, 0b11 as c_int),
            RETRO_ENVIRONMENT_GET_INPUT_BITMASKS => true,

            RETRO_ENVIRONMENT_SET_SUPPORT_NO_GAME => {
                s.support_no_game = read::<bool>(data).unwrap_or(false);
                true
            }
            RETRO_ENVIRONMENT_SET_GEOMETRY => match read::<retro_game_geometry>(data) {
                Some(g) => {
                    s.av_info.set_geometry(&g);
                    s.av_changed = true;
                    true
                }
                None => false,
            },
            RETRO_ENVIRONMENT_SET_SYSTEM_AV_INFO => match read::<retro_system_av_info>(data) {
                Some(info) => {
                    s.av_info = AvInfo::from_raw(&info);
                    s.av_changed = true;
                    true
                }
                None => false,
            },
            RETRO_ENVIRONMENT_SHUTDOWN => {
                s.shutdown_requested = true;
                true
            }
            RETRO_ENVIRONMENT_SET_MESSAGE => {
                if let Some(msg) = read::<retro_message>(data) {
                    tracing::info!(target: "core", "message: {}", str_from(msg.msg));
                }
                true
            }

            // Core options.
            RETRO_ENVIRONMENT_GET_CORE_OPTIONS_VERSION => write(data, 2 as c_uint),
            RETRO_ENVIRONMENT_SET_VARIABLES if !data.is_null() => {
                s.options.set_variables(data.cast());
                true
            }
            RETRO_ENVIRONMENT_SET_CORE_OPTIONS if !data.is_null() => {
                s.options.set_v1(data.cast());
                true
            }
            RETRO_ENVIRONMENT_SET_CORE_OPTIONS_INTL => {
                match read::<retro_core_options_intl>(data) {
                    Some(intl) if !intl.us.is_null() => {
                        s.options.set_v1(intl.us);
                        true
                    }
                    _ => false,
                }
            }
            RETRO_ENVIRONMENT_SET_CORE_OPTIONS_V2 => match read::<retro_core_options_v2>(data) {
                Some(opts) if !opts.definitions.is_null() => {
                    s.options.set_v2(opts.definitions);
                    true
                }
                _ => false,
            },
            RETRO_ENVIRONMENT_SET_CORE_OPTIONS_V2_INTL => {
                match read::<retro_core_options_v2_intl>(data).and_then(|i| i.us.as_ref().copied())
                {
                    Some(opts) if !opts.definitions.is_null() => {
                        s.options.set_v2(opts.definitions);
                        true
                    }
                    _ => false,
                }
            }
            RETRO_ENVIRONMENT_GET_VARIABLE => {
                let Some(mut var) = read::<retro_variable>(data) else {
                    return false;
                };
                let key = str_from(var.key);
                var.value = s.options.value_ptr(&key).unwrap_or(ptr::null());
                write(data, var) && !var.value.is_null()
            }
            RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE => write(data, s.options.take_updated()),
            RETRO_ENVIRONMENT_SET_VARIABLE => match read::<retro_variable>(data) {
                Some(var) if !var.key.is_null() && !var.value.is_null() => {
                    s.options.set(&str_from(var.key), &str_from(var.value))
                }
                // A NULL `data` asks whether the command is supported.
                None => true,
                _ => false,
            },
            RETRO_ENVIRONMENT_SET_CORE_OPTIONS_DISPLAY => true,

            RETRO_ENVIRONMENT_SET_DISK_CONTROL_INTERFACE => {
                if let Some(cb) = read::<retro_disk_control_callback>(data) {
                    if let (
                        Some(set_eject),
                        Some(get_eject),
                        Some(get_idx),
                        Some(set_idx),
                        Some(get_num),
                    ) = (
                        cb.set_eject_state,
                        cb.get_eject_state,
                        cb.get_image_index,
                        cb.set_image_index,
                        cb.get_num_images,
                    ) {
                        s.disk_control = Some(crate::disk::DiskControl {
                            set_eject_state: set_eject,
                            get_eject_state: get_eject,
                            get_image_index: get_idx,
                            set_image_index: set_idx,
                            get_num_images: get_num,
                            get_image_label: None,
                        });
                        tracing::info!("core registered disk control interface");
                        true
                    } else {
                        false
                    }
                } else {
                    s.disk_control = None;
                    true
                }
            }

            // Version 1: the extended interface (with disk labels) is supported.
            // Cores ask this before choosing which interface to register.
            RETRO_ENVIRONMENT_GET_DISK_CONTROL_INTERFACE_VERSION => write(data, 1 as c_uint),

            RETRO_ENVIRONMENT_SET_DISK_CONTROL_EXT_INTERFACE => {
                if let Some(cb) = read::<retro_disk_control_ext_callback>(data) {
                    if let (
                        Some(set_eject),
                        Some(get_eject),
                        Some(get_idx),
                        Some(set_idx),
                        Some(get_num),
                    ) = (
                        cb.set_eject_state,
                        cb.get_eject_state,
                        cb.get_image_index,
                        cb.set_image_index,
                        cb.get_num_images,
                    ) {
                        s.disk_control = Some(crate::disk::DiskControl {
                            set_eject_state: set_eject,
                            get_eject_state: get_eject,
                            get_image_index: get_idx,
                            set_image_index: set_idx,
                            get_num_images: get_num,
                            get_image_label: cb.get_image_label,
                        });
                        tracing::info!("core registered extended disk control interface");
                        true
                    } else {
                        false
                    }
                } else {
                    s.disk_control = None;
                    true
                }
            }

            // Accepted and ignored: informational for now.
            RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS
            | RETRO_ENVIRONMENT_SET_CONTROLLER_INFO
            | RETRO_ENVIRONMENT_SET_SUBSYSTEM_INFO
            | RETRO_ENVIRONMENT_SET_MEMORY_MAPS
            | RETRO_ENVIRONMENT_SET_SUPPORT_ACHIEVEMENTS
            | RETRO_ENVIRONMENT_SET_SERIALIZATION_QUIRKS
            | RETRO_ENVIRONMENT_SET_PERFORMANCE_LEVEL
            | RETRO_ENVIRONMENT_SET_MINIMUM_AUDIO_LATENCY => true,

            // Not supported yet (hardware rendering lands in phase 5).
            RETRO_ENVIRONMENT_SET_HW_RENDER
            | RETRO_ENVIRONMENT_GET_PREFERRED_HW_RENDER
            | RETRO_ENVIRONMENT_SET_ROTATION
            | RETRO_ENVIRONMENT_GET_RUMBLE_INTERFACE
            | RETRO_ENVIRONMENT_GET_USERNAME
            | RETRO_ENVIRONMENT_GET_MESSAGE_INTERFACE_VERSION => false,

            _ => {
                if s.unknown_commands.insert(cmd) {
                    tracing::debug!(cmd, "unsupported environment command");
                }
                false
            }
        }
    }
}
