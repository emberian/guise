//! Entity-level tests on gpui's test harness (`#[gpui::test]` +
//! `TestAppContext` — the same rig zed's own tests use). These cover the
//! wiring pure unit tests can't reach: signals, bindings, form observers,
//! entity events, and the theme global.
//!
//! Observer effects flush between `cx.update` blocks, so assertions that
//! depend on an observer firing sit in their own block.

use gpui::prelude::*;
use gpui::{div, Context, Entity, Modifiers, MouseButton, TestAppContext, Window};

use crate::ai::{AIChatView, AIComposer, AIComposerEvent, AITurn};
use crate::input::{Date, DatePicker, LineEditor as _, Select, TextInput};
use crate::reactive::{validators, Form, Signal};
use crate::theme::{theme, Color, Theme};
use crate::update::{
    is_installing, Release, UpdateNotice, UpdateNoticeEvent, UpdateOutcome, UpdatePrompt,
    UpdatePromptEvent, UpdateStage, Updater,
};
use crate::{Carousel, CarouselEvent};

#[gpui::test]
fn signal_binding_and_lens_round_trip(cx: &mut TestAppContext) {
    let count = cx.update(|cx| Signal::new(cx, 5_i32));
    let binding = count.binding();
    cx.update(|cx| {
        assert_eq!(binding.get(cx), 5);
        binding.set(cx, 9);
        assert_eq!(count.get(cx), 9);
    });

    #[derive(Clone, PartialEq)]
    struct Settings {
        muted: bool,
    }
    let settings = cx.update(|cx| Signal::new(cx, Settings { muted: false }));
    let muted = settings.lens(|s| s.muted, |s, v| s.muted = v);
    cx.update(|cx| {
        muted.set(cx, true);
        assert!(settings.read(cx).muted);
        // Mapped bindings convert both ways.
        let as_text = muted.map(|b| b.to_string(), |s: String| s == "true");
        assert_eq!(as_text.get(cx), "true");
        as_text.set(cx, "false".to_string());
        assert!(!settings.read(cx).muted);
    });
}

#[gpui::test]
fn form_validates_and_revalidates_live(cx: &mut TestAppContext) {
    let form = cx.update(|cx| {
        Form::new(cx)
            .field(cx, "email", "")
            .rule("email", validators::required())
            .rule("email", validators::email())
            .field(cx, "confirm", "")
            .rule_form("confirm", validators::equals_field("email", "Must match"))
    });

    cx.update(|cx| {
        assert!(!form.validate(cx));
        assert!(form.error(cx, "email").is_some());
        assert!(!form.is_valid(cx));
    });

    // Fixing the field re-validates it live (it carried an error) — the
    // observer fires on the effect flush between these blocks.
    cx.update(|cx| form.set(cx, "email", "a@b.com"));
    cx.update(|cx| {
        assert_eq!(form.error(cx, "email"), None);
        assert!(form.touched("email"));
    });

    cx.update(|cx| form.set(cx, "confirm", "a@b.com"));
    cx.update(|cx| {
        let values = form.submit(cx).expect("form should validate");
        assert_eq!(values["email"], "a@b.com");
    });

    // Cross-field: change email, confirm no longer matches.
    cx.update(|cx| form.set(cx, "email", "other@b.com"));
    cx.update(|cx| assert!(!form.validate(cx)));
}

#[gpui::test]
fn select_bind_follows_the_signal_both_ways(cx: &mut TestAppContext) {
    let choice = cx.update(|cx| Signal::new(cx, 2_usize));
    let select = cx.update(|cx| cx.new(|cx| Select::new(cx).data(["a", "b", "c"])));
    cx.update(|cx| Select::bind(&select, &choice, cx));

    // The signal is the source of truth: the picker adopts it immediately…
    cx.update(|cx| assert_eq!(select.read(cx).selected_index(), Some(2)));

    // …and follows later writes.
    cx.update(|cx| choice.set(cx, 0));
    cx.update(|cx| assert_eq!(select.read(cx).selected_index(), Some(0)));
}

#[gpui::test]
fn datepicker_bind_adopts_signal_writes(cx: &mut TestAppContext) {
    let date = Date::new(2026, 7, 14).unwrap();
    let picked = cx.update(|cx| Signal::new(cx, None::<Date>));
    let picker = cx.update(|cx| cx.new(DatePicker::new));
    cx.update(|cx| DatePicker::bind(&picker, &picked, cx));

    cx.update(|cx| assert_eq!(picker.read(cx).selected_date(), None));
    cx.update(|cx| picked.set(cx, Some(date)));
    cx.update(|cx| assert_eq!(picker.read(cx).selected_date(), Some(date)));
}

#[gpui::test]
fn carousel_navigates_and_emits(cx: &mut TestAppContext) {
    use std::cell::RefCell;
    use std::rc::Rc;

    let deck = cx.update(|cx| {
        cx.new(|cx| {
            Carousel::new(cx)
                .slide(|_, _| gpui::Empty)
                .slide(|_, _| gpui::Empty)
                .slide(|_, _| gpui::Empty)
        })
    });
    let seen: Rc<RefCell<Vec<usize>>> = Rc::default();
    let log = seen.clone();
    cx.update(|cx| {
        cx.subscribe(&deck, move |_deck, event: &CarouselEvent, _cx| {
            log.borrow_mut().push(event.0);
        })
        .detach();
    });

    deck.update(cx, |deck, cx| {
        deck.next(cx);
        deck.next(cx);
        deck.next(cx); // wraps to 0
        deck.prev(cx); // wraps back to 2
        deck.go_to(1, cx);
        deck.go_to(1, cx); // no-op, no event
    });
    assert_eq!(*seen.borrow(), vec![1, 2, 0, 2, 1]);
    cx.update(|cx| assert_eq!(deck.read(cx).current(), 1));
}

/// The name of each event, so a subscription can be asserted on without
/// `UpdatePromptEvent` having to be `PartialEq`.
fn prompt_event_name(event: &UpdatePromptEvent) -> &'static str {
    match event {
        UpdatePromptEvent::Started => "started",
        UpdatePromptEvent::Stage(_) => "stage",
        UpdatePromptEvent::Installed(_) => "installed",
        UpdatePromptEvent::Failed(_) => "failed",
        UpdatePromptEvent::Dismissed => "dismissed",
    }
}

fn offered_release() -> Release {
    Release {
        version: "2.0.0".to_string(),
        url: "https://example.com/releases/2.0.0".to_string(),
        assets: Vec::new(),
    }
}

/// A test binary is neither an installed `.app` nor an AppImage, so `detect()`
/// reports `Unknown` — the case where the prompt must not promise an install it
/// can't perform. Accepting has to open the release page and stand down instead
/// of starting one.
#[gpui::test]
fn update_prompt_offers_the_page_when_it_cannot_install_in_place(cx: &mut TestAppContext) {
    use std::cell::RefCell;
    use std::rc::Rc;

    let updater = Updater::github("Acme", "1.0.0", "acme/acme");
    let prompt = cx.update(|cx| cx.new(|cx| UpdatePrompt::new(updater, offered_release(), cx)));
    let seen: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    let log = seen.clone();
    cx.update(|cx| {
        cx.subscribe(&prompt, move |_prompt, event: &UpdatePromptEvent, _cx| {
            log.borrow_mut().push(prompt_event_name(event));
        })
        .detach();
    });

    prompt.update(cx, |prompt, cx| prompt.accept(cx));
    assert_eq!(*seen.borrow(), vec!["dismissed"]);
    cx.update(|cx| {
        assert!(!prompt.read(cx).busy());
        // Nothing was started, so no other prompt is locked out.
        assert!(!is_installing(cx));
    });
}

/// The states a host driving its own install moves the prompt through, and the
/// guarantee that a running install can't be dismissed out from under itself.
#[gpui::test]
fn update_prompt_tracks_the_stages_a_host_drives(cx: &mut TestAppContext) {
    use std::cell::RefCell;
    use std::rc::Rc;

    let updater = Updater::github("Acme", "1.0.0", "acme/acme");
    let prompt = cx.update(|cx| cx.new(|cx| UpdatePrompt::new(updater, offered_release(), cx)));
    let seen: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    let log = seen.clone();
    cx.update(|cx| {
        cx.subscribe(&prompt, move |_prompt, event: &UpdatePromptEvent, _cx| {
            log.borrow_mut().push(prompt_event_name(event));
        })
        .detach();
    });

    prompt.update(cx, |prompt, cx| {
        prompt.set_stage(UpdateStage::Preparing, cx);
        // Inert while installing: the progress must not be closable.
        prompt.dismiss(cx);
    });
    cx.update(|cx| {
        let prompt = prompt.read(cx);
        assert!(prompt.busy());
        assert_eq!(prompt.stage(), Some(&UpdateStage::Preparing));
        assert_eq!(prompt.error(), None);
    });
    assert!(seen.borrow().is_empty());

    prompt.update(cx, |prompt, cx| prompt.set_failed("no disk space", cx));
    cx.update(|cx| {
        let prompt = prompt.read(cx);
        assert!(!prompt.busy());
        assert_eq!(prompt.error(), Some("no disk space"));
    });

    // A failure is retryable, and dismissable again.
    prompt.update(cx, |prompt, cx| {
        prompt.reset(cx);
        prompt.dismiss(cx);
    });
    cx.update(|cx| assert_eq!(prompt.read(cx).error(), None));
    assert_eq!(*seen.borrow(), vec!["dismissed"]);
}

#[gpui::test]
fn update_notice_answers_a_check_and_dismisses(cx: &mut TestAppContext) {
    use std::cell::RefCell;
    use std::rc::Rc;

    let updater = Updater::github("Acme", "1.31.0", "acme/acme");
    let notice = cx.update(|cx| {
        cx.new(|cx| UpdateNotice::new(updater, UpdateOutcome::Pending("1.32.0".into()), cx))
    });
    let dismissed: Rc<RefCell<usize>> = Rc::default();
    let count = dismissed.clone();
    cx.update(|cx| {
        cx.subscribe(&notice, move |_notice, event: &UpdateNoticeEvent, _cx| {
            let UpdateNoticeEvent::Dismissed = event;
            *count.borrow_mut() += 1;
        })
        .detach();
    });

    cx.update(|cx| {
        assert_eq!(
            notice.read(cx).outcome(),
            &UpdateOutcome::Pending("1.32.0".to_string())
        );
    });
    notice.update(cx, |notice, cx| notice.dismiss(cx));
    assert_eq!(*dismissed.borrow(), 1);
}

#[gpui::test]
fn theme_presets_install_and_resolve(cx: &mut TestAppContext) {
    cx.update(|cx| {
        Theme::catppuccin().init(cx);
        let t = theme(cx);
        assert!(t.scheme.is_dark());
        assert_eq!(t.primary(), Color::hex("#89b4fa"));
        assert_eq!(t.body(), Color::hex("#1e1e2e"));

        // Swapping the global restyles everything that reads theme(cx).
        Theme::solarized_light().init(cx);
        let t = theme(cx);
        assert!(!t.scheme.is_dark());
        assert_eq!(t.primary(), Color::hex("#268bd2"));
    });
}

// --- single-line fields -----------------------------------------------------
//
// These drive a real window: `simulate_input` dispatches the key event first
// and only then hands the character to the platform's input handler, exactly
// as macOS, X11, and Windows do. That is the path a text field now takes, so
// nothing here works unless the field is wired up the way the real thing is.

/// Two fields in a window — the smallest form that can show focus moving.
struct Pair {
    first: Entity<TextInput>,
    second: Entity<TextInput>,
}

impl Render for Pair {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(self.first.clone())
            .child(self.second.clone())
    }
}

fn pair(cx: &mut TestAppContext) -> (Entity<Pair>, &mut gpui::VisualTestContext) {
    cx.update(|cx| Theme::light().init(cx));
    cx.add_window_view(|_window, cx| Pair {
        first: cx.new(|cx| TextInput::new(cx).placeholder("first")),
        second: cx.new(|cx| TextInput::new(cx).placeholder("second")),
    })
}

/// Focus a field and let the window lay out again, so the tab-stop ring and
/// the field's shaped line are both current.
fn focus(field: &Entity<TextInput>, cx: &mut gpui::VisualTestContext) {
    let handle = field.read_with(cx, |field, _| field.focus_handle());
    cx.update(|window, _| window.focus(&handle));
    cx.run_until_parked();
}

#[gpui::test]
fn text_input_types_through_the_platform_input_handler(cx: &mut TestAppContext) {
    let (view, cx) = pair(cx);
    let field = view.read_with(cx, |view, _| view.first.clone());
    focus(&field, cx);

    cx.simulate_input("hello");
    assert_eq!(field.read_with(cx, |field, _| field.text()), "hello");

    // Backspace and the arrows still come from key handling.
    cx.simulate_keystrokes("backspace left left");
    cx.simulate_input("L");
    assert_eq!(field.read_with(cx, |field, _| field.text()), "heLll");
}

#[gpui::test]
fn text_input_tab_moves_focus_instead_of_typing(cx: &mut TestAppContext) {
    let (view, cx) = pair(cx);
    let (first, second) = view.read_with(cx, |view, _| (view.first.clone(), view.second.clone()));
    focus(&first, cx);
    cx.simulate_input("one");

    cx.simulate_keystrokes("tab");
    // The platform reports a `\t` for Tab. A field that typed it would be the
    // bug this replaced.
    assert_eq!(first.read_with(cx, |field, _| field.text()), "one");
    let second_handle = second.read_with(cx, |field, _| field.focus_handle());
    assert!(cx.update(|window, _| second_handle.is_focused(window)));

    cx.simulate_input("two");
    assert_eq!(second.read_with(cx, |field, _| field.text()), "two");

    cx.simulate_keystrokes("shift-tab");
    let first_handle = first.read_with(cx, |field, _| field.focus_handle());
    assert!(cx.update(|window, _| first_handle.is_focused(window)));
    assert_eq!(second.read_with(cx, |field, _| field.text()), "two");
}

#[gpui::test]
fn text_input_cuts_copies_and_pastes(cx: &mut TestAppContext) {
    let (view, cx) = pair(cx);
    let (first, second) = view.read_with(cx, |view, _| (view.first.clone(), view.second.clone()));
    focus(&first, cx);
    cx.simulate_input("copy me");

    cx.simulate_keystrokes("cmd-a cmd-c");
    focus(&second, cx);
    cx.simulate_keystrokes("cmd-v cmd-v");
    assert_eq!(
        second.read_with(cx, |field, _| field.text()),
        "copy mecopy me"
    );

    // Cut empties the source and leaves the clipboard usable again.
    focus(&first, cx);
    cx.simulate_keystrokes("cmd-a cmd-x");
    assert_eq!(first.read_with(cx, |field, _| field.text()), "");
    cx.simulate_keystrokes("cmd-v");
    assert_eq!(first.read_with(cx, |field, _| field.text()), "copy me");
}

#[gpui::test]
fn text_input_pasting_flattens_line_breaks(cx: &mut TestAppContext) {
    let (view, cx) = pair(cx);
    let field = view.read_with(cx, |view, _| view.first.clone());
    focus(&field, cx);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("one\ntwo\r\nthree".into()));
    cx.simulate_keystrokes("cmd-v");
    assert_eq!(
        field.read_with(cx, |field, _| field.text()),
        "one two  three"
    );
}

#[gpui::test]
fn text_input_undoes_by_word(cx: &mut TestAppContext) {
    let (view, cx) = pair(cx);
    let field = view.read_with(cx, |view, _| view.first.clone());
    focus(&field, cx);
    cx.simulate_input("alpha beta");

    cx.simulate_keystrokes("cmd-z");
    assert_eq!(field.read_with(cx, |field, _| field.text()), "alpha ");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(field.read_with(cx, |field, _| field.text()), "");
    cx.simulate_keystrokes("cmd-shift-z cmd-shift-z");
    assert_eq!(field.read_with(cx, |field, _| field.text()), "alpha beta");
}

#[gpui::test]
fn text_input_click_places_the_caret_and_drag_selects(cx: &mut TestAppContext) {
    let (view, cx) = pair(cx);
    let field = view.read_with(cx, |view, _| view.first.clone());
    focus(&field, cx);
    cx.simulate_input("hello world");

    // Ask the shaped line itself where a character sits, so the test doesn't
    // depend on the font's metrics.
    let (bounds, x_of) = field.read_with(cx, |field, _| {
        let state = field.line();
        let shaped = state.shaped.clone().expect("the field has been painted");
        let bounds = state.bounds.expect("the field has been painted");
        (bounds, move |index: usize| shaped.x_for_index(index))
    });
    let at = |index: usize| gpui::point(bounds.left() + x_of(index), bounds.center().y);

    cx.simulate_click(at(2), Modifiers::none());
    assert_eq!(field.read_with(cx, |field, _| field.edit().cursor()), 2);

    // Press at 2, drag to 7, release: "llo w" is selected.
    cx.simulate_mouse_down(at(2), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(at(7), MouseButton::Left, Modifiers::none());
    assert_eq!(
        field.read_with(cx, |field, _| field.edit().selected_text()),
        Some("llo w".to_string())
    );
    cx.simulate_mouse_up(at(7), MouseButton::Left, Modifiers::none());

    // Typing replaces the selection, as it would in a browser.
    cx.simulate_input("X");
    assert_eq!(field.read_with(cx, |field, _| field.text()), "heXorld");
}

#[gpui::test]
fn text_input_double_click_takes_a_word(cx: &mut TestAppContext) {
    let (view, cx) = pair(cx);
    let field = view.read_with(cx, |view, _| view.first.clone());
    focus(&field, cx);
    cx.simulate_input("hello world");

    let (bounds, x_of) = field.read_with(cx, |field, _| {
        let state = field.line();
        let shaped = state.shaped.clone().expect("the field has been painted");
        let bounds = state.bounds.expect("the field has been painted");
        (bounds, move |index: usize| shaped.x_for_index(index))
    });
    let position = gpui::point(bounds.left() + x_of(8), bounds.center().y);

    cx.simulate_event(gpui::MouseDownEvent {
        button: MouseButton::Left,
        position,
        modifiers: Modifiers::none(),
        click_count: 2,
        first_mouse: false,
    });
    assert_eq!(
        field.read_with(cx, |field, _| field.edit().selected_text()),
        Some("world".to_string())
    );

    cx.simulate_event(gpui::MouseDownEvent {
        button: MouseButton::Left,
        position,
        modifiers: Modifiers::none(),
        click_count: 3,
        first_mouse: false,
    });
    assert_eq!(
        field.read_with(cx, |field, _| field.edit().selected_text()),
        Some("hello world".to_string())
    );
}

#[gpui::test]
fn text_input_honours_max_length(cx: &mut TestAppContext) {
    cx.update(|cx| Theme::light().init(cx));
    let (field, cx) = cx.add_window_view(|_window, cx| TextInput::new(cx).max_length(4));
    focus(&field, cx);
    cx.simulate_input("abcdefg");
    assert_eq!(field.read_with(cx, |field, _| field.text()), "abcd");
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("xyz".into()));
    cx.simulate_keystrokes("cmd-v");
    assert_eq!(field.read_with(cx, |field, _| field.text()), "abcd");
}

#[gpui::test]
fn text_input_read_only_selects_but_never_edits(cx: &mut TestAppContext) {
    cx.update(|cx| Theme::light().init(cx));
    let (field, cx) =
        cx.add_window_view(|_window, cx| TextInput::new(cx).read_only(true).value("locked"));
    focus(&field, cx);
    cx.simulate_input("nope");
    cx.simulate_keystrokes("backspace");
    assert_eq!(field.read_with(cx, |field, _| field.text()), "locked");
    // Read-only still selects and copies.
    cx.simulate_keystrokes("cmd-a cmd-c");
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("locked".to_string())
    );
}

#[gpui::test]
fn text_input_never_copies_a_password(cx: &mut TestAppContext) {
    cx.update(|cx| Theme::light().init(cx));
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("untouched".into()));
    let (field, cx) = cx.add_window_view(|_window, cx| TextInput::new(cx).password(true));
    focus(&field, cx);
    cx.simulate_input("hunter2");
    cx.simulate_keystrokes("cmd-a cmd-c");
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("untouched".to_string())
    );
    cx.simulate_keystrokes("cmd-x");
    assert_eq!(field.read_with(cx, |field, _| field.text()), "hunter2");
}

// --- AI components ----------------------------------------------------------

#[gpui::test]
fn chat_view_streams_a_reply_and_closes_it(cx: &mut TestAppContext) {
    cx.update(|cx| Theme::light().init(cx));
    let (chat, cx) = cx.add_window_view(|_window, cx| AIChatView::new(cx));

    chat.update(cx, |chat, cx| {
        chat.push(AITurn::user("hello"), cx);
        chat.begin_reply(cx);
        chat.push_delta("Hi", cx);
        chat.push_delta(" there", cx);
        chat.push_reasoning("weighing it up", cx);
    });
    chat.read_with(cx, |chat, _| {
        assert_eq!(chat.turn_count(), 2);
        let reply = chat.turn(1).unwrap();
        assert_eq!(reply.body, "Hi there");
        assert_eq!(reply.reasoning.as_deref(), Some("weighing it up"));
        assert!(reply.streaming);
    });

    chat.update(cx, |chat, cx| chat.end_reply(cx));
    chat.read_with(cx, |chat, _| assert!(!chat.turn(1).unwrap().streaming));

    // A delta that lands after the turn closed — a cancelled request whose
    // last chunk was already in flight — must not reopen it.
    chat.update(cx, |chat, cx| chat.push_delta(" and more", cx));
    chat.read_with(cx, |chat, _| {
        assert_eq!(chat.turn(1).unwrap().body, "Hi there")
    });
}

#[gpui::test]
fn chat_view_keeps_partial_text_when_a_reply_fails(cx: &mut TestAppContext) {
    cx.update(|cx| Theme::light().init(cx));
    let (chat, cx) = cx.add_window_view(|_window, cx| AIChatView::new(cx));

    chat.update(cx, |chat, cx| {
        chat.begin_reply(cx);
        chat.push_delta("partial", cx);
        chat.fail_reply("connection reset", cx);
    });
    chat.read_with(cx, |chat, _| {
        let reply = chat.turn(0).unwrap();
        assert_eq!(reply.body, "partial");
        assert_eq!(reply.error.as_deref(), Some("connection reset"));
        assert!(!reply.streaming);
    });

    // Failing with nothing open is a no-op, not a panic.
    chat.update(cx, |chat, cx| chat.fail_reply("late", cx));
    chat.read_with(cx, |chat, _| {
        assert_eq!(
            chat.turn(0).unwrap().error.as_deref(),
            Some("connection reset")
        );
    });
}

#[gpui::test]
fn chat_view_edits_are_bounds_checked(cx: &mut TestAppContext) {
    cx.update(|cx| Theme::light().init(cx));
    let (chat, cx) = cx.add_window_view(|_window, cx| AIChatView::new(cx));
    // Editing a turn that isn't there must not panic.
    chat.update(cx, |chat, cx| {
        chat.update_turn(9, |turn| turn.body.push_str("nope"), cx);
        assert!(chat.turn(9).is_none());
    });
}

/// A long transcript must not re-parse every turn's markdown on every frame,
/// and skipping the ones off screen must not move anything that is on it.
#[gpui::test]
fn chat_view_skips_turns_that_are_far_off_screen(cx: &mut TestAppContext) {
    cx.update(|cx| Theme::light().init(cx));
    let body = "## Heading\n\nA paragraph with **bold** and `code` in it.\n\n                1. one\n2. two\n\nAnother paragraph to give the turn some height.";
    let turns: Vec<AITurn> = (0..60)
        .map(|i| {
            if i % 2 == 0 {
                AITurn::user(format!("Question {i}"))
            } else {
                AITurn::assistant(body)
            }
        })
        .collect();

    let (chat, cx) = cx.add_window_view(|_window, cx| AIChatView::new(cx).turns(turns.clone()));
    cx.run_until_parked();

    // The first frame has nothing measured, so everything is built.
    // By the second, only the turns near the viewport are.
    let drawn = chat.update(cx, |chat, _| chat.drawn_count());
    assert!(
        drawn < 60,
        "expected some turns to be skipped, drew {drawn}"
    );
    assert!(drawn > 0, "the visible turns must still be built");

    // Skipping them must leave the scroll extent exactly where it was: the
    // spacers carry each turn's measured height.
    let extent = chat.read_with(cx, |chat, _| chat.scroll_extent());
    cx.run_until_parked();
    let after = chat.read_with(cx, |chat, _| chat.scroll_extent());
    assert_eq!(
        extent, after,
        "content height moved when turns became spacers"
    );

    // A resize reflows every turn, so the measured heights are stale and all
    // of them have to be drawn once more to re-measure — clearing the heights
    // alone wouldn't do it, since an off-screen turn's bounds are its
    // spacer's and would be read straight back.
    cx.simulate_resize(gpui::size(gpui::px(500.0), gpui::px(700.0)));
    cx.run_until_parked();
    assert_eq!(
        chat.update(cx, |chat, _| chat.drawn_count()),
        60,
        "a resize must re-measure every turn"
    );

    // Turning it off builds every turn again.
    let (all, cx) = cx
        .add_window_view(|_window, cx| AIChatView::new(cx).turns(turns.clone()).virtualize(false));
    cx.run_until_parked();
    assert_eq!(all.update(cx, |chat, _| chat.drawn_count()), 60);
}

#[gpui::test]
fn composer_sends_on_enter_and_refuses_blank_drafts(cx: &mut TestAppContext) {
    cx.update(|cx| Theme::light().init(cx));
    let (composer, cx) = cx.add_window_view(|_window, cx| AIComposer::new(cx));

    let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let sink = sent.clone();
    cx.update(|_, cx| {
        cx.subscribe(&composer, move |_composer, event: &AIComposerEvent, _cx| {
            if let AIComposerEvent::Submit(text) = event {
                sink.borrow_mut().push(text.clone());
            }
        })
        .detach();
    });

    let input = composer.read_with(cx, |composer, _| composer.input().clone());
    let handle = input.read_with(cx, |input, _| input.focus_handle());
    cx.update(|window, _| window.focus(&handle));
    cx.run_until_parked();

    // Whitespace is not a prompt.
    cx.simulate_input("   ");
    cx.simulate_keystrokes("enter");
    assert!(sent.borrow().is_empty());

    cx.simulate_input("write a haiku");
    cx.simulate_keystrokes("enter");
    assert_eq!(sent.borrow().as_slice(), ["   write a haiku"]);
    // The box clears itself, so the next prompt starts empty.
    assert_eq!(composer.read_with(cx, |composer, cx| composer.text(cx)), "");

    // Shift+Enter is a newline, not a send.
    cx.simulate_input("one");
    cx.simulate_keystrokes("shift-enter");
    cx.simulate_input("two");
    assert_eq!(sent.borrow().len(), 1);
    assert_eq!(
        composer.read_with(cx, |composer, cx| composer.text(cx)),
        "one\ntwo"
    );

    // While a reply is streaming the composer will not send.
    composer.update(cx, |composer, cx| composer.set_busy(true, cx));
    cx.simulate_keystrokes("enter");
    assert_eq!(sent.borrow().len(), 1);
}
