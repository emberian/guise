//! `SettingsRow` — one setting: what it is on the left, its control on the
//! right.
//!
//! This is [`crate::input::Field`]'s horizontal sibling. `Field` stacks a label
//! and description *above* an input, which is the shape a form wants; a
//! settings list wants them beside it, so the eye runs down one column of names
//! and one column of controls. Same information, different axis, and keeping
//! them as separate components is what stops either from growing a `horizontal`
//! flag that doubles its layout code.
//!
//! The row is stateless. It renders the value the caller passes and reports
//! clicks; the caller owns the setting, writes it, and decides what "modified"
//! means — usually "the user's config file pins this key", not "differs from
//! the default", because those two can agree and only the first is actionable.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, App, ElementId, IntoElement, SharedString, Window};

use crate::devtools::Probed;
use crate::icon::IconName;
use crate::input::ClickHandler;
use crate::theme::{theme, Size};
use crate::ActionIcon;

/// One row in a settings list.
#[derive(IntoElement)]
pub struct SettingsRow {
    id: ElementId,
    label: SharedString,
    description: Option<SharedString>,
    modified: bool,
    on_reset: Option<ClickHandler>,
    control: Option<AnyElement>,
    divider: bool,
}

impl SettingsRow {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        SettingsRow {
            id: id.into(),
            label: label.into(),
            description: None,
            modified: false,
            on_reset: None,
            control: None,
            divider: true,
        }
    }

    /// One short sentence under the label, for what the label cannot say.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Mark the setting as set by the user rather than left at its default.
    pub fn modified(mut self, modified: bool) -> Self {
        self.modified = modified;
        self
    }

    /// Offer to put the setting back to its default. Shown only while
    /// [`modified`](Self::modified) — a reset button on an untouched setting is
    /// a button that does nothing.
    pub fn on_reset(
        mut self,
        handler: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_reset = Some(Box::new(handler));
        self
    }

    /// The control on the right — a `Switch`, a `Select`, a `TextInput`, a row
    /// of `Button`s. Any element; the row only positions it.
    pub fn control(mut self, control: impl IntoElement) -> Self {
        self.control = Some(control.into_any_element());
        self
    }

    /// Draw the hairline under the row (default `true`). Turn it off for the
    /// last row in a section, where the section's own edge already separates.
    pub fn divider(mut self, divider: bool) -> Self {
        self.divider = divider;
        self
    }
}

impl RenderOnce for SettingsRow {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = theme(cx);
        let text = t.text().hsla();
        let dimmed = t.dimmed().hsla();
        let border = t.border().hsla();
        let accent = t.primary().hsla();
        let font_sm = t.font_size(Size::Sm);
        let font_xs = t.font_size(Size::Xs);

        let mut name = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(font_sm))
            .text_color(text)
            .child(self.label.clone());

        // Exactly one marker, never two: a reset control when the caller offers
        // one, a dot when it does not. Both would say the same thing twice.
        if self.modified {
            name = match self.on_reset {
                Some(handler) => name.child(
                    ActionIcon::new(self.id.clone(), IconName::RotateCcw)
                        .label("Reset")
                        .size(Size::Xs)
                        .variant(crate::style::Variant::Subtle)
                        .on_click(handler),
                ),
                None => name.child(
                    div()
                        .flex_none()
                        .w(px(5.0))
                        .h(px(5.0))
                        .rounded_full()
                        .bg(accent),
                ),
            };
        }

        let mut left = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .gap(px(2.0))
            .child(name);
        if let Some(description) = self.description {
            left = left.child(
                div()
                    .text_size(px(font_xs))
                    .text_color(dimmed)
                    .child(description),
            );
        }

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .w_full()
            .py(px(12.0))
            .when(self.divider, |el| el.border_b_1().border_color(border))
            .child(left)
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_none()
                    .children(self.control),
            )
            .probe("SettingsRow")
            .attr_if("modified", self.modified)
    }
}
