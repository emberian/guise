# GPU View

`GpuView` renders an app-owned scene through gpui's native GPU paint pipeline.
It is intended for maps, simulations, diagrams, sprite-heavy status surfaces,
and other worlds that are easier to model as logical coordinates than as a
tree of controls. It does not embed a browser, WebGL canvas, or platform view.

The caller owns state and time. Build a lightweight `GpuScene` from current
application state and notify the parent entity when it changes; Guise maps the
scene into the component bounds and gpui batches its quads and textures into the
window's normal render pass.

```rust
let floor = GpuTexture::png(include_bytes!("assets/floor.png").to_vec());
let workers = GpuTexture::png(include_bytes!("assets/workers.png").to_vec());
let scene = GpuScene::new(1536.0, 1024.0)
    .background(floor)
    .sprite(
        workers,
        GpuRect::new(frame as f32 * 0.25, worker_row as f32 * 0.25, 0.25, 0.25),
        GpuRect::new(x - 54.0, y - 96.0, 108.0, 108.0),
    )
    .bordered_rect(
        GpuRect::new(120.0, 160.0, 24.0, 24.0),
        theme(cx).primary().hsla(),
        theme(cx).border().hsla(),
        3.0,
        4.0,
    );

GpuView::new(scene)
    .fit(GpuFit::Cover)
    .height(360.0)
    .pixelated()
```

## Scene coordinates

`GpuScene::new(width, height)` defines a logical coordinate space independent
of the window scale. Commands paint in insertion order:

- `rect` adds a filled rectangle.
- `rounded_rect` adds a filled rectangle with a radius.
- `bordered_rect` adds a filled, solid-bordered rectangle.
- `texture` maps encoded image data to a scene rectangle.
- `sprite` maps a normalized region of a texture to a scene rectangle. Source `x`,
  `y`, `width`, and `height` are clamped to `0.0..=1.0`, making atlas frame selection
  independent of the source image's pixel dimensions.
- `background` maps a texture to the full scene.

`GpuTexture` accepts PNG, JPEG, or WebP bytes. gpui decodes the image through
its asset cache and uploads it to the sprite atlas; clones retain the same data
without duplicating the encoded buffer.

## Fitting and sizing

`GpuFit::Contain` preserves the whole scene and letterboxes it. `Cover` fills
the component and clips excess scene area. `Stretch` maps the axes
independently. The view is full width and 240 px high by default; use `width`,
`height`, or `full_height` to fit its parent.

`.pixelated()` snaps transformed command bounds to physical pixel boundaries.
It is useful for tiled maps and crisp low-resolution art; it does not alter the
source image or impose a particular texture filtering algorithm.

## Animation

`GpuView` is deliberately stateless. Store positions, velocities, selections, frame
indices, and time in the parent entity, rebuild the scene, then call `cx.notify()`.
Use `sprite` to select the current atlas cell without decoding another texture. The
application should pause its clock when the surface is hidden or its window is inactive,
and should choose a stable frame when reduced motion is enabled. This keeps simulation
and accessibility ownership explicit; `GpuView` itself never starts background work.
