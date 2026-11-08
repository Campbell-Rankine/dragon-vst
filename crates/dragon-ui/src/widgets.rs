//! Small shared widgets: titled panels, selector chips and ON/OFF pills.

use crate::theme;
use nih_plug::prelude::{BoolParam, ParamSetter};
use nih_plug_egui::egui::{self, pos2, vec2, Color32, Stroke, Ui};

/// Inner margin and border width of a [`panel`].
const PANEL_MARGIN: f32 = 10.0;
const PANEL_STROKE: f32 = 1.5;

/// A titled, peach-bordered panel whose contents are laid out in a row.
/// Returns the panel's response (its `rect` is the outer border).
pub fn panel(ui: &mut Ui, title: &str, title_color: Color32, add_contents: impl FnOnce(&mut Ui)) -> egui::Response {
    panel_sized(ui, title, title_color, None, |_| {}, add_contents)
}

/// [`panel`] with extra widgets (e.g. [`chip`]s) after the title.
pub fn panel_with_header(
    ui: &mut Ui,
    title: &str,
    title_color: Color32,
    add_header: impl FnOnce(&mut Ui),
    add_contents: impl FnOnce(&mut Ui),
) -> egui::Response {
    panel_sized(ui, title, title_color, None, add_header, add_contents)
}

/// The most general panel: a header after the title, and an optional outer
/// `size` (border included) so panels can line up in a grid. Contents
/// that need more room than `size` still get it.
pub fn panel_sized(
    ui: &mut Ui,
    title: &str,
    title_color: Color32,
    size: Option<egui::Vec2>,
    add_header: impl FnOnce(&mut Ui),
    add_contents: impl FnOnce(&mut Ui),
) -> egui::Response {
    egui::Frame::new()
        .fill(theme::PANEL)
        .stroke(Stroke::new(PANEL_STROKE, theme::PEACH))
        .corner_radius(8.0)
        .inner_margin(PANEL_MARGIN)
        .show(ui, |ui| {
            if let Some(size) = size {
                let inner = size - egui::Vec2::splat(2.0 * (PANEL_MARGIN + PANEL_STROKE));
                ui.set_min_size(inner);
                ui.set_max_width(inner.x);
            }
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(title).size(13.0).strong().color(title_color));
                    ui.add_space(6.0);
                    add_header(ui);
                });
                ui.add_space(4.0);
                ui.horizontal(add_contents);
            });
        })
        .response
}

/// A selector chip, as the EQs' band chips: filled and lettered in `color`
/// when `lit` (in use), outlined in peach when `selected`. Returns the
/// response so the caller can act on clicks and add hover text.
pub fn chip(ui: &mut Ui, label: &str, color: Color32, lit: bool, selected: bool) -> egui::Response {
    let font = egui::FontId::proportional(11.0);
    let text_width = ui.fonts(|f| f.layout_no_wrap(label.to_owned(), font.clone(), color).size().x);
    let (rect, response) = ui.allocate_exact_size(vec2((text_width + 12.0).max(26.0), 18.0), egui::Sense::click());
    let fill = if lit { color.linear_multiply(0.3) } else { theme::PANEL };
    let stroke = if selected {
        Stroke::new(2.0_f32, theme::PEACH)
    } else if lit {
        Stroke::new(1.0_f32, color)
    } else {
        Stroke::new(1.0_f32, Color32::from_gray(60))
    };
    let painter = ui.painter();
    painter.rect(rect, 5.0, fill, stroke, egui::StrokeKind::Inside);
    painter.text(rect.center(), egui::Align2::CENTER_CENTER, label, font, if lit { color } else { theme::TEXT_DIM });
    response
}

/// A pill-shaped ON/OFF switch for a bool parameter, filled with `color`
/// when on.
pub fn on_toggle(ui: &mut Ui, param: &BoolParam, setter: &ParamSetter, color: Color32) {
    let (rect, response) = ui.allocate_exact_size(vec2(48.0, 22.0), egui::Sense::click());
    let on = param.value();
    if response.clicked() {
        set_bool(setter, param, !on);
    }
    let painter = ui.painter();
    let (fill, text_color) = if on { (color, theme::BACKGROUND) } else { (theme::BACKGROUND, theme::TEXT_DIM) };
    painter.rect(rect, 11.0, fill, Stroke::new(1.0_f32, if on { color } else { Color32::from_gray(70) }), egui::StrokeKind::Inside);
    painter.text(
        pos2(rect.center().x, rect.center().y),
        egui::Align2::CENTER_CENTER,
        if on { "ON" } else { "OFF" },
        egui::FontId::proportional(11.0),
        text_color,
    );
}

/// Set a bool parameter as one host automation gesture.
pub fn set_bool(setter: &ParamSetter, param: &BoolParam, value: bool) {
    setter.begin_set_parameter(param);
    setter.set_parameter(param, value);
    setter.end_set_parameter(param);
}
