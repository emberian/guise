//! `WindowControls` and `ResizeHandles` — the chrome an app has to draw itself
//! when the platform will not.
//!
//! On macOS and Windows the OS draws the close/minimise/zoom buttons and owns
//! the resize border. On Linux, a client-side-decorated window draws both or
//! goes without: no buttons, and edges that cannot be dragged.
//!
//! Ported from sinclair, which needed exactly this and nothing more. Both
//! components render on every platform if you ask them to — you decide, not a
//! `cfg` inside the library, because a `cfg!(target_os)` buried in a component
//! is impossible to preview from the other side. [`WindowControls::platform`]
//! is the convenience for the usual case: draw them only where the OS doesn't.

use gpui::prelude::*;
use gpui::{div, px, App, IntoElement, MouseButton, SharedString, Window, WindowControlArea};

use crate::devtools::Probed;
use crate::theme::theme;

/// Minimise / maximise / close, for a window the app decorates itself.
#[derive(IntoElement)]
pub struct WindowControls {
  width: f32,
  height: f32,
}

impl Default for WindowControls {
  fn default() -> Self {
    WindowControls::new()
  }
}

impl WindowControls {
  pub fn new() -> Self {
    WindowControls {
      width: 46.0,
      height: 28.0,
    }
  }

  /// Whether this platform leaves the buttons to the app.
  ///
  /// True on Linux, false where the OS draws its own. Use it to decide
  /// whether to render at all:
  /// `.children(WindowControls::needed().then(WindowControls::new))`.
  pub fn needed() -> bool {
    cfg!(target_os = "linux")
  }

  /// Width of one button (default 46px).
  pub fn button_width(mut self, width: f32) -> Self {
    self.width = width;
    self
  }

  /// Height of the strip (default 28px).
  pub fn height(mut self, height: f32) -> Self {
    self.height = height;
    self
  }
}

impl RenderOnce for WindowControls {
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let t = theme(cx);
    let fg = t.text().hsla();
    let dim = fg.opacity(0.6);
    let hover = fg.opacity(0.12);
    let (width, height) = (self.width, self.height);

    let button = move |id: &'static str, glyph: &'static str| {
      div()
        .id(id)
        .w(px(width))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .text_color(dim)
        .hover(move |st| st.bg(hover).text_color(fg))
        .child(SharedString::new_static(glyph))
    };

    div()
      .flex()
      .items_center()
      .flex_none()
      .h(px(height))
      .child(
        button("guise-window-min", "\u{2013}")
          .window_control_area(WindowControlArea::Min)
          .on_click(|_, window, _| window.minimize_window()),
      )
      .child(
        button("guise-window-max", "\u{25a1}")
          .window_control_area(WindowControlArea::Max)
          .on_click(|_, window, _| window.zoom_window()),
      )
      .child(
        button("guise-window-close", "\u{2715}")
          .window_control_area(WindowControlArea::Close)
          .on_click(|_, window, _| window.remove_window()),
      )
      .probe("WindowControls")
  }
}

/// Invisible edge and corner hit-zones that start a window resize.
///
/// Absolutely positioned over the whole window, inert in the middle, so it
/// never swallows a click meant for the app. Put it last in the root element so
/// the edges sit above the content.
#[derive(IntoElement)]
pub struct ResizeHandles {
  edge: f32,
  corner: f32,
}

impl Default for ResizeHandles {
  fn default() -> Self {
    ResizeHandles::new()
  }
}

impl ResizeHandles {
  pub fn new() -> Self {
    ResizeHandles {
      edge: 6.0,
      corner: 12.0,
    }
  }

  /// Whether this platform leaves the resize border to the app.
  pub fn needed() -> bool {
    cfg!(target_os = "linux")
  }

  /// Thickness of the edge strips (default 6px).
  pub fn edge(mut self, edge: f32) -> Self {
    self.edge = edge;
    self
  }

  /// Size of the corner squares (default 12px). Corners are larger because
  /// they steer two axes at once and are the harder target to hit.
  pub fn corner(mut self, corner: f32) -> Self {
    self.corner = corner;
    self
  }
}

impl RenderOnce for ResizeHandles {
  fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
    use gpui::ResizeEdge;

    let zone = |id: &'static str, edge: ResizeEdge| {
      div()
        .id(id)
        .absolute()
        .on_mouse_down(MouseButton::Left, move |_, window, _| {
          window.start_window_resize(edge);
        })
    };
    let (t, c) = (px(self.edge), px(self.corner));

    div()
      .absolute()
      .inset_0()
      .child(
        zone("guise-resize-t", ResizeEdge::Top)
          .top_0()
          .left_0()
          .right_0()
          .h(t),
      )
      .child(
        zone("guise-resize-b", ResizeEdge::Bottom)
          .bottom_0()
          .left_0()
          .right_0()
          .h(t),
      )
      .child(
        zone("guise-resize-l", ResizeEdge::Left)
          .top_0()
          .bottom_0()
          .left_0()
          .w(t),
      )
      .child(
        zone("guise-resize-r", ResizeEdge::Right)
          .top_0()
          .bottom_0()
          .right_0()
          .w(t),
      )
      .child(
        zone("guise-resize-tl", ResizeEdge::TopLeft)
          .top_0()
          .left_0()
          .w(c)
          .h(c),
      )
      .child(
        zone("guise-resize-tr", ResizeEdge::TopRight)
          .top_0()
          .right_0()
          .w(c)
          .h(c),
      )
      .child(
        zone("guise-resize-bl", ResizeEdge::BottomLeft)
          .bottom_0()
          .left_0()
          .w(c)
          .h(c),
      )
      .child(
        zone("guise-resize-br", ResizeEdge::BottomRight)
          .bottom_0()
          .right_0()
          .w(c)
          .h(c),
      )
      .probe("ResizeHandles")
  }
}

/// Clearance to reserve at the leading edge of a custom titlebar so the macOS
/// traffic lights are not overlapped.
///
/// Zero where the OS draws no inset. sinclair's pane-group tab bar doubles as
/// its titlebar and reserves exactly this.
pub const TRAFFIC_LIGHT_INSET: f32 = if cfg!(target_os = "macos") { 88.0 } else { 0.0 };
