//! The egui editor:
//!
//! ```text
//! DRAGON PUMP  [TEMPO][MIDI][AUDIO]                     hint
//! ┌ CURVE [Pump][Soft]…[Breathe] ───────┐ ┌ TRIGGER ──────────┐
//! │ one cycle, floor, handles, playhead │ │ rate chips        │
//! │                                     │ │ (Offset) RUN STOP │
//! └─────────────────────────────────────┘ └───────────────────┘
//! ┌ SHAPE ─────────────────────────┐ ┌ OUT ─────────────────────┐
//! │ (Depth)(Attack)(Hold)(Rel)(Ten)│ │ (Mix)(Output) IN SC GR   │
//! └────────────────────────────────┘ └──────────────────────────┘
//! ```
//!
//! The TRIGGER panel shows the mode's controls (Tempo: rate chips, Offset,
//! Run When Stopped; MIDI: Velocity; Audio: the detector's Threshold, Freq,
//! Retrig, its level meter with a hit lamp, and Listen). The curve's handles are in `curve_view`; the
//! shape chips are `shapes::SHAPES`.

use crate::curve_view;
use crate::meters::{GrDisplay, MeterDisplay, Meters, FLOOR_DB};
use crate::params::{DragonPumpParams, Feel, Mode, NoteValue, Rate};
use crate::shapes::SHAPES;
use dragon_ui::theme;
use dragon_ui::widgets::{chip, on_toggle, panel_sized};
use dragon_ui::perf::{Meter, PerfStats};
use dragon_ui::Knob;
use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, vec2, Color32, Ui};
use nih_plug_egui::{create_egui_editor, EguiState};
use std::sync::Arc;
use std::time::Instant;

/// Outer sizes of the four panels (borders included), laid out to fill
/// the 760 x 440 window less its 16 px margin.
const CURVE_PANEL: egui::Vec2 = vec2(470.0, 216.0);
const TRIGGER_PANEL: egui::Vec2 = vec2(250.0, 216.0);
const SHAPE_PANEL: egui::Vec2 = vec2(440.0, 140.0);
const OUT_PANEL: egui::Vec2 = vec2(280.0, 140.0);
/// The curve plot inside its panel.
const CURVE_SIZE: egui::Vec2 = vec2(447.0, 168.0);
const KNOB: f32 = 52.0;
const METER_SIZE: egui::Vec2 = vec2(16.0, 70.0);
const GAP: f32 = 8.0;

/// Editor-only state: the meters' displayed levels.
struct EditorState {
    input: MeterDisplay,
    sidechain: MeterDisplay,
    gr: GrDisplay,
    /// Playhead, seconds into the curve.
    pos: Option<f32>,
    bpm: f32,
    /// Audio mode: the detector's level, the hit count last seen, and how
    /// long the hit lamp stays lit (s).
    detector: MeterDisplay,
    last_hits: u32,
    hit_flash: f32,
    /// The DSP / GUI load readout in the header.
    meter: Meter,
    last_frame: Instant,
}

/// How long the hit lamp lights for each hit (s).
const HIT_FLASH_SECS: f32 = 0.1;
/// The detector meter's range (dBFS).
const DETECTOR_FLOOR_DB: f32 = -60.0;
const DETECTOR_METER: egui::Vec2 = vec2(150.0, 10.0);

pub fn create(
    params: Arc<DragonPumpParams>,
    egui_state: Arc<EguiState>,
    meters: Arc<Meters>,
    perf: Arc<PerfStats>,
) -> Option<Box<dyn Editor>> {
    let state = EditorState {
        input: MeterDisplay::default(),
        sidechain: MeterDisplay::default(),
        gr: GrDisplay::default(),
        pos: None,
        bpm: crate::meters::DEFAULT_BPM,
        detector: MeterDisplay::default(),
        last_hits: 0,
        hit_flash: 0.0,
        meter: Meter::new(perf),
        last_frame: Instant::now(),
    };
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
            let duck = meters.take_duck();
            state.gr.update(duck.gain_low, dt);
            state.pos = duck.pos;
            state.bpm = duck.bpm;
            let (detector, hits) = meters.take_detector();
            state.detector.update(detector, dt);
            state.hit_flash = if hits != state.last_hits { HIT_FLASH_SECS } else { (state.hit_flash - dt).max(0.0) };
            state.last_hits = hits;
            let started = Instant::now();
            show(ctx, &params, setter, state);
            state.meter.frame(started.elapsed().as_secs_f32() * 1e3);
        },
    )
}

fn show(ctx: &egui::Context, params: &DragonPumpParams, setter: &ParamSetter, state: &EditorState) {
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(16.0))
        .show(ctx, |ui| {
            draw(ui, params, setter, state, params.mode.value());
        });
}

/// The whole editor, laid out for `mode` (the Mode parameter; passed in
/// so the layout test can try every mode).
fn draw(ui: &mut Ui, params: &DragonPumpParams, setter: &ParamSetter, state: &EditorState, mode: Mode) -> HeaderEdges {
    ui.spacing_mut().item_spacing = vec2(GAP, GAP);
    let header = header(ui, params, setter, state, mode);
    ui.horizontal_top(|ui| {
        panel_sized(
            ui,
            "CURVE",
            theme::CYAN,
            Some(CURVE_PANEL),
            |ui| shape_chips(ui, params, setter, mode),
            |ui| curve(ui, params, setter, state, mode),
        );
        panel_sized(ui, "TRIGGER", theme::PEACH, Some(TRIGGER_PANEL), |_| {}, |ui| match mode {
            Mode::Tempo => tempo_trigger(ui, params, setter, state),
            Mode::Midi => midi_trigger(ui, params, setter, state),
            Mode::Audio => audio_trigger(ui, params, setter, state),
        });
    });
    ui.horizontal_top(|ui| {
        panel_sized(ui, "SHAPE", theme::CYAN, Some(SHAPE_PANEL), |_| {}, |ui| {
            ui.add(Knob::new(&params.depth, setter, "Depth", theme::RED).diameter(KNOB));
            ui.add(Knob::new(&params.attack, setter, "Attack", theme::CYAN_LIGHT).diameter(KNOB));
            // Tempo mode's Hold and Release are fractions of the cycle,
            // the others' are in ms: separate parameters, one pair shown.
            let (hold, release) =
                if mode.is_cyclic() { (&params.hold_pct, &params.release_pct) } else { (&params.hold_ms, &params.release_ms) };
            ui.add(Knob::new(hold, setter, "Hold", theme::CYAN).diameter(KNOB));
            ui.add(Knob::new(release, setter, "Release", theme::CYAN).diameter(KNOB));
            ui.add(Knob::new(&params.tension, setter, "Tension", theme::CYAN_DARK).bipolar().diameter(KNOB));
        });
        panel_sized(ui, "OUT", theme::OUTPUT, Some(OUT_PANEL), |_| {}, |ui| {
            ui.add(Knob::new(&params.mix, setter, "Mix", theme::OUTPUT).diameter(KNOB));
            ui.add(Knob::new(&params.output_gain, setter, "Output", theme::OUTPUT).bipolar().diameter(KNOB));
            ui.add_space(4.0);
            level_meter(ui, "IN", &state.input, theme::CYAN, "Input level");
            let sc_hover = if state.sidechain.active() {
                "Sidechain: receiving audio"
            } else {
                "Sidechain: no signal. Route a track (e.g. the kick) to this plugin's \"Sidechain\" input; \
                 if this meter moves, the routing works."
            };
            level_meter(ui, "SC", &state.sidechain, theme::MINT, sc_hover);
            gr_meter(ui, &state.gr);
        });
    });
    header
}

/// Where the header's left group (title, mode chips) ends and its right
/// group (hint, load meter) starts, so the layout test can check they
/// don't collide.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(not(test), allow(dead_code))] // read by the layout test
struct HeaderEdges {
    left_end: f32,
    right_start: f32,
}

fn header(ui: &mut Ui, params: &DragonPumpParams, setter: &ParamSetter, state: &EditorState, mode: Mode) -> HeaderEdges {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("DRAGON PUMP").size(20.0).strong().color(theme::PEACH));
        ui.add_space(8.0);
        for (m, name, hint) in [
            (Mode::Tempo, "TEMPO", "Dip on every beat of the Rate, locked to the host"),
            (Mode::Midi, "MIDI", "Dip on every MIDI note-on (e.g. the kick's pattern on this track)"),
            (Mode::Audio, "AUDIO", "Dip on every kick arriving at the Sidechain input"),
        ] {
            if chip(ui, name, theme::PEACH, m == mode, m == mode).on_hover_text(hint).clicked() && m != mode {
                setter.begin_set_parameter(&params.mode);
                setter.set_parameter(&params.mode, m);
                setter.end_set_parameter(&params.mode);
            }
        }
        let left_end = ui.min_rect().right();
        let right_start = ui
            .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                state.meter.show(ui, "the ducker");
                ui.add_space(12.0);
                ui.label(egui::RichText::new("shift: fine · double-click: reset").small().color(theme::TEXT_DIM));
                ui.min_rect().left()
            })
            .inner;
        HeaderEdges { left_end, right_start }
    })
    .inner
}

/// One chip per shape preset, lit when the curve is at that shape.
fn shape_chips(ui: &mut Ui, params: &DragonPumpParams, setter: &ParamSetter, mode: Mode) {
    ui.spacing_mut().item_spacing.x = 5.0;
    for shape in &SHAPES {
        let lit = shape.matches(params, mode);
        if chip(ui, shape.name, theme::CYAN, lit, false).on_hover_text(shape.hint).clicked() && !lit {
            shape.apply(params, mode, setter);
        }
    }
}

fn curve(ui: &mut Ui, params: &DragonPumpParams, setter: &ParamSetter, state: &EditorState, mode: Mode) {
    let rate = params.rate.value();
    let cycle_secs = (rate.beats() * 60.0 / state.bpm.max(1.0) as f64) as f32;
    let status = match mode {
        Mode::Tempo if state.pos.is_some() => format!("{} · {:.0} BPM", params.rate, state.bpm),
        Mode::Tempo => format!("{} · {:.0} BPM · stopped", params.rate, state.bpm),
        Mode::Midi | Mode::Audio => {
            let shape = curve_view::display_shape(params, mode, cycle_secs);
            let span = curve_view::View::for_mode(mode, cycle_secs, &shape).span;
            let from = if mode == Mode::Midi { "note-on" } else { "kick" };
            format!("from each {from} · {} ms shown", (span * 1000.0).round())
        }
    };
    curve_view::show(ui, CURVE_SIZE, params, setter, mode, cycle_secs, state.pos, &status);
}

fn tempo_trigger(ui: &mut Ui, params: &DragonPumpParams, setter: &ParamSetter, state: &EditorState) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        let (value, feel) = params.rate.value().split();
        small_label(ui, "RATE");
        ui.horizontal(|ui| {
            for v in NoteValue::ALL {
                if chip(ui, v.label(), theme::PEACH, v == value, false).clicked() && v != value {
                    set_rate(setter, params, Rate::join(v, feel));
                }
            }
        });
        ui.horizontal(|ui| {
            for f in Feel::ALL {
                // 1/1 only comes straight.
                let usable = value != NoteValue::Whole || f == Feel::Straight;
                let response = ui.add_enabled_ui(usable, |ui| chip(ui, f.label(), theme::PEACH_LIGHT, f == feel, false)).inner;
                if response.clicked() && f != feel {
                    set_rate(setter, params, Rate::join(value, f));
                }
            }
        });
        ui.add_space(2.0);
        ui.horizontal_top(|ui| {
            ui.add(Knob::new(&params.offset, setter, "Offset", theme::PEACH_LIGHT).bipolar().diameter(KNOB));
            ui.vertical(|ui| {
                small_label(ui, "RUN STOPPED")
                    .on_hover_text("Keep pumping at the host tempo while the transport is stopped");
                on_toggle(ui, &params.run_stopped, setter, theme::PEACH);
                ui.add_space(4.0);
                sidechain_status(ui, state);
            });
        });
    });
}

fn midi_trigger(ui: &mut Ui, params: &DragonPumpParams, setter: &ParamSetter, state: &EditorState) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        ui.label(
            egui::RichText::new(
                "Every note-on starts a dip, on its exact sample; note-offs are ignored. \
                 Put the kick's MIDI pattern on this track, or copy it here.",
            )
            .small()
            .color(theme::TEXT_DIM),
        );
        ui.add_space(2.0);
        ui.horizontal_top(|ui| {
            ui.add(Knob::new(&params.velocity, setter, "Velocity", theme::PEACH).diameter(KNOB))
                .on_hover_text("How much a note's velocity scales its depth. 0 % = every note dips fully");
            ui.vertical(|ui| {
                ui.add_space(4.0);
                sidechain_status(ui, state);
            });
        });
    });
}

fn audio_trigger(ui: &mut Ui, params: &DragonPumpParams, setter: &ParamSetter, state: &EditorState) {
    const SMALL_KNOB: f32 = 44.0;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        ui.horizontal_top(|ui| {
            ui.add(Knob::new(&params.sc_threshold, setter, "Threshold", theme::MINT).diameter(SMALL_KNOB))
                .on_hover_text("A kick counts when the detector rises through this level (the white mark below)");
            ui.add(Knob::new(&params.sc_freq, setter, "Freq", theme::MINT).diameter(SMALL_KNOB))
                .on_hover_text("The detector listens around this frequency: set it to the kick's thump (Listen helps)");
            ui.add(Knob::new(&params.sc_retrigger, setter, "Retrig", theme::MINT_DARK).diameter(SMALL_KNOB))
                .on_hover_text("Hits closer together than this are ignored (flams, a kick's bounce)");
        });
        ui.horizontal(|ui| {
            detector_meter(ui, &state.detector, params.sc_threshold.value());
            hit_lamp(ui, state.hit_flash > 0.0);
        });
        ui.horizontal(|ui| {
            small_label(ui, "LISTEN").on_hover_text("Hear what the detector hears (the band-passed sidechain) to tune Freq");
            on_toggle(ui, &params.sc_listen, setter, theme::MINT);
            ui.add_space(4.0);
            sidechain_status(ui, state);
        });
    });
}

/// The detector's level (-60..0 dBFS) as a horizontal bar, with a white
/// mark at the threshold.
fn detector_meter(ui: &mut Ui, display: &MeterDisplay, threshold_db: f32) {
    let (rect, response) = ui.allocate_exact_size(DETECTOR_METER, egui::Sense::hover());
    let x_at = |db: f32| rect.left() + ((db - DETECTOR_FLOOR_DB) / -DETECTOR_FLOOR_DB).clamp(0.0, 1.0) * rect.width();
    let painter = ui.painter_at(rect.expand(2.0));
    painter.rect_filled(rect, 2.0, theme::BACKGROUND);
    let above = display.db >= threshold_db;
    let level = egui::Rect::from_min_max(rect.min, egui::pos2(x_at(display.db), rect.bottom()));
    painter.rect_filled(level, 2.0, if above { theme::MINT } else { theme::MINT_DARK.linear_multiply(0.6) });
    let tx = x_at(threshold_db);
    painter.line_segment([egui::pos2(tx, rect.top() - 2.0), egui::pos2(tx, rect.bottom() + 2.0)], egui::Stroke::new(2.0_f32, Color32::WHITE));
    painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.0_f32, Color32::from_gray(60)), egui::StrokeKind::Inside);
    response.on_hover_text(format!("Detector level {:.0} dB · threshold {:.1} dB", display.db.max(DETECTOR_FLOOR_DB), threshold_db));
}

/// Lights for a moment on each detected kick.
fn hit_lamp(ui: &mut Ui, lit: bool) {
    let (rect, response) = ui.allocate_exact_size(vec2(12.0, 12.0), egui::Sense::hover());
    let color = if lit { theme::MINT } else { theme::BACKGROUND };
    ui.painter().circle(rect.center(), 5.0, color, egui::Stroke::new(1.0_f32, theme::MINT_DARK));
    response.on_hover_text("Lights on each detected kick");
}

/// Whether audio is arriving on the Sidechain input, for checking the
/// host's routing.
fn sidechain_status(ui: &mut Ui, state: &EditorState) {
    let (text, color) =
        if state.sidechain.active() { ("SIDECHAIN: IN", theme::MINT) } else { ("SIDECHAIN: NONE", theme::TEXT_DIM) };
    ui.label(egui::RichText::new(text).small().strong().color(color)).on_hover_text(
        "Whether audio is arriving on the Sidechain input (in LYNX: the SC menu at the top \
         right of the plugin window, or Wrapper Settings > Processing).",
    );
}

fn small_label(ui: &mut Ui, text: &str) -> egui::Response {
    ui.label(egui::RichText::new(text).small().color(theme::TEXT_DIM))
}

/// Set the rate as one automation gesture.
fn set_rate(setter: &ParamSetter, params: &DragonPumpParams, rate: Rate) {
    setter.begin_set_parameter(&params.rate);
    setter.set_parameter(&params.rate, rate);
    setter.end_set_parameter(&params.rate);
}

/// A narrow vertical level bar under its label, filling up from the bottom.
fn level_meter(ui: &mut Ui, label: &str, display: &MeterDisplay, color: Color32, hover: &str) {
    let db = if display.db <= FLOOR_DB { "-inf dB".to_owned() } else { format!("{:.0} dB", display.db) };
    meter_bar(ui, label, display.fill(), false, color).on_hover_text(format!("{hover}\n{db}"));
}

/// The gain-reduction bar: hangs from the top, red.
fn gr_meter(ui: &mut Ui, gr: &GrDisplay) {
    meter_bar(ui, "GR", gr.fill(), true, theme::RED)
        .on_hover_text(format!("Gain reduction: {:.1} dB (bar spans 0 to {:.0} dB)", gr.db, crate::meters::GR_RANGE_DB));
}

fn meter_bar(ui: &mut Ui, label: &str, fill: f32, from_top: bool, color: Color32) -> egui::Response {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 4.0);
        small_label(ui, label);
        let (rect, response) = ui.allocate_exact_size(METER_SIZE, egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 3.0, theme::BACKGROUND);
        if fill > 0.0 {
            let h = fill.clamp(0.0, 1.0) * rect.height();
            let bar = if from_top {
                egui::Rect::from_min_size(rect.min, vec2(rect.width(), h))
            } else {
                egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - h), rect.max)
            };
            painter.rect_filled(bar, 3.0, color);
        }
        painter.rect_stroke(rect, 3.0, egui::Stroke::new(1.0_f32, Color32::from_gray(60)), egui::StrokeKind::Inside);
        response
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{HEIGHT, WIDTH};

    /// A host stand-in: parameter changes go nowhere.
    struct NoHost;

    impl GuiContext for NoHost {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        fn request_resize(&self) -> bool {
            false
        }
        unsafe fn raw_begin_set_parameter(&self, _: ParamPtr) {}
        unsafe fn raw_set_parameter_normalized(&self, _: ParamPtr, _: f32) {}
        unsafe fn raw_end_set_parameter(&self, _: ParamPtr) {}
        fn get_state(&self) -> PluginState {
            unimplemented!()
        }
        fn set_state(&self, _: PluginState) {}
    }

    /// Lay the editor out headless at its fixed size and return the area
    /// its contents used.
    fn used_rect(state: &EditorState, mode: Mode) -> (egui::Rect, HeaderEdges) {
        let params = DragonPumpParams::default();
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let ctx = egui::Context::default();
        theme::apply_visuals(&ctx);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(WIDTH as f32, HEIGHT as f32))),
            ..Default::default()
        };
        let mut used = egui::Rect::NOTHING;
        let mut edges = HeaderEdges { left_end: 0.0, right_start: 0.0 };
        // Twice: egui settles some sizes (e.g. right-to-left layouts) on the
        // second pass.
        for _ in 0..2 {
            let _ = ctx.run(input.clone(), |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(16.0))
                    .show(ctx, |ui| {
                        edges = draw(ui, &params, &setter, state, mode);
                        used = ui.min_rect();
                    });
            });
        }
        (used, edges)
    }

    /// Room (px) the header must leave between its left and right groups:
    /// about 7 more characters of the load readout (e.g. "DSP 100% max
    /// 999%" instead of "DSP 0% max 0%").
    const HEADER_SLACK: f32 = 50.0;

    #[test]
    fn everything_fits_the_window() {
        let state = EditorState {
            input: MeterDisplay::default(),
            sidechain: MeterDisplay::default(),
            gr: GrDisplay::default(),
            pos: Some(0.1),
            bpm: 120.0,
            detector: MeterDisplay::default(),
            last_hits: 0,
            hit_flash: 0.1,
            meter: Meter::default(),
            last_frame: Instant::now(),
        };
        let inner = egui::Rect::from_min_size(egui::pos2(16.0, 16.0), vec2(WIDTH as f32 - 32.0, HEIGHT as f32 - 32.0));
        for mode in [Mode::Tempo, Mode::Midi, Mode::Audio] {
            let (used, header) = used_rect(&state, mode);
            assert!(inner.contains_rect(used), "{mode:?}: contents {used:?} overflow {inner:?}");
            let gap = header.right_start - header.left_end;
            assert!(gap >= HEADER_SLACK, "{mode:?}: header groups only {gap} px apart");
        }
    }
}
