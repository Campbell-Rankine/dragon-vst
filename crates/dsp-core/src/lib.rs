//! Host-independent DSP building blocks for dragon-vst plugins.
//!
//! Nothing in this crate may depend on nih-plug, so it can be unit-tested
//! without a plugin host.

pub mod biquad;
pub mod eq;
pub mod matched;
pub mod svf;

pub use biquad::Biquad;
pub use eq::{analog_magnitude_db, BandDesign, BandFilter, BandSettings, FilterType, Slope, MAX_SECTIONS};
pub use svf::{Svf, SvfCoefs};

/// Convert decibels to linear gain. Values at or below -100 dB map to 0.0.
#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    if db > -100.0 {
        10f32.powf(db * 0.05)
    } else {
        0.0
    }
}

/// Convert linear gain to decibels, flooring at -100 dB.
#[inline]
pub fn gain_to_db(gain: f32) -> f32 {
    if gain > 1e-5 {
        20.0 * gain.log10()
    } else {
        -100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unity_gain_is_zero_db() {
        assert!((db_to_gain(0.0) - 1.0).abs() < 1e-6);
        assert!(gain_to_db(1.0).abs() < 1e-5);
    }

    #[test]
    fn round_trip() {
        for db in [-60.0f32, -12.0, -6.0, 3.0, 12.0, 24.0] {
            assert!((gain_to_db(db_to_gain(db)) - db).abs() < 1e-3, "{db}");
        }
    }

    #[test]
    fn silence_floor() {
        assert_eq!(db_to_gain(-120.0), 0.0);
        assert_eq!(gain_to_db(0.0), -100.0);
    }
}
