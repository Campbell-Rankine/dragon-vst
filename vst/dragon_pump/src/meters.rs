//! Input and sidechain level meters. The audio thread stores each buffer's
//! peak into atomics (no allocation, no locks); the editor takes them each
//! frame and draws them with a falling release.
//!
//! The sidechain meter is how a host's sidechain routing is checked: if it
//! moves, audio is arriving on the aux input.

use nih_plug::prelude::AtomicF32;
use std::sync::atomic::Ordering;

/// Below this (dBFS) a meter counts as silent.
pub const FLOOR_DB: f32 = -70.0;
/// How fast a displayed meter falls (dB per second).
const RELEASE_DB_PER_SEC: f32 = 24.0;
/// How long after the last sidechain signal the status still says
/// "receiving" (seconds).
const SIGNAL_HOLD_SECS: f32 = 2.0;

/// Shared between the audio thread (writer) and the editor (reader).
#[derive(Default)]
pub struct Meters {
    input_peak: AtomicF32,
    sidechain_peak: AtomicF32,
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

    /// Editor: the peaks since the last call (linear), starting new ones.
    pub fn take(&self) -> (f32, f32) {
        (self.input_peak.swap(0.0, Ordering::Relaxed), self.sidechain_peak.swap(0.0, Ordering::Relaxed))
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
    fn display_never_drops_below_the_floor() {
        let mut d = MeterDisplay::default();
        d.update(1e-9, 100.0);
        assert_eq!(d.db, FLOOR_DB);
        assert_eq!(d.fill(), 0.0);
    }
}
