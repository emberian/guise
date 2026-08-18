# Changelog

Notable changes to [`guise-ui`](https://crates.io/crates/guise-ui). Versions
follow [semver](https://semver.org): from 1.0 on, a breaking change means a
major release, and is called out under **Breaking**. Releases before 1.0 landed
breaking changes in minor versions.

## 1.0.0 — 2026-08-18

The API is stable. Everything below 1.0 moved breaking changes through minor
versions; from here a break means a major release.

That is the whole meaning of this number — it is not a rewrite. `guise` has
been carrying real applications for months: ~60 components, a reactive layer, a
pane system, an editor, a markdown editor, an AI component set, a self-updater,
515 tests, and a documented page for every module. What changes today is the
promise, not the code.

One caveat worth stating plainly: `guise` builds against `gpui = "0.2.2"`, which
is itself pre-1.0. A breaking change in gpui forces a breaking change here, so
2.0 may well arrive on gpui's schedule rather than this crate's. The alternative
— staying at 0.x forever because a dependency is — helps nobody.


### New in 1.0: DevTools — Safari's Web Inspector, aimed at your own app

`guise::devtools` adds an in-app inspector with eight tools across the top:
Elements, Network, Sources, Timelines, Storage, Layers, Logs and Audit.

```rust
DevToolsState::new().init(cx);          // once at startup
let devtools = cx.new(DevTools::new);   // then put it wherever you like
```

`cargo run -p guise-ui --example devtools` opens it beside a small app.

- **Elements is real introspection, not a mock.** Every component now ends its
  `render` with `.probe("Name")`, which snapshots the element's
  `StyleRefinement` and brackets `prepaint` to rebuild the tree. So the outline
  is the live component hierarchy — `<Button variant="filled" size="sm" />`,
  foldable, with closing tags — and the Styles sidebar shows the element's
  actual declarations with color swatches, the Computed sidebar its real box
  model, and the Node sidebar the source location it was constructed at. gpui
  exposes only the element under the pointer and no way to enumerate a tree,
  which is why the recording exists.
- **Logs, Network, Storage and Timelines are reported by the host**, the same
  arrangement `ai/` uses: `log`, `network_begin`/`network_update`,
  `storage_set`, `measure`. Nothing in `guise` opens a socket. `log` is
  `#[track_caller]`, so a line knows where it came from without being told.
- **Sources** reads the files the tree points at off disk, resolving the
  workspace-relative paths `#[track_caller]` produces against the working
  directory and its ancestors.
- **Audit** runs rules over the recorded tree — WCAG text contrast, hit target
  size, collapsed containers, children escaping their parent — each finding
  selecting the node it came from.
- **Cost.** A probe is one boolean check per element per frame while the
  inspector is closed, and allocates nothing in that state. Recording starts
  with the first `DevTools` and stops with the last. An app that never
  constructs one links none of the panels.

Named Logs rather than Console on purpose: half of Safari's Console tab is a
JavaScript evaluator, and a compiled binary has nothing to evaluate.

### Also

- `Size::label()` and `Variant::label()` — the token names the docs already
  used, now available to code.
- New guide: [DevTools](docs/devtools.md).

## 0.13.0 — 2026-08-17

The release that makes text fields behave like the ones people already know,
adds a component set for putting a model in front of a person, and takes 43%
off the binary.

### Text fields work the way an `<input>` does

Every single-line field — `TextInput`, `PasswordInput`, `NumberInput`,
`ColorInput`, `Combobox`, `Autocomplete`, and the query box in `TagsInput` —
is now built on one shared core (`input/line.rs`) that shapes the line through
gpui's text system instead of drawing it as three sibling divs.

- **Tab moves to the next field.** It used to type a literal tab character:
  the platform reports a `\t` for the key, and the printable-input path took
  it. Shift+Tab goes back. Ordering follows render order, like `tabindex="0"`.
- **The mouse works.** Click to place the caret, drag to select, double-click a
  word, triple-click the value, Shift+click to extend. None of this existed —
  which is the real reason copying felt broken, since there was no way to
  select anything to copy.
- **Clipboard everywhere.** Cut, copy, and paste were on `TextInput` and
  `TextArea` only; the other six had none. A multi-line paste is flattened to
  one line, the way `<input>` flattens it.
- **Undo and redo**, coalesced by word rather than by keystroke.
- **IME, dead keys, press-and-hold accents, and the macOS character palette.**
  Painting a field now registers an `ElementInputHandler`, which is the only
  way to see any of them. Text entry therefore no longer runs through key
  handling — the platform delivers it after the key handler declines it.
- **Long values scroll horizontally** to keep the caret in view instead of
  disappearing under the border.
- The caret sits on a glyph boundary and blinks.

New on the fields: `read_only`, `max_length`, `tab_index`, `tab_stop`, and
`focus_handle` — the last three were on three fields and missing from four
that were nonetheless in the Tab ring.

`TextArea` gains Tab-moves-focus, undo, `max_rows`, `submit_on_enter` (with a
separate `TextAreaSubmit` event), `is_blank`, and a placeholder that stays
visible while the field is focused and empty.

### AI components

A new `guise::ai` module: a transcript, a prompt box, streaming feedback, tool
calls, citations, and the controls and meters around a request. See
[`docs/ai.md`](docs/ai.md) and `cargo run -p guise-ui --example ai`.

- `AIChatView` — the transcript, with stick-to-bottom scrolling that follows
  the tail only while you are already at the tail, and per-turn disclosure
  state. `AITurn` / `AITurnTool` are what go in it.
- `AIMessage` — one turn, if you would rather lay the list out yourself.
- `AIComposer` — Enter sends, Shift+Enter breaks the line, the box grows to a
  ceiling, and the send button becomes a stop button while a reply streams.
- `AIStreamingText`, `AIThinking`, `AIReasoning`, `AIToolCall`, `AICitation`,
  `AISources`, `AIModelPicker`, `AITokenMeter`, `AICost`, `AISettings`.

None of it opens a socket or holds a key — the host owns the request, so the
same transcript drives a local model, a hosted API, or a replayed log.

Also new: **`markdown::Markdown`**, a read-only markdown renderer over the same
pure passes `MarkdownEditor` uses. It is what message bodies draw with, and it
works anywhere text does.

### Security

- **Linux updates are verifiable.** `update::appimage` downloaded a file,
  marked it executable, and renamed it over the running binary with nothing but
  a byte count vouching for it — macOS had a pinned `codesign` requirement and
  Linux had no equivalent. A published SHA-256 is now checked when a release
  ships one, on both platforms, and `UpdateConfig::require_checksum(true)`
  turns a *missing* digest from a silent pass into a refusal. Recognises
  `<asset>.sha256` and `SHA256SUMS`-style listings; the hash comes from
  `shasum`, `sha256sum`, or `openssl`.
- **Unbounded recursion in the pane-layout decoder.** A corrupted or hostile
  snapshot of `"h0.5("` repeated recursed until the stack ran out, which
  aborts the process rather than unwinding. Capped at 64 levels, matching the
  cap the JSON reader already had.
- A stale IME composition range could produce text runs longer than the string
  they cover, which the text system slices by — a panic, not a mis-draw.
- `WebView`'s local-file handler no longer builds responses through `unwrap`
  on wry's request thread, where a panic takes the process with it.

### Size and performance

The gallery went from **13.86 MB to 7.82 MB** — see
[`docs/performance.md`](docs/performance.md) for the breakdown and the release
profile to copy.

- The bundled Lucide font is **78 KB smaller**: GSUB and the v2 `post` table
  are stripped, since glyphs are addressed by codepoint and neither is ever
  read (`scripts/stripfont.py`, run by the icon generator).
- `IconName`'s `Debug` is written out instead of derived — a derived one is a
  match with 1991 arms to print a string the name table already holds.
- The icon tables are `static` rather than `const`, so they are not
  materialised at each use site.
- `AIChatView` virtualizes: a turn more than a screen away is drawn as a spacer
  of its measured height, because building one re-parses its markdown. That
  was 1.25 ms per frame on a 46 KB transcript and grew with the conversation;
  it is now proportional to the viewport. `.virtualize(false)` opts out.
- Undo history is bounded by the text it retains (256k chars), not just by step
  count — 128 steps of a 200 KB `TextArea` was 100 MB of snapshots.
- `TextEdit::insert` and `replace_range` use one `splice` instead of shifting
  the tail per character, which was quadratic on a large paste.
- Assorted per-frame allocations removed: a masked field no longer builds the
  cleartext buffer to throw it away, collapsed reasoning blocks and folded tool
  cards no longer copy text nothing draws, and the composer no longer
  materialises the whole draft to test whether it is blank.

### Fixed

- The read-only markdown renderer and `MarkdownEditor` had drifted apart on
  heading sizes (h2 1.4 vs 1.45, h3 1.25 vs 1.28, code 0.92 vs 0.88), so the
  same document rendered at different sizes depending on whether you were
  reading or editing it. One table now serves both, in `markdown::layout`.
- `AIToolCall` asked for the `"monospace"` family, which the text system does
  not resolve — tool arguments and results rendered in the prose font.
- `AIModelPicker` sized itself ad-hoc and came out 44px tall where every other
  control at `Size::Sm` is 36, so it would not line up in a toolbar.
- The text-selection tint was open-coded in eight files and had already drifted
  from the editor's. It is now `Theme::selection()`.

### Breaking

- `theme::mantine()` is now **`theme::open_color()`**. The palette is
  open-color; the old name described where it was borrowed from rather than
  what it is.
- `AIChatViewEvent::Retry` removed — it was never emitted.
- `AIModelPicker::selected(index)` removed; use `selected_id(&str)`, which
  covers both the build-time and runtime cases.
- `IconName`'s `Debug` now prints the kebab-case name (`arrow-up`) rather than
  the variant name (`ArrowUp`), matching what lucide.dev lists it under.
- `apply_key` no longer types control characters, so Tab and Enter cannot be
  inserted as text. Fields use the new `apply_nav`, which leaves text entry to
  the platform's input handler; `apply_key` remains for hosts driving a
  `TextEdit` from raw key events.
- `Progress::color` and `Loader::color` take `impl Into<ColorValue>` rather
  than `ColorName`. Existing calls still compile.

### Added API

`Theme::selection()`, `markdown::layout::{metrics, RowMetrics}`,
`TextEdit::{chars, replace_range, undo, redo, break_undo, cursor, set_cursor,
set_selection, extend_to, word_at, byte_of, char_of, set_text}`,
`NumberInput::{set_value, set_min, set_max}`, `Slider::set_value`,
`TextArea::{is_blank, set_placeholder}`, `AIToolCall::expandable`,
`input::{LineEditor, LineState}`.

## Earlier releases

0.12.0 and before predate this changelog; see the
[git history](https://github.com/wess/guise/commits/main) and the
[release tags](https://github.com/wess/guise/releases).
