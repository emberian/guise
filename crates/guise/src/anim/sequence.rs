//! A sequence: several motions on one element, placed on a shared clock.
//!
//! anime.js calls this a timeline. It is the same idea — position each
//! motion absolutely, relative to what came before, or against a named
//! label, then sample the whole thing as one clip. Overlapping motions
//! layer: a later entry writing the same property wins for that frame,
//! which is what lets a slide-in and a colour change share an element.
//!
//! For the *other* kind of choreography — the same motion across many
//! elements, offset per index — see [`Stagger`](super::Stagger). One element
//! is one clip, so N elements are N clips with N delays, not one timeline
//! with N targets.

use gpui::SharedString;

use super::motion::fold_time;
use super::{Frame, Loop, Motion};

/// Where a motion starts inside a sequence.
#[derive(Debug, Clone, PartialEq)]
pub enum At {
    /// After everything already added — the default, and what `add` uses.
    End,
    /// A fixed offset from the sequence's own start.
    Abs(f32),
    /// Relative to the end of everything already added. Negative overlaps
    /// with the tail of the previous motion.
    Rel(f32),
    /// The same start as the previously added motion, plus an offset — two
    /// motions running together.
    With(f32),
    /// An offset from a label placed with [`Sequence::label`].
    Label(SharedString, f32),
}

impl At {
    /// `At::With(0.0)` — start alongside the previous motion.
    pub fn with_previous() -> Self {
        At::With(0.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Entry {
    start: f32,
    motion: Motion,
}

/// Motions placed on one clock.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sequence {
    entries: Vec<Entry>,
    labels: Vec<(SharedString, f32)>,
    loops: Loop,
    alternate: bool,
    reversed: bool,
    previous_start: f32,
}

impl Sequence {
    pub fn new() -> Self {
        Sequence::default()
    }

    /// Add a motion after everything already in the sequence.
    // Named for what a timeline does, not for `std::ops::Add`. `push` would be
    // a list's word and this is not a list.
    #[allow(clippy::should_implement_trait)]
    pub fn add(self, motion: Motion) -> Self {
        self.add_at(motion, At::End)
    }

    pub fn add_at(mut self, motion: Motion, at: At) -> Self {
        let start = self.resolve(&at).max(0.0);
        self.previous_start = start;
        self.entries.push(Entry { start, motion });
        self
    }

    /// Name a position so later entries can hang off it.
    pub fn label(mut self, name: impl Into<SharedString>, at: At) -> Self {
        let position = self.resolve(&at).max(0.0);
        self.labels.push((name.into(), position));
        self
    }

    pub fn loops(mut self, loops: Loop) -> Self {
        self.loops = loops;
        self
    }

    pub fn repeat(mut self, times: u32) -> Self {
        self.loops = Loop::Times(times);
        self
    }

    pub fn repeat_forever(mut self) -> Self {
        self.loops = Loop::Forever;
        self
    }

    pub fn alternate(mut self, alternate: bool) -> Self {
        self.alternate = alternate;
        self
    }

    pub fn reversed(mut self, reversed: bool) -> Self {
        self.reversed = reversed;
        self
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether every other pass runs backwards.
    pub fn is_alternating(&self) -> bool {
        self.alternate
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Where a position lands, in milliseconds from the sequence's start.
    pub fn resolve(&self, at: &At) -> f32 {
        match at {
            At::End => self.iteration_ms(),
            At::Abs(ms) => *ms,
            At::Rel(ms) => self.iteration_ms() + ms,
            At::With(ms) => self.previous_start + ms,
            At::Label(name, ms) => {
                let base = self
                    .labels
                    .iter()
                    .find(|(label, _)| label == name)
                    .map(|(_, position)| *position)
                    .unwrap_or(0.0);
                base + ms
            }
        }
    }

    /// One pass through every entry. A child that loops forever contributes
    /// a single iteration here — it keeps looping, but it doesn't stretch the
    /// sequence to infinity.
    pub fn iteration_ms(&self) -> f32 {
        self.entries
            .iter()
            .map(|entry| {
                let span = match entry.motion.loops {
                    Loop::Forever => entry.motion.iteration_ms(),
                    _ => entry.motion.total_ms(),
                };
                entry.start + span
            })
            .fold(0.0_f32, f32::max)
    }

    pub fn total_ms(&self) -> f32 {
        match self.loops.count() {
            Some(n) => self.iteration_ms() * n as f32,
            None => f32::INFINITY,
        }
    }

    pub fn sample(&self, t: f32) -> Frame {
        let mut frame = Frame::new();
        self.sample_into(t, &mut frame);
        frame
    }

    pub fn sample_into(&self, t: f32, frame: &mut Frame) {
        // `iteration_ms` walks every entry and every track inside it, and
        // `total_ms` would walk them all again for the same answer.
        let (local, progress, finished) = fold_time(
            t,
            self.iteration_ms(),
            self.loops,
            self.alternate,
            self.reversed,
        );

        for entry in &self.entries {
            // A motion contributes nothing before its turn: the element keeps
            // whatever the host styled it with until the clip reaches it.
            if local + f32::EPSILON >= entry.start {
                entry.motion.sample_into(local - entry.start, frame);
            }
        }

        frame.progress = progress;
        frame.finished = finished;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::{Easing, Prop};

    fn leg(prop: Prop, from: f32, to: f32, ms: f32) -> Motion {
        Motion::new()
            .duration(ms)
            .ease(Easing::Linear)
            .tween(prop, from, to)
    }

    #[test]
    fn entries_queue_up_end_to_end() {
        let sequence = Sequence::new()
            .add(leg(Prop::Opacity, 0.0, 1.0, 100.0))
            .add(leg(Prop::X, 0.0, 50.0, 100.0));
        assert_eq!(sequence.iteration_ms(), 200.0);

        let early = sequence.sample(50.0);
        assert!((early.number(Prop::Opacity).unwrap() - 0.5).abs() < 1e-5);
        assert_eq!(early.number(Prop::X), None, "not its turn yet");

        let late = sequence.sample(150.0);
        assert_eq!(late.number(Prop::Opacity), Some(1.0));
        assert!((late.number(Prop::X).unwrap() - 25.0).abs() < 1e-4);
    }

    #[test]
    fn relative_placement_overlaps_the_tail() {
        let sequence = Sequence::new()
            .add(leg(Prop::Opacity, 0.0, 1.0, 100.0))
            .add_at(leg(Prop::X, 0.0, 50.0, 100.0), At::Rel(-50.0));
        assert_eq!(sequence.iteration_ms(), 150.0);
        let mid = sequence.sample(75.0);
        assert!(mid.number(Prop::Opacity).unwrap() > 0.5);
        assert!(mid.number(Prop::X).unwrap() > 0.0);
    }

    #[test]
    fn with_previous_starts_them_together() {
        let sequence = Sequence::new()
            .add(leg(Prop::Opacity, 0.0, 1.0, 100.0))
            .add_at(leg(Prop::X, 0.0, 50.0, 100.0), At::with_previous());
        assert_eq!(sequence.iteration_ms(), 100.0);
        let mid = sequence.sample(50.0);
        assert!((mid.number(Prop::Opacity).unwrap() - 0.5).abs() < 1e-5);
        assert!((mid.number(Prop::X).unwrap() - 25.0).abs() < 1e-4);
    }

    #[test]
    fn labels_anchor_later_entries() {
        let sequence = Sequence::new()
            .add(leg(Prop::Opacity, 0.0, 1.0, 100.0))
            .label("settled", At::End)
            .add(leg(Prop::X, 0.0, 50.0, 100.0))
            .add_at(
                leg(Prop::Y, 0.0, 10.0, 50.0),
                At::Label("settled".into(), 25.0),
            );
        assert_eq!(sequence.resolve(&At::Label("settled".into(), 0.0)), 100.0);
        // The label sits at 100, so the Y leg is still waiting at 120.
        assert_eq!(sequence.sample(120.0).number(Prop::Y), None);
        assert!(sequence.sample(130.0).number(Prop::Y).is_some());
    }

    #[test]
    fn a_missing_label_falls_back_to_the_start() {
        let sequence = Sequence::new().add(leg(Prop::X, 0.0, 1.0, 100.0));
        assert_eq!(sequence.resolve(&At::Label("nope".into(), 10.0)), 10.0);
    }

    #[test]
    fn the_last_writer_wins_when_motions_overlap() {
        let sequence = Sequence::new()
            .add(leg(Prop::Opacity, 0.0, 1.0, 100.0))
            .add_at(leg(Prop::Opacity, 1.0, 0.0, 100.0), At::with_previous());
        assert!((sequence.sample(50.0).number(Prop::Opacity).unwrap() - 0.5).abs() < 1e-5);
        assert_eq!(sequence.sample(100.0).number(Prop::Opacity), Some(0.0));
    }

    #[test]
    fn a_forever_child_does_not_make_the_sequence_infinite() {
        let sequence = Sequence::new().add(leg(Prop::Opacity, 0.0, 1.0, 100.0).repeat_forever());
        assert_eq!(sequence.iteration_ms(), 100.0);
        assert!(sequence.total_ms().is_finite());
    }

    #[test]
    fn the_sequence_can_loop_as_a_whole() {
        let sequence = Sequence::new()
            .add(leg(Prop::Opacity, 0.0, 1.0, 100.0))
            .repeat(2);
        assert_eq!(sequence.total_ms(), 200.0);
        assert!((sequence.sample(150.0).number(Prop::Opacity).unwrap() - 0.5).abs() < 1e-5);
        assert!(sequence.sample(200.0).finished);
    }
}
