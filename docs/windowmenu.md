# Window menu & chrome

The native application menu (the macOS menu bar / OS menu) is a gpui feature,
not a `guise` component — `guise` doesn't theme native chrome. This page shows
the idiomatic way to wire it, as the gallery does.

> For a **themed, in-window** menu bar — drawn by `guise`, useful when you
> render your own titlebar or run on a platform with no native menu bar — see
> [`MenuBar`](overlays.md#menubar-entity) instead.

## Define actions

Menu items dispatch `gpui::Action` types. Derive them:

```rust
#[derive(Clone, PartialEq, Default, Debug, gpui::Action)]
#[action(namespace = myapp, no_json)]
struct ToggleThemeAction;

#[derive(Clone, PartialEq, Default, Debug, gpui::Action)]
#[action(namespace = myapp, no_json)]
struct QuitAction;
```

## Build the menus

Call `cx.set_menus(...)` in your `run` closure. Use the fully-qualified
`gpui::Menu` / `gpui::MenuItem` so they don't clash with guise's overlay
[`Menu`](overlays.md#menu-entity).

```rust
cx.set_menus(vec![
    gpui::Menu {
        name: SharedString::new_static("My App"),
        items: vec![
            gpui::MenuItem::action("Toggle Theme", ToggleThemeAction),
            gpui::MenuItem::separator(),
            gpui::MenuItem::action("Quit", QuitAction),
        ],
    },
    gpui::Menu {
        name: SharedString::new_static("View"),
        items: vec![gpui::MenuItem::action("Toggle Theme", ToggleThemeAction)],
    },
]);
```

`MenuItem` also offers `MenuItem::submenu(menu)` and
`MenuItem::os_action(...)` for standard OS roles.

## Handle the actions

Register global handlers with `cx.on_action`:

```rust
cx.on_action::<QuitAction>(|_, cx| cx.quit());

cx.on_action::<ToggleThemeAction>(|_, cx| {
    let dark = cx.global::<Theme>().scheme.is_dark();
    cx.global_mut::<Theme>().scheme = if dark { ColorScheme::Light } else { ColorScheme::Dark };
    cx.refresh_windows();
});
```

Global handlers fire regardless of which view is focused — convenient for a
single-window app. To scope an action to a view instead, register it on the
view's root element with `.on_action(cx.listener(Self::handler))`.

## Full wiring

```rust
gpui::Application::new().run(|cx: &mut App| {
    Theme::dark().init(cx);

    cx.set_menus(/* … as above … */);
    cx.on_action::<QuitAction>(|_, cx| cx.quit());
    cx.on_action::<ToggleThemeAction>(|_, cx| { /* … */ });

    // open_window(...);
    cx.activate(true);
});
```

## Window chrome the app has to draw itself

On macOS and Windows the OS draws the close/minimise/zoom buttons and owns the
resize border. A client-side-decorated window on Linux draws both or goes
without: no buttons, and edges that cannot be dragged.

```rust
// Only where the platform leaves it to you.
div()
    .child(
        Group::new()
            .child(tab_bar)
            .children(WindowControls::needed().then(WindowControls::new)),
    )
    .children(ResizeHandles::needed().then(ResizeHandles::new))
```

Both render on any platform if you ask them to. The `cfg` is in `needed()`
rather than inside the components, because a `cfg!(target_os)` buried in a
component is impossible to preview from the other side.

`ResizeHandles` is absolutely positioned over the whole window and inert in the
middle, so it never swallows a click meant for the app — put it last in the root
element so its edges sit above the content.

`TRAFFIC_LIGHT_INSET` is the clearance to reserve at the leading edge of a
custom titlebar so the macOS traffic lights are not overlapped; it is zero where
the OS draws no inset. `PaneGroup`'s top-row tab bar already reserves it.

## About

The small centred card an app owes its users, opened from the application menu:

```rust
About::new("Acme")
    .icon(img(icon).w(px(128.0)).h(px(128.0)))
    .version(env!("CARGO_PKG_VERSION"))
    .tagline("Does the thing, quickly.")
    .build(BuildKind::Released, env!("BUILD_DATE"))
    .link(Anchor::new("repo", "github.com/acme/acme"))
    .credits("© 2026 Acme")
```

The part worth having is `BuildKind`. A build made from some commit that merely
carries the version number is not the release, and printing "Released
2026-08-18" on one is a small lie that costs a bug report — so a development
build says what it is:

| `BuildKind` | Date given | Line |
| --- | --- | --- |
| `Released` | `2026-08-18` | Released 2026-08-18 |
| `Development` | `2026-08-18` | Development build · 2026-08-18 |
| either | `unknown` or empty | Released build / Development build |

Set it from a build script — the tag check is what makes the distinction
honest, not the constant.
