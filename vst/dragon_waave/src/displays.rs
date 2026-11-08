//! Shape displays for the editor: the selected envelope and LFO, drawn with
//! the engine's own curve functions so they can't disagree with the sound.
//!
//! The point calculations are pure functions (unit-tested); the `draw_*`
//! functions paint them.

use dragon_ui::theme;
use dsp_core::synth::env::{attack_shape, fall_shape, MAX_DECAY_SHARPNESS};
use dsp_core::synth::lfo::Lfo;
use dsp_core::synth::{EnvSettings, LfoShape};
use nih_plug_egui::egui::{self, pos2, vec2, Color32, Pos2, Rect, Shape, Stroke, Ui};

/// Share of the width given to the sustain plateau.
const SUSTAIN_WIDTH: f32 = 0.2;
/// Points per timed stage, and per LFO cycle.
const STAGE_POINTS: usize = 24;
const LFO_POINTS: usize = 120;
/// LFO cycles shown (one is too few for the random shapes to show steps).
pub const LFO_CYCLES: usize = 2;

/// An envelope's outline in unit coordinates (x and level both 0..1),
/// plus the x of each stage boundary.
///
/// Stage widths follow the square root of their times, so a 5 ms attack
/// stays visible next to a 10 s release; sustain gets a fixed share.
pub fn envelope_points(env: &EnvSettings) -> (Vec<(f32, f32)>, Vec<f32>) {
    let sharp = env.decay_curve * MAX_DECAY_SHARPNESS;
    let s = env.sustain;
    // (time, level at progress p), in order: delay, attack, hold, decay, release.
    type Stage<'a> = (f32, Box<dyn Fn(f32) -> f32 + 'a>);
    let timed: [Stage; 5] = [
        (env.delay, Box::new(|_| 0.0)),
        (env.attack, Box::new(|p| attack_shape(p, env.attack_curve))),
        (env.hold, Box::new(|_| 1.0)),
        (env.decay, Box::new(move |p| s + (1.0 - s) * fall_shape(p, sharp))),
        (env.release, Box::new(move |p| s * fall_shape(p, sharp))),
    ];
    let total: f32 = timed.iter().map(|(t, _)| t.max(0.0).sqrt()).sum();
    let scale = if total > 0.0 { (1.0 - SUSTAIN_WIDTH) / total } else { 0.0 };

    let mut points = vec![(0.0, 0.0)];
    let mut bounds = Vec::new();
    let mut x = 0.0;
    for (i, (time, level)) in timed.iter().enumerate() {
        if i == 4 {
            // The sustain plateau sits between decay and release.
            points.push((x + SUSTAIN_WIDTH, s));
            x += SUSTAIN_WIDTH;
            bounds.push(x);
        }
        let width = time.max(0.0).sqrt() * scale;
        if width > 0.0 {
            for k in 1..=STAGE_POINTS {
                let p = k as f32 / STAGE_POINTS as f32;
                points.push((x + width * p, level(p)));
            }
            x += width;
        }
        if i < 4 {
            bounds.push(x);
        }
    }
    (points, bounds)
}

/// `LFO_CYCLES` cycles of an LFO from its start phase, -1..1. Random shapes
/// use a fixed seed, so the picture doesn't change from frame to frame.
pub fn lfo_points(shape: LfoShape, phase: f32) -> Vec<f32> {
    let mut lfo = Lfo::new(0x5EED);
    lfo.restart(phase);
    let n = LFO_POINTS * LFO_CYCLES;
    (0..=n).map(|_| lfo.tick(shape, 1.0 / LFO_POINTS as f32)).collect()
}

/// The display's frame; returns the area to draw in.
fn frame(ui: &mut Ui, size: egui::Vec2) -> Rect {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect(rect, 6.0, theme::BACKGROUND, Stroke::new(1.0_f32, Color32::from_gray(60)), egui::StrokeKind::Inside);
    rect.shrink(6.0)
}

fn faint() -> Stroke {
    Stroke::new(1.0_f32, Color32::from_gray(48))
}

/// Draw an envelope (dimmed when it isn't in use).
pub fn draw_envelope(ui: &mut Ui, size: egui::Vec2, env: &EnvSettings, color: Color32, in_use: bool) {
    let area = frame(ui, size);
    let (points, bounds) = envelope_points(env);
    let at = |(x, y): (f32, f32)| pos2(area.left() + x * area.width(), area.bottom() - y * area.height());
    let painter = ui.painter_at(area.expand(1.0));
    for x in bounds {
        let px = area.left() + x * area.width();
        painter.line_segment([pos2(px, area.top()), pos2(px, area.bottom())], faint());
    }
    let color = if in_use { color } else { color.linear_multiply(0.35) };
    let line: Vec<Pos2> = points.into_iter().map(at).collect();
    painter.add(Shape::line(line, Stroke::new(2.0_f32, color)));
}

/// Draw an LFO's shape (dimmed when it isn't in use).
pub fn draw_lfo(ui: &mut Ui, size: egui::Vec2, shape: LfoShape, phase: f32, color: Color32, in_use: bool) {
    let area = frame(ui, size);
    let values = lfo_points(shape, phase);
    let painter = ui.painter_at(area.expand(1.0));
    painter.line_segment([area.left_center(), area.right_center()], faint());
    let cycle = area.width() / LFO_CYCLES as f32;
    for c in 1..LFO_CYCLES {
        let x = area.left() + c as f32 * cycle;
        painter.line_segment([pos2(x, area.top()), pos2(x, area.bottom())], faint());
    }
    let n = (values.len() - 1) as f32;
    let line: Vec<Pos2> = values
        .iter()
        .enumerate()
        .map(|(i, v)| area.left_center() + vec2(i as f32 / n * area.width(), -v * area.height() / 2.0))
        .collect();
    let color = if in_use { color } else { color.linear_multiply(0.35) };
    painter.add(Shape::line(line, Stroke::new(2.0_f32, color)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsp_core::synth::Patch;

    fn env(delay: f32, attack: f32, hold: f32, decay: f32, sustain: f32, release: f32) -> EnvSettings {
        EnvSettings { delay, attack, hold, decay, sustain, release, ..Patch::default().amp_env }
    }

    #[test]
    fn envelope_outline_runs_left_to_right_from_zero_to_zero() {
        for e in [env(0.0, 0.005, 0.0, 0.3, 1.0, 0.08), env(0.5, 2.0, 0.3, 1.0, 0.4, 10.0), env(0.0, 0.0005, 0.0, 0.001, 0.0, 0.001)]
        {
            let (points, bounds) = envelope_points(&e);
            assert_eq!(points[0], (0.0, 0.0));
            let last = *points.last().unwrap();
            assert!((last.0 - 1.0).abs() < 1e-4 && last.1.abs() < 1e-4, "{last:?}");
            assert!(points.windows(2).all(|w| w[1].0 >= w[0].0), "x never goes back");
            assert!(points.iter().all(|&(_, y)| (0.0..=1.0 + 1e-4).contains(&y)));
            // Peak reaches full level; the plateau sits at the sustain level.
            assert!(points.iter().any(|&(_, y)| (y - 1.0).abs() < 1e-4));
            assert!(points.iter().any(|&(_, y)| (y - e.sustain).abs() < 1e-4));
            assert_eq!(bounds.len(), 5);
        }
    }

    #[test]
    fn stage_widths_follow_the_square_root_of_time() {
        // Attack 0.01 s vs release 0.04 s: the release is twice as wide.
        let (_, b) = envelope_points(&env(0.0, 0.01, 0.0, 0.0, 0.5, 0.04));
        let (attack, release) = (b[1] - b[0], 1.0 - b[4]);
        assert!((release / attack - 2.0).abs() < 1e-3, "{attack} vs {release}");
        // Sustain always gets its fixed share.
        assert!((b[4] - b[3] - SUSTAIN_WIDTH).abs() < 1e-6);
    }

    #[test]
    fn lfo_outline_matches_the_engine() {
        // A sine from phase 0.25 starts at its peak; every shape stays in range.
        let v = lfo_points(LfoShape::Sine, 0.25);
        assert!((v[0] - 1.0).abs() < 1e-5);
        assert_eq!(v.len(), LFO_POINTS * LFO_CYCLES + 1);
        for shape in [LfoShape::Square, LfoShape::SampleHold, LfoShape::SmoothRandom, LfoShape::SawDown] {
            let v = lfo_points(shape, 0.0);
            assert!(v.iter().all(|x| (-1.0..=1.0).contains(x)), "{shape:?}");
            // Fixed seed: the same picture every frame.
            assert_eq!(v, lfo_points(shape, 0.0));
        }
    }
}
