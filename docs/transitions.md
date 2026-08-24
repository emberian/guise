# Motion & transitions

Everything animated in guise lives in `guise::anim`, and it splits in two.

The **description** is pure. A `Motion` is keyframed tracks over a duration; a
`Sequence` puts several motions on one clock; a `Stagger` turns an index into a
delay. `sample(t)` maps a millisecond offset to a `Frame` — the properties that
have a value at that instant — with no state, no window, and nothing to tick.

The **clock** is a thin shell over it. `Animated` (or `.animate(..)`) plays a
clip once when an element mounts; `Animator` is an entity that owns a playhead
you can play, pause, reverse, scrub and re-speed. `Presence` latches an element
through an *exit* animation before it unmounts. `Transition` and `Collapse` are
the older, narrower wrappers over the same curves, and still the shortest path
to a fade or a reveal.

The shape is anime.js's — timing on the animation, per-keyframe overrides,
timelines, stagger, playback controls — with the vocabulary Rust and gpui
actually have.

> gpui has no transform matrix on an element, so there is no `translate` or
> `scale` to animate. Motion is expressed through opacity, the relative inset
> (which shifts an element at paint time without disturbing its siblings — the
> closest thing to a translate), the box, and colours.

```rust
use guise::prelude::*;

Animated::new("card")
    .motion(
        Motion::new()
            .duration(420.0)
            .ease(Easing::Out(Curve::Back))
            .tween(Prop::Opacity, 0.0, 1.0)
            .tween(Prop::Y, 12.0, 0.0),
    )
    .child(card)
```

Two runnable demos, and a tutorial that builds the second one:

```sh
cargo run -p guise-ui --example motion      # the API, in one window
cargo run -p guise-ui --example checklist   # the motion tutorial's app
```

New to it? Start with the [motion tutorial](motiontutorial.md) — nine short
chapters that build one animated panel. This page is the reference.

## Easing

`Easing` is a `Copy` enum you can store on any builder. Every variant maps
normalized time 0..=1 and hits both endpoints exactly.

```rust
use guise::anim::{Easing, Spring};

Easing::EaseOutBack                      // overshoot + settle
Easing::CubicBezier(0.25, 0.1, 0.25, 1.0) // CSS "ease"
Easing::Spring(Spring::wobbly())          // physical spring
```

Variants: `Linear`, `EaseIn`, `EaseOut` (default), `EaseInOut`, `EaseInCubic`,
`EaseOutCubic`, `EaseInOutCubic`, `EaseOutQuint`, `EaseOutExpo`, `EaseOutBack`,
`EaseOutElastic`, `EaseOutBounce`, `CubicBezier(x1, y1, x2, y2)`,
`Spring(Spring)`, `Steps(n)`, and the composed family:

```rust
Easing::In(Curve::Quad)      // anime.js inQuad
Easing::Out(Curve::Elastic)  // outElastic
Easing::InOut(Curve::Sine)   // inOutSine
```

`Curve` is `Quad`, `Cubic`, `Quart`, `Quint`, `Sine`, `Expo`, `Circ`, `Back`,
`Elastic`, `Bounce` — a direction plus a shape instead of thirty variants.
`Curve::ALL` lists them for a picker.

The raw curves are plain functions in `guise::anim::ease` if you're driving
`with_animation` yourself. Two ways to get a gpui `Animation` from an
`Easing`:

- `animation(duration_ms)` — the curve installed in gpui's easing slot,
  **clamped** into `0..=1`. gpui debug-asserts easing output into that range,
  which overshooting curves (`Spring`, `EaseOutBack`, `EaseOutElastic`)
  violate by design — unclamped they abort any debug build. The clamp
  flattens overshoot peaks.
- `clock(duration_ms)` — the un-eased linear clock (springs still size it by
  `settle_seconds()`). Apply the curve yourself inside the animator, where
  overshoot is legal — this is what `Transition`/`Collapse`/`Presence` do,
  so their springs keep the full overshoot:

```rust
el.with_animation(id, easing.clock(200), move |el, t| {
    let delta = easing.apply(t);            // may pass 1.0 and settle back
    el.ml(px((1.0 - delta) * 8.0))          // offsets may overshoot
        .opacity(delta.clamp(0.0, 1.0))     // opacity must not
})
```

### Springs

`Spring { stiffness, damping }` is a closed-form damped oscillator — no
simulation loop. `damping < 2·√stiffness` overshoots and rings; more damping
approaches without crossing. Springs carry their own clock:
`settle_seconds()` says how long until it stays within 1% of the target, and
`Easing::Spring` ignores the surrounding `duration_ms` in favor of it.

Presets: `Spring::default()` (slight overshoot, fast settle),
`Spring::wobbly()` (visible ring), `Spring::stiff()` (no overshoot).

## motion! — the short way

Everything below has a builder and a macro, and the macro is usually what you
want. `motion!` is a block of declarations, the way
[`style!`](macros.md#style--css-like-style-blocks) is:

```rust
div().child(card).animate("card", motion! {
    enter: slide_up 12;
    duration: 420;
    ease: out back;
})
```

It expands to the builder, so the two mix freely and anything the block does
not cover still chains. [Macros](macros.md#motion--animation-as-a-declaration-block)
has the full grammar; the rest of this page explains what the pieces mean.

## Motion

A `Motion` is a set of **tracks**, one per property, plus shared timing. Every
track says where it starts, because there is no element to read a current value
off.

```rust
Motion::new()
    .duration(600.0)              // ms; the default each track gets
    .delay(80.0)                  // ms of stillness first
    .end_delay(120.0)             // ms of stillness after, inside the loop
    .ease(Easing::Out(Curve::Cubic))
    .tween(Prop::Opacity, 0.0, 1.0)
    .tween(Prop::Y, 12.0, 0.0)
```

`sample(ms)` returns a `Frame`. `iteration_ms()` is one pass, `total_ms()` is
all of them (`f32::INFINITY` when it loops forever).

### Properties

`Prop` names what a motion may move. The ones `Frame::apply` sets on an
element:

| Prop | Unit |
| --- | --- |
| `Opacity` | 0..=1 |
| `X`, `Y` | px, as a relative inset — the element moves, the layout does not |
| `Width`, `Height` | px |
| `MarginTop/Right/Bottom/Left`, `PadTop/…` | px |
| `Radius`, `BorderWidth`, `Gap`, `FontSize` | px |
| `Background`, `BorderColor`, `TextColor` | colours |

And three it carries for you to read out yourself: `Rotate` and `Scale` (gpui
can only transform an `Image`/`Svg`, through its own `Transformation`) and
`Custom("name")` for anything that isn't a style at all — a chart's value, a
scroll offset, a number you're counting up.

Values are numbers or colours. Colours blend the short way round the hue
wheel, so red → magenta doesn't sweep through green.

### Keyframes

More than one leg per property. A leg with no `duration` takes an even share of
whatever the motion's `duration` has left over after the fixed ones; a leg
longer than the whole motion stretches it rather than being cut off.

```rust
Motion::new()
    .duration(900.0)
    .keyframes(Prop::Y, 0.0, [
        Keyframe::to(-30.0).duration(300.0),
        Keyframe::to(0.0).ease(Easing::Out(Curve::Bounce)),
    ])
```

`Keyframe::to(value)` takes `.duration(ms)`, `.delay(ms)` and `.ease(..)`.

### Repeating

`.repeat(3)`, `.repeat_forever()`, `.alternate(true)` (every other pass runs
backwards), `.reversed(true)` (the whole thing runs backwards).

During a leading `delay` every track reports its *starting* value — which is
what keeps a staggered entrance hidden until its turn instead of flashing and
then animating.

> An endless motion asks the window for a frame forever. That is what looping
> costs; use it for a hint, not for a screen, and pause it when it is off
> screen.

### Presets

`.as_margins()` re-expresses `X`/`Y` as margins. Those two are relative
insets, which is the right way to move something in flow — it slides and its
neighbours do not care. But an element pinned with `absolute()` *is* its inset,
so animating one would drag it off its pin; margins offset it from where it was
pinned instead.

`Motion::enter(TransitionKind)` and `Motion::exit(TransitionKind)` are the
stock fade/slide pair, as a motion you can retime, delay, stagger or drop into
a sequence. `enter_from(kind, distance)` / `exit_to(kind, distance)` spell out
the travel. `Motion::pulse()` is an endless breathing fade.

## Playing one

Two ways, and the difference is whether you want a wrapper element.

```rust
// A wrapper div around anything:
Animated::new("card").motion(motion).child(card)

// Or straight onto an element that is already `Styled` — no extra element,
// no change to how it sits in its parent:
div().w_full().child(row).animate("row", motion)
```

`.animate(id, clip)` comes from the `Motioned` trait, implemented for
everything that is `IntoElement + Styled`. Prefer it whenever you have a box
already: a wrapper is a new flex item, and a `w_full` child would start
measuring against it instead of the row it was in.

`.animate_when(condition, id, clip)` is the conditional twin.
`.when(cond, |el| el.animate(..))` cannot compile — `animate` changes the
element's type and `when` has to hand back the type it was given — so this
keeps the type stable by running an empty clip when the condition is false. The
two states get different element ids, so the clip starts from its beginning
each time the condition turns on.

Both play once, from the moment the element first lays out. **Changing the id
replays it** — that is the only way to restart a mounted one-shot, and it is
what a "play again" button hands you:

```rust
Animated::new(("card", self.epoch))       // bump epoch to replay
```

## Sequences

Several motions on one element, on one clock. anime.js calls this a timeline.
The [`sequence!` macro](macros.md#sequence--motions-on-one-clock) is the terse
form of everything below.

```rust
Sequence::new()
    .add(slide_out)                          // at the end of what's there
    .add_at(drop_down, At::With(0.0))        // alongside the previous one
    .add_at(come_back, At::Rel(-140.0))      // overlapping its tail
    .label("settled", At::End)
    .add_at(flash, At::Label("settled".into(), 50.0))
    .repeat_forever()
```

`At` is `End` (the default), `Abs(ms)`, `Rel(ms)` (from the end — negative
overlaps), `With(ms)` (the previous entry's start), and `Label(name, ms)`.

Motions layer: a later entry writing the same property wins for that frame. A
motion contributes nothing before its turn, so an element keeps whatever you
styled it with until the clip reaches it.

## Stagger

The *other* kind of choreography: the same motion across many elements, offset
per index. One element is one clip, so this is not a timeline feature — it is a
function from an index to a delay that you fold into each element's own motion.
Which means a list can grow or reorder mid-flight without restarting anything.

```rust
let rise = Stagger::new(60.0).from(StaggerFrom::Center);

for (i, row) in rows.iter().enumerate() {
    div()
        .child(row)
        .animate(("row", i), Motion::enter(TransitionKind::SlideUp)
            .delay(rise.at(i, rows.len())))
}
```

`Stagger::new(step_ms)` takes `.start(ms)`, `.from(StaggerFrom::{First, Last,
Center, Index(i)})`, `.grid(cols, rows)` (distance measured in two dimensions),
`.axis(StaggerAxis::{X, Y})`, `.ease(..)` (reshapes the spacing without moving
the ends), and `.reversed(true)`.

`at(index, total)` is the delay, `span(total)` is how long until the last one
starts, and `value(index, total, from, to)` spreads a *value* across a range
instead of a delay — anime.js's `stagger([a, b])`.

## Animator — a playhead you own

`Animated` is fire-and-forget. When a user drives the animation — a scrubber, a
play/pause, something that has to run backwards — the state belongs in an
entity.

```rust
let player = cx.new(|cx| Animator::new(sequence, cx).autoplay(cx));

// from handlers:
player.update(cx, |p, cx| p.toggle(cx));
player.update(cx, |p, cx| p.reverse(cx));
player.update(cx, |p, cx| p.seek_progress(0.5, cx));
player.update(cx, |p, cx| p.set_speed(0.25, cx));

// in render:
Animated::new("stage").animator(&player).child(box_)
```

Methods: `play`, `pause`, `toggle`, `restart`, `stop`, `seek(ms)`,
`seek_progress(0..=1)`, `reverse`, `set_speed`, plus `time()`, `progress()`,
`is_playing()` and `is_reversed()`. It emits
`AnimatorEvent::Begin` and `AnimatorEvent::Complete`.

There is no per-frame event, because your `render` already runs every frame:
reading `animator.frame(window)` there *is* the update callback. That call also
asks the window for the next frame while the clip is still moving, which is the
whole repaint loop — nothing ticks, nothing mutates per frame, and a paused
animation costs nothing. Under test, `frame_at(instant)` samples without a
window.

## Reading values yourself

`Frame::apply(el)` sets every styled property on an element. When you want the
numbers instead — driving a chart, a counter, an `Image` transformation:

```rust
let frame = self.player.read(cx).frame(window);
let angle = frame.number_or(Prop::Rotate, 0.0);
let count = frame.number_or(Prop::Custom("total"), 0.0);
let fill  = frame.color(Prop::Background);
```

## Transition

Plays a one-shot entrance animation around its child. Give it a stable id so
the animation has identity.

```rust
Transition::new("hero")
    .kind(TransitionKind::SlideUp)
    .easing(Easing::Spring(Spring::default()))
    .duration_ms(220)
    .child(Card::new().child(content))
```

Methods: `new(id)`, `kind(TransitionKind)` (default `Fade`), `easing(Easing)`,
`duration_ms(u64)` (default `200`), `child(impl IntoElement)`.

`TransitionKind` is `Fade` | `SlideUp` | `SlideDown` | `SlideLeft` | `SlideRight`.

## Collapse

Reveals gated content. Give it the content height and it animates that height
**open and closed** — a real collapse, content clipped while moving:

```rust
Collapse::new("details")
    .open(self.expanded)
    .height(120.0)             // content height in px
    .easing(Easing::EaseInOutCubic)
    .child(detail_panel())
```

With a height, the child stays mounted at height 0 while closed so it can
animate back open. Without one, `Collapse` falls back to the old behavior:
fade in on open, unmount instantly on close.

Methods: `new(id)`, `open(bool)`, `height(f32)`, `easing(Easing)`,
`duration_ms(u64)` (default `180`), `child(impl IntoElement)`.

## Presence — exit animations

A stateless conditional (`if self.show { modal }`) can't animate out: the
element is gone the frame the flag flips. `Presence` is a small entity that
latches the element through its exit.

```rust
let presence = cx.new(|cx| {
    Presence::new(cx)
        .kind(TransitionKind::SlideUp)
        .duration_ms(160)
        .content(|_window, _cx| {
            Modal::new("settings").child(settings_form()).into_any_element()
        })
});

// open / close from handlers:
presence.update(cx, |p, cx| p.set_open(true, cx));
presence.update(cx, |p, cx| p.set_open(false, cx));  // plays exit, then unmounts
```

The content closure is re-invoked every frame while visible (live data, same
rule as Tabs/Accordion panels). `set_open(false)` plays the exit animation,
then stops rendering and emits `PresenceEvent::Hidden` — subscribe if you
need to clean up after the element is truly gone. Rapid toggles are safe:
each open/close bumps an internal epoch, so a reopen cancels a pending hide.

Methods: `content(fn(&mut Window, &mut App) -> AnyElement)`,
`kind(TransitionKind)`, `easing(Easing)`, `duration_ms(u64)` (default `180`),
`set_open(bool, cx)`, `toggle(cx)`, `is_open()`. Emits `PresenceEvent::Shown`
/ `PresenceEvent::Hidden`.

Wrap a `Modal`, `Drawer`, or any overlay in a `Presence` to give it an exit
animation — the overlay itself doesn't need to know.
