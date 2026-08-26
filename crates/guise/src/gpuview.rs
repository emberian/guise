//! `GpuView` — a retained scene surface painted by gpui's GPU renderer.
//!
//! `GpuView` is for app-owned worlds that are more naturally expressed as a
//! scene than a tree of controls: maps, editors, diagrams, simulations, and
//! sprite-heavy status views. It keeps the scene API small, leaves animation
//! state with the caller, and submits quads and textures through gpui's native
//! paint pipeline. There is no web canvas or embedded browser involved.

use std::sync::Arc;

use gpui::prelude::*;
use gpui::{
  canvas, fill, px, quad, size, App, BorderStyle, Bounds, ContentMask, Corners, Edges, Hsla,
  ImageFormat, IntoElement, Pixels, RenderImage, Window,
};

use crate::devtools::Probed;
use crate::theme::theme;

/// How a scene's logical coordinate space maps into the view bounds.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GpuFit {
  /// Preserve aspect ratio and show the whole scene, letterboxing as needed.
  #[default]
  Contain,
  /// Preserve aspect ratio and fill the view, clipping the excess.
  Cover,
  /// Scale each axis independently to fill the view.
  Stretch,
}

/// A rectangle in the scene's logical coordinate space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GpuRect {
  pub x: f32,
  pub y: f32,
  pub width: f32,
  pub height: f32,
}

impl GpuRect {
  pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
    Self {
      x,
      y,
      width,
      height,
    }
  }
}

/// Encoded image data retained by a [`GpuScene`].
///
/// gpui decodes it through the normal asset cache and uploads the result to
/// its sprite atlas. Cloning a texture is cheap.
#[derive(Clone, Debug)]
pub struct GpuTexture {
  image: Arc<gpui::Image>,
}

impl GpuTexture {
  pub fn from_encoded(format: ImageFormat, bytes: impl Into<Vec<u8>>) -> Self {
    Self {
      image: Arc::new(gpui::Image::from_bytes(format, bytes.into())),
    }
  }

  pub fn png(bytes: impl Into<Vec<u8>>) -> Self {
    Self::from_encoded(ImageFormat::Png, bytes)
  }

  pub fn jpeg(bytes: impl Into<Vec<u8>>) -> Self {
    Self::from_encoded(ImageFormat::Jpeg, bytes)
  }

  pub fn webp(bytes: impl Into<Vec<u8>>) -> Self {
    Self::from_encoded(ImageFormat::Webp, bytes)
  }
}

#[derive(Clone, Debug)]
struct GpuQuad {
  bounds: GpuRect,
  fill: Hsla,
  radius: f32,
  border_width: f32,
  border: Hsla,
}

#[derive(Clone, Debug)]
enum GpuCommand {
  Quad(GpuQuad),
  Texture {
    texture: GpuTexture,
    bounds: GpuRect,
    source: Option<GpuRect>,
  },
}

/// A retained list of GPU-friendly drawing commands in a logical coordinate
/// space. Build or update it with application state, then pass it to
/// [`GpuView`]. Commands paint in insertion order.
#[derive(Clone, Debug)]
pub struct GpuScene {
  width: f32,
  height: f32,
  commands: Vec<GpuCommand>,
}

impl GpuScene {
  pub fn new(width: f32, height: f32) -> Self {
    Self {
      width: finite_positive(width),
      height: finite_positive(height),
      commands: Vec::new(),
    }
  }

  pub fn size(&self) -> (f32, f32) {
    (self.width, self.height)
  }

  pub fn len(&self) -> usize {
    self.commands.len()
  }

  pub fn is_empty(&self) -> bool {
    self.commands.is_empty()
  }

  /// Append a filled rectangle.
  pub fn rect(mut self, bounds: GpuRect, color: Hsla) -> Self {
    self.push_rect(bounds, color);
    self
  }

  /// Append a filled rounded rectangle.
  pub fn rounded_rect(mut self, bounds: GpuRect, color: Hsla, radius: f32) -> Self {
    self.commands.push(GpuCommand::Quad(GpuQuad {
      bounds: sane_rect(bounds),
      fill: color,
      radius: finite_nonnegative(radius),
      border_width: 0.0,
      border: gpui::transparent_black(),
    }));
    self
  }

  /// Append a filled rectangle with a solid border.
  pub fn bordered_rect(
    mut self,
    bounds: GpuRect,
    fill: Hsla,
    border: Hsla,
    border_width: f32,
    radius: f32,
  ) -> Self {
    self.commands.push(GpuCommand::Quad(GpuQuad {
      bounds: sane_rect(bounds),
      fill,
      radius: finite_nonnegative(radius),
      border_width: finite_nonnegative(border_width),
      border,
    }));
    self
  }

  pub fn push_rect(&mut self, bounds: GpuRect, color: Hsla) {
    self.commands.push(GpuCommand::Quad(GpuQuad {
      bounds: sane_rect(bounds),
      fill: color,
      radius: 0.0,
      border_width: 0.0,
      border: gpui::transparent_black(),
    }));
  }

  /// Append a texture mapped over the supplied scene rectangle.
  pub fn texture(mut self, texture: GpuTexture, bounds: GpuRect) -> Self {
    self.push_texture(texture, bounds);
    self
  }

  /// Draw one normalized region of a texture into a scene rectangle.
  ///
  /// `source` uses `0.0..=1.0` texture coordinates. This keeps sprite-sheet
  /// animation app-owned: select a frame while rebuilding the scene, without
  /// creating a new texture or decoding the atlas again.
  pub fn sprite(mut self, texture: GpuTexture, source: GpuRect, bounds: GpuRect) -> Self {
    self.push_sprite(texture, source, bounds);
    self
  }

  /// Append a texture covering the scene's full logical bounds.
  pub fn background(self, texture: GpuTexture) -> Self {
    let bounds = GpuRect::new(0.0, 0.0, self.width, self.height);
    self.texture(texture, bounds)
  }

  pub fn push_texture(&mut self, texture: GpuTexture, bounds: GpuRect) {
    self.commands.push(GpuCommand::Texture {
      texture,
      bounds: sane_rect(bounds),
      source: None,
    });
  }

  pub fn push_sprite(&mut self, texture: GpuTexture, source: GpuRect, bounds: GpuRect) {
    self.commands.push(GpuCommand::Texture {
      texture,
      bounds: sane_rect(bounds),
      source: Some(sane_source_rect(source)),
    });
  }
}

/// A stateless scene component backed by gpui's native GPU paint pipeline.
///
/// The caller owns simulation and animation state. Rebuild the lightweight
/// [`GpuScene`] and notify the parent entity when a frame changes.
#[derive(IntoElement)]
pub struct GpuView {
  scene: GpuScene,
  fit: GpuFit,
  width: Option<f32>,
  height: Option<f32>,
  background: Option<Hsla>,
  pixel_snap: bool,
}

impl GpuView {
  pub fn new(scene: GpuScene) -> Self {
    Self {
      scene,
      fit: GpuFit::Contain,
      width: None,
      height: Some(240.0),
      background: None,
      pixel_snap: false,
    }
  }

  pub fn fit(mut self, fit: GpuFit) -> Self {
    self.fit = fit;
    self
  }

  pub fn width(mut self, width: f32) -> Self {
    self.width = Some(finite_positive(width));
    self
  }

  pub fn height(mut self, height: f32) -> Self {
    self.height = Some(finite_positive(height));
    self
  }

  /// Stretch to the parent's available height instead of using a fixed one.
  pub fn full_height(mut self) -> Self {
    self.height = None;
    self
  }

  /// Color behind uncovered areas. Defaults to the theme surface.
  pub fn background(mut self, color: Hsla) -> Self {
    self.background = Some(color);
    self
  }

  /// Snap transformed command bounds to physical pixel boundaries. Useful
  /// for tile maps and pixel art.
  pub fn pixelated(mut self) -> Self {
    self.pixel_snap = true;
    self
  }
}

impl RenderOnce for GpuView {
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let background = self
      .background
      .unwrap_or_else(|| theme(cx).surface().hsla());
    let scene = Arc::new(self.scene);
    let prepaint_scene = scene.clone();
    let fit = self.fit;
    let pixel_snap = self.pixel_snap;

    let mut surface = canvas(
      move |_, window, cx| {
        prepaint_scene
          .commands
          .iter()
          .filter_map(|command| match command {
            GpuCommand::Texture { texture, .. } => {
              Some(texture.image.clone().use_render_image(window, cx))
            }
            GpuCommand::Quad(_) => None,
          })
          .collect::<Vec<Option<Arc<RenderImage>>>>()
      },
      move |bounds, images, window, _cx| {
        window.paint_quad(fill(bounds, background));
        let mut images = images.iter();
        let transform = SceneTransform::new(
          scene.width,
          scene.height,
          f32::from(bounds.size.width),
          f32::from(bounds.size.height),
          fit,
        );
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
          for command in &scene.commands {
            match command {
              GpuCommand::Quad(command) => {
                let command_bounds = transform.bounds(bounds, command.bounds, pixel_snap);
                let radius = transform.radius(command.radius, pixel_snap);
                let border_width = transform.radius(command.border_width, pixel_snap);
                window.paint_quad(quad(
                  command_bounds,
                  Corners::all(px(radius)),
                  command.fill,
                  Edges::all(px(border_width)),
                  command.border,
                  BorderStyle::Solid,
                ));
              }
              GpuCommand::Texture {
                bounds: destination,
                source,
                ..
              } => {
                let Some(image) = images.next().and_then(Clone::clone) else {
                  continue;
                };
                let image_bounds = transform.bounds(bounds, *destination, pixel_snap);
                if let Some(source) = source {
                  let full_width = f32::from(image_bounds.size.width) / source.width;
                  let full_height = f32::from(image_bounds.size.height) / source.height;
                  let atlas_bounds = Bounds::new(
                    image_bounds.origin
                      - gpui::point(px(source.x * full_width), px(source.y * full_height)),
                    size(px(full_width), px(full_height)),
                  );
                  window.with_content_mask(
                    Some(ContentMask {
                      bounds: image_bounds,
                    }),
                    |window| {
                      let _ = window.paint_image(atlas_bounds, Corners::default(), image, 0, false);
                    },
                  );
                } else {
                  let _ = window.paint_image(image_bounds, Corners::default(), image, 0, false);
                }
              }
            }
          }
        });
      },
    )
    .overflow_hidden();

    surface = match self.height {
      Some(height) => surface.h(px(height)),
      None => surface.h_full(),
    };

    match self.width {
      Some(width) => surface.w(px(width)).probe("GpuView"),
      None => surface.w_full().probe("GpuView"),
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SceneTransform {
  scale_x: f32,
  scale_y: f32,
  offset_x: f32,
  offset_y: f32,
}

impl SceneTransform {
  fn new(scene_w: f32, scene_h: f32, view_w: f32, view_h: f32, fit: GpuFit) -> Self {
    let sx = finite_positive(view_w) / finite_positive(scene_w);
    let sy = finite_positive(view_h) / finite_positive(scene_h);
    let (scale_x, scale_y) = match fit {
      GpuFit::Contain => {
        let scale = sx.min(sy);
        (scale, scale)
      }
      GpuFit::Cover => {
        let scale = sx.max(sy);
        (scale, scale)
      }
      GpuFit::Stretch => (sx, sy),
    };
    Self {
      scale_x,
      scale_y,
      offset_x: (view_w - scene_w * scale_x) * 0.5,
      offset_y: (view_h - scene_h * scale_y) * 0.5,
    }
  }

  fn bounds(self, viewport: Bounds<Pixels>, source: GpuRect, pixel_snap: bool) -> Bounds<Pixels> {
    let x = self.offset_x + source.x * self.scale_x;
    let y = self.offset_y + source.y * self.scale_y;
    let width = source.width * self.scale_x;
    let height = source.height * self.scale_y;
    let snap = |value: f32| if pixel_snap { value.round() } else { value };
    Bounds::new(
      viewport.origin + gpui::point(px(snap(x)), px(snap(y))),
      size(px(snap(width)), px(snap(height))),
    )
  }

  fn radius(self, value: f32, pixel_snap: bool) -> f32 {
    let scaled = value * self.scale_x.min(self.scale_y);
    if pixel_snap {
      scaled.round()
    } else {
      scaled
    }
  }
}

fn finite_positive(value: f32) -> f32 {
  if value.is_finite() && value > 0.0 {
    value
  } else {
    1.0
  }
}

fn finite_nonnegative(value: f32) -> f32 {
  if value.is_finite() {
    value.max(0.0)
  } else {
    0.0
  }
}

fn sane_rect(rect: GpuRect) -> GpuRect {
  GpuRect {
    x: if rect.x.is_finite() { rect.x } else { 0.0 },
    y: if rect.y.is_finite() { rect.y } else { 0.0 },
    width: finite_nonnegative(rect.width),
    height: finite_nonnegative(rect.height),
  }
}

fn sane_source_rect(rect: GpuRect) -> GpuRect {
  let x = if rect.x.is_finite() {
    rect.x.clamp(0.0, 1.0 - f32::EPSILON)
  } else {
    0.0
  };
  let y = if rect.y.is_finite() {
    rect.y.clamp(0.0, 1.0 - f32::EPSILON)
  } else {
    0.0
  };
  let width = if rect.width.is_finite() {
    rect.width.clamp(f32::EPSILON, 1.0 - x)
  } else {
    1.0 - x
  };
  let height = if rect.height.is_finite() {
    rect.height.clamp(f32::EPSILON, 1.0 - y)
  } else {
    1.0 - y
  };
  GpuRect::new(x, y, width, height)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn contain_centers_the_letterboxed_scene() {
    let transform = SceneTransform::new(200.0, 100.0, 100.0, 100.0, GpuFit::Contain);
    assert_eq!(transform.scale_x, 0.5);
    assert_eq!(transform.scale_y, 0.5);
    assert_eq!(transform.offset_x, 0.0);
    assert_eq!(transform.offset_y, 25.0);
  }

  #[test]
  fn cover_centers_the_clipped_scene() {
    let transform = SceneTransform::new(200.0, 100.0, 100.0, 100.0, GpuFit::Cover);
    assert_eq!(transform.scale_x, 1.0);
    assert_eq!(transform.scale_y, 1.0);
    assert_eq!(transform.offset_x, -50.0);
    assert_eq!(transform.offset_y, 0.0);
  }

  #[test]
  fn stretch_maps_each_axis_independently() {
    let transform = SceneTransform::new(200.0, 100.0, 100.0, 100.0, GpuFit::Stretch);
    assert_eq!(transform.scale_x, 0.5);
    assert_eq!(transform.scale_y, 1.0);
    assert_eq!(transform.offset_x, 0.0);
    assert_eq!(transform.offset_y, 0.0);
  }

  #[test]
  fn scene_sanitizes_non_finite_geometry() {
    let scene = GpuScene::new(f32::NAN, 0.0).rect(
      GpuRect::new(f32::INFINITY, 2.0, -3.0, f32::NAN),
      gpui::black(),
    );
    assert_eq!(scene.size(), (1.0, 1.0));
    let GpuCommand::Quad(command) = &scene.commands[0] else {
      panic!("expected quad");
    };
    assert_eq!(command.bounds, GpuRect::new(0.0, 2.0, 0.0, 0.0));
  }

  #[test]
  fn sprite_sources_are_clamped_to_the_texture() {
    assert_eq!(
      sane_source_rect(GpuRect::new(0.75, -1.0, 0.5, f32::NAN)),
      GpuRect::new(0.75, 0.0, 0.25, 1.0)
    );
  }
}
