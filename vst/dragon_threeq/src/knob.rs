//! A rotary knob for nih-plug parameters, drawn with egui.
//!
//! Drag up/down to change (hold Shift for fine control), double-click to reset.

use nih_plug::prelude::{Param, ParamSetter};
use nih_plug_egui::egui::{
    self, pos2, vec2, Color32, Response, Sense, Shape, Stroke, Ui, Widget,
};
use std::f32::consts::PI;

/// Knob sweep: 270 degrees, starting bottom-left.
const START: f32 = 0.75 * PI;
const SWEEP: f32 = 1.5 * PI;
/// Normalized change per pixel of vertical drag.
const DRAG_SPEED: f32 = 0.005;
const FINE_FACTOR: f32 = 0.1;

pub struct Knob<'a, P: Param> {
    param: &'a P,
    setter: &'a ParamSetter<'a>,
    label: &'a str,
    color: Color32,
    diameter: f32,
    /// Draw the value arc from the middle (for bipolar params like gain).
    bipolar: bool,
}

impl<'a, P: Param> Knob<'a, P> {
    pub fn new(param: &'a P, setter: &'a ParamSetter<'a>, label: &'a str, color: Color32) -> Self {
        Self { param, setter, label, color, diameter: 64.0, bipolar: false }
    }

    pub fn bipolar(mut self) -> Self {
        self.bipolar = true;
        self
    }
}

fn arc_points(center: egui::Pos2, radius: f32, from: f32, to: f32) -> Vec<egui::Pos2> {
    let (from, to) = if from <= to { (from, to) } else { (to, from) };
    let steps = (((to - from) / SWEEP) * 48.0).ceil().max(1.0) as usize;
    (0..=steps)
        .map(|i| {
            let a = from + (to - from) * i as f32 / steps as f32;
            center + radius * vec2(a.cos(), a.sin())
        })
        .collect()
}

impl<P: Param> Widget for Knob<'_, P> {
    fn ui(self, ui: &mut Ui) -> Response {
        let text_h = 16.0;
        let size = vec2(self.diameter + 24.0, self.diameter + 2.0 * text_h + 8.0);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());

        // Interaction: wrap edits in begin/end so the host records one automation gesture.
        if response.double_clicked() {
            self.setter.begin_set_parameter(self.param);
            self.setter.set_parameter(self.param, self.param.default_plain_value());
            self.setter.end_set_parameter(self.param);
        } else {
            if response.drag_started() {
                self.setter.begin_set_parameter(self.param);
            }
            if response.dragged() {
                let fine = ui.input(|i| i.modifiers.shift);
                let delta = -response.drag_delta().y
                    * DRAG_SPEED
                    * if fine { FINE_FACTOR } else { 1.0 };
                if delta != 0.0 {
                    let value = (self.param.unmodulated_normalized_value() + delta).clamp(0.0, 1.0);
                    self.setter.set_parameter_normalized(self.param, value);
                }
            }
            if response.drag_stopped() {
                self.setter.end_set_parameter(self.param);
            }
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let center = pos2(rect.center().x, rect.top() + text_h + 4.0 + self.diameter / 2.0);
            let radius = self.diameter / 2.0;
            let norm = self.param.unmodulated_normalized_value();
            let angle = START + SWEEP * norm;
            let hovered = response.hovered() || response.dragged();

            // Label above.
            painter.text(
                pos2(rect.center().x, rect.top()),
                egui::Align2::CENTER_TOP,
                self.label,
                egui::FontId::proportional(13.0),
                Color32::from_gray(200),
            );

            // Body and track.
            painter.circle_filled(center, radius - 6.0, Color32::from_rgb(0x16, 0x18, 0x1d));
            painter.add(Shape::line(
                arc_points(center, radius, START, START + SWEEP),
                Stroke::new(5.0_f32, Color32::from_rgb(0x38, 0x3d, 0x47)),
            ));

            // Value arc in the band colour.
            let from = if self.bipolar { START + SWEEP * 0.5 } else { START };
            if (angle - from).abs() > 1e-3 {
                painter.add(Shape::line(
                    arc_points(center, radius, from, angle),
                    Stroke::new(5.0_f32, self.color),
                ));
            }

            // Pointer.
            let dir = vec2(angle.cos(), angle.sin());
            painter.line_segment(
                [center + dir * (radius * 0.25), center + dir * (radius - 9.0)],
                Stroke::new(3.0_f32, if hovered { Color32::WHITE } else { self.color }),
            );

            // Value below.
            painter.text(
                pos2(rect.center().x, rect.bottom()),
                egui::Align2::CENTER_BOTTOM,
                self.param.to_string(),
                egui::FontId::monospace(12.0),
                if hovered { self.color } else { Color32::from_gray(160) },
            );
        }

        response
    }
}
