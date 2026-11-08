//! The egui editor: a row of 24 band chips, a panel of knobs for the selected
//! band, and the output panel.

use dragon_ui::widgets::{on_toggle, panel, set_bool};
use dragon_ui::Knob;
use crate::params::{BandParams, BandSlope, BandType, Dragon24EqParams, NUM_BANDS};
use crate::theme;
use dsp_core::FilterType;
use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, vec2, Color32, Sense, Stroke, Ui};
use nih_plug_egui::{create_egui_editor, EguiState};
use std::sync::Arc;

/// Editor-only state (not saved with the project).
#[derive(Default)]
pub struct EditorState {
    selected: usize,
}

pub fn create(params: Arc<Dragon24EqParams>, egui_state: Arc<EguiState>) -> Option<Box<dyn Editor>> {
    create_egui_editor(
        egui_state,
        EditorState::default(),
        |ctx, _| {
            let mut visuals = egui::Visuals::dark();
            visuals.panel_fill = theme::BACKGROUND;
            visuals.window_fill = theme::BACKGROUND;
            ctx.set_visuals(visuals);
        },
        move |ctx, setter, state| {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(16.0))
                .show(ctx, |ui| draw(ui, &params, setter, state));
        },
    )
}

fn draw(ui: &mut Ui, params: &Dragon24EqParams, setter: &ParamSetter, state: &mut EditorState) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("DRAGON 24EQ").size(20.0).strong().color(theme::PEACH));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new("click a band to edit · double-click to switch on/off").color(theme::TEXT_DIM));
        });
    });
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (i, band) in params.bands.iter().enumerate() {
            band_chip(ui, i, band, setter, &mut state.selected);
        }
    });
    ui.add_space(10.0);

    let index = state.selected.min(NUM_BANDS - 1);
    let band = &params.bands[index];
    let band_type = band.filter_type.value();
    let filter_type = FilterType::from(band_type);
    let on = band.on.value();
    let color = if on { theme::band_color(band_type) } else { theme::OFF };
    let q_applies = !(filter_type.uses_slope() && band.slope.value() == BandSlope::Db6);

    ui.horizontal_top(|ui| {
        panel(ui, &format!("BAND {}", index + 1), color, |ui| {
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
        panel(ui, "OUT", theme::OUTPUT, |ui| {
            ui.add(Knob::new(&params.output_gain, setter, "Gain", theme::OUTPUT).bipolar());
        });
    });
}

/// A numbered chip for one band: lit in the band's colour when on, peach
/// outline when selected. Click selects, double-click toggles on/off.
fn band_chip(ui: &mut Ui, index: usize, band: &BandParams, setter: &ParamSetter, selected: &mut usize) {
    let (rect, response) = ui.allocate_exact_size(vec2(28.0, 24.0), Sense::click());
    let on = band.on.value();
    let band_type = band.filter_type.value();
    let color = theme::band_color(band_type);

    if response.double_clicked() {
        set_bool(setter, &band.on, !on);
    }
    if response.clicked() || response.double_clicked() {
        *selected = index;
    }

    let is_selected = *selected == index;
    let fill = if on { color.linear_multiply(0.3) } else { theme::PANEL };
    let stroke = if is_selected {
        Stroke::new(2.0_f32, theme::PEACH)
    } else if on {
        Stroke::new(1.0_f32, color)
    } else {
        Stroke::new(1.0_f32, Color32::from_gray(60))
    };
    let painter = ui.painter();
    painter.rect(rect, 5.0, fill, stroke, egui::StrokeKind::Inside);
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        (index + 1).to_string(),
        egui::FontId::proportional(12.0),
        if on { color } else { theme::TEXT_DIM },
    );

    response.on_hover_text(if on {
        format!(
            "Band {} · {} · {} · {}",
            index + 1,
            band_type_name(band_type),
            band.freq.to_string(),
            if FilterType::from(band_type).uses_gain() { band.gain.to_string() } else { band.slope.to_string() },
        )
    } else {
        format!("Band {} (off)", index + 1)
    });
}

fn band_type_name(t: BandType) -> &'static str {
    BandType::variants()[t.to_index()]
}
