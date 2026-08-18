//! `AISources` — the list a reply's citations point at.
//!
//! Citations are only checkable if the reader can get from the number to the
//! thing. This is that list: numbered to match, each row clickable, each
//! carrying the excerpt the model actually used when the provider returned
//! one — because "it says so on this page" and "it says so in this sentence"
//! are different claims.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{div, px, App, IntoElement, SharedString, Window};

use super::AISource;
use crate::devtools::Probed;
use crate::icon::{Icon, IconName};
use crate::theme::{theme, ColorName, Size};

/// Called with the 0-based index of the source that was clicked.
type SourceHandler = Rc<dyn Fn(usize, &mut Window, &mut App) + 'static>;

/// A numbered list of sources.
#[derive(IntoElement)]
pub struct AISources {
    sources: Vec<AISource>,
    title: Option<SharedString>,
    size: Size,
    /// Show each source's excerpt under its title.
    excerpts: bool,
    on_open: Option<SourceHandler>,
}

impl AISources {
    pub fn new(sources: impl IntoIterator<Item = AISource>) -> Self {
        AISources {
            sources: sources.into_iter().collect(),
            title: None,
            size: Size::Xs,
            excerpts: false,
            on_open: None,
        }
    }

    /// A heading above the list. Defaults to none, since a reply that ends in
    /// a numbered list of links rarely needs one.
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    /// Show the passage each source contributed, where there is one.
    pub fn excerpts(mut self, excerpts: bool) -> Self {
        self.excerpts = excerpts;
        self
    }

    /// Handle a click on a row. Without one the list is not interactive —
    /// guise has no opinion on how a host opens a URL.
    pub fn on_open(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_open = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for AISources {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = theme(cx);
        let font = t.font_size(self.size);
        let dimmed = t.dimmed().hsla();
        let text_color = t.text().hsla();
        let border = t.border().hsla();
        let surface = t.surface_hover().alpha(0.5);
        let radius = t.radius(Size::Sm);

        let mut list = div().flex().flex_col().gap(px(4.0)).w_full();
        if let Some(title) = self.title {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(font))
                    .text_color(dimmed)
                    .child(
                        Icon::new(IconName::BookOpen)
                            .size(Size::Xs)
                            .color(ColorName::Gray),
                    )
                    .child(title),
            );
        }

        for (index, source) in self.sources.into_iter().enumerate() {
            let handler = self.on_open.clone();
            list = list.child(
                div()
                    .id(("guise-ai-source", index))
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap(px(8.0))
                    .w_full()
                    .px(px(8.0))
                    .py(px(6.0))
                    .rounded(px(radius))
                    .border_1()
                    .border_color(border)
                    .text_size(px(font))
                    .when(handler.is_some(), |row| {
                        row.cursor_pointer().hover(move |row| row.bg(surface))
                    })
                    .child(
                        div()
                            .flex_none()
                            .min_w(px(font * 1.4))
                            .text_color(dimmed)
                            .child(SharedString::from((index + 1).to_string())),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(1.0))
                            .flex_1()
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .text_color(text_color)
                                    .child(SharedString::from(source.title)),
                            )
                            .child(
                                div()
                                    .text_color(dimmed)
                                    .child(SharedString::from(source.location)),
                            )
                            .when_some(
                                source.excerpt.filter(|_| self.excerpts),
                                |column, excerpt| {
                                    column.child(
                                        div()
                                            .mt(px(4.0))
                                            .pl(px(8.0))
                                            .border_l_2()
                                            .border_color(border)
                                            .text_color(dimmed)
                                            .child(SharedString::from(excerpt)),
                                    )
                                },
                            ),
                    )
                    .when_some(handler, |row, handler| {
                        row.on_click(move |_event, window, cx| handler(index, window, cx))
                    }),
            );
        }
        list.probe("AISources")
    }
}
