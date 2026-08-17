//! `AITokenMeter` — how much of the context window is gone.
//!
//! Context exhaustion is the failure people hit without warning: the
//! conversation works, and then it doesn't. A bar that turns amber at
//! three-quarters and red at ninety percent turns that into something visible
//! a few turns ahead of time, which is the only point at which it's still
//! cheap to do something about it.

use gpui::prelude::*;
use gpui::{div, px, App, IntoElement, SharedString, Window};

use crate::feedback::Progress;
use crate::theme::{theme, Color, Size};

/// Fraction of the window at which the meter warns.
const WARN_AT: f32 = 0.75;
/// Fraction at which it goes critical.
const DANGER_AT: f32 = 0.90;

/// A used/limit gauge for a model's context window.
#[derive(IntoElement)]
pub struct AITokenMeter {
    used: u64,
    limit: u64,
    label: Option<SharedString>,
    size: Size,
    /// Draw the bar as well as the numbers.
    bar: bool,
}

impl AITokenMeter {
    pub fn new(used: u64, limit: u64) -> Self {
        AITokenMeter {
            used,
            limit,
            label: None,
            size: Size::Xs,
            bar: true,
        }
    }

    /// Text before the count — "Context", "Session".
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    /// Numbers only, for a status bar with no room for a bar.
    pub fn bar(mut self, bar: bool) -> Self {
        self.bar = bar;
        self
    }

    /// How full the window is, in `0.0..=1.0`. A zero limit reads as empty
    /// rather than dividing by it.
    pub fn fraction(&self) -> f32 {
        if self.limit == 0 {
            return 0.0;
        }
        (self.used as f64 / self.limit as f64).clamp(0.0, 1.0) as f32
    }
}

impl RenderOnce for AITokenMeter {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = theme(cx);
        let font = t.font_size(self.size);
        let dimmed = t.dimmed().hsla();
        let fraction = self.fraction();

        let fill: Color = if fraction >= DANGER_AT {
            t.danger()
        } else if fraction >= WARN_AT {
            t.warning()
        } else {
            t.primary()
        };
        let text_color = if fraction >= WARN_AT {
            fill.hsla()
        } else {
            dimmed
        };

        div()
            .flex()
            .flex_col()
            .gap(px(3.0))
            .w_full()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .text_size(px(font))
                    .text_color(dimmed)
                    .children(self.label)
                    .child(
                        div()
                            .text_color(text_color)
                            .child(SharedString::from(format!(
                                "{} / {}",
                                compact(self.used),
                                compact(self.limit)
                            ))),
                    ),
            )
            .when(self.bar, |column| {
                column.child(
                    Progress::new(fraction * 100.0)
                        .size(Size::Xs)
                        .color(fill.hsla()),
                )
            })
    }
}

/// Token counts run to six digits, which is unreadable in a status bar.
pub(crate) fn compact(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => {
            let thousands = n as f64 / 1_000.0;
            if thousands < 10.0 {
                format!("{thousands:.1}k")
            } else {
                format!("{}k", thousands.round() as u64)
            }
        }
        _ => {
            let millions = n as f64 / 1_000_000.0;
            format!("{millions:.1}M")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_counts_stay_short() {
        assert_eq!(compact(0), "0");
        assert_eq!(compact(999), "999");
        assert_eq!(compact(1_000), "1.0k");
        assert_eq!(compact(9_949), "9.9k");
        assert_eq!(compact(12_500), "13k");
        assert_eq!(compact(200_000), "200k");
        assert_eq!(compact(1_500_000), "1.5M");
    }

    #[test]
    fn a_zero_limit_reads_as_empty_rather_than_dividing_by_it() {
        assert_eq!(AITokenMeter::new(10, 0).fraction(), 0.0);
        assert_eq!(AITokenMeter::new(0, 100).fraction(), 0.0);
    }

    #[test]
    fn overflowing_the_window_pins_at_full() {
        assert_eq!(AITokenMeter::new(300, 100).fraction(), 1.0);
        assert_eq!(AITokenMeter::new(50, 100).fraction(), 0.5);
    }

    #[test]
    fn huge_counts_do_not_overflow_the_fraction() {
        assert_eq!(AITokenMeter::new(u64::MAX, u64::MAX).fraction(), 1.0);
        assert!(AITokenMeter::new(1, u64::MAX).fraction() < 0.001);
    }
}
