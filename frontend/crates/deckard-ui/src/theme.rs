//! Interface colors, in one place. The palette follows the Steam Deck
//! interface: dark blue-grey surfaces, light grey text and Steam blue for the
//! focused element. Values are sRGB, 0..1 (the background lives in
//! `render/ui.wgsl` and uses the same tones).

use crate::layout::Color;

/// Builds a color from a `0xRRGGBB` value and an alpha.
pub const fn hex(rgb: u32, alpha: f32) -> Color {
    [
        ((rgb >> 16) & 0xff) as f32 / 255.0,
        ((rgb >> 8) & 0xff) as f32 / 255.0,
        (rgb & 0xff) as f32 / 255.0,
        alpha,
    ]
}

/// `color` with its alpha multiplied by `alpha` (fades).
pub fn faded(color: Color, alpha: f32) -> Color {
    [color[0], color[1], color[2], color[3] * alpha]
}

/// Main text.
pub const TEXT: Color = hex(0xffffff, 1.0);
/// Secondary text (descriptions, values).
pub const TEXT_DIM: Color = hex(0xb8bcbf, 1.0);
/// Least important text (hints, disabled).
pub const TEXT_MUTED: Color = hex(0x8b929a, 1.0);
/// Text on light backgrounds (button badges, selected keys).
pub const TEXT_DARK: Color = hex(0x0e141b, 1.0);

/// Steam blue: the focused element.
pub const ACCENT: Color = hex(0x1a9fff, 1.0);
/// Darker blue for toggles that are on.
pub const ACCENT_DARK: Color = hex(0x0e5c99, 1.0);
/// Errors and destructive actions.
pub const DANGER: Color = hex(0xe8574d, 1.0);

/// Dialogs and side panels.
pub const PANEL: Color = hex(0x1f242c, 1.0);
/// Fields and wells inside panels.
pub const PANEL_DEEP: Color = hex(0x0e1218, 1.0);
/// Tiles and buttons that are not focused.
pub const SURFACE: Color = hex(0x3d4450, 1.0);
/// Secondary buttons (special keys).
pub const SURFACE_DARK: Color = hex(0x2a2f38, 1.0);
/// Dims what is behind a dialog.
pub const SCRIM: Color = hex(0x000000, 0.7);
/// Button badges in the hint bar.
pub const BADGE: Color = hex(0xdcdedf, 1.0);

/// Linear blend between two colors (`t` = 0 → `a`, 1 → `b`).
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_mix() {
        assert_eq!(hex(0xff0000, 0.5), [1.0, 0.0, 0.0, 0.5]);
        let m = mix(hex(0x000000, 1.0), hex(0xffffff, 1.0), 0.5);
        assert!((m[0] - 0.5).abs() < 1e-6);
        let clamped = mix(TEXT, ACCENT, 2.0);
        assert!(
            clamped.iter().zip(ACCENT).all(|(a, b)| (a - b).abs() < 1e-6),
            "clamped"
        );
        assert_eq!(faded(ACCENT, 0.5)[3], 0.5);
    }
}
