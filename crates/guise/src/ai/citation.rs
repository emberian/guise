//! `AICitation` — a numbered reference chip.
//!
//! An assistant that cites its sources is only useful if the citation is
//! reachable, so this is a click target, not a decoration. It stays small
//! enough to sit inline at the end of a sentence and carries the number the
//! reader can match against the [`AISources`](super::AISources) list below.

use gpui::prelude::*;
use gpui::{div, px, App, ClickEvent, ElementId, IntoElement, SharedString, Window};

use crate::devtools::Probed;
use crate::input::ClickHandler;
use crate::theme::{theme, Size};

/// One source, as cited.
#[derive(Debug, Clone, Default)]
pub struct AISource {
  /// What to show: a page title, a filename.
  pub title: String,
  /// Where it came from — a URL, a path. Shown as the subtitle.
  pub location: String,
  /// The passage the model drew on, if the provider returned one.
  pub excerpt: Option<String>,
}

impl AISource {
  pub fn new(title: impl Into<String>, location: impl Into<String>) -> Self {
    AISource {
      title: title.into(),
      location: location.into(),
      excerpt: None,
    }
  }

  pub fn excerpt(mut self, excerpt: impl Into<String>) -> Self {
    self.excerpt = Some(excerpt.into());
    self
  }
}

/// A small numbered chip pointing at a source.
#[derive(IntoElement)]
pub struct AICitation {
  id: ElementId,
  index: usize,
  label: Option<SharedString>,
  size: Size,
  on_click: Option<ClickHandler>,
}

impl AICitation {
  /// `index` is the 1-based number shown to the reader.
  pub fn new(id: impl Into<ElementId>, index: usize) -> Self {
    AICitation {
      id: id.into(),
      index,
      label: None,
      size: Size::Xs,
      on_click: None,
    }
  }

  /// Show a name beside the number — a domain, a filename.
  pub fn label(mut self, label: impl Into<SharedString>) -> Self {
    self.label = Some(label.into());
    self
  }

  pub fn size(mut self, size: Size) -> Self {
    self.size = size;
    self
  }

  pub fn on_click(
    mut self,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
  ) -> Self {
    self.on_click = Some(Box::new(handler));
    self
  }
}

impl RenderOnce for AICitation {
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let t = theme(cx);
    let font = t.font_size(self.size);
    let dimmed = t.dimmed().hsla();
    let text_color = t.text().hsla();
    let surface = t.surface_hover().hsla();
    let primary = t.primary().hsla();
    let clickable = self.on_click.is_some();

    div()
      .id(self.id)
      .flex()
      .items_center()
      .gap(px(4.0))
      .h(px(font * 1.7))
      .px(px(6.0))
      .rounded(px(font))
      .bg(surface)
      .text_size(px(font))
      .text_color(dimmed)
      .when(clickable, |chip| {
        chip
          .cursor_pointer()
          .hover(move |chip| chip.text_color(primary))
      })
      .child(
        div()
          .text_color(text_color)
          .child(SharedString::from(self.index.to_string())),
      )
      .children(self.label)
      .when_some(self.on_click, |chip, handler| {
        chip.on_click(move |event, window, cx| handler(event, window, cx))
      })
      .probe("AICitation")
  }
}
