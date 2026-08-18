//! The Logs panel: the message transcript and its filters.
//!
//! Safari's console is two things stacked — a filtered transcript and a
//! JavaScript evaluator. Only the first half transfers. A compiled binary has
//! no interpreter to hand a string to, so a prompt here would be a text field
//! that cannot evaluate anything; the panel is named for what it actually is.
//!
//! What it keeps is the part that earns its place: levels, coalesced repeats,
//! expandable detail rows, a source link per line, and a filter that searches
//! all three.

use std::collections::HashSet;

use gpui::prelude::*;
use gpui::{div, px, AnyElement, Context, Entity, Hsla, SharedString, Window};

use super::shell::{filter_bar, filter_pill, glyph, Ink, LABEL_SIZE, MONO_SIZE, ROW_HEIGHT};
use super::state::{format_timestamp, DevToolsState, LogLevel, LogRecord};
use super::DevTools;
use crate::icon::IconName;
use crate::input::{TextInput, TextInputEvent};
use crate::style::MONO_FAMILY;
use crate::theme::Size;

/// The level filter, matching Safari's segmented control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LevelFilter {
    #[default]
    All,
    Errors,
    Warnings,
    Logs,
}

impl LevelFilter {
    fn label(self) -> &'static str {
        match self {
            LevelFilter::All => "All",
            LevelFilter::Errors => "Errors",
            LevelFilter::Warnings => "Warnings",
            LevelFilter::Logs => "Logs",
        }
    }

    /// Whether a record survives this filter.
    pub fn admits(self, level: LogLevel) -> bool {
        match self {
            LevelFilter::All => true,
            LevelFilter::Errors => level == LogLevel::Error,
            LevelFilter::Warnings => matches!(level, LogLevel::Warning | LogLevel::Error),
            LevelFilter::Logs => {
                matches!(level, LogLevel::Log | LogLevel::Info | LogLevel::Debug)
            }
        }
    }

    const ALL: [LevelFilter; 4] = [
        LevelFilter::All,
        LevelFilter::Errors,
        LevelFilter::Warnings,
        LevelFilter::Logs,
    ];
}

/// Whether a record passes the level filter and the search text together.
pub fn matches(record: &LogRecord, level: LevelFilter, query: &str) -> bool {
    if !level.admits(record.level) {
        return false;
    }
    if query.is_empty() {
        return true;
    }
    let needle = query.to_lowercase();
    record.message.to_lowercase().contains(&needle)
        || record.details.iter().any(|(key, value)| {
            key.to_lowercase().contains(&needle) || value.to_lowercase().contains(&needle)
        })
        || record
            .source
            .as_ref()
            .is_some_and(|source| source.file.to_lowercase().contains(&needle))
}

pub struct LogsPanel {
    search: Entity<TextInput>,
    level: LevelFilter,
    query: SharedString,
    timestamps: bool,
    expanded: HashSet<u64>,
}

impl LogsPanel {
    pub fn new(cx: &mut Context<DevTools>) -> Self {
        let search = cx.new(|cx| TextInput::new(cx).placeholder("Filter").size(Size::Xs));

        cx.subscribe(&search, |this: &mut DevTools, _search, event, cx| {
            if let TextInputEvent::Change(query) = event {
                this.logs.query = SharedString::from(query.clone());
                cx.notify();
            }
        })
        .detach();

        LogsPanel {
            search,
            level: LevelFilter::default(),
            query: SharedString::default(),
            timestamps: false,
            expanded: HashSet::new(),
        }
    }

    fn level_color(level: LogLevel, ink: &Ink) -> Hsla {
        match level {
            LogLevel::Error => ink.danger,
            LogLevel::Warning => ink.warning,
            LogLevel::Info => ink.info,
            LogLevel::Debug => ink.dim,
            LogLevel::Log => ink.text,
        }
    }

    fn level_icon(level: LogLevel) -> Option<IconName> {
        match level {
            LogLevel::Error => Some(IconName::CircleX),
            LogLevel::Warning => Some(IconName::TriangleAlert),
            LogLevel::Info => Some(IconName::Info),
            LogLevel::Log | LogLevel::Debug => None,
        }
    }

    pub fn render(&self, window: &mut Window, cx: &mut Context<DevTools>) -> AnyElement {
        let ink = Ink::read(cx);
        let records: Vec<LogRecord> = cx
            .try_global::<DevToolsState>()
            .map(|state| {
                state
                    .logs()
                    .iter()
                    .filter(|record| matches(record, self.level, self.query.as_ref()))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();

        let mut bar = filter_bar(&ink);
        for level in LevelFilter::ALL {
            bar = bar.child(
                filter_pill(
                    ("devtools-logs-level", level as usize),
                    level.label(),
                    self.level == level,
                    &ink,
                )
                .on_click(cx.listener(
                    move |this: &mut DevTools, _event, _window, cx| {
                        this.logs.level = level;
                        cx.notify();
                    },
                )),
            );
        }
        bar = bar
            .child(
                filter_pill("devtools-logs-timestamps", "Times", self.timestamps, &ink).on_click(
                    cx.listener(|this: &mut DevTools, _event, _window, cx| {
                        this.logs.timestamps = !this.logs.timestamps;
                        cx.notify();
                    }),
                ),
            )
            .child(div().flex_1().child(self.search.clone()))
            .child(
                filter_pill("devtools-logs-clear", "Clear", false, &ink).on_click(cx.listener(
                    |_this: &mut DevTools, _event, _window, cx| {
                        if cx.has_global::<DevToolsState>() {
                            cx.update_global::<DevToolsState, _>(|state, _cx| state.clear_logs());
                        }
                        cx.notify();
                    },
                )),
            );

        let mut list = div()
            .id("devtools-logs-list")
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .overflow_scroll()
            .bg(ink.content);

        if records.is_empty() {
            list = list.child(
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .w_full()
                    .text_size(px(13.0))
                    .text_color(ink.dim)
                    .child(SharedString::new_static("No messages")),
            );
        }

        for record in &records {
            list = list.child(self.row(record, &ink, cx));
        }

        let _ = window;

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .child(bar)
            .child(list)
            .into_any_element()
    }

    fn row(&self, record: &LogRecord, ink: &Ink, cx: &mut Context<DevTools>) -> AnyElement {
        let color = Self::level_color(record.level, ink);
        let expanded = self.expanded.contains(&record.id);
        let id = record.id;
        let hover_bg = ink.hover;
        let has_details = !record.details.is_empty();

        // Errors and warnings get a tinted band, as Safari tints its rows.
        let band = match record.level {
            LogLevel::Error => Some(ink.danger.opacity(0.10)),
            LogLevel::Warning => Some(ink.warning.opacity(0.10)),
            _ => None,
        };

        let mut head = div()
            .id(("devtools-logs-row", id as usize))
            .flex()
            .items_start()
            .gap(px(5.0))
            .w_full()
            .px(px(8.0))
            .py(px(2.0))
            .min_h(px(ROW_HEIGHT))
            .border_b_1()
            .border_color(ink.border.opacity(0.4))
            .font_family(MONO_FAMILY)
            .text_size(px(MONO_SIZE))
            .when_some(band, |el, band| el.bg(band))
            .hover(move |st| st.bg(hover_bg));

        // A row with detail rows is expandable, and Safari marks that with a
        // triangle rather than leaving you to discover it by clicking.
        head = head.child(
            div()
                .flex()
                .flex_none()
                .w(px(10.0))
                .h(px(ROW_HEIGHT))
                .items_center()
                .children(has_details.then(|| {
                    glyph(
                        if expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        },
                        10.0,
                        ink.dim,
                        cx,
                    )
                })),
        );

        head = head.child(
            div()
                .flex()
                .flex_none()
                .w(px(13.0))
                .h(px(ROW_HEIGHT))
                .items_center()
                .children(Self::level_icon(record.level).map(|icon| glyph(icon, 11.0, color, cx))),
        );

        if self.timestamps {
            head = head.child(
                div()
                    .flex_none()
                    .text_color(ink.dim)
                    .child(SharedString::from(format_timestamp(record.at))),
            );
        }

        if record.count > 1 {
            head = head.child(
                div()
                    .flex_none()
                    .px(px(5.0))
                    .rounded(px(8.0))
                    .bg(ink.dim.opacity(0.25))
                    .text_size(px(9.0))
                    .text_color(ink.text)
                    .child(SharedString::from(record.count.to_string())),
            );
        }

        head = head
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_color(color)
                    .child(record.message.clone()),
            )
            .when_some(record.source.as_ref(), |el, source| {
                let target = source.clone();
                el.child(
                    div()
                        .id(("devtools-logs-source", id as usize))
                        .flex_none()
                        .text_size(px(LABEL_SIZE))
                        .text_color(ink.dim)
                        .hover(|st| st.text_color(ink.accent))
                        .child(SharedString::from(source.short()))
                        .on_click(
                            cx.listener(move |this: &mut DevTools, _event, _window, cx| {
                                this.reveal_source(target.clone(), cx);
                            }),
                        ),
                )
            })
            .when(has_details, |el| {
                el.on_click(
                    cx.listener(move |this: &mut DevTools, _event, _window, cx| {
                        if !this.logs.expanded.remove(&id) {
                            this.logs.expanded.insert(id);
                        }
                        cx.notify();
                    }),
                )
            });

        if !expanded || !has_details {
            return head.into_any_element();
        }

        let mut block = div().flex().flex_col().w_full();
        block = block.child(head);
        for (key, value) in &record.details {
            block = block.child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(6.0))
                    .w_full()
                    .pl(px(28.0))
                    .pr(px(8.0))
                    .py(px(1.0))
                    .bg(ink.stripe)
                    .font_family(MONO_FAMILY)
                    .text_size(px(MONO_SIZE))
                    .child(
                        div()
                            .flex_none()
                            .w(px(110.0))
                            .text_color(ink.attr)
                            .child(key.clone()),
                    )
                    .child(div().flex_1().text_color(ink.value).child(value.clone())),
            );
        }
        block.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(level: LogLevel, message: &str) -> LogRecord {
        LogRecord::new(level, message.to_string())
    }

    #[test]
    fn the_all_filter_admits_every_level() {
        for level in [
            LogLevel::Log,
            LogLevel::Debug,
            LogLevel::Info,
            LogLevel::Warning,
            LogLevel::Error,
        ] {
            assert!(LevelFilter::All.admits(level));
        }
    }

    #[test]
    fn warnings_include_errors_but_not_the_reverse() {
        assert!(LevelFilter::Warnings.admits(LogLevel::Error));
        assert!(LevelFilter::Warnings.admits(LogLevel::Warning));
        assert!(!LevelFilter::Warnings.admits(LogLevel::Log));
        assert!(!LevelFilter::Errors.admits(LogLevel::Warning));
    }

    #[test]
    fn logs_exclude_the_issue_levels() {
        assert!(LevelFilter::Logs.admits(LogLevel::Log));
        assert!(LevelFilter::Logs.admits(LogLevel::Info));
        assert!(!LevelFilter::Logs.admits(LogLevel::Warning));
        assert!(!LevelFilter::Logs.admits(LogLevel::Error));
    }

    #[test]
    fn search_is_case_insensitive_over_the_message() {
        let record = record(LogLevel::Log, "Cache MISS for user 42");
        assert!(matches(&record, LevelFilter::All, "cache"));
        assert!(matches(&record, LevelFilter::All, "MISS"));
        assert!(!matches(&record, LevelFilter::All, "hit"));
    }

    #[test]
    fn search_also_covers_detail_rows_and_the_source() {
        let record = record(LogLevel::Log, "request")
            .detail("status", "503")
            .source(super::super::state::SourceRef::new("net/pool.rs", 8, 1));

        assert!(matches(&record, LevelFilter::All, "503"));
        assert!(matches(&record, LevelFilter::All, "status"));
        assert!(matches(&record, LevelFilter::All, "pool.rs"));
        assert!(!matches(&record, LevelFilter::All, "absent"));
    }

    #[test]
    fn the_level_filter_and_the_search_both_have_to_pass() {
        let record = record(LogLevel::Log, "cache miss");
        assert!(!matches(&record, LevelFilter::Errors, "cache"));
        assert!(!matches(&record, LevelFilter::All, "nope"));
        assert!(matches(&record, LevelFilter::Logs, "cache"));
    }

    #[test]
    fn an_empty_query_filters_nothing_out() {
        let record = record(LogLevel::Error, "boom");
        assert!(matches(&record, LevelFilter::All, ""));
        assert!(matches(&record, LevelFilter::Errors, ""));
    }
}
