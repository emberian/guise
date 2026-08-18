//! `AIComposer` — the box you type the prompt into.
//!
//! Three things make this different from a plain [`TextArea`]: Enter sends and
//! Shift+Enter breaks the line, the box grows with what's in it up to a
//! ceiling, and the send button becomes a stop button while a reply is in
//! flight — because being able to interrupt a long generation is the control
//! people reach for most.
//!
//! It emits [`AIComposerEvent`] and holds no opinion about what happens next;
//! the host sends the request.
//!
//! ```ignore
//! let composer = cx.new(|cx| AIComposer::new(cx).placeholder("Ask anything\u{2026}"));
//! cx.subscribe(&composer, |this, composer, event: &AIComposerEvent, cx| {
//!     match event {
//!         AIComposerEvent::Submit(text) => this.send(text.clone(), cx),
//!         AIComposerEvent::Stop => this.cancel(cx),
//!         AIComposerEvent::Attach => this.pick_files(cx),
//!     }
//! })
//! .detach();
//! ```

use gpui::prelude::*;
use gpui::{
    div, px, Context, Entity, EventEmitter, FocusHandle, IntoElement, SharedString, Window,
};

use crate::actionicon::ActionIcon;
use crate::devtools::Probed;
use crate::icon::IconName;
use crate::input::{TextArea, TextAreaEvent, TextAreaSubmit};
use crate::style::Variant;
use crate::theme::{theme, ColorName, Size};

/// What the composer asks the host to do.
#[derive(Debug, Clone)]
pub enum AIComposerEvent {
    /// Send this. The composer has already cleared itself.
    Submit(String),
    /// Interrupt the reply in flight.
    Stop,
    /// The attach button was pressed; the host owns the file dialog.
    Attach,
    /// The draft changed. Carries the current text, for hosts that persist it.
    Change(String),
}

/// A prompt box with a send/stop button.
pub struct AIComposer {
    input: Entity<TextArea>,
    /// A reply is streaming: send becomes stop.
    busy: bool,
    /// Show the paperclip.
    attachments: bool,
    hint: Option<SharedString>,
    size: Size,
    disabled: bool,
}

impl EventEmitter<AIComposerEvent> for AIComposer {}

impl AIComposer {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            TextArea::new(cx)
                .rows(1)
                .max_rows(12)
                .submit_on_enter(true)
                .placeholder("Send a message\u{2026}")
        });
        // The field is a child entity, so its events have to be forwarded on
        // rather than reaching the host directly.
        cx.subscribe(&input, |this, input, _event: &TextAreaSubmit, cx| {
            let text = input.read(cx).text();
            this.submit(text, cx);
        })
        .detach();
        cx.subscribe(&input, |_this, _input, event: &TextAreaEvent, cx| {
            cx.emit(AIComposerEvent::Change(event.0.clone()));
        })
        .detach();

        AIComposer {
            input,
            busy: false,
            attachments: false,
            hint: None,
            size: Size::Sm,
            disabled: false,
        }
    }

    pub fn placeholder(self, placeholder: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        let placeholder = placeholder.into();
        self.input.update(cx, |input, cx| {
            input.set_placeholder(placeholder, cx);
        });
        self
    }

    /// Offer an attach button, which emits [`AIComposerEvent::Attach`].
    pub fn attachments(mut self, attachments: bool) -> Self {
        self.attachments = attachments;
        self
    }

    /// A line under the box — "Shift+Enter for a new line", a model name, a
    /// disclaimer.
    pub fn hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// The inner field, for hosts that need to bind or style it.
    pub fn input(&self) -> &Entity<TextArea> {
        &self.input
    }

    pub fn focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.input.read(cx).focus_handle()
    }

    /// The current draft.
    pub fn text(&self, cx: &gpui::App) -> String {
        self.input.read(cx).text()
    }

    /// Replace the draft — restoring a saved one, or dropping in a template.
    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.set_text(text, cx));
        cx.notify();
    }

    /// Switch the send button to a stop button. Set it when the request goes
    /// out and clear it when the reply ends, including on error.
    pub fn set_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.busy = busy;
        cx.notify();
    }

    pub fn is_busy(&self) -> bool {
        self.busy
    }

    /// Send the draft and clear the box. An empty or blank draft does nothing
    /// — sending whitespace to a model is never what was meant.
    fn submit(&mut self, text: String, cx: &mut Context<Self>) {
        if self.disabled || self.busy || text.trim().is_empty() {
            return;
        }
        self.input.update(cx, |input, cx| input.set_text("", cx));
        cx.emit(AIComposerEvent::Submit(text));
        cx.notify();
    }
}

impl Render for AIComposer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let dimmed = t.dimmed().hsla();
        let font_xs = t.font_size(Size::Xs);
        let empty = self.input.read(cx).is_blank();

        let action = if self.busy {
            ActionIcon::new("guise-ai-stop", IconName::Square)
                .variant(Variant::Filled)
                .color(ColorName::Red)
                .size(self.size)
                .on_click(cx.listener(|_this, _event, _window, cx| {
                    cx.emit(AIComposerEvent::Stop);
                }))
        } else {
            ActionIcon::new("guise-ai-send", IconName::ArrowUp)
                .variant(Variant::Filled)
                .size(self.size)
                // Nothing to send is a disabled button, not a silent no-op:
                // the state is visible before the click.
                .disabled(self.disabled || empty)
                .on_click(cx.listener(|this, _event, _window, cx| {
                    let text = this.input.read(cx).text();
                    this.submit(text, cx);
                }))
        };

        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .w_full()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_end()
                    .gap(px(8.0))
                    .w_full()
                    .child(div().flex_1().min_w(px(0.0)).child(self.input.clone()))
                    .when(self.attachments, |row| {
                        row.child(
                            ActionIcon::new("guise-ai-attach", IconName::Paperclip)
                                .variant(Variant::Subtle)
                                .color(ColorName::Gray)
                                .size(self.size)
                                .disabled(self.disabled)
                                .on_click(cx.listener(|_this, _event, _window, cx| {
                                    cx.emit(AIComposerEvent::Attach);
                                })),
                        )
                    })
                    .child(action),
            )
            .when_some(self.hint.clone(), |column, hint| {
                column.child(div().text_size(px(font_xs)).text_color(dimmed).child(hint))
            })
            .probe("AIComposer")
    }
}
