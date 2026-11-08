//! Editor colours: the shared dragon-ui palette (mint, cyan, red, peach
//! and shades), plus this plugin's band colours.
//!
//! - Band colour follows the filter type's family: low-end types are mint,
//!   bell-like types cyan, high-end types and the notch red, tilt peach.
//! - The discrete knobs (Type, Slope, Stereo) use peach shades.

use crate::params::BandType;
use nih_plug_egui::egui::Color32;

pub use dragon_ui::theme::*;

pub fn band_color(t: BandType) -> Color32 {
    match t {
        BandType::LowShelf => MINT,
        BandType::LowCut => MINT_DARK,
        BandType::Bell => CYAN,
        BandType::BandPass => CYAN_LIGHT,
        BandType::AllPass => CYAN_DARK,
        BandType::HighShelf => RED,
        BandType::HighCut => RED_DARK,
        BandType::Notch => RED_LIGHT,
        BandType::TiltShelf => PEACH,
    }
}
