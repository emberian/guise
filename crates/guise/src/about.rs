//! `About` — the small centered card every desktop app owes its users: icon,
//! name, version, what kind of build this is, and a link home.
//!
//! Ported from sinclair, where the interesting part was never the layout. It
//! was [`BuildKind`]: a build made from some commit that merely carries the
//! version number is not the release, and saying "Released 2026-08-18" on one
//! is a small lie that costs a bug report. So the line says what the build
//! actually is, and the type makes it hard to say otherwise.
//!
//! The card is a `RenderOnce` builder, so it goes wherever you want it — its
//! own window, a modal, a settings page.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, App, FontWeight, IntoElement, SharedString, Window};

use crate::devtools::Probed;
use crate::theme::{theme, Size};

/// What kind of build this is, which decides how the dated line reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BuildKind {
    /// Built from the tag matching its version. Only this may call its date a
    /// release date.
    Released,
    /// Any other build, whatever version number it carries.
    #[default]
    Development,
}

impl BuildKind {
    /// The dated line for a build of this kind.
    ///
    /// Outside a git checkout there is often no date to qualify, so an unknown
    /// date reads as what the build is rather than as a date it hasn't got.
    pub fn line(self, date: &str) -> String {
        match (self, date.trim()) {
            (BuildKind::Released, "") | (BuildKind::Released, "unknown") => {
                "Released build".to_string()
            }
            (BuildKind::Released, date) => format!("Released {date}"),
            (BuildKind::Development, "") | (BuildKind::Development, "unknown") => {
                "Development build".to_string()
            }
            (BuildKind::Development, date) => format!("Development build · {date}"),
        }
    }
}

/// The About card.
#[derive(IntoElement)]
pub struct About {
    name: SharedString,
    version: Option<SharedString>,
    icon: Option<AnyElement>,
    build_date: Option<SharedString>,
    kind: BuildKind,
    tagline: Option<SharedString>,
    credits: Option<SharedString>,
    links: Vec<AnyElement>,
}

impl About {
    pub fn new(name: impl Into<SharedString>) -> Self {
        About {
            name: name.into(),
            version: None,
            icon: None,
            build_date: None,
            kind: BuildKind::default(),
            tagline: None,
            credits: None,
            links: Vec::new(),
        }
    }

    /// The version string, shown as "Version 1.0.0". Usually
    /// `env!("CARGO_PKG_VERSION")`.
    pub fn version(mut self, version: impl Into<SharedString>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// The app icon — an `img(..)`, an [`Icon`](crate::Icon), anything.
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }

    /// The build date, and whether this build is the release of its version.
    /// Both come from the build script; see [`BuildKind`].
    pub fn build(mut self, kind: BuildKind, date: impl Into<SharedString>) -> Self {
        self.kind = kind;
        self.build_date = Some(date.into());
        self
    }

    /// One line under the name.
    pub fn tagline(mut self, tagline: impl Into<SharedString>) -> Self {
        self.tagline = Some(tagline.into());
        self
    }

    /// The copyright or acknowledgement line at the foot.
    pub fn credits(mut self, credits: impl Into<SharedString>) -> Self {
        self.credits = Some(credits.into());
        self
    }

    /// A link, usually an [`Anchor`](crate::Anchor). Several may be added.
    pub fn link(mut self, link: impl IntoElement) -> Self {
        self.links.push(link.into_any_element());
        self
    }
}

impl RenderOnce for About {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = theme(cx);
        let body = t.body().hsla();
        let text = t.text().hsla();
        let dimmed = t.dimmed().hsla();
        let font_lg = t.font_size(Size::Lg);
        let font_sm = t.font_size(Size::Sm);
        let font_xs = t.font_size(Size::Xs);
        let gap = t.spacing(Size::Sm);

        let mut card = div()
            .flex()
            .flex_col()
            .items_center()
            .size_full()
            .px(px(28.0))
            .py(px(32.0))
            .gap(px(gap))
            .bg(body)
            .text_color(text)
            .children(self.icon)
            .child(
                div()
                    .text_size(px(font_lg))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.name.clone()),
            );

        if let Some(tagline) = self.tagline {
            card = card.child(
                div()
                    .text_size(px(font_sm))
                    .text_color(dimmed)
                    .child(tagline),
            );
        }
        if let Some(version) = self.version {
            card = card.child(
                div()
                    .text_size(px(font_sm))
                    .text_color(dimmed)
                    .child(SharedString::from(format!("Version {version}"))),
            );
        }
        if let Some(date) = self.build_date {
            card = card.child(
                div()
                    .text_size(px(font_xs))
                    .text_color(dimmed)
                    .child(SharedString::from(self.kind.line(date.as_ref()))),
            );
        }
        if !self.links.is_empty() {
            card = card.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(gap))
                    .pt(px(gap))
                    .children(self.links),
            );
        }

        card = card.child(div().flex_1());

        if let Some(credits) = self.credits {
            card = card.child(
                div()
                    .text_size(px(font_xs))
                    .text_color(dimmed)
                    .child(credits),
            );
        }

        card.probe("About").attr("name", self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_released_build_may_call_its_date_a_release_date() {
        assert_eq!(
            BuildKind::Released.line("2026-08-18"),
            "Released 2026-08-18"
        );
        assert_eq!(
            BuildKind::Development.line("2026-08-18"),
            "Development build · 2026-08-18"
        );
    }

    #[test]
    fn an_unknown_date_reads_as_the_build_kind_alone() {
        assert_eq!(BuildKind::Development.line("unknown"), "Development build");
        assert_eq!(BuildKind::Development.line(""), "Development build");
        assert_eq!(BuildKind::Released.line("unknown"), "Released build");
    }

    #[test]
    fn whitespace_is_not_a_date() {
        assert_eq!(BuildKind::Development.line("   "), "Development build");
    }

    #[test]
    fn a_build_is_a_development_build_until_proven_otherwise() {
        assert_eq!(BuildKind::default(), BuildKind::Development);
    }
}
