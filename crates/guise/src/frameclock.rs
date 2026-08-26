use std::cell::RefCell;
use std::collections::HashSet;
use std::time::Duration;

use gpui::{App, EntityId, Window};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum FrameKind {
    Continuous,
    Caret,
}

thread_local! {
    static PENDING: RefCell<HashSet<(EntityId, FrameKind)>> = RefCell::new(HashSet::new());
}

/// Ask for one future render of the current view, coalescing requests from
/// every animation of the same kind in that view. Stateless animated elements
/// may be repainted for unrelated reasons while their timer is pending; without
/// this guard each repaint would start another timer and multiply the frame rate.
pub(crate) fn request_frame(kind: FrameKind, after: Duration, window: &mut Window, cx: &mut App) {
    let view = window.current_view();
    let key = (view, kind);
    if !PENDING.with(|pending| pending.borrow_mut().insert(key)) {
        return;
    }

    window
        .spawn(cx, async move |cx| {
            cx.background_executor().timer(after).await;
            PENDING.with(|pending| pending.borrow_mut().remove(&key));
            cx.update(move |_, cx| cx.notify(view)).ok();
        })
        .detach();
}
