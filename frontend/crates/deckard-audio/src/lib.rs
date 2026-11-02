//! Audio output for Deckard.
//!
//! The emulation thread pushes core samples with [`AudioOutput::push`]; they are
//! resampled to the device rate and handed to the audio thread through a
//! lock-free ring buffer. Dynamic rate control nudges the resampling ratio
//! (at most ±0.5%) to keep that buffer half full: no crackles, no drift.

mod resampler;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};

pub use crate::resampler::Resampler;

/// Maximum ratio adjustment applied by dynamic rate control.
const MAX_RATE_DEVIATION: f64 = 0.005;

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("no audio output device found")]
    NoDevice,
    #[error("audio device error: {0}")]
    Device(#[from] cpal::Error),
    #[error("unsupported sample format {0}")]
    UnsupportedFormat(SampleFormat),
}

/// Ratio correction for a buffer that is `fill` full (0.0..=1.0), target half full.
pub fn rate_control(fill: f64) -> f64 {
    let deviation = ((0.5 - fill) * 2.0).clamp(-1.0, 1.0);
    1.0 + MAX_RATE_DEVIATION * deviation
}

/// An open audio output stream.
pub struct AudioOutput {
    stream: cpal::Stream,
    paused: bool,
    producer: rtrb::Producer<f32>,
    capacity: usize,
    device_rate: u32,
    resampler: Resampler,
    scratch: Vec<f32>,
    underruns: Arc<AtomicU64>,
    dropped: u64,
}

impl AudioOutput {
    /// Opens the default output device. `latency_ms` is the target buffering.
    pub fn new(latency_ms: u32) -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or(AudioError::NoDevice)?;
        let supported = device.default_output_config()?;
        let format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let device_rate = config.sample_rate;

        // Room for twice the target latency, in interleaved stereo samples.
        let capacity = (device_rate as usize * 2 * latency_ms as usize * 2 / 1000).max(1024);
        let (producer, consumer) = rtrb::RingBuffer::new(capacity);
        let underruns = Arc::new(AtomicU64::new(0));

        let stream = match format {
            SampleFormat::F32 => build::<f32>(&device, config, consumer, underruns.clone())?,
            SampleFormat::I16 => build::<i16>(&device, config, consumer, underruns.clone())?,
            SampleFormat::I32 => build::<i32>(&device, config, consumer, underruns.clone())?,
            SampleFormat::U16 => build::<u16>(&device, config, consumer, underruns.clone())?,
            other => return Err(AudioError::UnsupportedFormat(other)),
        };
        stream.play()?;

        tracing::info!(
            rate = device_rate,
            channels = config.channels,
            ?format,
            latency_ms,
            "audio output opened"
        );
        Ok(Self {
            stream,
            paused: false,
            producer,
            capacity,
            device_rate,
            resampler: Resampler::new(),
            scratch: Vec::new(),
            underruns,
            dropped: 0,
        })
    }

    /// Stops or restarts playback (e.g. while the game is paused). The queued
    /// audio is kept and plays on resume.
    pub fn set_paused(&mut self, paused: bool) {
        if paused == self.paused {
            return;
        }
        let result = if paused {
            self.stream.pause()
        } else {
            self.stream.play()
        };
        match result {
            Ok(()) => self.paused = paused,
            Err(e) => tracing::warn!(paused, "cannot change audio playback: {e}"),
        }
    }

    pub fn device_rate(&self) -> u32 {
        self.device_rate
    }

    /// Buffer fill level, 0.0 (empty) to 1.0 (full).
    pub fn fill(&self) -> f64 {
        1.0 - self.producer.slots() as f64 / self.capacity as f64
    }

    /// Number of times the audio thread ran out of samples.
    pub fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }

    /// Queues interleaved stereo samples produced at `source_rate` Hz.
    pub fn push(&mut self, samples: &[i16], source_rate: f64) {
        if samples.is_empty() || source_rate <= 0.0 {
            return;
        }
        // An empty buffer (start, or after a stall) would take seconds to refill
        // through rate control alone: prime it to the target level with silence.
        if self.producer.slots() == self.capacity {
            let silence = (self.capacity / 2) & !1; // whole stereo frames
            for _ in 0..silence {
                let _ = self.producer.push(0.0);
            }
            tracing::debug!(samples = silence, "audio buffer primed");
        }
        let ratio = f64::from(self.device_rate) / source_rate * rate_control(self.fill());
        self.scratch.clear();
        self.resampler.process(samples, ratio, &mut self.scratch);

        let mut dropped = 0;
        for &s in &self.scratch {
            if self.producer.push(s).is_err() {
                dropped += 1;
            }
        }
        if dropped > 0 {
            self.dropped += dropped;
            tracing::debug!(
                dropped,
                total = self.dropped,
                "audio buffer full, samples dropped"
            );
        }
    }
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut consumer: rtrb::Consumer<f32>,
    underruns: Arc<AtomicU64>,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels.max(1));
    // Silence before the first sample arrives is not an underrun.
    let mut started = false;
    let stream = device.build_output_stream(
        config,
        move |out: &mut [T], _| {
            let mut starved = false;
            for frame in out.chunks_mut(channels) {
                let (l, r) = match (consumer.pop(), consumer.pop()) {
                    (Ok(l), Ok(r)) => {
                        started = true;
                        (l, r)
                    }
                    _ => {
                        starved = started;
                        (0.0, 0.0)
                    }
                };
                match frame {
                    [mono] => *mono = T::from_sample((l + r) * 0.5),
                    [left, right, rest @ ..] => {
                        *left = T::from_sample(l);
                        *right = T::from_sample(r);
                        rest.fill(T::from_sample(0.0));
                    }
                    [] => {}
                }
            }
            if starved {
                underruns.fetch_add(1, Ordering::Relaxed);
            }
        },
        |err| tracing::warn!("audio stream error: {err}"),
        None,
    )?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_control_targets_half_full() {
        assert_eq!(rate_control(0.5), 1.0);
        assert!(rate_control(0.0) > 1.0, "empty buffer: produce more");
        assert!(rate_control(1.0) < 1.0, "full buffer: produce less");
        assert!((rate_control(0.0) - 1.005).abs() < 1e-9);
        assert!((rate_control(2.0) - 0.995).abs() < 1e-9, "clamped");
    }
}
