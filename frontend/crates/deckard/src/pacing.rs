//! Decides how many emulated frames to run per displayed frame.

use std::time::Instant;

/// Largest display/core rate mismatch handled by syncing to vsync. The audio
/// input rate is scaled by the same factor, so sound pitch shifts by at most this.
pub const MAX_TIMING_SKEW: f64 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMode {
    /// One core frame per display refresh; audio rate scaled to match.
    Vsync,
    /// Core frames scheduled by wall clock (display rate too far from core rate).
    Timed,
}

/// Frame scheduler.
#[derive(Debug)]
pub struct Pacer {
    mode: SyncMode,
    core_fps: f64,
    display_hz: Option<f64>,
    last: Option<Instant>,
    accumulator: f64,
}

impl Pacer {
    pub fn new(core_fps: f64, display_hz: Option<f64>) -> Self {
        let mut pacer = Self {
            mode: SyncMode::Timed,
            core_fps,
            display_hz,
            last: None,
            accumulator: 0.0,
        };
        pacer.update(core_fps, display_hz);
        pacer
    }

    /// Recomputes the mode after the core or display timing changed.
    pub fn update(&mut self, core_fps: f64, display_hz: Option<f64>) {
        self.core_fps = core_fps;
        self.display_hz = display_hz;
        self.mode = match display_hz {
            Some(hz) if core_fps > 0.0 && ((hz - core_fps) / core_fps).abs() <= MAX_TIMING_SKEW => {
                SyncMode::Vsync
            }
            _ => SyncMode::Timed,
        };
        tracing::info!(core_fps, ?display_hz, mode = ?self.mode, "frame pacing");
    }

    #[cfg(test)]
    pub fn mode(&self) -> SyncMode {
        self.mode
    }

    /// Audio rate to feed the resampler with: in vsync mode the core runs at the
    /// display rate, so its audio is produced faster or slower by the same ratio.
    pub fn audio_source_rate(&self, core_sample_rate: f64) -> f64 {
        match (self.mode, self.display_hz) {
            (SyncMode::Vsync, Some(hz)) => core_sample_rate * hz / self.core_fps,
            _ => core_sample_rate,
        }
    }

    /// Number of core frames to run now.
    pub fn frames_due(&mut self, now: Instant) -> u32 {
        match self.mode {
            SyncMode::Vsync => 1,
            SyncMode::Timed => {
                let elapsed = self
                    .last
                    .map_or(1.0 / self.core_fps, |l| (now - l).as_secs_f64());
                self.last = Some(now);
                // Cap catch-up after stalls (window drag, breakpoints).
                self.accumulator = (self.accumulator + elapsed * self.core_fps).min(3.0);
                let due = self.accumulator.floor();
                self.accumulator -= due;
                due as u32
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn chooses_vsync_when_rates_are_close() {
        assert_eq!(Pacer::new(60.0988, Some(60.0)).mode(), SyncMode::Vsync); // NES/SNES on the Deck
        assert_eq!(Pacer::new(59.7275, Some(60.0)).mode(), SyncMode::Vsync); // GBA
        assert_eq!(Pacer::new(50.0, Some(60.0)).mode(), SyncMode::Timed); // PAL
        assert_eq!(Pacer::new(60.0, Some(144.0)).mode(), SyncMode::Timed); // gaming monitor
        assert_eq!(Pacer::new(60.0, None).mode(), SyncMode::Timed);
    }

    #[test]
    fn scales_audio_rate_in_vsync_mode() {
        let pacer = Pacer::new(59.7275, Some(60.0));
        let rate = pacer.audio_source_rate(32_768.0);
        assert!((rate - 32_768.0 * 60.0 / 59.7275).abs() < 1e-6);
        assert_eq!(
            Pacer::new(50.0, Some(60.0)).audio_source_rate(48_000.0),
            48_000.0
        );
    }

    #[test]
    fn timed_mode_runs_at_core_rate() {
        let mut pacer = Pacer::new(60.0, Some(144.0));
        let start = Instant::now();
        let mut total = 0;
        // Simulate one second of 144 Hz refreshes.
        for i in 0..=144u32 {
            total += pacer.frames_due(start + Duration::from_secs_f64(f64::from(i) / 144.0));
        }
        assert!((60..=62).contains(&total), "ran {total} frames");
    }

    #[test]
    fn timed_mode_caps_catch_up() {
        let mut pacer = Pacer::new(60.0, None);
        let start = Instant::now();
        pacer.frames_due(start);
        assert_eq!(pacer.frames_due(start + Duration::from_secs(5)), 3);
    }
}
