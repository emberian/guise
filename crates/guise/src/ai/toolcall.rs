//! `AIToolCall` — what the model did, and whether it worked.
//!
//! A tool call is the part of a reply the user did not ask to read but needs
//! to be able to check: which tool, with what arguments, and what came back.
//! So the card shows the name and status always, and folds the arguments and
//! the result away until asked.
//!
//! Status is the load-bearing part. A call that is running, one that returned,
//! and one that failed have to be distinguishable at a glance, because a
//! stalled tool is the most common way an assistant appears broken.
//!
//! ```ignore
//! AIToolCall::new(("tool", i), "read_file")
//!     .status(AIToolStatus::Ok)
//!     .arguments(r#"{"path": "src/main.rs"}"#)
//!     .result(preview)
//!     .open(expanded)
//!     .on_toggle(cx.listener(|this, _, _, cx| this.toggle(i, cx)))
//! ```

use gpui::prelude::*;
use gpui::{div, px, App, ClickEvent, ElementId, IntoElement, SharedString, Window};

use crate::devtools::Probed;
use crate::feedback::{Loader, LoaderVariant};
use crate::icon::{Icon, IconName};
use crate::input::ClickHandler;
use crate::style::MONO_FAMILY;
use crate::theme::{theme, Color, ColorName, Size};

/// Where a tool call has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AIToolStatus {
    /// Requested by the model, not started.
    #[default]
    Pending,
    /// In flight.
    Running,
    /// Returned successfully.
    Ok,
    /// Returned an error, or never returned.
    Error,
}

impl AIToolStatus {
    fn label(self) -> &'static str {
        match self {
            AIToolStatus::Pending => "queued",
            AIToolStatus::Running => "running",
            AIToolStatus::Ok => "done",
            AIToolStatus::Error => "failed",
        }
    }
}

/// A collapsible record of one tool invocation.
#[derive(IntoElement)]
pub struct AIToolCall {
    id: ElementId,
    name: SharedString,
    status: AIToolStatus,
    arguments: Option<SharedString>,
    result: Option<SharedString>,
    /// Trailing detail on the header: a duration, a byte count.
    meta: Option<SharedString>,
    open: bool,
    /// Whether the card has foldable content, stated rather than inferred from
    /// `arguments`/`result` being present — a host that only supplies those
    /// while the card is open still needs the chevron to open it with.
    expandable: Option<bool>,
    size: Size,
    on_toggle: Option<ClickHandler>,
}

impl AIToolCall {
    pub fn new(id: impl Into<ElementId>, name: impl Into<SharedString>) -> Self {
        AIToolCall {
            id: id.into(),
            name: name.into(),
            status: AIToolStatus::default(),
            arguments: None,
            result: None,
            meta: None,
            open: false,
            expandable: None,
            size: Size::Sm,
            on_toggle: None,
        }
    }

    pub fn status(mut self, status: AIToolStatus) -> Self {
        self.status = status;
        self
    }

    /// The call's input, shown verbatim. Pretty-print it before passing it in
    /// if it's JSON — this draws what it's given.
    pub fn arguments(mut self, arguments: impl Into<SharedString>) -> Self {
        self.arguments = Some(arguments.into());
        self
    }

    /// What the tool returned, or the error it raised.
    pub fn result(mut self, result: impl Into<SharedString>) -> Self {
        self.result = Some(result.into());
        self
    }

    pub fn meta(mut self, meta: impl Into<SharedString>) -> Self {
        self.meta = Some(meta.into());
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// Offer the fold affordance even with no content attached yet. Defaults to
    /// whether `arguments` or `result` was given.
    pub fn expandable(mut self, expandable: bool) -> Self {
        self.expandable = Some(expandable);
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

impl RenderOnce for AIToolCall {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = theme(cx);
        let dimmed = t.dimmed().hsla();
        let text_color = t.text().hsla();
        let border = t.border().hsla();
        let surface = t.surface_hover().alpha(0.6);
        let radius = t.radius(Size::Sm);
        let font_xs = t.font_size(Size::Xs);
        let mono_font = t.font_size(self.size) * 0.92;

        let accent: Color = match self.status {
            AIToolStatus::Pending => t.dimmed(),
            AIToolStatus::Running => t.info(),
            AIToolStatus::Ok => t.success(),
            AIToolStatus::Error => t.danger(),
        };
        let accent_hsla = accent.hsla();

        // The status glyph carries the state; the word next to it is for
        // anyone who can't tell the colors apart.
        let badge = match self.status {
            AIToolStatus::Running => Loader::new()
                .variant(LoaderVariant::Dots)
                .size(Size::Xs)
                .color(ColorName::Blue)
                .into_any_element(),
            AIToolStatus::Ok => Icon::new(IconName::Check)
                .size(Size::Xs)
                .color(ColorName::Green)
                .into_any_element(),
            AIToolStatus::Error => Icon::new(IconName::CircleX)
                .size(Size::Xs)
                .color(ColorName::Red)
                .into_any_element(),
            AIToolStatus::Pending => Icon::new(IconName::Clock)
                .size(Size::Xs)
                .color(ColorName::Gray)
                .into_any_element(),
        };

        let expandable = self
            .expandable
            .unwrap_or(self.arguments.is_some() || self.result.is_some());
        let has_toggle = self.on_toggle.is_some() && expandable;

        let header = div()
            .id(self.id)
            .flex()
            .items_center()
            .gap(px(8.0))
            .w_full()
            .text_size(px(font_xs))
            .when(has_toggle, |header| header.cursor_pointer())
            .child(badge)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .flex_1()
                    .min_w(px(0.0))
                    .text_color(text_color)
                    .child(
                        Icon::new(IconName::Wrench)
                            .size(Size::Xs)
                            .color(ColorName::Gray),
                    )
                    .child(self.name.clone()),
            )
            .when_some(self.meta.clone(), |header, meta| {
                header.child(div().text_color(dimmed).child(meta))
            })
            .child(
                div()
                    .text_color(accent_hsla)
                    .child(SharedString::new_static(self.status.label())),
            )
            .when(expandable, |header| {
                header.child(
                    Icon::new(if self.open {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .size(Size::Xs)
                    .color(ColorName::Gray),
                )
            })
            .when_some(self.on_toggle, |header, handler| {
                header.on_click(move |event, window, cx| handler(event, window, cx))
            });

        let block = |title: &'static str, body: SharedString, color| {
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(font_xs))
                        .text_color(dimmed)
                        .child(SharedString::new_static(title)),
                )
                .child(
                    div()
                        .w_full()
                        .px(px(8.0))
                        .py(px(6.0))
                        .rounded(px(radius))
                        .bg(surface)
                        .font_family(MONO_FAMILY)
                        .text_size(px(mono_font))
                        .text_color(color)
                        .child(body),
                )
        };

        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .w_full()
            .px(px(10.0))
            .py(px(8.0))
            .rounded(px(radius))
            .border_1()
            .border_color(if self.status == AIToolStatus::Error {
                accent_hsla
            } else {
                border
            })
            .child(header)
            .when(self.open, |card| {
                card.when_some(self.arguments, |card, arguments| {
                    card.child(block("Arguments", arguments, text_color))
                })
                .when_some(self.result, |card, result| {
                    card.child(block(
                        if self.status == AIToolStatus::Error {
                            "Error"
                        } else {
                            "Result"
                        },
                        result,
                        if self.status == AIToolStatus::Error {
                            accent_hsla
                        } else {
                            text_color
                        },
                    ))
                })
            })
            .probe("AIToolCall")
    }
}
