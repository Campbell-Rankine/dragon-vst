//! Voice allocation for poly mode.
//!
//! Policy:
//! - A new note takes an idle slot if the polyphony limit allows.
//! - At the limit, a sounding voice is *stolen*: the oldest released voice
//!   first, otherwise the oldest held one. The victim fades out over a few
//!   milliseconds in its own slot while the new note starts immediately in
//!   a free one, so stealing neither clicks nor delays the new note.
//! - The pool has a few spare slots for those fades. Only if every slot is
//!   busy is the quietest fading voice cut off outright.

use super::voice::{Voice, VoiceState};

/// Fade out sounding voices, oldest first (released before held), until at
/// most `limit` remain.
pub(crate) fn enforce_limit(voices: &mut [Voice], limit: usize) {
    loop {
        let sounding = voices.iter().filter(|v| v.is_sounding()).count();
        if sounding <= limit {
            return;
        }
        let victim = voices
            .iter()
            .enumerate()
            .filter(|(_, v)| v.is_sounding())
            // Released (false < true) before held, then oldest.
            .min_by_key(|(_, v)| (v.state == VoiceState::Held, v.age))
            .map(|(i, _)| i);
        match victim {
            Some(i) => voices[i].state = VoiceState::Fading,
            None => return,
        }
    }
}

/// A slot for a new note: an idle one, or failing that the quietest fading
/// voice. Returns the slot and whether it cut off a voice that was still
/// sounding (which the caller must report as ended).
pub(crate) fn free_slot(voices: &[Voice]) -> (usize, bool) {
    if let Some(i) = voices.iter().position(|v| v.state == VoiceState::Idle) {
        return (i, false);
    }
    let i = voices
        .iter()
        .enumerate()
        .filter(|(_, v)| v.state == VoiceState::Fading)
        .min_by(|(_, a), (_, b)| a.fade.total_cmp(&b.fade))
        // Unreachable while the pool is larger than the polyphony limit, but
        // never panic on the audio thread: take the oldest voice instead.
        .or_else(|| voices.iter().enumerate().min_by_key(|(_, v)| v.age))
        .map(|(i, _)| i)
        .unwrap_or(0);
    (i, true)
}
