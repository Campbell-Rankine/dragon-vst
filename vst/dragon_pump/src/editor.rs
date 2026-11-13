//! The egui editor (step 0, the skeleton): title, a SIGNAL panel with input
//! and sidechain meters and a routing status line, and the OUT panel. The
//! curve, trigger and shape panels arrive in step 2 (see
//! `docs/dragon_pump.md`).

use crate::meters::{MeterDisplay, Meters, FLOOR_DB};
use crate::params::DragonPumpParams;
use dragon_ui::theme;
use dragon_ui::widgets::panel;
use dragon_ui::Knob;
use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, Color32, Ui};
use nih_plug_egui::{create_egui_editor, EguiState};
use std::sync::Arc;
use std::time::Instant;

const METER_SIZE: egui::Vec2 = egui::vec2(28.0, 120.0);

/// Editor-only state: the meters' displayed levels.
struct EditorState {
    input: MeterDisplay,
    sidechain: MeterDisplay,
    last_frame: Instant,
}

pub fn create(
    params: Arc<DragonPumpParams>,
    egui_state: Arc<EguiState>,
    meters: Arc<Meters>,
) -> Option<Box<dyn Editor>> {
    let state = EditorState { input: MeterDisplay::default(), sidechain: MeterDisplay::default(), last_frame: Instant::now() };
    create_egui_editor(
        egui_state,
        state,
        |ctx, _| theme::apply_visuals(ctx),
        move |ctx, setter, state| {
            let dt = state.last_frame.elapsed().as_secs_f32().min(0.25);
            state.last_frame = Instant::now();
            let (input, sidechain) = meters.take();
            state.input.update(input, dt);
            state.sidechain.update(sidechain, dt);
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(16.0))
                .show(ctx, |ui| draw(ui, &params, setter, state));
        },
    )
}

fn draw(ui: &mut Ui, params: &DragonPumpParams, setter: &ParamSetter, state: &EditorState) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("DRAGON PUMP").size(20.0).strong().color(theme::PEACH));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new("drag to change · shift for fine · double-click to reset").color(theme::TEXT_DIM));
        });
    });
    ui.add_space(8.0);

    ui.horizontal_top(|ui| {
        panel(ui, "SIGNAL", theme::MINT, |ui| {
            meter(ui, "IN", &state.input, theme::CYAN);
            ui.add_space(8.0);
            meter(ui, "SIDECHAIN", &state.sidechain, theme::MINT);
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.set_max_width(300.0);
                ui.add_space(4.0);
                let (text, color) = if state.sidechain.active() {
                    ("Sidechain: receiving audio", theme::MINT)
                } else {
                    ("Sidechain: no signal", theme::TEXT_DIM)
                };
                ui.label(egui::RichText::new(text).strong().color(color));
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "Route a track (e.g. the kick) to this plugin's \"Sidechain\" input in the host. \
                         If the meter moves, the routing works.\n\n\
                         The ducking itself (tempo, MIDI and sidechain triggers, the curve) arrives in \
                         the next steps; for now audio passes through with the Output gain.",
                    )
                    .color(theme::TEXT_DIM),
                );
            });
        });
        panel(ui, "OUT", theme::OUTPUT, |ui| {
            ui.add(Knob::new(&params.output_gain, setter, "Output", theme::OUTPUT).bipolar());
        });
    });
}

/// A vertical level bar with its label and dB readout.
fn meter(ui: &mut Ui, label: &str, display: &MeterDisplay, color: Color32) {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(label).small().color(theme::TEXT_DIM));
        let (rect, _) = ui.allocate_exact_size(METER_SIZE, egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 3.0, theme::BACKGROUND);
        let fill = display.fill();
        if fill > 0.0 {
            let top = rect.bottom() - fill * rect.height();
            painter.rect_filled(egui::Rect::from_min_max(egui::pos2(rect.left(), top), rect.max), 3.0, color);
        }
        painter.rect_stroke(rect, 3.0, egui::Stroke::new(1.0_f32, Color32::from_gray(60)), egui::StrokeKind::Inside);
        let text = if display.db <= FLOOR_DB { "-inf".to_owned() } else { format!("{:.0}", display.db) };
        ui.label(egui::RichText::new(text).small().monospace().color(theme::TEXT_DIM));
    });
}
