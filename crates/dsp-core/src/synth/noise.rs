//! Deterministic random numbers and noise sources. No external crates, and
//! the same seed always gives the same output, so renders are repeatable.

use super::patch::NoiseColor;

/// xorshift32: tiny, fast, good enough for audio noise and random phases.
#[derive(Debug, Clone, Copy)]
pub struct Rng(u32);

impl Rng {
    /// Any seed works; 0 (which xorshift can't leave) is replaced.
    pub fn new(seed: u32) -> Self {
        Self(if seed == 0 { 0x9E37_79B9 } else { seed })
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// Uniform in `0..1`.
    #[inline]
    pub fn next_f32(&mut self) -> f32 {
        // Top 24 bits -> exactly representable in f32.
        (self.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }

    /// Uniform in `-1..1`.
    #[inline]
    pub fn next_bipolar(&mut self) -> f32 {
        2.0 * self.next_f32() - 1.0
    }
}

/// White or pink noise, roughly -1..1.
#[derive(Debug, Clone, Copy)]
pub struct Noise {
    rng: Rng,
    /// Pink filter state (Paul Kellet's refined method).
    b: [f32; 7],
}

impl Noise {
    pub fn new(seed: u32) -> Self {
        Self { rng: Rng::new(seed), b: [0.0; 7] }
    }

    #[inline]
    pub fn next(&mut self, color: NoiseColor) -> f32 {
        let white = self.rng.next_bipolar();
        match color {
            NoiseColor::White => white,
            NoiseColor::Pink => {
                let b = &mut self.b;
                b[0] = 0.99886 * b[0] + white * 0.055_517_9;
                b[1] = 0.99332 * b[1] + white * 0.075_075_9;
                b[2] = 0.96900 * b[2] + white * 0.153_852;
                b[3] = 0.86650 * b[3] + white * 0.310_485_6;
                b[4] = 0.55000 * b[4] + white * 0.532_952_2;
                b[5] = -0.7616 * b[5] - white * 0.016_898;
                let pink = b[0] + b[1] + b[2] + b[3] + b[4] + b[5] + b[6] + white * 0.5362;
                b[6] = white * 0.115_926;
                pink * 0.11
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(x: &[f32]) -> (f64, f64, f64) {
        let n = x.len() as f64;
        let mean = x.iter().map(|&v| v as f64).sum::<f64>() / n;
        let power = x.iter().map(|&v| (v as f64).powi(2)).sum::<f64>() / n;
        let diff = x.windows(2).map(|w| ((w[1] - w[0]) as f64).powi(2)).sum::<f64>() / (n - 1.0);
        (mean, power, diff)
    }

    fn render(color: NoiseColor, n: usize) -> Vec<f32> {
        let mut noise = Noise::new(1234);
        (0..n).map(|_| noise.next(color)).collect()
    }

    #[test]
    fn white_noise_is_uniform() {
        let (mean, power, diff) = stats(&render(NoiseColor::White, 480_000));
        assert!(mean.abs() < 0.01, "mean {mean}");
        // Uniform -1..1 has power 1/3; uncorrelated samples double it when differenced.
        assert!((power - 1.0 / 3.0).abs() < 0.01, "power {power}");
        assert!((diff / power - 2.0).abs() < 0.05, "diff ratio {}", diff / power);
    }

    #[test]
    fn pink_noise_is_darker_and_bounded() {
        let x = render(NoiseColor::Pink, 480_000);
        let (mean, power, diff) = stats(&x);
        assert!(mean.abs() < 0.05, "mean {mean}");
        assert!(power > 0.01 && power < 0.3, "power {power}");
        // Mostly low-frequency energy: differencing removes most of it.
        assert!(diff / power < 0.5, "diff ratio {}", diff / power);
        assert!(x.iter().all(|v| v.abs() < 2.0));
    }

    #[test]
    fn same_seed_same_output() {
        assert_eq!(render(NoiseColor::Pink, 1000), render(NoiseColor::Pink, 1000));
        assert_ne!(Noise::new(1).next(NoiseColor::White), Noise::new(2).next(NoiseColor::White));
    }
}
