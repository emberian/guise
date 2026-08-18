//! The Timelines panel: the frame graph, the bands, and the event list.
//!
//! Safari records instruments — Network, Layout & Rendering, JavaScript &
//! Events — and lays them out against one ruler so you can see what overlapped
//! what. The Frames band here is measured by the inspector itself, from the
//! interval between its own renders; the others are whatever the host reported
//! through [`super::timeline_event`] and [`super::measure`].

use std::time::Duration;

use gpui::prelude::*;
use gpui::{div, px, AnyElement, Context, Hsla, SharedString, Window};

use super::shell::{
    cell, empty_state, filter_pill, header_cell, tool_button, Ink, LABEL_SIZE, MONO_SIZE,
    NAV_WIDTH, ROW_HEIGHT,
};
use super::state::{format_duration, DevToolsState, TimelineEvent, TimelineKind};
use super::DevTools;
use crate::icon::IconName;
use crate::style::MONO_FAMILY;

/// The frame budget for 60 Hz. Anything past this dropped a frame, and the
/// graph colors it accordingly.
const FRAME_BUDGET: Duration = Duration::from_micros(16_667);

#[derive(Default)]
pub struct TimelinesPanel {
    /// `None` shows every band, as Safari's "All" instrument does.
    band: Option<TimelineKind>,
    /// Whether the frame recorder is armed.
    ///
    /// Off by default, and that is not laziness. gpui paints on demand, so the
    /// interval between two frames of an idle window is however long it sat
    /// idle — reported as a frame rate it reads as a stall that never
    /// happened. Safari makes you press Record for the same reason: the
    /// measurement only means something while something is being measured.
    pub(crate) recording: bool,
}

impl TimelinesPanel {
    /// The window every band is laid out against: the earliest start to the
    /// latest end across the events being shown.
    pub fn span(events: &[TimelineEvent]) -> (Duration, Duration) {
        let start = events
            .iter()
            .map(|event| event.start)
            .min()
            .unwrap_or(Duration::ZERO);
        let end = events
            .iter()
            .map(|event| event.end())
            .max()
            .unwrap_or(Duration::ZERO);
        (start, end.max(start))
    }

    fn band_color(kind: TimelineKind, ink: &Ink) -> Hsla {
        match kind {
            TimelineKind::Frame => ink.accent,
            TimelineKind::Layout => ink.info,
            TimelineKind::Paint => ink.tag,
            TimelineKind::Script => ink.warning,
            TimelineKind::Network => ink.success,
        }
    }

    pub fn render(&self, window: &mut Window, cx: &mut Context<DevTools>) -> AnyElement {
        let ink = Ink::read(cx);
        let (events, frames, fps) = cx
            .try_global::<DevToolsState>()
            .map(|state| {
                let events: Vec<TimelineEvent> = state
                    .timeline()
                    .iter()
                    .filter(|event| self.band.is_none_or(|band| event.kind == band))
                    .cloned()
                    .collect();
                let frames: Vec<Duration> = state.frames().iter().copied().collect();
                (events, frames, state.fps())
            })
            .unwrap_or_default();

        let mut bar = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(4.0))
            .h(px(26.0))
            .px(px(8.0))
            .w_full()
            .bg(ink.chrome)
            .border_b_1()
            .border_color(ink.border)
            .child(
                tool_button(
                    "devtools-timeline-record",
                    if self.recording {
                        IconName::Square
                    } else {
                        IconName::Circle
                    },
                    if self.recording {
                        "Stop recording"
                    } else {
                        "Start recording"
                    },
                    self.recording,
                    &ink,
                    cx,
                )
                .on_click(cx.listener(
                    |this: &mut DevTools, _event, _window, cx| {
                        this.timelines.recording = !this.timelines.recording;
                        if !this.timelines.recording && cx.has_global::<DevToolsState>() {
                            cx.update_global::<DevToolsState, _>(|state, _cx| state.stop_frames());
                        }
                        cx.notify();
                    },
                )),
            )
            .child(
                filter_pill("devtools-timeline-all", "All", self.band.is_none(), &ink).on_click(
                    cx.listener(|this: &mut DevTools, _event, _window, cx| {
                        this.timelines.band = None;
                        cx.notify();
                    }),
                ),
            );
        for kind in TimelineKind::ALL {
            bar = bar.child(
                filter_pill(
                    ("devtools-timeline-band", kind as usize),
                    kind.label(),
                    self.band == Some(kind),
                    &ink,
                )
                .on_click(cx.listener(
                    move |this: &mut DevTools, _event, _window, cx| {
                        this.timelines.band = Some(kind);
                        cx.notify();
                    },
                )),
            );
        }
        bar = bar.child(div().flex_1()).when_some(fps, |el, fps| {
            el.child(
                div()
                    .text_size(px(LABEL_SIZE))
                    .text_color(if fps >= 55.0 {
                        ink.success
                    } else {
                        ink.warning
                    })
                    .child(SharedString::from(format!("{fps:.0} fps"))),
            )
        });

        let _ = window;

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .child(bar)
            .child(self.frame_graph(&frames, &ink))
            .child(self.bands(&events, &ink))
            .child(self.list(&events, &ink, cx))
            .into_any_element()
    }

    /// One bar per recorded frame, red past the 60 Hz budget. This is the band
    /// that makes a stutter visible without any host instrumentation at all.
    fn frame_graph(&self, frames: &[Duration], ink: &Ink) -> AnyElement {
        let mut graph = div()
            .flex()
            .flex_none()
            .items_end()
            .gap(px(1.0))
            .h(px(46.0))
            .w_full()
            .px(px(8.0))
            .py(px(4.0))
            .bg(ink.content)
            .border_b_1()
            .border_color(ink.border);

        if frames.is_empty() {
            return graph
                .items_center()
                .child(div().text_size(px(LABEL_SIZE)).text_color(ink.dim).child(
                    SharedString::new_static(if self.recording {
                        "Measuring frames…"
                    } else {
                        "Press Record to measure frames"
                    }),
                ))
                .into_any_element();
        }

        // Scale against twice the budget so a normal frame sits at half height
        // and a dropped one visibly spikes.
        let ceiling = (FRAME_BUDGET.as_secs_f32() * 2.0).max(0.001);
        for delta in frames {
            let fraction = (delta.as_secs_f32() / ceiling).clamp(0.02, 1.0);
            let over = *delta > FRAME_BUDGET;
            graph = graph.child(
                div()
                    .flex_1()
                    .min_w(px(1.0))
                    .h(gpui::relative(fraction))
                    .bg(if over { ink.danger } else { ink.success }),
            );
        }

        graph.into_any_element()
    }

    /// The instrument bands: one row per kind, spans placed on a shared ruler.
    fn bands(&self, events: &[TimelineEvent], ink: &Ink) -> AnyElement {
        if events.is_empty() {
            return div().into_any_element();
        }

        let (start, end) = Self::span(events);
        let window = (end.saturating_sub(start)).as_secs_f32().max(0.001);

        let mut rows = div()
            .flex()
            .flex_col()
            .flex_none()
            .w_full()
            .bg(ink.content)
            .border_b_1()
            .border_color(ink.border);

        for kind in TimelineKind::ALL {
            let band: Vec<&TimelineEvent> =
                events.iter().filter(|event| event.kind == kind).collect();
            if band.is_empty() {
                continue;
            }

            let mut track = div().relative().flex_1().h(px(14.0)).bg(ink.stripe);
            for event in band {
                let offset = event.start.saturating_sub(start).as_secs_f32() / window;
                let width = (event.duration.as_secs_f32() / window).clamp(0.002, 1.0);
                track = track.child(
                    div()
                        .absolute()
                        .top(px(2.0))
                        .left(gpui::relative(offset.clamp(0.0, 0.998)))
                        .w(gpui::relative(width))
                        .h(px(10.0))
                        .rounded(px(2.0))
                        .bg(Self::band_color(kind, ink)),
                );
            }

            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .w_full()
                    .px(px(8.0))
                    .py(px(2.0))
                    .child(
                        div()
                            .flex_none()
                            .w(px(NAV_WIDTH - 40.0))
                            .text_size(px(LABEL_SIZE))
                            .text_color(ink.dim)
                            .child(SharedString::new_static(kind.label())),
                    )
                    .child(track),
            );
        }

        rows.into_any_element()
    }

    fn list(&self, events: &[TimelineEvent], ink: &Ink, cx: &mut Context<DevTools>) -> AnyElement {
        if events.is_empty() {
            return empty_state(
                "No timeline events recorded. Wrap work in devtools::measure to populate this.",
                ink,
            )
            .into_any_element();
        }

        let mut list = div()
            .id("devtools-timeline-list")
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .overflow_scroll()
            .bg(ink.content)
            .font_family(MONO_FAMILY)
            .text_size(px(MONO_SIZE));

        list = list.child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .h(px(20.0))
                .w_full()
                .bg(ink.chrome)
                .border_b_1()
                .border_color(ink.border)
                .child(header_cell("Event", None, ink))
                .child(header_cell("Type", Some(140.0), ink))
                .child(header_cell("Start", Some(80.0), ink))
                .child(header_cell("Duration", Some(80.0), ink)),
        );

        // Newest first: the thing that just happened is the thing being chased.
        for (position, event) in events.iter().rev().enumerate() {
            let source = event.source.clone();
            let hover_bg = ink.hover;
            let mut row = div()
                .id(("devtools-timeline-row", position))
                .flex()
                .items_center()
                .flex_none()
                .h(px(ROW_HEIGHT))
                .w_full()
                .when(position % 2 == 1, |el| el.bg(ink.stripe))
                .hover(move |st| st.bg(hover_bg))
                .child(cell(event.label.clone(), None, ink.text))
                .child(cell(
                    event.kind.label(),
                    Some(140.0),
                    Self::band_color(event.kind, ink),
                ))
                .child(cell(format_duration(event.start), Some(80.0), ink.dim))
                .child(cell(
                    format_duration(event.duration),
                    Some(80.0),
                    if event.duration > FRAME_BUDGET {
                        ink.warning
                    } else {
                        ink.dim
                    },
                ));

            if let Some(source) = source {
                row = row.on_click(
                    cx.listener(move |this: &mut DevTools, _event, _window, cx| {
                        this.reveal_source(source.clone(), cx);
                    }),
                );
            }

            list = list.child(row);
        }

        list.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: TimelineKind, start_ms: u64, duration_ms: u64) -> TimelineEvent {
        let mut event = TimelineEvent::new(kind, "work", Duration::from_millis(duration_ms));
        event.start = Duration::from_millis(start_ms);
        event
    }

    #[test]
    fn the_span_covers_every_event() {
        let events = vec![
            event(TimelineKind::Script, 100, 40),
            event(TimelineKind::Layout, 20, 10),
            event(TimelineKind::Paint, 60, 5),
        ];

        let (start, end) = TimelinesPanel::span(&events);
        assert_eq!(start, Duration::from_millis(20));
        assert_eq!(end, Duration::from_millis(140));
    }

    #[test]
    fn an_empty_timeline_has_a_zero_span() {
        assert_eq!(TimelinesPanel::span(&[]), (Duration::ZERO, Duration::ZERO));
    }

    #[test]
    fn a_single_instant_event_does_not_invert_the_span() {
        let events = vec![event(TimelineKind::Script, 50, 0)];
        let (start, end) = TimelinesPanel::span(&events);
        assert_eq!(start, Duration::from_millis(50));
        assert_eq!(end, Duration::from_millis(50));
    }

    #[test]
    fn the_frame_budget_is_sixty_hertz() {
        assert!(FRAME_BUDGET < Duration::from_millis(17));
        assert!(FRAME_BUDGET > Duration::from_millis(16));
    }
}
