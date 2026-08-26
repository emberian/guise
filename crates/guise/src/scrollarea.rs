//! `ScrollArea` — a bounded, scrollable container.
//!
//! Desktop UIs scroll; most builders assume their content fits. Wrap an
//! overflowing column (or row) in a `ScrollArea` and give it a bound. There are
//! two, and which one is right is a layout question, not a preference:
//! `max_height` for a list that occupies a fixed slice of a larger layout, and
//! `fill` for a pane that should be as tall as whatever the window gives it.
//! Each instance needs a unique id so gpui can track its scroll offset.

use crate::devtools::Probed;
use gpui::prelude::*;
use gpui::{div, px, AnyElement, App, ElementId, IntoElement, SharedString, Window};

/// A scrollable region. `ScrollArea::new("id").max_height(240.0)`, or
/// `ScrollArea::new("id").fill()` to take the space the parent has left.
#[derive(IntoElement)]
pub struct ScrollArea {
  id: ElementId,
  children: Vec<AnyElement>,
  max_height: Option<f32>,
  fill: bool,
  horizontal: bool,
}

impl ScrollArea {
  pub fn new(id: impl Into<ElementId>) -> Self {
    ScrollArea {
      id: id.into(),
      children: Vec::new(),
      max_height: None,
      fill: false,
      horizontal: false,
    }
  }

  /// Clip to this height (px) and scroll past it.
  pub fn max_height(mut self, height: f32) -> Self {
    self.max_height = Some(height);
    self
  }

  /// Take the space the parent has left over, and scroll past it — the mode
  /// for a full-height pane, where any fixed number is wrong at every window
  /// size but one.
  ///
  /// The parent still has to be bounded itself; filling an unbounded parent
  /// sizes to the content and there is nothing to scroll.
  pub fn fill(mut self) -> Self {
    self.fill = true;
    self
  }

  /// Scroll horizontally instead of vertically.
  pub fn horizontal(mut self, horizontal: bool) -> Self {
    self.horizontal = horizontal;
    self
  }
}

impl ParentElement for ScrollArea {
  fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
    self.children.extend(elements);
  }
}

impl RenderOnce for ScrollArea {
  fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
    let bound: SharedString = match (self.fill, self.max_height) {
      (true, _) => "fill".into(),
      (false, Some(height)) => format!("{height}px").into(),
      (false, None) => "none".into(),
    };
    let mut el = div().id(self.id).flex();
    el = if self.horizontal {
      let el = el.flex_row().overflow_x_scroll();
      if self.fill {
        // Three settings for three parents: `flex_1` claims the leftover
        // main axis under a flex parent, the relative size does the same
        // under a plain block one (where grow means nothing, and where a
        // flex basis would win anyway if both applied), and the zero
        // minimum is what lets the box shrink under its content instead
        // of pushing the parent open.
        el.flex_1().w_full().min_w_0()
      } else {
        el
      }
    } else {
      let el = el.flex_col().overflow_y_scroll();
      if self.fill {
        el.flex_1().h_full().min_h_0()
      } else {
        el
      }
    };
    // A cap still applies while filling: grow into the window, but never
    // past this.
    if let Some(height) = self.max_height {
      el = el.max_h(px(height));
    }
    el.children(self.children)
      .probe("ScrollArea")
      .attr("axis", if self.horizontal { "x" } else { "y" })
      .attr("bound", bound)
  }
}
