//! `Skeleton` — an animated loading placeholder.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{
  canvas, pulsating_between, px, quad, transparent_black, App, BorderStyle, Bounds, IntoElement,
  Pixels, Window,
};

use crate::devtools::Probed;
use crate::frameclock::{request_frame, FrameKind};
use crate::theme::{theme, ColorName, Size};

const FRAME_INTERVAL: Duration = Duration::from_millis(60);
const CYCLE_SECONDS: f32 = 1.1;

fn animation_start() -> Instant {
  static START: OnceLock<Instant> = OnceLock::new();
  *START.get_or_init(Instant::now)
}

fn request_next_frame(window: &mut Window, cx: &mut App) {
  request_frame(FrameKind::Continuous, FRAME_INTERVAL, window, cx);
}

/// A pulsing placeholder block.
#[derive(IntoElement)]
pub struct Skeleton {
  width: Option<f32>,
  height: f32,
  radius: Size,
  circle: bool,
}

impl Skeleton {
  pub fn new() -> Self {
    Skeleton {
      width: None,
      height: 16.0,
      radius: Size::Sm,
      circle: false,
    }
  }

  pub fn width(mut self, width: f32) -> Self {
    self.width = Some(width);
    self
  }

  pub fn height(mut self, height: f32) -> Self {
    self.height = height;
    self
  }

  pub fn radius(mut self, radius: Size) -> Self {
    self.radius = radius;
    self
  }

  /// Render a circle of `size` (overrides width/height/radius).
  pub fn circle(mut self, size: f32) -> Self {
    self.circle = true;
    self.width = Some(size);
    self.height = size;
    self
  }
}

impl Default for Skeleton {
  fn default() -> Self {
    Skeleton::new()
  }
}

impl RenderOnce for Skeleton {
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let t = theme(cx);
    let color = t
      .color(ColorName::Gray, if t.scheme.is_dark() { 7 } else { 2 })
      .hsla();
    let radius = if self.circle {
      self.height
    } else {
      t.radius(self.radius)
    };

    let mut block = canvas(
      |_, _, _| (),
      move |bounds: Bounds<Pixels>, _, window, cx| {
        if !bounds.intersects(&window.content_mask().bounds) {
          return;
        }
        let cycle = (animation_start().elapsed().as_secs_f32() / CYCLE_SECONDS) % 1.0;
        let pulse = pulsating_between(0.4, 1.0);
        window.paint_quad(quad(
          bounds,
          px(radius),
          color.opacity(pulse(cycle)),
          px(0.0),
          transparent_black(),
          BorderStyle::default(),
        ));
        request_next_frame(window, cx);
      },
    )
    .h(px(self.height));
    block = match self.width {
      Some(width) => block.w(px(width)),
      None => block.w_full(),
    };
    block.probe("Skeleton")
  }
}
