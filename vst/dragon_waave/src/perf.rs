//! The load meter in the editor's header: how much of each buffer's time
//! `process()` used, and how long the editor takes to build a frame. It's
//! there to diagnose dropouts in a real host, where a profiler is awkward.
//!
//! The audio thread only stores into atomics (no allocation, no locks).
//! Measuring costs two clock reads per `process()` call.

use nih_plug::prelude::AtomicF32;
use std::sync::atomic::Ordering;
use std::time::Instant;

/// How quickly the averages follow changes (per update).
const SMOOTHING: f32 = 0.05;

/// Shared between the audio thread (writer) and the editor (reader).
#[derive(Default)]
pub struct PerfStats {
    /// Smoothed share of the buffer's real-time budget `process()` used, 0..1+.
    audio_load: AtomicF32,
    /// Highest share since the editor last took it.
    audio_peak: AtomicF32,
}

impl PerfStats {
    /// Audio thread: one `process()` call that started at `start` and
    /// rendered `samples` at `sample_rate`.
    pub fn record(&self, start: Instant, samples: usize, sample_rate: f32) {
        if samples == 0 || sample_rate <= 0.0 {
            return;
        }
        let budget = samples as f32 / sample_rate;
        let load = start.elapsed().as_secs_f32() / budget;
        let avg = self.audio_load.load(Ordering::Relaxed);
        self.audio_load.store(avg + SMOOTHING * (load - avg), Ordering::Relaxed);
        if load > self.audio_peak.load(Ordering::Relaxed) {
            self.audio_peak.store(load, Ordering::Relaxed);
        }
    }

    pub fn audio_load(&self) -> f32 {
        self.audio_load.load(Ordering::Relaxed)
    }

    /// The peak since the last call, which starts a new one.
    pub fn take_audio_peak(&self) -> f32 {
        self.audio_peak.swap(0.0, Ordering::Relaxed)
    }
}

/// Editor side: frame times and the audio peak, held for a second so the
/// numbers are readable.
pub struct Meter {
    pub stats: std::sync::Arc<PerfStats>,
    /// Smoothed time to build a frame, in ms.
    pub frame_ms: f32,
    frame_peak: f32,
    held_audio_peak: f32,
    held_frame_peak: f32,
    last_hold: Instant,
}

impl Default for Meter {
    fn default() -> Self {
        Self {
            stats: Default::default(),
            frame_ms: 0.0,
            frame_peak: 0.0,
            held_audio_peak: 0.0,
            held_frame_peak: 0.0,
            last_hold: Instant::now(),
        }
    }
}

impl Meter {
    /// Record one frame that took `ms` to build.
    pub fn frame(&mut self, ms: f32) {
        self.frame_ms += SMOOTHING * (ms - self.frame_ms);
        self.frame_peak = self.frame_peak.max(ms);
        if self.last_hold.elapsed().as_secs_f32() >= 1.0 {
            self.held_audio_peak = self.stats.take_audio_peak();
            self.held_frame_peak = std::mem::take(&mut self.frame_peak);
            self.last_hold = Instant::now();
        }
    }

    /// e.g. "DSP 6% max 14% · GUI 2.1 ms max 4.0" (average, then peak).
    pub fn text(&self) -> String {
        format!(
            "DSP {:.0}% max {:.0}% · GUI {:.1} ms max {:.1}",
            100.0 * self.stats.audio_load(),
            100.0 * self.held_audio_peak,
            self.frame_ms,
            self.held_frame_peak
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_is_taken_once() {
        let s = PerfStats::default();
        s.audio_peak.store(0.5, Ordering::Relaxed);
        assert_eq!(s.take_audio_peak(), 0.5);
        assert_eq!(s.take_audio_peak(), 0.0);
    }

    #[test]
    fn record_tracks_load_against_the_budget() {
        let s = PerfStats::default();
        // A call that started long ago used more than its whole budget.
        let start = Instant::now() - std::time::Duration::from_millis(5);
        s.record(start, 48, 48_000.0); // 1 ms budget
        assert!(s.take_audio_peak() > 4.0);
        assert!(s.audio_load() > 0.0);
    }
}
