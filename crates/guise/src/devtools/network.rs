//! The Network panel: the request table, the waterfall, and the detail
//! sidebar.
//!
//! Safari's Network tab is a sortable table whose last column is a timing
//! graph, with a per-request sidebar of Headers / Cookies / Sizes / Timing /
//! Preview behind it. All of that is here; what feeds it is the host, through
//! [`super::network_begin`] and [`super::network_update`], because a component
//! library has no business owning an HTTP client.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, Context, Entity, Hsla, SharedString, Window};

use super::shell::{
    cell, elide, empty_state, filter_bar, filter_pill, header_cell, kv_row, section_header, Ink,
    LABEL_SIZE, MONO_SIZE, ROW_HEIGHT, SIDEBAR_WIDTH,
};
use super::state::{
    format_bytes, format_duration, DevToolsState, NetworkRecord, RequestState, ResourceKind,
};
use super::DevTools;
use crate::input::{TextInput, TextInputEvent};
use crate::style::MONO_FAMILY;
use crate::theme::Size;

/// The detail sidebar's tabs, in Safari's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RequestDetail {
    #[default]
    Headers,
    Cookies,
    Sizes,
    Timing,
    Preview,
}

impl RequestDetail {
    fn label(self) -> &'static str {
        match self {
            RequestDetail::Headers => "Headers",
            RequestDetail::Cookies => "Cookies",
            RequestDetail::Sizes => "Sizes",
            RequestDetail::Timing => "Timing",
            RequestDetail::Preview => "Preview",
        }
    }

    const ALL: [RequestDetail; 5] = [
        RequestDetail::Headers,
        RequestDetail::Cookies,
        RequestDetail::Sizes,
        RequestDetail::Timing,
        RequestDetail::Preview,
    ];
}

/// Whether a request survives the type filter and the search box.
pub fn matches(record: &NetworkRecord, kind: Option<ResourceKind>, query: &str) -> bool {
    if let Some(kind) = kind {
        if record.kind != kind {
            return false;
        }
    }
    if query.is_empty() {
        return true;
    }
    let needle = query.to_lowercase();
    record.url.to_lowercase().contains(&needle)
        || record.method.to_lowercase().contains(&needle)
        || record
            .status
            .is_some_and(|status| status.to_string().contains(&needle))
}

/// Split a `Cookie:` or `Set-Cookie:` header into its name/value pairs.
pub fn parse_cookies(header: &str) -> Vec<(String, String)> {
    header
        .split(';')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .filter_map(|part| {
            let (name, value) = part.split_once('=')?;
            Some((name.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

pub struct NetworkPanel {
    search: Entity<TextInput>,
    kind: Option<ResourceKind>,
    query: SharedString,
    selected: Option<u64>,
    detail: RequestDetail,
}

impl NetworkPanel {
    pub fn new(cx: &mut Context<DevTools>) -> Self {
        let search = cx.new(|cx| TextInput::new(cx).placeholder("Filter").size(Size::Xs));

        cx.subscribe(&search, |this: &mut DevTools, _search, event, cx| {
            if let TextInputEvent::Change(query) = event {
                this.network.query = SharedString::from(query.clone());
                cx.notify();
            }
        })
        .detach();

        NetworkPanel {
            search,
            kind: None,
            query: SharedString::default(),
            selected: None,
            detail: RequestDetail::default(),
        }
    }

    fn status_color(record: &NetworkRecord, ink: &Ink) -> Hsla {
        match record.state {
            RequestState::Failed => ink.danger,
            RequestState::Canceled => ink.warning,
            RequestState::Pending => ink.dim,
            RequestState::Finished => match record.status {
                Some(status) if status >= 500 => ink.danger,
                Some(status) if status >= 400 => ink.danger,
                Some(status) if status >= 300 => ink.warning,
                _ => ink.success,
            },
        }
    }

    fn kind_color(kind: ResourceKind, ink: &Ink) -> Hsla {
        match kind {
            ResourceKind::Document => ink.info,
            ResourceKind::Stylesheet => ink.accent,
            ResourceKind::Image => ink.success,
            ResourceKind::Font => ink.warning,
            ResourceKind::Script => ink.attr,
            ResourceKind::Fetch => ink.tag,
            ResourceKind::WebSocket => ink.property,
            ResourceKind::Media => ink.value,
            ResourceKind::Other => ink.dim,
        }
    }

    pub fn render(&self, window: &mut Window, cx: &mut Context<DevTools>) -> AnyElement {
        let ink = Ink::read(cx);
        let (records, span) = cx
            .try_global::<DevToolsState>()
            .map(|state| {
                let records: Vec<NetworkRecord> = state
                    .network()
                    .iter()
                    .filter(|record| matches(record, self.kind, self.query.as_ref()))
                    .cloned()
                    .collect();
                (records, state.network_span())
            })
            .unwrap_or_default();

        let mut bar = filter_bar(&ink).child(
            filter_pill("devtools-network-all", "All", self.kind.is_none(), &ink).on_click(
                cx.listener(|this: &mut DevTools, _event, _window, cx| {
                    this.network.kind = None;
                    cx.notify();
                }),
            ),
        );
        for kind in ResourceKind::ALL {
            bar = bar.child(
                filter_pill(
                    ("devtools-network-kind", kind as usize),
                    kind.label(),
                    self.kind == Some(kind),
                    &ink,
                )
                .on_click(cx.listener(
                    move |this: &mut DevTools, _event, _window, cx| {
                        this.network.kind = Some(kind);
                        cx.notify();
                    },
                )),
            );
        }
        bar = bar.child(div().flex_1().child(self.search.clone()));

        if records.is_empty() {
            return div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h(px(0.0))
                .w_full()
                .child(bar)
                .child(empty_state("No network activity recorded", &ink))
                .into_any_element();
        }

        let selected = self
            .selected
            .and_then(|id| records.iter().find(|record| record.id == id).cloned());

        let table = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .child(self.header_row(&ink))
            .child(self.rows(&records, span, &ink, cx))
            .child(self.summary(&records, &ink));

        let mut body = div().flex().flex_1().min_h(px(0.0)).w_full().child(table);

        if let Some(record) = selected {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .w(px(SIDEBAR_WIDTH))
                    .h_full()
                    .border_l_1()
                    .border_color(ink.border)
                    .bg(ink.content)
                    .child(self.detail_tabs(&ink, cx))
                    .child(
                        div()
                            .id("devtools-network-detail")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h(px(0.0))
                            .w_full()
                            .overflow_scroll()
                            .child(self.detail_body(&record, &ink)),
                    ),
            );
        }

        let _ = window;

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .child(bar)
            .child(body)
            .into_any_element()
    }

    fn header_row(&self, ink: &Ink) -> gpui::Div {
        div()
            .flex()
            .flex_none()
            .items_center()
            .h(px(20.0))
            .w_full()
            .bg(ink.chrome)
            .border_b_1()
            .border_color(ink.border)
            .child(header_cell("Name", None, ink))
            .child(header_cell("Domain", Some(108.0), ink))
            .child(header_cell("Type", Some(62.0), ink))
            .child(header_cell("Method", Some(52.0), ink))
            .child(header_cell("Status", Some(50.0), ink))
            .child(header_cell("Transfer", Some(64.0), ink))
            .child(header_cell("Time", Some(58.0), ink))
            .child(header_cell("Waterfall", Some(132.0), ink))
    }

    fn rows(
        &self,
        records: &[NetworkRecord],
        span: (std::time::Duration, std::time::Duration),
        ink: &Ink,
        cx: &mut Context<DevTools>,
    ) -> gpui::Stateful<gpui::Div> {
        let (start, end) = span;
        let window = (end.saturating_sub(start)).as_secs_f32().max(0.001);

        let mut list = div()
            .id("devtools-network-rows")
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .overflow_scroll()
            .bg(ink.content)
            .font_family(MONO_FAMILY)
            .text_size(px(MONO_SIZE));

        for (position, record) in records.iter().enumerate() {
            let is_selected = self.selected == Some(record.id);
            let id = record.id;
            let hover_bg = ink.hover;
            let status_color = Self::status_color(record, ink);
            let text = if is_selected {
                ink.selected_text
            } else {
                ink.text
            };
            let dim = if is_selected {
                ink.selected_text
            } else {
                ink.dim
            };

            let status = match (record.state, record.status) {
                (RequestState::Pending, _) => "—".to_string(),
                (RequestState::Failed, _) => "failed".to_string(),
                (RequestState::Canceled, _) => "cancel".to_string(),
                (_, Some(status)) => status.to_string(),
                (_, None) => "—".to_string(),
            };

            list = list.child(
                div()
                    .id(("devtools-network-row", position))
                    .flex()
                    .items_center()
                    .flex_none()
                    .h(px(ROW_HEIGHT))
                    .w_full()
                    .when(is_selected, |el| el.bg(ink.selected))
                    .when(!is_selected && position % 2 == 1, |el| el.bg(ink.stripe))
                    .when(!is_selected, |el| el.hover(move |st| st.bg(hover_bg)))
                    .child(cell(record.name().to_string(), None, text))
                    .child(cell(record.domain().to_string(), Some(108.0), dim))
                    .child(cell(
                        record.kind.label(),
                        Some(62.0),
                        if is_selected {
                            ink.selected_text
                        } else {
                            Self::kind_color(record.kind, ink)
                        },
                    ))
                    .child(cell(record.method.clone(), Some(52.0), dim))
                    .child(cell(
                        status,
                        Some(50.0),
                        if is_selected {
                            ink.selected_text
                        } else {
                            status_color
                        },
                    ))
                    .child(cell(
                        if record.cached {
                            "cached".to_string()
                        } else {
                            format_bytes(record.transfer_size)
                        },
                        Some(64.0),
                        dim,
                    ))
                    .child(cell(format_duration(record.duration()), Some(58.0), dim))
                    .child(self.waterfall(record, start, window, ink))
                    .on_click(
                        cx.listener(move |this: &mut DevTools, _event, _window, cx| {
                            this.network.selected = Some(id);
                            cx.notify();
                        }),
                    ),
            );
        }

        list
    }

    /// The timing bar: one segment per phase, positioned in the shared window
    /// so rows line up against each other.
    fn waterfall(
        &self,
        record: &NetworkRecord,
        span_start: std::time::Duration,
        span: f32,
        ink: &Ink,
    ) -> gpui::Div {
        let offset = record.start.saturating_sub(span_start).as_secs_f32() / span;
        let phases = record.timings.phases();
        let colors = [
            ink.dim,
            ink.info,
            ink.warning,
            ink.success,
            ink.accent,
            ink.tag,
        ];

        let mut track = div()
            .flex()
            .items_center()
            .h(px(9.0))
            .w_full()
            .pl(gpui::relative(offset.clamp(0.0, 0.98)));

        if phases.is_empty() {
            // A request still in flight has no measured phases yet; show a stub
            // so the row is not silently blank.
            track = track.child(
                div()
                    .h(px(7.0))
                    .w(px(3.0))
                    .rounded(px(2.0))
                    .bg(ink.dim.opacity(0.6)),
            );
        }

        for (index, (_, duration)) in phases.iter().enumerate() {
            let fraction = (duration.as_secs_f32() / span).clamp(0.0015, 1.0);
            track = track.child(
                div()
                    .h(px(7.0))
                    .w(gpui::relative(fraction))
                    .bg(colors[index % colors.len()]),
            );
        }

        div()
            .flex()
            .items_center()
            .flex_none()
            .w(px(132.0))
            .h_full()
            .px(px(6.0))
            .child(track)
    }

    fn summary(&self, records: &[NetworkRecord], ink: &Ink) -> gpui::Div {
        let transfer: u64 = records.iter().map(|record| record.transfer_size).sum();
        let resource: u64 = records.iter().map(|record| record.resource_size).sum();
        let slowest = records
            .iter()
            .map(|record| record.duration())
            .max()
            .unwrap_or_default();

        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(12.0))
            .h(px(20.0))
            .w_full()
            .px(px(8.0))
            .bg(ink.chrome)
            .border_t_1()
            .border_color(ink.border)
            .text_size(px(LABEL_SIZE))
            .text_color(ink.dim)
            .child(SharedString::from(format!("{} requests", records.len())))
            .child(SharedString::from(format!(
                "{} transferred",
                format_bytes(transfer)
            )))
            .child(SharedString::from(format!(
                "{} resources",
                format_bytes(resource)
            )))
            .child(SharedString::from(format!(
                "slowest {}",
                format_duration(slowest)
            )))
    }

    fn detail_tabs(&self, ink: &Ink, cx: &mut Context<DevTools>) -> gpui::Div {
        let mut tabs = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(3.0))
            .h(px(26.0))
            .px(px(6.0))
            .w_full()
            .bg(ink.chrome)
            .border_b_1()
            .border_color(ink.border);

        for detail in RequestDetail::ALL {
            tabs = tabs.child(
                filter_pill(
                    ("devtools-network-detail-tab", detail as usize),
                    detail.label(),
                    self.detail == detail,
                    ink,
                )
                .on_click(cx.listener(
                    move |this: &mut DevTools, _event, _window, cx| {
                        this.network.detail = detail;
                        cx.notify();
                    },
                )),
            );
        }

        tabs
    }

    fn detail_body(&self, record: &NetworkRecord, ink: &Ink) -> AnyElement {
        match self.detail {
            RequestDetail::Headers => self.headers_view(record, ink),
            RequestDetail::Cookies => self.cookies_view(record, ink),
            RequestDetail::Sizes => self.sizes_view(record, ink),
            RequestDetail::Timing => self.timing_view(record, ink),
            RequestDetail::Preview => self.preview_view(record, ink),
        }
    }

    fn headers_view(&self, record: &NetworkRecord, ink: &Ink) -> AnyElement {
        let mut general = div().flex().flex_col().w_full().py(px(4.0));
        general = general.child(kv_row("URL", record.url.clone(), ink));
        general = general.child(kv_row("Method", record.method.clone(), ink));
        general = general.child(kv_row(
            "Status",
            match record.status {
                Some(status) => SharedString::from(format!("{status} {}", record.status_text)),
                None => SharedString::new_static("—"),
            },
            ink,
        ));
        if !record.protocol.is_empty() {
            general = general.child(kv_row("Protocol", record.protocol.clone(), ink));
        }
        if !record.remote_address.is_empty() {
            general = general.child(kv_row("Address", record.remote_address.clone(), ink));
        }
        if !record.priority.is_empty() {
            general = general.child(kv_row("Priority", record.priority.clone(), ink));
        }
        if let Some(initiator) = &record.initiator {
            general = general.child(kv_row(
                "Initiator",
                SharedString::from(initiator.short()),
                ink,
            ));
        }
        if let Some(error) = &record.error {
            general = general.child(kv_row("Error", error.clone(), ink));
        }

        let headers = |rows: &[(SharedString, SharedString)]| {
            let mut block = div().flex().flex_col().w_full().py(px(4.0));
            if rows.is_empty() {
                block = block.child(
                    div()
                        .px(px(8.0))
                        .py(px(2.0))
                        .text_size(px(LABEL_SIZE))
                        .text_color(ink.dim)
                        .child(SharedString::new_static("None")),
                );
            }
            for (name, value) in rows {
                block = block.child(kv_row(name.clone(), value.clone(), ink));
            }
            block
        };

        div()
            .flex()
            .flex_col()
            .w_full()
            .child(section_header("General", ink))
            .child(general)
            .child(section_header("Request Headers", ink))
            .child(headers(&record.request_headers))
            .child(section_header("Response Headers", ink))
            .child(headers(&record.response_headers))
            .into_any_element()
    }

    fn cookies_view(&self, record: &NetworkRecord, ink: &Ink) -> AnyElement {
        let collect = |rows: &[(SharedString, SharedString)], wanted: &str| {
            rows.iter()
                .filter(|(name, _)| name.eq_ignore_ascii_case(wanted))
                .flat_map(|(_, value)| parse_cookies(value))
                .collect::<Vec<_>>()
        };
        let sent = collect(&record.request_headers, "cookie");
        let received = collect(&record.response_headers, "set-cookie");

        let block = |cookies: Vec<(String, String)>| {
            let mut block = div().flex().flex_col().w_full().py(px(4.0));
            if cookies.is_empty() {
                block = block.child(
                    div()
                        .px(px(8.0))
                        .py(px(2.0))
                        .text_size(px(LABEL_SIZE))
                        .text_color(ink.dim)
                        .child(SharedString::new_static("No cookies")),
                );
            }
            for (name, value) in cookies {
                block = block.child(kv_row(name, value, ink));
            }
            block
        };

        div()
            .flex()
            .flex_col()
            .w_full()
            .child(section_header("Request Cookies", ink))
            .child(block(sent))
            .child(section_header("Response Cookies", ink))
            .child(block(received))
            .into_any_element()
    }

    fn sizes_view(&self, record: &NetworkRecord, ink: &Ink) -> AnyElement {
        let saved = record.resource_size.saturating_sub(record.transfer_size);
        let ratio = if record.resource_size > 0 {
            saved as f32 / record.resource_size as f32 * 100.0
        } else {
            0.0
        };

        div()
            .flex()
            .flex_col()
            .w_full()
            .child(section_header("Sizes", ink))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .py(px(4.0))
                    .child(kv_row(
                        "Transfer",
                        SharedString::from(format_bytes(record.transfer_size)),
                        ink,
                    ))
                    .child(kv_row(
                        "Resource",
                        SharedString::from(format_bytes(record.resource_size)),
                        ink,
                    ))
                    .child(kv_row(
                        "Saved",
                        SharedString::from(format!("{} ({:.0}%)", format_bytes(saved), ratio)),
                        ink,
                    ))
                    .child(kv_row(
                        "Cached",
                        SharedString::new_static(if record.cached { "Yes" } else { "No" }),
                        ink,
                    )),
            )
            .into_any_element()
    }

    fn timing_view(&self, record: &NetworkRecord, ink: &Ink) -> AnyElement {
        let total = record.duration().as_secs_f32().max(0.0001);
        let colors = [
            ink.dim,
            ink.info,
            ink.warning,
            ink.success,
            ink.accent,
            ink.tag,
        ];

        let mut rows = div().flex().flex_col().w_full().py(px(4.0));
        for (index, (name, duration)) in record.timings.phases().into_iter().enumerate() {
            let fraction = (duration.as_secs_f32() / total).clamp(0.0, 1.0);
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .w_full()
                    .px(px(8.0))
                    .py(px(2.0))
                    .font_family(MONO_FAMILY)
                    .text_size(px(MONO_SIZE))
                    .child(
                        div()
                            .flex_none()
                            .w(px(64.0))
                            .text_color(ink.dim)
                            .child(SharedString::new_static(name)),
                    )
                    .child(
                        div().flex_1().h(px(8.0)).bg(ink.stripe).child(
                            div()
                                .h_full()
                                .w(gpui::relative(fraction))
                                .bg(colors[index % colors.len()]),
                        ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(62.0))
                            .text_color(ink.text)
                            .child(SharedString::from(format_duration(duration))),
                    ),
            );
        }

        div()
            .flex()
            .flex_col()
            .w_full()
            .child(section_header("Timing", ink))
            .child(rows)
            .child(div().flex().flex_col().w_full().pb(px(6.0)).child(kv_row(
                "Total",
                SharedString::from(format_duration(record.duration())),
                ink,
            )))
            .into_any_element()
    }

    fn preview_view(&self, record: &NetworkRecord, ink: &Ink) -> AnyElement {
        let body = record
            .response_body
            .clone()
            .or_else(|| record.request_body.clone());

        let content = match body {
            None => div()
                .px(px(8.0))
                .py(px(6.0))
                .text_size(px(LABEL_SIZE))
                .text_color(ink.dim)
                .child(SharedString::new_static("No body recorded")),
            Some(body) => div()
                .w_full()
                .px(px(8.0))
                .py(px(6.0))
                .font_family(MONO_FAMILY)
                .text_size(px(MONO_SIZE))
                .text_color(ink.text)
                .child(body),
        };

        div()
            .flex()
            .flex_col()
            .w_full()
            .child(section_header(
                SharedString::from(elide(record.url.as_ref(), 40)),
                ink,
            ))
            .child(content)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> NetworkRecord {
        NetworkRecord::new("GET", "https://api.example.com/v1/users?page=2")
            .kind(ResourceKind::Fetch)
            .status(200, "OK")
    }

    #[test]
    fn the_type_filter_is_exact() {
        assert!(matches(&record(), Some(ResourceKind::Fetch), ""));
        assert!(!matches(&record(), Some(ResourceKind::Image), ""));
        assert!(matches(&record(), None, ""));
    }

    #[test]
    fn search_covers_url_method_and_status() {
        assert!(matches(&record(), None, "users"));
        assert!(matches(&record(), None, "GET"));
        assert!(matches(&record(), None, "200"));
        assert!(!matches(&record(), None, "404"));
    }

    #[test]
    fn search_is_case_insensitive() {
        assert!(matches(&record(), None, "USERS"));
        assert!(matches(&record(), None, "get"));
    }

    #[test]
    fn the_type_filter_and_the_search_both_have_to_pass() {
        assert!(!matches(&record(), Some(ResourceKind::Image), "users"));
        assert!(!matches(&record(), Some(ResourceKind::Fetch), "absent"));
        assert!(matches(&record(), Some(ResourceKind::Fetch), "users"));
    }

    #[test]
    fn cookies_split_on_semicolons() {
        let cookies = parse_cookies("session=abc123; theme=dark; region=us-east");
        assert_eq!(
            cookies,
            vec![
                ("session".to_string(), "abc123".to_string()),
                ("theme".to_string(), "dark".to_string()),
                ("region".to_string(), "us-east".to_string()),
            ]
        );
    }

    #[test]
    fn a_set_cookie_keeps_its_attributes_as_pairs_and_drops_bare_flags() {
        let cookies = parse_cookies("session=abc; Path=/; HttpOnly; Max-Age=3600");
        assert_eq!(cookies.len(), 3);
        assert_eq!(cookies[1], ("Path".to_string(), "/".to_string()));
        assert!(!cookies.iter().any(|(name, _)| name == "HttpOnly"));
    }

    #[test]
    fn an_empty_cookie_header_yields_nothing() {
        assert!(parse_cookies("").is_empty());
        assert!(parse_cookies("   ; ;  ").is_empty());
    }
}
