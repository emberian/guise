//! `SettingsView` — the settings window shape: a list of pages down the left,
//! the selected page's content on the right, an optional search field, and an
//! optional footer.
//!
//! The view owns navigation and nothing else. It does not know what a setting
//! is, cannot read one, and will not write one — the content closure is handed
//! the active page and the current query and returns whatever should be on
//! screen. That is deliberate: every app types its settings against its own
//! config struct, and a component generic enough to hold those would make the
//! caller's code worse than the twenty lines it replaced.
//!
//! Search works the same way. The view has nothing to search, so it reports the
//! query through [`SettingsViewEvent::Search`] and passes it to the content
//! closure; matching is the host's, because only the host knows what the
//! settings are.

use gpui::prelude::*;
use gpui::{
    div, px, AnyElement, App, Context, ElementId, Entity, EventEmitter, FocusHandle, Focusable,
    IntoElement, SharedString, Window,
};

use crate::devtools::Probed;
use crate::icon::IconName;
use crate::input::{TextInput, TextInputEvent};
use crate::theme::{theme, Size};
use crate::Icon;

/// What the view builds its page content with: the active page id, the current
/// search query, and the usual pair.
type ContentBuilder = Box<dyn Fn(&str, &str, &mut Window, &mut App) -> AnyElement + 'static>;
/// What the view builds its footer with.
type FooterBuilder = Box<dyn Fn(&mut Window, &mut App) -> AnyElement + 'static>;

/// One entry in the page list.
pub struct SettingsPage {
    pub id: SharedString,
    pub title: SharedString,
    pub icon: Option<IconName>,
}

/// Emitted as the user navigates.
#[derive(Debug, Clone)]
pub enum SettingsViewEvent {
    /// A different page was selected. Carries its id.
    PageChanged(SharedString),
    /// The search field changed. Carries the full query, empty when cleared.
    Search(SharedString),
}

/// The settings shell. Create with `cx.new(|cx| SettingsView::new(cx))`.
pub struct SettingsView {
    focus: FocusHandle,
    pages: Vec<SettingsPage>,
    active: usize,
    search: Entity<TextInput>,
    searchable: bool,
    query: SharedString,
    content: Option<ContentBuilder>,
    footer: Option<FooterBuilder>,
    sidebar_width: f32,
}

impl EventEmitter<SettingsViewEvent> for SettingsView {}

impl Focusable for SettingsView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl SettingsView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| {
            TextInput::new(cx)
                .placeholder("Search settings")
                .size(Size::Xs)
        });

        cx.subscribe(&search, |this: &mut SettingsView, _search, event, cx| {
            if let TextInputEvent::Change(query) = event {
                this.query = SharedString::from(query.clone());
                cx.emit(SettingsViewEvent::Search(this.query.clone()));
                cx.notify();
            }
        })
        .detach();

        SettingsView {
            focus: cx.focus_handle(),
            pages: Vec::new(),
            active: 0,
            search,
            searchable: false,
            query: SharedString::default(),
            content: None,
            footer: None,
            sidebar_width: 190.0,
        }
    }

    /// Add a page to the list.
    pub fn page(mut self, id: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        self.pages.push(SettingsPage {
            id: id.into(),
            title: title.into(),
            icon: None,
        });
        self
    }

    /// Add a page with an icon beside its title.
    pub fn page_icon(
        mut self,
        id: impl Into<SharedString>,
        title: impl Into<SharedString>,
        icon: IconName,
    ) -> Self {
        self.pages.push(SettingsPage {
            id: id.into(),
            title: title.into(),
            icon: Some(icon),
        });
        self
    }

    /// Show the search field above the page list.
    pub fn searchable(mut self, searchable: bool) -> Self {
        self.searchable = searchable;
        self
    }

    /// What to render for the active page. Re-invoked every frame — with the
    /// active page's id and the current query — so a page shows live values
    /// rather than a snapshot taken when the view was built.
    pub fn content<E: IntoElement>(
        mut self,
        content: impl Fn(&str, &str, &mut Window, &mut App) -> E + 'static,
    ) -> Self {
        self.content = Some(Box::new(move |page, query, window, cx| {
            content(page, query, window, cx).into_any_element()
        }));
        self
    }

    /// A row pinned under the content — where the file lives, a Done button.
    pub fn footer<E: IntoElement>(
        mut self,
        footer: impl Fn(&mut Window, &mut App) -> E + 'static,
    ) -> Self {
        self.footer = Some(Box::new(move |window, cx| {
            footer(window, cx).into_any_element()
        }));
        self
    }

    /// Width of the page list (default 190px).
    pub fn sidebar_width(mut self, width: f32) -> Self {
        self.sidebar_width = width;
        self
    }

    /// Open on a particular page. Unknown ids are ignored rather than
    /// panicking — a stale id from a restored session is not a crash.
    pub fn active(mut self, id: impl Into<SharedString>) -> Self {
        let id = id.into();
        if let Some(index) = self.pages.iter().position(|page| page.id == id) {
            self.active = index;
        }
        self
    }

    /// The active page's id, or `None` when no pages were added.
    pub fn active_page(&self) -> Option<&SharedString> {
        self.pages.get(self.active).map(|page| &page.id)
    }

    /// Select a page by id, emitting [`SettingsViewEvent::PageChanged`].
    pub fn set_page(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(index) = self.pages.iter().position(|page| page.id.as_ref() == id) {
            self.select(index, cx);
        }
    }

    fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.pages.len() || index == self.active {
            return;
        }
        self.active = index;
        cx.emit(SettingsViewEvent::PageChanged(self.pages[index].id.clone()));
        cx.notify();
    }

    /// The current search query.
    pub fn query(&self) -> &SharedString {
        &self.query
    }

    /// Clear the search field.
    pub fn clear_search(&mut self, cx: &mut Context<Self>) {
        self.query = SharedString::default();
        self.search.update(cx, |search, cx| search.set_text("", cx));
        cx.emit(SettingsViewEvent::Search(self.query.clone()));
        cx.notify();
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let surface = t.surface_hover().hsla();
        let border = t.border().hsla();
        let text = t.text().hsla();
        let dimmed = t.dimmed().hsla();
        let selected_bg = t.primary().hsla();
        let selected_fg = t.white.hsla();
        let hover = t.surface_hover().hsla();
        let font_sm = t.font_size(Size::Sm);
        let radius = t.radius(Size::Sm);
        let gap = t.spacing(Size::Xs);

        let mut bar = div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(self.sidebar_width))
            .h_full()
            .p(px(8.0))
            .gap(px(2.0))
            .bg(surface)
            .border_r_1()
            .border_color(border);

        if self.searchable {
            bar = bar.child(div().pb(px(gap)).child(self.search.clone()));
        }

        for (index, page) in self.pages.iter().enumerate() {
            let selected = index == self.active;
            let fg = if selected { selected_fg } else { dimmed };
            bar = bar.child(
                div()
                    .id(ElementId::NamedInteger(
                        SharedString::new_static("guise-settings-page"),
                        index as u64,
                    ))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .h(px(30.0))
                    .px(px(8.0))
                    .rounded(px(radius))
                    .text_size(px(font_sm))
                    .text_color(if selected { selected_fg } else { text })
                    .when(selected, |el| el.bg(selected_bg))
                    .when(!selected, |el| el.hover(move |st| st.bg(hover)))
                    .children(page.icon.map(|icon| Icon::new(icon).size(Size::Xs)))
                    .child(page.title.clone())
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.select(index, cx);
                    }))
                    .text_color(if selected { selected_fg } else { fg }),
            );
        }

        bar.child(div().flex_1())
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let body = t.body().hsla();
        let border = t.border().hsla();
        let text = t.text().hsla();
        let dimmed = t.dimmed().hsla();
        let font_sm = t.font_size(Size::Sm);

        let page = self
            .active_page()
            .cloned()
            .unwrap_or_else(|| SharedString::new_static(""));

        let content = match &self.content {
            Some(build) => build(page.as_ref(), self.query.as_ref(), window, cx),
            None => div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_size(px(font_sm))
                .text_color(dimmed)
                .child(SharedString::new_static("No content builder set"))
                .into_any_element(),
        };

        let footer = self.footer.as_ref().map(|build| {
            div()
                .flex()
                .flex_none()
                .items_center()
                .w_full()
                .px(px(24.0))
                .py(px(10.0))
                .border_t_1()
                .border_color(border)
                .child(build(window, cx))
        });

        div()
            .track_focus(&self.focus)
            .key_context("SettingsView")
            .flex()
            .size_full()
            .min_h(px(0.0))
            .bg(body)
            .text_color(text)
            .child(self.sidebar(cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.0))
                    .h_full()
                    .child(
                        div()
                            .id("guise-settings-content")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h(px(0.0))
                            .w_full()
                            .px(px(24.0))
                            .pb(px(24.0))
                            .overflow_y_scroll()
                            .child(content),
                    )
                    .children(footer),
            )
            .probe("SettingsView")
            .attr("page", page)
    }
}
