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

use std::time::Duration;

use gpui::prelude::*;
use gpui::{div, px, Animation, AnimationExt, App, IntoElement, SharedString, Window};

use crate::devtools::Probed;
use crate::markdown::Markdown;
use crate::theme::{theme, Size};

/// How long the caret takes to go from solid to clear and back.
const BLINK_MS: u64 = 900;

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
        let caret = div()
            .w(px(font * 0.5))
            .h(px(font * 1.1))
            .bg(caret_color)
            .with_animation(
                "guise-ai-caret",
                Animation::new(Duration::from_millis(BLINK_MS)).repeat(),
                // A square wave, not a fade: a caret that dims looks like a
                // rendering artifact, one that switches looks deliberate.
                |caret, delta| caret.opacity(if delta < 0.5 { 1.0 } else { 0.0 }),
            );

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
