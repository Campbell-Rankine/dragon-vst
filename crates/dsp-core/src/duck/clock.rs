//! Tempo mode's clock: where we are in the current duck cycle, from the
//! host's beat position.
//!
//! Tempo mode keeps no trigger state. The duck amount is a pure function of
//! the position within the cycle, so loops, jumps and starting playback
//! mid-bar can't leave it out of step. The cycle starts on the beat grid
//! (shifted by the offset).

use super::curve::DuckShape;

/// Beat position, advanced per sample and re-synced to the host each
/// buffer (when the host reports one).
///
/// The position is `base + n * step` rather than a running sum, so it
/// doesn't drift: a 1/4-note cycle at 120 BPM starts on exactly every
/// 24000th sample however long it runs.
#[derive(Debug, Clone, Copy, Default)]
pub struct TempoClock {
    /// Position (quarter-note beats) at the last sync.
    base: f64,
    /// Samples since the last sync.
    n: u64,
    /// Beats per sample.
    step: f64,
}

/// Slack at cycle boundaries (in cycles), for positions that land a hair
/// short of a boundary through rounding.
const BOUNDARY_EPSILON: f64 = 1e-9;

impl TempoClock {
    /// Take the host's position (at the start of a buffer) and the current
    /// beats per sample.
    pub fn sync(&mut self, beats: f64, beats_per_sample: f64) {
        self.base = beats;
        self.n = 0;
        self.step = beats_per_sample;
    }

    /// Keep running from where we are at `beats_per_sample` (no host
    /// position, e.g. the transport is stopped). A tempo change re-bases,
    /// so the position stays continuous.
    pub fn free_run(&mut self, beats_per_sample: f64) {
        if beats_per_sample != self.step {
            self.sync(self.beats(), beats_per_sample);
        }
    }

    /// The current position in beats.
    #[inline]
    pub fn beats(&self) -> f64 {
        self.base + self.n as f64 * self.step
    }

    /// Advance one sample.
    #[inline]
    pub fn advance(&mut self) {
        self.n += 1;
    }

    /// Seconds since the current cycle started. `rate_beats` is the cycle
    /// length in beats (1/4 note = 1), `offset_beats` shifts the grid later.
    #[inline]
    pub fn time_in_cycle(&self, rate_beats: f64, offset_beats: f64, secs_per_beat: f64) -> f64 {
        let pos = self.beats() - offset_beats;
        let cycle = (pos / rate_beats + BOUNDARY_EPSILON).floor();
        ((pos - cycle * rate_beats) * secs_per_beat).max(0.0)
    }
}

/// The duck amount `t` seconds into a cycle of `cycle` seconds.
///
/// Each cycle's attack starts from where the previous cycle ended, so a
/// release longer than the cycle carries over without a jump. That end
/// point doesn't depend on where the attack started (as long as attack
/// and hold fit in the cycle), so this needs no state.
#[inline]
pub fn tempo_duck(shape: &DuckShape, t: f32, cycle: f32) -> f32 {
    let carried = if shape.attack + shape.hold < cycle { shape.duck_at(cycle, 0.0) } else { 1.0 };
    shape.duck_at(t, carried)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;

    /// Samples (from 0) where a new cycle starts, over `n` samples.
    fn cycle_starts(bpm: f64, rate_beats: f64, offset_beats: f64, start_beats: f64, n: usize) -> Vec<usize> {
        let mut c = TempoClock::default();
        let bps = bpm / 60.0 / SR;
        c.sync(start_beats, bps);
        let spb = 60.0 / bpm;
        let mut prev = f64::INFINITY;
        let mut starts = Vec::new();
        for i in 0..n {
            let t = c.time_in_cycle(rate_beats, offset_beats, spb);
            if t < prev {
                starts.push(i);
            }
            prev = t;
            c.advance();
        }
        starts
    }

    #[test]
    fn quarter_notes_at_120_bpm_start_every_half_second() {
        assert_eq!(cycle_starts(120.0, 1.0, 0.0, 0.0, 72_001), vec![0, 24_000, 48_000, 72_000]);
    }

    #[test]
    fn dotted_and_triplet_spacing() {
        // 1/8 dotted = 0.75 beats = 18000 samples at 120 BPM.
        assert_eq!(cycle_starts(120.0, 0.75, 0.0, 0.0, 36_001), vec![0, 18_000, 36_000]);
        // 1/8 triplet = 1/3 beat = 8000 samples.
        assert_eq!(cycle_starts(120.0, 1.0 / 3.0, 0.0, 0.0, 16_001), vec![0, 8_000, 16_000]);
    }

    #[test]
    fn starting_mid_beat_waits_for_the_grid() {
        // Playback starts a quarter of the way into a beat: the first full
        // cycle starts 3/4 beat (18000 samples) later.
        assert_eq!(cycle_starts(120.0, 1.0, 0.0, 0.25, 24_000)[1..], [18_000]);
    }

    #[test]
    fn offset_shifts_the_grid_by_exact_samples() {
        // +10 ms = 0.02 beats at 120 BPM = 480 samples later.
        assert_eq!(cycle_starts(120.0, 1.0, 0.02, 0.0, 24_481)[1..], [480, 24_480]);
    }

    #[test]
    fn free_run_continues_and_rebases_on_a_tempo_change() {
        let mut c = TempoClock::default();
        c.sync(1.0, 120.0 / 60.0 / SR);
        for _ in 0..24_000 {
            c.advance();
        }
        assert!((c.beats() - 2.0).abs() < 1e-9);
        // Same tempo: nothing changes.
        c.free_run(120.0 / 60.0 / SR);
        assert!((c.beats() - 2.0).abs() < 1e-9);
        // Slower: continues from beat 2 at the new rate.
        c.free_run(60.0 / 60.0 / SR);
        for _ in 0..48_000 {
            c.advance();
        }
        assert!((c.beats() - 3.0).abs() < 1e-9);
    }

    #[test]
    fn tempo_change_keeps_the_grid() {
        // 90 BPM, 1/4: 2/3 s = 32000 samples per beat.
        assert_eq!(cycle_starts(90.0, 1.0, 0.0, 0.0, 64_001), vec![0, 32_000, 64_000]);
    }

    #[test]
    fn a_long_release_carries_over_without_a_jump() {
        // Release longer than the cycle: the end of one cycle and the
        // start of the next match.
        let s = DuckShape { depth: 1.0, attack: 0.001, hold: 0.0, release: 0.8, tension: 0.4 };
        let cycle = 0.5;
        let end = tempo_duck(&s, cycle - 1e-6, cycle);
        let start = tempo_duck(&s, 0.0, cycle);
        assert!(end > 0.0);
        assert!((end - start).abs() < 1e-3, "{end} vs {start}");
    }

    #[test]
    fn a_short_release_ends_each_cycle_at_rest() {
        let s = DuckShape::default();
        assert_eq!(tempo_duck(&s, 0.0, 0.5), 0.0);
        assert_eq!(tempo_duck(&s, 0.4, 0.5), 0.0);
        assert_eq!(tempo_duck(&s, 0.001, 0.5), 1.0);
    }
}
