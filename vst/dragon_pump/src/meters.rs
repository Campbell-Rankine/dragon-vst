//! Input and sidechain level meters. The audio thread stores each buffer's
//! peak into atomics (no allocation, no locks); the editor takes them each
//! frame and draws them with a falling release.
//!
//! The sidechain meter is how a host's sidechain routing is checked: if it
//! moves, audio is arriving on the aux input.
//!
//! The same atomics carry the ducking state for the curve display: where
//! the playhead is on the curve, the host tempo, and the deepest gain since
//! the editor last looked (for the gain-reduction meter).

use nih_plug::prelude::AtomicF32;
use std::sync::atomic::{AtomicU32, Ordering};

/// Below this (dBFS) a meter counts as silent.
pub const FLOOR_DB: f32 = -70.0;
/// How fast a displayed meter falls (dB per second).
const RELEASE_DB_PER_SEC: f32 = 24.0;
/// How long after the last sidechain signal the status still says
/// "receiving" (seconds).
const SIGNAL_HOLD_SECS: f32 = 2.0;

/// The gain-reduction meter's range (dB of reduction).
pub const GR_RANGE_DB: f32 = 40.0;
/// How fast the displayed gain reduction falls back (dB per second). Fast,
/// so the bar follows the pump's recovery rather than smearing it.
const GR_RELEASE_DB_PER_SEC: f32 = 240.0;
/// Tempo shown before the audio thread has reported one.
pub const DEFAULT_BPM: f32 = 120.0;

/// Shared between the audio thread (writer) and the editor (reader).
pub struct Meters {
    input_peak: AtomicF32,
    sidechain_peak: AtomicF32,
    /// Lowest ducking gain (linear, before Output) since the last take.
    gain_low: AtomicF32,
    /// Playhead: seconds into the cycle (Tempo) or since the last trigger
    /// (MIDI), negative when there's none.
    pos: AtomicF32,
    /// Host tempo (or the default when it has none).
    bpm: AtomicF32,
    /// Audio mode: the detector's highest follower level since the last
    /// take (linear), and how many hits it has found (wrapping).
    detector_peak: AtomicF32,
    hits: AtomicU32,
}

impl Default for Meters {
    fn default() -> Self {
        Self {
            input_peak: AtomicF32::new(0.0),
            sidechain_peak: AtomicF32::new(0.0),
            gain_low: AtomicF32::new(1.0),
            pos: AtomicF32::new(-1.0),
            bpm: AtomicF32::new(DEFAULT_BPM),
            detector_peak: AtomicF32::new(0.0),
            hits: AtomicU32::new(0),
        }
    }
}

/// What the editor reads for the curve display each frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DuckState {
    /// Lowest gain since the last take (linear, 1 = no reduction).
    pub gain_low: f32,
    /// Playhead, seconds into the curve; `None` when not running (Tempo)
    /// or nothing has triggered yet (MIDI).
    pub pos: Option<f32>,
    pub bpm: f32,
}

impl Meters {
    /// Audio thread: one buffer's input peak (linear).
    pub fn record_input(&self, peak: f32) {
        raise(&self.input_peak, peak);
    }

    /// Audio thread: one buffer's sidechain peak (linear).
    pub fn record_sidechain(&self, peak: f32) {
        raise(&self.sidechain_peak, peak);
    }

    /// Audio thread: one buffer's ducking. `gain_low` is the buffer's lowest
    /// gain, `pos` the playhead at its end (seconds into the curve).
    pub fn record_duck(&self, gain_low: f32, pos: Option<f32>, bpm: f32) {
        if gain_low < self.gain_low.load(Ordering::Relaxed) {
            self.gain_low.store(gain_low, Ordering::Relaxed);
        }
        self.pos.store(pos.unwrap_or(-1.0), Ordering::Relaxed);
        self.bpm.store(bpm, Ordering::Relaxed);
    }

    /// Audio thread: one buffer's detector level (linear) and hit count.
    pub fn record_detector(&self, level: f32, hits: u32) {
        raise(&self.detector_peak, level);
        if hits > 0 {
            self.hits.fetch_add(hits, Ordering::Relaxed);
        }
    }

    /// Editor: the detector's peak level since the last call (linear) and
    /// the running hit count (compare with the last one to spot new hits).
    pub fn take_detector(&self) -> (f32, u32) {
        (self.detector_peak.swap(0.0, Ordering::Relaxed), self.hits.load(Ordering::Relaxed))
    }

    /// Editor: the peaks since the last call (linear), starting new ones.
    pub fn take(&self) -> (f32, f32) {
        (self.input_peak.swap(0.0, Ordering::Relaxed), self.sidechain_peak.swap(0.0, Ordering::Relaxed))
    }

    /// Editor: the ducking since the last call, starting a new low.
    pub fn take_duck(&self) -> DuckState {
        let pos = self.pos.load(Ordering::Relaxed);
        DuckState {
            gain_low: self.gain_low.swap(1.0, Ordering::Relaxed),
            pos: (pos >= 0.0).then_some(pos),
            bpm: self.bpm.load(Ordering::Relaxed),
        }
    }
}

/// Editor side: the gain-reduction meter's displayed reduction.
#[derive(Debug, Clone, Copy, Default)]
pub struct GrDisplay {
    /// Displayed reduction, dB, 0..=[`GR_RANGE_DB`].
    pub db: f32,
}

impl GrDisplay {
    /// Advance by `dt` seconds with the lowest gain since last time: jump
    /// down at once, recover at the release rate.
    pub fn update(&mut self, gain_low: f32, dt: f32) {
        let new_db = if gain_low > 0.0 { (-20.0 * gain_low.log10()).clamp(0.0, GR_RANGE_DB) } else { GR_RANGE_DB };
        self.db = new_db.max(self.db - GR_RELEASE_DB_PER_SEC * dt).max(0.0);
    }

    /// 0..1 fill for a bar hanging from the top.
    pub fn fill(&self) -> f32 {
        self.db / GR_RANGE_DB
    }
}

/// Store `peak` if it's above what's there (single writer, so no CAS loop).
fn raise(slot: &AtomicF32, peak: f32) {
    if peak > slot.load(Ordering::Relaxed) {
        slot.store(peak, Ordering::Relaxed);
    }
}

/// Editor side: one meter's displayed level.
#[derive(Debug, Clone, Copy)]
pub struct MeterDisplay {
    /// Displayed level, dBFS, never below [`FLOOR_DB`].
    pub db: f32,
    /// Seconds since the level was last above the floor.
    pub silent_for: f32,
}

impl Default for MeterDisplay {
    fn default() -> Self {
        Self { db: FLOOR_DB, silent_for: f32::INFINITY }
    }
}

impl MeterDisplay {
    /// Advance by `dt` seconds with a new peak (linear) since last time:
    /// jump up at once, fall at the release rate.
    pub fn update(&mut self, peak: f32, dt: f32) {
        let new_db = if peak > 0.0 { (20.0 * peak.log10()).max(FLOOR_DB) } else { FLOOR_DB };
        self.db = new_db.max(self.db - RELEASE_DB_PER_SEC * dt).max(FLOOR_DB);
        self.silent_for = if new_db > FLOOR_DB { 0.0 } else { self.silent_for + dt };
    }

    /// Whether signal arrived recently.
    pub fn active(&self) -> bool {
        self.silent_for < SIGNAL_HOLD_SECS
    }

    /// 0..1 fill for a bar from [`FLOOR_DB`] to 0 dBFS.
    pub fn fill(&self) -> f32 {
        ((self.db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_returns_the_highest_peak_once() {
        let m = Meters::default();
        m.record_input(0.2);
        m.record_input(0.5);
        m.record_input(0.1);
        m.record_sidechain(0.3);
        assert_eq!(m.take(), (0.5, 0.3));
        assert_eq!(m.take(), (0.0, 0.0));
    }

    #[test]
    fn display_jumps_up_and_falls_at_the_release_rate() {
        let mut d = MeterDisplay::default();
        assert!(!d.active());
        d.update(1.0, 0.016);
        assert_eq!(d.db, 0.0);
        assert!(d.active());
        d.update(0.0, 0.5);
        assert!((d.db + 12.0).abs() < 1e-4, "{}", d.db);
        // Silent for longer than the hold: no longer active.
        d.update(0.0, SIGNAL_HOLD_SECS);
        assert!(!d.active());
        assert_eq!(d.fill(), ((d.db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0));
    }

    #[test]
    fn duck_state_keeps_the_lowest_gain_and_the_latest_position() {
        let m = Meters::default();
        assert_eq!(m.take_duck(), DuckState { gain_low: 1.0, pos: None, bpm: DEFAULT_BPM });
        m.record_duck(0.5, Some(0.1), 90.0);
        m.record_duck(0.2, Some(0.2), 90.0);
        m.record_duck(0.7, Some(0.3), 90.0);
        assert_eq!(m.take_duck(), DuckState { gain_low: 0.2, pos: Some(0.3), bpm: 90.0 });
        // A new low starts; a stopped transport hides the playhead.
        m.record_duck(0.9, None, 90.0);
        assert_eq!(m.take_duck(), DuckState { gain_low: 0.9, pos: None, bpm: 90.0 });
    }

    #[test]
    fn detector_keeps_its_peak_and_counts_hits() {
        let m = Meters::default();
        m.record_detector(0.2, 0);
        m.record_detector(0.6, 1);
        m.record_detector(0.1, 2);
        assert_eq!(m.take_detector(), (0.6, 3));
        // The peak restarts; the count keeps running.
        assert_eq!(m.take_detector(), (0.0, 3));
    }

    #[test]
    fn gr_jumps_down_and_recovers_at_the_release_rate() {
        let mut gr = GrDisplay::default();
        gr.update(0.5, 0.016);
        assert!((gr.db - 6.0206).abs() < 1e-3);
        gr.update(0.0, 0.016);
        assert_eq!(gr.db, GR_RANGE_DB);
        assert_eq!(gr.fill(), 1.0);
        gr.update(1.0, 0.1);
        assert!((gr.db - (GR_RANGE_DB - 24.0)).abs() < 1e-4);
        gr.update(1.0, 1.0);
        assert_eq!(gr.db, 0.0);
    }

    #[test]
    fn display_never_drops_below_the_floor() {
        let mut d = MeterDisplay::default();
        d.update(1e-9, 100.0);
        assert_eq!(d.db, FLOOR_DB);
        assert_eq!(d.fill(), 0.0);
    }
}
