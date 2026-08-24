//! One sampled instant of an animation: the set of properties that have a
//! value right now, plus how far through the clip that instant is.
//!
//! A frame is built fresh every render — sampling is pure, so nothing has to
//! be kept between frames and a paused animation costs exactly nothing.

use gpui::prelude::*;
use gpui::{px, Hsla};

use super::{AnimValue, Prop};

/// How many properties a frame holds without touching the heap.
///
/// A motion is two or three tracks in almost every case — an entrance is
/// opacity plus one offset. Sampling runs once per animated element per
/// frame, so the common case should not allocate at all; a sequence layering
/// more than this spills to a `Vec` and nothing else changes.
pub(crate) const INLINE: usize = 4;

/// The filler the unused inline slots hold. Never read: `set` and `iter`
/// bound themselves by `inline_len`.
const EMPTY: (Prop, AnimValue) = (Prop::Opacity, AnimValue::Number(0.0));

/// The values an animation resolves to at one moment in time.
#[derive(Debug, Clone)]
pub struct Frame {
    inline: [(Prop, AnimValue); INLINE],
    inline_len: usize,
    spill: Vec<(Prop, AnimValue)>,
    /// 0..=1 through the whole clip, loops included.
    pub progress: f32,
    /// True once the clip has run out of time. Always false while looping
    /// forever.
    pub finished: bool,
}

impl Default for Frame {
    fn default() -> Self {
        Frame {
            inline: [EMPTY; INLINE],
            inline_len: 0,
            spill: Vec::new(),
            progress: 0.0,
            finished: false,
        }
    }
}

impl PartialEq for Frame {
    /// Compared by what it holds, not by where it holds it — two frames with
    /// the same properties are equal whether or not one of them spilled.
    fn eq(&self, other: &Self) -> bool {
        self.progress == other.progress
            && self.finished == other.finished
            && self.len() == other.len()
            && self.iter().eq(other.iter())
    }
}

impl Frame {
    pub fn new() -> Self {
        Frame::default()
    }

    /// Set a property, replacing any value already there. Later writers win,
    /// which is what layers a sequence's overlapping tracks.
    pub fn set(&mut self, prop: Prop, value: impl Into<AnimValue>) {
        let value = value.into();
        if let Some(slot) = self.inline[..self.inline_len]
            .iter_mut()
            .find(|(p, _)| *p == prop)
        {
            slot.1 = value;
            return;
        }
        if let Some(slot) = self.spill.iter_mut().find(|(p, _)| *p == prop) {
            slot.1 = value;
            return;
        }
        if self.inline_len < INLINE {
            self.inline[self.inline_len] = (prop, value);
            self.inline_len += 1;
        } else {
            self.spill.push((prop, value));
        }
    }

    pub fn get(&self, prop: Prop) -> Option<AnimValue> {
        self.iter().find(|(p, _)| *p == prop).map(|(_, v)| v)
    }

    pub fn number(&self, prop: Prop) -> Option<f32> {
        self.get(prop).map(AnimValue::number)
    }

    /// The number, or `fallback` when the property isn't animating.
    pub fn number_or(&self, prop: Prop, fallback: f32) -> f32 {
        self.number(prop).unwrap_or(fallback)
    }

    pub fn color(&self, prop: Prop) -> Option<Hsla> {
        self.get(prop).and_then(AnimValue::color)
    }

    pub fn is_empty(&self) -> bool {
        self.inline_len == 0
    }

    pub fn len(&self) -> usize {
        self.inline_len + self.spill.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (Prop, AnimValue)> + '_ {
        self.inline[..self.inline_len]
            .iter()
            .chain(self.spill.iter())
            .copied()
    }

    /// Empty it for reuse, keeping any spill capacity it earned.
    pub fn clear(&mut self) {
        self.inline_len = 0;
        self.spill.clear();
        self.progress = 0.0;
        self.finished = false;
    }

    /// Write every styled property onto an element.
    ///
    /// [`Prop::X`]/[`Prop::Y`] become a relative inset: gpui elements are
    /// `Position::Relative` by default and taffy treats an inset on those as
    /// a paint-time correction, so the element slides without pushing its
    /// siblings around. [`Prop::Rotate`], [`Prop::Scale`] and
    /// [`Prop::Custom`] are skipped — read those back yourself.
    pub fn apply<E: Styled>(&self, mut el: E) -> E {
        for (prop, value) in self.iter() {
            // A NaN reaching taffy corrupts a layout silently and forever;
            // treating it as zero is a visible, recoverable wrong answer. It
            // can only get here from a caller tweening to one — the timing
            // setters sanitize their own input.
            let n = match value.number() {
                n if n.is_finite() => n,
                _ => 0.0,
            };
            el = match prop {
                Prop::Opacity => el.opacity(n.clamp(0.0, 1.0)),
                Prop::X => el.left(px(n)),
                Prop::Y => el.top(px(n)),
                Prop::Width => el.w(px(n.max(0.0))),
                Prop::Height => el.h(px(n.max(0.0))),
                Prop::MarginTop => el.mt(px(n)),
                Prop::MarginRight => el.mr(px(n)),
                Prop::MarginBottom => el.mb(px(n)),
                Prop::MarginLeft => el.ml(px(n)),
                Prop::PadTop => el.pt(px(n.max(0.0))),
                Prop::PadRight => el.pr(px(n.max(0.0))),
                Prop::PadBottom => el.pb(px(n.max(0.0))),
                Prop::PadLeft => el.pl(px(n.max(0.0))),
                Prop::Radius => el.rounded(px(n.max(0.0))),
                Prop::BorderWidth => el.border(px(n.max(0.0))),
                Prop::Gap => el.gap(px(n.max(0.0))),
                Prop::FontSize => el.text_size(px(n.max(0.0))),
                Prop::Background => match value.color() {
                    Some(color) => el.bg(color),
                    None => el,
                },
                Prop::BorderColor => match value.color() {
                    Some(color) => el.border_color(color),
                    None => el,
                },
                Prop::TextColor => match value.color() {
                    Some(color) => el.text_color(color),
                    None => el,
                },
                Prop::Rotate | Prop::Scale | Prop::Custom(_) => el,
            };
        }
        el
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_the_same_prop_twice_replaces_it() {
        let mut frame = Frame::new();
        frame.set(Prop::Opacity, 0.2);
        frame.set(Prop::Opacity, 0.9);
        assert_eq!(frame.len(), 1);
        assert_eq!(frame.number(Prop::Opacity), Some(0.9));
    }

    #[test]
    fn missing_props_fall_back() {
        let frame = Frame::new();
        assert_eq!(frame.number(Prop::X), None);
        assert_eq!(frame.number_or(Prop::X, 4.0), 4.0);
    }

    /// The claim `INLINE` is chosen for: nothing you would normally reach
    /// for should reach the heap once per frame.
    #[test]
    fn the_stock_motions_fit_inline() {
        use crate::anim::Motion;
        use crate::TransitionKind;

        for kind in [
            TransitionKind::Fade,
            TransitionKind::SlideUp,
            TransitionKind::SlideDown,
            TransitionKind::SlideLeft,
            TransitionKind::SlideRight,
        ] {
            for motion in [Motion::enter(kind), Motion::exit(kind)] {
                let frame = motion.sample(motion.iteration_ms() / 2.0);
                assert!(frame.len() <= INLINE, "{kind:?} spilled: {frame:?}");
                assert!(frame.spill.is_empty());
            }
        }
        assert!(Motion::pulse().sample(100.0).spill.is_empty());
    }

    #[test]
    fn a_frame_spills_past_the_inline_slots_and_still_reads_back() {
        let mut frame = Frame::new();
        let props = [
            Prop::Opacity,
            Prop::X,
            Prop::Y,
            Prop::Radius,
            Prop::Gap,
            Prop::FontSize,
        ];
        for (i, prop) in props.iter().enumerate() {
            frame.set(*prop, i as f32);
        }
        assert_eq!(frame.len(), props.len());
        for (i, prop) in props.iter().enumerate() {
            assert_eq!(frame.number(*prop), Some(i as f32), "{prop:?}");
        }
        // Order survives the spill boundary.
        let seen: Vec<Prop> = frame.iter().map(|(p, _)| p).collect();
        assert_eq!(seen, props);

        // Replacing works on either side of it.
        frame.set(Prop::Opacity, 9.0);
        frame.set(Prop::FontSize, 9.0);
        assert_eq!(frame.len(), props.len());
        assert_eq!(frame.number(Prop::Opacity), Some(9.0));
        assert_eq!(frame.number(Prop::FontSize), Some(9.0));
    }

    #[test]
    fn frames_compare_by_content_not_by_storage() {
        let mut small = Frame::new();
        let mut spilled = Frame::new();
        for i in 0..5 {
            let prop = [Prop::Opacity, Prop::X, Prop::Y, Prop::Radius, Prop::Gap][i];
            small.set(prop, i as f32);
            spilled.set(prop, i as f32);
        }
        assert_eq!(small, spilled);
        spilled.set(Prop::Gap, 99.0);
        assert_ne!(small, spilled);
    }

    #[test]
    fn a_non_finite_value_never_reaches_the_layout() {
        let mut frame = Frame::new();
        frame.set(Prop::X, f32::NAN);
        frame.set(Prop::Width, f32::INFINITY);
        frame.set(Prop::Opacity, f32::NEG_INFINITY);
        // `apply` is where it matters, but the values are readable as-is —
        // a host reading `Prop::Custom` gets exactly what was tweened.
        assert!(frame.number(Prop::X).unwrap().is_nan());

        let mut styled = frame.apply(gpui::div());
        let style = styled.style();
        assert_eq!(style.inset.left, Some(gpui::px(0.0).into()));
        assert_eq!(style.size.width, Some(gpui::px(0.0).into()));
        assert_eq!(style.opacity, Some(0.0));
    }

    #[test]
    fn clearing_lets_a_frame_be_reused() {
        let mut frame = Frame::new();
        frame.set(Prop::Opacity, 1.0);
        frame.progress = 0.5;
        frame.finished = true;
        frame.clear();
        assert!(frame.is_empty());
        assert_eq!(frame.len(), 0);
        assert_eq!(frame.progress, 0.0);
        assert!(!frame.finished);
        assert_eq!(frame.number(Prop::Opacity), None);
    }

    #[test]
    fn insertion_order_is_preserved() {
        let mut frame = Frame::new();
        frame.set(Prop::Y, 3.0);
        frame.set(Prop::X, 1.0);
        frame.set(Prop::Y, 5.0);
        let props: Vec<Prop> = frame.iter().map(|(p, _)| p).collect();
        assert_eq!(props, vec![Prop::Y, Prop::X]);
    }
}
