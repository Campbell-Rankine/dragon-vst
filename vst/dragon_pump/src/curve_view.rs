//! The CURVE panel: the ducking curve, the depth floor, the two handles,
//! and the playhead.
//!
//! - **Tempo mode** shows one beat cycle; Hold and Release are fractions
//!   of it.
//! - **MIDI mode** shows the curve from a note-on, over a fixed time span
//!   (0.25 to 3 s, the shortest that fits); Hold and Release are in ms.
//!
//! Handles:
//! - **Hold** (on the floor, where the hold ends): drag left/right.
//! - **Release** (halfway through the release, on the curve): drag
//!   left/right for Release, up/down for Tension (down = slower start, the
//!   classic pump).
//! - **Floor line:** drag up/down for Depth.
//!
//! Shift drags finely; double-clicking a handle or the floor resets what it
//! controls. Each drag is one automation gesture, as with the knobs. The
//! time span is frozen while dragging so the handle stays under the
//! pointer.
//!
//! The maths (`View`, `curve_points`, `handles`, `hit_test`, `DragValues`)
//! is plain functions so it's unit-tested without egui. The y axis is
//! linear gain (top = unity, bottom = silence), so the dip looks as deep as
//! it sounds.

use crate::params::{
    DragonPumpParams, Mode, HOLD_MAX, HOLD_MS_MAX, RELEASE_MAX, RELEASE_MIN, RELEASE_MS_MAX, RELEASE_MS_MIN,
};
use dragon_ui::theme;
use dsp_core::duck::{tempo_duck, DuckShape};
use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, pos2, vec2, Color32, Mesh, Pos2, Rect, Shape, Stroke, Ui};

/// Evenly spaced samples of the curve over the span (plus its corners).
const CURVE_SAMPLES: usize = 240;
/// Space above unity and below silence, so the line isn't on the border.
const PAD_Y: f32 = 10.0;
/// Room on the left for the dB labels.
const PAD_LEFT: f32 = 30.0;
/// How near (px) the pointer must be to grab a handle, or the floor line.
const HANDLE_REACH: f32 = 9.0;
const FLOOR_REACH: f32 = 5.0;
/// Shift-drag moves this fraction as far.
const FINE_FACTOR: f32 = 0.1;
/// Tension change per plot height of vertical drag.
const TENSION_PER_HEIGHT: f32 = 2.0;
/// Below this Mix the floor barely moves with Depth, so it can't be dragged.
const MIN_MIX_FOR_DEPTH_DRAG: f32 = 0.05;
/// MIDI mode's time spans (s): the shortest that shows the whole curve
/// with some room after it is used.
const FREE_SPANS: [f32; 6] = [0.25, 0.5, 1.0, 1.5, 2.0, 3.0];
/// Room after the curve's end, as a fraction of its length.
const FREE_SPAN_ROOM: f32 = 1.15;

/// How the x axis maps to time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimeBase {
    /// Tempo mode: one cycle of this many seconds, each dip carrying on
    /// from the last.
    Cycle(f32),
    /// MIDI mode: from a single trigger.
    Free,
}

/// The x axis: what it shows and how many seconds it spans.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub base: TimeBase,
    pub span: f32,
}

impl View {
    pub fn tempo(cycle_secs: f32) -> Self {
        Self { base: TimeBase::Cycle(cycle_secs), span: cycle_secs }
    }

    /// A view from a trigger, long enough for a curve of `length` seconds.
    pub fn free(length: f32) -> Self {
        let wanted = length * FREE_SPAN_ROOM;
        let span = FREE_SPANS.iter().copied().find(|&s| s >= wanted).unwrap_or(FREE_SPANS[FREE_SPANS.len() - 1]);
        Self { base: TimeBase::Free, span }
    }

    /// The view for the current mode.
    pub fn for_mode(mode: Mode, cycle_secs: f32, shape: &DuckShape) -> Self {
        if mode.is_cyclic() {
            Self::tempo(cycle_secs)
        } else {
            Self::free(shape.length())
        }
    }

    /// The duck amount `t` seconds in.
    fn duck(&self, shape: &DuckShape, t: f32) -> f32 {
        match self.base {
            TimeBase::Cycle(cycle) => tempo_duck(shape, t, cycle),
            TimeBase::Free => shape.duck_at(t, 0.0),
        }
    }

    /// The curve's gain at `x` (0..1) across the view.
    pub fn gain_at(&self, shape: &DuckShape, x: f32) -> f32 {
        shape.gain(self.duck(shape, x.clamp(0.0, 1.0) * self.span))
    }
}

/// The curve the plugin is applying, from the parameters' current values.
/// Depth includes Mix, so the floor is where the gain really goes.
pub fn display_shape(params: &DragonPumpParams, mode: Mode, cycle_secs: f32) -> DuckShape {
    let (hold, release) = if mode.is_cyclic() {
        (params.hold_pct.value() * cycle_secs, params.release_pct.value() * cycle_secs)
    } else {
        (params.hold_ms.value() / 1000.0, params.release_ms.value() / 1000.0)
    };
    DuckShape {
        depth: params.depth.value() * params.mix.value(),
        attack: params.attack.value() / 1000.0,
        hold,
        release,
        tension: params.tension.value(),
    }
    .sanitized()
}

/// The curve across the view as `(x, gain)`, x = 0..=1, left to right.
/// The attack, hold and release corners are included exactly, since a
/// 1 ms attack is far narrower than one even step.
pub fn curve_points(shape: &DuckShape, view: &View) -> Vec<(f32, f32)> {
    let span = view.span;
    let corners = [shape.attack, shape.attack + shape.hold, shape.length()];
    let mut times: Vec<f32> = (0..=CURVE_SAMPLES).map(|i| span * i as f32 / CURVE_SAMPLES as f32).collect();
    times.extend(corners.iter().copied().filter(|&t| t < span));
    times.sort_by(f32::total_cmp);
    times.dedup();
    times.into_iter().map(|t| (t / span, shape.gain(view.duck(shape, t)))).collect()
}

/// What a drag on the plot changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Hold,
    Release,
    Depth,
}

/// Where the handles are, as (x 0..1 across the view, gain).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Handles {
    /// Where the hold ends, on the floor.
    pub hold: (f32, f32),
    /// Halfway through the release, on the curve.
    pub release: (f32, f32),
    /// The floor's gain (1 - depth).
    pub floor: f32,
}

/// The handles for a shape. A handle that would fall past the view's end
/// is drawn at the end, so it can always be grabbed.
pub fn handles(shape: &DuckShape, view: &View) -> Handles {
    let x = |t: f32| (t / view.span).min(1.0);
    let hold_x = x(shape.attack + shape.hold);
    let release_t = shape.attack + shape.hold + 0.5 * shape.release;
    let release_x = x(release_t);
    let release_gain = if release_t < view.span { view.gain_at(shape, release_x) } else { 1.0 - shape.depth };
    Handles { hold: (hold_x, 1.0 - shape.depth), release: (release_x, release_gain), floor: 1.0 - shape.depth }
}

/// Maps (x 0..1, gain 0..1) into the plot area.
#[derive(Debug, Clone, Copy)]
pub struct Plot {
    pub area: Rect,
}

impl Plot {
    pub fn for_rect(rect: Rect) -> Self {
        Self { area: Rect::from_min_max(pos2(rect.left() + PAD_LEFT, rect.top() + PAD_Y), pos2(rect.right() - 8.0, rect.bottom() - PAD_Y)) }
    }

    pub fn pos(&self, x: f32, gain: f32) -> Pos2 {
        pos2(self.area.left() + x * self.area.width(), self.area.bottom() - gain.clamp(0.0, 1.0) * self.area.height())
    }
}

/// What's under the pointer: the release handle first (it can sit on the
/// floor at full tension), then the hold handle, then the floor line.
/// The floor can't be grabbed when Depth can't move it (`depth_draggable`).
pub fn hit_test(plot: &Plot, h: &Handles, pointer: Pos2, depth_draggable: bool) -> Option<Target> {
    let near = |(x, g): (f32, f32)| plot.pos(x, g).distance(pointer) <= HANDLE_REACH;
    if near(h.release) {
        Some(Target::Release)
    } else if near(h.hold) {
        Some(Target::Hold)
    } else if depth_draggable
        && (plot.pos(0.0, h.floor).y - pointer.y).abs() <= FLOOR_REACH
        && (plot.area.left()..=plot.area.right()).contains(&pointer.x)
    {
        Some(Target::Depth)
    } else {
        None
    }
}

/// Hold and Release limits in seconds for a mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    pub hold_max: f32,
    pub release_min: f32,
    pub release_max: f32,
}

impl Limits {
    pub fn for_mode(mode: Mode, cycle_secs: f32) -> Self {
        if mode.is_cyclic() {
            Self { hold_max: HOLD_MAX * cycle_secs, release_min: RELEASE_MIN * cycle_secs, release_max: RELEASE_MAX * cycle_secs }
        } else {
            Self { hold_max: HOLD_MS_MAX / 1000.0, release_min: RELEASE_MS_MIN / 1000.0, release_max: RELEASE_MS_MAX / 1000.0 }
        }
    }
}

/// The values a drag edits (Hold and Release in seconds), accumulated
/// across frames (as the knobs do) so small moves aren't lost to rounding,
/// and clamped as they go so turning back responds at once.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragValues {
    pub hold: f32,
    pub release: f32,
    pub tension: f32,
    pub depth: f32,
}

impl DragValues {
    /// From a display shape (seconds) and the unmixed Depth.
    pub fn new(shape: &DuckShape, depth: f32) -> Self {
        Self { hold: shape.hold, release: shape.release, tension: shape.tension, depth }
    }

    /// Apply a move of `dx` seconds (+ = right) and `dy` (fractions of the
    /// plot height, + = down).
    pub fn drag(&mut self, target: Target, dx: f32, dy: f32, mix: f32, limits: &Limits) {
        match target {
            Target::Hold => self.hold = (self.hold + dx).clamp(0.0, limits.hold_max),
            Target::Release => {
                // The handle is at the release's midpoint, so the release
                // grows twice as fast as it moves: it stays under the pointer.
                self.release = (self.release + 2.0 * dx).clamp(limits.release_min, limits.release_max);
                self.tension = (self.tension + TENSION_PER_HEIGHT * dy).clamp(-1.0, 1.0);
            }
            // The floor is at 1 - depth * mix: follow the pointer.
            Target::Depth => self.depth = (self.depth + dy / mix.max(MIN_MIX_FOR_DEPTH_DRAG)).clamp(0.0, 1.0),
        }
    }
}

/// A drag in progress: what it holds, its values, and the span frozen at
/// its start.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ActiveDrag {
    target: Target,
    values: DragValues,
    span: f32,
}

/// Hold and Release's parameters in a mode.
fn hold_release(p: &DragonPumpParams, mode: Mode) -> (&FloatParam, &FloatParam) {
    if mode.is_cyclic() {
        (&p.hold_pct, &p.release_pct)
    } else {
        (&p.hold_ms, &p.release_ms)
    }
}

/// The parameters a target edits.
fn target_params(p: &DragonPumpParams, mode: Mode, target: Target) -> impl Iterator<Item = &FloatParam> {
    let (hold, release) = hold_release(p, mode);
    match target {
        Target::Hold => [Some(hold), None],
        Target::Release => [Some(release), Some(&p.tension)],
        Target::Depth => [Some(&p.depth), None],
    }
    .into_iter()
    .flatten()
}

/// The plot with its handles: draws it and handles dragging. `pos` is the
/// playhead in seconds (`None` hides it); `status` is shown at the bottom
/// right.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut Ui,
    size: egui::Vec2,
    params: &DragonPumpParams,
    setter: &ParamSetter,
    mode: Mode,
    cycle_secs: f32,
    pos: Option<f32>,
    status: &str,
) {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    let plot = Plot::for_rect(rect);
    let mix = params.mix.value();
    let depth_draggable = mix >= MIN_MIX_FOR_DEPTH_DRAG;
    let shape = display_shape(params, mode, cycle_secs);

    // Which handle a drag holds is decided where the press started, and
    // kept (with the span) for the whole drag.
    let drag_id = response.id.with("curve-drag");
    let mut active: Option<ActiveDrag> = ui.memory(|m| m.data.get_temp(drag_id));
    let mut view = View::for_mode(mode, cycle_secs, &shape);
    if let Some(a) = active {
        view.span = a.span;
    }
    let h = handles(&shape, &view);
    if response.drag_started() {
        let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or(rect.center());
        active = hit_test(&plot, &h, origin, depth_draggable).map(|target| ActiveDrag {
            target,
            values: DragValues::new(&shape, params.depth.value()),
            span: view.span,
        });
        if let Some(a) = active {
            for param in target_params(params, mode, a.target) {
                setter.begin_set_parameter(param);
            }
        }
    }
    if let Some(a) = active.as_mut() {
        if response.dragged() {
            let fine = if ui.input(|i| i.modifiers.shift) { FINE_FACTOR } else { 1.0 };
            let d = response.drag_delta() * fine;
            let limits = Limits::for_mode(mode, cycle_secs);
            a.values.drag(a.target, d.x / plot.area.width() * a.span, d.y / plot.area.height(), mix, &limits);
            set_values(params, setter, mode, cycle_secs, a.target, &a.values);
        }
        if response.drag_stopped() {
            for param in target_params(params, mode, a.target) {
                setter.end_set_parameter(param);
            }
            active = None;
        }
    }
    ui.memory_mut(|m| match active {
        Some(a) => m.data.insert_temp(drag_id, a),
        None => m.data.remove::<ActiveDrag>(drag_id),
    });

    // Double-click resets what's under the pointer.
    let hovered = active.map(|a| a.target).or_else(|| response.hover_pos().and_then(|p| hit_test(&plot, &h, p, depth_draggable)));
    if response.double_clicked() {
        if let Some(target) = hovered {
            for param in target_params(params, mode, target) {
                setter.begin_set_parameter(param);
                setter.set_parameter(param, param.default_plain_value());
                setter.end_set_parameter(param);
            }
        }
    }
    if let Some(target) = hovered {
        ui.ctx().set_cursor_icon(match target {
            Target::Hold => egui::CursorIcon::ResizeHorizontal,
            Target::Release => egui::CursorIcon::Move,
            Target::Depth => egui::CursorIcon::ResizeVertical,
        });
        let tip = match target {
            Target::Hold => "Hold: drag left/right · double-click to reset",
            Target::Release => "Release: drag left/right · Tension: drag up/down · double-click to reset",
            Target::Depth => "Depth: drag up/down · double-click to reset",
        };
        response.on_hover_text_at_pointer(tip);
    }

    // Redraw from the values just set, so the curve follows the pointer
    // this frame.
    let shape = display_shape(params, mode, cycle_secs);
    let x = pos.map(|p| p / view.span).filter(|x| *x <= 1.0);
    draw(ui, rect, &plot, &shape, &view, x, status, hovered);
}

/// Send a drag's values to the host (inside the gesture begun at the
/// drag's start), in the mode's units.
fn set_values(params: &DragonPumpParams, setter: &ParamSetter, mode: Mode, cycle_secs: f32, target: Target, v: &DragValues) {
    let (hold, release) = hold_release(params, mode);
    let to_param = |secs: f32| if mode.is_cyclic() { secs / cycle_secs } else { secs * 1000.0 };
    match target {
        Target::Hold => setter.set_parameter(hold, to_param(v.hold)),
        Target::Release => {
            setter.set_parameter(release, to_param(v.release));
            setter.set_parameter(&params.tension, v.tension);
        }
        Target::Depth => setter.set_parameter(&params.depth, v.depth),
    }
}

#[allow(clippy::too_many_arguments)]
fn draw(
    ui: &Ui,
    rect: Rect,
    plot: &Plot,
    shape: &DuckShape,
    view: &View,
    playhead: Option<f32>,
    status: &str,
    hovered: Option<Target>,
) {
    let painter = ui.painter_at(rect);
    let area = plot.area;
    painter.rect(rect, 6.0, theme::BACKGROUND, Stroke::new(1.0_f32, Color32::from_gray(60)), egui::StrokeKind::Inside);
    let faint = Stroke::new(1.0_f32, Color32::from_gray(45));
    let small = egui::FontId::proportional(10.0);

    // Grid: gain lines with dB labels, and the span in quarters.
    for (gain, text) in [(1.0, "0 dB"), (0.5, "-6"), (0.25, "-12"), (0.0, "-inf")] {
        let y = plot.pos(0.0, gain).y;
        painter.line_segment([pos2(area.left(), y), pos2(area.right(), y)], faint);
        painter.text(pos2(rect.left() + 4.0, y), egui::Align2::LEFT_CENTER, text, small.clone(), theme::TEXT_DIM);
    }
    for q in 1..4 {
        let x = plot.pos(q as f32 / 4.0, 0.0).x;
        painter.line_segment([pos2(x, area.top()), pos2(x, area.bottom())], faint);
    }

    // The reduction: red between unity and the curve.
    let points: Vec<Pos2> = curve_points(shape, view).into_iter().map(|(x, g)| plot.pos(x, g)).collect();
    let mut mesh = Mesh::default();
    let fill = theme::RED.linear_multiply(0.18);
    for (i, p) in points.iter().enumerate() {
        mesh.colored_vertex(pos2(p.x, area.top()), fill);
        mesh.colored_vertex(*p, fill);
        if i > 0 {
            let k = 2 * i as u32;
            mesh.add_triangle(k - 2, k - 1, k);
            mesh.add_triangle(k - 1, k, k + 1);
        }
    }
    painter.add(Shape::mesh(mesh));

    // The depth floor: thicker when it's being grabbed.
    let floor_width = if hovered == Some(Target::Depth) { 2.0_f32 } else { 1.0 };
    let floor_y = plot.pos(0.0, 1.0 - shape.depth).y;
    painter.add(Shape::dashed_line(
        &[pos2(area.left(), floor_y), pos2(area.right(), floor_y)],
        Stroke::new(floor_width, theme::RED),
        4.0,
        4.0,
    ));

    painter.add(Shape::line(points, Stroke::new(2.0_f32, theme::CYAN)));

    // Handles: filled when hovered or held.
    let h = handles(shape, view);
    for (target, (x, g)) in [(Target::Hold, h.hold), (Target::Release, h.release)] {
        let fill = if hovered == Some(target) { theme::CYAN_LIGHT } else { theme::BACKGROUND };
        painter.circle(plot.pos(x, g), 5.0, fill, Stroke::new(2.0_f32, theme::CYAN_LIGHT));
    }

    // Playhead: a line across the plot and a dot on the curve.
    if let Some(x) = playhead {
        let p = plot.pos(x, view.gain_at(shape, x));
        painter.line_segment([pos2(p.x, area.top()), pos2(p.x, area.bottom())], Stroke::new(1.0_f32, theme::PEACH.linear_multiply(0.7)));
        painter.circle_filled(p, 4.0, theme::PEACH);
    }

    painter.text(area.right_bottom() - vec2(2.0, 2.0), egui::Align2::RIGHT_BOTTOM, status, small, theme::TEXT_DIM);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape() -> DuckShape {
        DuckShape { depth: 0.8, attack: 0.001, hold: 0.05, release: 0.2, tension: 0.4 }.sanitized()
    }

    fn plot() -> Plot {
        Plot::for_rect(Rect::from_min_size(Pos2::ZERO, vec2(438.0, 120.0)))
    }

    #[test]
    fn curve_runs_across_the_cycle_from_the_trigger() {
        let s = shape();
        let pts = curve_points(&s, &View::tempo(0.5));
        assert_eq!(pts.first().unwrap(), &(0.0, 1.0));
        assert_eq!(pts.last().unwrap().0, 1.0);
        assert!(pts.windows(2).all(|w| w[0].0 < w[1].0), "x must increase");
        // It reaches the floor (1 - depth) and recovers to unity in the cycle.
        let low = pts.iter().map(|p| p.1).fold(1.0, f32::min);
        assert!((low - 0.2).abs() < 1e-6, "{low}");
        assert_eq!(pts.last().unwrap().1, 1.0);
    }

    #[test]
    fn corners_are_drawn_exactly() {
        let s = shape();
        let pts = curve_points(&s, &View::tempo(0.5));
        // The attack's end (1 ms, well inside the first step) is a point.
        assert!(pts.iter().any(|&(x, g)| (x - 0.002).abs() < 1e-6 && (g - 0.2).abs() < 1e-6));
        // So is the hold's end.
        assert!(pts.iter().any(|&(x, _)| (x - 0.102).abs() < 1e-6));
    }

    #[test]
    fn handles_sit_at_the_hold_end_and_mid_release() {
        let s = shape();
        let v = View::tempo(0.5);
        let h = handles(&s, &v);
        assert!((h.hold.0 - 0.102).abs() < 1e-6);
        assert!((h.hold.1 - 0.2).abs() < 1e-6);
        // Halfway through the release (0.051 + 0.1 s), on the curve.
        assert!((h.release.0 - 0.302).abs() < 1e-6);
        assert!((h.release.1 - v.gain_at(&s, 0.302)).abs() < 1e-6);
        assert!(h.release.1 > 0.2 && h.release.1 < 1.0);
        // Never past the end of the cycle.
        let long = DuckShape { hold: 0.25, release: 0.5, ..s };
        assert_eq!(handles(&long, &v).release.0, 1.0);
    }

    #[test]
    fn playhead_gain_matches_the_curve() {
        let s = shape();
        let v = View::tempo(0.5);
        assert_eq!(v.gain_at(&s, 0.0), 1.0);
        assert!((v.gain_at(&s, 0.05) - 0.2).abs() < 1e-6, "in the hold");
        assert_eq!(v.gain_at(&s, 0.99), 1.0);
        // Out-of-range positions are clamped, not extrapolated.
        assert_eq!(v.gain_at(&s, -1.0), 1.0);
    }

    #[test]
    fn hit_test_finds_each_handle_and_the_floor() {
        let s = shape();
        let h = handles(&s, &View::tempo(0.5));
        let p = plot();
        let at = |(x, g): (f32, f32)| p.pos(x, g);
        assert_eq!(hit_test(&p, &h, at(h.hold), true), Some(Target::Hold));
        assert_eq!(hit_test(&p, &h, at(h.release) + vec2(3.0, -3.0), true), Some(Target::Release));
        // On the floor away from the hold handle.
        assert_eq!(hit_test(&p, &h, at((0.9, h.floor)) + vec2(0.0, 4.0), true), Some(Target::Depth));
        assert_eq!(hit_test(&p, &h, at((0.9, h.floor)), false), None, "mix 0: floor not draggable");
        // Empty space.
        assert_eq!(hit_test(&p, &h, at((0.9, 0.6)), true), None);
    }

    #[test]
    fn release_handle_wins_when_it_overlaps_the_hold_handle() {
        // Full tension and a tiny release put both handles on the floor
        // close together.
        let s = DuckShape { release: 0.002, tension: 1.0, ..shape() };
        let h = handles(&s, &View::tempo(0.5));
        assert_eq!(hit_test(&plot(), &h, plot().pos(h.hold.0, h.hold.1), true), Some(Target::Release));
    }

    #[test]
    fn drags_move_the_right_values_and_stay_in_range() {
        let limits = Limits::for_mode(Mode::Tempo, 1.0);
        let start = DragValues { hold: 0.1, release: 0.5, tension: 0.4, depth: 0.5 };
        let mut v = start;
        v.drag(Target::Hold, 0.1, 0.3, 1.0, &limits);
        assert_eq!(v, DragValues { hold: 0.2, ..start }, "hold ignores vertical moves");
        let mut v = start;
        v.drag(Target::Release, 0.1, 0.1, 1.0, &limits);
        assert!((v.release - 0.7).abs() < 1e-6, "release moves twice as far");
        assert!((v.tension - 0.6).abs() < 1e-6, "down = more tension");
        assert_eq!((v.hold, v.depth), (start.hold, start.depth));
        // Depth follows the floor: with Mix 50 % it moves twice as far.
        let mut v = start;
        v.drag(Target::Depth, 0.2, 0.1, 0.5, &limits);
        assert!((v.depth - 0.7).abs() < 1e-6);
        // Clamped at every edge, and turning back responds at once.
        let mut v = start;
        v.drag(Target::Hold, 5.0, 0.0, 1.0, &limits);
        assert_eq!(v.hold, HOLD_MAX);
        v.drag(Target::Hold, -0.1, 0.0, 1.0, &limits);
        assert!((v.hold - (HOLD_MAX - 0.1)).abs() < 1e-6);
        v.drag(Target::Release, -5.0, -5.0, 1.0, &limits);
        assert_eq!((v.release, v.tension), (RELEASE_MIN, -1.0));
        v.drag(Target::Depth, 0.0, 5.0, 1.0, &limits);
        assert_eq!(v.depth, 1.0);
        // MIDI mode's limits are the ms parameters' ranges.
        let mut v = start;
        v.drag(Target::Release, 10.0, 0.0, 1.0, &Limits::for_mode(Mode::Midi, 1.0));
        assert_eq!(v.release, RELEASE_MS_MAX / 1000.0);
    }

    #[test]
    fn midi_view_fits_the_curve_with_room_after_it() {
        // The default MIDI curve: 1 ms + 0 + 300 ms.
        let params = DragonPumpParams::default();
        let s = display_shape(&params, Mode::Midi, 0.5);
        assert!((s.length() - 0.301).abs() < 1e-6);
        let v = View::for_mode(Mode::Midi, 0.5, &s);
        assert_eq!(v, View { base: TimeBase::Free, span: 0.5 });
        // The curve ends at rest well inside the view and stays there: no
        // carry-over from a previous cycle.
        let pts = curve_points(&s, &v);
        assert_eq!(pts.first().unwrap().1, 1.0);
        assert!(pts.iter().filter(|p| p.0 > 0.62).all(|p| p.1 == 1.0));
        // Longest curve: 20 ms + 500 ms + 2 s fits the longest span.
        assert_eq!(View::free(2.52).span, 3.0);
        assert_eq!(View::free(0.1).span, 0.25);
    }

    #[test]
    fn tempo_view_carries_a_long_release_over() {
        // Release longer than the cycle: the curve starts where it ends.
        let s = DuckShape { release: 0.8, ..shape() };
        let pts = curve_points(&s, &View::tempo(0.5));
        assert!(pts[0].1 < 1.0);
        assert!((pts[0].1 - pts.last().unwrap().1).abs() < 1e-3);
    }

    /// A host stand-in that records what the editor sends it.
    #[derive(Default)]
    struct RecordingHost {
        calls: std::sync::Mutex<Vec<(&'static str, ParamPtr, f32)>>,
    }

    impl GuiContext for RecordingHost {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        fn request_resize(&self) -> bool {
            false
        }
        unsafe fn raw_begin_set_parameter(&self, p: ParamPtr) {
            self.calls.lock().unwrap().push(("begin", p, 0.0));
        }
        unsafe fn raw_set_parameter_normalized(&self, p: ParamPtr, v: f32) {
            self.calls.lock().unwrap().push(("set", p, v));
        }
        unsafe fn raw_end_set_parameter(&self, p: ParamPtr) {
            self.calls.lock().unwrap().push(("end", p, 0.0));
        }
        fn get_state(&self) -> PluginState {
            unimplemented!()
        }
        fn set_state(&self, _: PluginState) {}
    }

    /// Drag through egui, headless, from `from` by `by` over a few frames,
    /// with the plot at the window's top-left.
    fn drag(params: &DragonPumpParams, mode: Mode, host: &RecordingHost, from: Pos2, by: egui::Vec2) {
        let size = vec2(447.0, 168.0);
        let setter = ParamSetter::new(host);
        let ctx = egui::Context::default();
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let frames = [
            vec![egui::Event::PointerMoved(from)],
            vec![button(from, true)],
            vec![egui::Event::PointerMoved(from + by * 0.5)],
            vec![egui::Event::PointerMoved(from + by)],
            vec![button(from + by, false)],
            vec![],
        ];
        for (i, events) in frames.into_iter().enumerate() {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                events,
                time: Some(i as f64 * 0.1),
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx, |ui| {
                    show(ui, size, params, &setter, mode, 0.5, None, "");
                });
            });
        }
    }

    #[test]
    fn dragging_the_hold_handle_is_one_gesture_on_hold() {
        let params = DragonPumpParams::default();
        let host = RecordingHost::default();
        let plot = Plot::for_rect(Rect::from_min_size(Pos2::ZERO, vec2(447.0, 168.0)));
        let h = handles(&display_shape(&params, Mode::Tempo, 0.5), &View::tempo(0.5));
        // Drag right by a tenth of the plot: Hold goes to 10 % of the cycle.
        let by = vec2(0.1 * plot.area.width(), 0.0);
        drag(&params, Mode::Tempo, &host, plot.pos(h.hold.0, h.hold.1), by);

        let calls = host.calls.lock().unwrap();
        let hold = params.hold_pct.as_ptr();
        let kinds: Vec<_> = calls.iter().map(|c| c.0).collect();
        assert_eq!(kinds.first(), Some(&"begin"), "{kinds:?}");
        assert_eq!(kinds.last(), Some(&"end"), "{kinds:?}");
        assert!(calls.iter().all(|c| c.1 == hold), "only Hold is touched");
        assert_eq!(kinds.iter().filter(|k| **k == "begin").count(), 1);
        let last = calls.iter().rev().find(|c| c.0 == "set").expect("no value sent").2;
        let plain = params.hold_pct.preview_plain(last);
        assert!((plain - 0.1).abs() < 0.01, "hold {plain}");
    }

    #[test]
    fn dragging_empty_space_changes_nothing() {
        let params = DragonPumpParams::default();
        let host = RecordingHost::default();
        let plot = Plot::for_rect(Rect::from_min_size(Pos2::ZERO, vec2(447.0, 168.0)));
        drag(&params, Mode::Tempo, &host, plot.pos(0.9, 0.6), vec2(30.0, 30.0));
        assert!(host.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn midi_mode_drags_edit_the_ms_parameters() {
        let params = DragonPumpParams::default();
        let host = RecordingHost::default();
        let plot = Plot::for_rect(Rect::from_min_size(Pos2::ZERO, vec2(447.0, 168.0)));
        let s = display_shape(&params, Mode::Midi, 0.5);
        let v = View::for_mode(Mode::Midi, 0.5, &s);
        let h = handles(&s, &v);
        // A tenth of the 0.5 s view to the right: Hold 50 ms.
        drag(&params, Mode::Midi, &host, plot.pos(h.hold.0, h.hold.1), vec2(0.1 * plot.area.width(), 0.0));
        let calls = host.calls.lock().unwrap();
        assert!(calls.iter().all(|c| c.1 == params.hold_ms.as_ptr()), "only Hold (ms) is touched");
        let last = calls.iter().rev().find(|c| c.0 == "set").expect("no value sent").2;
        let ms = params.hold_ms.preview_plain(last);
        assert!((ms - 50.0).abs() < 2.0, "hold {ms} ms");
    }

    #[test]
    fn display_shape_folds_mix_into_depth_and_scales_by_the_cycle() {
        let params = DragonPumpParams::default();
        let s = display_shape(&params, Mode::Tempo, 0.5);
        assert_eq!(s.depth, 1.0);
        assert!((s.attack - 0.001).abs() < 1e-6);
        assert!((s.release - 0.3).abs() < 1e-6);
        assert_eq!(s.hold, 0.0);
    }
}
