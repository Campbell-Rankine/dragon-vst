//! Audio mode's kick detector: finds the hits in a sidechain signal.
//!
//! ```text
//! sidechain ─▶ band-pass (SC Freq, Q 1) ─▶ |x| ─▶ peak follower ─▶ threshold ─▶ trigger
//!                                                 1 ms up, 50 ms down   + 3 dB hysteresis
//!                                                                      + retrigger gap
//! ```
//!
//! - The **band-pass** picks the kick's low thump out of whatever else is
//!   on the track; its output is also what Listen plays.
//! - A hit is the follower **rising through** the threshold. It must fall
//!   3 dB below the threshold before another hit can count, so a kick's
//!   own wobble around the threshold triggers once.
//! - A rise inside the **retrigger gap** after the last hit is ignored
//!   (flams, a kick's second bounce), and it doesn't wait to fire late.

use crate::svf::{Svf, SvfCoefs};

/// The band-pass's quality: wide enough to catch a kick's fundamental
/// wherever it is near SC Freq.
pub const DETECTOR_Q: f32 = 1.0;
/// Peak follower times (s).
pub const FOLLOWER_ATTACK: f32 = 0.001;
pub const FOLLOWER_RELEASE: f32 = 0.05;
/// How far (dB) below the threshold the follower must fall to re-arm.
pub const HYSTERESIS_DB: f32 = 3.0;

/// The detector's settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectorSettings {
    /// Hit threshold (dBFS, of the band-passed signal).
    pub threshold_db: f32,
    /// Band-pass centre (Hz).
    pub freq: f32,
    /// Shortest time between hits (s).
    pub retrigger: f32,
}

impl Default for DetectorSettings {
    fn default() -> Self {
        Self { threshold_db: -24.0, freq: 80.0, retrigger: 0.06 }
    }
}

/// One sample's output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detection {
    /// The band-passed sidechain (what Listen plays).
    pub filtered: f32,
    /// The peak follower (linear), for the editor's detector meter.
    pub level: f32,
    /// A hit starts on this sample.
    pub hit: bool,
}

#[derive(Debug, Clone)]
pub struct KickDetector {
    sample_rate: f32,
    svf: Svf,
    coefs: SvfCoefs,
    /// The frequency `coefs` was designed for.
    coefs_freq: f32,
    attack: f32,
    release: f32,
    level: f32,
    armed: bool,
    /// Seconds since the last hit.
    since_hit: f32,
}

impl KickDetector {
    pub fn new(sample_rate: f32) -> Self {
        let coef = |secs: f32| (-1.0 / (secs * sample_rate)).exp();
        let freq = DetectorSettings::default().freq;
        Self {
            sample_rate,
            svf: Svf::default(),
            coefs: SvfCoefs::band_pass(freq, DETECTOR_Q, sample_rate),
            coefs_freq: freq,
            attack: coef(FOLLOWER_ATTACK),
            release: coef(FOLLOWER_RELEASE),
            level: 0.0,
            armed: true,
            since_hit: f32::INFINITY,
        }
    }

    /// Forget the signal so far.
    pub fn reset(&mut self) {
        self.svf.reset();
        self.level = 0.0;
        self.armed = true;
        self.since_hit = f32::INFINITY;
    }

    /// Process one (mono) sidechain sample.
    #[inline]
    pub fn process(&mut self, x: f32, s: &DetectorSettings) -> Detection {
        if s.freq != self.coefs_freq {
            self.coefs = SvfCoefs::band_pass(s.freq, DETECTOR_Q, self.sample_rate);
            self.coefs_freq = s.freq;
        }
        let filtered = self.svf.process(&self.coefs, x);
        let rect = filtered.abs();
        let coef = if rect > self.level { self.attack } else { self.release };
        self.level = rect + coef * (self.level - rect);

        let threshold = db_to_gain(s.threshold_db);
        self.since_hit += 1.0 / self.sample_rate;
        let mut hit = false;
        if self.armed && self.level >= threshold {
            self.armed = false;
            if self.since_hit >= s.retrigger {
                hit = true;
                self.since_hit = 0.0;
            }
        } else if !self.armed && self.level < threshold * db_to_gain(-HYSTERESIS_DB) {
            self.armed = true;
        }
        Detection { filtered, level: self.level, hit }
    }
}

#[inline]
fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    const SR: f32 = 48_000.0;

    /// A synthetic kick: a 60 Hz sine with an 80 ms decay.
    fn kick(t: f32) -> f32 {
        0.8 * (-t / 0.08).exp() * (2.0 * PI * 60.0 * t).sin()
    }

    /// Hit sample indices for a signal.
    fn hits(signal: impl Fn(usize) -> f32, n: usize, s: &DetectorSettings) -> Vec<usize> {
        let mut d = KickDetector::new(SR);
        (0..n).filter(|&i| d.process(signal(i), s).hit).collect()
    }

    /// Kicks every `period` samples.
    fn kicks(period: usize) -> impl Fn(usize) -> f32 {
        move |i| kick((i % period) as f32 / SR)
    }

    #[test]
    fn one_hit_per_kick_at_120_bpm() {
        let h = hits(kicks(24_000), 96_000, &DetectorSettings::default());
        assert_eq!(h.len(), 4, "{h:?}");
        // Each hit lands within 5 ms of its kick's start.
        for (n, &i) in h.iter().enumerate() {
            let late = i - n * 24_000;
            assert!(late < 240, "kick {n} detected {late} samples late");
        }
    }

    #[test]
    fn silence_and_quiet_signals_never_trigger() {
        let s = DetectorSettings::default();
        assert!(hits(|_| 0.0, 48_000, &s).is_empty());
        // A kick 30 dB down is under the -24 dB threshold.
        assert!(hits(|i| 0.03 * kicks(24_000)(i), 96_000, &s).is_empty());
    }

    #[test]
    fn a_sustained_bass_triggers_once() {
        // A held 60 Hz note at -6 dB: one rise through the threshold, then
        // it never falls far enough to re-arm.
        let bass = |i: usize| 0.5 * (2.0 * PI * 60.0 * i as f32 / SR).sin();
        assert_eq!(hits(bass, 96_000, &DetectorSettings::default()).len(), 1);
    }

    #[test]
    fn the_band_pass_ignores_highs() {
        // A loud 3 kHz click track at 120 BPM: filtered away.
        let hat = |i: usize| {
            let t = (i % 24_000) as f32 / SR;
            0.8 * (-t / 0.02).exp() * (2.0 * PI * 3000.0 * t).sin()
        };
        assert!(hits(hat, 96_000, &DetectorSettings::default()).is_empty());
    }

    #[test]
    fn hits_inside_the_retrigger_gap_are_ignored() {
        // 10 ms bursts of 150 Hz every 60 ms (2880 samples), short enough
        // for the follower to re-arm between them. A 100 ms gap lets every
        // other one through; a 30 ms gap lets each through.
        let bursts = |i: usize| {
            let t = (i % 2_880) as f32 / SR;
            if t < 0.01 { 0.8 * (2.0 * PI * 150.0 * t).sin() } else { 0.0 }
        };
        let base = DetectorSettings { freq: 150.0, threshold_db: -6.0, ..Default::default() };
        let with_gap = hits(bursts, 48_000, &DetectorSettings { retrigger: 0.1, ..base });
        let without = hits(bursts, 48_000, &DetectorSettings { retrigger: 0.03, ..base });
        assert_eq!(without.len(), 17, "every burst: {without:?}");
        assert_eq!(with_gap.len(), 9, "every other burst: {with_gap:?}");
        assert!(with_gap.windows(2).all(|w| w[1] - w[0] >= (0.1 * SR) as usize), "{with_gap:?}");
    }

    #[test]
    fn frequency_changes_take_effect() {
        let mut d = KickDetector::new(SR);
        let s = DetectorSettings { freq: 200.0, ..Default::default() };
        d.process(0.0, &s);
        assert_eq!(d.coefs, SvfCoefs::band_pass(200.0, DETECTOR_Q, SR));
    }
}
