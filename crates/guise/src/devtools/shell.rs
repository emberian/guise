//! The inspector's chrome: the resolved palette and the small pieces of
//! furniture every panel is built from.
//!
//! Safari's Web Inspector has a look of its own — denser than the app around
//! it, monospaced wherever it shows data, striped tables, hairline dividers.
//! None of that may be hardcoded here, so this module resolves it all from
//! [`Theme`] once per frame into an [`Ink`], and the panels read *that*. Swap
//! the theme and the inspector follows, which is the whole point of the rule.

use gpui::prelude::*;
use gpui::{div, px, App, Div, ElementId, Hsla, SharedString, Stateful};

use crate::icon::{ensure_font, IconName, FONT_FAMILY};
use crate::style::MONO_FAMILY;
use crate::theme::{theme, ColorName, Theme};

/// The inspector's row height. Safari's tables and trees are 20-22px; this is
/// what makes it read as a tool rather than as app UI.
pub(crate) const ROW_HEIGHT: f32 = 21.0;
/// Toolbar and tab-bar height.
pub(crate) const BAR_HEIGHT: f32 = 29.0;
/// The data font size used throughout.
pub(crate) const MONO_SIZE: f32 = 11.0;
/// The label font size used in chrome.
pub(crate) const LABEL_SIZE: f32 = 11.0;
/// Width of the detail sidebars (Styles, request details).
pub(crate) const SIDEBAR_WIDTH: f32 = 292.0;
/// Width of the navigation sidebars (Storage domains, Sources files).
pub(crate) const NAV_WIDTH: f32 = 200.0;
/// The floor for a table's one flexible column. Without it the fixed columns
/// win the whole row and the name — the column you actually read — collapses to
/// a sliver.
pub(crate) const FLEX_COLUMN_MIN: f32 = 96.0;

/// Every color the inspector paints, resolved from the theme.
///
/// Derived rather than declared: the chrome sits one step off the app's
/// surface, the markup colors reuse the same palette hues the editor's
/// highlighter does, so a custom theme restyles the inspector for free.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ink {
    /// Toolbar, tab bar, and status bar background.
    pub chrome: Hsla,
    /// The selected tab and pressed-button fill.
    pub chrome_active: Hsla,
    /// Panel background.
    pub content: Hsla,
    /// Every other row, for table striping.
    pub stripe: Hsla,
    /// Hovered row.
    pub hover: Hsla,
    /// Selected row, focused.
    pub selected: Hsla,
    /// Text on a selected row.
    pub selected_text: Hsla,
    pub border: Hsla,
    pub text: Hsla,
    pub dim: Hsla,
    pub accent: Hsla,
    pub success: Hsla,
    pub warning: Hsla,
    pub danger: Hsla,
    pub info: Hsla,
    /// `<Tag>` in the element tree.
    pub tag: Hsla,
    /// `attr=` in the element tree.
    pub attr: Hsla,
    /// `"value"` in the element tree, and string literals.
    pub value: Hsla,
    /// Angle brackets, slashes, commas.
    pub punct: Hsla,
    /// CSS-style property names in the Styles panel.
    pub property: Hsla,
    /// The four box-model bands, outermost first.
    pub box_margin: Hsla,
    pub box_border: Hsla,
    pub box_padding: Hsla,
    pub box_content: Hsla,
}

impl Ink {
    pub fn new(t: &Theme) -> Self {
        let dark = t.scheme.is_dark();
        // Chrome sits one step away from the app surface in whichever
        // direction the scheme leaves room.
        let chrome = if dark {
            t.color(ColorName::Dark, 7).hsla()
        } else {
            t.color(ColorName::Gray, 1).hsla()
        };
        let content = if dark {
            t.color(ColorName::Dark, 8).hsla()
        } else {
            t.white.hsla()
        };
        let shade = if dark { 4 } else { 7 };

        Ink {
            chrome,
            chrome_active: if dark {
                t.color(ColorName::Dark, 5).hsla()
            } else {
                t.white.hsla()
            },
            content,
            stripe: if dark {
                t.color(ColorName::Dark, 7).hsla()
            } else {
                t.color(ColorName::Gray, 0).hsla()
            },
            hover: t.surface_hover().hsla(),
            selected: t.primary().hsla(),
            selected_text: t.white.hsla(),
            border: t.border().hsla(),
            text: t.text().hsla(),
            dim: t.dimmed().hsla(),
            accent: t.primary().hsla(),
            success: t.success().hsla(),
            warning: t.warning().hsla(),
            danger: t.danger().hsla(),
            info: t.info().hsla(),
            tag: t.color(ColorName::Violet, shade).hsla(),
            attr: t.color(ColorName::Orange, shade).hsla(),
            value: t.color(ColorName::Blue, shade).hsla(),
            punct: t.dimmed().hsla(),
            property: t.color(ColorName::Teal, shade).hsla(),
            box_margin: t.color(ColorName::Orange, if dark { 8 } else { 2 }).hsla(),
            box_border: t.color(ColorName::Yellow, if dark { 8 } else { 2 }).hsla(),
            box_padding: t.color(ColorName::Green, if dark { 8 } else { 2 }).hsla(),
            box_content: t.color(ColorName::Blue, if dark { 8 } else { 2 }).hsla(),
        }
    }

    /// Read the palette for the current frame.
    pub fn read(cx: &App) -> Self {
        Ink::new(theme(cx))
    }
}

/// A Lucide glyph tinted with an arbitrary color. [`crate::Icon`] only takes a
/// palette name, and the inspector paints icons in resolved `Ink` colors.
pub(crate) fn glyph(name: IconName, size: f32, color: Hsla, cx: &App) -> Div {
    ensure_font(cx);
    div()
        .flex()
        .items_center()
        .justify_center()
        .flex_none()
        .w(px(size))
        .h(px(size))
        .font_family(FONT_FAMILY)
        .text_size(px(size))
        .line_height(px(size))
        .text_color(color)
        .child(SharedString::new_static(name.glyph()))
}

/// A horizontal hairline.
pub(crate) fn hairline(ink: &Ink) -> Div {
    div().h(px(1.0)).w_full().flex_none().bg(ink.border)
}

/// A vertical hairline.
pub(crate) fn hairline_v(ink: &Ink) -> Div {
    div().w(px(1.0)).h_full().flex_none().bg(ink.border)
}

/// A square icon button, as the toolbar and every panel's controls use.
/// `active` paints the pressed state Safari gives its toggles.
pub(crate) fn tool_button(
    id: impl Into<ElementId>,
    icon: IconName,
    tooltip: &'static str,
    active: bool,
    ink: &Ink,
    cx: &App,
) -> Stateful<Div> {
    let fg = if active { ink.accent } else { ink.dim };
    let hover_bg = ink.hover;
    div()
        .id(id.into())
        .flex()
        .items_center()
        .justify_center()
        .flex_none()
        .w(px(24.0))
        .h(px(21.0))
        .rounded(px(4.0))
        .when(active, |el| el.bg(ink.chrome_active))
        .hover(move |st| st.bg(hover_bg))
        .tooltip(crate::overlay::tooltip(tooltip))
        .child(glyph(icon, 13.0, fg, cx))
}

/// A rounded label toggle: the Network type filters and the Console level
/// filters are rows of these.
pub(crate) fn filter_pill(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    active: bool,
    ink: &Ink,
) -> Stateful<Div> {
    let hover_bg = ink.hover;
    div()
        .id(id.into())
        .flex()
        .flex_none()
        .items_center()
        .h(px(17.0))
        .px(px(7.0))
        .rounded(px(9.0))
        .text_size(px(LABEL_SIZE))
        .when(active, |el| {
            el.bg(ink.selected).text_color(ink.selected_text)
        })
        .when(!active, |el| {
            el.text_color(ink.dim).hover(move |st| st.bg(hover_bg))
        })
        .child(label.into())
}

/// The bar above a panel's content that holds its filters. Chrome-colored, one
/// row tall, hairline underneath.
pub(crate) fn filter_bar(ink: &Ink) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.0))
        .h(px(26.0))
        .px(px(8.0))
        .w_full()
        .bg(ink.chrome)
        .border_b_1()
        .border_color(ink.border)
}

/// A column header row for the data tables.
pub(crate) fn header_cell(label: impl Into<SharedString>, width: Option<f32>, ink: &Ink) -> Div {
    let mut el = div()
        .flex()
        .items_center()
        .h_full()
        .px(px(6.0))
        .text_size(px(LABEL_SIZE))
        .text_color(ink.dim)
        .overflow_hidden()
        .child(label.into());
    match width {
        Some(width) => el = el.w(px(width)).flex_none(),
        None => el = el.flex_1().min_w(px(FLEX_COLUMN_MIN)),
    }
    el
}

/// A data cell, monospaced and clipped rather than wrapped — a table row is
/// always exactly one line tall.
pub(crate) fn cell(text: impl Into<SharedString>, width: Option<f32>, color: Hsla) -> Div {
    let mut el = div()
        .flex()
        .items_center()
        .h_full()
        .px(px(6.0))
        .overflow_hidden()
        .whitespace_nowrap()
        .text_color(color)
        .child(text.into());
    match width {
        Some(width) => el = el.w(px(width)).flex_none(),
        None => el = el.flex_1().min_w(px(FLEX_COLUMN_MIN)),
    }
    el
}

/// A section heading inside a sidebar: uppercase, dim, with a rule under it.
pub(crate) fn section_header(label: impl Into<SharedString>, ink: &Ink) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .h(px(22.0))
        .px(px(8.0))
        .w_full()
        .bg(ink.chrome)
        .border_b_1()
        .border_color(ink.border)
        .text_size(px(LABEL_SIZE))
        .text_color(ink.dim)
        .child(label.into())
}

/// A key/value row, as the Headers, Timing, and Node panels all use.
pub(crate) fn kv_row(
    key: impl Into<SharedString>,
    value: impl Into<SharedString>,
    ink: &Ink,
) -> Div {
    div()
        .flex()
        .items_start()
        .gap(px(6.0))
        .px(px(8.0))
        .py(px(2.0))
        .w_full()
        .font_family(MONO_FAMILY)
        .text_size(px(MONO_SIZE))
        .child(
            div()
                .flex_none()
                .w(px(104.0))
                .text_color(ink.dim)
                .child(key.into()),
        )
        .child(div().flex_1().text_color(ink.text).child(value.into()))
}

/// What a panel shows when it has nothing to show. Safari centers a single
/// dimmed sentence, and so does this.
pub(crate) fn empty_state(message: impl Into<SharedString>, ink: &Ink) -> Div {
    div()
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .w_full()
        .bg(ink.content)
        .text_size(px(13.0))
        .text_color(ink.dim)
        .child(message.into())
}

/// A disclosure triangle. `None` means the row cannot be disclosed and the
/// space is held open so siblings still line up.
pub(crate) fn disclosure(expanded: Option<bool>, ink: &Ink, cx: &App) -> Div {
    match expanded {
        Some(expanded) => glyph(
            if expanded {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            },
            11.0,
            ink.dim,
            cx,
        ),
        None => div().flex_none().w(px(11.0)).h(px(11.0)),
    }
}

/// Shorten a string in the middle, keeping both ends legible — how Safari
/// truncates long URLs and selectors.
pub(crate) fn elide(text: &str, max: usize) -> String {
    if text.chars().count() <= max || max < 4 {
        return text.to_string();
    }
    let head = (max - 1) / 2;
    let tail = max - 1 - head;
    let chars: Vec<char> = text.chars().collect();
    let start: String = chars[..head].iter().collect();
    let end: String = chars[chars.len() - tail..].iter().collect();
    format!("{start}…{end}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elide_keeps_both_ends() {
        assert_eq!(elide("short", 10), "short");
        assert_eq!(elide("abcdefghij", 10), "abcdefghij");
        assert_eq!(elide("abcdefghijk", 7), "abc…ijk");
        assert_eq!(elide("abcdefghijk", 6), "ab…ijk".to_string());
    }

    #[test]
    fn elide_counts_characters_not_bytes() {
        // A byte-based split would panic or cut a code point in half here.
        assert_eq!(elide("ααααααααααα", 7).chars().count(), 7);
    }
}
