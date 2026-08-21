# DevTools

`guise::devtools` is Safari's Web Inspector, aimed at the gpui app it is running
inside. Eight tools across the top — Elements, Network, Sources, Timelines,
Storage, Layers, Logs, Audit — a tree beside a details sidebar, and a drawer
that drops the log over whatever else you were looking at.

```rust
use guise::prelude::*;

// once at startup
DevToolsState::new().init(cx);

// anywhere you want it: a pane, a drawer, its own window
let devtools = cx.new(DevTools::new);
```

That is the whole integration for the panels that inspect the UI.

An inspector records the window it is rendered in, and only that one. The
recorder is per thread rather than per window, so an inspector claims the
current frame when it renders and every other window your app draws that frame
is skipped — put the inspector in the window whose components you want to see.
Two inspectors in two windows take that claim from each other every frame and
both come up empty; there is one tree, and showing each of them a tree of both
windows would be worse than showing neither. Try it:

```sh
cargo run -p guise-ui --example devtools           # opens on Elements
cargo run -p guise-ui --example devtools -- network
```

## Where the data comes from

The split between what the inspector *knows* and what it is *told* is the whole
design, and it is worth understanding before wiring anything up.

| Panel | Source |
| --- | --- |
| Elements, Layers, Styles | **Introspection.** Components report themselves; the tree, the bounds and the style declarations are read back out of what actually rendered. |
| Logs, Network, Storage, Timelines | **Reported by the host.** `guise` never opens a socket, so your code calls `log`, `network_begin`, `storage_set`, `measure`. |
| Sources | The `#[track_caller]` locations on the tree, read off disk. |
| Audit | Computed here, from the tree, against the library's own rules. |

This is the same arrangement [`guise::ai`](ai.md) uses: the component owns the
display, the host owns the work.

### Why it is called Logs and not Console

Half of Safari's Console tab is a JavaScript evaluator. A compiled binary has
nothing to evaluate, so a prompt here would be a text field that cannot answer.
The panel is named for the half that transfers.

## Elements

The tree is the **component** hierarchy, not a wall of anonymous containers:
`Button   variant: filled, size: sm`, foldable, one row per component.

It is an indented tree rather than the markup a browser prints, because these
are components built from builder calls, not tags: there is no attributes-versus
-children distinction to draw, `<Button … />` would promise a model gpui does
not have, and a closing row says nothing the next row's indentation has not
already said while costing a container half the panel. Props read as a YAML flow
mapping, the way the Styles pane reads a declaration. Selecting a node fills the
sidebar.

- **Styles** — the element's own declarations, rendered as a CSS rule, with color
  swatches and the source location it was constructed at. Click the location in
  the Node pane to jump to Sources.
- **Computed** — the box model diagram (margin / border / padding / content, each
  edge labelled in pixels) plus every declaration sorted by name.
- **Node** — identity, geometry and the reported attributes.

The sidebar is read-only, and deliberately: a probe is a snapshot taken during
prepaint, so writing to it would edit a copy and change nothing on screen.

### How the tree gets built

gpui will tell you which element the pointer is over, but it will not enumerate
a tree — `inspector_hitboxes` is crate-private and holds one frame of whatever
was under the cursor. So `guise` records its own.

Every component tags its root element:

```rust
impl RenderOnce for Button {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        // ...
        element
            .probe("Button")
            .attr("variant", self.variant.label())
            .attr("size", self.size.label())
            .attr_if("disabled", self.disabled)
    }
}
```

`probe` wraps the element in a pass-through that pushes a node on the way into
`prepaint` and pops it on the way out. gpui prepaints depth-first, so the
push/pop pairs nest exactly like the element tree does.

Do the same in your own components and they appear in the tree beside the
library's. It costs one boolean check per element per frame while the inspector
is closed, and nothing is allocated in that state — attributes are dropped at the
setter, and `attr_with` defers a `format!` you would otherwise pay for on every
release-build frame.

| Method | Notes |
| --- | --- |
| `probe(name)` | The normal case. Snapshots the element's style, which is what fills the Styles sidebar. Needs `Styled`. |
| `probe_any(name)` | For a component that returns something already composed — a `Field`, a `deferred(..)` overlay. No style snapshot; the wrapped component reports its own. |
| `attr(name, value)` | A prop shown inline after the component's name. |
| `attr_with(name, \|\| …)` | The same, but the value is only built while recording. |
| `attr_if(name, bool)` | A prop with no value, printed on its own when true — `dimmed` rather than `dimmed: true`. |

## Logs

Levels, coalesced repeats, expandable detail rows, a source link per line, and a
filter that searches all three.

```rust
guise::devtools::log(cx, LogLevel::Warning, "cache miss");

guise::devtools::log_record(
    cx,
    LogRecord::new(LogLevel::Error, "Failed to decode avatar.png")
        .detail("bytes", "18420")
        .detail("format", "png"),
);
```

`log` is `#[track_caller]`, so the line knows where it came from without you
passing a location. Identical consecutive messages collapse into one row with a
counter rather than scrolling the useful history away.

## Network

A sortable table whose last column is the timing waterfall, with a per-request
sidebar of Headers / Cookies / Sizes / Timing / Preview.

Open the record when the request starts and settle it by id when it lands:

```rust
let id = guise::devtools::network_begin(
    cx,
    NetworkRecord::new("GET", "https://api.example.com/v1/items")
        .kind(ResourceKind::Fetch)
        .request_header("Accept", "application/json"),
);

if let Some(id) = id {
    guise::devtools::network_update(cx, id, |record| {
        record.state = RequestState::Finished;
        record.status = Some(200);
        record.transfer_size = 4_200;
        record.resource_size = 11_800;
        record.timings.response = Duration::from_millis(34);
    });
}
```

A record shows as Pending for as long as it is in flight. `network_update` on an
id that has already been evicted does nothing, so a long-lived request cannot
panic the inspector.

## Storage

The host names its own domains; the panel groups them under Safari's headings.

```rust
guise::devtools::storage_set(
    cx,
    StorageDomain::new("prefs", "app.preferences")
        .kind(StorageKind::Local)
        .entry(StorageEntry::new("theme", "dark"))
        .entry(StorageEntry::new("window", "1280×820")),
);
```

Registering the same id again replaces the snapshot, so a host can publish on
every change without accumulating duplicates. `StorageDomain::columns` adds
columns beyond Key and Value — cookie attributes, record types — which each
entry fills in by name through `StorageEntry::extra`.

## Timelines

Instrument bands laid out against one ruler, plus an event list.

```rust
// time an existing call without restructuring it
let index = guise::devtools::measure(cx, "reindex()", || reindex());

// or report a span you measured yourself
guise::devtools::timeline_event(
    cx,
    TimelineEvent::new(TimelineKind::Layout, "layout pass", elapsed),
);
```

The **Frames** band is measured by the inspector itself and is off until you
press Record. That is not laziness: gpui paints on demand, so the gap between
two frames of an idle window is however long it sat idle, which would report as
a stall that never happened.

## Sources

The files the tree's elements were constructed in, read off disk and shown with
line numbers around the target line. Paths from `#[track_caller]` are relative
to the workspace root, so they are resolved against the working directory and
each of its ancestors — which finds the checkout from anywhere inside it. When
the file is not there, the panel says so rather than guessing.

## Audit

Rules that run over the recorded tree, worst first, each finding pointing at a
node the Elements panel can select.

| Rule | Reports |
| --- | --- |
| Text contrast | Text on a background it fails WCAG's 4.5:1 against. |
| Hit target size | A control small in *both* directions — a wide row a pixel or two short of 24 is not the defect the rule is aimed at. |
| Collapsed container | Children, but zero width or height. Almost always a missing `flex_1`, `w_full` or `min_h(0)`. |
| Overflow | A child painting outside its parent by more than a pixel. |
| Nesting depth | Deeper than 24 levels. |

## Events

Everything the inspector can do alone, it does alone. These are the rest:

```rust
cx.subscribe(&devtools, |this, _devtools, event: &DevToolsEvent, cx| match event {
    // A source location was clicked. Sources has already opened it; this is
    // your chance to open it in a real editor instead.
    DevToolsEvent::RevealSource(source) => this.open_in_editor(source),
    DevToolsEvent::Dock(side) => this.move_inspector(*side, cx),
    DevToolsEvent::Close => this.hide_inspector(cx),
    DevToolsEvent::Picking(armed) => this.arm_picker(*armed, cx),
})
.detach();
```

`Dock` and `Close` are requests, not actions: `guise` cannot move or hide a panel
the host owns, so the buttons report and you decide.

## Element picking

The crosshair arms the picker and emits `DevToolsEvent::Picking(true)`. Hit
testing happens in the window, not in the panel, so the host forwards the point:

```rust
if devtools.read(cx).is_picking() {
    devtools.update(cx, |devtools, cx| devtools.pick_at(event.position, cx));
}
```

`pick_at` selects the deepest recorded node containing that point, expands its
ancestors, scrolls the tree to it, and disarms. `DevTools::selected_bounds` gives the selection's bounds
back, for a host that wants to paint a highlight over its own window.

## Cost

Nothing here is compiled out of release builds, because dead-code elimination
already handles that: an app that never constructs `DevTools` links none of it.
What an app *does* carry is the `probe` calls left in components, and those are
a boolean check while the inspector is closed. Recording turns on when a
`DevTools` is created and off when it is dropped.

The stores are rings — 1000 log lines, 1000 requests, 4000 timeline events by
default — so a long-running app cannot grow the inspector without bound. Change
them with `DevToolsState::new().limits(Limits { .. })`.

Every reporting call is a no-op when `DevToolsState` was never installed, so
instrumentation can be left in place unconditionally.
