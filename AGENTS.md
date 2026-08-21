# guise

A component library for [gpui](https://github.com/zed-industries/zed)
(Zed's GPU-accelerated Rust UI framework). Workspace: `crates/guise` (library) +
`crates/gallery` (live showcase) + `crates/tailor/*` (Tailor, the visual
interface builder), with `docs/` (markdown) rendered by `site/` (Bun).
Full human docs live in [`docs/`](docs/readme.md);
[`docs/architecture.md`](docs/architecture.md) is the map,
[`docs/tutorial.md`](docs/tutorial.md) the walkthrough, and
[`docs/tailor.md`](docs/tailor.md) the builder.

## Commands

```sh
cargo run -p gallery      # launch the showcase
cargo run -p tailor-app   # launch Tailor, the interface builder (binary: tailordev)
cargo check -p guise-ui   # fast type-check (package is guise-ui; lib name is guise)
cargo test -p guise-ui    # 520+ tests: inline #[cfg(test)] + src/apptests.rs
cargo test -p tailor-model -p tailor-codegen -p tailor-store   # Tailor's pure half
cargo build --workspace --locked                               # what CI builds
cd site && bun run build.ts                                    # docs/ -> site/dist
```

## Build constraints

- Everything builds against **plain crates.io `gpui = "0.2.2"`** — no git pins,
  no `[patch.crates-io]`. Don't use gpui APIs newer than that snapshot; the few
  style/scroll gaps it has are shimmed in `style.rs` (`FlexExt`).
  `thirdparty/block/` is a leftover vendored crate referenced by no manifest.
- The library package is **`guise-ui`** (crates.io name) with `[lib] name = "guise"`,
  so cargo commands use `-p guise-ui` while code imports `use guise::...`.

## The two component patterns

1. **Stateless `RenderOnce` builder** — `#[derive(IntoElement)]`, chainable
   `mut self -> Self` setters, parent owns all state. Most components. *Controlled*
   inputs (`Checkbox`, `Switch`, `Radio`) are this: parent holds the value and
   passes `.on_change(cx.listener(...))`.
2. **Stateful gpui entity** — `Render` + `EventEmitter<…>`, owns a
   `FocusHandle`/buffer/open-state. Built with `cx.new(...)`, parent subscribes to
   events. These are `TextInput`, `TextArea`, `NumberInput`, `PasswordInput`,
   `PinInput`, `Select`, `Combobox`, `SegmentedControl`, `Slider`, `RangeSlider`,
   `ColorInput`, `TagsInput`, `Menu`, `ContextMenu`, `HoverCard`, `Tabs`,
   `Accordion`, `Pagination`, `Editor`, `MarkdownEditor`, `TableView`, `DataView`, `TreeView`,
   `TabBar`, `SplitPanel`, `PaneGroup`, `UpdatePrompt`, `UpdateNotice`.

Both patterns can two-way bind to the reactive layer (`guise::reactive`):
`Signal<T>` is the store, `Binding<T>` the connection — controlled builders take
`.bind(signal.binding())`, entities take `X::bind(&entity, &signal, cx)`.

## Conventions (non-obvious — follow exactly)

- **Read every visual from the theme** via `theme(cx)` — never hardcode a color or
  size. This is what makes light/dark switching free. Semantic getters
  (`body`/`surface`/`text`/`dimmed`/`border`) and the `surface(theme, color, variant)`
  resolver in `style.rs` already encode the dark/light branches.
- **Resolve all theme values into locals BEFORE any `cx.listener(...)` or content
  closure.** `theme(cx)` borrows `cx` immutably; listeners need it mutably. A late
  `theme(cx)` read overlaps the borrow and won't compile.
- **Closures stored on elements (`.hover`, `.on_click`) must be `'static`** — capture
  resolved `Hsla`/`f32` values, not the `&Theme` borrow.
- **Tabs/Accordion panel content is a builder closure re-invoked every frame** so
  panels show live data, not a snapshot.
- **Overlays paint above siblings via `deferred()` + `occlude()`** (Modal, Menu,
  Select dropdown).
- Container components implement `ParentElement` (just `extend`); `.child`/`.children`
  come free.
- **Icons are Lucide**, drawn from an icon font embedded in the crate
  (`assets/lucide/`); it self-registers on first render, so consumers need no
  asset setup. `src/icon/lucide.rs` is generated — never hand-edit it;
  regenerate with `bun scripts/icons.ts`. Icon slots on components take
  `impl Into<Glyph>` (a Lucide `IconName` or literal text).

## Tailor — the interface builder in `crates/tailor/`

A drag-and-drop builder for guise interfaces, shipped in this repo as five
`publish = false` crates. `cargo run -p tailor-app` (binary `tailordev`);
`cargo test -p tailor-model -p tailor-codegen -p tailor-store` for the pure
half. Full docs in [`docs/tailor.md`](docs/tailor.md).

- **`model/`** — the document: the component catalog, the node arena, tokens,
  state variables, actions, undo, and the `.tailor` file format. No gpui, and it
  carries most of the tests: reparent rules, cycle checks, the lint pass, and the
  file round-trip are where a builder actually goes wrong.
- **`codegen/`** — document → guise Rust. Driven by the same catalog the canvas
  reads, so a component cannot render one way and generate another.
- **`store/`** — project files, recents, editor settings, export.
- **`render/`** — document → live guise components. Interaction never reaches
  back into the app directly: it goes through `Hooks`, built from a *weak*
  handle, because a live component tree must not own the view that renders it.
- **`app/`** — one `Workbench` entity owns the project and every panel; the
  panels are render methods in sibling files, not views of their own.
- **The editor bridge** (`store/src/bridge.rs` + `--reveal`) jumps both ways
  between a component and its code: out through `Generated::lines`, the map
  codegen builds by tagging each node's expression as it writes; in through an
  export index and a focus request the open window picks up on its existing
  poll. `extensions/zed/` is separate — a Zed MCP context server, its own cargo
  workspace because it targets `wasm32-wasip2`.
- **`mcp/`** — an MCP server over the same model (`tailor-mcp`). Hand-rolled
  JSON-RPC over stdio; it saves after every change, and the app polls the file
  it has open, which is the whole integration between them.

Two things about it are load-bearing and easy to break:

- **The catalog is the single source of truth.** Adding a component is one
  `comp!` entry plus one arm in `render/src/nodes/build.rs`. `PropSpec::emit`
  decides what the generator prints. Editing one without the other is how the
  canvas and the export drift.
- **Five containers are drawn, not instantiated** (`Tabs`, `Accordion`,
  `SplitPanel`, `AppShell`, `Carousel`). Their regions take `'static` closures,
  which a designer cannot drop into; drawing them from the theme is what makes
  their slots real drop targets. Generated code uses the real component.

Tailor wears the *project's* theme app-wide, because guise resolves colours when
a component paints rather than when you build it — there is no way to scope a
second theme to the canvas subtree.

The project is held as `Arc<Project>` and edited through `Arc::make_mut`: the
history and the canvas share the same allocation, so a commit and a frame are
free and an edit pays for exactly one copy. Codegen, lint, autosave, export and
the file watcher run on the background executor, debounced, guarded by a
revision counter and cancelled by dropping the held `Task`. Anything touching
gpui entities — the preview store — stays on the main thread. Tests zero the
debounce and call `cx.run_until_parked()` before asserting on generated code or
problems.

## File/naming conventions

- **One component per file**, lowercase, no `-`/`_`/spaces. Group with directories
  (`input/select.rs`), never concatenated names (`input-select.rs`).
- `flex/` is **not** glob-exported (names overlap with `layout/`); import via
  `use guise::flex::*`. `layout/` is token/`Size`-based; `flex/` is
  pixel-based Flutter-style (`Row`/`Column`/`Expanded`/`EdgeInsets`).
- `update/` (self-update: release check, in-place install, `UpdatePrompt`) is the
  one module that **shells out** — `curl`, and `hdiutil`/`rsync`/`codesign` on
  macOS — and the one that parses nested JSON, with its own reader
  (`theme/json.rs` is flat-only on purpose). Keep both confined there.

## Adding a component

1. New file under the right module (or crate root for a loose one).
2. `RenderOnce` builder, or `Render` + `EventEmitter` entity if it owns state.
   Resolve visuals from `theme(cx)`.
3. Re-export: module `mod.rs` → `lib.rs` → `prelude`.
4. Add a showcase to `crates/gallery/`.
5. Document it on the right `docs/*.md` page; a **new** page must also be
   registered in `site/render/nav.ts` or the site won't build it.
6. Unit-test pure logic (parsing, range math, editing models) with `#[cfg(test)]`
   next to the code. For wiring that needs a live app — signals, bindings, entity
   events, the theme global — use the gpui test harness:
   `#[gpui::test] fn x(cx: &mut TestAppContext)` in `src/apptests.rs`.
