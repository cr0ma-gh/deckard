//! Checks every constant in libretro-sys against the vendored libretro.h.
//!
//! The bindings are hand-written; this test is what keeps them honest.
//! When adding a constant to `src/lib.rs`, add it to the matching list below.

use std::collections::HashMap;

use libretro_sys::*;

const HEADER: &str = include_str!("../include/libretro.h");

/// Removes `/* ... */` and `// ...` comments.
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    loop {
        let block = rest.find("/*");
        let line = rest.find("//");
        match (block, line) {
            (Some(b), l) if l.is_none_or(|l| b < l) => {
                out.push_str(&rest[..b]);
                rest = rest[b..].find("*/").map_or("", |end| &rest[b + end + 2..]);
            }
            (_, Some(l)) => {
                out.push_str(&rest[..l]);
                rest = rest[l..].find('\n').map_or("", |end| &rest[l + end..]);
            }
            _ => {
                out.push_str(rest);
                return out;
            }
        }
    }
}

/// Evaluates the small set of value forms libretro.h uses:
/// `12`, `0x10000`, `INT_MAX`, `NAME`, `(47 | NAME)`.
fn eval(expr: &str, known: &HashMap<String, i64>) -> Option<i64> {
    let expr = expr.trim();
    if let Some(inner) = expr.strip_prefix('(').and_then(|e| e.strip_suffix(')')) {
        return inner
            .split('|')
            .map(|part| eval(part, known))
            .try_fold(0, |acc, v| v.map(|v| acc | v));
    }
    if expr == "INT_MAX" {
        return Some(i64::from(i32::MAX));
    }
    if let Some(hex) = expr.strip_prefix("0x") {
        return i64::from_str_radix(hex, 16).ok();
    }
    expr.parse().ok().or_else(|| known.get(expr).copied())
}

/// Collects `#define RETRO_*` values and `RETRO_*` enum entries from the header.
fn parse_header() -> HashMap<String, i64> {
    let src = strip_comments(HEADER);
    let mut values = HashMap::new();

    for line in src.lines() {
        let Some(rest) = line.trim().strip_prefix("#define ") else {
            continue;
        };
        let rest = rest.trim();
        let Some((name, value)) = rest.split_once(char::is_whitespace) else {
            continue;
        };
        if !name.starts_with("RETRO_") || name.contains('(') {
            continue;
        }
        if let Some(v) = eval(value, &values) {
            values.insert(name.to_owned(), v);
        }
    }

    // Enum bodies: entries are `NAME = VALUE` or `NAME` (previous + 1).
    let mut rest = src.as_str();
    while let Some(start) = rest.find("enum retro_") {
        let after = &rest[start..];
        let (Some(open), Some(close)) = (after.find('{'), after.find('}')) else {
            break;
        };
        if open < close {
            let mut next = 0;
            for entry in after[open + 1..close].split(',') {
                let entry = entry.trim();
                if entry.is_empty() {
                    continue;
                }
                let (name, value) = match entry.split_once('=') {
                    Some((n, v)) => (n.trim(), eval(v, &values)),
                    None => (entry, Some(next)),
                };
                // `enum retro_x` can also appear as a parameter type, which makes
                // this scan land on a struct body; skip anything not an identifier.
                let is_ident = name.starts_with("RETRO_")
                    && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                if !is_ident {
                    continue;
                }
                if let Some(v) = value {
                    values.insert(name.to_owned(), v);
                    next = v + 1;
                }
            }
        }
        rest = &after[close + 1..];
    }

    values
}

macro_rules! check {
    ($header:expr, $failures:expr, [$($name:ident),* $(,)?]) => {
        $(
            let name = stringify!($name);
            match $header.get(name) {
                Some(&expected) if expected == $name as i64 => {}
                Some(&expected) => $failures.push(format!("{name}: rust={} header={expected}", $name)),
                None => $failures.push(format!("{name}: not found in libretro.h")),
            }
        )*
    };
}

#[test]
fn constants_match_libretro_h() {
    let header = parse_header();
    let mut failures: Vec<String> = Vec::new();

    check!(
        header,
        failures,
        [
            RETRO_API_VERSION,
            RETRO_DEVICE_TYPE_SHIFT,
            RETRO_DEVICE_NONE,
            RETRO_DEVICE_JOYPAD,
            RETRO_DEVICE_MOUSE,
            RETRO_DEVICE_KEYBOARD,
            RETRO_DEVICE_LIGHTGUN,
            RETRO_DEVICE_ANALOG,
            RETRO_DEVICE_POINTER,
            RETRO_DEVICE_TOUCHPAD,
            RETRO_DEVICE_ID_JOYPAD_B,
            RETRO_DEVICE_ID_JOYPAD_Y,
            RETRO_DEVICE_ID_JOYPAD_SELECT,
            RETRO_DEVICE_ID_JOYPAD_START,
            RETRO_DEVICE_ID_JOYPAD_UP,
            RETRO_DEVICE_ID_JOYPAD_DOWN,
            RETRO_DEVICE_ID_JOYPAD_LEFT,
            RETRO_DEVICE_ID_JOYPAD_RIGHT,
            RETRO_DEVICE_ID_JOYPAD_A,
            RETRO_DEVICE_ID_JOYPAD_X,
            RETRO_DEVICE_ID_JOYPAD_L,
            RETRO_DEVICE_ID_JOYPAD_R,
            RETRO_DEVICE_ID_JOYPAD_L2,
            RETRO_DEVICE_ID_JOYPAD_R2,
            RETRO_DEVICE_ID_JOYPAD_L3,
            RETRO_DEVICE_ID_JOYPAD_R3,
            RETRO_DEVICE_ID_JOYPAD_MASK,
            RETRO_DEVICE_INDEX_ANALOG_LEFT,
            RETRO_DEVICE_INDEX_ANALOG_RIGHT,
            RETRO_DEVICE_INDEX_ANALOG_BUTTON,
            RETRO_DEVICE_ID_ANALOG_X,
            RETRO_DEVICE_ID_ANALOG_Y,
            RETRO_REGION_NTSC,
            RETRO_REGION_PAL,
            RETRO_MEMORY_MASK,
            RETRO_MEMORY_SAVE_RAM,
            RETRO_MEMORY_RTC,
            RETRO_MEMORY_SYSTEM_RAM,
            RETRO_MEMORY_VIDEO_RAM,
            RETRO_MEMORY_ROM,
            RETRO_ENVIRONMENT_EXPERIMENTAL,
            RETRO_ENVIRONMENT_PRIVATE,
            RETRO_ENVIRONMENT_SET_ROTATION,
            RETRO_ENVIRONMENT_GET_OVERSCAN,
            RETRO_ENVIRONMENT_GET_CAN_DUPE,
            RETRO_ENVIRONMENT_SET_MESSAGE,
            RETRO_ENVIRONMENT_SHUTDOWN,
            RETRO_ENVIRONMENT_SET_PERFORMANCE_LEVEL,
            RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY,
            RETRO_ENVIRONMENT_SET_PIXEL_FORMAT,
            RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS,
            RETRO_ENVIRONMENT_SET_DISK_CONTROL_INTERFACE,
            RETRO_ENVIRONMENT_SET_HW_RENDER,
            RETRO_ENVIRONMENT_GET_VARIABLE,
            RETRO_ENVIRONMENT_SET_VARIABLES,
            RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE,
            RETRO_ENVIRONMENT_SET_SUPPORT_NO_GAME,
            RETRO_ENVIRONMENT_GET_LIBRETRO_PATH,
            RETRO_ENVIRONMENT_GET_RUMBLE_INTERFACE,
            RETRO_ENVIRONMENT_GET_LOG_INTERFACE,
            RETRO_ENVIRONMENT_GET_CORE_ASSETS_DIRECTORY,
            RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY,
            RETRO_ENVIRONMENT_SET_SYSTEM_AV_INFO,
            RETRO_ENVIRONMENT_SET_CONTROLLER_INFO,
            RETRO_ENVIRONMENT_SET_GEOMETRY,
            RETRO_ENVIRONMENT_GET_LANGUAGE,
            RETRO_ENVIRONMENT_GET_AUDIO_VIDEO_ENABLE,
            RETRO_ENVIRONMENT_GET_FASTFORWARDING,
            RETRO_ENVIRONMENT_GET_TARGET_REFRESH_RATE,
            RETRO_ENVIRONMENT_GET_INPUT_BITMASKS,
            RETRO_ENVIRONMENT_GET_CORE_OPTIONS_VERSION,
            RETRO_ENVIRONMENT_GET_INPUT_MAX_USERS,
            RETRO_ENVIRONMENT_SET_CORE_OPTIONS_V2,
            RETRO_ENVIRONMENT_SET_VARIABLE,
            RETRO_ENVIRONMENT_SET_KEYBOARD_CALLBACK,
            RETRO_ENVIRONMENT_SET_FRAME_TIME_CALLBACK,
            RETRO_ENVIRONMENT_GET_PERF_INTERFACE,
            RETRO_ENVIRONMENT_SET_SUBSYSTEM_INFO,
            RETRO_ENVIRONMENT_SET_MEMORY_MAPS,
            RETRO_ENVIRONMENT_GET_USERNAME,
            RETRO_ENVIRONMENT_SET_SUPPORT_ACHIEVEMENTS,
            RETRO_ENVIRONMENT_SET_SERIALIZATION_QUIRKS,
            RETRO_ENVIRONMENT_GET_VFS_INTERFACE,
            RETRO_ENVIRONMENT_SET_CORE_OPTIONS,
            RETRO_ENVIRONMENT_SET_CORE_OPTIONS_INTL,
            RETRO_ENVIRONMENT_SET_CORE_OPTIONS_DISPLAY,
            RETRO_ENVIRONMENT_GET_PREFERRED_HW_RENDER,
            RETRO_ENVIRONMENT_SET_DISK_CONTROL_EXT_INTERFACE,
            RETRO_ENVIRONMENT_GET_DISK_CONTROL_INTERFACE_VERSION,
            RETRO_ENVIRONMENT_GET_MESSAGE_INTERFACE_VERSION,
            RETRO_ENVIRONMENT_SET_MESSAGE_EXT,
            RETRO_ENVIRONMENT_SET_AUDIO_BUFFER_STATUS_CALLBACK,
            RETRO_ENVIRONMENT_SET_MINIMUM_AUDIO_LATENCY,
            RETRO_ENVIRONMENT_SET_CORE_OPTIONS_V2_INTL,
            RETRO_ENVIRONMENT_SET_CORE_OPTIONS_UPDATE_DISPLAY_CALLBACK,
            RETRO_LANGUAGE_ENGLISH,
            RETRO_LANGUAGE_ITALIAN,
            RETRO_NUM_CORE_OPTION_VALUES_MAX,
            RETRO_PIXEL_FORMAT_0RGB1555,
            RETRO_PIXEL_FORMAT_XRGB8888,
            RETRO_PIXEL_FORMAT_RGB565,
            RETRO_PIXEL_FORMAT_XRGB2101010,
            RETRO_PIXEL_FORMAT_HDR10_2101010,
            RETRO_PIXEL_FORMAT_SCRGB_RGBA16F,
            RETRO_PIXEL_FORMAT_UNKNOWN,
            RETRO_LOG_DEBUG,
            RETRO_LOG_INFO,
            RETRO_LOG_WARN,
            RETRO_LOG_ERROR,
            RETRO_LOG_DUMMY,
        ]
    );

    assert!(
        failures.is_empty(),
        "mismatches with libretro.h:\n{}",
        failures.join("\n")
    );
}

#[test]
fn parser_sanity() {
    let header = parse_header();
    // Guard against the parser silently finding nothing.
    assert!(header.len() > 100, "parsed only {} values", header.len());
}
