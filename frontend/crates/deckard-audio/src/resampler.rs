//! Streaming stereo resampler with a variable ratio (cubic Hermite).
//!
//! The ratio can change on every call, which is what dynamic rate control
//! needs: it nudges the ratio to keep the output buffer half full.

/// Converts interleaved stereo `i16` input into interleaved stereo `f32` output.
#[derive(Debug, Clone)]
pub struct Resampler {
    /// Last four input frames; output is interpolated between `history[1]` and `history[2]`.
    history: [[f32; 2]; 4],
    /// Position inside the current interval, in input frames (0.0..1.0).
    position: f64,
}

impl Default for Resampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Resampler {
    pub fn new() -> Self {
        Self {
            history: [[0.0; 2]; 4],
            position: 0.0,
        }
    }

    /// Resamples `input` (interleaved stereo) and appends the result to `output`.
    ///
    /// `ratio` is output rate / input rate (e.g. 48000 / 44100).
    pub fn process(&mut self, input: &[i16], ratio: f64, output: &mut Vec<f32>) {
        if ratio <= 0.0 || !ratio.is_finite() {
            return;
        }
        let step = 1.0 / ratio;
        output.reserve((input.len() as f64 * ratio) as usize + 4);

        for frame in input.as_chunks::<2>().0 {
            self.history.rotate_left(1);
            self.history[3] = [f32::from(frame[0]) / 32768.0, f32::from(frame[1]) / 32768.0];

            while self.position < 1.0 {
                let t = self.position as f32;
                for ch in 0..2 {
                    output.push(hermite(
                        self.history[0][ch],
                        self.history[1][ch],
                        self.history[2][ch],
                        self.history[3][ch],
                        t,
                    ));
                }
                self.position += step;
            }
            self.position -= 1.0;
        }
    }
}

/// Cubic Hermite interpolation between `p1` and `p2` at `t` in `[0, 1)`.
fn hermite(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let c0 = p1;
    let c1 = 0.5 * (p2 - p0);
    let c2 = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
    let c3 = 0.5 * (p3 - p0) + 1.5 * (p1 - p2);
    ((c3 * t + c2) * t + c1) * t + c0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: f64, freq: f64, frames: usize) -> Vec<i16> {
        (0..frames)
            .flat_map(|i| {
                let v = ((i as f64 * freq * std::f64::consts::TAU / rate).sin() * 16000.0) as i16;
                [v, v]
            })
            .collect()
    }

    #[test]
    fn output_length_follows_ratio() {
        for ratio in [1.0, 48_000.0 / 44_100.0, 0.5, 2.0] {
            let mut r = Resampler::new();
            let mut out = Vec::new();
            r.process(&vec![0; 2 * 10_000], ratio, &mut out);
            let expected = 10_000.0 * ratio;
            let frames = out.len() as f64 / 2.0;
            assert!(
                (frames - expected).abs() <= 2.0,
                "ratio {ratio}: {frames} vs {expected}"
            );
        }
    }

    #[test]
    fn length_is_stable_across_chunks() {
        let mut r = Resampler::new();
        let mut out = Vec::new();
        let ratio = 48_000.0 / 32_040.0;
        for _ in 0..100 {
            r.process(&vec![0; 2 * 534], ratio, &mut out);
        }
        let expected = 100.0 * 534.0 * ratio;
        assert!((out.len() as f64 / 2.0 - expected).abs() <= 2.0);
    }

    #[test]
    fn preserves_a_sine_wave() {
        let (in_rate, out_rate, freq) = (44_100.0, 48_000.0, 440.0);
        let mut r = Resampler::new();
        let mut out = Vec::new();
        r.process(&sine(in_rate, freq, 4410), out_rate / in_rate, &mut out);

        // Output lags by two input frames (the interpolation window).
        let delay = 2.0 / in_rate;
        let max_err = out
            .as_chunks::<2>()
            .0
            .iter()
            .enumerate()
            .skip(8)
            .map(|(i, f)| {
                let t = i as f64 / out_rate - delay;
                let expected = (t * freq * std::f64::consts::TAU).sin() * 16000.0 / 32768.0;
                (f64::from(f[0]) - expected).abs()
            })
            .fold(0.0, f64::max);
        assert!(max_err < 0.01, "max error {max_err}");
    }
}
