# Settings

`guise::settings` is the settings screen every desktop app has: a list of pages
down the left, the selected page on the right, groups with a heading, and rows
with the name on one side and the control on the other.

```rust
use guise::prelude::*;

let settings = cx.new(|cx| {
    SettingsView::new(cx)
        .page_icon("appearance", "Appearance", IconName::Palette)
        .page_icon("editor", "Editor", IconName::FileCode)
        .searchable(true)
        .content(move |page, query, _window, cx| match page {
            "appearance" => appearance_page(&options, query, cx),
            _ => editor_page(&options, cx),
        })
});
```

## What it does not do

It is the chrome, and only the chrome. There is no schema type, no `Setting`
trait, no value marshalling — the view does not know what a setting is, cannot
read one, and will not write one.

That is a deliberate line. Every app types its settings against its own config
struct (`fn(&Options) -> bool` and friends), and a component generic enough to
hold those would push the cost back onto the caller as type parameters or
stringly-typed values — leaving their code worse than the two hundred lines it
replaced. The schema is also the part worth owning: it *is* the product surface.

What was worth sharing is the part three apps had each written separately, and
which had already drifted apart: one marked an overridden key with a reset
arrow, another with a dot.

## The three pieces

| Component | What it is |
| --- | --- |
| `SettingsView` | The shell: page list, content pane, optional search and footer. A stateful entity. |
| `SettingsSection` | A titled group of rows inside a page. A `ParentElement` builder. |
| `SettingsRow` | One setting: name and description left, control right. A builder. |

### SettingsRow

`SettingsRow` is [`Field`](inputs.md)'s horizontal sibling. `Field` stacks a
label and description *above* an input, which is what a form wants; a settings
list wants them beside it, so the eye runs down one column of names and one
column of controls.

```rust
SettingsRow::new("dark", "Dark mode")
    .description("Pin the scheme instead of following the system.")
    .modified(options.pinned("theme"))
    .on_reset(cx.listener(|this, _, _, cx| this.reset("theme", cx)))
    .control(Switch::new("dark-switch").bind(dark.binding()))
```

`modified` means *the user's file pins this key*, not *this differs from the
default* — the two can agree, and only the first is actionable.

The row shows exactly one marker for it, never two: a reset control when you
offer `on_reset`, a dot when you don't. Both would say the same thing twice.

| Method | Notes |
| --- | --- |
| `description(..)` | One sentence under the label. |
| `modified(bool)` | Show the marker. |
| `on_reset(handler)` | Offer to restore the default. Shown only while `modified` — a reset button on an untouched setting does nothing. |
| `control(..)` | Any element: a `Switch`, a `Select`, a row of `Button`s. |
| `divider(bool)` | The hairline under the row (default on). Turn it off for the last row in a section. |

### SettingsSection

A page is rarely one flat list. Appearance wants Theme separated from
Typography; Security wants the settings worth reading twice under their own
heading.

```rust
SettingsSection::new("Theme")
    .description("How the app looks.")
    .child(row_one)
    .child(row_two)
```

It's a plain `ParentElement`, so a section holding a chart or a table is just as
valid as one holding rows.

### SettingsView

```rust
SettingsView::new(cx)
    .page("appearance", "Appearance")     // or page_icon(..) with a Lucide icon
    .searchable(true)
    .active("editor")                     // open on a page; unknown ids are ignored
    .sidebar_width(190.0)
    .sidebar_matches_body(true)            // use the content background in the sidebar
    .content(|page, query, window, cx| { … })
    .footer(|window, cx| Button::new("done", "Done"))
```

`content` is re-invoked every frame with the active page's id and the current
query — the same contract as `Tabs` and `Accordion` — so rows show live values
rather than a snapshot taken when the view was built.

**Search is the host's.** The view has nothing to search, so it reports the
query through `SettingsViewEvent::Search` and hands it to the content closure.
Matching is yours, because only you know what the settings are.

```rust
cx.subscribe(&settings, |this, _view, event: &SettingsViewEvent, cx| match event {
    SettingsViewEvent::PageChanged(id) => this.remember_page(id, cx),
    SettingsViewEvent::Search(query) => this.note_query(query, cx),
})
.detach();
```

| Method | Notes |
| --- | --- |
| `active_page()` | The selected page's id, or `None` when no pages were added. |
| `set_page(id, cx)` | Select by id. An unknown id is ignored rather than panicking — a stale id from a restored session is not a crash. |
| `query()` | The current search text. |
| `clear_search(cx)` | Empty the field and report it. |
| `sidebar_matches_body(bool)` | Use the content background for the page list instead of the raised surface. |

## A whole page

```rust
fn appearance_page(options: &Options, query: &str, cx: &mut App) -> AnyElement {
    let mut section = SettingsSection::new("Theme").description("How the app looks.");

    for setting in schema::in_section(Section::Appearance) {
        if !setting.matches(query) {
            continue;
        }
        section = section.child(
            SettingsRow::new(setting.key, setting.label)
                .description(setting.desc)
                .modified(options.pinned(setting.key))
                .control(control_for(setting, options, cx)),
        );
    }

    section.into_any_element()
}
```

The schema, `matches`, and `control_for` are yours. Everything the rows and the
shell draw is not.
