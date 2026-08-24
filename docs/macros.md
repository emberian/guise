# Macros

Terse builders for the common containers, a CSS-like block for styling, and a
matching pair for motion. They're all in the prelude, so
`use guise::prelude::*;` is all you need — the macros bring `.child()` into scope
themselves (no extra trait import).

Each container macro takes comma-separated children; a trailing comma is fine.

## Containers

One macro **per container component** — every type that takes a variadic list of
children.

| Macro | Builds | Spacing |
| --- | --- | --- |
| `row![ … ]` | [`flex::Row`](flex.md#row--column) | none (use `SizedBox`/`Spacer`) |
| `col![ … ]` | [`flex::Column`](flex.md#row--column) | none |
| `zstack![ … ]` | [`flex::Stack`](flex.md#stack--positioned) (overlap) | — |
| `wrap![ … ]` | [`flex::Wrap`](flex.md#wrap) | default spacing |
| `vstack![ … ]` | [`layout::Stack`](layout.md#stack) (themed) | token gap |
| `hstack![ … ]` | [`layout::Group`](layout.md#group) (themed) | token gap |
| `center![ … ]` | [`layout::Center`](layout.md#center) | — |
| `paper![ … ]` | [`Paper`](layout.md#paper) | — |
| `card![ … ]` | [`Card`](layout.md#card) | — |
| `modal![ … ]` | [`Modal`](overlays.md#modal) | — |

```rust
use guise::prelude::*;

col![
    row![avatar, name, Spacer::new(), actions],
    SizedBox::height(8.0),
    body,
]
```

Because a macro returns the underlying builder, you can keep chaining:

```rust
row![left, right].main_axis_alignment(MainAxisAlignment::SpaceBetween)
```

## Component shorthands

A few of the most common leaf components have shorthand macros too. They expand
to `Type::new(...)`, so every builder method still chains.

| Macro | Builds | Notes |
| --- | --- | --- |
| `text!(...)` | [`Text`](typography.md#text) | accepts `format!` args |
| `title!(...)` | [`Title`](typography.md#title) | accepts `format!` args |
| `code!(...)` | [`Code`](typography.md#code) | accepts `format!` args |
| `kbd!(...)` | [`Kbd`](typography.md#kbd) | accepts `format!` args |
| `button!(id, label)` | [`Button`](buttons.md#button) | forwards args |
| `badge!(label)` | [`Badge`](data.md#badge) | forwards args |

There are two more that aren't components at all — `motion!` and `sequence!`,
below — plus `color!` and `style!`.

The content macros take `format!`-style arguments, which is the real win over
the plain constructor:

```rust
text!("Signed in as {name}")          // = Text::new(format!("Signed in as {name}"))
title!("Page {}", n).order(2)
button!("save", "Save").variant(Variant::Filled).color(ColorName::Blue)
```

This is a deliberately small set. Most components **don't** get a macro: for a
builder with several setters, `Type::new(...)` chained with methods is already
the clearest form, and stateful entities (`TextInput`, `Select`, …) are created
with `cx.new(...)` where a macro doesn't fit. The shorthands exist only where
they genuinely read better.

## `color!` — CSS color literals

`color!` produces a gpui `Hsla` from CSS notation. See
[Theming → CSS-style colors](theming.md#css-style-colors).

```rust
color!(rgb(34, 139, 230))      color!(rgba(34, 139, 230, 0.5))
color!(hsl(210, 80, 52))       color!(teal)        color!("#228be6")
```

## `style!` — CSS-like style blocks

`style!` expands to an element transform you apply with `.apply(...)` (from the
`StyleExt` trait, in the prelude). It maps CSS-ish properties onto gpui's builder
methods, so a block of declarations reads like a stylesheet.

```rust
use guise::prelude::*;

gpui::div().apply(style! {
    display: flex;
    direction: column;
    align: center;
    justify: between;
    gap: 8;
    padding: 16;
    width: full;
    height: 200;
    radius: 12;
    background: "#11151c";              // string → css() shorthand
    color: color!(rgb(230, 230, 230));  // or any color! / Hsla expr
    border: color!("#2a2f3a");          // 1px border of this color
    weight: semibold;
    opacity: 0.95;
})
```

- **Numbers are pixels.** `padding: 16` → `.p(px(16.))`.
- **Colors** are a string literal (parsed by `css`) or any `Into<Hsla>` expression
  (e.g. `color!(..)`).
- **Every declaration ends with `;`.**
- **No theme tokens.** `style!` is pure and has no `cx`, so `Size::Md`-based
  spacing/radius/font aren't available — use raw px here, or the builder methods
  (which read the theme) for token values.

Supported properties: `background`, `color`, `border`; `display: flex`;
`direction: row|column|col`; `align: start|center|end|stretch`;
`justify: start|center|end|between|around|evenly`; `position: absolute|relative`;
`weight: bold|semibold|medium|normal`; `width`/`height` (`full` or px), `size`,
`min_width`, `min_height`, `padding`/`px`/`py`/`pt`/`pr`/`pb`/`pl`,
`margin`/`mx`/`my`/`mt`/`mr`/`mb`/`ml`, `radius`, `gap`, `font_size`, `opacity`.

Because it's just a transform, it composes with everything: keep chaining
interactive methods (`.id(..)`, `.on_click(..)`, `.hover(..)`) after `.apply(..)`.

## `motion!` — animation as a declaration block

What `style!` is to a box, `motion!` is to an animation: timing and tweens as
one block instead of a chain of setters. See
[Motion & transitions](transitions.md) for what the pieces mean, and the
[motion tutorial](motiontutorial.md) for building one up.

```rust
use guise::prelude::*;

div().child(card).animate("card", motion! {
    duration: 420;
    ease: out back;
    opacity: 0 => 1;
    y: 12 => 0;
})
```

A track is `prop: from => to`. Numbers are px (or degrees for `rotate`, or a
multiplier for `scale`); colours are any `Into<Hsla>`, so `color!(..)` and a
theme read both work. A list on the right is a multi-leg path:

```rust
motion! {
    duration: 900;
    ease: in_out quad;
    y: 0 => [-30, 0];                       // two legs, splitting the duration
    bg: soft => [
        Keyframe::to(accent).duration(500.0),   // or legs with their own timing
        Keyframe::to(soft),
    ];
}
```

**Timing and repetition**

| Declaration | Means |
| --- | --- |
| `duration: 420;` | ms each track gets, before per-leg overrides |
| `delay: 80;` | ms of stillness first — tracks hold their starting value |
| `end_delay: 120;` | ms of stillness after, inside the loop |
| `ease: out back;` | see below |
| `repeat: forever;` | or `once`, or a count: `repeat: 3;` |
| `alternate;` | bare flag — every other pass runs backwards |
| `reversed;` | bare flag — the whole thing runs backwards |
| `margins;` | bare flag — `x`/`y` become margins, for an `absolute()` element |

**Easing** is a direction and a shape, plus three words of its own:

```rust
ease: linear;        ease: spring;        ease: steps(4);
ease: in quad;       ease: out elastic;   ease: in_out sine;
ease: Easing::CubicBezier(0.25, 0.1, 0.25, 1.0);   // any Easing expression
```

Shapes: `quad`, `cubic`, `quart`, `quint`, `sine`, `expo`, `circ`, `back`,
`elastic`, `bounce`.

**Presets** pick the constructor, so they come first if you use one:

```rust
motion! { enter: slide_up; duration: 300; delay: 80; }
motion! { enter: slide_left 24; }        // with the travel spelled out
motion! { exit: fade; }
```

`fade`, `slide_up`, `slide_down`, `slide_left`, `slide_right`.

**Props**: `opacity`, `x`, `y`, `w`/`width`, `h`/`height`, `mt`/`mr`/`mb`/`ml`,
`pt`/`pr`/`pb`/`pl`, `radius`, `border_width`, `gap`, `font_size`,
`bg`/`background`, `border_color`, `color`, `rotate`, `scale`.

The block returns the builder, so anything it doesn't cover still chains:

```rust
motion! { duration: 200; opacity: 0 => 1; }.repeat(3)
```

## `sequence!` — motions on one clock

The variadic one. `sequence!` is to motions what `col!` is to children:

```rust
sequence![
    fade_in,
    rel(-120) => slide_up,              // 120ms before the end so far
    with(0) => tint,                    // alongside the previous entry
    abs(600) => flash,                  // from the sequence's own start
    label("settled", 50) => ripple,     // 50ms after a placed label
]
```

A bare entry lands after everything before it. The position goes **in front**
because a Rust macro can't read anything but `,`, `;` or `=>` after an
expression — and reading "at rel(-120), slide up" turns out to be the right way
round anyway.

Labels are placed with the builder (`Sequence::label`), so a sequence that
refers to one starts there and the macro fills in the rest.

## Why `col!`, not `column!`

The standard library already exports a `column!` macro (it returns the current
source column number). Naming ours `col!` avoids the clash when both are in
scope via globs.

## How they stay import-free

The macros expand to e.g. `flex::Row::new().child(a).child(b)`. `.child()` comes
from gpui's `ParentElement` trait, which the macro brings into scope anonymously
through a hidden re-export (`guise::__ParentElement`). You never have to import
the trait yourself.
