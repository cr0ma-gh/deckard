//! Pixel format conversion of core frames to BGRA8.

use deckard_core::{PixelFormat, VideoFrame};

/// Converts a tightly packed core frame to BGRA8 bytes in `out`.
///
/// XRGB8888 is already B, G, R, X in little-endian memory and is copied as is
/// (the shader ignores the X byte).
pub fn to_bgra8(frame: &VideoFrame<'_>, out: &mut Vec<u8>) {
    out.clear();
    match frame.format {
        PixelFormat::Xrgb8888 => out.extend_from_slice(frame.data),
        PixelFormat::Rgb565 => {
            out.reserve(frame.data.len() * 2);
            for px in frame.data.as_chunks::<2>().0 {
                let p = u16::from_le_bytes(*px);
                let (r, g, b) = ((p >> 11) & 0x1f, (p >> 5) & 0x3f, p & 0x1f);
                out.extend_from_slice(&[expand5(b), expand6(g), expand5(r), 0xff]);
            }
        }
        PixelFormat::Rgb1555 => {
            out.reserve(frame.data.len() * 2);
            for px in frame.data.as_chunks::<2>().0 {
                let p = u16::from_le_bytes(*px);
                let (r, g, b) = ((p >> 10) & 0x1f, (p >> 5) & 0x1f, p & 0x1f);
                out.extend_from_slice(&[expand5(b), expand5(g), expand5(r), 0xff]);
            }
        }
    }
}

/// 5-bit channel to 8 bits (replicating the high bits keeps white at 255).
fn expand5(v: u16) -> u8 {
    ((v << 3) | (v >> 2)) as u8
}

/// 6-bit channel to 8 bits.
fn expand6(v: u16) -> u8 {
    ((v << 2) | (v >> 4)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convert(format: PixelFormat, data: &[u8], width: u32) -> Vec<u8> {
        let frame = VideoFrame {
            data,
            width,
            height: 1,
            format,
        };
        let mut out = Vec::new();
        to_bgra8(&frame, &mut out);
        out
    }

    #[test]
    fn rgb565_primaries() {
        let px = |v: u16| v.to_le_bytes();
        let data = [px(0xF800), px(0x07E0), px(0x001F), px(0xFFFF)].concat();
        let out = convert(PixelFormat::Rgb565, &data, 4);
        assert_eq!(&out[0..4], &[0, 0, 255, 255], "red");
        assert_eq!(&out[4..8], &[0, 255, 0, 255], "green");
        assert_eq!(&out[8..12], &[255, 0, 0, 255], "blue");
        assert_eq!(&out[12..16], &[255, 255, 255, 255], "white");
    }

    #[test]
    fn rgb1555_primaries() {
        let px = |v: u16| v.to_le_bytes();
        let data = [px(0x7C00), px(0x03E0), px(0x001F)].concat();
        let out = convert(PixelFormat::Rgb1555, &data, 3);
        assert_eq!(&out[0..4], &[0, 0, 255, 255]);
        assert_eq!(&out[4..8], &[0, 255, 0, 255]);
        assert_eq!(&out[8..12], &[255, 0, 0, 255]);
    }

    #[test]
    fn xrgb8888_is_copied() {
        let data = 0x0012_3456u32.to_le_bytes();
        assert_eq!(
            convert(PixelFormat::Xrgb8888, &data, 1),
            [0x56, 0x34, 0x12, 0x00]
        );
    }
}
