//! Shared egui widgets and palette for the dragon-vst plugin editors, so
//! every plugin looks and behaves the same.
//!
//! - [`knob::Knob`]: the rotary knob for any nih-plug parameter.
//! - [`theme`]: the palette (background, panels, mint / cyan / red / peach).
//! - [`widgets`]: titled panels, ON/OFF pills, and helpers.
//! - [`perf`]: the DSP / GUI load meter for an editor's header.

pub mod knob;
pub mod perf;
pub mod theme;
pub mod widgets;

pub use knob::Knob;
