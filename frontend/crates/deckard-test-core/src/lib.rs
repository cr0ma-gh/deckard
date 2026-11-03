//! Minimal libretro core used to test the Deckard frontend.
//!
//! Behavior, chosen so each frontend feature has something observable:
//! - video: 320x240 XRGB8888 scrolling gradient with a white square;
//! - input: the D-pad moves the square, holding A plays a 440 Hz tone;
//! - core options: `deckard_test_speed` (1/2/4) sets the scroll speed;
//! - SRAM: 16 bytes holding a magic value and a frame counter;
//! - save states: square position and frame counter;
//! - log interface: logs a line on init.

#![allow(clippy::missing_safety_doc)]

use std::ffi::{CStr, c_char, c_uint, c_void};
use std::ptr;
use std::sync::Mutex;

use libretro_sys::*;

pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 240;
pub const FPS: f64 = 60.0;
pub const SAMPLE_RATE: f64 = 44_100.0;
const SQUARE: usize = 24;
const SRAM_MAGIC: u64 = 0xDEC4_A2D0_0000_0001;
const SRAM_SIZE: usize = 16;
const STATE_SIZE: usize = 24;
const SPEED_KEY: &CStr = c"deckard_test_speed";

struct Callbacks {
    environment: retro_environment_t,
    video: retro_video_refresh_t,
    audio_batch: retro_audio_sample_batch_t,
    input_poll: retro_input_poll_t,
    input_state: retro_input_state_t,
}

static CALLBACKS: Mutex<Callbacks> = Mutex::new(Callbacks {
    environment: None,
    video: None,
    audio_batch: None,
    input_poll: None,
    input_state: None,
});

struct State {
    frame: Vec<u32>,
    audio: Vec<i16>,
    sram: Box<[u8; SRAM_SIZE]>,
    x: i32,
    y: i32,
    phase: f64,
    speed: u64,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn callbacks() -> std::sync::MutexGuard<'static, Callbacks> {
    CALLBACKS.lock().unwrap_or_else(|e| e.into_inner())
}

fn state() -> std::sync::MutexGuard<'static, Option<State>> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Calls the frontend environment callback.
fn env(cmd: c_uint, data: *mut c_void) -> bool {
    let cb = callbacks().environment;
    match cb {
        // SAFETY: the frontend gave us this callback; `data` matches what `cmd` expects.
        Some(f) => unsafe { f(cmd, data) },
        None => false,
    }
}

fn frame_counter(sram: &[u8; SRAM_SIZE]) -> u64 {
    let mut bytes = [0; 8];
    bytes.copy_from_slice(&sram[8..16]);
    u64::from_le_bytes(bytes)
}

fn set_frame_counter(sram: &mut [u8; SRAM_SIZE], value: u64) {
    sram[..8].copy_from_slice(&SRAM_MAGIC.to_le_bytes());
    sram[8..].copy_from_slice(&value.to_le_bytes());
}

#[unsafe(no_mangle)]
pub extern "C" fn retro_api_version() -> c_uint {
    RETRO_API_VERSION
}

#[unsafe(no_mangle)]
pub extern "C" fn retro_set_environment(cb: retro_environment_t) {
    callbacks().environment = cb;

    let mut no_game = true;
    env(
        RETRO_ENVIRONMENT_SET_SUPPORT_NO_GAME,
        (&raw mut no_game).cast(),
    );

    let mut version: c_uint = 0;
    let has_v2 = env(
        RETRO_ENVIRONMENT_GET_CORE_OPTIONS_VERSION,
        (&raw mut version).cast(),
    ) && version >= 2;

    if has_v2 {
        let empty = retro_core_option_value {
            value: ptr::null(),
            label: ptr::null(),
        };
        let mut values = [empty; RETRO_NUM_CORE_OPTION_VALUES_MAX];
        values[0] = retro_core_option_value {
            value: c"1".as_ptr(),
            label: c"Slow".as_ptr(),
        };
        values[1] = retro_core_option_value {
            value: c"2".as_ptr(),
            label: ptr::null(),
        };
        values[2] = retro_core_option_value {
            value: c"4".as_ptr(),
            label: c"Fast".as_ptr(),
        };
        let definitions = [
            retro_core_option_v2_definition {
                key: SPEED_KEY.as_ptr(),
                desc: c"Scroll speed".as_ptr(),
                desc_categorized: ptr::null(),
                info: c"How fast the background scrolls.".as_ptr(),
                info_categorized: ptr::null(),
                category_key: ptr::null(),
                values,
                default_value: c"1".as_ptr(),
            },
            retro_core_option_v2_definition {
                key: ptr::null(),
                desc: ptr::null(),
                desc_categorized: ptr::null(),
                info: ptr::null(),
                info_categorized: ptr::null(),
                category_key: ptr::null(),
                values: [empty; RETRO_NUM_CORE_OPTION_VALUES_MAX],
                default_value: ptr::null(),
            },
        ];
        let categories = [retro_core_option_v2_category {
            key: ptr::null(),
            desc: ptr::null(),
            info: ptr::null(),
        }];
        let mut options = retro_core_options_v2 {
            categories: categories.as_ptr(),
            definitions: definitions.as_ptr(),
        };
        env(
            RETRO_ENVIRONMENT_SET_CORE_OPTIONS_V2,
            (&raw mut options).cast(),
        );
    } else {
        let mut vars = [
            retro_variable {
                key: SPEED_KEY.as_ptr(),
                value: c"Scroll speed; 1|2|4".as_ptr(),
            },
            retro_variable {
                key: ptr::null(),
                value: ptr::null(),
            },
        ];
        env(RETRO_ENVIRONMENT_SET_VARIABLES, vars.as_mut_ptr().cast());
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn retro_set_video_refresh(cb: retro_video_refresh_t) {
    callbacks().video = cb;
}

#[unsafe(no_mangle)]
pub extern "C" fn retro_set_audio_sample(_cb: retro_audio_sample_t) {}

#[unsafe(no_mangle)]
pub extern "C" fn retro_set_audio_sample_batch(cb: retro_audio_sample_batch_t) {
    callbacks().audio_batch = cb;
}

#[unsafe(no_mangle)]
pub extern "C" fn retro_set_input_poll(cb: retro_input_poll_t) {
    callbacks().input_poll = cb;
}

#[unsafe(no_mangle)]
pub extern "C" fn retro_set_input_state(cb: retro_input_state_t) {
    callbacks().input_state = cb;
}

#[unsafe(no_mangle)]
pub extern "C" fn retro_init() {
    let mut log_cb = retro_log_callback { log: None };
    let has_log = env(
        RETRO_ENVIRONMENT_GET_LOG_INTERFACE,
        (&raw mut log_cb).cast(),
    );
    if let (true, Some(log)) = (has_log, log_cb.log) {
        // SAFETY: printf-style call; "%s" consumes exactly one C string argument.
        unsafe {
            log(
                RETRO_LOG_INFO,
                c"%s\n".as_ptr(),
                c"test core initialized".as_ptr(),
            )
        };
    }

    let mut sram = Box::new([0; SRAM_SIZE]);
    set_frame_counter(&mut sram, 0);
    *state() = Some(State {
        frame: vec![0; WIDTH * HEIGHT],
        audio: Vec::with_capacity(2 * 1024),
        sram,
        x: (WIDTH / 2) as i32,
        y: (HEIGHT / 2) as i32,
        phase: 0.0,
        speed: 1,
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn retro_deinit() {
    *state() = None;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn retro_get_system_info(info: *mut retro_system_info) {
    // SAFETY: the frontend passes a valid, writable struct.
    unsafe {
        *info = retro_system_info {
            library_name: c"Deckard Test Core".as_ptr(),
            library_version: c"0.1.0".as_ptr(),
            valid_extensions: c"bin|txt".as_ptr(),
            need_fullpath: false,
            block_extract: false,
        };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn retro_get_system_av_info(info: *mut retro_system_av_info) {
    // SAFETY: the frontend passes a valid, writable struct.
    unsafe {
        *info = retro_system_av_info {
            geometry: retro_game_geometry {
                base_width: WIDTH as c_uint,
                base_height: HEIGHT as c_uint,
                max_width: WIDTH as c_uint,
                max_height: HEIGHT as c_uint,
                aspect_ratio: 4.0 / 3.0,
            },
            timing: retro_system_timing {
                fps: FPS,
                sample_rate: SAMPLE_RATE,
            },
        };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn retro_set_controller_port_device(_port: c_uint, _device: c_uint) {}

#[unsafe(no_mangle)]
pub extern "C" fn retro_reset() {
    if let Some(s) = state().as_mut() {
        s.x = (WIDTH / 2) as i32;
        s.y = (HEIGHT / 2) as i32;
    }
}

struct DiscState {
    ejected: bool,
    index: c_uint,
}

static DISCS: Mutex<DiscState> = Mutex::new(DiscState {
    ejected: false,
    index: 0,
});

unsafe extern "C" fn disk_set_eject_state(ejected: bool) -> bool {
    let mut d = DISCS.lock().unwrap_or_else(|e| e.into_inner());
    d.ejected = ejected;
    true
}

unsafe extern "C" fn disk_get_eject_state() -> bool {
    let d = DISCS.lock().unwrap_or_else(|e| e.into_inner());
    d.ejected
}

unsafe extern "C" fn disk_get_image_index() -> c_uint {
    let d = DISCS.lock().unwrap_or_else(|e| e.into_inner());
    d.index
}

unsafe extern "C" fn disk_set_image_index(index: c_uint) -> bool {
    let mut d = DISCS.lock().unwrap_or_else(|e| e.into_inner());
    if d.ejected && index < 2 {
        d.index = index;
        true
    } else {
        false
    }
}

unsafe extern "C" fn disk_get_num_images() -> c_uint {
    2
}

unsafe extern "C" fn disk_replace_image_index(
    _index: c_uint,
    _info: *const retro_game_info,
) -> bool {
    false
}

unsafe extern "C" fn disk_add_image_index() -> bool {
    false
}

unsafe extern "C" fn disk_get_image_label(index: c_uint, s: *mut c_char, len: usize) -> bool {
    let label = match index {
        0 => c"Disc 1",
        1 => c"Disc 2",
        _ => return false,
    };
    let bytes = label.to_bytes_with_nul();
    if len < bytes.len() {
        return false;
    }
    // SAFETY: caller provided a writable buffer of length `len`.
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr().cast::<c_char>(), s, bytes.len());
    }
    true
}

#[unsafe(no_mangle)]
pub extern "C" fn retro_load_game(_game: *const retro_game_info) -> bool {
    let mut format = RETRO_PIXEL_FORMAT_XRGB8888;
    let _ = env(RETRO_ENVIRONMENT_SET_PIXEL_FORMAT, (&raw mut format).cast());

    // Like real cores: ask the frontend which disk interface it supports and
    // register the extended one (with labels) only if it says version >= 1.
    let mut version: c_uint = 0;
    let has_ext = env(
        RETRO_ENVIRONMENT_GET_DISK_CONTROL_INTERFACE_VERSION,
        (&raw mut version).cast(),
    ) && version >= 1;
    if has_ext {
        let disk_cb = retro_disk_control_ext_callback {
            set_eject_state: Some(disk_set_eject_state),
            get_eject_state: Some(disk_get_eject_state),
            get_image_index: Some(disk_get_image_index),
            set_image_index: Some(disk_set_image_index),
            get_num_images: Some(disk_get_num_images),
            replace_image_index: Some(disk_replace_image_index),
            add_image_index: Some(disk_add_image_index),
            set_initial_image: None,
            get_image_path: None,
            get_image_label: Some(disk_get_image_label),
        };
        env(
            RETRO_ENVIRONMENT_SET_DISK_CONTROL_EXT_INTERFACE,
            (&raw const disk_cb).cast_mut().cast(),
        );
    } else {
        let disk_cb = retro_disk_control_callback {
            set_eject_state: Some(disk_set_eject_state),
            get_eject_state: Some(disk_get_eject_state),
            get_image_index: Some(disk_get_image_index),
            set_image_index: Some(disk_set_image_index),
            get_num_images: Some(disk_get_num_images),
            replace_image_index: Some(disk_replace_image_index),
            add_image_index: Some(disk_add_image_index),
        };
        env(
            RETRO_ENVIRONMENT_SET_DISK_CONTROL_INTERFACE,
            (&raw const disk_cb).cast_mut().cast(),
        );
    }
    true
}

#[unsafe(no_mangle)]
pub extern "C" fn retro_load_game_special(
    _game_type: c_uint,
    _info: *const retro_game_info,
    _num_info: usize,
) -> bool {
    false
}

#[unsafe(no_mangle)]
pub extern "C" fn retro_unload_game() {}

#[unsafe(no_mangle)]
pub extern "C" fn retro_get_region() -> c_uint {
    RETRO_REGION_NTSC
}

fn read_speed_option() -> Option<u64> {
    let mut var = retro_variable {
        key: SPEED_KEY.as_ptr(),
        value: ptr::null(),
    };
    if !env(RETRO_ENVIRONMENT_GET_VARIABLE, (&raw mut var).cast()) || var.value.is_null() {
        return None;
    }
    // SAFETY: the frontend returned a valid NUL-terminated string.
    let value = unsafe { CStr::from_ptr(var.value) };
    value.to_str().ok()?.parse().ok()
}

#[unsafe(no_mangle)]
pub extern "C" fn retro_run() {
    let (video, audio_batch, input_poll, input_state) = {
        let cb = callbacks();
        (cb.video, cb.audio_batch, cb.input_poll, cb.input_state)
    };

    let mut updated = false;
    env(
        RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE,
        (&raw mut updated).cast(),
    );
    let new_speed = if updated { read_speed_option() } else { None };

    if let Some(poll) = input_poll {
        // SAFETY: frontend callback with no arguments.
        unsafe { poll() };
    }
    let pressed = |id: c_uint| match input_state {
        // SAFETY: frontend callback; arguments are plain integers.
        Some(f) => unsafe { f(0, RETRO_DEVICE_JOYPAD, 0, id) != 0 },
        None => false,
    };

    let mut guard = state();
    let Some(s) = guard.as_mut() else { return };
    if let Some(speed) = new_speed {
        s.speed = speed;
    }

    let half = (SQUARE / 2) as i32;
    if pressed(RETRO_DEVICE_ID_JOYPAD_LEFT) {
        s.x -= 2;
    }
    if pressed(RETRO_DEVICE_ID_JOYPAD_RIGHT) {
        s.x += 2;
    }
    if pressed(RETRO_DEVICE_ID_JOYPAD_UP) {
        s.y -= 2;
    }
    if pressed(RETRO_DEVICE_ID_JOYPAD_DOWN) {
        s.y += 2;
    }
    s.x = s.x.clamp(half, WIDTH as i32 - half);
    s.y = s.y.clamp(half, HEIGHT as i32 - half);

    let count = frame_counter(&s.sram) + 1;
    set_frame_counter(&mut s.sram, count);

    // Video: scrolling gradient + square.
    let scroll = (count * s.speed) as usize;
    for (row, line) in s.frame.as_chunks_mut::<WIDTH>().0.iter_mut().enumerate() {
        for (col, px) in line.iter_mut().enumerate() {
            let r = ((col + scroll) % 256) as u32;
            let g = ((row + scroll / 2) % 256) as u32;
            *px = (r << 16) | (g << 8) | 0x60;
        }
    }
    let (x0, y0) = ((s.x - half) as usize, (s.y - half) as usize);
    for row in y0..y0 + SQUARE {
        s.frame[row * WIDTH + x0..row * WIDTH + x0 + SQUARE].fill(0x00FF_FFFF);
    }

    // Audio: one frame worth of stereo samples, tone while A is held.
    let samples = (SAMPLE_RATE / FPS) as usize;
    let tone = pressed(RETRO_DEVICE_ID_JOYPAD_A);
    s.audio.clear();
    for _ in 0..samples {
        let v = if tone {
            (s.phase.sin() * 6000.0) as i16
        } else {
            0
        };
        s.phase = (s.phase + 440.0 * std::f64::consts::TAU / SAMPLE_RATE) % std::f64::consts::TAU;
        s.audio.extend_from_slice(&[v, v]);
    }

    if let Some(video) = video {
        // SAFETY: the buffer holds WIDTH*HEIGHT pixels and stays alive during the call.
        unsafe {
            video(
                s.frame.as_ptr().cast(),
                WIDTH as c_uint,
                HEIGHT as c_uint,
                WIDTH * 4,
            );
        }
    }
    if let Some(batch) = audio_batch {
        // SAFETY: `samples` stereo frames are stored in `s.audio`.
        unsafe { batch(s.audio.as_ptr(), samples) };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn retro_serialize_size() -> usize {
    STATE_SIZE
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn retro_serialize(data: *mut c_void, len: usize) -> bool {
    let guard = state();
    let Some(s) = guard.as_ref() else {
        return false;
    };
    if len < STATE_SIZE {
        return false;
    }
    let mut buf = [0u8; STATE_SIZE];
    buf[..4].copy_from_slice(&s.x.to_le_bytes());
    buf[4..8].copy_from_slice(&s.y.to_le_bytes());
    buf[8..16].copy_from_slice(&frame_counter(&s.sram).to_le_bytes());
    buf[16..24].copy_from_slice(&s.speed.to_le_bytes());
    // SAFETY: the frontend guarantees `data` has `len >= STATE_SIZE` writable bytes.
    unsafe { ptr::copy_nonoverlapping(buf.as_ptr(), data.cast(), STATE_SIZE) };
    true
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn retro_unserialize(data: *const c_void, len: usize) -> bool {
    if len < STATE_SIZE {
        return false;
    }
    let mut buf = [0u8; STATE_SIZE];
    // SAFETY: the frontend guarantees `data` has `len >= STATE_SIZE` readable bytes.
    unsafe { ptr::copy_nonoverlapping(data.cast(), buf.as_mut_ptr(), STATE_SIZE) };
    let mut guard = state();
    let Some(s) = guard.as_mut() else {
        return false;
    };
    let word = |r: std::ops::Range<usize>| {
        let mut b = [0u8; 8];
        b[..r.len()].copy_from_slice(&buf[r]);
        b
    };
    s.x = i32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    s.y = i32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
    set_frame_counter(&mut s.sram, u64::from_le_bytes(word(8..16)));
    s.speed = u64::from_le_bytes(word(16..24));
    true
}

#[unsafe(no_mangle)]
pub extern "C" fn retro_cheat_reset() {}

#[unsafe(no_mangle)]
pub extern "C" fn retro_cheat_set(_index: c_uint, _enabled: bool, _code: *const c_char) {}

#[unsafe(no_mangle)]
pub extern "C" fn retro_get_memory_data(id: c_uint) -> *mut c_void {
    match state().as_mut() {
        // The SRAM box is never reallocated while the core is initialized.
        Some(s) if id == RETRO_MEMORY_SAVE_RAM => s.sram.as_mut_ptr().cast(),
        _ => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn retro_get_memory_size(id: c_uint) -> usize {
    if id == RETRO_MEMORY_SAVE_RAM {
        SRAM_SIZE
    } else {
        0
    }
}
