//! `Loader` — an animated busy indicator (pulsing dots or bars).

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{
  canvas, point, pulsating_between, px, quad, size, transparent_black, App, BorderStyle, Bounds,
  IntoElement, Pixels, Window,
};

use crate::devtools::Probed;
use crate::frameclock::{request_frame, FrameKind};
use crate::style::ColorValue;
use crate::theme::{theme, ColorName, Size};

const FRAME_INTERVAL: Duration = Duration::from_millis(60);
const CYCLE_SECONDS: f32 = 0.9;

fn animation_start() -> Instant {
  static START: OnceLock<Instant> = OnceLock::new();
  *START.get_or_init(Instant::now)
}

fn request_next_frame(window: &mut Window, cx: &mut App) {
  request_frame(FrameKind::Continuous, FRAME_INTERVAL, window, cx);
}

/// The loader's visual style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoaderVariant {
  /// Three pulsing dots (the default).
  Dots,
  /// Three pulsing vertical bars.
  Bars,
}

/// An animated loading indicator.
#[derive(IntoElement)]
pub struct Loader {
  variant: LoaderVariant,
  size: Size,
  color: ColorValue,
}

impl Loader {
  pub fn new() -> Self {
    Loader {
      variant: LoaderVariant::Dots,
      size: Size::Md,
      color: ColorValue::Named(ColorName::Blue),
    }
  }

  pub fn variant(mut self, variant: LoaderVariant) -> Self {
    self.variant = variant;
    self
  }

  pub fn size(mut self, size: Size) -> Self {
    self.size = size;
    self
  }

  pub fn color(mut self, color: impl Into<ColorValue>) -> Self {
    self.color = color.into();
    self
  }

  fn unit(&self) -> f32 {
    match self.size {
      Size::Xs => 6.0,
      Size::Sm => 8.0,
      Size::Md => 10.0,
      Size::Lg => 13.0,
      Size::Xl => 16.0,
    }
  }
}

impl Default for Loader {
  fn default() -> Self {
    Loader::new()
  }
}

impl RenderOnce for Loader {
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let t = theme(cx);
    let color = crate::style::solid(t, self.color);
    let unit = self.unit();
    let bars = self.variant == LoaderVariant::Bars;
    let width = if bars { unit * 0.6 } else { unit };
    let height = if bars { unit * 2.4 } else { unit };
    let gap = unit * 0.6;
    let total_width = width * 3.0 + gap * 2.0;
    let radius = if bars { unit * 0.3 } else { unit };

    canvas(
      |_, _, _| (),
      move |bounds: Bounds<Pixels>, _, window, cx| {
        if !bounds.intersects(&window.content_mask().bounds) {
          return;
        }
        let cycle = animation_start().elapsed().as_secs_f32() / CYCLE_SECONDS;
        let pulse = pulsating_between(0.25, 1.0);
        for index in 0..3 {
          let delta = (cycle + index as f32 / 3.0) % 1.0;
          let item = Bounds {
            origin: point(
              bounds.origin.x + px(index as f32 * (width + gap)),
              bounds.origin.y,
            ),
            size: size(px(width), px(height)),
          };
          window.paint_quad(quad(
            item,
            px(radius),
            color.opacity(pulse(delta)),
            px(0.0),
            transparent_black(),
            BorderStyle::default(),
          ));
        }
        request_next_frame(window, cx);
      },
    )
    .w(px(total_width))
    .h(px(height))
    .probe("Loader")
  }
}
