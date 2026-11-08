//! Small helpers shared by the synth modules.

/// Frequency of a MIDI note in Hz (A4 = note 69 = 440 Hz).
#[inline]
pub fn note_to_freq(note: f32) -> f32 {
    440.0 * ((note - 69.0) / 12.0).exp2()
}

/// Linear ramp to a target over a fixed number of samples. Used to spread
/// a once-per-block parameter change across the block so it doesn't step.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ramp {
    value: f32,
    target: f32,
    step: f32,
    remaining: u32,
}

impl Ramp {
    /// Jump straight to `v`.
    pub fn snap(&mut self, v: f32) {
        *self = Self { value: v, target: v, step: 0.0, remaining: 0 };
    }

    /// Ramp from the current value to `target` over `len` samples.
    pub fn set_target(&mut self, target: f32, len: u32) {
        if len == 0 || target == self.value {
            self.snap(target);
        } else {
            self.target = target;
            self.step = (target - self.value) / len as f32;
            self.remaining = len;
        }
    }

    #[inline]
    pub fn tick(&mut self) -> f32 {
        if self.remaining > 0 {
            self.remaining -= 1;
            self.value = if self.remaining == 0 { self.target } else { self.value + self.step };
        }
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_frequencies() {
        assert_eq!(note_to_freq(69.0), 440.0);
        assert!((note_to_freq(60.0) - 261.625_58).abs() < 1e-3);
        assert!((note_to_freq(81.0) - 880.0).abs() < 1e-3);
    }

    #[test]
    fn ramp_reaches_target_exactly_in_len_steps() {
        let mut r = Ramp::default();
        r.snap(1.0);
        r.set_target(0.0, 4);
        let v: Vec<f32> = (0..6).map(|_| r.tick()).collect();
        assert_eq!(v, [0.75, 0.5, 0.25, 0.0, 0.0, 0.0]);
    }
}
