//! The dragon-vst palette: four base colours (mint, cyan, red, peach) with
//! light and dark shades, on a dark, cool background.
//!
//! Plugins map these to their own meanings (e.g. the EQs colour a band by
//! its filter type), in their own `theme.rs`.

use nih_plug_egui::egui::{self, Color32, Stroke};

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

/// egui's own widgets (dropdowns, their popups, sliders, selectable
/// labels) in the palette: dark fills, grey outlines turning peach on
/// hover, peach selection.
pub fn apply_visuals(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BACKGROUND;
    v.window_fill = PANEL;
    v.window_stroke = Stroke::new(1.0_f32, PEACH);
    v.extreme_bg_color = BACKGROUND;
    v.selection.bg_fill = PEACH_DARK.linear_multiply(0.6);
    v.selection.stroke = Stroke::new(1.0_f32, PEACH);
    let w = &mut v.widgets;
    for (state, fill, stroke, text) in [
        (&mut w.inactive, BACKGROUND, Color32::from_gray(70), Color32::from_gray(210)),
        (&mut w.hovered, BACKGROUND, PEACH, PEACH_LIGHT),
        (&mut w.active, PANEL, PEACH, PEACH),
        (&mut w.open, PANEL, PEACH, PEACH),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0_f32, stroke);
        state.fg_stroke = Stroke::new(1.0_f32, text);
        state.corner_radius = 4.0.into();
    }
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT_DIM);
    ctx.set_visuals(v);
}
