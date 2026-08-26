//! What a motion is allowed to move.
//!
//! gpui has no transform matrix on a `div`, so there is no `translate` or
//! `scale` to animate — motion is expressed through the properties that do
//! exist: opacity, the relative inset (which shifts an element at paint time
//! without disturbing its siblings, the closest thing to a translate), the
//! box, and colours. Naming them in a `Copy` enum instead of strings is what
//! makes [`Frame::apply`](super::Frame::apply) exhaustive and a typo a
//! compile error.

/// One animatable property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Prop {
  Opacity,
  /// Horizontal offset in px, applied as a relative inset — the element
  /// moves, the layout does not.
  X,
  /// Vertical offset in px, same mechanism as [`Prop::X`].
  Y,
  Width,
  Height,
  MarginTop,
  MarginRight,
  MarginBottom,
  MarginLeft,
  PadTop,
  PadRight,
  PadBottom,
  PadLeft,
  Radius,
  BorderWidth,
  Gap,
  FontSize,
  Background,
  BorderColor,
  TextColor,
  /// Turns in degrees. gpui can only rotate an `Image`/`Svg` (through its
  /// own `Transformation`), so this is carried for you to read out of the
  /// [`Frame`](super::Frame) — `apply` skips it.
  Rotate,
  /// A multiplier. Same story as [`Prop::Rotate`]: yours to apply.
  Scale,
  /// Anything else you want tweened. The value never touches a style — you
  /// read it back out of the frame and do what you like with it.
  Custom(&'static str),
}

impl Prop {
  /// Whether the property expects an [`AnimValue::Color`](super::AnimValue).
  pub fn is_color(self) -> bool {
    matches!(self, Prop::Background | Prop::BorderColor | Prop::TextColor)
  }
}
