//! Raw FFI bindings to the libretro API.
//!
//! Hand-written from `include/libretro.h` (the official header, vendored).
//! Names follow the C header exactly so they can be looked up there.
//! `tests/header_consistency.rs` checks every constant against the header.
//!
//! Only the subset Deckard needs is bound; add items here as phases need them.
//!
//! C enums are exposed as `c_int` aliases plus constants, never Rust enums:
//! a core may pass any integer, and an out-of-range Rust enum is undefined behavior.

#![no_std]
#![allow(non_camel_case_types)]

use core::ffi::{c_char, c_int, c_uint, c_void};

// ---------------------------------------------------------------------------
// API version
// ---------------------------------------------------------------------------

pub const RETRO_API_VERSION: c_uint = 1;

// ---------------------------------------------------------------------------
// Devices
// ---------------------------------------------------------------------------

pub const RETRO_DEVICE_TYPE_SHIFT: c_uint = 8;
pub const RETRO_DEVICE_MASK: c_uint = (1 << RETRO_DEVICE_TYPE_SHIFT) - 1;

/// Builds a device subclass id, like the `RETRO_DEVICE_SUBCLASS` macro.
pub const fn retro_device_subclass(base: c_uint, id: c_uint) -> c_uint {
    ((id + 1) << RETRO_DEVICE_TYPE_SHIFT) | base
}

pub const RETRO_DEVICE_NONE: c_uint = 0;
pub const RETRO_DEVICE_JOYPAD: c_uint = 1;
pub const RETRO_DEVICE_MOUSE: c_uint = 2;
pub const RETRO_DEVICE_KEYBOARD: c_uint = 3;
pub const RETRO_DEVICE_LIGHTGUN: c_uint = 4;
pub const RETRO_DEVICE_ANALOG: c_uint = 5;
pub const RETRO_DEVICE_POINTER: c_uint = 6;
pub const RETRO_DEVICE_TOUCHPAD: c_uint = 7;

pub const RETRO_DEVICE_ID_JOYPAD_B: c_uint = 0;
pub const RETRO_DEVICE_ID_JOYPAD_Y: c_uint = 1;
pub const RETRO_DEVICE_ID_JOYPAD_SELECT: c_uint = 2;
pub const RETRO_DEVICE_ID_JOYPAD_START: c_uint = 3;
pub const RETRO_DEVICE_ID_JOYPAD_UP: c_uint = 4;
pub const RETRO_DEVICE_ID_JOYPAD_DOWN: c_uint = 5;
pub const RETRO_DEVICE_ID_JOYPAD_LEFT: c_uint = 6;
pub const RETRO_DEVICE_ID_JOYPAD_RIGHT: c_uint = 7;
pub const RETRO_DEVICE_ID_JOYPAD_A: c_uint = 8;
pub const RETRO_DEVICE_ID_JOYPAD_X: c_uint = 9;
pub const RETRO_DEVICE_ID_JOYPAD_L: c_uint = 10;
pub const RETRO_DEVICE_ID_JOYPAD_R: c_uint = 11;
pub const RETRO_DEVICE_ID_JOYPAD_L2: c_uint = 12;
pub const RETRO_DEVICE_ID_JOYPAD_R2: c_uint = 13;
pub const RETRO_DEVICE_ID_JOYPAD_L3: c_uint = 14;
pub const RETRO_DEVICE_ID_JOYPAD_R3: c_uint = 15;
/// Used with `GET_INPUT_BITMASKS`: returns all joypad buttons as a bitmask.
pub const RETRO_DEVICE_ID_JOYPAD_MASK: c_uint = 256;

pub const RETRO_DEVICE_INDEX_ANALOG_LEFT: c_uint = 0;
pub const RETRO_DEVICE_INDEX_ANALOG_RIGHT: c_uint = 1;
pub const RETRO_DEVICE_INDEX_ANALOG_BUTTON: c_uint = 2;
pub const RETRO_DEVICE_ID_ANALOG_X: c_uint = 0;
pub const RETRO_DEVICE_ID_ANALOG_Y: c_uint = 1;

// ---------------------------------------------------------------------------
// Regions and memory
// ---------------------------------------------------------------------------

pub const RETRO_REGION_NTSC: c_uint = 0;
pub const RETRO_REGION_PAL: c_uint = 1;

pub const RETRO_MEMORY_MASK: c_uint = 0xff;
pub const RETRO_MEMORY_SAVE_RAM: c_uint = 0;
pub const RETRO_MEMORY_RTC: c_uint = 1;
pub const RETRO_MEMORY_SYSTEM_RAM: c_uint = 2;
pub const RETRO_MEMORY_VIDEO_RAM: c_uint = 3;
pub const RETRO_MEMORY_ROM: c_uint = 4;

// ---------------------------------------------------------------------------
// Environment commands
// ---------------------------------------------------------------------------

pub const RETRO_ENVIRONMENT_EXPERIMENTAL: c_uint = 0x10000;
pub const RETRO_ENVIRONMENT_PRIVATE: c_uint = 0x20000;

pub const RETRO_ENVIRONMENT_SET_ROTATION: c_uint = 1;
pub const RETRO_ENVIRONMENT_GET_OVERSCAN: c_uint = 2;
pub const RETRO_ENVIRONMENT_GET_CAN_DUPE: c_uint = 3;
pub const RETRO_ENVIRONMENT_SET_MESSAGE: c_uint = 6;
pub const RETRO_ENVIRONMENT_SHUTDOWN: c_uint = 7;
pub const RETRO_ENVIRONMENT_SET_PERFORMANCE_LEVEL: c_uint = 8;
pub const RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY: c_uint = 9;
pub const RETRO_ENVIRONMENT_SET_PIXEL_FORMAT: c_uint = 10;
pub const RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS: c_uint = 11;
pub const RETRO_ENVIRONMENT_SET_HW_RENDER: c_uint = 14;
pub const RETRO_ENVIRONMENT_GET_VARIABLE: c_uint = 15;
pub const RETRO_ENVIRONMENT_SET_VARIABLES: c_uint = 16;
pub const RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE: c_uint = 17;
pub const RETRO_ENVIRONMENT_SET_SUPPORT_NO_GAME: c_uint = 18;
pub const RETRO_ENVIRONMENT_GET_LIBRETRO_PATH: c_uint = 19;
pub const RETRO_ENVIRONMENT_GET_RUMBLE_INTERFACE: c_uint = 23;
pub const RETRO_ENVIRONMENT_GET_LOG_INTERFACE: c_uint = 27;
pub const RETRO_ENVIRONMENT_GET_CORE_ASSETS_DIRECTORY: c_uint = 30;
pub const RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY: c_uint = 31;
pub const RETRO_ENVIRONMENT_SET_SYSTEM_AV_INFO: c_uint = 32;
pub const RETRO_ENVIRONMENT_SET_CONTROLLER_INFO: c_uint = 35;
pub const RETRO_ENVIRONMENT_SET_GEOMETRY: c_uint = 37;
pub const RETRO_ENVIRONMENT_GET_LANGUAGE: c_uint = 39;
pub const RETRO_ENVIRONMENT_GET_AUDIO_VIDEO_ENABLE: c_uint = 47 | RETRO_ENVIRONMENT_EXPERIMENTAL;
pub const RETRO_ENVIRONMENT_GET_FASTFORWARDING: c_uint = 49 | RETRO_ENVIRONMENT_EXPERIMENTAL;
pub const RETRO_ENVIRONMENT_GET_TARGET_REFRESH_RATE: c_uint = 50 | RETRO_ENVIRONMENT_EXPERIMENTAL;
pub const RETRO_ENVIRONMENT_GET_INPUT_BITMASKS: c_uint = 51 | RETRO_ENVIRONMENT_EXPERIMENTAL;
pub const RETRO_ENVIRONMENT_GET_CORE_OPTIONS_VERSION: c_uint = 52;
pub const RETRO_ENVIRONMENT_GET_INPUT_MAX_USERS: c_uint = 61;
pub const RETRO_ENVIRONMENT_SET_CORE_OPTIONS_V2: c_uint = 67;
pub const RETRO_ENVIRONMENT_SET_VARIABLE: c_uint = 70;

// ---------------------------------------------------------------------------
// Enums (as c_int + constants)
// ---------------------------------------------------------------------------

pub type retro_pixel_format = c_int;
pub const RETRO_PIXEL_FORMAT_0RGB1555: retro_pixel_format = 0;
pub const RETRO_PIXEL_FORMAT_XRGB8888: retro_pixel_format = 1;
pub const RETRO_PIXEL_FORMAT_RGB565: retro_pixel_format = 2;
pub const RETRO_PIXEL_FORMAT_XRGB2101010: retro_pixel_format = 3;
pub const RETRO_PIXEL_FORMAT_HDR10_2101010: retro_pixel_format = 4;
pub const RETRO_PIXEL_FORMAT_SCRGB_RGBA16F: retro_pixel_format = 5;
pub const RETRO_PIXEL_FORMAT_UNKNOWN: retro_pixel_format = c_int::MAX;

pub type retro_log_level = c_int;
pub const RETRO_LOG_DEBUG: retro_log_level = 0;
pub const RETRO_LOG_INFO: retro_log_level = 1;
pub const RETRO_LOG_WARN: retro_log_level = 2;
pub const RETRO_LOG_ERROR: retro_log_level = 3;
pub const RETRO_LOG_DUMMY: retro_log_level = c_int::MAX;

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct retro_system_info {
    pub library_name: *const c_char,
    pub library_version: *const c_char,
    pub valid_extensions: *const c_char,
    pub need_fullpath: bool,
    pub block_extract: bool,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct retro_game_geometry {
    pub base_width: c_uint,
    pub base_height: c_uint,
    pub max_width: c_uint,
    pub max_height: c_uint,
    pub aspect_ratio: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct retro_system_timing {
    pub fps: f64,
    pub sample_rate: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct retro_system_av_info {
    pub geometry: retro_game_geometry,
    pub timing: retro_system_timing,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct retro_game_info {
    pub path: *const c_char,
    pub data: *const c_void,
    pub size: usize,
    pub meta: *const c_char,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct retro_variable {
    pub key: *const c_char,
    pub value: *const c_char,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct retro_input_descriptor {
    pub port: c_uint,
    pub device: c_uint,
    pub index: c_uint,
    pub id: c_uint,
    pub description: *const c_char,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct retro_message {
    pub msg: *const c_char,
    pub frames: c_uint,
}

/// Variadic printf-style log callback. Stable Rust cannot *define* variadic
/// functions, so the frontend implements this in a small C shim (phase 1).
pub type retro_log_printf_t =
    Option<unsafe extern "C" fn(level: retro_log_level, fmt: *const c_char, ...)>;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct retro_log_callback {
    pub log: retro_log_printf_t,
}

// ---------------------------------------------------------------------------
// Callbacks the frontend passes to the core
// ---------------------------------------------------------------------------

pub type retro_environment_t = Option<unsafe extern "C" fn(cmd: c_uint, data: *mut c_void) -> bool>;
pub type retro_video_refresh_t =
    Option<unsafe extern "C" fn(data: *const c_void, width: c_uint, height: c_uint, pitch: usize)>;
pub type retro_audio_sample_t = Option<unsafe extern "C" fn(left: i16, right: i16)>;
pub type retro_audio_sample_batch_t =
    Option<unsafe extern "C" fn(data: *const i16, frames: usize) -> usize>;
pub type retro_input_poll_t = Option<unsafe extern "C" fn()>;
pub type retro_input_state_t =
    Option<unsafe extern "C" fn(port: c_uint, device: c_uint, index: c_uint, id: c_uint) -> i16>;

// ---------------------------------------------------------------------------
// Functions exported by a core (signatures for dynamic loading)
// ---------------------------------------------------------------------------

pub type retro_set_environment_fn = unsafe extern "C" fn(cb: retro_environment_t);
pub type retro_set_video_refresh_fn = unsafe extern "C" fn(cb: retro_video_refresh_t);
pub type retro_set_audio_sample_fn = unsafe extern "C" fn(cb: retro_audio_sample_t);
pub type retro_set_audio_sample_batch_fn = unsafe extern "C" fn(cb: retro_audio_sample_batch_t);
pub type retro_set_input_poll_fn = unsafe extern "C" fn(cb: retro_input_poll_t);
pub type retro_set_input_state_fn = unsafe extern "C" fn(cb: retro_input_state_t);
pub type retro_init_fn = unsafe extern "C" fn();
pub type retro_deinit_fn = unsafe extern "C" fn();
pub type retro_api_version_fn = unsafe extern "C" fn() -> c_uint;
pub type retro_get_system_info_fn = unsafe extern "C" fn(info: *mut retro_system_info);
pub type retro_get_system_av_info_fn = unsafe extern "C" fn(info: *mut retro_system_av_info);
pub type retro_set_controller_port_device_fn = unsafe extern "C" fn(port: c_uint, device: c_uint);
pub type retro_reset_fn = unsafe extern "C" fn();
pub type retro_run_fn = unsafe extern "C" fn();
pub type retro_serialize_size_fn = unsafe extern "C" fn() -> usize;
pub type retro_serialize_fn = unsafe extern "C" fn(data: *mut c_void, len: usize) -> bool;
pub type retro_unserialize_fn = unsafe extern "C" fn(data: *const c_void, len: usize) -> bool;
pub type retro_cheat_reset_fn = unsafe extern "C" fn();
pub type retro_cheat_set_fn =
    unsafe extern "C" fn(index: c_uint, enabled: bool, code: *const c_char);
pub type retro_load_game_fn = unsafe extern "C" fn(game: *const retro_game_info) -> bool;
pub type retro_load_game_special_fn =
    unsafe extern "C" fn(game_type: c_uint, info: *const retro_game_info, num_info: usize) -> bool;
pub type retro_unload_game_fn = unsafe extern "C" fn();
pub type retro_get_region_fn = unsafe extern "C" fn() -> c_uint;
pub type retro_get_memory_data_fn = unsafe extern "C" fn(id: c_uint) -> *mut c_void;
pub type retro_get_memory_size_fn = unsafe extern "C" fn(id: c_uint) -> usize;

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, size_of};

    // Layouts must match the C ABI on the 64-bit targets we build for
    // (x86_64 Linux on the Deck, x86_64 Windows for development).
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn struct_layouts_match_c_abi() {
        assert_eq!(size_of::<retro_system_info>(), 32);
        assert_eq!(size_of::<retro_game_geometry>(), 20);
        assert_eq!(size_of::<retro_system_timing>(), 16);
        assert_eq!(size_of::<retro_system_av_info>(), 40);
        assert_eq!(align_of::<retro_system_av_info>(), 8);
        assert_eq!(size_of::<retro_game_info>(), 32);
        assert_eq!(size_of::<retro_variable>(), 16);
        assert_eq!(size_of::<retro_input_descriptor>(), 24);
        assert_eq!(size_of::<retro_message>(), 16);
        assert_eq!(size_of::<retro_log_callback>(), 8);
    }

    #[test]
    fn device_subclass_matches_macro() {
        // RETRO_DEVICE_SUBCLASS(RETRO_DEVICE_JOYPAD, 0) == (1 << 8) | 1
        assert_eq!(retro_device_subclass(RETRO_DEVICE_JOYPAD, 0), 0x101);
        assert_eq!(RETRO_DEVICE_MASK, 0xff);
    }
}
