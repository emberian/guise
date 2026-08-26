//! `AIMessage` — one turn in a conversation.
//!
//! A message is a role, a markdown body, and whatever the model attached to
//! it: reasoning, tool calls, sources, an error. Rather than model all of that
//! as fields, the body is what this draws and everything else arrives as
//! children — so a host that has some other thing to show under a reply
//! (a diff, an image, a rating widget) just adds it.
//!
//! The shape follows what people expect from a chat: the user's turn is a
//! contained bubble, the assistant's runs full width, and system and tool
//! turns are quiet marginalia.
//!
//! ```ignore
//! AIMessage::new(AIRole::Assistant, reply)
//!     .streaming(true)
//!     .child(AIToolCall::new("search").status(AIToolStatus::Running))
//! ```

use gpui::prelude::*;
use gpui::{div, px, AnyElement, App, IntoElement, SharedString, Window};

use super::AIStreamingText;
use crate::devtools::Probed;
use crate::icon::{Icon, IconName};
use crate::markdown::Markdown;
use crate::theme::{theme, ColorName, Size};

/// Who produced a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AIRole {
  /// The person typing.
  #[default]
  User,
  /// The model.
  Assistant,
  /// Instructions the conversation runs under.
  System,
  /// The result handed back from a tool.
  Tool,
}

impl AIRole {
  /// The name shown above the turn.
  pub fn label(self) -> &'static str {
    match self {
      AIRole::User => "You",
      AIRole::Assistant => "Assistant",
      AIRole::System => "System",
      AIRole::Tool => "Tool",
    }
  }

  /// The glyph in the turn's gutter.
  pub fn icon(self) -> IconName {
    match self {
      AIRole::User => IconName::User,
      AIRole::Assistant => IconName::Sparkles,
      AIRole::System => IconName::Info,
      AIRole::Tool => IconName::Wrench,
    }
  }
}

/// One turn, rendered.
#[derive(IntoElement)]
pub struct AIMessage {
  role: AIRole,
  body: SharedString,
  /// Draw the body with a trailing caret, for a reply still arriving.
  streaming: bool,
  /// Shown instead of the body when the turn failed.
  error: Option<SharedString>,
  name: Option<SharedString>,
  meta: Option<SharedString>,
  avatar: bool,
  size: Size,
  /// Attachments — tool cards, sources, actions — drawn under the body.
  children: Vec<AnyElement>,
}

impl AIMessage {
  pub fn new(role: AIRole, body: impl Into<SharedString>) -> Self {
    AIMessage {
      role,
      body: body.into(),
      streaming: false,
      error: None,
      name: None,
      meta: None,
      avatar: true,
      size: Size::Sm,
      children: Vec::new(),
    }
  }

  /// The reply is still arriving: draw a caret after the last character.
  pub fn streaming(mut self, streaming: bool) -> Self {
    self.streaming = streaming;
    self
  }

  /// Replace the body with a failure notice. Whatever text had already
  /// arrived stays above it.
  pub fn error(mut self, error: impl Into<SharedString>) -> Self {
    self.error = Some(error.into());
    self
  }

  /// Override the role's default name — a model id, a person's name.
  pub fn name(mut self, name: impl Into<SharedString>) -> Self {
    self.name = Some(name.into());
    self
  }

  /// Trailing detail on the header row: a timestamp, a token count.
  pub fn meta(mut self, meta: impl Into<SharedString>) -> Self {
    self.meta = Some(meta.into());
    self
  }

  /// Hide the gutter icon and pull the body out to the full width.
  pub fn avatar(mut self, avatar: bool) -> Self {
    self.avatar = avatar;
    self
  }

  pub fn size(mut self, size: Size) -> Self {
    self.size = size;
    self
  }
}

impl ParentElement for AIMessage {
  fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
    self.children.extend(elements);
  }
}

impl RenderOnce for AIMessage {
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let t = theme(cx);
    let font = t.font_size(self.size);
    let radius = t.radius(Size::Md);
    let dimmed = t.dimmed().hsla();
    let border = t.border().hsla();
    let danger = t.danger().hsla();
    let bubble = t.surface_hover().hsla();
    let quiet = self.role == AIRole::System || self.role == AIRole::Tool;

    let header = div()
      .flex()
      .items_center()
      .gap(px(6.0))
      .text_size(px(t.font_size(Size::Xs)))
      .text_color(dimmed)
      // The role's label is already `&'static str`, so the default
      // costs nothing and the override is a refcount bump.
      .child(
        self
          .name
          .unwrap_or_else(|| SharedString::new_static(self.role.label())),
      )
      .when_some(self.meta, |row, meta| {
        row
          .child(div().child(SharedString::new_static("\u{00b7}")))
          .child(meta)
      });

    let body = div()
      .w_full()
      .when(!self.body.is_empty(), |column| {
        column.child(if self.streaming {
          AIStreamingText::new(self.body.clone())
            .size(self.size)
            .into_any_element()
        } else {
          Markdown::new(self.body.clone())
            .size(self.size)
            .into_any_element()
        })
      })
      .when_some(self.error, |column, error| {
        column.child(
          div()
            .mt(px(6.0))
            .flex()
            .items_start()
            .gap(px(6.0))
            .text_size(px(t.font_size(Size::Xs)))
            .text_color(danger)
            .child(Icon::new(IconName::TriangleAlert).size(Size::Xs))
            .child(error),
        )
      })
      .children(self.children);

    // The user's turn is a contained bubble; the assistant's is the page.
    // Anything else is marginalia, dimmed and small.
    let content = match self.role {
      AIRole::User => div()
        .max_w(px(560.0))
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(radius))
        .bg(bubble)
        .child(body),
      _ if quiet => div()
        .w_full()
        .px(px(10.0))
        .py(px(6.0))
        .rounded(px(radius))
        .border_1()
        .border_color(border)
        .text_color(dimmed)
        .child(body),
      _ => div().w_full().child(body),
    };

    let gutter = self.avatar.then(|| {
      div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .w(px(24.0))
        .h(px(24.0))
        .rounded(px(12.0))
        .bg(bubble)
        .text_color(dimmed)
        .child(
          Icon::new(self.role.icon())
            .size(Size::Xs)
            .color(ColorName::Gray),
        )
    });

    div()
      .flex()
      .flex_row()
      .items_start()
      .gap(px(10.0))
      .w_full()
      .text_size(px(font))
      .when(self.role == AIRole::User, |row| row.justify_end())
      .children(gutter.filter(|_| self.role != AIRole::User))
      .child(
        div()
          .flex()
          .flex_col()
          .gap(px(2.0))
          .flex_1()
          .min_w(px(0.0))
          .when(self.role == AIRole::User, |column| column.items_end())
          .child(header)
          .child(content),
      )
      .probe("AIMessage")
  }
}
