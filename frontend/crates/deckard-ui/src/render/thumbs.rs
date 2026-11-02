//! Cover images: decoded on a worker thread, uploaded to the GPU on demand.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

/// Largest side accepted for a cover; bigger images are skipped.
const MAX_SIDE: u32 = 4096;
/// Covers kept on the GPU.
const CACHE_SIZE: usize = 24;

/// A decoded RGBA8 image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Decodes a PNG file to RGBA8.
pub fn decode_png(path: &Path) -> Result<Rgba, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut decoder = png::Decoder::new(std::io::Cursor::new(data));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let size = reader.output_buffer_size().ok_or("image too large")?;
    let mut buf = vec![0; size];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(info.buffer_size());
    if info.width == 0 || info.height == 0 || info.width > MAX_SIDE || info.height > MAX_SIDE {
        return Err(format!("unsupported size {}x{}", info.width, info.height));
    }
    let pixels = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[r, g, b]| [r, g, b, 255])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[v, a]| [v, v, v, a])
            .collect(),
        png::ColorType::Indexed => return Err("indexed PNG not expanded".into()),
    };
    Ok(Rgba {
        width: info.width,
        height: info.height,
        pixels,
    })
}

/// A cover on the GPU.
pub struct GpuImage {
    pub bind_group: wgpu::BindGroup,
    pub width: u32,
    pub height: u32,
}

enum Entry {
    Loading,
    Ready(GpuImage),
    Failed,
}

type Decoded = (PathBuf, Result<Rgba, String>);

/// Asynchronous cover cache.
pub struct Thumbnails {
    entries: HashMap<PathBuf, Entry>,
    order: VecDeque<PathBuf>,
    requests: Sender<PathBuf>,
    results: Receiver<Decoded>,
}

impl Thumbnails {
    pub fn new() -> Self {
        let (requests, worker_rx) = channel::<PathBuf>();
        let (worker_tx, results) = channel::<Decoded>();
        let spawned = std::thread::Builder::new()
            .name("deckard-covers".into())
            .spawn(move || {
                for path in worker_rx {
                    let decoded = decode_png(&path);
                    if worker_tx.send((path, decoded)).is_err() {
                        break;
                    }
                }
            });
        if let Err(e) = spawned {
            tracing::warn!("cover loader unavailable: {e}");
        }
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            requests,
            results,
        }
    }

    /// Returns the cover if it is ready, starting to load it otherwise.
    pub fn get(&mut self, path: &Path) -> Option<&GpuImage> {
        if !self.entries.contains_key(path) {
            self.entries.insert(path.to_path_buf(), Entry::Loading);
            let _ = self.requests.send(path.to_path_buf());
        }
        match self.entries.get(path) {
            Some(Entry::Ready(img)) => Some(img),
            _ => None,
        }
    }

    /// The cover if it is already on the GPU (never starts a load).
    pub fn ready(&self, path: &Path) -> Option<&GpuImage> {
        match self.entries.get(path) {
            Some(Entry::Ready(img)) => Some(img),
            _ => None,
        }
    }

    /// Uploads covers decoded since the last call.
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        make: impl Fn(&wgpu::Device, &wgpu::Queue, &Rgba) -> GpuImage,
    ) {
        while let Ok((path, decoded)) = self.results.try_recv() {
            let entry = match decoded {
                Ok(rgba) => Entry::Ready(make(device, queue, &rgba)),
                Err(e) => {
                    tracing::warn!(path = %path.display(), "cannot load cover: {e}");
                    Entry::Failed
                }
            };
            self.entries.insert(path.clone(), entry);
            self.order.push_back(path);
        }
        while self.order.len() > CACHE_SIZE {
            if let Some(old) = self.order.pop_front() {
                self.entries.remove(&old);
            }
        }
    }
}

impl Default for Thumbnails {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_png(path: &Path, color: png::ColorType, data: &[u8], w: u32, h: u32) {
        let file = std::fs::File::create(path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(color);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(data).unwrap();
    }

    #[test]
    fn decodes_rgb_and_gray_to_rgba() {
        let dir = std::env::temp_dir().join(format!("deckard-png-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rgb = dir.join("rgb.png");
        write_png(&rgb, png::ColorType::Rgb, &[255, 0, 0, 0, 255, 0], 2, 1);
        let img = decode_png(&rgb).unwrap();
        assert_eq!((img.width, img.height), (2, 1));
        assert_eq!(img.pixels, [255, 0, 0, 255, 0, 255, 0, 255]);

        let gray = dir.join("gray.png");
        write_png(&gray, png::ColorType::Grayscale, &[128], 1, 1);
        assert_eq!(decode_png(&gray).unwrap().pixels, [128, 128, 128, 255]);

        assert!(decode_png(&dir.join("missing.png")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
