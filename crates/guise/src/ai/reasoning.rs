//! `AIReasoning` — extended thinking, folded away.
//!
//! Reasoning output is usually longer than the answer it produced and is not
//! what the reader came for, so it collapses by default. It is not hidden
//! though: being able to open it is the point, and while it is still streaming
//! the header says so, because a collapsed block that is quietly filling up is
//! the one case where the user wants to know before they open it.
//!
//! The open state belongs to whatever owns the transcript — a turn scrolled
//! out of view should not forget it was expanded — so this is a controlled
//! component: pass `open` and an `on_toggle`.
//!
//! ```ignore
//! AIReasoning::new(("reasoning", turn_index), text)
//!     .open(turn.reasoning_open)
//!     .streaming(turn.streaming)
//!     .on_toggle(cx.listener(|this, _, _, cx| this.toggle_reasoning(cx)))
//! ```

use gpui::prelude::*;
use gpui::{div, px, App, ClickEvent, ElementId, IntoElement, SharedString, Window};

use crate::devtools::Probed;
use crate::icon::{Icon, IconName};
use crate::input::ClickHandler;
use crate::markdown::Markdown;
use crate::theme::{theme, ColorName, Size};

/// A collapsible block of the model's reasoning.
#[derive(IntoElement)]
pub struct AIReasoning {
    id: ElementId,
    text: SharedString,
    label: Option<SharedString>,
    open: bool,
    streaming: bool,
    size: Size,
    on_toggle: Option<ClickHandler>,
}

impl AIReasoning {
    pub fn new(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Self {
        AIReasoning {
            id: id.into(),
            text: text.into(),
            label: None,
            open: false,
            streaming: false,
            size: Size::Sm,
            on_toggle: None,
        }
    }

    /// Override the header text. The default names the state.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// Still arriving, so the header says so even while collapsed.
    pub fn streaming(mut self, streaming: bool) -> Self {
        self.streaming = streaming;
        self
    }

    pub fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    pub fn on_toggle(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_toggle = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for AIReasoning {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = theme(cx);
        let dimmed = t.dimmed().hsla();
        let border = t.border().hsla();
        let font_xs = t.font_size(Size::Xs);

        let label = self.label.unwrap_or_else(|| {
            if self.streaming {
                SharedString::new_static("Thinking\u{2026}")
            } else {
                SharedString::new_static("Reasoning")
            }
        });

        let header = div()
            .id(self.id)
            .flex()
            .items_center()
            .gap(px(6.0))
            .cursor_pointer()
            .text_size(px(font_xs))
            .text_color(dimmed)
            .child(
                Icon::new(if self.open {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                })
                .size(Size::Xs)
                .color(ColorName::Gray),
            )
            .child(
                Icon::new(IconName::Brain)
                    .size(Size::Xs)
                    .color(ColorName::Gray),
            )
            .child(label)
            .when_some(self.on_toggle, |header, handler| {
                header.on_click(move |event, window, cx| handler(event, window, cx))
            });

        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .w_full()
            .child(header)
            .when(self.open, |column| {
                column.child(
                    div()
                        .w_full()
                        .pl(px(10.0))
                        .border_l_2()
                        .border_color(border)
                        .text_color(dimmed)
                        .child(Markdown::new(self.text).size(self.size)),
                )
            })
            .probe("AIReasoning")
    }
}
