//! `TextArea` — a multiline text field (gpui entity).
//!
//! Reuses the [`TextEdit`] char model (newline-aware), renders line-by-line with
//! a caret on the active line, and emits [`TextAreaEvent`] on edit. Enter inserts
//! a newline; up/down move between lines keeping the column.

use gpui::prelude::*;
use gpui::{
  div, px, App, ClipboardItem, Context, Entity, EventEmitter, FocusHandle, IntoElement,
  KeyDownEvent, MouseButton, ScrollHandle, SharedString, Window,
};

use super::{control_metrics, Field, TextEdit};
use crate::devtools::ProbedAny;
use crate::reactive::Signal;
use crate::theme::{theme, ColorName, Size};

/// Emitted as the user edits the field. Carries the full new value.
#[derive(Debug, Clone)]
pub struct TextAreaEvent(pub String);

/// Emitted when Enter commits the field, which only happens under
/// [`TextArea::submit_on_enter`]. Carries the value. It is a separate event
/// type rather than a variant so that subscribers to [`TextAreaEvent`] keep
/// working unchanged.
#[derive(Debug, Clone)]
pub struct TextAreaSubmit(pub String);

/// A multiline text field. Create with `cx.new(|cx| TextArea::new(cx))`.
pub struct TextArea {
  edit: TextEdit,
  focus: FocusHandle,
  scroll: ScrollHandle,
  placeholder: SharedString,
  label: Option<SharedString>,
  description: Option<SharedString>,
  error: Option<SharedString>,
  rows: usize,
  max_rows: Option<usize>,
  submit_on_enter: bool,
  size: Size,
  disabled: bool,
}

impl EventEmitter<TextAreaEvent> for TextArea {}
impl EventEmitter<TextAreaSubmit> for TextArea {}

/// A line that renders with height even when empty.
fn line(text: &str) -> SharedString {
  if text.is_empty() {
    SharedString::new_static(" ")
  } else {
    SharedString::from(text.to_string())
  }
}

impl TextArea {
  pub fn new(cx: &mut Context<Self>) -> Self {
    TextArea {
      edit: TextEdit::new(""),
      focus: cx.focus_handle().tab_stop(true),
      scroll: ScrollHandle::new(),
      placeholder: SharedString::default(),
      label: None,
      description: None,
      error: None,
      rows: 3,
      max_rows: None,
      submit_on_enter: false,
      size: Size::Sm,
      disabled: false,
    }
  }

  pub fn value(mut self, value: &str) -> Self {
    self.edit = TextEdit::new(value);
    self
  }

  pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
    self.placeholder = placeholder.into();
    self
  }

  /// Replace the placeholder after construction, for a field that is built
  /// once and re-labelled later.
  pub fn set_placeholder(&mut self, placeholder: impl Into<SharedString>, cx: &mut Context<Self>) {
    self.placeholder = placeholder.into();
    cx.notify();
  }

  pub fn label(mut self, label: impl Into<SharedString>) -> Self {
    self.label = Some(label.into());
    self
  }

  pub fn description(mut self, description: impl Into<SharedString>) -> Self {
    self.description = Some(description.into());
    self
  }

  pub fn error(mut self, error: impl Into<SharedString>) -> Self {
    self.error = Some(error.into());
    self
  }

  /// Stop growing past `rows` and scroll instead. Without this a field that
  /// grows with its content has no ceiling, which is wrong for a composer
  /// pinned to the bottom of a window.
  pub fn max_rows(mut self, rows: usize) -> Self {
    self.max_rows = Some(rows.max(1));
    self
  }

  /// Make Enter commit the value (emitting [`TextAreaSubmit`]) and
  /// Shift+Enter insert the newline — the convention a chat composer uses.
  pub fn submit_on_enter(mut self, submit: bool) -> Self {
    self.submit_on_enter = submit;
    self
  }

  /// Minimum visible rows (sets the field's minimum height).
  pub fn rows(mut self, rows: usize) -> Self {
    self.rows = rows.max(1);
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

  /// The field's focus handle, so a host can focus it on open.
  pub fn focus_handle(&self) -> FocusHandle {
    self.focus.clone()
  }

  pub fn text(&self) -> String {
    self.edit.text()
  }

  /// Whether the field holds nothing but whitespace. Cheaper than
  /// `text().trim().is_empty()`, which builds and throws away a copy of the
  /// whole value — and callers ask this every frame to enable a send button.
  pub fn is_blank(&self) -> bool {
    self.edit.chars().iter().all(|c| c.is_whitespace())
  }

  pub fn set_text(&mut self, value: &str, cx: &mut Context<Self>) {
    self.edit = TextEdit::new(value);
    cx.notify();
  }

  /// Two-way bind this field's text to a `Signal<String>`. The signal is
  /// the source of truth: the field adopts its value now, edits write back
  /// through [`Signal::set_if_changed`], and signal writes replace the text.
  /// Equality guards on both directions prevent update loops.
  pub fn bind(entity: &Entity<TextArea>, signal: &Signal<String>, cx: &mut App) {
    let initial = signal.get(cx);
    entity.update(cx, |this, cx| {
      if this.text() != initial {
        this.set_text(&initial, cx);
      }
    });
    let sink = signal.clone();
    cx.subscribe(entity, move |_area, event: &TextAreaEvent, cx| {
      sink.set_if_changed(cx, event.0.clone());
    })
    .detach();
    let area = entity.downgrade();
    cx.observe(signal.entity(), move |observed, cx| {
      let value = observed.read(cx).clone();
      area
        .update(cx, |this, cx| {
          if this.text() != value {
            this.set_text(&value, cx);
          }
        })
        .ok();
    })
    .detach();
  }

  fn copy(&self, cx: &mut Context<Self>) {
    if let Some(text) = self.edit.selected_text() {
      cx.write_to_clipboard(ClipboardItem::new_string(text));
    }
    cx.stop_propagation();
  }

  fn cut(&mut self, cx: &mut Context<Self>) {
    if let Some(text) = self.edit.selected_text() {
      cx.write_to_clipboard(ClipboardItem::new_string(text));
      self.edit.delete_selection();
      cx.emit(TextAreaEvent(self.edit.text()));
      cx.notify();
    }
    cx.stop_propagation();
  }

  fn paste(&mut self, cx: &mut Context<Self>) {
    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
      self
        .edit
        .insert(&text.replace("\r\n", "\n").replace('\r', "\n"));
      cx.emit(TextAreaEvent(self.edit.text()));
      cx.notify();
    }
    cx.stop_propagation();
  }

  fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
    if self.disabled {
      return;
    }
    let ks = &event.keystroke;
    let m = &ks.modifiers;
    if m.platform && !m.alt && !m.control {
      match ks.key.as_str() {
        "a" => {
          self.edit.select_all();
          cx.notify();
          cx.stop_propagation();
          return;
        }
        "c" => return self.copy(cx),
        "x" => return self.cut(cx),
        "v" => return self.paste(cx),
        "z" | "y" => {
          let undo = ks.key == "z" && !m.shift;
          let changed = if undo {
            self.edit.undo()
          } else {
            self.edit.redo()
          };
          if changed {
            cx.emit(TextAreaEvent(self.edit.text()));
          }
          cx.notify();
          cx.stop_propagation();
          return;
        }
        _ => {}
      }
    }
    // Tab moves to the next field, as it does in a `<textarea>` — a text
    // area is still a form control, not a code editor. Escape bubbles so
    // the host can dismiss. (Enter inserts a newline: this is multi-line.)
    if ks.key == "tab" && !m.platform && !m.control {
      if m.shift {
        window.focus_prev(cx);
      } else {
        window.focus_next(cx);
      }
      cx.notify();
      cx.stop_propagation();
      return;
    }
    if ks.key == "escape" {
      return;
    }
    let edited = match ks.key.as_str() {
      "enter" if self.submit_on_enter && !m.shift => {
        cx.emit(TextAreaSubmit(self.edit.text()));
        cx.notify();
        cx.stop_propagation();
        return;
      }
      "enter" => {
        self.edit.insert("\n");
        true
      }
      "left" => {
        if !m.shift && !m.platform && !m.alt && self.edit.collapse_selection_start() {
          true
        } else {
          self.edit.pre_move(m.shift);
          if m.platform {
            self.edit.line_home();
          } else if m.alt {
            self.edit.word_left();
          } else {
            self.edit.left();
          }
          true
        }
      }
      "right" => {
        if !m.shift && !m.platform && !m.alt && self.edit.collapse_selection_end() {
          true
        } else {
          self.edit.pre_move(m.shift);
          if m.platform {
            self.edit.line_end();
          } else if m.alt {
            self.edit.word_right();
          } else {
            self.edit.right();
          }
          true
        }
      }
      "up" => {
        self.edit.pre_move(m.shift);
        if m.platform {
          self.edit.home();
        } else {
          self.edit.up();
        }
        true
      }
      "down" => {
        self.edit.pre_move(m.shift);
        if m.platform {
          self.edit.end();
        } else {
          self.edit.down();
        }
        true
      }
      "home" => {
        self.edit.pre_move(m.shift);
        self.edit.line_home();
        true
      }
      "end" => {
        self.edit.pre_move(m.shift);
        self.edit.line_end();
        true
      }
      "backspace" => {
        if m.platform {
          self.edit.delete_to_start();
        } else if m.alt {
          self.edit.delete_word_back();
        } else {
          self.edit.backspace();
        }
        true
      }
      "delete" => {
        if m.platform {
          self.edit.delete_to_end();
        } else if m.alt {
          self.edit.delete_word_forward();
        } else {
          self.edit.delete();
        }
        true
      }
      "k" if m.control => {
        self.edit.delete_to_end();
        true
      }
      "a" if m.control => {
        self.edit.home();
        true
      }
      "e" if m.control => {
        self.edit.end();
        true
      }
      _ => {
        if !m.platform && !m.control {
          if let Some(text) = ks.key_char.as_deref().filter(|t| !t.is_empty()) {
            self.edit.insert(text);
            true
          } else {
            false
          }
        } else {
          false
        }
      }
    };
    if edited {
      cx.emit(TextAreaEvent(self.edit.text()));
      cx.notify();
      cx.stop_propagation();
    }
  }
}

impl Render for TextArea {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let t = theme(cx);
    let (_, pad_x, font) = control_metrics(self.size);
    let radius = t.radius(t.default_radius);
    let focused = self.focus.is_focused(window) && !self.disabled;
    let line_h = font * 1.5;
    let pad_y = 8.0;
    let min_h = self.rows as f32 * line_h + pad_y * 2.0;
    let max_h = self
      .max_rows
      .map(|rows| rows as f32 * line_h + pad_y * 2.0)
      .filter(|max| *max >= min_h);

    let border = if self.error.is_some() {
      t.color(ColorName::Red, 6)
    } else if focused {
      t.primary()
    } else {
      t.border()
    }
    .hsla();
    let text_color = t.text().hsla();
    let dimmed = t.dimmed().hsla();
    let surface = t.surface().hsla();
    let caret = t.primary().hsla();
    let selection_bg = t.selection();

    let mut body = div().flex().flex_col().text_color(text_color);
    // A focused-but-empty field still shows its placeholder, the way a
    // `<textarea>` does — hiding it the moment the caret lands takes the
    // label away exactly when the user is deciding what to write. The
    // caret is drawn beside it.
    if focused && self.edit.is_empty() && !self.placeholder.is_empty() {
      body = body.child(
        div()
          .flex()
          .flex_wrap()
          .items_center()
          .w_full()
          .min_h(px(line_h))
          .child(div().w(px(1.0)).h(px(font * 1.15)).bg(caret))
          .child(
            div()
              .flex_1()
              .min_w(px(0.0))
              .text_color(dimmed)
              .child(self.placeholder.clone()),
          ),
      );
    } else if focused {
      if let Some((before, selected, after)) = self.edit.split_selection() {
        let before_lines: Vec<&str> = before.split('\n').collect();
        let selected_lines: Vec<&str> = selected.split('\n').collect();
        let after_lines: Vec<&str> = after.split('\n').collect();
        let before_last = before_lines.len() - 1;
        for text in &before_lines[..before_last] {
          body = body.child(div().w_full().min_h(px(line_h)).child(line(text)));
        }
        let selected_part = |text: &str| div().bg(selection_bg).rounded(px(2.0)).child(line(text));
        body = body.child(
          div()
            .flex()
            .flex_wrap()
            .items_center()
            .w_full()
            .min_h(px(line_h))
            .child(SharedString::from(before_lines[before_last].to_string()))
            .child(selected_part(selected_lines[0]))
            .when(selected_lines.len() == 1, |row| {
              row.child(SharedString::from(after_lines[0].to_string()))
            }),
        );
        if selected_lines.len() > 1 {
          for text in &selected_lines[1..selected_lines.len() - 1] {
            body = body.child(
              div()
                .flex()
                .flex_wrap()
                .w_full()
                .min_h(px(line_h))
                .child(selected_part(text)),
            );
          }
          body = body.child(
            div()
              .flex()
              .flex_wrap()
              .items_center()
              .w_full()
              .min_h(px(line_h))
              .child(selected_part(selected_lines[selected_lines.len() - 1]))
              .child(SharedString::from(after_lines[0].to_string())),
          );
        }
        for text in &after_lines[1..] {
          body = body.child(div().w_full().min_h(px(line_h)).child(line(text)));
        }
      } else {
        let (before, after) = self.edit.split();
        let before_lines: Vec<&str> = before.split('\n').collect();
        let after_lines: Vec<&str> = after.split('\n').collect();
        let last = before_lines.len() - 1;
        for text in &before_lines[..last] {
          body = body.child(div().w_full().min_h(px(line_h)).child(line(text)));
        }
        body = body.child(
          div()
            .flex()
            .flex_wrap()
            .items_center()
            .w_full()
            .min_h(px(line_h))
            .child(SharedString::from(before_lines[last].to_string()))
            .child(div().w(px(1.0)).h(px(font * 1.15)).bg(caret))
            .child(SharedString::from(after_lines[0].to_string())),
        );
        for text in &after_lines[1..] {
          body = body.child(div().w_full().min_h(px(line_h)).child(line(text)));
        }
      }
    } else if self.edit.is_empty() {
      body = body.text_color(dimmed).child(
        div()
          .w_full()
          .min_h(px(line_h))
          .child(self.placeholder.clone()),
      );
    } else {
      for l in self.edit.text().split('\n') {
        body = body.child(div().w_full().min_h(px(line_h)).child(line(l)));
      }
    }

    let mut field = div()
      .id("guise-textarea")
      .track_focus(&self.focus)
      .on_key_down(cx.listener(Self::on_key))
      .on_mouse_down(
        MouseButton::Left,
        cx.listener(|this, _ev, window, cx| {
          window.focus(&this.focus, cx);
          cx.notify();
        }),
      )
      .flex()
      .items_start()
      .overflow_x_hidden()
      .min_h(px(min_h))
      .w_full()
      .px(px(pad_x))
      .py(px(pad_y))
      .rounded(px(radius))
      .border_1()
      .border_color(border)
      .bg(surface)
      .text_size(px(font))
      .line_height(px(line_h))
      .child(div().w_full().min_w(px(0.0)).child(body));

    if let Some(max) = max_h {
      field = field
        .max_h(px(max))
        .overflow_y_scroll()
        .track_scroll(&self.scroll);
    }

    let mut chrome = Field::new().child(if self.disabled {
      field.opacity(0.6)
    } else {
      field
    });
    if let Some(label) = self.label.clone() {
      chrome = chrome.label(label);
    }
    if let Some(error) = self.error.clone() {
      chrome = chrome.error(error);
    } else if let Some(description) = self.description.clone() {
      chrome = chrome.description(description);
    }
    chrome.probe_any("TextArea")
  }
}
