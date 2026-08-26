//! `SettingsSection` — a titled group of rows inside a settings page.
//!
//! A page is rarely one flat list. "Appearance" wants Theme separated from
//! Typography; "Security" wants the settings worth reading twice under their
//! own heading. This is that grouping: a title, an optional sentence, a rule,
//! and the rows.
//!
//! It is a plain [`ParentElement`], so the rows are whatever the caller puts
//! in — usually [`SettingsRow`](super::SettingsRow), but a section holding a
//! chart or a table is just as valid.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, App, FontWeight, IntoElement, SharedString, Window};

use crate::devtools::Probed;
use crate::theme::{theme, Size};

/// A titled group of settings.
#[derive(IntoElement)]
pub struct SettingsSection {
  title: SharedString,
  description: Option<SharedString>,
  rule: bool,
  children: Vec<AnyElement>,
}

impl SettingsSection {
  pub fn new(title: impl Into<SharedString>) -> Self {
    SettingsSection {
      title: title.into(),
      description: None,
      rule: true,
      children: Vec::new(),
    }
  }

  /// A sentence under the heading.
  pub fn description(mut self, description: impl Into<SharedString>) -> Self {
    self.description = Some(description.into());
    self
  }

  /// Draw the rule under the heading (default `true`).
  pub fn rule(mut self, rule: bool) -> Self {
    self.rule = rule;
    self
  }
}

impl ParentElement for SettingsSection {
  fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
    self.children.extend(elements);
  }
}

impl RenderOnce for SettingsSection {
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let t = theme(cx);
    let text = t.text().hsla();
    let dimmed = t.dimmed().hsla();
    let border = t.border().hsla();
    let font_md = t.font_size(Size::Md);
    let font_sm = t.font_size(Size::Sm);
    let gap = t.spacing(Size::Xs);

    let mut header = div().flex().flex_col().w_full().gap(px(gap)).child(
      div()
        .text_size(px(font_md))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(text)
        .child(self.title.clone()),
    );
    if let Some(description) = self.description {
      header = header.child(
        div()
          .text_size(px(font_sm))
          .text_color(dimmed)
          .child(description),
      );
    }
    if self.rule {
      header = header.child(div().w_full().h(px(1.0)).bg(border));
    }

    div()
      .flex()
      .flex_col()
      .w_full()
      .pt(px(18.0))
      .gap(px(gap))
      .child(header)
      .children(self.children)
      .probe("SettingsSection")
      .attr("title", self.title)
  }
}
