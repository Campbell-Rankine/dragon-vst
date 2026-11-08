//! The egui editor: one page, styled like Dragon 24EQ (shared `dragon-ui`
//! widgets). Rows, top to bottom:
//!
//! 1. OSC 1 | OSC 2
//! 2. MIXER | FILTER
//! 3. ENV (Amp / Filter / Mod, one shown at a time) | LFO (1 / 2)
//! 4. MOD MATRIX | VOICE | FX / OUT
//!
//! See `docs/dragon_waave_ui.md` for the plan this follows.

use crate::displays::{draw_envelope, draw_lfo};
use crate::params::{Read, 
    DestParam, DragonWaaveParams, EnvParams, FilterTypeParam, LfoModeParam, LfoParams, ModSlotParams, OscParams,
    SourceParam, VoiceModeParam, WaveParam,
};
use dragon_ui::theme;
use dragon_ui::widgets::{chip, on_toggle, panel_sized};
use dragon_ui::Knob;
use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, Color32, Ui};
use nih_plug_egui::widgets::ParamSlider;
use nih_plug_egui::{create_egui_editor, EguiState};
use std::sync::Arc;

/// Window size (fits a 13" laptop screen).
pub const WIDTH: u32 = 1220;
pub const HEIGHT: u32 = 760;
/// Space around the page.
const MARGIN: f32 = 12.0;
/// Knob diameter.
const KNOB: f32 = 40.0;

const ENV_NAMES: [&str; 3] = ["AMP", "FILT", "MOD"];
const ENV_TITLES: [&str; 3] = [
    "Amp envelope (ENV 1)",
    "Filter envelope (ENV 2)",
    "Mod envelope (ENV 3)",
];
const ENV_COLORS: [Color32; 3] = [theme::RED_LIGHT, theme::RED, theme::RED_DARK];

/// Editor-only state (not saved with the project): which envelope and LFO
/// the panels show.
#[derive(Default)]
pub struct EditorState {
    pub env: usize,
    pub lfo: usize,
    /// Each panel's outer rectangle from the last frame (row by row), for
    /// tests to check the grid lines up.
    pub panel_rects: Vec<egui::Rect>,
}

pub fn create(params: Arc<DragonWaaveParams>, egui_state: Arc<EguiState>) -> Option<Box<dyn Editor>> {
    create_egui_editor(
        egui_state,
        EditorState::default(),
        |ctx, _| setup(ctx),
        move |ctx, setter, state| {
            show(ctx, &params, setter, state);
        },
    )
}

fn setup(ctx: &egui::Context) {
    theme::apply_visuals(ctx);
}

// ----- What's in use (lights chips) -----

/// A matrix slot's settings, as plain values.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SlotView {
    pub source: SourceParam,
    pub dest: DestParam,
    pub amount: f32,
    pub via: SourceParam,
}

fn slot_views(params: &DragonWaaveParams) -> Vec<SlotView> {
    params
        .mod_slots()
        .iter()
        .map(|s| SlotView {
            source: s.source.value(),
            dest: s.dest.value(),
            amount: s.amount.value(),
            via: s.via.value(),
        })
        .collect()
}

/// Whether an active slot (a source, a destination and a non-zero amount)
/// uses `source` as its source or its via.
pub(crate) fn matrix_uses(slots: &[SlotView], source: SourceParam) -> bool {
    slots.iter().any(|s| {
        let active = s.source != SourceParam::None && s.dest != DestParam::None && s.amount != 0.0;
        active && (s.source == source || s.via == source)
    })
}

/// Whether each envelope affects the sound: Amp always; Filter through its
/// amount (with the filter on) or the matrix; Mod through the matrix.
pub(crate) fn envs_used(slots: &[SlotView], filter_on: bool, filter_env_amount: f32) -> [bool; 3] {
    [
        true,
        (filter_on && filter_env_amount != 0.0) || matrix_uses(slots, SourceParam::FilterEnv),
        matrix_uses(slots, SourceParam::ModEnv),
    ]
}

/// Whether each LFO affects the sound: LFO 1 through its routes or the
/// matrix; LFO 2 through the matrix.
pub(crate) fn lfos_used(slots: &[SlotView], lfo1_routes: [f32; 3]) -> [bool; 2] {
    [
        lfo1_routes.iter().any(|&a| a != 0.0) || matrix_uses(slots, SourceParam::Lfo1),
        matrix_uses(slots, SourceParam::Lfo2),
    ]
}

fn env_in_use(params: &DragonWaaveParams) -> [bool; 3] {
    envs_used(
        &slot_views(params),
        params.filter_type.value() != FilterTypeParam::Off,
        params.env_amount.value(),
    )
}

fn lfo_in_use(params: &DragonWaaveParams) -> [bool; 2] {
    let routes = [
        params.lfo1_pitch.value(),
        params.lfo1_pwm.value(),
        params.lfo1_cutoff.value(),
    ];
    lfos_used(&slot_views(params), routes)
}

/// Draw the editor; returns the area its contents used.
fn show(ctx: &egui::Context, params: &DragonWaaveParams, setter: &ParamSetter, state: &mut EditorState) -> egui::Rect {
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(MARGIN))
        .show(ctx, |ui| {
            draw(ui, params, setter, state);
            ui.min_rect()
        })
        .inner
}

/// A regular knob.
fn knob<P: Param>(ui: &mut Ui, param: &P, setter: &ParamSetter, label: &str, color: Color32) {
    ui.add(Knob::new(param, setter, label, color).diameter(KNOB));
}

/// A regular knob, greyed out and disabled unless `enabled`.
fn knob_if<P: Param>(ui: &mut Ui, enabled: bool, param: &P, setter: &ParamSetter, label: &str, color: Color32) {
    ui.add_enabled(enabled, Knob::new(param, setter, label, color).diameter(KNOB));
}

/// A regular knob drawn from the centre (bipolar parameters).
fn bipolar<P: Param>(ui: &mut Ui, param: &P, setter: &ParamSetter, label: &str, color: Color32) {
    bipolar_if(ui, true, param, setter, label, color);
}

fn bipolar_if<P: Param>(ui: &mut Ui, enabled: bool, param: &P, setter: &ParamSetter, label: &str, color: Color32) {
    ui.add_enabled(enabled, Knob::new(param, setter, label, color).diameter(KNOB).bipolar());
}

/// An ON/OFF pill with a label above it, the height of a knob.
fn pill(ui: &mut Ui, param: &BoolParam, setter: &ParamSetter, label: &str, color: Color32) {
    ui.vertical(|ui| {
        ui.set_width(56.0);
        ui.label(egui::RichText::new(label).size(13.0).color(theme::TEXT_DIM));
        ui.add_space(10.0);
        on_toggle(ui, param, setter, color);
    });
}

/// egui's item spacing (between knobs inside panels, and between rows).
const ITEM_SPACING: egui::Vec2 = egui::vec2(2.0, 4.0);
/// Gap between panels, across and down.
const GAP: f32 = 8.0;
/// Width of the page's content: the window less its margins.
const CONTENT_WIDTH: f32 = WIDTH as f32 - 2.0 * MARGIN;
/// The left column's width. Every row splits here, so panels stack into
/// columns: OSC 1 / MIXER / ENV / MOD MATRIX on the left.
const LEFT_COLUMN: f32 = 573.0;
const RIGHT_COLUMN: f32 = CONTENT_WIDTH - GAP - LEFT_COLUMN;
/// Row heights: each fits its tallest panel's contents.
const ROW_HEIGHTS: [f32; 4] = [129.0, 129.0, 213.0, 213.0];
/// VOICE's width; FX / OUT takes the rest of the right column.
const VOICE_WIDTH: f32 = 380.0;
/// Shape display sizes: the height of two knob rows.
const ENV_DISPLAY: egui::Vec2 = egui::vec2(140.0, 164.0);
const LFO_DISPLAY: egui::Vec2 = egui::vec2(170.0, 164.0);

/// The panel grid: each panel's outer size, by row and column. Every row
/// spans the full width and every panel in a row is the same height, so
/// the page reads as a clean grid.
fn cell(row: usize, col: usize) -> Option<egui::Vec2> {
    let width = match (row, col) {
        (_, 0) => LEFT_COLUMN,
        (3, 1) => VOICE_WIDTH,
        (3, 2) => RIGHT_COLUMN - GAP - VOICE_WIDTH,
        _ => RIGHT_COLUMN,
    };
    Some(egui::vec2(width, ROW_HEIGHTS[row]))
}

fn draw(ui: &mut Ui, params: &DragonWaaveParams, setter: &ParamSetter, state: &mut EditorState) {
    let mut rects = Vec::new();
    ui.spacing_mut().item_spacing = ITEM_SPACING;

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("DRAGON WAAVE")
                .size(20.0)
                .strong()
                .color(theme::PEACH),
        );
        ui.add_space(24.0);
        ui.label(egui::RichText::new("HQ").color(theme::TEXT_DIM));
        on_toggle(ui, &params.hq, setter, theme::PEACH);
        ui.add_space(12.0);
        ui.label(egui::RichText::new("CLIP").color(theme::TEXT_DIM));
        on_toggle(ui, &params.output_clip, setter, theme::PEACH);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new("drag to change · shift for fine · double-click to reset").color(theme::TEXT_DIM),
            );
        });
    });

    ui.horizontal_top(|ui| {
        rects.push(panel_sized(ui, "OSC 1", theme::MINT, cell(0, 0), |_| {}, |ui| osc_controls(ui, &params.osc1, setter)).rect);
        ui.add_space(GAP - ITEM_SPACING.x);
        rects.push(panel_sized(ui, "OSC 2", theme::MINT, cell(0, 1), |_| {}, |ui| {
            osc_controls(ui, &params.osc2, setter);
            pill(ui, &params.osc2_sync, setter, "Sync", theme::MINT);
            knob(ui, &params.cross_mod, setter, "X-Mod", theme::MINT);
        }).rect);
    });

    ui.add_space(GAP - ITEM_SPACING.y);
    ui.horizontal_top(|ui| {
        rects.push(panel_sized(ui, "MIXER", theme::MINT, cell(1, 0), |_| {}, |ui| {
            knob(ui, &params.sub_wave, setter, "Sub Wave", theme::PEACH);
            knob(ui, &params.sub_octave, setter, "Sub Oct", theme::PEACH_LIGHT);
            knob(ui, &params.sub_level, setter, "Sub", theme::MINT);
            knob(ui, &params.noise_color, setter, "Noise Col", theme::PEACH);
            knob(ui, &params.noise_level, setter, "Noise", theme::MINT);
            knob(ui, &params.ring_level, setter, "Ring", theme::MINT);
        }).rect);
        ui.add_space(GAP - ITEM_SPACING.x);
        rects.push(panel_sized(ui, "FILTER", theme::CYAN, cell(1, 1), |_| {}, |ui| {
            knob(ui, &params.filter_type, setter, "Type", theme::PEACH);
            // With the filter off, everything that only moves it greys out.
            let on = params.filter_type.value() != FilterTypeParam::Off;
            knob_if(ui, on, &params.cutoff, setter, "Cutoff", theme::CYAN);
            knob_if(ui, on, &params.resonance, setter, "Reso", theme::CYAN);
            knob(ui, &params.drive, setter, "Drive", theme::CYAN);
            bipolar_if(ui, on, &params.env_amount, setter, "Env Amt", theme::CYAN);
            knob_if(ui, on, &params.key_track, setter, "Key Trk", theme::CYAN_LIGHT);
            bipolar_if(ui, on, &params.velocity_track, setter, "Velocity", theme::CYAN_LIGHT);
            let ladder = params.filter_type.value() == FilterTypeParam::Ladder;
            knob_if(ui, ladder, &params.bass_comp, setter, "Bass Comp", theme::CYAN_DARK);
            knob(ui, &params.hpf_cutoff, setter, "HPF", theme::CYAN_DARK);
        }).rect);
    });

    ui.add_space(GAP - ITEM_SPACING.y);
    ui.horizontal_top(|ui| {
        // The chips (header) pick the selection; the contents show the one
        // selected when the frame began. Copies keep the two closures apart.
        let env = state.env.min(2);
        let mut next_env = env;
        let env_used = env_in_use(params);
        rects.push(panel_sized(
            ui,
            "ENV",
            ENV_COLORS[env],
            cell(2, 0),
            |ui| {
                for i in 0..3 {
                    let used = if env_used[i] {
                        "in use"
                    } else {
                        "not routed: use the Mod Matrix"
                    };
                    if chip(ui, ENV_NAMES[i], ENV_COLORS[i], env_used[i], env == i)
                        .on_hover_text(format!("{} · {used}", ENV_TITLES[i]))
                        .clicked()
                    {
                        next_env = i;
                    }
                }
            },
            |ui| {
                let p = [&params.amp_env, &params.filter_env, &params.mod_env][env];
                draw_envelope(ui, ENV_DISPLAY, &p.settings(Read::Target), ENV_COLORS[env], env_used[env]);
                ui.add_space(6.0);
                env_controls(ui, p, setter, ENV_COLORS[env]);
            },
        ).rect);
        state.env = next_env;
        let lfo = state.lfo.min(1);
        let mut next_lfo = lfo;
        let lfo_used = lfo_in_use(params);
        ui.add_space(GAP - ITEM_SPACING.x);
        rects.push(panel_sized(
            ui,
            "LFO",
            theme::PEACH,
            cell(2, 1),
            |ui| {
                for (i, &in_use) in lfo_used.iter().enumerate() {
                    let used = if in_use { "in use" } else { "not routed" };
                    if chip(ui, &(i + 1).to_string(), theme::PEACH, in_use, lfo == i)
                        .on_hover_text(format!("LFO {} · {used}", i + 1))
                        .clicked()
                    {
                        next_lfo = i;
                    }
                }
            },
            |ui| {
                let p = [&params.lfo1, &params.lfo2][lfo];
                draw_lfo(ui, LFO_DISPLAY, p.shape.value().into(), p.phase.value(), theme::PEACH, lfo_used[lfo]);
                ui.add_space(6.0);
                lfo_controls(ui, p, setter);
                if lfo == 0 {
                    ui.separator();
                    ui.vertical(|ui| {
                        bipolar(ui, &params.lfo1_pitch, setter, "> Pitch", theme::PEACH);
                        bipolar(ui, &params.lfo1_pwm, setter, "> PWM", theme::PEACH);
                    });
                    ui.vertical(|ui| {
                        bipolar(ui, &params.lfo1_cutoff, setter, "> Cutoff", theme::PEACH);
                    });
                }
            },
        ).rect);
        state.lfo = next_lfo;
    });

    ui.add_space(GAP - ITEM_SPACING.y);
    ui.horizontal_top(|ui| {
        rects.push(panel_sized(ui, "MOD MATRIX", theme::PEACH, cell(3, 0), |_| {}, |ui| {
            let slots = [
                &params.mod1,
                &params.mod2,
                &params.mod3,
                &params.mod4,
                &params.mod5,
                &params.mod6,
            ];
            ui.vertical(|ui| {
                for (i, slot) in slots.into_iter().enumerate() {
                    mod_slot(ui, i + 1, slot, setter);
                }
            });
        }).rect);
        ui.add_space(GAP - ITEM_SPACING.x);
        rects.push(panel_sized(ui, "VOICE", theme::OUTPUT, cell(3, 1), |_| {}, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    knob(ui, &params.voice_mode, setter, "Mode", theme::PEACH);
                    knob(ui, &params.unison, setter, "Unison", theme::OUTPUT);
                    let stacked = params.unison.value() > 1;
                    knob_if(ui, stacked, &params.detune, setter, "Detune", theme::OUTPUT);
                    knob_if(ui, stacked, &params.spread, setter, "Spread", theme::OUTPUT);
                    knob(ui, &params.polyphony, setter, "Voices", theme::OUTPUT);
                });
                ui.horizontal(|ui| {
                    knob(ui, &params.glide, setter, "Glide", theme::OUTPUT);
                    knob_if(
                        ui,
                        params.glide.value() > 0.0,
                        &params.glide_mode,
                        setter,
                        "Glide Mode",
                        theme::PEACH_LIGHT,
                    );
                    let mono = params.voice_mode.value() != VoiceModeParam::Poly;
                    knob_if(ui, mono, &params.retrigger, setter, "Retrig", theme::PEACH_DARK);
                    knob(ui, &params.drift, setter, "Drift", theme::OUTPUT);
                    knob(ui, &params.bend_range, setter, "Bend", theme::OUTPUT);
                });
            });
        }).rect);
        ui.add_space(GAP - ITEM_SPACING.x);
        rects.push(panel_sized(ui, "FX / OUT", theme::OUTPUT, cell(3, 2), |_| {}, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    knob(ui, &params.chorus_mode, setter, "Chorus", theme::PEACH);
                    knob(ui, &params.chorus_rate, setter, "Rate", theme::OUTPUT);
                    knob(ui, &params.chorus_depth, setter, "Depth", theme::OUTPUT);
                });
                ui.horizontal(|ui| {
                    knob(ui, &params.chorus_mix, setter, "Mix", theme::OUTPUT);
                    knob(ui, &params.chorus_tone, setter, "Tone", theme::OUTPUT);
                    knob(ui, &params.volume, setter, "Volume", theme::OUTPUT);
                });
            });
        }).rect);
    });
    state.panel_rects = rects;
}

fn osc_controls(ui: &mut Ui, osc: &OscParams, setter: &ParamSetter) {
    knob(ui, &osc.wave, setter, "Wave", theme::PEACH);
    knob(ui, &osc.octave, setter, "Octave", theme::PEACH_LIGHT);
    bipolar(ui, &osc.semitone, setter, "Semi", theme::MINT);
    bipolar(ui, &osc.fine, setter, "Fine", theme::MINT);
    knob_if(
        ui,
        osc.wave.value() == WaveParam::Pulse,
        &osc.pulse_width,
        setter,
        "PW",
        theme::MINT_DARK,
    );
    knob(ui, &osc.level, setter, "Level", theme::MINT);
    knob(ui, &osc.phase, setter, "Phase", theme::PEACH_DARK);
}

fn env_controls(ui: &mut Ui, env: &EnvParams, setter: &ParamSetter, color: Color32) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            knob(ui, &env.delay, setter, "Delay", color);
            knob(ui, &env.attack, setter, "Attack", color);
            knob(ui, &env.hold, setter, "Hold", color);
            knob(ui, &env.decay, setter, "Decay", color);
            knob(ui, &env.sustain, setter, "Sustain", color);
            knob(ui, &env.release, setter, "Release", color);
        });
        ui.horizontal(|ui| {
            knob(ui, &env.attack_curve, setter, "Atk Curve", color);
            knob(ui, &env.decay_curve, setter, "Dec Curve", color);
            knob(ui, &env.velocity_level, setter, "Velocity", color);
            knob(ui, &env.velocity_attack, setter, "Vel>Atk", color);
        });
    });
}

fn lfo_controls(ui: &mut Ui, lfo: &LfoParams, setter: &ParamSetter) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            knob(ui, &lfo.shape, setter, "Shape", theme::PEACH);
            let synced = lfo.sync.value();
            knob_if(ui, !synced, &lfo.rate, setter, "Rate", theme::PEACH);
            knob_if(ui, synced, &lfo.division, setter, "Division", theme::PEACH_LIGHT);
            pill(ui, &lfo.sync, setter, "Sync", theme::PEACH);
        });
        ui.horizontal(|ui| {
            // Free poly LFOs start each note at a random phase.
            knob_if(
                ui,
                lfo.mode.value() != LfoModeParam::Poly,
                &lfo.phase,
                setter,
                "Phase",
                theme::PEACH_LIGHT,
            );
            knob(ui, &lfo.delay, setter, "Delay", theme::PEACH);
            knob(ui, &lfo.fade, setter, "Fade In", theme::PEACH);
            knob(ui, &lfo.mode, setter, "Mode", theme::PEACH_DARK);
        });
    });
}

/// One matrix row: number, Source -> Destination, Amount, via Via.
fn mod_slot(ui: &mut Ui, n: usize, slot: &ModSlotParams, setter: &ParamSetter) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(egui::RichText::new(n.to_string()).strong().color(theme::PEACH));
        enum_combo(ui, ("src", n), &slot.source, setter, 92.0);
        ui.label(egui::RichText::new("→").color(theme::TEXT_DIM));
        enum_combo(ui, ("dest", n), &slot.dest, setter, 104.0);
        let routed = slot.source.value() != SourceParam::None && slot.dest.value() != DestParam::None;
        ui.add_enabled(routed, ParamSlider::for_param(&slot.amount, setter).with_width(96.0));
        ui.add_enabled_ui(routed, |ui| {
            ui.label(egui::RichText::new("via").color(theme::TEXT_DIM));
            enum_combo(ui, ("via", n), &slot.via, setter, 92.0);
        });
    });
}

/// A dropdown for an enum parameter, setting it as one automation gesture.
fn enum_combo<T: Enum + PartialEq + 'static>(
    ui: &mut Ui,
    id: impl std::hash::Hash,
    param: &EnumParam<T>,
    setter: &ParamSetter,
    width: f32,
) {
    let current = param.value().to_index();
    let names = T::variants();
    egui::ComboBox::from_id_salt(id)
        .width(width)
        .selected_text(names[current])
        .show_ui(ui, |ui| {
            for (i, name) in names.iter().enumerate() {
                if ui.selectable_label(i == current, *name).clicked() && i != current {
                    setter.begin_set_parameter(param);
                    setter.set_parameter(param, T::from_index(i));
                    setter.end_set_parameter(param);
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A GUI context that ignores everything: lets tests draw the editor
    /// headlessly.
    struct NullContext;

    impl GuiContext for NullContext {
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
            unimplemented!("not used by the editor")
        }
        fn set_state(&self, _: PluginState) {}
    }

    /// Draw one frame at the window size; returns the area the contents used.
    pub(crate) fn frame(state: &mut EditorState) -> egui::Rect {
        let params = DragonWaaveParams::default();
        let context = NullContext;
        let setter = ParamSetter::new(&context);
        let ctx = egui::Context::default();
        setup(&ctx);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(WIDTH as f32, HEIGHT as f32),
            )),
            ..Default::default()
        };
        // Two passes: egui sizes some layouts on the first.
        let mut used = egui::Rect::NOTHING;
        for _ in 0..2 {
            let _ = ctx.run(input.clone(), |ctx| used = show(ctx, &params, &setter, state));
        }
        used
    }

    fn slot(source: SourceParam, dest: DestParam, amount: f32, via: SourceParam) -> SlotView {
        SlotView {
            source,
            dest,
            amount,
            via,
        }
    }

    #[test]
    fn chips_light_up_when_their_source_is_used() {
        use SourceParam as S;
        let none: [SlotView; 0] = [];
        // Amp is always in use; the others only when routed.
        assert_eq!(envs_used(&none, true, 0.0), [true, false, false]);
        assert_eq!(envs_used(&none, true, 0.3), [true, true, false]);
        // A filter amount with the filter off does nothing.
        assert_eq!(envs_used(&none, false, 0.3), [true, false, false]);
        let mod_env = [slot(S::ModEnv, DestParam::Pitch, 0.1, S::None)];
        assert_eq!(envs_used(&mod_env, true, 0.0), [true, false, true]);
        // As a via, too.
        let via = [slot(S::Velocity, DestParam::Cutoff, 0.1, S::FilterEnv)];
        assert_eq!(envs_used(&via, true, 0.0), [true, true, false]);

        assert_eq!(lfos_used(&none, [0.0; 3]), [false, false]);
        assert_eq!(lfos_used(&none, [0.0, 0.2, 0.0]), [true, false]);
        assert_eq!(
            lfos_used(&[slot(S::Lfo2, DestParam::Pan, -0.5, S::None)], [0.0; 3]),
            [false, true]
        );
    }

    #[test]
    fn inactive_slots_light_nothing() {
        use SourceParam as S;
        for s in [
            slot(S::Lfo2, DestParam::None, 1.0, S::None),
            slot(S::Lfo2, DestParam::Cutoff, 0.0, S::None),
            slot(S::None, DestParam::Cutoff, 1.0, S::Lfo2),
        ] {
            assert!(!matrix_uses(&[s], S::Lfo2), "{s:?}");
        }
    }

    #[test]
    fn default_patch_lights_amp_and_lfo_1() {
        // The default slots route LFO 1 (via the wheel) and Aftertouch.
        let params = DragonWaaveParams::default();
        assert_eq!(env_in_use(&params), [true, false, false]);
        assert_eq!(lfo_in_use(&params), [true, false]);
    }

    /// Panels form a grid: every row starts and ends at the content edges,
    /// panels in a row share a height, and every row splits at the same
    /// column boundary. Checked for every ENV / LFO selection.
    #[test]
    fn panels_form_a_clean_grid() {
        let close = |a: f32, b: f32| (a - b).abs() < 0.5;
        for env in 0..3 {
            for lfo in 0..2 {
                let mut state = EditorState { env, lfo, ..Default::default() };
                frame(&mut state);
                let mut rows: Vec<Vec<egui::Rect>> = Vec::new();
                for r in state.panel_rects {
                    match rows.last_mut() {
                        Some(row) if close(row[0].min.y, r.min.y) => row.push(r),
                        _ => rows.push(vec![r]),
                    }
                }
                assert_eq!(rows.len(), 4, "env {env} lfo {lfo}: rows");
                for (i, row) in rows.iter().enumerate() {
                    let what = format!("env {env} lfo {lfo} row {i}");
                    assert!(close(row[0].min.x, MARGIN), "{what}: starts at {}", row[0].min.x);
                    let right = row.last().unwrap().max.x;
                    assert!(close(right, WIDTH as f32 - MARGIN), "{what}: ends at {right}");
                    assert!(row.iter().all(|r| close(r.height(), row[0].height())), "{what}: uneven heights {row:?}");
                    assert!(close(row[0].max.x, MARGIN + LEFT_COLUMN), "{what}: column split at {}", row[0].max.x);
                }
            }
        }
    }

    #[test]
    fn every_panel_fits_the_window() {
        for env in 0..3 {
            for lfo in 0..2 {
                let used = frame(&mut EditorState { env, lfo, ..Default::default() });
                // Content must stop a margin short of the window's edges.
                let (w, h) = (WIDTH as f32 - MARGIN, HEIGHT as f32 - MARGIN);
                assert!(
                    used.max.x <= w && used.max.y <= h,
                    "env {env} lfo {lfo}: content reaches {:.0} x {:.0}, limit {w} x {h}",
                    used.max.x,
                    used.max.y
                );
            }
        }
    }
}
