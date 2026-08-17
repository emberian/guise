# AI components

Everything a model-facing app needs on screen: a transcript, a prompt box,
streaming feedback, tool calls, citations, and the controls and meters around a
request.

None of it opens a socket. guise is gpui and std, and a component library is
the wrong place to keep someone's API key — so the host owns the request and
these own what the user sees while it happens. That split is also what makes
them portable: the same `AIChatView` drives a local model, a hosted API, or a
replayed transcript, because all it ever receives is text.

```rust
use guise::ai::*;
```

## The short version

```rust
let chat = cx.new(|cx| AIChatView::new(cx).max_width(760.0));
let composer = cx.new(|cx| AIComposer::new(cx).hint("Shift+Enter for a new line"));

cx.subscribe(&composer, move |this, _composer, event: &AIComposerEvent, cx| {
    if let AIComposerEvent::Submit(text) = event {
        this.chat.update(cx, |chat, cx| {
            chat.push(AITurn::user(text.clone()), cx);
            chat.begin_reply(cx);
        });
        this.send(text.clone(), cx); // your transport
    }
})
.detach();
```

Then, as tokens arrive:

```rust
chat.update(cx, |chat, cx| chat.push_delta(&token, cx));
```

and when the reply ends, `chat.end_reply(cx)` — or `chat.fail_reply(error, cx)`
if it didn't, which keeps whatever text had already arrived and shows the error
under it.

## AIChatView (entity)

The transcript. It owns the conversation so a host doesn't have to re-derive
one every frame, and it owns the scroll position and the per-turn disclosure
state along with it.

| Method | Notes |
| --- | --- |
| `new(cx)` | |
| `turns(iter)` | seed it — restoring a saved conversation |
| `max_width(f32)` | cap the reading width and center it |
| `empty_message(impl Into<SharedString>)` | shown before anything is said |
| `size(Size)` | |
| `push(AITurn, cx) -> usize` | append a turn, returning its index |
| `begin_reply(cx) -> usize` | open an empty assistant turn to stream into |
| `push_delta(&str, cx)` | append to the open turn |
| `push_reasoning(&str, cx)` | append to the open turn's reasoning |
| `end_reply(cx)` / `fail_reply(error, cx)` | close it |
| `set_pending(Some("Searching…"), cx)` | a "working on it" line under the transcript |
| `update_turn(i, edit, cx)` | edit in place — attaching a tool result |
| `clear(cx)` / `scroll_to_bottom(cx)` | |
| `all()` / `turn(i)` / `turn_count()` | read the transcript |

A delta that arrives with no turn open is dropped rather than resurrecting a
finished one, so a cancelled request whose last chunk was already in flight
can't reopen it.

**Stick-to-bottom.** A transcript that auto-scrolls unconditionally rips the
page away from someone reading back through it; one that never scrolls leaves
the newest text off screen. So it follows the tail only while the view is
already at the tail. Scrolling up detaches it; scrolling back within a couple
of lines of the end re-attaches, and so does sending — you just acted, so you
want to see the result.

It emits `AIChatViewEvent::OpenSource(turn, source)` when a citation or a
source row is clicked.

### AITurn

```rust
AITurn::user("What changed in 1.5?")
AITurn::assistant(reply).name("claude-opus-5").meta("2.1s · 412 tokens")
AITurn::system("You are a careful engineer.")
```

Fields: `role`, `body`, `reasoning` / `reasoning_open`, `tools`, `sources`,
`streaming`, `error`, `name`, `meta`.

## AIMessage

One turn, if you'd rather lay the list out yourself. The user's turn is a
contained bubble, the assistant's runs full width, system and tool turns are
quiet marginalia. The body is markdown; anything else — a tool card, a diff, a
rating widget — arrives as a child.

```rust
AIMessage::new(AIRole::Assistant, reply)
    .streaming(true)
    .meta("1.4s")
    .child(AIToolCall::new(("tool", 0), "read_file").status(AIToolStatus::Ok))
```

`AIRole` is `User`, `Assistant`, `System`, or `Tool`.

## AIComposer (entity)

The prompt box. Enter sends and Shift+Enter breaks the line, the box grows with
what's in it up to a ceiling, and the send button becomes a stop button while a
reply is in flight — being able to interrupt a long generation is the control
people reach for most.

```rust
let composer = cx.new(|cx| {
    AIComposer::new(cx)
        .attachments(true)
        .hint("Claude can make mistakes. Check important info.")
});
composer.update(cx, |c, cx| c.set_busy(true, cx)); // send → stop
```

Emits `AIComposerEvent::{Submit(String), Stop, Attach, Change(String)}`. It
clears itself on submit and refuses a blank draft — sending whitespace to a
model is never what was meant.

## AIStreamingText and AIThinking

`AIStreamingText` renders exactly what `Markdown` renders and puts a blinking
block on the end, the way a terminal shows a process still writing. It takes
the whole text every frame rather than a delta, because that is what a render
pass has.

`AIThinking` covers the gap before the first token, which can run to several
seconds and reads as a hang if nothing moves. Give it a specific label —
"Searching the web" is worth far more than "Thinking".

```rust
AIThinking::new().label("Running tests")
```

## AIReasoning

Extended thinking, folded away — it is usually longer than the answer and not
what the reader came for. While it is still streaming the header says so, which
is the one case where someone wants to know before they open it.

Open state belongs to whatever owns the transcript, so it is controlled:

```rust
AIReasoning::new(("reasoning", i), text)
    .open(turn.reasoning_open)
    .streaming(turn.streaming)
    .on_toggle(cx.listener(|this, _, _, cx| this.toggle_reasoning(i, cx)))
```

## AIToolCall

What the model did and whether it worked. Name and status always visible,
arguments and result folded away until asked. Status is the load-bearing part:
a stalled tool is the most common way an assistant appears broken.

```rust
AIToolCall::new(("tool", i), "read_file")
    .status(AIToolStatus::Running)
    .arguments(r#"{"path": "src/main.rs"}"#)
    .result(preview)
    .meta("120ms")
    .open(expanded)
    .on_toggle(...)
```

`AIToolStatus` is `Pending`, `Running`, `Ok`, or `Error`. An errored card is
outlined in the danger color and labels its result "Error".

The fold affordance appears when `arguments` or `result` is given. Pass
`expandable(true)` to offer it anyway — `AIChatView` does, so it can withhold a
tens-of-kilobytes result until the card is actually open.

## AICitation and AISources

A citation is only useful if it is reachable, so `AICitation` is a click
target, not a decoration, and `AISources` is the numbered list it points at.

```rust
AICitation::new(("cite", i), 1).label("docs.rs").on_click(...)

AISources::new(sources)
    .excerpts(true)
    .on_open(|index, _window, cx| { /* open it */ })
```

`AISource::new(title, location).excerpt(passage)` — the excerpt matters:
"it says so on this page" and "it says so in this sentence" are different
claims.

## AIModelPicker (entity)

A model is more than a name — which one is selected changes what a request
costs and how much context it has, and both matter at the moment of choosing.
So each row carries its description and context size, and the picker hands back
the whole `AIModel`.

```rust
let picker = cx.new(|cx| {
    AIModelPicker::new(cx)
        .models([
            AIModel::new("claude-opus-5", "Opus 5")
                .description("Deepest reasoning")
                .context(200_000)
                .pricing(AIPricing::new(15.0, 75.0)),
            AIModel::new("claude-sonnet-5", "Sonnet 5")
                .description("Balanced")
                .context(200_000)
                .pricing(AIPricing::new(3.0, 15.0)),
        ])
        .selected_id("claude-sonnet-5")
});
```

Emits `AIModelPickerEvent(AIModel)`. `set_models` keeps the selection on the
same id when it survives the swap.

## AITokenMeter, AICost, AISettings

`AITokenMeter::new(used, limit)` turns context exhaustion — the failure people
hit without warning — into something visible a few turns early: amber at 75%,
red at 90%.

`AICost::new(usage, pricing)` keeps a running total. The arithmetic lives here
because getting it wrong by a factor of a thousand is easy — prices are quoted
per *million* tokens:

```rust
let usage = AIUsage::new(1_200, 800).cache_read(40_000);
let pricing = AIPricing::new(3.0, 15.0).cache_read(0.30);
AICost::new(usage, pricing).breakdown(true)
```

`AIUsage` adds with `+` and sums with `.sum()`, saturating rather than wrapping.

`AISettings` is the two knobs every provider takes: temperature as a slider,
because the useful range is narrow and the exact value rarely matters, and max
tokens as a number field, because it does. Both clamp before they emit, so the
event can go straight into a request. `ceiling(tokens, cx)` follows the
selected model and pulls the value down with it, so the pair can never describe
an impossible request.

## Markdown

Message bodies are rendered by `Markdown`, a read-only `RenderOnce` builder
that works anywhere:

```rust
div().child(Markdown::new("# Notes\n\n- **bold** and `code`"))
```

It shares the three pure passes `MarkdownEditor` uses — headings, lists, task
boxes, quotes, fenced code, rules, links, emphasis — without the caret, scroll
model, or hand-rolled glyph layout an editor needs.
