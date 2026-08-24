//! The values a motion can carry between two states.
//!
//! Everything an animation moves is either a number or a colour, so that is
//! the whole vocabulary: one `Copy` enum, one `lerp`. Keeping it closed (no
//! boxed `dyn Animatable`) is what lets a sampled [`Frame`](super::Frame) be
//! built and thrown away every frame without allocating per value.

use gpui::{Hsla, Pixels, Rgba};

/// A number or a colour, mid-interpolation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnimValue {
    Number(f32),
    Color(Hsla),
}

impl AnimValue {
    /// Interpolate toward `other`. Mixed kinds can't be blended, so the
    /// destination wins outright — a mismatch is a programming error, not a
    /// reason to panic mid-frame.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        match (self, other) {
            (AnimValue::Number(a), AnimValue::Number(b)) => AnimValue::Number(a + (b - a) * t),
            (AnimValue::Color(a), AnimValue::Color(b)) => AnimValue::Color(lerp_hsla(a, b, t)),
            (_, b) => b,
        }
    }

    /// The number, or `0.0` for a colour.
    pub fn number(self) -> f32 {
        match self {
            AnimValue::Number(v) => v,
            AnimValue::Color(_) => 0.0,
        }
    }

    pub fn color(self) -> Option<Hsla> {
        match self {
            AnimValue::Color(c) => Some(c),
            AnimValue::Number(_) => None,
        }
    }

    pub fn is_color(self) -> bool {
        matches!(self, AnimValue::Color(_))
    }
}

impl From<f32> for AnimValue {
    fn from(v: f32) -> Self {
        AnimValue::Number(v)
    }
}

impl From<f64> for AnimValue {
    fn from(v: f64) -> Self {
        AnimValue::Number(v as f32)
    }
}

impl From<i32> for AnimValue {
    fn from(v: i32) -> Self {
        AnimValue::Number(v as f32)
    }
}

impl From<Pixels> for AnimValue {
    fn from(v: Pixels) -> Self {
        AnimValue::Number(f32::from(v))
    }
}

impl From<Hsla> for AnimValue {
    fn from(v: Hsla) -> Self {
        AnimValue::Color(v)
    }
}

impl From<Rgba> for AnimValue {
    fn from(v: Rgba) -> Self {
        AnimValue::Color(v.into())
    }
}

/// Blend two colours the short way around the hue wheel.
///
/// Component-wise lerp on HSL sends red → cyan the long way through green;
/// taking the shorter arc is what makes a hover colour change look like a
/// crossfade instead of a rainbow sweep. Fully transparent endpoints carry
/// no meaningful hue, so they borrow the other end's.
fn lerp_hsla(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let (ah, bh) = match (a.a <= 0.0, b.a <= 0.0) {
        (true, false) => (b.h, b.h),
        (false, true) => (a.h, a.h),
        _ => (a.h, b.h),
    };
    let mut delta = bh - ah;
    if delta > 0.5 {
        delta -= 1.0;
    } else if delta < -0.5 {
        delta += 1.0;
    }
    let h = (ah + delta * t).rem_euclid(1.0);
    Hsla {
        h,
        s: a.s + (b.s - a.s) * t,
        l: a.l + (b.l - a.l) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hsla(h: f32, s: f32, l: f32, a: f32) -> Hsla {
        Hsla { h, s, l, a }
    }

    #[test]
    fn numbers_interpolate_linearly() {
        let v = AnimValue::from(10.0).lerp(AnimValue::from(20.0), 0.25);
        assert_eq!(v.number(), 12.5);
    }

    #[test]
    fn hue_takes_the_short_way_round() {
        // 0.9 -> 0.1 is 0.2 forward through the wrap, not 0.8 backward.
        let a = hsla(0.9, 1.0, 0.5, 1.0);
        let b = hsla(0.1, 1.0, 0.5, 1.0);
        let mid = AnimValue::from(a).lerp(AnimValue::from(b), 0.5).color();
        assert!((mid.unwrap().h - 0.0).abs() < 1e-5, "{mid:?}");
    }

    #[test]
    fn transparent_endpoints_borrow_the_other_hue() {
        let clear = hsla(0.0, 0.0, 0.0, 0.0);
        let blue = hsla(0.6, 1.0, 0.5, 1.0);
        let mid = AnimValue::from(clear)
            .lerp(AnimValue::from(blue), 0.5)
            .color()
            .unwrap();
        assert!((mid.h - 0.6).abs() < 1e-5, "{mid:?}");
        assert!((mid.a - 0.5).abs() < 1e-5);
    }

    #[test]
    fn mismatched_kinds_snap_to_the_destination() {
        let v = AnimValue::from(1.0).lerp(AnimValue::from(hsla(0.5, 1.0, 0.5, 1.0)), 0.5);
        assert!(v.is_color());
    }
}
