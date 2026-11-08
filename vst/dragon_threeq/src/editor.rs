//! The egui editor: one panel of knobs per band (Low, Mid, High), stacked,
//! with the output panel beside them.

use dragon_ui::widgets::{on_toggle, panel};
use dragon_ui::Knob;
use crate::params::{BandParams, BandSlope, DragonThreeqParams};
use crate::theme;
use dsp_core::FilterType;
use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, Ui};
use nih_plug_egui::{create_egui_editor, EguiState};
use std::sync::Arc;

/// Panel titles, in processing order.
const BAND_TITLES: [&str; 3] = ["LOW", "MID", "HIGH"];

pub fn create(params: Arc<DragonThreeqParams>, egui_state: Arc<EguiState>) -> Option<Box<dyn Editor>> {
    create_egui_editor(
        egui_state,
        (),
        |ctx, _| {
            let mut visuals = egui::Visuals::dark();
            visuals.panel_fill = theme::BACKGROUND;
            visuals.window_fill = theme::BACKGROUND;
            ctx.set_visuals(visuals);
        },
        move |ctx, setter, _| {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(16.0))
                .show(ctx, |ui| draw(ui, &params, setter));
        },
    )
}

fn draw(ui: &mut Ui, params: &DragonThreeqParams, setter: &ParamSetter) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("DRAGON THREEQ").size(20.0).strong().color(theme::PEACH));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new("drag to change · shift for fine · double-click to reset").color(theme::TEXT_DIM));
        });
    });
    ui.add_space(8.0);

    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            for (title, band) in BAND_TITLES.into_iter().zip(params.bands()) {
                band_panel(ui, title, band, setter);
            }
        });
        panel(ui, "OUT", theme::OUTPUT, |ui| {
            ui.add(Knob::new(&params.output_gain, setter, "Gain", theme::OUTPUT).bipolar());
        });
    });
}

/// One band's controls: ON/OFF, Freq / Gain / Q in the band's colour, then
/// Type / Slope / Stereo in peach shades. Controls that don't apply to the
/// band's type are greyed out.
fn band_panel(ui: &mut Ui, title: &str, band: &BandParams, setter: &ParamSetter) {
    let band_type = band.filter_type.value();
    let filter_type = FilterType::from(band_type);
    let on = band.on.value();
    let color = if on { theme::band_color(band_type) } else { theme::OFF };
    let q_applies = !(filter_type.uses_slope() && band.slope.value() == BandSlope::Db6);

    panel(ui, title, color, |ui| {
        ui.vertical(|ui| {
            ui.add_space(22.0);
            on_toggle(ui, &band.on, setter, theme::band_color(band_type));
        });
        ui.add(Knob::new(&band.freq, setter, "Freq", color));
        ui.add_enabled(filter_type.uses_gain(), Knob::new(&band.gain, setter, "Gain", color).bipolar());
        ui.add_enabled(q_applies, Knob::new(&band.q, setter, "Q", color));
        ui.separator();
        ui.add(Knob::new(&band.filter_type, setter, "Type", theme::PEACH));
        ui.add_enabled(filter_type.uses_slope(), Knob::new(&band.slope, setter, "Slope", theme::PEACH_LIGHT));
        ui.add(Knob::new(&band.placement, setter, "Stereo", theme::PEACH_DARK));
    });
}
