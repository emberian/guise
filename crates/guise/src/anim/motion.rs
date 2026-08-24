//! A motion: what moves, from where to where, over how long.
//!
//! This is the declarative half of the animation system and it is pure —
//! `Motion::sample(t)` maps a millisecond offset to a [`Frame`] with no
//! state, no clock, and no gpui. Everything else (the one-shot element, the
//! controllable [`Animator`](super::Animator), the [`Sequence`](super::Sequence))
//! is a way of deciding which `t` to ask for.
//!
//! The shape follows anime.js: timing on the motion, optional per-keyframe
//! overrides, and a loop/alternate pair for repetition.

use super::{AnimValue, Easing, Frame, Prop};
use crate::transition::TransitionKind;

/// How many times a motion runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Loop {
    #[default]
    Once,
    /// Exactly `n` iterations (`0` and `1` both mean once).
    Times(u32),
    Forever,
}

impl Loop {
    /// The iteration count, or `None` when it never ends.
    pub fn count(self) -> Option<u32> {
        match self {
            Loop::Once => Some(1),
            Loop::Times(n) => Some(n.max(1)),
            Loop::Forever => None,
        }
    }
}

/// One destination in a track, with optional overrides for how long the leg
/// to it takes and how it eases.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keyframe {
    pub value: AnimValue,
    /// Milliseconds for this leg. `None` takes an even share of whatever the
    /// motion's `duration` has left over after the fixed legs.
    pub duration: Option<f32>,
    /// Milliseconds to hold the previous value before this leg starts.
    pub delay: f32,
    /// `None` inherits the motion's easing.
    pub ease: Option<Easing>,
}

impl Keyframe {
    pub fn to(value: impl Into<AnimValue>) -> Self {
        Keyframe {
            value: value.into(),
            duration: None,
            delay: 0.0,
            ease: None,
        }
    }

    pub fn duration(mut self, ms: f32) -> Self {
        self.duration = Some(ms.max(0.0));
        self
    }

    pub fn delay(mut self, ms: f32) -> Self {
        self.delay = ms.max(0.0);
        self
    }

    pub fn ease(mut self, easing: Easing) -> Self {
        self.ease = Some(easing);
        self
    }
}

/// Anything that can stand in for a [`Keyframe`] in a list.
///
/// A list of legs is usually a list of destinations — `[-30.0, 0.0]` — and
/// only sometimes a list of built keyframes with their own timings. Rust
/// arrays are homogeneous either way, so one trait covers both without any
/// wrapping at the call site.
pub trait IntoKeyframe {
    fn into_keyframe(self) -> Keyframe;
}

impl IntoKeyframe for Keyframe {
    fn into_keyframe(self) -> Keyframe {
        self
    }
}

macro_rules! keyframe_from_value {
    ($($ty:ty),* $(,)?) => {
        $(
            impl IntoKeyframe for $ty {
                fn into_keyframe(self) -> Keyframe {
                    Keyframe::to(self)
                }
            }
        )*
    };
}

keyframe_from_value!(
    f32,
    f64,
    i32,
    gpui::Pixels,
    gpui::Hsla,
    gpui::Rgba,
    AnimValue
);

/// One property's path: where it starts, and every leg from there.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub prop: Prop,
    /// The value before the first leg begins. There is no element to read a
    /// current value off of, so a track always says where it starts.
    pub from: AnimValue,
    pub frames: Vec<Keyframe>,
}

impl Track {
    pub fn new(prop: Prop, from: impl Into<AnimValue>) -> Self {
        Track {
            prop,
            from: from.into(),
            frames: Vec::new(),
        }
    }

    pub fn to(mut self, value: impl Into<AnimValue>) -> Self {
        self.frames.push(Keyframe::to(value));
        self
    }

    pub fn keyframe(mut self, frame: Keyframe) -> Self {
        self.frames.push(frame);
        self
    }

    /// Split `duration` between the legs that didn't ask for a specific one.
    fn leg_duration(&self, duration: f32) -> f32 {
        let mut fixed = 0.0;
        let mut flexible = 0usize;
        for frame in &self.frames {
            fixed += frame.delay;
            match frame.duration {
                Some(d) => fixed += d,
                None => flexible += 1,
            }
        }
        if flexible == 0 {
            0.0
        } else {
            ((duration - fixed) / flexible as f32).max(0.0)
        }
    }

    /// How long this track runs. Never shorter than the fixed legs it
    /// declares, so an over-long keyframe stretches the motion rather than
    /// being cut off.
    pub fn span(&self, duration: f32) -> f32 {
        let each = self.leg_duration(duration);
        self.frames
            .iter()
            .map(|frame| frame.delay + frame.duration.unwrap_or(each))
            .sum()
    }

    /// The value at `t` ms into the track. Before the first leg it holds
    /// `from`; after the last it holds the final value.
    pub fn sample(&self, t: f32, duration: f32, ease: Easing) -> AnimValue {
        let each = self.leg_duration(duration);
        let mut value = self.from;
        let mut cursor = 0.0;
        for frame in &self.frames {
            let leg = frame.duration.unwrap_or(each);
            let start = cursor + frame.delay;
            if t <= start {
                return value;
            }
            let end = start + leg;
            if t < end {
                let local = if leg <= 0.0 { 1.0 } else { (t - start) / leg };
                let curve = frame.ease.unwrap_or(ease);
                return value.lerp(frame.value, curve.apply(local));
            }
            value = frame.value;
            cursor = end;
        }
        value
    }
}

/// A set of tracks that run together, with shared timing.
#[derive(Debug, Clone, PartialEq)]
pub struct Motion {
    pub tracks: Vec<Track>,
    /// Milliseconds each track gets, before per-keyframe overrides.
    pub duration: f32,
    /// Milliseconds of stillness before the tracks start. The tracks hold
    /// their `from` value throughout — which is what makes a staggered
    /// entrance stay hidden until its turn.
    pub delay: f32,
    /// Milliseconds of stillness after the tracks finish, inside the
    /// iteration — so a loop pauses at the end instead of snapping back.
    pub end_delay: f32,
    pub ease: Easing,
    pub loops: Loop,
    /// Play every other iteration backwards.
    pub alternate: bool,
    /// Play the whole thing backwards.
    pub reversed: bool,
}

impl Default for Motion {
    fn default() -> Self {
        Motion {
            tracks: Vec::new(),
            duration: 300.0,
            delay: 0.0,
            end_delay: 0.0,
            ease: Easing::default(),
            loops: Loop::Once,
            alternate: false,
            reversed: false,
        }
    }
}

impl Motion {
    pub fn new() -> Self {
        Motion::default()
    }

    /// A single leg from `from` to `to`.
    pub fn tween(
        mut self,
        prop: Prop,
        from: impl Into<AnimValue>,
        to: impl Into<AnimValue>,
    ) -> Self {
        self.tracks.push(Track::new(prop, from).to(to));
        self
    }

    /// A multi-leg path for one property. The legs are destinations, or
    /// [`Keyframe`]s when a leg needs its own duration or easing.
    pub fn keyframes<K: IntoKeyframe>(
        mut self,
        prop: Prop,
        from: impl Into<AnimValue>,
        frames: impl IntoIterator<Item = K>,
    ) -> Self {
        let mut track = Track::new(prop, from);
        track
            .frames
            .extend(frames.into_iter().map(IntoKeyframe::into_keyframe));
        self.tracks.push(track);
        self
    }

    pub fn track(mut self, track: Track) -> Self {
        self.tracks.push(track);
        self
    }

    /// Milliseconds. Springs ignore it — they run for their settle time.
    pub fn duration(mut self, ms: f32) -> Self {
        self.duration = ms.max(0.0);
        self
    }

    pub fn delay(mut self, ms: f32) -> Self {
        self.delay = ms.max(0.0);
        self
    }

    pub fn end_delay(mut self, ms: f32) -> Self {
        self.end_delay = ms.max(0.0);
        self
    }

    pub fn ease(mut self, easing: Easing) -> Self {
        self.ease = easing;
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

    /// The duration of one pass, delays included.
    pub fn iteration_ms(&self) -> f32 {
        let span = self
            .tracks
            .iter()
            .map(|track| track.span(self.duration))
            .fold(0.0_f32, f32::max);
        self.delay + span + self.end_delay
    }

    /// The duration of every pass together, or `f32::INFINITY` when it loops
    /// forever.
    pub fn total_ms(&self) -> f32 {
        match self.loops.count() {
            Some(n) => self.iteration_ms() * n as f32,
            None => f32::INFINITY,
        }
    }

    /// The values `t` milliseconds in.
    pub fn sample(&self, t: f32) -> Frame {
        let mut frame = Frame::new();
        self.sample_into(t, &mut frame);
        frame
    }

    /// Sample into an existing frame. Later writers win, which is how a
    /// [`Sequence`](super::Sequence) layers overlapping motions.
    pub fn sample_into(&self, t: f32, frame: &mut Frame) {
        // One walk of the tracks, not two: `total_ms` would recompute the
        // same span, and this runs for every animated element every frame.
        let (local, progress, finished) = fold_time(
            t,
            self.iteration_ms(),
            self.loops,
            self.alternate,
            self.reversed,
        );
        // Negative here means "still in the leading delay", and every track
        // reports its starting value for it — a staggered entrance has to
        // stay hidden until its turn, not flash and then animate.
        let track_time = local - self.delay;
        for track in &self.tracks {
            frame.set(
                track.prop,
                track.sample(track_time, self.duration, self.ease),
            );
        }
        frame.progress = progress;
        frame.finished = finished;
    }
}

/// Fold absolute time into one iteration: resolve the loop count, flip
/// alternating passes, and apply the direction. Shared by [`Motion`] and
/// [`Sequence`](super::Sequence), which repeat by exactly the same rules.
pub(crate) fn fold_time(
    t: f32,
    iteration: f32,
    loops: Loop,
    alternate: bool,
    reversed: bool,
) -> (f32, f32, bool) {
    let total = match loops.count() {
        Some(n) => iteration * n as f32,
        None => f32::INFINITY,
    };
    let t = t.max(0.0);
    let finished = total.is_finite() && t >= total;
    let (index, mut local) = if iteration <= 0.0 {
        (0u32, 0.0)
    } else if finished {
        // Rest on the end of the last iteration rather than wrapping to 0.
        (loops.count().unwrap_or(1).saturating_sub(1), iteration)
    } else {
        ((t / iteration) as u32, t % iteration)
    };

    if alternate && index % 2 == 1 {
        local = iteration - local;
    }
    if reversed {
        local = iteration - local;
    }

    let progress = if total.is_finite() {
        if total <= 0.0 {
            1.0
        } else {
            (t / total).clamp(0.0, 1.0)
        }
    } else if iteration <= 0.0 {
        0.0
    } else {
        (t % iteration) / iteration
    };

    (local, progress, finished)
}

/// How far a slide preset travels, in px.
pub const SLIDE_DISTANCE: f32 = 8.0;

impl Motion {
    /// The entrance [`Transition`](crate::Transition) plays, as a motion you
    /// can retime, delay, stagger or drop into a [`Sequence`](super::Sequence).
    pub fn enter(kind: TransitionKind) -> Self {
        Motion::enter_from(kind, SLIDE_DISTANCE)
    }

    /// [`Motion::enter`] with the slide distance spelled out.
    pub fn enter_from(kind: TransitionKind, distance: f32) -> Self {
        let motion = Motion::new().duration(200.0).tween(Prop::Opacity, 0.0, 1.0);
        match kind {
            TransitionKind::Fade => motion,
            TransitionKind::SlideUp => motion.tween(Prop::Y, distance, 0.0),
            TransitionKind::SlideDown => motion.tween(Prop::Y, -distance, 0.0),
            TransitionKind::SlideLeft => motion.tween(Prop::X, distance, 0.0),
            TransitionKind::SlideRight => motion.tween(Prop::X, -distance, 0.0),
        }
    }

    /// The mirror of [`Motion::enter`] — ends invisible and displaced.
    pub fn exit(kind: TransitionKind) -> Self {
        Motion::exit_to(kind, SLIDE_DISTANCE)
    }

    pub fn exit_to(kind: TransitionKind, distance: f32) -> Self {
        let motion = Motion::new().duration(160.0).tween(Prop::Opacity, 1.0, 0.0);
        match kind {
            TransitionKind::Fade => motion,
            TransitionKind::SlideUp => motion.tween(Prop::Y, 0.0, -distance),
            TransitionKind::SlideDown => motion.tween(Prop::Y, 0.0, distance),
            TransitionKind::SlideLeft => motion.tween(Prop::X, 0.0, -distance),
            TransitionKind::SlideRight => motion.tween(Prop::X, 0.0, distance),
        }
    }

    /// Re-express [`Prop::X`]/[`Prop::Y`] as margins.
    ///
    /// Those two are relative insets, which is the right way to move an
    /// element in flow — it slides and its neighbours do not care. But an
    /// element pinned with `absolute()` *is* its inset, so animating one
    /// would drag it off its pin and leave it wherever the motion ended.
    /// Margins offset a pinned element from where it was pinned, which is
    /// the same visible motion without the fight.
    pub fn as_margins(mut self) -> Self {
        for track in &mut self.tracks {
            track.prop = match track.prop {
                Prop::X => Prop::MarginLeft,
                Prop::Y => Prop::MarginTop,
                other => other,
            };
        }
        self
    }

    /// A breathing fade that never stops — the shape of a "working on it"
    /// hint that isn't a spinner.
    pub fn pulse() -> Self {
        Motion::new()
            .duration(900.0)
            .ease(Easing::InOut(super::ease::Curve::Sine))
            .alternate(true)
            .repeat_forever()
            .tween(Prop::Opacity, 1.0, 0.35)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opacity(motion: &Motion, t: f32) -> f32 {
        motion.sample(t).number(Prop::Opacity).unwrap()
    }

    #[test]
    fn a_tween_runs_from_end_to_end() {
        let motion =
            Motion::new()
                .duration(100.0)
                .ease(Easing::Linear)
                .tween(Prop::Opacity, 0.0, 1.0);
        assert_eq!(motion.iteration_ms(), 100.0);
        assert_eq!(opacity(&motion, 0.0), 0.0);
        assert!((opacity(&motion, 50.0) - 0.5).abs() < 1e-5);
        assert_eq!(opacity(&motion, 100.0), 1.0);
        assert_eq!(opacity(&motion, 500.0), 1.0);
    }

    #[test]
    fn a_delay_holds_the_starting_value() {
        let motion = Motion::new()
            .duration(100.0)
            .delay(50.0)
            .ease(Easing::Linear)
            .tween(Prop::Opacity, 0.0, 1.0);
        assert_eq!(motion.iteration_ms(), 150.0);
        assert_eq!(opacity(&motion, 0.0), 0.0);
        assert_eq!(opacity(&motion, 49.0), 0.0);
        assert!((opacity(&motion, 100.0) - 0.5).abs() < 1e-5);
        assert_eq!(opacity(&motion, 150.0), 1.0);
    }

    #[test]
    fn unsized_keyframes_split_the_duration_evenly() {
        let motion = Motion::new()
            .duration(300.0)
            .ease(Easing::Linear)
            .keyframes(
                Prop::X,
                0.0,
                [Keyframe::to(10.0), Keyframe::to(20.0), Keyframe::to(30.0)],
            );
        assert_eq!(motion.iteration_ms(), 300.0);
        assert!((motion.sample(100.0).number(Prop::X).unwrap() - 10.0).abs() < 1e-4);
        assert!((motion.sample(200.0).number(Prop::X).unwrap() - 20.0).abs() < 1e-4);
        assert!((motion.sample(300.0).number(Prop::X).unwrap() - 30.0).abs() < 1e-4);
    }

    #[test]
    fn a_fixed_leg_takes_its_time_and_the_rest_share() {
        let motion = Motion::new()
            .duration(300.0)
            .ease(Easing::Linear)
            .keyframes(
                Prop::X,
                0.0,
                [Keyframe::to(10.0).duration(200.0), Keyframe::to(20.0)],
            );
        assert_eq!(motion.iteration_ms(), 300.0);
        assert!((motion.sample(100.0).number(Prop::X).unwrap() - 5.0).abs() < 1e-4);
        assert!((motion.sample(250.0).number(Prop::X).unwrap() - 15.0).abs() < 1e-4);
    }

    #[test]
    fn over_long_legs_stretch_the_motion() {
        let motion = Motion::new().duration(100.0).keyframes(
            Prop::X,
            0.0,
            [Keyframe::to(1.0).duration(400.0)],
        );
        assert_eq!(motion.iteration_ms(), 400.0);
    }

    #[test]
    fn repeats_replay_the_iteration() {
        let motion = Motion::new()
            .duration(100.0)
            .ease(Easing::Linear)
            .repeat(3)
            .tween(Prop::Opacity, 0.0, 1.0);
        assert_eq!(motion.total_ms(), 300.0);
        assert!((opacity(&motion, 150.0) - 0.5).abs() < 1e-5);
        assert!(!motion.sample(299.0).finished);
        assert!(motion.sample(300.0).finished);
    }

    #[test]
    fn alternate_runs_odd_iterations_backwards() {
        let motion = Motion::new()
            .duration(100.0)
            .ease(Easing::Linear)
            .repeat(2)
            .alternate(true)
            .tween(Prop::Opacity, 0.0, 1.0);
        assert!((opacity(&motion, 50.0) - 0.5).abs() < 1e-5);
        assert!((opacity(&motion, 150.0) - 0.5).abs() < 1e-5);
        assert!(opacity(&motion, 120.0) > 0.75);
    }

    #[test]
    fn reversed_plays_from_the_far_end() {
        let motion = Motion::new()
            .duration(100.0)
            .ease(Easing::Linear)
            .reversed(true)
            .tween(Prop::Opacity, 0.0, 1.0);
        assert_eq!(opacity(&motion, 0.0), 1.0);
        assert_eq!(opacity(&motion, 100.0), 0.0);
    }

    #[test]
    fn forever_never_finishes() {
        let motion = Motion::pulse();
        assert!(motion.total_ms().is_infinite());
        let frame = motion.sample(10_000.0);
        assert!(!frame.finished);
        assert!((0.0..=1.0).contains(&frame.progress));
    }

    #[test]
    fn end_delay_holds_the_final_value_inside_the_iteration() {
        let motion = Motion::new()
            .duration(100.0)
            .end_delay(100.0)
            .ease(Easing::Linear)
            .tween(Prop::Opacity, 0.0, 1.0);
        assert_eq!(motion.iteration_ms(), 200.0);
        assert_eq!(opacity(&motion, 150.0), 1.0);
    }

    #[test]
    fn as_margins_moves_the_offsets_off_the_inset() {
        let motion = Motion::enter(TransitionKind::SlideUp).as_margins();
        let start = motion.sample(0.0);
        assert_eq!(start.number(Prop::Y), None);
        assert_eq!(start.number(Prop::MarginTop), Some(SLIDE_DISTANCE));
        // Opacity is untouched — only the two offsets move.
        assert_eq!(start.number(Prop::Opacity), Some(0.0));
    }

    #[test]
    fn presets_start_hidden_and_end_settled() {
        for kind in [
            TransitionKind::Fade,
            TransitionKind::SlideUp,
            TransitionKind::SlideDown,
            TransitionKind::SlideLeft,
            TransitionKind::SlideRight,
        ] {
            let motion = Motion::enter(kind);
            let start = motion.sample(0.0);
            let end = motion.sample(motion.total_ms());
            assert_eq!(start.number(Prop::Opacity), Some(0.0));
            assert_eq!(end.number(Prop::Opacity), Some(1.0));
            if kind != TransitionKind::Fade {
                let moved = end.number(Prop::X).or(end.number(Prop::Y)).unwrap();
                assert_eq!(moved, 0.0);
            }
        }
    }
}

#[cfg(test)]
mod bench {
    use super::*;
    use std::time::Instant;

    /// Not an assertion about wall-clock — a way to re-measure the sampling
    /// path when it changes. Ignored by default; run it with
    /// `cargo test --release -p guise-ui bench -- --ignored --nocapture`.
    ///
    /// For reference, on an M-series laptop: a two-track motion samples in
    /// ~19ns and a three-entry sequence in ~71ns, neither touching the heap.
    #[test]
    #[ignore = "a measurement, not a test"]
    fn sampling_cost() {
        let simple = Motion::enter(crate::TransitionKind::SlideUp);
        let heavy = Motion::new().duration(900.0).keyframes(
            Prop::Y,
            0.0,
            [
                Keyframe::to(10.0),
                Keyframe::to(20.0),
                Keyframe::to(30.0),
                Keyframe::to(40.0),
            ],
        );
        let sequence = crate::Sequence::new()
            .add(simple.clone())
            .add(heavy.clone())
            .add(simple.clone());

        type Case = (&'static str, Box<dyn Fn(f32)>);
        let cases: [Case; 3] = [
            (
                "motion(2 tracks)",
                Box::new(move |t| {
                    simple.sample(t);
                }),
            ),
            (
                "motion(4 legs)",
                Box::new(move |t| {
                    heavy.sample(t);
                }),
            ),
            (
                "sequence(3)",
                Box::new(move |t| {
                    sequence.sample(t);
                }),
            ),
        ];
        for (name, run) in cases {
            let start = Instant::now();
            for i in 0..100_000 {
                run(i as f32 * 0.01);
            }
            let each = start.elapsed().as_secs_f64() * 1e9 / 100_000.0;
            println!("{name:20} {each:8.1} ns/sample");
        }
    }
}
