//! EQ bands: filter types, cut slopes, and their design as a cascade of
//! second-order sections run in SVF form.
//!
//! - Bells, shelves, tilt, notch, band-pass and all-pass use magnitude-matched
//!   design ([`crate::matched`]): no cramping near Nyquist.
//! - Low/high cuts use the bilinear transform, pre-warped at the corner. A
//!   cascade of up to 8 matched sections accumulates passband ripple near
//!   Nyquist (+8 dB for a 96 dB/oct cut at 20 kHz), while the bilinear design
//!   keeps the Butterworth passband flat and the -3 dB corner exact; its only
//!   deviation is a deeper stopband approaching Nyquist, which is harmless.

use crate::biquad::Biquad;
use crate::matched::{bilinear, matched, matched_allpass, Analog2};
use crate::svf::{Svf, SvfCoefs};
use std::f64::consts::{FRAC_1_SQRT_2, PI};

/// Most sections any band needs: a 96 dB/oct cut is 16th order = 8 sections.
pub const MAX_SECTIONS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterType {
    Bell,
    LowShelf,
    HighShelf,
    /// High-pass. Uses [`Slope`].
    LowCut,
    /// Low-pass. Uses [`Slope`].
    HighCut,
    Notch,
    /// Unity gain at the centre frequency.
    BandPass,
    /// Tilts around the frequency: lows at −gain/2, highs at +gain/2.
    TiltShelf,
    /// Flat magnitude, phase shift around the frequency.
    AllPass,
}

impl FilterType {
    /// Whether the gain setting affects this type.
    pub fn uses_gain(self) -> bool {
        matches!(self, Self::Bell | Self::LowShelf | Self::HighShelf | Self::TiltShelf)
    }

    /// Whether the slope setting affects this type.
    pub fn uses_slope(self) -> bool {
        matches!(self, Self::LowCut | Self::HighCut)
    }
}

/// Cut-filter steepness. Each 6 dB/oct is one filter order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slope {
    Db6,
    Db12,
    Db18,
    Db24,
    Db30,
    Db36,
    Db48,
    Db72,
    Db96,
}

impl Slope {
    pub fn order(self) -> usize {
        match self {
            Self::Db6 => 1,
            Self::Db12 => 2,
            Self::Db18 => 3,
            Self::Db24 => 4,
            Self::Db30 => 5,
            Self::Db36 => 6,
            Self::Db48 => 8,
            Self::Db72 => 12,
            Self::Db96 => 16,
        }
    }

    pub const ALL: [Slope; 9] =
        [Self::Db6, Self::Db12, Self::Db18, Self::Db24, Self::Db30, Self::Db36, Self::Db48, Self::Db72, Self::Db96];
}

/// Everything that determines one band's response.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BandSettings {
    pub filter_type: FilterType,
    /// Hz. Clamped below Nyquist at design time.
    pub freq: f32,
    pub gain_db: f32,
    /// Bandwidth for bells/notches/band-passes, resonance for shelves and cuts.
    pub q: f32,
    pub slope: Slope,
}

/// The analog sections making up a band, normalised to the band frequency.
/// `shelf` sections may be designed inverted for accuracy; see [`BandDesign::new`].
#[derive(Clone, Copy)]
struct Prototypes {
    sections: [Analog2; MAX_SECTIONS],
    len: usize,
    shelf: bool,
    allpass: bool,
    /// Low/high cut: designed with the bilinear transform.
    cut: bool,
}

impl Prototypes {
    fn one(p: Analog2) -> Self {
        let mut sections = [p; MAX_SECTIONS];
        sections[0] = p;
        Self { sections, len: 1, shelf: false, allpass: false, cut: false }
    }

    fn build(s: &BandSettings) -> Self {
        let q = (s.q as f64).max(0.01);
        let a = 10f64.powf(s.gain_db as f64 / 40.0);
        let sa = a.sqrt();
        match s.filter_type {
            FilterType::Bell => Self::one(Analog2 { n: [1.0, a / q, 1.0], d: [1.0, 1.0 / (a * q), 1.0] }),
            FilterType::LowShelf => Self {
                shelf: true,
                ..Self::one(Analog2 { n: [a * a, a * sa / q, a], d: [1.0, sa / q, a] })
            },
            FilterType::HighShelf => Self {
                shelf: true,
                ..Self::one(Analog2 { n: [a, a * sa / q, a * a], d: [a, sa / q, 1.0] })
            },
            FilterType::TiltShelf => Self {
                shelf: true,
                // High shelf scaled by 1/A: DC at 1/A (−gain/2), HF at A (+gain/2).
                ..Self::one(Analog2 { n: [1.0, sa / q, a], d: [a, sa / q, 1.0] })
            },
            FilterType::Notch => Self::one(Analog2 { n: [1.0, 0.0, 1.0], d: [1.0, 1.0 / q, 1.0] }),
            FilterType::BandPass => Self::one(Analog2 { n: [0.0, 1.0 / q, 0.0], d: [1.0, 1.0 / q, 1.0] }),
            FilterType::AllPass => Self {
                allpass: true,
                ..Self::one(Analog2 { n: [1.0, -1.0 / q, 1.0], d: [1.0, 1.0 / q, 1.0] })
            },
            FilterType::LowCut | FilterType::HighCut => Self::butterworth(s.filter_type == FilterType::HighCut, s.slope, q),
        }
    }

    /// Butterworth cascade. The user Q scales the most resonant section, so
    /// Q = 0.707 is plain Butterworth and higher values add a peak at the corner.
    fn butterworth(lowpass: bool, slope: Slope, q: f64) -> Self {
        let order = slope.order();
        let pairs = order / 2;
        let odd = order % 2 == 1;
        let resonance = q / FRAC_1_SQRT_2;

        let mut out = Self { cut: true, ..Self::one(Analog2 { n: [1.0, 0.0, 0.0], d: [1.0, 1.0, 0.0] }) };
        out.len = 0;
        for k in 1..=pairs {
            let angle = if odd {
                k as f64 * PI / order as f64
            } else {
                (2 * k - 1) as f64 * PI / (2 * order) as f64
            };
            let mut qk = 1.0 / (2.0 * angle.cos());
            // The last pair has the largest angle, i.e. the highest Q.
            if k == pairs {
                qk *= resonance;
            }
            let n = if lowpass { [1.0, 0.0, 0.0] } else { [0.0, 0.0, 1.0] };
            out.sections[out.len] = Analog2 { n, d: [1.0, 1.0 / qk, 1.0] };
            out.len += 1;
        }
        if odd {
            let n = if lowpass { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
            out.sections[out.len] = Analog2 { n, d: [1.0, 1.0, 0.0] };
            out.len += 1;
        }
        out
    }
}

/// The ideal analog magnitude (dB) of a band at `freq` Hz. This is the target
/// the digital design is tested against, and what an EQ graph should draw.
pub fn analog_magnitude_db(settings: &BandSettings, freq: f64) -> f64 {
    let p = Prototypes::build(settings);
    let omega = freq / settings.freq as f64;
    // Sum per-section dB rather than multiplying, so steep cuts don't underflow.
    p.sections[..p.len].iter().map(|s| 10.0 * s.magnitude_sq(omega).max(1e-300).log10()).sum()
}

/// A designed band: matched biquads plus their SVF equivalents.
#[derive(Debug, Clone, Copy)]
pub struct BandDesign {
    biquads: [Biquad; MAX_SECTIONS],
    svf: [SvfCoefs; MAX_SECTIONS],
    len: usize,
}

impl BandDesign {
    /// A band that does nothing.
    pub const IDENTITY: Self = Self {
        biquads: [Biquad::IDENTITY; MAX_SECTIONS],
        svf: [SvfCoefs::IDENTITY; MAX_SECTIONS],
        len: 0,
    };

    pub fn new(settings: &BandSettings, sample_rate: f32) -> Self {
        let fs = sample_rate as f64;
        let f0 = (settings.freq as f64).clamp(1.0, 0.49 * fs);
        let protos = Prototypes::build(settings);

        let mut out = Self::IDENTITY;
        out.len = protos.len;
        for (i, proto) in protos.sections[..protos.len].iter().enumerate() {
            let bq = if protos.cut {
                bilinear(proto, f0, fs)
            } else if protos.allpass {
                matched_allpass(proto, f0, fs)
            } else if protos.shelf && proto.pole_freq() > proto.zero_freq() {
                // Poles above the zeros can land past Nyquist (e.g. a high
                // boost at 18 kHz), which impulse invariance can't represent.
                // Design the reciprocal shelf, whose poles are the lower pair,
                // and invert it.
                matched(&proto.swapped(), f0, fs).inverted()
            } else {
                matched(proto, f0, fs)
            };
            out.biquads[i] = bq;
            out.svf[i] = bq.to_svf();
        }
        out
    }

    /// The sections to run, in order.
    pub fn sections(&self) -> &[SvfCoefs] {
        &self.svf[..self.len]
    }

    /// Designed (digital) magnitude in dB at `freq` Hz.
    pub fn magnitude_db(&self, freq: f64, sample_rate: f64) -> f64 {
        let w = 2.0 * PI * freq / sample_rate;
        self.biquads[..self.len].iter().map(|b| 20.0 * b.magnitude(w).max(1e-300).log10()).sum()
    }

    pub fn is_stable(&self) -> bool {
        self.biquads[..self.len].iter().all(Biquad::is_stable)
    }
}

/// One channel's running state for a band.
#[derive(Debug, Clone, Copy, Default)]
pub struct BandFilter {
    state: [Svf; MAX_SECTIONS],
}

impl BandFilter {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    #[inline]
    pub fn process(&mut self, design: &BandDesign, mut x: f32) -> f32 {
        for (st, c) in self.state.iter_mut().zip(design.sections()) {
            x = st.process(c, x);
        }
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: [FilterType; 9] = [
        FilterType::Bell,
        FilterType::LowShelf,
        FilterType::HighShelf,
        FilterType::LowCut,
        FilterType::HighCut,
        FilterType::Notch,
        FilterType::BandPass,
        FilterType::TiltShelf,
        FilterType::AllPass,
    ];

    fn band(filter_type: FilterType, freq: f32, gain_db: f32, q: f32, slope: Slope) -> BandSettings {
        BandSettings { filter_type, freq, gain_db, q, slope }
    }

    /// Log-spaced test frequencies from 20 Hz to `max`.
    fn grid(max: f64) -> impl Iterator<Item = f64> {
        (0..=120).map(move |i| 20.0 * (max / 20.0f64).powf(i as f64 / 120.0))
    }

    /// Largest |digital - analog| in dB over the audible grid, ignoring
    /// points where the analog response is below `floor_db` (deep stopbands
    /// and notch bottoms, where dB errors are meaningless).
    fn max_error_db(s: &BandSettings, fs: f64, floor_db: f64) -> f64 {
        let d = BandDesign::new(s, fs as f32);
        grid(20_000.0f64.min(0.45 * fs))
            .filter(|&f| analog_magnitude_db(s, f) > floor_db)
            .map(|f| (d.magnitude_db(f, fs) - analog_magnitude_db(s, f)).abs())
            .fold(0.0, f64::max)
    }

    #[test]
    fn analog_butterworth_is_3db_down_at_corner() {
        for slope in Slope::ALL {
            for t in [FilterType::LowCut, FilterType::HighCut] {
                let s = band(t, 1_000.0, 0.0, 0.707_106_8, slope);
                let at = analog_magnitude_db(&s, 1_000.0);
                assert!((at + 3.0103).abs() < 0.01, "{t:?} {slope:?}: {at} dB");
            }
        }
    }

    #[test]
    fn cut_slopes_have_the_right_steepness() {
        for slope in Slope::ALL {
            let s = band(FilterType::HighCut, 100.0, 0.0, 0.707, slope);
            // One octave, far above the corner.
            let drop = analog_magnitude_db(&s, 1_600.0) - analog_magnitude_db(&s, 3_200.0);
            let expected = 6.0206 * slope.order() as f64;
            assert!((drop - expected).abs() < 0.05, "{slope:?}: {drop} dB/oct");
        }
    }

    #[test]
    fn digital_matches_analog_prototypes() {
        // (type, freq, gain, q, slope, tolerance dB)
        let cases = [
            (FilterType::Bell, 1_000.0, 12.0, 1.0, Slope::Db12, 0.1),
            (FilterType::Bell, 100.0, -18.0, 4.0, Slope::Db12, 0.1),
            (FilterType::Bell, 8_000.0, 6.0, 0.5, Slope::Db12, 0.5),
            (FilterType::LowShelf, 200.0, 9.0, 0.707, Slope::Db12, 0.1),
            (FilterType::LowShelf, 2_000.0, -12.0, 1.5, Slope::Db12, 0.3),
            (FilterType::HighShelf, 5_000.0, -9.0, 0.707, Slope::Db12, 0.3),
            (FilterType::HighShelf, 3_000.0, 12.0, 1.0, Slope::Db12, 0.3),
            (FilterType::TiltShelf, 1_000.0, 8.0, 0.707, Slope::Db12, 0.2),
            (FilterType::Notch, 1_000.0, 0.0, 2.0, Slope::Db12, 0.3),
            // Exact at the peak; drifts ~1 dB above it where it is already 15-20 dB down.
            (FilterType::BandPass, 2_000.0, 0.0, 1.0, Slope::Db12, 1.0),
            (FilterType::AllPass, 1_000.0, 0.0, 1.0, Slope::Db12, 1e-6),
            (FilterType::LowCut, 80.0, 0.0, 0.707, Slope::Db24, 0.2),
            (FilterType::LowCut, 30.0, 0.0, 2.0, Slope::Db96, 0.3),
            (FilterType::LowCut, 500.0, 0.0, 0.707, Slope::Db6, 0.2),
            // Cuts are bilinear: frequency warping makes the knee sharper near
            // Nyquist (less attenuation just below the corner), never a boost.
            (FilterType::HighCut, 2_000.0, 0.0, 0.707, Slope::Db48, 0.2),
            (FilterType::HighCut, 5_000.0, 0.0, 0.707, Slope::Db18, 0.2),
            (FilterType::HighCut, 10_000.0, 0.0, 0.707, Slope::Db12, 0.4),
            (FilterType::HighCut, 18_000.0, 0.0, 0.707, Slope::Db96, 1.7),
        ];
        let mut failures = Vec::new();
        for fs in [44_100.0, 48_000.0, 96_000.0] {
            for (t, f, g, q, slope, tol) in cases {
                let s = band(t, f, g, q, slope);
                // Cuts (bilinear) are held to the passband and corner; their
                // stopband is deliberately deeper than analog near Nyquist.
                let floor = if t.uses_slope() { -3.02 } else { -30.0 };
                let err = max_error_db(&s, fs, floor);
                if err >= tol {
                    failures.push(format!("{t:?} {f} Hz {g} dB Q{q} {slope:?} @ {fs}: error {err:.3} dB > {tol}"));
                }
            }
        }
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }

    #[test]
    fn no_cramping_near_nyquist() {
        // A +12 dB, Q 1 bell at 16 kHz / 44.1 kHz. The bilinear (RBJ) design
        // is off by 8.0 dB at 20 kHz because it is forced to 0 dB at Nyquist.
        // The matched design hits the peak exactly; its worst error is ~1.3 dB
        // in the lower skirt (the known limit of 3-point matching for wide,
        // high bells).
        let s = band(FilterType::Bell, 16_000.0, 12.0, 1.0, Slope::Db12);
        let d = BandDesign::new(&s, 44_100.0);
        let peak = d.magnitude_db(16_000.0, 44_100.0);
        assert!((peak - 12.0).abs() < 0.01, "peak {peak}");
        let at_20k = d.magnitude_db(20_000.0, 44_100.0) - analog_magnitude_db(&s, 20_000.0);
        assert!(at_20k.abs() < 1.0, "error at 20 kHz {at_20k}");
        let err = max_error_db(&s, 44_100.0, -30.0);
        assert!(err < 1.5, "max error {err}");
    }

    #[test]
    fn cuts_never_overshoot_near_nyquist() {
        // Regression: matched high-order cuts near Nyquist bumped the passband
        // by up to +8.5 dB per band. A Butterworth cut must never boost.
        for fs in [44_100.0, 48_000.0] {
            for slope in Slope::ALL {
                for t in [FilterType::LowCut, FilterType::HighCut] {
                    for f in [30.0, 1_000.0, 12_000.0, 16_000.0, 20_000.0] {
                        let s = band(t, f, 0.0, 0.707_106_8, slope);
                        let d = BandDesign::new(&s, fs as f32);
                        for probe in grid(0.45 * fs) {
                            let m = d.magnitude_db(probe, fs);
                            assert!(m < 0.01, "{t:?} {slope:?} @ {f} Hz, fs {fs}: {m:+.2} dB at {probe:.0} Hz");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn cut_corner_is_exactly_3db_down() {
        for slope in Slope::ALL {
            for t in [FilterType::LowCut, FilterType::HighCut] {
                for f in [30.0, 1_000.0, 18_000.0] {
                    let s = band(t, f, 0.0, 0.707_106_8, slope);
                    let at = BandDesign::new(&s, 48_000.0).magnitude_db(f as f64, 48_000.0);
                    assert!((at + 3.0103).abs() < 0.01, "{t:?} {slope:?} @ {f}: {at} dB");
                }
            }
        }
    }

    #[test]
    fn zero_gain_is_transparent() {
        for t in [FilterType::Bell, FilterType::LowShelf, FilterType::HighShelf, FilterType::TiltShelf] {
            for f in [30.0, 1_000.0, 15_000.0] {
                let s = band(t, f, 0.0, 0.707, Slope::Db12);
                let d = BandDesign::new(&s, 48_000.0);
                for probe in [20.0, 1_000.0, 20_000.0] {
                    let m = d.magnitude_db(probe, 48_000.0);
                    assert!(m.abs() < 1e-6, "{t:?} @ {f}: {m} dB at {probe}");
                }
            }
        }
    }

    #[test]
    fn notch_is_deep_at_centre() {
        let s = band(FilterType::Notch, 1_000.0, 0.0, 4.0, Slope::Db12);
        let d = BandDesign::new(&s, 48_000.0);
        assert!(d.magnitude_db(1_000.0, 48_000.0) < -60.0);
    }

    #[test]
    fn every_setting_is_stable_and_finite() {
        let freqs = [10.0, 40.0, 300.0, 3_000.0, 12_000.0, 19_000.0, 22_000.0];
        let gains = [-24.0, -6.0, 0.0, 6.0, 24.0];
        let qs = [0.1, 0.707, 3.0, 30.0];
        for fs in [44_100.0f32, 48_000.0, 96_000.0] {
            for t in TYPES {
                for &f in &freqs {
                    for &g in &gains {
                        for &q in &qs {
                            let slopes: &[Slope] = if t.uses_slope() { &Slope::ALL } else { &[Slope::Db12] };
                            for &slope in slopes {
                                let s = band(t, f, g, q, slope);
                                let d = BandDesign::new(&s, fs);
                                assert!(d.is_stable(), "unstable: {s:?} @ {fs}");
                                let mut filt = BandFilter::default();
                                let mut y = filt.process(&d, 1.0);
                                for _ in 0..2048 {
                                    y = filt.process(&d, 0.0);
                                }
                                assert!(y.is_finite(), "non-finite: {s:?} @ {fs}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn processing_matches_design_for_a_sine() {
        // End-to-end: SVF processing of a cascade gives the designed gain.
        let s = band(FilterType::LowCut, 200.0, 0.0, 0.707, Slope::Db48);
        let fs = 48_000.0f32;
        let d = BandDesign::new(&s, fs);
        for probe in [150.0f32, 400.0] {
            let mut filt = BandFilter::default();
            let n = 48_000;
            let mut peak = 0f32;
            for i in 0..n {
                let x = (2.0 * std::f32::consts::PI * probe * i as f32 / fs).sin();
                let y = filt.process(&d, x);
                if i > n / 2 {
                    peak = peak.max(y.abs());
                }
            }
            let measured = 20.0 * (peak as f64).log10();
            let designed = d.magnitude_db(probe as f64, fs as f64);
            assert!((measured - designed).abs() < 0.05, "{probe} Hz: {measured} vs {designed}");
        }
    }
}
