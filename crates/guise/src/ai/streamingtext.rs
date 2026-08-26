//! `AIStreamingText` — markdown with a caret on the end.
//!
//! A reply that is still arriving reads as finished unless something says
//! otherwise, and a spinner in the corner is the wrong signal — the text is
//! already there, it is just not done. So this renders exactly what
//! [`Markdown`] renders and puts a blinking block on the last line, the way a
//! terminal shows a process still writing.
//!
//! It takes the whole text every frame rather than a delta, because that is
//! what a `Render` pass has: the host appends to its own `String` and this
//! draws it.
//!
//! ```ignore
//! AIStreamingText::new(&partial_reply)
//! ```

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{canvas, div, fill, px, App, Bounds, IntoElement, Pixels, SharedString, Window};

use crate::devtools::Probed;
use crate::frameclock::{request_frame, FrameKind};
use crate::markdown::Markdown;
use crate::theme::{theme, Size};

/// How long the caret takes to go from solid to clear and back.
const BLINK_MS: u64 = 900;

fn animation_start() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

fn request_next_toggle(window: &mut Window, cx: &mut App, after_ms: u64) {
    request_frame(
        FrameKind::Caret,
        Duration::from_millis(after_ms.max(1)),
        window,
        cx,
    );
}

/// Streaming markdown with a trailing caret.
#[derive(IntoElement)]
pub struct AIStreamingText {
    text: SharedString,
    size: Size,
    caret: bool,
}

impl AIStreamingText {
    pub fn new(text: impl Into<SharedString>) -> Self {
        AIStreamingText {
            text: text.into(),
            size: Size::Sm,
            caret: true,
        }
    }

    pub fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    /// Drop the caret while keeping the same layout — for the frame a reply
    /// finishes on, so the text doesn't jump.
    pub fn caret(mut self, caret: bool) -> Self {
        self.caret = caret;
        self
    }
}

impl RenderOnce for AIStreamingText {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = theme(cx);
        let font = t.font_size(self.size);
        let caret_color = t.text().hsla();

        // The caret sits on its own row under the text rather than inline:
        // the markdown body is a column of laid-out lines, and threading a
        // caret into the last one would mean shaping the text twice.
        let caret = canvas(
            |_, _, _| (),
            move |bounds: Bounds<Pixels>, _, window, cx| {
                if !bounds.intersects(&window.content_mask().bounds) {
                    return;
                }
                let elapsed = animation_start().elapsed().as_millis() as u64 % BLINK_MS;
                let half = BLINK_MS / 2;
                if elapsed < half {
                    window.paint_quad(fill(bounds, caret_color));
                    request_next_toggle(window, cx, half - elapsed);
                } else {
                    request_next_toggle(window, cx, BLINK_MS - elapsed);
                }
            },
        )
        .w(px(font * 0.5))
        .h(px(font * 1.1));

        div()
            .flex()
            .flex_col()
            .w_full()
            .child(Markdown::new(self.text).size(self.size))
            .when(self.caret, |column| {
                column.child(div().flex().items_center().h(px(font * 1.3)).child(caret))
            })
            .probe("AIStreamingText")
    }
}
