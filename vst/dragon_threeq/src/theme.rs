//! Editor colours. Four base colours (mint, cyan, red, peach) plus shades.
//!
//! - Band colour follows the filter type's family: low-end types are mint,
//!   bell-like types cyan, high-end types and the notch red, tilt peach.
//! - The discrete knobs (Type, Slope, Stereo) use peach shades.

use crate::params::BandType;
use nih_plug_egui::egui::Color32;

pub const BACKGROUND: Color32 = Color32::from_rgb(0x12, 0x14, 0x18);
/// The background, lightened, with the same cool blue tint.
pub const PANEL: Color32 = Color32::from_rgb(0x24, 0x28, 0x30);
pub const TEXT_DIM: Color32 = Color32::from_gray(140);
pub const OFF: Color32 = Color32::from_gray(110);
pub const OUTPUT: Color32 = Color32::from_gray(200);

pub const MINT: Color32 = Color32::from_rgb(0x5c, 0xf2, 0xb0);
pub const MINT_DARK: Color32 = Color32::from_rgb(0x2f, 0xb9, 0x7f);

pub const CYAN: Color32 = Color32::from_rgb(0x00, 0xe5, 0xff);
pub const CYAN_LIGHT: Color32 = Color32::from_rgb(0x8a, 0xf3, 0xff);
pub const CYAN_DARK: Color32 = Color32::from_rgb(0x00, 0xa3, 0xba);

pub const RED: Color32 = Color32::from_rgb(0xff, 0x3b, 0x4e);
pub const RED_LIGHT: Color32 = Color32::from_rgb(0xff, 0x8a, 0x96);
pub const RED_DARK: Color32 = Color32::from_rgb(0xc4, 0x28, 0x38);

pub const PEACH: Color32 = Color32::from_rgb(0xff, 0xb3, 0x8a);
pub const PEACH_LIGHT: Color32 = Color32::from_rgb(0xff, 0xd6, 0xbf);
pub const PEACH_DARK: Color32 = Color32::from_rgb(0xe0, 0x8a, 0x5c);

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
