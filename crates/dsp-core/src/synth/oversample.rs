//! 2x oversampling for HQ mode: voices run at twice the sample rate and a
//! halfband FIR filters and decimates the result back down, so harmonics
//! that drive, sync, FM and resonance push above the output Nyquist are
//! removed instead of folding back as inharmonic aliases.

/// Halfband FIR length (odd). Kaiser-windowed (beta 8) sinc: flat to about
/// 20 kHz (within 0.01 dB) and over 80 dB down from 28 kHz (at 48 kHz out), so
/// anything that would fold back below 20 kHz is removed. Half the taps are
/// zero, so it costs about 32 multiplies per output sample.
pub const TAPS: usize = 63;
/// Delay added by the filter, in output samples.
pub const LATENCY: f32 = (TAPS - 1) as f32 / 4.0;

const KAISER_BETA: f64 = 8.0;

/// Modified Bessel function of the first kind, order 0 (power series).
fn bessel_i0(x: f64) -> f64 {
    let (mut sum, mut term) = (1.0, 1.0);
    for k in 1..50 {
        term *= (x / (2.0 * k as f64)).powi(2);
        sum += term;
    }
    sum
}

/// Decimates by 2: two input samples in, one output sample out.
#[derive(Debug, Clone, Copy)]
pub struct Halfband {
    coeffs: [f32; TAPS],
    /// The last `TAPS` inputs (ring buffer).
    hist: [f32; 64],
    pos: usize,
}

impl Default for Halfband {
    fn default() -> Self {
        Self::new()
    }
}

impl Halfband {
    pub fn new() -> Self {
        let mut coeffs = [0.0f32; TAPS];
        let mid = (TAPS / 2) as f64;
        let mut sum = 0.0;
        for (i, c) in coeffs.iter_mut().enumerate() {
            let n = i as f64 - mid;
            // Ideal low-pass at a quarter of the input rate: 0.5 sinc(n / 2).
            let sinc = if n == 0.0 { 0.5 } else { (std::f64::consts::FRAC_PI_2 * n).sin() / (std::f64::consts::PI * n) };
            let r = n / mid;
            let window = bessel_i0(KAISER_BETA * (1.0 - r * r).sqrt()) / bessel_i0(KAISER_BETA);
            *c = (sinc * window) as f32;
            sum += sinc * window;
        }
        // Unity gain at DC.
        coeffs.iter_mut().for_each(|c| *c /= sum as f32);
        Self { coeffs, hist: [0.0; 64], pos: 0 }
    }

    pub fn reset(&mut self) {
        self.hist = [0.0; 64];
    }

    #[inline]
    fn push(&mut self, x: f32) {
        self.pos = (self.pos + 1) & 63;
        self.hist[self.pos] = x;
    }

    #[inline]
    pub fn process(&mut self, a: f32, b: f32) -> f32 {
        self.push(a);
        self.push(b);
        let mut y = 0.0;
        for (i, c) in self.coeffs.iter().enumerate() {
            if *c != 0.0 {
                y += c * self.hist[self.pos.wrapping_sub(i) & 63];
            }
        }
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gain in dB of a sine at `f` (in units of the output rate, 48 kHz).
    fn gain_db(f: f32) -> f32 {
        let fs_in = 96_000.0f64;
        let mut h = Halfband::new();
        // f64 phase: in f32, sin of large arguments adds noise near -50 dB.
        let x = |i: usize| (std::f64::consts::TAU * f as f64 * i as f64 / fs_in).sin() as f32;
        let out: Vec<f32> = (0..48_000).map(|k| h.process(x(2 * k), x(2 * k + 1))).collect();
        // RMS, not peak: near the output Nyquist a sine's samples miss its peaks.
        let tail = &out[200..];
        let rms = (tail.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / tail.len() as f64).sqrt();
        20.0 * (rms * std::f64::consts::SQRT_2).log10() as f32
    }

    #[test]
    #[ignore]
    fn print_response() {
        for f in [16_000.0f32, 18_000.0, 20_000.0, 22_000.0, 24_000.0, 26_000.0, 28_000.0, 30_000.0, 32_000.0, 36_000.0, 40_000.0] {
            println!("{f:>6} Hz: {:7.2} dB", gain_db(f));
        }
    }

    #[test]
    fn passes_the_audio_band_and_blocks_what_would_alias() {
        for f in [100.0f32, 1_000.0, 10_000.0, 16_000.0, 19_000.0] {
            assert!(gain_db(f).abs() < 0.05, "{f} Hz: {:.3} dB", gain_db(f));
        }
        // Above 28 kHz (which would fold to below 20 kHz) is gone.
        for f in [28_000.0f32, 32_000.0, 40_000.0, 47_000.0] {
            assert!(gain_db(f) < -75.0, "{f} Hz: {:.1} dB", gain_db(f));
        }
    }
}
