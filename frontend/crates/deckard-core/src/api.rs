//! Function table of a loaded libretro core.

use libloading::Library;
use libretro_sys::*;

use crate::CoreError;

/// Every function a libretro core must export, resolved once at load time.
#[derive(Clone, Copy)]
pub(crate) struct CoreApi {
    pub set_environment: retro_set_environment_fn,
    pub set_video_refresh: retro_set_video_refresh_fn,
    pub set_audio_sample: retro_set_audio_sample_fn,
    pub set_audio_sample_batch: retro_set_audio_sample_batch_fn,
    pub set_input_poll: retro_set_input_poll_fn,
    pub set_input_state: retro_set_input_state_fn,
    pub init: retro_init_fn,
    pub deinit: retro_deinit_fn,
    pub api_version: retro_api_version_fn,
    pub get_system_info: retro_get_system_info_fn,
    pub get_system_av_info: retro_get_system_av_info_fn,
    pub set_controller_port_device: retro_set_controller_port_device_fn,
    pub reset: retro_reset_fn,
    pub run: retro_run_fn,
    pub serialize_size: retro_serialize_size_fn,
    pub serialize: retro_serialize_fn,
    pub unserialize: retro_unserialize_fn,
    pub load_game: retro_load_game_fn,
    pub unload_game: retro_unload_game_fn,
    pub get_memory_data: retro_get_memory_data_fn,
    pub get_memory_size: retro_get_memory_size_fn,
}

impl CoreApi {
    /// Resolves all symbols. The returned pointers are only valid while `lib` is loaded.
    ///
    /// # Safety
    /// `lib` must be a libretro core: each symbol must have the libretro signature.
    pub unsafe fn load(lib: &Library) -> Result<Self, CoreError> {
        macro_rules! sym {
            ($name:literal) => {{
                // SAFETY: the caller guarantees `lib` is a libretro core, so the symbol
                // has the type declared in libretro.h (mirrored by libretro-sys).
                match unsafe { lib.get(concat!("retro_", $name, "\0").as_bytes()) } {
                    Ok(s) => *s,
                    Err(source) => {
                        return Err(CoreError::MissingSymbol {
                            name: concat!("retro_", $name),
                            source,
                        });
                    }
                }
            }};
        }
        Ok(Self {
            set_environment: sym!("set_environment"),
            set_video_refresh: sym!("set_video_refresh"),
            set_audio_sample: sym!("set_audio_sample"),
            set_audio_sample_batch: sym!("set_audio_sample_batch"),
            set_input_poll: sym!("set_input_poll"),
            set_input_state: sym!("set_input_state"),
            init: sym!("init"),
            deinit: sym!("deinit"),
            api_version: sym!("api_version"),
            get_system_info: sym!("get_system_info"),
            get_system_av_info: sym!("get_system_av_info"),
            set_controller_port_device: sym!("set_controller_port_device"),
            reset: sym!("reset"),
            run: sym!("run"),
            serialize_size: sym!("serialize_size"),
            serialize: sym!("serialize"),
            unserialize: sym!("unserialize"),
            load_game: sym!("load_game"),
            unload_game: sym!("unload_game"),
            get_memory_data: sym!("get_memory_data"),
            get_memory_size: sym!("get_memory_size"),
        })
    }
}
