//! `UpdateNotice` — the answer to a check that had nothing to install
//! (gpui entity).
//!
//! A panel rather than a desktop notification: a notification is silently dropped
//! when the user has denied the app permission to post one, and a "Check for
//! Updates…" that appears to do nothing at all is worse than the answer being
//! unwelcome. [`UpdatePrompt`](super::UpdatePrompt) already works this way; this
//! is the other half of it.

use gpui::prelude::*;
use gpui::{
  div, px, App, Context, EventEmitter, FocusHandle, Focusable, FontWeight, IntoElement,
  KeyDownEvent, MouseButton, SharedString, Window, WindowControlArea,
};

use super::Updater;
use crate::devtools::Probed;
use crate::theme::{theme, Size};
use crate::{Button, Variant};

/// Height of the strip that drags the window, and the padding that clears a
/// transparent titlebar. A platform metric, not a themed one.
const TITLEBAR: f32 = 34.0;

/// The outcome of a check that has nothing to install — everything the prompt
/// cannot represent, because it exists to run an install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome {
  /// Nothing newer is published.
  UpToDate,
  /// Something newer exists but hasn't uploaded what this machine installs.
  Pending(String),
  /// The check itself failed (offline, the host down, a parse error).
  Failed(String),
}

impl UpdateOutcome {
  /// Headline and detail. Both are needed: "You're up to date" alone leaves a
  /// user who half-expected an update wondering whether the check ran at all.
  pub fn lines(&self, app: &str, current: &str) -> (String, String) {
    match self {
      UpdateOutcome::UpToDate => (
        "You're up to date".to_string(),
        format!("{app} {current} is the latest version."),
      ),
      UpdateOutcome::Pending(version) => (
        format!("{app} {version} is on the way"),
        "It is still building for this platform. Check again shortly.".to_string(),
      ),
      UpdateOutcome::Failed(why) => ("Couldn't check for updates".to_string(), why.clone()),
    }
  }
}

/// Emitted when the notice is done with.
#[derive(Debug, Clone)]
pub enum UpdateNoticeEvent {
  /// The user acknowledged it (the button or Escape). Whoever owns the window
  /// closes it.
  Dismissed,
}

/// The short answer to a manual update check.
pub struct UpdateNotice {
  updater: Updater,
  outcome: UpdateOutcome,
  window_root: bool,
  focus: FocusHandle,
}

impl UpdateNotice {
  pub fn new(updater: Updater, outcome: UpdateOutcome, cx: &mut Context<Self>) -> Self {
    UpdateNotice {
      updater,
      outcome,
      window_root: false,
      focus: cx.focus_handle(),
    }
  }

  /// Whether this notice is the root view of its own window: draws the titlebar
  /// drag strip and pads for a transparent titlebar. [`super::check_now`] sets
  /// it; leave it off when embedding the notice in a window of your own.
  pub fn window_root(mut self, window_root: bool) -> Self {
    self.window_root = window_root;
    self
  }

  /// What the check found.
  pub fn outcome(&self) -> &UpdateOutcome {
    &self.outcome
  }

  /// Acknowledge the notice.
  pub fn dismiss(&mut self, cx: &mut Context<Self>) {
    cx.emit(UpdateNoticeEvent::Dismissed);
  }

  fn key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
    if event.keystroke.key == "escape" {
      self.dismiss(cx);
    }
  }
}

impl Focusable for UpdateNotice {
  fn focus_handle(&self, _cx: &App) -> FocusHandle {
    self.focus.clone()
  }
}

impl EventEmitter<UpdateNoticeEvent> for UpdateNotice {}

impl Render for UpdateNotice {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let t = theme(cx);
    let bg = t.body().hsla();
    let text = t.text().hsla();
    let dim = t.dimmed().hsla();
    let pad = t.spacing(Size::Lg);
    let gap = t.spacing(Size::Xs);
    let headline_size = t.font_size(Size::Md);
    let small = t.font_size(Size::Xs);
    let (headline, detail) = self
      .outcome
      .lines(self.updater.app(), self.updater.version());

    div()
      .size_full()
      .flex()
      .flex_col()
      .track_focus(&self.focus)
      .on_key_down(cx.listener(Self::key_down))
      .bg(bg)
      .text_color(text)
      .pt(px(if self.window_root { TITLEBAR } else { pad }))
      .px(px(pad))
      .pb(px(pad))
      .gap(px(gap))
      .when(self.window_root, |this| this.child(drag_strip()))
      .child(
        div()
          .text_size(px(headline_size))
          .font_weight(FontWeight::BOLD)
          .child(SharedString::from(headline)),
      )
      .child(
        div()
          .text_size(px(small))
          .text_color(dim)
          .child(SharedString::from(detail)),
      )
      .child(div().flex_1())
      .child(
        div().flex().items_center().justify_end().child(
          Button::new("guise-update-ok", "OK")
            .variant(Variant::Filled)
            .on_click(cx.listener(|this, _, _, cx| this.dismiss(cx))),
        ),
      )
      .probe("UpdateNotice")
  }
}

/// The strip along the top of an update window that drags it, kept clear of the
/// macOS traffic lights.
fn drag_strip() -> impl IntoElement {
  let lead = if cfg!(target_os = "macos") { 70.0 } else { 0.0 };
  div()
    .absolute()
    .top_0()
    .left(px(lead))
    .right_0()
    .h(px(TITLEBAR - 6.0))
    .window_control_area(WindowControlArea::Drag)
    .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Every outcome names both what happened and why. A headline alone leaves
  /// someone who half-expected an update unsure the check even ran.
  #[test]
  fn every_outcome_says_what_happened_and_why() {
    for outcome in [
      UpdateOutcome::UpToDate,
      UpdateOutcome::Pending("1.32.0".into()),
      UpdateOutcome::Failed("network unreachable".into()),
    ] {
      let (headline, detail) = outcome.lines("Acme", "1.31.0");
      assert!(!headline.trim().is_empty());
      assert!(!detail.trim().is_empty());
    }
  }

  #[test]
  fn up_to_date_names_the_version_you_are_on() {
    let (_, detail) = UpdateOutcome::UpToDate.lines("Acme", "1.31.0");
    assert!(detail.contains("1.31.0"), "{detail}");
    assert!(detail.contains("Acme"), "{detail}");
  }

  /// A release still uploading must not read as "up to date" — that is the case
  /// [`UpdateOutcome::Pending`] exists to distinguish.
  #[test]
  fn a_pending_release_is_not_reported_as_up_to_date() {
    let (headline, detail) = UpdateOutcome::Pending("1.32.0".into()).lines("Acme", "1.31.0");
    assert!(headline.contains("1.32.0"), "{headline}");
    assert!(!headline.contains("up to date"), "{headline}");
    assert!(detail.contains("building"), "{detail}");
  }

  /// A failed check reports the reason rather than a generic apology.
  #[test]
  fn a_failed_check_surfaces_its_reason() {
    let (_, detail) = UpdateOutcome::Failed("network unreachable".into()).lines("Acme", "1.31.0");
    assert_eq!(detail, "network unreachable");
  }
}
