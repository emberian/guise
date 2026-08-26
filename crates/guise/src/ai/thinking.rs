//! `AIThinking` — the gap between sending and the first token.
//!
//! That gap can run to several seconds, and an unchanged screen during it
//! reads as a hang. Three dots cycling is the smallest thing that says the
//! request is alive without claiming to know how long it will take, which a
//! progress bar would.

use gpui::prelude::*;
use gpui::{div, px, App, IntoElement, SharedString, Window};

use crate::devtools::Probed;
use crate::feedback::{Loader, LoaderVariant};
use crate::theme::{theme, ColorName, Size};

/// A "still working" indicator with an optional label.
#[derive(IntoElement)]
pub struct AIThinking {
  label: Option<SharedString>,
  size: Size,
  color: Option<ColorName>,
}

impl AIThinking {
  pub fn new() -> Self {
    AIThinking {
      label: None,
      size: Size::Sm,
      color: None,
    }
  }

  /// Say what is happening — "Thinking", "Searching the web", "Running
  /// tests". A specific label is worth far more than a generic one.
  pub fn label(mut self, label: impl Into<SharedString>) -> Self {
    self.label = Some(label.into());
    self
  }

  pub fn size(mut self, size: Size) -> Self {
    self.size = size;
    self
  }

  pub fn color(mut self, color: ColorName) -> Self {
    self.color = Some(color);
    self
  }
}

impl Default for AIThinking {
  fn default() -> Self {
    AIThinking::new()
  }
}

impl RenderOnce for AIThinking {
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let t = theme(cx);
    let font = t.font_size(self.size);
    let dot_color = self
      .color
      .map_or_else(|| t.dimmed(), |name| t.color(name, 6))
      .hsla();
    let dimmed = t.dimmed().hsla();
    div()
      .flex()
      .items_center()
      .gap(px(8.0))
      .text_size(px(font))
      .text_color(dimmed)
      .child(
        Loader::new()
          .variant(LoaderVariant::Dots)
          .size(self.size)
          .color(dot_color),
      )
      .children(self.label)
      .probe("AIThinking")
  }
}
